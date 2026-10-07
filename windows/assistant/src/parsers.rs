//! Strict parsers for the arguments of each tool. The code that actually sets a reminder or opens a draft
//! only ever receives what these return, so nothing malformed, ambiguous or in the wrong language gets that far.

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::text::{is_visibly_empty, sanitize, strip_hidden, trim_inline_spaces};
use crate::tool::ToolArguments;

// ── Dates ─────────────────────────────────────────────────────────────────────

/// ISO 8601 date and time with a time zone designator ("Z" or "+01:00"), with or without fractional
/// seconds. Anything else is `None`: a date alone, a time with no zone, a zone written without its colon,
/// or words. A time with no zone is refused instead of guessed, because guessing silently moves a reminder.
pub fn parse_iso8601(text: &str) -> Option<OffsetDateTime> {
    let trimmed = text.trim();
    if !has_zone_designator(trimmed) {
        return None;
    }
    OffsetDateTime::parse(trimmed, &Rfc3339).ok()
}

fn has_zone_designator(text: &str) -> bool {
    if text.ends_with('Z') {
        return true;
    }
    let bytes = text.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    let tail = &bytes[bytes.len() - 6..];
    (tail[0] == b'+' || tail[0] == b'-')
        && tail[1].is_ascii_digit()
        && tail[2].is_ascii_digit()
        && tail[3] == b':'
        && tail[4].is_ascii_digit()
        && tail[5].is_ascii_digit()
}

/// "Wed 7 Oct 2026, 15:00 UTC": a moment written for a person, in UTC. The island shows the same moment in the
/// user's own time zone from the ISO text; this is what is shown when it cannot.
pub fn format_utc(moment: OffsetDateTime) -> String {
    format!("{} UTC", date_words(moment.to_offset(time::UtcOffset::UTC)))
}

/// The same, in a zone `offset_minutes` east of UTC: "Wed 7 Oct 2026, 16:00 (UTC+01:00)". A silly offset
/// falls back to UTC.
pub fn format_in_zone(unix_seconds: i64, offset_minutes: i32) -> String {
    let Ok(moment) = OffsetDateTime::from_unix_timestamp(unix_seconds) else { return String::new() };
    let Ok(offset) = time::UtcOffset::from_whole_seconds(offset_minutes.saturating_mul(60)) else {
        return format_utc(moment);
    };
    if offset_minutes == 0 {
        return format_utc(moment);
    }
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let magnitude = offset_minutes.unsigned_abs();
    format!("{} (UTC{sign}{:02}:{:02})", date_words(moment.to_offset(offset)), magnitude / 60, magnitude % 60)
}

fn date_words(utc: OffsetDateTime) -> String {
    let weekday = match utc.weekday() {
        time::Weekday::Monday => "Mon",
        time::Weekday::Tuesday => "Tue",
        time::Weekday::Wednesday => "Wed",
        time::Weekday::Thursday => "Thu",
        time::Weekday::Friday => "Fri",
        time::Weekday::Saturday => "Sat",
        time::Weekday::Sunday => "Sun",
    };
    let month = match utc.month() {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    };
    format!("{weekday} {} {month} {}, {:02}:{:02}", utc.day(), utc.year(), utc.hour(), utc.minute())
}

/// The same moment as an ISO 8601 text in UTC, "2026-10-07T15:00:00Z". Written by hand so it needs no
/// formatting feature.
pub fn iso_utc(unix_seconds: i64) -> String {
    match OffsetDateTime::from_unix_timestamp(unix_seconds) {
        Ok(t) => format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            t.year(),
            t.month() as u8,
            t.day(),
            t.hour(),
            t.minute(),
            t.second()
        ),
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

/// The same moment written in a zone that is `offset_minutes` east of UTC, "2026-10-07T16:00:00+01:00". The
/// model is told the time this way so the dates it gives back carry the user's own zone. An offset outside
/// +-18 hours, or a time that cannot be shown, falls back to UTC.
pub fn iso_with_offset(unix_seconds: i64, offset_minutes: i32) -> String {
    let Ok(offset) = time::UtcOffset::from_whole_seconds(offset_minutes.saturating_mul(60)) else {
        return iso_utc(unix_seconds);
    };
    match OffsetDateTime::from_unix_timestamp(unix_seconds) {
        Ok(t) => {
            let t = t.to_offset(offset);
            let sign = if offset_minutes < 0 { '-' } else { '+' };
            let magnitude = offset_minutes.unsigned_abs();
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
                t.year(),
                t.month() as u8,
                t.day(),
                t.hour(),
                t.minute(),
                t.second(),
                magnitude / 60,
                magnitude % 60
            )
        }
        Err(_) => iso_utc(unix_seconds),
    }
}

