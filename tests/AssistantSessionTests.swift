import Foundation

@main
enum AssistantSessionTests {

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

    // MARK: Fixtures

    static let now = Date(timeIntervalSince1970: 1_800_000_000)

    static func iso(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.string(from: date)
    }

    static func toolUse(_ id: String, _ name: String, _ input: [String: Any]) -> [String: Any] {
        ["type": "tool_use", "id": id, "name": name, "input": input]
    }

    static var webResult: [String: Any] {
        ["type": "web_search_tool_result", "tool_use_id": "srv1", "content": [[String: Any]]()]
    }

    static func reminderTurn(_ id: String = "t1", title: String = "Call Ada") -> ExtractedTurn {
        ToolCallExtractor.extract(content: [
            toolUse(id, "reminder_create", ["title": title, "due": iso(now.addingTimeInterval(3600))])
        ])
    }

    static func mailTurn(_ id: String = "m1", outside: Bool = false) -> ExtractedTurn {
        var blocks: [[String: Any]] = []
        if outside { blocks.append(webResult) }
        blocks.append(toolUse(id, "mail_draft", ["to": "ada@example.com", "subject": "Hello", "body": "Hi Ada"]))
        return ToolCallExtractor.extract(content: blocks)
    }

    // MARK: Entry point

    static func main() {
        schemas()
        extractor()
        sessionBasics()
        untrustedContent()
        feedbackToTheModel()
        endingAConversation()
        mailtoLinks()
        print("")
        if failures == 0 {
            print("All assistant session checks passed.")
        } else {
            print("\(failures) check(s) failed.")
            exit(1)
        }
    }

    // MARK: Schemas

    static func schemas() {
        print("Tool schemas")
        let tools = ToolSchemas.anthropicTools(ToolCatalog.all)
        checkTrue("one entry per tool", tools.count == ToolCatalog.all.count)
        var names: [String] = []
        for tool in tools {
            let name = (tool["name"] as? String) ?? ""
            names.append(name)
            checkTrue("\(name) is a valid wire name",
                      name.range(of: "^[A-Za-z0-9_-]{1,64}$", options: .regularExpression) != nil)
            checkTrue("\(name) has an object schema", (tool["input_schema"] as? [String: Any])?["type"] as? String == "object")
        }
        checkTrue("wire names are unique", Set(names).count == names.count)
        checkTrue("every wire name maps back to a catalog id",
                  ToolSchemas.idsByWireName(ToolCatalog.all)["reminder_create"] == "reminder.create")

        let reminder = tools.first { ($0["name"] as? String) == "reminder_create" }
        let schema = reminder?["input_schema"] as? [String: Any]
        check("reminder required", ((schema?["required"] as? [String]) ?? []).joined(separator: ","), "title,due")
        let properties = (schema?["properties"] as? [String: Any]) ?? [:]
        check("reminder properties", properties.keys.sorted().joined(separator: ","), "due,notes,title")
        checkTrue("reminder says a click is needed",
                  ((reminder?["description"] as? String) ?? "").contains("must click Allow"))
        let mail = tools.first { ($0["name"] as? String) == "mail_draft" }
        checkTrue("mail says nothing is sent", ((mail?["description"] as? String) ?? "").contains("Nothing is sent"))
    }

    // MARK: Extractor

    static func extractor() {
        print("Reading tool calls")
        let turn = ToolCallExtractor.extract(content: [
            ["type": "text", "text": "Sure."],
            toolUse("a", "reminder_create", ["title": "x", "due": "y"])
        ])
        checkTrue("one call", turn.calls.count == 1)
        check("use id kept", turn.calls.first?.useId ?? "", "a")
        check("name kept", turn.calls.first?.name ?? "", "reminder_create")
        checkTrue("no outside content", !turn.sawOutsideContent)

        let outside = ToolCallExtractor.extract(content: [webResult])
        checkTrue("web results are outside content", outside.sawOutsideContent)
        let server = ToolCallExtractor.extract(content: [["type": "server_tool_use", "id": "s", "name": "web_search", "input": [String: Any]()]])
        checkTrue("server tool use is outside content", server.sawOutsideContent && server.calls.isEmpty)

        let numeric = ToolCallExtractor.extract(content: [toolUse("n", "reminder_create", ["title": 5, "due": "x"])])
        check("non-text argument is a problem", numeric.calls.first?.problem ?? "", "\"title\" must be plain text.")
        checkTrue("and no arguments are kept", numeric.calls.first?.arguments.isEmpty == true)

        let noId = ToolCallExtractor.extract(content: [["type": "tool_use", "name": "mail_draft", "input": [String: Any]()]])
        checkTrue("a call with no id is skipped", noId.calls.isEmpty)

        var many: [[String: Any]] = []
        for index in 1...4 { many.append(toolUse("c\(index)", "mail_draft", ["to": "a@b.com", "subject": "s", "body": "b"])) }
        let crowded = ToolCallExtractor.extract(content: many)
        checkTrue("four calls are all kept", crowded.calls.count == 4)
        checkTrue("the first three are fine", crowded.calls.prefix(3).allSatisfy { $0.problem == nil })
        check("the fourth is refused", crowded.calls[3].problem ?? "", "Too many actions in one reply. Ask for them one at a time.")
    }

