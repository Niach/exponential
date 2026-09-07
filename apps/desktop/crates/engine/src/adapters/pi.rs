//! EXP-746 — PiAgent: `pi --mode rpc` presented as an ACP agent. Owned by
//! lane E4.
//!
//! Shape (E4): `pi --mode rpc [--model] [--thinking] [--session <file>] -e
//! ./.exp-pi-mcp.ts [-e ./.exp-pi-plan.ts]`. The OBSERVER extension is
//! dropped on this path (the rpc stream is the observation channel); the MCP
//! bridge stays because pi has no native MCP. Non-obvious invariants: the
//! stream is strict LF-only JSONL and mixes four shapes discriminated only by
//! `type`, unknown types are ignorable rather than fatal; `agent_settled` is
//! the true idle edge; pi is the one agent that reports COST; and pi is
//! FILE-PATH keyed, not id-keyed, so its resume handle is a path.
//!
//! EXP-752 — plan mode. pi has NO native modes, so this adapter builds one
//! out of the same `.exp-pi-plan.ts` extension the PTY transport uses: the
//! extension is loaded with `-e` only when the launch asked for plan mode,
//! and only then does the session advertise the ACP modes `plan` + `default`.
//! Switching is a `prompt` carrying `/exp-plan on|off` — pi dispatches a
//! registered extension command instead of prompting the model — and it is
//! refused mid-turn, because a queued command would land in the wrong turn.
//! Approving a plan is the extension's `confirm` dialog: the one confirm
//! titled [`coding::pi_bridge::PI_PLAN_CONFIRM_TITLE`] IS a mode switch, so
//! its permission card is a `SwitchMode` and an approval moves the session to
//! `default` (the extension has stopped planning by then).
//!
//! Two structural rules, both load-bearing:
//!
//! - **The pump owns the stream.** ONE task reads pi's stdout and does three
//!   things with a line: resolve the command waiter it answers, turn an event
//!   into `session/update`s, or hand an `extension_ui_request` to a spawned
//!   task that asks the client. It never awaits a client round-trip inline,
//!   because pi keeps streaming while a dialog is open.
//! - **Handlers never block the dispatch loop.** `initialize` answers inline
//!   (it touches nothing); everything else needs a pi round-trip and is
//!   spawned with its `Responder` moved in, then swallows its own errors —
//!   a spawned task that returns `Err` tears the whole connection down.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, AgentResponse, AvailableCommand, AvailableCommandInput, ClientNotification,
    ClientRequest, CompactionId, CompactionStatus, CompactionUpdate, ConfigOptionUpdate, Content,
    ContentBlock, ContentChunk, Cost, CreateElicitationRequest, CurrentModeUpdate, ElicitationAction,
    ElicitationFormMode, ElicitationSchema, ElicitationSessionScope, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, NewSessionRequest,
    NewSessionResponse, PermissionOption, PermissionOptionKind, PromptCapabilities, PromptRequest,
    PromptResponse, RequestPermissionOutcome, RequestPermissionRequest,
    SessionConfigBoolean, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelect, SessionConfigSelectOption, SessionId, SessionInfoUpdate, SessionMode,
    SessionModeId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, StopReason, StringPropertySchema, ToolCall, ToolCallContent,
    ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    UnstructuredCommandInput, UsageUpdate,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, Error, Responder};
use serde_json::{json, Value};

use super::pi_wire::{
    self, PiArgs, PiEvent, PiModel, PiOut, PiState, PiUi, PiUsage, QUEUE_MODES,
    SYNTHESIZED_COMMANDS, THINKING_LEVELS,
};
use super::AdapterSpec;
use crate::session::{EngineError, ResumeHandle};
use crate::transport::{spawn_lines, ChildLines, StderrPolicy};

/// Where an adapter reports the agent's OWN session handle (D8's
/// `agent_native_session_id`) — the `NewSessionResponse` meta, because the
/// ACP `SessionId` alone cannot carry it for agents whose native identity is
/// a different shape. For pi the two coincide (both are the session FILE),
/// which is exactly why a resumed pi run re-enters through either recorded
/// field. The key is the engine's own ([`crate::NATIVE_SESSION_META_KEY`]).
pub use crate::host::NATIVE_SESSION_META_KEY;

/// Where an adapter reports a compaction's TRIGGER, which ACP's
/// `CompactionUpdate` has no field for. The mapper folds it with
/// `steer::normalize_compaction_trigger` (`manual` stays manual, everything
/// else is `auto`), so pi's `threshold`/`overflow` land as `auto`. The key is
/// the engine's own ([`crate::COMPACTION_TRIGGER_META_KEY`]).
pub use crate::local::COMPACTION_TRIGGER_META_KEY;

/// Config option ids. Stable strings: they key the relay `config_state`
/// options AND the `set_config` frames every client sends back.
const CONFIG_MODEL: &str = "model";
const CONFIG_THINKING: &str = "thinking_level";
const CONFIG_STEERING: &str = "steering_mode";
const CONFIG_FOLLOW_UP: &str = "follow_up_mode";
const CONFIG_AUTO_COMPACTION: &str = "auto_compaction";

/// The two mode ids a plan-mode pi session advertises. Same strings claude
/// uses, so every client's mode picker renders them identically.
const MODE_PLAN: &str = "plan";
const MODE_DEFAULT: &str = "default";

/// EXP-758: how long one rpc command may go unanswered before its caller
/// gives up. pi has no timeout of its own, so without this a wedged (or
/// silently dropped) command parks its handler task forever and the config
/// setter, the compaction or the `session/new` handshake that issued it never
/// answers. Sized like the neighbours: claude's `CONTROL_TIMEOUT` is 90 s,
/// codex's `CALL_TIMEOUT` 120 s, and pi's slowest command is `compact`.
const RPC_TIMEOUT: Duration = Duration::from_secs(120);

/// A tool's streamed output is a LOCAL card, but it still crosses a channel
/// and lands in a `Vec` — cap it so a runaway `bash` cannot grow the session
/// without bound. (The renderer caps again at 200 lines.)
const OUTPUT_CHUNK_MAX: usize = 64 * 1024;

pub struct PiAgent {
    spec: AdapterSpec,
    child: ChildLines,
    rpc_timeout: Duration,
}

impl PiAgent {
    pub fn new(spec: AdapterSpec) -> Result<PiAgent, EngineError> {
        PiAgent::with_rpc_timeout(spec, RPC_TIMEOUT)
    }

    /// [`PiAgent::new`] with an explicit rpc deadline. Only the adapter tests
    /// pass anything but [`RPC_TIMEOUT`]: a suite cannot wait two minutes to
    /// prove a command that is never answered gives up.
    pub fn with_rpc_timeout(
        spec: AdapterSpec,
        rpc_timeout: Duration,
    ) -> Result<PiAgent, EngineError> {
        let mut spawn = spec.spawn.clone();
        // The Acp arm hands us an EMPTY argv on purpose (D2): the rpc argv
        // has nothing in common with the TUI one.
        spawn.args = pi_argv_for(&spec);
        let child = spawn_lines(&spawn, StderrPolicy::Log)?;
        child.forward_exit(&spec.exit);
        log::info!(
            "engine: pi rpc child {} started for session {}",
            child.pid,
            spec.session_id
        );
        Ok(PiAgent { spec, child, rpc_timeout })
    }
}

/// The rpc argv for one launch. Split out of [`PiAgent::new`] so the argv is
/// testable without a child process.
fn pi_argv_for(spec: &AdapterSpec) -> Vec<String> {
    let model = spec.options.model.trim();
    let thinking = spec.options.effort.trim();
    let session_file = resume_session_file(spec.resume.as_ref());
    // pi has no native MCP: the bridge extension IS the MCP wiring. The
    // OBSERVER extension belongs to the PTY transport (the rpc stream is this
    // path's observation channel); the PLAN extension (EXP-752) is loaded
    // only when the launch asked for plan mode — the launcher writes it on
    // both transports and gates it on `EXP_PI_PLAN_MODE`, but an extension
    // that is not on the argv can never register its `/exp-plan` command.
    let mut extensions: Vec<PathBuf> = match spec.mcp {
        coding::AgentMcp::PiExtension => {
            vec![PathBuf::from(format!("./{}", coding::pi_bridge::PI_BRIDGE_FILE))]
        }
        _ => Vec::new(),
    };
    if spec.options.plan_mode {
        extensions.push(PathBuf::from(format!(
            "./{}",
            coding::pi_bridge::PI_PLAN_FILE
        )));
    }
    pi_wire::pi_argv(&PiArgs {
        model: (!model.is_empty()).then_some(model),
        thinking: (!thinking.is_empty()).then_some(thinking),
        session_file: session_file.as_deref(),
        extensions: &extensions,
        // EXP-763: the run playbook, on every start and resume.
        append_system_prompt: Some(coding::skill::RUN_SKILL),
    })
}

