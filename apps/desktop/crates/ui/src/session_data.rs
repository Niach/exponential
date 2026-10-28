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

use domain::message_conversation::ConversationSessions;
use domain::pr_stack::PrSet;
use domain::rows::{
    Issue, MessageRow, PullRequestIssueRow, PullRequestRow, SessionIssueRow, SessionReadRow, SessionRow,
};
use domain::session_tree::{
    build_session_thread, list_agents, partition_threads, session_card, tree_order,
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

/// A session the agent is working on right now or is about to: the shared
/// four-state rule's `working` (`domain::session_tree::session_state`:
/// queued, or `busy` and neither ended nor waiting on a person). A
/// `running` row between turns is idle. The agent mark's and the prune
/// checks' rule.
pub(crate) fn session_is_working(session: &SessionRow) -> bool {
    domain::session_tree::session_state(session) == domain::session_tree::SessionState::Working
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

/// S-3: the teams whose Assistant this app run already ensured.
#[derive(Default)]
struct EnsuredAssistants(HashSet<String>);

impl gpui::Global for EnsuredAssistants {}

/// S-3 `agents.ensureAssistant({teamId})`: the composer home, the chats list
/// and the Settings Agents group call it on mount; it runs ONCE per team per
/// app run (fire-and-forget, the synced `agents` row is the answer; a
/// failure only logs and re-arms).
pub(crate) fn ensure_assistant(team_id: &str, cx: &mut App) {
    if team_id.is_empty() {
        return;
    }
    if cx.try_global::<EnsuredAssistants>().is_some_and(|done| done.0.contains(team_id)) {
        return;
    }
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    cx.default_global::<EnsuredAssistants>().0.insert(team_id.to_string());
    let team = team_id.to_string();
    cx.spawn(async move |cx| {
        let probe = team.clone();
        let result = cx
            .background_executor()
            .spawn(async move { api::agents::ensure_assistant(&trpc, &probe) })
            .await;
        if let Err(err) = result {
            log::warn!("[ui] agents.ensureAssistant failed: {err}");
            let _ = cx.update(|cx| {
                cx.default_global::<EnsuredAssistants>().0.remove(&team);
            });
        }
    })
    .detach();
}

/// S-3: the viewer's own Assistant in `team_id` (synced row), if any.
pub(crate) fn my_assistant(team_id: &str, cx: &App) -> Option<domain::rows::AgentDefRow> {
    let store = Store::try_global(cx)?;
    let me = signed_in_user(cx);
    let agents = store.collections().agents.read(cx);
    domain::agent_settings::my_assistant(agents.iter(), team_id, me.as_deref()).cloned()
}

/// NEW-2: one synced agent DEFINITION (`agents` shape).
pub(crate) fn agent_def(agent_id: &str, cx: &App) -> Option<domain::rows::AgentDefRow> {
    let store = Store::try_global(cx)?;
    store.collections().agents.read(cx).get(agent_id).cloned()
}

/// NEW-2: the viewer's own conversation session with the agent (a root
/// with `agent_id` and `user_id == me`), `None` until the first message
/// creates it (`sessions.send({agentId})`).
pub(crate) fn my_conversation(agent_id: &str, cx: &App) -> Option<SessionRow> {
    let store = Store::try_global(cx)?;
    let me = crate::queries::active_account(cx)?.user_id;
    store
        .collections()
        .sessions
        .read(cx)
        .iter()
        .find(|row| {
            row.is_root()
                && row.agent_id.as_deref() == Some(agent_id)
                && row.user_id.as_deref() == Some(me.as_str())
        })
        .cloned()
}

/// VAPP-110: the viewer's own EDITOR SESSION root for an app (a root with
/// `app_id` and `user_id == me`), `None` until the first message creates it
/// (`sessions.send({appId})`).
pub(crate) fn my_editor_session(app_id: &str, cx: &App) -> Option<SessionRow> {
    let store = Store::try_global(cx)?;
    let me = crate::queries::active_account(cx)?.user_id;
    store
        .collections()
        .sessions
        .read(cx)
        .iter()
        .find(|row| {
            row.is_root()
                && row.app_id.as_deref() == Some(app_id)
                && row.user_id.as_deref() == Some(me.as_str())
        })
        .cloned()
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
/// (`domain::session_tree::tree_order`). Empty
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

/// THE session card of one session, named by [`session_title_of`].
pub(crate) fn session_card_of(session_id: &str, now_ms: i64, cx: &App) -> Option<SessionCard> {
    let row = session(session_id, cx)?;
    let title = session_title_of(session_id, cx)?;
    Some(session_card(&row, &title, now_ms))
}

/// EXP-1341: THE display title of any session (`domain::session_title`:
/// its name/title, its issues (a root's links, a child's ASSIGNED ones),
/// `Run <action>`, its first prompt, else `Untitled`) off the synced rows;
/// `None` while the session has not synced.
pub(crate) fn session_title_of(session_id: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    // #EXP-1380 PERF: the tab strip asks this for every thread tab on every
    // frame (twice: measure + paint); memoised per store change, it used to
    // walk and clone every synced message each time.
    let revisions = [
        collections.sessions.read(cx).revision(),
        collections.session_issues.read(cx).revision(),
        collections.issues.read(cx).revision(),
        collections.conversations.read(cx).revision(),
    ];
    if let Some(title) = TITLE_MEMO.with(|memo| {
        memo.borrow()
            .get(session_id)
            .filter(|(at, _)| *at == revisions)
            .map(|(_, title)| title.clone())
    }) {
        return title;
    }
    let title = derive_session_title(session_id, cx);
    TITLE_MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.len() > 512 {
            memo.clear();
        }
        memo.insert(session_id.to_string(), (revisions, title.clone()));
    });
    title
}

thread_local! {
    /// [`session_title_of`]'s memo: session id → (the store revisions it
    /// was derived at, the title).
    static TITLE_MEMO: std::cell::RefCell<HashMap<String, ([u64; 4], Option<String>)>> =
        std::cell::RefCell::new(HashMap::new());
}

fn derive_session_title(session_id: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let row = collections.sessions.read(cx).get(session_id)?.clone();
    // NEW-1: a session titles from its OWN rows (the row sits on its holder).
    let links: Vec<SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| link.session_id == session_id)
        .cloned()
        .collect();
    let all_issues = collections.issues.read(cx);
    let issues: Vec<TreeIssue> = links
        .iter()
        .filter_map(|link| all_issues.get(&link.issue_id))
        .map(TreeIssue::from_issue)
        .collect();
    // Thread e4a04d22: the first prompt is the conversation digest's
    // `first` (no read of a long chat's messages at all).
    let messages: Vec<MessageRow> = digest_messages(cx, |owner| owner == session_id)
        .into_iter()
        .filter(|message| is_first_prompt_candidate(message, session_id))
        .collect();
    let link_refs: Vec<&SessionIssueRow> = links.iter().collect();
    let message_refs: Vec<&MessageRow> = messages.iter().collect();
    Some(domain::session_title::session_title(&row, &link_refs, &issues, &message_refs))
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
        // Release train 2026-10-23 (VAPP-110): an app editor's session is the
        // editor's chat, never a thread on the issue.
        .filter(|root| root.is_root() && root.is_thread() && !root.is_app_editor())
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

/// NEW-8: conversation id → session id over the synced `conversations`
/// (a message belongs to the session of its conversation).
pub(crate) fn conversation_sessions(cx: &App) -> ConversationSessions {
    let Some(store) = Store::try_global(cx) else {
        return ConversationSessions::new();
    };
    domain::message_conversation::conversation_sessions(store.collections().conversations.read(cx).iter())
}

/// Thread e4a04d22 (#EXP-1380 on every form factor): THE lists' messages —
/// each session conversation's synced DIGEST read back
/// ([`domain::chats::digest_messages`]), kept where `keep(session_id)`
/// holds. The titles, the Threads bands and the Chats list read these and
/// never the transcript; the open chat, unread marking and pickup stay on
/// [`keyed_messages`].
pub(crate) fn digest_messages(cx: &App, keep: impl Fn(&str) -> bool) -> Vec<MessageRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let conversations = store.collections().conversations.read(cx);
    let mut out: Vec<MessageRow> = Vec::new();
    for conversation in conversations.iter() {
        let Some(session_id) = conversation.session_id.as_deref().filter(|id| !id.is_empty() && keep(id)) else {
            continue;
        };
        let digest = domain::chats::ConversationDigest::decode(conversation.digest.as_ref());
        out.extend(domain::chats::digest_messages(session_id, digest.as_ref()).into_iter().map(|message| MessageRow {
            conversation_id: conversation.id.clone(),
            ..message
        }));
    }
    out
}

