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
//! - The argv keeps `--settings <claude-hooks/<pid>/<sid>.settings.json>`:
//!   that path in the process command line is the reaper's only way to find
//!   an escaped claude (EXP-300), and the file pins auto mode OFF (EXP-1124:
//!   plan mode otherwise inherits a user `defaultMode: auto` and routes every
//!   tool call through the classifier, never reaching `can_use_tool`).
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
    SessionConfigOptionCategory, SessionConfigOptionValue, SessionConfigSelectOption, SessionId,
    SessionInfo, SessionInfoUpdate, SessionListCapabilities, SessionMode, SessionModeId, SessionModeState,
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
use crate::host::{CANCEL_QUEUED_META_KEY, NATIVE_SESSION_META_KEY};
use crate::mapper::{API_ERROR_META_KEY, PERMISSION_OPTION_DESCRIPTION_META, RATE_LIMIT_META_KEY};
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

/// EXP-850: the three `_meta` keys this adapter's new state rides on, plus
/// the two a WAIT tool row carries (ACP v1 has no `wait` kind).
pub use crate::local::{
    BACKGROUND_TASKS_META_KEY, TOOL_DETAIL_META_KEY, TOOL_KIND_META_KEY, TURN_META_KEY,
    TURN_TOKENS_META_KEY, WORKFLOW_META_KEY,
};

/// The subagents the CLI ships. They are spawned by the model, never picked
/// for the main thread, so the `agent` option offers only what the user (or a
/// plugin) configured.
const BUILTIN_AGENT_NAMES: [&str; 5] =
    ["claude", "general-purpose", "Explore", "Plan", "statusline-setup"];

/// The config option ids `session/set_config_option` still ACCEPTS. EXP-772
/// retired the chips themselves; EXP-877 advertises [`CONFIG_MODEL`] again as
/// a VALUE only (no menu — a switch is `/model <alias>` typed as a message),
/// and the rest are accepted but never advertised. `mode` is gone from the
/// vocabulary entirely: modes ride the ACP-native `session/set_mode` lane alone.
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

/// EXP-1224: how long a continuation opened ahead of its `init` (off the
/// background task's terminal frame) waits for the CLI to actually start it
/// before the turn is closed again. The CLI re-announces `init` within
/// milliseconds of the notification; this only bounds a CLI that decided not
/// to continue and never said `idle` either.
const ANTICIPATED_CONTINUATION_GRACE: Duration = Duration::from_secs(30);

/// The re-prompt that carries a plan into a fresh context after the user
/// picked one of the "clear context" plan options.
const PLAN_RESTART_PROMPT: &str = "Implement the following plan:";

