//! EXP-746 — ClaudeAgent: the user's own `claude` CLI in stream-json mode,
//! presented to the engine as an ACP agent. Owned by lane E2.
//!
//! One child process per session, spawned with the SDK argv
//! ([`claude_wire::claude_argv`]) over [`crate::transport::spawn_lines`], and
//! ONE pump task that owns every frame: stdout frames become `session/update`
//! notifications, `control_request`s become `session/request_permission` /
//! `elicitation/create`, and the ACP requests coming the other way become
//! stdin messages and control requests. Non-obvious invariants, each measured
//! or ported rather than guessed:
//!
//! - `--permission-prompt-tool stdio` is MANDATORY or no `can_use_tool` ever
//!   arrives and the whole permission surface silently dies — and
//!   `--permission-mode` must ALWAYS be pinned: absent, the CLI takes the
//!   USER's settings default, which on an `auto` machine never asks at all.
//! - `keep_alive` is dropped and NEVER answered; an unknown
//!   `request_user_dialog` kind is answered with SILENCE (never a synthesized
//!   cancel), because the CLI treats a missing reply as "this client cannot
//!   render it" and degrades on its own.
//! - Control requests share ONE channel: a control request issued from inside
//!   a hook handler, before that hook is answered, deadlocks the CLI. Every
//!   hook answer is therefore written BEFORE the work it triggers is spawned.
//! - The argv keeps `--settings <claude-hooks/<pid>/<sid>.settings.json>` even
//!   though the file is `{}`: that path in the process command line is the
//!   reaper's only way to find an escaped claude (EXP-300).
//! - Nothing raw reaches the wire from here. The relay-facing derivation,
//!   redaction and caps live in the engine's mapper; this adapter's job is to
//!   produce HONEST ACP updates, including rebuilding `Read` results from
//!   their structured output so `<system-reminder>` blocks never leave the
//!   machine.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    AgentCapabilities, AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate,
    CancelNotification, CompactionId, CompactionStatus, CompactionUpdate, ConfigOptionUpdate,
    Content, ContentBlock, ContentChunk, Cost, CreateElicitationRequest, CurrentModeUpdate, Diff, ElicitationAction, ElicitationContentValue, ElicitationFormMode,
    ElicitationPropertySchema, ElicitationSchema, ElicitationSessionScope, EnumOption, Implementation,
    InitializeRequest, InitializeResponse, ListSessionsRequest, ListSessionsResponse,
    LoadSessionRequest, LoadSessionResponse, MessageId, MultiSelectPropertySchema, NewSessionRequest,
    NewSessionResponse, PermissionOption, PermissionOptionId, PermissionOptionKind, Plan, PlanEntry,
    PlanEntryPriority, PlanEntryStatus, PromptCapabilities, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, ResumeSessionRequest,
    ResumeSessionResponse, SessionCapabilities, SessionConfigId, SessionConfigOption,
    SessionConfigOptionValue, SessionId,
    SessionInfo, SessionListCapabilities, SessionMode, SessionModeId, SessionModeState,
    SessionNotification, SessionResumeCapabilities, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse, StopReason,
    StringPropertySchema, TextContent, ToolCall, ToolCallContent, ToolCallId, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, UnstructuredCommandInput,
    UsageUpdate,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Agent, Client, ConnectTo, ConnectionTo, Error,
};
use serde_json::{json, Map, Value};

use super::claude_wire::{self as wire, ClaudeArgs, ClaudeOut, McpConfig, SystemSubtype, TurnOutcome};
use super::AdapterSpec;
use crate::session::{EngineError, ResumeHandle};
use crate::transport::{spawn_lines, ChildLines, StderrPolicy};

/// `_meta` key carrying a subagent edge — the engine's own
/// [`crate::SUBAGENT_META_KEY`]. ACP v1 has no subagent update, and the
/// relay's `subagent` card predates ACP by a year, so the adapter stamps the
/// edge onto a no-op `ToolCallUpdate` for the spawning tool call and the
/// engine's mapper reads it back out. Shape:
/// `{"id": <spawning tool_use id>, "agentType": <subagent type>, "status": started|completed|failed}`.
/// The id is the SPAWNING TOOL CALL (falling back to claude's task id) so it
/// matches the `subagentId` its nested tool calls carry — the PTY path's
/// `attribute_to_card` remap, done at the source.
pub use crate::local::SUBAGENT_META_KEY;

/// `_meta` key naming the subagent a message or tool call belongs to
/// (claude's `parent_tool_use_id`) — the engine's `subagentId`.
pub use crate::local::SUBAGENT_ID_META_KEY as PARENT_TOOL_CALL_META_KEY;

/// `_meta` key marking a prompt this adapter injected (EXP-772).
pub use crate::local::INJECTED_PROMPT_META_KEY;

/// The subagents the CLI ships. They are spawned by the model, never picked
/// for the main thread, so the `agent` option offers only what the user (or a
/// plugin) configured.
const BUILTIN_AGENT_NAMES: [&str; 5] =
    ["claude", "general-purpose", "Explore", "Plan", "statusline-setup"];

/// The config option ids `session/set_config_option` still ACCEPTS. EXP-772
/// retired the chips themselves — nothing advertises or sends these any more
/// (`config_options` is empty), and `mode` is gone from the vocabulary
/// entirely: modes ride the ACP-native `session/set_mode` lane alone.
const CONFIG_MODEL: &str = "model";
const CONFIG_EFFORT: &str = "effort";
const CONFIG_FAST: &str = "fast";
const CONFIG_AGENT: &str = "agent";

/// The value that means "whatever the CLI would pick" for effort and agent.
const CONFIG_DEFAULT_VALUE: &str = "default";

/// How long any control request waits for its response before the caller
/// gives up on it (`session/new` then seeds the session from `system/init`
/// alone). The spike measured `initialize` at 0.6-1.8 s normally and ~25 s
/// with an unreachable MCP endpoint — the CLI waits on the MCP handshake
/// before answering — so the budget is generous on purpose: a slow init is
/// not a dead CLI.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(90);

/// How long a live background task may hold back the settlement of the turn
/// that spawned it. The CLI drops `task_notification`s — 14 of 24 background
/// agents in one measured run never reported a terminal status — and before
/// EXP-780 a single dropped one deferred EVERY later turn forever, so
/// `session/prompt` never answered and the run wedged on "Working…". The
/// stall watchdog cannot rescue that: other work keeps resetting its
/// `last_activity`.
const TASK_MAX_LIFETIME: Duration = Duration::from_secs(600);

/// The re-prompt that carries a plan into a fresh context after the user
/// picked one of the "clear context" plan options.
const PLAN_RESTART_PROMPT: &str = "Implement the following plan:";

pub struct ClaudeAgent {
    spec: AdapterSpec,
}

impl ClaudeAgent {
    pub fn new(spec: AdapterSpec) -> Result<ClaudeAgent, EngineError> {
        Ok(ClaudeAgent { spec })
    }
}

