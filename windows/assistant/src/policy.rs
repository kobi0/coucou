//! Decides, from the tool's declared spec and where the call came from, whether a call may run, must be put in
//! front of the user, or is refused. The confirmation card is built here from the spec's fixed labels and the
//! call's structured arguments. Text written by the model never becomes a label, a title or a risk level, so the
//! model cannot disguise what a click does. For tools that ask, an argument with no card field is refused, so
//! nothing runs that the card did not show, and anything cut short on the card is counted and flagged.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::parsers::{format_utc, iso_utc, parse_iso8601};
use crate::text::{is_visibly_empty, sanitize, strip_hidden, truncate};
use crate::tool::{ToolCall, ToolOrigin, ToolRisk, ToolSpec};

/// Longest argument value accepted at all, in UTF-8 bytes.
pub const MAX_ARGUMENT_BYTES: usize = 20_000;
/// Longest value shown on a card, in characters (Unicode scalars).
pub const MAX_DISPLAY_LENGTH: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    /// A short value chosen from a known set or checked by a parser, such as an address.
    Text,
    /// Free text written by a person or a model. Show it as quoted content.
    Content,
    /// A moment in time. `iso` carries it so the island can show it in the user's own time zone.
    DateTime,
}

/// One line of a confirmation card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationLine {
    pub label: String,
    pub value: String,
    pub kind: LineKind,
    /// How many characters were cut from the value to fit the card. Zero when all of it is shown. When it is
    /// not zero, the card must say so, for example "and 1,450 more characters".
    pub hidden_characters: usize,
    /// For a date-time line: the same moment as ISO 8601 text in UTC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationCard {
    pub title: String,
    pub risk: ToolRisk,
    pub lines: Vec<ConfirmationLine>,
    /// True when the call came out of, or after reading, a file, web page or email. Show that clearly.
    pub from_untrusted_content: bool,
    /// True for critical tools: the user must click twice.
    pub needs_second_click: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Safe to run without asking.
    Run,
    /// Put this card in front of the user. Nothing runs until a click approves it.
    AskUser(ConfirmationCard),
    /// Do not run. The text says why, in plain words.
    Refuse(String),
}

pub fn decide(call: &ToolCall, catalog: &BTreeMap<String, ToolSpec>) -> PolicyDecision {
    let Some(spec) = catalog.get(&call.tool_id) else {
        return PolicyDecision::Refuse(format!("Unknown tool: {}", sanitize(&call.tool_id, 60)));
    };
    if let Some(problem) = validate(call, spec) {
        return PolicyDecision::Refuse(problem);
    }
    let untrusted = call.origin == ToolOrigin::UntrustedContent;
    let needs_card = match spec.risk {
        ToolRisk::Read => false,
        ToolRisk::Draft => untrusted,
        ToolRisk::Act | ToolRisk::Critical => true,
    };
    if needs_card {
        let shown: BTreeSet<&str> = spec.fields.iter().map(|f| f.argument.as_str()).collect();
        for key in call.arguments.keys() {
            if !shown.contains(key.as_str()) {
                return PolicyDecision::Refuse(format!(
                    "\"{}\" would not be shown on the card for {}.",
                    sanitize(key, 40),
                    spec.id
                ));
            }
        }
    }
    match spec.risk {
        ToolRisk::Read => PolicyDecision::Run,
        ToolRisk::Draft if untrusted => PolicyDecision::AskUser(make_card(spec, call, false)),
        ToolRisk::Draft => PolicyDecision::Run,
        ToolRisk::Act => PolicyDecision::AskUser(make_card(spec, call, false)),
        ToolRisk::Critical => PolicyDecision::AskUser(make_card(spec, call, true)),
    }
}

/// Returns a plain-words problem, or `None` when the arguments match the spec exactly.
pub fn validate(call: &ToolCall, spec: &ToolSpec) -> Option<String> {
    for key in call.arguments.keys() {
        if !spec.required.contains(key) && !spec.optional.contains(key) {
            return Some(format!("Unexpected argument \"{}\" for {}.", sanitize(key, 40), spec.id));
        }
    }
    for (key, value) in &call.arguments {
        if value.len() > MAX_ARGUMENT_BYTES {
            return Some(format!("\"{}\" is too long for {}.", sanitize(key, 40), spec.id));
        }
    }
    for key in &spec.required {
        let value = strip_hidden(call.arguments.get(key).map(String::as_str).unwrap_or(""), false);
        if is_visibly_empty(&value) {
            return Some(format!("Missing \"{key}\" for {}.", spec.id));
        }
    }
    None
}

