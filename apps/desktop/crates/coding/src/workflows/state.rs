//! EXP-982/EXP-1102 — the workflow engine's host bookkeeping, in a store of
//! its OWN: `{data_dir}/workflows/<workflowId>.json`, one document per
//! workflow, one writer (the host that evaluates it), replaced by rename
//! ([`api::device_store::JsonStore`]).
//!
//! Until EXP-1102 this rode as one foreign key of settings.json, the file
//! eight writers across two processes merge into; one torn write of it cost
//! workflow 2f353e88 its host and every bit of this state with it. Losing
//! this store must now cost ONE slower beat, never a stuck workflow, so
//! everything here is either a CACHE the host re-derives from git and the
//! synced rows (`merged`, `synthetic`, `conflicts`, `tips_seen`, `fix_runs`,
//! `checkpoint_tips`) or at-most-once bookkeeping whose loss is one repeated
//! side effect (`nudged`, `woken`, `branch_deleted`, `bases_deleted`,
//! `land_refused`, the review run records, `resuming`):
//!
//! ```json
//! {
//!   "nudged": [["<sessionId>", "<key>"]],
//!   "branchDeleted": true,
//!   "merged": { "<nodeId>": { "<branch>": "<sha>" } },
//!   "woken": { "<nodeId>": { "<branch>": "<sha>" } },
//!   "synthetic": { "<baseBranch>": [["<branch>", "<sha>"]] },
//!   "conflicts": { "<a>|<b>|<shaA>|<shaB>": true },
//!   "basesDeleted": ["<baseBranch>"],
//!   "tipsSeen": { "<branch>": ["<sha>", 1726000000000] },
//!   "reviewRuns": { "<nodeId>": "<sessionId>" },
//!   "reviewRounds": { "<nodeId>": 1 },
//!   "reviewFailures": { "<nodeId>": 1 },
//!   "fixRuns": { "<wave>": "<sessionId>" },
//!   "finalPrCloseHandled": false,
//!   "landRefused": { "<nodeId>": "<sha>" },
//!   "resuming": { "<sessionId>": 1726000000000 }
//! }
//! ```
//!
//! The in-flight sets are NOT here: they live for one host process and must
//! not survive a crash (a restart re-evaluates and re-decides from the
//! synced rows, which is the whole point of a level-triggered engine).
//!
//! A pre-EXP-1102 install still carries its state under settings.json's
//! `workflowEngine.<deviceId>` key: [`WorkflowStore::migrate_legacy`] moves
//! it into the store once and clears that device's sub-map, so a sibling
//! process of an older build keeps its own.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The legacy settings.json top-level key (read for migration only).
pub const WORKFLOW_ENGINE_KEY: &str = "workflowEngine";

/// The store's directory under the data dir.
pub const WORKFLOWS_STORE_DIR: &str = "workflows";

