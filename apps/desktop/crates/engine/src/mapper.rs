//! EXP-746 — the ONE `SessionUpdate` → `ActivityEvent` path.
//!
//! Pure: no channels, no sockets, no clock beyond what the caller passes in,
//! so every adapter lane can test its frames against it with a fixture and a
//! `Vec`. Three rules the fillers must not soften:
//!
//! 1. **Everything on the wire is derived, redacted and capped.** ACP hands
//!    us `raw_input`, `raw_output` and `ToolCallContent::Diff` — real command
//!    strings and patch bodies. Forwarding any of them verbatim would leak
//!    strictly more to the relay (and thus web/iOS/Android) than the PTY path
//!    ever did. `Tool { detail }` is a derivation (path / pattern / first
//!    token / `input.description`), then `Redactor::redact`, then
//!    `truncate`/`truncate_marked` at the existing caps in UTF-8 BYTES.
//! 2. **Chunks coalesce.** ACP streams are ~100x denser than the 1 s
//!    transcript poll and the journal evicts at 2000 events / 4 MiB; narration
//!    and thought chunks coalesce by `message_id` and flush on idle or turn
//!    end, `ToolCallUpdate` status transitions never publish, and
//!    `config_state`/`usage` are latest-wins slots.
//! 3. **Question ids are ACP ids** (D3): `id` = the tool-call id for a
//!    permission, `<ask>#<n>` / `<ask>#submit` for an elicitation stepper, and
//!    `QuestionOption::key` is the ACP option id — never a keystroke.
//!
//! Every emit goes through [`emit`], which puts the SAME event on the wire
//! list and on the local list (as [`LocalFeedEvent::Activity`]) — the desktop
//! feed and the relay room are then the same stream by construction, and a
//! transcript replay (which has no relay room at all) renders identically.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    AvailableCommand, ContentBlock, ContentChunk, CreateElicitationRequest, ElicitationMode,
    ElicitationPropertySchema, PermissionOption, PermissionOptionKind, RequestPermissionRequest,
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions,
    SessionModeState, SessionNotification, SessionUpdate, StopReason, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use serde_json::{Map, Value};
use steer::activity::{
    ANSWERS_MAX, ANSWER_MAX, AGENT_TYPE_MAX, ID_MAX, NARRATION_MAX, OPTION_DESCRIPTION_MAX,
    OPTION_LABEL_MAX, QUESTION_HEADER_MAX, QUESTION_TEXT_MAX, TOOL_DETAIL_MAX, TOOL_NAME_MAX,
};
use steer::frames::CompactionPhase;
use steer::{ActivityEvent, QuestionOption};

use crate::local::{
    EnginePhase, LocalFeedEvent, PlanEntryPriorityView, PlanEntryStatusView, PlanEntryView,
    SubagentEdge, SubagentEdgeStatus, ToolCardKind, ToolCardStatus,
};

/// How long a coalescer holds a partial message before [`Mapper::flush`]
/// publishes it. A turn end flushes unconditionally, so this only bounds the
/// gap INSIDE a turn (rule 2).
pub const FLUSH_IDLE: Duration = Duration::from_millis(250);

/// How long a started compaction may stay open before the mapper closes it
/// itself. Mirrors `steer::activity::COMPACTION_MAX` (crate-private there);
/// the four clients hold their "compacting" strip until the `ended` edge, so
/// an agent that dies mid-compaction must not strand it.
pub const COMPACTION_MAX: Duration = Duration::from_secs(300);

/// The `_meta` key an adapter stamps a subagent edge under (see
/// [`SubagentEdge`]).
pub use crate::local::{COMPACTION_TRIGGER_META_KEY, SUBAGENT_ID_META_KEY, SUBAGENT_META_KEY};

/// Everything the mapper needs that is constant for a session.
pub struct MapperConfig {
    /// Session secrets (installation token, the `expu_` personal key) plus the
    /// static patterns — every wire string passes through it. Shared
    /// (`SessionCtx.redactor`) so the lifecycle's `diff` ticker masks with the
    /// SAME secret set as the mapper (REV2-17).
    pub redactor: Arc<steer::Redactor>,
    /// The worktree, for relativizing tool-call paths.
    pub cwd: std::path::PathBuf,
    /// Keys the command catalog and the codex sigil guard; `External` is
    /// deliberately neutral.
    pub agent: steer::SessionAgent,
    /// Seed for `steer::synthetic_question_id` when the agent stamps no id.
    pub session_seed: String,
}

/// One mapping step's output. Wire events go to the relay through the
/// [`crate::sink::EventSink`]; local events go to the host's `LocalSink`;
/// the two flags drive `coding_sessions.needs_input` (EXP-214) and
/// `steer::TurnSignal`.
#[derive(Default)]
pub struct MapOut {
    pub wire: Vec<steer::ActivityEvent>,
    pub local: Vec<LocalFeedEvent>,
    pub needs_input: Option<bool>,
    pub idle: Option<bool>,
}

/// Identifies one parked ask so an inbound `answer` frame can find the ACP
/// `Responder` it must resolve. `question_id` is what every client echoes
/// back; `ask_id` is set only for an elicitation stepper's steps.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PendingAskKey {
    pub question_id: String,
    pub ask_id: Option<String>,
}

impl PendingAskKey {
    /// The key the HOST parks the ACP `Responder` under: one responder per
    /// ELICITATION (all its steps resolve the same request), one per
    /// permission.
    pub fn responder_key(&self) -> &str {
        self.ask_id.as_deref().unwrap_or(&self.question_id)
    }
}

/// What the host should do with the parked `Responder` for an answer.
#[derive(Clone, Debug, PartialEq)]
pub enum AnswerDecision {
    /// Resolve a `session/request_permission` with this ACP option id.
    Permission { option_id: String },
    /// Record one elicitation step. `submit` = this was the final step, so
    /// the form goes back as `ElicitationAction::Accept`.
    Elicitation {
        fields: serde_json::Value,
        submit: bool,
    },
    /// Already answered — re-ack only, never re-resolve (EXP-374).
    ReAck,
    /// Nothing is parked under that key; the answer is dropped.
    Unknown,
}

/// The stateful half: chunk coalescers, the tool table, the subagent table,
/// compaction state and the pending-ask registry.
pub struct Mapper {
    config: MapperConfig,
    /// Agent message chunks, coalesced by `message_id`.
    message: Coalescer,
    /// Agent THOUGHT chunks — a separate buffer so a thought never merges
    /// into the visible answer.
    thought: Coalescer,
    /// User message chunks (an agent replaying what it was sent).
    user: Coalescer,
    /// Prompts the HOST already published as `user_message` (it sends them,
    /// so it need not wait for an echo): an agent that replays the user's
    /// message (claude's `--replay-user-messages`) is deduped against this.
    pending_echoes: std::collections::VecDeque<String>,
    tools: HashMap<String, ToolState>,
    subagents: HashMap<String, String>,
    permissions: HashMap<String, PermissionAsk>,
    elicitations: HashMap<String, ElicitationAsk>,
    /// question id → which ask owns it (a stepper registers one per step).
    questions: HashMap<String, AskRef>,
    /// The live `config_state` snapshot — always published WHOLE (D4).
    config_state: ConfigSnapshot,
    compacting_since: Option<Instant>,
    /// Disambiguates two synthetic ids whose text is identical.
    ordinal: u32,
}

#[derive(Default)]
struct ConfigSnapshot {
    options: Vec<steer::ConfigOption>,
    current_mode: Option<String>,
    modes: Option<Vec<steer::ConfigMode>>,
    commands: Option<Vec<steer::ConfigCommand>>,
}

struct ToolState {
    title: String,
    kind: ToolKind,
}

struct PermissionAsk {
    /// (option key, label) in published order — the answer's labels come
    /// from here so `question_resolved` reads like the card did.
    options: Vec<(String, String)>,
    answered: bool,
}

struct ElicitationAsk {
    steps: Vec<ElicitStep>,
    /// The step a fresh answer applies to; `steps.len()` = the submit step.
    current: usize,
    fields: Map<String, Value>,
    answered: bool,
    message: String,
}

struct ElicitStep {
    property: String,
    text: String,
    /// The property's title when it is not already the card text.
    header: Option<String>,
    options: Vec<QuestionOption>,
    multi_select: bool,
    kind: StepKind,
    /// A free-text sibling folded into this step (claude's
    /// `question_<n>_custom`): the card grows a "Type something." row and a
    /// typed answer lands on THIS property instead of the choice.
    custom_property: Option<String>,
}

/// How a step's answer is folded back into the elicitation's `content` map.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StepKind {
    Text,
    Bool,
    Integer,
    Number,
    Strings,
}

enum AskRef {
    Permission(String),
    Elicitation(String),
}

/// The chunk coalescer of rule 2: one buffer per `message_id`, flushed when
/// the id changes, when the turn ends, or after [`FLUSH_IDLE`].
#[derive(Default)]
struct Coalescer {
    message_id: Option<String>,
    buf: String,
    since: Option<Instant>,
}

impl Coalescer {
    /// Returns the text of a buffer that has to flush BEFORE this chunk (a
    /// new `message_id` starts a new message).
    fn push(&mut self, message_id: Option<String>, text: &str) -> Option<String> {
        let boundary = self.message_id != message_id && !self.buf.is_empty();
        let flushed = boundary.then(|| std::mem::take(&mut self.buf));
        self.message_id = message_id;
        self.buf.push_str(text);
        self.since = Some(Instant::now());
        flushed
    }

    fn take(&mut self) -> Option<String> {
        self.since = None;
        self.message_id = None;
        (!self.buf.is_empty()).then(|| std::mem::take(&mut self.buf))
    }

    /// Take only if the buffer has been quiet for [`FLUSH_IDLE`].
    fn take_if_idle(&mut self) -> Option<String> {
        match self.since {
            Some(since) if since.elapsed() >= FLUSH_IDLE => self.take(),
            _ => None,
        }
    }
}

