//! Typed row structs for the 25 synced shapes (masterplan-v3 §5.1/§5.5) —
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
//! server-side).
//!
//! EXP-1294 (Threads v2): the session tree is [`SessionRow`] (+
//! [`SessionIssueRow`], [`MessageRow`]) and every pull request is a
//! [`PullRequestRow`] (+ [`PullRequestIssueRow`]); [`Issue`] carries no PR or
//! branch column any more.

use serde::Deserialize;

use crate::enums::{IssuePriority, IssueStatus};
pub use crate::sealed::SealState;
use crate::hydrate::{
    tolerant_bool_or_false, tolerant_i64, tolerant_id_list, tolerant_opt_bool, tolerant_opt_f64,
    tolerant_opt_i64, tolerant_opt_json,
};

/// `teams` shape row. Teams are always private — the shape carries
/// no `is_public`/`public_write_policy`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    /// EXP-319 PR-open automation target: `Some(id)` pins an
    /// `issue_statuses` row; `None` = the builtin In Review default.
    #[serde(default)]
    pub pr_opened_status_id: Option<String>,
    /// `Some(false)` = "do nothing" on PR open. `None` (pre-column rows) =
    /// enabled, matching the server DEFAULT true.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub pr_opened_automation: Option<bool>,
    /// EXP-319 PR-merge automation target: `Some(id)` pins an
    /// `issue_statuses` row; `None` = the builtin Done default.
    #[serde(default)]
    pub pr_merged_status_id: Option<String>,
    /// `Some(false)` = "do nothing" on PR merge. `None` = enabled.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub pr_merged_automation: Option<bool>,
    /// EXP-711: does a merged PR end the live coding sessions on its
    /// issues? `Some(false)` keeps them running. `None` (pre-column rows) =
    /// enabled, matching the server DEFAULT true.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub end_sessions_on_merge: Option<bool>,
    /// EXP-1105 yolo mode: `Some(true)` hides Files and Source
    /// Control for every member (failures still surface) and auto-merges
    /// every PR an agent opens. `None` (pre-column rows) = off, matching the
    /// server DEFAULT false.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub yolo_mode: Option<bool>,
    /// EXP-630: the estimate scale (contract `issueEstimation`): `none` (or
    /// a pre-column `None`) = estimates off; the others pick the ladder and
    /// how a value reads (t-shirt = XS…XL over the fibonacci points).
    #[serde(default)]
    pub estimation_type: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Team {
    /// The estimate scale, `"none"` for pre-column rows and unknown values
    /// (a newer server's scale reads as off rather than as a stray label).
    pub fn estimation(&self) -> &str {
        match self.estimation_type.as_deref() {
            Some(value) if crate::contract::ISSUE_ESTIMATION_VALUES.contains(&value) => value,
            _ => crate::contract::ISSUE_ESTIMATION_NONE,
        }
    }

    /// Optimistic local row built from a mutation's own response (EXP-470) —
    /// identity fields only; everything else stays `None` (degrading like a
    /// pre-column row) until the Electric echo overwrites it.
    pub fn seeded(id: impl Into<String>, name: impl Into<String>, slug: Option<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            slug,
            icon_url: None,
            pr_opened_status_id: None,
            pr_opened_automation: None,
            pr_merged_status_id: None,
            pr_merged_automation: None,
            end_sessions_on_merge: None,
            yolo_mode: None,
            estimation_type: None,
            created_at: None,
            updated_at: None,
        }
    }
}

impl Team {
    /// EXP-711: whether a merged PR ends this team's live coding sessions.
    /// A row synced before the column existed reads as enabled (the server
    /// default), so only an explicit `false` keeps sessions running.
    pub fn ends_sessions_on_merge(&self) -> bool {
        self.end_sessions_on_merge != Some(false)
    }

    /// EXP-1105: whether the team runs in yolo mode. Only an explicit `true`
    /// turns it on; pre-column rows read as off (the server default).
    pub fn yolo_mode(&self) -> bool {
        self.yolo_mode == Some(true)
    }
}

/// `boards` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Board {
    pub id: String,
    pub team_id: String,
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// Curated icon name (`crate::contract::BOARD_ICON_VALUES`) chosen at
    /// create time. `None` on pre-icon boards — consumers fall back to an
    /// attribute-derived glyph.
    #[serde(default)]
    pub icon: Option<String>,
    /// `boards.repository_id` (v4 §3.1, nullable) — the one repository this
    /// board clones/branches against, or `None` for a repo-less board.
    /// Coding affordances gate purely on this presence.
    #[serde(default)]
    pub repository_id: Option<String>,
    /// `boards.default_branch` (EXP-712, nullable) — the branch THIS board's
    /// coding sessions branch from and whose PRs they target. `None` = follow
    /// the repo's own default (team pin, else GitHub), so two boards on one
    /// repository can develop on different branches.
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Board {
    /// Optimistic local row from a mutation response (EXP-470) — see
    /// [`Team::seeded`]. Callers set the optional fields they know.
    pub fn seeded(
        id: impl Into<String>,
        team_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            team_id: team_id.into(),
            name: name.into(),
            slug: None,
            prefix: None,
            color: None,
            icon: None,
            repository_id: None,
            default_branch: None,
            sort_order: None,
            created_at: None,
            updated_at: None,
        }
    }
}

/// `issues` shape row (§5.5's exemplar struct plus the full column set).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Issue {
    pub id: String,
    pub board_id: String,
    #[serde(deserialize_with = "tolerant_i64")]
    pub number: i64,
    pub identifier: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The enum ANCHOR (EXP-314): still dual-written by the server for every
    /// status change, so enum-only writers/readers stay correct. `status_id`
    /// below is the precise per-team row.
    pub status: IssueStatus,
    /// EXP-314 `issues.status_id` — the `issue_statuses` row this issue sits
    /// in. `None` on rows synced before the column existed (and on writes
    /// that only set the enum); [`crate::statuses::resolve_status`] falls
    /// back to the anchor's builtin row.
    #[serde(default)]
    pub status_id: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: IssuePriority,
    #[serde(default)]
    pub assignee_id: Option<String>,
    /// #EXP-1383 `issues.assignee_agent_id`: a team AGENT assignee,
    /// mutually exclusive with `assignee_id` (`agent-settings.json`
    /// `assignee`).
    #[serde(default)]
    pub assignee_agent_id: Option<String>,
    #[serde(default)]
    pub creator_id: Option<String>,
    /// #EXP-1383 `issues.creator_agent_id`: the agent whose session filed
    /// it (`creator_id` keeps its member).
    #[serde(default)]
    pub creator_agent_id: Option<String>,
    /// `issues.source` wire value (`user`/`widget`) — tolerant raw string
    /// (`crate::contract::ISSUE_SOURCE_VALUES`); `None` on pre-column rows.
    #[serde(default)]
    pub source: Option<String>,
    /// `date` column — `"2026-05-20"`; parse at the UI edge if needed (§5.5).
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub duplicate_of_id: Option<String>,
    /// EXP-630 `issues.estimate` — story points, always a point NUMBER;
    /// the team's `estimation_type` decides how it reads
    /// ([`crate::issue_estimate`]). `None` = unset (and on rows synced
    /// before the column existed; `heal_missing_columns` ALTERs it in).
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub estimate: Option<i64>,
    /// EXP-1231 `issues.draft_id` — the issue draft this issue was created
    /// from (stamped by `issues.create({draftId})`): an open draft page that
    /// sees it replaces itself with this issue. `None` otherwise (and on rows
    /// synced before the column existed; `heal_missing_columns` ALTERs it in).
    #[serde(default)]
    pub draft_id: Option<String>,
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
    pub team_id: String,
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
    pub team_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// `issue_relations` shape row (EXP-736) — one directed edge of the relation