/// NEW-8: the synced messages KEYED through their conversation (each
/// `session_id` = its conversation's session; a dm/channel message or one
/// whose conversation has not synced is held back), kept where
/// `keep(session_id, message)` holds. Store order; the row's own legacy
/// `session_id` is never read.
pub(crate) fn keyed_messages(cx: &App, keep: impl Fn(&str, &MessageRow) -> bool) -> Vec<MessageRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let sessions = conversation_sessions(cx);
    let messages = store.collections().messages.read(cx);
    domain::message_conversation::key_messages_by_session(
        messages.iter().filter(|message| {
            domain::message_conversation::session_of(message, &sessions)
                .is_some_and(|session_id| keep(session_id, message))
        }),
        &sessions,
    )
}

fn sort_messages(messages: &mut [MessageRow]) {
    messages.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// The messages of ONE session (its own conversation), oldest first.
pub(crate) fn messages_of_session(session_id: &str, cx: &App) -> Vec<MessageRow> {
    let mut out: Vec<MessageRow> = keyed_messages(cx, |owner, _| owner == session_id);
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

/// The thread screen's model for `root_id` (`build_session_thread`): the
/// tabs, the conversation rows, the composer target; `None` while the root
/// has not synced. Works for an agent root too (its channel). #EXP-1380:
/// with the root's own messages (oldest first) off the SAME scan.
/// `whole_tree` (VAPP-110) = an app EDITOR's channel over the WHOLE tree's
/// messages (web `useTreeMessages`): the editor session's children (and a
/// pre-FEED-134 continuation child) post there too.
pub(crate) fn thread_view_and_messages(
    root_id: &str,
    focus: Option<&str>,
    now_ms: i64,
    whole_tree: bool,
    cx: &App,
) -> Option<(SessionThreadView, Vec<MessageRow>)> {
    thread_view(root_id, focus, now_ms, whole_tree, cx)
}

/// #EXP-1380: a card-less input that may name `owner` (another team
/// session): `session_title::first_prompt`'s candidates (a person's text or
/// another session's input, with a usable first line). Only the oldest is
/// kept per session — all another root's plain messages are ever read for
/// here, so a 1500-message chat beside this one no longer rides along.
fn is_first_prompt_candidate(message: &MessageRow, owner: &str) -> bool {
    matches!(message.card, None | Some(serde_json::Value::Null))
        && (message.is_from_user()
            || message
                .author_session_id
                .as_deref()
                .is_some_and(|author| !author.is_empty() && author != owner))
        && domain::session_title::first_line(message.body_text()).is_some()
}

fn thread_view(
    root_id: &str,
    focus: Option<&str>,
    now_ms: i64,
    whole_tree: bool,
    cx: &App,
) -> Option<(SessionThreadView, Vec<MessageRow>)> {
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
    let team_ids: HashSet<&str> = team_sessions.iter().map(|session| session.id.as_str()).collect();
    // EXP-1341: every team root's links name its cards (a child's assigned
    // issues sit on its root); the tabs read the root's own.
    let mut links: Vec<&SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| team_ids.contains(link.session_id.as_str()))
        .collect();
    // One order everywhere (title, tabs, the list row's identifier).
    sort_links(&mut links);
    // EXP-1334: the root's own messages plus its children's question / plan
    // cards (they surface in the Conversation, answered in place).
    let tree_ids: HashSet<&str> = team_sessions
        .iter()
        .filter(|session| session.root_id_or_self() == root_id)
        .map(|session| session.id.as_str())
        .collect();
    // NEW-8: every message keyed through its conversation.
    // EXP-1341: a card-less input names its session (first prompt); #EXP-1380:
    // another root's only as that one input.
    let owners = conversation_sessions(cx);
    let all_messages = collections.messages.read(cx);
    // ONE pass: the root's (and its tree's) rows, plus each other team
    // session's first prompt.
    let (mut picked, mut first): (Vec<&MessageRow>, HashMap<&str, (i64, &MessageRow)>) = (Vec::new(), HashMap::new());
    for message in all_messages.iter() {
        let Some(owner) = domain::message_conversation::session_of(message, &owners) else {
            continue;
        };
        let in_tree = owner == root_id || tree_ids.contains(owner);
        if in_tree {
            let card_kind = message.card.as_ref().and_then(|card| card.get("kind")).and_then(serde_json::Value::as_str);
            if owner == root_id
                || message.card.is_none()
                || matches!(
                    card_kind,
                    Some(domain::contract::MESSAGE_CARD_KIND_QUESTION) | Some(domain::contract::MESSAGE_CARD_KIND_PLAN)
                )
            {
                picked.push(message);
            }
        } else if team_ids.contains(owner) && is_first_prompt_candidate(message, owner) {
            let at = message.created_at.as_deref().and_then(domain::session_tree::iso_ms).unwrap_or(0);
            let slot = first.entry(owner).or_insert((at, message));
            if (at, message.id.as_str()) < (slot.0, slot.1.id.as_str()) {
                *slot = (at, message);
            }
        }
    }
    picked.extend(first.into_values().map(|(_, message)| message));
    // #EXP-1399: an `ask` mirror from another tree draws THE origin card,
    // read from the origin message (`card.messageId`): load those by id.
    let mut picked_ids: HashSet<&str> = picked.iter().map(|message| message.id.as_str()).collect();
    let origins: Vec<&MessageRow> = picked
        .iter()
        .filter_map(|message| {
            let card = message.card.as_ref()?;
            (card.get("kind")?.as_str()? == domain::contract::MESSAGE_CARD_KIND_ASK)
                .then(|| card.get("messageId")?.as_str())
                .flatten()
        })
        .filter_map(|id| all_messages.get(id))
        .filter(|origin| picked_ids.insert(origin.id.as_str()))
        .collect();
    picked.extend(origins);
    let keyed: Vec<MessageRow> = domain::message_conversation::key_messages_by_session(picked, &owners);
    let messages: Vec<&MessageRow> = keyed.iter().collect();
    // VAPP-110: the tree's plain messages re-homed onto the root (cards stay
    // where they are: a continuation's own session card is not a message).
    let rehomed: Vec<MessageRow> = if whole_tree {
        messages
            .iter()
            .filter(|message| {
                message.session_id != root_id
                    && tree_ids.contains(message.session_id.as_str())
                    && message.card.is_none()
            })
            .map(|message| MessageRow { session_id: root_id.to_string(), ..(*message).clone() })
            .collect()
    } else {
        Vec::new()
    };
    let messages: Vec<&MessageRow> = messages.into_iter().chain(rehomed.iter()).collect();
    // EXP-1334: each issue's parent (`issue_relations` kind parent, the
    // lowest parent id when several) so linked sub-issues fold under their
    // nearest LINKED ancestor; the unlinked ancestors ride along so the
    // walk can pass them.
    let relations = collections.issue_relations.read(cx);
    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    for relation in relations.iter().filter(|relation| relation.kind.as_deref() == Some("parent")) {
        let (parent, child) = (relation.issue_id.as_str(), relation.related_issue_id.as_str());
        if parent == child {
            continue;
        }
        let slot = parent_of.entry(child).or_insert(parent);
        if parent < *slot {
            *slot = parent;
        }
    }
    // The linked issues plus every issue an event card names: an unlinked
    // issue still reads "Unlinked EXP-12" (the event lines look it up).
    let all_issues = collections.issues.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let named = links
        .iter()
        .map(|link| link.issue_id.as_str())
        .chain(messages.iter().filter_map(|message| {
            message
                .card
                .as_ref()?
                .get("issueId")?
                .as_str()
        }))
        // EXP-1323: a filed suggestion reads `Filed EXP-9`.
        .chain(messages.iter().flat_map(|message| {
            message
                .card
                .as_ref()
                .and_then(|card| card.get("items"))
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("issueId")?.as_str())
        }))
        .filter(|id| seen.insert(id))
        .collect::<Vec<&str>>();
    // Each issue's parent = its nearest SYNCED ancestor over the relations
    // themselves (like the web's `linkedIssueParents` walk): an ancestor
    // this device has not synced is walked past, so a missing intermediate
    // never breaks the chain and promotes a sub-issue to a top-level chip.
    let synced_parent = |id: &str| -> Option<String> {
        let mut walked: HashSet<&str> = HashSet::from([id]);
        let mut parent = parent_of.get(id).copied();
        while let Some(candidate) = parent {
            if !walked.insert(candidate) {
                return None;
            }
            if all_issues.get(candidate).is_some() {
                return Some(candidate.to_string());
            }
            parent = parent_of.get(candidate).copied();
        }
        None
    };
    let mut issues: Vec<TreeIssue> = named
        .iter()
        .filter_map(|id| all_issues.get(id))
        .map(|issue| TreeIssue::from_issue(issue).with_parent(synced_parent(issue.id.as_str())))
        .collect();
    // The ancestors ride along (synced or not, the walk passes them) so the
    // fold reaches every linked ancestor.
    let mut cursor: Vec<&str> = named.iter().filter_map(|id| parent_of.get(id).copied()).collect();
    while let Some(id) = cursor.pop() {
        if !seen.insert(id) {
            continue;
        }
        cursor.extend(parent_of.get(id).copied());
        if let Some(issue) = all_issues.get(id) {
            issues.push(TreeIssue::from_issue(issue).with_parent(synced_parent(id)));
        }
    }
    let pulls: Vec<&PullRequestRow> = collections
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.team_id == root.team_id)
        .collect();
    let view = build_session_thread(
        SessionThreadInput {
            root,
            sessions: &team_sessions,
            session_issues: &links,
            issues: &issues,
            messages: &messages,
            pull_requests: &pulls,
            now_ms,
        },
        focus,
    );
    // The root's own messages (an editor: the tree's), oldest first.
    let mut own: Vec<MessageRow> = keyed
        .iter()
        .filter(|message| {
            message.session_id == root_id || (whole_tree && tree_ids.contains(message.session_id.as_str()))
        })
        .cloned()
        .collect();
    sort_messages(&mut own);
    Some((view, own))
}

