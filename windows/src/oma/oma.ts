// Oma, the new companion, drawn in code with Canvas 2D (no images), as set out in the character sheet:
// a white glossy dome, a dark visor that is the face, big glowing eyes, an antenna with a ring-light orb,
// side lights and two stubby feet. Only the glow parts change colour to show state; the shell stays white.
//
// This file is just the drawing and the timing. It does not touch the island, the engine or Mochi.
// `poseFor(state, t)` says what Oma looks like at a moment, `drawOma(ctx, pose, size)` paints it.

export type OmaState =
  | "idle" | "listening" | "thinking" | "speaking" | "surprised"
  | "happy" | "error" | "needsClick" | "sleepy" | "working";

export interface OmaPose {
  /** Where she looks, -1 to 1. The eyes and mouth move inside the visor, the side lights shift the other way. */
  lookX: number;
  lookY: number;
  /** 0 open, 1 closed. */
  blink: number;
  eye: "round" | "arc";
  /** Share of each eye hidden from the top (sleepy, focused). */
  lid: number;
  /** Positive: outer corners down (sad, worried). */
  lidTilt: number;
  /** Overall eye size. */
  size: number;
  /** Surprise pop. 1 is fully out of the visor, a little over 1 overshoots, below 0 squeezes. */
  bulge: number;
  mouth: "smile" | "flat" | "o" | "wobble";
  /** 0 closed, 1 wide open. Drives speech. */
  mouthOpen: number;
  /** Colour of the eyes and cheeks. */
  glow: string;
  /** Colour of the antenna orb and side lights: the state signal. */
  signal: string;
  glowAmount: number;
  blush: number;
  /** Body lift in design pixels (200 px design space). */
  hop: number;
  /** Vertical squash and stretch, about -0.1 to 0.1. */
  squash: number;
  tilt: number;
  /** Antenna lift, 0 to 1. */
  antenna: number;
  orb: number;
  ring: number;
  /** Strength of the "!!" marks. */
  marks: number;
  /** Eye tremble in design pixels. */
  jitter: number;
}

export const COLORS = {
  shell: "#F4F7FA",
  visor: "#12151B",
  glow: "#7CC4FF",
  glowLight: "#CFEAFF",
  ok: "#6FE3A1",
  attention: "#FFC857",
  trouble: "#FF8A7A",
};

export const BASE_POSE: OmaPose = {
  lookX: 0, lookY: 0, blink: 0, eye: "round", lid: 0, lidTilt: 0, size: 1, bulge: 0,
  mouth: "smile", mouthOpen: 0, glow: COLORS.glow, signal: COLORS.glow, glowAmount: 1, blush: 0.35,
  hop: 0, squash: 0, tilt: 0, antenna: 0, orb: 0.6, ring: 0.5, marks: 0, jitter: 0,
};

// ── Timing ────────────────────────────────────────────────────────────────────

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const lerp = (a: number, b: number, k: number) => a + (b - a) * k;
const easeOutCubic = (k: number) => 1 - Math.pow(1 - k, 3);
const easeInOut = (k: number) => (k < 0.5 ? 2 * k * k : 1 - Math.pow(-2 * k + 2, 2) / 2);
const easeOutBack = (k: number) => {
  const c1 = 1.70158;
  const c3 = c1 + 1;
  return 1 + c3 * Math.pow(k - 1, 3) + c1 * Math.pow(k - 1, 2);
};

/** Total length of the surprise, in milliseconds. */
export const SURPRISE_MS = 1900;

/**
 * The surprise pop, the cartoon way. `t` is milliseconds since she was startled.
 *   0 to 70      squeeze: the eyes dip smaller, so the pop reads as sudden
 *   70 to 190    pop: out of the visor, past full size
 *   190 to 1500  wobble on a spring, then hold, trembling a little
 *   1500 to 1900 pull back in, with a small bounce
 */
