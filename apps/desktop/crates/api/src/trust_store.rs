//! The per-device Actions trust store (EXP-253 — the direct descendant of
//! the run-config Trust & Run gate).
//!
//! Actions are DB-stored markdown prompts the desktop runs as interactive
//! claude sessions: the ONE place server data drives local execution. The
//! mandatory compensating control is client-side — before a launch, the
//! actions panel re-fetches the action and compares `sha256(body)` (see
//! [`crate::actions::body_hash`]) against the hash this device last trusted
//! for that action. Any mismatch (an edited body / another author's change /
//! a fresh device) blocks the launch behind the trust dialog; trusting
//! records the new hash here. **Never auto-run untrusted.** A broken trust
//! store must fail CLOSED: callers treat errors as "untrusted", never as
//! "trusted".
//!
//! Storage: a tiny rusqlite DB in the per-account dir
//! (`{data_dir}/accounts/{account_id}/run_trust.sqlite` — the historical
//! filename survives the run-config removal so account dirs don't litter),
//! keyed `(device_id, action_id)`. Legacy run-config tables are dropped on
//! open — their trust covered a dead feature, and dropping fails closed
//! (everything starts untrusted).
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
/// (fail CLOSED: callers treat errors as "untrusted", never as "trusted").
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

    /// Open (creating parent dirs + schema on first use). Drops the legacy
    /// run-config tables — that feature is gone, and a dropped trust record
    /// only ever means "ask again".
    pub fn open(path: &Path) -> Result<Self, TrustStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "DROP TABLE IF EXISTS run_config_trust;
            DROP TABLE IF EXISTS run_config_selection;
            CREATE TABLE IF NOT EXISTS action_trust (
                device_id    TEXT NOT NULL,
                action_id    TEXT NOT NULL,
                trusted_hash TEXT NOT NULL,
                updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                PRIMARY KEY (device_id, action_id)
            );",
        )?;
        Ok(Self { conn })
    }

    /// The body hash this device last trusted for `action_id`, if any.
    pub fn trusted_hash(
        &self,
        device_id: &str,
        action_id: &str,
    ) -> Result<Option<String>, TrustStoreError> {
        let hash = self
            .conn
            .query_row(
                "SELECT trusted_hash FROM action_trust
                 WHERE device_id = ?1 AND action_id = ?2",
                params![device_id, action_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(hash)
    }

    /// The trust gate: is `hash` (over the FRESHLY FETCHED body) exactly what
    /// this device last trusted? Any drift — including a never-trusted
    /// action — is `false`.
    pub fn is_trusted(
        &self,
        device_id: &str,
        action_id: &str,
        hash: &str,
    ) -> Result<bool, TrustStoreError> {
        Ok(self.trusted_hash(device_id, action_id)?.as_deref() == Some(hash))
    }

    /// Record the accepted hash (the trust dialog's confirm action).
    /// Overwrites any previous trust for the action on this device.
    pub fn trust(
        &self,
        device_id: &str,
        action_id: &str,
        hash: &str,
    ) -> Result<(), TrustStoreError> {
        self.conn.execute(
            "INSERT INTO action_trust (device_id, action_id, trusted_hash)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (device_id, action_id)
             DO UPDATE SET trusted_hash = excluded.trusted_hash,
                           updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![device_id, action_id, hash],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Minimal SHA-256 (FIPS 180-4) — avoids a crypto dependency for a
// change-detection hash. Not used for secrets. `crate::actions::body_hash`
// is the public face.
// ---------------------------------------------------------------------------

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
        // A never-before-seen device starts untrusted.
        let dir = TempDir::new("fresh");
        let store = TrustStore::open(&dir.0.join("run_trust.sqlite")).unwrap();
        assert_eq!(store.trusted_hash("dev-1", "action-1").unwrap(), None);
        assert!(!store.is_trusted("dev-1", "action-1", "hash-a").unwrap());
    }

    #[test]
    fn trust_then_match_then_refire_on_change() {
        // The lifecycle: trust stores the hash; the same hash passes; ANY
        // change (edited body → new hash) re-blocks; re-trusting passes.
        let dir = TempDir::new("lifecycle");
        let store = TrustStore::open(&dir.0.join("run_trust.sqlite")).unwrap();

        store.trust("dev-1", "action-1", "hash-a").unwrap();
        assert!(store.is_trusted("dev-1", "action-1", "hash-a").unwrap());

        // Body changed → hash mismatch → untrusted again.
        assert!(!store.is_trusted("dev-1", "action-1", "hash-b").unwrap());

        store.trust("dev-1", "action-1", "hash-b").unwrap();
        assert!(store.is_trusted("dev-1", "action-1", "hash-b").unwrap());
        // The old hash no longer passes (single slot per (device, action)).
        assert!(!store.is_trusted("dev-1", "action-1", "hash-a").unwrap());
    }

    #[test]
    fn trust_is_scoped_per_device_and_action() {
        let dir = TempDir::new("scope");
        let store = TrustStore::open(&dir.0.join("run_trust.sqlite")).unwrap();
        store.trust("dev-1", "action-1", "hash-a").unwrap();
        // Another device on the same action: untrusted.
        assert!(!store.is_trusted("dev-2", "action-1", "hash-a").unwrap());
        // Same device, another action: untrusted.
        assert!(!store.is_trusted("dev-1", "action-2", "hash-a").unwrap());
    }

    #[test]
    fn trust_survives_reopen() {
        let dir = TempDir::new("reopen");
        let path = dir.0.join("run_trust.sqlite");
        TrustStore::open(&path)
            .unwrap()
            .trust("dev-1", "action-1", "hash-a")
            .unwrap();
        let reopened = TrustStore::open(&path).unwrap();
        assert!(reopened.is_trusted("dev-1", "action-1", "hash-a").unwrap());
    }

    #[test]
    fn legacy_run_config_tables_are_dropped_on_open() {
        // A pre-EXP-253 DB carries run-config trust/selection tables. Open
        // must drop them (dead feature; dropping fails closed) and leave a
        // working action_trust store behind.
        let dir = TempDir::new("legacy");
        let path = dir.0.join("run_trust.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE run_config_trust (
                    device_id TEXT NOT NULL, board_id TEXT NOT NULL,
                    trusted_hash TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT '',
                    PRIMARY KEY (device_id, board_id)
                );
                CREATE TABLE run_config_selection (
                    board_id TEXT PRIMARY KEY, run_config_id TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT ''
                );
                INSERT INTO run_config_trust VALUES ('dev-1','proj-1','old-hash','');",
            )
            .unwrap();
        }
        let store = TrustStore::open(&path).unwrap();
        // Everything starts untrusted — no legacy carry-over.
        assert!(!store.is_trusted("dev-1", "proj-1", "old-hash").unwrap());
        for table in ["run_config_trust", "run_config_selection"] {
            let exists = store
                .conn
                .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")
                .unwrap()
                .exists([table])
                .unwrap();
            assert!(!exists, "{table} should be dropped");
        }
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
    fn sha256_matches_the_fips_vectors() {
        // FIPS 180-4 known-answer vectors — guards the hand-rolled digest.
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
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
