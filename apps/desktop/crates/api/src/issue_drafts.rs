//! Typed `issueDrafts.*` tRPC helpers (EXP-878 issue drafts). Verified
//! against `apps/web/src/lib/trpc/issue-drafts.ts`:
//!
//! - `issueDrafts.upsert({id, teamId, boardId, title, description, …})` →
//!   `{draft, txId}` — the ONE write. It is keyed by the CLIENT-minted id
//!   ([`new_draft_id`]), so re-sending the same snapshot is idempotent: the
//!   composer mints one id per dialog session and upserts it on close (and
//!   once more, eagerly, the first time a paste needs a row to upload onto).
//! - `issueDrafts.delete({id})` → `{txId, deleted}` — closing a draft with
//!   everything cleared, and the create path's server-side cleanup echo.
//! - `issueDrafts.listAttachments({id})` → the draft's attachment rows, the
//!   same JSON the `attachments` shape carries. Draft-owned attachments are
//!   server-only (the shape excludes `issue_id IS NULL`), so reopening a
//!   draft fetches its file rail from here.
//!
//! The row itself re-streams over the per-user `issue_drafts` shape, so
//! callers fire-and-forget and let the collection settle the list.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;
use domain::IssuePriority;

/// A fresh client-minted draft id. The dialog owns ONE for its whole life —
/// the upsert is keyed by it, so every write from the same dialog session
/// lands on the same row (and a never-saved id simply never exists).
pub fn new_draft_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// `issueDrafts.upsert` input. Optionals are OMITTED when unset (the server
/// treats an absent key as "clear"), but `label_ids` is ALWAYS sent: it is
/// the full replacement set, and omitting it on a draft that had labels
/// would silently keep the stale ones.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDraftUpsertInput {
    pub id: String,
    pub team_id: String,
    pub board_id: String,
    pub title: String,
    pub description: String,
    /// EXP-314: the precise `issue_statuses` row; omitted = the team's
    /// Backlog builtin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    pub label_ids: Vec<String>,
    /// `YYYY-MM-DD`, or an explicit `null` to clear — date only, like the
    /// issue field (REV2-49).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
}

impl IssueDraftUpsertInput {
    pub fn new(
        id: impl Into<String>,
        team_id: impl Into<String>,
        board_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            team_id: team_id.into(),
            board_id: board_id.into(),
            title: String::new(),
            description: String::new(),
            status_id: None,
            priority: None,
            assignee_id: None,
            label_ids: Vec::new(),
            due_date: None,
        }
    }
}

/// `issueDrafts.upsert` output. Only `txId` is read today (the row arrives
/// over the shape); `draft` is decoded loosely so a server-side field
/// addition never fails the call.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDraftUpsertOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub draft: Option<serde_json::Value>,
}

/// `issueDrafts.delete` output.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDraftDeleteOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
    /// `false` when the id named nothing (already filed, already deleted) —
    /// never an error on this path.
    #[serde(default)]
    pub deleted: bool,
}

/// One attachment row of a draft — the same JSON shape the `attachments`
/// shape carries (camelCase over tRPC).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachment {
    pub id: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub poster_storage_key: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// `issueDrafts.upsert` — mutation.
pub fn issue_drafts_upsert(
    trpc: &TrpcClient,
    input: &IssueDraftUpsertInput,
) -> Result<IssueDraftUpsertOutput, ApiError> {
    trpc.mutation("issueDrafts.upsert", input)
}

/// `issueDrafts.delete` — mutation.
pub fn issue_drafts_delete(
    trpc: &TrpcClient,
    id: &str,
) -> Result<IssueDraftDeleteOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("issueDrafts.delete", &Input { id })
}

/// `issueDrafts.listAttachments` — query. Draft-owned attachments never
/// sync, so this is the only way to repopulate a reopened draft's file rail.
pub fn issue_drafts_list_attachments(
    trpc: &TrpcClient,
    id: &str,
) -> Result<Vec<DraftAttachment>, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.query_with_input("issueDrafts.listAttachments", &Input { id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn minted_ids_are_unique_uuids() {
        let a = new_draft_id();
        let b = new_draft_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36, "{a}");
    }

    #[test]
    fn upsert_omits_unset_optionals_but_always_sends_label_ids() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":7,"draft":{"id":"d-1"}}}}"#);
        let input = IssueDraftUpsertInput::new("d-1", "t-1", "b-1");
        let out = issue_drafts_upsert(&client(&base), &input).unwrap();
        assert_eq!(out.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issueDrafts.upsert HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"id":"d-1","teamId":"t-1","boardId":"b-1","title":"","description":"","labelIds":[]}"#
        ));
    }

    #[test]
    fn upsert_serializes_every_pick() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":8}}}"#);
        let mut input = IssueDraftUpsertInput::new("d-1", "t-1", "b-1");
        input.title = "Title".into();
        input.description = "Body".into();
        input.status_id = Some("s-1".into());
        input.priority = Some(IssuePriority::High);
        input.assignee_id = Some("u-1".into());
        input.label_ids = vec!["l-1".into()];
        input.due_date = Some("2026-09-14".into());
        issue_drafts_upsert(&client(&base), &input).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"id":"d-1","teamId":"t-1","boardId":"b-1","title":"Title","description":"Body","statusId":"s-1","priority":"high","assigneeId":"u-1","labelIds":["l-1"],"dueDate":"2026-09-14"}"#
        ));
    }

    #[test]
    fn delete_posts_the_id_and_decodes_the_flag() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":9,"deleted":true}}}"#);
        let out = issue_drafts_delete(&client(&base), "d-1").unwrap();
        assert!(out.deleted);
        assert_eq!(out.tx_id, Some(9));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issueDrafts.delete HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"d-1"}"#));
    }

    #[test]
    fn list_attachments_gets_the_rows() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":[{"id":"att-1","filename":"a.pdf","contentType":"application/pdf","sizeBytes":12,"url":"/api/attachments/att-1"}]}}"#,
        );
        let rows = issue_drafts_list_attachments(&client(&base), "d-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].filename.as_deref(), Some("a.pdf"));
        assert_eq!(rows[0].size_bytes, Some(12));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/issueDrafts.listAttachments?input="));
    }
}
