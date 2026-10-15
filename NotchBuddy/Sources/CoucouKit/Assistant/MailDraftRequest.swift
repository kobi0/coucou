import Foundation

// MARK: - Mail draft request (Foundation-only, testable)
//
// Turns the structured arguments of "mail.draft" into a checked request. The app only ever opens a draft for
// the user to review; sending stays a separate step behind a click. Whatever code builds the draft (Mail.app
// through AppleScript, or the system compose sheet) must still escape these values for its own syntax. This
// parser makes sure there is nothing in them that could add recipients or headers.

struct MailDraftRequest: Sendable, Equatable {
    let to: String
    let subject: String
    let body: String
}

enum MailDraftParseError: Error, Sendable, Equatable {
    case badRecipient
    case missingSubject
    case badSubject
    case subjectTooLong
    case missingBody

    var message: String {
        switch self {
        case .badRecipient:   return "I need one valid email address to write to."
        case .missingSubject: return "The email needs a subject."
        case .badSubject:     return "The subject must be a single line."
        case .subjectTooLong: return "The subject is too long."
        case .missingBody:    return "The email needs a message."
        }
    }
}

enum MailDraftParser {
    static let maxSubjectLength = 200

    static func parse(_ arguments: ToolArguments) -> Result<MailDraftRequest, MailDraftParseError> {
        let to = (arguments["to"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard isPlausibleAddress(to) else { return .failure(.badRecipient) }

        let subject = (arguments["subject"] ?? "").trimmingCharacters(in: .whitespaces)
        if subject.isEmpty { return .failure(.missingSubject) }
        // A line break in a subject is how extra headers get injected. Refuse it instead of cleaning it.
        if subject.unicodeScalars.contains(where: { $0.properties.generalCategory == .control }) {
            return .failure(.badSubject)
        }
        if subject.count > maxSubjectLength { return .failure(.subjectTooLong) }

        let body = (arguments["body"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if body.isEmpty { return .failure(.missingBody) }

        return .success(MailDraftRequest(to: to, subject: subject, body: body))
    }

    /// One plain ASCII address: no display name, no list, no angle brackets, no spaces or control characters.
    /// Non-ASCII addresses are refused for now because look-alike letters can disguise the real domain.
    static func isPlausibleAddress(_ text: String) -> Bool {
        guard !text.isEmpty, text.count <= 254 else { return false }
        let ascii = text.unicodeScalars.allSatisfy { $0.value > 32 && $0.value < 127 }
        if !ascii { return false }
        let forbidden: Set<Character> = [",", ";", "<", ">", "\"", "(", ")", "[", "]", "\\", ":"]
        if text.contains(where: { forbidden.contains($0) }) { return false }

        let parts = text.split(separator: "@", omittingEmptySubsequences: false)
        guard parts.count == 2 else { return false }
        let local = String(parts[0])
        let domain = String(parts[1])
        guard !local.isEmpty, local.count <= 64 else { return false }
        if local.hasPrefix(".") || local.hasSuffix(".") || local.contains("..") { return false }

        let labels = domain.split(separator: ".", omittingEmptySubsequences: false)
        guard labels.count >= 2 else { return false }
        for label in labels {
            if label.isEmpty || label.count > 63 { return false }
            if label.hasPrefix("-") || label.hasSuffix("-") { return false }
            let allowed = label.unicodeScalars.allSatisfy {
                ($0.value >= 48 && $0.value <= 57) || ($0.value >= 65 && $0.value <= 90)
                    || ($0.value >= 97 && $0.value <= 122) || $0.value == 45
            }
            if !allowed { return false }
        }
        guard let tld = labels.last, tld.count >= 2 else { return false }
        return tld.unicodeScalars.allSatisfy { ($0.value >= 65 && $0.value <= 90) || ($0.value >= 97 && $0.value <= 122) }
    }
}
