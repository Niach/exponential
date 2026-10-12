//! EXP-1284: what a THREAD looks like, resolved once and rendered identically
//! ×4 — the twin of web `apps/web/src/lib/thread-view.ts` (iOS
//! `ThreadView.swift`, Android `ThreadView.kt`), byte-locked by
//! `packages/domain-contract/fixtures/thread-view.json`. A thread holds n
//! issues and m runs (`thread_items`); this module turns the synced rows into:
//!
//! * its TITLE (the stored one, else the first run's name unless that is the
//!   generic `Chat`, else the first issue's title, else `Chat`/`Thread`);
//! * its CHILD STRIP: Conversation, one chip per issue in link order, the
//!   "+" and the Guide (dot when the thread's Guide has entries);
//! * its CAPTIONS: the header's `2 sessions · 2 devices` and a list row's
//!   `2 issues · 1 session · macbook` (the device part only while a run is live,
//!   naming the live runs' devices);
//! * its CONVERSATION: every run in start order as a `Session N` block (the
//!   product says SESSION; state, device, `resumed`, `started by Session K`), the issues its agent linked as
//!   rows under it (`Linked` / `Filed and linked`), and the runs one parent
//!   started as ONE parallel card once there are two or more of them;
//! * its LIST placement ([`partition_thread_list`]): Running (a live session
//!   of mine) vs Recent (my other threads), scoped to a team or not;
//! * its Guide's PR ([`thread_pr_target`]: newest open, else newest merged)
//!   and the Guide entries ONE session block shows ([`session_guide_entries`]).
//!
//! Pure: callers pass what they synced; an item whose issue or run did not
//! resolve is dropped (trashed board, purged run). The view types serialize
//! to the fixture's camelCase shape, so the test compares whole values.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::activity_fold::parse_instant;
use crate::batch_run::CHAT_RUN_NAME;

