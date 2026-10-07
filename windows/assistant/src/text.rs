//! Display safety: what may appear on a card or in a log, and what may not.
//!
//! Text that a person or a model wrote can hide things: right-to-left overrides that flip an email address,
//! zero-width spaces, tag characters, a hundred stacked accents that fill the screen. Everything that is shown
//! or logged goes through here first. Lengths count Unicode scalars (Rust `char`), so combining marks cannot
//! be used to get past a limit.

use unicode_properties::{GeneralCategory as Gc, UnicodeGeneralCategory};

/// Most combining marks kept in a row. Stops stacked "zalgo" text from filling the screen.
pub const MAX_COMBINING_MARKS: usize = 4;

/// Removes characters that change how text looks, or hide text, without showing anything: control
/// characters, format characters (bidirectional overrides, zero-width spaces, soft hyphens, tag characters),
/// private-use characters, and combining marks beyond [`MAX_COMBINING_MARKS`] in a row. The zero-width joiner
/// and non-joiner stay, because emoji sequences and some scripts need them. Line breaks and tabs are kept
/// when `keep_line_breaks` is true. Otherwise they become spaces.
pub fn strip_hidden(text: &str, keep_line_breaks: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut mark_run = 0usize;
    for c in text.chars() {
        let category = c.general_category();
        match category {
            Gc::NonspacingMark | Gc::EnclosingMark | Gc::SpacingMark => {
                mark_run += 1;
                if mark_run <= MAX_COMBINING_MARKS {
                    out.push(c);
                }
                continue;
            }
            _ => mark_run = 0,
        }
        match category {
            Gc::Control => {
                if c == '\n' || c == '\t' {
                    out.push(if keep_line_breaks { c } else { ' ' });
                }
            }
            Gc::Format => {
                if c == '\u{200C}' || c == '\u{200D}' {
                    out.push(c);
                }
            }
            Gc::LineSeparator | Gc::ParagraphSeparator => out.push(if keep_line_breaks { '\n' } else { ' ' }),
            Gc::PrivateUse | Gc::Surrogate => {}
            _ => out.push(c),
        }
    }
    out
}

/// True when nothing a person could see is left: only spaces, line breaks and joiners.
pub fn is_visibly_empty(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace() || c == '\u{200C}' || c == '\u{200D}')
}

/// Trims spaces and tabs from both ends, but not line breaks.
pub fn trim_inline_spaces(text: &str) -> &str {
    text.trim_matches(|c: char| c == '\t' || c.general_category() == Gc::SpaceSeparator)
}

/// Collapses every run of whitespace, line breaks included, into one space.
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cuts text to `limit` scalars and adds an ellipsis. The second value is how many scalars were cut.
pub fn truncate(text: &str, limit: usize) -> (String, usize) {
    let count = text.chars().count();
    if count <= limit {
        return (text.to_string(), 0);
    }
    let mut kept: String = text.chars().take(limit).collect();
    kept.push('…');
    (kept, count - limit)
}

/// Makes text safe to show on a card or write to a log: hidden characters are removed, line breaks become a
/// visible marker, tabs become spaces, and long text is cut with an ellipsis.
pub fn sanitize(text: &str, limit: usize) -> String {
    let cleaned = strip_hidden(text, true);
    let mut out = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        match c {
            '\n' => out.push_str(" ⏎ "),
            '\t' => out.push(' '),
            other => out.push(other),
        }
    }
    truncate(trim_inline_spaces(&out), limit).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_left_alone() {
        assert_eq!(sanitize("Call Mum at 4", 50), "Call Mum at 4");
        assert_eq!(sanitize("a\tb", 50), "a b");
    }

    #[test]
    fn hidden_characters_are_removed() {
        assert_eq!(sanitize("a\u{E0041}b", 50), "ab", "tag characters");
        assert_eq!(sanitize("a\u{00AD}b", 50), "ab", "soft hyphen");
        assert_eq!(sanitize("a\u{E000}b", 50), "ab", "private use");
        assert_eq!(sanitize("a\u{202E}b", 50), "ab", "right-to-left override");
        assert_eq!(sanitize("a\u{200B}b", 50), "ab", "zero-width space");
        assert_eq!(sanitize("a\u{0007}b", 50), "ab", "bell");
    }

    #[test]
    fn line_breaks_become_a_visible_marker() {
        let shown = sanitize("ada@example.com\u{202E}moc.live@evil\nBcc: x@y.com\u{200B}", 500);
        assert!(shown.contains('⏎') && !shown.contains('\n'));
        assert!(!shown.contains('\u{202E}') && !shown.contains('\u{200B}'));
        assert_eq!(sanitize("a\nb", 50), "a ⏎ b");
        assert_eq!(sanitize("a\r\nb", 50), "a ⏎ b", "a carriage return is dropped");
    }

    #[test]
    fn emoji_joiners_are_kept() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(sanitize(family, 50), family);
    }

    #[test]
    fn stacked_combining_marks_are_capped() {
        let zalgo = format!("a{}", "\u{0301}".repeat(50));
        assert_eq!(sanitize(&zalgo, 100).chars().count(), 1 + MAX_COMBINING_MARKS);
        // a run of marks restarts after a letter
        let two = format!("a{}b{}", "\u{0301}".repeat(10), "\u{0301}".repeat(10));
        assert_eq!(sanitize(&two, 100).chars().count(), 2 + 2 * MAX_COMBINING_MARKS);
    }

    #[test]
    fn long_text_is_cut_and_counted() {
        let long = "x".repeat(900);
        assert_eq!(sanitize(&long, 500).chars().count(), 501);
        let (shown, hidden) = truncate(&long, 500);
        assert_eq!(shown.chars().count(), 501);
        assert_eq!(hidden, 400);
        assert_eq!(truncate("short", 500), ("short".to_string(), 0));
    }

    #[test]
    fn the_limit_counts_scalars_not_marks() {
        // 10 letters each carrying 4 accents = 50 scalars. A limit of 20 cuts it.
        let text = "a\u{0301}\u{0301}\u{0301}\u{0301}".repeat(10);
        let (shown, hidden) = truncate(&text, 20);
        assert_eq!(shown.chars().count(), 21);
        assert_eq!(hidden, 30);
    }

    #[test]
    fn visibly_empty() {
        assert!(is_visibly_empty("   \n "));
        assert!(is_visibly_empty("\u{200C}\u{200D}"));
        assert!(!is_visibly_empty(" a "));
        // zero-width space is a format character: stripped first, then empty
        assert!(is_visibly_empty(&strip_hidden("\u{200B}\u{200B}", false)));
    }

    #[test]
    fn strip_hidden_line_break_modes() {
        assert_eq!(strip_hidden("a\nb\tc", true), "a\nb\tc");
        assert_eq!(strip_hidden("a\nb\tc", false), "a b c");
        assert_eq!(strip_hidden("a\u{2028}b", true), "a\nb");
        assert_eq!(strip_hidden("a\u{2029}b", false), "a b");
    }

    #[test]
    fn whitespace_helpers() {
        assert_eq!(collapse_whitespace("  a \n\t b  "), "a b");
        assert_eq!(trim_inline_spaces("\t a \u{00A0}"), "a");
        assert_eq!(trim_inline_spaces("\na\n"), "\na\n", "line breaks are not trimmed");
    }
}
