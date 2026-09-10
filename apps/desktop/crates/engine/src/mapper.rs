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
//!    EXP-785/786 revised the "content stays local" half: a call's SETTLE
//!    (`completed`/`failed`) and an `edit` call's per-file diff now ALSO ride
//!    the wire as a `tool_update` log row — a unified diff BUILT here from
//!    old/new text, redacted, then cut on line boundaries to the contract's
//!    `toolDiffMaxLines`/`toolDiffMaxBytes`. Clients fold it into the tool
//!    row by id; the local ACP content (whole patches, command output)
//!    remains richer than the wire ever is.
//! 2. **Chunks coalesce.** ACP streams are ~100x denser than the 1 s
//!    transcript poll and the journal evicts at 2000 events / 4 MiB; narration
//!    and thought chunks coalesce by `message_id` and flush on idle or turn
//!    end, `ToolCallUpdate` publishes only on a SETTLE (never the pending →
//!    in-progress churn), and `config_state`/`usage`/`rate_limit` are
//!    latest-wins slots.
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
    SessionConfigOption,
    SessionModeState, SessionNotification, SessionUpdate, StopReason, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use serde_json::{Map, Value};
use steer::activity::{
    ANSWERS_MAX, ANSWER_MAX, AGENT_TYPE_MAX, ID_MAX, NARRATION_MAX, OPTION_DESCRIPTION_MAX,
    OPTION_LABEL_MAX, QUESTION_HEADER_MAX, QUESTION_TEXT_MAX, TOOL_DETAIL_MAX, TOOL_NAME_MAX,
};
use steer::frames::CompactionPhase;
use steer::{ActivityEvent, QuestionOption, ToolKind as WireToolKind, ToolUpdateStatus};

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

/// How many ANSWERED asks the mapper keeps for re-ack (EXP-766). A long
/// unattended run answers thousands of permissions; each one held its options
/// and, for a form, its whole field map for the rest of the session.
pub const ANSWERED_ASKS_MAX: usize = 64;

/// How many SETTLED tool calls keep their title/kind row. A settled call still
/// gets trailing content-only updates (codex's `outputDelta` lands after the
/// call reports completed), and those inherit their title and kind from this
/// table, so the row outlives the settle and is evicted oldest-first instead.
pub const SETTLED_TOOLS_MAX: usize = 256;

/// The `_meta` key an adapter stamps a subagent edge under (see
/// [`SubagentEdge`]).
pub use crate::local::{
    COMPACTION_TRIGGER_META_KEY, INJECTED_PROMPT_META_KEY, SUBAGENT_ID_META_KEY, SUBAGENT_META_KEY,
};

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
    /// EXP-804: a CHANGE to the agent's usage wall — `Some(Some(..))` hit
    /// one, `Some(None)` cleared it, `None` said nothing about it. Nested
    /// exactly like `needs_input`'s `Option<bool>`: the outer layer is "did
    /// this step speak", the inner one is the value.
    pub blocked: Option<Option<steer::SessionBlocked>>,
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
    /// Settled tool calls in settle order, evicted past [`SETTLED_TOOLS_MAX`].
    settled_tools: std::collections::VecDeque<String>,
    subagents: HashMap<String, String>,
    /// EXP-748: attributed tool calls per live subagent. The completed edge
    /// reports the total, so a viewer whose replay lost the individual rows
    /// (the journal drops them first) still captions "N tool calls".
    subagent_tool_calls: HashMap<String, u32>,
    /// EXP-773: subagents whose STARTING prompt has already been seen. A
    /// subagent's first user turn IS the prompt the Task tool call already
    /// says, so it is dropped instead of published twice.
    subagent_prompts_seen: std::collections::HashSet<String>,
    permissions: HashMap<String, PermissionAsk>,
    elicitations: HashMap<String, ElicitationAsk>,
    /// question id → which ask owns it (a stepper registers one per step).
    questions: HashMap<String, AskRef>,
    /// EXP-766: answered asks in answer order. An answered ask is kept only
    /// so a re-tap can re-ack it (EXP-374), so the oldest are evicted past
    /// [`ANSWERED_ASKS_MAX`]; unanswered asks are NEVER evicted.
    answered_asks: std::collections::VecDeque<RetiredAsk>,
    /// The live `config_state` snapshot — always published WHOLE (D4).
    config_state: ConfigSnapshot,
    /// EXP-758: the last snapshot of each latest-wins slot that actually went
    /// out. One `set_config` used to publish two identical `config_state`
    /// frames and one `set_mode` three (the request's answer, the engine's
    /// own mirror, the agent's echo of it), which every client then re-rendered
    /// and the journal stored. An identical re-emit says nothing.
    last_config_state: Option<ActivityEvent>,
    last_usage: Option<ActivityEvent>,
    /// EXP-784: the rate-limit slot's last published snapshot.
    last_rate_limit: Option<ActivityEvent>,
    compacting_since: Option<Instant>,
    /// Disambiguates two synthetic ids whose text is identical.
    ordinal: u32,
}

#[derive(Default)]
struct ConfigSnapshot {
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

/// An ask that has been answered (or cancelled), queued for eviction.
enum RetiredAsk {
    Permission(String),
    Elicitation(String),
}

/// The chunk coalescer of rule 2: one buffer per `message_id`, flushed when
/// the id changes, when the turn ends, or after [`FLUSH_IDLE`].
#[derive(Default)]
struct Coalescer {
    message_id: Option<String>,
    /// EXP-773: the subagent every chunk of the buffered message belongs to.
    /// A change of owner is a message boundary like a new id.
    subagent_id: Option<String>,
    buf: String,
    since: Option<Instant>,
}

/// One coalesced message, with the identity every event it produces carries:
/// the agent's `messageId` (EXP-772, the clients' merge key) and the subagent
/// it belongs to (EXP-773).
struct Flushed {
    text: String,
    message_id: Option<String>,
    subagent_id: Option<String>,
}

impl Coalescer {
    /// Returns the text of a buffer that has to flush BEFORE this chunk (a
    /// new `message_id` starts a new message).
    ///
    /// EXP-758: an ID-LESS chunk is a WHOLE message everywhere the adapters
    /// produce one (pi's error narration and its extension notices, codex's
    /// `codex error:` lines, claude's usage markdown): every streamed delta
    /// carries an id, pi's falling back to the message timestamp. Two of them
    /// in one flush window used to glue into
    /// `pi: Codex error: …pi: Codex error: …`, so they are SEPARATED by a
    /// newline instead. Deliberately not a hard boundary: an external ACP
    /// agent that streams id-less deltas would then publish one feed row per
    /// delta, which is the far worse failure.
    fn push(
        &mut self,
        message_id: Option<String>,
        subagent_id: Option<String>,
        text: &str,
    ) -> Option<Flushed> {
        let boundary = (self.message_id != message_id || self.subagent_id != subagent_id)
            && !self.buf.is_empty();
        let flushed = boundary.then(|| Flushed {
            text: std::mem::take(&mut self.buf),
            message_id: self.message_id.clone(),
            subagent_id: self.subagent_id.clone(),
        });
        let separate = message_id.is_none()
            && self.message_id.is_none()
            && !self.buf.is_empty()
            && !self.buf.ends_with('\n')
            && !text.starts_with('\n');
        self.message_id = message_id;
        self.subagent_id = subagent_id;
        if separate {
            self.buf.push('\n');
        }
        self.buf.push_str(text);
        self.since = Some(Instant::now());
        flushed
    }

