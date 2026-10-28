//! EXP-1375 — the worktree lifecycle's disk side, on top of NEW-5's automatic
//! cleanup ([`crate::run_cleanup::finish_worktree`] at a session's end,
//! [`crate::prune::sweep_stale_worktrees`] one day after its last activity):
//!
//! - [`prune_build_output`]: a FINISHED worktree that survives (kept dirty or
//!   ahead, a failed park) loses its regenerable build output at once —
//!   `node_modules`, cargo `target*`, `.build`, `DerivedData`, `dist`, … —
//!   and only directories git itself calls ignored and tracks nothing in;
//! - [`maintain_clone`]: `git worktree prune` plus stale `*.lock` files in the
//!   clone's git dir, run with every sweep;
//! - the KEPT list ([`record_sweep`] / [`kept_rows`]): what the sweep could
//!   not remove and why, persisted in the data dir and listed in the doctor's
//!   storage group (spec: `fixtures/device-doctor.json`, EXP-1375 rule);
//! - [`free_bytes`]: the free space of the volume holding the repos root,
//!   the doctor's `disk` row.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::git_worktree::run_git;
use crate::prune::SweepReport;
use crate::run_cleanup::{FinishOutcome, SkipReason};

/// Below this many free bytes the doctor's `disk` row warns (10 GB, decimal).
pub const LOW_DISK_BYTES: u64 = 10_000_000_000;

/// The doctor lists at most this many kept worktrees (newest first).
pub const MAX_KEPT_ROWS: usize = 12;

/// A git `*.lock` older than this has no live git behind it.
pub const STALE_LOCK_AGE: Duration = Duration::from_secs(60 * 60);

/// How deep [`prune_build_output`] looks for build output (`apps/web/node_modules`
/// = depth 3, `apps/desktop/crates/x/target` = depth 5).
const MAX_DEPTH: usize = 5;

/// The well-known regenerable directory names (plus cargo's `target-*`
/// per-lane dirs). A match is removed only when git ignores it and tracks
/// nothing inside it.
pub const BUILD_OUTPUT_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".build",
    "DerivedData",
    "dist",
    "build",
    ".turbo",
    ".next",
    ".svelte-kit",
    ".vite",
    ".parcel-cache",
    ".gradle",
];

fn is_build_output_name(name: &str) -> bool {
    BUILD_OUTPUT_DIRS.contains(&name) || name.starts_with("target-")
}

/// Candidate build-output dirs under `worktree` (relative paths), never
/// descending into a match, `.git`, or a symlink.
fn candidates(worktree: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(PathBuf::new(), 0usize)];
    while let Some((rel, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(worktree.join(&rel)) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if !kind.is_dir() || kind.is_symlink() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == ".git" {
                continue;
            }
            let child = rel.join(name);
            if is_build_output_name(name) {
                found.push(child);
            } else if depth + 1 < MAX_DEPTH {
                stack.push((child, depth + 1));
            }
        }
    }
    found.sort();
    found
}

/// Remove the regenerable build output of a finished `worktree`: every
/// [`BUILD_OUTPUT_DIRS`] match that `git check-ignore` calls ignored and in
/// which `git ls-files` finds no tracked file. Anything git cannot vouch for
/// stays. Returns the removed dirs (relative). Blocking.
pub fn prune_build_output(worktree: &Path) -> Vec<PathBuf> {
    let found = candidates(worktree);
    if found.is_empty() {
        return Vec::new();
    }
    let paths: Vec<String> = found.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let mut args: Vec<&str> = vec!["check-ignore", "--"];
    args.extend(paths.iter().map(String::as_str));
    // Exit 1 = none ignored; any failure reads the same (nothing removed).
    let ignored: Vec<String> = run_git(Some(worktree), &args, None, "git check-ignore (build output)")
        .map(|out| out.lines().map(|line| line.trim().trim_end_matches('/').to_string()).collect())
        .unwrap_or_default();
    let mut args: Vec<&str> = vec!["ls-files", "--"];
    args.extend(paths.iter().map(String::as_str));
    let Ok(tracked) = run_git(Some(worktree), &args, None, "git ls-files (build output)") else {
        return Vec::new();
    };
    let tracked: Vec<&str> = tracked.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let mut removed = Vec::new();
    for (rel, path) in found.iter().zip(paths.iter()) {
        if !ignored.iter().any(|ignored| ignored == path) {
            continue;
        }
        let prefix = format!("{path}/");
        if tracked.iter().any(|file| file.starts_with(&prefix)) {
            continue;
        }
        if std::fs::remove_dir_all(worktree.join(rel)).is_ok() {
            removed.push(rel.clone());
        }
    }
    removed
}