// ---------------------------------------------------------------------------
// The Threads page, the rail and the team picker
// ---------------------------------------------------------------------------

/// The rail's Agents section: the agent DEFINITIONS of `team_id` (NEW-2,
/// `domain::session_tree::list_agents`), lit off the viewer's own
/// conversation session with each.
pub(crate) fn agents_in_team(team_id: &str, cx: &App) -> Vec<AgentRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let agents = store.collections().agents.read(cx);
    let agents: Vec<&domain::rows::AgentDefRow> = agents.iter().collect();
    let sessions = store.collections().sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions.iter().collect();
    // An agent's spinner / needs-you dot is the viewer's conversation only:
    // the threads it spawns are their own roots (the Threads entry lights
    // for them).
    list_agents(&agents, &rows, Some(team_id), me.as_deref())
}

/// The store state one [`thread_lists`] answer derives from.
#[derive(Clone, PartialEq, Eq)]
struct ListsKey {
    team_id: String,
    revisions: [u64; 5],
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
            collections.conversations.read(cx).revision(),
            collections.issues.read(cx).revision(),
            collections.session_reads.read(cx).revision(),
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
    // NEW-1: every session's OWN rows title it (root or child alike).
    let team_session_ids: HashSet<&str> = rows.iter().map(|session| session.id.as_str()).collect();
    let mut links: Vec<&SessionIssueRow> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| team_session_ids.contains(link.session_id.as_str()))
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
    // A session's own messages title it; EXP-1323: the tree's agent messages
    // decide its unread bold (and each child's own).
    let in_team: HashSet<&str> = rows.iter().map(|session| session.id.as_str()).collect();
    // EXP-1341: every session's own first prompt names it; thread e4a04d22:
    // read off each conversation's digest, never the transcript.
    let keyed: Vec<MessageRow> = digest_messages(cx, |owner| in_team.contains(owner));
    let messages: Vec<&MessageRow> = keyed.iter().collect();
    let session_reads = collections.session_reads.read(cx);
    let reads: Vec<&SessionReadRow> = session_reads
        .iter()
        .filter(|read| in_team.contains(read.session_id.as_str()))
        .collect();
    partition_threads(ThreadsListInput {
        sessions: &rows,
        session_issues: &links,
        issues: &issues,
        messages: &messages,
        team_id: Some(team_id),
        reads: &reads,
        now_ms: Some(now_ms()),
    })
}

