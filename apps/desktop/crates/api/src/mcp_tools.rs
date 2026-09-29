//! EXP-1110: typed calls into the instance's OWN MCP tools (`POST /api/mcp`)
//! for the reads and writes the CLI needs but no tRPC procedure serves.
//!
//! Devices and coding sessions reach every GUI client over the Electric
//! shapes; there is deliberately no tRPC `list` for either. The MCP tools
//! `exponential_devices_list` / `exponential_sessions_{list,get,start,
//! message,kill}` are the server-side reads over the SAME predicates the
//! shapes use (membership, trash/archive, grant scope), and `sessions_message`
//! is the only server path that posts steer input to a live run
//! (`relayPostInput`). A one-shot CLI command therefore calls them instead of
//! syncing a shape.
//!
//! Wire: the route runs a STATELESS streamable-HTTP transport with JSON
//! responses (`sessionIdGenerator: undefined, enableJsonResponse: true`), so a
//! bare `tools/call` needs no `initialize` handshake and no session header.
//! The transport refuses a request whose `Accept` does not list both
//! `application/json` and `text/event-stream` (406). A tool result is
//! `{content: [{type: "text", text}], isError?}`: the text is the tool's
//! `JSON.stringify` output on success and the error sentence on failure.
//!
//! The bearer is the account's own credential (session token or `expu_` key)
//! — full membership access, exactly like the web session. No
//! `X-Exp-Session-Id` header rides these calls: a person at a terminal is not
//! a run, so the self-kill/self-message guards never apply.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::devices::DeviceOwner;
use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `sessions_start` polls up to 10s for the device's row, so the ordinary 30s
/// budget is tight on a slow link; one generous budget for every call.
const MCP_TIMEOUT: Duration = Duration::from_secs(60);

/// What the transport requires (406 without both).
const MCP_ACCEPT: &str = "application/json, text/event-stream";

/// The MCP tools' page cap (`pageInput.limit.max`).
pub const MAX_PAGE: u32 = 200;

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: RpcParams<'a>,
}

#[derive(Serialize)]
struct RpcParams<'a> {
    name: &'a str,
    arguments: &'a Value,
}

#[derive(Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<ToolResult>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    #[serde(default)]
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolResult {
    #[serde(default)]
    content: Vec<ToolContent>,
    #[serde(default)]
    is_error: bool,
}

#[derive(Deserialize)]
struct ToolContent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

/// The JSON-RPC body for one `tools/call` (pure, unit-tested).
pub fn tool_call_body(name: &str, arguments: &Value) -> String {
    serde_json::to_string(&RpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: RpcParams { name, arguments },
    })
    .unwrap_or_default()
}

/// Decode a `tools/call` response: the tool's JSON output, or its error
/// sentence as [`ApiError::Http`] (status 400 — the server answered, the tool
/// refused). A success text that is not JSON comes back as a JSON string.
pub fn parse_tool_response(body: &str) -> Result<Value, ApiError> {
    let response: RpcResponse =
        serde_json::from_str(body).map_err(|e| ApiError::Decode(format!("mcp tools/call: {e}")))?;
    if let Some(error) = response.error {
        return Err(ApiError::Http {
            status: 400,
            message: error.message,
        });
    }
    let result = response
        .result
        .ok_or_else(|| ApiError::Decode("mcp tools/call: no result".to_string()))?;
    let text = result
        .content
        .iter()
        .filter(|content| content.kind == "text")
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if result.is_error {
        return Err(ApiError::Http {
            status: 400,
            message: text,
        });
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// One `tools/call` against `/api/mcp`, decoded to the tool's JSON output.
pub fn call_tool(trpc: &TrpcClient, name: &str, arguments: &Value) -> Result<Value, ApiError> {
    let body = trpc.post_json(
        "/api/mcp",
        &tool_call_body(name, arguments),
        MCP_ACCEPT,
        MCP_TIMEOUT,
    )?;
    parse_tool_response(&body)
}

fn call_typed<T: serde::de::DeserializeOwned>(
    trpc: &TrpcClient,
    name: &str,
    arguments: &Value,
) -> Result<T, ApiError> {
    let value = call_tool(trpc, name, arguments)?;
    serde_json::from_value(value).map_err(|e| ApiError::Decode(format!("{name}: {e}")))
}

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

/// One `exponential_devices_list` row. `online` is the server's
/// `last_seen_at` freshness rule (contract `device.onlineWindowSeconds`) —
/// the same one every synced client applies.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDevice {
    pub device_id: String,
    #[serde(default)]
    pub label: String,
    /// `desktop` or `server` (the CLI daemon).
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub unauthed_agents: Vec<String>,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub shared_team_ids: Vec<String>,
    /// Set only on a teammate's shared row (`team_id` given).
    #[serde(default)]
    pub owner: Option<DeviceOwner>,
}

/// `exponential_devices_list` — the caller's machines, plus the servers
/// teammates shared with `team_id` when given.
pub fn devices_list(
    trpc: &TrpcClient,
    team_id: Option<&str>,
) -> Result<Vec<RemoteDevice>, ApiError> {
    let mut arguments = serde_json::json!({ "limit": MAX_PAGE });
    if let Some(team_id) = team_id {
        arguments["teamId"] = Value::String(team_id.to_string());
    }
    call_typed(trpc, "exponential_devices_list", &arguments)
}

// ---------------------------------------------------------------------------
// Teams
// ---------------------------------------------------------------------------

/// One `exponential_teams_list` row (the caller's memberships).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTeam {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    /// `owner` | `member`.
    #[serde(default)]
    pub role: String,
}

