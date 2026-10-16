//! EXP-1294 (Threads v2): the session tree's view models — the Rust twin of
//! `packages/domain-contract/src/session-tree.ts` (iOS ExpCore
//! `SessionTree.swift`, Android `domain/SessionTree.kt`), byte-locked by the
//! fixtures every mirror replays:
//!
//! * `fixtures/session-card.json`   → [`session_card`] (THE session card);
//! * `fixtures/session-thread.json` → [`build_session_thread`] (the thread
//!   screen: tabs, conversation rows, composer target) + [`guide_pull_request`]
//!   (the Guide's ONE PR) + [`guide_menu_entries`] (its header `…`,
//!   EXP-1307) + [`composer_yields_to_card`] (EXP-820);
//! * `fixtures/threads-list.json`   → [`partition_threads`] (the Threads page:
//!   Needs you · Running · Recent) + [`list_agents`] (the rail's Agents).
//!
//! The copy, icon concepts, tones and test ids come FROM the fixtures
//! ([`SESSION_CARD`], [`THREAD`], [`THREADS_LIST`]), never re-typed. Inputs
//! are the synced domain rows themselves ([`SessionRow`], [`SessionIssueRow`],
//! [`MessageRow`], [`PullRequestRow`]) plus a small [`TreeIssue`], so the UI
//! converts nothing. Timestamps stay ISO strings (parsed with the crate's one
//! ISO reader); `now_ms` = epoch milliseconds. Pure; the view types serialize
//! to the fixtures' camelCase shape so the replay compares whole values.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity_fold::parse_instant;
use crate::contract::{
    MESSAGE_AUTHOR_KIND_AGENT, MESSAGE_AUTHOR_KIND_USER, MESSAGE_CARD_KIND_EVENT,
    MESSAGE_CARD_KIND_PICTURE, MESSAGE_CARD_KIND_PLAN, MESSAGE_CARD_KIND_QUESTION,
    MESSAGE_CARD_KIND_SESSION, MESSAGE_CARD_KIND_SUBAGENT, MESSAGE_CARD_KIND_THREAD_STARTED,
    SESSION_KIND_AGENT, SESSION_KIND_THREAD, SESSION_STATUS_ENDED, SESSION_STATUS_QUEUED,
    SESSION_STATUS_RUNNING,
};
use crate::rows::{Issue, MessageRow, PullRequestRow, SessionIssueRow, SessionRow};

// ---------------------------------------------------------------------------
// Fixture-sourced copy
// ---------------------------------------------------------------------------

/// One string per [`SessionState`] (the fixtures key them snake_case).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StateStrings {
    pub queued: String,
    pub working: String,
    pub waiting: String,
    pub needs_you: String,
    pub blocked: String,
    pub ended: String,
}

impl StateStrings {
    pub fn get(&self, state: SessionState) -> &str {
        match state {
            SessionState::Queued => &self.queued,
            SessionState::Working => &self.working,
            SessionState::Waiting => &self.waiting,
            SessionState::NeedsYou => &self.needs_you,
            SessionState::Blocked => &self.blocked,
            SessionState::Ended => &self.ended,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionCardCopy {
    /// `Session` (an untitled session reads `Session N`).
    pub session: String,
    /// The muted state word per state.
    pub states: StateStrings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionCardTestIds {
    pub card: String,
}

/// `fixtures/session-card.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCardSpec {
    pub copy: SessionCardCopy,
    /// Icon CONCEPT per state (`coding-running`, `ui-clock`, …).
    pub icons: StateStrings,
    /// Tone per state (`muted`, `foreground`, `amber`, `rose`).
    pub tones: StateStrings,
    pub test_ids: SessionCardTestIds,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadCopy {
    /// `Untitled`.
    pub untitled: String,
    /// The reply composer's placeholder (`Reply…`).
    pub placeholder: String,
    /// The first tab (`Conversation`).
    pub conversation: String,
    /// The last tab (`Guide`).
    pub guide: String,
    /// Event templates by `messageEventKind` (`{issue}` / `{pr}` slots).
    pub events: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadTestIds {
    pub tabs: String,
    pub conversation: String,
    pub composer: String,
    /// EXP-1316: the header's Merge pill (`Merge PR` | `Merge stack`).
    #[serde(rename = "guideMerge")]
    pub guide_merge: String,
    /// EXP-1307: the header's Guide `…` trigger.
    #[serde(rename = "guideMenu")]
    pub guide_menu: String,
}

/// `fixtures/session-thread.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSpec {
    pub copy: ThreadCopy,
    /// A title derived from a person's first line is capped at this many
    /// characters (with `…`).
    pub title_max: usize,
    /// EXP-1316: the derived title templates.
    pub titles: ThreadTitles,
    pub test_ids: ThreadTestIds,
}

/// `session-thread.json` `titles` (`{action}` / `{issue}` / `{more}` slots).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadTitles {
    /// `Run {action}`.
    pub action: String,
    /// `Implement {issue}`.
    pub issue: String,
    /// `Implement {issue} + {more}`.
    pub issues: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadsListGroupTitles {
    pub needs_you: String,
    pub running: String,
    pub recent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadsListCopy {
    /// The page / rail entry name (`Threads`).
    pub title: String,
    /// The rail section header (`Agents`).
    pub agents: String,
    pub groups: ThreadsListGroupTitles,
    /// The empty page (`No sessions yet`).
    pub empty: String,
    /// EXP-1318: the rail band title (`Active`).
    pub rail: String,
    /// EXP-1318: the trailing words.
    pub words: ThreadRowWords,
}

/// `threads-list.json` `copy.words`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRowWords {
    pub stack: String,
    pub ready_to_merge: String,
    pub merged: String,
    pub question: String,
    pub needs_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentDefaults {
    pub icon: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsListTestIds {
    pub list: String,
    pub row: String,
    pub agent_row: String,
}

/// `fixtures/threads-list.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsListSpec {
    pub copy: ThreadsListCopy,
    /// An agent row without its own icon/color.
    pub agent_defaults: AgentDefaults,
    pub test_ids: ThreadsListTestIds,
}

const SESSION_CARD_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/session-card.json");
const SESSION_THREAD_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/session-thread.json");
const THREADS_LIST_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/threads-list.json");

/// THE session card's copy, icons, tones and test id.
pub static SESSION_CARD: LazyLock<SessionCardSpec> = LazyLock::new(|| {
    serde_json::from_str(SESSION_CARD_JSON).expect("session-card.json parses")
});
/// The thread screen's copy, title cap and test ids.
pub static THREAD: LazyLock<ThreadSpec> = LazyLock::new(|| {
    serde_json::from_str(SESSION_THREAD_JSON).expect("session-thread.json parses")
});
/// The Threads page / rail Agents copy, agent defaults and test ids.
pub static THREADS_LIST: LazyLock<ThreadsListSpec> = LazyLock::new(|| {
    serde_json::from_str(THREADS_LIST_JSON).expect("threads-list.json parses")
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// ISO → epoch ms; `None` for absent/unparsable (TS `ms`).
fn ms(iso: Option<&str>) -> Option<i64> {
    iso.filter(|value| !value.is_empty()).and_then(parse_instant)
}

fn ms_or_zero(iso: Option<&str>) -> i64 {
    ms(iso).unwrap_or(0)
}

/// JS `String(value ?? '')`.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

fn card_str<'a>(card: &'a Value, key: &str) -> Option<&'a str> {
    card.get(key)?.as_str()
}

fn is_root(session: &SessionRow) -> bool {
    session.is_root()
}

fn kind_is(session: &SessionRow, kind: &str) -> bool {
    session.kind.as_deref() == Some(kind)
}

fn status_is(session: &SessionRow, status: &str) -> bool {
    session.status.as_deref() == Some(status)
}

/// The work card's elapsed ladder: `37s`, `2m 04s`, `1h 03m`.
pub fn format_elapsed(duration_ms: i64) -> String {
    let total = duration_ms.max(0) / 1000;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

// ---------------------------------------------------------------------------
// The session card
// ---------------------------------------------------------------------------

/// One state word per session: the card's kind word, icon and tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Queued,
    Working,
    Waiting,
    NeedsYou,
    Blocked,
    Ended,
}

impl SessionState {
    /// The fixture's wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::NeedsYou => "needs_you",
            Self::Blocked => "blocked",
            Self::Ended => "ended",
        }
    }

    /// The muted state word (`Working`, `Needs you`, …).
    pub fn word(self) -> &'static str {
        SESSION_CARD.copy.states.get(self)
    }

    /// The icon concept.
    pub fn icon(self) -> &'static str {
        SESSION_CARD.icons.get(self)
    }

    /// The tone token (`muted` | `foreground` | `amber` | `rose`).
    pub fn tone(self) -> &'static str {
        SESSION_CARD.tones.get(self)
    }
}

