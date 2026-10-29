//! What a launch is made of and how it can fail (EXP-1426 part 4c, out of
//! `engine::session` / `engine::host` / `engine::aside`, which re-export
//! every item here): the run's picks ([`RunOptions`]), the resume handle, the
//! start error, the `_meta` keys the host and the adapters exchange over ACP,
//! and the aside's tool posture.

/// The run's own picks an adapter configures its CLI with. Exponential
/// fills it from `coding::LaunchOptions` (`engine::adapters::AdapterSpec`);
/// a server-free host from its own settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Model choice within the agent's closed set; blank = the agent's own
    /// default.
    pub model: String,
    /// Effort/reasoning level; blank = omit.
    pub effort: String,
    /// Dynamic workflows (`--effort ultracode`). Claude-only; wins over
    /// `effort`.
    pub ultracode: bool,
    /// Start in plan mode (the host's flag; the CLI never sees it).
    pub plan_mode: bool,
    /// The LOGIN the run spends, as the live usage registry keys it
    /// ([`crate::usage::live`]); empty = the unnamed default login.
    pub account_profile: String,
}

/// The `_meta` key an adapter stamps its own session identity under, on the
/// `session/new` (or `session/load`) RESPONSE: claude's stream-json
/// `session_id`, codex's `thread.id`. It lands on the
/// run record as `agent_native_session_id` (D8) and is what a PTY resume of
/// the same conversation would need.
pub const NATIVE_SESSION_META_KEY: &str = "exponentialNativeSessionId";

/// EXP-784: the `_meta` key on a `session/cancel` saying whether the agent
/// should ALSO drop the user messages it holds queued behind the running
/// turn. Absent = `true` (a Stop is a stop); the stall watchdog's interrupt
/// sends `false` so the steers that piled up behind a wedged turn flow the
/// moment it unwinds. Claude maps it onto `interrupt.cancel_queued`; the
/// other adapters have no such knob and ignore it.
pub const CANCEL_QUEUED_META_KEY: &str = "exponentialCancelQueued";

/// EXP-1280: the `_meta` key on a `session/prompt` asking the agent to take
/// it in a FRESH context (a plan approved "in a fresh context"): claude
/// injects `/clear` first, between turns only; an adapter with no such reset
/// (codex) takes the prompt in the running conversation (the prompt carries
/// the whole plan either way).
pub const FRESH_CONTEXT_META_KEY: &str = "exponentialFreshContext";

/// What an aside may not touch: every writing tool, the shell, and
/// subagents (which would carry those tools back in).
pub const ASIDE_DISALLOWED_TOOLS: &[&str] =
    &["Edit", "MultiEdit", "Write", "NotebookEdit", "Bash", "Task", "Agent"];

/// The aside's WHOLE claude `--mcp-config` (strict): no servers. The
/// read-only tool list above would be no posture with `exponential` still
/// wired — `issues_update`, `comments_create`, `pr_merge` run under the
/// person's key — nor with code mode's `exec` or the team's servers, so a
/// side question gets none of them (the launcher skips resolving them,
/// `coding::launcher`; this document keeps the user's own config out too).
pub const ASIDE_MCP_CONFIG: &str = r#"{"mcpServers":{}}"#;

/// How a resumed (or replayed) run is re-entered: the recorded ACP id, or the
/// agent's own native id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeHandle {
    /// The recorded ACP `sessionId` — `session/load` or `session/resume`.
    Acp(String),
    /// claude's transcript uuid (`--resume=<id>`), codex's thread id.
    Native(String),
}

/// Why a start could not happen. Everything AFTER a successful the engine's start is
/// reported through the engine's exit instead.
#[derive(Debug)]
pub enum EngineError {
    /// The agent child never started (missing binary, bad cwd, no PATH).
    Spawn(std::io::Error),
    /// The child started but never completed `initialize` / `session/new`.
    Handshake(String),
    /// A backend call the start depends on failed (Exponential: a tRPC
    /// call; `api` converts its `ApiError` into this).
    Api(Box<dyn std::error::Error + Send + Sync>),
    /// The agent has no ACP adapter on this build. EXP-773: there is no
    /// terminal transport to fall back to, so the caller REFUSES the start
    /// and shows this.
    Unsupported(&'static str),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Spawn(err) => write!(f, "could not start the agent: {err}"),
            EngineError::Handshake(message) => write!(f, "the agent did not connect: {message}"),
            EngineError::Api(err) => write!(f, "{err}"),
            EngineError::Unsupported(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EngineError::Spawn(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(err: std::io::Error) -> Self {
        EngineError::Spawn(err)
    }
}
