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
//! There is no read side here any more (EXP-485): the "My machines" rows
//! stream over the synced `devices` shape, and [`latest_versions`] is the
//! one query left — instance config (`CLIENT_LATEST_VERSION_*`) that sync
//! cannot carry. [`DeviceEntry`] stays as the shape the UI maps synced rows
//! into.

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
    /// EXP-481: this machine's launch defaults, applied server-side ONLY as
    /// a first-ever seed (the server copy is authoritative after that). The
    /// `coding::remote_admin::defaults_wire` JSON, passed as a raw value —
    /// `api` deliberately does not depend on `coding`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_defaults: Option<&'a serde_json::Value>,
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

/// `devices.register` output (EXP-481): the CURRENT server copy of the
/// launch defaults (post-seed) so the device converges immediately instead
/// of waiting a heartbeat. Every field defaulted — older servers answer
/// `{ok}` only.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub launch_defaults: Option<serde_json::Value>,
    #[serde(default)]
    pub launch_defaults_updated_at: Option<String>,
}

/// `devices.register` — upsert this machine for the signed-in user.
pub fn register(trpc: &TrpcClient, input: &RegisterDevice) -> Result<RegisterResult, ApiError> {
    trpc.mutation("devices.register", input)
}

/// The `{deviceId}` body shared by `remove` and `requestUpdate`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceIdInput<'a> {
    device_id: &'a str,
}

/// `devices.heartbeat` input (EXP-411): the live-session count rides every
/// beat so a pending update request can read "queued behind sessions"
/// instead of spinning forever. Older servers' zod strips the extra keys.
/// EXP-481: `defaults_synced_at` is the launch-defaults stamp this device
/// last converged to (`null` = never) — the server answers the current copy
/// only when it differs, keeping the steady-state beat tiny.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatInput<'a> {
    device_id: &'a str,
    active_sessions: u32,
    defaults_synced_at: Option<&'a str>,
}

/// EXP-481: one pending owner→device command riding the heartbeat response.
/// Rows stay pending server-side until `completeCommand` — redelivery on a
/// missed cycle is free idempotency, so executors must tolerate a repeat.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCommand {
    pub id: String,
    /// `worktree_remove` | `worktree_prune`; unknown kinds are completed
    /// `ok: false` ("unsupported") by the executor, never dropped silently.
    #[serde(default)]
    pub kind: String,
    /// `worktree_remove`: `{repoFullName, branch}`; `worktree_prune`: `{}`.
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// `devices.heartbeat` output: `ok: false` = the row is gone (removed from
/// the UI) — re-register; `update_requested` = the web "Update" button was
/// clicked — check for a new release, update + restart when one exists, and
/// re-register either way to consume the request. EXP-481: the beat is also
/// the device's WORK PULL — pending commands ride every response, and the
/// authoritative launch defaults ride it when the device's stamp is stale.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub update_requested: bool,
    #[serde(default)]
    pub commands: Vec<PendingCommand>,
    #[serde(default)]
    pub launch_defaults: Option<serde_json::Value>,
    #[serde(default)]
    pub launch_defaults_updated_at: Option<String>,
}

