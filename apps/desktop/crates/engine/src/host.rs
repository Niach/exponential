//! EXP-746 — the ACP client host: what the engine advertises, what it asks of
//! whoever started it, and the command loop that keeps ONE `ConnectionTo`
//! open across a whole session.
//!
//! Two dispatch loops sit back to back — our `Client` role and the adapter's
//! `Agent` role — and the ONE rule that keeps them alive is:
//! **any handler that can take more than a few milliseconds must
//! `cx.spawn(...)` with its `Responder` moved in.** Handlers run inside the
//! dispatch loop and block every further message until they return, so a
//! permission handler that awaits a relay round-trip inline deadlocks the
//! whole session — silently, with no error. The matching hazard on the way
//! out: a task handed to `ConnectionTo::spawn` that returns `Err` shuts the
//! WHOLE connection down, so every spawned task swallows its error into
//! `Responder::respond_with_error` and returns `Ok(())`.
//!
//! What runs where:
//!
//! - `session/update` → [`crate::mapper::Mapper::on_update`] INLINE (pure,
//!   microseconds) and straight out through [`SessionCtx::dispatch`];
//! - `session/request_permission` / `elicitation/create` → mapped inline (so
//!   cards publish in arrival order) and then SPAWNED to await the answer;
//! - `fs/read_text_file` / `fs/write_text_file` → inline, one `std::fs` call;
//! - `session/prompt` → spawned from the command loop, so a `Cancel` arriving
//!   mid-turn still reaches the adapter.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    BooleanConfigOptionCapabilities, CancelNotification, ClientCapabilities,
    ClientSessionCapabilities, CompactionCapabilities, ContentBlock, CreateElicitationRequest,
    CreateElicitationResponse, ElicitationAcceptAction, ElicitationAction, ElicitationCapabilities,
    ElicitationContentValue, ElicitationFormCapabilities, FileSystemCapabilities,
    InitializeRequest, LoadSessionRequest, NewSessionRequest, PermissionOptionId, PromptRequest,
    ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionConfigId,
    SessionConfigOptionValue, SessionId, SessionModeId, SessionNotification,
    SessionConfigOptionsCapabilities, SetSessionConfigOptionRequest, SetSessionModeRequest,
    StopReason, TextContent, WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Client, ConnectTo, Error,
};

use crate::local::{EnginePhase, LocalFeedEvent};
use crate::mapper::{AnswerDecision, MapOut, Mapper, PendingAskKey, FLUSH_IDLE};
use crate::session::ResumeHandle;
use crate::sink::EventSink;

/// Where the rich, host-local feed items go. `None` on the daemon, which
/// publishes and nothing else.
///
/// It is called from INSIDE the ACP dispatch loop, so it must never block:
/// the desktop pushes onto a channel and marshals to the gpui foreground
/// itself (`EngineSession::subscribe` is the same stream, backlog first, for
/// a view that attaches late).
pub type LocalSink = Arc<dyn Fn(LocalFeedEvent) + Send + Sync>;

/// The `_meta` key an adapter stamps its own session identity under, on the
/// `session/new` (or `session/load`) RESPONSE: claude's stream-json
/// `session_id`, codex's `thread.id`, pi's session file path. It lands on the
/// run record as `agent_native_session_id` (D8) and is what a PTY resume of
/// the same conversation would need.
pub const NATIVE_SESSION_META_KEY: &str = "exponentialNativeSessionId";

/// How many local feed events a session keeps for a late subscriber. A tab
/// reopened mid-session replays this much and no more: `Output` chunks are
/// unbounded by nature, and an engine that kept every one of them would grow
/// without limit on a long run.
pub const BACKLOG_CAP: usize = 4096;

/// Why the run must stop. Produced by the host's own kill source: the
/// desktop's Electric `sync::kill_watch`, the CLI's 15 s tRPC poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillReason {
    /// Stop now (a relay `kill` frame, an `ended` row flipped by a person).
    Now,
    /// The agent declared its own run over (EXP-637) — wait `steer::STOP_GRACE`
    /// on the `TurnSignal` so the last turn lands before the row ends.
    AfterTurn,
}

/// A kill source the HOST registered before `engine::start`, so the watch
/// exists before the first frame can arrive. The engine only consumes edges;
/// the decision (`kill_poll_decision`, EXP-681's `GATED_KILL_AFTER`,
/// `ended_policy`) stays with the host that owns it.
///
/// Dropping this unwatches: the engine drops it FIRST in the end sequence, so
/// the row's own `ended` flip can never bounce back as a second kill (EXP-283).
pub struct KillFeed {
    pub rx: flume::Receiver<KillReason>,
    pub unwatch: Option<Box<dyn FnOnce() + Send>>,
}

impl KillFeed {
    /// A feed nothing ever fires — tests, and `open_transcript` (a replay has
    /// no row to kill).
    pub fn inert() -> KillFeed {
        let (_tx, rx) = flume::unbounded();
        KillFeed { rx, unwatch: None }
    }
}

impl Drop for KillFeed {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
    }
}

/// The one callback the two hosts implement. Fired ONCE per run, off the
/// engine thread — the desktop marshals to the gpui foreground itself.
pub trait EngineHost: Send + Sync + 'static {
    fn on_exit(&self, exit: EngineExit);
}

