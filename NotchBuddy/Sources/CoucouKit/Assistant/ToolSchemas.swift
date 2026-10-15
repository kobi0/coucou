import Foundation

// MARK: - Tool schemas for the model (Foundation-only, testable)
//
// Turns the catalog into the tool list that is sent to the model. Tool ids contain a dot ("reminder.create"),
// but the Anthropic API only accepts letters, digits, underscores and dashes in a tool name, so the name on
// the wire swaps the dot for an underscore. A name that comes back is turned into an id only by looking it up
// in the catalog, never by string tricks, so the model cannot invent a tool by choosing a clever name.

enum ToolSchemas {
    static func wireName(_ id: String) -> String {
        id.replacingOccurrences(of: ".", with: "_")
    }

    /// Wire name to catalog id, built from the catalog.
    static func idsByWireName(_ catalog: [String: ToolSpec]) -> [String: String] {
        var map: [String: String] = [:]
        for id in catalog.keys { map[wireName(id)] = id }
        return map
    }

    /// The `tools` entries for an Anthropic Messages request, in a stable order.
    static func anthropicTools(_ catalog: [String: ToolSpec]) -> [[String: Any]] {
        var tools: [[String: Any]] = []
        for id in catalog.keys.sorted() {
            guard let spec = catalog[id] else { continue }
            var properties: [String: Any] = [:]
            for key in spec.required + spec.optional {
                var property: [String: Any] = ["type": "string"]
                if let help = spec.argumentHelp[key] { property["description"] = help }
                properties[key] = property
            }
            var description = spec.summary
            switch spec.risk {
            case .read:
                break
            case .draft:
                description += " This only prepares something for the user to read. Nothing is sent."
            case .act:
                description += " The user must click Allow on a card before this happens."
            case .critical:
                description += " The user must click Allow twice on a card before this happens."
            }
            let schema: [String: Any] = [
                "type": "object",
                "properties": properties,
                "required": spec.required,
                "additionalProperties": false
            ]
            tools.append([
                "name": wireName(id),
                "description": description.trimmingCharacters(in: .whitespaces),
                "input_schema": schema
            ])
        }
        return tools
    }
}
