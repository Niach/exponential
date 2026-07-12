//! Typed `issues.*` tRPC helpers.
//!
//! Two surfaces share this router mirror:
//!
//! 1. **Mutations** (masterplan-v3 §4.1/§4.2) — `issues.create` /
//!    `issues.update` / `issues.delete` / `issues.move`, verified against
//!    `apps/web/src/lib/trpc/issues.ts`: create returns `{issue, txId}`,
//!    update returns `{issue}` (NO txId — inline edits are the §4.1 un-gated
//!    form; the Electric echo re-renders), delete returns `{txId, id}`,
//!    move returns `{txId, issue, projectSlug}` (EXP-57).
//!    Update inputs use [`Patch`] for the zod `.nullable().optional()` fields
//!    (omit = unchanged, null = clear, value = set). Never pass
//!    `IssueStatus::Unknown` / `IssuePriority::Unknown` — they serialize as
//!    `"unknown"` and the server's schema rejects them.
//!
//! 2. **The `issues.prFiles` query** (masterplan-v3 §7.8) — the data source
//!    of the desktop side-by-side diff view.
//!
//! Wire shape verified against `apps/web/src/lib/trpc/issues.ts` (`prFiles`)
//! and `apps/web/src/lib/integrations/github-pr.ts` (`PullFile`):
//!
//! ```ts
//! // input
//! { issueId: string /* uuid */ }
//! // output
//! { repo: string | null, prNumber: number | null, files: PullFile[] }
//! interface PullFile { filename: string; status: string; additions: number;
//!                      deletions: number; patch?: string }
//! ```
//!
//! An issue without a linked PR returns `repo: null, prNumber: null,
//! files: []` (NOT an error); a GitHub-side failure surfaces as tRPC
//! `BAD_GATEWAY` → [`crate::ApiError::Http`] with the server's message.
//!
//! §7.8 forward-compat: the brief also names `sha` and `previousFilename` on
//! `PullFile` for rename-stable anchoring. The server does not send them yet
//! (the additive `fetchPullFiles` change is pending); they are decoded as
//! `Option` + `#[serde(default)]` so this wrapper works before and after that
//! server change without another client release.

use serde::{Deserialize, Serialize};

use domain::{IssuePriority, IssueStatus};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;

// ---------------------------------------------------------------------------
// Mutations (§4.1/§4.2)
// ---------------------------------------------------------------------------

/// Slim camelCase mirror of the issue row a mutation returns (the web DB
/// row). Everything but `id` is tolerant-optional; the sync collections stay
/// the read source of truth (§4.1) — this exists for navigate-after-create
/// and logging.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueOut {
    pub id: String,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub assignee_id: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

/// `issues.create` input (`issues.ts` create schema). Create-time nullable
/// fields treat null == absent, so plain skip-if-`None` options suffice.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesCreateInput {
    pub project_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    /// GFM markdown. The server rejects embedded images at create time
    /// ("Images can only be added after the issue is created").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// `HH:MM` (cascade-null rules live server-side, §4.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
    /// Must be set together with `recurrence_unit` (server-enforced pair).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_interval: Option<i64>,
    /// A `recurrenceUnitValues` contract value (`domain::contract`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_unit: Option<String>,
}