/// graph. Direction is canonical: `blocks` = this issue blocks the related
/// one, `parent` = this issue is the parent, `duplicate` = this issue is the
/// duplicate and the related one canonical, `related` = symmetric (the server
/// normalizes `issue_id < related_issue_id`). `kind` is `Option` like every
/// other tolerant enum column: an unknown/absent type must degrade to a
/// hidden row, never wedge hydration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IssueRelation {
    pub id: String,
    pub issue_id: String,
    pub related_issue_id: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub board_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `pins` shape row (EXP-778) — one personal pin (`user_id = me`, static
/// per-user shape, NOT team/trash scoped). `kind` is a contract `pinKind`
/// (`issue` | `session`; NEW-8: a legacy `action` pin is ignored) and its
/// target id is set; every non-PK field is `Option` like `Notification` so an unknown
/// kind or an absent target degrades to a hidden row, never a wedged
/// hydration. `sort_order` ascending = display order (a new pin appends).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Pin {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `issue_drafts` shape row (EXP-878) — one unfiled create-issue dialog,
/// from a STATIC per-user shape (`user_id = me`), neither team- nor
/// trash-scoped. A row renders only once its `board_id` resolves in the
/// boards collection (the pins rule), so a draft on a trashed board is
/// simply hidden rather than an error.
///
/// Every non-PK field is `Option` like [`Pin`]: a partial row degrades to a
/// blank field, never a dropped draft. `label_ids` arrives as a Postgres
/// `uuid[]` cell (`{a,b}`, a JSON array, or a single id) — the same tolerant
/// list parse `DeviceRow::shared_team_ids` takes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IssueDraftRow {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub board_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// EXP-314: the precise `issue_statuses` row; `None` = the team's
    /// Backlog builtin (what a fresh composer starts on).
    #[serde(default)]
    pub status_id: Option<String>,
    #[serde(default)]
    pub priority: Option<IssuePriority>,
    #[serde(default)]
    pub assignee_id: Option<String>,
    /// #EXP-1383: an agent assignee (exclusive with `assignee_id`).
    #[serde(default)]
    pub assignee_agent_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_id_list")]
    pub label_ids: Vec<String>,
    /// `YYYY-MM-DD` — date only, like `issues.due_date`.
    #[serde(default)]
    pub due_date: Option<String>,
    /// EXP-1294: the session root a "New issue" opened from a thread files
    /// into (`issues.create({draftId})` links it there). Stored on the draft,
    /// so a draft reopened from Drafts still files into that root.
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `users` shape row (co-member-scoped; the server pins the 6-column
/// contract list — admin/verification/billing fields never sync).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Display fallback for a user id whose [`User`] row didn't sync: a referenced
/// id can resolve to no local row (the user's row hasn't synced yet, or the
/// account was deleted). Rather than leak the raw id (or a misleading
/// "Someone"), show `Member <LAST4>` — the uppercased last four chars of the
/// id.
pub fn member_fallback_label(user_id: &str) -> String {
    let chars: Vec<char> = user_id.chars().collect();
    let start = chars.len().saturating_sub(4);
    let tail: String = chars[start..].iter().collect();
    format!("Member {}", tail.to_uppercase())
}

/// `team_members` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamMember {
    pub id: String,
    pub team_id: String,
    pub user_id: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `team_invites` shape row (requireAuth shape, §5.9). The server's
/// columns allowlist excludes the bearer `token` from the shape (REV-4/14) —
/// the invite link is built once from the create mutation's response, never
/// from synced rows.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamInvite {
    pub id: String,
    pub team_id: String,
    #[serde(default)]
    pub invited_by_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    /// Optional invitee email (EXP-188 invite-by-email; null on link-only
    /// invites).
    #[serde(default)]
    pub email: Option<String>,
    /// EXP-630: the PLACEHOLDER member this invite created — the person is on
    /// the roster (assignable, attributable) before they ever sign in. `None`
    /// on link invites and on invites to an existing account;
    /// [`crate::placeholder_status`] turns an unaccepted one into the member
    /// row's "Invited" / "Invite expired" badge.
    #[serde(default)]
    pub placeholder_user_id: Option<String>,
    #[serde(default)]
    pub accepted_at: Option<String>,
    /// EXP-1076: when the invite LINK was issued. `None` = a roster row nobody
    /// was ever invited (the Linear import seats its placeholders without a
    /// mail); the badge reads "Not invited" and the menu offers a FIRST send.
    /// Such rows are also stamped `expires_at = created_at`, so expiry alone
    /// would misread them as lapsed.
    #[serde(default)]
    pub sent_at: Option<String>,
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
    pub team_id: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    /// #EXP-1383: the agent whose session wrote it (`author_id` = its
    /// member): the card names the agent and drops `via MCP`.
    #[serde(default)]
    pub author_agent_id: Option<String>,
    /// EXP-741: the top-level comment this one replies to (one level deep);
    /// `None` = a top-level card.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// EXP-741: `user` | `mcp` — an agent posted it over MCP ("via MCP").
    /// SLOP-4: `reporter` — a widget reporter wrote it from the reporter
    /// page (`author_id` NULL; the card names the submission's reporter).
    #[serde(default)]
    pub source: Option<String>,
    /// SLOP-4: `team` | `reporter` (contract `commentAudience`) — whether
    /// the comment left the team (emailed to the widget reporter, shown on
    /// the reporter page). Missing/NULL (pre-column store rows) reads as
    /// `team`, the server default.
    #[serde(default = "default_comment_audience", deserialize_with = "comment_audience")]
    pub audience: String,
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

fn default_comment_audience() -> String {
    crate::contract::COMMENT_AUDIENCE_TEAM.to_string()
}

/// `audience` hydrates tolerantly: a missing key, an explicit `null` (a
/// store row healed before its refetch landed) or an empty string all read
/// as `team`.
fn comment_audience<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    Ok(value
        .filter(|audience| !audience.is_empty())
        .unwrap_or_else(default_comment_audience))
}

impl Comment {
    /// SLOP-4: a widget reporter's own words (source `reporter`, no author).
    pub fn is_from_reporter(&self) -> bool {
        self.source.as_deref() == Some(crate::contract::COMMENT_SOURCE_REPORTER)
    }

    /// SLOP-4: a member's comment that was emailed to the reporter.
    pub fn is_to_reporter(&self) -> bool {
        self.audience == crate::contract::COMMENT_AUDIENCE_REPORTER
    }
}

/// `attachments` shape row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub board_id: Option<String>,
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
    /// EXP-824: probed media duration (video/audio rows only).
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub duration_ms: Option<i64>,
    /// EXP-824: non-null when a poster frame exists (`/api/attachments/{id}?poster=1`).
    #[serde(default)]
    pub poster_storage_key: Option<String>,
    /// EXP-1247: uploaded through a FILE button (paperclip / Attach file):
    /// never inlined, listed under Files even when it is an image. `None` (a
    /// NULL or partial row) = false.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub as_file: Option<bool>,
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
    /// Set on issue-less `agent_message` (EXP-801) and `session_blocked`
    /// (EXP-980) rows so the inbox row can name the team; NULL on
    /// issue-anchored rows (their team resolves via the issue) and on
    /// pre-column rows. SLOP-4: the helpdesk's `support_reply` rows are gone
    /// — `reporter_reply` is issue-scoped like `issue_comment`.
    #[serde(default)]
    pub team_id: Option<String>,
    /// EXP-980: set on issue-less `session_blocked` rows (the run that hit a
    /// rate limit) so the inbox row can open it; NULL everywhere else, and on
    /// a row whose run has since been pruned.
    #[serde(default)]
    pub session_id: Option<String>,
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
    pub team_id: Option<String>,
    #[serde(default)]
    pub actor_user_id: Option<String>,
    /// #EXP-1383: the agent whose session acted (`actor_user_id` = its
    /// member).
    #[serde(default)]
    pub actor_agent_id: Option<String>,
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
    pub team_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub unsubscribed: Option<bool>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// EXP-1294 `sessions` shape row — ONE table = the session TREE (Threads v2).
/// Roots (`parent_id` NULL, `root_id = id`, depth 0) are threads or an
/// agent's conversation (`agent_id` set; its name/icon/color live on the
/// `agents` definition); children are thread sessions under a root. The runner columns
/// (`device_id`, `cli`, `account`, `model`, …) say where and how it runs;
/// `status` = contract `sessionStatus` (`queued`/`running`/`idle`/`ended`),
/// with `busy`/`needs_input`/`blocked` device-written beside it. Thread
/// facets (`title`, `guide`) live on roots. NEW-8: the legacy columns
/// (`kind`, `dispatch`, `caption`, `aside`, `action_*`, `name`, `icon`, …)
/// are never decoded: present today, absent after the drop, ignored both
/// ways (serde is non-strict). Server-only (never synced): `host_user_id`, `mcp_server_ids`,
/// `instructions` (`sessions.getAgent`).
///
/// Only `id` is required; the bools the UI branches on read `false` when
/// absent or unparsable, jsonb arrives tolerant (a TEXT cell re-parses).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SessionRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    /// `None` on a root.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// The tree's root (`= id` on a root). `None` only on a partial row:
    /// read through [`SessionRow::root_id_or_self`].
    #[serde(default)]
    pub root_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub depth: Option<i64>,
    /// NEW-2: the agent DEFINITION (`agents` row) this conversation session
    /// runs; `None` = a thread.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// VAPP-110: the APP whose editor session this root is (`apps` row; one
    /// root per app and member).
    #[serde(default)]
    pub app_id: Option<String>,
    /// The person who STARTED it (not necessarily the host's owner).
    #[serde(default)]
    pub user_id: Option<String>,
    /// The device that hosts it (steer device id).
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
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub plan_mode: bool,
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub computer_use: bool,
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub ultracode: bool,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    /// Contract `sessionStatus`: `queued` | `running` | `idle` | `ended`.
    #[serde(default)]
    pub status: Option<String>,
    /// The agent is executing a turn RIGHT NOW (the spinner's one input).
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub busy: bool,
    /// EXP-1374: busy only because background tasks (shell, workflow,
    /// subagent) still run after the turn (absent = false).
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub background: bool,
    /// Parked on a question/plan for a person.
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub needs_input: bool,
    /// EXP-804 jsonb usage wall `{kind, agent, window, resetsAt, since}`;
    /// `None` = not blocked.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub blocked: Option<serde_json::Value>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub last_turn_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    /// #EXP-1383: when the device last fetched this session's brief
    /// (`sessions.brief`); the agent chat's instructions hint compares it
    /// with `agents.instructions_updated_at`.
    #[serde(default)]
    pub agent_instructions_at: Option<String>,
    /// #EXP-1383: the agent whose session started this root (an agent tree).
    #[serde(default)]
    pub started_by_agent_id: Option<String>,
    /// `false` = private (a lock glyph): only its starter sees it.
    #[serde(default, deserialize_with = "tolerant_bool_or_false")]
    pub shared: bool,
    #[serde(default)]
    pub title: Option<String>,
    /// EXP-1458: the root's tree public key (base64url, 32 bytes) and its
    /// current epoch; both `None` until the tree is keyed (root rows only).
    #[serde(default)]
    pub seal_public_key: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub seal_epoch: Option<i64>,
    /// EXP-1458: the title sealed to the tree (body envelope); `title` then
    /// stays a neutral fallback. Open it through `coding::sealing`.
    #[serde(default)]
    pub sealed_title: Option<String>,
    /// THE Guide (`GuideEntry[]`, the old results entries = the PR body);
    /// read through [`crate::session_results`].
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub guide: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl SessionRow {
    /// A tree root (no parent).
    pub fn is_root(&self) -> bool {
        self.parent_id.as_deref().is_none_or(str::is_empty)
    }

    /// NEW-2: the derived kind ([`crate::session_kind::session_kind_of`]).
    pub fn session_kind(&self) -> &'static str {
        crate::session_kind::session_kind_of(self.agent_id.as_deref())
    }

    /// An agent's conversation session (`agent_id` set; never the `kind`
    /// shim).
    pub fn is_agent(&self) -> bool {
        self.session_kind() == crate::contract::SESSION_KIND_AGENT
    }

    /// VAPP-110: a member's app EDITOR session (`app_id` set): no thread,
    /// like an agent's conversation (`partitionThreads` skips both).
    pub fn is_app_editor(&self) -> bool {
        self.app_id.as_deref().is_some_and(|id| !id.is_empty())
    }

    /// EXP-1341: a persistent session (an agent's conversation): it idles
    /// instead of ending and its device revives it ([`crate::session_kind`]).
    pub fn is_persistent(&self) -> bool {
        crate::session_kind::is_persistent(self.agent_id.as_deref())
    }

    /// A thread session (root or child): every kind that is not `agent`.
    pub fn is_thread(&self) -> bool {
        !self.is_agent()
    }

    pub fn is_ended(&self) -> bool {
        self.status.as_deref() == Some(crate::contract::SESSION_STATUS_ENDED)
    }

    pub fn is_queued(&self) -> bool {
        self.status_is(crate::contract::SESSION_STATUS_QUEUED)
    }

    /// `status` equals the contract `sessionStatus` value.
    pub fn status_is(&self, status: &str) -> bool {
        self.status.as_deref() == Some(status)
    }

    /// EXP-1374: an INTERRUPTION its device continues by itself — `blocked`
    /// names `device_offline` | `restarted` with `resume: true` (the run
    /// was mid-turn). The kind, else `None`.
    pub fn continue_interruption(&self) -> Option<&str> {
        let blocked = self.blocked.as_ref()?;
        let kind = blocked.get("kind")?.as_str()?;
        let resume = blocked.get("resume").and_then(serde_json::Value::as_bool) == Some(true);
        (resume && matches!(kind, "device_offline" | "restarted")).then_some(kind)
    }

    /// Hosted by this steer device.
    pub fn on_device(&self, device_id: &str) -> bool {
        self.device_id.as_deref() == Some(device_id)
    }

    /// Not ended (queued, running or idle).
    pub fn is_live(&self) -> bool {
        !self.is_ended()
    }

    /// The tree's root id: `root_id`, else the row itself.
    pub fn root_id_or_self(&self) -> &str {
        self.root_id
            .as_deref()
            .filter(|root| !root.is_empty())
            .unwrap_or(&self.id)
    }

    /// The stored title, trimmed; `None` when unset or blank.
    pub fn explicit_title(&self) -> Option<&str> {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
    }
}

