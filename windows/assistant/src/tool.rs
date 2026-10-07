//! The vocabulary: what a tool is, how risky it is, and where a call came from.

use std::collections::BTreeMap;

use serde::Serialize;

/// How much a tool can change in the world. A higher value means more care.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolRisk {
    /// Looks at something. Nothing changes.
    Read,
    /// Prepares something locally. Nothing leaves the machine and nothing is committed.
    Draft,
    /// Changes something or sends something out (a reminder, an email, a calendar event).
    Act,
    /// Money, bulk or irreversible changes, sharing with other people.
    Critical,
}

impl ToolRisk {
    pub fn name(self) -> &'static str {
        match self {
            ToolRisk::Read => "read",
            ToolRisk::Draft => "draft",
            ToolRisk::Act => "act",
            ToolRisk::Critical => "critical",
        }
    }
}

/// Where a proposed tool call came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolOrigin {
    /// Typed or spoken by the user, with nothing untrusted read since.
    User,
    /// Came out of, or after reading, a file, web page, email or other content. That content is data, never
    /// instructions, so a call like this is always put in front of the user.
    UntrustedContent,
}

impl ToolOrigin {
    pub fn name(self) -> &'static str {
        match self {
            ToolOrigin::User => "user",
            ToolOrigin::UntrustedContent => "untrustedContent",
        }
    }
}

/// Arguments are always structured text: full email addresses, ISO 8601 dates with a time zone, plain
/// strings. They never depend on the language the assistant is speaking. Sorted by key, so every walk over
/// them is in the same order.
pub type ToolArguments = BTreeMap<String, String>;

/// One line on a confirmation card. The label is fixed text chosen by the app, never by the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolField {
    /// What the user reads, for example "When".
    pub label: String,
    /// Key into the call's arguments.
    pub argument: String,
    /// Free text written by a person or a model. Shown on the card as quoted content, never trusted.
    pub is_content: bool,
    /// An ISO 8601 date and time. The card shows it in the user's own time zone.
    pub is_date_time: bool,
    /// May appear in the local activity log. Off by default so personal text stays out of it.
    pub is_logged: bool,
}

impl ToolField {
    pub fn new(label: &str, argument: &str) -> Self {
        Self {
            label: label.to_string(),
            argument: argument.to_string(),
            is_content: false,
            is_date_time: false,
            is_logged: false,
        }
    }
    pub fn content(mut self) -> Self {
        self.is_content = true;
        self
    }
    pub fn date_time(mut self) -> Self {
        self.is_date_time = true;
        self
    }
    pub fn logged(mut self) -> Self {
        self.is_logged = true;
        self
    }
}

/// What a tool is, declared once. The policy and the confirmation card are built from this, not from model text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSpec {
    /// Stable contract value, for example "reminder.create". Never rename one that has shipped.
    pub id: String,
    /// Card heading chosen by the app, for example "Set a reminder".
    pub title: String,
    pub risk: ToolRisk,
    /// Argument keys that must be present and not blank.
    pub required: Vec<String>,
    /// Argument keys that may be present. Any other key is refused.
    pub optional: Vec<String>,
    /// What the confirmation card shows, in order. For tools that ask, every argument must have a field, or
    /// the call is refused: nothing runs that the card did not show.
    pub fields: Vec<ToolField>,
    /// One sentence that tells the model what the tool does. Sent to the model only.
    pub summary: String,
    /// Short help for each argument, keyed by argument name. Sent to the model only.
    pub argument_help: BTreeMap<String, String>,
}

impl ToolSpec {
    pub fn new(id: &str, title: &str, risk: ToolRisk) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            risk,
            required: Vec::new(),
            optional: Vec::new(),
            fields: Vec::new(),
            summary: String::new(),
            argument_help: BTreeMap::new(),
        }
    }
    pub fn required(mut self, keys: &[&str]) -> Self {
        self.required = keys.iter().map(|k| k.to_string()).collect();
        self
    }
    pub fn optional(mut self, keys: &[&str]) -> Self {
        self.optional = keys.iter().map(|k| k.to_string()).collect();
        self
    }
    pub fn fields(mut self, fields: Vec<ToolField>) -> Self {
        self.fields = fields;
        self
    }
    pub fn summary(mut self, summary: &str) -> Self {
        self.summary = summary.to_string();
        self
    }
    pub fn help(mut self, argument: &str, text: &str) -> Self {
        self.argument_help.insert(argument.to_string(), text.to_string());
        self
    }
}

/// A tool call the assistant wants to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub tool_id: String,
    pub arguments: ToolArguments,
    pub origin: ToolOrigin,
}

/// Tracks, for one turn of the conversation, whether the assistant has read anything it should not trust.
/// Once it has, every call in that turn is labelled untrusted, because the model can no longer be assumed
/// to be acting on the user's words alone. Make every [`ToolCall`] through [`TurnContext::make_call`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnContext {
    saw_untrusted_content: bool,
}

impl TurnContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call this whenever a file, web page, email, clipboard or other outside content enters the turn.
    pub fn note_untrusted_content(&mut self) {
        self.saw_untrusted_content = true;
    }

    pub fn origin(&self) -> ToolOrigin {
        if self.saw_untrusted_content {
            ToolOrigin::UntrustedContent
        } else {
            ToolOrigin::User
        }
    }

    pub fn make_call(&self, tool_id: &str, arguments: ToolArguments) -> ToolCall {
        ToolCall { tool_id: tool_id.to_string(), arguments, origin: self.origin() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_is_ordered() {
        assert!(ToolRisk::Read < ToolRisk::Draft && ToolRisk::Draft < ToolRisk::Act && ToolRisk::Act < ToolRisk::Critical);
    }

    #[test]
    fn a_turn_starts_trusted_and_stays_marked_once_it_reads_outside_content() {
        let mut turn = TurnContext::new();
        assert_eq!(turn.origin(), ToolOrigin::User);
        assert_eq!(turn.make_call("test.draft", ToolArguments::new()).origin, ToolOrigin::User);
        turn.note_untrusted_content();
        assert_eq!(turn.make_call("test.draft", ToolArguments::new()).origin, ToolOrigin::UntrustedContent);
        assert_eq!(TurnContext::new().origin(), ToolOrigin::User, "a fresh turn starts trusted again");
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(ToolRisk::Act.name(), "act");
        assert_eq!(ToolOrigin::UntrustedContent.name(), "untrustedContent");
    }
}
