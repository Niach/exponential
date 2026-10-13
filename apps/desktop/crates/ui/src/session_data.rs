//! EXP-1294 — the read helpers over the ONE session tree.
//!
//! Five synced collections carry Threads v2: `sessions` (roots of kind
//! `thread` | `agent` and their child sessions), `session_issues` (root ↔
//! issue), `messages` (every conversation row and card), `pull_requests` and
//! `pull_request_issues`. Every screen reads them through here and feeds the
//! pure `domain::session_tree` / `domain::pr_stack` functions (the Rust
//! mirrors of `session-tree.ts`, fixture-replayed), so a view never filters a
//! collection itself (masterplan-v3 §4.1, the `queries.rs` rule).
//!
//! The functions taking `&App` are plain reads; the two list reads every rail
//! render asks for ([`thread_lists`], [`rail_threads_badge`]) memoise per
//! store change and take `&mut App`. Pure cores sit next to their `cx`
//! wrappers and carry the tests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use domain::contract::{SESSION_STATUS_QUEUED, SESSION_STATUS_RUNNING};
use domain::pr_stack::PrSet;
use domain::rows::{Issue, MessageRow, PullRequestIssueRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_tree::{
    build_session_thread, list_agents, partition_threads, session_card, thread_title, tree_order,
    AgentRow, SessionCard, SessionThreadInput, SessionThreadView, ThreadsListGroup, ThreadsListInput,
    ThreadsListKey, TreeIssue,
};
use gpui::App;
use sync::Store;

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// Now as epoch milliseconds — the `now_ms` every `domain::session_tree`
/// function takes.
pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------------------------------------------------------------------------
// Row predicates
// ---------------------------------------------------------------------------

/// A session the agent is working on right now or is about to: status
/// `queued` or `running` (an `idle` one waits on a person, an `ended` one is
/// over). The Running band's rule (`partition_threads`).
pub(crate) fn session_is_working(session: &SessionRow) -> bool {
    matches!(
        session.status.as_deref(),
        Some(SESSION_STATUS_QUEUED) | Some(SESSION_STATUS_RUNNING)
    )
}

/// A non-ended session that waits on a person (a question or a plan).
pub(crate) fn session_needs_you(session: &SessionRow) -> bool {
    session.needs_input && !session.is_ended()
}

/// `updated_at`, else `created_at` — ISO strings from one source sort
/// lexicographically.
fn activity_key(session: &SessionRow) -> &str {
    session
        .updated_at
        .as_deref()
        .or(session.created_at.as_deref())
        .unwrap_or("")
}

fn signed_in_user(cx: &App) -> Option<String> {
    crate::queries::active_account(cx).map(|account| account.user_id)
}

// ---------------------------------------------------------------------------
// Sessions and trees
// ---------------------------------------------------------------------------

/// One synced session row.
pub(crate) fn session(session_id: &str, cx: &App) -> Option<SessionRow> {
    let store = Store::try_global(cx)?;
    store.collections().sessions.read(cx).get(session_id).cloned()
}

/// The root of `session_id`'s tree (the row itself for a root); `None` while
/// either row has not synced.
pub(crate) fn root_of(session_id: &str, cx: &App) -> Option<SessionRow> {
    let store = Store::try_global(cx)?;
    let sessions = store.collections().sessions.read(cx);
    let row = sessions.get(session_id)?;
    if row.is_root() {
        return Some(row.clone());
    }
    sessions.get(row.root_id_or_self()).cloned()
}

/// The root plus every descendant, in the tree's creation order
/// (`domain::session_tree::tree_order`, the `Session N` ordinals). Empty
/// while the root has not synced.
pub(crate) fn tree_of_root(root_id: &str, cx: &App) -> Vec<SessionRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let sessions = store.collections().sessions.read(cx);
    if sessions.get(root_id).is_none() {
        return Vec::new();
    }
    let members: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .collect();
    tree_order(&members).into_iter().cloned().collect()
}

