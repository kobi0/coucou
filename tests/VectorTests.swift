import Foundation

// Runs the shared test vectors in tests/vectors/*.json against the Swift core. The Rust core
// (windows/assistant/tests/vectors.rs) and the TypeScript for reading aloud (windows/scripts/test-vectors.mjs)
// run the same files, so a rule that drifts in one place fails there. Run from the repository root.

@main
enum VectorTests {

    static var failures = 0
    static var passed = 0

    static func check(_ label: String, _ got: String, _ expected: String) {
        if got == expected {
            passed += 1
        } else {
            print("  ✗ \(label)")
            print("    got:      \(got.debugDescription)")
            print("    expected: \(expected.debugDescription)")
            failures += 1
        }
    }

    static func load(_ name: String) -> [String: Any] {
        let url = URL(fileURLWithPath: "tests/vectors/\(name)")
        guard let data = try? Data(contentsOf: url),
              let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
            print("  ✗ cannot read tests/vectors/\(name) (run this from the repository root)")
            exit(1)
        }
        return object
    }

    static func cases(_ file: [String: Any], _ key: String) -> [[String: Any]] {
        guard let list = file[key] as? [[String: Any]], !list.isEmpty else {
            print("  ✗ no \(key) in the vectors")
            exit(1)
        }
        return list
    }

    static func main() {
        stripHiddenVectors()
        transcriptVectors()
        speakableVectors()
        sessionVectors()
        print("\(passed) vector checks passed, \(failures) failed.")
        if failures > 0 { exit(1) }
    }

    // MARK: Hidden characters

    static func stripHiddenVectors() {
        for c in cases(load("strip_hidden.json"), "cases") {
            let name = c["name"] as? String ?? "?"
            let keep = c["keepLineBreaks"] as? Bool ?? true
            let got = ActionPolicy.stripHidden(c["input"] as? String ?? "", keepLineBreaks: keep)
            check("stripHidden: \(name)", got, c["expected"] as? String ?? "")
        }
    }

    // MARK: Transcripts

    static func transcriptVectors() {
        for c in cases(load("voice.json"), "transcript") {
            let name = c["name"] as? String ?? "?"
            let input = c["input"] as? String ?? ""
            let got: String?
            if let limit = c["limit"] as? Int {
                got = VoiceTranscript.clean(input, limit: limit)
            } else {
                got = VoiceTranscript.clean(input)
            }
            check("transcript: \(name)", got ?? "<nothing>", (c["expected"] as? String) ?? "<nothing>")
        }
    }

    // MARK: Reading aloud

    static func speakableVectors() {
        for c in cases(load("voice.json"), "speakable") {
            let name = c["name"] as? String ?? "?"
            let input = c["input"] as? String ?? ""
            let spoken = SpeakableText.make(from: input, limit: (c["limit"] as? Int) ?? SpeakableText.defaultLimit)
            let got = spoken.map { "\($0.text)|\($0.truncated)" } ?? "<nothing>"
            var want = "<nothing>"
            if let expected = c["expected"] as? [String: Any],
               let text = expected["text"] as? String, let truncated = expected["truncated"] as? Bool {
                want = "\(text)|\(truncated)"
            }
            check("speakable: \(name)", got, want)
        }
    }

    // MARK: Push to talk

    static func commandName(_ command: VoiceCommand) -> String {
        switch command {
        case .startListening: return "startListening"
        case .stopListening: return "stopListening"
        case .discard: return "discard"
        case .send(let words): return "send:\(words)"
        case .hint(let phrase): return "hint:\(phrase.rawValue)"
        }
    }

    static func stateName(_ state: VoiceState) -> String {
        switch state {
        case .idle: return "idle"
        case .listening: return "listening"
        case .finishing: return "finishing"
        }
    }

    static func sessionVectors() {
        let base = Date(timeIntervalSince1970: 1_800_000_000)
        for script in cases(load("voice.json"), "sessions") {
            let name = script["name"] as? String ?? "?"
            var session = VoiceSession()
            let steps = script["steps"] as? [[String: Any]] ?? []
            for (index, step) in steps.enumerated() {
                let op = step["op"] as? String ?? "?"
                let now = base.addingTimeInterval((step["at"] as? Double) ?? 0)
                var commands: [VoiceCommand]? = nil
                switch op {
                case "press": commands = session.press(now: now)
                case "release": commands = session.release(now: now)
                case "tick": commands = session.tick(now: now)
                case "finished": commands = session.finished(transcript: step["transcript"] as? String)
                case "cancel": commands = session.cancel()
                case "heard": session.heard(partial: step["text"] as? String ?? "")
                case "failed":
                    switch step["kind"] as? String ?? "" {
                    case "micBlocked": commands = session.failed(.micBlocked)
                    case "unavailable": commands = session.failed(.unavailable)
                    default: commands = session.failed(.other)
                    }
                default:
                    print("  ✗ \(name): unknown op \(op)")
                    failures += 1
                }
                let label = "session: \(name), step \(index + 1) (\(op))"
                if let commands, let expected = step["expect"] as? [String] {
                    check(label, commands.map(commandName).joined(separator: ", "), expected.joined(separator: ", "))
                }
                if let state = step["state"] as? String {
                    check("\(label): state", stateName(session.state), state)
                }
                if let partial = step["partial"] as? String {
                    check("\(label): partial", session.partial, partial)
                }
            }
        }
    }
}
