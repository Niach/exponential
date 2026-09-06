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
    pub permission_mode: Option<String>,
    pub effort: Option<String>,
    /// `system/task_started`: the subagent id that a later `can_use_tool`
    /// carries as `agent_id`.
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
    pub is_replay: bool,
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
    /// `system/task_started` frame.
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
}