/// The user's offset from UTC in minutes, worked out from the wall clock the system shows and the real time.
/// The two describe the same instant, so their difference is the zone. Rounded to a whole minute. `None` when
/// the wall clock is not a real date.
pub fn offset_minutes_from_wall_clock(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8, unix_now: i64) -> Option<i32> {
    let month = time::Month::try_from(month).ok()?;
    let date = time::Date::from_calendar_date(year, month, day).ok()?;
    let wall = time::PrimitiveDateTime::new(date, time::Time::from_hms(hour, minute, second).ok()?).assume_utc().unix_timestamp();
    let diff = wall - unix_now;
    let minutes = (diff as f64 / 60.0).round() as i32;
    (minutes.abs() <= 18 * 60).then_some(minutes)
}

// ── Reminder ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReminderRequest {
    pub title: String,
    /// Seconds since 1970, UTC.
    pub due: i64,
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReminderParseError {
    MissingTitle,
    TitleTooLong,
    NotesTooLong,
    MissingDue,
    BadDate,
    DateInPast,
    DateTooFar,
}

impl ReminderParseError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingTitle => "The reminder needs a title.",
            Self::TitleTooLong => "The reminder title is too long.",
            Self::NotesTooLong => "The reminder notes are too long.",
            Self::MissingDue => "The reminder needs a time.",
            Self::BadDate => "I could not read that time. It needs a date, a time and a time zone.",
            Self::DateInPast => "That time has already passed.",
            Self::DateTooFar => "That time is too far away for a reminder.",
        }
    }
}

pub const MAX_TITLE_LENGTH: usize = 200;
pub const MAX_NOTES_LENGTH: usize = 2_000;
/// A reminder may be this many seconds in the past (a slow click, clock drift) and still count as now.
pub const PAST_GRACE: f64 = 60.0;
/// Five years, in seconds.
pub const MAX_FUTURE: f64 = 5.0 * 365.0 * 24.0 * 3600.0;

/// `now` is seconds since 1970, UTC.
pub fn parse_reminder(arguments: &ToolArguments, now: f64) -> Result<ReminderRequest, ReminderParseError> {
    let title = sanitize(arguments.get("title").map(String::as_str).unwrap_or(""), usize::MAX);
    if title.is_empty() {
        return Err(ReminderParseError::MissingTitle);
    }
    if title.chars().count() > MAX_TITLE_LENGTH {
        return Err(ReminderParseError::TitleTooLong);
    }

    let due_text = arguments.get("due").map(|d| d.trim()).unwrap_or("");
    if due_text.is_empty() {
        return Err(ReminderParseError::MissingDue);
    }
    let due = parse_iso8601(due_text).ok_or(ReminderParseError::BadDate)?.unix_timestamp();
    if (due as f64) < now - PAST_GRACE {
        return Err(ReminderParseError::DateInPast);
    }
    if (due as f64) > now + MAX_FUTURE {
        return Err(ReminderParseError::DateTooFar);
    }

    let mut notes = None;
    if let Some(raw) = arguments.get("notes") {
        let cleaned = sanitize(raw, usize::MAX);
        if cleaned.chars().count() > MAX_NOTES_LENGTH {
            return Err(ReminderParseError::NotesTooLong);
        }
        if !cleaned.is_empty() {
            notes = Some(cleaned);
        }
    }
    Ok(ReminderRequest { title, due, notes })
}

// ── Mail draft ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailDraftRequest {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailDraftParseError {
    BadRecipient,
    MissingSubject,
    BadSubject,
    SubjectTooLong,
    MissingBody,
}

impl MailDraftParseError {
    pub fn message(self) -> &'static str {
        match self {
            Self::BadRecipient => "I need one valid email address to write to.",
            Self::MissingSubject => "The email needs a subject.",
            Self::BadSubject => "The subject must be a single line.",
            Self::SubjectTooLong => "The subject is too long.",
            Self::MissingBody => "The email needs a message.",
        }
    }
}

pub const MAX_SUBJECT_LENGTH: usize = 200;
/// Longest link built. Some mail apps fail on longer ones, so a longer message is refused instead of cut.
pub const MAX_LINK_LENGTH: usize = 6_000;

