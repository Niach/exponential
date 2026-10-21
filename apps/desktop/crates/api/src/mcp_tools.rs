//! EXP-1110: typed calls into the instance's OWN MCP tools (`POST /api/mcp`)
//! for the reads and writes the CLI needs but no tRPC procedure serves.
//!
//! Devices and sessions reach every GUI client over the Electric shapes;
//! there is deliberately no tRPC `list` for either. The MCP tools
//! `exponential_devices_list` / `exponential_sessions_{list,get}` are the
//! server-side reads over the SAME predicates the shapes use (membership,
//! trash/archive, grant scope). EXP-1292: `exponential_sessions_send` is THE
//! entrypoint for a start and a message alike (the database is the bus: the
//! device picks the row up off the `sessions`/`messages` shapes), plus
//! `sessions_{stop,end}`. A one-shot CLI command therefore calls them instead
//! of syncing a shape.
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

/// One generous budget for every call (a slow link plus a server-side wait
/// must never trip the ordinary 30s budget).
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
/// NEW-4: the server's noun tools take an `op` beside the op's fields
/// (`exponential_sessions {op: "list", ...}`); the CLI's typed calls build
/// their fields and stamp the op here.
pub fn with_op(op: &str, mut arguments: Value) -> Value {
    if let Some(object) = arguments.as_object_mut() {
        object.insert("op".to_string(), Value::String(op.to_string()));
        arguments
    } else {
        serde_json::json!({ "op": op })
    }
}

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
    call_typed(trpc, "exponential_devices", &with_op("list", arguments))
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
    call_typed(trpc, "exponential_teams", &serde_json::json!({ "op": "list", "limit": MAX_PAGE }))
}

// ---------------------------------------------------------------------------
// Sessions (EXP-1292: the MCP v2 session tools — `sessions_send` is THE
// entrypoint for a start and a message alike; reads are the server's
// projection of the `sessions` tree)
// ---------------------------------------------------------------------------

/// One issue linked to a tree's root (`exponential_sessions_get` `issues`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionIssue {
    pub issue_id: String,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// One `sessions` row as the MCP v2 session tools project it: a
/// `exponential_sessions_list` row (roots only, `title` = the server's
/// derived title) or a `exponential_sessions_get` tree member (the public
/// row; the root adds `displayTitle`). Tolerant: every field but `id`
/// defaults, so either projection (and a server addition) decodes.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSession {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    /// `None` on a root (a thread or a team agent's channel).
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub root_id: Option<String>,
    /// How deep the row sits in its tree (0 = the root).
    #[serde(default)]
    pub depth: u32,
    /// Contract `sessionKind` (`thread` | `agent` on a root).
    #[serde(default)]
    pub kind: Option<String>,
    /// The row's title (a list row: the derived one).
    #[serde(default)]
    pub title: Option<String>,
    /// A tree root's derived title (`exponential_sessions_get`).
    #[serde(default)]
    pub display_title: Option<String>,
    /// A team agent's display name (`kind = agent` roots only).
    #[serde(default)]
    pub name: Option<String>,
    /// The action a thread ran, by name (a legacy read shim).
    #[serde(default)]
    pub action_name: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    /// The agent CLI the row runs on (`claude` | `codex`).
    #[serde(default)]
    pub cli: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// A dispatcher root (threads + team agents): runs on the thread model
    /// and hands work to child sessions. Absent on the wire = `false`.
    #[serde(default)]
    pub dispatch: bool,
    /// `queued` / `running` / `idle` / `ended` (contract `sessionStatus`).
    #[serde(default)]
    pub status: String,
    /// A turn is in flight.
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub needs_input: bool,
    /// EXP-804: the usage-wall refusal, verbatim (`window`, `resetsAt`, …).
    #[serde(default)]
    pub blocked: Option<Value>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub last_turn_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
}

/// `exponential_sessions_get`: ONE session tree (any member id resolves to
/// its root) — the root, its sub-sessions and the root's issues.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionTree {
    pub root: RemoteSession,
    #[serde(default, deserialize_with = "null_vec")]
    pub children: Vec<RemoteSession>,
    #[serde(default, deserialize_with = "null_vec")]
    pub issues: Vec<RemoteSessionIssue>,
}

impl RemoteSessionTree {
    /// The tree member `id` names (the root or one of its children).
    pub fn session(&self, id: &str) -> Option<&RemoteSession> {
        std::iter::once(&self.root)
            .chain(self.children.iter())
            .find(|row| row.id == id)
    }
}