/// Put one event on BOTH streams: the relay's and the host's local feed.
/// `tool_call_id` names the ACP tool call the event belongs to so the desktop
/// can hang a diff/output card off the feed row this appends.
fn emit(out: &mut MapOut, event: ActivityEvent, tool_call_id: Option<String>) {
    out.local.push(LocalFeedEvent::Activity {
        event: event.clone(),
        tool_call_id,
    });
    out.wire.push(event);
}

impl Mapper {
    pub fn new(config: MapperConfig) -> Self {
        Self {
            config,
            message: Coalescer::default(),
            thought: Coalescer::default(),
            user: Coalescer::default(),
            pending_echoes: std::collections::VecDeque::new(),
            tools: HashMap::new(),
            subagents: HashMap::new(),
            permissions: HashMap::new(),
            elicitations: HashMap::new(),
            questions: HashMap::new(),
            config_state: ConfigSnapshot::default(),
            compacting_since: None,
            ordinal: 0,
        }
    }

    /// The session's constants (redactor, cwd, agent, seed).
    pub fn config(&self) -> &MapperConfig {
        &self.config
    }

    /// Redact, then truncate at `max` UTF-8 bytes — the order rule 1 fixes
    /// (a truncation that cut a secret in half would leave the head of it on
    /// the wire).
    fn clean(&self, text: &str, max: usize) -> String {
        steer::truncate(&self.config.redactor.redact(text), max)
    }

    /// Same, but marks the cut (`[truncated]`) for text a human reads whole:
    /// a plan body, a question.
    fn clean_marked(&self, text: &str, max: usize) -> String {
        steer::truncate_marked(&self.config.redactor.redact(text), max)
    }

