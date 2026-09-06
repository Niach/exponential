//! EXP-746 — starting a run and the handle both hosts hold onto.
//!
//! [`start`] is the whole entry point after `coding::prepare_with_hooks`
//! returned `Prepared::Ready` with `transport == LaunchTransport::Acp`. It is
//! NON-BLOCKING: it spawns the `acp-engine-<sid8>` thread and returns, so a
//! handshake failure never surfaces here — it arrives as an
//! [`EngineExit`](crate::EngineExit) with `error: Some(..)` through
//! [`EngineHost::on_exit`](crate::EngineHost::on_exit).
//!
//! The exit contract deliberately mirrors `cli::session_host::RunningSession`
//! (`is_done`/`wait`/`wait_timeout`/`kill`) so the daemon's `LiveSession`
//! bookkeeping, reap block and quit sweep compile against either backend.
//!
//! Signatures land in P0; lane E1 fills the bodies.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::host::{EngineCommand, EngineExit, EngineHost, KillFeed, LocalSink, SessionCtx};
use crate::local::LocalFeedEvent;

/// Everything a run needs once `prepare` said Ready on the Acp arm.
pub struct EngineStart {
    /// `transport == Acp` and `acp.is_some()`. Its `launch_hold` must ALREADY
    /// be taken by the host (EXP-478: the gate has to outlive registration in
    /// `LocalSessions`/`LiveSession`, and a destructure in here would drop it
    /// pre-spawn) — [`start`] debug-asserts it.
    pub prepared: coding::PreparedLaunch,
    pub trpc: Arc<api::TrpcClient>,
    pub runtime: Arc<steer::SteerRuntime>,
    pub data_dir: PathBuf,
    pub account_id: String,
    /// The signed-in user's id — the kill-watch owner pin (EXP-105).
    pub own_user_id: Option<String>,
    /// The `expu_` personal key: the redactor's exact-match secret (REV2-17)
    /// AND the codex/pi MCP bearer that rides the spawn env.
    pub personal_key: Option<String>,
    /// The relay room's subject; `None` for batch and action rooms.
    pub issue_id: Option<String>,
    /// EXP-444: a foreign requester on a shared host — suppresses login
    /// affordances.
    pub foreign_host: bool,
    /// `false` = no relay room at all (unit tests, `--no-publish`).
    pub publish: bool,
    /// Registered by the host BEFORE this call, so no edge can be missed
    /// between row creation and the first frame.
    pub kill: KillFeed,
    /// Desktop / `exponential code` attach; `None` on the daemon.
    pub local_sink: Option<LocalSink>,
}

/// Open an ENDED run's transcript read-only (a Past row on the desktop).
/// Creates no `coding_sessions` row, no heartbeat, no publisher and no kill
/// watch: it replays history through the same mapper and stops. Resuming is a
/// different action entirely (`steer.startSession { resumeSessionId }`).
pub struct OpenTranscript {
    pub runtime: Arc<steer::SteerRuntime>,
    pub data_dir: PathBuf,
    pub personal_key: Option<String>,
    pub handle: HistoryHandle,
    pub local_sink: LocalSink,
}

/// Everything needed to find an ended run's history, straight off its
/// `runs.json` [`coding::run_registry::RunRecord`].
pub struct HistoryHandle {
    pub agent: coding::AgentKind,
    pub cwd: PathBuf,
    /// The recorded ACP session id (`session/load`), when the run took the
    /// ACP path at all.
    pub acp_session_id: Option<String>,
    /// The agent-native handle — claude's transcript uuid, codex's thread id,
    /// pi's session FILE (pi is path-keyed, not id-keyed).
    pub native: ResumeHandle,
}

/// A cheap-clone handle on a live (or replaying) run. `Send + Sync`.
#[derive(Clone)]
pub struct EngineSession(Arc<Inner>);

/// The mutable half a session accumulates. Constructed only by [`start`] and
/// [`EngineSession::open_transcript`].
// EXP-746: the skeleton declares the shape before E1 constructs it — drop
// this allow with the bodies.
#[allow(dead_code)]
pub(crate) struct Inner {
    pub(crate) ctx: Arc<SessionCtx>,
    /// The builtin agent, or the user's external ACP binary (D13).
    pub(crate) agent: coding::AgentKind,
    pub(crate) commands: flume::Sender<EngineCommand>,
    // EXP-746 E1: fill — the local feed backlog + its subscribers, the ids
    // learned at `session/new`, the pending-ask table, the exit slot.
}

