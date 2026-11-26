//! #EXP-2161 (`packages/domain-contract/fixtures/member-space.json`): the
//! typed `memberSpaces.*` mirrors the HOST uses, and the mode a space's own
//! daemon runs in.
//!
//! IN A SPACE ([`enter`], set once at the start of that process) the account
//! credential is a space key: it reaches exactly the fixture's `procedures`
//! and `shapes`. [`TrpcClient`] refuses every other procedure itself, so a
//! feature that is not a space's never reaches the network.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// A space `memberSpaces.hosted` lists for a host.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedSpace {
    /// The space's `devices` row id.
    pub space_id: String,
    /// `space-<uuid>`.
    pub device_id: String,
    #[serde(default)]
    pub member_id: String,
    #[serde(default)]
    pub member_name: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Deserialize)]
struct Hosted {
    spaces: Vec<HostedSpace>,
}

/// `memberSpaces.provision`'s answer: a fresh key, shown once.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provisioned {
    pub key: String,
    pub user_id: String,
    #[serde(default)]
    pub email: Option<String>,
    pub device_id: String,
    #[serde(default)]
    pub label: String,
}

impl std::fmt::Debug for Provisioned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Provisioned").field("key", &"<redacted>").field("device_id", &self.device_id).finish()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceInput<'a> {
    device_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceInput<'a> {
    space_id: &'a str,
}

/// `memberSpaces.hosted {deviceId}`: the spaces this host runs.
pub fn hosted(trpc: &TrpcClient, device_id: &str) -> Result<Vec<HostedSpace>, ApiError> {
    trpc.query_with_input::<_, Hosted>("memberSpaces.hosted", &DeviceInput { device_id }).map(|answer| answer.spaces)
}

/// `memberSpaces.provision {spaceId}`: a fresh space key (the earlier one is gone).
pub fn provision(trpc: &TrpcClient, space_id: &str) -> Result<Provisioned, ApiError> {
    trpc.mutation("memberSpaces.provision", &SpaceInput { space_id })
}

/// `memberSpaces.request`'s answer: the space row the Assistant now runs on.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requested {
    pub space_id: String,
    /// `space-<uuid>`: the Assistant's `agents.device_id` from now on.
    pub device_id: String,
    #[serde(default)]
    pub txid: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestInput<'a> {
    device_id: &'a str,
    team_id: &'a str,
}

/// `memberSpaces.request {deviceId, teamId}` (the HOST's `device_id`): run
/// the caller's Assistant on a team device; its running conversation is
/// detached and the next message starts a fresh one there.
pub fn request(trpc: &TrpcClient, host_device_id: &str, team_id: &str) -> Result<Requested, ApiError> {
    trpc.mutation("memberSpaces.request", &RequestInput { device_id: host_device_id, team_id })
}

/// `memberSpaces.mine`'s space: `starting` (its daemon has not registered),
/// `waiting` (for the member's approval) or `ready`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MySpace {
    pub space_id: String,
    pub device_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub host_device_id: String,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub owner_name: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub status: String,
}

/// `memberSpaces.mine`: the caller's own space (`None` = none) and whether
/// their Assistant lost its device (`ended`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mine {
    #[serde(default)]
    pub space: Option<MySpace>,
    #[serde(default)]
    pub ended: bool,
}

/// `memberSpaces.mine`.
pub fn mine(trpc: &TrpcClient) -> Result<Mine, ApiError> {
    trpc.query("memberSpaces.mine")
}

/// One member with a space on a device (`memberSpaces.forDevice`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceMember {
    pub space_id: String,
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub team_id: Option<String>,
}

#[derive(Deserialize)]
struct Members {
    members: Vec<SpaceMember>,
}

#[derive(Serialize)]
struct RowInput<'a> {
    id: &'a str,
}

