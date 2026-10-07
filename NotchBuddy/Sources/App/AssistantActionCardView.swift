import SwiftUI

// MARK: - Confirmation card for assistant actions
//
// Drawn over the chat while an action waits for the user. Everything on it comes from the ConfirmationCard the
// policy built: the title and the labels are fixed by the app, and the values are the call's own arguments with
// hidden characters removed. The Allow button is the only place in the app that approves an action, and it
// does so through the .click channel. There is no keyboard shortcut on purpose: Return belongs to the chat.

struct AssistantActionOverlay: View {
    @ObservedObject var state: AppState

    var body: some View {
        let pending = state.assistant.waiting(now: Date())
        if state.assistantTools || state.assistant.hasToolHistory, let action = pending.first {
            AssistantActionCard(state: state, action: action, othersWaiting: pending.count - 1)
                .id(action.id)
                .transition(.opacity)
        }
    }
}

struct AssistantActionCard: View {
    @ObservedObject var state: AppState
    let action: PendingAction
    let othersWaiting: Int
    /// A critical action has been clicked once and is waiting for the second click.
    @State private var armed = false
    @State private var problem: String? = nil

    var body: some View {
        ZStack(alignment: .leading) {
            CardBackground(wash: .amber)

            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Text(action.card.title)
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundColor(Color(hex: "#F1F2F4"))
                    Spacer(minLength: 4)
                    if othersWaiting > 0 {
                        Text("+\(othersWaiting) waiting")
                            .font(.system(size: 10.5))
                            .foregroundColor(Color(hex: "#7B8089"))
                    }
                }

                if action.card.fromUntrustedContent {
                    Text("Asked after reading a web page or file. Check every line.")
                        .font(.system(size: 10.5, weight: .medium))
                        .foregroundColor(Color(hex: "#F5A524"))
                }

                ScrollView(.vertical, showsIndicators: true) {
                    VStack(alignment: .leading, spacing: 3) {
                        ForEach(Array(action.card.lines.enumerated()), id: \.offset) { _, line in
                            lineView(line)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 46)

                if let problem {
                    Text(problem)
                        .font(.system(size: 10.5))
                        .foregroundColor(Color(hex: "#F4505E"))
                }

                HStack(spacing: 8) {
                    SecondaryButton("Deny") {
                        AssistantFlow.deny(action.id, state: state)
                    }
                    PrimaryButton(allowTitle) {
                        allow()
                    }
                }
            }
            .padding(.leading, 84)
            .padding(.trailing, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var allowTitle: String {
        if armed { return "Click again to confirm" }
        return action.card.needsSecondClick ? "Allow…" : "Allow"
    }

    @ViewBuilder private func lineView(_ line: ConfirmationLine) -> some View {
        HStack(alignment: .top, spacing: 6) {
            Text(verbatim: line.label)
                .font(.system(size: 10.5, weight: .medium))
                .foregroundColor(Color(hex: "#7B8089"))
                .frame(width: 52, alignment: .leading)
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: line.isContent ? "“" + line.value + "”" : line.value)
                    .font(.system(size: 11.5))
                    .foregroundColor(Color(hex: "#F1F2F4"))
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                if line.hiddenCharacters > 0 {
                    Text(verbatim: "and \(line.hiddenCharacters) more characters not shown")
                        .font(.system(size: 10))
                        .foregroundColor(Color(hex: "#F5A524"))
                }
            }
        }
    }

    /// The one place an action is approved. Only a click on this button gets here.
    private func allow() {
        switch state.assistant.approve(action.id, via: .click, now: Date()) {
        case .run(let useId, let call):
            Task { await AssistantFlow.runApproved(useId: useId, call: call, state: state) }
        case .needsSecondClick:
            armed = true
            problem = nil
        case .rejected(let message):
            problem = message
            // When the action is gone (expired, or already answered) the card disappears, so say it in the chat too.
            if !state.assistant.waiting(now: Date()).contains(where: { $0.id == action.id }) {
                AssistantFlow.say(message, state: state)
            }
        }
    }
}
