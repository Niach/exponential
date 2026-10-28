//! EXP-1294 (Threads v2): the session tree's view models — the Rust twin of
//! `packages/domain-contract/src/session-tree.ts` (iOS ExpCore
//! `SessionTree.swift`, Android `domain/SessionTree.kt`), byte-locked by the
//! fixtures every mirror replays:
//!
//! * `fixtures/session-card.json`   → [`session_card`] (THE session card);
//! * `fixtures/session-thread.json` → [`build_session_thread`] (the thread
//!   screen: tabs, conversation rows, composer target) + [`guide_pull_request`]
//!   (the Guide's ONE PR) + [`guide_menu_entries`] (its header `…`,
//!   EXP-1307) + [`partition_thread_strip`] (the never-scrolling strip, EXP-1334);
//! * `fixtures/threads-list.json`   → [`partition_threads`] (the Threads page:
//!   Needs you · Running · Recent, live trees nested, EXP-1323) +
//!   [`threads_unread`] (the nav dot) + [`list_agents`] (the rail's Agents).
//!
//! The copy, icon concepts, tones and test ids come FROM the fixtures
//! ([`SESSION_CARD`], [`THREAD`], [`THREADS_LIST`]), never re-typed. Inputs
//! are the synced domain rows themselves ([`SessionRow`], [`SessionIssueRow`],
//! [`MessageRow`], [`PullRequestRow`]) plus a small [`TreeIssue`], so the UI
//! converts nothing. Timestamps stay ISO strings (parsed with the crate's one
//! ISO reader); `now_ms` = epoch milliseconds. Pure; the view types serialize
//! to the fixtures' camelCase shape so the replay compares whole values.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity_fold::parse_instant;
use crate::contract::{
    MESSAGE_AUTHOR_KIND_AGENT, MESSAGE_AUTHOR_KIND_USER, MESSAGE_CARD_KIND_ASK, MESSAGE_CARD_KIND_EVENT,
    MESSAGE_CARD_KIND_PICTURE, MESSAGE_CARD_KIND_PLAN, MESSAGE_CARD_KIND_QUESTION,
    MESSAGE_CARD_KIND_REPLIES, MESSAGE_CARD_KIND_REPORT, MESSAGE_CARD_KIND_SESSION,
    MESSAGE_CARD_KIND_SUBAGENT, MESSAGE_CARD_KIND_SURFACE,
    MESSAGE_CARD_KIND_SUGGESTIONS, MESSAGE_CARD_KIND_THREAD_STARTED,
    SESSION_STATUS_ENDED, SESSION_STATUS_QUEUED,
    PR_STATE_MERGED,
};
use crate::rows::{AgentDefRow, Issue, MessageRow, PullRequestRow, SessionIssueRow, SessionReadRow, SessionRow};
use crate::session_title::{session_title, session_titles};

/// A resolved title from [`session_titles`] (`""` when absent, TS `?? ''`).
fn title_of<'t>(titles: &'t HashMap<String, String>, id: &str) -> &'t str {
    titles.get(id).map_or("", String::as_str)
}

// ---------------------------------------------------------------------------
// Fixture-sourced copy
// ---------------------------------------------------------------------------

/// One string per [`SessionState`] (the fixtures key them snake_case).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateStrings {
    pub working: String,
    pub needs_you: String,
    pub idle: String,
    pub done: String,
}

impl StateStrings {
    pub fn get(&self, state: SessionState) -> &str {
        match state {
            SessionState::Working => &self.working,
            SessionState::NeedsYou => &self.needs_you,
            SessionState::Idle => &self.idle,
            SessionState::Done => &self.done,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCardCopy {
    /// The muted state word per state.
    pub states: StateStrings,
    /// NEW-1: the word per `blocked.kind` of a session that needs you
    /// (`rate_limit` → `Usage limit`, `unclaimed`, `stuck`).
    pub blocked_kinds: BTreeMap<String, String>,
}

/// EXP-1374 / S-3: `session-card.json` `banner` (text), `bannerIcons`,
/// `bannerLabels` = the Dock banner per id (background, queued).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionBannerCopy {
    pub background: String,
    /// #EXP-1405: a queued row waiting for its device.
    pub queued: String,
}

impl SessionBannerCopy {
    pub fn get(&self, id: SessionBannerId) -> &str {
        match id {
            SessionBannerId::Background => &self.background,
            SessionBannerId::Queued => &self.queued,
        }
    }
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
    /// EXP-1374: the Dock banner's text ([`session_banner`]).
    pub banner: SessionBannerCopy,
    /// S-3: the banner's icon concept per id.
    pub banner_icons: SessionBannerCopy,
    /// S-3: the banner's label per id.
    pub banner_labels: SessionBannerCopy,
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
    /// EXP-1323: a child's report line (`Report from {session}`).
    pub report: String,
    /// EXP-1323: the suggestions card's copy.
    pub suggestions: SuggestionsCopy,
    /// EXP-1211: the quick replies row above the composer.
    pub replies: RepliesCopy,
    /// S-3: the engine's notice for a turn that ended with nothing said,
    /// thought or done (`{reason}` = the ACP stopReason).
    #[serde(rename = "emptyTurn")]
    pub empty_turn: String,
}

/// EXP-1211: `session-thread.json` `copy.replies`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepliesCopy {
    /// The row's accessible label (`Quick replies`).
    pub label: String,
    /// The trailing × button's label (`Dismiss quick replies`).
    pub dismiss: String,
}

/// EXP-1323: `session-thread.json` `copy.suggestions` (`{count}` /
/// `{issue}` slots).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionsCopy {
    pub one: String,
    pub many: String,
    pub started: String,
    pub start_all: String,
    pub start: String,
    pub here: String,
    pub options: String,
    pub start_on: String,
    pub start_with: String,
    pub file: String,
    pub dismiss: String,
    pub sent_here: String,
    pub filed: String,
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
    /// EXP-1323: a report row, the suggestions card, the live tail.
    pub report: String,
    pub suggestions: String,
    /// EXP-1211: the quick replies row.
    pub replies: String,
    #[serde(rename = "liveTail")]
    pub live_tail: String,
}

/// `fixtures/session-thread.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSpec {
    pub copy: ThreadCopy,
    pub test_ids: ThreadTestIds,
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
    /// S-3: the fold row's label ([`earlier_label`]).
    pub earlier: EarlierCopy,
}

/// S-3: `threads-list.json` `copy.earlier` (`{n}` slot).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EarlierCopy {
    pub one: String,
    pub other: String,
}

/// `threads-list.json` `copy.words`. `deny_unknown_fields`: a word key
/// re-added to the fixture must fail here as it does on iOS/Android, which
/// assert the key set.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadRowWords {
    pub stack: String,
    pub ready_to_merge: String,
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
    /// NEW-1: the Recent band's window + fill.
    pub recent: RecentRules,
    /// S-3: a nested child shows while active within this many minutes
    /// ([`visible_child`]).
    pub recent_child_minutes: i64,
}

/// NEW-1: `threads-list.json` `recent`: every tree active within `hours`,
/// filled up to the last `max` by activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RecentRules {
    pub hours: i64,
    pub max: usize,
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

/// EXP-1323: an ISO timestamp as epoch ms through the crate's one reader
/// (the read-mark comparisons outside this module).
pub fn iso_ms(iso: &str) -> Option<i64> {
    ms(Some(iso))
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

/// S-3 (owner 2026-10-28): the FOUR display states ×4, nothing else (no
/// running, paused, interrupted or stale in the UI). `working` = queued or
/// busy (background tasks included); `needs_you` = needs input (a card, a
/// usage wall, an unclaimed/stuck flag); `done` = ended; `idle` = the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Working,
    NeedsYou,
    Idle,
    Done,
}

impl SessionState {
    /// The fixture's wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NeedsYou => "needs_you",
            Self::Idle => "idle",
            Self::Done => "done",
        }
    }

    /// The muted state word (`Working`, `Needs you`, `Idle`, `Done`).
    pub fn word(self) -> &'static str {
        SESSION_CARD.copy.states.get(self)
    }

    /// The icon concept.
    pub fn icon(self) -> &'static str {
        SESSION_CARD.icons.get(self)
    }

    /// The tone token (`muted` | `foreground` | `amber`).
    pub fn tone(self) -> &'static str {
        SESSION_CARD.tones.get(self)
    }

    /// The card's spinner (= working).
    pub fn running(self) -> bool {
        self == Self::Working
    }
}

/// S-3 (TS `sessionState`): ended → done; needs input → needs you; queued
/// or busy → working; else idle.
pub fn session_state(session: &SessionRow) -> SessionState {
    if status_is(session, SESSION_STATUS_ENDED) {
        SessionState::Done
    } else if session.needs_input {
        SessionState::NeedsYou
    } else if status_is(session, SESSION_STATUS_QUEUED) || session.busy {
        SessionState::Working
    } else {
        SessionState::Idle
    }
}

/// S-3 (TS `visibleChild`): a NESTED session (a child row, a strip chip)
/// shows while it works or needs you, or while its last activity (done:
/// `ended_at`, else `updated_at`) is within `recentChildMinutes` of `now`
/// (`None` = never).
pub fn visible_child(session: &SessionRow, now_ms: Option<i64>) -> bool {
    let state = session_state(session);
    if matches!(state, SessionState::Working | SessionState::NeedsYou) {
        return true;
    }
    let last = if state == SessionState::Done {
        ms(session.ended_at.as_deref()).or_else(|| ms(session.updated_at.as_deref()))
    } else {
        ms(session.updated_at.as_deref())
    };
    match (now_ms, last) {
        (Some(at), Some(last)) => at - last <= THREADS_LIST.recent_child_minutes * 60 * 1000,
        _ => false,
    }
}

/// S-3 (TS `visibleTree`): the ids of a tree's sessions that show nested:
/// [`visible_child`] or an ancestor of one (a working grandchild keeps its
/// idle parent).
pub fn visible_tree<'a>(tree: &[&'a SessionRow], now_ms: Option<i64>) -> HashSet<&'a str> {
    let by_id: HashMap<&str, &'a SessionRow> = tree.iter().map(|session| (session.id.as_str(), *session)).collect();
    let mut out: HashSet<&'a str> = HashSet::new();
    for session in tree.iter().copied() {
        if !visible_child(session, now_ms) {
            continue;
        }
        let mut at = Some(session);
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(current) = at {
            if !seen.insert(current.id.as_str()) {
                break;
            }
            out.insert(current.id.as_str());
            at = current.parent_id.as_deref().and_then(|parent| by_id.get(parent).copied());
        }
    }
    out
}

