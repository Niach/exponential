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

/// Where a request line goes. Abstracted over [`LineWriter`] for ONE reason:
/// the adapter tests drive a fake app-server in-process, and a `LineWriter`
/// can only ever wrap a real child's stdin. Everything in production is the
/// child (EXP-746 E3).
pub trait LineSink: Send + Sync + 'static {
    fn write_line(&self, line: &str) -> std::io::Result<()>;
}

impl LineSink for LineWriter {
    fn write_line(&self, line: &str) -> std::io::Result<()> {
        LineWriter::write_line(self, line)
    }
}

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
    writer: Arc<dyn LineSink>,
    /// Owning the child here is what kills it: dropping the last `AppServer`
    /// reference drops the transport's guard. Handing it to the router thread
    /// instead would keep a wedged app-server alive for the life of that
    /// thread, which is exactly the escape EXP-300 is about. `None` only for
    /// the in-process fake the adapter tests drive.
    _child: Option<ChildLines>,
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
        let writer: Arc<dyn LineSink> = Arc::new(child.writer.clone());
        let (server, notifications, requests) =
            AppServer::route(lines, writer, Some(child), &format!("codex-router-{pid}"))?;
        Ok((server, notifications, requests, exit))
    }

    /// Route an already-open line stream. The production caller is [`spawn`];
    /// the adapter tests hand in an in-process fake app-server instead of a
    /// child process (EXP-746 E3 — the alternative is a shell script writing
    /// JSON, which cannot answer a request by id).
    ///
    /// [`spawn`]: AppServer::spawn
    #[allow(clippy::type_complexity)]
    pub fn attach(
        lines: flume::Receiver<String>,
        writer: Arc<dyn LineSink>,
    ) -> std::io::Result<(
        Arc<AppServer>,
        flume::Receiver<(String, Value)>,
        flume::Receiver<ServerRequest>,
    )> {
        AppServer::route(lines, writer, None, "codex-router-attached")
    }

    #[allow(clippy::type_complexity)]
    fn route(
        lines: flume::Receiver<String>,
        writer: Arc<dyn LineSink>,
        child: Option<ChildLines>,
        thread_name: &str,
    ) -> std::io::Result<(
        Arc<AppServer>,
        flume::Receiver<(String, Value)>,
        flume::Receiver<ServerRequest>,
    )> {
        let server = Arc::new(AppServer {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            writer,
            _child: child,
        });

        let (notification_tx, notifications) = flume::unbounded();
        let (request_tx, requests) = flume::unbounded();
        // WEAK on purpose: a strong reference here would keep the child alive
        // for as long as the router thread runs, which is precisely as long as
        // the child lives — a cycle nothing could break.
        let router = Arc::downgrade(&server);
        std::thread::Builder::new()
            .name(thread_name.to_string())
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

        Ok((server, notifications, requests))
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
/// is strictly cheaper than decoding and discarding a token-rate delta stream:
/// one measured codex turn produced 911 notifications in 20 seconds.
///
/// Only methods with NO reader are listed. `item/commandExecution/outputDelta`
/// is deliberately absent — it is the command-output card's stream — and so
/// are the two token deltas. The app-server ignores names it does not know, so
/// an entry for a method a newer codex renamed costs nothing.
pub const OPT_OUT_NOTIFICATIONS: &[&str] = &[
    // High-volume streams with no reader on our side.
    "process/outputDelta",
    "process/exited",
    "command/exec/outputDelta",
    "item/reasoning/summaryPartAdded",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    // `turn/diff/updated` is a whole-turn unified diff, but the wire `diff`
    // kind stays `DiffSnapshots`' debounced worktree patch on every agent
    // (§4.1), so this one would only be parsed and dropped.
    "turn/diff/updated",
    "hook/started",
    "hook/completed",
    "item/autoApprovalReview/started",
    "item/autoApprovalReview/completed",
    "fuzzyFileSearch/sessionUpdated",
    "fuzzyFileSearch/sessionCompleted",
    "mcpServer/startupStatus/updated",
    "mcpServer/oauthLogin/completed",
    "remoteControl/status/changed",
    "app/list/updated",
    "skills/changed",
    "fs/changed",
    "serverRequest/resolved",
    "thread/realtime/started",
    "thread/realtime/itemAdded",
    "thread/realtime/transcript/delta",
    "thread/realtime/transcript/done",
    "thread/realtime/outputAudio/delta",
    "thread/realtime/sdp",
    "thread/realtime/error",
    "thread/realtime/closed",
    "windows/worldWritableWarning",
    "windowsSandbox/setupCompleted",
    "externalAgentConfig/import/progress",
    "externalAgentConfig/import/completed",
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodexMode {
    ReadOnly,
    Agent,
    /// D3: the ACP default, because the PTY path always passed
    /// `--dangerously-bypass-approvals-and-sandbox`.
    #[default]
    AgentFullAccess,
}

/// The three presets in the order a picker shows them: least access first.
pub const CODEX_MODES: [CodexMode; 3] =
    [CodexMode::ReadOnly, CodexMode::Agent, CodexMode::AgentFullAccess];

impl CodexMode {
    /// The id an unknown value falls back FROM: an id we do not know can only
    /// be answered with the default, never with a quieter mode than the user
    /// asked for.
    pub fn parse(id: &str) -> Option<CodexMode> {
        CODEX_MODES.into_iter().find(|mode| mode.id() == id)
    }

    /// codex-acp's own wording. The ids are misleading on their own
    /// (`read-only` is workspace-write with approvals), so the NAME is what a
    /// person sees.
    pub fn label(self) -> &'static str {
        match self {
            CodexMode::ReadOnly => "Ask for approval",
            CodexMode::Agent => "Approve for me",
            CodexMode::AgentFullAccess => "Full access",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            CodexMode::ReadOnly => "Always ask to edit external files and use the internet",
            CodexMode::Agent => "Only ask for actions detected as potentially unsafe",
            CodexMode::AgentFullAccess => {
                "Unrestricted access to the internet and any file on your computer"
            }
        }
    }

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
    let input = [json!({ "type": "text", "text": text, "text_elements": [] })];
    turn_start_request(&TurnRequest {
        thread_id,
        input: &input,
        model,
        effort,
        mode,
        service_tier: None,
        writable_roots,
    })
}

/// `turn/steer` — `expectedTurnId` is a PRECONDITION: the call fails when
/// that turn is no longer the active one, which is what turns a lost race
/// into a fresh turn instead of a silently dropped message.
pub fn turn_steer_params(thread_id: &str, expected_turn_id: &str, input: &[Value]) -> Value {
    json!({
        "threadId": thread_id,
        "expectedTurnId": expected_turn_id,
        "input": input,
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

/// `thread/resume` — re-enters a recorded thread id. Same override set as
/// `thread/start`, so the config blob rides along unchanged.
pub fn thread_resume_params(thread_id: &str, cwd: &std::path::Path, config: Value) -> Value {
    json!({ "threadId": thread_id, "cwd": cwd.display().to_string(), "config": config })
}

/// `thread/read` — the whole thread including its turns, which is how an
/// ended run's transcript is replayed (Past, D6).
pub fn thread_read_params(thread_id: &str, include_turns: bool) -> Value {
    json!({ "threadId": thread_id, "includeTurns": include_turns })
}

/// `thread/list` — newest first. `cwd` filters SERVER-side, which is cheaper
/// than codex-acp's client-side pass over every thread on the machine.
pub fn thread_list_params(cwd: Option<&std::path::Path>, cursor: Option<&str>) -> Value {
    let mut params = serde_json::Map::new();
    if let Some(cwd) = cwd {
        params.insert("cwd".to_string(), json!({ "path": cwd.display().to_string() }));
    }
    if let Some(cursor) = cursor {
        params.insert("cursor".to_string(), json!(cursor));
    }
    params.insert("sortKey".to_string(), json!("updated_at"));
    Value::Object(params)
}

/// `thread/compact/start`. There is NO `turn/compact`, and the response says
/// nothing: completion arrives as `thread/compacted` or as an `item/completed`
/// carrying a `contextCompaction` item.
pub fn thread_compact_start_params(thread_id: &str) -> Value {
    json!({ "threadId": thread_id })
}

/// `thread/settings/update` — how codex-acp pushes a collaboration mode
/// (`default` | `plan`). The method is NOT in the installed 0.144.5 schema, so
/// the caller must treat a method-not-found error as "this codex has no plan
/// mode" rather than as a failure.
pub fn thread_settings_collaboration_params(
    thread_id: &str,
    mode: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Value {
    json!({
        "threadId": thread_id,
        "collaborationMode": {
            "mode": mode,
            "settings": {
                "model": model,
                "reasoning_effort": effort,
                "developer_instructions": Value::Null,
            },
        },
    })
}

/// Everything one `turn/start` carries. Model, effort, mode and service tier
/// all ride the TURN, never the thread: codex-acp passes neither
/// `approvalPolicy` nor `sandbox` at `thread/start`, and every turn override
/// is documented as sticky for the turns after it.
#[derive(Clone, Copy, Debug)]
pub struct TurnRequest<'a> {
    pub thread_id: &'a str,
    /// codex `UserInput` items, already built from the ACP content blocks.
    pub input: &'a [Value],
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub mode: CodexMode,
    /// `"fast"` when the model advertises the tier and the chip is on.
    pub service_tier: Option<&'a str>,
    pub writable_roots: &'a [std::path::PathBuf],
}

/// `turn/start` from a [`TurnRequest`].
pub fn turn_start_request(request: &TurnRequest) -> Value {
    let mut params = json!({
        "threadId": request.thread_id,
        "input": request.input,
        "model": request.model,
        "effort": request.effort,
        "summary": "auto",
        "approvalPolicy": request.mode.approval_policy(),
        "approvalsReviewer": request.mode.approvals_reviewer(),
        "sandboxPolicy": request.mode.sandbox_policy(request.writable_roots),
    });
    if let (Some(tier), Some(object)) = (request.service_tier, params.as_object_mut()) {
        object.insert("serviceTier".to_string(), json!(tier));
    }
    params
}

/// codex-acp's `ModelId`: model and reasoning effort encoded as the ONE
/// string `"<model>[<effort>]"`. It is the value id of the model config
/// option, so a chip pick carries both halves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelId {
    pub model: String,
    pub effort: Option<String>,
}

impl ModelId {
    pub fn new(model: impl Into<String>, effort: Option<String>) -> ModelId {
        ModelId { model: model.into(), effort }
    }

    /// `fromString` requires the bracket form; a bare model name keeps its
    /// effort unset rather than being rejected (an older codex, or a model
    /// with no reasoning efforts at all).
    pub fn parse(value: &str) -> ModelId {
        match (value.find('['), value.strip_suffix(']')) {
            (Some(open), Some(trimmed)) if open + 1 <= trimmed.len() => ModelId {
                model: value[..open].to_string(),
                effort: Some(trimmed[open + 1..].to_string()),
            },
            _ => ModelId { model: value.to_string(), effort: None },
        }
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.effort {
            Some(effort) => write!(f, "{}[{}]", self.model, effort),
            None => write!(f, "{}", self.model),
        }
    }
}

/// The headline a `commandExecution` item renders as: the command without the
/// shell wrapper codex puts around almost everything.
pub fn strip_shell_prefix(command: &str) -> &str {
    let command = command.trim();
    for prefix in ["bash -lc ", "bash -c ", "sh -lc ", "sh -c ", "zsh -lc ", "zsh -c "] {
        if let Some(rest) = command.strip_prefix(prefix) {
            let rest = rest.trim();
            // The wrapped command is usually one quoted argument.
            for quote in ['\'', '"'] {
                if let Some(inner) = rest.strip_prefix(quote).and_then(|r| r.strip_suffix(quote)) {
                    return inner;
                }
            }
            return rest;
        }
    }
    command
}

/// How an approval option reads to a person picking it. Maps onto ACP's
/// `PermissionOptionKind` in the adapter; kept here so the option TABLES stay
/// beside the decisions they produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
}

/// One answer we can give codex, as the client will see it.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalChoice {
    /// The ACP `optionId` — never a keystroke (D3).
    pub option_id: String,
    pub label: String,
    pub kind: ApprovalKind,
    /// The COMPLETE JSON-RPC result for the approval request.
    pub result: Value,
}

impl ApprovalChoice {
    fn new(option_id: &str, label: &str, kind: ApprovalKind, result: Value) -> ApprovalChoice {
        ApprovalChoice {
            option_id: option_id.to_string(),
            label: label.to_string(),
            kind,
            result,
        }
    }
}

fn decision(value: &str) -> Value {
    json!({ "decision": value })
}

/// `item/commandExecution/requestApproval`. `availableDecisions` is
/// AUTHORITATIVE when the server sends it (0.153.3+); the installed 0.144.5
/// omits it, so the default set stands in. Unknown decision strings are
/// dropped rather than guessed at.
pub fn command_approval_choices(params: &Value) -> Vec<ApprovalChoice> {
    let available = params
        .get("availableDecisions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        });
    let network = params.get("networkApprovalContext").is_some();
    let mut choices = Vec::new();
    let wanted = available.unwrap_or_else(|| {
        ["accept", "acceptForSession", "decline", "cancel"]
            .iter()
            .map(|value| value.to_string())
            .collect()
    });
    for value in &wanted {
        let choice = match value.as_str() {
            "accept" if network => ApprovalChoice::new(
                "allow_once",
                "Yes, just this once",
                ApprovalKind::AllowOnce,
                decision("accept"),
            ),
            "accept" => ApprovalChoice::new(
                "allow_once",
                "Yes, proceed",
                ApprovalKind::AllowOnce,
                decision("accept"),
            ),
            "acceptForSession" => ApprovalChoice::new(
                "allow_for_session",
                "Yes, and don't ask again for this command in this session",
                ApprovalKind::AllowAlways,
                decision("acceptForSession"),
            ),
            "decline" => ApprovalChoice::new(
                "decline",
                "No, continue without running it",
                ApprovalKind::RejectOnce,
                decision("decline"),
            ),
            "cancel" => ApprovalChoice::new(
                "cancel",
                "No, and tell Codex what to do differently",
                ApprovalKind::RejectOnce,
                decision("cancel"),
            ),
            // Every remaining decision is an OBJECT shape whose payload comes
            // out of the request itself; the two the schema documents are
            // handled below, anything else is unknown and skipped.
            _ => continue,
        };
        choices.push(choice);
    }
    if let Some(amendment) = params
        .get("proposedExecpolicyAmendment")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
    {
        let prefix = amendment
            .first()
            .and_then(Value::as_str)
            .unwrap_or("this command");
        choices.push(ApprovalChoice::new(
            "accept_execpolicy_amendment",
            &format!("Yes, and don't ask again for commands that start with `{prefix}`"),
            ApprovalKind::AllowAlways,
            json!({ "decision": { "acceptWithExecpolicyAmendment": { "execpolicy_amendment": amendment } } }),
        ));
    }
    for (index, amendment) in params
        .get("proposedNetworkPolicyAmendments")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let host = amendment.get("host").and_then(Value::as_str).unwrap_or("this host");
        let allow = amendment.get("action").and_then(Value::as_str) == Some("allow");
        let label = if allow {
            format!("Yes, and allow {host} in the future")
        } else {
            format!("No, and block {host} in the future")
        };
        choices.push(ApprovalChoice::new(
            &format!("apply_network_policy_amendment:{index}"),
            &label,
            if allow { ApprovalKind::AllowAlways } else { ApprovalKind::RejectOnce },
            json!({ "decision": { "applyNetworkPolicyAmendment": { "network_policy_amendment": amendment } } }),
        ));
    }
    order_choices(choices)
}

/// `item/fileChange/requestApproval` — a fixed three-option set.
pub fn file_change_approval_choices() -> Vec<ApprovalChoice> {
    vec![
        ApprovalChoice::new("allow_once", "Yes, make the edits", ApprovalKind::AllowOnce, decision("accept")),
        ApprovalChoice::new(
            "allow_for_session",
            "Yes, and don't ask again for these files in this session",
            ApprovalKind::AllowAlways,
            decision("acceptForSession"),
        ),
        ApprovalChoice::new(
            "cancel",
            "No, and tell Codex what to do differently",
            ApprovalKind::RejectOnce,
            decision("cancel"),
        ),
    ]
}

/// `item/permissions/requestApproval` — the result is a granted PROFILE, not
/// a decision string: an allow echoes the requested profile back, a reject
/// grants nothing.
pub fn permission_approval_choices(params: &Value) -> Vec<ApprovalChoice> {
    let requested = params.get("permissions").cloned().unwrap_or_else(|| json!({}));
    vec![
        ApprovalChoice::new(
            "allow_permissions_turn",
            "Yes, for this turn",
            ApprovalKind::AllowOnce,
            json!({ "permissions": requested, "scope": "turn" }),
        ),
        ApprovalChoice::new(
            "allow_permissions_turn_strict_auto_review",
            "Yes, for this turn, reviewing every command",
            ApprovalKind::AllowOnce,
            json!({ "permissions": requested, "scope": "turn", "strictAutoReview": true }),
        ),
        ApprovalChoice::new(
            "allow_permissions_session",
            "Yes, for the rest of this session",
            ApprovalKind::AllowAlways,
            json!({ "permissions": requested, "scope": "session" }),
        ),
        ApprovalChoice::new(
            "reject_permissions",
            "No, keep the current permissions",
            ApprovalKind::RejectOnce,
            json!({ "permissions": {}, "scope": "turn" }),
        ),
    ]
}

/// codex-acp's ordering: allows first, rejects last, and a set with no allow
/// or no reject is rejected wholesale rather than shown half-broken.
fn order_choices(mut choices: Vec<ApprovalChoice>) -> Vec<ApprovalChoice> {
    choices.sort_by_key(|choice| match choice.kind {
        ApprovalKind::AllowOnce => 0,
        ApprovalKind::AllowAlways => 1,
        ApprovalKind::RejectOnce => 2,
    });
    let has_allow = choices
        .iter()
        .any(|choice| matches!(choice.kind, ApprovalKind::AllowOnce | ApprovalKind::AllowAlways));
    let has_reject = choices
        .iter()
        .any(|choice| matches!(choice.kind, ApprovalKind::RejectOnce));
    if !has_allow || !has_reject {
        return Vec::new();
    }
    choices
}

/// What to answer an approval with when the turn was cancelled, the client
/// dismissed the card, or the option id came back unknown. Every path lands
/// on "cancel" — never a silent approval.
pub fn cancel_result(method: &str) -> Value {
    match method {
        "item/permissions/requestApproval" => json!({ "permissions": {}, "scope": "turn" }),
        "item/tool/requestUserInput" => json!({ "answers": {} }),
        "mcpServer/elicitation/request" => json!({ "action": "cancel" }),
        // Both command and file-change approvals, and anything a newer codex
        // adds that we have not learned yet.
        _ => decision("cancel"),
    }
}

/// Apply a unified diff to `original`. `item/fileChange` carries a REAL diff
/// only for `update` changes (add and delete carry the whole file), and the
/// hunks are anchored against the file as it is on disk, so the card needs
/// both sides reconstructed. Returns `None` when a hunk does not match, which
/// is the caller's signal to try [`revert_unified_diff`] instead (the file may
/// already carry the change).
pub fn apply_unified_diff(original: &str, diff: &str) -> Option<String> {
    patch(original, diff, false)
}

/// The same hunks read backwards: given the PATCHED text, reconstruct the
/// original.
pub fn revert_unified_diff(patched: &str, diff: &str) -> Option<String> {
    patch(patched, diff, true)
}

fn patch(input: &str, diff: &str, reverse: bool) -> Option<String> {
    let source: Vec<&str> = input.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    let mut hunks = 0usize;
    let mut lines = diff.split('\n').peekable();
    while let Some(line) = lines.next() {
        let Some(header) = parse_hunk_header(line) else { continue };
        hunks += 1;
        let start = if reverse { header.new_start } else { header.old_start };
        // Hunk starts are 1-based; a zero start means an empty side.
        let start = start.saturating_sub(1);
        if start < cursor || start > source.len() {
            return None;
        }
        out.extend(source[cursor..start].iter().map(|line| (*line).to_string()));
        cursor = start;
        while let Some(body) = lines.peek() {
            if parse_hunk_header(body).is_some() {
                break;
            }
            let body = *body;
            let (marker, rest) = match body.chars().next() {
                Some(marker) => (marker, &body[marker.len_utf8()..]),
                // A fully empty line inside a hunk is a context line whose
                // content is empty (some producers drop the leading space).
                None => (' ', ""),
            };
            match marker {
                ' ' | '\t' => {
                    if source.get(cursor).copied() != Some(rest) {
                        return None;
                    }
                    out.push(rest.to_string());
                    cursor += 1;
                }
                '-' | '+' => {
                    let consumes = (marker == '-') != reverse;
                    if consumes {
                        if source.get(cursor).copied() != Some(rest) {
                            return None;
                        }
                        cursor += 1;
                    } else {
                        out.push(rest.to_string());
                    }
                }
                // "\ No newline at end of file" and the ---/+++ headers of a
                // second file in the same patch.
                '\\' => {}
                _ => break,
            }
            lines.next();
        }
    }
    if hunks == 0 {
        return None;
    }
    out.extend(source[cursor..].iter().map(|line| (*line).to_string()));
    Some(out.join("\n"))
}

struct HunkHeader {
    old_start: usize,
    new_start: usize,
}

/// `@@ -12,7 +12,9 @@ optional trailing context`.
fn parse_hunk_header(line: &str) -> Option<HunkHeader> {
    let rest = line.strip_prefix("@@ ")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(' ')?;
    Some(HunkHeader {
        old_start: parse_range_start(old.strip_prefix('-')?)?,
        new_start: parse_range_start(new.strip_prefix('+')?)?,
    })
}

fn parse_range_start(range: &str) -> Option<usize> {
    range
        .split_once(',')
        .map(|(start, _)| start)
        .unwrap_or(range)
        .parse()
        .ok()
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

#[cfg(test)]
mod wire_tests {
    use super::*;

    #[test]
    fn the_default_mode_is_full_access() {
        // D3: the ACP path must be as permissive as the PTY path, which always
        // passed --dangerously-bypass-approvals-and-sandbox.
        assert_eq!(CodexMode::default(), CodexMode::AgentFullAccess);
        assert_eq!(CodexMode::parse("agent"), Some(CodexMode::Agent));
        assert_eq!(CodexMode::parse("nonsense"), None);
    }

    #[test]
    fn a_workspace_mode_keeps_its_writable_roots() {
        let roots = vec![std::path::PathBuf::from("/work/tree")];
        assert_eq!(
            CodexMode::Agent.sandbox_policy(&roots),
            json!({ "type": "workspaceWrite", "writableRoots": ["/work/tree"] })
        );
        assert_eq!(CodexMode::Agent.approvals_reviewer(), "auto_review");
    }

    #[test]
    fn a_fast_turn_carries_the_service_tier() {
        let params = turn_start_request(&TurnRequest {
            thread_id: "t1",
            input: &[json!({ "type": "text", "text": "hi", "text_elements": [] })],
            model: Some("gpt-5.4-codex"),
            effort: Some("high"),
            mode: CodexMode::AgentFullAccess,
            service_tier: Some("fast"),
            writable_roots: &[],
        });
        assert_eq!(params["serviceTier"], json!("fast"));
        assert_eq!(params["effort"], json!("high"));
        assert_eq!(params["input"][0]["text"], json!("hi"));
    }

    #[test]
    fn the_model_id_round_trips_the_bracket_form() {
        let parsed = ModelId::parse("gpt-5.4-codex[high]");
        assert_eq!(parsed, ModelId::new("gpt-5.4-codex", Some("high".to_string())));
        assert_eq!(parsed.to_string(), "gpt-5.4-codex[high]");
        // A bare name is not an error: an older codex, or a model with no
        // reasoning efforts at all.
        assert_eq!(ModelId::parse("gpt-5.4").effort, None);
        assert_eq!(ModelId::parse("gpt-5.4").to_string(), "gpt-5.4");
    }

    #[test]
    fn the_command_headline_drops_the_shell_wrapper() {
        assert_eq!(strip_shell_prefix("bash -lc 'cargo test -p engine'"), "cargo test -p engine");
        assert_eq!(strip_shell_prefix("sh -c \"ls\""), "ls");
        assert_eq!(strip_shell_prefix("  git status  "), "git status");
    }

    #[test]
    fn command_approval_options_come_from_available_decisions() {
        // 0.153.3 sends the set; the installed 0.144.5 does not, and the
        // defaults stand in.
        let params = json!({ "availableDecisions": ["accept", "cancel", "somethingNew"] });
        let choices = command_approval_choices(&params);
        assert_eq!(
            choices.iter().map(|choice| choice.option_id.as_str()).collect::<Vec<_>>(),
            ["allow_once", "cancel"]
        );
        // An unknown decision is skipped, never guessed at.
        assert!(choices.iter().all(|choice| choice.option_id != "somethingNew"));
        assert_eq!(choices[0].result, json!({ "decision": "accept" }));

        let defaults = command_approval_choices(&json!({}));
        assert_eq!(
            defaults.iter().map(|choice| choice.option_id.as_str()).collect::<Vec<_>>(),
            ["allow_once", "allow_for_session", "decline", "cancel"]
        );
    }

    #[test]
    fn an_execpolicy_amendment_becomes_its_own_option() {
        let params = json!({
            "availableDecisions": ["accept", "cancel"],
            "proposedExecpolicyAmendment": ["git", "status"],
        });
        let choices = command_approval_choices(&params);
        let amendment = choices
            .iter()
            .find(|choice| choice.option_id == "accept_execpolicy_amendment")
            .expect("the amendment is offered");
        assert_eq!(amendment.kind, ApprovalKind::AllowAlways);
        assert_eq!(
            amendment.result,
            json!({ "decision": { "acceptWithExecpolicyAmendment": { "execpolicy_amendment": ["git", "status"] } } })
        );
        // Allows lead, rejects trail.
        assert_eq!(choices.last().expect("options").kind, ApprovalKind::RejectOnce);
    }

    #[test]
    fn an_option_set_with_no_reject_is_dropped_wholesale() {
        let choices = command_approval_choices(&json!({ "availableDecisions": ["accept"] }));
        assert!(choices.is_empty());
    }

    #[test]
    fn a_permission_grant_echoes_the_requested_profile() {
        let params = json!({ "permissions": { "network": { "enabled": true } } });
        let choices = permission_approval_choices(&params);
        assert_eq!(choices.len(), 4);
        assert_eq!(
            choices[0].result,
            json!({ "permissions": { "network": { "enabled": true } }, "scope": "turn" })
        );
        let reject = choices.last().expect("a reject");
        assert_eq!(reject.result, json!({ "permissions": {}, "scope": "turn" }));
    }

    #[test]
    fn every_unanswerable_approval_cancels() {
        assert_eq!(
            cancel_result("item/commandExecution/requestApproval"),
            json!({ "decision": "cancel" })
        );
        assert_eq!(cancel_result("item/tool/requestUserInput"), json!({ "answers": {} }));
        assert_eq!(cancel_result("mcpServer/elicitation/request"), json!({ "action": "cancel" }));
        // An approval kind a newer codex adds still gets a decision, never a
        // silent approval.
        assert_eq!(cancel_result("item/somethingNew/requestApproval"), json!({ "decision": "cancel" }));
    }

    #[test]
    fn a_file_change_diff_applies_and_reverts() {
        let original = "one\ntwo\nthree\n";
        let diff = "@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let patched = apply_unified_diff(original, diff).expect("the hunk applies");
        assert_eq!(patched, "one\nTWO\nthree\n");
        // The revert is what rescues a card whose file already carries the
        // change by the time the item is surfaced.
        assert_eq!(
            revert_unified_diff(&patched, diff).expect("the hunk reverts"),
            original
        );
    }

    #[test]
    fn a_diff_that_does_not_match_the_file_is_refused() {
        // Anchors that do not exist must fail rather than produce a plausible
        // wrong card.
        assert!(apply_unified_diff("one\ntwo\n", "@@ -1,2 +1,2 @@\n one\n-nope\n+TWO\n").is_none());
        assert!(apply_unified_diff("one\n", "not a diff at all").is_none());
    }

    #[test]
    fn the_opt_out_list_keeps_the_streams_we_read() {
        // Opting out of these would silently kill the command-output card and
        // the token stream.
        for method in [
            "item/commandExecution/outputDelta",
            "item/agentMessage/delta",
            "item/reasoning/textDelta",
            "thread/tokenUsage/updated",
        ] {
            assert!(
                !OPT_OUT_NOTIFICATIONS.contains(&method),
                "{method} must keep arriving"
            );
        }
        assert!(OPT_OUT_NOTIFICATIONS.contains(&"process/outputDelta"));
    }
}