/// EXP-905: when a turn re-reads the transcript for the conversation's name
/// besides its end — claude writes the `ai-title` line in the background soon
/// after the first prompt (EXP-1134: once asked, on a CLI that no longer names
/// SDK sessions itself), so a long first turn would otherwise stay "Chat"
/// until it finished. Only while no title is known yet.
const TITLE_EARLY_POLLS: [Duration; 2] = [Duration::from_secs(8), Duration::from_secs(30)];

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
                                        .config_options(session.config_options())
                                        .meta(session.native_id_meta()),
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
                            //
                            // EXP-784: the ACP id on the request is the
                            // host's stable handle; the transcript is named
                            // by claude's OWN id, which the host passes as a
                            // `_meta` hint off the run record. Without one
                            // (a pre-784 record) the two were the same uuid.
                            let hinted = request
                                .meta
                                .as_ref()
                                .and_then(|meta| meta.get(NATIVE_SESSION_META_KEY))
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            // No child has run yet, so the pin minted in
                            // `new` names nothing on disk: the hint wins,
                            // else the ACP id IS the transcript (pre-784).
                            let transcript = {
                                let mut state = session.lock();
                                let transcript =
                                    hinted.unwrap_or_else(|| request.session_id.0.to_string());
                                state.native_session_id = Some(transcript.clone());
                                transcript
                            };
                            session.replay_history(&spawned, &transcript);
                            // EXP-905: a LIVE resume keeps the name the
                            // conversation already has; a read-only replay
                            // writes no row, so it reads nothing. SCHEDULED,
                            // never called here: the first read of a resumed
                            // run's transcript can be megabytes, and this is
                            // the pump — `schedule_title_poll` puts it on the
                            // blocking thread, so the load responds now and
                            // the title follows.
                            if !session.spec.replay {
                                session.schedule_title_poll(&spawned, Duration::ZERO);
                            }
                            responder.respond(
                                LoadSessionResponse::new()
                                    .modes(session.mode_state())
                                    .config_options(session.config_options())
                                    .meta(session.native_id_meta()),
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
                            // EXP-784: our own ACP id names the native
                            // transcript behind it; any other id is taken as
                            // a claude session id verbatim.
                            let resume = session.resume_handle_for(&request.session_id);
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
                    async move |notification: CancelNotification, _cx| {
                        // EXP-784: absent = a Stop (the queued steers go
                        // too); the stall watchdog's interrupt says `false`.
                        let cancel_queued = notification
                            .meta
                            .as_ref()
                            .and_then(|meta| meta.get(CANCEL_QUEUED_META_KEY))
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        on_cancel.cancel(cancel_queued);
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
                    session.detach_live_usage();
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
    /// The ACP session id: the host's STABLE handle for this run. EXP-784
    /// decoupled it from claude's own session uuid (`State::native_session_id`,
    /// the `--session-id` pin / `--resume` target): a `/clear` gives the CLI a
    /// fresh transcript under a NEW uuid, and with the two ids being one
    /// string the run record's resume handle silently pointed at the dead
    /// conversation. Now the native id moves and is re-published
    /// (`NATIVE_SESSION_META_KEY`) while this one never does.
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
    /// EXP-909: the LOGIN this run spends (`coding::profile_id` of the
    /// launch's `account`, so `system` = the ambient one). Every live usage
    /// publish is keyed by it, so a run on a secondary account never moves
    /// the ambient login's numbers.
    account_profile: String,
    /// EXP-819: this session's slot in the machine's live usage registry
    /// (`coding::agent_usage::live`), held for the run like codex's. Released
    /// by [`ClaudeSession::detach_live_usage`] when the connection ends; its
    /// `Drop` is the backstop for every path that never gets there.
    /// EXP-909: `None` for a transcript REPLAY — reading history spends no
    /// tokens, and a viewer left open used to hold a phantom live session.
    live_usage: Mutex<Option<coding::agent_usage::live::Attached>>,
    state: Mutex<State>,
    /// EXP-905: the incremental reader of the conversation's name out of
    /// claude's own transcript. Its OWN lock: a poll does file I/O and must
    /// never hold `state` across it.
    title: Mutex<TitleTail>,
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
    /// EXP-866: `replay_history` is walking a transcript off disk. A
    /// rate-limit notice met THERE is history — the wall that ended the run
    /// this one continues (an account switch, a resume) — and must not be
    /// re-armed as a live wall on the continuation, which then showed
    /// "rate limited" until the next real assistant token cleared it.
    replaying_history: bool,
    /// One settle channel per in-flight `session/prompt`, oldest first: claude
    /// emits one `result` per turn, so the front of the queue owns the next.
    turns: VecDeque<flume::Sender<TurnOutcome>>,
    /// An outcome held back while background subagents of that turn are still
    /// live — settling at the `result` would strand their permission requests
    /// on an RPC nobody answers.
    deferred: Option<DeferredSettle>,
    /// EXP-1224: a turn the CLI started ON ITS OWN is running — after a
    /// `result` it re-announces `system/init` (same session id) and works
    /// on whenever a background task's notification is pending, with no
    /// `session/prompt` behind it. Its own `result` settles no prompt; it
    /// closes this turn (the `TURN_META_KEY` edge the mapper folds into the
    /// `turn` slot).
    continuation_running: bool,
    /// EXP-1224: the continuation's `result` came while background tasks of
    /// the run were still live — it ends when they are gone, by the same
    /// rule (and the same defer timer) a prompted turn's settle follows.
    continuation_end_pending: bool,
    /// EXP-1224: the continuation was opened AHEAD of its `init`, off the
    /// background task's terminal frame that will wake the model (that frame
    /// also releases a deferred settle, and the turn must not read ended in
    /// between). Cleared by the `init` that confirms it; a CLI that goes
    /// idle instead, or says nothing for [`ANTICIPATED_CONTINUATION_GRACE`],
    /// closes it.
    continuation_anticipated: bool,
    /// Bumped by every continuation start, so a backstop timer armed for one
    /// never closes a later one.
    continuation_seq: u64,
    /// Claude's OWN session uuid: minted here for a fresh run (the
    /// `--session-id` pin), the recorded handle for a resume, and re-read
    /// from every `system/init` (a `/clear` changes it, EXP-784).
    native_session_id: Option<String>,
    /// The native id last put on a response or notification `_meta`, so an
    /// init that reports the same one publishes nothing.
    published_native_id: Option<String>,
    /// Whether an init frame has been seen: the CLI re-announces one per
    /// turn under the SAME id, so a later init under a NEW id is a `/clear`.
    saw_init: bool,
    /// EXP-784: the rate-limit slot's adapter-side state.
    rate_limit: RateLimitState,
    /// EXP-784: message ids of `<synthetic>` messages seen at `message_start`,
    /// whose text deltas must not stream as narration.
    synthetic_messages: HashSet<String>,
    /// EXP-784: whether the interrupt a deferred cancel will deliver also
    /// drops the CLI's queued user messages.
    interrupt_cancel_queued: bool,
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
    /// EXP-1051: the `context_layout` frame has gone out for THIS
    /// conversation. One-shot: the layers are launch-time facts and the base
    /// is measured off the first request, so a per-turn re-publish would only
    /// redraw the same bar. A `/clear` re-arms it (a new conversation has a
    /// new prefix) — a compaction does not, since the slot already holds the
    /// frame and compaction changes the occupancy, not what the run started
    /// with.
    context_layout_published: bool,
    /// EXP-1051: the base a PREVIOUS run of this conversation measured,
    /// carried over `runs.json` by a native resume. Used only while the model
    /// is unchanged, and dropped by a `/clear`.
    carried_base: Option<coding::CarriedBase>,
    /// EXP-1051: this conversation was RELOADED by a native resume, so its
    /// first request carries the whole transcript, not a fresh prefix —
    /// nothing is measured until a `/clear` starts a new one; the bar draws
    /// the carried base (or none) instead.
    resumed_conversation: bool,
    /// EXP-1134: the conversation's name was asked for (or, on a resume,
    /// is already there) — [`ClaudeSession::request_title`] asks once.
    title_requested: bool,
    context_window: ContextWindow,
    compaction: Option<String>,
    tasks: HashMap<String, TaskEntry>,
    /// EXP-850 §2: the CLI's FULL current background-task list, as the last
    /// `background_tasks_changed` named it. Read by a `TaskOutput` row to
    /// label itself and published as the `background_tasks` slot.
    background_tasks: Vec<steer::BackgroundTask>,
    /// EXP-850 §3: one entry per `Workflow` run, in start order, keyed by the
    /// `Workflow` call's `tool_use_id` (= the card's wire id).
    workflows: Vec<WorkflowRun>,
    /// `task_id` -> workflow id, so a `task_updated` (which names only the
    /// task) finds its card.
    workflow_of_task: HashMap<String, String>,
    /// EXP-850 §2: `task_id` -> the `tool_use_id` that launched it. The
    /// `background_tasks_changed` frame names neither — only `task_started`
    /// does, and it arrives AFTER the list — so the strip's `toolId` is
    /// filled from here and the list re-published once it is known.
    task_tool_ids: HashMap<String, String>,
    /// EXP-850 §5: the turn's token estimate — claude's `thinking_tokens`
    /// deltas plus the highest `usage.output_tokens` each assistant message
    /// reported (the consolidated frame repeats the streamed one's count, so
    /// summing per id would double it). Reset at every prompt.
    turn_thinking_tokens: u64,
    turn_message_tokens: HashMap<String, u64>,
    /// EXP-853 rule 2: the last EXPLICIT mode change (a plan approval, a
    /// steered `set_mode`, the `EnterPlanMode` hook) and when it landed. An
    /// `init` that contradicts it inside [`MODE_ANNOUNCE_GRACE`] is the CLI
    /// re-announcing a mode it has not applied yet, and is dropped.
    explicit_mode: Option<(String, Instant)>,
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
    /// EXP-954: the newest `plans/*.md` the agent wrote — the plan an
    /// `ExitPlanMode` with no `plan` input is approving.
    plan_file: Option<PlanFile>,
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

/// EXP-784: what the adapter knows about the plan window, from the CLI's
/// `rate_limit_event`s and its synthetic "You've hit your…" notices.
#[derive(Default)]
struct RateLimitState {
    /// The last LIMITED status a `rate_limit_event` reported
    /// (`allowed_warning`, `rejected`); `None` after an `allowed`.
    status: Option<String>,
    /// Its `resetsAt`, already in unix ms.
    resets_at: Option<i64>,
    /// FEED-34: the contract window that `resets_at` belongs to
    /// (`RateLimitInfo::window`), kept across the notice so the row's
    /// `blocked.window` never pairs a `session` label with a weekly reset.
    window: Option<&'static str>,
    /// A synthetic limit notice is on the slot. Cleared by the next REAL
    /// assistant activity (text OR a tool call — EXP-831: a run that comes
    /// back with tool calls only kept the wall until it next narrated), and
    /// by a per-turn `allowed` event only once `resets_at` has passed: the
    /// event fires at the request, BEFORE the 429 that produces the notice,
    /// so honouring it inside the window would flicker the slot
    /// clear-then-limited every turn. While the notice stands, `status`,
    /// `resets_at` and `window` stay put for that later check (and for the
    /// CLI repeating the notice).
    notice_active: bool,
    /// The slot as last published, so the CLI repeating its notice on
    /// every request of a turn (measured: N identical frames) publishes
    /// once. The mapper dedupes too; this keeps the ACP stream honest.
    published: Option<Value>,
    /// EXP-819: the usage windows the run's `rate_limit_event`s have named
    /// so far, latest per key — a frame that carries only the limiting
    /// window must not un-publish the other one. Outlives the slot: a
    /// cleared notice keeps them.
    windows: Vec<coding::agent_usage::UsageWindow>,
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

/// EXP-850 §3 review: keep at most [`steer::journal::JOURNAL_WORKFLOW_CAP`]
/// cards, oldest first, exactly like the journal and the relay room — a run
/// that starts workflows all day otherwise grows three maps for the life of
/// the process. Dropping a card drops its id maps with it: the `task_id`
/// mappings that pointed at it and the launching tool ids they named.
fn evict_workflows(state: &mut State) {
    while state.workflows.len() > steer::journal::JOURNAL_WORKFLOW_CAP {
        let dropped = state.workflows.remove(0);
        let orphans: Vec<String> = state
            .workflow_of_task
            .iter()
            .filter(|(_, id)| **id == dropped.id)
            .map(|(task_id, _)| task_id.clone())
            .collect();
        for task_id in orphans {
            state.workflow_of_task.remove(&task_id);
            state.task_tool_ids.remove(&task_id);
        }
        state.task_tool_ids.retain(|_, tool_id| *tool_id != dropped.id);
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

/// The live tasks [`ClaudeAgent::expire_tasks`] retires: past
/// [`TASK_MAX_LIFETIME`] AND absent from the CLI's latest `background_tasks`
/// list (EXP-927 — a listed task is running, however old).
fn expired_tasks(state: &State) -> Vec<String> {
    state
        .tasks
        .iter()
        .filter(|(_, task)| task.live && task.started_at.elapsed() >= TASK_MAX_LIFETIME)
        .filter(|(task_id, _)| !state.background_tasks.iter().any(|listed| listed.id == **task_id))
        .map(|(task_id, _)| task_id.clone())
        .collect()
}

/// How long the defer timer sleeps: until the YOUNGEST blocking task — the
/// last one to reach [`TASK_MAX_LIFETIME`] — has expired, plus a second so
/// the sleep lands strictly past the expiry it is meant to observe. With no
/// blocking task left (settled between the check and the arm) it still
/// sleeps a full lifetime rather than spinning.
fn defer_timer_wait(state: &State) -> Duration {
    blocking_tasks(state)
        .map(|(_, task)| TASK_MAX_LIFETIME.saturating_sub(task.started_at.elapsed()))
        .max()
        .unwrap_or(TASK_MAX_LIFETIME)
        + Duration::from_secs(1)
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
    /// EXP-847: the spawning `Agent` call's own `description` (its `name` as a
    /// fallback) — read off the tool table at `task_started` and kept HERE,
    /// because the tool RESULT takes the table entry away and the completed
    /// edge must still name the subagent.
    title: Option<String>,
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
    /// EXP-850 §4: the workflow card this task's agent belongs to, when its
    /// id matched a workflow agent's `agentId`. Stamped on every edge so the
    /// completed one nests under the card too.
    workflow_id: Option<String>,
    /// EXP-850 §2: a task that is NOT an agent (`task_type` `local_bash`: a
    /// backgrounded Bash or a Monitor, the main thread's or a subagent's) is
    /// tracked for the turn-settle deferral only. It rides the
    /// `background_tasks` strip and never publishes a subagent edge; live
    /// runs showed every such task as a loose "agent · done" row and even a
    /// subagent tab.
    silent: bool,
}

/// EXP-850 §3: one `Workflow` run as the adapter folds it — the progress
/// array is latest-per-`type:index`, so phases and agents are maps keyed by
/// the CLI's own index.
struct WorkflowRun {
    /// The `Workflow` tool call's `tool_use_id` — the card's wire id.
    id: String,
    name: String,
    description: Option<String>,
    status: steer::WorkflowStatus,
    phases: BTreeMap<u32, steer::WorkflowPhase>,
    agents: BTreeMap<u32, steer::WorkflowAgent>,
    summary: Option<String>,
    /// When the card was last published — the §3 throttle's clock.
    published_at: Option<Instant>,
    /// EXP-1225: what each agent's synthesized lane has published so far,
    /// keyed like `agents`. Kept HERE, never in `State::tasks`: a workflow
    /// agent is not a task of the run (the workflow is), so it never holds
    /// back a settle.
    lanes: BTreeMap<u32, AgentLane>,
}

/// EXP-1225: a workflow agent's subagent lane, synthesized from its progress
/// entries. The CLI never streams a workflow agent's own frames (no
/// `task_started`, no `parent_tool_use_id`) — the progress array is all there
/// is, so the lane is its edges, one settled row per distinct
/// `lastToolName`/`lastToolSummary`, and the result preview as narration.
#[derive(Default)]
struct AgentLane {
    /// The `started` edge went out (under the agent's `agentId`).
    started: bool,
    /// The last `(lastToolName, lastToolSummary)` a row was published for.
    last_tool: Option<(String, String)>,
    /// How many rows the lane has published — the synthesized ids' suffix.
    rows: u32,
    /// The terminal edge went out.
    finished: bool,
}

/// EXP-1225: one step of a workflow agent's synthesized lane, collected under
/// the state lock and published after it.
enum LaneStep {
    Started { id: String, title: String, workflow_id: String },
    Tool { id: String, lane: String, name: String, summary: String },
    Narration { lane: String, text: String },
    Finished {
        id: String,
        title: String,
        workflow_id: String,
        status: &'static str,
        tool_calls: Option<u32>,
    },
}

/// EXP-1225: the lane steps one progress entry adds, given what the lane has
/// already published. An agent gets a lane once it RUNS under an `agentId`
/// (the id every client keys its tab on, and the one EXP-856's duplicate
/// check matches); a queued agent has none yet.
fn lane_steps(
    workflow_id: &str,
    agent: &steer::WorkflowAgent,
    lane: &mut AgentLane,
) -> Vec<LaneStep> {
    let mut steps = Vec::new();
    let Some(id) = agent.agent_id.clone() else { return steps };
    if agent.state == steer::WorkflowAgentState::Queued || lane.finished {
        return steps;
    }
    let title = if agent.label.trim().is_empty() { id.clone() } else { agent.label.clone() };
    if !lane.started {
        lane.started = true;
        steps.push(LaneStep::Started {
            id: id.clone(),
            title: title.clone(),
            workflow_id: workflow_id.to_string(),
        });
    }
    if let Some(name) = agent.last_tool.clone().filter(|name| !name.trim().is_empty()) {
        let summary = agent.last_tool_summary.clone().unwrap_or_default();
        let key = (name.clone(), summary.clone());
        if lane.last_tool.as_ref() != Some(&key) {
            lane.last_tool = Some(key);
            lane.rows += 1;
            steps.push(LaneStep::Tool {
                id: format!("{id}:tool:{}", lane.rows),
                lane: id.clone(),
                name,
                summary,
            });
        }
    }
    let (status, text) = match agent.state {
        steer::WorkflowAgentState::Done => ("completed", agent.result_preview.clone()),
        steer::WorkflowAgentState::Error => ("failed", agent.error.clone()),
        _ => return steps,
    };
    lane.finished = true;
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        steps.push(LaneStep::Narration { lane: id.clone(), text });
    }
    steps.push(LaneStep::Finished {
        id,
        title,
        workflow_id: workflow_id.to_string(),
        status,
        tool_calls: agent.tool_calls,
    });
    steps
}

/// EXP-1225: the workflow itself ended. A lane its progress never finished
/// (a stopped or killed workflow freezes its agents mid-run) closes with it,
/// or every client keeps that agent's tab spinning for the rest of the run.
fn close_lanes(run: &mut WorkflowRun) -> Vec<LaneStep> {
    let status = match run.status {
        steer::WorkflowStatus::Completed => "completed",
        _ => "failed",
    };
    let mut steps = Vec::new();
    for (index, lane) in run.lanes.iter_mut() {
        if !lane.started || lane.finished {
            continue;
        }
        let Some(agent) = run.agents.get(index) else { continue };
        let Some(id) = agent.agent_id.clone() else { continue };
        lane.finished = true;
        steps.push(LaneStep::Finished {
            title: if agent.label.trim().is_empty() { id.clone() } else { agent.label.clone() },
            id,
            workflow_id: run.id.clone(),
            status,
            tool_calls: agent.tool_calls,
        });
    }
    steps
}

/// EXP-1225: the tool input a workflow agent's `lastToolName` +
/// `lastToolSummary` stand for, so [`tool_info`] titles and kinds the
/// synthesized row exactly as it would the agent's real call. The summary
/// is the CLI's one-line description of the call: the path for the file
/// tools, the pattern for a search, the command for `Bash`.
fn lane_tool_input(name: &str, summary: &str) -> Value {
    match name {
        "Bash" => json!({ "command": summary, "description": summary }),
        "Read" | "Write" | "Edit" | "NotebookEdit" => json!({ "file_path": summary }),
        "Glob" | "Grep" => json!({ "pattern": summary }),
        "WebFetch" => json!({ "url": summary }),
        "WebSearch" => json!({ "query": summary }),
        "Agent" | "Task" => json!({ "description": summary }),
        "Skill" => json!({ "skill": summary }),
        _ => json!({ "description": summary }),
    }
}

impl WorkflowRun {
    /// The wire payload: the WHOLE state, every time (§3 is latest-wins).
    fn state(&self) -> steer::WorkflowState {
        steer::WorkflowState {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            status: self.status,
            phases: self.phases.values().cloned().collect(),
            agents: self.agents.values().cloned().collect(),
            summary: self.summary.clone(),
            at: None,
        }
    }

    /// The agent with this `agentId`, if the card holds one (EXP-856: a
    /// `task_started` whose `task_id` matches names a copy of it).
    fn agent_by_id(&self, agent_id: &str) -> Option<&steer::WorkflowAgent> {
        self.agents
            .values()
            .find(|agent| agent.agent_id.as_deref() == Some(agent_id))
    }
}

/// EXP-850 §3: the card's NAME. A `task_started` without a `workflow_name`
/// (a script the CLI never named) used to open a card called `""`, which
/// web, iOS and Android drop entirely — so the run showed no card at all.
/// The description is the next best thing the frame carries, and the word
/// `workflow` is the floor.
fn workflow_card_name(workflow_name: Option<&str>, description: Option<&str>) -> String {
    for candidate in [workflow_name, description] {
        let Some(candidate) = candidate else { continue };
        if !candidate.trim().is_empty() {
            return candidate.to_string();
        }
    }
    "workflow".to_string()
}

/// EXP-850 §3: at most one card frame per workflow per this long while it is
/// running — every agent state CHANGE and the terminal status publish
/// immediately regardless.
const WORKFLOW_PUBLISH_INTERVAL: Duration = Duration::from_millis(1000);

/// EXP-853 rule 2: how long an EXPLICIT mode change outranks the mode the CLI
/// re-announces on every turn `init`. Past it the CLI's word is the truth.
const MODE_ANNOUNCE_GRACE: Duration = Duration::from_secs(10);

/// The RUN-scoped override of [`MODE_ANNOUNCE_GRACE`], in milliseconds. Read
/// off this session's own spawn env — never the process environment — so the
/// `plan-stuck` fixture can exercise the past-the-window half of rule 2
/// without a ten-second sleep, and so one test can never change another's
/// clock. Absent (every real launch) = the constant above.
const MODE_ANNOUNCE_GRACE_ENV: &str = "EXP_MODE_ANNOUNCE_GRACE_MS";

struct PlanTask {
    subject: String,
    status: String,
    active_form: Option<String>,
}

struct PlanRestart {
    plan: String,
    mode: String,
}

/// EXP-954: the plan file the CLI wrote last. The current claude writes its
/// plan to `{CLAUDE_CONFIG_DIR}/plans/<slug>.md` with an ordinary `Write` and
/// then raises `ExitPlanMode` with an EMPTY input, so the plan markdown the
/// approval card shows has to come from here.
struct PlanFile {
    path: PathBuf,
    /// The `content` the `Write` call carried, when the stream held it (a
    /// resumed conversation replays no tool input, so the file is read from
    /// disk instead).
    text: Option<String>,
}

impl ClaudeSession {
    fn new(spec: AdapterSpec) -> ClaudeSession {
        // EXP-784: TWO uuids. The ACP session id is the host's handle (a
        // recorded one on `session/load`, else fresh); claude's own is the
        // `--session-id` pin, a uuid the CLI has never seen, or the recorded
        // native handle on a resume. On `Acp(id)` the native one arrives
        // later as the load request's hint (a pre-784 record has none, and
        // then the two are the same string, as they always were).
        let (session_id, native_session_id) = match &spec.resume {
            Some(ResumeHandle::Acp(id)) => (id.clone(), None),
            Some(ResumeHandle::Native(id)) => (uuid::Uuid::new_v4().to_string(), Some(id.clone())),
            _ => (uuid::Uuid::new_v4().to_string(), Some(uuid::Uuid::new_v4().to_string())),
        };
        let options = &spec.options;
        let state = State {
            native_session_id,
            model: options.model.trim().to_string(),
            effort: (!options.effort.trim().is_empty()).then(|| options.effort.trim().to_string()),
            ultracode: options.ultracode,
            mode: if options.plan_mode {
                "plan".to_string()
            } else {
                "bypassPermissions".to_string()
            },
            // EXP-1051: the launcher only fills this on a NATIVE resume — a
            // fresh run has no conversation to carry a base from.
            carried_base: spec.context_layers.carried_base.clone(),
            resumed_conversation: spec.resume.is_some(),
            title_requested: spec.resume.is_some() || !spec.name_conversation,
            ..State::default()
        };
        let account_profile = coding::profile_id(options.account.as_deref());
        // EXP-909: a replay is a READ of a transcript — it spends nothing, so
        // it attaches nothing.
        let live_usage = (!spec.replay)
            .then(|| coding::agent_usage::live::attach(coding::CodingAgent::Claude, &account_profile));
        let (gone_gate, gone) = flume::bounded(0);
        ClaudeSession {
            spec,
            session_id: SessionId::new(session_id),
            gone,
            gone_gate: Mutex::new(Some(gone_gate)),
            start_gate: tokio::sync::Mutex::new(()),
            account_profile,
            live_usage: Mutex::new(live_usage),
            state: Mutex::new(state),
            title: Mutex::new(TitleTail::default()),
        }
    }

    /// EXP-819: the session is over, so it stops answering for this machine's
    /// usage numbers. Idempotent; the last published windows stay in the
    /// registry, aged out by their own stamp.
    fn detach_live_usage(&self) {
        let attached = match self.live_usage.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(attached);
    }

    fn cwd(&self) -> &Path {
        &self.spec.cwd
    }

    /// EXP-784: the `_meta` naming claude's own session id, for the
    /// `session/new` / `session/load` response — what the run record's
    /// `agent_native_session_id` is written from. Empty when nothing is
    /// known yet (an `Acp` load with no hint, before the first init).
    fn native_id_meta(&self) -> Map<String, Value> {
        let mut state = self.lock();
        let mut meta = Map::new();
        if let Some(native) = state.native_session_id.clone() {
            state.published_native_id = Some(native.clone());
            meta.insert(NATIVE_SESSION_META_KEY.to_string(), json!(native));
        }
        meta
    }

    /// EXP-784: the claude session id a `session/resume` for `session_id`
    /// reopens — the native one behind OUR ACP id, any other id verbatim.
    fn resume_handle_for(&self, session_id: &SessionId) -> String {
        if *session_id == self.session_id {
            self.lock()
                .native_session_id
                .clone()
                .unwrap_or_else(|| self.session_id.0.to_string())
        } else {
            session_id.0.to_string()
        }
    }

    /// EXP-784: the handle a lazy spawn (`prompt` on a loaded session)
    /// resumes with: the native id, never the public one.
    fn lazy_resume_handle(&self) -> Option<String> {
        let state = self.lock();
        state.child.is_none().then(|| {
            state
                .native_session_id
                .clone()
                .unwrap_or_else(|| self.session_id.0.to_string())
        })
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
        let native = state.native_session_id.clone();
        drop(state);

        let inline_mcp;
        let mcp = match &self.spec.mcp {
            coding::AgentMcp::ClaudeInline { url, session_id } => {
                inline_mcp =
                    wire::inline_mcp_config(url, session_id.as_deref(), &self.spec.servers);
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
        // so pinning it as a fresh `--session-id` would be refused. EXP-784:
        // the conversation is the NATIVE id (the load hint, or the recorded
        // handle); the ACP id is only ever the transcript name on a pre-784
        // record, where the two were one uuid.
        let recorded = match &self.spec.resume {
            Some(ResumeHandle::Acp(id)) | Some(ResumeHandle::Native(id)) => {
                Some(native.clone().unwrap_or_else(|| id.clone()))
            }
            None => None,
        };
        let resume = resume.map(str::to_string).or(recorded);
        // The pin: claude's own uuid, minted in `new` and NEVER the ACP id.
        let pin = native.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        {
            // Whatever the child is told to be is what it is until an init
            // says otherwise: a resume continues under the resumed id.
            let mut state = self.lock();
            state.native_session_id = Some(resume.clone().unwrap_or_else(|| pin.clone()));
        }
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
            session_id: Some(pin.as_str()),
            resume: resume.as_deref(),
            fork_session: false,
            mcp_config: mcp,
            strict_mcp_config: mcp.is_some(),
            settings: self.spec.reaper_settings_path.as_deref(),
            add_dirs: &[],
            disallowed_tools: disallowed,
            // EXP-763: the run playbook (EXP-1025: + the team prompt), on
            // every start and resume.
            append_system_prompt: Some(self.spec.system_append.as_str()),
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

    /// `cancel_queued` (EXP-784): a user's Stop halts the whole session —
    /// the running turn AND the user messages the CLI holds queued behind
    /// it (`true`); the stall watchdog's interrupt unwinds the wedged turn
    /// alone (`false`) so those queued steers flow, which is the promise
    /// `stall.rs` makes.
    fn cancel(&self, cancel_queued: bool) {
        let mut state = self.lock();
        state.cancelled = true;
        state.cancel_epoch = state.cancel_epoch.wrapping_add(1);
        state.interrupt_cancel_queued = cancel_queued;
        let closed = state.closed;
        drop(state);
        if closed {
            // A finished query rejects `interrupt` — the upstream's
            // `queryClosed` guard, ported.
            return;
        }
        if let Err(error) = self.send(wire::control_request(
            &wire::new_request_id(),
            wire::interrupt(cancel_queued),
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
        let cancel_queued = state.interrupt_cancel_queued;
        drop(state);
        if !deliver {
            return;
        }
        if let Err(error) = self.send(wire::control_request(
            &wire::new_request_id(),
            wire::interrupt(cancel_queued),
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
        // EXP-784: the handle is claude's NATIVE id, never the ACP one.
        let resume = self.lazy_resume_handle();
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
            // EXP-850 §5: the working caption counts THIS turn's output.
            state.turn_thinking_tokens = 0;
            state.turn_message_tokens.clear();
            state.turns.push_back(tx);
        }
        self.send(claude_user_message(&request.prompt, &text))?;
        // EXP-758: a cancel that raced this turn's lazy spawn had no child to
        // reach. It does now, and the turn it meant to stop is running.
        self.deliver_pending_interrupt();
        // EXP-905: a resumed conversation is already named; a fresh one gets
        // its name shortly after this first prompt (EXP-1134: by asking).
        self.request_title(cx, &text);
        self.schedule_title_poll(cx, Duration::ZERO);
        if !self.title_known() {
            for delay in TITLE_EARLY_POLLS {
                self.schedule_title_poll(cx, delay);
            }
        }
        let outcome = rx.recv_async().await.unwrap_or(TurnOutcome::EndTurn);
        match outcome {
            TurnOutcome::EndTurn => Ok(PromptResponse::new(StopReason::EndTurn)),
            TurnOutcome::MaxTokens => Ok(PromptResponse::new(StopReason::MaxTokens)),
            TurnOutcome::MaxTurnRequests => Ok(PromptResponse::new(StopReason::MaxTurnRequests)),
            TurnOutcome::Refusal => Ok(PromptResponse::new(StopReason::Refusal)),
            TurnOutcome::Cancelled => {
                // FEED-44: an interrupt kills the turn's background agents
                // with it, and the CLI need not re-list. Nothing may hold
                // the run busy past a Stop; a later list frame that still
                // names one restores it.
                self.retire_background_agents(cx, None);
                Ok(PromptResponse::new(StopReason::Cancelled))
            }
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
        // EXP-1224: a continuation whose `result` waited on the same tasks.
        if state.continuation_end_pending {
            self.end_continuation(cx, state);
        }
    }

    /// EXP-1224: the CLI started (or, `anticipated`, is about to start) a
    /// turn nobody prompted. Publishes the agent's `started` edge once per
    /// continuation; a second start inside one (the `init` confirming an
    /// anticipated one) only clears the anticipation.
    fn begin_continuation(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        state: &mut State,
        anticipated: bool,
    ) {
        if state.continuation_running {
            if !anticipated {
                state.continuation_anticipated = false;
            }
            return;
        }
        state.continuation_running = true;
        // A continuation that starts while the previous one waits on its
        // tasks supersedes that end: the agent is working again.
        state.continuation_end_pending = false;
        state.continuation_anticipated = anticipated;
        state.continuation_seq = state.continuation_seq.wrapping_add(1);
        // EXP-850 §5: a NEW turn counts its own output. Kept while the
        // prompt's settle is still deferred: the mapper's slot never closed,
        // so this is the same working stretch to everyone watching.
        if state.deferred.is_none() {
            state.turn_thinking_tokens = 0;
            state.turn_message_tokens.clear();
        }
        self.publish_turn(cx, steer::TurnState::Started);
        if anticipated {
            let seq = state.continuation_seq;
            let session = self.clone();
            let out = cx.clone();
            let _ = cx.spawn(async move {
                tokio::time::sleep(ANTICIPATED_CONTINUATION_GRACE).await;
                let mut state = session.lock();
                if state.continuation_running
                    && state.continuation_anticipated
                    && state.continuation_seq == seq
                {
                    log::debug!("engine: claude never continued after its notification");
                    session.end_continuation(&out, &mut state);
                }
                Ok(())
            });
        }
    }

    /// EXP-1224: the CLI's own turn is over — the agent's `ended` edge.
    fn end_continuation(&self, cx: &ConnectionTo<Client>, state: &mut State) {
        state.continuation_running = false;
        state.continuation_end_pending = false;
        state.continuation_anticipated = false;
        self.publish_turn(cx, steer::TurnState::Ended);
    }

    /// EXP-1224: the agent's own turn edge, on the token count's carrier.
    fn publish_turn(&self, cx: &ConnectionTo<Client>, turn: steer::TurnState) {
        let mut meta = Map::new();
        meta.insert(TURN_META_KEY.to_string(), json!({ "state": turn.id() }));
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// Retire every live task past [`TASK_MAX_LIFETIME`], publishing a
    /// `failed` edge for each: the client's subagent card stops spinning and
    /// the mapper drops its per-subagent bookkeeping, which otherwise only
    /// ever clears on a terminal edge the CLI may never send.
    ///
    /// EXP-927: never a task the CLI's latest `background_tasks` list still
    /// NAMES. The lifetime exists so a dropped notification cannot defer a
    /// settle forever ([`blocking_tasks`] ages a task out on its own); it says
    /// nothing about whether the task runs. A background lane that works for
    /// longer than the lifetime used to get this `failed` edge mid-run, which
    /// closed its conversation tab on every client while the strip went on
    /// listing it. Such a task is retired the moment the list drops it.
    fn expire_tasks(&self, cx: &ConnectionTo<Client>, state: &mut State) {
        for task_id in expired_tasks(state) {
            let Some(task) = state.tasks.get_mut(&task_id) else { continue };
            task.live = false;
            if task.last_status.as_deref() == Some("failed") {
                continue;
            }
            task.last_status = Some("failed".to_string());
            let tool_use_id = task.tool_use_id.clone();
            let subagent_type = task.subagent_type.clone();
            let title = task.title.clone();
            let workflow_id = task.workflow_id.clone();
            log::warn!("engine: claude task {task_id} never reported back; retiring it");
            self.publish_subagent_edge(
                cx,
                tool_use_id.as_deref().unwrap_or(&task_id),
                subagent_type.as_deref(),
                "failed",
                title.as_deref(),
                workflow_id.as_deref(),
            );
        }
    }

    /// A deferral must not be able to outlive [`TASK_MAX_LIFETIME`] in total
    /// silence: nothing else wakes `settle_deferred` when the CLI simply stops
    /// sending frames for the task it is waiting on.
    ///
    /// Lock discipline: the timer task takes the state lock exactly as a
    /// frame handler would, and everything it calls under it
    /// (`expire_tasks` → `publish_subagent` → `notify_meta`, `settle_turns`)
    /// only sends on channels — none of it re-enters the lock.
    fn arm_defer_timer(self: &Arc<Self>, cx: &ConnectionTo<Client>, state: &mut State) {
        if state.defer_timer_armed {
            return;
        }
        state.defer_timer_armed = true;
        let wait = defer_timer_wait(state);
        let session = self.clone();
        let out = cx.clone();
        let _ = cx.spawn(async move {
            tokio::time::sleep(wait).await;
            let mut state = session.lock();
            state.defer_timer_armed = false;
            session.settle_deferred(&out, &mut state);
            // EXP-795: this timer was sized for the tasks blocking when it was
            // armed. A deferral that survived it is held by a YOUNGER task (a
            // later turn's, or one started after the arm) — without a fresh
            // timer sized for that one, it would be the silent wedge again.
            if state.deferred.is_some() || state.continuation_end_pending {
                session.arm_defer_timer(&out, &mut state);
            }
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
        // EXP-853 rule 2: an explicit switch outranks the CLI's next init
        // announcement for `MODE_ANNOUNCE_GRACE`.
        self.note_explicit_mode(&mode);
        self.lock().mode = mode.clone();
        self.notify(
            cx,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(mode))),
        );
        self.publish_config(cx);
        Ok(())
    }

    /// EXP-877: exactly ONE option, the `model` VALUE — no menu values, so no
    /// client draws a picker off the wire. Effort / fast / agent stay gone
    /// (EXP-772) and every option stays launch-time (EXP-790); the model is
    /// here only because the CLI's own `/model` moves it mid-run and a viewer
    /// otherwise never learns which model it is talking to.
    ///
    /// The value is the ALIAS the composer offers ([`coding::claude_model_alias`]),
    /// falling back to the raw resolved id when the CLI reports a tier this
    /// build does not know. `set_config` still ACCEPTS the ids an older
    /// publisher may send.
    fn config_options(&self) -> Vec<SessionConfigOption> {
        let model = self.lock().model.clone();
        let current = coding::claude_model_alias(&model)
            .map(str::to_string)
            .unwrap_or(model);
        vec![SessionConfigOption::select(
            CONFIG_MODEL,
            "Model",
            current,
            Vec::<SessionConfigSelectOption>::new(),
        )
        .category(SessionConfigOptionCategory::Model)]
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

    /// One subagent lifecycle edge, under the id its whole life publishes
    /// under (EXP-850 §4: a workflow agent's id is its `agentId`, so the
    /// duplicate warning and the copy's own edges share one identity).
    fn publish_subagent_edge(
        &self,
        cx: &ConnectionTo<Client>,
        id: &str,
        agent_type: Option<&str>,
        status: &str,
        title: Option<&str>,
        workflow_id: Option<&str>,
    ) {
        self.publish_subagent_edge_counted(cx, id, agent_type, status, title, workflow_id, None);
    }

    /// [`Self::publish_subagent_edge`] with the adapter's own tool-call count
    /// (EXP-1225: a workflow agent's `toolCalls`, which the mapper cannot
    /// count from rows it never saw).
    #[allow(clippy::too_many_arguments)]
    fn publish_subagent_edge_counted(
        &self,
        cx: &ConnectionTo<Client>,
        id: &str,
        agent_type: Option<&str>,
        status: &str,
        title: Option<&str>,
        workflow_id: Option<&str>,
        tool_calls: Option<u32>,
    ) {
        // The edge rides a no-op patch of the tool call that spawned the
        // subagent, so a client that ignores the meta sees nothing at all.
        let mut meta = Map::new();
        let mut edge = Map::new();
        edge.insert("id".to_string(), json!(id));
        edge.insert("agentType".to_string(), json!(agent_type));
        edge.insert("status".to_string(), json!(status));
        // EXP-847: omitted rather than null when the call named nothing.
        if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
            edge.insert("title".to_string(), json!(title));
        }
        if let Some(workflow_id) = workflow_id.filter(|id| !id.is_empty()) {
            edge.insert("workflowId".to_string(), json!(workflow_id));
        }
        if let Some(tool_calls) = tool_calls {
            edge.insert("toolCalls".to_string(), json!(tool_calls));
        }
        meta.insert(SUBAGENT_META_KEY.to_string(), Value::Object(edge));
        self.notify_meta(
            cx,
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
                ToolCallUpdateFields::new(),
            )),
            meta,
        );
    }

    /// EXP-1225: one step of a workflow agent's synthesized lane. The rows
    /// ride the SAME plumbing a streamed subagent's do (the lane's id as the
    /// `subagentId` meta), so every client files them under the agent's tab.
    fn publish_lane_step(&self, cx: &ConnectionTo<Client>, step: LaneStep) {
        match step {
            LaneStep::Started { id, title, workflow_id } => self.publish_subagent_edge(
                cx,
                &id,
                Some(GENERIC_AGENT_TYPE),
                "started",
                Some(&title),
                Some(&workflow_id),
            ),
            LaneStep::Tool { id, lane, name, summary } => {
                let input = lane_tool_input(&name, &summary);
                let mut info = tool_info(&name, &input, self.cwd());
                // As for a streamed call (EXP-1202): an Execute row's content
                // would read as command output.
                if info.kind == ToolKind::Execute {
                    info.content.clear();
                }
                let parent = Some(lane);
                self.emit_chunk(
                    cx,
                    SessionUpdate::ToolCall(
                        ToolCall::new(ToolCallId::new(id.clone()), info.title)
                            .kind(info.kind)
                            .status(ToolCallStatus::InProgress)
                            .content(info.content)
                            .locations(info.locations)
                            .raw_input(input),
                    ),
                    &parent,
                );
                // The progress entry only ever names a call that already
                // ran: the row is settled at once, with no output.
                self.emit_chunk(
                    cx,
                    SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                        ToolCallId::new(id),
                        ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
                    )),
                    &parent,
                );
            }
            LaneStep::Narration { lane, text } => {
                let message_id = format!("{lane}:result");
                let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                    .message_id(Some(MessageId::new(message_id)));
                self.emit_chunk(cx, SessionUpdate::AgentMessageChunk(chunk), &Some(lane));
            }
            LaneStep::Finished { id, title, workflow_id, status, tool_calls } => self
                .publish_subagent_edge_counted(
                    cx,
                    &id,
                    Some(GENERIC_AGENT_TYPE),
                    status,
                    Some(&title),
                    Some(&workflow_id),
                    tool_calls,
                ),
        }
    }

    /// EXP-856: a second copy of an id that is STILL running. The detail text
    /// is the contract's own sentence — every client renders it verbatim in an
    /// amber warning row, and the desktop raises an OS notification off it.
    fn publish_duplicate(
        &self,
        cx: &ConnectionTo<Client>,
        id: &str,
        agent_type: Option<&str>,
        title: Option<&str>,
        workflow_id: Option<&str>,
        label: &str,
    ) {
        let detail = duplicate_agent_detail(label);
        log::warn!("engine: claude started a second copy of live agent {id} ({label})");
        let mut meta = Map::new();
        let mut edge = Map::new();
        edge.insert("id".to_string(), json!(id));
        edge.insert("agentType".to_string(), json!(agent_type));
        edge.insert("status".to_string(), json!("duplicate"));
        edge.insert("detail".to_string(), json!(detail));
        if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
            edge.insert("title".to_string(), json!(title));
        }
        if let Some(workflow_id) = workflow_id.filter(|id| !id.is_empty()) {
            edge.insert("workflowId".to_string(), json!(workflow_id));
        }
        meta.insert(SUBAGENT_META_KEY.to_string(), Value::Object(edge));
        self.notify_meta(
            cx,
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
                ToolCallUpdateFields::new(),
            )),
            meta,
        );
    }

    /// FEED-44: drop a finished AGENT from the strip without waiting for the
    /// CLI to re-list. The host reads `agent_busy` off the latest list's
    /// agent entries, and the CLI does not always send a
    /// `background_tasks_changed` after the agent's own completion frame
    /// (a Stop killing background agents, a `/clear`, a lost frame): the
    /// run then showed the working dot on an idle session until Stop.
    /// `task_id` = `None` retires EVERY listed agent (a cancelled turn).
    /// Only an agent-kind entry moves: a shell task's row is the CLI's to
    /// keep, and only agents hold the run busy. Republished through the
    /// same slot a list frame uses, so the host's flag recomputes off it.
    fn retire_background_agents(&self, cx: &ConnectionTo<Client>, task_id: Option<&str>) {
        let changed = {
            let mut state = self.lock();
            let before = state.background_tasks.len();
            state.background_tasks.retain(|listed| {
                listed.kind != steer::BackgroundTaskKind::Agent
                    || task_id.is_some_and(|task_id| listed.id != task_id)
            });
            state.background_tasks.len() != before
        };
        if changed {
            self.publish_background_tasks(cx);
        }
    }

    /// EXP-850 §2: the background-task slot, as `_meta` on a no-op
    /// `session_info_update` (the rate-limit slot's carrier). The FULL list
    /// every time — an empty one closes the strip.
    fn publish_background_tasks(&self, cx: &ConnectionTo<Client>) {
        let tasks = self.lock().background_tasks.clone();
        let Ok(tasks) = serde_json::to_value(tasks) else { return };
        let mut meta = Map::new();
        meta.insert(BACKGROUND_TASKS_META_KEY.to_string(), tasks);
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// EXP-850 §1: the human label of a WAIT row — for `TaskOutput` the
    /// description of the task it names, off the LATEST background-task list
    /// (its id as the fallback); for `Monitor` its own `description` input.
    fn wait_detail(&self, name: &str, input: &Value) -> Option<String> {
        let string = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        if name == "Monitor" {
            return string("description");
        }
        let task_id = string("task_id").or_else(|| string("taskId"))?;
        let state = self.lock();
        let clean = |text: &str| Some(text.trim().to_string()).filter(|text| !text.is_empty());
        let described = state
            .background_tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| clean(&task.description))
            // The list frame may already have dropped a finished task (the
            // wait for a workflow that just completed): fall back to the
            // workflow's description or name, then to the task's own title.
            .or_else(|| {
                let id = state.workflow_of_task.get(&task_id)?;
                let run = state.workflows.iter().find(|run| run.id == *id)?;
                run.description
                    .as_deref()
                    .and_then(clean)
                    .or_else(|| clean(&format!("Workflow {}", run.name)))
            })
            .or_else(|| state.tasks.get(&task_id).and_then(|task| task.title.as_deref().and_then(clean)));
        Some(described.unwrap_or(task_id))
    }

    /// EXP-850 §3: ONE workflow card. `force` publishes regardless of the
    /// throttle — every agent state change and the terminal status do.
    fn publish_workflow(&self, cx: &ConnectionTo<Client>, id: &str, force: bool) {
        let state = {
            let mut session = self.lock();
            let Some(run) = session.workflows.iter_mut().find(|run| run.id == id) else {
                return;
            };
            let due = force
                || run
                    .published_at
                    .is_none_or(|at| at.elapsed() >= WORKFLOW_PUBLISH_INTERVAL);
            if !due {
                return;
            }
            run.published_at = Some(Instant::now());
            run.state()
        };
        let Ok(state) = serde_json::to_value(state) else { return };
        let mut meta = Map::new();
        meta.insert(WORKFLOW_META_KEY.to_string(), state);
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// EXP-850 §5: the turn's token estimate. The MAPPER owns the tick rate
    /// (the slot is latest-wins, so a tick costs no feed growth); the adapter
    /// simply reports what it measured.
    fn publish_turn_tokens(&self, cx: &ConnectionTo<Client>) {
        let tokens = {
            let state = self.lock();
            state.turn_thinking_tokens + state.turn_message_tokens.values().sum::<u64>()
        };
        if tokens == 0 {
            return;
        }
        let mut meta = Map::new();
        meta.insert(TURN_TOKENS_META_KEY.to_string(), json!(tokens));
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// EXP-853 rule 2: record an EXPLICIT mode change (an approval, a steered
    /// `set_mode`, the `EnterPlanMode` hook) so the CLI's next `init`
    /// announcement cannot walk it back inside the grace window.
    fn note_explicit_mode(&self, mode: &str) {
        self.lock().explicit_mode = Some((mode.to_string(), Instant::now()));
    }

    /// How long this run's explicit mode outranks the CLI's announcement
    /// ([`MODE_ANNOUNCE_GRACE`], or the run-scoped override).
    fn mode_announce_grace(&self) -> Duration {
        self.spec
            .spawn
            .env
            .iter()
            .find(|(key, _)| key == MODE_ANNOUNCE_GRACE_ENV)
            .and_then(|(_, value)| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(MODE_ANNOUNCE_GRACE)
    }

    /// EXP-784: claude's session id changed mid-run (a `/clear`). Rides a
    /// no-op `session_info_update` the mapper ignores; the host reads the
    /// `_meta` and re-records the run's `agent_native_session_id`.
    fn publish_native_id(&self, cx: &ConnectionTo<Client>, native: &str) {
        let mut meta = Map::new();
        meta.insert(NATIVE_SESSION_META_KEY.to_string(), json!(native));
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// EXP-1051: the `context_layout` slot, on the same carrier as the
    /// rate-limit one — a no-op `session_info_update` with the frame on its
    /// `_meta`.
    ///
    /// ONE-SHOT per conversation, under the state lock: the first request of
    /// a run reports how many tokens it carried, everything else in the bar
    /// is a launch-time byte count, and re-publishing that every turn would
    /// be a slot rewrite that says nothing new.
    ///
    /// `prefix` = what the agent just measured. Without one (the first
    /// request has not landed yet) a NATIVE resume may still draw the bar
    /// from the base its predecessor measured — but only while the model is
    /// the same string, because the base IS the model's own system prompt.
    fn publish_context_layout(&self, cx: &ConnectionTo<Client>, prefix: Option<u64>) {
        let (segments, model) = {
            let mut state = self.lock();
            if state.context_layout_published {
                return;
            }
            let base = match prefix {
                Some(prefix) => Some(crate::context_layout::BasePrefix::Measured(prefix)),
                None => state
                    .carried_base
                    .as_ref()
                    .filter(|carried| carried.model == state.model)
                    .map(|carried| crate::context_layout::BasePrefix::Carried(carried.tokens)),
            };
            let segments = crate::context_layout::layout(&self.spec.context_layers, base);
            if segments.is_empty() {
                return;
            }
            state.context_layout_published = true;
            (segments, state.model.clone())
        };
        self.notify_meta(
            cx,
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()),
            crate::context_layout::meta(&segments, Some(&model)),
        );
    }

    /// EXP-784: the `rate_limit` slot, as `_meta` on a no-op
    /// `session_info_update` (`RATE_LIMIT_META_KEY`, read by the mapper).
    /// `status` empty/`ok` is the clear; the mapper dedupes identical
    /// re-emits, so the adapter never has to.
    fn publish_rate_limit(
        &self,
        cx: &ConnectionTo<Client>,
        status: &str,
        resets_at: Option<i64>,
        message: Option<&str>,
        window: Option<&str>,
    ) {
        let mut slot = Map::new();
        slot.insert("status".to_string(), json!(status));
        if let Some(at) = resets_at {
            slot.insert("resetsAt".to_string(), json!(at));
        }
        if let Some(message) = message {
            slot.insert("message".to_string(), json!(message));
        }
        // FEED-34: the window the reset belongs to, for the row's `blocked`
        // (the wire's `rate_limit` frame does not carry it).
        if let Some(window) = window {
            slot.insert("window".to_string(), json!(window));
        }
        let slot = Value::Object(slot);
        {
            let mut state = self.lock();
            if state.rate_limit.published.as_ref() == Some(&slot) {
                return;
            }
            state.rate_limit.published = Some(slot.clone());
        }
        let mut meta = Map::new();
        meta.insert(RATE_LIMIT_META_KEY.to_string(), slot);
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// A synthetic non-prose frame: drop what streamed of it, and on the MAIN
    /// lane count it as delivered so the turn's `result` (which repeats it)
    /// is never forwarded as a narration. A subagent's frame is not the
    /// main turn's result.
    fn suppress_synthetic(&self, message_id: Option<&str>, main_lane: bool) {
        let mut state = self.lock();
        if let Some(id) = message_id {
            state.streamed.remove(id);
        }
        if main_lane {
            state.delivered_text = true;
        }
    }

    /// An inline API failure ROW (`API_ERROR_META_KEY` on a no-op
    /// `session_info_update`, read by the mapper) — never the rate-limit
    /// slot, never a wall, never deduped. A subagent's carries its lane.
    fn publish_api_error(
        &self,
        cx: &ConnectionTo<Client>,
        text: &str,
        error_type: Option<&str>,
        parent: &Option<String>,
    ) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let mut slot = Map::new();
        slot.insert("message".to_string(), json!(text));
        if let Some(error_type) = error_type {
            slot.insert("errorType".to_string(), json!(error_type));
        }
        let mut meta = Map::new();
        meta.insert(API_ERROR_META_KEY.to_string(), Value::Object(slot));
        if let Some(parent) = parent {
            meta.insert(PARENT_TOOL_CALL_META_KEY.to_string(), json!(parent));
        }
        self.notify_meta(cx, SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()), meta);
    }

    /// A `rate_limit_event`: a limited status goes on the slot (keeping a
    /// notice's text if one is up); the ordinary `allowed` clears it while
    /// no notice is up, or once the notice's window has reopened by the
    /// clock — see [`RateLimitState::notice_active`].
    fn on_rate_limit_event(&self, cx: &ConnectionTo<Client>, info: &wire::RateLimitInfo) {
        self.publish_live_usage(info);
        let resets_at = wire::resets_at_millis(info.resets_at);
        let window = info.window();
        let mut state = self.lock();
        if info.is_limited() {
            let status = info.status.trim().to_string();
            state.rate_limit.status = Some(status.clone());
            state.rate_limit.resets_at = resets_at;
            state.rate_limit.window = window;
            drop(state);
            self.publish_rate_limit(cx, &status, resets_at, None, window);
            return;
        }
        if state.rate_limit.notice_active {
            // EXP-831: the notice named when its window reopens; past that
            // stamp a non-limited event IS the reopening. Inside it the
            // event is the pre-429 request check — keep the wall (and the
            // state the next repeated notice reads).
            let reopened = state
                .rate_limit
                .resets_at
                .is_some_and(|at| now_unix_millis() >= at);
            drop(state);
            if reopened {
                self.clear_rate_limit_notice(cx);
            }
            return;
        }
        state.rate_limit.status = None;
        state.rate_limit.resets_at = None;
        state.rate_limit.window = None;
        drop(state);
        self.publish_rate_limit(cx, "ok", None, None, None);
    }

    /// EXP-819: the frame's windows into the machine's live usage registry,
    /// merged over what this run already named — the usage poller reads
    /// them instead of spending a request, so the bar moves per turn.
    fn publish_live_usage(&self, info: &wire::RateLimitInfo) {
        let fresh = info.usage_windows();
        if fresh.is_empty() {
            return;
        }
        let windows = {
            let mut state = self.lock();
            state.rate_limit.windows =
                coding::agent_usage::merge_live_windows(&state.rate_limit.windows, &fresh);
            state.rate_limit.windows.clone()
        };
        coding::agent_usage::live::publish(
            coding::CodingAgent::Claude,
            &self.account_profile,
            windows,
        );
    }

    /// A synthetic limit notice (`You've hit your session limit · resets
    /// 12:10pm`): ONTO the slot as its message, never a narration — the CLI
    /// repeats it on every request of the turn, and each used to be a bubble.
    fn on_rate_limit_notice(&self, cx: &ConnectionTo<Client>, text: &str) {
        let mut state = self.lock();
        state.rate_limit.notice_active = true;
        let status = state
            .rate_limit
            .status
            .clone()
            .unwrap_or_else(|| wire::RATE_LIMIT_FALLBACK_STATUS.to_string());
        let resets_at = state.rate_limit.resets_at;
        // The typed event before the notice named the window; a CLI that
        // sent the notice alone names it in prose.
        let window = state.rate_limit.window.or_else(|| wire::window_from_notice(text));
        state.rate_limit.window = window;
        drop(state);
        self.publish_rate_limit(cx, &status, resets_at, Some(text.trim()), window);
    }

    /// Real assistant activity (text or a tool call) after a notice, or a
    /// non-limited event past its reset: the window reopened, clear it.
    fn clear_rate_limit_notice(&self, cx: &ConnectionTo<Client>) {
        let mut state = self.lock();
        if !state.rate_limit.notice_active {
            return;
        }
        state.rate_limit = RateLimitState {
            windows: std::mem::take(&mut state.rate_limit.windows),
            ..RateLimitState::default()
        };
        drop(state);
        self.publish_rate_limit(cx, "ok", None, None, None);
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
            // EXP-1224: no turn of the CLI's own outlives the CLI (the host
            // closes the slot when the connection goes).
            state.continuation_running = false;
            state.continuation_end_pending = false;
            state.continuation_anticipated = false;
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
            // EXP-784: the plan window, onto the wire's `rate_limit` slot.
            // (The upstream adapter re-emits its usage snapshot on it; the
            // numbers are unchanged, so nothing of that is mirrored.)
            ClaudeOut::RateLimitEvent(event) => self.on_rate_limit_event(cx, &event.rate_limit_info),
            // Answering a keep_alive is a protocol error; unknown frame types
            // are how the CLI ships new features.
            ClaudeOut::KeepAlive | ClaudeOut::Unknown => {}
        }
    }

    fn on_system(self: &Arc<Self>, cx: &ConnectionTo<Client>, system: wire::SystemMsg) {
        match SystemSubtype::classify(&system.subtype) {
            SystemSubtype::Init => {
                let mut state = self.lock();
                let mut republish = None;
                let mut cleared_context = false;
                let first_init = !std::mem::replace(&mut state.saw_init, true);
                if !system.session_id.is_empty() {
                    state.native_session_id = Some(system.session_id.clone());
                    // EXP-784: a `/clear` re-inits under a NEW uuid. The run
                    // record must follow it or a resume reopens the dead
                    // conversation; the same id again is not news.
                    if state.published_native_id.as_deref() != Some(system.session_id.as_str()) {
                        // A LATER init under a new id is a cleared
                        // conversation (the first may lawfully differ from
                        // the pin): its todo list went with it, so the Task*
                        // lane is forgotten and an empty plan goes out below
                        // (the mapper's `task_list: []`).
                        cleared_context = !first_init;
                        if cleared_context {
                            state.plan_tasks.clear();
                            // EXP-1051: a `/clear` is a NEW conversation
                            // under a new uuid — its prefix gets measured
                            // again, and the base the old one carried
                            // describes a transcript that no longer exists.
                            state.context_layout_published = false;
                            state.carried_base = None;
                            state.resumed_conversation = false;
                        }
                        state.published_native_id = Some(system.session_id.clone());
                        republish = Some(system.session_id.clone());
                    }
                }
                if !system.model.is_empty() {
                    state.model = system.model.clone();
                    state.context_window.infer(&system.model);
                }
                // EXP-853 rule 2: the CLI re-announces `permissionMode` on
                // every turn init, and an approval it has not applied yet
                // announces the OLD mode — which flipped the Plan chip back
                // and forth. An announcement that CONTRADICTS an explicit
                // change inside `MODE_ANNOUNCE_GRACE` is dropped; past the
                // window the CLI's word is the truth and is published.
                let grace = self.mode_announce_grace();
                let announced = init_mode(system.permission_mode.as_deref(), &state.mode);
                let stale = match (&announced, &state.explicit_mode) {
                    (Some(announced), Some((explicit, at))) => {
                        announced != explicit && at.elapsed() < grace
                    }
                    _ => false,
                };
                if stale {
                    log::debug!(
                        "engine: claude init announced mode {:?} against the explicit {:?} set {}ms ago — dropped",
                        system.permission_mode,
                        state.explicit_mode.as_ref().map(|(mode, _)| mode),
                        state
                            .explicit_mode
                            .as_ref()
                            .map(|(_, at)| at.elapsed().as_millis())
                            .unwrap_or(0),
                    );
                }
                let mode_changed = match announced.filter(|_| !stale) {
                    Some(mode) => {
                        // The CLI agrees with us now (or won): the explicit
                        // record has done its job.
                        state.explicit_mode = None;
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
                // EXP-1224 (A): a re-announce under the SAME id with no
                // prompt waiting on a `result` (none in flight, or the one
                // in flight already had its `result` and only waits on its
                // background tasks) is the CLI starting a turn by itself —
                // a background task's notification woke the model. Never
                // the first init, a `/clear`, or a transcript replay.
                let continuing = !first_init
                    && !cleared_context
                    && !state.replaying_history
                    && (state.turns.is_empty() || state.deferred.is_some());
                if continuing {
                    self.begin_continuation(cx, &mut state, false);
                }
                drop(state);
                if let Some(native) = republish {
                    self.publish_native_id(cx, &native);
                }
                if cleared_context {
                    self.notify(cx, SessionUpdate::Plan(Plan::new(Vec::new())));
                }
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
                } else {
                    // EXP-969: a manual `/compact` does not always land a
                    // `compact_boundary` — the status frame simply stops
                    // saying "compacting". Without this edge the host's queue
                    // gate stays shut for the rest of the run and every
                    // message typed behind the fold is held forever.
                    let closed = self.lock().compaction.take();
                    if let Some(id) = closed {
                        let mut meta = Map::new();
                        meta.insert(
                            crate::local::COMPACTION_TRIGGER_META_KEY.to_string(),
                            json!("manual"),
                        );
                        self.notify_meta(
                            cx,
                            SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                                CompactionId::new(id),
                                CompactionStatus::Completed,
                            )),
                            meta,
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
                // EXP-1051: and it does not change the context LAYOUT either
                // — the playbook, the memory files and the system prompt all
                // survive the fold, so the slot already holds the right
                // frame and nothing is re-published here.
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
                    // EXP-1224: the CLI says it is idle, so no turn of its
                    // own runs — the backstop for a continuation whose
                    // `result` never came (or that was anticipated and never
                    // started).
                    if state.continuation_running {
                        self.end_continuation(cx, &mut state);
                    } else if state.turns.is_empty() && !state.continuation_end_pending {
                        // A turn the mapper re-opened off output alone
                        // (EXP-1224 C) is over too; an `ended` with nothing
                        // open is a no-op there.
                        self.publish_turn(cx, steer::TurnState::Ended);
                    }
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
                let task_type = system
                    .extra
                    .get("task_type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // EXP-850 §2: the strip's `toolId`. The list frame that opened
                // this task named no tool call (the CLI only says it here), so
                // the entry is patched and the strip re-published.
                let mut relist = false;
                if let Some(tool_use_id) = tool_use_id.clone() {
                    let mut state = self.lock();
                    state.task_tool_ids.insert(task_id.clone(), tool_use_id.clone());
                    if let Some(task) = state
                        .background_tasks
                        .iter_mut()
                        .find(|task| task.id == task_id && task.tool_id.is_none())
                    {
                        task.tool_id = Some(tool_use_id);
                        relist = true;
                    }
                }
                if relist {
                    self.publish_background_tasks(cx);
                }
                let description = system
                    .extra
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                // EXP-850 §3: a `local_workflow` task IS the card. It opens
                // one (keyed by the `Workflow` call's own tool_use_id, so the
                // card patches that tool row) and publishes NO subagent edge:
                // the card replaces it.
                let workflow = task_type == "local_workflow";
                if workflow {
                    let id = tool_use_id.clone().unwrap_or_else(|| task_id.clone());
                    let mut state = self.lock();
                    state.workflow_of_task.insert(task_id.clone(), id.clone());
                    if !state.workflows.iter().any(|run| run.id == id) {
                        state.workflows.push(WorkflowRun {
                            id: id.clone(),
                            name: workflow_card_name(
                                system.extra.get("workflow_name").and_then(Value::as_str),
                                description.as_deref(),
                            ),
                            description: description.clone(),
                            status: steer::WorkflowStatus::Running,
                            phases: BTreeMap::new(),
                            agents: BTreeMap::new(),
                            summary: None,
                            published_at: None,
                            lanes: BTreeMap::new(),
                        });
                        // EXP-850 §3 review: the card map is capped like the
                        // journal's — a long run that starts a hundred
                        // workflows keeps the newest 16, and the id maps of
                        // the ones it drops go with them.
                        evict_workflows(&mut state);
                    }
                    drop(state);
                    self.publish_workflow(cx, &id, true);
                }
                let mut state = self.lock();
                let turn_seq = state.turn_seq;
                // EXP-847: the `Agent` call's own words for the job, off the
                // tool table the `content_block_start` filled. Read ONCE here:
                // the tool result takes the entry away.
                let spawn_input = tool_use_id
                    .as_deref()
                    .and_then(|id| state.tools.get(id))
                    .map(|entry| entry.input.clone());
                let title = spawn_input.as_ref().and_then(task_title);
                let subagent_type = task_agent_type(subagent_type.as_deref(), spawn_input.as_ref());
                // EXP-850 §4 / EXP-856: a task whose id equals a WORKFLOW
                // AGENT's `agentId` is that agent — its edges publish under
                // the agent id (so the duplicate and its own started/completed
                // share one identity) and carry the card's id, the agent's
                // label and its type.
                let workflow_agent = state.workflows.iter().find_map(|run| {
                    run.agent_by_id(&task_id)
                        .map(|agent| (run.id.clone(), agent.label.clone(), agent.state, run.status))
                });
                let live_copy = match &workflow_agent {
                    // EXP-856 review: an agent is only a LIVE copy while its
                    // workflow itself still runs. A stopped or killed
                    // workflow leaves its agents frozen mid-`running`, and
                    // without this a legitimate later resume of that id would
                    // publish a false `duplicate` warning.
                    Some((_, _, agent_state, status)) => {
                        *status == steer::WorkflowStatus::Running && !agent_state.is_finished()
                    }
                    // An ordinary subagent that never finished: the CLI reuses
                    // its id for the resumed copy.
                    None => state.tasks.get(&task_id).is_some_and(|task| task.live),
                };
                let edge_id = match &workflow_agent {
                    Some(_) => task_id.clone(),
                    None => state
                        .tasks
                        .get(&task_id)
                        .and_then(|task| task.tool_use_id.clone())
                        .or_else(|| tool_use_id.clone())
                        .unwrap_or_else(|| task_id.clone()),
                };
                let workflow_id = workflow_agent.as_ref().map(|(id, ..)| id.clone());
                let label = workflow_agent
                    .as_ref()
                    .map(|(_, label, ..)| label.clone())
                    .filter(|label| !label.is_empty())
                    .or_else(|| title.clone())
                    .or_else(|| description.clone())
                    .filter(|label| !label.trim().is_empty())
                    .unwrap_or_else(|| task_id.clone());
                let title = match &workflow_agent {
                    Some((_, agent_label, ..)) if !agent_label.is_empty() => {
                        Some(agent_label.clone())
                    }
                    _ => title.clone(),
                };
                let agent_type = subagent_type
                    .clone()
                    .or_else(|| workflow_id.as_ref().map(|_| "agent".to_string()));
                // EXP-850 §2: only an AGENT task gets subagent edges. An
                // older CLI sends no `task_type` at all, which stays an agent.
                let silent = !task_type.is_empty() && task_type != "local_agent" && !workflow;
                state.tasks.insert(
                    task_id.clone(),
                    TaskEntry {
                        tool_use_id: Some(edge_id.clone()),
                        subagent_type: agent_type.clone(),
                        title: title.clone(),
                        live: true,
                        backgrounded,
                        turn_seq,
                        started_at: Instant::now(),
                        last_status: Some("started".to_string()),
                        workflow_id: workflow_id.clone(),
                        silent,
                    },
                );
                drop(state);
                if workflow || silent {
                    return;
                }
                // EXP-856: the warning goes out BEFORE the ordinary started
                // edge, so a reader sees "a second copy started" above the run
                // it is about.
                if live_copy {
                    self.publish_duplicate(
                        cx,
                        &edge_id,
                        agent_type.as_deref(),
                        title.as_deref(),
                        workflow_id.as_deref(),
                        &label,
                    );
                }
                self.publish_subagent_edge(
                    cx,
                    &edge_id,
                    agent_type.as_deref(),
                    "started",
                    title.as_deref(),
                    workflow_id.as_deref(),
                );
            }
            // EXP-850 §2: the CLI's FULL current list — the strip's one input.
            SystemSubtype::BackgroundTasksChanged => {
                let tasks: Vec<steer::BackgroundTask> = system
                    .extra
                    .get("tasks")
                    .and_then(Value::as_array)
                    .map(|tasks| {
                        tasks
                            .iter()
                            .filter_map(|task| {
                                let id = task.get("task_id").and_then(Value::as_str)?;
                                Some(steer::BackgroundTask {
                                    id: id.to_string(),
                                    kind: steer::BackgroundTaskKind::from_task_type(
                                        task.get("task_type")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default(),
                                    ),
                                    description: task
                                        .get("description")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                    tool_id: task
                                        .get("tool_use_id")
                                        .and_then(Value::as_str)
                                        .map(str::to_string),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                {
                    let mut state = self.lock();
                    let mut tasks = tasks;
                    for task in tasks.iter_mut() {
                        if task.tool_id.is_none() {
                            task.tool_id = state.task_tool_ids.get(&task.id).cloned();
                        }
                    }
                    if state.background_tasks == tasks {
                        return;
                    }
                    state.background_tasks = tasks;
                    // EXP-927: an overdue task the list just DROPPED is over,
                    // whether or not its notification ever arrives.
                    self.expire_tasks(cx, &mut state);
                }
                self.publish_background_tasks(cx);
            }
            // EXP-850 §3: one workflow's progress array.
            SystemSubtype::TaskProgress => {
                let Some(task_id) = system.task_id.clone() else { return };
                let Some(entries) = system
                    .extra
                    .get("workflow_progress")
                    .and_then(Value::as_array)
                    .cloned()
                else {
                    // A progress frame WITHOUT the array is a usage tick; it
                    // says nothing the card renders.
                    return;
                };
                let id = {
                    let state = self.lock();
                    match state.workflow_of_task.get(&task_id) {
                        Some(id) => id.clone(),
                        None => system
                            .extra
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_else(|| task_id.clone()),
                    }
                };
                let mut changed = false;
                let mut steps = Vec::new();
                {
                    let mut state = self.lock();
                    if !state.workflows.iter().any(|run| run.id == id) {
                        // Review: no card holds this id (an evicted workflow,
                        // or a progress frame for something else entirely) —
                        // leave no mapping behind, or `workflow_of_task` grows
                        // for the rest of the run over ids nothing reads.
                        return;
                    }
                    state.workflow_of_task.insert(task_id.clone(), id.clone());
                    let Some(run) = state.workflows.iter_mut().find(|run| run.id == id) else {
                        return;
                    };
                    for entry in &entries {
                        match entry.get("type").and_then(Value::as_str) {
                            Some("workflow_phase") => {
                                let Some(index) = progress_index(entry) else { continue };
                                run.phases.insert(
                                    index,
                                    steer::WorkflowPhase {
                                        index,
                                        title: entry
                                            .get("title")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_string(),
                                    },
                                );
                            }
                            Some("workflow_agent") => {
                                let Some(index) = progress_index(entry) else { continue };
                                let agent = workflow_agent(index, entry);
                                let moved = run
                                    .agents
                                    .get(&index)
                                    .is_none_or(|held| held.state != agent.state);
                                changed |= moved;
                                // EXP-1225: the agent's own lane.
                                let lane = run.lanes.entry(index).or_default();
                                steps.extend(lane_steps(&run.id, &agent, lane));
                                run.agents.insert(index, agent);
                            }
                            _ => {}
                        }
                    }
                }
                // §3: at most one frame per second while nothing moves, but
                // EVERY agent state change publishes at once.
                self.publish_workflow(cx, &id, changed);
                for step in steps {
                    self.publish_lane_step(cx, step);
                }
            }
            // EXP-850 §5: half of the working caption's token count. The
            // frame's `estimated_tokens` restarts per thinking block, so the
            // DELTAS are what accumulate over a turn.
            SystemSubtype::ThinkingTokens => {
                let delta = system
                    .extra
                    .get("estimated_tokens_delta")
                    .and_then(Value::as_u64)
                    .or_else(|| system.extra.get("estimated_tokens").and_then(Value::as_u64))
                    .unwrap_or(0);
                if delta == 0 {
                    return;
                }
                self.lock().turn_thinking_tokens += delta;
                self.publish_turn_tokens(cx);
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
                // EXP-856: `killed` and `stopped` are terminal too — the
                // capture's background Bash reports both, and a task left
                // LIVE forever deferred the turn's settle and made every later
                // `task_started` for its id read as a duplicate.
                let terminal = matches!(
                    status.as_str(),
                    "completed" | "failed" | "cancelled" | "canceled" | "killed" | "stopped"
                );
                // EXP-850 §3: a workflow's terminal edge moves the CARD, never
                // a subagent row.
                let workflow_id = self.lock().workflow_of_task.get(&task_id).cloned();
                if let Some(id) = &workflow_id {
                    let summary = system
                        .extra
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .filter(|summary| !summary.trim().is_empty());
                    let mut closed = Vec::new();
                    {
                        let mut state = self.lock();
                        if let Some(run) = state.workflows.iter_mut().find(|run| run.id == *id) {
                            if terminal {
                                run.status = steer::WorkflowStatus::from_task_status(&status);
                                closed = close_lanes(run);
                            }
                            if let Some(summary) = summary {
                                run.summary = Some(summary);
                            }
                        }
                    }
                    self.publish_workflow(cx, id, true);
                    for step in closed {
                        self.publish_lane_step(cx, step);
                    }
                }
                let mut state = self.lock();
                // `task_notification` and `task_updated` share this arm and
                // the CLI sends both for one edge often enough to matter
                // (7 duplicate `completed`s in 54, measured), which drew the
                // subagent twice. Only a CHANGE is republished.
                let (tool_use_id, subagent_type, title, repeat, agent_workflow, wakes_model) =
                    match state.tasks.get_mut(&task_id) {
                        Some(task) => {
                            // A silent (non-agent) task moves the strip, never
                            // a subagent row: `repeat` folds it into the
                            // "nothing to publish" branch below.
                            let repeat = task.silent
                                || task.last_status.as_deref() == Some(status.as_str());
                            // EXP-1224: a BACKGROUND task (a backgrounded
                            // agent or shell, or a workflow) that just
                            // finished hands the model a notification, and
                            // the CLI wakes it for a turn of its own. Only
                            // the first terminal frame of the pair counts.
                            let wakes_model = task.live
                                && terminal
                                && (task.backgrounded || workflow_id.is_some())
                                && matches!(status.as_str(), "completed" | "failed");
                            task.live = !terminal;
                            task.last_status = Some(status.clone());
                            (
                                task.tool_use_id.clone(),
                                task.subagent_type.clone(),
                                task.title.clone(),
                                repeat,
                                task.workflow_id.clone(),
                                wakes_model,
                            )
                        }
                        None => (None, None, None, false, None, false),
                    };
                drop(state);
                // The edge goes out BEFORE the settle it unblocks: settling
                // first ends the `session/prompt`, and a client that renders
                // the subagent card off the edge would see the run finish
                // with that card still spinning (EXP-753).
                if !repeat && workflow_id.is_none() {
                    self.publish_subagent_edge(
                        cx,
                        tool_use_id.as_deref().unwrap_or(&task_id),
                        subagent_type.as_deref(),
                        &status,
                        title.as_deref(),
                        agent_workflow.as_deref(),
                    );
                }
                if terminal {
                    // FEED-44: the strip drops the finished agent NOW, before
                    // the settle ends the turn, so the idle edge already
                    // reads the run as not busy.
                    self.retire_background_agents(cx, Some(&task_id));
                    let mut state = self.lock();
                    // EXP-1224: when the model is BETWEEN turns (its prompt
                    // already had its `result`), that notification is about
                    // to start a continuation — open it NOW, before the
                    // settle this frame releases, so the prompt's answer
                    // lands on a turn that is still running instead of
                    // flashing "Done" until the CLI's `init` re-opens it.
                    let between_turns = state.deferred.is_some()
                        || state.continuation_end_pending
                        || (state.turns.is_empty() && !state.continuation_running);
                    if wakes_model && between_turns && !state.replaying_history {
                        self.begin_continuation(cx, &mut state, true);
                    }
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

        // EXP-784: claude writes its own notices as `<synthetic>` frames;
        // `classify_assistant_notice` sorts them by the frame's `error`. Only
        // the usage wall is STATE (the slot gets its text, no bubble; a real
        // message afterwards clears it); an API failure is an inline row,
        // filler is dropped, the login notice stays a narration.
        let model = message.message.get("model").and_then(Value::as_str);
        let text: String = blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(id) = &message_id {
            let mut state = self.lock();
            state.synthetic_messages.remove(id);
        }
        // EXP-850 §5: the other half of the turn's token count. The streamed
        // frame and the consolidated one repeat the SAME `output_tokens`, so
        // the highest per message id is kept rather than summed.
        if let Some(id) = &message_id {
            if let Some(tokens) = message
                .message
                .get("usage")
                .and_then(|usage| usage.get("output_tokens"))
                .and_then(Value::as_u64)
            {
                let moved = {
                    let mut state = self.lock();
                    let held = state.turn_message_tokens.entry(id.clone()).or_insert(0);
                    let moved = tokens > *held;
                    if moved {
                        *held = tokens;
                    }
                    moved
                };
                if moved {
                    self.publish_turn_tokens(cx);
                }
            }
        }
        // EXP-1051: the fallback for a gateway that streams nothing — the
        // consolidated `assistant` frame carries the same `usage`. The
        // one-shot flag means the streamed path above wins when both arrive.
        self.measure_prefix(cx, parent.as_deref(), model, &message.message["usage"]);
        match wire::classify_assistant_notice(
            model,
            message.error.as_deref(),
            message.is_api_error_message,
            &text,
        ) {
            wire::AssistantNotice::RateLimit => {
                if let Some(id) = &message_id {
                    self.lock().streamed.remove(id);
                }
                // Counts as delivered: the turn's `result` repeats the notice
                // and must not forward it as a narration either.
                let replaying = {
                    let mut state = self.lock();
                    state.delivered_text = true;
                    state.replaying_history
                };
                // EXP-866: a notice inside a replayed transcript is the wall the
                // PREVIOUS run hit, not this one's — swallowed, never re-armed.
                if !replaying {
                    self.on_rate_limit_notice(cx, &text);
                }
                return;
            }
            // A transient failure (outage, timeout, 400): an inline row, never
            // the rate-limit slot — it used to wall the run and rotate its
            // account. Published on replay too: a transcript fact, not state.
            wire::AssistantNotice::ApiError { error_type } => {
                self.suppress_synthetic(message_id.as_deref(), parent.is_none());
                self.publish_api_error(cx, &text, error_type, &parent);
                return;
            }
            // Client-side filler (`No response requested.`): nothing.
            wire::AssistantNotice::Filler => {
                self.suppress_synthetic(message_id.as_deref(), parent.is_none());
                return;
            }
            wire::AssistantNotice::Auth | wire::AssistantNotice::None => {}
        }
        // EXP-831: a tool call is as much an answer as text — a run that
        // resumed with tool calls only used to keep the wall up until it
        // next narrated.
        if model != Some(wire::SYNTHETIC_MODEL) && wire::is_real_assistant_activity(&blocks) {
            self.clear_rate_limit_notice(cx);
        }

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
        self.remember_plan_file(&name, &input);

        // TodoWrite IS the plan lane, and the Task* tools render as plan
        // entries when their results arrive — neither surfaces a tool call.
        if name == "TodoWrite" {
            let entries = todo_entries(&input);
            // EXP-927: the plan lane is the MAIN thread's list (the strip's
            // first block ×4); a subagent's own list never replaces it. An
            // EMPTY list on the main thread goes out too: it is the agent
            // clearing its list, and the mapper turns it into the
            // `task_list {entries: []}` every client needs to unstick a
            // stale "2 open" (release review R2).
            if parent.is_none() {
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

        let mut info = tool_info(&name, &input, self.cwd());
        // EXP-1202: `Content` on an `Execute` call IS command output to the
        // engine (codex reports it the same way), so the `Bash` description
        // the permission card shows as its body is not announced with the
        // call: it would open the published output.
        if info.kind == ToolKind::Execute {
            info.content.clear();
        }
        // EXP-850 §1: a WAIT row carries its kind and its human label in
        // `_meta` — the label is resolved HERE because only the adapter holds
        // the background-task list a `TaskOutput` names by id.
        let wait = is_wait_tool(&name).then(|| self.wait_detail(&name, &input));
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
        match wait {
            Some(detail) => {
                let mut meta = Map::new();
                meta.insert(TOOL_KIND_META_KEY.to_string(), json!("wait"));
                if let Some(detail) = detail {
                    meta.insert(TOOL_DETAIL_META_KEY.to_string(), json!(detail));
                }
                if let Some(parent) = parent {
                    meta.insert(PARENT_TOOL_CALL_META_KEY.to_string(), json!(parent));
                }
                self.notify_meta(cx, update, meta);
            }
            None => self.emit_chunk(cx, update, parent),
        }
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

    /// EXP-784: whether the message now streaming for `parent_key` is a
    /// `<synthetic>` one, whose text is the limit notice the consolidated
    /// `assistant` frame puts on the slot — never streamed as prose.
    fn streaming_synthetic(&self, parent_key: &str) -> bool {
        let state = self.lock();
        state
            .current_message
            .get(parent_key)
            .is_some_and(|id| state.synthetic_messages.contains(id))
    }

    fn on_stream_event(self: &Arc<Self>, cx: &ConnectionTo<Client>, event: wire::StreamEventMsg) {
        let parent = event.parent_tool_use_id.clone();
        let parent_key = parent.clone().unwrap_or_default();
        match event.event_type() {
            "message_start" => {
                let message = &event.event["message"];
                let model = message.get("model").and_then(Value::as_str);
                if let Some(id) = message.get("id").and_then(Value::as_str) {
                    let mut state = self.lock();
                    state.current_message.insert(parent_key.clone(), id.to_string());
                    // EXP-784: a synthetic message's deltas are not prose.
                    if model == Some(wire::SYNTHETIC_MODEL) {
                        state.synthetic_messages.insert(id.to_string());
                    }
                }
                if let Some(model) = model.filter(|model| *model != wire::SYNTHETIC_MODEL) {
                    self.lock().context_window.infer(model);
                }
                self.merge_usage(cx, &message["usage"], None);
                // EXP-1051: the ONE measured number the context bar has —
                // what the FIRST request of this conversation carried.
                // Deliberately not `TokenSnapshot::used()`, which adds the
                // output this request is about to produce.
                self.measure_prefix(cx, parent.as_deref(), model, &message["usage"]);
            }
            "message_delta" => self.merge_usage(cx, &event.event["usage"], None),
            "content_block_start" => {
                if self.streaming_synthetic(&parent_key) {
                    return;
                }
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
                if self.streaming_synthetic(&parent_key) {
                    return;
                }
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

    /// EXP-1051: whether the message now arriving is a REQUEST of this
    /// conversation whose reported input is the conversation's own prefix.
    /// A subagent runs in a context of its own; a `<synthetic>` frame is
    /// claude's limit notice rather than a request; the `/clear` a plan
    /// restart injects carries an empty prefix that would freeze the bar at
    /// nothing; and a replayed transcript is the PREVIOUS run's numbers.
    fn measurable_request(&self, parent: Option<&str>, model: Option<&str>) -> bool {
        if parent.is_some() || model == Some(wire::SYNTHETIC_MODEL) {
            return false;
        }
        let state = self.lock();
        !state.skip_local_command_result && !state.replaying_history
    }

    /// EXP-1051: one request of this conversation has just reported its
    /// `usage` — publish the context layout off it, once. A FRESH
    /// conversation measures its prefix; a RESUMED one (its first request
    /// re-sends the whole transcript, which is no prefix at all) publishes
    /// without a measurement and draws the base its predecessor recorded,
    /// if the model is unchanged. Everything `measurable_request` refuses
    /// (subagents, the synthetic notice, the injected `/clear`, replayed
    /// history) publishes nothing.
    fn measure_prefix(
        &self,
        cx: &ConnectionTo<Client>,
        parent: Option<&str>,
        model: Option<&str>,
        usage: &Value,
    ) {
        if !self.measurable_request(parent, model) {
            return;
        }
        if self.lock().resumed_conversation {
            self.publish_context_layout(cx, None);
            return;
        }
        let prefix = prefix_tokens(usage);
        if prefix > 0 {
            self.publish_context_layout(cx, Some(prefix));
        }
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
        // EXP-784: a limited turn's result REPEATS the notice; the slot has
        // it (or gets it here, for a CLI that sent no assistant frame). An
        // API failure's result repeats ITS row the same way.
        if wire::is_rate_limit_result(&result) {
            self.on_rate_limit_notice(cx, &result.result);
        } else if result.is_error
            && !delivered
            && !local_only
            && result.result.trim_start().starts_with(wire::API_ERROR_PREFIX)
        {
            // A CLI that sent the failure only as the result: still a row,
            // never a narration or a wall.
            self.publish_api_error(cx, &result.result, None, &None);
        } else if wire::should_forward_result(local_only, delivered, &result) {
            self.notify(
                cx,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(result.result.clone()),
                ))),
            );
        }

        // EXP-905: every turn end re-reads the transcript's tail for the
        // conversation's (possibly renamed) title — incrementally, off the
        // pump.
        self.schedule_title_poll(cx, Duration::ZERO);

        let cancelled = self.lock().cancelled;
        let outcome = wire::turn_outcome(&result, cancelled);
        let mut state = self.lock();
        state.delivered_text = false;
        state.local_only_command = false;
        // EXP-1224 (A): the `result` of a turn the CLI started on its own (or
        // one with no prompt behind it at all) settles no prompt — the front
        // of `turns` is a prompt sent DURING it, still owed its own `result`.
        // Only what the CLI folded into this turn (beyond its
        // `queued_turn_count`) settles here, exactly as `settle_turns` folds
        // steers. The continuation then ends — unless background tasks of the
        // run are still live, in which case it ends when they are gone, by
        // the rule a prompted turn's settle follows.
        if state.continuation_running || (state.turns.is_empty() && state.deferred.is_none()) {
            state.continuation_running = false;
            state.continuation_anticipated = false;
            if let Some(queued) = result.queued_turn_count {
                while state.turns.len() as u64 > queued {
                    match state.turns.pop_front() {
                        Some(turn) => {
                            let _ = turn.send(outcome);
                        }
                        None => break,
                    }
                }
            }
            self.expire_tasks(cx, &mut state);
            if blocking_tasks(&state).next().is_some() {
                state.continuation_end_pending = true;
                self.arm_defer_timer(cx, &mut state);
            } else {
                self.end_continuation(cx, &mut state);
            }
            return;
        }
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
            session.note_explicit_mode(&mode);
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
            // EXP-853 rule 3: the approval's own `updatedPermissions` does not
            // stop the CLI re-announcing the OLD mode on its next init, so the
            // engine ALSO sets it explicitly (idempotent). Spawned rather than
            // awaited: this runs on the control-answer path, and the request
            // must not gate the notification the client is waiting for.
            self.note_explicit_mode(&mode);
            let session = self.clone();
            let requested = mode.clone();
            let _ = cx.spawn(async move {
                if let Err(error) =
                    session.control_request(wire::set_permission_mode(&requested)).await
                {
                    log::warn!("engine: claude plan-approval mode switch failed: {error}");
                }
                Ok(())
            });
            self.notify(
                cx,
                SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(mode))),
            );
            self.publish_config(cx);
        }
    }

    /// EXP-954: remember a `Write` into a `plans/*.md` file — the plan the
    /// `ExitPlanMode` that follows it is asking about. The newest one wins;
    /// nothing else about the call is touched (it still surfaces as an
    /// ordinary tool card).
    fn remember_plan_file(&self, name: &str, input: &Value) {
        if name != "Write" {
            return;
        }
        let Some(path) = input.get("file_path").and_then(Value::as_str) else { return };
        let path = PathBuf::from(path);
        if !is_plan_file(&path) {
            return;
        }
        let text = input
            .get("content")
            .and_then(Value::as_str)
            .filter(|content| !content.trim().is_empty())
            .map(str::to_string);
        self.lock().plan_file = Some(PlanFile { path, text });
    }

    /// EXP-954: the plan markdown for an `ExitPlanMode` whose input carries
    /// none — the content of the plan file the agent just wrote, read off
    /// disk when the stream never held it (a resumed conversation). `None`
    /// when this session has seen no plan file at all: the card then reads
    /// as it did before, title only.
    fn recovered_plan(&self, input: &Value) -> Option<String> {
        let has_plan = input
            .get("plan")
            .and_then(Value::as_str)
            .is_some_and(|plan| !plan.trim().is_empty());
        if has_plan {
            return None;
        }
        let (path, text) = {
            let state = self.lock();
            let file = state.plan_file.as_ref()?;
            (file.path.clone(), file.text.clone())
        };
        let plan = match text {
            Some(text) => text,
            None => std::fs::read_to_string(&path).ok()?,
        };
        (!plan.trim().is_empty()).then_some(plan)
    }

    async fn request_permission(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> PermissionAnswer {
        let exit_plan = request.is_exit_plan_mode();
        // EXP-954: an ExitPlanMode with an EMPTY input is the current CLI
        // approving the plan FILE it wrote a moment ago. Patch the recovered
        // markdown into the request once, here: the card body, the
        // fresh-context option and the re-prompt a clear-context approval
        // sends all read the plan from this one place.
        let patched = exit_plan
            .then(|| self.recovered_plan(&request.input))
            .flatten()
            .map(|plan| {
                let mut patched = request.clone();
                patched.input = with_plan(&request.input, plan);
                patched
            });
        let request = patched.as_ref().unwrap_or(request);
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
            // EXP-853 rule 2: the CLI's own switch INTO plan is explicit too.
            self.note_explicit_mode("plan");
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
    /// `session_id` is claude's OWN (the transcript name), not the ACP id
    /// (EXP-784).
    /// EXP-905: has the conversation been named yet?
    fn title_known(&self) -> bool {
        self.title
            .lock()
            .map(|tail| tail.published.is_some())
            .unwrap_or(false)
    }

    /// EXP-905: [`Self::poll_title`] after `delay`, on a blocking thread (the
    /// first read of a resumed run's transcript can be megabytes).
    fn schedule_title_poll(self: &Arc<Self>, cx: &ConnectionTo<Client>, delay: Duration) {
        let session = self.clone();
        let out = cx.clone();
        let _ = cx.spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let _ = tokio::task::spawn_blocking(move || session.poll_title(&out)).await;
            Ok(())
        });
    }

    /// EXP-905: read what claude appended to its transcript since the last
    /// poll and publish the conversation's name as a `session_info_update`
    /// title when it CHANGED. Stream-json never carries the name: claude
    /// writes it only into the transcript, as `ai-title` lines (re-appended
    /// as the conversation evolves) and `custom-title` lines (`/rename`).
    fn poll_title(&self, cx: &ConnectionTo<Client>) {
        let Some(native) = self.lock().native_session_id.clone() else { return };
        let changed = {
            let Ok(mut tail) = self.title.lock() else { return };
            tail.poll(&native, || transcript_path(&self.spec.spawn.env, &native))
        };
        self.publish_title(cx, changed);
    }

    /// EXP-1134: an SDK-mode claude (2.1.282 on) never names a conversation
    /// by itself, so a FRESH chat run asks for its name once, off its first
    /// real prompt (a slash command names nothing). An issue or action run is
    /// listed under its issue or action and never asks
    /// ([`AdapterSpec::name_conversation`]); a resume keeps the name the
    /// transcript already carries. Never awaited: the answer (a small model
    /// call) lands while the turn runs.
    fn request_title(self: &Arc<Self>, cx: &ConnectionTo<Client>, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with('/') {
            return;
        }
        {
            let mut state = self.lock();
            if state.title_requested {
                return;
            }
            state.title_requested = true;
        }
        let session = self.clone();
        let out = cx.clone();
        let request = wire::generate_session_title(trimmed);
        let _ = cx.spawn(async move {
            match session.control_request(request).await {
                Ok(response) => {
                    if let Some(title) = wire::generated_session_title(&response.response) {
                        let changed = session.title.lock().ok().and_then(|mut tail| tail.adopt(title));
                        session.publish_title(&out, changed);
                    }
                }
                Err(error) => log::debug!("engine: claude session title request failed: {error:?}"),
            }
            Ok(())
        });
    }

    fn publish_title(&self, cx: &ConnectionTo<Client>, changed: Option<String>) {
        if let Some(title) = changed {
            self.notify(
                cx,
                SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().title(title)),
            );
        }
    }

    fn replay_history(self: &Arc<Self>, cx: &ConnectionTo<Client>, session_id: &str) {
        let Some(path) = transcript_path(&self.spec.spawn.env, session_id) else {
            log::warn!("engine: no claude transcript for {session_id}");
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        // EXP-866: everything below is history — see `replaying_history`.
        self.lock().replaying_history = true;
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
        self.lock().replaying_history = false;
    }
}

// ---------------------------------------------------------------------------
// pure helpers
// ---------------------------------------------------------------------------

/// EXP-772: plan on, plan off. Permissions are bypassed in every mode, so
/// `default`/`acceptEdits`/`auto` differ in nothing a user can see and are
/// never offered; `dontAsk` was never offered either.
/// EXP-831: the wall clock the rate-limit notice's `resets_at` is compared
/// against (unix ms, the slot's own unit).
fn now_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

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

/// EXP-847: what the spawning `Agent`/`Task` call said the subagent is FOR —
/// its `description` input, its `name` as a fallback, `None` when it named
/// neither (the chip then falls back to the agent TYPE). The same input
/// [`tool_info`] titles the card from, so the chip and the card agree.
fn task_title(input: &Value) -> Option<String> {
    ["description", "name"]
        .into_iter()
        .filter_map(|key| input.get(key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

/// EXP-847: the subagent TYPE the chip's secondary caption reads. The CLI
/// reports `agentType: "agent"` for a teammate spawn — a word that says
/// nothing about the job — so a GENERIC or empty report defers to what the
/// spawning `Agent` call itself named (`subagent_type`, then `name`). `None`
/// when neither side named anything.
fn task_agent_type(reported: Option<&str>, input: Option<&Value>) -> Option<String> {
    let specific = |value: &str| {
        let value = value.trim();
        (!value.is_empty() && value != GENERIC_AGENT_TYPE).then(|| value.to_string())
    };
    if let Some(reported) = reported.and_then(specific) {
        return Some(reported);
    }
    let named = input.and_then(|input| {
        ["subagent_type", "agentType", "agent_type", "name"]
            .into_iter()
            .filter_map(|key| input.get(key).and_then(Value::as_str))
            .find_map(specific)
    });
    named.or_else(|| reported.map(str::trim).filter(|r| !r.is_empty()).map(str::to_string))
}

/// The CLI's placeholder subagent type for a teammate spawn (see
/// [`task_agent_type`]).
const GENERIC_AGENT_TYPE: &str = "agent";

/// EXP-856: the duplicate warning's text — byte-identical on every client
/// (STEER-WIRE-EXP-850.md §4).
pub fn duplicate_agent_detail(label: &str) -> String {
    format!("Second copy of {label} started while the first is still running (resumed by SendMessage)")
}

/// EXP-850 §3: one `workflow_progress` entry's `index` (the CLI counts from 1).
fn progress_index(entry: &Value) -> Option<u32> {
    entry
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| u32::try_from(index).ok())
}

/// EXP-850 §3: one `workflow_agent` progress entry, folded onto the wire
/// shape. `state` is the CLI's own word plus whether it named a `startedAt` —
/// a `start` without one is still QUEUED.
fn workflow_agent(index: u32, entry: &Value) -> steer::WorkflowAgent {
    let string = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
    };
    let number = |key: &str| entry.get(key).and_then(Value::as_u64);
    steer::WorkflowAgent {
        index,
        label: string("label").unwrap_or_default(),
        phase_index: entry
            .get("phaseIndex")
            .and_then(Value::as_u64)
            .and_then(|index| u32::try_from(index).ok()),
        agent_id: string("agentId"),
        model: string("model"),
        state: steer::WorkflowAgentState::from_cli(
            entry.get("state").and_then(Value::as_str).unwrap_or_default(),
            entry.get("startedAt").and_then(Value::as_u64).is_some(),
        ),
        tokens: number("tokens"),
        tool_calls: number("toolCalls").and_then(|calls| u32::try_from(calls).ok()),
        duration_ms: number("durationMs"),
        last_tool: string("lastToolName"),
        last_tool_summary: string("lastToolSummary"),
        result_preview: string("resultPreview"),
        error: string("error"),
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

/// EXP-850 §1: claude's two WAIT tools. ACP v1 has no `wait` kind, so
/// [`tool_info`] leaves them on `Other` and the adapter names the wire kind in
/// `_meta` instead ([`TOOL_KIND_META_KEY`]) — the mapper prefers that for both
/// the wire row and the local card.
///
/// `TaskOutput` blocks on a background task by id; `Monitor` watches a stream
/// it describes. Neither is `Execute`: nothing runs, the session is WAITING.
fn is_wait_tool(name: &str) -> bool {
    matches!(name, "TaskOutput" | "Monitor")
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
            // EXP-1202: the bare output, never a markdown fence: it lands in
            // the command-output card and the settle's wire `output` as is.
            if output.is_empty() {
                fields
            } else {
                fields.content(vec![text_content(output)])
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
            // EXP-1202: an MCP answer also rides as `raw_output`, the JSON the
            // mapper reads an Exponential call's preview off (codex hands its
            // `item.result` over the same way). The transcript keeps the text.
            if name.starts_with("mcp__") {
                if let Some(output) = mcp_raw_output(&raw, structured) {
                    fields = fields.raw_output(output);
                }
            }
            if text.is_empty() {
                fields
            } else {
                fields.content(vec![text_content(text)])
            }
        }
    }
}

/// EXP-1202: an MCP call's answer in a shape `exp_tool_refs::tool_result_payload`
/// reads — the message-level `tool_use_result` when it is an object or an
/// array, else the block's own content. A bare block array (what the CLI
/// records for an MCP answer) is wrapped back into the server's
/// `{content:[…]}` envelope, so its JSON text is the payload; a string stays
/// a string. Never called for a failed result, which has no answer.
fn mcp_raw_output(raw: &Value, structured: Option<&Value>) -> Option<Value> {
    let envelope = |value: &Value| match value {
        Value::Array(blocks) if blocks.iter().all(|block| block.get("type").is_some()) => {
            json!({ "content": blocks })
        }
        other => other.clone(),
    };
    match structured {
        Some(value @ (Value::Object(_) | Value::Array(_))) => Some(envelope(value)),
        _ => match raw {
            Value::Array(_) | Value::String(_) => Some(envelope(raw)),
            _ => None,
        },
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

/// EXP-954: does this path name a plan the CLI wrote for itself — any
/// `**/plans/<name>.md` (the current claude puts them under
/// `{CLAUDE_CONFIG_DIR}/plans/`, which a per-account profile moves)?
fn is_plan_file(path: &Path) -> bool {
    let md = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
    let in_plans = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|dir| dir == "plans");
    md && in_plans
}

/// EXP-954: the ExitPlanMode input with the recovered plan written into it,
/// so everything downstream sees the request the CLI used to send.
fn with_plan(input: &Value, plan: String) -> Value {
    let mut object = match input {
        Value::Object(object) => object.clone(),
        _ => Map::new(),
    };
    object.insert("plan".to_string(), Value::String(plan));
    Value::Object(object)
}

/// The plan-approval menu. EXP-772: permissions are bypassed in every mode,
/// so "manually approve edits" is no longer an answer that means anything —
/// what is left is code it, code it in a FRESH context (only when there is a
/// plan to carry), or keep planning. EXP-788: the plain "Yes" is FIRST (every
/// client promotes index 0 as the primary), the fresh-context variant second,
/// and "No, keep planning" LAST, carrying a description that says what it
/// does — it is a deny that sends the next message back to planning.
fn exit_plan_options(input: &Value) -> Vec<PermissionOption> {
    let plan = input.get("plan").and_then(Value::as_str).unwrap_or_default();
    let mut options = vec![PermissionOption::new(
        PermissionOptionId::new("exit-plan-bypass"),
        "Yes",
        PermissionOptionKind::AllowAlways,
    )];
    if !plan.trim().is_empty() {
        options.push(PermissionOption::new(
            PermissionOptionId::new("exit-plan-clear-bypass"),
            "Yes, and start with a fresh context",
            PermissionOptionKind::AllowAlways,
        ));
    }
    options.push(
        PermissionOption::new(
            PermissionOptionId::new("reject"),
            "No, keep planning",
            PermissionOptionKind::RejectOnce,
        )
        .meta(option_description("Sends your next message back to planning")),
    );
    options
}

/// ACP's `PermissionOption` has no description of its own; the mapper reads
/// the option's second line from `_meta` under
/// [`PERMISSION_OPTION_DESCRIPTION_META`] and publishes it as
/// `QuestionOption.description`.
fn option_description(text: &str) -> Map<String, Value> {
    let mut meta = Map::new();
    meta.insert(
        PERMISSION_OPTION_DESCRIPTION_META.to_string(),
        Value::String(text.to_string()),
    );
    meta
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

/// EXP-905: the incremental title reader over ONE claude transcript. Every
/// poll reads only the bytes appended since the last one (a transcript runs
/// to megabytes; a per-turn re-read would be quadratic), parses only the
/// lines that can carry a name, and reports the best name when it changed.
#[derive(Default)]
struct TitleTail {
    /// The native session id the tail follows: a `/clear` moves claude to a
    /// fresh transcript, and the tail starts over on it.
    native: Option<String>,
    path: Option<PathBuf>,
    /// Bytes consumed so far — always at a line boundary.
    offset: u64,
    /// The latest `ai-title` and `custom-title` seen.
    ai: Option<String>,
    custom: Option<String>,
    /// The name last reported, so an unchanged one reports nothing.
    published: Option<String>,
}

impl TitleTail {
    /// `locate` resolves the transcript lazily (claude creates it only once
    /// the conversation has content). Returns the name to publish, if it
    /// changed.
    fn poll(&mut self, native: &str, locate: impl FnOnce() -> Option<PathBuf>) -> Option<String> {
        if self.native.as_deref() != Some(native) {
            self.native = Some(native.to_string());
            self.path = None;
            self.offset = 0;
            self.ai = None;
            self.custom = None;
        }
        if self.path.is_none() {
            self.path = locate();
        }
        let path = self.path.clone()?;
        if let Err(error) = self.read_from(&path) {
            log::debug!("engine: claude transcript title read failed: {error}");
        }
        self.publish()
    }

    /// EXP-1134: the name `generate_session_title` answered with, taken as
    /// the conversation's `ai-title` before claude's own transcript line for
    /// it lands (the tail reads that line later as the same name). Returns
    /// the name to publish, if it changed.
    fn adopt(&mut self, title: String) -> Option<String> {
        self.ai = Some(title);
        self.publish()
    }

    fn publish(&mut self) -> Option<String> {
        // A `/rename` is the user's word and outranks the model's guess.
        let best = self.custom.clone().or_else(|| self.ai.clone())?;
        if self.published.as_deref() == Some(best.as_str()) {
            return None;
        }
        self.published = Some(best.clone());
        Some(best)
    }

    fn read_from(&mut self, path: &Path) -> std::io::Result<()> {
        use std::io::{BufRead, Seek, SeekFrom};
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            // Rewritten shorter underneath us: start over.
            self.offset = 0;
            self.ai = None;
            self.custom = None;
        }
        if len == self.offset {
            return Ok(());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut reader = std::io::BufReader::new(file);
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            // EOF, or a line claude is still writing: consume it next time.
            if read == 0 || line.last() != Some(&b'\n') {
                break;
            }
            self.offset += read as u64;
            self.note_line(&line);
        }
        Ok(())
    }

    fn note_line(&mut self, line: &[u8]) {
        const AI: &[u8] = b"\"ai-title\"";
        const CUSTOM: &[u8] = b"\"custom-title\"";
        let has = |needle: &[u8]| line.windows(needle.len()).any(|window| window == needle);
        // Cheap byte pre-filter: almost every line is a message, and only a
        // title line is worth a JSON parse.
        if !has(AI) && !has(CUSTOM) {
            return;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else { return };
        let (slot, key) = match value.get("type").and_then(Value::as_str) {
            Some("ai-title") => (&mut self.ai, "aiTitle"),
            Some("custom-title") => (&mut self.custom, "customTitle"),
            _ => return,
        };
        if let Some(title) = value
            .get(key)
            .and_then(Value::as_str)
            .and_then(steer::normalize_agent_title)
        {
            *slot = Some(title);
        }
    }
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

/// EXP-1051: the tokens ONE request carried into the model — input, cache
/// reads and cache creation, and deliberately NOT `output_tokens`, which is
/// what the request produced rather than what it sent. `TokenSnapshot::used()`
/// sums all four because context OCCUPANCY includes the answer; the context
/// bar's base is about the prefix alone.
fn prefix_tokens(usage: &Value) -> u64 {
    let field = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    field("input_tokens") + field("cache_read_input_tokens") + field("cache_creation_input_tokens")
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
            title: Some("Audit the shape proxies".to_string()),
            live,
            backgrounded: true,
            turn_seq,
            started_at: Instant::now() - age,
            last_status: None,
            workflow_id: None,
            silent: false,
        }
    }

    /// EXP-847: the subagent chip's title is the spawning call's own
    /// `description`, its `name` second, nothing when it named neither.
    fn title_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp905-title-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn append(path: &Path, text: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    /// EXP-905: the LATEST `ai-title` names the conversation (claude
    /// re-appends it as the conversation evolves), a `custom-title` outranks
    /// it, and an unchanged name reports nothing.
    #[test]
    fn the_latest_ai_title_wins_and_a_custom_title_outranks_it() {
        let dir = title_dir("latest");
        let path = dir.join("s1.jsonl");
        let body = [
            json!({ "type": "user", "message": { "role": "user", "content": "the \"ai-title\" word in a prompt" } }),
            json!({ "type": "ai-title", "aiTitle": "First  guess", "sessionId": "s1" }),
            json!({ "type": "assistant", "message": { "id": "m1", "content": [] } }),
            json!({ "type": "ai-title", "aiTitle": "Better\nguess", "sessionId": "s1" }),
        ]
        .iter()
        .map(|line| format!("{line}\n"))
        .collect::<String>();
        append(&path, &body);
        let mut tail = TitleTail::default();
        assert_eq!(tail.poll("s1", || Some(path.clone())).as_deref(), Some("Better guess"));
        assert_eq!(tail.poll("s1", || Some(path.clone())), None, "unchanged");

        append(&path, &format!("{}\n", json!({ "type": "custom-title", "customTitle": "Mine", "sessionId": "s1" })));
        assert_eq!(tail.poll("s1", || Some(path.clone())).as_deref(), Some("Mine"));
        // A later model guess never overrides the user's own name.
        append(&path, &format!("{}\n", json!({ "type": "ai-title", "aiTitle": "Third guess", "sessionId": "s1" })));
        assert_eq!(tail.poll("s1", || Some(path.clone())), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// EXP-1134: an asked-for name publishes at once, the transcript's own
    /// line for it then reads as no change, and a `/rename` still outranks it.
    #[test]
    fn an_adopted_title_publishes_once_and_yields_to_a_rename() {
        let dir = title_dir("adopt");
        let path = dir.join("s1.jsonl");
        append(&path, &format!("{}\n", json!({ "type": "user", "message": { "content": "hi" } })));
        let mut tail = TitleTail::default();
        assert_eq!(tail.poll("s1", || Some(path.clone())), None);
        assert_eq!(tail.adopt("Fix login".into()).as_deref(), Some("Fix login"));
        append(&path, &format!("{}\n", json!({ "type": "ai-title", "aiTitle": "Fix login", "sessionId": "s1" })));
        assert_eq!(tail.poll("s1", || unreachable!()), None);
        append(&path, &format!("{}\n", json!({ "type": "custom-title", "customTitle": "Mine", "sessionId": "s1" })));
        assert_eq!(tail.poll("s1", || unreachable!()).as_deref(), Some("Mine"));
        assert_eq!(tail.adopt("Later guess".into()), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// EXP-905: each poll reads only what was appended, never consumes a
    /// half-written line, and starts over on a `/clear`'s new transcript.
    #[test]
    fn the_title_tail_reads_incrementally_from_its_offset() {
        let dir = title_dir("offset");
        let path = dir.join("s1.jsonl");
        let mut tail = TitleTail::default();
        // No transcript yet: nothing, and the locator is retried next poll.
        assert_eq!(tail.poll("s1", || None), None);

        let first = format!("{}\n", json!({ "type": "user", "message": { "content": "hi" } }));
        append(&path, &first);
        assert_eq!(tail.poll("s1", || Some(path.clone())), None);
        assert_eq!(tail.offset, first.len() as u64);

        // A title line claude is still writing: not consumed yet.
        let line = json!({ "type": "ai-title", "aiTitle": "Split write", "sessionId": "s1" }).to_string();
        let (head, rest) = line.split_at(20);
        append(&path, head);
        assert_eq!(tail.poll("s1", || unreachable!("the path is cached")), None);
        assert_eq!(tail.offset, first.len() as u64);
        append(&path, &format!("{rest}\n"));
        assert_eq!(tail.poll("s1", || unreachable!()).as_deref(), Some("Split write"));
        assert_eq!(tail.offset, std::fs::metadata(&path).unwrap().len());

        // Bytes before the offset are never re-read: a title planted there
        // (by rewriting the prefix in place, same length) is not seen.
        let mut bytes = std::fs::read(&path).unwrap();
        let planted = format!("{}\n", json!({ "type": "ai-title", "aiTitle": "Zz" }));
        assert!(planted.len() <= first.len());
        let padded = format!("{}{}\n", &planted[..planted.len() - 1], " ".repeat(first.len() - planted.len()));
        bytes.splice(0..first.len(), padded.bytes());
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(tail.poll("s1", || unreachable!()), None);

        // `/clear`: a new native id follows its own transcript from zero.
        let other = dir.join("s2.jsonl");
        append(&other, &format!("{}\n", json!({ "type": "ai-title", "aiTitle": "Fresh start" })));
        assert_eq!(tail.poll("s2", || Some(other.clone())).as_deref(), Some("Fresh start"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_task_title_prefers_the_description() {
        assert_eq!(
            task_title(&json!({ "description": "  Audit the shape proxies  ", "name": "explore" })),
            Some("Audit the shape proxies".to_string())
        );
        assert_eq!(
            task_title(&json!({ "description": "   ", "name": "explore" })),
            Some("explore".to_string())
        );
        assert_eq!(task_title(&json!({ "prompt": "do it" })), None);
        assert_eq!(task_title(&Value::Null), None);
    }

    /// EXP-847: `agentType: "agent"` (what the CLI reports for a teammate
    /// spawn) says nothing — the spawning call's own `subagent_type`/`name`
    /// takes over, and only a call that named nothing either keeps the
    /// generic word.
    #[test]
    fn a_generic_agent_type_defers_to_the_spawning_call() {
        let input = json!({ "subagent_type": "explore", "name": "scout" });
        assert_eq!(
            task_agent_type(Some("agent"), Some(&input)),
            Some("explore".to_string())
        );
        assert_eq!(
            task_agent_type(Some("  "), Some(&json!({ "name": "scout" }))),
            Some("scout".to_string())
        );
        // A SPECIFIC report always wins — the CLI knows the type best.
        assert_eq!(
            task_agent_type(Some("general-purpose"), Some(&input)),
            Some("general-purpose".to_string())
        );
        // Nothing named anywhere: the generic word is all there is.
        assert_eq!(
            task_agent_type(Some("agent"), Some(&json!({ "prompt": "do it" }))),
            Some("agent".to_string())
        );
        assert_eq!(task_agent_type(None, None), None);
    }

    /// EXP-850 §3 review — a card called `""` is dropped by web, iOS and
    /// Android, so a `task_started` without a `workflow_name` used to show no
    /// card at all. The description is the fallback, the word `workflow` the
    /// floor.
    #[test]
    fn a_workflow_card_is_never_nameless() {
        assert_eq!(
            workflow_card_name(Some("Nightly sweep"), Some("Probe the wire")),
            "Nightly sweep"
        );
        assert_eq!(workflow_card_name(Some("  "), Some("Probe the wire")), "Probe the wire");
        assert_eq!(workflow_card_name(None, Some("Probe the wire")), "Probe the wire");
        assert_eq!(workflow_card_name(Some(""), Some("   ")), "workflow");
        assert_eq!(workflow_card_name(None, None), "workflow");
    }

    /// EXP-850 §3 review — the adapter's card map is capped like the journal
    /// and the relay room, and an evicted card takes its `task_id` mappings
    /// with it. Without this a run that starts workflows all day grows three
    /// maps for the life of the process.
    #[test]
    fn the_workflow_map_is_capped_and_evicts_its_id_maps() {
        let mut state = State::default();
        let total = steer::journal::JOURNAL_WORKFLOW_CAP + 4;
        for index in 0..total {
            let id = format!("toolu_{index}");
            let task_id = format!("task-{index}");
            state.workflows.push(WorkflowRun {
                id: id.clone(),
                name: "wire-probe".to_string(),
                description: None,
                status: steer::WorkflowStatus::Running,
                phases: BTreeMap::new(),
                agents: BTreeMap::new(),
                summary: None,
                published_at: None,
                lanes: BTreeMap::new(),
            });
            state.workflow_of_task.insert(task_id.clone(), id.clone());
            state.task_tool_ids.insert(task_id, id);
            evict_workflows(&mut state);
        }

        assert_eq!(state.workflows.len(), steer::journal::JOURNAL_WORKFLOW_CAP);
        // The NEWEST cards survive, in first-appearance order.
        assert_eq!(state.workflows.first().map(|run| run.id.as_str()), Some("toolu_4"));
        assert_eq!(
            state.workflows.last().map(|run| run.id.as_str()),
            Some(format!("toolu_{}", total - 1).as_str())
        );
        // …and the dropped cards left no id mappings behind.
        assert_eq!(state.workflow_of_task.len(), steer::journal::JOURNAL_WORKFLOW_CAP);
        assert_eq!(state.task_tool_ids.len(), steer::journal::JOURNAL_WORKFLOW_CAP);
        assert!(!state.workflow_of_task.contains_key("task-0"));
        assert!(!state.task_tool_ids.contains_key("task-0"));
        assert_eq!(state.workflow_of_task.get("task-4").map(String::as_str), Some("toolu_4"));
    }

    fn lane_agent(state: steer::WorkflowAgentState) -> steer::WorkflowAgent {
        steer::WorkflowAgent {
            index: 1,
            label: "alpha:one".to_string(),
            agent_id: Some("a37045b3fb76c076a".to_string()),
            state,
            ..steer::WorkflowAgent::default()
        }
    }

    fn step_names(steps: &[LaneStep]) -> Vec<String> {
        steps
            .iter()
            .map(|step| match step {
                LaneStep::Started { .. } => "started".to_string(),
                LaneStep::Tool { name, summary, .. } => format!("tool:{name}:{summary}"),
                LaneStep::Narration { text, .. } => format!("say:{text}"),
                LaneStep::Finished { status, .. } => format!("end:{status}"),
            })
            .collect()
    }

    /// EXP-1225: a workflow agent's lane, step by step off its progress
    /// entries — nothing while queued, the edge once it runs, ONE row per
    /// distinct last tool, the preview and the end once done, and nothing
    /// after that.
    #[test]
    fn a_workflow_agents_lane_follows_its_progress_entries() {
        let mut lane = AgentLane::default();
        let mut queued = lane_agent(steer::WorkflowAgentState::Queued);
        queued.agent_id = None;
        assert!(lane_steps("wf", &queued, &mut lane).is_empty());

        let running = lane_agent(steer::WorkflowAgentState::Running);
        assert_eq!(step_names(&lane_steps("wf", &running, &mut lane)), vec!["started"]);
        // The same entry again says nothing.
        assert!(lane_steps("wf", &running, &mut lane).is_empty());

        let mut on_bash = running.clone();
        on_bash.last_tool = Some("Bash".to_string());
        on_bash.last_tool_summary = Some("echo one".to_string());
        assert_eq!(
            step_names(&lane_steps("wf", &on_bash, &mut lane)),
            vec!["tool:Bash:echo one"]
        );
        assert!(lane_steps("wf", &on_bash, &mut lane).is_empty(), "one row per distinct call");

        let mut done = on_bash.clone();
        done.state = steer::WorkflowAgentState::Done;
        done.result_preview = Some("ok".to_string());
        done.tool_calls = Some(1);
        let steps = lane_steps("wf", &done, &mut lane);
        assert_eq!(step_names(&steps), vec!["say:ok", "end:completed"]);
        assert!(matches!(
            steps.last(),
            Some(LaneStep::Finished { tool_calls: Some(1), title, workflow_id, .. })
                if title == "alpha:one" && workflow_id == "wf"
        ));
        assert!(lane_steps("wf", &done, &mut lane).is_empty(), "a finished lane is closed");

        // Straight from queued to error, in one frame: the whole life.
        let mut lane = AgentLane::default();
        let mut failed = lane_agent(steer::WorkflowAgentState::Error);
        failed.error = Some("panicked".to_string());
        assert_eq!(
            step_names(&lane_steps("wf", &failed, &mut lane)),
            vec!["started", "say:panicked", "end:failed"]
        );
    }

    /// EXP-1225: the synthesized row reads as the call it stands for.
    #[test]
    fn a_lane_row_is_titled_like_the_tool_it_names() {
        let cwd = PathBuf::from("/work/tree");
        let bash = tool_info("Bash", &lane_tool_input("Bash", "echo one"), &cwd);
        assert_eq!(bash.kind, ToolKind::Execute);
        assert_eq!(bash.title, "echo one");
        let read = tool_info("Read", &lane_tool_input("Read", "/work/tree/src/lib.rs"), &cwd);
        assert_eq!(read.kind, ToolKind::Read);
        assert_eq!(read.locations.len(), 1);
        let other = tool_info("Frobnicate", &lane_tool_input("Frobnicate", "it"), &cwd);
        assert_eq!(other.kind, ToolKind::Other);
        assert_eq!(other.title, "Frobnicate");
    }

    /// EXP-1225: a workflow that ends with lanes still open (stopped, killed)
    /// closes them, so no agent tab spins for the rest of the run.
    #[test]
    fn a_stopped_workflow_closes_its_open_lanes() {
        let mut run = WorkflowRun {
            id: "wf".to_string(),
            name: "wire-probe".to_string(),
            description: None,
            status: steer::WorkflowStatus::Stopped,
            phases: BTreeMap::new(),
            agents: BTreeMap::new(),
            summary: None,
            published_at: None,
            lanes: BTreeMap::new(),
        };
        let running = lane_agent(steer::WorkflowAgentState::Running);
        let mut lane = AgentLane::default();
        lane_steps("wf", &running, &mut lane);
        run.agents.insert(1, running);
        run.lanes.insert(1, lane);
        let steps = close_lanes(&mut run);
        assert_eq!(step_names(&steps), vec!["end:failed"]);
        assert!(close_lanes(&mut run).is_empty(), "closed once");
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

    /// EXP-927: a lane that outlives the lifetime keeps its tab for as long as
    /// the CLI's own list names it — it stops DEFERRING the settle, nothing
    /// more — and is retired the moment the list drops it.
    #[test]
    fn an_overdue_task_the_cli_still_lists_is_not_retired() {
        let mut state = State { turn_seq: 4, ..State::default() };
        let overdue = TASK_MAX_LIFETIME + Duration::from_secs(1);
        state.tasks.insert("lane".to_string(), task(4, true, overdue));
        state.tasks.insert("lost".to_string(), task(4, true, overdue));
        state.tasks.insert("young".to_string(), task(4, true, Duration::ZERO));
        state.background_tasks = vec![steer::BackgroundTask {
            id: "lane".to_string(),
            kind: steer::BackgroundTaskKind::Agent,
            description: "web lane".to_string(),
            tool_id: None,
        }];
        assert_eq!(expired_tasks(&state), vec!["lost".to_string()]);
        // Neither overdue task defers the settle any more.
        assert_eq!(blocking_tasks(&state).count(), 1);

        state.background_tasks.clear();
        let mut expired = expired_tasks(&state);
        expired.sort();
        assert_eq!(expired, vec!["lane".to_string(), "lost".to_string()]);
    }

    /// The defer timer is sized for the LAST blocking task to expire: an
    /// older task's remaining lifetime is shorter, and a timer sized for it
    /// would wake to find the younger one still blocking and go back to
    /// sleep — which is why the wakeup re-arms when a deferral survives it.
    #[test]
    fn the_defer_timer_waits_for_the_youngest_blocking_task() {
        let mut state = State { turn_seq: 2, ..State::default() };
        state.tasks.insert("old".to_string(), task(2, true, Duration::from_secs(500)));
        state.tasks.insert("young".to_string(), task(2, true, Duration::from_secs(20)));
        // An earlier turn's task and a dead one never count.
        state.tasks.insert("other-turn".to_string(), task(1, true, Duration::ZERO));
        state.tasks.insert("done".to_string(), task(2, false, Duration::ZERO));

        let wait = defer_timer_wait(&state);
        // 600 - 20 (+1 s), give or take the test's own elapsed time.
        assert!(wait >= Duration::from_secs(579) && wait <= Duration::from_secs(582), "{wait:?}");

        // Nothing blocking: a full lifetime, not a zero-length spin.
        state.tasks.clear();
        assert_eq!(defer_timer_wait(&state), TASK_MAX_LIFETIME + Duration::from_secs(1));
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

    fn first_text(fields: &ToolCallUpdateFields) -> String {
        match fields.content.as_ref().and_then(|content| content.first()) {
            Some(ToolCallContent::Content(content)) => match &content.content {
                ContentBlock::Text(text) => text.text.clone(),
                other => panic!("expected text, got {other:?}"),
            },
            other => panic!("expected content, got {other:?}"),
        }
    }

    /// EXP-1202: an MCP answer settles with its JSON as `raw_output` (the
    /// server's `{content:[…]}` envelope, what the mapper reads a preview
    /// off) AND its text as content, which the transcript shows.
    #[test]
    fn an_mcp_result_carries_its_answer_as_raw_output() {
        let answer = r#"{"id":"7c0b9f3e-2d4a-4e8b-9a61-3f5d2c1b0a99","topic":"Progress","label":"web"}"#;
        let block = json!({
            "tool_use_id": "t1",
            "content": [{ "type": "text", "text": answer }],
        });
        let fields = tool_result_fields(
            "mcp__exponential__exponential_sessions_show",
            &json!({ "file": "/work/tree/shot.png", "text": "The header" }),
            &block,
            None,
            false,
        );
        assert_eq!(
            fields.raw_output,
            Some(json!({ "content": [{ "type": "text", "text": answer }] }))
        );
        assert_eq!(first_text(&fields), answer);
        let payload = crate::exp_tool_refs::tool_result_payload(
            fields.raw_output.as_ref().expect("an answer"),
        );
        assert_eq!(
            payload.and_then(|payload| payload.get("id").cloned()),
            Some(json!("7c0b9f3e-2d4a-4e8b-9a61-3f5d2c1b0a99"))
        );

        // The message-level structured result wins when it is one, and a
        // bare string answer stays a string.
        let structured = json!({ "id": "i-1", "identifier": "EXP-1" });
        let fields = tool_result_fields(
            "mcp__exponential__exponential_issues_get",
            &Value::Null,
            &block,
            Some(&structured),
            false,
        );
        assert_eq!(fields.raw_output, Some(structured));
        let string = json!({ "tool_use_id": "t1", "content": answer });
        let fields = tool_result_fields(
            "mcp__exponential__exponential_sessions_show",
            &Value::Null,
            &string,
            None,
            false,
        );
        assert_eq!(fields.raw_output, Some(json!(answer)));

        // Only MCP calls: a built-in tool's answer is no preview source.
        let fields = tool_result_fields("Grep", &Value::Null, &block, None, false);
        assert!(fields.raw_output.is_none());
    }

    /// EXP-1202: a failed MCP call has no answer, so no `raw_output` and no
    /// preview — even when its text parses as a row.
    #[test]
    fn a_failed_mcp_result_has_no_raw_output() {
        let block = json!({
            "tool_use_id": "t1",
            "is_error": true,
            "content": [{ "type": "text", "text": r#"{"id":"i-1","identifier":"EXP-404"}"# }],
        });
        let fields = tool_result_fields(
            "mcp__exponential__exponential_issues_get",
            &Value::Null,
            &block,
            Some(&json!({ "id": "i-1" })),
            true,
        );
        assert_eq!(fields.status, Some(ToolCallStatus::Failed));
        assert!(fields.raw_output.is_none());
        assert!(first_text(&fields).contains("EXP-404"), "the error still renders");
    }

    /// EXP-1202: a `Bash` settle is the bare output — no markdown fence, which
    /// would otherwise open and close the published command output.
    #[test]
    fn a_bash_result_is_the_bare_output() {
        let block = json!({ "tool_use_id": "t1", "content": "fg-done" });
        let structured = json!({ "stdout": "fg-done\n", "stderr": "", "interrupted": false });
        let fields = tool_result_fields(
            "Bash",
            &json!({ "command": "echo fg-done", "description": "Print the marker" }),
            &block,
            Some(&structured),
            false,
        );
        assert_eq!(first_text(&fields), "fg-done");
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
    /// one — every answer lands on `bypassPermissions`. EXP-788: the plain
    /// "Yes" is index 0 (the primary on every client), the reject is last and
    /// explains itself.
    #[test]
    fn the_plan_menu_offers_the_plain_yes_first() {
        let options = exit_plan_options(&json!({ "plan": "# Plan" }));
        let ids: Vec<String> =
            options.iter().map(|option| option.option_id.0.to_string()).collect();
        assert_eq!(
            ids,
            vec![
                "exit-plan-bypass".to_string(),
                "exit-plan-clear-bypass".to_string(),
                "reject".to_string(),
            ]
        );
        let labels: Vec<&str> = options.iter().map(|option| option.name.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Yes", "Yes, and start with a fresh context", "No, keep planning"]
        );
        let reject = options.last().expect("the reject option");
        assert_eq!(
            reject
                .meta
                .as_ref()
                .and_then(|meta| meta.get(PERMISSION_OPTION_DESCRIPTION_META))
                .and_then(Value::as_str),
            Some("Sends your next message back to planning")
        );
        assert!(options[0].meta.is_none());
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

    /// EXP-1051: the base is measured off what a request SENT — the output
    /// it is about to produce is not part of the prefix, which is the whole
    /// difference between this and `TokenSnapshot::used()`.
    #[test]
    fn the_prefix_is_the_input_halves_and_never_the_output() {
        let usage = json!({
            "input_tokens": 2,
            "cache_creation_input_tokens": 6_721,
            "cache_read_input_tokens": 11_474,
            "output_tokens": 4_000,
        });
        assert_eq!(prefix_tokens(&usage), 2 + 6_721 + 11_474);
        assert_eq!(
            wire::TokenSnapshot {
                input: 2,
                cache_creation: 6_721,
                cache_read: 11_474,
                output: 4_000,
            }
            .used(),
            prefix_tokens(&usage) + 4_000,
            "the occupancy adds the answer; the prefix does not"
        );
    }

    /// A frame with no usage object at all is not a measurement of zero — the
    /// caller's `prefix > 0` guard is what keeps it off the slot.
    #[test]
    fn a_missing_usage_measures_nothing() {
        assert_eq!(prefix_tokens(&Value::Null), 0);
        assert_eq!(prefix_tokens(&json!({})), 0);
        assert_eq!(prefix_tokens(&json!({ "output_tokens": 900 })), 0);
    }
}
