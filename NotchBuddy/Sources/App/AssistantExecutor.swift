import AppKit
import Foundation
import UserNotifications

// MARK: - Running assistant tools (Mac)
//
// This is the only place that touches the outside world for the assistant. It is reached from two places and
// only two: a tool the policy says is safe to run (a draft), and an action the user approved with a click on the
// confirmation card. Every call is parsed again here with the strict parsers, so nothing malformed gets as far
// as the system. The messages it returns are fixed wording plus values the parsers have checked. They never
// contain free text written by the model.

/// Lets a reminder show as a banner even while this app is the active one.
final class ReminderNotificationDelegate: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {
    static let shared = ReminderNotificationDelegate()

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            willPresent notification: UNNotification,
                                            withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        completionHandler([.banner, .sound])
    }
}

@MainActor
enum AssistantExecutor {

    static func execute(_ call: ToolCall, now: Date) async -> ToolResult {
        switch call.toolId {
        case ToolCatalog.mailDraft.id:     return openMailDraft(call)
        case ToolCatalog.reminderCreate.id: return await setReminder(call, now: now)
        default:
            return ToolResult(ok: false, message: "That action is not available in this version.")
        }
    }

    private static func openMailDraft(_ call: ToolCall) -> ToolResult {
        switch MailDraftParser.parse(call.arguments) {
        case .failure(let error):
            return ToolResult(ok: false, message: error.message)
        case .success(let request):
            guard let url = request.mailtoURL() else {
                return ToolResult(ok: false, message: "That message is too long to open as a draft. Ask for a shorter one.")
            }
            if NSWorkspace.shared.open(url) {
                return ToolResult(ok: true, message: "A draft to \(request.to) is open in your mail app. Nothing was sent.")
            }
            return ToolResult(ok: false, message: "I could not open your mail app.")
        }
    }

    private static func setReminder(_ call: ToolCall, now: Date) async -> ToolResult {
        switch ReminderParser.parse(call.arguments, now: now) {
        case .failure(let error):
            return ToolResult(ok: false, message: error.message)
        case .success(let request):
            let center = UNUserNotificationCenter.current()
            if center.delegate == nil { center.delegate = ReminderNotificationDelegate.shared }
            let granted = (try? await center.requestAuthorization(options: [.alert, .sound])) ?? false
            guard granted else {
                return ToolResult(ok: false, message: "Notifications are off for this app. Turn them on in System Settings, then ask again.")
            }
            let content = UNMutableNotificationContent()
            content.title = "Reminder"
            content.body = request.notes.map { request.title + " — " + $0 } ?? request.title
            content.sound = .default
            // A time a moment ago (a slow click) still fires, a couple of seconds from now.
            let fireDate = max(request.due, now.addingTimeInterval(2))
            let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute, .second], from: fireDate)
            let trigger = UNCalendarNotificationTrigger(dateMatching: parts, repeats: false)
            let notification = UNNotificationRequest(identifier: "assistant.reminder.\(UUID().uuidString)",
                                                     content: content, trigger: trigger)
            do {
                try await center.add(notification)
            } catch {
                return ToolResult(ok: false, message: "I could not schedule the reminder.")
            }
            let when = ActionPolicy.localDateTime(fireDate, timeZone: .current)
            return ToolResult(ok: true, message: "It will show as a notification on \(when).")
        }
    }
}

// MARK: - Glue between the chat, the session and the executor

@MainActor
enum AssistantFlow {

    /// A reply with tool calls arrived. Safe ones run now, the rest wait for a click, refused ones are said so.
    static func handle(_ turn: ExtractedTurn, state: AppState) async {
        let steps = state.assistant.beginTurn(turn, now: Date())
        for step in steps {
            switch step {
            case .run(let useId, let call):
                let result = await AssistantExecutor.execute(call, now: Date())
                state.assistant.finish(useId: useId, ok: result.ok, message: result.message, now: Date())
                say(result.message, state: state)
            case .waiting:
                // Saying yes aloud does not approve anything. Point to the card, after the reply has been read.
                VoiceFlow.speakPhrase(.checkTheCard, state: state)
            case .refused(_, let reason):
                say("I did not do that. \(reason)", state: state)
            }
        }
        flushLog(state: state)
    }

    /// The user clicked Allow and the session said it may run.
    static func runApproved(useId: String, call: ToolCall, state: AppState) async {
        let result = await AssistantExecutor.execute(call, now: Date())
        state.assistant.finish(useId: useId, ok: result.ok, message: result.message, now: Date())
        say(result.message, state: state)
        flushLog(state: state)
    }

    static func deny(_ id: UUID, state: AppState) {
        state.assistant.deny(id, now: Date())
        flushLog(state: state)
    }

    /// App-written line in the chat. Never carries text written by the model.
    static func say(_ text: String, state: AppState) {
        state.chatHistory.append(ChatMessage(role: .assistant, content: text))
    }

    /// Appends new activity entries to ~/Library/Logs/NotchBuddy/assistant-activity.jsonl (owner-only, local).
    static func flushLog(state: AppState) {
        let entries = state.assistant.takeLog()
        guard !entries.isEmpty else { return }
        let fm = FileManager.default
        let dir = fm.urls(for: .libraryDirectory, in: .userDomainMask)[0].appendingPathComponent("Logs/NotchBuddy")
        try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        try? fm.setAttributes([.posixPermissions: 0o700 as NSNumber], ofItemAtPath: dir.path)
        let file = dir.appendingPathComponent("assistant-activity.jsonl")
        for entry in entries {
            if let line = ActivityLog.jsonLine(entry) { ActivityLogFile.append(line, to: file) }
        }
    }
}
