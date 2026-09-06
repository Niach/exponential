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
use std::time::Duration;

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

/// `_meta` key carrying a subagent edge. ACP v1 has no subagent update, and
/// the relay's `subagent` card predates ACP by a year, so the adapter stamps
/// the edge onto a no-op `ToolCallUpdate` for the spawning tool call and the
/// engine's mapper reads it back out. Shape:
/// `{"id": <task id>, "agentType": <subagent type>, "status": started|completed|failed}`.
pub const SUBAGENT_META_KEY: &str = "exp/subagent";

/// `_meta` key carrying the tool call a message or tool call belongs to, when
/// it belongs to a subagent's nested run (claude's `parent_tool_use_id`).
pub const PARENT_TOOL_CALL_META_KEY: &str = "exp/parentToolCallId";

/// The subagents the CLI ships. They are spawned by the model, never picked
/// for the main thread, so the `agent` option offers only what the user (or a
/// plugin) configured.
const BUILTIN_AGENT_NAMES: [&str; 5] =
    ["claude", "general-purpose", "Explore", "Plan", "statusline-setup"];

/// Config option ids. `mode` is NOT among the options the adapter advertises —
/// modes ride the ACP-native `SessionModeState`/`session/set_mode` lane, and
/// advertising both would render two mode chips on every client. It is still
/// ACCEPTED by `session/set_config_option` so a client that only knows config
/// options can steer the mode.
const CONFIG_MODEL: &str = "model";
const CONFIG_EFFORT: &str = "effort";
const CONFIG_FAST: &str = "fast";
const CONFIG_AGENT: &str = "agent";
const CONFIG_MODE: &str = "mode";

/// The value that means "whatever the CLI would pick" for effort and agent.
const CONFIG_DEFAULT_VALUE: &str = "default";

