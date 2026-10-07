//! Holds every proposed action that is waiting for a person. The only way out of "waiting" with a yes is
//! `approve` with the `Click` channel. A spoken yes, a message written by the model, or an automatic trigger
//! cannot approve anything. Voice may prompt ("Check di card"); only a click approves.

use std::collections::BTreeMap;

use crate::policy::{decide, ConfirmationCard, PolicyDecision};
use crate::tool::{ToolCall, ToolSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalChannel {
    /// A deliberate click or tap on the Allow button of the confirmation card.
    Click,
    /// A spoken "yes". Voice can prompt, it cannot approve.
    Voice,
    /// Text written by the model.
    Model,
    /// A script, shortcut or any other automatic trigger.
    Automation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingAction {
    pub id: u64,
    pub call: ToolCall,
    pub card: ConfirmationCard,
    /// Seconds since 1970.
    pub created_at: f64,
    pub expires_at: f64,
    /// Time of the first click on a critical action. `None` until then.
    pub first_click_at: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalResult {
    /// Go ahead and run this call. It has been removed from the waiting list.
    Approved(ToolCall),
    /// A critical action needs one more, separate click.
    NeedsSecondClick,
    /// Nothing runs. The text says why, in plain words.
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Proposal {
    /// The policy says this is safe to run without asking.
    Run(ToolCall),
    /// Show this action's card and wait for a click.
    Pending(PendingAction),
    /// Do not run. The text says why.
    Refused(String),
}

#[derive(Clone, Debug)]
pub struct PendingActionStore {
    items: BTreeMap<u64, PendingAction>,
    next_id: u64,
    /// How long an action waits before it expires, in seconds.
    pub time_to_live: f64,
    /// Most actions that may wait at once.
    pub max_pending: usize,
    /// A second click closer than this to the first counts as the same gesture (a double-click).
    pub minimum_second_click_gap: f64,
}

impl Default for PendingActionStore {
    fn default() -> Self {
        Self { items: BTreeMap::new(), next_id: 1, time_to_live: 600.0, max_pending: 20, minimum_second_click_gap: 0.8 }
    }
}

impl PendingActionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&PendingAction> {
        self.items.get(&id)
    }

    /// Actions still waiting, oldest first.
    pub fn waiting(&self) -> Vec<&PendingAction> {
        self.items.values().collect()
    }

    /// Entry point for every tool call the assistant wants to make.
    pub fn propose(&mut self, call: ToolCall, catalog: &BTreeMap<String, ToolSpec>, now: f64) -> Proposal {
        match decide(&call, catalog) {
            PolicyDecision::Run => Proposal::Run(call),
            PolicyDecision::Refuse(reason) => Proposal::Refused(reason),
            PolicyDecision::AskUser(card) => {
                self.remove_expired(now);
                if self.items.len() >= self.max_pending {
                    return Proposal::Refused("Too many actions are waiting. Answer or dismiss some first.".to_string());
                }
                let id = self.next_id;
                self.next_id += 1;
                let action = PendingAction {
                    id,
                    call,
                    card,
                    created_at: now,
                    expires_at: now + self.time_to_live,
                    first_click_at: None,
                };
                self.items.insert(id, action.clone());
                Proposal::Pending(action)
            }
        }
    }

    /// Only the `Click` channel can approve. Any other channel is rejected and the action keeps waiting.
    pub fn approve(&mut self, id: u64, channel: ApprovalChannel, now: f64) -> ApprovalResult {
        if channel != ApprovalChannel::Click {
            return ApprovalResult::Rejected("Only a click on the card can approve an action.".to_string());
        }
        let Some(action) = self.items.get_mut(&id) else {
            return ApprovalResult::Rejected("That action is no longer waiting.".to_string());
        };
        if now >= action.expires_at {
            self.items.remove(&id);
            return ApprovalResult::Rejected("That action expired. Ask again if you still want it.".to_string());
        }
        if action.card.needs_second_click {
            match action.first_click_at {
                None => {
                    action.first_click_at = Some(now);
                    return ApprovalResult::NeedsSecondClick;
                }
                Some(first) if now - first < self.minimum_second_click_gap => {
                    return ApprovalResult::NeedsSecondClick;
                }
                Some(_) => {}
            }
        }
        match self.items.remove(&id) {
            Some(action) => ApprovalResult::Approved(action.call),
            None => ApprovalResult::Rejected("That action is no longer waiting.".to_string()),
        }
    }

    /// The user said no. Returns false when the action was not waiting.
    pub fn deny(&mut self, id: u64) -> bool {
        self.items.remove(&id).is_some()
    }

    /// Drops actions that waited too long and returns their ids, so the app can log them.
    pub fn remove_expired(&mut self, now: f64) -> Vec<u64> {
        let gone: Vec<u64> = self.items.values().filter(|a| now >= a.expires_at).map(|a| a.id).collect();
        for id in &gone {
            self.items.remove(id);
        }
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog as real_catalog;
    use crate::tool::{ToolArguments, ToolField, ToolOrigin, ToolRisk};

    const NOW: f64 = 1_800_000_000.0;

    fn catalog() -> BTreeMap<String, ToolSpec> {
        let mut map = real_catalog();
        for spec in [
            ToolSpec::new("test.draft", "Prepare something", ToolRisk::Draft)
                .required(&["text"])
                .fields(vec![ToolField::new("Text", "text").content()]),
            ToolSpec::new("test.act", "Send something", ToolRisk::Act)
                .required(&["to"])
                .fields(vec![ToolField::new("To", "to").logged()]),
            ToolSpec::new("test.critical", "Move money", ToolRisk::Critical)
                .required(&["amount"])
                .fields(vec![ToolField::new("Amount", "amount")]),
        ] {
            map.insert(spec.id.clone(), spec);
        }
        map
    }

    fn call(id: &str, args: &[(&str, &str)]) -> ToolCall {
        let arguments: ToolArguments = args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ToolCall { tool_id: id.to_string(), arguments, origin: ToolOrigin::User }
    }

    fn act() -> ToolCall {
        call("test.act", &[("to", "ada@example.com")])
    }

    fn pending(store: &mut PendingActionStore, c: ToolCall, now: f64) -> PendingAction {
        match store.propose(c, &catalog(), now) {
            Proposal::Pending(p) => p,
            other => panic!("expected a pending action, got {other:?}"),
        }
    }

    fn rejected(r: ApprovalResult) -> String {
        match r {
            ApprovalResult::Rejected(why) => why,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn only_a_click_approves() {
        let mut store = PendingActionStore::new();
        let p = pending(&mut store, act(), NOW);
        assert_eq!(store.len(), 1);
        assert_eq!(p.call, act());
        for channel in [ApprovalChannel::Voice, ApprovalChannel::Model, ApprovalChannel::Automation] {
            rejected(store.approve(p.id, channel, NOW));
        }
        assert!(store.get(p.id).is_some(), "still waiting after those");
        assert_eq!(store.approve(p.id, ApprovalChannel::Click, NOW + 5.0), ApprovalResult::Approved(act()));
        assert!(store.is_empty(), "an approved action leaves the waiting list");
        rejected(store.approve(p.id, ApprovalChannel::Click, NOW + 6.0));
    }

    #[test]
    fn drafts_and_refusals_are_not_stored() {
        let mut store = PendingActionStore::new();
        match store.propose(call("test.draft", &[("text", "hi")]), &catalog(), NOW) {
            Proposal::Run(c) => assert_eq!(c.tool_id, "test.draft"),
            other => panic!("a draft runs straight away, got {other:?}"),
        }
        assert!(matches!(store.propose(call("nope", &[]), &catalog(), NOW), Proposal::Refused(_)));
        assert!(store.is_empty());
    }

    #[test]
    fn actions_expire() {
        let mut store = PendingActionStore::new();
        store.time_to_live = 60.0;
        let p = pending(&mut store, act(), NOW);
        assert!(rejected(store.approve(p.id, ApprovalChannel::Click, NOW + 61.0)).contains("expired"));
        assert!(store.is_empty());
        let again = pending(&mut store, act(), NOW);
        assert_eq!(store.remove_expired(NOW + 120.0), vec![again.id]);
    }

    #[test]
    fn deny_removes() {
        let mut store = PendingActionStore::new();
        let p = pending(&mut store, act(), NOW);
        assert!(store.deny(p.id) && store.is_empty());
        assert!(!store.deny(p.id));
    }

    #[test]
    fn capacity_is_limited() {
        let mut store = PendingActionStore::new();
        store.max_pending = 2;
        pending(&mut store, act(), NOW);
        pending(&mut store, act(), NOW);
        match store.propose(act(), &catalog(), NOW) {
            Proposal::Refused(why) => assert!(why.contains("Too many")),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(store.waiting().len(), 2);
    }

    #[test]
    fn ids_are_not_reused() {
        let mut store = PendingActionStore::new();
        let first = pending(&mut store, act(), NOW);
        store.deny(first.id);
        let second = pending(&mut store, act(), NOW);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn a_critical_action_needs_two_separate_clicks() {
        let mut store = PendingActionStore::new();
        let p = pending(&mut store, call("test.critical", &[("amount", "5000")]), NOW);
        assert_eq!(store.approve(p.id, ApprovalChannel::Click, NOW), ApprovalResult::NeedsSecondClick);
        assert_eq!(store.approve(p.id, ApprovalChannel::Click, NOW + 0.1), ApprovalResult::NeedsSecondClick, "a double-click counts as one");
        assert!(matches!(store.approve(p.id, ApprovalChannel::Click, NOW + 2.0), ApprovalResult::Approved(_)));
        assert!(store.is_empty());
    }

    #[test]
    fn voice_never_advances_a_critical_action() {
        let mut store = PendingActionStore::new();
        let p = pending(&mut store, call("test.critical", &[("amount", "5000")]), NOW);
        store.approve(p.id, ApprovalChannel::Voice, NOW);
        store.approve(p.id, ApprovalChannel::Voice, NOW + 5.0);
        assert_eq!(store.get(p.id).unwrap().first_click_at, None);
    }

    #[test]
    fn a_reminder_end_to_end() {
        let mut store = PendingActionStore::new();
        let later = crate::parsers::iso_utc(NOW as i64 + 3600);
        let c = call("reminder.create", &[("title", "Call Mum"), ("due", &later)]);
        let p = match store.propose(c, &real_catalog(), NOW) {
            Proposal::Pending(p) => p,
            other => panic!("a reminder waits for a click, got {other:?}"),
        };
        assert_eq!(p.card.title, "Set a reminder");
        store.approve(p.id, ApprovalChannel::Voice, NOW + 1.0);
        assert!(store.get(p.id).is_some(), "saying yes does nothing");
        let ApprovalResult::Approved(approved) = store.approve(p.id, ApprovalChannel::Click, NOW + 2.0) else {
            panic!("a click approves the reminder")
        };
        let request = crate::parsers::parse_reminder(&approved.arguments, NOW + 2.0).unwrap();
        assert_eq!(request.title, "Call Mum");
    }
}