/// How a run finished. `end` is the result of `coding::end_session`, carried
/// here so the CLI can apply `registry::end_outcome_resolves` (EXP-641)
/// without ending the row a second time itself.
#[derive(Debug)]
pub struct EngineExit {
    pub session_id: String,
    /// The publisher `bye` outcome, in today's vocabulary:
    /// `exit:<code>` | `ended` | `killed`.
    pub outcome: String,
    /// Present when the adapter owned a child process.
    pub child: Option<terminal::pty::ChildExit>,
    /// A handshake or transport failure, for the screen's banner.
    pub error: Option<String>,
    pub end: Option<Result<api::coding_sessions::CodingSession, api::error::ApiError>>,
}

/// What the client advertises at `initialize`.
///
/// `terminal: false` is deliberate (D5): command output renders as local
/// cards built from `ToolCallContent::Content`, so `terminal/create|output|
/// wait_for_exit|kill|release` stay unimplemented and claude takes its
/// fenced-console branch. `elicitation.form` is NOT optional — without it the
/// claude port has to disallow `AskUserQuestion` and codex answers
/// `requestUserInput` with `{}` immediately, silently discarding the agent's
/// question.
pub fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities::new()
        .fs(FileSystemCapabilities::new()
            .read_text_file(true)
            .write_text_file(true))
        .terminal(false)
        .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()))
        .session(
            ClientSessionCapabilities::new()
                .config_options(
                    SessionConfigOptionsCapabilities::new()
                        .boolean(BooleanConfigOptionCapabilities::new()),
                )
                // EXP-724's relay `compaction` kind is fed by
                // `SessionUpdate::CompactionUpdate`, which is gated behind the
                // crate's `unstable_session_compaction` feature.
                .compaction(CompactionCapabilities::new()),
        )
}

/// Fire-and-forget commands the `main_fn` loop drains for the session's whole
/// lifetime. This is the ONLY supported way to hold a `ConnectionTo` open
/// across prompts: `Builder::into_connection_and_future` is private, so the
/// connection lives exactly as long as `main_fn` does.
pub(crate) enum EngineCommand {
    Prompt(Vec<ContentBlock>),
    /// Mid-turn steering — the same entry point a relay `input` frame takes.
    Steer(String),
    Cancel,
    SetConfig {
        id: SessionConfigId,
        value: SessionConfigOptionValue,
    },
    SetMode(SessionModeId),
    Answer(steer::RemoteAnswer),
    /// A `/` command: an agent-advertised one becomes prompt text, a contract
    /// one is handled by `steer`'s `CommandLink` before it ever gets here.
    Command {
        name: String,
        args: String,
    },
    /// Replay history through the mapper (`session/load`).
    LoadHistory,
    Shutdown {
        outcome: &'static str,
    },
}

/// One user message as ACP content blocks.
pub(crate) fn text_blocks(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new(text))]
}

// ---------------------------------------------------------------------------
// The child a stdio adapter owns (EXP-746)
// ---------------------------------------------------------------------------

/// How an adapter reports its CHILD's exit to the engine core.
///
/// The adapter owns the process (`crate::transport::ChildLines`), the engine
/// owns the outcome vocabulary — `exit:<code>` is the publisher `bye` every
/// client already reads. An adapter records the exit here the moment its wait
/// thread reaps; the engine reads it in the end sequence and, absent an
/// explicit kill, ends the row with that code.
///
/// It reaches the adapters as a field on `AdapterSpec` (E2/E3/E4 record into
/// it; an adapter that owns no child never touches it, and the run then ends
/// as `ended`).
#[derive(Clone)]
pub struct ChildExitLink {
    slot: Arc<Mutex<Option<terminal::pty::ChildExit>>>,
    /// Held until the exit is recorded; dropping it is the signal
    /// [`ChildExitLink::reaped`] waits on. A flume receiver whose senders are
    /// all gone resolves immediately and KEEPS resolving, so the edge is
    /// memoryful: a waiter that arrives after the child died sees it too.
    gate: Arc<Mutex<Option<flume::Sender<()>>>>,
    signal: flume::Receiver<()>,
}

impl Default for ChildExitLink {
    fn default() -> ChildExitLink {
        ChildExitLink::new()
    }
}

/// How long a closing adapter waits for its child's exit CODE.
///
/// The child's stdout ends on one thread and `wait()` reaps it on another, so
/// the two race by microseconds — but the end sequence reads the code once,
/// right after the connection closes. Without this grace a crashed CLI would
/// end the run as a bare `ended` instead of `exit:<code>`.
pub(crate) const CHILD_EXIT_GRACE: Duration = Duration::from_secs(2);

impl ChildExitLink {
    pub fn new() -> ChildExitLink {
        let (gate, signal) = flume::bounded(0);
        ChildExitLink {
            slot: Arc::new(Mutex::new(None)),
            gate: Arc::new(Mutex::new(Some(gate))),
            signal,
        }
    }

    /// Adapter side: the child was reaped.
    pub fn record(&self, exit: terminal::pty::ChildExit) {
        if let Ok(mut slot) = self.slot.lock() {
            slot.get_or_insert(exit);
        }
        // Dropped last: a waiter woken by this must find the code already in
        // the slot.
        if let Ok(mut gate) = self.gate.lock() {
            gate.take();
        }
    }

    /// Engine side: what the child exited with, if it did.
    pub fn get(&self) -> Option<terminal::pty::ChildExit> {
        self.slot.lock().ok().and_then(|slot| slot.clone())
    }

    /// Resolves once [`ChildExitLink::record`] has run — immediately, if it
    /// already has. An adapter awaits this (bounded by [`CHILD_EXIT_GRACE`])
    /// between its child's stdout EOF and closing the connection.
    pub async fn reaped(&self) {
        let _ = self.signal.recv_async().await;
    }
}