/// ended → needs you → blocked → queued → working (busy) → waiting.
pub fn session_state(session: &SessionRow) -> SessionState {
    if status_is(session, SESSION_STATUS_ENDED) {
        SessionState::Ended
    } else if session.needs_input {
        SessionState::NeedsYou
    } else if session.blocked.is_some() {
        SessionState::Blocked
    } else if status_is(session, SESSION_STATUS_QUEUED) {
        SessionState::Queued
    } else if session.busy {
        SessionState::Working
    } else {
        SessionState::Waiting
    }
}

/// EXP-1312: a thread row's lead outside the Threads list (the rail's
/// Running rows) ×4: needs you beats working, else the bare mark
/// (`threads-list.json` `markCases`).
pub fn thread_row_mark_state(needs_you: bool, busy: bool) -> SessionState {
    if needs_you {
        SessionState::NeedsYou
    } else if busy {
        SessionState::Working
    } else {
        SessionState::Waiting
    }
}

/// Sessions of one tree in creation order (ties by id): `Session N` names.
pub fn tree_order<'a>(sessions: &[&'a SessionRow]) -> Vec<&'a SessionRow> {
    let mut ordered = sessions.to_vec();
    ordered.sort_by(|a, b| {
        ms_or_zero(a.created_at.as_deref())
            .cmp(&ms_or_zero(b.created_at.as_deref()))
            .then_with(|| a.id.cmp(&b.id))
    });
    ordered
}

/// The 1-based place of `session_id` in its tree's creation order (1 when
/// it is not in `tree`): the `ordinal` [`session_card`] takes.
pub fn tree_ordinal(session_id: &str, tree: &[&SessionRow]) -> usize {
    tree_order(tree)
        .iter()
        .position(|session| session.id == session_id)
        .map_or(1, |index| index + 1)
}

/// THE session card = the Work card's header row: icon, name, the muted
/// state word, trailing elapsed, a spinner while `running`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCard {
    pub session_id: String,
    /// Icon concept.
    pub icon: String,
    pub name: String,
    pub kind_word: String,
    pub state: SessionState,
    pub tone: String,
    pub elapsed: Option<String>,
    pub running: bool,
    pub test_id: String,
}

/// The card for `session`; `ordinal` = its 1-based place in the tree's
/// creation order (names an untitled session `Session N`).
pub fn session_card(session: &SessionRow, ordinal: usize, now_ms: i64) -> SessionCard {
    let state = session_state(session);
    let start = ms(session.started_at.as_deref());
    let end = if state == SessionState::Ended {
        ms(session.ended_at.as_deref()).or_else(|| ms(session.updated_at.as_deref()))
    } else {
        Some(now_ms)
    };
    let elapsed = match (start, end) {
        (Some(start), Some(end)) if state != SessionState::Queued => {
            Some(format_elapsed(end - start))
        }
        _ => None,
    };
    let name = match session.explicit_title() {
        Some(title) => title.to_string(),
        None => format!("{} {ordinal}", SESSION_CARD.copy.session),
    };
    SessionCard {
        session_id: session.id.clone(),
        icon: state.icon().to_string(),
        name,
        kind_word: state.word().to_string(),
        state,
        tone: state.tone().to_string(),
        elapsed,
        running: state == SessionState::Working,
        test_id: SESSION_CARD.test_ids.card.clone(),
    }
}

// ---------------------------------------------------------------------------
// The thread title
// ---------------------------------------------------------------------------

/// An issue as the tree views need it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TreeIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
}

impl TreeIssue {
    pub fn from_issue(issue: &Issue) -> Self {
        Self {
            id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
        }
    }
}

/// First line of a person's text, capped (a thread named by its request).
fn first_line(body: &str, max: usize) -> Option<String> {
    let line = body
        .split('\n')
        .map(str::trim)
        .find(|part| !part.is_empty() && !part.starts_with("!["))?;
    if line.chars().count() > max {
        let head: String = line.chars().take(max.saturating_sub(1)).collect();
        Some(format!("{}…", head.trim_end()))
    } else {
        Some(line.to_string())
    }
}

