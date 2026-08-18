//! Automation state persistence (EXP-530): per-(device, action) firing
//! bookkeeping in settings.json, the [`crate::launch_defaults_sync`]
//! read-modify-write idiom — ONE foreign top-level key the desktop app and
//! the CLI daemon share (two device ids, two sub-maps):
//!
//! ```json
//! "actionAutomations": {
//!   "<deviceId>": {
//!     "<actionId>": {
//!       "fingerprint": "…", "lastFiredAt": 123, "watermarkCreatedAt": 123,
//!       "watermarkId": "…", "cooldownUntil": 123
//!     }
//!   }
//! }
//! ```
//!
//! All stamps are ms epochs. `Settings::save`'s merge-preserve keeps the
//! key; it must never enter `DEAD_KEYS`. [`write_states`] replaces the
//! device's WHOLE map, so deleted actions' entries prune themselves on the
//! next write.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

/// The settings.json top-level key holding all per-device automation state.
pub const AUTOMATIONS_KEY: &str = "actionAutomations";

/// One action's automation bookkeeping on one device.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutomationState {
    /// [`crate::automations::trigger_fingerprint`] of the trigger this
    /// state was seeded/advanced against — a mismatch re-seeds.
    pub fingerprint: String,
    /// Schedule anchor / reseed stamp (ms epoch).
    pub last_fired_at: Option<i64>,
    /// Event high-water mark: `(created_at_ms, id)` of the newest row seen.
    pub watermark_created_at: Option<i64>,
    pub watermark_id: Option<String>,
    /// No firing before this stamp (cooldown / prepare-failure backoff).
    pub cooldown_until: Option<i64>,
}

/// Read `device_id`'s whole state map. Missing/corrupt file or key →
/// empty — the engine then re-seeds every trigger (never fires blind).
pub fn read_states(settings_path: &Path, device_id: &str) -> HashMap<String, AutomationState> {
    let Some(root) = read_root(settings_path) else {
        return HashMap::new();
    };
    let Some(entries) = root
        .get(AUTOMATIONS_KEY)
        .and_then(|devices| devices.get(device_id))
        .and_then(Value::as_object)
    else {
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
                },
            )
        })
        .collect()
}

/// Write `device_id`'s state map, read-modify-write on the raw JSON so
/// every other top-level key (and every sibling device's map) survives.
/// Replaces the device's map WHOLESALE — entries for actions the caller no
/// longer tracks are pruned.
pub fn write_states(
    settings_path: &Path,
    device_id: &str,
    states: &HashMap<String, AutomationState>,
) -> std::io::Result<()> {
    if let Some(dir) = settings_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut root = read_root(settings_path)
        .unwrap_or_else(|| Value::Object(Default::default()));
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
            }),
        );
    }
    if let Some(object) = root.as_object_mut() {
        let devices = object
            .entry(AUTOMATIONS_KEY.to_string())
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
        }
    }

    #[test]
    fn states_round_trip_and_preserve_siblings() {
        let dir = temp_dir("roundtrip");
        let path = dir.0.join("settings.json");
        std::fs::write(&path, r#"{"deviceId":"desk-1","claudeModel":"sonnet"}"#).unwrap();

        let mine: HashMap<String, AutomationState> =
            [("act-1".to_string(), state("fp-1", 100))].into();
        write_states(&path, "cli-dev", &mine).unwrap();
        let sibling: HashMap<String, AutomationState> =
            [("act-2".to_string(), state("fp-2", 200))].into();
        write_states(&path, "desk-dev", &sibling).unwrap();

        assert_eq!(read_states(&path, "cli-dev"), mine);
        assert_eq!(read_states(&path, "desk-dev"), sibling);
        assert!(read_states(&path, "unknown").is_empty());

        // Foreign keys and a Settings::save survive around the states
        // (AUTOMATIONS_KEY is a foreign key to the merge-save, like the
        // launchDefaultsSync marker).
        let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["deviceId"], "desk-1");
        Settings::default().save(&path).unwrap();
        assert_eq!(read_states(&path, "cli-dev"), mine, "Settings::save preserves the states");

        // A rewrite replaces the device's map wholesale — the stale action
        // entry prunes itself; the sibling device is untouched.
        let replaced: HashMap<String, AutomationState> =
            [("act-3".to_string(), state("fp-3", 300))].into();
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
        // Writing heals the file.
        let states: HashMap<String, AutomationState> =
            [("act-1".to_string(), state("fp", 1))].into();
        write_states(&path, "d", &states).unwrap();
        assert_eq!(read_states(&path, "d"), states);
    }

    #[test]
    fn optional_stamps_survive_as_null() {
        let dir = temp_dir("nulls");
        let path = dir.0.join("settings.json");
        let bare: HashMap<String, AutomationState> = [(
            "act-1".to_string(),
            AutomationState {
                fingerprint: "fp".to_string(),
                ..Default::default()
            },
        )]
        .into();
        write_states(&path, "d", &bare).unwrap();
        assert_eq!(read_states(&path, "d"), bare);
    }
}
