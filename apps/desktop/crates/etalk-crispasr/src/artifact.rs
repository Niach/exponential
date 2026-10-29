//! Pinned downloads: one [`Artifact`] = one URL with its published sha256.
//!
//! An artifact is verified BEFORE anything reads it: the download streams
//! into `<id>.part` while it is hashed, and a mismatch deletes the file and
//! fails. Only then is a model moved into place or an archive opened, and
//! only the files named in `keep` are written (regular files at exactly
//! that path; no symlinks, no `..`, nothing else of the archive). Each kept
//! file's own sha256 goes into the artifact's manifest
//! (`.<id>.sha256`), and [`verify`] re-hashes EVERY kept file (libraries and
//! models alike) before it is loaded or parsed, so a file changed on disk
//! since is never opened. Measured on mint (SHA-NI, release build): the
//! 1.83 GB model in 0.95 s warm / 1.76 s cold, the codec in 0.2 s; paid once
//! per worker start, on the worker's own thread.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

/// How the downloaded bytes become files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pack {
    /// The download IS the file (`keep` = one entry, its source ignored).
    File,
    /// A `.tar.gz`.
    TarGz,
    /// A `.zip` (a Python wheel).
    Zip,
}

/// One pinned download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Artifact {
    /// Unique per artifact; names the manifest and the partial download.
    pub id: &'static str,
    pub url: &'static str,
    /// The sha256 of the downloaded bytes (the publisher's digest).
    pub sha256: &'static str,
    /// The download's size in bytes (for the progress total).
    pub size: u64,
    pub pack: Pack,
    /// `(path inside the archive, file name written)`.
    pub keep: &'static [(&'static str, &'static str)],
}

/// Why an artifact is not usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bytes did not match the pin (the file is deleted).
    Mismatch { id: &'static str, actual: String },
    /// Network, disk or archive trouble.
    Io(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Mismatch { id, actual } => write!(f, "{id} failed verification (sha256 {actual})"),
            Error::Io(message) => f.write_str(message),
        }
    }
}

fn io_error(context: impl std::fmt::Display, err: impl std::fmt::Display) -> Error {
    Error::Io(format!("{context}: {err}"))
}

/// #NEW-80: set once the process is shutting down; every download stops at
/// its next read (the partial file is removed, nothing is installed).
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// #NEW-80: stop every running and future download of this process (the
/// speech worker's host is gone). One way: there is no resume.
pub fn cancel_downloads() {
    CANCELLED.store(true, Ordering::SeqCst);
}

fn cancelled() -> Result<(), Error> {
    if CANCELLED.load(Ordering::SeqCst) { Err(Error::Io("the download was cancelled".into())) } else { Ok(()) }
}

fn manifest_path(dir: &Path, artifact: &Artifact) -> PathBuf {
    dir.join(format!(".{}.sha256", artifact.id))
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The sha256 of a regular file on disk; a symlink is refused (unix:
/// `O_NOFOLLOW`, and everywhere the metadata check first).
pub fn file_sha256(path: &Path) -> io::Result<String> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::other(format!("{} is not a regular file", path.display())));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(hasher.finalize()))
}

/// The manifest: the pin on line one, then `<sha256>  <file>` per kept file.
fn read_manifest(dir: &Path, artifact: &Artifact) -> Option<Vec<(String, String)>> {
    let text = fs::read_to_string(manifest_path(dir, artifact)).ok()?;
    let mut lines = text.lines();
    if lines.next()?.trim() != artifact.sha256 {
        return None;
    }
    let files: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once("  ").map(|(sha, name)| (sha.to_string(), name.to_string())))
        .collect();
    // Only names the artifact keeps: a manifest naming anything else is not
    // ours, and `verify` must never delete a name it read from disk.
    if files.iter().any(|(_, name)| !artifact.keep.iter().any(|(_, kept)| kept == name)) {
        return None;
    }
    let complete = artifact.keep.iter().all(|(_, name)| files.iter().any(|(_, file)| file == name));
    complete.then_some(files)
}

/// Whether `artifact` is in `dir`, downloaded and verified (the manifest
/// matches the pin and names every kept file, and they exist).
pub fn is_ready(dir: &Path, artifact: &Artifact) -> bool {
    read_manifest(dir, artifact).is_some_and(|files| files.iter().all(|(_, name)| dir.join(name).is_file()))
}

