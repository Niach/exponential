//! EXP-792: typed `mcpServers.*` tRPC helpers — the device side of team MCP
//! servers. The server holds NON-SECRET config only (names, url, header /
//! env NAMES, scopes, auth kind) plus a per-device readiness matrix; every
//! credential lives in this machine's 0600 secret store
//! ([`crate::token_store`]), so nothing here ever carries a value.
//!
//! Shapes mirror `apps/web/src/lib/trpc/mcp-servers.ts`:
//!
//! - `mcpServers.listForDevice` — **query** — every server of every team the
//!   caller belongs to (the launcher resolves a run's `mcp_server_ids`
//!   against it; the readiness reporter walks it on the heartbeat).
//! - `mcpServers.list({teamId})` — **query** — one team's servers joined
//!   with the readiness rows of the devices visible to the caller (the
//!   desktop settings pane).
//! - `mcpServers.reportReadiness` — **mutation** — this device's readiness
//!   per server (also folded into `devices.heartbeat` as `mcpReadiness`).
//! - `mcpServers.finishOAuth` — **mutation** — the LOOPBACK sign-in's
//!   completion: no `mcp_oauth_code` command exists on that path (the code
//!   lands on the device's own listener), so the device reports the flow's
//!   outcome by `state`. The hosted path completes through
//!   `devices.completeCommand` like every other command (and may call this
//!   too; it is idempotent).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `mcp_servers` row (contract `mcpTransport` / `mcpAuth` vocabularies).
/// Every list field defaults so an older server's thinner row still parses.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    #[serde(default)]
    pub team_id: String,
    pub name: String,
    /// `http` | `stdio`.
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    /// Header NAMES the device supplies values for (`http`).
    #[serde(default)]
    pub header_names: Vec<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Env NAMES the device supplies values for (`stdio`).
    #[serde(default)]
    pub env_names: Vec<String>,
    /// Advisory OAuth scopes to request.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// `none` | `oauth` | `secret`.
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub enabled_by_default: bool,
}

fn default_transport() -> String {
    "http".to_string()
}

fn default_auth() -> String {
    "none".to_string()
}

impl McpServerConfig {
    pub fn is_http(&self) -> bool {
        self.transport != "stdio"
    }

    pub fn is_oauth(&self) -> bool {
        self.auth == "oauth"
    }

    /// The device-typed secret positions (`auth: secret`): every declared
    /// header name (http) or env name (stdio). An OAuth server has none —
    /// its token set is one entry keyed by the server id.
    pub fn secret_names(&self) -> &[String] {
        if self.is_http() {
            &self.header_names
        } else {
            &self.env_names
        }
    }
}

/// `mcpServers.listForDevice` — every server the signed-in user may connect
/// to, across their teams.
pub fn list_for_device(trpc: &TrpcClient) -> Result<Vec<McpServerConfig>, ApiError> {
    trpc.query("mcpServers.listForDevice")
}

/// One device's readiness for one server, as the server stores it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpReadinessRow {
    pub server_id: String,
    /// The `devices` row id (NOT the steer deviceId).
    pub device_row_id: String,
    /// The steer deviceId of that row, when the server joined it.
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub checked_at: Option<String>,
}

/// `mcpServers.list` entry: the config plus every visible device's readiness.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerListEntry {
    #[serde(flatten)]
    pub config: McpServerConfig,
    #[serde(default)]
    pub readiness: Vec<McpReadinessRow>,
}

/// `mcpServers.list({teamId})`.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<McpServerListEntry>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("mcpServers.list", &Input { team_id })
}

/// One readiness report entry (`mcpServers.reportReadiness` +
/// `devices.heartbeat.mcpReadiness`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpReadinessReport {
    pub server_id: String,
    pub ready: bool,
    /// ISO timestamp of the access token's expiry (OAuth only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Why not ready (`no credential on this device`, `refresh failed: …`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The OWNER-write field set (`mcpServers.create` / `mcpServers.update`).
///
/// Every field is optional because `update` is a PATCH: the server merges it
/// onto the stored row and validates the MERGED result, so flipping `auth` to
/// `secret` alone still has to find exactly one declared header/env name.
/// `create` runs the same validator on a NON-partial input, so `name`,
/// `transport` and `auth` must be present there — the server answers
/// `BAD_REQUEST` otherwise, and this type does not pretend to know better.
///
/// The cross-field rules are the router's, not ours (`normalizeFields` in
/// `apps/web/src/lib/trpc/mcp-servers.ts`): an http server needs a `url` and
/// a stdio one a `command`; `oauth` is http-only; `secret` declares exactly
/// one name on the transport's side. A blank/`None` field is simply absent
/// from the wire, never `null`.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `http` | `stdio`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    /// `none` | `oauth` | `secret`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
}

/// `mcpServers.create` — owner-only. Returns the stored row.
pub fn create(
    trpc: &TrpcClient,
    team_id: &str,
    fields: &McpServerFields,
) -> Result<McpServerConfig, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        #[serde(flatten)]
        fields: &'a McpServerFields,
    }
    trpc.mutation("mcpServers.create", &Input { team_id, fields })
}

/// `mcpServers.update` — owner-only PATCH. Returns the stored row.
pub fn update(
    trpc: &TrpcClient,
    id: &str,
    fields: &McpServerFields,
) -> Result<McpServerConfig, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        #[serde(flatten)]
        fields: &'a McpServerFields,
    }
    trpc.mutation("mcpServers.update", &Input { id, fields })
}

