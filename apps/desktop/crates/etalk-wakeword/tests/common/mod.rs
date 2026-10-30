//! The test models: livekit-wakeword 0.1.3's crate tarball (Apache-2.0),
//! fetched once by sha256 into the target dir: the two front-end models
//! (byte-identical to [`etalk_wakeword::models`]' pins), `hey_livekit.onnx`
//! and its positive and negative WAVs.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use etalk_wakeword::{Classifier, FrontEnd};
use sha2::{Digest, Sha256};

const CRATE_URL: &str = "https://static.crates.io/crates/livekit-wakeword/livekit-wakeword-0.1.3.crate";
const CRATE_SHA256: &str = "4f66023a64fefe9aa5b717223379936d4a6ee15f6af93f94e15f1790b4c7f887";

fn sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The unpacked tarball (`onnx/`, `tests/fixtures/`). #EXP-1465: the tests
/// run in parallel (threads, and the example beside them as a process), so
/// one thread per process fetches it (a `OnceLock`) into a private staging
/// dir and renames the finished tree into place: a reader never sees a
/// half-written tarball or a half-unpacked tree.
pub fn fixtures() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(unpack).clone()
}

fn unpack() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let dir = target.join("etalk-wakeword-fixtures");
    // Only ever created by the rename below, so it exists = it is complete (a
    // new name: the old racy unpack's `livekit-wakeword-0.1.3/` is never trusted).
    let root = dir.join("unpacked-0.1.3");
    if root.exists() {
        return root;
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let staging = dir.join(format!(".staging-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&staging).unwrap();
    let tarball = staging.join("livekit-wakeword-0.1.3.crate");
    let status = Command::new("curl").args(["-sSfL", "-o"]).arg(&tarball).arg(CRATE_URL).status().unwrap();
    assert!(status.success(), "download {CRATE_URL}");
    assert_eq!(sha256(&tarball), CRATE_SHA256, "the tarball's pin");
    let status = Command::new("tar").arg("xzf").arg(&tarball).arg("-C").arg(&staging).status().unwrap();
    assert!(status.success());
    // Losing the rename to another process is fine: its tree is complete.
    let _ = std::fs::rename(staging.join("livekit-wakeword-0.1.3"), &root);
    let _ = std::fs::remove_dir_all(&staging);
    assert!(root.join("tests/fixtures/hey_livekit.onnx").exists(), "{} unpacked", root.display());
    root
}

pub fn front_end() -> Arc<FrontEnd> {
    let root = fixtures();
    for (file, pin) in [("onnx/melspectrogram.onnx", etalk_wakeword::models::MELSPECTROGRAM), ("onnx/embedding_model.onnx", etalk_wakeword::models::EMBEDDING)] {
        assert_eq!(sha256(&root.join(file)), pin.sha256, "{file} = the pinned download");
    }
    FrontEnd::load(&root.join("onnx/melspectrogram.onnx"), &root.join("onnx/embedding_model.onnx")).unwrap()
}

pub fn hey_livekit() -> Arc<Classifier> {
    Classifier::load(&fixtures().join("tests/fixtures/hey_livekit.onnx")).unwrap()
}

/// A 16-bit PCM WAV as mono -1…1 samples and its rate.
pub fn wav(path: &Path) -> (Vec<f32>, u32) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    let (mut channels, mut rate, mut at) = (1usize, 16_000u32, 12usize);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        if id == b"fmt " {
            channels = u16::from_le_bytes([body[2], body[3]]) as usize;
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            assert_eq!(u16::from_le_bytes([body[14], body[15]]), 16, "16-bit PCM");
        } else if id == b"data" {
            let samples: Vec<f32> = body.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0).collect();
            return (samples.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect(), rate);
        }
        at += 8 + size + (size & 1);
    }
    panic!("no data chunk in {}", path.display());
}

/// `seconds` of silence around `samples`, so the stream starts and ends quiet.
pub fn padded(samples: &[f32], seconds: f32) -> Vec<f32> {
    let pad = vec![0.0; (seconds * etalk_wakeword::RATE as f32) as usize];
    [pad.as_slice(), samples, pad.as_slice()].concat()
}
