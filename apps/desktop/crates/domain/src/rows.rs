//! Typed row structs for the 16 synced shapes (masterplan-v3 §5.1/§5.5) —
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

use serde::Deserialize;

use crate::enums::{IssuePriority, IssueStatus};
use crate::hydrate::{
    tolerant_i64, tolerant_opt_bool, tolerant_opt_f64, tolerant_opt_i64, tolerant_opt_json,
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
    /// EXP-180 helpdesk gate: `Some(true)` unlocks the team's Support inbox
    /// (standalone support tickets, every member handles them). `None` on
    /// rows synced before the column existed — treated as disabled.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub helpdesk_enabled: Option<bool>,
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
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Team {
    /// Optimistic local row built from a mutation's own response (EXP-470) —
    /// identity fields only; everything else stays `None` (degrading like a
    /// pre-column row) until the Electric echo overwrites it.
    pub fn seeded(id: impl Into<String>, name: impl Into<String>, slug: Option<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            slug,
            icon_url: None,
            helpdesk_enabled: None,
            pr_opened_status_id: None,
            pr_opened_automation: None,
            pr_merged_status_id: None,
            pr_merged_automation: None,
            end_sessions_on_merge: None,
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
    #[serde(default)]
    pub creator_id: Option<String>,
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
    pub team_id: Option<String>,
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
    pub team_id: Option<String>,
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
    /// EXP-180: set on issue-less `support_reply` rows (the ticket's team) so
    /// the inbox can group helpdesk activity per team; NULL on issue-anchored
    /// rows (their team resolves via the issue) and on pre-column rows.
    #[serde(default)]
    pub team_id: Option<String>,
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

/// `coding_sessions` shape row (the cross-client "coding now" badge).
/// `issue_id` is NULL on batch-scoped (multi-issue) session rows — those
/// carry only `team_id` (enforced by the tRPC writer).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CodingSession {
    pub id: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    /// EXP-549/550: the host machine's steer deviceId — join the synced
    /// `devices` row for the machine's LIVE label (the user's rename) and
    /// its online-ness (an offline host means the session is paused, not
    /// live). `None` on rows written by pre-EXP-549 clients.
    #[serde(default)]
    pub device_id: Option<String>,
    /// `running` / `in_review` / `ended` — raw wire value (contract-locked).
    #[serde(default)]
    pub status: Option<String>,
    /// EXP-484: the agent CLI running it (`claude`/`codex`/`pi`) — raw wire
    /// value; `None` on rows written before the column existed.
    #[serde(default)]
    pub agent: Option<String>,
    /// Desktop-written attention flag (EXP-214): the agent is parked on a
    /// plan-approval / AskUserQuestion picker and waits for a human.
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub needs_input: Option<bool>,
    /// Action-run scoping (EXP-253/EXP-530): the `actions` row id plus its
    /// name SNAPSHOT (survives the action's deletion); `None` = issue/batch.
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub action_name: Option<String>,
    /// EXP-583: the `automations` row that fired this run; `None` on manual
    /// runs (and on rows written before automations became their own entity).
    #[serde(default)]
    pub automation_id: Option<String>,
    /// `schedule` / `event` on automation-started runs (EXP-530) — raw wire
    /// value; `None` = a person started it.
    #[serde(default)]
    pub started_reason: Option<String>,
    /// EXP-637 close-out: the agent's own one-paragraph summary, written by
    /// the `exponential_sessions_end` MCP tool (≤4000 chars, plain GFM).
    #[serde(default)]
    pub summary: Option<String>,
    /// EXP-637: who ended the run — `agent` / `user` / `client` / `merge` /
    /// `system` (`CODING_SESSION_ENDED_BY_VALUES`).
    #[serde(default)]
    pub ended_by: Option<String>,
    /// EXP-637: the ended session this run was resumed from; `None` on a
    /// fresh run.
    #[serde(default)]
    pub resumed_from_id: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `actions` shape row (EXP-268) — the body-less list projection: the ≤64KB
/// prompt `body` is excluded from sync server-side (runs/editors fetch it
/// fresh via tRPC `actions.get`), so no local field may exist to hold a
/// stale copy. EXP-583 dropped `trigger`: automations are their own row
/// now, and serde is non-strict so a stale synced column is ignored.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ActionRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// EXP-273: curated registry icon name; `None` = the generic action glyph.
    #[serde(default)]
    pub icon: Option<String>,
    /// jsonb `ActionInputDef[]` — TEXT-stored (§5.5), re-parsed at hydrate
    /// like `issue_events.payload`.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub inputs: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `automations` shape row (EXP-583) — the 19th shape: ONE action + ONE
/// device + ONE trigger, split out of the old `actions.trigger` column
/// (EXP-530). `trigger` is the WHEN-part only; the runner binding
/// (`device_id`) and the per-run overrides (`agent`/`model`/`effort`, NULL =
/// the device's launch defaults) are columns of their own.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AutomationRow {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    /// FK to the `actions` row this fires.
    #[serde(default)]
    pub action_id: Option<String>,
    /// The steer TEXT device id (`devices.device_id`, NOT a row uuid) whose
    /// host evaluates and fires this automation — every other device ignores
    /// the row.
    #[serde(default)]
    pub device_id: Option<String>,
    /// Server default TRUE; a missing value reads as enabled
    /// ([`AutomationRow::is_enabled`]).
    #[serde(default, deserialize_with = "tolerant_opt_bool")]
    pub enabled: Option<bool>,
    /// jsonb `AutomationTrigger` — TEXT-stored (§5.5), re-parsed at hydrate
    /// like `actions.inputs`. The `coding::automations` engine owns the
    /// tolerant parse.
    #[serde(default, deserialize_with = "tolerant_opt_json")]
    pub trigger: Option<serde_json::Value>,
    /// Pinned launch overrides; `None` = the device's own launch defaults.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default, deserialize_with = "tolerant_opt_f64")]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl AutomationRow {
    /// A row with no synced flag is ON — the column is NOT NULL DEFAULT true
    /// server-side, so an absent value can only be an old/partial hydrate.
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
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
    #[serde(default, deserialize_with = "tolerant_opt_i64")]
    pub active_sessions: Option<i64>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    #[serde(default)]
    pub shared_team_id: Option<String>,
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

    pub fn cap_ids(&self) -> Vec<String> {
        Self::string_list(&self.caps)
    }

    pub fn is_server(&self) -> bool {
        self.kind.as_deref() == Some("server")
    }

    /// EXP-484: this machine's account row for `agent` (`claude`/`codex`/
    /// `pi`), still as the raw wire object — `domain` deliberately does not
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
/// The scoping mirrors (`user_id`/`shared_team_id`) are proxy-excluded and
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

impl DeviceWorktreeRow {
    /// The marker's agent ids; `None` = pre-marker worktree (any agent).
    pub fn agent_ids(&self) -> Option<Vec<String>> {
        self.agents.as_ref().and_then(|v| v.as_array()).map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
    }

    /// Whether `agent` may resume in this worktree (marker absent = any).
    pub fn offers_agent(&self, agent: &str) -> bool {
        match self.agent_ids() {
            None => true,
            Some(ids) => ids.iter().any(|id| id == agent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        // A minimal row (older server) still hydrates.
        let narrow: DeviceRow = serde_json::from_value(json!({"id": "row-2"})).unwrap();
        assert!(!narrow.is_server());
        assert!(narrow.agent_ids().is_empty());
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
        assert_eq!(row.agent_account("pi"), None);
        assert_eq!(
            row.agent_usage_for("claude").unwrap()["windows"][0]["percent"],
            42
        );
        assert_eq!(row.agent_usage_for("codex"), None);
        assert_eq!(row.agent_usage_at.as_deref(), Some("2026-08-28T10:00:00.000Z"));

        // A row from an older server (or a garbage column) reads as nothing
        // to render, never a dropped row.
        let narrow: DeviceRow = serde_json::from_value(json!({
            "id": "row-2",
            "agent_accounts": "not json",
        }))
        .unwrap();
        assert_eq!(narrow.agent_account("claude"), None);
        assert_eq!(narrow.agent_usage_for("claude"), None);
    }

    #[test]
    fn device_worktree_row_offers_agents_via_the_marker() {
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
        assert!(row.offers_agent("codex"));
        assert!(!row.offers_agent("claude"));
        assert_eq!(row.busy, Some(true));
        // No marker = any agent may resume (pre-marker worktree).
        let unmarked: DeviceWorktreeRow =
            serde_json::from_value(json!({"id": "wt-2"})).unwrap();
        assert!(unmarked.offers_agent("claude"));
        assert_eq!(unmarked.agent_ids(), None);
    }

    #[test]
    fn coding_session_hydrates_action_attribution() {
        // EXP-530: TEXT-stored wire forms; pre-column rows degrade to None
        // (a manual run), never a dropped row.
        let session: CodingSession = serde_json::from_value(json!({
            "id": "cs-1",
            "team_id": "t-1",
            "status": "running",
            "action_id": "act-1",
            "action_name": "Daily standup digest",
            "started_reason": "schedule",
        }))
        .unwrap();
        assert_eq!(session.action_id.as_deref(), Some("act-1"));
        assert_eq!(session.action_name.as_deref(), Some("Daily standup digest"));
        assert_eq!(session.started_reason.as_deref(), Some("schedule"));
        // EXP-583: the firing automation rides along too.
        let automated: CodingSession = serde_json::from_value(json!({
            "id": "cs-3",
            "status": "running",
            "action_id": "act-1",
            "automation_id": "auto-1",
            "started_reason": "event",
        }))
        .unwrap();
        assert_eq!(automated.automation_id.as_deref(), Some("auto-1"));

        let manual: CodingSession = serde_json::from_value(json!({
            "id": "cs-2",
            "status": "running",
        }))
        .unwrap();
        assert_eq!(manual.action_id, None);
        assert_eq!(manual.action_name, None);
        assert_eq!(manual.started_reason, None);
        assert_eq!(manual.automation_id, None);
    }

    #[test]
    fn automation_row_hydrates_the_trigger_like_inputs() {
        // EXP-583: the trigger arrives TEXT-stored (§5.5) — the string form
        // must re-parse into structured JSON, like `actions.inputs`.
        let row: AutomationRow = serde_json::from_value(json!({
            "id": "auto-1",
            "team_id": "t-1",
            "action_id": "act-1",
            "device_id": "dev-1",
            "enabled": "t",
            "trigger": "{\"kind\":\"schedule\",\"interval\":\"daily\"}",
            "agent": "codex",
            "sort_order": "1.5",
        }))
        .unwrap();
        assert_eq!(row.trigger.as_ref().unwrap()["kind"], "schedule");
        assert_eq!(row.trigger.as_ref().unwrap()["interval"], "daily");
        assert!(row.is_enabled());
        assert_eq!(row.agent.as_deref(), Some("codex"));
        // NULL agent/model/effort = the device's launch defaults.
        assert_eq!(row.model, None);
        assert_eq!(row.sort_order, Some(1.5));

        // A narrow row (older/partial hydrate) still decodes, and an absent
        // `enabled` reads as ON — the column is NOT NULL DEFAULT true.
        let narrow: AutomationRow = serde_json::from_value(json!({"id": "auto-2"})).unwrap();
        assert!(narrow.is_enabled());
        assert_eq!(narrow.trigger, None);

        // EXP-583: a stale synced `trigger` column on an actions row is
        // IGNORED (serde is non-strict), never a dropped row.
        let action: ActionRow = serde_json::from_value(json!({
            "id": "act-1",
            "team_id": "t-1",
            "name": "Groom",
            "trigger": "{\"kind\":\"schedule\"}",
        }))
        .unwrap();
        assert_eq!(action.name.as_deref(), Some("Groom"));
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
        // `is_protected` (EXP-364) no longer arrive — a stray one from an
        // older server or a pre-drop local table is simply ignored (row
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
    fn team_helpdesk_enabled_hydrates_tolerantly() {
        // SQLite TEXT store form ("t"/"f") — the tolerant opt-bool path.
        let team: Team = serde_json::from_value(json!({
            "id": "w-1",
            "name": "Acme",
            "helpdesk_enabled": "t"
        }))
        .unwrap();
        assert_eq!(team.helpdesk_enabled, Some(true));

        // Bare wire bool works too.
        let team: Team = serde_json::from_value(json!({
            "id": "w-2",
            "name": "Beta",
            "helpdesk_enabled": false
        }))
        .unwrap();
        assert_eq!(team.helpdesk_enabled, Some(false));

        // Pre-column rows degrade to None (disabled), never a dropped row.
        let team: Team = serde_json::from_value(json!({
            "id": "w-3",
            "name": "Legacy"
        }))
        .unwrap();
        assert_eq!(team.helpdesk_enabled, None);
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
    }

    #[test]
    fn notification_team_id_hydrates_and_degrades_to_none() {
        // EXP-180: an issue-less support_reply row carries the ticket's team.
        let n: Notification = serde_json::from_value(json!({
            "id": "n-1",
            "user_id": "u-1",
            "issue_id": null,
            "team_id": "w-1",
            "type": "support_reply",
            "title": "Reporter replied",
            "body": "Thanks, that fixed it!"
        }))
        .unwrap();
        assert_eq!(n.issue_id, None);
        assert_eq!(n.team_id.as_deref(), Some("w-1"));
        assert_eq!(n.kind.as_deref(), Some("support_reply"));

        // Issue-anchored / pre-column rows degrade to None, never a drop.
        let n: Notification = serde_json::from_value(json!({
            "id": "n-2",
            "user_id": "u-1",
            "issue_id": "i-1",
            "type": "issue_assigned"
        }))
        .unwrap();
        assert_eq!(n.team_id, None);
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
