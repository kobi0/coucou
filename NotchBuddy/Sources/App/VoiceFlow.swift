import AppKit
import SwiftUI

// MARK: - Voice glue (Mac)
//
// Connects the push-to-talk state machine to the microphone, the chat and the speaker. What is heard becomes
// ordinary chat text from the person. It cannot approve an action: that is only ever a click on the card.

@MainActor
enum VoiceFlow {
    private static var tickTimer: Timer?

    /// Voice input needs the two usage descriptions in the app's Info.plist. Without them macOS ends the app
    /// the moment it asks for the microphone, so the mic button is only offered when both are there.
    static var inputAvailable: Bool {
        let info = Bundle.main.infoDictionary ?? [:]
        return info["NSMicrophoneUsageDescription"] != nil && info["NSSpeechRecognitionUsageDescription"] != nil
    }

    // MARK: Hold to talk

    static func press(state: AppState) {
        run(state.voice.press(now: Date()), state: state)
    }

    static func release(state: AppState) {
        run(state.voice.release(now: Date()), state: state)
    }

    private static func run(_ commands: [VoiceCommand], state: AppState) {
        for command in commands {
            switch command {
            case .startListening:
                // Never listen to our own voice.
                SpeechOutputMac.shared.stop()
                startTicking(state: state)
                SpeechInputMac.shared.start(
                    language: state.assistantLanguage,
                    onPartial: { text in state.voice.heard(partial: text) },
                    onFinished: { result in handle(result, state: state) }
                )
            case .stopListening:
                SpeechInputMac.shared.stop()
            case .discard:
                stopTicking()
                SpeechInputMac.shared.cancel()
            case .send(let text):
                stopTicking()
                deliver(text, state: state)
            case .hint(let phrase):
                stopTicking()
                hint(phrase, state: state)
            }
        }
    }

    private static func handle(_ result: SpeechInputResult, state: AppState) {
        switch result {
        case .transcript(let text): run(state.voice.finished(transcript: text), state: state)
        case .nothingHeard:         run(state.voice.finished(transcript: nil), state: state)
        case .blocked:              run(state.voice.failed(.micBlocked), state: state)
        case .unavailable:          run(state.voice.failed(.unavailable), state: state)
        case .failed:               run(state.voice.failed(.other), state: state)
        }
    }

    private static func startTicking(state: AppState) {
        tickTimer?.invalidate()
        tickTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
            MainActor.assumeIsolated { run(state.voice.tick(now: Date()), state: state) }
        }
    }

    private static func stopTicking() {
        tickTimer?.invalidate()
        tickTimer = nil
    }

    // MARK: What was heard

    /// The words become the person's message, in the chat where they can be read. With "send straight away"
    /// off they land in the text box instead, to be checked first.
    private static func deliver(_ text: String, state: AppState) {
        guard state.view == .prompt else { return }
        if state.voiceAutoSend {
            state.chatHistory.append(ChatMessage(role: .user, content: text))
            state.stateOverride = .thinking
            Task { await ClaudeService.shared.chat(query: text, context: state.promptContext, state: state) }
        } else {
            state.voiceDraft = text
        }
    }

    /// A short app-written line, always shown and also spoken when replies are read aloud.
    private static func hint(_ phrase: VoicePhrase, state: AppState) {
        let line = state.assistantLanguage.phrase(phrase)
        AssistantFlow.say(line, state: state)
        speakPhrase(phrase, state: state, interrupting: true)
    }

    // MARK: Speaking

    /// Reads a chat reply aloud when "Read replies aloud" is on. The text on screen is not changed.
    static func speakReply(_ text: String, state: AppState) {
        guard state.speakReplies, let spoken = SpeakableText.make(from: text) else { return }
        let language = state.assistantLanguage
        let full = spoken.truncated ? spoken.text + " " + language.phrase(.restOnScreen) : spoken.text
        SpeechOutputMac.shared.speak(full, language: language, interrupting: true)
    }

    /// Says one of the app's fixed lines when replies are read aloud.
    static func speakPhrase(_ phrase: VoicePhrase, state: AppState, interrupting: Bool = false) {
        guard state.speakReplies else { return }
        SpeechOutputMac.shared.speak(state.assistantLanguage.phrase(phrase), language: state.assistantLanguage,
                                     interrupting: interrupting)
    }
}

// MARK: - The mic button

/// Hold to talk. Listening starts when pressed and ends when let go.
struct MicHoldButton: View {
    @ObservedObject var state: AppState
    @State private var pressing = false

    var body: some View {
        let listening = state.voice.state == .listening
        Image(systemName: "mic.fill")
            .font(.system(size: 11, weight: .semibold))
            .foregroundColor(listening ? Color(hex: "#F4505E") : Color(hex: "#9AA0AA"))
            .frame(width: 22, height: 22)
            .background(Color.white.opacity(listening ? 0.16 : 0.07))
            .clipShape(Circle())
            .contentShape(Circle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !pressing {
                            pressing = true
                            VoiceFlow.press(state: state)
                        }
                    }
                    .onEnded { _ in
                        pressing = false
                        VoiceFlow.release(state: state)
                    }
            )
            .help("Hold to talk")
            .accessibilityLabel("Hold to talk")
    }
}