// ---------------------------------------------------------------------------
// The Chats page (#EXP-1380: the list on every form factor)
// ---------------------------------------------------------------------------

/// The Chats page's model: the WHOLE list (filter All, chips filter it in
/// the view) and the agent bubbles.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChatsModel {
    pub(crate) list: domain::chats::ChatList,
    /// #EXP-1388: the Archived page's list (view `archived`, filter All).
    pub(crate) archived: domain::chats::ChatList,
    pub(crate) bubbles: Vec<domain::chats::AgentBubble>,
}

#[derive(Clone, PartialEq, Eq)]
struct ChatsKey {
    team_id: String,
    revisions: [u64; 7],
    /// The time labels move with the local hour (today → Yesterday).
    hour: i64,
}

#[derive(Default)]
struct ChatsMemo(Option<(ChatsKey, Rc<ChatsModel>)>);

impl gpui::Global for ChatsMemo {}

/// [`ChatsModel`] of `team_id` (`domain::chats::chat_list` +
/// `agent_bubbles`), memoised per store change.
pub(crate) fn chats_model(team_id: &str, cx: &mut App) -> Rc<ChatsModel> {
    let Some(store) = Store::try_global(cx) else {
        return Rc::new(ChatsModel::default());
    };
    let collections = store.collections();
    let key = ChatsKey {
        team_id: team_id.to_string(),
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.session_issues.read(cx).revision(),
            collections.conversations.read(cx).revision(),
            collections.issues.read(cx).revision(),
            collections.session_reads.read(cx).revision(),
            collections.pull_requests.read(cx).revision(),
            collections.agents.read(cx).revision(),
        ],
        hour: now_ms() / 3_600_000,
    };
    if let Some((cached, model)) = &cx.default_global::<ChatsMemo>().0 {
        if *cached == key {
            return model.clone();
        }
    }
    let groups = thread_lists(team_id, cx);
    let model = Rc::new(derive_chats_model(team_id, &groups, cx));
    cx.global_mut::<ChatsMemo>().0 = Some((key, model.clone()));
    model
}

