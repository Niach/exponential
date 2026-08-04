//! Typed `devices.*` tRPC mirrors (EXP-403 registered devices). Shapes
//! verified against `apps/web/src/lib/trpc/devices.ts`.
//!
//! Desktops call [`register`] from control-channel start (kind `desktop`);
//! the headless `exponential` daemon registers as kind `server` and then
//! [`heartbeat`]s to keep `last_seen_at` fresh — `ok: false` from a
//! heartbeat means the row was removed in the UI and the caller should
//! re-register. Registration is best-effort everywhere: an older server
//! without the router must never break control-channel start (callers
//! ignore the error).
//!
//! [`list`] is the read side behind the "My machines" section (all four
//! clients render it): the registry merged with live relay presence, so a
//! row carries both durable identity (label, kind, last seen, version) and
//! the live advertisement (online, agents, caps).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `devices.register` input. `kind` is `"desktop"` or `"server"`;
/// `platform` is `std::env::consts::OS` (`linux`/`macos`/`windows`);
/// `version` is the marketing version (registering also CONSUMES a pending
/// web "Update" request server-side).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDevice<'a> {
    pub device_id: &'a str,
    pub label: &'a str,
    pub kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<&'a str>,
    /// Runnable agents (installed AND signed in since EXP-409).
    pub agents: &'a [String],
    /// EXP-409: installed but signed out. Skipped when empty so older
    /// servers with a strict input schema never see the field.
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    pub unauthed_agents: &'a [String],
    pub caps: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct OkResult {
    // Decoded for envelope validation; register/rename callers don't branch
    // on it (their failures surface as HTTP errors).
    #[serde(default)]
    #[allow(dead_code)]
    ok: bool,
}

/// `devices.register` — upsert this machine for the signed-in user.
pub fn register(trpc: &TrpcClient, input: &RegisterDevice) -> Result<(), ApiError> {
    let _: OkResult = trpc.mutation("devices.register", input)?;
    Ok(())
}

/// The `{deviceId}` body shared by `remove` and `requestUpdate`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceIdInput<'a> {
    device_id: &'a str,
}

/// `devices.heartbeat` input (EXP-411): the live-session count rides every
/// beat so a pending update request can read "queued behind sessions"
/// instead of spinning forever. Older servers' zod strips the extra key.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatInput<'a> {
    device_id: &'a str,
    active_sessions: u32,
}

/// `devices.heartbeat` output: `ok: false` = the row is gone (removed from
/// the UI) — re-register; `update_requested` = the web "Update" button was
/// clicked — check for a new release, update + restart when one exists, and
/// re-register either way to consume the request.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub update_requested: bool,
}

/// `devices.heartbeat` — bump `last_seen_at` and report the live-session
/// count (EXP-411).
pub fn heartbeat(
    trpc: &TrpcClient,
    device_id: &str,
    active_sessions: u32,
) -> Result<HeartbeatResult, ApiError> {
    trpc.mutation(
        "devices.heartbeat",
        &HeartbeatInput {
            device_id,
            active_sessions,
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameInput<'a> {
    device_id: &'a str,
    label: &'a str,
}

/// `devices.rename` — the EXPLICIT label write. `register` deliberately
/// never overwrites an existing row's label (a re-register must not stomp a
/// user's rename with the hostname default), so the daemon's `--label` flag
/// lands through here.
pub fn rename(trpc: &TrpcClient, device_id: &str, label: &str) -> Result<(), ApiError> {
    let _: OkResult = trpc.mutation("devices.rename", &RenameInput { device_id, label })?;
    Ok(())
}

/// One row of `devices.list`. Every field past `deviceId` is defaulted: an
/// older server answers a narrower shape, and a missing field must read as
/// "unknown", never as a decode failure that blanks the whole list.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEntry {
    pub device_id: String,
    #[serde(default)]
    pub device_label: String,
    /// `"desktop"` or `"server"` — an unknown value renders as a desktop.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// EXP-409: installed-but-signed-out agents — shown with a "sign in"
    /// hint, never offered in pickers.
    #[serde(default)]
    pub unauthed_agents: Vec<String>,
    #[serde(default)]
    pub caps: Vec<String>,
    /// Connected to the steer relay right now.
    #[serde(default)]
    pub online: bool,
    /// ISO timestamp of the last register/heartbeat; `None` for a relay-only
    /// device that predates the registry.
    #[serde(default)]
    pub last_seen_at: Option<String>,
    /// `false` = relay presence only, so it has no registry row to rename,
    /// remove or update.
    #[serde(default)]
    pub registered: bool,
    #[serde(default)]
    pub version: Option<String>,
    /// An Update click is pending — the device consumes it on its next
    /// register.
    #[serde(default)]
    pub update_requested: bool,
    /// EXP-411: the pending request is parked behind live coding sessions on
    /// the machine — render "Update queued", not an endless spinner.
    #[serde(default)]
    pub update_blocked: bool,
}

impl DeviceEntry {
    /// The headless `exponential` daemon (the only kind that self-updates).
    pub fn is_server(&self) -> bool {
        self.kind == "server"
    }
}

/// Informational `CLIENT_LATEST_VERSION_*` values (`None` when unset
/// server-side).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersions {
    #[serde(default)]
    pub desktop: Option<String>,
    #[serde(default)]
    pub cli: Option<String>,
}

