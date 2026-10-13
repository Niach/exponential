//! EXP-1292 — the device's read of the Threads v2 shapes (`sessions`,
//! `messages`, `session_issues`), decoded straight off a shape store's TEXT
//! rows (§5.5 tolerant decode: an undecodable row is dropped, never fatal).
//!
//! TEMPORARY home: #EXP-1294 owns the row model (`domain::rows`) and the
//! sync shape specs. Once its `SessionRow`/`MessageRow` land, these structs
//! go and the pickup reads those instead. Only the columns the DEVICE reads
//! are modelled here (the shapes' allowlists are wider).

use domain::hydrate::{tolerant_bool, tolerant_opt_i64, tolerant_opt_json};
use serde::Deserialize;

/// `sessions.kind` of a root that is a team agent (never ends).
pub const SESSION_KIND_AGENT: &str = "agent";
/// `sessions.status` values (contract `sessionStatus`).
pub const SESSION_STATUS_QUEUED: &str = "queued";
pub const SESSION_STATUS_RUNNING: &str = "running";
pub const SESSION_STATUS_IDLE: &str = "idle";
pub const SESSION_STATUS_ENDED: &str = "ended";
/// `messages.author_kind` of a person's message (the only kind delivered).
pub const MESSAGE_AUTHOR_USER: &str = "user";

/// One `sessions` row: a thread root, a team agent root or a sub-session.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SessionRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub root_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub depth: Option<i64>,
    /// `thread` | `agent` (roots only; children are `thread`).
    #[serde(default)]
    pub kind: Option<String>,
    /// The starter.
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub cli: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub plan_mode: bool,
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub computer_use: bool,
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub ultracode: bool,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    /// `queued` | `running` | `idle` | `ended`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub busy: bool,
    #[serde(default)]
    pub title: Option<String>,
    /// Agent facets (kind `agent`).
    #[serde(default)]
    pub name: Option<String>,
    /// jsonb `AgentTrigger[]`, TEXT-stored; `coding::triggers` parses it.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub triggers: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl SessionRow {
    pub fn is_agent(&self) -> bool {
        self.parent_id.is_none() && self.kind.as_deref() == Some(SESSION_KIND_AGENT)
    }

    pub fn status_is(&self, status: &str) -> bool {
        self.status.as_deref() == Some(status)
    }

    pub fn on_device(&self, device_id: &str) -> bool {
        self.device_id.as_deref() == Some(device_id)
    }

    /// The tree's root id (a root is its own root).
    pub fn root(&self) -> &str {
        self.root_id.as_deref().unwrap_or(&self.id)
    }
}

/// One `messages` row of the Conversation.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct MessageRow {
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub root_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    /// `user` | `agent` | `system`.
    #[serde(default)]
    pub author_kind: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// jsonb `MessageCard`, TEXT-stored.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub card: Option<serde_json::Value>,
    #[serde(default)]
    pub answered_at: Option<String>,
    /// NULL = queued for the agent (not yet delivered).
    #[serde(default)]
    pub consumed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

impl MessageRow {
    /// A person's message still waiting for delivery into its session.
    pub fn is_pending_person_message(&self) -> bool {
        self.author_kind.as_deref() == Some(MESSAGE_AUTHOR_USER)
            && self.consumed_at.is_none()
            && self.card.is_none()
    }

    /// The card's `kind`, when the row carries one.
    pub fn card_kind(&self) -> Option<&str> {
        self.card.as_ref()?.get("kind")?.as_str()
    }
}

/// One `session_issues` row (root ↔ issue).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionIssueRow {
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub sort_order: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_stored_session_row_decodes() {
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s1",
            "kind": "agent",
            "device_id": "dev",
            "status": "idle",
            "busy": "false",
            "plan_mode": "true",
            "triggers": "[{\"id\":\"t\",\"enabled\":true,\"kind\":\"schedule\"}]",
            "some_future_column": "x"
        }))
        .unwrap();
        assert!(row.is_agent());
        assert!(row.plan_mode);
        assert!(!row.busy);
        assert!(row.on_device("dev"));
        assert_eq!(row.root(), "s1");
        assert!(row.triggers.unwrap().is_array());
    }

    #[test]
    fn only_unconsumed_card_less_person_rows_are_pending() {
        let pending: MessageRow = serde_json::from_value(serde_json::json!({
            "id": "m1", "session_id": "s1", "author_kind": "user", "body": "hi"
        }))
        .unwrap();
        assert!(pending.is_pending_person_message());
        let card: MessageRow = serde_json::from_value(serde_json::json!({
            "id": "m2", "author_kind": "agent", "card": "{\"kind\":\"question\"}"
        }))
        .unwrap();
        assert!(!card.is_pending_person_message());
        assert_eq!(card.card_kind(), Some("question"));
    }
}
