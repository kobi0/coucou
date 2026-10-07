//! A local, human-readable record of what the assistant did and what the user allowed. It never leaves the
//! device. Only fields a tool marks `is_logged` are written, so reminder titles, email subjects and message
//! bodies stay out of it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::parsers::iso_utc;
use crate::text::sanitize;
use crate::tool::{ToolCall, ToolSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityOutcome {
    Ran,
    Proposed,
    Approved,
    Denied,
    Expired,
    Refused,
    Failed,
}

impl ActivityOutcome {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ran => "ran",
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }
}

/// Fields are in alphabetical order on purpose: they are written in this order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActivityEntry {
    /// ISO 8601, UTC.
    pub at: String,
    pub details: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub origin: String,
    pub outcome: ActivityOutcome,
    pub risk: String,
    #[serde(rename = "toolId")]
    pub tool_id: String,
}

/// `at` is seconds since 1970.
pub fn entry(call: &ToolCall, spec: Option<&ToolSpec>, outcome: ActivityOutcome, at: f64, note: Option<&str>) -> ActivityEntry {
    let mut details = BTreeMap::new();
    if let Some(spec) = spec {
        for field in spec.fields.iter().filter(|f| f.is_logged) {
            if let Some(raw) = call.arguments.get(&field.argument) {
                let value = sanitize(raw, 200);
                if !value.is_empty() {
                    details.insert(field.argument.clone(), value);
                }
            }
        }
    }
    ActivityEntry {
        at: iso_utc(at as i64),
        details,
        note: note.map(|n| sanitize(n, 200)),
        origin: call.origin.name().to_string(),
        outcome,
        risk: spec.map(|s| s.risk.name()).unwrap_or("unknown").to_string(),
        tool_id: sanitize(&call.tool_id, 60),
    }
}

/// One JSON object on one line, for the log file.
pub fn json_line(entry: &ActivityEntry) -> Option<String> {
    serde_json::to_string(entry).ok()
}

/// A short line for showing in the app.
pub fn human_line(entry: &ActivityEntry) -> String {
    let mut parts = vec![entry.at.clone(), entry.tool_id.clone(), entry.outcome.name().to_string()];
    for (key, value) in &entry.details {
        parts.push(format!("{key}={value}"));
    }
    if let Some(note) = entry.note.as_deref().filter(|n| !n.is_empty()) {
        parts.push(note.to_string());
    }
    parts.join(" · ")
}

/// Default size at which the log is moved aside.
pub const DEFAULT_MAX_BYTES: u64 = 1_000_000;

/// Appends one line. On Unix the file is created readable by its owner only (on Windows it lives in the user's
/// own local app data). When it grows past `max_bytes` it is moved to "<name>.1", replacing the previous one,
/// and a new file starts. Returns false on any failure: the log is best effort and must never stop the assistant.
pub fn append_line(path: &Path, line: &str, max_bytes: u64) -> bool {
    let attempt = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > max_bytes {
                let old = rotated_name(path);
                let _ = std::fs::remove_file(&old);
                std::fs::rename(path, &old)?;
            }
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")
    };
    attempt().is_ok()
}

fn rotated_name(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".1");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{reminder_create, REMINDER_CREATE};
    use crate::tool::{ToolArguments, ToolOrigin};

    fn call(id: &str, args: &[(&str, &str)]) -> ToolCall {
        let arguments: ToolArguments = args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ToolCall { tool_id: id.to_string(), arguments, origin: ToolOrigin::User }
    }

    fn reminder_entry() -> ActivityEntry {
        let c = call(
            REMINDER_CREATE,
            &[("title", "Call Mum about the money"), ("due", "2027-01-15T09:20:00Z"), ("notes", "private")],
        );
        entry(&c, Some(&reminder_create()), ActivityOutcome::Approved, 1_800_000_000.0, None)
    }

    #[test]
    fn only_logged_fields_are_kept() {
        let e = reminder_entry();
        assert!(e.details.contains_key("due"));
        assert!(!e.details.contains_key("title"));
        assert!(!e.details.contains_key("notes"));
        assert_eq!(e.risk, "act");
        assert_eq!(e.origin, "user");
        assert_eq!(e.at, "2027-01-15T08:00:00Z");
    }

    #[test]
    fn an_unknown_tool_logs_its_risk_as_unknown() {
        let e = entry(&call("nope", &[]), None, ActivityOutcome::Refused, 1_800_000_000.0, Some("Unknown tool"));
        assert_eq!(e.risk, "unknown");
        assert_eq!(e.note.as_deref(), Some("Unknown tool"));
    }

    #[test]
    fn the_json_line_is_one_private_line() {
        let line = json_line(&reminder_entry()).unwrap();
        assert!(!line.contains('\n'));
        assert!(line.contains("reminder.create"));
        assert!(!line.contains("Call Mum"), "the title stays out");
        assert!(line.contains("\"toolId\""));
        let back: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(back["outcome"], "approved");
        assert_eq!(back["details"]["due"], "2027-01-15T09:20:00Z");
        assert!(back.get("note").is_none(), "no note means no field");
        let keys: Vec<&str> = line.split('"').filter(|s| matches!(*s, "at" | "details" | "origin" | "outcome" | "risk" | "toolId")).collect();
        assert_eq!(keys, vec!["at", "details", "origin", "outcome", "risk", "toolId"], "keys are written in alphabetical order");
    }

    #[test]
    fn the_human_line_names_the_tool_and_outcome() {
        let human = human_line(&reminder_entry());
        assert!(human.contains("reminder.create") && human.contains("approved") && human.contains("due="));
    }

    #[test]
    fn the_file_rotates() {
        let dir = std::env::temp_dir().join(format!("coucou-activity-{}-{}", std::process::id(), line!()));
        let path = dir.join("activity.log");
        assert!(append_line(&path, &"a".repeat(40), 50));
        assert!(append_line(&path, &"b".repeat(40), 50));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the log is private to its owner");
        }
        assert!(append_line(&path, &"c".repeat(40), 50));
        assert!(rotated_name(&path).exists(), "a big log is moved aside");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{}\n", "c".repeat(40)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
