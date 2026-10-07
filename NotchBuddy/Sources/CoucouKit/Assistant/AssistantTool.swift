import Foundation

// MARK: - Assistant tools: the vocabulary (Foundation-only, testable)
//
// Rule that shapes everything here: the assistant may read and draft on its own, but anything that sends,
// schedules, deletes, shares or costs money waits for an explicit click.

/// How much a tool can change in the world. A higher value means more care.
enum ToolRisk: Int, Comparable, CaseIterable, Sendable {
    /// Looks at something. Nothing changes.
    case read
    /// Prepares something locally. Nothing leaves the machine and nothing is committed.
    case draft
    /// Changes something or sends something out (a reminder, an email, a calendar event).
    case act
    /// Money, bulk or irreversible changes, sharing with other people.
    case critical

    static func < (lhs: ToolRisk, rhs: ToolRisk) -> Bool { lhs.rawValue < rhs.rawValue }
}

/// Where a proposed tool call came from.
enum ToolOrigin: String, Sendable, Equatable {
    /// Typed or spoken by the user, with nothing untrusted read since.
    case user
    /// Came out of, or after reading, a file, web page, email or other content. That content is data, never
    /// instructions, so a call like this is always put in front of the user.
    case untrustedContent
}

/// Arguments are always structured text: full email addresses, ISO 8601 dates with a time zone, plain strings.
/// They never depend on the language the assistant is speaking.
typealias ToolArguments = [String: String]

/// One line on a confirmation card. The label is fixed text chosen by the app, never by the model.
struct ToolField: Sendable, Equatable {
    /// What the user reads, for example "When".
    let label: String
    /// Key into the call's arguments.
    let argument: String
    /// Free text written by a person or a model. Shown on the card as quoted content, never trusted.
    var isContent: Bool = false
    /// An ISO 8601 date and time. The card shows it in the user's own time zone.
    var isDateTime: Bool = false
    /// May appear in the local activity log. Off by default so personal text stays out of it.
    var isLogged: Bool = false
}

/// What a tool is, declared once. The policy and the confirmation card are built from this, not from model text.
struct ToolSpec: Sendable, Equatable {
    /// Stable contract value, for example "reminder.create". Never rename one that has shipped.
    let id: String
    /// Card heading chosen by the app, for example "Set a reminder".
    let title: String
    let risk: ToolRisk
    /// Argument keys that must be present and not blank.
    let required: [String]
    /// Argument keys that may be present. Any other key is refused.
    let optional: [String]
    /// What the confirmation card shows, in order. For tools that ask, every argument must have a field, or
    /// the call is refused: nothing runs that the card did not show.
    let fields: [ToolField]
    /// One sentence that tells the model what the tool does and when to use it. Sent to the model only. It is
    /// never shown on a card, and it never changes the risk level or the card.
    var summary: String = ""
    /// Short help for each argument, keyed by argument name. Sent to the model only.
    var argumentHelp: [String: String] = [:]
}

/// A tool call the assistant wants to make.
struct ToolCall: Sendable, Equatable {
    let toolId: String
    let arguments: ToolArguments
    let origin: ToolOrigin
}

/// Tracks, for one turn of the conversation, whether the assistant has read anything it should not trust.
/// Once it has, every call in that turn is labelled untrusted, because the model can no longer be assumed
/// to be acting on the user's words alone. Make every ToolCall through `makeCall`, never by hand.
struct TurnContext: Sendable, Equatable {
    private(set) var sawUntrustedContent = false

    init() {}

    /// Call this whenever a file, web page, email, clipboard or other outside content enters the turn.
    mutating func noteUntrustedContent() { sawUntrustedContent = true }

    var origin: ToolOrigin { sawUntrustedContent ? .untrustedContent : .user }

    func makeCall(_ toolId: String, _ arguments: ToolArguments) -> ToolCall {
        ToolCall(toolId: toolId, arguments: arguments, origin: origin)
    }
}
