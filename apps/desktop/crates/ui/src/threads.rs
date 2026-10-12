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
//!   now (`partition_thread_list`): the rail's Running section, the Agent
//!   home's Recent and Agent › Recent.
//!
//! Pure over the store: nothing here writes. The writes (`threads.link` …)
//! live with the thread screen that issues them.

use std::collections::{HashMap, HashSet};

use gpui::{App, SharedString};
use sync::Store;

use domain::rows::{CodingSession, Issue, ThreadItemRow, ThreadRow};
use domain::thread_view::{
    build_thread_view, partition_thread_list, ThreadListEntry, ThreadView, ThreadViewIssue,
    ThreadViewItem, ThreadViewRun, ThreadViewThread,
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

    /// The thread's PR, the merge capsule's target: the PR of its newest run
    /// with one (the shared run rule, [`crate::changes_bar::merge_target_for_run`]),
    /// else the open PR its issues share.
    pub(crate) fn pr_target(&self) -> Option<crate::changes_bar::MergeTarget> {
        for id in self.view.run_ids.iter().rev() {
            let Some(run) = self.runs.get(id) else { continue };
            let target = crate::changes_bar::merge_target_for_run(
                run.issue_id.as_deref(),
                Some(run),
                self.issues.values(),
            );
            if target.is_some() {
                return target;
            }
        }
        self.view
            .issue_ids
            .iter()
            .filter_map(|id| self.issues.get(id))
            .find(|issue| crate::changes_bar::issue_has_open_pr(issue))
            .map(|issue| crate::changes_bar::MergeTarget::Issue {
                issue_id: issue.id.clone(),
            })
    }
}

fn now_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A run as the contract's thread view reads it. `name` is the run's
/// identity subject ([`crate::run_rows::run_title`], the web
/// `sessionIdentity(...).subject`); a sweep end (EXP-888) is not an end.
pub(crate) fn view_run(session: &CodingSession, issue: Option<&Issue>, batch: &[Issue], now: i64) -> ThreadViewRun {
    let ended = crate::run_rows::run_has_ended(session);
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
/// `listCaption`, led by its newest run's agent mark (spinning while
/// `busy`, amber while `needs_input`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadListRow {
    pub(crate) thread_id: String,
    pub(crate) title: SharedString,
    pub(crate) caption: SharedString,
    pub(crate) agent: coding::CodingAgent,
    pub(crate) live: bool,
    pub(crate) busy: bool,
    pub(crate) needs_input: bool,
    pub(crate) mine_live: bool,
    pub(crate) updated_at: String,
    /// The thread's run ids — a row stays lit while one of them is on screen.
    pub(crate) run_ids: Vec<String>,
}

impl ThreadListEntry for ThreadListRow {
    fn thread_id(&self) -> &str {
        &self.thread_id
    }
    fn updated_at(&self) -> &str {
        &self.updated_at
    }
    fn mine_live(&self) -> bool {
        self.mine_live
    }
}

impl ThreadListRow {
    /// The mark's state, in the run lists' own vocabulary.
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

/// EXP-1284 — the team's threads that are MINE (a run of mine sits in it, or
/// I opened it), split Running (a live run of mine) / Recent, both newest
/// activity first — the contract's `partition_thread_list`.
pub(crate) fn team_thread_lists(team_id: &str, cx: &App) -> (Vec<ThreadListRow>, Vec<ThreadListRow>) {
    let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
        return (Vec::new(), Vec::new());
    };
    let Some(store) = Store::try_global(cx) else {
        return (Vec::new(), Vec::new());
    };
    let collections = store.collections();
    let now = now_epoch();
    let threads: Vec<ThreadRow> = collections
        .threads
        .read(cx)
        .iter()
        .filter(|thread| thread.team_id.as_deref() == Some(team_id))
        .cloned()
        .collect();
    if threads.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut items_by_thread: HashMap<String, Vec<ThreadItemRow>> = HashMap::new();
    for item in collections.thread_items.read(cx).iter() {
        items_by_thread
            .entry(item.thread_id.clone())
            .or_default()
            .push(item.clone());
    }
    let mut rows = Vec::new();
    for thread in threads {
        let items = items_by_thread.remove(&thread.id).unwrap_or_default();
        let model = model_from(thread, items, cx);
        let mine = |run: &CodingSession| run.user_id.as_deref() == Some(me.as_str());
        let has_mine = model.runs.values().any(mine);
        let created_by_me = model.thread.created_by.as_deref() == Some(me.as_str());
        if !has_mine && !created_by_me {
            continue;
        }
        // A thread nothing of which synced yet (an empty shell) lists once
        // something lands in it.
        if model.view.run_ids.is_empty() && model.view.issue_ids.is_empty() {
            continue;
        }
        let mine_live = model
            .runs
            .values()
            .any(|run| mine(run) && crate::queries::coding_session_is_live(run, now));
        rows.push(ThreadListRow {
            thread_id: model.thread.id.clone(),
            title: SharedString::from(model.view.title.clone()),
            caption: SharedString::from(model.view.list_caption.clone()),
            agent: model.agent(),
            live: model.view.live,
            busy: model.view.busy,
            needs_input: model.view.needs_input,
            mine_live,
            updated_at: activity(&model.thread),
            run_ids: model.view.run_ids.clone(),
        });
    }
    partition_thread_list(&rows)
}

/// The tab chip / window title of a thread: its view title, `Thread` while
/// the row is still on its way.
pub(crate) fn thread_title(thread_id: &str, cx: &App) -> SharedString {
    thread_model(thread_id, cx)
        .map(|model| SharedString::from(model.view.title))
        .unwrap_or_else(|| domain::thread_view::THREAD_FALLBACK_TITLE.into())
}