/// THE session card of one session, its ordinal taken from its tree.
pub(crate) fn session_card_of(session_id: &str, now_ms: i64, cx: &App) -> Option<SessionCard> {
    let store = Store::try_global(cx)?;
    let sessions = store.collections().sessions.read(cx);
    let row = sessions.get(session_id)?;
    let root_id = row.root_id_or_self();
    let tree: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .collect();
    let ordinal = domain::session_tree::tree_ordinal(session_id, &tree);
    Some(session_card(row, ordinal, now_ms))
}

// ---------------------------------------------------------------------------
// Issues
// ---------------------------------------------------------------------------

/// The issues linked to `root_id`, by link `sort_order`; unsynced issues drop
/// out.
pub(crate) fn issues_of_root(root_id: &str, cx: &App) -> Vec<Issue> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let mut links: Vec<&SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| link.session_id == root_id)
        .collect();
    sort_links(&mut links);
    links
        .into_iter()
        .filter_map(|link| issues.get(&link.issue_id).cloned())
        .collect()
}

/// [`issues_of_root`] as the tree views take them.
pub(crate) fn tree_issues_of_root(root_id: &str, cx: &App) -> Vec<TreeIssue> {
    issues_of_root(root_id, cx)
        .iter()
        .map(TreeIssue::from_issue)
        .collect()
}

fn sort_links(links: &mut [&SessionIssueRow]) {
    links.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(0.0)
            .total_cmp(&b.sort_order.unwrap_or(0.0))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// The thread roots an issue sits in (the issue's Threads row), newest
/// activity first.
pub(crate) fn roots_of_issue(issue_id: &str, cx: &App) -> Vec<SessionRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let mut roots: Vec<SessionRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| link.issue_id == issue_id)
        .filter_map(|link| sessions.get(&link.session_id))
        .filter(|root| root.is_root() && root.is_thread())
        .cloned()
        .collect();
    roots.sort_by(|a, b| {
        activity_key(b)
            .cmp(activity_key(a))
            .then_with(|| a.id.cmp(&b.id))
    });
    roots.dedup_by(|a, b| a.id == b.id);
    roots
}

/// The identifiers of the issues linked to `root_id`, link order — what the
/// device pickup names a branch / runbook after.
pub(crate) fn root_issue_identifiers(root_id: &str, cx: &App) -> Vec<String> {
    issues_of_root(root_id, cx)
        .into_iter()
        .map(|issue| issue.identifier)
        .collect()
}

/// The first linked issue's identifier — the mono lead of a list row.
pub(crate) fn first_issue_identifier(root_id: &str, cx: &App) -> Option<String> {
    issues_of_root(root_id, cx)
        .into_iter()
        .next()
        .map(|issue| issue.identifier)
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

fn sort_messages(messages: &mut [MessageRow]) {
    messages.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// The messages of ONE session (its own conversation), oldest first.
pub(crate) fn messages_of_session(session_id: &str, cx: &App) -> Vec<MessageRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let mut out: Vec<MessageRow> = store
        .collections()
        .messages
        .read(cx)
        .iter()
        .filter(|message| message.session_id == session_id)
        .cloned()
        .collect();
    sort_messages(&mut out);
    out
}

// ---------------------------------------------------------------------------
// Pull requests
// ---------------------------------------------------------------------------

fn newest_first(pulls: &mut [PullRequestRow]) {
    pulls.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
}

/// The pull requests that close `issue_id` (`pull_request_issues`), newest
/// first.
pub(crate) fn pull_requests_of_issue(issue_id: &str, cx: &App) -> Vec<PullRequestRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let pulls = collections.pull_requests.read(cx);
    let mut out: Vec<PullRequestRow> = collections
        .pull_request_issues
        .read(cx)
        .iter()
        .filter(|link| link.issue_id == issue_id)
        .filter_map(|link| pulls.get(&link.pull_request_id).cloned())
        .collect();
    newest_first(&mut out);
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// The pull requests opened from `root_id`'s tree (`root_session_id`),
/// newest first — the Guide's PRs.
pub(crate) fn pull_requests_of_root(root_id: &str, cx: &App) -> Vec<PullRequestRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let mut out: Vec<PullRequestRow> = store
        .collections()
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.root_session_id.as_deref() == Some(root_id))
        .cloned()
        .collect();
    newest_first(&mut out);
    out
}

