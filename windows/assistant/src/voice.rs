//! Voice: the two assistant languages, the words the app says by itself, and the hold-to-talk state machine.
//! The same design as NotchBuddy/Sources/CoucouKit/Assistant/Voice.swift and VoiceSession.swift.
//!
//! Everything the app itself says aloud or shows as a spoken hint comes from the fixed phrases below, never
//! from the model. The persona text only changes how the model writes its replies; tool arguments stay in
//! standard form (full email addresses, ISO 8601 times) whatever language is spoken.
//!
//! Listening happens only while the person holds the mic button. Nothing listens in the background and there
//! is no wake word. What was heard is text like any other typed text: it is shown in the chat, and it can
//! never approve an action. Only a click on a confirmation card approves.
//!
//! The Pidgin wording is a first draft. A native speaker should review it before release.

use crate::text::{collapse_whitespace, strip_hidden};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VoiceLanguage {
    #[default]
    English,
    Pidgin,
}

/// Lines the app says or shows by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePhrase {
    Listening,
    DidNotCatch,
    CheckTheCard,
    MicBlocked,
    NeedsRecognizer,
    RestOnScreen,
}

impl VoicePhrase {
    pub const ALL: [VoicePhrase; 6] = [
        Self::Listening,
        Self::DidNotCatch,
        Self::CheckTheCard,
        Self::MicBlocked,
        Self::NeedsRecognizer,
        Self::RestOnScreen,
    ];
}

impl VoiceLanguage {
    /// The code stored in settings.
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Pidgin => "pcm",
        }
    }

    /// Anything unknown is English, so a damaged settings file never silences the assistant.
    pub fn from_code(code: &str) -> Self {
        if code == "pcm" {
            Self::Pidgin
        } else {
            Self::English
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Pidgin => "Nigerian Pidgin",
        }
    }

    /// Locales tried in order when hearing the user. No Pidgin recogniser exists, so English is used for both.
    pub fn recognizer_locales(self) -> &'static [&'static str] {
        &["en-NG", "en-GB", "en-US"]
    }

    /// Locales tried in order when choosing a voice to read replies.
    pub fn voice_locales(self) -> &'static [&'static str] {
        &["en-NG", "en-GB", "en-US"]
    }

    pub fn phrase(self, phrase: VoicePhrase) -> &'static str {
        use VoicePhrase::*;
        match self {
            Self::English => match phrase {
                Listening => "Listening…",
                DidNotCatch => "Sorry, I did not catch that. Please try again.",
                CheckTheCard => "I have prepared it. Please check the card and press Allow if it is right.",
                MicBlocked => "I cannot hear you yet. Please allow the microphone in your system settings.",
                NeedsRecognizer => "Voice input is not set up on this computer yet. You can type for now.",
                RestOnScreen => "The rest is on the screen.",
            },
            Self::Pidgin => match phrase {
                Listening => "I dey listen…",
                DidNotCatch => "Sorry, I no hear you well. Abeg try again.",
                CheckTheCard => "I don prepare am. Abeg check di card, then press Allow if e correct.",
                MicBlocked => "I no fit hear you yet. Abeg allow di microphone for system settings.",
                NeedsRecognizer => "Voice input never set for dis computer yet. You fit type for now.",
                RestOnScreen => "Di rest dey for screen.",
            },
        }
    }

    /// Added to the system prompt. English adds nothing, so the chat reads as it always did.
    pub fn style_prompt(self) -> &'static str {
        match self {
            Self::English => "",
            Self::Pidgin => " Write every reply in Nigerian Pidgin English, the way a friendly person in Lagos would talk, in plain easy words (na, dey, wetin, abeg, make, don, go, wan, sabi, no wahala, how far). Keep it clear first: do not overdo the slang and do not mock the language. Never change names, numbers, dates, times, email addresses, links or code: write those exactly as they are. When you call a tool, its arguments stay in standard form (full email addresses, ISO 8601 times) whatever language you are speaking. Answer in Pidgin even when the user writes in another language, unless they ask you to change.",
        }
    }
}

/// Added to the system prompt when replies are read aloud.
pub const SPOKEN_STYLE: &str = " Your reply will be read aloud, so keep it to a few short sentences in plain words, with no tables, code or long lists. Anything long can stay on the screen.";

pub const MAX_TRANSCRIPT: usize = 2_000;

