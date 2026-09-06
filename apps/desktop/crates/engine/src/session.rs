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

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use agent_client_protocol::{Client, ConnectTo};

use crate::adapters::{Adapter, AdapterKind, AdapterSpec};
use crate::host::{
    thread_name, ChildExitLink, EngineCommand, EngineExit, EngineHost, ExitState, KillFeed,
    LocalFeed, LocalSink, PendingAsks, RunFacts, SessionCtx, SessionIds,
};
use crate::lifecycle::RunLifecycle;
use crate::local::LocalFeedEvent;
use crate::mapper::{Mapper, MapperConfig};
use crate::sink::EventSink;

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

/// The pieces [`start`] normally builds itself. Split out so a test (and an
/// adapter lane's own harness) can drive the REAL engine with a scripted
/// `ConnectTo<Client>` instead of a child process.
pub struct EngineParts<A> {
    pub adapter: A,
    /// Where the adapter records its child's exit — the `exit:<code>` the
    /// publisher says `bye` with.
    pub child_exit: ChildExitLink,
    /// Publish through this instead of a relay room. `None` = the room (or
    /// nothing at all when `publish` is false).
    pub sink: Option<Arc<dyn EventSink>>,
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
pub(crate) struct Inner {
    pub(crate) ctx: Arc<SessionCtx>,
    /// The builtin agent, or the user's external ACP binary (D13).
    pub(crate) agent: coding::AgentKind,
    pub(crate) commands: flume::Sender<EngineCommand>,
}

impl EngineSession {
    /// Read-only transcript replay. Ends by itself once the load's updates
    /// stop arriving; there is nothing to kill and nothing to end.
    ///
    /// The `local_sink` is honoured, but the desktop reads the replay off
    /// [`EngineSession::subscribe`] instead: the backlog carries every event
    /// the load produced, so a view that attaches after the load finished
    /// still renders the whole transcript.
    pub fn open_transcript(open: OpenTranscript) -> Result<EngineSession, EngineError> {
        let OpenTranscript {
            runtime,
            data_dir,
            personal_key,
            handle,
            local_sink,
        } = open;
        let HistoryHandle {
            agent,
            cwd,
            acp_session_id,
            native,
        } = handle;
        let kind = AdapterKind::from_agent(&agent);
        let builtin = agent.builtin();
        let child_exit = ChildExitLink::new();
        // The recorded ACP id is what `session/load` takes (for pi it is the
        // session file, which doubles as its ACP id); a run recorded without
        // one falls back to the agent-native handle the adapter knows how to
        // reopen.
        let resume = match acp_session_id {
            Some(id) => ResumeHandle::Acp(id),
            None => native,
        };
        let adapter = Adapter::new(AdapterSpec {
            kind,
            agent: agent.clone(),
            spawn: terminal::pty::SpawnSpec {
                program: builtin
                    .map(|agent| agent.default_binary().to_string())
                    .unwrap_or_else(|| agent.id().to_string()),
                args: Vec::new(),
                cwd: Some(cwd.clone()),
                env: Vec::new(),
            },
            options: coding::LaunchOptions {
                agent: builtin.unwrap_or_default(),
                model: String::new(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                external: match &agent {
                    coding::AgentKind::External(spec) => Some(spec.clone()),
                    coding::AgentKind::Builtin(_) => None,
                },
            },
            // A replay never talks to MCP: it reads history and stops.
            mcp: coding::AgentMcp::ClaudeFile,
            cwd: cwd.clone(),
            session_id: String::new(),
            prompt: None,
            resume: Some(resume.clone()),
            personal_key: personal_key.clone(),
            reaper_settings_path: None,
            exit: child_exit.clone(),
        })?;

        // A replay's session id is local bookkeeping only — no row exists.
        let session_id = format!("replay-{}", uuid::Uuid::new_v4());
        let ctx = build_ctx(CtxSpec {
            session_id: session_id.clone(),
            run: RunFacts {
                worktree: cwd.clone(),
                ..RunFacts::default()
            },
            trpc: Arc::new(api::TrpcClient::new("http://127.0.0.1", Arc::new(|| None))),
            runtime,
            data_dir,
            account_id: String::new(),
            own_user_id: None,
            personal_key,
            issue_id: None,
            foreign_host: false,
            publish: false,
            local_sink: Some(local_sink),
            agent: agent.clone(),
            replay: true,
            // The HOST issues `session/load` for it: a replay that opened a
            // fresh `session/new` would show an empty transcript.
            resume: Some(resume),
            prompt: None,
            child_exit,
        });
        spawn_engine(ctx, agent, adapter, KillFeed::inert(), Arc::new(NoHost))
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
        &self.0.ctx.run.worktree
    }

    pub fn branch(&self) -> &str {
        &self.0.ctx.run.branch
    }

    /// The git ref "Latest changes" is measured from (EXP-688).
    pub fn base_ref(&self) -> Option<&str> {
        self.0.ctx.run.base_ref.as_deref()
    }

    /// The ACP `SessionId` from `session/new` — upserted onto the RunRecord
    /// (D8) as soon as the handshake completes, hence `Option` here.
    pub fn acp_session_id(&self) -> Option<String> {
        self.0.ctx.ids().acp
    }

    /// What the adapter reports underneath: claude's stream-json `session_id`,
    /// codex's `thread.id`, pi's session file path (D8).
    pub fn agent_native_session_id(&self) -> Option<String> {
        self.0.ctx.ids().native
    }

    /// A fresh receiver that replays the buffered backlog FIRST, so a view
    /// attaching late (a tab reopened, the screen rebuilt) sees the whole
    /// session rather than the tail.
    pub fn subscribe(&self) -> flume::Receiver<LocalFeedEvent> {
        self.0.ctx.feed.subscribe()
    }

    /// A new user message. Between turns this starts one; the engine never
    /// blocks the caller.
    pub fn send_prompt(&self, text: String) {
        self.send(EngineCommand::Prompt(crate::host::text_blocks(&text)));
    }

    /// Mid-turn steering — the same entry point a relay `input` frame takes.
    pub fn steer(&self, text: String) {
        self.send(EngineCommand::Steer(text));
    }

    /// Answer a pending question card. Resolves the parked ACP `Responder`,
    /// then acks (D3).
    pub fn answer(&self, answer: steer::RemoteAnswer) {
        self.send(EngineCommand::Answer(answer));
    }

    /// A `/` command the AGENT advertised (contract commands never reach the
    /// engine — `steer`'s `CommandLink` handles those).
    pub fn run_command(&self, name: &str, args: &str) {
        self.send(EngineCommand::Command {
            name: name.to_string(),
            args: args.to_string(),
        });
    }

    /// Change one live option. Fire-and-forget: the re-emitted `config_state`
    /// IS the confirmation (D4).
    pub fn set_config(&self, id: &str, value: ConfigValue) {
        self.send(EngineCommand::SetConfig {
            id: agent_client_protocol::schema::v1::SessionConfigId::new(id.to_string()),
            value: value.into(),
        });
    }

    pub fn set_mode(&self, id: &str) {
        self.send(EngineCommand::SetMode(
            agent_client_protocol::schema::v1::SessionModeId::new(id.to_string()),
        ));
    }

    /// Replay this session's history through the mapper (`session/load`) —
    /// what a resumed tab does to repaint everything that came before.
    pub fn load_history(&self) {
        self.send(EngineCommand::LoadHistory);
    }

    /// Interrupt the running turn without ending the session.
    pub fn cancel_turn(&self) {
        self.send(EngineCommand::Cancel);
    }

    pub fn turn_signal(&self) -> Arc<steer::TurnSignal> {
        self.0.ctx.turn_signal.clone()
    }

    /// Stop now and end the row with `outcome` as the publisher `bye`.
    /// Idempotent.
    pub fn kill(&self, outcome: &'static str) {
        self.send(EngineCommand::Shutdown { outcome });
    }

    pub fn is_done(&self) -> bool {
        self.0.ctx.exit.is_done()
    }

    /// Blocks until the run ends. The FULL exit (with the
    /// `coding::end_session` result) went to
    /// [`EngineHost::on_exit`](crate::EngineHost::on_exit); this rebuilds the
    /// same exit with `end: None`, which is all a waiter needs — the
    /// `SessionEndObserver` already applied that result (EXP-641).
    pub fn wait(&self) -> EngineExit {
        self.0.ctx.exit.wait(None).unwrap_or_else(|| EngineExit {
            session_id: self.0.ctx.session_id.clone(),
            outcome: "ended".to_string(),
            child: None,
            error: None,
            end: None,
        })
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Option<EngineExit> {
        self.0.ctx.exit.wait(Some(timeout))
    }

    fn send(&self, command: EngineCommand) {
        // A dead loop (the session already ended) drops the command: every
        // one of them is fire-and-forget by contract.
        let _ = self.0.commands.send(command);
    }
}

/// Start a run. Non-blocking; handshake failures arrive through
/// [`EngineHost::on_exit`].
pub fn start(start: EngineStart, host: Arc<dyn EngineHost>) -> Result<EngineSession, EngineError> {
    debug_assert!(
        start.prepared.launch_hold.is_none(),
        "EXP-746 (EXP-478): the host takes `prepared.launch_hold` BEFORE engine::start \
         and drops it only after it registered the session"
    );
    let acp = start
        .prepared
        .acp
        .clone()
        .ok_or(EngineError::Unsupported("this launch has no ACP half"))?;
    let agent = agent_kind(&start.prepared);
    let child_exit = ChildExitLink::new();
    let adapter = Adapter::new(AdapterSpec {
        kind: AdapterKind::from_agent(&agent),
        agent: agent.clone(),
        spawn: start.prepared.spawn.clone(),
        options: acp.options.clone(),
        mcp: acp.mcp.clone(),
        cwd: start.prepared.worktree.clone(),
        session_id: acp.session_id.clone(),
        prompt: acp.prompt.clone(),
        resume: acp.resume.clone().map(ResumeHandle::from),
        personal_key: start.personal_key.clone(),
        reaper_settings_path: acp.reaper_settings_path.clone(),
    exit: child_exit.clone(),
})?;
    start_with(
        start,
        host,
        EngineParts {
            adapter,
            child_exit,
            sink: None,
        },
    )
}

/// [`start`] with the transport handed in. The ONE seam a test drives.
pub fn start_with<A>(
    start: EngineStart,
    host: Arc<dyn EngineHost>,
    parts: EngineParts<A>,
) -> Result<EngineSession, EngineError>
where
    A: ConnectTo<Client> + 'static,
{
    let EngineStart {
        prepared,
        trpc,
        runtime,
        data_dir,
        account_id,
        own_user_id,
        personal_key,
        issue_id,
        foreign_host,
        publish,
        kill,
        local_sink,
    } = start;
    let agent = agent_kind(&prepared);
    let acp = prepared.acp.clone();
    let ctx = build_ctx(CtxSpec {
        session_id: prepared.session_id.clone(),
        run: RunFacts {
            worktree: prepared.worktree.clone(),
            branch: prepared.branch.clone(),
            base_ref: prepared.base_ref.clone(),
            repository_id: prepared.repository_id.clone(),
            clone: prepared.clone.clone(),
            heartbeat_scope: Some(prepared.heartbeat_scope.clone()),
        },
        trpc,
        runtime,
        data_dir,
        account_id,
        own_user_id,
        personal_key,
        issue_id,
        foreign_host,
        publish,
        local_sink,
        agent: agent.clone(),
        replay: false,
        resume: acp.as_ref().and_then(|acp| acp.resume.clone()).map(ResumeHandle::from),
        prompt: acp.as_ref().and_then(|acp| acp.prompt.clone()),
        child_exit: parts.child_exit,
    });
    if let Some(sink) = parts.sink {
        let _ = ctx.sink.set(sink);
    }
    spawn_engine(ctx, agent, parts.adapter, kill, host)
}

/// The builtin agent the launch names, or the external spec its options carry
/// (D13 — `PreparedLaunch.agent` is always a builtin).
fn agent_kind(prepared: &coding::PreparedLaunch) -> coding::AgentKind {
    match prepared
        .acp
        .as_ref()
        .and_then(|acp| acp.options.external.clone())
    {
        Some(spec) => coding::AgentKind::External(spec),
        None => coding::AgentKind::Builtin(prepared.agent),
    }
}

struct CtxSpec {
    session_id: String,
    run: RunFacts,
    trpc: Arc<api::TrpcClient>,
    runtime: Arc<steer::SteerRuntime>,
    data_dir: PathBuf,
    account_id: String,
    own_user_id: Option<String>,
    personal_key: Option<String>,
    issue_id: Option<String>,
    foreign_host: bool,
    publish: bool,
    local_sink: Option<LocalSink>,
    agent: coding::AgentKind,
    replay: bool,
    resume: Option<ResumeHandle>,
    prompt: Option<String>,
    /// The link the ADAPTER records its child's exit into — the same one, so
    /// the end sequence reads what the adapter wrote.
    child_exit: ChildExitLink,
}

fn build_ctx(spec: CtxSpec) -> Arc<SessionCtx> {
    // REV2-17: the session's own launcher secrets (the EXP-73 credential
    // file, a token in a remote URL) plus the `expu_` key, masked out of
    // every wire string.
    let mut secrets = steer::activity::secrets_from_worktree(&spec.run.worktree);
    secrets.extend(spec.personal_key.clone());
    let mapper = Mapper::new(MapperConfig {
        redactor: steer::Redactor::new(secrets),
        cwd: spec.run.worktree.clone(),
        agent: AdapterKind::from_agent(&spec.agent).session_agent(),
        session_seed: spec.session_id.clone(),
    });
    Arc::new(SessionCtx {
        session_id: spec.session_id,
        run: spec.run,
        trpc: spec.trpc,
        runtime: spec.runtime,
        data_dir: spec.data_dir,
        account_id: spec.account_id,
        own_user_id: spec.own_user_id,
        personal_key: spec.personal_key,
        issue_id: spec.issue_id,
        foreign_host: spec.foreign_host,
        publish: spec.publish,
        local_sink: spec.local_sink,
        turn_signal: Arc::new(steer::TurnSignal::new()),
        agent: spec.agent,
        replay: spec.replay,
        resume: spec.resume,
        prompt: spec.prompt,
        mapper: Mutex::new(mapper),
        sink: OnceLock::new(),
        feed: LocalFeed::default(),
        asks: PendingAsks::default(),
        ids: Mutex::new(SessionIds::default()),
        needs_input: AtomicBool::new(false),
        exit: ExitState::default(),
        outcome: Mutex::new(None),
        child_exit: spec.child_exit,
    })
}

/// Attach the lifecycle, then run the connection on its own OS thread.
///
/// `Handle::block_on` gives the whole graph a tokio context (the mapper's
/// flush tick, the ACP timers) WITHOUT requiring the top-level future to be
/// `Send` and without a `LocalSet`, and it blocks a PLAIN thread — never one
/// of `SteerRuntime`'s two workers (D1).
fn spawn_engine<A>(
    ctx: Arc<SessionCtx>,
    agent: coding::AgentKind,
    adapter: A,
    kill: KillFeed,
    host: Arc<dyn EngineHost>,
) -> Result<EngineSession, EngineError>
where
    A: ConnectTo<Client> + 'static,
{
    let (commands, inbox) = flume::unbounded();
    let lifecycle = RunLifecycle::attach(Arc::clone(&ctx), kill, commands.clone())?;
    let thread_ctx = Arc::clone(&ctx);
    let spawned = std::thread::Builder::new()
        .name(thread_name(&ctx.session_id))
        .spawn(move || {
            let runtime = Arc::clone(&thread_ctx.runtime);
            let result = runtime
                .handle()
                .block_on(crate::host::run_session(
                    Arc::clone(&thread_ctx),
                    adapter,
                    inbox,
                ));
            let error = result.err().map(|err| err.to_string());
            let outcome = thread_ctx.end_outcome();
            let child = thread_ctx.child_exit.get();
            lifecycle.end(&thread_ctx, host.as_ref(), &outcome, child, error);
        });
    match spawned {
        Ok(_) => Ok(EngineSession(Arc::new(Inner {
            ctx,
            agent,
            commands,
        }))),
        Err(err) => Err(EngineError::Spawn(err)),
    }
}

/// `open_transcript` answers to nobody: a replay has no row, no publisher and
/// no host bookkeeping to unwind.
struct NoHost;

impl EngineHost for NoHost {
    fn on_exit(&self, _exit: EngineExit) {}
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
