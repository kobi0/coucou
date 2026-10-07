// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, type ChatContext } from "../core/bridge";
import { Sound } from "../core/sound";
import { State, type ChatMessage } from "../core/state";
import { buildActionCard } from "./action-card";
import type { ViewHost } from "./views";

let nextId = 1;

function bubble(message: ChatMessage): HTMLElement {
  if (message.role === "user") {
    return h(
      "div",
      { class: "chat-row user" },
      h("div", { class: "bubble", text: message.content }),
    );
  }
  return h("div", { class: "chat-row" }, h("div", { class: "reply", text: message.content }));
}

function typingDots(): HTMLElement {
  return h(
    "div",
    { class: "chat-row" },
    h("div", { class: "typing" }, h("i"), h("i"), h("i")),
  );
}

/** The coloured chip showing what the question is about (a dropped file). */
function contextChip(label: string): HTMLElement {
  const chip = h("div", { class: "chip" }, h("i", { class: "chip-dot" }), h("span", { text: label }));
  requestAnimationFrame(() => chip.classList.add("settled"));
  return chip;
}

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  const log = h("div", { class: "chat-log" });
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: "Ask me anything…",
    spellcheck: "false",
  }) as HTMLInputElement;
  const send = h("button", { class: "send-btn", title: "Send" }, svg(ICONS.arrowUp, 11));
  const bar = h("div", { class: "chat-bar" }, input, send);

  // Lines written by the app (never by the model) go in as assistant messages.
  function say(lines: string[]) {
    for (const line of lines) {
      if (line) State.chatHistory.push({ id: nextId++, role: "assistant", content: line });
    }
  }

  const actionCard = buildActionCard({
    allow: (id) => void allow(id),
    deny: (id) => void deny(id),
  });

  const el = h(
    "div",
    { class: "view" },
    h("div", { class: "card wash chat-card" }, h("div", { class: "chat-body" }, chipRow, log, bar, actionCard.el)),
  );
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  let renderedCount = -1;
  let expiryTimer: number | undefined;

  async function allow(id: number) {
    try {
      const reply = await Bridge.assistantApprove(id);
      State.pendingActions = reply.actions;
      say(reply.status === "rejected" ? [reply.message] : reply.notices);
      if (reply.status === "done") Sound.play("approve");
    } catch (err) {
      say([String(err).replace(/^Error:\s*/, "")]);
    }
    State.notify();
    onHeightChange();
  }

  async function deny(id: number) {
    try {
      State.pendingActions = await Bridge.assistantDeny(id);
    } catch (err) {
      say([String(err).replace(/^Error:\s*/, "")]);
    }
    State.notify();
    onHeightChange();
  }

  async function submit() {
    const query = input.value.trim();
    if (!query || sending) return;
    input.value = "";
    sending = true;
    Sound.play("send");

    State.chatHistory.push({ id: nextId++, role: "user", content: query });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();

    const file = State.droppedFile;
    const context: ChatContext | null =
      State.chatHistory.length === 1 && file ? { kind: "file", name: file.name, path: file.path } : null;

    try {
      const reply = await Bridge.chatSend(query, context);
      if (reply.text) State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      say(reply.notices);
      State.pendingActions = reply.actions;
      State.stateOverride = null;
      Sound.play("finish");
    } catch (err) {
      State.stateOverride = null;
      State.noteMessage = String(err).replace(/^Error:\s*/, "");
      State.view = "note";
      Sound.play("error");
    } finally {
      sending = false;
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  send.addEventListener("click", () => void submit());
  input.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") {
      e.preventDefault();
      void submit();
    }
    e.stopPropagation(); // Escape closes the island, not the chat
  });

  return {
    el,
    sync() {
      const file = State.droppedFile;
      const wantChip = file?.name ?? "";
      if (chipRow.dataset.label !== wantChip) {
        chipRow.dataset.label = wantChip;
        clear(chipRow);
        if (wantChip) chipRow.append(contextChip(wantChip));
      }

      const thinking = State.stateOverride === "thinking";
      const count = State.chatHistory.length + (thinking ? 0.5 : 0);
      if (count !== renderedCount) {
        renderedCount = count;
        clear(log);
        for (const m of State.chatHistory) log.append(bubble(m));
        if (thinking) log.append(typingDots());
        log.scrollTop = log.scrollHeight;
      }

      input.placeholder = State.chatHistory.length === 0 ? "Ask me anything…" : "Continue…";
      input.disabled = sending;

      const nowSeconds = Date.now() / 1000;
      actionCard.sync(State.pendingActions, nowSeconds);
      // A card that nobody answers disappears when it expires.
      window.clearTimeout(expiryTimer);
      const soonest = Math.min(...State.pendingActions.map((a) => a.expiresAt));
      if (Number.isFinite(soonest)) {
        expiryTimer = window.setTimeout(() => State.notify(), Math.max(250, (soonest - nowSeconds) * 1000 + 100));
      }
    },
    focus() {
      input.focus();
      input.select();
    },
  };
}