/// Cleans what the recogniser heard before it becomes a message: removes hidden characters, collapses spaces
/// and line breaks, and cuts to `limit` characters. None when nothing readable is left.
pub fn clean_transcript(text: &str, limit: usize) -> Option<String> {
    let stripped = strip_hidden(text, false);
    let collapsed = collapse_whitespace(&stripped);
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= limit {
        return Some(collapsed);
    }
    let cut: String = collapsed.chars().take(limit).collect();
    let trimmed = cut.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceState {
    Idle,
    /// The button is held and the recogniser is hearing.
    Listening,
    /// The button was let go. Waiting for the final words.
    Finishing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceFailure {
    MicBlocked,
    /// No speech recogniser set up on this computer.
    Unavailable,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceCommand {
    StartListening,
    /// Stop recording and wait for the final words.
    StopListening,
    /// Stop recording and drop whatever was heard.
    Discard,
    /// Deliver these words as the person's message.
    Send(String),
    /// Tell the person something, in the assistant's language.
    Hint(VoicePhrase),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceSession {
    state: VoiceState,
    partial: String,
    started_at: Option<f64>,
}

impl Default for VoiceSession {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceSession {
    /// A press shorter than this many seconds is an accidental tap and is dropped.
    pub const MINIMUM_HOLD: f64 = 0.4;
    /// Listening stops by itself after this many seconds.
    pub const MAXIMUM_LISTEN: f64 = 60.0;

    pub fn new() -> Self {
        Self { state: VoiceState::Idle, partial: String::new(), started_at: None }
    }

    pub fn state(&self) -> VoiceState {
        self.state
    }

    /// The words heard so far while listening, cleaned. Shown where the person types.
    pub fn partial(&self) -> &str {
        &self.partial
    }

    fn reset(&mut self) {
        self.state = VoiceState::Idle;
        self.started_at = None;
        self.partial.clear();
    }

    /// `now` is seconds on any steady clock.
    pub fn press(&mut self, now: f64) -> Vec<VoiceCommand> {
        if self.state != VoiceState::Idle {
            return vec![];
        }
        self.state = VoiceState::Listening;
        self.started_at = Some(now);
        self.partial.clear();
        vec![VoiceCommand::StartListening]
    }

    pub fn release(&mut self, now: f64) -> Vec<VoiceCommand> {
        let (VoiceState::Listening, Some(start)) = (self.state, self.started_at) else {
            return vec![];
        };
        self.started_at = None;
        if now - start < Self::MINIMUM_HOLD {
            self.reset();
            return vec![VoiceCommand::Discard];
        }
        self.state = VoiceState::Finishing;
        vec![VoiceCommand::StopListening]
    }

    /// Call about once a second while listening.
    pub fn tick(&mut self, now: f64) -> Vec<VoiceCommand> {
        match (self.state, self.started_at) {
            (VoiceState::Listening, Some(start)) if now - start >= Self::MAXIMUM_LISTEN => self.release(now),
            _ => vec![],
        }
    }

    /// The recogniser has words so far.
    pub fn heard(&mut self, partial: &str) {
        if self.state == VoiceState::Idle {
            return;
        }
        self.partial = clean_transcript(partial, MAX_TRANSCRIPT).unwrap_or_default();
    }

    /// The recogniser is done. `transcript` is None or empty when nothing was heard.
    pub fn finished(&mut self, transcript: Option<&str>) -> Vec<VoiceCommand> {
        if self.state == VoiceState::Idle {
            return vec![];
        }
        let mut commands = vec![];
        if self.state == VoiceState::Listening {
            commands.push(VoiceCommand::StopListening);
        }
        self.reset();
        match clean_transcript(transcript.unwrap_or(""), MAX_TRANSCRIPT) {
            Some(text) => commands.push(VoiceCommand::Send(text)),
            None => commands.push(VoiceCommand::Hint(VoicePhrase::DidNotCatch)),
        }
        commands
    }

    /// Something went wrong while listening or finishing.
    pub fn failed(&mut self, failure: VoiceFailure) -> Vec<VoiceCommand> {
        if self.state == VoiceState::Idle {
            return vec![];
        }
        self.reset();
        let hint = match failure {
            VoiceFailure::MicBlocked => VoicePhrase::MicBlocked,
            VoiceFailure::Unavailable => VoicePhrase::NeedsRecognizer,
            VoiceFailure::Other => VoicePhrase::DidNotCatch,
        };
        vec![VoiceCommand::Discard, VoiceCommand::Hint(hint)]
    }

    pub fn cancel(&mut self) -> Vec<VoiceCommand> {
        if self.state == VoiceState::Idle {
            return vec![];
        }
        self.reset();
        vec![VoiceCommand::Discard]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use VoiceCommand::*;

    #[test]
    fn every_phrase_exists_in_both_languages() {
        for p in VoicePhrase::ALL {
            assert!(!VoiceLanguage::English.phrase(p).is_empty());
            assert!(!VoiceLanguage::Pidgin.phrase(p).is_empty());
        }
    }

    #[test]
    fn unknown_code_is_english() {
        assert_eq!(VoiceLanguage::from_code("pcm"), VoiceLanguage::Pidgin);
        assert_eq!(VoiceLanguage::from_code("en"), VoiceLanguage::English);
        assert_eq!(VoiceLanguage::from_code("zz"), VoiceLanguage::English);
        assert_eq!(VoiceLanguage::from_code(""), VoiceLanguage::English);
    }

    #[test]
    fn english_persona_adds_nothing_and_pidgin_keeps_tool_arguments_standard() {
        assert_eq!(VoiceLanguage::English.style_prompt(), "");
        let p = VoiceLanguage::Pidgin.style_prompt();
        assert!(p.contains("Pidgin") && p.contains("ISO 8601") && p.contains("email addresses"));
    }

    #[test]
    fn transcript_is_cleaned_and_cut() {
        assert_eq!(clean_transcript("  hello \n\n  world\u{200b} ", 100).as_deref(), Some("hello world"));
        assert_eq!(clean_transcript("\u{200b}  \n", 100), None);
        assert_eq!(clean_transcript("abcdef", 3).as_deref(), Some("abc"));
        assert_eq!(clean_transcript("ab   def", 3).as_deref(), Some("ab"));
    }

    #[test]
    fn a_short_tap_is_dropped() {
        let mut s = VoiceSession::new();
        assert_eq!(s.press(10.0), vec![StartListening]);
        assert_eq!(s.release(10.2), vec![Discard]);
        assert_eq!(s.state(), VoiceState::Idle);
    }

    #[test]
    fn a_hold_sends_what_was_heard() {
        let mut s = VoiceSession::new();
        s.press(0.0);
        s.heard("write to  tunde");
        assert_eq!(s.partial(), "write to tunde");
        assert_eq!(s.release(2.0), vec![StopListening]);
        assert_eq!(s.state(), VoiceState::Finishing);
        assert_eq!(s.finished(Some("write to tunde")), vec![Send("write to tunde".into())]);
        assert_eq!(s.state(), VoiceState::Idle);
        assert_eq!(s.partial(), "");
    }

    #[test]
    fn silence_gives_a_hint_not_a_message() {
        let mut s = VoiceSession::new();
        s.press(0.0);
        s.release(1.0);
        assert_eq!(s.finished(None), vec![Hint(VoicePhrase::DidNotCatch)]);
        s.press(5.0);
        assert_eq!(s.finished(Some("  ")), vec![StopListening, Hint(VoicePhrase::DidNotCatch)]);
    }

    #[test]
    fn it_stops_by_itself_after_a_minute() {
        let mut s = VoiceSession::new();
        s.press(0.0);
        assert!(s.tick(59.0).is_empty());
        assert_eq!(s.tick(60.0), vec![StopListening]);
    }

    #[test]
    fn failures_map_to_hints_and_idle_ignores_everything() {
        let mut s = VoiceSession::new();
        assert!(s.release(1.0).is_empty() && s.finished(Some("x")).is_empty() && s.failed(VoiceFailure::Other).is_empty());
        assert!(s.cancel().is_empty());
        s.press(0.0);
        assert_eq!(s.failed(VoiceFailure::MicBlocked), vec![Discard, Hint(VoicePhrase::MicBlocked)]);
        s.press(0.0);
        assert_eq!(s.failed(VoiceFailure::Unavailable), vec![Discard, Hint(VoicePhrase::NeedsRecognizer)]);
        s.press(0.0);
        assert_eq!(s.cancel(), vec![Discard]);
        assert_eq!(s.press(1.0), vec![StartListening]);
        assert!(s.press(1.1).is_empty(), "a second press while listening does nothing");
    }
}