/// EXP-1312: a thread row's lead outside the Threads list (the rail's
/// Running rows) ×4: needs you beats working, else the bare mark
/// (`threads-list.json` `markCases`; the bare mark = [`SessionState::Idle`],
/// the fixture's `waiting`, [`thread_row_mark_word`]).
pub fn thread_row_mark_state(needs_you: bool, busy: bool) -> SessionState {
    if needs_you {
        SessionState::NeedsYou
    } else if busy {
        SessionState::Working
    } else {
        SessionState::Idle
    }
}

/// The wire word of a [`thread_row_mark_state`] (TS `threadRowMarkState`:
/// `needs_you` | `working` | `waiting`).
pub fn thread_row_mark_word(state: SessionState) -> &'static str {
    match state {
        SessionState::NeedsYou => "needs_you",
        SessionState::Working => "working",
        SessionState::Idle | SessionState::Done => "waiting",
    }
}

/// #EXP-1380: a thread row's PR lead (`threadRowLeadPr`): an open PR
/// beats a merged one; none = the bare mark.
pub fn thread_row_lead_pr<'a>(states: impl IntoIterator<Item = &'a str>) -> Option<PrLead> {
    let states: Vec<&str> = states.into_iter().collect();
    if states.contains(&"open") {
        Some(PrLead::Open)
    } else if states.contains(&"merged") {
        Some(PrLead::Merged)
    } else {
        None
    }
}

/// [`thread_row_lead_pr`]'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrLead {
    Open,
    Merged,
}

/// Sessions of one tree in creation order (ties by id).
pub fn tree_order<'a>(sessions: &[&'a SessionRow]) -> Vec<&'a SessionRow> {
    let mut ordered = sessions.to_vec();
    ordered.sort_by(|a, b| {
        ms_or_zero(a.created_at.as_deref())
            .cmp(&ms_or_zero(b.created_at.as_deref()))
            .then_with(|| a.id.cmp(&b.id))
    });
    ordered
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

/// NEW-1: the `blocked.kind` of a blocked session (`None` when it carries
/// none: a usage wall from before the kinds).
pub fn blocked_kind(session: &SessionRow) -> Option<&str> {
    session
        .blocked
        .as_ref()?
        .get("kind")?
        .as_str()
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
}

/// NEW-1: whether a blocked session sits behind a USAGE wall (`rate_limit`,
/// or no kind at all) — the only block another account can lift (the
/// Switch account control shows for it alone).
pub fn is_usage_wall(session: &SessionRow) -> bool {
    session.blocked.is_some() && blocked_kind(session).is_none_or(|kind| kind == "rate_limit")
}

/// NEW-1 (TS `sessionKindWord`): the card's state word; a session that
/// needs you because of a flag names WHY (`copy.blockedKinds[blocked.kind]`).
pub fn session_kind_word(session: &SessionRow, state: SessionState) -> String {
    if state == SessionState::NeedsYou {
        if let Some(word) = blocked_kind(session).and_then(|kind| SESSION_CARD.copy.blocked_kinds.get(kind)) {
            return word.clone();
        }
    }
    state.word().to_string()
}

/// NEW-1 (web `blockedBadgeLabel`): a WATCHDOG flag (`device_offline`,
/// `unclaimed`, `stuck`) names itself with the card's word and has no reset
/// to count down to; `None` for a usage wall (or an unknown kind), which
/// keeps its own label + countdown.
pub fn watchdog_blocked_word(session: &SessionRow) -> Option<String> {
    let kind = blocked_kind(session).filter(|kind| *kind != "rate_limit")?;
    SESSION_CARD.copy.blocked_kinds.get(kind).cloned()
}

/// The card for `session`; `title` = its [`session_title`] (EXP-1341,
/// `fixtures/session-title.json`; never `Session N`).
pub fn session_card(session: &SessionRow, title: &str, now_ms: i64) -> SessionCard {
    let state = session_state(session);
    let start = ms(session.started_at.as_deref());
    // S-3: elapsed for working (not queued; to now) and done (to endedAt,
    // else updatedAt) only.
    let end = match state {
        SessionState::Done => ms(session.ended_at.as_deref()).or_else(|| ms(session.updated_at.as_deref())),
        SessionState::Working if !status_is(session, SESSION_STATUS_QUEUED) => Some(now_ms),
        _ => None,
    };
    let elapsed = match (start, end) {
        (Some(start), Some(end)) => Some(format_elapsed(end - start)),
        _ => None,
    };
    let name = title.to_string();
    SessionCard {
        session_id: session.id.clone(),
        icon: state.icon().to_string(),
        name,
        kind_word: session_kind_word(session, state),
        state,
        tone: state.tone().to_string(),
        elapsed,
        running: state.running(),
        test_id: SESSION_CARD.test_ids.card.clone(),
    }
}

/// EXP-1374: the Dock card of a background session and (#EXP-1405) of a
/// queued one waiting for its device (a dock.json `banner` source without
/// its `kind`; TS `SessionBanner`). S-3: an interruption has no card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionBanner {
    pub id: SessionBannerId,
    /// Icon concept.
    pub icon: String,
    pub label: String,
    pub text: String,
    /// Always `muted`.
    pub tone: String,
    /// Always `None` (no action).
    pub action: Option<String>,
}

/// [`SessionBanner::id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBannerId {
    Background,
    Queued,
}

/// EXP-1374 / S-3 (TS `sessionBanner`): `None` once ended or needing you;
/// queued → `queued`; busy on background tasks → `background`; else `None`.
pub fn session_banner(session: &SessionRow) -> Option<SessionBanner> {
    if session.is_ended() || session.needs_input {
        return None;
    }
    let id = if status_is(session, SESSION_STATUS_QUEUED) {
        SessionBannerId::Queued
    } else if session.busy && session.background {
        SessionBannerId::Background
    } else {
        return None;
    };
    Some(SessionBanner {
        id,
        icon: SESSION_CARD.banner_icons.get(id).to_string(),
        label: SESSION_CARD.banner_labels.get(id).to_string(),
        text: SESSION_CARD.banner.get(id).to_string(),
        tone: "muted".to_string(),
        action: None,
    })
}

// ---------------------------------------------------------------------------
// The tree issue
// ---------------------------------------------------------------------------

/// An issue as the tree views need it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TreeIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    /// EXP-1334: the issue's parent (`issue_relations` kind parent; absent =
    /// None): a linked sub-issue folds under its nearest LINKED ancestor.
    #[serde(default, rename = "parentId")]
    pub parent_id: Option<String>,
}

impl TreeIssue {
    pub fn from_issue(issue: &Issue) -> Self {
        Self {
            id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            parent_id: None,
        }
    }

    /// [`Self::from_issue`] with its parent (`issue_relations` kind parent).
    pub fn with_parent(mut self, parent_id: Option<String>) -> Self {
        self.parent_id = parent_id;
        self
    }
}

/// NEW-1: a thread's issues = the UNION over its tree's rows: the sessions
/// in tree order (the root first, then creation order, [`tree_order`]), each
/// one's rows by sort order then issue id, every issue once; unsynced issues
/// drop out.
pub fn tree_issues(
    tree: &[&SessionRow],
    session_issues: &[&SessionIssueRow],
    issues: &[TreeIssue],
) -> Vec<TreeIssue> {
    let by_id: HashMap<&str, &TreeIssue> = issues.iter().map(|issue| (issue.id.as_str(), issue)).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for session in tree {
        let mut links: Vec<&SessionIssueRow> = session_issues
            .iter()
            .copied()
            .filter(|link| link.session_id == session.id)
            .collect();
        links.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(0.0)
                .partial_cmp(&b.sort_order.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.issue_id.cmp(&b.issue_id))
        });
        for link in links {
            let Some(issue) = by_id.get(link.issue_id.as_str()) else {
                continue;
            };
            if seen.insert(issue.id.as_str()) {
                out.push((*issue).clone());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The thread screen
// ---------------------------------------------------------------------------

/// One tab of the thread header strip.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ThreadTab {
    /// The `+` (link / file an issue), LEFT of Conversation (EXP-1334).
    Add,
    Conversation,
    /// EXP-1334: a TOP-LEVEL linked issue; its linked sub-issues fold into
    /// `nested` (the chip's dropdown).
    Issue {
        issue_id: String,
        identifier: String,
        nested: Vec<ThreadTabNestedIssue>,
    },
    /// EXP-1334: one of the dispatcher's DIRECT children; every deeper
    /// session folds into `nested`.
    Session {
        session_id: String,
        name: String,
        mark_state: SessionTabMarkState,
        nested: Vec<ThreadTabNestedSession>,
    },
    /// `dot` = the root's guide holds any text or picture.
    Guide { dot: bool },
}

/// EXP-1334: one row of a folded issue chip's subtree; `depth` is relative
/// to the chip (its children = 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTabNestedIssue {
    pub issue_id: String,
    pub identifier: String,
    pub depth: usize,
}

/// EXP-1334: one row of a folded session chip's subtree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTabNestedSession {
    pub session_id: String,
    pub name: String,
    pub mark_state: SessionTabMarkState,
    pub depth: usize,
}

/// EXP-1316 / S-3: a session chip's lead ×4 = THAT session's display
/// state ([`session_state`]).
pub type SessionTabMarkState = SessionState;

pub fn session_tab_mark_state(session: &SessionRow) -> SessionTabMarkState {
    session_state(session)
}

/// EXP-1322: one option of a thread question card. The synced card carries
/// `options` as bare labels with the parallel `optionDescriptions` sidecar
/// (null / "" = none; the key omitted when no option has one); they zip here
/// ([`question_options`]). The answer IS the label; the description renders
/// under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
    /// EXP-1280: the option's picture (plan-card.json `pictureFor`);
    /// `None` = a text option.
    pub attachment_id: Option<String>,
}

/// EXP-1280: one choice question of a plan card in the row form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanQuestion {
    pub id: String,
    pub prompt: String,
    pub multi: bool,
    /// Free text allowed (default true).
    pub custom: bool,
    pub options: Vec<QuestionOption>,
}