/// EXP-1294 `session_issues` shape row — a ROOT session ↔ issue link
/// (contract `sessionIssueSource` `user` | `agent`, the latter naming the
/// session whose agent linked it). Board-scoped like every issue child (the
/// board mirrors stay server-side).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionIssueRow {
    pub id: String,
    pub session_id: String,
    pub issue_id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub added_by: Option<String>,
    #[serde(default)]
    pub added_by_session_id: Option<String>,
    /// Ascending = the thread's issue-chip order.
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// EXP-1294 `messages` shape row — one entry of a session's conversation.
/// `author_kind` = contract `messageAuthorKind` (`user` | `agent` |
/// `system`); `card` = jsonb `{kind, …}` (contract `messageCardKind`; an
/// unknown kind renders nothing). A person's message with `consumed_at`
/// NULL is QUEUED (the device has not picked it up yet).
///
/// NEW-8: a message lives in a CONVERSATION (`conversation_id`); its session
/// is the session of that conversation, resolved through the `conversations`
/// shape by [`crate::message_conversation::key_messages_by_session`]. The
/// row's own legacy `session_id` column is NEVER decoded.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct MessageRow {
    pub id: String,
    /// NEW-8: the conversation the row lives in (required on the wire).
    pub conversation_id: String,
    /// NEW-8: the RESOLVED session (the session its conversation belongs
    /// to), filled by [`crate::message_conversation::key_messages_by_session`]
    /// only; empty on a row straight off the store. Never decoded: a legacy
    /// `session_id` on the server row is ignored.
    #[serde(skip_deserializing)]
    pub session_id: String,
    #[serde(default)]
    pub root_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub author_kind: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub card: Option<serde_json::Value>,
    #[serde(default)]
    pub answered_at: Option<String>,
    #[serde(default)]
    pub consumed_at: Option<String>,
    /// EXP-1323: the session that posted it (a child asking its parent,
    /// a report, a suggestions card); `None` = a person or the root itself.
    #[serde(default)]
    pub author_session_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// EXP-1459: the sealed body/card's state after `sync::sealed` opened
    /// it (`None` = plaintext row). Not `Opened` = body empty, card = its
    /// clear part: render the locked row.
    #[serde(skip_deserializing)]
    pub seal: Option<SealState>,
}