/// One workflow's bookkeeping on one device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkflowState {
    /// `(session id, key)` pairs already said to a run: a rate-limit reset
    /// (`key` = the reset stamp), the contract's freeze notice (`freeze`).
    pub nudged: HashSet<(String, String)>,
    /// The cancel sweep already dropped the integration branch.
    pub branch_deleted: bool,
    /// EXP-1059: the current CLOSED episode of the final PR was put to the
    /// server (`Snapshot::final_pr_close_handled`); cleared by
    /// [`final_pr_close_handled`] once the PR reads open again.
    pub final_pr_close_handled: bool,
    /// EXP-1106 (cache): `node id → base branch → the tip of it KNOWN to be
    /// in the node's branch` — merged by the host, or cut at it. Re-derived
    /// by ancestry (`git merge-base --is-ancestor`) when missing.
    pub merged: HashMap<String, HashMap<String, String>>,
    /// EXP-1106: `node id → base branch → the tip whose CONFLICT the run was
    /// already woken for`, so one collision wakes the agent once.
    pub woken: HashMap<String, HashMap<String, String>>,
    /// EXP-983 (cache): `synthetic base → the (branch, sha) pairs it was
    /// built from`. A difference is what makes the next pass refresh it.
    pub synthetic: HashMap<String, Vec<(String, String)>>,
    /// EXP-983 (cache): `git merge-tree` verdicts, keyed by the two sides and
    /// the two tips they were tested at.
    pub conflicts: HashMap<String, bool>,
    /// EXP-983: synthetic bases this device already dropped.
    pub bases_deleted: HashSet<String>,
    /// EXP-1106 (cache): `branch → (sha, ms epoch the host first saw that
    /// sha)` — the debounce clock for upstream moves.
    pub tips_seen: HashMap<String, (String, i64)>,
    /// EXP-984: `node id → the session id of its reviewer run`, so a live
    /// review is never started twice (its liveness comes off the synced
    /// `coding_sessions` row).
    pub review_runs: HashMap<String, String>,
    /// EXP-984: `node id → the node's review round when its reviewer run was
    /// launched` — a run that ends with the round unchanged had no verdict.
    pub review_rounds: HashMap<String, i64>,
    /// EXP-984: `node id → reviewer runs that ended without a verdict`, the
    /// bound on re-launching them.
    pub review_failures: HashMap<String, i64>,
    /// EXP-1103 (cache): `review wave → the session id of its fix run`
    /// (re-derived from the synced rows by the fix branch).
    pub fix_runs: HashMap<i64, String>,
    /// `node id → the head of its pull request when GitHub last refused to
    /// merge it`; the node holds `updating` until that head moves.
    pub land_refused: HashMap<String, String>,
    /// `session id → ms epoch` of the runs this host is RESUMING: they read
    /// as live until the node names the new run, or the grace passes.
    pub resuming: HashMap<String, i64>,
}

/// The conflict cache's key: the pair and the tips it was decided at, both
/// orders folded into one so `(a, b)` and `(b, a)` share a verdict.
pub fn conflict_key(left: (&str, &str), right: (&str, &str)) -> String {
    let (first, second) = if left.0 <= right.0 {
        (left, right)
    } else {
        (right, left)
    };
    format!("{}|{}|{}|{}", first.0, second.0, first.1, second.1)
}

/// FEED-49: a PERSON's resume of a node run — the desktop Resume button, a
/// relay resume frame, an account switch — takes the same hold the engine's
/// own resumes take ([`super::apply_resuming`]). The run is ENDED before the
/// server re-points the node at its successor (`codingSessions.start`), and
/// a pass in between read the ended row and re-decided the node. Held, the
/// ended row reads live until the node names the new run, or the grace
/// passes.
///
/// `nodes` = every synced `(workflow id, session id)` pair. Returns the
/// workflow held; `None` when no node names the run and no workflow's
/// `review_runs` records it (not a workflow run: nothing to hold, nothing
/// to write).
pub fn hold_resume(
    states: &mut HashMap<String, WorkflowState>,
    nodes: impl IntoIterator<Item = (String, String)>,
    session_id: &str,
    now_ms: i64,
) -> Option<String> {
    let workflow_id = nodes
        .into_iter()
        .find(|(_, named)| named == session_id)
        .map(|(workflow_id, _)| workflow_id)
        .or_else(|| {
            let mut reviewing: Vec<&String> = states
                .iter()
                .filter(|(_, state)| state.review_runs.values().any(|id| id == session_id))
                .map(|(workflow_id, _)| workflow_id)
                .collect();
            reviewing.sort();
            reviewing.first().map(|id| (*id).clone())
        })?;
    states
        .entry(workflow_id.clone())
        .or_default()
        .resuming
        .insert(session_id.to_string(), now_ms);
    Some(workflow_id)
}

/// The hold [`hold_resume`] took, released: the resume was refused after the
/// run ended, so the engine may decide the node now rather than after the
/// grace. `true` when a hold was there.
pub fn release_resume(states: &mut HashMap<String, WorkflowState>, session_id: &str) -> bool {
    let mut released = false;
    for state in states.values_mut() {
        released |= state.resuming.remove(session_id).is_some();
    }
    released
}

/// The pass's write-back of its settled `resuming` map, merged rather than
/// assigned: a hold a person's resume took ([`hold_resume`], off the
/// foreground) between the pass's read (`before`) and this write is KEPT,
/// while every entry the pass dropped (past the grace, or no node names it)
/// goes. FEED-49: an entry is dropped by VALUE, not by key, so a hold
/// re-taken meanwhile on a session id the pass read (and aged out) stays.
pub fn merge_resuming(
    persisted: &mut HashMap<String, i64>,
    before: &HashMap<String, i64>,
    settled: HashMap<String, i64>,
) {
    persisted.retain(|session_id, held_at| before.get(session_id) != Some(held_at));
    persisted.extend(settled);
}

