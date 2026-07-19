//! Derived read queries over the §05 collections (masterplan-v3 §4.1: "Keep
//! query helpers in `ui/src/queries.rs`, one function per web hook — do not
//! scatter filtering logic into views"), plus the shared tRPC-client
//! accessor mutations go through.
//!
//! Queries are plain Rust over the in-memory collections — no query engine,
//! no SQL at render time. Grouping/sorting semantics live in `domain::board`
//! (the verbatim `board-view.ts` port); this module only joins collections.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::App;
use sync::Store;

use domain::board::{
    build_filtered_issues, build_issue_label_ids_map, build_visible_issue_groups, IssueGroup,
};
use domain::filters::IssueFilters;
use domain::rows::Label;

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
    pub groups: Vec<IssueGroup>,
    /// issue id → its labels (for the row label chips).
    pub labels_by_issue: HashMap<String, Vec<Label>>,
}

/// `use-board-view-data.ts`: one board's issues, filtered + grouped.
pub fn board_board(cx: &App, board_id: &str, filters: &IssueFilters) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues = collections.issues_in_board(board_id, cx);
    board_data_from(cx, issues, filters)
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
    board_data_from(cx, issues, filters)
}

fn board_data_from(
    cx: &App,
    issues: Vec<domain::rows::Issue>,
    filters: &IssueFilters,
) -> BoardData {
    let collections = Store::global(cx).collections();
    let is_ready = collections.issues.read(cx).is_ready()
        && collections.boards.read(cx).is_ready()
        && collections.issue_labels.read(cx).is_ready()
        && collections.labels.read(cx).is_ready();

    let issue_links: Vec<_> = collections.issue_labels.read(cx).iter().cloned().collect();
    let label_ids_by_issue = build_issue_label_ids_map(&issue_links);

    let has_any_issues = !issues.is_empty();
    let filtered = build_filtered_issues(issues, &label_ids_by_issue, filters);
    let today = today_local();
    let groups = build_visible_issue_groups(&filtered, &filters.statuses, &today);

    // Resolve label rows for the chips (web buildIssueLabelMap: unknown label
    // ids are skipped — referential integrity is a query-time concern, §5.4).
    let labels = collections.labels.read(cx);
    let mut labels_by_issue: HashMap<String, Vec<Label>> = HashMap::new();
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
                labels_by_issue.insert(issue.id.clone(), resolved);
            }
        }
    }

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
/// (name-sorted for deterministic pickers). Synthetic `is_agent` users
/// (widget creators) are excluded — every assignee/member picker wants the
/// HUMAN members, matching the web's `people` filter (EXP-50 alignment: this
/// query and the properties panel's member read now share the rule).
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
        .filter(|user| member_ids.contains(user.id.as_str()) && user.is_agent != Some(true))
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
/// notifications, ONE group per ticket team. Desktop has no push channel,
/// so these synced rows are its only passive helpdesk signal — dropping
/// them made a reporter reply invisible unless the Support tool happened to
/// be open. Click marks the group read and opens the team's Support tool.
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

/// The per-issue Reviews predicate: an OPEN pull request on a NON-archived
/// issue. Archived issues are hidden on every other surface (the boards go
/// through `issues_in_*`, which filter `archived_at`) — Reviews drops them at
/// the issue level too, mobile parity: a batch PR entry survives with its
/// remaining issues and disappears only when ALL of its issues are archived.
fn is_reviewable(issue: &domain::rows::Issue) -> bool {
    issue.pr_state.as_deref() == Some("open") && issue.archived_at.is_none()
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

/// Every non-archived issue in a team (issues ⨝ boards, shared sort
/// order) — the add-issues picker's candidate pool (the dialog filters
/// status/membership on top).
pub fn team_issues(cx: &App, team_id: &str) -> Vec<domain::rows::Issue> {
    Store::global(cx)
        .collections()
        .issues_in_team(team_id, cx)
}

/// EXP-153: a `running` coding_sessions row renders as live only while its
/// `updated_at` (heartbeat-advanced) is inside the contract stale window —
/// stale rows are treated as ABSENT, mirroring the server sweep's DELETE
/// (never as `ended`, which is the kill-switch signal). Missing/unparseable
/// `updated_at` → live (fail-open: never hide a session the server still
/// considers alive; the sweep is the backstop). No re-render timer: gpui
/// re-evaluates on every notify, and this process is the heartbeat writer
/// for its own sessions — a phantom row from a crashed prior instance
/// re-evaluates on the next render regardless.
pub(crate) fn coding_session_is_live(
    session: &domain::rows::CodingSession,
    now_epoch: i64,
) -> bool {
    if session.status.as_deref() != Some(domain::contract::CODING_SESSION_STATUS_RUNNING) {
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

    fn issue(pr_state: Option<&str>, archived_at: Option<&str>) -> domain::rows::Issue {
        serde_json::from_value(json!({
            "id": "i-1",
            "board_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": "in_review",
            "pr_state": pr_state,
            "archived_at": archived_at,
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
            "status": "todo",
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
    fn reviews_exclude_archived_issues() {
        // Open PR on a live issue → in the queue.
        assert!(is_reviewable(&issue(Some("open"), None)));
        // Archived issues are hidden everywhere else (boards, mobile Reviews)
        // — an open PR must not resurrect one in Reviews.
        assert!(!is_reviewable(&issue(
            Some("open"),
            Some("2026-07-15T08:00:00Z")
        )));
        // Non-open PR states never review, archived or not.
        assert!(!is_reviewable(&issue(Some("merged"), None)));
        assert!(!is_reviewable(&issue(None, None)));
    }
}
