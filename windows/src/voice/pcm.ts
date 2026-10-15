// Turning what the microphone heard into what the recogniser wants: 16 kHz, mono, 16-bit. Pure functions.

export const TARGET_RATE = 16_000;

export function concat(chunks: Float32Array[]): Float32Array {
  let length = 0;
  for (const c of chunks) length += c.length;
  const out = new Float32Array(length);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

/** Brings samples at `rate` to 16 kHz. Going down, each new sample averages the old ones it covers, so
 *  high tones do not fold back into the voice. Going up (rare), it interpolates. */
export function resample(input: Float32Array, rate: number, target = TARGET_RATE): Float32Array {
  if (rate === target || input.length === 0) return input;
  const ratio = rate / target;
  const length = Math.floor(input.length / ratio);
  const out = new Float32Array(length);
  if (ratio > 1) {
    for (let i = 0; i < length; i++) {
      const start = Math.floor(i * ratio);
      const end = Math.min(input.length, Math.max(start + 1, Math.floor((i + 1) * ratio)));
      let sum = 0;
      for (let j = start; j < end; j++) sum += input[j];
      out[i] = sum / (end - start);
    }
  } else {
    for (let i = 0; i < length; i++) {
      const pos = i * ratio;
      const a = Math.floor(pos);
      const b = Math.min(input.length - 1, a + 1);
      out[i] = input[a] + (input[b] - input[a]) * (pos - a);
    }
  }
  return out;
}

/** Samples in -1..1 to 16-bit little-endian PCM. */
export function toPcm16(samples: Float32Array): Uint8Array {
  const view = new DataView(new ArrayBuffer(samples.length * 2));
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(i * 2, Math.round(s < 0 ? s * 32768 : s * 32767), true);
  }
  return new Uint8Array(view.buffer);
}

/** False for silence and room hiss. A recogniser given nothing but noise tends to invent words, so such a
 *  clip is treated as "nothing heard" and never sent. */
export function hasSpeech(samples: Float32Array): boolean {
  if (samples.length < TARGET_RATE * 0.25) return false;
  let sum = 0;
  let peak = 0;
  for (const s of samples) {
    sum += s * s;
    const a = Math.abs(s);
    if (a > peak) peak = a;
  }
  const rms = Math.sqrt(sum / samples.length);
  return rms > 0.004 && peak > 0.03;
}