/// EXP-1280: a card's inline Exponential UI surface (`root` = a core
/// NestedNode, `data` = its initial data model).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CardSurface {
    pub root: Value,
    pub data: Option<Value>,
}

/// A card's surface (tolerant read): an object with a `root` object, else
/// `None`; `data` only when it is an object.
fn card_surface(raw: Option<&Value>) -> Option<CardSurface> {
    let surface = raw?.as_object()?;
    let root = surface.get("root").filter(|root| root.is_object())?;
    Some(CardSurface {
        root: root.clone(),
        data: surface.get("data").filter(|data| data.is_object()).cloned(),
    })
}

/// A plan card's `questions` in the row form; a malformed question (no
/// string id/prompt) or option (no string label) drops. An option's
/// `picture` label resolves through the card's `pictures`.
fn plan_questions(card: &Value) -> Vec<PlanQuestion> {
    let Some(questions) = card.get("questions").and_then(Value::as_array) else {
        return Vec::new();
    };
    let pictures = card.get("pictures");
    questions
        .iter()
        .filter_map(|question| {
            let id = question.get("id")?.as_str()?;
            let prompt = question.get("prompt")?.as_str()?;
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .filter_map(|option| {
                    let label = option.get("label")?.as_str()?;
                    Some(QuestionOption {
                        label: label.to_string(),
                        description: option
                            .get("description")
                            .and_then(Value::as_str)
                            .filter(|text| !text.is_empty())
                            .map(str::to_string),
                        attachment_id: crate::plan_card::picture_for(
                            pictures,
                            option.get("picture").and_then(Value::as_str),
                        ),
                    })
                })
                .collect();
            Some(PlanQuestion {
                id: id.to_string(),
                prompt: prompt.to_string(),
                multi: question.get("multi").and_then(Value::as_bool) == Some(true),
                custom: question.get("custom").and_then(Value::as_bool) != Some(false),
                options,
            })
        })
        .collect()
}

/// A plan card's `answers` (question id → labels, strings only); `None`
/// when not an object.
fn plan_answers(raw: Option<&Value>) -> Option<BTreeMap<String, Vec<String>>> {
    let answers = raw?.as_object()?;
    Some(
        answers
            .iter()
            .filter_map(|(id, labels)| Some((id.clone(), string_list(Some(labels))?)))
            .collect(),
    )
}

fn string_list(raw: Option<&Value>) -> Option<Vec<String>> {
    Some(raw?.as_array()?.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
}

/// The card's `options` zipped with its `optionDescriptions` sidecar by
/// index, BEFORE dropping non-string options (a description is the sidecar
/// slot when it is a non-empty string). No array = no options.
fn question_options(card: &Value) -> Vec<QuestionOption> {
    let Some(Value::Array(options)) = card.get("options") else {
        return Vec::new();
    };
    let descriptions = card.get("optionDescriptions").and_then(Value::as_array);
    let picture_labels = card.get("optionPictures").and_then(Value::as_array);
    let pictures = card.get("pictures");
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
                attachment_id: crate::plan_card::picture_for(
                    pictures,
                    picture_labels.and_then(|list| list.get(index)).and_then(Value::as_str),
                ),
            })
        })
        .collect()
}

/// Thread 58cb32df (chats.json SURFACE CARD): a `surface` message as its
/// Conversation row (the thread's, and the session view's): the title
/// trimmed (`None` when blank); no parseable surface = no row.
pub fn surface_card_row(message: &MessageRow) -> Option<SessionThreadRow> {
    let card = message.card.as_ref()?;
    if card_str(card, "kind") != Some(MESSAGE_CARD_KIND_SURFACE) {
        return None;
    }
    let surface = card_surface(card.get("surface"))?;
    Some(SessionThreadRow::Surface {
        id: message.id.clone(),
        session_id: message.session_id.clone(),
        title: card_str(card, "title")
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string),
        surface,
    })
}

/// EXP-1280: a `question` / `plan` message as its Conversation row (the
/// thread's, and the session view's tool-posted card); any other message =
/// `None`.
pub fn ask_card_row(message: &MessageRow) -> Option<SessionThreadRow> {
    let card = message.card.as_ref()?;
    Some(match card_str(card, "kind") {
        Some(MESSAGE_CARD_KIND_QUESTION) => SessionThreadRow::Question {
            id: message.id.clone(),
            session_id: message.session_id.clone(),
            question: js_string(card.get("question")),
            options: question_options(card),
            custom: card.get("custom").and_then(Value::as_bool) != Some(false),
            multi: card.get("multi").and_then(Value::as_bool) == Some(true),
            surface: card_surface(card.get("surface")),
            answer: card_str(card, "answer").map(str::to_string),
            answers: string_list(card.get("answers")),
            surface_data: card.get("surfaceData").filter(|data| data.is_object()).cloned(),
            answered: message.answered_at.is_some(),
        },
        Some(MESSAGE_CARD_KIND_PLAN) => SessionThreadRow::Plan {
            id: message.id.clone(),
            session_id: message.session_id.clone(),
            plan: js_string(card.get("plan")),
            questions: plan_questions(card),
            surface: card_surface(card.get("surface")),
            decision: card_str(card, "decision").map(str::to_string),
            fresh: card.get("fresh").and_then(Value::as_bool) == Some(true),
            answers: plan_answers(card.get("answers")),
            feedback: card_str(card, "feedback").filter(|text| !text.is_empty()).map(str::to_string),
            surface_data: card.get("surfaceData").filter(|data| data.is_object()).cloned(),
        },
        _ => return None,
    })
}

/// EXP-1280: a TOOL-posted card (`exponential_plan_propose` /
/// `exponential_sessions_ask`): no engine ask (`questionId`) behind it, so
/// it answers only through `sessions.answer`.
pub fn is_tool_card(message: &MessageRow) -> bool {
    message.card.as_ref().is_some_and(|card| {
        matches!(card_str(card, "kind"), Some(MESSAGE_CARD_KIND_QUESTION) | Some(MESSAGE_CARD_KIND_PLAN))
            && card_str(card, "questionId").is_none()
    })
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
        /// EXP-1323: the session that posted it (a child asking its parent).
        author_session_id: Option<String>,
    },
    /// A session's card (`id` = the message id of the `session` /
    /// `thread_started` card that announced it; EXP-1334: the dispatcher
    /// has no leading card of its own); opens that session.
    Session { id: String, card: SessionCard },
    /// EXP-1323: a child's end report (`Report from <name>` + the summary);
    /// opens `session_id`.
    Report {
        id: String,
        session_id: String,
        text: String,
        summary: String,
    },
    /// EXP-1323: ONE suggestions card (`sessions_suggest`); `session_id` =
    /// the posting session (Do it here messages it).
    Suggestions {
        id: String,
        session_id: String,
        header: String,
        start_all: bool,
        items: Vec<SuggestionRow>,
    },
    Question {
        id: String,
        /// EXP-1334: the session whose card it is (the root, or a child
        /// whose own question surfaces in the Conversation).
        session_id: String,
        question: String,
        options: Vec<QuestionOption>,
        /// EXP-1316: a free-text answer is offered (EXP-1280: unless the
        /// card says `custom: false`).
        custom: bool,
        /// EXP-1280: several picks allowed.
        multi: bool,
        /// EXP-1280: the inline Exponential UI surface, if any.
        surface: Option<CardSurface>,
        answer: Option<String>,
        /// EXP-1280: the picked labels (`None` on a legacy card = `answer` alone).
        answers: Option<Vec<String>>,
        /// EXP-1280: the surface's data model as the person answered it.
        surface_data: Option<Value>,
        answered: bool,
    },
    Plan {
        id: String,
        session_id: String,
        plan: String,
        /// EXP-1280: the choice questions, the inline surface.
        questions: Vec<PlanQuestion>,
        surface: Option<CardSurface>,
        decision: Option<String>,
        /// An approval that started in a fresh context (ask-dock.json).
        fresh: bool,
        /// EXP-1280: the picked labels per question, the note.
        answers: Option<BTreeMap<String, Vec<String>>>,
        feedback: Option<String>,
        /// The surface's data model as the person answered it.
        surface_data: Option<Value>,
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
    /// Thread 58cb32df (chats.json): a person's `/new` or `/compact` line,
    /// painted as the centred divider pill (`at` = the message's createdAt).
    Divider {
        id: String,
        kind: crate::chats::DividerKind,
        at: String,
    },
    /// Thread 58cb32df (chats.json): an agent's inline Exponential UI preview.
    Surface {
        id: String,
        session_id: String,
        title: Option<String>,
        surface: CardSurface,
    },
}

impl SessionThreadRow {
    /// The row's id (the message id it was built from).
    pub fn id(&self) -> &str {
        match self {
            Self::Message { id, .. }
            | Self::Session { id, .. }
            | Self::Report { id, .. }
            | Self::Suggestions { id, .. }
            | Self::Question { id, .. }
            | Self::Plan { id, .. }
            | Self::Subagent { id, .. }
            | Self::Picture { id, .. }
            | Self::Event { id, .. }
            | Self::Divider { id, .. }
            | Self::Surface { id, .. } => id,
        }
    }
}

/// EXP-1323: a suggested task's state: `open` (× · Options · Do it here ·
/// Start), `started` (the child's card once synced), `here` (sent to the
/// posting session), `filed` (the issue it became). Dismissed items drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionState {
    Open,
    Started,
    Here,
    Filed,
}

/// EXP-1323: one item of a suggestions row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionRow {
    pub id: String,
    pub title: String,
    pub text: String,
    pub state: SuggestionState,
    pub card: Option<SessionCard>,
    pub issue_id: Option<String>,
    pub identifier: Option<String>,
}