/// pi resumes by FILE. A recorded ACP session id IS that file (see
/// [`NATIVE_SESSION_META_KEY`]), so both handle shapes resolve to a path and
/// a run recorded on either transport reopens the same conversation.
fn resume_session_file(resume: Option<&ResumeHandle>) -> Option<PathBuf> {
    match resume? {
        ResumeHandle::PiSessionFile(path) => Some(path.clone()),
        ResumeHandle::Acp(id) | ResumeHandle::Native(id) => {
            let path = PathBuf::from(id);
            // An id that is not a path cannot be pi's (a claude uuid landing
            // here would create a junk session file called `<uuid>`).
            (path.is_absolute() || id.contains(std::path::MAIN_SEPARATOR)).then_some(path)
        }
    }
}

impl ConnectTo<Client> for PiAgent {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        async move {
            let exit = self.spec.exit.clone();
            let session = Arc::new(PiSession::new(&self.spec, self.child, self.rpc_timeout));
            let handler_session = Arc::clone(&session);
            let notification_session = Arc::clone(&session);
            Agent
                .builder()
                .name("exponential-pi")
                .on_receive_request(
                    async move |request: ClientRequest, responder, cx| {
                        handle_request(&handler_session, request, responder, cx)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_notification(
                    async move |notification: ClientNotification, _cx| {
                        handle_notification(&notification_session, notification)
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(client, async move |cx: ConnectionTo<Client>| {
                    pump(session, exit, cx).await
                })
                .await
        }
    }
}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// One pending prompt turn. pi settles a turn ONCE (`agent_settled`) however
/// many messages were steered into it, so every waiter resolves together.
///
/// EXP-758: each waiter carries an id, because a steer whose rpc FAILS has to
/// withdraw its own waiter and leave the turn (and every other waiter on it)
/// exactly as it was: pi is still running the first message.
#[derive(Default)]
struct Turn {
    active: bool,
    next_waiter: u64,
    waiters: Vec<(u64, tokio::sync::oneshot::Sender<StopReason>)>,
}

/// What we remembered when a tool call started.
struct ToolEntry {
    kind: ToolKind,
    path: Option<PathBuf>,
    /// The file's content BEFORE an edit-shaped tool ran; `None` for a file
    /// that did not exist yet (a `write` of a new file) or a non-edit tool.
    before: Option<String>,
}

struct PiSession {
    /// The live child. Held whole because `ChildLines` is a kill-on-drop
    /// guard: keeping only its channels would reap pi the moment
    /// `PiAgent::connect_to` moved past the constructor.
    child: ChildLines,
    cwd: PathBuf,
    next_id: AtomicU64,
    pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<PiReply>>>,
    turn: Mutex<Turn>,
    /// Empty until `session/new` names it. pi emits nothing before the first
    /// prompt, so no update is lost; an update that did arrive early would be
    /// undeliverable anyway (a client drops notifications for an unknown
    /// session).
    session_id: Mutex<SessionId>,
    state: Mutex<PiState>,
    models: Mutex<Vec<PiModel>>,
    /// EXP-752: the session's ACP mode, and the only record that plan mode
    /// was requested at all. `None` = no plan extension on the argv, so this
    /// session has NO modes (pi's own answer); `Some` starts at
    /// [`MODE_PLAN`] because the extension starts planning.
    mode: Mutex<Option<String>>,
    tools: Mutex<HashMap<String, ToolEntry>>,
    compactions: AtomicU64,
    open_compaction: Mutex<Option<String>>,
    /// EXP-758: the advertised commands pi runs ITSELF (`source` =
    /// `extension`). They start no turn, so `session/prompt` answers them off
    /// the rpc response instead of waiting for an `agent_settled` that never
    /// comes. Filled by [`publish_commands`], on `session/new` and
    /// `session/load` alike.
    extension_commands: Mutex<HashSet<String>>,
    /// EXP-758: what this session has spent, in USD. pi reports cost PER
    /// TURN, but ACP's `UsageUpdate.cost` is "cost information for a session"
    /// and every client renders it as one running total, so the adapter adds
    /// the turns up instead of reporting the last one.
    cost_usd: Mutex<f64>,
    /// How long one rpc command may go unanswered ([`RPC_TIMEOUT`]).
    rpc_timeout: Duration,
}

/// One `{"type":"response"}` line, matched to the command that asked.
struct PiReply {
    success: bool,
    data: Value,
}

impl PiReply {
    fn ok(self) -> Result<Value, Error> {
        if self.success {
            Ok(self.data)
        } else {
            Err(Error::internal_error().data(json!(self
                .data
                .as_str()
                .unwrap_or("the agent rejected the command"))))
        }
    }
}

/// Lock without ever poisoning a session: a panic in one handler must not
/// take the rest of the run down with it.
fn guard<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl PiSession {
    fn new(spec: &AdapterSpec, child: ChildLines, rpc_timeout: Duration) -> PiSession {
        PiSession {
            child,
            cwd: spec.cwd.clone(),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            turn: Mutex::new(Turn::default()),
            session_id: Mutex::new(SessionId::new("")),
            state: Mutex::new(PiState::default()),
            models: Mutex::new(Vec::new()),
            mode: Mutex::new(initial_mode(spec.options.plan_mode)),
            tools: Mutex::new(HashMap::new()),
            compactions: AtomicU64::new(0),
            open_compaction: Mutex::new(None),
            extension_commands: Mutex::new(HashSet::new()),
            cost_usd: Mutex::new(0.0),
            rpc_timeout,
        }
    }

    fn next_id(&self) -> String {
        self.next_id.fetch_add(1, Ordering::SeqCst).to_string()
    }

    /// Send one rpc command and await its `{"type":"response"}`.
    async fn request(&self, command: &str, params: Value) -> Result<Value, Error> {
        let id = self.next_id();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        guard(&self.pending).insert(id.clone(), sender);
        let line = pi_wire::command(&id, command, params).to_string();
        if let Err(err) = self.child.writer.write_line(&line) {
            guard(&self.pending).remove(&id);
            return Err(Error::internal_error().data(json!(format!("pi stdin: {err}"))));
        }
        // EXP-758: pi answers or it does not, and an unanswered command used
        // to park its handler task for the life of the run. Drop the pending
        // entry on the way out so a very late answer resolves nothing.
        match tokio::time::timeout(self.rpc_timeout, receiver).await {
            Ok(Ok(reply)) => reply.ok(),
            Ok(Err(_)) => Err(Error::internal_error().data(json!("pi stopped before answering"))),
            Err(_) => {
                guard(&self.pending).remove(&id);
                Err(Error::internal_error().data(json!(format!(
                    "pi did not answer {command} within {}s",
                    self.rpc_timeout.as_secs()
                ))))
            }
        }
    }

    /// Fire one rpc command with no waiter (`abort`, a steer).
    fn notify_pi(&self, command: &str, params: Value) {
        let id = self.next_id();
        let line = pi_wire::command(&id, command, params).to_string();
        if let Err(err) = self.child.writer.write_line(&line) {
            log::warn!("engine: pi stdin ({command}): {err}");
        }
    }

    fn resolve(&self, id: Option<String>, command: &str, success: bool, data: Value) {
        let Some(id) = id else {
            log::debug!("engine: pi answered {command} with no id");
            return;
        };
        if let Some(sender) = guard(&self.pending).remove(&id) {
            let _ = sender.send(PiReply { success, data });
        }
    }

    /// Publish one `session/update`. Fire-and-forget: a closed connection is
    /// the shutdown path, not an error to propagate.
    fn notify(&self, cx: &ConnectionTo<Client>, update: SessionUpdate) {
        let session_id = guard(&self.session_id).clone();
        if session_id.0.is_empty() {
            log::debug!("engine: pi update before session/new, dropped");
            return;
        }
        if let Err(err) = cx.send_notification(SessionNotification::new(session_id, update)) {
            log::debug!("engine: pi update not delivered: {err}");
        }
    }

    /// The whole config snapshot, always emitted WHOLE (`config_state` is
    /// latest-wins state, never a delta).
    fn config_options(&self) -> Vec<SessionConfigOption> {
        config_options(&guard(&self.state), &guard(&self.models))
    }

    /// Refresh `get_state` and republish the options — every setter ends here
    /// so the re-emitted `config_state` IS the confirmation (D4).
    async fn refresh_state(&self, cx: &ConnectionTo<Client>) -> Vec<SessionConfigOption> {
        if let Ok(data) = self.request("get_state", json!({})).await {
            *guard(&self.state) = pi_wire::parse_state(&data);
        }
        let options = self.config_options();
        self.notify(
            cx,
            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(options.clone())),
        );
        options
    }

    /// EXP-752: the mode state a plan-mode session advertises on `session/new`
    /// and `session/load`. `None` for every other pi session — pi has no
    /// native modes, and a mode picker over nothing is worse than none.
    fn mode_state(&self) -> Option<SessionModeState> {
        mode_state(&guard(&self.mode))
    }

    /// Take the session to `mode` and tell the client. Callers own the pi
    /// round-trip; this is the bookkeeping half, shared by `session/set_mode`
    /// and the approved plan confirm. A session with NO modes stays that way:
    /// a `current_mode_update` for a mode set the client never saw would be
    /// undeliverable state.
    fn enter_mode(&self, cx: &ConnectionTo<Client>, mode: &str) {
        {
            let mut slot = guard(&self.mode);
            if slot.is_none() {
                return;
            }
            *slot = Some(mode.to_string());
        }
        self.notify(
            cx,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(SessionModeId::new(mode))),
        );
    }