    /// The main path: one `session/update` notification.
    pub fn on_update(&mut self, notification: &SessionNotification, out: &mut MapOut) {
        // An adapter stamps its `_meta` on whichever carrier is natural to it:
        // the notification (claude, whose edges ride no-op patches) or the
        // update itself (codex, whose edges ride the subagent's own card).
        if let Some(edge) = notification
            .meta
            .as_ref()
            .and_then(SubagentEdge::from_meta)
            .or_else(|| update_meta(&notification.update).and_then(SubagentEdge::from_meta))
        {
            self.on_subagent(&edge, out);
        }
        // The three coalescers join chunks of ONE message each; a chunk of
        // another kind ends whatever the other two hold, so a replay (or any
        // burst inside one flush window) keeps its user → thought → message
        // interleaving instead of grouping by kind at the next flush.
        match &notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                self.flush_message(out);
                self.flush_thought(out);
                if let Some(text) = self.user.push(message_id(chunk), &chunk_text(chunk)) {
                    self.emit_user(&text, out);
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                self.flush_user(out);
                self.flush_thought(out);
                if let Some(text) = self.message.push(message_id(chunk), &chunk_text(chunk)) {
                    self.emit_narration(&text, out);
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                let text = chunk_text(chunk);
                // Recent models stream signature-only thinking blocks: an
                // empty thought is not a thought.
                if text.trim().is_empty() {
                    return;
                }
                self.flush_user(out);
                self.flush_message(out);
                if let Some(flushed) = self.thought.push(message_id(chunk), &text) {
                    self.emit_thought(&flushed, out);
                }
            }
            SessionUpdate::ToolCall(call) => {
                self.on_tool_call(call, notification.meta.as_ref(), out)
            }
            SessionUpdate::ToolCallUpdate(update) => self.on_tool_call_update(update, out),
            SessionUpdate::Plan(plan) => {
                let entries = plan
                    .entries
                    .iter()
                    .map(|entry| PlanEntryView {
                        content: entry.content.clone(),
                        priority: match entry.priority {
                            agent_client_protocol::schema::v1::PlanEntryPriority::High => {
                                PlanEntryPriorityView::High
                            }
                            agent_client_protocol::schema::v1::PlanEntryPriority::Low => {
                                PlanEntryPriorityView::Low
                            }
                            _ => PlanEntryPriorityView::Medium,
                        },
                        status: match entry.status {
                            agent_client_protocol::schema::v1::PlanEntryStatus::InProgress => {
                                PlanEntryStatusView::InProgress
                            }
                            agent_client_protocol::schema::v1::PlanEntryStatus::Completed => {
                                PlanEntryStatusView::Completed
                            }
                            _ => PlanEntryStatusView::Pending,
                        },
                    })
                    .collect();
                out.local.push(LocalFeedEvent::Plan { entries });
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                self.config_state.commands = Some(map_commands(&update.available_commands));
                self.emit_config_state(out);
            }
            SessionUpdate::CurrentModeUpdate(update) => {
                self.config_state.current_mode =
                    Some(steer::truncate(&update.current_mode_id.0, ID_MAX));
                self.emit_config_state(out);
            }
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.config_state.options = self.map_options(&update.config_options);
                self.emit_config_state(out);
            }
            // A session-row fact, not a feed row.
            SessionUpdate::SessionInfoUpdate(_) => {}
            SessionUpdate::UsageUpdate(usage) => {
                let cost = usage
                    .cost
                    .as_ref()
                    // The wire field is `costUsd`; a cost quoted in anything
                    // else is not one this client can label.
                    .filter(|cost| cost.currency.eq_ignore_ascii_case("usd"))
                    .map(|cost| cost.amount);
                emit(
                    out,
                    ActivityEvent::usage(usage.used as i64, usage.size as i64, cost),
                    None,
                );
            }
            SessionUpdate::CompactionUpdate(update) => {
                use agent_client_protocol::schema::v1::CompactionStatus;
                let trigger = notification
                    .meta
                    .as_ref()
                    .or(defined(&update.meta))
                    .and_then(|meta| meta.get(COMPACTION_TRIGGER_META_KEY))
                    .and_then(Value::as_str);
                match update.status {
                    CompactionStatus::InProgress => self.start_compaction(trigger, out),
                    _ => self.end_compaction(trigger, out),
                }
            }
            SessionUpdate::CompactionSummaryChunk(_) => {}
            // `SessionUpdate` is `#[non_exhaustive]`: an agent speaking a
            // newer schema must never break this session.
            _ => {}
        }
        self.close_stale_compaction(out);
    }

    /// A `session/request_permission`. Returns the key the host parks the
    /// `Responder` under. `ToolKind::SwitchMode` (ExitPlanMode) emits a
    /// PLAN card: `plan_mode: true` with the plan markdown as `text`, or all
    /// four clients degrade it to a generic question.
    pub fn on_permission(
        &mut self,
        request: &RequestPermissionRequest,
        out: &mut MapOut,
    ) -> PendingAskKey {
        // Whatever the agent was narrating belongs ABOVE the card it is now
        // blocked on.
        self.flush_all(out);
        let fields = &request.tool_call.fields;
        let title = fields.title.clone().unwrap_or_default();
        let body = content_text(fields.content.as_deref().unwrap_or_default());
        let plan_mode = fields.kind == Some(ToolKind::SwitchMode);
        let raw_id = request.tool_call.tool_call_id.0.to_string();
        let question_id = if raw_id.is_empty() {
            self.synthetic_id("perm", &title)
        } else {
            steer::truncate(&raw_id, ID_MAX)
        };

        let options = permission_options(&request.options)
            .into_iter()
            .map(|(key, label, description)| QuestionOption {
                label: self.clean(&label, OPTION_LABEL_MAX),
                key: steer::truncate(&key, ID_MAX),
                description: description.map(|d| self.clean(&d, OPTION_DESCRIPTION_MAX)),
                free_text: false,
            })
            .collect::<Vec<_>>();

        let text = if plan_mode {
            // The plan body IS the card on all four clients.
            self.clean_marked(if body.is_empty() { &title } else { &body }, QUESTION_TEXT_MAX)
        } else if body.is_empty() {
            self.clean_marked(&title, QUESTION_TEXT_MAX)
        } else {
            self.clean_marked(&body, QUESTION_TEXT_MAX)
        };
        let header = (!plan_mode && !title.is_empty())
            .then(|| self.clean(&title, QUESTION_HEADER_MAX));

        self.permissions.insert(
            question_id.clone(),
            PermissionAsk {
                options: options
                    .iter()
                    .map(|option| (option.key.clone(), option.label.clone()))
                    .collect(),
                answered: false,
            },
        );
        self.questions
            .insert(question_id.clone(), AskRef::Permission(question_id.clone()));

        emit(
            out,
            ActivityEvent::Question {
                text,
                options,
                multi_select: None,
                plan_mode: plan_mode.then_some(true),
                id: Some(question_id.clone()),
                ask_id: None,
                index: None,
                total: None,
                header,
                at: None,
            },
            Some(question_id.clone()),
        );
        out.needs_input = Some(true);
        PendingAskKey {
            question_id,
            ask_id: None,
        }
    }

    /// An `elicitation/create` form, published as a `<ask>#<n>` … `<ask>#submit`
    /// stepper.
    pub fn on_elicitation(
        &mut self,
        ask_id: &str,
        request: &CreateElicitationRequest,
        out: &mut MapOut,
    ) -> PendingAskKey {
        self.flush_all(out);
        let ask_id = if ask_id.is_empty() {
            self.synthetic_id("ask", &request.message)
        } else {
            steer::truncate(ask_id, ID_MAX)
        };
        let steps = match &request.mode {
            ElicitationMode::Form(form) => {
                self.elicit_steps(&request.message, &form.requested_schema)
            }
            // A URL (or any newer) elicitation has nothing to step through:
            // one confirm card, then the form goes back accepted.
            _ => Vec::new(),
        };
        let ask = ElicitationAsk {
            steps,
            current: 0,
            fields: Map::new(),
            answered: false,
            message: request.message.clone(),
        };
        self.elicitations.insert(ask_id.clone(), ask);
        let question_id = self.publish_elicit_step(&ask_id, out);
        out.needs_input = Some(true);
        PendingAskKey {
            question_id,
            ask_id: Some(ask_id),
        }
    }

    /// An inbound `answer` frame (relay or local composer). Publishes
    /// `answer_ack` then `question_resolved` and tells the host what to do
    /// with the parked responder.
    pub fn on_answer(
        &mut self,
        key: &PendingAskKey,
        answer: &steer::RemoteAnswer,
        out: &mut MapOut,
    ) -> AnswerDecision {
        match self.questions.get(&key.question_id) {
            Some(AskRef::Permission(id)) => {
                let id = id.clone();
                self.answer_permission(&id, answer, out)
            }
            Some(AskRef::Elicitation(ask_id)) => {
                let ask_id = ask_id.clone();
                self.answer_elicitation(&ask_id, key, answer, out)
            }
            None => AnswerDecision::Unknown,
        }
    }

    /// `session/cancel`: the ACP contract REQUIRES answering every pending
    /// permission with `Cancelled`, and every open card is dismissed.
    pub fn on_cancel(&mut self, out: &mut MapOut) {
        let mut dismissed: Vec<(String, Option<String>)> = Vec::new();
        for (id, ask) in self.permissions.iter_mut() {
            if !ask.answered {
                ask.answered = true;
                dismissed.push((id.clone(), None));
            }
        }
        for (ask_id, ask) in self.elicitations.iter_mut() {
            if !ask.answered {
                ask.answered = true;
                let id = elicit_step_id(ask_id, ask.current, ask.steps.len());
                dismissed.push((id, Some(ask_id.clone())));
            }
        }
        for (id, ask_id) in dismissed {
            emit(
                out,
                ActivityEvent::QuestionResolved {
                    id: Some(id),
                    ask_id,
                    answers: None,
                    dismissed: Some(true),
                    at: None,
                },
                None,
            );
        }
        self.flush_all(out);
        self.end_compaction(None, out);
        out.needs_input = Some(false);
        // NOT idle: the agent is still finishing the prompt it was asked to
        // interrupt. `on_stop` flips the turn signal when that prompt
        // actually answers (`StopReason::Cancelled` routes back through
        // here), so an `AfterTurn` kill cannot tear the run down while the
        // interrupt is still settling.
    }

    /// A turn ended. No wire event: the stop reason only moves `out.idle`.
    pub fn on_stop(&mut self, stop: StopReason, out: &mut MapOut) {
        self.flush_all(out);
        if matches!(stop, StopReason::Cancelled) {
            self.on_cancel(out);
        }
        out.idle = Some(true);
        if out.needs_input.is_none() && self.pending_asks() == 0 {
            out.needs_input = Some(false);
        }
    }

    /// The full config snapshot from `session/new`, `session/load`,
    /// `session/set_config_option` or a `ConfigOptionUpdate` — always emitted
    /// WHOLE and clamped (`steer::clamp_config_state`), because
    /// `config_state` is latest-wins state, not a delta.
    pub fn on_session_state(
        &mut self,
        modes: Option<&SessionModeState>,
        options: &[SessionConfigOption],
        commands: &[AvailableCommand],
        out: &mut MapOut,
    ) {
        if let Some(modes) = modes {
            self.config_state.current_mode =
                Some(steer::truncate(&modes.current_mode_id.0, ID_MAX));
            self.config_state.modes = Some(
                modes
                    .available_modes
                    .iter()
                    .map(|mode| steer::ConfigMode {
                        id: steer::truncate(&mode.id.0, ID_MAX),
                        label: self.clean(&mode.name, OPTION_LABEL_MAX),
                        description: mode
                            .description
                            .as_ref()
                            .map(|text| self.clean(text, OPTION_DESCRIPTION_MAX)),
                    })
                    .collect(),
            );
        }
        if !options.is_empty() {
            self.config_state.options = self.map_options(options);
        }
        if !commands.is_empty() {
            self.config_state.commands = Some(map_commands(commands));
        }
        self.emit_config_state(out);
    }

    /// `session/set_mode` answered with nothing, so the engine mirrors the
    /// mode it just set: the re-emitted `config_state` is the ONLY
    /// confirmation the wire has (D4). An agent that also pushes
    /// `CurrentModeUpdate` then re-emits an identical snapshot, which is
    /// latest-wins and therefore free.
    pub fn set_current_mode(&mut self, mode_id: &str, out: &mut MapOut) {
        self.config_state.current_mode = Some(steer::truncate(mode_id, ID_MAX));
        self.emit_config_state(out);
    }

    /// Debounce tick: emits whatever the coalescers have been holding.
    pub fn flush(&mut self, out: &mut MapOut) {
        if let Some(text) = self.message.take_if_idle() {
            self.emit_narration(&text, out);
        }
        if let Some(text) = self.thought.take_if_idle() {
            self.emit_thought(&text, out);
        }
        if let Some(text) = self.user.take_if_idle() {
            self.emit_user(&text, out);
        }
        self.close_stale_compaction(out);
    }

    /// An adapter's subagent edge (`_meta`, see [`SubagentEdge`]).
    pub fn on_subagent(&mut self, edge: &SubagentEdge, out: &mut MapOut) {
        let id = steer::truncate(&edge.id, ID_MAX);
        let agent_type = match edge.status {
            SubagentEdgeStatus::Started => {
                let agent_type = self.clean(&edge.agent_type, AGENT_TYPE_MAX);
                self.subagents.insert(id.clone(), agent_type.clone());
                agent_type
            }
            SubagentEdgeStatus::Completed => self
                .subagents
                .remove(&id)
                .unwrap_or_else(|| self.clean(&edge.agent_type, AGENT_TYPE_MAX)),
        };
        emit(
            out,
            ActivityEvent::Subagent {
                id,
                agent_type,
                status: match edge.status {
                    SubagentEdgeStatus::Started => steer::SubagentStatus::Started,
                    SubagentEdgeStatus::Completed => steer::SubagentStatus::Completed,
                },
                detail: edge
                    .detail
                    .as_ref()
                    .map(|detail| self.clean(detail, TOOL_DETAIL_MAX)),
                at: None,
                tool_calls: None,
            },
            None,
        );
    }

    /// An adapter or transport error worth showing: one capped narration,
    /// prefixed so it reads as the session speaking, not the agent.
    pub fn on_error(&mut self, message: &str, out: &mut MapOut) {
        self.flush_all(out);
        let text = self.clean(&format!("Agent error: {message}"), NARRATION_MAX);
        emit(out, ActivityEvent::narration(text), None);
    }

    /// A compaction edge an adapter synthesized (claude hooks, codex
    /// `context_compacted`, pi `session_compact`) rather than sending as a
    /// `CompactionUpdate`.
    pub fn on_compaction(&mut self, phase: CompactionPhase, trigger: Option<&str>, out: &mut MapOut) {
        match phase {
            CompactionPhase::Started => self.start_compaction(trigger, out),
            CompactionPhase::Ended => self.end_compaction(trigger, out),
        }
    }

    /// The key an inbound answer resolves — `ask_id` filled in from the
    /// mapper's own table, because a client only has to echo the card id.
    pub fn ask_key(&self, answer: &steer::RemoteAnswer) -> PendingAskKey {
        match self.questions.get(&answer.question_id) {
            Some(AskRef::Elicitation(ask_id)) => PendingAskKey {
                question_id: answer.question_id.clone(),
                ask_id: Some(ask_id.clone()),
            },
            _ => PendingAskKey {
                question_id: answer.question_id.clone(),
                ask_id: answer.ask_id.clone(),
            },
        }
    }

    /// How many cards are still open — the `needs_input` source of truth.
    pub fn pending_asks(&self) -> usize {
        self.permissions.values().filter(|ask| !ask.answered).count()
            + self.elicitations.values().filter(|ask| !ask.answered).count()
    }

    /// The live `config_state`, for a caller that has to re-publish it (a
    /// reconnect replays the journal, so this is only the local mirror).
    pub fn config_state(&self) -> ActivityEvent {
        self.build_config_state()
    }

    // -- internals ---------------------------------------------------------

    fn flush_all(&mut self, out: &mut MapOut) {
        self.flush_message(out);
        self.flush_thought(out);
        self.flush_user(out);
    }

    fn flush_message(&mut self, out: &mut MapOut) {
        if let Some(text) = self.message.take() {
            self.emit_narration(&text, out);
        }
    }

    fn flush_thought(&mut self, out: &mut MapOut) {
        if let Some(text) = self.thought.take() {
            self.emit_thought(&text, out);
        }
    }

    fn flush_user(&mut self, out: &mut MapOut) {
        if let Some(text) = self.user.take() {
            self.emit_user(&text, out);
        }
    }

    fn emit_narration(&mut self, text: &str, out: &mut MapOut) {
        if text.trim().is_empty() {
            return;
        }
        let text = self.clean(text, NARRATION_MAX);
        emit(out, ActivityEvent::narration(text), None);
    }

    /// The host is about to send `text` as a prompt (a seed, a steer, a
    /// command): publish it as the user's message NOW, the way the PTY path
    /// echoed typed input, and remember it so the agent's own replay of the
    /// same message (claude) does not land twice. An agent that never echoes
    /// (codex, pi) gets its `user_message` from here alone.
    pub fn on_prompt(&mut self, text: &str, out: &mut MapOut) {
        if text.trim().is_empty() {
            return;
        }
        self.flush_all(out);
        self.pending_echoes.push_back(text.trim().to_string());
        if self.pending_echoes.len() > PENDING_ECHOES_MAX {
            self.pending_echoes.pop_front();
        }
        let text = self.clean(text, NARRATION_MAX);
        emit(out, ActivityEvent::user_message(text), None);
    }

    fn emit_user(&mut self, text: &str, out: &mut MapOut) {
        if text.trim().is_empty() {
            return;
        }
        if let Some(at) = self.pending_echoes.iter().position(|sent| sent == text.trim()) {
            // The agent replayed what the host already published.
            self.pending_echoes.remove(at);
            return;
        }
        let text = self.clean(text, NARRATION_MAX);
        emit(out, ActivityEvent::user_message(text), None);
    }

    /// A thought is BOTH a capped narration on the wire (viewers have always
    /// seen the agent think) and a richer local item the desktop styles.
    fn emit_thought(&mut self, text: &str, out: &mut MapOut) {
        if text.trim().is_empty() {
            return;
        }
        let clean = self.clean(text, NARRATION_MAX);
        emit(out, ActivityEvent::narration(clean.clone()), None);
        out.local.push(LocalFeedEvent::Thought {
            message_id: self.thought.message_id.clone(),
            text: clean,
        });
    }

    fn synthetic_id(&mut self, kind: &str, text: &str) -> String {
        self.ordinal = self.ordinal.wrapping_add(1);
        steer::synthetic_question_id(&self.config.session_seed, kind, text, self.ordinal)
    }

    /// `notification_meta`: the carrying notification's `_meta`, the other
    /// place an adapter may name the owning subagent.
    fn on_tool_call(
        &mut self,
        call: &ToolCall,
        notification_meta: Option<&BTreeMapLike>,
        out: &mut MapOut,
    ) {
        self.flush_all(out);
        let id = steer::truncate(&call.tool_call_id.0, ID_MAX);
        let subagent_id = call
            .meta
            .as_ref()
            .and_then(subagent_id_from_meta)
            .or_else(|| notification_meta.and_then(subagent_id_from_meta))
            .map(|id| steer::truncate(&id, ID_MAX));
        let detail = self.tool_detail(call.kind, &call.title, &call.locations, call.raw_input.as_ref());
        let name = self.wire_tool_name(call.kind, &call.title, detail.as_deref());
        emit(
            out,
            ActivityEvent::Tool {
                name: self.clean(&name, TOOL_NAME_MAX),
                detail,
                subagent_id,
                at: None,
            },
            Some(id.clone()),
        );
        out.local.push(LocalFeedEvent::ToolCall {
            id: id.clone(),
            title: call.title.clone(),
            kind: card_kind(call.kind),
            status: card_status(call.status),
            locations: call
                .locations
                .iter()
                .map(|location| location.path.clone())
                .collect(),
        });
        self.tools.insert(
            id.clone(),
            ToolState {
                title: call.title.clone(),
                kind: call.kind,
            },
        );
        self.tool_content(&id, call.kind, &call.content, call.raw_output.as_ref(), out);
    }

    fn on_tool_call_update(&mut self, update: &ToolCallUpdate, out: &mut MapOut) {
        let id = steer::truncate(&update.tool_call_id.0, ID_MAX);
        let ToolCallUpdateFields {
            kind,
            status,
            title,
            content,
            locations,
            raw_output,
            ..
        } = &update.fields;
        let known = self.tools.get(&id);
        let kind = kind.or_else(|| known.map(|state| state.kind)).unwrap_or(ToolKind::Other);
        let title = title
            .clone()
            .or_else(|| known.map(|state| state.title.clone()))
            .unwrap_or_default();
        if let Some(state) = self.tools.get_mut(&id) {
            state.kind = kind;
            state.title.clone_from(&title);
        }
        // Rule 2: a status transition is a local card patch, never a wire row.
        out.local.push(LocalFeedEvent::ToolCall {
            id: id.clone(),
            title,
            kind: card_kind(kind),
            status: card_status(status.unwrap_or(ToolCallStatus::InProgress)),
            locations: locations
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|location| location.path.clone())
                .collect(),
        });
        if let Some(content) = content {
            self.tool_content(&id, kind, content, raw_output.as_ref(), out);
        }
    }

    /// Tool-call content is LOCAL only: a `Diff` carries a whole patch body
    /// and `Content` on an `Execute` call carries command output, neither of
    /// which the wire ever carried (rule 1). The wire `diff` stays the
    /// debounced whole-worktree snapshot the lifecycle publishes.
    fn tool_content(
        &mut self,
        id: &str,
        kind: ToolKind,
        content: &[ToolCallContent],
        raw_output: Option<&Value>,
        out: &mut MapOut,
    ) {
        let exit_code = raw_output
            .and_then(|raw| raw.get("exit_code").or_else(|| raw.get("exitCode")))
            .and_then(Value::as_i64)
            .map(|code| code as i32);
        for item in content {
            match item {
                ToolCallContent::Diff(diff) => out.local.push(LocalFeedEvent::EditDiff {
                    tool_call_id: id.to_string(),
                    path: diff.path.clone(),
                    old_text: diff.old_text.clone(),
                    new_text: diff.new_text.clone(),
                }),
                ToolCallContent::Content(block) if kind == ToolKind::Execute => {
                    let chunk = block_text(&block.content);
                    if !chunk.is_empty() {
                        out.local.push(LocalFeedEvent::Output {
                            tool_call_id: id.to_string(),
                            chunk,
                            exit_code,
                        });
                    }
                }
                // EXP-750: the client DOES run terminals, so a Terminal item
                // names the live command this call renders — for any kind,
                // since an agent is free to embed one under a tool card that
                // is not `Execute`. Local like everything else here: the
                // engine binds the terminal to this call and its output
                // streams in as `Output` chunks.
                ToolCallContent::Terminal(terminal) => {
                    out.local.push(LocalFeedEvent::TerminalBound {
                        tool_call_id: id.to_string(),
                        terminal_id: terminal.terminal_id.0.to_string(),
                    })
                }
                // Everything else is a card detail the local renderer
                // already has.
                _ => {}
            }
        }
    }

    /// The `Tool { detail }` DERIVATION (rule 1). Never `raw_input`'s command
    /// string, never a patch body: a path, a pattern, the command's first
    /// token, or the agent's own one-line description.
    /// The wire `tool.name`: the PTY path's bare vocabulary, never a title
    /// that quotes the command. An `Execute` card's title IS the command on
    /// every adapter (claude `Bash`, codex `commandExecution`, pi `bash`), so
    /// it maps to the name the PTY path published for that agent; any other
    /// title drops the detail it embeds (`Write smoke.txt` → `Write`).
    fn wire_tool_name(&self, kind: ToolKind, title: &str, detail: Option<&str>) -> String {
        if kind == ToolKind::Execute {
            return match self.config.agent {
                steer::SessionAgent::Claude => "Bash",
                steer::SessionAgent::Codex => "exec_command",
                steer::SessionAgent::Pi => "bash",
                _ => "Execute",
            }
            .to_string();
        }
        let title = title.trim();
        if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
            if let Some(at) = title.find(detail) {
                let head = title[..at].trim();
                if !head.is_empty() {
                    return head.to_string();
                }
            }
        }
        title.to_string()
    }

    fn tool_detail(
        &self,
        kind: ToolKind,
        title: &str,
        locations: &[agent_client_protocol::schema::v1::ToolCallLocation],
        raw_input: Option<&Value>,
    ) -> Option<String> {
        if let Some(location) = locations.first() {
            return Some(self.clean(&self.display_path(&location.path), TOOL_DETAIL_MAX));
        }
        let Some(input) = raw_input.and_then(Value::as_object) else {
            // No structured input (codex's commandExecution card): an Execute
            // title IS the command, and its first token is the detail.
            return (kind == ToolKind::Execute)
                .then(|| command_head(title))
                .filter(|head| !head.is_empty())
                .map(|head| self.clean(head, TOOL_DETAIL_MAX));
        };
        let string = |key: &str| input.get(key).and_then(Value::as_str).filter(|s| !s.is_empty());
        if kind == ToolKind::Execute {
            // The PTY path publishes the model's own description of a command
            // (never the command); the first token is the fallback below.
            if let Some(description) = string("description") {
                return Some(self.clean(description, TOOL_DETAIL_MAX));
            }
        }
        if let Some(path) = string("file_path").or_else(|| string("path")).or_else(|| string("filePath")) {
            return Some(self.clean(&self.display_path(&PathBuf::from(path)), TOOL_DETAIL_MAX));
        }
        if let Some(pattern) = string("pattern").or_else(|| string("query")).or_else(|| string("url")) {
            return Some(self.clean(pattern, TOOL_DETAIL_MAX));
        }
        if kind == ToolKind::Execute {
            if let Some(command) = string("command") {
                // The FIRST TOKEN only: `rm -rf …` reads as `rm`, and no
                // argument (a URL with a token in it, a heredoc) reaches the
                // relay. codex wraps a command as `/bin/zsh -lc <command>`,
                // which names the shell, not the command.
                let head = command_head(command);
                if !head.is_empty() {
                    return Some(self.clean(head, TOOL_DETAIL_MAX));
                }
            }
        }
        if let Some(description) = string("description") {
            return Some(self.clean(description, TOOL_DETAIL_MAX));
        }
        None
    }

    /// Paths render relative to the worktree, like every other surface.
    fn display_path(&self, path: &std::path::Path) -> String {
        path.strip_prefix(&self.config.cwd)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    fn map_options(&self, options: &[SessionConfigOption]) -> Vec<steer::ConfigOption> {
        options
            .iter()
            .map(|option| {
                let (value, values) = match &option.kind {
                    SessionConfigKind::Select(select) => {
                        let values = match &select.options {
                            SessionConfigSelectOptions::Ungrouped(list) => list
                                .iter()
                                .map(|value| steer::ConfigValue {
                                    id: steer::truncate(&value.value.0, ID_MAX),
                                    label: self.clean(&value.name, OPTION_LABEL_MAX),
                                })
                                .collect(),
                            SessionConfigSelectOptions::Grouped(groups) => groups
                                .iter()
                                .flat_map(|group| group.options.iter())
                                .map(|value| steer::ConfigValue {
                                    id: steer::truncate(&value.value.0, ID_MAX),
                                    label: self.clean(&value.name, OPTION_LABEL_MAX),
                                })
                                .collect(),
                            // `#[non_exhaustive]`: an unknown grouping still
                            // renders as a read-only chip.
                            _ => Vec::new(),
                        };
                        (
                            Some(steer::truncate(&select.current_value.0, ID_MAX)),
                            Some(values),
                        )
                    }
                    SessionConfigKind::Boolean(boolean) => (
                        Some(boolean.current_value.to_string()),
                        Some(vec![
                            steer::ConfigValue {
                                id: "true".to_string(),
                                label: "On".to_string(),
                            },
                            steer::ConfigValue {
                                id: "false".to_string(),
                                label: "Off".to_string(),
                            },
                        ]),
                    ),
                    _ => (None, None),
                };
                steer::ConfigOption {
                    id: steer::truncate(&option.id.0, ID_MAX),
                    label: self.clean(&option.name, OPTION_LABEL_MAX),
                    category: option.category.as_ref().map(category_id),
                    value,
                    values,
                }
            })
            .collect()
    }

    fn build_config_state(&self) -> ActivityEvent {
        let mut event = ActivityEvent::ConfigState {
            options: self.config_state.options.clone(),
            current_mode: self.config_state.current_mode.clone(),
            modes: self.config_state.modes.clone(),
            commands: self.config_state.commands.clone(),
            at: None,
        };
        // An over-cap frame does not degrade: the relay drops the WHOLE frame
        // and no client ever paints a chip. Clamp before anyone sees it, so
        // the local feed and the relay agree on what was published.
        steer::clamp_config_state(&mut event);
        event
    }

    fn emit_config_state(&mut self, out: &mut MapOut) {
        let event = self.build_config_state();
        emit(out, event, None);
    }

    fn start_compaction(&mut self, trigger: Option<&str>, out: &mut MapOut) {
        if self.compacting_since.is_some() {
            return;
        }
        self.flush_all(out);
        self.compacting_since = Some(Instant::now());
        emit(
            out,
            ActivityEvent::compaction(
                CompactionPhase::Started,
                steer::normalize_compaction_trigger(trigger),
            ),
            None,
        );
    }

    fn end_compaction(&mut self, trigger: Option<&str>, out: &mut MapOut) {
        if self.compacting_since.take().is_none() {
            return;
        }
        emit(
            out,
            ActivityEvent::compaction(
                CompactionPhase::Ended,
                steer::normalize_compaction_trigger(trigger),
            ),
            None,
        );
    }

    /// A compaction whose end never arrived (the agent died mid-compaction)
    /// must not strand the clients' strip.
    fn close_stale_compaction(&mut self, out: &mut MapOut) {
        if self
            .compacting_since
            .is_some_and(|since| since.elapsed() >= COMPACTION_MAX)
        {
            self.end_compaction(None, out);
        }
    }

    fn answer_permission(
        &mut self,
        id: &str,
        answer: &steer::RemoteAnswer,
        out: &mut MapOut,
    ) -> AnswerDecision {
        let Some(ask) = self.permissions.get_mut(id) else {
            return AnswerDecision::Unknown;
        };
        // EXP-374: a re-tap of an answered card re-acks and stops there —
        // the ACP responder is long resolved.
        if ask.answered {
            emit(
                out,
                ActivityEvent::AnswerAck {
                    id: id.to_string(),
                    ask_id: None,
                    at: None,
                },
                None,
            );
            return AnswerDecision::ReAck;
        }
        let Some(key) = answer.keys.first().cloned() else {
            return AnswerDecision::Unknown;
        };
        let Some((option_id, label)) = ask
            .options
            .iter()
            .find(|(option, _)| option == &key)
            .cloned()
        else {
            // An option the card never offered cannot be forwarded to the
            // agent: dropping it keeps the card answerable.
            return AnswerDecision::Unknown;
        };
        ask.answered = true;
        emit(
            out,
            ActivityEvent::AnswerAck {
                id: id.to_string(),
                ask_id: None,
                at: None,
            },
            None,
        );
        emit(
            out,
            ActivityEvent::QuestionResolved {
                id: Some(id.to_string()),
                ask_id: None,
                answers: Some(vec![steer::truncate(&label, ANSWER_MAX)]),
                dismissed: None,
                at: None,
            },
            None,
        );
        if self.pending_asks() == 0 {
            out.needs_input = Some(false);
        }
        AnswerDecision::Permission { option_id }
    }

    fn answer_elicitation(
        &mut self,
        ask_id: &str,
        key: &PendingAskKey,
        answer: &steer::RemoteAnswer,
        out: &mut MapOut,
    ) -> AnswerDecision {
        let Some(ask) = self.elicitations.get_mut(ask_id) else {
            return AnswerDecision::Unknown;
        };
        if ask.answered {
            emit(
                out,
                ActivityEvent::AnswerAck {
                    id: key.question_id.clone(),
                    ask_id: Some(ask_id.to_string()),
                    at: None,
                },
                None,
            );
            return AnswerDecision::ReAck;
        }
        let expected = elicit_step_id(ask_id, ask.current, ask.steps.len());
        if expected != key.question_id {
            // A stale step (the stepper already moved on) re-acks only.
            emit(
                out,
                ActivityEvent::AnswerAck {
                    id: key.question_id.clone(),
                    ask_id: Some(ask_id.to_string()),
                    at: None,
                },
                None,
            );
            return AnswerDecision::ReAck;
        }
        emit(
            out,
            ActivityEvent::AnswerAck {
                id: key.question_id.clone(),
                ask_id: Some(ask_id.to_string()),
                at: None,
            },
            None,
        );

        let submit = ask.current >= ask.steps.len();
        let answers: Vec<String> = if submit {
            vec!["Submit".to_string()]
        } else {
            let step = &ask.steps[ask.current];
            let labels: Vec<String> = answer
                .keys
                .iter()
                .map(|key| {
                    step.options
                        .iter()
                        .find(|option| &option.key == key)
                        .map(|option| option.label.clone())
                        .unwrap_or_else(|| key.clone())
                })
                .collect();
            let typed = answer.text.clone().filter(|text| !text.trim().is_empty());
            let choice_keys: Vec<String> =
                answer.keys.iter().filter(|key| *key != FREE_TEXT_KEY).cloned().collect();
            match (&step.custom_property, &typed) {
                // The folded free-text row: the typed answer is the custom
                // field and the choice stays unanswered (claude's
                // `applyAskElicitationResponse` lets the custom text win).
                (Some(custom), Some(text)) => {
                    ask.fields.insert(custom.clone(), Value::String(text.clone()));
                }
                _ => {
                    let value = step_value(step, &choice_keys, typed.as_deref());
                    if !value.is_null() && value != Value::String(String::new()) {
                        ask.fields.insert(step.property.clone(), value);
                    }
                }
            }
            match typed {
                Some(text) => vec![text],
                None => labels
                    .into_iter()
                    .filter(|label| answer.keys.iter().all(|key| key != FREE_TEXT_KEY) || label != "Type something.")
                    .collect(),
            }
        };
        emit(
            out,
            ActivityEvent::QuestionResolved {
                id: Some(key.question_id.clone()),
                ask_id: Some(ask_id.to_string()),
                answers: Some(
                    answers
                        .into_iter()
                        .take(ANSWERS_MAX)
                        .map(|answer| steer::truncate(&answer, ANSWER_MAX))
                        .collect(),
                ),
                dismissed: None,
                at: None,
            },
            None,
        );

        // A one-step form has nothing to review: its answer IS the submit,
        // exactly the one tap the PTY card took.
        let lone_step = ask.steps.len() == 1 && !submit;
        if submit || lone_step {
            ask.answered = true;
            let fields = Value::Object(ask.fields.clone());
            if self.pending_asks() == 0 {
                out.needs_input = Some(false);
            }
            return AnswerDecision::Elicitation {
                fields,
                submit: true,
            };
        }
        ask.current += 1;
        let fields = Value::Object(ask.fields.clone());
        self.publish_elicit_step(ask_id, out);
        AnswerDecision::Elicitation {
            fields,
            submit: false,
        }
    }

    /// Publish the step the stepper is on (or its submit marker) and register
    /// the question id. Returns that id.
    fn publish_elicit_step(&mut self, ask_id: &str, out: &mut MapOut) -> String {
        let Some(ask) = self.elicitations.get(ask_id) else {
            return String::new();
        };
        let total = ask.steps.len();
        let question_id = elicit_step_id(ask_id, ask.current, total);
        let event = if ask.current >= total {
            // The submit step: BOTH `index` and `total` absent — that is the
            // marker every client keys on (D3).
            ActivityEvent::Question {
                text: self.clean_marked(&ask.message, QUESTION_TEXT_MAX),
                options: vec![QuestionOption::new("Submit", "submit")],
                multi_select: None,
                plan_mode: None,
                id: Some(question_id.clone()),
                ask_id: Some(ask_id.to_string()),
                index: None,
                total: None,
                header: None,
                at: None,
            }
        } else {
            let step = &ask.steps[ask.current];
            ActivityEvent::Question {
                text: step.text.clone(),
                options: step.options.clone(),
                multi_select: step.multi_select.then_some(true),
                plan_mode: None,
                id: Some(question_id.clone()),
                ask_id: Some(ask_id.to_string()),
                index: Some(ask.current as u32 + 1),
                total: Some(total as u32),
                header: step.header.clone(),
                at: None,
            }
        };
        self.questions
            .insert(question_id.clone(), AskRef::Elicitation(ask_id.to_string()));
        emit(out, event, None);
        question_id
    }

    /// One step per property, except that a plain-string property named
    /// `<name>_custom` beside a choice `<name>` is FOLDED into that choice's
    /// step as its free-text row (the shape claude's AskUserQuestion form
    /// takes), so the card reads like the PTY path's: options plus "Type
    /// something.", one tap. A single-step form reads the request message
    /// as its text (the message IS the question) and the property title as
    /// its header.
    fn elicit_steps(
        &self,
        message: &str,
        schema: &agent_client_protocol::schema::v1::ElicitationSchema,
    ) -> Vec<ElicitStep> {
        let is_plain_string = |property: &ElicitationPropertySchema| {
            matches!(
                property,
                ElicitationPropertySchema::String(schema)
                    if schema.one_of.as_ref().is_none_or(Vec::is_empty)
                        && schema.enum_values.as_ref().is_none_or(Vec::is_empty)
            )
        };
        let folded: std::collections::HashSet<String> = schema
            .properties
            .iter()
            .filter_map(|(name, property)| {
                let choice = name.strip_suffix("_custom")?;
                let sibling = schema.properties.get(choice)?;
                (is_plain_string(property) && !is_plain_string(sibling)).then(|| name.clone())
            })
            .collect();
        let mut steps: Vec<ElicitStep> = schema
            .properties
            .iter()
            .filter(|(name, _)| !folded.contains(*name))
            .map(|(name, property)| {
                let mut step = self.elicit_step(name, property);
                let custom = format!("{name}_custom");
                if folded.contains(&custom) {
                    if !step.options.iter().any(|option| option.free_text) {
                        step.options.push(QuestionOption {
                            label: "Type something.".to_string(),
                            key: FREE_TEXT_KEY.to_string(),
                            description: None,
                            free_text: true,
                        });
                    }
                    step.custom_property = Some(custom);
                }
                step
            })
            .collect();
        if steps.len() == 1 && !message.trim().is_empty() {
            let step = &mut steps[0];
            let text = self.clean_marked(message, QUESTION_TEXT_MAX);
            if step.text != text {
                let title = std::mem::replace(&mut step.text, text);
                step.header = Some(self.clean(&title, QUESTION_HEADER_MAX));
            }
        }
        steps
    }

    fn elicit_step(&self, name: &str, property: &ElicitationPropertySchema) -> ElicitStep {
        let free_text = |label: &str| QuestionOption {
            label: label.to_string(),
            key: FREE_TEXT_KEY.to_string(),
            description: None,
            free_text: true,
        };
        match property {
            ElicitationPropertySchema::String(schema) => {
                // The description is the QUESTION when a form carries several
                // (the title is its short header); a lone title is the text.
                let (text, header) = match (schema.description.as_deref(), schema.title.as_deref()) {
                    (Some(description), title) if !description.trim().is_empty() => (
                        self.clean_marked(description, QUESTION_TEXT_MAX),
                        title
                            .filter(|title| !title.trim().is_empty())
                            .map(|title| self.clean(title, QUESTION_HEADER_MAX)),
                    ),
                    (_, Some(title)) if !title.trim().is_empty() => {
                        (self.clean_marked(title, QUESTION_TEXT_MAX), None)
                    }
                    _ => (self.clean_marked(name, QUESTION_TEXT_MAX), None),
                };
                let options = match (&schema.one_of, &schema.enum_values) {
                    (Some(one_of), _) if !one_of.is_empty() => one_of
                        .iter()
                        .map(|option| QuestionOption {
                            label: self.clean(&option.title, OPTION_LABEL_MAX),
                            key: steer::truncate(&option.value, ID_MAX),
                            description: option
                                .description
                                .as_ref()
                                .map(|text| self.clean(text, OPTION_DESCRIPTION_MAX)),
                            free_text: false,
                        })
                        .collect(),
                    (_, Some(values)) if !values.is_empty() => values
                        .iter()
                        .map(|value| QuestionOption {
                            label: self.clean(value, OPTION_LABEL_MAX),
                            key: steer::truncate(value, ID_MAX),
                            description: None,
                            free_text: false,
                        })
                        .collect(),
                    _ => vec![free_text("Type something.")],
                };
                ElicitStep {
                    property: name.to_string(),
                    text,
                    header,
                    options,
                    multi_select: false,
                    kind: StepKind::Text,
                    custom_property: None,
                }
            }
            ElicitationPropertySchema::Boolean(schema) => ElicitStep {
                property: name.to_string(),
                text: self.clean_marked(
                    schema
                        .title
                        .as_deref()
                        .or(schema.description.as_deref())
                        .unwrap_or(name),
                    QUESTION_TEXT_MAX,
                ),
                header: None,
                options: vec![
                    QuestionOption::new("Yes", "true"),
                    QuestionOption::new("No", "false"),
                ],
                multi_select: false,
                kind: StepKind::Bool,
                custom_property: None,
            },
            ElicitationPropertySchema::Integer(_) => ElicitStep {
                property: name.to_string(),
                text: self.clean_marked(name, QUESTION_TEXT_MAX),
                header: None,
                options: vec![free_text("Type a number.")],
                multi_select: false,
                kind: StepKind::Integer,
                custom_property: None,
            },
            ElicitationPropertySchema::Number(_) => ElicitStep {
                property: name.to_string(),
                text: self.clean_marked(name, QUESTION_TEXT_MAX),
                header: None,
                options: vec![free_text("Type a number.")],
                multi_select: false,
                kind: StepKind::Number,
                custom_property: None,
            },
            ElicitationPropertySchema::Array(schema) => ElicitStep {
                property: name.to_string(),
                text: self.clean_marked(
                    schema
                        .title
                        .as_deref()
                        .or(schema.description.as_deref())
                        .unwrap_or(name),
                    QUESTION_TEXT_MAX,
                ),
                header: None,
                options: multi_select_options(&schema.items)
                    .into_iter()
                    .map(|(value, label)| QuestionOption {
                        label: self.clean(&label, OPTION_LABEL_MAX),
                        key: steer::truncate(&value, ID_MAX),
                        description: None,
                        free_text: false,
                    })
                    .collect(),
                multi_select: true,
                kind: StepKind::Strings,
                custom_property: None,
            },
            // `#[non_exhaustive]`: an unknown property kind still gets a
            // free-text step rather than silently dropping the question.
            _ => ElicitStep {
                property: name.to_string(),
                text: self.clean_marked(name, QUESTION_TEXT_MAX),
                header: None,
                options: vec![free_text("Type something.")],
                multi_select: false,
                kind: StepKind::Text,
                custom_property: None,
            },
        }
    }
}