/// Every OPEN pull request of `team_id`, newest first (the action `pr` pick,
/// the stack lookups).
pub(crate) fn open_pull_requests_in_team(team_id: &str, cx: &App) -> Vec<PullRequestRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let mut out: Vec<PullRequestRow> = store
        .collections()
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.team_id.as_deref() == Some(team_id) && pull.is_open())
        .cloned()
        .collect();
    newest_first(&mut out);
    out
}

/// One team's pull requests with their issue links and the linked issues —
/// what `domain::pr_stack::PrSet` borrows. Snapshot once, then
/// [`PrSnapshot::with_set`].
#[derive(Clone, Debug, Default)]
pub(crate) struct PrSnapshot {
    pub(crate) pulls: Vec<PullRequestRow>,
    pub(crate) links: Vec<PullRequestIssueRow>,
    pub(crate) issues: Vec<Issue>,
}

impl PrSnapshot {
    /// Run `f` over the snapshot as a [`PrSet`].
    pub(crate) fn with_set<R>(&self, f: impl FnOnce(&PrSet<'_>) -> R) -> R {
        let pulls: Vec<&PullRequestRow> = self.pulls.iter().collect();
        let links: Vec<&PullRequestIssueRow> = self.links.iter().collect();
        let issues: Vec<&Issue> = self.issues.iter().collect();
        f(&PrSet {
            pulls: &pulls,
            links: &links,
            issues: &issues,
        })
    }

    pub(crate) fn pull(&self, pull_request_id: &str) -> Option<&PullRequestRow> {
        self.pulls.iter().find(|pull| pull.id == pull_request_id)
    }
}

/// [`PrSnapshot`] of `team_id`.
pub(crate) fn team_pr_snapshot(team_id: &str, cx: &App) -> PrSnapshot {
    let Some(store) = Store::try_global(cx) else {
        return PrSnapshot::default();
    };
    let collections = store.collections();
    let pulls: Vec<PullRequestRow> = collections
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.team_id.as_deref() == Some(team_id))
        .cloned()
        .collect();
    let ids: HashSet<&str> = pulls.iter().map(|pull| pull.id.as_str()).collect();
    let links: Vec<PullRequestIssueRow> = collections
        .pull_request_issues
        .read(cx)
        .iter()
        .filter(|link| ids.contains(link.pull_request_id.as_str()))
        .cloned()
        .collect();
    let all_issues = collections.issues.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let issues: Vec<Issue> = links
        .iter()
        .filter(|link| seen.insert(link.issue_id.as_str()))
        .filter_map(|link| all_issues.get(&link.issue_id).cloned())
        .collect();
    PrSnapshot {
        pulls,
        links,
        issues,
    }
}

// ---------------------------------------------------------------------------
// The thread screen
// ---------------------------------------------------------------------------

/// The thread title of `root_id` (`domain::session_tree::thread_title`: the
/// stored title, the action, the first issue, the first message, else
/// `Untitled`); `None` while the root has not synced.
pub(crate) fn thread_title_of(root_id: &str, cx: &App) -> Option<String> {
    let root = session(root_id, cx)?;
    let issues = tree_issues_of_root(root_id, cx);
    let messages = messages_of_session(root_id, cx);
    let refs: Vec<&MessageRow> = messages.iter().collect();
    Some(thread_title(&root, &issues, &refs).title)
}

/// The thread screen's model for `root_id` (`build_session_thread`): the
/// tabs, the conversation rows, the composer target. `None` while the root
/// has not synced. Works for an agent root too (its channel).
pub(crate) fn session_thread_view(root_id: &str, now_ms: i64, cx: &App) -> Option<SessionThreadView> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let root = sessions.get(root_id)?;
    // The team's sessions, not just the tree: an agent channel's
    // `thread_started` cards name OTHER roots.
    let team_sessions: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.team_id == root.team_id || session.root_id_or_self() == root_id)
        .collect();
    let mut links: Vec<&SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| link.session_id == root_id)
        .collect();
    // One order everywhere (title, tabs, the list row's identifier).
    sort_links(&mut links);
    let messages: Vec<&MessageRow> = collections
        .messages
        .read(cx)
        .iter()
        .filter(|message| message.session_id == root_id)
        .collect();
    // The linked issues plus every issue an event card names: an unlinked
    // issue still reads "Unlinked EXP-12" (the event lines look it up).
    let all_issues = collections.issues.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let issues: Vec<TreeIssue> = links
        .iter()
        .map(|link| link.issue_id.as_str())
        .chain(messages.iter().filter_map(|message| {
            message
                .card
                .as_ref()?
                .get("issueId")?
                .as_str()
        }))
        .filter(|id| seen.insert(id))
        .filter_map(|id| all_issues.get(id))
        .map(TreeIssue::from_issue)
        .collect();
    let pulls: Vec<&PullRequestRow> = collections
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.team_id == root.team_id)
        .collect();
    Some(build_session_thread(SessionThreadInput {
        root,
        sessions: &team_sessions,
        session_issues: &links,
        issues: &issues,
        messages: &messages,
        pull_requests: &pulls,
        now_ms,
    }))
}

