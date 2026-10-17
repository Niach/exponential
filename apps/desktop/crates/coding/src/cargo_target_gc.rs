//! Bounded shared cargo target: evicts stale build units from
//! `<clone>.cargo-target` (EXP-76's ONE target dir per repo).
//!
//! Cargo never evicts: every feature-unification variant, every bumped dep
//! and every example a session ever built stays in `deps/` forever. On
//! 2026-09-25 this repo's shared target reached 38 GB and helped fill the
//! disk mid-release. Landed worktrees (and any private `target-*` inside
//! them) are already reclaimed by [`crate::prune`]; this sweep bounds the one
//! cache the prune never touches.
//!
//! The rule: only when the whole dir is over [`BUDGET_BYTES`], and per
//! profile dir (`debug`, `release`, `<triple>/release`, …) only while WE hold
//! its `.cargo-lock` (the flock cargo itself takes; a running build skips the
//! profile, a build starting mid-sweep waits for us), delete
//! `deps/`, `build/` and `.fingerprint/` entries last written more than
//! [`MAX_AGE`] ago plus all of `examples/` and `incremental/`. Top-level
//! artifacts (the binaries shots and the IDE run) are never touched. Cargo
//! treats a missing output or fingerprint as stale and rebuilds that unit, so
//! the worst case is a partial cold build.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::git_worktree::shared_cargo_target_dir;

/// Sweep only once the shared target holds more than this.
pub const BUDGET_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Units last compiled longer ago than this are evicted.
pub const MAX_AGE: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Per-profile subdirs whose stale entries are evicted.
const AGED_DIRS: [&str; 3] = ["deps", "build", ".fingerprint"];

/// Per-profile subdirs dropped whole (rebuilt on demand).
const DROPPED_DIRS: [&str; 2] = ["examples", "incremental"];

/// Outcome of one sweep.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    /// Total size before the sweep (0 when the dir is missing/not a target).
    pub bytes_before: u64,
    /// Bytes deleted.
    pub bytes_freed: u64,
    /// Profile dirs skipped because a build held their lock.
    pub skipped_locked: Vec<PathBuf>,
}

/// Sweep the clone's shared cargo target with the default budget and age.
pub fn sweep(clone: &Path) -> SweepReport {
    sweep_with(
        &shared_cargo_target_dir(clone),
        BUDGET_BYTES,
        MAX_AGE,
        SystemTime::now(),
    )
}

/// [`sweep`] over an explicit target dir, budget, age and clock.
pub fn sweep_with(target: &Path, budget: u64, max_age: Duration, now: SystemTime) -> SweepReport {
    let mut report = SweepReport::default();
    // Only ever a dir cargo made: it writes CACHEDIR.TAG into every target.
    if !target.join("CACHEDIR.TAG").is_file() {
        return report;
    }
    report.bytes_before = tree_size(target);
    if report.bytes_before <= budget {
        return report;
    }
    let cutoff = now.checked_sub(max_age).unwrap_or(SystemTime::UNIX_EPOCH);
    for profile in profile_dirs(target) {
        let Some(_lock) = try_lock_profile(&profile) else {
            report.skipped_locked.push(profile);
            continue;
        };
        for name in AGED_DIRS {
            let Ok(entries) = fs::read_dir(profile.join(name)) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if newest_mtime(&path).is_some_and(|mtime| mtime < cutoff) {
                    report.bytes_freed += remove(&path);
                }
            }
        }
        for name in DROPPED_DIRS {
            let path = profile.join(name);
            if path.symlink_metadata().is_ok() {
                report.bytes_freed += remove(&path);
            }
        }
    }
    report
}

/// Profile dirs = dirs up to depth 2 holding a `.cargo-lock`.
fn profile_dirs(target: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut level = vec![target.to_path_buf()];
    for _ in 0..2 {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let path = entry.path();
                if path.join(".cargo-lock").is_file() {
                    out.push(path);
                } else {
                    next.push(path);
                }
            }
        }
        level = next;
    }
    out.sort();
    out
}

/// The profile's cargo build lock, held until dropped; `None` = a build
/// holds it (or it can't be opened).
fn try_lock_profile(profile: &Path) -> Option<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(profile.join(".cargo-lock"))
        .ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Newest mtime of a path and (for a real dir) everything beneath it;
/// symlinks count as themselves, never followed.
fn newest_mtime(path: &Path) -> Option<SystemTime> {
    let meta = path.symlink_metadata().ok()?;
    let mut newest = meta.modified().ok()?;
    if meta.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(mtime) = newest_mtime(&entry.path()) {
                    newest = newest.max(mtime);
                }
            }
        }
    }
    Some(newest)
}