/// How many host-sent prompts wait for their echo at once (a mid-turn steer
/// queue is short; claude replays each message before the next turn).
const PENDING_ECHOES_MAX: usize = 8;

/// The option key of a free-text row. Never a VALUE: a typed answer rides
/// `answer.text`, and picking the row with nothing typed means "no answer".
const FREE_TEXT_KEY: &str = "text";

/// `(value, label)` per multi-select item, over both shapes the schema has.
fn multi_select_options(
    items: &agent_client_protocol::schema::v1::MultiSelectItems,
) -> Vec<(String, String)> {
    use agent_client_protocol::schema::v1::MultiSelectItems;
    match items {
        MultiSelectItems::String(items) => items
            .values
            .iter()
            .map(|value| (value.clone(), value.clone()))
            .collect(),
        MultiSelectItems::Titled(items) => items
            .options
            .iter()
            .map(|option| (option.value.clone(), option.title.clone()))
            .collect(),
        // `#[non_exhaustive]`: an unknown item shape renders no options, so
        // the step degrades to an unanswerable card rather than a wrong one.
        _ => Vec::new(),
    }
}

/// `<ask>#<n>` for a step, `<ask>#submit` for the final marker (D3).
fn elicit_step_id(ask_id: &str, step: usize, total: usize) -> String {
    if step >= total {
        format!("{ask_id}#submit")
    } else {
        format!("{ask_id}#{step}")
    }
}

