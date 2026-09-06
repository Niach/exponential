//! EXP-746 — codex's private wire: a long-lived `codex app-server` JSON-RPC
//! connection. Owned by lane E3.
//!
//! `coding::codex_app_server` deliberately is NOT this: it is a one-shot
//! probe whose `route_line` returns `None` for anything carrying a `method`,
//! which would silently drop every server-initiated approval and hang the
//! session with no symptom. Hence the 3-way [`classify_line`]. What IS reused
//! verbatim: `rpc_request`, `rpc_notification` and the spawn recipe.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::transport::{spawn_lines, ChildLines, LineWriter, StderrPolicy};

/// A request the SERVER sent us (an approval, an elicitation). `id` stays a
/// `Value` because the app-server's id shape is not ours to normalize.
#[derive(Clone, Debug)]
pub struct ServerRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

/// The three things a line off the app-server can be, plus junk.
#[derive(Clone, Debug)]
pub enum Incoming {
    /// An answer to one of OUR requests. `Err` carries the JSON-RPC error.
    Response {
        id: u64,
        result: Result<Value, Value>,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A log line, a malformed frame — never mistaken for an answer.
    Junk,
}

/// Classify one line. The rule `route_line` gets wrong: a frame with BOTH an
/// `id` and a `method` is a server REQUEST, not our response.
pub fn classify_line(line: &str) -> Incoming {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Incoming::Junk;
    };
    let Some(object) = value.as_object() else {
        return Incoming::Junk;
    };
    let method = object.get("method").and_then(Value::as_str);
    let id = object.get("id");
    match (id, method) {
        // id + method: the server is ASKING us something (an approval, an
        // elicitation) and is waiting for a response with that id.
        (Some(id), Some(method)) => Incoming::ServerRequest {
            id: id.clone(),
            method: method.to_string(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        },
        // method only: a one-way notification.
        (None, Some(method)) => Incoming::Notification {
            method: method.to_string(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        },
        // id only: the answer to one of ours. A numeric id is ours by
        // construction; anything else cannot be matched to a waiter.
        (Some(id), None) => {
            let Some(id) = id.as_u64() else {
                return Incoming::Junk;
            };
            if let Some(error) = object.get("error") {
                Incoming::Response { id, result: Err(error.clone()) }
            } else {
                Incoming::Response {
                    id,
                    result: Ok(object.get("result").cloned().unwrap_or(Value::Null)),
                }
            }
        }
        (None, None) => Incoming::Junk,
    }
}

/// The live connection: id → waiter registry, the two inbound channels, and
/// the shared writer.
///
/// The waiters are `flume` channels rather than `tokio::sync::oneshot` so the
/// same registry serves both [`AppServer::request`] (async, driven on the
/// steer runtime) and [`AppServer::request_blocking`] (the spike harness and
/// the doctor probe, which have no runtime).
pub struct AppServer {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, flume::Sender<Result<Value, Value>>>>,
    writer: LineWriter,
    /// Owning the child here is what kills it: dropping the last `AppServer`
    /// reference drops the transport's guard. Handing it to the router thread
    /// instead would keep a wedged app-server alive for the life of that
    /// thread, which is exactly the escape EXP-300 is about.
    _child: ChildLines,
}

impl AppServer {
    /// Spawn `codex app-server --listen stdio://` and start routing its lines.
    /// Returns the connection plus the notification stream, the server-request
    /// stream and the child's exit.
    #[allow(clippy::type_complexity)]
    pub fn spawn(
        spec: &terminal::pty::SpawnSpec,
    ) -> std::io::Result<(
        Arc<AppServer>,
        flume::Receiver<(String, Value)>,
        flume::Receiver<ServerRequest>,
        flume::Receiver<terminal::pty::ChildExit>,
    )> {
        let child = spawn_lines(spec, StderrPolicy::Log)?;
        let lines = child.lines.clone();
        let exit = child.exit.clone();
        let pid = child.pid;
        let server = Arc::new(AppServer {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            writer: child.writer.clone(),
            _child: child,
        });

        let (notification_tx, notifications) = flume::unbounded();
        let (request_tx, requests) = flume::unbounded();
        // WEAK on purpose: a strong reference here would keep the child alive
        // for as long as the router thread runs, which is precisely as long as
        // the child lives — a cycle nothing could break.
        let router = Arc::downgrade(&server);
        std::thread::Builder::new()
            .name(format!("codex-router-{pid}"))
            .spawn(move || {
                for line in lines.iter() {
                    let Some(router) = router.upgrade() else { return };
                    match classify_line(&line) {
                        Incoming::Response { id, result } => router.resolve(id, result),
                        Incoming::ServerRequest { id, method, params } => {
                            let _ = request_tx.send(ServerRequest { id, method, params });
                        }
                        Incoming::Notification { method, params } => {
                            let _ = notification_tx.send((method, params));
                        }
                        // A codex log line on stdout is normal; dropping it is
                        // the point of the fourth arm.
                        Incoming::Junk => log::debug!("engine: codex junk line: {line}"),
                    }
                }
                // stdout closed: nobody will ever answer the outstanding
                // requests, so fail them instead of leaking their waiters.
                if let Some(router) = router.upgrade() {
                    router.fail_all(json!({ "code": -32000, "message": "app-server closed" }));
                }
            })?;

        Ok((server, notifications, requests, exit))
    }

    fn resolve(&self, id: u64, result: Result<Value, Value>) {
        let waiter = self.pending.lock().ok().and_then(|mut pending| pending.remove(&id));
        match waiter {
            Some(waiter) => {
                let _ = waiter.send(result);
            }
            None => log::debug!("engine: codex response for an unknown id {id}"),
        }
    }

    fn fail_all(&self, error: Value) {
        let Ok(mut pending) = self.pending.lock() else { return };
        for (_, waiter) in pending.drain() {
            let _ = waiter.send(Err(error.clone()));
        }
    }

    /// Register a waiter and write the request line.
    fn issue(&self, method: &str, params: Value) -> Result<flume::Receiver<Result<Value, Value>>, Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = flume::bounded(1);
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| json!({ "message": "codex pending registry poisoned" }))?;
            pending.insert(id, tx);
        }
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(err) = self.writer.write_line(&line.to_string()) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            return Err(json!({ "message": format!("codex stdin write failed: {err}") }));
        }
        Ok(rx)
    }

    /// One request, awaited. `Err` is the app-server's own JSON-RPC error.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, Value> {
        let rx = self.issue(method, params)?;
        rx.recv_async()
            .await
            .map_err(|_| json!({ "message": "codex app-server dropped the request" }))?
    }

    /// The same request from a thread with no runtime (the spike harness and
    /// the doctor probe).
    pub fn request_blocking(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, Value> {
        let rx = self.issue(method, params)?;
        rx.recv_timeout(timeout)
            .map_err(|_| json!({ "message": format!("codex {method} timed out") }))?
    }

    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        let line = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.writer.write_line(&line.to_string())
    }

    /// Answer a server-initiated request (an approval decision).
    pub fn respond(&self, id: Value, result: Value) -> std::io::Result<()> {
        let line = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        self.writer.write_line(&line.to_string())
    }
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

