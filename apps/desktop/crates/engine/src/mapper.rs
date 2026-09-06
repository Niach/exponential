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
//! Signatures land in P0; lane E1 fills every body.

use agent_client_protocol::schema::v1::{
    AvailableCommand, CreateElicitationRequest, RequestPermissionRequest, SessionConfigOption,
    SessionModeState, SessionNotification, StopReason,
};

use crate::local::LocalFeedEvent;

/// Everything the mapper needs that is constant for a session.
pub struct MapperConfig {
    /// Session secrets (installation token, the `expu_` personal key) plus the
    /// static patterns — every wire string passes through it.
    pub redactor: steer::Redactor,
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
    // EXP-746 E1: fill — coalescers keyed by MessageId, the tool-call table,
    // the subagent edges, the compaction state machine and the pending-ask
    // registry all live here.
}

impl Mapper {
    pub fn new(config: MapperConfig) -> Self {
        Self { config }
    }

    /// The session's constants (redactor, cwd, agent, seed).
    pub fn config(&self) -> &MapperConfig {
        &self.config
    }

    /// The main path: one `session/update` notification.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_update(&mut self, notification: &SessionNotification, out: &mut MapOut) {
        todo!("EXP-746 E1: map every SessionUpdate variant per the plan's mapping table")
    }

    /// A `session/request_permission`. Returns the key the host parks the
    /// `Responder` under. `ToolKind::SwitchMode` (ExitPlanMode) emits a
    /// PLAN card: `plan_mode: true` with the plan markdown as `text`, or all
    /// four clients degrade it to a generic question.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_permission(
        &mut self,
        request: &RequestPermissionRequest,
        out: &mut MapOut,
    ) -> PendingAskKey {
        todo!("EXP-746 E1: permission -> answerable question card (D3)")
    }

    /// An `elicitation/create` form, published as a `<ask>#<n>` … `<ask>#submit`
    /// stepper.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_elicitation(
        &mut self,
        ask_id: &str,
        request: &CreateElicitationRequest,
        out: &mut MapOut,
    ) -> PendingAskKey {
        todo!("EXP-746 E1: elicitation -> question stepper (D3)")
    }

    /// An inbound `answer` frame (relay or local composer). Publishes
    /// `answer_ack` then `question_resolved` and tells the host what to do
    /// with the parked responder.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_answer(
        &mut self,
        key: &PendingAskKey,
        answer: &steer::RemoteAnswer,
        out: &mut MapOut,
    ) -> AnswerDecision {
        todo!("EXP-746 E1: resolve the parked ask, ack, then question_resolved")
    }

    /// `session/cancel`: the ACP contract REQUIRES answering every pending
    /// permission with `Cancelled`, and every open card is dismissed.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_cancel(&mut self, out: &mut MapOut) {
        todo!("EXP-746 E1: dismiss every open ask and set idle")
    }

    /// A turn ended. No wire event: the stop reason only moves `out.idle`.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_stop(&mut self, stop: StopReason, out: &mut MapOut) {
        todo!("EXP-746 E1: flush the coalescers and set out.idle")
    }

    /// The full config snapshot from `session/new`, `session/load`,
    /// `session/set_config_option` or a `ConfigOptionUpdate` — always emitted
    /// WHOLE and clamped (`steer::clamp_config_state`), because
    /// `config_state` is latest-wins state, not a delta.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn on_session_state(
        &mut self,
        modes: Option<&SessionModeState>,
        options: &[SessionConfigOption],
        commands: &[AvailableCommand],
        out: &mut MapOut,
    ) {
        todo!("EXP-746 E1: build one clamped ConfigState from the whole snapshot")
    }

    /// Debounce tick: emits whatever the coalescers have been holding.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub fn flush(&mut self, out: &mut MapOut) {
        todo!("EXP-746 E1: flush the narration/thought buffers")
    }
}
