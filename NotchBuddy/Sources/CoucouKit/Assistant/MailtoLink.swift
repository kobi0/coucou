import Foundation

// MARK: - mailto link for a draft (Foundation-only, testable)
//
// Builds the link that opens a draft in the user's mail app. Opening it sends nothing. Hidden characters are
// removed from the subject and message first, so what goes into the draft matches what the card could show.
// Every character other than letters, digits and "-._~" is percent-encoded, so nothing in the text can add a
// second recipient or a header.

extension MailDraftRequest {
    /// Longest link built. Some mail apps fail on longer ones, so a longer message is refused instead of cut.
    static let maxLinkLength = 6_000

    func mailtoURL() -> URL? {
        let safeAddress = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@._-+")
        guard let address = to.addingPercentEncoding(withAllowedCharacters: safeAddress), address == to else { return nil }

        let cleanSubject = ActionPolicy.stripHidden(subject, keepLineBreaks: false)
            .trimmingCharacters(in: .whitespaces)
        let cleanBody = ActionPolicy.stripHidden(body, keepLineBreaks: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanSubject.isEmpty, !cleanBody.isEmpty else { return nil }

        let unreserved = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~")
        let lineBreaksAsCRLF = cleanBody
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
            .replacingOccurrences(of: "\n", with: "\r\n")
        guard let encodedSubject = cleanSubject.addingPercentEncoding(withAllowedCharacters: unreserved),
              let encodedBody = lineBreaksAsCRLF.addingPercentEncoding(withAllowedCharacters: unreserved) else { return nil }

        let text = "mailto:\(address)?subject=\(encodedSubject)&body=\(encodedBody)"
        if text.count > Self.maxLinkLength { return nil }
        return URL(string: text)
    }
}
