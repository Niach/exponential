//! EXP-746 — claude's private wire: the argv, the stream-json decoder and the
//! control-protocol envelopes. Owned by lane E2 (the argv is also spike
//! checkpoint #1; lane S1 landed the decoder + envelopes the spike needs).
//!
//! The argv is a flag-for-flag port of what the claude SDK spawns, in the
//! SAME order, so a diff against the vendored `sdk.mjs` stays readable.
//!
//! Decoder posture, from the upstream adapter and non-negotiable: every
//! struct is `#[serde(default)]`, every field optional, every enum has an
//! `Unknown` arm, nothing is `deny_unknown_fields`, and unmatched keys land
//! in a flattened `extra` map. The CLI ships new frame types and new fields
//! on every release; a strict decoder breaks the session on the next one.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::transport::{spawn_lines, ChildLines, StderrPolicy};

/// Whether `-p` leads the argv.
///
/// SPIKE CHECKPOINT #1 (S1), MEASURED against claude 2.1.263 on 2026-09-06:
/// the vendored SDK never passes it, the CLI's own `--help` says
/// `--input-format`/`--output-format`/`--include-partial-messages`/
/// `--permission-prompt-tool`/`--replay-user-messages` "only work with
/// --print", and in fact BOTH spellings behave identically over pipes — each
/// ran two prompts on one stdin, 26 frames apiece, and both raised
/// `can_use_tool`. It stays `true` because that is the spelling the binary
/// documents, so a future release that starts enforcing the help text fails
/// loudly at spawn rather than silently withholding permission requests.
/// Flipping it is this one const plus the two argv vectors below.
pub const CLAUDE_PRINT_MODE: bool = true;

/// Where the MCP config comes from. `File` is the zero-risk fallback (the
/// same `.exp-mcp.json` the PTY path writes); `Inline` keeps the `expu_` key
/// off disk entirely — spike checkpoint #2, and MEASURED green: the CLI
/// expands `${EXP_MCP_TOKEN}` inside a header VALUE of an inline
/// `--mcp-config` document from the child's own environment (a throwaway
/// listener received the expanded bearer, and the custom
/// `X-Exp-Session-Id` header rode along on both variants).
#[derive(Clone, Copy, Debug)]
pub enum McpConfig<'a> {
    File(&'a Path),
    Inline(&'a str),
}

/// Everything that varies in a claude ACP spawn.
pub struct ClaudeArgs<'a> {
    pub print_mode: bool,
    pub model: Option<&'a str>,
    /// `ultracode` wins over a plain effort level.
    pub effort: Option<&'a str>,
    /// The ARGV spelling, which is NOT the control-protocol spelling: the
    /// binary's choices are `acceptEdits|auto|bypassPermissions|manual|
    /// dontAsk|plan`, and `manual` is what the protocol calls `default`.
    /// Always pin one. With the flag absent the CLI takes the USER's settings
    /// default, which on a machine set to `auto` never raises a permission
    /// request at all (measured — that is what made the first spike run look
    /// like the stdio permission lane was dead).
    pub permission_mode: Option<&'a str>,
    pub allow_dangerous: bool,
    pub session_id: Option<&'a str>,
    pub resume: Option<&'a str>,
    pub fork_session: bool,
    pub mcp_config: Option<McpConfig<'a>>,
    pub strict_mcp_config: bool,
    /// The `--settings` file: `{}` on this path, kept ONLY because the
    /// reaper selects escaped claude processes by that path appearing in the
    /// process command line (EXP-300).
    pub settings: Option<&'a Path>,
    pub add_dirs: &'a [std::path::PathBuf],
    pub disallowed_tools: &'a [&'a str],
}

impl Default for ClaudeArgs<'_> {
    fn default() -> Self {
        ClaudeArgs {
            print_mode: CLAUDE_PRINT_MODE,
            model: None,
            effort: None,
            permission_mode: None,
            allow_dangerous: false,
            session_id: None,
            resume: None,
            fork_session: false,
            mcp_config: None,
            strict_mcp_config: false,
            settings: None,
            add_dirs: &[],
            disallowed_tools: &[],
        }
    }
}

/// The full argv after the program name.
///
/// Emission order mirrors `ProcessTransport.initialize` (`sdk.mjs:121`)
/// flag-for-flag so a diff against the vendored SDK stays readable. Two
/// spellings are deliberate and easy to get wrong:
/// - `--session-id`/`--resume` take the `=` form (the SDK's `VC` helper only
///   uses `--k=v` when the VALUE starts with `-`, but these three are pushed
///   pre-joined);
/// - `--disallowedTools` is camelCase, unlike every other flag.
///
/// `--replay-user-messages` is emitted as a BARE flag. The adapter routes it
/// through `extraArgs: {"replay-user-messages": ""}`, which emits a stray
/// empty positional argument after it; that empty string is an artefact of
/// the SDK's generic value pusher, not part of the contract.
pub fn claude_argv(args: &ClaudeArgs<'_>) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    let mut push = |value: &str| argv.push(value.to_string());

    if args.print_mode {
        push("-p");
    }
    push("--output-format");
    push("stream-json");
    push("--verbose");
    push("--input-format");
    push("stream-json");
    push("--include-partial-messages");
    // MANDATORY: without it the CLI never sends `can_use_tool` and the whole
    // permission/steering surface dies silently.
    push("--permission-prompt-tool");
    push("stdio");
    push("--replay-user-messages");

    if let Some(effort) = args.effort {
        push("--effort");
        push(effort);
    }
    if let Some(model) = args.model {
        push("--model");
        push(model);
    }
    match args.mcp_config {
        Some(McpConfig::File(path)) => {
            push("--mcp-config");
            push(&path.display().to_string());
        }
        Some(McpConfig::Inline(json)) => {
            push("--mcp-config");
            push(json);
        }
        None => {}
    }
    if args.strict_mcp_config {
        push("--strict-mcp-config");
    }
    if let Some(mode) = args.permission_mode {
        push("--permission-mode");
        push(mode);
    }
    if args.allow_dangerous {
        push("--allow-dangerously-skip-permissions");
    }
    // claude refuses --session-id together with --resume; the caller picks one.
    if let Some(resume) = args.resume {
        push(&format!("--resume={resume}"));
    } else if let Some(session_id) = args.session_id {
        push(&format!("--session-id={session_id}"));
    }
    if args.fork_session {
        push("--fork-session");
    }
    if let Some(settings) = args.settings {
        push("--settings");
        push(&settings.display().to_string());
    }
    for dir in args.add_dirs {
        push("--add-dir");
        push(&dir.display().to_string());
    }
    if !args.disallowed_tools.is_empty() {
        push("--disallowedTools");
        push(&args.disallowed_tools.join(","));
    }
    argv
}

// ---------------------------------------------------------------------------
// stdout frames
// ---------------------------------------------------------------------------

/// One line off claude's stdout.
///
/// `KeepAlive` is dropped silently and NEVER answered; `ControlCancelRequest`
/// aborts in-flight work for that `request_id` and is likewise not answered.
/// Everything unrecognized decodes as [`ClaudeOut::Unknown`] rather than
/// erroring — a new frame type in the next CLI release must not kill a live
/// session.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeOut {
    System(SystemMsg),
    Assistant(AssistantMsg),
    User(UserMsg),
    StreamEvent(StreamEventMsg),
    Result(ResultMsg),
    ControlRequest(ControlRequestFrame),
    ControlResponse(ControlResponseFrame),
    ControlCancelRequest(ControlCancelFrame),
    KeepAlive,
    #[serde(other)]
    Unknown,
}

impl ClaudeOut {
    /// Decode one stdout line. A malformed or unrecognized line is
    /// [`ClaudeOut::Unknown`]; the raw text stays with the caller for logging.
    pub fn parse(line: &str) -> ClaudeOut {
        serde_json::from_str(line).unwrap_or(ClaudeOut::Unknown)
    }

    /// A short, stable label for the frame — the left column of the spike's
    /// per-frame mapping table.
    pub fn label(&self) -> String {
        match self {
            ClaudeOut::System(msg) => format!("system/{}", msg.subtype),
            ClaudeOut::Assistant(_) => "assistant".to_string(),
            ClaudeOut::User(msg) => {
                if msg.is_replay {
                    "user (replay)".to_string()
                } else {
                    "user".to_string()
                }
            }
            ClaudeOut::StreamEvent(msg) => format!("stream_event/{}", msg.event_type()),
            ClaudeOut::Result(msg) => format!("result/{}", msg.subtype),
            ClaudeOut::ControlRequest(frame) => {
                format!("control_request/{}", frame.request.subtype())
            }
            ClaudeOut::ControlResponse(_) => "control_response".to_string(),
            ClaudeOut::ControlCancelRequest(_) => "control_cancel_request".to_string(),
            ClaudeOut::KeepAlive => "keep_alive".to_string(),
            ClaudeOut::Unknown => "unknown".to_string(),
        }
    }
}

