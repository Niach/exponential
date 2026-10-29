//! EXP-746 — starting a run and the handle every host holds onto.
//!
//! EXP-1426 part 4d: moved into the agent runtime out of Exponential's
//! `engine::session`, which wraps it. [`start`] is the whole server-free entry
//! point: a [`LaunchSpec`] (the user's own `claude` / `codex` adapter), a
//! [`SessionSpec`] (where events go, the tokio runtime) and a
//! [`RunBackend`] ([`NoBackend`](super::host::NoBackend) for none). It is
//! NON-BLOCKING: it spawns the `acp-engine-<sid8>` thread and returns, so a
//! handshake failure never surfaces here — it arrives through the backend's
//! end sequence as an exit with `error: Some(..)`.
//!
//! The exit contract deliberately mirrors `cli::session_host::RunningSession`
//! (`is_done`/`wait`/`wait_timeout`/`kill`) so the daemon's `LiveSession`
//! bookkeeping, reap block and quit sweep compile against either backend.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use agent_client_protocol::{Client, ConnectTo};

use super::adapters::{Adapter, AdapterKind, LaunchSpec};
use super::host::{
    thread_name, ChildExitLink, EngineCommand, EngineExit, ExitState, LocalFeed, LocalSink,
    PendingAsks, PromptLocalizer, RunBackend, RunFacts, SessionCtx, SessionIds,
};
use super::host_tools::{HostTools, NoHostTools};
use super::launch::{EngineError, ResumeHandle};
use super::local::{EnginePhase, LocalFeedEvent};
use super::mapper::{Mapper, MapperConfig};
use super::sink::EventSink;
use crate::agent::CodingAgent;
use crate::wire;

/// A cheap-clone handle on a live (or replaying) run. `Send + Sync`.
#[derive(Clone)]
pub struct EngineSession(Arc<Inner>);

/// The mutable half a session accumulates. Constructed only by [`start_with`].
pub(crate) struct Inner {
    pub(crate) ctx: Arc<SessionCtx>,
    pub(crate) agent: CodingAgent,
    pub(crate) commands: flume::Sender<EngineCommand>,
}

impl EngineSession {
    pub fn session_id(&self) -> &str {
        &self.0.ctx.session_id
    }

    pub fn agent(&self) -> crate::agent::CodingAgent {
        self.0.agent
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
        self.send(EngineCommand::Prompt(super::host::text_blocks(&text)));
    }

    /// Mid-turn steering — the same entry point a relay `input` frame takes.
    pub fn steer(&self, text: String) {
        self.send(EngineCommand::Steer(text));
    }

    /// EXP-1292: a person's `messages` row into the session — between turns
    /// it starts one, mid-turn it goes to the agent like a steer (held while
    /// a compaction is open). The row id is the queue line's id
    /// ([`EngineSession::unqueue`] revokes it) and the engine reports it
    /// consumed (`sessions.consume`) once the agent has it.
    pub fn deliver(&self, message_id: String, text: String) {
        self.send(EngineCommand::Deliver { message_id, text, fresh: false });
    }

    /// EXP-1280: [`Self::deliver`] in a FRESH context — a plan approved "in
    /// a fresh context": claude clears the conversation (`/clear`) and takes
    /// `text` (which carries the plan) as the new conversation's first
    /// prompt; codex, which has no such reset, takes it in the running one.
    /// The row is consumed like any delivery.
    pub fn deliver_fresh(&self, message_id: String, text: String) {
        self.send(EngineCommand::Deliver { message_id, text, fresh: true });
    }

    /// EXP-1292: a question/plan card answered IN the Conversation (the
    /// `messages.card` JSON: `answer`, or `decision` + `feedback`), fed to
    /// the open ask `question_id` exactly like a relay answer.
    pub fn answer_card(&self, question_id: &str, card: &serde_json::Value) {
        self.send(EngineCommand::AnswerCard {
            question_id: question_id.to_string(),
            card: card.clone(),
        });
    }

    /// EXP-1292: the question/plan cards this run posted that are still
    /// open — `messages.id` → the engine's question id (the host's
    /// `LiveView::open_cards`). Empty on a replay.
    pub fn open_cards(&self) -> std::collections::HashMap<String, String> {
        self.0
            .ctx
            .backend
            .get()
            .map(|backend| backend.open_cards())
            .unwrap_or_default()
    }

