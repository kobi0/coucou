//! Running an assistant tool. This is the only code that touches the outside world for the assistant, and it
//! is reached from two places only: a tool the policy says is safe to run (a draft), and an action the user
//! approved with a click on the card. Every call is parsed again here with the strict parsers, so nothing
//! malformed gets as far as the system. The messages it returns are fixed wording plus values the parsers
//! have checked. They never contain free text written by the model.
//!
//! The system calls are passed in, so this is testable and so the app decides how a link is opened.

use crate::catalog::{MAIL_DRAFT, REMINDER_CREATE};
use crate::parsers::{format_in_zone, parse_mail_draft, parse_reminder};
use crate::reminders::ReminderBook;
use crate::session::ToolResult;
use crate::tool::ToolCall;

/// What `run_tool` needs from the app.
pub struct Env<'a> {
    /// Seconds since 1970.
    pub now: f64,
    /// The user's offset from UTC in minutes, for saying a time back to them.
    pub offset_minutes: i32,
    /// Opens a link in the user's own mail app. Returns false when it could not.
    pub open_link: &'a dyn Fn(&str) -> bool,
    pub reminders: &'a mut ReminderBook,
}

/// What the app must do after a tool ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub result: ToolResult,
    /// True when a reminder was added: save the book and arm the timer again.
    pub reminders_changed: bool,
}

fn failed(message: &str) -> Outcome {
    Outcome { result: ToolResult { ok: false, message: message.to_string() }, reminders_changed: false }
}

pub fn run_tool(call: &ToolCall, env: &mut Env) -> Outcome {
    match call.tool_id.as_str() {
        MAIL_DRAFT => open_mail_draft(call, env),
        REMINDER_CREATE => set_reminder(call, env),
        _ => failed("That action is not available in this version."),
    }
}

fn open_mail_draft(call: &ToolCall, env: &Env) -> Outcome {
    let request = match parse_mail_draft(&call.arguments) {
        Ok(request) => request,
        Err(error) => return failed(error.message()),
    };
    let Some(link) = request.mailto_url() else {
        return failed("That message is too long to open as a draft. Ask for a shorter one.");
    };
    if (env.open_link)(&link) {
        Outcome {
            result: ToolResult { ok: true, message: format!("A draft to {} is open in your mail app. Nothing was sent.", request.to) },
            reminders_changed: false,
        }
    } else {
        failed("I could not open your mail app.")
    }
}

