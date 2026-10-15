//! The wire format for the Gemini Live voice: what to send to open a spoken conversation and how to read what
//! comes back. Pure functions, no network. The connection itself is in the app (src-tauri/src/live.rs).
//!
//! It follows Google's Live API reference (BidiGenerateContent, v1beta). Hold-to-talk uses manual activity
//! signals: automatic voice detection is switched off, so the model hears nothing until `activityStart` and
//! stops at `activityEnd`. That keeps the rule that nothing listens unless the mic button is held.
//!
//! The voice conversation has no tools. Whatever the model says is words and sound, never an action.
//!
//! Not yet checked against the real service: this was written from the published reference.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::text::sanitize;
use crate::voice::VoiceLanguage;

pub const DEFAULT_MODEL: &str = "gemini-3.8-live";
/// The voice from the AI Studio sample this was taken from.
pub const DEFAULT_VOICE: &str = "Puck";
/// What the app sends: raw 16-bit little-endian PCM, 16 kHz, mono.
pub const INPUT_MIME: &str = "audio/pcm;rate=16000";
/// What comes back: 16-bit PCM at 24 kHz, mono.
pub const OUTPUT_RATE: u32 = 24_000;
/// One chunk of microphone audio, at most. About two seconds.
pub const MAX_CHUNK_BYTES: usize = 64_000;
const MAX_TRANSCRIPT: usize = 2_000;

/// The socket address. The key is a query parameter, so this string is a secret: never log it.
pub fn endpoint(key: &str) -> String {
    format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={key}"
    )
}

/// Who the model is and how it talks. English adds a plain spoken style; Pidgin adds the Lagos persona and the
/// qualities of the voice itself (weight, pace, rhythm), which the audio model follows.
pub fn instruction(language: VoiceLanguage) -> String {
    let mut text = String::from(
        "You are Oma, a warm, friendly personal assistant who lives at the top of the user's screen. \
You are speaking out loud, so keep every reply to a few short sentences in plain words, and never read out links, code or long lists. \
You cannot send messages, emails or reminders or take any action in this voice conversation: if asked, say that those are done from the chat box, where the person confirms them.",
    );
    match language {
        VoiceLanguage::English => {
            text.push_str(" Speak clearly and warmly in English at a moderate conversational pace.");
        }
        VoiceLanguage::Pidgin => {
            text.push_str(language.style_prompt());
            text.push_str(
                " Speak in a warm, street-smart Lagos voice: light to medium in weight, not deep, at a moderate conversational pace, \
with the musical, lilting, syllable-timed rhythm of Nigerian Pidgin.",
            );
        }
    }
    text
}

/// The first message on the socket.
pub fn setup_message(model: &str, voice: &str, language: VoiceLanguage) -> String {
    json!({
        "setup": {
            "model": format!("models/{model}"),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } }
            },
            "systemInstruction": { "parts": [{ "text": instruction(language) }] },
            // The person decides when they are speaking, by holding the mic button.
            "realtimeInputConfig": { "automaticActivityDetection": { "disabled": true } },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {}
        }
    })
    .to_string()
}

pub fn activity_start() -> String {
    json!({ "realtimeInput": { "activityStart": {} } }).to_string()
}

pub fn activity_end() -> String {
    json!({ "realtimeInput": { "activityEnd": {} } }).to_string()
}

