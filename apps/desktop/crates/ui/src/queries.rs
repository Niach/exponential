//! Derived read queries over the §05 collections (masterplan-v3 §4.1: "Keep
//! query helpers in `ui/src/queries.rs`, one function per web hook — do not
//! scatter filtering logic into views"), plus the shared tRPC-client
//! accessor mutations go through.
//!
//! Queries are plain Rust over the in-memory collections — no query engine,
//! no SQL at render time. Grouping/sorting semantics live in `domain::board`
//! (the verbatim `board-view.ts` port); this module only joins collections.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext as _};
use sync::Store;

use domain::board::build_status_groups;
use domain::issue_graph::{block_counts, BlockCounts, GraphIssue, GraphRelation};
use domain::issue_nesting::{nest_issue_rows, NestingRelation};
use domain::rows::{Issue, IssueStatusRow, Label, SessionRow};
use domain::statuses::{resolve_status_sorted, sort_team_statuses, ResolvedStatus};

use crate::session::AuthContext;

/// The board data one render needs (mirror of the web's
/// `use-board-view-data.ts` return, §4.1).
pub struct BoardData {
    /// Ready = every shape the query reads has seen its first `up-to-date`
    /// (§4.1 `is_ready`: skeleton while false, real empty-state only when
    /// true — the never-flash-empty rule).
    pub is_ready: bool,
    /// Status groups in display order, empty groups hidden (web parity).
    pub groups: Vec<BoardGroup>,
    /// issue id → its labels (for the row label chips), shared behind `Rc`
    /// so per-frame row building clones a handle, not the resolved vec.
    pub labels_by_issue: HashMap<String, Rc<Vec<Label>>>,
    /// EXP-980: issue id → its `blocks` badge numbers, computed ONCE per
    /// query over the synced relations and EVERY synced issue (a blocker on
    /// another board still counts). An issue with neither count is absent.
    pub block_counts: HashMap<String, BlockCounts>,
}

/// One status group as the views consume it — [`domain::board::IssueGroup`]
/// with every issue behind an `Rc` (REV-39): an [`Issue`] carries the full
/// markdown description, and the list re-derives its rows on every render, so
/// group members must be clonable as handles, never as row payloads.
pub struct BoardGroup {
    pub status: ResolvedStatus,
    pub issues: Vec<Rc<Issue>>,
    /// EXP-980: row-parallel with [`Self::issues`] — the nesting depth each
    /// row renders at (0 = a root). A sub-issue leaves its own status group
    /// and follows its parent here, so this vector and `issues` are ONE
    /// tree-ordered sequence; `domain::tree_guides::guides_for` turns it
    /// into the elbow connectors.
    pub depths: Vec<usize>,
}

/// `use-board-view-data.ts`: one board's issues, grouped by status.
pub fn board_board(cx: &App, board_id: &str) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues = collections.issues_in_board(board_id, cx);
    let team_id = collections
        .boards
        .read(cx)
        .get(board_id)
        .map(|board| board.team_id.clone());
    board_data_from(cx, issues, team_id.as_deref())
}

/// `use-my-issues-data.ts`: the team's issues assigned to me, grouped like
/// a board.
pub fn my_issues(cx: &App, team_id: &str, user_id: &str) -> BoardData {
    let collections = Store::global(cx).collections();
    let issues: Vec<_> = collections
        .issues_in_team(team_id, cx)
        .into_iter()
        .filter(|issue| issue.assignee_id.as_deref() == Some(user_id))
        .collect();
    board_data_from(cx, issues, Some(team_id))
}

fn board_data_from(cx: &App, issues: Vec<Issue>, team_id: Option<&str>) -> BoardData {
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

    let today = today_local();
    let groups = build_status_groups(issues, &status_rows, &[], &today);

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

    // EXP-980: the `blocks` badge numbers, once per query. The whole synced
    // issue set feeds them — a blocker on another board is still in the way,
    // so scoping them to the list would under-count every row.
    let all_issues = collections.issues.read(cx);
    let relation_rows = collections.issue_relations.read(cx);
    let graph_issues: Vec<GraphIssue<'_>> = all_issues
        .iter()
        .map(|issue| GraphIssue {
            id: &issue.id,
            identifier: &issue.identifier,
            status: issue.status.as_wire().unwrap_or_default(),
        })
        .collect();
    let graph_relations: Vec<GraphRelation<'_>> = relation_rows
        .iter()
        .map(|row| GraphRelation {
            kind: row.kind.as_deref().unwrap_or_default(),
            issue_id: &row.issue_id,
            related_issue_id: &row.related_issue_id,
        })
        .collect();
    let block_counts = block_counts(&graph_relations, &graph_issues);

    // EXP-980: sub-issues leave their own status group and follow their
    // parent. The ROOT decided the group and the sort position above; this
    // only re-threads the rows, so the comparator's work survives.
    let group_ids: Vec<Vec<String>> = groups
        .iter()
        .map(|group| group.issues.iter().map(|issue| issue.id.clone()).collect())
        .collect();
    let nesting: Vec<NestingRelation<'_>> = relation_rows
        .iter()
        .map(|row| NestingRelation {
            kind: row.kind.as_deref().unwrap_or_default(),
            issue_id: &row.issue_id,
            related_issue_id: &row.related_issue_id,
        })
        .collect();
    let identifiers: HashMap<&str, &str> = groups
        .iter()
        .flat_map(|group| group.issues.iter())
        .map(|issue| (issue.id.as_str(), issue.identifier.as_str()))
        .collect();
    let nested = nest_issue_rows(&group_ids, &nesting, |id| {
        identifiers.get(id).copied().unwrap_or(id).to_string()
    });

    // The grouped issues move behind `Rc` handles (one allocation per issue,
    // no row payload copies) — the list's rows clone these per frame.
    let mut by_id: HashMap<String, Rc<Issue>> = HashMap::new();
    let statuses: Vec<ResolvedStatus> = groups
        .iter()
        .map(|group| group.status.clone())
        .collect();
    for group in groups {
        for issue in group.issues {
            by_id.insert(issue.id.clone(), Rc::new(issue));
        }
    }
    let groups: Vec<BoardGroup> = statuses
        .into_iter()
        .zip(nested)
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(status, rows)| {
            let mut issues: Vec<Rc<Issue>> = Vec::with_capacity(rows.len());
            let mut depths: Vec<usize> = Vec::with_capacity(rows.len());
            for row in &rows {
                if let Some(issue) = by_id.get(&row.id) {
                    issues.push(issue.clone());
                    depths.push(row.depth);
                }
            }
            BoardGroup {
                status,
                issues,
                depths,
            }
        })
        .collect();

    BoardData {
        is_ready,
        groups,
        labels_by_issue,
        block_counts,
    }
}

// ---------------------------------------------------------------------------
// Render-time memoization (REV-39, EXP-915)
// ---------------------------------------------------------------------------

/// A one-slot cache keyed by the inputs a derived query reads.
///
/// gpui re-runs every view's `render` on each window refresh — a scroll
/// tick, a hover flip, every Electric batch — so a list that derives its rows
/// IN `render` pays the whole clone+filter+sort pipeline per frame. A hit
/// here is one key compare and an `Rc` clone. REV-39 gave the big issue list
/// this cache; EXP-915 generalised it for the sidebar's lists, which had
/// none and re-ran their queries on every scrolled pixel.
pub(crate) struct Memo<K, V> {
    slot: Option<(K, Rc<V>)>,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self { slot: None }
    }
}

impl<K: PartialEq, V> Memo<K, V> {
    /// The cached value when `key` matches the last one, else `build()`,
    /// re-keyed.
    pub(crate) fn get_or_insert_with(&mut self, key: K, build: impl FnOnce() -> V) -> Rc<V> {
        if let Some((cached_key, cached)) = &self.slot {
            if *cached_key == key {
                return cached.clone();
            }
        }
        let value = Rc::new(build());
        self.slot = Some((key, value.clone()));
        value
    }

    /// Drop the slot — the next read rebuilds whatever the key says.
    pub(crate) fn clear(&mut self) {
        self.slot = None;
    }
}