fn set_reminder(call: &ToolCall, env: &mut Env) -> Outcome {
    let request = match parse_reminder(&call.arguments, env.now) {
        Ok(request) => request,
        Err(error) => return failed(error.message()),
    };
    // A time a moment ago (a slow click) still fires, a couple of seconds from now.
    let due = request.due.max(env.now as i64 + 2);
    let text = match &request.notes {
        Some(notes) => format!("{} — {}", request.title, notes),
        None => request.title.clone(),
    };
    if env.reminders.add(due, &text).is_none() {
        return failed("There are too many reminders waiting. Wait for some to pass first.");
    }
    Outcome {
        result: ToolResult {
            ok: true,
            message: format!("It will show on the island on {}, while the app is running.", format_in_zone(due, env.offset_minutes)),
        },
        reminders_changed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::iso_utc;
    use crate::tool::{ToolArguments, ToolOrigin};
    use std::cell::RefCell;

    const NOW: f64 = 1_800_000_000.0;

    fn call(id: &str, args: &[(&str, &str)]) -> ToolCall {
        let arguments: ToolArguments = args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ToolCall { tool_id: id.to_string(), arguments, origin: ToolOrigin::User }
    }

    #[test]
    fn a_reminder_is_stored_and_said_back_in_the_users_zone() {
        let mut book = ReminderBook::new();
        let open = |_: &str| true;
        let mut env = Env { now: NOW, offset_minutes: 60, open_link: &open, reminders: &mut book };
        let due = iso_utc(NOW as i64 + 3600);
        let out = run_tool(&call(REMINDER_CREATE, &[("title", "Call Mum"), ("due", &due), ("notes", "about the trip")]), &mut env);
        assert!(out.result.ok && out.reminders_changed);
        assert_eq!(out.result.message, "It will show on the island on Fri 15 Jan 2027, 10:00 (UTC+01:00), while the app is running.");
        assert_eq!(book.items()[0].title, "Call Mum — about the trip");
        assert_eq!(book.items()[0].due, NOW as i64 + 3600);
    }

    #[test]
    fn a_time_a_moment_ago_still_fires_soon() {
        let mut book = ReminderBook::new();
        let open = |_: &str| true;
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let due = iso_utc(NOW as i64 - 30);
        assert!(run_tool(&call(REMINDER_CREATE, &[("title", "x"), ("due", &due)]), &mut env).result.ok);
        assert_eq!(book.items()[0].due, NOW as i64 + 2);
    }

    #[test]
    fn a_bad_reminder_is_refused_with_the_parsers_words() {
        let mut book = ReminderBook::new();
        let open = |_: &str| true;
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let out = run_tool(&call(REMINDER_CREATE, &[("title", "x"), ("due", "tomorrow")]), &mut env);
        assert!(!out.result.ok && !out.reminders_changed);
        assert!(book.is_empty());
    }

    #[test]
    fn a_draft_opens_the_users_mail_app_and_sends_nothing() {
        let mut book = ReminderBook::new();
        let opened = RefCell::new(Vec::<String>::new());
        let open = |link: &str| {
            opened.borrow_mut().push(link.to_string());
            true
        };
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let out = run_tool(&call(MAIL_DRAFT, &[("to", "ada@example.com"), ("subject", "Hi there"), ("body", "Line1\nLine2")]), &mut env);
        assert!(out.result.ok && !out.reminders_changed);
        assert_eq!(out.result.message, "A draft to ada@example.com is open in your mail app. Nothing was sent.");
        assert_eq!(opened.borrow().as_slice(), ["mailto:ada@example.com?subject=Hi%20there&body=Line1%0D%0ALine2"]);
    }

    #[test]
    fn a_draft_that_cannot_open_says_so() {
        let mut book = ReminderBook::new();
        let open = |_: &str| false;
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let out = run_tool(&call(MAIL_DRAFT, &[("to", "ada@example.com"), ("subject", "s"), ("body", "b")]), &mut env);
        assert_eq!(out.result.message, "I could not open your mail app.");
        assert!(!out.result.ok);
    }

    #[test]
    fn a_draft_with_two_recipients_never_opens() {
        let mut book = ReminderBook::new();
        let opened = RefCell::new(0);
        let open = |_: &str| {
            *opened.borrow_mut() += 1;
            true
        };
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let out = run_tool(&call(MAIL_DRAFT, &[("to", "a@b.com,c@d.com"), ("subject", "s"), ("body", "b")]), &mut env);
        assert!(!out.result.ok);
        assert_eq!(*opened.borrow(), 0);
    }

    #[test]
    fn a_draft_that_is_too_long_is_refused_not_cut() {
        let mut book = ReminderBook::new();
        let open = |_: &str| true;
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let body = "a".repeat(7_000);
        let out = run_tool(&call(MAIL_DRAFT, &[("to", "a@b.com"), ("subject", "s"), ("body", &body)]), &mut env);
        assert_eq!(out.result.message, "That message is too long to open as a draft. Ask for a shorter one.");
    }

    #[test]
    fn an_unknown_tool_does_nothing() {
        let mut book = ReminderBook::new();
        let open = |_: &str| true;
        let mut env = Env { now: NOW, offset_minutes: 0, open_link: &open, reminders: &mut book };
        let out = run_tool(&call("wire.money", &[]), &mut env);
        assert!(!out.result.ok);
    }
}
