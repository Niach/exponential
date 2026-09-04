//! EXP-481: convergence bookkeeping between the SERVER-AUTHORITATIVE
//! `devices.launch_defaults` column and the local settings.json cache.
//!
//! The marker lives in settings.json as ONE foreign top-level key, keyed
//! per DEVICE id — the desktop app and the CLI daemon share the file but
//! are two device rows (`deviceId` vs `cliDeviceId`), each converging to
//! its own server copy:
//!
//! ```json
//! "launchDefaultsSync": { "<deviceId>": { "syncedAt": "…", "dirty": false, "hash": "…" } }
//! ```
//!
//! `Settings::save`'s merge-preserve keeps the key; it must never enter
//! `DEAD_KEYS`. `syncedAt` echoes the server's `launchDefaultsUpdatedAt`
//! stamp VERBATIM (equality compare, no clock semantics); `hash` is the
//! local fingerprint at the last convergence, so a hand-edit (or an IDE
//! save) is detectable as `fingerprint != hash`; `dirty` queues a push that
//! failed (offline) for the next heartbeat.
//!
//! Race outcomes (deterministic, documented): a device push always carries
//! `expectedUpdatedAt` — the server refuses stale stamps with the current
//! copy, which the device ADOPTS (server wins offline-concurrent races). A
//! device that never synced against a server that has defaults applies the
//! server copy, never blind-pushes over a web edit.

use std::path::Path;

use serde_json::Value;

use crate::remote_admin::defaults_wire;
use crate::settings::Settings;

/// The settings.json top-level key holding all per-device markers.
pub const SYNC_MARKER_KEY: &str = "launchDefaultsSync";

/// One device's convergence marker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncMarker {
    /// The server stamp last converged to; `None` = never synced.
    pub synced_at: Option<String>,
    /// A local edit failed to push (offline) — retry on the next beat.
    pub dirty: bool,
    /// [`defaults_fingerprint`] at the last convergence.
    pub hash: Option<String>,
}

/// What the reconcile decided (pure — unit-tested like the daemon's
/// `advert_transition`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileAction {
    /// Adopt the server copy (apply + save + re-mark).
    ApplyServer,
    /// Push the local values up (with `expectedUpdatedAt` = the marker's
    /// stamp; a CONFLICT answer means adopt the returned server copy).
    PushLocal,
    Noop,
}

/// Decide between the server copy and the local file.
///
/// * Never synced (`marker.synced_at` is `None`): the server copy wins when
///   one exists (a fresh device must not stomp web edits); otherwise the
///   local values SEED the server.
/// * A queued (`dirty`) or detected (`file_hash != marker.hash`) local edit
///   pushes — the CAS resolves it against a concurrent server edit.
/// * A moved server stamp applies.
pub fn reconcile(
    marker: &SyncMarker,
    file_hash: &str,
    server_updated_at: Option<&str>,
    server_has_defaults: bool,
) -> ReconcileAction {
    if marker.synced_at.is_none() {
        return if server_has_defaults {
            ReconcileAction::ApplyServer
        } else {
            ReconcileAction::PushLocal
        };
    }
    if marker.dirty || marker.hash.as_deref() != Some(file_hash) {
        return ReconcileAction::PushLocal;
    }
    if server_updated_at != marker.synced_at.as_deref() {
        return ReconcileAction::ApplyServer;
    }
    ReconcileAction::Noop
}