    /// Register a waiter for the running turn and say whether this message
    /// STARTS the turn (`prompt`) or joins it (`steer`).
    fn enter_turn(&self, sender: tokio::sync::oneshot::Sender<StopReason>) -> (bool, u64) {
        let mut turn = guard(&self.turn);
        let starting = !turn.active;
        turn.active = true;
        turn.next_waiter += 1;
        let waiter_id = turn.next_waiter;
        turn.waiters.push((waiter_id, sender));
        (starting, waiter_id)
    }

    /// EXP-758: withdraw ONE waiter without touching the turn. The steer that
    /// failed to reach pi is over; the message pi is still working on is not.
    fn leave_turn(&self, waiter_id: u64) {
        guard(&self.turn).waiters.retain(|(id, _)| *id != waiter_id);
    }

    /// Settle the turn and answer every prompt request waiting on it.
    fn settle(&self, stop: StopReason) {
        let mut turn = guard(&self.turn);
        turn.active = false;
        for (_, waiter) in turn.waiters.drain(..) {
            let _ = waiter.send(stop);
        }
    }

    /// EXP-758: is `name` one of the commands pi dispatches itself?
    fn is_extension_command(&self, name: &str) -> bool {
        guard(&self.extension_commands).contains(name)
    }

    /// The child's stdout ended: nothing will ever answer again.
    fn shutdown(&self) {
        guard(&self.pending).clear();
        self.settle(StopReason::EndTurn);
    }
}

// ---------------------------------------------------------------------------
// The pump: pi stdout -> ACP
// ---------------------------------------------------------------------------

/// `main_fn`: read pi's stdout until EOF. Returning ends the connection, so
/// the child's exit IS the session's end.
async fn pump(
    session: Arc<PiSession>,
    exit: crate::ChildExitLink,
    cx: ConnectionTo<Client>,
) -> Result<(), Error> {
    let mut child_gone = false;
    loop {
        tokio::select! {
            // The host ended the session (a kill, a quit, the screen closing):
            // stop reading, and let this session's drop kill pi through the
            // `ChildLines` guard rather than waiting for it to notice its
            // stdin closed.
            () = cx.incoming_closed() => break,
            line = session.child.lines.recv_async() => match line {
                Ok(line) => on_line(&session, &line, &cx),
                // The child closed stdout: the run is over either way.
                Err(_) => {
                    child_gone = true;
                    break;
                }
            },
        }
    }
    session.shutdown();
    if child_gone {
        // stdout ends a beat before `wait()` reaps: give the code its moment
        // so the run ends as `exit:<code>` rather than a bare `ended`.
        let _ = tokio::time::timeout(crate::host::CHILD_EXIT_GRACE, exit.reaped()).await;
    }
    Ok(())
}

fn on_line(session: &Arc<PiSession>, line: &str, cx: &ConnectionTo<Client>) {
    match pi_wire::parse_line(line) {
        PiOut::Response { id, command, success, data } => {
            session.resolve(id, &command, success, data)
        }
        PiOut::ExtensionUiRequest { id, method, params } => {
            on_ui_request(session, id, &method, &params, cx)
        }
        PiOut::ExtensionError { message } => session.notify(
            cx,
            SessionUpdate::AgentMessageChunk(text_chunk(
                None,
                format!("pi extension error: {message}"),
            )),
        ),
        PiOut::Event { kind, event } => on_event(session, &kind, &event, cx),
        PiOut::Unknown => log::debug!("engine: pi line ignored: {}", first_chars(line, 120)),
    }
}

fn on_event(session: &Arc<PiSession>, kind: &str, event: &Value, cx: &ConnectionTo<Client>) {
    match pi_wire::classify_event(kind, event) {
        PiEvent::TextDelta { message_id, text } => {
            if !text.is_empty() {
                session.notify(
                    cx,
                    SessionUpdate::AgentMessageChunk(text_chunk(message_id, text)),
                );
            }
        }
        PiEvent::ThinkingDelta { message_id, text } => {
            if !text.is_empty() {
                session.notify(
                    cx,
                    SessionUpdate::AgentThoughtChunk(text_chunk(message_id, text)),
                );
            }
        }
        PiEvent::ToolStart { id, name, args } => {
            let kind = tool_kind(&name);
            let path = tool_path(&args, &session.cwd);
            // The before/after snapshot IS pi's diff: pi reports no patch of
            // its own, so an edit card would otherwise be a bare status line.
            let before = path
                .as_ref()
                .filter(|_| is_edit(kind))
                .and_then(|path| std::fs::read_to_string(path).ok());
            guard(&session.tools).insert(
                id.clone(),
                ToolEntry { kind, path: path.clone(), before },
            );
            let mut call = ToolCall::new(id, name)
                .kind(kind)
                .status(ToolCallStatus::InProgress)
                .raw_input(args);
            if let Some(path) = path {
                call = call.locations(vec![ToolCallLocation::new(path)]);
            }
            session.notify(cx, SessionUpdate::ToolCall(call));
        }
        PiEvent::ToolUpdate { id, output } => {
            let Some(output) = output else { return };
            let is_execute = guard(&session.tools)
                .get(&id)
                .is_some_and(|entry| entry.kind == ToolKind::Execute);
            if !is_execute {
                return;
            }
            session.notify(
                cx,
                SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    id,
                    ToolCallUpdateFields::new()
                        .content(vec![output_content(&output)])
                        .status(ToolCallStatus::InProgress),
                )),
            );
        }
        PiEvent::ToolEnd { id, result, is_error } => {
            let entry = guard(&session.tools).remove(&id);
            let mut fields = ToolCallUpdateFields::new().status(if is_error {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            });
            let mut content = Vec::new();
            if let Some(entry) = entry {
                match (is_edit(entry.kind), entry.path) {
                    (true, Some(path)) => {
                        let after = std::fs::read_to_string(&path).ok();
                        if let Some(after) = after {
                            if entry.before.as_deref() != Some(after.as_str()) {
                                content.push(ToolCallContent::Diff(
                                    agent_client_protocol::schema::v1::Diff::new(path, after)
                                        .old_text(entry.before),
                                ));
                            }
                        }
                    }
                    _ => {
                        if entry.kind == ToolKind::Execute {
                            if let Some(text) = result_text(&result) {
                                content.push(output_content(&text));
                            }
                        }
                    }
                }
            }
            if !content.is_empty() {
                fields = fields.content(content);
            }
            session.notify(
                cx,
                SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    id,
                    fields.raw_output(Some(result)),
                )),
            );
        }
        PiEvent::TurnEnd { usage, error } => {
            if let Some(usage) = usage {
                if let Some(update) = usage_update(session, &usage) {
                    session.notify(cx, SessionUpdate::UsageUpdate(update));
                }
            }
            if let Some(error) = error {
                session.notify(
                    cx,
                    SessionUpdate::AgentMessageChunk(text_chunk(None, format!("pi: {error}"))),
                );
            }
        }
        PiEvent::Settled => session.settle(StopReason::EndTurn),
        PiEvent::CompactionStart { reason } => {
            let id = format!(
                "compaction-{}",
                session.compactions.fetch_add(1, Ordering::SeqCst)
            );
            *guard(&session.open_compaction) = Some(id.clone());
            let mut update =
                CompactionUpdate::new(CompactionId::new(id), CompactionStatus::InProgress);
            if let Some(meta) = trigger_meta(reason.as_deref()) {
                update = update.meta(meta);
            }
            session.notify(cx, SessionUpdate::CompactionUpdate(update));
        }
        PiEvent::CompactionEnd { reason, error } => {
            let id = guard(&session.open_compaction)
                .take()
                .unwrap_or_else(|| "compaction-0".to_string());
            let status = if error.is_some() {
                CompactionStatus::Failed
            } else {
                CompactionStatus::Completed
            };
            let mut update = CompactionUpdate::new(CompactionId::new(id), status);
            if let Some(meta) = trigger_meta(reason.as_deref()) {
                update = update.meta(meta);
            }
            if let Some(error) = error {
                update = update.error(error);
            }
            session.notify(cx, SessionUpdate::CompactionUpdate(update));
        }
        PiEvent::ThinkingLevelChanged { level } => {
            guard(&session.state).thinking_level = level;
            let options = session.config_options();
            session.notify(
                cx,
                SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(options)),
            );
        }
        PiEvent::SessionInfoChanged { name } => {
            guard(&session.state).session_name = name.clone();
            session.notify(
                cx,
                SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().title(name)),
            );
        }
        PiEvent::Ignored => {}
    }
}