/// The `main_fn` every stdio adapter runs: hold the connection open until
/// EITHER half goes away, then return so the connection shuts down.
///
/// The child half is the load-bearing one. The SDK's own `connect_to` waits
/// only on `incoming_closed`, and the host's loop waits only on the adapter,
/// so an agent CLI that crashed (OOM, `kill -9`, the reaper) would leave the
/// two halves waiting on each other forever: no `on_exit`, no `exit:<code>`,
/// a heartbeat still marking the row `running` and a desktop tab still Live
/// (EXP-746 review E1). On the child's edge the exit code gets
/// [`CHILD_EXIT_GRACE`] to land so the run ends as `exit:<code>`.
pub(crate) async fn until_either_closes(
    cx: &agent_client_protocol::ConnectionTo<Client>,
    exit: &ChildExitLink,
    child_gone: impl std::future::Future<Output = ()>,
) {
    tokio::select! {
        () = cx.incoming_closed() => {}
        () = child_gone => {
            let _ = tokio::time::timeout(CHILD_EXIT_GRACE, exit.reaped()).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Parked asks
// ---------------------------------------------------------------------------

/// What an answered (or cancelled) card does to the ACP request it is holding.
#[derive(Clone, Debug)]
pub(crate) enum AskOutcome {
    Permission(String),
    Elicitation(serde_json::Value),
    /// `session/cancel`, a kill, or the connection going away: the ACP
    /// contract requires the pending request to be answered, not dropped.
    Cancelled,
}

/// The parked `Responder`s, keyed by [`PendingAskKey::responder_key`] — one
/// entry per permission, one per ELICITATION (all its steps resolve the same
/// request).
#[derive(Default)]
pub(crate) struct PendingAsks(Mutex<std::collections::HashMap<String, flume::Sender<AskOutcome>>>);

impl PendingAsks {
    fn park(&self, key: &PendingAskKey) -> flume::Receiver<AskOutcome> {
        let (tx, rx) = flume::bounded(1);
        if let Ok(mut asks) = self.0.lock() {
            asks.insert(key.responder_key().to_string(), tx);
        }
        rx
    }

    fn resolve(&self, key: &str, outcome: AskOutcome) {
        let sender = self
            .0
            .lock()
            .ok()
            .and_then(|mut asks| asks.remove(key));
        if let Some(sender) = sender {
            let _ = sender.send(outcome);
        }
    }

    /// Cancel everything still parked (the ACP cancellation contract).
    fn cancel_all(&self) {
        let senders: Vec<flume::Sender<AskOutcome>> = match self.0.lock() {
            Ok(mut asks) => asks.drain().map(|(_, sender)| sender).collect(),
            Err(_) => Vec::new(),
        };
        for sender in senders {
            let _ = sender.send(AskOutcome::Cancelled);
        }
    }
}

// ---------------------------------------------------------------------------
// The local feed fan-out
// ---------------------------------------------------------------------------

/// The host-local feed: the attached sink (desktop/CLI), every `subscribe()`
/// receiver, and the backlog a late subscriber replays first.
#[derive(Default)]
pub(crate) struct LocalFeed {
    backlog: Mutex<std::collections::VecDeque<LocalFeedEvent>>,
    subscribers: Mutex<Vec<flume::Sender<LocalFeedEvent>>>,
}

impl LocalFeed {
    fn emit(&self, sink: Option<&LocalSink>, event: LocalFeedEvent) {
        if let Some(sink) = sink {
            sink(event.clone());
        }
        if let Ok(mut backlog) = self.backlog.lock() {
            if backlog.len() >= BACKLOG_CAP {
                backlog.pop_front();
            }
            backlog.push_back(event.clone());
        }
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
        }
    }

    /// A receiver that replays the backlog FIRST, so a view attaching late
    /// sees the whole session rather than the tail.
    pub(crate) fn subscribe(&self) -> flume::Receiver<LocalFeedEvent> {
        let (tx, rx) = flume::unbounded();
        if let Ok(backlog) = self.backlog.lock() {
            for event in backlog.iter() {
                let _ = tx.send(event.clone());
            }
        }
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(tx);
        }
        rx
    }
}

// ---------------------------------------------------------------------------
// The exit slot
// ---------------------------------------------------------------------------

/// The run's one-shot exit, readable by `wait()` and `is_done()`.
///
/// [`EngineExit`] is not `Clone` (`ApiError` is not), and BOTH consumers are
/// real: the desktop takes it through [`EngineHost::on_exit`], the CLI through
/// `wait()`. So the full exit — including the `coding::end_session` result —
/// goes to `on_exit`, and `wait()` rebuilds the same exit from the summary
/// with `end: None`. Nothing downstream needs that result twice: the
/// `SessionEndObserver` already applied it (EXP-641).
#[derive(Default)]
pub(crate) struct ExitState {
    done: AtomicBool,
    summary: Mutex<Option<ExitSummary>>,
    waiters: Condvar,
    lock: Mutex<()>,
}

#[derive(Clone)]
struct ExitSummary {
    session_id: String,
    outcome: String,
    child: Option<terminal::pty::ChildExit>,
    error: Option<String>,
}

impl ExitState {
    pub(crate) fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    pub(crate) fn finish(&self, exit: &EngineExit) {
        if let Ok(mut summary) = self.summary.lock() {
            *summary = Some(ExitSummary {
                session_id: exit.session_id.clone(),
                outcome: exit.outcome.clone(),
                child: exit.child.clone(),
                error: exit.error.clone(),
            });
        }
        self.done.store(true, Ordering::SeqCst);
        let _guard = self.lock.lock();
        self.waiters.notify_all();
    }

    pub(crate) fn wait(&self, timeout: Option<Duration>) -> Option<EngineExit> {
        let deadline = timeout.map(|timeout| std::time::Instant::now() + timeout);
        let mut guard = self.lock.lock().unwrap_or_else(|err| err.into_inner());
        while !self.is_done() {
            match deadline {
                Some(deadline) => {
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        return None;
                    }
                    let (next, _) = self
                        .waiters
                        .wait_timeout(guard, deadline - now)
                        .unwrap_or_else(|err| err.into_inner());
                    guard = next;
                }
                None => {
                    guard = self
                        .waiters
                        .wait(guard)
                        .unwrap_or_else(|err| err.into_inner())
                }
            }
        }
        drop(guard);
        self.summary
            .lock()
            .ok()
            .and_then(|summary| summary.clone())
            .map(|summary| EngineExit {
                session_id: summary.session_id,
                outcome: summary.outcome,
                child: summary.child,
                error: summary.error,
                end: None,
            })
    }
}

