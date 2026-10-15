//! The shared test vectors in tests/vectors/*.json. The Swift core on the Mac and the TypeScript for reading aloud
//! run the same files, so a rule that drifts in one of them fails here or there.

use coucou_assistant::text::strip_hidden;
use coucou_assistant::voice::{
    clean_transcript, VoiceCommand, VoiceFailure, VoicePhrase, VoiceSession, VoiceState, MAX_TRANSCRIPT,
};
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = format!("{}/../../tests/vectors/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} is not valid JSON: {e}"))
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_else(|| panic!("missing string {key} in {v}")).to_string()
}

#[test]
fn strip_hidden_matches_the_vectors() {
    let file = load("strip_hidden.json");
    let cases = file["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "the vectors went missing");
    for case in cases {
        let got = strip_hidden(&text(case, "input"), case["keepLineBreaks"].as_bool().unwrap());
        assert_eq!(got, text(case, "expected"), "{}", text(case, "name"));
    }
}

#[test]
fn transcripts_match_the_vectors() {
    let file = load("voice.json");
    let cases = file["transcript"].as_array().unwrap();
    assert!(cases.len() >= 8);
    for case in cases {
        let limit = case.get("limit").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(MAX_TRANSCRIPT);
        let got = clean_transcript(&text(case, "input"), limit);
        let expected = case["expected"].as_str().map(str::to_string);
        assert_eq!(got, expected, "{}", text(case, "name"));
    }
}

fn phrase_name(p: VoicePhrase) -> &'static str {
    match p {
        VoicePhrase::DidNotCatch => "didNotCatch",
        VoicePhrase::MicBlocked => "micBlocked",
        // The Mac calls this one needsOnDevice. Same line, wider name here.
        VoicePhrase::NeedsRecognizer => "needsOnDevice",
        other => panic!("the vectors do not cover {other:?}"),
    }
}

fn command_name(c: &VoiceCommand) -> String {
    match c {
        VoiceCommand::StartListening => "startListening".into(),
        VoiceCommand::StopListening => "stopListening".into(),
        VoiceCommand::Discard => "discard".into(),
        VoiceCommand::Send(words) => format!("send:{words}"),
        VoiceCommand::Hint(p) => format!("hint:{}", phrase_name(*p)),
    }
}

fn state_name(s: VoiceState) -> &'static str {
    match s {
        VoiceState::Idle => "idle",
        VoiceState::Listening => "listening",
        VoiceState::Finishing => "finishing",
    }
}

#[test]
fn push_to_talk_matches_the_vectors() {
    let file = load("voice.json");
    let scripts = file["sessions"].as_array().unwrap();
    assert!(scripts.len() >= 10);
    for script in scripts {
        let name = text(script, "name");
        let mut session = VoiceSession::new();
        for (i, step) in script["steps"].as_array().unwrap().iter().enumerate() {
            let at = step.get("at").and_then(Value::as_f64).unwrap_or(0.0);
            let commands = match text(step, "op").as_str() {
                "press" => Some(session.press(at)),
                "release" => Some(session.release(at)),
                "tick" => Some(session.tick(at)),
                "finished" => Some(session.finished(step["transcript"].as_str())),
                "failed" => Some(session.failed(match text(step, "kind").as_str() {
                    "micBlocked" => VoiceFailure::MicBlocked,
                    "unavailable" => VoiceFailure::Unavailable,
                    "other" => VoiceFailure::Other,
                    k => panic!("unknown failure {k}"),
                })),
                "cancel" => Some(session.cancel()),
                "heard" => {
                    session.heard(&text(step, "text"));
                    None
                }
                op => panic!("unknown op {op}"),
            };
            let at_step = format!("{name}, step {} ({})", i + 1, text(step, "op"));
            if let (Some(commands), Some(expected)) = (commands, step.get("expect")) {
                let got: Vec<String> = commands.iter().map(command_name).collect();
                let want: Vec<String> =
                    expected.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
                assert_eq!(got, want, "{at_step}");
            }
            if let Some(state) = step.get("state").and_then(Value::as_str) {
                assert_eq!(state_name(session.state()), state, "{at_step}: state");
            }
            if let Some(partial) = step.get("partial").and_then(Value::as_str) {
                assert_eq!(session.partial(), partial, "{at_step}: partial");
            }
        }
    }
}