/// `system` frames. The `subtype` stays a plain `String` on purpose: serde's
/// `#[serde(other)]` is unavailable for a string-valued fieldless enum, and
/// the subtype list grows every release. [`SystemSubtype::classify`] gives
/// the typed view where one is wanted.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SystemMsg {
    pub subtype: String,
    pub session_id: String,
    pub uuid: String,
    pub model: String,
    pub cwd: String,
    /// `msg_lifecycle_v1`, `interrupt_receipt_v1`, … — capability SNIFFING is
    /// how the upstream adapter version-gates. Never a version comparison.
    pub capabilities: Vec<String>,
    pub slash_commands: Vec<String>,
    pub tools: Vec<String>,
    /// camelCase on the wire (measured): `permissionMode`, not the
    /// snake_case its siblings use.
    #[serde(rename = "permissionMode", alias = "permission_mode")]
    pub permission_mode: Option<String>,
    pub effort: Option<String>,
    /// `system/task_started`: the subagent id that a later `can_use_tool`
    /// carries as `agent_id` — MEASURED end to end against the CLI, not
    /// inferred (EXP-753, `tests/fixtures/claude/subagent/`). The frame also
    /// carries the `tool_use_id` of the Task call that spawned it, which is
    /// what the adapter attributes nested rows to.
    pub task_id: Option<String>,
    pub parent_tool_use_id: Option<String>,
    pub compact_metadata: Option<CompactMetadata>,
    /// `system/status`: `compacting` | `requesting` | null.
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The `system` subtypes the engine acts on. Everything else is `Other` and
/// is logged, never fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemSubtype {
    Init,
    Status,
    CompactBoundary,
    SessionStateChanged,
    TaskStarted,
    TaskNotification,
    TaskUpdated,
    CommandsChanged,
    PermissionDenied,
    ModelRefusalFallback,
    Other,
}