fn derive_chats_model(team_id: &str, groups: &[ThreadsListGroup], cx: &App) -> ChatsModel {
    let collections = Store::global(cx).collections();
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let agents = agents_in_team(team_id, cx);
    let sessions = collections.sessions.read(cx);
    let team_sessions: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .collect();
    // Only the ROOT conversations (the list's roots + my agent chats) count.
    let roots: HashSet<&str> = groups
        .iter()
        .flat_map(|group| group.rows.iter())
        .filter(|row| row.depth == 0)
        .map(|row| row.session_id.as_str())
        .chain(
            team_sessions
                .iter()
                .filter(|session| session.is_root() && session.agent_id.is_some())
                .map(|session| session.id.as_str()),
        )
        .collect();
    // Thread e4a04d22: the digest's messages, never the transcript.
    let keyed: Vec<MessageRow> = digest_messages(cx, |owner| roots.contains(owner));
    let messages: Vec<&MessageRow> = keyed.iter().collect();
    let session_reads = collections.session_reads.read(cx);
    let reads: Vec<&SessionReadRow> = session_reads
        .iter()
        .filter(|read| roots.contains(read.session_id.as_str()))
        .collect();
    let pulls: Vec<PullRequestRow> = collections
        .pull_requests
        .read(cx)
        .iter()
        .filter(|pull| pull.team_id.as_deref() == Some(team_id))
        .cloned()
        .collect();
    let mut pr_states: HashMap<String, Vec<String>> = HashMap::new();
    for pull in &pulls {
        if let Some(root) = pull.root_session_id.as_deref() {
            pr_states.entry(root.to_string()).or_default().push(pull.state.clone().unwrap_or_default());
        }
    }
    let words: HashMap<String, domain::session_tree::ThreadRowWord> = groups
        .iter()
        .flat_map(|group| group.rows.iter())
        .filter(|row| row.depth == 0)
        .filter_map(|row| {
            crate::threads_view::thread_word(&row.session_id, row.busy, row.needs_you, &pulls)
                .map(|word| (row.session_id.clone(), word))
        })
        .collect();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
    let visible: HashSet<&str> = sessions.iter().map(|session| session.id.as_str()).collect();
    let list_of = |view| {
        domain::chats::chat_list(domain::chats::ChatListInput {
            groups,
            messages: &messages,
            reads: &reads,
            pr_states: &pr_states,
            words: &words,
            filter: domain::chats::ChatFilter::All,
            view,
            now: &now,
            utc_offset_minutes: offset,
            visible_session_ids: Some(&visible),
        })
    };
    let list = list_of(domain::chats::ChatListView::List);
    let archived = list_of(domain::chats::ChatListView::Archived);
    let bubbles = domain::chats::agent_bubbles(&agents, &team_sessions, &messages, &reads, me.as_deref());
    ChatsModel { list, archived, bubbles }
}

