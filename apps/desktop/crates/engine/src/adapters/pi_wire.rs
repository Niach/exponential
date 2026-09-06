//! EXP-746 — pi's private wire: `--mode rpc` line JSON. Owned by lane E4.
//!
//! Four shapes share one stdout stream, discriminated ONLY by `type`, and an
//! unrecognized `type` must be ignorable rather than fatal. Strict LF framing
//! both ways (pi's own jsonl reader is deliberately LF-only because
//! U+2028/U+2029 are legal inside JSON strings).

use std::time::Instant;

use serde_json::{json, Value};

use crate::transport::{spawn_lines, ChildLines, StderrPolicy};

/// Everything that varies in a pi ACP spawn.
pub struct PiArgs<'a> {
    pub model: Option<&'a str>,
    pub thinking: Option<&'a str>,
    /// pi resumes by FILE, not by id.
    pub session_file: Option<&'a std::path::Path>,
    /// Extensions to load with `-e`. On this path that is the MCP bridge and
    /// NOTHING else: the observer and plan extensions belong to the PTY path.
    pub extensions: &'a [std::path::PathBuf],
}

impl Default for PiArgs<'_> {
    fn default() -> Self {
        PiArgs { model: None, thinking: None, session_file: None, extensions: &[] }
    }
}

/// The full argv after the program name (always starts `--mode rpc`).
pub fn pi_argv(args: &PiArgs<'_>) -> Vec<String> {
    let mut argv = vec!["--mode".to_string(), "rpc".to_string()];
    if let Some(model) = args.model {
        argv.push("--model".to_string());
        argv.push(model.to_string());
    }
    if let Some(thinking) = args.thinking {
        argv.push("--thinking".to_string());
        argv.push(thinking.to_string());
    }
    if let Some(session) = args.session_file {
        argv.push("--session".to_string());
        argv.push(session.display().to_string());
    }
    for extension in args.extensions {
        argv.push("-e".to_string());
        argv.push(extension.display().to_string());
    }
    argv
}

/// One line off pi's stdout.
#[derive(Clone, Debug)]
pub enum PiOut {
    /// An answer to one of our commands.
    Response {
        id: Option<String>,
        command: String,
        success: bool,
        data: Value,
    },
    /// The only interactive channel pi has — an extension asking the user.
    ExtensionUiRequest {
        id: String,
        method: String,
        params: Value,
    },
    ExtensionError {
        message: String,
    },
    /// A raw `AgentSessionEvent` (message/tool/turn/compaction/…).
    Event {
        kind: String,
        event: Value,
    },
    /// An unknown `type` — logged and dropped, never fatal.
    Unknown,
}

impl PiOut {
    /// A short, stable label — the left column of the spike's mapping table.
    pub fn label(&self) -> String {
        match self {
            PiOut::Response { command, success, .. } => {
                format!("response/{command}{}", if *success { "" } else { " (failed)" })
            }
            PiOut::ExtensionUiRequest { method, .. } => format!("extension_ui_request/{method}"),
            PiOut::ExtensionError { .. } => "extension_error".to_string(),
            PiOut::Event { kind, event } => match kind.as_str() {
                // The streaming deltas all share one event type; the delta
                // kind is what decides message vs thought.
                "message_update" => {
                    let delta = event
                        .get("assistantMessageEvent")
                        .and_then(|inner| inner.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("?");
                    format!("message_update/{delta}")
                }
                other => other.to_string(),
            },
            PiOut::Unknown => "unknown".to_string(),
        }
    }
}

/// Classify one line off pi's stdout.
pub fn parse_line(line: &str) -> PiOut {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return PiOut::Unknown;
    };
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return PiOut::Unknown;
    };
    match kind {
        "response" => PiOut::Response {
            id: value.get("id").and_then(Value::as_str).map(str::to_string),
            command: value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            success: value.get("success").and_then(Value::as_bool).unwrap_or(false),
            data: value
                .get("data")
                .cloned()
                .or_else(|| value.get("error").cloned())
                .unwrap_or(Value::Null),
        },
        "extension_ui_request" => PiOut::ExtensionUiRequest {
            id: value.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
            method: value.get("method").and_then(Value::as_str).unwrap_or_default().to_string(),
            params: value.clone(),
        },
        "extension_error" => PiOut::ExtensionError {
            message: value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("extension error")
                .to_string(),
        },
        // Everything else pi emits on this stream is a raw AgentSessionEvent.
        // Deliberately NOT an enum: pi adds event types between releases and a
        // closed set would turn a new one into a dead session.
        other => PiOut::Event { kind: other.to_string(), event: value },
    }
}