/// Every input a memoized [`BoardData`] derives from (REV-39). Revisions
/// alone are not enough: an empty up-to-date batch flips a collection's
/// readiness phase WITHOUT bumping its revision, so the combined `ready` bit
/// rides along; `today` covers the local-midnight overdue boundary. The
/// SCOPE (board / team + assignee) is the caller's half of the key.
#[derive(PartialEq, Eq)]
pub(crate) struct BoardDataKey {
    issues: u64,
    issue_labels: u64,
    labels: u64,
    boards: u64,
    issue_statuses: u64,
    /// EXP-980: the nesting and the `blocks` badges are derived from the
    /// relation rows, so a relation echo has to re-run the pipeline.
    issue_relations: u64,
    ready: bool,
    today: String,
}

pub(crate) fn board_data_key(cx: &App) -> BoardDataKey {
    let collections = Store::global(cx).collections();
    BoardDataKey {
        issues: collections.issues.read(cx).revision(),
        issue_labels: collections.issue_labels.read(cx).revision(),
        labels: collections.labels.read(cx).revision(),
        boards: collections.boards.read(cx).revision(),
        issue_statuses: collections.issue_statuses.read(cx).revision(),
        issue_relations: collections.issue_relations.read(cx).revision(),
        ready: collections.issues.read(cx).is_ready()
            && collections.boards.read(cx).is_ready()
            && collections.issue_labels.read(cx).is_ready()
            && collections.labels.read(cx).is_ready(),
        today: today_local(),
    }
}

/// Every input [`inbox`] reads (EXP-915) — the same readiness caveat as
/// [`BoardDataKey`].
#[derive(PartialEq, Eq)]
pub(crate) struct InboxDataKey {
    notifications: u64,
    issues: u64,
    boards: u64,
    teams: u64,
    ready: bool,
}

pub(crate) fn inbox_data_key(cx: &App) -> InboxDataKey {
    let collections = Store::global(cx).collections();
    InboxDataKey {
        notifications: collections.notifications.read(cx).revision(),
        issues: collections.issues.read(cx).revision(),
        boards: collections.boards.read(cx).revision(),
        teams: collections.teams.read(cx).revision(),
        ready: collections.notifications.read(cx).is_ready()
            && collections.issues.read(cx).is_ready()
            && collections.boards.read(cx).is_ready()
            && collections.teams.read(cx).is_ready(),
    }
}

/// Today as `YYYY-MM-DD` for the overdue boundary. Device-LOCAL date — the
/// EXP-38 boundary every client uses: web `formatDateForMutation(new Date())`,
/// iOS `Calendar.current`, Android `LocalDate.now()`.
///
/// Rendered code asks for this a LOT — twice on the memo path alone (the
/// key's `today` plus the build's `board_data_from`), and once per overdue
/// row in the big list — and `Local::now()` is a timezone lookup plus a
/// format each time. The answer only changes at local midnight, so it is
/// cached for the current wall-clock SECOND: still exact within a second of
/// the boundary, and free for every other caller in the frame.
pub fn today_local() -> String {
    use std::cell::RefCell;
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static CACHED: RefCell<Option<(u64, String)>> = const { RefCell::new(None) };
    }
    let second = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    CACHED.with(|cached| {
        let mut cached = cached.borrow_mut();
        if let Some((at, today)) = cached.as_ref() {
            if *at == second {
                return today.clone();
            }
        }
        let today = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        *cached = Some((second, today.clone()));
        today
    })
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

/// The active account's instance URL — the origin an ABSOLUTE attachment
/// link must sit on to be ours (EXP-824 media lift, see
/// [`crate::markdown::image_url::is_own_attachment_src`]). `None` when
/// signed out or before the store exists.
pub(crate) fn instance_origin(cx: &App) -> Option<String> {
    let account_id = Store::try_global(cx)?.session(cx).account_id()?.to_string();
    cx.try_global::<AuthContext>()?
        .auth
        .account(&account_id)
        .map(|account| account.instance_url)
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

/// #EXP-1383 (agent-settings.json `assignee`): a team's agents as assignee
/// options, in the picker's order (`assignee_options`: sortOrder, then name).
/// S-3: another member's Assistant is never one (`visible_agent`); an
/// assignment to it still reads through `agent_actor`.
pub fn team_agents(cx: &App, team_id: &str) -> Vec<domain::agent_settings::AgentActorRow> {
    let me = active_account(cx).map(|account| account.user_id);
    let collections = Store::global(cx).collections();
    let rows: Vec<domain::rows::AgentDefRow> = collections
        .agents
        .read(cx)
        .iter()
        .filter(|agent| agent.team_id.as_deref() == Some(team_id))
        .filter(|agent| {
            domain::agent_settings::visible_agent(Some(agent.role.as_str()), agent.owner_id.as_deref(), me.as_deref())
        })
        .cloned()
        .collect();
    let keyed: Vec<(String, String, f64)> = rows
        .iter()
        .map(|row| (row.id.clone(), row.name.clone().unwrap_or_default(), row.sort_order.unwrap_or(0.)))
        .collect();
    domain::agent_settings::assignee_options(&[], &keyed)
        .into_iter()
        .filter_map(|id| id.strip_prefix(domain::agent_settings::ASSIGNEE_AGENT_PREFIX).map(str::to_string))
        .filter_map(|id| rows.iter().find(|row| row.id == id))
        .map(domain::agent_settings::AgentActorRow::of)
        .collect()
}

/// #EXP-1383 (agent-settings.json `actor`): every synced agent, the rows an
/// agent actor / assignee resolves against (`issue_actor`).
pub fn agent_actor_rows(cx: &App) -> Vec<domain::agent_settings::AgentActorRow> {
    Store::global(cx)
        .collections()
        .agents
        .read(cx)
        .iter()
        .map(domain::agent_settings::AgentActorRow::of)
        .collect()
}

/// #EXP-1383: one synced agent as an actor / assignee, `None` when its row
/// is gone (deleted, or not synced).
pub fn agent_actor(cx: &App, agent_id: &str) -> Option<domain::agent_settings::AgentActorRow> {
    Store::try_global(cx)?
        .collections()
        .agents
        .read(cx)
        .get(agent_id)
        .map(domain::agent_settings::AgentActorRow::of)
}

/// A team's saved prompts from the synced `prompts` shape (NEW-2: prompts
/// replaced actions; the body never syncs), sorted `sortOrder` then `name`
/// (collections hydrate unordered — the server ordering must be re-applied
/// client-side). Second value = the shape reached readiness (for the
/// loading-vs-empty split).
pub fn team_prompts(cx: &App, team_id: &str) -> (Vec<api::prompts::Prompt>, bool) {
    let collections = Store::global(cx).collections();
    let collection = collections.prompts.read(cx);
    let mut out: Vec<api::prompts::Prompt> = collection
        .iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .map(api::prompts::from_row)
        .collect();
    out.sort_by(|a, b| {
        a.sort_order
            .total_cmp(&b.sort_order)
            .then_with(|| a.name.cmp(&b.name))
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
    // EXP-1092: `try_global` — with no Store (the styleguide's paint test)
    // the issue resolves to its builtin anchor, like an unsynced board.
    let Some(store) = cx.try_global::<Store>() else {
        return domain::statuses::constructed_default(issue.status);
    };
    let collections = store.collections();
    // S-3: a board outside the boards collection (another team's, not yet
    // synced) still resolves the issue's `status_id` through the row's own
    // team, so a Done issue never reads as its anchor's default.
    let team_id = collections
        .boards
        .read(cx)
        .get(&issue.board_id)
        .map(|board| board.team_id.clone())
        .or_else(|| {
            let status_id = issue.status_id.as_deref()?;
            collections
                .issue_statuses
                .read(cx)
                .iter()
                .find(|row| row.id == status_id)
                .map(|row| row.team_id.clone())
        });
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

impl InboxGroup {
}

/// One agent message (EXP-801): an issue-less `agent_message` row is its own
/// entry — never bundled, each is a distinct thing someone's agent said.
/// Click marks it read; there is nowhere to navigate.
pub struct MessageInboxEntry {
    pub item: domain::rows::Notification,
    /// The synced team's name (`None` when the team row hasn't synced).
    pub team_name: Option<String>,
}

impl MessageInboxEntry {
    pub fn unread(&self) -> usize {
        usize::from(self.item.read_at.is_none())
    }
}

/// One blocked run (EXP-980): an issue-less `session_blocked` row is its own
/// entry, like an agent message. Click marks it read and opens the run —
/// unless the run has been pruned (`session_id` NULL), when the row is still
/// worth reading but leads nowhere.
pub struct SessionInboxEntry {
    pub item: domain::rows::Notification,
    /// The synced team's name (`None` when the team row hasn't synced).
    pub team_name: Option<String>,
}

impl SessionInboxEntry {
    pub fn unread(&self) -> usize {
        usize::from(self.item.read_at.is_none())
    }

    /// The run to open, when it is still synced.
    pub fn session_id(&self) -> Option<&str> {
        self.item.session_id.as_deref().filter(|id| !id.is_empty())
    }
}

/// One inbox card — an issue group, an agent message or a blocked run.
/// Entries are interleaved newest-first by their latest item (web
/// `inbox-view.tsx` sorts all groups together).
pub enum InboxEntry {
    Issue(InboxGroup),
    Message(MessageInboxEntry),
    Session(SessionInboxEntry),
}

impl InboxEntry {
    pub fn unread(&self) -> usize {
        match self {
            InboxEntry::Issue(group) => group.unread,
            InboxEntry::Message(entry) => entry.unread(),
            InboxEntry::Session(entry) => entry.unread(),
        }
    }
}

/// The inbox read: is-ready gate + grouped notifications. The notifications
/// shape is already user-scoped server-side; like web, groups are NOT
/// team-filtered (the join to a synced issue+board is the only membership
/// requirement; issue-less rows need none).
pub struct InboxData {
    pub is_ready: bool,
    pub groups: Vec<InboxEntry>,
    /// Unread across ALL entries, issue-less ones included — the count the
    /// tool header/badge surfaces.
    pub total_unread: usize,
}

/// `inbox-view.tsx` grouping: notifications ⨝ issues ⨝ boards grouped by
/// issue, plus one entry per issue-less `agent_message` / `session_blocked`
/// row; group order = newest first item.
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

/// The issue-less kinds the inbox renders: agent messages (EXP-801) and
/// blocked runs (EXP-980). Any other issue-less kind is unknown-future and
/// skipped everywhere this is consulted (SLOP-4: the helpdesk's issue-less
/// `support_reply` rows are gone; `reporter_reply` is issue-scoped).
fn issueless_kind_renderable(kind: Option<&str>) -> bool {
    matches!(
        kind,
        Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE)
            | Some(domain::contract::NOTIFICATION_TYPE_SESSION_BLOCKED)
    )
}

/// EXP-699: the rail's Inbox dot — any unread notification the inbox can
/// render (issue-keyed rows need the issue AND its board synced, issue-less
/// rows count only as `agent_message` or `session_blocked`): the [`inbox`]
/// renderability rule without the grouping work.
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
                None => issueless_kind_renderable(notification.kind.as_deref()),
            }
        })
}

