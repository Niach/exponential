//! EXP-746 — the LOCAL feed vocabulary (desktop session screen, `exponential
//! code`'s line printer).
//!
//! Everything here is host-local by construction. The wire half of a session
//! is [`steer::ActivityEvent`] and nothing else: per-edit diffs, command
//! output, the pinned plan and thoughts are strictly richer than the relay
//! contract and would leak raw patch bodies and command strings to every
//! viewer if they rode it (§4.2). The one crossover is
//! [`LocalFeedEvent::Activity`], which carries the SAME event the relay got so
//! `steer::feed::SteerFeed::apply` works unchanged on the local path.
//!
//! Types land in P0; the producers are lane E1 (mapper + lifecycle).

use std::path::PathBuf;

/// A local renderer's view of one engine event.
#[derive(Clone, Debug)]
pub enum LocalFeedEvent {
    /// The exact event that went to the relay. `tool_call_id` names the ACP
    /// tool call it belongs to (when it has one) so the desktop can hang
    /// local extras — a diff card, an output card — off the feed row the
    /// event appends. `config_state` and `usage` ride here too (D4).
    Activity {
        event: steer::ActivityEvent,
        tool_call_id: Option<String>,
    },
    /// A tool call surfaced or changed status — the collapsible card header.
    ToolCall {
        id: String,
        title: String,
        kind: ToolCardKind,
        status: ToolCardStatus,
        locations: Vec<PathBuf>,
    },
    /// ACP `ToolCallContent::Diff` — one edit, rendered as a diff card.
    EditDiff {
        tool_call_id: String,
        path: PathBuf,
        old_text: Option<String>,
        new_text: String,
    },
    /// Output of an `Execute` tool call, streamed. `exit_code` arrives with
    /// the final chunk. Never `ToolCallContent::Terminal`: the client
    /// advertises `terminal: false` (D5).
    Output {
        tool_call_id: String,
        chunk: String,
        exit_code: Option<i32>,
    },
    /// ACP `Plan` — the pinned plan card, replaced wholesale each time.
    Plan { entries: Vec<PlanEntryView> },
    /// An agent thought chunk, coalesced by `message_id`.
    Thought {
        message_id: Option<String>,
        text: String,
    },
    /// Connection edges the screen renders as banners.
    Phase(EnginePhase),
}

/// Where the session is in its life. `Connecting` covers spawn + handshake +
/// `session/new`; `Ended` is emitted exactly once, after the end sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnginePhase {
    Connecting,
    Live,
    Ended,
}

/// A local mirror of ACP `ToolKind` — the icon/label bucket a tool card
/// renders in. Mirrored (not re-exported) so the desktop never has to match
/// on a `#[non_exhaustive]` foreign enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCardKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    Other,
}

/// A local mirror of ACP `ToolCallStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCardStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// One row of the pinned plan card (a local mirror of ACP `PlanEntry`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanEntryView {
    pub content: String,
    pub priority: PlanEntryPriorityView,
    pub status: PlanEntryStatusView,
}

/// A local mirror of ACP `PlanEntryPriority`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanEntryPriorityView {
    High,
    Medium,
    Low,
}

/// A local mirror of ACP `PlanEntryStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanEntryStatusView {
    Pending,
    InProgress,
    Completed,
}
