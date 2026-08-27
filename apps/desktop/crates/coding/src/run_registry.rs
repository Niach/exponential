//! EXP-637 — the on-disk record of ACTION/CHAT runs this install launched
//! (`<data_dir>/runs.json`), so an ended run can be RESUMED: the row id
//! alone says nothing about which agent ran, where, on what branch, or with
//! which options.
//!
//! Deliberately its OWN file, not an extension of
//! `coding-session-registry.json`: that one has a byte contract shared with
//! the CLI's crash reconcile (`cli/src/registry.rs` + `ui/src/
//! session_registry.rs` round-trip each other's entries), and adding fields
//! there would be silently dropped by whichever side rewrites first. This
//! file is written and read by the SAME struct on both hosts.
//!
//! gpui-free and dependency-light on purpose — the desktop app and the
//! headless CLI daemon both record into it and both resume out of it.
//!
//! Everything here is best-effort: a corrupt or missing file simply means
//! "no resumable runs", never a failed launch.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;

/// Records past this age are dropped on the next write — a resume that far
/// out would find a pruned worktree and a garbage-collected transcript
/// anyway.
const TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Serializes every load-modify-save, exactly like the session registry:
/// the automation host records on its own threads while the cleanup path
/// removes concurrently.
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Which program the recorded run executed — the resume path re-enters the
/// same one. Mirrors `launcher::ActionRunKind` without its payloads (a
/// fix-conflicts resume keeps its PR context in [`RunFix`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind {
    Team,
    Chat,
    CreateAction,
    FixConflicts,
}

/// The fix-conflicts run's PR context, kept so a resume lands back on the
/// same branch with the same merge target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFix {
    pub branch: String,
    pub default_branch: String,
    pub identifier: String,
    pub issue_id: String,
}

/// One recorded input value (the definition-ordered run form), replayed into
/// a resume's fallback prompt when the native transcript is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInput {
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// Everything a resume needs about one finished (or running) action/chat run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    /// The `coding_sessions` row id — the record's primary key.
    pub session_id: String,
    pub account_id: String,
    pub agent: CodingAgent,
    pub kind: RunKind,
    /// The action row id (or the builtin literal) this run executed.
    pub action_id: String,
    pub action_name: String,
    pub team_id: String,
    /// The spawn cwd — a run worktree, the trunk clone, or a scratch dir.
    pub cwd: PathBuf,
    /// The clone the worktree hangs off; `None` on repo-less scratch runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<PathBuf>,
    /// `owner/name` — resolves the repo for the resume's token mint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The team `repositories` row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// The run's own branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// What the run branch was cut from (`origin/<base_branch>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// EXP-443 identity pins — the strongest resume handles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_session_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_originator: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<RunInput>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub ultracode: bool,
    #[serde(default)]
    pub skip_permissions: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<RunFix>,
    /// `schedule`/`event` when an automation started the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_reason: Option<String>,
    /// The ended session THIS run resumed (a resume of a resume chains).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumed_from_id: Option<String>,
    /// Unix seconds — the TTL prune's key.
    pub recorded_at: u64,
}

impl RunRecord {
    /// Whether the recorded workspace still exists: the resume spawns IN
    /// this cwd, and a repo-backed run also needs its worktree's `.git`
    /// link (a removed worktree leaves the dir gone, or gutted).
    pub fn resumable(&self) -> bool {
        if !self.cwd.is_dir() {
            return false;
        }
        if self.clone.is_some() && !self.cwd.join(".git").exists() {
            return false;
        }
        true
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("runs.json")
}

fn load(data_dir: &Path) -> Vec<RunRecord> {
    let Ok(raw) = std::fs::read_to_string(registry_path(data_dir)) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|err| {
        log::warn!("run registry unreadable ({err}); starting empty");
        Vec::new()
    })
}

