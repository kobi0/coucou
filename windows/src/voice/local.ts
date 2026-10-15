// The on-device recogniser. Records while the mic button is held, then hands the clip to Rust, where
// the speech model Flow dictation already uses (Parakeet) turns it into words on this computer. The audio is never sent anywhere and never saved.
// It plugs into voice/speech.ts only when Rust says a model is in place (see voice/engine.ts).

import { Bridge } from "../core/bridge";
import { MicCapture, micRefused } from "./capture";
import { concat, hasSpeech, resample, toPcm16 } from "./pcm";
import type { Recognizer, RecognizerResult } from "./speech";

export class LocalRecognizer implements Recognizer {
  private chunks: Float32Array[] = [];
  private mic = new MicCapture();
  private rate = 16_000;
  private done: ((r: RecognizerResult) => void) | null = null;
  private setup: Promise<void> | null = null;
  /** Bumped by every start and cancel, so a late callback from an old recording is ignored. */
  private session = 0;

  start(_locales: string[], _onPartial: (text: string) => void, onFinished: (r: RecognizerResult) => void) {
    // Whisper gives words only once the clip is whole, so there are no partial words to show.
    const session = ++this.session;
    this.chunks = [];
    this.done = onFinished;
    this.setup = this.mic
      .start((c) => this.chunks.push(c))
      .then((rate) => {
        this.rate = rate;
      })
      .catch((err) => {
        if (session !== this.session) return;
        this.mic.stop();
        this.finish(session, { kind: micRefused(err) ? "blocked" : "failed" });
      });
  }

  stop() {
    const session = this.session;
    void (this.setup ?? Promise.resolve()).then(async () => {
      if (session !== this.session || !this.done) return;
      this.mic.stop();
      const samples = resample(concat(this.chunks), this.rate);
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
    this.mic.stop();
  }

  private finish(session: number, result: RecognizerResult) {
    if (session !== this.session) return;
    const done = this.done;
    this.done = null;
    done?.(result);
  }
}

export interface RecognizerStatus {
  state: "ready" | "noModel" | "notBuilt";
  model: string | null;
  folder: string;
}