/// `devices.heartbeat` — bump `last_seen_at`, report the live-session count
/// (EXP-411) and pull pending work (EXP-481).
pub fn heartbeat(
    trpc: &TrpcClient,
    device_id: &str,
    active_sessions: u32,
    defaults_synced_at: Option<&str>,
) -> Result<HeartbeatResult, ApiError> {
    trpc.mutation(
        "devices.heartbeat",
        &HeartbeatInput {
            device_id,
            active_sessions,
            defaults_synced_at,
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

/// One rendered machine row. Since EXP-485 nothing decodes this off the
/// wire — the UI maps SYNCED `devices` rows into it — but every field past
/// `deviceId` stays defaulted so a narrower source reads as "unknown"
/// rather than blanking the row.
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
    /// EXP-481: the launch defaults — live advertisement when online, the
    /// persisted server copy when offline (older servers omit).
    #[serde(default)]
    pub launch_defaults: Option<serde_json::Value>,
    /// EXP-432: the team this device is shared with (`None` = private).
    #[serde(default)]
    pub shared_team_id: Option<String>,
    /// EXP-622: the caller's default machine — always false on a teammate's
    /// shared row (that flag is its owner's preference).
    #[serde(default)]
    pub is_default: bool,
    /// EXP-432: set only on teammates' shared rows — the device owner.
    #[serde(default)]
    pub owner: Option<DeviceOwner>,
}

/// The owning user of a teammate's shared row (EXP-432).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOwner {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
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

/// `devices.latestVersions` — query, no input. The device ROWS stream over
/// the `devices` shape (EXP-485); the only thing left that sync cannot carry
/// is this instance-level config pair, so it is its own tiny query.
pub fn latest_versions(trpc: &TrpcClient) -> Result<LatestVersions, ApiError> {
    trpc.query("devices.latestVersions")
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

/// `devices.setShared` (EXP-432) — share/unshare a SERVER device with a
/// team. `team_id: None` clears the share and MUST serialize as an explicit
/// JSON `null` (the server input is required-nullable) — no skip attribute.
pub fn set_shared(
    trpc: &TrpcClient,
    device_id: &str,
    team_id: Option<&str>,
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        team_id: Option<&'a str>,
    }
    let _: OkResult = trpc.mutation("devices.setShared", &Input { device_id, team_id })?;
    Ok(())
}

/// `devices.setDefault` (EXP-622) — mark one of the caller's OWN machines as
/// their default, the row every device picker prefills. The server clears the
/// flag on their other machines in the same transaction, so the result lands
/// through the `devices` shape rather than this response.
pub fn set_default(trpc: &TrpcClient, device_id: &str, is_default: bool) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        is_default: bool,
    }
    let _: OkResult = trpc.mutation(
        "devices.setDefault",
        &Input {
            device_id,
            is_default,
        },
    )?;
    Ok(())
}

/// `devices.setLaunchDefaults` output (EXP-481). `conflict: true` = the CAS
/// stamp was stale — the caller ADOPTS the returned server copy (server
/// wins offline-concurrent races, deterministically).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLaunchDefaultsResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub conflict: bool,
    #[serde(default)]
    pub launch_defaults: Option<serde_json::Value>,
    #[serde(default)]
    pub launch_defaults_updated_at: Option<String>,
}

/// The CAS arm of `devices.setLaunchDefaults` (EXP-481).
#[derive(Clone, Copy, Debug)]
pub enum ExpectedStamp<'a> {
    /// UI edit — unconditional last-write-wins (the field is omitted).
    Unconditional,
    /// Device push — expect exactly this server stamp (`None` = expect the
    /// column NULL); a mismatch answers `conflict` + the current copy.
    Expect(Option<&'a str>),
}

/// `devices.setLaunchDefaults` — write the server-authoritative launch
/// defaults. `launch_defaults` is the `coding::remote_admin::defaults_wire`
/// JSON (raw value — `api` does not depend on `coding`).
pub fn set_launch_defaults(
    trpc: &TrpcClient,
    device_id: &str,
    launch_defaults: &serde_json::Value,
    expected: ExpectedStamp,
) -> Result<SetLaunchDefaultsResult, ApiError> {
    let mut input = serde_json::json!({
        "deviceId": device_id,
        "launchDefaults": launch_defaults,
    });
    if let ExpectedStamp::Expect(stamp) = expected {
        input["expectedUpdatedAt"] = match stamp {
            Some(stamp) => serde_json::Value::String(stamp.to_string()),
            None => serde_json::Value::Null,
        };
    }
    trpc.mutation("devices.setLaunchDefaults", &input)
}

/// One row of `devices.reportWorktrees` (EXP-481) — the device's worktree
/// inventory, full current set per report (the server diff-upserts).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeReportEntry<'a> {
    pub repo_full_name: &'a str,
    pub branch: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_identifier: Option<&'a str>,
    /// `.exp-agents` marker ids; omitted = pre-marker worktree (any agent
    /// may resume).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<&'a [String]>,
    /// `clean` | `untracked` | `tracked` (the server degrades anything newer
    /// to `unknown`).
    pub dirty: &'a str,
    pub busy: bool,
}

/// `devices.reportWorktrees` — replace this device's persisted inventory.
pub fn report_worktrees(
    trpc: &TrpcClient,
    device_id: &str,
    worktrees: &[WorktreeReportEntry],
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        worktrees: &'a [WorktreeReportEntry<'a>],
    }
    let _: OkResult = trpc.mutation(
        "devices.reportWorktrees",
        &Input {
            device_id,
            worktrees,
        },
    )?;
    Ok(())
}