/// Whether a [`FinishOutcome`] left the worktree on disk for good (so its
/// build output goes): kept dirty/ahead, or a failed git step. A transient
/// skip (gate held, a live holder, already gone) is not.
pub fn left_on_disk(outcome: &FinishOutcome) -> bool {
    kept_reason(outcome).is_some()
}

/// After a session's worktree finished ([`crate::run_cleanup::finish_worktree`]):
/// a worktree that stays loses its build output. Blocking.
pub fn after_finish(worktree: &Path, outcome: &FinishOutcome) {
    if !left_on_disk(outcome) || !worktree.is_dir() {
        return;
    }
    let removed = prune_build_output(worktree);
    if !removed.is_empty() {
        log::info!("build output pruned in kept worktree {}: {removed:?}", worktree.display());
    }
}

/// `git worktree prune` plus every `*.lock` older than [`STALE_LOCK_AGE`]
/// directly in the clone's git dir and its `worktrees/<name>/` dirs (the
/// `index.lock` / `HEAD.lock` / `config.lock` a killed git leaves behind,
/// which fail every later git call in that tree). Returns the locks removed.
/// Blocking, local.
pub fn maintain_clone(clone: &Path, now: SystemTime) -> Vec<PathBuf> {
    let removed = remove_stale_locks(&clone.join(".git"), now);
    // After the locks: a stale `index.lock` never blocks a prune, but the
    // order keeps the pass's git calls on a tree that can take them.
    let _ = run_git(Some(clone), &["worktree", "prune"], None, "git worktree prune");
    removed
}

fn remove_stale_locks(git_dir: &Path, now: SystemTime) -> Vec<PathBuf> {
    if !git_dir.is_dir() {
        return Vec::new();
    }
    let mut dirs = vec![git_dir.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(git_dir.join("worktrees")) {
        dirs.extend(entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()));
    }
    let mut removed = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_lock = path.extension().is_some_and(|ext| ext == "lock")
                && entry.file_type().is_ok_and(|kind| kind.is_file());
            if !is_lock {
                continue;
            }
            let stale = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age >= STALE_LOCK_AGE);
            if stale && std::fs::remove_file(&path).is_ok() {
                removed.push(path);
            }
        }
    }
    removed
}

/// Why the automatic cleanup kept a worktree (the doctor row's detail).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeptReason {
    /// Tracked changes and no `origin` to park them on.
    Uncommitted,
    /// Commits the base lacks, no `origin`.
    Unpushed,
    /// The WIP commit, the leftover push or the removal failed.
    Failed,
}

impl KeptReason {
    /// The fixture's `copy.kept*` line.
    pub fn detail(self) -> &'static str {
        match self {
            KeptReason::Uncommitted => "Uncommitted changes",
            KeptReason::Unpushed => "Unpushed commits",
            KeptReason::Failed => "Cleanup failed",
        }
    }
}

/// The reason a finish outcome KEPT the worktree; `None` = removed, or a
/// transient skip the next pass retries.
pub fn kept_reason(outcome: &FinishOutcome) -> Option<KeptReason> {
    match outcome {
        FinishOutcome::Removed { .. } => None,
        FinishOutcome::KeptDirty(_) => Some(KeptReason::Uncommitted),
        FinishOutcome::KeptAhead(_) => Some(KeptReason::Unpushed),
        FinishOutcome::Skipped(SkipReason::GitFailed(_)) => Some(KeptReason::Failed),
        FinishOutcome::Skipped(_) => None,
    }
}

/// One persisted kept worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptWorktree {
    pub clone: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub reason: KeptReason,
    /// Unix seconds of the pass that kept it.
    pub at: u64,
}

const KEPT_FILE: &str = "kept-worktrees.json";

fn kept_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEPT_FILE)
}