/// EXP-1059: the snapshot's `final_pr_close_handled` for one workflow. True
/// only while the final PR still reads `closed` AND this device already put
/// that close to the server. A PR back at `open` (the reopen's echo, or a
/// member's `openFinalPr`) CLEARS the persisted flag, so the next close is
/// asked again — the server's `final_pr_reopened` event decides whether
/// that is the one reopen or a person's decision.
pub fn final_pr_close_handled(store: &WorkflowStore, workflow_id: &str, final_pr_closed: bool) -> bool {
    let state = store.read(workflow_id);
    if !state.final_pr_close_handled {
        return false;
    }
    if final_pr_closed {
        return true;
    }
    if let Err(err) = store.update(workflow_id, |state| state.final_pr_close_handled = false) {
        log::warn!("[workflows] state write failed: {err}");
    }
    false
}

/// The store: `{data_dir}/workflows/`, one document per workflow id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowStore {
    inner: api::device_store::JsonStore,
}

impl WorkflowStore {
    /// The store under `data_dir`.
    pub fn open(data_dir: &Path) -> Self {
        Self::at(data_dir.join(WORKFLOWS_STORE_DIR))
    }

    /// A store at an explicit directory (tests).
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            inner: api::device_store::JsonStore::new(dir),
        }
    }

    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// One workflow's state; missing or unreadable → default (a cache miss,
    /// which the next pass re-derives).
    pub fn read(&self, workflow_id: &str) -> WorkflowState {
        self.inner
            .read(workflow_id)
            .map(|value| decode(&value))
            .unwrap_or_default()
    }

    /// Every stored workflow's state, by id.
    pub fn read_all(&self) -> HashMap<String, WorkflowState> {
        self.inner
            .names()
            .into_iter()
            .map(|id| {
                let state = self.read(&id);
                (id, state)
            })
            .collect()
    }

    pub fn write(&self, workflow_id: &str, state: &WorkflowState) -> std::io::Result<()> {
        self.inner.write(workflow_id, &encode(state))
    }

    /// Read-modify-write one workflow's document under the store's section.
    pub fn update(
        &self,
        workflow_id: &str,
        edit: impl FnOnce(&mut WorkflowState),
    ) -> std::io::Result<()> {
        self.inner.update(workflow_id, |value| {
            let mut state = decode(value);
            edit(&mut state);
            *value = encode(&state);
        })
    }

    /// Drop the documents of workflows that are GONE — not among `keep`, the
    /// ids of every synced workflow row this device can see (any device's:
    /// a workflow rebound to a sibling host on the same machine keeps its
    /// document for that host to pick up).
    pub fn prune(&self, keep: &HashSet<String>) {
        for id in self.inner.names() {
            if !keep.contains(&id) {
                if let Err(err) = self.inner.remove(&id) {
                    log::warn!("[workflows] pruning state {id} failed: {err}");
                }
            }
        }
    }

    /// EXP-1102: move this device's pre-store state out of settings.json —
    /// every workflow under `workflowEngine.<device_id>` becomes a document
    /// (a document already there wins), then that device's sub-map is
    /// cleared so nothing reads it twice. Idempotent; a settings.json without
    /// the key does nothing at all. Returns how many documents were written.
    pub fn migrate_legacy(&self, settings_path: &Path, device_id: &str) -> usize {
        let Some(data_dir) = settings_path.parent() else {
            return 0;
        };
        let _guard = api::settings_lock::locked(data_dir);
        let Some(mut root) = read_root(settings_path) else {
            return 0;
        };
        let Some(entries) = root
            .get(WORKFLOW_ENGINE_KEY)
            .and_then(|devices| devices.get(device_id))
            .and_then(Value::as_object)
            .cloned()
        else {
            return 0;
        };
        let mut written = 0;
        for (workflow_id, entry) in &entries {
            if self.inner.read(workflow_id).is_some() {
                continue;
            }
            let state = decode_legacy(entry);
            match self.write(workflow_id, &state) {
                Ok(()) => written += 1,
                Err(err) => {
                    log::warn!("[workflows] migrating state {workflow_id} failed: {err}");
                    return written;
                }
            }
        }
        if let Some(devices) = root
            .get_mut(WORKFLOW_ENGINE_KEY)
            .and_then(Value::as_object_mut)
        {
            devices.remove(device_id);
            if devices.is_empty() {
                root.as_object_mut().map(|object| object.remove(WORKFLOW_ENGINE_KEY));
            }
        }
        let mut rendered = serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
        rendered.push('\n');
        if let Err(err) = api::atomic_file::write_atomic(settings_path, &rendered) {
            log::warn!("[workflows] clearing the legacy state key failed: {err}");
        }
        written
    }
}