/// `devices.completeCommand` — report a pulled command's outcome. `ok:
/// false` in the RESULT means the row was already terminal (a redelivered
/// duplicate raced the first completion) — expected, not an error.
pub fn complete_command(
    trpc: &TrpcClient,
    command_id: &str,
    ok: bool,
    message: Option<&str>,
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        command_id: &'a str,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<&'a str>,
    }
    let _: OkResult = trpc.mutation(
        "devices.completeCommand",
        &Input {
            command_id,
            ok,
            message,
        },
    )?;
    Ok(())
}

/// `devices.createCommand` output.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CreatedCommand {
    #[serde(default)]
    pub id: String,
}

/// `devices.createCommand` (owner-side) — queue a `worktree_remove` (repo +
/// branch required) or `worktree_prune` against one of the CALLER's own
/// devices; the device picks it up on its next heartbeat (nudged when
/// online).
pub fn create_command(
    trpc: &TrpcClient,
    device_id: &str,
    kind: &str,
    repo_full_name: Option<&str>,
    branch: Option<&str>,
) -> Result<CreatedCommand, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        kind: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        repo_full_name: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<&'a str>,
    }
    trpc.mutation(
        "devices.createCommand",
        &Input {
            device_id,
            kind,
            repo_full_name,
            branch,
        },
    )
}

/// One `device_commands` row (`devices.getCommand` / `devices.listCommands`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRow {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    /// `pending` → `done` | `failed`.
    #[serde(default)]
    pub status: String,
    /// Device-reported message (prune summary, refusal reason).
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

impl CommandRow {
    pub fn is_terminal(&self) -> bool {
        self.status == "done" || self.status == "failed"
    }
}

