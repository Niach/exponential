//! Typed `users.*` tRPC helpers + the EXP-2a hidden personal key
//! (masterplan-v3 §7.2). Server procedures verified against
//! `apps/web/src/lib/trpc/users.ts`:
//!
//! - `users.mintPersonalApiKey({name?})` → `{key, id, name, start, prefix,
//!   createdAt}` — `key` is the RAW `expu_…` credential, returned **exactly
//!   once** (the server stores only a hash).
//! - `users.listPersonalApiKeys()` → `{keys: [{id, name, start, prefix,
//!   createdAt, lastRequest}]}`.
//! - `users.revokePersonalApiKey({id})` → `{ok: true}`.
//!
//! **EXP-2a is explicit: there is never a manual API-key text field in the
//! desktop UI.** The key is minted silently on first need (the coding
//! launcher's `.mcp.json`, §7.1 step 4), stored in the file store, and only
//! ever flows token-store → `.mcp.json`. Settings shows a status row (`start`
//! prefix from the list call) with Regenerate as the ONLY control.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::ApiError;
use crate::token_store::{SecretKind, TokenStore};
use crate::trpc::TrpcClient;

/// §7.2: never block the launcher critical path — bound secret reads to ~2s
/// (a slow Secret Service prompt falls back to the file store, not a hang).
pub const PERSONAL_KEY_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// `users.mintPersonalApiKey` output. `key` is the raw credential — handle
/// it like a password: token-store/`.mcp.json` only, NEVER logged or displayed.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MintedPersonalKey {
    pub key: String,
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Non-secret display prefix (e.g. `expu_ab`) — safe for the settings row.
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// One row of `users.listPersonalApiKeys` — display metadata only, no secret.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalKeyMeta {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub last_request: Option<String>,
}

#[derive(Deserialize)]
struct ListKeysResponse {
    keys: Vec<PersonalKeyMeta>,
}

#[derive(Serialize)]
struct MintInput<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

#[derive(Serialize)]
struct RevokeInput<'a> {
    id: &'a str,
}

/// `users.mintPersonalApiKey` — mutation.
pub fn mint_personal_api_key(
    trpc: &TrpcClient,
    name: Option<&str>,
) -> Result<MintedPersonalKey, ApiError> {
    trpc.mutation("users.mintPersonalApiKey", &MintInput { name })
}

/// `users.listPersonalApiKeys` — query (GET; POST would 405).
pub fn list_personal_api_keys(trpc: &TrpcClient) -> Result<Vec<PersonalKeyMeta>, ApiError> {
    let response: ListKeysResponse = trpc.query("users.listPersonalApiKeys")?;
    Ok(response.keys)
}

/// `users.revokePersonalApiKey` — mutation.
pub fn revoke_personal_api_key(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Deserialize)]
    struct RevokeAck {
        #[allow(dead_code)]
        #[serde(default)]
        ok: bool,
    }
    let _: RevokeAck = trpc.mutation("users.revokePersonalApiKey", &RevokeInput { id })?;
    Ok(())
}

/// The key's server-side display name for this device (§7.2:
/// `Device: <hostname>`).
pub fn device_key_name() -> String {
    format!("Device: {}", hostname())
}

