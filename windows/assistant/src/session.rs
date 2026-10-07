//! The ledger for one conversation. Port of `AssistantSession.swift`.
//!
//! It keeps track of which tool calls the model made, which are waiting for a click, what became of each, and
//! what the model must be told next. It never runs a tool. It hands back "run this" steps and the app runs
//! them, then reports the result with `finish`. Everything the model is told about an outcome is written here
//! from fixed words and the tool's own title, never from text the model wrote.
//!
//! Once the conversation has read outside content (web search results, an attached file or window), every later
//! call in it is labelled untrusted and asks the user, even a draft. The mark stays until the conversation
//! ends, because the model still carries what it read.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::catalog::catalog as default_catalog;
use crate::log::{entry, ActivityEntry, ActivityOutcome};
use crate::policy::ConfirmationCard;
use crate::schema::{ids_by_wire_name, ExtractedTurn};
use crate::store::{ApprovalChannel, ApprovalResult, PendingAction, PendingActionStore, Proposal};
use crate::tool::{ToolCall, ToolSpec, TurnContext};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub ok: bool,
    /// Plain words for the user and the model. Must not contain text written by the model.
    pub message: String,
}

/// What the model is told about one of its tool calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResultText {
    pub use_id: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolStep {
    /// Safe to run now. Run it, then call `finish`.
    Run { use_id: String, call: ToolCall },
    /// Waiting for a click. The app shows `action.card`.
    Waiting { use_id: String, action: PendingAction },
    /// Not done. The text says why.
    Refused { use_id: String, reason: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalStep {
    /// A click approved it. Run it, then call `finish`.
    Run { use_id: String, call: ToolCall },
    /// A critical action needs a second, separate click.
    NeedsSecondClick,
    Rejected(String),
}

/// What the UI needs to draw one waiting card.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingView {
    pub id: u64,
    pub card: ConfirmationCard,
    /// Seconds since 1970.
    pub expires_at: f64,
}

#[derive(Clone, Debug)]
pub struct AssistantSession {
    catalog: BTreeMap<String, ToolSpec>,
    store: PendingActionStore,
    /// True once the conversation has read outside content.
    tainted: bool,
    /// Tool uses the model has not yet been answered about, in order.
    order: Vec<String>,
    calls: BTreeMap<String, ToolCall>,
    results: BTreeMap<String, String>,
    waiting_links: BTreeMap<u64, String>,
    /// Tool uses whose answer has already gone to the model.
    reported: BTreeSet<String>,
    notes: Vec<String>,
    log_entries: Vec<ActivityEntry>,
}

impl Default for AssistantSession {
    fn default() -> Self {
        Self::new()
    }
}

impl AssistantSession {
    pub fn new() -> Self {
        Self::with_catalog(default_catalog())
    }

    pub fn with_catalog(catalog: BTreeMap<String, ToolSpec>) -> Self {
        Self {
            catalog,
            store: PendingActionStore::new(),
            tainted: false,
            order: Vec::new(),
            calls: BTreeMap::new(),
            results: BTreeMap::new(),
            waiting_links: BTreeMap::new(),
            reported: BTreeSet::new(),
            notes: Vec::new(),
            log_entries: Vec::new(),
        }
    }

    pub fn catalog(&self) -> &BTreeMap<String, ToolSpec> {
        &self.catalog
    }

    pub fn is_tainted(&self) -> bool {
        self.tainted
    }

    /// True once the model has made any tool call in this conversation.
    pub fn has_tool_history(&self) -> bool {
        !self.calls.is_empty()
    }

    /// Actions still waiting for a click and not yet expired, oldest first.
    pub fn waiting(&self, now: f64) -> Vec<&PendingAction> {
        self.store.waiting().into_iter().filter(|a| now < a.expires_at).collect()
    }

    /// The same, shaped for the UI.
    pub fn waiting_views(&self, now: f64) -> Vec<PendingView> {
        self.waiting(now)
            .into_iter()
            .map(|a| PendingView { id: a.id, card: a.card.clone(), expires_at: a.expires_at })
            .collect()
    }

    /// Call when a file, window or other outside content is attached to the conversation.
    pub fn note_outside_content(&mut self) {
        self.tainted = true;
    }

    // ── A reply arrives ────────────────────────────────────────────────────────────────────────────────────

    pub fn begin_turn(&mut self, turn: &ExtractedTurn, now: f64) -> Vec<ToolStep> {
        if turn.saw_outside_content {
            self.tainted = true;
        }
        self.sweep_expired(now);
        let wire_to_id = ids_by_wire_name(&self.catalog);
        let mut context = TurnContext::new();
        if self.tainted {
            context.note_untrusted_content();
        }

        let mut steps = Vec::new();
        for extracted in &turn.calls {
            let use_id = extracted.use_id.clone();
            if self.calls.contains_key(&use_id) {
                continue;
            }
            self.order.push(use_id.clone());

            if let Some(problem) = &extracted.problem {
                let call = ToolCall {
                    tool_id: extracted.name.clone(),
                    arguments: Default::default(),
                    origin: context.origin(),
                };
                self.calls.insert(use_id.clone(), call.clone());
                self.refuse(&use_id, &call, problem, now);
                steps.push(ToolStep::Refused { use_id, reason: problem.clone() });
                continue;
            }

            let tool_id = wire_to_id.get(&extracted.name).cloned().unwrap_or_else(|| extracted.name.clone());
            let call = context.make_call(&tool_id, extracted.arguments.clone());
            self.calls.insert(use_id.clone(), call.clone());
            match self.store.propose(call.clone(), &self.catalog, now) {
                Proposal::Run(allowed) => steps.push(ToolStep::Run { use_id, call: allowed }),
                Proposal::Pending(action) => {
                    self.waiting_links.insert(action.id, use_id.clone());
                    self.record(&use_id, ActivityOutcome::Proposed, None, now);
                    steps.push(ToolStep::Waiting { use_id, action });
                }
                Proposal::Refused(reason) => {
                    self.refuse(&use_id, &call, &reason, now);
                    steps.push(ToolStep::Refused { use_id, reason });
                }
            }
        }
        steps
    }

    // ── The user answers ───────────────────────────────────────────────────────────────────────────────────

    /// Only a click approves. Any other channel is rejected and the action keeps waiting.
    pub fn approve(&mut self, id: u64, channel: ApprovalChannel, now: f64) -> ApprovalStep {
        self.sweep_expired(now);
        match self.store.approve(id, channel, now) {
            ApprovalResult::Approved(call) => match self.waiting_links.remove(&id) {
                Some(use_id) => {
                    self.record(&use_id, ActivityOutcome::Approved, None, now);
                    ApprovalStep::Run { use_id, call }
                }
                None => ApprovalStep::Rejected("That action is no longer waiting.".to_string()),
            },
            ApprovalResult::NeedsSecondClick => ApprovalStep::NeedsSecondClick,
            ApprovalResult::Rejected(message) => ApprovalStep::Rejected(message),
        }
    }

    pub fn deny(&mut self, id: u64, now: f64) {
        if !self.store.deny(id) {
            return;
        }
        let Some(use_id) = self.waiting_links.remove(&id) else { return };
        self.record(&use_id, ActivityOutcome::Denied, None, now);
        let text = format!("The user declined \"{}\". Nothing was done.", self.title_of(&use_id));
        self.settle(&use_id, text);
    }

    /// Reports what happened when the app ran a tool.
    pub fn finish(&mut self, use_id: &str, ok: bool, message: &str, now: f64) {
        if !self.calls.contains_key(use_id) || self.results.contains_key(use_id) {
            return;
        }
        self.record(use_id, if ok { ActivityOutcome::Ran } else { ActivityOutcome::Failed }, if ok { None } else { Some(message) }, now);
        let name = self.title_of(use_id);
        let text = if ok {
            format!("Done: \"{name}\". {message}")
        } else {
            format!("It did not work: \"{name}\". {message}")
        };
        self.settle(use_id, text);
    }

    pub fn sweep_expired(&mut self, now: f64) {
        for id in self.store.remove_expired(now) {
            let Some(use_id) = self.waiting_links.remove(&id) else { continue };
            self.record(&use_id, ActivityOutcome::Expired, None, now);
            let text = format!("The user did not answer in time, so \"{}\" was not done.", self.title_of(&use_id));
            self.settle(&use_id, text);
        }
    }

    /// Ends the conversation. Anything still waiting is dropped and logged as declined.
    pub fn end_conversation(&mut self, now: f64) {
        let ids: Vec<u64> = self.store.waiting().iter().map(|a| a.id).collect();
        for id in ids {
            self.deny(id, now);
        }
        self.order.clear();
        self.calls.clear();
        self.results.clear();
        self.waiting_links.clear();
        self.reported.clear();
        self.notes.clear();
        self.tainted = false;
        self.store = PendingActionStore::new();
    }

    // ── What the model is told next ────────────────────────────────────────────────────────────────────────

    /// The answers the next request must carry: first the tool results (one for every tool use not yet
    /// answered, in order), then short notes about earlier requests that were settled since. This changes
    /// nothing: call `mark_feedback_sent` once the request has gone through. If the request fails, the same
    /// answers are offered again.
    pub fn feedback(&self) -> (Vec<ToolResultText>, Vec<String>) {
        let blocks = self
            .order
            .iter()
            .map(|use_id| ToolResultText {
                use_id: use_id.clone(),
                text: self
                    .results
                    .get(use_id)
                    .cloned()
                    .unwrap_or_else(|| "The user has not answered yet. Nothing has been done.".to_string()),
            })
            .collect();
        (blocks, self.notes.clone())
    }

    pub fn mark_feedback_sent(&mut self) {
        for use_id in self.order.drain(..) {
            self.reported.insert(use_id);
        }
        self.notes.clear();
    }

    /// Log entries written since the last call. The app appends them to the activity log file.
    pub fn take_log(&mut self) -> Vec<ActivityEntry> {
        std::mem::take(&mut self.log_entries)
    }

    // ── Helpers ────────────────────────────────────────────────────────────────────────────────────────────

    fn title_of(&self, use_id: &str) -> String {
        self.calls
            .get(use_id)
            .and_then(|call| self.catalog.get(&call.tool_id))
            .map(|spec| spec.title.clone())
            .unwrap_or_else(|| "an action".to_string())
    }

    fn refuse(&mut self, use_id: &str, call: &ToolCall, reason: &str, now: f64) {
        let e = entry(call, self.catalog.get(&call.tool_id), ActivityOutcome::Refused, now, Some(reason));
        self.log_entries.push(e);
        self.settle(use_id, format!("The app did not do this: {reason}"));
    }

    fn record(&mut self, use_id: &str, outcome: ActivityOutcome, note: Option<&str>, now: f64) {
        let Some(call) = self.calls.get(use_id) else { return };
        let e = entry(call, self.catalog.get(&call.tool_id), outcome, now, note);
        self.log_entries.push(e);
    }

    /// Stores the final answer for a tool use. When the model has already been told "not answered yet", the
    /// final answer goes out as a note with the next request instead.
    fn settle(&mut self, use_id: &str, text: String) {
        if self.reported.contains(use_id) {
            self.notes.push(format!("Update from the app about an earlier request: {text}"));
        }
        self.results.insert(use_id.to_string(), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::json_line;
    use crate::parsers::{iso_utc, parse_iso8601};
    use crate::schema::extract;
    use serde_json::{json, Value};

    const NOW: f64 = 1_800_000_000.0;

    fn tool_use(id: &str, name: &str, input: Value) -> Value {
        json!({"type": "tool_use", "id": id, "name": name, "input": input})
    }

    fn web_result() -> Value {
        json!({"type": "web_search_tool_result", "tool_use_id": "srv1", "content": []})
    }

    fn reminder_turn(id: &str) -> ExtractedTurn {
        extract(&[tool_use(id, "reminder_create", json!({"title": "Call Ada", "due": iso_utc(NOW as i64 + 3600)}))])
    }

    fn mail_turn(id: &str, outside: bool) -> ExtractedTurn {
        let mut blocks = Vec::new();
        if outside {
            blocks.push(web_result());
        }
        blocks.push(tool_use(id, "mail_draft", json!({"to": "ada@example.com", "subject": "Hello", "body": "Hi Ada"})));
        extract(&blocks)
    }

    fn waiting_one(session: &mut AssistantSession, turn: &ExtractedTurn) -> (String, PendingAction) {
        match session.begin_turn(turn, NOW).into_iter().next() {
            Some(ToolStep::Waiting { use_id, action }) => (use_id, action),
            other => panic!("expected a waiting action, got {other:?}"),
        }
    }

    fn outcomes(entries: &[ActivityEntry]) -> String {
        entries.iter().map(|e| e.outcome.name()).collect::<Vec<_>>().join(",")
    }

    #[test]
    fn waiting_approving_finishing() {
        let mut session = AssistantSession::new();
        let (use_id, action) = waiting_one(&mut session, &reminder_turn("t1"));
        assert_eq!(use_id, "t1");
        assert_eq!(action.card.title, "Set a reminder");
        assert!(!action.card.from_untrusted_content);
        assert_eq!(session.waiting(NOW).len(), 1);

        assert!(matches!(session.approve(action.id, ApprovalChannel::Voice, NOW), ApprovalStep::Rejected(_)), "a spoken yes does not approve");
        assert!(matches!(session.approve(action.id, ApprovalChannel::Model, NOW), ApprovalStep::Rejected(_)), "the model cannot approve");
        assert_eq!(session.waiting(NOW).len(), 1);

        match session.approve(action.id, ApprovalChannel::Click, NOW) {
            ApprovalStep::Run { use_id, call } => {
                assert_eq!(use_id, "t1");
                assert_eq!(call.tool_id, "reminder.create");
            }
            other => panic!("a click runs it, got {other:?}"),
        }
        assert!(session.waiting(NOW).is_empty());
        assert!(matches!(session.approve(action.id, ApprovalChannel::Click, NOW), ApprovalStep::Rejected(_)));

        session.finish("t1", true, "It will show as a notification.", NOW);
        assert_eq!(session.feedback().0[0].text, "Done: \"Set a reminder\". It will show as a notification.");

        let log = session.take_log();
        assert_eq!(outcomes(&log), "proposed,approved,ran");
        assert_eq!(log[0].details.keys().cloned().collect::<Vec<_>>(), vec!["due"], "only the time is logged");
        let lines: String = log.iter().filter_map(json_line).collect();
        assert!(!lines.contains("Call Ada"), "the title stays out of the log");
        assert!(session.take_log().is_empty());
    }

    #[test]
    fn refusals() {
        let mut session = AssistantSession::new();
        let steps = session.begin_turn(&extract(&[tool_use("u1", "wire_money", json!({"amount": "5"}))]), NOW);
        match &steps[0] {
            ToolStep::Refused { reason, .. } => assert_eq!(reason, "Unknown tool: wire_money"),
            other => panic!("unknown tool is refused, got {other:?}"),
        }
        assert_eq!(session.feedback().0[0].text, "The app did not do this: Unknown tool: wire_money");

        let vague = session.begin_turn(&extract(&[tool_use("u2", "reminder_create", json!({"title": "x", "due": "tomorrow"}))]), NOW);
        assert!(matches!(vague[0], ToolStep::Waiting { .. }), "the policy accepts the shape");
        assert!(parse_iso8601("tomorrow").is_none(), "the strict parser refuses the time when the app runs it");

        let twice = session.begin_turn(
            &extract(&[
                tool_use("d1", "mail_draft", json!({"to": "a@b.com", "subject": "s", "body": "b"})),
                tool_use("d1", "mail_draft", json!({"to": "a@b.com", "subject": "s", "body": "b"})),
            ]),
            NOW,
        );
        assert_eq!(twice.len(), 1, "the same tool use id twice counts once");
    }

    #[test]
    fn a_problem_in_the_arguments_is_refused_and_answered() {
        let mut session = AssistantSession::new();
        let steps = session.begin_turn(&extract(&[tool_use("n", "reminder_create", json!({"title": 5, "due": "x"}))]), NOW);
        assert!(matches!(&steps[0], ToolStep::Refused { reason, .. } if reason == "\"title\" must be plain text."));
        assert_eq!(session.feedback().0[0].text, "The app did not do this: \"title\" must be plain text.");
    }

    #[test]
    fn a_safe_draft_runs() {
        let mut session = AssistantSession::new();
        match &session.begin_turn(&mail_turn("m1", false), NOW)[0] {
            ToolStep::Run { call, .. } => assert_eq!(call.tool_id, "mail.draft"),
            other => panic!("a draft runs without asking, got {other:?}"),
        }
    }

    #[test]
    fn untrusted_content() {
        let mut session = AssistantSession::new();
        let (_, action) = waiting_one(&mut session, &mail_turn("m1", true));
        assert!(action.card.from_untrusted_content, "the card says where it came from");
        assert!(matches!(session.begin_turn(&mail_turn("m2", false), NOW)[0], ToolStep::Waiting { .. }), "the mark stays");

        let mut attached = AssistantSession::new();
        attached.note_outside_content();
        assert!(matches!(attached.begin_turn(&mail_turn("m3", false), NOW)[0], ToolStep::Waiting { .. }), "an attached file or window also marks it");
    }

    #[test]
    fn what_the_model_is_told() {
        let mut session = AssistantSession::new();
        let (_, action) = waiting_one(&mut session, &reminder_turn("t1"));
        let first = session.feedback();
        assert_eq!(first.0[0].text, "The user has not answered yet. Nothing has been done.");
        assert_eq!(session.feedback().0, first.0, "asking twice gives the same answer");

        session.mark_feedback_sent();
        assert!(session.feedback().0.is_empty());

        if let ApprovalStep::Run { .. } = session.approve(action.id, ApprovalChannel::Click, NOW) {
            session.finish("t1", true, "Done it.", NOW);
        }
        let late = session.feedback();
        assert!(late.0.is_empty(), "no second answer for the same tool use");
        assert_eq!(late.1[0], "Update from the app about an earlier request: Done: \"Set a reminder\". Done it.");
    }

    #[test]
    fn a_no_is_reported_and_logged() {
        let mut session = AssistantSession::new();
        let (_, pending) = waiting_one(&mut session, &reminder_turn("t2"));
        session.deny(pending.id, NOW);
        assert_eq!(session.feedback().0[0].text, "The user declined \"Set a reminder\". Nothing was done.");
        assert_eq!(outcomes(&session.take_log()), "proposed,denied");
    }

    #[test]
    fn an_old_card_expires() {
        let mut session = AssistantSession::new();
        let (_, pending) = waiting_one(&mut session, &reminder_turn("t3"));
        let later = NOW + 601.0;
        session.sweep_expired(later);
        assert!(session.waiting(later).is_empty());
        assert_eq!(session.feedback().0[0].text, "The user did not answer in time, so \"Set a reminder\" was not done.");
        match session.approve(pending.id, ApprovalChannel::Click, later) {
            ApprovalStep::Rejected(message) => assert_eq!(message, "That action is no longer waiting."),
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn a_failure_is_told_plainly() {
        let mut session = AssistantSession::new();
        let (_, action) = waiting_one(&mut session, &reminder_turn("t4"));
        assert!(matches!(session.approve(action.id, ApprovalChannel::Click, NOW), ApprovalStep::Run { .. }));
        session.finish("t4", false, "The time was not understood.", NOW);
        assert_eq!(session.feedback().0[0].text, "It did not work: \"Set a reminder\". The time was not understood.");
        session.finish("t4", true, "again", NOW);
        assert_eq!(session.feedback().0[0].text, "It did not work: \"Set a reminder\". The time was not understood.", "the first answer stands");
        assert_eq!(outcomes(&session.take_log()), "proposed,approved,failed");
    }

    #[test]
    fn ending_a_conversation() {
        let mut session = AssistantSession::new();
        let _ = session.begin_turn(&mail_turn("m1", true), NOW);
        assert!(session.is_tainted() && session.waiting(NOW).len() == 1);
        session.end_conversation(NOW);
        assert!(session.waiting(NOW).is_empty());
        assert!(!session.is_tainted());
        assert!(!session.has_tool_history());
        assert_eq!(outcomes(&session.take_log()), "proposed,denied");
        assert!(matches!(session.begin_turn(&mail_turn("m9", false), NOW)[0], ToolStep::Run { .. }), "a new conversation starts trusted");
    }

    #[test]
    fn the_views_are_ready_for_the_ui() {
        let mut session = AssistantSession::new();
        let _ = waiting_one(&mut session, &reminder_turn("t5"));
        let views = session.waiting_views(NOW);
        assert_eq!(views.len(), 1);
        let json = serde_json::to_value(&views[0]).unwrap();
        assert_eq!(json["card"]["title"], "Set a reminder");
        assert!(json["expiresAt"].is_number());
        assert!(session.waiting_views(NOW + 601.0).is_empty(), "an expired card is not shown");
    }
}