pub const THREAD_FALLBACK_TITLE: &str = "Thread";
pub const THREAD_LINKED_LABEL: &str = "Linked";
pub const THREAD_FILED_LABEL: &str = "Filed and linked";
/// An issue created at most this long before the agent linked it was FILED
/// by that agent for the thread.
pub const THREAD_FILED_WINDOW_MS: i64 = 120_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadViewThread {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Only "is it a non-empty array" matters here (the Guide dot).
    #[serde(default)]
    pub results: Option<serde_json::Value>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadViewItem {
    pub id: String,
    /// `issue` | `run` (contract `threadItemKind`).
    pub kind: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    /// `user` | `agent` (contract `threadItemSource`).
    pub source: String,
    #[serde(default)]
    pub added_by_session_id: Option<String>,
    pub sort_order: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadViewIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadViewRun {
    pub id: String,
    /// The run's identity subject (the session identity: issue title, action
    /// name, chat title…), used where a run is named on its own.
    pub name: String,
    pub user_id: String,
    #[serde(default)]
    pub device_label: Option<String>,
    /// `running` | `in_review` | `ended`.
    pub status: String,
    pub needs_input: bool,
    pub agent_busy: bool,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub resumed_from_id: Option<String>,
    pub started_at: String,
    #[serde(default)]
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadRunState {
    Live,
    NeedsInput,
    Ended,
}

impl ThreadRunState {
    /// The trailing state word a block header shows.
    pub fn word(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::NeedsInput => "needs input",
            Self::Ended => "ended",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ThreadChild {
    Conversation,
    Issue {
        #[serde(rename = "issueId")]
        issue_id: String,
        identifier: String,
        title: String,
    },
    Add,
    Guide {
        dot: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadLinkRow {
    pub item_id: String,
    pub issue_id: String,
    pub identifier: String,
    pub title: String,
    /// `Linked` | `Filed and linked` (the agent filed it, then linked it).
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRunBlock {
    pub run_id: String,
    /// `Session 1`, … in start order across the whole thread.
    pub label: String,
    /// The muted parts after the label: device, `resumed`, `started by Session K`.
    pub caption: Vec<String>,
    pub state: ThreadRunState,
    /// Ended runs fold to one line until Show work opens them.
    pub folded: bool,
    pub links: Vec<ThreadLinkRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadParallelRow {
    pub run_id: String,
    pub name: String,
    pub issue_id: Option<String>,
    pub device_label: Option<String>,
    pub state: ThreadRunState,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadParallelBlock {
    /// `Sessions 3 and 4`, `Sessions 3, 4 and 5`.
    pub label: String,
    /// `started by Session 2`.
    pub caption: Vec<String>,
    /// Live while any of its runs is.
    pub state: ThreadRunState,
    pub runs: Vec<ThreadParallelRow>,
    pub links: Vec<ThreadLinkRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ThreadBlock {
    Run(ThreadRunBlock),
    Parallel(ThreadParallelBlock),
}

impl ThreadBlock {
    pub fn links(&self) -> &[ThreadLinkRow] {
        match self {
            Self::Run(block) => &block.links,
            Self::Parallel(block) => &block.links,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadView {
    pub title: String,
    /// True when `title` is derived (the thread has no stored title).
    pub title_derived: bool,
    pub strip: Vec<ThreadChild>,
    pub header_caption: String,
    pub list_caption: String,
    pub live: bool,
    pub needs_input: bool,
    pub busy: bool,
    pub conversation: Vec<ThreadBlock>,
    /// Run ids in start order (Session 1 first).
    pub run_ids: Vec<String>,
    /// Issue ids in link order.
    pub issue_ids: Vec<String>,
}

pub fn thread_run_state(status: &str, needs_input: bool) -> ThreadRunState {
    if status == "ended" {
        ThreadRunState::Ended
    } else if needs_input {
        ThreadRunState::NeedsInput
    } else {
        ThreadRunState::Live
    }
}

fn run_state(run: &ThreadViewRun) -> ThreadRunState {
    thread_run_state(&run.status, run.needs_input)
}

/// `Date.parse` with an unparseable/absent value sorting as 0 (the web's
/// `time()` helper).
fn time(value: &str) -> i64 {
    parse_instant(value).unwrap_or(0)
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `3`, `3 and 4`, `3, 4 and 5`.
fn join_ordinals(ordinals: &[usize]) -> String {
    match ordinals.split_last() {
        None => String::new(),
        Some((last, [])) => last.to_string(),
        Some((last, head)) => {
            let head: Vec<String> = head.iter().map(usize::to_string).collect();
            format!("{} and {last}", head.join(", "))
        }
    }
}

fn devices_part<'a>(runs: impl IntoIterator<Item = &'a ThreadViewRun>) -> Option<String> {
    let mut labels: Vec<&str> = Vec::new();
    for run in runs {
        let Some(label) = run.device_label.as_deref().map(str::trim) else { continue };
        if !label.is_empty() && !labels.contains(&label) {
            labels.push(label);
        }
    }
    match labels.as_slice() {
        [] => None,
        [one] => Some((*one).to_string()),
        many => Some(format!("{} devices", many.len())),
    }
}

/// Whether a thread's Guide (`threads.results`) has entries — the strip's
/// Guide dot.
pub fn thread_has_guide(results: Option<&serde_json::Value>) -> bool {
    matches!(results, Some(serde_json::Value::Array(entries)) if !entries.is_empty())
}

pub fn build_thread_view(
    thread: &ThreadViewThread,
    items: &[ThreadViewItem],
    issues: &[ThreadViewIssue],
    runs: &[ThreadViewRun],
) -> ThreadView {
    let issues_by_id: HashMap<&str, &ThreadViewIssue> =
        issues.iter().map(|issue| (issue.id.as_str(), issue)).collect();
    let runs_by_id: HashMap<&str, &ThreadViewRun> =
        runs.iter().map(|run| (run.id.as_str(), run)).collect();
    let mut items: Vec<&ThreadViewItem> = items.iter().collect();
    items.sort_by(|a, b| {
        a.sort_order
            .partial_cmp(&b.sort_order)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| time(&a.created_at).cmp(&time(&b.created_at)))
            .then_with(|| a.id.cmp(&b.id))
    });

    let issue_items: Vec<&ThreadViewItem> = items
        .iter()
        .copied()
        .filter(|item| {
            item.kind == "issue"
                && item
                    .issue_id
                    .as_deref()
                    .is_some_and(|id| issues_by_id.contains_key(id))
        })
        .collect();
    let thread_issues: Vec<&ThreadViewIssue> = issue_items
        .iter()
        .map(|item| issues_by_id[item.issue_id.as_deref().unwrap_or_default()])
        .collect();
    let mut thread_runs: Vec<&ThreadViewRun> = items
        .iter()
        .filter(|item| item.kind == "run")
        .filter_map(|item| item.session_id.as_deref().and_then(|id| runs_by_id.get(id)))
        .copied()
        .collect();
    thread_runs.sort_by(|a, b| {
        time(&a.started_at)
            .cmp(&time(&b.started_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    // The thread is named after its first prompt (the first run's own name:
    // a chat's agent title, an action's name, the issue a run started on),
    // the first issue only while that run is still the generic `Chat`.
    let stored = thread
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty());
    let first_run = thread_runs.first().map(|run| run.name.trim()).unwrap_or("");
    let first_issue = thread_issues.first().map(|issue| issue.title.trim()).unwrap_or("");
    let derived = if !first_run.is_empty() && first_run != CHAT_RUN_NAME {
        first_run
    } else if !first_issue.is_empty() {
        first_issue
    } else if !first_run.is_empty() {
        first_run
    } else {
        THREAD_FALLBACK_TITLE
    };
    let title = stored.unwrap_or(derived).to_string();

    let mut strip = vec![ThreadChild::Conversation];
    strip.extend(thread_issues.iter().map(|issue| ThreadChild::Issue {
        issue_id: issue.id.clone(),
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
    }));
    strip.push(ThreadChild::Add);
    strip.push(ThreadChild::Guide {
        dot: thread_has_guide(thread.results.as_ref()),
    });

    let live_runs: Vec<&ThreadViewRun> = thread_runs
        .iter()
        .copied()
        .filter(|run| run.status != "ended")
        .collect();
    let runs_part = plural(thread_runs.len(), "session", "sessions");
    let header_parts: Vec<String> = [Some(runs_part.clone()), devices_part(thread_runs.iter().copied())]
        .into_iter()
        .flatten()
        .collect();
    let issues_part = if thread_issues.is_empty() {
        "no issues".to_string()
    } else {
        plural(thread_issues.len(), "issue", "issues")
    };
    let live_devices = if live_runs.is_empty() {
        None
    } else {
        devices_part(live_runs.iter().copied())
    };
    let list_parts: Vec<String> = [Some(issues_part), Some(runs_part), live_devices]
        .into_iter()
        .flatten()
        .collect();

    let ordinal: HashMap<&str, usize> = thread_runs
        .iter()
        .enumerate()
        .map(|(index, run)| (run.id.as_str(), index + 1))
        .collect();

    let links_of = |run_ids: &HashSet<&str>| -> Vec<ThreadLinkRow> {
        let mut linked: Vec<&ThreadViewItem> = issue_items
            .iter()
            .copied()
            .filter(|item| {
                item.source == "agent"
                    && item
                        .added_by_session_id
                        .as_deref()
                        .is_some_and(|id| run_ids.contains(id))
            })
            .collect();
        linked.sort_by(|a, b| {
            time(&a.created_at)
                .cmp(&time(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        linked
            .into_iter()
            .map(|item| {
                let issue = issues_by_id[item.issue_id.as_deref().unwrap_or_default()];
                let filed = time(&issue.created_at) >= time(&item.created_at) - THREAD_FILED_WINDOW_MS;
                ThreadLinkRow {
                    item_id: item.id.clone(),
                    issue_id: issue.id.clone(),
                    identifier: issue.identifier.clone(),
                    title: issue.title.clone(),
                    label: if filed { THREAD_FILED_LABEL } else { THREAD_LINKED_LABEL }.to_string(),
                }
            })
            .collect()
    };

    // Children one in-thread parent started, grouped once there are 2+.
    let mut children_of: HashMap<&str, Vec<&ThreadViewRun>> = HashMap::new();
    for run in &thread_runs {
        if let Some(parent) = run.parent_session_id.as_deref() {
            if ordinal.contains_key(parent) && run.resumed_from_id.is_none() {
                children_of.entry(parent).or_default().push(run);
            }
        }
    }

    let mut conversation = Vec::new();
    let mut placed: HashSet<&str> = HashSet::new();
    for run in &thread_runs {
        if placed.contains(run.id.as_str()) {
            continue;
        }
        let siblings = match (run.parent_session_id.as_deref(), &run.resumed_from_id) {
            (Some(parent), None) => children_of.get(parent),
            _ => None,
        };
        let started_by = run
            .parent_session_id
            .as_deref()
            .and_then(|parent| ordinal.get(parent))
            .map(|n| format!("started by Session {n}"));
        if let Some(siblings) = siblings.filter(|siblings| siblings.len() >= 2) {
            for sibling in siblings {
                placed.insert(sibling.id.as_str());
            }
            let states: Vec<ThreadRunState> = siblings.iter().map(|run| run_state(run)).collect();
            let state = if states.contains(&ThreadRunState::NeedsInput) {
                ThreadRunState::NeedsInput
            } else if states.contains(&ThreadRunState::Live) {
                ThreadRunState::Live
            } else {
                ThreadRunState::Ended
            };
            let ordinals: Vec<usize> = siblings.iter().map(|s| ordinal[s.id.as_str()]).collect();
            conversation.push(ThreadBlock::Parallel(ThreadParallelBlock {
                label: format!("Sessions {}", join_ordinals(&ordinals)),
                caption: started_by.into_iter().collect(),
                state,
                runs: siblings
                    .iter()
                    .map(|sibling| ThreadParallelRow {
                        run_id: sibling.id.clone(),
                        name: sibling.name.clone(),
                        issue_id: sibling.issue_id.clone(),
                        device_label: sibling.device_label.clone(),
                        state: run_state(sibling),
                        started_at: sibling.started_at.clone(),
                    })
                    .collect(),
                links: links_of(&siblings.iter().map(|s| s.id.as_str()).collect()),
            }));
            continue;
        }
        placed.insert(run.id.as_str());
        let state = run_state(run);
        let caption: Vec<String> = [
            run.device_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_string),
            run.resumed_from_id.as_ref().map(|_| "resumed".to_string()),
            started_by,
        ]
        .into_iter()
        .flatten()
        .collect();
        conversation.push(ThreadBlock::Run(ThreadRunBlock {
            run_id: run.id.clone(),
            label: format!("Session {}", ordinal[run.id.as_str()]),
            caption,
            state,
            folded: state == ThreadRunState::Ended,
            links: links_of(&HashSet::from([run.id.as_str()])),
        }));
    }

    ThreadView {
        title,
        title_derived: stored.is_none(),
        strip,
        header_caption: header_parts.join(" · "),
        list_caption: list_parts.join(" · "),
        live: !live_runs.is_empty(),
        needs_input: live_runs.iter().any(|run| run.needs_input),
        busy: live_runs.iter().any(|run| run.agent_busy),
        conversation,
        run_ids: thread_runs.iter().map(|run| run.id.clone()).collect(),
        issue_ids: thread_issues.iter().map(|issue| issue.id.clone()).collect(),
    }
}

/// One thread as a list sees it ([`partition_thread_list`]).
pub trait ThreadListEntry {
    fn thread_id(&self) -> &str;
    fn team_id(&self) -> &str;
    fn updated_at(&self) -> &str;
    /// The viewer owns a live session in it.
    fn mine_live(&self) -> bool;
    /// The viewer has a session in it or created it (Recent lists only these).
    fn mine(&self) -> bool;
}

/// Running (a live session of mine) vs Recent (my other threads: a session of
/// mine or created by me), newest activity first. Scope: `team_id` = the
/// ACTIVE team (web, desktop); `None` = every team (phones, which band the
/// rows per team).
pub fn partition_thread_list<T: ThreadListEntry + Clone>(
    entries: &[T],
    team_id: Option<&str>,
) -> (Vec<T>, Vec<T>) {
    let mut by_activity: Vec<&T> = entries
        .iter()
        .filter(|entry| team_id.is_none_or(|team| entry.team_id() == team))
        .collect();
    by_activity.sort_by(|a, b| {
        time(b.updated_at())
            .cmp(&time(a.updated_at()))
            .then_with(|| a.thread_id().cmp(b.thread_id()))
    });
    let running = by_activity.iter().filter(|e| e.mine_live()).map(|e| (*e).clone()).collect();
    let recent = by_activity
        .iter()
        .filter(|e| !e.mine_live() && e.mine())
        .map(|e| (*e).clone())
        .collect();
    (running, recent)
}

/// One PR a thread's Guide could target: on one of its sessions or issues.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ThreadPrCandidate {
    pub url: String,
    /// `open` | `merged` | `closed` (any other value counts as closed).
    #[serde(default)]
    pub state: Option<String>,
    /// When the PR was stamped (its session's or issue's activity time).
    pub at: String,
}

/// The PR a thread's Guide and Merge capsule target: the newest OPEN PR, else
/// the newest MERGED one, else none.
pub fn thread_pr_target(candidates: &[ThreadPrCandidate]) -> Option<String> {
    let newest = |state: &str| {
        candidates
            .iter()
            .filter(|candidate| candidate.state.as_deref() == Some(state) && !candidate.url.is_empty())
            .max_by(|a, b| time(&a.at).cmp(&time(&b.at)).then_with(|| b.url.cmp(&a.url)))
            .map(|candidate| candidate.url.clone())
    };
    newest("open").or_else(|| newest("merged"))
}

/// The Guide entries a session block shows: the ones that session wrote
/// (`sessionId`); entries filed before the stamp belong to the thread's FIRST
/// session. The thread's Guide face shows every entry.
pub fn session_guide_entries(
    results: Option<&serde_json::Value>,
    session_id: &str,
    first_session_id: Option<&str>,
) -> Vec<serde_json::Value> {
    let Some(serde_json::Value::Array(entries)) = results else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| {
            match entry
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty())
            {
                Some(id) => id == session_id,
                None => Some(session_id) == first_session_id,
            }
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// The ONE contract fixture, replayed on every client (web
    /// `thread-view.test.ts`, iOS `ThreadViewTests`, Android `ThreadViewTest`).
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/thread-view.json");

    #[derive(Deserialize)]
    struct ViewInput {
        thread: ThreadViewThread,
        items: Vec<ThreadViewItem>,
        issues: Vec<ThreadViewIssue>,
        runs: Vec<ThreadViewRun>,
    }

    #[derive(Deserialize)]
    struct ViewCase {
        name: String,
        input: ViewInput,
        expected: Value,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Entry {
        thread_id: String,
        team_id: String,
        updated_at: String,
        mine_live: bool,
        mine: bool,
    }

    impl ThreadListEntry for Entry {
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

    #[derive(Deserialize)]
    struct PartitionExpected {
        running: Vec<Entry>,
        recent: Vec<Entry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Scope {
        team_id: Option<String>,
    }

    #[derive(Deserialize)]
    struct PartitionCase {
        name: String,
        input: Vec<Entry>,
        scope: Scope,
        expected: PartitionExpected,
    }

    #[derive(Deserialize)]
    struct PrTargetCase {
        name: String,
        input: Vec<ThreadPrCandidate>,
        expected: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GuideSliceInput {
        results: Option<Value>,
        session_id: String,
        first_session_id: Option<String>,
    }

    #[derive(Deserialize)]
    struct GuideSliceCase {
        name: String,
        input: GuideSliceInput,
        expected: Vec<Value>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        views: Vec<ViewCase>,
        partitions: Vec<PartitionCase>,
        pr_targets: Vec<PrTargetCase>,
        guide_slices: Vec<GuideSliceCase>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("thread-view.json decodes")
    }

    #[test]
    fn thread_pr_target_matches_every_fixture_case() {
        let fixture = fixture();
        assert!(!fixture.pr_targets.is_empty());
        for case in fixture.pr_targets {
            assert_eq!(thread_pr_target(&case.input), case.expected, "fixture case: {}", case.name);
        }
    }

    #[test]
    fn session_guide_entries_match_every_fixture_case() {
        let fixture = fixture();
        assert!(!fixture.guide_slices.is_empty());
        for case in fixture.guide_slices {
            let actual = session_guide_entries(
                case.input.results.as_ref(),
                &case.input.session_id,
                case.input.first_session_id.as_deref(),
            );
            assert_eq!(actual, case.expected, "fixture case: {}", case.name);
        }
    }

    #[test]
    fn build_thread_view_matches_every_fixture_case() {
        let fixture = fixture();
        assert!(!fixture.views.is_empty());
        for case in fixture.views {
            let view = build_thread_view(
                &case.input.thread,
                &case.input.items,
                &case.input.issues,
                &case.input.runs,
            );
            let actual = serde_json::to_value(&view).unwrap();
            assert_eq!(actual, case.expected, "fixture case: {}", case.name);
        }
    }

    #[test]
    fn partition_thread_list_matches_every_fixture_case() {
        let fixture = fixture();
        assert!(!fixture.partitions.is_empty());
        for case in fixture.partitions {
            let (running, recent) = partition_thread_list(&case.input, case.scope.team_id.as_deref());
            assert_eq!(running, case.expected.running, "fixture case: {}", case.name);
            assert_eq!(recent, case.expected.recent, "fixture case: {}", case.name);
        }
    }

    #[test]
    fn ordinals_join_like_the_web() {
        assert_eq!(join_ordinals(&[]), "");
        assert_eq!(join_ordinals(&[3]), "3");
        assert_eq!(join_ordinals(&[3, 4]), "3 and 4");
        assert_eq!(join_ordinals(&[3, 4, 5]), "3, 4 and 5");
    }

    #[test]
    fn state_words() {
        assert_eq!(thread_run_state("running", false).word(), "live");
        assert_eq!(thread_run_state("in_review", true).word(), "needs input");
        assert_eq!(thread_run_state("ended", true).word(), "ended");
    }
}