impl EngineSession {
    /// Read-only transcript replay. Ends by itself once the load's updates
    /// stop arriving; there is nothing to kill and nothing to end.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn open_transcript(open: OpenTranscript) -> Result<EngineSession, EngineError> {
        todo!("EXP-746 E1: session/load replay with no row, no publisher, no heartbeat")
    }

    pub fn session_id(&self) -> &str {
        &self.0.ctx.session_id
    }

    /// The builtin agent, or the external spec (D13).
    pub fn agent(&self) -> &coding::AgentKind {
        &self.0.agent
    }

    /// Display name for the session header and tab strip.
    pub fn agent_label(&self) -> &str {
        self.0.agent.label()
    }

    pub fn worktree(&self) -> &Path {
        &self.0.ctx.prepared.worktree
    }

    pub fn branch(&self) -> &str {
        &self.0.ctx.prepared.branch
    }

    /// The git ref "Latest changes" is measured from (EXP-688).
    pub fn base_ref(&self) -> Option<&str> {
        self.0.ctx.prepared.base_ref.as_deref()
    }

    /// The ACP `SessionId` from `session/new` — upserted onto the RunRecord
    /// (D8) as soon as the handshake completes, hence `Option` here.
    // EXP-746 E1: fill
    pub fn acp_session_id(&self) -> Option<String> {
        todo!("EXP-746 E1: read the id learned at session/new")
    }

    /// What the adapter reports underneath: claude's stream-json `session_id`,
    /// codex's `thread.id`, pi's session file path (D8).
    // EXP-746 E1: fill
    pub fn agent_native_session_id(&self) -> Option<String> {
        todo!("EXP-746 E1: read the agent-native id learned at handshake")
    }

    /// A fresh receiver that replays the buffered backlog FIRST, so a view
    /// attaching late (a tab reopened, the screen rebuilt) sees the whole
    /// session rather than the tail.
    // EXP-746 E1: fill
    pub fn subscribe(&self) -> flume::Receiver<LocalFeedEvent> {
        todo!("EXP-746 E1: backlog replay + a live subscriber")
    }

    /// A new user message. Between turns this starts one; the engine never
    /// blocks the caller.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn send_prompt(&self, text: String) {
        todo!("EXP-746 E1: EngineCommand::Prompt")
    }

    /// Mid-turn steering — the same entry point a relay `input` frame takes.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn steer(&self, text: String) {
        todo!("EXP-746 E1: EngineCommand::Steer")
    }

    /// Answer a pending question card. Resolves the parked ACP `Responder`,
    /// then acks (D3).
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn answer(&self, answer: steer::RemoteAnswer) {
        todo!("EXP-746 E1: EngineCommand::Answer")
    }

    /// A `/` command the AGENT advertised (contract commands never reach the
    /// engine — `steer`'s `CommandLink` handles those).
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn run_command(&self, name: &str, args: &str) {
        todo!("EXP-746 E1: EngineCommand::Command")
    }

    /// Change one live option. Fire-and-forget: the re-emitted `config_state`
    /// IS the confirmation (D4).
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn set_config(&self, id: &str, value: ConfigValue) {
        todo!("EXP-746 E1: EngineCommand::SetConfig")
    }

    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn set_mode(&self, id: &str) {
        todo!("EXP-746 E1: EngineCommand::SetMode")
    }

    /// Interrupt the running turn without ending the session.
    // EXP-746 E1: fill
    pub fn cancel_turn(&self) {
        todo!("EXP-746 E1: EngineCommand::Cancel")
    }

    pub fn turn_signal(&self) -> Arc<steer::TurnSignal> {
        self.0.ctx.turn_signal.clone()
    }

    /// Stop now and end the row with `outcome` as the publisher `bye`.
    /// Idempotent.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn kill(&self, outcome: &'static str) {
        todo!("EXP-746 E1: EngineCommand::Shutdown + the D14 end sequence")
    }

    // EXP-746 E1: fill
    pub fn is_done(&self) -> bool {
        todo!("EXP-746 E1: read the exit slot")
    }

    // EXP-746 E1: fill
    pub fn wait(&self) -> EngineExit {
        todo!("EXP-746 E1: block on the exit slot")
    }

    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn wait_timeout(&self, timeout: Duration) -> Option<EngineExit> {
        todo!("EXP-746 E1: block on the exit slot with a deadline")
    }
}

/// Start a run. Non-blocking; handshake failures arrive through
/// [`EngineHost::on_exit`].
// EXP-746 E1: fill
#[allow(unused_variables)]
pub fn start(start: EngineStart, host: Arc<dyn EngineHost>) -> Result<EngineSession, EngineError> {
    debug_assert!(
        start.prepared.launch_hold.is_none(),
        "EXP-746 (EXP-478): the host takes `prepared.launch_hold` BEFORE engine::start \
         and drops it only after it registered the session"
    );
    todo!("EXP-746 E1: build the ctx, spawn acp-engine-<sid8>, attach the lifecycle")
}

