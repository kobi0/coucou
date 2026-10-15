// Picks what hears and speaks: Gemini Live if the person chose it in Settings, otherwise the on-device
// recogniser when its model file is in place. Asked at start, when settings change and when the chat opens.

import { Bridge } from "../core/bridge";
import { LiveRecognizer } from "./live";
import { LocalRecognizer } from "./local";
import { Hearing } from "./speech";

let live: LiveRecognizer | null = null;
let local: LocalRecognizer | null = null;

export async function refreshVoice(): Promise<void> {
  const gemini = await Bridge.liveStatus();
  if (gemini?.chosen) {
    // The mic shows even with no key yet, so pressing it can say why it is not working.
    live ??= new LiveRecognizer();
    if (Hearing.get() !== live) {
      Hearing.get()?.cancel();
      Hearing.set(live);
    }
    return;
  }
  if (Hearing.get() === live && live) {
    live.hangUp();
    Hearing.set(null);
  }
  const status = await Bridge.voiceRecognizerStatus();
  if (status?.state === "ready") {
    local ??= new LocalRecognizer();
    if (Hearing.get() !== local) Hearing.set(local);
  } else if (Hearing.get() === local && local) {
    local.cancel();
    Hearing.set(null);
  }
}