/// NEW-8 `conversations` shape row: the chat primitive a message lives in.
/// `kind` = `session` (one per session, `session_id` set) | `dm` |
/// `channel` (`session_id` NULL).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ConversationRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// #EXP-1380: the list's digest of the messages (trigger-written jsonb;
    /// [`crate::chats::digest_messages`] reads it back).
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub digest: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// EXP-1323 `session_reads` shape row (per user, static): the viewer read
/// `session_id` up to `read_at` (`sessions.markRead`). Drives the bold
/// unread titles + the Threads nav dot.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SessionReadRow {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    pub session_id: String,
    #[serde(default)]
    pub read_at: Option<String>,
    /// #EXP-1388: the viewer archived the chat (per viewer, on the root).
    #[serde(default)]
    pub archived_at: Option<String>,
    /// S-3: the viewer's stories mark on the root (`sessions.markStoriesSeen`;
    /// seen = createdAt <= max(read_at, story_seen_at)).
    #[serde(default)]
    pub story_seen_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl MessageRow {
    /// EXP-1459: why this row's sealed content stays locked here.
    pub fn locked(&self) -> Option<crate::sealed::LockedReason> {
        crate::sealed::locked_reason(self.seal)
    }

    /// The card's `kind` (`question`, `plan`, `session`, …); `None` for a
    /// plain message (or a card without a string kind).
    pub fn card_kind(&self) -> Option<&str> {
        self.card.as_ref()?.get("kind")?.as_str()
    }

    pub fn body_text(&self) -> &str {
        self.body.as_deref().unwrap_or_default()
    }

    pub fn is_from_user(&self) -> bool {
        self.author_kind.as_deref() == Some(crate::contract::MESSAGE_AUTHOR_KIND_USER)
    }

    /// A person's message the device has not consumed yet.
    pub fn is_queued(&self) -> bool {
        self.is_from_user() && self.consumed_at.as_deref().is_none_or(str::is_empty)
    }
}

/// EXP-1294 `pull_requests` shape row — ALL PR state left `issues`. One row
/// per GitHub PR (`repository_id` + `number`); `root_session_id` = the
/// thread it belongs to, `session_id` = the session that opened it.
/// `base_pull_request_id` = the explicit STACK edge (the PR this one is
/// based on, [`crate::pr_stack`]); `state` = contract `prState`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PullRequestRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub root_session_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub number: Option<i64>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// The head branch.
    #[serde(default)]
    pub head: Option<String>,
    /// The branch it targets.
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub base_pull_request_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub merged_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl PullRequestRow {
    pub fn is_open(&self) -> bool {
        self.state.as_deref() == Some(crate::contract::PR_STATE_OPEN)
    }

    pub fn is_merged(&self) -> bool {
        self.state.as_deref() == Some(crate::contract::PR_STATE_MERGED)
    }

    pub fn is_closed(&self) -> bool {
        self.state.as_deref() == Some(crate::contract::PR_STATE_CLOSED)
    }
}

/// EXP-1294 `pull_request_issues` shape row — a PR ↔ issue link (a PR that
/// closes several issues carries several rows).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PullRequestIssueRow {
    pub id: String,
    pub pull_request_id: String,
    pub issue_id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// NEW-2: `agents` shape row: a team agent DEFINITION (never a session).
/// The server-only `instructions` and `trigger_state` never sync (tRPC
/// `agents.get`). Named `AgentDefRow` so it never clashes with the Agents
/// list's [`crate::session_tree::AgentRow`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentDefRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// An icon concept (contract `boardIcon`); `None` = the generic glyph.
    #[serde(default)]
    pub icon: Option<String>,
    /// `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    /// jsonb `AgentTrigger[]` (`{id, enabled, kind: schedule, cron, tz?}` |
    /// `{id, enabled, kind: event, name}`); the SERVER fires them.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub triggers: Option<serde_json::Value>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub cli: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    /// S-3: contract `agentRole` (`assistant` | `agent`); an old row without
    /// it is a team agent.
    #[serde(default = "default_agent_role", deserialize_with = "tolerant_agent_role")]
    pub role: String,
    /// #EXP-1383: when the instructions TEXT last changed (chats.json
    /// `agentHint`).
    #[serde(default)]
    pub instructions_updated_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn default_agent_role() -> String {
    crate::contract::AGENT_ROLE_AGENT.to_string()
}

/// `null` / absent / not a string = a team agent.
fn tolerant_agent_role<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = <Option<serde_json::Value> as Deserialize>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(role)) if !role.is_empty() => role,
        _ => default_agent_role(),
    })
}

impl AgentDefRow {
    /// S-3: the member's Assistant (`role = assistant`).
    pub fn is_assistant(&self) -> bool {
        self.role == crate::contract::AGENT_ROLE_ASSISTANT
    }

    /// The agent's triggers (`[]` when absent or not an array).
    pub fn trigger_list(&self) -> Vec<serde_json::Value> {
        match &self.triggers {
            Some(serde_json::Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        }
    }
}

/// NEW-2: `events` shape row: one entry of the team's event stream.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EventRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    /// Free-text event name (`issue.created`, or anything `events_emit`
    /// posts).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub payload: Option<serde_json::Value>,
    /// Contract `events.emitterValues`: `session` | `user` | `server`.
    #[serde(default)]
    pub emitted_by: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// NEW-2: `prompts` shape row: a saved prompt, body-less (the body stays
/// server-only, tRPC `prompts.get`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PromptRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// VAPP-110: `apps` shape row: a team's declarative Exponential UI app. The
/// HEAD version's `package` rides along (the editor's live preview repaints
/// from it); `deleted_at` is the trash mirror (clients hide trashed rows).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AppRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub package: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub head_version: Option<i64>,
    #[serde(default)]
    pub created_by: Option<String>,
    /// VAPP-120: the exact `exponential.*` function names this app may run as
    /// the viewer without the consent card (jsonb `string[]`; null, absent or
    /// garbage reads as EMPTY, never a dropped row).
    #[serde(default, deserialize_with = "tolerant_id_list")]
    pub granted_tools: Vec<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// VAPP-110: `app_versions` shape row: one published version, package-less
/// (an old version's package loads on demand, tRPC `apps.get {version}`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AppVersionRow {
    pub id: String,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub number: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// VAPP-122: `app_records` shape row: one record of an app's own data. The
/// trash columns stay server-side (a trashed record leaves the shape);
/// `data` is the record's JSON object (a stringified cell re-parses, null or
/// garbage reads as `null`, never a dropped row).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AppRecordRow {
    pub id: String,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default, deserialize_with = "tolerant_json")]
    pub data: serde_json::Value,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn tolerant_json<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(tolerant_opt_json(deserializer)?.unwrap_or(serde_json::Value::Null))
}

/// `issue_statuses` shape row (EXP-314) — the 16th shape: one team's status
/// vocabulary. `category` stays a RAW wire string (typed at use through
/// [`crate::statuses::IssueStatusCategory::from_wire`]) so a newer server's
/// category can never fail hydration and drop the row (§5.5).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IssueStatusRow {
    pub id: String,
    pub team_id: String,
    /// Wire value of `contract::ISSUE_STATUS_CATEGORY_VALUES`.
    pub category: String,
    pub name: String,
    /// `#rrggbb`. Rendered only for CUSTOM rows — builtin rows render their
    /// theme token (see the `statuses` module header).
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    /// `Some(wire)` on the 7 locked builtins, `None` on custom rows.
    #[serde(default)]
    pub builtin_key: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `devices` shape row (EXP-481) — the per-user machine registry, own rows
