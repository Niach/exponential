//! EXP-746 — CodexAgent: a LONG-LIVED `codex app-server` connection presented
//! as an ACP agent. Owned by lane E3.
//!
//! Shape: `initialize` (with `optOutNotificationMethods` for the ~30
//! notifications we would parse and drop, `process/outputDelta` and
//! `command/exec/outputDelta` first) → `initialized` → `thread/start` whose
//! `config` carries the MCP server, the project trust level and the writable
//! roots — that object REPLACES today's `-c mcp_servers.*` argv — then one
//! `turn/start` per prompt with the `agent-full-access` preset (D3).
//!
//! Non-obvious invariants, each measured or read out of codex-acp:
//!
//! - **The stale fence is mandatory.** `turn/interrupt` returns `Ok` and three
//!   more `item/agentMessage/delta` notifications still arrived afterwards in
//!   the phase-1 spike. Cancel therefore marks the turn stale FIRST, then
//!   interrupts, then drops every later notification bearing that turn id and
//!   auto-answers every in-flight approval `"cancel"`.
//! - **`item/fileChange` `add`/`delete` carry the FULL FILE CONTENT** in
//!   `diff`; only `update` is a real unified diff, and its hunks are anchored
//!   against the file as it is on disk.
//! - **Approvals are server→client REQUESTS.** Dropping one hangs codex
//!   forever with no symptom, which is why the transport classifies them
//!   separately ([`codex_wire::classify_line`]).
//! - **Every enum over methods, item types, effort and service tier needs a
//!   catch-all**: the installed codex (0.144.5) is older than the one
//!   codex-acp targets (0.153.3), and both directions of skew are live.
//! - **Coalescing is a correctness requirement, not an optimization**: one
//!   20-second codex turn produced 911 notifications in the spike. The engine
//!   mapper coalesces; the adapter's job is to not add to the pile (hence the
//!   opt-out list).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CreateElicitationRequest, ElicitationAction, ElicitationContentValue,
    ElicitationFormMode, ElicitationSchema, ElicitationSessionScope, StringPropertySchema, AvailableCommand, AvailableCommandsUpdate, CancelNotification,
    CompactionId, CompactionStatus, CompactionUpdate, ContentBlock, ContentChunk, CurrentModeUpdate, Diff,
    InitializeRequest, InitializeResponse, ListSessionsRequest, ListSessionsResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus,
    PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, SessionCapabilities, SessionConfigOption, SessionConfigOptionCategory,
    PermissionOptionId, SessionConfigOptionValue, SessionConfigSelectOption,
    SessionConfigValueId, SessionId,
    SessionInfo, SessionListCapabilities, SessionMode, SessionModeId, SessionModeState,
    SessionNotification, SessionResumeCapabilities, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse, StopReason,
    TextContent, ToolCall, ToolCallContent, ToolCallId, ToolCallLocation, ToolCallStatus,
    ToolCallUpdate,
    ToolCallUpdateFields, ToolKind, UsageUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, Error};
use serde_json::{json, Value};
use tokio::sync::Notify;

use super::codex_wire::{
    self, ApprovalKind, AppServer, CodexMode, ServerRequest, TurnRequest, CODEX_MODES,
};
use super::AdapterSpec;
use crate::session::{EngineError, ResumeHandle};

/// How long a single app-server call may take before the adapter gives up on
/// it. Generous on purpose: an unreachable MCP endpoint delayed one CLI's
/// handshake by 25 seconds in the spike, and a slow start is not a dead CLI.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// `/compact` has no response of its own — completion arrives as a separate
/// notification, so the wait needs its own bound. Mirrors the clients' shared
/// `COMPACTION_TIMEOUT`.
const COMPACTION_TIMEOUT: Duration = Duration::from_secs(180);

/// The live rate-limit windows codex pushes over `account/rateLimits/updated`,
/// parsed into the SAME shape the desktop usage sheet reads.
///
/// EXP-754: every store also PUBLISHES into `coding::agent_usage::live`, and
/// the handle holds that registry's session guard for the run. One machine
/// runs one app-server per session, so the usage poller reads these numbers
/// instead of spawning a second one of its own. Clone the handle off the
/// agent BEFORE it is moved into `Adapter::Codex` and the host can read it
/// whenever it likes.
#[derive(Clone)]
pub struct CodexUsage(Arc<UsageSlot>);

struct UsageSlot {
    windows: Mutex<Vec<coding::agent_usage::UsageWindow>>,
    /// The live-registry attachment, released by [`CodexUsage::detach`] at
    /// session end — or, if nobody gets there, by dropping the last handle.
    attached: Mutex<Option<coding::agent_usage::live::Attached>>,
}

impl CodexUsage {
    /// Register a live codex session with the machine's usage registry.
    pub fn attach() -> CodexUsage {
        CodexUsage(Arc::new(UsageSlot {
            windows: Mutex::new(Vec::new()),
            attached: Mutex::new(Some(coding::agent_usage::live::attach(
                coding::CodingAgent::Codex,
            ))),
        }))
    }

    pub fn windows(&self) -> Vec<coding::agent_usage::UsageWindow> {
        match self.0.windows.lock() {
            Ok(windows) => windows.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The session ended: release the registry slot NOW rather than whenever
    /// the last handle happens to drop (the host keeps one for the run's
    /// lifetime). Idempotent.
    fn detach(&self) {
        let attached = match self.0.attached.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(attached);
    }

    fn store(&self, windows: Vec<coding::agent_usage::UsageWindow>) {
        let published = windows.clone();
        match self.0.windows.lock() {
            Ok(mut slot) => *slot = windows,
            Err(poisoned) => *poisoned.into_inner() = windows,
        }
        // EXP-754: the machine's usage poller reads THIS instead of spawning
        // a second `codex app-server` to ask the same account again.
        coding::agent_usage::live::publish(coding::CodingAgent::Codex, published);
    }
}

/// A live app-server: the request half plus the two inbound streams. Split out
/// so the adapter tests can drive an in-process fake through the same code
/// path the child process takes.
pub struct CodexConnection {
    pub server: Arc<AppServer>,
    pub notifications: flume::Receiver<(String, Value)>,
    pub requests: flume::Receiver<ServerRequest>,
    pub exit: Option<flume::Receiver<terminal::pty::ChildExit>>,
    /// EXP-758: the app-server child's pid (`None` on the in-process test
    /// fake) — recorded into the run record for the orphan reaper.
    pub pid: Option<u32>,
}

impl CodexConnection {
    /// Spawn `codex app-server --listen stdio://` from the launch's own
    /// `SpawnSpec`, so the ACP child inherits the exact program, cwd and env
    /// (`EXP_MCP_TOKEN` included) the PTY launch would have used.
    pub fn spawn(spec: &terminal::pty::SpawnSpec) -> std::io::Result<CodexConnection> {
        let mut spec = spec.clone();
        spec.args = vec![
            "app-server".to_string(),
            "--listen".to_string(),
            "stdio://".to_string(),
        ];
        let (server, notifications, requests, exit, pid) = AppServer::spawn(&spec)?;
        Ok(CodexConnection {
            server,
            notifications,
            requests,
            exit: Some(exit),
            pid: Some(pid),
        })
    }
}

pub struct CodexAgent {
    spec: AdapterSpec,
    connection: CodexConnection,
    usage: CodexUsage,
}

impl CodexAgent {
    pub fn new(spec: AdapterSpec) -> Result<CodexAgent, EngineError> {
        let connection = CodexConnection::spawn(&spec.spawn).map_err(EngineError::Spawn)?;
        if let Some(pid) = connection.pid {
            spec.exit.record_pid(pid);
        }
        if let Some(exit) = &connection.exit {
            crate::transport::forward_exit(exit.clone(), spec.exit.clone());
        }
        Ok(CodexAgent {
            spec,
            connection,
            usage: CodexUsage::attach(),
        })
    }

    /// Drive an already-connected app-server (the adapter tests).
    pub fn with_connection(spec: AdapterSpec, connection: CodexConnection) -> CodexAgent {
        CodexAgent {
            spec,
            connection,
            usage: CodexUsage::attach(),
        }
    }

    /// The live rate-limit snapshot. Clone it before the agent is moved into
    /// the `Adapter` enum; the handle stays valid for the run.
    pub fn usage(&self) -> CodexUsage {
        self.usage.clone()
    }
}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// What one turn ended as. `Interrupted` is a cancel (ours or codex's own),
/// `Failed` a `turn/failed` or a dead app-server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnOutcome {
    Completed,
    Interrupted,
    Failed,
}

impl TurnOutcome {
    fn stop_reason(self) -> StopReason {
        match self {
            TurnOutcome::Completed => StopReason::EndTurn,
            TurnOutcome::Interrupted => StopReason::Cancelled,
            // EXP-758: `EndTurn`, not `Refusal`. ACP gives a turn five
            // terminal reasons and none of them means "it broke": `Refusal`
            // is the MODEL declining, and the schema attaches a behaviour to
            // it (the prompt and everything after it drops out of the next
            // one), which is a lie about a `turn/failed` or a dead
            // app-server, and codex keeps that history. `Cancelled` is reserved
            // for `session/cancel` by the spec. `EndTurn` is the honest
            // remainder: the turn is over, and the reason already reached the
            // feed as an `error` notification.
            TurnOutcome::Failed => StopReason::EndTurn,
        }
    }
}

#[derive(Default)]
struct Turns {
    /// The turn a prompt is currently riding, if any.
    live: Option<String>,
    /// `turn/steer` supersedes a turn with a NEW id; a waiter parked on the
    /// old one follows this chain to the tail.
    successor: HashMap<String, String>,
    outcomes: HashMap<String, TurnOutcome>,
    /// Cancelled turns. Everything they emit afterwards is dropped.
    stale: HashSet<String>,
}

impl Turns {
    /// The outcome of `turn_id` or of whatever superseded it.
    fn outcome(&self, turn_id: &str) -> Option<TurnOutcome> {
        let mut id = turn_id.to_string();
        for _ in 0..16 {
            if let Some(outcome) = self.outcomes.get(&id) {
                return Some(*outcome);
            }
            match self.successor.get(&id) {
                Some(next) => id = next.clone(),
                None => return None,
            }
        }
        None
    }
}

/// The model/effort/mode picture the config chips render.
#[derive(Default)]
struct Config {
    models: Vec<ModelInfo>,
    model: String,
    effort: Option<String>,
    fast: bool,
    mode: CodexMode,
    collaboration: String,
}

#[derive(Clone, Debug, Default)]
struct ModelInfo {
    id: String,
    model: String,
    display: String,
    efforts: Vec<String>,
    default_effort: Option<String>,
    supports_fast: bool,
}

struct Shared {
    spec: AdapterSpec,
    server: Arc<AppServer>,
    session_id: Mutex<Option<SessionId>>,
    cwd: Mutex<PathBuf>,
    roots: Mutex<Vec<PathBuf>>,
    turns: Mutex<Turns>,
    wake: Notify,
    config: Mutex<Config>,
    /// Item id → what we surfaced it as, so an update can patch the right card
    /// and a delta knows whether its item ever produced one.
    items: Items,
    /// Server-request ids already answered (a cancel races the client's own
    /// outcome; whoever gets there first wins and the other is dropped).
    answered: Mutex<HashSet<String>>,
    /// Approvals waiting on a person. A cancel answers them itself rather than
    /// leaving codex holding a request whose turn is already gone.
    in_flight: Mutex<HashMap<String, ServerRequest>>,
    /// `error` notifications that arrived before `turn/started` — codex sends
    /// them with no turn to attach to, and flushing them at turn start is what
    /// keeps the first error of a turn from vanishing.
    pending_errors: Mutex<Vec<String>>,
    compaction: Mutex<Option<String>>,
    /// Turns whose compaction already reached the feed as a `contextCompaction`
    /// item, so the `thread/compacted` notification for the SAME compaction
    /// does not draw a second strip.
    compacted: Mutex<HashSet<String>>,
    usage: CodexUsage,
    closed: AtomicBool,
    /// [`start_pumps`] already ran. Both entry points call it — `thread/start`
    /// and a resuming `session/load` — and the receivers they hold are clones
    /// of ONE queue, so a second pump would steal half the frames.
    pumping: AtomicBool,
}

/// The item table. A value of its own so the whole notification → update
/// mapping is a function of (items, method, params) and can be unit-tested
/// without an app-server.
#[derive(Default)]
struct Items(Mutex<HashMap<String, ItemView>>);

impl Items {
    fn seen_deltas(&self, id: &str) -> bool {
        self.0
            .lock()
            .map(|items| items.get(id).is_some_and(|view| view.deltas))
            .unwrap_or(false)
    }

