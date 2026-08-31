//! Derived read queries over the §05 collections (masterplan-v3 §4.1: "Keep
//! query helpers in `ui/src/queries.rs`, one function per web hook — do not
//! scatter filtering logic into views"), plus the shared tRPC-client
//! accessor mutations go through.
//!
//! Queries are plain Rust over the in-memory collections — no query engine,
//! no SQL at render time. Grouping/sorting semantics live in `domain::board`
//! (the verbatim `board-view.ts` port); this module only joins collections.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui::App;
use sync::Store;

use domain::board::{build_filtered_issues, build_status_groups};
use domain::filters::IssueFilters;
use domain::rows::{Issue, IssueStatusRow, Label};
use domain::statuses::{resolve_status_sorted, sort_team_statuses, ResolvedStatus};

use crate::session::AuthContext;

/// The board data one render needs (mirror of the web's
/// `use-board-view-data.ts` return, §4.1).
pub struct BoardData {
    /// Ready = every shape the query reads has seen its first `up-to-date`
    /// (§4.1 `is_ready`: skeleton while false, real empty-state only when
    /// true — the never-flash-empty rule).
    pub is_ready: bool,
    /// Whether the unfiltered scope has ANY issue (distinguishes "no issues
    /// yet" from "filters hide everything", web `hasAnyIssues`).
    pub has_any_issues: bool,
    /// Status groups in display order, empty groups hidden (web parity).
    pub groups: Vec<BoardGroup>,
    /// issue id → its labels (for the row label chips), shared behind `Rc`
    /// so per-frame row building clones a handle, not the resolved vec.
    pub labels_by_issue: HashMap<String, Rc<Vec<Label>>>,
}

/// One status group as the views consume it — [`domain::board::IssueGroup`]
/// with every issue behind an `Rc` (REV-39): an [`Issue`] carries the full
/// markdown description, and the list re-derives its rows on every render, so
/// group members must be clonable as handles, never as row payloads.
pub struct BoardGroup {
    pub status: ResolvedStatus,
    pub issues: Vec<Rc<Issue>>,
}

impl BoardData {
    /// [`domain::board::flatten_group_issue_ids`] over the `Rc`'d groups: the
    /// visible top-to-bottom issue order (the EXP-48 prev/next switcher's
    /// read).
    pub fn flatten_issue_ids(&self) -> Vec<String> {
        self.groups
            .iter()
            .flat_map(|group| group.issues.iter().map(|issue| issue.id.clone()))
            .collect()
    }
}

/// `use-board-view-data.ts`: one board's issues, filtered + grouped.
pub fn board_board(cx: &App, board_id: &str, filters: &IssueFilters) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues = collections.issues_in_board(board_id, cx);
    let team_id = collections
        .boards
        .read(cx)
        .get(board_id)
        .map(|board| board.team_id.clone());
    board_data_from(cx, issues, team_id.as_deref(), filters)
}

/// `use-my-issues-data.ts`: the team's issues assigned to me, filtered +
/// grouped like a board.
pub fn my_issues(
    cx: &App,
    team_id: &str,
    user_id: &str,
    filters: &IssueFilters,
) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues: Vec<_> = collections
        .issues_in_team(team_id, cx)
        .into_iter()
        .filter(|issue| issue.assignee_id.as_deref() == Some(user_id))
        .collect();
    board_data_from(cx, issues, Some(team_id), filters)
}

fn board_data_from(
    cx: &App,
    issues: Vec<Issue>,
    team_id: Option<&str>,
    filters: &IssueFilters,
) -> BoardData {
    let collections = Store::global(cx).collections();
    let is_ready = collections.issues.read(cx).is_ready()
        && collections.boards.read(cx).is_ready()
        && collections.issue_labels.read(cx).is_ready()
        && collections.labels.read(cx).is_ready();
    // EXP-314 deliberately does NOT gate readiness on the `issue_statuses`
    // shape: a board rendered before it lands groups by the CONSTRUCTED
    // builtin defaults, which look identical, and re-keys once the rows
    // arrive. Gating would hang the whole list on a permanent skeleton
    // against a server that does not serve the shape at all.

    // REV-39: the issue_labels shape spans every member team — build the
    // label-ids map scoped to THIS scope's issues instead of cloning the
    // whole collection into a Vec first (web buildIssueLabelIdsMap over the
    // scoped links).
    let label_ids_by_issue = {
        let issue_ids: HashSet<&str> = issues.iter().map(|issue| issue.id.as_str()).collect();
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for link in collections.issue_labels.read(cx).iter() {
            if issue_ids.contains(link.issue_id.as_str()) {
                map.entry(link.issue_id.clone())
                    .or_default()
                    .push(link.label_id.clone());
            }
        }
        map
    };

    // EXP-314: the board groups by the team's OWN status rows.
    let status_rows = team_id.map(|id| team_statuses(cx, id)).unwrap_or_default();

    let has_any_issues = !issues.is_empty();
    let filtered = build_filtered_issues(issues, &label_ids_by_issue, &status_rows, filters);
    let today = today_local();
    let groups = build_status_groups(filtered, &status_rows, &filters.status_keys, &today);

    // Resolve label rows for the chips (web buildIssueLabelMap: unknown label
    // ids are skipped — referential integrity is a query-time concern, §5.4).
    let labels = collections.labels.read(cx);
    let mut labels_by_issue: HashMap<String, Rc<Vec<Label>>> = HashMap::new();
    for group in &groups {
        for issue in &group.issues {
            let Some(ids) = label_ids_by_issue.get(&issue.id) else {
                continue;
            };
            let resolved: Vec<Label> = ids
                .iter()
                .filter_map(|id| labels.get(id).cloned())
                .collect();
            if !resolved.is_empty() {
                labels_by_issue.insert(issue.id.clone(), Rc::new(resolved));
            }
        }
    }

    // The grouped issues move behind `Rc` handles (one allocation per issue,
    // no row payload copies) — the list's rows clone these per frame.
    let groups = groups
        .into_iter()
        .map(|group| BoardGroup {
            status: group.status,
            issues: group.issues.into_iter().map(Rc::new).collect(),
        })
        .collect();

    BoardData {
        is_ready,
        has_any_issues,
        groups,
        labels_by_issue,
    }
}

/// Today as `YYYY-MM-DD` for the overdue boundary. Device-LOCAL date — the
/// EXP-38 boundary every client uses: web `formatDateForMutation(new Date())`,
/// iOS `Calendar.current`, Android `LocalDate.now()`.
pub fn today_local() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// The signed-in account (per the §5 session machine) — `None` unless Synced.
pub fn active_account(cx: &App) -> Option<api::Account> {
    let account_id = Store::global(cx).session(cx).account_id()?.to_string();
    cx.try_global::<AuthContext>()?
        .auth
        .account(&account_id)
}

/// The active account's synced `users` row — the profile `image` lives only
/// there (accounts.json carries name/email but never the avatar URL). `None`
/// until the users shape has landed the caller's own row.
pub(crate) fn active_user(cx: &App) -> Option<domain::rows::User> {
    let account = active_account(cx)?;
    Store::global(cx)
        .collections()
        .users
        .read(cx)
        .get(&account.user_id)
        .cloned()
}

