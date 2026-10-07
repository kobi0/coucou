import AVFoundation
import Foundation

// MARK: - Reading replies aloud (Mac)
//
// Uses the system voices through AVSpeechSynthesizer. Nothing is sent anywhere. The first locale in
// VoiceLanguage.voiceLocales that has an installed voice is used, and the system default voice otherwise.

@MainActor
final class SpeechOutputMac: SpeechOutput {
    static let shared = SpeechOutputMac()

    private let synthesizer = AVSpeechSynthesizer()

    var isSpeaking: Bool { synthesizer.isSpeaking }

    func speak(_ text: String, language: VoiceLanguage, interrupting: Bool) {
        guard !text.isEmpty else { return }
        if interrupting { synthesizer.stopSpeaking(at: .immediate) }
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = Self.voice(for: language)
        synthesizer.speak(utterance)
    }

    func stop() {
        synthesizer.stopSpeaking(at: .immediate)
    }

    private static func voice(for language: VoiceLanguage) -> AVSpeechSynthesisVoice? {
        for id in language.voiceLocales {
            if let voice = AVSpeechSynthesisVoice(language: id) { return voice }
        }
        return nil
    }
}
