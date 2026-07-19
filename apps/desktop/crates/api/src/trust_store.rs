//! §7.3.5 Trust & Run — the per-device trust store.
//!
//! Run configs are DB-stored argv the desktop spawns as local child
//! processes: the ONE place server data is executed. The mandatory
//! compensating control is client-side — before a launch, the run bar
//! compares [`crate::run_configs::command_set_hash`] over the *full fetched
//! set* against the hash this device last trusted for the board. Any
//! mismatch (add / edit / a config from another author / a fresh device)
//! blocks the launch behind the Trust & Run dialog; trusting records the new
//! hash here. **Never auto-run untrusted.**
//!
//! Storage: a tiny rusqlite DB in the per-account dir
//! (`{data_dir}/accounts/{account_id}/run_trust.sqlite` — §7.3.5 "a small
//! rusqlite table in the per-account store"), keyed `(device_id, board_id)`.
//! The same DB carries the run bar's last-selected run config per board
//! (§7.5 dropdown persistence) — UI convenience, not part of the trust
//! boundary.
//!
//! The `device_id` (§7.7) is a stable per-install UUID living as the
//! `deviceId` top-level key of `{data_dir}/settings.json` (the local config —
//! [`crate::default_data_dir`]); [`device_id`] generates it once and never
//! regenerates. It is shared with steer presence (§08). Reads/writes merge
//! over the existing JSON object so keys owned by other subsystems
//! (`claudePath`, `reposRoot`, …) survive.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension as _};

/// Error surface of the trust store — descriptive, log-and-degrade material
/// (a broken trust store must fail CLOSED: callers treat errors as
/// "untrusted", never as "trusted").
#[derive(Debug)]
pub struct TrustStoreError(pub String);

impl fmt::Display for TrustStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "trust store: {}", self.0)
    }
}

impl std::error::Error for TrustStoreError {}

impl From<rusqlite::Error> for TrustStoreError {
    fn from(err: rusqlite::Error) -> Self {
        TrustStoreError(err.to_string())
    }
}

impl From<std::io::Error> for TrustStoreError {
    fn from(err: std::io::Error) -> Self {
        TrustStoreError(err.to_string())
    }
}

/// Per-device, per-account trust records (see module docs).
pub struct TrustStore {
    conn: Connection,
}

impl TrustStore {
    /// Canonical location: `{data_dir}/accounts/{account_id}/run_trust.sqlite`
    /// — next to that account's `sync-v2.sqlite` (§5.4 layout).
    pub fn default_path(data_dir: &Path, account_id: &str) -> PathBuf {
        data_dir
            .join("accounts")
            .join(account_id)
            .join("run_trust.sqlite")
    }