/// The pure grouping core of [`inbox`]. `resolve_issue` returns the synced
/// issue (only while its board is synced too); `resolve_team` returns a
/// synced team's name (`None` when the team row has not synced).
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
        Message(String),
        Session(String),
    }

    let mut order: Vec<Key> = Vec::new();
    let mut by_issue: HashMap<String, InboxGroup> = HashMap::new();
    let mut by_message: HashMap<String, MessageInboxEntry> = HashMap::new();
    let mut by_session: HashMap<String, SessionInboxEntry> = HashMap::new();
    for notification in notifications {
        let unread = notification.read_at.is_none();
        let Some(issue_id) = notification.issue_id.clone() else {
            // Issue-less rows are an agent's message (EXP-801) or a blocked
            // run (EXP-980) — one entry per row; any other issue-less kind is
            // unknown-future and skipped.
            if !issueless_kind_renderable(notification.kind.as_deref()) {
                continue;
            }
            let team_name = notification.team_id.as_deref().and_then(&resolve_team);
            if notification.kind.as_deref()
                == Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE)
            {
                order.push(Key::Message(notification.id.clone()));
                by_message.insert(
                    notification.id.clone(),
                    MessageInboxEntry {
                        item: notification,
                        team_name,
                    },
                );
                continue;
            }
            // `issueless_kind_renderable` left only the blocked run.
            order.push(Key::Session(notification.id.clone()));
            by_session.insert(
                notification.id.clone(),
                SessionInboxEntry {
                    item: notification,
                    team_name,
                },
            );
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
            Key::Message(id) => by_message.remove(&id).map(InboxEntry::Message),
            Key::Session(id) => by_session.remove(&id).map(InboxEntry::Session),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Session hosts (EXP-1294: ONE `sessions` tree; reads of the tree itself live
// in `session_data`)
// ---------------------------------------------------------------------------

/// EXP-549/550: how a session names its host machine and whether that
/// machine is offline, resolved against the synced `devices` rows so a
/// rename shows up everywhere and a lid-closed host stops reading as live.
pub(crate) struct SessionDevicePresentation {
    /// The resolved `devices.label`; `None` when no row resolves.
    pub label: Option<String>,
    /// The resolved machine has not heartbeat inside the contract's online
    /// window. Always `false` when no `devices` row resolves — an unknown
    /// machine is never accused of being offline.
    pub offline: bool,
}

/// Resolve `session`'s host machine against the synced `devices` rows
/// ([`session_device_row`]).
pub(crate) fn session_device_presentation<'a>(
    session: &SessionRow,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    now_ms: i64,
) -> SessionDevicePresentation {
    match session_device_row(session, devices) {
        Some(row) => SessionDevicePresentation {
            label: row.label.clone().filter(|label| !label.trim().is_empty()),
            offline: !crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
        },
        None => SessionDevicePresentation {
            label: None,
            offline: false,
        },
    }
}

/// The synced `devices` row hosting `session`: the exact `device_id` match,
/// the starter's own row when a shared device produced several, and `None`
/// for a session not placed on a device yet or an unknown machine. Never a
/// label match.
pub(crate) fn session_device_row<'a>(
    session: &SessionRow,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
) -> Option<&'a domain::rows::DeviceRow> {
    let device_id = session.device_id.as_deref().filter(|id| !id.is_empty())?;
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

// ---------------------------------------------------------------------------
// Launch device candidates (EXP-696)
// ---------------------------------------------------------------------------

/// EXP-696: one machine the Start-coding dialog can start a run on — this
/// IDE itself, one of the caller's other registered devices, or a teammate's
/// shared server.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LaunchDevice {
    /// The steer TEXT id (`devices.device_id`) — `steer.startSession`'s
    /// target, and what this install compares itself against.
    pub(crate) device_id: String,
    /// The SYNCED row's uuid — the `device_worktrees` join key. Empty for a
    /// local entry whose row has not synced yet.
    pub(crate) row_id: String,
    /// The picker's line, [`launch_device_label`]-formatted.
    pub(crate) label: String,
    /// The agent CLIs the machine can actually run right now.
    pub(crate) agents: Vec<coding::CodingAgent>,
    /// EXP-749: the subset of [`Self::agents`] that runs on the SESSION
    /// SCREEN there; EXP-773 left no fallback, so the rest cannot start a
    /// run there at all. Every machine advertises it on register and on
    /// every heartbeat.
    pub(crate) acp_agents: Vec<coding::CodingAgent>,
    /// Its published launch defaults, clamped onto a default `Settings`
    /// (the same clamp `device_settings::baseline_for` runs for remote rows).
    pub(crate) defaults: coding::Settings,
    /// This IDE — the local launch paths, not `steer.startSession`.
    pub(crate) is_own: bool,
    /// EXP-622: the caller's default machine (never a teammate's flag).
    pub(crate) is_default: bool,
}

/// The picker line for a candidate machine — web `launch-options-pane`
/// parity (`"<label> — <owner>"` for a teammate's shared server), plus the
/// desktop-only "This device" marker for the machine the IDE runs on.
pub(crate) fn launch_device_label(
    label: &str,
    device_id: &str,
    owner: Option<&str>,
    is_own: bool,
) -> String {
    let base = match label.trim() {
        "" => device_id,
        label => label,
    };
    if is_own {
        return format!("{base} — This device");
    }
    match owner.map(str::trim).filter(|owner| !owner.is_empty()) {
        Some(owner) => format!("{base} — {owner}"),
        None => base.to_string(),
    }
}

