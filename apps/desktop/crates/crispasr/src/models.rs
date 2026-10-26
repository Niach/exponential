//! The pinned model files (Hugging Face LFS objects at a pinned revision;
//! sha256 = the LFS oid).

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

/// NEW-78: Parakeet TDT 0.6B v3, Q8_0 (CC-BY-4.0, NVIDIA; cstr's GGUF
/// conversion for CrispASR): the transcriber, 25 European languages with
/// punctuation and casing. Q8_0, not Q4_K: measured on mint 2026-10-26 at
/// 11.5 % WER on the spike's English set with no content-word errors.
pub const PARAKEET_TDT_0_6B_V3: Artifact = Artifact {
    id: "parakeet-tdt-0.6b-v3-q8_0",
    url: "https://huggingface.co/cstr/parakeet-tdt-0.6b-v3-GGUF/resolve/dff69b0e675921ffd5a6133f5fbe2aac690d3ec3/parakeet-tdt-0.6b-v3-q8_0.gguf",
    sha256: "300de963db10e991a8c3c1674000245546f2e99d396f860aecbde2dd0534e43f",
    size: 674_342_336,
    pack: Pack::File,
    keep: &[("", "parakeet-tdt-0.6b-v3-q8_0.gguf")],
};

/// NEW-78: Silero VAD v6.2 in ggml's format (MIT; ggml-org's conversion
/// for whisper.cpp, which CrispASR runs): speech probability per 32 ms.
pub const SILERO_VAD_V6_2: Artifact = Artifact {
    id: "ggml-silero-v6.2.0",
    url: "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin",
    sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
    size: 885_098,
    pack: Pack::File,
    keep: &[("", "ggml-silero-v6.2.0.bin")],
};