    /// Open (creating parent dirs + schema on first use).
    pub fn open(path: &Path) -> Result<Self, TrustStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        migrate_legacy_board_columns(&conn)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS run_config_trust (
                device_id    TEXT NOT NULL,
                board_id   TEXT NOT NULL,
                trusted_hash TEXT NOT NULL,
                updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                PRIMARY KEY (device_id, board_id)
            );
            CREATE TABLE IF NOT EXISTS run_config_selection (
                board_id    TEXT PRIMARY KEY,
                run_config_id TEXT NOT NULL,
                updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
            );",
        )?;
        Ok(Self { conn })
    }

    /// The hash this device last trusted for `board_id`, if any.
    pub fn trusted_hash(
        &self,
        device_id: &str,
        board_id: &str,
    ) -> Result<Option<String>, TrustStoreError> {
        let hash = self
            .conn
            .query_row(
                "SELECT trusted_hash FROM run_config_trust
                 WHERE device_id = ?1 AND board_id = ?2",
                params![device_id, board_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(hash)
    }

    /// §7.3.5 gate: is `hash` (the CURRENT fetched command-set hash) exactly
    /// what this device last trusted? Any drift — including a never-trusted
    /// board — is `false`.
    pub fn is_trusted(
        &self,
        device_id: &str,
        board_id: &str,
        hash: &str,
    ) -> Result<bool, TrustStoreError> {
        Ok(self.trusted_hash(device_id, board_id)?.as_deref() == Some(hash))
    }

    /// Record the accepted hash (the Trust & Run dialog's confirm action).
    /// Overwrites any previous trust for the board on this device.
    pub fn trust(
        &self,
        device_id: &str,
        board_id: &str,
        hash: &str,
    ) -> Result<(), TrustStoreError> {
        self.conn.execute(
            "INSERT INTO run_config_trust (device_id, board_id, trusted_hash)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (device_id, board_id)
             DO UPDATE SET trusted_hash = excluded.trusted_hash,
                           updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![device_id, board_id, hash],
        )?;
        Ok(())
    }

    /// Last-selected run config for the board (§7.5 dropdown persistence).
    pub fn selected_run_config(
        &self,
        board_id: &str,
    ) -> Result<Option<String>, TrustStoreError> {
        let id = self
            .conn
            .query_row(
                "SELECT run_config_id FROM run_config_selection WHERE board_id = ?1",
                params![board_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Persist the dropdown selection for the board.
    pub fn set_selected_run_config(
        &self,
        board_id: &str,
        run_config_id: &str,
    ) -> Result<(), TrustStoreError> {
        self.conn.execute(
            "INSERT INTO run_config_selection (board_id, run_config_id)
             VALUES (?1, ?2)
             ON CONFLICT (board_id)
             DO UPDATE SET run_config_id = excluded.run_config_id,
                           updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![board_id, run_config_id],
        )?;
        Ok(())
    }
}

/// EXP-180 rename migration: pre-rename trust DBs carry `project_id` columns
/// (the legacy name for `board_id`). Rename them in place — trust records are
/// keyed by server board ids, which the rename did NOT change, so existing
/// trust and dropdown selections survive verbatim. Runs before the
/// `CREATE TABLE IF NOT EXISTS` in [`TrustStore::open`]; a fresh DB (no
/// tables yet) is a no-op.
fn migrate_legacy_board_columns(conn: &Connection) -> Result<(), TrustStoreError> {
    for table in ["run_config_trust", "run_config_selection"] {
        let has_legacy = conn
            .prepare(&format!(
                "SELECT 1 FROM pragma_table_info('{table}') WHERE name = 'project_id'"
            ))?
            .exists([])?;
        if has_legacy {
            conn.execute_batch(&format!(
                "ALTER TABLE \"{table}\" RENAME COLUMN \"project_id\" TO \"board_id\""
            ))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// §7.7 deviceId — generate once, never regenerate
// ---------------------------------------------------------------------------

/// The stable per-install device UUID: the `deviceId` key of
/// `{data_dir}/settings.json`. Created (and persisted, merge-preserving) on
/// first call; identical forever after. If persisting fails the generated id
/// is still returned — callers keep working, the id just won't be stable
/// until the disk is writable again.
pub fn device_id(data_dir: &Path) -> String {
    let path = data_dir.join("settings.json");
    let mut root = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));

    if let Some(existing) = root
        .get("deviceId")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        return existing.to_string();
    }

    let generated = uuid::Uuid::new_v4().to_string();
    if let Some(object) = root.as_object_mut() {
        object.insert(
            "deviceId".to_string(),
            serde_json::Value::String(generated.clone()),
        );
    }
    let persist = || -> std::io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let mut rendered = serde_json::to_string_pretty(&root)
            .unwrap_or_else(|_| "{}".to_string());
        rendered.push('\n');
        fs::write(&path, rendered)
    };
    let _ = persist();
    generated
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-trust-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fresh_device_is_untrusted() {
        // §7.3.5: "A never-before-seen device starts untrusted."
        let dir = TempDir::new("fresh");
        let store = TrustStore::open(&dir.0.join("run_trust.sqlite")).unwrap();
        assert_eq!(store.trusted_hash("dev-1", "proj-1").unwrap(), None);
        assert!(!store.is_trusted("dev-1", "proj-1", "hash-a").unwrap());
    }

    #[test]
    fn trust_then_match_then_refire_on_change() {
        // The §7.3.5 lifecycle: trust stores the hash; the same hash passes;
        // ANY change (new hash) re-blocks; re-trusting the new hash passes.
        let dir = TempDir::new("lifecycle");
        let store = TrustStore::open(&dir.0.join("run_trust.sqlite")).unwrap();

        store.trust("dev-1", "proj-1", "hash-a").unwrap();
        assert!(store.is_trusted("dev-1", "proj-1", "hash-a").unwrap());

        // Config set changed → hash mismatch → untrusted again.
        assert!(!store.is_trusted("dev-1", "proj-1", "hash-b").unwrap());

        store.trust("dev-1", "proj-1", "hash-b").unwrap();
        assert!(store.is_trusted("dev-1", "proj-1", "hash-b").unwrap());
        // The old hash no longer passes (single-slot per (device, board)).
        assert!(!store.is_trusted("dev-1", "proj-1", "hash-a").unwrap());
    }

    #[test]
    fn trust_is_scoped_per_device_and_board() {
        let dir = TempDir::new("scope");
        let store = TrustStore::open(&dir.0.join("run_trust.sqlite")).unwrap();
        store.trust("dev-1", "proj-1", "hash-a").unwrap();
        // Another device on the same board: untrusted.
        assert!(!store.is_trusted("dev-2", "proj-1", "hash-a").unwrap());
        // Same device, another board: untrusted.
        assert!(!store.is_trusted("dev-1", "proj-2", "hash-a").unwrap());
    }

    #[test]
    fn trust_survives_reopen() {
        let dir = TempDir::new("reopen");
        let path = dir.0.join("run_trust.sqlite");
        TrustStore::open(&path)
            .unwrap()
            .trust("dev-1", "proj-1", "hash-a")
            .unwrap();
        let reopened = TrustStore::open(&path).unwrap();
        assert!(reopened.is_trusted("dev-1", "proj-1", "hash-a").unwrap());
    }

    #[test]
    fn selection_persists_per_board() {
        // §7.5: last-selected run config, per board, across restarts.
        let dir = TempDir::new("selection");
        let path = dir.0.join("run_trust.sqlite");
        {
            let store = TrustStore::open(&path).unwrap();
            assert_eq!(store.selected_run_config("proj-1").unwrap(), None);
            store.set_selected_run_config("proj-1", "rc-1").unwrap();
            store.set_selected_run_config("proj-2", "rc-9").unwrap();
            // Re-selecting overwrites.
            store.set_selected_run_config("proj-1", "rc-2").unwrap();
        }
        let store = TrustStore::open(&path).unwrap();
        assert_eq!(
            store.selected_run_config("proj-1").unwrap().as_deref(),
            Some("rc-2")
        );
        assert_eq!(
            store.selected_run_config("proj-2").unwrap().as_deref(),
            Some("rc-9")
        );
    }

    #[test]
    fn default_path_sits_in_the_account_dir() {
        let path = TrustStore::default_path(Path::new("/data"), "acct-1");
        assert_eq!(
            path,
            Path::new("/data/accounts/acct-1/run_trust.sqlite")
        );
    }

    #[test]
    fn device_id_is_stable_and_uuid_shaped() {
        let dir = TempDir::new("device-id");
        let first = device_id(&dir.0);
        let second = device_id(&dir.0);
        assert_eq!(first, second, "generate once, never regenerate (§7.7)");
        assert_eq!(first.len(), 36);
        assert!(uuid::Uuid::parse_str(&first).is_ok());
    }

    #[test]
    fn device_id_preserves_foreign_settings_keys() {
        // settings.json is shared with the coding settings (§7.7) — the merge
        // write must not clobber them.
        let dir = TempDir::new("device-merge");
        fs::write(
            dir.0.join("settings.json"),
            r#"{"claudePath":"/opt/claude","reposRoot":"~/code"}"#,
        )
        .unwrap();
        let id = device_id(&dir.0);
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(root["claudePath"], "/opt/claude");
        assert_eq!(root["reposRoot"], "~/code");
        assert_eq!(root["deviceId"], id.as_str());
    }

    #[test]
    fn device_id_respects_an_existing_value() {
        let dir = TempDir::new("device-existing");
        fs::write(
            dir.0.join("settings.json"),
            r#"{"deviceId":"pre-existing-id"}"#,
        )
        .unwrap();
        assert_eq!(device_id(&dir.0), "pre-existing-id");
    }
}