/// Every persisted kept worktree (unfiltered; unreadable = none).
pub fn load_kept(data_dir: &Path) -> Vec<KeptWorktree> {
    std::fs::read_to_string(kept_path(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn store_kept(data_dir: &Path, kept: &[KeptWorktree]) {
    let Ok(text) = serde_json::to_string_pretty(kept) else { return };
    let path = kept_path(data_dir);
    let tmp = data_dir.join(format!("{KEPT_FILE}.{}.tmp", std::process::id()));
    if std::fs::create_dir_all(data_dir).is_ok()
        && std::fs::write(&tmp, text).is_ok()
        && std::fs::rename(&tmp, &path).is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Replace `clone`'s entries with what this sweep pass kept (a worktree it
/// did not consider — live again, young after a resume — drops off).
pub fn record_sweep(data_dir: &Path, clone: &Path, report: &SweepReport, now: SystemTime) {
    let at = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let mut kept: Vec<KeptWorktree> =
        load_kept(data_dir).into_iter().filter(|entry| entry.clone != clone).collect();
    let before = kept.len();
    kept.extend(report.finished.iter().filter_map(|swept| {
        Some(KeptWorktree {
            clone: clone.to_path_buf(),
            path: swept.path.clone(),
            branch: swept.branch.clone(),
            session_id: swept.session_id.clone(),
            reason: kept_reason(&swept.outcome)?,
            at,
        })
    }));
    if kept.len() != before || std::fs::metadata(kept_path(data_dir)).is_ok() {
        store_kept(data_dir, &kept);
    }
}

/// One kept-worktree row of the doctor: its label (the worktree's directory
/// name) and why it stays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptRow {
    pub label: String,
    pub reason: KeptReason,
}

/// The doctor's kept rows: persisted entries whose dir still exists, newest
/// first, at most [`MAX_KEPT_ROWS`].
pub fn kept_rows(data_dir: &Path) -> Vec<KeptRow> {
    let mut kept: Vec<KeptWorktree> =
        load_kept(data_dir).into_iter().filter(|entry| entry.path.is_dir()).collect();
    kept.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.path.cmp(&b.path)));
    kept.into_iter()
        .take(MAX_KEPT_ROWS)
        .map(|entry| KeptRow {
            label: entry
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.branch.clone()),
            reason: entry.reason,
        })
        .collect()
}

/// The free bytes available to this user on the volume holding `path` (its
/// nearest existing ancestor). `None` = unreadable (and on Windows for now).
pub fn free_bytes(path: &Path) -> Option<u64> {
    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    free_bytes_at(probe)
}