/// `devices.list` output.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    #[serde(default)]
    pub devices: Vec<DeviceEntry>,
    #[serde(default)]
    pub latest_versions: LatestVersions,
}

/// `devices.list` — query, no input. Per-USER (never team-scoped), and
/// deliberately tRPC rather than an Electric shape: machine state is not
/// team product data.
pub fn list(trpc: &TrpcClient) -> Result<DeviceList, ApiError> {
    trpc.query("devices.list")
}

/// `devices.remove` — drop the registry row. A still-running daemon
/// re-registers on its next heartbeat, and a live relay connection is
/// untouched.
pub fn remove(trpc: &TrpcClient, device_id: &str) -> Result<(), ApiError> {
    let _: OkResult = trpc.mutation("devices.remove", &DeviceIdInput { device_id })?;
    Ok(())
}

/// `devices.requestUpdate` — flag the device; its next heartbeat picks the
/// request up, self-updates when a newer release exists, and its following
/// register consumes the flag either way.
pub fn request_update(trpc: &TrpcClient, device_id: &str) -> Result<(), ApiError> {
    let _: OkResult = trpc.mutation("devices.requestUpdate", &DeviceIdInput { device_id })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn list_decodes_rows_and_latest_versions() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"devices":[{"deviceId":"dev-1","deviceLabel":"tower","kind":"server","platform":"linux","agents":["claude"],"caps":["actions"],"online":true,"lastSeenAt":"2026-08-03T10:11:12.000Z","registered":true,"version":"0.4.1","updateRequested":true,"updateBlocked":true}],"latestVersions":{"desktop":"0.9.0","cli":"0.5.0"}}}}"#,
        );
        let list = list(&client(&base)).unwrap();
        assert_eq!(list.devices.len(), 1);
        let device = &list.devices[0];
        assert_eq!(device.device_id, "dev-1");
        assert_eq!(device.device_label, "tower");
        assert!(device.is_server());
        assert_eq!(device.platform.as_deref(), Some("linux"));
        assert_eq!(device.agents, vec!["claude".to_string()]);
        assert!(device.online);
        assert_eq!(device.last_seen_at.as_deref(), Some("2026-08-03T10:11:12.000Z"));
        assert!(device.registered);
        assert_eq!(device.version.as_deref(), Some("0.4.1"));
        assert!(device.update_requested);
        assert!(device.update_blocked);
        assert_eq!(list.latest_versions.desktop.as_deref(), Some("0.9.0"));
        assert_eq!(list.latest_versions.cli.as_deref(), Some("0.5.0"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/devices.list HTTP/1.1"));
    }

    #[test]
    fn list_tolerates_a_narrow_row_and_absent_versions() {
        // An older server (or a relay-only row) answers without the newer
        // fields — the list must still decode instead of reading as empty.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"devices":[{"deviceId":"dev-2","deviceLabel":"laptop","kind":"desktop","platform":null,"online":false,"lastSeenAt":null,"registered":false}]}}}"#,
        );
        let list = list(&client(&base)).unwrap();
        let device = &list.devices[0];
        assert!(!device.is_server());
        assert!(device.agents.is_empty());
        assert_eq!(device.version, None);
        assert!(!device.update_requested);
        assert!(!device.update_blocked);
        assert_eq!(list.latest_versions.cli, None);
    }

    #[test]
    fn heartbeat_posts_device_id_and_active_sessions() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ok":true,"updateRequested":false}}}"#,
        );
        let result = heartbeat(&client(&base), "dev-1", 2).unwrap();
        assert!(result.ok);
        assert!(!result.update_requested);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.heartbeat HTTP/1.1"));
        assert!(request.ends_with(r#"{"deviceId":"dev-1","activeSessions":2}"#));
    }

    #[test]
    fn remove_posts_device_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        remove(&client(&base), "dev-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.remove HTTP/1.1"));
        assert!(request.ends_with(r#"{"deviceId":"dev-1"}"#));
    }

    #[test]
    fn request_update_posts_device_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        request_update(&client(&base), "dev-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.requestUpdate HTTP/1.1"));
        assert!(request.ends_with(r#"{"deviceId":"dev-1"}"#));
    }
}
