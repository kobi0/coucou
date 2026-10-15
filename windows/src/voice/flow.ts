// Hold to talk, glued to the chat. The rules (a tap is dropped, a minute is the longest, silence is a hint and
// never a message) live in Rust, in coucou-assistant's VoiceSession. This file only does what Rust asks.

import { Bridge, type VoiceCommand, type VoiceLines } from "../core/bridge";
import { State } from "../core/state";
import { Hearing, Speaker, speakable, type RecognizerResult } from "./speech";

export interface VoiceFlowHooks {
  /** Puts words in the text box. */
  setDraft(text: string): void;
  /** Sends words as the person's message. */
  send(text: string): void;
  /** Shows a line the app wrote (never the model). */
  say(line: string): void;
  /** Called whenever listening starts or stops. */
  onListening(listening: boolean): void;
}

/** Reads a reply aloud when "Read replies aloud" is on and this computer has a voice that stays on it. */
export async function speakReply(text: string, hasPendingAction: boolean) {
  if (!State.settings.speakReplies) return;
  const lines = await Bridge.voiceLines();
  if (!lines) return;
  const spoken = speakable(text);
  if (spoken) {
    const full = spoken.truncated ? `${spoken.text} ${lines.restOnScreen}` : spoken.text;
    Speaker.speak(full, lines.voiceLocales, true);
  }
  if (hasPendingAction) Speaker.speak(lines.checkTheCard, lines.voiceLocales, !spoken);
}

export function createVoiceFlow(hooks: VoiceFlowHooks) {
  let lines: VoiceLines | null = null;
  let ticker: number | undefined;
  let listening = false;

  function setListening(on: boolean) {
    if (listening === on) return;
    listening = on;
    hooks.onListening(on);
  }

  function stopTicking() {
    window.clearInterval(ticker);
    ticker = undefined;
  }

  async function finish(result: RecognizerResult) {
    switch (result.kind) {
      case "transcript": return run(await Bridge.voiceFinished(result.text));
      case "nothing": return run(await Bridge.voiceFinished(null));
      case "blocked": return run(await Bridge.voiceFailed("blocked"));
      case "unavailable": return run(await Bridge.voiceFailed("unavailable"));
      case "failed": return run(await Bridge.voiceFailed("failed"));
    }
  }

  function run(commands: VoiceCommand[] | null) {
    for (const c of commands ?? []) {
      switch (c.kind) {
        case "start": {
          const hearing = Hearing.get();
          if (!hearing) break;
          setListening(true);
          ticker = window.setInterval(() => void Bridge.voiceTick().then(run), 1000);
          hearing.start(
            lines?.recognizerLocales ?? ["en-GB"],
            (partial) => void Bridge.voiceHeard(partial).then((clean) => clean !== null && hooks.setDraft(clean)),
            (result) => void finish(result),
          );
          break;
        }
        case "stop":
          stopTicking();
          setListening(false);
          Hearing.get()?.stop();
          break;
        case "discard":
          stopTicking();
          setListening(false);
          Hearing.get()?.cancel();
          hooks.setDraft("");
          break;
        case "send":
          stopTicking();
          setListening(false);
          if (State.settings.voiceAutoSend) {
            hooks.setDraft("");
            hooks.send(c.text);
          } else {
            hooks.setDraft(c.text);
          }
          break;
        case "hint":
          hooks.say(c.text);
          if (State.settings.speakReplies && lines) Speaker.speak(c.text, lines.voiceLocales, true);
          break;
      }
    }
  }

  return {
    /** The mic button went down. */
    async press() {
      if (!Hearing.get()) return;
      Speaker.stop();
      lines = await Bridge.voiceLines();
      run(await Bridge.voicePress());
    },
    /** The mic button came up, or the pointer left it. */
    async release() {
      run(await Bridge.voiceRelease());
    },
    /** The chat closed or something else took over. */
    async cancel() {
      run(await Bridge.voiceCancel());
    },
    get listeningText() {
      return lines?.listening ?? "Listening…";
    },
  };
}
