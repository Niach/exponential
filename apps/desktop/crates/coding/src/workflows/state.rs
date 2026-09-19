//! EXP-982 — the workflow engine's host bookkeeping, persisted exactly like
//! [`crate::automations::state`]: ONE foreign top-level key in settings.json
//! the desktop app and the CLI daemon share (two device ids, two sub-maps):
//!
//! ```json
//! "workflowEngine": {
//!   "<deviceId>": {
//!     "<workflowId>": {
//!       "nudged": ["<sessionId><resetsAtMs>"],
//!       "branchDeleted": true
//!     }
//!   }
//! }
//! ```
//!
//! The in-flight sets are NOT here: they live for one host process and must
//! not survive a crash (a restart re-evaluates and re-decides from the
//! synced rows, which is the whole point of a level-triggered engine). What
//! persists is only what must happen AT MOST ONCE across restarts: a nudge
//! already sent, and a branch already deleted.
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
                },
            )
        })
        .collect()
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
        entries.insert(
            workflow_id.clone(),
            serde_json::json!({
                "nudged": nudged,
                "branchDeleted": state.branch_deleted,
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
