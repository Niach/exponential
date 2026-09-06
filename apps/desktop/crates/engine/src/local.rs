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

// ---------------------------------------------------------------------------
// The adapter-facing subagent seam (EXP-746)
// ---------------------------------------------------------------------------

/// The `_meta` key an adapter stamps a subagent edge under.
///
/// Subagents have no ACP shape of their own: an adapter that learns one
/// (claude's `system/task_started`, codex's `subAgentActivity`) puts the edge
/// into the `_meta` of ANY `session/update` notification it is already
/// sending, and [`crate::mapper::Mapper::on_update`] turns it into the relay
/// `subagent` event. A tool call belonging to a subagent carries
/// `_meta.subagentId` instead, which becomes `Tool { subagentId }`.
pub const SUBAGENT_META_KEY: &str = "exponentialSubagent";

/// The `_meta` key on a `ToolCall` (or the notification carrying it) naming
/// the subagent the call belongs to; it becomes `Tool { subagentId }` and must
/// equal the edge's `id` above.
pub const SUBAGENT_ID_META_KEY: &str = "subagentId";

/// The `_meta` key on a `CompactionUpdate` (or the notification carrying it)
/// naming what triggered the compaction — ACP has no field for it. Folded by
/// `steer::normalize_compaction_trigger` (`manual` stays, everything else is
/// `auto`).
pub const COMPACTION_TRIGGER_META_KEY: &str = "trigger";

/// One subagent lifecycle edge. Deliberately tiny: the relay vocabulary has
/// exactly `{id, agentType, status, detail?, toolCalls?}` and nothing an
/// adapter adds beyond that could be rendered anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentEdge {
    pub id: String,
    pub agent_type: String,
    pub status: SubagentEdgeStatus,
    pub detail: Option<String>,
    /// EXP-748: an adapter-side tool-call count, when the adapter knows it.
    /// The mapper keeps its own count from the attributed tool calls and
    /// prefers the larger of the two on the completed edge.
    pub tool_calls: Option<u32>,
}

/// A local mirror of `steer::SubagentStatus`, so an adapter never has to
/// name a relay type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentEdgeStatus {
    Started,
    Completed,
}

impl SubagentEdge {
    /// Adapter side: the `_meta` object to attach to a notification.
    pub fn to_meta(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut meta = serde_json::Map::new();
        let mut edge = serde_json::Map::new();
        edge.insert("id".to_string(), serde_json::Value::String(self.id.clone()));
        edge.insert(
            "agentType".to_string(),
            serde_json::Value::String(self.agent_type.clone()),
        );
        edge.insert(
            "status".to_string(),
            serde_json::Value::String(
                match self.status {
                    SubagentEdgeStatus::Started => "started",
                    SubagentEdgeStatus::Completed => "completed",
                }
                .to_string(),
            ),
        );
        if let Some(detail) = &self.detail {
            edge.insert(
                "detail".to_string(),
                serde_json::Value::String(detail.clone()),
            );
        }
        if let Some(tool_calls) = self.tool_calls {
            edge.insert(
                "toolCalls".to_string(),
                serde_json::Value::from(tool_calls),
            );
        }
        meta.insert(SUBAGENT_META_KEY.to_string(), serde_json::Value::Object(edge));
        meta
    }

    /// Engine side: the edge an adapter stamped, if any. A malformed one is
    /// ignored rather than failing the notification it rode on.
    pub fn from_meta(
        meta: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<SubagentEdge> {
        let edge = meta.get(SUBAGENT_META_KEY)?.as_object()?;
        let id = edge.get("id")?.as_str()?.to_string();
        if id.is_empty() {
            return None;
        }
        // The relay knows two states; an adapter's failed/cancelled edge is
        // a subagent that is no longer running, i.e. completed.
        let status = match edge.get("status").and_then(serde_json::Value::as_str) {
            Some("completed" | "failed" | "cancelled" | "stopped" | "ended") => {
                SubagentEdgeStatus::Completed
            }
            _ => SubagentEdgeStatus::Started,
        };
        Some(SubagentEdge {
            id,
            agent_type: edge
                .get("agentType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("agent")
                .to_string(),
            status,
            detail: edge
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            tool_calls: edge
                .get("toolCalls")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subagent_edge_round_trips_through_meta() {
        let edge = SubagentEdge {
            id: "task-1".to_string(),
            agent_type: "explore".to_string(),
            status: SubagentEdgeStatus::Completed,
            detail: Some("found it".to_string()),
            tool_calls: Some(3),
        };
        assert_eq!(SubagentEdge::from_meta(&edge.to_meta()), Some(edge));
    }

    #[test]
    fn a_meta_without_an_edge_is_ignored() {
        let meta = serde_json::Map::new();
        assert_eq!(SubagentEdge::from_meta(&meta), None);
    }
}
