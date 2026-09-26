//! Automation state persistence (EXP-530): per-(device, action) firing
//! bookkeeping. EXP-1102: in a store of its OWN, `{data_dir}/automations/
//! <deviceId>.json` (one document per device id — the desktop app and the
//! CLI daemon share a data dir and have two ids), replaced by rename
//! ([`api::device_store::JsonStore`]); until then ONE foreign top-level key
//! of settings.json, the file a torn write of which cost workflow 2f353e88
//! its host:
//!
//! ```json
//! {
//!   "<actionId>": {
//!     "fingerprint": "…", "lastFiredAt": 123, "watermarkCreatedAt": 123,
//!     "watermarkId": "…", "cooldownUntil": 123,
//!     "seenFloor": 123, "seenIds": ["evt-…"]
//!   }
//! }
//! ```
//!
//! All stamps are ms epochs. [`write_states`] replaces the device's WHOLE
//! map, so deleted actions' entries prune themselves on the next write. A
//! pre-EXP-1102 install still carries its map under settings.json's
//! `actionAutomations.<deviceId>`: [`AutomationStore::migrate_legacy`] moves
//! it once and clears that device's sub-map.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The legacy settings.json top-level key (read for migration only).
pub const AUTOMATIONS_KEY: &str = "actionAutomations";

/// The store's directory under the data dir.
pub const AUTOMATIONS_STORE_DIR: &str = "automations";

/// The store: `{data_dir}/automations/`, one document per device id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationStore {
    inner: api::device_store::JsonStore,
    device_id: String,
}

impl AutomationStore {
    pub fn open(data_dir: &Path, device_id: &str) -> Self {
        Self::at(data_dir.join(AUTOMATIONS_STORE_DIR), device_id)
    }