/// One chunk of microphone audio. None when it is empty, odd in length or too long.
pub fn audio_message(pcm: &[u8]) -> Option<String> {
    if pcm.is_empty() || pcm.len() % 2 != 0 || pcm.len() > MAX_CHUNK_BYTES {
        return None;
    }
    Some(
        json!({ "realtimeInput": { "audio": { "data": STANDARD.encode(pcm), "mimeType": INPUT_MIME } } })
            .to_string(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    /// What the person said, as heard by the model.
    User,
    /// What the model said.
    Model,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveEvent {
    /// The service accepted the setup. Audio may flow.
    Ready,
    /// Sound to play: 16-bit PCM, 24 kHz, mono.
    Audio(Vec<u8>),
    /// A piece of a transcript, cleaned. Pieces of one turn arrive in order.
    Text { speaker: Speaker, text: String },
    /// The person spoke over the model: stop playing what is queued.
    Interrupted,
    TurnComplete,
    /// The service is about to end the session.
    GoAway,
}

/// Reads one message from the service. One message can carry several things (sound and words together), so
/// this returns a list. Anything unknown or malformed gives an empty list: it is ignored, never trusted.
pub fn parse_server_message(raw: &str) -> Vec<LiveEvent> {
    let Ok(message) = serde_json::from_str::<Value>(raw) else {
        return vec![];
    };
    let mut events = vec![];
    if message.get("setupComplete").is_some() {
        events.push(LiveEvent::Ready);
    }
    if message.get("goAway").is_some() {
        events.push(LiveEvent::GoAway);
    }
    let Some(content) = message.get("serverContent") else {
        return events;
    };
    if let Some(parts) = content.pointer("/modelTurn/parts").and_then(Value::as_array) {
        for part in parts {
            let Some(blob) = part.get("inlineData") else { continue };
            let mime = blob.get("mimeType").and_then(Value::as_str).unwrap_or("");
            if !mime.starts_with("audio/pcm") {
                continue;
            }
            let Some(data) = blob.get("data").and_then(Value::as_str) else { continue };
            if let Ok(bytes) = STANDARD.decode(data) {
                if !bytes.is_empty() && bytes.len() % 2 == 0 {
                    events.push(LiveEvent::Audio(bytes));
                }
            }
        }
    }
    for (field, speaker) in [("inputTranscription", Speaker::User), ("outputTranscription", Speaker::Model)] {
        if let Some(text) = content.pointer(&format!("/{field}/text")).and_then(Value::as_str) {
            // Pieces keep their own spaces, so only hidden characters go and the ends are left alone.
            let cleaned = sanitize_piece(text);
            if !cleaned.is_empty() {
                events.push(LiveEvent::Text { speaker, text: cleaned });
            }
        }
    }
    if content.get("interrupted").and_then(Value::as_bool) == Some(true) {
        events.push(LiveEvent::Interrupted);
    }
    if content.get("turnComplete").and_then(Value::as_bool) == Some(true) {
        events.push(LiveEvent::TurnComplete);
    }
    events
}

fn sanitize_piece(text: &str) -> String {
    let cleaned = crate::text::strip_hidden(text, false);
    let capped = sanitize(&cleaned, MAX_TRANSCRIPT);
    // `sanitize` trims the ends; a piece such as " world" needs its leading space to join "hello".
    if cleaned.starts_with(' ') && !capped.is_empty() {
        format!(" {capped}")
    } else {
        capped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: Value) -> Vec<LiveEvent> {
        parse_server_message(&v.to_string())
    }

    #[test]
    fn setup_turns_off_automatic_listening_and_asks_for_audio_and_transcripts() {
        let v: Value = serde_json::from_str(&setup_message("gemini-3.8-live", "Puck", VoiceLanguage::Pidgin)).unwrap();
        let s = &v["setup"];
        assert_eq!(s["model"], "models/gemini-3.8-live");
        assert_eq!(s["generationConfig"]["responseModalities"][0], "AUDIO");
        assert_eq!(s["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"], "Puck");
        assert_eq!(s["realtimeInputConfig"]["automaticActivityDetection"]["disabled"], true);
        assert!(s["inputAudioTranscription"].is_object() && s["outputAudioTranscription"].is_object());
        assert!(s.get("tools").is_none(), "the voice conversation has no tools");
        assert!(s.get("responseModalities").is_none(), "it belongs inside generationConfig");
    }

    #[test]
    fn instruction_carries_the_persona_and_says_it_cannot_act() {
        let pidgin = instruction(VoiceLanguage::Pidgin);
        for must in ["Nigerian Pidgin", "Lagos-influenced", "street-smart", "lilting", "cannot send", "chat box"] {
            assert!(pidgin.contains(must), "missing {must}");
        }
        let english = instruction(VoiceLanguage::English);
        assert!(!english.contains("Pidgin") && english.contains("cannot send") && english.contains("English"));
    }

    #[test]
    fn audio_goes_out_as_base64_with_its_rate() {
        let v: Value = serde_json::from_str(&audio_message(&[1, 2, 3, 4]).unwrap()).unwrap();
        assert_eq!(v["realtimeInput"]["audio"]["mimeType"], "audio/pcm;rate=16000");
        assert_eq!(STANDARD.decode(v["realtimeInput"]["audio"]["data"].as_str().unwrap()).unwrap(), vec![1, 2, 3, 4]);
        assert!(audio_message(&[]).is_none() && audio_message(&[1, 2, 3]).is_none());
        assert!(audio_message(&vec![0; MAX_CHUNK_BYTES + 2]).is_none());
        assert!(audio_message(&vec![0; MAX_CHUNK_BYTES]).is_some());
    }

    #[test]
    fn activity_signals_are_empty_objects() {
        assert_eq!(activity_start(), r#"{"realtimeInput":{"activityStart":{}}}"#);
        assert_eq!(activity_end(), r#"{"realtimeInput":{"activityEnd":{}}}"#);
    }

    #[test]
    fn a_message_with_sound_and_words_gives_both() {
        let events = parse(json!({ "serverContent": {
            "modelTurn": { "parts": [
                { "inlineData": { "mimeType": "audio/pcm;rate=24000", "data": STANDARD.encode([9u8, 0, 8, 0]) } },
                { "text": "thinking out loud" }
            ] },
            "outputTranscription": { "text": " how far" },
            "turnComplete": true
        } }));
        assert_eq!(
            events,
            vec![
                LiveEvent::Audio(vec![9, 0, 8, 0]),
                LiveEvent::Text { speaker: Speaker::Model, text: " how far".into() },
                LiveEvent::TurnComplete
            ]
        );
    }

    #[test]
    fn setup_complete_interruption_go_away_and_user_words() {
        assert_eq!(parse(json!({ "setupComplete": {} })), vec![LiveEvent::Ready]);
        assert_eq!(parse(json!({ "goAway": { "timeLeft": "5s" } })), vec![LiveEvent::GoAway]);
        assert_eq!(parse(json!({ "serverContent": { "interrupted": true } })), vec![LiveEvent::Interrupted]);
        assert_eq!(
            parse(json!({ "serverContent": { "inputTranscription": { "text": "write to Tunde" } } })),
            vec![LiveEvent::Text { speaker: Speaker::User, text: "write to Tunde".into() }]
        );
    }

    #[test]
    fn junk_is_ignored() {
        assert!(parse_server_message("not json").is_empty());
        assert!(parse(json!({ "usageMetadata": {} })).is_empty());
        // not audio, bad base64, odd length, empty
        for part in [
            json!({ "inlineData": { "mimeType": "image/png", "data": "AAAA" } }),
            json!({ "inlineData": { "mimeType": "audio/pcm", "data": "!!!" } }),
            json!({ "inlineData": { "mimeType": "audio/pcm", "data": STANDARD.encode([1u8]) } }),
            json!({ "inlineData": { "mimeType": "audio/pcm", "data": "" } }),
        ] {
            assert!(parse(json!({ "serverContent": { "modelTurn": { "parts": [part] } } })).is_empty());
        }
        assert!(parse(json!({ "serverContent": { "outputTranscription": { "text": "\u{200b}" } } })).is_empty());
    }

    #[test]
    fn hidden_characters_never_reach_the_chat() {
        let events = parse(json!({ "serverContent": { "outputTranscription": { "text": "he\u{202e}llo" } } }));
        assert_eq!(events, vec![LiveEvent::Text { speaker: Speaker::Model, text: "hello".into() }]);
    }

    #[test]
    fn the_endpoint_carries_the_key_as_a_query() {
        assert!(endpoint("K").ends_with("BidiGenerateContent?key=K"));
        assert!(endpoint("K").starts_with("wss://generativelanguage.googleapis.com/ws/"));
    }
}
