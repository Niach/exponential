//! Secret storage (masterplan-v3 §5.7 / §7.2): a **file-based store** with
//! `0600` file / `0700` dir permissions at
//! `{data_dir}/accounts/{account_id}/{kind}` — the same posture the old macOS
//! app used for its debug credential store.
//!
//! **Locked decision (2026-07-03): never the OS keyring.** The `keyring`
//! crate's macOS Keychain backend re-prompts on every rebuild (an unsigned
//! dev binary gets a fresh code identity each build, so "Always Allow" never
//! sticks), and Linux Secret Service is absent on headless boxes anyway. The
//! file store is the single code path on every platform, dev and release.
//!
//! Entries are **per account** (an account = server URL + user, see
//! [`crate::accounts`]) and **per kind** — the Better Auth session token and
//! the hidden `expu_` personal key are separate named entries and must
//! never be confused (§5.7).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::ApiError;

/// Which secret an entry holds. Each kind is a distinct named entry per
/// account — the two credentials of §5.7 never share a slot. Team MCP
/// server credentials are NOT here: the server holds them per member
/// (`api::mcp_servers`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretKind {
    /// The Better Auth session token — the `Authorization: Bearer` for every
    /// shape + tRPC request. Deleted on 401 (§5.6b).
    SessionToken,
    /// The hidden auto-minted `expu_` personal API key — used ONLY
    /// inside the coding launcher's `.exp-mcp.json`, never as a sync/tRPC
    /// credential.
    PersonalApiKey,
    /// The server-side row id of the personal key (not itself a secret;
    /// stored alongside so Regenerate can revoke the *previous* row by id —
    /// §7.2 mint-new-then-revoke-old).
    PersonalApiKeyId,
    /// EXP-1140: what the stored personal key was minted AS on the server
    /// (`users::AGENT_KEY_PURPOSE` once tagged). Absent = a key minted before
    /// the tag existed, which `ensure_personal_key` regenerates once so the
    /// server can tell the agent's key from the person's.
    PersonalApiKeyPurpose,
}

impl SecretKind {
    /// Stable file name per kind. `SessionToken` is literally `token`,
    /// matching the §5.7 path spec (`{data_dir}/accounts/{id}/token`).
    fn suffix(&self) -> PathBuf {
        match self {
            SecretKind::SessionToken => PathBuf::from("token"),
            SecretKind::PersonalApiKey => PathBuf::from("personal-key"),
            SecretKind::PersonalApiKeyId => PathBuf::from("personal-key-id"),
            SecretKind::PersonalApiKeyPurpose => PathBuf::from("personal-key-purpose"),
        }
    }
}

/// File-based secret store (0600 files, 0700 dirs). The only secret store —
/// see the module docs for why there is deliberately no OS-keyring path.
pub struct TokenStore {
    data_dir: PathBuf,
}

impl TokenStore {
    /// Store rooted at the app data dir ([`crate::default_data_dir`]).
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Alias of [`TokenStore::new`], kept for call sites/tests that
    /// predate the never-keyring decision.
    pub fn file_only(data_dir: PathBuf) -> Self {
        Self::new(data_dir)
    }

    /// Read a secret.
    pub fn get(&self, account_id: &str, kind: SecretKind) -> Option<String> {
        self.file_get(account_id, kind)
    }

    /// Read a secret. The `timeout` is vestigial (it bounded OS-keyring
    /// prompts); a local 0600-file read never blocks, so this is just
    /// [`TokenStore::get`]. Kept so §7.2 call sites read as intended.
    pub fn get_bounded(
        &self,
        account_id: &str,
        kind: SecretKind,
        _timeout: Duration,
    ) -> Option<String> {
        self.file_get(account_id, kind)
    }

    /// Store a secret (0600, perms set before content is written).
    pub fn set(&self, account_id: &str, kind: SecretKind, value: &str) -> Result<(), ApiError> {
        self.file_set(account_id, kind, value)
    }

