import Foundation

// MARK: - Assistant session (Foundation-only, testable)
//
// Keeps the ledger for one conversation: which tool calls the model made, which are waiting for a click, what
// became of each, and what the model must be told next. It never runs a tool. It hands back "run this" steps
// and the app runs them, then reports the result with `finish`. Everything the model is told about an outcome
// is written here from fixed words and the tool's own title, never from text the model wrote.
//
// Once the conversation has read outside content (web search results, an attached file or window), every later
// call in it is labelled untrusted and asks the user, even a draft. The mark stays until the conversation ends,
// because the model still carries what it read.

struct ToolResult: Sendable, Equatable {
    let ok: Bool
    /// Plain words for the user and the model. Must not contain text written by the model.
    let message: String
}

/// What the model is told about one of its tool calls.
struct ToolResultText: Sendable, Equatable {
    let useId: String
    let text: String
}

enum ToolStep: Sendable, Equatable {
    /// Safe to run now. Run it, then call `finish`.
    case run(useId: String, call: ToolCall)
    /// Waiting for a click. The app shows `action.card`.
    case waiting(useId: String, action: PendingAction)
    /// Not done. The text says why.
    case refused(useId: String, reason: String)
}

enum ApprovalStep: Sendable, Equatable {
    /// A click approved it. Run it, then call `finish`.
    case run(useId: String, call: ToolCall)
    /// A critical action needs a second, separate click.
    case needsSecondClick
    case rejected(String)
}

struct AssistantSession: Sendable {
    private(set) var catalog: [String: ToolSpec]
    private(set) var store = PendingActionStore()
    /// True once the conversation has read outside content.
    private(set) var tainted = false

    /// Tool uses the model has not yet been answered about, in order.
    private var order: [String] = []
    private var calls: [String: ToolCall] = [:]
    private var results: [String: String] = [:]
    private var waitingLinks: [UUID: String] = [:]
    /// Tool uses whose answer has already gone to the model.
    private var reported: Set<String> = []
    private var notes: [String] = []
    private var logEntries: [ActivityEntry] = []

    init(catalog: [String: ToolSpec] = ToolCatalog.all) {
        self.catalog = catalog
    }

    /// True once the model has made any tool call in this conversation.
    var hasToolHistory: Bool { !calls.isEmpty }

    /// Actions still waiting for a click and not yet expired, oldest first.
    func waiting(now: Date) -> [PendingAction] {
        store.waiting.filter { now < $0.expiresAt }
    }

    /// Call when a file, window or other outside content is attached to the conversation.
    mutating func noteOutsideContent() { tainted = true }

    // MARK: A reply arrives

    mutating func beginTurn(_ turn: ExtractedTurn, now: Date) -> [ToolStep] {
        if turn.sawOutsideContent { tainted = true }
        sweepExpired(now: now)
        let wireToId = ToolSchemas.idsByWireName(catalog)
        var context = TurnContext()
        if tainted { context.noteUntrustedContent() }

        var steps: [ToolStep] = []
        for extracted in turn.calls {
            let useId = extracted.useId
            if calls[useId] != nil { continue }
            order.append(useId)

            if let problem = extracted.problem {
                let call = ToolCall(toolId: extracted.name, arguments: [:], origin: context.origin)
                calls[useId] = call
                refuse(useId, call, problem, now: now)
                steps.append(.refused(useId: useId, reason: problem))
                continue
            }

            let toolId = wireToId[extracted.name] ?? extracted.name
            let call = context.makeCall(toolId, extracted.arguments)
            calls[useId] = call
            switch store.propose(call, catalog: catalog, now: now) {
            case .run(let allowed):
                steps.append(.run(useId: useId, call: allowed))
            case .pending(let action):
                waitingLinks[action.id] = useId
                record(useId, .proposed, note: nil, now: now)
                steps.append(.waiting(useId: useId, action: action))
            case .refused(let reason):
                refuse(useId, call, reason, now: now)
                steps.append(.refused(useId: useId, reason: reason))
            }
        }
        return steps
    }

    // MARK: The user answers

