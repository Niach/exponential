//! Typed `issues.*` tRPC helpers.
//!
//! Two surfaces share this router mirror:
//!
//! 1. **Mutations** (masterplan-v3 §4.1/§4.2) — `issues.create` /
//!    `issues.update` / `issues.delete` / `issues.move`, verified against
//!    `apps/web/src/lib/trpc/issues.ts`: create returns `{issue, txId}`,
//!    update returns `{issue, txId}` (EXP-707; the no-op path omits txId and
//!    the Electric echo re-renders either way), delete returns `{txId, id}`,
//!    move returns `{txId, issue, boardSlug}` (EXP-57).
//!    Update inputs use [`Patch`] for the zod `.nullable().optional()` fields
//!    (omit = unchanged, null = clear, value = set). Never pass
//!    `IssueStatus::Unknown` / `IssuePriority::Unknown` — they serialize as
//!    `"unknown"` and the server's schema rejects them.
//!
//! 2. **[`PrFiles`]** — the changed-files shape `repositories.branchDiff`
//!    answers with (the desktop diff view). EXP-1291: the PR procedures left
//!    this router (`pullRequests.*`, [`crate::pull_requests`]).
//!
//! `PullFile` forward-compat (§7.8): `sha` and `previousFilename` decode as
//! `Option` + `#[serde(default)]`; the server does not send them yet.

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
    pub board_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub assignee_id: Option<String>,
    #[serde(default)]
    pub assignee_agent_id: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

/// `issues.create` input (`issues.ts` create schema). Create-time nullable
/// fields treat null == absent, so plain skip-if-`None` options suffice.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesCreateInput {
    pub board_id: String,
    pub title: String,
    /// EXP-314: the enum ANCHOR. Mutually exclusive with [`Self::status_id`]
    /// server-side — set one or the other, never both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    /// EXP-314: the precise `issue_statuses` row. Pickers send this; the
    /// enum above stays for the pre-sync constructed fallbacks and the
    /// enum-only convenience writes.
    #[serde(rename = "statusId", skip_serializing_if = "Option::is_none")]
    pub status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    /// #EXP-1383: a team agent as the assignee (exclusive with
    /// [`Self::assignee_id`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_agent_id: Option<String>,
    /// GFM markdown. The server rejects embedded images at create time
    /// ("Images can only be added after the issue is created").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `YYYY-MM-DD` — date only; REV2-49 deleted the time-of-day fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
    /// EXP-760: file the issue as a sub-issue of this parent in the same
    /// server transaction (the inline sub-issue composer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// EXP-878: the `issue_drafts` row this create came from. The server
    /// reparents the draft's attachments onto the new issue and deletes the
    /// draft in the SAME transaction — which is why a create carrying one
    /// uploads nothing: every image in `description` is already a
    /// `/api/attachments/{id}` URL owned by that draft.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    /// EXP-1284 → EXP-1291: "New issue in this thread" — the server links
    /// the new issue into that session's thread root (source `user`) in the
    /// create's transaction. Skipped when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl IssuesCreateInput {
    pub fn new(board_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            board_id: board_id.into(),
            title: title.into(),
            status: None,
            status_id: None,
            priority: None,
            assignee_id: None,
            assignee_agent_id: None,
            description: None,
            due_date: None,
            label_ids: None,
            parent_id: None,
            draft_id: None,
            session_id: None,
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
    /// EXP-314: the enum ANCHOR (mutually exclusive with
    /// [`Self::status_id`], and with a non-null `duplicate_of_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    /// EXP-314: the precise `issue_statuses` row.
    #[serde(rename = "statusId", skip_serializing_if = "Option::is_none")]
    pub status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub assignee_id: Patch<String>,
    /// #EXP-1383: the agent column of the assignee pair; set it with
    /// [`Self::assign`], which writes BOTH columns (the cleared one null).
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub assignee_agent_id: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub description: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub due_date: Patch<String>,
    /// Mark-as-duplicate (§4.2): `Set(canonical_id)` forces
    /// `status='duplicate'` server-side; `Null` unmarks (restores backlog).
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub duplicate_of_id: Patch<String>,
    /// EXP-630: story points (always a point number, the team's scale only
    /// decides how it reads); `Null` clears.
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub estimate: Patch<i64>,
}