/// `initialize` params. `optOutNotificationMethods` is the lever that keeps
/// the ~35 notifications the engine would parse-and-drop off the wire
/// entirely, `process/outputDelta` and `command/exec/outputDelta` included.
pub fn initialize_params(version: &str, opt_out: &[&str]) -> Value {
    json!({
        "clientInfo": { "name": "exponential", "version": version },
        "capabilities": {
            "experimentalApi": true,
            "requestAttestation": false,
            "optOutNotificationMethods": opt_out,
        },
    })
}

/// The notifications the engine never reads. Suppressing them at the source
/// is strictly cheaper than decoding and discarding a token-rate delta stream.
pub const OPT_OUT_NOTIFICATIONS: &[&str] = &[
    "process/outputDelta",
    "command/exec/outputDelta",
    "item/reasoning/summaryPartAdded",
];

/// `thread/start`. The MCP block rides `config`, NOT a param of its own —
/// this is the exact replacement for the PTY path's `-c mcp_servers.*` argv
/// overrides. `projects.<root>.trust_level` is what stops the app-server
/// asking about an untrusted directory.
pub fn thread_start_params(cwd: &std::path::Path, config: Value) -> Value {
    json!({ "cwd": cwd.display().to_string(), "config": config })
}

/// The `config` blob for [`thread_start_params`]: the exponential MCP server,
/// project trust for every directory the turn may touch, and the writable
/// roots of the workspace sandbox.
pub fn thread_config(
    mcp_url: Option<&str>,
    session_id: &str,
    trusted_roots: &[std::path::PathBuf],
) -> Value {
    let mut config = serde_json::Map::new();
    if let Some(url) = mcp_url {
        config.insert(
            "mcp_servers".to_string(),
            json!({
                "exponential": {
                    "url": url,
                    // The key itself never lands in the config: the app-server
                    // reads it out of the child's own environment.
                    "bearer_token_env_var": "EXP_MCP_TOKEN",
                    "http_headers": { "X-Exp-Session-Id": session_id },
                },
            }),
        );
        config.insert("experimental_use_rmcp_client".to_string(), json!(true));
    }
    let mut projects = serde_json::Map::new();
    for root in trusted_roots {
        projects.insert(
            root.display().to_string(),
            json!({ "trust_level": "trusted" }),
        );
    }
    if !projects.is_empty() {
        config.insert("projects".to_string(), Value::Object(projects));
        config.insert(
            "sandbox_workspace_write".to_string(),
            json!({
                "writable_roots": trusted_roots
                    .iter()
                    .map(|root| root.display().to_string())
                    .collect::<Vec<_>>(),
            }),
        );
    }
    Value::Object(config)
}

