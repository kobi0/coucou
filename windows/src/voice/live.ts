// Gemini Live: a spoken conversation, in English or Nigerian Pidgin, with Google's voice. Chosen in Settings.
//
// This is the one voice engine that sends what you say to a server (Google's), so it is only used when you pick
// it. It still obeys the hold-to-talk rule: the microphone is open only while the button is held, and Gemini is
// told when you start and stop speaking. The websocket and the key live in Rust; this file records, plays, and
// shows the words. There are no tools: the model can talk but cannot do anything.

import { Bridge, type LiveEvent } from "../core/bridge";
import { MicCapture, micRefused } from "./capture";
import { concat, resample, toPcm16 } from "./pcm";
import type { Recognizer, RecognizerResult } from "./speech";

const OUTPUT_RATE = 24_000;
/** Matches VoiceSession::MINIMUM_HOLD in coucou-assistant. */
const MINIMUM_HOLD_MS = 400;
/** About a tenth of a second of 16 kHz audio per message. */
const CHUNK_SAMPLES = 1600;

/** Plays the model's sound as it arrives, one piece after another, and can cut it off at once. */
class Playback {
  private context: AudioContext | null = null;
  private next = 0;
  private playing = new Set<AudioBufferSourceNode>();

  /** Must be created from a click or key press, which the mic button is. */
  prepare() {
    this.context ??= new AudioContext();
    void this.context.resume();
  }

  push(base64: string) {
    const context = this.context;
    if (!context) return;
    const bin = atob(base64);
    const samples = new Float32Array(bin.length >> 1);
    for (let i = 0; i < samples.length; i++) {
      const lo = bin.charCodeAt(i * 2);
      const hi = bin.charCodeAt(i * 2 + 1);
      const v = (hi << 8) | lo;
      samples[i] = (v >= 0x8000 ? v - 0x10000 : v) / 32768;
    }
    const buffer = context.createBuffer(1, samples.length, OUTPUT_RATE);
    buffer.copyToChannel(samples, 0);
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);
    const at = Math.max(this.next, context.currentTime + 0.03);
    source.start(at);
    this.next = at + buffer.duration;
    this.playing.add(source);
    source.onended = () => this.playing.delete(source);
  }

  /** Stops everything queued: the person spoke over it. */
  stop() {
    for (const s of this.playing) {
      try {
        s.stop();
      } catch {
        /* already finished */
      }
    }
    this.playing.clear();
    this.next = 0;
  }
}

export class LiveRecognizer implements Recognizer {
  readonly handlesReply = true;
  onText?: (role: "user" | "model", text: string) => void;

  private playback = new Playback();
  private mic = new MicCapture();
  private done: ((r: RecognizerResult) => void) | null = null;
  private ready: Promise<boolean> | null = null;
  private markReady: ((ok: boolean) => void) | null = null;
  private open = false;
  /** Bumped by every start and cancel. */
  private turn = 0;
  /** True from the press until the model has finished answering. */
  private turnActive = false;

  start(_locales: string[], _onPartial: (text: string) => void, onFinished: (r: RecognizerResult) => void) {
    const turn = ++this.turn;
    this.done = onFinished;
    this.turnActive = true;
    this.playback.prepare();
    this.playback.stop(); // pressing to speak interrupts the model
    void this.run(turn);
  }

