import Foundation

// MARK: - Action policy (Foundation-only, testable)
//
// Decides, from the tool's declared spec and where the call came from, whether a call may run, must be put in
// front of the user, or is refused. The confirmation card is built here from the spec's fixed labels and the
// call's structured arguments. Text written by the model never becomes a label, a title or a risk level, so the
// model cannot disguise what a click does. For tools that ask, an argument with no card field is refused, so
// nothing runs that the card did not show, and anything cut short on the card is counted and flagged.

/// One line of a confirmation card.
struct ConfirmationLine: Sendable, Equatable {
    let label: String
    let value: String
    /// True when the value is free text written by a person or a model. Show it as quoted content.
    let isContent: Bool
    /// How many characters were cut from the value to fit the card. Zero when all of it is shown. When it is
    /// not zero, the card must say so, for example "and 1,450 more characters".
    let hiddenCharacters: Int
}

struct ConfirmationCard: Sendable, Equatable {
    let title: String
    let risk: ToolRisk
    let lines: [ConfirmationLine]
    /// True when the call came out of, or after reading, a file, web page or email. Show that clearly.
    let fromUntrustedContent: Bool
    /// True for critical tools: the user must click twice.
    let needsSecondClick: Bool
}

enum PolicyDecision: Sendable, Equatable {
    /// Safe to run without asking.
    case run
    /// Put this card in front of the user. Nothing runs until a click approves it.
    case askUser(ConfirmationCard)
    /// Do not run. The text says why, in plain words.
    case refuse(String)
}

enum ActionPolicy {
    /// Longest argument value accepted at all, in UTF-8 bytes.
    static let maxArgumentBytes = 20_000
    /// Longest value shown on a card, in characters (Unicode scalars).
    static let maxDisplayLength = 500
    /// Most combining marks kept in a row. Stops stacked "zalgo" text from filling the screen.
    static let maxCombiningMarks = 4

    static func decide(_ call: ToolCall, catalog: [String: ToolSpec], timeZone: TimeZone = .current) -> PolicyDecision {
        guard let spec = catalog[call.toolId] else {
            return .refuse("Unknown tool: \(sanitize(call.toolId, limit: 60))")
        }
        if let problem = validate(call, spec: spec) {
            return .refuse(problem)
        }
        let untrusted = call.origin == .untrustedContent
        let needsCard: Bool
        switch spec.risk {
        case .read:           needsCard = false
        case .draft:          needsCard = untrusted
        case .act, .critical: needsCard = true
        }
        if needsCard {
            let shown = Set(spec.fields.map { $0.argument })
            for key in call.arguments.keys.sorted() where !shown.contains(key) {
                return .refuse("\"\(sanitize(key, limit: 40))\" would not be shown on the card for \(spec.id).")
            }
        }
        switch spec.risk {
        case .read:
            return .run
        case .draft:
            return untrusted ? .askUser(makeCard(spec, call, secondClick: false, timeZone: timeZone)) : .run
        case .act:
            return .askUser(makeCard(spec, call, secondClick: false, timeZone: timeZone))
        case .critical:
            return .askUser(makeCard(spec, call, secondClick: true, timeZone: timeZone))
        }
    }

    /// Returns a plain-words problem, or nil when the arguments match the spec exactly.
    static func validate(_ call: ToolCall, spec: ToolSpec) -> String? {
        let allowed = Set(spec.required + spec.optional)
        for key in call.arguments.keys.sorted() where !allowed.contains(key) {
            return "Unexpected argument \"\(sanitize(key, limit: 40))\" for \(spec.id)."
        }
        for key in call.arguments.keys.sorted() {
            if (call.arguments[key] ?? "").utf8.count > maxArgumentBytes {
                return "\"\(key)\" is too long for \(spec.id)."
            }
        }
        for key in spec.required {
            let value = stripHidden(call.arguments[key] ?? "", keepLineBreaks: false)
            if isVisiblyEmpty(value) { return "Missing \"\(key)\" for \(spec.id)." }
        }
        return nil
    }