/// One step's answer, in the shape `elicitation/create`'s `content` map wants.
fn step_value(step: &ElicitStep, keys: &[String], typed: Option<&str>) -> Value {
    match step.kind {
        StepKind::Bool => Value::Bool(keys.first().map(|key| key == "true").unwrap_or(false)),
        StepKind::Integer => typed
            .and_then(|text| text.trim().parse::<i64>().ok())
            .map(Value::from)
            .unwrap_or(Value::Null),
        StepKind::Number => typed
            .and_then(|text| text.trim().parse::<f64>().ok())
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        StepKind::Strings => Value::Array(keys.iter().cloned().map(Value::String).collect()),
        StepKind::Text => match typed {
            Some(text) if !text.is_empty() => Value::String(text.to_string()),
            _ => Value::String(keys.first().cloned().unwrap_or_default()),
        },
    }
}

/// Permission options in the order the four clients read best: allow first,
/// reject last (`PermissionOptionKind` has no relay field of its own). The
/// sort is STABLE, so the agent's own order survives inside a rank.
fn permission_options(options: &[PermissionOption]) -> Vec<(String, String, Option<String>)> {
    let mut ranked: Vec<(usize, &PermissionOption)> = options
        .iter()
        .map(|option| {
            let rank = match option.kind {
                PermissionOptionKind::AllowOnce => 0,
                PermissionOptionKind::AllowAlways => 1,
                PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways => 3,
                _ => 2,
            };
            (rank, option)
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .take(steer::QUESTION_OPTIONS_MAX)
        .map(|(_, option)| (option.option_id.0.to_string(), option.name.clone(), None))
        .collect()
}

fn map_commands(commands: &[AvailableCommand]) -> Vec<steer::ConfigCommand> {
    commands
        .iter()
        .map(|command| steer::ConfigCommand {
            name: command.name.clone(),
            description: command.description.clone(),
            hint: command.input.as_ref().and_then(|input| match input {
                agent_client_protocol::schema::v1::AvailableCommandInput::Unstructured(
                    unstructured,
                ) => Some(unstructured.hint.clone()),
                _ => None,
            }),
        })
        .collect()
}

/// The relay's `option.category` grouping hint. Deliberately the four names
/// the clients already know (`model`, `effort`, `mode`), never the ACP enum's
/// Rust spelling.
fn category_id(category: &SessionConfigOptionCategory) -> String {
    match category {
        SessionConfigOptionCategory::Model => "model".to_string(),
        SessionConfigOptionCategory::ThoughtLevel => "effort".to_string(),
        SessionConfigOptionCategory::ModelConfig => "model_config".to_string(),
        SessionConfigOptionCategory::Mode => "mode".to_string(),
        SessionConfigOptionCategory::Other(other) => other.clone(),
        _ => "other".to_string(),
    }
}

fn message_id(chunk: &ContentChunk) -> Option<String> {
    chunk.message_id.as_ref().map(|id| id.0.to_string())
}

fn chunk_text(chunk: &ContentChunk) -> String {
    block_text(&chunk.content)
}

fn block_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        // An image/audio/resource chunk has no text the wire can carry; the
        // local card keeps its own copy where it matters.
        _ => String::new(),
    }
}

