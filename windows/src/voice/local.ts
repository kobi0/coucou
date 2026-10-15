// The on-device recogniser. Records while the mic button is held, then hands the clip to Rust, where
// whisper.cpp turns it into words on this computer. The audio is never sent anywhere and never saved.
// It plugs into voice/speech.ts only when Rust says a model is in place (see refreshRecognizer).

import { Bridge } from "../core/bridge";
import { concat, hasSpeech, resample, toPcm16 } from "./pcm";
import { Hearing, type Recognizer, type RecognizerResult } from "./speech";

class LocalRecognizer implements Recognizer {
  private chunks: Float32Array[] = [];
  private context: AudioContext | null = null;
  private stream: MediaStream | null = null;
  private done: ((r: RecognizerResult) => void) | null = null;
  private setup: Promise<void> | null = null;
  /** Bumped by every start and cancel, so a late callback from an old recording is ignored. */
  private session = 0;

  start(_locales: string[], _onPartial: (text: string) => void, onFinished: (r: RecognizerResult) => void) {
    // Whisper gives words only once the clip is whole, so there are no partial words to show.
    const session = ++this.session;
    this.chunks = [];
    this.done = onFinished;
    this.setup = this.open(session).catch((err) => {
      if (session !== this.session) return;
      this.release();
      const name = (err as DOMException)?.name ?? "";
      const blocked = name === "NotAllowedError" || name === "SecurityError" || name === "NotFoundError";
      this.finish(session, { kind: blocked ? "blocked" : "failed" });
    });
  }

  private async open(session: number) {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    if (session !== this.session) {
      stream.getTracks().forEach((t) => t.stop());
      return;
    }
    this.stream = stream;
    let context: AudioContext;
    try {
      context = new AudioContext({ sampleRate: 16_000 });
    } catch {
      context = new AudioContext(); // some webviews insist on the device rate; resampled on the way out
    }
    this.context = context;
    await context.audioWorklet.addModule("/voice-capture-worklet.js");
    if (session !== this.session) return;
    const node = new AudioWorkletNode(context, "coucou-capture", { numberOfOutputs: 1, outputChannelCount: [1] });
    node.port.onmessage = (e) => this.chunks.push(e.data as Float32Array);
    const silent = context.createGain();
    silent.gain.value = 0; // the graph only runs when it reaches the output; nothing is played
    context.createMediaStreamSource(stream).connect(node);
    node.connect(silent).connect(context.destination);
  }

  stop() {
    const session = this.session;
    void (this.setup ?? Promise.resolve()).then(async () => {
      if (session !== this.session || !this.done) return;
      const rate = this.context?.sampleRate ?? 16_000;
      this.release();
      const samples = resample(concat(this.chunks), rate);
      this.chunks = [];
      if (!hasSpeech(samples)) return this.finish(session, { kind: "nothing" });
      try {
        const text = await Bridge.voiceTranscribe(toPcm16(samples));
        this.finish(session, text ? { kind: "transcript", text } : { kind: "nothing" });
      } catch (err) {
        const code = String(err);
        const gone = code.includes("no_model") || code.includes("not_built");
        this.finish(session, { kind: gone ? "unavailable" : "failed" });
      }
    });
  }

  cancel() {
    this.session++;
    this.done = null;
    this.chunks = [];
    this.release();
  }

  private finish(session: number, result: RecognizerResult) {
    if (session !== this.session) return;
    const done = this.done;
    this.done = null;
    done?.(result);
  }

  private release() {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    void this.context?.close().catch(() => {});
    this.context = null;
  }
}

export interface RecognizerStatus {
  state: "ready" | "noModel" | "notBuilt";
  model: string | null;
  folder: string;
}

/** Asks Rust whether this build can hear and has a model, and plugs the recogniser in (or out) to match. */
export async function refreshRecognizer(): Promise<RecognizerStatus | null> {
  const status = await Bridge.voiceRecognizerStatus();
  if (!status) return null;
  if (status.state === "ready") {
    if (!Hearing.get()) Hearing.set(new LocalRecognizer());
  } else {
    Hearing.get()?.cancel();
    Hearing.set(null);
  }
  return status;
}
