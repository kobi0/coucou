import Foundation

// MARK: - Tool catalog (Foundation-only, testable)
//
// Every tool the assistant may call, declared once. Tool ids are stable contract values (they appear in the
// activity log and in saved settings): never rename one that has shipped. Add a tool by adding a spec here,
// a parser if it takes structured input, and the app code that performs it.

enum ToolCatalog {
    /// Sets a reminder. Changes the outside world, so it always waits for a click.
    static let reminderCreate = ToolSpec(
        id: "reminder.create",
        title: "Set a reminder",
        risk: .act,
        required: ["title", "due"],
        optional: ["notes"],
        fields: [
            ToolField(label: "Reminder", argument: "title", isContent: true),
            ToolField(label: "When", argument: "due", isDateTime: true, isLogged: true),
            ToolField(label: "Notes", argument: "notes", isContent: true)
        ]
    )

    /// Opens a draft email for the user to read. It does not send. Sending is a separate, later step.
    static let mailDraft = ToolSpec(
        id: "mail.draft",
        title: "Draft an email",
        risk: .draft,
        required: ["to", "subject", "body"],
        optional: [],
        fields: [
            ToolField(label: "To", argument: "to", isLogged: true),
            ToolField(label: "Subject", argument: "subject", isContent: true),
            ToolField(label: "Message", argument: "body", isContent: true)
        ]
    )

    static let all: [String: ToolSpec] = {
        var map: [String: ToolSpec] = [:]
        for spec in [reminderCreate, mailDraft] { map[spec.id] = spec }
        return map
    }()
}