/// `exponential_teams_list` — every team the caller belongs to, by name.
pub fn teams_list(trpc: &TrpcClient) -> Result<Vec<RemoteTeam>, ApiError> {
    call_typed(trpc, "exponential_teams_list", &serde_json::json!({ "limit": MAX_PAGE }))
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// One published screenshot (`exponential_sessions_get` only).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SessionResult {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
}

/// One coding-session row as the MCP session tools project it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSession {
    pub id: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub issue_identifier: Option<String>,
    #[serde(default)]
    pub issue_title: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub action_name: Option<String>,
    #[serde(default)]
    pub started_reason: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    /// `running` / `in_review` / `ended`.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub pr_url: Option<String>,
    #[serde(default)]
    pub pr_number: Option<i64>,
    #[serde(default)]
    pub pr_state: Option<String>,
    #[serde(default)]
    pub ended_by: Option<String>,
    #[serde(default)]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub needs_input: bool,
    /// EXP-804: the usage-wall refusal, verbatim (`window`, `resetsAt`, …).
    #[serde(default)]
    pub blocked: Option<Value>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    /// How deep the row sits in its run tree.
    #[serde(default)]
    pub depth: u32,
    /// `exponential_sessions_get` only.
    #[serde(default)]
    pub results: Vec<SessionResult>,
}

/// `exponential_sessions_list` filters.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only runs the caller started or hosts.
    pub mine: bool,
    pub limit: u32,
}

/// `exponential_sessions_list` — newest first.
pub fn sessions_list(
    trpc: &TrpcClient,
    query: &SessionsQuery,
) -> Result<Vec<RemoteSession>, ApiError> {
    let arguments = serde_json::to_value(query).map_err(|e| ApiError::Decode(e.to_string()))?;
    call_typed(trpc, "exponential_sessions_list", &arguments)
}

/// `exponential_sessions_get` — one row plus its published screenshots.
pub fn sessions_get(trpc: &TrpcClient, id: &str) -> Result<RemoteSession, ApiError> {
    call_typed(
        trpc,
        "exponential_sessions_get",
        &serde_json::json!({ "id": id }),
    )
}

/// `exponential_sessions_start` input: the `steer.startSession` payload the
/// tool forwards (identifiers accepted for `issue_id`), minus the workflow
/// membership keys only a run may name. Absent options = the target device's
/// own launch defaults.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStartInput {
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<std::collections::BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// What a remote start hands back once the device registered the run.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartedSession {
    pub session_id: String,
    #[serde(default)]
    pub session: Option<RemoteSession>,
}