impl ConnectTo<Client> for ClaudeAgent {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        async move {
            let session = Arc::new(ClaudeSession::new(self.spec));
            let main_session = session.clone();
            let on_initialize = session.clone();
            let on_new = session.clone();
            let on_load = session.clone();
            let on_resume = session.clone();
            let on_list = session.clone();
            let on_prompt = session.clone();
            let on_mode = session.clone();
            let on_config = session.clone();
            let on_cancel = session.clone();
            Agent
                .builder()
                .name("exponential-claude")
                .on_receive_request(
                    async move |request: InitializeRequest, responder, _cx| {
                        responder.respond(on_initialize.initialize(&request))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: NewSessionRequest, responder, cx: ConnectionTo<Client>| {
                        // Spawn + handshake takes seconds; a handler that waits
                        // for it inline blocks every further message on the
                        // connection, `$/cancel_request` included.
                        let session = on_new.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            if request.cwd != session.spec.cwd {
                                // The child is already pinned to the prepared
                                // worktree; a different cwd here would mean the
                                // launcher and the client disagree about which
                                // tree this run edits.
                                log::warn!(
                                    "engine: claude session/new cwd {} is not the prepared worktree {}",
                                    request.cwd.display(),
                                    session.spec.cwd.display()
                                );
                            }
                            let started = session.start(&spawned, None).await;
                            match started {
                                Ok(()) => responder.respond(
                                    NewSessionResponse::new(session.session_id.clone())
                                        .modes(session.mode_state())
                                        .config_options(session.config_options()),
                                ),
                                Err(error) => responder.respond_with_error(error),
                            }
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: LoadSessionRequest, responder, cx: ConnectionTo<Client>| {
                        let session = on_load.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            // A load replays a transcript off disk: no child,
                            // no turn, nothing to kill. The child is spawned
                            // lazily if the caller then prompts (D6's Replay
                            // tab never does).
                            session.replay_history(&spawned, &request.session_id);
                            responder.respond(
                                LoadSessionResponse::new()
                                    .modes(session.mode_state())
                                    .config_options(session.config_options()),
                            )
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: ResumeSessionRequest, responder, cx: ConnectionTo<Client>| {
                        let session = on_resume.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            let resume = request.session_id.0.to_string();
                            match session.start(&spawned, Some(&resume)).await {
                                Ok(()) => responder.respond(
                                    ResumeSessionResponse::new()
                                        .modes(session.mode_state())
                                        .config_options(session.config_options()),
                                ),
                                Err(error) => responder.respond_with_error(error),
                            }
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: ListSessionsRequest, responder, _cx| {
                        let cwd = request.cwd.clone().unwrap_or_else(|| on_list.cwd().to_path_buf());
                        let sessions = transcript_sessions(&on_list.spec.spawn.env, &cwd);
                        responder.respond(ListSessionsResponse::new(sessions))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: PromptRequest, responder, cx: ConnectionTo<Client>| {
                        let session = on_prompt.clone();
                        let spawned = cx.clone();
                        // Read synchronously, before the spawn: a cancel that
                        // arrives while the turn task is still queued belongs
                        // to THIS turn.
                        let epoch = session.lock().cancel_epoch;
                        // The turn outlives this handler by design: `Cancel`
                        // has to be dispatchable while it runs.
                        cx.spawn(async move {
                            match session.prompt(&spawned, request, epoch).await {
                                Ok(response) => responder.respond(response),
                                Err(error) => responder.respond_with_error(error),
                            }
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionModeRequest, responder, cx: ConnectionTo<Client>| {
                        let session = on_mode.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            match session.set_mode(&spawned, &request.mode_id.0).await {
                                Ok(()) => responder.respond(SetSessionModeResponse::new()),
                                Err(error) => responder.respond_with_error(error),
                            }
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionConfigOptionRequest,
                                responder,
                                cx: ConnectionTo<Client>| {
                        let session = on_config.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
                            match session
                                .set_config(&spawned, &request.config_id, &request.value)
                                .await
                            {
                                Ok(options) => {
                                    responder.respond(SetSessionConfigOptionResponse::new(options))
                                }
                                Err(error) => responder.respond_with_error(error),
                            }
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_notification(
                    async move |_notification: CancelNotification, _cx| {
                        on_cancel.cancel();
                        Ok(())
                    },
                    on_receive_notification!(),
                )
                // NOT `connect_to`: its main_fn waits only on the client, and
                // the host's loop waits only on us, so a claude that died
                // would strand both halves (EXP-746 review E1). The child's
                // EOF closes the connection here.
                .connect_with(client, async move |cx: ConnectionTo<Client>| {
                    let session = main_session;
                    crate::host::until_either_closes(
                        &cx,
                        &session.spec.exit,
                        session.child_gone(),
                    )
                    .await;
                    Ok(())
                })
                .await
        }
    }
}

// ---------------------------------------------------------------------------
// session state
// ---------------------------------------------------------------------------

struct ClaudeSession {
    spec: AdapterSpec,
    /// The ACP session id, which IS the uuid pinned on the argv as
    /// `--session-id` — so `acp_session_id` and claude's own transcript name
    /// are the same string and `--resume=<acp id>` reopens exactly this run.
    session_id: SessionId,
    /// The child's stdout EOF, as a future the connection's `main_fn` waits
    /// on. Held as the RECEIVER of a rendezvous channel whose sender the pump
    /// drops: a receiver with no senders resolves immediately and forever, so
    /// the edge survives whoever asks for it late.
    gone: flume::Receiver<()>,
    /// The pump's half of `gone`, dropped when stdout ends.
    gone_gate: Mutex<Option<flume::Sender<()>>>,
    /// EXP-766: serializes [`ClaudeSession::start`]. The child check and the
    /// child store cannot be one atomic step (a spawn plus a handshake sits
    /// between them), so two prompts racing on a loaded session used to spawn
    /// two CLIs, the second silently orphaning the first.
    start_gate: tokio::sync::Mutex<()>,
    state: Mutex<State>,
}

/// EXP-761: the session's context window, from ONE source per session. The
/// heuristic (`wire::infer_context_window`, off the model id) only bridges the
/// gap until the first `result` reports the real number; that report is then
/// final — a later result's `modelUsage` is per turn and can name only the
/// haiku helper (200000), so re-deriving on every result made the published
/// `usage.contextSize` alternate within one run. A window reported for the
/// session's OWN model id is exact and locks immediately; one taken from the
/// map's largest entry (the id spelled differently) is kept until an exact
/// one shows up, never re-derived per turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ContextWindow {
    #[default]
    Unknown,
    /// Guessed from the model id; replaced by the first report.
    Inferred(u64),
    /// From `result.modelUsage`; `exact` = the session model's own entry.
    Reported { window: u64, exact: bool },
}

impl ContextWindow {
    fn size(self) -> u64 {
        match self {
            ContextWindow::Unknown => 0,
            ContextWindow::Inferred(window) | ContextWindow::Reported { window, .. } => window,
        }
    }

    /// The heuristic, taken only while nothing at all is known.
    fn infer(&mut self, model: &str) {
        if *self == ContextWindow::Unknown {
            *self = ContextWindow::Inferred(wire::infer_context_window(model));
        }
    }

    /// A live model switch (`set_config`): the locked window belonged to the
    /// model that just went away, so it is dropped and re-derived from the
    /// new id. Without this the 1M ↔ 200k switch published the OLD size for
    /// the rest of the run — [`Self::infer`] is a no-op once anything is
    /// known and [`Self::report`] is final once exact.
    fn switch_model(&mut self, model: &str) {
        *self = ContextWindow::Unknown;
        self.infer(model);
    }

    /// An authoritative report; the first exact one is final.
    fn report(&mut self, report: wire::ContextWindowReport) {
        if matches!(self, ContextWindow::Reported { exact: true, .. }) {
            return;
        }
        if matches!(self, ContextWindow::Reported { exact: false, .. }) && !report.exact {
            return;
        }
        *self = ContextWindow::Reported { window: report.window, exact: report.exact };
    }
}

#[derive(Default)]
struct State {
    /// The live child. Held whole: dropping `ChildLines` kills the process
    /// group, which is what keeps an ACP claude from escaping (EXP-300).
    child: Option<Arc<ChildLines>>,
    client_supports_form_elicitation: bool,
    /// Waiters for `control_response` frames, keyed by request id.
    pending_control: HashMap<String, flume::Sender<wire::ControlResp>>,
    /// Control requests the CLI cancelled while we were still answering them.
    aborted_requests: HashSet<String>,
    /// EXP-758: the control requests we are STILL answering. A cancel for one
    /// of these is worth remembering; a cancel that trails an answer already
    /// sent is not, and recording it anyway grew `aborted_requests` by one id
    /// per late cancel for the life of the run.
    answering: HashSet<String>,
    /// EXP-758: a `session/cancel` that arrived before the child existed (the
    /// window between a prompt spawning its task and `start` returning). The
    /// interrupt is delivered as soon as there IS a process to send it to.
    interrupt_pending: bool,
    /// One settle channel per in-flight `session/prompt`, oldest first: claude
    /// emits one `result` per turn, so the front of the queue owns the next.
    turns: VecDeque<flume::Sender<TurnOutcome>>,
    /// An outcome held back while background subagents of that turn are still
    /// live — settling at the `result` would strand their permission requests
    /// on an RPC nobody answers.
    deferred: Option<DeferredSettle>,
    native_session_id: Option<String>,
    model: String,
    models: Vec<wire::ModelInfo>,
    custom_agents: Vec<String>,
    agent: Option<String>,
    /// `None` = the CLI's own default (the effort flag layer is unset).
    effort: Option<String>,
    /// Launched with `--effort ultracode`: a real hidden level (measured), so
    /// it stays offered for the session that started on it.
    ultracode: bool,
    fast: bool,
    fast_supported: bool,
    mode: String,
    commands: Vec<wire::CommandRow>,
    /// The catalog as the CLI reported it, kept because the terminal-only
    /// names arrive LATER (on `system/init`) and re-filter it.
    raw_commands: Vec<wire::SlashCommandInfo>,
    terminal_commands: Vec<String>,
    tools: HashMap<String, ToolEntry>,
    /// Text/thinking blocks already streamed, per message id, in document
    /// order — the consolidated `assistant` message forwards only what did
    /// NOT stream.
    streamed: HashMap<String, BTreeMap<u64, StreamedBlock>>,
    /// The message id the current `content_block_*` frames belong to, per
    /// parent tool call (a subagent streams beside the main thread).
    current_message: HashMap<String, String>,
    usage: wire::TokenSnapshot,
    context_window: ContextWindow,
    compaction: Option<String>,
    tasks: HashMap<String, TaskEntry>,
    /// Bumped by every `session/prompt`. Tags the tasks a turn spawns so a
    /// stale one cannot defer a later turn (EXP-780).
    turn_seq: u64,
    /// Set while a defer timer is in flight, so one deferral arms one timer.
    defer_timer_armed: bool,
    plan_tasks: BTreeMap<String, PlanTask>,
    delivered_text: bool,
    local_only_command: bool,
    /// The plan a "clear context" approval is waiting to re-prompt with.
    pending_plan_restart: Option<PlanRestart>,
    /// Set while the `/clear` the restart injects has not reported its own
    /// `result` yet. Discriminated by output tokens rather than by counting
    /// results: a local command does no model work, so a result with tokens is
    /// always the plan turn and settles even when a future CLI stops
    /// answering `/clear` at all.
    skip_local_command_result: bool,
    cancelled: bool,
    /// Bumped by every cancel. A `session/prompt` records the epoch when its
    /// handler was DISPATCHED, so a cancel that lands between the dispatch and
    /// the turn's first stdin write still cancels that turn instead of being
    /// cleared by it — the CLI's own prewait latch, ported.
    cancel_epoch: u64,
    closed: bool,
}

/// One `result`'s settlement, kept whole so a deferral (a live subagent)
/// replays it exactly as the `result` reported it — the fold-in count
/// included.
#[derive(Clone, Copy)]
struct DeferredSettle {
    outcome: TurnOutcome,
    /// The `result`'s `queued_turn_count`; `None` = the CLI did not say.
    queued: Option<u64>,
}

/// Settle the turn this `result` ends, plus every mid-turn prompt the CLI
/// FOLDED INTO it. `queued` is the result's own `queued_turn_count`: what the
/// CLI still holds behind this turn. Anything the queue keeps beyond that
/// count was answered by THIS result and gets none of its own, so it settles
/// here — a stranded steer would otherwise hang its `session/prompt` forever
/// and leave every later turn settling the channel in front of it (EXP-746).
/// A CLI that reports no count folds nothing in: each turn waits.
fn settle_turns(state: &mut State, settle: DeferredSettle) {
    // A settled turn's dead tasks can never matter again: drop them, or a
    // long run's `tasks` map grows for the life of the process.
    let turn_seq = state.turn_seq;
    state.tasks.retain(|_, task| task.live || task.turn_seq == turn_seq);
    if let Some(turn) = state.turns.pop_front() {
        let _ = turn.send(settle.outcome);
    }
    let Some(queued) = settle.queued else { return };
    while state.turns.len() as u64 > queued {
        match state.turns.pop_front() {
            Some(turn) => {
                let _ = turn.send(settle.outcome);
            }
            None => break,
        }
    }
}

/// The tasks that may still hold back a settlement: LIVE, spawned by the turn
/// now running, and younger than [`TASK_MAX_LIFETIME`]. Everything else is a
/// task the CLI stopped talking about, and waiting on one of those is the
/// "Working…" wedge (EXP-780).
fn blocking_tasks(state: &State) -> impl Iterator<Item = (&String, &TaskEntry)> {
    let turn_seq = state.turn_seq;
    state.tasks.iter().filter(move |(_, task)| {
        task.live && task.turn_seq == turn_seq && task.started_at.elapsed() < TASK_MAX_LIFETIME
    })
}

struct ToolEntry {
    name: String,
    input: Value,
    surfaced: bool,
}

struct StreamedBlock {
    thinking: bool,
    text: String,
}

struct TaskEntry {
    tool_use_id: Option<String>,
    subagent_type: Option<String>,
    live: bool,
    /// `task_started.is_backgrounded`: the model did NOT stop for this one, so
    /// the main thread keeps running (and asking) beside it.
    backgrounded: bool,
    /// The turn that spawned it ([`State::turn_seq`]). A task only ever
    /// defers ITS OWN turn: without this, one task the CLI forgot about
    /// blocked every turn the session would ever run.
    turn_seq: u64,
    /// When `task_started` arrived; a task past [`TASK_MAX_LIFETIME`] stops
    /// blocking and is published as `failed` so its card stops spinning.
    started_at: Instant,
    /// The status last PUBLISHED for this task. `task_notification` and
    /// `task_updated` share an arm and the CLI often sends both for one edge
    /// (7 duplicate `completed`s in 54 edges, measured), which surfaced as a
    /// second completed subagent row.
    last_status: Option<String>,
}

struct PlanTask {
    subject: String,
    status: String,
    active_form: Option<String>,
}

struct PlanRestart {
    plan: String,
    mode: String,
}

impl ClaudeSession {
    fn new(spec: AdapterSpec) -> ClaudeSession {
        // The ACP session id is minted here rather than taken from the
        // `coding_sessions` row: it doubles as claude's `--session-id`, which
        // must be a uuid the CLI has never seen.
        let session_id = match &spec.resume {
            Some(ResumeHandle::Acp(id)) | Some(ResumeHandle::Native(id)) => id.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };
        let options = &spec.options;
        let state = State {
            model: options.model.trim().to_string(),
            effort: (!options.effort.trim().is_empty()).then(|| options.effort.trim().to_string()),
            ultracode: options.ultracode,
            mode: if options.plan_mode {
                "plan".to_string()
            } else {
                "bypassPermissions".to_string()
            },
            ..State::default()
        };
        let (gone_gate, gone) = flume::bounded(0);
        ClaudeSession {
            spec,
            session_id: SessionId::new(session_id),
            gone,
            gone_gate: Mutex::new(Some(gone_gate)),
            start_gate: tokio::sync::Mutex::new(()),
            state: Mutex::new(state),
        }
    }

    fn cwd(&self) -> &Path {
        &self.spec.cwd
    }

    /// Resolves when the child's stdout ends — EOF, a crash, an external kill
    /// — and never for a session that has no child at all (a `session/load`
    /// replay), which ends when the client closes instead.
    async fn child_gone(&self) {
        let _ = self.gone.recv_async().await;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A poisoned lock means a panic inside a handler; the session is
        // still better off continuing on the state it had than dying.
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn initialize(&self, request: &InitializeRequest) -> InitializeResponse {
        let supports_form = request
            .client_capabilities
            .elicitation
            .as_ref()
            .is_some_and(|elicitation| elicitation.supports_form());
        self.lock().client_supports_form_elicitation = supports_form;
        InitializeResponse::new(ProtocolVersion::V1)
            .agent_info(Implementation::new("claude", env!("CARGO_PKG_VERSION")))
            .agent_capabilities(
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

    // -----------------------------------------------------------------------
    // spawn + handshake
    // -----------------------------------------------------------------------

    /// Spawn the CLI (once), start the pump, and run the `initialize` control
    /// request. `resume` reopens a recorded conversation, in which case the
    /// fresh `--session-id` pin is dropped (claude refuses both).
    ///
    /// EXP-766: serialized on `start_gate`. Two prompts can reach this at the
    /// same time on a loaded session (the `session/load` → prompt path spawns
    /// the child lazily), and the check and the store are not one step, so a
    /// second caller has to WAIT for the first rather than race it.
    async fn start(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        resume: Option<&str>,
    ) -> Result<(), Error> {
        if self.lock().child.is_some() {
            return Ok(());
        }
        let _starting = self.start_gate.lock().await;
        // Re-checked under the gate: the caller we queued behind may have
        // spawned the child while we waited.
        if self.lock().child.is_some() {
            return Ok(());
        }
        let child = Arc::new(self.spawn_child(resume).map_err(|error| {
            Error::internal_error()
                .data(json!({ "reason": format!("could not start claude: {error}") }))
        })?);
        child.forward_exit(&self.spec.exit);
        {
            let mut state = self.lock();
            state.child = Some(child.clone());
            state.closed = false;
        }
        let lines = child.lines.clone();
        let pump_cx = cx.clone();
        let pump = self.clone();
        cx.spawn(async move {
            pump.pump(lines, pump_cx).await;
            Ok(())
        })?;

        // The `initialize` response is the CLI's only report of its command
        // catalog, model list and custom agents. A CLI that never answers is
        // not fatal: the session seeds from `system/init` instead.
        match self.control_request(wire::initialize(wire::initialize_hooks())).await {
            Ok(response) => {
                let info: wire::InitializeInfo =
                    serde_json::from_value(response.response.clone()).unwrap_or_default();
                self.apply_initialize_info(info);
            }
            Err(error) => log::warn!("engine: claude initialize did not answer: {error}"),
        }
        Ok(())
    }

    fn spawn_child(&self, resume: Option<&str>) -> std::io::Result<ChildLines> {
        let state = self.lock();
        let model = state.model.clone();
        let effort = if state.ultracode {
            Some("ultracode".to_string())
        } else {
            state.effort.clone()
        };
        let mode = state.mode.clone();
        let disallow_ask = !state.client_supports_form_elicitation;
        drop(state);

        let inline_mcp;
        let mcp = match &self.spec.mcp {
            coding::AgentMcp::ClaudeInline { url, session_id } => {
                inline_mcp = wire::inline_mcp_config(url, session_id.as_deref());
                Some(McpConfig::Inline(&inline_mcp))
            }
            // The PTY path's `.exp-mcp.json`, kept as the zero-risk fallback.
            coding::AgentMcp::ClaudeFile => {
                inline_mcp = self
                    .spec
                    .cwd
                    .join(coding::mcp_json::MCP_JSON_FILE)
                    .display()
                    .to_string();
                Some(McpConfig::File(Path::new(&inline_mcp)))
            }
            _ => None,
        };
        // A launch that carries a resume seed reopens THAT conversation even
        // when the host called `session/new`: the recorded id is already taken,
        // so pinning it as a fresh `--session-id` would be refused.
        let recorded = match &self.spec.resume {
            Some(ResumeHandle::Acp(id)) | Some(ResumeHandle::Native(id)) => Some(id.as_str()),
            Some(ResumeHandle::PiSessionFile(_)) | None => None,
        };
        let resume = resume.or(recorded);
        // Without `elicitation.form` the model must never pick
        // AskUserQuestion: there would be nothing to render its form with.
        let disallowed: &[&str] = if disallow_ask { &["AskUserQuestion"] } else { &[] };
        let argv = wire::claude_argv(&ClaudeArgs {
            print_mode: wire::CLAUDE_PRINT_MODE,
            model: (!model.is_empty()).then_some(model.as_str()),
            effort: effort.as_deref(),
            // ALWAYS pinned (measured): with the flag absent the CLI takes the
            // user's own settings default and an `auto` machine never asks.
            permission_mode: Some(wire::argv_permission_mode(&mode)),
            // EXP-772: ALWAYS. Permissions are bypassed in every mode, plan
            // included — a plan launch keeps `--permission-mode plan` for the
            // planning behaviour, and the mode switch the plan approval makes
            // ("bypassPermissions") is only accepted when the flag was there
            // at spawn.
            allow_dangerous: true,
            session_id: Some(self.session_id.0.as_ref()),
            resume,
            fork_session: false,
            mcp_config: mcp,
            strict_mcp_config: mcp.is_some(),
            settings: self.spec.reaper_settings_path.as_deref(),
            add_dirs: &[],
            disallowed_tools: disallowed,
            // EXP-763: the run playbook, on every start and resume.
            append_system_prompt: Some(coding::skill::RUN_SKILL),
        });
        let mut spawn = self.spec.spawn.clone().args(argv);
        for (key, value) in wire::extra_env() {
            spawn = spawn.env(key, value);
        }
        spawn_lines(&spawn, StderrPolicy::Log)
    }

    fn apply_initialize_info(&self, info: wire::InitializeInfo) {
        let mut state = self.lock();
        if !info.commands.is_empty() {
            state.commands = wire::available_commands(&info.commands, &state.terminal_commands);
            state.raw_commands = info.commands.clone();
        }
        if !info.models.is_empty() {
            if state.model.is_empty() {
                state.model = info.models[0].value.clone();
            }
            state.models = info.models;
        }
        state.custom_agents = info
            .agents
            .iter()
            .filter_map(|agent| match agent {
                Value::String(name) => Some(name.clone()),
                Value::Object(fields) => fields
                    .get("name")
                    .or_else(|| fields.get("agent_type"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                _ => None,
            })
            // The CLI reports its own built-in subagents beside the user's;
            // offering those as a "run as" pick would be a lie (they are
            // spawned by the model, never selected for the main thread).
            .filter(|agent| !BUILTIN_AGENT_NAMES.contains(&agent.as_str()))
            .collect();
        state.fast_supported = info.fast_mode_state.is_some();
        state.fast = info.fast_mode_state.as_deref() == Some("on");
        // 2.1.263 reports no effort on `system/init`; the launch flag is the
        // only truth until the user picks one (spike finding #7).
        if let Some(effort) = info.effort {
            state.effort = (effort != CONFIG_DEFAULT_VALUE).then_some(effort);
        }
    }

    // -----------------------------------------------------------------------
    // control protocol
    // -----------------------------------------------------------------------

    fn send(&self, value: Value) -> Result<(), Error> {
        let child = self.lock().child.clone();
        let Some(child) = child else {
            return Err(Error::internal_error()
                .data(json!({ "reason": "the claude process is not running" })));
        };
        child
            .writer
            .write_line(&value.to_string())
            .map_err(|error| Error::internal_error().data(json!({ "reason": error.to_string() })))
    }

    /// Issue one control request and await its response. Never called from a
    /// hook handler before that hook's own answer went out — the CLI
    /// serializes control traffic on one channel and would deadlock.
    async fn control_request(&self, request: Value) -> Result<wire::ControlResp, Error> {
        let request_id = wire::new_request_id();
        let (tx, rx) = flume::bounded(1);
        self.lock().pending_control.insert(request_id.clone(), tx);
        if let Err(error) = self.send(wire::control_request(&request_id, request)) {
            self.lock().pending_control.remove(&request_id);
            return Err(error);
        }
        let response = match tokio::time::timeout(CONTROL_TIMEOUT, rx.recv_async()).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) | Err(_) => {
                self.lock().pending_control.remove(&request_id);
                return Err(Error::internal_error()
                    .data(json!({ "reason": "claude did not answer a control request" })));
            }
        };
        if !response.is_success() {
            let message = response.error.clone().unwrap_or_else(|| "control request failed".into());
            return Err(Error::internal_error().data(json!({ "reason": message })));
        }
        Ok(response)
    }

    fn cancel(&self) {
        let mut state = self.lock();
        state.cancelled = true;
        state.cancel_epoch = state.cancel_epoch.wrapping_add(1);
        let closed = state.closed;
        drop(state);
        if closed {
            // A finished query rejects `interrupt` — the upstream's
            // `queryClosed` guard, ported.
            return;
        }
        if let Err(error) = self.send(wire::control_request(
            &wire::new_request_id(),
            wire::interrupt(true),
        )) {
            // EXP-758: the usual reason is that the child is not up YET (a
            // cancel racing a prompt's lazy spawn), and a cancel dropped
            // there leaves the turn it meant to stop running. Remember it;
            // `start` delivers it the moment there is a process.
            log::warn!("engine: claude interrupt deferred: {error}");
            self.lock().interrupt_pending = true;
        }
    }

    /// EXP-758: deliver a cancel that could not reach a child that did not
    /// exist yet. Called once the turn it belongs to is really running, which
    /// is the only moment the CLI can act on an interrupt.
    ///
    /// The flag is ALWAYS consumed: a deferred interrupt that no longer
    /// applies (a cancel with no turn behind it, then an unrelated prompt)
    /// must not fire at some later turn. `cancelled` is the test for "still
    /// applies": `prompt` recomputes it from the cancel epoch, so it is true
    /// exactly when the cancel landed after this turn was dispatched.
    fn deliver_pending_interrupt(&self) {
        let mut state = self.lock();
        let deferred = std::mem::take(&mut state.interrupt_pending);
        let deliver = deferred && state.cancelled && !state.closed;
        drop(state);
        if !deliver {
            return;
        }
        if let Err(error) = self.send(wire::control_request(
            &wire::new_request_id(),
            wire::interrupt(true),
        )) {
            log::warn!("engine: claude interrupt failed: {error}");
        }
    }

    // -----------------------------------------------------------------------
    // turns
    // -----------------------------------------------------------------------

    async fn prompt(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request: PromptRequest,
        epoch: u64,
    ) -> Result<PromptResponse, Error> {
        let text = prompt_text(&request.prompt);
        // A child spawned lazily here is the `session/load` → prompt path: a
        // replay that the user decided to continue. EXP-758: `/usage` takes
        // the SAME handle. Spawning it fresh with a new `--session-id` forked
        // the conversation, so the next real prompt landed in an empty one.
        let resume = self.lock().child.is_none().then(|| self.session_id.0.to_string());
        // `/usage` is answered from the `get_usage` control request instead of
        // a turn; `get_context_usage` is never sent at all (it stalls ~15 s
        // before the first turn and serializes ahead of an awaited set_model).
        if wire::is_usage_command(&text) {
            self.start(cx, resume.as_deref()).await?;
            let usage = self.control_request(wire::get_usage_without_behaviors()).await?;
            self.notify(
                cx,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(wire::render_usage_markdown(&usage.response)),
                ))),
            );
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }

        self.start(cx, resume.as_deref()).await?;

        let (tx, rx) = flume::bounded(1);
        {
            let mut state = self.lock();
            // A cancel between this prompt's dispatch and here already applies
            // to it; anything older does not.
            state.cancelled = state.cancel_epoch != epoch;
            state.delivered_text = false;
            state.local_only_command = wire::LOCAL_ONLY_COMMANDS
                .iter()
                .any(|command| text.trim() == *command);
            // EXP-780: from here on, a `task_started` belongs to THIS turn.
            state.turn_seq = state.turn_seq.wrapping_add(1);
            state.turns.push_back(tx);
        }
        self.send(claude_user_message(&request.prompt, &text))?;
        // EXP-758: a cancel that raced this turn's lazy spawn had no child to
        // reach. It does now, and the turn it meant to stop is running.
        self.deliver_pending_interrupt();
        let outcome = rx.recv_async().await.unwrap_or(TurnOutcome::EndTurn);
        match outcome {
            TurnOutcome::EndTurn => Ok(PromptResponse::new(StopReason::EndTurn)),
            TurnOutcome::MaxTokens => Ok(PromptResponse::new(StopReason::MaxTokens)),
            TurnOutcome::MaxTurnRequests => Ok(PromptResponse::new(StopReason::MaxTurnRequests)),
            TurnOutcome::Refusal => Ok(PromptResponse::new(StopReason::Refusal)),
            TurnOutcome::Cancelled => Ok(PromptResponse::new(StopReason::Cancelled)),
            // Not a stop reason: a logged-out CLI answers a perfectly ordinary
            // `result/success` whose text is "Not logged in · Please run
            // /login" (measured), and a turn that reports EndTurn there looks
            // like a well-behaved run that said nothing useful.
            TurnOutcome::AuthRequired => Err(Error::auth_required()),
        }
    }

    /// Settle, unless a background subagent THIS turn spawned is still live
    /// (issues #864/#866: settling early strands its permission request).
    fn settle_or_defer(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        state: &mut State,
        settle: DeferredSettle,
    ) {
        self.expire_tasks(cx, state);
        if blocking_tasks(state).next().is_some() {
            state.deferred = Some(settle);
            self.arm_defer_timer(cx, state);
        } else {
            settle_turns(state, settle);
        }
    }

    fn settle_deferred(self: &Arc<Self>, cx: &ConnectionTo<Client>, state: &mut State) {
        self.expire_tasks(cx, state);
        if blocking_tasks(state).next().is_some() {
            return;
        }
        if let Some(settle) = state.deferred.take() {
            settle_turns(state, settle);
        }
    }

    /// Retire every live task past [`TASK_MAX_LIFETIME`], publishing a
    /// `failed` edge for each: the client's subagent card stops spinning and
    /// the mapper drops its per-subagent bookkeeping, which otherwise only
    /// ever clears on a terminal edge the CLI may never send.
    fn expire_tasks(&self, cx: &ConnectionTo<Client>, state: &mut State) {
        let expired: Vec<String> = state
            .tasks
            .iter()
            .filter(|(_, task)| task.live && task.started_at.elapsed() >= TASK_MAX_LIFETIME)
            .map(|(task_id, _)| task_id.clone())
            .collect();
        for task_id in expired {
            let Some(task) = state.tasks.get_mut(&task_id) else { continue };
            task.live = false;
            if task.last_status.as_deref() == Some("failed") {
                continue;
            }
            task.last_status = Some("failed".to_string());
            let tool_use_id = task.tool_use_id.clone();
            let subagent_type = task.subagent_type.clone();
            log::warn!("engine: claude task {task_id} never reported back; retiring it");
            self.publish_subagent(
                cx,
                &task_id,
                tool_use_id.as_deref(),
                subagent_type.as_deref(),
                "failed",
            );
        }
    }

    /// A deferral must not be able to outlive [`TASK_MAX_LIFETIME`] in total
    /// silence: nothing else wakes `settle_deferred` when the CLI simply stops
    /// sending frames for the task it is waiting on.
    fn arm_defer_timer(self: &Arc<Self>, cx: &ConnectionTo<Client>, state: &mut State) {
        if state.defer_timer_armed {
            return;
        }
        state.defer_timer_armed = true;
        // The oldest blocking task bounds the wait; +1 s so the sleep lands
        // strictly past the expiry it is meant to observe.
        let wait = blocking_tasks(state)
            .map(|(_, task)| TASK_MAX_LIFETIME.saturating_sub(task.started_at.elapsed()))
            .max()
            .unwrap_or(TASK_MAX_LIFETIME)
            + Duration::from_secs(1);
        let session = self.clone();
        let out = cx.clone();
        let _ = cx.spawn(async move {
            tokio::time::sleep(wait).await;
            let mut state = session.lock();
            state.defer_timer_armed = false;
            session.settle_deferred(&out, &mut state);
            Ok(())
        });
    }

    // -----------------------------------------------------------------------
    // modes and config options
    // -----------------------------------------------------------------------

    fn mode_state(&self) -> SessionModeState {
        let current = self.lock().mode.clone();
        SessionModeState::new(SessionModeId::new(current), available_modes())
    }

    async fn set_mode(self: &Arc<Self>, cx: &ConnectionTo<Client>, mode: &str) -> Result<(), Error> {
        let mode = clamp_mode(mode);
        self.control_request(wire::set_permission_mode(&mode)).await?;
        self.lock().mode = mode.clone();
        self.notify(
            cx,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(mode))),
        );
        self.publish_config(cx);
        Ok(())
    }

    /// EXP-772: EMPTY. Model / effort / fast / agent pickers left the
    /// mid-session steering UI on every client, so the adapter advertises no
    /// options at all; `set_config` still ACCEPTS the ids an older publisher
    /// may send.
    fn config_options(&self) -> Vec<SessionConfigOption> {
        Vec::new()
    }

    async fn set_config(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        id: &SessionConfigId,
        value: &SessionConfigOptionValue,
    ) -> Result<Vec<SessionConfigOption>, Error> {
        let picked = value.as_value_id().map(|value| value.0.to_string());
        match id.0.as_ref() {
            CONFIG_MODEL => {
                let model = picked.unwrap_or_default();
                self.control_request(wire::set_model(Some(&model))).await?;
                let mut state = self.lock();
                // EXP-761: the window follows the model (1M ↔ 200k), so the
                // one taken for the previous id is dropped here — nothing
                // else in the run ever re-derives it.
                state.context_window.switch_model(&model);
                state.model = model;
                // The CLI persists `/effort` per model since 2.1.243, so a
                // pinned effort carries across the switch and an unpinned one
                // stays unpinned.
                drop(state);
            }
            CONFIG_EFFORT => {
                let level = picked.unwrap_or_else(|| CONFIG_DEFAULT_VALUE.to_string());
                let settings = match level.as_str() {
                    CONFIG_DEFAULT_VALUE => json!({ "effortLevel": Value::Null }),
                    // Ultracode is xhigh plus standing workflow orchestration,
                    // and it is session-scoped: the settings layer is the only
                    // way in mid-session (`--effort` is a spawn flag).
                    "ultracode" => json!({ "effortLevel": "xhigh", "ultracode": true }),
                    other => json!({ "effortLevel": other }),
                };
                self.control_request(wire::apply_flag_settings(settings)).await?;
                let mut state = self.lock();
                state.effort = (level != CONFIG_DEFAULT_VALUE && level != "ultracode")
                    .then(|| level.clone());
                state.ultracode = level == "ultracode";
            }
            CONFIG_FAST => {
                let enabled = value.as_bool().unwrap_or(picked.as_deref() == Some("on"));
                self.control_request(wire::apply_flag_settings(json!({ "fastMode": enabled })))
                    .await?;
                self.lock().fast = enabled;
            }
            CONFIG_AGENT => {
                let agent = picked.filter(|agent| agent != CONFIG_DEFAULT_VALUE);
                self.control_request(wire::apply_flag_settings(json!({ "agent": agent })))
                    .await?;
                self.lock().agent = agent;
            }
            other => {
                return Err(Error::invalid_params()
                    .data(json!({ "reason": format!("unknown config option {other}") })))
            }
        }
        let options = self.config_options();
        self.notify(
            cx,
            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(options.clone())),
        );
        Ok(options)
    }

    // -----------------------------------------------------------------------
    // notifications
    // -----------------------------------------------------------------------

    fn notify(&self, cx: &ConnectionTo<Client>, update: SessionUpdate) {
        let _ = cx.send_notification(SessionNotification::new(self.session_id.clone(), update));
    }

    fn notify_meta(&self, cx: &ConnectionTo<Client>, update: SessionUpdate, meta: Map<String, Value>) {
        let _ = cx.send_notification(
            SessionNotification::new(self.session_id.clone(), update).meta(meta),
        );
    }

    fn publish_commands(&self, cx: &ConnectionTo<Client>) {
        let commands: Vec<AvailableCommand> = self
            .lock()
            .commands
            .iter()
            .map(|command| {
                AvailableCommand::new(command.name.clone(), command.description.clone())
                    .input(command.hint.clone().map(|hint| {
                        AvailableCommandInput::Unstructured(UnstructuredCommandInput::new(hint))
                    }))
            })
            .collect();
        self.notify(
            cx,
            SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
        );
    }

    fn publish_config(&self, cx: &ConnectionTo<Client>) {
        self.notify(
            cx,
            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(self.config_options())),
        );
    }

    fn publish_usage(&self, cx: &ConnectionTo<Client>, used: u64, cost: Option<f64>) {
        let size = self.lock().context_window.size();
        if size == 0 {
            return;
        }
        let mut update = UsageUpdate::new(used, size);
        if let Some(cost) = cost {
            update = update.cost(Cost::new(cost, "USD"));
        }
        self.notify(cx, SessionUpdate::UsageUpdate(update));
    }

    fn publish_plan(&self, cx: &ConnectionTo<Client>) {
        let entries: Vec<PlanEntry> = self
            .lock()
            .plan_tasks
            .values()
            .map(|task| {
                let content = match (task.status.as_str(), &task.active_form) {
                    ("in_progress", Some(active)) => active.clone(),
                    _ => task.subject.clone(),
                };
                PlanEntry::new(content, PlanEntryPriority::Medium, plan_status(&task.status))
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        self.notify(cx, SessionUpdate::Plan(Plan::new(entries)));
    }

    fn publish_subagent(
        &self,
        cx: &ConnectionTo<Client>,
        task_id: &str,
        tool_use_id: Option<&str>,
        agent_type: Option<&str>,
        status: &str,
    ) {
        // The edge rides a no-op patch of the tool call that spawned the
        // subagent, so a client that ignores the meta sees nothing at all.
        let id = tool_use_id.unwrap_or(task_id);
        let mut meta = Map::new();
        meta.insert(
            SUBAGENT_META_KEY.to_string(),
            json!({ "id": id, "agentType": agent_type, "status": status }),
        );
        self.notify_meta(
            cx,
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
                ToolCallUpdateFields::new(),
            )),
            meta,
        );
    }

    // -----------------------------------------------------------------------
    // the pump
    // -----------------------------------------------------------------------

    async fn pump(self: Arc<Self>, lines: flume::Receiver<String>, cx: ConnectionTo<Client>) {
        while let Ok(line) = lines.recv_async().await {
            let frame = ClaudeOut::parse(&line);
            self.on_frame(&cx, frame);
        }
        // stdout closed: the query is over. Settle everything still waiting so
        // no `session/prompt` hangs on a dead process.
        {
            let mut state = self.lock();
            state.closed = true;
            let outcome =
                if state.cancelled { TurnOutcome::Cancelled } else { TurnOutcome::EndTurn };
            state.deferred = None;
            while let Some(turn) = state.turns.pop_front() {
                let _ = turn.send(outcome);
            }
            // EXP-758: and every CONTROL request too. A `set_mode`,
            // `set_config`, `/usage` or `initialize` in flight when the CLI
            // died used to sit out the whole 90 s `CONTROL_TIMEOUT` before
            // its caller learned that nothing was ever going to answer.
            for (request_id, waiter) in std::mem::take(&mut state.pending_control) {
                let _ = waiter.send(wire::ControlResp::failed(&request_id, "claude exited"));
            }
        }
        // Settled FIRST, announced second: `main_fn` closes the connection on
        // this edge, and a turn that settles after the close never reaches the
        // client.
        if let Ok(mut gate) = self.gone_gate.lock() {
            gate.take();
        }
    }

    fn on_frame(self: &Arc<Self>, cx: &ConnectionTo<Client>, frame: ClaudeOut) {
        match frame {
            ClaudeOut::System(system) => self.on_system(cx, system),
            ClaudeOut::Assistant(message) => self.on_assistant(cx, message),
            ClaudeOut::User(message) => self.on_user(cx, message),
            ClaudeOut::StreamEvent(event) => self.on_stream_event(cx, event),
            ClaudeOut::Result(result) => self.on_result(cx, result),
            ClaudeOut::ControlRequest(request) => self.on_control_request(cx, request),
            ClaudeOut::ControlResponse(response) => {
                let waiter = self.lock().pending_control.remove(&response.response.request_id);
                if let Some(waiter) = waiter {
                    let _ = waiter.send(response.response);
                }
            }
            ClaudeOut::ControlCancelRequest(cancel) => {
                // The CLI abandoned a request it sent us: never answer it.
                // EXP-758: unless we already did. A cancel that trails its
                // own answer is noise, and remembering it leaked the id.
                let mut state = self.lock();
                if state.answering.contains(&cancel.request_id) {
                    log::debug!("engine: claude cancelled control request {}", cancel.request_id);
                    state.aborted_requests.insert(cancel.request_id);
                } else {
                    log::debug!(
                        "engine: claude cancelled control request {} after it was answered",
                        cancel.request_id
                    );
                }
            }
            // Answering a keep_alive is a protocol error; unknown frame types
            // are how the CLI ships new features. `rate_limit_event` lands here
            // deliberately: the upstream adapter re-emits its usage snapshot on
            // it, but the numbers are unchanged and the plan windows ride
            // `devices.agent_usage`, not the session feed.
            ClaudeOut::KeepAlive | ClaudeOut::Unknown => {}
        }
    }

    fn on_system(self: &Arc<Self>, cx: &ConnectionTo<Client>, system: wire::SystemMsg) {
        match SystemSubtype::classify(&system.subtype) {
            SystemSubtype::Init => {
                let mut state = self.lock();
                if !system.session_id.is_empty() {
                    state.native_session_id = Some(system.session_id.clone());
                }
                if !system.model.is_empty() {
                    state.model = system.model.clone();
                    state.context_window.infer(&system.model);
                }
                let mode_changed = match init_mode(system.permission_mode.as_deref(), &state.mode) {
                    Some(mode) => {
                        state.mode = mode;
                        true
                    }
                    None => false,
                };
                state.terminal_commands = system
                    .extra
                    .get("terminal_slash_commands")
                    .and_then(Value::as_array)
                    .map(|names| {
                        names.iter().filter_map(Value::as_str).map(str::to_string).collect()
                    })
                    .unwrap_or_default();
                if state.raw_commands.is_empty() && !system.slash_commands.is_empty() {
                    // Names only, until the initialize response lands — still
                    // enough for the `/` menu to offer them.
                    state.raw_commands = system
                        .slash_commands
                        .iter()
                        .map(|name| wire::SlashCommandInfo {
                            name: name.clone(),
                            ..wire::SlashCommandInfo::default()
                        })
                        .collect();
                }
                // Re-filter: the terminal-only names are an init-frame fact,
                // and the initialize response that seeded the catalog did not
                // have them yet.
                let terminal = state.terminal_commands.clone();
                let raw = state.raw_commands.clone();
                state.commands = wire::available_commands(&raw, &terminal);
                drop(state);
                self.publish_commands(cx);
                self.publish_config(cx);
                if mode_changed {
                    let mode = self.lock().mode.clone();
                    self.notify(
                        cx,
                        SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(
                            SessionModeId::new(mode),
                        )),
                    );
                }
            }
            SystemSubtype::Status => {
                if system.status.as_deref() == Some("compacting") {
                    let mut state = self.lock();
                    if state.compaction.is_none() {
                        let id = uuid::Uuid::new_v4().to_string();
                        state.compaction = Some(id.clone());
                        drop(state);
                        self.notify(
                            cx,
                            SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                                CompactionId::new(id),
                                CompactionStatus::InProgress,
                            )),
                        );
                    }
                }
            }
            SystemSubtype::CompactBoundary => {
                let mut state = self.lock();
                let id = state
                    .compaction
                    .take()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let post_tokens = system
                    .compact_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.post_tokens)
                    .unwrap_or(0);
                let trigger = system
                    .compact_metadata
                    .as_ref()
                    .map(|metadata| metadata.trigger.clone())
                    .unwrap_or_default();
                // Compaction frees occupancy, it does not change the window.
                state.usage = wire::TokenSnapshot { input: post_tokens, ..Default::default() };
                drop(state);
                let mut meta = Map::new();
                meta.insert(crate::local::COMPACTION_TRIGGER_META_KEY.to_string(), json!(trigger));
                self.notify_meta(
                    cx,
                    SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                        CompactionId::new(id),
                        CompactionStatus::Completed,
                    )),
                    meta,
                );
                self.publish_usage(cx, post_tokens, None);
            }
            SystemSubtype::SessionStateChanged => {
                let state_name = system.extra.get("state").and_then(Value::as_str).unwrap_or("");
                if state_name == "idle" {
                    let mut state = self.lock();
                    self.settle_deferred(cx, &mut state);
                }
            }
            SystemSubtype::TaskStarted => {
                let Some(task_id) = system.task_id.clone() else { return };
                let tool_use_id = system
                    .extra
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let subagent_type = system
                    .extra
                    .get("subagent_type")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let backgrounded = system
                    .extra
                    .get("is_backgrounded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let mut state = self.lock();
                let turn_seq = state.turn_seq;
                state.tasks.insert(
                    task_id.clone(),
                    TaskEntry {
                        tool_use_id: tool_use_id.clone(),
                        subagent_type: subagent_type.clone(),
                        live: true,
                        backgrounded,
                        turn_seq,
                        started_at: Instant::now(),
                        last_status: Some("started".to_string()),
                    },
                );
                drop(state);
                self.publish_subagent(
                    cx,
                    &task_id,
                    tool_use_id.as_deref(),
                    subagent_type.as_deref(),
                    "started",
                );
            }
            SystemSubtype::TaskNotification | SystemSubtype::TaskUpdated => {
                let Some(task_id) = system.task_id.clone() else { return };
                // `task_notification` puts the status in the TYPED `status`
                // field (`system/status` shares the name), `task_updated`
                // inside its `patch` — reading only the flattened extras saw
                // neither, so a completed task stayed live forever and the
                // turn it deferred never settled (EXP-753).
                let status = system
                    .status
                    .clone()
                    .or_else(|| {
                        system
                            .extra
                            .get("status")
                            .or_else(|| {
                                system.extra.get("patch").and_then(|patch| patch.get("status"))
                            })
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "running".to_string());
                let terminal = matches!(status.as_str(), "completed" | "failed" | "cancelled");
                let mut state = self.lock();
                // `task_notification` and `task_updated` share this arm and
                // the CLI sends both for one edge often enough to matter
                // (7 duplicate `completed`s in 54, measured), which drew the
                // subagent twice. Only a CHANGE is republished.
                let (tool_use_id, subagent_type, repeat) = match state.tasks.get_mut(&task_id) {
                    Some(task) => {
                        let repeat = task.last_status.as_deref() == Some(status.as_str());
                        task.live = !terminal;
                        task.last_status = Some(status.clone());
                        (task.tool_use_id.clone(), task.subagent_type.clone(), repeat)
                    }
                    None => (None, None, false),
                };
                drop(state);
                // The edge goes out BEFORE the settle it unblocks: settling
                // first ends the `session/prompt`, and a client that renders
                // the subagent card off the edge would see the run finish
                // with that card still spinning (EXP-753).
                if !repeat {
                    self.publish_subagent(
                        cx,
                        &task_id,
                        tool_use_id.as_deref(),
                        subagent_type.as_deref(),
                        &status,
                    );
                }
                if terminal {
                    let mut state = self.lock();
                    self.settle_deferred(cx, &mut state);
                }
            }
            SystemSubtype::CommandsChanged => {
                let commands = system
                    .extra
                    .get("slash_commands")
                    .cloned()
                    .and_then(|value| {
                        serde_json::from_value::<Vec<wire::SlashCommandInfo>>(value).ok()
                    })
                    .unwrap_or_default();
                if commands.is_empty() {
                    return;
                }
                let mut state = self.lock();
                let terminal = state.terminal_commands.clone();
                state.commands = wire::available_commands(&commands, &terminal);
                drop(state);
                self.publish_commands(cx);
            }
            SystemSubtype::ModelRefusalFallback => {
                let model = system
                    .extra
                    .get("to_model")
                    .or_else(|| system.extra.get("fallback_model"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(model) = model {
                    self.lock().model = model;
                    self.publish_config(cx);
                }
            }
            SystemSubtype::PermissionDenied | SystemSubtype::Other => {}
        }
    }

    fn on_assistant(self: &Arc<Self>, cx: &ConnectionTo<Client>, message: wire::AssistantMsg) {
        let parent = message.parent_tool_use_id.clone();
        let message_id = message.message.get("id").and_then(Value::as_str).map(str::to_string);
        let content = message.message.get("content").cloned().unwrap_or(Value::Null);
        let blocks = match &content {
            Value::Array(blocks) => blocks.clone(),
            Value::String(text) => vec![json!({ "type": "text", "text": text })],
            _ => Vec::new(),
        };

        // Each text/thinking block may already have streamed as deltas: diff
        // it against what streamed (in document order) and forward only the
        // remainder — nothing in the common case, the whole block on a
        // non-streaming gateway, the tail when a stream was cut short.
        let streamed = message_id
            .as_ref()
            .map(|id| self.lock().streamed.remove(id).unwrap_or_default())
            .unwrap_or_default();
        let mut streamed = streamed.into_values();

        for block in blocks {
            let kind = block.get("type").and_then(Value::as_str).unwrap_or_default();
            match kind {
                "text" | "thinking" => {
                    let full = block
                        .get(if kind == "text" { "text" } else { "thinking" })
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let already = streamed.next().map(|block| block.text).unwrap_or_default();
                    let rest = full.strip_prefix(already.as_str()).unwrap_or(full);
                    if rest.is_empty() {
                        continue;
                    }
                    self.emit_text(cx, kind == "thinking", rest, message_id.as_deref(), &parent);
                }
                "image" => {
                    if let Some(image) = image_block(&block) {
                        self.emit_chunk(
                            cx,
                            SessionUpdate::AgentMessageChunk(
                                ContentChunk::new(image)
                                    .message_id(message_id.clone().map(MessageId::new)),
                            ),
                            &parent,
                        );
                    }
                }
                "tool_use" | "server_tool_use" | "mcp_tool_use" => {
                    self.on_tool_use(cx, &block, &parent);
                }
                _ => {}
            }
        }
    }

    fn emit_text(
        &self,
        cx: &ConnectionTo<Client>,
        thinking: bool,
        text: &str,
        message_id: Option<&str>,
        parent: &Option<String>,
    ) {
        // Recent models stream signature-only thinking blocks with empty text.
        if thinking && text.trim().is_empty() {
            return;
        }
        if !thinking {
            self.lock().delivered_text = true;
        }
        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
            .message_id(message_id.map(MessageId::new));
        let update = if thinking {
            SessionUpdate::AgentThoughtChunk(chunk)
        } else {
            SessionUpdate::AgentMessageChunk(chunk)
        };
        self.emit_chunk(cx, update, parent);
    }

    fn emit_chunk(
        &self,
        cx: &ConnectionTo<Client>,
        update: SessionUpdate,
        parent: &Option<String>,
    ) {
        match parent {
            Some(parent) => {
                let mut meta = Map::new();
                meta.insert(PARENT_TOOL_CALL_META_KEY.to_string(), json!(parent));
                self.notify_meta(cx, update, meta);
            }
            None => self.notify(cx, update),
        }
    }

    /// A `content_block_start` tool_use: remember the call (name, so far
    /// empty input) without surfacing it — see [`Self::on_tool_use`].
    fn record_tool_use(&self, block: &Value) {
        let Some(id) = block.get("id").and_then(Value::as_str) else { return };
        let name = block.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        if name == "TodoWrite" || is_task_tool(&name) {
            return;
        }
        let input = block.get("input").cloned().unwrap_or(Value::Null);
        self.lock()
            .tools
            .entry(id.to_string())
            .or_insert(ToolEntry { name, input, surfaced: false });
    }

    fn on_tool_use(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        block: &Value,
        parent: &Option<String>,
    ) {
        let Some(id) = block.get("id").and_then(Value::as_str) else { return };
        let name = block.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        let input = block.get("input").cloned().unwrap_or(Value::Null);

        // TodoWrite IS the plan lane, and the Task* tools render as plan
        // entries when their results arrive — neither surfaces a tool call.
        if name == "TodoWrite" {
            let entries = todo_entries(&input);
            if !entries.is_empty() {
                self.notify(cx, SessionUpdate::Plan(Plan::new(entries)));
            }
            return;
        }
        if is_task_tool(&name) {
            return;
        }

        let mut state = self.lock();
        let surfaced = match state.tools.get_mut(id) {
            Some(entry) => {
                entry.input = input.clone();
                std::mem::replace(&mut entry.surfaced, true)
            }
            None => {
                state.tools.insert(
                    id.to_string(),
                    ToolEntry { name: name.clone(), input: input.clone(), surfaced: true },
                );
                false
            }
        };
        drop(state);

        let info = tool_info(&name, &input, self.cwd());
        let update = if surfaced {
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
                ToolCallUpdateFields::new()
                    .title(info.title)
                    .kind(info.kind)
                    .content(info.content)
                    .locations(info.locations)
                    .raw_input(input),
            ))
        } else {
            SessionUpdate::ToolCall(
                ToolCall::new(ToolCallId::new(id), info.title)
                    .kind(info.kind)
                    .status(ToolCallStatus::InProgress)
                    .content(info.content)
                    .locations(info.locations)
                    .raw_input(input),
            )
        };
        self.emit_chunk(cx, update, parent);
    }

