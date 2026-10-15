import Foundation

@main
enum AssistantCoreTests {

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

    static let readSpec = ToolSpec(
        id: "test.read", title: "Look at something", risk: .read,
        required: [], optional: ["what"], fields: [ToolField(label: "What", argument: "what")]
    )
    static let draftSpec = ToolSpec(
        id: "test.draft", title: "Prepare something", risk: .draft,
        required: ["text"], optional: [], fields: [ToolField(label: "Text", argument: "text", isContent: true)]
    )
    static let actSpec = ToolSpec(
        id: "test.act", title: "Send something", risk: .act,
        required: ["to"], optional: ["token"],
        fields: [ToolField(label: "To", argument: "to", isLogged: true),
                 ToolField(label: "Secret note", argument: "note", isContent: true)]
    )
    static let criticalSpec = ToolSpec(
        id: "test.critical", title: "Move money", risk: .critical,
        required: ["amount"], optional: [], fields: [ToolField(label: "Amount", argument: "amount")]
    )
    static var catalog: [String: ToolSpec] {
        var map: [String: ToolSpec] = [:]
        for spec in [readSpec, draftSpec, actSpec, criticalSpec] { map[spec.id] = spec }
        for (id, spec) in ToolCatalog.all { map[id] = spec }
        return map
    }

    static func call(_ id: String, _ args: ToolArguments = [:], _ origin: ToolOrigin = .user) -> ToolCall {
        ToolCall(toolId: id, arguments: args, origin: origin)
    }

