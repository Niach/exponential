//! EXP-766 — atomic replacement of a small config file: write a UNIQUE temp
//! beside the target, fsync it, chmod it to the target's mode, rename over it.
//!
//! It lives HERE, at the bottom of the dep graph, because every writer of the
//! one per-install `{data_dir}/settings.json` has to share it:
//! [`crate::device_identity`], `coding::Settings::save`,
//! `coding::launch_defaults_sync` and `cli::prefs`. A plain `fs::write`
//! truncates in place, so a concurrent reader sees a HALF file — which is how
//! a torn read used to make `device_identity` re-mint the device id.
//!
//! The temp name carries pid + nanos: a fixed `<name>.tmp` lets one process
//! rename a file a sibling is still writing.
//!
//! Mode preservation matters because the rename carries the temp's mode onto
//! the target — a umask-default temp silently widens a 0600 file, and the
//! files routed through here (`~/.claude.json`, codex `config.toml`,
//! settings.json) hold OAuth state and keys.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `<dir>/<name>.<pid>.<nanos>.<seq>.tmp` — unique per process and per call.
/// The counter is not decoration: the system clock is coarser than a thread
/// switch, so two threads DO read the same nanos and would then race on one
/// temp (one renames it away, the other's rename fails).
fn temp_path(path: &Path) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string());
    let temp = format!("{name}.{}.{nanos}.{seq}.tmp", std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(temp),
        _ => PathBuf::from(temp),
    }
}

/// Replace `path`'s contents in one rename. Creates the parent directory and
/// the file itself when missing (fresh files land at 0600 on unix). The temp
/// never survives a failure.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let temp = temp_path(path);
    let replace = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        // fsync before the rename: without it a crash can leave the renamed
        // name pointing at zero bytes.
        file.sync_all()?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path)
                .map(|meta| meta.permissions().mode() & 0o777)
                .unwrap_or(0o600);
            fs::set_permissions(&temp, fs::Permissions::from_mode(mode))?;
        }
        fs::rename(&temp, path)
    };
    match replace() {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-atomic-file-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn writes_through_a_temp_that_never_survives() {
        let dir = temp_dir("write");
        let path = dir.0.join("nested").join("settings.json");
        write_atomic(&path, "{\"a\":1}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":1}");
        write_atomic(&path, "{\"a\":2}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":2}");
        assert!(leftovers(path.parent().unwrap()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn preserves_the_targets_mode_and_creates_private_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("mode");
        let fresh = dir.0.join("fresh.json");
        write_atomic(&fresh, "{}").unwrap();
        assert_eq!(
            fs::metadata(&fresh).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let wide = dir.0.join("wide.json");
        fs::write(&wide, "{}").unwrap();
        fs::set_permissions(&wide, fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic(&wide, "{\"b\":1}").unwrap();
        assert_eq!(
            fs::metadata(&wide).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    /// The property the four settings.json writers depend on: a reader never
    /// observes a partial file, whichever writer is mid-flight.
    #[test]
    fn a_reader_never_sees_a_torn_file() {
        let dir = temp_dir("torn");
        let path = dir.0.join("settings.json");
        write_atomic(&path, "{\"n\":0}").unwrap();

        let writers: Vec<_> = (1..=4)
            .map(|writer| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for round in 0..50 {
                        let body = "x".repeat(4096);
                        let contents =
                            format!("{{\"n\":{writer},\"round\":{round},\"pad\":\"{body}\"}}");
                        write_atomic(&path, &contents).unwrap();
                    }
                })
            })
            .collect();
        for _ in 0..400 {
            if let Ok(raw) = fs::read_to_string(&path) {
                serde_json::from_str::<serde_json::Value>(&raw)
                    .unwrap_or_else(|err| panic!("torn read ({err}): {} bytes", raw.len()));
            }
        }
        for writer in writers {
            writer.join().unwrap();
        }
        assert!(leftovers(&dir.0).is_empty());
    }
}