    fn on_user(self: &Arc<Self>, cx: &ConnectionTo<Client>, message: wire::UserMsg) {
        let parent = message.parent_tool_use_id.clone();
        let content = message.message.get("content").cloned().unwrap_or(Value::Null);
        let blocks = match &content {
            Value::Array(blocks) => blocks.clone(),
            Value::String(text) => vec![json!({ "type": "text", "text": text })],
            _ => Vec::new(),
        };
        // `tool_use_result` is message-level and carries no tool_use_id of its
        // own: it describes THE tool_result block of the message it rode in
        // on, so it is only honoured when there is exactly one.
        let results = blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
            .count();
        let structured =
            (results == 1 && !message.tool_use_result.is_null()).then(|| &message.tool_use_result);
        // EXP-772: the CLI writes machinery into the transcript as `user`
        // entries — system reminders, synthetic refusals, the summary a
        // compaction hands the fresh context. None of it is a human turn, so
        // none of it becomes a user bubble. The `tool_result` blocks of such
        // an entry are still honoured: they are the agent's own plumbing.
        let injected = message.is_meta || message.is_synthetic || message.is_compact_summary;

        for block in &blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_result") => self.on_tool_result(cx, block, structured, &parent),
                Some("text") if !injected => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or_default();
                    // A block that OPENS with an injection marker is machinery
                    // whole; the local-command wrappers are stripped in place
                    // so real prose beside them survives.
                    if wire::is_injected_user_block(text) {
                        continue;
                    }
                    let Some(text) = wire::strip_local_command_metadata(text) else { continue };
                    if text.trim().is_empty() {
                        continue;
                    }
                    // Unlike the upstream adapter, user echoes ARE forwarded:
                    // `--replay-user-messages` is how a message steered from
                    // web or a phone reaches every other viewer's feed.
                    self.emit_chunk(
                        cx,
                        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new(text),
                        ))),
                        &parent,
                    );
                }
                _ => {}
            }
        }
    }

    fn on_tool_result(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        block: &Value,
        structured: Option<&Value>,
        parent: &Option<String>,
    ) {
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { return };
        let failed = block.get("is_error").and_then(Value::as_bool).unwrap_or(false);
        // The result is the last reader of the call, so TAKE the entry: a
        // finished Write/Edit must not keep its whole input alive for the run.
        let entry = self.lock().tools.remove(id).map(|entry| (entry.name, entry.input));
        let (name, input) = entry.unwrap_or_else(|| (String::new(), Value::Null));

        if is_task_tool(&name) {
            // Task* results feed the plan card, never a tool card.
            self.publish_plan(cx);
            return;
        }

        let fields = tool_result_fields(&name, &input, block, structured, failed);
        self.emit_chunk(
            cx,
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(ToolCallId::new(id), fields)),
            parent,
        );
    }

    fn on_stream_event(self: &Arc<Self>, cx: &ConnectionTo<Client>, event: wire::StreamEventMsg) {
        let parent = event.parent_tool_use_id.clone();
        let parent_key = parent.clone().unwrap_or_default();
        match event.event_type() {
            "message_start" => {
                let message = &event.event["message"];
                if let Some(id) = message.get("id").and_then(Value::as_str) {
                    self.lock().current_message.insert(parent_key, id.to_string());
                }
                if let Some(model) = message.get("model").and_then(Value::as_str) {
                    self.lock().context_window.infer(model);
                }
                self.merge_usage(cx, &message["usage"], None);
            }
            "message_delta" => self.merge_usage(cx, &event.event["usage"], None),
            "content_block_start" => {
                let block = &event.event["content_block"];
                let index = event.event.get("index").and_then(Value::as_u64).unwrap_or(0);
                match block.get("type").and_then(Value::as_str) {
                    Some(kind @ ("text" | "thinking")) => {
                        let text = block
                            .get(if kind == "text" { "text" } else { "thinking" })
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let message_id = self.record_streamed(
                            &parent_key,
                            index,
                            kind == "thinking",
                            &text,
                        );
                        if !text.is_empty() {
                            self.emit_text(
                                cx,
                                kind == "thinking",
                                &text,
                                message_id.as_deref(),
                                &parent,
                            );
                        }
                    }
                    // The input streams in AFTER this frame (`input_json_delta`),
                    // so the card is only RECORDED here and surfaces with its
                    // complete input on the consolidated `assistant` message:
                    // the relay `tool` event is one-shot and must carry the real
                    // title and path, never a "Preparing file…" placeholder.
                    Some("tool_use") => self.record_tool_use(block),
                    _ => {}
                }
            }
            "content_block_delta" => {
                let index = event.event.get("index").and_then(Value::as_u64).unwrap_or(0);
                let delta = &event.event["delta"];
                match event.delta_type() {
                    Some(kind @ ("text_delta" | "thinking_delta")) => {
                        let thinking = kind == "thinking_delta";
                        let text = delta
                            .get(if thinking { "thinking" } else { "text" })
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if text.is_empty() {
                            return;
                        }
                        let message_id =
                            self.record_streamed(&parent_key, index, thinking, &text);
                        self.emit_text(cx, thinking, &text, message_id.as_deref(), &parent);
                    }
                    // Partial tool input is deliberately NOT refined into the
                    // card: an Edit missing its `new_string` renders as a pure
                    // deletion, and the consolidated assistant message carries
                    // the complete input moments later.
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Append streamed text to the per-message record and return the message
    /// id the chunk belongs to.
    fn record_streamed(
        &self,
        parent_key: &str,
        index: u64,
        thinking: bool,
        text: &str,
    ) -> Option<String> {
        let mut state = self.lock();
        let message_id = state.current_message.get(parent_key).cloned()?;
        let blocks = state.streamed.entry(message_id.clone()).or_default();
        let block = blocks.entry(index).or_insert(StreamedBlock { thinking, text: String::new() });
        block.thinking = thinking;
        block.text.push_str(text);
        Some(message_id)
    }

    fn merge_usage(&self, cx: &ConnectionTo<Client>, usage: &Value, cost: Option<f64>) {
        if !usage.is_object() {
            return;
        }
        let used = {
            let mut state = self.lock();
            state.usage.merge(usage);
            state.usage.used()
        };
        self.publish_usage(cx, used, cost);
    }

    fn on_result(self: &Arc<Self>, cx: &ConnectionTo<Client>, result: wire::ResultMsg) {
        // The authoritative context window only ever arrives here. EXP-761:
        // it is taken ONCE per session ([`ContextWindow::report`]) — a
        // later result's map may lack the session's model (a turn only the
        // haiku helper worked on), and re-deriving from it every turn made
        // the published size alternate 1000000 ↔ 200000 within one run.
        {
            let mut state = self.lock();
            let model = state.model.clone();
            if let Some(window) = wire::context_window_from_model_usage(&result.model_usage, &model)
            {
                state.context_window.report(window);
            }
        }
        // `result.usage` is the turn's CUMULATIVE token count (every request
        // of the turn summed), not the context occupancy; the occupancy is
        // what the last `message_start`/`message_delta` already merged. The
        // result only contributes the cost, and its usage counts only when
        // nothing streamed (a non-streaming gateway).
        let streamed = self.lock().usage.used() > 0;
        if streamed {
            let used = self.lock().usage.used();
            self.publish_usage(cx, used, result.total_cost_usd);
        } else {
            self.merge_usage(cx, &result.usage, result.total_cost_usd);
        }

        // A "clear context" plan approval interrupted this turn on purpose:
        // the same ACP turn continues on a fresh conversation instead of
        // settling here.
        let restart = self.lock().pending_plan_restart.take();
        if let Some(restart) = restart {
            self.restart_with_plan(cx, restart);
            return;
        }

        let output_tokens =
            result.usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
        {
            let mut state = self.lock();
            if state.skip_local_command_result {
                state.skip_local_command_result = false;
                if output_tokens == 0 {
                    // The `/clear` turn the restart injected: it ends a turn
                    // the client never asked for.
                    return;
                }
            }
        }

        let (local_only, delivered) = {
            let state = self.lock();
            (state.local_only_command, state.delivered_text)
        };
        if wire::should_forward_result(local_only, delivered, &result) {
            self.notify(
                cx,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(result.result.clone()),
                ))),
            );
        }

        let cancelled = self.lock().cancelled;
        let outcome = wire::turn_outcome(&result, cancelled);
        let mut state = self.lock();
        state.delivered_text = false;
        state.local_only_command = false;
        self.settle_or_defer(
            cx,
            &mut state,
            DeferredSettle {
                outcome,
                queued: result.queued_turn_count,
            },
        );
    }

    /// Continue the accepted plan in a fresh context: clear the conversation,
    /// switch to the mode the user picked, and re-prompt with the plan. The
    /// upstream restarts its whole query for this; `/clear` is the same reset
    /// without discarding the process (and, if a future CLI drops it, the
    /// plan is still re-prompted — the context simply is not cleared).
    fn restart_with_plan(self: &Arc<Self>, cx: &ConnectionTo<Client>, restart: PlanRestart) {
        let session = self.clone();
        let mode = restart.mode.clone();
        let plan = restart.plan.clone();
        let injected = cx.clone();
        let _ = cx.spawn(async move {
            if let Err(error) = session.control_request(wire::set_permission_mode(&mode)).await {
                log::warn!("engine: claude plan-mode switch failed: {error}");
            }
            {
                let mut state = session.lock();
                state.mode = mode.clone();
                state.skip_local_command_result = true;
            }
            session.inject_prompt(&injected, "/clear");
            let prompt = format!("{PLAN_RESTART_PROMPT}\n\n{plan}");
            session.inject_prompt(&injected, &prompt);
            Ok(())
        });
        self.notify(
            cx,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(
                restart.mode,
            ))),
        );
    }

    /// Send a prompt the ADAPTER composed (`/clear`, the plan hand-off), not
    /// the user. EXP-772: the CLI replays every user turn, so the mapper is
    /// told to arm its echo dedupe first and publishes no bubble for either
    /// the injection or its replay.
    fn inject_prompt(self: &Arc<Self>, cx: &ConnectionTo<Client>, text: &str) {
        let mut meta = Map::new();
        meta.insert(INJECTED_PROMPT_META_KEY.to_string(), json!(true));
        self.notify_meta(
            cx,
            SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text),
            ))),
            meta,
        );
        let _ = self.send(wire::user_message(text, None));
    }

    // -----------------------------------------------------------------------
    // inbound control requests
    // -----------------------------------------------------------------------

    fn on_control_request(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        frame: wire::ControlRequestFrame,
    ) {
        let request_id = frame.request_id.clone();
        match frame.request.subtype() {
            "can_use_tool" => {
                let session = self.clone();
                let cx = cx.clone();
                // EXP-758: on the books BEFORE the spawn, so a cancel that
                // races the answer is recorded and one that trails it is not.
                session.lock().answering.insert(request_id.clone());
                let _ = cx.clone().spawn(async move {
                    session.answer_can_use_tool(&cx, request_id, frame.request).await;
                    Ok(())
                });
            }
            "hook_callback" => self.on_hook_callback(cx, request_id, frame.request),
            // An unrecognized dialog kind is answered with SILENCE: the CLI
            // fails closed on a dialog no client declared and degrades to its
            // own behaviour, while a synthesized cancel would kill the flow.
            "request_user_dialog" => {}
            other => {
                let _ = self.send(wire::control_response_error(
                    &request_id,
                    &format!("Unsupported control request subtype: {other}"),
                ));
            }
        }
    }

    async fn answer_can_use_tool(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request_id: String,
        request: wire::ControlReq,
    ) {
        let tool_use_id = request.tool_use_id.clone().unwrap_or_else(|| request_id.clone());
        let answer = if request.is_ask_user_question() {
            PermissionAnswer::plain(self.ask_user_question(cx, &request, &tool_use_id).await)
        } else if request.is_exit_plan_mode() {
            self.request_permission(cx, &request, &tool_use_id).await
        } else {
            // EXP-772: permissions are bypassed in EVERY mode, so the only
            // two dialogs left are the plan approval and a question the model
            // asked. Everything else is allowed here, without a card: the CLI
            // still asks under a safety check, and a card the user cannot
            // meaningfully refuse is just a stall.
            log::debug!(
                "engine: claude auto-allowing {} (permissions bypassed)",
                request.tool_name
            );
            PermissionAnswer::plain(wire::permission_allow(&tool_use_id, request.input.clone()))
        };
        // The CLI cancelled this request while we were asking: answering it
        // now would be answering a request that no longer exists.
        let aborted = {
            let mut state = self.lock();
            state.answering.remove(&request_id);
            state.aborted_requests.remove(&request_id)
        };
        if aborted {
            log::debug!("engine: claude abandoned control request {request_id}; not answering");
            return;
        }
        // The answer's SIDE EFFECTS (a mode switch, an armed plan restart)
        // land only once the response is actually on the wire: an aborted or
        // unwritable request must not clear the plan or start a build turn.
        match self.send(wire::control_response_success(&request_id, answer.response)) {
            Ok(()) => {
                log::debug!("engine: answered claude control request {request_id}");
                self.apply_permission_effects(cx, answer.effects);
            }
            Err(error) => log::warn!("engine: claude control response {request_id} failed: {error}"),
        }
    }

    /// Fold a permission answer's effects into the session: the mode switch is
    /// published as a `CurrentModeUpdate` plus a fresh `config_state`, exactly
    /// like a steered `session/set_mode`, so the client's Plan/Build toggle
    /// follows a plan approval instead of staying stuck on Plan.
    fn apply_permission_effects(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        effects: PermissionEffects,
    ) {
        let switched = {
            let mut state = self.lock();
            take_permission_effects(&mut state, effects)
        };
        if let Some(mode) = switched {
            self.notify(
                cx,
                SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(mode))),
            );
            self.publish_config(cx);
        }
    }

    async fn request_permission(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> PermissionAnswer {
        let exit_plan = request.is_exit_plan_mode();
        let info = tool_info(&request.tool_name, &request.input, self.cwd());
        let options = if exit_plan {
            exit_plan_options(&request.input)
        } else {
            permission_options(request)
        };
        // The upstream hard-codes this title for the plan approval, and all
        // four clients key their "Plan ready" card on the switch_mode kind
        // plus the plan markdown the tool call carries as content.
        let title = if exit_plan { "Ready to code?".to_string() } else { info.title.clone() };
        let fields = ToolCallUpdateFields::new()
            .title(title)
            .kind(info.kind)
            .status(ToolCallStatus::Pending)
            .content(info.content)
            .locations(info.locations)
            .raw_input(request.input.clone());
        let mut tool_call = ToolCallUpdate::new(ToolCallId::new(tool_use_id), fields);
        let mut meta = Map::new();
        // `decision_reason_type` says WHY the CLI is asking under a mode that
        // normally would not (`safetyCheck` is the bypass-immune one).
        if let Some(description) = request.description.clone().or_else(|| {
            request.decision_reason_type.clone().map(|reason| format!("Reason: {reason}"))
        }) {
            meta.insert(
                "permission".to_string(),
                json!({ "version": 1, "description": description }),
            );
        }
        // A permission raised INSIDE a subagent belongs to that subagent's
        // card, exactly like the chunks and tool calls around it.
        if let Some(parent) = self.subagent_parent(request.agent_id.as_deref()) {
            meta.insert(PARENT_TOOL_CALL_META_KEY.to_string(), json!(parent));
        }
        if !meta.is_empty() {
            tool_call = tool_call.meta(meta);
        }
        self.await_permission(cx, tool_call, options, request, tool_use_id).await
    }

    /// The tool call that spawned the subagent a `can_use_tool` was raised
    /// inside, or `None` for the main thread. `agent_id` IS the `task_id` of
    /// an earlier `system/task_started` (EXP-753, measured against the CLI),
    /// so the lookup is exact.
    ///
    /// NO `agent_id` at all is the MAIN thread — every main-thread
    /// `can_use_tool` the fixtures recorded omits the field — so it resolves
    /// to `None` without guessing: guessing there nests a main-thread
    /// permission under a background subagent's card. The fallback covers only
    /// a CLI that sends an id we cannot resolve: a FOREGROUND Task the model
    /// waits on holds the main thread, so the ONE live foreground task is the
    /// only thing that can be asking — with two live there is nothing to
    /// distinguish them, so it attributes to neither, and a BACKGROUNDED task
    /// never qualifies because the main thread runs on beside it.
    fn subagent_parent(&self, agent_id: Option<&str>) -> Option<String> {
        let agent_id = agent_id?;
        let state = self.lock();
        if let Some(parent) = state.tasks.get(agent_id).and_then(|task| task.tool_use_id.clone()) {
            return Some(parent);
        }
        let mut live = state.tasks.values().filter(|task| task.live && !task.backgrounded);
        let only = live.next()?;
        if live.next().is_some() {
            return None;
        }
        only.tool_use_id.clone()
    }

    async fn await_permission(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        tool_call: ToolCallUpdate,
        options: Vec<PermissionOption>,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> PermissionAnswer {
        let outcome = cx
            .send_request(RequestPermissionRequest::new(
                self.session_id.clone(),
                tool_call,
                options,
            ))
            .block_task()
            .await;
        let selected = match outcome {
            Ok(response) => match response.outcome {
                RequestPermissionOutcome::Selected(selected) => {
                    Some(selected.option_id.0.to_string())
                }
                // The ACP cancellation contract: the client answers every
                // pending permission with `Cancelled` on session/cancel.
                _ => None,
            },
            Err(error) => {
                log::warn!("engine: claude permission request failed: {error}");
                None
            }
        };
        let Some(selected) = selected else {
            return PermissionAnswer::plain(wire::permission_deny(
                tool_use_id,
                "Cancelled by the user",
                false,
            ));
        };
        permission_answer(&selected, request, tool_use_id)
    }

    async fn ask_user_question(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> Value {
        let questions = ask_questions(&request.input);
        if questions.is_empty() {
            return wire::permission_allow(tool_use_id, request.input.clone());
        }
        let elicitation = ask_elicitation(&self.session_id, tool_use_id, &questions);
        let response = cx.send_request(elicitation).block_task().await;
        let action = match response {
            Ok(response) => response.action,
            Err(error) => {
                log::warn!("engine: claude question elicitation failed: {error}");
                ElicitationAction::Cancel
            }
        };
        match action {
            ElicitationAction::Accept(accepted) => {
                let content = accepted.content.unwrap_or_default();
                let answers = ask_answers(&questions, &content);
                let mut input = request.input.clone();
                if let Some(object) = input.as_object_mut() {
                    object.insert("answers".to_string(), Value::Object(answers));
                }
                wire::permission_allow(tool_use_id, input)
            }
            // Declining is an answer: the model is told the user skipped
            // rather than the turn aborting.
            ElicitationAction::Decline => {
                let mut input = request.input.clone();
                if let Some(object) = input.as_object_mut() {
                    object.insert("answers".to_string(), Value::Object(Map::new()));
                }
                wire::permission_allow(tool_use_id, input)
            }
            _ => wire::permission_deny(tool_use_id, "Tool use aborted", false),
        }
    }

    /// Hooks are answered FIRST and their side effects run after: a control
    /// request issued while the CLI waits on a hook response deadlocks the
    /// single control channel (the upstream's `setImmediate` workaround).
    fn on_hook_callback(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request_id: String,
        request: wire::ControlReq,
    ) {
        let _ = self.send(wire::control_response_success(&request_id, json!({ "continue": true })));
        let callback = request.callback_id.clone().unwrap_or_default();
        let input: wire::HookInput =
            serde_json::from_value(request.input.clone()).unwrap_or_default();
        match callback.as_str() {
            wire::HOOK_POST_TOOL_USE => self.on_post_tool_use(cx, &input),
            wire::HOOK_POST_MODEL_SWITCH => {
                // A `/model` typed as a prompt: mirror it into the picker (and
                // re-clamp the effort the new model supports) AFTER the answer
                // above went out.
                let source = input.source.clone().unwrap_or_default();
                if source == "sdk" || source == "resume" {
                    return;
                }
                if let Some(model) = input.to_model.clone() {
                    self.lock().model = model;
                    self.publish_config(cx);
                }
            }
            wire::HOOK_TASK_CREATED | wire::HOOK_TASK_COMPLETED => {
                let Some(task_id) = input.task_id.clone() else { return };
                let completed = callback == wire::HOOK_TASK_COMPLETED;
                let mut state = self.lock();
                let entry = state.plan_tasks.entry(task_id).or_insert(PlanTask {
                    subject: input.task_subject.clone().unwrap_or_default(),
                    status: "pending".to_string(),
                    active_form: input.task_active_form.clone(),
                });
                if let Some(subject) = input.task_subject.clone() {
                    entry.subject = subject;
                }
                entry.status =
                    if completed { "completed".to_string() } else { "in_progress".to_string() };
                drop(state);
                self.publish_plan(cx);
            }
            _ => {}
        }
    }

    /// `PostToolUse` for Edit/Write carries the REAL `structuredPatch` — which
    /// is exactly why those tools' `tool_result` renders nothing. It also
    /// carries the EnterPlanMode edge, the only signal that the CLI itself
    /// switched into plan mode.
    fn on_post_tool_use(self: &Arc<Self>, cx: &ConnectionTo<Client>, input: &wire::HookInput) {
        if input.tool_name == "EnterPlanMode" {
            self.lock().mode = "plan".to_string();
            self.notify(
                cx,
                SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(
                    "plan",
                ))),
            );
            self.publish_config(cx);
            return;
        }
        if !matches!(input.tool_name.as_str(), "Edit" | "Write") {
            return;
        }
        let Some(tool_use_id) = input
            .extra
            .get("tool_use_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let (content, locations) = diff_from_hook(&input.tool_response);
        if content.is_empty() {
            return;
        }
        self.notify(
            cx,
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_use_id),
                ToolCallUpdateFields::new()
                    .status(ToolCallStatus::Completed)
                    .content(content)
                    .locations(locations),
            )),
        );
    }

    // -----------------------------------------------------------------------
    // history replay (`session/load`)
    // -----------------------------------------------------------------------

    /// Replay a persisted transcript through the SAME frame path a live
    /// session takes, with no child process and no hooks — so a Past run
    /// renders exactly like the run did (minus the per-edit diffs, which only
    /// the PostToolUse hook can produce).
    fn replay_history(self: &Arc<Self>, cx: &ConnectionTo<Client>, session_id: &SessionId) {
        let Some(path) = transcript_path(&self.spec.spawn.env, session_id.0.as_ref()) else {
            log::warn!("engine: no claude transcript for {}", session_id.0);
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match ClaudeOut::parse(line) {
                // Replays carry no control traffic and no turn to settle: a
                // recorded `result` would settle a turn that never started.
                ClaudeOut::ControlRequest(_)
                | ClaudeOut::ControlResponse(_)
                | ClaudeOut::ControlCancelRequest(_)
                | ClaudeOut::Result(_)
                | ClaudeOut::KeepAlive
                | ClaudeOut::Unknown => {}
                frame => self.on_frame(cx, frame),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// pure helpers
// ---------------------------------------------------------------------------

/// EXP-772: plan on, plan off. Permissions are bypassed in every mode, so
/// `default`/`acceptEdits`/`auto` differ in nothing a user can see and are
/// never offered; `dontAsk` was never offered either.
fn available_modes() -> Vec<SessionMode> {
    vec![
        SessionMode::new(SessionModeId::new("plan"), "Plan")
            .description("Create a plan before making changes"),
        SessionMode::new(SessionModeId::new("bypassPermissions"), "Build")
            .description("Make the changes"),
    ]
}

/// EXP-772: the only steerable modes are `plan` and `bypassPermissions`.
/// Anything else (an older publisher's `default`, `acceptEdits`, `auto`, or
/// whatever the CLI reports it launched in) means "stop planning", so it lands
/// on `bypassPermissions` rather than a mode [`available_modes`] never offers
/// and no client can render.
fn clamp_mode(mode: &str) -> String {
    match mode {
        "plan" => "plan".to_string(),
        _ => "bypassPermissions".to_string(),
    }
}

/// The mode a `system/init` frame adopts, or `None` to keep the current one.
/// The CLI reports its OWN spelling (`acceptEdits` after a `--permission-mode`
/// launch), which goes through the same clamp as a steered switch: state.mode
/// feeds `mode_state`, and a value outside `available_modes` strands the
/// client's Plan/Build toggle.
fn init_mode(reported: Option<&str>, current: &str) -> Option<String> {
    let reported = reported.filter(|mode| !mode.is_empty())?;
    let clamped = clamp_mode(reported);
    (clamped != current).then_some(clamped)
}

fn plan_status(status: &str) -> PlanEntryStatus {
    match status {
        "in_progress" => PlanEntryStatus::InProgress,
        "completed" => PlanEntryStatus::Completed,
        _ => PlanEntryStatus::Pending,
    }
}

fn is_task_tool(name: &str) -> bool {
    matches!(name, "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet")
}

/// `planEntries(input)` — a TodoWrite call IS the plan.
fn todo_entries(input: &Value) -> Vec<PlanEntry> {
    input
        .get("todos")
        .and_then(Value::as_array)
        .map(|todos| {
            todos
                .iter()
                .map(|todo| {
                    let status = todo.get("status").and_then(Value::as_str).unwrap_or("pending");
                    let active = todo.get("activeForm").and_then(Value::as_str);
                    let content = match (status, active) {
                        ("in_progress", Some(active)) if !active.is_empty() => active,
                        _ => todo.get("content").and_then(Value::as_str).unwrap_or_default(),
                    };
                    PlanEntry::new(content, PlanEntryPriority::Medium, plan_status(status))
                })
                .collect()
        })
        .unwrap_or_default()
}

struct ToolInfo {
    title: String,
    kind: ToolKind,
    content: Vec<ToolCallContent>,
    locations: Vec<ToolCallLocation>,
}

fn text_content(text: impl Into<String>) -> ToolCallContent {
    ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(text))))
}