/// plus team-shared server rows. SERVER-AUTHORITATIVE: `launch_defaults` is
/// the canonical copy this machine's settings.json converges to; online-ness
/// derives from `last_seen_at` freshness against
/// `contract::DEVICE_ONLINE_WINDOW_MS` — never relay presence.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DeviceRow {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    /// The steer device id (the remote-start target), NOT this row's PK.
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// `desktop` | `server` — unknown renders as desktop.
    #[serde(default)]
    pub kind: Option<String>,
    /// EXP-924: the owner-picked device glyph (`contract::DEVICE_ICON_VALUES`).
    /// NULL = the kind default, and so does a name outside the device set —
    /// the resolver is `ui::icons::device_icon`, never a raw read of this.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// jsonb string[] — TEXT-stored (§5.5), re-parsed at hydrate.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub agents: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub caps: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub unauthed_agents: Option<serde_json::Value>,
    /// EXP-749 jsonb string[] — the subset of `agents` that speaks ACP on
    /// that machine. Every build at the current floor writes it on register
    /// AND on every heartbeat, so a NULL column belongs to a row nothing
    /// online wrote: it reads as EMPTY (nothing there speaks ACP), never as
    /// "assume everything". Still tolerantly decoded — a row that omits the
    /// column must parse, not sink the whole device list.
    /// Never a filter, only a label ([`Self::agent_cannot_run_session`]).
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub acp_agents: Option<serde_json::Value>,
    /// jsonb `{defaultAgent?, agents?: {..}}` — camelCase inner keys.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub launch_defaults: Option<serde_json::Value>,
    #[serde(default)]
    pub launch_defaults_updated_at: Option<String>,
    /// EXP-484 jsonb `{<agent>: {signedIn, email?, plan?, checkedAt}}` — who
    /// is signed in to each agent CLI on that machine. READ-ONLY here: the
    /// device writes it on register/heartbeat, no client ever edits it.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub agent_accounts: Option<serde_json::Value>,
    /// EXP-484 jsonb `{<agent>: {fetchedAt, stale, windows: [..]}}` — the
    /// rate-limit windows, same rules.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub agent_usage: Option<serde_json::Value>,
    /// When the server last accepted an `agent_usage` write. Deliberately
    /// NOT a sync nudge trigger anywhere: it moves every few minutes.
    #[serde(default)]
    pub agent_usage_at: Option<String>,
    /// EXP-1196 jsonb `{checkedAt, items: [..]}` — the device readiness
    /// block that machine built (`coding::device_doctor::DeviceDoctor`).
    /// READ-ONLY; NULL (an older build) renders no block. Kept raw here:
    /// the ui parses it leniently.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub doctor: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub active_sessions: Option<i64>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    /// FEED-33 `uuid[]`: every team this (server) machine is shared with,
    /// sorted + deduped server-side, EMPTY when private. Arrives as the
    /// Postgres array literal in a JSON string (`"{a,b}"`).
    #[serde(default, deserialize_with = "tolerant_id_list")]
    pub shared_team_ids: Vec<String>,
    /// EXP-622: the ROW OWNER's default machine — the one every device picker
    /// prefills. Honour it only when `user_id` is the signed-in user: a
    /// teammate's shared server carries THEIR preference, not ours.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub update_requested_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl DeviceRow {
    /// String-array view of a jsonb column (`agents`/`caps`/…).
    fn string_list(value: &Option<serde_json::Value>) -> Vec<String> {
        value
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn agent_ids(&self) -> Vec<String> {
        Self::string_list(&self.agents)
    }

    pub fn unauthed_agent_ids(&self) -> Vec<String> {
        Self::string_list(&self.unauthed_agents)
    }

    /// EXP-749: the machine's ACP-ready agents. A NULL/unparseable column
    /// folds to EMPTY: every build a client can reach advertises the list, so
    /// silence is "nothing here speaks ACP", not a licence to assume.
    pub fn acp_agent_ids(&self) -> Vec<String> {
        let Some(list) = self.acp_agents.as_ref().and_then(|value| value.as_array()) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect()
    }

    /// EXP-773: whether `agent` CANNOT run a coding session on this machine
    /// — it is runnable there, but absent from the ACP-ready list, and the
    /// engine is the only transport. An agent this device cannot run at all
    /// is not this note's business: the launch gate names that failure
    /// instead.
    pub fn agent_cannot_run_session(&self, agent: &str) -> bool {
        let acp = self.acp_agent_ids();
        self.agent_ids().iter().any(|id| id == agent) && !acp.iter().any(|id| id == agent)
    }

    pub fn cap_ids(&self) -> Vec<String> {
        Self::string_list(&self.caps)
    }

    pub fn is_server(&self) -> bool {
        self.kind.as_deref() == Some("server")
    }

    /// FEED-33: shared with at least one team.
    pub fn is_shared(&self) -> bool {
        !self.shared_team_ids.is_empty()
    }

    /// FEED-33: shared with `team_id` specifically.
    pub fn is_shared_with(&self, team_id: &str) -> bool {
        self.shared_team_ids.iter().any(|id| id == team_id)
    }

    /// #EXP-1440: one of the caller's OWN machines may run a team's sessions
    /// when it is personal (shared with no team) or shared with that team; a
    /// machine shared only with other teams is theirs (web
    /// `session-device-move.ts` `deviceUsableInTeam`).
    pub fn is_usable_in_team(&self, team_id: &str) -> bool {
        !self.is_shared() || self.is_shared_with(team_id)
    }

    /// EXP-484: this machine's account row for `agent` (`claude`/`codex`/
    /// `codex`), still as the raw wire object — `domain` deliberately does not
    /// depend on `coding`, so the typed shape stays there and callers
    /// deserialize what they need.
    pub fn agent_account(&self, agent: &str) -> Option<&serde_json::Value> {
        Self::agent_entry(&self.agent_accounts, agent)
    }

    /// EXP-484: its usage snapshot for `agent`.
    pub fn agent_usage_for(&self, agent: &str) -> Option<&serde_json::Value> {
        Self::agent_entry(&self.agent_usage, agent)
    }

    fn agent_entry<'a>(
        column: &'a Option<serde_json::Value>,
        agent: &str,
    ) -> Option<&'a serde_json::Value> {
        column
            .as_ref()?
            .as_object()?
            .get(agent)
            .filter(|entry| entry.is_object())
    }
}