/// The three approval × sandbox presets, copied from codex-acp's `AgentMode`.
/// Applied PER TURN, never at `thread/start`. The default is
/// `agent-full-access` — today's `--dangerously-bypass-approvals-and-sandbox`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexMode {
    ReadOnly,
    Agent,
    AgentFullAccess,
}

impl CodexMode {
    pub fn id(self) -> &'static str {
        match self {
            CodexMode::ReadOnly => "read-only",
            CodexMode::Agent => "agent",
            CodexMode::AgentFullAccess => "agent-full-access",
        }
    }

    pub fn approval_policy(self) -> &'static str {
        match self {
            CodexMode::ReadOnly | CodexMode::Agent => "on-request",
            CodexMode::AgentFullAccess => "never",
        }
    }

    pub fn approvals_reviewer(self) -> &'static str {
        match self {
            CodexMode::ReadOnly | CodexMode::AgentFullAccess => "user",
            CodexMode::Agent => "auto_review",
        }
    }

    pub fn sandbox_policy(self, writable_roots: &[std::path::PathBuf]) -> Value {
        match self {
            CodexMode::AgentFullAccess => json!({ "type": "dangerFullAccess" }),
            CodexMode::ReadOnly | CodexMode::Agent => json!({
                "type": "workspaceWrite",
                "writableRoots": writable_roots
                    .iter()
                    .map(|root| root.display().to_string())
                    .collect::<Vec<_>>(),
            }),
        }
    }
}

/// `turn/start`. Model/effort/mode all ride the TURN, not the thread.
pub fn turn_start_params(
    thread_id: &str,
    text: &str,
    model: Option<&str>,
    effort: Option<&str>,
    mode: CodexMode,
    writable_roots: &[std::path::PathBuf],
) -> Value {
    json!({
        "threadId": thread_id,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
        "model": model,
        "effort": effort,
        "summary": "auto",
        "approvalPolicy": mode.approval_policy(),
        "approvalsReviewer": mode.approvals_reviewer(),
        "sandboxPolicy": mode.sandbox_policy(writable_roots),
    })
}

/// `turn/steer` — `expectedTurnId` is a PRECONDITION: the call fails when
/// that turn is no longer the active one.
pub fn turn_steer_params(thread_id: &str, expected_turn_id: &str, text: &str) -> Value {
    json!({
        "threadId": thread_id,
        "expectedTurnId": expected_turn_id,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
    })
}

