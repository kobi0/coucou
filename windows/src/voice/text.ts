// Text rules shared with the Mac and the Rust core. The same cases (tests/vectors/*.json) run against all three:
// `npm run test:vectors` runs these here. Change a rule here and it must change in Swift and Rust too.

/** Most combining marks kept in a row. Stops stacked "zalgo" text from filling the screen. */
export const MAX_COMBINING_MARKS = 4;

const HIDDEN = {
  control: /^\p{Cc}$/u,
  format: /^\p{Cf}$/u,
  separator: /^[\p{Zl}\p{Zp}]$/u,
  privateUse: /^[\p{Co}\p{Cs}]$/u,
  mark: /^\p{M}$/u,
};

/** ActionPolicy.stripHidden on the Mac, text::strip_hidden in Rust. Removes control, format (bidi overrides,
 *  zero-width spaces, soft hyphens, tags), private-use characters and combining marks beyond four in a row.
 *  The zero-width joiner and non-joiner stay. Line breaks and tabs stay when `keepLineBreaks`, else become spaces. */
export function stripHidden(text: string, keepLineBreaks: boolean): string {
  let out = "";
  let markRun = 0;
  for (const c of text) {
    if (HIDDEN.mark.test(c)) {
      markRun += 1;
      if (markRun <= MAX_COMBINING_MARKS) out += c;
      continue;
    }
    markRun = 0;
    if (HIDDEN.control.test(c)) {
      if (c === "\n" || c === "\t") out += keepLineBreaks ? c : " ";
    } else if (HIDDEN.format.test(c)) {
      if (c === "‌" || c === "‍") out += c;
    } else if (HIDDEN.separator.test(c)) {
      out += keepLineBreaks ? "\n" : " ";
    } else if (!HIDDEN.privateUse.test(c)) {
      out += c;
    }
  }
  return out;
}

export interface SpokenText {
  text: string;
  truncated: boolean;
}

export const SPEAKABLE_LIMIT = 600;

/** SpeakableText.make on the Mac. Turns a reply into something worth reading aloud: no code, links or marks, one
 *  sentence per line, and cut at a sentence (or a word) when it is long. Null when nothing is left to say. */
export function speakable(markdown: string, limit = SPEAKABLE_LIMIT): SpokenText | null {
  let text = stripHidden(markdown, true);
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

  // One sentence per line, so list items do not run together.
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
  let result = head;
  if (cut >= 0 && Array.from(head.slice(0, cut)).length >= Math.floor(limit / 3)) {
    result = head.slice(0, cut + 1);
  } else {
    const space = head.lastIndexOf(" ");
    if (space >= 0) result = head.slice(0, space);
  }
  const trimmed = result.replace(/^[ \t]+|[ \t]+$/g, "");
  return trimmed ? { text: trimmed, truncated: true } : null;
}
