// The microphone, as a stream of samples. Opened when the mic button goes down and closed when it comes up:
// nothing is listening in between. Used by the on-device recogniser (whole clip) and by Gemini Live (chunks).

export class MicCapture {
  private stream: MediaStream | null = null;
  private context: AudioContext | null = null;
  private closed = false;

  /** Starts listening. Resolves with the sample rate the chunks are at. Rejects if the mic is refused. */
  async start(onChunk: (samples: Float32Array) => void): Promise<number> {
    this.closed = false;
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    if (this.closed) {
      stream.getTracks().forEach((t) => t.stop());
      throw new DOMException("stopped before it started", "AbortError");
    }
    this.stream = stream;
    let context: AudioContext;
    try {
      context = new AudioContext({ sampleRate: 16_000 });
    } catch {
      context = new AudioContext(); // some webviews insist on the device rate; callers resample
    }
    this.context = context;
    await context.audioWorklet.addModule("/voice-capture-worklet.js");
    if (this.closed) throw new DOMException("stopped before it started", "AbortError");
    const node = new AudioWorkletNode(context, "coucou-capture", { numberOfOutputs: 1, outputChannelCount: [1] });
    node.port.onmessage = (e) => {
      if (!this.closed) onChunk(e.data as Float32Array);
    };
    const silent = context.createGain();
    silent.gain.value = 0; // the graph only runs when it reaches the output; nothing is played
    context.createMediaStreamSource(stream).connect(node);
    node.connect(silent).connect(context.destination);
    return context.sampleRate;
  }

  /** Stops listening and lets go of the microphone. Safe to call at any time, any number of times. */
  stop() {
    this.closed = true;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    void this.context?.close().catch(() => {});
    this.context = null;
  }
}

/** True for the errors that mean "the person or the system said no to the microphone". */
export function micRefused(err: unknown): boolean {
  const name = (err as DOMException)?.name ?? "";
  return name === "NotAllowedError" || name === "SecurityError" || name === "NotFoundError";
}
