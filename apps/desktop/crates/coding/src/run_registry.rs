//! EXP-637 — the on-disk record of the runs this install launched
//! (`<data_dir>/runs.json`), so an ended run can be RESUMED: the row id
//! alone says nothing about which agent ran, where, on what branch, or with
//! which options. EXP-662 widened it from action/chat runs to ISSUE and
//! BATCH sessions, which is what made the cwd-scoped `--continue` machinery
//! redundant — every resume now relaunches an exact recorded transcript.
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

use std::collections::BTreeMap;
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
/// same one. The first four mirror `launcher::ActionRunKind` without its
/// payloads (a fix-conflicts resume keeps its PR context in [`RunFix`]);
/// `Issue`/`Batch` (EXP-662) are the two SESSION shapes, whose subject rides
/// in [`RunRecord::issue_id`] / [`RunRecord::issues`] instead of an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind {
    Team,
    Chat,
    CreateAction,
    FixConflicts,
    Issue,
    Batch,
}

impl RunKind {
    /// Whether this record is an action/chat RUN (as opposed to an issue or
    /// batch session). Runs own their branch and worktree end-to-end; issue
    /// and batch worktrees are the prune's business.
    pub fn is_action(self) -> bool {
        !matches!(self, Self::Issue | Self::Batch)
    }

    /// Whether the recorded run owns the worktree it spawned in — i.e. may
    /// have it auto-removed when it ends ([`crate::run_cleanup`]). A
    /// fix-conflicts run works in the PR branch's shared worktree, and
    /// issue/batch worktrees survive their session by design.
    pub fn owns_run_worktree(self) -> bool {
        matches!(self, Self::Team | Self::Chat)
    }
}

/// One issue a BATCH run covered — enough to name the run and re-render its
/// fallback prompt without a sync store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunIssue {
    pub issue_id: String,
    pub identifier: String,
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

