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

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `devices.register` input. `kind` is `"desktop"` or `"server"`;
/// `platform` is `std::env::consts::OS` (`linux`/`macos`/`windows`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDevice<'a> {
    pub device_id: &'a str,
    pub label: &'a str,
    pub kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<&'a str>,
    pub agents: &'a [String],
    pub caps: &'a [String],
}

#[derive(Debug, Deserialize)]
struct OkResult {
    #[serde(default)]
    ok: bool,
}

/// `devices.register` — upsert this machine for the signed-in user.
pub fn register(trpc: &TrpcClient, input: &RegisterDevice) -> Result<(), ApiError> {
    let _: OkResult = trpc.mutation("devices.register", input)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatInput<'a> {
    device_id: &'a str,
}

/// `devices.heartbeat` — bump `last_seen_at`. Returns `false` when the row
/// no longer exists (removed from the UI) — re-register then.
pub fn heartbeat(trpc: &TrpcClient, device_id: &str) -> Result<bool, ApiError> {
    let result: OkResult = trpc.mutation("devices.heartbeat", &HeartbeatInput { device_id })?;
    Ok(result.ok)
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