/// pi's ONLY interactive channel. `confirm`/`select` become a permission
/// request, `input`/`editor` an elicitation; the fire-and-forget methods are
/// never answered (`rpc-mode.js` has no waiter for them, so a reply would be
/// an unmatched id).
fn on_ui_request(
    session: &Arc<PiSession>,
    id: String,
    method: &str,
    params: &Value,
    cx: &ConnectionTo<Client>,
) {
    let request = pi_wire::classify_ui(method, params);
    match request {
        PiUi::Notify { message } => {
            session.notify(
                cx,
                SessionUpdate::AgentMessageChunk(text_chunk(None, message)),
            );
            return;
        }
        PiUi::Ignored => return,
        _ => {}
    }
    let session_id = guard(&session.session_id).clone();
    if session_id.0.is_empty() {
        // Nothing can answer yet; releasing pi is better than wedging it.
        answer_ui(session, &id, json!({ "cancelled": true }));
        return;
    }
    let asked = cx.spawn({
        let session = Arc::clone(session);
        let cx = cx.clone();
        let id = id.clone();
        async move {
            let payload = ask_client(&session, &cx, session_id, &request).await;
            answer_ui(&session, &id, payload);
            Ok(())
        }
    });
    if let Err(err) = asked {
        log::warn!("engine: pi dialog not forwarded: {err}");
        answer_ui(session, &id, json!({ "cancelled": true }));
    }
}

/// Ask the client and translate its answer back into pi's response shape. A
/// declined, cancelled or undeliverable ask is `{cancelled: true}`, which is
/// what pi's own timeout path resolves to.
async fn ask_client(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    session_id: SessionId,
    request: &PiUi,
) -> Value {
    match request {
        PiUi::Confirm { title, message } => {
            // EXP-752: the plan extension's approval dialog is the one confirm
            // that is really a MODE SWITCH — the card says so, and an approval
            // moves the session to `default` (the extension has stopped
            // planning by the time it returns).
            let is_plan = title == coding::pi_bridge::PI_PLAN_CONFIRM_TITLE;
            let tool_call = ToolCallUpdate::new(
                format!("pi-confirm-{}", session.next_id()),
                ToolCallUpdateFields::new()
                    .title(Some(title.clone()))
                    .kind(Some(if is_plan {
                        ToolKind::SwitchMode
                    } else {
                        ToolKind::Other
                    }))
                    .status(Some(ToolCallStatus::Pending))
                    .content(Some(vec![ToolCallContent::Content(Content::new(
                        ContentBlock::from(message.clone()),
                    ))])),
            );
            let options = vec![
                PermissionOption::new("allow", "Yes", PermissionOptionKind::AllowOnce),
                PermissionOption::new("reject", "No", PermissionOptionKind::RejectOnce),
            ];
            let outcome = cx
                .send_request(RequestPermissionRequest::new(session_id, tool_call, options))
                .block_task()
                .await;
            match outcome.map(|response| response.outcome) {
                Ok(RequestPermissionOutcome::Selected(selected)) => {
                    let confirmed = selected.option_id.0.as_ref() == "allow";
                    if is_plan && confirmed {
                        session.enter_mode(cx, MODE_DEFAULT);
                    }
                    json!({ "confirmed": confirmed })
                }
                _ => json!({ "cancelled": true }),
            }
        }
        PiUi::Select { title, options } => {
            let permission_options: Vec<PermissionOption> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    PermissionOption::new(
                        index.to_string(),
                        option.clone(),
                        PermissionOptionKind::AllowOnce,
                    )
                })
                .collect();
            let tool_call = ToolCallUpdate::new(
                format!("pi-select-{}", session.next_id()),
                ToolCallUpdateFields::new()
                    .title(Some(title.clone()))
                    .kind(Some(ToolKind::Other))
                    .status(Some(ToolCallStatus::Pending)),
            );
            let outcome = cx
                .send_request(RequestPermissionRequest::new(
                    session_id,
                    tool_call,
                    permission_options,
                ))
                .block_task()
                .await;
            match outcome.map(|response| response.outcome) {
                Ok(RequestPermissionOutcome::Selected(selected)) => selected
                    .option_id
                    .0
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| options.get(index))
                    .map(|value| json!({ "value": value }))
                    .unwrap_or_else(|| json!({ "cancelled": true })),
                _ => json!({ "cancelled": true }),
            }
        }
        PiUi::Input { title, placeholder } | PiUi::Editor { title, prefill: placeholder } => {
            let mut field = StringPropertySchema::new().title(Some(title.clone()));
            if let Some(placeholder) = placeholder {
                field = field.description(Some(placeholder.clone()));
            }
            let schema = ElicitationSchema::new().property("value", field, true);
            let response = cx
                .send_request(CreateElicitationRequest::new(
                    ElicitationFormMode::new(ElicitationSessionScope::new(session_id), schema),
                    title.clone(),
                ))
                .block_task()
                .await;
            match response.map(|response| response.action) {
                Ok(ElicitationAction::Accept(accept)) => accept
                    .content
                    .as_ref()
                    .and_then(|content| content.get("value"))
                    .and_then(|value| match value {
                        agent_client_protocol::schema::v1::ElicitationContentValue::String(
                            text,
                        ) => Some(json!({ "value": text })),
                        _ => None,
                    })
                    .unwrap_or_else(|| json!({ "cancelled": true })),
                _ => json!({ "cancelled": true }),
            }
        }
        PiUi::Notify { .. } | PiUi::Ignored => json!({ "cancelled": true }),
    }
}

fn answer_ui(session: &PiSession, id: &str, payload: Value) {
    let line = pi_wire::extension_ui_response(id, payload).to_string();
    if let Err(err) = session.child.writer.write_line(&line) {
        log::warn!("engine: pi dialog answer not written: {err}");
    }
}

// ---------------------------------------------------------------------------
// Handlers: ACP -> pi
// ---------------------------------------------------------------------------

fn handle_request(
    session: &Arc<PiSession>,
    request: ClientRequest,
    responder: Responder<Value>,
    cx: ConnectionTo<Client>,
) -> Result<(), Error> {
    // A handler registered over the whole `ClientRequest` enum answers in raw
    // JSON (the enum's `JsonRpcRequest::Response` is `Value`); casting keeps
    // the typed responses AND the schema's method/variant check.
    let responder = responder.cast::<AgentResponse>();
    match request {
        // The only handler that answers INLINE: it touches no pi state.
        ClientRequest::InitializeRequest(request) => {
            responder.respond(AgentResponse::InitializeResponse(initialize(&request)))
        }
        ClientRequest::NewSessionRequest(request) => {
            spawn_answer(session, &cx, responder, move |session, cx| async move {
                new_session(&session, &cx, request)
                    .await
                    .map(AgentResponse::NewSessionResponse)
            })
        }
        ClientRequest::LoadSessionRequest(request) => {
            spawn_answer(session, &cx, responder, move |session, cx| async move {
                load_session(&session, &cx, request)
                    .await
                    .map(AgentResponse::LoadSessionResponse)
            })
        }
        ClientRequest::PromptRequest(request) => {
            spawn_answer(session, &cx, responder, move |session, cx| async move {
                prompt(&session, &cx, request)
                    .await
                    .map(AgentResponse::PromptResponse)
            })
        }
        ClientRequest::SetSessionConfigOptionRequest(request) => {
            spawn_answer(session, &cx, responder, move |session, cx| async move {
                set_config_option(&session, &cx, request)
                    .await
                    .map(AgentResponse::SetSessionConfigOptionResponse)
            })
        }
        // EXP-752: the ONE mode pi can be given — and only when the launch
        // loaded the plan extension. Without it the config options are still
        // the whole switchable surface, so this stays a client bug rather
        // than a silent no-op.
        ClientRequest::SetSessionModeRequest(request) => {
            spawn_answer(session, &cx, responder, move |session, cx| async move {
                set_mode(&session, &cx, request)
                    .await
                    .map(AgentResponse::SetSessionModeResponse)
            })
        }
        other => responder.respond_with_error(
            Error::method_not_found().data(json!(format!("pi does not implement {}", other.method()))),
        ),
    }
}