/// EXP-1316: a thread's title, never stored by a person (no rename ×4):
/// an agent's `name`; else `Run <action>`; else `Implement <IDENT>` (the
/// first issue of `issues` = the root's synced issues by sort order,
/// `+ <n-1>` with more); else the stored title (the CLI's auto-name); else
/// the first line of the root's first person message (capped); else
/// `Untitled`.
pub fn thread_title(root: &SessionRow, issues: &[TreeIssue], messages: &[&MessageRow]) -> String {
    let titles = &THREAD.titles;
    if kind_is(root, SESSION_KIND_AGENT) {
        if let Some(name) = root.name.as_deref().map(str::trim).filter(|name| !name.is_empty()) {
            return name.to_string();
        }
    }
    if let Some(action) = root.action_name.as_deref().map(str::trim).filter(|name| !name.is_empty()) {
        return titles.action.replacen("{action}", action, 1);
    }
    if let Some(first) = issues.first() {
        return if issues.len() > 1 {
            titles
                .issues
                .replacen("{issue}", &first.identifier, 1)
                .replacen("{more}", &(issues.len() - 1).to_string(), 1)
        } else {
            titles.issue.replacen("{issue}", &first.identifier, 1)
        };
    }
    if let Some(stored) = root.explicit_title() {
        return stored.to_string();
    }
    let mut own: Vec<&MessageRow> = messages
        .iter()
        .copied()
        .filter(|message| {
            message.session_id == root.id
                && message.author_kind.as_deref() == Some(MESSAGE_AUTHOR_KIND_USER)
        })
        .collect();
    own.sort_by_key(|message| ms_or_zero(message.created_at.as_deref()));
    own.first()
        .and_then(|message| first_line(message.body_text(), THREAD.title_max))
        .unwrap_or_else(|| THREAD.copy.untitled.clone())
}

/// The issues linked to `root_id`, by link sort order; unsynced issues drop
/// out.
pub fn root_issues(
    root_id: &str,
    session_issues: &[&SessionIssueRow],
    issues: &[TreeIssue],
) -> Vec<TreeIssue> {
    let by_id: HashMap<&str, &TreeIssue> = issues.iter().map(|issue| (issue.id.as_str(), issue)).collect();
    let mut links: Vec<&SessionIssueRow> = session_issues
        .iter()
        .copied()
        .filter(|link| link.session_id == root_id)
        .collect();
    links.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(0.0)
            .partial_cmp(&b.sort_order.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    links
        .into_iter()
        .filter_map(|link| by_id.get(link.issue_id.as_str()).map(|issue| (*issue).clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// The thread screen
// ---------------------------------------------------------------------------

/// One tab of the thread header strip.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ThreadTab {
    Conversation,
    Issue { issue_id: String, identifier: String },
    /// The `+` (link / file an issue).
    Add,
    /// EXP-1316: a session at the viewed level (the root's run + its direct
    /// children, or the focused session + ITS direct children).
    Session {
        session_id: String,
        name: String,
        mark_state: SessionTabMarkState,
    },
    /// `dot` = the root's guide holds any text or picture.
    Guide { dot: bool },
}

/// EXP-1316: a session chip's lead ×4 = the thread row mark of THAT
/// session, `ended` once it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTabMarkState {
    NeedsYou,
    Working,
    Waiting,
    Ended,
}

pub fn session_tab_mark_state(session: &SessionRow) -> SessionTabMarkState {
    if session.is_ended() {
        return SessionTabMarkState::Ended;
    }
    match thread_row_mark_state(session.needs_input, session.busy) {
        SessionState::NeedsYou => SessionTabMarkState::NeedsYou,
        SessionState::Working => SessionTabMarkState::Working,
        _ => SessionTabMarkState::Waiting,
    }
}

/// EXP-1322: one option of a thread question card. The synced card carries
/// `options` as bare labels with the parallel `optionDescriptions` sidecar
/// (null / "" = none; the key omitted when no option has one); they zip here
/// ([`question_options`]). The answer IS the label; the description renders
/// under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// The card's `options` zipped with its `optionDescriptions` sidecar by
/// index, BEFORE dropping non-string options (a description is the sidecar
/// slot when it is a non-empty string). No array = no options.
fn question_options(options: Option<&Value>, descriptions: Option<&Value>) -> Vec<QuestionOption> {
    let Some(Value::Array(options)) = options else {
        return Vec::new();
    };
    let descriptions = descriptions.and_then(Value::as_array);
    options
        .iter()
        .enumerate()
        .filter_map(|(index, option)| {
            let label = option.as_str()?;
            Some(QuestionOption {
                label: label.to_string(),
                description: descriptions
                    .and_then(|list| list.get(index))
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

/// One row of the thread's Conversation (the TS `ThreadRow`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SessionThreadRow {
    /// A card-less message: `author` = `user` | `agent` | `system`; a
    /// person's row is `queued` until the device consumed it.
    Message {
        id: String,
        author: String,
        author_id: Option<String>,
        body: String,
        queued: bool,
    },
    /// A session's card (`id` = the message id, or the root's id for its
    /// own leading card); opens that session.
    Session { id: String, card: SessionCard },
    Question {
        id: String,
        question: String,
        options: Vec<QuestionOption>,
        /// EXP-1316: a free-text answer is offered (always, THE card).
        custom: bool,
        answer: Option<String>,
        answered: bool,
    },
    Plan {
        id: String,
        plan: String,
        decision: Option<String>,
    },
    Subagent { id: String, title: String, status: String },
    Picture {
        id: String,
        attachment_id: String,
        width: Option<f64>,
        height: Option<f64>,
        caption: Option<String>,
    },
    /// One muted line (`Linked EXP-12`, `Merged PR #45`).
    Event { id: String, event: String, text: String },
}

impl SessionThreadRow {
    /// The row's id (the message id; the root id for its own card).
    pub fn id(&self) -> &str {
        match self {
            Self::Message { id, .. }
            | Self::Session { id, .. }
            | Self::Question { id, .. }
            | Self::Plan { id, .. }
            | Self::Subagent { id, .. }
            | Self::Picture { id, .. }
            | Self::Event { id, .. } => id,
        }
    }
}

/// Where the thread's composer sends (`sessions.send({sessionId})`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadComposer {
    pub session_id: String,
    pub placeholder: String,
}

/// The thread screen's model.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionThreadView {
    pub root_id: String,
    pub title: String,
    /// `!root.shared`: a lock glyph beside the title.
    pub private: bool,
    /// EXP-1316: the focused session (`None` = the thread).
    pub focus_id: Option<String>,
    pub tabs: Vec<ThreadTab>,
    pub rows: Vec<SessionThreadRow>,
    pub composer: ThreadComposer,
    /// Any non-ended session in the tree.
    pub live: bool,
    pub busy: bool,
    pub needs_you: bool,
}

/// [`build_session_thread`]'s input: what the caller synced.
#[derive(Debug, Clone, Copy)]
pub struct SessionThreadInput<'a> {
    pub root: &'a SessionRow,
    /// The tree's sessions (others are ignored).
    pub sessions: &'a [&'a SessionRow],
    pub session_issues: &'a [&'a SessionIssueRow],
    pub issues: &'a [TreeIssue],
    pub messages: &'a [&'a MessageRow],
    pub pull_requests: &'a [&'a PullRequestRow],
    pub now_ms: i64,
}

