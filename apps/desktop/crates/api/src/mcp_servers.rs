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
//! - `mcpServers.completeOAuth` is NOT here: the device answers an OAuth
//!   command through `devices.completeCommand` like every other command.

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

#[cfg(test)]
mod tests {
    use super::*;

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