impl IssuesUpdateInput {
    /// All fields "leave unchanged"; set only what you mutate.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: None,
            status: None,
            status_id: None,
            priority: None,
            assignee_id: Patch::Omit,
            assignee_agent_id: Patch::Omit,
            description: Patch::Omit,
            due_date: Patch::Omit,
            duplicate_of_id: Patch::Omit,
            estimate: Patch::Omit,
        }
    }
}

impl IssuesUpdateInput {
    /// #EXP-1383: the two-column assignee write: an agent wins and clears
    /// the person, a person (or nobody) clears the agent, both explicit.
    pub fn assign(mut self, assignee: &Assignee) -> Self {
        let (user, agent) = assignee.patches();
        self.assignee_id = user;
        self.assignee_agent_id = agent;
        self
    }
}

/// #EXP-1383 (agent-settings.json `assignee`): an issue's assignee is ONE of
/// two mutually exclusive columns, a member or a team agent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Assignee {
    #[default]
    Nobody,
    User(String),
    Agent(String),
}

impl Assignee {
    /// A row's pair (an agent wins, like the DB check allows only one).
    pub fn of(assignee_id: Option<&str>, assignee_agent_id: Option<&str>) -> Self {
        if let Some(agent) = assignee_agent_id.filter(|id| !id.is_empty()) {
            return Self::Agent(agent.to_string());
        }
        match assignee_id.filter(|id| !id.is_empty()) {
            Some(user) => Self::User(user.to_string()),
            None => Self::Nobody,
        }
    }

    /// `(assigneeId, assigneeAgentId)` as an explicit write.
    pub fn patches(&self) -> (Patch<String>, Patch<String>) {
        match self {
            Self::Nobody => (Patch::Null, Patch::Null),
            Self::User(id) => (Patch::Set(id.clone()), Patch::Null),
            Self::Agent(id) => (Patch::Null, Patch::Set(id.clone())),
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::User(id) => Some(id),
            _ => None,
        }
    }

    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::Agent(id) => Some(id),
            _ => None,
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
/// board_id) plus the target board's slug — the web navigates with it;
/// the desktop's issue tabs key on the stable UUID, so it's informational.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesMoveOutput {
    pub issue: IssueOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub board_slug: Option<String>,
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

/// `issues.move` — same-team board move (EXP-57). The server
/// renumbers the issue in the target board (EXP-42 → ABC-17), re-points
/// the denormalized child rows and records a `board_moved` timeline event;
/// moving to the current board or across teams is a BAD_REQUEST.
/// Blocking; background executor only (§3.5).
pub fn issues_move(
    trpc: &TrpcClient,
    id: &str,
    board_id: &str,
) -> Result<IssuesMoveOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        id: &'a str,
        board_id: &'a str,
    }
    trpc.mutation("issues.move", &Input { id, board_id })
}

// ---------------------------------------------------------------------------
// Bulk mutations (the multi-select action bar)
// ---------------------------------------------------------------------------

/// `issues.bulkUpdate` input: 1..=200 ids (one team per batch,
/// server-enforced) plus the PROPERTY fields only — status, priority and the
/// tri-state assignee. Label bulk writes deliberately live on
/// `issueLabels.bulkAdd/bulkRemove` (`crate::labels`), never here.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesBulkUpdateInput {
    /// EXP-707: the wire name is `issueIds` (renamed from `ids`).
    pub issue_ids: Vec<String>,
    /// EXP-314: the enum ANCHOR (mutually exclusive with
    /// [`Self::status_id`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    /// EXP-314: the precise `issue_statuses` row.
    #[serde(rename = "statusId", skip_serializing_if = "Option::is_none")]
    pub status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    /// zod `.nullable().optional()` — `Null` unassigns every issue.
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub assignee_id: Patch<String>,
    /// #EXP-1383: the agent column; set both through [`Self::assign`].
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub assignee_agent_id: Patch<String>,
}