    /// Delete a secret (idempotent; §5.7: on 401 delete the session-token
    /// entry).
    pub fn delete(&self, account_id: &str, kind: SecretKind) {
        let _ = fs::remove_file(self.file_path(account_id, kind));
    }

    /// Delete every secret kind for an account (full account removal).
    pub fn delete_all(&self, account_id: &str) {
        for kind in [
            SecretKind::SessionToken,
            SecretKind::PersonalApiKey,
            SecretKind::PersonalApiKeyId,
        ] {
            self.delete(account_id, kind);
        }
        self.purge_retired_mcp(account_id);
    }

    /// EXP-792: the retired device-held MCP credentials (`accounts/<id>/mcp/`,
    /// from before the server held them): OAuth access + refresh tokens,
    /// typed secrets and DCR client ids in plaintext 0600 files no build
    /// reads or refreshes any more. An in-place upgrade would otherwise keep
    /// them indefinitely, so they go the first time the account is loaded or
    /// signed in after the upgrade (every host, desktop and CLI daemon,
    /// passes through `AuthStore` for that), not only when the account is
    /// removed. Returns whether anything was removed; a missing dir is the
    /// steady state and says nothing.
    pub fn purge_retired_mcp(&self, account_id: &str) -> bool {
        let dir = self.account_dir(account_id).join("mcp");
        match fs::remove_dir_all(&dir) {
            Ok(()) => {
                log::info!(
                    "[auth] removed the retired device-held MCP credentials of account {account_id}"
                );
                true
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
            Err(err) => {
                log::warn!(
                    "[auth] could not remove the retired MCP credentials at {}: {err}",
                    dir.display()
                );
                false
            }
        }
    }

    // ---- file store ----

    fn account_dir(&self, account_id: &str) -> PathBuf {
        self.data_dir.join("accounts").join(account_id)
    }

    fn file_path(&self, account_id: &str, kind: SecretKind) -> PathBuf {
        self.account_dir(account_id).join(kind.suffix())
    }

    fn file_get(&self, account_id: &str, kind: SecretKind) -> Option<String> {
        let raw = fs::read_to_string(self.file_path(account_id, kind)).ok()?;
        let trimmed = raw.trim_end_matches(['\n', '\r']).to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn file_set(&self, account_id: &str, kind: SecretKind, value: &str) -> Result<(), ApiError> {
        let path = self.file_path(account_id, kind);
        let dir = path
            .parent()
            .ok_or_else(|| ApiError::TokenStore(format!("no parent dir for {path:?}")))?;
        fs::create_dir_all(dir)
            .map_err(|e| ApiError::TokenStore(format!("create {dir:?}: {e}")))?;
        // Tighten every level between the account dir and the file, not just
        // the immediate parent.
        let account_dir = self.account_dir(account_id);
        let mut level = Some(dir);
        while let Some(current) = level {
            restrict_dir(current);
            if current == account_dir.as_path() {
                break;
            }
            level = current.parent().filter(|parent| parent.starts_with(&account_dir));
        }

        // §7.2: create with restrictive perms BEFORE the write — the file
        // must never exist world-readable, even for an instant.
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|e| ApiError::TokenStore(format!("open {path:?}: {e}")))?;
        // `mode(0o600)` only applies on create; tighten a pre-existing file too.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        }
        file.write_all(value.as_bytes())
            .map_err(|e| ApiError::TokenStore(format!("write {path:?}: {e}")))?;
        Ok(())
    }
}

/// Best-effort 0700 on the per-account secrets dir.
fn restrict_dir(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Self-cleaning unique temp dir (no tempfile dep in the team pins).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-api-test-{tag}-{}-{nanos}", std::process::id()));
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
    fn file_store_round_trips() {
        let dir = TempDir::new("roundtrip");
        let store = TokenStore::new(dir.0.clone());

        assert_eq!(store.get("acct-1", SecretKind::SessionToken), None);
        store
            .set("acct-1", SecretKind::SessionToken, "tok-abc123")
            .unwrap();
        assert_eq!(
            store.get("acct-1", SecretKind::SessionToken).as_deref(),
            Some("tok-abc123")
        );

        // Kinds are isolated slots.
        store
            .set("acct-1", SecretKind::PersonalApiKey, "expu_secret")
            .unwrap();
        assert_eq!(
            store.get("acct-1", SecretKind::SessionToken).as_deref(),
            Some("tok-abc123")
        );
        assert_eq!(
            store.get("acct-1", SecretKind::PersonalApiKey).as_deref(),
            Some("expu_secret")
        );

        // Accounts are isolated.
        assert_eq!(store.get("acct-2", SecretKind::SessionToken), None);

        store.delete("acct-1", SecretKind::SessionToken);
        assert_eq!(store.get("acct-1", SecretKind::SessionToken), None);
        // Delete is idempotent.
        store.delete("acct-1", SecretKind::SessionToken);
    }

    /// EXP-792: the pre-rewrite `accounts/<id>/mcp/` tree goes on the first
    /// purge, the account's live secrets stay, and a second pass is a no-op
    /// (a missing dir is the steady state).
    #[test]
    fn purge_retired_mcp_removes_the_dir_once_and_keeps_the_live_secrets() {
        let dir = TempDir::new("mcp-purge");
        let store = TokenStore::file_only(dir.0.clone());
        let mcp = dir.0.join("accounts").join("a").join("mcp");
        fs::create_dir_all(mcp.join("x")).unwrap();
        fs::write(mcp.join("x").join("oauth"), r#"{"access":"t"}"#).unwrap();
        store.set("a", SecretKind::SessionToken, "tok").unwrap();

        assert!(store.purge_retired_mcp("a"));
        assert!(!mcp.exists());
        assert_eq!(store.get("a", SecretKind::SessionToken).as_deref(), Some("tok"));
        assert!(!store.purge_retired_mcp("a"));
        // An account that never had the dir is a no-op too.
        assert!(!store.purge_retired_mcp("never"));
    }

    #[test]
    fn session_token_file_path_matches_spec() {
        // §5.7: {data_dir}/accounts/{id}/token
        let dir = TempDir::new("path");
        let store = TokenStore::new(dir.0.clone());
        store.set("a1", SecretKind::SessionToken, "t").unwrap();
        assert!(dir.0.join("accounts").join("a1").join("token").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn store_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("perms");
        let store = TokenStore::new(dir.0.clone());
        store.set("acct-1", SecretKind::SessionToken, "tok").unwrap();
        let path = dir.0.join("accounts").join("acct-1").join("token");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "token file must be 0600, was {mode:o}");
        let dir_mode = fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700, "account dir must be 0700, was {dir_mode:o}");
    }

    #[test]
    fn get_bounded_is_plain_get() {
        let dir = TempDir::new("bounded");
        let store = TokenStore::new(dir.0.clone());
        store
            .set("acct-1", SecretKind::PersonalApiKey, "expu_k")
            .unwrap();
        assert_eq!(
            store
                .get_bounded(
                    "acct-1",
                    SecretKind::PersonalApiKey,
                    Duration::from_millis(50)
                )
                .as_deref(),
            Some("expu_k")
        );
    }

    #[test]
    fn delete_all_clears_every_kind() {
        let dir = TempDir::new("delall");
        let store = TokenStore::new(dir.0.clone());
        store.set("a", SecretKind::SessionToken, "t").unwrap();
        store.set("a", SecretKind::PersonalApiKey, "k").unwrap();
        store.set("a", SecretKind::PersonalApiKeyId, "id").unwrap();
        store.delete_all("a");
        assert_eq!(store.get("a", SecretKind::SessionToken), None);
        assert_eq!(store.get("a", SecretKind::PersonalApiKey), None);
        assert_eq!(store.get("a", SecretKind::PersonalApiKeyId), None);
    }
}