    fn mark_delta(&self, id: &str) {
        if let Ok(mut items) = self.0.lock() {
            items.entry(id.to_string()).or_insert_with(ItemView::message).deltas = true;
        }
    }

    fn has_card(&self, id: &str) -> bool {
        self.0
            .lock()
            .map(|items| items.get(id).is_some_and(|view| view.tool))
            .unwrap_or(false)
    }

    fn add_card(&self, id: &str) {
        if let Ok(mut items) = self.0.lock() {
            items.insert(id.to_string(), ItemView::card());
        }
    }
}

#[derive(Clone, Debug)]
struct ItemView {
    /// Whether a `tool_call` was published for this item at all (an agent
    /// message or a reasoning block produces none).
    tool: bool,
    /// Whether any delta arrived, so a completion knows not to repeat the text.
    deltas: bool,
}

impl Shared {
    fn session_id(&self) -> Option<SessionId> {
        self.session_id.lock().ok().and_then(|id| id.clone())
    }

    fn is_stale(&self, turn_id: &str) -> bool {
        self.turns
            .lock()
            .map(|turns| turns.stale.contains(turn_id))
            .unwrap_or(false)
    }

    /// Resolves once the app-server's stdout has ended ([`Shared::mark_closed`])
    /// — immediately, if it already has. The connection's `main_fn` waits on
    /// this: a codex that died must close the ACP connection, not leave the
    /// host's loop waiting on an agent that will never speak again.
    async fn child_gone(&self) {
        loop {
            // Registered BEFORE the check, like every other waiter on `wake`:
            // a `mark_closed` between the two must still wake this one.
            let notified = self.wake.notified();
            if self.closed.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(mut turns) = self.turns.lock() {
            let live: Vec<String> = turns.live.iter().cloned().collect();
            for id in live {
                turns.outcomes.entry(id).or_insert(TurnOutcome::Failed);
            }
            turns.live = None;
        }
        self.wake.notify_waiters();
    }

    fn writable_roots(&self) -> Vec<PathBuf> {
        self.roots.lock().map(|roots| roots.clone()).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// The connection
// ---------------------------------------------------------------------------

impl ConnectTo<Client> for CodexAgent {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        async move {
            let CodexAgent {
                spec,
                connection,
                usage,
            } = self;
            let cwd = spec.cwd.clone();
            let shared = Arc::new(Shared {
                spec,
                server: connection.server.clone(),
                session_id: Mutex::new(None),
                cwd: Mutex::new(cwd.clone()),
                roots: Mutex::new(vec![cwd]),
                turns: Mutex::new(Turns::default()),
                wake: Notify::new(),
                config: Mutex::new(Config::default()),
                items: Items::default(),
                answered: Mutex::new(HashSet::new()),
                in_flight: Mutex::new(HashMap::new()),
                pending_errors: Mutex::new(Vec::new()),
                compaction: Mutex::new(None),
                compacted: Mutex::new(HashSet::new()),
                usage,
                closed: AtomicBool::new(false),
                pumping: AtomicBool::new(false),
            });
            let notifications = connection.notifications.clone();
            let requests = connection.requests.clone();
            // The SAME queue, for the other entry point: a run that resumes
            // arrives as `session/load`, never `session/new`, and needs the
            // pumps just as much ([`load_thread`]).
            let load_notifications = notifications.clone();
            let load_requests = requests.clone();

            let main_shared = shared.clone();
            let init = shared.clone();
            let new_session = shared.clone();
            let load = shared.clone();
            let list = shared.clone();
            let prompt = shared.clone();
            let cancel = shared.clone();
            let set_mode = shared.clone();
            let set_config = shared.clone();

            Agent
                .builder()
                .name("codex-app-server")
                .on_receive_request(
                    async move |request: InitializeRequest, responder, _cx| {
                        let shared = init.clone();
                        match handshake(&shared).await {
                            Ok(()) => responder.respond(initialize_response(&request)),
                            Err(error) => responder.respond_with_error(error),
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: NewSessionRequest, responder, cx: ConnectionTo<Client>| {
                        // EXP-758: SPAWNED, like the prompt handler. This is
                        // `thread/start` plus up to eight `model/list` pages,
                        // each with the 120 s `CALL_TIMEOUT` behind it, and
                        // inline every one of those seconds is a second in
                        // which `session/cancel` cannot be dispatched.
                        let shared = new_session.clone();
                        let notifications = notifications.clone();
                        let requests = requests.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            let opened =
                                open_thread(&shared, &spawned, request, notifications, requests)
                                    .await;
                            let _ = match opened {
                                Ok(response) => {
                                    let sent = responder.respond(response);
                                    publish_commands(&shared, &spawned);
                                    sent
                                }
                                Err(error) => responder.respond_with_error(error),
                            };
                            // NEVER `Err`: a spawned task that fails takes the
                            // whole connection down with it.
                            Ok(())
                        })
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: LoadSessionRequest, responder, cx: ConnectionTo<Client>| {
                        // SPAWNED for the same reason `session/new` is: a
                        // resuming load is `thread/resume` plus the config
                        // seed's `model/list` pages, each with `CALL_TIMEOUT`
                        // behind it, and inline none of those seconds can
                        // dispatch a `session/cancel`.
                        let shared = load.clone();
                        let notifications = load_notifications.clone();
                        let requests = load_requests.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            let loaded =
                                load_thread(&shared, &spawned, request, notifications, requests)
                                    .await;
                            let _ = match loaded {
                                Ok(response) => {
                                    let sent = responder.respond(response);
                                    if !shared.spec.replay {
                                        publish_commands(&shared, &spawned);
                                    }
                                    sent
                                }
                                Err(error) => responder.respond_with_error(error),
                            };
                            // NEVER `Err`: a spawned task that fails takes the
                            // whole connection down with it.
                            Ok(())
                        })
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: ListSessionsRequest, responder, _cx| {
                        let shared = list.clone();
                        match list_threads(&shared, request).await {
                            Ok(response) => responder.respond(response),
                            Err(error) => responder.respond_with_error(error),
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: PromptRequest, responder, cx: ConnectionTo<Client>| {
                        // SPAWNED, never inline: a handler blocks every later
                        // message until it returns, and `session/cancel` is a
                        // later message. Inline, a turn could never be
                        // cancelled.
                        let shared = prompt.clone();
                        cx.spawn(async move {
                            let outcome = run_prompt(&shared, request).await;
                            let _ = match outcome {
                                Ok(stop) => responder.respond(PromptResponse::new(stop)),
                                Err(error) => responder.respond_with_error(error),
                            };
                            // NEVER `Err`: a spawned task that fails takes the
                            // whole connection down with it.
                            Ok(())
                        })
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionModeRequest, responder, cx: ConnectionTo<Client>| {
                        let shared = set_mode.clone();
                        let Some(mode) = CodexMode::parse(request.mode_id.0.as_ref()) else {
                            return responder.respond_with_error(Error::invalid_params());
                        };
                        if let Ok(mut config) = shared.config.lock() {
                            config.mode = mode;
                        }
                        let sent = responder.respond(SetSessionModeResponse::new());
                        // The response carries nothing, so the chip only moves
                        // once the mode is echoed back as an update.
                        if let Some(session_id) = shared.session_id() {
                            let _ = cx.send_notification(SessionNotification::new(
                                session_id,
                                SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(
                                    SessionModeId::new(mode.id()),
                                )),
                            ));
                        }
                        sent
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionConfigOptionRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        // EXP-758: SPAWNED. `thread/settings/update` is an
                        // app-server round trip, and a slow one held every
                        // later message, `session/cancel` included.
                        let shared = set_config.clone();
                        cx.spawn(async move {
                            apply_config_option(&shared, &request).await;
                            let _ = responder.respond(SetSessionConfigOptionResponse::new(
                                config_options(&shared),
                            ));
                            Ok(())
                        })
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_notification(
                    async move |_notification: CancelNotification, cx: ConnectionTo<Client>| {
                        // The FENCE first, synchronously: everything after this
                        // line is allowed to be late.
                        let shared = cancel.clone();
                        let live = mark_stale(&shared);
                        let spawned = shared.clone();
                        let _ = cx.spawn(async move {
                            interrupt(&spawned, live).await;
                            Ok(())
                        });
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                // NOT `connect_to`: its main_fn waits only on the client, and
                // the host's loop waits only on us, so an app-server that died
                // would strand both halves (EXP-746 review E1). Its stdout
                // ending closes the connection here.
                .connect_with(client, async move |cx: ConnectionTo<Client>| {
                    let shared = main_shared;
                    crate::host::until_either_closes(
                        &cx,
                        &shared.spec.exit,
                        shared.child_gone(),
                    )
                    .await;
                    // EXP-754: the session is over, so it stops answering for
                    // this machine's usage numbers (its `Drop` is the backstop
                    // for every path that never reaches this line).
                    shared.usage.detach();
                    Ok(())
                })
                .await
        }
    }
}

fn internal(message: impl std::fmt::Display) -> Error {
    Error::internal_error().data(Value::String(message.to_string()))
}

/// One app-server call with the shared deadline. A timeout is an error, never
/// a silent `None`: a wedged call must surface, not look like an empty answer.
async fn call(shared: &Shared, method: &str, params: Value) -> Result<Value, Error> {
    match tokio::time::timeout(CALL_TIMEOUT, shared.server.request(method, params)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(internal(format!("codex {method}: {error}"))),
        Err(_) => Err(internal(format!("codex {method} timed out"))),
    }
}

async fn handshake(shared: &Shared) -> Result<(), Error> {
    let version = env!("CARGO_PKG_VERSION");
    call(
        shared,
        "initialize",
        codex_wire::initialize_params(version, codex_wire::OPT_OUT_NOTIFICATIONS),
    )
    .await?;
    // Mandatory: the app-server answers nothing else until it lands.
    shared
        .server
        .notify("initialized", json!({}))
        .map_err(internal)
}

fn initialize_response(request: &InitializeRequest) -> InitializeResponse {
    InitializeResponse::new(request.protocol_version).agent_capabilities(
        AgentCapabilities::new()
            .load_session(true)
            .prompt_capabilities(
                PromptCapabilities::new().image(true).embedded_context(true),
            )
            .session_capabilities(
                SessionCapabilities::new()
                    .list(SessionListCapabilities::new())
                    .resume(SessionResumeCapabilities::new()),
            ),
    )
}

// ---------------------------------------------------------------------------
// session/new
// ---------------------------------------------------------------------------

async fn open_thread(
    shared: &Arc<Shared>,
    cx: &ConnectionTo<Client>,
    request: NewSessionRequest,
    notifications: flume::Receiver<(String, Value)>,
    requests: flume::Receiver<ServerRequest>,
) -> Result<NewSessionResponse, Error> {
    let cwd = request.cwd.clone();
    let mut roots = vec![cwd.clone()];
    roots.extend(request.additional_directories.iter().cloned());
    if let Ok(mut slot) = shared.cwd.lock() {
        *slot = cwd.clone();
    }
    if let Ok(mut slot) = shared.roots.lock() {
        *slot = roots.clone();
    }

    let config = thread_config_for(shared, &roots);

    // A recorded thread id re-enters the SAME conversation; anything else
    // starts a fresh one. `Acp` and `Native` are the same string here because
    // the ACP session id IS the codex thread id.
    let resumed = match &shared.spec.resume {
        Some(ResumeHandle::Acp(id)) | Some(ResumeHandle::Native(id)) => Some(id.clone()),
        _ => None,
    };
    // EXP-763: the run playbook, on start AND resume (codex replays the
    // developer message from the rollout; identical text keeps it stable).
    let playbook = Some(coding::skill::RUN_SKILL);
    let response = match &resumed {
        Some(thread_id) => {
            call(
                shared,
                "thread/resume",
                codex_wire::thread_resume_params(thread_id, &cwd, config, playbook),
            )
            .await?
        }
        None => {
            call(
                shared,
                "thread/start",
                codex_wire::thread_start_params(&cwd, config, playbook),
            )
            .await?
        }
    };

    let thread_id = response
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .or_else(|| response.get("threadId").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| internal("codex thread/start returned no thread id"))?;
    let session_id = SessionId::new(thread_id.as_str());
    if let Ok(mut slot) = shared.session_id.lock() {
        *slot = Some(session_id.clone());
    }

    // Only now: the channels buffer from spawn, so nothing said during the
    // handshake is lost by starting the pumps here.
    start_pumps(shared, cx, notifications, requests);

    seed_config(shared, &response).await;

    Ok(NewSessionResponse::new(session_id)
        .modes(mode_state(shared))
        .config_options(config_options(shared)))
}

/// The `thread/start` | `thread/resume` config block: the run's MCP posture
/// plus the roots. Shared by both entry points so a resumed thread gets the
/// SAME tools and sandbox roots a fresh one does.
fn thread_config_for(shared: &Arc<Shared>, roots: &[PathBuf]) -> Value {
    let (mcp_url, mcp_session) = match &shared.spec.mcp {
        coding::AgentMcp::CodexOverrides { url, session_id } => {
            (Some(url.clone()), session_id.clone())
        }
        // The other postures belong to other agents; a codex launch without an
        // MCP block is legal (an agent shell) and just means no tools.
        _ => (None, None),
    };
    codex_wire::thread_config(
        mcp_url.as_deref(),
        mcp_session.as_deref().unwrap_or(&shared.spec.session_id),
        roots,
    )
}

fn start_pumps(
    shared: &Arc<Shared>,
    cx: &ConnectionTo<Client>,
    notifications: flume::Receiver<(String, Value)>,
    requests: flume::Receiver<ServerRequest>,
) {
    if shared.pumping.swap(true, Ordering::SeqCst) {
        return;
    }
    let notification_shared = shared.clone();
    let notification_cx = cx.clone();
    let _ = cx.spawn(async move {
        while let Ok((method, params)) = notifications.recv_async().await {
            on_notification(&notification_shared, &notification_cx, &method, &params);
        }
        notification_shared.mark_closed();
        Ok(())
    });

    let request_shared = shared.clone();
    let request_cx = cx.clone();
    let _ = cx.spawn(async move {
        while let Ok(request) = requests.recv_async().await {
            // One task per approval: an approval waits on a person, and the
            // next one must not queue behind it.
            let shared = request_shared.clone();
            let cx = request_cx.clone();
            let spawned = request_cx.clone();
            let _ = spawned.spawn(async move {
                on_server_request(&shared, &cx, request).await;
                Ok(())
            });
        }
        Ok(())
    });
}

/// The initial model / effort picture, from the thread response plus the
/// (paginated) model list. Every step is best effort: a codex that cannot list
/// models still runs turns, it just shows no model chip.
async fn seed_config(shared: &Arc<Shared>, thread: &Value) {
    let model = thread
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let effort = thread
        .get("reasoningEffort")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut models = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..8 {
        let Ok(page) = call(shared, "model/list", codex_wire::model_list_params(cursor.as_deref()))
            .await
        else {
            break;
        };
        for entry in page.get("data").and_then(Value::as_array).unwrap_or(&Vec::new()) {
            models.push(model_info(entry));
        }
        match page.get("nextCursor").and_then(Value::as_str) {
            Some(next) if !next.is_empty() => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    if let Ok(mut config) = shared.config.lock() {
        config.models = models;
        config.model = model;
        config.effort = effort;
        config.mode = CodexMode::default();
        config.collaboration = "default".to_string();
    }
}

fn model_info(entry: &Value) -> ModelInfo {
    let efforts = entry
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    value
                        .get("reasoningEffort")
                        .and_then(Value::as_str)
                        .or_else(|| value.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    let id = entry
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let model = entry
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(id.as_str())
        .to_string();
    ModelInfo {
        display: entry
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or(model.as_str())
            .to_string(),
        id,
        model,
        efforts,
        default_effort: entry
            .get("defaultReasoningEffort")
            .and_then(Value::as_str)
            .map(str::to_string),
        supports_fast: entry
            .get("additionalSpeedTiers")
            .and_then(Value::as_array)
            .is_some_and(|tiers| tiers.iter().any(|tier| tier.as_str() == Some("fast"))),
    }
}

fn mode_state(shared: &Shared) -> SessionModeState {
    let current = shared
        .config
        .lock()
        .map(|config| config.mode)
        .unwrap_or_default();
    SessionModeState::new(
        SessionModeId::new(current.id()),
        CODEX_MODES
            .into_iter()
            .map(|mode| {
                SessionMode::new(SessionModeId::new(mode.id()), mode.label())
                    .description(mode.description())
            })
            .collect(),
    )
}

/// The chips: model, reasoning effort, fast mode and the collaboration mode.
/// The approval × sandbox presets are ACP MODES, not a config option, so the
/// mode chip has exactly one home (D4).
fn config_options(shared: &Shared) -> Vec<SessionConfigOption> {
    let Ok(config) = shared.config.lock() else {
        return Vec::new();
    };
    let mut options = Vec::new();
    if !config.models.is_empty() {
        options.push(
            SessionConfigOption::select(
                "model",
                "Model",
                SessionConfigValueId::new(config.model.as_str()),
                config
                    .models
                    .iter()
                    .map(|model| {
                        SessionConfigSelectOption::new(
                            SessionConfigValueId::new(model.id.as_str()),
                            model.display.as_str(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .category(SessionConfigOptionCategory::Model),
        );
    }
    let current = config
        .models
        .iter()
        .find(|model| model.id == config.model || model.model == config.model);
    if let Some(model) = current {
        if !model.efforts.is_empty() {
            let value = config
                .effort
                .clone()
                .or_else(|| model.default_effort.clone())
                .unwrap_or_else(|| model.efforts[0].clone());
            options.push(
                SessionConfigOption::select(
                    "reasoning_effort",
                    "Effort",
                    SessionConfigValueId::new(value.as_str()),
                    model
                        .efforts
                        .iter()
                        .map(|effort| {
                            SessionConfigSelectOption::new(
                                SessionConfigValueId::new(effort.as_str()),
                                effort.as_str(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .category(SessionConfigOptionCategory::ThoughtLevel),
            );
        }
        if model.supports_fast {
            options.push(
                SessionConfigOption::boolean("fast-mode", "Fast", config.fast)
                    .category(SessionConfigOptionCategory::ModelConfig),
            );
        }
    }
    options.push(
        SessionConfigOption::select(
            "collaboration_mode",
            "Collaboration",
            SessionConfigValueId::new(config.collaboration.as_str()),
            vec![
                SessionConfigSelectOption::new(SessionConfigValueId::new("default"), "Default"),
                SessionConfigSelectOption::new(SessionConfigValueId::new("plan"), "Plan"),
            ],
        )
        .category(SessionConfigOptionCategory::Other("collaboration_mode".to_string())),
    );
    options
}

async fn apply_config_option(shared: &Arc<Shared>, request: &SetSessionConfigOptionRequest) {
    let id = request.config_id.0.to_string();
    let value = match &request.value {
        SessionConfigOptionValue::Boolean { value, .. } => ConfigPick::Bool(*value),
        SessionConfigOptionValue::ValueId { value, .. } => ConfigPick::Id(value.0.to_string()),
        // A future value shape: keep the current picture rather than guessing.
        _ => return,
    };
    match (id.as_str(), value) {
        ("model", ConfigPick::Id(model)) => {
            if let Ok(mut config) = shared.config.lock() {
                let effort = config
                    .models
                    .iter()
                    .find(|entry| entry.id == model || entry.model == model)
                    .and_then(|entry| {
                        let current = config.effort.clone();
                        current
                            .filter(|effort| entry.efforts.contains(effort))
                            .or_else(|| entry.default_effort.clone())
                    });
                config.model = model;
                config.effort = effort;
            }
        }
        ("reasoning_effort", ConfigPick::Id(effort)) => {
            if let Ok(mut config) = shared.config.lock() {
                config.effort = Some(effort);
            }
        }
        ("fast-mode", ConfigPick::Bool(on)) => {
            if let Ok(mut config) = shared.config.lock() {
                config.fast = on;
            }
        }
        ("fast-mode", ConfigPick::Id(value)) => {
            if let Ok(mut config) = shared.config.lock() {
                config.fast = value == "true" || value == "on";
            }
        }
        ("collaboration_mode", ConfigPick::Id(mode)) => {
            let (thread_id, model, effort) = {
                let session = shared.session_id();
                let config = shared.config.lock().ok();
                (
                    session.map(|id| id.0.to_string()),
                    config.as_ref().map(|config| config.model.clone()),
                    config.as_ref().and_then(|config| config.effort.clone()),
                )
            };
            let Some(thread_id) = thread_id else { return };
            let result = call(
                shared,
                "thread/settings/update",
                codex_wire::thread_settings_collaboration_params(
                    &thread_id,
                    &mode,
                    model.as_deref(),
                    effort.as_deref(),
                ),
            )
            .await;
            match result {
                Ok(_) => {
                    if let Ok(mut config) = shared.config.lock() {
                        config.collaboration = mode;
                    }
                }
                // 0.144.5 has no `thread/settings/update`, so plan mode simply
                // is not available on that codex. Leaving the value where it
                // was makes the chip snap back instead of lying.
                Err(error) => log::warn!("engine: codex collaboration mode unavailable: {error}"),
            }
        }
        _ => {}
    }
}

enum ConfigPick {
    Bool(bool),
    Id(String),
}

/// The one command the adapter can actually execute. Advertising more would
/// mean promising `/review` and `/status`, which the engine has no path for:
/// an agent command it does not implement becomes prompt text, and codex has
/// no idea what to do with a literal `/review`.
fn publish_commands(shared: &Arc<Shared>, cx: &ConnectionTo<Client>) {
    let Some(session_id) = shared.session_id() else { return };
    let _ = cx.send_notification(SessionNotification::new(
        session_id,
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(vec![
            AvailableCommand::new("compact", "Compact the conversation to free up context"),
        ])),
    ));
}

// ---------------------------------------------------------------------------
// Prompts and turns
// ---------------------------------------------------------------------------

/// codex-acp's `buildPromptItems`: ACP content blocks → codex `UserInput`.
/// Audio is dropped (codex takes none); everything else lands as text so a
/// resource still reaches the model.
fn prompt_input(blocks: &[ContentBlock]) -> Vec<Value> {
    let mut input = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => {
                input.push(json!({ "type": "text", "text": text.text, "text_elements": [] }));
            }
            ContentBlock::Image(image) => {
                let url = match &image.uri {
                    Some(uri) => uri.clone(),
                    None => format!("data:{};base64,{}", image.mime_type, image.data),
                };
                input.push(json!({ "type": "image", "url": url }));
            }
            ContentBlock::ResourceLink(link) => {
                input.push(json!({
                    "type": "text",
                    "text": format!("[@{}]({})", link.name, link.uri),
                    "text_elements": [],
                }));
            }
            ContentBlock::Resource(resource) => {
                if let Some(text) = embedded_text(resource) {
                    input.push(json!({ "type": "text", "text": text, "text_elements": [] }));
                }
            }
            _ => {}
        }
    }
    input
}

fn embedded_text(resource: &agent_client_protocol::schema::v1::EmbeddedResource) -> Option<String> {
    match &resource.resource {
        agent_client_protocol::schema::v1::EmbeddedResourceResource::TextResourceContents(text) => {
            Some(format!(
                "<context ref=\"{}\">\n{}\n</context>",
                text.uri, text.text
            ))
        }
        _ => None,
    }
}

fn prompt_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn run_prompt(shared: &Arc<Shared>, request: PromptRequest) -> Result<StopReason, Error> {
    let thread_id = shared
        .session_id()
        .map(|id| id.0.to_string())
        .ok_or_else(|| internal("codex has no thread yet"))?;

    // `/compact` is a command, not a turn: it has its own method and its own
    // completion notification.
    let text = prompt_text(&request.prompt);
    if text.trim() == "/compact" || text.trim().starts_with("/compact ") {
        return compact(shared, &thread_id).await;
    }

    let input = prompt_input(&request.prompt);
    if input.is_empty() {
        return Ok(StopReason::EndTurn);
    }

    let live = shared.turns.lock().ok().and_then(|turns| {
        turns
            .live
            .clone()
            .filter(|id| turns.outcome(id).is_none())
    });

    let turn_id = match live {
        // Mid-turn steering rides `turn/steer`, whose `expectedTurnId` is a
        // PRECONDITION: if the turn ended in between, the call fails and the
        // text starts a fresh turn instead of vanishing.
        Some(previous) => {
            let steered = call(
                shared,
                "turn/steer",
                codex_wire::turn_steer_params(&thread_id, &previous, &input),
            )
            .await;
            match steered {
                Ok(response) => {
                    let next = response
                        .get("turnId")
                        .and_then(Value::as_str)
                        .unwrap_or(previous.as_str())
                        .to_string();
                    if let Ok(mut turns) = shared.turns.lock() {
                        if next != previous {
                            turns.successor.insert(previous.clone(), next.clone());
                        }
                        turns.live = Some(next.clone());
                    }
                    next
                }
                Err(error) => {
                    log::debug!("engine: codex steer rejected ({error}); starting a turn");
                    start_turn(shared, &thread_id, &input).await?
                }
            }
        }
        None => start_turn(shared, &thread_id, &input).await?,
    };

    Ok(wait_for_turn(shared, &turn_id).await.stop_reason())
}

async fn start_turn(
    shared: &Arc<Shared>,
    thread_id: &str,
    input: &[Value],
) -> Result<String, Error> {
    let (model, effort, mode, service_tier) = {
        let config = shared.config.lock().ok();
        let model = config.as_ref().map(|config| config.model.clone()).unwrap_or_default();
        let effort = config.as_ref().and_then(|config| config.effort.clone());
        let mode = config.as_ref().map(|config| config.mode).unwrap_or_default();
        let fast = config.as_ref().is_some_and(|config| config.fast);
        let tier = if fast { Some("fast".to_string()) } else { None };
        (model, effort, mode, tier)
    };
    let roots = shared.writable_roots();
    let params = codex_wire::turn_start_request(&TurnRequest {
        thread_id,
        input,
        model: (!model.is_empty()).then_some(model.as_str()),
        effort: effort.as_deref(),
        mode,
        service_tier: service_tier.as_deref(),
        writable_roots: &roots,
    });
    let response = call(shared, "turn/start", params).await?;
    let turn_id = response
        .get("turn")
        .and_then(|turn| turn.get("id"))
        .and_then(Value::as_str)
        .or_else(|| response.get("turnId").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| internal("codex turn/start returned no turn id"))?;
    if let Ok(mut turns) = shared.turns.lock() {
        // THE EARLY-COMPLETION GUARD: `turn/completed` can land before the
        // `turn/start` response does, so an outcome already recorded for this
        // id must survive being registered as live.
        if turns.outcomes.contains_key(&turn_id) {
            turns.live = None;
        } else {
            turns.live = Some(turn_id.clone());
        }
    }
    shared.wake.notify_waiters();
    Ok(turn_id)
}

async fn wait_for_turn(shared: &Arc<Shared>, turn_id: &str) -> TurnOutcome {
    loop {
        // Register BEFORE checking: a notification that lands between the
        // check and the await must still wake this task.
        let notified = shared.wake.notified();
        if let Some(outcome) = shared
            .turns
            .lock()
            .ok()
            .and_then(|turns| turns.outcome(turn_id))
        {
            return outcome;
        }
        if shared.closed.load(Ordering::SeqCst) {
            return TurnOutcome::Failed;
        }
        notified.await;
    }
}

async fn compact(shared: &Arc<Shared>, thread_id: &str) -> Result<StopReason, Error> {
    if let Ok(mut slot) = shared.compaction.lock() {
        *slot = Some(thread_id.to_string());
    }
    call(
        shared,
        "thread/compact/start",
        codex_wire::thread_compact_start_params(thread_id),
    )
    .await?;
    let deadline = std::time::Instant::now() + COMPACTION_TIMEOUT;
    loop {
        let notified = shared.wake.notified();
        let done = shared
            .compaction
            .lock()
            .map(|slot| slot.is_none())
            .unwrap_or(true);
        if done || shared.closed.load(Ordering::SeqCst) {
            return Ok(StopReason::EndTurn);
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            if let Ok(mut slot) = shared.compaction.lock() {
                *slot = None;
            }
            return Ok(StopReason::EndTurn);
        }
        let _ = tokio::time::timeout(left, notified).await;
    }
}

/// The fence. Marks every live turn stale and returns them so the interrupt
/// can follow; the caller must do this BEFORE awaiting anything.
fn mark_stale(shared: &Arc<Shared>) -> Vec<String> {
    // Every approval still on screen belongs to the turn being cancelled.
    let pending: Vec<ServerRequest> = shared
        .in_flight
        .lock()
        .map(|mut in_flight| in_flight.drain().map(|(_, request)| request).collect())
        .unwrap_or_default();
    for request in pending {
        let key = request.id.to_string();
        answer(shared, &key, &request, codex_wire::cancel_result(&request.method));
    }
    let mut live = Vec::new();
    if let Ok(mut turns) = shared.turns.lock() {
        if let Some(id) = turns.live.clone() {
            live.push(id.clone());
            turns.stale.insert(id.clone());
            // Resolve the waiter now: codex answers `turn/interrupt` with
            // `Ok({})` and only later sends `turn/completed`, and a prompt
            // that waits for that is a prompt that hangs if the app-server
            // dies in between.
            turns.outcomes.insert(id, TurnOutcome::Interrupted);
        }
        turns.live = None;
    }
    shared.wake.notify_waiters();
    live
}

async fn interrupt(shared: &Arc<Shared>, live: Vec<String>) {
    let Some(thread_id) = shared.session_id().map(|id| id.0.to_string()) else {
        return;
    };
    for turn_id in live {
        let _ = shared
            .server
            .request(
                "turn/interrupt",
                codex_wire::turn_interrupt_params(&thread_id, &turn_id),
            )
            .await;
    }
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

fn on_notification(
    shared: &Arc<Shared>,
    cx: &ConnectionTo<Client>,
    method: &str,
    params: &Value,
) {
    let turn_id = params
        .get("turn")
        .and_then(|turn| turn.get("id"))
        .and_then(Value::as_str)
        .or_else(|| params.get("turnId").and_then(Value::as_str))
        .map(str::to_string);

    // 1. Bookkeeping. NEVER gated on staleness: a cancelled turn still has to
    //    finish, and its usage and rate limits are still true.
    match method {
        "turn/started" => {
            if let (Some(turn_id), Ok(mut turns)) = (turn_id.clone(), shared.turns.lock()) {
                if turns.outcome(&turn_id).is_none() {
                    turns.live = Some(turn_id);
                }
            }
            flush_errors(shared, cx);
        }
        "turn/completed" | "turn/failed" => {
            let status = params
                .get("turn")
                .and_then(|turn| turn.get("status"))
                .and_then(Value::as_str)
                .unwrap_or(if method == "turn/failed" { "failed" } else { "completed" });
            let outcome = match status {
                "completed" => TurnOutcome::Completed,
                "interrupted" => TurnOutcome::Interrupted,
                _ => TurnOutcome::Failed,
            };
            if let (Some(turn_id), Ok(mut turns)) = (turn_id.clone(), shared.turns.lock()) {
                turns.outcomes.insert(turn_id.clone(), outcome);
                if turns.live.as_deref() == Some(turn_id.as_str()) {
                    turns.live = None;
                }
            }
            shared.wake.notify_waiters();
        }
        "account/rateLimits/updated" => {
            shared
                .usage
                .store(coding::agent_usage::parse_codex_rate_limits(params));
        }
        "thread/compacted" => {
            finish_compaction(shared);
        }
        "item/completed"
            if params
                .get("item")
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                == Some("contextCompaction") =>
        {
            finish_compaction(shared);
            if let (Some(turn_id), Ok(mut compacted)) = (turn_id.clone(), shared.compacted.lock()) {
                compacted.insert(turn_id);
            }
        }
        "error" => {
            let message = params
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("codex reported an error")
                .to_string();
            let live = shared
                .turns
                .lock()
                .map(|turns| turns.live.is_some())
                .unwrap_or(false);
            if live {
                emit(shared, cx, error_update(&message));
            } else if let Ok(mut pending) = shared.pending_errors.lock() {
                pending.push(message);
            }
        }
        _ => {}
    }

    // 2. The stale gate: everything a cancelled turn says afterwards is noise.
    if let Some(turn_id) = &turn_id {
        if shared.is_stale(turn_id) {
            return;
        }
    }

    // The compaction the `contextCompaction` item already drew: codex reports
    // one compaction through two channels and the strip must appear once.
    if method == "thread/compacted" {
        let drawn = turn_id
            .as_ref()
            .and_then(|turn_id| {
                shared
                    .compacted
                    .lock()
                    .ok()
                    .map(|compacted| compacted.contains(turn_id))
            })
            .unwrap_or(false);
        if drawn {
            return;
        }
    }

    // 3. The feed.
    for update in feed_updates(&shared.items, method, params) {
        emit(shared, cx, update);
    }
}

/// A compaction ended: release whatever `/compact` is waiting on it.
fn finish_compaction(shared: &Arc<Shared>) {
    if let Ok(mut slot) = shared.compaction.lock() {
        *slot = None;
    }
    shared.wake.notify_waiters();
}

fn emit(shared: &Arc<Shared>, cx: &ConnectionTo<Client>, update: SessionUpdate) {
    let Some(session_id) = shared.session_id() else { return };
    if let Err(error) = cx.send_notification(SessionNotification::new(session_id, update)) {
        log::debug!("engine: codex update not delivered: {error}");
    }
}

fn flush_errors(shared: &Arc<Shared>, cx: &ConnectionTo<Client>) {
    let pending = match shared.pending_errors.lock() {
        Ok(mut pending) => std::mem::take(&mut *pending),
        Err(_) => Vec::new(),
    };
    for message in pending {
        emit(shared, cx, error_update(&message));
    }
}

fn error_update(message: &str) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
        format!("codex error: {message}"),
    ))))
}

/// The notification → `SessionUpdate` table. Anything not listed produces
/// nothing on purpose: with two codex versions in play, an unknown method is
/// the normal case, not an error.
fn feed_updates(items: &Items, method: &str, params: &Value) -> Vec<SessionUpdate> {
    let item_id = params
        .get("itemId")
        .and_then(Value::as_str)
        .map(str::to_string);
    match method {
        "item/agentMessage/delta" => {
            let Some(delta) = delta_text(params) else { return Vec::new() };
            if let Some(id) = &item_id {
                items.mark_delta(id);
            }
            let mut chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(delta)));
            if let Some(id) = item_id {
                chunk = chunk.message_id(id.as_str());
            }
            vec![SessionUpdate::AgentMessageChunk(chunk)]
        }
        "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
            let Some(delta) = delta_text(params) else { return Vec::new() };
            if let Some(id) = &item_id {
                items.mark_delta(id);
            }
            let mut chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(delta)));
            if let Some(id) = item_id {
                chunk = chunk.message_id(id.as_str());
            }
            vec![SessionUpdate::AgentThoughtChunk(chunk)]
        }
        "item/commandExecution/outputDelta" | "item/mcpToolCall/progress" => {
            let Some(id) = item_id else { return Vec::new() };
            let chunk = delta_text(params)
                .or_else(|| params.get("message").and_then(Value::as_str).map(str::to_string));
            let Some(chunk) = chunk else { return Vec::new() };
            vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id.as_str()),
                ToolCallUpdateFields::new().content(vec![text_content(&chunk)]),
            ))]
        }
        "item/started" => params
            .get("item")
            .map(|item| item_updates(items, item, false, false))
            .unwrap_or_default(),
        "item/completed" => params
            .get("item")
            .map(|item| item_updates(items, item, true, false))
            .unwrap_or_default(),
        "thread/tokenUsage/updated" => usage_update(params).into_iter().collect(),
        "turn/plan/updated" => plan_update(params).into_iter().collect(),
        "thread/compacted" => vec![SessionUpdate::CompactionUpdate(CompactionUpdate::new(
            compaction_id(params),
            CompactionStatus::Completed,
        ))],
        "thread/name/updated" => params
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(|name| {
                SessionUpdate::SessionInfoUpdate(
                    agent_client_protocol::schema::v1::SessionInfoUpdate::new()
                        .title(name.to_string()),
                )
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn delta_text(params: &Value) -> Option<String> {
    // `delta` on this codex, `deltaText` on newer ones.
    params
        .get("delta")
        .or_else(|| params.get("deltaText"))
        .and_then(Value::as_str)
        .filter(|delta| !delta.is_empty())
        .map(str::to_string)
}

fn text_content(text: &str) -> ToolCallContent {
    ToolCallContent::Content(agent_client_protocol::schema::v1::Content::new(
        ContentBlock::Text(TextContent::new(text)),
    ))
}

fn compaction_id(params: &Value) -> String {
    params
        .get("turnId")
        .and_then(Value::as_str)
        .map(|turn| format!("compact-{turn}"))
        .unwrap_or_else(|| "compact".to_string())
}

/// `usage_update` is null unless BOTH halves are present: a context window we
/// do not know turns the percentage into a lie.
fn usage_update(params: &Value) -> Option<SessionUpdate> {
    let usage = params.get("tokenUsage")?;
    let used = usage
        .get("last")
        .and_then(|last| last.get("totalTokens"))
        .and_then(Value::as_u64)?;
    let size = usage.get("modelContextWindow").and_then(Value::as_u64)?;
    Some(SessionUpdate::UsageUpdate(UsageUpdate::new(used, size)))
}

fn plan_update(params: &Value) -> Option<SessionUpdate> {
    let steps = params.get("plan")?.as_array()?;
    let entries = steps
        .iter()
        .filter_map(|step| {
            let content = step.get("step").and_then(Value::as_str)?;
            let status = match step.get("status").and_then(Value::as_str) {
                Some("completed") => PlanEntryStatus::Completed,
                Some("inProgress") => PlanEntryStatus::InProgress,
                _ => PlanEntryStatus::Pending,
            };
            Some(PlanEntry::new(content, PlanEntryPriority::Medium, status))
        })
        .collect::<Vec<_>>();
    Some(SessionUpdate::Plan(Plan::new(entries)))
}

impl ItemView {
    /// A message or reasoning item: no card, but its deltas are remembered so
    /// the completion does not repeat the text.
    fn message() -> ItemView {
        ItemView {
            tool: false,
            deltas: false,
        }
    }

    fn card() -> ItemView {
        ItemView {
            tool: true,
            deltas: false,
        }
    }
}

fn acp_status(status: Option<&str>) -> ToolCallStatus {
    match status {
        Some("completed") => ToolCallStatus::Completed,
        Some("inProgress") => ToolCallStatus::InProgress,
        Some("failed") | Some("declined") | Some("interrupted") => ToolCallStatus::Failed,
        _ => ToolCallStatus::Pending,
    }
}

/// One `ThreadItem` → the updates it produces. `history` replays a stored
/// thread, where no deltas ever arrived, so text items must render themselves.
fn item_updates(
    items: &Items,
    item: &Value,
    completed: bool,
    history: bool,
) -> Vec<SessionUpdate> {
    let Some(id) = item.get("id").and_then(Value::as_str) else {
        return Vec::new();
    };
    let kind = item.get("type").and_then(Value::as_str).unwrap_or_default();
    let status = acp_status(item.get("status").and_then(Value::as_str));
    let seen_deltas = items.seen_deltas(id);

    match kind {
        "agentMessage" => {
            if !completed || (seen_deltas && !history) {
                return Vec::new();
            }
            let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
            if text.is_empty() {
                return Vec::new();
            }
            vec![SessionUpdate::AgentMessageChunk(
                ContentChunk::new(ContentBlock::Text(TextContent::new(text))).message_id(id),
            )]
        }
        "reasoning" => {
            if !completed || (seen_deltas && !history) {
                return Vec::new();
            }
            let text = joined(item, "summary").or_else(|| joined(item, "content"));
            text.map(|text| {
                SessionUpdate::AgentThoughtChunk(
                    ContentChunk::new(ContentBlock::Text(TextContent::new(text))).message_id(id),
                )
            })
            .into_iter()
            .collect()
        }
        "plan" => {
            if !completed {
                return Vec::new();
            }
            item.get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| {
                    SessionUpdate::AgentMessageChunk(
                        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                            .message_id(id),
                    )
                })
                .into_iter()
                .collect()
        }
        "userMessage" if history => {
            let text = item
                .get("content")
                .and_then(Value::as_array)
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|block| block.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            if text.is_empty() {
                return Vec::new();
            }
            vec![SessionUpdate::UserMessageChunk(ContentChunk::new(
                ContentBlock::Text(TextContent::new(text)),
            ))]
        }
        "commandExecution" => command_updates(items, id, item, completed, status),
        "fileChange" => file_change_updates(items, id, item, completed, status),
        "mcpToolCall" => {
            let title = format!(
                "mcp.{}.{}",
                item.get("server").and_then(Value::as_str).unwrap_or("?"),
                item.get("tool").and_then(Value::as_str).unwrap_or("?")
            );
            surface(items, id, item, completed, status, ToolKind::Other, title, Vec::new())
        }
        "dynamicToolCall" => {
            let title = item
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            surface(items, id, item, completed, status, ToolKind::Other, title, Vec::new())
        }
        "webSearch" => {
            let title = match item.get("query").and_then(Value::as_str) {
                Some(query) if !query.is_empty() => format!("Search the web for '{query}'"),
                _ => "Search the web".to_string(),
            };
            surface(items, id, item, completed, status, ToolKind::Fetch, title, Vec::new())
        }
        "imageView" => {
            let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
            surface(
                items,
                id,
                item,
                completed,
                status,
                ToolKind::Read,
                format!("View image {path}"),
                locations(path),
            )
        }
        "contextCompaction" => {
            let status = if completed {
                CompactionStatus::Completed
            } else {
                CompactionStatus::InProgress
            };
            vec![SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                CompactionId::new(id),
                status,
            ))]
        }
        "subAgentActivity" => {
            let name = item
                .get("agentPath")
                .and_then(Value::as_str)
                .unwrap_or("subagent");
            let activity = item.get("kind").and_then(Value::as_str).unwrap_or("started");
            let title = format!("{} subagent {}", verb(activity), short_name(name));
            let thread = item
                .get("agentThreadId")
                .and_then(Value::as_str)
                .unwrap_or(id);
            subagent(items, id, completed, status, title, thread, name, activity)
        }
        "collabAgentToolCall" => {
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("subagent");
            let thread = item
                .get("receiverThreadIds")
                .and_then(Value::as_array)
                .and_then(|ids| ids.first())
                .and_then(Value::as_str)
                .unwrap_or(id);
            subagent(
                items,
                id,
                completed,
                status,
                tool.to_string(),
                thread,
                tool,
                if completed { "completed" } else { "started" },
            )
        }
        "exitedReviewMode" => {
            if !completed {
                return Vec::new();
            }
            item.get("review")
                .and_then(Value::as_str)
                .map(|review| {
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(review),
                    )))
                })
                .into_iter()
                .collect()
        }
        // `sleep`, `hookPrompt`, `enteredReviewMode`, `imageGeneration`,
        // `functionCallOutput` (0.153.3) and anything a newer codex adds.
        _ => Vec::new(),
    }
}

fn verb(activity: &str) -> &'static str {
    match activity {
        "interacted" => "Interact with",
        "interrupted" => "Interrupt",
        "completed" => "Finish",
        _ => "Start",
    }
}

fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn joined(item: &Value, field: &str) -> Option<String> {
    let parts = item.get(field)?.as_array()?;
    let text = parts
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

fn locations(path: &str) -> Vec<ToolCallLocation> {
    if path.is_empty() {
        return Vec::new();
    }
    vec![ToolCallLocation::new(PathBuf::from(path))]
}

/// A tool card, first surface or update. `raw_input`/`raw_output` stay LOCAL:
/// the mapper derives every wire string from the title and the derived detail,
/// never from these (§4.2).
#[allow(clippy::too_many_arguments)]
fn surface(
    items: &Items,
    id: &str,
    item: &Value,
    completed: bool,
    status: ToolCallStatus,
    kind: ToolKind,
    title: String,
    locations: Vec<ToolCallLocation>,
) -> Vec<SessionUpdate> {
    let known = items.has_card(id);
    if !known {
        items.add_card(id);
        let mut call = ToolCall::new(ToolCallId::new(id), title)
            .kind(kind)
            .status(status)
            .locations(locations);
        if let Some(arguments) = item.get("arguments") {
            call = call.raw_input(arguments.clone());
        }
        let mut updates = vec![SessionUpdate::ToolCall(call)];
        if completed {
            updates.extend(complete(id, item, status));
        }
        return updates;
    }
    if completed {
        return complete(id, item, status);
    }
    vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
        ToolCallUpdateFields::new().status(status),
    ))]
}

fn complete(id: &str, item: &Value, status: ToolCallStatus) -> Vec<SessionUpdate> {
    let mut fields = ToolCallUpdateFields::new().status(status);
    if let Some(result) = item.get("result") {
        fields = fields.raw_output(result.clone());
    } else if let Some(error) = item.get("error") {
        fields = fields.raw_output(error.clone());
    }
    vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new(id),
        fields,
    ))]
}