/// The value half of `session/set_config_option`. Mirrors ACP's
/// `SessionConfigOptionValue` without leaking it into the hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigValue {
    ValueId(String),
    Bool(bool),
}

impl ConfigValue {
    /// The relay's `set_config` frame carries a bare string, and a BLANK one
    /// is the deliberate "CLI default / unset" choice (which is why the
    /// relay's zod has no `min(1)` on it). Booleans ride the same field as
    /// `"true"`/`"false"` because the wire has no second shape.
    pub fn from_wire(value: &str) -> ConfigValue {
        match value {
            "true" => ConfigValue::Bool(true),
            "false" => ConfigValue::Bool(false),
            other => ConfigValue::ValueId(other.to_string()),
        }
    }

    /// What the same value looks like going back out as `config_state`.
    pub fn to_wire(&self) -> String {
        match self {
            ConfigValue::ValueId(value) => value.clone(),
            ConfigValue::Bool(value) => value.to_string(),
        }
    }
}

impl From<ConfigValue> for agent_client_protocol::schema::v1::SessionConfigOptionValue {
    fn from(value: ConfigValue) -> Self {
        match value {
            ConfigValue::ValueId(id) => {
                agent_client_protocol::schema::v1::SessionConfigOptionValue::value_id(id)
            }
            ConfigValue::Bool(value) => {
                agent_client_protocol::schema::v1::SessionConfigOptionValue::boolean(value)
            }
        }
    }
}

/// How a resumed (or replayed) run is re-entered. Three shapes because the
/// three agents have three identity models: ACP ids, agent-native ids, and
/// pi's session FILE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeHandle {
    /// The recorded ACP `sessionId` — `session/load` or `session/resume`.
    Acp(String),
    /// claude's transcript uuid (`--resume=<id>`), codex's thread id.
    Native(String),
    /// pi is FILE-PATH keyed (`--session <file>` / `switch_session`).
    PiSessionFile(PathBuf),
}

impl From<coding::ResumeSeed> for ResumeHandle {
    fn from(seed: coding::ResumeSeed) -> Self {
        match seed {
            coding::ResumeSeed::Acp(id) => ResumeHandle::Acp(id),
            coding::ResumeSeed::Native(id) => ResumeHandle::Native(id),
            coding::ResumeSeed::PiSessionFile(path) => ResumeHandle::PiSessionFile(path),
        }
    }
}

/// Why a start could not happen. Everything AFTER a successful [`start`] is
/// reported through [`EngineExit`] instead.
#[derive(Debug)]
pub enum EngineError {
    /// The agent child never started (missing binary, bad cwd, no PATH).
    Spawn(std::io::Error),
    /// The child started but never completed `initialize` / `session/new`.
    Handshake(String),
    /// A tRPC call the start depends on failed.
    Api(api::error::ApiError),
    /// The agent has no ACP adapter on this build — the caller falls back to
    /// the terminal transport.
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

impl From<api::error::ApiError> for EngineError {
    fn from(err: api::error::ApiError) -> Self {
        EngineError::Api(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resume_seed_becomes_a_resume_handle() {
        assert_eq!(
            ResumeHandle::from(coding::ResumeSeed::Acp("sess-1".into())),
            ResumeHandle::Acp("sess-1".into())
        );
        assert_eq!(
            ResumeHandle::from(coding::ResumeSeed::Native("uuid-1".into())),
            ResumeHandle::Native("uuid-1".into())
        );
        assert_eq!(
            ResumeHandle::from(coding::ResumeSeed::PiSessionFile(PathBuf::from("/tmp/s.json"))),
            ResumeHandle::PiSessionFile(PathBuf::from("/tmp/s.json"))
        );
    }

    #[test]
    fn a_blank_config_value_stays_the_cli_default() {
        assert_eq!(ConfigValue::from_wire(""), ConfigValue::ValueId(String::new()));
        assert_eq!(ConfigValue::from_wire("true"), ConfigValue::Bool(true));
        assert_eq!(ConfigValue::from_wire("opus"), ConfigValue::ValueId("opus".into()));
        assert_eq!(ConfigValue::Bool(false).to_wire(), "false");
        assert_eq!(ConfigValue::ValueId("opus".into()).to_wire(), "opus");
    }

    #[test]
    fn config_values_cross_into_the_acp_shape() {
        use agent_client_protocol::schema::v1::SessionConfigOptionValue;
        let value: SessionConfigOptionValue = ConfigValue::Bool(true).into();
        assert_eq!(value.as_bool(), Some(true));
        let value: SessionConfigOptionValue = ConfigValue::ValueId("high".into()).into();
        assert_eq!(value.as_value_id().map(|id| id.0.to_string()), Some("high".into()));
    }
}
