//! EXP-982 — the workflow engine's host bookkeeping, persisted exactly like
//! [`crate::automations::state`]: ONE foreign top-level key in settings.json
//! the desktop app and the CLI daemon share (two device ids, two sub-maps):
//!
//! ```json
//! "workflowEngine": {
//!   "<deviceId>": {
//!     "<workflowId>": {
//!       "nudged": ["<sessionId><resetsAtMs>"],
//!       "branchDeleted": true,
//!       "propagated": { "<nodeId>": { "<branch>": "<sha>" } },
//!       "synthetic": { "<baseBranch>": [["<branch>", "<sha>"]] },
//!       "conflicts": { "<nodeA>|<nodeB>|<shaA>|<shaB>": true },
//!       "basesDeleted": ["<baseBranch>"],
//!       "reviewedHead": { "<nodeId>": "<sha>" },
//!       "findingsSent": { "<nodeId>": 2 },
//!       "reviewRuns": { "<nodeId>": "<sessionId>" },
//!       "checkpointTips": { "<nodeId>": "<sha>" }
//!     }
//!   }
//! }
//! ```
//!
//! The in-flight sets are NOT here: they live for one host process and must
//! not survive a crash (a restart re-evaluates and re-decides from the
//! synced rows, which is the whole point of a level-triggered engine). What
//! persists is only what must happen AT MOST ONCE across restarts: a nudge
//! already sent, a branch already deleted, a movement already announced
//! (EXP-983 `propagated`) and what a synthetic base was last built from
//! (`synthetic`). `conflicts` is a pure CACHE of merge-tree verdicts per tip
//! pair — losing it only costs one git call.
//!
//! `Settings::save`'s merge-preserve keeps the key; it must never enter
//! `DEAD_KEYS`. [`write_states`] replaces the device's WHOLE map, so deleted
//! workflows' entries prune themselves on the next write.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

/// The settings.json top-level key holding all per-device engine state.
pub const WORKFLOW_ENGINE_KEY: &str = "workflowEngine";

/// The separator inside a persisted nudge key — the unit separator, which
/// cannot occur in a session uuid or a number.
const NUDGE_SEP: char = '\u{1f}';

/// One workflow's bookkeeping on one device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkflowState {
    /// `(session id, resets_at_ms)` pairs already nudged.
    pub nudged: HashSet<(String, i64)>,
    /// The cancel sweep already dropped the integration branch.
    pub branch_deleted: bool,
    /// EXP-983: `node id → branch → the tip that node was last TOLD to
    /// merge`, so one movement is announced exactly once.
    pub propagated: HashMap<String, HashMap<String, String>>,
    /// EXP-983: `synthetic base → the (branch, sha) pairs it was built
    /// from`. A difference is what makes the next pass refresh it.
    pub synthetic: HashMap<String, Vec<(String, String)>>,
    /// EXP-983: cached `git merge-tree` verdicts, keyed by the two nodes and
    /// the two tips they were tested at.
    pub conflicts: HashMap<String, bool>,
    /// EXP-983: synthetic bases this device already dropped.
    pub bases_deleted: HashSet<String>,
    /// EXP-984: `node id → the branch tip its last agent review ran
    /// against`, so one push is reviewed once however many beats pass.
    pub reviewed_head: HashMap<String, String>,
    /// EXP-984: `node id → the highest review round whose findings were
    /// delivered to its author` — said once per round, never per beat.
    pub findings_sent: HashMap<String, i64>,
    /// EXP-984: `node id → the session id of its reviewer run`, so a live
    /// review is never started twice (its liveness comes off the synced
    /// `coding_sessions` row).
    pub review_runs: HashMap<String, String>,
    /// EXP-984: `node id → the branch tip last counted as a contract
    /// change`, the input to the `contractChanges` metric.
    pub checkpoint_tips: HashMap<String, String>,
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

