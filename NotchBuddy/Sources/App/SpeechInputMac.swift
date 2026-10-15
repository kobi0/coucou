import AVFoundation
import Foundation
import Speech

// MARK: - Hearing the person (Mac)
//
// Push to talk only: recording starts when the mic button is pressed and ends when it is let go. Speech is
// recognised on this Mac. If no recogniser that works on-device exists for the language, voice reports
// "unavailable" instead of falling back to Apple's servers, because the app only talks to services the person
// set up. Permissions are asked the first time the button is held.

/// Audio capture and recognition. Not main-actor bound on purpose: the audio tap and the recogniser call back
/// on their own threads, and a closure made in a main-actor method would be asserted onto the main actor and
/// crash. The same shape as the iPhone dictation class.
final class MicRecognizer: @unchecked Sendable {
    enum StartResult { case started, unavailable, failed }

    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    /// The audio device is only touched once a tap is installed, so nothing reaches for the microphone before
    /// permission has been given.
    private var tapInstalled = false

    /// `onUpdate` gets the words so far and whether they are final. `onError` is called when recognition ends
    /// with an error and no words. Both may be called on any thread.
    func start(localeIDs: [String],
               onUpdate: @escaping @Sendable (String, Bool) -> Void,
               onError: @escaping @Sendable () -> Void) -> StartResult {
        guard let recognizer = Self.onDeviceRecognizer(localeIDs) else { return .unavailable }
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = true

        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        // No microphone gives a format with no sample rate, and installing a tap on it crashes.
        guard format.sampleRate > 0, format.channelCount > 0 else { return .failed }
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
        }
        tapInstalled = true
        engine.prepare()
        do {
            try engine.start()
        } catch {
            removeTap()
            return .failed
        }
        self.request = request
        task = recognizer.recognitionTask(with: request) { result, error in
            if let result {
                onUpdate(result.bestTranscription.formattedString, result.isFinal)
            } else if error != nil {
                onError()
            }
        }
        return .started
    }

    /// Stops recording and lets the recogniser deliver its final words.
    func finishAudio() {
        engine.stop()
        removeTap()
        request?.endAudio()
    }

    /// Stops everything and drops the result.
    func cancel() {
        engine.stop()
        removeTap()
        request?.endAudio()
        task?.cancel()
        request = nil
        task = nil
    }

    private func removeTap() {
        guard tapInstalled else { return }
        engine.inputNode.removeTap(onBus: 0)
        tapInstalled = false
    }

    private static func onDeviceRecognizer(_ ids: [String]) -> SFSpeechRecognizer? {
        for id in ids {
            if let recognizer = SFSpeechRecognizer(locale: Locale(identifier: id)),
               recognizer.isAvailable, recognizer.supportsOnDeviceRecognition {
                return recognizer
            }
        }
        return nil
    }
}

@MainActor
final class SpeechInputMac: SpeechInput {
    static let shared = SpeechInputMac()

    private let mic = MicRecognizer()
    private var onPartial: (@MainActor (String) -> Void)?
    private var onFinished: (@MainActor (SpeechInputResult) -> Void)?
    private var latest = ""
    /// Bumped on every start and cancel, so a late callback from an older session is ignored.
    private var generation = 0
    private var listening = false
    /// The button was let go before recording had started (the permission question was still open).
    private var stopRequested = false

    func start(language: VoiceLanguage,
               onPartial: @escaping @MainActor (String) -> Void,
               onFinished: @escaping @MainActor (SpeechInputResult) -> Void) {
        mic.cancel()
        generation += 1
        let token = generation
        self.onPartial = onPartial
        self.onFinished = onFinished
        latest = ""
        listening = false
        stopRequested = false
        Task { [weak self] in
            let allowed = await Self.authorized()
            guard let self, token == self.generation else { return }
            guard allowed else { self.finish(.blocked); return }
            if self.stopRequested { self.finish(.nothingHeard); return }
            self.begin(language: language, token: token)
        }
    }

    func stop() {
        stopRequested = true
        guard listening else { return }
        mic.finishAudio()
        let token = generation
        // If the recogniser never sends a final result, use what was heard.
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            guard let self, token == self.generation, self.onFinished != nil else { return }
            self.finish(self.latest.isEmpty ? .nothingHeard : .transcript(self.latest))
        }
    }

    func cancel() {
        generation += 1
        stopRequested = true
        listening = false
        mic.cancel()
        onPartial = nil
        onFinished = nil
    }

    private func begin(language: VoiceLanguage, token: Int) {
        let result = mic.start(
            localeIDs: language.recognizerLocales,
            onUpdate: { [weak self] text, isFinal in
                Task { @MainActor in self?.handleUpdate(text, isFinal: isFinal, token: token) }
            },
            onError: { [weak self] in
                Task { @MainActor in self?.handleError(token: token) }
            }
        )
        switch result {
        case .started:
            listening = true
            if stopRequested { stop() }
        case .unavailable:
            finish(.unavailable)
        case .failed:
            finish(.failed)
        }
    }

    private func handleUpdate(_ text: String, isFinal: Bool, token: Int) {
        guard token == generation, onFinished != nil else { return }
        latest = text
        if isFinal {
            finish(text.isEmpty ? .nothingHeard : .transcript(text))
        } else {
            onPartial?(text)
        }
    }

    private func handleError(token: Int) {
        guard token == generation, onFinished != nil else { return }
        finish(latest.isEmpty ? .failed : .transcript(latest))
    }

    private func finish(_ result: SpeechInputResult) {
        listening = false
        mic.cancel()
        let done = onFinished
        onFinished = nil
        onPartial = nil
        done?(result)
    }

    /// Not main-actor bound: both permission questions call back on their own queues.
    nonisolated private static func authorized() async -> Bool {
        let speech = await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
            SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0 == .authorized) }
        }
        guard speech else { return false }
        return await AVCaptureDevice.requestAccess(for: .audio)
    }
}
