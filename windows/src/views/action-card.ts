// The confirmation card for assistant actions. Rust builds the card from fixed labels and checked values;
// this file only draws it. Everything is set with textContent, so nothing a model wrote can become markup.
//
// Rules this view keeps:
//   * Allow is the only way to approve, and only a real mouse click counts: `isTrusted` rules out script-made
//     clicks, `detail === 0` rules out Enter or Space on a focused button. There is no keyboard shortcut.
//   * The buttons never take focus, so Enter in the chat field can never land on Allow.
//   * A critical action asks for a second, separate click.
//   * Cut text says how much is hidden. Content that came after outside material is flagged.

import { h, clear } from "./dom";
import type { CardLine, PendingAction } from "../core/state";

export interface ActionCardHandlers {
  allow: (id: number) => void;
  deny: (id: number) => void;
}

/** A moment from its ISO text, in the user's own time zone. Falls back to the card's own words. */
export function localMoment(iso: string | undefined, fallback: string): string {
  if (!iso) return fallback;
  const when = new Date(iso);
  if (Number.isNaN(when.getTime())) return fallback;
  return when.toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZoneName: "short",
  });
}

function lineView(line: CardLine): HTMLElement {
  const shown = line.kind === "dateTime" ? localMoment(line.iso, line.value) : line.value;
  const value = h("div", { class: `ac-value ${line.kind === "content" ? "quoted" : ""}`, text: shown });
  const row = h("div", { class: "ac-line" }, h("div", { class: "ac-label", text: line.label }), value);
  if (line.hiddenCharacters > 0) {
    row.append(
      h("div", {
        class: "ac-hidden",
        text: `…and ${line.hiddenCharacters.toLocaleString()} more characters not shown`,
      }),
    );
  }
  return row;
}

/** True only for a click a person made with the mouse. */
export function isRealClick(e: Event): boolean {
  return e.isTrusted && (e as MouseEvent).detail > 0;
}

export interface ActionCard {
  el: HTMLElement;
  /** Redraws for the given waiting actions (oldest first). Hides itself when there are none. */
  sync(actions: PendingAction[], nowSeconds: number): void;
}

export function buildActionCard(handlers: ActionCardHandlers): ActionCard {
  const el = h("div", { class: "action-card", hidden: true });
  let confirming = -1; // id of the critical action that has had its first click

  function sync(actions: PendingAction[], nowSeconds: number) {
    const live = actions.filter((a) => a.expiresAt > nowSeconds);
    el.hidden = live.length === 0;
    clear(el);
    if (live.length === 0) {
      confirming = -1;
      return;
    }
    const action = live[0];
    const card = action.card;
    if (confirming !== action.id) confirming = -1;

    const head = h(
      "div",
      { class: "ac-head" },
      h("div", { class: "ac-title", text: card.title }),
      live.length > 1 ? h("div", { class: "ac-count", text: `1 of ${live.length}` }) : null,
    );
    const body = h("div", { class: "ac-lines" }, ...card.lines.map(lineView));

    const deny = h("button", { class: "btn secondary", tabindex: -1, text: "Deny" });
    const allow = h("button", {
      class: "btn primary",
      tabindex: -1,
      text: card.needsSecondClick ? (confirming === action.id ? "Click again to confirm" : "Allow…") : "Allow",
    });
    // A press on a button must not move focus out of the chat field or onto the button.
    for (const b of [deny, allow]) b.addEventListener("mousedown", (e) => e.preventDefault());
    deny.addEventListener("click", (e) => {
      if (!isRealClick(e)) return;
      confirming = -1;
      handlers.deny(action.id);
    });
    allow.addEventListener("click", (e) => {
      if (!isRealClick(e)) return;
      if (card.needsSecondClick && confirming !== action.id) {
        confirming = action.id;
        sync(actions, nowSeconds);
        return;
      }
      handlers.allow(action.id);
    });

    el.append(head);
    if (card.fromUntrustedContent) {
      el.append(
        h("div", {
          class: "ac-warn",
          text: "This came after I read a file, window or web page. Check every detail before you allow it.",
        }),
      );
    }
    el.append(body, h("div", { class: "ac-buttons" }, deny, allow));
  }

  return { el, sync };
}