/// The launch defaults a device advertises, clamped onto a default
/// [`coding::Settings`]. The synced jsonb may itself arrive as a JSON
/// STRING (§5.5 TEXT storage) — parse through that before the patch.
pub(crate) fn device_launch_settings(value: Option<&serde_json::Value>) -> coding::Settings {
    let mut settings = coding::Settings::default();
    let Some(value) = value else {
        return settings;
    };
    let parsed = match value {
        serde_json::Value::String(raw) => serde_json::from_str::<serde_json::Value>(raw).ok(),
        other => Some(other.clone()),
    };
    if let Some(patch) =
        parsed.and_then(|value| serde_json::from_value::<coding::DefaultsPatch>(value).ok())
    {
        coding::apply_defaults_patch(&mut settings, &patch);
    }
    settings
}

/// EXP-696: the REMOTE candidates for the launch device picker — every
/// synced row that is ONLINE and advertises at least one runnable agent, and
/// nothing else (web `launch-dialog.tsx`'s `candidateDevices`: EXP-639 made
/// the whole fleet above the version floor advertise the per-tab caps, so
/// only those two filters remain and `steer.startSession` re-checks the
/// agent server-side). `own_device_id` is dropped here — this IDE is added
/// by [`launch_devices`] from LOCAL state, so it stays a candidate even
/// before its own row has synced.
///
/// S-3: `now_ms` = `None` while the `devices` shape has not caught up in
/// this process (the rows are the SQLite cache's, their heartbeats as old as the last quit):
/// a heartbeat is then UNKNOWN, not offline, so no row is dropped for it —
/// the picker lists the other machines at first paint instead of seconds
/// later, and `steer.startSession` still refuses an offline one.
///
/// #EXP-1440: scoped to `team_id`, one of the caller's own machines shared
/// only with OTHER teams is theirs and never listed
/// ([`domain::rows::DeviceRow::is_usable_in_team`]).
pub(crate) fn remote_launch_devices<'a>(
    rows: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    now_ms: Option<i64>,
    team_id: Option<&str>,
    me_user_id: &str,
    own_device_id: &str,
    owner_name: &dyn Fn(&str) -> Option<String>,
) -> Vec<LaunchDevice> {
    let mut devices: Vec<LaunchDevice> = rows
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            if device_id == own_device_id {
                return None;
            }
            if now_ms.is_some_and(|now_ms| {
                !crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms)
            }) {
                return None;
            }
            let agents: Vec<coding::CodingAgent> = coding::CodingAgent::ALL
                .into_iter()
                .filter(|agent| row.agent_ids().iter().any(|id| id == agent.id()))
                .collect();
            if agents.is_empty() {
                return None;
            }
            // EXP-749: never a filter — the picker offers the agent either
            // way and names the transport.
            let ready = row.acp_agent_ids();
            let acp_agents: Vec<coding::CodingAgent> = coding::CodingAgent::ALL
                .into_iter()
                .filter(|agent| ready.iter().any(|id| id == agent.id()))
                .collect();
            let owned = row.user_id.as_deref() == Some(me_user_id);
            if owned && team_id.is_some_and(|team_id| !row.is_usable_in_team(team_id)) {
                return None;
            }
            let owner = (!owned)
                .then(|| row.user_id.as_deref().and_then(owner_name))
                .flatten();
            Some(LaunchDevice {
                label: launch_device_label(
                    row.label.as_deref().unwrap_or_default(),
                    &device_id,
                    owner.as_deref(),
                    false,
                ),
                row_id: row.id.clone(),
                agents,
                acp_agents,
                defaults: device_launch_settings(row.launch_defaults.as_ref()),
                is_own: false,
                // EXP-622: a teammate's flag is THEIR preference, never ours.
                is_default: owned && row.is_default.unwrap_or(false),
                device_id,
            })
        })
        .collect();
    // Every candidate is online, so a plain label order is stable (EXP-623 —
    // heartbeats must not reorder a picker the user is reading).
    devices.sort_by(|a, b| {
        a.label
            .to_lowercase()
            .cmp(&b.label.to_lowercase())
            .then_with(|| a.device_id.cmp(&b.device_id))
    });
    devices
}

struct OwnDeviceIdGlobal(String);
impl gpui::Global for OwnDeviceIdGlobal {}

/// EXP-696: this install's steer device id, resolved ONCE per process.
/// `steer::persistent_device_id` READS (and on a first run writes)
/// settings.json, so it must never sit on a render pass — the machines list
/// asks for it every frame.
pub(crate) fn own_device_id(cx: &mut App) -> String {
    if let Some(global) = cx.try_global::<OwnDeviceIdGlobal>() {
        return global.0.clone();
    }
    let id = steer::persistent_device_id(&AuthContext::global(cx).data_dir);
    cx.set_global(OwnDeviceIdGlobal(id.clone()));
    id
}

/// How long a FAILED `steer.config` fetch stays un-rearmed. The error path
/// notifies, every observer's handler reads the cache again, and an ungated
/// re-arm turns that into one fetch per notify while the network is down.
const STEER_CONFIG_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

/// EXP-696: the process-wide `steer.config` answer. Remote start exists only
/// when the instance runs a relay, and that is INSTANCE config (it moves on a
/// deploy, not on a heartbeat) — so it is fetched once and cached here.
///
/// EXP-1164: and REMEMBERED across launches ([`STEER_CONFIG_FILE`], per
/// instance URL). Every remote machine in the launch device picker sits
/// behind this answer, so a cold cache showed only "This device" until the
/// fetch landed — the default machine (another one) appeared seconds later,
/// or 30s later after a failed first fetch. The last known answer seeds the
/// cache at once and the fetch still runs to correct it.
#[derive(Default)]
pub(crate) struct SteerConfigCache {
    /// `None` until the first answer lands (or a remembered one seeds it);
    /// `Some(false)` = relay off, which is a normal state and never an error
    /// (§8.2).
    pub(crate) enabled: Option<bool>,
    /// EXP-1164: the instance the cache speaks for. Another account's
    /// instance starts the cache over (and an answer for the old one that
    /// lands late is dropped).
    instance: Option<String>,
    requested: bool,
    /// When the last fetch failed — the [`STEER_CONFIG_RETRY`] cooldown's
    /// anchor. Cleared by a successful answer.
    failed_at: Option<std::time::Instant>,
}

impl SteerConfigCache {
    /// Whether a fetch may be armed at `now`: nothing in flight (or already
    /// answered), and a failed attempt has cooled down.
    fn may_request(&self, now: std::time::Instant) -> bool {
        if self.requested {
            return false;
        }
        match self.failed_at {
            Some(failed_at) => now.duration_since(failed_at) >= STEER_CONFIG_RETRY,
            None => true,
        }
    }
}

struct SteerConfigGlobal(gpui::Entity<SteerConfigCache>);
impl gpui::Global for SteerConfigGlobal {}

/// EXP-1164: the last known `steer.config.enabled` per instance URL, in the
/// auth data dir — a server fact remembered, never a preference.
const STEER_CONFIG_FILE: &str = "steer-config.json";

fn steer_config_file(cx: &App) -> Option<std::path::PathBuf> {
    Some(cx.try_global::<AuthContext>()?.data_dir.join(STEER_CONFIG_FILE))
}

/// The remembered answer for `instance`; a missing or unreadable file is
/// simply "not known yet".
fn load_known_steer_enabled(path: &std::path::Path, instance: &str) -> Option<bool> {
    let json = std::fs::read_to_string(path).ok()?;
    let known: BTreeMap<String, bool> = serde_json::from_str(&json).ok()?;
    known.get(instance).copied()
}

/// Remember `enabled` for `instance`, keeping every other instance's answer.
/// Best effort: a failed write only costs the next launch's first paint.
fn store_known_steer_enabled(path: &std::path::Path, instance: &str, enabled: bool) {
    let mut known: BTreeMap<String, bool> = std::fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    if known.insert(instance.to_string(), enabled) == Some(enabled) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&known) {
        let _ = std::fs::write(path, json);
    }
}

