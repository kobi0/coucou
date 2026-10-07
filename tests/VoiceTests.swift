import Foundation

@main
enum VoiceTests {

    static var failures = 0

    static func check(_ label: String, _ got: String, _ expected: String) {
        if got == expected {
            print("  ✓ \(label)")
        } else {
            print("  ✗ \(label)")
            print("    got:      \(got.debugDescription)")
            print("    expected: \(expected.debugDescription)")
            failures += 1
        }
    }

    static func checkTrue(_ label: String, _ value: Bool) {
        if value { print("  ✓ \(label)") }
        else      { print("  ✗ \(label)"); failures += 1 }
    }

    static let t0 = Date(timeIntervalSince1970: 1_800_000_000)

    static func main() {
        languages()
        transcripts()
        sessions()
        speakable()
        print("")
        if failures == 0 {
            print("All voice checks passed.")
        } else {
            print("\(failures) check(s) failed.")
            exit(1)
        }
    }

    // MARK: Languages and wording

    static func languages() {
        print("Languages and wording")
        checkTrue("english code", VoiceLanguage(rawValue: "en") == .english)
        checkTrue("pidgin code", VoiceLanguage(rawValue: "pcm") == .pidgin)
        checkTrue("two voices", VoiceLanguage.allCases.count == 2)
        for language in VoiceLanguage.allCases {
            for phrase in VoicePhrase.allCases {
                checkTrue("\(language.rawValue) has \(phrase.rawValue)", !language.phrase(phrase).isEmpty)
            }
        }
        for phrase in VoicePhrase.allCases {
            checkTrue("pidgin \(phrase.rawValue) is its own wording",
                      VoiceLanguage.pidgin.phrase(phrase) != VoiceLanguage.english.phrase(phrase))
        }
        check("english adds nothing to the prompt", VoicePersona.stylePrompt(.english), "")
        let pidgin = VoicePersona.stylePrompt(.pidgin)
        checkTrue("pidgin prompt names the language", pidgin.contains("Nigerian Pidgin"))
        checkTrue("pidgin prompt keeps tool arguments standard", pidgin.contains("ISO 8601"))
        checkTrue("read-aloud prompt asks for short replies", VoicePersona.spokenStyle.contains("read aloud"))
        checkTrue("recogniser and voice locales are listed",
                  !VoiceLanguage.english.recognizerLocales.isEmpty && !VoiceLanguage.pidgin.voiceLocales.isEmpty)
    }

    // MARK: Transcripts

    static func transcripts() {
        print("Cleaning what was heard")
        check("hidden characters are removed", VoiceTranscript.clean("he\u{202E}llo") ?? "nil", "hello")
        check("spaces and line breaks collapse", VoiceTranscript.clean("  a \n\t b  ") ?? "nil", "a b")
        checkTrue("only spaces is nothing", VoiceTranscript.clean("   ") == nil)
        checkTrue("only a zero-width space is nothing", VoiceTranscript.clean("\u{200B}") == nil)
        check("long text is cut", VoiceTranscript.clean(String(repeating: "a b ", count: 10), limit: 7) ?? "nil", "a b a b")
    }

    // MARK: The push-to-talk machine

