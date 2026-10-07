//! The assistant core, shared by the Windows and Linux app.
//!
//! The rule that shapes everything here: the assistant may read and draft on its own, but anything that sends,
//! schedules, deletes, shares or costs money waits for an explicit click. This crate decides what may run,
//! builds the confirmation card from fixed labels (never from model text), holds the actions waiting for a
//! click, and keeps the ledger of what the model is owed. It never runs a tool and never touches the network.
//!
//! It is the same design as the Swift core in NotchBuddy/Sources/CoucouKit/Assistant, kept in step by hand.
//! The two share their test vectors by copy, so a rule that changes in one must change in the other.

pub mod catalog;
pub mod exec;
pub mod log;
pub mod parsers;
pub mod policy;
pub mod reminders;
pub mod schema;
pub mod session;
pub mod store;
pub mod text;
pub mod tool;

pub use catalog::catalog;
pub use policy::{decide, ConfirmationCard, ConfirmationLine, LineKind, PolicyDecision};
pub use schema::{anthropic_tools, extract, ExtractedCall, ExtractedTurn};
pub use session::{ApprovalStep, AssistantSession, PendingView, ToolResult, ToolResultText, ToolStep};
pub use store::{ApprovalChannel, ApprovalResult, PendingAction, PendingActionStore, Proposal};
pub use tool::{ToolArguments, ToolCall, ToolField, ToolOrigin, ToolRisk, ToolSpec, TurnContext};
pub use exec::{run_tool, Env, Outcome};
pub use reminders::{Reminder, ReminderBook};
