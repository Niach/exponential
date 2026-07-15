//! Typed row structs for the 15 synced shapes (masterplan-v3 §5.1/§5.5) —
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
    /// Whether this is a public board — anyone with the link can read it. The
    /// canonical publicness signal.
    /// NOT NULL default false server-side, but `Option` locally: a store-healed
    /// column is SQL NULL on pre-existing rows (explicit JSON null at hydrate,
    /// which `serde(default)` does NOT cover), and a not-yet-updated
    /// self-hosted server never serves the column — a required bool would drop
    /// the whole row (§5.5). Read sites default to `false`.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub is_public: Option<bool>,
    /// Curated icon name (`crate::contract::PROJECT_ICON_VALUES`) chosen at
    /// create time. `None` on pre-icon boards — consumers fall back to an
    /// attribute-derived glyph.
    #[serde(default)]
    pub icon: Option<String>,
    /// `projects.repository_id` (v4 §3.1, nullable) — the one repository this
    /// project clones/branches against, or `None` for a repo-less board. Coding
    /// affordances gate purely on this presence, never on board type.
    #[serde(default)]
    pub repository_id: Option<String>,
    /// Feedback-board anonymous-visitor toggles (v7). Inert on other types;
    /// carried so the desktop mirrors the full shape. `None` on legacy/other
    /// rows.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub public_show_comments: Option<bool>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub public_show_activity: Option<bool>,
    /// Trash contract: protected projects (the bootstrap dogfood board) are
    /// non-deletable/non-archivable/non-retypable — the server refuses, and
    /// clients disable the affordances from this flag. `None` on legacy rows.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub is_protected: Option<bool>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub archived_at: Option<String>,
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
/// `issue_id` is NULL on batch-scoped (multi-issue) session rows — those
/// carry only `workspace_id` (enforced by the tRPC writer).
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
    fn project_public_and_icon_hydrate() {
        // Publicness comes from `is_public` and the glyph from `icon`; the
        // dropped `type` column no longer arrives (a stray one is simply
        // ignored — no field models it).
        let public: Project = serde_json::from_value(json!({
            "id": "p-1",
            "workspace_id": "w-1",
            "name": "Feedback",
            "is_public": "t",
            "icon": "megaphone",
            "public_show_comments": "t"
        }))
        .unwrap();
        assert_eq!(public.is_public, Some(true));
        assert_eq!(public.icon.as_deref(), Some("megaphone"));
        assert_eq!(public.public_show_comments, Some(true));

        let private: Project = serde_json::from_value(json!({
            "id": "p-2", "workspace_id": "w-1", "name": "Tasks",
            "is_public": false, "icon": "square-kanban"
        }))
        .unwrap();
        assert_eq!(private.is_public, Some(false));
        assert_eq!(private.icon.as_deref(), Some("square-kanban"));

        // A row missing the new columns degrades: publicness to None (read
        // sites default to false), glyph to None (attribute-derived fallback).
        let sparse: Project = serde_json::from_value(json!({
            "id": "p-3", "workspace_id": "w-1", "name": "Sparse"
        }))
        .unwrap();
        assert_eq!(sparse.is_public, None);
        assert_eq!(sparse.icon, None);
    }

    #[test]
    fn project_with_null_is_public_hydrates_not_dropped() {
        // The 0.8.4→0.8.5 upgrade regression: `heal_missing_columns` ALTERs
        // `is_public` in as TEXT NULL on pre-existing rows, and the store's
        // hydrate read re-wraps SQL NULL as EXPLICIT JSON null — which
        // `serde(default)` does not cover. A required bool here made every
        // pre-upgrade project row fail hydration and silently vanish from the
        // project list (§5.5: a partial row degrades, never drops).
        let healed: Project = serde_json::from_value(json!({
            "id": "p-1", "workspace_id": "w-1", "name": "Healed",
            "is_public": null
        }))
        .expect("explicit-null is_public must not drop the row");
        assert_eq!(healed.is_public, None);
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
