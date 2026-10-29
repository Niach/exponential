//! The built-in voices: one reference clip per language (`assets/`, our own
//! Qwen3-TTS CustomVoice output, see its README), cloned when the member's
//! Voice file is empty or rejected.

use std::path::{Path, PathBuf};

pub struct Builtin {
    pub language: &'static str,
    pub wav: &'static [u8],
    /// The clip's exact words (the clone reads them).
    pub transcript: &'static str,
}

pub const BUILTIN: &[Builtin] = &[
    Builtin {
        language: "en",
        wav: include_bytes!("../assets/builtin-en.wav"),
        transcript: "Good morning. I've looked through the changes, and the tests are passing cleanly now. Just tell me what you need next.",
    },
    Builtin {
        language: "de",
        wav: include_bytes!("../assets/builtin-de.wav"),
        transcript: "Guten Morgen. Ich habe mir die Änderungen angesehen, und die Tests laufen jetzt sauber durch. Sag mir einfach, was du als Nächstes brauchst.",
    },
];

/// The languages the engine speaks (one built-in clip each).
pub fn languages() -> Vec<String> {
    BUILTIN.iter().map(|builtin| builtin.language.to_string()).collect()
}

/// The built-in voice of `language` (English for anything unknown).
pub fn for_language(language: &str) -> &'static Builtin {
    BUILTIN.iter().find(|builtin| builtin.language == language).unwrap_or(&BUILTIN[0])
}

/// Write `builtin`'s clip under `dir` (CrispASR reads a voice from a path)
/// and return the path; rewritten when it differs.
pub fn materialize(dir: &Path, builtin: &Builtin) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    let path = dir.join(format!("builtin-{}.wav", builtin.language));
    if std::fs::read(&path).ok().as_deref() != Some(builtin.wav) {
        std::fs::write(&path, builtin.wav).map_err(|err| format!("write {}: {err}", path.display()))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_wav_clip_and_its_transcript() {
        assert_eq!(languages(), ["en", "de"]);
        for builtin in BUILTIN {
            assert_eq!(&builtin.wav[..4], b"RIFF", "{}", builtin.language);
            assert_eq!(&builtin.wav[8..12], b"WAVE", "{}", builtin.language);
            assert!(builtin.wav.len() > 300_000, "8-15 s of audio: {}", builtin.language);
            assert!(!builtin.transcript.trim().is_empty());
        }
        assert_eq!(for_language("fr").language, "en");
    }
}
