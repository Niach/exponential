//! EXP-757 — reclaim the scratch dirs of repo-less runs.
//!
//! A repo-less run (the "Create action" creator, a repo-less chat or team
//! action) spawns in `<data_dir>/actions/<action>/<run>/`, a directory
//! holding nothing but launcher sidecars: the MCP config, the `.exp-agents`
//! marker, a pi bridge extension. Every agent keeps its transcript
//! ELSEWHERE — claude under `~/.claude/projects/<munged cwd>/` keyed by the
//! path STRING, codex in its own rollouts, pi in `<data_dir>/pi-sessions/` —
//! so the directory is disposable: it goes when the run ends and a resume
//! simply re-creates it at the recorded path (`launcher::prepare_resume_run`).
//! The run RECORD (`run_registry`) is what keeps a run resumable, for its
//! own TTL.
//!
//! Two entry points, shared by the desktop app and the CLI daemon:
//!
//! - [`reclaim`] — one run that just ended (the host knows its cwd).
//! - [`sweep`] — the startup pass over everything under the root: crashes,
//!   launches that failed after the dir was created, dirs an older build
//!   left behind, one-shot `exponential run`s. Live runs are protected by
//!   the ids the host passes (its own in-process set plus the pid-owned
//!   entries of the shared session registry, mapped to cwds through the run
//!   registry — REV-20: desktop and daemon share one data dir, and neither
//!   may delete the other's live chat) and by a [`GRACE`] age floor covering
//!   the prepare→register window.
//!
//! Both also drop the claude trust entries the launcher seeded for the dir
//! (`claude_trust`), which is the other half of the leak: `~/.claude.json`
//! grew one `projects` key per click. codex keys its trust on the ACTION dir
//! (one per action, not per run) and is left alone.
//!
//! Best-effort throughout: a failure to remove is logged, never surfaced.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Dirs (and legacy sidecar files) younger than this are never swept — the
/// same floor the worktree prune uses for launches it cannot see.
pub const GRACE: Duration = crate::prune::LAUNCH_GRACE;

/// Files pre-EXP-637 builds wrote DIRECTLY into `<root>/<action>/` (one
/// scratch per action, not per run). They keep the action dir non-empty
/// forever, so the sweep clears them once they are older than [`GRACE`].
const LEGACY_SIDECARS: &[&str] = &[
    crate::mcp_json::MCP_JSON_FILE,
    crate::mcp_json::LEGACY_MCP_JSON_FILE,
    crate::worktree_agents::AGENTS_FILE,
    crate::pi_bridge::PI_BRIDGE_FILE,
];

/// `<data_dir>/actions` — where `launcher::prepare_action` puts every
/// repo-less run.
pub fn scratch_root(data_dir: &Path) -> PathBuf {
    data_dir.join("actions")
}

/// Whether `cwd` is a per-RUN scratch dir: exactly `<root>/<action>/<run>`,
/// never the root, an action dir or anything outside the tree. Guards every
/// removal — a host that hands over a worktree or trunk clone by mistake must
/// not lose it.
pub fn is_scratch_dir(data_dir: &Path, cwd: &Path) -> bool {
    let root = scratch_root(data_dir);
    let Some(action_dir) = cwd.parent() else {
        return false;
    };
    let Some(parent_root) = action_dir.parent() else {
        return false;
    };
    parent_root == root
        && is_plain_segment(cwd.file_name())
        && is_plain_segment(action_dir.file_name())
}

fn is_plain_segment(name: Option<&std::ffi::OsStr>) -> bool {
    name.is_some_and(|name| !name.is_empty() && name != "." && name != "..")
}