/// `null` → `[]` (a `Vec` field rejects `null` even with `default`, which
/// only covers a MISSING key).
fn null_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(<Option<Vec<T>> as Deserialize>::deserialize(deserializer)?.unwrap_or_default())
}

/// `exponential_sessions_list` filters: the roots of one kind the caller may
/// see (shared ones and their own), newest activity first.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionsQuery {
    /// `thread` (the server's default) | `agent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    /// `true` = not ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    pub limit: u32,
}

/// `exponential_sessions_list` — root sessions, newest activity first.
pub fn sessions_list(
    trpc: &TrpcClient,
    query: &SessionsQuery,
) -> Result<Vec<RemoteSession>, ApiError> {
    let arguments = serde_json::to_value(query).map_err(|e| ApiError::Decode(e.to_string()))?;
    call_typed(trpc, "exponential_sessions", &with_op("list", arguments))
}

/// `exponential_sessions_get` — the tree `session_id` belongs to (no
/// messages: the CLI never prints them).
pub fn sessions_get(trpc: &TrpcClient, session_id: &str) -> Result<RemoteSessionTree, ApiError> {
    call_typed(
        trpc,
        "exponential_sessions",
        &serde_json::json!({ "op": "get", "id": session_id, "messages": 0 }),
    )
}

/// `exponential_sessions_send` input — the server's `sessionsSendInputSchema`
/// (web `@exp/db-schema/domain`): message an existing session (`session_id`;
/// an idle one resumes), start a CHILD (`parent_id`), or start a new ROOT
/// thread (neither; `issue_ids` / `prompt_id` / `repository_id`); NEW-2:
/// `agent_id` messages a team agent (the caller's conversation with it).
/// A new session names its `device_id`; absent runner pickers = that
/// device's own launch defaults. Every absent field stays off the wire.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionsSendInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    /// The team MCP servers the run gets (EXP-792); absent = the device's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_ids: Option<Vec<String>>,
    /// NEW-2: a new root run from a saved prompt (its body first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// EXP-1341: an explicit name for a new root or child.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// NEW-2: a message to a team agent (the caller's conversation with it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// What `exponential_sessions_send` hands back: the session (new or the one
/// messaged) and the `messages` row the text became (`None` for a start
/// without text).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SentSession {
    pub session_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
}

/// `exponential_sessions_send` — THE entrypoint (EXP-1292): a start is a
/// `queued` row naming its device (the device picks it up off the shape), a
/// message a `messages` row. A refusal (offline device, tree caps, an
/// ended agent) comes back as the tool's own sentence.
pub fn sessions_send(
    trpc: &TrpcClient,
    input: &SessionsSendInput,
) -> Result<SentSession, ApiError> {
    let arguments = serde_json::to_value(input).map_err(|e| ApiError::Decode(e.to_string()))?;
    call_typed(trpc, "exponential_sessions_send", &arguments)
}

/// `exponential_sessions_stop` — interrupt the session's current turn; it
/// stays live and goes idle (`sessions.stop`).
pub fn sessions_stop(trpc: &TrpcClient, session_id: &str) -> Result<(), ApiError> {
    call_tool(trpc, "exponential_sessions_send", &serde_json::json!({ "sessionId": session_id, "stop": true }))?;
    Ok(())
}

