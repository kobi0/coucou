# Character sheet — the new companion

> Draft built from the reference render supplied by the project owner (a white, rounded robot with a dark
> visor face, glowing blue eyes, an antenna with a ring-light orb, a glowing side ring and two stubby feet).
> **Name is a placeholder: [CHARACTER NAME].** Colours below are approximate; sample them from the source file.
> This replaces the upstream character entirely. Nothing here depends on the upstream artwork.

## 0. Provenance first (do this before any artwork is drawn)

The whole character inherits the legal status of the reference image. Confirm and write down, in
`oma/PROVENANCE.md`:

- [ ] **Who made the image** and how: your own work, a commissioned designer, a stock/marketplace asset, or an
      image generator.
- [ ] **Rights**: a written assignment or licence that allows commercial use and modification. For generated
      images, read the generator's terms on commercial use and ownership.
- [ ] **Not someone else's character**: reverse-image-search the render and compare it with existing consumer
      robots, mascots and app icons. If it resembles an existing product closely, redesign the silhouette and
      details until it does not.
- [ ] **Keep the reference out of the public repo** until the above is confirmed.

## 1. Design read

| Element | What it is | Why it works for an assistant |
|---|---|---|
| Body | soft, wide, slightly flattened sphere, glossy white shell | friendly, non-human, reads at small sizes |
| Visor | dark rounded-rectangle face plate inset in the shell | a **screen face**: eyes, mouth and symbols can all be drawn on it, which suits voice and status |
| Eyes | big round eyes with two highlights, cool-blue glow | expressive and legible even at 24 pt |
| Mouth | tiny smile on the visor | easy to animate as visemes for speech |
| Cheeks | two soft glow spots under the eyes | warmth, emotion layer |
| Antenna | thin stalk with a ring-light orb | **status light**: listening, thinking, needs-you |
| Side ring | glowing ring on the shell's side | secondary status and a depth cue when he turns |
| Feet | two rounded stubs at the front-bottom | bounce, hop and weight; no arms to animate |

Silhouette at notch size: a wide dome with a small round bump on top. Keep that silhouette; it is
the identity. Test at 24, 32, 64 and 120 pt (the desktop-companion size).

## 2. Palette and material (approximate)

| Token | Use | Approx. |
|---|---|---|
| shell | body | cool white `#F4F7FA` with soft grey-blue shading |
| visor | face plate | near-black `#12151B` with a faint glass sheen |
| glow | eyes, ring, orb | `#7CC4FF` core, `#CFEAFF` highlight |
| cheek | blush spots | glow at ~40 % opacity |
| ok | success flash | soft green `#6FE3A1` |
| attention | needs your click | warm amber `#FFC857` |
| trouble | error | soft coral `#FF8A7A` (never harsh red) |

Rule: **only the glow elements change colour to signal state.** The shell stays white so the character
never looks alarming.

## 3. States and expressions (what the engine must be able to show)

Existing engine behaviours are kept where they apply. New voice-era states are marked **new**.

| State | Eyes | Mouth | Antenna / ring | Body |
|---|---|---|---|---|
| idle | slow blink, follow cursor | soft smile | orb dim, ring dim | breathing |
| peek / hover | eyes grow slightly | smile | orb pulses once | rises out of the island |
| greet | wide, bright | open smile | orb flicks, ring pulses | little hop and lean |
| **listening (new)** | wide, attentive | neutral | orb steady, soft pulse with input level | leans toward the user |
| **thinking (new)** | glance up and aside | small flat line | orb "chases" (ring segments turn) | slow sway |
| **speaking (new)** | soft, blink between phrases | **visemes** driven by audio level | orb brightens with speech | tiny bob on stress |
| working (multi-step task) | focused, half-lidded | flat | ring fills step by step | gentle bounce |
| **needs your click (new)** | look at the card | small "o" | orb and ring amber, slow pulse | gentle bounce until answered |
| done / happy | upturned arcs | big smile | ring flashes green | jump |
| error | flattened, slight droop | small wobble | ring coral, once | slump, then recover |
| sleepy | half-closed | small | all dim | slow breathing |
| **muted (new)** | normal | flat line | orb off | small head shake when poked |
| poked / annoyed | squint | pinched | ring flicker | squish |
| dizzy (3 fast pokes) | spiral eyes | wavy | ring spins | wobble |

Speaking visemes (v1, four shapes is enough): closed, small, open, wide. Map from audio amplitude, with a
short smoothing window. A fuller set can follow with a better TTS provider.

## 4. Rig for a code-drawn character (Canvas / SwiftUI)

The project draws its character in code, with no images. Build this one the same way.

Layers, back to front: **shadow → feet → body → side ring → visor → cheeks → eyes → mouth → antenna stalk →
antenna orb**.

- Body: an ovoid or superellipse with a soft highlight and a rim light; glossy look from two gradients and
  one specular shape, not from textures.
- Eye tracking: the visor is flat, so move the **eyes and mouth inside the visor** toward the cursor, shift the
  side ring a few points the opposite way, and nudge the antenna for parallax. That gives a convincing turn
  without a 3D model.
- Side ring: flips to the left or right depending on where the character looks.
- Everything glowing shares one `glow` colour that the state machine can change.
- Outfits (inherited feature) attach to the top of the shell; the antenna passes through or stands beside
  them. Design a hat slot early.
- Budget: still 0 % CPU when hidden; one `TimelineView` at 60 fps only while visible.

## 5. Outfits with a Nigerian touch (optional wardrobe set)

A first set that suits the audience and replaces the inherited seasonal outfits:

- **Fila cap** and **gele** (head wrap), drawn small and clean.
- **Ankara-pattern scarf** or bow tie (simple repeated shapes, not photos).
- **Green-white-green** accents for national days; plain festive hats for Christmas and Eid.
- Keep each outfit respectful and simple; ask people from the communities represented to review the designs.

## 6. Asset deliverables for the artist (or for you)

- Turnaround: front, three-quarter, side, back.
- Expression sheet covering every row in section 3.
- Colour and material spec with exact sampled values.
- Size ladder: 24 / 32 / 64 / 120 / 256 / 1024 px renders to check legibility.
- **App icon**: visor face and glow on a soft gradient, no text.
- **Menu-bar / tray glyph**: a template (single-colour) silhouette, e.g. dome plus antenna orb, readable at 18 pt.
- **Sound palette** (replace all upstream sounds): soft chimes and gentle synth blips for greet, listen start,
  listen end, done, needs-click, error; short and quiet; no melody copied from anything. Voice lines themselves
  are covered in `docs/VOICE.md`.

## 7. Naming and trademark note

The character name and the product name can differ. Whichever you choose, run a trademark and app-store search
first. An AI assistant named Omi already exists, so that name carries real conflict risk.