// ---------------------------------------------------------------------------
// The session context
// ---------------------------------------------------------------------------

/// What the engine needs to know about the launch it is driving.
#[derive(Clone, Default)]
pub(crate) struct RunFacts {
    pub(crate) worktree: PathBuf,
    pub(crate) branch: String,
    /// EXP-688: the ref "Latest changes" is measured from.
    pub(crate) base_ref: Option<String>,
    /// EXP-447: the clone whose installation token wants refreshing.
    pub(crate) repository_id: Option<String>,
    pub(crate) clone: PathBuf,
    /// EXP-105: re-sent with every heartbeat. `None` on a replay.
    pub(crate) heartbeat_scope: Option<api::coding_sessions::HeartbeatScope>,
}

/// The ids a session learns at handshake time (D8).
#[derive(Clone, Default)]
pub(crate) struct SessionIds {
    pub(crate) acp: Option<String>,
    pub(crate) native: Option<String>,
}

/// Everything a running session needs that never changes after `start`.
/// Cheap to clone behind an `Arc`; the mutable half (the pending-ask table,
/// the ids learned at `session/new`) lives behind its own locks here.
pub(crate) struct SessionCtx {
    pub(crate) session_id: String,
    /// The launch facts the engine itself needs. NOT the whole
    /// `PreparedLaunch`: `open_transcript` replays an ended run that never
    /// had one (no row, no worktree gate, no heartbeat scope).
    pub(crate) run: RunFacts,
    pub(crate) trpc: Arc<api::TrpcClient>,
    pub(crate) runtime: Arc<steer::SteerRuntime>,
    pub(crate) data_dir: std::path::PathBuf,
    /// EXP-105/EXP-444 identity, carried for the adapters and the hosts that
    /// read it off the session; the connection loop itself never needs it.
    #[allow(dead_code)]
    pub(crate) account_id: String,
    #[allow(dead_code)]
    pub(crate) own_user_id: Option<String>,
    pub(crate) issue_id: Option<String>,
    /// EXP-444: a foreign requester on a shared host. The ACP path has no
    /// login affordance to suppress, so nothing reads it yet.
    #[allow(dead_code)]
    pub(crate) foreign_host: bool,
    pub(crate) publish: bool,
    pub(crate) local_sink: Option<LocalSink>,
    pub(crate) turn_signal: Arc<steer::TurnSignal>,
    /// The builtin agent, or the user's external ACP binary (D13).
    pub(crate) agent: coding::AgentKind,
    /// REV2-17: the ONE redactor of this run — the session's launcher secrets
    /// (EXP-73 credential file, a token in the remote URL, the
    /// `.exp-mcp.json` key) plus the `expu_` key, on top of the static
    /// patterns. The mapper holds the same `Arc` and the lifecycle's `diff`
    /// ticker masks with it, so no publisher of this run carries a weaker
    /// secret set than another. The key itself is deliberately NOT kept
    /// here: a second redactor built from it alone is exactly the leak.
    pub(crate) redactor: Arc<steer::Redactor>,
    /// Read-only transcript replay: no row, no publisher, no heartbeat.
    pub(crate) replay: bool,
    pub(crate) resume: Option<ResumeHandle>,
    /// The seed prompt, sent once the session exists.
    pub(crate) prompt: Option<String>,
    pub(crate) mapper: Mutex<Mapper>,
    /// The relay sink. Set ONCE — by `start_with` (a test's recording sink)
    /// or by the lifecycle when the publisher comes up; absent means this run
    /// publishes nowhere (`publish: false`, or a replay).
    pub(crate) sink: OnceLock<Arc<dyn EventSink>>,
    pub(crate) feed: LocalFeed,
    pub(crate) asks: PendingAsks,
    pub(crate) ids: Mutex<SessionIds>,
    /// EXP-214: the latest pending flag; the lifecycle ticker forwards it.
    pub(crate) needs_input: AtomicBool,
    pub(crate) exit: ExitState,
    /// Set by `Shutdown`; wins over the child's exit code.
    pub(crate) outcome: Mutex<Option<&'static str>>,
    pub(crate) child_exit: ChildExitLink,
}

