//! EXP-1284 — THREADS on the desktop: the read side every surface shares.
//!
//! A thread is one unit of work — n issues and m runs (`thread_items`) plus
//! THE Guide (`threads.results`). The view model is the contract's
//! ([`domain::thread_view`], fixture-locked ×4); this module only feeds it
//! the synced rows (`threads`, `thread_items`, `issues`, `coding_sessions`)
//! and answers the small questions the rest of the IDE asks:
//!
//! * [`thread_model`] — one thread's [`ThreadView`] plus the rows behind it
//!   (the thread page, its tab title);
//! * [`thread_of_session`] — the thread a run sits in (a run sits in exactly
//!   one), which is how a session tab offers its thread and how every Guide
//!   reader finds THE Guide ([`guide_results_of_session`]);
//! * [`issue_threads`] — the threads holding an issue (the issue page's
//!   `Threads` row);
//! * [`team_thread_lists`] — the Running / Recent split every run list shows
//!   now (`partition_thread_list`): the rail's Running section and Agent ›
//!   Recent.
//!
//! Pure over the store: nothing here writes. The writes (`threads.link` …)
//! live with the thread screen that issues them.

use std::collections::{HashMap, HashSet};

use gpui::{App, SharedString};
use sync::Store;

use domain::rows::{CodingSession, Issue, ThreadItemRow, ThreadRow};
use domain::thread_view::{
    build_thread_view, partition_thread_list, ThreadListEntry, ThreadPrCandidate, ThreadView,
    ThreadViewIssue, ThreadViewItem, ThreadViewRun, ThreadViewThread,
};

/// One thread, resolved: the synced row, its contract view and the run rows
/// it names (keyed by id — the conversation asks for a run's agent, owner,
/// results).
#[derive(Clone)]
pub(crate) struct ThreadModel {
    pub(crate) thread: ThreadRow,
    pub(crate) view: ThreadView,
    pub(crate) runs: HashMap<String, CodingSession>,
    pub(crate) issues: HashMap<String, Issue>,
    /// `thread_items` per issue id, so a chip knows its item (Unlink).
    pub(crate) items: Vec<ThreadItemRow>,
}

impl ThreadModel {
    /// The newest run in the thread (the last `Run N`).
    pub(crate) fn latest_run(&self) -> Option<&CodingSession> {
        self.view.run_ids.last().and_then(|id| self.runs.get(id))
    }

    /// The agent CLI the thread's newest run uses — the mark a list row and
    /// the thread's header lead with.
    pub(crate) fn agent(&self) -> coding::CodingAgent {
        self.latest_run()
            .and_then(|run| run.agent.as_deref())
            .and_then(coding::CodingAgent::parse)
            .unwrap_or_default()
    }

    /// The thread's PR ([`domain::thread_view::thread_pr_target`], ×4):
    /// the newest OPEN PR on its sessions and issues, else the newest MERGED
    /// one. `None` = no PR at all (a closed one never counts).
    pub(crate) fn pr_url(&self) -> Option<String> {
        let mut candidates: Vec<ThreadPrCandidate> = Vec::new();
        for run in self.runs.values() {
            if let Some(url) = run.pr_url.as_deref().filter(|url| !url.is_empty()) {
                candidates.push(ThreadPrCandidate {
                    url: url.to_string(),
                    state: run.pr_state.clone(),
                    at: run.updated_at.clone().or_else(|| run.started_at.clone()).unwrap_or_default(),
                });
            }
        }
        for issue in self.view.issue_ids.iter().filter_map(|id| self.issues.get(id)).chain(
            self.runs
                .values()
                .filter_map(|run| run.issue_id.as_deref())
                .filter_map(|id| self.issues.get(id)),
        ) {
            if let Some(url) = issue.pr_url.as_deref().filter(|url| !url.is_empty()) {
                candidates.push(ThreadPrCandidate {
                    url: url.to_string(),
                    state: issue.pr_state.clone(),
                    at: issue.updated_at.clone().unwrap_or_default(),
                });
            }
        }
        domain::thread_view::thread_pr_target(&candidates)
    }

    /// The merge capsule's target: [`Self::pr_url`] while that PR is OPEN —
    /// through the issue that carries it, else the session that opened it
    /// (the shared run rule, [`crate::changes_bar::merge_target_for_run`]).
    pub(crate) fn pr_target(&self) -> Option<crate::changes_bar::MergeTarget> {
        let url = self.pr_url()?;
        if let Some(issue) = self
            .issues
            .values()
            .find(|issue| issue.pr_url.as_deref() == Some(url.as_str()))
        {
            return crate::changes_bar::issue_has_open_pr(issue).then(|| {
                crate::changes_bar::MergeTarget::Issue {
                    issue_id: issue.id.clone(),
                }
            });
        }
        let run = self
            .runs
            .values()
            .find(|run| run.pr_url.as_deref() == Some(url.as_str()))?;
        crate::changes_bar::merge_target_for_run(run.issue_id.as_deref(), Some(run), self.issues.values())
    }