/// Total bytes of files under `path` (symlinks not followed).
fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path)
        .map(|entries| entries.flatten().map(|entry| tree_size(&entry.path())).sum())
        .unwrap_or(0)
}

/// Delete a file, symlink or dir tree; returns the bytes it held. Failures
/// are logged and count as nothing freed.
fn remove(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    let size = tree_size(path);
    let result = if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => size,
        Err(err) => {
            log::warn!("[coding] cargo-target gc: couldn't remove {}: {err}", path.display());
            0
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
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-coding-gc-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn age(path: &Path, days: u64) {
        let when = SystemTime::now() - DAY * days as u32;
        File::open(path).unwrap().set_modified(when).unwrap();
    }

    /// A target with a `debug` profile: old + fresh deps, an old fingerprint
    /// dir, examples, and a top-level binary.
    fn seed(root: &Path) -> PathBuf {
        let target = root.join("repo.cargo-target");
        write(&target.join("CACHEDIR.TAG"), 10);
        let debug = target.join("debug");
        write(&debug.join(".cargo-lock"), 0);
        write(&debug.join("exp-desktop"), 100);
        write(&debug.join("deps/libold-aaaa.rlib"), 100);
        write(&debug.join("deps/libfresh-bbbb.rlib"), 100);
        write(&debug.join(".fingerprint/old-aaaa/lib-old"), 10);
        write(&debug.join("examples/sample"), 100);
        age(&debug.join("exp-desktop"), 30);
        age(&debug.join("deps/libold-aaaa.rlib"), 10);
        age(&debug.join(".fingerprint/old-aaaa/lib-old"), 10);
        age(&debug.join(".fingerprint/old-aaaa"), 10);
        target
    }

    #[test]
    fn not_a_cargo_target_is_untouched() {
        let tmp = temp_dir("untagged");
        let target = seed(&tmp.0);
        fs::remove_file(target.join("CACHEDIR.TAG")).unwrap();
        let report = sweep_with(&target, 0, MAX_AGE, SystemTime::now());
        assert_eq!(report, SweepReport::default());
        assert!(target.join("debug/deps/libold-aaaa.rlib").exists());
    }

    #[test]
    fn under_budget_is_untouched() {
        let tmp = temp_dir("under");
        let target = seed(&tmp.0);
        let report = sweep_with(&target, 1 << 20, MAX_AGE, SystemTime::now());
        assert_eq!(report.bytes_freed, 0);
        assert!(target.join("debug/deps/libold-aaaa.rlib").exists());
        assert!(target.join("debug/examples/sample").exists());
    }

    #[test]
    fn over_budget_evicts_stale_units_only() {
        let tmp = temp_dir("over");
        let target = seed(&tmp.0);
        let report = sweep_with(&target, 0, MAX_AGE, SystemTime::now());
        assert_eq!(report.bytes_freed, 100 + 10 + 100);
        assert!(!target.join("debug/deps/libold-aaaa.rlib").exists());
        assert!(!target.join("debug/.fingerprint/old-aaaa").exists());
        assert!(!target.join("debug/examples").exists());
        assert!(target.join("debug/deps/libfresh-bbbb.rlib").exists());
        assert!(target.join("debug/exp-desktop").exists());
    }

    #[test]
    fn a_locked_profile_is_skipped() {
        let tmp = temp_dir("locked");
        let target = seed(&tmp.0);
        let held = File::open(target.join("debug/.cargo-lock")).unwrap();
        held.lock().unwrap();
        let report = sweep_with(&target, 0, MAX_AGE, SystemTime::now());
        assert_eq!(report.bytes_freed, 0);
        assert_eq!(report.skipped_locked, vec![target.join("debug")]);
        assert!(target.join("debug/deps/libold-aaaa.rlib").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let tmp = temp_dir("symlink");
        let target = seed(&tmp.0);
        let outside = tmp.0.join("outside/keep.txt");
        write(&outside, 10);
        std::os::unix::fs::symlink(outside.parent().unwrap(), target.join("debug/examples/link"))
            .unwrap();
        sweep_with(&target, 0, MAX_AGE, SystemTime::now());
        assert!(outside.exists());
        assert!(!target.join("debug/examples").exists());
    }

    /// Manual check against a real (copied) target:
    /// `EXP_GC_TARGET=/path cargo test -p coding gc_live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn gc_live() {
        let target = PathBuf::from(std::env::var("EXP_GC_TARGET").unwrap());
        let budget = std::env::var("EXP_GC_BUDGET")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(BUDGET_BYTES);
        println!("{:?}", sweep_with(&target, budget, MAX_AGE, SystemTime::now()));
        println!("after: {}", tree_size(&target));
    }
}