fn handle_notification(
    session: &Arc<PiSession>,
    notification: ClientNotification,
) -> Result<(), Error> {
    if let ClientNotification::CancelNotification(_) = notification {
        // `abort` is pi's interrupt. The ACP contract still owes the prompt
        // request an answer, so settle every waiter as Cancelled rather than
        // waiting for an `agent_settled` that an aborted turn may not send.
        session.notify_pi("abort", json!({}));
        session.settle(StopReason::Cancelled);
    }
    Ok(())
}

/// Move `responder` into a spawned task so the dispatch loop stays free, and
/// swallow every error into the response — a spawned task that returns `Err`
/// shuts the whole connection down.
fn spawn_answer<Fut, Make>(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    responder: Responder<AgentResponse>,
    make: Make,
) -> Result<(), Error>
where
    Make: FnOnce(Arc<PiSession>, ConnectionTo<Client>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<AgentResponse, Error>> + Send + 'static,
{
    let session = Arc::clone(session);
    let spawned = cx.spawn({
        let cx = cx.clone();
        async move {
            let answer = make(session, cx).await;
            let _ = match answer {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(error),
            };
            Ok(())
        }
    });
    if let Err(err) = spawned {
        log::warn!("engine: pi request not spawned: {err}");
    }
    Ok(())
}

fn initialize(request: &InitializeRequest) -> InitializeResponse {
    let version = if request.protocol_version.as_u16() > ProtocolVersion::V1.as_u16() {
        ProtocolVersion::V1
    } else {
        request.protocol_version
    };
    InitializeResponse::new(version).agent_capabilities(
        AgentCapabilities::new()
            // pi replays a session FILE, which is what `session/load` needs.
            .load_session(true)
            .prompt_capabilities(
                PromptCapabilities::new()
                    .image(true)
                    .embedded_context(true),
            ),
    )
}

async fn new_session(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    _request: NewSessionRequest,
) -> Result<NewSessionResponse, Error> {
    // The cwd is NOT applied here: the child was spawned into the worktree
    // already (`PreparedLaunch::spawn`), and pi has no rpc verb to move.
    let state = pi_wire::parse_state(&session.request("get_state", json!({})).await?);
    let models = session
        .request("get_available_models", json!({}))
        .await
        .map(|data| pi_wire::parse_models(&data))
        .unwrap_or_default();
    // pi is FILE-keyed: the session file is both the ACP session id and the
    // agent-native handle, so a resume re-enters through either recorded
    // field (D8).
    let identity = state
        .session_file
        .clone()
        .unwrap_or_else(|| state.session_id.clone());
    *guard(&session.state) = state;
    *guard(&session.models) = models;
    *guard(&session.session_id) = SessionId::new(identity.clone());

    let options = session.config_options();
    publish_commands(session, cx).await;
    let mut meta = serde_json::Map::new();
    meta.insert(NATIVE_SESSION_META_KEY.to_string(), json!(identity.clone()));
    Ok(NewSessionResponse::new(SessionId::new(identity))
        .modes(session.mode_state())
        .config_options(options)
        .meta(meta))
}

async fn load_session(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    request: LoadSessionRequest,
) -> Result<LoadSessionResponse, Error> {
    let path = request.session_id.0.to_string();
    *guard(&session.session_id) = request.session_id.clone();
    session
        .request("switch_session", json!({ "sessionPath": path }))
        .await?;
    let state = pi_wire::parse_state(&session.request("get_state", json!({})).await?);
    let models = session
        .request("get_available_models", json!({}))
        .await
        .map(|data| pi_wire::parse_models(&data))
        .unwrap_or_default();
    *guard(&session.state) = state;
    *guard(&session.models) = models;

    let entries = session.request("get_entries", json!({})).await?;
    for update in replay_updates(&entries) {
        session.notify(cx, update);
    }
    let options = session.config_options();
    publish_commands(session, cx).await;
    Ok(LoadSessionResponse::new()
        .modes(session.mode_state())
        .config_options(options))
}

async fn prompt(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    request: PromptRequest,
) -> Result<PromptResponse, Error> {
    let (text, images) = split_prompt(&request.prompt);
    // A `/` command the agent owns is a VERB, not a message: sending it as
    // text would put the literal `/compact` into the conversation.
    if let Some((name, args)) = slash_command(&text) {
        // EXP-752: `/exp-plan` is the MODE SWITCH, and `publish_commands`
        // hides it from the picker — but free text (the composer, a remote
        // steer message) still reaches here. pi runs a registered extension
        // command and starts NO turn, so forwarding it as an ordinary prompt
        // would park this request on a turn that never settles: take the same
        // route `session/set_mode` takes, refusals included.
        if name == coding::pi_bridge::PI_PLAN_COMMAND {
            switch_plan_mode(session, cx, plan_argument(args)).await?;
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }
        if let Some(result) = run_command(session, cx, name, args).await {
            result?;
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }
        // EXP-758: `/exp-plan` is not the only command pi runs itself. EVERY
        // advertised `source: extension` command is dispatched inside pi with
        // NO turn behind it, so the same route applies: send the rpc `prompt`,
        // answer off its response, and never `enter_turn`: a turn entered
        // here would stay active forever and turn every later message into a
        // `steer` aimed at nothing. Prompt- and skill-sourced commands ARE
        // message text and fall through to the real turn below.
        if session.is_extension_command(name) {
            session
                .request("prompt", json!({ "message": text.trim() }))
                .await?;
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let (starting, waiter_id) = session.enter_turn(sender);
    let mut params = json!({ "message": text });
    if !images.is_empty() {
        params["images"] = Value::Array(images);
    }
    // Mid-turn steering is pi's `steer` verb; a fresh turn is `prompt`. Both
    // settle on the SAME `agent_settled`, which is why every waiter parks on
    // one turn.
    let verb = if starting { "prompt" } else { "steer" };
    if let Err(error) = session.request(verb, params).await {
        // EXP-758: only the message that OPENED the turn may settle it. A
        // steer that never reached pi leaves the running turn (and the
        // request that started it) alone: cancelling here answered the first
        // prompt with `Cancelled` while pi was still working, and flipped
        // `turn.active` off under it.
        if starting {
            session.settle(StopReason::Cancelled);
        } else {
            session.leave_turn(waiter_id);
        }
        return Err(error);
    }
    Ok(PromptResponse::new(
        receiver.await.unwrap_or(StopReason::EndTurn),
    ))
}

/// EXP-752: `session/set_mode`. pi has no mode verb, so the switch is the
/// plan extension's `/exp-plan on|off` sent as a `prompt` — pi runs a
/// registered extension command instead of prompting the model with it.
///
/// Two refusals, both deliberate: a session that never loaded the extension
/// has no modes at all, and a switch MID-TURN would be queued into the
/// running turn (pi's own streaming rules) and take effect at the wrong
/// moment — the caller waits for the turn instead. Both live in
/// [`switch_plan_mode`], which a TYPED `/exp-plan` takes too.
async fn set_mode(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    request: SetSessionModeRequest,
) -> Result<SetSessionModeResponse, Error> {
    if guard(&session.mode).is_none() {
        return Err(Error::invalid_request().data(json!("pi has no session modes")));
    }
    let requested = request.mode_id.0.to_string();
    let argument = match requested.as_str() {
        MODE_PLAN => "on",
        MODE_DEFAULT => "off",
        other => {
            return Err(Error::invalid_params()
                .data(json!(format!("pi has no session mode {other}"))))
        }
    };
    switch_plan_mode(session, cx, argument).await?;
    Ok(SetSessionModeResponse::new())
}

/// The ONE plan switch, shared by `session/set_mode` and a typed
/// `/exp-plan`: send the extension command as a `prompt` (pi runs it and
/// starts no turn), then take the session to the mode it leaves behind and
/// tell the client. `argument` is pi's own `on`/`off`.
async fn switch_plan_mode(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    argument: &str,
) -> Result<(), Error> {
    if guard(&session.mode).is_none() {
        return Err(Error::invalid_request().data(json!("pi has no session modes")));
    }
    if guard(&session.turn).active {
        return Err(Error::invalid_request()
            .data(json!("pi plan mode switches between turns only")));
    }
    session
        .request(
            "prompt",
            json!({
                "message": format!("/{} {argument}", coding::pi_bridge::PI_PLAN_COMMAND),
            }),
        )
        .await?;
    session.enter_mode(cx, if argument == "off" { MODE_DEFAULT } else { MODE_PLAN });
    Ok(())
}

/// The `on`/`off` a typed `/exp-plan <args>` means, by the extension's OWN
/// rule (`.trim() !== "off"` turns planning on), so a bare `/exp-plan` and
/// anything unrecognised land where pi would put them.
fn plan_argument(args: &str) -> &'static str {
    if args.trim() == "off" {
        "off"
    } else {
        "on"
    }
}

async fn set_config_option(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    request: SetSessionConfigOptionRequest,
) -> Result<SetSessionConfigOptionResponse, Error> {
    let id = request.config_id.0.to_string();
    let text = request
        .value
        .as_value_id()
        .map(|value| value.0.to_string())
        .unwrap_or_default();
    let toggle = request.value.as_bool();
    match id.as_str() {
        // A BLANK value is the wire's "CLI default" (D4) and pi has no verb
        // for "unset": report the options back unchanged instead of sending
        // an empty model id.
        _ if text.is_empty() && toggle.is_none() => {}
        CONFIG_MODEL => {
            let (provider, model_id) = PiModel::split_value_id(&text);
            session
                .request(
                    "set_model",
                    json!({ "provider": provider, "modelId": model_id }),
                )
                .await?;
        }
        CONFIG_THINKING => {
            session
                .request("set_thinking_level", json!({ "level": text }))
                .await?;
        }
        CONFIG_STEERING => {
            session
                .request("set_steering_mode", json!({ "mode": text }))
                .await?;
        }
        CONFIG_FOLLOW_UP => {
            session
                .request("set_follow_up_mode", json!({ "mode": text }))
                .await?;
        }
        CONFIG_AUTO_COMPACTION => {
            let enabled = toggle.unwrap_or(text == "true");
            session
                .request("set_auto_compaction", json!({ "enabled": enabled }))
                .await?;
        }
        other => {
            return Err(Error::invalid_params()
                .data(json!(format!("pi has no config option {other}"))))
        }
    }
    Ok(SetSessionConfigOptionResponse::new(
        session.refresh_state(cx).await,
    ))
}

/// Run one of pi's verb-only commands ([`SYNTHESIZED_COMMANDS`]). `None` =
/// not one of ours. EXP-758: what the caller does then depends on pi's own
/// `source`. An `extension` command is dispatched by pi and starts no turn,
/// while a `prompt` or `skill` command IS message text and starts a real one.
async fn run_command(
    session: &Arc<PiSession>,
    cx: &ConnectionTo<Client>,
    name: &str,
    args: &str,
) -> Option<Result<(), Error>> {
    let args = args.trim();
    let result = match name {
        "compact" => {
            let params = if args.is_empty() {
                json!({})
            } else {
                json!({ "customInstructions": args })
            };
            session.request("compact", params).await.map(|_| ())
        }
        "new" => session.request("new_session", json!({})).await.map(|_| ()),
        "model" => {
            let (provider, model_id) = PiModel::split_value_id(args);
            session
                .request(
                    "set_model",
                    json!({ "provider": provider, "modelId": model_id }),
                )
                .await
                .map(|_| ())
        }
        "thinking" => session
            .request("set_thinking_level", json!({ "level": args }))
            .await
            .map(|_| ()),
        "name" => session
            .request("set_session_name", json!({ "name": args }))
            .await
            .map(|_| ()),
        "fork" => session
            .request("fork", json!({ "entryId": args }))
            .await
            .map(|_| ()),
        "clone" => session.request("clone", json!({})).await.map(|_| ()),
        "export" => {
            let params = if args.is_empty() {
                json!({})
            } else {
                json!({ "outputPath": args })
            };
            session.request("export_html", params).await.map(|_| ())
        }
        _ => return None,
    };
    if result.is_ok() && matches!(name, "model" | "thinking" | "new") {
        session.refresh_state(cx).await;
    }
    Some(result)
}

/// `get_commands` plus the verbs pi never advertises, published as one
/// `available_commands_update` (the `/` menu unions this with the contract
/// commands on the client side).
async fn publish_commands(session: &Arc<PiSession>, cx: &ConnectionTo<Client>) {
    let advertised = session
        .request("get_commands", json!({}))
        .await
        .map(|data| pi_wire::parse_commands(&data))
        .unwrap_or_default();
    let mut commands: Vec<AvailableCommand> = SYNTHESIZED_COMMANDS
        .iter()
        .map(|(name, description, hint)| {
            let command = AvailableCommand::new(*name, *description);
            if hint.is_empty() {
                command
            } else {
                command.input(AvailableCommandInput::Unstructured(
                    UnstructuredCommandInput::new(*hint),
                ))
            }
        })
        .collect();
    // EXP-758: remember which of them pi runs ITSELF, before the dedup drops
    // any of the names. A `prompt` carrying one of these gets no turn back.
    // A name that also names a verb-only command keeps the verb (the dedup
    // below), and `run_command` is consulted first, so the two never collide.
    *guard(&session.extension_commands) = advertised
        .iter()
        .filter(|command| command.is_extension())
        .map(|command| command.name.clone())
        .collect();
    for command in advertised {
        if commands.iter().any(|known| known.name == command.name) {
            continue;
        }
        // EXP-752: `/exp-plan` is OUR mode switch, not a command a person
        // should type — the mode picker owns it, and typing it would leave
        // the advertised mode lying.
        if command.name == coding::pi_bridge::PI_PLAN_COMMAND {
            continue;
        }
        commands.push(AvailableCommand::new(command.name, command.description));
    }
    session.notify(
        cx,
        SessionUpdate::AvailableCommandsUpdate(
            agent_client_protocol::schema::v1::AvailableCommandsUpdate::new(commands),
        ),
    );
}

// ---------------------------------------------------------------------------
// Pure helpers (the testable half)
// ---------------------------------------------------------------------------

/// EXP-752: a session's starting mode — `plan` when the launch loaded the
/// plan extension, and NO modes at all otherwise.
fn initial_mode(plan_mode: bool) -> Option<String> {
    plan_mode.then(|| MODE_PLAN.to_string())
}

/// The advertised mode state for a mode slot (`None` = this session has no
/// modes; see [`initial_mode`]).
fn mode_state(mode: &Option<String>) -> Option<SessionModeState> {
    let current = mode.clone()?;
    Some(SessionModeState::new(
        SessionModeId::new(current),
        available_modes(),
    ))
}

/// EXP-752: the two modes a plan-mode pi session offers. `default` is pi's
/// ordinary posture (pi has no permission system of its own), `plan` is the
/// extension's gate; the ids match claude's so one picker renders both.
fn available_modes() -> Vec<SessionMode> {
    vec![
        SessionMode::new(SessionModeId::new(MODE_DEFAULT), "Manual")
            .description("Run without the plan gate"),
        SessionMode::new(SessionModeId::new(MODE_PLAN), "Plan")
            .description("Create a plan before making changes"),
    ]
}

/// The full config snapshot for a state + model list.
fn config_options(state: &PiState, models: &[PiModel]) -> Vec<SessionConfigOption> {
    let mut options = Vec::new();
    let current_model = state
        .model
        .as_ref()
        .map(PiModel::value_id)
        .unwrap_or_default();
    let mut values: Vec<SessionConfigSelectOption> = models
        .iter()
        .map(|model| SessionConfigSelectOption::new(model.value_id(), model.name.clone()))
        .collect();
    if !current_model.is_empty()
        && !values.iter().any(|value| value.value.0.as_ref() == current_model)
    {
        // The running model always appears, even when `get_available_models`
        // does not list it (a scoped or provider-side model) — a select whose
        // current value is absent renders empty on every client.
        let label = state
            .model
            .as_ref()
            .map(|model| model.name.clone())
            .unwrap_or_else(|| current_model.clone());
        values.insert(
            0,
            SessionConfigSelectOption::new(current_model.clone(), label),
        );
    }
    if !values.is_empty() {
        options.push(
            SessionConfigOption::new(
                CONFIG_MODEL,
                "Model",
                SessionConfigKind::Select(SessionConfigSelect::new(current_model, values)),
            )
            .category(SessionConfigOptionCategory::Model),
        );
    }
    if !state.thinking_level.is_empty() {
        options.push(
            SessionConfigOption::new(
                CONFIG_THINKING,
                "Thinking",
                SessionConfigKind::Select(SessionConfigSelect::new(
                    state.thinking_level.clone(),
                    select_values(&THINKING_LEVELS),
                )),
            )
            .category(SessionConfigOptionCategory::ThoughtLevel),
        );
    }
    if !state.steering_mode.is_empty() {
        options.push(SessionConfigOption::new(
            CONFIG_STEERING,
            "Steering",
            SessionConfigKind::Select(SessionConfigSelect::new(
                state.steering_mode.clone(),
                select_values(&QUEUE_MODES),
            )),
        ));
    }
    if !state.follow_up_mode.is_empty() {
        options.push(SessionConfigOption::new(
            CONFIG_FOLLOW_UP,
            "Follow-up",
            SessionConfigKind::Select(SessionConfigSelect::new(
                state.follow_up_mode.clone(),
                select_values(&QUEUE_MODES),
            )),
        ));
    }
    options.push(SessionConfigOption::new(
        CONFIG_AUTO_COMPACTION,
        "Auto-compaction",
        SessionConfigKind::Boolean(SessionConfigBoolean::new(state.auto_compaction)),
    ));
    options
}

fn select_values(values: &[&str]) -> Vec<SessionConfigSelectOption> {
    values
        .iter()
        .map(|value| SessionConfigSelectOption::new(value.to_string(), value.to_string()))
        .collect()
}

/// pi's builtin tools plus the shape most MCP tools take. Unknown names are
/// `Other`, never a guess: the kind drives the icon AND whether we snapshot
/// a file.
fn tool_kind(name: &str) -> ToolKind {
    match name.to_ascii_lowercase().as_str() {
        "read" => ToolKind::Read,
        "write" | "edit" | "multi_edit" | "apply_patch" | "str_replace" => ToolKind::Edit,
        "bash" | "shell" | "run" => ToolKind::Execute,
        "ls" | "find" | "grep" | "glob" | "search" => ToolKind::Search,
        "fetch" | "web_fetch" | "web_search" => ToolKind::Fetch,
        "think" | "todo" | "todowrite" => ToolKind::Think,
        _ => ToolKind::Other,
    }
}

fn is_edit(kind: ToolKind) -> bool {
    kind == ToolKind::Edit
}

/// The file a tool call names, resolved against the worktree. pi's builtins
/// use `path`; MCP tools commonly use `file_path`.
fn tool_path(args: &Value, cwd: &Path) -> Option<PathBuf> {
    let raw = args
        .get("path")
        .or_else(|| args.get("file_path"))
        .or_else(|| args.get("filePath"))
        .and_then(Value::as_str)?;
    if raw.is_empty() {
        return None;
    }
    let path = Path::new(raw);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    })
}