/// `device_worktrees` shape row (EXP-481) — one reported session worktree.
/// The scoping mirrors (`user_id`/`shared_team_ids`) are proxy-excluded and
/// deliberately NOT modeled (issue_subscribers-email stance).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DeviceWorktreeRow {
    pub id: String,
    /// FK to the DEVICES row's `id` (uuid) — not the steer device-id string.
    #[serde(default)]
    pub device_row_id: Option<String>,
    #[serde(default)]
    pub repo_full_name: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    /// `exp/<ID>` linkage as the device parsed it; `None` on batch branches.
    #[serde(default)]
    pub issue_identifier: Option<String>,
    /// jsonb string[] `.exp-agents` marker; `None` = any agent may resume.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub agents: Option<serde_json::Value>,
    /// `clean` | `untracked` | `tracked` | `unknown` — raw wire string.
    #[serde(default)]
    pub dirty: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub busy: Option<bool>,
    #[serde(default)]
    pub reported_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// EXP-824: media rows hydrate their probed duration (a quoted wire int
    /// too) and the poster marker; a partial row still hydrates with both
    /// absent.
    #[test]
    fn attachment_row_hydrates_media_fields_tolerantly() {
        let row: Attachment = serde_json::from_value(json!({
            "id": "att-1",
            "board_id": "b-1",
            "content_type": "video/mp4",
            "duration_ms": "154000",
            "poster_storage_key": "posters/att-1.jpg",
        }))
        .unwrap();
        assert_eq!(row.board_id.as_deref(), Some("b-1"));
        assert_eq!(row.duration_ms, Some(154_000));
        assert_eq!(row.poster_storage_key.as_deref(), Some("posters/att-1.jpg"));
        let narrow: Attachment = serde_json::from_value(json!({ "id": "att-2" })).unwrap();
        assert_eq!(narrow.duration_ms, None);
        assert_eq!(narrow.poster_storage_key, None);
        assert_eq!(narrow.board_id, None);
        assert_eq!(narrow.as_file, None);
    }

    /// EXP-1247: the file-mode marker hydrates from every wire form.
    #[test]
    fn attachment_row_hydrates_as_file_tolerantly() {
        for (wire, want) in [(json!(true), Some(true)), (json!("t"), Some(true)), (json!("false"), Some(false))] {
            let row: Attachment = serde_json::from_value(json!({ "id": "a", "as_file": wire })).unwrap();
            assert_eq!(row.as_file, want);
        }
    }

    #[test]
    fn device_row_hydrates_tolerantly_and_lists_agents() {
        // jsonb columns arrive TEXT-stored (§5.5) — string forms must
        // re-parse; wire ints may be quoted.
        let row: DeviceRow = serde_json::from_value(json!({
            "id": "row-1",
            "user_id": "u-1",
            "device_id": "dev-1",
            "label": "buildbox",
            "kind": "server",
            "agents": "[\"claude\",\"codex\"]",
            "caps": ["actions", "resume"],
            "launch_defaults": "{\"defaultAgent\":\"codex\"}",
            "active_sessions": "2",
            "last_seen_at": "2026-08-11T10:00:00.000Z",
        }))
        .unwrap();
        assert!(row.is_server());
        assert_eq!(row.agent_ids(), vec!["claude", "codex"]);
        assert_eq!(row.cap_ids(), vec!["actions", "resume"]);
        assert_eq!(
            row.launch_defaults.as_ref().unwrap()["defaultAgent"],
            "codex"
        );
        assert_eq!(row.active_sessions, Some(2));
        // A minimal (partial) row still hydrates.
        let narrow: DeviceRow = serde_json::from_value(json!({"id": "row-2"})).unwrap();
        assert!(!narrow.is_server());
        assert!(narrow.agent_ids().is_empty());
        assert!(!narrow.is_shared());
    }

    /// #EXP-1440: personal -> usable, shared with the team -> usable, shared
    /// only with another team -> not usable.
    #[test]
    fn device_row_is_usable_in_team_unless_another_team_owns_it() {
        let row = |shares: &[&str]| DeviceRow {
            shared_team_ids: shares.iter().map(|id| id.to_string()).collect(),
            ..serde_json::from_value(json!({ "id": "d-1" })).unwrap()
        };
        assert!(row(&[]).is_usable_in_team("team-1"));
        assert!(row(&["team-1"]).is_usable_in_team("team-1"));
        assert!(row(&["team-2", "team-1"]).is_usable_in_team("team-1"));
        assert!(!row(&["team-2"]).is_usable_in_team("team-1"));
    }

    /// EXP-878: a draft row hydrates from the wire with every optional
    /// absent, and `label_ids` takes the same `uuid[]` cell forms
    /// `shared_team_ids` does.
    #[test]
    fn issue_draft_row_hydrates_sparsely_and_parses_label_ids() {
        let bare: IssueDraftRow = serde_json::from_value(serde_json::json!({
            "id": "d-1",
        }))
        .expect("bare draft hydrates");
        assert_eq!(bare.title, None);
        assert_eq!(bare.priority, None);
        assert!(bare.label_ids.is_empty());

        let full: IssueDraftRow = serde_json::from_value(serde_json::json!({
            "id": "d-1",
            "user_id": "u-1",
            "team_id": "t-1",
            "board_id": "b-1",
            "title": "Ship it",
            "description": "Body",
            "status_id": "s-1",
            "priority": "high",
            "assignee_id": "u-2",
            // The Postgres array literal, exactly like devices.shared_team_ids.
            "label_ids": "{l-1,l-2}",
            "due_date": "2026-09-14",
            "session_id": "th-1",
        }))
        .expect("full draft hydrates");
        assert_eq!(full.label_ids, vec!["l-1", "l-2"]);
        // EXP-1294: the draft's session root (absent = files into none).
        assert_eq!(full.session_id.as_deref(), Some("th-1"));
        assert_eq!(bare.session_id, None);
        assert_eq!(full.priority, Some(IssuePriority::High));
        assert_eq!(full.due_date.as_deref(), Some("2026-09-14"));

        // A JSON array and a null cell hydrate too — never a dropped row.
        let json_array: IssueDraftRow = serde_json::from_value(serde_json::json!({
            "id": "d-2",
            "label_ids": ["l-9"],
        }))
        .expect("json array hydrates");
        assert_eq!(json_array.label_ids, vec!["l-9"]);
        let null_cell: IssueDraftRow = serde_json::from_value(serde_json::json!({
            "id": "d-3",
            "label_ids": serde_json::Value::Null,
        }))
        .expect("null hydrates");
        assert!(null_cell.label_ids.is_empty());
    }

    /// FEED-33: `shared_team_ids` is a `uuid[]`: the wire cell is the
    /// Postgres array literal as a string; a JSON array, null and garbage
    /// hydrate too (the last two as private).
    #[test]
    fn device_row_hydrates_shared_team_ids_from_every_wire_form() {
        let row: DeviceRow = serde_json::from_value(json!({
            "id": "row-1",
            "shared_team_ids": "{team-1,team-2}",
        }))
        .unwrap();
        assert_eq!(row.shared_team_ids, vec!["team-1", "team-2"]);
        assert!(row.is_shared());
        assert!(row.is_shared_with("team-2"));
        assert!(!row.is_shared_with("team-3"));

        let one: DeviceRow = serde_json::from_value(json!({
            "id": "row-1",
            "shared_team_ids": "{\"team-1\"}",
        }))
        .unwrap();
        assert_eq!(one.shared_team_ids, vec!["team-1"]);

        let json_array: DeviceRow = serde_json::from_value(json!({
            "id": "row-1",
            "shared_team_ids": ["team-1"],
        }))
        .unwrap();
        assert_eq!(json_array.shared_team_ids, vec!["team-1"]);

        for cell in [json!("{}"), json!(null), json!("garbage"), json!(7)] {
            let row: DeviceRow = serde_json::from_value(json!({
                "id": "row-1",
                "shared_team_ids": cell,
            }))
            .unwrap();
            assert!(!row.is_shared(), "{cell} must read as private");
        }
    }

    /// EXP-484: the two agent-status columns hydrate through the same
    /// TEXT-stored tolerance as every other jsonb, and the accessors read
    /// one agent out (an absent/garbage column is simply nothing to show).
    #[test]
    fn device_row_reads_agent_accounts_and_usage() {
        let row: DeviceRow = serde_json::from_value(json!({
            "id": "row-1",
            "agent_accounts": "{\"claude\":{\"signedIn\":true,\"email\":\"dev@acme.test\",\"plan\":\"max\"},\"codex\":{\"signedIn\":false}}",
            "agent_usage": {"claude": {"fetchedAt": "2026-08-28T10:00:00.000Z", "stale": false, "windows": [{"key": "session", "label": "5h", "percent": 42}]}},
            "agent_usage_at": "2026-08-28T10:00:00.000Z",
        }))
        .unwrap();
        assert_eq!(
            row.agent_account("claude").unwrap()["email"],
            "dev@acme.test"
        );
        assert_eq!(row.agent_account("codex").unwrap()["signedIn"], false);
        assert_eq!(row.agent_account("gemini"), None);
        assert_eq!(
            row.agent_usage_for("claude").unwrap()["windows"][0]["percent"],
            42
        );
        assert_eq!(row.agent_usage_for("codex"), None);
        assert_eq!(row.agent_usage_at.as_deref(), Some("2026-08-28T10:00:00.000Z"));

        // A partial row (or a garbage column) reads as nothing
        // to render, never a dropped row.
        let narrow: DeviceRow = serde_json::from_value(json!({
            "id": "row-2",
            "agent_accounts": "not json",
        }))
        .unwrap();
        assert_eq!(narrow.agent_account("claude"), None);
        assert_eq!(narrow.agent_usage_for("claude"), None);
    }

    /// EXP-749: the ACP-ready subset. The list is authoritative (EXP-773: an
    /// agent missing from it cannot start a run there at all), and a NULL
    /// column reads as EMPTY — every build a client can reach advertises it.
    #[test]
    fn device_row_reads_acp_agents_and_null_means_none() {
        // TEXT-stored jsonb, like every other list column.
        let row: DeviceRow = serde_json::from_value(json!({
            "id": "row-1",
            "agents": "[\"claude\",\"codex\"]",
            "acp_agents": "[\"claude\"]",
        }))
        .unwrap();
        assert_eq!(row.acp_agent_ids(), vec!["claude".to_string()]);
        assert!(!row.agent_cannot_run_session("claude"));
        assert!(row.agent_cannot_run_session("codex"));
        // An agent the machine cannot run at all is the launch gate's
        // business, not this note.
        assert!(!row.agent_cannot_run_session("nope"));

        // An EMPTY list is a real answer: nothing there speaks ACP.
        let none_ready: DeviceRow = serde_json::from_value(json!({
            "id": "row-2",
            "agents": ["claude"],
            "acp_agents": [],
        }))
        .unwrap();
        assert!(none_ready.acp_agent_ids().is_empty());
        assert!(none_ready.agent_cannot_run_session("claude"));

        // NULL / absent / garbage: the ROW still parses (tolerant decoding),
        // and reads as "nothing here speaks ACP".
        for column in [json!(null), json!("not json"), json!(7)] {
            let row: DeviceRow = serde_json::from_value(json!({
                "id": "row-3",
                "agents": ["claude"],
                "acp_agents": column,
            }))
            .unwrap();
            assert!(row.acp_agent_ids().is_empty());
            assert!(row.agent_cannot_run_session("claude"));
        }
        let missing: DeviceRow = serde_json::from_value(json!({"id": "row-4"})).unwrap();
        assert!(missing.acp_agent_ids().is_empty());
        // Nothing runnable is declared either, so the note still says nothing.
        assert!(!missing.agent_cannot_run_session("claude"));
    }

    #[test]
    fn device_worktree_row_hydrates_its_flags() {
        let row: DeviceWorktreeRow = serde_json::from_value(json!({
            "id": "wt-1",
            "device_row_id": "row-1",
            "repo_full_name": "acme/web",
            "branch": "exp/EXP-7",
            "issue_identifier": "EXP-7",
            "agents": "[\"codex\"]",
            "dirty": "clean",
            "busy": "t",
        }))
        .unwrap();
        assert_eq!(row.busy, Some(true));
        assert!(row.agents.is_some());
        let unmarked: DeviceWorktreeRow =
            serde_json::from_value(json!({"id": "wt-2"})).unwrap();
        assert!(unmarked.agents.is_none());
    }

    /// NEW-2: the three new rows hydrate tolerantly (TEXT-stored jsonb and
    /// numerics, unknown columns ignored); a session's kind derives from
    /// `agent_id`, never the `kind` shim.
    #[test]
    fn new2_rows_hydrate_and_the_kind_derives_from_agent_id() {
        let agent: AgentDefRow = serde_json::from_value(json!({
            "id": "ag-1",
            "team_id": "t-1",
            "name": "Triage",
            "triggers": "[{\"id\":\"tr-1\",\"enabled\":true,\"kind\":\"schedule\",\"cron\":\"0 9 * * 1-5\"}]",
            "sort_order": "1",
            "some_future_column": 7,
        }))
        .unwrap();
        assert_eq!(agent.triggers.as_ref().unwrap()[0]["cron"], "0 9 * * 1-5");
        assert_eq!(agent.trigger_list().len(), 1);
        assert_eq!(agent.sort_order, Some(1.0));
        let event: EventRow = serde_json::from_value(json!({
            "id": "ev-1",
            "name": "issue.created",
            "payload": "{\"issueId\":\"i-1\"}",
            "emitted_by": "server",
        }))
        .unwrap();
        assert_eq!(event.payload.as_ref().unwrap()["issueId"], "i-1");
        let prompt: PromptRow =
            serde_json::from_value(json!({"id": "p-1", "name": "Groom", "body": "never synced"})).unwrap();
        assert_eq!(prompt.name.as_deref(), Some("Groom"));

        let conversation: SessionRow =
            serde_json::from_value(json!({"id": "s-1", "kind": "thread", "agent_id": "ag-1"})).unwrap();
        assert!(conversation.is_agent() && conversation.is_persistent());
        let legacy: SessionRow =
            serde_json::from_value(json!({"id": "s-2", "kind": "agent"})).unwrap();
        assert!(!legacy.is_agent() && !legacy.is_persistent());
        // NEW-8: every legacy column, present (today) or absent (after the
        // drop), hydrates the same row.
        let with: SessionRow = serde_json::from_value(json!({
            "id": "s-3", "kind": "thread", "dispatch": "t", "move_to_device_id": "d-2",
            "handoff_restored_at": "2026-10-01", "caption": "Editing", "aside": "f",
            "action_id": "a-1", "action_name": "Groom", "action_inputs": "{}",
            "spawned_by_session_id": "s-0", "name": "Triage", "icon": "bot",
            "color": "#ff0000", "triggers": "[]", "sort_order": "2"
        }))
        .unwrap();
        let without: SessionRow = serde_json::from_value(json!({"id": "s-3"})).unwrap();
        assert_eq!(with, without);
    }

    #[test]
    fn issue_status_row_hydrates_tolerantly() {
        // Wire numerics arrive as TEXT (§5.5), unknown categories survive, and
        // a custom row's absent builtin_key/color decode to None.
        let row: IssueStatusRow = serde_json::from_value(json!({
            "id": "s-1",
            "team_id": "t-1",
            "category": "started",
            "name": "Building",
            "sort_order": "2",
            "some_future_column": 7,
        }))
        .unwrap();
        assert_eq!(row.sort_order, Some(2.0));
        assert_eq!(row.builtin_key, None);
        assert_eq!(row.color, None);

        let row: IssueStatusRow = serde_json::from_value(json!({
            "id": "s-2",
            "team_id": "t-1",
            "category": "triaged",
            "name": "Triaged",
            "color": "#22c55e",
            "sort_order": serde_json::Value::Null,
            "builtin_key": serde_json::Value::Null,
        }))
        .unwrap();
        assert_eq!(row.category, "triaged");
        assert_eq!(row.sort_order, None);
        assert_eq!(row.color.as_deref(), Some("#22c55e"));
    }

    #[test]
    fn issue_hydrates_status_id_when_present() {
        let issue: Issue = serde_json::from_value(json!({
            "id": "i-1",
            "board_id": "b-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": "backlog",
            "status_id": "s-1",
        }))
        .unwrap();
        assert_eq!(issue.status_id.as_deref(), Some("s-1"));

        // Pre-backfill rows carry no column at all.
        let issue: Issue = serde_json::from_value(json!({
            "id": "i-2",
            "board_id": "b-1",
            "number": 2,
            "identifier": "EXP-2",
            "title": "t",
            "status": "backlog",
        }))
        .unwrap();
        assert_eq!(issue.status_id, None);
    }

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
            "board_id": "p-1",
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

    /// EXP-630: `estimate` arrives as a bare number from tRPC seeds and as
    /// TEXT from Electric; a pre-column row simply has none.
    #[test]
    fn issue_estimate_hydrates_from_number_or_string_and_defaults_to_none() {
        let base = json!({
            "id": "i-1",
            "board_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "Sized",
            "status": "backlog"
        });
        let mut numeric = base.clone();
        numeric["estimate"] = json!(5);
        let issue: Issue = serde_json::from_value(numeric).expect("numeric estimate");
        assert_eq!(issue.estimate, Some(5));

        let mut text = base.clone();
        text["estimate"] = json!("8");
        let issue: Issue = serde_json::from_value(text).expect("text estimate");
        assert_eq!(issue.estimate, Some(8));

        let mut cleared = base.clone();
        cleared["estimate"] = json!(null);
        let issue: Issue = serde_json::from_value(cleared).expect("null estimate");
        assert_eq!(issue.estimate, None);

        let issue: Issue = serde_json::from_value(base).expect("pre-column row");
        assert_eq!(issue.estimate, None);
    }

    #[test]
    fn issue_with_unknown_enum_value_is_kept_not_dropped() {
        let issue: Issue = serde_json::from_value(json!({
            "id": "i-1",
            "board_id": "p-1",
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
    fn board_icon_hydrates_and_stray_public_columns_are_ignored() {
        // The glyph comes from `icon`; the dropped public-board columns
        // (`is_public`/`public_show_*`, like the older `type`) and the dropped
        // `is_protected` (EXP-364) no longer arrive — a stray one from a
        // pre-drop local table is simply ignored (row
        // structs carry no deny_unknown_fields).
        let board: Board = serde_json::from_value(json!({
            "id": "p-1",
            "team_id": "w-1",
            "name": "Exponential",
            "icon": "megaphone",
            "is_public": "t",
            "public_show_comments": "t",
            "public_show_activity": null,
            "is_protected": "t"
        }))
        .unwrap();
        assert_eq!(board.icon.as_deref(), Some("megaphone"));

        // A row missing `icon` degrades to None (attribute-derived fallback).
        let sparse: Board = serde_json::from_value(json!({
            "id": "p-3", "team_id": "w-1", "name": "Sparse"
        }))
        .unwrap();
        assert_eq!(sparse.icon, None);
    }

    #[test]
    fn team_tolerates_a_dropped_helpdesk_column() {
        // SLOP-4: `helpdesk_enabled` left the teams shape (the helpdesk is
        // gone). A pre-SLOP-4 store row still carries the orphaned column —
        // it must decode, the stale key simply ignored.
        let team: Team = serde_json::from_value(json!({
            "id": "w-1",
            "name": "Acme",
            "helpdesk_enabled": "t"
        }))
        .unwrap();
        assert_eq!(team.name, "Acme");
    }

    #[test]
    fn team_yolo_mode_hydrates_tolerantly() {
        // EXP-1105: SQLite TEXT store form ("t"/"f") plus the bare wire bool.
        let team: Team = serde_json::from_value(json!({
            "id": "w-1",
            "name": "Acme",
            "yolo_mode": "t"
        }))
        .unwrap();
        assert_eq!(team.yolo_mode, Some(true));
        assert!(team.yolo_mode());

        let team: Team = serde_json::from_value(json!({
            "id": "w-2",
            "name": "Beta",
            "yolo_mode": false
        }))
        .unwrap();
        assert_eq!(team.yolo_mode, Some(false));
        assert!(!team.yolo_mode());

        // Pre-column rows degrade to None (off), never a dropped row.
        let team: Team = serde_json::from_value(json!({
            "id": "w-3",
            "name": "Legacy"
        }))
        .unwrap();
        assert_eq!(team.yolo_mode, None);
        assert!(!team.yolo_mode());
        assert!(!Team::seeded("w-4", "Seeded", None).yolo_mode());
    }

    #[test]
    fn team_pr_automation_hydrates_tolerantly() {
        // EXP-319: TEXT-store bool forms plus the status-id pins. A strict
        // bool would drop the whole Team row on hydration — far worse than
        // the automation card breaking.
        let team: Team = serde_json::from_value(json!({
            "id": "w-1",
            "name": "Acme",
            "pr_opened_status_id": "s-1",
            "pr_opened_automation": "t",
            "pr_merged_status_id": null,
            "pr_merged_automation": "f",
            "end_sessions_on_merge": "f"
        }))
        .unwrap();
        assert_eq!(team.pr_opened_status_id.as_deref(), Some("s-1"));
        assert_eq!(team.pr_opened_automation, Some(true));
        assert_eq!(team.pr_merged_status_id, None);
        assert_eq!(team.pr_merged_automation, Some(false));
        // EXP-711: an explicit `f` keeps sessions running.
        assert_eq!(team.end_sessions_on_merge, Some(false));
        assert!(!team.ends_sessions_on_merge());

        // Pre-column rows degrade to None everywhere (None flag = ENABLED,
        // matching the server DEFAULT true).
        let team: Team = serde_json::from_value(json!({
            "id": "w-2",
            "name": "Legacy"
        }))
        .unwrap();
        assert_eq!(team.pr_opened_status_id, None);
        assert_eq!(team.pr_opened_automation, None);
        assert_eq!(team.pr_merged_status_id, None);
        assert_eq!(team.pr_merged_automation, None);
        assert_eq!(team.end_sessions_on_merge, None);
        assert!(team.ends_sessions_on_merge());
        // EXP-630: no column, an unknown scale and an explicit none all read
        // as off; a known scale reads as itself.
        assert_eq!(team.estimation(), "none");
        let mut scaled = team.clone();
        scaled.estimation_type = Some("tshirt".to_string());
        assert_eq!(scaled.estimation(), "tshirt");
        scaled.estimation_type = Some("hexagonal".to_string());
        assert_eq!(scaled.estimation(), "none");
    }

    #[test]
    fn notification_team_id_hydrates_and_degrades_to_none() {
        // EXP-801: an issue-less agent_message row carries its team.
        let n: Notification = serde_json::from_value(json!({
            "id": "n-1",
            "user_id": "u-1",
            "issue_id": null,
            "team_id": "w-1",
            "type": "agent_message",
            "title": "Your agent",
            "body": "Done with the batch."
        }))
        .unwrap();
        assert_eq!(n.issue_id, None);
        assert_eq!(n.team_id.as_deref(), Some("w-1"));
        assert_eq!(n.kind.as_deref(), Some("agent_message"));

        // Issue-anchored / pre-column rows degrade to None, never a drop.
        let n: Notification = serde_json::from_value(json!({
            "id": "n-2",
            "user_id": "u-1",
            "issue_id": "i-1",
            "type": "issue_assigned"
        }))
        .unwrap();
        assert_eq!(n.team_id, None);
        assert_eq!(n.session_id, None);
    }

    #[test]
    fn notification_session_id_hydrates_and_degrades_to_none() {
        // EXP-980: an issue-less session_blocked row names the run it is
        // about; the team rides along so the inbox can switch to it.
        let n: Notification = serde_json::from_value(json!({
            "id": "n-1",
            "user_id": "u-1",
            "issue_id": null,
            "team_id": "w-1",
            "session_id": "s-1",
            "type": "session_blocked",
            "title": "EXP-12 hit a rate limit",
            "body": "Rate limited · resets in 2h"
        }))
        .unwrap();
        assert_eq!(n.session_id.as_deref(), Some("s-1"));
        assert_eq!(n.kind.as_deref(), Some("session_blocked"));

        // A pruned run (or a pre-column row) degrades to None, never a drop.
        let n: Notification = serde_json::from_value(json!({
            "id": "n-2",
            "user_id": "u-1",
            "issue_id": null,
            "team_id": "w-1",
            "session_id": null,
            "type": "session_blocked"
        }))
        .unwrap();
        assert_eq!(n.session_id, None);
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

    /// EXP-1294: a session row hydrates from TEXT cells (the store's form):
    /// bools in every tolerant form, absent/null bools read false, jsonb
    /// re-parses, numbers coerce.
    #[test]
    fn session_row_hydrates_tolerantly() {
        let row: SessionRow = serde_json::from_value(json!({
            "id": "s-1",
            "team_id": "t-1",
            "parent_id": null,
            "root_id": "s-1",
            "depth": "0",
            "kind": "thread",
            "status": "running",
            "busy": "t",
            "needs_input": "f",
            "shared": "true",
            "plan_mode": null,
            "blocked": "{\"window\":\"weekly\"}",
            "guide": "[{\"topic\":\"Summary\",\"text\":\"Done.\"}]",
            "some_future_column": "x"
        }))
        .unwrap();
        assert!(row.is_root());
        assert!(!row.is_agent());
        assert!(row.busy && row.shared && !row.needs_input && !row.plan_mode);
        assert!(!row.computer_use, "absent reads false");
        assert_eq!(row.depth, Some(0));
        assert_eq!(row.blocked.as_ref().unwrap()["window"], "weekly");
        assert!(row.guide.as_ref().unwrap().is_array());
        assert_eq!(row.root_id_or_self(), "s-1");

        let child: SessionRow = serde_json::from_value(json!({
            "id": "c-1",
            "parent_id": "s-1",
            "root_id": "s-1",
            "kind": "thread",
            "status": "ended",
            "busy": "garbage"
        }))
        .unwrap();
        assert!(!child.is_root());
        assert!(child.is_ended());
        assert!(!child.busy, "unparsable reads false");
        assert_eq!(child.root_id_or_self(), "s-1");

        let agent: SessionRow = serde_json::from_value(json!({
            "id": "a-1",
            "kind": "agent",
            "agent_id": "ag-1",
            "name": "Triage",
            "triggers": "[{\"kind\":\"schedule\"}]"
        }))
        .unwrap();
        assert!(agent.is_agent());
        assert_eq!(agent.root_id_or_self(), "a-1", "a partial row is its own root");
    }

    /// EXP-1294: messages read their card kind and queued state; PR rows
    /// their state.
    #[test]
    fn message_and_pull_request_rows_hydrate() {
        let queued: MessageRow = serde_json::from_value(json!({
            "id": "m-1",
            "conversation_id": "c-1",
            "session_id": "s-1",
            "author_kind": "user",
            "body": "Fix it",
            "consumed_at": null
        }))
        .unwrap();
        assert!(queued.is_queued());
        assert_eq!(queued.card_kind(), None);
        // NEW-8: the legacy column is never decoded; the conversation decides.
        assert_eq!(queued.conversation_id, "c-1");
        assert_eq!(queued.session_id, "");
        let card: MessageRow = serde_json::from_value(json!({
            "id": "m-2",
            "conversation_id": "c-1",
            "author_kind": "agent",
            "card": "{\"kind\":\"question\",\"question\":\"Ship?\"}"
        }))
        .unwrap();
        assert_eq!(card.card_kind(), Some("question"));
        assert!(!card.is_queued());
        assert_eq!(card.body_text(), "");
        // NEW-8: a row without its conversation never hydrates.
        assert!(serde_json::from_value::<MessageRow>(json!({"id": "m-3", "session_id": "s-1"})).is_err());
        let conversation: ConversationRow = serde_json::from_value(json!({
            "id": "c-1",
            "kind": "session",
            "session_id": "s-1"
        }))
        .unwrap();
        assert_eq!(conversation.session_id.as_deref(), Some("s-1"));

        let pr: PullRequestRow = serde_json::from_value(json!({
            "id": "pr-1",
            "number": "45",
            "state": "merged",
            "head": "exp/EXP-1",
            "base": "master"
        }))
        .unwrap();
        assert_eq!(pr.number, Some(45));
        assert!(pr.is_merged() && !pr.is_open());

        let link: SessionIssueRow = serde_json::from_value(json!({
            "id": "si-1",
            "session_id": "s-1",
            "issue_id": "i-1",
            "sort_order": 1
        }))
        .unwrap();
        assert_eq!(link.sort_order, Some(1.0));
        let pr_link: PullRequestIssueRow = serde_json::from_value(json!({
            "id": "pi-1",
            "pull_request_id": "pr-1",
            "issue_id": "i-1"
        }))
        .unwrap();
        assert_eq!(pr_link.issue_id, "i-1");
    }
}
