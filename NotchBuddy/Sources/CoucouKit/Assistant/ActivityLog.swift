import Foundation

// MARK: - Activity log (Foundation-only, testable)
//
// A local, human-readable record of what the assistant did and what the user allowed. It never leaves the
// device. Only fields a tool marks `isLogged` are written, so reminder titles, email subjects and message
// bodies stay out of it.

enum ActivityOutcome: String, Codable, Sendable, Equatable {
    case ran, proposed, approved, denied, expired, refused, failed
}

struct ActivityEntry: Codable, Sendable, Equatable {
    let at: Date
    let toolId: String
    let risk: String
    let origin: String
    let outcome: ActivityOutcome
    let details: [String: String]
    let note: String?
}

enum ActivityLog {
    static func entry(call: ToolCall, spec: ToolSpec?, outcome: ActivityOutcome, at: Date, note: String? = nil) -> ActivityEntry {
        var details: [String: String] = [:]
        if let spec {
            for field in spec.fields where field.isLogged {
                guard let raw = call.arguments[field.argument] else { continue }
                let value = ActionPolicy.sanitize(raw, limit: 200)
                if !value.isEmpty { details[field.argument] = value }
            }
        }
        let riskName = spec.map { String(describing: $0.risk) } ?? "unknown"
        let cleanNote = note.map { ActionPolicy.sanitize($0, limit: 200) }
        return ActivityEntry(
            at: at,
            toolId: ActionPolicy.sanitize(call.toolId, limit: 60),
            risk: riskName,
            origin: call.origin.rawValue,
            outcome: outcome,
            details: details,
            note: cleanNote
        )
    }

    /// One JSON object on one line, keys sorted, for the log file.
    static func jsonLine(_ entry: ActivityEntry) -> String? {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(entry) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// A short line for showing in the app.
    static func humanLine(_ entry: ActivityEntry) -> String {
        let formatter = ISO8601DateFormatter()
        var parts = [formatter.string(from: entry.at), entry.toolId, entry.outcome.rawValue]
        for key in entry.details.keys.sorted() {
            parts.append("\(key)=\(entry.details[key] ?? "")")
        }
        if let note = entry.note, !note.isEmpty { parts.append(note) }
        return parts.joined(separator: " · ")
    }
}

enum ActivityLogFile {
    static let defaultMaxBytes = 1_000_000

    /// Appends one line. The file is created readable by its owner only. When it grows past `maxBytes` it is
    /// moved to "<name>.1", replacing the previous one, and a new file starts. Returns false on any failure;
    /// the log is best effort and must never stop the assistant.
    @discardableResult
    static func append(_ line: String, to url: URL, maxBytes: Int = ActivityLogFile.defaultMaxBytes) -> Bool {
        let fm = FileManager.default
        do {
            try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let attributes = try? fm.attributesOfItem(atPath: url.path),
               let size = (attributes[.size] as? NSNumber)?.intValue,
               size > maxBytes {
                let old = url.appendingPathExtension("1")
                try? fm.removeItem(at: old)
                try fm.moveItem(at: url, to: old)
            }
            if !fm.fileExists(atPath: url.path) {
                let made = fm.createFile(atPath: url.path, contents: nil, attributes: [.posixPermissions: 0o600])
                if !made { return false }
            }
            let handle = try FileHandle(forWritingTo: url)
            defer { try? handle.close() }
            try handle.seekToEnd()
            if let data = (line + "\n").data(using: .utf8) {
                try handle.write(contentsOf: data)
            }
            return true
        } catch {
            return false
        }
    }
}