/// `mcpServers.remove` — owner-only. Readiness rows and OAuth flows cascade
/// with the server row; the CREDENTIALS every device holds do not, so a
/// caller that also wants them gone runs
/// `coding::mcp_servers::forget_server` on this machine.
pub fn remove(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("mcpServers.remove", &Input { id })?;
    Ok(())
}

/// `mcpServers.reportReadiness` — replace this device's readiness rows for
/// the listed servers (servers absent from `entries` are left alone).
pub fn report_readiness(
    trpc: &TrpcClient,
    device_id: &str,
    entries: &[McpReadinessReport],
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        entries: &'a [McpReadinessReport],
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation(
        "mcpServers.reportReadiness",
        &Input { device_id, entries },
    )?;
    Ok(())
}

/// `mcpServers.finishOAuth` — mark the flow identified by `state` done
/// (`ok`, with the token's expiry) or failed (`error`). On success the server
/// also upserts this device's readiness `ready=true` for the flow's server.
pub fn finish_oauth(
    trpc: &TrpcClient,
    state: &str,
    ok: bool,
    expires_at: Option<&str>,
    error: Option<&str>,
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        state: &'a str,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation(
        "mcpServers.finishOAuth",
        &Input {
            state,
            ok,
            expires_at,
            error,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn finish_oauth_posts_state_and_skips_absent_optionals() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        finish_oauth(&client(&base), "st-1", true, Some("2026-09-09T10:00:00.000Z"), None)
            .expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.finishOAuth"));
        assert!(request.contains(r#"{"state":"st-1","ok":true,"expiresAt":"2026-09-09T10:00:00.000Z"}"#));
    }

    #[test]
    fn report_readiness_sends_the_entries_under_the_device_id() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        report_readiness(
            &client(&base),
            "dev-1",
            &[McpReadinessReport {
                server_id: "s1".into(),
                ready: false,
                expires_at: None,
                error: Some("not signed in on this machine".into()),
            }],
        )
        .expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.reportReadiness"));
        assert!(request.contains(
            r#"{"deviceId":"dev-1","entries":[{"serverId":"s1","ready":false,"error":"not signed in on this machine"}]}"#
        ));
    }

    /// The create wire is `{teamId, ...fields}` — a flattened field set, so
    /// the router's `fieldsSchema.extend({teamId})` sees ONE flat object.
    #[test]
    fn create_flattens_the_fields_beside_the_team_id() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"s1","name":"Linear","transport":"http","auth":"oauth"}}}"#,
        );
        let row = create(
            &client(&base),
            "team-1",
            &McpServerFields {
                name: Some("Linear".into()),
                transport: Some("http".into()),
                url: Some("https://mcp.linear.app/mcp".into()),
                auth: Some("oauth".into()),
                enabled_by_default: Some(true),
                ..Default::default()
            },
        )
        .expect("ok");
        assert_eq!(row.id, "s1");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.create"));
        assert!(request.contains(
            r#"{"teamId":"team-1","name":"Linear","transport":"http","url":"https://mcp.linear.app/mcp","auth":"oauth","enabledByDefault":true}"#
        ));
    }

    /// An update is a PATCH: absent fields never reach the wire as `null`,
    /// which is what lets the server validate the MERGED row.
    #[test]
    fn update_sends_only_the_named_fields() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"s1","name":"Linear"}}}"#,
        );
        update(
            &client(&base),
            "s1",
            &McpServerFields {
                enabled_by_default: Some(false),
                ..Default::default()
            },
        )
        .expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.update"));
        assert!(request.contains(r#"{"id":"s1","enabledByDefault":false}"#));
    }

    #[test]
    fn remove_posts_the_id() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        remove(&client(&base), "s1").expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.remove"));
        assert!(request.contains(r#"{"id":"s1"}"#));
    }

    #[test]
    fn config_parses_a_thin_row_with_defaults() {
        let row: McpServerConfig =
            serde_json::from_str(r#"{"id":"s1","name":"Linear"}"#).expect("parse");
        assert_eq!(row.transport, "http");
        assert_eq!(row.auth, "none");
        assert!(row.header_names.is_empty());
        assert!(row.is_http());
        assert!(!row.is_oauth());
    }

    #[test]
    fn secret_names_follow_the_transport() {
        let http = McpServerConfig {
            header_names: vec!["X-Api-Key".into()],
            env_names: vec!["IGNORED".into()],
            ..Default::default()
        };
        assert_eq!(http.secret_names(), ["X-Api-Key".to_string()]);
        let stdio = McpServerConfig {
            transport: "stdio".into(),
            header_names: vec!["IGNORED".into()],
            env_names: vec!["GITHUB_TOKEN".into()],
            ..Default::default()
        };
        assert_eq!(stdio.secret_names(), ["GITHUB_TOKEN".to_string()]);
    }

    #[test]
    fn readiness_report_skips_absent_optionals() {
        let entry = McpReadinessReport {
            server_id: "s1".into(),
            ready: true,
            expires_at: None,
            error: None,
        };
        assert_eq!(
            serde_json::to_string(&entry).unwrap(),
            r#"{"serverId":"s1","ready":true}"#
        );
    }
}