/// `devices.getCommand` — the issuing UI's poll target while a command is in
/// flight.
pub fn get_command(trpc: &TrpcClient, command_id: &str) -> Result<CommandRow, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        command_id: &'a str,
    }
    trpc.query_with_input("devices.getCommand", &Input { command_id })
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
    fn latest_versions_queries_the_wire_path_and_tolerates_absent_values() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"desktop":"0.9.0","cli":"0.5.0"}}}"#,
        );
        let latest = latest_versions(&client(&base)).unwrap();
        assert_eq!(latest.desktop.as_deref(), Some("0.9.0"));
        assert_eq!(latest.cli.as_deref(), Some("0.5.0"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/devices.latestVersions HTTP/1.1"));

        // Unset server-side (and an older server answering a narrower shape):
        // every field must read as "unknown", never as a decode failure.
        let (base, _captured) =
            one_shot_server(200, r#"{"result":{"data":{"desktop":null}}}"#);
        let latest = latest_versions(&client(&base)).unwrap();
        assert_eq!(latest.desktop, None);
        assert_eq!(latest.cli, None);
    }

    #[test]
    fn heartbeat_posts_device_id_sessions_and_defaults_stamp() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ok":true,"updateRequested":false}}}"#,
        );
        let result = heartbeat(&client(&base), "dev-1", 2, None).unwrap();
        assert!(result.ok);
        assert!(!result.update_requested);
        assert!(result.commands.is_empty(), "older server: no commands field");
        assert!(result.launch_defaults.is_none());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.heartbeat HTTP/1.1"));
        // `defaultsSyncedAt` is ALWAYS present (null = never converged) —
        // the server includes defaults only on a stamp mismatch.
        assert!(request.ends_with(
            r#"{"deviceId":"dev-1","activeSessions":2,"defaultsSyncedAt":null}"#
        ));
    }

    #[test]
    fn heartbeat_decodes_commands_and_defaults() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ok":true,"updateRequested":false,"commands":[{"id":"cmd-1","kind":"worktree_remove","payload":{"repoFullName":"acme/web","branch":"exp/EXP-7"}},{"id":"cmd-2","kind":"worktree_prune","payload":{}}],"launchDefaults":{"defaultAgent":"codex"},"launchDefaultsUpdatedAt":"2026-08-11T10:00:00.000Z"}}}"#,
        );
        let result = heartbeat(&client(&base), "dev-1", 0, Some("2026-08-01T00:00:00.000Z")).unwrap();
        assert_eq!(result.commands.len(), 2);
        assert_eq!(result.commands[0].kind, "worktree_remove");
        assert_eq!(result.commands[0].payload["branch"], "exp/EXP-7");
        assert_eq!(result.commands[1].kind, "worktree_prune");
        assert_eq!(
            result.launch_defaults.as_ref().unwrap()["defaultAgent"],
            "codex"
        );
        assert_eq!(
            result.launch_defaults_updated_at.as_deref(),
            Some("2026-08-11T10:00:00.000Z")
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"deviceId":"dev-1","activeSessions":0,"defaultsSyncedAt":"2026-08-01T00:00:00.000Z"}"#
        ));
    }

    #[test]
    fn set_shared_serializes_the_clearing_null_explicitly() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_shared(&client(&base), "dev-1", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.setShared HTTP/1.1"));
        // The server input is required-nullable: the key MUST be present.
        assert!(request.ends_with(r#"{"deviceId":"dev-1","teamId":null}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_shared(&client(&base), "dev-1", Some("team-9")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"deviceId":"dev-1","teamId":"team-9"}"#));
    }

    #[test]
    fn set_launch_defaults_carries_the_cas_arm() {
        let defaults = serde_json::json!({"defaultAgent": "pi"});
        // Unconditional (UI edit): the CAS key is omitted entirely.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ok":true,"launchDefaults":{"defaultAgent":"pi"},"launchDefaultsUpdatedAt":"2026-08-11T10:00:00.000Z"}}}"#,
        );
        let result = set_launch_defaults(
            &client(&base),
            "dev-1",
            &defaults,
            ExpectedStamp::Unconditional,
        )
        .unwrap();
        assert!(result.ok && !result.conflict);
        assert_eq!(
            result.launch_defaults_updated_at.as_deref(),
            Some("2026-08-11T10:00:00.000Z")
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.setLaunchDefaults HTTP/1.1"));
        assert!(!request.contains("expectedUpdatedAt"));

        // Device push expecting NULL: explicit null on the wire.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ok":false,"conflict":true,"launchDefaults":{"defaultAgent":"claude"},"launchDefaultsUpdatedAt":"2026-08-12T09:00:00.000Z"}}}"#,
        );
        let result = set_launch_defaults(
            &client(&base),
            "dev-1",
            &defaults,
            ExpectedStamp::Expect(None),
        )
        .unwrap();
        assert!(result.conflict, "stale stamp answers the server copy to adopt");
        assert_eq!(
            result.launch_defaults.as_ref().unwrap()["defaultAgent"],
            "claude"
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains(r#""expectedUpdatedAt":null"#));
    }

    #[test]
    fn report_worktrees_serializes_the_inventory() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        let agents = vec!["claude".to_string()];
        report_worktrees(
            &client(&base),
            "dev-1",
            &[
                WorktreeReportEntry {
                    repo_full_name: "acme/web",
                    branch: "exp/EXP-7",
                    issue_identifier: Some("EXP-7"),
                    agents: Some(&agents),
                    dirty: "clean",
                    busy: true,
                },
                WorktreeReportEntry {
                    repo_full_name: "acme/web",
                    branch: "exp/batch-a1b2c3d4",
                    issue_identifier: None,
                    agents: None,
                    dirty: "untracked",
                    busy: false,
                },
            ],
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.reportWorktrees HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"deviceId":"dev-1","worktrees":[{"repoFullName":"acme/web","branch":"exp/EXP-7","issueIdentifier":"EXP-7","agents":["claude"],"dirty":"clean","busy":true},{"repoFullName":"acme/web","branch":"exp/batch-a1b2c3d4","dirty":"untracked","busy":false}]}"#
        ));
    }

    #[test]
    fn command_lifecycle_bindings_round_trip() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"id":"cmd-9"}}}"#);
        let created = create_command(
            &client(&base),
            "dev-1",
            "worktree_remove",
            Some("acme/web"),
            Some("exp/EXP-7"),
        )
        .unwrap();
        assert_eq!(created.id, "cmd-9");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.createCommand HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"deviceId":"dev-1","kind":"worktree_remove","repoFullName":"acme/web","branch":"exp/EXP-7"}"#
        ));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        complete_command(&client(&base), "cmd-9", false, Some("Uncommitted changes")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/devices.completeCommand HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"commandId":"cmd-9","ok":false,"message":"Uncommitted changes"}"#
        ));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"cmd-9","kind":"worktree_prune","payload":{},"status":"done","result":"Pruned 2 worktrees","completedAt":"2026-08-11T10:00:00.000Z","createdAt":"2026-08-11T09:59:00.000Z"}}}"#,
        );
        let row = get_command(&client(&base), "cmd-9").unwrap();
        assert!(row.is_terminal());
        assert_eq!(row.result.as_deref(), Some("Pruned 2 worktrees"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/devices.getCommand"));
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