/// A tool result rendered as text: a bash result is an object, a simple tool
/// answers with a string.
fn result_text(result: &Value) -> Option<String> {
    match result {
        Value::String(text) => Some(text.clone()),
        Value::Object(fields) => fields
            .get("output")
            .or_else(|| fields.get("stdout"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn output_content(text: &str) -> ToolCallContent {
    ToolCallContent::Content(Content::new(ContentBlock::from(first_chars(
        text,
        OUTPUT_CHUNK_MAX,
    ))))
}

fn text_chunk(message_id: Option<String>, text: String) -> ContentChunk {
    let chunk = ContentChunk::new(ContentBlock::from(text));
    match message_id {
        Some(id) => chunk.message_id(Some(
            agent_client_protocol::schema::v1::MessageId::new(id),
        )),
        None => chunk,
    }
}

/// `used`/`size` for one turn's usage. `size` is the model's context window;
/// with no model in hand there is no denominator and no update (a `usage`
/// event with a zero size clears the bar on every client).
///
/// EXP-758: the COST is cumulative. pi reports what the turn that just ended
/// cost, but ACP documents `UsageUpdate.cost` as the cost of the SESSION and
/// every client renders exactly one figure, so the running total is added up
/// here: a per-turn number there reads as a session that got cheaper.
fn usage_update(session: &PiSession, usage: &PiUsage) -> Option<UsageUpdate> {
    let size = guard(&session.state)
        .model
        .as_ref()
        .map(|model| model.context_window)
        .unwrap_or_default();
    // The total is banked even when there is no denominator to report it
    // with, so the next turn that HAS a model reports the whole session.
    let total = usage.cost_usd.map(|cost| {
        let mut total = guard(&session.cost_usd);
        *total += cost;
        *total
    });
    if size == 0 {
        return None;
    }
    let update = UsageUpdate::new(usage.total_tokens, size);
    Some(match total {
        // pi is the ONE agent that reports money.
        Some(total) => update.cost(Cost::new(total, "USD")),
        None => update,
    })
}

fn trigger_meta(reason: Option<&str>) -> Option<serde_json::Map<String, Value>> {
    let reason = reason?;
    let mut meta = serde_json::Map::new();
    meta.insert(COMPACTION_TRIGGER_META_KEY.to_string(), json!(reason));
    Some(meta)
}

/// Split a prompt into pi's `message` text and its `images`.
fn split_prompt(blocks: &[ContentBlock]) -> (String, Vec<Value>) {
    let mut text = String::new();
    let mut images = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(content) => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&content.text);
            }
            ContentBlock::Image(image) => images.push(json!({
                "type": "image",
                "data": image.data,
                "mimeType": image.mime_type,
            })),
            _ => {}
        }
    }
    (text, images)
}