/// Re-hash every kept file of `artifact` against its manifest. Run before
/// a library is loaded or a model is parsed; a mismatch (or a symlink)
/// deletes that file and the manifest (the next [`ensure`] fetches it
/// again). Only names in `artifact.keep` are ever deleted.
pub fn verify(dir: &Path, artifact: &Artifact) -> Result<(), Error> {
    let files = read_manifest(dir, artifact).ok_or_else(|| Error::Io(format!("{} is not downloaded", artifact.id)))?;
    for (sha, name) in files {
        let Some((_, kept)) = artifact.keep.iter().find(|(_, kept)| *kept == name) else {
            return Err(Error::Io(format!("{} lists an unknown file", artifact.id)));
        };
        let path = dir.join(kept);
        let actual = match file_sha256(&path) {
            Ok(actual) => actual,
            Err(err) => {
                let _ = fs::remove_file(manifest_path(dir, artifact));
                return Err(io_error(format!("read {kept}"), err));
            }
        };
        if actual != sha {
            let _ = fs::remove_file(manifest_path(dir, artifact));
            let _ = fs::remove_file(&path);
            return Err(Error::Mismatch { id: artifact.id, actual });
        }
    }
    Ok(())
}

/// Make every artifact ready in `dir`. `progress(received, total)` covers
/// all of them together (the bytes of what is already on disk count as
/// received), so the caller can show ONE download.
pub fn ensure_all(dir: &Path, artifacts: &[Artifact], mut progress: impl FnMut(u64, u64)) -> Result<(), Error> {
    let total: u64 = artifacts.iter().map(|artifact| artifact.size).sum();
    let pending: Vec<&Artifact> = artifacts.iter().filter(|artifact| !is_ready(dir, artifact)).collect();
    if pending.is_empty() {
        return Ok(());
    }
    let mut done: u64 = artifacts.iter().filter(|artifact| is_ready(dir, artifact)).map(|artifact| artifact.size).sum();
    progress(done, total);
    for artifact in pending {
        ensure(dir, artifact, |received| progress(done + received, total))?;
        done += artifact.size;
    }
    progress(total, total);
    Ok(())
}

/// Make one artifact ready in `dir`: download, verify, then unpack.
pub fn ensure(dir: &Path, artifact: &Artifact, mut progress: impl FnMut(u64)) -> Result<(), Error> {
    if is_ready(dir, artifact) {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|err| io_error(format!("create {}", dir.display()), err))?;
    let part = dir.join(format!("{}.part", artifact.id));
    let result = download(artifact, &part, &mut progress).and_then(|()| install(dir, artifact, &part));
    let _ = fs::remove_file(&part);
    result
}

fn download(artifact: &Artifact, part: &Path, progress: &mut impl FnMut(u64)) -> Result<(), Error> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| io_error("http client", err))?;
    cancelled()?;
    let mut response = client
        .get(artifact.url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| io_error(format!("download {}", artifact.url), err))?;
    let mut file = File::create(part).map_err(|err| io_error(format!("create {}", part.display()), err))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut received = 0u64;
    let mut reported = 0u64;
    loop {
        if let Err(err) = cancelled() {
            drop(file);
            let _ = fs::remove_file(part);
            return Err(err);
        }
        let read = response.read(&mut buffer).map_err(|err| io_error("download read", err))?;
        if read == 0 {
            break;
        }
        received += read as u64;
        if received > artifact.size {
            drop(file);
            let _ = fs::remove_file(part);
            return Err(Error::Io(format!("{} is larger than its pinned {} bytes", artifact.id, artifact.size)));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|err| io_error(format!("write {}", part.display()), err))?;
        if received - reported >= 8 * 1024 * 1024 {
            reported = received;
            progress(received);
        }
    }
    file.flush().map_err(|err| io_error("flush", err))?;
    progress(received);
    let actual = hex(hasher.finalize());
    if actual != artifact.sha256 {
        let _ = fs::remove_file(part);
        return Err(Error::Mismatch { id: artifact.id, actual });
    }
    Ok(())
}