    /// The session whose Guide face the thread's Guide child reads through
    /// (its PR's files): the one behind [`Self::pr_url`], else the newest.
    pub(crate) fn guide_session_id(&self) -> Option<String> {
        let url = self.pr_url();
        url.as_deref()
            .and_then(|url| {
                self.view.run_ids.iter().rev().find(|id| {
                    self.runs.get(*id).is_some_and(|run| {
                        run.pr_url.as_deref() == Some(url)
                            || run
                                .issue_id
                                .as_deref()
                                .and_then(|issue| self.issues.get(issue))
                                .is_some_and(|issue| issue.pr_url.as_deref() == Some(url))
                    })
                })
            })
            .cloned()
            .or_else(|| self.view.run_ids.last().cloned())
    }

    /// THE Guide entries one session block shows
    /// ([`domain::thread_view::session_guide_entries`]): what that session
    /// wrote into the thread's Guide (else its own row's), unstamped legacy
    /// entries going to the thread's first session.
    pub(crate) fn session_guide(&self, session_id: &str) -> serde_json::Value {
        let results = self
            .thread
            .results
            .clone()
            .filter(|results| domain::thread_view::thread_has_guide(Some(results)))
            .or_else(|| self.runs.get(session_id).and_then(|run| run.results.clone()));
        serde_json::Value::Array(domain::thread_view::session_guide_entries(
            results.as_ref(),
            session_id,
            self.view.run_ids.first().map(String::as_str),
        ))
    }
}

fn now_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A run as the contract's thread view reads it. `name` is the run's
/// identity subject ([`crate::run_rows::run_title`], the web
/// `sessionIdentity(...).subject`). Live = the heartbeat rule every session
/// list uses ([`crate::queries::coding_session_is_live`]): a sweep end
/// (EXP-888) is not an end, a heartbeat gone stale is.
pub(crate) fn view_run(session: &CodingSession, issue: Option<&Issue>, batch: &[Issue], now: i64) -> ThreadViewRun {
    let ended = !crate::queries::coding_session_is_live(session, now);
    ThreadViewRun {
        id: session.id.clone(),
        name: crate::run_rows::run_title(session, issue, batch).to_string(),
        user_id: session.user_id.clone().unwrap_or_default(),
        device_label: session.device_label.clone(),
        status: if ended {
            domain::contract::CODING_SESSION_STATUS_ENDED.to_string()
        } else {
            session
                .status
                .clone()
                .filter(|status| status != domain::contract::CODING_SESSION_STATUS_ENDED)
                .unwrap_or_else(|| "running".to_string())
        },
        needs_input: !ended && session.needs_input.unwrap_or(false),
        agent_busy: crate::queries::session_agent_busy(session, None, now),
        issue_id: session.issue_id.clone(),
        parent_session_id: session.parent_session_id.clone(),
        resumed_from_id: session.resumed_from_id.clone(),
        started_at: crate::run_rows::run_started_at(session).unwrap_or_default().to_string(),
        ended_at: session.ended_at.clone(),
    }
}

fn view_item(item: &ThreadItemRow) -> Option<ThreadViewItem> {
    Some(ThreadViewItem {
        id: item.id.clone(),
        kind: item.kind.clone()?,
        issue_id: item.issue_id.clone(),
        session_id: item.session_id.clone(),
        source: item
            .source
            .clone()
            .unwrap_or_else(|| domain::contract::THREAD_ITEM_SOURCE_USER.to_string()),
        added_by_session_id: item.added_by_session_id.clone(),
        sort_order: item.sort_order.unwrap_or(0.),
        created_at: item.created_at.clone().unwrap_or_default(),
    })
}

fn view_issue(issue: &Issue) -> ThreadViewIssue {
    ThreadViewIssue {
        id: issue.id.clone(),
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
        created_at: issue.created_at.clone().unwrap_or_default(),
    }
}

fn view_thread(thread: &ThreadRow) -> ThreadViewThread {
    ThreadViewThread {
        id: thread.id.clone(),
        title: thread.title.clone(),
        results: thread.results.clone(),
        updated_at: thread
            .updated_at
            .clone()
            .or_else(|| thread.created_at.clone())
            .unwrap_or_default(),
    }
}