/// Reclaim ONE ended run's scratch dir and its agent trust entries. Returns
/// whether the directory was removed (`false` when it was already gone, when
/// `cwd` is not a scratch dir at all, or when a resume re-entered it — logged,
/// never fatal). The run record is deliberately left alone: it is what keeps
/// the run resumable.
///
/// `requested_at` is the moment the run was seen to END, not the moment this
/// runs: both hosts queue the reclaim (the desktop onto its background
/// executor, the daemon behind the git work of its 1Hz tick), and a resume of
/// the just-ended run lands in that window — `prepare_resume_run` re-creates
/// the SAME recorded cwd and [`touch`]es it, so anything newer than the
/// request means the dir is live again and must not be deleted under its
/// fresh `.exp-mcp.json`.
pub fn reclaim(data_dir: &Path, cwd: &Path, requested_at: SystemTime) -> bool {
    if !is_scratch_dir(data_dir, cwd) {
        log::warn!("scratch reclaim: {} is not a scratch dir; kept", cwd.display());
        return false;
    }
    if newest_mtime(cwd).is_some_and(|newest| newest > requested_at) {
        log::info!(
            "scratch reclaim: {} was re-entered after the run ended; kept",
            cwd.display()
        );
        return false;
    }
    // The trust keys, raw AND canonical, resolved BEFORE the dirs go —
    // canonicalize needs the path to exist. EXP-758: claude keys trust by the
    // spawn cwd (the RUN dir), codex by the stable per-action root one level
    // up (`launcher::prepare_action`), so the two configs take different keys
    // and the codex block only goes when its dir does.
    let trust_keys = trust_keys_for(cwd);
    let action_dir = cwd.parent().map(Path::to_path_buf);
    let codex_trust_keys = action_dir.as_deref().map(trust_keys_for).unwrap_or_default();
    let removed = match std::fs::remove_dir_all(cwd) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            log::warn!("scratch reclaim: remove {}: {err}", cwd.display());
            false
        }
    };
    if let Some(action_dir) = &action_dir {
        // Empty action dirs go too; a non-empty one (a sibling run) stays —
        // and so does its codex trust entry, which that run still needs.
        if std::fs::remove_dir(action_dir).is_ok() {
            crate::codex_trust::forget(&codex_trust_keys);
        }
    }
    crate::claude_trust::forget(&trust_keys);
    removed
}

fn trust_keys_for(cwd: &Path) -> Vec<PathBuf> {
    let mut keys = vec![cwd.to_path_buf()];
    if let Ok(canonical) = std::fs::canonicalize(cwd) {
        if canonical != cwd {
            keys.push(canonical);
        }
    }
    keys
}

/// What one [`sweep`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Run dirs removed (legacy sidecar files are not listed).
    pub removed: Vec<PathBuf>,
    /// Run dirs kept because a live session owns them.
    pub kept_live: usize,
    /// Run dirs kept because they are younger than [`GRACE`].
    pub kept_young: usize,
    /// Claude `projects` entries dropped for dirs that no longer exist.
    pub trust_dropped: usize,
}

impl SweepReport {
    pub fn is_noop(&self) -> bool {
        self.removed.is_empty() && self.trust_dropped == 0
    }
}

/// The startup pass: every run dir under the root that no live session owns
/// and that is older than [`GRACE`] is removed, emptied action dirs go with
/// them, and every claude trust entry under the root whose dir is gone is
/// dropped. `live_session_ids` are the sessions the host knows to be running
/// — its own, plus the ones the shared session registry attributes to a live
/// sibling process; their recorded cwds are the keep set.
pub fn sweep(data_dir: &Path, live_session_ids: &[String]) -> SweepReport {
    let root = scratch_root(data_dir);
    let mut report = SweepReport::default();
    let live = live_cwds(data_dir, live_session_ids);
    let now = SystemTime::now();
    let Ok(actions) = std::fs::read_dir(&root) else {
        return report;
    };
    for action in actions.flatten() {
        let action_dir = action.path();
        if !action_dir.is_dir() {
            continue;
        }
        if let Ok(runs) = std::fs::read_dir(&action_dir) {
            for run in runs.flatten() {
                let run_dir = run.path();
                if run_dir.is_dir() {
                    if is_live(&live, &run_dir) {
                        report.kept_live += 1;
                        continue;
                    }
                    if is_young(&run_dir, now) {
                        report.kept_young += 1;
                        continue;
                    }
                    match std::fs::remove_dir_all(&run_dir) {
                        Ok(()) => report.removed.push(run_dir),
                        Err(err) => {
                            log::warn!("scratch sweep: remove {}: {err}", run_dir.display())
                        }
                    }
                } else if is_legacy_sidecar(&run_dir) && !is_young(&run_dir, now) {
                    let _ = std::fs::remove_file(&run_dir);
                }
            }
        }
        let _ = std::fs::remove_dir(&action_dir);
    }
    report.trust_dropped = crate::claude_trust::forget_missing_under(&root);
    report
}