    // MARK: Session basics

    static func sessionBasics() {
        print("Session: waiting, approving, finishing")
        var session = AssistantSession()
        let steps = session.beginTurn(reminderTurn(), now: now)
        guard case .waiting(let useId, let action)? = steps.first else {
            checkTrue("a reminder waits for a click", false)
            return
        }
        checkTrue("a reminder waits for a click", true)
        check("use id linked", useId, "t1")
        check("card title is the app's", action.card.title, "Set a reminder")
        checkTrue("not marked untrusted", !action.card.fromUntrustedContent)
        checkTrue("shows as waiting", session.waiting(now: now).count == 1)

        let voice = session.approve(action.id, via: .voice, now: now)
        if case .rejected = voice { checkTrue("a spoken yes does not approve", true) }
        else { checkTrue("a spoken yes does not approve", false) }
        let model = session.approve(action.id, via: .model, now: now)
        if case .rejected = model { checkTrue("the model cannot approve", true) }
        else { checkTrue("the model cannot approve", false) }
        checkTrue("still waiting", session.waiting(now: now).count == 1)

        let click = session.approve(action.id, via: .click, now: now)
        if case .run(let id, let call) = click {
            check("click runs it", id, "t1")
            check("the right tool", call.toolId, "reminder.create")
        } else {
            checkTrue("click runs it", false)
        }
        checkTrue("no longer waiting", session.waiting(now: now).isEmpty)
        let again = session.approve(action.id, via: .click, now: now)
        if case .rejected = again { checkTrue("a second approval of the same action is rejected", true) }
        else { checkTrue("a second approval of the same action is rejected", false) }

        session.finish(useId: "t1", ok: true, message: "It will show as a notification.", now: now)
        check("the model is told what happened",
              session.feedback().results.first?.text ?? "",
              "Done: \"Set a reminder\". It will show as a notification.")

        let log = session.takeLog()
        check("log outcomes", log.map { $0.outcome.rawValue }.joined(separator: ","), "proposed,approved,ran")
        checkTrue("only the time is logged", log.first?.details.keys.sorted() == ["due"])
        let lines = log.compactMap { ActivityLog.jsonLine($0) }.joined()
        checkTrue("the reminder title stays out of the log", !lines.contains("Call Ada"))
        checkTrue("the log is drained", session.takeLog().isEmpty)

        print("Session: refusals")
        var other = AssistantSession()
        let unknown = other.beginTurn(
            ToolCallExtractor.extract(content: [toolUse("u1", "wire_money", ["amount": "5"])]), now: now)
        if case .refused(_, let reason)? = unknown.first { check("unknown tool is refused", reason, "Unknown tool: wire_money") }
        else { checkTrue("unknown tool is refused", false) }
        check("the model is told", other.feedback().results.first?.text ?? "", "The app did not do this: Unknown tool: wire_money")

        let bad = other.beginTurn(
            ToolCallExtractor.extract(content: [toolUse("u2", "reminder_create", ["title": "x", "due": "tomorrow"])]), now: now)
        if case .waiting? = bad.first {
            // The policy accepts the shape. The strict parser refuses the time when the app tries to run it.
            checkTrue("a reminder with a vague time still waits, then the parser refuses it", ReminderParser.parseISO8601("tomorrow") == nil)
        } else {
            checkTrue("a reminder with a vague time still waits, then the parser refuses it", false)
        }

        let dup = other.beginTurn(
            ToolCallExtractor.extract(content: [
                toolUse("d1", "mail_draft", ["to": "a@b.com", "subject": "s", "body": "b"]),
                toolUse("d1", "mail_draft", ["to": "a@b.com", "subject": "s", "body": "b"])
            ]), now: now)
        checkTrue("the same tool use id twice counts once", dup.count == 1)

        print("Session: a safe draft runs, then waits when untrusted")
        var drafts = AssistantSession()
        let plain = drafts.beginTurn(mailTurn("m1"), now: now)
        if case .run(_, let call)? = plain.first { check("a draft runs without asking", call.toolId, "mail.draft") }
        else { checkTrue("a draft runs without asking", false) }
    }

    // MARK: Untrusted content

    static func untrustedContent() {
        print("Untrusted content")
        var session = AssistantSession()
        let afterSearch = session.beginTurn(mailTurn("m1", outside: true), now: now)
        if case .waiting(_, let action)? = afterSearch.first {
            checkTrue("a draft after a web search waits for a click", true)
            checkTrue("and the card says where it came from", action.card.fromUntrustedContent)
        } else {
            checkTrue("a draft after a web search waits for a click", false)
        }
        let later = session.beginTurn(mailTurn("m2"), now: now)
        if case .waiting? = later.first { checkTrue("the mark stays for the rest of the conversation", true) }
        else { checkTrue("the mark stays for the rest of the conversation", false) }

        var attached = AssistantSession()
        attached.noteOutsideContent()
        let withFile = attached.beginTurn(mailTurn("m3"), now: now)
        if case .waiting? = withFile.first { checkTrue("an attached file or window also marks the conversation", true) }
        else { checkTrue("an attached file or window also marks the conversation", false) }
    }