fn suggestions_row(
    message: &MessageRow,
    card: &Value,
    by_id: &HashMap<&str, &SessionRow>,
    titles: &HashMap<String, String>,
    issues: &HashMap<&str, &TreeIssue>,
    now: i64,
) -> Option<SessionThreadRow> {
    let Value::Array(raw_items) = card.get("items")? else {
        return None;
    };
    let mut items = Vec::new();
    for raw in raw_items {
        let Some(Value::String(id)) = raw.get("id") else {
            continue;
        };
        if !raw.is_object() || raw.get("dismissed") == Some(&Value::Bool(true)) {
            continue;
        }
        let session_id = card_str(raw, "sessionId");
        let issue_id = card_str(raw, "issueId");
        let target = session_id.and_then(|id| by_id.get(id));
        let state = if session_id.is_some() {
            SuggestionState::Started
        } else if raw.get("here") == Some(&Value::Bool(true)) {
            SuggestionState::Here
        } else if issue_id.is_some() {
            SuggestionState::Filed
        } else {
            SuggestionState::Open
        };
        items.push(SuggestionRow {
            id: id.clone(),
            title: js_string(raw.get("title")),
            text: js_string(raw.get("text")),
            state,
            card: target.map(|target| session_card(target, title_of(titles, &target.id), now)),
            issue_id: issue_id.map(str::to_string),
            identifier: issue_id.and_then(|id| issues.get(id)).map(|issue| issue.identifier.clone()),
        });
    }
    if items.is_empty() {
        return None;
    }
    let copy = &THREAD.copy.suggestions;
    let started = items.iter().filter(|item| item.state == SuggestionState::Started).count();
    let count = if items.len() == 1 {
        copy.one.clone()
    } else {
        copy.many.replacen("{count}", &items.len().to_string(), 1)
    };
    let header = if started > 0 {
        format!("{count} · {}", copy.started.replacen("{count}", &started.to_string(), 1))
    } else {
        count
    };
    Some(SessionThreadRow::Suggestions {
        id: message.id.clone(),
        session_id: message.session_id.clone(),
        header,
        start_all: items.iter().filter(|item| item.state == SuggestionState::Open).count() > 1,
        items,
    })
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
    /// EXP-1323: the live tail (Tasks card + status line) reads this
    /// session's relay state; `None` once the root ended.
    pub live_session_id: Option<String>,
    pub busy: bool,
    pub needs_you: bool,
    /// NEW-6 (N-1): a thread with ONE session (the root alone, no child yet)
    /// shows that session's raw work as the main thing: its id, and the
    /// strip carries no Conversation tab. `None` once a second session exists.
    pub main_session_id: Option<String>,
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
    session_name: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let event = js_string(card.get("event"));
    let mut text = THREAD.copy.events.get(&event)?.clone();
    if text.contains("{issue}") {
        let issue = card_str(card, "issueId").and_then(|id| issues.get(id))?;
        text = text.replacen("{issue}", &issue.identifier, 1);
    }
    if text.contains("{pr}") {
        let pr = card_str(card, "pullRequestId").and_then(|id| prs.get(id))?;
        let number = pr.number?;
        text = text.replacen("{pr}", &format!("#{number}"), 1);
    }
    if text.contains("{session}") {
        // NEW-1: a bus line about a session of the tree; an unsynced one drops.
        let name = card_str(card, "sessionId").and_then(&session_name)?;
        text = text.replacen("{session}", &name, 1);
    }
    if text.contains("{name}") {
        // NEW-2: a trigger line names its event (an event trigger) and
        // nothing else; a schedule firing has no name and draws no line (its
        // body speaks).
        let name = card_str(card, "name").filter(|name| !name.trim().is_empty())?;
        text = text.replacen("{name}", name, 1);
    }
    Some(text)
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
    ClosePr,
}

impl GuideMenuEntry {
    /// The fixture's entry id.
    pub fn id(self) -> &'static str {
        match self {
            Self::ClosePr => "close-pr",
        }
    }
}

/// EXP-1307: the Guide `…` entries for the Guide PR's `state` (None = no
/// PR): Close PR while the PR is open and the viewer is a member, else none
/// (EXP-1338: no GitHub entry, only the agent opens PRs).
pub fn guide_menu_entries(state: Option<&str>, member: bool) -> Vec<GuideMenuEntry> {
    match state {
        Some("open") if member => vec![GuideMenuEntry::ClosePr],
        _ => Vec::new(),
    }
}

/// EXP-1211: one open quick reply (`sessions.sendReply({messageId, itemId})`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerReply {
    pub id: String,
    pub text: String,
}

/// EXP-1211: the quick replies row above a session's composer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerReplies {
    pub message_id: String,
    pub items: Vec<ComposerReply>,
}

/// EXP-1211 (`session-thread.json` `repliesCases`, TS `composerReplies`):
/// the session's NEWEST `replies` card (created_at, then id) minus its sent,
/// id-less and blank items; `None` when nothing is left. A replies card is
/// never a Conversation row.
pub fn composer_replies(session_id: &str, messages: &[&MessageRow]) -> Option<ComposerReplies> {
    let mut newest: Option<&MessageRow> = None;
    for message in messages.iter().copied() {
        if message.session_id != session_id
            || message.card.as_ref().and_then(|card| card_str(card, "kind")) != Some(MESSAGE_CARD_KIND_REPLIES)
        {
            continue;
        }
        let newer = newest.is_none_or(|best| {
            let (at, best_at) = (ms_or_zero(message.created_at.as_deref()), ms_or_zero(best.created_at.as_deref()));
            at > best_at || (at == best_at && message.id > best.id)
        });
        if newer {
            newest = Some(message);
        }
    }
    let newest = newest?;
    let Some(Value::Array(raw_items)) = newest.card.as_ref().and_then(|card| card.get("items")) else {
        return None;
    };
    let items: Vec<ComposerReply> = raw_items
        .iter()
        .filter_map(|raw| {
            let id = raw.get("id")?.as_str()?;
            let text = raw.get("text")?.as_str()?;
            (raw.get("sent") != Some(&Value::Bool(true)) && !text.trim().is_empty())
                .then(|| ComposerReply { id: id.to_string(), text: text.to_string() })
        })
        .collect();
    (!items.is_empty()).then(|| ComposerReplies { message_id: newest.id.clone(), items })
}

// ── EXP-1334: the thread strip never scrolls ────────────────────────────────

/// Which chips get drawn, in order; the rest collapse into one "+N"
/// (EXP-288/326/343, the work tab strip's packer, shared with the thread
/// strip). Chips pack in order until the next would not fit; the overflow
/// chip's width is reserved only once something overflows; the ACTIVE
/// chip's width is committed first so it is always visible (EXP-343: the
/// old prefix-then-swap shape overflowed with a wide active tab); one chip
/// always survives. An `active_ix` past the chips counts as none.
pub fn partition_tabs(
    widths: &[f32],
    available: f32,
    gap: f32,
    overflow_w: f32,
    active_ix: Option<usize>,
) -> Vec<usize> {
    let count = widths.len();
    let active_ix = active_ix.filter(|ix| *ix < count);
    let available = available.max(0.);
    let total = widths.iter().sum::<f32>() + gap * count.saturating_sub(1) as f32;
    if total <= available {
        return (0..count).collect();
    }

    let budget = (available - overflow_w - gap).max(0.);
    let mut visible: Vec<usize> = Vec::new();
    let mut used = active_ix.map_or(0., |ix| widths[ix]);
    let mut chips = usize::from(active_ix.is_some());
    for (ix, width) in widths.iter().enumerate() {
        if Some(ix) == active_ix {
            visible.push(ix);
            continue;
        }
        let next = used + width + if chips > 0 { gap } else { 0. };
        if next > budget && chips > 0 {
            break;
        }
        visible.push(ix);
        used = next;
        chips += 1;
    }
    if let Some(active) = active_ix {
        if !visible.contains(&active) {
            visible.push(active);
        }
    }
    if visible.is_empty() && count > 0 {
        visible.push(0);
    }
    visible
}

/// [`partition_thread_strip`]'s tier: `Full` = identifier + title, `Short`
/// = the identifier alone, `Fold` = short and some chips behind a "+N".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StripTier {
    Full,
    Short,
    Fold,
}

/// One issue chip's two measured widths.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct StripIssueWidth {
    pub full: f32,
    pub short: f32,
}

/// [`partition_thread_strip`]'s answer: indexes of the drawn chips per zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThreadStripLayout {
    pub tier: StripTier,
    pub issues: Vec<usize>,
    pub sessions: Vec<usize>,
}

/// EXP-1334: the strip between Conversation and Guide (both pinned, never
/// scrolled) holds two zones, issues then sessions, one `gap` between them.
/// Tiers: everything full → every issue short → fold: the sessions keep
/// their OWN room (what the short issues leave, at least half), each zone
/// packs with [`partition_tabs`] and its own "+N" (`overflow_w`).
pub fn partition_thread_strip(
    issues: &[StripIssueWidth],
    sessions: &[f32],
    available: f32,
    gap: f32,
    overflow_w: f32,
    active_issue: Option<usize>,
    active_session: Option<usize>,
) -> ThreadStripLayout {
    let room = available.max(0.);
    let span = |widths: &[f32]| -> f32 {
        if widths.is_empty() {
            0.
        } else {
            widths.iter().sum::<f32>() + gap * (widths.len() - 1) as f32
        }
    };
    let sep = if !issues.is_empty() && !sessions.is_empty() { gap } else { 0. };
    let session_span = span(sessions);
    let full: Vec<f32> = issues.iter().map(|issue| issue.full).collect();
    let short: Vec<f32> = issues.iter().map(|issue| issue.short).collect();
    if span(&full) + sep + session_span <= room {
        return ThreadStripLayout {
            tier: StripTier::Full,
            issues: (0..issues.len()).collect(),
            sessions: (0..sessions.len()).collect(),
        };
    }
    if span(&short) + sep + session_span <= room {
        return ThreadStripLayout {
            tier: StripTier::Short,
            issues: (0..issues.len()).collect(),
            sessions: (0..sessions.len()).collect(),
        };
    }
    let zone = |widths: &[f32], budget: f32, active: Option<usize>| -> (Vec<usize>, f32) {
        let shown = partition_tabs(widths, budget, gap, overflow_w, active);
        let drawn: Vec<f32> = shown.iter().map(|&ix| widths[ix]).collect();
        let used = span(&drawn) + if shown.len() < widths.len() { gap + overflow_w } else { 0. };
        (shown, used)
    };
    let session_budget = if issues.is_empty() {
        room
    } else {
        session_span.min((room / 2.).max(room - span(&short) - sep))
    };
    let (session_shown, session_used) = zone(sessions, session_budget, active_session);
    let issue_budget = room - if sessions.is_empty() { 0. } else { session_used + sep };
    let (issue_shown, _) = zone(&short, issue_budget, active_issue);
    ThreadStripLayout {
        tier: StripTier::Fold,
        issues: issue_shown,
        sessions: session_shown,
    }
}