    /// Answer a pending question card. Resolves the parked ACP `Responder`,
    /// then acks (D3).
    pub fn answer(&self, answer: wire::RemoteAnswer) {
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

    /// A PERSON's mode switch (the Plan chip): EXP-1280 a real plan-mode
    /// move steers the agent one line ([`super::host::plan_mode_on_line`] /
    /// `PLAN_MODE_OFF_LINE`).
    pub fn set_mode(&self, id: &str) {
        self.send(EngineCommand::SetMode {
            id: agent_client_protocol::schema::v1::SessionModeId::new(id.to_string()),
            announce: true,
        });
    }

    /// EXP-1280: flip Exponential's plan flag WITHOUT telling the agent —
    /// a plan approval (the answer the agent reads already says so).
    pub fn set_plan_mode_silent(&self, on: bool) {
        self.send(EngineCommand::SetMode {
            id: agent_client_protocol::schema::v1::SessionModeId::new(plan_mode_id(on).to_string()),
            announce: false,
        });
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

    /// Interrupt the running turn without ending the session. EXP-861: a
    /// Stop also drops the messages held queued behind the turn.
    pub fn cancel_turn(&self) {
        self.send(EngineCommand::Cancel);
    }

    /// EXP-861: revoke ONE message the host holds queued behind the running
    /// turn — the local composer's × on the queue bar (a viewer's arrives as
    /// the relay `unqueue` frame). The re-published `queue` slot is the
    /// confirmation; an unknown id is a no-op. EXP-1292: a delivered row's
    /// id is its `messages.id` (the pickup's revoke of an unqueued row).
    pub fn unqueue(&self, id: &str) {
        self.send(EngineCommand::Unqueue(id.to_string()));
    }

    /// EXP-936: the run's own `exponential_sessions_compact`, as the host
    /// decides it — the verdict the relay hands back to the web server. A
    /// remote ask arrives through the publisher's hook and takes the same
    /// path; this entry is for the process hosting the run (tests, a local
    /// composer). An acceptance sends the agent's `/compact <keep>` at the
    /// next turn boundary and a continuation prompt after the compaction.
    pub fn request_compaction(&self, keep: Option<String>) -> wire::frames::CompactVerdict {
        self.0.ctx.request_compaction(keep)
    }

    pub fn turn_signal(&self) -> Arc<wire::TurnSignal> {
        self.0.ctx.turn_signal.clone()
    }

    /// EXP-848 / FEED-44: the value the synced `agent_busy` column mirrors —
    /// a prompt in flight, or background tasks still running after
    /// the turn that launched them ended. NOT the turn signal: that one
    /// stays "between turns" while they run, so the queue drains and an
    /// account switch may proceed.
    pub fn agent_busy(&self) -> bool {
        self.0.ctx.agent_busy()
    }

    /// EXP-1374: the wall-clock time of the run's last activity (FEED-25's
    /// clock: any mapper output, tool call, terminal output) — the
    /// heartbeat's `sessionActivity` entry.
    pub fn last_activity_at(&self) -> std::time::SystemTime {
        let ago = std::time::Instant::now().saturating_duration_since(self.0.ctx.last_activity());
        std::time::SystemTime::now()
            .checked_sub(ago)
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    }

    /// EXP-1374: the value the synced `background` column mirrors — the
    /// turn is over but background tasks (shell, workflow, subagent) still
    /// run, so the run is busy for that reason only.
    pub fn background(&self) -> bool {
        self.0.ctx.background()
    }

    /// EXP-1005: the agent's usage wall as the engine last saw it (`None` =
    /// not blocked) — what a host's account-rotation beat reads, together
    /// with the turn slot, to decide whether this run may move to another
    /// account right now. RAW: a wall whose own reset stamp passed still
    /// reads here (Exponential's `engine::EngineSession::blocked` drops it
    /// by the rule the synced row follows).
    pub fn blocked_wall(&self) -> Option<wire::SessionBlocked> {
        self.0.ctx.blocked.lock().ok().and_then(|wall| wall.clone())
    }

    /// EXP-1280: is Exponential's plan flag on (`None` = the agent
    /// advertised no modes)?
    pub fn plan_mode(&self) -> Option<bool> {
        self.0.ctx.with_mapper(|mapper| mapper.current_mode().map(|mode| mode == PLAN_MODE_ID))
    }

    /// EXP-1334: the live `model` option, as the agent last reported it.
    pub fn model(&self) -> Option<String> {
        self.0.ctx.with_mapper(|mapper| mapper.current_model().map(str::to_string))
    }

    /// FEED-25's activity clock, as an `Instant`.
    pub fn last_activity(&self) -> std::time::Instant {
        self.0.ctx.last_activity()
    }

    /// EXP-850 §8: the in-process workflow caption of THIS run — the second
    /// line a session row hosted here renders. EXP-1292: local only; no
    /// column carries it any more.
    pub fn caption_signal(&self) -> Arc<wire::CaptionSignal> {
        self.0.ctx.caption_signal.clone()
    }

    /// EXP-905: the name the agent gave this conversation, in process — the
    /// same value the engine forwards to the synced `agent_title` column.
    /// `None` until the agent names it.
    pub fn agent_title(&self) -> Option<String> {
        self.0.ctx.agent_title.get()
    }

    /// Stop now and end the row with `outcome` as the publisher `bye`.
    /// Idempotent.
    pub fn kill(&self, outcome: &'static str) {
        self.send(EngineCommand::Shutdown { outcome });
    }

    pub fn is_done(&self) -> bool {
        self.0.ctx.exit.is_done()
    }

    /// Blocks until the run ends. The FULL exit (with the backend's last
    /// write) went to the backend's host; this rebuilds the same exit with
    /// `end: None`, which is all a waiter needs — Exponential's
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

/// Start a run of the user's own agent CLI: spawn its ACP adapter from
/// `launch` and drive it. Non-blocking; handshake failures arrive through
/// the backend's end sequence.
pub fn start(
    launch: LaunchSpec,
    session: SessionSpec,
    backend: Arc<dyn RunBackend>,
) -> Result<EngineSession, EngineError> {
    let adapter = Adapter::new(launch)?;
    start_with(session, adapter, backend)
}

/// [`start`] with the transport handed in. The ONE seam a test drives (a
/// scripted `ConnectTo<Client>`), and the one Exponential's `engine::start`
/// takes once it built its adapter.
pub fn start_with<A>(
    session: SessionSpec,
    adapter: A,
    backend: Arc<dyn RunBackend>,
) -> Result<EngineSession, EngineError>
where
    A: ConnectTo<Client> + 'static,
{
    let agent = session.agent;
    let sink = session.sink.clone();
    let ctx = build_ctx(session);
    if let Some(sink) = sink {
        let _ = ctx.sink.set(sink);
    }
    spawn_engine(ctx, agent, adapter, backend)
}

// EXP-1334/EXP-1280: the ACP mode ids both adapters advertise
// (`available_modes`), never re-typed here.
use super::adapters::{BUILD_MODE_ID, PLAN_MODE_ID};

/// The ACP mode id for Exponential's plan flag.
pub fn plan_mode_id(on: bool) -> &'static str {
    if on {
        PLAN_MODE_ID
    } else {
        BUILD_MODE_ID
    }
}

/// Everything the session context needs besides the adapter. Build one
/// with [`SessionSpec::for_launch`] and adjust what the host knows more of.
pub struct SessionSpec {
    /// The host's session id (Exponential: the `sessions` row id).
    pub session_id: String,
    pub run: RunFacts,
    /// The tokio runtime the connection is driven on.
    pub runtime: tokio::runtime::Handle,
    /// The host's data dir (Exponential: its `runs.json`, the journals).
    pub data_dir: PathBuf,
    /// The `expu_`-style key the redactor masks exactly (REV2-17).
    pub personal_key: Option<String>,
    /// EXP-1292/1341: a PERSISTENT session (an agent root): its process
    /// exiting idles the row instead of ending it.
    pub persistent: bool,
    /// EXP-1292: the host message id the seed prompt delivers.
    pub seed_message_id: Option<String>,
    /// EXP-1005: the host runs an account-rotation beat.
    pub rotation_host: bool,
    /// `false` = no relay room at all (the backend reads it).
    pub publish: bool,
    /// The rich, host-local feed; `None` keeps the small headless backlog.
    pub local_sink: Option<LocalSink>,
    pub agent: CodingAgent,
    /// EXP-792: every MCP secret the launcher put in the spawn env —
    /// exact-match entries for the redactor beside the key.
    pub mcp_secrets: Vec<String>,
    pub replay: bool,
    pub resume: Option<ResumeHandle>,
    pub prompt: Option<String>,
    /// EXP-1330: a `/btw` aside run.
    pub aside: bool,
    /// The link the ADAPTER records its child's exit into — the same one, so
    /// the end sequence reads what the adapter wrote.
    pub child_exit: ChildExitLink,
    /// EXP-825: the localizer for the host's own prompts (`None` = sent as
    /// they are).
    pub attachments: Option<Arc<dyn PromptLocalizer>>,
    /// The host's own tools for the mapper ([`NoHostTools`] for none).
    pub host_tools: Arc<dyn HostTools>,
    /// Where wire events go ([`super::sink::RecordingSink`], a voice loop, a
    /// UI). `None` = the backend sets one on attach, or nowhere.
    pub sink: Option<Arc<dyn EventSink>>,
}

impl SessionSpec {
    /// The spec a server-free host starts `launch` with: its session id,
    /// worktree, agent, seed prompt, resume handle and child-exit link,
    /// nothing published, no host tools, no localizer.
    pub fn for_launch(launch: &LaunchSpec, runtime: tokio::runtime::Handle) -> SessionSpec {
        SessionSpec {
            session_id: launch.session_id.clone(),
            run: RunFacts {
                worktree: launch.cwd.clone(),
                ..RunFacts::default()
            },
            runtime,
            data_dir: PathBuf::new(),
            personal_key: launch.personal_key.clone(),
            persistent: false,
            seed_message_id: None,
            rotation_host: false,
            publish: false,
            local_sink: None,
            agent: launch.agent,
            mcp_secrets: Vec::new(),
            replay: launch.replay,
            resume: launch.resume.clone(),
            prompt: launch.prompt.clone(),
            aside: launch.aside,
            child_exit: launch.exit.clone(),
            attachments: None,
            host_tools: Arc::new(NoHostTools),
            sink: None,
        }
    }
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
    let mut secrets = wire::activity::secrets_from_worktree(worktree);
    secrets.extend(personal_key);
    secrets.extend(mcp_secrets);
    secrets
}

fn build_ctx(spec: SessionSpec) -> Arc<SessionCtx> {
    // REV2-17: the session's own launcher secrets (the EXP-73 credential
    // file, a token in a remote URL, the `.exp-mcp.json` key) plus the
    // `expu_` key, masked out of every wire string. Built ONCE and shared:
    // the mapper's strings and the lifecycle's `diff` ticker are two
    // publishers of the same run and must mask the same set, or the weaker
    // one becomes the leak.
    let redactor = Arc::new(wire::Redactor::new(session_secrets(
        &spec.run.worktree,
        spec.personal_key,
        spec.mcp_secrets,
    )));
    // EXP-766: a host with a local sink (the desktop) reattaches a view
    // mid-run and keeps the full row backlog; a headless host keeps only the
    // small attach-window ring (`BacklogMode::Headless`).
    let keep_backlog = spec.local_sink.is_some();
    // EXP-825: a replay sends nothing and gets no localizer.
    let attachments = if spec.replay { None } else { spec.attachments };
    let mapper = Mapper::new(MapperConfig {
        redactor: Arc::clone(&redactor),
        cwd: spec.run.worktree.clone(),
        agent: AdapterKind::from_agent(spec.agent).session_agent(),
        session_seed: spec.session_id.clone(),
        host_tools: spec.host_tools,
    });
    Arc::new(SessionCtx {
        session_id: spec.session_id,
        run: spec.run,
        runtime: spec.runtime,
        data_dir: spec.data_dir,
        persistent: spec.persistent,
        seed_message_id: spec.seed_message_id,
        backend: OnceLock::new(),
        message_ids: Mutex::new(std::collections::HashSet::new()),
        rotation_host: spec.rotation_host,
        publish: spec.publish,
        local_sink: spec.local_sink,
        turn_signal: Arc::new(wire::TurnSignal::new()),
        caption_signal: Arc::new(wire::CaptionSignal::new()),
        aside: spec.aside.then(super::aside::AsideState::default),
        agent_title: Arc::new(wire::AgentTitleSignal::new()),
        workflows: Mutex::new(Vec::new()),
        agent: spec.agent,
        redactor,
        replay: spec.replay,
        resume: spec.resume,
        prompt: spec.prompt,
        attachments,
        open_questions: Mutex::new(std::collections::VecDeque::new()),
        mapper: Mutex::new(mapper),
        sink: OnceLock::new(),
        feed: LocalFeed::new(keep_backlog),
        asks: PendingAsks::default(),
        terminals: Default::default(),
        ids: Mutex::new(SessionIds::default()),
        needs_input: AtomicBool::new(false),
        background_tasks_live: AtomicBool::new(false),
        blocked: Mutex::new(None),
        prompt_queue: Mutex::new(std::collections::VecDeque::new()),
        queue_commands: OnceLock::new(),
        drain_wanted: AtomicBool::new(false),
        compaction: Arc::new(Mutex::new(super::compaction::CompactionPolicy::default())),
        last_activity: Mutex::new(std::time::Instant::now()),
        seed_retry: std::sync::atomic::AtomicBool::new(false),
        turns_started: std::sync::atomic::AtomicU64::new(0),
        busy_at_shutdown: std::sync::atomic::AtomicBool::new(false),
        api_retry: Mutex::new(super::api_retry::ApiRetry::new()),
        failure: Mutex::new(None),
        exit: ExitState::default(),
        outcome: Mutex::new(None),
        child_exit: spec.child_exit,
    })
}

/// Attach the backend, then run the connection on its own OS thread.
///
/// `Handle::block_on` gives the whole graph a tokio context (the mapper's
/// flush tick, the ACP timers) WITHOUT requiring the top-level future to be
/// `Send` and without a `LocalSet`, and it blocks a PLAIN thread — never one
/// of `SteerRuntime`'s two workers (D1).
fn spawn_engine<A>(
    ctx: Arc<SessionCtx>,
    agent: CodingAgent,
    adapter: A,
    backend: Arc<dyn RunBackend>,
) -> Result<EngineSession, EngineError>
where
    A: ConnectTo<Client> + 'static,
{
    let (commands, inbox) = flume::unbounded();
    // EXP-861: the queue drains through the command loop, so the context
    // needs the loop's own inbox handle.
    let _ = ctx.queue_commands.set(commands.clone());
    // EXP-1426 part 4d: the backend is on the context BEFORE it attaches,
    // so the first edge its side-car produces already reaches it.
    let _ = ctx.backend.set(Arc::clone(&backend));
    backend.attach(&ctx, &commands)?;
    let thread_ctx = Arc::clone(&ctx);
    let spawned = std::thread::Builder::new()
        .name(thread_name(&ctx.session_id))
        .spawn(move || {
            // The exit slot is flipped by the backend's `end` alone, so a
            // PANIC under `run_session` would leave `is_done()` false forever
            // — the daemon's `LiveSession` never leaves its map and a
            // self-update parks behind it. The guard flips it on an unwind.
            let mut guard = ExitGuard::arm(&thread_ctx.exit, &thread_ctx.session_id);
            let runtime = thread_ctx.runtime.clone();
            let result = runtime
                .block_on(super::host::run_session(
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
            backend.end(&thread_ctx, &outcome, child, error);
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
/// its backend's `end`.
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
        self.exit.finish(&EngineExit::<()> {
            session_id: self.session_id.to_string(),
            outcome: "ended".to_string(),
            child: None,
            error: Some("the engine thread ended unexpectedly".to_string()),
            end: None,
        });
    }
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
#[cfg(test)]
mod tests {
    use super::*;

    /// A run that dies without reaching its backend's `end` (a panic under
    /// `run_session`) STILL ends: the daemon reaps on `is_done`, so a slot
    /// that never flips parks a self-update forever. The normal path disarms
    /// the guard, so it never writes over the real exit.
    #[test]
    fn the_exit_guard_finishes_a_run_that_never_ended() {
        let exit = ExitState::default();
        drop(ExitGuard::arm(&exit, "sess-1"));
        assert!(exit.is_done());
        let finished = exit.wait::<()>(Some(Duration::from_millis(0))).expect("an exit");
        assert_eq!(finished.session_id, "sess-1");
        assert!(finished.error.is_some());

        let ended = ExitState::default();
        let mut guard = ExitGuard::arm(&ended, "sess-2");
        guard.disarm();
        drop(guard);
        assert!(!ended.is_done());
    }

    /// EXP-792: the team servers' credential values mask like the key.
    #[test]
    fn the_team_mcp_secrets_join_the_redactor_set() {
        let dir = std::env::temp_dir().join(format!("exp792-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let redactor = wire::Redactor::new(session_secrets(
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
        let bare = wire::Redactor::new(session_secrets(&dir, None, Vec::new()));
        assert_eq!(bare.redact_exact_only("plain text"), "plain text");
        let _ = std::fs::remove_dir_all(&dir);
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