pub fn make_card(spec: &ToolSpec, call: &ToolCall, second_click: bool) -> ConfirmationCard {
    let mut lines = Vec::new();
    for field in &spec.fields {
        let Some(raw) = call.arguments.get(&field.argument) else { continue };
        if field.is_date_time {
            if let Some(moment) = parse_iso8601(raw) {
                lines.push(ConfirmationLine {
                    label: field.label.clone(),
                    value: format_utc(moment),
                    kind: LineKind::DateTime,
                    hidden_characters: 0,
                    iso: Some(iso_utc(moment.unix_timestamp())),
                });
                continue;
            }
        }
        let (value, hidden) = truncate(&sanitize(raw, usize::MAX), MAX_DISPLAY_LENGTH);
        if value.is_empty() {
            continue;
        }
        lines.push(ConfirmationLine {
            label: field.label.clone(),
            value,
            kind: if field.is_content { LineKind::Content } else { LineKind::Text },
            hidden_characters: hidden,
            iso: None,
        });
    }
    ConfirmationCard {
        title: spec.title.clone(),
        risk: spec.risk,
        lines,
        from_untrusted_content: call.origin == ToolOrigin::UntrustedContent,
        needs_second_click: second_click,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog as real_catalog;
    use crate::tool::{ToolArguments, ToolField};

    fn test_catalog() -> BTreeMap<String, ToolSpec> {
        let mut map = real_catalog();
        let specs = [
            ToolSpec::new("test.read", "Look at something", ToolRisk::Read)
                .optional(&["what"])
                .fields(vec![ToolField::new("What", "what")]),
            ToolSpec::new("test.draft", "Prepare something", ToolRisk::Draft)
                .required(&["text"])
                .fields(vec![ToolField::new("Text", "text").content()]),
            ToolSpec::new("test.act", "Send something", ToolRisk::Act)
                .required(&["to"])
                .optional(&["token"])
                .fields(vec![ToolField::new("To", "to").logged(), ToolField::new("Secret note", "note").content()]),
            ToolSpec::new("test.critical", "Move money", ToolRisk::Critical)
                .required(&["amount"])
                .fields(vec![ToolField::new("Amount", "amount")]),
        ];
        for spec in specs {
            map.insert(spec.id.clone(), spec);
        }
        map
    }

    fn call(id: &str, args: &[(&str, &str)], origin: ToolOrigin) -> ToolCall {
        let arguments: ToolArguments = args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ToolCall { tool_id: id.to_string(), arguments, origin }
    }
    fn user(id: &str, args: &[(&str, &str)]) -> ToolCall {
        call(id, args, ToolOrigin::User)
    }
    fn refusal(c: &ToolCall) -> String {
        match decide(c, &test_catalog()) {
            PolicyDecision::Refuse(why) => why,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
    fn card(c: &ToolCall) -> ConfirmationCard {
        match decide(c, &test_catalog()) {
            PolicyDecision::AskUser(card) => card,
            other => panic!("expected a card, got {other:?}"),
        }
    }

    #[test]
    fn who_may_run_without_asking() {
        let cat = test_catalog();
        assert!(refusal(&user("nope", &[])).contains("Unknown tool"));
        assert_eq!(decide(&user("test.read", &[]), &cat), PolicyDecision::Run, "read runs");
        assert_eq!(decide(&user("test.draft", &[("text", "hello")]), &cat), PolicyDecision::Run, "a draft from the user runs");
        let from_file = card(&call("test.draft", &[("text", "hello")], ToolOrigin::UntrustedContent));
        assert!(from_file.from_untrusted_content, "a draft that came from a file asks, and says so");
        let act = card(&user("test.act", &[("to", "ada@example.com")]));
        assert_eq!(act.title, "Send something", "card title comes from the spec");
        assert!(!act.needs_second_click);
        assert_eq!(act.risk, ToolRisk::Act);
        assert!(card(&user("test.critical", &[("amount", "5000")])).needs_second_click);
    }

    #[test]
    fn argument_checks() {
        assert!(refusal(&user("test.act", &[("to", "a@b.co"), ("title", "Totally safe")])).contains("Unexpected argument"));
        assert!(refusal(&user("test.act", &[])).contains("Missing"));
        assert!(refusal(&user("test.act", &[("to", "   \n ")])).contains("Missing"));
        let huge = "a".repeat(MAX_ARGUMENT_BYTES + 1);
        assert!(refusal(&user("test.act", &[("to", &huge)])).contains("too long"));
        assert!(refusal(&user("test.act", &[("to", "\u{200B}\u{200B}")])).contains("Missing"), "only invisible characters");
        let emoji = "\u{1F600}".repeat(6_000); // 6,000 characters but 24,000 bytes
        assert!(refusal(&user("test.act", &[("to", &emoji)])).contains("too long"), "the size limit counts bytes");
    }

    #[test]
    fn nothing_runs_that_the_card_did_not_show() {
        let why = refusal(&user("test.act", &[("to", "ada@example.com"), ("token", "SECRET")]));
        assert!(why.contains("would not be shown"));
        assert!(!why.contains("SECRET"), "the refusal does not repeat its value");
        assert_eq!(card(&user("test.act", &[("to", "ada@example.com")])).lines[0].label, "To");
    }

    #[test]
    fn hidden_characters_never_reach_the_card() {
        let sneaky = "ada@example.com\u{202E}moc.live@evil\nBcc: x@y.com\u{200B}";
        let shown = &card(&user("test.act", &[("to", sneaky)])).lines[0].value;
        assert!(!shown.contains('\u{202E}') && !shown.contains('\u{200B}') && !shown.contains('\n'));
        assert!(shown.contains('⏎'));
    }

    #[test]
    fn a_cut_value_says_how_much_was_hidden() {
        let long = "x".repeat(900);
        let c = card(&call("test.draft", &[("text", &long)], ToolOrigin::UntrustedContent));
        assert_eq!(c.lines[0].hidden_characters, 400);
        assert_eq!(c.lines[0].value.chars().count(), MAX_DISPLAY_LENGTH + 1);
        assert_eq!(c.lines[0].kind, LineKind::Content);
        let short = card(&call("test.draft", &[("text", "short")], ToolOrigin::UntrustedContent));
        assert_eq!(short.lines[0].hidden_characters, 0);
    }

    #[test]
    fn dates_on_the_card() {
        let cat = real_catalog();
        let remind = user("reminder.create", &[("title", "Call Mum"), ("due", "2026-10-07T16:00:00+01:00")]);
        let PolicyDecision::AskUser(c) = decide(&remind, &cat) else { panic!("a reminder gets a card") };
        let when = c.lines.iter().find(|l| l.label == "When").unwrap();
        assert_eq!(when.value, "Wed 7 Oct 2026, 15:00 UTC");
        assert_eq!(when.kind, LineKind::DateTime);
        assert_eq!(when.iso.as_deref(), Some("2026-10-07T15:00:00Z"), "the island shows this in the user's own zone");
        let title = c.lines.iter().find(|l| l.label == "Reminder").unwrap();
        assert_eq!(title.kind, LineKind::Content);

        let vague = user("reminder.create", &[("title", "x"), ("due", "tomorrow at 4")]);
        let PolicyDecision::AskUser(c) = decide(&vague, &cat) else { panic!("an unreadable time still gets a card") };
        let when = c.lines.iter().find(|l| l.label == "When").unwrap();
        assert_eq!(when.value, "tomorrow at 4", "a time that cannot be read is shown as written");
        assert_eq!(when.iso, None);
    }

    #[test]
    fn a_card_serialises_for_the_island() {
        let c = card(&user("test.act", &[("to", "ada@example.com")]));
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["title"], "Send something");
        assert_eq!(json["risk"], "act");
        assert_eq!(json["fromUntrustedContent"], false);
        assert_eq!(json["needsSecondClick"], false);
        assert_eq!(json["lines"][0]["label"], "To");
        assert_eq!(json["lines"][0]["kind"], "text");
        assert_eq!(json["lines"][0]["hiddenCharacters"], 0);
        assert!(json["lines"][0].get("iso").is_none());
    }
}
