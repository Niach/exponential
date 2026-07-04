//! Typed row structs for the 14 synced shapes (masterplan-v3 §5.1/§5.5) —
//! hand-written mirrors of `packages/db-schema`, one per Electric shape,
//! hydrated from the sync store's snake_case JSON objects.
//!
//! Hydration rules (§5.5):
//! * Values arrive as heterogeneous JSON scalars (SQLite TEXT re-wrapped, but
//!   also bare numbers/bools straight off the wire) — numeric/bool fields use
//!   the [`crate::hydrate`] tolerant deserializers, never strict types.
//! * Enum fields deserialize tolerantly ([`crate::enums`]): an unknown value
//!   becomes `Unknown`, never a dropped row.
//! * Only the primary key and identity-critical fields are required; every
//!   other column is `Option` so a partial row degrades gracefully. A row
//!   that fails hydration is caught and logged by the collections layer,
//!   never at apply (§5.5).
//! * Timestamps/dates stay ISO strings; parse at the UI edge if needed.
//!
//! Column sets intentionally mirror `sync::shapes::SHAPES` (the §5.4
//! known-column allowlists): no `email` on [`IssueSubscriber`] (PII stays
//! server-side), no stale `due_time`/`end_time` on [`Issue`].

use serde::Deserialize;

use crate::enums::{IssuePriority, IssueStatus};
use crate::hydrate::{tolerant_i64, tolerant_opt_bool, tolerant_opt_f64, tolerant_opt_i64};

/// `workspaces` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub is_public: Option<bool>,
    #[serde(default)]
    pub public_write_policy: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `projects` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Project {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// `projects.repository_id` (v4 §3.1) — the one repository this project
    /// clones/branches against. Additive shape column (shape count unchanged);
    /// `None` on legacy rows until the server backfill lands.
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub archived_at: Option<String>,
    /// jsonb on the server; stored/carried as raw JSON text.
    #[serde(default)]
    pub preview_config: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `issues` shape row (§5.5's exemplar struct plus the full column set).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    #[serde(deserialize_with = "tolerant_i64")]
    pub number: i64,
    pub identifier: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: IssueStatus,
    #[serde(default = "default_priority")]
    pub priority: IssuePriority,
    #[serde(default)]
    pub assignee_id: Option<String>,
    #[serde(default)]
    pub creator_id: Option<String>,
    /// `date` column — `"2026-05-20"`; parse at the UI edge if needed (§5.5).
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub duplicate_of_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub recurrence_interval: Option<i64>,
    #[serde(default)]
    pub recurrence_unit: Option<String>,
    #[serde(default)]
    pub pr_url: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub pr_number: Option<i64>,
    #[serde(default)]
    pub pr_state: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub pr_merged_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn default_priority() -> IssuePriority {
    IssuePriority::None
}

/// `labels` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Label {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `issue_labels` shape row — the ONLY composite-PK, id-less table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IssueLabel {
    pub issue_id: String,
    pub label_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// `users` shape row (co-member-scoped; full rows incl. email, §5.9).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub is_admin: Option<bool>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub is_agent: Option<bool>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `workspace_members` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkspaceMember {
    pub id: String,
    pub workspace_id: String,
    pub user_id: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `workspace_invites` shape row (requireAuth shape, §5.9).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkspaceInvite {
    pub id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub invited_by_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `comments` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    /// GFM markdown (the cross-client interchange contract).
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub edited_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `attachments` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub comment_id: Option<String>,
    #[serde(default)]
    pub uploader_id: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub storage_key: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Probed dimensions so clients can pre-size and avoid layout shift.
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub width: Option<i64>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub height: Option<i64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `notifications` shape row (requireAuth shape; `user_id = me`, §5.9).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    /// `notification_type` wire value — typed enum lands with the Phase-3
    /// inbox (§4.7); Phase 2 carries the raw string.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub read_at: Option<String>,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `issue_events` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IssueEvent {
    pub id: String,
    pub issue_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub actor_user_id: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// jsonb payload; carried as raw JSON.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `issue_subscribers` shape row — deliberately NO `email` field (§5.4/§5.9:
/// widget-reporter PII is excluded from sync; no local field may exist to
/// leak it).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IssueSubscriber {
    pub id: String,
    pub issue_id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub unsubscribed: Option<bool>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `coding_sessions` shape row (the cross-client "coding now" badge).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CodingSession {
    pub id: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    /// `running` / `ended` — raw wire value (contract-locked).
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn issue_hydrates_from_snake_map_with_heterogeneous_scalars() {
        // Mirrors the conformance-fixture shapes: bare number for `number`,
        // float for `sort_order`, TEXT forms elsewhere.
        let issue: Issue = serde_json::from_value(json!({
            "id": "01J9K0A0X3CB4E5F6G7H8J9K0L",
            "project_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "First issue",
            "description": null,
            "status": "in_progress",
            "priority": "high",
            "sort_order": "1.5",
            "assignee_id": null,
            "created_at": "2026-05-20T00:00:00Z",
            "updated_at": "2026-05-20T00:00:00Z"
        }))
        .expect("issue hydrates");
        assert_eq!(issue.number, 1);
        assert_eq!(issue.status, IssueStatus::InProgress);
        assert_eq!(issue.priority, IssuePriority::High);
        assert_eq!(issue.sort_order, Some(1.5));
        assert_eq!(issue.description, None);
    }

    #[test]
    fn issue_with_unknown_enum_value_is_kept_not_dropped() {
        let issue: Issue = serde_json::from_value(json!({
            "id": "i-1",
            "project_id": "p-1",
            "number": "7",
            "identifier": "EXP-7",
            "title": "Future status",
            "status": "brand_new_state",
            "priority": "urgent"
        }))
        .expect("unknown enum value must not drop the row (§5.5)");
        assert_eq!(issue.status, IssueStatus::Unknown);
        assert_eq!(issue.number, 7);
    }

    #[test]
    fn workspace_bool_hydrates_from_text_form() {
        let ws: Workspace = serde_json::from_value(json!({
            "id": "w-1",
            "name": "Exponential",
            "slug": "exponential",
            "is_public": "t"
        }))
        .unwrap();
        assert_eq!(ws.is_public, Some(true));
    }

    #[test]
    fn issue_subscriber_has_no_email_field() {
        // The PII belt (§5.4): even a wire row carrying `email` hydrates
        // without ever modeling it.
        let sub: IssueSubscriber = serde_json::from_value(json!({
            "id": "s-1",
            "issue_id": "i-1",
            "source": "widget_reporter",
            "unsubscribed": "f",
            "email": "reporter@example.com"
        }))
        .unwrap();
        assert_eq!(sub.unsubscribed, Some(false));
        // Compile-time guarantee: no `email` field exists to read.
    }
}