impl SessionCtx {
    pub(crate) fn with_mapper<R>(&self, f: impl FnOnce(&mut Mapper) -> R) -> R {
        let mut mapper = self.mapper.lock().unwrap_or_else(|err| err.into_inner());
        f(&mut mapper)
    }

    /// Route one mapping step: wire events to the relay, local events to the
    /// host, and the two flags to their owners. The ONLY place events leave
    /// the mapper.
    pub(crate) fn dispatch(&self, out: MapOut) {
        if let Some(sink) = self.sink.get() {
            for event in out.wire {
                sink.send(event);
            }
        }
        for event in out.local {
            self.feed.emit(self.local_sink.as_ref(), event);
        }
        if let Some(pending) = out.needs_input {
            self.needs_input.store(pending, Ordering::SeqCst);
        }
        if let Some(idle) = out.idle {
            self.turn_signal.set_idle(idle);
        }
    }

    /// A local-only edge (phases): never a wire event.
    pub(crate) fn emit_local(&self, event: LocalFeedEvent) {
        self.feed.emit(self.local_sink.as_ref(), event);
    }

    pub(crate) fn phase(&self, phase: EnginePhase) {
        self.emit_local(LocalFeedEvent::Phase(phase));
    }

    pub(crate) fn ids(&self) -> SessionIds {
        self.ids
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default()
    }

    fn set_ids(&self, acp: Option<String>, native: Option<String>) {
        if let Ok(mut ids) = self.ids.lock() {
            if acp.is_some() {
                ids.acp = acp;
            }
            if native.is_some() {
                ids.native = native;
            }
        }
    }

    /// The outcome the end sequence publishes as `bye`: an explicit kill
    /// wins, then the child's exit code, then a plain `ended`.
    pub(crate) fn end_outcome(&self) -> String {
        if let Some(outcome) = self.outcome.lock().ok().and_then(|outcome| *outcome) {
            return outcome.to_string();
        }
        match self.child_exit.get() {
            Some(exit) => format!("exit:{}", exit.code),
            None => "ended".to_string(),
        }
    }

    fn set_outcome(&self, outcome: &'static str) {
        if let Ok(mut slot) = self.outcome.lock() {
            slot.get_or_insert(outcome);
        }
    }
}

// ---------------------------------------------------------------------------
// The connection driver
// ---------------------------------------------------------------------------

