//! Tool schemas for the model, and reading tool calls out of a model reply. Port of `ToolSchemas.swift` and
//! `ToolCallExtractor.swift`.
//!
//! Tool ids contain a dot ("reminder.create"), but the Anthropic API only accepts letters, digits, underscores
//! and dashes in a tool name, so the name on the wire swaps the dot for an underscore. A name that comes back is
//! turned into an id only by looking it up in the catalog, never by string tricks, so the model cannot invent a
//! tool by choosing a clever name.
//!
//! Extraction decides nothing about safety. It turns blocks into structured calls, refuses anything that is not
//! plain text arguments, and notes whether the reply came after the model read outside content.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::text::sanitize;
use crate::tool::{ToolArguments, ToolRisk, ToolSpec};

pub fn wire_name(id: &str) -> String {
    id.replace('.', "_")
}

/// Wire name to catalog id, built from the catalog.
pub fn ids_by_wire_name(catalog: &BTreeMap<String, ToolSpec>) -> BTreeMap<String, String> {
    catalog.keys().map(|id| (wire_name(id), id.clone())).collect()
}

/// The `tools` entries for an Anthropic Messages request, in a stable order.
pub fn anthropic_tools(catalog: &BTreeMap<String, ToolSpec>) -> Vec<Value> {
    let mut tools = Vec::new();
    for (id, spec) in catalog {
        let mut properties = Map::new();
        for key in spec.required.iter().chain(spec.optional.iter()) {
            let mut property = Map::new();
            property.insert("type".into(), json!("string"));
            if let Some(help) = spec.argument_help.get(key) {
                property.insert("description".into(), json!(help));
            }
            properties.insert(key.clone(), Value::Object(property));
        }
        let mut description = spec.summary.clone();
        match spec.risk {
            ToolRisk::Read => {}
            ToolRisk::Draft => description.push_str(" This only prepares something for the user to read. Nothing is sent."),
            ToolRisk::Act => description.push_str(" The user must click Allow on a card before this happens."),
            ToolRisk::Critical => description.push_str(" The user must click Allow twice on a card before this happens."),
        }
        tools.push(json!({
            "name": wire_name(id),
            "description": description.trim_matches(|c| c == ' ' || c == '\t'),
            "input_schema": {
                "type": "object",
                "properties": Value::Object(properties),
                "required": spec.required,
                "additionalProperties": false,
            }
        }));
    }
    tools
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedCall {
    /// The id the API gave this tool use. The answer must carry the same id.
    pub use_id: String,
    /// The tool name exactly as the model wrote it (cleaned for display).
    pub name: String,
    /// The arguments, all plain text. Empty when `problem` is set.
    pub arguments: ToolArguments,
    /// Why this call cannot be used at all, in plain words.
    pub problem: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractedTurn {
    pub calls: Vec<ExtractedCall>,
    /// True when the reply contains results from a web search or other outside content.
    pub saw_outside_content: bool,
}

/// Most tool calls looked at in one reply. Any more are refused.
pub const MAX_CALLS_PER_TURN: usize = 3;
pub const MAX_USE_ID_LENGTH: usize = 200;

pub fn extract(content: &[Value]) -> ExtractedTurn {
    let mut calls: Vec<ExtractedCall> = Vec::new();
    let mut saw_outside = false;
    for block in content {
        let Some(kind) = block.get("type").and_then(Value::as_str) else { continue };
        if kind == "server_tool_use" || kind.ends_with("_tool_result") {
            saw_outside = true;
            continue;
        }
        if kind != "tool_use" {
            continue;
        }
        let Some(use_id) = block.get("id").and_then(Value::as_str) else { continue };
        if use_id.is_empty() || use_id.chars().count() > MAX_USE_ID_LENGTH {
            continue;
        }
        let use_id = use_id.to_string();
        let name = sanitize(block.get("name").and_then(Value::as_str).unwrap_or(""), 80);

        let refused = |problem: &str| ExtractedCall {
            use_id: use_id.clone(),
            name: name.clone(),
            arguments: ToolArguments::new(),
            problem: Some(problem.to_string()),
        };

        if calls.len() >= MAX_CALLS_PER_TURN {
            calls.push(refused("Too many actions in one reply. Ask for them one at a time."));
            continue;
        }
        let Some(input) = block.get("input").and_then(Value::as_object) else {
            calls.push(refused("The action had no readable arguments."));
            continue;
        };
        let mut keys: Vec<&String> = input.keys().collect();
        keys.sort();
        let mut arguments = ToolArguments::new();
        let mut problem: Option<String> = None;
        for key in keys {
            match input[key].as_str() {
                Some(text) => {
                    arguments.insert(key.clone(), text.to_string());
                }
                None => {
                    problem = Some(format!("\"{}\" must be plain text.", sanitize(key, 40)));
                    break;
                }
            }
        }
        if problem.is_some() {
            arguments.clear();
        }
        calls.push(ExtractedCall { use_id, name, arguments, problem });
    }
    ExtractedTurn { calls, saw_outside_content: saw_outside }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog;

    #[test]
    fn wire_names_have_no_dots_and_map_back_only_through_the_catalog() {
        let cat = catalog();
        let map = ids_by_wire_name(&cat);
        assert_eq!(wire_name("reminder.create"), "reminder_create");
        assert_eq!(map.get("reminder_create").map(String::as_str), Some("reminder.create"));
        assert!(!map.contains_key("reminder.create"));
        assert!(!map.contains_key("made_up"));
    }

    #[test]
    fn the_tool_list_is_valid_and_stable() {
        let tools = anthropic_tools(&catalog());
        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["mail_draft", "reminder_create"], "sorted by id");
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'), "{name}");
            assert_eq!(tool["input_schema"]["type"], "object");
            assert_eq!(tool["input_schema"]["additionalProperties"], false);
            assert!(!tool["description"].as_str().unwrap().is_empty());
        }
        let reminder = tools.iter().find(|t| t["name"] == "reminder_create").unwrap();
        assert!(reminder["description"].as_str().unwrap().contains("click Allow"));
        let required: Vec<&str> = reminder["input_schema"]["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required.contains(&"title") && required.contains(&"due"));
        assert_eq!(reminder["input_schema"]["properties"]["due"]["type"], "string");
        let mail = tools.iter().find(|t| t["name"] == "mail_draft").unwrap();
        assert!(mail["description"].as_str().unwrap().contains("Nothing is sent") || mail["description"].as_str().unwrap().contains("click Allow"));
    }

    fn use_block(id: &str, name: &str, input: Value) -> Value {
        json!({"type": "tool_use", "id": id, "name": name, "input": input})
    }

    #[test]
    fn plain_text_calls_are_extracted() {
        let turn = extract(&[
            json!({"type": "text", "text": "Sure"}),
            use_block("toolu_1", "reminder_create", json!({"title": "Call Mum", "due": "2027-01-15T09:20:00Z"})),
        ]);
        assert!(!turn.saw_outside_content);
        assert_eq!(turn.calls.len(), 1);
        let call = &turn.calls[0];
        assert_eq!(call.use_id, "toolu_1");
        assert_eq!(call.name, "reminder_create");
        assert_eq!(call.arguments.get("title").map(String::as_str), Some("Call Mum"));
        assert!(call.problem.is_none());
    }

    #[test]
    fn non_text_arguments_are_a_problem() {
        let turn = extract(&[use_block("a", "reminder_create", json!({"title": "x", "due": 5}))]);
        let call = &turn.calls[0];
        assert!(call.arguments.is_empty());
        assert_eq!(call.problem.as_deref(), Some("\"due\" must be plain text."));
        let nested = extract(&[use_block("b", "reminder_create", json!({"title": {"a": 1}}))]);
        assert!(nested.calls[0].problem.is_some());
    }

    #[test]
    fn missing_input_is_a_problem() {
        let turn = extract(&[json!({"type": "tool_use", "id": "a", "name": "x"})]);
        assert_eq!(turn.calls[0].problem.as_deref(), Some("The action had no readable arguments."));
    }

    #[test]
    fn blocks_without_a_usable_id_are_skipped() {
        let long = "x".repeat(MAX_USE_ID_LENGTH + 1);
        let turn = extract(&[
            json!({"type": "tool_use", "name": "x", "input": {}}),
            use_block("", "x", json!({})),
            use_block(&long, "x", json!({})),
        ]);
        assert!(turn.calls.is_empty());
    }

    #[test]
    fn only_three_calls_are_looked_at() {
        let blocks: Vec<Value> = (0..5).map(|i| use_block(&format!("t{i}"), "mail_draft", json!({}))).collect();
        let turn = extract(&blocks);
        assert_eq!(turn.calls.len(), 5, "every use id still gets an answer");
        assert!(turn.calls[..3].iter().all(|c| c.problem.is_none()));
        assert!(turn.calls[3..].iter().all(|c| c.problem.as_deref().unwrap().contains("Too many")));
    }

    #[test]
    fn search_results_mark_outside_content() {
        for kind in ["server_tool_use", "web_search_tool_result", "web_fetch_tool_result"] {
            let turn = extract(&[json!({"type": kind})]);
            assert!(turn.saw_outside_content, "{kind}");
            assert!(turn.calls.is_empty());
        }
        assert!(!extract(&[json!({"type": "text", "text": "hi"})]).saw_outside_content);
    }

    #[test]
    fn the_model_name_is_cleaned_for_display() {
        let turn = extract(&[use_block("a", "bad\u{202E}name", json!({}))]);
        assert_eq!(turn.calls[0].name, "badname");
    }
}
