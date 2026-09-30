//! Typed `users.*` tRPC helpers + the hidden personal key
//! (masterplan-v3 §7.2). Server procedures verified against
//! `apps/web/src/lib/trpc/users.ts`:
//!
//! - `users.mintPersonalApiKey({name?})` → `{key, id, name, start, prefix,
//!   createdAt}` — `key` is the RAW `expu_…` credential, returned **exactly
//!   once** (the server stores only a hash).
//! - `users.listPersonalApiKeys()` → `{keys: [{id, name, start, prefix,
//!   createdAt, lastRequest}]}`.
//! - `users.revokePersonalApiKey({id})` → `{ok: true}`.
//! - `users.timezone()` → `{timezone: string | null}` and
//!   `users.setTimezone({timezone, onlyIfUnset?})` → `{saved}` (EXP-369) — the
//!   IANA zone the daily digest's send hour is read in. SERVER-ONLY column
//!   (never synced), so both go through tRPC.
//!
//! (Account deletion is deliberately web/mobile-only — EXP-69 removed the
//! desktop flow, so there is no `users.deleteAccount` helper here.)
//!
//! **The rule is explicit: the desktop never asks the user to TYPE or PASTE
//! an API key.** The device's own key is minted silently on first need (the
//! coding launcher's `.exp-mcp.json`, §7.1 step 4), stored in the file store,
//! and only ever flows token-store → `.exp-mcp.json`. EXP-238 added a
//! Settings → API keys pane that lists/mints/revokes the account's keys —
//! a mint shows the raw key exactly once for the user to copy OUT (scripts,
//! other MCP clients); it is still never an input.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::ApiError;
use crate::token_store::{SecretKind, TokenStore};
use crate::trpc::TrpcClient;

/// §7.2: never block the launcher critical path — bound secret reads to ~2s
/// (a slow Secret Service prompt falls back to the file store, not a hang).
pub const PERSONAL_KEY_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// `users.mintPersonalApiKey` output. `key` is the raw credential — handle
/// it like a password: never logged; shown to the user exactly once, and
/// only by the Settings → API keys pane's explicit mint (EXP-238).
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
    /// EXP-1140: `agent` tags the hidden key the launcher mints for the
    /// agent it spawns (the server stores it as the key's `kind`); absent =
    /// a person's own key, so an older server sees the wire it always did.
    #[serde(skip_serializing_if = "Option::is_none")]
    purpose: Option<&'a str>,
}

#[derive(Serialize)]
struct RevokeInput<'a> {
    id: &'a str,
}

/// EXP-1140: the `purpose` the launcher mints its hidden agent key with. The
/// server refuses such a key where a person's key passes
/// (`mcpServers.resolveForLaunch`): a prompt-injected agent holds it, so it
/// must never be able to read the member's MCP credentials back.
pub const AGENT_KEY_PURPOSE: &str = "agent";

/// `users.mintPersonalApiKey` — mutation. `purpose` = `None` for a key the
/// person mints for themselves, [`AGENT_KEY_PURPOSE`] for the launcher's.
pub fn mint_personal_api_key(
    trpc: &TrpcClient,
    name: Option<&str>,
    purpose: Option<&str>,
) -> Result<MintedPersonalKey, ApiError> {
    trpc.mutation("users.mintPersonalApiKey", &MintInput { name, purpose })
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

/// `mcpGrants.hasAny` — query (GET): whether the user has authorized any MCP
/// OAuth client. One half of the getting-started "Connect your tools via
/// MCP" signal (EXP-548, web parity: a grant OR a personal API key).
pub fn mcp_grants_has_any(trpc: &TrpcClient) -> Result<bool, ApiError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HasAny {
        #[serde(default)]
        has_any: bool,
    }
    let response: HasAny = trpc.query("mcpGrants.hasAny")?;
    Ok(response.has_any)
}