/// Answer an `extension_ui_request`. Every shape shares the envelope; the
/// payload key differs per method (`confirmed`, `value`, …), and a cancel is
/// `{cancelled: true}` for all of them.
pub fn extension_ui_response(id: &str, payload: Value) -> Value {
    let mut response = json!({ "type": "extension_ui_response", "id": id });
    if let (Some(object), Some(payload)) = (response.as_object_mut(), payload.as_object()) {
        for (key, value) in payload {
            object.insert(key.clone(), value.clone());
        }
    }
    response
}

/// One rpc command. `id` correlates the response; pi echoes it back.
pub fn command(id: &str, command: &str, params: Value) -> Value {
    let mut line = json!({ "id": id, "type": command });
    if let (Some(object), Some(params)) = (line.as_object_mut(), params.as_object()) {
        for (key, value) in params {
            object.insert(key.clone(), value.clone());
        }
    }
    line
}

// ---------------------------------------------------------------------------
// Typed views of the shapes the adapter reads (EXP-746 E4)
//
// Everything below is PURE: `Value` in, small owned structs out, no channel,
// no clock, no filesystem. `pi.rs` owns the ACP mapping; this half owns
// "what did pi actually say", so a fixture line can be asserted on its own.
// Every reader is lenient by construction — pi adds fields and event types
// between releases and a missing one must degrade, never fail a session.
// ---------------------------------------------------------------------------

/// pi's `ThinkingLevel`, in pi's own order (`pi-agent-core` `types.d.ts`).
pub const THINKING_LEVELS: [&str; 7] =
    ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// `steering_mode` and `follow_up_mode` share one two-value vocabulary.
pub const QUEUE_MODES: [&str; 2] = ["all", "one-at-a-time"];

/// The commands pi exposes as RPC VERBS but never advertises through
/// `get_commands` (which lists only extension/prompt/skill commands), so the
/// `/` menu would otherwise lose them. `(name, description, argument hint)`;
/// an empty hint means the command takes no argument.
pub const SYNTHESIZED_COMMANDS: [(&str, &str, &str); 8] = [
    ("compact", "Compact the conversation", ""),
    ("new", "Start a new session", ""),
    ("model", "Switch the model", "provider/model"),
    ("thinking", "Set the thinking level", "level"),
    ("name", "Name this session", "name"),
    ("fork", "Fork the session at an entry", "entry id"),
    ("clone", "Clone this session", ""),
    ("export", "Export the session as HTML", "path"),
];

/// One entry of pi's model list. `provider` + `id` are what `set_model`
/// takes; `context_window` is the denominator of every `usage` event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiModel {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub context_window: u64,
}

impl PiModel {
    /// The stable value id the config option uses. pi needs BOTH halves back
    /// for `set_model`, and provider ids carry no `/`, so the first `/`
    /// splits it again ([`PiModel::split_value_id`]).
    pub fn value_id(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }

    /// Inverse of [`PiModel::value_id`]. A value with no `/` is taken as a
    /// bare model id with an unknown provider (a hand-typed CLI default).
    pub fn split_value_id(value: &str) -> (String, String) {
        match value.split_once('/') {
            Some((provider, id)) => (provider.to_string(), id.to_string()),
            None => (String::new(), value.to_string()),
        }
    }
}

/// Read one `Model` object (the `get_state.model`, `set_model` and
/// `get_available_models` shapes are the same type).
pub fn parse_model(value: &Value) -> Option<PiModel> {
    let id = value.get("id").and_then(Value::as_str)?;
    Some(PiModel {
        provider: value
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        id: id.to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        context_window: value
            .get("contextWindow")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    })
}

