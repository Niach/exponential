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
        assert!(!format!("{key:?}").contains("expu_x"));
    }
}