/// `memberSpaces.forDevice {id}` (the host's `devices` ROW id): the members
/// running their Assistant there, for its owner or a team owner.
pub fn for_device(trpc: &TrpcClient, row_id: &str) -> Result<Vec<SpaceMember>, ApiError> {
    trpc.query_with_input::<_, Members>("memberSpaces.forDevice", &RowInput { id: row_id }).map(|answer| answer.members)
}

/// `memberSpaces.remove {spaceId}`: the device owner's, or a team owner's, Remove.
pub fn remove(trpc: &TrpcClient, space_id: &str) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation("memberSpaces.remove", &SpaceInput { space_id })?;
    Ok(())
}

// ── inside a space ────────────────────────────────────────────────────────

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// This process is a space's daemon (never undone).
pub fn enter() {
    ACTIVE.store(true, Ordering::SeqCst);
}

/// Whether this process is a space's daemon.
pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Allowed {
    procedures: HashSet<String>,
    shapes: HashSet<String>,
}

fn allowed() -> &'static Allowed {
    static ALLOWED: OnceLock<Allowed> = OnceLock::new();
    ALLOWED.get_or_init(|| {
        // Compiled in and gated by the test below: it always decodes.
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/member-space.json"))
            .unwrap_or(Allowed { procedures: HashSet::new(), shapes: HashSet::new() })
    })
}

/// Whether a space key reaches `path` (`devices.heartbeat`).
pub fn procedure_allowed(path: &str) -> bool {
    allowed().procedures.contains(path)
}

/// Whether a space key syncs `shape` (a table name).
pub fn shape_allowed(shape: &str) -> bool {
    allowed().shapes.contains(shape)
}

/// In a space: the refusal of a procedure its key does not reach, without a
/// request (the server's own answer).
pub(crate) fn refuse(path: &str) -> Result<(), ApiError> {
    if !active() || procedure_allowed(path) {
        return Ok(());
    }
    log::debug!("{path}: not in a member space");
    Err(ApiError::Http { status: 403, message: "Not available in a member space".to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowlist_is_the_fixtures() {
        assert!(procedure_allowed("devices.heartbeat"));
        assert!(procedure_allowed("sessions.reply"));
        for refused in ["memberSpaces.provision", "memberSpaces.hosted", "installs.approve", "users.listPersonalApiKeys", ""] {
            assert!(!procedure_allowed(refused), "{refused}");
        }
        assert!(shape_allowed("messages"));
        assert!(!shape_allowed("devices"));
    }

    #[test]
    fn the_answers_decode() {
        let hosted: Hosted = serde_json::from_str(
            r#"{"spaces":[{"spaceId":"s","deviceId":"space-1","memberId":"u","memberName":"Ada","label":"box","later":1}]}"#,
        )
        .unwrap();
        assert_eq!(hosted.spaces[0].member_name, "Ada");
        let key: Provisioned = serde_json::from_str(r#"{"key":"expu_x","userId":"u","deviceId":"space-1"}"#).unwrap();
        let mine: Mine = serde_json::from_str(
            r#"{"space":{"spaceId":"s","deviceId":"space-1","label":"unraid","hostDeviceId":"srv","ownerId":"u2","ownerName":"Mira","teamId":"t1","teamIds":["t1"],"status":"waiting","endpointId":null},"ended":false}"#,
        )
        .unwrap();
        assert_eq!(mine.space.as_ref().map(|space| space.status.as_str()), Some("waiting"));
        let none: Mine = serde_json::from_str(r#"{"space":null,"ended":true}"#).unwrap();
        assert!(none.space.is_none() && none.ended);
        let members: Members = serde_json::from_str(r#"{"members":[{"spaceId":"s","userId":"u","name":"Ada","teamId":null}]}"#).unwrap();
        assert_eq!(members.members[0].name, "Ada");
        let requested: Requested = serde_json::from_str(r#"{"spaceId":"s","deviceId":"space-1","txid":"12"}"#).unwrap();
        assert_eq!(requested.device_id, "space-1");
        assert!(!format!("{key:?}").contains("expu_x"));
    }
}