impl IssuesCreateInput {
    pub fn new(project_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            title: title.into(),
            status: None,
            priority: None,
            assignee_id: None,
            description: None,
            due_date: None,
            due_time: None,
            end_time: None,
            label_ids: None,
            recurrence_interval: None,
            recurrence_unit: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesCreateOutput {
    pub issue: IssueOut,
    /// The Postgres txid for the §4.1 `awaitTxId` gate (create/navigate flows).
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `issues.update` input. Plain-`Option` fields are non-nullable on the
/// server (omit = unchanged); [`Patch`] fields are zod
/// `.nullable().optional()` (omit / null-clear / set).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesUpdateInput {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub assignee_id: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub description: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub due_date: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub due_time: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub end_time: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub recurrence_interval: Patch<i64>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub recurrence_unit: Patch<String>,
    /// Mark-as-duplicate (§4.2): `Set(canonical_id)` forces
    /// `status='duplicate'` server-side; `Null` unmarks (restores backlog).
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub duplicate_of_id: Patch<String>,
    /// ISO datetime; `Null` un-archives.
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub archived_at: Patch<String>,
}

impl IssuesUpdateInput {
    /// All fields "leave unchanged"; set only what you mutate.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: None,
            status: None,
            priority: None,
            assignee_id: Patch::Omit,
            description: Patch::Omit,
            due_date: Patch::Omit,
            due_time: Patch::Omit,
            end_time: Patch::Omit,
            recurrence_interval: Patch::Omit,
            recurrence_unit: Patch::Omit,
            duplicate_of_id: Patch::Omit,
            archived_at: Patch::Omit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesUpdateOutput {
    pub issue: IssueOut,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesDeleteOutput {
    pub id: String,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `issues.move` output (EXP-57): the renumbered issue (fresh identifier +
/// project_id) plus the target project's slug — the web navigates with it;
/// the desktop's issue tabs key on the stable UUID, so it's informational.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesMoveOutput {
    pub issue: IssueOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub project_slug: Option<String>,
}

/// `issues.create` — mutation. Blocking; background executor only (§3.5).
pub fn issues_create(
    trpc: &TrpcClient,
    input: &IssuesCreateInput,
) -> Result<IssuesCreateOutput, ApiError> {
    trpc.mutation("issues.create", input)
}

/// `issues.update` — mutation. Blocking; background executor only (§3.5).
pub fn issues_update(
    trpc: &TrpcClient,
    input: &IssuesUpdateInput,
) -> Result<IssuesUpdateOutput, ApiError> {
    trpc.mutation("issues.update", input)
}

/// `issues.delete` — mutation. Blocking; background executor only (§3.5).
pub fn issues_delete(trpc: &TrpcClient, id: &str) -> Result<IssuesDeleteOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("issues.delete", &Input { id })
}

/// `issues.move` — same-workspace project move (EXP-57). The server
/// renumbers the issue in the target project (EXP-42 → ABC-17), re-points
/// the denormalized child rows and records a `project_moved` timeline event;
/// moving to the current project or across workspaces is a BAD_REQUEST.
/// Blocking; background executor only (§3.5).
pub fn issues_move(
    trpc: &TrpcClient,
    id: &str,
    project_id: &str,
) -> Result<IssuesMoveOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        id: &'a str,
        project_id: &'a str,
    }
    trpc.mutation("issues.move", &Input { id, project_id })
}

// ---------------------------------------------------------------------------
// Bulk mutations (the multi-select action bar)
// ---------------------------------------------------------------------------

/// `issues.bulkUpdate` input: 1..=200 ids (one workspace per batch,
/// server-enforced) plus the PROPERTY fields only — status, priority and the
/// tri-state assignee. Label bulk writes deliberately live on
/// `issueLabels.bulkAdd/bulkRemove` (`crate::labels`), never here.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesBulkUpdateInput {
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    /// zod `.nullable().optional()` — `Null` unassigns every issue.
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub assignee_id: Patch<String>,
}

impl IssuesBulkUpdateInput {
    /// All fields "leave unchanged"; set exactly the one you bulk-edit.
    pub fn new(ids: Vec<String>) -> Self {
        Self {
            ids,
            status: None,
            priority: None,
            assignee_id: Patch::Omit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesBulkUpdateOutput {
    /// Survivor count (stale ids / trashed projects are silently skipped).
    #[serde(default)]
    pub updated: Option<i64>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesBulkDeleteOutput {
    #[serde(default)]
    pub deleted: Option<i64>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `issues.bulkUpdate` — mutation. One transaction / one txId server-side;
/// callers chunk at 200 ids and call sequentially. Blocking; background
/// executor only (§3.5).
pub fn issues_bulk_update(
    trpc: &TrpcClient,
    input: &IssuesBulkUpdateInput,
) -> Result<IssuesBulkUpdateOutput, ApiError> {
    trpc.mutation("issues.bulkUpdate", input)
}

/// `issues.bulkDelete` — mutation (same 200-id chunk contract). Blocking;
/// background executor only (§3.5).
pub fn issues_bulk_delete(
    trpc: &TrpcClient,
    ids: &[String],
) -> Result<IssuesBulkDeleteOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        ids: &'a [String],
    }
    trpc.mutation("issues.bulkDelete", &Input { ids })
}

// ---------------------------------------------------------------------------
// issues.mergePr
// ---------------------------------------------------------------------------

/// Output of `issues.mergePr` — `{"merged": true}` on success. The server is
/// idempotent: an already-merged PR also returns `merged: true` (no error).
#[derive(Debug, Deserialize)]
pub struct MergeResult {
    pub merged: bool,
}

/// `issues.mergePr` — squash-merges the issue's open PR server-side through
/// the GitHub App installation token (the desktop never touches `gh`/git for
/// this). Guard failures (no linked PR, PR not open, no repo, App not
/// installed, GitHub-side rejection) surface as [`ApiError::Http`] with the
/// server's user-facing message. Blocking; background executor only (§3.5).
pub fn merge_pr(trpc: &TrpcClient, issue_id: &str) -> Result<MergeResult, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        issue_id: &'a str,
    }
    trpc.mutation("issues.mergePr", &Input { issue_id })
}

// ---------------------------------------------------------------------------
// issues.search (EXP-3)
// ---------------------------------------------------------------------------

/// One relevance-ordered hit of `issues.search` — the server full-text pass
/// that also matches description + comment bodies (the desktop layers it
/// under the instant local title/identifier filter). Enums decode through
/// the §5.5 tolerant-unknown rule so a new status value never drops a hit.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueSearchHit {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub project_id: String,
    pub status: IssueStatus,
    pub priority: IssuePriority,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchInput<'a> {
    workspace_id: &'a str,
    query: &'a str,
    limit: u32,
}

/// `issues.search` — workspace-scoped server full-text search (title,
/// identifier, description, comment bodies; relevance-ordered). `limit` is
/// server-clamped to 50. Blocking; background executor only (§3.5).
pub fn search(
    client: &TrpcClient,
    workspace_id: &str,
    query: &str,
    limit: u32,
) -> Result<Vec<IssueSearchHit>, ApiError> {
    client.query_with_input(
        "issues.search",
        &SearchInput {
            workspace_id,
            query,
            limit,
        },
    )
}

// ---------------------------------------------------------------------------
// issues.prFiles (§7.8)
// ---------------------------------------------------------------------------

/// One changed file of the issue's PR (GitHub "list pull request files" row,
/// as forwarded by `issues.prFiles`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullFile {
    pub filename: String,
    /// GitHub file status: `added` / `modified` / `renamed` / `removed`
    /// (plus rare `copied` / `changed` / `unchanged`). Kept as a string —
    /// mirroring the web type — so unknown values never fail the decode.
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    /// Unified patch body (hunks only, starting at `@@`). Absent for binary
    /// or too-large files — the web renders "No textual diff" for those.
    #[serde(default)]
    pub patch: Option<String>,
    /// Blob SHA (§7.8 forward-compat; server-side addition pending).
    #[serde(default)]
    pub sha: Option<String>,
    /// Pre-rename path (§7.8 forward-compat; accepts both the camelCase web
    /// form and GitHub's raw snake_case, whichever the server ends up
    /// forwarding).
    #[serde(default, alias = "previous_filename")]
    pub previous_filename: Option<String>,
}