/// `get_available_models` → `{models: Model[]}`.
pub fn parse_models(data: &Value) -> Vec<PiModel> {
    data.get("models")
        .and_then(Value::as_array)
        .map(|models| models.iter().filter_map(parse_model).collect())
        .unwrap_or_default()
}

/// `RpcSessionState` — everything the config snapshot and the session
/// identity are built from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PiState {
    pub model: Option<PiModel>,
    pub thinking_level: String,
    pub steering_mode: String,
    pub follow_up_mode: String,
    pub auto_compaction: bool,
    /// pi is FILE-path keyed: this is the run's transcript and its resume
    /// handle in one.
    pub session_file: Option<String>,
    pub session_id: String,
    pub session_name: Option<String>,
}

/// `get_state` → [`PiState`].
pub fn parse_state(data: &Value) -> PiState {
    PiState {
        model: data.get("model").and_then(parse_model),
        thinking_level: data
            .get("thinkingLevel")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        steering_mode: data
            .get("steeringMode")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        follow_up_mode: data
            .get("followUpMode")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        auto_compaction: data
            .get("autoCompactionEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        session_file: data
            .get("sessionFile")
            .and_then(Value::as_str)
            .map(str::to_string),
        session_id: data
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        session_name: data
            .get("sessionName")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// One `RpcSlashCommand`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiSlashCommand {
    pub name: String,
    pub description: String,
}

/// `get_commands` → `{commands: RpcSlashCommand[]}`.
pub fn parse_commands(data: &Value) -> Vec<PiSlashCommand> {
    data.get("commands")
        .and_then(Value::as_array)
        .map(|commands| {
            commands
                .iter()
                .filter_map(|command| {
                    let name = command.get("name").and_then(Value::as_str)?;
                    Some(PiSlashCommand {
                        name: name.to_string(),
                        description: command
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What a turn cost. pi is the ONE agent that reports money, so `cost_usd`
/// is real here and `None` everywhere else.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PiUsage {
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
}

/// Read `message.usage` off a `turn_end` (or any message-bearing event).
pub fn parse_usage(message: &Value) -> Option<PiUsage> {
    let usage = message.get("usage")?;
    Some(PiUsage {
        total_tokens: usage
            .get("totalTokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        cost_usd: usage
            .get("cost")
            .and_then(|cost| cost.get("total"))
            .and_then(Value::as_f64),
    })
}

/// An assistant message's coalescing key. `responseId` when the provider
/// gave one, else the message timestamp — both are stable for the whole
/// message, which is all the mapper's chunk coalescer needs.
pub fn message_key(message: &Value) -> Option<String> {
    if let Some(id) = message.get("responseId").and_then(Value::as_str) {
        return Some(id.to_string());
    }
    message
        .get("timestamp")
        .and_then(Value::as_i64)
        .map(|timestamp| timestamp.to_string())
}

/// The pi events the adapter acts on. Everything else pi emits
/// (`queue_update`, `entry_appended`, `auto_retry_*`, `agent_start`,
/// `turn_start`, an event added after this release) classifies as
/// [`PiEvent::Ignored`] — a new event type must never end a session.
///
/// Two of the mappings are easy to get wrong and were measured in the spike:
/// `agent_settled` (NOT `agent_end`, which can be followed by a retry) is
/// the idle edge, and an errored turn arrives as a normal message whose
/// `stopReason` is `error` with the text in `errorMessage`.
#[derive(Clone, Debug, PartialEq)]
pub enum PiEvent {
    TextDelta {
        message_id: Option<String>,
        text: String,
    },
    ThinkingDelta {
        message_id: Option<String>,
        text: String,
    },
    ToolStart {
        id: String,
        name: String,
        args: Value,
    },
    ToolUpdate {
        id: String,
        output: Option<String>,
    },
    ToolEnd {
        id: String,
        result: Value,
        is_error: bool,
    },
    /// The turn's accounting. `error` is set for a failed turn.
    TurnEnd {
        usage: Option<PiUsage>,
        error: Option<String>,
    },
    /// The idle edge.
    Settled,
    CompactionStart {
        /// `manual` | `threshold` | `overflow` — normalized on the wire by
        /// `steer::normalize_compaction_trigger` (everything but `manual`
        /// is `auto`).
        reason: Option<String>,
    },
    CompactionEnd {
        reason: Option<String>,
        error: Option<String>,
    },
    ThinkingLevelChanged {
        level: String,
    },
    SessionInfoChanged {
        name: Option<String>,
    },
    Ignored,
}

/// Classify one raw `AgentSessionEvent` (the `kind`/`event` pair
/// [`PiOut::Event`] carries).
pub fn classify_event(kind: &str, event: &Value) -> PiEvent {
    match kind {
        "message_update" => {
            let inner = &event["assistantMessageEvent"];
            let delta = inner
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let message_id = message_key(&event["message"]);
            match inner.get("type").and_then(Value::as_str).unwrap_or_default() {
                "text_delta" => PiEvent::TextDelta { message_id, text: delta },
                "thinking_delta" => PiEvent::ThinkingDelta { message_id, text: delta },
                _ => PiEvent::Ignored,
            }
        }
        // The error half of a turn: pi answers a rejected model or a provider
        // outage with an ordinary message whose stopReason is `error`.
        "message_end" => match error_message(&event["message"]) {
            Some(error) => PiEvent::TurnEnd { usage: None, error: Some(error) },
            None => PiEvent::Ignored,
        },
        "tool_execution_start" => PiEvent::ToolStart {
            id: tool_call_id(event),
            name: event
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            args: event.get("args").cloned().unwrap_or(Value::Null),
        },
        "tool_execution_update" => PiEvent::ToolUpdate {
            id: tool_call_id(event),
            output: partial_output(event.get("partialResult")),
        },
        "tool_execution_end" => PiEvent::ToolEnd {
            id: tool_call_id(event),
            result: event.get("result").cloned().unwrap_or(Value::Null),
            is_error: event.get("isError").and_then(Value::as_bool).unwrap_or(false),
        },
        "turn_end" => PiEvent::TurnEnd {
            usage: parse_usage(&event["message"]),
            error: error_message(&event["message"]),
        },
        "agent_settled" => PiEvent::Settled,
        "compaction_start" => PiEvent::CompactionStart {
            reason: event
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "compaction_end" => PiEvent::CompactionEnd {
            reason: event
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string),
            error: event
                .get("errorMessage")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|_| {
                    event.get("aborted").and_then(Value::as_bool).unwrap_or(false)
                        || event.get("errorMessage").is_some()
                }),
        },
        "thinking_level_changed" => PiEvent::ThinkingLevelChanged {
            level: event
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        "session_info_changed" => PiEvent::SessionInfoChanged {
            name: event.get("name").and_then(Value::as_str).map(str::to_string),
        },
        _ => PiEvent::Ignored,
    }
}

fn tool_call_id(event: &Value) -> String {
    event
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `stopReason: "error"` carries its text in `errorMessage` (spike finding).
fn error_message(message: &Value) -> Option<String> {
    let failed = message
        .get("stopReason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason == "error");
    if !failed {
        return None;
    }
    Some(
        message
            .get("errorMessage")
            .and_then(Value::as_str)
            .unwrap_or("the agent stopped with an error")
            .to_string(),
    )
}

/// A tool's partial result — a bash tool streams a string, everything else
/// streams a structure we do not render mid-flight.
fn partial_output(partial: Option<&Value>) -> Option<String> {
    match partial? {
        Value::String(text) => Some(text.clone()),
        Value::Object(fields) => fields
            .get("output")
            .or_else(|| fields.get("stdout"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// The interactive half of `extension_ui_request`. pi's extensions are its
/// ONLY permission-ish channel (the repo's own plan gate uses `confirm`),
/// and only these four shapes wait for an answer — `notify`, `setStatus`,
/// `setWidget`, `setTitle` and `set_editor_text` are fire-and-forget
/// (`rpc-mode.js`), so answering them would be a protocol error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PiUi {
    Confirm { title: String, message: String },
    Select { title: String, options: Vec<String> },
    Input { title: String, placeholder: Option<String> },
    Editor { title: String, prefill: Option<String> },
    /// A notice: rendered, never answered.
    Notify { message: String },
    /// Chrome we have no surface for; drop it silently.
    Ignored,
}

/// Classify one `extension_ui_request` by its `method`.
pub fn classify_ui(method: &str, params: &Value) -> PiUi {
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match method {
        "confirm" => PiUi::Confirm {
            title,
            message: params
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        "select" => PiUi::Select {
            title,
            options: params
                .get("options")
                .and_then(Value::as_array)
                .map(|options| {
                    options
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        },
        "input" => PiUi::Input {
            title,
            placeholder: params
                .get("placeholder")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "editor" => PiUi::Editor {
            title,
            prefill: params
                .get("prefill")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "notify" => PiUi::Notify {
            message: params
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => PiUi::Ignored,
    }
}

/// One pi child driven synchronously — the spike's harness. The real adapter
/// (E4) drives the same transport from the ACP connection actor.
pub struct PiProcess {
    child: ChildLines,
}

impl PiProcess {
    pub fn spawn(spec: &terminal::pty::SpawnSpec) -> std::io::Result<PiProcess> {
        Ok(PiProcess { child: spawn_lines(spec, StderrPolicy::Log)? })
    }

    pub fn pid(&self) -> u32 {
        self.child.pid
    }

    pub fn send(&self, value: &Value) -> std::io::Result<()> {
        self.child.writer.write_line(&value.to_string())
    }

    /// Read frames until `stop` says so, the child's EOF or `deadline`.
    /// `on_frame` may return lines to write back (an `extension_ui_response`).
    pub fn pump<F, S>(&self, deadline: Instant, mut on_frame: F, mut stop: S) -> bool
    where
        F: FnMut(&PiOut, &str) -> Vec<Value>,
        S: FnMut(&PiOut) -> bool,
    {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let Ok(line) = self.child.lines.recv_timeout(deadline - now) else {
                return false;
            };
            let frame = parse_line(&line);
            for reply in on_frame(&frame, &line) {
                if let Err(err) = self.send(&reply) {
                    log::warn!("engine: pi stdin write failed: {err}");
                    return false;
                }
            }
            if stop(&frame) {
                return true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_argv_starts_with_the_rpc_mode_and_carries_one_extension() {
        let extensions = vec![std::path::PathBuf::from("./.exp-pi-mcp.ts")];
        let session = std::path::PathBuf::from("/sessions/abc.jsonl");
        let args = PiArgs {
            model: Some("anthropic/claude-opus-4-6"),
            thinking: Some("high"),
            session_file: Some(&session),
            extensions: &extensions,
        };
        assert_eq!(
            pi_argv(&args),
            vec![
                "--mode",
                "rpc",
                "--model",
                "anthropic/claude-opus-4-6",
                "--thinking",
                "high",
                "--session",
                "/sessions/abc.jsonl",
                "-e",
                "./.exp-pi-mcp.ts",
            ]
        );
    }

    #[test]
    fn a_line_separator_inside_a_json_string_stays_one_line() {
        // U+2028 is legal inside a JSON string and pi's own jsonl reader is
        // LF-only for exactly this reason. A splitter that also broke on
        // U+2028/U+2029 would cut this frame in half and lose the turn.
        let line = "{\"type\":\"message_update\",\"assistantMessageEvent\":{\"type\":\"text_delta\",\
                    \"delta\":\"first\u{2028}second\"}}";
        assert!(!line.contains('\n'));
        match parse_line(line) {
            PiOut::Event { kind, event } => {
                assert_eq!(kind, "message_update");
                assert_eq!(
                    event["assistantMessageEvent"]["delta"],
                    json!("first\u{2028}second")
                );
            }
            other => panic!("expected an event, got {other:?}"),
        }
    }

    #[test]
    fn parse_line_discriminates_the_four_shapes() {
        match parse_line(r#"{"id":"1","type":"response","command":"get_state","success":true,"data":{"sessionId":"s"}}"#) {
            PiOut::Response { id, command, success, data } => {
                assert_eq!(id.as_deref(), Some("1"));
                assert_eq!(command, "get_state");
                assert!(success);
                assert_eq!(data["sessionId"], json!("s"));
            }
            other => panic!("expected a response, got {other:?}"),
        }
        match parse_line(r#"{"type":"extension_ui_request","id":"u1","method":"confirm","title":"Run?"}"#) {
            PiOut::ExtensionUiRequest { id, method, params } => {
                assert_eq!(id, "u1");
                assert_eq!(method, "confirm");
                assert_eq!(params["title"], json!("Run?"));
            }
            other => panic!("expected a ui request, got {other:?}"),
        }
        match parse_line(r#"{"type":"extension_error","error":"boom"}"#) {
            PiOut::ExtensionError { message } => assert_eq!(message, "boom"),
            other => panic!("expected an extension error, got {other:?}"),
        }
        match parse_line(r#"{"type":"agent_settled"}"#) {
            PiOut::Event { kind, .. } => assert_eq!(kind, "agent_settled"),
            other => panic!("expected an event, got {other:?}"),
        }
        assert!(matches!(parse_line("not json"), PiOut::Unknown));
        assert!(matches!(parse_line(r#"{"no":"type"}"#), PiOut::Unknown));
    }

    #[test]
    fn the_command_and_response_envelopes_carry_their_payload_inline() {
        assert_eq!(
            command("7", "prompt", json!({ "message": "hi" })),
            json!({ "id": "7", "type": "prompt", "message": "hi" })
        );
        assert_eq!(
            extension_ui_response("u1", json!({ "confirmed": true })),
            json!({ "type": "extension_ui_response", "id": "u1", "confirmed": true })
        );
    }
    #[test]
    fn parse_state_reads_the_model_the_levels_and_the_session_file() {
        // The real `get_state` answer from the phase-1 spike, trimmed to the
        // fields the adapter reads.
        let data = json!({
            "model": {
                "id": "gpt-5.4",
                "name": "GPT-5.4",
                "provider": "openai-codex",
                "contextWindow": 272000
            },
            "thinkingLevel": "medium",
            "isStreaming": false,
            "steeringMode": "one-at-a-time",
            "followUpMode": "one-at-a-time",
            "sessionFile": "/sessions/2026-09-06_abc.jsonl",
            "sessionId": "01a07612",
            "autoCompactionEnabled": true
        });
        let state = parse_state(&data);
        assert_eq!(state.thinking_level, "medium");
        assert_eq!(state.steering_mode, "one-at-a-time");
        assert_eq!(state.follow_up_mode, "one-at-a-time");
        assert!(state.auto_compaction);
        assert_eq!(state.session_file.as_deref(), Some("/sessions/2026-09-06_abc.jsonl"));
        assert_eq!(state.session_id, "01a07612");
        let model = state.model.expect("the state names a model");
        assert_eq!(model.value_id(), "openai-codex/gpt-5.4");
        assert_eq!(model.context_window, 272000);
        // A state with no model at all still parses (pi answers `model?`).
        assert_eq!(parse_state(&json!({})).model, None);
    }

    #[test]
    fn a_model_value_id_round_trips_provider_and_id() {
        assert_eq!(
            PiModel::split_value_id("openai-codex/gpt-5.4"),
            ("openai-codex".to_string(), "gpt-5.4".to_string())
        );
        // A bare id (a hand-typed settings default) keeps its provider empty
        // rather than swallowing the model name.
        assert_eq!(
            PiModel::split_value_id("gpt-5.4"),
            (String::new(), "gpt-5.4".to_string())
        );
    }

    #[test]
    fn classify_event_names_the_deltas_the_tools_and_the_idle_edge() {
        let text = json!({
            "type": "message_update",
            "message": { "responseId": "resp-1", "timestamp": 1788687314849i64 },
            "assistantMessageEvent": { "type": "text_delta", "delta": "hello" }
        });
        assert_eq!(
            classify_event("message_update", &text),
            PiEvent::TextDelta { message_id: Some("resp-1".to_string()), text: "hello".to_string() }
        );
        // No responseId: the timestamp keys the coalescer instead.
        let thinking = json!({
            "type": "message_update",
            "message": { "timestamp": 7 },
            "assistantMessageEvent": { "type": "thinking_delta", "delta": "hmm" }
        });
        assert_eq!(
            classify_event("message_update", &thinking),
            PiEvent::ThinkingDelta { message_id: Some("7".to_string()), text: "hmm".to_string() }
        );
        let start = json!({
            "type": "tool_execution_start",
            "toolCallId": "call-1",
            "toolName": "edit",
            "args": { "path": "src/lib.rs" }
        });
        assert_eq!(
            classify_event("tool_execution_start", &start),
            PiEvent::ToolStart {
                id: "call-1".to_string(),
                name: "edit".to_string(),
                args: json!({ "path": "src/lib.rs" }),
            }
        );
        assert_eq!(
            classify_event(
                "tool_execution_update",
                &json!({ "toolCallId": "call-1", "partialResult": "line 1\n" })
            ),
            PiEvent::ToolUpdate { id: "call-1".to_string(), output: Some("line 1\n".to_string()) }
        );
        // `agent_end` is NOT the idle edge (a retry can follow it) —
        // `agent_settled` is.
        assert_eq!(
            classify_event("agent_end", &json!({ "willRetry": false })),
            PiEvent::Ignored
        );
        assert_eq!(classify_event("agent_settled", &json!({})), PiEvent::Settled);
        // An event type this build has never seen must be inert.
        assert_eq!(classify_event("some_future_event", &json!({})), PiEvent::Ignored);
    }

    #[test]
    fn a_turn_that_errored_carries_its_message_and_its_usage() {
        let turn_end = json!({
            "type": "turn_end",
            "message": {
                "role": "assistant",
                "stopReason": "error",
                "errorMessage": "Codex error: model not supported",
                "usage": {
                    "totalTokens": 1234,
                    "cost": { "total": 0.0125 }
                }
            }
        });
        match classify_event("turn_end", &turn_end) {
            PiEvent::TurnEnd { usage, error } => {
                let usage = usage.expect("turn_end carries usage");
                assert_eq!(usage.total_tokens, 1234);
                assert_eq!(usage.cost_usd, Some(0.0125));
                assert_eq!(error.as_deref(), Some("Codex error: model not supported"));
            }
            other => panic!("expected a turn end, got {other:?}"),
        }
        // A clean turn has no error text at all.
        let clean = json!({ "message": { "stopReason": "stop", "usage": { "totalTokens": 10 } } });
        match classify_event("turn_end", &clean) {
            PiEvent::TurnEnd { error, .. } => assert_eq!(error, None),
            other => panic!("expected a turn end, got {other:?}"),
        }
    }

    #[test]
    fn classify_ui_answers_only_the_four_interactive_methods() {
        assert_eq!(
            classify_ui("confirm", &json!({ "title": "Run?", "message": "rm -rf" })),
            PiUi::Confirm { title: "Run?".to_string(), message: "rm -rf".to_string() }
        );
        assert_eq!(
            classify_ui("select", &json!({ "title": "Pick", "options": ["a", "b"] })),
            PiUi::Select {
                title: "Pick".to_string(),
                options: vec!["a".to_string(), "b".to_string()],
            }
        );
        assert!(matches!(classify_ui("input", &json!({ "title": "Name" })), PiUi::Input { .. }));
        assert!(matches!(classify_ui("editor", &json!({ "title": "Edit" })), PiUi::Editor { .. }));
        assert_eq!(
            classify_ui("notify", &json!({ "message": "done" })),
            PiUi::Notify { message: "done".to_string() }
        );
        // Fire-and-forget chrome: answering it would be a protocol error.
        assert_eq!(classify_ui("setWidget", &json!({ "widgetKey": "w" })), PiUi::Ignored);
        assert_eq!(classify_ui("setTitle", &json!({ "title": "t" })), PiUi::Ignored);
    }

    #[test]
    fn the_command_catalog_covers_pis_verb_only_commands() {
        let names: Vec<&str> = SYNTHESIZED_COMMANDS.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(
            names,
            vec!["compact", "new", "model", "thinking", "name", "fork", "clone", "export"]
        );
        assert_eq!(parse_commands(&json!({})), vec![]);
        assert_eq!(
            parse_commands(&json!({ "commands": [
                { "name": "review", "description": "Review the diff", "source": "prompt" },
                { "description": "no name at all" }
            ] })),
            vec![PiSlashCommand {
                name: "review".to_string(),
                description: "Review the diff".to_string(),
            }]
        );
    }
}
