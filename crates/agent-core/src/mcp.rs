//! MCP client for `/api/mcp` — a port of `apps/companion/src/exponential-mcp-client.ts`.
//!
//! The server runs the MCP Streamable-HTTP transport STATELESS
//! (`sessionIdGenerator: undefined`, `enableJsonResponse: true`), so — verified
//! against the live endpoint — a bare `tools/call` JSON-RPC POST works with NO
//! `initialize` handshake and returns a single JSON response (no SSE, no
//! session). Auth: `Authorization: Bearer <expk_ key>`. Each tool's payload is a
//! JSON string inside `result.content[].text` (parsed by `parse_tool_payload`).

use crate::pipeline::{Comment, IssueDetail};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static RPC_ID: AtomicU64 = AtomicU64::new(1);

fn mcp_url(base_url: &str) -> String {
    format!("{}/api/mcp", base_url.trim_end_matches('/'))
}

/// Decode a JSON-RPC envelope → the `result` object, or the `error.message`.
pub fn parse_rpc(payload: &Value) -> Result<Value, String> {
    if let Some(msg) = payload.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Err(msg.to_string());
    }
    payload.get("result").cloned().ok_or_else(|| "invalid JSON-RPC response".to_string())
}

/// Extract a tool's structured payload: `null` on `isError`, else parse the JSON
/// string in the first `text` content block.
pub fn parse_tool_payload(result: &Value) -> Option<Value> {
    if result.get("isError").and_then(|b| b.as_bool()).unwrap_or(false) {
        return None;
    }
    let text = result
        .get("content")?
        .as_array()?
        .iter()
        .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))?
        .get("text")?
        .as_str()?;
    serde_json::from_str(text).ok()
}

/// Call a tool. Returns the parsed payload (None if the tool returned isError or
/// had no JSON text — matching the companion's `parseToolPayload`).
pub fn call_tool(
    base_url: &str,
    api_key: &str,
    name: &str,
    arguments: Value,
    timeout_s: u64,
) -> Result<Option<Value>, String> {
    let id = RPC_ID.fetch_add(1, Ordering::Relaxed);
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    });
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_s)).build();
    let req = agent
        .post(&mcp_url(base_url))
        .set("content-type", "application/json")
        .set("accept", "application/json, text/event-stream")
        .set("authorization", &format!("Bearer {api_key}"));
    let resp = match req.send_json(&body) {
        Ok(r) => r,
        Err(ureq::Error::Status(_code, r)) => r,
        Err(e) => return Err(format!("mcp request failed: {e}")),
    };
    let payload: Value = resp.into_json().map_err(|e| format!("mcp parse: {e}"))?;
    let result = parse_rpc(&payload)?;
    Ok(parse_tool_payload(&result))
}

// --- typed tool wrappers (mirror ExponentialMcpClient) ---

pub fn create_comment(base_url: &str, api_key: &str, issue_id: &str, body_text: &str, kind: Option<&str>, timeout_s: u64) -> Result<(), String> {
    let mut args = json!({ "issueId": issue_id, "bodyText": body_text });
    if let Some(k) = kind {
        args["kind"] = json!(k);
    }
    call_tool(base_url, api_key, "exponential_comments_create", args, timeout_s).map(|_| ())
}

pub fn update_issue_status(base_url: &str, api_key: &str, issue_id: &str, status: &str, timeout_s: u64) -> Result<(), String> {
    call_tool(base_url, api_key, "exponential_issues_update", json!({ "id": issue_id, "status": status }), timeout_s).map(|_| ())
}

pub fn submit_agent_plan(base_url: &str, api_key: &str, issue_id: &str, plan: &str, state: &str, timeout_s: u64) -> Result<(), String> {
    call_tool(base_url, api_key, "exponential_agent_plan_submit", json!({ "issueId": issue_id, "plan": plan, "state": state }), timeout_s).map(|_| ())
}

pub fn mark_agent_plan_started(base_url: &str, api_key: &str, issue_id: &str, timeout_s: u64) -> Result<(), String> {
    call_tool(base_url, api_key, "exponential_agent_plan_mark_started", json!({ "issueId": issue_id }), timeout_s).map(|_| ())
}

pub fn reset_agent_plan(base_url: &str, api_key: &str, issue_id: &str, timeout_s: u64) -> Result<(), String> {
    call_tool(base_url, api_key, "exponential_agent_plan_reset", json!({ "issueId": issue_id }), timeout_s).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub github_repo: Option<String>,
}