/// Build one thread's model off the store. `None` while its row has not
/// synced (or never will: a foreign/purged thread).
pub(crate) fn thread_model(thread_id: &str, cx: &App) -> Option<ThreadModel> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let thread = collections.threads.read(cx).get(thread_id)?.clone();
    let items: Vec<ThreadItemRow> = collections
        .thread_items
        .read(cx)
        .iter()
        .filter(|item| item.thread_id == thread_id)
        .cloned()
        .collect();
    Some(model_from(thread, items, cx))
}

fn model_from(thread: ThreadRow, items: Vec<ThreadItemRow>, cx: &App) -> ThreadModel {
    let collections = Store::global(cx).collections();
    let issues_collection = collections.issues.read(cx);
    let sessions = collections.coding_sessions.read(cx);
    let now = now_epoch();
    let mut issues: HashMap<String, Issue> = HashMap::new();
    let mut runs: HashMap<String, CodingSession> = HashMap::new();
    for item in &items {
        if let Some(issue_id) = item.issue() {
            if let Some(issue) = issues_collection.get(issue_id) {
                issues.insert(issue.id.clone(), issue.clone());
            }
        }
        if let Some(session_id) = item.run() {
            if let Some(session) = sessions.get(session_id) {
                runs.insert(session.id.clone(), session.clone());
            }
        }
    }
    // A run's own issue (its name, the parallel row's chip) may sit outside
    // the thread's linked set.
    for run in runs.values() {
        if let Some(issue_id) = run.issue_id.as_deref() {
            if !issues.contains_key(issue_id) {
                if let Some(issue) = issues_collection.get(issue_id) {
                    issues.insert(issue.id.clone(), issue.clone());
                }
            }
        }
    }
    let view_runs: Vec<ThreadViewRun> = runs
        .values()
        .map(|run| {
            let issue = run.issue_id.as_deref().and_then(|id| issues.get(id));
            let batch: Vec<Issue> = if domain::batch_run::is_batch_run(run) {
                domain::batch_run::batch_run_issues(run, issues_collection.iter())
                    .into_iter()
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            view_run(run, issue, &batch, now)
        })
        .collect();
    let linked: HashSet<&str> = items.iter().filter_map(ThreadItemRow::issue).collect();
    let view_issues: Vec<ThreadViewIssue> = issues
        .values()
        .filter(|issue| linked.contains(issue.id.as_str()))
        .map(view_issue)
        .collect();
    let view_items: Vec<ThreadViewItem> = items.iter().filter_map(view_item).collect();
    let view = build_thread_view(&view_thread(&thread), &view_items, &view_issues, &view_runs);
    ThreadModel {
        thread,
        view,
        runs,
        issues,
        items,
    }
}

/// The thread `session_id` sits in (its `thread_items` run row). `None`
/// before the item syncs, and for a pre-threads run with none.
pub(crate) fn thread_of_session(session_id: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let items = store.collections().thread_items.read(cx);
    items
        .iter()
        .find(|item| item.run() == Some(session_id))
        .map(|item| item.thread_id.clone())
}

/// EXP-1284 — THE Guide a run's readers show: its thread's (`threads.results`,
/// canonical once threads exist) while that has entries, else the run's own
/// `results` (a run with no thread, a thread not synced yet). The web
/// `guideOfRun` rule.
pub(crate) fn guide_results_of_session(
    session: &CodingSession,
    cx: &App,
) -> Option<serde_json::Value> {
    thread_guide_of_session(&session.id, cx).or_else(|| session.results.clone())
}

/// The run's thread's Guide while it has entries.
fn thread_guide_of_session(session_id: &str, cx: &App) -> Option<serde_json::Value> {
    let thread_id = thread_of_session(session_id, cx)?;
    let store = Store::try_global(cx)?;
    let threads = store.collections().threads.read(cx);
    let results = threads.get(&thread_id)?.results.clone()?;
    domain::thread_view::thread_has_guide(Some(&results)).then_some(results)
}

/// One thread an issue sits in — a chip on the issue page's `Threads` row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IssueThreadChip {
    pub(crate) thread_id: String,
    pub(crate) title: SharedString,
    pub(crate) live: bool,
    pub(crate) busy: bool,
    pub(crate) needs_input: bool,
    pub(crate) agent: coding::CodingAgent,
}