/// `users.timezone` — query (GET). `None` = never captured; the digest sweep
/// falls back to UTC for those accounts.
pub fn users_timezone(trpc: &TrpcClient) -> Result<Option<String>, ApiError> {
    #[derive(Deserialize)]
    struct TimezoneResponse {
        #[serde(default)]
        timezone: Option<String>,
    }
    let response: TimezoneResponse = trpc.query("users.timezone")?;
    Ok(response.timezone)
}

/// `users.setTimezone` — mutation. `only_if_unset` is the post-login claim
/// (the server keeps an existing value); an explicit pick in settings sends
/// `false`, which omits the flag entirely.
pub fn users_set_timezone(
    trpc: &TrpcClient,
    timezone: &str,
    only_if_unset: bool,
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        timezone: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        only_if_unset: Option<bool>,
    }
    #[derive(Deserialize)]
    struct SetAck {
        #[allow(dead_code)]
        #[serde(default)]
        saved: bool,
    }
    let _: SetAck = trpc.mutation(
        "users.setTimezone",
        &Input {
            timezone,
            only_if_unset: only_if_unset.then_some(true),
        },
    )?;
    Ok(())
}

// ---- EXP-1126: account sign-in methods ----
//
// `users.signInMethods` is ONE payload for every client
// (apps/web/src/lib/auth/sign-in-methods.ts): the primary email + whether
// codes go out, the linkable providers in render order (Apple, Google, the
// OIDC providers, then a `credential` "Password" row only while one is set,
// plus linked-but-unconfigured rows with `available: false`), the passkeys,
// and `waysIn` — how many logins the account holds right now. Removals go
// through tRPC so the server's last-way-in rule answers
// (`PRECONDITION_FAILED`, [`LAST_SIGN_IN_METHOD_MESSAGE`]).

/// The server's refusal when a removal would leave no way in — byte-equal to
/// `LAST_SIGN_IN_METHOD_MESSAGE` (apps/web/src/lib/auth/sign-in-methods.ts).
pub const LAST_SIGN_IN_METHOD_MESSAGE: &str =
    "This is your only way to sign in. Add another method before removing it.";

/// `users.signInMethods` output.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignInMethods {
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub email_otp_enabled: bool,
    #[serde(default)]
    pub password_enabled: bool,
    #[serde(default)]
    pub passkey_enabled: bool,
    #[serde(default)]
    pub providers: Vec<SignInProvider>,
    #[serde(default)]
    pub passkeys: Vec<SignInPasskey>,
    #[serde(default)]
    pub ways_in: u32,
}

/// One provider row. `id` is the Better Auth provider id (`google`, `apple`,
/// an OIDC id, or `credential` for the password); `kind` is
/// `apple|google|oidc|password` (kept a string so a newer kind never fails
/// the decode).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignInProvider {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// The instance still offers this login (a linked-but-unconfigured
    /// provider stays listed so it can be unlinked, never re-linked).
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub linked: bool,
    #[serde(default)]
    pub linked_at: Option<String>,
}

/// One passkey row.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignInPasskey {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub backed_up: bool,
}

/// `users.mintSignInLinkTicket` output — the short-lived ticket the browser
/// handoff's LINK mode redeems (`/api/mobile-oauth-start?link=…`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInLinkTicket {
    pub ticket: String,
    #[serde(default)]
    pub expires_in_seconds: u64,
}

#[derive(Deserialize)]
struct OkAck {
    #[allow(dead_code)]
    #[serde(default)]
    ok: bool,
}

/// `users.signInMethods` — query (GET).
pub fn sign_in_methods(trpc: &TrpcClient) -> Result<SignInMethods, ApiError> {
    trpc.query("users.signInMethods")
}

/// `users.unlinkSignInMethod({providerId})` — mutation. Refused with
/// `PRECONDITION_FAILED` ([`LAST_SIGN_IN_METHOD_MESSAGE`]) for the last way in.
pub fn unlink_sign_in_method(trpc: &TrpcClient, provider_id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        provider_id: &'a str,
    }
    let _: OkAck = trpc.mutation("users.unlinkSignInMethod", &Input { provider_id })?;
    Ok(())
}

