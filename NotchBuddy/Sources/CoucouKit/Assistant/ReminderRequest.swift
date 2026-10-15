import Foundation

// MARK: - Reminder request (Foundation-only, testable)
//
// Turns the structured arguments of "reminder.create" into a checked request. The EventKit or notification
// code that actually creates the reminder lives in the app and only ever receives a ReminderRequest, so
// nothing malformed or in the wrong language gets that far.
//
// `due` must be ISO 8601 with a time zone, for example 2026-10-07T16:00:00+01:00 or 2026-10-07T15:00:00Z.
// A date alone, or a time with no zone, is refused instead of guessed.

struct ReminderRequest: Sendable, Equatable {
    let title: String
    let due: Date
    let notes: String?
}

enum ReminderParseError: Error, Sendable, Equatable {
    case missingTitle
    case titleTooLong
    case notesTooLong
    case missingDue
    case badDate
    case dateInPast
    case dateTooFar

    var message: String {
        switch self {
        case .missingTitle:  return "The reminder needs a title."
        case .titleTooLong:  return "The reminder title is too long."
        case .notesTooLong:  return "The reminder notes are too long."
        case .missingDue:    return "The reminder needs a time."
        case .badDate:       return "I could not read that time. It needs a date, a time and a time zone."
        case .dateInPast:    return "That time has already passed."
        case .dateTooFar:    return "That time is too far away for a reminder."
        }
    }
}

enum ReminderParser {
    static let maxTitleLength = 200
    static let maxNotesLength = 2_000
    /// A reminder may be this many seconds in the past (a slow click, clock drift) and still count as now.
    static let pastGrace: TimeInterval = 60
    /// Five years.
    static let maxFuture: TimeInterval = 5 * 365 * 24 * 3600

    static func parse(_ arguments: ToolArguments, now: Date) -> Result<ReminderRequest, ReminderParseError> {
        let title = ActionPolicy.sanitize(arguments["title"] ?? "", limit: Int.max)
        if title.isEmpty { return .failure(.missingTitle) }
        if title.count > maxTitleLength { return .failure(.titleTooLong) }

        let dueText = (arguments["due"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if dueText.isEmpty { return .failure(.missingDue) }
        guard let due = parseISO8601(dueText) else { return .failure(.badDate) }
        if due < now.addingTimeInterval(-pastGrace) { return .failure(.dateInPast) }
        if due > now.addingTimeInterval(maxFuture) { return .failure(.dateTooFar) }

        var notes: String? = nil
        if let rawNotes = arguments["notes"] {
            let cleaned = ActionPolicy.sanitize(rawNotes, limit: Int.max)
            if cleaned.count > maxNotesLength { return .failure(.notesTooLong) }
            if !cleaned.isEmpty { notes = cleaned }
        }
        return .success(ReminderRequest(title: title, due: due, notes: notes))
    }

    /// ISO 8601 date and time with a time zone designator ("Z" or "+01:00"), with or without fractional seconds.
    /// Returns nil for anything else.
    static func parseISO8601(_ text: String) -> Date? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        // Do not rely on the formatter to refuse a time with no zone: some systems read it in the local zone,
        // which would silently move the reminder. Require "Z" or "+hh:mm" / "-hh:mm" at the end.
        guard trimmed.range(of: "(Z|[+-][0-9]{2}:[0-9]{2})$", options: .regularExpression) != nil else { return nil }
        let plain = ISO8601DateFormatter()
        plain.formatOptions = [.withInternetDateTime]
        if let date = plain.date(from: trimmed) { return date }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: trimmed)
    }
}