  private async run(turn: number) {
    const pressedAt = performance.now();
    // The microphone opens at once and what it hears waits in `pending`, so the first words are not lost
    // while the connection is made.
    let pending: Float32Array[] = [];
    let flowing = false;
    let carry: Float32Array<ArrayBufferLike> = new Float32Array(0);
    let rate = 16_000;
    const sendChunks = (samples: Float32Array, flush: boolean) => {
      carry = carry.length ? concat([carry, samples]) : samples;
      const per = Math.round((CHUNK_SAMPLES * rate) / 16_000);
      while (carry.length >= per || (flush && carry.length > 0)) {
        const part = carry.slice(0, per);
        carry = carry.slice(part.length);
        void Bridge.liveAudio(toPcm16(resample(part, rate)));
      }
    };
    try {
      rate = await this.mic.start((chunk) => {
        if (turn !== this.turn) return;
        if (flowing) sendChunks(chunk, false);
        else pending.push(chunk);
      });
    } catch (err) {
      return this.fail(turn, micRefused(err) ? { kind: "blocked" } : { kind: "failed" });
    }
    if (turn !== this.turn) return;

    const ok = await this.ensureOpen();
    if (turn !== this.turn) return;
    if (!ok) return; // ensureOpen has already finished the turn with a line for the person
    // A press shorter than the minimum hold is a slip of the finger and is dropped (the rules are in Rust).
    // Nothing goes to Gemini until it counts as a real hold, so a tap sends no empty turn.
    const wait = pressedAt + MINIMUM_HOLD_MS - performance.now();
    if (wait > 0) await new Promise((r) => setTimeout(r, wait));
    if (turn !== this.turn) return;
    void Bridge.liveActivity(true);
    this.activityOpen = true;
    sendChunks(concat(pending), false);
    pending = [];
    flowing = true;
    this.stopSignal = () => {
      flowing = false;
      this.mic.stop();
      sendChunks(new Float32Array(0), true);
      this.activityOpen = false;
      void Bridge.liveActivity(false);
    };
    if (this.stopRequested) this.stop();
  }

  private activityOpen = false;
  private stopSignal: (() => void) | null = null;
  private stopRequested = false;

  stop() {
    // Hold released. Tell Gemini the person has finished, then report the turn as handled: the words and the
    // reply arrive on their own, through `onText` and the playback.
    if (this.stopSignal) {
      const signal = this.stopSignal;
      this.stopSignal = null;
      signal();
      this.stopRequested = false;
      this.finish({ kind: "transcript", text: "." });
    } else if (this.done) {
      this.stopRequested = true; // released before the connection was ready: finish once it is
    }
  }

  cancel() {
    this.turn++;
    this.done = null;
    this.stopSignal = null;
    this.stopRequested = false;
    this.mic.stop();
    // Only close a turn that was opened: an end signal with no start would be a mistake the service rejects.
    if (this.activityOpen) void Bridge.liveActivity(false);
    this.activityOpen = false;
    this.turnActive = false;
  }

  /** Hangs up: stops sound, closes the connection. */
  hangUp() {
    this.cancel();
    this.playback.stop();
    this.open = false;
    this.ready = null;
    void Bridge.liveClose();
  }

  private ensureOpen(): Promise<boolean> {
    if (this.open) return Promise.resolve(true);
    if (this.ready) return this.ready;
    this.ready = new Promise<boolean>((resolve) => {
      this.markReady = resolve;
    });
    void Bridge.liveOpen((e) => this.onEvent(e)).then((started) => {
      if (!started && this.markReady) this.settle(false, "liveFailed"); // the call itself failed
    });
    return this.ready;
  }

  private settle(ok: boolean, line?: "needsGeminiKey" | "liveFailed") {
    const resolve = this.markReady;
    this.markReady = null;
    if (ok) this.open = true;
    else {
      this.open = false;
      this.ready = null;
    }
    resolve?.(ok);
    if (!ok && line) this.fail(this.turn, { kind: "problem", line });
  }

  private onEvent(e: LiveEvent) {
    switch (e.kind) {
      case "ready":
        this.settle(true);
        break;
      case "audio":
        this.playback.push(e.data);
        break;
      case "text":
        this.onText?.(e.role, e.text);
        break;
      case "interrupted":
        this.playback.stop();
        break;
      case "turnComplete":
        this.turnActive = false;
        break;
      case "error":
        if (this.markReady) {
          this.settle(false, e.code === "no_key" ? "needsGeminiKey" : "liveFailed");
        } else if (this.done || this.turnActive) {
          this.fail(this.turn, { kind: "problem", line: "liveFailed" });
        }
        break;
      case "closed":
        this.open = false;
        this.ready = null;
        break;
    }
  }

  private fail(turn: number, result: RecognizerResult) {
    if (turn !== this.turn) return;
    this.mic.stop();
    this.stopSignal = null;
    this.activityOpen = false;
    this.turnActive = false;
    this.finish(result);
  }

  private finish(result: RecognizerResult) {
    const done = this.done;
    this.done = null;
    done?.(result);
  }
}