impl SystemSubtype {
    pub fn classify(subtype: &str) -> SystemSubtype {
        match subtype {
            "init" => SystemSubtype::Init,
            "status" => SystemSubtype::Status,
            "compact_boundary" => SystemSubtype::CompactBoundary,
            "session_state_changed" => SystemSubtype::SessionStateChanged,
            "task_started" => SystemSubtype::TaskStarted,
            "task_notification" => SystemSubtype::TaskNotification,
            "task_updated" => SystemSubtype::TaskUpdated,
            "commands_changed" => SystemSubtype::CommandsChanged,
            "permission_denied" => SystemSubtype::PermissionDenied,
            "model_refusal_fallback" => SystemSubtype::ModelRefusalFallback,
            _ => SystemSubtype::Other,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CompactMetadata {
    /// `manual` | `auto`.
    pub trigger: String,
    pub pre_tokens: Option<u64>,
    pub post_tokens: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AssistantMsg {
    /// The raw Anthropic `BetaMessage`; E2 walks its content blocks.
    pub message: Value,
    pub parent_tool_use_id: Option<String>,
    pub subagent_type: Option<String>,
    pub session_id: String,
    pub uuid: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserMsg {
    pub message: Value,
    pub parent_tool_use_id: Option<String>,
    /// Message-level and carrying NO `tool_use_id`: honour it only when the
    /// message holds exactly one `tool_result` block.
    pub tool_use_result: Value,
    /// camelCase on the wire (measured), unlike its snake_case siblings; the
    /// alias keeps a hand-written fixture in either spelling decoding.
    #[serde(rename = "isReplay", alias = "is_replay")]
    pub is_replay: bool,
    #[serde(rename = "isSynthetic", alias = "is_synthetic")]
    pub is_synthetic: bool,
    pub session_id: String,
    pub uuid: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StreamEventMsg {
    /// A raw `BetaRawMessageStreamEvent` (`message_start`, `content_block_*`,
    /// `message_delta`, `message_stop`).
    pub event: Value,
    pub parent_tool_use_id: Option<String>,
    pub session_id: String,
    pub uuid: String,
    pub ttft_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl StreamEventMsg {
    pub fn event_type(&self) -> &str {
        self.event.get("type").and_then(Value::as_str).unwrap_or("?")
    }

    /// The delta kind of a `content_block_delta` (`text_delta`,
    /// `thinking_delta`, `input_json_delta`), if this is one.
    pub fn delta_type(&self) -> Option<&str> {
        self.event.get("delta")?.get("type")?.as_str()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ResultMsg {
    /// `success` | `error_during_execution` | `error_max_turns` |
    /// `error_max_budget_usd` | `error_max_structured_output_retries`.
    pub subtype: String,
    pub is_error: bool,
    pub duration_ms: u64,
    pub duration_api_ms: u64,
    pub num_turns: u32,
    pub result: String,
    /// Checked BEFORE the subtype switch: `refusal` is its own stop reason.
    pub stop_reason: Option<String>,
    pub total_cost_usd: Option<f64>,
    pub usage: Value,
    /// camelCase on the wire, unlike its siblings — the authoritative
    /// `contextWindow` per model lives in here.
    #[serde(rename = "modelUsage")]
    pub model_usage: Value,
    pub permission_denials: Vec<Value>,
    pub errors: Vec<String>,
    pub session_id: String,
    pub uuid: String,
    /// How many turns the CLI still holds behind this one. The ONLY signal
    /// that tells a folded-in steer (a mid-turn user message the CLI answered
    /// inside the running turn: one `result`, `queued_turn_count` 0) apart
    /// from a queued one (its own `result` later, counted here). `None` on a
    /// CLI that does not report it — then nothing is folded and every turn
    /// waits for a `result` of its own. The doctor's ACP floor
    /// (`MIN_CLAUDE_ACP_VERSION`, 2.1.263) does report it, so that arm is
    /// defensive.
    pub queued_turn_count: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ControlRequestFrame {
    pub request_id: String,
    pub request: ControlReq,
}

/// A control request FROM the CLI. `Unknown` is load-bearing: an unrecognized
/// `request_user_dialog` kind must be answered with SILENCE, never with a
/// synthesized cancel, so the engine has to be able to see it without
/// crashing on it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ControlReq {
    pub subtype: String,
    pub tool_name: String,
    pub input: Value,
    pub tool_use_id: Option<String>,
    /// Subagent attribution: equals the `task_id` of an earlier
    /// `system/task_started` frame (EXP-753, measured). The adapter resolves
    /// it back to that task's spawning tool call, so a permission raised
    /// INSIDE a subagent carries the same `subagentId` its nested chunks and
    /// tool calls do.
    pub agent_id: Option<String>,
    pub permission_suggestions: Value,
    /// `rule` | `mode` | `safetyCheck` | `classifier` | … — `safetyCheck` is
    /// the one that reaches the client even under `bypassPermissions`.
    pub decision_reason_type: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub blocked_path: Option<String>,
    /// `hook_callback` only.
    pub callback_id: Option<String>,
    /// `request_user_dialog` only.
    pub dialog_kind: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ControlReq {
    pub fn subtype(&self) -> &str {
        &self.subtype
    }

    /// `can_use_tool` for `AskUserQuestion` becomes an ACP elicitation form,
    /// not a permission request.
    pub fn is_ask_user_question(&self) -> bool {
        self.subtype == "can_use_tool" && self.tool_name == "AskUserQuestion"
    }

    pub fn is_exit_plan_mode(&self) -> bool {
        self.subtype == "can_use_tool" && self.tool_name == "ExitPlanMode"
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ControlResponseFrame {
    pub response: ControlResp,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ControlResp {
    /// `success` | `error`.
    pub subtype: String,
    pub request_id: String,
    pub response: Value,
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ControlResp {
    pub fn is_success(&self) -> bool {
        self.subtype == "success"
    }

    /// EXP-758: a response the ADAPTER synthesizes for a request the CLI will
    /// never answer (its stdout ended with the request still in flight).
    /// Shaped like the real error frame so the one caller path applies.
    pub fn failed(request_id: &str, reason: &str) -> ControlResp {
        ControlResp {
            subtype: "error".to_string(),
            request_id: request_id.to_string(),
            response: Value::Null,
            error: Some(reason.to_string()),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ControlCancelFrame {
    pub request_id: String,
}

// ---------------------------------------------------------------------------
// stdin envelopes
// ---------------------------------------------------------------------------

/// A fresh `request_id`. The SDK mints `Math.random().toString(36).slice(2,15)`
/// — an opaque 12-ish character token; only uniqueness within the process
/// matters, so a uuid's hex head is the same thing without a rand dep.
pub fn new_request_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// `{"type":"control_request","request_id":…,"request":{…}}`.
pub fn control_request(request_id: &str, request: Value) -> Value {
    json!({ "type": "control_request", "request_id": request_id, "request": request })
}

/// `{"type":"control_cancel_request","request_id":…}` — abandons an
/// outstanding request; the CLI sends no reply.
pub fn control_cancel_request(request_id: &str) -> Value {
    json!({ "type": "control_cancel_request", "request_id": request_id })
}

/// The answer to a CLI-initiated `control_request`.
pub fn control_response_success(request_id: &str, response: Value) -> Value {
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": response },
    })
}

pub fn control_response_error(request_id: &str, error: &str) -> Value {
    json!({
        "type": "control_response",
        "response": { "subtype": "error", "request_id": request_id, "error": error },
    })
}

/// The `initialize` request payload. `hooks` is the `{event: [matcher…]}` map
/// the engine registers (PostToolUse for real Edit/Write diffs, PostModelSwitch
/// for the effort re-clamp, TaskCreated/TaskCompleted for plan entries); an
/// empty map is legal and is what the spike sends.
pub fn initialize(hooks: Value) -> Value {
    json!({ "subtype": "initialize", "hooks": hooks })
}

/// `default | acceptEdits | plan | auto | bypassPermissions` — the CONTROL
/// spelling, which is not the argv spelling (`default` is `manual` there).
pub fn set_permission_mode(mode: &str) -> Value {
    json!({ "subtype": "set_permission_mode", "mode": mode })
}

/// `None` resets the model to the account default.
pub fn set_model(model: Option<&str>) -> Value {
    json!({ "subtype": "set_model", "model": model })
}

/// The mid-session settings layer: `{"effortLevel":…}` for the effort chip,
/// `{"agent":…}` for the agent chip. `--effort` is a spawn-time flag only.
pub fn apply_flag_settings(settings: Value) -> Value {
    json!({ "subtype": "apply_flag_settings", "settings": settings })
}

/// Never sent once the query stream has closed.
pub fn interrupt(cancel_queued: bool) -> Value {
    if cancel_queued {
        json!({ "subtype": "interrupt", "cancel_queued": true })
    } else {
        json!({ "subtype": "interrupt" })
    }
}

/// Only for the `/usage` command. Its sibling `get_context_usage` is NEVER
/// sent: it stalls ~15 s before the first turn and serializes ahead of an
/// awaited `set_model`.
pub fn get_usage() -> Value {
    json!({ "subtype": "get_usage" })
}

/// A user turn. `origin:{kind:"human"}` is load-bearing — the CLI's `isHuman`
/// trust gates fail CLOSED without it.
pub fn user_message(text: &str, session_id: Option<&str>) -> Value {
    json!({
        "type": "user",
        "message": { "role": "user", "content": [{ "type": "text", "text": text }] },
        "session_id": session_id,
        "parent_tool_use_id": Value::Null,
        "origin": { "kind": "human" },
    })
}

/// Answer a `can_use_tool`: run the tool as proposed.
pub fn permission_allow(tool_use_id: &str, updated_input: Value) -> Value {
    json!({
        "behavior": "allow",
        "updatedInput": updated_input,
        "toolUseID": tool_use_id,
        "decisionClassification": "user_temporary",
    })
}

/// Answer a `can_use_tool`: refuse. `interrupt` stops the turn as well — the
/// plan-mode "clear context" option and "keep planning" both need it.
pub fn permission_deny(tool_use_id: &str, message: &str, interrupt: bool) -> Value {
    json!({
        "behavior": "deny",
        "message": message,
        "interrupt": interrupt,
        "toolUseID": tool_use_id,
        "decisionClassification": "user_reject",
    })
}

// ---------------------------------------------------------------------------
// A minimal synchronous driver (the spike's harness)
// ---------------------------------------------------------------------------

/// One claude child driven synchronously: write a line, read frames until the
/// turn's `result`. The real adapter (E2) drives the same transport from the
/// ACP connection actor instead; this exists so the spike can exercise the
/// wire without the engine core.
pub struct ClaudeProcess {
    child: ChildLines,
}

/// What ended a [`ClaudeProcess::pump_turn`] loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEnd {
    /// A `result` frame arrived — the turn completed.
    Result,
    /// The child closed stdout (or exited) first.
    Eof,
    /// The deadline passed with the turn still open.
    Timeout,
}

impl ClaudeProcess {
    pub fn spawn(spec: &terminal::pty::SpawnSpec) -> std::io::Result<ClaudeProcess> {
        Ok(ClaudeProcess { child: spawn_lines(spec, StderrPolicy::Log)? })
    }

    pub fn pid(&self) -> u32 {
        self.child.pid
    }

    pub fn send(&self, value: &Value) -> std::io::Result<()> {
        self.child.writer.write_line(&value.to_string())
    }

    pub fn send_user(&self, text: &str, session_id: Option<&str>) -> std::io::Result<()> {
        self.send(&user_message(text, session_id))
    }

    /// Read frames until the turn's `result`, the child's EOF or `deadline`.
    ///
    /// `on_frame` sees every frame (raw line included, so the spike can dump
    /// captures) and returns the lines to write back — that is how a
    /// `can_use_tool` gets answered without a second thread. Returning frames
    /// from the callback keeps the single-writer discipline the real adapter
    /// also relies on.
    pub fn pump_turn<F>(&self, deadline: Instant, mut on_frame: F) -> TurnEnd
    where
        F: FnMut(&ClaudeOut, &str) -> Vec<Value>,
    {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return TurnEnd::Timeout;
            }
            let line = match self.child.lines.recv_timeout(deadline - now) {
                Ok(line) => line,
                Err(flume::RecvTimeoutError::Timeout) => return TurnEnd::Timeout,
                Err(flume::RecvTimeoutError::Disconnected) => return TurnEnd::Eof,
            };
            let frame = ClaudeOut::parse(&line);
            let is_result = matches!(frame, ClaudeOut::Result(_));
            for reply in on_frame(&frame, &line) {
                if let Err(err) = self.send(&reply) {
                    log::warn!("engine: claude stdin write failed: {err}");
                    return TurnEnd::Eof;
                }
            }
            if is_result {
                return TurnEnd::Result;
            }
        }
    }

    /// Await one control response for `request_id`, forwarding every other
    /// frame to `on_frame`.
    pub fn await_control_response<F>(
        &self,
        request_id: &str,
        deadline: Instant,
        mut on_frame: F,
    ) -> Option<ControlResp>
    where
        F: FnMut(&ClaudeOut, &str),
    {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let line = self.child.lines.recv_timeout(deadline - now).ok()?;
            let frame = ClaudeOut::parse(&line);
            if let ClaudeOut::ControlResponse(response) = &frame {
                if response.response.request_id == request_id {
                    return Some(response.response.clone());
                }
            }
            on_frame(&frame, &line);
        }
    }

    /// Exit status, once the child is gone.
    pub fn wait_exit(&self, timeout: Duration) -> Option<terminal::pty::ChildExit> {
        self.child.exit.recv_timeout(timeout).ok()
    }
}

/// Env every claude ACP child carries beyond `PreparedLaunch.spawn.env`.
///
/// `CLAUDE_CODE_ENTRYPOINT` is deliberately absent: the SDK sets it to
/// `sdk-ts`, but inside the CLI it only feeds telemetry plus a two-value
/// feature gate we do not want, and claiming to be the SDK would be a lie.
pub fn extra_env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    // The `system/session_state_changed` idle frame is the turn-settlement
    // backstop when a background subagent outlives the first `result`.
    env.insert("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS".to_string(), "1".to_string());
    env
}

// ---------------------------------------------------------------------------
// argv spellings and the inline MCP document
// ---------------------------------------------------------------------------

/// The CONTROL-protocol permission modes (`set_permission_mode`), in the order
/// the mode picker offers them. `dontAsk` is accepted by the CLI but never
/// offered, exactly as the upstream adapter does it.
pub const PERMISSION_MODES: [&str; 5] =
    ["default", "acceptEdits", "plan", "auto", "bypassPermissions"];

/// The argv spelling of a control-protocol permission mode.
///
/// `claude --help` (2.1.263) takes `acceptEdits|auto|bypassPermissions|manual|
/// dontAsk|plan`: the protocol's `default` is **`manual`** on the command line,
/// and passing `default` there is rejected by commander.
pub fn argv_permission_mode(mode: &str) -> &str {
    if mode == "default" {
        "manual"
    } else {
        mode
    }
}

/// The `--mcp-config` document for the ACP arm, with the personal key left as
/// an env REFERENCE.
///
/// Measured in the phase-1 spike: claude expands `${EXP_MCP_TOKEN}` inside a
/// header VALUE from the child's own environment, so the `expu_` key rides
/// `PreparedLaunch::spawn.env` and never lands on disk (which is what
/// `AgentMcp::ClaudeInline` exists for). The document shape mirrors
/// `coding::mcp_json::render_mcp_json` field for field so the two paths stay
/// comparable.
pub fn inline_mcp_config(url: &str, session_id: Option<&str>) -> String {
    let mut headers = Map::new();
    headers.insert(
        "Authorization".to_string(),
        Value::String(format!("Bearer ${{{}}}", coding::MCP_TOKEN_ENV)),
    );
    if let Some(session_id) = session_id {
        headers.insert(
            "X-Exp-Session-Id".to_string(),
            Value::String(session_id.to_string()),
        );
    }
    json!({
        "mcpServers": {
            "exponential": { "type": "http", "url": url, "headers": Value::Object(headers) },
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// initialize: hooks in, capabilities out
// ---------------------------------------------------------------------------

/// The hook callback ids the `initialize` request registers. The CLI echoes
/// the id back on every `hook_callback` control request, so these are the
/// adapter's routing keys — plain constants rather than minted ids because
/// exactly one callback is registered per event.
pub const HOOK_POST_TOOL_USE: &str = "exp_post_tool_use";
pub const HOOK_POST_MODEL_SWITCH: &str = "exp_post_model_switch";
pub const HOOK_TASK_CREATED: &str = "exp_task_created";
pub const HOOK_TASK_COMPLETED: &str = "exp_task_completed";

/// The `hooks` block of the `initialize` request.
///
/// Four events, no more: `PostToolUse` carries the REAL `structuredPatch` for
/// Edit/Write (which is why their `tool_result` renders nothing) and the
/// EnterPlanMode edge; `PostModelSwitch` mirrors a `/model` typed as a prompt
/// back into the picker; `TaskCreated`/`TaskCompleted` keep the task plan in
/// step. Deliberately NOT `PreCompact`/`PostCompact` — compaction is read off
/// `system/status` + `system/compact_boundary`, which also covers the
/// automatic compactions no hook fires for.
pub fn initialize_hooks() -> Value {
    let matcher = |id: &str| json!([{ "hookCallbackIds": [id] }]);
    json!({
        "PostToolUse": matcher(HOOK_POST_TOOL_USE),
        "PostModelSwitch": matcher(HOOK_POST_MODEL_SWITCH),
        "TaskCreated": matcher(HOOK_TASK_CREATED),
        "TaskCompleted": matcher(HOOK_TASK_COMPLETED),
    })
}

/// `get_usage` without the transcript scan: the behaviours section walks every
/// transcript touched in the last seven days, and the session screen renders
/// only the plan windows and the session's own cost.
pub fn get_usage_without_behaviors() -> Value {
    json!({ "subtype": "get_usage", "skip_behaviors": true })
}

/// The `initialize` control response — the ONLY place the CLI reports its
/// command catalog, model list and custom agents. Everything is optional: a
/// CLI that predates a field simply leaves the option empty.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InitializeInfo {
    pub commands: Vec<SlashCommandInfo>,
    /// Custom agents (`--agents`): the strings the `agent` config option offers.
    pub agents: Vec<Value>,
    pub models: Vec<ModelInfo>,
    pub output_style: String,
    /// `on` | `off` | absent — drives whether a Fast toggle is offered.
    pub fast_mode_state: Option<String>,
    /// The effort level the session will send next. Absent on 2.1.263, which
    /// is why the adapter seeds the effort chip from the launch flag instead.
    pub effort: Option<String>,
    pub hooks_applied: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SlashCommandInfo {
    pub name: String,
    pub description: String,
    /// camelCase on the wire; a string or (older CLIs) a list of strings.
    #[serde(rename = "argumentHint")]
    pub argument_hint: Value,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelInfo {
    pub value: String,
    pub display_name: String,
    pub description: String,
    pub supports_effort: bool,
    pub supported_effort_levels: Vec<String>,
    pub supports_fast_mode: bool,
    pub supports_auto_mode: bool,
}

/// One `/` command as the ACP client sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRow {
    pub name: String,
    pub description: String,
    pub hint: Option<String>,
}

/// Commands the CLI's TUI owns and an ACP client can neither run nor render.
const UNSUPPORTED_COMMANDS: [&str; 8] = [
    "clear",
    "cost",
    "keybindings-help",
    "login",
    "logout",
    "output-style:new",
    "release-notes",
    "todos",
];

/// The port of `getAvailableSlashCommands`: drop the CLI's terminal-only
/// commands (matched on the RAW name, before the rename), rewrite
/// `"foo (MCP)"` to `mcp:foo`, then drop the hard-coded unsupported set.
pub fn available_commands(commands: &[SlashCommandInfo], terminal: &[String]) -> Vec<CommandRow> {
    commands
        .iter()
        .filter(|command| !terminal.iter().any(|name| name == &command.name))
        .map(|command| {
            let name = match command.name.strip_suffix(" (MCP)") {
                Some(base) => format!("mcp:{base}"),
                None => command.name.clone(),
            };
            let hint = match &command.argument_hint {
                Value::String(hint) if !hint.is_empty() => Some(hint.clone()),
                Value::Array(parts) => {
                    let joined = parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ");
                    (!joined.is_empty()).then_some(joined)
                }
                _ => None,
            };
            CommandRow { name, description: command.description.clone(), hint }
        })
        .filter(|command| !UNSUPPORTED_COMMANDS.contains(&command.name.as_str()))
        .collect()
}

/// The payload of a `hook_callback` control request. One shape for all four
/// registered events; every field is optional because each event fills a
/// different subset.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HookInput {
    pub hook_event_name: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_response: Value,
    /// `PostModelSwitch`: where the switch came from. `sdk` is our own
    /// `set_model` and `resume` is a transcript restore — neither needs a
    /// re-sync.
    pub source: Option<String>,
    pub to_model: Option<String>,
    pub task_id: Option<String>,
    pub task_subject: Option<String>,
    pub task_description: Option<String>,
    pub task_active_form: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

// ---------------------------------------------------------------------------
// text hazards (ported from `tools.js` / `acp-agent.js`)
// ---------------------------------------------------------------------------

/// `toDisplayPath`: relative to the session cwd, absolute when outside it.
pub fn display_path(path: &str, cwd: &Path) -> String {
    let file = Path::new(path);
    match file.strip_prefix(cwd) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ => path.to_string(),
    }
}

const USAGE_OPEN: &str = "<usage>";
const USAGE_CLOSE: &str = "</usage>";

/// Drop a trailing `<usage>…</usage>` block (plus the newline before it).
/// Matched from the LAST opener so a report that merely mentions the marker
/// earlier is not truncated at the mention.
fn strip_usage_block(text: &str) -> &str {
    let body = text.trim_end();
    if !body.ends_with(USAGE_CLOSE) {
        return text;
    }
    let search_end = body.len() - USAGE_CLOSE.len();
    let Some(open) = body[..search_end].rfind(USAGE_OPEN) else {
        return text;
    };
    let cut = if open > 0 && body.as_bytes()[open - 1] == b'\n' { open - 1 } else { open };
    &body[..cut]
}

/// Drop a final `agentId: <id> (…)` continuation line. Anchored to a whole
/// line so a format change stops matching instead of mangling the report.
fn strip_agent_id_line(text: &str) -> &str {
    let body = text.trim_end();
    let line_start = body.rfind('\n').map(|at| at + 1).unwrap_or(0);
    let line = &body[line_start..];
    let Some(rest) = line.strip_prefix("agentId: ") else {
        return text;
    };
    let Some((id, tail)) = rest.split_once(' ') else {
        return text;
    };
    let id_ok = !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let tail_ok = tail.starts_with('(') && tail.ends_with(')') && !tail[1..].contains('(');
    if !id_ok || !tail_ok {
        return text;
    }
    &body[..line_start.saturating_sub(1)]
}

/// The model-directed trailer of an `Agent`/`Task` result: a `<usage>` totals
/// block and/or the `agentId: … (use SendMessage …)` continuation line. Both
/// are tail-anchored and independent (older CLIs emit only one).
pub fn strip_agent_trailer(text: &str) -> String {
    strip_agent_id_line(strip_usage_block(text)).to_string()
}

const PARTIAL_OUTPUT_LABEL: &str = "[Agent stopped at its turn limit — the output below is partial]";

/// Swap the CLI's model-directed "stopped at its N-turn limit" note for a
/// client-facing label. Anchored on the stable prefix only (CLI 2.1.246+).
pub fn replace_partial_output_note(text: &str) -> String {
    let Some(rest) = text.strip_prefix("NOTE: this agent stopped at its ") else {
        return text.to_string();
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !rest[digits.len()..].starts_with("-turn limit before finishing.") {
        return text.to_string();
    }
    match text.find("\n\n") {
        Some(end) => {
            let report = text[end + 2..].trim_start();
            if report.is_empty() {
                PARTIAL_OUTPUT_LABEL.to_string()
            } else {
                format!("{PARTIAL_OUTPUT_LABEL}\n\n{report}")
            }
        }
        None => PARTIAL_OUTPUT_LABEL.to_string(),
    }
}

/// The five wrapper tags the CLI persists around a local command's echo.
const LOCAL_COMMAND_MARKERS: [&str; 5] = [
    "command-name",
    "command-message",
    "command-args",
    "local-command-stdout",
    "local-command-stderr",
];

/// Single-pass removal of every `<tag>…</tag>` marker, matching the nearest
/// closing tag of the same name (what a lazy regex would do).
pub fn strip_marker_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while let Some(at) = rest.find('<') {
        for marker in LOCAL_COMMAND_MARKERS {
            let open = format!("<{marker}>");
            let close = format!("</{marker}>");
            if rest[at..].starts_with(&open) {
                if let Some(end) = rest[at + open.len()..].find(&close) {
                    out.push_str(&rest[..at]);
                    rest = &rest[at + open.len() + end + close.len()..];
                    continue 'outer;
                }
            }
        }
        out.push_str(&rest[..at + 1]);
        rest = &rest[at + 1..];
    }
    out.push_str(rest);
    out
}

/// User-message text with the local-command markers removed, or `None` when
/// nothing meaningful is left (the caller then skips the message). Real prose
/// mixed in beside the markers survives.
pub fn strip_local_command_metadata(text: &str) -> Option<String> {
    let stripped = strip_marker_tags(text);
    (!stripped.trim().is_empty()).then_some(stripped)
}

/// Wrap `text` in a fence long enough to survive fences inside it.
pub fn markdown_escape(text: &str) -> String {
    let mut fence = String::from("```");
    for line in text.lines() {
        let ticks = line.chars().take_while(|c| *c == '`').count();
        if line.starts_with("```") {
            while ticks >= fence.len() {
                fence.push('`');
            }
        }
    }
    let tail = if text.ends_with('\n') { "" } else { "\n" };
    format!("{fence}\n{text}{tail}{fence}")
}

/// The line-numbered `Read` view, rebuilt from the STRUCTURED output.
///
/// The raw `tool_result` text is the model-facing view: it embeds
/// `<system-reminder>` blocks (malicious-code checks, memory staleness notes)
/// that must never reach a client — and, on this path, never reach the relay.
pub fn numbered_read_view(content: &str, start_line: u64, truncated: Option<(u64, u64)>) -> String {
    let body = content.strip_suffix('\n').unwrap_or(content);
    let mut view = body
        .split('\n')
        .enumerate()
        .map(|(index, line)| format!("{}\t{line}", start_line + index as u64))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some((shown, total)) = truncated {
        view.push_str(&format!("\n[File truncated: showing {shown} of {total} lines]"));
    }
    view
}

/// The refusal a LOGGED-OUT claude reports as a perfectly ordinary
/// `result/success` (measured in the spike — there is no error frame). Without
/// this special case a logged-out agent looks like a well-behaved run that
/// happens to say nothing useful.
pub fn is_login_required_result(text: &str) -> bool {
    text.contains("Please run /login")
}

// ---------------------------------------------------------------------------
// usage
// ---------------------------------------------------------------------------

/// Anthropic's cumulative per-message token counts.
///
/// `message_delta.usage` is CUMULATIVE and only `output_tokens` is guaranteed
/// non-null, so every other field falls back to the previous snapshot instead
/// of resetting to zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenSnapshot {
    pub input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
}

impl TokenSnapshot {
    /// Fold one `usage` object in, keeping the previous value of every field
    /// the frame omitted.
    pub fn merge(&mut self, usage: &Value) {
        let field = |name: &str| usage.get(name).and_then(Value::as_u64);
        self.input = field("input_tokens").unwrap_or(self.input);
        self.cache_read = field("cache_read_input_tokens").unwrap_or(self.cache_read);
        self.cache_creation = field("cache_creation_input_tokens").unwrap_or(self.cache_creation);
        self.output = field("output_tokens").unwrap_or(self.output);
    }

    /// Context OCCUPANCY: everything the next request re-sends plus what this
    /// one produced.
    pub fn used(&self) -> u64 {
        self.input + self.cache_read + self.cache_creation + self.output
    }
}

/// The authoritative context window for `model`, from `result.modelUsage`.
/// Falls back to the largest window reported for any model in the map, which
/// is what the turn actually ran against when the id is spelled differently.
pub fn context_window_from_model_usage(model_usage: &Value, model: &str) -> Option<u64> {
    let entries = model_usage.as_object()?;
    if let Some(window) = entries
        .get(model)
        .and_then(|entry| entry.get("contextWindow"))
        .and_then(Value::as_u64)
    {
        return Some(window);
    }
    entries
        .values()
        .filter_map(|entry| entry.get("contextWindow").and_then(Value::as_u64))
        .max()
}

/// The heuristic the upstream adapter uses before any authoritative number
/// arrives: a `1m` in the model id means the million-token window.
pub fn infer_context_window(model: &str) -> u64 {
    let lower = model.to_ascii_lowercase();
    let millionish = lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|part| part == "1m");
    if millionish {
        1_000_000
    } else {
        200_000
    }
}

/// `/usage` rendered from the `get_usage` control response.
///
/// Deliberately small: the session screen has its own usage sheet, so this is
/// the plan windows plus this session's cost, not the CLI's full dialog.
pub fn render_usage_markdown(usage: &Value) -> String {
    let mut lines = vec!["**Usage**".to_string()];
    if let Some(cost) = usage
        .get("session")
        .and_then(|session| session.get("total_cost_usd"))
        .and_then(Value::as_f64)
    {
        lines.push(format!("- Session cost: ${cost:.2}"));
    }
    let windows: [(&str, &str); 3] = [
        ("five_hour", "5-hour limit"),
        ("seven_day", "Weekly limit"),
        ("seven_day_opus", "Weekly Opus limit"),
    ];
    if let Some(limits) = usage.get("rate_limits") {
        for (key, label) in windows {
            let Some(window) = limits.get(key).filter(|value| !value.is_null()) else {
                continue;
            };
            let Some(utilization) = window.get("utilization").and_then(Value::as_f64) else {
                continue;
            };
            let resets = window
                .get("resets_at")
                .and_then(Value::as_str)
                .map(|at| format!(" (resets {at})"))
                .unwrap_or_default();
            lines.push(format!("- {label}: {}% used{resets}", utilization.round() as i64));
        }
    }
    if lines.len() == 1 {
        lines.push("- No plan limits reported for this account.".to_string());
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// turn outcome
// ---------------------------------------------------------------------------

/// How a turn ended, in the vocabulary the adapter answers `session/prompt`
/// with. `AuthRequired` is not an ACP stop reason: it is the logged-out
/// special case, which fails the prompt instead of ending it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    AuthRequired,
}

/// The stop-reason table (`acp-agent.js:3311-3740`), in the CLI's own order of
/// precedence: a refusal is decided BEFORE the subtype switch, a cancelled
/// session beats every success, and only then does the subtype matter.
pub fn turn_outcome(result: &ResultMsg, cancelled: bool) -> TurnOutcome {
    if result.stop_reason.as_deref() == Some("refusal") {
        return TurnOutcome::Refusal;
    }
    if cancelled {
        return TurnOutcome::Cancelled;
    }
    match result.subtype.as_str() {
        "success" if is_login_required_result(&result.result) => TurnOutcome::AuthRequired,
        "success" if result.stop_reason.as_deref() == Some("max_tokens") => TurnOutcome::MaxTokens,
        "error_during_execution" if result.stop_reason.as_deref() == Some("max_tokens") => {
            TurnOutcome::MaxTokens
        }
        "error_max_turns" | "error_max_budget_usd" | "error_max_structured_output_retries" => {
            TurnOutcome::MaxTurnRequests
        }
        _ => TurnOutcome::EndTurn,
    }
}

/// Whether the `result` text is the turn's only output and must be forwarded
/// as an assistant message.
///
/// Two cases: a local-only command (`/context`, …) whose output IS the result,
/// and a cache-replayed turn that answers on the `result` alone with no
/// `stream_event` and no consolidated `assistant` message (adapter issue #453).
pub fn should_forward_result(
    local_only_command: bool,
    delivered_assistant_text: bool,
    result: &ResultMsg,
) -> bool {
    if result.result.trim().is_empty() {
        return false;
    }
    if local_only_command {
        return true;
    }
    let output_tokens = result
        .usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    !delivered_assistant_text && output_tokens == 0
}

/// Commands whose output the CLI puts in the turn's `result` instead of
/// streaming it as an assistant message.
pub const LOCAL_ONLY_COMMANDS: [&str; 3] = ["/context", "/heapdump", "/extra-usage"];

/// `/usage` is answered from the `get_usage` control request instead of a
/// turn — `get_context_usage` is NEVER sent (it stalls ~15 s before the first
/// turn and serializes ahead of an awaited `set_model`).
pub fn is_usage_command(text: &str) -> bool {
    text.trim() == "/usage"
}

/// A `/mcp:server:command args` prompt in the CLI's own spelling
/// (`/server:command (MCP) args`).
pub fn prompt_to_claude(text: &str) -> String {
    let Some(rest) = text.strip_prefix("/mcp:") else {
        return text.to_string();
    };
    let (head, args) = match rest.split_once(' ') {
        Some((head, args)) => (head, Some(args)),
        None => (rest, None),
    };
    let Some((server, command)) = head.split_once(':') else {
        return text.to_string();
    };
    if server.is_empty() || command.is_empty() {
        return text.to_string();
    }
    match args {
        Some(args) => format!("/{server}:{command} (MCP) {args}"),
        None => format!("/{server}:{command} (MCP)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base() -> Vec<String> {
        vec![
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--include-partial-messages".into(),
            "--permission-prompt-tool".into(),
            "stdio".into(),
            "--replay-user-messages".into(),
        ]
    }

    #[test]
    fn argv_without_print_mode_is_the_sdk_base_array() {
        let args = ClaudeArgs { print_mode: false, ..ClaudeArgs::default() };
        assert_eq!(claude_argv(&args), base());
    }

    #[test]
    fn argv_with_print_mode_leads_with_dash_p() {
        let args = ClaudeArgs { print_mode: true, ..ClaudeArgs::default() };
        let mut expected = vec!["-p".to_string()];
        expected.extend(base());
        assert_eq!(claude_argv(&args), expected);
    }

    #[test]
    fn argv_for_a_fresh_session_pins_the_session_id_with_the_equals_form() {
        let settings = PathBuf::from("/data/claude-hooks/1/sid.json");
        let mcp = PathBuf::from("/work/tree/.exp-mcp.json");
        let args = ClaudeArgs {
            print_mode: true,
            model: Some("claude-opus-4-6"),
            session_id: Some("11111111-2222-3333-4444-555555555555"),
            mcp_config: Some(McpConfig::File(&mcp)),
            strict_mcp_config: true,
            allow_dangerous: true,
            settings: Some(&settings),
            ..ClaudeArgs::default()
        };
        let argv = claude_argv(&args);
        let mut expected = vec!["-p".to_string()];
        expected.extend(base());
        expected.extend([
            "--model".to_string(),
            "claude-opus-4-6".to_string(),
            "--mcp-config".to_string(),
            "/work/tree/.exp-mcp.json".to_string(),
            "--strict-mcp-config".to_string(),
            "--allow-dangerously-skip-permissions".to_string(),
            "--session-id=11111111-2222-3333-4444-555555555555".to_string(),
            "--settings".to_string(),
            "/data/claude-hooks/1/sid.json".to_string(),
        ]);
        assert_eq!(argv, expected);
    }

    #[test]
    fn argv_for_a_resume_drops_the_session_id() {
        let args = ClaudeArgs {
            session_id: Some("ignored-when-resuming"),
            resume: Some("abc-123"),
            fork_session: true,
            ..ClaudeArgs::default()
        };
        let argv = claude_argv(&args);
        assert!(argv.contains(&"--resume=abc-123".to_string()));
        assert!(!argv.iter().any(|arg| arg.starts_with("--session-id")));
        assert!(argv.contains(&"--fork-session".to_string()));
    }

    #[test]
    fn argv_for_plan_mode_and_ultracode() {
        let args = ClaudeArgs {
            effort: Some("ultracode"),
            permission_mode: Some("plan"),
            ..ClaudeArgs::default()
        };
        let argv = claude_argv(&args);
        let effort = argv.iter().position(|arg| arg == "--effort").expect("--effort emitted");
        assert_eq!(argv[effort + 1], "ultracode");
        let mode = argv
            .iter()
            .position(|arg| arg == "--permission-mode")
            .expect("--permission-mode emitted");
        assert_eq!(argv[mode + 1], "plan");
        // The effort flag precedes the mode flag, mirroring the SDK order.
        assert!(effort < mode);
    }

    #[test]
    fn argv_with_inline_mcp_json_passes_the_document_as_one_argument() {
        let inline = r#"{"mcpServers":{"exponential":{"type":"http","url":"https://x/api/mcp"}}}"#;
        let dirs = vec![PathBuf::from("/extra/dir")];
        let args = ClaudeArgs {
            mcp_config: Some(McpConfig::Inline(inline)),
            add_dirs: &dirs,
            disallowed_tools: &["AskUserQuestion", "WebSearch"],
            ..ClaudeArgs::default()
        };
        let argv = claude_argv(&args);
        let at = argv.iter().position(|arg| arg == "--mcp-config").expect("--mcp-config emitted");
        assert_eq!(argv[at + 1], inline);
        let dir = argv.iter().position(|arg| arg == "--add-dir").expect("--add-dir emitted");
        assert_eq!(argv[dir + 1], "/extra/dir");
        // camelCase, unlike every other flag.
        let tools =
            argv.iter().position(|arg| arg == "--disallowedTools").expect("--disallowedTools");
        assert_eq!(argv[tools + 1], "AskUserQuestion,WebSearch");
    }

    #[test]
    fn keep_alive_decodes_and_is_not_an_error() {
        assert!(matches!(ClaudeOut::parse(r#"{"type":"keep_alive"}"#), ClaudeOut::KeepAlive));
    }

    #[test]
    fn control_cancel_request_decodes_with_its_request_id() {
        let frame = ClaudeOut::parse(r#"{"type":"control_cancel_request","request_id":"abc"}"#);
        match frame {
            ClaudeOut::ControlCancelRequest(cancel) => assert_eq!(cancel.request_id, "abc"),
            other => panic!("expected a cancel frame, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_top_level_type_decodes_as_unknown() {
        let frame = ClaudeOut::parse(r#"{"type":"prompt_suggestion","suggestions":[1,2]}"#);
        assert!(matches!(frame, ClaudeOut::Unknown));
    }

    #[test]
    fn an_unknown_system_subtype_still_decodes() {
        let frame = ClaudeOut::parse(
            r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"id":"t1"}]}"#,
        );
        match frame {
            ClaudeOut::System(system) => {
                assert_eq!(system.subtype, "background_tasks_changed");
                assert_eq!(SystemSubtype::classify(&system.subtype), SystemSubtype::Other);
                assert!(system.extra.contains_key("tasks"));
            }
            other => panic!("expected a system frame, got {other:?}"),
        }
    }

    #[test]
    fn a_result_with_unmodelled_fields_decodes() {
        let frame = ClaudeOut::parse(
            r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":12,
                "num_turns":1,"result":"ok","stop_reason":"end_turn","total_cost_usd":0.01,
                "usage":{"output_tokens":7},"modelUsage":{"m":{"contextWindow":200000}},
                "queued_turn_count":0,"terminal_reason":"done","brand_new_field":true}"#,
        );
        match frame {
            ClaudeOut::Result(result) => {
                assert_eq!(result.subtype, "success");
                assert_eq!(result.stop_reason.as_deref(), Some("end_turn"));
                assert_eq!(
                    result.model_usage["m"]["contextWindow"],
                    serde_json::json!(200000)
                );
                assert!(result.extra.contains_key("brand_new_field"));
            }
            other => panic!("expected a result frame, got {other:?}"),
        }
    }

    #[test]
    fn a_can_use_tool_request_exposes_its_tool_and_subagent() {
        let frame = ClaudeOut::parse(
            r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool",
                "tool_name":"Bash","input":{"command":"ls"},"tool_use_id":"toolu_1",
                "agent_id":"task_9","decision_reason_type":"safetyCheck"}}"#,
        );
        match frame {
            ClaudeOut::ControlRequest(request) => {
                assert_eq!(request.request_id, "r1");
                assert_eq!(request.request.tool_name, "Bash");
                assert_eq!(request.request.agent_id.as_deref(), Some("task_9"));
                assert_eq!(
                    request.request.decision_reason_type.as_deref(),
                    Some("safetyCheck")
                );
                assert!(!request.request.is_ask_user_question());
            }
            other => panic!("expected a control request, got {other:?}"),
        }
    }

    #[test]
    fn the_control_envelopes_match_the_sdk_shape() {
        // Parsed Values, never serialized strings: serde_json's preserve_order
        // is feature-unified ON in this workspace and key order flips between
        // single-crate and multi-crate test runs.
        assert_eq!(
            control_request("r7", set_permission_mode("plan")),
            json!({
                "type": "control_request",
                "request_id": "r7",
                "request": { "subtype": "set_permission_mode", "mode": "plan" },
            })
        );
        assert_eq!(
            control_response_success("r7", json!({ "behavior": "allow" })),
            json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": "r7",
                    "response": { "behavior": "allow" },
                },
            })
        );
        assert_eq!(
            control_response_error("r7", "nope"),
            json!({
                "type": "control_response",
                "response": { "subtype": "error", "request_id": "r7", "error": "nope" },
            })
        );
        assert_eq!(
            apply_flag_settings(json!({ "effortLevel": "high" })),
            json!({ "subtype": "apply_flag_settings", "settings": { "effortLevel": "high" } })
        );
        assert_eq!(interrupt(false), json!({ "subtype": "interrupt" }));
        assert_eq!(get_usage(), json!({ "subtype": "get_usage" }));
        assert_eq!(set_model(None), json!({ "subtype": "set_model", "model": null }));
    }

    #[test]
    fn a_user_message_declares_a_human_origin() {
        assert_eq!(
            user_message("hi", Some("s1")),
            json!({
                "type": "user",
                "message": { "role": "user", "content": [{ "type": "text", "text": "hi" }] },
                "session_id": "s1",
                "parent_tool_use_id": null,
                "origin": { "kind": "human" },
            })
        );
    }

    #[test]
    fn the_argv_spelling_of_the_default_mode_is_manual() {
        assert_eq!(argv_permission_mode("default"), "manual");
        assert_eq!(argv_permission_mode("plan"), "plan");
        assert_eq!(argv_permission_mode("bypassPermissions"), "bypassPermissions");
        // Every control-protocol mode has an argv spelling the CLI accepts.
        for mode in PERMISSION_MODES {
            assert!(matches!(
                argv_permission_mode(mode),
                "manual" | "acceptEdits" | "plan" | "auto" | "bypassPermissions"
            ));
        }
    }

    #[test]
    fn the_inline_mcp_document_leaves_the_key_as_an_env_reference() {
        let rendered = inline_mcp_config("https://app.example/api/mcp", Some("sess-1"));
        let parsed: Value = serde_json::from_str(&rendered).expect("inline config is JSON");
        assert_eq!(
            parsed,
            json!({
                "mcpServers": {
                    "exponential": {
                        "type": "http",
                        "url": "https://app.example/api/mcp",
                        "headers": {
                            "Authorization": "Bearer ${EXP_MCP_TOKEN}",
                            "X-Exp-Session-Id": "sess-1",
                        },
                    }
                }
            })
        );
        // No session id outside a launched session — the header is dropped,
        // never rendered as an empty string.
        let bare = inline_mcp_config("https://app.example/api/mcp", None);
        let parsed: Value = serde_json::from_str(&bare).expect("inline config is JSON");
        assert!(parsed["mcpServers"]["exponential"]["headers"]
            .get("X-Exp-Session-Id")
            .is_none());
        // The raw key never appears in the document.
        assert!(!rendered.contains("expu_"));
    }

    #[test]
    fn the_initialize_hooks_register_one_callback_per_event() {
        assert_eq!(
            initialize_hooks(),
            json!({
                "PostToolUse": [{ "hookCallbackIds": [HOOK_POST_TOOL_USE] }],
                "PostModelSwitch": [{ "hookCallbackIds": [HOOK_POST_MODEL_SWITCH] }],
                "TaskCreated": [{ "hookCallbackIds": [HOOK_TASK_CREATED] }],
                "TaskCompleted": [{ "hookCallbackIds": [HOOK_TASK_COMPLETED] }],
            })
        );
        // Compaction is read off the frames, never from a hook.
        let hooks = initialize_hooks();
        assert!(hooks.get("PreCompact").is_none());
        assert!(hooks.get("PostCompact").is_none());
    }

    #[test]
    fn an_initialize_response_with_unmodelled_fields_decodes() {
        let info: InitializeInfo = serde_json::from_value(json!({
            "commands": [{ "name": "compact", "description": "Compact", "argumentHint": "" }],
            "agents": [{ "name": "reviewer" }],
            "models": [{
                "value": "claude-opus-5[1m]",
                "displayName": "Opus",
                "description": "The big one",
                "supportsEffort": true,
                "supportedEffortLevels": ["low", "high"],
                "supportsFastMode": true,
                "supportsAutoMode": true,
            }],
            "output_style": "default",
            "account": { "email": "someone@example.com" },
            "brand_new_field": 7,
        }))
        .expect("a tolerant decode");
        assert_eq!(info.commands.len(), 1);
        assert_eq!(info.models[0].display_name, "Opus");
        assert!(info.models[0].supports_effort);
        assert_eq!(info.models[0].supported_effort_levels, vec!["low", "high"]);
        assert!(info.extra.contains_key("brand_new_field"));
    }

    #[test]
    fn available_commands_drop_terminal_and_unsupported_names_and_rename_mcp() {
        let commands = vec![
            SlashCommandInfo {
                name: "compact".into(),
                description: "Compact the conversation".into(),
                argument_hint: Value::String("<instructions>".into()),
                aliases: vec![],
            },
            SlashCommandInfo {
                name: "doctor".into(),
                description: "Terminal only".into(),
                ..SlashCommandInfo::default()
            },
            SlashCommandInfo {
                name: "clear".into(),
                description: "Unsupported".into(),
                ..SlashCommandInfo::default()
            },
            SlashCommandInfo {
                name: "issues (MCP)".into(),
                description: "From a server".into(),
                argument_hint: Value::Array(vec![json!("<id>"), json!("<title>")]),
                aliases: vec![],
            },
        ];
        let rows = available_commands(&commands, &["doctor".to_string()]);
        assert_eq!(
            rows,
            vec![
                CommandRow {
                    name: "compact".into(),
                    description: "Compact the conversation".into(),
                    hint: Some("<instructions>".into()),
                },
                CommandRow {
                    name: "mcp:issues".into(),
                    description: "From a server".into(),
                    hint: Some("<id> <title>".into()),
                },
            ]
        );
    }

    #[test]
    fn a_hook_callback_payload_decodes_with_its_structured_patch() {
        let input: HookInput = serde_json::from_value(json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": { "file_path": "/w/a.rs" },
            "tool_response": {
                "filePath": "/w/a.rs",
                "structuredPatch": [{ "newStart": 3, "lines": [" keep", "-old", "+new"] }],
            },
            "session_id": "s1",
        }))
        .expect("a tolerant decode");
        assert_eq!(input.hook_event_name, "PostToolUse");
        assert_eq!(input.tool_response["structuredPatch"][0]["newStart"], json!(3));
        assert!(input.extra.contains_key("session_id"));
    }

    #[test]
    fn display_path_relativizes_inside_the_worktree_only() {
        let cwd = PathBuf::from("/work/tree");
        assert_eq!(display_path("/work/tree/src/main.rs", &cwd), "src/main.rs");
        assert_eq!(display_path("/etc/hosts", &cwd), "/etc/hosts");
        // The cwd itself has no relative form worth showing.
        assert_eq!(display_path("/work/tree", &cwd), "/work/tree");
    }

    #[test]
    fn the_agent_trailer_is_stripped_tail_anchored() {
        let text = "Report body\n\nagentId: abc-123 (use SendMessage to continue)";
        assert_eq!(strip_agent_trailer(text), "Report body\n");
        let with_usage = "Report body\n<usage>total: 12</usage>";
        assert_eq!(strip_agent_trailer(with_usage), "Report body");
        // A mention of the marker earlier in the report is not a trailer.
        let mention = "I saw <usage> in the file and left it alone";
        assert_eq!(strip_agent_trailer(mention), mention);
        // A malformed continuation line stops matching instead of eating text.
        let malformed = "Report body\nagentId: not a real line";
        assert_eq!(strip_agent_trailer(malformed), malformed);
    }

    #[test]
    fn the_partial_output_note_becomes_a_client_facing_label() {
        let text = "NOTE: this agent stopped at its 30-turn limit before finishing. \
                    Send the agent a message.\n\nThe actual report.";
        assert_eq!(
            replace_partial_output_note(text),
            "[Agent stopped at its turn limit — the output below is partial]\n\nThe actual report."
        );
        assert_eq!(replace_partial_output_note("A normal report"), "A normal report");
    }

    #[test]
    fn local_command_markers_are_stripped_and_marker_only_messages_vanish() {
        assert_eq!(
            strip_local_command_metadata("<command-name>compact</command-name>hi").as_deref(),
            Some("hi")
        );
        assert_eq!(
            strip_local_command_metadata(
                "<command-name>usage</command-name><local-command-stdout>x</local-command-stdout>"
            ),
            None
        );
        // An unclosed marker is left alone rather than eating the rest.
        let unclosed = "<command-name>oops";
        assert_eq!(strip_marker_tags(unclosed), unclosed);
    }

    #[test]
    fn markdown_escape_outgrows_the_fences_inside_it() {
        assert_eq!(markdown_escape("plain"), "```\nplain\n```");
        assert_eq!(markdown_escape("```\ninner\n```"), "````\n```\ninner\n```\n````");
    }

    #[test]
    fn the_read_view_is_rebuilt_from_the_structured_output() {
        assert_eq!(numbered_read_view("a\nb\n", 1, None), "1\ta\n2\tb");
        assert_eq!(numbered_read_view("a\nb", 12, None), "12\ta\n13\tb");
        assert_eq!(
            numbered_read_view("a", 1, Some((1, 40))),
            "1\ta\n[File truncated: showing 1 of 40 lines]"
        );
    }

    #[test]
    fn a_logged_out_run_is_recognised_from_its_success_result() {
        let result = ResultMsg {
            subtype: "success".into(),
            result: "Not logged in · Please run /login".into(),
            ..ResultMsg::default()
        };
        assert!(is_login_required_result(&result.result));
        assert_eq!(turn_outcome(&result, false), TurnOutcome::AuthRequired);
    }

    #[test]
    fn cumulative_usage_falls_back_to_the_previous_snapshot() {
        let mut snapshot = TokenSnapshot::default();
        snapshot.merge(&json!({
            "input_tokens": 2,
            "cache_read_input_tokens": 11_474,
            "cache_creation_input_tokens": 6_721,
            "output_tokens": 4,
        }));
        assert_eq!(snapshot.used(), 2 + 11_474 + 6_721 + 4);
        // A `message_delta` carries only output_tokens: everything else keeps
        // its previous value instead of collapsing to zero.
        snapshot.merge(&json!({ "output_tokens": 9 }));
        assert_eq!(
            snapshot,
            TokenSnapshot { input: 2, cache_read: 11_474, cache_creation: 6_721, output: 9 }
        );
    }

    #[test]
    fn the_context_window_prefers_the_authoritative_model_usage() {
        let model_usage = json!({
            "claude-haiku-4-5": { "contextWindow": 200_000 },
            "claude-opus-5[1m]": { "contextWindow": 1_000_000 },
        });
        assert_eq!(
            context_window_from_model_usage(&model_usage, "claude-opus-5[1m]"),
            Some(1_000_000)
        );
        // An id spelled differently still resolves to the turn's real window.
        assert_eq!(context_window_from_model_usage(&model_usage, "opus"), Some(1_000_000));
        assert_eq!(context_window_from_model_usage(&json!({}), "opus"), None);
        assert_eq!(infer_context_window("claude-opus-5[1m]"), 1_000_000);
        assert_eq!(infer_context_window("claude-sonnet-5"), 200_000);
    }

    #[test]
    fn the_usage_markdown_renders_the_plan_windows() {
        let rendered = render_usage_markdown(&json!({
            "session": { "total_cost_usd": 1.239 },
            "rate_limits": {
                "five_hour": { "utilization": 4.4, "resets_at": "2026-09-06T18:00:00Z" },
                "seven_day": { "utilization": null },
                "seven_day_opus": null,
            },
        }));
        assert_eq!(
            rendered,
            "**Usage**\n- Session cost: $1.24\n- 5-hour limit: 4% used (resets 2026-09-06T18:00:00Z)"
        );
        assert_eq!(
            render_usage_markdown(&json!({})),
            "**Usage**\n- No plan limits reported for this account."
        );
    }

    #[test]
    fn the_stop_reason_table_puts_a_refusal_before_the_subtype() {
        let refusal = ResultMsg {
            subtype: "error_during_execution".into(),
            stop_reason: Some("refusal".into()),
            ..ResultMsg::default()
        };
        assert_eq!(turn_outcome(&refusal, true), TurnOutcome::Refusal);
        let plain = ResultMsg { subtype: "success".into(), ..ResultMsg::default() };
        assert_eq!(turn_outcome(&plain, true), TurnOutcome::Cancelled);
        assert_eq!(turn_outcome(&plain, false), TurnOutcome::EndTurn);
        let max_tokens = ResultMsg {
            subtype: "success".into(),
            stop_reason: Some("max_tokens".into()),
            ..ResultMsg::default()
        };
        assert_eq!(turn_outcome(&max_tokens, false), TurnOutcome::MaxTokens);
        let max_turns = ResultMsg { subtype: "error_max_turns".into(), ..ResultMsg::default() };
        assert_eq!(turn_outcome(&max_turns, false), TurnOutcome::MaxTurnRequests);
    }

    #[test]
    fn a_cache_replayed_turn_forwards_its_result_text() {
        let replayed = ResultMsg {
            subtype: "success".into(),
            result: "Already answered from cache".into(),
            usage: json!({ "output_tokens": 0 }),
            ..ResultMsg::default()
        };
        assert!(should_forward_result(false, false, &replayed));
        // Text already streamed as assistant chunks is never repeated.
        assert!(!should_forward_result(false, true, &replayed));
        // …unless the turn was a local-only command, whose output IS the result.
        assert!(should_forward_result(true, true, &replayed));
        let normal = ResultMsg {
            subtype: "success".into(),
            result: "hi".into(),
            usage: json!({ "output_tokens": 4 }),
            ..ResultMsg::default()
        };
        assert!(!should_forward_result(false, false, &normal));
    }

    #[test]
    fn an_mcp_command_prompt_is_rewritten_to_the_cli_spelling() {
        assert_eq!(
            prompt_to_claude("/mcp:exponential:issues_get EXP-1"),
            "/exponential:issues_get (MCP) EXP-1"
        );
        assert_eq!(prompt_to_claude("/mcp:exponential:list"), "/exponential:list (MCP)");
        assert_eq!(prompt_to_claude("/compact"), "/compact");
        assert!(is_usage_command("  /usage "));
        assert!(!is_usage_command("/usage limits"));
        assert!(LOCAL_ONLY_COMMANDS.contains(&"/context"));
    }
}