#[allow(clippy::too_many_arguments)]
fn subagent(
    items: &Items,
    id: &str,
    completed: bool,
    status: ToolCallStatus,
    title: String,
    thread: &str,
    agent_type: &str,
    activity: &str,
) -> Vec<SessionUpdate> {
    // ACP v1 has no subagent update, so the edge rides `_meta` on the tool
    // card the subagent already produces, under the engine's own key: the
    // mapper reads it off the card and publishes the relay `subagent` kind.
    let mut meta = serde_json::Map::new();
    meta.insert(
        crate::local::SUBAGENT_META_KEY.to_string(),
        json!({
            "id": thread,
            "agentType": short_name(agent_type),
            "status": if completed || activity == "completed" { "completed" } else { "started" },
        }),
    );
    let known = items.has_card(id);
    if !known {
        items.add_card(id);
        return vec![SessionUpdate::ToolCall(
            ToolCall::new(ToolCallId::new(id), title)
                .kind(ToolKind::Other)
                .status(status)
                .meta(meta),
        )];
    }
    vec![SessionUpdate::ToolCallUpdate(
        ToolCallUpdate::new(ToolCallId::new(id), ToolCallUpdateFields::new().status(status)).meta(meta),
    )]
}

fn command_updates(
    items: &Items,
    id: &str,
    item: &Value,
    completed: bool,
    status: ToolCallStatus,
) -> Vec<SessionUpdate> {
    let command = item.get("command").and_then(Value::as_str).unwrap_or_default();
    let actions = item
        .get("commandActions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (kind, title, locations) = match actions.as_slice() {
        [action] => command_action(action, command),
        // Several actions, or none: the command itself is the headline.
        _ => (
            ToolKind::Execute,
            codex_wire::strip_shell_prefix(command).to_string(),
            Vec::new(),
        ),
    };
    let known = items.has_card(id);
    let mut updates = Vec::new();
    if !known {
        items.add_card(id);
        updates.push(SessionUpdate::ToolCall(
            ToolCall::new(ToolCallId::new(id), title).kind(kind).status(status).locations(locations),
        ));
        if !completed {
            return updates;
        }
    }
    if completed {
        // The output lands as CONTENT, never as a `terminal` — the client
        // advertises `terminal: false` and renders a command-output card
        // instead (D5).
        let mut fields = ToolCallUpdateFields::new().status(status);
        let output = item
            .get("aggregatedOutput")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !output.is_empty() {
            fields = fields.content(vec![text_content(output)]);
        }
        fields = fields.raw_output(json!({
            "formatted_output": output,
            "exit_code": item.get("exitCode").cloned().unwrap_or(Value::Null),
        }));
        updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            ToolCallId::new(id),
            fields,
        )));
    } else if known {
        updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
            ToolCallUpdateFields::new().status(status),
        )));
    }
    updates
}