/// `exponential_sessions_end` — end a session explicitly (status `ended`;
/// ending a root ends its whole tree; agents never end).
pub fn sessions_end(trpc: &TrpcClient, session_id: &str) -> Result<(), ApiError> {
    call_tool(trpc, "exponential_sessions", &serde_json::json!({ "op": "end", "id": session_id }))?;
    Ok(())
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
        let body = tool_call_body("exponential_sessions", &serde_json::json!({"id": "s1", "op": "get"}));
        assert_eq!(
            body,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"exponential_sessions","arguments":{"id":"s1","op":"get"}}}"#
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
            request.ends_with(r#""arguments":{"limit":200,"op":"list"}}}"#),
            "{request}"
        );
    }

    #[test]
    fn a_send_omits_absent_options() {
        // A new root thread on an issue, run on a named device.
        let input = SessionsSendInput {
            device_id: Some("d1".into()),
            issue_ids: Some(vec!["i1".into()]),
            text: Some("hi".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"deviceId":"d1","text":"hi","issueIds":["i1"]}"#
        );
        let prompt = SessionsSendInput {
            team_id: Some("t1".into()),
            device_id: Some("d1".into()),
            plan_mode: Some(true),
            prompt_id: Some("p1".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&prompt).unwrap(),
            r#"{"teamId":"t1","deviceId":"d1","planMode":true,"promptId":"p1"}"#
        );
        let agent = SessionsSendInput {
            agent_id: Some("ag1".into()),
            text: Some("hi".into()),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&agent).unwrap(), r#"{"text":"hi","agentId":"ag1"}"#);
    }

    #[test]
    fn sessions_send_posts_the_tool_call_and_decodes_the_ids() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"sessionId\":\"s1\",\"messageId\":null}"}]}}"#,
        );
        let sent = sessions_send(
            &client(&base),
            &SessionsSendInput {
                session_id: Some("s0".into()),
                text: Some("go on".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(sent.session_id, "s1");
        assert_eq!(sent.message_id, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/mcp HTTP/1.1"), "{request}");
        assert!(
            request.ends_with(
                r#""params":{"name":"exponential_sessions_send","arguments":{"sessionId":"s0","text":"go on"}}}"#
            ),
            "{request}"
        );
    }

    #[test]
    fn sessions_stop_and_end_name_the_session() {
        // NEW-4: stop rides `sessions_send`; end is the `sessions` noun's op.
        for (call, expected) in [
            (
                sessions_stop as fn(&TrpcClient, &str) -> Result<(), ApiError>,
                r#""params":{"name":"exponential_sessions_send","arguments":{"sessionId":"s1","stop":true}}}"#,
            ),
            (sessions_end, r#""params":{"name":"exponential_sessions","arguments":{"id":"s1","op":"end"}}}"#),
        ] {
            let (base, captured) = one_shot_server(
                200,
                r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"ok\":true}"}]}}"#,
            );
            call(&client(&base), "s1").unwrap();
            let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(request.ends_with(expected), "{request}");
        }
    }

    #[test]
    fn sessions_rows_decode_with_nulls() {
        let row: RemoteSession = serde_json::from_str(
            r#"{"id":"s1","parentId":null,"rootId":"s1","kind":"thread","title":null,"status":"idle","deviceId":"d1","cli":"claude","busy":false,"needsInput":false,"blocked":null,"depth":0}"#,
        )
        .unwrap();
        assert_eq!(row.status, "idle");
        assert_eq!(row.kind.as_deref(), Some("thread"));
        assert_eq!(row.depth, 0);
        assert!(row.blocked.is_none());
    }

    #[test]
    fn sessions_get_reads_the_tree_by_id() {
        let tree = r#"{\"root\":{\"id\":\"r1\",\"status\":\"idle\",\"displayTitle\":\"Fix\"},\"issues\":[{\"issueId\":\"i1\",\"identifier\":\"EXP-1\",\"title\":\"Fix\",\"status\":\"backlog\"}],\"pullRequests\":[],\"guide\":[],\"children\":[{\"id\":\"c1\",\"parentId\":\"r1\",\"depth\":1,\"status\":\"running\"}],\"messages\":[]}"#;
        let (base, captured) = one_shot_server(
            200,
            Box::leak(
                format!(r#"{{"jsonrpc":"2.0","id":1,"result":{{"content":[{{"type":"text","text":"{tree}"}}]}}}}"#)
                    .into_boxed_str(),
            ),
        );
        let tree = sessions_get(&client(&base), "c1").unwrap();
        assert_eq!(tree.root.display_title.as_deref(), Some("Fix"));
        assert_eq!(tree.issues[0].identifier.as_deref(), Some("EXP-1"));
        assert_eq!(tree.session("c1").map(|row| row.depth), Some(1));
        assert!(tree.session("nope").is_none());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.ends_with(
                r#""params":{"name":"exponential_sessions","arguments":{"id":"c1","messages":0,"op":"get"}}}"#
            ),
            "{request}"
        );
    }

    #[test]
    fn the_list_query_speaks_the_v2_filters() {
        let query = SessionsQuery { kind: Some("agent".into()), live: Some(true), limit: 50, ..Default::default() };
        assert_eq!(
            serde_json::to_string(&query).unwrap(),
            r#"{"kind":"agent","live":true,"limit":50}"#
        );
    }
}