/// `turn/interrupt`. The caller marks the turn STALE first, then interrupts:
/// without the fence, late notifications keep polluting the feed and in-flight
/// approvals hang with nobody to answer them.
pub fn turn_interrupt_params(thread_id: &str, turn_id: &str) -> Value {
    json!({ "threadId": thread_id, "turnId": turn_id })
}

/// `model/list`, paginated until `nextCursor` comes back null.
pub fn model_list_params(cursor: Option<&str>) -> Value {
    match cursor {
        Some(cursor) => json!({ "cursor": cursor }),
        None => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_line_routes_a_response() {
        let line = r#"{"jsonrpc":"2.0","id":7,"result":{"threadId":"t1"}}"#;
        match classify_line(line) {
            Incoming::Response { id, result } => {
                assert_eq!(id, 7);
                assert_eq!(result.expect("ok result")["threadId"], json!("t1"));
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_routes_an_error_response() {
        let line = r#"{"jsonrpc":"2.0","id":8,"error":{"code":-32601,"message":"nope"}}"#;
        match classify_line(line) {
            Incoming::Response { id, result } => {
                assert_eq!(id, 8);
                assert_eq!(result.expect_err("error result")["code"], json!(-32601));
            }
            other => panic!("expected an error response, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_routes_a_server_request() {
        // The regression this whole type exists for: `route_line` returns None
        // here, so every approval would be dropped and codex would hang with
        // no symptom at all.
        let line = r#"{"jsonrpc":"2.0","id":"srv-1","method":"item/commandExecution/requestApproval","params":{"command":"rm -rf x"}}"#;
        match classify_line(line) {
            Incoming::ServerRequest { id, method, params } => {
                assert_eq!(id, json!("srv-1"));
                assert_eq!(method, "item/commandExecution/requestApproval");
                assert_eq!(params["command"], json!("rm -rf x"));
            }
            other => panic!("expected a server request, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_routes_a_notification() {
        let line = r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turnId":"turn_1"}}"#;
        match classify_line(line) {
            Incoming::Notification { method, params } => {
                assert_eq!(method, "turn/started");
                assert_eq!(params["turnId"], json!("turn_1"));
            }
            other => panic!("expected a notification, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_treats_anything_else_as_junk() {
        for line in [
            "not json at all",
            r#"{"jsonrpc":"2.0"}"#,
            r#"[1,2,3]"#,
            // A non-numeric id with no method cannot belong to any waiter.
            r#"{"id":"who","result":{}}"#,
        ] {
            assert!(matches!(classify_line(line), Incoming::Junk), "line: {line}");
        }
    }

    #[test]
    fn the_thread_config_carries_mcp_and_project_trust() {
        let roots = vec![std::path::PathBuf::from("/work/tree")];
        let config = thread_config(Some("https://x/api/mcp"), "sess-1", &roots);
        assert_eq!(
            config["mcp_servers"]["exponential"],
            json!({
                "url": "https://x/api/mcp",
                "bearer_token_env_var": "EXP_MCP_TOKEN",
                "http_headers": { "X-Exp-Session-Id": "sess-1" },
            })
        );
        assert_eq!(config["experimental_use_rmcp_client"], json!(true));
        assert_eq!(config["projects"]["/work/tree"]["trust_level"], json!("trusted"));
        assert_eq!(
            config["sandbox_workspace_write"]["writable_roots"],
            json!(["/work/tree"])
        );
    }

    #[test]
    fn full_access_turns_ask_for_no_approvals() {
        let params = turn_start_params("t1", "hi", Some("gpt-5"), Some("high"), CodexMode::AgentFullAccess, &[]);
        assert_eq!(params["approvalPolicy"], json!("never"));
        assert_eq!(params["sandboxPolicy"], json!({ "type": "dangerFullAccess" }));
        assert_eq!(params["input"], json!([{ "type": "text", "text": "hi", "text_elements": [] }]));
    }
}
