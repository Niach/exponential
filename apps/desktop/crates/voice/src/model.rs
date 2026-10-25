//! The Kokoro voice on disk: downloaded once into `{data_dir}/talk/`,
//! verified against a pinned SHA-256 (the release's published digest, the
//! way `coding::managed_codex` pins Codex), unpacked with ONLY the files an
//! English Kokoro needs. The bundle also carries `espeak-ng-data` (GPL data
//! files a lexicon voice never reads); it is skipped, never written.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// One pinned model archive.
pub struct ModelSpec {
    pub url: &'static str,
    pub sha256: &'static str,
    /// The archive's top-level directory = the directory under `talk/`.
    pub dir_name: &'static str,
    /// The files kept from the archive (by name, inside `dir_name`).
    pub keep: &'static [&'static str],
}

/// Kokoro v1.0 (Apache-2.0 weights), the full-precision bundle the owner
/// heard on the macbook (2026-10-25). sha256 = the GitHub release asset
/// digest. ~350 MB download, ~340 MB on disk after the skip.
pub const KOKORO: ModelSpec = ModelSpec {
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2",
    sha256: "c5f7e2d2caf082bc1d20fb70334a61d99d20b484500aad32e7cf84c128ea3298",
    dir_name: "kokoro-multi-lang-v1_0",
    keep: &["model.onnx", "voices.bin", "tokens.txt", "lexicon-us-en.txt", "LICENSE"],
};

const MARKER: &str = ".exp-sha256-verified";

/// Where `spec` lives under `data_dir`.
pub fn model_dir(data_dir: &Path, spec: &ModelSpec) -> PathBuf {
    data_dir.join("talk").join(spec.dir_name)
}

/// Whether `spec` is on disk and verified.
pub fn is_ready(data_dir: &Path, spec: &ModelSpec) -> bool {
    let dir = model_dir(data_dir, spec);
    fs::read_to_string(dir.join(MARKER)).is_ok_and(|text| text.trim() == spec.sha256)
        && spec.keep.iter().all(|name| dir.join(name).is_file())
}

/// Make `spec` ready: a no-op when it already is, else download, verify,
/// unpack and mark it. `progress(received, total)` runs while downloading
/// (`total` 0 = unknown).
pub fn ensure(data_dir: &Path, spec: &ModelSpec, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, String> {
    let dir = model_dir(data_dir, spec);
    if is_ready(data_dir, spec) {
        return Ok(dir);
    }
    let talk_dir = data_dir.join("talk");
    fs::create_dir_all(&talk_dir).map_err(|err| format!("create {}: {err}", talk_dir.display()))?;
    let archive = talk_dir.join(format!("{}.tar.bz2.part", spec.dir_name));
    download(spec, &archive, &mut progress)?;
    let result = unpack(spec, &archive, &dir);
    let _ = fs::remove_file(&archive);
    result?;
    fs::write(dir.join(MARKER), spec.sha256).map_err(|err| format!("mark {}: {err}", dir.display()))?;
    Ok(dir)
}

fn download(spec: &ModelSpec, archive: &Path, progress: &mut impl FnMut(u64, u64)) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| format!("http client: {err}"))?;
    let mut response = client
        .get(spec.url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("download {}: {err}", spec.url))?;
    let total = response.content_length().unwrap_or(0);
    let mut file = File::create(archive).map_err(|err| format!("create {}: {err}", archive.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut received = 0u64;
    let mut reported = 0u64;
    loop {
        let read = response.read(&mut buffer).map_err(|err| format!("download read: {err}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|err| format!("write {}: {err}", archive.display()))?;
        received += read as u64;
        // ~1% steps (every 4 MB when the size is unknown).
        let step = if total > 0 { total / 100 } else { 4 * 1024 * 1024 };
        if received - reported >= step.max(1) {
            reported = received;
            progress(received, total);
        }
    }
    file.flush().map_err(|err| format!("flush: {err}"))?;
    progress(received, total);
    let actual: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    if actual != spec.sha256 {
        let _ = fs::remove_file(archive);
        return Err(format!("voice download failed verification: sha256 {actual}, pinned {}", spec.sha256));
    }
    Ok(())
}

fn unpack(spec: &ModelSpec, archive: &Path, dir: &Path) -> Result<(), String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|err| format!("clear {}: {err}", dir.display()))?;
    }
    fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    let file = File::open(archive).map_err(|err| format!("open {}: {err}", archive.display()))?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(io::BufReader::new(file)));
    let entries = tar.entries().map_err(|err| format!("read archive: {err}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| format!("read archive entry: {err}"))?;
        let path = entry.path().map_err(|err| format!("archive path: {err}"))?.into_owned();
        // Only `<dir_name>/<kept file>`: no nesting, no `..`, no espeak data.
        let mut components = path.components();
        let (Some(top), Some(name), None) = (components.next(), components.next(), components.next()) else {
            continue;
        };
        if top.as_os_str() != spec.dir_name {
            continue;
        }
        let Some(name) = name.as_os_str().to_str().filter(|name| spec.keep.contains(name)) else {
            continue;
        };
        let target = dir.join(name);
        let mut out = File::create(&target).map_err(|err| format!("create {}: {err}", target.display()))?;
        io::copy(&mut entry, &mut out).map_err(|err| format!("unpack {name}: {err}"))?;
    }
    for name in spec.keep {
        if !dir.join(name).is_file() {
            return Err(format!("voice archive is missing {name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bz2_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap()
    }

    const SPEC: ModelSpec = ModelSpec {
        url: "unused",
        sha256: "unused",
        dir_name: "voice-x",
        keep: &["model.onnx", "tokens.txt"],
    };

    #[test]
    fn unpack_keeps_only_the_listed_files() {
        let root = std::env::temp_dir().join(format!("voice-unpack-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.bz2");
        fs::write(
            &archive,
            bz2_tar(&[
                ("voice-x/model.onnx", b"m"),
                ("voice-x/tokens.txt", b"t"),
                ("voice-x/espeak-ng-data/de_dict", b"gpl"),
                ("voice-x/README.md", b"r"),
                ("other/model.onnx", b"x"),
            ]),
        )
        .unwrap();
        let dir = root.join("out");
        unpack(&SPEC, &archive, &dir).unwrap();
        let mut names: Vec<String> =
            fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, ["model.onnx", "tokens.txt"]);
        assert_eq!(fs::read(dir.join("model.onnx")).unwrap(), b"m");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unpack_fails_when_a_kept_file_is_missing() {
        let root = std::env::temp_dir().join(format!("voice-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("a.tar.bz2");
        fs::write(&archive, bz2_tar(&[("voice-x/model.onnx", b"m")])).unwrap();
        let err = unpack(&SPEC, &archive, &root.join("out")).unwrap_err();
        assert!(err.contains("tokens.txt"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn readiness_needs_the_marker_and_the_files() {
        let root = std::env::temp_dir().join(format!("voice-ready-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let spec = ModelSpec { sha256: "abc", ..SPEC };
        let dir = model_dir(&root, &spec);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("model.onnx"), b"m").unwrap();
        fs::write(dir.join("tokens.txt"), b"t").unwrap();
        assert!(!is_ready(&root, &spec));
        fs::write(dir.join(MARKER), "abc").unwrap();
        assert!(is_ready(&root, &spec));
        fs::write(dir.join(MARKER), "other").unwrap();
        assert!(!is_ready(&root, &spec));
        let _ = fs::remove_dir_all(&root);
    }
}
