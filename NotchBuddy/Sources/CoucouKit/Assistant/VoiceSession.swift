import Foundation

// MARK: - Push-to-talk state machine (Foundation-only, testable)
//
// Listening happens only while the person holds the mic button. Nothing listens in the background, and there is
// no wake word. What was heard is text like any other text the person types: it is shown in the chat, and it can
// never approve an action. Only a click on a confirmation card approves.
//
// The machine does not touch the microphone. It says what the app should do next as `VoiceCommand`s, and the
// app reports back what the recogniser did.

enum VoiceState: Sendable, Equatable {
    case idle
    /// The button is held and the recogniser is hearing.
    case listening
    /// The button was let go. Waiting for the final words.
    case finishing
}

enum VoiceFailure: Sendable, Equatable {
    case micBlocked
    /// No speech recogniser that works on this Mac, without sending audio away.
    case unavailable
    case other
}

enum VoiceCommand: Sendable, Equatable {
    case startListening
    /// Stop recording and wait for the final words.
    case stopListening
    /// Stop recording and drop whatever was heard.
    case discard
    /// Deliver these words as the person's message.
    case send(String)
    /// Tell the person something, in the assistant's language.
    case hint(VoicePhrase)
}

/// Cleans what the recogniser heard before it becomes a message.
enum VoiceTranscript {
    static let maxLength = 2_000

    /// Removes hidden characters, collapses spaces and line breaks, and cuts to `maxLength` characters
    /// (Unicode scalars). Nil when nothing readable is left.
    static func clean(_ text: String, limit: Int = maxLength) -> String? {
        let stripped = ActionPolicy.stripHidden(text, keepLineBreaks: false)
        let collapsed = stripped.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
        if collapsed.isEmpty { return nil }
        if collapsed.unicodeScalars.count <= limit { return collapsed }
        let cut = String(String.UnicodeScalarView(collapsed.unicodeScalars.prefix(limit)))
        let trimmed = cut.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }
}

struct VoiceSession: Sendable, Equatable {
    /// A press shorter than this is an accidental tap and is dropped.
    static let minimumHold: TimeInterval = 0.4
    /// Listening stops by itself after this long.
    static let maximumListen: TimeInterval = 60

    private(set) var state: VoiceState = .idle
    /// The words heard so far while listening, cleaned. Shown where the person types.
    private(set) var partial: String = ""
    private var startedAt: Date?

    init() {}

    mutating func press(now: Date) -> [VoiceCommand] {
        guard state == .idle else { return [] }
        state = .listening
        startedAt = now
        partial = ""
        return [.startListening]
    }

    mutating func release(now: Date) -> [VoiceCommand] {
        guard state == .listening, let start = startedAt else { return [] }
        startedAt = nil
        if now.timeIntervalSince(start) < Self.minimumHold {
            state = .idle
            partial = ""
            return [.discard]
        }
        state = .finishing
        return [.stopListening]
    }

    /// Call about once a second while listening.
    mutating func tick(now: Date) -> [VoiceCommand] {
        guard state == .listening, let start = startedAt,
              now.timeIntervalSince(start) >= Self.maximumListen else { return [] }
        return release(now: now)
    }

    /// The recogniser has words so far.
    mutating func heard(partial text: String) {
        guard state != .idle else { return }
        partial = VoiceTranscript.clean(text) ?? ""
    }

    /// The recogniser is done. `transcript` is nil or empty when nothing was heard.
    mutating func finished(transcript: String?) -> [VoiceCommand] {
        guard state != .idle else { return [] }
        var commands: [VoiceCommand] = []
        if state == .listening { commands.append(.stopListening) }
        state = .idle
        startedAt = nil
        partial = ""
        if let cleaned = VoiceTranscript.clean(transcript ?? "") {
            commands.append(.send(cleaned))
        } else {
            commands.append(.hint(.didNotCatch))
        }
        return commands
    }

    /// Something went wrong while listening or finishing.
    mutating func failed(_ failure: VoiceFailure) -> [VoiceCommand] {
        guard state != .idle else { return [] }
        state = .idle
        startedAt = nil
        partial = ""
        switch failure {
        case .micBlocked:  return [.discard, .hint(.micBlocked)]
        case .unavailable: return [.discard, .hint(.needsOnDevice)]
        case .other:       return [.discard, .hint(.didNotCatch)]
        }
    }

    mutating func cancel() -> [VoiceCommand] {
        guard state != .idle else { return [] }
        state = .idle
        startedAt = nil
        partial = ""
        return [.discard]
    }
}