    fn take(&mut self) -> Option<Flushed> {
        self.since = None;
        let message_id = self.message_id.take();
        let subagent_id = self.subagent_id.take();
        (!self.buf.is_empty()).then(|| Flushed {
            text: std::mem::take(&mut self.buf),
            message_id,
            subagent_id,
        })
    }

    /// Take only if the buffer has been quiet for [`FLUSH_IDLE`].
    fn take_if_idle(&mut self) -> Option<Flushed> {
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
            settled_tools: std::collections::VecDeque::new(),
            subagents: HashMap::new(),
            subagent_tool_calls: HashMap::new(),
            subagent_prompts_seen: std::collections::HashSet::new(),
            permissions: HashMap::new(),
            elicitations: HashMap::new(),
            questions: HashMap::new(),
            answered_asks: std::collections::VecDeque::new(),
            config_state: ConfigSnapshot::default(),
            last_config_state: None,
            last_usage: None,
            last_rate_limit: None,
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
        // EXP-773: prose and human turns are scoped to their subagent the same
        // way tool calls are — off the chunk's own `_meta`, else the carrying
        // notification's.
        let chunk_subagent = notification
            .meta
            .as_ref()
            .and_then(subagent_id_from_meta)
            .or_else(|| update_meta(&notification.update).and_then(subagent_id_from_meta))
            .map(|id| steer::truncate(&id, ID_MAX));
        // EXP-772: a prompt the ADAPTER typed for the user (`/clear`, the plan
        // hand-off). It is not a human turn, so it publishes nothing — it only
        // arms the echo dedupe the CLI's replay of it will hit.
        if let SessionUpdate::UserMessageChunk(chunk) = &notification.update {
            if notification
                .meta
                .as_ref()
                .and_then(|meta| meta.get(INJECTED_PROMPT_META_KEY))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                self.arm_echo(&chunk_text(chunk));
                return;
            }
        }
        // The three coalescers join chunks of ONE message each; a chunk of
        // another kind ends whatever the other two hold, so a replay (or any
        // burst inside one flush window) keeps its user → thought → message
        // interleaving instead of grouping by kind at the next flush.
        match &notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                self.flush_message(out);
                self.flush_thought(out);
                if let Some(flushed) =
                    self.user.push(message_id(chunk), chunk_subagent, &chunk_text(chunk))
                {
                    self.emit_user(&flushed, out);
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                self.flush_user(out);
                self.flush_thought(out);
                if let Some(flushed) =
                    self.message.push(message_id(chunk), chunk_subagent, &chunk_text(chunk))
                {
                    self.emit_narration(&flushed, out);
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
                if let Some(flushed) = self.thought.push(message_id(chunk), chunk_subagent, &text) {
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
                let mapped = self.map_commands(&update.available_commands);
                self.config_state.commands = Some(mapped);
                self.emit_config_state(out);
            }
            SessionUpdate::CurrentModeUpdate(update) => {
                self.config_state.current_mode =
                    Some(steer::truncate(&update.current_mode_id.0, ID_MAX));
                self.emit_config_state(out);
            }
            // EXP-772: option chips are gone from every client, so an agent's
            // option vocabulary is no longer news the wire carries.
            SessionUpdate::ConfigOptionUpdate(_) => {}
            // A session-row fact, not a feed row — unless its `_meta` carries
            // the EXP-784 rate-limit slot (claude's `rate_limit_event` and its
            // synthetic "You've hit your…" notices ride a no-op one).
            SessionUpdate::SessionInfoUpdate(_) => {
                if let Some(slot) = notification
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get(RATE_LIMIT_META_KEY))
                    .and_then(Value::as_object)
                {
                    self.emit_rate_limit(
                        slot.get("status").and_then(Value::as_str).unwrap_or(""),
                        slot.get("resetsAt").and_then(Value::as_i64),
                        slot.get("message").and_then(Value::as_str),
                        out,
                    );
                }
            }
            SessionUpdate::UsageUpdate(usage) => {
                let cost = usage
                    .cost
                    .as_ref()
                    // The wire field is `costUsd`; a cost quoted in anything
                    // else is not one this client can label.
                    .filter(|cost| cost.currency.eq_ignore_ascii_case("usd"))
                    .map(|cost| cost.amount);
                self.emit_usage(usage.used, usage.size, cost, out);
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
                id: question_id.clone(),
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
        let mut retired: Vec<RetiredAsk> = Vec::new();
        for (id, ask) in self.permissions.iter_mut() {
            if !ask.answered {
                ask.answered = true;
                dismissed.push((id.clone(), None));
                retired.push(RetiredAsk::Permission(id.clone()));
            }
        }
        for (ask_id, ask) in self.elicitations.iter_mut() {
            if !ask.answered {
                ask.answered = true;
                let id = elicit_step_id(ask_id, ask.current, ask.steps.len());
                dismissed.push((id, Some(ask_id.clone())));
                retired.push(RetiredAsk::Elicitation(ask_id.clone()));
            }
        }
        for ask in retired {
            self.retire_ask(ask);
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
    ///
    /// EXP-772: `_options` is IGNORED. Model/effort/fast pickers left the
    /// steering UI on every client, so the snapshot carries modes and
    /// commands only; the parameter stays so an agent may keep answering with
    /// its own vocabulary.
    pub fn on_session_state(
        &mut self,
        modes: Option<&SessionModeState>,
        _options: &[SessionConfigOption],
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
        if !commands.is_empty() {
            let mapped = self.map_commands(commands);
            self.config_state.commands = Some(mapped);
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
        if let Some(flushed) = self.message.take_if_idle() {
            self.emit_narration(&flushed, out);
        }
        if let Some(flushed) = self.thought.take_if_idle() {
            self.emit_thought(&flushed, out);
        }
        if let Some(flushed) = self.user.take_if_idle() {
            self.emit_user(&flushed, out);
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
        // EXP-748: the completed edge carries the run's tool-call total — what
        // this mapper attributed, or the adapter's own number when it counted
        // more (it sees calls that never reached a `session/update`).
        let tool_calls = match edge.status {
            SubagentEdgeStatus::Started => None,
            SubagentEdgeStatus::Completed => {
                self.subagent_prompts_seen.remove(&id);
                let counted = self.subagent_tool_calls.remove(&id).unwrap_or(0);
                Some(counted.max(edge.tool_calls.unwrap_or(0))).filter(|total| *total > 0)
            }
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
                tool_calls,
            },
            None,
        );
    }

    /// An adapter or transport error worth showing: one capped narration,
    /// prefixed so it reads as the session speaking, not the agent.
    pub fn on_error(&mut self, message: &str, out: &mut MapOut) {
        self.on_notice(&format!("Agent error: {message}"), out);
    }

    /// The ENGINE itself speaking into the transcript (FEED-25: the stall
    /// watchdog announcing the turn it just cancelled). The same row
    /// [`Mapper::on_error`] uses — one capped narration, because narration is
    /// the only notice kind the relay's activity schema has — flushed first
    /// so the notice lands AFTER whatever text it is explaining.
    pub fn on_notice(&mut self, message: &str, out: &mut MapOut) {
        self.flush_all(out);
        let text = self.clean(message, NARRATION_MAX);
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

    /// Queue an answered ask for eviction and drop everything past
    /// [`ANSWERED_ASKS_MAX`] (EXP-766). An evicted card answers as
    /// `AnswerDecision::Unknown` on a re-tap instead of re-acking, which is
    /// the same thing a client that reconnected past the journal sees.
    fn retire_ask(&mut self, retired: RetiredAsk) {
        self.answered_asks.push_back(retired);
        while self.answered_asks.len() > ANSWERED_ASKS_MAX {
            match self.answered_asks.pop_front() {
                // An agent may REUSE an id for a new card. Evicting by id
                // alone then deleted a live ask nobody had answered yet, so
                // an entry that is unanswered is left alone: it owns the id
                // now and gets retired by its own answer.
                Some(RetiredAsk::Permission(id)) => {
                    if self.permissions.get(&id).is_some_and(|ask| ask.answered) {
                        self.permissions.remove(&id);
                        self.questions.remove(&id);
                    }
                }
                Some(RetiredAsk::Elicitation(ask_id)) => {
                    if !self.elicitations.get(&ask_id).is_some_and(|ask| ask.answered) {
                        continue;
                    }
                    if let Some(ask) = self.elicitations.remove(&ask_id) {
                        let total = ask.steps.len();
                        for step in 0..=total {
                            self.questions.remove(&elicit_step_id(&ask_id, step, total));
                        }
                    }
                }
                None => break,
            }
        }
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
        if let Some(flushed) = self.message.take() {
            self.emit_narration(&flushed, out);
        }
    }

    fn flush_thought(&mut self, out: &mut MapOut) {
        if let Some(flushed) = self.thought.take() {
            self.emit_thought(&flushed, out);
        }
    }

    fn flush_user(&mut self, out: &mut MapOut) {
        if let Some(flushed) = self.user.take() {
            self.emit_user(&flushed, out);
        }
    }

    /// EXP-772: every piece of ONE message carries the agent's own message id,
    /// so a client merges the pieces back into a single row instead of
    /// painting one bubble per flush.
    fn emit_narration(&mut self, flushed: &Flushed, out: &mut MapOut) {
        if flushed.text.trim().is_empty() {
            return;
        }
        let text = self.clean(&flushed.text, NARRATION_MAX);
        emit(
            out,
            ActivityEvent::Narration {
                text,
                before_question_id: None,
                message_id: flushed.message_id.clone(),
                subagent_id: flushed.subagent_id.clone(),
                at: None,
            },
            None,
        );
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
        self.arm_echo(text);
        let text = self.clean(text, NARRATION_MAX);
        emit(out, ActivityEvent::user_message(text), None);
    }

    /// Remember `text` so the agent's own replay of it is swallowed. Used by
    /// [`Mapper::on_prompt`] and by the adapters' injected prompts (EXP-772).
    fn arm_echo(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.pending_echoes.push_back(text.trim().to_string());
        if self.pending_echoes.len() > PENDING_ECHOES_MAX {
            self.pending_echoes.pop_front();
        }
    }

    fn emit_user(&mut self, flushed: &Flushed, out: &mut MapOut) {
        let text = flushed.text.as_str();
        if text.trim().is_empty() {
            return;
        }
        if is_synthetic_interrupt(text) {
            return;
        }
        if let Some(at) = self.pending_echoes.iter().position(|sent| sent == text.trim()) {
            // The agent replayed what the host already published.
            self.pending_echoes.remove(at);
            return;
        }
        // EXP-773: a subagent's FIRST turn is the prompt its Task tool call
        // already names — publishing it would print the whole prompt twice.
        if let Some(subagent_id) = &flushed.subagent_id {
            if self.subagent_prompts_seen.insert(subagent_id.clone()) {
                return;
            }
        }
        let text = self.clean(text, NARRATION_MAX);
        emit(
            out,
            ActivityEvent::UserMessage {
                text,
                subagent_id: flushed.subagent_id.clone(),
                at: None,
            },
            None,
        );
    }

    /// A thought is BOTH a capped narration on the wire (viewers have always
    /// seen the agent think) and a richer local item the desktop styles.
    ///
    /// The narration carries a DERIVED id, never the assistant message's own:
    /// a thought and the answer that follows it share one `messageId`, and the
    /// clients merge narration rows by that key, so the chain of thought
    /// landed inside the answer bubble. Thoughts of one message still merge
    /// with each other (same derived id) and never with the answer.
    fn emit_thought(&mut self, flushed: &Flushed, out: &mut MapOut) {
        if flushed.text.trim().is_empty() {
            return;
        }
        let clean = self.clean(&flushed.text, NARRATION_MAX);
        let message_id = flushed.message_id.as_ref().map(|id| thought_message_id(id));
        emit(
            out,
            ActivityEvent::Narration {
                text: clean.clone(),
                before_question_id: None,
                message_id: message_id.clone(),
                subagent_id: flushed.subagent_id.clone(),
                at: None,
            },
            None,
        );
        out.local.push(LocalFeedEvent::Thought {
            message_id,
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
        // EXP-748: count it for the subagent's completed edge — the rows
        // themselves are the first thing a replay drops.
        if let Some(subagent_id) = subagent_id.clone() {
            *self.subagent_tool_calls.entry(subagent_id).or_insert(0) += 1;
        }
        let detail = self.tool_detail(call.kind, &call.title, &call.locations, call.raw_input.as_ref());
        let name = self.wire_tool_name(call.kind, &call.title, detail.as_deref());
        emit(
            out,
            ActivityEvent::Tool {
                name: self.clean(&name, TOOL_NAME_MAX),
                detail,
                id: Some(id.clone()),
                tool_kind: Some(wire_tool_kind(call.kind)),
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
        // The id is LIVE again: an eviction queued by an earlier call under
        // the same id would otherwise drop this one's row mid-flight.
        self.settled_tools.retain(|settled| settled != &id);
        self.tools.insert(
            id.clone(),
            ToolState {
                title: call.title.clone(),
                kind: call.kind,
            },
        );
        let diff = self.tool_content(&id, call.kind, &call.content, call.raw_output.as_ref(), out);
        // A call that arrives already settled (an adapter that reports the
        // whole call at once) never gets an update: settle it from here.
        self.emit_tool_update(&id, settle_status(call.status), diff, out);
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
        let known_before = known.is_some();
        let kind = kind.or_else(|| known.map(|state| state.kind)).unwrap_or(ToolKind::Other);
        let title = title
            .clone()
            .or_else(|| known.map(|state| state.title.clone()))
            .unwrap_or_default();
        if let Some(state) = self.tools.get_mut(&id) {
            state.kind = kind;
            state.title.clone_from(&title);
        }
        // Rule 2: the pending → in-progress churn is a local card patch; only
        // a SETTLE (and an edit's diff) becomes a wire row, below.
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
        let diff = match content {
            Some(content) => self.tool_content(&id, kind, content, raw_output.as_ref(), out),
            None => None,
        };
        // EXP-785/786: one `tool_update` per settle/diff — only for a call
        // this mapper announced (the clients hold exactly the rows it did,
        // so an update for a call they never saw would only be dropped).
        if known_before {
            self.emit_tool_update(&id, status.and_then(settle_status), diff, out);
        }
        // A settled call still gets trailing content-only updates (codex
        // streams `outputDelta` past the completed status), and dropping the
        // row right here published those under an empty title and
        // `ToolKind::Other`. It is retired instead: kept for the next updates
        // and evicted oldest-first past [`SETTLED_TOOLS_MAX`] (EXP-766).
        if matches!(status, Some(ToolCallStatus::Completed) | Some(ToolCallStatus::Failed)) {
            self.retire_tool(id);
        }
    }

    /// EXP-785/786: the `tool_update` row for `id`, when there is anything
    /// to say — a settle, a diff, or both in ONE event.
    fn emit_tool_update(
        &mut self,
        id: &str,
        status: Option<ToolUpdateStatus>,
        diff: Option<String>,
        out: &mut MapOut,
    ) {
        if status.is_none() && diff.is_none() {
            return;
        }
        emit(
            out,
            ActivityEvent::tool_update(id, status, diff),
            Some(id.to_string()),
        );
    }

    /// Queue a settled tool call for eviction. Re-settling one (a `failed`
    /// after a `completed`) does not queue it twice.
    fn retire_tool(&mut self, id: String) {
        if !self.settled_tools.contains(&id) {
            self.settled_tools.push_back(id);
        }
        while self.settled_tools.len() > SETTLED_TOOLS_MAX {
            if let Some(evicted) = self.settled_tools.pop_front() {
                self.tools.remove(&evicted);
            }
        }
    }

    /// Tool-call content is LOCAL first: a `Diff` carries a whole patch body
    /// and `Content` on an `Execute` call carries command output, and the
    /// local cards get both whole. EXP-786 sends ONE derived piece onward —
    /// an `edit` call's diff, as a unified patch built from old/new text,
    /// redacted, then cut to the contract's caps on line boundaries — and
    /// returns it for the caller's `tool_update`. Command output never goes;
    /// the wire `diff` stays the debounced whole-worktree snapshot.
    fn tool_content(
        &mut self,
        id: &str,
        kind: ToolKind,
        content: &[ToolCallContent],
        raw_output: Option<&Value>,
        out: &mut MapOut,
    ) -> Option<String> {
        let exit_code = raw_output
            .and_then(|raw| raw.get("exit_code").or_else(|| raw.get("exitCode")))
            .and_then(Value::as_i64)
            .map(|code| code as i32);
        let mut wire_diff: Option<String> = None;
        for item in content {
            match item {
                ToolCallContent::Diff(diff) => {
                    out.local.push(LocalFeedEvent::EditDiff {
                        tool_call_id: id.to_string(),
                        path: diff.path.clone(),
                        old_text: diff.old_text.clone(),
                        new_text: diff.new_text.clone(),
                    });
                    if kind == ToolKind::Edit {
                        // The LAST diff of one update wins; an update carrying
                        // several files is not something any adapter emits.
                        wire_diff = self.wire_edit_diff(diff).or(wire_diff);
                    }
                }
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
        wire_diff
    }

    /// EXP-786: the per-call patch for the wire — built, redacted, cut. A
    /// cut patch ends in a `\ N more lines truncated` marker line (a `\`
    /// line is unified-diff metadata, so a renderer shows it as a note and
    /// never as a hunk). `None` when the edit changed nothing.
    fn wire_edit_diff(&self, diff: &agent_client_protocol::schema::v1::Diff) -> Option<String> {
        let path = self.display_path(&diff.path);
        let patch = steer::unified_diff(&path, diff.old_text.as_deref(), &diff.new_text);
        if patch.is_empty() {
            return None;
        }
        let redacted = self.config.redactor.redact(&patch);
        let (mut kept, omitted) = steer::truncate_unified_diff(
            &redacted,
            steer::TOOL_DIFF_MAX_LINES,
            steer::TOOL_DIFF_MAX_BYTES,
        );
        if omitted > 0 {
            kept.push_str(&format!("\\ {omitted} more lines truncated\n"));
        }
        Some(kept)
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

    /// The agent's own `/` commands. EXP-758: their three labels go through
    /// [`Mapper::clean`] like every other published string: an agent's
    /// command catalog is built from files in the REPO (claude's
    /// `.claude/commands`, pi's extensions), so a description is exactly as
    /// likely to quote a token as any other text the run produces.
    fn map_commands(&self, commands: &[AvailableCommand]) -> Vec<steer::ConfigCommand> {
        commands
            .iter()
            .map(|command| steer::ConfigCommand {
                name: self.clean(&command.name, steer::activity::CONFIG_ID_MAX),
                description: self.clean(
                    &command.description,
                    steer::activity::CONFIG_DESCRIPTION_MAX,
                ),
                hint: command.input.as_ref().and_then(|input| match input {
                    agent_client_protocol::schema::v1::AvailableCommandInput::Unstructured(
                        unstructured,
                    ) => Some(self.clean(&unstructured.hint, steer::activity::CONFIG_HINT_MAX)),
                    _ => None,
                }),
            })
            .collect()
    }

    fn build_config_state(&self) -> ActivityEvent {
        let mut event = ActivityEvent::ConfigState {
            // EXP-772: ALWAYS empty. Modes (plan on/off) and the `/` catalog
            // are the whole mid-session steering vocabulary now.
            options: Vec::new(),
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
        // EXP-758: latest-wins state that did not change is not news.
        if self.last_config_state.as_ref() == Some(&event) {
            return;
        }
        self.last_config_state = Some(event.clone());
        emit(out, event, None);
    }

    /// The context/spend meter, clamped to the relay's bounds and deduped
    /// like the `config_state` slot above (EXP-758).
    fn emit_usage(&mut self, used: u64, size: u64, cost: Option<f64>, out: &mut MapOut) {
        let mut event = ActivityEvent::usage(
            // SATURATING: a `u64` that does not fit an `i64` used to wrap to
            // a NEGATIVE token count, which the relay's zod bounds reject,
            // dropping the frame WHOLE and freezing every viewer's meter on
            // the last good one.
            i64::try_from(used).unwrap_or(i64::MAX),
            i64::try_from(size).unwrap_or(i64::MAX),
            cost,
        );
        clamp_usage(&mut event);
        if self.last_usage.as_ref() == Some(&event) {
            return;
        }
        self.last_usage = Some(event.clone());
        emit(out, event, None);
    }

    /// EXP-784: the rate-limit slot, deduped like `usage` above. `status` is
    /// the agent's own word for the window; an empty/`ok` status is the
    /// CLEAR frame (`steer::rate_limit_clears`), published once. Fed by the
    /// [`RATE_LIMIT_META_KEY`] `_meta` the claude adapter stamps on a no-op
    /// `session_info_update`.
    pub(crate) fn emit_rate_limit(
        &mut self,
        status: &str,
        resets_at: Option<i64>,
        message: Option<&str>,
        out: &mut MapOut,
    ) {
        let status = self.clean(status.trim(), steer::activity::CONFIG_ID_MAX);
        let message = message
            .map(|message| self.clean(message.trim(), RATE_LIMIT_MESSAGE_MAX))
            .filter(|message| !message.is_empty());
        let clears = steer::rate_limit_clears(&status);
        let event = if clears {
            ActivityEvent::rate_limit("", None, None)
        } else {
            ActivityEvent::rate_limit(status, resets_at.filter(|at| *at >= 0), message)
        };
        if self.last_rate_limit.as_ref() == Some(&event) {
            return;
        }
        self.last_rate_limit = Some(event.clone());
        // EXP-804: the same edge that drives the viewer's banner drives the
        // ROW's durable `blocked`. The banner only exists while somebody
        // watches the stream; the row is what a teammate's list and a parent
        // agent's `sessions_get` read, and it is the only place a walled run
        // is distinguishable from a healthy one.
        out.blocked = Some(if clears {
            None
        } else {
            Some(steer::SessionBlocked {
                kind: steer::activity::BLOCKED_KIND_RATE_LIMIT.to_string(),
                agent: self.config.agent.id().to_string(),
                // The agent names a status, not a window. `session` is the
                // wall a run actually hits (claude's 5h credit frame); a
                // future producer that names its window fills this properly.
                window: steer::activity::BLOCKED_WINDOW_SESSION.to_string(),
                resets_at: resets_at.and_then(steer::iso_from_unix_millis),
                since: coding::agent_accounts::now_iso(),
            })
        });
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
        self.retire_ask(RetiredAsk::Permission(id.to_string()));
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
            self.retire_ask(RetiredAsk::Elicitation(ask_id.to_string()));
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
                id: question_id.clone(),
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
                id: question_id.clone(),
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

/// The CLI SYNTHESISES an interrupt marker into the conversation as a USER
/// turn and `--replay-user-messages` plays it back at us. Nobody typed it, so
/// no echo was ever armed for it and the echo FIFO ([`Mapper::arm_echo`])
/// cannot swallow it — it surfaced as the user's own message, once per
/// interrupt and again per replay (EXP-780). The clients already render the
/// cancellation from the turn's stop reason.
fn is_synthetic_interrupt(text: &str) -> bool {
    let Some(inner) = text
        .trim()
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
    else {
        return false;
    };
    inner.trim().to_ascii_lowercase().starts_with("request interrupted")
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

/// The `_meta` key an adapter puts a permission option's second line under
/// (ACP's `PermissionOption` has no description field); the mapper publishes
/// it as `QuestionOption.description`, which every client renders.
pub const PERMISSION_OPTION_DESCRIPTION_META: &str = "description";

/// EXP-788: a `session/request_permission` is a PERMISSION — an allow/reject
/// card, ranked allow-first — when EVERY option carries an allow/reject kind.
/// Duplicate kinds are legal (codex sends two `allow_always` options per exec
/// approval, claude two for a plan); only an option of some OTHER kind makes
/// the request the agent's own question, whose options keep the agent's
/// order.
fn is_user_question(options: &[PermissionOption]) -> bool {
    options.iter().any(|option| {
        !matches!(
            option.kind,
            PermissionOptionKind::AllowOnce
                | PermissionOptionKind::AllowAlways
                | PermissionOptionKind::RejectOnce
                | PermissionOptionKind::RejectAlways
        )
    })
}

/// Permission options in the order the four clients read best: allow first,
/// reject last (`PermissionOptionKind` has no relay field of its own). The
/// sort is STABLE, so the agent's own order survives inside a rank — and a
/// user question (see [`is_user_question`]) is not ranked at all.
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
    if !is_user_question(options) {
        ranked.sort_by_key(|(rank, _)| *rank);
    }
    ranked
        .into_iter()
        .take(steer::QUESTION_OPTIONS_MAX)
        .map(|(_, option)| {
            let description = option
                .meta
                .as_ref()
                .and_then(|meta| meta.get(PERMISSION_OPTION_DESCRIPTION_META))
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .map(str::to_string);
            (option.option_id.0.to_string(), option.name.clone(), description)
        })
        .collect()
}

/// The relay's zod bounds on `usage` (`apps/steer-relay/src/protocol.ts`):
/// `contextUsed`/`contextSize` are `int().min(0).max(1e9)` and `costUsd` is
/// `number().min(0).max(1e6)`.
const USAGE_TOKENS_MAX: i64 = 1_000_000_000;
const USAGE_COST_MAX: f64 = 1_000_000.0;

/// EXP-758: clamp an [`ActivityEvent::Usage`] to those bounds: the sibling
/// of `steer::clamp_config_state`, and for the same reason. `activityEvent`
/// is a discriminated union, so an out-of-bounds meter is not clipped by the
/// relay, it is DROPPED: the viewer's context pill then freezes on the last
/// good frame for the rest of the run. An agent reporting a nonsense number
/// (a `u64` sentinel, a negative cost) is worth a pinned meter, not a dead
/// one. A no-op for every other kind.
pub fn clamp_usage(event: &mut ActivityEvent) {
    let ActivityEvent::Usage {
        context_used,
        context_size,
        cost_usd,
        ..
    } = event
    else {
        return;
    };
    *context_used = (*context_used).clamp(0, USAGE_TOKENS_MAX);
    *context_size = (*context_size).clamp(0, USAGE_TOKENS_MAX);
    // NaN and the infinities have no JSON spelling zod would accept either.
    *cost_usd = cost_usd
        .filter(|cost| cost.is_finite())
        .map(|cost| cost.clamp(0.0, USAGE_COST_MAX));
}


fn message_id(chunk: &ContentChunk) -> Option<String> {
    chunk.message_id.as_ref().map(|id| id.0.to_string())
}

/// The suffix that separates a thought's narration id from its message's.
const THOUGHT_ID_SUFFIX: &str = "#thought";

/// The id a thought's narration publishes under. The agent gives a thought and
/// the answer that follows it the SAME `messageId`, and clients merge
/// narration rows by it, so the thought needs an id of its own.
fn thought_message_id(message_id: &str) -> String {
    let head = steer::truncate(message_id, ID_MAX - THOUGHT_ID_SUFFIX.len());
    format!("{head}{THOUGHT_ID_SUFFIX}")
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

/// EXP-784: the relay's cap on `rate_limit.message` (protocol.ts).
const RATE_LIMIT_MESSAGE_MAX: usize = 1024;

/// EXP-784: the `_meta` key an adapter publishes the rate-limit slot under,
/// on a no-op `session_info_update`: `{"status": <agent word>, "resetsAt":
/// <unix ms>?, "message": <text>?}`; status empty/`ok` clears.
pub const RATE_LIMIT_META_KEY: &str = "exponentialRateLimit";

/// EXP-785: the wire's kind bucket for an ACP kind — every ACP value has a
/// contract twin; a future ACP kind lands on `Other`.
fn wire_tool_kind(kind: ToolKind) -> WireToolKind {
    match kind {
        ToolKind::Read => WireToolKind::Read,
        ToolKind::Edit => WireToolKind::Edit,
        ToolKind::Delete => WireToolKind::Delete,
        ToolKind::Move => WireToolKind::Move,
        ToolKind::Search => WireToolKind::Search,
        ToolKind::Execute => WireToolKind::Execute,
        ToolKind::Think => WireToolKind::Think,
        ToolKind::Fetch => WireToolKind::Fetch,
        ToolKind::SwitchMode => WireToolKind::SwitchMode,
        _ => WireToolKind::Other,
    }
}

/// EXP-785: the two ACP statuses that SETTLE a call; anything else is churn.
fn settle_status(status: ToolCallStatus) -> Option<ToolUpdateStatus> {
    match status {
        ToolCallStatus::Completed => Some(ToolUpdateStatus::Completed),
        ToolCallStatus::Failed => Some(ToolUpdateStatus::Failed),
        _ => None,
    }
}

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
                assert_eq!(id, "tc-9");
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

    /// EXP-788: codex's exec approval carries TWO `allow_always` options (the
    /// execpolicy amendment and a network-policy one) next to its allow_once
    /// and reject. Every option has an allow/reject kind, so it is a
    /// permission — one answerable card, ranked allow-first with the agent's
    /// order kept inside a rank — never re-read as the agent's own question.
    #[test]
    fn a_codex_approval_with_duplicate_allow_kinds_is_a_permission() {
        let options = vec![
            PermissionOption::new(
                PermissionOptionId::new("reject"),
                "No, and tell Codex what to do differently",
                PermissionOptionKind::RejectOnce,
            ),
            PermissionOption::new(
                PermissionOptionId::new("allow_once"),
                "Yes, run it",
                PermissionOptionKind::AllowOnce,
            ),
            PermissionOption::new(
                PermissionOptionId::new("accept_execpolicy_amendment"),
                "Yes, and don't ask again for commands that start with `cargo`",
                PermissionOptionKind::AllowAlways,
            ),
            PermissionOption::new(
                PermissionOptionId::new("apply_network_policy_amendment:0"),
                "Yes, and allow crates.io in the future",
                PermissionOptionKind::AllowAlways,
            ),
        ];
        assert!(!is_user_question(&options));
        assert!(!is_user_question(&[]));

        let mut mapper = mapper();
        let mut out = MapOut::default();
        let request = RequestPermissionRequest::new(
            SessionId::new("acp-1"),
            ToolCallUpdate::new(
                ToolCallId::new("call-7"),
                ToolCallUpdateFields::new().title("cargo build"),
            ),
            options,
        );
        let key = mapper.on_permission(&request, &mut out);
        assert_eq!(key.question_id, "call-7");
        assert_eq!(out.wire.len(), 1);
        match &out.wire[0] {
            ActivityEvent::Question { options, plan_mode, .. } => {
                let keys: Vec<&str> = options.iter().map(|option| option.key.as_str()).collect();
                assert_eq!(
                    keys,
                    vec![
                        "allow_once",
                        "accept_execpolicy_amendment",
                        "apply_network_policy_amendment:0",
                        "reject",
                    ]
                );
                assert!(options.iter().all(|option| option.description.is_none()));
                assert_eq!(*plan_mode, None);
            }
            other => panic!("expected a permission card, got {other:?}"),
        }
        assert_eq!(out.needs_input, Some(true));
    }

    /// EXP-788: an adapter's option description rides `_meta` and lands on
    /// the wire as `QuestionOption.description`; a blank one is dropped.
    #[test]
    fn an_option_description_in_meta_reaches_the_wire() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let request = RequestPermissionRequest::new(
            SessionId::new("acp-1"),
            ToolCallUpdate::new(ToolCallId::new("call-8"), ToolCallUpdateFields::new()),
            vec![
                PermissionOption::new(
                    PermissionOptionId::new("yes"),
                    "Yes",
                    PermissionOptionKind::AllowAlways,
                )
                .meta(json!({"description": "   "}).as_object().cloned()),
                PermissionOption::new(
                    PermissionOptionId::new("no"),
                    "No",
                    PermissionOptionKind::RejectOnce,
                )
                .meta(json!({"description": "Sends your next message back to planning"}).as_object().cloned()),
            ],
        );
        mapper.on_permission(&request, &mut out);
        match &out.wire[0] {
            ActivityEvent::Question { options, .. } => {
                assert_eq!(options[0].description, None);
                assert_eq!(
                    options[1].description.as_deref(),
                    Some("Sends your next message back to planning")
                );
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    /// EXP-766: an unattended run answers thousands of permissions; only the
    /// last [`ANSWERED_ASKS_MAX`] stay around for a re-ack, and an OPEN card
    /// is never evicted.
    #[test]
    fn answered_permissions_are_bounded_and_open_ones_are_kept() {
        let mut mapper = mapper();
        for n in 0..200 {
            let id = format!("tc-{n}");
            let mut out = MapOut::default();
            let request = RequestPermissionRequest::new(
                SessionId::new("acp-1"),
                ToolCallUpdate::new(
                    ToolCallId::new(id.clone()),
                    ToolCallUpdateFields::new().title("Write src/main.rs"),
                ),
                vec![PermissionOption::new(
                    PermissionOptionId::new("allow"),
                    "Yes",
                    PermissionOptionKind::AllowOnce,
                )],
            );
            mapper.on_permission(&request, &mut out);
            let answer = steer::RemoteAnswer {
                question_id: id,
                ask_id: None,
                keys: vec!["allow".to_string()],
                text: None,
            };
            let mut answered = MapOut::default();
            mapper.on_answer(&mapper.ask_key(&answer), &answer, &mut answered);
        }
        // One card left OPEN at the end.
        let mut out = MapOut::default();
        let request = RequestPermissionRequest::new(
            SessionId::new("acp-1"),
            ToolCallUpdate::new(
                ToolCallId::new("tc-open"),
                ToolCallUpdateFields::new().title("Write src/main.rs"),
            ),
            vec![PermissionOption::new(
                PermissionOptionId::new("allow"),
                "Yes",
                PermissionOptionKind::AllowOnce,
            )],
        );
        mapper.on_permission(&request, &mut out);

        let answered = mapper
            .permissions
            .values()
            .filter(|ask| ask.answered)
            .count();
        assert!(answered <= ANSWERED_ASKS_MAX, "{answered} answered kept");
        assert_eq!(mapper.pending_asks(), 1);
        assert!(mapper.permissions.contains_key("tc-open"));
        assert!(mapper.questions.contains_key("tc-open"));
        // The oldest answered card is gone with its question registration.
        assert!(!mapper.permissions.contains_key("tc-0"));
        assert!(!mapper.questions.contains_key("tc-0"));
        // The newest answered ones still re-ack.
        assert!(mapper.permissions.contains_key("tc-199"));
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
    fn config_state_is_one_clamped_whole_snapshot_without_options() {
        // EXP-772: the agent may advertise a whole option vocabulary; the
        // snapshot carries modes and commands ONLY, and `options` is empty.
        use agent_client_protocol::schema::v1::{
            SessionConfigSelectOption, SessionMode, SessionModeId,
        };
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let modes = SessionModeState::new(
            SessionModeId::new("plan"),
            vec![
                SessionMode::new(SessionModeId::new("plan"), "Plan"),
                SessionMode::new(SessionModeId::new("bypassPermissions"), "Build"),
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
        )];
        let commands = vec![AvailableCommand::new("compact", "Compact the context")];
        mapper.on_session_state(Some(&modes), &options, &commands, &mut out);
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("config_state serializes"),
            json!({
                "kind": "config_state",
                "options": [],
                "currentMode": "plan",
                "modes": [
                    {"id": "plan", "label": "Plan"},
                    {"id": "bypassPermissions", "label": "Build"}
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

    /// EXP-748: the journal drops a subagent's tool rows before anything of
    /// the main transcript, so the completed edge has to carry the total the
    /// caption falls back on.
    #[test]
    fn a_completed_subagent_edge_carries_its_tool_call_count() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let start = SubagentEdge {
            id: "task-1".to_string(),
            agent_type: "explore".to_string(),
            status: SubagentEdgeStatus::Started,
            detail: None,
            tool_calls: None,
        };
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("", None))).meta(start.to_meta()),
            &mut out,
        );

        let mut owned = serde_json::Map::new();
        owned.insert(
            SUBAGENT_ID_META_KEY.to_string(),
            json!("task-1"),
        );
        for i in 0..3 {
            let call = ToolCall::new(ToolCallId::new(format!("tc-{i}")), "Read file.rs")
                .kind(ToolKind::Read);
            mapper.on_update(
                &notify(SessionUpdate::ToolCall(call)).meta(owned.clone()),
                &mut out,
            );
        }
        // A call of the MAIN line is never attributed to the subagent.
        mapper.on_update(
            &notify(SessionUpdate::ToolCall(
                ToolCall::new(ToolCallId::new("tc-main"), "Read main.rs").kind(ToolKind::Read),
            )),
            &mut out,
        );

        let mut out = MapOut::default();
        mapper.on_subagent(
            &SubagentEdge {
                status: SubagentEdgeStatus::Completed,
                ..start
            },
            &mut out,
        );
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("subagent serializes"),
            json!({
                "kind": "subagent",
                "id": "task-1",
                "agentType": "explore",
                "status": "completed",
                "toolCalls": 3
            })
        );

        // A subagent that ran no tools keeps the key off the wire entirely.
        let mut out = MapOut::default();
        mapper.on_subagent(
            &SubagentEdge {
                id: "task-2".to_string(),
                agent_type: "plan".to_string(),
                status: SubagentEdgeStatus::Completed,
                detail: None,
                tool_calls: None,
            },
            &mut out,
        );
        assert_eq!(
            serde_json::to_value(&out.wire[0]).expect("subagent serializes"),
            json!({"kind": "subagent", "id": "task-2", "agentType": "plan", "status": "completed"})
        );
    }

    /// EXP-772: two flushes of ONE message carry the SAME `messageId`, so a
    /// client merges them back into one row.
    #[test]
    fn every_flush_of_one_message_carries_the_same_message_id() {
        let mut mapper = mapper();
        let mut first = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("Looking", Some("msg-1")))),
            &mut first,
        );
        // A turn end flushes the first half.
        mapper.on_stop(StopReason::EndTurn, &mut first);
        let mut second = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk(" at it.", Some("msg-1")))),
            &mut second,
        );
        mapper.on_stop(StopReason::EndTurn, &mut second);
        let ids: Vec<Option<String>> = first
            .wire
            .iter()
            .chain(second.wire.iter())
            .filter_map(|event| match event {
                ActivityEvent::Narration { message_id, .. } => Some(message_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec![Some("msg-1".to_string()), Some("msg-1".to_string())]);
        // An id-less agent still publishes, without the merge key.
        let mut plain = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("done", None))),
            &mut plain,
        );
        mapper.on_stop(StopReason::EndTurn, &mut plain);
        assert!(matches!(
            &plain.wire[0],
            ActivityEvent::Narration { message_id: None, .. }
        ));
    }

    /// A thought and the answer that follows it carry ONE agent `messageId`,
    /// and clients merge narration rows by it: the thought has to publish
    /// under an id of its own or the chain of thought lands in the answer.
    #[test]
    fn a_thought_and_its_answer_never_share_a_message_id() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentThoughtChunk(chunk("Weighing it up", Some("msg-1")))),
            &mut out,
        );
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("Here is the fix.", Some("msg-1")))),
            &mut out,
        );
        mapper.on_stop(StopReason::EndTurn, &mut out);
        let ids: Vec<Option<String>> = out
            .wire
            .iter()
            .filter_map(|event| match event {
                ActivityEvent::Narration { message_id, .. } => Some(message_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 2, "{:?}", out.wire);
        assert_ne!(ids[0], ids[1], "the thought merged into the answer row");
        // The answer keeps the agent's own id; the thought derives one, so
        // two thoughts of one message still merge with each other.
        assert_eq!(ids[1], Some("msg-1".to_string()));
        assert_eq!(ids[0], Some(thought_message_id("msg-1")));
        // The local item agrees with the wire row.
        assert!(out.local.iter().any(|event| matches!(
            event,
            LocalFeedEvent::Thought { message_id, .. }
                if message_id.as_deref() == Some(thought_message_id("msg-1").as_str())
        )));
        // A long id still fits the relay's id cap after the suffix.
        assert!(thought_message_id(&"m".repeat(ID_MAX * 2)).len() <= ID_MAX);
    }

    /// codex streams a tool call's output PAST its completed status: the
    /// settled row is kept, so the trailing content-only update still knows
    /// the card's title and kind (an `Execute` card's output is dropped
    /// outright when the kind is lost).
    #[test]
    fn a_settled_tool_calls_trailing_output_still_lands_on_its_card() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-out"), "ls -la").kind(ToolKind::Execute);
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        let settled = ToolCallUpdate::new(
            ToolCallId::new("tc-out"),
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(settled)), &mut MapOut::default());

        let trailing_update = ToolCallUpdate::new(
            ToolCallId::new("tc-out"),
            ToolCallUpdateFields::new().content(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new("total 8"),
            ))]),
        );
        let mut trailing = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::ToolCallUpdate(trailing_update)),
            &mut trailing,
        );
        assert!(
            trailing.local.iter().any(|event| matches!(
                event,
                LocalFeedEvent::Output { tool_call_id, chunk, .. }
                    if tool_call_id == "tc-out" && chunk == "total 8"
            )),
            "{:?}",
            trailing.local
        );
        assert!(
            trailing.local.iter().any(|event| matches!(
                event,
                LocalFeedEvent::ToolCall { title, .. } if title == "ls -la"
            )),
            "{:?}",
            trailing.local
        );
    }