fn command_action(action: &Value, command: &str) -> (ToolKind, String, Vec<ToolCallLocation>) {
    let path = action.get("path").and_then(Value::as_str).unwrap_or_default();
    match action.get("type").and_then(Value::as_str) {
        Some("read") => (
            ToolKind::Read,
            format!("Read file '{path}'"),
            locations(path),
        ),
        Some("listFiles") => (
            ToolKind::Read,
            format!("List files in '{path}'"),
            locations(path),
        ),
        Some("search") => {
            let query = action.get("query").and_then(Value::as_str).unwrap_or_default();
            let title = if path.is_empty() {
                format!("Search for '{query}'")
            } else {
                format!("Search for '{query}' in {path}")
            };
            (ToolKind::Search, title, locations(path))
        }
        _ => (
            ToolKind::Execute,
            codex_wire::strip_shell_prefix(command).to_string(),
            Vec::new(),
        ),
    }
}

fn file_change_updates(
    items: &Items,
    id: &str,
    item: &Value,
    completed: bool,
    status: ToolCallStatus,
) -> Vec<SessionUpdate> {
    let known = items.has_card(id);
    if known {
        return vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
            ToolCallUpdateFields::new().status(status),
        ))];
    }
    let changes = item
        .get("changes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut content = Vec::new();
    let mut paths = Vec::new();
    for change in &changes {
        let Some(path) = change.get("path").and_then(Value::as_str) else { continue };
        paths.push(path.to_string());
        if let Some(diff) = file_change_diff(change, path) {
            content.push(ToolCallContent::Diff(diff));
        }
    }
    let title = match paths.as_slice() {
        [one] => format!("Edit {}", short_name(one)),
        _ => "Editing files".to_string(),
    };
    items.add_card(id);
    let mut updates = vec![SessionUpdate::ToolCall(
        ToolCall::new(ToolCallId::new(id), title)
            .kind(ToolKind::Edit)
            .status(status)
            .content(content)
            .locations(
                paths
                    .iter()
                    .map(|path| ToolCallLocation::new(PathBuf::from(path)))
                    .collect(),
            ),
    )];
    if completed {
        updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
            ToolCallUpdateFields::new().status(status),
        )));
    }
    updates
}