impl IssuesBulkUpdateInput {
    /// All fields "leave unchanged"; set exactly the one you bulk-edit.
    pub fn new(ids: Vec<String>) -> Self {
        Self {
            issue_ids: ids,
            status: None,
            status_id: None,
            priority: None,
            assignee_id: Patch::Omit,
            assignee_agent_id: Patch::Omit,
        }
    }

    /// #EXP-1383: the two-column assignee write (see
    /// [`IssuesUpdateInput::assign`]).
    pub fn assign(mut self, assignee: &Assignee) -> Self {
        let (user, agent) = assignee.patches();
        self.assignee_id = user;
        self.assignee_agent_id = agent;
        self
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesBulkUpdateOutput {
    /// Survivor count (stale ids / trashed boards are silently skipped).
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
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        issue_ids: &'a [String],
    }
    trpc.mutation("issues.bulkDelete", &Input { issue_ids: ids })
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
    pub board_id: String,
    /// The enum ANCHOR — search is CROSS-BOARD but single-team, so a hit can
    /// still be resolved against the team's status rows via `status_id`.
    pub status: IssueStatus,
    /// EXP-314 `issues.search` projection: the precise status row, `None`
    /// when the row has none.
    #[serde(default)]
    pub status_id: Option<String>,
    pub priority: IssuePriority,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchInput<'a> {
    team_id: &'a str,
    query: &'a str,
    limit: u32,
}

/// `issues.search` — team-scoped server full-text search (title,
/// identifier, description, comment bodies; relevance-ordered). `limit` is
/// server-clamped to 50. Blocking; background executor only (§3.5).
pub fn search(
    client: &TrpcClient,
    team_id: &str,
    query: &str,
    limit: u32,
) -> Result<Vec<IssueSearchHit>, ApiError> {
    client.query_with_input(
        "issues.search",
        &SearchInput {
            team_id,
            query,
            limit,
        },
    )
}

// ---------------------------------------------------------------------------
// issues.get (EXP-403 — the CLI's identifier→row resolver)
// ---------------------------------------------------------------------------

/// The `issues.get` row (the synced issues-shape allowlist, camelCase over
/// tRPC). Only the fields the CLI launcher consumes are typed; everything
/// else is tolerant so server-side additions never break the decode.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedIssue {
    pub id: String,
    /// `EXP-42`.
    pub identifier: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The board the issue lives on — EXP-712: the headless launcher needs it
    /// to resolve the board's branch (the sync store the desktop reads is not
    /// available here).
    #[serde(default)]
    pub board_id: Option<String>,
    /// The dual-written ANCHOR enum (`backlog`/`in_progress`/…).
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub pr_url: Option<String>,
    #[serde(default)]
    pub pr_state: Option<String>,
}

/// `issues.get` output: the row plus its team (top-level, not a row field).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueGetResult {
    pub issue: FetchedIssue,
    pub team_id: String,
}

#[derive(Serialize)]
struct IssueGetInput<'a> {
    id: &'a str,
}

/// `issues.get` — query. Accepts a UUID or a human identifier (`EXP-42`,
/// case-insensitive server-side — the same semantics as the MCP layer's
/// resolveIssueId). NOT_FOUND (404) for unknown/trashed issues.
pub fn issues_get(client: &TrpcClient, id: &str) -> Result<IssueGetResult, ApiError> {
    client.query_with_input("issues.get", &IssueGetInput { id })
}