/// Turn a VERIFIED download into the kept files plus the manifest.
fn install(dir: &Path, artifact: &Artifact, part: &Path) -> Result<(), Error> {
    let mut written: Vec<(String, String)> = Vec::new();
    match artifact.pack {
        Pack::File => {
            let (_, name) = artifact.keep.first().ok_or_else(|| Error::Io(format!("{} keeps nothing", artifact.id)))?;
            let target = dir.join(name);
            fs::rename(part, &target).map_err(|err| io_error(format!("move {name}"), err))?;
            written.push((artifact.sha256.to_string(), name.to_string()));
        }
        Pack::TarGz => {
            let file = File::open(part).map_err(|err| io_error(format!("open {}", part.display()), err))?;
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(io::BufReader::new(file)));
            let entries = archive.entries().map_err(|err| io_error("read archive", err))?;
            for entry in entries {
                let mut entry = entry.map_err(|err| io_error("read archive entry", err))?;
                if !entry.header().entry_type().is_file() {
                    continue;
                }
                let path = entry.path().map_err(|err| io_error("archive path", err))?;
                let Some(path) = path.to_str().map(str::to_string) else { continue };
                if let Some(name) = kept_name(artifact, &path) {
                    written.push((write_kept(dir, name, &mut entry)?, name.to_string()));
                }
            }
        }
        Pack::Zip => {
            let file = File::open(part).map_err(|err| io_error(format!("open {}", part.display()), err))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|err| io_error("read zip", err))?;
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).map_err(|err| io_error("read zip entry", err))?;
                if !entry.is_file() {
                    continue;
                }
                let Ok(path) = entry.name().map(|name| name.into_owned()) else { continue };
                if let Some(name) = kept_name(artifact, &path) {
                    written.push((write_kept(dir, name, &mut entry)?, name.to_string()));
                }
            }
        }
    }
    for (_, name) in artifact.keep {
        if !written.iter().any(|(_, file)| file == name) {
            return Err(Error::Io(format!("{} is missing {name}", artifact.id)));
        }
    }
    let mut manifest = format!("{}\n", artifact.sha256);
    for (sha, name) in &written {
        manifest.push_str(&format!("{sha}  {name}\n"));
    }
    fs::write(manifest_path(dir, artifact), manifest).map_err(|err| io_error("write manifest", err))
}

/// The written name for an archive path in `keep`: exact matches only.
fn kept_name(artifact: &Artifact, path: &str) -> Option<&'static str> {
    artifact.keep.iter().find(|(source, _)| *source == path).map(|(_, name)| *name)
}