pub fn get_project(base_url: &str, api_key: &str, project_id: &str, timeout_s: u64) -> Result<Option<Project>, String> {
    let payload = call_tool(base_url, api_key, "exponential_projects_get", json!({ "id": project_id }), timeout_s)?;
    Ok(payload.as_ref().map(to_project))
}

pub fn get_issue(base_url: &str, api_key: &str, issue_id: &str, timeout_s: u64) -> Result<Option<IssueDetail>, String> {
    let payload = call_tool(base_url, api_key, "exponential_issues_get", json!({ "id": issue_id }), timeout_s)?;
    Ok(payload.as_ref().map(to_issue_detail))
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn opt_s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}
/// Pull markdown out of a jsonb `{ "text": "…" }` value.
fn text_of(v: Option<&Value>) -> String {
    v.and_then(|b| b.get("text")).and_then(|t| t.as_str()).unwrap_or("").to_string()
}

pub fn to_project(payload: &Value) -> Project {
    Project {
        id: s(payload, "id"),
        workspace_id: s(payload, "workspaceId"),
        name: s(payload, "name"),
        github_repo: opt_s(payload, "githubRepo"),
    }
}

/// Map a `exponential_issues_get` payload into the pipeline's `IssueDetail`.
pub fn to_issue_detail(payload: &Value) -> IssueDetail {
    let comments = payload
        .get("recentComments")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| Comment {
                    kind: s(c, "kind"),
                    body_text: text_of(c.get("body")),
                    author_id: s(c, "authorId"),
                    created_at: s(c, "createdAt"),
                })
                .collect()
        })
        .unwrap_or_default();
    IssueDetail {
        identifier: s(payload, "identifier"),
        title: s(payload, "title"),
        description_text: text_of(payload.get("description")),
        agent_plan_state: opt_s(payload, "agentPlanState"),
        agent_plan_revision: payload.get("agentPlanRevision").and_then(|n| n.as_i64()).unwrap_or(0),
        agent_plan_approved_at: opt_s(payload, "agentPlanApprovedAt"),
        agent_last_comment_seen_at: opt_s(payload, "agentLastCommentSeenAt"),
        recent_comments: comments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rpc_result_and_error() {
        assert!(parse_rpc(&json!({"result": {"x": 1}, "jsonrpc": "2.0", "id": 1})).is_ok());
        assert_eq!(
            parse_rpc(&json!({"error": {"code": -32000, "message": "nope"}, "jsonrpc": "2.0", "id": 1})).unwrap_err(),
            "nope"
        );
    }

    #[test]
    fn tool_payload_parses_text_json() {
        let result = json!({ "content": [{ "type": "text", "text": "{\"id\":\"p1\",\"name\":\"Proj\"}" }] });
        let p = parse_tool_payload(&result).unwrap();
        assert_eq!(p.get("id").unwrap(), &json!("p1"));
    }

    #[test]
    fn tool_payload_is_none_on_error_or_no_text() {
        assert!(parse_tool_payload(&json!({ "isError": true, "content": [{ "type": "text", "text": "{}" }] })).is_none());
        assert!(parse_tool_payload(&json!({ "content": [{ "type": "image" }] })).is_none());
    }

    #[test]
    fn maps_project_payload() {
        let p = to_project(&json!({ "id": "p1", "workspaceId": "w1", "name": "Proj", "githubRepo": "o/r" }));
        assert_eq!(p.github_repo.as_deref(), Some("o/r"));
        assert_eq!(p.workspace_id, "w1");
        let none = to_project(&json!({ "id": "p1", "workspaceId": "w1", "name": "Proj", "githubRepo": null }));
        assert!(none.github_repo.is_none());
    }

    #[test]
    fn maps_issue_detail_payload() {
        let payload = json!({
            "id": "i1", "projectId": "p1", "identifier": "EXP-1", "title": "Fix it",
            "description": { "text": "do the thing" },
            "agentPlanState": "awaiting_approval", "agentPlanRevision": 2,
            "agentPlanApprovedAt": null, "agentLastCommentSeenAt": "2026-06-01T00:00:00Z",
            "recentComments": [
                { "kind": "plan", "body": { "text": "the plan" }, "authorId": "agent", "createdAt": "2026-06-01T00:00:00Z" }
            ]
        });
        let d = to_issue_detail(&payload);
        assert_eq!(d.identifier, "EXP-1");
        assert_eq!(d.description_text, "do the thing");
        assert_eq!(d.agent_plan_state.as_deref(), Some("awaiting_approval"));
        assert_eq!(d.agent_plan_revision, 2);
        assert_eq!(d.recent_comments.len(), 1);
        assert_eq!(d.recent_comments[0].body_text, "the plan");
    }
}
