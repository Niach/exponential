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
use crate::local::{EnginePhase, LocalFeedEvent};
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
    /// AND the codex/external MCP bearer that rides the spawn env.
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
    /// The agent-native handle — claude's transcript uuid, codex's thread id.
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
        // The recorded ACP id is what `session/load` takes; a run recorded without
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
                mcp_server_ids: Vec::new(),
                account: None,
                external: match &agent {
                    coding::AgentKind::External(spec) => Some(spec.clone()),
                    coding::AgentKind::Builtin(_) => None,
                },
            },
            // A replay never talks to MCP: it reads history and stops.
            mcp: coding::AgentMcp::ClaudeFile,
            servers: Vec::new(),
            cwd: cwd.clone(),
            session_id: String::new(),
            prompt: None,
            resume: Some(resume.clone()),
            replay: true,
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
            // A replay spawned nothing, so there is nothing to mask.
            mcp_secrets: Vec::new(),
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
    /// codex's `thread.id` (D8).
    pub fn agent_native_session_id(&self) -> Option<String> {
        self.0.ctx.ids().native
    }

    /// A fresh receiver that replays the buffered backlog FIRST, so a view
    /// attaching late (a tab reopened, the screen rebuilt) sees the whole
    /// session rather than the tail, and the latest-wins state (phase,
    /// config, usage, diff) after it.
    pub fn subscribe(&self) -> flume::Receiver<LocalFeedEvent> {
        self.0.ctx.feed.subscribe()
    }

    /// Where the run is right now — `None` until the first phase edge. A view
    /// attaching late seeds itself from this instead of painting one frame of
    /// "Connecting" over a session that has been live for an hour; the replay
    /// then carries the same answer (EXP-746 review UI-2).
    pub fn phase(&self) -> Option<EnginePhase> {
        self.0.ctx.feed.phase()
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

    /// EXP-750: stop one live `terminal/*` command without touching the run
    /// — what the Stop button on a running output card does. Unknown ids are
    /// a no-op: the card may be older than the terminal's release.
    pub fn kill_terminal(&self, terminal_id: &str) {
        self.0.ctx.terminals.kill(terminal_id);
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
    let acp = start.prepared.acp.clone();
    let agent = agent_kind(&start.prepared);
    let child_exit = ChildExitLink::new();
    let adapter = Adapter::new(AdapterSpec {
        kind: AdapterKind::from_agent(&agent),
        agent: agent.clone(),
        spawn: start.prepared.spawn.clone(),
        options: acp.options.clone(),
        mcp: acp.mcp.clone(),
        servers: acp.servers.clone(),
        cwd: start.prepared.worktree.clone(),
        session_id: acp.session_id.clone(),
        prompt: acp.prompt.clone(),
        resume: acp.resume.clone().map(ResumeHandle::from),
        replay: false,
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
        // EXP-792: the team servers' device-held values, masked like the key.
        mcp_secrets: acp.mcp_secrets.clone().into_vec(),
        replay: false,
        resume: acp.resume.clone().map(ResumeHandle::from),
        prompt: acp.prompt.clone(),
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
    match prepared.acp.options.external.clone() {
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
    /// EXP-792: every team-MCP secret the launcher put in the spawn env —
    /// exact-match entries for the redactor beside the `expu_` key.
    mcp_secrets: Vec<String>,
    replay: bool,
    resume: Option<ResumeHandle>,
    prompt: Option<String>,
    /// The link the ADAPTER records its child's exit into — the same one, so
    /// the end sequence reads what the adapter wrote.
    child_exit: ChildExitLink,
}

/// The redactor's exact-match set: the worktree's launcher secrets, the
/// `expu_` key and (EXP-792) every team-MCP value the launcher put in the
/// spawn env — an OAuth bearer, a typed header or env value. Same posture as
/// the key: a tool result that echoes one must never reach the relay.
fn session_secrets(
    worktree: &Path,
    personal_key: Option<String>,
    mcp_secrets: Vec<String>,
) -> Vec<String> {
    let mut secrets = steer::activity::secrets_from_worktree(worktree);
    secrets.extend(personal_key);
    secrets.extend(mcp_secrets);
    secrets
}

fn build_ctx(spec: CtxSpec) -> Arc<SessionCtx> {
    // REV2-17: the session's own launcher secrets (the EXP-73 credential
    // file, a token in a remote URL, the `.exp-mcp.json` key) plus the
    // `expu_` key, masked out of every wire string. Built ONCE and shared:
    // the mapper's strings and the lifecycle's `diff` ticker are two
    // publishers of the same run and must mask the same set, or the weaker
    // one becomes the leak.
    let redactor = Arc::new(steer::Redactor::new(session_secrets(
        &spec.run.worktree,
        spec.personal_key,
        spec.mcp_secrets,
    )));
    // EXP-766: a host with a local sink (the desktop) reattaches a view
    // mid-run and keeps the full row backlog; a headless host keeps only the
    // small attach-window ring (`BacklogMode::Headless`).
    let keep_backlog = spec.local_sink.is_some();
    // EXP-825: the ONE localizer of this run — the seed prompt, a local
    // composer message and (through the lifecycle's publisher) every steered
    // message download into the worktree's steer-images dir. A replay sends
    // nothing and gets none.
    let attachments = (!spec.replay).then(|| {
        steer::image_localizer(
            Arc::clone(&spec.trpc),
            crate::host::steer_images_dir(&spec.run.worktree),
        )
    });
    let mapper = Mapper::new(MapperConfig {
        redactor: Arc::clone(&redactor),
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
        issue_id: spec.issue_id,
        foreign_host: spec.foreign_host,
        publish: spec.publish,
        local_sink: spec.local_sink,
        turn_signal: Arc::new(steer::TurnSignal::new()),
        agent: spec.agent,
        redactor,
        replay: spec.replay,
        resume: spec.resume,
        prompt: spec.prompt,
        embeds: steer::ImageEmbeds::default(),
        attachments,
        mapper: Mutex::new(mapper),
        sink: OnceLock::new(),
        feed: LocalFeed::new(keep_backlog),
        asks: PendingAsks::default(),
        terminals: Default::default(),
        ids: Mutex::new(SessionIds::default()),
        needs_input: AtomicBool::new(false),
        blocked: Mutex::new(None),
        last_activity: Mutex::new(std::time::Instant::now()),
        failure: Mutex::new(None),
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
            // The exit slot is flipped by `RunLifecycle::end` alone, so a
            // PANIC under `run_session` would leave `is_done()` false forever
            // — the daemon's `LiveSession` never leaves its map and a
            // self-update parks behind it. The guard flips it on an unwind.
            let mut guard = ExitGuard::arm(&thread_ctx.exit, &thread_ctx.session_id);
            let runtime = Arc::clone(&thread_ctx.runtime);
            let result = runtime
                .handle()
                .block_on(crate::host::run_session(
                    Arc::clone(&thread_ctx),
                    adapter,
                    inbox,
                ));
            // FEED-25: a watchdog end is not a connection error, but it has
            // a reason the banner must show.
            let error = result
                .err()
                .map(|err| err.to_string())
                .or_else(|| thread_ctx.take_failure());
            let outcome = thread_ctx.end_outcome();
            let child = thread_ctx.child_exit.get();
            lifecycle.end(&thread_ctx, host.as_ref(), &outcome, child, error);
            guard.disarm();
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

/// The engine thread's last resort: an exit for a run that never reached
/// `RunLifecycle::end`.
///
/// `end` is the ONLY writer of the exit slot, and a panic anywhere under
/// `run_session` skips it — leaving `is_done()` false and every `wait()`
/// blocked for the life of the process. Armed for the whole thread body and
/// disarmed once `end` returned, so the normal path never writes twice.
///
/// Deliberately minimal: it flips the slot and nothing else. It runs on an
/// UNWINDING thread, where any lock the panic was holding is poisoned, so it
/// touches only `ExitState::finish` (which recovers from a poisoned summary
/// by skipping it) — no publisher, no row end, no host callback.
struct ExitGuard<'a> {
    exit: &'a ExitState,
    session_id: &'a str,
    armed: bool,
}

impl<'a> ExitGuard<'a> {
    fn arm(exit: &'a ExitState, session_id: &'a str) -> Self {
        ExitGuard { exit, session_id, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ExitGuard<'_> {
    fn drop(&mut self) {
        if !self.armed || self.exit.is_done() {
            return;
        }
        self.exit.finish(&EngineExit {
            session_id: self.session_id.to_string(),
            outcome: "ended".to_string(),
            child: None,
            error: Some("the engine thread ended unexpectedly".to_string()),
            end: None,
        });
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

/// How a resumed (or replayed) run is re-entered: the recorded ACP id, or the
/// agent's own native id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeHandle {
    /// The recorded ACP `sessionId` — `session/load` or `session/resume`.
    Acp(String),
    /// claude's transcript uuid (`--resume=<id>`), codex's thread id.
    Native(String),
}

impl From<coding::ResumeSeed> for ResumeHandle {
    fn from(seed: coding::ResumeSeed) -> Self {
        match seed {
            coding::ResumeSeed::Acp(id) => ResumeHandle::Acp(id),
            coding::ResumeSeed::Native(id) => ResumeHandle::Native(id),
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

impl From<api::error::ApiError> for EngineError {
    fn from(err: api::error::ApiError) -> Self {
        EngineError::Api(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run that dies without reaching `RunLifecycle::end` (a panic under
    /// `run_session`) STILL ends: the daemon reaps on `is_done`, so a slot
    /// that never flips parks a self-update forever. The normal path disarms
    /// the guard, so it never writes over the real exit.
    #[test]
    fn the_exit_guard_finishes_a_run_that_never_ended() {
        let exit = ExitState::default();
        drop(ExitGuard::arm(&exit, "sess-1"));
        assert!(exit.is_done());
        let finished = exit.wait(Some(Duration::from_millis(0))).expect("an exit");
        assert_eq!(finished.session_id, "sess-1");
        assert!(finished.error.is_some());

        let ended = ExitState::default();
        let mut guard = ExitGuard::arm(&ended, "sess-2");
        guard.disarm();
        drop(guard);
        assert!(!ended.is_done());
    }

    /// EXP-792: the team servers' device-held values mask like the key.
    #[test]
    fn the_team_mcp_secrets_join_the_redactor_set() {
        let dir = std::env::temp_dir().join(format!("exp792-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let redactor = steer::Redactor::new(session_secrets(
            &dir,
            Some("expu_personalkey1234".to_string()),
            vec![
                "oauth-access-token-value-1".to_string(),
                "ghp_typed_value_2".to_string(),
            ],
        ));
        let masked = redactor.redact(
            "curl -H 'Authorization: Bearer oauth-access-token-value-1' GITHUB_TOKEN=ghp_typed_value_2 expu_personalkey1234",
        );
        assert!(!masked.contains("oauth-access-token-value-1"), "{masked}");
        assert!(!masked.contains("ghp_typed_value_2"), "{masked}");
        assert!(!masked.contains("expu_personalkey1234"), "{masked}");
        // Nothing extra is masked when there is nothing to mask.
        let bare = steer::Redactor::new(session_secrets(&dir, None, Vec::new()));
        assert_eq!(bare.redact_exact_only("plain text"), "plain text");
        let _ = std::fs::remove_dir_all(&dir);
    }

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
