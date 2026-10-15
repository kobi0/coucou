#![cfg_attr(not(feature = "local-speech"), allow(dead_code))] // the helpers below are used by the recogniser only
// On-device speech recognition for hold-to-talk. The page records while the mic button is held and sends the
// whole clip here as 16 kHz mono 16-bit PCM; whisper.cpp turns it into text on this computer. Audio is never
// sent anywhere and is never written to disk. What comes back is only text for the chat box.
//
// The model is a file the person puts in `<config>/models` (see windows/README.md). Nothing is downloaded by
// the app. Without the `local-speech` feature this build has no recogniser and the mic stays hidden.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// 16 kHz mono, as Whisper expects.
pub const SAMPLE_RATE: usize = 16_000;
/// A clip longer than this is refused. The hold-to-talk machine already stops at 60 s.
const MAX_SECONDS: usize = 65;
/// Whisper wants at least a second of audio; shorter clips are padded with silence.
const MIN_SAMPLES: usize = SAMPLE_RATE;
/// A model file smaller than this is a partial download, not a model.
const MIN_MODEL_BYTES: u64 = 1_000_000;

/// Model files in the order they are preferred. English-only models are quicker and more accurate for English.
const MODEL_PREFERENCE: &[&str] = &[
    "ggml-small.en.bin",
    "ggml-base.en.bin",
    "ggml-tiny.en.bin",
    "ggml-small.bin",
    "ggml-base.bin",
    "ggml-tiny.bin",
];

pub fn models_dir() -> PathBuf {
    crate::settings::config_dir().join("models")
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognizerStatus {
    /// "ready", "noModel" (built in, but no model file yet) or "notBuilt" (this build has no recogniser).
    pub state: &'static str,
    pub model: Option<String>,
    pub folder: String,
}

/// The preferred model among the file names found, or any other `ggml-*.bin` as a last resort.
pub fn pick_model(names: &[String]) -> Option<String> {
    for wanted in MODEL_PREFERENCE {
        if names.iter().any(|n| n.eq_ignore_ascii_case(wanted)) {
            return names.iter().find(|n| n.eq_ignore_ascii_case(wanted)).cloned();
        }
    }
    let mut others: Vec<&String> = names
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.starts_with("ggml-") && l.ends_with(".bin")
        })
        .collect();
    others.sort();
    others.first().map(|n| (*n).clone())
}

fn find_model(dir: &Path) -> Option<PathBuf> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.metadata().map(|m| m.is_file() && m.len() >= MIN_MODEL_BYTES).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    pick_model(&names).map(|n| dir.join(n))
}

pub fn status() -> RecognizerStatus {
    let folder = models_dir();
    let shown = folder.display().to_string();
    if !cfg!(feature = "local-speech") {
        return RecognizerStatus { state: "notBuilt", model: None, folder: shown };
    }
    match find_model(&folder) {
        Some(path) => RecognizerStatus {
            state: "ready",
            model: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            folder: shown,
        },
        None => RecognizerStatus { state: "noModel", model: None, folder: shown },
    }
}

/// 16-bit little-endian PCM to samples in -1..1. Refuses odd lengths and anything over the longest clip.
pub fn decode_pcm16(bytes: &[u8]) -> Result<Vec<f32>, &'static str> {
    if bytes.len() % 2 != 0 {
        return Err("bad_audio");
    }
    if bytes.len() / 2 > MAX_SECONDS * SAMPLE_RATE {
        return Err("too_long");
    }
    Ok(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
}

/// Whisper marks sounds and silence in brackets, parentheses or with music notes: `[BLANK_AUDIO]`, `(music)`,
/// `♪`. None of that is something the person said.
pub fn clean_text(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut depth = 0u32;
    for ch in raw.chars() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            '♪' | '♫' | '♬' => {}
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() { None } else { Some(collapsed) }
}

/// Pads a short clip with silence up to what Whisper accepts.
pub fn pad(mut samples: Vec<f32>) -> Vec<f32> {
    if samples.len() < MIN_SAMPLES {
        samples.resize(MIN_SAMPLES, 0.0);
    }
    samples
}

// ── The recogniser itself ─────────────────────────────────────────────────────

#[derive(Default)]
pub struct Engine {
    #[cfg(feature = "local-speech")]
    loaded: std::sync::Mutex<Option<(PathBuf, whisper_rs::WhisperContext)>>,
}

