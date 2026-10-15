// Hold-to-talk in the chat: the page records while the mic button is held and sends the clip here as 16 kHz
// mono 16-bit PCM; the speech model Coucou already uses for dictation (speech/, Parakeet, on this computer)
// turns it into text. Audio is never sent anywhere and never written to disk. What comes back is only text for
// the chat box, and hearing never approves anything.
//
// The model is the one downloaded from Settings → Dictation. This file downloads nothing.

use serde::Serialize;

use crate::speech;

const SAMPLE_RATE: usize = 16_000;
/// A clip longer than this is refused. The hold-to-talk machine already stops at 60 s.
const MAX_SECONDS: usize = 65;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognizerStatus {
    /// "ready", or "noModel" (the dictation model is not downloaded yet).
    pub state: &'static str,
    pub model: Option<String>,
    pub folder: String,
}

pub fn status() -> RecognizerStatus {
    let folder = speech::models_dir().display().to_string();
    if speech::is_ready() {
        RecognizerStatus { state: "ready", model: Some("Parakeet".into()), folder }
    } else {
        RecognizerStatus { state: "noModel", model: None, folder }
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

/// Turns a clip into words. Blocks, so call it off the UI thread. `Ok(None)` means nothing was heard.
pub fn transcribe(pcm: &[u8]) -> Result<Option<String>, String> {
    if !speech::is_ready() {
        return Err("no_model".into());
    }
    let samples = decode_pcm16(pcm).map_err(str::to_string)?;
    let heard = speech::transcribe(&samples).ok_or_else(|| "failed".to_string())?;
    let text = heard.text.split_whitespace().collect::<Vec<_>>().join(" ");
    Ok((!text.is_empty()).then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_decodes_to_unit_range_and_refuses_bad_input() {
        let out = decode_pcm16(&[0x00, 0x00, 0x00, 0x80, 0xFF, 0x7F]).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], -1.0);
        assert!(out[2] > 0.999 && out[2] < 1.0);
        assert_eq!(decode_pcm16(&[1, 2, 3]), Err("bad_audio"));
        assert_eq!(decode_pcm16(&vec![0u8; (MAX_SECONDS * SAMPLE_RATE + 1) * 2]), Err("too_long"));
    }
}