export function surprise(t: number) {
  const none = { bulge: 0, hop: 0, marks: 0, jitter: 0, antenna: 0, mouth: 0 };
  if (t <= 0 || t >= SURPRISE_MS) return none;
  let bulge: number;
  if (t < 70) {
    bulge = -0.8 * easeInOut(t / 70);
  } else if (t < 190) {
    bulge = lerp(-0.8, 1.3, easeOutCubic((t - 70) / 120));
  } else if (t < 1500) {
    const s = t - 190;
    bulge = 1 + 0.3 * Math.exp(-s / 110) * Math.cos(s / 38);
  } else {
    bulge = 1 - easeOutBack((t - 1500) / 400);
  }
  const s = Math.max(0, t - 190);
  const hold = t >= 190 && t < 1500 ? Math.min(1, (t - 190) / 300) * (1 - (t - 190) / 1310) : 0;
  return {
    bulge,
    hop: t < 190 ? 9 * ((t - 70) / 120 > 0 ? (t - 70) / 120 : 0) : 9 * Math.exp(-s / 180) * Math.abs(Math.cos(s / 60)),
    marks: t < 120 ? 0 : t < 1300 ? Math.min(1, (t - 120) / 80) : clamp(1 - (t - 1300) / 300, 0, 1),
    jitter: 1.1 * hold * Math.sin(t / 17),
    antenna: t < 70 ? 0 : Math.exp(-s / 260),
    mouth: t < 120 ? 0 : t < 1500 ? 0.65 - 0.25 * Math.min(1, s / 600) : lerp(0.4, 0, (t - 1500) / 400),
  };
}

/** A blink every few seconds, never two alike. Pure in `t`, so it needs no timer. */
function blinkAt(t: number): number {
  const period = 3700;
  const n = Math.floor(t / period);
  const start = n * period + 1500 + ((n * 977) % 1200);
  const k = (t - start) / 150;
  return k < 0 || k > 1 ? 0 : 1 - Math.abs(2 * k - 1);
}

/** What Oma looks like at `t` ms. `since` is how long she has been in this state, `level` is 0 to 1 audio level. */
export function poseFor(state: OmaState, t: number, since = 0, level = 0): OmaPose {
  const p: OmaPose = {
    ...BASE_POSE,
    blink: blinkAt(t),
    lookX: 0.35 * Math.sin(t / 2300),
    lookY: 0.15 * Math.sin(t / 1700 + 1),
    squash: 0.012 * Math.sin(t / 900),
  };
  switch (state) {
    case "idle":
      break;
    case "listening":
      Object.assign(p, { size: 1.1, mouth: "flat", ring: 0.55 + 0.45 * level, orb: 0.85, tilt: 0.05, lookX: 0, lookY: 0 });
      break;
    case "thinking":
      Object.assign(p, {
        lookX: 0.6, lookY: -0.7, mouth: "flat", orb: 0.5 + 0.4 * Math.sin(t / 220), ring: 0.5 + 0.5 * Math.sin(t / 300),
        tilt: 0.04 * Math.sin(t / 700),
      });
      break;
    case "speaking":
      Object.assign(p, { mouthOpen: clamp(level, 0, 1), orb: 0.6 + 0.4 * level, hop: 1.5 * level, lookX: 0, lookY: 0.05 });
      break;
    case "surprised": {
      const s = surprise(since);
      Object.assign(p, {
        bulge: s.bulge, hop: s.hop, marks: s.marks, jitter: s.jitter, antenna: s.antenna,
        mouth: "o", mouthOpen: s.mouth, blink: 0, lookX: p.lookX * 0.3, lookY: 0, orb: 1, ring: 0.9,
        squash: s.bulge > 0 ? 0.03 * Math.min(1, s.bulge) : 0,
      });
      break;
    }
    case "happy":
      Object.assign(p, {
        eye: "arc", mouthOpen: 0.55, signal: COLORS.ok, ring: 1, orb: 1, blush: 0.6,
        hop: since < 900 ? 6 * Math.abs(Math.sin(since / 150)) : 0,
      });
      break;
    case "error":
      Object.assign(p, {
        lid: 0.18, lidTilt: 0.9, mouth: "wobble", signal: COLORS.trouble, ring: 0.9, orb: 0.8, squash: -0.03, hop: -1.5,
        lookY: 0.3, lookX: 0, blush: 0.1,
      });
      break;
    case "needsClick":
      Object.assign(p, {
        lookX: 0, lookY: 0.55, mouth: "o", mouthOpen: 0.3, signal: COLORS.attention,
        orb: 0.6 + 0.4 * Math.sin(t / 420), ring: 0.6 + 0.4 * Math.sin(t / 420), hop: 2.5 * Math.abs(Math.sin(t / 380)),
      });
      break;
    case "sleepy":
      Object.assign(p, { lid: 0.55, glowAmount: 0.45, orb: 0.2, ring: 0.2, blush: 0.15, lookX: 0, lookY: 0.2, squash: 0.02 * Math.sin(t / 1400) });
      break;
    case "working":
      Object.assign(p, {
        lid: 0.3, mouth: "flat", ring: 0.5 + 0.5 * Math.sin(t / 260), orb: 0.7, hop: 2 * Math.abs(Math.sin(t / 340)), lookY: 0.15,
      });
      break;
  }
  return p;
}