pub fn parse_mail_draft(arguments: &ToolArguments) -> Result<MailDraftRequest, MailDraftParseError> {
    let to = arguments.get("to").map(|t| t.trim()).unwrap_or("");
    if !is_plausible_address(to) {
        return Err(MailDraftParseError::BadRecipient);
    }

    let subject = trim_inline_spaces(arguments.get("subject").map(String::as_str).unwrap_or(""));
    if subject.is_empty() {
        return Err(MailDraftParseError::MissingSubject);
    }
    // A line break in a subject is how extra headers get injected. Refuse it instead of cleaning it.
    if subject.chars().any(|c| c.is_control()) {
        return Err(MailDraftParseError::BadSubject);
    }
    if subject.chars().count() > MAX_SUBJECT_LENGTH {
        return Err(MailDraftParseError::SubjectTooLong);
    }

    let body = arguments.get("body").map(|b| b.trim()).unwrap_or("");
    if body.is_empty() {
        return Err(MailDraftParseError::MissingBody);
    }
    Ok(MailDraftRequest { to: to.to_string(), subject: subject.to_string(), body: body.to_string() })
}

/// One plain ASCII address: no display name, no list, no angle brackets, no spaces or control characters.
/// Non-ASCII addresses are refused for now because look-alike letters can disguise the real domain.
pub fn is_plausible_address(text: &str) -> bool {
    if text.is_empty() || text.chars().count() > 254 {
        return false;
    }
    if !text.chars().all(|c| (c as u32) > 32 && (c as u32) < 127) {
        return false;
    }
    if text.chars().any(|c| matches!(c, ',' | ';' | '<' | '>' | '"' | '(' | ')' | '[' | ']' | '\\' | ':')) {
        return false;
    }
    let parts: Vec<&str> = text.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    for label in &labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        if !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return false;
        }
    }
    match labels.last() {
        Some(tld) => tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic()),
        None => false,
    }
}

impl MailDraftRequest {
    /// The link that opens this draft in the user's mail app. Opening it sends nothing. Hidden characters are
    /// removed from the subject and message first, so what goes into the draft matches what the card could
    /// show. Every character other than letters, digits and "-._~" is percent-encoded, so nothing in the text
    /// can add a second recipient or a header. `None` when the address is not plain, there is nothing left to
    /// say, or the link would be too long.
    pub fn mailto_url(&self) -> Option<String> {
        let safe_address = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'@' | b'.' | b'_' | b'-' | b'+');
        if self.to.is_empty() || !self.to.bytes().all(safe_address) {
            return None;
        }

        let subject = strip_hidden(&self.subject, false);
        let subject = trim_inline_spaces(&subject);
        let body = strip_hidden(&self.body, true);
        let body = body.trim();
        if is_visibly_empty(subject) || is_visibly_empty(body) {
            return None;
        }
        let crlf = body.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n");

        let url = format!("mailto:{}?subject={}&body={}", self.to, percent_encode(subject), percent_encode(&crlf));
        if url.chars().count() > MAX_LINK_LENGTH {
            return None;
        }
        Some(url)
    }
}