/// Read `device_id`'s whole state map. Missing/corrupt file or key → empty,
/// which can only re-send a nudge, never skip one.
pub fn read_states(settings_path: &Path, device_id: &str) -> HashMap<String, WorkflowState> {
    let Some(root) = read_root(settings_path) else {
        return HashMap::new();
    };
    let Some(entries) = root
        .get(WORKFLOW_ENGINE_KEY)
        .and_then(|devices| devices.get(device_id))
        .and_then(Value::as_object)
    else {
        return HashMap::new();
    };
    entries
        .iter()
        .map(|(workflow_id, entry)| {
            (
                workflow_id.clone(),
                WorkflowState {
                    nudged: entry
                        .get("nudged")
                        .and_then(Value::as_array)
                        .map(|keys| {
                            keys.iter().filter_map(Value::as_str).filter_map(parse_nudge).collect()
                        })
                        .unwrap_or_default(),
                    branch_deleted: entry
                        .get("branchDeleted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    propagated: read_propagated(entry.get("propagated")),
                    synthetic: read_synthetic(entry.get("synthetic")),
                    conflicts: entry
                        .get("conflicts")
                        .and_then(Value::as_object)
                        .map(|rows| {
                            rows.iter()
                                .filter_map(|(key, value)| {
                                    Some((key.clone(), value.as_bool()?))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    bases_deleted: entry
                        .get("basesDeleted")
                        .and_then(Value::as_array)
                        .map(|keys| {
                            keys.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    reviewed_head: read_string_map(entry.get("reviewedHead")),
                    findings_sent: read_round_map(entry.get("findingsSent")),
                    review_runs: read_string_map(entry.get("reviewRuns")),
                    checkpoint_tips: read_string_map(entry.get("checkpointTips")),
                },
            )
        })
        .collect()
}

/// `node id → branch → sha`; anything malformed is dropped rather than
/// poisoning the map (the worst case is one repeated instruction).
fn read_propagated(value: Option<&Value>) -> HashMap<String, HashMap<String, String>> {
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

/// EXP-984: a flat `key → string` map (`reviewedHead`, `reviewRuns`,
/// `checkpointTips`); anything malformed is dropped, which at worst re-runs
/// one review or re-counts one metric.
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

/// EXP-984: `node id → round`.
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

/// Write `device_id`'s state map, read-modify-write on the raw JSON so every
/// other top-level key (and every sibling device's map) survives.
pub fn write_states(
    settings_path: &Path,
    device_id: &str,
    states: &HashMap<String, WorkflowState>,
) -> std::io::Result<()> {
    if let Some(dir) = settings_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut root = read_root(settings_path).unwrap_or_else(|| Value::Object(Default::default()));
    let mut entries = serde_json::Map::new();
    for (workflow_id, state) in states {
        // Sorted: settings.json is a file a person reads, and a set's
        // iteration order would churn it on every write.
        let mut nudged: Vec<String> = state
            .nudged
            .iter()
            .map(|(session_id, resets_at)| format!("{session_id}{NUDGE_SEP}{resets_at}"))
            .collect();
        nudged.sort();
        let mut bases_deleted: Vec<String> = state.bases_deleted.iter().cloned().collect();
        bases_deleted.sort();
        entries.insert(
            workflow_id.clone(),
            serde_json::json!({
                "nudged": nudged,
                "branchDeleted": state.branch_deleted,
                "propagated": state.propagated,
                "synthetic": state.synthetic,
                "conflicts": state.conflicts,
                "basesDeleted": bases_deleted,
                "reviewedHead": state.reviewed_head,
                "findingsSent": state.findings_sent,
                "reviewRuns": state.review_runs,
                "checkpointTips": state.checkpoint_tips,
            }),
        );
    }
    if let Some(object) = root.as_object_mut() {
        let devices = object
            .entry(WORKFLOW_ENGINE_KEY.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        if !devices.is_object() {
            *devices = Value::Object(Default::default());
        }
        if let Some(devices) = devices.as_object_mut() {
            devices.insert(device_id.to_string(), Value::Object(entries));
        }
    }
    let mut rendered = serde_json::to_string_pretty(&root).expect("render settings json");
    rendered.push('\n');
    std::fs::write(settings_path, rendered)
}

fn parse_nudge(raw: &str) -> Option<(String, i64)> {
    let (session_id, resets_at) = raw.split_once(NUDGE_SEP)?;
    Some((session_id.to_string(), resets_at.parse().ok()?))
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
    use crate::settings::Settings;
    use std::path::PathBuf;

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

    fn state(session_id: &str, resets_at: i64, deleted: bool) -> WorkflowState {
        WorkflowState {
            nudged: [(session_id.to_string(), resets_at)].into_iter().collect(),
            branch_deleted: deleted,
            ..WorkflowState::default()
        }
    }

    #[test]
    fn states_round_trip_and_preserve_siblings() {
        let dir = temp_dir("roundtrip");
        let path = dir.0.join("settings.json");
        std::fs::write(&path, r#"{"deviceId":"desk-1","claudeModel":"sonnet"}"#).unwrap();

        let mine: HashMap<String, WorkflowState> =
            [("wf-1".to_string(), state("s-1", 100, false))].into();
        write_states(&path, "cli-dev", &mine).unwrap();
        let sibling: HashMap<String, WorkflowState> =
            [("wf-2".to_string(), state("s-2", 200, true))].into();
        write_states(&path, "desk-dev", &sibling).unwrap();

        assert_eq!(read_states(&path, "cli-dev"), mine);
        assert_eq!(read_states(&path, "desk-dev"), sibling);
        assert!(read_states(&path, "unknown").is_empty());

        // A foreign key to the merge-save, like the automations map: a
        // Settings::save must leave it alone.
        let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["deviceId"], "desk-1");
        Settings::default().save(&path).unwrap();
        assert_eq!(read_states(&path, "cli-dev"), mine, "Settings::save preserves the states");

        // A rewrite replaces the device's map wholesale — a deleted
        // workflow's entry prunes itself; the sibling device is untouched.
        let replaced: HashMap<String, WorkflowState> =
            [("wf-3".to_string(), state("s-3", 300, false))].into();
        write_states(&path, "cli-dev", &replaced).unwrap();
        assert_eq!(read_states(&path, "cli-dev"), replaced);
        assert_eq!(read_states(&path, "desk-dev"), sibling);
    }

    #[test]
    fn missing_or_corrupt_file_reads_as_empty() {
        let dir = temp_dir("corrupt");
        let path = dir.0.join("settings.json");
        assert!(read_states(&path, "d").is_empty());
        std::fs::write(&path, "{not json").unwrap();
        assert!(read_states(&path, "d").is_empty());
        let states: HashMap<String, WorkflowState> =
            [("wf-1".to_string(), state("s-1", 1, false))].into();
        write_states(&path, "d", &states).unwrap();
        assert_eq!(read_states(&path, "d"), states);
    }

    /// EXP-983 — the speculative bookkeeping round-trips whole, and a
    /// malformed half is dropped rather than losing the rest.
    #[test]
    fn the_speculative_state_round_trips() {
        let dir = temp_dir("speculative");
        let path = dir.0.join("settings.json");
        let mut mine = WorkflowState::default();
        mine.propagated.insert(
            "node-1".to_string(),
            [("exp/EXP-1".to_string(), "sha-a2".to_string())].into(),
        );
        mine.synthetic.insert(
            "exp/wf-abcdef12-base-EXP-3".to_string(),
            vec![
                ("exp/EXP-1".to_string(), "sha-a2".to_string()),
                ("exp/wf-abcdef12".to_string(), "sha-i1".to_string()),
            ],
        );
        mine.conflicts.insert(
            conflict_key(("node-1", "sha-a2"), ("node-2", "sha-b1")),
            true,
        );
        mine.bases_deleted
            .insert("exp/wf-abcdef12-base-EXP-9".to_string());
        // EXP-984: the review gate's at-most-once bookkeeping.
        mine.reviewed_head
            .insert("node-1".to_string(), "sha-a2".to_string());
        mine.findings_sent.insert("node-1".to_string(), 2);
        mine.review_runs
            .insert("node-1".to_string(), "sess-r1".to_string());
        mine.checkpoint_tips
            .insert("node-1".to_string(), "sha-a2".to_string());
        let states: HashMap<String, WorkflowState> = [("wf-1".to_string(), mine.clone())].into();
        write_states(&path, "d", &states).unwrap();
        assert_eq!(read_states(&path, "d")["wf-1"], mine);

        // The pair's key folds both orders into one verdict.
        assert_eq!(
            conflict_key(("node-1", "sha-a2"), ("node-2", "sha-b1")),
            conflict_key(("node-2", "sha-b1"), ("node-1", "sha-a2"))
        );

        // A half that is not shaped the way it was written simply reads
        // empty: the engine then re-announces or re-tests, never crashes.
        let raw = serde_json::json!({
            "workflowEngine": { "d": { "wf-1": {
                "propagated": { "node-1": "not-a-map" },
                "synthetic": { "base": [["only-one"]] },
                "conflicts": { "k": "not-a-bool" },
                "reviewedHead": { "node-1": 7 },
                "findingsSent": { "node-1": "two" },
            } } }
        });
        std::fs::write(&path, raw.to_string()).unwrap();
        let read = read_states(&path, "d");
        assert!(read["wf-1"].propagated.is_empty());
        assert_eq!(read["wf-1"].synthetic["base"], Vec::new());
        assert!(read["wf-1"].conflicts.is_empty());
        assert!(read["wf-1"].reviewed_head.is_empty());
        assert!(read["wf-1"].findings_sent.is_empty());
    }

    /// A malformed nudge key is dropped rather than poisoning the set: the
    /// worst case is one extra nudge, and the best is not losing the rest.
    #[test]
    fn a_malformed_nudge_key_is_dropped() {
        let dir = temp_dir("malformed");
        let path = dir.0.join("settings.json");
        // Written through serde so the control separator is escaped the
        // way a real settings.json carries it.
        let raw = serde_json::json!({
            "workflowEngine": {
                "d": {
                    "wf-1": {
                        "nudged": [
                            format!("s-1{NUDGE_SEP}100"),
                            "broken".to_string(),
                            format!("s-2{NUDGE_SEP}nope"),
                        ],
                        "branchDeleted": false,
                    }
                }
            }
        });
        std::fs::write(&path, raw.to_string()).unwrap();
        let states = read_states(&path, "d");
        assert_eq!(states["wf-1"].nudged, [("s-1".to_string(), 100)].into());
    }
}
