//! EXP-1424: which phrases have a model, and the pinned files.
//!
//! Every file is a pinned download (`url` + its sha256 + size); the speech
//! worker fetches and verifies them like its other models, nothing is in
//! the binary. A phrase without a model here keeps the transcript matcher
//! (talk-listen.json `wake.detector`).

/// One pinned file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    /// Unique; names the file's manifest.
    pub id: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    /// The file name written.
    pub file: &'static str,
}

/// openWakeWord v0.5.1's front end (Apache-2.0; the embedding is Google's
/// speech_embedding, Apache-2.0); byte-identical to the copies
/// livekit-wakeword 0.1.3 bundles.
pub const MELSPECTROGRAM: Pin = Pin {
    id: "oww-melspectrogram-0.5.1",
    url: "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/melspectrogram.onnx",
    sha256: "ba2b0e0f8b7b875369a2c89cb13360ff53bac436f2895cced9f479fa65eb176f",
    size: 1_087_958,
    file: "melspectrogram.onnx",
};

pub const EMBEDDING: Pin = Pin {
    id: "oww-embedding-0.5.1",
    url: "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/embedding_model.onnx",
    sha256: "70d164290c1d095d1d4ee149bc5e00543250a7316b59f31d056cff7bd3075c1f",
    size: 1_326_578,
    file: "embedding_model.onnx",
};

/// One phrase's classifier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub id: &'static str,
    /// The phrases it detects, as [`phrase_key`] writes them.
    pub phrases: &'static [&'static str],
    pub classifier: Pin,
    /// [`crate::Detector::new`]'s `scale` (1.0: trained by livekit-wakeword).
    pub scale: f32,
    pub threshold: f32,
}

impl Model {
    /// Pinned for real: an unpublished model (no digest yet) is never used.
    pub fn published(&self) -> bool {
        !self.classifier.sha256.is_empty() && !self.classifier.url.is_empty()
    }

    /// Every file it needs, the front end first.
    pub fn files(&self) -> [Pin; 3] {
        [MELSPECTROGRAM, EMBEDDING, self.classifier]
    }
}

/// Frames a detection needs in a row, and the quiet after one (80 ms frames).
pub const PATIENCE: usize = 2;
pub const DEBOUNCE: usize = 25;
/// The default threshold; a model carries its own, measured at training.
pub const THRESHOLD: f32 = 0.5;

/// spezl's phrase (#YO-1), trained by `train/train.sh` with
/// `train/hey_spezl.yaml` (2026-10-29, livekit-wakeword conv-attention
/// small, 10k Piper samples per class, 30k steps). Committed at
/// `models/hey_spezl.onnx` and downloaded from the repository's master
/// branch (the sha256 is the pin; until it lands there the download fails
/// and the transcript matcher hears the phrase). Its threshold is the
/// trainer's optimum over 19.2 h of validation audio (`models/
/// hey_spezl_eval.json`): 0.10 false accepts per hour at 83 % recall
/// (at 0.5: 0 per hour, 42 %). Known confusion: `Hey special`.
pub const HEY_SPEZL: Model = Model {
    id: "hey_spezl",
    phrases: &["hey spezl"],
    classifier: Pin {
        id: "hey-spezl-v1",
        url: "https://raw.githubusercontent.com/Niach/exponential/master/apps/desktop/crates/etalk-wakeword/models/hey_spezl.onnx",
        sha256: "d075fbc74818736b4b5e251b1124975c9a7b2274b3455acf9eb1cc50c498dcfc",
        size: 172_445,
        file: "hey_spezl.onnx",
    },
    scale: 1.0,
    threshold: 0.18,
};

/// The shipped models.
pub const MODELS: &[Model] = &[HEY_SPEZL];

/// A phrase as the registry compares it: its words (runs of letters and
/// digits, lowercased) joined by one space (`Hey, Spezl!` = `hey spezl`).
pub fn phrase_key(phrase: &str) -> String {
    phrase
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The published model of `phrase` among `models`, if any.
pub fn model_in<'a>(models: &'a [Model], phrase: &str) -> Option<&'a Model> {
    let key = phrase_key(phrase);
    models.iter().find(|model| model.published() && model.phrases.contains(&key.as_str()))
}

/// The shipped model of `phrase`, if any.
pub fn model_for(phrase: &str) -> Option<&'static Model> {
    model_in(MODELS, phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed model is the pinned one, and it loads.
    #[test]
    fn hey_spezl_is_the_committed_file() {
        use sha2::Digest;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("models").join(HEY_SPEZL.classifier.file);
        let bytes = std::fs::read(&path).unwrap();
        let digest: String = sha2::Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(digest, HEY_SPEZL.classifier.sha256);
        assert_eq!(bytes.len() as u64, HEY_SPEZL.classifier.size);
        assert!(HEY_SPEZL.published());
        assert!(HEY_SPEZL.classifier.url.ends_with(&format!("crates/etalk-wakeword/models/{}", HEY_SPEZL.classifier.file)));
        crate::Classifier::from_bytes(&bytes).expect("tract loads it");
    }

    #[test]
    fn keys_fold_case_and_punctuation() {
        assert_eq!(phrase_key("Hey, Spezl!"), "hey spezl");
        assert_eq!(phrase_key("  hey   SPEZL "), "hey spezl");
        assert_eq!(phrase_key("Hey Studio"), "hey studio");
    }

    #[test]
    fn an_unpublished_model_is_never_picked() {
        let draft = Model { classifier: Pin { url: "", sha256: "", ..HEY_SPEZL.classifier }, ..HEY_SPEZL };
        assert_eq!(model_in(&[draft], "Hey spezl"), None);
        let published = Model { classifier: Pin { url: "https://example.invalid/m.onnx", sha256: "00", ..HEY_SPEZL.classifier }, ..HEY_SPEZL };
        assert_eq!(model_in(&[published], "hey, Spezl").map(|model| model.id), Some("hey_spezl"));
        assert_eq!(model_in(&[published], "Hey Studio"), None);
    }
}