/// The cache entity — surfaces observe it so the answer landing re-renders
/// them. Reading it also kicks the ONE fetch (a failed one re-arms, so a
/// transport blip cannot pin the app to "local only" for the session).
/// EXP-1164: a cache meeting an instance for the first time seeds itself
/// from the remembered answer before that fetch.
pub(crate) fn steer_config(cx: &mut App) -> gpui::Entity<SteerConfigCache> {
    let entity = match cx.try_global::<SteerConfigGlobal>() {
        Some(global) => global.0.clone(),
        None => {
            let entity = cx.new(|_| SteerConfigCache::default());
            cx.set_global(SteerConfigGlobal(entity.clone()));
            entity
        }
    };
    let instance = active_account(cx).map(|account| account.instance_url);
    if let Some(instance) = instance.as_ref() {
        if entity.read(cx).instance.as_ref() != Some(instance) {
            let known = steer_config_file(cx)
                .and_then(|path| load_known_steer_enabled(&path, instance));
            entity.update(cx, |this, _| {
                *this = SteerConfigCache {
                    enabled: known,
                    instance: Some(instance.clone()),
                    ..Default::default()
                };
            });
        }
    }
    if entity.read(cx).may_request(std::time::Instant::now()) {
        if let Some(trpc) = trpc_client(cx) {
            entity.update(cx, |this, _| this.requested = true);
            let entity = entity.clone();
            let path = steer_config_file(cx);
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::steer::config(&trpc) })
                    .await;
                let _ = entity.update(cx, |this, cx| {
                    // EXP-1164: the account moved to another instance while
                    // this was in flight — not its answer.
                    if this.instance != instance {
                        return;
                    }
                    match result {
                        Ok(config) => {
                            this.enabled = Some(config.enabled);
                            this.failed_at = None;
                            if let (Some(path), Some(instance)) = (path.as_ref(), instance.as_ref()) {
                                store_known_steer_enabled(path, instance, config.enabled);
                            }
                        }
                        Err(err) => {
                            log::warn!("[ui] steer.config failed: {err}");
                            // Re-armed by the next read AFTER the cooldown —
                            // this very notify re-enters `steer_config` from
                            // every observer, so an ungated re-arm would
                            // storm the transport (see `may_request`).
                            this.requested = false;
                            this.failed_at = Some(std::time::Instant::now());
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
    entity
}

/// Whether this instance can start runs on ANOTHER machine right now. Unknown
/// (not yet answered, or the fetch failed) reads FALSE: a picker that cannot
/// start anything is worse than no picker (EXP-696 — relay off must look
/// exactly like the app did before remote start existed).
pub(crate) fn remote_start_enabled(cx: &mut App) -> bool {
    steer_config(cx).read(cx).enabled.unwrap_or(false)
}

/// EXP-696: the launch device candidates, THIS machine first. The local
/// entry is built from local state (the hub's settings + the doctor's
/// runnable agents) rather than from its synced row: the IDE can always
/// start a run on itself, whether or not its heartbeat has landed. Other
/// machines only ever join the list while the instance runs a steer relay.
/// #EXP-1440: scoped to `team_id`, a machine of the caller's shared only
/// with OTHER teams (this one included) is theirs and never listed.
pub(crate) fn launch_devices(team_id: Option<&str>, cx: &mut App) -> Vec<LaunchDevice> {
    let own_device_id = own_device_id(cx);
    let hub = crate::coding_flow::CodingHub::global(cx);
    let (settings, report) = {
        let hub = hub.read(cx);
        (hub.settings.clone(), hub.doctor.report.clone())
    };
    let mut agents = report
        .as_ref()
        .map(|report| report.installed_agents())
        .unwrap_or_default();
    // EXP-749: the same subset this machine advertises to `devices.register`.
    let mut own_acp_agents: Vec<coding::CodingAgent> = report
        .as_ref()
        .map(|report| {
            report
                .installed_agents()
                .into_iter()
                .filter(|agent| report.check_for(*agent).acp == Some(true))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if agents.is_empty() {
        // The doctor has not landed (or nothing is signed in) — the LOCAL
        // gate is `launch_blocker`'s per-agent doctor check, which names the
        // real reason; the picker must not hide this machine meanwhile, and
        // must not caption an unprobed agent as unable to run either.
        agents = coding::CodingAgent::ALL.to_vec();
        own_acp_agents = coding::CodingAgent::ALL.to_vec();
    }
    let remote_enabled = remote_start_enabled(cx);
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    let own_row = collections
        .devices
        .read(cx)
        .iter()
        .find(|row| row.device_id.as_deref() == Some(own_device_id.as_str()))
        .cloned();
    let own = LaunchDevice {
        label: launch_device_label(
            own_row
                .as_ref()
                .and_then(|row| row.label.as_deref())
                .unwrap_or(""),
            &own_device_id,
            None,
            true,
        ),
        row_id: own_row.as_ref().map(|row| row.id.clone()).unwrap_or_default(),
        agents,
        acp_agents: own_acp_agents,
        defaults: settings,
        is_own: true,
        is_default: own_row
            .as_ref()
            .and_then(|row| row.is_default)
            .unwrap_or(false),
        device_id: own_device_id.clone(),
    };
    let me = active_account(cx).map(|account| account.user_id).unwrap_or_default();
    let users = collections.users.read(cx);
    let owner_name = |user_id: &str| {
        users
            .get(user_id)
            .and_then(|user| user.name.clone())
            .filter(|name| !name.is_empty())
    };
    let own_usable = own_row.as_ref().is_none_or(|row| {
        team_id.is_none_or(|team_id| row.is_usable_in_team(team_id))
    });
    let mut devices = if own_usable { vec![own] } else { Vec::new() };
    if remote_enabled {
        let rows = collections.devices.read(cx);
        devices.extend(remote_launch_devices(
            rows.iter(),
            rows.is_caught_up().then(|| chrono::Utc::now().timestamp_millis()),
            team_id,
            &me,
            &own_device_id,
            &owner_name,
        ));
    }
    devices
}

/// EXP-696/EXP-836: which machine a device settle lands on, returning its
/// `device_id`. One PURE precedence rule, derived on every settle — web
/// `lib/launch-device.ts` `resolveLaunchDeviceId`, with the desktop's own
/// machine folded into the fallback chain:
///
///   1. an explicit REQUEST (a play button's `?device=`, `ChatSeed.device_id`)
///      the moment that machine is a candidate — it outranks everything,
///      including a settle that already happened: the composer screen is
///      LONG-LIVED here, so a seed always arrives at an already-settled
///      screen, and reading the settled value first (what this did before)
///      made the default machine win every ▶;
///   2. the PERSON's pick from the Device pin;
///   3. their DEFAULT machine (EXP-622 — uniform across platforms);
///   4. this machine;
///   5. the first candidate.
///
/// A request is ONE-SHOT: a person's pick clears it (`set_device`), and it is
/// never persisted as a default either — only `devices.setDefault` writes that
/// flag. While the requested machine is NOT a candidate (still syncing, went
/// offline, every agent signed out there) the chain falls through to the
/// fallback and [`requested_device_note`] says why, which is also what lets a
/// late-hydrating row still win.
///
/// `picked` is STICKY by definition: it survives its machine dropping out of
/// the candidate list — one lapsed heartbeat must never silently retarget the
/// run at this machine with this machine's defaults. The composer blocks the
/// launch and names the offline machine instead (`offline_pick`), and the pick
/// resumes when it comes back.
pub(crate) fn settled_device(
    devices: &[LaunchDevice],
    requested: Option<&str>,
    picked: Option<&str>,
) -> Option<String> {
    let known = |id: &str| devices.iter().any(|device| device.device_id == id);
    if let Some(requested) = requested.filter(|id| known(id)) {
        return Some(requested.to_string());
    }
    if let Some(picked) = picked {
        return Some(picked.to_string());
    }
    devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.iter().find(|device| device.is_own))
        .or_else(|| devices.first())
        .map(|device| device.device_id.clone())
}

/// EXP-836: why the machine a ▶ REQUESTED cannot take the run, for the options
/// line under the composer — the run goes to the fallback machine, and saying
/// so beats silently starting somewhere else. Byte-identical to web
/// `use-launch-composer.ts` `deviceRequestNote`.
///
/// `None` when there is no request, when the requested machine IS a candidate,
/// or while its row has not synced yet (a machine that has not arrived is not a
/// missing one — and the request keeps outranking the default until it does).
pub(crate) fn requested_device_note<'a>(
    devices: &[LaunchDevice],
    requested: Option<&str>,
    mut rows: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    rows_synced: bool,
    now_ms: i64,
) -> Option<String> {
    let requested = requested?;
    if devices.iter().any(|device| device.device_id == requested) {
        return None;
    }
    if !rows_synced {
        return None;
    }
    let Some(row) = rows.find(|row| row.device_id.as_deref() == Some(requested)) else {
        return Some("That machine is no longer in your registry.".to_string());
    };
    let label = match row.label.as_deref().map(str::trim).filter(|label| !label.is_empty()) {
        Some(label) => label.to_string(),
        None => requested.to_string(),
    };
    if !crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms) {
        return Some(format!("{label} is offline."));
    }
    if row.agent_ids().is_empty() {
        return Some(format!("No agent is signed in on {label}."));
    }
    // Online, signed in, still not a candidate: remote start is off on this
    // install (`STEER_RELAY_URL` unset), which is a machine-wide setting, not
    // something about the requested machine.
    None
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

    #[test]
    fn inbox_interleaves_issueless_and_issue_groups_newest_first() {
        let entries = build_inbox_entries(
            vec![
                notification("n-old", Some("i-1"), None, "issue_comment", "2026-07-18T08:00:00Z", false),
                notification("n-message", None, Some("w-1"), "agent_message", "2026-07-18T09:00:00Z", false),
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
                InboxEntry::Message(_) => "message",
                InboxEntry::Session(_) => "session",
            })
            .collect();
        assert_eq!(kinds, ["i-2", "message", "i-1"]);
    }

    /// SLOP-4: a widget reporter's reply is issue-scoped — it groups under
    /// its issue like a comment, and an issue-less one (nothing writes those)
    /// is an unknown issue-less kind: dropped, never a Support group.
    #[test]
    fn inbox_groups_reporter_replies_under_their_issue() {
        let entries = build_inbox_entries(
            vec![
                notification("c-1", Some("i-1"), None, "issue_comment", "2026-07-18T09:00:00Z", true),
                notification("r-1", Some("i-1"), None, "reporter_reply", "2026-07-18T10:00:00Z", false),
                notification("r-2", None, Some("w-1"), "reporter_reply", "2026-07-18T11:00:00Z", false),
            ],
            |issue_id| Some(inbox_issue(issue_id)),
            |_| Some("Acme".to_string()),
        );
        assert_eq!(entries.len(), 1);
        let InboxEntry::Issue(group) = &entries[0] else {
            panic!("expected an Issue entry");
        };
        assert_eq!(group.items.len(), 2);
        assert_eq!(group.items[0].id, "r-1");
        assert_eq!(group.unread, 1);
    }

    /// EXP-801: an agent's message is one entry per row, never bundled,
    /// interleaved with the rest by its own timestamp, team name resolved.
    #[test]
    fn inbox_lists_agent_messages_one_entry_each() {
        let entries = build_inbox_entries(
            vec![
                notification("m-1", None, Some("w-1"), "agent_message", "2026-07-18T10:00:00Z", false),
                notification("b-1", None, Some("w-1"), "session_blocked", "2026-07-18T09:30:00Z", false),
                notification("m-2", None, Some("w-gone"), "agent_message", "2026-07-18T09:00:00Z", true),
            ],
            |_| None,
            |team_id| (team_id == "w-1").then(|| "Acme".to_string()),
        );
        assert_eq!(entries.len(), 3);
        let InboxEntry::Message(first) = &entries[0] else {
            panic!("expected a Message entry");
        };
        assert_eq!(first.item.id, "m-1");
        assert_eq!(first.team_name.as_deref(), Some("Acme"));
        assert_eq!(first.unread(), 1);
        assert!(matches!(&entries[1], InboxEntry::Session(_)));
        let InboxEntry::Message(second) = &entries[2] else {
            panic!("expected a Message entry");
        };
        assert_eq!(second.item.id, "m-2");
        assert_eq!(second.team_name, None);
        assert_eq!(second.unread(), 0);
        assert_eq!(entries.iter().map(InboxEntry::unread).sum::<usize>(), 2);
    }

    /// EXP-980: a blocked run's row is issue-less and names its run — one
    /// entry each, like an agent message, and a pruned run (`session_id`
    /// NULL) still renders rather than vanishing.
    #[test]
    fn inbox_lists_blocked_runs_one_entry_each() {
        let blocked = |id: &str, session_id: Option<&str>, created_at: &str, read: bool| {
            let mut row =
                notification(id, None, Some("w-1"), "session_blocked", created_at, read);
            row.session_id = session_id.map(str::to_string);
            row
        };
        let entries = build_inbox_entries(
            vec![
                blocked("b-1", Some("s-1"), "2026-07-18T10:00:00Z", false),
                notification("m-1", None, Some("w-1"), "agent_message", "2026-07-18T09:30:00Z", false),
                blocked("b-2", None, "2026-07-18T09:00:00Z", false),
            ],
            |_| None,
            |team_id| (team_id == "w-1").then(|| "Acme".to_string()),
        );
        assert_eq!(entries.len(), 3);
        let InboxEntry::Session(first) = &entries[0] else {
            panic!("expected a Session entry");
        };
        assert_eq!(first.item.id, "b-1");
        assert_eq!(first.session_id(), Some("s-1"));
        assert_eq!(first.team_name.as_deref(), Some("Acme"));
        assert!(matches!(&entries[1], InboxEntry::Message(_)));
        let InboxEntry::Session(second) = &entries[2] else {
            panic!("expected a Session entry");
        };
        // The run is gone; the row stays, it just leads nowhere.
        assert_eq!(second.session_id(), None);
        assert_eq!(entries.iter().map(InboxEntry::unread).sum::<usize>(), 3);
    }

    #[test]
    fn inbox_still_drops_unresolvable_issue_rows_and_unknown_issueless_kinds() {
        let entries = build_inbox_entries(
            vec![
                // Issue not synced (or its board trashed) — dropped.
                notification("n-1", Some("i-gone"), None, "issue_comment", "2026-07-18T10:00:00Z", false),
                // Issue-less row of an unknown future kind — dropped.
                notification("n-2", None, None, "mystery_kind", "2026-07-18T09:00:00Z", false),
            ],
            |_| None,
            |_| None,
        );
        assert!(entries.is_empty());
    }

    // -- EXP-696 launch device candidates -------------------------------------

    const NOW_MS: i64 = 1_754_900_000_000;

    fn seen(offset_secs: i64) -> String {
        chrono::DateTime::from_timestamp(NOW_MS / 1_000 + offset_secs, 0)
            .unwrap()
            .to_rfc3339()
    }

    fn launch_device_row(
        id: &str,
        device_id: &str,
        label: &str,
        user_id: &str,
        agents: &[&str],
        offset_secs: i64,
    ) -> domain::rows::DeviceRow {
        serde_json::from_value(json!({
            "id": id,
            "device_id": device_id,
            "label": label,
            "user_id": user_id,
            "kind": "desktop",
            "agents": agents,
            "last_seen_at": seen(offset_secs),
        }))
        .unwrap()
    }

    #[test]
    fn launch_device_labels_mark_this_device_and_shared_owners() {
        assert_eq!(
            launch_device_label("buildbox", "dev-1", None, true),
            "buildbox — This device"
        );
        assert_eq!(
            launch_device_label("buildbox", "dev-1", Some("Ada"), false),
            "buildbox — Ada"
        );
        // No owner (one of the caller's own machines) = the bare name.
        assert_eq!(launch_device_label("buildbox", "dev-1", None, false), "buildbox");
        // A blank owner name never renders a dangling separator.
        assert_eq!(launch_device_label("buildbox", "dev-1", Some("  "), false), "buildbox");
        // An unnamed row falls back to its device id.
        assert_eq!(launch_device_label("  ", "dev-1", None, false), "dev-1");
    }

    #[test]
    fn remote_candidates_need_online_and_a_runnable_agent() {
        let rows = vec![
            launch_device_row("r-1", "dev-online", "Zeta", "me", &["claude"], -30),
            // Offline: the relay would refuse the start with device_offline.
            launch_device_row("r-2", "dev-offline", "Alpha", "me", &["claude"], -600),
            // EXP-409: online but every CLI is signed out — nothing runnable.
            launch_device_row("r-3", "dev-signed-out", "Beta", "me", &[], -30),
            launch_device_row("r-4", "dev-mate", "Gamma", "mate", &["codex"], -30),
            // This machine is added from LOCAL state, never from its row.
            launch_device_row("r-5", "dev-own", "Local", "me", &["claude"], -30),
        ];
        let owner = |user_id: &str| (user_id == "mate").then(|| "Ada".to_string());
        let devices = remote_launch_devices(rows.iter(), Some(NOW_MS), None, "me", "dev-own", &owner);
        let ids: Vec<&str> = devices
            .iter()
            .map(|device| device.device_id.as_str())
            .collect();
        // Sorted by label: "Gamma — Ada" before "Zeta".
        assert_eq!(ids, ["dev-mate", "dev-online"]);
        assert_eq!(devices[0].label, "Gamma — Ada");
        assert_eq!(devices[0].agents, vec![coding::CodingAgent::Codex]);
        assert!(devices.iter().all(|device| !device.is_own));
    }

    /// #EXP-1440: scoped to a team, an own machine shared only with OTHER
    /// teams is never listed; personal and team-shared ones are.
    #[test]
    fn remote_candidates_drop_own_machines_another_team_owns() {
        let personal = launch_device_row("r-1", "dev-personal", "Alpha", "me", &["claude"], -30);
        let mut here = launch_device_row("r-2", "dev-here", "Beta", "me", &["claude"], -30);
        here.shared_team_ids = vec!["team-1".into()];
        let mut theirs = launch_device_row("r-3", "dev-theirs", "Gamma", "me", &["claude"], -30);
        theirs.shared_team_ids = vec!["team-2".into()];
        let rows = vec![personal, here, theirs];
        let owner = |_: &str| None;
        let ids = |team: Option<&str>| -> Vec<String> {
            remote_launch_devices(rows.iter(), Some(NOW_MS), team, "me", "dev-own", &owner)
                .into_iter()
                .map(|device| device.device_id)
                .collect()
        };
        assert_eq!(ids(Some("team-1")), ["dev-personal", "dev-here"]);
        // Not team-scoped: every own machine stays.
        assert_eq!(ids(None), ["dev-personal", "dev-here", "dev-theirs"]);
    }

    /// EXP-749: a machine's ACP-ready subset rides the synced row into the
    /// candidate. A NULL column reads as none — and either way the agent
    /// stays offerable: the picker labels the transport, it never filters the
    /// strip.
    #[test]
    fn remote_launch_devices_carry_acp_agents_and_null_means_none() {
        let mut ready = launch_device_row("r-1", "dev-1", "Alpha", "me", &["claude", "codex"], -30);
        ready.acp_agents = Some(json!(["claude"]));
        let mut none_ready = launch_device_row("r-2", "dev-2", "Beta", "me", &["claude"], -30);
        none_ready.acp_agents = Some(json!([]));
        // No column at all: nothing there is claimed to speak ACP.
        let silent = launch_device_row("r-3", "dev-3", "Gamma", "me", &["claude"], -30);
        let rows = vec![ready, none_ready, silent];
        let owner = |_: &str| None;
        let devices = remote_launch_devices(rows.iter(), Some(NOW_MS), None, "me", "dev-own", &owner);

        assert_eq!(devices[0].device_id, "dev-1");
        assert_eq!(devices[0].acp_agents, vec![coding::CodingAgent::Claude]);
        // Never a filter: codex is still offered, the pill just says it
        // cannot run a session there.
        assert_eq!(
            devices[0].agents,
            vec![coding::CodingAgent::Claude, coding::CodingAgent::Codex]
        );
        // An EMPTY list is a real answer.
        assert_eq!(devices[1].device_id, "dev-2");
        assert!(devices[1].acp_agents.is_empty());
        assert_eq!(devices[1].agents, vec![coding::CodingAgent::Claude]);
        // A NULL column is the same answer, never "assume everything".
        assert_eq!(devices[2].device_id, "dev-3");
        assert!(devices[2].acp_agents.is_empty());
    }

    #[test]
    fn remote_candidate_defaults_come_from_the_advertised_launch_defaults() {
        let mut row = launch_device_row("r-1", "dev-1", "Buildbox", "me", &["codex"], -30);
        row.launch_defaults = Some(json!({
            "defaultAgent": "codex",
            "agents": {"codex": {"model": "gpt-5.6-terra", "effort": "high"}},
        }));
        let owner = |_: &str| None;
        let devices = remote_launch_devices(
            std::iter::once(&row),
            Some(NOW_MS),
            None,
            "me",
            "dev-own",
            &owner,
        );
        let settings = &devices[0].defaults;
        assert_eq!(settings.default_agent, coding::CodingAgent::Codex);
        assert_eq!(settings.codex_model, "gpt-5.6-terra");
        assert_eq!(settings.codex_effort, "high");
    }

    #[test]
    fn launch_defaults_parse_through_a_json_string_column() {
        // §5.5: a jsonb column can arrive as a JSON STRING.
        let raw = json!(r#"{"defaultAgent":"codex"}"#);
        assert_eq!(
            device_launch_settings(Some(&raw)).default_agent,
            coding::CodingAgent::Codex
        );
        // Absent / unparsable degrades to the static defaults, never panics.
        assert_eq!(
            device_launch_settings(None).default_agent,
            coding::Settings::default().default_agent
        );
        assert_eq!(
            device_launch_settings(Some(&json!("not json"))).default_agent,
            coding::Settings::default().default_agent
        );
    }

    fn candidate(device_id: &str, is_own: bool, is_default: bool) -> LaunchDevice {
        LaunchDevice {
            device_id: device_id.to_string(),
            row_id: format!("row-{device_id}"),
            label: device_id.to_string(),
            agents: vec![coding::CodingAgent::Claude],
            acp_agents: vec![coding::CodingAgent::Claude],
            defaults: coding::Settings::default(),
            is_own,
            is_default,
        }
    }

    /// EXP-622 parity: a settle with NO request and NO pick prefers the user's
    /// DEFAULT machine over this one, exactly like web's
    /// `defaultCandidateId(devices) ?? devices[0]` — then this machine, then the
    /// first candidate.
    #[test]
    fn device_settle_prefers_the_users_default_machine() {
        let with_default = vec![
            candidate("dev-own", true, false),
            candidate("dev-build", false, true),
        ];
        assert_eq!(
            settled_device(&with_default, None, None).as_deref(),
            Some("dev-build")
        );
        // No default flagged anywhere: this machine.
        let no_default = vec![candidate("dev-mate", false, false), candidate("dev-own", true, false)];
        assert_eq!(settled_device(&no_default, None, None).as_deref(), Some("dev-own"));
        // Neither (this machine's entry has not been built): the first row.
        let foreign = vec![candidate("dev-mate", false, false)];
        assert_eq!(settled_device(&foreign, None, None).as_deref(), Some("dev-mate"));
        assert_eq!(settled_device(&[], None, None), None);
    }

    /// EXP-836: the whole precedence, as the web rule — a ▶'s REQUEST beats the
    /// person's pick beats the default beats this machine beats the first row.
    #[test]
    fn a_requested_machine_outranks_the_pick_and_the_default() {
        let devices = vec![
            candidate("dev-own", true, false),
            candidate("dev-build", false, true),
            candidate("dev-mate", false, false),
        ];
        // Request alone, and request over a pick.
        assert_eq!(
            settled_device(&devices, Some("dev-mate"), None).as_deref(),
            Some("dev-mate")
        );
        assert_eq!(
            settled_device(&devices, Some("dev-mate"), Some("dev-own")).as_deref(),
            Some("dev-mate")
        );
        // A pick beats the default…
        assert_eq!(
            settled_device(&devices, None, Some("dev-own")).as_deref(),
            Some("dev-own")
        );
        // …and the default beats this machine.
        assert_eq!(settled_device(&devices, None, None).as_deref(), Some("dev-build"));
        // A request for a machine that is not a candidate (still syncing, gone
        // offline) falls through to the rest of the chain — the request is KEPT
        // by the caller, so it wins the moment the row arrives, and
        // `requested_device_note` explains the gap meanwhile.
        assert_eq!(
            settled_device(&devices, Some("dev-syncing"), None).as_deref(),
            Some("dev-build")
        );
        assert_eq!(
            settled_device(&devices, Some("dev-syncing"), Some("dev-own")).as_deref(),
            Some("dev-own")
        );
        assert_eq!(settled_device(&[], Some("dev-syncing"), None), None);
    }

    /// EXP-836 regression: the composer screen is LONG-LIVED, so every seed
    /// lands on an already-settled screen. The settle is derived from the
    /// request and the pick ALONE — never from what it settled on last time —
    /// or the default machine the first settle landed on would win every ▶.
    #[test]
    fn a_seed_still_wins_on_an_already_settled_screen() {
        let devices = vec![
            candidate("dev-own", true, false),
            candidate("dev-build", false, true),
            candidate("dev-mate", false, false),
        ];
        // First settle (screen opened): the default machine.
        let settled = settled_device(&devices, None, None);
        assert_eq!(settled.as_deref(), Some("dev-build"));
        // The ▶ arrives afterwards: its machine takes the run.
        assert_eq!(
            settled_device(&devices, Some("dev-mate"), None).as_deref(),
            Some("dev-mate")
        );
        // The person then picks another one (which drops the request).
        assert_eq!(
            settled_device(&devices, None, Some("dev-own")).as_deref(),
            Some("dev-own")
        );
    }

    /// EXP-696: the PERSON's pick is sticky — it survives its machine leaving
    /// the candidate list (a lapsed heartbeat), and the composer blocks the
    /// launch naming that machine instead of silently retargeting this one.
    #[test]
    fn device_settle_keeps_the_persons_pick_when_its_machine_drops_out() {
        let devices = vec![
            candidate("dev-own", true, false),
            candidate("dev-build", false, true),
        ];
        assert_eq!(
            settled_device(&devices, None, Some("dev-build")).as_deref(),
            Some("dev-build")
        );
        assert_eq!(
            settled_device(&devices, None, Some("dev-gone")).as_deref(),
            Some("dev-gone")
        );
    }

    /// EXP-836: what the options line says while the requested machine is not a
    /// candidate — byte-identical to web `deviceRequestNote`.
    #[test]
    fn the_request_note_names_why_that_machine_cannot_take_the_run() {
        let devices = vec![candidate("dev-own", true, false)];
        let rows = vec![
            launch_device_row("r-1", "dev-offline", "Mint", "me", &["claude"], -600),
            launch_device_row("r-2", "dev-signed-out", "Air", "me", &[], -30),
            launch_device_row("r-3", "dev-ready", "Zeta", "me", &["claude"], -30),
        ];
        let note = |requested: Option<&str>, synced: bool| {
            requested_device_note(&devices, requested, rows.iter(), synced, NOW_MS)
        };
        assert_eq!(note(Some("dev-offline"), true).as_deref(), Some("Mint is offline."));
        assert_eq!(
            note(Some("dev-signed-out"), true).as_deref(),
            Some("No agent is signed in on Air.")
        );
        assert_eq!(
            note(Some("dev-unknown"), true).as_deref(),
            Some("That machine is no longer in your registry.")
        );
        // No request, or a request that IS a candidate: nothing to say.
        assert_eq!(note(None, true), None);
        assert_eq!(note(Some("dev-own"), true), None);
        // Still syncing: a machine that has not arrived is not a missing one.
        assert_eq!(note(Some("dev-unknown"), false), None);
        // Online and signed in but not a candidate (remote start off here) —
        // that is about this install, not about that machine.
        assert_eq!(note(Some("dev-ready"), true), None);
    }

    /// A FAILED `steer.config` fetch re-arms only after its cooldown — the
    /// error path notifies, and every observer reading the cache back would
    /// otherwise re-spawn the fetch on the spot.
    #[test]
    fn steer_config_refetch_waits_out_the_cooldown() {
        let start = std::time::Instant::now();
        let fresh = SteerConfigCache::default();
        assert!(fresh.may_request(start));
        // In flight: never a second fetch.
        let in_flight = SteerConfigCache {
            requested: true,
            ..Default::default()
        };
        assert!(!in_flight.may_request(start));
        let failed = SteerConfigCache {
            failed_at: Some(start),
            ..Default::default()
        };
        assert!(!failed.may_request(start));
        assert!(!failed.may_request(start + STEER_CONFIG_RETRY - std::time::Duration::from_secs(1)));
        assert!(failed.may_request(start + STEER_CONFIG_RETRY));
    }

    /// EXP-1164: the remembered `steer.config` answer is PER INSTANCE, an
    /// answer for one never clobbers another's, and a missing or garbage
    /// file reads as "not known yet" (the fetch decides), never a panic.
    #[test]
    fn the_remembered_steer_config_is_per_instance() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir()
            .join(format!("exp-steer-config-{}-{stamp}", std::process::id()));
        let path = dir.join(STEER_CONFIG_FILE);
        assert_eq!(load_known_steer_enabled(&path, "https://a.example"), None);

        store_known_steer_enabled(&path, "https://a.example", true);
        store_known_steer_enabled(&path, "https://b.example", false);
        assert_eq!(load_known_steer_enabled(&path, "https://a.example"), Some(true));
        assert_eq!(load_known_steer_enabled(&path, "https://b.example"), Some(false));
        assert_eq!(load_known_steer_enabled(&path, "https://c.example"), None);

        // A relay switched off on a deploy: the next answer overwrites.
        store_known_steer_enabled(&path, "https://a.example", false);
        assert_eq!(load_known_steer_enabled(&path, "https://a.example"), Some(false));

        std::fs::write(&path, "not json").expect("write");
        assert_eq!(load_known_steer_enabled(&path, "https://a.example"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }


    // ── EXP-1294: session hosts ────────────────────────────────────────────

    fn host_session(id: &str, root_id: &str, status: &str, device_id: Option<&str>) -> SessionRow {
        serde_json::from_value(json!({
            "id": id,
            "root_id": root_id,
            "parent_id": (id != root_id).then_some(root_id),
            "kind": "thread",
            "user_id": "user-1",
            "status": status,
            "device_id": device_id,
        }))
        .unwrap()
    }

    fn host_device(device_id: &str, label: &str, last_seen_at: &str) -> domain::rows::DeviceRow {
        serde_json::from_value(json!({
            "id": format!("row-{device_id}"),
            "device_id": device_id,
            "user_id": "user-1",
            "label": label,
            "last_seen_at": last_seen_at,
        }))
        .unwrap()
    }

    /// The host resolves by `device_id` only; a lapsed heartbeat reads
    /// offline, an unknown machine never does.
    #[test]
    fn session_device_presentation_resolves_by_device_id() {
        let now_ms = 1_784_289_600_000_i64;
        let devices = vec![
            host_device("dev-1", "Studio", "2026-07-17T11:59:30Z"),
            host_device("dev-2", "Laptop", "2026-07-10T00:00:00Z"),
        ];
        let on = |device: Option<&str>| {
            session_device_presentation(&host_session("r1", "r1", "running", device), devices.iter(), now_ms)
        };
        let studio = on(Some("dev-1"));
        assert_eq!(studio.label.as_deref(), Some("Studio"));
        assert!(!studio.offline);
        let laptop = on(Some("dev-2"));
        assert_eq!(laptop.label.as_deref(), Some("Laptop"));
        assert!(laptop.offline);
        let unknown = on(Some("dev-9"));
        assert_eq!(unknown.label, None);
        assert!(!unknown.offline);
        assert!(on(None).label.is_none());
    }
}
