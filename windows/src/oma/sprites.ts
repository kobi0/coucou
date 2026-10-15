// Oma's 30 expressions, exactly as drawn on the character sheet: one transparent picture per expression,
// cut from the sheet and kept in public/oma (see public/oma/manifest.json). Nothing is redrawn here.
//
// If the pictures are not there (a fresh checkout before they are added), `OmaSprites.load` gives null and
// the caller falls back to the code-drawn Oma in oma.ts, so the app never shows an empty space.

import type { OmaState } from "./oma";

export const EXPRESSIONS = [
  "idle", "listening", "thinking", "searching", "speaking", "greeting",
  "happy", "laughing", "surprised", "curious", "confused", "shy",
  "sad", "worried", "annoyed", "sleepy", "sleeping", "winking",
  "love", "proud", "celebrating", "apologetic", "error", "success",
  "uploading", "dancing", "sunglasses", "winter-scarf", "party-hat", "crown",
] as const;

export type Expression = (typeof EXPRESSIONS)[number];

/** Which sheet expression shows for each state the app can be in. */
export const EXPRESSION_FOR_STATE: Record<OmaState, Expression> = {
  idle: "idle",
  listening: "listening",
  thinking: "thinking",
  speaking: "speaking",
  surprised: "surprised",
  happy: "happy",
  error: "error",
  needsClick: "curious",
  sleepy: "sleepy",
  working: "searching",
};

/** Where the body is on the shared canvas, in picture pixels (set by oma/character/slice-sheet.py). */
export interface SpriteMetrics {
  file: string;
  bodyWidth: number;
  bodyHeight: number;
  bodyTop: number;
  bodyBottom: number;
  centerX: number;
}

/** Size of every picture, in pixels. They share one canvas, so they line up when you swap them. */
export const SPRITE_WIDTH = 256;
export const SPRITE_HEIGHT = 220;

export class OmaSprites {
  private constructor(
    private readonly images: Map<Expression, HTMLImageElement>,
    private readonly sizes: Map<Expression, SpriteMetrics>,
  ) {}

  /** Loads all 30 pictures. Null when any is missing, so a half-drawn Oma is never shown. */
  static async load(base = "/oma/"): Promise<OmaSprites | null> {
    try {
      const response = await fetch(`${base}manifest.json`);
      if (!response.ok) return null;
      const manifest = (await response.json()) as Record<string, SpriteMetrics>;
      const images = new Map<Expression, HTMLImageElement>();
      const sizes = new Map<Expression, SpriteMetrics>();
      await Promise.all(
        EXPRESSIONS.map(async (name) => {
          const entry = manifest[name];
          if (!entry) throw new Error(`no sprite for ${name}`);
          const img = new Image();
          img.src = `${base}${entry.file}`;
          await img.decode();
          images.set(name, img);
          sizes.set(name, entry);
        }),
      );
      return new OmaSprites(images, sizes);
    } catch {
      return null;
    }
  }

  metrics(expression: Expression): SpriteMetrics | undefined {
    return this.sizes.get(expression);
  }

  /** The picture itself, for callers that place it with their own transform. */
  image(expression: Expression): HTMLImageElement | undefined {
    return this.images.get(expression);
  }

  has(expression: Expression): boolean {
    return this.images.has(expression);
  }

  /** Draws one expression with its top-left at (x, y), scaled so it is `height` pixels tall. */
  draw(ctx: CanvasRenderingContext2D, expression: Expression, x: number, y: number, height: number) {
    const img = this.images.get(expression);
    if (!img) return;
    const scale = height / SPRITE_HEIGHT;
    ctx.drawImage(img, x, y, SPRITE_WIDTH * scale, SPRITE_HEIGHT * scale);
  }
}