// ---------------------------------------------------------------------------
// The Threads page, the rail and the team picker
// ---------------------------------------------------------------------------

/// The rail's Agents section: the agent roots of `team_id`
/// (`domain::session_tree::list_agents`).
pub(crate) fn agents_in_team(team_id: &str, cx: &App) -> Vec<AgentRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let sessions = store.collections().sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions.iter().collect();
    // An agent's spinner / needs-you dot is its own row only: the threads
    // it spawns are their own roots (the Threads entry lights for them).
    list_agents(&rows, Some(team_id))
}

/// The store state one [`thread_lists`] answer derives from.
#[derive(Clone, PartialEq, Eq)]
struct ListsKey {
    team_id: String,
    revisions: [u64; 4],
}

#[derive(Default)]
struct ListsMemo(Option<(ListsKey, Rc<Vec<ThreadsListGroup>>)>);

impl gpui::Global for ListsMemo {}

/// The Threads page bands of `team_id` (`partition_threads`: Needs you ·
/// Running · Recent, empty bands omitted). Memoised per store change: the
/// rail's dot reads it on every render.
pub(crate) fn thread_lists(team_id: &str, cx: &mut App) -> Rc<Vec<ThreadsListGroup>> {
    let Some(store) = Store::try_global(cx) else {
        return Rc::new(Vec::new());
    };
    let collections = store.collections();
    let key = ListsKey {
        team_id: team_id.to_string(),
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.session_issues.read(cx).revision(),
            collections.messages.read(cx).revision(),
            collections.issues.read(cx).revision(),
        ],
    };
    if let Some((cached, lists)) = &cx.default_global::<ListsMemo>().0 {
        if *cached == key {
            return lists.clone();
        }
    }
    let lists = Rc::new(derive_thread_lists(team_id, cx));
    cx.global_mut::<ListsMemo>().0 = Some((key, lists.clone()));
    lists
}

fn derive_thread_lists(team_id: &str, cx: &App) -> Vec<ThreadsListGroup> {
    let collections = Store::global(cx).collections();
    let sessions = collections.sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .collect();
    let roots: HashSet<&str> = rows
        .iter()
        .filter(|session| session.is_root())
        .map(|session| session.id.as_str())
        .collect();
    let mut links: Vec<&SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| roots.contains(link.session_id.as_str()))
        .collect();
    sort_links(&mut links);
    let all_issues = collections.issues.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let issues: Vec<TreeIssue> = links
        .iter()
        .filter(|link| seen.insert(link.issue_id.as_str()))
        .filter_map(|link| all_issues.get(&link.issue_id))
        .map(TreeIssue::from_issue)
        .collect();
    // Only a root's own messages can title it.
    let messages: Vec<&MessageRow> = collections
        .messages
        .read(cx)
        .iter()
        .filter(|message| roots.contains(message.session_id.as_str()))
        .collect();
    partition_threads(ThreadsListInput {
        sessions: &rows,
        session_issues: &links,
        issues: &issues,
        messages: &messages,
        team_id: Some(team_id),
    })
}

/// The rail Threads entry's dot: `(needs_you, running)` — some thread of
/// `team_id` waits on a person / is queued or running.
pub(crate) fn rail_threads_badge(team_id: &str, cx: &mut App) -> (bool, bool) {
    threads_badge_of(&thread_lists(team_id, cx))
}

