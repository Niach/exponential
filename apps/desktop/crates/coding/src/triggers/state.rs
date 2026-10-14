//! Trigger state persistence (EXP-530): per-(device, trigger) firing
//! bookkeeping. EXP-1102: in a store of its OWN, `{data_dir}/triggers/
//! <deviceId>.json` (one document per device id — the desktop app and the
//! CLI daemon share a data dir and have two ids), replaced by rename
//! ([`api::device_store::JsonStore`]):
//!
//! ```json
//! {
//!   "<triggerId>": {
//!     "fingerprint": "…", "lastFiredAt": 123, "watermarkCreatedAt": 123,
//!     "watermarkId": "…", "cooldownUntil": 123,
//!     "seenFloor": 123, "seenIds": ["evt-…"]
//!   }
//! }
//! ```
//!
//! All stamps are ms epochs. [`write_states`] replaces the device's WHOLE
//! map, so deleted triggers' entries prune themselves on the next write.
//! EXP-1292: the settings.json (`actionAutomations`) and `automations/`
//! migrations are gone — action-trigger states key ids no agent trigger
//! carries, so there is nothing left to carry over.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The store's directory under the data dir.
pub const TRIGGERS_STORE_DIR: &str = "triggers";

/// The store: `{data_dir}/triggers/`, one document per device id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerStore {
    inner: api::device_store::JsonStore,
    device_id: String,
}

impl TriggerStore {
    pub fn open(data_dir: &Path, device_id: &str) -> Self {
        Self::at(data_dir.join(TRIGGERS_STORE_DIR), device_id)
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
}

/// One trigger's bookkeeping on one device.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriggerState {
    /// [`crate::triggers::trigger_fingerprint`] of the trigger this
    /// state was seeded/advanced against — a mismatch re-seeds.
    pub fingerprint: String,
    /// Schedule anchor / reseed stamp (ms epoch).
    pub last_fired_at: Option<i64>,
    /// Event high-water mark: `(created_at_ms, id)` of the newest row seen.
    /// Still written on every seed/fire, but only LEGACY states (written
    /// before EXP-562) are still read from it — see `seen_floor`.
    pub watermark_created_at: Option<i64>,
    pub watermark_id: Option<String>,
    /// No firing before this stamp (cooldown / send-failure backoff).
    pub cooldown_until: Option<i64>,
    /// EXP-562 grace cursor: the oldest `created_at` (ms epoch) still
    /// eligible to fire. A bare watermark loses a row that COMMITS earlier
    /// but SYNCS later than its neighbour (overlapping transactions), so
    /// the cursor trails the snapshot max by
    /// [`crate::triggers::EVENT_GRACE_MS`] and dedupes by id instead.
    /// `None` = a legacy state, still read strictly off the watermark; it
    /// migrates on the next fire.
    pub seen_floor: Option<i64>,
    /// The matching row ids at or above `seen_floor` that were already
    /// fired on — capped at [`crate::triggers::SEEN_IDS_CAP`].
    pub seen_ids: Vec<String>,
}

/// Read the device's whole state map. Missing/corrupt document → empty —
/// the engine then re-seeds every trigger (never fires blind).
pub fn read_states(store: &TriggerStore) -> HashMap<String, TriggerState> {
    store
        .inner
        .read(&store.device_id)
        .map(|value| decode_map(&value))
        .unwrap_or_default()
}

fn decode_map(value: &Value) -> HashMap<String, TriggerState> {
    let Some(entries) = value.as_object() else {
        return HashMap::new();
    };
    entries
        .iter()
        .map(|(trigger_id, entry)| {
            (
                trigger_id.clone(),
                TriggerState {
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
/// entries for triggers the caller no longer tracks are pruned.
pub fn write_states(
    store: &TriggerStore,
    states: &HashMap<String, TriggerState>,
) -> std::io::Result<()> {
    let mut entries = serde_json::Map::new();
    for (trigger_id, state) in states {
        entries.insert(
            trigger_id.clone(),
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
        path.push(format!("exp-triggers-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn state(fingerprint: &str, fired: i64) -> TriggerState {
        TriggerState {
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
        let cli = TriggerStore::open(&dir.0, "cli-dev");
        let desk = TriggerStore::open(&dir.0, "desk-dev");
        let mine: HashMap<String, TriggerState> =
            [("trg-1".to_string(), state("fp-1", 100))].into();
        write_states(&cli, &mine).unwrap();
        let sibling: HashMap<String, TriggerState> =
            [("trg-2".to_string(), state("fp-2", 200))].into();
        write_states(&desk, &sibling).unwrap();
        assert_eq!(read_states(&cli), mine);
        assert_eq!(read_states(&desk), sibling);
        assert!(read_states(&TriggerStore::open(&dir.0, "unknown")).is_empty());
        let replaced: HashMap<String, TriggerState> =
            [("trg-3".to_string(), state("fp-3", 300))].into();
        write_states(&cli, &replaced).unwrap();
        assert_eq!(read_states(&cli), replaced);
        assert_eq!(read_states(&desk), sibling);
        assert!(dir.0.join("triggers").join("cli-dev.json").exists());
        assert!(!dir.0.join("settings.json").exists());
    }

    #[test]
    fn missing_or_corrupt_document_reads_as_empty() {
        let dir = temp_dir("corrupt");
        let store = TriggerStore::open(&dir.0, "d");
        assert!(read_states(&store).is_empty());
        std::fs::create_dir_all(dir.0.join("triggers")).unwrap();
        std::fs::write(dir.0.join("triggers").join("d.json"), "{not json").unwrap();
        assert!(read_states(&store).is_empty());
        let states: HashMap<String, TriggerState> =
            [("trg-1".to_string(), state("fp", 1))].into();
        write_states(&store, &states).unwrap();
        assert_eq!(read_states(&store), states);
    }

    #[test]
    fn optional_stamps_survive_as_null() {
        let dir = temp_dir("nulls");
        let store = TriggerStore::open(&dir.0, "d");
        let bare: HashMap<String, TriggerState> = [(
            "trg-1".to_string(),
            TriggerState {
                fingerprint: "fp".to_string(),
                ..Default::default()
            },
        )]
        .into();
        write_states(&store, &bare).unwrap();
        assert_eq!(read_states(&store), bare);
    }
}
