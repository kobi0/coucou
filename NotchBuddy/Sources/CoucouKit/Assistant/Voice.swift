import Foundation

// MARK: - Voice: languages, wording and persona (Foundation-only, testable)
//
// Two assistant voices: English and Nigerian Pidgin. Everything the app itself says aloud or shows as a spoken
// hint comes from the fixed phrases below, never from the model. The persona text only changes how the model
// writes its replies. It does not change what a tool call looks like: tool arguments stay in standard form
// (full email addresses, ISO 8601 times) whatever language is spoken.
//
// Pidgin note: the wording below is a first draft. A native speaker should review it before release, and the
// Mac has no Pidgin voice or recogniser, so Pidgin is read by an English voice and heard by an English
// recogniser.

enum VoiceLanguage: String, CaseIterable, Sendable, Equatable, Hashable {
    case english = "en"
    case pidgin = "pcm"

    var displayName: String {
        switch self {
        case .english: return "English"
        case .pidgin:  return "Nigerian Pidgin"
        }
    }

    /// Locales tried in order when hearing the user.
    var recognizerLocales: [String] { ["en-NG", "en-GB", "en-US"] }

    /// Locales tried in order when choosing a voice to read replies.
    var voiceLocales: [String] { ["en-NG", "en-GB", "en-US"] }

    func phrase(_ phrase: VoicePhrase) -> String {
        switch self {
        case .english:
            switch phrase {
            case .listening:     return "Listening…"
            case .didNotCatch:   return "Sorry, I did not catch that. Please try again."
            case .checkTheCard:  return "I have prepared it. Please check the card and press Allow if it is right."
            case .micBlocked:    return "I cannot hear you yet. Please allow the microphone in System Settings."
            case .needsOnDevice: return "Voice needs speech recognition that works on this Mac. Please turn on Dictation in System Settings."
            case .restOnScreen:  return "The rest is on the screen."
            }
        case .pidgin:
            switch phrase {
            case .listening:     return "I dey listen…"
            case .didNotCatch:   return "Sorry, I no hear you well. Abeg try again."
            case .checkTheCard:  return "I don prepare am. Abeg check di card, then press Allow if e correct."
            case .micBlocked:    return "I no fit hear you yet. Abeg allow di microphone for System Settings."
            case .needsOnDevice: return "Voice need speech recognition wey dey work for dis Mac. Abeg turn on Dictation for System Settings."
            case .restOnScreen:  return "Di rest dey for screen."
            }
        }
    }
}

/// Lines the app says or shows by itself.
enum VoicePhrase: String, CaseIterable, Sendable, Equatable {
    case listening
    case didNotCatch
    case checkTheCard
    case micBlocked
    case needsOnDevice
    case restOnScreen
}

enum VoicePersona {
    /// Added to the system prompt. English adds nothing, so the chat reads as it always did.
    static func stylePrompt(_ language: VoiceLanguage) -> String {
        switch language {
        case .english:
            return ""
        case .pidgin:
            return " Write every reply in Nigerian Pidgin English, the way a friendly person in Lagos would talk, in plain easy words (na, dey, wetin, abeg, make, don, go, wan, sabi, no wahala, how far). Keep it clear first: do not overdo the slang and do not mock the language. Never change names, numbers, dates, times, email addresses, links or code: write those exactly as they are. When you call a tool, its arguments stay in standard form (full email addresses, ISO 8601 times) whatever language you are speaking. Answer in Pidgin even when the user writes in another language, unless they ask you to change."
        }
    }

    /// Added when replies are read aloud.
    static let spokenStyle = " Your reply will be read aloud, so keep it to a few short sentences in plain words, with no tables, code or long lists. Anything long can stay on the screen."
}