fn threads_badge_of(groups: &[ThreadsListGroup]) -> (bool, bool) {
    let has = |key: ThreadsListKey| groups.iter().any(|group| group.key == key && !group.rows.is_empty());
    (has(ThreadsListKey::NeedsYou), has(ThreadsListKey::Running))
}

/// EXP-1075 / EXP-1294 — one team's tally of MY live threads: how many are
/// working, and whether any waits on me (the team switcher's dot turns amber).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TeamLiveRuns {
    pub(crate) count: usize,
    pub(crate) needs_input: bool,
}

/// The thread roots whose tree holds a session I started (`user_id` = the
/// starter) that is working ([`session_is_working`]) or waits on a person,
/// grouped by root id. The ONE liveness rule the rail's Running section and
/// the team switcher's dot share.
fn my_live_trees<'a>(sessions: &[&'a SessionRow], me: &str) -> BTreeMap<&'a str, Vec<&'a SessionRow>> {
    let thread_roots: HashMap<&str, &SessionRow> = sessions
        .iter()
        .copied()
        .filter(|session| session.is_root() && session.is_thread())
        .map(|session| (session.id.as_str(), session))
        .collect();
    let mut trees: BTreeMap<&str, Vec<&SessionRow>> = BTreeMap::new();
    for session in sessions.iter().copied() {
        if session.user_id.as_deref() != Some(me) {
            continue;
        }
        if !session_is_working(session) && !session_needs_you(session) {
            continue;
        }
        let root_id = session.root_id_or_self();
        if thread_roots.contains_key(root_id) {
            trees.entry(root_id).or_default().push(session);
        }
    }
    trees
}

/// Pure core of [`own_live_roots_by_team`]. A root without a `team_id` is
/// never counted (the column is NOT NULL server-side: `None` = a decode gap).
pub(crate) fn own_live_roots_by_team_from(sessions: &[&SessionRow], me: &str) -> BTreeMap<String, TeamLiveRuns> {
    let roots: HashMap<&str, &SessionRow> = sessions
        .iter()
        .copied()
        .map(|session| (session.id.as_str(), session))
        .collect();
    let mut by_team: BTreeMap<String, TeamLiveRuns> = BTreeMap::new();
    for (root_id, live) in my_live_trees(sessions, me) {
        let Some(team_id) = roots.get(root_id).and_then(|root| root.team_id.as_deref()) else {
            continue;
        };
        let entry = by_team.entry(team_id.to_string()).or_default();
        entry.count += 1;
        entry.needs_input |= live.iter().any(|session| session_needs_you(session));
    }
    by_team
}

/// The team switcher's dot: per team, my live threads
/// ([`own_live_roots_by_team_from`]). Replaces the run-era
/// `queries::own_live_runs_by_team_now`.
pub(crate) fn own_live_roots_by_team(cx: &App) -> BTreeMap<String, TeamLiveRuns> {
    let Some(me) = signed_in_user(cx) else {
        return BTreeMap::new();
    };
    let Some(store) = Store::try_global(cx) else {
        return BTreeMap::new();
    };
    let sessions = store.collections().sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions.iter().collect();
    own_live_roots_by_team_from(&rows, &me)
}

/// One row of the rail's Running section: a thread of mine with a live
/// session — the mark, the first issue's identifier and the title, no
/// caption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveRootRow {
    pub(crate) root_id: String,
    pub(crate) team_id: String,
    pub(crate) identifier: Option<String>,
    pub(crate) title: String,
    pub(crate) busy: bool,
    pub(crate) needs_you: bool,
    /// Every session of the tree — the row stays lit while one is on screen.
    pub(crate) session_ids: Vec<String>,
    /// The tree's newest activity (the sort key), verbatim.
    pub(crate) activity_at: String,
}