/// How long `session/new` waits for the `initialize` control response before
/// giving up on it and seeding the session from `system/init` alone. The
/// spike measured 0.6-1.8 s normally and ~25 s with an unreachable MCP
/// endpoint (the CLI waits on the MCP handshake first), so the budget is
/// generous on purpose: a slow init is not a dead CLI.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(90);

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
                    async move |_request: NewSessionRequest, responder, cx: ConnectionTo<Client>| {
                        // Spawn + handshake takes seconds; a handler that waits
                        // for it inline blocks every further message on the
                        // connection, `$/cancel_request` included.
                        let session = on_new.clone();
                        let spawned = cx.clone();
                        cx.spawn(async move {
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
                        // The turn outlives this handler by design: `Cancel`
                        // has to be dispatchable while it runs.
                        cx.spawn(async move {
                            match session.prompt(&spawned, request).await {
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
                .connect_to(client)
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
    state: Mutex<State>,
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
    /// One settle channel per in-flight `session/prompt`, oldest first: claude
    /// emits one `result` per turn, so the front of the queue owns the next.
    turns: VecDeque<flume::Sender<TurnOutcome>>,
    /// An outcome held back while background subagents of that turn are still
    /// live — settling at the `result` would strand their permission requests
    /// on an RPC nobody answers.
    deferred: Option<TurnOutcome>,
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
    context_size: u64,
    compaction: Option<String>,
    tasks: HashMap<String, TaskEntry>,
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
    closed: bool,
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
        ClaudeSession { spec, session_id: SessionId::new(session_id), state: Mutex::new(state) }
    }

    fn cwd(&self) -> &Path {
        &self.spec.cwd
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
    async fn start(&self, cx: &ConnectionTo<Client>, resume: Option<&str>) -> Result<(), Error> {
        if self.lock().child.is_some() {
            return Ok(());
        }
        let child = Arc::new(self.spawn_child(resume).map_err(|error| {
            Error::internal_error()
                .data(json!({ "reason": format!("could not start claude: {error}") }))
        })?);
        {
            let mut state = self.lock();
            state.child = Some(child.clone());
            state.closed = false;
        }
        let lines = child.lines.clone();
        let pump_cx = cx.clone();
        let pump = self.clone_arc();
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

    /// A second `Arc` to the same session. The handlers hold one each, so the
    /// pump can be handed one without threading it through every call.
    fn clone_arc(&self) -> Arc<ClaudeSession> {
        // Safety of the pattern, not of memory: `ClaudeSession` is only ever
        // constructed inside an `Arc` in `connect_to`, and every method that
        // needs a second handle is reached through it.
        unsafe { Arc::increment_strong_count(self as *const ClaudeSession) };
        unsafe { Arc::from_raw(self as *const ClaudeSession) }
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
            allow_dangerous: mode == "bypassPermissions",
            session_id: Some(self.session_id.0.as_ref()),
            resume,
            fork_session: false,
            mcp_config: mcp,
            strict_mcp_config: mcp.is_some(),
            settings: self.spec.reaper_settings_path.as_deref(),
            add_dirs: &[],
            disallowed_tools: disallowed,
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
        let response = match tokio::time::timeout(INITIALIZE_TIMEOUT, rx.recv_async()).await {
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
    ) -> Result<PromptResponse, Error> {
        let text = prompt_text(&request.prompt);
        // `/usage` is answered from the `get_usage` control request instead of
        // a turn; `get_context_usage` is never sent at all (it stalls ~15 s
        // before the first turn and serializes ahead of an awaited set_model).
        if wire::is_usage_command(&text) {
            self.start(cx, None).await?;
            let usage = self.control_request(wire::get_usage_without_behaviors()).await?;
            self.notify(
                cx,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(wire::render_usage_markdown(&usage.response)),
                ))),
            );
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }

        // A child spawned lazily here is the `session/load` → prompt path: a
        // replay that the user decided to continue.
        let resume = self.lock().child.is_none().then(|| self.session_id.0.to_string());
        self.start(cx, resume.as_deref()).await?;

        let (tx, rx) = flume::bounded(1);
        {
            let mut state = self.lock();
            state.cancelled = false;
            state.delivered_text = false;
            state.local_only_command = wire::LOCAL_ONLY_COMMANDS
                .iter()
                .any(|command| text.trim() == *command);
            state.turns.push_back(tx);
        }
        self.send(claude_user_message(&request.prompt, &text))?;
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

    fn settle(&self, state: &mut State, outcome: TurnOutcome) {
        if let Some(turn) = state.turns.pop_front() {
            let _ = turn.send(outcome);
        }
    }

    /// Settle, unless a background subagent this turn spawned is still live
    /// (issues #864/#866: settling early strands its permission request).
    fn settle_or_defer(&self, state: &mut State, outcome: TurnOutcome) {
        if state.tasks.values().any(|task| task.live) {
            state.deferred = Some(outcome);
        } else {
            self.settle(state, outcome);
        }
    }

    fn settle_deferred(&self, state: &mut State) {
        if state.tasks.values().any(|task| task.live) {
            return;
        }
        if let Some(outcome) = state.deferred.take() {
            self.settle(state, outcome);
        }
    }

    // -----------------------------------------------------------------------
    // modes and config options
    // -----------------------------------------------------------------------

    fn mode_state(&self) -> SessionModeState {
        let current = self.lock().mode.clone();
        SessionModeState::new(SessionModeId::new(current), available_modes())
    }

    async fn set_mode(self: &Arc<Self>, cx: &ConnectionTo<Client>, mode: &str) -> Result<(), Error> {
        let mode = self.clamp_mode(cx, mode);
        self.control_request(wire::set_permission_mode(&mode)).await?;
        self.lock().mode = mode.clone();
        self.notify(
            cx,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(mode))),
        );
        self.publish_config(cx);
        Ok(())
    }

    /// `auto` needs model support; the upstream falls back to `acceptEdits`
    /// and says so once, which is better than a mode the model ignores.
    fn clamp_mode(self: &Arc<Self>, cx: &ConnectionTo<Client>, mode: &str) -> String {
        if mode != "auto" {
            return mode.to_string();
        }
        let state = self.lock();
        let supported = state
            .models
            .iter()
            .find(|model| model.value == state.model)
            .map(|model| model.supports_auto_mode)
            // No model list (a CLI that answered no initialize) — trust the pick.
            .unwrap_or(true);
        drop(state);
        if supported {
            return mode.to_string();
        }
        self.notify(
            cx,
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(
                    "**Auto mode unavailable:** this model does not support it, so the session \
                     stays on Accept edits.",
                ),
            ))),
        );
        "acceptEdits".to_string()
    }

    fn config_options(&self) -> Vec<SessionConfigOption> {
        let state = self.lock();
        let mut options = Vec::new();

        let model_options: Vec<SessionConfigSelectOption> = if state.models.is_empty() {
            vec![SessionConfigSelectOption::new(
                state.model.clone(),
                display_model(&state.model),
            )]
        } else {
            state
                .models
                .iter()
                .map(|model| {
                    SessionConfigSelectOption::new(model.value.clone(), model.display_name.clone())
                        .description((!model.description.is_empty())
                            .then(|| model.description.clone()))
                })
                .collect()
        };
        options.push(
            SessionConfigOption::select(CONFIG_MODEL, "Model", state.model.clone(), model_options)
                .category(SessionConfigOptionCategory::Model),
        );

        let current_model = state.models.iter().find(|model| model.value == state.model);
        let effort_levels: Vec<String> = match current_model {
            Some(model) if model.supports_effort && !model.supported_effort_levels.is_empty() => {
                model.supported_effort_levels.clone()
            }
            // Before the model list arrives (or on a CLI that reports none) the
            // contract's own effort vocabulary keeps the chip usable.
            _ => coding::CodingAgent::Claude
                .effort_values()
                .iter()
                .map(|level| (*level).to_string())
                .collect(),
        };
        let mut effort_options = vec![SessionConfigSelectOption::new(
            CONFIG_DEFAULT_VALUE,
            "CLI default",
        )];
        effort_options.extend(
            effort_levels
                .iter()
                .map(|level| SessionConfigSelectOption::new(level.clone(), title_case(level))),
        );
        if state.ultracode {
            effort_options.push(SessionConfigSelectOption::new("ultracode", "Ultracode"));
        }
        let current_effort = if state.ultracode && state.effort.is_none() {
            "ultracode".to_string()
        } else {
            state.effort.clone().unwrap_or_else(|| CONFIG_DEFAULT_VALUE.to_string())
        };
        options.push(
            SessionConfigOption::select(CONFIG_EFFORT, "Effort", current_effort, effort_options)
                .category(SessionConfigOptionCategory::ThoughtLevel),
        );

        if state.fast_supported
            && current_model.map(|model| model.supports_fast_mode).unwrap_or(false)
        {
            options.push(
                SessionConfigOption::boolean(CONFIG_FAST, "Fast", state.fast)
                    .category(SessionConfigOptionCategory::ModelConfig),
            );
        }

        if !state.custom_agents.is_empty() {
            let mut agent_options = vec![SessionConfigSelectOption::new(
                CONFIG_DEFAULT_VALUE,
                "CLI default",
            )];
            agent_options.extend(
                state
                    .custom_agents
                    .iter()
                    .map(|agent| SessionConfigSelectOption::new(agent.clone(), agent.clone())),
            );
            let current =
                state.agent.clone().unwrap_or_else(|| CONFIG_DEFAULT_VALUE.to_string());
            options.push(
                SessionConfigOption::select(CONFIG_AGENT, "Agent", current, agent_options)
                    .category(SessionConfigOptionCategory::Other("agent".to_string())),
            );
        }
        options
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
            // Not advertised (modes ride `session/set_mode`), but accepted so
            // a client that only speaks config options can still steer.
            CONFIG_MODE => {
                let mode = picked.unwrap_or_else(|| "default".to_string());
                self.set_mode(cx, &mode).await?;
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
        let size = self.lock().context_size;
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
        let mut meta = Map::new();
        meta.insert(
            SUBAGENT_META_KEY.to_string(),
            json!({ "id": task_id, "agentType": agent_type, "status": status }),
        );
        // The edge rides a no-op patch of the tool call that spawned the
        // subagent, so a client that ignores the meta sees nothing at all.
        let id = tool_use_id.unwrap_or(task_id);
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
        let mut state = self.lock();
        state.closed = true;
        let outcome =
            if state.cancelled { TurnOutcome::Cancelled } else { TurnOutcome::EndTurn };
        state.deferred = None;
        while let Some(turn) = state.turns.pop_front() {
            let _ = turn.send(outcome);
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
                self.lock().aborted_requests.insert(cancel.request_id);
            }
            // Answering a keep_alive is a protocol error; unknown frame types
            // are how the CLI ships new features.
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
                    if state.context_size == 0 {
                        state.context_size = wire::infer_context_window(&system.model);
                    }
                }
                let mode_changed = match &system.permission_mode {
                    Some(mode) if !mode.is_empty() && *mode != state.mode => {
                        state.mode = mode.clone();
                        true
                    }
                    _ => false,
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
                meta.insert("trigger".to_string(), json!(trigger));
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
                    self.settle_deferred(&mut state);
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
                self.lock().tasks.insert(
                    task_id.clone(),
                    TaskEntry {
                        tool_use_id: tool_use_id.clone(),
                        subagent_type: subagent_type.clone(),
                        live: true,
                    },
                );
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
                let status = system
                    .extra
                    .get("status")
                    .or_else(|| system.extra.get("patch").and_then(|patch| patch.get("status")))
                    .and_then(Value::as_str)
                    .unwrap_or("running")
                    .to_string();
                let terminal = matches!(status.as_str(), "completed" | "failed" | "cancelled");
                let mut state = self.lock();
                let (tool_use_id, subagent_type) = match state.tasks.get_mut(&task_id) {
                    Some(task) => {
                        task.live = !terminal;
                        (task.tool_use_id.clone(), task.subagent_type.clone())
                    }
                    None => (None, None),
                };
                if terminal {
                    self.settle_deferred(&mut state);
                }
                drop(state);
                self.publish_subagent(
                    cx,
                    &task_id,
                    tool_use_id.as_deref(),
                    subagent_type.as_deref(),
                    &status,
                );
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

        for block in &blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_result") => self.on_tool_result(cx, block, structured, &parent),
                Some("text") => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or_default();
                    // The CLI persists local commands wrapped in
                    // <command-name>/<local-command-stdout> markers; a message
                    // that is nothing but markers is not a user message.
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
        let entry = self.lock().tools.get(id).map(|entry| (entry.name.clone(), entry.input.clone()));
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
                    let mut state = self.lock();
                    if state.context_size == 0 {
                        state.context_size = wire::infer_context_window(model);
                    }
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
                    Some("tool_use") => self.on_tool_use(cx, block, &parent),
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
        // The authoritative context window only ever arrives here.
        {
            let mut state = self.lock();
            let model = state.model.clone();
            if let Some(window) = wire::context_window_from_model_usage(&result.model_usage, &model)
            {
                state.context_size = window;
            }
        }
        self.merge_usage(cx, &result.usage, result.total_cost_usd);

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
        self.settle_or_defer(&mut state, outcome);
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
        let _ = cx.spawn(async move {
            if let Err(error) = session.control_request(wire::set_permission_mode(&mode)).await {
                log::warn!("engine: claude plan-mode switch failed: {error}");
            }
            {
                let mut state = session.lock();
                state.mode = mode.clone();
                state.skip_local_command_result = true;
            }
            let _ = session.send(wire::user_message("/clear", None));
            let prompt = format!("{PLAN_RESTART_PROMPT}\n\n{plan}");
            let _ = session.send(wire::user_message(&prompt, None));
            Ok(())
        });
        self.notify(
            cx,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(
                restart.mode,
            ))),
        );
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
            self.ask_user_question(cx, &request, &tool_use_id).await
        } else {
            self.request_permission(cx, &request, &tool_use_id).await
        };
        // The CLI cancelled this request while we were asking: answering it
        // now would be answering a request that no longer exists.
        if self.lock().aborted_requests.remove(&request_id) {
            return;
        }
        let _ = self.send(wire::control_response_success(&request_id, answer));
    }

    async fn request_permission(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> Value {
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
        // `decision_reason_type` says WHY the CLI is asking under a mode that
        // normally would not (`safetyCheck` is the bypass-immune one).
        if let Some(description) = request.description.clone().or_else(|| {
            request.decision_reason_type.clone().map(|reason| format!("Reason: {reason}"))
        }) {
            let mut meta = Map::new();
            meta.insert(
                "permission".to_string(),
                json!({ "version": 1, "description": description }),
            );
            tool_call = tool_call.meta(meta);
        }
        self.await_permission(cx, tool_call, options, request, tool_use_id).await
    }

    async fn await_permission(
        self: &Arc<Self>,
        cx: &ConnectionTo<Client>,
        tool_call: ToolCallUpdate,
        options: Vec<PermissionOption>,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> Value {
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
            return wire::permission_deny(tool_use_id, "Cancelled by the user", false);
        };
        self.apply_permission_selection(&selected, request, tool_use_id)
    }

    fn apply_permission_selection(
        self: &Arc<Self>,
        option_id: &str,
        request: &wire::ControlReq,
        tool_use_id: &str,
    ) -> Value {
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
                self.lock().pending_plan_restart =
                    Some(PlanRestart { plan, mode: mode.to_string() });
                return wire::permission_deny(
                    tool_use_id,
                    "User accepted the plan and requested a fresh context",
                    true,
                );
            }
            if let Some(mode) = exit_plan_mode(option_id) {
                self.lock().mode = mode.to_string();
                let mut allow = wire::permission_allow(tool_use_id, request.input.clone());
                allow["updatedPermissions"] =
                    json!([{ "type": "setMode", "mode": mode, "destination": "session" }]);
                if mode != "default" {
                    allow["decisionClassification"] = json!("user_permanent");
                }
                return allow;
            }
            if option_id == "reject" {
                // A plain deny lets claude keep planning; the interrupt stops
                // this turn so the user can steer instead.
                return wire::permission_deny(tool_use_id, "User chose to keep planning", true);
            }
        }
        match option_id {
            "allow-once" => wire::permission_allow(tool_use_id, request.input.clone()),
            "allow-with-updates" => {
                let mut allow = wire::permission_allow(tool_use_id, request.input.clone());
                if let Some(updates) = request.permission_suggestions.as_array() {
                    allow["updatedPermissions"] = json!(updates);
                    allow["decisionClassification"] = json!("user_permanent");
                }
                allow
            }
            "reject" => {
                wire::permission_deny(tool_use_id, "User refused permission to run tool", false)
            }
            other => {
                log::warn!("engine: claude permission option {other} is not one we offered");
                wire::permission_deny(tool_use_id, "User refused permission to run tool", false)
            }
        }
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