fn encode(state: &WorkflowState) -> Value {
    // Sorted: a file a person reads, and a set's iteration order would
    // churn it on every write.
    let mut nudged: Vec<(String, String)> = state.nudged.iter().cloned().collect();
    nudged.sort();
    let mut bases_deleted: Vec<String> = state.bases_deleted.iter().cloned().collect();
    bases_deleted.sort();
    let fix_runs: serde_json::Map<String, Value> = state
        .fix_runs
        .iter()
        .map(|(wave, session_id)| (wave.to_string(), Value::String(session_id.clone())))
        .collect();
    let tips_seen: serde_json::Map<String, Value> = state
        .tips_seen
        .iter()
        .map(|(branch, (sha, at))| (branch.clone(), serde_json::json!([sha, at])))
        .collect();
    serde_json::json!({
        "nudged": nudged,
        "branchDeleted": state.branch_deleted,
        "finalPrCloseHandled": state.final_pr_close_handled,
        "merged": state.merged,
        "woken": state.woken,
        "synthetic": state.synthetic,
        "conflicts": state.conflicts,
        "basesDeleted": bases_deleted,
        "tipsSeen": tips_seen,
        "reviewRuns": state.review_runs,
        "reviewRounds": state.review_rounds,
        "reviewFailures": state.review_failures,
        "fixRuns": fix_runs,
        "landRefused": state.land_refused,
        "resuming": state.resuming,
    })
}

