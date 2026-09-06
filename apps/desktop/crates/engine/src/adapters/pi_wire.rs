//! EXP-746 — pi's private wire: `--mode rpc` line JSON. Owned by lane E4.
//!
//! Four shapes share one stdout stream, discriminated ONLY by `type`, and an
//! unrecognized `type` must be ignorable rather than fatal. Strict LF framing
//! both ways (pi's own jsonl reader is deliberately LF-only because
//! U+2028/U+2029 are legal inside JSON strings).

use serde_json::Value;

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

/// The full argv after the program name (always starts `--mode rpc`).
// EXP-746 E4: fill
#[allow(unused_variables)]
pub fn pi_argv(args: &PiArgs<'_>) -> Vec<String> {
    todo!("EXP-746 E4: --mode rpc [--model] [--thinking] [--session] [-e ...]")
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
    ExtensionError { message: String },
    /// A raw `AgentSessionEvent` (message/tool/turn/compaction/…).
    Event { kind: String, event: Value },
    /// An unknown `type` — logged and dropped, never fatal.
    Unknown,
}

/// Classify one line off pi's stdout.
// EXP-746 E4: fill
#[allow(unused_variables)]
pub fn parse_line(line: &str) -> PiOut {
    todo!("EXP-746 E4: discriminate on `type`, defaulting to PiOut::Unknown")
}