/// Depth-first walk of a forest given each node's FOLD parent (None = a
/// top-level node), children in the given order; depth relative to the top;
/// one item is placed once (a cycle's first item becomes a top).
fn fold_forest<'a, T: ?Sized>(
    items: &[&'a T],
    id: impl Fn(&'a T) -> &'a str,
    fold_parent: impl Fn(&'a T) -> Option<&'a str>,
) -> Vec<(&'a T, Vec<(&'a T, usize)>)> {
    let mut children: HashMap<Option<&'a str>, Vec<&'a T>> = HashMap::new();
    for item in items {
        children.entry(fold_parent(item)).or_default().push(item);
    }
    fn walk<'a, T: ?Sized>(
        parent: &'a str,
        depth: usize,
        children: &HashMap<Option<&'a str>, Vec<&'a T>>,
        id: &impl Fn(&'a T) -> &'a str,
        seen: &mut HashSet<&'a str>,
        nested: &mut Vec<(&'a T, usize)>,
    ) {
        for child in children.get(&Some(parent)).into_iter().flatten() {
            let child_id = id(child);
            if !seen.insert(child_id) {
                continue;
            }
            nested.push((*child, depth));
            walk(child_id, depth + 1, children, id, seen, nested);
        }
    }
    let mut seen: HashSet<&'a str> = HashSet::new();
    let fold = |top: &'a T, seen: &mut HashSet<&'a str>| {
        let mut nested = Vec::new();
        seen.insert(id(top));
        walk(id(top), 1, &children, &id, seen, &mut nested);
        (top, nested)
    };
    let mut out: Vec<(&'a T, Vec<(&'a T, usize)>)> = Vec::new();
    for top in children.get(&None).into_iter().flatten() {
        out.push(fold(top, &mut seen));
    }
    // A fold-parent cycle (a ↔ b) reaches no top: its first unreached item
    // (input order) becomes a top-level chip, the rest fold under it.
    for item in items {
        if !seen.contains(id(item)) {
            out.push(fold(item, &mut seen));
        }
    }
    out
}

/// EXP-1334: a linked issue's fold parent = its nearest LINKED ancestor (an
/// unlinked parent is skipped; a cycle or none = top-level).
fn issue_fold_parent<'a>(
    issue: &'a TreeIssue,
    linked: &HashSet<&str>,
    all: &HashMap<&str, &'a TreeIssue>,
) -> Option<&'a str> {
    let mut seen: HashSet<&str> = HashSet::from([issue.id.as_str()]);
    let mut parent = issue.parent_id.as_deref();
    while let Some(id) = parent {
        if seen.contains(id) {
            break;
        }
        if linked.contains(id) {
            return Some(id);
        }
        seen.insert(id);
        parent = all.get(id).and_then(|issue| issue.parent_id.as_deref());
    }
    None
}

/// The thread screen (EXP-1334): the header strip (`+` · Conversation · one
/// chip per TOP-LEVEL linked issue, its linked sub-issues folded under it ·
/// one chip per DIRECT child of the dispatcher, every deeper session folded
/// under its top-level ancestor · Guide), the dispatcher's conversation as
/// rows (no card of its own: its messages oldest first, a
/// `session`/`thread_started` card becoming that session's card, a child's
/// open question/plan card surfacing here) and the composer target (the
/// root). `focus` = the session open in place ([`SessionThreadView::focus_id`]);
/// the chips stay the dispatcher's.
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
    let mut by_id: HashMap<&str, &SessionRow> =
        input.sessions.iter().map(|session| (session.id.as_str(), *session)).collect();
    by_id.insert(root.id.as_str(), root);
    let everyone: Vec<&SessionRow> = by_id.values().copied().collect();
    let titles = session_titles(&everyone, input.session_issues, input.issues, input.messages);
    let issues = tree_issues(&tree, input.session_issues, input.issues);
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
    let title = title_of(&titles, &root.id).to_string();
    let focus = focus.and_then(|id| tree.iter().copied().find(|session| session.id == id));
    // EXP-1334: the conversation IS the dispatcher (no chip of its own); the
    // chips = its DIRECT children, every deeper session folded under its
    // top-level ancestor (the chip's dropdown).
    let name_of = |session: &SessionRow| {
        session_card(session, title_of(&titles, &session.id), now).name
    };
    let tree_ids: HashSet<&str> = tree.iter().map(|session| session.id.as_str()).collect();
    // S-3: only the VISIBLE sessions get chips ([`visible_tree`]), the
    // focused one always; the rest stay reachable through their cards.
    let below: Vec<&SessionRow> = tree.iter().copied().filter(|session| session.id != root.id).collect();
    let shown = visible_tree(&below, Some(now));
    let children: Vec<&SessionRow> = below
        .iter()
        .copied()
        .filter(|session| shown.contains(session.id.as_str()) || focus.is_some_and(|focus| focus.id == session.id))
        .collect();
    let session_tabs: Vec<ThreadTab> = fold_forest(
        &children,
        |session| session.id.as_str(),
        |session| match session.parent_id.as_deref() {
            Some(parent) if parent != root.id && tree_ids.contains(parent) => Some(parent),
            _ => None,
        },
    )
    .into_iter()
    .map(|(top, nested)| ThreadTab::Session {
        session_id: top.id.clone(),
        name: name_of(top),
        mark_state: session_tab_mark_state(top),
        nested: nested
            .into_iter()
            .map(|(item, depth)| ThreadTabNestedSession {
                session_id: item.id.clone(),
                name: name_of(item),
                mark_state: session_tab_mark_state(item),
                depth,
            })
            .collect(),
    })
    .collect();
    let linked_ids: HashSet<&str> = issues.iter().map(|issue| issue.id.as_str()).collect();
    let issue_refs: Vec<&TreeIssue> = issues.iter().collect();
    let issue_tabs: Vec<ThreadTab> = fold_forest(
        &issue_refs,
        |issue| issue.id.as_str(),
        |issue| issue_fold_parent(issue, &linked_ids, &issue_map),
    )
    .into_iter()
    .map(|(top, nested)| ThreadTab::Issue {
        issue_id: top.id.clone(),
        identifier: top.identifier.clone(),
        nested: nested
            .into_iter()
            .map(|(item, depth)| ThreadTabNestedIssue {
                issue_id: item.id.clone(),
                identifier: item.identifier.clone(),
                depth,
            })
            .collect(),
    })
    .collect();

    // EXP-1334: no root card; a CHILD's own question / plan card surfaces in
    // the Conversation (answered here, straight back to that child).
    let mut conversation: Vec<&MessageRow> = own.clone();
    conversation.extend(input.messages.iter().copied().filter(|message| {
        message.session_id != root.id
            && tree_ids.contains(message.session_id.as_str())
            && message.card.as_ref().is_some_and(|card| {
                matches!(card_str(card, "kind"), Some(MESSAGE_CARD_KIND_QUESTION) | Some(MESSAGE_CARD_KIND_PLAN))
            })
    }));
    conversation.sort_by(|a, b| {
        ms_or_zero(a.created_at.as_deref())
            .cmp(&ms_or_zero(b.created_at.as_deref()))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut rows: Vec<SessionThreadRow> = Vec::new();
    let messages_by_id: HashMap<&str, &MessageRow> =
        input.messages.iter().map(|message| (message.id.as_str(), *message)).collect();
    let mut escalated: HashSet<&str> = HashSet::new();
    for message in conversation {
        let Some(card) = message.card.as_ref() else {
            let author = match message.author_kind.as_deref() {
                Some(MESSAGE_AUTHOR_KIND_USER) => MESSAGE_AUTHOR_KIND_USER,
                Some(MESSAGE_AUTHOR_KIND_AGENT) => MESSAGE_AUTHOR_KIND_AGENT,
                _ => crate::contract::MESSAGE_AUTHOR_KIND_SYSTEM,
            };
            if message.body_text().trim().is_empty() {
                continue;
            }
            let divider = (author == MESSAGE_AUTHOR_KIND_USER)
                .then(|| crate::chats::command_divider(message.body_text()))
                .flatten();
            if let Some(kind) = divider {
                rows.push(SessionThreadRow::Divider {
                    id: message.id.clone(),
                    kind,
                    at: message.created_at.clone().unwrap_or_default(),
                });
                continue;
            }
            rows.push(SessionThreadRow::Message {
                id: message.id.clone(),
                author: author.to_string(),
                author_id: message.author_id.clone(),
                body: message.body_text().to_string(),
                queued: author == MESSAGE_AUTHOR_KIND_USER
                    && message.consumed_at.as_deref().is_none_or(str::is_empty),
                author_session_id: message.author_session_id.clone(),
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
                    card: session_card(target, title_of(&titles, &target.id), now),
                });
            }
            Some(MESSAGE_CARD_KIND_QUESTION) | Some(MESSAGE_CARD_KIND_PLAN) => {
                rows.extend(ask_card_row(message));
            }
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
            Some(MESSAGE_CARD_KIND_REPORT) => {
                let Some(target) = card_str(card, "sessionId").and_then(|id| by_id.get(id)) else {
                    continue;
                };
                let name = session_card(target, title_of(&titles, &target.id), now).name;
                rows.push(SessionThreadRow::Report {
                    id: message.id.clone(),
                    session_id: target.id.clone(),
                    text: THREAD.copy.report.replacen("{session}", &name, 1),
                    summary: card_str(card, "summary")
                        .map_or_else(|| message.body_text().to_string(), str::to_string),
                });
            }
            Some(MESSAGE_CARD_KIND_SUGGESTIONS) => {
                if let Some(row) = suggestions_row(message, card, &by_id, &titles, &issue_map, now) {
                    rows.push(row);
                }
            }
            Some(MESSAGE_CARD_KIND_EVENT) => {
                let session_name = |id: &str| {
                    by_id
                        .get(id)
                        .map(|target| session_card(target, title_of(&titles, &target.id), now).name)
                };
                if let Some(text) = event_text(card, &issue_map, &pr_map, session_name) {
                    rows.push(SessionThreadRow::Event {
                        id: message.id.clone(),
                        event: js_string(card.get("event")),
                        text,
                    });
                }
            }
            // NEW-1: a child's ask mirrored into its parent: the child's own
            // card already surfaces in the Conversation, so it draws nothing.
            // #EXP-1399: a mirror from ANOTHER tree (an agent's conversation
            // watching a thread, `escalate`) = the origin's session card +
            // THE origin card, read from its message (first mirror wins).
            Some(MESSAGE_CARD_KIND_ASK) => {
                let Some(target) = card_str(card, "sessionId").and_then(|id| by_id.get(id)) else {
                    continue;
                };
                let Some(origin) = card_str(card, "messageId").and_then(|id| messages_by_id.get(id)) else {
                    continue;
                };
                if tree_ids.contains(target.id.as_str()) || origin.session_id != target.id {
                    continue;
                }
                // #EXP-1399 review: only the origin session's own mirror draws it.
                if message.author_session_id.as_deref() != Some(target.id.as_str()) {
                    continue;
                }
                let Some(origin_row) = ask_card_row(origin) else {
                    continue;
                };
                if !escalated.insert(origin.id.as_str()) {
                    continue;
                }
                rows.push(SessionThreadRow::Session {
                    id: message.id.clone(),
                    card: session_card(target, title_of(&titles, &target.id), now),
                });
                rows.push(origin_row);
            }
            Some(MESSAGE_CARD_KIND_SURFACE) => rows.extend(surface_card_row(message)),
            _ => {}
        }
    }

    let live: Vec<&SessionRow> = tree.iter().copied().filter(|session| !session.is_ended()).collect();
    // NEW-6 (N-1): the root alone = the thread IS that session.
    // NEW-6 (N-1): the root alone = the thread IS that session (the
    // Conversation tab opens its raw work, `main_session_id`); NEW-46: the
    // tab itself is always in the strip.
    let single = tree.iter().all(|session| session.id == root.id);
    let mut tabs = vec![ThreadTab::Add, ThreadTab::Conversation];
    tabs.extend(issue_tabs);
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
        live_session_id: (!root.is_ended()).then(|| root.id.clone()),
        busy: live.iter().any(|session| session.busy),
        needs_you: live.iter().any(|session| session.needs_input),
        main_session_id: single.then(|| root.id.clone()),
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
    /// EXP-1323: 0 = the thread (opens the thread), >0 = a nested child
    /// (opens that session); `parent_id` `None` on a root.
    #[serde(default)]
    pub depth: usize,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub root_id: String,
    pub title: String,
    /// EXP-1318: the ROOT session's logical `device_id` (the trailing
    /// device icon); `None` when unset.
    #[serde(default)]
    pub device_id: Option<String>,
    pub busy: bool,
    pub needs_you: bool,
    /// EXP-1323: bold title (a root: the tree's newest agent message is
    /// newer than the viewer's read of the root; a child: its own).
    #[serde(default)]
    pub unread: bool,
    /// The tree's newest `updated_at` (the sort key), verbatim; a child's
    /// own `updated_at`.
    pub activity_at: String,
    /// S-3: set ONLY on a tree's fold row (`<rootId>:earlier`, depth 1, the
    /// muted list-item row; opens the thread) = how many sub-sessions it
    /// hides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earlier: Option<usize>,
}

