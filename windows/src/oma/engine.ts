// Oma in the island. This is Mochi's engine with a different face: it keeps all of BotEngine's behaviour
// (states, emotes, slaps, dizzy, bounces, breathing, the look direction) and only changes what is drawn.
// So the island, its hover and click reactions, and the state machine need no changes at all.
//
// Each state and emote picks one of the 30 sheet expressions, drawn exactly as on the sheet. The engine's own
// motion (squash, tilt, hop, the pop of an emote) moves the whole picture. If the pictures are missing it draws
// the code-drawn Oma from oma.ts instead, so there is never an empty space where she should be.

import { BotEngine, type EyeShape } from "../mochi/engine";
import type { BotStateName } from "../core/layout";
import { drawOma, poseFor, type OmaState } from "./oma";
import { OmaSprites, SPRITE_HEIGHT, SPRITE_WIDTH, type Expression } from "./sprites";

/** The expression for each state the island can be in. */
export const EXPRESSION_FOR_BOT_STATE: Record<BotStateName, Expression> = {
  idle: "idle",
  working: "searching",
  thinking: "thinking",
  searching: "searching",
  approval: "curious",
  question: "confused",
  error: "error",
  finished: "success",
  ratelimit: "worried",
  sleeping: "sleeping",
  dizzy: "confused",
};

/** The expression for an emote, which the engine reports as an eye shape. */
export const EXPRESSION_FOR_EYE: Partial<Record<EyeShape, Expression>> = {
  heart: "love",
  dot: "surprised",
  star: "proud",
  wink: "winking",
  tired: "sleepy",
  happy: "happy",
  line: "annoyed",
  spiral: "confused",
  cup: "uploading",
};

/** The state of the code-drawn Oma that stands in for an expression when the pictures are missing. */
const FALLBACK_STATE: Partial<Record<Expression, OmaState>> = {
  idle: "idle", listening: "listening", thinking: "thinking", speaking: "speaking", surprised: "surprised",
  happy: "happy", success: "happy", laughing: "happy", love: "happy", proud: "happy", celebrating: "happy",
  error: "error", sad: "error", worried: "error", curious: "needsClick", confused: "needsClick",
  sleepy: "sleepy", sleeping: "sleepy", searching: "working", uploading: "working",
};

let sprites: OmaSprites | null = null;
let loading: Promise<void> | null = null;

/** Loads the pictures once. Resolves when they are ready or known to be missing. */
export function preloadOma(): Promise<void> {
  loading ??= OmaSprites.load().then((loaded) => {
    sprites = loaded;
  });
  return loading;
}

export class OmaEngine extends BotEngine {
  private shown: Expression = "idle";
  private shownSince = performance.now();

  /** Which sheet expression is showing now. */
  expression(): Expression {
    const wanted =
      (this.eyeOverride && EXPRESSION_FOR_EYE[this.eyeOverride]) || EXPRESSION_FOR_BOT_STATE[this.state];
    if (wanted !== this.shown) {
      this.shown = wanted;
      this.shownSince = performance.now();
    }
    return this.shown;
  }

  override draw(ctx: CanvasRenderingContext2D, W: number, H: number) {
    const expression = this.expression();
    const R = W * 0.3;
    // Where Mochi's body would be, plus a small lean towards what the pointer is doing.
    const cx = W / 2 + this.ox * R + this.lookX * W * 0.014;
    const cy = H / 2 + this.particleOverhang / 2 + this.oy * R + R * 0.06 - this.lookY * W * 0.008;
    const pop = 1 + (this.es - 1) * 0.6;

    if (sprites && sprites.image(expression)) {
      this.drawSprite(ctx, expression, W, cx, cy, pop);
    } else {
      this.drawFallback(ctx, expression, W, cx, cy, pop);
    }
  }

  private drawSprite(ctx: CanvasRenderingContext2D, expression: Expression, W: number, cx: number, cy: number, pop: number) {
    const img = sprites!.image(expression)!;
    // Every expression is drawn at one scale, set by the idle one, so she never changes size between them.
    const ref = sprites!.metrics("idle")!;
    const s = Math.min((0.66 * W) / ref.bodyWidth, (0.8 * W) / ref.bodyHeight);
    const feetY = cy + (ref.bodyHeight * s) / 2;
    ctx.save();
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.translate(cx, feetY);
    if (this.tilt !== 0) ctx.rotate(this.tilt);
    ctx.scale(this.sx * pop, this.sy * pop);
    ctx.drawImage(img, -ref.centerX * s, -ref.bodyBottom * s, SPRITE_WIDTH * s, SPRITE_HEIGHT * s);
    ctx.restore();
  }

  private drawFallback(ctx: CanvasRenderingContext2D, expression: Expression, W: number, cx: number, cy: number, pop: number) {
    const state = FALLBACK_STATE[expression] ?? "idle";
    const t = performance.now();
    const pose = poseFor(state, t, t - this.shownSince);
    const size = W * 0.96;
    const feetY = cy + size * 0.36;
    ctx.save();
    ctx.translate(cx, feetY);
    if (this.tilt !== 0) ctx.rotate(this.tilt);
    ctx.scale(this.sx * pop, this.sy * pop);
    ctx.translate(-size / 2, -size * 0.93);
    drawOma(ctx, pose, size);
    ctx.restore();
  }
}