/// `/name rest` → `("name", "rest")`, on the FIRST line only (a prompt that
/// merely mentions a slash later is a message).
fn slash_command(text: &str) -> Option<(&str, &str)> {
    let first = text.lines().next()?.trim();
    let rest = first.strip_prefix('/')?;
    if text.lines().count() > 1 {
        return None;
    }
    let (name, args) = match rest.split_once(char::is_whitespace) {
        Some((name, args)) => (name, args),
        None => (rest, ""),
    };
    (!name.is_empty()).then_some((name, args))
}

/// `get_entries` → the replayed transcript. Only message entries carry
/// conversation; the rest (thinking-level changes, labels, custom extension
/// state) are persistence details.
fn replay_updates(entries: &Value) -> Vec<SessionUpdate> {
    let Some(entries) = entries.get("entries").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut updates = Vec::new();
    for entry in entries {
        if entry.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let message = &entry["message"];
        let role = message.get("role").and_then(Value::as_str).unwrap_or_default();
        let text = message
            .get("content")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let chunk = text_chunk(pi_wire::message_key(message), text);
        updates.push(match role {
            "user" => SessionUpdate::UserMessageChunk(chunk),
            _ => SessionUpdate::AgentMessageChunk(chunk),
        });
    }
    updates
}

/// Truncate on a CHARACTER boundary (never a byte split mid-codepoint).
fn first_chars(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PiState {
        PiState {
            model: Some(PiModel {
                provider: "openai-codex".to_string(),
                id: "gpt-5.4".to_string(),
                name: "GPT-5.4".to_string(),
                context_window: 272_000,
            }),
            thinking_level: "medium".to_string(),
            steering_mode: "one-at-a-time".to_string(),
            follow_up_mode: "all".to_string(),
            auto_compaction: true,
            session_file: Some("/sessions/run.jsonl".to_string()),
            session_id: "01a0".to_string(),
            session_name: None,
        }
    }

    #[test]
    fn the_config_snapshot_carries_every_switchable_pi_setting() {
        let models = vec![PiModel {
            provider: "anthropic".to_string(),
            id: "claude-opus-4-6".to_string(),
            name: "Claude Opus".to_string(),
            context_window: 200_000,
        }];
        let options = config_options(&state(), &models);
        let ids: Vec<String> = options.iter().map(|option| option.id.0.to_string()).collect();
        assert_eq!(
            ids,
            vec![
                "model",
                "thinking_level",
                "steering_mode",
                "follow_up_mode",
                "auto_compaction"
            ]
        );
        // The RUNNING model is always selectable, even when the list omits it.
        let model = &options[0];
        match &model.kind {
            SessionConfigKind::Select(select) => {
                assert_eq!(select.current_value.0.as_ref(), "openai-codex/gpt-5.4");
                let values = match &select.options {
                    agent_client_protocol::schema::v1::SessionConfigSelectOptions::Ungrouped(
                        values,
                    ) => values,
                    other => panic!("expected ungrouped values, got {other:?}"),
                };
                assert_eq!(values[0].value.0.as_ref(), "openai-codex/gpt-5.4");
                assert_eq!(values[1].value.0.as_ref(), "anthropic/claude-opus-4-6");
            }
            other => panic!("expected a select, got {other:?}"),
        }
        assert_eq!(model.category, Some(SessionConfigOptionCategory::Model));
        assert_eq!(
            options[1].category,
            Some(SessionConfigOptionCategory::ThoughtLevel)
        );
        // pi has no modes, so nothing here claims a Mode category.
        assert!(options
            .iter()
            .all(|option| option.category != Some(SessionConfigOptionCategory::Mode)));
    }

    #[test]
    fn a_state_without_a_model_still_yields_options() {
        let bare = PiState { model: None, ..state() };
        let options = config_options(&bare, &[]);
        let ids: Vec<String> = options.iter().map(|option| option.id.0.to_string()).collect();
        assert_eq!(
            ids,
            vec![
                "thinking_level",
                "steering_mode",
                "follow_up_mode",
                "auto_compaction"
            ]
        );
    }

    #[test]
    fn tool_kinds_and_paths_come_off_the_arguments() {
        assert_eq!(tool_kind("edit"), ToolKind::Edit);
        assert_eq!(tool_kind("Write"), ToolKind::Edit);
        assert_eq!(tool_kind("bash"), ToolKind::Execute);
        assert_eq!(tool_kind("grep"), ToolKind::Search);
        assert_eq!(tool_kind("mcp__exponential__issues_get"), ToolKind::Other);
        let cwd = Path::new("/work/tree");
        assert_eq!(
            tool_path(&json!({ "path": "src/lib.rs" }), cwd),
            Some(PathBuf::from("/work/tree/src/lib.rs"))
        );
        assert_eq!(
            tool_path(&json!({ "file_path": "/abs/main.rs" }), cwd),
            Some(PathBuf::from("/abs/main.rs"))
        );
        assert_eq!(tool_path(&json!({ "command": "ls" }), cwd), None);
    }

    #[test]
    fn only_a_verb_only_command_is_intercepted() {
        assert_eq!(slash_command("/compact"), Some(("compact", "")));
        assert_eq!(slash_command("  /name my run "), Some(("name", "my run")));
        assert_eq!(slash_command("fix the /compact bug"), None);
        // A multi-line message is a message, even when it opens with a slash.
        assert_eq!(slash_command("/compact\nand then ship it"), None);
        assert_eq!(slash_command("/"), None);
    }

    #[test]
    fn a_typed_plan_command_reads_as_the_extensions_own_argument() {
        // The extension turns planning on for ANYTHING but `off`, so a bare
        // `/exp-plan` (and a typo) has to land the same way here.
        assert_eq!(plan_argument(""), "on");
        assert_eq!(plan_argument(" on "), "on");
        assert_eq!(plan_argument("please"), "on");
        assert_eq!(plan_argument(" off "), "off");
        // The switch is reached through `slash_command`, which trims the line.
        assert_eq!(
            slash_command("  /exp-plan off  "),
            Some((coding::pi_bridge::PI_PLAN_COMMAND, "off"))
        );
    }

    #[test]
    fn a_resume_handle_only_resolves_to_a_path() {
        assert_eq!(
            resume_session_file(Some(&ResumeHandle::PiSessionFile(PathBuf::from(
                "/sessions/a.jsonl"
            )))),
            Some(PathBuf::from("/sessions/a.jsonl"))
        );
        // pi's ACP session id IS its session file (D8), so an Acp handle
        // resumes the same run.
        assert_eq!(
            resume_session_file(Some(&ResumeHandle::Acp("/sessions/a.jsonl".to_string()))),
            Some(PathBuf::from("/sessions/a.jsonl"))
        );
        // A claude uuid landing here would create a junk file named after it.
        assert_eq!(
            resume_session_file(Some(&ResumeHandle::Native(
                "9c1b0f6e-0000-4000-8000-000000000000".to_string()
            ))),
            None
        );
        assert_eq!(resume_session_file(None), None);
    }

    #[test]
    fn a_prompt_splits_into_pis_message_and_images() {
        let blocks = vec![
            ContentBlock::from("first"),
            ContentBlock::Image(
                agent_client_protocol::schema::v1::ImageContent::new("QUJD", "image/png"),
            ),
            ContentBlock::from("second"),
        ];
        let (text, images) = split_prompt(&blocks);
        assert_eq!(text, "first\nsecond");
        assert_eq!(
            images,
            vec![json!({ "type": "image", "data": "QUJD", "mimeType": "image/png" })]
        );
    }

    #[test]
    fn a_transcript_replays_as_user_and_agent_chunks() {
        let entries = json!({
            "entries": [
                { "type": "session_info", "name": "run" },
                {
                    "type": "message",
                    "message": {
                        "role": "user",
                        "content": [{ "type": "text", "text": "hello" }],
                        "timestamp": 1
                    }
                },
                {
                    "type": "message",
                    "message": {
                        "role": "assistant",
                        "content": [
                            { "type": "thinking", "thinking": "hmm" },
                            { "type": "text", "text": "hi" }
                        ],
                        "responseId": "resp-1"
                    }
                },
                { "type": "message", "message": { "role": "assistant", "content": [] } }
            ]
        });
        let updates = replay_updates(&entries);
        assert_eq!(updates.len(), 2);
        assert!(matches!(updates[0], SessionUpdate::UserMessageChunk(_)));
        match &updates[1] {
            SessionUpdate::AgentMessageChunk(chunk) => {
                assert_eq!(
                    serde_json::to_value(&chunk.content).expect("the chunk serializes"),
                    json!({ "type": "text", "text": "hi" })
                );
                assert_eq!(
                    chunk.message_id.as_ref().map(|id| id.0.to_string()),
                    Some("resp-1".to_string())
                );
            }
            other => panic!("expected an agent chunk, got {other:?}"),
        }
    }

    #[test]
    fn the_compaction_trigger_rides_the_meta_because_acp_has_no_field_for_it() {
        let meta = trigger_meta(Some("threshold")).expect("a reason produces meta");
        assert_eq!(
            serde_json::to_value(&meta).expect("meta serializes"),
            json!({ "trigger": "threshold" })
        );
        assert!(trigger_meta(None).is_none());
    }

    fn pi_spec(plan_mode: bool) -> AdapterSpec {
        let mut options = coding::LaunchOptions::defaults(&coding::Settings::default());
        options.agent = coding::CodingAgent::Pi;
        options.model = "openai-codex/gpt-5.4".to_string();
        options.effort = "high".to_string();
        options.plan_mode = plan_mode;
        AdapterSpec {
            kind: super::super::AdapterKind::Pi,
            agent: coding::AgentKind::Builtin(coding::CodingAgent::Pi),
            spawn: terminal::pty::SpawnSpec::new("pi"),
            options,
            mcp: coding::AgentMcp::PiExtension,
            cwd: PathBuf::from("/work/tree"),
            session_id: "sess-1".to_string(),
            prompt: None,
            resume: Some(ResumeHandle::PiSessionFile(PathBuf::from("/s/run.jsonl"))),
            personal_key: None,
            reaper_settings_path: None,
            exit: crate::ChildExitLink::new(),
        }
    }

    #[test]
    fn the_rpc_argv_drops_the_observer_and_plan_extensions() {
        assert_eq!(
            pi_argv_for(&pi_spec(false)),
            vec![
                "--mode",
                "rpc",
                "--model",
                "openai-codex/gpt-5.4",
                "--thinking",
                "high",
                "--session",
                "/s/run.jsonl",
                "-e",
                "./.exp-pi-mcp.ts",
                // EXP-763: the run playbook, last.
                "--append-system-prompt",
                coding::skill::RUN_SKILL,
            ]
        );
    }

    /// EXP-752: the plan extension is pi's plan mode on this transport too —
    /// but only when the launch asked for it. Its absence is what makes an
    /// ordinary rpc session mode-less.
    #[test]
    fn the_rpc_argv_adds_the_plan_extension_only_in_plan_mode() {
        let planning = pi_argv_for(&pi_spec(true));
        assert_eq!(
            planning.iter().rev().take(6).rev().collect::<Vec<_>>(),
            vec![
                "-e",
                "./.exp-pi-mcp.ts",
                "-e",
                "./.exp-pi-plan.ts",
                "--append-system-prompt",
                coding::skill::RUN_SKILL,
            ]
        );
        assert!(!pi_argv_for(&pi_spec(false))
            .iter()
            .any(|arg| arg.contains(coding::pi_bridge::PI_PLAN_FILE)));
    }

    /// The mode surface exists ONLY for a plan-mode launch: pi has no native
    /// modes, so a picker over nothing would be a lie.
    #[test]
    fn only_a_plan_mode_launch_advertises_modes() {
        let state = mode_state(&initial_mode(true)).expect("a plan launch has modes");
        assert_eq!(state.current_mode_id.0.as_ref(), "plan");
        let ids: Vec<String> = state
            .available_modes
            .iter()
            .map(|mode| mode.id.0.to_string())
            .collect();
        assert_eq!(ids, vec!["default", "plan"]);
        assert!(mode_state(&initial_mode(false)).is_none());
    }
}