/// Pure core of [`live_roots_for_rail`]: newest activity first, ties by id.
pub(crate) fn live_roots_from(
    sessions: &[&SessionRow],
    links: &[&SessionIssueRow],
    issues: &[TreeIssue],
    messages: &[&MessageRow],
    team_id: &str,
    me: &str,
) -> Vec<LiveRootRow> {
    let by_id: HashMap<&str, &SessionRow> = sessions
        .iter()
        .copied()
        .map(|session| (session.id.as_str(), session))
        .collect();
    let mut rows: Vec<LiveRootRow> = Vec::new();
    for (root_id, live) in my_live_trees(sessions, me) {
        let Some(root) = by_id.get(root_id).copied() else {
            continue;
        };
        if root.team_id.as_deref() != Some(team_id) {
            continue;
        }
        let tree: Vec<&SessionRow> = sessions
            .iter()
            .copied()
            .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
            .collect();
        let root_links: Vec<&SessionIssueRow> = links.iter().copied().filter(|link| link.session_id == root_id).collect();
        let root_issues = domain::session_tree::root_issues(root_id, &root_links, issues);
        let own: Vec<&MessageRow> = messages.iter().copied().filter(|message| message.session_id == root_id).collect();
        let activity_at = tree
            .iter()
            .map(|session| activity_key(session))
            .max()
            .unwrap_or("")
            .to_string();
        rows.push(LiveRootRow {
            root_id: root_id.to_string(),
            team_id: team_id.to_string(),
            identifier: root_issues.first().map(|issue| issue.identifier.clone()),
            title: thread_title(root, &root_issues, &own).title,
            busy: live.iter().any(|session| session.busy),
            needs_you: live.iter().any(|session| session_needs_you(session)),
            session_ids: tree_order(&tree).iter().map(|session| session.id.clone()).collect(),
            activity_at,
        });
    }
    rows.sort_by(|a, b| {
        b.activity_at
            .cmp(&a.activity_at)
            .then_with(|| a.root_id.cmp(&b.root_id))
    });
    rows
}

/// The rail's Running section for `team_id`: my live threads
/// ([`live_roots_from`]).
pub(crate) fn live_roots_for_rail(team_id: &str, cx: &App) -> Vec<LiveRootRow> {
    let Some(me) = signed_in_user(cx) else {
        return Vec::new();
    };
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .collect();
    let links: Vec<&SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| link.team_id.as_deref().is_none_or(|team| team == team_id))
        .collect();
    let all_issues = collections.issues.read(cx);
    let issues: Vec<TreeIssue> = links
        .iter()
        .filter_map(|link| all_issues.get(&link.issue_id))
        .map(TreeIssue::from_issue)
        .collect();
    let messages: Vec<&MessageRow> = collections
        .messages
        .read(cx)
        .iter()
        .filter(|message| message.team_id.as_deref().is_none_or(|team| team == team_id))
        .collect();
    live_roots_from(&rows, &links, &issues, &messages, team_id, &me)
}

