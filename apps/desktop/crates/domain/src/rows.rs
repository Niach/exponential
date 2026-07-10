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
use crate::hydrate::{
    tolerant_i64, tolerant_opt_bool, tolerant_opt_f64, tolerant_opt_i64, tolerant_opt_json,
};

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
    /// `project_type` (v7) — `dev` / `tasks` / `feedback`. `type` is a Rust
    /// keyword, so this renames onto the snake_case column `type`. `None` on
    /// legacy rows (deployed before the column existed) — treated as `dev`
    /// everywhere via [`Project::is_dev`], matching the server default.
    #[serde(default, rename = "type")]
    pub project_type: Option<String>,
    /// `projects.repository_id` (v4 §3.1, nullable since v7) — the one
    /// repository this project clones/branches against. `None` for repo-less
    /// `tasks`/`feedback` boards and legacy rows until the server backfill
    /// lands.
    #[serde(default)]
    pub repository_id: Option<String>,
    /// Feedback-board anonymous-visitor toggles (v7). Inert on other types;
    /// carried so the desktop mirrors the full shape (P7 live-coding reads
    /// `public_show_coding`). `None` on legacy/other rows.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub public_show_comments: Option<bool>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub public_show_activity: Option<bool>,
    /// `off` / `badge` / `live` — raw wire value (contract-locked).
    #[serde(default)]
    pub public_show_coding: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Project {
    /// Whether this is a repo-backed `dev` board. An absent/unknown
    /// `project_type` counts as `dev` — the server default and the type every
    /// legacy row predates. Drives the coding affordances (a non-dev repo-less
    /// board never shows Start coding) and the picker glyph.
    pub fn is_dev(&self) -> bool {
        match self.project_type.as_deref() {
            Some(crate::contract::PROJECT_TYPE_TASKS)
            | Some(crate::contract::PROJECT_TYPE_FEEDBACK) => false,
            _ => true,
        }
    }

    /// Whether this is a public `feedback` board.
    pub fn is_feedback(&self) -> bool {
        self.project_type.as_deref() == Some(crate::contract::PROJECT_TYPE_FEEDBACK)
    }

    /// Whether coding sessions on this project publish a PUBLIC live activity
    /// stream (§P7): a feedback board with `public_show_coding == 'live'`. Gates
    /// both the "Keep private" opt-out UI and the activity emitter itself.
    pub fn is_live_public_coding(&self) -> bool {
        self.is_feedback()
            && self.public_show_coding.as_deref()
                == Some(crate::contract::PUBLIC_CODING_VISIBILITY_LIVE)
    }
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

/// Display fallback for a user id whose [`User`] row didn't sync: the server
/// no longer syncs user rows for public-workspace co-members, so a known id can
/// resolve to no row. Rather than leak the raw id (or a misleading "Someone"),
/// show `Member <LAST4>` — the uppercased last four chars of the id.
pub fn member_fallback_label(user_id: &str) -> String {
    let chars: Vec<char> = user_id.chars().collect();
    let start = chars.len().saturating_sub(4);
    let tail: String = chars[start..].iter().collect();
    format!("Member {}", tail.to_uppercase())
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

/// `workspace_invites` shape row (requireAuth shape, §5.9). The server's
/// columns allowlist excludes the bearer `token` from the shape (REV-4/14) —
/// the invite link is built once from the create mutation's response, never
/// from synced rows.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkspaceInvite {
    pub id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub invited_by_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
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
    /// jsonb payload. The store pins TEXT storage (§5.5), so hydrate must
    /// re-parse the stringified object back into structured JSON — without
    /// this the timeline reads `payload.to` off a string and renders
    /// "changed status to ‹blank›" (EXP-33).
    #[serde(default, deserialize_with = "tolerant_opt_json")]
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
    fn member_fallback_uses_uppercased_last_four() {
        assert_eq!(member_fallback_label("user_abc123ef"), "Member 23EF");
        // Shorter ids just use the whole id.
        assert_eq!(member_fallback_label("ab"), "Member AB");
        assert_eq!(member_fallback_label(""), "Member ");
    }

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
    fn project_type_renames_from_type_column_and_classifies() {
        // The wire column is `type` (a Rust keyword) — it must land on
        // `project_type` and drive is_dev/is_feedback.
        let feedback: Project = serde_json::from_value(json!({
            "id": "p-1",
            "workspace_id": "w-1",
            "name": "Feedback",
            "type": "feedback",
            "public_show_comments": "t",
            "public_show_coding": "badge"
        }))
        .unwrap();
        assert_eq!(feedback.project_type.as_deref(), Some("feedback"));
        assert!(feedback.is_feedback());
        assert!(!feedback.is_dev());
        assert_eq!(feedback.public_show_comments, Some(true));
        assert_eq!(feedback.public_show_coding.as_deref(), Some("badge"));

        let tasks: Project = serde_json::from_value(json!({
            "id": "p-2", "workspace_id": "w-1", "name": "Tasks", "type": "tasks"
        }))
        .unwrap();
        assert!(!tasks.is_dev());
        assert!(!tasks.is_feedback());

        // A legacy row (deployed before the column existed) has no `type` —
        // it must classify as dev, matching the server default.
        let legacy: Project = serde_json::from_value(json!({
            "id": "p-3", "workspace_id": "w-1", "name": "Legacy"
        }))
        .unwrap();
        assert_eq!(legacy.project_type, None);
        assert!(legacy.is_dev());
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