    static func makeCard(_ spec: ToolSpec, _ call: ToolCall, secondClick: Bool, timeZone: TimeZone = .current) -> ConfirmationCard {
        var lines: [ConfirmationLine] = []
        for field in spec.fields {
            guard let raw = call.arguments[field.argument] else { continue }
            let value: String
            let hidden: Int
            if field.isDateTime, let local = localDateTime(fromISO: raw, timeZone: timeZone) {
                value = local
                hidden = 0
            } else {
                let cut = truncate(sanitize(raw, limit: Int.max), limit: maxDisplayLength)
                value = cut.text
                hidden = cut.hidden
            }
            if value.isEmpty { continue }
            lines.append(ConfirmationLine(label: field.label, value: value, isContent: field.isContent, hiddenCharacters: hidden))
        }
        return ConfirmationCard(
            title: spec.title,
            risk: spec.risk,
            lines: lines,
            fromUntrustedContent: call.origin == .untrustedContent,
            needsSecondClick: secondClick
        )
    }

    /// An ISO 8601 moment written the way the user will read it, in their own time zone, for example
    /// "Wed 7 Oct 2026, 16:00 (GMT+1)". Nil when the text is not a valid ISO 8601 moment.
    static func localDateTime(fromISO iso: String, timeZone: TimeZone) -> String? {
        guard let date = ReminderParser.parseISO8601(iso) else { return nil }
        return localDateTime(date, timeZone: timeZone)
    }

    /// A moment written the way the user will read it, in their own time zone.
    static func localDateTime(_ date: Date, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "EEE d MMM yyyy, HH:mm"
        let zone = timeZone.abbreviation(for: date) ?? timeZone.identifier
        return formatter.string(from: date) + " (" + zone + ")"
    }

    // MARK: Display safety

    /// Removes characters that change how text looks, or hide text, without showing anything:
    /// control characters, format characters (bidirectional overrides, zero-width spaces, soft hyphens, tag
    /// characters and the like), private-use characters, and combining marks beyond `maxCombiningMarks` in a row.
    /// The zero-width joiner and non-joiner stay, because emoji sequences and some scripts need them.
    /// Line breaks and tabs are kept when `keepLineBreaks` is true. Otherwise they become spaces.
    static func stripHidden(_ text: String, keepLineBreaks: Bool) -> String {
        var out = String.UnicodeScalarView()
        var markRun = 0
        for scalar in text.unicodeScalars {
            let category = scalar.properties.generalCategory
            switch category {
            case .nonspacingMark, .enclosingMark, .spacingMark:
                markRun += 1
                if markRun <= maxCombiningMarks { out.append(scalar) }
                continue
            default:
                markRun = 0
            }
            switch category {
            case .control:
                if scalar == "\n" || scalar == "\t" {
                    let kept: Unicode.Scalar = keepLineBreaks ? scalar : " "
                    out.append(kept)
                }
            case .format:
                if scalar.value == 0x200C || scalar.value == 0x200D { out.append(scalar) }
            case .lineSeparator, .paragraphSeparator:
                let kept: Unicode.Scalar = keepLineBreaks ? "\n" : " "
                out.append(kept)
            case .privateUse, .surrogate:
                break
            default:
                out.append(scalar)
            }
        }
        return String(out)
    }

    /// True when nothing a person could see is left: only spaces, line breaks and joiners.
    static func isVisiblyEmpty(_ text: String) -> Bool {
        text.unicodeScalars.allSatisfy { $0.properties.isWhitespace || $0.value == 0x200C || $0.value == 0x200D }
    }

    /// Makes text safe to show on a card or write to a log: hidden characters are removed, line breaks become a
    /// visible marker, tabs become spaces, and long text is cut with an ellipsis. The limit counts Unicode
    /// scalars, so combining marks cannot be used to get past it.
    static func sanitize(_ text: String, limit: Int) -> String {
        let cleaned = stripHidden(text, keepLineBreaks: true)
        var out = String.UnicodeScalarView()
        for scalar in cleaned.unicodeScalars {
            if scalar == "\n" {
                out.append(contentsOf: " ⏎ ".unicodeScalars)
            } else if scalar == "\t" {
                out.append(" ")
            } else {
                out.append(scalar)
            }
        }
        let trimmed = String(out).trimmingCharacters(in: .whitespaces)
        return truncate(trimmed, limit: limit).text
    }

    /// Cuts text to `limit` Unicode scalars and adds an ellipsis. `hidden` is how many scalars were cut.
    private static func truncate(_ text: String, limit: Int) -> (text: String, hidden: Int) {
        let count = text.unicodeScalars.count
        if count <= limit { return (text, 0) }
        let kept = String(String.UnicodeScalarView(text.unicodeScalars.prefix(limit)))
        return (kept + "…", count - limit)
    }
}
