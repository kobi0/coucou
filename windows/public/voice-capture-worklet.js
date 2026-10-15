// Hands the microphone's samples to the page in small chunks while the mic button is held.
// The audio stays in this window's memory: it is turned into text on this computer and then dropped.
class CoucouCapture extends AudioWorkletProcessor {
  process(inputs) {
    const channel = inputs[0] && inputs[0][0];
    if (channel) this.port.postMessage(channel.slice(0));
    return true;
  }
}
registerProcessor("coucou-capture", CoucouCapture);
