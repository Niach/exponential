//! The pinned model files (Hugging Face LFS objects; sha256 = the LFS oid).

use crate::artifact::{Artifact, Pack};

/// The directory under `{data_dir}/talk/` the models download into.
pub const MODELS_DIR: &str = "models";

/// Qwen3-TTS 12 Hz 0.6B Base, F16 (Apache-2.0; cstr's GGUF conversion for
/// CrispASR): the voice-cloning talker. F16, not Q8_0: in the NEW-74
/// measurement it cloned within 0.01 speaker similarity of the reference
/// implementation, Q8_0 lost 0.025.
pub const QWEN3_TTS_0_6B_BASE: Artifact = Artifact {
    id: "qwen3-tts-12hz-0.6b-base",
    url: "https://huggingface.co/cstr/qwen3-tts-0.6b-base-GGUF/resolve/94072f5c394b4dc4d98e97cc874800dde6ecacda/qwen3-tts-12hz-0.6b-base.gguf",
    sha256: "9b719418cf9d31fc8472a60ecdab3553280d4b15a9cbfc1363ce9352d64af6dc",
    size: 1_834_817_248,
    pack: Pack::File,
    keep: &[("", "qwen3-tts-12hz-0.6b-base.gguf")],
};

/// Its 12 Hz tokenizer / codec, F16 (Apache-2.0); quantising the codec
/// hurts cloning first, so it stays F16.
pub const QWEN3_TTS_CODEC: Artifact = Artifact {
    id: "qwen3-tts-tokenizer-12hz",
    url: "https://huggingface.co/cstr/qwen3-tts-tokenizer-12hz-GGUF/resolve/344e279551177654762e5c7d39e217834325f222/qwen3-tts-tokenizer-12hz.gguf",
    sha256: "70dc95dbfdd9aa5d9d406236ff771d061bf17b0cda02a72513953355606e719b",
    size: 358_453_280,
    pack: Pack::File,
    keep: &[("", "qwen3-tts-tokenizer-12hz.gguf")],
};