fn content_text(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ToolCallContent::Content(block) => Some(block_text(&block.content)),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn subagent_id_from_meta(meta: &BTreeMapLike) -> Option<String> {
    meta.get(SUBAGENT_ID_META_KEY)
        .or_else(|| meta.get("subagent_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The first token of a command line, past a `<shell> -lc`/`-c` wrapper
/// (`/bin/zsh -lc "bun test"` reads as `bun`).
fn command_head(command: &str) -> &str {
    let mut tokens = command.split_whitespace();
    let first = tokens.next().unwrap_or_default();
    let is_shell = first.rsplit('/').next().is_some_and(|name| name.ends_with("sh"));
    if is_shell {
        if let Some(flag) = tokens.next() {
            if matches!(flag, "-lc" | "-c" | "-ic" | "-lic") {
                return tokens.next().unwrap_or_default().trim_matches(|c| c == '"' || c == '\'');
            }
        }
    }
    first
}

/// The `_meta` of the update a notification carries, for the variants an
/// adapter may stamp instead of the notification itself.
fn update_meta(update: &SessionUpdate) -> Option<&BTreeMapLike> {
    match update {
        SessionUpdate::ToolCall(call) => call.meta.as_ref(),
        SessionUpdate::ToolCallUpdate(update) => update.meta.as_ref(),
        SessionUpdate::CompactionUpdate(update) => defined(&update.meta),
        _ => None,
    }
}

/// A patch-shaped field (`MaybeUndefined`) read as plain presence.
fn defined<T>(value: &agent_client_protocol::schema::MaybeUndefined<T>) -> Option<&T> {
    match value {
        agent_client_protocol::schema::MaybeUndefined::Value(value) => Some(value),
        _ => None,
    }
}

/// ACP's `_meta` is a `serde_json::Map`; named so the helper above reads.
type BTreeMapLike = Map<String, Value>;

fn card_kind(kind: ToolKind) -> ToolCardKind {
    match kind {
        ToolKind::Read => ToolCardKind::Read,
        ToolKind::Edit => ToolCardKind::Edit,
        ToolKind::Delete => ToolCardKind::Delete,
        ToolKind::Move => ToolCardKind::Move,
        ToolKind::Search => ToolCardKind::Search,
        ToolKind::Execute => ToolCardKind::Execute,
        ToolKind::Think => ToolCardKind::Think,
        ToolKind::Fetch => ToolCardKind::Fetch,
        ToolKind::SwitchMode => ToolCardKind::SwitchMode,
        _ => ToolCardKind::Other,
    }
}

fn card_status(status: ToolCallStatus) -> ToolCardStatus {
    match status {
        ToolCallStatus::Pending => ToolCardStatus::Pending,
        ToolCallStatus::Completed => ToolCardStatus::Completed,
        ToolCallStatus::Failed => ToolCardStatus::Failed,
        _ => ToolCardStatus::InProgress,
    }
}

/// The phase edge a host renders as a banner — re-exported here so the
/// lifecycle and the mapper agree on one vocabulary.
pub fn phase_event(phase: EnginePhase) -> LocalFeedEvent {
    LocalFeedEvent::Phase(phase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        ContentChunk, PermissionOption, PermissionOptionId, RequestPermissionRequest, SessionId,
        SessionNotification, TextContent, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
    };
    use serde_json::json;

    fn mapper() -> Mapper {
        Mapper::new(MapperConfig {
            redactor: Arc::new(steer::Redactor::new(vec!["expu_supersecretkey".to_string()])),
            cwd: PathBuf::from("/tmp/worktree"),
            agent: steer::SessionAgent::Claude,
            session_seed: "sess-1".to_string(),
        })
    }

    fn notify(update: SessionUpdate) -> SessionNotification {
        SessionNotification::new(SessionId::new("acp-1"), update)
    }

    fn chunk(text: &str, message_id: Option<&str>) -> ContentChunk {
        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)));
        match message_id {
            Some(id) => chunk.message_id(agent_client_protocol::schema::v1::MessageId::new(id)),
            None => chunk,
        }
    }

    /// The host publishes a prompt as the user's message itself, and an
    /// agent that replays it (claude) is deduped; one that never echoes
    /// (codex) still gets exactly one `user_message`.
    #[test]
    fn a_prompt_is_the_users_message_once_whether_or_not_the_agent_echoes_it() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.on_prompt("fix the login bug", &mut out);
        assert_eq!(
            out.wire.iter().filter(|event| matches!(event, ActivityEvent::UserMessage { .. })).count(),
            1
        );
        // claude's replay of the same text: silent.
        let mut echoed = MapOut::default();
        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("fix the login bug")));
        mapper.on_update(&notify(SessionUpdate::UserMessageChunk(chunk)), &mut echoed);
        mapper.on_stop(StopReason::EndTurn, &mut echoed);
        assert!(
            !echoed.wire.iter().any(|event| matches!(event, ActivityEvent::UserMessage { .. })),
            "{:?}",
            echoed.wire
        );
        // A different user message the agent surfaces (a session/load
        // replay) still publishes.
        let mut other = MapOut::default();
        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("and the signup page")));
        mapper.on_update(&notify(SessionUpdate::UserMessageChunk(chunk)), &mut other);
        mapper.on_stop(StopReason::EndTurn, &mut other);
        assert_eq!(
            other.wire.iter().filter(|event| matches!(event, ActivityEvent::UserMessage { .. })).count(),
            1
        );
    }

    /// A replay (or any burst inside one flush window) delivers user,
    /// thought and message chunks back to back: the wire keeps that order
    /// instead of grouping the three coalescers' contents by kind.
    #[test]
    fn chunks_of_different_kinds_keep_their_arrival_order() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let user = ContentChunk::new(ContentBlock::Text(TextContent::new("fix the login bug")));
        let thought = ContentChunk::new(ContentBlock::Text(TextContent::new("Planning the fix")));
        let message = ContentChunk::new(ContentBlock::Text(TextContent::new("On it.")));
        mapper.on_update(&notify(SessionUpdate::UserMessageChunk(user)), &mut out);
        mapper.on_update(&notify(SessionUpdate::AgentThoughtChunk(thought)), &mut out);
        mapper.on_update(&notify(SessionUpdate::AgentMessageChunk(message)), &mut out);
        mapper.on_stop(StopReason::EndTurn, &mut out);
        let kinds: Vec<String> = out
            .wire
            .iter()
            .map(|event| serde_json::to_value(event).unwrap()["kind"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(kinds, vec!["user_message", "narration", "narration"], "{:?}", out.wire);
        let texts: Vec<String> = out
            .wire
            .iter()
            .map(|event| serde_json::to_value(event).unwrap()["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(texts, vec!["fix the login bug", "Planning the fix", "On it."]);
    }

    #[test]
    fn message_chunks_coalesce_until_the_turn_ends() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("Hello ", Some("m1")))),
            &mut out,
        );
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("world", Some("m1")))),
            &mut out,
        );
        assert!(out.wire.is_empty(), "a partial message never publishes");
        mapper.on_stop(StopReason::EndTurn, &mut out);
        assert_eq!(out.wire.len(), 1);
        match &out.wire[0] {
            ActivityEvent::Narration { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected one narration, got {other:?}"),
        }
        assert_eq!(out.idle, Some(true));
    }

    #[test]
    fn a_new_message_id_flushes_the_previous_message() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("first", Some("m1")))),
            &mut out,
        );
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("second", Some("m2")))),
            &mut out,
        );
        assert_eq!(out.wire.len(), 1);
        mapper.on_stop(StopReason::EndTurn, &mut out);
        assert_eq!(out.wire.len(), 2);
    }

    #[test]
    fn an_empty_thought_chunk_is_dropped() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentThoughtChunk(chunk("   ", None))),
            &mut out,
        );
        mapper.on_stop(StopReason::EndTurn, &mut out);
        assert!(out.wire.is_empty());
    }

    #[test]
    fn a_tool_detail_is_derived_never_the_raw_command() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-1"), "Run tests")
            .kind(ToolKind::Execute)
            .raw_input(json!({"command": "npm test --token expu_supersecretkey"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                // The PTY vocabulary for this agent, never the title/command.
                assert_eq!(name, "Bash");
                assert_eq!(detail.as_deref(), Some("npm"));
            }
            other => panic!("expected a tool event, got {other:?}"),
        }
    }

    /// The PTY path publishes the model's description of a command, never
    /// the command; the first token is only the fallback.
    #[test]
    fn a_command_description_is_the_detail_before_the_first_token() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-3"), "printf 'smoke %s' one two")
            .kind(ToolKind::Execute)
            .raw_input(json!({"command": "printf 'smoke %s' one two", "description": "Print the smoke lines"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "Bash");
                assert_eq!(detail.as_deref(), Some("Print the smoke lines"));
            }
            other => panic!("expected a tool event, got {other:?}"),
        }
    }

    /// A title that embeds the detail (`Write smoke.txt`) publishes as the
    /// bare verb, so the clients' `name · detail` row reads like the PTY one.
    #[test]
    fn the_wire_tool_name_drops_the_detail_the_title_embeds() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-4"), "Write smoke.txt")
            .kind(ToolKind::Edit)
            .raw_input(json!({"file_path": "/tmp/worktree/smoke.txt", "content": "hello"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "Write");
                assert_eq!(detail.as_deref(), Some("smoke.txt"));
            }
            other => panic!("expected a tool event, got {other:?}"),
        }
        // codex wraps the command in a login shell: the detail is the
        // command's own first token, never the shell.
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-7"), "printf 'smoke %s' one two")
            .kind(ToolKind::Execute)
            .raw_input(json!({"command": "/bin/zsh -lc \"printf 'smoke %s' one two\""}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { detail, .. } => assert_eq!(detail.as_deref(), Some("printf")),
            other => panic!("expected a tool event, got {other:?}"),
        }
        // codex's command card carries no raw input: the title IS the
        // command and its first token is the detail.
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-6"), "printf 'smoke %s' one two").kind(ToolKind::Execute);
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "Bash");
                assert_eq!(detail.as_deref(), Some("printf"));
            }
            other => panic!("expected a tool event, got {other:?}"),
        }
        // A title with no embedded detail is published as it is.
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-5"), "mcp.exponential.issues_get").kind(ToolKind::Other);
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "mcp.exponential.issues_get");
                assert_eq!(detail, &None);
            }
            other => panic!("expected a tool event, got {other:?}"),
        }
    }

    #[test]
    fn a_secret_in_a_tool_path_is_redacted() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-2"), "Read")
            .kind(ToolKind::Read)
            .raw_input(json!({"file_path": "/tmp/worktree/expu_supersecretkey.txt"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { detail, .. } => {
                let detail = detail.as_deref().unwrap_or_default();
                assert!(!detail.contains("expu_supersecretkey"), "{detail}");
            }
            other => panic!("expected a tool event, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_update_never_reaches_the_wire() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let update = ToolCallUpdate::new(
            ToolCallId::new("tc-1"),
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(update)), &mut out);
        assert!(out.wire.is_empty());
        assert!(matches!(
            out.local.first(),
            Some(LocalFeedEvent::ToolCall { .. })
        ));
    }

    #[test]
    fn a_permission_becomes_an_answerable_question_keyed_on_the_tool_call() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let request = RequestPermissionRequest::new(
            SessionId::new("acp-1"),
            ToolCallUpdate::new(
                ToolCallId::new("tc-9"),
                ToolCallUpdateFields::new().title("Write src/main.rs"),
            ),
            vec![
                PermissionOption::new(
                    PermissionOptionId::new("reject"),
                    "No",
                    PermissionOptionKind::RejectOnce,
                ),
                PermissionOption::new(
                    PermissionOptionId::new("allow"),
                    "Yes",
                    PermissionOptionKind::AllowOnce,
                ),
            ],
        );
        let key = mapper.on_permission(&request, &mut out);
        assert_eq!(key.question_id, "tc-9");
        assert_eq!(out.needs_input, Some(true));
        match &out.wire[0] {
            ActivityEvent::Question { id, options, plan_mode, header, .. } => {
                assert_eq!(id.as_deref(), Some("tc-9"));
                // Allow sorts first, reject last.
                assert_eq!(options[0].key, "allow");
                assert_eq!(options[1].key, "reject");
                assert_eq!(*plan_mode, None);
                assert_eq!(header.as_deref(), Some("Write src/main.rs"));
            }
            other => panic!("expected a question, got {other:?}"),
        }

        let answer = steer::RemoteAnswer {
            question_id: "tc-9".to_string(),
            ask_id: None,
            keys: vec!["allow".to_string()],
            text: None,
        };
        let mut answered = MapOut::default();
        let decision = mapper.on_answer(&mapper.ask_key(&answer), &answer, &mut answered);
        assert_eq!(
            decision,
            AnswerDecision::Permission {
                option_id: "allow".to_string()
            }
        );
        assert!(matches!(answered.wire[0], ActivityEvent::AnswerAck { .. }));
        assert!(matches!(
            answered.wire[1],
            ActivityEvent::QuestionResolved { .. }
        ));
        assert_eq!(answered.needs_input, Some(false));

        // EXP-374: a re-tap re-acks and never re-resolves.
        let mut again = MapOut::default();
        assert_eq!(
            mapper.on_answer(&mapper.ask_key(&answer), &answer, &mut again),
            AnswerDecision::ReAck
        );
        assert_eq!(again.wire.len(), 1);
    }

    #[test]
    fn an_exit_plan_mode_permission_is_a_plan_card() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let request = RequestPermissionRequest::new(
            SessionId::new("acp-1"),
            ToolCallUpdate::new(
                ToolCallId::new("tc-plan"),
                ToolCallUpdateFields::new()
                    .kind(ToolKind::SwitchMode)
                    .title("Ready to code?")
                    .content(vec![ToolCallContent::from(ContentBlock::Text(
                        TextContent::new("## Plan\n\n1. Do the thing"),
                    ))]),
            ),
            vec![PermissionOption::new(
                PermissionOptionId::new("approve"),
                "Yes, and auto-accept edits",
                PermissionOptionKind::AllowAlways,
            )],
        );
        mapper.on_permission(&request, &mut out);
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("the question serializes"),
            json!({
                "kind": "question",
                "text": "## Plan\n\n1. Do the thing",
                "options": [{"label": "Yes, and auto-accept edits", "key": "approve"}],
                "planMode": true,
                "id": "tc-plan"
            })
        );
    }

    #[test]
    fn cancelling_dismisses_every_open_card() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let request = RequestPermissionRequest::new(
            SessionId::new("acp-1"),
            ToolCallUpdate::new(
                ToolCallId::new("tc-3"),
                ToolCallUpdateFields::new().title("Delete everything"),
            ),
            vec![PermissionOption::new(
                PermissionOptionId::new("allow"),
                "Yes",
                PermissionOptionKind::AllowOnce,
            )],
        );
        mapper.on_permission(&request, &mut out);
        let mut cancelled = MapOut::default();
        mapper.on_cancel(&mut cancelled);
        match &cancelled.wire[0] {
            ActivityEvent::QuestionResolved { id, dismissed, .. } => {
                assert_eq!(id.as_deref(), Some("tc-3"));
                assert_eq!(*dismissed, Some(true));
            }
            other => panic!("expected a dismissal, got {other:?}"),
        }
        assert_eq!(cancelled.needs_input, Some(false));
    }

    /// EXP-746: `session/cancel` asks the agent to stop, it does not stop it.
    /// The turn signal only flips when the interrupted prompt actually
    /// answers, or an `AfterTurn` kill (EXP-637) tears the run down while the
    /// agent is still winding the turn up.
    #[test]
    fn a_cancel_stays_busy_until_the_cancelled_turn_answers() {
        let mut mapper = mapper();
        let mut cancelled = MapOut::default();
        mapper.on_cancel(&mut cancelled);
        assert_eq!(cancelled.idle, None, "the interrupt has not settled yet");

        let mut stopped = MapOut::default();
        mapper.on_stop(StopReason::Cancelled, &mut stopped);
        assert_eq!(stopped.idle, Some(true));
    }

    #[test]
    fn config_state_is_one_clamped_whole_snapshot() {
        use agent_client_protocol::schema::v1::{
            SessionConfigSelectOption, SessionMode, SessionModeId,
        };
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let modes = SessionModeState::new(
            SessionModeId::new("plan"),
            vec![
                SessionMode::new(SessionModeId::new("plan"), "Plan"),
                SessionMode::new(SessionModeId::new("default"), "Default"),
            ],
        );
        let options = vec![SessionConfigOption::select(
            "model",
            "Model",
            "opus",
            vec![
                SessionConfigSelectOption::new("opus", "Opus"),
                SessionConfigSelectOption::new("sonnet", "Sonnet"),
            ],
        )
        .category(SessionConfigOptionCategory::Model)];
        let commands = vec![AvailableCommand::new("compact", "Compact the context")];
        mapper.on_session_state(Some(&modes), &options, &commands, &mut out);
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("config_state serializes"),
            json!({
                "kind": "config_state",
                "options": [{
                    "id": "model",
                    "label": "Model",
                    "category": "model",
                    "value": "opus",
                    "values": [
                        {"id": "opus", "label": "Opus"},
                        {"id": "sonnet", "label": "Sonnet"}
                    ]
                }],
                "currentMode": "plan",
                "modes": [
                    {"id": "plan", "label": "Plan"},
                    {"id": "default", "label": "Default"}
                ],
                "commands": [{"name": "compact", "description": "Compact the context"}]
            })
        );
    }

    #[test]
    fn usage_carries_only_a_dollar_cost() {
        use agent_client_protocol::schema::v1::{Cost, UsageUpdate};
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::UsageUpdate(
                UsageUpdate::new(124_000, 200_000).cost(Cost::new(1.24, "USD")),
            )),
            &mut out,
        );
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("usage serializes"),
            json!({"kind": "usage", "contextUsed": 124_000, "contextSize": 200_000, "costUsd": 1.24})
        );
    }

    #[test]
    fn a_subagent_edge_rides_the_notification_meta() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let edge = SubagentEdge {
            id: "task-1".to_string(),
            agent_type: "explore".to_string(),
            status: SubagentEdgeStatus::Started,
            detail: None,
            tool_calls: None,
        };
        let notification = notify(SessionUpdate::AgentMessageChunk(chunk("", None)))
            .meta(edge.to_meta().clone());
        mapper.on_update(&notification, &mut out);
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("subagent serializes"),
            json!({"kind": "subagent", "id": "task-1", "agentType": "explore", "status": "started"})
        );
    }

    #[test]
    fn narration_truncates_at_the_relay_cap() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let long = "x".repeat(NARRATION_MAX + 1024);
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk(&long, None))),
            &mut out,
        );
        mapper.on_stop(StopReason::EndTurn, &mut out);
        match &out.wire[0] {
            ActivityEvent::Narration { text, .. } => {
                assert!(text.len() <= NARRATION_MAX, "{} bytes", text.len())
            }
            other => panic!("expected a narration, got {other:?}"),
        }
    }
}
