import Foundation

// MARK: - Speech in and out, behind protocols (Foundation-only)
//
// The app talks to these, not to AVFoundation or the Speech framework directly, so the Mac and the iPhone can
// each plug in their own, and tests can plug in a fake.

@MainActor
protocol SpeechOutput: AnyObject {
    var isSpeaking: Bool { get }
    /// Reads `text` aloud. `interrupting` true stops anything being said first. False queues after it.
    func speak(_ text: String, language: VoiceLanguage, interrupting: Bool)
    func stop()
}

enum SpeechInputResult: Sendable, Equatable {
    case transcript(String)
    case nothingHeard
    /// Microphone or speech recognition permission was refused.
    case blocked
    /// No recogniser that works on this device without sending audio away.
    case unavailable
    case failed
}

@MainActor
protocol SpeechInput: AnyObject {
    /// Starts hearing. `onPartial` gets the words so far. `onFinished` is called exactly once, unless `cancel`
    /// is called first.
    func start(language: VoiceLanguage,
               onPartial: @escaping @MainActor (String) -> Void,
               onFinished: @escaping @MainActor (SpeechInputResult) -> Void)
    /// Stops recording and delivers the final words through `onFinished`.
    func stop()
    /// Stops recording and drops everything. `onFinished` is not called.
    func cancel()
}