fn hostname() -> String {
    // No std hostname API; env vars first (cheap), then the ubiquitous
    // `hostname` binary (macOS/Linux/Windows all ship one).
    for var in ["HOSTNAME", "COMPUTERNAME", "HOST"] {
        if let Ok(value) = std::env::var(var) {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    if let Ok(output) = std::process::Command::new("hostname").output() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    "unknown-host".to_string()
}

/// EXP-2a auto-mint (§7.2): return the device's personal `expu_` key, minting
/// it silently on first need. The secret read is bounded
/// ([`PERSONAL_KEY_READ_TIMEOUT`]) so this never stalls Start-coding; the
/// mint itself can race the git prep (only `.mcp.json` needs the result).
/// The user never sees, types, or pastes the key.
pub fn ensure_personal_key(
    trpc: &TrpcClient,
    store: &TokenStore,
    account_id: &str,
) -> Result<String, ApiError> {
    if let Some(key) = store.get_bounded(
        account_id,
        SecretKind::PersonalApiKey,
        PERSONAL_KEY_READ_TIMEOUT,
    ) {
        return Ok(key);
    }
    let minted = mint_personal_api_key(trpc, Some(&device_key_name()))?;
    store.set(account_id, SecretKind::PersonalApiKey, &minted.key)?;
    // Best-effort: remember the row id so Regenerate can revoke precisely.
    let _ = store.set(account_id, SecretKind::PersonalApiKeyId, &minted.id);
    Ok(minted.key)
}

/// §7.2 Regenerate — **mint-new-then-revoke-old; order is load-bearing**:
/// never revoke before the new key is safely stored, or a crash mid-operation
/// leaves the device with no working key. `revoke_id` lets the settings row
/// pass the exact listed row; when `None`, the locally remembered id is used.
/// A failed revoke is non-fatal (the new key already works; the stale row
/// stays visible in the list until the next regenerate).
pub fn regenerate_personal_key(
    trpc: &TrpcClient,
    store: &TokenStore,
    account_id: &str,
    revoke_id: Option<&str>,
) -> Result<MintedPersonalKey, ApiError> {
    let old_id = revoke_id.map(str::to_string).or_else(|| {
        store.get_bounded(
            account_id,
            SecretKind::PersonalApiKeyId,
            PERSONAL_KEY_READ_TIMEOUT,
        )
    });

    // 1. Mint the fresh key.
    let minted = mint_personal_api_key(trpc, Some(&device_key_name()))?;
    // 2. Store it — the point of no return for the OLD key.
    store.set(account_id, SecretKind::PersonalApiKey, &minted.key)?;
    let _ = store.set(account_id, SecretKind::PersonalApiKeyId, &minted.id);
    // 3. Only now revoke the previous row.
    if let Some(old) = old_id {
        if old != minted.id {
            let _ = revoke_personal_api_key(trpc, &old);
        }
    }
    Ok(minted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-users-test-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    const MINT_BODY: &str = r#"{"result":{"data":{
        "key":"expu_rawsecret123","id":"key-1","name":"Device: testbox",
        "start":"expu_ra","prefix":"expu_","createdAt":"2026-07-02T10:00:00.000Z"}}}"#;

    #[test]
    fn mint_decodes_camel_case_envelope() {
        let (base, captured) = one_shot_server(200, MINT_BODY);
        let minted = mint_personal_api_key(&client(&base), Some("Device: testbox")).unwrap();
        assert_eq!(minted.key, "expu_rawsecret123");
        assert_eq!(minted.id, "key-1");
        assert_eq!(minted.start.as_deref(), Some("expu_ra"));
        assert_eq!(
            minted.created_at.as_deref(),
            Some("2026-07-02T10:00:00.000Z")
        );
        let request = captured
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(request.starts_with("POST /api/trpc/users.mintPersonalApiKey HTTP/1.1"));
        assert!(request.ends_with(r#"{"name":"Device: testbox"}"#));
    }

    #[test]
    fn list_decodes_keys_array_and_uses_get() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"keys":[
                {"id":"key-1","name":"Device: a","start":"expu_ra","prefix":"expu_",
                 "createdAt":"2026-07-01T00:00:00.000Z","lastRequest":null}]}}}"#,
        );
        let keys = list_personal_api_keys(&client(&base)).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, "key-1");
        assert_eq!(keys[0].last_request, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // tRPC routes reads as GET — POST to a .query 405s (iOS-proven).
        assert!(request.starts_with("GET /api/trpc/users.listPersonalApiKeys HTTP/1.1"));
    }

    #[test]
    fn ensure_personal_key_reads_store_without_network() {
        let dir = TempDir::new("ensure-hit");
        let store = TokenStore::file_only(dir.0.clone());
        store
            .set("acct", SecretKind::PersonalApiKey, "expu_existing")
            .unwrap();
        // Unroutable base: any network call would error — proving the hit
        // path never touches the server.
        let trpc = client("http://127.0.0.1:1");
        let key = ensure_personal_key(&trpc, &store, "acct").unwrap();
        assert_eq!(key, "expu_existing");
    }

    #[test]
    fn ensure_personal_key_mints_and_stores_on_first_need() {
        let dir = TempDir::new("ensure-mint");
        let store = TokenStore::file_only(dir.0.clone());
        let (base, captured) = one_shot_server(200, MINT_BODY);
        let key = ensure_personal_key(&client(&base), &store, "acct").unwrap();
        assert_eq!(key, "expu_rawsecret123");
        // Raw key + row id both stashed for later sessions / regenerate.
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKey).as_deref(),
            Some("expu_rawsecret123")
        );
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKeyId).as_deref(),
            Some("key-1")
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // The silent mint names the key after the device (§7.2).
        assert!(request.contains(r#"{"name":"Device: "#));
    }

    #[test]
    fn regenerate_stores_new_key_before_any_revoke() {
        let dir = TempDir::new("regen");
        let store = TokenStore::file_only(dir.0.clone());
        store
            .set("acct", SecretKind::PersonalApiKey, "expu_old")
            .unwrap();
        store
            .set("acct", SecretKind::PersonalApiKeyId, "key-0")
            .unwrap();
        // One-shot server: serves ONLY the mint; the follow-up revoke call
        // finds the socket closed and fails — which must be non-fatal.
        let (base, _captured) = one_shot_server(200, MINT_BODY);
        let minted = regenerate_personal_key(&client(&base), &store, "acct", None).unwrap();
        assert_eq!(minted.id, "key-1");
        // New key is in the store even though the revoke errored.
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKey).as_deref(),
            Some("expu_rawsecret123")
        );
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKeyId).as_deref(),
            Some("key-1")
        );
    }

    #[test]
    fn device_key_name_has_prefix() {
        let name = device_key_name();
        assert!(name.starts_with("Device: "));
        assert!(name.len() > "Device: ".len());
    }
}