    static func main() {

        // ── ToolRisk ───────────────────────────────────────────────────────────
        print("ToolRisk")
        checkTrue("read < draft < act < critical",
                  ToolRisk.read < .draft && ToolRisk.draft < .act && ToolRisk.act < .critical)

        // ── ActionPolicy.decide ────────────────────────────────────────────────
        print("ActionPolicy.decide")

        if case .refuse(let why) = ActionPolicy.decide(call("nope"), catalog: catalog) {
            checkTrue("unknown tool is refused", why.contains("Unknown tool"))
        } else { checkTrue("unknown tool is refused", false) }

        checkTrue("read runs without asking",
                  ActionPolicy.decide(call("test.read"), catalog: catalog) == .run)
        checkTrue("draft from the user runs without asking",
                  ActionPolicy.decide(call("test.draft", ["text": "hello"]), catalog: catalog) == .run)

        if case .askUser(let card) = ActionPolicy.decide(call("test.draft", ["text": "hello"], .untrustedContent), catalog: catalog) {
            checkTrue("draft that came from a file asks the user", true)
            checkTrue("…and the card says it came from content", card.fromUntrustedContent)
        } else { checkTrue("draft that came from a file asks the user", false) }

        if case .askUser(let card) = ActionPolicy.decide(call("test.act", ["to": "ada@example.com"]), catalog: catalog) {
            checkTrue("act always asks", true)
            check("card title comes from the spec", card.title, "Send something")
            checkTrue("act needs one click", !card.needsSecondClick)
            checkTrue("risk is carried on the card", card.risk == .act)
        } else { checkTrue("act always asks", false) }

        if case .askUser(let card) = ActionPolicy.decide(call("test.critical", ["amount": "5000"]), catalog: catalog) {
            checkTrue("critical asks and needs a second click", card.needsSecondClick)
        } else { checkTrue("critical asks and needs a second click", false) }

        // ── Argument checks ────────────────────────────────────────────────────
        print("Argument checks")

        if case .refuse(let why) = ActionPolicy.decide(call("test.act", ["to": "a@b.co", "title": "Totally safe"]), catalog: catalog) {
            checkTrue("an argument the tool does not declare is refused", why.contains("Unexpected argument"))
        } else { checkTrue("an argument the tool does not declare is refused", false) }

        if case .refuse(let why) = ActionPolicy.decide(call("test.act", [:]), catalog: catalog) {
            checkTrue("a missing required argument is refused", why.contains("Missing"))
        } else { checkTrue("a missing required argument is refused", false) }

        if case .refuse = ActionPolicy.decide(call("test.act", ["to": "   \n "]), catalog: catalog) {
            checkTrue("a blank required argument is refused", true)
        } else { checkTrue("a blank required argument is refused", false) }

        let huge = String(repeating: "a", count: ActionPolicy.maxArgumentBytes + 1)
        if case .refuse(let why) = ActionPolicy.decide(call("test.act", ["to": huge]), catalog: catalog) {
            checkTrue("an oversized argument is refused", why.contains("too long"))
        } else { checkTrue("an oversized argument is refused", false) }

        // ── Confirmation card ──────────────────────────────────────────────────
        print("Confirmation card")

        if case .refuse(let why) = ActionPolicy.decide(call("test.act", ["to": "ada@example.com", "token": "SECRET"]), catalog: catalog) {
            checkTrue("an argument with no card field is refused, so nothing runs unseen", why.contains("would not be shown"))
            checkTrue("…and the refusal does not repeat its value", !why.contains("SECRET"))
        } else { checkTrue("an argument with no card field is refused, so nothing runs unseen", false) }

        if case .askUser(let card) = ActionPolicy.decide(call("test.act", ["to": "ada@example.com"]), catalog: catalog) {
            check("labels come from the spec", card.lines.first?.label ?? "", "To")
        } else { checkTrue("labels come from the spec", false) }

        let sneaky = "ada@example.com\u{202E}moc.live@evil\nBcc: x@y.com\u{200B}"
        if case .askUser(let card) = ActionPolicy.decide(call("test.act", ["to": sneaky]), catalog: catalog) {
            let shown = card.lines.first?.value ?? ""
            checkTrue("right-to-left override is removed", !shown.unicodeScalars.contains { $0.value == 0x202E })
            checkTrue("zero-width characters are removed", !shown.unicodeScalars.contains { $0.value == 0x200B })
            checkTrue("a line break becomes a visible marker", shown.contains("⏎") && !shown.contains("\n"))
        } else { checkTrue("hidden characters are cleaned", false) }

        let longText = String(repeating: "x", count: 900)
        check("long text is cut at the display limit",
              String(ActionPolicy.sanitize(longText, limit: ActionPolicy.maxDisplayLength).count),
              String(ActionPolicy.maxDisplayLength + 1))
        check("sanitize leaves plain text alone", ActionPolicy.sanitize("Call Mum at 4", limit: 50), "Call Mum at 4")
        check("sanitize turns tabs into spaces", ActionPolicy.sanitize("a\tb", limit: 50), "a b")

        // ── Hardened display and limits ────────────────────────────────────────
        print("Hardened display and limits")

        if case .askUser(let card) = ActionPolicy.decide(call("test.draft", ["text": longText], .untrustedContent), catalog: catalog) {
            let line = card.lines.first
            check("a cut value says how much was hidden", String(line?.hiddenCharacters ?? -1), "400")
            check("the shown part is the display limit plus an ellipsis", String(line?.value.count ?? 0), String(ActionPolicy.maxDisplayLength + 1))
        } else { checkTrue("a long value still gets a card", false) }
        if case .askUser(let card) = ActionPolicy.decide(call("test.draft", ["text": "short"], .untrustedContent), catalog: catalog) {
            check("a value that fits hides nothing", String(card.lines.first?.hiddenCharacters ?? -1), "0")
        } else { checkTrue("a short value still gets a card", false) }

        let zalgo = "a" + String(repeating: "\u{0301}", count: 50)
        check("stacked combining marks are capped",
              String(ActionPolicy.sanitize(zalgo, limit: 100).unicodeScalars.count), String(1 + ActionPolicy.maxCombiningMarks))
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"
        check("emoji joiners are kept", ActionPolicy.sanitize(family, limit: 50), family)
        check("tag characters are removed", ActionPolicy.sanitize("a\u{E0041}b", limit: 50), "ab")
        check("soft hyphens are removed", ActionPolicy.sanitize("a\u{00AD}b", limit: 50), "ab")
        check("private-use characters are removed", ActionPolicy.sanitize("a\u{E000}b", limit: 50), "ab")

        if case .refuse(let why) = ActionPolicy.decide(call("test.act", ["to": "\u{200B}\u{200B}"]), catalog: catalog) {
            checkTrue("a required argument made only of invisible characters is refused", why.contains("Missing"))
        } else { checkTrue("a required argument made only of invisible characters is refused", false) }

        let emoji = String(repeating: "\u{1F600}", count: 6_000)   // 6,000 characters but 24,000 bytes
        if case .refuse(let why) = ActionPolicy.decide(call("test.act", ["to": emoji]), catalog: catalog) {
            checkTrue("the size limit counts bytes, not characters", why.contains("too long"))
        } else { checkTrue("the size limit counts bytes, not characters", false) }

        // ── TurnContext ────────────────────────────────────────────────────────
        print("TurnContext")

        var turn = TurnContext()
        check("a new turn is trusted", turn.origin.rawValue, "user")
        check("a call made in a trusted turn is from the user", turn.makeCall("test.draft", ["text": "hi"]).origin.rawValue, "user")
        turn.noteUntrustedContent()
        let afterReading = turn.makeCall("test.draft", ["text": "hi"])
        checkTrue("after reading outside content every call is untrusted", afterReading.origin == .untrustedContent)
        if case .askUser = ActionPolicy.decide(afterReading, catalog: catalog) {
            checkTrue("so even a draft waits for the user", true)
        } else { checkTrue("so even a draft waits for the user", false) }
        checkTrue("a fresh turn starts trusted again", TurnContext().origin == .user)

        // ── Dates on the card ──────────────────────────────────────────────────
        print("Dates on the card")

        let plusOne = TimeZone(secondsFromGMT: 3600)!
        let utcZone = TimeZone(secondsFromGMT: 0)!
        let remind = call("reminder.create", ["title": "Call Mum", "due": "2026-10-07T15:00:00Z"])
        if case .askUser(let card) = ActionPolicy.decide(remind, catalog: ToolCatalog.all, timeZone: plusOne) {
            let when = card.lines.first { $0.label == "When" }?.value ?? ""
            checkTrue("the time is shown in the user's own time zone", when.hasPrefix("Wed 7 Oct 2026, 16:00"))
            checkTrue("the reminder text is marked as content", card.lines.first { $0.label == "Reminder" }?.isContent == true)
        } else { checkTrue("a reminder gets a card", false) }
        if case .askUser(let card) = ActionPolicy.decide(remind, catalog: ToolCatalog.all, timeZone: utcZone) {
            let when = card.lines.first { $0.label == "When" }?.value ?? ""
            checkTrue("the same moment reads 15:00 in UTC", when.hasPrefix("Wed 7 Oct 2026, 15:00"))
        } else { checkTrue("a reminder gets a card in UTC", false) }
        let unreadable = call("reminder.create", ["title": "x", "due": "tomorrow at 4"])
        if case .askUser(let card) = ActionPolicy.decide(unreadable, catalog: ToolCatalog.all, timeZone: plusOne) {
            check("a time that cannot be read is shown as written", card.lines.first { $0.label == "When" }?.value ?? "", "tomorrow at 4")
        } else { checkTrue("an unreadable time still gets a card", false) }

        // ── PendingActionStore ─────────────────────────────────────────────────
        print("PendingActionStore")

        var store = PendingActionStore()
        let actCall = call("test.act", ["to": "ada@example.com"])

        guard case .pending(let pending) = store.propose(actCall, catalog: catalog, now: now) else {
            print("  ✗ act becomes a pending action"); failures += 1
            finish(); return
        }
        checkTrue("act becomes a pending action", store.items.count == 1)
        checkTrue("pending action keeps the call", pending.call == actCall)

        if case .rejected = store.approve(pending.id, via: .voice, now: now) {
            checkTrue("a spoken yes cannot approve", true)
        } else { checkTrue("a spoken yes cannot approve", false) }
        if case .rejected = store.approve(pending.id, via: .model, now: now) {
            checkTrue("a message from the model cannot approve", true)
        } else { checkTrue("a message from the model cannot approve", false) }
        if case .rejected = store.approve(pending.id, via: .automation, now: now) {
            checkTrue("an automatic trigger cannot approve", true)
        } else { checkTrue("an automatic trigger cannot approve", false) }
        checkTrue("the action is still waiting after those", store.items[pending.id] != nil)

        if case .approved(let approved) = store.approve(pending.id, via: .click, now: now.addingTimeInterval(5)) {
            checkTrue("a click approves", approved == actCall)
        } else { checkTrue("a click approves", false) }
        checkTrue("an approved action leaves the waiting list", store.items.isEmpty)
        if case .rejected = store.approve(pending.id, via: .click, now: now.addingTimeInterval(6)) {
            checkTrue("an action cannot be approved twice", true)
        } else { checkTrue("an action cannot be approved twice", false) }

        // draft from the user is not stored
        var store2 = PendingActionStore()
        if case .run(let ran) = store2.propose(call("test.draft", ["text": "hi"]), catalog: catalog, now: now) {
            checkTrue("a draft runs straight away", ran.toolId == "test.draft")
        } else { checkTrue("a draft runs straight away", false) }
        checkTrue("…and is not stored", store2.items.isEmpty)

        if case .refused = store2.propose(call("nope"), catalog: catalog, now: now) {
            checkTrue("a refused call is not stored", store2.items.isEmpty)
        } else { checkTrue("a refused call is not stored", false) }

        // expiry
        var store3 = PendingActionStore()
        store3.timeToLive = 60
        if case .pending(let p3) = store3.propose(actCall, catalog: catalog, now: now) {
            if case .rejected(let why) = store3.approve(p3.id, via: .click, now: now.addingTimeInterval(61)) {
                checkTrue("an expired action cannot be approved", why.contains("expired"))
            } else { checkTrue("an expired action cannot be approved", false) }
            checkTrue("…and is removed", store3.items.isEmpty)
        }
        if case .pending(let p3b) = store3.propose(actCall, catalog: catalog, now: now) {
            let gone = store3.removeExpired(now: now.addingTimeInterval(120))
            checkTrue("removeExpired returns the ids it dropped", gone == [p3b.id])
        }

        // deny
        var store4 = PendingActionStore()
        if case .pending(let p4) = store4.propose(actCall, catalog: catalog, now: now) {
            checkTrue("deny removes the action", store4.deny(p4.id) && store4.items.isEmpty)
            checkTrue("deny on something not waiting is false", !store4.deny(p4.id))
        }

        // capacity
        var store5 = PendingActionStore()
        store5.maxPending = 2
        _ = store5.propose(actCall, catalog: catalog, now: now)
        _ = store5.propose(actCall, catalog: catalog, now: now)
        if case .refused(let why) = store5.propose(actCall, catalog: catalog, now: now) {
            checkTrue("too many waiting actions are refused", why.contains("Too many"))
        } else { checkTrue("too many waiting actions are refused", false) }
        checkTrue("waiting lists the two that fit", store5.waiting.count == 2)

        // critical needs two separate clicks
        var store6 = PendingActionStore()
        if case .pending(let p6) = store6.propose(call("test.critical", ["amount": "5000"]), catalog: catalog, now: now) {
            if case .needsSecondClick = store6.approve(p6.id, via: .click, now: now) {
                checkTrue("first click on a critical action is not enough", true)
            } else { checkTrue("first click on a critical action is not enough", false) }
            if case .needsSecondClick = store6.approve(p6.id, via: .click, now: now.addingTimeInterval(0.1)) {
                checkTrue("a double-click counts as one", true)
            } else { checkTrue("a double-click counts as one", false) }
            if case .approved = store6.approve(p6.id, via: .click, now: now.addingTimeInterval(2)) {
                checkTrue("a separate second click approves", true)
            } else { checkTrue("a separate second click approves", false) }
            checkTrue("…and the action is gone", store6.items.isEmpty)
        } else { checkTrue("critical action becomes pending", false) }

        // a voice "yes" must not count as a first click either
        var store7 = PendingActionStore()
        if case .pending(let p7) = store7.propose(call("test.critical", ["amount": "5000"]), catalog: catalog, now: now) {
            _ = store7.approve(p7.id, via: .voice, now: now)
            _ = store7.approve(p7.id, via: .voice, now: now.addingTimeInterval(5))
            checkTrue("voice never advances a critical action", store7.items[p7.id]?.firstClickAt == nil)
        }

        // ── ActivityLog ────────────────────────────────────────────────────────
        print("ActivityLog")

        let logDate = Date(timeIntervalSince1970: 1_800_000_000)
        let reminderCall = call("reminder.create", ["title": "Call Mum about the money", "due": iso(now.addingTimeInterval(3600)), "notes": "private"])
        let entry = ActivityLog.entry(call: reminderCall, spec: ToolCatalog.reminderCreate, outcome: .approved, at: logDate)
        checkTrue("logged fields are kept", entry.details["due"] != nil)
        checkTrue("title is not logged", entry.details["title"] == nil)
        checkTrue("notes are not logged", entry.details["notes"] == nil)
        check("risk is recorded by name", entry.risk, "act")
        check("origin is recorded", entry.origin, "user")

        let unknown = ActivityLog.entry(call: call("nope"), spec: nil, outcome: .refused, at: logDate, note: "Unknown tool")
        check("an unknown tool logs risk as unknown", unknown.risk, "unknown")
        check("a note is kept", unknown.note ?? "", "Unknown tool")

        if let line = ActivityLog.jsonLine(entry) {
            checkTrue("json line is one line", !line.contains("\n"))
            checkTrue("json line has the tool id", line.contains("reminder.create"))
            checkTrue("json line leaves out the title", !line.contains("Call Mum"))
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            if let data = line.data(using: .utf8), let back = try? decoder.decode(ActivityEntry.self, from: data) {
                checkTrue("json line reads back the same", back == entry)
            } else { checkTrue("json line reads back the same", false) }
        } else { checkTrue("json line is produced", false) }

        let human = ActivityLog.humanLine(entry)
        checkTrue("human line names the tool and the outcome", human.contains("reminder.create") && human.contains("approved"))

        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("coucou-activity-\(UUID().uuidString)", isDirectory: true)
        let logURL = tmp.appendingPathComponent("activity.log")
        checkTrue("first line is written", ActivityLogFile.append(String(repeating: "a", count: 40), to: logURL, maxBytes: 50))
        checkTrue("second line is written", ActivityLogFile.append(String(repeating: "b", count: 40), to: logURL, maxBytes: 50))
        let permissions = (try? FileManager.default.attributesOfItem(atPath: logURL.path))?[.posixPermissions] as? NSNumber
        checkTrue("log file is private to its owner", permissions?.intValue == 0o600)
        checkTrue("third line is written", ActivityLogFile.append(String(repeating: "c", count: 40), to: logURL, maxBytes: 50))
        let rotated = logURL.appendingPathExtension("1")
        checkTrue("a big log is moved aside", FileManager.default.fileExists(atPath: rotated.path))
        let current = (try? String(contentsOf: logURL, encoding: .utf8)) ?? ""
        check("a new log starts with the latest line", current, String(repeating: "c", count: 40) + "\n")
        try? FileManager.default.removeItem(at: tmp)

        // ── ReminderParser ─────────────────────────────────────────────────────
        print("ReminderParser")

        func reminder(_ args: ToolArguments) -> Result<ReminderRequest, ReminderParseError> {
            ReminderParser.parse(args, now: now)
        }
        let hourLater = iso(now.addingTimeInterval(3600))

        if case .success(let r) = reminder(["title": "  Call Mum  ", "due": hourLater]) {
            check("title is trimmed", r.title, "Call Mum")
            checkTrue("due time is read", r.due == now.addingTimeInterval(3600))
            checkTrue("no notes means nil", r.notes == nil)
        } else { checkTrue("a valid reminder parses", false) }

        if case .success(let r) = reminder(["title": "x", "due": hourLater, "notes": "bring the file"]) {
            check("notes are kept", r.notes ?? "", "bring the file")
        } else { checkTrue("notes parse", false) }

        let utcDue = ReminderParser.parseISO8601("2026-10-07T15:00:00Z")
        let lagosDue = ReminderParser.parseISO8601("2026-10-07T16:00:00+01:00")
        checkTrue("a +01:00 offset means the same moment as Z", utcDue != nil && utcDue == lagosDue)
        checkTrue("fractional seconds are accepted", ReminderParser.parseISO8601("2026-10-07T15:00:00.500Z") != nil)
        checkTrue("a date with no time is not accepted", ReminderParser.parseISO8601("2026-10-07") == nil)
        checkTrue("a time with no zone is not accepted", ReminderParser.parseISO8601("2026-10-07T15:00:00") == nil)
        checkTrue("a zone written without a colon is not accepted", ReminderParser.parseISO8601("2026-10-07T15:00:00+0100") == nil)
        checkTrue("words are not accepted", ReminderParser.parseISO8601("tomorrow at 4") == nil)

        if case .failure(let e) = reminder(["title": "x", "due": "tomorrow at 4"]) { checkTrue("bad date gives badDate", e == .badDate) }
        else { checkTrue("bad date gives badDate", false) }
        if case .failure(let e) = reminder(["title": "x", "due": iso(now.addingTimeInterval(-3600))]) { checkTrue("an hour ago gives dateInPast", e == .dateInPast) }
        else { checkTrue("an hour ago gives dateInPast", false) }
        if case .success = reminder(["title": "x", "due": iso(now.addingTimeInterval(-30))]) { checkTrue("30 seconds ago still counts as now", true) }
        else { checkTrue("30 seconds ago still counts as now", false) }
        if case .failure(let e) = reminder(["title": "x", "due": iso(now.addingTimeInterval(ReminderParser.maxFuture + 86_400))]) { checkTrue("more than five years away gives dateTooFar", e == .dateTooFar) }
        else { checkTrue("more than five years away gives dateTooFar", false) }
        if case .failure(let e) = reminder(["title": "   ", "due": hourLater]) { checkTrue("blank title gives missingTitle", e == .missingTitle) }
        else { checkTrue("blank title gives missingTitle", false) }
        if case .failure(let e) = reminder(["title": "x"]) { checkTrue("no time gives missingDue", e == .missingDue) }
        else { checkTrue("no time gives missingDue", false) }
        if case .failure(let e) = reminder(["title": String(repeating: "t", count: ReminderParser.maxTitleLength + 1), "due": hourLater]) { checkTrue("long title gives titleTooLong", e == .titleTooLong) }
        else { checkTrue("long title gives titleTooLong", false) }
        if case .failure(let e) = reminder(["title": "x", "due": hourLater, "notes": String(repeating: "n", count: ReminderParser.maxNotesLength + 1)]) { checkTrue("long notes give notesTooLong", e == .notesTooLong) }
        else { checkTrue("long notes give notesTooLong", false) }
        if case .success(let r) = reminder(["title": "a\nb", "due": hourLater]) { check("a line break in a title becomes a marker", r.title, "a ⏎ b") }
        else { checkTrue("a line break in a title is handled", false) }
        checkTrue("every parse error has a message", [ReminderParseError.missingTitle, .titleTooLong, .notesTooLong, .missingDue, .badDate, .dateInPast, .dateTooFar].allSatisfy { !$0.message.isEmpty })

        // ── MailDraftParser ────────────────────────────────────────────────────
        print("MailDraftParser")

        for good in ["ada@example.com", "ada.o+news@mail.example.co.uk", "A_B-c@sub.example.ng"] {
            checkTrue("accepts \(good)", MailDraftParser.isPlausibleAddress(good))
        }
        let bad: [String] = [
            "", "ada", "ada@example", "ada@@example.com", "ada example@example.com",
            "ada@example.com, bob@example.com", "ada@example.com;bob@example.com", "<ada@example.com>",
            "Ada <ada@example.com>", "ada@exa mple.com", "ada@-example.com", "ada@example-.com",
            "ada@example.c", "ada@example..com", ".ada@example.com", "ada.@example.com",
            "ad\u{00E0}@example.com", "ada@example.com\nBcc: x@y.com", "ada@exam\u{0430}ple.com"
        ]
        for text in bad {
            checkTrue("refuses \(text.debugDescription)", !MailDraftParser.isPlausibleAddress(text))
        }

        if case .success(let m) = MailDraftParser.parse(["to": " ada@example.com ", "subject": "Hello", "body": "Hi Ada,\nSee you soon."]) {
            check("address is trimmed", m.to, "ada@example.com")
            check("subject is kept", m.subject, "Hello")
            checkTrue("body keeps its line breaks", m.body.contains("\n"))
        } else { checkTrue("a valid draft parses", false) }
        if case .failure(let e) = MailDraftParser.parse(["to": "nope", "subject": "Hello", "body": "Hi"]) { checkTrue("bad address gives badRecipient", e == .badRecipient) }
        else { checkTrue("bad address gives badRecipient", false) }
        if case .failure(let e) = MailDraftParser.parse(["to": "ada@example.com", "subject": "Hi\nBcc: x@y.com", "body": "Hi"]) { checkTrue("a line break in the subject gives badSubject", e == .badSubject) }
        else { checkTrue("a line break in the subject gives badSubject", false) }
        if case .failure(let e) = MailDraftParser.parse(["to": "ada@example.com", "subject": "  ", "body": "Hi"]) { checkTrue("blank subject gives missingSubject", e == .missingSubject) }
        else { checkTrue("blank subject gives missingSubject", false) }
        if case .failure(let e) = MailDraftParser.parse(["to": "ada@example.com", "subject": String(repeating: "s", count: MailDraftParser.maxSubjectLength + 1), "body": "Hi"]) { checkTrue("long subject gives subjectTooLong", e == .subjectTooLong) }
        else { checkTrue("long subject gives subjectTooLong", false) }
        if case .failure(let e) = MailDraftParser.parse(["to": "ada@example.com", "subject": "Hi", "body": " \n "]) { checkTrue("blank body gives missingBody", e == .missingBody) }
        else { checkTrue("blank body gives missingBody", false) }
        checkTrue("every mail error has a message", [MailDraftParseError.badRecipient, .missingSubject, .badSubject, .subjectTooLong, .missingBody].allSatisfy { !$0.message.isEmpty })

        // ── ToolCatalog ────────────────────────────────────────────────────────
        print("ToolCatalog")

        check("reminder tool id is stable", ToolCatalog.reminderCreate.id, "reminder.create")
        check("mail draft tool id is stable", ToolCatalog.mailDraft.id, "mail.draft")
        checkTrue("setting a reminder is an act", ToolCatalog.reminderCreate.risk == .act)
        checkTrue("drafting an email is a draft", ToolCatalog.mailDraft.risk == .draft)
        checkTrue("the catalog is keyed by id", ToolCatalog.all.allSatisfy { $0.key == $0.value.id })
        for spec in ToolCatalog.all.values {
            let declared = Set(spec.required + spec.optional)
            checkTrue("\(spec.id): every card field is a declared argument", spec.fields.allSatisfy { declared.contains($0.argument) })
            checkTrue("\(spec.id): no argument is both required and optional", Set(spec.required).isDisjoint(with: Set(spec.optional)))
        }

        // ── End to end ─────────────────────────────────────────────────────────
        print("End to end")

        var flow = PendingActionStore()
        let spoken = call("reminder.create", ["title": "Call Mum", "due": hourLater])
        if case .pending(let p) = flow.propose(spoken, catalog: ToolCatalog.all, now: now) {
            checkTrue("a reminder waits for a click", p.card.title == "Set a reminder")
            _ = flow.approve(p.id, via: .voice, now: now.addingTimeInterval(1))
            checkTrue("saying yes does nothing", flow.items[p.id] != nil)
            if case .approved(let approved) = flow.approve(p.id, via: .click, now: now.addingTimeInterval(2)) {
                if case .success(let request) = ReminderParser.parse(approved.arguments, now: now.addingTimeInterval(2)) {
                    check("the approved call becomes a checked reminder", request.title, "Call Mum")
                } else { checkTrue("the approved call becomes a checked reminder", false) }
            } else { checkTrue("a click approves the reminder", false) }
        } else { checkTrue("a reminder waits for a click", false) }

        var flow2 = PendingActionStore()
        let fromFile = call("mail.draft", ["to": "ada@example.com", "subject": "Hi", "body": "Hello"], .untrustedContent)
        if case .pending(let p) = flow2.propose(fromFile, catalog: ToolCatalog.all, now: now) {
            checkTrue("a draft that came out of a file waits for a click", p.card.fromUntrustedContent)
        } else { checkTrue("a draft that came out of a file waits for a click", false) }
        if case .run = flow2.propose(call("mail.draft", ["to": "ada@example.com", "subject": "Hi", "body": "Hello"]), catalog: ToolCatalog.all, now: now) {
            checkTrue("a draft the user asked for runs straight away", true)
        } else { checkTrue("a draft the user asked for runs straight away", false) }

        finish()
    }

    static func finish() {
        if failures == 0 {
            print("\nAll tests passed.")
            exit(0)
        } else {
            print("\n\(failures) test(s) failed.")
            exit(1)
        }
    }
}