/// EXP-1323: the Threads entry's unread dot — any thread of `team_id` has
/// an agent reply the viewer has not read (`threads_unread`).
/// #EXP-1388: an archived chat never lights it (`chats_unread`).
pub(crate) fn rail_threads_unread(team_id: &str, cx: &mut App) -> bool {
    let groups = thread_lists(team_id, cx);
    let Some(store) = Store::try_global(cx) else {
        return domain::session_tree::threads_unread(&groups);
    };
    let session_reads = store.collections().session_reads.read(cx);
    let reads: Vec<&SessionReadRow> = session_reads.iter().filter(|read| read.archived_at.is_some()).collect();
    domain::chats::chats_unread(&groups, &reads)
}

// ---------------------------------------------------------------------------
// Read marks (EXP-1323)
// ---------------------------------------------------------------------------

/// The newest agent-authored message of `session_id` (`whole_tree` = of
/// every session under that root) when it is newer than the viewer's
/// synced read of `session_id` — what a `sessions.markRead` would clear;
/// `None` = already read (or nothing to read).
pub(crate) fn unread_marker(session_id: &str, whole_tree: bool, cx: &App) -> Option<i64> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    // NEW-8: a message's session is its conversation's.
    let owners = conversation_sessions(cx);
    let in_scope = |message: &MessageRow| {
        domain::message_conversation::session_of(message, &owners).is_some_and(|owner| {
            owner == session_id
                || (whole_tree
                    && sessions.get(owner).is_some_and(|session| session.root_id_or_self() == session_id))
        })
    };
    let newest = collections
        .messages
        .read(cx)
        .iter()
        .filter(|message| message.author_kind.as_deref() == Some(domain::contract::MESSAGE_AUTHOR_KIND_AGENT))
        .filter(|message| in_scope(message))
        .map(|message| message.created_at.as_deref().and_then(domain::session_tree::iso_ms).unwrap_or(0))
        .max()?;
    let read = collections
        .session_reads
        .read(cx)
        .iter()
        .filter(|read| read.session_id == session_id)
        .filter_map(|read| read.read_at.as_deref().and_then(domain::session_tree::iso_ms))
        .max()
        .unwrap_or(-1);
    (newest > read).then_some(newest)
}

/// The marks already sent this run (session id → the newest message they
/// cleared), so a render loop never re-sends one before it syncs back.
#[derive(Default)]
struct SentReads(HashMap<String, i64>);

impl gpui::Global for SentReads {}

/// The collection revisions each session's [`unread_marker`] scan last ran
/// at (`messages`, `session_reads`, `sessions` for a whole tree, and NEW-8
/// `conversations`): the
/// screens call [`mark_read_if_unread`] every frame, and the scan walks every
/// synced message — so a frame in which nothing synced skips it.
#[derive(Default)]
struct ReadScans(HashMap<String, (u64, u64, u64, u64)>);