/// Everything a resume needs about one finished (or running) run or session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    /// The `coding_sessions` row id — the record's primary key.
    pub session_id: String,
    pub account_id: String,
    pub agent: CodingAgent,
    pub kind: RunKind,
    /// The action row id (or the builtin literal) this run executed; empty
    /// on issue/batch sessions (EXP-662), which have no action.
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub action_name: String,
    #[serde(default)]
    pub team_id: String,
    /// EXP-662 — the issue this SESSION coded on, and its identifier
    /// (`EXP-42`). Both `None` on every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_identifier: Option<String>,
    /// EXP-662 — the client-minted batch id (`exp/batch-<id8>`'s suffix) and
    /// the issues the batch covered. `None`/empty on every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<RunIssue>,
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
    /// EXP-712: the board this run's branch is based on — replayed on the
    /// resume mint so the reinstated worktree keeps the board's base branch
    /// instead of falling back to the repo's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_id: Option<String>,
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
    /// Every field of this entry this build does not know — the desktop app
    /// and the CLI daemon share the file and update independently, so a
    /// record a NEWER build widened must survive an older host's
    /// load-modify-save byte-for-byte instead of being re-serialized without
    /// it. Sorted so the rewritten JSON stays stable.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
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

    /// What this record is CALLED wherever a resume is offered or narrated
    /// (tab titles, the fallback prompt, the ended strip): the issue's
    /// identifier, the batch's `EXP-42 +2` shape, or the action's name.
    pub fn display_name(&self) -> String {
        match self.kind {
            RunKind::Issue => self
                .issue_identifier
                .clone()
                .unwrap_or_else(|| self.action_name.clone()),
            RunKind::Batch => {
                let first = self
                    .issues
                    .first()
                    .map(|issue| issue.identifier.as_str())
                    .unwrap_or("batch");
                format!("{first} +{}", self.issues.len().saturating_sub(1))
            }
            _ => self.action_name.clone(),
        }
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

/// The file split into what THIS build understands and what it does not.
/// Parsing is PER ENTRY: the desktop app and the CLI daemon share one data
/// dir and update independently, so a newer build widening [`RunKind`] (or
/// the record shape) writes entries this one cannot deserialize — as a
/// whole-file parse they would take every sibling record down with them, and
/// the next `record()` would rewrite the file with its single entry. Unknown
/// entries are instead carried verbatim in [`Registry::unknown`] through every
/// load-modify-save, so an older host never deletes a newer host's records.
///
/// The other half of that promise is [`RunRecord::extra`]: an entry this build
/// CAN parse but which carries fields it has never heard of keeps them too, so
/// a rewrite is not a silent downgrade of the newer host's record.
#[derive(Default)]
struct Registry {
    records: Vec<RunRecord>,
    unknown: Vec<serde_json::Value>,
}

/// Record keys this build DELETED (never keys it merely predates) — dropped
/// on load so [`RunRecord::extra`]'s forward-compat catch-all does not
/// resurrect them on every rewrite. `skipPermissions` went with EXP-690's
/// toggle; its `#[serde(skip_serializing)]` tombstone went with EXP-693.
const DEAD_KEYS: &[&str] = &["skipPermissions"];

fn load_registry(data_dir: &Path) -> Registry {
    let Ok(raw) = std::fs::read_to_string(registry_path(data_dir)) else {
        return Registry::default();
    };
    let entries: Vec<serde_json::Value> = match serde_json::from_str(&raw) {
        Ok(entries) => entries,
        Err(err) => {
            log::warn!("run registry unreadable ({err}); starting empty");
            return Registry::default();
        }
    };
    let mut registry = Registry::default();
    for entry in entries {
        match serde_json::from_value::<RunRecord>(entry.clone()) {
            Ok(mut record) => {
                // EXP-693: keys this build RETIRED are not keys it never heard
                // of — [`RunRecord::extra`] would otherwise carry a pre-0.15
                // `skipPermissions` through every rewrite forever.
                for dead in DEAD_KEYS {
                    record.extra.remove(*dead);
                }
                registry.records.push(record);
            }
            Err(err) => {
                log::debug!("run registry: keeping an entry this build cannot read ({err})");
                registry.unknown.push(entry);
            }
        }
    }
    registry
}

fn load(data_dir: &Path) -> Vec<RunRecord> {
    load_registry(data_dir).records
}

/// A field off an unknown entry — the only two this build reads out of one
/// (the upsert key and the TTL key); everything else stays opaque.
fn entry_session_id(entry: &serde_json::Value) -> Option<&str> {
    entry.get("sessionId")?.as_str()
}

fn entry_recorded_at(entry: &serde_json::Value) -> Option<u64> {
    entry.get("recordedAt")?.as_u64()
}

fn save(data_dir: &Path, registry: &Registry) {
    let path = registry_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let mut entries: Vec<serde_json::Value> = Vec::with_capacity(registry.records.len());
    for record in &registry.records {
        let Ok(value) = serde_json::to_value(record) else {
            return;
        };
        entries.push(value);
    }
    entries.extend(registry.unknown.iter().cloned());
    let Ok(json) = serde_json::to_string_pretty(&entries) else {
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
    let mut registry = load_registry(data_dir);
    let cutoff = now_secs().saturating_sub(TTL_SECS);
    registry
        .records
        .retain(|old| old.session_id != record.session_id && old.recorded_at >= cutoff);
    // Unknown entries take the same upsert and TTL rules where they expose
    // the two keys they ride on, and are kept untouched where they don't.
    registry.unknown.retain(|entry| {
        entry_session_id(entry) != Some(record.session_id.as_str())
            && entry_recorded_at(entry).is_none_or(|at| at >= cutoff)
    });
    registry.records.push(record);
    save(data_dir, &registry);
}

pub fn get(data_dir: &Path, session_id: &str) -> Option<RunRecord> {
    let _guard = locked();
    load(data_dir)
        .into_iter()
        .find(|record| record.session_id == session_id)
}

/// EXP-662 — the newest still-resumable ISSUE record for `issue_id` on this
/// account. The Start-coding dialog's Resume offer and every remote
/// `resume: true` start resolve through here: a hit becomes a
/// `PrepareRequest::ResumeRun`, a miss a fresh launch. `record()` appends the
/// newest last, so a resume-of-a-resume chain resolves to its tail even when
/// two records share a second.
pub fn latest_for_issue(
    data_dir: &Path,
    account_id: &str,
    issue_id: &str,
) -> Option<RunRecord> {
    let _guard = locked();
    load(data_dir)
        .into_iter()
        .filter(|record| {
            record.kind == RunKind::Issue
                && record.account_id == account_id
                && record.issue_id.as_deref() == Some(issue_id)
                && record.resumable()
        })
        .max_by_key(|record| record.recorded_at)
}

pub fn remove(data_dir: &Path, session_id: &str) {
    let _guard = locked();
    let mut registry = load_registry(data_dir);
    let before = registry.records.len() + registry.unknown.len();
    registry
        .records
        .retain(|record| record.session_id != session_id);
    registry
        .unknown
        .retain(|entry| entry_session_id(entry) != Some(session_id));
    if registry.records.len() + registry.unknown.len() != before {
        save(data_dir, &registry);
    }
}

/// Every recorded run branch on `clone` — the prune's nomination list
/// (EXP-637: run worktrees are ours to reclaim, but git still has to confirm
/// the branch landed before anything is removed). EXP-662: ACTION kinds only
/// — issue and batch worktrees stay governed by the prune's own prefix/keep
/// policy, which a nomination would bypass.
pub fn branches_for_clone(data_dir: &Path, clone: &Path) -> Vec<String> {
    let _guard = locked();
    let mut branches: Vec<String> = load(data_dir)
        .into_iter()
        .filter(|record| record.clone.as_deref() == Some(clone))
        .filter(|record| record.kind.is_action())
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
            board_id: None,
            account_id: "acc-1".to_string(),
            agent: CodingAgent::Claude,
            kind: RunKind::Team,
            action_id: "act-1".to_string(),
            action_name: "Code review".to_string(),
            team_id: "ws-1".to_string(),
            issue_id: None,
            issue_identifier: None,
            batch_id: None,
            issues: Vec::new(),
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
            fix: None,
            started_reason: None,
            resumed_from_id: None,
            recorded_at: now_secs(),
            extra: BTreeMap::new(),
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

    /// EXP-662: an issue record made from `sample`'s shape — a real worktree
    /// so `resumable()` passes.
    fn issue_sample(dir: &Path, session_id: &str, issue_id: &str) -> RunRecord {
        let worktree = dir.join(format!("wt-{session_id}"));
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: /elsewhere").unwrap();
        RunRecord {
            kind: RunKind::Issue,
            action_id: String::new(),
            action_name: String::new(),
            issue_id: Some(issue_id.to_string()),
            issue_identifier: Some("EXP-42".to_string()),
            cwd: worktree,
            branch: Some("exp/EXP-42".to_string()),
            ..sample(session_id)
        }
    }

    #[test]
    fn latest_for_issue_prefers_the_newest_resumable_record() {
        let dir = temp_dir("latest-for-issue");
        let mut older = issue_sample(&dir, "sess-1", "issue-1");
        older.recorded_at = now_secs().saturating_sub(60);
        record(&dir, older);
        record(&dir, issue_sample(&dir, "sess-2", "issue-1"));
        // Another issue, another account, and an ACTION record on the same
        // account are all invisible to this lookup.
        record(&dir, issue_sample(&dir, "sess-3", "issue-2"));
        let mut foreign = issue_sample(&dir, "sess-4", "issue-1");
        foreign.account_id = "acc-2".to_string();
        record(&dir, foreign);
        record(&dir, sample("sess-5"));

        assert_eq!(
            latest_for_issue(&dir, "acc-1", "issue-1")
                .map(|record| record.session_id)
                .as_deref(),
            Some("sess-2")
        );
        // A record whose worktree is gone is not resumable — it must not
        // shadow the older one that still is.
        let mut gone = issue_sample(&dir, "sess-6", "issue-1");
        gone.cwd = dir.join("vanished");
        record(&dir, gone);
        assert_eq!(
            latest_for_issue(&dir, "acc-1", "issue-1")
                .map(|record| record.session_id)
                .as_deref(),
            Some("sess-2")
        );
        assert_eq!(latest_for_issue(&dir, "acc-1", "issue-nope"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn issue_and_batch_branches_are_not_prune_nominations() {
        // EXP-662: session worktrees stay governed by the prune's own
        // prefix/keep policy — nominating them would hand the prune a
        // worktree the user still expects to find.
        let dir = temp_dir("prune-nominations");
        record(&dir, sample("sess-1"));
        record(&dir, issue_sample(&dir, "sess-2", "issue-1"));
        let mut batch = issue_sample(&dir, "sess-3", "issue-1");
        batch.kind = RunKind::Batch;
        batch.issue_id = None;
        batch.issue_identifier = None;
        batch.batch_id = Some("a1b2c3d4".to_string());
        batch.branch = Some("exp/batch-a1b2c3d4".to_string());
        record(&dir, batch);

        assert_eq!(
            branches_for_clone(&dir, Path::new("/repos/owner/name")),
            vec!["exp/code-review-1a2b3c4d".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_name_names_each_kind() {
        let dir = temp_dir("display-name");
        assert_eq!(sample("sess-1").display_name(), "Code review");
        assert_eq!(issue_sample(&dir, "sess-2", "issue-1").display_name(), "EXP-42");
        let mut batch = sample("sess-3");
        batch.kind = RunKind::Batch;
        batch.issues = vec![
            RunIssue {
                issue_id: "issue-1".to_string(),
                identifier: "EXP-42".to_string(),
            },
            RunIssue {
                issue_id: "issue-2".to_string(),
                identifier: "EXP-43".to_string(),
            },
        ];
        assert_eq!(batch.display_name(), "EXP-42 +1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pre_exp662_action_record_still_parses() {
        // The EXP-637 wire, byte-for-byte: no issue/batch fields at all. A
        // record written by the previous build must keep resuming.
        let json = r#"[{
            "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
            "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
            "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
            "clone":"/repos/owner/name","repo":"owner/name","repositoryId":"repo-1",
            "branch":"exp/code-review-1a2b3c4d","baseBranch":"main",
            "claudeSessionId":"cs-1","model":"fable","effort":"high",
            "ultracode":false,"skipPermissions":true,"recordedAt":1
        }]"#;
        let records: Vec<RunRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, RunKind::Team);
        assert_eq!(records[0].issue_id, None);
        assert!(records[0].issues.is_empty());
        assert_eq!(records[0].display_name(), "Code review");
    }

    #[test]
    fn a_retired_skip_permissions_key_neither_breaks_a_load_nor_survives_a_write() {
        // EXP-693 removed the `skip_permissions` tombstone field. Records the
        // pre-0.15 builds wrote still carry the key: it must parse (no
        // `deny_unknown_fields` anywhere on `RunRecord`) and must NOT ride
        // `extra` back out — `extra` is for fields a NEWER build added, never
        // for ones this build deleted.
        let dir = temp_dir("dead-key");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
                "claudeSessionId":"cs-1","model":"fable","effort":"high",
                "ultracode":false,"skipPermissions":true,"recordedAt":{now}
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        let loaded = get(&dir, "sess-1").expect("a pre-0.15 record still parses");
        assert_eq!(loaded.action_name, "Code review");
        assert!(loaded.extra.get("skipPermissions").is_none());

        // And the rewrite drops it for good.
        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert!(entries
            .iter()
            .all(|entry| entry.get("skipPermissions").is_none()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_this_build_cannot_read_survives_a_write() {
        // The desktop app and the CLI daemon share this file and update
        // independently: an entry a NEWER build wrote (here a widened
        // `RunKind`) must cost nothing but itself — its siblings still load,
        // and the next write carries it through verbatim instead of the older
        // host silently wiping every record it could not parse.
        let dir = temp_dir("unknown-entry");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
                "clone":"/repos/owner/name","repo":"owner/name","repositoryId":"repo-1",
                "branch":"exp/code-review-1a2b3c4d","baseBranch":"main",
                "claudeSessionId":"cs-1","model":"fable","effort":"high",
                "ultracode":false,"skipPermissions":true,"recordedAt":{now}
            }},{{
                "sessionId":"sess-future","accountId":"acc-1","agent":"claude",
                "kind":"someFutureKind","cwd":"/repos/owner/name",
                "somethingNew":{{"deep":[1,2]}},"recordedAt":{now}
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        // The readable record loads; the other one is invisible to every read.
        assert_eq!(load(&dir).len(), 1);
        assert!(get(&dir, "sess-1").is_some());
        assert_eq!(get(&dir, "sess-future"), None);

        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert_eq!(entries.len(), 3, "the write kept both siblings");
        let future = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-future")
            .expect("the unreadable entry survives a neighbour's write");
        assert_eq!(future["kind"], "someFutureKind");
        assert_eq!(future["somethingNew"]["deep"], serde_json::json!([1, 2]));
        assert_eq!(load(&dir).len(), 2);

        // It is still addressable by session id, so a removal reaches it.
        remove(&dir, "sess-future");
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert_eq!(entries.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_known_entry_keeps_the_fields_this_build_never_heard_of() {
        // The half [`RunRecord::extra`] covers: a NEWER build's record whose
        // `kind` this one already knows parses fine, so the lenient per-entry
        // load cannot save it — without the flattened catch-all the next
        // `record()` would rewrite it stripped of `worktreeMode`, silently
        // downgrading the other host's record.
        let dir = temp_dir("unknown-field");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
                "clone":"/repos/owner/name","repo":"owner/name","repositoryId":"repo-1",
                "branch":"exp/code-review-1a2b3c4d","baseBranch":"main",
                "claudeSessionId":"cs-1","model":"fable","effort":"high",
                "ultracode":false,"skipPermissions":true,"recordedAt":{now},
                "worktreeMode":"scratch"
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        // It loads as an ordinary record, unknown field and all.
        let loaded = get(&dir, "sess-1").expect("a known kind still parses");
        assert_eq!(loaded.action_name, "Code review");
        assert_eq!(
            loaded.extra.get("worktreeMode"),
            Some(&serde_json::json!("scratch"))
        );

        // ... and a neighbour's write carries it back out verbatim.
        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert_eq!(entries.len(), 2);
        let kept = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-1")
            .expect("the record survives");
        assert_eq!(kept["worktreeMode"], "scratch");
        assert_eq!(kept["kind"], "team");
        // A record with nothing extra serializes exactly as before — no
        // empty object, no stray key.
        let plain = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-2")
            .expect("the neighbour");
        assert_eq!(plain.get("extra"), None);
        assert_eq!(plain.get("worktreeMode"), None);
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