/// The threads holding `issue_id`, newest activity first.
pub(crate) fn issue_threads(issue_id: &str, cx: &App) -> Vec<IssueThreadChip> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let ids: Vec<String> = {
        let items = store.collections().thread_items.read(cx);
        let mut ids: Vec<String> = items
            .iter()
            .filter(|item| item.issue() == Some(issue_id))
            .map(|item| item.thread_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let mut models: Vec<ThreadModel> = ids.iter().filter_map(|id| thread_model(id, cx)).collect();
    models.sort_by(|a, b| {
        activity(&b.thread)
            .cmp(&activity(&a.thread))
            .then_with(|| a.thread.id.cmp(&b.thread.id))
    });
    models
        .into_iter()
        .map(|model| IssueThreadChip {
            thread_id: model.thread.id.clone(),
            title: SharedString::from(model.view.title.clone()),
            live: model.view.live,
            busy: model.view.busy,
            needs_input: model.view.needs_input,
            agent: model.agent(),
        })
        .collect()
}

fn activity(thread: &ThreadRow) -> String {
    thread
        .updated_at
        .clone()
        .or_else(|| thread.created_at.clone())
        .unwrap_or_default()
}

/// One row of a thread list (Running / Recent): the thread's title over its
/// `listCaption`, led by its newest session's agent mark (spinning while
/// `busy`, amber while `needs_input`). EXP-1284 review T4: a live session of
/// mine that sits in NO thread yet (placement failed, or its item lags) is a
/// row too — a plain session row (`orphan_session_id`), never hidden.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadListRow {
    pub(crate) thread_id: String,
    pub(crate) team_id: String,
    pub(crate) title: SharedString,
    pub(crate) caption: SharedString,
    pub(crate) agent: coding::CodingAgent,
    pub(crate) live: bool,
    pub(crate) busy: bool,
    pub(crate) needs_input: bool,
    pub(crate) mine_live: bool,
    /// A session of mine sits in it, or I created it (Recent lists only these).
    pub(crate) mine: bool,
    pub(crate) updated_at: String,
    /// The thread's session ids — a row stays lit while one is on screen.
    pub(crate) run_ids: Vec<String>,
    /// My LIVE sessions in it, `(id, name)` — the row's "Stop session" (T7).
    pub(crate) my_live: Vec<(String, String)>,
    /// `Some` = an orphan session row (no thread yet): opens that session.
    pub(crate) orphan_session_id: Option<String>,
}

impl ThreadListEntry for ThreadListRow {
    fn thread_id(&self) -> &str {
        &self.thread_id
    }
    fn team_id(&self) -> &str {
        &self.team_id
    }
    fn updated_at(&self) -> &str {
        &self.updated_at
    }
    fn mine_live(&self) -> bool {
        self.mine_live
    }
    fn mine(&self) -> bool {
        self.mine
    }
}

impl ThreadListRow {
    /// The mark's state, in the session lists' own vocabulary.
    pub(crate) fn display(&self) -> Option<crate::queries::CodingSessionDisplay> {
        if self.needs_input {
            Some(crate::queries::CodingSessionDisplay::NeedsInput)
        } else if self.busy {
            Some(crate::queries::CodingSessionDisplay::Working)
        } else {
            None
        }
    }
}

/// The lists as of one store state ([`team_thread_lists`]'s memo key): the
/// collections' revisions, the viewer, the team and the 5 s liveness bucket
/// (a heartbeat going stale changes no row).
#[derive(Clone, PartialEq, Eq)]
struct ListsKey {
    team_id: String,
    me: String,
    revisions: [u64; 4],
    bucket: i64,
}

#[derive(Default)]
struct ListsMemo(Option<(ListsKey, (Vec<ThreadListRow>, Vec<ThreadListRow>))>);

impl gpui::Global for ListsMemo {}

/// EXP-1284 — the ACTIVE team's thread lists (the contract's
/// `partition_thread_list` with the team scope, review T6): Running = a live
/// session of mine (plus my orphan live sessions, T4), Recent = my other
/// threads (a session of mine or created by me), newest activity first.
/// Memoised per store change (review T16): every rail render reads it.
pub(crate) fn team_thread_lists(team_id: &str, cx: &mut App) -> (Vec<ThreadListRow>, Vec<ThreadListRow>) {
    let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
        return (Vec::new(), Vec::new());
    };
    let Some(store) = Store::try_global(cx) else {
        return (Vec::new(), Vec::new());
    };
    let collections = store.collections();
    let key = ListsKey {
        team_id: team_id.to_string(),
        me: me.clone(),
        revisions: [
            collections.threads.read(cx).revision(),
            collections.thread_items.read(cx).revision(),
            collections.coding_sessions.read(cx).revision(),
            collections.issues.read(cx).revision(),
        ],
        bucket: now_epoch() / 5,
    };
    if let Some((cached, lists)) = &cx.default_global::<ListsMemo>().0 {
        if *cached == key {
            return lists.clone();
        }
    }
    let lists = derive_team_thread_lists(team_id, &me, cx);
    cx.global_mut::<ListsMemo>().0 = Some((key, lists.clone()));
    lists
}