// ---------------------------------------------------------------------------
// The changed-files shape (§7.8; `repositories.branchDiff`)
// ---------------------------------------------------------------------------

/// One changed file (GitHub "list pull request files" row, as forwarded by
/// `repositories.branchDiff`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullFile {
    pub filename: String,
    /// GitHub file status: `added` / `modified` / `renamed` / `removed`
    /// (plus rare `copied` / `changed` / `unchanged`). Kept as a string —
    /// mirroring the web type — so unknown values never fail the decode.
    pub status: String,
    /// EXP-912: SIGNED, and wide, because the wire is a JSON number nobody on
    /// this side validates — the shared diff fixture
    /// (`packages/domain-contract/fixtures/diff/cases.json`) locks
    /// `additions: -3` clamping to `0`, and `domain::diff::from_pull_file`
    /// takes `i64` for exactly that. A `u32` here rejected the whole
    /// `PrFilesResponse` on the desktop over one number every other client
    /// clamps.
    pub additions: i64,
    pub deletions: i64,
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

/// The changed-files answer. `repo` is `owner/name`; both `repo` and
/// `pr_number` are `None` when there is no PR.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrFiles {
    pub repo: Option<String>,
    pub pr_number: Option<i64>,
    pub files: Vec<PullFile>,
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

    fn files(json: &str) -> PrFiles {
        serde_json::from_str(json).expect("the changed-files shape decodes")
    }

    #[test]
    fn decodes_pr_files() {
        let out = files(
            r#"{"repo":"acme/widgets","prNumber":42,"files":[
                {"filename":"src/a.ts","status":"modified","additions":3,"deletions":1,"patch":"@@ -1 +1,3 @@\n-a\n+b\n+c\n+d"},
                {"filename":"img/logo.png","status":"added","additions":0,"deletions":0}
            ]}"#,
        );
        assert_eq!(out.repo.as_deref(), Some("acme/widgets"));
        assert_eq!(out.pr_number, Some(42));
        assert_eq!(out.files.len(), 2);
        assert!(out.files[0].patch.as_deref().unwrap().starts_with("@@ -1"));
        // Binary file: no patch field → None, never a decode error.
        assert_eq!(out.files[1].patch, None);
        assert_eq!(out.files[1].status, "added");
    }

    /// EXP-912: a NEGATIVE count decodes. The shared diff fixture locks
    /// `additions: -3 → 0` (every client clamps the one number), so the
    /// desktop must not reject the whole response over it — the clamp lives in
    /// `domain::diff::from_pull_file`, which takes `i64`.
    #[test]
    fn decodes_a_negative_count_instead_of_failing_the_whole_response() {
        let out = files(
            r#"{"repo":"a/b","prNumber":7,"files":[
                {"filename":"src/a.ts","status":"modified","additions":-3,"deletions":2},
                {"filename":"src/b.ts","status":"modified","additions":4294967296,"deletions":0}
            ]}"#,
        );
        assert_eq!((out.files[0].additions, out.files[0].deletions), (-3, 2));
        assert_eq!(out.files[1].additions, 4_294_967_296, "wider than a u32, too");
    }

    #[test]
    fn decodes_no_pr_as_nulls_and_empty_files() {
        let out = files(r#"{"repo":null,"prNumber":null,"files":[]}"#);
        assert_eq!(out.repo, None);
        assert_eq!(out.pr_number, None);
        assert!(out.files.is_empty());
    }

    #[test]
    fn decodes_forward_compat_sha_and_previous_filename_both_casings() {
        let out = files(
            r#"{"repo":"a/b","prNumber":1,"files":[
                {"filename":"new.rs","status":"renamed","additions":0,"deletions":0,"sha":"abc123","previousFilename":"old.rs"},
                {"filename":"new2.rs","status":"renamed","additions":0,"deletions":0,"previous_filename":"old2.rs"}
            ]}"#,
        );
        assert_eq!(out.files[0].sha.as_deref(), Some("abc123"));
        assert_eq!(out.files[0].previous_filename.as_deref(), Some("old.rs"));
        assert_eq!(out.files[1].previous_filename.as_deref(), Some("old2.rs"));
    }

    #[test]
    fn search_sends_team_scoped_input_and_decodes_hits() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":[
                {"id":"i-1","identifier":"EXP-1","title":"Fix search","boardId":"p-1","status":"in_progress","priority":"high"},
                {"id":"i-2","identifier":"EXP-2","title":"Later","boardId":"p-1","status":"brand_new_state","priority":"none"}
            ]}}"#,
        );
        let out = search(&client(&base), "ws-1", "descr text", 20).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].identifier, "EXP-1");
        assert_eq!(out[0].status, IssueStatus::InProgress);
        // EXP-314: `statusId` is nullable on the wire — a row without one
        // decodes to None and the hit still renders via its anchor.
        assert_eq!(out[0].status_id, None);
        // Tolerant-unknown (§5.5): a new server enum value never drops the hit.
        assert_eq!(out[1].status, IssueStatus::Unknown);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/issues.search?input="));
        assert!(request.contains("%22teamId%22"));
        assert!(request.contains("%22limit%22%3A20"));
        assert!(crate::trpc::tests::has_header(&request, "Authorization: Bearer tok-1"));
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
    fn status_writes_send_either_the_row_id_or_the_anchor() {
        // EXP-314: a picker pick on a SYNCED row sends `statusId` alone —
        // sending both is a server-side BAD_REQUEST (mutually exclusive).
        let mut input = IssuesUpdateInput::new("i-1");
        input.status_id = Some("s-1".to_string());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"id":"i-1","statusId":"s-1"}"#
        );

        // A pick on a CONSTRUCTED `builtin:<key>` fallback (statuses shape not
        // synced) degrades to the enum anchor.
        let mut input = IssuesUpdateInput::new("i-1");
        input.status = Some(IssueStatus::Done);
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"id":"i-1","status":"done"}"#
        );

        // Same on create + bulk.
        let mut input = IssuesCreateInput::new("b-1", "Title");
        input.status_id = Some("s-2".to_string());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"boardId":"b-1","title":"Title","statusId":"s-2"}"#
        );
        let mut input = IssuesBulkUpdateInput::new(vec!["i-1".to_string()]);
        input.status_id = Some("s-3".to_string());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"issueIds":["i-1"],"statusId":"s-3"}"#
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

    /// EXP-630: the estimate rides `issues.update` as a plain number, and
    /// `null` clears it — never `0`, never a string.
    #[test]
    fn update_sets_and_clears_the_estimate() {
        let mut input = IssuesUpdateInput::new("i-1");
        input.estimate = Patch::Set(5);
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"id":"i-1","estimate":5}"#
        );
        let mut input = IssuesUpdateInput::new("i-1");
        input.estimate = Patch::Null;
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"id":"i-1","estimate":null}"#
        );
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
            r#"{"result":{"data":{"issue":{"id":"i-9","identifier":"EXP-9","boardId":"p-1","title":"New"},"txId":4242}}}"#,
        );
        let mut input = IssuesCreateInput::new("p-1", "New");
        input.priority = Some(IssuePriority::High);
        let out = issues_create(&client(&base), &input).unwrap();
        assert_eq!(out.issue.identifier.as_deref(), Some("EXP-9"));
        assert_eq!(out.issue.board_id.as_deref(), Some("p-1"));
        assert_eq!(out.tx_id, Some(4242));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.create HTTP/1.1"));
        assert!(request.ends_with(r#"{"boardId":"p-1","title":"New","priority":"high"}"#));
    }

    #[test]
    fn create_files_a_sub_issue_with_parent_id() {
        // EXP-760: the inline sub-issue composer files the child in ONE
        // server transaction — `parentId` rides the create, and stays off the
        // wire for every ordinary create.
        let mut input = IssuesCreateInput::new("p-1", "Child");
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"boardId":"p-1","title":"Child"}"#
        );
        input.parent_id = Some("i-parent".to_string());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"boardId":"p-1","title":"Child","parentId":"i-parent"}"#
        );
    }

    #[test]
    fn create_files_from_a_draft_with_draft_id() {
        // EXP-878/EXP-1170: the New issue page hands the server the draft
        // row it was composing in, so the draft's already
        // uploaded attachments are reparented and the row deleted in the
        // same transaction. It stays off the wire for every other create.
        let mut input = IssuesCreateInput::new("p-1", "From draft");
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"boardId":"p-1","title":"From draft"}"#
        );
        input.draft_id = Some("d-1".to_string());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"boardId":"p-1","title":"From draft","draftId":"d-1"}"#
        );
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
        assert!(request.ends_with(r#"{"issueIds":["i-1","i-2"],"status":"done"}"#));
    }

    #[test]
    fn bulk_update_distinguishes_unassign_null_from_omit() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":22,"updated":1}}}"#);
        let mut input = IssuesBulkUpdateInput::new(vec!["i-1".to_string()]);
        input.assignee_id = Patch::Null;
        let _ = issues_bulk_update(&client(&base), &input).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueIds":["i-1"],"assigneeId":null}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":23,"updated":1}}}"#);
        let mut input = IssuesBulkUpdateInput::new(vec!["i-1".to_string()]);
        input.priority = Some(IssuePriority::High);
        input.assignee_id = Patch::Set("u-1".to_string());
        let _ = issues_bulk_update(&client(&base), &input).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueIds":["i-1"],"priority":"high","assigneeId":"u-1"}"#));
    }

    /// #EXP-1383: every assignee write sends BOTH columns, the cleared one
    /// as an explicit null.
    #[test]
    fn assignee_writes_send_both_columns() {
        let agent = IssuesUpdateInput::new("i-1").assign(&Assignee::Agent("g-1".into()));
        assert_eq!(
            serde_json::to_string(&agent).unwrap(),
            r#"{"id":"i-1","assigneeId":null,"assigneeAgentId":"g-1"}"#
        );
        let user = IssuesUpdateInput::new("i-1").assign(&Assignee::User("u-1".into()));
        assert_eq!(
            serde_json::to_string(&user).unwrap(),
            r#"{"id":"i-1","assigneeId":"u-1","assigneeAgentId":null}"#
        );
        let nobody = IssuesBulkUpdateInput::new(vec!["i-1".into()]).assign(&Assignee::Nobody);
        assert_eq!(
            serde_json::to_string(&nobody).unwrap(),
            r#"{"issueIds":["i-1"],"assigneeId":null,"assigneeAgentId":null}"#
        );
        assert_eq!(Assignee::of(Some("u-1"), Some("g-1")), Assignee::Agent("g-1".into()));
        assert_eq!(Assignee::of(Some("u-1"), None), Assignee::User("u-1".into()));
        assert_eq!(Assignee::of(None, Some("")), Assignee::Nobody);
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
        assert!(request.ends_with(r#"{"issueIds":["i-1","i-2","i-3"]}"#));
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
            r#"{"result":{"data":{"txId":9,"issue":{"id":"i-1","identifier":"ABC-17","boardId":"p-2"},"boardSlug":"abc"}}}"#,
        );
        let out = issues_move(&client(&base), "i-1", "p-2").unwrap();
        assert_eq!(out.issue.identifier.as_deref(), Some("ABC-17"));
        assert_eq!(out.issue.board_id.as_deref(), Some("p-2"));
        assert_eq!(out.board_slug.as_deref(), Some("abc"));
        assert_eq!(out.tx_id, Some(9));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issues.move HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"i-1","boardId":"p-2"}"#));
    }

}
