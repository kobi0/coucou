import Foundation

// MARK: - Reading tool calls out of a model reply (Foundation-only, testable)
//
// Works on the content blocks of an Anthropic Messages reply. It does not decide anything about safety. It
// only turns blocks into structured calls, refuses anything that is not plain text arguments, and notes
// whether the reply came after the model read outside content (a web search, for example). The policy and the
// session decide what happens next.

struct ExtractedCall: Sendable, Equatable {
    /// The id the API gave this tool use. The answer must carry the same id.
    let useId: String
    /// The tool name exactly as the model wrote it.
    let name: String
    /// The arguments, all plain text. Empty when `problem` is set.
    let arguments: ToolArguments
    /// Why this call cannot be used at all, in plain words.
    let problem: String?
}

struct ExtractedTurn: Sendable, Equatable {
    var calls: [ExtractedCall]
    /// True when the reply contains results from a web search or other outside content.
    var sawOutsideContent: Bool

    static let empty = ExtractedTurn(calls: [], sawOutsideContent: false)
}

enum ToolCallExtractor {
    /// Most tool calls looked at in one reply. Any more are refused.
    static let maxCallsPerTurn = 3
    static let maxUseIdLength = 200

    static func extract(content: [[String: Any]]) -> ExtractedTurn {
        var calls: [ExtractedCall] = []
        var sawOutside = false
        for block in content {
            guard let type = block["type"] as? String else { continue }
            if type == "server_tool_use" || type.hasSuffix("_tool_result") {
                sawOutside = true
                continue
            }
            guard type == "tool_use" else { continue }
            guard let useId = block["id"] as? String, !useId.isEmpty, useId.count <= maxUseIdLength else { continue }
            let name = ActionPolicy.sanitize((block["name"] as? String) ?? "", limit: 80)

            if calls.count >= maxCallsPerTurn {
                calls.append(ExtractedCall(useId: useId, name: name, arguments: [:],
                                           problem: "Too many actions in one reply. Ask for them one at a time."))
                continue
            }
            guard let input = block["input"] as? [String: Any] else {
                calls.append(ExtractedCall(useId: useId, name: name, arguments: [:],
                                           problem: "The action had no readable arguments."))
                continue
            }
            var arguments: ToolArguments = [:]
            var problem: String? = nil
            for key in input.keys.sorted() {
                if let text = input[key] as? String {
                    arguments[key] = text
                } else {
                    problem = "\"\(ActionPolicy.sanitize(key, limit: 40))\" must be plain text."
                    break
                }
            }
            if problem != nil { arguments = [:] }
            calls.append(ExtractedCall(useId: useId, name: name, arguments: arguments, problem: problem))
        }
        return ExtractedTurn(calls: calls, sawOutsideContent: sawOutside)
    }
}