// ── Drawing ───────────────────────────────────────────────────────────────────

function rgba(hex: string, a: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${clamp(a, 0, 1)})`;
}

function glowStyle(ctx: CanvasRenderingContext2D, color: string, blur: number) {
  ctx.shadowColor = color;
  ctx.shadowBlur = blur;
}

function noGlow(ctx: CanvasRenderingContext2D) {
  ctx.shadowBlur = 0;
  ctx.shadowColor = "transparent";
}

const VISOR = { x: 44, y: 76, w: 112, h: 72, r: 34 };

function drawEye(ctx: CanvasRenderingContext2D, p: OmaPose, side: -1 | 1) {
  const r0 = 15;
  const pop = Math.max(0, p.bulge);
  const base = r0 * p.size;
  const R = base * (p.bulge >= 0 ? 1 + 0.75 * p.bulge : 1 + 0.15 * p.bulge);
  const cx = 100 + side * (24 + 6.5 * pop) + p.lookX * 7 * (1 - 0.5 * Math.min(1, pop)) + p.jitter * side;
  const cy = 108 + p.lookY * 5 - 4.5 * pop + (p.jitter ? p.jitter * 0.4 : 0);
  const open = 1 - p.blink * 0.92;

  if (p.eye === "arc") {
    ctx.save();
    glowStyle(ctx, p.glow, 8 * p.glowAmount);
    ctx.strokeStyle = COLORS.glowLight;
    ctx.lineWidth = 4.5;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.arc(cx, cy + 6, 13, Math.PI * 1.12, Math.PI * 1.88);
    ctx.stroke();
    ctx.restore();
    return;
  }

  ctx.save();
  ctx.translate(cx, cy);
  ctx.scale(1, open);

  // The eyeball that pops out of the visor. It fades in as the pop begins.
  const ballAlpha = clamp(pop * 1.6, 0, 1);
  if (ballAlpha > 0) {
    ctx.save();
    ctx.globalAlpha = ballAlpha;
    ctx.fillStyle = "rgba(14,24,44,0.38)"; // shadow on the shell and the visor, so it stands off the face
    ctx.filter = "blur(3px)";
    ctx.beginPath();
    ctx.ellipse(1.5 * side, 5, R * 1.04, R * 1.0, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.filter = "none";
    const g = ctx.createRadialGradient(-R * 0.32, -R * 0.38, R * 0.1, 0, 0, R);
    g.addColorStop(0, "#FFFFFF");
    g.addColorStop(0.6, "#EEF4FB");
    g.addColorStop(1, "#B7C7DC");
    ctx.fillStyle = g;
    ctx.strokeStyle = "rgba(44,66,98,0.7)";
    ctx.lineWidth = 1.7;
    ctx.beginPath();
    ctx.arc(0, 0, R, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }

  // The eye itself: a dark disc with a glowing rim. Inside the ball it shrinks to a small pupil.
  const pupil = base * lerp(1, 0.4, clamp(pop, 0, 1.2));
  const px = p.lookX * R * 0.28 * Math.min(1, pop) + p.lookX * 0;
  const py = p.lookY * R * 0.2 * Math.min(1, pop);
  ctx.save();
  if (pop < 0.35) glowStyle(ctx, p.glow, 9 * p.glowAmount * (1 - pop * 2));
  const rim = ctx.createRadialGradient(px, py, pupil * 0.2, px, py, pupil);
  rim.addColorStop(0, "#1F3158");
  rim.addColorStop(0.75, "#0A0F1C");
  rim.addColorStop(1, "#05070D");
  ctx.fillStyle = rim;
  ctx.beginPath();
  ctx.arc(px, py, pupil, 0, Math.PI * 2);
  ctx.fill();
  noGlow(ctx);
  ctx.strokeStyle = rgba(p.glow, 0.85 * p.glowAmount * (1 - clamp(pop * 1.4, 0, 1)));
  ctx.lineWidth = 2.2;
  ctx.stroke();
  // two highlights
  ctx.fillStyle = "rgba(255,255,255,0.95)";
  ctx.beginPath();
  ctx.arc(px - pupil * 0.34, py - pupil * 0.38, pupil * 0.3, 0, Math.PI * 2);
  ctx.fill();
  ctx.beginPath();
  ctx.arc(px + pupil * 0.36, py + pupil * 0.32, pupil * 0.14, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
  ctx.restore();

  // Lids: sleepy, focused, sad. They are cut from the visor colour, over the eye.
  if (p.lid > 0.01) {
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R + 1.5, 0, Math.PI * 2);
    ctx.clip();
    const top = cy - R + p.lid * 2 * R;
    const outer = top + p.lidTilt * 7;
    const inner = top - p.lidTilt * 7;
    ctx.fillStyle = COLORS.visor;
    ctx.beginPath();
    ctx.moveTo(cx - R - 3, cy - R - 3);
    ctx.lineTo(cx + R + 3, cy - R - 3);
    ctx.lineTo(cx + R + 3, side === 1 ? outer : inner);
    ctx.lineTo(cx - R - 3, side === 1 ? inner : outer);
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }
}

function drawMouth(ctx: CanvasRenderingContext2D, p: OmaPose) {
  const x = 100 + p.lookX * 3;
  const y = 138 + p.lookY * 2 + Math.max(0, p.bulge) * 2;
  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const open = clamp(p.mouthOpen, 0, 1);
  if (p.mouth === "o" || (p.mouth === "smile" && open > 0.08)) {
    const w = p.mouth === "o" ? lerp(3, 9, open) : lerp(7, 13, open);
    const h = p.mouth === "o" ? lerp(3, 12, open) : lerp(2.5, 9, open);
    ctx.fillStyle = "#2A0E18";
    ctx.strokeStyle = "rgba(255,160,180,0.9)";
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.ellipse(x, y + h * 0.3, w, h, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    if (p.mouth === "smile" && open > 0.3) {
      ctx.fillStyle = "#FF8AA0";
      ctx.beginPath();
      ctx.ellipse(x, y + h * 0.9, w * 0.55, h * 0.4, 0, 0, Math.PI * 2);
      ctx.fill();
    }
  } else if (p.mouth === "flat") {
    ctx.strokeStyle = COLORS.glowLight;
    ctx.lineWidth = 2.4;
    ctx.beginPath();
    ctx.moveTo(x - 6, y + 2);
    ctx.lineTo(x + 6, y + 2);
    ctx.stroke();
  } else if (p.mouth === "wobble") {
    ctx.strokeStyle = COLORS.glowLight;
    ctx.lineWidth = 2.4;
    ctx.beginPath();
    ctx.moveTo(x - 9, y + 3);
    ctx.bezierCurveTo(x - 5, y - 2, x - 2, y + 6, x + 1, y + 1);
    ctx.bezierCurveTo(x + 4, y - 3, x + 6, y + 5, x + 9, y + 1);
    ctx.stroke();
  } else {
    ctx.strokeStyle = COLORS.glowLight;
    ctx.lineWidth = 2.4;
    ctx.beginPath();
    ctx.moveTo(x - 7, y);
    ctx.quadraticCurveTo(x, y + 8, x + 7, y);
    ctx.stroke();
  }
  ctx.restore();
}

function drawMarks(ctx: CanvasRenderingContext2D, p: OmaPose) {
  if (p.marks <= 0.01) return;
  ctx.save();
  ctx.globalAlpha = p.marks;
  glowStyle(ctx, p.glow, 10);
  ctx.strokeStyle = COLORS.glowLight;
  ctx.fillStyle = COLORS.glowLight;
  ctx.lineCap = "round";
  ctx.lineWidth = 3.4;
  const wiggle = Math.sin(p.marks * 40) * 0.6;
  for (const [x, y, a] of [[152, 40, 0.28], [168, 34, 0.28]] as const) {
    ctx.save();
    ctx.translate(x, y + wiggle);
    ctx.rotate(a);
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(-0.8, 12);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(-1.2, 18.5, 2.1, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }
  ctx.restore();
}

/** Paints Oma into a square of `size` pixels with its top-left at the origin. Call with a cleared canvas. */
export function drawOma(ctx: CanvasRenderingContext2D, p: OmaPose, size: number) {
  ctx.save();
  // A margin all round, so the antenna and the pop never touch the edge of the canvas.
  ctx.scale(size / 200, size / 200);
  ctx.translate(8, 14);
  ctx.scale(0.92, 0.92);

  // Shadow on the ground. It shrinks while she is in the air.
  const air = clamp(p.hop / 12, -0.2, 1);
  ctx.save();
  glowStyle(ctx, rgba(p.signal, 0.5 * p.glowAmount), 14);
  ctx.fillStyle = "rgba(30,60,110,0.28)";
  ctx.beginPath();
  ctx.ellipse(100, 187, 60 * (1 - air * 0.25), 7 * (1 - air * 0.3), 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();

  // Everything else pivots on the feet.
  ctx.translate(100, 178);
  ctx.rotate(p.tilt);
  ctx.scale(1 - p.squash * 0.6, 1 + p.squash);
  ctx.translate(-100, -178 - p.hop);

  // Feet
  for (const fx of [74, 126]) {
    const g = ctx.createRadialGradient(fx - 4, 168, 1, fx, 172, 18);
    g.addColorStop(0, "#FFFFFF");
    g.addColorStop(1, "#C3D1E3");
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.ellipse(fx, 172, 17, 9.5, 0, 0, Math.PI * 2);
    ctx.fill();
  }

  // Side lights (behind the shell edge): they shift the opposite way to the look direction.
  for (const side of [-1, 1] as const) {
    const x = 100 + side * 68 - p.lookX * 3 * side * 0.0 - p.lookX * 2.5;
    ctx.save();
    glowStyle(ctx, p.signal, 10 * p.ring * p.glowAmount);
    ctx.fillStyle = rgba(p.signal, 0.35 + 0.6 * p.ring * p.glowAmount);
    ctx.beginPath();
    ctx.roundRect(x - 3.5, 100, 7, 28, 3.5);
    ctx.fill();
    ctx.restore();
  }

  // Body
  const bodyPath = () => {
    ctx.beginPath();
    ctx.ellipse(100, 114, 72, 60, 0, 0, Math.PI * 2);
  };
  const shell = ctx.createRadialGradient(78, 84, 6, 100, 114, 112);
  shell.addColorStop(0, "#FFFFFF");
  shell.addColorStop(0.55, COLORS.shell);
  shell.addColorStop(1, "#B5C5DA");
  ctx.fillStyle = shell;
  bodyPath();
  ctx.fill();
  ctx.save();
  bodyPath();
  ctx.clip();
  const rimLight = ctx.createLinearGradient(0, 130, 0, 176);
  rimLight.addColorStop(0, rgba(p.signal, 0));
  rimLight.addColorStop(1, rgba(p.signal, 0.45 * p.glowAmount));
  ctx.fillStyle = rimLight;
  ctx.fillRect(20, 120, 160, 60);
  ctx.restore();
  // specular
  const spec = ctx.createRadialGradient(72, 70, 1, 72, 70, 26);
  spec.addColorStop(0, "rgba(255,255,255,0.9)");
  spec.addColorStop(1, "rgba(255,255,255,0)");
  ctx.fillStyle = spec;
  ctx.beginPath();
  ctx.ellipse(72, 70, 28, 11, -0.5, 0, Math.PI * 2);
  ctx.fill();

  // Visor
  const v = VISOR;
  const vg = ctx.createLinearGradient(0, v.y, 0, v.y + v.h);
  vg.addColorStop(0, "#1B2130");
  vg.addColorStop(1, "#0C0F15");
  ctx.fillStyle = vg;
  ctx.strokeStyle = "rgba(255,255,255,0.09)";
  ctx.lineWidth = 1.2;
  ctx.beginPath();
  ctx.roundRect(v.x, v.y, v.w, v.h, v.r);
  ctx.fill();
  ctx.stroke();
  ctx.fillStyle = "rgba(255,255,255,0.05)";
  ctx.beginPath();
  ctx.ellipse(100, v.y + 13, 44, 8, 0, 0, Math.PI * 2);
  ctx.fill();

  // Cheeks
  if (p.blush > 0.02 && p.eye === "round") {
    for (const side of [-1, 1]) {
      ctx.fillStyle = rgba(p.glow, 0.4 * p.blush * p.glowAmount);
      ctx.beginPath();
      ctx.ellipse(100 + side * 38 + p.lookX * 3, 134, 7, 3.6, 0, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  drawEye(ctx, p, -1);
  drawEye(ctx, p, 1);
  drawMouth(ctx, p);

  // Antenna
  const lift = p.antenna * 7;
  const orbY = 24 - lift;
  ctx.strokeStyle = "#DDE7F2";
  ctx.lineWidth = 4.2;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.moveTo(100, 58);
  ctx.lineTo(100, orbY + 8);
  ctx.stroke();
  const orb = ctx.createRadialGradient(97, orbY - 3, 1, 100, orbY, 11);
  orb.addColorStop(0, "#FFFFFF");
  orb.addColorStop(1, "#C9D7E8");
  ctx.fillStyle = orb;
  ctx.beginPath();
  ctx.arc(100, orbY, 10, 0, Math.PI * 2);
  ctx.fill();
  ctx.save();
  glowStyle(ctx, p.signal, 9 * p.orb * p.glowAmount);
  ctx.strokeStyle = rgba(p.signal, 0.3 + 0.7 * p.orb * p.glowAmount);
  ctx.lineWidth = 2.2;
  ctx.beginPath();
  ctx.arc(100, orbY, 5, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();

  drawMarks(ctx, p);
  ctx.restore();
}
