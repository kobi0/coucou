//! Every tool the assistant may call, declared once. Tool ids are stable contract values (they appear in the
//! activity log and in saved settings): never rename one that has shipped. Add a tool by adding a spec here, a
//! parser if it takes structured input, and the app code that performs it.

use std::collections::BTreeMap;

use crate::tool::{ToolField, ToolRisk, ToolSpec};

pub const REMINDER_CREATE: &str = "reminder.create";
pub const MAIL_DRAFT: &str = "mail.draft";

/// Sets a reminder. Changes the outside world, so it always waits for a click.
pub fn reminder_create() -> ToolSpec {
    ToolSpec::new(REMINDER_CREATE, "Set a reminder", ToolRisk::Act)
        .required(&["title", "due"])
        .optional(&["notes"])
        .fields(vec![
            ToolField::new("Reminder", "title").content(),
            ToolField::new("When", "due").date_time().logged(),
            ToolField::new("Notes", "notes").content(),
        ])
        .summary("Set a reminder that shows in Coucou at a given time.")
        .help("title", "What to be reminded about, in the user's language.")
        .help("due", "When, as ISO 8601 date and time with a time zone offset, for example 2026-10-07T16:00:00+01:00.")
        .help("notes", "Optional extra detail.")
}

/// Opens a draft email for the user to read. It does not send. Sending is a separate, later step.
pub fn mail_draft() -> ToolSpec {
    ToolSpec::new(MAIL_DRAFT, "Draft an email", ToolRisk::Draft)
        .required(&["to", "subject", "body"])
        .fields(vec![
            ToolField::new("To", "to").logged(),
            ToolField::new("Subject", "subject").content(),
            ToolField::new("Message", "body").content(),
        ])
        .summary("Open a draft email in the user's mail app for them to read and send themselves.")
        .help("to", "One full email address, for example name@example.com. Never guess an address.")
        .help("subject", "A single line.")
        .help("body", "The whole message, written in the language the user wants.")
}

/// The whole catalog, keyed by tool id.
pub fn catalog() -> BTreeMap<String, ToolSpec> {
    let mut map = BTreeMap::new();
    for spec in [reminder_create(), mail_draft()] {
        map.insert(spec.id.clone(), spec);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ids_and_risks_are_stable() {
        assert_eq!(reminder_create().id, "reminder.create");
        assert_eq!(mail_draft().id, "mail.draft");
        assert_eq!(reminder_create().risk, ToolRisk::Act);
        assert_eq!(mail_draft().risk, ToolRisk::Draft);
    }

    #[test]
    fn the_catalog_is_consistent() {
        for (id, spec) in catalog() {
            assert_eq!(id, spec.id);
            let declared: BTreeSet<&str> = spec.required.iter().chain(spec.optional.iter()).map(String::as_str).collect();
            assert!(spec.fields.iter().all(|f| declared.contains(f.argument.as_str())), "{id}: every card field is a declared argument");
            assert!(spec.required.iter().all(|r| !spec.optional.contains(r)), "{id}: no argument is both required and optional");
            assert!(declared.iter().all(|a| spec.fields.iter().any(|f| f.argument == *a)), "{id}: every argument has a card field");
        }
    }
}
