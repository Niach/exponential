//! Typed `installs.*` tRPC mirrors (EXP-1457 step 1, the #VAPP-37 wire
//! contract). Shapes verified against `apps/web/src/lib/trpc/installs.ts`.
//!
//! Every eNet install (the desktop app, the CLI daemon) registers its
//! endpoint id on every start and loads its team's installs as the node's
//! access allowlist. Server-only table: no Electric shape. The node side
//! (identity file, signing, the allowlist loop) is `steer::enet`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `installs.register` input. `endpoint_id` = lowercase hex of the 32-byte
/// ed25519 public key, `x25519_public` = unpadded base64url, `signature` =
/// unpadded base64url of the identity's ed25519 signature over
/// `exp-install-v1\n<userId>\n<endpointId>\n<x25519Public>\n<signedAt>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterInstall {
    /// `desktop` | `daemon` (phones and browsers register themselves).
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// The `devices.device_id` this install registered with (the row must
    /// exist: register the device first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub endpoint_id: String,
    pub x25519_public: String,
    pub signed_at: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registered {
    pub install_id: String,
}

/// One row of `installs.list {teamId}`: a team member's live install.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallEntry {
    pub install_id: String,
    pub user_id: String,
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    pub endpoint_id: String,
    pub x25519_public: String,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    /// EXP-1458: the `devices.device_id` the install registered with
    /// (desktop + daemon); a new tree's key is wrapped for the install of
    /// the device that will run it.
    #[serde(default)]
    pub device_id: Option<String>,
}

/// One row of `installs.mine`: the caller's own installs, revoked included.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyInstall {
    pub install_id: String,
    pub kind: String,
    pub endpoint_id: String,
    #[serde(default)]
    pub revoked_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamInput<'a> {
    team_id: &'a str,
}

/// VAPP-33: one row of `installs.forSession`: an install a viewer dials to
/// steer the session over eNet (newest first).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInstall {
    pub endpoint_id: String,
    pub kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionInput<'a> {
    session_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallInput<'a> {
    install_id: &'a str,
}

/// `installs.register`: idempotent, refreshes `last_seen_at`. `CONFLICT`
/// (409) = the endpoint belongs to another account, `FORBIDDEN` (403) = it
/// was revoked (mint a new identity).
pub fn register(trpc: &TrpcClient, input: &RegisterInstall) -> Result<Registered, ApiError> {
    trpc.mutation("installs.register", input)
}

/// `installs.list {teamId}`: the team members' installs that aren't revoked.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<InstallEntry>, ApiError> {
    trpc.query_with_input("installs.list", &TeamInput { team_id })
}

/// VAPP-33 `installs.forSession {sessionId}`: the live desktop/daemon
/// installs of the machine running the session (`fixtures/steer-enet.json`
/// DISCOVERY). NOT_FOUND (404) when the caller cannot see the session.
pub fn for_session(trpc: &TrpcClient, session_id: &str) -> Result<Vec<SessionInstall>, ApiError> {
    trpc.query_with_input("installs.forSession", &SessionInput { session_id })
}

/// `installs.mine`: the caller's own installs.
pub fn mine(trpc: &TrpcClient) -> Result<Vec<MyInstall>, ApiError> {
    trpc.query("installs.mine")
}

/// `installs.revoke {installId}`: only the caller's own installs; the
/// endpoint id can never register again.
pub fn revoke(trpc: &TrpcClient, install_id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation("installs.revoke", &InstallInput { install_id })?;
    Ok(())
}

// ── #EXP-1950 install approval (fixtures/install-trust.json) ───────────
// The answers are read by `enet_core::install_trust` (generic here: this
// crate does not depend on eNet).

/// `installs.trust {installId}`: every live install of the account and its
/// team-mates, and every approval about them.
pub fn trust<T: serde::de::DeserializeOwned>(trpc: &TrpcClient, install_id: &str) -> Result<T, ApiError> {
    trpc.query_with_input("installs.trust", &InstallInput { install_id })
}

/// `installs.requestApproval {installId}`: a waiting install asks, once.
pub fn request_approval(trpc: &TrpcClient, install_id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation("installs.requestApproval", &InstallInput { install_id })?;
    Ok(())
}

/// One `approve` riding in a root claim (an install that existed, decision e).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootClaimApproval {
    pub subject_install_id: String,
    pub signature: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaimRootInput<'a> {
    install_id: &'a str,
    signature: &'a str,
    #[serde(skip_serializing_if = "<[RootClaimApproval]>::is_empty")]
    approvals: &'a [RootClaimApproval],
}

/// `installs.claimRoot`: this install starts its account's chain (CONFLICT
/// = another install of the account claimed first).
pub fn claim_root(trpc: &TrpcClient, install_id: &str, signature: &str, approvals: &[RootClaimApproval]) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation("installs.claimRoot", &ClaimRootInput { install_id, signature, approvals })?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApproveInput<'a> {
    install_id: &'a str,
    subject_install_id: &'a str,
    kind: &'a str,
    signature: &'a str,
}

/// `installs.approve {kind: approve | reset | decline}`.
pub fn approve(trpc: &TrpcClient, install_id: &str, subject_install_id: &str, kind: &str, signature: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation("installs.approve", &ApproveInput { install_id, subject_install_id, kind, signature })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_input_is_camel_case_and_skips_absent_fields() {
        let input = RegisterInstall {
            kind: "daemon".into(),
            label: None,
            platform: Some("linux".into()),
            device_id: Some("dev-1".into()),
            endpoint_id: "ab".repeat(32),
            x25519_public: "x".into(),
            signed_at: "2026-10-30T12:00:00.000Z".into(),
            signature: "s".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["deviceId"], "dev-1");
        assert_eq!(json["endpointId"], "ab".repeat(32));
        assert_eq!(json["x25519Public"], "x");
        assert_eq!(json["signedAt"], "2026-10-30T12:00:00.000Z");
        assert!(json.get("label").is_none());
    }

    #[test]
    fn list_rows_decode() {
        let rows: Vec<InstallEntry> = serde_json::from_str(
            r#"[{"installId":"i","userId":"u","kind":"desktop","label":null,"platform":"macos",
                "endpointId":"00","x25519Public":"k","lastSeenAt":"2026-10-30T12:00:00.000Z"}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].install_id, "i");
        assert_eq!(rows[0].label, None);
    }
}