/// The `toolInfoFromToolUse` port (`tools.js:16`), case for case.
///
/// `MultiEdit`, `BashOutput`, `KillShell`, `LS`, `NotebookEdit`,
/// `NotebookRead` and every `mcp__*` fall through to the default arm — that
/// is the upstream's behaviour too, verified by grep, not an omission.
fn tool_info(name: &str, input: &Value, cwd: &Path) -> ToolInfo {
    let string = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or_default();
    let number = |key: &str| input.get(key).and_then(Value::as_u64);
    match name {
        "Agent" | "Task" => {
            let description = string("description");
            ToolInfo {
                title: if description.is_empty() { "Task".into() } else { description.into() },
                kind: ToolKind::Think,
                // EXP-773: the prompt body is NOT the card. A subagent's own
                // rows render inside its card; repeating the whole prompt
                // above them buried the run.
                content: vec![],
                locations: vec![],
            }
        }
        "Bash" => {
            let command = string("command");
            let description = string("description");
            ToolInfo {
                title: if command.is_empty() { "Terminal".into() } else { command.into() },
                kind: ToolKind::Execute,
                // The client advertises `terminal: false`, so the description
                // (never the command output) is the card's body.
                content: if description.is_empty() {
                    vec![]
                } else {
                    vec![text_content(description)]
                },
                locations: vec![],
            }
        }
        "Read" => {
            let path = string("file_path");
            let offset = number("offset");
            let limit = number("limit");
            let suffix = match (limit, offset) {
                (Some(limit), offset) if limit > 0 => {
                    let start = offset.unwrap_or(1);
                    format!(" ({start} - {})", start + limit - 1)
                }
                (_, Some(offset)) => format!(" (from line {offset})"),
                _ => String::new(),
            };
            let display =
                if path.is_empty() { "File".to_string() } else { wire::display_path(path, cwd) };
            ToolInfo {
                title: format!("Read {display}{suffix}"),
                kind: ToolKind::Read,
                content: vec![],
                locations: if path.is_empty() {
                    vec![]
                } else {
                    vec![ToolCallLocation::new(PathBuf::from(path)).line(
                        offset.unwrap_or(1).try_into().unwrap_or(u32::MAX),
                    )]
                },
            }
        }
        "Write" => {
            let path = string("file_path");
            let content = string("content");
            let body = if !path.is_empty() {
                vec![ToolCallContent::Diff(Diff::new(PathBuf::from(path), content))]
            } else if !content.is_empty() {
                vec![text_content(content)]
            } else {
                vec![]
            };
            ToolInfo {
                title: if path.is_empty() {
                    "Preparing file…".to_string()
                } else {
                    format!("Write {}", wire::display_path(path, cwd))
                },
                kind: ToolKind::Edit,
                content: body,
                locations: location_for(path),
            }
        }
        "Edit" => {
            let path = string("file_path");
            let old = string("old_string");
            let new = string("new_string");
            let body = if !path.is_empty() && (!old.is_empty() || !new.is_empty()) {
                vec![ToolCallContent::Diff(
                    Diff::new(PathBuf::from(path), new)
                        .old_text((!old.is_empty()).then(|| old.to_string())),
                )]
            } else {
                vec![]
            };
            ToolInfo {
                title: if path.is_empty() {
                    "Edit".to_string()
                } else {
                    format!("Edit {}", wire::display_path(path, cwd))
                },
                kind: ToolKind::Edit,
                content: body,
                locations: location_for(path),
            }
        }
        "Glob" => {
            let path = string("path");
            let pattern = string("pattern");
            let mut title = "Find".to_string();
            if !path.is_empty() {
                title.push_str(&format!(" `{path}`"));
            }
            if !pattern.is_empty() {
                title.push_str(&format!(" `{pattern}`"));
            }
            ToolInfo {
                title,
                kind: ToolKind::Search,
                content: vec![],
                locations: location_for(path),
            }
        }
        "Grep" => ToolInfo {
            title: grep_command(input),
            kind: ToolKind::Search,
            content: vec![],
            locations: vec![],
        },
        "WebFetch" => {
            let url = string("url");
            let prompt = string("prompt");
            ToolInfo {
                title: if url.is_empty() { "Fetch".into() } else { format!("Fetch {url}") },
                kind: ToolKind::Fetch,
                content: if prompt.is_empty() { vec![] } else { vec![text_content(prompt)] },
                locations: vec![],
            }
        }
        "WebSearch" => {
            let query = string("query");
            ToolInfo {
                title: if query.is_empty() {
                    "Web search".into()
                } else {
                    format!("Search \"{query}\"")
                },
                kind: ToolKind::Fetch,
                content: vec![],
                locations: vec![],
            }
        }
        "ExitPlanMode" => {
            let plan = string("plan");
            ToolInfo {
                title: "Approve Plan".to_string(),
                kind: ToolKind::SwitchMode,
                content: if plan.is_empty() { vec![] } else { vec![text_content(plan)] },
                locations: vec![],
            }
        }
        "Skill" => {
            let skill = string("skill");
            ToolInfo {
                title: if skill.is_empty() {
                    "Load skill".into()
                } else {
                    format!("Load skill: {skill}")
                },
                kind: ToolKind::Other,
                content: vec![],
                locations: vec![],
            }
        }
        "AskUserQuestion" => {
            let questions = ask_questions(input);
            let title = match questions.first() {
                Some(question) if questions.len() == 1 && !question.question.is_empty() => {
                    question.question.clone()
                }
                _ => "Asking for your input".to_string(),
            };
            ToolInfo {
                title,
                kind: ToolKind::Other,
                content: questions
                    .iter()
                    .map(|question| text_content(question.question.clone()))
                    .collect(),
                locations: vec![],
            }
        }
        "TodoWrite" => ToolInfo {
            title: "Update TODOs".to_string(),
            kind: ToolKind::Think,
            content: vec![],
            locations: vec![],
        },
        _ => ToolInfo {
            title: if name.is_empty() { "Unknown Tool".into() } else { name.to_string() },
            kind: ToolKind::Other,
            content: vec![],
            locations: vec![],
        },
    }
}