#[cfg(unix)]
fn free_bytes_at(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is a valid NUL-terminated string and `stat` a properly
    // sized out-parameter that statvfs fills on success.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)]
    Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

#[cfg(not(unix))]
fn free_bytes_at(_path: &Path) -> Option<u64> {
    None
}

/// The `disk` row's size: decimal GB, one decimal below 10 GB (`6.2 GB`),
/// whole above (`214 GB`), always rounded DOWN.
pub fn format_free(bytes: u64) -> String {
    let tenths = bytes / 100_000_000;
    if tenths < 100 {
        format!("{}.{} GB", tenths / 10, tenths % 10)
    } else {
        format!("{} GB", tenths / 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prune::SweptWorktree;
    use crate::prune::DirtyState;
    use std::fs;
    use std::process::Command;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("exp-wt-lifecycle-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn repo(dir: &Path) -> PathBuf {
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--quiet", "-b", "main"]);
        fs::write(repo.join(".gitignore"), "node_modules/\ntarget*/\ndist/\n").unwrap();
        // A TRACKED dir named like build output, and one inside dist.
        fs::create_dir_all(repo.join("build")).unwrap();
        fs::write(repo.join("build/script.sh"), "echo\n").unwrap();
        fs::create_dir_all(repo.join("dist")).unwrap();
        fs::write(repo.join("dist/keep.txt"), "tracked\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["add", "-f", "dist/keep.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "seed"]);
        repo
    }

    #[test]
    fn prunes_only_ignored_untracked_build_output() {
        let dir = temp_dir("prune");
        let repo = repo(&dir.0);
        for rel in ["node_modules/a", "apps/web/node_modules/b", "target-lane/debug", "src/target"] {
            fs::create_dir_all(repo.join(rel)).unwrap();
            fs::write(repo.join(rel).join("f"), "x").unwrap();
        }
        // `.build` is not ignored here: it stays (it could be real work).
        fs::create_dir_all(repo.join(".build")).unwrap();
        fs::write(repo.join(".build/f"), "x").unwrap();
        let removed = prune_build_output(&repo);
        let removed: Vec<String> =
            removed.iter().map(|path| path.to_string_lossy().into_owned()).collect();
        assert_eq!(
            removed,
            ["apps/web/node_modules", "node_modules", "src/target", "target-lane"],
        );
        assert!(!repo.join("node_modules").exists());
        assert!(!repo.join("apps/web/node_modules").exists());
        // Tracked `build/`, `dist/` with a force-added file, unignored `.build`.
        assert!(repo.join("build/script.sh").exists());
        assert!(repo.join("dist/keep.txt").exists());
        assert!(repo.join(".build/f").exists());
        assert!(repo.join("apps/web").is_dir());
    }

    #[test]
    fn a_non_repo_dir_loses_nothing() {
        let dir = temp_dir("norepo");
        fs::create_dir_all(dir.0.join("node_modules")).unwrap();
        assert!(prune_build_output(&dir.0).is_empty());
        assert!(dir.0.join("node_modules").exists());
    }

    #[test]
    fn maintain_clone_prunes_worktrees_and_stale_locks() {
        let dir = temp_dir("maintain");
        let repo = repo(&dir.0);
        let gone = dir.0.join("gone");
        git(&repo, &["worktree", "add", "--quiet", "-b", "exp/gone", gone.to_str().unwrap()]);
        fs::remove_dir_all(&gone).unwrap();
        let git_dir = repo.join(".git");
        let stale = git_dir.join("index.lock");
        let fresh = git_dir.join("HEAD.lock");
        fs::write(&stale, "").unwrap();
        fs::write(&fresh, "").unwrap();
        let old = SystemTime::now() - 2 * STALE_LOCK_AGE;
        fs::OpenOptions::new().write(true).open(&stale).unwrap().set_modified(old).unwrap();
        let removed = maintain_clone(&repo, SystemTime::now());
        assert_eq!(removed, vec![stale.clone()]);
        assert!(!stale.exists());
        assert!(fresh.exists(), "a young lock may belong to a live git");
        let list = crate::git_worktree::list_worktrees(&repo).unwrap();
        assert_eq!(list.len(), 1, "the missing worktree was pruned: {list:?}");
    }

    #[test]
    fn kept_reasons_follow_the_finish_outcome() {
        assert_eq!(kept_reason(&FinishOutcome::Removed { leftover: None }), None);
        assert_eq!(kept_reason(&FinishOutcome::Removed { leftover: Some("x".into()) }), None);
        assert_eq!(
            kept_reason(&FinishOutcome::KeptDirty(DirtyState::TrackedChanges)),
            Some(KeptReason::Uncommitted)
        );
        assert_eq!(kept_reason(&FinishOutcome::KeptAhead(2)), Some(KeptReason::Unpushed));
        assert_eq!(
            kept_reason(&FinishOutcome::Skipped(SkipReason::GitFailed("push".into()))),
            Some(KeptReason::Failed)
        );
        assert_eq!(kept_reason(&FinishOutcome::Skipped(SkipReason::GateHeld)), None);
        assert_eq!(kept_reason(&FinishOutcome::Skipped(SkipReason::Missing)), None);
    }

    #[test]
    fn the_kept_list_is_per_clone_newest_first_and_skips_gone_dirs() {
        let dir = temp_dir("kept");
        let data = dir.0.join("data");
        let a = dir.0.join("a.worktrees/exp-A");
        let b = dir.0.join("b.worktrees/exp-B");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let swept = |path: &Path, outcome| SweptWorktree {
            path: path.to_path_buf(),
            branch: "exp/x".into(),
            session_id: Some("s".into()),
            outcome,
        };
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let report = |path: &Path, outcome| SweepReport { finished: vec![swept(path, outcome)] };
        record_sweep(&data, Path::new("/a"), &report(&a, FinishOutcome::KeptAhead(1)), t0);
        record_sweep(
            &data,
            Path::new("/b"),
            &report(&b, FinishOutcome::KeptDirty(DirtyState::TrackedChanges)),
            t0 + Duration::from_secs(5),
        );
        assert_eq!(
            kept_rows(&data),
            vec![
                KeptRow { label: "exp-B".into(), reason: KeptReason::Uncommitted },
                KeptRow { label: "exp-A".into(), reason: KeptReason::Unpushed },
            ]
        );
        // A later pass over `a` that removed it drops it; `b`'s dir vanishing
        // by hand hides it too.
        record_sweep(&data, Path::new("/a"), &report(&a, FinishOutcome::Removed { leftover: None }), t0);
        fs::remove_dir_all(&b).unwrap();
        assert!(kept_rows(&data).is_empty());
    }

    #[test]
    fn free_space_reads_and_formats() {
        let dir = temp_dir("free");
        assert!(free_bytes(&dir.0.join("not/yet/there")).is_some_and(|bytes| bytes > 0));
        assert_eq!(format_free(6_249_999_999), "6.2 GB");
        assert_eq!(format_free(9_999_999_999), "9.9 GB");
        assert_eq!(format_free(10_000_000_000), "10 GB");
        assert_eq!(format_free(214_900_000_000), "214 GB");
        assert_eq!(format_free(0), "0.0 GB");
    }

    #[test]
    fn after_finish_prunes_only_a_kept_worktree() {
        let dir = temp_dir("after");
        let repo = repo(&dir.0);
        fs::create_dir_all(repo.join("node_modules/x")).unwrap();
        after_finish(&repo, &FinishOutcome::Skipped(SkipReason::GateHeld));
        assert!(repo.join("node_modules").exists());
        after_finish(&repo, &FinishOutcome::KeptAhead(1));
        assert!(!repo.join("node_modules").exists());
    }
}
