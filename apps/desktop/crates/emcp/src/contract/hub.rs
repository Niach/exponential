//! The local hub's rules (`packages/emcp/fixtures/hub.json`, #EXP-1507):
//! server names, the run's tool list under its grant, routing a call, and
//! the `mcp_calls` row each call writes. The tool naming is exposes.json's
//! (the server name in the slug's place), the visibility rule access.json's
//! and the row audit.json's.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use super::access::{tool_visible, Level};
use super::audit::{args_hash, CallStatus};
use super::exposes::{list_tool_names, split_tool_name, upstream_access, ListedTool, SEPARATOR};

/// A mounted server's name: at most this many characters.
pub const SERVER_NAME_MAX: usize = 32;

/// `^[a-z0-9][a-z0-9-]*$`, at most [`SERVER_NAME_MAX`].
pub fn server_name_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    name.len() <= SERVER_NAME_MAX
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// What a run may reach: a level per mounted server (a server not named
/// = `none`), and who it acts for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubGrant {
    pub servers: BTreeMap<String, Level>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
}

impl HubGrant {
    pub fn level(&self, server: &str) -> Level {
        self.servers.get(server).copied().unwrap_or(Level::None)
    }
}

/// The run's tools/list: every tool its grant lets it see, qualified
/// `<server>__<tool>`. `servers` = (name, its MCP tool objects).
pub fn tool_list(grant: &HubGrant, servers: &[(String, Vec<Value>)]) -> Vec<ListedTool> {
    let visible: Vec<(String, Vec<String>)> = servers
        .iter()
        .map(|(server, tools)| {
            let level = grant.level(server);
            let names = tools
                .iter()
                .filter_map(|tool| {
                    let name = tool.get("name").and_then(Value::as_str)?;
                    let access = upstream_access(tool.get("annotations"));
                    (!name.contains(SEPARATOR) && tool_visible(level, access)).then(|| name.to_string())
                })
                .collect();
            (server.clone(), names)
        })
        .collect();
    list_tool_names(&visible).into_iter().filter(|listed| split_tool_name(&listed.name).is_some()).collect()
}

/// Where a call goes: a name on the run's own list as (server, tool),
/// else `None` (unknown to this run).
pub fn route(grant: &HubGrant, servers: &[(String, Vec<Value>)], name: &str) -> Option<(String, String)> {
    tool_list(grant, servers).into_iter().find(|listed| listed.name == name).map(|listed| (listed.app, listed.tool))
}

/// What happened to one hub call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubOutcome {
    Ok,
    IsError,
    InvalidParams,
    RpcError,
    Unreachable,
    Timeout,
    Unknown,
}

impl HubOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            HubOutcome::Ok => "ok",
            HubOutcome::IsError => "isError",
            HubOutcome::InvalidParams => "invalidParams",
            HubOutcome::RpcError => "rpcError",
            HubOutcome::Unreachable => "unreachable",
            HubOutcome::Timeout => "timeout",
            HubOutcome::Unknown => "unknown",
        }
    }

    pub fn parse(text: &str) -> Option<HubOutcome> {
        [
            HubOutcome::Ok,
            HubOutcome::IsError,
            HubOutcome::InvalidParams,
            HubOutcome::RpcError,
            HubOutcome::Unreachable,
            HubOutcome::Timeout,
            HubOutcome::Unknown,
        ]
        .into_iter()
        .find(|outcome| outcome.as_str() == text)
    }

    /// The row's `status`.
    pub fn status(self) -> CallStatus {
        match self {
            HubOutcome::Ok => CallStatus::Ok,
            HubOutcome::IsError => CallStatus::Failed,
            HubOutcome::InvalidParams => CallStatus::Invalid,
            HubOutcome::RpcError | HubOutcome::Unreachable | HubOutcome::Timeout => CallStatus::Upstream,
            HubOutcome::Unknown => CallStatus::Denied,
        }
    }

    /// The row's `error` code.
    pub fn error(self) -> Option<&'static str> {
        match self {
            HubOutcome::Ok | HubOutcome::IsError => None,
            HubOutcome::InvalidParams => Some("invalidArguments"),
            HubOutcome::RpcError | HubOutcome::Unreachable => Some("upstreamFailed"),
            HubOutcome::Timeout => Some("upstreamTimeout"),
            HubOutcome::Unknown => Some("unknownTool"),
        }
    }
}

/// One `mcp_calls` row (audit.json's columns).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRow {
    pub id: String,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    /// The run's session id: on a device the run is the client.
    pub client_id: String,
    /// Always `None` on the hub (no app tool runs here).
    pub app_id: Option<String>,
    pub tool: String,
    pub status: CallStatus,
    pub error: Option<&'static str>,
    pub latency_ms: u64,
    pub args_hash: String,
    pub created_at: String,
}

impl AuditRow {
    /// The row by its column names, as the server stores it.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "team_id": self.team_id,
            "user_id": self.user_id,
            "client_id": self.client_id,
            "app_id": self.app_id,
            "tool": self.tool,
            "status": self.status.as_str(),
            "error": self.error,
            "latency_ms": self.latency_ms,
            "args_hash": self.args_hash,
            "created_at": self.created_at,
        })
    }
}

/// One hub call's row.
pub struct RowInput<'a> {
    pub id: String,
    pub grant: &'a HubGrant,
    pub run: &'a str,
    pub name: &'a str,
    pub args: Option<&'a Value>,
    pub outcome: HubOutcome,
    pub latency_ms: u64,
    pub created_ms: i64,
}

pub fn audit_row(input: RowInput<'_>) -> AuditRow {
    AuditRow {
        id: input.id,
        team_id: input.grant.team_id.clone(),
        user_id: input.grant.user_id.clone(),
        client_id: input.run.to_string(),
        app_id: None,
        tool: input.name.to_string(),
        status: input.outcome.status(),
        error: input.outcome.error(),
        latency_ms: input.latency_ms,
        args_hash: args_hash(input.args),
        created_at: format_utc_ms(input.created_ms),
    }
}

/// Unix ms → `YYYY-MM-DDTHH:MM:SS.fffZ` (the inverse of
/// [`super::audit::parse_utc_ms`]).
pub fn format_utc_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rest = ms.rem_euclid(86_400_000);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    let (hh, mm, ss, fff) = (rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{fff:03}Z")
}