/// Write one kept file through a temp name (a crash never leaves half a
/// library under the real name) and return its sha256.
fn write_kept(dir: &Path, name: &str, reader: &mut impl Read) -> Result<String, Error> {
    let temp = dir.join(format!("{name}.tmp"));
    let mut out = File::create(&temp).map_err(|err| io_error(format!("create {name}"), err))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|err| io_error(format!("unpack {name}"), err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read]).map_err(|err| io_error(format!("write {name}"), err))?;
    }
    out.flush().map_err(|err| io_error(format!("flush {name}"), err))?;
    drop(out);
    fs::rename(&temp, dir.join(name)).map_err(|err| io_error(format!("move {name}"), err))?;
    Ok(hex(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("crispasr-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tar_gz(entries: &[(&str, &[u8])], symlink: Option<(&str, &str)>) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        if let Some((path, target)) = symlink {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            builder.append_link(&mut header, path, target).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap()
    }

    fn sha(bytes: &[u8]) -> String {
        hex(Sha256::digest(bytes))
    }

    const LIB: Artifact = Artifact {
        id: "lib",
        url: "unused",
        sha256: "unused",
        size: 0,
        pack: Pack::TarGz,
        keep: &[("pkg/lib/libx.so.0.1", "libx.so.0"), ("pkg/LICENSE", "LICENSE")],
    };

    #[test]
    fn a_tarball_keeps_only_the_listed_regular_files_under_their_new_names() {
        let dir = temp_dir("tar");
        let part = dir.join("lib.part");
        fs::write(
            &part,
            tar_gz(
                &[("pkg/lib/libx.so.0.1", b"elf"), ("pkg/LICENSE", b"mit"), ("pkg/lib/libevil.so", b"x"), ("other/LICENSE", b"no")],
                Some(("pkg/lib/libx.so.0", "/etc/passwd")),
            ),
        )
        .unwrap();
        install(&dir, &LIB, &part).unwrap();
        let mut names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, [".lib.sha256", "LICENSE", "lib.part", "libx.so.0"]);
        assert_eq!(fs::read(dir.join("libx.so.0")).unwrap(), b"elf");
        assert!(!fs::symlink_metadata(dir.join("libx.so.0")).unwrap().file_type().is_symlink());
        assert!(is_ready(&dir, &Artifact { sha256: "unused", ..LIB }));
        verify(&dir, &LIB).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_changed_after_install_fails_verification_and_is_removed() {
        let dir = temp_dir("tamper");
        let part = dir.join("lib.part");
        fs::write(&part, tar_gz(&[("pkg/lib/libx.so.0.1", b"elf"), ("pkg/LICENSE", b"mit")], None)).unwrap();
        install(&dir, &LIB, &part).unwrap();
        fs::write(dir.join("libx.so.0"), b"patched").unwrap();
        let err = verify(&dir, &LIB).unwrap_err();
        assert_eq!(err, Error::Mismatch { id: "lib", actual: sha(b"patched") });
        assert!(!dir.join("libx.so.0").exists());
        assert!(!is_ready(&dir, &LIB));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_missing_a_kept_file_fails() {
        let dir = temp_dir("missing");
        let part = dir.join("lib.part");
        fs::write(&part, tar_gz(&[("pkg/LICENSE", b"mit")], None)).unwrap();
        let err = install(&dir, &LIB, &part).unwrap_err();
        assert!(err.to_string().contains("libx.so.0"), "{err}");
        assert!(!is_ready(&dir, &LIB));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zip_keeps_only_the_listed_entries() {
        let dir = temp_dir("zip");
        let part = dir.join("wheel.part");
        {
            let mut writer = zip::ZipWriter::new(File::create(&part).unwrap());
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("nvidia/x/lib/liby.so.12", options).unwrap();
            writer.write_all(b"elf").unwrap();
            writer.start_file("nvidia/x/lib/libz.so.12", options).unwrap();
            writer.write_all(b"other").unwrap();
            writer.finish().unwrap();
        }
        let wheel = Artifact { id: "wheel", pack: Pack::Zip, keep: &[("nvidia/x/lib/liby.so.12", "liby.so.12")], ..LIB };
        install(&dir, &wheel, &part).unwrap();
        assert_eq!(fs::read(dir.join("liby.so.12")).unwrap(), b"elf");
        assert!(!dir.join("libz.so.12").exists());
        verify(&dir, &wheel).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_symlinked_kept_file_fails_verification() {
        let dir = temp_dir("symlink");
        let part = dir.join("lib.part");
        fs::write(&part, tar_gz(&[("pkg/lib/libx.so.0.1", b"elf"), ("pkg/LICENSE", b"mit")], None)).unwrap();
        install(&dir, &LIB, &part).unwrap();
        #[cfg(unix)]
        {
            fs::write(dir.join("elsewhere"), b"elf").unwrap();
            fs::remove_file(dir.join("libx.so.0")).unwrap();
            std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join("libx.so.0")).unwrap();
            assert!(verify(&dir, &LIB).is_err(), "same bytes behind a symlink are still refused");
            assert!(!is_ready(&dir, &LIB));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_manifest_naming_a_foreign_file_is_not_trusted_or_acted_on() {
        let dir = temp_dir("foreign");
        fs::write(dir.join("libx.so.0"), b"elf").unwrap();
        fs::write(dir.join("LICENSE"), b"mit").unwrap();
        fs::write(dir.join("precious"), b"keep me").unwrap();
        fs::write(manifest_path(&dir, &LIB), format!("unused\n{}  libx.so.0\n{}  LICENSE\nbad  precious\n", sha(b"elf"), sha(b"mit")))
            .unwrap();
        assert!(!is_ready(&dir, &LIB));
        assert!(verify(&dir, &LIB).is_err());
        assert_eq!(fs::read(dir.join("precious")).unwrap(), b"keep me", "verify never deletes a name it read");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn readiness_follows_the_pin() {
        let dir = temp_dir("pin");
        let model = Artifact { id: "model", sha256: "abc", pack: Pack::File, keep: &[("", "m.gguf")], ..LIB };
        fs::write(dir.join("m.gguf"), b"m").unwrap();
        assert!(!is_ready(&dir, &model));
        fs::write(manifest_path(&dir, &model), "abc\nabc  m.gguf\n").unwrap();
        assert!(is_ready(&dir, &model));
        assert!(!is_ready(&dir, &Artifact { sha256: "other", ..model }), "a new pin downloads again");
        let _ = fs::remove_dir_all(&dir);
    }
}