/// The recorded cwds of the live sessions, raw and canonical, so a keep
/// check never depends on how either side spelled the path.
fn live_cwds(data_dir: &Path, live_session_ids: &[String]) -> HashSet<PathBuf> {
    let mut live = HashSet::new();
    for cwd in crate::run_registry::cwds_for(data_dir, live_session_ids) {
        if let Ok(canonical) = std::fs::canonicalize(&cwd) {
            live.insert(canonical);
        }
        live.insert(cwd);
    }
    live
}

fn is_live(live: &HashSet<PathBuf>, run_dir: &Path) -> bool {
    if live.contains(run_dir) {
        return true;
    }
    std::fs::canonicalize(run_dir).is_ok_and(|canonical| live.contains(&canonical))
}

fn is_legacy_sidecar(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| LEGACY_SIDECARS.contains(&name))
}

/// The newest of the entry's own mtime and (for a directory) its direct
/// children's: a directory's mtime moves only on entry create/delete, not on
/// a child's content write, and which sidecar a launch writes depends on the
/// agent and transport.
fn newest_mtime(path: &Path) -> Option<SystemTime> {
    let mut newest = std::fs::metadata(path).ok()?.modified().ok()?;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) {
                newest = newest.max(modified);
            }
        }
    }
    Some(newest)
}

/// Younger than [`GRACE`] — or unreadable, which keeps (never delete on a
/// guess).
fn is_young(path: &Path, now: SystemTime) -> bool {
    match newest_mtime(path) {
        Some(modified) => now
            .duration_since(modified)
            .map_or(true, |age| age < GRACE),
        None => true,
    }
}