fn derive_team_thread_lists(team_id: &str, me: &str, cx: &App) -> (Vec<ThreadListRow>, Vec<ThreadListRow>) {
    let collections = Store::global(cx).collections();
    let now = now_epoch();
    let threads: Vec<ThreadRow> = collections
        .threads
        .read(cx)
        .iter()
        .filter(|thread| thread.team_id.as_deref() == Some(team_id))
        .cloned()
        .collect();
    let mut items_by_thread: HashMap<String, Vec<ThreadItemRow>> = HashMap::new();
    let mut placed: HashSet<String> = HashSet::new();
    for item in collections.thread_items.read(cx).iter() {
        if let Some(session_id) = item.run() {
            placed.insert(session_id.to_string());
        }
        items_by_thread
            .entry(item.thread_id.clone())
            .or_default()
            .push(item.clone());
    }
    let mut rows = Vec::new();
    for thread in threads {
        let items = items_by_thread.remove(&thread.id).unwrap_or_default();
        let model = model_from(thread, items, cx);
        // A thread nothing of which synced yet (an empty shell) lists once
        // something lands in it.
        if model.view.run_ids.is_empty() && model.view.issue_ids.is_empty() {
            continue;
        }
        let mine_run = |run: &CodingSession| run.user_id.as_deref() == Some(me);
        let mine = model.runs.values().any(mine_run)
            || model.thread.created_by.as_deref() == Some(me);
        let my_live: Vec<(String, String)> = model
            .view
            .run_ids
            .iter()
            .filter_map(|id| model.runs.get(id))
            .filter(|run| mine_run(run) && crate::queries::coding_session_is_live(run, now))
            .map(|run| {
                let issue = run.issue_id.as_deref().and_then(|id| model.issues.get(id));
                (run.id.clone(), crate::run_rows::run_title(run, issue, &[]).to_string())
            })
            .collect();
        rows.push(ThreadListRow {
            thread_id: model.thread.id.clone(),
            team_id: team_id.to_string(),
            title: SharedString::from(model.view.title.clone()),
            caption: SharedString::from(model.view.list_caption.clone()),
            agent: model.agent(),
            live: model.view.live,
            busy: model.view.busy,
            needs_input: model.view.needs_input,
            mine_live: !my_live.is_empty(),
            mine,
            updated_at: activity(&model.thread),
            run_ids: model.view.run_ids.clone(),
            my_live,
            orphan_session_id: None,
        });
    }
    // T4: my live sessions of this team with no thread item — plain rows.
    let issues = collections.issues.read(cx);
    for session in collections.coding_sessions.read(cx).iter() {
        if placed.contains(&session.id)
            || session.user_id.as_deref() != Some(me)
            || session.team_id.as_deref() != Some(team_id)
            || !crate::queries::coding_session_is_live(session, now)
        {
            continue;
        }
        let issue = session.issue_id.as_deref().and_then(|id| issues.get(id));
        let batch: Vec<Issue> = domain::batch_run::batch_run_issues(session, issues.iter())
            .into_iter()
            .cloned()
            .collect();
        let run = view_run(session, issue, &batch, now);
        let name = run.name.clone();
        rows.push(ThreadListRow {
            thread_id: session.id.clone(),
            team_id: team_id.to_string(),
            title: SharedString::from(name.clone()),
            caption: SharedString::from(session.device_label.clone().unwrap_or_default()),
            agent: session
                .agent
                .as_deref()
                .and_then(coding::CodingAgent::parse)
                .unwrap_or_default(),
            live: true,
            busy: run.agent_busy,
            needs_input: run.needs_input,
            mine_live: true,
            mine: true,
            updated_at: session
                .updated_at
                .clone()
                .or_else(|| session.started_at.clone())
                .unwrap_or_default(),
            run_ids: vec![session.id.clone()],
            my_live: vec![(session.id.clone(), name)],
            orphan_session_id: Some(session.id.clone()),
        });
    }
    partition_thread_list(&rows, Some(team_id))
}

/// The tab chip / window title of a thread: its view title, `Thread` while
/// the row is still on its way.
pub(crate) fn thread_title(thread_id: &str, cx: &App) -> SharedString {
    thread_model(thread_id, cx)
        .map(|model| SharedString::from(model.view.title))
        .unwrap_or_else(|| domain::thread_view::THREAD_FALLBACK_TITLE.into())
}