    pub fn at(dir: impl Into<PathBuf>, device_id: &str) -> Self {
        Self {
            inner: api::device_store::JsonStore::new(dir),
            device_id: device_id.to_string(),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// EXP-1102: move this device's pre-store map out of settings.json — the
    /// document is written only when the store has none yet, and the legacy
    /// sub-map is cleared either way. Idempotent. `true` when a document was
    /// written.
    pub fn migrate_legacy(&self, settings_path: &Path) -> bool {
        let Some(data_dir) = settings_path.parent() else {
            return false;
        };
        let _guard = api::settings_lock::locked(data_dir);
        let Some(mut root) = read_root(settings_path) else {
            return false;
        };
        let Some(entries) = root
            .get(AUTOMATIONS_KEY)
            .and_then(|devices| devices.get(&self.device_id))
            .and_then(Value::as_object)
            .cloned()
        else {
            return false;
        };
        let mut written = false;
        if self.inner.read(&self.device_id).is_none() {
            let states = decode_map(&Value::Object(entries));
            match write_states(self, &states) {
                Ok(()) => written = true,
                Err(err) => {
                    log::warn!("[automations] migrating state failed: {err}");
                    return false;
                }
            }
        }
        if let Some(devices) = root.get_mut(AUTOMATIONS_KEY).and_then(Value::as_object_mut) {
            devices.remove(&self.device_id);
            if devices.is_empty() {
                root.as_object_mut().map(|object| object.remove(AUTOMATIONS_KEY));
            }
        }
        let mut rendered = serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
        rendered.push('\n');
        if let Err(err) = api::atomic_file::write_atomic(settings_path, &rendered) {
            log::warn!("[automations] clearing the legacy state key failed: {err}");
        }
        written
    }
}

/// One action's automation bookkeeping on one device.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutomationState {
    /// [`crate::automations::trigger_fingerprint`] of the trigger this
    /// state was seeded/advanced against — a mismatch re-seeds.
    pub fingerprint: String,
    /// Schedule anchor / reseed stamp (ms epoch).
    pub last_fired_at: Option<i64>,
    /// Event high-water mark: `(created_at_ms, id)` of the newest row seen.
    /// Still written on every seed/fire, but only LEGACY states (written
    /// before EXP-562) are still read from it — see `seen_floor`.
    pub watermark_created_at: Option<i64>,
    pub watermark_id: Option<String>,
    /// No firing before this stamp (cooldown / prepare-failure backoff).
    pub cooldown_until: Option<i64>,
    /// EXP-562 grace cursor: the oldest `created_at` (ms epoch) still
    /// eligible to fire. A bare watermark loses a row that COMMITS earlier
    /// but SYNCS later than its neighbour (overlapping transactions), so
    /// the cursor trails the snapshot max by
    /// [`crate::automations::EVENT_GRACE_MS`] and dedupes by id instead.
    /// `None` = a legacy state, still read strictly off the watermark; it
    /// migrates on the next fire.
    pub seen_floor: Option<i64>,
    /// The matching row ids at or above `seen_floor` that were already
    /// fired on — capped at [`crate::automations::SEEN_IDS_CAP`].
    pub seen_ids: Vec<String>,
}

/// Read the device's whole state map. Missing/corrupt document → empty —
/// the engine then re-seeds every trigger (never fires blind).
pub fn read_states(store: &AutomationStore) -> HashMap<String, AutomationState> {
    store
        .inner
        .read(&store.device_id)
        .map(|value| decode_map(&value))
        .unwrap_or_default()
}

fn decode_map(value: &Value) -> HashMap<String, AutomationState> {
    let Some(entries) = value.as_object() else {
        return HashMap::new();
    };
    entries
        .iter()
        .map(|(action_id, entry)| {
            (
                action_id.clone(),
                AutomationState {
                    fingerprint: entry
                        .get("fingerprint")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    last_fired_at: entry.get("lastFiredAt").and_then(Value::as_i64),
                    watermark_created_at: entry.get("watermarkCreatedAt").and_then(Value::as_i64),
                    watermark_id: entry
                        .get("watermarkId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    cooldown_until: entry.get("cooldownUntil").and_then(Value::as_i64),
                    seen_floor: entry.get("seenFloor").and_then(Value::as_i64),
                    // Missing / non-array / non-string entries → empty: a
                    // hole here can only re-fire a row, never lose one, and
                    // the floor still bounds the blast radius.
                    seen_ids: entry
                        .get("seenIds")
                        .and_then(Value::as_array)
                        .map(|ids| ids.iter().filter_map(Value::as_str).map(str::to_string).collect())
                        .unwrap_or_default(),
                },
            )
        })
        .collect()
}

/// Write the device's state map, replacing the document WHOLESALE —
/// entries for actions the caller no longer tracks are pruned.
pub fn write_states(
    store: &AutomationStore,
    states: &HashMap<String, AutomationState>,
) -> std::io::Result<()> {
    let mut entries = serde_json::Map::new();
    for (action_id, state) in states {
        entries.insert(
            action_id.clone(),
            serde_json::json!({
                "fingerprint": state.fingerprint,
                "lastFiredAt": state.last_fired_at,
                "watermarkCreatedAt": state.watermark_created_at,
                "watermarkId": state.watermark_id,
                "cooldownUntil": state.cooldown_until,
                "seenFloor": state.seen_floor,
                "seenIds": state.seen_ids,
            }),
        );
    }
    store.inner.write(&store.device_id, &Value::Object(entries))
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
        path.push(format!("exp-automations-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn state(fingerprint: &str, fired: i64) -> AutomationState {
        AutomationState {
            fingerprint: fingerprint.to_string(),
            last_fired_at: Some(fired),
            watermark_created_at: Some(fired),
            watermark_id: Some("evt-1".to_string()),
            cooldown_until: None,
            seen_floor: Some(fired),
            seen_ids: vec!["evt-1".to_string()],
        }
    }

    /// One document per device: a write replaces the device's map wholesale
    /// and never touches the sibling device's, and settings.json is never
    /// involved.
    #[test]
    fn states_round_trip_per_device_and_prune_wholesale() {
        let dir = temp_dir("roundtrip");
        let cli = AutomationStore::open(&dir.0, "cli-dev");
        let desk = AutomationStore::open(&dir.0, "desk-dev");
        let mine: HashMap<String, AutomationState> =
            [("act-1".to_string(), state("fp-1", 100))].into();
        write_states(&cli, &mine).unwrap();
        let sibling: HashMap<String, AutomationState> =
            [("act-2".to_string(), state("fp-2", 200))].into();
        write_states(&desk, &sibling).unwrap();
        assert_eq!(read_states(&cli), mine);
        assert_eq!(read_states(&desk), sibling);
        assert!(read_states(&AutomationStore::open(&dir.0, "unknown")).is_empty());
        let replaced: HashMap<String, AutomationState> =
            [("act-3".to_string(), state("fp-3", 300))].into();
        write_states(&cli, &replaced).unwrap();
        assert_eq!(read_states(&cli), replaced);
        assert_eq!(read_states(&desk), sibling);
        assert!(dir.0.join("automations").join("cli-dev.json").exists());
        assert!(!dir.0.join("settings.json").exists());
    }

    #[test]
    fn missing_or_corrupt_document_reads_as_empty() {
        let dir = temp_dir("corrupt");
        let store = AutomationStore::open(&dir.0, "d");
        assert!(read_states(&store).is_empty());
        std::fs::create_dir_all(dir.0.join("automations")).unwrap();
        std::fs::write(dir.0.join("automations").join("d.json"), "{not json").unwrap();
        assert!(read_states(&store).is_empty());
        let states: HashMap<String, AutomationState> =
            [("act-1".to_string(), state("fp", 1))].into();
        write_states(&store, &states).unwrap();
        assert_eq!(read_states(&store), states);
    }

    #[test]
    fn optional_stamps_survive_as_null() {
        let dir = temp_dir("nulls");
        let store = AutomationStore::open(&dir.0, "d");
        let bare: HashMap<String, AutomationState> = [(
            "act-1".to_string(),
            AutomationState {
                fingerprint: "fp".to_string(),
                ..Default::default()
            },
        )]
        .into();
        write_states(&store, &bare).unwrap();
        assert_eq!(read_states(&store), bare);
    }

    /// EXP-1102: the legacy settings.json map moves into the store once —
    /// this device's sub-map only — and the key is cleared behind it.
    #[test]
    fn legacy_settings_state_migrates_once() {
        let dir = temp_dir("migrate");
        let path = dir.0.join("settings.json");
        let raw = serde_json::json!({
            "deviceId": "desk-1",
            "actionAutomations": {
                "desk-1": { "act-1": { "fingerprint": "fp-1", "lastFiredAt": 100, "seenIds": ["e"] } },
                "cli-1": { "act-9": { "fingerprint": "fp-9" } }
            }
        });
        std::fs::write(&path, raw.to_string()).unwrap();
        let store = AutomationStore::open(&dir.0, "desk-1");
        assert!(store.migrate_legacy(&path));
        let states = read_states(&store);
        assert_eq!(states["act-1"].fingerprint, "fp-1");
        assert_eq!(states["act-1"].last_fired_at, Some(100));
        assert_eq!(states["act-1"].seen_ids, ["e"]);
        let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["deviceId"], "desk-1");
        assert!(root["actionAutomations"].get("desk-1").is_none());
        assert_eq!(root["actionAutomations"]["cli-1"]["act-9"]["fingerprint"], "fp-9");
        assert!(!store.migrate_legacy(&path), "nothing left");
        // The sibling's turn clears the key entirely.
        assert!(AutomationStore::open(&dir.0, "cli-1").migrate_legacy(&path));
        let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(root.get("actionAutomations").is_none());
    }
}