/// Best-effort bump of a dir's mtime — the resume path calls it on the
/// scratch dir it re-enters, so a dir that survived a crash reads as young
/// to a sweep racing the relaunch.
pub(crate) fn touch(dir: &Path) {
    let now = SystemTime::now();
    let _ = std::fs::File::open(dir).and_then(|file| file.set_modified(now));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn age(path: &Path, by: Duration) {
        let then = SystemTime::now() - by;
        let file = std::fs::File::open(path).unwrap();
        file.set_modified(then).unwrap();
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap().flatten() {
                std::fs::File::open(entry.path())
                    .unwrap()
                    .set_modified(then)
                    .unwrap();
            }
        }
    }

    fn scratch(data_dir: &Path, action: &str, run: &str) -> PathBuf {
        let dir = scratch_root(data_dir).join(action).join(run);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(crate::mcp_json::MCP_JSON_FILE), "{}").unwrap();
        dir
    }

    const OLD: Duration = Duration::from_secs(60 * 60);

    #[test]
    fn is_scratch_dir_accepts_exactly_two_segments_under_the_root() {
        let data = PathBuf::from("/data");
        assert!(is_scratch_dir(&data, Path::new("/data/actions/builtin_chat/1a2b3c4d")));
        assert!(!is_scratch_dir(&data, Path::new("/data/actions")));
        assert!(!is_scratch_dir(&data, Path::new("/data/actions/builtin_chat")));
        assert!(!is_scratch_dir(&data, Path::new("/data/actions/a/b/c")));
        assert!(!is_scratch_dir(&data, Path::new("/repos/owner/name.worktrees/exp-EXP-1")));
        assert!(!is_scratch_dir(&data, Path::new("/data/actions/a/..")));
        assert!(!is_scratch_dir(&data, Path::new("/other/actions/a/b")));
    }

    #[test]
    fn reclaim_removes_the_run_dir_and_its_emptied_action_dir() {
        let dir = temp_dir("scratch-reclaim");
        let run = scratch(&dir.0, "builtin_chat", "1a2b3c4d");
        assert!(reclaim(&dir.0, &run, SystemTime::now()));
        assert!(!run.exists());
        assert!(!run.parent().unwrap().exists(), "the empty action dir goes too");
        // Already gone: a no-op, not an error.
        assert!(!reclaim(&dir.0, &run, SystemTime::now()));
    }

    #[test]
    fn reclaim_keeps_an_action_dir_with_a_sibling_run() {
        let dir = temp_dir("scratch-reclaim-sibling");
        let run = scratch(&dir.0, "builtin_chat", "1a2b3c4d");
        let sibling = scratch(&dir.0, "builtin_chat", "ffffffff");
        assert!(reclaim(&dir.0, &run, SystemTime::now()));
        assert!(!run.exists());
        assert!(sibling.join(crate::mcp_json::MCP_JSON_FILE).exists());
    }

    /// The resume race: the run ended, the reclaim was queued, and a resume
    /// re-created the recorded cwd (and its fresh `.exp-mcp.json`) before the
    /// queued reclaim ran. The dir is live again — deleting it would pull the
    /// MCP config out from under the relaunch.
    #[test]
    fn reclaim_keeps_a_dir_a_resume_re_entered() {
        let dir = temp_dir("scratch-reclaim-resumed");
        let run = scratch(&dir.0, "builtin_chat", "1a2b3c4d");
        let requested_at = SystemTime::now() - Duration::from_secs(5);
        assert!(!reclaim(&dir.0, &run, requested_at));
        assert!(run.join(crate::mcp_json::MCP_JSON_FILE).exists());
        // Untouched since the run ended, it goes on the next pass.
        age(&run, OLD);
        assert!(reclaim(&dir.0, &run, requested_at));
        assert!(!run.exists());
    }

    #[test]
    fn reclaim_refuses_anything_that_is_not_a_run_dir() {
        let dir = temp_dir("scratch-reclaim-refuse");
        let run = scratch(&dir.0, "builtin_chat", "1a2b3c4d");
        let action_dir = run.parent().unwrap().to_path_buf();
        let foreign = dir.0.join("worktree");
        std::fs::create_dir_all(&foreign).unwrap();
        assert!(!reclaim(&dir.0, &action_dir, SystemTime::now()));
        assert!(!reclaim(&dir.0, &scratch_root(&dir.0), SystemTime::now()));
        assert!(!reclaim(&dir.0, &foreign, SystemTime::now()));
        assert!(run.exists());
        assert!(foreign.exists());
    }

    #[test]
    fn sweep_removes_old_orphans_and_keeps_young_ones() {
        let dir = temp_dir("scratch-sweep");
        let old = scratch(&dir.0, "builtin_chat", "aaaaaaaa");
        age(&old, OLD);
        let young = scratch(&dir.0, "builtin_chat", "bbbbbbbb");
        let report = sweep(&dir.0, &[]);
        assert_eq!(report.removed, vec![old.clone()]);
        assert_eq!(report.kept_young, 1);
        assert!(!old.exists());
        assert!(young.exists());
    }

    /// A dir whose files were touched after the dir itself: the newest
    /// child's mtime is the age, not the directory's.
    #[test]
    fn sweep_ages_a_dir_by_its_newest_child() {
        let dir = temp_dir("scratch-sweep-child");
        let run = scratch(&dir.0, "builtin_chat", "aaaaaaaa");
        age(&run, OLD);
        std::fs::write(run.join(crate::pi_bridge::PI_BRIDGE_FILE), "fresh").unwrap();
        std::fs::File::open(&run)
            .unwrap()
            .set_modified(SystemTime::now() - OLD)
            .unwrap();
        let report = sweep(&dir.0, &[]);
        assert!(report.removed.is_empty(), "{report:?}");
        assert_eq!(report.kept_young, 1);
        assert!(run.exists());
    }

    #[test]
    fn sweep_keeps_the_cwd_of_a_live_session() {
        let dir = temp_dir("scratch-sweep-live");
        let live = scratch(&dir.0, "builtin_chat", "aaaaaaaa");
        age(&live, OLD);
        let ended = scratch(&dir.0, "builtin_chat", "bbbbbbbb");
        age(&ended, OLD);
        let mut live_record = crate::run_registry::sample_record("sess-live");
        live_record.cwd = live.clone();
        live_record.clone = None;
        crate::run_registry::record(&dir.0, live_record);
        let mut ended_record = crate::run_registry::sample_record("sess-ended");
        ended_record.cwd = ended.clone();
        ended_record.clone = None;
        crate::run_registry::record(&dir.0, ended_record);

        let report = sweep(&dir.0, &["sess-live".to_string()]);
        assert_eq!(report.removed, vec![ended.clone()]);
        assert_eq!(report.kept_live, 1);
        assert!(live.exists());
        assert!(!ended.exists());
        // The ended run's RECORD survives: it is what keeps the run resumable.
        assert!(crate::run_registry::get(&dir.0, "sess-ended").is_some());
    }

    /// A record a NEWER build wrote, which this one cannot parse, still names
    /// its cwd — an older host must never sweep a newer host's live run.
    #[test]
    fn sweep_keeps_a_live_cwd_named_only_by_an_unknown_record() {
        let dir = temp_dir("scratch-sweep-unknown");
        let live = scratch(&dir.0, "builtin_chat", "aaaaaaaa");
        age(&live, OLD);
        std::fs::write(
            dir.0.join("runs.json"),
            format!(
                r#"[{{"sessionId":"sess-future","cwd":{},"recordedAt":{}}}]"#,
                serde_json::to_string(&live.to_string_lossy()).unwrap(),
                crate::run_registry::now_secs()
            ),
        )
        .unwrap();
        let report = sweep(&dir.0, &["sess-future".to_string()]);
        assert!(report.removed.is_empty(), "{report:?}");
        assert_eq!(report.kept_live, 1);
        assert!(live.exists());
    }

    #[test]
    fn sweep_clears_legacy_sidecars_and_leaves_unknown_files_alone() {
        let dir = temp_dir("scratch-sweep-legacy");
        let action_dir = scratch_root(&dir.0).join("act-1");
        std::fs::create_dir_all(&action_dir).unwrap();
        let legacy = action_dir.join(crate::mcp_json::MCP_JSON_FILE);
        std::fs::write(&legacy, "{}").unwrap();
        age(&legacy, OLD);
        let other = scratch_root(&dir.0).join("act-2");
        std::fs::create_dir_all(&other).unwrap();
        let stranger = other.join("notes.txt");
        std::fs::write(&stranger, "mine").unwrap();
        age(&stranger, OLD);

        let report = sweep(&dir.0, &[]);
        assert!(report.removed.is_empty());
        assert!(!legacy.exists());
        assert!(!action_dir.exists(), "the emptied action dir goes");
        assert!(stranger.exists(), "an unknown file is never ours to delete");
        assert!(other.exists());
    }

    #[test]
    fn sweep_without_a_root_is_a_noop() {
        let dir = temp_dir("scratch-sweep-empty");
        assert_eq!(sweep(&dir.0, &[]), SweepReport::default());
    }
}
