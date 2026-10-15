// Speech in and out for the chat. Mirrors the Mac's SpeakableText, SpeechOutput and SpeechInput.
//
// Reading aloud uses the voices installed on this computer. A voice that sends the text to a server (the
// "online" or "natural" ones Edge and WebView2 also list) is never used: `localService` must be true.
//
// Hearing goes through a `Recognizer`. None ships yet, so the mic button stays hidden until one is plugged in
// with `setRecognizer`. What is heard is only ever text for the chat box. It can never approve an action.

export interface SpokenText {
  text: string;
  truncated: boolean;
}

const SPEAKABLE_LIMIT = 600;

/** Turns a reply into something worth reading aloud: no code, links or marks, one sentence per line. */
export function speakable(markdown: string, limit = SPEAKABLE_LIMIT): SpokenText | null {
  let text = markdown.replace(/[​-‏‪-‮⁠-⁤﻿]/g, "");
  const steps: [RegExp, string][] = [
    [/```[\s\S]*?```/g, " "], // code blocks are on the screen
    [/!\[[^\]]*\]\([^)]*\)/g, " "], // images
    [/\[([^\]]*)\]\([^)]*\)/g, "$1"], // links keep their words
    [/https?:\/\/\S+/g, " "], // bare links
    [/`+/g, ""], // inline code marks
    [/^[ \t]*#{1,6}[ \t]*/gm, ""], // headings
    [/^[ \t]*(?:[-*•]|\d+[.)])[ \t]+/gm, ""], // list marks
    [/[*~]+/g, ""], // bold, italic, strike marks
  ];
  for (const [pattern, template] of steps) text = text.replace(pattern, template);

  const sentences: string[] = [];
  for (const raw of text.split(/\r\n|[\r\n\u2028\u2029]/)) {
    const line = raw.split(/\s+/).filter(Boolean).join(" ");
    if (!line) continue;
    sentences.push(".!?:;,…".includes(line[line.length - 1]) ? line : line + ".");
  }
  const joined = sentences.join(" ");
  if (!joined) return null;
  const chars = Array.from(joined);
  if (chars.length <= limit) return { text: joined, truncated: false };

  const head = chars.slice(0, limit).join("");
  let cut = -1;
  for (const t of [". ", "! ", "? "]) cut = Math.max(cut, head.lastIndexOf(t));
  const body = (cut > 0 ? head.slice(0, cut + 1) : head).trim();
  return { text: body, truncated: true };
}

// ── Reading aloud ─────────────────────────────────────────────────────────────

/** The installed voice that best matches the wanted locales, in order. Online voices are never chosen. */
export function pickVoice(voices: SpeechSynthesisVoice[], locales: string[]): SpeechSynthesisVoice | null {
  const local = voices.filter((v) => v.localService);
  const norm = (s: string) => s.replace("_", "-").toLowerCase();
  for (const wanted of locales) {
    const hit = local.find((v) => norm(v.lang) === norm(wanted));
    if (hit) return hit;
  }
  for (const wanted of locales) {
    const base = norm(wanted).split("-")[0];
    const hit = local.find((v) => norm(v.lang).split("-")[0] === base);
    if (hit) return hit;
  }
  return null;
}

export const Speaker = {
  /** True when this computer has a voice that stays on it. */
  available(): boolean {
    return "speechSynthesis" in window && speechSynthesis.getVoices().some((v) => v.localService);
  },

  speak(text: string, locales: string[], interrupting: boolean): boolean {
    if (!("speechSynthesis" in window)) return false;
    const voice = pickVoice(speechSynthesis.getVoices(), locales);
    if (!voice) return false; // no voice that stays on this computer: say nothing rather than send text away
    if (interrupting) speechSynthesis.cancel();
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.voice = voice;
    utterance.lang = voice.lang;
    speechSynthesis.speak(utterance);
    return true;
  },

  stop() {
    if ("speechSynthesis" in window) speechSynthesis.cancel();
  },
};

// ── Hearing ───────────────────────────────────────────────────────────────────

export type RecognizerResult =
  | { kind: "transcript"; text: string }
  | { kind: "nothing" }
  | { kind: "blocked" }
  | { kind: "unavailable" }
  | { kind: "failed" }
  /** The engine could not start and has already told the person why (a line from the app, never the model). */
  | { kind: "problem"; line: "needsGeminiKey" | "liveFailed" };

export interface Recognizer {
  /** True when the engine answers by itself, speaking and writing the reply (Gemini Live). The words it heard
   *  are then not sent on to the chat model. */
  readonly handlesReply?: boolean;
  /** Where an engine that handles its own reply puts the words of the conversation, as they come. */
  onText?: (role: "user" | "model", text: string) => void;
  /** Starts hearing. `onFinished` is called once, unless `cancel` is called first. */
  start(locales: string[], onPartial: (text: string) => void, onFinished: (result: RecognizerResult) => void): void;
  /** Stops recording and delivers the final words through `onFinished`. */
  stop(): void;
  /** Stops recording and drops everything. `onFinished` is not called. */
  cancel(): void;
}

let recognizer: Recognizer | null = null;

export const Hearing = {
  set(next: Recognizer | null) {
    recognizer = next;
  },
  get(): Recognizer | null {
    return recognizer;
  },
};
