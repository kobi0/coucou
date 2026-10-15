# Oma

Oma is the everyday-assistant version of this project: a small robot who lives at the top of your screen and
helps with emails, reminders and daily tasks, in English or Nigerian Pidgin. The coding-agent features stay
as an optional developer pack. This folder holds everything that is Oma's own, so it never gets mixed up with
Coucou's name, Mochi or the Coucou sounds.

## What is here

| Path | What it is |
|---|---|
| `character/CHARACTER-SHEET.md` | The design brief: the look, the palette, the states, the outfits, what to check. |
| `character/sprites/` | The 30 expressions from the character sheet, one transparent picture each, and `manifest.json`. |
| `character/slice-sheet.py` | Cuts the 30-expression sheet into those pictures. |
| `PROVENANCE.md` | Where the artwork came from, and the rights to it. Fill this in before the pictures are published. |

The code that draws her is in the apps, not here:

- `windows/src/oma/sprites.ts` draws the 30 expressions exactly as on the sheet and maps each state of the app
  to one of them (needs-your-click shows "curious", working shows "searching").
- `windows/src/oma/oma.ts` is a second Oma drawn in code, with a cartoon eye-pop for surprise. It is the
  stand-in when the pictures are missing.

The Windows and Linux app serves this folder's `sprites/` at `/oma/` (see `windows/vite.config.ts`), the same
way it shares the sounds.

## The 30 expressions

idle, listening, thinking, searching, speaking, greeting, happy, laughing, surprised, curious, confused, shy,
sad, worried, annoyed, sleepy, sleeping, winking, love, proud, celebrating, apologetic, error, success,
uploading, dancing, sunglasses, winter-scarf, party-hat, crown.

To cut them again from the sheet:

```
python3 oma/character/slice-sheet.py <the sheet>.png oma/character/sprites
```

The sheet is small (each picture is about 150 px tall), so they look best at that size. A larger render of the
sheet would make them sharper without changing anything else.

## Status

Done: the assistant core (draft freely, act only on a click), the confirmation card, voice on the Mac, the 30
expressions as pictures and a code-drawn Oma. Oma now lives in the Windows island (Settings → Character switches
back to Mochi); if the pictures are missing she is drawn in code instead.

The small ones on the pills and in the compact grid are Oma too, each dyed purple, green, blue or army.

Voice on Windows: Pidgin/English language, reading replies aloud with local voices, and the hold-to-talk plumbing are
in. Hearing is on-device, with the Parakeet model Flow dictation already ships (the person downloads it from Flow's settings);
the code is written and its parts are tested, but it has not yet run on Windows or heard a real voice.
The Pidgin and English voice can instead be Gemini Live (opt-in, Windows first; the Mac has no Gemini voice yet): the
only path that sends the voice to a server, behind an explicit choice and a Gemini key the person adds.

Not done: Oma in the Mac app (it still shows Mochi), and on Windows the greeting and the drop animation still show Mochi. Also the Nigerian outfits, the Pidgin varieties file, Oma sounds, the new app name and icon.

## Licence and names

Coucou's code is MIT. The names "Coucou" and "Mochi", the Mochi character, the icons, the sounds and the media in
`docs/` and `design/` belong to Louis Raillé (see `LICENSE-ASSETS.md`). A fork that ships needs its own name,
icon, character and sounds. Oma replaces Mochi; the sounds still have to be replaced, and Louis's permission has
to be written down, before anything is published under a new name. The pictures in `character/sprites/` are kept
out of git (see `.gitignore`) until `PROVENANCE.md` is filled in.