/// A tRPC client bound to the active account (call-time token provider, §5.7).
/// Build per mutation — cheap (an `Agent` + two `Arc`s), and always pointed at
/// the CURRENT account even across re-login.
pub fn trpc_client(cx: &App) -> Option<api::TrpcClient> {
    let auth = cx.try_global::<AuthContext>()?;
    let account = active_account(cx)?;
    let provider: Arc<dyn api::TokenProvider> = auth.auth.token_provider(&account.id);
    Some(api::TrpcClient::new(&account.instance_url, provider))
}

/// The auth-gated attachment transport bound to the active account (§4.5's
/// single image path: upload for the editors, bearer-fetched bytes for
/// rendering `/api/attachments/{id}`). `None` unless signed in.
pub(crate) fn attachment_transport(
    cx: &App,
) -> Option<Arc<dyn crate::markdown::AttachmentTransport>> {
    let auth = cx.try_global::<AuthContext>()?;
    let account = active_account(cx)?;
    let provider: Arc<dyn api::TokenProvider> = auth.auth.token_provider(&account.id);
    Some(Arc::new(crate::markdown::HttpAttachmentTransport::new(
        &account.instance_url,
        provider,
    )))
}

/// Resolve a relative `/api/...` URL (the canonical stored form of
/// attachment URLs) against the active account's instance base — the same
/// base `HttpAttachmentTransport` fetches through. Absolute URLs pass
/// through; `None` when signed out or the URL is unopenable (e.g. a
/// create-dialog `draft://` staging URL).
pub(crate) fn absolute_api_url(cx: &App, url: &str) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(url.to_string());
    }
    if !url.starts_with('/') {
        return None;
    }
    let account = active_account(cx)?;
    Some(format!("{}{url}", account.instance_url.trim_end_matches('/')))
}

/// An issue's team (issue → board → `team_id`) — the scoping
/// join the editors/autocomplete need (§4.6). `None` while the chain has not
/// synced.
pub(crate) fn issue_team_id(cx: &App, issue_id: &str) -> Option<String> {
    let collections = Store::global(cx).collections();
    let board_id = collections
        .issues
        .read(cx)
        .get(issue_id)
        .map(|issue| issue.board_id.clone())?;
    collections
        .boards
        .read(cx)
        .get(&board_id)
        .map(|board| board.team_id.clone())
}

/// `use-team-data.ts` `useTeamUsers`: `team_members` ⨝ `users`
/// (name-sorted for deterministic pickers), matching the web's `people`
/// filter (EXP-50 alignment: this query and the properties panel's member
/// read share the rule).
pub fn team_users(cx: &App, team_id: &str) -> Vec<domain::rows::User> {
    let collections = Store::global(cx).collections();
    let members = collections.team_members.read(cx);
    let member_ids: std::collections::HashSet<&str> = members
        .iter()
        .filter(|member| member.team_id == team_id)
        .map(|member| member.user_id.as_str())
        .collect();
    let mut out: Vec<domain::rows::User> = collections
        .users
        .read(cx)
        .iter()
        .filter(|user| member_ids.contains(user.id.as_str()))
        .cloned()
        .collect();
    out.sort_by_key(|user| {
        user.name
            .clone()
            .or_else(|| user.email.clone())
            .unwrap_or_default()
            .to_lowercase()
    });
    out
}

/// A team's actions from the synced `actions` shape (EXP-268): the local
/// builtins ("Create action", then "Fix merge conflicts" — EXP-259; neither is
/// a DB row, so the shape can't carry them) prepended, real rows sorted
/// `sortOrder` then `name` (collections hydrate unordered — the old server
/// ordering must be re-applied client-side). Second value = the shape reached
/// readiness (for the loading-vs-empty split).
pub fn team_actions(cx: &App, team_id: &str) -> (Vec<api::actions::Action>, bool) {
    let collections = Store::global(cx).collections();
    let collection = collections.actions.read(cx);
    let mut out: Vec<api::actions::Action> = collection
        .iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .map(api::actions::from_row)
        .collect();
    out.sort_by(|a, b| {
        a.sort_order
            .total_cmp(&b.sort_order)
            .then_with(|| a.name.cmp(&b.name))
    });
    out.insert(0, api::actions::builtin_fix_conflicts_action(team_id));
    out.insert(0, api::actions::builtin_create_action(team_id));
    (out, collection.is_ready())
}