    static func sessions() {
        print("Push to talk")
        var session = VoiceSession()
        checkTrue("starts idle", session.state == .idle)
        checkTrue("release with nothing held does nothing", session.release(now: t0).isEmpty)

        checkTrue("pressing starts listening", session.press(now: t0) == [.startListening])
        checkTrue("now listening", session.state == .listening)
        checkTrue("pressing again does nothing", session.press(now: t0).isEmpty)

        checkTrue("a quick tap is dropped", session.release(now: t0.addingTimeInterval(0.2)) == [.discard])
        checkTrue("and goes back to idle", session.state == .idle)

        _ = session.press(now: t0)
        checkTrue("a real hold stops listening", session.release(now: t0.addingTimeInterval(1)) == [.stopListening])
        checkTrue("and waits for the words", session.state == .finishing)
        checkTrue("the words are cleaned and sent",
                  session.finished(transcript: "  hello   world \n") == [.send("hello world")])
        checkTrue("back to idle", session.state == .idle)
        checkTrue("a stray result while idle is ignored", session.finished(transcript: "hi").isEmpty)

        _ = session.press(now: t0)
        _ = session.release(now: t0.addingTimeInterval(1))
        checkTrue("nothing heard gives a hint", session.finished(transcript: nil) == [.hint(.didNotCatch)])
        _ = session.press(now: t0)
        _ = session.release(now: t0.addingTimeInterval(1))
        checkTrue("only hidden characters also gives a hint", session.finished(transcript: "\u{200B}") == [.hint(.didNotCatch)])

        _ = session.press(now: t0)
        checkTrue("no time limit hit at 59 seconds", session.tick(now: t0.addingTimeInterval(59)).isEmpty)
        checkTrue("listening stops by itself at 60 seconds", session.tick(now: t0.addingTimeInterval(60)) == [.stopListening])
        checkTrue("and waits for the words", session.state == .finishing)
        _ = session.finished(transcript: "ok")

        _ = session.press(now: t0)
        checkTrue("the recogniser can finish early", session.finished(transcript: "hi") == [.stopListening, .send("hi")])
        checkTrue("and then the button release does nothing", session.release(now: t0.addingTimeInterval(2)).isEmpty)

        _ = session.press(now: t0)
        checkTrue("a blocked microphone says so", session.failed(.micBlocked) == [.discard, .hint(.micBlocked)])
        checkTrue("and goes back to idle", session.state == .idle)
        _ = session.press(now: t0)
        checkTrue("no on-device recogniser says so", session.failed(.unavailable) == [.discard, .hint(.needsOnDevice)])
        _ = session.press(now: t0)
        checkTrue("any other failure asks to try again", session.failed(.other) == [.discard, .hint(.didNotCatch)])
        checkTrue("a failure while idle does nothing", session.failed(.other).isEmpty)

        _ = session.press(now: t0)
        session.heard(partial: " he\u{202E}llo ")
        check("words so far are cleaned", session.partial, "hello")
        checkTrue("cancel drops it", session.cancel() == [.discard])
        check("and clears the words", session.partial, "")
        session.heard(partial: "late")
        check("words that arrive while idle are ignored", session.partial, "")
        checkTrue("cancel while idle does nothing", session.cancel().isEmpty)
    }

    // MARK: Text for reading aloud

    static func speakable() {
        print("Text for reading aloud")
        let a = SpeakableText.make(from: "Hello **world**! Here is a [link](https://x.com/a) and https://y.com/b too.")
        check("marks and links", a?.text ?? "nil", "Hello world! Here is a link and too.")
        checkTrue("not cut", a?.truncated == false)

        let b = SpeakableText.make(from: "Run this:\n```bash\nls -la\n```\nThen relax")
        check("code blocks stay on the screen", b?.text ?? "nil", "Run this: Then relax.")

        let c = SpeakableText.make(from: "# Plan\n- one\n- two\n1. three")
        check("headings and lists become sentences", c?.text ?? "nil", "Plan. one. two. three.")

        let d = SpeakableText.make(from: "Write to `ada@example.com` now")
        check("an email address is kept", d?.text ?? "nil", "Write to ada@example.com now.")

        checkTrue("only code is nothing to say", SpeakableText.make(from: "```\ncode\n```") == nil)
        checkTrue("an empty reply is nothing to say", SpeakableText.make(from: "  \n ") == nil)

        let sentences = "First sentence here. Second sentence here. Third sentence here. Fourth one."
        let cutAtSentence = SpeakableText.make(from: sentences, limit: 45)
        check("a long reply is cut at a sentence", cutAtSentence?.text ?? "nil", "First sentence here. Second sentence here.")
        checkTrue("and marked as cut", cutAtSentence?.truncated == true)
        let earlier = SpeakableText.make(from: sentences, limit: 40)
        check("a shorter limit cuts earlier", earlier?.text ?? "nil", "First sentence here.")

        let noSentence = SpeakableText.make(from: "alpha beta gamma delta epsilon", limit: 14)
        check("with no sentence end it cuts at a word", noSentence?.text ?? "nil", "alpha beta")
        checkTrue("and is marked as cut", noSentence?.truncated == true)

        let hidden = SpeakableText.make(from: "Hi\u{202E} there")
        check("hidden characters are removed", hidden?.text ?? "nil", "Hi there.")
    }
}