fn event_text(
    card: &Value,
    issues: &HashMap<&str, &TreeIssue>,
    prs: &HashMap<&str, &PullRequestRow>,
) -> Option<String> {
    let event = js_string(card.get("event"));
    let template = THREAD.copy.events.get(&event)?;
    if template.contains("{issue}") {
        let issue = card_str(card, "issueId").and_then(|id| issues.get(id))?;
        return Some(template.replacen("{issue}", &issue.identifier, 1));
    }
    if template.contains("{pr}") {
        let pr = card_str(card, "pullRequestId").and_then(|id| prs.get(id))?;
        let number = pr.number?;
        return Some(template.replacen("{pr}", &format!("#{number}"), 1));
    }
    Some(template.clone())
}

fn guide_dot(guide: Option<&Value>) -> bool {
    let Some(Value::Array(entries)) = guide else {
        return false;
    };
    entries.iter().any(|entry| {
        let text = entry.get("text").and_then(Value::as_str).unwrap_or_default();
        let label = entry.get("label").is_some_and(|label| !label.is_null());
        !text.trim().is_empty() || label
    })
}

/// EXP-1306: the ONE pull request the thread's Guide speaks for (its stack
/// rail + the bottom Merge capsule): the newest open one, else the newest
/// merged one (`created_at`, ties by the higher number); a closed PR never
/// counts.
pub fn guide_pull_request<'a>(
    pull_requests: impl IntoIterator<Item = &'a PullRequestRow>,
) -> Option<&'a PullRequestRow> {
    let mut open: Option<&PullRequestRow> = None;
    let mut merged: Option<&PullRequestRow> = None;
    for pull in pull_requests {
        let slot = if pull.is_open() {
            &mut open
        } else if pull.is_merged() {
            &mut merged
        } else {
            continue;
        };
        let newer = slot.is_none_or(|best| {
            let diff = ms_or_zero(pull.created_at.as_deref()) - ms_or_zero(best.created_at.as_deref());
            diff > 0 || (diff == 0 && pull.number.unwrap_or(0) > best.number.unwrap_or(0))
        });
        if newer {
            *slot = Some(pull);
        }
    }
    open.or(merged)
}

/// EXP-1307: one entry of the thread header's Guide `…`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideMenuEntry {
    OpenGithub,
    ClosePr,
}

impl GuideMenuEntry {
    /// The fixture's entry id.
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenGithub => "open-github",
            Self::ClosePr => "close-pr",
        }
    }
}

/// EXP-1307: the Guide `…` entries for the Guide PR's `state` (None = no
/// PR = no menu): Open on GitHub always, then Close PR while the PR is open
/// and the viewer is a member.
pub fn guide_menu_entries(state: Option<&str>, member: bool) -> Vec<GuideMenuEntry> {
    match state {
        None => Vec::new(),
        Some("open") if member => vec![GuideMenuEntry::OpenGithub, GuideMenuEntry::ClosePr],
        Some(_) => vec![GuideMenuEntry::OpenGithub],
    }
}

/// EXP-1306 (EXP-820 ×4): a question or plan card waiting on the viewer
/// takes the session composer's place.
pub fn composer_yields_to_card(live: bool, pending_ask: bool) -> bool {
    live && pending_ask
}