/// The connection driver. Runs on the dedicated `acp-engine-<sid8>` thread
/// inside `SteerRuntime::handle().block_on(..)` and does not return until the
/// session ends.
///
/// Generic over the transport so the fake adapter in `tests/fake_agent.rs`
/// drives the REAL host: `crate::adapters::Adapter` is just one
/// `ConnectTo<Client>` among them.
pub(crate) async fn run_session<A>(
    ctx: Arc<SessionCtx>,
    adapter: A,
    commands: flume::Receiver<EngineCommand>,
) -> Result<(), Error>
where
    A: ConnectTo<Client> + 'static,
{
    let notify_ctx = ctx.clone();
    let permission_ctx = ctx.clone();
    let elicit_ctx = ctx.clone();
    let main_ctx = ctx.clone();

    Client
        .builder()
        .name("exponential")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                let mut out = MapOut::default();
                notify_ctx.with_mapper(|mapper| mapper.on_update(&notification, &mut out));
                notify_ctx.dispatch(out);
                Ok(())
            },
            on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, cx| {
                // Mapped INLINE so cards publish in arrival order; only the
                // wait is spawned.
                let mut out = MapOut::default();
                let key = permission_ctx
                    .with_mapper(|mapper| mapper.on_permission(&request, &mut out));
                permission_ctx.dispatch(out);
                let answers = permission_ctx.asks.park(&key);
                cx.spawn(async move {
                    let outcome = match answers.recv_async().await {
                        Ok(AskOutcome::Permission(option_id)) => {
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                PermissionOptionId::new(option_id),
                            ))
                        }
                        // A dropped sender is the connection going away: the
                        // ACP contract still wants an answer.
                        _ => RequestPermissionOutcome::Cancelled,
                    };
                    if let Err(err) = responder.respond(RequestPermissionResponse::new(outcome)) {
                        log::warn!("engine: permission response failed: {err}");
                    }
                    Ok(())
                })?;
                Ok(())
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |request: CreateElicitationRequest, responder, cx| {
                let ask_id = elicitation_id(&request);
                let mut out = MapOut::default();
                let key = elicit_ctx
                    .with_mapper(|mapper| mapper.on_elicitation(&ask_id, &request, &mut out));
                elicit_ctx.dispatch(out);
                let answers = elicit_ctx.asks.park(&key);
                cx.spawn(async move {
                    let action = match answers.recv_async().await {
                        Ok(AskOutcome::Elicitation(fields)) => ElicitationAction::Accept(
                            ElicitationAcceptAction::new().content(elicitation_content(&fields)),
                        ),
                        _ => ElicitationAction::Cancel,
                    };
                    if let Err(err) = responder.respond(CreateElicitationResponse::new(action)) {
                        log::warn!("engine: elicitation response failed: {err}");
                    }
                    Ok(())
                })?;
                Ok(())
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReadTextFileRequest, responder, _cx| {
                match read_text_file(&request) {
                    Ok(content) => responder.respond(ReadTextFileResponse::new(content)),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |request: WriteTextFileRequest, responder, _cx| {
                match write_text_file(&request) {
                    Ok(()) => responder.respond(WriteTextFileResponse::new()),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            on_receive_request!(),
        )
        .connect_with(adapter, async move |cx| {
            let ctx = main_ctx;
            ctx.phase(EnginePhase::Connecting);
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(client_capabilities()),
            )
            .block_task()
            .await?;

            let cwd = ctx.run.worktree.clone();
            let (session_id, modes, options, meta) = match &ctx.resume {
                // A recorded ACP session re-enters through `session/load`;
                // every other resume shape is the ADAPTER's business (claude
                // `--resume=`, codex `thread/resume`, pi `--session`) and
                // arrives here as a plain `session/new`.
                Some(ResumeHandle::Acp(id)) => {
                    let session_id = SessionId::new(id.clone());
                    let response = cx
                        .send_request(LoadSessionRequest::new(session_id.clone(), cwd.clone()))
                        .block_task()
                        .await?;
                    (
                        session_id,
                        response.modes.clone(),
                        response.config_options.clone(),
                        response.meta.clone(),
                    )
                }
                _ => {
                    let response = cx
                        .send_request(NewSessionRequest::new(cwd.clone()))
                        .block_task()
                        .await?;
                    (
                        response.session_id.clone(),
                        response.modes.clone(),
                        response.config_options.clone(),
                        response.meta.clone(),
                    )
                }
            };
            ctx.set_ids(
                Some(session_id.0.to_string()),
                meta.as_ref()
                    .and_then(|meta| meta.get(NATIVE_SESSION_META_KEY))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            );
            crate::lifecycle::record_session_ids(&ctx);

            let mut out = MapOut::default();
            ctx.with_mapper(|mapper| {
                mapper.on_session_state(
                    modes.as_ref(),
                    options.as_deref().unwrap_or_default(),
                    &[],
                    &mut out,
                )
            });
            ctx.dispatch(out);
            ctx.phase(EnginePhase::Live);
            // Between turns from the very first moment: a kill that asks to
            // wait for the turn (EXP-637) must not sit out `STOP_GRACE` on a
            // session that never started one.
            ctx.turn_signal.set_idle(true);

            if ctx.replay {
                // A transcript replay has nothing to steer: `session/load`
                // already streamed its history through the notification
                // handler above. Flush what the coalescers still hold (a
                // replay has no turn end and may finish inside the 250 ms
                // idle window) before the phase closes the feed.
                let mut out = MapOut::default();
                ctx.with_mapper(|mapper| mapper.on_stop(StopReason::EndTurn, &mut out));
                ctx.dispatch(out);
                ctx.phase(EnginePhase::Ended);
                return Ok(());
            }

            // The in-flight turn counter: `idle` only flips back when the
            // LAST turn answers, so a steered second prompt cannot end the
            // turn early.
            let turns = Arc::new(AtomicUsize::new(0));
            if let Some(prompt) = ctx.prompt.clone() {
                start_turn(&cx, &ctx, &session_id, text_blocks(&prompt), &turns);
            }
            loop {
                tokio::select! {
                    command = commands.recv_async() => {
                        let Ok(command) = command else { break };
                        if !handle_command(&cx, &ctx, &session_id, &turns, command) {
                            break;
                        }
                    }
                    // The agent went away (child EOF, a crash): the end
                    // sequence turns the recorded child exit into
                    // `exit:<code>`.
                    () = cx.incoming_closed() => break,
                    () = tokio::time::sleep(FLUSH_IDLE) => {
                        let mut out = MapOut::default();
                        ctx.with_mapper(|mapper| mapper.flush(&mut out));
                        ctx.dispatch(out);
                    }
                }
            }
            ctx.asks.cancel_all();
            Ok(())
        })
        .await
}

/// Returns `false` when the loop must stop.
fn handle_command(
    cx: &agent_client_protocol::ConnectionTo<agent_client_protocol::Agent>,
    ctx: &Arc<SessionCtx>,
    session_id: &SessionId,
    turns: &Arc<AtomicUsize>,
    command: EngineCommand,
) -> bool {
    match command {
        EngineCommand::Prompt(blocks) => start_turn(cx, ctx, session_id, blocks, turns),
        EngineCommand::Steer(text) => {
            if turns.load(Ordering::SeqCst) == 0 {
                start_turn(cx, ctx, session_id, text_blocks(&text), turns);
            } else {
                // Mid-turn steering: a `session/prompt` that arrives while a
                // turn is running IS the steer seam — the adapter folds it
                // into the live turn (codex `turn/steer`, claude's queued
                // user message), so its response is not a turn end and is
                // deliberately detached.
                announce_prompt(ctx, &text);
                cx.send_request(PromptRequest::new(session_id.clone(), text_blocks(&text)))
                    .detach();
            }
        }
        EngineCommand::Cancel => {
            let _ = cx.send_notification(CancelNotification::new(session_id.clone()));
            ctx.asks.cancel_all();
            let mut out = MapOut::default();
            ctx.with_mapper(|mapper| mapper.on_cancel(&mut out));
            ctx.dispatch(out);
        }
        EngineCommand::SetConfig { id, value } => {
            let sent = cx.send_request(SetSessionConfigOptionRequest::new(
                session_id.clone(),
                id,
                value,
            ));
            let ctx = ctx.clone();
            let _ = cx.spawn(async move {
                let mut out = MapOut::default();
                match sent.block_task().await {
                    // The re-emitted `config_state` IS the confirmation (D4).
                    Ok(response) => ctx.with_mapper(|mapper| {
                        mapper.on_session_state(None, &response.config_options, &[], &mut out)
                    }),
                    Err(err) => {
                        ctx.with_mapper(|mapper| mapper.on_error(&err.to_string(), &mut out))
                    }
                }
                ctx.dispatch(out);
                Ok(())
            });
        }
        EngineCommand::SetMode(mode_id) => {
            let sent = cx.send_request(SetSessionModeRequest::new(
                session_id.clone(),
                mode_id.clone(),
            ));
            let ctx = ctx.clone();
            let _ = cx.spawn(async move {
                let mut out = MapOut::default();
                match sent.block_task().await {
                    // `set_mode` answers with nothing, so the engine mirrors
                    // the mode itself; the agent's own `CurrentModeUpdate`
                    // (when it sends one) is then a no-op re-emit.
                    Ok(_) => ctx.with_mapper(|mapper| {
                        mapper.set_current_mode(&mode_id.0, &mut out);
                    }),
                    Err(err) => {
                        ctx.with_mapper(|mapper| mapper.on_error(&err.to_string(), &mut out))
                    }
                }
                ctx.dispatch(out);
                Ok(())
            });
        }
        EngineCommand::Answer(answer) => {
            let mut out = MapOut::default();
            let (key, decision) = ctx.with_mapper(|mapper| {
                let key = mapper.ask_key(&answer);
                let decision = mapper.on_answer(&key, &answer, &mut out);
                (key, decision)
            });
            ctx.dispatch(out);
            match decision {
                AnswerDecision::Permission { option_id } => ctx
                    .asks
                    .resolve(key.responder_key(), AskOutcome::Permission(option_id)),
                AnswerDecision::Elicitation {
                    fields,
                    submit: true,
                } => ctx
                    .asks
                    .resolve(key.responder_key(), AskOutcome::Elicitation(fields)),
                // A mid-stepper answer published the next step and resolves
                // nothing; a re-tap re-acked; an unknown id was dropped.
                _ => {}
            }
        }
        EngineCommand::Command { name, args } => {
            // An agent-advertised command is prompt text; the contract ones
            // never reach the engine (steer's `CommandLink` owns those).
            let text = if args.is_empty() {
                format!("/{name}")
            } else {
                format!("/{name} {args}")
            };
            start_turn(cx, ctx, session_id, text_blocks(&text), turns);
        }
        EngineCommand::LoadHistory => {
            let sent = cx.send_request(LoadSessionRequest::new(
                session_id.clone(),
                ctx.run.worktree.clone(),
            ));
            let ctx = ctx.clone();
            let _ = cx.spawn(async move {
                if let Err(err) = sent.block_task().await {
                    let mut out = MapOut::default();
                    ctx.with_mapper(|mapper| mapper.on_error(&err.to_string(), &mut out));
                    ctx.dispatch(out);
                }
                Ok(())
            });
        }
        EngineCommand::Shutdown { outcome } => {
            ctx.set_outcome(outcome);
            return false;
        }
    }
    true
}

/// Send one `session/prompt` as a TURN: spawned (never inline) so a `Cancel`
/// arriving mid-turn is still dispatched, with the stop reason folded back
/// through the mapper when the last in-flight turn answers.
fn start_turn(
    cx: &agent_client_protocol::ConnectionTo<agent_client_protocol::Agent>,
    ctx: &Arc<SessionCtx>,
    session_id: &SessionId,
    blocks: Vec<ContentBlock>,
    turns: &Arc<AtomicUsize>,
) {
    announce_prompt(ctx, &blocks_text(&blocks));
    let sent = cx.send_request(PromptRequest::new(session_id.clone(), blocks));
    turns.fetch_add(1, Ordering::SeqCst);
    ctx.turn_signal.set_idle(false);
    let ctx = ctx.clone();
    let turns = turns.clone();
    let _ = cx.spawn(async move {
        let result = sent.block_task().await;
        let remaining = turns.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
        let mut out = MapOut::default();
        match result {
            Ok(response) => {
                if remaining == 0 {
                    ctx.with_mapper(|mapper| mapper.on_stop(response.stop_reason, &mut out));
                }
            }
            Err(err) => {
                ctx.with_mapper(|mapper| mapper.on_error(&err.to_string(), &mut out));
                if remaining == 0 {
                    ctx.with_mapper(|mapper| mapper.on_stop(StopReason::EndTurn, &mut out));
                }
            }
        }
        ctx.dispatch(out);
        Ok(())
    });
}

/// Publish what the host is about to send as the user's own message (the
/// PTY path echoed typed input; an agent that never replays it, codex, would
/// otherwise leave the transcript without the question the reply answers).
fn announce_prompt(ctx: &Arc<SessionCtx>, text: &str) {
    let mut out = MapOut::default();
    ctx.with_mapper(|mapper| mapper.on_prompt(text, &mut out));
    ctx.dispatch(out);
}

/// The text of a prompt's blocks, for the user's own message row.
fn blocks_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The elicitation's id: its ACP scope where it has one, else a stable
/// stand-in the mapper hashes.
fn elicitation_id(request: &CreateElicitationRequest) -> String {
    use agent_client_protocol::schema::v1::{ElicitationMode, ElicitationScope};
    let scope = match &request.mode {
        ElicitationMode::Form(form) => Some(&form.scope),
        ElicitationMode::Url(url) => Some(&url.scope),
        _ => None,
    };
    match scope {
        Some(ElicitationScope::Request(scope)) => format!("{:?}", scope.request_id),
        _ => request
            .meta
            .as_ref()
            .and_then(|meta| meta.get("askId"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// The accumulated stepper fields in `elicitation/create`'s own shape.
fn elicitation_content(
    fields: &serde_json::Value,
) -> std::collections::BTreeMap<String, ElicitationContentValue> {
    let mut content = std::collections::BTreeMap::new();
    let Some(object) = fields.as_object() else {
        return content;
    };
    for (key, value) in object {
        let value = match value {
            serde_json::Value::String(text) => ElicitationContentValue::String(text.clone()),
            serde_json::Value::Bool(flag) => ElicitationContentValue::Boolean(*flag),
            serde_json::Value::Number(number) => match number.as_i64() {
                Some(integer) => ElicitationContentValue::Integer(integer),
                None => ElicitationContentValue::Number(number.as_f64().unwrap_or_default()),
            },
            serde_json::Value::Array(values) => ElicitationContentValue::StringArray(
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect(),
            ),
            // Null (an unparseable number step) is simply not sent.
            _ => continue,
        };
        content.insert(key.clone(), value);
    }
    content
}

fn read_text_file(request: &ReadTextFileRequest) -> Result<String, std::io::Error> {
    let content = std::fs::read_to_string(&request.path)?;
    if request.line.is_none() && request.limit.is_none() {
        return Ok(content);
    }
    // `line` is 1-based, `limit` a line count — the same window the agent
    // would have read itself.
    let start = request.line.unwrap_or(1).saturating_sub(1) as usize;
    let lines: Vec<&str> = content.lines().skip(start).collect();
    let lines = match request.limit {
        Some(limit) => lines.into_iter().take(limit as usize).collect::<Vec<_>>(),
        None => lines,
    };
    Ok(lines.join("\n"))
}

fn write_text_file(request: &WriteTextFileRequest) -> Result<(), std::io::Error> {
    if let Some(parent) = request.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&request.path, &request.content)
}

/// The engine thread's name: `acp-engine-<sid8>`.
pub(crate) fn thread_name(session_id: &str) -> String {
    let short: String = session_id.chars().take(8).collect();
    format!("acp-engine-{short}")
}

/// The worktree's steer image directory (EXP-511 embeds land here).
pub(crate) fn steer_images_dir(worktree: &std::path::Path) -> PathBuf {
    worktree.join(coding::launcher::STEER_IMAGES_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_advertises_no_terminal_and_a_form_elicitation() {
        let capabilities = client_capabilities();
        assert!(capabilities.fs.read_text_file);
        assert!(capabilities.fs.write_text_file);
        // D5: command output is a local card, never a live terminal.
        assert!(!capabilities.terminal);
        assert!(capabilities
            .elicitation
            .as_ref()
            .is_some_and(|elicitation| elicitation.supports_form()));
    }

    #[test]
    fn dropping_the_kill_feed_unwatches() {
        let unwatched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = unwatched.clone();
        let (_tx, rx) = flume::unbounded::<KillReason>();
        drop(KillFeed {
            rx,
            unwatch: Some(Box::new(move || {
                flag.store(true, std::sync::atomic::Ordering::SeqCst)
            })),
        });
        assert!(unwatched.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn the_engine_thread_is_named_after_the_session() {
        assert_eq!(
            thread_name("0f8c2a11-4d3e-4f0a-9c1d-8a7b6c5d4e3f"),
            "acp-engine-0f8c2a11"
        );
    }

    #[test]
    fn a_child_exit_becomes_the_bye_outcome() {
        let link = ChildExitLink::new();
        assert_eq!(link.get(), None);
        link.record(terminal::pty::ChildExit {
            code: 3,
            success: false,
            signal: None,
        });
        // A second report never overwrites the first.
        link.record(terminal::pty::ChildExit {
            code: 0,
            success: true,
            signal: None,
        });
        assert_eq!(link.get().map(|exit| exit.code), Some(3));
    }

    #[tokio::test]
    async fn a_recorded_child_exit_wakes_waiters_before_and_after_it() {
        let link = ChildExitLink::new();
        // Nothing yet: an adapter parked here keeps the connection open.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), link.reaped())
                .await
                .is_err()
        );
        let waiting = link.clone();
        let parked = tokio::spawn(async move { waiting.reaped().await });
        link.record(terminal::pty::ChildExit {
            code: 3,
            success: false,
            signal: None,
        });
        tokio::time::timeout(Duration::from_secs(5), parked)
            .await
            .expect("the waiter parked before the exit is woken")
            .expect("the waiter did not panic");
        // And the edge is memoryful: an adapter that asks AFTER the child died
        // must not park forever on an event it missed.
        tokio::time::timeout(Duration::from_secs(5), link.reaped())
            .await
            .expect("a late waiter resolves immediately");
    }

    #[test]
    fn elicitation_content_carries_every_step_shape() {
        let fields = serde_json::json!({
            "name": "exp",
            "confirm": true,
            "count": 3,
            "tags": ["a", "b"],
            "skipped": null
        });
        let content = elicitation_content(&fields);
        assert_eq!(
            content.get("name"),
            Some(&ElicitationContentValue::String("exp".to_string()))
        );
        assert_eq!(
            content.get("confirm"),
            Some(&ElicitationContentValue::Boolean(true))
        );
        assert_eq!(
            content.get("count"),
            Some(&ElicitationContentValue::Integer(3))
        );
        assert_eq!(
            content.get("tags"),
            Some(&ElicitationContentValue::StringArray(vec![
                "a".to_string(),
                "b".to_string()
            ]))
        );
        assert!(!content.contains_key("skipped"));
    }
}