/// EXP-583: a team's synced `automations` rows in the server's own list
/// order (`sortOrder`, then `createdAt`). The bool is the shape's readiness —
/// an empty list before it is "still syncing", never "no automations".
pub fn team_automations(cx: &App, team_id: &str) -> (Vec<api::automations::Automation>, bool) {
    let collections = Store::global(cx).collections();
    let collection = collections.automations.read(cx);
    let mut out: Vec<api::automations::Automation> = collection
        .iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .map(api::automations::from_row)
        .collect();
    out.sort_by(|a, b| {
        a.sort_order
            .total_cmp(&b.sort_order)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    (out, collection.is_ready())
}

/// EXP-314: a team's `issue_statuses` rows in the canonical order (category
/// display order, then `sort_order`, `created_at`, `id`). The ONE read every
/// status surface goes through — pass the result straight to
/// `domain::statuses::resolve_status_sorted` so clock positions agree.
pub fn team_statuses(cx: &App, team_id: &str) -> Vec<IssueStatusRow> {
    let collections = Store::global(cx).collections();
    let rows: Vec<IssueStatusRow> = collections
        .issue_statuses
        .read(cx)
        .iter()
        .filter(|row| row.team_id == team_id)
        .cloned()
        .collect();
    sort_team_statuses(&rows)
}

/// EXP-314: resolve a bare `(status_id, anchor)` pair — a row NOT locally
/// synced (server search hits, `status_changed` event payloads) — against a
/// team's sorted status rows: the exact row when the id resolves, else the
/// anchor's builtin row, else a constructed default. Shared by the search
/// sheet and the timeline (EXP-525).
pub(crate) fn resolve_status_ref(
    status_id: Option<&str>,
    anchor: domain::IssueStatus,
    sorted: &[IssueStatusRow],
) -> ResolvedStatus {
    if let Some(status_id) = status_id {
        if let Some(index) = sorted.iter().position(|row| row.id == status_id) {
            return domain::statuses::resolve_row(sorted, index);
        }
    }
    // Unknown forward-compat anchors normalize to backlog before the lookup,
    // so a hit still shows the team's real Backlog row.
    let anchor = domain::statuses::normalized_anchor(anchor);
    if let Some(wire) = anchor.as_wire() {
        if let Some(index) = sorted
            .iter()
            .position(|row| row.builtin_key.as_deref() == Some(wire))
        {
            return domain::statuses::resolve_row(sorted, index);
        }
    }
    domain::statuses::constructed_default(anchor)
}

/// A team's status vocabulary as the pickers/filters render it — the synced
/// rows, or the constructed `builtin:<key>` defaults while the shape has not
/// landed its first snapshot (so a picker is never empty).
pub fn team_status_options(cx: &App, team_id: &str) -> Vec<ResolvedStatus> {
    domain::statuses::team_resolved_statuses(&team_statuses(cx, team_id))
}

/// Resolve ONE issue's status against its board's team (the per-row read the
/// cross-team surfaces use — resolution is per-issue, only GROUPING is
/// team-scoped). Falls back to the constructed default when the board/team is
/// not synced yet.
pub fn resolve_issue_status(cx: &App, issue: &domain::rows::Issue) -> ResolvedStatus {
    let collections = Store::global(cx).collections();
    let team_id = collections
        .boards
        .read(cx)
        .get(&issue.board_id)
        .map(|board| board.team_id.clone());
    match team_id {
        Some(team_id) => resolve_status_sorted(issue, &team_statuses(cx, &team_id)),
        None => domain::statuses::constructed_default(issue.status),
    }
}

/// The label-picker read (`label-picker.tsx`): a team's labels,
/// sort-order sorted.
pub fn team_labels(cx: &App, team_id: &str) -> Vec<Label> {
    let collections = Store::global(cx).collections();
    let mut out: Vec<Label> = collections
        .labels
        .read(cx)
        .iter()
        .filter(|label| label.team_id == team_id)
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(f64::MAX)
            .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

// ---------------------------------------------------------------------------
// Inbox (mirror of `components/inbox/inbox-view.tsx` — §4.2)
// ---------------------------------------------------------------------------

/// One "For me" card: notifications grouped by issue, newest first.
pub struct InboxGroup {
    pub issue: domain::rows::Issue,
    /// Group items, newest first (web orders the feed by `created_at desc`
    /// before grouping).
    pub items: Vec<domain::rows::Notification>,
    pub unread: usize,
}

/// One synthetic Support card (EXP-180): issue-less `support_reply`
/// notifications, ONE group per ticket team. These synced rows are the
/// desktop's passive helpdesk signal (EXP-638 raises them as OS
/// notifications too) — dropping them made a reporter reply invisible
/// unless the Support tool happened to be open. Click marks the group read and opens the team's Support tool.
pub struct SupportInboxGroup {
    /// The ticket team — `None` for the ONE generic group collecting rows
    /// from before the synced `team_id` column existed plus rows whose team
    /// row hasn't synced (click falls back to the current team's Support).
    pub team_id: Option<String>,
    /// The synced team's name; `None` for the generic group (the row renders
    /// the plain "Support" label either way, web parity).
    pub team_name: Option<String>,
    /// Group items, newest first (like [`InboxGroup::items`]).
    pub items: Vec<domain::rows::Notification>,
    pub unread: usize,
}

/// One inbox card — an issue group or a synthetic Support group. Entries are
/// interleaved newest-first by their latest item (web `inbox-view.tsx` sorts
/// all groups together).
pub enum InboxEntry {
    Issue(InboxGroup),
    Support(SupportInboxGroup),
}

impl InboxEntry {
    pub fn unread(&self) -> usize {
        match self {
            InboxEntry::Issue(group) => group.unread,
            InboxEntry::Support(group) => group.unread,
        }
    }
}

/// The inbox read: is-ready gate + grouped notifications. The notifications
/// shape is already user-scoped server-side; like web, groups are NOT
/// team-filtered (the join to a synced issue+board — or, for Support groups,
/// nothing at all — is the only membership requirement).
pub struct InboxData {
    pub is_ready: bool,
    pub groups: Vec<InboxEntry>,
    /// Unread across ALL entries, Support groups included — the count the
    /// tool header/badge surfaces.
    pub total_unread: usize,
}

/// `inbox-view.tsx` grouping: notifications ⨝ issues ⨝ boards grouped by
/// issue, plus per-team Support groups for issue-less `support_reply` rows;
/// group order = newest first item.
pub fn inbox(cx: &App) -> InboxData {
    let collections = Store::global(cx).collections();
    let is_ready = collections.notifications.read(cx).is_ready()
        && collections.issues.read(cx).is_ready()
        && collections.boards.read(cx).is_ready()
        && collections.teams.read(cx).is_ready();

    let notifications: Vec<domain::rows::Notification> = collections
        .notifications
        .read(cx)
        .iter()
        .cloned()
        .collect();

    let issues = collections.issues.read(cx);
    let boards = collections.boards.read(cx);
    let teams = collections.teams.read(cx);

    let groups = build_inbox_entries(
        notifications,
        |issue_id| {
            // An issue joins only while it AND its board are synced.
            let issue = issues.get(issue_id)?;
            boards.get(&issue.board_id)?;
            Some(issue.clone())
        },
        |team_id| teams.get(team_id).map(|team| team.name.clone()),
    );
    let total_unread = groups.iter().map(|entry| entry.unread()).sum();

    InboxData {
        is_ready,
        groups,
        total_unread,
    }
}

/// Unread helpdesk activity in one team (EXP-182): issue-less `support_reply`
/// rows carry a synced team_id — the same rule the Support inbox groups use.
/// Lights the rail's Support badge.
pub fn support_unread(cx: &App, team_id: &str) -> bool {
    Store::global(cx)
        .collections()
        .notifications
        .read(cx)
        .iter()
        .any(|notification| {
            notification.kind.as_deref()
                == Some(domain::contract::NOTIFICATION_TYPE_SUPPORT_REPLY)
                && notification.issue_id.is_none()
                && notification.team_id.as_deref() == Some(team_id)
                && notification.read_at.is_none()
        })
}

/// EXP-699: the rail's Inbox dot — any unread notification the inbox can
/// render (issue-keyed rows need the issue AND its board synced, issue-less
/// rows count only as `support_reply`): the [`inbox`] renderability rule
/// without the grouping work.
pub fn inbox_unread(cx: &App) -> bool {
    let collections = Store::global(cx).collections();
    let issues = collections.issues.read(cx);
    let boards = collections.boards.read(cx);
    collections
        .notifications
        .read(cx)
        .iter()
        .any(|notification| {
            if notification.read_at.is_some() {
                return false;
            }
            match notification.issue_id.as_deref() {
                Some(issue_id) => issues
                    .get(issue_id)
                    .is_some_and(|issue| boards.get(&issue.board_id).is_some()),
                None => {
                    notification.kind.as_deref()
                        == Some(domain::contract::NOTIFICATION_TYPE_SUPPORT_REPLY)
                }
            }
        })
}

/// EXP-699: the rail's Devices dot — is any of MY coding sessions in this
/// team live right now, and does one of them wait on input (the amber
/// escalation)? Mirrors web's `useAgentsRunningCount` scoping: own sessions
/// only, active team, staleness-filtered.
#[derive(Default)]
pub(crate) struct AgentsRunning {
    pub running: bool,
    pub needs_input: bool,
}

pub(crate) fn agents_running(cx: &App, team_id: &str) -> AgentsRunning {
    let Some(account) = active_account(cx) else {
        return AgentsRunning::default();
    };
    let now = chrono::Utc::now().timestamp();
    let mut result = AgentsRunning::default();
    for session in Store::global(cx)
        .collections()
        .coding_sessions
        .read(cx)
        .iter()
    {
        if session.user_id.as_deref() != Some(account.user_id.as_str())
            || session.team_id.as_deref() != Some(team_id)
            || !coding_session_is_live(session, now)
        {
            continue;
        }
        result.running = true;
        if coding_session_display(session, None) == CodingSessionDisplay::NeedsInput {
            result.needs_input = true;
        }
    }
    result
}

/// The pure grouping core of [`inbox`]. `resolve_issue` returns the synced
/// issue (only while its board is synced too); `resolve_team` returns a
/// synced team's name — `None` collapses the row into the generic Support
/// group, exactly like web's `teamMap.get` miss.
fn build_inbox_entries(
    mut notifications: Vec<domain::rows::Notification>,
    resolve_issue: impl Fn(&str) -> Option<domain::rows::Issue>,
    resolve_team: impl Fn(&str) -> Option<String>,
) -> Vec<InboxEntry> {
    // Newest first (web `orderBy createdAt desc`); ISO strings from one
    // source compare lexicographically.
    notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    /// First-seen registry key: feed order is newest-first, so first-seen
    /// group order IS the web's sort-by-newest-item entry order.
    #[derive(Clone, PartialEq, Eq, Hash)]
    enum Key {
        Issue(String),
        Support(Option<String>),
    }

    let mut order: Vec<Key> = Vec::new();
    let mut by_issue: HashMap<String, InboxGroup> = HashMap::new();
    let mut by_support_team: HashMap<Option<String>, SupportInboxGroup> = HashMap::new();
    for notification in notifications {
        let unread = notification.read_at.is_none();
        let Some(issue_id) = notification.issue_id.clone() else {
            // Issue-less rows are the helpdesk fan-out (EXP-180); any other
            // issue-less kind is unknown-future and skipped.
            if notification.kind.as_deref()
                != Some(domain::contract::NOTIFICATION_TYPE_SUPPORT_REPLY)
            {
                continue;
            }
            let team_name = notification.team_id.as_deref().and_then(&resolve_team);
            // Unknown/NULL teams collapse into the ONE generic group.
            let key = if team_name.is_some() {
                notification.team_id.clone()
            } else {
                None
            };
            let group = by_support_team.entry(key.clone()).or_insert_with(|| {
                order.push(Key::Support(key.clone()));
                SupportInboxGroup {
                    team_id: key,
                    team_name,
                    items: Vec::new(),
                    unread: 0,
                }
            });
            if unread {
                group.unread += 1;
            }
            group.items.push(notification);
            continue;
        };
        let Some(issue) = resolve_issue(&issue_id) else {
            continue;
        };
        let group = by_issue.entry(issue_id.clone()).or_insert_with(|| {
            order.push(Key::Issue(issue_id.clone()));
            InboxGroup {
                issue,
                items: Vec::new(),
                unread: 0,
            }
        });
        if unread {
            group.unread += 1;
        }
        group.items.push(notification);
    }

    order
        .into_iter()
        .filter_map(|key| match key {
            Key::Issue(issue_id) => by_issue.remove(&issue_id).map(InboxEntry::Issue),
            Key::Support(team_id) => by_support_team
                .remove(&team_id)
                .map(InboxEntry::Support),
        })
        .collect()
}

/// Open pull requests: synced issues in this team with an open PR — a
/// query over `issues`, independent of notifications. Feeds the Reviews rail
/// badge and, grouped, the Reviews tool window.
pub fn review_issues(cx: &App, team_id: &str) -> Vec<domain::rows::Issue> {
    let collections = Store::global(cx).collections();
    let boards = collections.boards.read(cx);
    collections
        .issues
        .read(cx)
        .iter()
        .filter(|issue| {
            is_reviewable(issue)
                && boards
                    .get(&issue.board_id)
                    .is_some_and(|board| board.team_id == team_id)
        })
        .cloned()
        .collect()
}

/// The per-issue Reviews predicate: an OPEN pull request. A batch PR entry
/// groups every issue that shares the `pr_url` (mobile parity).
pub(crate) fn is_reviewable(issue: &domain::rows::Issue) -> bool {
    issue.pr_state.as_deref() == Some("open")
}

/// One Reviews entry: the issue(s) behind a single open PR. A plain
/// single-issue PR has one issue; a batch run (EXP-131) lands N issues on ONE
/// branch under ONE `pr_url`, so they collapse into a single entry. Issues are
/// newest first; [`representative`](Self::representative) (the first) carries
/// the shared `pr_number`/`branch` and is the merge/dismiss target.
pub struct ReviewEntry {
    pub issues: Vec<domain::rows::Issue>,
}

impl ReviewEntry {
    /// The representative issue — the one whose id drives row-click, merge and
    /// dismiss (the server acts on the ONE linked PR either way).
    pub fn representative(&self) -> &domain::rows::Issue {
        &self.issues[0]
    }

    /// A batch PR groups more than one issue.
    pub fn is_batch(&self) -> bool {
        self.issues.len() > 1
    }
}

/// One Reviews tool-window section: a board and its open-PR entries (the
/// desktop mirror of the web `use-reviews-data.ts` `ReviewGroup`).
pub struct ReviewGroup {
    pub board: domain::rows::Board,
    pub entries: Vec<ReviewEntry>,
}

/// The Reviews tool window read: [`review_issues`] collapsed to ONE entry per
/// PR (issues sharing a `pr_url` — a batch run — group together; issues with
/// no `pr_url` key on their own id), then grouped by board. Groups follow
/// board `sort_order` (name tiebreak, like the sidebars); entries are newest
/// first within a group — web parity.
pub fn review_groups(cx: &App, team_id: &str) -> Vec<ReviewGroup> {
    let open = review_issues(cx, team_id);
    let collections = Store::global(cx).collections();
    let boards = collections.boards.read(cx);

    // Collapse issues sharing a PR into one entry (fallback key = issue id when
    // `pr_url` is absent — a lone issue). Preserve first-seen order so the
    // in-entry newest-first sort below is deterministic.
    let mut by_pr: HashMap<String, Vec<domain::rows::Issue>> = HashMap::new();
    let mut pr_order: Vec<String> = Vec::new();
    for issue in open {
        let key = issue
            .pr_url
            .clone()
            .unwrap_or_else(|| issue.id.clone());
        let bucket = by_pr.entry(key.clone()).or_default();
        if bucket.is_empty() {
            pr_order.push(key);
        }
        bucket.push(issue);
    }

    // One entry per PR; issues newest first (ISO strings from one source
    // compare lexicographically, None last) so the representative is newest.
    let mut by_board: HashMap<String, Vec<ReviewEntry>> = HashMap::new();
    let mut board_order: Vec<String> = Vec::new();
    for key in pr_order {
        let mut issues = by_pr.remove(&key).unwrap_or_default();
        issues.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let board_id = issues[0].board_id.clone();
        let entries = by_board.entry(board_id.clone()).or_default();
        if entries.is_empty() {
            board_order.push(board_id);
        }
        entries.push(ReviewEntry { issues });
    }

    let mut groups: Vec<ReviewGroup> = board_order
        .into_iter()
        .filter_map(|board_id| {
            // The team filter in `review_issues` already proved the
            // board exists; the lookup only resolves the row.
            let board = boards.get(&board_id)?.clone();
            let mut entries = by_board.remove(&board_id).unwrap_or_default();
            // Newest entry first — by the representative's created_at.
            entries.sort_by(|a, b| {
                b.representative()
                    .created_at
                    .cmp(&a.representative().created_at)
            });
            Some(ReviewGroup { board, entries })
        })
        .collect();
    groups.sort_by(|a, b| {
        a.board
            .sort_order
            .unwrap_or(f64::MAX)
            .total_cmp(&b.board.sort_order.unwrap_or(f64::MAX))
            .then_with(|| {
                a.board
                    .name
                    .to_lowercase()
                    .cmp(&b.board.name.to_lowercase())
            })
    });
    groups
}

/// The Reviews tool window's unlinked-PR sections: keep only repos that have
/// open pulls (the server returns every team repo, unreachable ones with
/// an empty list — an empty section is noise, web parity).
pub fn visible_pull_repos(
    repos: &[api::repositories::OpenPullsRepo],
) -> Vec<api::repositories::OpenPullsRepo> {
    repos
        .iter()
        .filter(|repo| !repo.pulls.is_empty())
        .cloned()
        .collect()
}

/// Drop a pull from the fetched `repositories.openPulls` state after a
/// successful merge — the mutation has no Electric echo, so removal is local.
pub fn remove_merged_pull(
    repos: &mut [api::repositories::OpenPullsRepo],
    repository_id: &str,
    number: u64,
) {
    for repo in repos.iter_mut() {
        if repo.repository_id == repository_id {
            repo.pulls.retain(|pull| pull.number != number);
        }
    }
}

/// Every issue in a team (issues ⨝ boards, shared sort order) — the
/// add-issues picker's candidate pool (the dialog filters status/membership
/// on top).
pub fn team_issues(cx: &App, team_id: &str) -> Vec<domain::rows::Issue> {
    Store::global(cx)
        .collections()
        .issues_in_team(team_id, cx)
}

/// EXP-153: a `running` (or `in_review` — EXP-194: PR open, terminal still
/// alive) coding_sessions row renders as live only while its `updated_at`
/// (heartbeat-advanced) is inside the contract stale window — stale rows are
/// treated as ABSENT, mirroring the server sweep's DELETE (never as `ended`,
/// which is the kill-switch signal). Missing/unparseable `updated_at` → live
/// (fail-open: never hide a session the server still considers alive; the
/// sweep is the backstop). No re-render timer: gpui re-evaluates on every
/// notify, and this process is the heartbeat writer for its own sessions — a
/// phantom row from a crashed prior instance re-evaluates on the next render
/// regardless.
pub(crate) fn coding_session_is_live(
    session: &domain::rows::CodingSession,
    now_epoch: i64,
) -> bool {
    let live_status = matches!(
        session.status.as_deref(),
        Some(domain::contract::CODING_SESSION_STATUS_RUNNING)
            | Some(domain::contract::CODING_SESSION_STATUS_IN_REVIEW)
    );
    if !live_status {
        return false;
    }
    match session
        .updated_at
        .as_deref()
        .and_then(crate::comments::parse_epoch)
    {
        Some(seen) => now_epoch - seen < domain::contract::CODING_SESSION_STALE_MS / 1000,
        None => true,
    }
}

/// REV2-24: the device already coding `issue_id` according to the live SYNCED
/// rows, or `None` when the issue is free. The cross-device half of the
/// EXP-202 one-session-per-issue rule — `coding_flow::LocalSessions` knows
/// only this process, so every launch entry point (the dialog's blocker, the
/// relay's remote starts) must consult sync as well, or two agents end up
/// pushing to the same `exp/<ID>` branch from two machines.
pub(crate) fn live_session_device_for_issue(
    cx: &App,
    issue_id: &str,
    now_epoch: i64,
) -> Option<String> {
    let collections = Store::global(cx).collections();
    let sessions = collections.coding_sessions.read(cx);
    let devices = collections.devices.read(cx);
    live_session_device(
        sessions.iter(),
        devices.iter(),
        issue_id,
        now_epoch,
        now_epoch * 1_000,
    )
}

/// Pure core of [`live_session_device_for_issue`]. A live row with no
/// resolvable device name still blocks — the label is only for the message.
/// EXP-549: the name is the machine's CURRENT `devices.label` (the user's
/// rename) whenever the row resolves, not the start-time snapshot.
pub(crate) fn live_session_device<'a>(
    sessions: impl Iterator<Item = &'a domain::rows::CodingSession>,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    issue_id: &str,
    now_epoch: i64,
    now_ms: i64,
) -> Option<String> {
    let session = sessions
        .filter(|session| session.issue_id.as_deref() == Some(issue_id))
        .find(|session| coding_session_is_live(session, now_epoch))?;
    let presentation = session_device_presentation(session, devices, now_ms);
    Some(
        presentation
            .label
            .unwrap_or_else(|| "another device".to_string()),
    )
}

/// EXP-549/550: how a session names its host machine and whether that machine
/// is offline. Resolved against the synced `devices` rows, so a rename shows
/// up everywhere and a lid-closed host stops reading as live.
pub(crate) struct SessionDevicePresentation {
    /// The machine's name — the resolved `devices.label`, else the session's
    /// start-time `device_label` snapshot, else `None`.
    pub label: Option<String>,
    /// The resolved machine has not heartbeat inside the contract's online
    /// window. Always `false` when no `devices` row resolves — an unknown
    /// machine is never accused of being offline.
    pub offline: bool,
}

/// Resolve `session`'s host machine against the synced `devices` rows.
///
/// The ONE match is on the session's steer `device_id` (EXP-549's stamp),
/// preferring the session owner's own row when a shared device produced
/// several. EXP-589: a row with NO `device_id` resolves no machine at all —
/// it falls straight through to the session's own `device_label` snapshot,
/// never offline. The old label-equality fallback (unique-label match) is
/// gone: matching machines by NAME could only ever rename the right one or
/// mislabel the wrong one, and the presentation reads the same either way.
pub(crate) fn session_device_presentation<'a>(
    session: &domain::rows::CodingSession,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    now_ms: i64,
) -> SessionDevicePresentation {
    let row = match session.device_id.as_deref() {
        Some(device_id) => {
            let matches = devices
                .filter(|row| row.device_id.as_deref() == Some(device_id))
                .collect::<Vec<_>>();
            matches
                .iter()
                .find(|row| {
                    session.user_id.is_some() && row.user_id.as_deref() == session.user_id.as_deref()
                })
                .copied()
                .or_else(|| matches.first().copied())
        }
        None => None,
    };
    match row {
        Some(row) => SessionDevicePresentation {
            label: row
                .label
                .clone()
                .filter(|label| !label.is_empty())
                .or_else(|| session.device_label.clone()),
            offline: !crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
        },
        None => SessionDevicePresentation {
            label: session.device_label.clone(),
            offline: false,
        },
    }
}

/// EXP-550: a session whose host machine is offline (lid closed) is PAUSED,
/// not live — but only while it would otherwise read as in-flight. Review and
/// Done are outcomes the offline host cannot un-say, so they are never
/// overridden.
pub(crate) fn session_is_paused(
    display: CodingSessionDisplay,
    presentation: &SessionDevicePresentation,
) -> bool {
    presentation.offline
        && matches!(
            display,
            CodingSessionDisplay::Running | CodingSessionDisplay::NeedsInput
        )
}

/// EXP-214: how a LIVE coding session renders. The synced status alone is not
/// the whole story — `in_review` splits on the linked issue's PR outcome
/// (merged → the run is done, review otherwise, matching the issue-status
/// palette: review green, done blue), and the desktop-written `needs_input`
/// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
/// renders a RUNNING session as an amber "needs input". Callers pass only
/// sessions that already passed [`coding_session_is_live`].
///
/// EXP-531: `in_review` beats `needs_input` — once the PR is open the run is
/// done coding, and the flag remaining true is idle noise (claude's
/// "waiting for your input" nudge lands AFTER `open_pr` flips the row, and
/// old desktops keep writing it). EXP-679: the server ACCEPTS the flag on
/// every live status now (a person-started run stays live after its PR and
/// the idle edge is "your turn"), so this ordering is the ONLY mask.
///
/// EXP-498/EXP-540: a merged PR now ENDS the session, and the session status
/// `merged` is retired — so `in_review` + `pr_state = merged` is the only
/// merge inference left. It is kept for old-SERVER tolerance: a lagging
/// self-host server can still park a row in `in_review` while its PR is
/// already merged.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodingSessionDisplay {
    Running,
    NeedsInput,
    Review,
    Done,
}