/// `users.deletePasskey({id})` — mutation, same last-way-in refusal.
pub fn delete_passkey(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    let _: OkAck = trpc.mutation("users.deletePasskey", &RevokeInput { id })?;
    Ok(())
}

/// `users.mintSignInLinkTicket({provider})` — mutation. `provider` is
/// `google`, `apple` or an OIDC provider id.
pub fn mint_sign_in_link_ticket(
    trpc: &TrpcClient,
    provider: &str,
) -> Result<SignInLinkTicket, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        provider: &'a str,
    }
    trpc.mutation("users.mintSignInLinkTicket", &Input { provider })
}

/// Whether a provider row may be unlinked right now — the web section's rule
/// (`provider.linked && waysIn > 1`); the server re-checks either way.
pub fn can_unlink(methods: &SignInMethods, provider: &SignInProvider) -> bool {
    provider.linked && methods.ways_in > 1
}

/// The key's server-side display name for this device (§7.2:
/// `Device: <hostname>`).
pub fn device_key_name() -> String {
    format!("Device: {}", hostname())
}

/// The OS hostname — the human-readable steer `deviceLabel` in the phone's
/// device picker (§8.2) and the suffix of [`device_key_name`].
pub fn hostname() -> String {
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
    // EXP-419: CREATE_NO_WINDOW so a console-subsystem child never flashes a
    // conhost on Windows (inlined — `api` doesn't depend on the terminal
    // crate's shared `background_command`).
    #[allow(unused_mut)]
    let mut command = std::process::Command::new("hostname");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    if let Ok(output) = command.output() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    "unknown-host".to_string()
}