    /// Only a click approves. Any other channel is rejected and the action keeps waiting.
    mutating func approve(_ id: UUID, via channel: ApprovalChannel, now: Date) -> ApprovalStep {
        sweepExpired(now: now)
        switch store.approve(id, via: channel, now: now) {
        case .approved(let call):
            guard let useId = waitingLinks.removeValue(forKey: id) else {
                return .rejected("That action is no longer waiting.")
            }
            record(useId, .approved, note: nil, now: now)
            return .run(useId: useId, call: call)
        case .needsSecondClick:
            return .needsSecondClick
        case .rejected(let message):
            return .rejected(message)
        }
    }

    mutating func deny(_ id: UUID, now: Date) {
        guard store.deny(id), let useId = waitingLinks.removeValue(forKey: id) else { return }
        record(useId, .denied, note: nil, now: now)
        settle(useId, "The user declined \"\(title(of: useId))\". Nothing was done.")
    }

    /// Reports what happened when the app ran a tool.
    mutating func finish(useId: String, ok: Bool, message: String, now: Date) {
        guard calls[useId] != nil, results[useId] == nil else { return }
        record(useId, ok ? .ran : .failed, note: ok ? nil : message, now: now)
        let name = title(of: useId)
        settle(useId, ok ? "Done: \"\(name)\". \(message)" : "It did not work: \"\(name)\". \(message)")
    }

    mutating func sweepExpired(now: Date) {
        for id in store.removeExpired(now: now) {
            guard let useId = waitingLinks.removeValue(forKey: id) else { continue }
            record(useId, .expired, note: nil, now: now)
            settle(useId, "The user did not answer in time, so \"\(title(of: useId))\" was not done.")
        }
    }

    /// Ends the conversation. Anything still waiting is dropped and logged as declined.
    mutating func endConversation(now: Date) {
        for action in store.waiting {
            deny(action.id, now: now)
        }
        order = []
        calls = [:]
        results = [:]
        waitingLinks = [:]
        reported = []
        notes = []
        tainted = false
        store = PendingActionStore()
    }

    // MARK: What the model is told next

    /// The answers the next request must carry, first the tool results (one for every tool use not yet
    /// answered, in order), then short notes about earlier requests that were settled since. This does not
    /// change anything: call `markFeedbackSent` once the request has gone through. If the request fails, the
    /// same answers are offered again.
    func feedback() -> (results: [ToolResultText], notes: [String]) {
        var blocks: [ToolResultText] = []
        for useId in order {
            let text = results[useId] ?? "The user has not answered yet. Nothing has been done."
            blocks.append(ToolResultText(useId: useId, text: text))
        }
        return (blocks, notes)
    }

    mutating func markFeedbackSent() {
        for useId in order { reported.insert(useId) }
        order = []
        notes = []
    }

    /// Log entries written since the last call. The app appends them to the activity log file.
    mutating func takeLog() -> [ActivityEntry] {
        let entries = logEntries
        logEntries = []
        return entries
    }

    // MARK: Helpers

    private func title(of useId: String) -> String {
        guard let call = calls[useId], let spec = catalog[call.toolId] else { return "an action" }
        return spec.title
    }

    private mutating func refuse(_ useId: String, _ call: ToolCall, _ reason: String, now: Date) {
        logEntries.append(ActivityLog.entry(call: call, spec: catalog[call.toolId], outcome: .refused, at: now, note: reason))
        settle(useId, "The app did not do this: \(reason)")
    }

    private mutating func record(_ useId: String, _ outcome: ActivityOutcome, note: String?, now: Date) {
        guard let call = calls[useId] else { return }
        logEntries.append(ActivityLog.entry(call: call, spec: catalog[call.toolId], outcome: outcome, at: now, note: note))
    }

    /// Stores the final answer for a tool use. When the model has already been told "not answered yet", the
    /// final answer goes out as a note with the next request instead.
    private mutating func settle(_ useId: String, _ text: String) {
        results[useId] = text
        if reported.contains(useId) {
            notes.append("Update from the app about an earlier request: \(text)")
        }
    }
}
