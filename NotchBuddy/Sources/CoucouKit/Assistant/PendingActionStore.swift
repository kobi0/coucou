import Foundation

// MARK: - Pending actions and approval (Foundation-only, testable)
//
// Holds every proposed action that is waiting for a person. The only way out of "waiting" with a yes is
// approve(_:via:now:) with the .click channel. A spoken yes, a message written by the model, or an automatic
// trigger cannot approve anything. Voice may prompt ("You wan make I send am?"); only a click approves.

enum ApprovalChannel: String, Sendable, Equatable {
    /// A deliberate click or tap on the Allow button of the confirmation card (on the Mac or the iPhone).
    case click
    /// A spoken "yes". Voice can prompt, it cannot approve.
    case voice
    /// Text written by the model.
    case model
    /// A script, shortcut or any other automatic trigger.
    case automation
}

struct PendingAction: Sendable, Equatable, Identifiable {
    let id: UUID
    let call: ToolCall
    let card: ConfirmationCard
    let createdAt: Date
    let expiresAt: Date
    /// Time of the first click on a critical action. Nil until then.
    var firstClickAt: Date?
}

enum ApprovalResult: Sendable, Equatable {
    /// Go ahead and run this call. It has been removed from the waiting list.
    case approved(ToolCall)
    /// A critical action needs one more, separate click.
    case needsSecondClick
    /// Nothing runs. The text says why, in plain words.
    case rejected(String)
}

enum Proposal: Sendable, Equatable {
    /// The policy says this is safe to run without asking.
    case run(ToolCall)
    /// Show this action's card and wait for a click.
    case pending(PendingAction)
    /// Do not run. The text says why.
    case refused(String)
}

struct PendingActionStore: Sendable, Equatable {
    private(set) var items: [UUID: PendingAction] = [:]
    /// How long an action waits before it expires, in seconds.
    var timeToLive: TimeInterval = 600
    /// Most actions that may wait at once.
    var maxPending: Int = 20
    /// A second click closer than this to the first counts as the same gesture (a double-click).
    var minimumSecondClickGap: TimeInterval = 0.8

    init() {}

    /// Actions still waiting, oldest first.
    var waiting: [PendingAction] {
        items.values.sorted {
            if $0.createdAt != $1.createdAt { return $0.createdAt < $1.createdAt }
            return $0.id.uuidString < $1.id.uuidString
        }
    }

    /// Entry point for every tool call the assistant wants to make.
    mutating func propose(_ call: ToolCall, catalog: [String: ToolSpec], now: Date, id: UUID = UUID()) -> Proposal {
        switch ActionPolicy.decide(call, catalog: catalog) {
        case .run:
            return .run(call)
        case .refuse(let reason):
            return .refused(reason)
        case .askUser(let card):
            removeExpired(now: now)
            guard items.count < maxPending else {
                return .refused("Too many actions are waiting. Answer or dismiss some first.")
            }
            let action = PendingAction(
                id: id,
                call: call,
                card: card,
                createdAt: now,
                expiresAt: now.addingTimeInterval(timeToLive),
                firstClickAt: nil
            )
            items[id] = action
            return .pending(action)
        }
    }

    /// Only the .click channel can approve. Any other channel is rejected and the action keeps waiting.
    mutating func approve(_ id: UUID, via channel: ApprovalChannel, now: Date) -> ApprovalResult {
        guard channel == .click else {
            return .rejected("Only a click on the card can approve an action.")
        }
        guard var action = items[id] else {
            return .rejected("That action is no longer waiting.")
        }
        if now >= action.expiresAt {
            items[id] = nil
            return .rejected("That action expired. Ask again if you still want it.")
        }
        if action.card.needsSecondClick {
            guard let first = action.firstClickAt else {
                action.firstClickAt = now
                items[id] = action
                return .needsSecondClick
            }
            if now.timeIntervalSince(first) < minimumSecondClickGap {
                return .needsSecondClick
            }
        }
        items[id] = nil
        return .approved(action.call)
    }

    /// The user said no. Returns false when the action was not waiting.
    @discardableResult
    mutating func deny(_ id: UUID) -> Bool {
        items.removeValue(forKey: id) != nil
    }

    /// Drops actions that waited too long and returns their ids, so the app can log them.
    @discardableResult
    mutating func removeExpired(now: Date) -> [UUID] {
        let gone = items.values.filter { now >= $0.expiresAt }.map { $0.id }
        for id in gone { items[id] = nil }
        return gone
    }
}