fn save(data_dir: &Path, records: &[RunRecord]) {
    let path = registry_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let Ok(json) = serde_json::to_string_pretty(records) else {
        return;
    };
    let _ = std::fs::create_dir_all(data_dir);
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Upsert by `session_id` (a re-record of the same run replaces it) and
/// drop everything past the TTL in the same pass.
pub fn record(data_dir: &Path, record: RunRecord) {
    let _guard = locked();
    let mut records = load(data_dir);
    let cutoff = now_secs().saturating_sub(TTL_SECS);
    records.retain(|old| old.session_id != record.session_id && old.recorded_at >= cutoff);
    records.push(record);
    save(data_dir, &records);
}

pub fn get(data_dir: &Path, session_id: &str) -> Option<RunRecord> {
    let _guard = locked();
    load(data_dir)
        .into_iter()
        .find(|record| record.session_id == session_id)
}

pub fn remove(data_dir: &Path, session_id: &str) {
    let _guard = locked();
    let mut records = load(data_dir);
    let before = records.len();
    records.retain(|record| record.session_id != session_id);
    if records.len() != before {
        save(data_dir, &records);
    }
}

/// Every recorded run branch on `clone` — the prune's nomination list
/// (EXP-637: run worktrees are ours to reclaim, but git still has to confirm
/// the branch landed before anything is removed).
pub fn branches_for_clone(data_dir: &Path, clone: &Path) -> Vec<String> {
    let _guard = locked();
    let mut branches: Vec<String> = load(data_dir)
        .into_iter()
        .filter(|record| record.clone.as_deref() == Some(clone))
        .filter_map(|record| record.branch)
        .collect();
    branches.sort();
    branches.dedup();
    branches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-run-registry-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(session_id: &str) -> RunRecord {
        RunRecord {
            session_id: session_id.to_string(),
            account_id: "acc-1".to_string(),
            agent: CodingAgent::Claude,
            kind: RunKind::Team,
            action_id: "act-1".to_string(),
            action_name: "Code review".to_string(),
            team_id: "ws-1".to_string(),
            cwd: PathBuf::from("/repos/owner/name.worktrees/code-review-1a2b3c4d"),
            clone: Some(PathBuf::from("/repos/owner/name")),
            repo: Some("owner/name".to_string()),
            repository_id: Some("repo-1".to_string()),
            branch: Some("exp/code-review-1a2b3c4d".to_string()),
            base_branch: Some("main".to_string()),
            claude_session_id: Some("cs-1".to_string()),
            pi_session_file: None,
            codex_originator: None,
            inputs: vec![RunInput {
                key: "scope".to_string(),
                value: "everything".to_string(),
                display: None,
            }],
            model: "fable".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            skip_permissions: true,
            fix: None,
            started_reason: None,
            resumed_from_id: None,
            recorded_at: now_secs(),
        }
    }

    #[test]
    fn round_trips_a_record() {
        let dir = temp_dir("round-trip");
        record(&dir, sample("sess-1"));
        assert_eq!(get(&dir, "sess-1").unwrap(), sample("sess-1"));
        assert_eq!(get(&dir, "sess-nope"), None);
        remove(&dir, "sess-1");
        assert_eq!(get(&dir, "sess-1"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_upserts_by_session_id() {
        let dir = temp_dir("upsert");
        record(&dir, sample("sess-1"));
        let mut updated = sample("sess-1");
        updated.branch = Some("exp/chat-deadbeef".to_string());
        record(&dir, updated);
        assert_eq!(
            get(&dir, "sess-1").unwrap().branch.as_deref(),
            Some("exp/chat-deadbeef")
        );
        assert_eq!(load(&dir).len(), 1, "an upsert must not duplicate");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ancient_records_are_pruned_on_write() {
        let dir = temp_dir("ttl");
        let mut old = sample("sess-old");
        old.recorded_at = now_secs().saturating_sub(TTL_SECS + 60);
        record(&dir, old);
        record(&dir, sample("sess-new"));
        assert_eq!(get(&dir, "sess-old"), None);
        assert!(get(&dir, "sess-new").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn branches_for_clone_are_sorted_and_scoped() {
        let dir = temp_dir("branches");
        record(&dir, sample("sess-1"));
        let mut second = sample("sess-2");
        second.branch = Some("exp/chat-00000001".to_string());
        record(&dir, second);
        let mut foreign = sample("sess-3");
        foreign.clone = Some(PathBuf::from("/repos/other/repo"));
        foreign.branch = Some("exp/elsewhere-1".to_string());
        record(&dir, foreign);
        let mut repo_less = sample("sess-4");
        repo_less.clone = None;
        repo_less.branch = None;
        record(&dir, repo_less);

        assert_eq!(
            branches_for_clone(&dir, Path::new("/repos/owner/name")),
            vec![
                "exp/chat-00000001".to_string(),
                "exp/code-review-1a2b3c4d".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_reads_as_empty() {
        let dir = temp_dir("corrupt");
        std::fs::write(registry_path(&dir), "{not json").unwrap();
        assert!(load(&dir).is_empty());
        assert_eq!(get(&dir, "sess-1"), None);
        // ... and a write heals it.
        record(&dir, sample("sess-1"));
        assert!(get(&dir, "sess-1").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resumable_needs_the_workspace_to_still_exist() {
        let dir = temp_dir("resumable");
        let mut record = sample("sess-1");
        record.cwd = dir.join("gone");
        record.clone = Some(dir.clone());
        assert!(!record.resumable(), "a removed worktree is not resumable");

        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        record.cwd = worktree.clone();
        assert!(!record.resumable(), "a gutted worktree has no .git");
        std::fs::write(worktree.join(".git"), "gitdir: /elsewhere").unwrap();
        assert!(record.resumable());

        // A repo-less scratch run only needs its directory.
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        record.clone = None;
        record.cwd = scratch;
        assert!(record.resumable());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