/// S-3: the fold row's id suffix ([`ThreadsListRow::earlier`]).
pub const EARLIER_SUFFIX: &str = ":earlier";

/// S-3: the fold row's label (`copy.earlier`, `{n}` slot).
pub fn earlier_label(count: usize) -> String {
    let copy = &THREADS_LIST.copy.earlier;
    (if count == 1 { &copy.one } else { &copy.other }).replace("{n}", &count.to_string())
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
    /// EXP-1323: the viewer's `session_reads` (empty = none read).
    pub reads: &'a [&'a SessionReadRow],
    /// NEW-1: the clock for Recent's window (epoch ms; `None` = no window:
    /// the last `recent.max` only).
    pub now_ms: Option<i64>,
}

/// NEW-1 / S-3: a tree's derived state = the session's four
/// ([`SessionState`]).
pub type TreeState = SessionState;

/// NEW-1 / S-3 (TS `treeState`): `needs_you` = a live session waits on a
/// person (`needs_input`, a usage wall or a watchdog flag included);
/// `working` = any live session queued or busy; `done` = every session
/// ended, or every PR of the thread merged (none = never done this way);
/// else `idle`. `now_ms` is kept for the signature (no stale state).
pub fn tree_state(tree: &[&SessionRow], pull_requests: &[&PullRequestRow], _now_ms: i64) -> TreeState {
    let live: Vec<&SessionRow> = tree.iter().copied().filter(|session| !session.is_ended()).collect();
    if live.iter().any(|session| session.needs_input) {
        return SessionState::NeedsYou;
    }
    if live.iter().any(|session| status_is(session, SESSION_STATUS_QUEUED) || session.busy) {
        return SessionState::Working;
    }
    if live.is_empty() {
        return SessionState::Done;
    }
    if !pull_requests.is_empty() && pull_requests.iter().all(|pr| pr.state.as_deref() == Some(PR_STATE_MERGED)) {
        return SessionState::Done;
    }
    SessionState::Idle
}