/// The thread screen: the header tabs (Conversation · one chip per issue ·
/// + · session chips · Guide; the session chips = the viewed level: the
/// root's run + its direct children, or with `focus` that session + ITS
/// direct children), the root's conversation as rows (its own session card
/// first, then its messages oldest first; a `session`/`thread_started` card
/// becomes that session's card) and the composer target (the root).
pub fn build_session_thread(input: SessionThreadInput<'_>, focus: Option<&str>) -> SessionThreadView {
    let root = input.root;
    let now = input.now_ms;
    let members: Vec<&SessionRow> = input
        .sessions
        .iter()
        .copied()
        .filter(|session| session.root_id_or_self() == root.id || session.id == root.id)
        .collect();
    let mut tree = tree_order(&members);
    if !tree.iter().any(|session| session.id == root.id) {
        tree.insert(0, root);
    }
    let ordinals: HashMap<&str, usize> = tree
        .iter()
        .enumerate()
        .map(|(index, session)| (session.id.as_str(), index + 1))
        .collect();
    let mut by_id: HashMap<&str, &SessionRow> =
        input.sessions.iter().map(|session| (session.id.as_str(), *session)).collect();
    by_id.insert(root.id.as_str(), root);
    let issues = root_issues(&root.id, input.session_issues, input.issues);
    let issue_map: HashMap<&str, &TreeIssue> =
        input.issues.iter().map(|issue| (issue.id.as_str(), issue)).collect();
    let pr_map: HashMap<&str, &PullRequestRow> =
        input.pull_requests.iter().map(|pr| (pr.id.as_str(), *pr)).collect();
    let mut own: Vec<&MessageRow> = input
        .messages
        .iter()
        .copied()
        .filter(|message| message.session_id == root.id)
        .collect();
    own.sort_by(|a, b| {
        ms_or_zero(a.created_at.as_deref())
            .cmp(&ms_or_zero(b.created_at.as_deref()))
            .then_with(|| a.id.cmp(&b.id))
    });
    let title = thread_title(root, &issues, &own);
    let focus = focus.and_then(|id| tree.iter().copied().find(|session| session.id == id));
    let level = focus.unwrap_or(root);
    let session_tabs: Vec<ThreadTab> = std::iter::once(level)
        .chain(
            tree.iter()
                .copied()
                .filter(|session| session.parent_id.as_deref() == Some(level.id.as_str()) && session.id != level.id),
        )
        .map(|session| ThreadTab::Session {
            session_id: session.id.clone(),
            name: session_card(session, ordinals.get(session.id.as_str()).copied().unwrap_or(1), now).name,
            mark_state: session_tab_mark_state(session),
        })
        .collect();

    let mut rows = vec![SessionThreadRow::Session {
        id: root.id.clone(),
        card: session_card(root, 1, now),
    }];
    for message in own {
        let Some(card) = message.card.as_ref() else {
            let author = match message.author_kind.as_deref() {
                Some(MESSAGE_AUTHOR_KIND_USER) => MESSAGE_AUTHOR_KIND_USER,
                Some(MESSAGE_AUTHOR_KIND_AGENT) => MESSAGE_AUTHOR_KIND_AGENT,
                _ => crate::contract::MESSAGE_AUTHOR_KIND_SYSTEM,
            };
            if message.body_text().trim().is_empty() {
                continue;
            }
            rows.push(SessionThreadRow::Message {
                id: message.id.clone(),
                author: author.to_string(),
                author_id: message.author_id.clone(),
                body: message.body_text().to_string(),
                queued: author == MESSAGE_AUTHOR_KIND_USER
                    && message.consumed_at.as_deref().is_none_or(str::is_empty),
            });
            continue;
        };
        match card_str(card, "kind") {
            Some(MESSAGE_CARD_KIND_SESSION) | Some(MESSAGE_CARD_KIND_THREAD_STARTED) => {
                let Some(target) = card_str(card, "sessionId").and_then(|id| by_id.get(id)) else {
                    continue;
                };
                rows.push(SessionThreadRow::Session {
                    id: message.id.clone(),
                    card: session_card(
                        target,
                        ordinals.get(target.id.as_str()).copied().unwrap_or(1),
                        now,
                    ),
                });
            }
            Some(MESSAGE_CARD_KIND_QUESTION) => rows.push(SessionThreadRow::Question {
                id: message.id.clone(),
                question: js_string(card.get("question")),
                options: question_options(card.get("options"), card.get("optionDescriptions")),
                custom: true,
                answer: card_str(card, "answer").map(str::to_string),
                answered: message.answered_at.is_some(),
            }),
            Some(MESSAGE_CARD_KIND_PLAN) => rows.push(SessionThreadRow::Plan {
                id: message.id.clone(),
                plan: js_string(card.get("plan")),
                decision: card_str(card, "decision").map(str::to_string),
            }),
            Some(MESSAGE_CARD_KIND_SUBAGENT) => rows.push(SessionThreadRow::Subagent {
                id: message.id.clone(),
                title: js_string(card.get("title")),
                status: js_string(card.get("status")),
            }),
            Some(MESSAGE_CARD_KIND_PICTURE) => {
                let Some(attachment_id) = card_str(card, "attachmentId") else {
                    continue;
                };
                rows.push(SessionThreadRow::Picture {
                    id: message.id.clone(),
                    attachment_id: attachment_id.to_string(),
                    width: card.get("width").and_then(Value::as_f64),
                    height: card.get("height").and_then(Value::as_f64),
                    caption: card_str(card, "caption").map(str::to_string),
                });
            }
            Some(MESSAGE_CARD_KIND_EVENT) => {
                if let Some(text) = event_text(card, &issue_map, &pr_map) {
                    rows.push(SessionThreadRow::Event {
                        id: message.id.clone(),
                        event: js_string(card.get("event")),
                        text,
                    });
                }
            }
            _ => {}
        }
    }

    let live: Vec<&SessionRow> = tree.iter().copied().filter(|session| !session.is_ended()).collect();
    let mut tabs = vec![ThreadTab::Conversation];
    tabs.extend(issues.iter().map(|issue| ThreadTab::Issue {
        issue_id: issue.id.clone(),
        identifier: issue.identifier.clone(),
    }));
    tabs.push(ThreadTab::Add);
    tabs.extend(session_tabs);
    tabs.push(ThreadTab::Guide {
        dot: guide_dot(root.guide.as_ref()),
    });
    SessionThreadView {
        root_id: root.id.clone(),
        title,
        private: !root.shared,
        focus_id: focus.map(|session| session.id.clone()),
        tabs,
        rows,
        composer: ThreadComposer {
            session_id: root.id.clone(),
            placeholder: THREAD.copy.placeholder.clone(),
        },
        live: !live.is_empty(),
        busy: live.iter().any(|session| session.busy),
        needs_you: live.iter().any(|session| session.needs_input),
    }
}

// ---------------------------------------------------------------------------
// The Threads page + the rail's Agents
// ---------------------------------------------------------------------------

/// One row of the Threads page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsListRow {
    pub session_id: String,
    pub team_id: String,
    pub title: String,
    pub private: bool,
    /// EXP-1318: the ROOT session's logical `device_id` (the trailing
    /// device icon); `None` when unset.
    #[serde(default)]
    pub device_id: Option<String>,
    pub busy: bool,
    pub needs_you: bool,
    /// The tree's newest `updated_at` (the sort key), verbatim.
    pub activity_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadsListKey {
    NeedsYou,
    Running,
    Recent,
}

impl ThreadsListKey {
    pub const ALL: [ThreadsListKey; 3] = [Self::NeedsYou, Self::Running, Self::Recent];

    /// The band title (`Needs you`, `Running`, `Recent`).
    pub fn title(self) -> &'static str {
        let groups = &THREADS_LIST.copy.groups;
        match self {
            Self::NeedsYou => &groups.needs_you,
            Self::Running => &groups.running,
            Self::Recent => &groups.recent,
        }
    }
}

/// One band of the Threads page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsListGroup {
    pub key: ThreadsListKey,
    pub title: String,
    pub rows: Vec<ThreadsListRow>,
}

/// [`partition_threads`]'s input.
#[derive(Debug, Clone, Copy)]
pub struct ThreadsListInput<'a> {
    pub sessions: &'a [&'a SessionRow],
    pub session_issues: &'a [&'a SessionIssueRow],
    pub issues: &'a [TreeIssue],
    pub messages: &'a [&'a MessageRow],
    /// The ACTIVE team; `None` = every team.
    pub team_id: Option<&'a str>,
}

