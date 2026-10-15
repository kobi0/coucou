// Runs the shared test vectors (../tests/vectors/*.json) against the TypeScript text rules.
// Swift and Rust run the same files. Needs Node 22.6 or newer (it reads .ts files directly).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const vectors = (name) => JSON.parse(readFileSync(join(here, "../../tests/vectors", name), "utf8"));
const { stripHidden, speakable } = await import("../src/voice/text.ts");

let failures = 0;
let passed = 0;
function check(label, got, want) {
  if (JSON.stringify(got) === JSON.stringify(want)) {
    passed += 1;
  } else {
    failures += 1;
    console.log(`  ✗ ${label}\n    got:      ${JSON.stringify(got)}\n    expected: ${JSON.stringify(want)}`);
  }
}

for (const c of vectors("strip_hidden.json").cases) {
  check(`stripHidden: ${c.name}`, stripHidden(c.input, c.keepLineBreaks), c.expected);
}
for (const c of vectors("voice.json").speakable) {
  const got = c.limit === undefined ? speakable(c.input) : speakable(c.input, c.limit);
  check(`speakable: ${c.name}`, got, c.expected);
}

console.log(`${passed} vector checks passed, ${failures} failed.`);
process.exit(failures === 0 ? 0 : 1);
