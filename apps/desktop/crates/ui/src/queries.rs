//! Derived read queries over the §05 collections (masterplan-v3 §4.1: "Keep
//! query helpers in `ui/src/queries.rs`, one function per web hook — do not
//! scatter filtering logic into views"), plus the shared tRPC-client
//! accessor mutations go through.
//!
//! Queries are plain Rust over the in-memory collections — no query engine,
//! no SQL at render time. Grouping/sorting semantics live in `domain::board`
//! (the verbatim `project-board.ts` port); this module only joins collections.

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
/// `use-project-board-data.ts` return, §4.1).
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

/// `use-project-board-data.ts`: one project's issues, filtered + grouped.
pub fn project_board(cx: &App, project_id: &str, filters: &IssueFilters) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues = collections.issues_in_project(project_id, cx);
    board_data_from(cx, issues, filters)
}

/// `use-my-issues-data.ts`: the workspace's issues assigned to me, filtered +
/// grouped like a board.
pub fn my_issues(
    cx: &App,
    workspace_id: &str,
    user_id: &str,
    filters: &IssueFilters,
) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues: Vec<_> = collections
        .issues_in_workspace(workspace_id, cx)
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
        && collections.projects.read(cx).is_ready()
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

/// An issue's workspace (issue → project → `workspace_id`) — the scoping
/// join the editors/autocomplete need (§4.6). `None` while the chain has not
/// synced.
pub(crate) fn issue_workspace_id(cx: &App, issue_id: &str) -> Option<String> {
    let collections = Store::global(cx).collections();
    let project_id = collections
        .issues
        .read(cx)
        .get(issue_id)
        .map(|issue| issue.project_id.clone())?;
    collections
        .projects
        .read(cx)
        .get(&project_id)
        .map(|project| project.workspace_id.clone())
}

/// `use-workspace-data.ts` `useWorkspaceUsers`: `workspace_members` ⨝ `users`
/// (name-sorted for deterministic pickers).
pub fn workspace_users(cx: &App, workspace_id: &str) -> Vec<domain::rows::User> {
    let collections = Store::global(cx).collections();
    let members = collections.workspace_members.read(cx);
    let member_ids: std::collections::HashSet<&str> = members
        .iter()
        .filter(|member| member.workspace_id == workspace_id)
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

/// The label-picker read (`label-picker.tsx`): a workspace's labels,
/// sort-order sorted.
pub fn workspace_labels(cx: &App, workspace_id: &str) -> Vec<Label> {
    let collections = Store::global(cx).collections();
    let mut out: Vec<Label> = collections
        .labels
        .read(cx)
        .iter()
        .filter(|label| label.workspace_id == workspace_id)
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

/// The inbox read: is-ready gate + issue-grouped notifications. The
/// notifications shape is already user-scoped server-side; like web, groups
/// are NOT workspace-filtered (the join to a synced issue+project is the only
/// membership requirement).
pub struct InboxData {
    pub is_ready: bool,
    pub groups: Vec<InboxGroup>,
    pub total_unread: usize,
}

/// `inbox-view.tsx` grouping: notifications ⨝ issues ⨝ projects, grouped by
/// issue, group order = newest first item.
pub fn inbox(cx: &App) -> InboxData {
    let collections = Store::global(cx).collections();
    let is_ready = collections.notifications.read(cx).is_ready()
        && collections.issues.read(cx).is_ready()
        && collections.projects.read(cx).is_ready();

    let mut notifications: Vec<domain::rows::Notification> = collections
        .notifications
        .read(cx)
        .iter()
        .cloned()
        .collect();
    // Newest first (web `orderBy createdAt desc`); ISO strings from one
    // source compare lexicographically.
    notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let issues = collections.issues.read(cx);
    let projects = collections.projects.read(cx);

    let mut order: Vec<String> = Vec::new();
    let mut by_issue: HashMap<String, InboxGroup> = HashMap::new();
    for notification in notifications {
        let Some(issue_id) = notification.issue_id.clone() else {
            continue;
        };
        let Some(issue) = issues.get(&issue_id) else {
            continue;
        };
        if projects.get(&issue.project_id).is_none() {
            continue;
        }
        let group = by_issue.entry(issue_id.clone()).or_insert_with(|| {
            order.push(issue_id.clone());
            InboxGroup {
                issue: issue.clone(),
                items: Vec::new(),
                unread: 0,
            }
        });
        if notification.read_at.is_none() {
            group.unread += 1;
        }
        group.items.push(notification);
    }

    // Feed order is newest-first, so first-seen issue order IS the web's
    // sort-by-newest-item group order.
    let groups: Vec<InboxGroup> = order
        .into_iter()
        .filter_map(|issue_id| by_issue.remove(&issue_id))
        .collect();
    let total_unread = groups.iter().map(|group| group.unread).sum();

    InboxData {
        is_ready,
        groups,
        total_unread,
    }
}

/// Open pull requests: synced issues in this workspace with an open PR — a
/// query over `issues`, independent of notifications. Feeds the Reviews rail
/// badge and, grouped, the Reviews tool window.
pub fn review_issues(cx: &App, workspace_id: &str) -> Vec<domain::rows::Issue> {
    let collections = Store::global(cx).collections();
    let projects = collections.projects.read(cx);
    collections
        .issues
        .read(cx)
        .iter()
        .filter(|issue| {
            issue.pr_state.as_deref() == Some("open")
                && projects
                    .get(&issue.project_id)
                    .is_some_and(|project| project.workspace_id == workspace_id)
        })
        .cloned()
        .collect()
}

/// One Reviews tool-window section: a project and its open-PR issues (the
/// desktop mirror of the web `use-reviews-data.ts` `ReviewGroup`).
pub struct ReviewGroup {
    pub project: domain::rows::Project,
    pub issues: Vec<domain::rows::Issue>,
}

/// The Reviews tool window read: [`review_issues`] grouped by project.
/// Groups follow project `sort_order` (name tiebreak, like the sidebars);
/// issues are newest first within a group — web parity.
pub fn review_groups(cx: &App, workspace_id: &str) -> Vec<ReviewGroup> {
    let open = review_issues(cx, workspace_id);
    let collections = Store::global(cx).collections();
    let projects = collections.projects.read(cx);

    let mut by_project: HashMap<String, Vec<domain::rows::Issue>> = HashMap::new();
    for issue in open {
        by_project
            .entry(issue.project_id.clone())
            .or_default()
            .push(issue);
    }

    let mut groups: Vec<ReviewGroup> = by_project
        .into_iter()
        .filter_map(|(project_id, mut issues)| {
            // The workspace filter in `review_issues` already proved the
            // project exists; the lookup only resolves the row.
            let project = projects.get(&project_id)?.clone();
            // Newest first — ISO strings from one source compare
            // lexicographically (None sorts last).
            issues.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            Some(ReviewGroup { project, issues })
        })
        .collect();
    groups.sort_by(|a, b| {
        a.project
            .sort_order
            .unwrap_or(f64::MAX)
            .total_cmp(&b.project.sort_order.unwrap_or(f64::MAX))
            .then_with(|| {
                a.project
                    .name
                    .to_lowercase()
                    .cmp(&b.project.name.to_lowercase())
            })
    });
    groups
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
