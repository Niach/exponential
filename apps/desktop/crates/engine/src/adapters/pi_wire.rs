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
}