impl gpui::Global for ReadScans {}

/// EXP-1323: `sessions.markRead` once `session_id` shows news the viewer
/// has not read ([`unread_marker`]): the thread (`whole_tree`, a root) or a
/// session view. A background write: a failure only logs (never a toast).
pub(crate) fn mark_read_if_unread(session_id: &str, whole_tree: bool, cx: &mut App) {
    let Some(store) = Store::try_global(cx) else {
        return;
    };
    let collections = store.collections();
    let revisions = (
        collections.messages.read(cx).revision(),
        collections.session_reads.read(cx).revision(),
        if whole_tree { collections.sessions.read(cx).revision() } else { 0 },
        collections.conversations.read(cx).revision(),
    );
    let scans = cx.default_global::<ReadScans>();
    if scans.0.get(session_id) == Some(&revisions) {
        return;
    }
    scans.0.insert(session_id.to_string(), revisions);
    let Some(newest) = unread_marker(session_id, whole_tree, cx) else {
        return;
    };
    let sent = cx.default_global::<SentReads>();
    if sent.0.get(session_id).is_some_and(|marked| *marked >= newest) {
        return;
    }
    sent.0.insert(session_id.to_string(), newest);
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    let session_id = session_id.to_string();
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::sessions::mark_read(&trpc, &session_id) {
                log::warn!("[threads] mark {session_id} read failed: {err}");
            }
        })
        .detach();
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
        // Release train 2026-10-23 (VAPP-110): a working app editor is not a
        // thread, so it stays out of Running and the switcher's dot.
        .filter(|session| session.is_root() && session.is_thread() && !session.is_app_editor())
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

/// One row of the rail's Active band (EXP-1318): a thread the shapes make
/// visible that needs you, runs, or still asks for a merge — the mark and
/// the title, no identifier, no caption. EXP-1323: a live tree's children
/// follow their root as nested rows (`depth` > 0, `session_id` = the child).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveRootRow {
    pub(crate) root_id: String,
    /// The row's own session (= `root_id` on the thread row).
    pub(crate) session_id: String,
    pub(crate) depth: usize,
    /// EXP-1323: a bold title.
    pub(crate) unread: bool,
    pub(crate) team_id: String,
    pub(crate) title: String,
    pub(crate) busy: bool,
    pub(crate) needs_you: bool,
    /// EXP-1318: the root session's device (the trailing device icon).
    pub(crate) device_id: Option<String>,
    /// Every session of the tree — the row stays lit while one is on screen
    /// (a child row: itself only).
    pub(crate) session_ids: Vec<String>,
    /// S-3: the tree's fold row (`<root>:earlier`) = how many sub-sessions
    /// it hides; it opens the thread.
    pub(crate) earlier: Option<usize>,
}

/// Pure core of [`live_roots_for_rail`]: the contract's `railThreads` over
/// the Threads bands (`groups`, every visible thread, not only mine), the
/// words off the team's pull requests; group order.
pub(crate) fn live_roots_from(
    groups: &[ThreadsListGroup],
    sessions: &[&SessionRow],
    team_pulls: &[PullRequestRow],
) -> Vec<LiveRootRow> {
    domain::session_tree::rail_threads(groups, |row| {
        crate::threads_view::thread_word(&row.session_id, row.busy, row.needs_you, team_pulls)
    })
    .into_iter()
    .map(|row| {
        let session_ids = if row.earlier.is_some() {
            Vec::new()
        } else if row.depth > 0 {
            vec![row.session_id.clone()]
        } else {
            let tree: Vec<&SessionRow> = sessions
                .iter()
                .copied()
                .filter(|session| session.id == row.session_id || session.root_id_or_self() == row.session_id)
                .collect();
            tree_order(&tree).iter().map(|session| session.id.clone()).collect()
        };
        LiveRootRow {
            session_ids,
            root_id: if row.root_id.is_empty() { row.session_id.clone() } else { row.root_id },
            session_id: row.session_id,
            depth: row.depth,
            unread: row.unread,
            team_id: row.team_id,
            title: row.title,
            busy: row.busy,
            needs_you: row.needs_you,
            device_id: row.device_id,
            earlier: row.earlier,
        }
    })
    .collect()
}