/// The Threads page: roots of kind `thread` in scope grouped Needs you (a
/// live session of the tree waits on a person) · Running (any session
/// queued or running) · Recent (the rest), each by the tree's newest
/// `updated_at` desc (ties by id). Empty groups are omitted.
pub fn partition_threads(input: ThreadsListInput<'_>) -> Vec<ThreadsListGroup> {
    let read_at: HashMap<&str, i64> = input
        .reads
        .iter()
        .map(|read| (read.session_id.as_str(), ms_or_zero(read.read_at.as_deref())))
        .collect();
    let root_of: HashMap<&str, &str> =
        input.sessions.iter().map(|session| (session.id.as_str(), session.root_id_or_self())).collect();
    let mut newest_agent: HashMap<&str, i64> = HashMap::new();
    let mut newest_agent_in_tree: HashMap<&str, i64> = HashMap::new();
    for message in input.messages {
        if message.author_kind.as_deref() != Some(MESSAGE_AUTHOR_KIND_AGENT) {
            continue;
        }
        let at = ms_or_zero(message.created_at.as_deref());
        let own = newest_agent.entry(message.session_id.as_str()).or_insert(at);
        *own = (*own).max(at);
        if let Some(root) = root_of.get(message.session_id.as_str()) {
            let tree = newest_agent_in_tree.entry(root).or_insert(at);
            *tree = (*tree).max(at);
        }
    }
    let unread_at = |newest: Option<&i64>, session_id: &str| {
        newest.is_some_and(|newest| *newest > read_at.get(session_id).copied().unwrap_or(-1))
    };
    let mut by_root: HashMap<&str, Vec<&SessionRow>> = HashMap::new();
    for session in input.sessions {
        by_root.entry(session.root_id_or_self()).or_default().push(session);
    }
    let mut buckets: HashMap<ThreadsListKey, Vec<(ThreadsListRow, Vec<ThreadsListRow>)>> = HashMap::new();
    for root in input.sessions.iter().copied().filter(|session| {
        is_root(session)
            // NEW-2: an agent's conversation (a root with `agent_id`) is no
            // thread; VAPP-110: neither is an app's editor session (`app_id`).
            && !session.is_agent()
            && !session.is_app_editor()
            && input
                .team_id
                .is_none_or(|team| session.team_id.as_deref() == Some(team))
    }) {
        let alone = [root];
        let tree: &[&SessionRow] = by_root.get(root.id.as_str()).map_or(&alone[..], Vec::as_slice);
        let live: Vec<&SessionRow> = tree.iter().copied().filter(|session| !session.is_ended()).collect();
        let needs_you = live.iter().any(|session| session.needs_input);
        let running = live.iter().any(|session| status_is(session, SESSION_STATUS_QUEUED) || session.busy);
        let mut activity = root.updated_at.clone().unwrap_or_default();
        for session in tree {
            if ms_or_zero(session.updated_at.as_deref()) > ms_or_zero(Some(&activity)) {
                activity = session.updated_at.clone().unwrap_or_default();
            }
        }
        let title = session_title(root, input.session_issues, input.issues, input.messages);
        let key = if needs_you {
            ThreadsListKey::NeedsYou
        } else if running {
            ThreadsListKey::Running
        } else {
            ThreadsListKey::Recent
        };
        let row = ThreadsListRow {
            session_id: root.id.clone(),
            team_id: root.team_id.clone().unwrap_or_default(),
            depth: 0,
            parent_id: None,
            root_id: root.id.clone(),
            title,
            device_id: root.device_id.clone(),
            busy: live.iter().any(|session| session.busy),
            needs_you,
            unread: unread_at(newest_agent_in_tree.get(root.id.as_str()), &root.id),
            activity_at: activity,
            earlier: None,
        };
        // A live tree nests its VISIBLE children (S-3 [`visible_tree`]:
        // working, needs you or active within `recentChildMinutes`;
        // depth-first, creation order), the rest fold into ONE `earlier`
        // row; Recent collapses to the root.
        let mut children = Vec::new();
        if key != ThreadsListKey::Recent {
            let ordered = tree_order(tree);
            let below: Vec<&SessionRow> =
                ordered.iter().copied().filter(|session| session.id != root.id).collect();
            let shown = visible_tree(&below, input.now_ms);
            fn walk<'s>(
                parent_id: &str,
                depth: usize,
                ordered: &[&'s SessionRow],
                shown: &HashSet<&str>,
                visit: &mut dyn FnMut(&'s SessionRow, &str, usize),
            ) {
                for child in ordered.iter().copied() {
                    if child.parent_id.as_deref() != Some(parent_id) || !shown.contains(child.id.as_str()) {
                        continue;
                    }
                    visit(child, parent_id, depth);
                    walk(&child.id, depth + 1, ordered, shown, visit);
                }
            }
            walk(&root.id, 1, &ordered, &shown, &mut |child, parent_id, depth| {
                let card = session_card(
                    child,
                    &session_title(child, input.session_issues, input.issues, input.messages),
                    0,
                );
                children.push(ThreadsListRow {
                    session_id: child.id.clone(),
                    team_id: child.team_id.clone().unwrap_or_default(),
                    depth,
                    parent_id: Some(parent_id.to_string()),
                    root_id: root.id.clone(),
                    title: card.name,
                    device_id: child.device_id.clone(),
                    busy: !child.is_ended() && child.busy,
                    needs_you: !child.is_ended() && child.needs_input,
                    unread: unread_at(newest_agent.get(child.id.as_str()), &child.id),
                    activity_at: child.updated_at.clone().unwrap_or_default(),
                    earlier: None,
                });
            });
            let hidden: Vec<&SessionRow> =
                below.iter().copied().filter(|session| !shown.contains(session.id.as_str())).collect();
            if let Some(first) = hidden.first() {
                let mut newest = first.updated_at.clone().unwrap_or_default();
                for session in &hidden {
                    if ms_or_zero(session.updated_at.as_deref()) > ms_or_zero(Some(&newest)) {
                        newest = session.updated_at.clone().unwrap_or_default();
                    }
                }
                children.push(ThreadsListRow {
                    session_id: format!("{}{EARLIER_SUFFIX}", root.id),
                    team_id: root.team_id.clone().unwrap_or_default(),
                    depth: 1,
                    parent_id: Some(root.id.clone()),
                    root_id: root.id.clone(),
                    title: earlier_label(hidden.len()),
                    device_id: None,
                    busy: false,
                    needs_you: false,
                    unread: false,
                    activity_at: newest,
                    earlier: Some(hidden.len()),
                });
            }
        }
        buckets.entry(key).or_default().push((row, children));
    }
    ThreadsListKey::ALL
        .into_iter()
        .filter_map(|key| {
            let mut entries = buckets.remove(&key)?;
            entries.sort_by(|(a, _), (b, _)| {
                ms_or_zero(Some(&b.activity_at))
                    .cmp(&ms_or_zero(Some(&a.activity_at)))
                    .then_with(|| a.session_id.cmp(&b.session_id))
            });
            if key == ThreadsListKey::Recent {
                // NEW-1: every tree active in the last `recent.hours`, filled
                // up to the last `recent.max` by activity.
                let rules = THREADS_LIST.recent;
                let window = rules.hours * 60 * 60 * 1000;
                entries = entries
                    .into_iter()
                    .enumerate()
                    .filter(|(index, (row, _))| {
                        *index < rules.max
                            || input
                                .now_ms
                                .is_some_and(|at| at - ms_or_zero(Some(&row.activity_at)) <= window)
                    })
                    .map(|(_, entry)| entry)
                    .collect();
            }
            let rows: Vec<ThreadsListRow> = entries
                .into_iter()
                .flat_map(|(row, children)| std::iter::once(row).chain(children))
                .collect();
            Some(ThreadsListGroup {
                key,
                title: key.title().to_string(),
                rows,
            })
        })
        .filter(|group| !group.rows.is_empty())
        .collect()
}

/// EXP-1323: the Threads nav dot = any thread (depth 0) row is unread
/// (`threads-list.json` `unreadCases`).
pub fn threads_unread(groups: &[ThreadsListGroup]) -> bool {
    groups.iter().any(|group| group.rows.iter().any(|row| row.depth == 0 && row.unread))
}

/// EXP-1309/1318: the ONE quiet trailing word of a thread row ×4 (EXP-1339:
/// no lock; `Merged` has no text, it only turns the lead violet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

    /// The copy (`ready to merge`, …); `None` for `Merged` (EXP-1339: the
    /// violet pr-merged lead says it, no trailing text).
    pub fn text(self) -> Option<&'static str> {
        let words = &THREADS_LIST.copy.words;
        match self {
            Self::Stack => Some(&words.stack),
            Self::ReadyToMerge => Some(&words.ready_to_merge),
            Self::Merged => None,
            Self::Question => Some(&words.question),
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRow {
    /// NEW-2: the `agents` definition row.
    pub agent_id: String,
    pub team_id: String,
    pub name: String,
    /// Icon concept (default `agentDefaults.icon`).
    pub icon: String,
    /// `#rrggbb` (default `agentDefaults.color`).
    pub color: String,
    /// From the VIEWER's own conversation session with the agent.
    pub busy: bool,
    pub needs_you: bool,
}

/// `localeCompare`-ish: case-insensitive first, then exact.
fn locale_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
}

/// NEW-2: the rail's Agents section: one row per `agents` definition of the
/// team (`None` = every team) by `sort_order`, then name, then id; missing
/// icon/color fall back to the fixture defaults. The spinner / needs-you
/// dot read the VIEWER's own conversation session with the agent (a root
/// with `agent_id` and `user_id == me`); a member without one sees a quiet
/// row.
pub fn list_agents(
    agents: &[&AgentDefRow],
    sessions: &[&SessionRow],
    team_id: Option<&str>,
    me: Option<&str>,
) -> Vec<AgentRow> {
    let mut mine: HashMap<&str, &SessionRow> = HashMap::new();
    for session in sessions {
        if let Some(agent_id) = session.agent_id.as_deref().filter(|id| !id.is_empty()) {
            if is_root(session) && me.is_some() && session.user_id.as_deref() == me {
                mine.insert(agent_id, session);
            }
        }
    }
    let mut agents: Vec<&AgentDefRow> = agents
        .iter()
        .copied()
        .filter(|agent| team_id.is_none_or(|team| agent.team_id.as_deref() == Some(team)))
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
        .map(|agent| {
            let conversation = mine.get(agent.id.as_str());
            let live = conversation.is_some_and(|session| !session.is_ended());
            AgentRow {
                agent_id: agent.id.clone(),
                team_id: agent.team_id.clone().unwrap_or_default(),
                name: agent.name.clone().unwrap_or_default(),
                icon: agent.icon.clone().unwrap_or_else(|| defaults.icon.clone()),
                color: agent.color.clone().unwrap_or_else(|| defaults.color.clone()),
                busy: live && conversation.is_some_and(|session| session.busy),
                needs_you: live && conversation.is_some_and(|session| session.needs_input),
            }
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
            let mut card = json!({ "options": options });
            if let Some(descriptions) = descriptions {
                card["optionDescriptions"] = descriptions;
            }
            question_options(&card)
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
        assert_eq!(question_options(&json!({ "optionDescriptions": ["x"] })), Vec::<QuestionOption>::new());
        assert_eq!(question_options(&json!({ "options": "Yes" })), Vec::<QuestionOption>::new());
        // EXP-1280: the picture sidecar resolves through `pictures`.
        let card = json!({
            "options": ["A", "B"],
            "optionPictures": ["a", null],
            "pictures": [{ "label": "a", "attachmentId": "att-1" }]
        });
        let ids: Vec<Option<String>> = question_options(&card).into_iter().map(|o| o.attachment_id).collect();
        assert_eq!(ids, [Some("att-1".to_string()), None]);
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

    /// NEW-8: a fixture `TreeMessage` is already KEYED (its `sessionId` is
    /// the session its conversation belongs to): it hydrates with a
    /// synthesized conversation and the resolved session set after decode
    /// (the row's own `session_id` is never decoded).
    fn messages(value: &Value) -> Vec<MessageRow> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let session_id = item["sessionId"].as_str().unwrap_or_default().to_string();
                        let mut item = item.clone();
                        if let Some(object) = item.as_object_mut() {
                            object.entry("conversationId").or_insert_with(|| json!(format!("c-{session_id}")));
                        }
                        let mut message: MessageRow = row(&item, &format!("row-{index}"));
                        message.session_id = session_id;
                        message
                    })
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

    /// NEW-2: a `trigger_fired` line shows for an EVENT firing ({name} =
    /// the event name); a schedule firing (no name) draws no line.
    #[test]
    fn a_trigger_line_names_its_event_and_a_schedule_draws_none() {
        let issues = HashMap::new();
        let prs = HashMap::new();
        let line = |card: Value| event_text(&card, &issues, &prs, |_| None);
        assert_eq!(
            line(serde_json::json!({"kind": "event", "event": "trigger_fired", "name": "issue.created", "agentId": "a", "triggerId": "t"}))
                .as_deref(),
            Some("Trigger fired: issue.created")
        );
        for name in [Value::Null, serde_json::json!("  ")] {
            assert_eq!(
                line(serde_json::json!({"kind": "event", "event": "trigger_fired", "name": name, "agentId": "a", "triggerId": "t"})),
                None
            );
        }
        assert_eq!(line(serde_json::json!({"kind": "event", "event": "trigger_fired"})), None);
    }

    /// NEW-1 (TS `sessionKindWord` / web `blockedBadgeLabel`): a blocked
    /// session's word names its kind; an unknown kind or none reads
    /// `Usage limit`; a watchdog flag has its word as the badge, a usage wall
    /// none; only a usage wall offers Switch account.
    #[test]
    fn a_blocked_session_names_its_kind() {
        let row = |blocked: serde_json::Value| -> SessionRow {
            serde_json::from_value(serde_json::json!({"id": "s", "status": "running", "blocked": blocked})).unwrap()
        };
        for (kind, word) in [
            ("rate_limit", "Usage limit"),
            ("unclaimed", "Not picked up"),
            ("stuck", "Stuck"),
        ] {
            let session: SessionRow = serde_json::from_value(
                serde_json::json!({"id": "s", "status": "running", "needs_input": true, "blocked": {"kind": kind}}),
            )
            .unwrap();
            assert_eq!(session_state(&session), SessionState::NeedsYou, "{kind}");
            assert_eq!(session_kind_word(&session, SessionState::NeedsYou), word, "{kind}");
            assert_eq!(session_card(&session, "T", 0).kind_word, word, "{kind}");
            assert_eq!(SESSION_CARD.copy.blocked_kinds.get(kind).map(String::as_str), Some(word));
            if kind == "rate_limit" {
                assert_eq!(watchdog_blocked_word(&session), None);
                assert!(is_usage_wall(&session));
            } else {
                assert_eq!(watchdog_blocked_word(&session).as_deref(), Some(word), "{kind}");
                assert!(!is_usage_wall(&session), "{kind}");
            }
        }
        // S-3: an interruption is invisible: an idle session reads Idle.
        for kind in ["device_offline", "restarted"] {
            let session = row(serde_json::json!({"kind": kind, "resume": true}));
            assert_eq!(session_state(&session), SessionState::Idle, "{kind}");
            assert_eq!(session_card(&session, "T", 0).kind_word, "Idle", "{kind}");
            assert!(!is_usage_wall(&session), "{kind}");
        }
        let future = row(serde_json::json!({"kind": "future_kind"}));
        assert_eq!(session_kind_word(&future, SessionState::NeedsYou), "Needs you");
        assert_eq!(watchdog_blocked_word(&future), None);
        let legacy = row(serde_json::json!({"window": "weekly"}));
        assert_eq!(session_kind_word(&legacy, SessionState::NeedsYou), "Needs you");
        assert!(is_usage_wall(&legacy));
        // Not blocked: the plain state word, no wall.
        let free: SessionRow = serde_json::from_value(serde_json::json!({"id": "s", "status": "running"})).unwrap();
        assert_eq!(session_kind_word(&free, session_state(&free)), session_state(&free).word());
        assert!(!is_usage_wall(&free));
    }

    #[test]
    fn the_copy_comes_from_the_fixtures() {
        assert_eq!(SessionState::NeedsYou.word(), "Needs you");
        assert_eq!(SessionState::Working.icon(), "coding-running");
        assert_eq!(SessionState::NeedsYou.tone(), "amber");
        assert_eq!(THREAD.copy.placeholder, "Type / for commands");
        assert_eq!(THREADS_LIST.copy.title, "Chats");
        assert_eq!(ThreadsListKey::Recent.title(), "Recent");
        assert_eq!(THREADS_LIST.agent_defaults.icon, "bot");
        assert_eq!(THREADS_LIST.recent, RecentRules { hours: 24, max: 20 });
        assert_eq!(THREADS_LIST.recent_child_minutes, 120);
        assert_eq!(earlier_label(1), "1 earlier sub-session");
        assert_eq!(earlier_label(3), "3 earlier sub-sessions");
        assert_eq!(SESSION_CARD.copy.blocked_kinds.get("rate_limit").map(String::as_str), Some("Usage limit"));
        // S-3: device_offline / restarted are invisible (no word).
        for kind in crate::contract::CODING_SESSION_BLOCKED_KINDS {
            let invisible = matches!(*kind, "device_offline" | "restarted");
            assert_eq!(SESSION_CARD.copy.blocked_kinds.contains_key(*kind), !invisible, "{kind}");
        }
    }

    /// The fixture's `copy.words` key set IS the struct's: an extra key
    /// (one re-added to the fixture) fails to parse, as on iOS/Android.
    #[test]
    fn thread_row_words_refuse_unknown_keys() {
        let mut words = fixture(THREADS_LIST_JSON)["copy"]["words"].clone();
        assert!(serde_json::from_value::<ThreadRowWords>(words.clone()).is_ok());
        words["merged"] = Value::String("merged".into());
        assert!(serde_json::from_value::<ThreadRowWords>(words).is_err());
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
            let title = input["title"].as_str().unwrap();
            assert_whole(session_card(&session, title, now(&input["now"])), &case["expected"], name);
        }
    }

    /// EXP-1374: `session-card.json` `bannerCases`, whole objects.
    #[test]
    fn session_banner_matches_the_fixture() {
        let fixture = fixture(SESSION_CARD_JSON);
        let cases = fixture["bannerCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let session: SessionRow = row(&case["session"], "s");
            assert_whole(session_banner(&session), &case["expected"], name);
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
            let messages: Vec<MessageRow> = messages(&input["messages"]);
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
            let messages: Vec<MessageRow> = messages(&input["messages"]);
            let reads: Vec<SessionReadRow> = rows(&input["reads"]);
            let sessions: Vec<&SessionRow> = sessions.iter().collect();
            let links: Vec<&SessionIssueRow> = links.iter().collect();
            let messages: Vec<&MessageRow> = messages.iter().collect();
            let reads: Vec<&SessionReadRow> = reads.iter().collect();
            let groups = partition_threads(ThreadsListInput {
                sessions: &sessions,
                session_issues: &links,
                issues: &issues,
                messages: &messages,
                team_id: input["teamId"].as_str(),
                reads: &reads,
                now_ms: input["now"].as_str().and_then(parse_instant),
            });
            assert_whole(&groups, &case["expected"], name);
        }
        let recent_cases = fixture["recentCases"].as_array().unwrap();
        assert!(!recent_cases.is_empty());
        for case in recent_cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let sessions: Vec<SessionRow> = rows(&input["sessions"]);
            let links: Vec<SessionIssueRow> = rows(&input["sessionIssues"]);
            let issues: Vec<TreeIssue> =
                serde_json::from_value(input["issues"].clone()).unwrap_or_default();
            let messages: Vec<MessageRow> = messages(&input["messages"]);
            let reads: Vec<SessionReadRow> = rows(&input["reads"]);
            let sessions: Vec<&SessionRow> = sessions.iter().collect();
            let links: Vec<&SessionIssueRow> = links.iter().collect();
            let messages: Vec<&MessageRow> = messages.iter().collect();
            let reads: Vec<&SessionReadRow> = reads.iter().collect();
            let groups = partition_threads(ThreadsListInput {
                sessions: &sessions,
                session_issues: &links,
                issues: &issues,
                messages: &messages,
                team_id: input["teamId"].as_str(),
                reads: &reads,
                now_ms: input["now"].as_str().and_then(parse_instant),
            });
            let recent: Vec<&str> = groups
                .iter()
                .find(|group| group.key == ThreadsListKey::Recent)
                .map(|group| {
                    group
                        .rows
                        .iter()
                        .filter(|row| row.depth == 0)
                        .map(|row| row.session_id.as_str())
                        .collect()
                })
                .unwrap_or_default();
            let expected: Vec<&str> =
                case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(recent, expected, "{name}");
        }
        let state_cases = fixture["treeStateCases"].as_array().unwrap();
        assert!(!state_cases.is_empty());
        for case in state_cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let tree: Vec<SessionRow> = rows(&input["tree"]);
            let prs: Vec<PullRequestRow> = rows(&input["pullRequests"]);
            let tree: Vec<&SessionRow> = tree.iter().collect();
            let prs: Vec<&PullRequestRow> = prs.iter().collect();
            assert_eq!(
                tree_state(&tree, &prs, now(&input["now"])).as_str(),
                case["expected"].as_str().unwrap(),
                "{name}"
            );
        }
        let unread_cases = fixture["unreadCases"].as_array().unwrap();
        assert!(!unread_cases.is_empty());
        for case in unread_cases {
            let groups: Vec<ThreadsListGroup> = serde_json::from_value(case["groups"].clone()).unwrap();
            assert_eq!(threads_unread(&groups), case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
        let agent_cases = fixture["agentCases"].as_array().unwrap();
        assert!(!agent_cases.is_empty());
        for case in agent_cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let agents: Vec<AgentDefRow> = rows(&input["agents"]);
            let agents: Vec<&AgentDefRow> = agents.iter().collect();
            let sessions: Vec<SessionRow> = rows(&input["sessions"]);
            let sessions: Vec<&SessionRow> = sessions.iter().collect();
            assert_whole(
                list_agents(&agents, &sessions, input["teamId"].as_str(), input["me"].as_str()),
                &case["expected"],
                name,
            );
        }
        let mark_cases = fixture["markCases"].as_array().unwrap();
        assert!(!mark_cases.is_empty());
        for case in mark_cases {
            let input = &case["input"];
            let state = thread_row_mark_state(input["needsYou"].as_bool().unwrap(), input["busy"].as_bool().unwrap());
            assert_eq!(Some(thread_row_mark_word(state)), case["expected"].as_str(), "{}", case["name"]);
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
        // Release train 2026-10-23 (NEW-41): `leadCases`, the row lead's PR
        // glyph. The IDE leads with the root's Guide PR
        // (`ui::threads_view::thread_row_lead`): merged → violet, else open
        // emerald, none → no glyph.
        let lead_cases = fixture["leadCases"].as_array().unwrap();
        assert!(!lead_cases.is_empty());
        for case in lead_cases {
            let states = case["prStates"].as_array().unwrap();
            let pulls: Vec<PullRequestRow> =
                rows(&Value::Array(states.iter().map(|state| json!({ "state": state })).collect()));
            let lead = guide_pull_request(&pulls).map(|pull| if pull.is_merged() { "merged" } else { "open" });
            assert_eq!(lead, case["expected"].as_str(), "{}", case["name"]);
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

    /// `session-thread.json` `guidePullRequestCases`.
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
    }

    /// EXP-1211: `session-thread.json` `repliesCases`, whole values.
    #[test]
    fn composer_replies_match_the_fixture() {
        let fixture = fixture(SESSION_THREAD_JSON);
        let cases = fixture["repliesCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let messages: Vec<MessageRow> = messages(&input["messages"]);
            let messages: Vec<&MessageRow> = messages.iter().collect();
            let replies = composer_replies(input["sessionId"].as_str().unwrap(), &messages);
            assert_whole(&replies, &case["expected"], case["name"].as_str().unwrap());
        }
        assert_eq!(THREAD.copy.replies.label, "Quick replies");
        assert_eq!(THREAD.test_ids.replies, "thread-replies");
    }

    /// An active index past the chips is no active chip: the plain pack,
    /// never a panic.
    #[test]
    fn partition_tabs_ignores_an_active_index_past_the_chips() {
        let widths = [40., 40., 40.];
        assert_eq!(
            partition_tabs(&widths, 100., 4., 30., Some(7)),
            partition_tabs(&widths, 100., 4., 30., None)
        );
        assert_eq!(partition_tabs(&[], 100., 4., 30., Some(0)), Vec::<usize>::new());
    }

    /// EXP-1334: `session-thread.json` `stripCases`, whole values.
    #[test]
    fn partition_thread_strip_matches_the_fixture() {
        let fixture = fixture(SESSION_THREAD_JSON);
        let cases = fixture["stripCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let issues: Vec<StripIssueWidth> = serde_json::from_value(input["issues"].clone()).unwrap();
            let sessions: Vec<f32> = serde_json::from_value(input["sessions"].clone()).unwrap();
            let num = |key: &str| input[key].as_f64().unwrap() as f32;
            let index = |key: &str| input[key].as_u64().map(|ix| ix as usize);
            let layout = partition_thread_strip(
                &issues,
                &sessions,
                num("available"),
                num("gap"),
                num("overflowW"),
                index("activeIssue"),
                index("activeSession"),
            );
            assert_whole(&layout, &case["expected"], case["name"].as_str().unwrap());
        }
    }
}