fn location_for(path: &str) -> Vec<ToolCallLocation> {
    if path.is_empty() {
        vec![]
    } else {
        vec![ToolCallLocation::new(PathBuf::from(path))]
    }
}

/// The reconstructed `grep` command line (`tools.js:142`).
fn grep_command(input: &Value) -> String {
    let mut label = String::from("grep");
    let flag = |key: &str| input.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("-i") {
        label.push_str(" -i");
    }
    if flag("-n") {
        label.push_str(" -n");
    }
    for key in ["-A", "-B", "-C"] {
        if let Some(value) = input.get(key).and_then(Value::as_u64) {
            label.push_str(&format!(" {key} {value}"));
        }
    }
    match input.get("output_mode").and_then(Value::as_str) {
        Some("files_with_matches") => label.push_str(" -l"),
        Some("count") => label.push_str(" -c"),
        _ => {}
    }
    if let Some(head) = input.get("head_limit").and_then(Value::as_u64) {
        label.push_str(&format!(" | head -{head}"));
    }
    if let Some(glob) = input.get("glob").and_then(Value::as_str) {
        label.push_str(&format!(" --include=\"{glob}\""));
    }
    if let Some(kind) = input.get("type").and_then(Value::as_str) {
        label.push_str(&format!(" --type={kind}"));
    }
    if flag("multiline") {
        label.push_str(" -P");
    }
    if let Some(pattern) = input.get("pattern").and_then(Value::as_str) {
        label.push_str(&format!(" \"{pattern}\""));
    }
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        label.push_str(&format!(" {path}"));
    }
    label
}