impl Engine {
    /// Turns a clip into words. Blocks, so call it off the UI thread. `Ok(None)` means nothing was heard.
    #[cfg(feature = "local-speech")]
    pub fn transcribe(&self, pcm: &[u8]) -> Result<Option<String>, String> {
        use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

        let samples = pad(decode_pcm16(pcm).map_err(str::to_string)?);
        let path = find_model(&models_dir()).ok_or_else(|| "no_model".to_string())?;

        let mut slot = self.loaded.lock().unwrap();
        if slot.as_ref().map(|(p, _)| p != &path).unwrap_or(true) {
            let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
                .map_err(|e| format!("model: {e}"))?;
            *slot = Some((path, ctx));
        }
        let (_, ctx) = slot.as_ref().unwrap();
        let mut state = ctx.create_state().map_err(|e| format!("state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // English for both assistant languages: no Pidgin recogniser exists, as on the Mac.
        params.set_language(Some("en"));
        params.set_translate(false);
        params.set_n_threads(std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2) as i32);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);

        state.full(params, &samples).map_err(|e| format!("run: {e}"))?;

        let mut words = String::new();
        for i in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(i) else { continue };
            // A segment Whisper itself thinks is silence is dropped, so noise does not become words.
            if segment.no_speech_probability() > 0.6 {
                continue;
            }
            if let Ok(text) = segment.to_str_lossy() {
                words.push(' ');
                words.push_str(&text);
            }
        }
        Ok(clean_text(&words))
    }

    #[cfg(not(feature = "local-speech"))]
    pub fn transcribe(&self, _pcm: &[u8]) -> Result<Option<String>, String> {
        Err("not_built".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_english_model_is_preferred_and_the_larger_one_wins() {
        assert_eq!(pick_model(&names(&["ggml-base.bin", "ggml-base.en.bin"])).as_deref(), Some("ggml-base.en.bin"));
        assert_eq!(pick_model(&names(&["ggml-tiny.en.bin", "ggml-small.en.bin"])).as_deref(), Some("ggml-small.en.bin"));
        assert_eq!(pick_model(&names(&["notes.txt"])), None);
        assert_eq!(pick_model(&names(&["ggml-medium.en.bin", "readme.md"])).as_deref(), Some("ggml-medium.en.bin"));
        assert_eq!(pick_model(&names(&[])), None);
    }

    #[test]
    fn pcm_decodes_and_refuses_bad_clips() {
        let ok = decode_pcm16(&[0x00, 0x40, 0x00, 0xC0, 0xFF, 0x7F]).unwrap();
        assert_eq!(ok.len(), 3);
        assert!((ok[0] - 0.5).abs() < 1e-6 && (ok[1] + 0.5).abs() < 1e-6 && ok[2] < 1.0);
        assert_eq!(decode_pcm16(&[1, 2, 3]), Err("bad_audio"));
        assert_eq!(decode_pcm16(&vec![0; (MAX_SECONDS * SAMPLE_RATE + 1) * 2]), Err("too_long"));
        assert_eq!(decode_pcm16(&[]).unwrap().len(), 0);
    }

    #[test]
    fn annotations_are_not_words() {
        assert_eq!(clean_text(" [BLANK_AUDIO] "), None);
        assert_eq!(clean_text("(music) ♪♪"), None);
        assert_eq!(clean_text(" write to  Tunde [pause] now "), Some("write to Tunde now".into()));
        assert_eq!(clean_text("call (Ada) back"), Some("call back".into()));
        assert_eq!(clean_text("broken ] bracket"), Some("broken bracket".into()));
    }

    #[test]
    fn short_clips_are_padded_to_a_second() {
        assert_eq!(pad(vec![0.1; 100]).len(), SAMPLE_RATE);
        assert_eq!(pad(vec![0.1; SAMPLE_RATE + 5]).len(), SAMPLE_RATE + 5);
    }

    #[test]
    fn without_the_feature_there_is_no_recogniser() {
        if !cfg!(feature = "local-speech") {
            assert_eq!(status().state, "notBuilt");
            assert_eq!(Engine::default().transcribe(&[0, 0]), Err("not_built".to_string()));
        }
    }

    #[cfg(feature = "local-speech")]
    #[test]
    fn a_bad_model_file_is_an_error_not_a_crash() {
        let path = std::env::temp_dir().join(format!("coucou-bad-model-{}.bin", std::process::id()));
        std::fs::write(&path, vec![7u8; 2_000_000]).unwrap();
        let loaded = whisper_rs::WhisperContext::new_with_params(&path, whisper_rs::WhisperContextParameters::default());
        let _ = std::fs::remove_file(&path);
        assert!(loaded.is_err());
    }
}