    // MARK: Feedback

    static func feedbackToTheModel() {
        print("What the model is told")
        var session = AssistantSession()
        guard case .waiting(_, let action)? = session.beginTurn(reminderTurn(), now: now).first else {
            checkTrue("setup", false)
            return
        }
        let first = session.feedback()
        check("waiting is reported as not answered",
              first.results.first?.text ?? "", "The user has not answered yet. Nothing has been done.")
        checkTrue("asking twice gives the same answer", session.feedback().results == first.results)

        session.markFeedbackSent()
        checkTrue("once sent, there is nothing more to send", session.feedback().results.isEmpty)

        if case .run = session.approve(action.id, via: .click, now: now) {
            session.finish(useId: "t1", ok: true, message: "Done it.", now: now)
        }
        let late = session.feedback()
        checkTrue("no second answer for the same tool use", late.results.isEmpty)
        check("a late result goes out as a note",
              late.notes.first ?? "",
              "Update from the app about an earlier request: Done: \"Set a reminder\". Done it.")

        var declined = AssistantSession()
        if case .waiting(_, let pending)? = declined.beginTurn(reminderTurn("t2"), now: now).first {
            declined.deny(pending.id, now: now)
            check("a no is reported",
                  declined.feedback().results.first?.text ?? "",
                  "The user declined \"Set a reminder\". Nothing was done.")
            check("and logged", declined.takeLog().map { $0.outcome.rawValue }.joined(separator: ","), "proposed,denied")
        }

        var slow = AssistantSession()
        if case .waiting(_, let pending)? = slow.beginTurn(reminderTurn("t3"), now: now).first {
            let later = now.addingTimeInterval(601)
            slow.sweepExpired(now: later)
            checkTrue("an old card disappears", slow.waiting(now: later).isEmpty)
            check("expiry is reported",
                  slow.feedback().results.first?.text ?? "",
                  "The user did not answer in time, so \"Set a reminder\" was not done.")
            let click = slow.approve(pending.id, via: .click, now: later)
            if case .rejected(let message) = click { check("approving an expired action is rejected", message, "That action is no longer waiting.") }
            else { checkTrue("approving an expired action is rejected", false) }
        }
    }

    // MARK: Ending a conversation

    static func endingAConversation() {
        print("Ending a conversation")
        var session = AssistantSession()
        _ = session.beginTurn(mailTurn("m1", outside: true), now: now)
        checkTrue("marked and waiting", session.tainted && session.waiting(now: now).count == 1)
        session.endConversation(now: now)
        checkTrue("nothing waits any more", session.waiting(now: now).isEmpty)
        checkTrue("the mark is cleared", !session.tainted)
        checkTrue("no tool history", !session.hasToolHistory)
        check("the dropped action is logged as declined",
              session.takeLog().map { $0.outcome.rawValue }.joined(separator: ","), "proposed,denied")
        let fresh = session.beginTurn(mailTurn("m9"), now: now)
        if case .run? = fresh.first { checkTrue("a new conversation starts trusted", true) }
        else { checkTrue("a new conversation starts trusted", false) }
    }

    // MARK: mailto links

    static func mailtoLinks() {
        print("Draft links")
        let simple = MailDraftRequest(to: "ada@example.com", subject: "Hi there", body: "Line1\nLine2")
        check("plain draft", simple.mailtoURL()?.absoluteString ?? "",
              "mailto:ada@example.com?subject=Hi%20there&body=Line1%0D%0ALine2")

        let accents = MailDraftRequest(to: "ada@example.com", subject: "Hi", body: "Café")
        check("accents are encoded", accents.mailtoURL()?.absoluteString ?? "",
              "mailto:ada@example.com?subject=Hi&body=Caf%C3%A9")

        let bidi = MailDraftRequest(to: "ada@example.com", subject: "Hi\u{202E}there", body: "ok")
        let bidiText = bidi.mailtoURL()?.absoluteString ?? ""
        checkTrue("hidden characters are removed from the draft", !bidiText.contains("%E2%80%AE") && bidiText.contains("subject=Hithere"))

        let two = MailDraftRequest(to: "a@b.com,c@d.com", subject: "s", body: "b")
        checkTrue("a second recipient is refused", two.mailtoURL() == nil)
        let bcc = MailDraftRequest(to: "a@b.com?bcc=x@y.com", subject: "s", body: "b")
        checkTrue("an added header is refused", bcc.mailtoURL() == nil)

        let injected = MailDraftRequest(to: "a@b.com", subject: "s&bcc=x@y.com", body: "b&cc=z@y.com")
        let injectedText = injected.mailtoURL()?.absoluteString ?? ""
        checkTrue("& and = in the text are encoded", injectedText.contains("subject=s%26bcc%3Dx%40y.com") && injectedText.contains("body=b%26cc%3Dz%40y.com"))

        let long = MailDraftRequest(to: "a@b.com", subject: "s", body: String(repeating: "a", count: 7_000))
        checkTrue("a message that is too long is refused, not cut", long.mailtoURL() == nil)
    }
}