/// The `toolUpdateFromToolResult` port (`tools.js:474`) with its four hazards
/// intact: `Read` is rebuilt from the structured output (the raw text carries
/// `<system-reminder>` blocks), `Agent`/`Task` loses its model-directed
/// trailer, `Edit`/`Write` render nothing (their diff comes from the hook),
/// and everything else falls back to the raw content the model saw.
fn tool_result_fields(
    name: &str,
    input: &Value,
    block: &Value,
    structured: Option<&Value>,
    failed: bool,
) -> ToolCallUpdateFields {
    let status = if failed { ToolCallStatus::Failed } else { ToolCallStatus::Completed };
    let raw = block.get("content").cloned().unwrap_or(Value::Null);
    let mut fields = ToolCallUpdateFields::new().status(status);

    // An error result renders its own content and nothing else.
    if failed {
        let text = content_text(&raw);
        if !text.is_empty() {
            fields = fields.content(vec![text_content(text)]);
        }
        return fields;
    }

    match name {
        "Read" => {
            let structured = structured.filter(|value| value.is_object());
            let file = structured.and_then(|value| value.get("file"));
            let content = file.and_then(|file| file.get("content")).and_then(Value::as_str);
            match content.filter(|content| !content.is_empty()) {
                Some(content) => {
                    let start = file
                        .and_then(|file| file.get("startLine"))
                        .and_then(Value::as_u64)
                        .or_else(|| input.get("offset").and_then(Value::as_u64))
                        .unwrap_or(1);
                    let truncated = file
                        .and_then(|file| file.get("truncatedByTokenCap"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let counts = truncated
                        .then(|| {
                            let lines = file.and_then(|file| file.get("numLines"))?.as_u64()?;
                            let total = file.and_then(|file| file.get("totalLines"))?.as_u64()?;
                            Some((lines, total))
                        })
                        .flatten();
                    let view = wire::numbered_read_view(content, start, counts);
                    fields.content(vec![text_content(wire::markdown_escape(&view))])
                }
                None => {
                    let text = content_text(&raw);
                    if text.is_empty() {
                        fields
                    } else {
                        fields.content(vec![text_content(wire::markdown_escape(&text))])
                    }
                }
            }
        }
        "Bash" => {
            let structured = structured.filter(|value| value.is_object());
            let field = |name: &str| structured.and_then(|value| value.get(name));
            let stdout = field("stdout").and_then(Value::as_str);
            let stderr = field("stderr").and_then(Value::as_str);
            let image = field("isImage").and_then(Value::as_bool).unwrap_or(false);
            let background = field("backgroundTaskId").is_some();
            let mut output = match (stdout, stderr, image, background) {
                // The structured stream excludes the model-directed suffixes
                // the raw text carries (stale-read hints, the persisted-output
                // wrapper); the facts they carried are re-established from the
                // structured flags below. Image and backgrounded results keep
                // the raw content, which carries what the structured pair does
                // not.
                (Some(stdout), Some(stderr), false, false) => {
                    let mut joined = String::from(stdout);
                    if !stderr.is_empty() {
                        if !joined.is_empty() {
                            joined.push('\n');
                        }
                        joined.push_str(stderr);
                    }
                    if field("interrupted").and_then(Value::as_bool).unwrap_or(false) {
                        if !joined.is_empty() {
                            joined.push('\n');
                        }
                        // An aborted command is not a success, and the CLI
                        // appends its marker only to the model-facing text.
                        joined.push_str("[Command was aborted before completion]");
                    }
                    if let Some(path) = field("persistedOutputPath").and_then(Value::as_str) {
                        let size = field("persistedOutputSize")
                            .and_then(Value::as_u64)
                            .map(|size| format!(" ({size} bytes total)"))
                            .unwrap_or_default();
                        if !joined.is_empty() {
                            joined.push('\n');
                        }
                        joined.push_str(&format!(
                            "[Output truncated{size}: full output saved to {path}]"
                        ));
                    }
                    joined
                }
                _ => content_text(&raw),
            };
            output.truncate(output.trim_end().len());
            if output.is_empty() {
                fields
            } else {
                fields.content(vec![text_content(format!("```console\n{output}\n```"))])
            }
        }
        "Agent" | "Task" => {
            let structured = structured.filter(|value| value.is_object());
            let completed =
                structured.and_then(|value| value.get("status")).and_then(Value::as_str)
                    == Some("completed");
            let text = if completed {
                content_text(structured.and_then(|value| value.get("content")).unwrap_or(&raw))
            } else {
                content_text(&raw)
            };
            let text = wire::replace_partial_output_note(&wire::strip_agent_trailer(&text));
            if text.trim().is_empty() {
                fields
            } else {
                fields.content(vec![text_content(text)])
            }
        }
        // The diff rides the PostToolUse hook, which carries the real
        // structuredPatch (multi-site replaceAll, context lines, create vs
        // update). Rendering the raw result here would fight it.
        "Edit" | "Write" | "Skill" => fields,
        "ExitPlanMode" => fields.title("Exited Plan Mode".to_string()),
        "WebSearch" => {
            let results = structured
                .and_then(|value| value.get("results"))
                .and_then(Value::as_array)
                .map(|results| {
                    results
                        .iter()
                        .flat_map(|entry| {
                            entry
                                .get("content")
                                .and_then(Value::as_array)
                                .map(|hits| {
                                    hits.iter()
                                        .filter_map(|hit| {
                                            let title = hit.get("title")?.as_str()?;
                                            let url = hit.get("url")?.as_str()?;
                                            Some(format!("{title} ({url})"))
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if results.is_empty() {
                let text = content_text(&raw);
                if text.is_empty() {
                    fields
                } else {
                    fields.content(vec![text_content(text)])
                }
            } else {
                fields.content(vec![text_content(results.join("\n"))])
            }
        }
        _ => {
            let text = content_text(&raw);
            if text.is_empty() {
                fields
            } else {
                fields.content(vec![text_content(text)])
            }
        }
    }
}

/// Flatten a `tool_result` content field (string or block array) to text.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// `toolUpdateFromDiffToolResponse` (`tools.js:1080`): one ACP diff per hunk
/// of the hook's `structuredPatch`, with the context lines the patch carries.
fn diff_from_hook(response: &Value) -> (Vec<ToolCallContent>, Vec<ToolCallLocation>) {
    let mut content = Vec::new();
    let mut locations = Vec::new();
    let Some(path) = response.get("filePath").and_then(Value::as_str) else {
        return (content, locations);
    };
    if let Some(hunks) = response.get("structuredPatch").and_then(Value::as_array) {
        for hunk in hunks {
            let Some(lines) = hunk.get("lines").and_then(Value::as_array) else { continue };
            let mut old_text = Vec::new();
            let mut new_text = Vec::new();
            for line in lines.iter().filter_map(Value::as_str) {
                let (marker, rest) = line.split_at(line.chars().next().map_or(0, char::len_utf8));
                match marker {
                    "-" => old_text.push(rest),
                    "+" => new_text.push(rest),
                    _ => {
                        old_text.push(rest);
                        new_text.push(rest);
                    }
                }
            }
            if old_text.is_empty() && new_text.is_empty() {
                continue;
            }
            let start = hunk.get("newStart").and_then(Value::as_u64).unwrap_or(1);
            locations.push(
                ToolCallLocation::new(PathBuf::from(path))
                    .line(start.try_into().unwrap_or(u32::MAX)),
            );
            content.push(ToolCallContent::Diff(
                Diff::new(PathBuf::from(path), new_text.join("\n"))
                    .old_text((!old_text.is_empty()).then(|| old_text.join("\n"))),
            ));
        }
    }
    // A Write `update` can arrive with an empty patch (nothing changed, or the
    // previous content was too large to diff). Leaving the optimistic
    // tool_use-time diff standing would render an overwrite as a creation.
    if content.is_empty() && response.get("type").and_then(Value::as_str) == Some("update") {
        if let Some(text) = response.get("content").and_then(Value::as_str) {
            locations.push(ToolCallLocation::new(PathBuf::from(path)));
            content.push(match response.get("originalFile").and_then(Value::as_str) {
                Some(original) => ToolCallContent::Diff(
                    Diff::new(PathBuf::from(path), text).old_text(original.to_string()),
                ),
                None => text_content(format!(
                    "Updated `{path}` (previous content too large to diff)"
                )),
            });
        }
    }
    (content, locations)
}

/// The permission option builders (`permissions/options/*.js`), reduced to the
/// three shapes the CLI's own suggestions support: allow once, allow with the
/// suggested rule bundle, reject. The skill/web-fetch/mcp variants all reduce
/// to "allow with updates" with a tool-specific label.
fn permission_options(request: &wire::ControlReq) -> Vec<PermissionOption> {
    let mut options = vec![PermissionOption::new(
        PermissionOptionId::new("allow-once"),
        "Yes",
        PermissionOptionKind::AllowOnce,
    )];
    let label = match request.tool_name.as_str() {
        "Skill" => request
            .input
            .get("skill")
            .and_then(Value::as_str)
            .map(|skill| format!("Yes, and don't ask again for {skill}")),
        "WebFetch" => request
            .input
            .get("url")
            .and_then(Value::as_str)
            .and_then(host_of)
            .map(|host| format!("Yes, and don't ask again for {host}")),
        tool if !tool.is_empty() => Some(format!("Yes, and don't ask again for {tool} commands")),
        _ => None,
    };
    let suggestions = request
        .permission_suggestions
        .as_array()
        .map(|updates| !updates.is_empty())
        .unwrap_or(false);
    if let (true, Some(label)) = (suggestions, label) {
        options.push(PermissionOption::new(
            PermissionOptionId::new("allow-with-updates"),
            label,
            PermissionOptionKind::AllowAlways,
        ));
    }
    options.push(PermissionOption::new(
        PermissionOptionId::new("reject"),
        "No",
        PermissionOptionKind::RejectOnce,
    ));
    options
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_string())
}

/// A picked permission option: the control response to write, plus what that
/// answer CHANGES once it is on the wire.
struct PermissionAnswer {
    response: Value,
    effects: PermissionEffects,
}

/// What a permission answer does BESIDES answering. Held apart from the
/// response because a request the CLI cancelled while its card was open is
/// never answered at all: arming a plan restart for one of those cleared the
/// context and ran a build turn for an approval that no longer existed.
#[derive(Default)]
struct PermissionEffects {
    /// The session mode the answer switches into.
    mode: Option<String>,
    /// The accepted plan to re-prompt in a fresh context.
    plan_restart: Option<PlanRestart>,
}

impl PermissionAnswer {
    /// An answer that changes nothing.
    fn plain(response: Value) -> PermissionAnswer {
        PermissionAnswer {
            response,
            effects: PermissionEffects::default(),
        }
    }
}

/// Fold an answered permission's effects into the session state. Returns the
/// mode the client has to be told about, if it moved.
fn take_permission_effects(state: &mut State, effects: PermissionEffects) -> Option<String> {
    if let Some(restart) = effects.plan_restart {
        state.pending_plan_restart = Some(restart);
    }
    let mode = effects.mode?;
    state.mode = mode.clone();
    Some(mode)
}

/// The control response for the option the user picked. PURE by design: every
/// consequence rides [`PermissionEffects`], so nothing has happened yet if the
/// response never reaches the CLI.
fn permission_answer(
    option_id: &str,
    request: &wire::ControlReq,
    tool_use_id: &str,
) -> PermissionAnswer {
    if request.is_exit_plan_mode() {
        if let Some(mode) = exit_plan_clear_context_mode(option_id) {
            // NOT an allow: allowing would run ExitPlanMode in the old
            // context before the hand-off. The interrupt is consumed by
            // `on_result`, which re-prompts the plan in a fresh context.
            let plan = request
                .input
                .get("plan")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            return PermissionAnswer {
                response: wire::permission_deny(
                    tool_use_id,
                    "User accepted the plan and requested a fresh context",
                    true,
                ),
                effects: PermissionEffects {
                    mode: None,
                    plan_restart: Some(PlanRestart {
                        plan,
                        mode: mode.to_string(),
                    }),
                },
            };
        }
        if let Some(mode) = exit_plan_mode(option_id) {
            let mut allow = wire::permission_allow(tool_use_id, request.input.clone());
            allow["updatedPermissions"] =
                json!([{ "type": "setMode", "mode": mode, "destination": "session" }]);
            if mode != "default" {
                allow["decisionClassification"] = json!("user_permanent");
            }
            return PermissionAnswer {
                response: allow,
                effects: PermissionEffects {
                    // Clamped like a steered switch: the toggle the approval
                    // moves is the one `available_modes` advertises.
                    mode: Some(clamp_mode(mode)),
                    plan_restart: None,
                },
            };
        }
        if option_id == "reject" {
            // A plain deny lets claude keep planning; the interrupt stops
            // this turn so the user can steer instead.
            return PermissionAnswer::plain(wire::permission_deny(
                tool_use_id,
                "User chose to keep planning",
                true,
            ));
        }
    }
    let response = match option_id {
        "allow-once" => wire::permission_allow(tool_use_id, request.input.clone()),
        "allow-with-updates" => {
            let mut allow = wire::permission_allow(tool_use_id, request.input.clone());
            if let Some(updates) = request.permission_suggestions.as_array() {
                allow["updatedPermissions"] = json!(updates);
                allow["decisionClassification"] = json!("user_permanent");
            }
            allow
        }
        "reject" => wire::permission_deny(tool_use_id, "User refused permission to run tool", false),
        other => {
            log::warn!("engine: claude permission option {other} is not one we offered");
            wire::permission_deny(tool_use_id, "User refused permission to run tool", false)
        }
    };
    PermissionAnswer::plain(response)
}

/// The plan-approval menu. EXP-772: permissions are bypassed in every mode,
/// so "manually approve edits" is no longer an answer that means anything —
/// what is left is code it, code it in a FRESH context (only when there is a
/// plan to carry), or keep planning.
fn exit_plan_options(input: &Value) -> Vec<PermissionOption> {
    let plan = input.get("plan").and_then(Value::as_str).unwrap_or_default();
    let mut options = Vec::new();
    if !plan.trim().is_empty() {
        options.push(PermissionOption::new(
            PermissionOptionId::new("exit-plan-clear-bypass"),
            "Yes, and start with a fresh context",
            PermissionOptionKind::AllowAlways,
        ));
    }
    options.push(PermissionOption::new(
        PermissionOptionId::new("exit-plan-bypass"),
        "Yes",
        PermissionOptionKind::AllowAlways,
    ));
    options.push(PermissionOption::new(
        PermissionOptionId::new("reject"),
        "No, keep planning",
        PermissionOptionKind::RejectOnce,
    ));
    options
}

/// The mode a plan option approves into, or `None` when it is a clear-context
/// option (which denies and re-prompts instead).
fn exit_plan_mode(option_id: &str) -> Option<&'static str> {
    match option_id {
        "exit-plan-auto" => Some("auto"),
        "exit-plan-bypass" => Some("bypassPermissions"),
        "exit-plan-accept-edits" => Some("acceptEdits"),
        "exit-plan-default" => Some("default"),
        _ => None,
    }
}

fn exit_plan_clear_context_mode(option_id: &str) -> Option<&'static str> {
    match option_id {
        "exit-plan-clear-auto" => Some("auto"),
        "exit-plan-clear-bypass" => Some("bypassPermissions"),
        "exit-plan-clear-accept-edits" => Some("acceptEdits"),
        _ => None,
    }
}

struct AskQuestion {
    question: String,
    header: Option<String>,
    multi_select: bool,
    options: Vec<(String, Option<String>)>,
}

fn ask_questions(input: &Value) -> Vec<AskQuestion> {
    input
        .get("questions")
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .filter_map(|question| {
                    let text = question.get("question")?.as_str()?.to_string();
                    let options = question
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|options| {
                            options
                                .iter()
                                .filter_map(|option| {
                                    let label = option.get("label")?.as_str()?.to_string();
                                    let description = option
                                        .get("description")
                                        .and_then(Value::as_str)
                                        .map(str::to_string);
                                    Some((label, description))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(AskQuestion {
                        question: text,
                        header: question
                            .get("header")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        multi_select: question
                            .get("multiSelect")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        options,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn question_field(index: usize) -> String {
    format!("question_{index}")
}

fn question_custom_field(index: usize) -> String {
    format!("question_{index}_custom")
}

/// `askUserQuestionsToCreateRequest` (`elicitation.js:115`): one field per
/// question plus a sibling free-text "Other" field, nothing required (the
/// built-in tool has its own Skip affordance).
fn ask_elicitation(
    session_id: &SessionId,
    tool_use_id: &str,
    questions: &[AskQuestion],
) -> CreateElicitationRequest {
    let single = questions.len() == 1;
    let mut schema = ElicitationSchema::new();
    for (index, question) in questions.iter().enumerate() {
        let options: Vec<EnumOption> = question
            .options
            .iter()
            .map(|(label, description)| {
                EnumOption::new(label.clone(), label.clone()).description(description.clone())
            })
            .collect();
        let description = (!single).then(|| question.question.clone());
        let property: ElicitationPropertySchema = if question.multi_select {
            let values: Vec<String> =
                question.options.iter().map(|(label, _)| label.clone()).collect();
            ElicitationPropertySchema::Array(
                MultiSelectPropertySchema::new(values)
                    .title(question.header.clone())
                    .description(description),
            )
        } else {
            ElicitationPropertySchema::String(
                StringPropertySchema::new()
                    .title(question.header.clone())
                    .description(description)
                    .one_of(options),
            )
        };
        schema = schema.property(question_field(index), property, false);
        schema = schema.property(
            question_custom_field(index),
            ElicitationPropertySchema::String(
                StringPropertySchema::new().title("Other".to_string()).description(
                    "Type your own answer instead of choosing an option above (optional)."
                        .to_string(),
                ),
            ),
            false,
        );
    }
    let message = if single {
        questions[0].question.clone()
    } else {
        "Please answer the following questions.".to_string()
    };
    CreateElicitationRequest::new(
        ElicitationFormMode::new(
            ElicitationSessionScope::new(session_id.clone())
                .tool_call_id(ToolCallId::new(tool_use_id)),
            schema,
        ),
        message,
    )
}

/// `applyAskElicitationResponse`: answers are keyed by the question TEXT (not
/// by the field key — measured), a multi-select joins with `, `, and a
/// non-empty custom answer WINS over the selection.
fn ask_answers(
    questions: &[AskQuestion],
    content: &BTreeMap<String, ElicitationContentValue>,
) -> Map<String, Value> {
    let mut answers = Map::new();
    for (index, question) in questions.iter().enumerate() {
        if let Some(ElicitationContentValue::String(custom)) =
            content.get(&question_custom_field(index))
        {
            if !custom.trim().is_empty() {
                answers.insert(question.question.clone(), json!(custom.trim()));
                continue;
            }
        }
        let text = match content.get(&question_field(index)) {
            Some(ElicitationContentValue::String(value)) => value.clone(),
            Some(ElicitationContentValue::StringArray(values)) => values.join(", "),
            Some(ElicitationContentValue::Integer(value)) => value.to_string(),
            Some(ElicitationContentValue::Number(value)) => value.to_string(),
            Some(ElicitationContentValue::Boolean(value)) => value.to_string(),
            _ => continue,
        };
        if text.is_empty() {
            continue;
        }
        answers.insert(question.question.clone(), json!(text));
    }
    answers
}

/// The prompt as plain text — what the mapper's `user_message` and the CLI's
/// own local-command detection both key on.
fn prompt_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            ContentBlock::ResourceLink(link) => Some(link.uri.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One stdin `user` message. Text is rewritten to the CLI's own MCP command
/// spelling; images ride as Anthropic image blocks so a steered screenshot
/// (EXP-511) reaches the model instead of a bare attachment URL.
fn claude_user_message(blocks: &[ContentBlock], text: &str) -> Value {
    let mut content = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(_) | ContentBlock::ResourceLink(_) => {}
            ContentBlock::Image(image) => {
                content.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "data": image.data,
                        "media_type": image.mime_type,
                    },
                }));
            }
            _ => {}
        }
    }
    let text = wire::prompt_to_claude(text);
    if !text.is_empty() {
        content.insert(0, json!({ "type": "text", "text": text }));
    }
    json!({
        "type": "user",
        "message": { "role": "user", "content": content },
        "session_id": Value::Null,
        "parent_tool_use_id": Value::Null,
        // `origin.kind` is load-bearing: the CLI's `isHuman` trust gates fail
        // CLOSED without it.
        "origin": { "kind": "human" },
    })
}

fn image_block(block: &Value) -> Option<ContentBlock> {
    let source = block.get("source")?;
    let data = source.get("data")?.as_str()?;
    let media_type = source.get("media_type").and_then(Value::as_str).unwrap_or("image/png");
    Some(ContentBlock::Image(
        agent_client_protocol::schema::v1::ImageContent::new(data, media_type),
    ))
}

// ---------------------------------------------------------------------------
// transcripts on disk (`session/list`, `session/load`)
// ---------------------------------------------------------------------------

/// `~/.claude/projects` — every project directory, not just the munged one: a
/// worktree can differ from where claude persisted the conversation.
///
/// `CLAUDE_CONFIG_DIR` is read from the CHILD's environment, not this
/// process's: the transcripts that matter are the ones the CLI we spawn would
/// write, and `PreparedLaunch::spawn.env` is what it runs with.
fn claude_projects_root(env: &[(String, String)]) -> Option<PathBuf> {
    let config = match env
        .iter()
        .find(|(key, _)| key == "CLAUDE_CONFIG_DIR")
        .map(|(_, value)| PathBuf::from(value))
        .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
    {
        Some(dir) => dir,
        None => PathBuf::from(std::env::var_os("HOME")?).join(".claude"),
    };
    let projects = config.join("projects");
    projects.is_dir().then_some(projects)
}

/// EXP-761: `coding::locate_claude_transcript`, the ONE locator (by session
/// id under every project dir — claude hash-suffixes long cwd names).
fn transcript_path(env: &[(String, String)], session_id: &str) -> Option<PathBuf> {
    let root = claude_projects_root(env)?;
    coding::locate_claude_transcript(&root, session_id)
}

/// Past conversations for `session/list`, newest first. `cwd` filters to the
/// transcripts recorded for that directory when the file says so; a
/// transcript whose cwd is unreadable is kept (a moved worktree still lists).
fn transcript_sessions(env: &[(String, String)], cwd: &Path) -> Vec<SessionInfo> {
    let Some(root) = claude_projects_root(env) else { return Vec::new() };
    let Ok(projects) = std::fs::read_dir(root) else { return Vec::new() };
    let mut rows: Vec<(std::time::SystemTime, SessionInfo)> = Vec::new();
    for project in projects.filter_map(Result::ok) {
        let Ok(entries) = std::fs::read_dir(project.path()) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(session_id) = path.file_stem().and_then(|stem| stem.to_str()) else { continue };
            let Ok(metadata) = entry.metadata() else { continue };
            let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
            let (recorded_cwd, title) = transcript_head(&path);
            if let Some(recorded) = &recorded_cwd {
                if Path::new(recorded) != cwd {
                    continue;
                }
            }
            rows.push((
                modified,
                SessionInfo::new(SessionId::new(session_id.to_string()), cwd.to_path_buf())
                    .title(title),
            ));
        }
    }
    rows.sort_by(|left, right| right.0.cmp(&left.0));
    rows.into_iter().map(|(_, info)| info).collect()
}

/// The transcript's own cwd and a title, read from its first few lines (a
/// transcript can be megabytes; the head carries both).
fn transcript_head(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else { return (None, None) };
    let mut cwd = None;
    let mut title = None;
    for line in text.lines().take(256) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        if cwd.is_none() {
            cwd = value.get("cwd").and_then(Value::as_str).map(str::to_string);
        }
        if title.is_none() {
            // Three spellings across CLI versions; `aiTitle` is what 2.1.263
            // writes.
            title = ["summary", "aiTitle", "title"]
                .iter()
                .find_map(|key| value.get(*key).and_then(Value::as_str))
                .map(str::to_string);
        }
        if cwd.is_some() && title.is_some() {
            break;
        }
    }
    (cwd, title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> PathBuf {
        PathBuf::from("/work/tree")
    }

    fn task(turn_seq: u64, live: bool, age: Duration) -> TaskEntry {
        TaskEntry {
            tool_use_id: Some("toolu_1".to_string()),
            subagent_type: Some("explore".to_string()),
            live,
            backgrounded: true,
            turn_seq,
            started_at: Instant::now() - age,
            last_status: None,
        }
    }

    /// EXP-780 — the "Working…" wedge. A task the CLI never reported back on
    /// used to defer EVERY later turn forever, because the deferral gate
    /// asked "is ANY task live" over a map nothing ever pruned.
    #[test]
    fn a_stale_task_blocks_only_its_own_turn_and_only_for_a_while() {
        let mut state = State { turn_seq: 4, ..State::default() };

        // Live, this turn, fresh: the deliberate #864/#866 deferral.
        state.tasks.insert("t-now".to_string(), task(4, true, Duration::ZERO));
        assert_eq!(blocking_tasks(&state).count(), 1);

        // An EARLIER turn's live task is not this turn's problem.
        state.tasks.clear();
        state.tasks.insert("t-old".to_string(), task(3, true, Duration::ZERO));
        assert_eq!(blocking_tasks(&state).count(), 0);

        // Neither is one this turn started and then went silent about.
        state.tasks.clear();
        state.tasks.insert(
            "t-lost".to_string(),
            task(4, true, TASK_MAX_LIFETIME + Duration::from_secs(1)),
        );
        assert_eq!(blocking_tasks(&state).count(), 0);

        // A task that reported terminal never blocks at all.
        state.tasks.clear();
        state.tasks.insert("t-done".to_string(), task(4, false, Duration::ZERO));
        assert_eq!(blocking_tasks(&state).count(), 0);
    }

    /// A settled turn drops the dead tasks of every EARLIER turn, so `tasks`
    /// cannot grow for the life of the process.
    #[test]
    fn settling_a_turn_prunes_the_tasks_of_the_ones_before_it() {
        let mut state = State { turn_seq: 7, ..State::default() };
        state.tasks.insert("old-done".to_string(), task(2, false, Duration::ZERO));
        state.tasks.insert("old-live".to_string(), task(2, true, Duration::ZERO));
        state.tasks.insert("now-done".to_string(), task(7, false, Duration::ZERO));
        let (tx, _rx) = flume::bounded(1);
        state.turns.push_back(tx);

        settle_turns(
            &mut state,
            DeferredSettle {
                outcome: TurnOutcome::EndTurn,
                queued: None,
            },
        );

        // The finished task of an old turn goes; a still-live one and this
        // turn's own bookkeeping (the duplicate-edge memo) stay.
        assert!(!state.tasks.contains_key("old-done"));
        assert!(state.tasks.contains_key("old-live"));
        assert!(state.tasks.contains_key("now-done"));
    }

    /// Two in-flight prompts, one `result`: `queued_turn_count` is the only
    /// thing that says whether the second was FOLDED INTO this turn (settle it
    /// too) or QUEUED behind it (leave it waiting for its own `result`).
    #[test]
    fn a_result_settles_the_steers_the_cli_folded_into_it() {
        fn two_turns() -> (State, flume::Receiver<TurnOutcome>, flume::Receiver<TurnOutcome>) {
            let mut state = State::default();
            let (first_tx, first) = flume::bounded(1);
            let (second_tx, second) = flume::bounded(1);
            state.turns.push_back(first_tx);
            state.turns.push_back(second_tx);
            (state, first, second)
        }

        // Folded in: one `result`, nothing left queued behind it.
        let (mut folded, first, second) = two_turns();
        settle_turns(
            &mut folded,
            DeferredSettle {
                outcome: TurnOutcome::EndTurn,
                queued: Some(0),
            },
        );
        assert_eq!(first.try_recv(), Ok(TurnOutcome::EndTurn));
        assert_eq!(second.try_recv(), Ok(TurnOutcome::EndTurn));
        assert!(folded.turns.is_empty());

        // Queued: the CLI still holds one turn, which owns the NEXT `result`.
        let (mut queued, first, second) = two_turns();
        settle_turns(
            &mut queued,
            DeferredSettle {
                outcome: TurnOutcome::EndTurn,
                queued: Some(1),
            },
        );
        assert_eq!(first.try_recv(), Ok(TurnOutcome::EndTurn));
        assert!(second.try_recv().is_err(), "the queued turn keeps waiting");
        assert_eq!(queued.turns.len(), 1);

        // No count at all: nothing is assumed folded.
        let (mut silent, first, second) = two_turns();
        settle_turns(
            &mut silent,
            DeferredSettle {
                outcome: TurnOutcome::EndTurn,
                queued: None,
            },
        );
        assert_eq!(first.try_recv(), Ok(TurnOutcome::EndTurn));
        assert!(second.try_recv().is_err());
        assert_eq!(silent.turns.len(), 1);
    }

    #[test]
    fn the_tool_table_matches_the_upstream_case_for_case() {
        let read = tool_info(
            "Read",
            &json!({ "file_path": "/work/tree/src/main.rs", "offset": 10, "limit": 5 }),
            &cwd(),
        );
        assert_eq!(read.title, "Read src/main.rs (10 - 14)");
        assert_eq!(read.kind, ToolKind::Read);
        assert_eq!(read.locations.len(), 1);

        let bash = tool_info(
            "Bash",
            &json!({ "command": "ls -la", "description": "List files" }),
            &cwd(),
        );
        assert_eq!(bash.title, "ls -la");
        assert_eq!(bash.kind, ToolKind::Execute);
        // Never a terminal: the client advertises `terminal: false`.
        assert!(matches!(bash.content.first(), Some(ToolCallContent::Content(_))));

        let edit = tool_info(
            "Edit",
            &json!({ "file_path": "/work/tree/a.rs", "old_string": "a", "new_string": "b" }),
            &cwd(),
        );
        assert_eq!(edit.title, "Edit a.rs");
        assert_eq!(edit.kind, ToolKind::Edit);
        match edit.content.first() {
            Some(ToolCallContent::Diff(diff)) => {
                assert_eq!(diff.old_text.as_deref(), Some("a"));
                assert_eq!(diff.new_text, "b");
            }
            other => panic!("expected a diff, got {other:?}"),
        }

        let plan = tool_info("ExitPlanMode", &json!({ "plan": "# Plan" }), &cwd());
        assert_eq!(plan.title, "Approve Plan");
        // The plan card hangs off this kind on all four clients.
        assert_eq!(plan.kind, ToolKind::SwitchMode);

        let grep = tool_info(
            "Grep",
            &json!({ "-i": true, "pattern": "fn main", "path": "src", "glob": "*.rs" }),
            &cwd(),
        );
        assert_eq!(grep.title, "grep -i --include=\"*.rs\" \"fn main\" src");

        // Everything the upstream never special-cased falls through, MultiEdit
        // and every mcp__ tool included.
        let unknown = tool_info("mcp__exponential__issues_get", &json!({}), &cwd());
        assert_eq!(unknown.title, "mcp__exponential__issues_get");
        assert_eq!(unknown.kind, ToolKind::Other);
    }

    #[test]
    fn a_read_result_is_rebuilt_from_its_structured_output() {
        let block = json!({
            "tool_use_id": "t1",
            "content": "     1\tline\n<system-reminder>never show this</system-reminder>",
        });
        let structured = json!({
            "type": "text",
            "file": { "content": "line one\nline two\n", "startLine": 4 },
        });
        let fields = tool_result_fields(
            "Read",
            &json!({ "file_path": "/work/tree/a.rs" }),
            &block,
            Some(&structured),
            false,
        );
        let content = fields.content.expect("a rebuilt view");
        match content.first() {
            Some(ToolCallContent::Content(content)) => match &content.content {
                ContentBlock::Text(text) => {
                    assert!(text.text.contains("4\tline one"));
                    assert!(text.text.contains("5\tline two"));
                    assert!(!text.text.contains("system-reminder"));
                }
                other => panic!("expected text, got {other:?}"),
            },
            other => panic!("expected content, got {other:?}"),
        }
    }

    #[test]
    fn an_edit_result_renders_nothing_because_the_hook_owns_the_diff() {
        let block = json!({ "tool_use_id": "t1", "content": "The file has been updated." });
        let fields = tool_result_fields("Edit", &Value::Null, &block, None, false);
        assert!(fields.content.is_none());
        assert_eq!(fields.status, Some(ToolCallStatus::Completed));
        // …and the hook's structuredPatch is what actually renders.
        let (content, locations) = diff_from_hook(&json!({
            "filePath": "/work/tree/a.rs",
            "structuredPatch": [{ "newStart": 3, "lines": [" keep", "-old", "+new"] }],
        }));
        assert_eq!(locations.len(), 1);
        match content.first() {
            Some(ToolCallContent::Diff(diff)) => {
                assert_eq!(diff.old_text.as_deref(), Some("keep\nold"));
                assert_eq!(diff.new_text, "keep\nnew");
            }
            other => panic!("expected a diff, got {other:?}"),
        }
    }

    #[test]
    fn an_agent_result_loses_its_model_directed_trailer() {
        let block = json!({
            "tool_use_id": "t1",
            "content": [{ "type": "text", "text": "The report\nagentId: abc (use SendMessage)" }],
        });
        let fields = tool_result_fields("Task", &Value::Null, &block, None, false);
        match fields.content.as_ref().and_then(|content| content.first()) {
            Some(ToolCallContent::Content(content)) => match &content.content {
                ContentBlock::Text(text) => assert_eq!(text.text, "The report"),
                other => panic!("expected text, got {other:?}"),
            },
            other => panic!("expected content, got {other:?}"),
        }
    }

    /// EXP-761: one context window per session. The id heuristic bridges
    /// until the first report; a report for the session's own model is
    /// final, so a later helper-only turn (haiku, 200000) cannot flip the
    /// published size back and forth.
    #[test]
    fn the_context_window_is_taken_once_per_session() {
        use wire::ContextWindowReport;
        let mut window = ContextWindow::default();
        assert_eq!(window.size(), 0, "nothing published before a model is known");
        window.infer("claude-opus-5");
        assert_eq!(window.size(), 200_000);
        // A second inference (message_start) never re-guesses.
        window.infer("claude-opus-5[1m]");
        assert_eq!(window.size(), 200_000);
        // The turn's result: exact for the session model — final.
        window.report(ContextWindowReport { window: 1_000_000, exact: true });
        assert_eq!(window.size(), 1_000_000);
        window.report(ContextWindowReport { window: 200_000, exact: false });
        window.report(ContextWindowReport { window: 200_000, exact: true });
        assert_eq!(window.size(), 1_000_000, "locked for the session");

        // A fallback read (the id spelled differently) holds until an exact
        // one arrives, and is never re-derived by another fallback.
        let mut window = ContextWindow::Inferred(200_000);
        window.report(ContextWindowReport { window: 1_000_000, exact: false });
        assert_eq!(window.size(), 1_000_000);
        window.report(ContextWindowReport { window: 200_000, exact: false });
        assert_eq!(window.size(), 1_000_000);
        window.report(ContextWindowReport { window: 500_000, exact: true });
        assert_eq!(window.size(), 500_000);

        // ...but a LIVE model switch is a new model: the locked window goes
        // and the new id is re-derived, so 1M ↔ 200k publishes the size of
        // the model actually running.
        let mut window = ContextWindow::Reported { window: 1_000_000, exact: true };
        window.switch_model("claude-opus-5");
        assert_eq!(window.size(), 200_000, "kept the old model's window");
        window.report(ContextWindowReport { window: 200_000, exact: true });
        window.switch_model("claude-opus-5[1m]");
        assert_eq!(window.size(), 1_000_000);
    }

    /// EXP-772: approving a plan means coding it, in this context or a fresh
    /// one — every answer lands on `bypassPermissions`.
    #[test]
    fn the_plan_menu_offers_the_clear_context_option_first() {
        let options = exit_plan_options(&json!({ "plan": "# Plan" }));
        let ids: Vec<String> =
            options.iter().map(|option| option.option_id.0.to_string()).collect();
        assert_eq!(
            ids,
            vec![
                "exit-plan-clear-bypass".to_string(),
                "exit-plan-bypass".to_string(),
                "reject".to_string(),
            ]
        );
        // A plan-less approval has nothing to carry into a fresh context.
        let ids: Vec<String> = exit_plan_options(&json!({}))
            .iter()
            .map(|option| option.option_id.0.to_string())
            .collect();
        assert_eq!(ids, vec!["exit-plan-bypass".to_string(), "reject".to_string()]);
        // The clear-context option DENIES with an interrupt; it never allows.
        assert_eq!(
            exit_plan_clear_context_mode("exit-plan-clear-bypass"),
            Some("bypassPermissions")
        );
        assert_eq!(exit_plan_mode("exit-plan-clear-bypass"), None);
        assert_eq!(exit_plan_mode("exit-plan-bypass"), Some("bypassPermissions"));
    }

    fn exit_plan_request() -> wire::ControlReq {
        wire::ControlReq {
            subtype: "can_use_tool".into(),
            tool_name: "ExitPlanMode".into(),
            input: json!({ "plan": "# Plan\n\n1. Do the thing" }),
            ..wire::ControlReq::default()
        }
    }

    /// Approving a plan with "Yes" moves the session to Build: the state has
    /// to move WITH the CLI, and the client is told, or its Plan/Build toggle
    /// stays stuck on Plan for the rest of the run.
    #[test]
    fn a_plan_approval_switches_the_mode_and_announces_it() {
        let request = exit_plan_request();
        let answer = permission_answer("exit-plan-bypass", &request, "toolu_1");
        assert_eq!(answer.response["behavior"], json!("allow"));
        assert_eq!(
            answer.response["updatedPermissions"][0]["mode"],
            json!("bypassPermissions")
        );
        assert_eq!(answer.effects.mode.as_deref(), Some("bypassPermissions"));

        // Folding the effects in moves the state and yields the mode to
        // announce as a `CurrentModeUpdate` + a fresh `config_state`.
        let mut state = State { mode: "plan".to_string(), ..State::default() };
        let announced = take_permission_effects(&mut state, answer.effects);
        assert_eq!(announced.as_deref(), Some("bypassPermissions"));
        assert_eq!(state.mode, "bypassPermissions");
        assert!(state.pending_plan_restart.is_none());

        // Keeping the plan changes nothing.
        let keep = permission_answer("reject", &request, "toolu_1");
        let mut state = State { mode: "plan".to_string(), ..State::default() };
        assert_eq!(take_permission_effects(&mut state, keep.effects), None);
        assert_eq!(state.mode, "plan");
    }

    /// A CANCELLED plan approval is never answered, so it must not arm the
    /// restart either: it used to `/clear` the context and run a build turn
    /// for an approval the CLI had already abandoned.
    #[test]
    fn a_cancelled_plan_approval_arms_no_restart() {
        let request = exit_plan_request();
        let answer = permission_answer("exit-plan-clear-bypass", &request, "toolu_1");
        // Computing the answer touches no state at all.
        assert_eq!(answer.response["behavior"], json!("deny"));
        assert!(answer.effects.plan_restart.is_some());

        // `answer_can_use_tool` takes the abort BEFORE it writes anything, and
        // the effects ride the write.
        let mut state = State::default();
        state.answering.insert("req-1".to_string());
        state.aborted_requests.insert("req-1".to_string());
        state.answering.remove("req-1");
        let aborted = state.aborted_requests.remove("req-1");
        assert!(aborted);
        if !aborted {
            take_permission_effects(&mut state, answer.effects);
        }
        assert!(state.pending_plan_restart.is_none(), "the abandoned plan armed a restart");

        // The same answer on a request that WAS written arms it.
        let answer = permission_answer("exit-plan-clear-bypass", &request, "toolu_1");
        let mut state = State::default();
        assert_eq!(take_permission_effects(&mut state, answer.effects), None);
        let restart = state.pending_plan_restart.expect("the accepted plan is armed");
        assert_eq!(restart.mode, "bypassPermissions");
        assert!(restart.plan.starts_with("# Plan"));
    }

    /// `system/init` reports the mode the CLI actually launched in, in its own
    /// spelling: it goes through the same clamp a steered switch does, since
    /// `available_modes` only ever offers `plan` and `bypassPermissions`.
    #[test]
    fn the_init_frame_mode_is_clamped_to_the_offered_ones() {
        let offered: Vec<String> =
            available_modes().iter().map(|mode| mode.id.0.to_string()).collect();
        for reported in ["acceptEdits", "default", "auto", "dontAsk", "bypassPermissions"] {
            let adopted = init_mode(Some(reported), "plan").expect("a change off plan");
            assert_eq!(adopted, "bypassPermissions", "{reported}");
            assert!(offered.contains(&adopted));
        }
        // Plan is adopted as itself.
        assert_eq!(init_mode(Some("plan"), "bypassPermissions").as_deref(), Some("plan"));
        // Nothing to adopt: no frame value, an empty one, or one that clamps
        // to the mode already current.
        assert_eq!(init_mode(None, "plan"), None);
        assert_eq!(init_mode(Some(""), "plan"), None);
        assert_eq!(init_mode(Some("acceptEdits"), "bypassPermissions"), None);
    }

    #[test]
    fn a_permission_offers_dont_ask_again_only_with_a_suggestion() {
        let request = wire::ControlReq {
            subtype: "can_use_tool".into(),
            tool_name: "Bash".into(),
            permission_suggestions: json!([{ "type": "addRules" }]),
            ..wire::ControlReq::default()
        };
        let ids: Vec<String> = permission_options(&request)
            .iter()
            .map(|option| option.option_id.0.to_string())
            .collect();
        assert_eq!(ids, vec!["allow-once", "allow-with-updates", "reject"]);

        let bare = wire::ControlReq {
            subtype: "can_use_tool".into(),
            tool_name: "Bash".into(),
            ..wire::ControlReq::default()
        };
        let ids: Vec<String> =
            permission_options(&bare).iter().map(|option| option.option_id.0.to_string()).collect();
        assert_eq!(ids, vec!["allow-once", "reject"]);
        assert_eq!(host_of("https://example.com/a?b").as_deref(), Some("example.com"));
    }

    #[test]
    fn ask_user_question_answers_are_keyed_by_the_question_text() {
        let input = json!({
            "questions": [{
                "question": "Tabs or spaces?",
                "header": "Indentation",
                "options": [
                    { "label": "Spaces", "description": "The common one" },
                    { "label": "Tabs" },
                ],
            }],
        });
        let questions = ask_questions(&input);
        assert_eq!(questions.len(), 1);
        let request = ask_elicitation(&SessionId::new("s1"), "toolu_1", &questions);
        assert_eq!(request.message, "Tabs or spaces?");
        let schema = match &request.mode {
            agent_client_protocol::schema::v1::ElicitationMode::Form(form) => {
                &form.requested_schema
            }
            other => panic!("expected a form, got {other:?}"),
        };
        assert!(schema.properties.contains_key("question_0"));
        assert!(schema.properties.contains_key("question_0_custom"));
        // Nothing is required — the built-in tool has its own Skip.
        assert!(schema.required.as_ref().map(Vec::is_empty).unwrap_or(true));

        let mut content = BTreeMap::new();
        content.insert(
            "question_0".to_string(),
            ElicitationContentValue::String("Tabs".to_string()),
        );
        let answers = ask_answers(&questions, &content);
        assert_eq!(answers.get("Tabs or spaces?"), Some(&json!("Tabs")));

        // A typed answer beats the selection.
        content.insert(
            "question_0_custom".to_string(),
            ElicitationContentValue::String("  two spaces  ".to_string()),
        );
        let answers = ask_answers(&questions, &content);
        assert_eq!(answers.get("Tabs or spaces?"), Some(&json!("two spaces")));
    }

    #[test]
    fn todo_write_becomes_the_plan() {
        let entries = todo_entries(&json!({
            "todos": [
                { "content": "Write it", "activeForm": "Writing it", "status": "in_progress" },
                { "content": "Ship it", "status": "pending" },
            ],
        }));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "Writing it");
        assert_eq!(entries[0].status, PlanEntryStatus::InProgress);
        assert_eq!(entries[1].content, "Ship it");
        assert_eq!(entries[1].status, PlanEntryStatus::Pending);
    }

    #[test]
    fn the_user_message_carries_images_beside_its_text() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("look at this")),
            ContentBlock::Image(agent_client_protocol::schema::v1::ImageContent::new(
                "AAAA", "image/png",
            )),
        ];
        let text = prompt_text(&blocks);
        assert_eq!(text, "look at this");
        let message = claude_user_message(&blocks, &text);
        assert_eq!(message["message"]["content"][0]["text"], json!("look at this"));
        assert_eq!(message["message"]["content"][1]["source"]["data"], json!("AAAA"));
        assert_eq!(message["origin"]["kind"], json!("human"));
    }

    /// EXP-772: plan on, plan off. Nothing else is steerable — every other
    /// permission mode differs in nothing a user can see.
    #[test]
    fn the_mode_picker_offers_plan_and_build_only() {
        let ids: Vec<String> =
            available_modes().iter().map(|mode| mode.id.0.to_string()).collect();
        assert_eq!(ids, vec!["plan".to_string(), "bypassPermissions".to_string()]);
        let labels: Vec<String> =
            available_modes().iter().map(|mode| mode.name.clone()).collect();
        assert_eq!(labels, vec!["Plan".to_string(), "Build".to_string()]);
    }
}