/// Output of `issues.prFiles`. `repo` is `owner/name`; both `repo` and
/// `pr_number` are `None` when the issue has no linked PR.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrFiles {
    pub repo: Option<String>,
    pub pr_number: Option<i64>,
    pub files: Vec<PullFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrFilesInput<'a> {
    issue_id: &'a str,
}

/// Fetch the changed files of the issue's PR (`issues.prFiles`). Blocking —
/// call from a background executor, never the foreground (§3.5).
pub fn pr_files(client: &TrpcClient, issue_id: &str) -> Result<PrFiles, ApiError> {
    client.query_with_input("issues.prFiles", &PrFilesInput { issue_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok-1".to_string())))
    }

    #[test]
    fn decodes_pr_files_and_sends_camel_case_input() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repo":"acme/widgets","prNumber":42,"files":[
                {"filename":"src/a.ts","status":"modified","additions":3,"deletions":1,"patch":"@@ -1 +1,3 @@\n-a\n+b\n+c\n+d"},
                {"filename":"img/logo.png","status":"added","additions":0,"deletions":0}
            ]}}}"#,
        );
        let out = pr_files(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000").unwrap();
        assert_eq!(out.repo.as_deref(), Some("acme/widgets"));
        assert_eq!(out.pr_number, Some(42));
        assert_eq!(out.files.len(), 2);
        assert_eq!(out.files[0].filename, "src/a.ts");
        assert!(out.files[0].patch.as_deref().unwrap().starts_with("@@ -1"));
        // Binary file: no patch field → None, never a decode error.
        assert_eq!(out.files[1].patch, None);
        assert_eq!(out.files[1].status, "added");

        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // GET query, camelCase issueId in the percent-encoded raw-JSON input.
        assert!(request.starts_with("GET /api/trpc/issues.prFiles?input="));
        assert!(request.contains("%22issueId%22"));
        assert!(request.contains("Authorization: Bearer tok-1"));
    }

    #[test]
    fn decodes_no_pr_as_nulls_and_empty_files() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repo":null,"prNumber":null,"files":[]}}}"#,
        );
        let out = pr_files(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000").unwrap();
        assert_eq!(out.repo, None);
        assert_eq!(out.pr_number, None);
        assert!(out.files.is_empty());
    }

    #[test]
    fn decodes_forward_compat_sha_and_previous_filename_both_casings() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repo":"a/b","prNumber":1,"files":[
                {"filename":"new.rs","status":"renamed","additions":0,"deletions":0,"sha":"abc123","previousFilename":"old.rs"},
                {"filename":"new2.rs","status":"renamed","additions":0,"deletions":0,"previous_filename":"old2.rs"}
            ]}}}"#,
        );
        let out = pr_files(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000").unwrap();
        assert_eq!(out.files[0].sha.as_deref(), Some("abc123"));
        assert_eq!(out.files[0].previous_filename.as_deref(), Some("old.rs"));
        assert_eq!(out.files[1].previous_filename.as_deref(), Some("old2.rs"));
    }

    #[test]
    fn search_sends_workspace_scoped_input_and_decodes_hits() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":[
                {"id":"i-1","identifier":"EXP-1","title":"Fix search","projectId":"p-1","status":"todo","priority":"high"},
                {"id":"i-2","identifier":"EXP-2","title":"Later","projectId":"p-1","status":"brand_new_state","priority":"none"}
            ]}}"#,
        );
        let out = search(&client(&base), "ws-1", "descr text", 20).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].identifier, "EXP-1");
        assert_eq!(out[0].status, IssueStatus::Todo);
        // Tolerant-unknown (§5.5): a new server enum value never drops the hit.
        assert_eq!(out[1].status, IssueStatus::Unknown);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/issues.search?input="));
        assert!(request.contains("%22workspaceId%22"));
        assert!(request.contains("%22limit%22%3A20"));
        assert!(request.contains("Authorization: Bearer tok-1"));
    }

    #[test]
    fn update_serializes_only_touched_fields() {
        // The inline status dropdown's exact wire body (§4.6): id + status,
        // nothing else — an accidental `"assigneeId":null` would CLEAR the
        // assignee on every status change.
        let mut input = IssuesUpdateInput::new("11111111-1111-1111-1111-111111111111");
        input.status = Some(IssueStatus::InProgress);
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(
            json,
            r#"{"id":"11111111-1111-1111-1111-111111111111","status":"in_progress"}"#
        );
    }

    #[test]
    fn update_distinguishes_null_clear_from_omit() {
        let mut input = IssuesUpdateInput::new("i-1");
        input.due_date = Patch::Null;
        input.assignee_id = Patch::Set("u-1".to_string());
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(json, r#"{"id":"i-1","assigneeId":"u-1","dueDate":null}"#);
    }

    #[test]
    fn update_posts_and_decodes_issue_envelope() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"issue":{"id":"i-1","identifier":"EXP-1","status":"done","completedAt":"2026-07-03T00:00:00Z"}}}}"#,
        );
        let mut input = IssuesUpdateInput::new("i-1");
        input.status = Some(IssueStatus::Done);
        let out = issues_update(&client(&base), &input).unwrap();
        assert_eq!(out.issue.id, "i-1");
        assert_eq!(out.issue.status.as_deref(), Some("done"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.update HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"i-1","status":"done"}"#));
    }

    #[test]
    fn create_decodes_tx_id_for_the_gate() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"issue":{"id":"i-9","identifier":"EXP-9","projectId":"p-1","title":"New"},"txId":4242}}}"#,
        );
        let mut input = IssuesCreateInput::new("p-1", "New");
        input.priority = Some(IssuePriority::High);
        let out = issues_create(&client(&base), &input).unwrap();
        assert_eq!(out.issue.identifier.as_deref(), Some("EXP-9"));
        assert_eq!(out.issue.project_id.as_deref(), Some("p-1"));
        assert_eq!(out.tx_id, Some(4242));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.create HTTP/1.1"));
        assert!(request.ends_with(r#"{"projectId":"p-1","title":"New","priority":"high"}"#));
    }

    #[test]
    fn bulk_update_serializes_only_touched_fields() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":21,"updated":2}}}"#);
        let mut input = IssuesBulkUpdateInput::new(vec!["i-1".to_string(), "i-2".to_string()]);
        input.status = Some(IssueStatus::Done);
        let out = issues_bulk_update(&client(&base), &input).unwrap();
        assert_eq!(out.updated, Some(2));
        assert_eq!(out.tx_id, Some(21));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.bulkUpdate HTTP/1.1"));
        // Status only — an accidental `"assigneeId":null` would bulk-unassign.
        assert!(request.ends_with(r#"{"ids":["i-1","i-2"],"status":"done"}"#));
    }

    #[test]
    fn bulk_update_distinguishes_unassign_null_from_omit() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":22,"updated":1}}}"#);
        let mut input = IssuesBulkUpdateInput::new(vec!["i-1".to_string()]);
        input.assignee_id = Patch::Null;
        let _ = issues_bulk_update(&client(&base), &input).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"ids":["i-1"],"assigneeId":null}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":23,"updated":1}}}"#);
        let mut input = IssuesBulkUpdateInput::new(vec!["i-1".to_string()]);
        input.priority = Some(IssuePriority::High);
        input.assignee_id = Patch::Set("u-1".to_string());
        let _ = issues_bulk_update(&client(&base), &input).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"ids":["i-1"],"priority":"high","assigneeId":"u-1"}"#));
    }

    #[test]
    fn bulk_delete_posts_the_id_array_and_decodes_counts() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":24,"deleted":3}}}"#);
        let out = issues_bulk_delete(
            &client(&base),
            &["i-1".to_string(), "i-2".to_string(), "i-3".to_string()],
        )
        .unwrap();
        assert_eq!(out.deleted, Some(3));
        assert_eq!(out.tx_id, Some(24));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.bulkDelete HTTP/1.1"));
        assert!(request.ends_with(r#"{"ids":["i-1","i-2","i-3"]}"#));
    }

    #[test]
    fn delete_sends_id_and_decodes_tx() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":7,"id":"i-1"}}}"#);
        let out = issues_delete(&client(&base), "i-1").unwrap();
        assert_eq!(out.id, "i-1");
        assert_eq!(out.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"i-1"}"#));
    }

    #[test]
    fn move_sends_camel_case_input_and_decodes_new_identity() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":9,"issue":{"id":"i-1","identifier":"ABC-17","projectId":"p-2"},"projectSlug":"abc"}}}"#,
        );
        let out = issues_move(&client(&base), "i-1", "p-2").unwrap();
        assert_eq!(out.issue.identifier.as_deref(), Some("ABC-17"));
        assert_eq!(out.issue.project_id.as_deref(), Some("p-2"));
        assert_eq!(out.project_slug.as_deref(), Some("abc"));
        assert_eq!(out.tx_id, Some(9));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.move HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"i-1","projectId":"p-2"}"#));
    }

    #[test]
    fn merge_pr_posts_camel_case_input_and_decodes_result() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"merged":true}}}"#);
        let out = merge_pr(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000").unwrap();
        assert!(out.merged);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.mergePr HTTP/1.1"));
        assert!(request.ends_with(r#"{"issueId":"1f7f6f9e-0000-4000-8000-000000000000"}"#));
        assert!(request.contains("Authorization: Bearer tok-1"));
    }

    #[test]
    fn merge_pr_guard_failure_surfaces_the_server_message() {
        // The server maps merge guards to PRECONDITION_FAILED with a
        // user-facing message — the panel shows it verbatim.
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"This issue has no linked pull request","code":-32603,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        let result = merge_pr(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000");
        match result {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 412);
                assert!(message.contains("no linked pull request"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    #[test]
    fn github_failure_surfaces_as_http_error_with_message() {
        // Server wraps GitHub failures in BAD_GATEWAY (issues.ts prFiles).
        let (base, _captured) = one_shot_server(
            502,
            r#"{"error":{"message":"GitHub returned 404 for acme/widgets#42","code":-32603,"data":{"code":"BAD_GATEWAY","httpStatus":502}}}"#,
        );
        let result = pr_files(&client(&base), "1f7f6f9e-0000-4000-8000-000000000000");
        match result {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 502);
                assert!(message.contains("GitHub returned 404"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }
}