/// Everything except letters, digits and "-._~" becomes %XX, byte by byte.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    fn args(pairs: &[(&str, &str)]) -> ToolArguments {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn hour_later() -> String {
        iso_utc(NOW as i64 + 3600)
    }

    // ── dates ──

    #[test]
    fn iso_dates() {
        let utc = parse_iso8601("2026-10-07T15:00:00Z").unwrap();
        let lagos = parse_iso8601("2026-10-07T16:00:00+01:00").unwrap();
        assert_eq!(utc.unix_timestamp(), lagos.unix_timestamp(), "a +01:00 offset means the same moment as Z");
        assert!(parse_iso8601("2026-10-07T15:00:00.500Z").is_some(), "fractional seconds");
        assert!(parse_iso8601("2026-10-07").is_none(), "a date with no time");
        assert!(parse_iso8601("2026-10-07T15:00:00").is_none(), "a time with no zone");
        assert!(parse_iso8601("2026-10-07T15:00:00+0100").is_none(), "a zone with no colon");
        assert!(parse_iso8601("tomorrow at 4").is_none(), "words");
        assert!(parse_iso8601("2026-13-45T15:00:00Z").is_none(), "an impossible date");
    }

    #[test]
    fn utc_formatting() {
        let t = parse_iso8601("2026-10-07T15:00:00Z").unwrap();
        assert_eq!(format_utc(t), "Wed 7 Oct 2026, 15:00 UTC");
        let lagos = parse_iso8601("2026-10-07T16:00:00+01:00").unwrap();
        assert_eq!(format_utc(lagos), "Wed 7 Oct 2026, 15:00 UTC");
        assert_eq!(iso_utc(parse_iso8601("2026-10-07T15:00:00Z").unwrap().unix_timestamp()), "2026-10-07T15:00:00Z");
    }

    #[test]
    fn dates_for_people_in_their_zone() {
        assert_eq!(format_in_zone(1_800_000_000, 60), "Fri 15 Jan 2027, 09:00 (UTC+01:00)");
        assert_eq!(format_in_zone(1_800_000_000, -330), "Fri 15 Jan 2027, 02:30 (UTC-05:30)");
        assert_eq!(format_in_zone(1_800_000_000, 0), "Fri 15 Jan 2027, 08:00 UTC");
        assert_eq!(format_in_zone(1_800_000_000, 99_999), "Fri 15 Jan 2027, 08:00 UTC");
    }

    #[test]
    fn times_in_the_users_zone() {
        assert_eq!(iso_with_offset(1_800_000_000, 60), "2027-01-15T09:00:00+01:00");
        assert_eq!(iso_with_offset(1_800_000_000, -330), "2027-01-15T02:30:00-05:30");
        assert_eq!(iso_with_offset(1_800_000_000, 0), "2027-01-15T08:00:00+00:00");
        assert_eq!(iso_with_offset(1_800_000_000, 99_999), "2027-01-15T08:00:00Z", "a silly zone falls back to UTC");
        assert!(parse_iso8601(&iso_with_offset(1_800_000_000, 60)).is_some(), "the strict parser accepts what we write");
        assert_eq!(parse_iso8601(&iso_with_offset(1_800_000_000, 60)).unwrap().unix_timestamp(), 1_800_000_000);
    }

    #[test]
    fn the_zone_is_found_from_the_wall_clock() {
        // 1_800_000_000 is 2027-01-15 08:00:00 UTC.
        assert_eq!(offset_minutes_from_wall_clock(2027, 1, 15, 9, 0, 0, 1_800_000_000), Some(60));
        assert_eq!(offset_minutes_from_wall_clock(2027, 1, 15, 8, 0, 1, 1_800_000_000), Some(0), "a second of drift rounds away");
        assert_eq!(offset_minutes_from_wall_clock(2027, 1, 15, 2, 30, 0, 1_800_000_000), Some(-330));
        assert_eq!(offset_minutes_from_wall_clock(2027, 2, 30, 9, 0, 0, 1_800_000_000), None);
        assert_eq!(offset_minutes_from_wall_clock(2027, 1, 16, 8, 0, 0, 1_800_000_000), None, "a day off is not a zone");
    }

    // ── reminder ──

    #[test]
    fn a_valid_reminder_parses() {
        let r = parse_reminder(&args(&[("title", "  Call Mum  "), ("due", &hour_later())]), NOW).unwrap();
        assert_eq!(r.title, "Call Mum");
        assert_eq!(r.due, NOW as i64 + 3600);
        assert_eq!(r.notes, None);
        let r = parse_reminder(&args(&[("title", "x"), ("due", &hour_later()), ("notes", "bring the file")]), NOW).unwrap();
        assert_eq!(r.notes.as_deref(), Some("bring the file"));
    }

    #[test]
    fn reminder_errors() {
        let later = hour_later();
        let e = |a: &[(&str, &str)]| parse_reminder(&args(a), NOW).unwrap_err();
        assert_eq!(e(&[("title", "x"), ("due", "tomorrow at 4")]), ReminderParseError::BadDate);
        assert_eq!(e(&[("title", "x"), ("due", &iso_utc(NOW as i64 - 3600))]), ReminderParseError::DateInPast);
        assert!(parse_reminder(&args(&[("title", "x"), ("due", &iso_utc(NOW as i64 - 30))]), NOW).is_ok(), "30 seconds ago is still now");
        assert_eq!(
            e(&[("title", "x"), ("due", &iso_utc((NOW + MAX_FUTURE) as i64 + 86_400))]),
            ReminderParseError::DateTooFar
        );
        assert_eq!(e(&[("title", "   "), ("due", &later)]), ReminderParseError::MissingTitle);
        assert_eq!(e(&[("title", "x")]), ReminderParseError::MissingDue);
        assert_eq!(e(&[("title", &"t".repeat(MAX_TITLE_LENGTH + 1)), ("due", &later)]), ReminderParseError::TitleTooLong);
        assert_eq!(
            e(&[("title", "x"), ("due", &later), ("notes", &"n".repeat(MAX_NOTES_LENGTH + 1))]),
            ReminderParseError::NotesTooLong
        );
        let r = parse_reminder(&args(&[("title", "a\nb"), ("due", &later)]), NOW).unwrap();
        assert_eq!(r.title, "a ⏎ b", "a line break in a title becomes a marker");
        for err in [
            ReminderParseError::MissingTitle,
            ReminderParseError::TitleTooLong,
            ReminderParseError::NotesTooLong,
            ReminderParseError::MissingDue,
            ReminderParseError::BadDate,
            ReminderParseError::DateInPast,
            ReminderParseError::DateTooFar,
        ] {
            assert!(!err.message().is_empty());
        }
    }

    // ── mail ──

    #[test]
    fn addresses() {
        for good in ["ada@example.com", "ada.o+news@mail.example.co.uk", "A_B-c@sub.example.ng"] {
            assert!(is_plausible_address(good), "accepts {good}");
        }
        let bad = [
            "", "ada", "ada@example", "ada@@example.com", "ada example@example.com",
            "ada@example.com, bob@example.com", "ada@example.com;bob@example.com", "<ada@example.com>",
            "Ada <ada@example.com>", "ada@exa mple.com", "ada@-example.com", "ada@example-.com",
            "ada@example.c", "ada@example..com", ".ada@example.com", "ada.@example.com",
            "ad\u{00E0}@example.com", "ada@example.com\nBcc: x@y.com", "ada@exam\u{0430}ple.com",
        ];
        for text in bad {
            assert!(!is_plausible_address(text), "refuses {text:?}");
        }
    }

    #[test]
    fn a_valid_draft_parses() {
        let m = parse_mail_draft(&args(&[("to", " ada@example.com "), ("subject", "Hello"), ("body", "Hi Ada,\nSee you soon.")])).unwrap();
        assert_eq!(m.to, "ada@example.com");
        assert_eq!(m.subject, "Hello");
        assert!(m.body.contains('\n'));
    }

    #[test]
    fn mail_errors() {
        let e = |a: &[(&str, &str)]| parse_mail_draft(&args(a)).unwrap_err();
        assert_eq!(e(&[("to", "nope"), ("subject", "Hello"), ("body", "Hi")]), MailDraftParseError::BadRecipient);
        assert_eq!(e(&[("to", "ada@example.com"), ("subject", "Hi\nBcc: x@y.com"), ("body", "Hi")]), MailDraftParseError::BadSubject);
        assert_eq!(e(&[("to", "ada@example.com"), ("subject", "  "), ("body", "Hi")]), MailDraftParseError::MissingSubject);
        assert_eq!(
            e(&[("to", "ada@example.com"), ("subject", &"s".repeat(MAX_SUBJECT_LENGTH + 1)), ("body", "Hi")]),
            MailDraftParseError::SubjectTooLong
        );
        assert_eq!(e(&[("to", "ada@example.com"), ("subject", "Hi"), ("body", " \n ")]), MailDraftParseError::MissingBody);
        for err in [
            MailDraftParseError::BadRecipient,
            MailDraftParseError::MissingSubject,
            MailDraftParseError::BadSubject,
            MailDraftParseError::SubjectTooLong,
            MailDraftParseError::MissingBody,
        ] {
            assert!(!err.message().is_empty());
        }
    }

    fn draft(to: &str, subject: &str, body: &str) -> MailDraftRequest {
        MailDraftRequest { to: to.into(), subject: subject.into(), body: body.into() }
    }

    #[test]
    fn mailto_links() {
        assert_eq!(
            draft("ada@example.com", "Hi there", "Line1\nLine2").mailto_url().unwrap(),
            "mailto:ada@example.com?subject=Hi%20there&body=Line1%0D%0ALine2"
        );
        assert_eq!(
            draft("ada@example.com", "Hi", "Café").mailto_url().unwrap(),
            "mailto:ada@example.com?subject=Hi&body=Caf%C3%A9"
        );
        let bidi = draft("ada@example.com", "Hi\u{202E}there", "ok").mailto_url().unwrap();
        assert!(!bidi.contains("%E2%80%AE") && bidi.contains("subject=Hithere"), "hidden characters are removed");
        assert!(draft("a@b.com,c@d.com", "s", "b").mailto_url().is_none(), "a second recipient");
        assert!(draft("a@b.com?bcc=x@y.com", "s", "b").mailto_url().is_none(), "an added header");
        let injected = draft("a@b.com", "s&bcc=x@y.com", "b&cc=z@y.com").mailto_url().unwrap();
        assert!(injected.contains("subject=s%26bcc%3Dx%40y.com") && injected.contains("body=b%26cc%3Dz%40y.com"));
        assert!(draft("a@b.com", "s", &"a".repeat(7_000)).mailto_url().is_none(), "too long is refused, not cut");
        assert!(draft("a@b.com", "\u{200B}", "b").mailto_url().is_none(), "nothing left to say");
    }
}