/// The rail's Active band for `team_id` ([`live_roots_from`]).
pub(crate) fn live_roots_for_rail(team_id: &str, cx: &mut App) -> Vec<LiveRootRow> {
    let groups = thread_lists(team_id, cx);
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let pulls = team_pr_snapshot(team_id, cx).pulls;
    let sessions = store.collections().sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .collect();
    live_roots_from(&groups, &rows, &pulls)
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
        // NEW-8: a message's session is its conversation's.
        cx.observe(&collections.conversations, |_, _, cx| cx.notify()),
        cx.observe(&collections.pull_requests, |_, _, cx| cx.notify()),
        cx.observe(&collections.pull_request_issues, |_, _, cx| cx.notify()),
        cx.observe(&collections.issues, |_, _, cx| cx.notify()),
        // #EXP-1380: the Chats list's unread badges, bold titles and bubble
        // rings follow the viewer's read marks.
        cx.observe(&collections.session_reads, |_, _, cx| cx.notify()),
        // NEW-2: the Agents rows are the agent definitions.
        cx.observe(&collections.agents, |_, _, cx| cx.notify()),
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
            "status": status, "busy": status == "running", "root_id": id, "shared": true,
            "created_at": "2026-10-13T10:00:00.000Z",
            "updated_at": "2026-10-13T10:00:00.000Z",
        }))
    }

    fn child(id: &str, root_id: &str, team: &str, user: &str, status: &str, updated_at: &str) -> SessionRow {
        row(json!({
            "id": id, "team_id": team, "kind": "thread", "user_id": user,
            "status": status, "busy": status == "running", "parent_id": root_id, "root_id": root_id, "depth": 1,
            "created_at": "2026-10-13T10:01:00.000Z",
            "updated_at": updated_at,
        }))
    }

    #[test]
    fn working_means_queued_or_busy() {
        assert!(session_is_working(&root("a", "t", "me", "queued")));
        // `root` marks a `running` row busy (a turn in flight).
        assert!(session_is_working(&root("a", "t", "me", "running")));
        assert!(!session_is_working(&root("a", "t", "me", "idle")));
        assert!(!session_is_working(&root("a", "t", "me", "ended")));
        // S-3: `running` between turns (not busy) is idle, not working.
        let mut between = root("a", "t", "me", "running");
        between.busy = false;
        assert!(!session_is_working(&between));
        // Waiting on a person beats busy; ended never works.
        let mut asking = root("a", "t", "me", "running");
        asking.needs_input = true;
        assert!(!session_is_working(&asking));
        let mut over = root("a", "t", "me", "ended");
        over.busy = true;
        assert!(!session_is_working(&over));
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
        // NEW-2: an agent's conversation is decided by `agent_id`, never `kind`.
        agent.agent_id = Some("ag-1".into());
        let mut loose = root("r1", "t1", "me", "running");
        loose.team_id = None;
        let sessions = [agent, loose];
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        assert!(own_live_roots_by_team_from(&refs, "me").is_empty());
    }

    /// EXP-1318: the Active band = needs you + running + the recent rows
    /// still asking for a merge, every visible thread (not only mine).
    #[test]
    fn the_rail_lists_needs_you_running_and_ready_to_merge() {
        let band = |key: ThreadsListKey, ids: &[&str]| ThreadsListGroup {
            key,
            title: key.title().to_string(),
            rows: ids
                .iter()
                .map(|id| domain::session_tree::ThreadsListRow {
                    session_id: id.to_string(),
                    team_id: "t1".into(),
                    depth: 0,
                    parent_id: None,
                    root_id: id.to_string(),
                    title: id.to_string(),
                    device_id: None,
                    busy: false,
                    needs_you: false,
                    unread: false,
                    activity_at: String::new(),
                    earlier: None,
                })
                .collect(),
        };
        let groups = [band(ThreadsListKey::Running, &["r1"]), band(ThreadsListKey::Recent, &["r2", "r3"])];
        let sessions = [
            root("r1", "t1", "someone", "running"),
            child("c1", "r1", "t1", "me", "running", "2026-10-13T11:00:00.000Z"),
            root("r2", "t1", "me", "idle"),
            root("r3", "t1", "me", "idle"),
        ];
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        let pulls: Vec<PullRequestRow> = vec![serde_json::from_value(json!({
            "id": "p", "root_session_id": "r3", "state": "open",
        }))
        .unwrap()];
        let rows = live_roots_from(&groups, &refs, &pulls);
        let ids: Vec<&str> = rows.iter().map(|row| row.root_id.as_str()).collect();
        assert_eq!(ids, vec!["r1", "r3"]);
        assert_eq!(rows[0].session_ids, vec!["r1".to_string(), "c1".to_string()]);
    }

    #[test]
    fn the_badge_reads_the_needs_you_and_running_bands() {
        let band = |key: ThreadsListKey| ThreadsListGroup {
            key,
            title: key.title().to_string(),
            rows: vec![domain::session_tree::ThreadsListRow {
                session_id: "r".into(),
                team_id: "t".into(),
                depth: 0,
                parent_id: None,
                root_id: "r".into(),
                title: "x".into(),
                device_id: None,
                busy: false,
                needs_you: false,
                unread: false,
                activity_at: String::new(),
                earlier: None,
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