/// `add` and `delete` carry the WHOLE FILE in `diff`; only `update` is a real
/// unified diff, and its hunks are anchored against the file on disk. Reading
/// the file and applying the patch is the only way to get both sides of the
/// card right — with a revert fallback for the case where the change already
/// landed.
fn file_change_diff(change: &Value, path: &str) -> Option<Diff> {
    let body = change.get("diff").and_then(Value::as_str).unwrap_or_default();
    let kind = change
        .get("kind")
        .and_then(|kind| kind.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("update");
    match kind {
        "add" => Some(Diff::new(PathBuf::from(path), body.to_string())),
        "delete" => Some(
            Diff::new(PathBuf::from(path), String::new()).old_text(body.to_string()),
        ),
        _ => {
            let on_disk = std::fs::read_to_string(Path::new(path)).ok()?;
            if let Some(new_text) = codex_wire::apply_unified_diff(&on_disk, body) {
                return Some(Diff::new(PathBuf::from(path), new_text).old_text(on_disk));
            }
            let old_text = codex_wire::revert_unified_diff(&on_disk, body)?;
            Some(Diff::new(PathBuf::from(path), on_disk).old_text(old_text))
        }
    }
}

// ---------------------------------------------------------------------------
// Server requests: approvals and elicitations
// ---------------------------------------------------------------------------

async fn on_server_request(shared: &Arc<Shared>, cx: &ConnectionTo<Client>, request: ServerRequest) {
    let key = request.id.to_string();
    let turn_id = request
        .params
        .get("turnId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // A stale turn's approval is answered "cancel" without ever reaching a
    // person: the turn it belongs to is already gone.
    if !turn_id.is_empty() && shared.is_stale(&turn_id) {
        answer(shared, &key, &request, codex_wire::cancel_result(&request.method));
        return;
    }

    let choices = match request.method.as_str() {
        "item/commandExecution/requestApproval" => {
            codex_wire::command_approval_choices(&request.params)
        }
        "item/fileChange/requestApproval" => codex_wire::file_change_approval_choices(),
        "item/permissions/requestApproval" => {
            codex_wire::permission_approval_choices(&request.params)
        }
        "item/tool/requestUserInput" => {
            // A question rather than an approval: the ONE stepper the relay
            // renders lives in the engine mapper, so each question rides its
            // own permission request with one option per offered answer.
            question(shared, cx, &key, &request).await;
            return;
        }
        "mcpServer/elicitation/request" => {
            // An MCP server asking the user something. Exponential's own MCP
            // server never elicits, and answering with a made-up shape is
            // worse than declining, so this one is declined with a log line
            // until a server that needs it exists.
            log::debug!("engine: codex mcp elicitation declined (unsupported)");
            answer(shared, &key, &request, codex_wire::cancel_result(&request.method));
            return;
        }
        // An approval kind we do not know cannot be answered honestly.
        _ => Vec::new(),
    };
    if choices.is_empty() {
        answer(shared, &key, &request, codex_wire::cancel_result(&request.method));
        return;
    }

    let Some(session_id) = shared.session_id() else {
        answer(shared, &key, &request, codex_wire::cancel_result(&request.method));
        return;
    };
    if let Ok(mut in_flight) = shared.in_flight.lock() {
        in_flight.insert(key.clone(), request.clone());
    }
    let outcome = cx
        .send_request(RequestPermissionRequest::new(
            session_id,
            approval_card(&request),
            choices
                .iter()
                .map(|choice| {
                    PermissionOption::new(
                        PermissionOptionId::new(choice.option_id.as_str()),
                        choice.label.clone(),
                        permission_kind(choice.kind),
                    )
                })
                .collect(),
        ))
        .block_task()
        .await;

    let result = match outcome {
        Ok(response) => match response.outcome {
            RequestPermissionOutcome::Selected(selected) => choices
                .iter()
                .find(|choice| choice.option_id == selected.option_id.0.as_ref())
                .map(|choice| choice.result.clone())
                // An option id we never offered is not an approval.
                .unwrap_or_else(|| codex_wire::cancel_result(&request.method)),
            _ => codex_wire::cancel_result(&request.method),
        },
        Err(error) => {
            log::debug!("engine: codex approval not answered: {error}");
            codex_wire::cancel_result(&request.method)
        }
    };
    forget(shared, &key);
    answer(shared, &key, &request, result);
}

fn permission_kind(kind: ApprovalKind) -> PermissionOptionKind {
    match kind {
        ApprovalKind::AllowOnce => PermissionOptionKind::AllowOnce,
        ApprovalKind::AllowAlways => PermissionOptionKind::AllowAlways,
        ApprovalKind::RejectOnce => PermissionOptionKind::RejectOnce,
    }
}

/// The card an approval renders on. The command itself is the title; the
/// engine mapper derives the wire `detail` from it and never from `rawInput`.
fn approval_card(request: &ServerRequest) -> ToolCallUpdate {
    let id = request
        .params
        .get("itemId")
        .and_then(Value::as_str)
        .unwrap_or("approval");
    let title = match request.method.as_str() {
        "item/commandExecution/requestApproval" => request
            .params
            .get("command")
            .and_then(Value::as_str)
            .map(|command| codex_wire::strip_shell_prefix(command).to_string())
            .unwrap_or_else(|| "Run command?".to_string()),
        "item/fileChange/requestApproval" => "Make edits?".to_string(),
        "item/permissions/requestApproval" => "Grant permissions?".to_string(),
        _ => "Codex needs an answer".to_string(),
    };
    let kind = match request.method.as_str() {
        "item/fileChange/requestApproval" => ToolKind::Edit,
        "item/permissions/requestApproval" => ToolKind::Other,
        _ => ToolKind::Execute,
    };
    ToolCallUpdate::new(
                ToolCallId::new(id),
        ToolCallUpdateFields::new().title(title).kind(kind),
    )
}

/// `item/tool/requestUserInput` as ONE ACP elicitation: a form with one
/// property per question, which is what the engine mapper turns into the
/// `<ask>#<n>` … `<ask>#submit` stepper the four clients render. One
/// permission request per question would collide on the id — codex sends every
/// question of a request under the SAME `itemId`.
///
/// `autoResolutionMs` races the person: codex resolves the request itself when
/// it expires, so an answer that arrives late must never be sent.
async fn question(
    shared: &Arc<Shared>,
    cx: &ConnectionTo<Client>,
    key: &str,
    request: &ServerRequest,
) {
    let questions = request
        .params
        .get("questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(session_id) = shared.session_id() else {
        answer(shared, key, request, codex_wire::cancel_result(&request.method));
        return;
    };
    if questions.is_empty() {
        answer(shared, key, request, codex_wire::cancel_result(&request.method));
        return;
    }

    let mut schema = ElicitationSchema::new();
    let mut headers = Vec::new();
    for entry in &questions {
        let Some(id) = entry.get("id").and_then(Value::as_str) else { continue };
        let text = entry
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let header = entry
            .get("header")
            .and_then(Value::as_str)
            .unwrap_or(text.as_str())
            .to_string();
        headers.push(header.clone());
        let mut property = StringPropertySchema::new()
            .title(header)
            .description(text);
        let labels: Vec<String> = entry
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(|option| option.get("label").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        // `isOther` means the person may write their own answer, so the
        // options stay a hint rather than the only choices.
        let free_text = entry
            .get("isOther")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !labels.is_empty() && !free_text {
            property = property.enum_values(labels);
        }
        schema = schema.property(id, property, true);
    }

    // EXP-758: INSIDE the cancel fence, exactly like an approval. A question
    // left out of `in_flight` survived `session/cancel` on codex's side: the
    // fence answered every approval and left this request open, so the
    // app-server sat waiting for an answer to a turn that was already gone.
    if let Ok(mut in_flight) = shared.in_flight.lock() {
        in_flight.insert(key.to_string(), request.clone());
    }
    let ask = cx.send_request(CreateElicitationRequest::new(
        ElicitationFormMode::new(
            ElicitationSessionScope::new(session_id).tool_call_id(
                request
                    .params
                    .get("itemId")
                    .and_then(Value::as_str)
                    .map(ToolCallId::new),
            ),
            schema,
        ),
        headers.first().cloned().unwrap_or_else(|| "Codex needs an answer".to_string()),
    ));
    let auto = request
        .params
        .get("autoResolutionMs")
        .and_then(Value::as_u64)
        .filter(|millis| *millis > 0);
    let response = match auto {
        Some(millis) => {
            match tokio::time::timeout(Duration::from_millis(millis), ask.block_task()).await {
                Ok(response) => response,
                // codex has resolved it itself by now; answering would be
                // answering a request that no longer exists.
                Err(_) => {
                    forget(shared, key);
                    return;
                }
            }
        }
        None => ask.block_task().await,
    };
    forget(shared, key);
    let Ok(response) = response else {
        answer(shared, key, request, codex_wire::cancel_result(&request.method));
        return;
    };
    let ElicitationAction::Accept(accepted) = response.action else {
        answer(shared, key, request, codex_wire::cancel_result(&request.method));
        return;
    };
    let mut answers = serde_json::Map::new();
    for (id, value) in accepted.content.unwrap_or_default() {
        let value = match value {
            ElicitationContentValue::String(value) => value,
            ElicitationContentValue::Integer(value) => value.to_string(),
            ElicitationContentValue::Number(value) => value.to_string(),
            ElicitationContentValue::Boolean(value) => value.to_string(),
            // A shape a newer client answers with: skipping it is better than
            // sending codex a stringified blob.
            _ => continue,
        };
        answers.insert(id, json!({ "answers": [value] }));
    }
    answer(shared, key, request, json!({ "answers": answers }));
}

/// EXP-758: take a server request back off the cancel fence's books. Called
/// the moment its own answer is decided, so the fence never re-answers a
/// request that is already spoken for (`answer` would drop it, but a request
/// codex resolved ITSELF is not on the `answered` books at all).
fn forget(shared: &Arc<Shared>, key: &str) {
    if let Ok(mut in_flight) = shared.in_flight.lock() {
        in_flight.remove(key);
    }
}

/// Answer a server request exactly once. Two paths race for it — the person's
/// pick and the cancel fence — and codex must see the first, not both.
fn answer(shared: &Arc<Shared>, key: &str, request: &ServerRequest, result: Value) {
    let first = shared
        .answered
        .lock()
        .map(|mut answered| answered.insert(key.to_string()))
        .unwrap_or(true);
    if !first {
        return;
    }
    if let Err(error) = shared.server.respond(request.id.clone(), result) {
        log::warn!("engine: codex approval not delivered: {error}");
    }
}

// ---------------------------------------------------------------------------
// History: session/load and session/list
// ---------------------------------------------------------------------------

/// `session/load`, both of its callers: the read-only transcript replay AND
/// the RESUME of an ended ACP run (`ResumeHandle::Acp`, `host.rs`) — the host
/// routes both here, and only `spec.replay` tells them apart.
///
/// A replay reads the rollout and stops. A resume has to come back STEERABLE,
/// which takes the two things `open_thread` does for a fresh thread and a
/// `thread/read` does not: `thread/resume` (codex has no live thread until it
/// is asked for one) and [`start_pumps`] (without them nothing drains
/// notifications or approvals, so the first prompt's `wait_for_turn` never
/// returns and the run wedges with no symptom).
async fn load_thread(
    shared: &Arc<Shared>,
    cx: &ConnectionTo<Client>,
    request: LoadSessionRequest,
    notifications: flume::Receiver<(String, Value)>,
    requests: flume::Receiver<ServerRequest>,
) -> Result<LoadSessionResponse, Error> {
    let thread_id = request.session_id.0.to_string();
    if let Ok(mut slot) = shared.session_id.lock() {
        *slot = Some(request.session_id.clone());
    }
    if let Ok(mut slot) = shared.cwd.lock() {
        *slot = request.cwd.clone();
    }
    handshake(shared).await.ok();
    let thread = call(
        shared,
        "thread/read",
        codex_wire::thread_read_params(&thread_id, true),
    )
    .await?;
    let turns = thread
        .get("thread")
        .and_then(|thread| thread.get("turns"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    log::debug!("engine: codex thread/read {thread_id}: {} turns", turns.len());
    for turn in turns {
        for item in turn.get("items").and_then(Value::as_array).unwrap_or(&Vec::new()) {
            let updates = item_updates(&shared.items, item, true, true);
            log::debug!(
                "engine: codex replay item {} -> {} updates",
                item.get("type").and_then(Value::as_str).unwrap_or("?"),
                updates.len()
            );
            for update in updates {
                emit(shared, cx, update);
            }
        }
    }
    if !shared.spec.replay {
        resume_loaded_thread(shared, cx, &thread_id, &request.cwd, notifications, requests).await?;
    }
    Ok(LoadSessionResponse::new()
        .modes(mode_state(shared))
        .config_options(config_options(shared)))
}

/// The half a RESUME needs on top of the history: `open_thread`'s tail, in its
/// order. A `thread/read` leaves codex with no live thread and this host with
/// no pumps, which is exactly what wedged a resumed run — the first prompt
/// reached `turn/start` and nothing ever drained the answer.
async fn resume_loaded_thread(
    shared: &Arc<Shared>,
    cx: &ConnectionTo<Client>,
    thread_id: &str,
    cwd: &Path,
    notifications: flume::Receiver<(String, Value)>,
    requests: flume::Receiver<ServerRequest>,
) -> Result<(), Error> {
    let roots = vec![cwd.to_path_buf()];
    if let Ok(mut slot) = shared.roots.lock() {
        *slot = roots.clone();
    }
    let config = thread_config_for(shared, &roots);
    // EXP-763: the run playbook, on start AND resume — same text both times.
    let response = call(
        shared,
        "thread/resume",
        codex_wire::thread_resume_params(thread_id, cwd, config, Some(coding::skill::RUN_SKILL)),
    )
    .await?;
    // The thread codex actually re-opened is the one every later `turn/start`
    // has to name (`run_prompt` reads this slot), so the response wins over
    // the id we were loaded with — normally the same string.
    if let Some(id) = response
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
    {
        if let Ok(mut slot) = shared.session_id.lock() {
            *slot = Some(SessionId::new(id));
        }
    }
    // Only now, like `open_thread`: the channels buffer from spawn, so the
    // resume's own frames are still there to drain.
    start_pumps(shared, cx, notifications, requests);
    seed_config(shared, &response).await;
    Ok(())
}

async fn list_threads(
    shared: &Arc<Shared>,
    request: ListSessionsRequest,
) -> Result<ListSessionsResponse, Error> {
    handshake(shared).await.ok();
    let page = call(
        shared,
        "thread/list",
        codex_wire::thread_list_params(request.cwd.as_deref(), request.cursor.as_deref()),
    )
    .await?;
    let sessions = page
        .get("data")
        .and_then(Value::as_array)
        .map(|threads| {
            threads
                .iter()
                .filter_map(|thread| {
                    let id = thread.get("id").and_then(Value::as_str)?;
                    let cwd = thread.get("cwd").and_then(Value::as_str).unwrap_or_default();
                    let title = thread
                        .get("name")
                        .and_then(Value::as_str)
                        .or_else(|| thread.get("preview").and_then(Value::as_str))
                        .filter(|title| !title.is_empty())
                        .map(str::to_string);
                    Some(SessionInfo::new(SessionId::new(id), PathBuf::from(cwd)).title(title))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = ListSessionsResponse::new(sessions);
    if let Some(cursor) = page.get("nextCursor").and_then(Value::as_str) {
        response = response.next_cursor(cursor.to_string());
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(updates: Vec<SessionUpdate>) -> SessionUpdate {
        assert_eq!(updates.len(), 1, "expected exactly one update");
        updates.into_iter().next().expect("one update")
    }

    fn text_of(update: &SessionUpdate) -> String {
        let chunk = match update {
            SessionUpdate::AgentMessageChunk(chunk)
            | SessionUpdate::AgentThoughtChunk(chunk)
            | SessionUpdate::UserMessageChunk(chunk) => chunk,
            other => panic!("expected a chunk, got {other:?}"),
        };
        match &chunk.content {
            ContentBlock::Text(text) => text.text.clone(),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn a_command_item_becomes_an_execute_card_and_its_output_is_content() {
        let items = Items::default();
        let started = json!({
            "threadId": "t1", "turnId": "turn_1",
            "item": { "id": "item_1", "type": "commandExecution",
                      "command": "bash -lc 'cargo test'", "commandActions": [],
                      "status": "inProgress" },
        });
        let update = only(feed_updates(&items, "item/started", &started));
        let SessionUpdate::ToolCall(call) = update else {
            panic!("expected a tool call");
        };
        assert_eq!(call.title, "cargo test");
        assert_eq!(call.kind, ToolKind::Execute);

        let completed = json!({
            "threadId": "t1", "turnId": "turn_1",
            "item": { "id": "item_1", "type": "commandExecution", "command": "bash -lc 'cargo test'",
                      "commandActions": [], "status": "completed",
                      "aggregatedOutput": "3 passed", "exitCode": 0 },
        });
        let update = only(feed_updates(&items, "item/completed", &completed));
        let SessionUpdate::ToolCallUpdate(patch) = update else {
            panic!("expected a tool call update");
        };
        assert_eq!(patch.fields.status, Some(ToolCallStatus::Completed));
        // D5: the client advertises `terminal: false`, so output rides as
        // CONTENT and renders as a local card.
        let content = patch.fields.content.expect("output content");
        assert!(matches!(content.first(), Some(ToolCallContent::Content(_))));
        assert_eq!(
            patch.fields.raw_output,
            Some(json!({ "formatted_output": "3 passed", "exit_code": 0 }))
        );
    }

    #[test]
    fn a_read_action_names_the_file_instead_of_the_command() {
        let items = Items::default();
        let started = json!({
            "turnId": "turn_1",
            "item": { "id": "item_2", "type": "commandExecution", "command": "sed -n 1,40p src/lib.rs",
                      "commandActions": [{ "type": "read", "path": "src/lib.rs", "command": "sed" }],
                      "status": "inProgress" },
        });
        let SessionUpdate::ToolCall(call) = only(feed_updates(&items, "item/started", &started))
        else {
            panic!("expected a tool call");
        };
        assert_eq!(call.title, "Read file 'src/lib.rs'");
        assert_eq!(call.kind, ToolKind::Read);
        assert_eq!(call.locations.len(), 1);
    }

    #[test]
    fn a_delta_suppresses_the_repeated_completion_text() {
        let items = Items::default();
        let delta = json!({ "turnId": "turn_1", "itemId": "item_3", "delta": "hello " });
        assert_eq!(
            text_of(&only(feed_updates(&items, "item/agentMessage/delta", &delta))),
            "hello "
        );
        // The completed item repeats the whole message; a client that already
        // streamed it must not print it twice.
        let completed = json!({
            "turnId": "turn_1",
            "item": { "id": "item_3", "type": "agentMessage", "text": "hello world" },
        });
        assert!(feed_updates(&items, "item/completed", &completed).is_empty());
    }

    #[test]
    fn an_item_that_never_streamed_renders_at_completion() {
        let items = Items::default();
        let completed = json!({
            "turnId": "turn_1",
            "item": { "id": "item_4", "type": "agentMessage", "text": "done" },
        });
        assert_eq!(
            text_of(&only(feed_updates(&items, "item/completed", &completed))),
            "done"
        );
    }

    #[test]
    fn token_usage_needs_both_halves() {
        let items = Items::default();
        let both = json!({
            "turnId": "turn_1",
            "tokenUsage": { "last": { "totalTokens": 1200 }, "modelContextWindow": 200000 },
        });
        let SessionUpdate::UsageUpdate(usage) =
            only(feed_updates(&items, "thread/tokenUsage/updated", &both))
        else {
            panic!("expected a usage update");
        };
        assert_eq!((usage.used, usage.size), (1200, 200000));
        // A context window we do not know turns the percentage into a lie.
        let half = json!({ "turnId": "turn_1", "tokenUsage": { "last": { "totalTokens": 1200 } } });
        assert!(feed_updates(&items, "thread/tokenUsage/updated", &half).is_empty());
    }

    #[test]
    fn a_plan_update_becomes_plan_entries() {
        let items = Items::default();
        let params = json!({
            "turnId": "turn_1",
            "plan": [
                { "step": "read the code", "status": "completed" },
                { "step": "write the test", "status": "inProgress" },
                { "step": "run it", "status": "pending" },
            ],
        });
        let SessionUpdate::Plan(plan) = only(feed_updates(&items, "turn/plan/updated", &params))
        else {
            panic!("expected a plan");
        };
        assert_eq!(plan.entries.len(), 3);
        assert_eq!(plan.entries[0].status, PlanEntryStatus::Completed);
        assert_eq!(plan.entries[1].status, PlanEntryStatus::InProgress);
        assert_eq!(plan.entries[2].content, "run it");
    }

    #[test]
    fn a_file_change_add_carries_the_whole_file_not_a_diff() {
        let items = Items::default();
        let params = json!({
            "turnId": "turn_1",
            "item": { "id": "item_5", "type": "fileChange", "status": "inProgress",
                      "changes": [{ "path": "/work/new.rs", "kind": { "type": "add" },
                                    "diff": "fn main() {}\n" }] },
        });
        let SessionUpdate::ToolCall(call) = only(feed_updates(&items, "item/started", &params))
        else {
            panic!("expected a tool call");
        };
        assert_eq!(call.kind, ToolKind::Edit);
        let Some(ToolCallContent::Diff(diff)) = call.content.first() else {
            panic!("expected a diff");
        };
        // `add` and `delete` carry the FULL FILE in `diff`; rendering it as a
        // patch would be wrong on both sides.
        assert_eq!(diff.new_text, "fn main() {}\n");
        assert_eq!(diff.old_text, None);
    }

    #[test]
    fn a_subagent_item_carries_its_edge_in_meta() {
        let items = Items::default();
        let params = json!({
            "turnId": "turn_1",
            "item": { "id": "item_6", "type": "subAgentActivity", "status": "inProgress",
                      "agentPath": "agents/reviewer", "agentThreadId": "thread_9",
                      "kind": "started" },
        });
        let SessionUpdate::ToolCall(call) = only(feed_updates(&items, "item/started", &params))
        else {
            panic!("expected a tool call");
        };
        assert_eq!(call.title, "Start subagent reviewer");
        // ACP v1 has no subagent update, so the edge rides `_meta` for the
        // engine mapper to publish as the relay `subagent` kind.
        let meta = call.meta.expect("subagent meta");
        assert_eq!(
            meta.get("exponentialSubagent"),
            Some(&json!({ "id": "thread_9", "agentType": "reviewer", "status": "started" }))
        );
    }

    #[test]
    fn an_unknown_notification_produces_nothing() {
        let items = Items::default();
        // Two codex versions are in play: an unknown method is the normal
        // case, never an error.
        for method in ["thread/reverted", "model/verification", "item/somethingNew"] {
            assert!(feed_updates(&items, method, &json!({ "turnId": "turn_1" })).is_empty());
        }
        // An item type we do not know is just as ordinary.
        let params = json!({ "item": { "id": "item_7", "type": "imageGeneration" } });
        assert!(feed_updates(&items, "item/completed", &params).is_empty());
    }

    #[test]
    fn a_prompt_becomes_codex_user_input() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("fix the test")),
            ContentBlock::ResourceLink(
                agent_client_protocol::schema::v1::ResourceLink::new("lib.rs", "file:///work/lib.rs"),
            ),
        ];
        let input = prompt_input(&blocks);
        assert_eq!(input[0], json!({ "type": "text", "text": "fix the test", "text_elements": [] }));
        assert_eq!(input[1]["text"], json!("[@lib.rs](file:///work/lib.rs)"));
    }

    #[test]
    fn the_turn_outcome_follows_a_steer_to_the_new_turn() {
        let mut turns = Turns::default();
        turns.successor.insert("turn_1".to_string(), "turn_2".to_string());
        turns.outcomes.insert("turn_2".to_string(), TurnOutcome::Completed);
        // A prompt parked on the turn that was steered must still resolve.
        assert_eq!(turns.outcome("turn_1"), Some(TurnOutcome::Completed));
        assert_eq!(turns.outcome("turn_3"), None);
        assert_eq!(TurnOutcome::Interrupted.stop_reason(), StopReason::Cancelled);
    }
}
