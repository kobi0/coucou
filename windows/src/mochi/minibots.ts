// Mini Mochis (pills + compact grid) — port of MiniBotCanvasView.
// Each canvas owns a BotEngine; the island's frame loop ticks every live one.

import { BotEngine, hexToRGB } from "./engine";
import { OMA_MINI_COLORS, OmaEngine } from "../oma/engine";
import type { AgentTask } from "../core/state";

interface MiniBot {
  canvas: HTMLCanvasElement;
  engine: BotEngine;
  cssSize: number;
  taskId: string;
  /** Asked every frame: whether this one dances (the music pill's, MiniBotCanvasView). */
  dancing?: () => boolean;
  task: AgentTask;
  index: number;
}

let character: "oma" | "mochi" = "mochi";

function makeEngine(task: AgentTask, index: number): BotEngine {
  let engine: BotEngine;
  if (character === "oma") {
    const oma = new OmaEngine();
    oma.bodyTint = OMA_MINI_COLORS[index % OMA_MINI_COLORS.length];
    engine = oma;
  } else {
    engine = new BotEngine();
    engine.bodyColor = hexToRGB(task.color);
  }
  engine.isMini = true;
  engine.setState(task.state, true);
  if (task.emote) engine.setPermanentEmote(task.emote);
  if (task.miniEye) {
    engine.permanentEye = task.miniEye;
    engine.eyeOverride = task.miniEye;
    engine.eyeOverrideUntil = Number.POSITIVE_INFINITY;
  }
  return engine;
}

/** Which character the small ones are. Swapping rebuilds each one's engine and keeps its state. */
export function setMiniCharacter(kind: "oma" | "mochi") {
  if (kind === character) return;
  character = kind;
  for (const mb of live.values()) {
    mb.engine = makeEngine(mb.task, mb.index);
  }
}

const live = new Map<HTMLCanvasElement, MiniBot>();

/**
 * Creates a mini Mochi whose **body** is `bodySize` CSS pixels across.
 *
 * The engine draws the body at 60 % of its canvas, so the canvas is
 * `bodySize / 0.6` and is centred in a `bodySize` slot, overflowing it — the
 * same thing SwiftUI does with a `.frame(width: 22/0.6)` inside a
 * `.frame(width: 22)`. Sizing the canvas itself to `bodySize` would shrink the
 * whole drawing to 60 %, which is what used to happen.
 */
export function createMiniBot(task: AgentTask, bodySize: number, dancing?: () => boolean, index = 0): HTMLElement {
  const slot = document.createElement("span");
  slot.className = "mini";
  slot.style.width = `${bodySize}px`;
  slot.style.height = `${bodySize}px`;

  const canvas = document.createElement("canvas");
  const engineSize = bodySize / 0.6;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  canvas.width = Math.round(engineSize * dpr);
  canvas.height = Math.round(engineSize * dpr);
  canvas.style.width = `${engineSize}px`;
  canvas.style.height = `${engineSize}px`;
  slot.append(canvas);

  const engine = makeEngine(task, index);

  live.set(canvas, { canvas, engine, cssSize: engineSize, taskId: task.id, dancing, task, index });
  return slot;
}

export function releaseMiniBot(canvas: HTMLCanvasElement) {
  live.delete(canvas);
}

/** Drops every canvas no longer in the document (views are rebuilt wholesale). */
export function pruneMiniBots() {
  for (const [canvas] of live) {
    if (!canvas.isConnected) live.delete(canvas);
  }
}

export function syncMiniBotStates(tasks: AgentTask[]) {
  for (const mb of live.values()) {
    const task = tasks.find((t) => t.id === mb.taskId);
    if (!task) continue;
    mb.engine.setState(task.state);
    mb.task = task;
    if (!(mb.engine instanceof OmaEngine)) mb.engine.bodyColor = hexToRGB(task.color);
  }
}

export function tickMiniBots(dt: number) {
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  for (const mb of live.values()) {
    const ctx = mb.canvas.getContext("2d");
    if (!ctx) continue;
    mb.engine.setDancing(mb.dancing?.() ?? false);
    mb.engine.update(dt);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, mb.cssSize, mb.cssSize);
    ctx.save();
    mb.engine.applyDance(ctx, mb.cssSize, mb.cssSize);
    mb.engine.draw(ctx, mb.cssSize, mb.cssSize);
    ctx.restore();
  }
}

export const miniBotCount = () => live.size;