/// Decode a document of any vintage; anything malformed reads empty rather
/// than poisoning the rest (the worst case is one repeated side effect or
/// one re-derivation).
fn decode(value: &Value) -> WorkflowState {
    WorkflowState {
        nudged: value
            .get("nudged")
            .and_then(Value::as_array)
            .map(|pairs| {
                pairs
                    .iter()
                    .filter_map(|pair| {
                        let pair = pair.as_array()?;
                        let session_id = pair.first()?.as_str()?.to_string();
                        // A pre-EXP-1106 document keyed the reset stamp as a
                        // number; it reads as the same string key.
                        let key = match pair.get(1)? {
                            Value::String(key) => key.clone(),
                            Value::Number(number) => number.to_string(),
                            _ => return None,
                        };
                        Some((session_id, key))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        branch_deleted: value
            .get("branchDeleted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        final_pr_close_handled: value
            .get("finalPrCloseHandled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        merged: read_nested(value.get("merged")),
        woken: read_nested(value.get("woken")),
        synthetic: read_synthetic(value.get("synthetic")),
        conflicts: value
            .get("conflicts")
            .and_then(Value::as_object)
            .map(|rows| {
                rows.iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.as_bool()?)))
                    .collect()
            })
            .unwrap_or_default(),
        bases_deleted: read_string_set(value.get("basesDeleted")),
        tips_seen: value
            .get("tipsSeen")
            .and_then(Value::as_object)
            .map(|rows| {
                rows.iter()
                    .filter_map(|(branch, pair)| {
                        let pair = pair.as_array()?;
                        Some((
                            branch.clone(),
                            (pair.first()?.as_str()?.to_string(), pair.get(1)?.as_i64()?),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        review_runs: read_string_map(value.get("reviewRuns")),
        review_rounds: read_round_map(value.get("reviewRounds")),
        review_failures: read_round_map(value.get("reviewFailures")),
        fix_runs: value
            .get("fixRuns")
            .and_then(Value::as_object)
            .map(|rows| {
                rows.iter()
                    .filter_map(|(wave, session_id)| {
                        Some((wave.parse().ok()?, session_id.as_str()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        land_refused: read_string_map(value.get("landRefused")),
        resuming: read_round_map(value.get("resuming")),
    }
}

/// A pre-EXP-1102 settings.json entry: the same keys, plus `propagated`
/// (what a node was TOLD to merge — the nearest thing to `merged`) and the
/// per-node review fields the wave model dropped, which are ignored.
fn decode_legacy(entry: &Value) -> WorkflowState {
    let mut state = decode(entry);
    if state.merged.is_empty() {
        state.merged = read_nested(entry.get("propagated"));
    }
    state
}

/// `node id → branch → sha`.
fn read_nested(value: Option<&Value>) -> HashMap<String, HashMap<String, String>> {
    value
        .and_then(Value::as_object)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|(node_id, branches)| {
                    let branches = branches.as_object()?;
                    Some((
                        node_id.clone(),
                        branches
                            .iter()
                            .filter_map(|(branch, sha)| {
                                Some((branch.clone(), sha.as_str()?.to_string()))
                            })
                            .collect(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_string_map(value: Option<&Value>) -> HashMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|rows| {
            rows.iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn read_string_set(value: Option<&Value>) -> HashSet<String> {
    value
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn read_round_map(value: Option<&Value>) -> HashMap<String, i64> {
    value
        .and_then(Value::as_object)
        .map(|rows| {
            rows.iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_i64()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// `synthetic base → [[branch, sha]]`.
fn read_synthetic(value: Option<&Value>) -> HashMap<String, Vec<(String, String)>> {
    value
        .and_then(Value::as_object)
        .map(|bases| {
            bases
                .iter()
                .filter_map(|(base, sources)| {
                    let sources = sources.as_array()?;
                    Some((
                        base.clone(),
                        sources
                            .iter()
                            .filter_map(|pair| {
                                let pair = pair.as_array()?;
                                Some((
                                    pair.first()?.as_str()?.to_string(),
                                    pair.get(1)?.as_str()?.to_string(),
                                ))
                            })
                            .collect(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_root(settings_path: &Path) -> Option<Value> {
    std::fs::read_to_string(settings_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_object)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-workflows-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn state(session_id: &str, key: &str, deleted: bool) -> WorkflowState {
        WorkflowState {
            nudged: [(session_id.to_string(), key.to_string())].into_iter().collect(),
            branch_deleted: deleted,
            ..WorkflowState::default()
        }
    }

    /// One document per workflow: a write never touches a sibling, a prune
    /// drops only what is gone, and settings.json is never created.
    #[test]
    fn documents_round_trip_and_prune() {
        let dir = temp_dir("roundtrip");
        let store = WorkflowStore::open(&dir.0);
        assert_eq!(store.dir(), dir.0.join("workflows"));
        let one = state("s-1", "100", false);
        let two = state("s-2", "200", true);
        store.write("wf-1", &one).unwrap();
        store.write("wf-2", &two).unwrap();
        assert_eq!(store.read("wf-1"), one);
        assert_eq!(store.read("wf-2"), two);
        assert_eq!(store.read("unknown"), WorkflowState::default());
        assert_eq!(store.read_all().len(), 2);
        store
            .update("wf-1", |state| state.branch_deleted = true)
            .unwrap();
        assert!(store.read("wf-1").branch_deleted);
        assert_eq!(store.read("wf-2"), two, "the sibling is untouched");
        store.prune(&["wf-2".to_string()].into_iter().collect());
        assert_eq!(store.read_all().keys().collect::<Vec<_>>(), ["wf-2"]);
        assert!(!dir.0.join("settings.json").exists());
    }

    /// EXP-1102: the legacy settings.json state moves into the store ONCE
    /// (this device's sub-map only), and the key is cleared so a second run
    /// finds nothing; a document already in the store wins.
    #[test]
    fn legacy_settings_state_migrates_once_and_leaves_siblings() {
        let dir = temp_dir("migrate");
        let path = dir.0.join("settings.json");
        let raw = serde_json::json!({
            "deviceId": "desk-1",
            "claudeModel": "sonnet",
            "workflowEngine": {
                "desk-1": {
                    "wf-1": {
                        "nudged": ["s-1\u{1f}100"],
                        "branchDeleted": true,
                        "propagated": { "n-1": { "exp/wf-abcdef12": "sha-i1" } },
                        "reviewRuns": { "n-1": "r-1" },
                        "findingsSent": { "n-1": 2 },
                    },
                    "wf-2": { "landRefused": { "n-2": "sha-x" } }
                },
                "cli-1": { "wf-9": { "branchDeleted": true } }
            }
        });
        std::fs::write(&path, raw.to_string()).unwrap();
        let store = WorkflowStore::open(&dir.0);
        store.write("wf-2", &state("s-2", "k", false)).unwrap();
        assert_eq!(store.migrate_legacy(&path, "desk-1"), 1);
        let one = store.read("wf-1");
        assert!(one.branch_deleted);
        // `propagated` (what a node was told) is the nearest thing to what is
        // known to be merged; the per-node review fields are gone.
        assert_eq!(one.merged["n-1"]["exp/wf-abcdef12"], "sha-i1");
        assert_eq!(one.review_runs["n-1"], "r-1");
        // The old nudge key form (session + separator + stamp) is a single
        // malformed entry now: dropped, which can only re-send one nudge.
        assert!(one.nudged.is_empty());
        assert_eq!(store.read("wf-2"), state("s-2", "k", false), "the store's copy wins");
        let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["deviceId"], "desk-1");
        assert_eq!(root["claudeModel"], "sonnet");
        assert!(root["workflowEngine"].get("desk-1").is_none(), "this device's map is cleared");
        assert!(root["workflowEngine"]["cli-1"]["wf-9"]["branchDeleted"].as_bool().unwrap());
        assert_eq!(store.migrate_legacy(&path, "desk-1"), 0, "nothing left to migrate");
        // A settings.json without the key at all: a no-op.
        assert_eq!(store.migrate_legacy(&dir.0.join("missing.json"), "desk-1"), 0);
    }

    /// FEED-49: a person's resume holds the node's workflow exactly as the
    /// engine's own resume does — under the workflow whose node names the
    /// run, and only then.
    #[test]
    fn a_person_resume_holds_the_workflow_whose_node_names_the_run() {
        let mut states: HashMap<String, WorkflowState> = HashMap::new();
        let nodes = vec![
            ("wf-1".to_string(), "s-other".to_string()),
            ("wf-2".to_string(), "s-old".to_string()),
        ];
        assert_eq!(
            hold_resume(&mut states, nodes.clone(), "s-old", 1_000),
            Some("wf-2".to_string())
        );
        assert_eq!(states["wf-2"].resuming.get("s-old"), Some(&1_000));
        assert!(!states.contains_key("wf-1"));
        assert_eq!(hold_resume(&mut states, nodes.clone(), "s-plain", 2_000), None);
        assert_eq!(states.len(), 1);
        states
            .entry("wf-3".to_string())
            .or_default()
            .review_runs
            .insert("node-1".to_string(), "r-1".to_string());
        assert_eq!(
            hold_resume(&mut states, nodes, "r-1", 3_000),
            Some("wf-3".to_string())
        );
        assert_eq!(states["wf-3"].resuming.get("r-1"), Some(&3_000));
        assert!(release_resume(&mut states, "s-old"));
        assert!(states["wf-2"].resuming.is_empty());
        assert!(!release_resume(&mut states, "s-old"));
    }

    /// FEED-49: the pass's write-back keeps a hold taken while it ran, and
    /// drops what the pass dropped — by value, so a re-taken hold survives.
    #[test]
    fn the_pass_write_back_keeps_holds_taken_meanwhile() {
        let before: HashMap<String, i64> =
            [("s-kept".to_string(), 10), ("s-expired".to_string(), 5)].into_iter().collect();
        let mut persisted = before.clone();
        persisted.insert("s-switched".to_string(), 42);
        let settled: HashMap<String, i64> = [("s-kept".to_string(), 10)].into_iter().collect();
        merge_resuming(&mut persisted, &before, settled);
        assert_eq!(
            persisted,
            [("s-kept".to_string(), 10), ("s-switched".to_string(), 42)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );
        let before: HashMap<String, i64> = [("s".to_string(), 5)].into_iter().collect();
        let mut persisted: HashMap<String, i64> = [("s".to_string(), 42)].into_iter().collect();
        merge_resuming(&mut persisted, &before, HashMap::new());
        assert_eq!(persisted, [("s".to_string(), 42)].into_iter().collect::<HashMap<_, _>>());
    }

    /// EXP-1059: one closed episode is put to the server once; the flag
    /// clears itself the moment the PR reads open again.
    #[test]
    fn the_final_pr_close_handled_flag_lives_for_one_closed_episode() {
        let dir = temp_dir("final-pr-close");
        let store = WorkflowStore::at(dir.0.join("s"));
        assert!(!final_pr_close_handled(&store, "wf", true));
        store
            .write("wf", &WorkflowState { final_pr_close_handled: true, ..WorkflowState::default() })
            .unwrap();
        assert!(final_pr_close_handled(&store, "wf", true));
        assert!(store.read("wf").final_pr_close_handled);
        assert!(!final_pr_close_handled(&store, "wf", false));
        assert!(!store.read("wf").final_pr_close_handled);
        assert!(!final_pr_close_handled(&store, "wf", true));
    }

    /// A corrupt document reads as a fresh one (a cache miss), and every field
    /// round-trips whole; a malformed half is dropped rather than losing the
    /// rest.
    #[test]
    fn the_whole_state_round_trips_and_malformed_halves_read_empty() {
        let dir = temp_dir("fields");
        let store = WorkflowStore::at(dir.0.join("s"));
        let mut mine = WorkflowState::default();
        mine.merged.insert(
            "node-1".to_string(),
            [("exp/EXP-1".to_string(), "sha-a2".to_string())].into(),
        );
        mine.woken.insert(
            "node-1".to_string(),
            [("exp/EXP-1".to_string(), "sha-a3".to_string())].into(),
        );
        mine.synthetic.insert(
            "exp/wf-abcdef12-base-EXP-3".to_string(),
            vec![("exp/EXP-1".to_string(), "sha-a2".to_string())],
        );
        mine.conflicts
            .insert(conflict_key(("node-1", "sha-a2"), ("node-2", "sha-b1")), true);
        mine.bases_deleted.insert("exp/wf-abcdef12-base-EXP-9".to_string());
        mine.tips_seen
            .insert("exp/EXP-1".to_string(), ("sha-a2".to_string(), 1_726_000_000_000));
        mine.review_runs.insert("node-1".to_string(), "sess-r1".to_string());
        mine.review_rounds.insert("node-1".to_string(), 1);
        mine.review_failures.insert("node-1".to_string(), 2);
        mine.fix_runs.insert(2, "sess-fix".to_string());
        mine.final_pr_close_handled = true;
        mine.land_refused.insert("node-1".to_string(), "sha-a2".to_string());
        mine.resuming.insert("sess-1".to_string(), 1_726_000_000_000);
        mine.nudged.insert(("sess-1".to_string(), "freeze".to_string()));
        store.write("wf-1", &mine).unwrap();
        assert_eq!(store.read("wf-1"), mine);
        assert_eq!(
            conflict_key(("node-1", "sha-a2"), ("node-2", "sha-b1")),
            conflict_key(("node-2", "sha-b1"), ("node-1", "sha-a2"))
        );

        std::fs::write(store.dir().join("wf-2.json"), "{not json").unwrap();
        assert_eq!(store.read("wf-2"), WorkflowState::default());
        let raw = serde_json::json!({
            "merged": { "node-1": "not-a-map" },
            "synthetic": { "base": [["only-one"]] },
            "conflicts": { "k": "not-a-bool" },
            "tipsSeen": { "b": ["sha"] },
            "fixRuns": { "x": "s" },
            "nudged": [["s-1", 100], ["broken"]],
        });
        std::fs::write(store.dir().join("wf-3.json"), raw.to_string()).unwrap();
        let read = store.read("wf-3");
        assert!(read.merged.is_empty());
        assert_eq!(read.synthetic["base"], Vec::new());
        assert!(read.conflicts.is_empty());
        assert!(read.tips_seen.is_empty());
        assert!(read.fix_runs.is_empty());
        assert_eq!(read.nudged, [("s-1".to_string(), "100".to_string())].into());
    }
}