/// The Threads page: roots of kind `thread` in scope grouped Needs you (a
/// live session of the tree waits on a person) · Running (any session
/// queued or running) · Recent (the rest), each by the tree's newest
/// `updated_at` desc (ties by id). Empty groups are omitted.
pub fn partition_threads(input: ThreadsListInput<'_>) -> Vec<ThreadsListGroup> {
    let mut by_root: HashMap<&str, Vec<&SessionRow>> = HashMap::new();
    for session in input.sessions {
        by_root.entry(session.root_id_or_self()).or_default().push(session);
    }
    let mut buckets: HashMap<ThreadsListKey, Vec<ThreadsListRow>> = HashMap::new();
    for root in input.sessions.iter().copied().filter(|session| {
        is_root(session)
            && kind_is(session, SESSION_KIND_THREAD)
            && input
                .team_id
                .is_none_or(|team| session.team_id.as_deref() == Some(team))
    }) {
        let alone = [root];
        let tree: &[&SessionRow] = by_root.get(root.id.as_str()).map_or(&alone[..], Vec::as_slice);
        let live: Vec<&SessionRow> = tree.iter().copied().filter(|session| !session.is_ended()).collect();
        let needs_you = live.iter().any(|session| session.needs_input);
        let running = live.iter().any(|session| {
            status_is(session, SESSION_STATUS_QUEUED) || status_is(session, SESSION_STATUS_RUNNING)
        });
        let mut activity = root.updated_at.clone().unwrap_or_default();
        for session in tree {
            if ms_or_zero(session.updated_at.as_deref()) > ms_or_zero(Some(&activity)) {
                activity = session.updated_at.clone().unwrap_or_default();
            }
        }
        let title = thread_title(
            root,
            &root_issues(&root.id, input.session_issues, input.issues),
            input.messages,
        );
        let key = if needs_you {
            ThreadsListKey::NeedsYou
        } else if running {
            ThreadsListKey::Running
        } else {
            ThreadsListKey::Recent
        };
        buckets.entry(key).or_default().push(ThreadsListRow {
            session_id: root.id.clone(),
            team_id: root.team_id.clone().unwrap_or_default(),
            title,
            private: !root.shared,
            device_id: root.device_id.clone(),
            busy: live.iter().any(|session| session.busy),
            needs_you,
            activity_at: activity,
        });
    }
    ThreadsListKey::ALL
        .into_iter()
        .filter_map(|key| {
            let mut rows = buckets.remove(&key)?;
            rows.sort_by(|a, b| {
                ms_or_zero(Some(&b.activity_at))
                    .cmp(&ms_or_zero(Some(&a.activity_at)))
                    .then_with(|| a.session_id.cmp(&b.session_id))
            });
            Some(ThreadsListGroup {
                key,
                title: key.title().to_string(),
                rows,
            })
        })
        .filter(|group| !group.rows.is_empty())
        .collect()
}

/// EXP-1309/1318: the ONE quiet trailing word of a thread row ×4 (the lock
/// rides beside it when private).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThreadRowWord {
    Stack,
    ReadyToMerge,
    Merged,
    Question,
}

impl ThreadRowWord {
    /// The fixture key (`readyToMerge`, …).
    pub fn key(self) -> &'static str {
        match self {
            Self::Stack => "stack",
            Self::ReadyToMerge => "readyToMerge",
            Self::Merged => "merged",
            Self::Question => "question",
        }
    }

    /// The copy (`ready to merge`, …).
    pub fn text(self) -> &'static str {
        let words = &THREADS_LIST.copy.words;
        match self {
            Self::Stack => &words.stack,
            Self::ReadyToMerge => &words.ready_to_merge,
            Self::Merged => &words.merged,
            Self::Question => &words.question,
        }
    }
}

/// A PR as the stack test needs it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackPr {
    pub id: String,
    pub state: String,
    pub base_pull_request_id: Option<String>,
}

impl StackPr {
    pub fn from_row(row: &PullRequestRow) -> Self {
        Self {
            id: row.id.clone(),
            state: row.state.clone().unwrap_or_default(),
            base_pull_request_id: row.base_pull_request_id.clone(),
        }
    }
}

/// An open PR of the thread sits in an open stack: based on another open
/// PR, or another open PR is based on it (`team_prs` = the team's PRs).
pub fn thread_in_stack(pull_requests: &[StackPr], team_prs: &[StackPr]) -> bool {
    let open: std::collections::HashSet<&str> = team_prs
        .iter()
        .filter(|pr| pr.state == "open")
        .map(|pr| pr.id.as_str())
        .collect();
    pull_requests.iter().filter(|pr| pr.state == "open").any(|pr| {
        pr.base_pull_request_id.as_deref().is_some_and(|base| open.contains(base))
            || team_prs
                .iter()
                .any(|other| other.state == "open" && other.base_pull_request_id.as_deref() == Some(pr.id.as_str()))
    })
}

/// Word order: an open PR in a stack → stack; an open PR while nothing
/// works → ready to merge (working = no word); else merged; else a
/// question when waiting on a person; else none. `states` = the thread's
/// PR states.
pub fn thread_row_word<'a>(
    states: impl IntoIterator<Item = &'a str>,
    busy: bool,
    needs_you: bool,
    in_stack: bool,
) -> Option<ThreadRowWord> {
    let states: Vec<&str> = states.into_iter().collect();
    if states.contains(&"open") {
        if in_stack {
            return Some(ThreadRowWord::Stack);
        }
        return (!busy).then_some(ThreadRowWord::ReadyToMerge);
    }
    if states.contains(&"merged") {
        return Some(ThreadRowWord::Merged);
    }
    needs_you.then_some(ThreadRowWord::Question)
}

/// The rail's Active band (`copy.rail`): every Needs you row, every Running
/// row, then the Recent rows whose word still asks for a merge.
pub fn rail_threads(
    groups: &[ThreadsListGroup],
    word_of: impl Fn(&ThreadsListRow) -> Option<ThreadRowWord>,
) -> Vec<ThreadsListRow> {
    groups
        .iter()
        .flat_map(|group| {
            group.rows.iter().filter(|row| {
                group.key != ThreadsListKey::Recent
                    || matches!(word_of(row), Some(ThreadRowWord::ReadyToMerge | ThreadRowWord::Stack))
            })
        })
        .cloned()
        .collect()
}