/// `exponential_sessions_start` — `steer.startSession` plus the tool's wait
/// for the device's row (≤10s), so the caller gets the run's id. An offline
/// device, a busy issue or a device that took no run comes back as the
/// tool's own sentence.
pub fn sessions_start(
    trpc: &TrpcClient,
    input: &RemoteStartInput,
) -> Result<StartedSession, ApiError> {
    let arguments = serde_json::to_value(input).map_err(|e| ApiError::Decode(e.to_string()))?;
    call_typed(trpc, "exponential_sessions_start", &arguments)
}

/// `exponential_sessions_message` — deliver text into a live run the caller
/// owns or hosts, as user input (the server prefixes its source).
pub fn sessions_message(trpc: &TrpcClient, id: &str, message: &str) -> Result<(), ApiError> {
    call_tool(
        trpc,
        "exponential_sessions_message",
        &serde_json::json!({ "id": id, "message": message }),
    )?;
    Ok(())
}

/// `exponential_sessions_kill` output.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KilledSession {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub ended_at: Option<String>,
}

/// `exponential_sessions_kill` — `steer.killSession` (owner or host,
/// idempotent) plus the parent notification an agent-started child owes.
pub fn sessions_kill(trpc: &TrpcClient, id: &str) -> Result<KilledSession, ApiError> {
    call_typed(
        trpc,
        "exponential_sessions_kill",
        &serde_json::json!({ "id": id }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::{has_header, one_shot_server};
    use crate::StaticToken;
    use std::sync::Arc;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn the_call_body_is_a_bare_tools_call() {
        let body = tool_call_body("exponential_sessions_get", &serde_json::json!({"id": "s1"}));
        assert_eq!(
            body,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"exponential_sessions_get","arguments":{"id":"s1"}}}"#
        );
    }

    #[test]
    fn a_tool_error_surfaces_its_sentence() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"isError":true,"content":[{"type":"text","text":"Session is not live"}]}}"#;
        match parse_tool_response(body) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 400);
                assert_eq!(message, "Session is not live");
            }
            other => panic!("expected the tool error, got {other:?}"),
        }
        let rpc = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"Invalid params"}}"#;
        assert!(
            matches!(parse_tool_response(rpc), Err(ApiError::Http { message, .. }) if message == "Invalid params")
        );
    }

    #[test]
    fn a_non_json_text_comes_back_as_a_string() {
        let body = r#"{"result":{"content":[{"type":"text","text":"plain"}]}}"#;
        assert_eq!(
            parse_tool_response(body).unwrap(),
            Value::String("plain".into())
        );
    }

    #[test]
    fn devices_list_posts_to_the_mcp_route_and_decodes_rows() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"[{\"deviceId\":\"d1\",\"label\":\"build box\",\"kind\":\"server\",\"online\":true,\"agents\":[\"claude\"],\"caps\":[],\"isDefault\":true}]"}]}}"#,
        );
        let devices = devices_list(&client(&base), None).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "d1");
        assert_eq!(devices[0].label, "build box");
        assert!(devices[0].online && devices[0].is_default);
        assert_eq!(devices[0].agents, vec!["claude".to_string()]);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/mcp HTTP/1.1"), "{request}");
        assert!(has_header(&request, "Authorization: Bearer tok"));
        assert!(has_header(
            &request,
            "Accept: application/json, text/event-stream"
        ));
        assert!(
            request.ends_with(r#""arguments":{"limit":200}}}"#),
            "{request}"
        );
    }

    #[test]
    fn a_remote_start_omits_absent_options() {
        let input = RemoteStartInput {
            device_id: "d1".into(),
            action_id: Some("builtin:chat".into()),
            team_id: Some("t1".into()),
            prompt: Some("hi".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"deviceId":"d1","actionId":"builtin:chat","teamId":"t1","prompt":"hi"}"#
        );
    }

    #[test]
    fn sessions_rows_decode_with_nulls() {
        let row: RemoteSession = serde_json::from_str(
            r#"{"id":"s1","issueId":null,"issueIdentifier":null,"actionName":"Chat","status":"running","deviceId":"d1","deviceLabel":"mac","agent":"claude","needsInput":false,"blocked":null,"prNumber":null,"depth":1}"#,
        )
        .unwrap();
        assert_eq!(row.status, "running");
        assert_eq!(row.action_name.as_deref(), Some("Chat"));
        assert_eq!(row.depth, 1);
        assert!(row.blocked.is_none());
    }
}