    /// The retained rows are bounded, and an id the agent REUSES for a new
    /// call is never evicted by the settle of the old one.
    #[test]
    fn settled_tool_rows_are_bounded_and_a_reused_id_is_kept() {
        fn start(mapper: &mut Mapper, id: &str) {
            let call = ToolCall::new(ToolCallId::new(id.to_string()), "ls -la").kind(ToolKind::Execute);
            mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut MapOut::default());
        }
        fn settle(mapper: &mut Mapper, id: &str) {
            let update = ToolCallUpdate::new(
                ToolCallId::new(id.to_string()),
                ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
            );
            mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(update)), &mut MapOut::default());
        }

        let mut mapper = mapper();
        // Settled once, then live again under the same id.
        start(&mut mapper, "tc-reused");
        settle(&mut mapper, "tc-reused");
        start(&mut mapper, "tc-reused");

        for n in 0..SETTLED_TOOLS_MAX + 8 {
            let id = format!("tc-{n}");
            start(&mut mapper, &id);
            settle(&mut mapper, &id);
        }
        // The oldest settled rows are gone, the newest are kept.
        assert!(!mapper.tools.contains_key("tc-0"));
        assert!(mapper.tools.contains_key(&format!("tc-{}", SETTLED_TOOLS_MAX + 7)));
        assert!(mapper.settled_tools.len() <= SETTLED_TOOLS_MAX);
        // The reused id belongs to the LIVE call, which no eviction touches.
        assert!(mapper.tools.contains_key("tc-reused"));
    }

    /// EXP-766 eviction is keyed on the ask id, and an agent may reuse one
    /// for a NEW card: popping it must not delete an ask that is still open.
    #[test]
    fn a_reused_ask_id_survives_the_answered_ask_eviction() {
        fn ask(mapper: &mut Mapper, id: &str) {
            let request = RequestPermissionRequest::new(
                SessionId::new("acp-1"),
                ToolCallUpdate::new(
                    ToolCallId::new(id.to_string()),
                    ToolCallUpdateFields::new().title("Write src/main.rs"),
                ),
                vec![PermissionOption::new(
                    PermissionOptionId::new("allow"),
                    "Yes",
                    PermissionOptionKind::AllowOnce,
                )],
            );
            mapper.on_permission(&request, &mut MapOut::default());
        }
        fn answer(id: &str) -> steer::RemoteAnswer {
            steer::RemoteAnswer {
                question_id: id.to_string(),
                ask_id: None,
                keys: vec!["allow".to_string()],
                text: None,
            }
        }

        let mut mapper = mapper();
        ask(&mut mapper, "tc-reused");
        let answered = answer("tc-reused");
        mapper.on_answer(&mapper.ask_key(&answered), &answered, &mut MapOut::default());
        // The same id, a NEW open card.
        ask(&mut mapper, "tc-reused");
        // Enough answers to pop the retired entry that named it.
        for n in 0..ANSWERED_ASKS_MAX {
            let id = format!("tc-{n}");
            ask(&mut mapper, &id);
            let answered = answer(&id);
            mapper.on_answer(&mapper.ask_key(&answered), &answered, &mut MapOut::default());
        }
        assert!(mapper.permissions.contains_key("tc-reused"));
        assert!(mapper.questions.contains_key("tc-reused"));
        assert_eq!(mapper.pending_asks(), 1);
        // Still answerable, not a dropped card.
        let answered = answer("tc-reused");
        let mut out = MapOut::default();
        assert_eq!(
            mapper.on_answer(&mapper.ask_key(&answered), &answered, &mut out),
            AnswerDecision::Permission {
                option_id: "allow".to_string()
            }
        );
    }

    /// EXP-773: prose and turns inside a subagent are scoped to it, and the
    /// subagent's FIRST user turn (its starting prompt) is dropped.
    #[test]
    fn subagent_prose_is_scoped_and_its_first_prompt_is_dropped() {
        let mut mapper = mapper();
        let mut meta = serde_json::Map::new();
        meta.insert(SUBAGENT_ID_META_KEY.to_string(), json!("task-1"));

        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::UserMessageChunk(chunk("Explore the repo", Some("u-1"))))
                .meta(meta.clone()),
            &mut out,
        );
        mapper.on_stop(StopReason::EndTurn, &mut out);
        assert!(
            !out.wire.iter().any(|event| matches!(event, ActivityEvent::UserMessage { .. })),
            "the starting prompt is the Task card's own title: {:?}",
            out.wire
        );

        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("Found it.", Some("m-1"))))
                .meta(meta.clone()),
            &mut out,
        );
        mapper.on_update(
            &notify(SessionUpdate::UserMessageChunk(chunk("keep going", Some("u-2"))))
                .meta(meta.clone()),
            &mut out,
        );
        mapper.on_stop(StopReason::EndTurn, &mut out);
        assert!(
            matches!(
                &out.wire[0],
                ActivityEvent::Narration { subagent_id: Some(id), .. } if id == "task-1"
            ),
            "{:?}",
            out.wire
        );
        assert!(
            matches!(
                &out.wire[1],
                ActivityEvent::UserMessage { subagent_id: Some(id), .. } if id == "task-1"
            ),
            "{:?}",
            out.wire
        );

        // The main line stays unscoped.
        let mut out = MapOut::default();
        mapper.on_update(
            &notify(SessionUpdate::AgentMessageChunk(chunk("Back on the main thread.", Some("m-2")))),
            &mut out,
        );
        mapper.on_stop(StopReason::EndTurn, &mut out);
        assert!(matches!(
            &out.wire[0],
            ActivityEvent::Narration { subagent_id: None, .. }
        ));
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

    /// EXP-784: the hook lane A calls — cleaned, deduped, and an empty/`ok`
    /// status is the one CLEAR frame.
    #[test]
    fn emit_rate_limit_dedupes_and_clears_on_ok() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        mapper.emit_rate_limit(
            " allowed_warning ",
            Some(1_700_000_000_000),
            Some("  80% of 5h used by expu_supersecretkey  "),
            &mut out,
        );
        mapper.emit_rate_limit("allowed_warning", Some(1_700_000_000_000), Some("80% of 5h used by expu_supersecretkey"), &mut out);
        assert_eq!(out.wire.len(), 1, "an identical re-emit says nothing");
        let first = serde_json::to_value(&out.wire[0]).unwrap();
        assert_eq!(first["kind"], "rate_limit");
        assert_eq!(first["status"], "allowed_warning");
        assert_eq!(first["resetsAt"], 1_700_000_000_000i64);
        let message = first["message"].as_str().unwrap();
        assert!(!message.contains("expu_"), "{message}");
        assert!(message.starts_with("80% of 5h used by"), "{message}");

        mapper.emit_rate_limit("rejected", Some(-5), Some("   "), &mut out);
        let second = serde_json::to_value(&out.wire[1]).unwrap();
        assert_eq!(second, json!({"kind": "rate_limit", "status": "rejected"}));

        mapper.emit_rate_limit("ok", Some(1), Some("fine"), &mut out);
        mapper.emit_rate_limit("", None, None, &mut out);
        assert_eq!(out.wire.len(), 3);
        assert_eq!(
            serde_json::to_value(&out.wire[2]).unwrap(),
            json!({"kind": "rate_limit", "status": ""}),
            "ok and empty are the same clear, sent once"
        );
        // The local feed saw the same three (the emit twin rule).
        let local = out
            .local
            .iter()
            .filter(|event| matches!(event, LocalFeedEvent::Activity { event: ActivityEvent::RateLimit { .. }, .. }))
            .count();
        assert_eq!(local, 3);
    }
}