/// Hidden-key auto-mint (§7.2): return the device's personal `expu_` key, minting
/// it silently on first need. The secret read is bounded
/// ([`PERSONAL_KEY_READ_TIMEOUT`]) so this never stalls Start-coding; the
/// mint itself can race the git prep (only `.exp-mcp.json` needs the result).
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
        let purpose = store.get_bounded(
            account_id,
            SecretKind::PersonalApiKeyPurpose,
            PERSONAL_KEY_READ_TIMEOUT,
        );
        if purpose.as_deref() == Some(AGENT_KEY_PURPOSE) {
            return Ok(key);
        }
        // EXP-1140: a key minted before the agent tag passes the server's
        // kind gate like a person's — retag it ONCE by regenerating
        // (mint-new-then-revoke-old, so a crash mid-way never strands the
        // device). Offline or refused, the old key keeps working and the
        // retag is retried on the next need.
        return match regenerate_personal_key(trpc, store, account_id, None) {
            Ok(minted) => Ok(minted.key),
            Err(err) => {
                log::warn!(
                    "[personal-key] could not retag the agent key ({}); keeping the untagged one for now",
                    err.user_message()
                );
                Ok(key)
            }
        };
    }
    let minted = mint_personal_api_key(trpc, Some(&device_key_name()), Some(AGENT_KEY_PURPOSE))?;
    store.set(account_id, SecretKind::PersonalApiKey, &minted.key)?;
    // Best-effort: remember the row id so Regenerate can revoke precisely,
    // and the purpose so the retag above never repeats.
    let _ = store.set(account_id, SecretKind::PersonalApiKeyId, &minted.id);
    let _ = store.set(account_id, SecretKind::PersonalApiKeyPurpose, AGENT_KEY_PURPOSE);
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

    // 1. Mint the fresh key (EXP-1140: always as the agent's).
    let minted = mint_personal_api_key(trpc, Some(&device_key_name()), Some(AGENT_KEY_PURPOSE))?;
    // 2. Store it — the point of no return for the OLD key.
    store.set(account_id, SecretKind::PersonalApiKey, &minted.key)?;
    let _ = store.set(account_id, SecretKind::PersonalApiKeyId, &minted.id);
    let _ = store.set(account_id, SecretKind::PersonalApiKeyPurpose, AGENT_KEY_PURPOSE);
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
        let minted = mint_personal_api_key(&client(&base), Some("Device: testbox"), None).unwrap();
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
    fn mcp_grants_has_any_decodes_and_uses_get() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"hasAny":true}}}"#);
        assert!(mcp_grants_has_any(&client(&base)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/mcpGrants.hasAny HTTP/1.1"));
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
        store
            .set("acct", SecretKind::PersonalApiKeyPurpose, AGENT_KEY_PURPOSE)
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
        // Raw key + row id both kept for later sessions / regenerate.
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKey).as_deref(),
            Some("expu_rawsecret123")
        );
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKeyId).as_deref(),
            Some("key-1")
        );
        // EXP-1140: minted AS the agent's key, and remembered as such.
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKeyPurpose).as_deref(),
            Some(AGENT_KEY_PURPOSE)
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // The silent mint names the key after the device (§7.2).
        assert!(request.contains(r#"{"name":"Device: "#));
        assert!(request.contains(r#""purpose":"agent"}"#), "{request}");
    }

    /// EXP-1140: a key minted before the agent tag would pass the server's
    /// kind gate like a person's. The first need after the update retags it
    /// by regenerating — new key stored, old row revoked — exactly once.
    #[test]
    fn ensure_personal_key_retags_a_legacy_key_once() {
        let dir = TempDir::new("ensure-retag");
        let store = TokenStore::file_only(dir.0.clone());
        store
            .set("acct", SecretKind::PersonalApiKey, "expu_legacy")
            .unwrap();
        store
            .set("acct", SecretKind::PersonalApiKeyId, "key-0")
            .unwrap();
        // One-shot: serves the mint; the follow-up revoke is best-effort.
        let (base, captured) = one_shot_server(200, MINT_BODY);
        let key = ensure_personal_key(&client(&base), &store, "acct").unwrap();
        assert_eq!(key, "expu_rawsecret123");
        assert_eq!(
            store.get("acct", SecretKind::PersonalApiKeyPurpose).as_deref(),
            Some(AGENT_KEY_PURPOSE)
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains(r#""purpose":"agent"}"#), "{request}");
        // Tagged now: the next need is a plain store hit (unroutable base).
        let again = ensure_personal_key(&client("http://127.0.0.1:1"), &store, "acct").unwrap();
        assert_eq!(again, "expu_rawsecret123");
    }

    /// The retag never strands an offline device: the legacy key keeps
    /// working and the retag is retried on the next need.
    #[test]
    fn ensure_personal_key_keeps_a_legacy_key_when_the_retag_fails() {
        let dir = TempDir::new("ensure-retag-offline");
        let store = TokenStore::file_only(dir.0.clone());
        store
            .set("acct", SecretKind::PersonalApiKey, "expu_legacy")
            .unwrap();
        let key = ensure_personal_key(&client("http://127.0.0.1:1"), &store, "acct").unwrap();
        assert_eq!(key, "expu_legacy");
        assert_eq!(store.get("acct", SecretKind::PersonalApiKeyPurpose), None);
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
    fn timezone_query_uses_get_and_decodes_null() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"timezone":null}}}"#);
        assert_eq!(users_timezone(&client(&base)).unwrap(), None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/users.timezone HTTP/1.1"));
    }

    #[test]
    fn set_timezone_omits_the_flag_unless_claiming() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"saved":true}}}"#);
        users_set_timezone(&client(&base), "Europe/Berlin", false).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/users.setTimezone HTTP/1.1"));
        assert!(request.ends_with(r#"{"timezone":"Europe/Berlin"}"#));
    }

    #[test]
    fn set_timezone_claim_sends_camel_case_flag() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"saved":false}}}"#);
        users_set_timezone(&client(&base), "America/New_York", true).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.ends_with(r#"{"timezone":"America/New_York","onlyIfUnset":true}"#)
        );
    }

    const SIGN_IN_METHODS_BODY: &str = r#"{"result":{"data":{
        "email":"ada@example.com","emailVerified":true,"emailOtpEnabled":true,
        "passwordEnabled":true,"passkeyEnabled":true,
        "providers":[
          {"id":"apple","name":"Apple","kind":"apple","available":true,"linked":false,"linkedAt":null},
          {"id":"google","name":"Google","kind":"google","available":true,"linked":true,"linkedAt":"2026-09-01T10:00:00.000Z"},
          {"id":"okta","name":"Okta","kind":"oidc","available":false,"linked":true,"linkedAt":null},
          {"id":"credential","name":"Password","kind":"password","available":true,"linked":true,"linkedAt":"2026-01-02T00:00:00.000Z"}],
        "passkeys":[{"id":"pk-1","name":null,"createdAt":"2026-09-02T00:00:00.000Z","backedUp":true}],
        "waysIn":4}}}"#;

    #[test]
    fn sign_in_methods_decodes_and_uses_get() {
        let (base, captured) = one_shot_server(200, SIGN_IN_METHODS_BODY);
        let methods = sign_in_methods(&client(&base)).unwrap();
        assert_eq!(methods.email, "ada@example.com");
        assert!(methods.email_otp_enabled);
        assert_eq!(methods.ways_in, 4);
        assert_eq!(methods.providers.len(), 4);
        assert_eq!(methods.providers[1].kind, "google");
        assert_eq!(
            methods.providers[1].linked_at.as_deref(),
            Some("2026-09-01T10:00:00.000Z")
        );
        assert!(!methods.providers[2].available);
        assert_eq!(methods.providers[3].id, "credential");
        assert_eq!(methods.passkeys.len(), 1);
        assert_eq!(methods.passkeys[0].name, None);
        assert!(methods.passkeys[0].backed_up);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/users.signInMethods HTTP/1.1"));
    }

    #[test]
    fn sign_in_methods_tolerates_missing_lists() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"email":"a@b.c","waysIn":1}}}"#,
        );
        let methods = sign_in_methods(&client(&base)).unwrap();
        assert!(methods.providers.is_empty());
        assert!(methods.passkeys.is_empty());
    }

    #[test]
    fn unlink_sends_the_provider_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        unlink_sign_in_method(&client(&base), "google").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/users.unlinkSignInMethod HTTP/1.1"));
        assert!(request.ends_with(r#"{"providerId":"google"}"#));
    }

    #[test]
    fn unlink_refusal_carries_the_server_message() {
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"This is your only way to sign in. Add another method before removing it.","code":-32600,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        let err = unlink_sign_in_method(&client(&base), "google").unwrap_err();
        assert_eq!(err.user_message(), LAST_SIGN_IN_METHOD_MESSAGE);
    }

    #[test]
    fn delete_passkey_sends_the_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        delete_passkey(&client(&base), "pk-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/users.deletePasskey HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"pk-1"}"#));
    }

    #[test]
    fn mint_link_ticket_sends_the_provider_and_decodes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ticket":"tkt.abc","expiresInSeconds":120}}}"#,
        );
        let ticket = mint_sign_in_link_ticket(&client(&base), "apple").unwrap();
        assert_eq!(ticket.ticket, "tkt.abc");
        assert_eq!(ticket.expires_in_seconds, 120);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/users.mintSignInLinkTicket HTTP/1.1"));
        assert!(request.ends_with(r#"{"provider":"apple"}"#));
    }

    #[test]
    fn can_unlink_needs_a_linked_row_and_another_way_in() {
        let provider = |linked: bool| SignInProvider {
            id: "google".to_string(),
            name: "Google".to_string(),
            kind: "google".to_string(),
            available: true,
            linked,
            linked_at: None,
        };
        let methods = |ways_in: u32| SignInMethods {
            ways_in,
            ..SignInMethods::default()
        };
        assert!(can_unlink(&methods(2), &provider(true)));
        assert!(!can_unlink(&methods(1), &provider(true)));
        assert!(!can_unlink(&methods(0), &provider(true)));
        assert!(!can_unlink(&methods(3), &provider(false)));
    }

    #[test]
    fn device_key_name_has_prefix() {
        let name = device_key_name();
        assert!(name.starts_with("Device: "));
        assert!(name.len() > "Device: ".len());
    }
}