pub(crate) fn coding_session_display(
    session: &domain::rows::CodingSession,
    pr_state: Option<&str>,
) -> CodingSessionDisplay {
    let merged = pr_state == Some(domain::contract::PR_STATE_MERGED);
    if session.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_IN_REVIEW) {
        return if merged {
            CodingSessionDisplay::Done
        } else {
            CodingSessionDisplay::Review
        };
    }
    if session.needs_input.unwrap_or(false) && !merged {
        return CodingSessionDisplay::NeedsInput;
    }
    CodingSessionDisplay::Running
}

// ---------------------------------------------------------------------------
// Create-flow sync gate (§4.1 "awaitTxId" analog)
// ---------------------------------------------------------------------------

/// Wait (bounded) until `id` is visible in a synced collection — the gate the
/// create dialogs use before close-and-navigate (§4.1: "gated for
/// create/delete/navigate flows"). The sync engine has no per-txid waiter
/// yet, and for creates row-visibility is the same signal. Returns `false` on
/// timeout/closed-window; callers proceed anyway (the target screen renders
/// from the live collection either way).
pub(crate) async fn await_row_visible<T: 'static>(
    collection: &gpui::Entity<sync::Collection<T>>,
    id: &str,
    window: &mut gpui::AsyncWindowContext,
) -> bool {
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
    const POLL: std::time::Duration = std::time::Duration::from_millis(60);
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let Ok(present) = window.update(|_, cx| collection.read(cx).get(id).is_some()) else {
            return false; // window gone — nothing left to gate
        };
        if present {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            log::warn!("[ui] create gate timed out waiting for row {id}");
            return false;
        }
        window.background_executor().timer(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pull(number: u64) -> api::repositories::OpenPull {
        serde_json::from_value(json!({
            "number": number,
            "url": format!("https://github.com/acme/web/pull/{number}"),
            "title": "t",
            "branch": "b",
            "baseBranch": "main",
            "draft": false,
            "createdAt": "2026-07-10T08:00:00Z",
        }))
        .unwrap()
    }

    fn pull_repo(repository_id: &str, numbers: &[u64]) -> api::repositories::OpenPullsRepo {
        serde_json::from_value(json!({
            "repositoryId": repository_id,
            "fullName": format!("acme/{repository_id}"),
            "pulls": [],
        }))
        .map(|mut repo: api::repositories::OpenPullsRepo| {
            repo.pulls = numbers.iter().map(|n| pull(*n)).collect();
            repo
        })
        .unwrap()
    }

    fn session(status: Option<&str>, updated_at: Option<&str>) -> domain::rows::CodingSession {
        serde_json::from_value(json!({
            "id": "sess-1",
            "issue_id": "issue-1",
            "status": status,
            "updated_at": updated_at,
        }))
        .unwrap()
    }

    #[test]
    fn coding_session_live_within_stale_window() {
        // 2026-07-17T12:00:00Z; heartbeat 30 minutes ago.
        let now = 1784289600_i64;
        let s = session(Some("running"), Some("2026-07-17T11:30:00Z"));
        assert!(coding_session_is_live(&s, now));
    }

    #[test]
    fn coding_session_stale_past_window() {
        let now = 1784289600_i64;
        // Last heartbeat 3h ago — past the 2h contract window.
        let s = session(Some("running"), Some("2026-07-17T09:00:00Z"));
        assert!(!coding_session_is_live(&s, now));
        // Sanity: the generated contract constant stays the 2h the server sweeps by.
        assert_eq!(domain::contract::CODING_SESSION_STALE_MS, 7_200_000);
    }

    #[test]
    fn coding_session_non_running_is_never_live() {
        let now = 1784289600_i64;
        let s = session(Some("ended"), Some("2026-07-17T11:59:00Z"));
        assert!(!coding_session_is_live(&s, now));
        let s = session(None, Some("2026-07-17T11:59:00Z"));
        assert!(!coding_session_is_live(&s, now));
        // EXP-540: the retired `merged` status is not a live status either —
        // a merged PR ends the session (EXP-498).
        let s = session(Some("merged"), Some("2026-07-17T11:59:00Z"));
        assert!(!coding_session_is_live(&s, now));
    }

    /// EXP-194: an `in_review` row (PR open, terminal alive) is live while
    /// fresh and absent when stale — exactly like `running`.
    #[test]
    fn coding_session_in_review_is_live_until_stale() {
        let now = 1784289600_i64;
        let fresh = session(Some("in_review"), Some("2026-07-17T11:30:00Z"));
        assert!(coding_session_is_live(&fresh, now));
        let stale = session(Some("in_review"), Some("2026-07-17T09:00:00Z"));
        assert!(!coding_session_is_live(&stale, now));
    }

    /// EXP-540: with the `merged` session status retired, `in_review` + a
    /// merged PR is the only merge inference left — kept for old-SERVER
    /// tolerance (a lagging self-host can still park a row in `in_review`
    /// while its PR is already merged).
    #[test]
    fn coding_session_display_infers_done_from_a_merged_pr_in_review() {
        let row = session(Some("in_review"), Some("2026-07-17T11:30:00Z"));
        assert!(
            coding_session_display(&row, Some(domain::contract::PR_STATE_MERGED))
                == CodingSessionDisplay::Done
        );
        assert!(
            coding_session_display(&row, Some(domain::contract::PR_STATE_OPEN))
                == CodingSessionDisplay::Review
        );
        assert!(coding_session_display(&row, None) == CodingSessionDisplay::Review);
    }

    #[test]
    fn coding_session_display_in_review_beats_needs_input() {
        // EXP-531: claude's idle nudge lands AFTER open_pr flips the row to
        // in_review, and the stamped flag used to mask "review" as an amber
        // "needs input" for the rest of the session. A running row keeps the
        // flag's meaning.
        let flagged: domain::rows::CodingSession = serde_json::from_value(json!({
            "id": "sess-1",
            "issue_id": "issue-1",
            "status": "in_review",
            "updated_at": "2026-07-17T11:30:00Z",
            "needs_input": true,
        }))
        .unwrap();
        assert!(
            coding_session_display(&flagged, Some(domain::contract::PR_STATE_OPEN))
                == CodingSessionDisplay::Review
        );
        let running: domain::rows::CodingSession = serde_json::from_value(json!({
            "id": "sess-2",
            "issue_id": "issue-1",
            "status": "running",
            "updated_at": "2026-07-17T11:30:00Z",
            "needs_input": true,
        }))
        .unwrap();
        assert!(coding_session_display(&running, None) == CodingSessionDisplay::NeedsInput);
    }

    #[test]
    fn coding_session_unparseable_updated_at_fails_open() {
        // Missing/garbled liveness signal ⇒ live — never hide a session the
        // server still considers alive; the sweep is the backstop.
        let now = 1784289600_i64;
        assert!(coding_session_is_live(&session(Some("running"), None), now));
        assert!(coding_session_is_live(
            &session(Some("running"), Some("not-a-timestamp")),
            now
        ));
    }

    fn session_on(
        id: &str,
        issue_id: Option<&str>,
        status: &str,
        updated_at: &str,
        device_label: Option<&str>,
    ) -> domain::rows::CodingSession {
        serde_json::from_value(json!({
            "id": id,
            "issue_id": issue_id,
            "status": status,
            "updated_at": updated_at,
            "device_label": device_label,
        }))
        .unwrap()
    }

    /// REV2-24: a live row on ANOTHER device blocks (and names it); a batch
    /// row (no `issue_id`) never claims an issue.
    #[test]
    fn live_session_device_reports_the_owning_device() {
        let now = 1784289600_i64;
        let rows = vec![
            session_on("s-batch", None, "running", "2026-07-17T11:59:00Z", None),
            session_on(
                "s-1",
                Some("issue-1"),
                "running",
                "2026-07-17T11:59:00Z",
                Some("mac-studio"),
            ),
        ];
        assert_eq!(
            live_session_device(rows.iter(), [].iter(), "issue-1", now, now * 1_000).as_deref(),
            Some("mac-studio")
        );
        assert_eq!(
            live_session_device(rows.iter(), [].iter(), "issue-2", now, now * 1_000),
            None
        );
        // A live row with no label still blocks.
        let unlabeled = vec![session_on(
            "s-2",
            Some("issue-2"),
            "in_review",
            "2026-07-17T11:59:00Z",
            None,
        )];
        assert_eq!(
            live_session_device(unlabeled.iter(), [].iter(), "issue-2", now, now * 1_000)
                .as_deref(),
            Some("another device")
        );
    }

    /// Ended and stale rows are absent — a remote start must not be blocked
    /// forever by a crashed session's leftover row.
    #[test]
    fn live_session_device_ignores_dead_rows() {
        let now = 1784289600_i64;
        let rows = vec![
            session_on(
                "s-1",
                Some("issue-1"),
                "ended",
                "2026-07-17T11:59:00Z",
                Some("m"),
            ),
            session_on(
                "s-2",
                Some("issue-1"),
                "running",
                "2026-07-17T09:00:00Z",
                Some("m"),
            ),
        ];
        assert_eq!(
            live_session_device(rows.iter(), [].iter(), "issue-1", now, now * 1_000),
            None
        );
    }

    // ---- EXP-549/550: host-machine resolution + the paused state --------

    fn device_row(
        id: &str,
        device_id: Option<&str>,
        user_id: Option<&str>,
        label: Option<&str>,
        last_seen_at: Option<&str>,
    ) -> domain::rows::DeviceRow {
        serde_json::from_value(json!({
            "id": id,
            "device_id": device_id,
            "user_id": user_id,
            "label": label,
            "last_seen_at": last_seen_at,
        }))
        .unwrap()
    }

    fn hosted_session(
        device_id: Option<&str>,
        device_label: Option<&str>,
        user_id: Option<&str>,
    ) -> domain::rows::CodingSession {
        serde_json::from_value(json!({
            "id": "sess-1",
            "issue_id": "issue-1",
            "status": "running",
            "updated_at": "2026-07-17T11:59:00Z",
            "device_id": device_id,
            "device_label": device_label,
            "user_id": user_id,
        }))
        .unwrap()
    }

    /// The `device_id` stamp wins over the stale label snapshot AND over a
    /// label-equal row — a rename must show up on the running session.
    #[test]
    fn session_device_presentation_prefers_the_device_id_match() {
        let now_ms = 1784289600_000_i64;
        let devices = vec![
            device_row(
                "d-1",
                Some("dev-1"),
                Some("user-1"),
                Some("Danny's MacBook"),
                Some("2026-07-17T11:59:30Z"),
            ),
            device_row(
                "d-2",
                Some("dev-2"),
                Some("user-1"),
                Some("mac-studio"),
                Some("2026-07-17T11:59:30Z"),
            ),
        ];
        let session = hosted_session(Some("dev-1"), Some("mac-studio"), Some("user-1"));
        let presentation = session_device_presentation(&session, devices.iter(), now_ms);
        assert_eq!(presentation.label.as_deref(), Some("Danny's MacBook"));
        assert!(!presentation.offline);
    }

    /// A shared device produces one row per member — the session owner's own
    /// row wins so the label is the one that user renamed.
    #[test]
    fn session_device_presentation_prefers_the_owner_row_on_a_shared_device() {
        let now_ms = 1784289600_000_i64;
        let devices = vec![
            device_row(
                "d-teammate",
                Some("dev-1"),
                Some("user-2"),
                Some("Shared box"),
                Some("2026-07-17T11:59:30Z"),
            ),
            device_row(
                "d-mine",
                Some("dev-1"),
                Some("user-1"),
                Some("Build server"),
                Some("2026-07-17T11:59:30Z"),
            ),
        ];
        let session = hosted_session(Some("dev-1"), Some("old-host"), Some("user-1"));
        let presentation = session_device_presentation(&session, devices.iter(), now_ms);
        assert_eq!(presentation.label.as_deref(), Some("Build server"));
    }

    /// EXP-589: a row with no `device_id` resolves NO machine — it shows its
    /// own label snapshot and is never accused of being offline, even when a
    /// same-named machine happens to be synced (that label match was the
    /// removed fallback). A session that DOES carry a device_id likewise never
    /// falls back to the label: an unknown machine stays unresolved.
    #[test]
    fn session_device_presentation_never_matches_by_label() {
        let now_ms = 1784289600_000_i64;
        let unique = vec![device_row(
            "d-1",
            Some("dev-1"),
            Some("user-1"),
            Some("mac-studio"),
            // Long stale: would have read "offline" through the old fallback.
            Some("2026-07-17T09:00:00Z"),
        )];
        let legacy = hosted_session(None, Some("mac-studio"), Some("user-1"));
        let presentation = session_device_presentation(&legacy, unique.iter(), now_ms);
        assert_eq!(presentation.label.as_deref(), Some("mac-studio"));
        assert!(!presentation.offline, "an unresolved machine is never offline");

        let stamped = hosted_session(Some("dev-unknown"), Some("mac-studio"), Some("user-1"));
        let presentation = session_device_presentation(&stamped, unique.iter(), now_ms);
        assert_eq!(presentation.label.as_deref(), Some("mac-studio"));
        assert!(!presentation.offline);
    }

    /// EXP-550: offline flips only the in-flight displays to paused.
    #[test]
    fn session_is_paused_only_while_in_flight() {
        let offline = SessionDevicePresentation {
            label: Some("mac-studio".to_string()),
            offline: true,
        };
        let online = SessionDevicePresentation {
            label: Some("mac-studio".to_string()),
            offline: false,
        };
        for display in [
            CodingSessionDisplay::Running,
            CodingSessionDisplay::NeedsInput,
        ] {
            assert!(session_is_paused(display, &offline));
            assert!(!session_is_paused(display, &online));
        }
        for display in [CodingSessionDisplay::Review, CodingSessionDisplay::Done] {
            assert!(!session_is_paused(display, &offline));
            assert!(!session_is_paused(display, &online));
        }
    }

    /// The blocker message names the RENAMED machine (EXP-549), not the
    /// start-time snapshot.
    #[test]
    fn live_session_device_reports_the_renamed_label() {
        let now = 1784289600_i64;
        let sessions = vec![hosted_session(
            Some("dev-1"),
            Some("old-host"),
            Some("user-1"),
        )];
        let devices = vec![device_row(
            "d-1",
            Some("dev-1"),
            Some("user-1"),
            Some("Danny's MacBook"),
            Some("2026-07-17T11:59:30Z"),
        )];
        assert_eq!(
            live_session_device(
                sessions.iter(),
                devices.iter(),
                "issue-1",
                now,
                now * 1_000
            )
            .as_deref(),
            Some("Danny's MacBook")
        );
    }

    #[test]
    fn visible_pull_repos_hides_empty_repos() {
        let repos = vec![pull_repo("repo-1", &[1, 2]), pull_repo("repo-2", &[])];
        let visible = visible_pull_repos(&repos);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].repository_id, "repo-1");
    }

    #[test]
    fn remove_merged_pull_drops_only_the_matching_row() {
        let mut repos = vec![pull_repo("repo-1", &[1, 2]), pull_repo("repo-2", &[1])];
        remove_merged_pull(&mut repos, "repo-1", 1);
        assert_eq!(
            repos[0].pulls.iter().map(|p| p.number).collect::<Vec<_>>(),
            [2]
        );
        // Same PR number in another repo is untouched.
        assert_eq!(repos[1].pulls.len(), 1);
        // Unknown targets are a no-op.
        remove_merged_pull(&mut repos, "repo-9", 1);
        remove_merged_pull(&mut repos, "repo-1", 99);
        assert_eq!(repos[0].pulls.len(), 1);
        assert_eq!(repos[1].pulls.len(), 1);
    }

    fn issue(pr_state: Option<&str>) -> domain::rows::Issue {
        serde_json::from_value(json!({
            "id": "i-1",
            "board_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": "in_review",
            "pr_state": pr_state,
        }))
        .unwrap()
    }

    fn notification(
        id: &str,
        issue_id: Option<&str>,
        team_id: Option<&str>,
        kind: &str,
        created_at: &str,
        read: bool,
    ) -> domain::rows::Notification {
        serde_json::from_value(json!({
            "id": id,
            "user_id": "u-1",
            "issue_id": issue_id,
            "team_id": team_id,
            "type": kind,
            "title": format!("title {id}"),
            "created_at": created_at,
            "read_at": read.then_some("2026-07-18T00:00:00Z"),
        }))
        .unwrap()
    }

    fn inbox_issue(id: &str) -> domain::rows::Issue {
        serde_json::from_value(json!({
            "id": id,
            "board_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": "backlog",
        }))
        .unwrap()
    }

    /// EXP-180: issue-less `support_reply` rows group per team instead of
    /// being dropped — desktop has no push channel, so these rows are its
    /// only passive helpdesk signal.
    #[test]
    fn inbox_groups_support_replies_per_team() {
        let entries = build_inbox_entries(
            vec![
                notification("n-1", None, Some("w-1"), "support_reply", "2026-07-18T10:00:00Z", false),
                notification("n-2", None, Some("w-1"), "support_reply", "2026-07-18T09:00:00Z", true),
                notification("n-3", None, Some("w-2"), "support_reply", "2026-07-18T08:00:00Z", false),
            ],
            |_| None,
            |team_id| match team_id {
                "w-1" => Some("Acme".to_string()),
                "w-2" => Some("Beta".to_string()),
                _ => None,
            },
        );
        assert_eq!(entries.len(), 2);
        let InboxEntry::Support(acme) = &entries[0] else {
            panic!("expected a Support entry");
        };
        assert_eq!(acme.team_id.as_deref(), Some("w-1"));
        assert_eq!(acme.team_name.as_deref(), Some("Acme"));
        assert_eq!(acme.items.len(), 2);
        assert_eq!(acme.unread, 1);
        // Newest first inside the group.
        assert_eq!(acme.items[0].id, "n-1");
        let InboxEntry::Support(beta) = &entries[1] else {
            panic!("expected a Support entry");
        };
        assert_eq!(beta.team_id.as_deref(), Some("w-2"));
        assert_eq!(beta.unread, 1);
        // Support unread counts ride the header/badge total.
        assert_eq!(entries.iter().map(InboxEntry::unread).sum::<usize>(), 2);
    }

    #[test]
    fn inbox_collapses_null_and_unknown_teams_into_one_generic_group() {
        let entries = build_inbox_entries(
            vec![
                // Legacy pre-column row (no team_id).
                notification("n-1", None, None, "support_reply", "2026-07-18T10:00:00Z", false),
                // team_id set but the team row hasn't synced.
                notification("n-2", None, Some("w-gone"), "support_reply", "2026-07-18T09:00:00Z", false),
            ],
            |_| None,
            |_| None,
        );
        assert_eq!(entries.len(), 1);
        let InboxEntry::Support(generic) = &entries[0] else {
            panic!("expected a Support entry");
        };
        assert_eq!(generic.team_id, None);
        assert_eq!(generic.team_name, None);
        assert_eq!(generic.items.len(), 2);
        assert_eq!(generic.unread, 2);
    }

    #[test]
    fn inbox_interleaves_support_and_issue_groups_newest_first() {
        let entries = build_inbox_entries(
            vec![
                notification("n-old", Some("i-1"), None, "issue_comment", "2026-07-18T08:00:00Z", false),
                notification("n-support", None, Some("w-1"), "support_reply", "2026-07-18T09:00:00Z", false),
                notification("n-new", Some("i-2"), None, "issue_assigned", "2026-07-18T10:00:00Z", false),
            ],
            |issue_id| Some(inbox_issue(issue_id)),
            |_| Some("Acme".to_string()),
        );
        // Web parity: ALL groups sort together by their latest item.
        let kinds: Vec<&str> = entries
            .iter()
            .map(|entry| match entry {
                InboxEntry::Issue(group) => group.issue.id.as_str(),
                InboxEntry::Support(_) => "support",
            })
            .collect();
        assert_eq!(kinds, ["i-2", "support", "i-1"]);
    }

    #[test]
    fn inbox_still_drops_unresolvable_issue_rows_and_unknown_issueless_kinds() {
        let entries = build_inbox_entries(
            vec![
                // Issue not synced (or its board trashed) — dropped.
                notification("n-1", Some("i-gone"), None, "issue_comment", "2026-07-18T10:00:00Z", false),
                // Issue-less row of an unknown future kind — dropped, never
                // a Support group.
                notification("n-2", None, None, "mystery_kind", "2026-07-18T09:00:00Z", false),
            ],
            |_| None,
            |_| None,
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn reviews_only_include_open_prs() {
        // Open PR → in the queue.
        assert!(is_reviewable(&issue(Some("open"))));
        // Non-open PR states never review.
        assert!(!is_reviewable(&issue(Some("merged"))));
        assert!(!is_reviewable(&issue(None)));
    }
}