/// The five modes the picker offers. `dontAsk` is accepted by the CLI and
/// deliberately never offered; `bypassPermissions` is hidden from root
/// (the CLI's own `ALLOW_BYPASS` rule) because the CLI refuses it there.
fn available_modes() -> Vec<SessionMode> {
    let mut modes = vec![
        SessionMode::new(SessionModeId::new("default"), "Manual")
            .description("Always ask before making changes"),
        SessionMode::new(SessionModeId::new("acceptEdits"), "Accept edits")
            .description("Automatically accept all file edits"),
        SessionMode::new(SessionModeId::new("plan"), "Plan")
            .description("Create a plan before making changes"),
        SessionMode::new(SessionModeId::new("auto"), "Auto")
            .description("Claude handles permission decisions"),
    ];
    if allow_bypass() {
        modes.push(
            SessionMode::new(SessionModeId::new("bypassPermissions"), "Bypass permissions")
                .description("Accepts all permissions"),
        );
    }
    modes
}

/// `ALLOW_BYPASS = !IS_ROOT || !!IS_SANDBOX` (`permissions/modes.js`).
fn allow_bypass() -> bool {
    #[cfg(unix)]
    let root = unsafe { libc::geteuid() } == 0;
    #[cfg(not(unix))]
    let root = false;
    !root || std::env::var_os("IS_SANDBOX").is_some()
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A readable label for a model the CLI never described (no initialize
/// response): the alias as typed, capitalized.
fn display_model(model: &str) -> String {
    if model.is_empty() {
        "CLI default".to_string()
    } else {
        title_case(model)
    }
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
            let prompt = string("prompt");
            ToolInfo {
                title: if description.is_empty() { "Task".into() } else { description.into() },
                kind: ToolKind::Think,
                content: if prompt.is_empty() { vec![] } else { vec![text_content(prompt)] },
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

/// The plan-approval menu (`permissions/options/tools.js`). The elevated mode
/// is `auto` when it is offered, else `bypassPermissions`, else `acceptEdits`
/// — and the clear-context option only exists when there IS a plan to carry.
fn exit_plan_options(input: &Value) -> Vec<PermissionOption> {
    let modes = available_modes();
    let has = |id: &str| modes.iter().any(|mode| mode.id.0.as_ref() == id);
    let elevated = if has("auto") {
        "auto"
    } else if has("bypassPermissions") {
        "bypassPermissions"
    } else {
        "acceptEdits"
    };
    let plan = input.get("plan").and_then(Value::as_str).unwrap_or_default();
    let mut options = Vec::new();
    if !plan.trim().is_empty() {
        let (id, name) = match elevated {
            "auto" => ("exit-plan-clear-auto", "Yes, clear context and use auto mode"),
            "bypassPermissions" => {
                ("exit-plan-clear-bypass", "Yes, clear context and bypass permissions")
            }
            _ => ("exit-plan-clear-accept-edits", "Yes, clear context and auto-accept edits"),
        };
        options.push(PermissionOption::new(
            PermissionOptionId::new(id),
            name,
            PermissionOptionKind::AllowAlways,
        ));
    }
    let (id, name) = match elevated {
        "auto" => ("exit-plan-auto", "Yes, and use auto mode"),
        "bypassPermissions" => ("exit-plan-bypass", "Yes, and bypass permissions"),
        _ => ("exit-plan-accept-edits", "Yes, auto-accept edits"),
    };
    options.push(PermissionOption::new(
        PermissionOptionId::new(id),
        name,
        PermissionOptionKind::AllowAlways,
    ));
    options.push(PermissionOption::new(
        PermissionOptionId::new("exit-plan-default"),
        "Yes, manually approve edits",
        PermissionOptionKind::AllowOnce,
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

fn transcript_path(env: &[(String, String)], session_id: &str) -> Option<PathBuf> {
    let root = claude_projects_root(env)?;
    let name = format!("{session_id}.jsonl");
    std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(&name))
        .find(|path| path.is_file())
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

    #[test]
    fn the_plan_menu_offers_four_options_with_the_clear_context_one_first() {
        let options = exit_plan_options(&json!({ "plan": "# Plan" }));
        let ids: Vec<String> =
            options.iter().map(|option| option.option_id.0.to_string()).collect();
        assert_eq!(
            ids,
            vec![
                "exit-plan-clear-auto".to_string(),
                "exit-plan-auto".to_string(),
                "exit-plan-default".to_string(),
                "reject".to_string(),
            ]
        );
        // A plan-less approval has nothing to carry into a fresh context.
        let ids: Vec<String> = exit_plan_options(&json!({}))
            .iter()
            .map(|option| option.option_id.0.to_string())
            .collect();
        assert_eq!(ids.len(), 3);
        // The clear-context options DENY with an interrupt; they never allow.
        assert_eq!(exit_plan_clear_context_mode("exit-plan-clear-auto"), Some("auto"));
        assert_eq!(exit_plan_mode("exit-plan-clear-auto"), None);
        assert_eq!(exit_plan_mode("exit-plan-default"), Some("default"));
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

    #[test]
    fn the_mode_picker_never_offers_dont_ask() {
        let ids: Vec<String> =
            available_modes().iter().map(|mode| mode.id.0.to_string()).collect();
        assert!(ids.starts_with(&[
            "default".to_string(),
            "acceptEdits".to_string(),
            "plan".to_string(),
            "auto".to_string(),
        ]));
        assert!(!ids.iter().any(|id| id == "dontAsk"));
    }
}
