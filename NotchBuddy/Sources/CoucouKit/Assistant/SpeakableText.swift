import Foundation

// MARK: - Turning a chat reply into something that can be read aloud (Foundation-only, testable)
//
// Code blocks, links, headings and list marks sound wrong when read out, so they are taken out or smoothed.
// The text on screen is never changed. Long replies are cut at a sentence, and the app adds a fixed line
// saying the rest is on the screen.

struct SpokenText: Sendable, Equatable {
    let text: String
    /// True when the reply was cut. The app adds "The rest is on the screen."
    let truncated: Bool
}

enum SpeakableText {
    static let defaultLimit = 600

    static func make(from markdown: String, limit: Int = defaultLimit) -> SpokenText? {
        var text = ActionPolicy.stripHidden(markdown, keepLineBreaks: true)
        let steps: [(pattern: String, template: String)] = [
            ("```[\\s\\S]*?```", " "),                           // code blocks are on the screen
            ("!\\[[^\\]]*\\]\\([^)]*\\)", " "),                   // images
            ("\\[([^\\]]*)\\]\\([^)]*\\)", "$1"),                 // links keep their words
            ("https?://\\S+", " "),                              // bare links
            ("`+", ""),                                          // inline code marks
            ("(?m)^[ \\t]*#{1,6}[ \\t]*", ""),                   // headings
            ("(?m)^[ \\t]*(?:[-*•]|\\d+[.)])[ \\t]+", ""),       // list marks
            ("[*~]+", "")                                        // bold, italic, strike marks
        ]
        for step in steps {
            text = text.replacingOccurrences(of: step.pattern, with: step.template, options: .regularExpression)
        }

        // One sentence per line, so list items do not run together.
        var sentences: [String] = []
        for rawLine in text.split(whereSeparator: { $0.isNewline }) {
            let line = rawLine.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
            if line.isEmpty { continue }
            if let last = line.last, ".!?:;,…".contains(last) {
                sentences.append(line)
            } else {
                sentences.append(line + ".")
            }
        }
        let joined = sentences.joined(separator: " ")
        if joined.isEmpty { return nil }
        if joined.unicodeScalars.count <= limit { return SpokenText(text: joined, truncated: false) }

        let head = String(String.UnicodeScalarView(joined.unicodeScalars.prefix(limit)))
        var cut: String.Index? = nil
        for terminator in [". ", "! ", "? "] {
            if let range = head.range(of: terminator, options: .backwards) {
                if let current = cut {
                    if range.lowerBound > current { cut = range.lowerBound }
                } else {
                    cut = range.lowerBound
                }
            }
        }
        var result = head
        if let end = cut, head.distance(from: head.startIndex, to: end) >= limit / 3 {
            result = String(head[...end])
        } else if let space = head.range(of: " ", options: .backwards) {
            result = String(head[..<space.lowerBound])
        }
        let trimmed = result.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : SpokenText(text: trimmed, truncated: true)
    }
}