/// Stable fingerprint over ONLY the launch-default fields (the
/// [`defaults_wire`] projection, canonical-JSON hashed via the std
/// `DefaultHasher`). Process-stable; a hasher change across Rust releases
/// costs at worst one spurious (CAS-guarded, value-identical) push.
pub fn defaults_fingerprint(settings: &Settings) -> String {
    use std::hash::{Hash, Hasher};
    let canonical = serde_json::to_string(&defaults_wire(settings))
        .expect("defaults serialize cannot fail");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Read `device_id`'s marker from the settings file. Missing/corrupt →
/// default (never synced) — the reconcile then behaves like a fresh device.
pub fn read_marker(settings_path: &Path, device_id: &str) -> SyncMarker {
    let Some(root) = read_root(settings_path) else {
        return SyncMarker::default();
    };
    let Some(entry) = root
        .get(SYNC_MARKER_KEY)
        .and_then(|markers| markers.get(device_id))
    else {
        return SyncMarker::default();
    };
    SyncMarker {
        synced_at: entry
            .get("syncedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        dirty: entry.get("dirty").and_then(Value::as_bool).unwrap_or(false),
        hash: entry
            .get("hash")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Write `device_id`'s marker, read-modify-write on the raw JSON so every
/// other top-level key (and every sibling device's marker) survives.
pub fn write_marker(
    settings_path: &Path,
    device_id: &str,
    marker: &SyncMarker,
) -> std::io::Result<()> {
    if let Some(dir) = settings_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut root = read_root(settings_path)
        .unwrap_or_else(|| Value::Object(Default::default()));
    let entry = serde_json::json!({
        "syncedAt": marker.synced_at,
        "dirty": marker.dirty,
        "hash": marker.hash,
    });
    if let Some(object) = root.as_object_mut() {
        let markers = object
            .entry(SYNC_MARKER_KEY.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        if !markers.is_object() {
            *markers = Value::Object(Default::default());
        }
        if let Some(markers) = markers.as_object_mut() {
            markers.insert(device_id.to_string(), entry);
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
        path.push(format!("exp-defaults-sync-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn marker(synced_at: Option<&str>, dirty: bool, hash: Option<&str>) -> SyncMarker {
        SyncMarker {
            synced_at: synced_at.map(str::to_string),
            dirty,
            hash: hash.map(str::to_string),
        }
    }

    /// The reconcile truth table (the advert_transition idiom).
    #[test]
    fn reconcile_truth_table() {
        // Never synced: server copy wins when present, else seed-push.
        assert_eq!(
            reconcile(&marker(None, false, None), "fp", Some("t1"), true),
            ReconcileAction::ApplyServer
        );
        assert_eq!(
            reconcile(&marker(None, true, Some("old")), "fp", Some("t1"), true),
            ReconcileAction::ApplyServer,
            "a fresh device must never blind-push over a web edit"
        );
        assert_eq!(
            reconcile(&marker(None, false, None), "fp", None, false),
            ReconcileAction::PushLocal
        );

        // Queued/detected local edits push (CAS resolves races).
        assert_eq!(
            reconcile(&marker(Some("t1"), true, Some("fp")), "fp", Some("t1"), true),
            ReconcileAction::PushLocal
        );
        assert_eq!(
            reconcile(&marker(Some("t1"), false, Some("old")), "fp", Some("t1"), true),
            ReconcileAction::PushLocal,
            "hand-edited file"
        );
        // Local edit + server edit at once: still PushLocal — the CAS
        // conflict answer carries the server copy to adopt.
        assert_eq!(
            reconcile(&marker(Some("t1"), false, Some("old")), "fp", Some("t2"), true),
            ReconcileAction::PushLocal
        );

        // A moved server stamp applies; steady state noops.
        assert_eq!(
            reconcile(&marker(Some("t1"), false, Some("fp")), "fp", Some("t2"), true),
            ReconcileAction::ApplyServer
        );
        assert_eq!(
            reconcile(&marker(Some("t1"), false, Some("fp")), "fp", None, false),
            ReconcileAction::ApplyServer,
            "server copy vanished (row recreated) — re-seed via apply/push cycle"
        );
        assert_eq!(
            reconcile(&marker(Some("t1"), false, Some("fp")), "fp", Some("t1"), true),
            ReconcileAction::Noop
        );
    }

    #[test]
    fn fingerprint_tracks_launch_default_fields_only() {
        let mut settings = Settings::default();
        let base = defaults_fingerprint(&settings);
        assert_eq!(base, defaults_fingerprint(&settings), "stable");
        // A non-launcher field must not move it.
        settings.changelog_seen_id = Some("x".into());
        settings.terminal_shell = Some("/bin/fish".into());
        assert_eq!(base, defaults_fingerprint(&settings));
        // A launch-default field must.
        settings.claude_model = "opus".into();
        assert_ne!(base, defaults_fingerprint(&settings));
    }

    #[test]
    fn marker_round_trips_and_preserves_siblings() {
        let dir = temp_dir("marker");
        let path = dir.0.join("settings.json");
        std::fs::write(&path, r#"{"deviceId":"desk-1","claudeModel":"sonnet"}"#).unwrap();

        let mine = marker(Some("2026-08-11T10:00:00.000Z"), false, Some("abc"));
        write_marker(&path, "cli-dev", &mine).unwrap();
        let sibling = marker(None, true, None);
        write_marker(&path, "desk-dev", &sibling).unwrap();

        assert_eq!(read_marker(&path, "cli-dev"), mine);
        assert_eq!(read_marker(&path, "desk-dev"), sibling);
        assert_eq!(read_marker(&path, "unknown"), SyncMarker::default());

        // Foreign keys and a Settings::save survive around the markers.
        let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["deviceId"], "desk-1");
        Settings::default().save(&path).unwrap();
        assert_eq!(read_marker(&path, "cli-dev"), mine, "Settings::save preserves the marker");
    }

    #[test]
    fn missing_or_corrupt_file_reads_as_never_synced() {
        let dir = temp_dir("corrupt");
        let path = dir.0.join("settings.json");
        assert_eq!(read_marker(&path, "d"), SyncMarker::default());
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(read_marker(&path, "d"), SyncMarker::default());
        // Writing heals the file.
        write_marker(&path, "d", &marker(Some("t"), false, Some("h"))).unwrap();
        assert_eq!(read_marker(&path, "d").synced_at.as_deref(), Some("t"));
    }
}