/// One row of the rail's Agents section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRow {
    pub session_id: String,
    pub team_id: String,
    pub name: String,
    /// Icon concept (default `agentDefaults.icon`).
    pub icon: String,
    /// `#rrggbb` (default `agentDefaults.color`).
    pub color: String,
    pub busy: bool,
    pub needs_you: bool,
}

/// `localeCompare`-ish: case-insensitive first, then exact.
fn locale_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
}

/// The agent roots of the team (`None` = every team) by `sort_order`, then
/// name, then id; missing icon/color fall back to the fixture defaults.
pub fn list_agents(sessions: &[&SessionRow], team_id: Option<&str>) -> Vec<AgentRow> {
    let mut agents: Vec<&SessionRow> = sessions
        .iter()
        .copied()
        .filter(|session| {
            is_root(session)
                && kind_is(session, SESSION_KIND_AGENT)
                && team_id.is_none_or(|team| session.team_id.as_deref() == Some(team))
        })
        .collect();
    agents.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(0.0)
            .partial_cmp(&b.sort_order.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                locale_cmp(a.name.as_deref().unwrap_or_default(), b.name.as_deref().unwrap_or_default())
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    let defaults = &THREADS_LIST.agent_defaults;
    agents
        .into_iter()
        .map(|session| AgentRow {
            session_id: session.id.clone(),
            team_id: session.team_id.clone().unwrap_or_default(),
            name: session.name.clone().unwrap_or_default(),
            icon: session.icon.clone().unwrap_or_else(|| defaults.icon.clone()),
            color: session.color.clone().unwrap_or_else(|| defaults.color.clone()),
            busy: session.busy,
            needs_you: session.needs_input,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Map};

    /// EXP-1322 sidecar: `optionDescriptions` zips with `options` by index
    /// (null / "" / a missing slot = none; a non-array sidecar is ignored);
    /// a non-string option drops after the zip.
    #[test]
    fn question_options_read_the_sidecar() {
        let pairs = |options: Value, descriptions: Option<Value>| -> Vec<(String, Option<String>)> {
            question_options(Some(&options), descriptions.as_ref())
                .into_iter()
                .map(|option| (option.label, option.description))
                .collect()
        };
        let some = |text: &str| Some(text.to_string());
        assert_eq!(
            pairs(
                json!(["Yes", "No", "Later", { "label": "Own" }, "Never"]),
                Some(json!(["Ship it", null, "", "Mine", "Not now"]))
            ),
            [
                ("Yes".to_string(), some("Ship it")),
                ("No".to_string(), None),
                ("Later".to_string(), None),
                ("Never".to_string(), some("Not now")),
            ]
        );
        assert_eq!(
            pairs(json!(["A", "B"]), None),
            [("A".to_string(), None), ("B".to_string(), None)]
        );
        assert_eq!(
            pairs(json!(["A"]), Some(json!("not a list"))),
            [("A".to_string(), None)]
        );
        assert_eq!(question_options(None, Some(&json!(["x"]))), Vec::<QuestionOption>::new());
        assert_eq!(question_options(Some(&json!("Yes")), None), Vec::<QuestionOption>::new());
    }

    /// A camelCase TS row → the snake_case domain row (top-level keys only:
    /// jsonb cells keep their own camelCase). `id` is synthesized when the
    /// fixture row has none (session-issue links).
    fn row<T: DeserializeOwned>(value: &Value, fallback_id: &str) -> T {
        let mut out = Map::new();
        for (key, cell) in value.as_object().expect("row object") {
            let mut snake = String::new();
            for ch in key.chars() {
                if ch.is_ascii_uppercase() {
                    snake.push('_');
                    snake.push(ch.to_ascii_lowercase());
                } else {
                    snake.push(ch);
                }
            }
            out.insert(snake, cell.clone());
        }
        out.entry("id").or_insert_with(|| json!(fallback_id));
        serde_json::from_value(Value::Object(out)).expect("fixture row hydrates")
    }

    fn rows<T: DeserializeOwned>(value: &Value) -> Vec<T> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| row(item, &format!("row-{index}")))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn now(value: &Value) -> i64 {
        parse_instant(value.as_str().expect("now")).expect("now parses")
    }

    /// Numbers compare by value (`1440` == `1440.0`).
    fn normalize(value: Value) -> Value {
        match value {
            Value::Number(number) => json!(number.as_f64().unwrap_or_default()),
            Value::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
            Value::Object(map) => {
                Value::Object(map.into_iter().map(|(key, cell)| (key, normalize(cell))).collect())
            }
            other => other,
        }
    }

    fn assert_whole(actual: impl Serialize, expected: &Value, name: &str) {
        let actual = normalize(serde_json::to_value(actual).unwrap());
        assert_eq!(actual, normalize(expected.clone()), "{name}");
    }

    fn fixture(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_copy_comes_from_the_fixtures() {
        assert_eq!(SESSION_CARD.copy.session, "Session");
        assert_eq!(SessionState::NeedsYou.word(), "Needs you");
        assert_eq!(SessionState::Working.icon(), "coding-running");
        assert_eq!(SessionState::Blocked.tone(), "rose");
        assert_eq!(THREAD.copy.placeholder, "Reply…");
        assert_eq!(THREAD.title_max, 80);
        assert_eq!(THREADS_LIST.copy.title, "Threads");
        assert_eq!(ThreadsListKey::Recent.title(), "Recent");
        assert_eq!(THREADS_LIST.agent_defaults.icon, "bot");
    }

    #[test]
    fn format_elapsed_walks_the_ladder() {
        assert_eq!(format_elapsed(37_000), "37s");
        assert_eq!(format_elapsed(124_000), "2m 04s");
        assert_eq!(format_elapsed(3_780_000), "1h 03m");
        assert_eq!(format_elapsed(-5), "0s");
    }

    /// `session-card.json` `cases`, whole objects.
    #[test]
    fn session_card_matches_the_fixture() {
        let fixture = fixture(SESSION_CARD_JSON);
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let session: SessionRow = row(&input["session"], "s");
            let ordinal = input["ordinal"].as_u64().unwrap() as usize;
            assert_whole(session_card(&session, ordinal, now(&input["now"])), &case["expected"], name);
        }
    }

    /// `session-thread.json` `cases`, whole objects.
    #[test]
    fn build_session_thread_matches_the_fixture() {
        let fixture = fixture(SESSION_THREAD_JSON);
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let root: SessionRow = row(&input["root"], "root");
            let sessions: Vec<SessionRow> = rows(&input["sessions"]);
            let links: Vec<SessionIssueRow> = rows(&input["sessionIssues"]);
            let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap();
            let messages: Vec<MessageRow> = rows(&input["messages"]);
            let prs: Vec<PullRequestRow> = rows(&input["pullRequests"]);
            let sessions: Vec<&SessionRow> = sessions.iter().collect();
            let links: Vec<&SessionIssueRow> = links.iter().collect();
            let messages: Vec<&MessageRow> = messages.iter().collect();
            let prs: Vec<&PullRequestRow> = prs.iter().collect();
            let view = build_session_thread(
                SessionThreadInput {
                root: &root,
                sessions: &sessions,
                session_issues: &links,
                issues: &issues,
                messages: &messages,
                pull_requests: &prs,
                now_ms: now(&input["now"]),
                },
                case["options"]["focus"].as_str(),
            );
            assert_whole(&view, &case["expected"], name);
        }
    }

    /// `threads-list.json` `cases` (partitionThreads), `agentCases`
    /// (listAgents), whole values, and `markCases` (EXP-1312).
    #[test]
    fn threads_list_matches_the_fixture() {
        let fixture = fixture(THREADS_LIST_JSON);
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let sessions: Vec<SessionRow> = rows(&input["sessions"]);
            let links: Vec<SessionIssueRow> = rows(&input["sessionIssues"]);
            let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap();
            let messages: Vec<MessageRow> = rows(&input["messages"]);
            let sessions: Vec<&SessionRow> = sessions.iter().collect();
            let links: Vec<&SessionIssueRow> = links.iter().collect();
            let messages: Vec<&MessageRow> = messages.iter().collect();
            let groups = partition_threads(ThreadsListInput {
                sessions: &sessions,
                session_issues: &links,
                issues: &issues,
                messages: &messages,
                team_id: input["teamId"].as_str(),
            });
            assert_whole(&groups, &case["expected"], name);
        }
        let agent_cases = fixture["agentCases"].as_array().unwrap();
        assert!(!agent_cases.is_empty());
        for case in agent_cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let sessions: Vec<SessionRow> = rows(&input["sessions"]);
            let sessions: Vec<&SessionRow> = sessions.iter().collect();
            assert_whole(list_agents(&sessions, input["teamId"].as_str()), &case["expected"], name);
        }
        let mark_cases = fixture["markCases"].as_array().unwrap();
        assert!(!mark_cases.is_empty());
        for case in mark_cases {
            let input = &case["input"];
            let state = thread_row_mark_state(input["needsYou"].as_bool().unwrap(), input["busy"].as_bool().unwrap());
            assert_eq!(Some(state.as_str()), case["expected"].as_str(), "{}", case["name"]);
        }
        assert_eq!(THREADS_LIST.copy.rail, "Active");
        let stack_cases = fixture["stackCases"].as_array().unwrap();
        assert!(!stack_cases.is_empty());
        for case in stack_cases {
            let input = &case["input"];
            let prs: Vec<StackPr> = serde_json::from_value(input["pullRequests"].clone()).unwrap();
            let team: Vec<StackPr> = serde_json::from_value(input["teamPrs"].clone()).unwrap();
            assert_eq!(
                thread_in_stack(&prs, &team),
                case["expected"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
        let word_cases = fixture["wordCases"].as_array().unwrap();
        assert!(!word_cases.is_empty());
        for case in word_cases {
            let input = &case["input"];
            let states: Vec<&str> = input["pullRequests"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pr| pr["state"].as_str().unwrap())
                .collect();
            let word = thread_row_word(
                states,
                input["busy"].as_bool().unwrap(),
                input["needsYou"].as_bool().unwrap(),
                input["inStack"].as_bool().unwrap(),
            );
            assert_eq!(word.map(ThreadRowWord::key), case["expected"].as_str(), "{}", case["name"]);
        }
        let rail_cases = fixture["railCases"].as_array().unwrap();
        assert!(!rail_cases.is_empty());
        for case in rail_cases {
            let input = &case["input"];
            let groups: Vec<ThreadsListGroup> = serde_json::from_value(input["groups"].clone()).unwrap();
            let words = &input["words"];
            let ids: Vec<String> = rail_threads(&groups, |row| {
                [ThreadRowWord::Stack, ThreadRowWord::ReadyToMerge, ThreadRowWord::Merged, ThreadRowWord::Question]
                    .into_iter()
                    .find(|word| words[row.session_id.as_str()].as_str() == Some(word.key()))
            })
            .into_iter()
            .map(|row| row.session_id)
            .collect();
            let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
    }

    /// EXP-1307: `session-thread.json` `guideMenuCases`.
    #[test]
    fn guide_menu_entries_match_the_fixture() {
        let fixture = fixture(SESSION_THREAD_JSON);
        let cases = fixture["guideMenuCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let ids: Vec<&str> = guide_menu_entries(input["state"].as_str(), input["member"].as_bool().unwrap())
                .into_iter()
                .map(GuideMenuEntry::id)
                .collect();
            let expected: Vec<&str> =
                case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
    }

    /// `session-thread.json` `guidePullRequestCases` + `composerCases`.
    #[test]
    fn guide_pull_request_and_composer_match_the_fixture() {
        let fixture = fixture(SESSION_THREAD_JSON);
        let cases = fixture["guidePullRequestCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let prs: Vec<PullRequestRow> = rows(&case["pullRequests"]);
            assert_eq!(
                guide_pull_request(&prs).map(|pull| pull.id.as_str()),
                case["expected"].as_str(),
                "{}",
                case["name"]
            );
        }
        let cases = fixture["composerCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            assert_eq!(
                composer_yields_to_card(input["live"].as_bool().unwrap(), input["pendingAsk"].as_bool().unwrap()),
                case["expected"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn tree_ordinal_counts_from_the_oldest() {
        let a: SessionRow = row(&json!({"id": "a", "createdAt": "2026-10-13T10:00:00Z"}), "a");
        let b: SessionRow = row(&json!({"id": "b", "createdAt": "2026-10-13T11:00:00Z"}), "b");
        assert_eq!(tree_ordinal("b", &[&b, &a]), 2);
        assert_eq!(tree_ordinal("zz", &[&b, &a]), 1);
    }
}