/// Re-render a view whenever one of the five session collections (or the
/// issues they name) changes — the observe set every session surface needs.
pub(crate) fn observe_session_collections<V: 'static>(cx: &mut gpui::Context<V>) -> Vec<gpui::Subscription> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    vec![
        cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
        cx.observe(&collections.session_issues, |_, _, cx| cx.notify()),
        cx.observe(&collections.messages, |_, _, cx| cx.notify()),
        cx.observe(&collections.pull_requests, |_, _, cx| cx.notify()),
        cx.observe(&collections.pull_request_issues, |_, _, cx| cx.notify()),
        cx.observe(&collections.issues, |_, _, cx| cx.notify()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(value: serde_json::Value) -> SessionRow {
        serde_json::from_value(value).unwrap()
    }

    fn root(id: &str, team: &str, user: &str, status: &str) -> SessionRow {
        row(json!({
            "id": id, "team_id": team, "kind": "thread", "user_id": user,
            "status": status, "root_id": id, "shared": true,
            "created_at": "2026-10-13T10:00:00.000Z",
            "updated_at": "2026-10-13T10:00:00.000Z",
        }))
    }

    fn child(id: &str, root_id: &str, team: &str, user: &str, status: &str, updated_at: &str) -> SessionRow {
        row(json!({
            "id": id, "team_id": team, "kind": "thread", "user_id": user,
            "status": status, "parent_id": root_id, "root_id": root_id, "depth": 1,
            "created_at": "2026-10-13T10:01:00.000Z",
            "updated_at": updated_at,
        }))
    }

    #[test]
    fn working_means_queued_or_running() {
        assert!(session_is_working(&root("a", "t", "me", "queued")));
        assert!(session_is_working(&root("a", "t", "me", "running")));
        assert!(!session_is_working(&root("a", "t", "me", "idle")));
        assert!(!session_is_working(&root("a", "t", "me", "ended")));
    }

    /// The switcher counts a thread ONCE however many of my sessions run in
    /// it, only my sessions count, idle and ended never do, and needs-you
    /// lifts only its own team.
    #[test]
    fn live_roots_fold_by_team() {
        let mut waiting = child("c2", "r2", "t2", "me", "idle", "2026-10-13T10:02:00.000Z");
        waiting.needs_input = true;
        let sessions = [
            root("r1", "t1", "me", "running"),
            child("c1", "r1", "t1", "me", "running", "2026-10-13T10:02:00.000Z"),
            root("r2", "t2", "me", "idle"),
            waiting,
            root("r3", "t1", "someone", "running"),
            root("r4", "t1", "me", "idle"),
            root("r5", "t1", "me", "ended"),
        ];
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        let by_team = own_live_roots_by_team_from(&refs, "me");
        assert_eq!(
            by_team.get("t1"),
            Some(&TeamLiveRuns {
                count: 1,
                needs_input: false
            })
        );
        assert_eq!(
            by_team.get("t2"),
            Some(&TeamLiveRuns {
                count: 1,
                needs_input: true
            })
        );
        assert_eq!(by_team.len(), 2);
    }

    #[test]
    fn agent_roots_and_team_less_roots_never_count() {
        let mut agent = root("a1", "t1", "me", "running");
        agent.kind = Some("agent".into());
        let mut loose = root("r1", "t1", "me", "running");
        loose.team_id = None;
        let sessions = [agent, loose];
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        assert!(own_live_roots_by_team_from(&refs, "me").is_empty());
    }

    #[test]
    fn rail_rows_carry_the_first_issue_and_sort_by_activity() {
        let sessions = [
            root("r1", "t1", "me", "running"),
            root("r2", "t1", "me", "queued"),
            child("c2", "r2", "t1", "me", "idle", "2026-10-13T11:00:00.000Z"),
            root("r3", "t2", "me", "running"),
        ];
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        let links: Vec<SessionIssueRow> = vec![serde_json::from_value(json!({
            "id": "l1", "session_id": "r1", "issue_id": "i1", "sort_order": 1,
        }))
        .unwrap()];
        let link_refs: Vec<&SessionIssueRow> = links.iter().collect();
        let issues = vec![TreeIssue {
            id: "i1".into(),
            identifier: "EXP-1".into(),
            title: "Fix the rail".into(),
        }];
        let rows = live_roots_from(&refs, &link_refs, &issues, &[], "t1", "me");
        let ids: Vec<&str> = rows.iter().map(|row| row.root_id.as_str()).collect();
        assert_eq!(ids, vec!["r2", "r1"]);
        assert_eq!(rows[1].identifier.as_deref(), Some("EXP-1"));
        assert_eq!(rows[1].title, "Fix the rail");
        assert_eq!(rows[0].identifier, None);
        assert_eq!(rows[0].session_ids, vec!["r2".to_string(), "c2".to_string()]);
    }

    #[test]
    fn the_badge_reads_the_needs_you_and_running_bands() {
        let band = |key: ThreadsListKey| ThreadsListGroup {
            key,
            title: key.title().to_string(),
            rows: vec![domain::session_tree::ThreadsListRow {
                session_id: "r".into(),
                team_id: "t".into(),
                title: "x".into(),
                private: false,
                busy: false,
                needs_you: false,
                activity_at: String::new(),
            }],
        };
        assert_eq!(threads_badge_of(&[]), (false, false));
        assert_eq!(threads_badge_of(&[band(ThreadsListKey::Recent)]), (false, false));
        assert_eq!(threads_badge_of(&[band(ThreadsListKey::Running)]), (false, true));
        assert_eq!(
            threads_badge_of(&[band(ThreadsListKey::NeedsYou), band(ThreadsListKey::Running)]),
            (true, true)
        );
    }

}
