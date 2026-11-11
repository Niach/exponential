//! EXP-1294 — the read helpers over the ONE session tree.
//!
//! Five synced collections carry the chats: `sessions` (roots of kind
//! `thread` (wire kind) | `agent` and their child sessions), `session_issues` (root ↔
//! issue), `messages` (every conversation row and card), `pull_requests` and
//! `pull_request_issues`. Every screen reads them through here and feeds the
//! pure `domain::session_tree` / `domain::pr_stack` functions (the Rust
//! mirrors of `session-tree.ts`, fixture-replayed), so a view never filters a
//! collection itself (masterplan-v3 §4.1, the `queries.rs` rule).
//!
//! The functions taking `&App` are plain reads; the two list reads every rail
//! render asks for ([`chat_lists`], [`rail_chats_badge`]) memoise per
//! store change and take `&mut App`. Pure cores sit next to their `cx`
//! wrappers and carry the tests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use domain::pr_stack::PrSet;
use domain::rows::{
    Issue, MessageRow, PullRequestIssueRow, PullRequestRow, SessionIssueRow, SessionReadRow, SessionRow,
};
use domain::session_tree::{
    build_session_chat, list_agents, partition_chats, session_card, tree_order,
    AgentRow, SessionCard, SessionChatInput, SessionChatRow, SessionChatView, ChatsListGroup, ChatsListInput,
    ChatsListKey, TreeIssue,
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

/// #EXP-1714: the synced teams' ids (`collections().teams_sorted()`; a
/// member's hidden personal team never syncs), what `agents_in_team` tells a
/// global Assistant by.
pub(crate) fn synced_team_ids(cx: &App) -> Vec<String> {
    Store::try_global(cx)
        .map(|store| store.collections().teams_sorted(cx).into_iter().map(|team| team.id).collect())
        .unwrap_or_default()
}

/// #EXP-1714: whether a team-scoped session list shows `session`: the team's
/// own, plus the viewer's own sessions in a team that does not sync (their
/// personal team: the global Assistant's conversation). Nothing counts as
/// personal before the teams sync.
pub(crate) fn in_team_or_personal(session: &SessionRow, team_id: &str, team_ids: &[String], me: Option<&str>) -> bool {
    match session.team_id.as_deref() {
        Some(team) if team == team_id => true,
        Some(team) => {
            !team_ids.is_empty()
                && me.is_some()
                && session.user_id.as_deref() == me
                && !team_ids.iter().any(|id| id == team)
        }
        None => false,
    }
}

/// #EXP-1714 (`domain::agent_settings::agents_in_team`): the agent rows
/// `team_id` lists: its own, plus the viewer's global Assistant while the
/// team holds no Assistant of theirs.
pub(crate) fn team_agent_defs(team_id: &str, cx: &App) -> Vec<domain::rows::AgentDefRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let team_ids = synced_team_ids(cx);
    let team_ids: Vec<&str> = team_ids.iter().map(String::as_str).collect();
    let agents = store.collections().agents.read(cx);
    domain::agent_settings::agents_in_team(agents.iter(), team_id, &team_ids, me.as_deref())
        .into_iter()
        .cloned()
        .collect()
}

/// S-3: the viewer's own Assistant as `team_id` lists it (#EXP-1714: the
/// team's own row, else the global one), if synced.
pub(crate) fn my_assistant(team_id: &str, cx: &App) -> Option<domain::rows::AgentDefRow> {
    let store = Store::try_global(cx)?;
    let me = signed_in_user(cx);
    let team_ids = synced_team_ids(cx);
    let team_ids: Vec<&str> = team_ids.iter().map(String::as_str).collect();
    let agents = store.collections().agents.read(cx);
    domain::agent_settings::my_assistant(agents.iter(), team_id, &team_ids, me.as_deref()).cloned()
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
    // #EXP-1380 PERF: the tab strip asks this for every chat tab on every
    // frame (twice: measure + paint); memoised per store change, it used to
    // walk and clone every synced message each time. #LOGS-11: ONE memo per
    // store change (Needs you titles every session of the team: a per-entry
    // cap thrashed past 512 sessions).
    let revisions = [
        collections.sessions.read(cx).revision(),
        collections.session_issues.read(cx).revision(),
        collections.issues.read(cx).revision(),
    ];
    if let Some(title) = TITLE_MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.0 != revisions {
            *memo = (revisions, HashMap::new());
        }
        memo.1.get(session_id).cloned()
    }) {
        return title;
    }
    let title = derive_session_title(session_id, cx);
    TITLE_MEMO.with(|memo| memo.borrow_mut().1.insert(session_id.to_string(), title.clone()));
    title
}

thread_local! {
    /// [`session_title_of`]'s memo: the store revisions it holds for, and
    /// session id → title.
    static TITLE_MEMO: std::cell::RefCell<([u64; 3], HashMap<String, Option<String>>)> = Default::default();
}

/// #LOGS-11: the `session_issues` rows of one session, through an index
/// built once per change of the collection (a per-session scan made every
/// title of a large team O(sessions × links)).
pub(crate) fn links_of_session(session_id: &str, cx: &App) -> Vec<SessionIssueRow> {
    thread_local! {
        static INDEX: std::cell::RefCell<(Option<(gpui::EntityId, u64)>, Rc<HashMap<String, Vec<SessionIssueRow>>>)> = Default::default();
    }
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collection = &store.collections().session_issues;
    let stamp = (collection.entity_id(), collection.read(cx).revision());
    let index = INDEX.with(|index| {
        let mut index = index.borrow_mut();
        if index.0 != Some(stamp) {
            let mut by_session: HashMap<String, Vec<SessionIssueRow>> = HashMap::new();
            for link in collection.read(cx).iter() {
                by_session.entry(link.session_id.clone()).or_default().push(link.clone());
            }
            *index = (Some(stamp), Rc::new(by_session));
        }
        index.1.clone()
    });
    index.get(session_id).cloned().unwrap_or_default()
}

fn derive_session_title(session_id: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    // #LOGS-3: borrowed (a clone carried the row's whole digest).
    let sessions = collections.sessions.read(cx);
    let row = sessions.get(session_id)?;
    // NEW-1: a session titles from its OWN rows (the row sits on its holder).
    let links = links_of_session(session_id, cx);
    let all_issues = collections.issues.read(cx);
    let issues: Vec<TreeIssue> = links
        .iter()
        .filter_map(|link| all_issues.get(&link.issue_id))
        .map(TreeIssue::from_issue)
        .collect();
    // Session e4a04d22: the first prompt is the conversation digest's
    // `first` (no read of a long chat's messages at all), off the row itself.
    let messages: Vec<MessageRow> = digest_of(&row.id, cx)
        .iter()
        .filter(|message| is_first_prompt_candidate(message, session_id))
        .cloned()
        .collect();
    let link_refs: Vec<&SessionIssueRow> = links.iter().collect();
    let message_refs: Vec<&MessageRow> = messages.iter().collect();
    Some(domain::session_title::session_title(row, &link_refs, &issues, &message_refs))
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

/// The chat roots an issue sits in (the issue's Chats row), newest
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
        // editor's chat, never a chat on the issue.
        .filter(|root| root.is_root() && root.is_chat() && !root.is_app_editor())
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

/// Session e4a04d22 (#EXP-1380 on every form factor): THE lists' messages —
/// each session conversation's synced DIGEST read back
/// ([`domain::chats::digest_messages`]), kept where `keep(session_id)`
/// holds. The titles, the Chats bands and the Chats list read these and
/// never the transcript; the open chat, unread marking and pickup stay on
/// [`keyed_messages`].
pub(crate) fn digest_messages(cx: &App, keep: impl Fn(&str) -> bool) -> Vec<MessageRow> {
    let digests = digests(cx);
    let mut out: Vec<MessageRow> = Vec::new();
    for (session_id, messages) in digests.iter() {
        if keep(session_id.as_str()) {
            out.extend(messages.iter().cloned());
        }
    }
    out
}

/// One session's [`digest_messages`].
pub(crate) fn digest_of(session_id: &str, cx: &App) -> Rc<Vec<MessageRow>> {
    digests(cx).get(session_id).cloned().unwrap_or_default()
}

type DigestIndex = HashMap<String, Rc<Vec<MessageRow>>>;

thread_local! {
    /// [`digests`]' memo: the `sessions` collection and revision it holds
    /// for, and each session's digest read back.
    static DIGESTS: std::cell::RefCell<(Option<(gpui::EntityId, u64)>, Rc<DigestIndex>)> = Default::default();
}

/// #LOGS-3: every session's digest read back
/// ([`domain::chats::digest_messages`]), kept across store changes: a
/// sessions delta re-decodes only the rows it wrote (the change log), not
/// every digest of the account four times over (the chat lists, Needs you,
/// the Chats model, the open chat's names: ~1,500 digests per delta on a
/// real account).
fn digests(cx: &App) -> Rc<DigestIndex> {
    let Some(store) = Store::try_global(cx) else {
        return Rc::default();
    };
    let collection = &store.collections().sessions;
    let sessions = collection.read(cx);
    let stamp = (collection.entity_id(), sessions.revision());
    let decode = |session: &SessionRow| {
        let digest = domain::chats::ConversationDigest::decode(session.digest.as_ref());
        Rc::new(domain::chats::digest_messages(&session.id, digest.as_ref()))
    };
    DIGESTS.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.0 == Some(stamp) {
            return memo.1.clone();
        }
        let changed = memo
            .0
            .filter(|(entity, _)| *entity == stamp.0)
            .and_then(|(_, revision)| sessions.changed_since(revision));
        let index = match changed {
            Some(keys) => {
                let mut index = Rc::unwrap_or_clone(std::mem::take(&mut memo.1));
                for key in keys {
                    let sync::protocol::RowKey::Single(id) = key else { continue };
                    match sessions.get(id) {
                        Some(session) => {
                            index.insert(id.clone(), decode(session));
                        }
                        None => {
                            index.remove(id);
                        }
                    }
                }
                index
            }
            None => sessions.iter().map(|session| (session.id.clone(), decode(session))).collect(),
        };
        *memo = (Some(stamp), Rc::new(index));
        memo.1.clone()
    })
}

/// #EXP-1730: the synced messages that live in the sessions `ids` (their
/// own `session_id`), kept where `keep(session_id, message)` holds.
/// #LOGS-11: read through the store's per-session index, never a scan of
/// every synced message.
pub(crate) fn keyed_messages<'a>(
    cx: &App,
    ids: impl IntoIterator<Item = &'a str>,
    keep: impl Fn(&str, &MessageRow) -> bool,
) -> Vec<MessageRow> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let messages = store.collections().messages.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let keep = &keep;
    ids.into_iter()
        .filter(|id| seen.insert(id))
        .flat_map(|id| messages.in_group(id).filter(move |message| keep(id, message)))
        .cloned()
        .collect()
}

fn sort_messages(messages: &mut [MessageRow]) {
    messages.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// The messages of ONE session (its own conversation), oldest first.
/// #LOGS-7: read once per change of the messages collection (a session
/// view reads them every frame; a 5,000-message chat cloned and sorted
/// every body on every keystroke).
pub(crate) fn messages_of_session(session_id: &str, cx: &App) -> Rc<Vec<MessageRow>> {
    thread_local! {
        /// session id → (the collection and the session's group revision
        /// it was read at, the collection revision, its messages).
        static MEMO: std::cell::RefCell<HashMap<String, ((gpui::EntityId, u64), u64, Rc<Vec<MessageRow>>)>> = Default::default();
    }
    let Some(store) = Store::try_global(cx) else {
        return Rc::default();
    };
    let collection = &store.collections().messages;
    let messages = collection.read(cx);
    // #LOGS-11: per session — another chat's streaming no longer re-reads
    // (clones and sorts) this one.
    let stamp = (collection.entity_id(), messages.group_revision(session_id));
    let cached = MEMO.with(|memo| memo.borrow().get(session_id).cloned());
    if let Some((at, _, rows)) = &cached {
        if *at == stamp {
            return rows.clone();
        }
    }
    let order = |a: &MessageRow, b: &MessageRow| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id));
    // #LOGS-11: new rows at the end (a streaming reply) extend the last
    // read; anything else (an edit, a delete, an earlier row) reads afresh.
    let appended = cached.filter(|(at, _, _)| at.0 == stamp.0).and_then(|(_, revision, rows)| {
        let mut added: Vec<&MessageRow> = Vec::new();
        for key in messages.changed_since(revision)? {
            match messages.get_key(key) {
                Some(message) if message.session_id != session_id => continue,
                Some(message) if rows.last().is_none_or(|last| order(last, message).is_lt()) => added.push(message),
                // An earlier or edited row of this session, or a delete.
                _ => return None,
            }
        }
        added.sort_by(|a, b| order(a, b));
        let mut next = Vec::with_capacity(rows.len() + added.len());
        next.extend(rows.iter().cloned());
        next.extend(added.into_iter().cloned());
        Some(next)
    });
    let out = Rc::new(appended.unwrap_or_else(|| {
        // Sorted by reference, then cloned once in order (moving whole rows
        // through the sort cost as much as the clone).
        let mut rows: Vec<&MessageRow> = messages.in_group(session_id).collect();
        rows.sort_by(|a, b| order(a, b));
        rows.into_iter().cloned().collect()
    }));
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.len() > 256 {
            memo.clear();
        }
        memo.insert(session_id.to_string(), (stamp, messages.revision(), out.clone()));
    });
    out
}

/// EXP-1398 (ask-dock.json `sessionCardsCases`): THE card set of a
/// session's conversation as its rows (a surface card, a question / plan).
/// #LOGS-7: derived once per change of the messages collection.
pub(crate) fn conversation_cards(session_id: &str, cx: &App) -> Rc<Vec<SessionChatRow>> {
    thread_local! {
        static MEMO: std::cell::RefCell<HashMap<String, (Rc<Vec<MessageRow>>, Rc<Vec<SessionChatRow>>)>> = Default::default();
    }
    let messages = messages_of_session(session_id, cx);
    if let Some(hit) = MEMO.with(|memo| memo.borrow().get(session_id).filter(|(at, _)| Rc::ptr_eq(at, &messages)).map(|(_, cards)| cards.clone())) {
        return hit;
    }
    let by_id: HashMap<&str, &MessageRow> = messages.iter().map(|message| (message.id.as_str(), message)).collect();
    let cards: Rc<Vec<SessionChatRow>> = Rc::new(
        domain::ask_dock::session_cards(&messages)
            .iter()
            .filter_map(|card| {
                let message = by_id.get(card.id.as_str())?;
                if card.kind == domain::ask_dock::SessionCardKind::Surface {
                    domain::session_tree::surface_card_row(message)
                } else {
                    domain::session_tree::ask_card_row(message)
                }
            })
            .collect(),
    );
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.len() > 64 {
            memo.clear();
        }
        memo.insert(session_id.to_string(), (messages.clone(), cards.clone()));
    });
    cards
}

/// THE chat view of `root_id` (no focus, its own tree): the session
/// screen's model and a root view's suggested tasks read the same one.
/// #LOGS-7: derived once per change of what it reads (and per second: a
/// working session's card counts its elapsed seconds), never per frame or
/// twice per change.
pub(crate) fn root_chat_view(root_id: &str, now_ms: i64, cx: &App) -> Option<Rc<SessionChatView>> {
    thread_local! {
        static MEMO: std::cell::RefCell<HashMap<String, ChatViewMemo>> = Default::default();
    }
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let messages = collections.messages.read(cx);
    // #LOGS-11: the messages it read, by their groups' revisions (a message
    // anywhere else no longer re-derives a long chat); its tree moving is
    // the sessions revision.
    let messages_at = |groups: &[String]| {
        groups
            .iter()
            .map(|group| if group == ANY_GROUP { messages.revision() } else { messages.group_revision(group) })
            .max()
            .unwrap_or(0)
    };
    let at = |messages_at: u64| {
        (
            [
                collections.sessions.read(cx).revision(),
                collections.session_issues.read(cx).revision(),
                messages_at,
                collections.issue_relations.read(cx).revision(),
                collections.issues.read(cx).revision(),
                collections.pull_requests.read(cx).revision(),
            ],
            now_ms.div_euclid(CHAT_VIEW_CLOCK_MS),
        )
    };
    let mut stale: Option<ChatViewMemo> = None;
    if let Some(hit) = MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        let entry = memo.get_mut(root_id)?;
        let stamp = at(messages_at(&entry.groups));
        if entry.stamp == stamp {
            return Some(entry.view.clone());
        }
        // #LOGS-3: a sessions or pull requests delta that touches nothing
        // this chat reads (another chat's digest moving, a PR it names no
        // card of) keeps the view.
        if untouched(root_id, entry, &stamp, cx) {
            entry.stamp = stamp;
            return Some(entry.view.clone());
        }
        stale = Some(entry.clone());
        None
    }) {
        return hit;
    }
    // #LOGS-11: new agent replies at the chat's end append their rows to
    // the last view (a streaming reply re-derived the whole long chat).
    if let Some(entry) = stale.filter(|entry| {
        let now = at(messages_at(&entry.groups));
        let mut was = entry.stamp;
        was.0[2] = now.0[2];
        was == now
    }) {
        if let Some(appended) = appended_view(root_id, &entry, &messages) {
            let entry = ChatViewMemo { stamp: at(messages_at(&entry.groups)), ..appended };
            let view = entry.view.clone();
            MEMO.with(|memo| memo.borrow_mut().insert(root_id.to_string(), entry));
            return view;
        }
    }
    let view = chat_view(root_id, None, now_ms, false, false, cx).map(|(view, _)| Rc::new(view));
    let groups = chat_view_groups(root_id, cx);
    let stamp = at(messages_at(&groups));
    let tail = chat_tail(root_id, &groups, &messages);
    let refs = Rc::new(chat_refs(root_id, &groups, &messages, cx));
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.len() > 64 {
            memo.clear();
        }
        memo.insert(root_id.to_string(), ChatViewMemo { stamp, groups, view: view.clone(), tail, refs });
    });
    view
}

/// #LOGS-3: what of the sessions and pull requests a chat's derive reads,
/// beyond its own tree: every id its messages name (an author, any string
/// of a card: the sessions its cards and xrefs show, the PRs its event
/// lines number) and each named session's root; and the agent whose
/// conversation started the root (its origin).
#[derive(Default)]
struct ChatRefs {
    ids: HashSet<String>,
    origin_agent: Option<String>,
    /// The tree's rows as the derive read them, less their digests (the
    /// chat reads its own transcript, never its tree's digests: a reply
    /// streaming in rewrites the root's digest and nothing it shows).
    tree: HashMap<String, SessionRow>,
    /// A card of the chat names its root (its card reads the root's
    /// `updated_at`).
    root_named: bool,
}

fn digestless(row: &SessionRow) -> SessionRow {
    SessionRow { digest: None, ..row.clone() }
}

fn chat_refs(root_id: &str, groups: &[String], messages: &sync::Collection<MessageRow>, cx: &App) -> ChatRefs {
    fn strings(value: &serde_json::Value, out: &mut HashSet<String>) {
        match value {
            serde_json::Value::String(text) => {
                out.insert(text.clone());
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
            serde_json::Value::Object(fields) => fields.values().for_each(|field| strings(field, out)),
            _ => {}
        }
    }
    let mut ids: HashSet<String> = HashSet::new();
    for message in groups.iter().flat_map(|group| messages.in_group(group)) {
        ids.extend(message.author_session_id.clone());
        if let Some(card) = &message.card {
            strings(card, &mut ids);
        }
    }
    let root_named = ids.contains(root_id);
    ids.extend(groups.iter().cloned());
    ids.insert(root_id.to_string());
    let Some(store) = Store::try_global(cx) else {
        return ChatRefs { ids, root_named, ..ChatRefs::default() };
    };
    let sessions = store.collections().sessions.read(cx);
    let roots: Vec<String> = ids.iter().filter_map(|id| sessions.get(id)).map(|session| session.root_id_or_self().to_string()).collect();
    ids.extend(roots);
    let origin_agent = sessions.get(root_id).and_then(|root| root.started_by_agent_id.clone()).filter(|agent| !agent.is_empty());
    let tree = sessions
        .iter()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .map(|session| (session.id.clone(), digestless(session)))
        .collect();
    ChatRefs { ids, origin_agent, tree, root_named }
}

/// Whether the store moved since `entry` only in sessions and pull requests
/// rows its chat does not read ([`ChatRefs`]): no session of its tree (a
/// new one included), none it names, no conversation of its origin agent,
/// no PR it names. A delete it cannot place, or a log that does not reach
/// back far enough, reads as touched.
fn untouched(root_id: &str, entry: &ChatViewMemo, stamp: &([u64; 6], i64), cx: &App) -> bool {
    const SESSIONS: usize = 0;
    const PULLS: usize = 5;
    let (was, now) = (&entry.stamp, stamp);
    if was.1 != now.1 || (0..6).any(|i| i != SESSIONS && i != PULLS && was.0[i] != now.0[i]) {
        return false;
    }
    let Some(store) = Store::try_global(cx) else { return false };
    let collections = store.collections();
    let refs = &entry.refs;
    let id = |key: &sync::protocol::RowKey| match key {
        sync::protocol::RowKey::Single(id) => Some(id.clone()),
        _ => None,
    };
    if was.0[SESSIONS] != now.0[SESSIONS] {
        let sessions = collections.sessions.read(cx);
        let Some(keys) = sessions.changed_since(was.0[SESSIONS]) else { return false };
        for key in keys {
            let Some(id) = id(key) else { return false };
            let row = sessions.get(&id);
            // A tree row whose digest alone moved (its own reply streaming).
            if let (Some(row), Some(was)) = (row, refs.tree.get(&id)) {
                let mut now = digestless(row);
                // The digest write bumps `updated_at`, which the chat reads
                // only off a child that is not working (how long a finished
                // one stays nested, a done card's end) or a root its cards
                // name.
                let working = matches!(
                    domain::session_tree::session_state(row),
                    domain::session_tree::SessionState::Working | domain::session_tree::SessionState::NeedsYou
                );
                if (row.id == root_id && !refs.root_named) || working {
                    now.updated_at = was.updated_at.clone();
                }
                if now == *was {
                    continue;
                }
                return false;
            }
            if refs.ids.contains(&id) {
                return false;
            }
            if let Some(row) = row {
                let in_tree = row.id == root_id || row.root_id_or_self() == root_id;
                let origin = refs.origin_agent.is_some() && row.agent_id == refs.origin_agent;
                if in_tree || origin {
                    return false;
                }
            }
        }
    }
    if was.0[PULLS] != now.0[PULLS] {
        let pulls = collections.pull_requests.read(cx);
        let Some(keys) = pulls.changed_since(was.0[PULLS]) else { return false };
        if keys.into_iter().any(|key| id(key).is_none_or(|id| refs.ids.contains(&id))) {
            return false;
        }
    }
    true
}

/// [`root_chat_view`]'s memo of one root.
#[derive(Clone)]
struct ChatViewMemo {
    stamp: ([u64; 6], i64),
    /// The message groups it read (the tree's sessions and its ask
    /// mirrors' origins).
    groups: Vec<String>,
    view: Option<Rc<SessionChatView>>,
    tail: ChatTail,
    refs: Rc<ChatRefs>,
}

/// #LOGS-11: what an append needs of the messages a view was derived from.
#[derive(Clone)]
struct ChatTail {
    /// The messages collection's revision.
    revision: u64,
    /// The newest message's place ([`domain::session_tree::chat_order_key`]).
    last: (i64, String),
    /// A trigger firing waits after the last row (THE marker shows only
    /// right above an agent reply: an append would bring it back).
    fire_pending: bool,
}

fn chat_tail(root_id: &str, groups: &[String], messages: &sync::Collection<MessageRow>) -> ChatTail {
    let mut last: (i64, &str) = (i64::MIN, "");
    let mut last_row: (i64, &str) = (i64::MIN, "");
    let mut last_fire: (i64, &str) = (i64::MIN, "");
    for message in groups.iter().filter(|group| group.as_str() != ANY_GROUP).flat_map(|group| messages.in_group(group)) {
        let key = domain::session_tree::chat_order_key(message);
        last = last.max(key);
        let card = message.card.as_ref().filter(|card| !card.is_null());
        let fire = card.is_some_and(|card| card.get("kind").and_then(serde_json::Value::as_str) == Some(domain::contract::MESSAGE_CARD_KIND_EVENT) && card.get("event").and_then(serde_json::Value::as_str) == Some("trigger_fired"));
        if fire {
            last_fire = last_fire.max(key);
        } else if message.session_id == root_id && (card.is_some() || !message.body_text().trim().is_empty()) {
            last_row = last_row.max(key);
        }
    }
    ChatTail { revision: messages.revision(), last: (last.0, last.1.to_string()), fire_pending: last_fire > last_row }
}

/// The view `entry` holds with the rows of the agent replies written since
/// it was derived appended, when that is all that moved in its groups: each
/// a plain, unlocked agent message of the root itself (or a plain message of
/// another tree session, which draws nothing), newer than every message the
/// view read. Else `None` (the caller derives afresh).
fn appended_view(root_id: &str, entry: &ChatViewMemo, messages: &sync::Collection<MessageRow>) -> Option<ChatViewMemo> {
    if entry.tail.fire_pending || entry.groups.iter().any(|group| group == ANY_GROUP) {
        return None;
    }
    let view = entry.view.as_ref()?;
    let mut appended: Vec<&MessageRow> = Vec::new();
    for key in messages.changed_since(entry.tail.revision)? {
        // A delete: its group is unknown here.
        let message = messages.get_key(key)?;
        if !entry.groups.iter().any(|group| *group == message.session_id) {
            continue;
        }
        let card = message.card.as_ref().is_some_and(|card| !card.is_null());
        let plain_agent = !card
            && message.locked().is_none()
            && message.author_kind.as_deref() == Some(domain::contract::MESSAGE_AUTHOR_KIND_AGENT)
            && message.author_session_id.as_deref().is_none_or(str::is_empty);
        let key = domain::session_tree::chat_order_key(message);
        if !plain_agent || key <= (entry.tail.last.0, entry.tail.last.1.as_str()) {
            return None;
        }
        if message.session_id == root_id {
            appended.push(message);
        }
    }
    appended.sort_by(|a, b| domain::session_tree::chat_order_key(a).cmp(&domain::session_tree::chat_order_key(b)));
    let mut next = (**view).clone();
    next.rows.extend(appended.iter().filter_map(|message| domain::session_tree::plain_message_row(message, None)));
    let mut tail = ChatTail { revision: messages.revision(), ..entry.tail.clone() };
    for message in messages.changed_since(entry.tail.revision)?.into_iter().filter_map(|key| messages.get_key(key)) {
        if entry.groups.iter().any(|group| *group == message.session_id) {
            let key = domain::session_tree::chat_order_key(message);
            if key > (tail.last.0, tail.last.1.as_str()) {
                tail.last = (key.0, key.1.to_string());
            }
        }
    }
    Some(ChatViewMemo { stamp: entry.stamp, groups: entry.groups.clone(), view: Some(Rc::new(next)), tail, refs: entry.refs.clone() })
}

/// [`chat_view_groups`]' stand-in for every group: an origin not synced yet
/// may arrive in any.
const ANY_GROUP: &str = "*";

/// #LOGS-11: the message groups [`chat_view`] reads for `root_id`: its
/// tree's sessions and the sessions of the origin messages its `ask`
/// mirrors draw ([`ANY_GROUP`] while one has not synced).
fn chat_view_groups(root_id: &str, cx: &App) -> Vec<String> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let messages = collections.messages.read(cx);
    let mut groups: Vec<String> = collections
        .sessions
        .read(cx)
        .iter()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .map(|session| session.id.clone())
        .collect();
    let origins: Vec<String> = groups
        .iter()
        .flat_map(|group| messages.in_group(group))
        .filter_map(|message| {
            let card = message.card.as_ref()?;
            (card.get("kind")?.as_str()? == domain::contract::MESSAGE_CARD_KIND_ASK)
                .then(|| card.get("messageId")?.as_str())
                .flatten()
        })
        .map(|id| messages.get(id).map_or_else(|| ANY_GROUP.to_string(), |origin| origin.session_id.clone()))
        .collect();
    groups.extend(origins);
    groups.sort();
    groups.dedup();
    groups
}

/// #LOGS-3: how coarse a chat's derive reads the clock. Its only clock
/// inputs are a session card's elapsed time (painted live, see
/// [`live_card_elapsed`]) and the minutes a finished child stays nested; a
/// per-second key re-derived a long chat on every working tick (an 8,000-row
/// chat: ~40 ms each second while anything in it worked).
const CHAT_VIEW_CLOCK_MS: i64 = 10_000;

/// A session card's elapsed time as of now (session-card.json: working
/// counts to now, done to its end), read off the synced row; the derived
/// card's own when the row is gone.
pub(crate) fn live_card_elapsed(card: &domain::session_tree::SessionCard, cx: &App) -> Option<String> {
    let live = Store::try_global(cx).and_then(|store| {
        let sessions = store.collections().sessions.read(cx);
        sessions.get(&card.session_id).map(|row| domain::session_tree::session_card(row, &card.name, now_ms()).elapsed)
    });
    live.unwrap_or_else(|| card.elapsed.clone())
}

/// #EXP-1732: a root's suggested-tasks cards (its chat's `Suggestions`
/// rows), taken from [`root_chat_view`] once per view.
pub(crate) fn tree_suggestions(root_id: &str, now_ms: i64, cx: &App) -> Rc<Vec<SessionChatRow>> {
    thread_local! {
        static MEMO: std::cell::RefCell<HashMap<String, (Rc<SessionChatView>, Rc<Vec<SessionChatRow>>)>> = Default::default();
    }
    let Some(view) = root_chat_view(root_id, now_ms, cx) else {
        return Rc::default();
    };
    if let Some(hit) = MEMO.with(|memo| memo.borrow().get(root_id).filter(|(at, _)| Rc::ptr_eq(at, &view)).map(|(_, rows)| rows.clone())) {
        return hit;
    }
    let rows: Rc<Vec<SessionChatRow>> =
        Rc::new(view.rows.iter().filter(|row| matches!(row, SessionChatRow::Suggestions { .. })).cloned().collect());
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.len() > 64 {
            memo.clear();
        }
        memo.insert(root_id.to_string(), (view, rows.clone()));
    });
    rows
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
// The chat screen
// ---------------------------------------------------------------------------

/// The chat screen's model for `root_id` (`build_session_chat`): the
/// tabs, the conversation rows, the composer target; `None` while the root
/// has not synced. Works for an agent root too (its channel). #EXP-1380:
/// with the root's own messages (oldest first) off the SAME scan.
/// `whole_tree` (VAPP-110) = an app EDITOR's channel over the WHOLE tree's
/// messages (web `useTreeMessages`): the editor session's children (and a
/// pre-FEED-134 continuation child) post there too.
pub(crate) fn chat_view_and_messages(
    root_id: &str,
    focus: Option<&str>,
    now_ms: i64,
    whole_tree: bool,
    cx: &App,
) -> Option<(SessionChatView, Vec<MessageRow>)> {
    chat_view(root_id, focus, now_ms, whole_tree, true, cx)
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

fn chat_view(
    root_id: &str,
    focus: Option<&str>,
    now_ms: i64,
    whole_tree: bool,
    own: bool,
    cx: &App,
) -> Option<(SessionChatView, Vec<MessageRow>)> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let root = sessions.get(root_id)?;
    // The team's sessions, not just the tree: an agent channel's
    // `thread_started` (wire kind) cards name OTHER roots.
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
    // #EXP-1730: every message keyed by its own `session_id`.
    // EXP-1341: a card-less input names its session (first prompt); #EXP-1380:
    // another root's only as that one input.
    let all_messages = collections.messages.read(cx);
    // #LOGS-11: the tree's rows through the store's per-session index (was
    // one pass over every synced message per derive).
    let mut picked: Vec<&MessageRow> = Vec::new();
    let mut tree_order: Vec<&str> = tree_ids.iter().copied().filter(|id| *id != root_id).collect();
    tree_order.sort_unstable();
    for owner in std::iter::once(root_id).chain(tree_order) {
        for message in all_messages.in_group(owner) {
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
        }
    }
    // Each other team session's first prompt, off its digest (session
    // e4a04d22: never another chat's transcript).
    let firsts: Vec<MessageRow> = team_sessions
        .iter()
        .filter(|session| session.id != root_id && !tree_ids.contains(session.id.as_str()))
        .filter_map(|session| {
            // #LOGS-3: off the shared read-back (no digest decoded here).
            digest_of(&session.id, cx)
                .iter()
                .filter(|message| is_first_prompt_candidate(message, &session.id))
                .cloned()
                .min_by_key(|message| (message.created_at.as_deref().and_then(domain::session_tree::iso_ms).unwrap_or(0), message.id.clone()))
        })
        .collect();
    picked.extend(firsts.iter());
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
    // #LOGS-11: by reference (a clone of every picked row per derive).
    let keyed: Vec<&MessageRow> = picked.into_iter().filter(|message| domain::message_session::message_session(message).is_some()).collect();
    let messages: Vec<&MessageRow> = keyed.clone();
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
    let view = build_session_chat(
        SessionChatInput {
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
    if !own {
        return Some((view, Vec::new()));
    }
    // The root's own messages (an editor: the tree's), oldest first.
    let mut own: Vec<MessageRow> = keyed
        .iter()
        .filter(|message| {
            message.session_id == root_id || (whole_tree && tree_ids.contains(message.session_id.as_str()))
        })
        .map(|message| (*message).clone())
        .collect();
    sort_messages(&mut own);
    Some((view, own))
}

// ---------------------------------------------------------------------------
// The Chats page, the rail and the team picker
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
    // the chats it spawns are their own roots (the Chats entry lights
    // for them).
    let team_ids = synced_team_ids(cx);
    let team_ids: Vec<&str> = team_ids.iter().map(String::as_str).collect();
    list_agents(&agents, &rows, Some(team_id), &team_ids, me.as_deref())
}

/// The store state one [`chat_lists`] answer derives from.
#[derive(Clone, PartialEq, Eq)]
struct ListsKey {
    team_id: String,
    revisions: [u64; 4],
}

#[derive(Default)]
struct ListsMemo(Option<(ListsKey, Rc<Vec<ChatsListGroup>>)>);

impl gpui::Global for ListsMemo {}

/// The Chats page bands of `team_id` (`partition_chats`: Needs you ·
/// Running · Recent, empty bands omitted). Memoised per store change: the
/// rail's dot reads it on every render.
pub(crate) fn chat_lists(team_id: &str, cx: &mut App) -> Rc<Vec<ChatsListGroup>> {
    let Some(store) = Store::try_global(cx) else {
        return Rc::new(Vec::new());
    };
    let collections = store.collections();
    let key = ListsKey {
        team_id: team_id.to_string(),
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.session_issues.read(cx).revision(),
            collections.issues.read(cx).revision(),
            collections.session_reads.read(cx).revision(),
        ],
    };
    if let Some((cached, lists)) = &cx.default_global::<ListsMemo>().0 {
        if *cached == key {
            return lists.clone();
        }
    }
    let lists = Rc::new(derive_chat_lists(team_id, cx));
    cx.global_mut::<ListsMemo>().0 = Some((key, lists.clone()));
    lists
}

fn derive_chat_lists(team_id: &str, cx: &App) -> Vec<ChatsListGroup> {
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
    // EXP-1341: every session's own first prompt names it; session e4a04d22:
    // read off each conversation's digest, never the transcript.
    let keyed: Vec<MessageRow> = digest_messages(cx, |owner| in_team.contains(owner));
    let messages: Vec<&MessageRow> = keyed.iter().collect();
    let session_reads = collections.session_reads.read(cx);
    let reads: Vec<&SessionReadRow> = session_reads
        .iter()
        .filter(|read| in_team.contains(read.session_id.as_str()))
        .collect();
    partition_chats(ChatsListInput {
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
/// the view) and the agent rows.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChatsModel {
    pub(crate) list: domain::chats::ChatList,
    /// #EXP-1388: the Archived page's list (view `archived`).
    pub(crate) archived: domain::chats::ChatList,
    /// S-3: the AGENT rows pinned on top.
    pub(crate) agent_rows: Vec<domain::chats::AgentChatRow>,
    /// #EXP-1733: the APP rows between the agents and the sessions.
    pub(crate) app_rows: Vec<domain::chats::AppChatRow>,
}

#[derive(Clone, PartialEq, Eq)]
struct ChatsKey {
    team_id: String,
    revisions: [u64; 8],
    /// The time labels move with the local hour (today → Yesterday).
    hour: i64,
}

#[derive(Default)]
struct ChatsMemo(Option<(ChatsKey, Rc<ChatsModel>)>);

impl gpui::Global for ChatsMemo {}

/// [`ChatsModel`] of `team_id` (`domain::chats::chat_list`), memoised per
/// store change.
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
            collections.issues.read(cx).revision(),
            collections.session_reads.read(cx).revision(),
            collections.pull_requests.read(cx).revision(),
            collections.agents.read(cx).revision(),
            // #EXP-1714: the synced teams tell a global Assistant apart.
            collections.teams.read(cx).revision(),
            // #EXP-1733: the app rows.
            collections.apps.read(cx).revision(),
        ],
        hour: now_ms() / 3_600_000,
    };
    if let Some((cached, model)) = &cx.default_global::<ChatsMemo>().0 {
        if *cached == key {
            return model.clone();
        }
    }
    let groups = chat_lists(team_id, cx);
    let model = Rc::new(derive_chats_model(team_id, &groups, cx));
    cx.global_mut::<ChatsMemo>().0 = Some((key, model.clone()));
    model
}

fn derive_chats_model(team_id: &str, groups: &[ChatsListGroup], cx: &App) -> ChatsModel {
    let collections = Store::global(cx).collections();
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let agents = agents_in_team(team_id, cx);
    let team_ids = synced_team_ids(cx);
    let sessions = collections.sessions.read(cx);
    let team_sessions: Vec<&SessionRow> = sessions
        .iter()
        .filter(|session| in_team_or_personal(session, team_id, &team_ids, me.as_deref()))
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
                // #EXP-1733: + my app sessions (the app rows).
                .filter(|session| session.is_root() && (session.agent_id.is_some() || session.is_app_editor()))
                .map(|session| session.id.as_str()),
        )
        .collect();
    // Session e4a04d22: the digest's messages, never the transcript.
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
    let words: HashMap<String, domain::session_tree::ChatRowWord> = groups
        .iter()
        .flat_map(|group| group.rows.iter())
        .filter(|row| row.depth == 0)
        .filter_map(|row| {
            crate::chats_view::chat_word(&row.session_id, row.busy, row.needs_you, row.offline, &pulls)
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
            view,
            now: &now,
            utc_offset_minutes: offset,
            visible_session_ids: Some(&visible),
        })
    };
    let list = list_of(domain::chats::ChatListView::List);
    let archived = list_of(domain::chats::ChatListView::Archived);
    let agent_rows = domain::chats::agent_chat_rows(domain::chats::AgentChatRowsInput {
        agents: &agents,
        sessions: &team_sessions,
        messages: &messages,
        reads: &reads,
        me: me.as_deref(),
        view: domain::chats::ChatListView::List,
        now: &now,
        utc_offset_minutes: offset,
    });
    // #EXP-1733: the team's live apps (the sidebar's apps), my session each.
    let app_store = collections.apps.read(cx);
    let apps: Vec<&domain::rows::AppRow> = app_store
        .iter()
        .filter(|app| app.team_id.as_deref() == Some(team_id) && app.deleted_at.is_none())
        .collect();
    let app_rows = domain::chats::app_chat_rows(domain::chats::AppChatRowsInput {
        apps: &apps,
        sessions: &team_sessions,
        messages: &messages,
        reads: &reads,
        me: me.as_deref(),
        view: domain::chats::ChatListView::List,
        now: &now,
        utc_offset_minutes: offset,
    });
    ChatsModel { list, archived, agent_rows, app_rows }
}

/// EXP-1323: the Chats entry's unread dot — any chat of `team_id` has
/// an agent reply the viewer has not read (`chats_list_unread`).
/// #EXP-1388: an archived chat never lights it (`chats_unread`).
pub(crate) fn rail_chats_unread(team_id: &str, cx: &mut App) -> bool {
    let groups = chat_lists(team_id, cx);
    let Some(store) = Store::try_global(cx) else {
        return domain::session_tree::chats_list_unread(&groups);
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
    // #EXP-1730: a message's session is its own `session_id`; #LOGS-11: read
    // through the store's per-session index (was every synced message).
    let mut scope: Vec<&str> = vec![session_id];
    if whole_tree {
        scope.extend(sessions.iter().filter(|session| session.id != session_id && session.root_id_or_self() == session_id).map(|session| session.id.as_str()));
    }
    let messages = collections.messages.read(cx);
    let newest = scope
        .into_iter()
        .flat_map(|owner| messages.in_group(owner))
        .filter(|message| message.author_kind.as_deref() == Some(domain::contract::MESSAGE_AUTHOR_KIND_AGENT))
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

/// #LOGS-11: the ids of `root_id`'s tree (itself included), read once per
/// change of the sessions collection.
fn tree_session_ids(root_id: &str, cx: &App) -> Rc<Vec<String>> {
    thread_local! {
        static MEMO: std::cell::RefCell<(u64, HashMap<String, Rc<Vec<String>>>)> = Default::default();
    }
    let Some(store) = Store::try_global(cx) else {
        return Rc::default();
    };
    let sessions = store.collections().sessions.read(cx);
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.0 != sessions.revision() {
            *memo = (sessions.revision(), HashMap::new());
        }
        memo.1
            .entry(root_id.to_string())
            .or_insert_with(|| {
                let mut ids: Vec<String> = sessions.iter().filter(|session| session.id == root_id || session.root_id_or_self() == root_id).map(|session| session.id.clone()).collect();
                if !ids.iter().any(|id| id == root_id) {
                    ids.push(root_id.to_string());
                }
                Rc::new(ids)
            })
            .clone()
    })
}

/// The marks already sent this run (session id → the newest message they
/// cleared), so a render loop never re-sends one before it syncs back.
#[derive(Default)]
struct SentReads(HashMap<String, i64>);

impl gpui::Global for SentReads {}

/// The collection revisions each session's [`unread_marker`] scan last ran
/// at (`messages`, `session_reads`, `sessions` for a whole tree): the
/// screens call [`mark_read_if_unread`] every frame, and the scan walks every
/// synced message — so a frame in which nothing synced skips it.
#[derive(Default)]
struct ReadScans(HashMap<String, (u64, u64, u64)>);

impl gpui::Global for ReadScans {}

/// EXP-1323: `sessions.markRead` once `session_id` shows news the viewer
/// has not read ([`unread_marker`]): the chat (`whole_tree`, a root) or a
/// session view. A background write: a failure only logs (never a toast).
pub(crate) fn mark_read_if_unread(session_id: &str, whole_tree: bool, cx: &mut App) {
    let Some(store) = Store::try_global(cx) else {
        return;
    };
    let collections = store.collections();
    // #LOGS-11: the scanned sessions' own messages, not every synced one (a
    // busy team's every message re-scanned the open chat).
    let messages_at = {
        let messages = collections.messages.read(cx);
        if whole_tree {
            tree_session_ids(session_id, cx).iter().map(|id| messages.group_revision(id)).max().unwrap_or(0)
        } else {
            messages.group_revision(session_id)
        }
    };
    let revisions = (
        messages_at,
        collections.session_reads.read(cx).revision(),
        if whole_tree { collections.sessions.read(cx).revision() } else { 0 },
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
                log::warn!("[chats] mark {session_id} read failed: {err}");
            }
        })
        .detach();
}

/// The rail Chats entry's dot: `(needs_you, running)` — some chat of
/// `team_id` waits on a person / is queued or running.
pub(crate) fn rail_chats_badge(team_id: &str, cx: &mut App) -> (bool, bool) {
    chats_badge_of(&chat_lists(team_id, cx))
}

fn chats_badge_of(groups: &[ChatsListGroup]) -> (bool, bool) {
    let has = |key: ChatsListKey| groups.iter().any(|group| group.key == key && !group.rows.is_empty());
    (has(ChatsListKey::NeedsYou), has(ChatsListKey::Running))
}

/// EXP-1075 / EXP-1294 — one team's tally of MY live chats: how many are
/// working, and whether any waits on me (the team switcher's dot turns amber).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TeamLiveRuns {
    pub(crate) count: usize,
    pub(crate) needs_input: bool,
}

/// The chat roots whose tree holds a session I started (`user_id` = the
/// starter) that is working ([`session_is_working`]) or waits on a person,
/// grouped by root id. The ONE liveness rule the rail's Running section and
/// the team switcher's dot share.
fn my_live_trees<'a>(sessions: &[&'a SessionRow], me: &str) -> BTreeMap<&'a str, Vec<&'a SessionRow>> {
    let chat_roots: HashMap<&str, &SessionRow> = sessions
        .iter()
        .copied()
        // Release train 2026-10-23 (VAPP-110): a working app editor is not a
        // chat, so it stays out of Running and the switcher's dot.
        .filter(|session| session.is_root() && session.is_chat() && !session.is_app_editor())
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
        if chat_roots.contains_key(root_id) {
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

/// The team switcher's dot: per team, my live chats
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

/// One row of the rail's Active band (EXP-1318): a chat the shapes make
/// visible that needs you, runs, or still asks for a merge — the mark and
/// the title, no identifier, no caption. #EXP-1461: a ROOT only (its
/// tree's state rolled up; no nested children, no fold row).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveRootRow {
    pub(crate) root_id: String,
    /// EXP-1323: a bold title.
    pub(crate) unread: bool,
    pub(crate) team_id: String,
    pub(crate) title: String,
    pub(crate) busy: bool,
    pub(crate) needs_you: bool,
    /// EXP-1318: the root session's device (the trailing device icon).
    pub(crate) device_id: Option<String>,
    /// Every session of the tree — the row stays lit while one is on screen.
    pub(crate) session_ids: Vec<String>,
}

/// Pure core of [`live_roots_for_rail`]: the contract's `railChats` over
/// the Chats bands (`groups`, every visible chat, not only mine), the
/// words off the team's pull requests; group order.
pub(crate) fn live_roots_from(
    groups: &[ChatsListGroup],
    sessions: &[&SessionRow],
    team_pulls: &[PullRequestRow],
) -> Vec<LiveRootRow> {
    let mut trees: Option<HashMap<&str, Vec<&SessionRow>>> = None;
    domain::session_tree::rail_chats(groups, |row| {
        crate::chats_view::chat_word(&row.session_id, row.busy, row.needs_you, row.offline, team_pulls)
    })
    .into_iter()
    .map(|row| {
        // #LOGS-11: the trees grouped once, not one scan per row.
        let trees = trees.get_or_insert_with(|| {
            let mut trees: HashMap<&str, Vec<&SessionRow>> = HashMap::new();
            for session in sessions.iter().copied() {
                trees.entry(session.root_id_or_self()).or_default().push(session);
                if session.root_id_or_self() != session.id {
                    trees.entry(session.id.as_str()).or_default().push(session);
                }
            }
            trees
        });
        let tree: Vec<&SessionRow> = trees.get(row.session_id.as_str()).cloned().unwrap_or_default();
        LiveRootRow {
            session_ids: tree_order(&tree).iter().map(|session| session.id.clone()).collect(),
            root_id: row.session_id,
            unread: row.unread,
            team_id: row.team_id,
            title: row.title,
            busy: row.busy,
            needs_you: row.needs_you,
            device_id: row.device_id,
        }
    })
    .collect()
}

/// The rail's Active band for `team_id` ([`live_roots_from`]).
pub(crate) fn live_roots_for_rail(team_id: &str, cx: &mut App) -> Vec<LiveRootRow> {
    let groups = chat_lists(team_id, cx);
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
    let mut subscriptions = observe_session_rows(cx);
    if let Some(store) = Store::try_global(cx) {
        let messages = store.collections().messages.clone();
        subscriptions.push(cx.observe(&messages, |_, _, cx| cx.notify()));
    }
    subscriptions
}

/// [`observe_session_collections`] without the transcripts (`messages`):
/// for a view that reads only the rows and their digests (#LOGS-11: the
/// rail, which a busy account's streaming messages re-rendered on every
/// delta).
pub(crate) fn observe_session_rows<V: 'static>(cx: &mut gpui::Context<V>) -> Vec<gpui::Subscription> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    vec![
        cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
        cx.observe(&collections.session_issues, |_, _, cx| cx.notify()),
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

    /// #LOGS-11: a chat's view and messages after agent replies stream in
    /// (the incremental append) equal a fresh read of the same store; an
    /// edit reads afresh.
    #[gpui::test]
    fn streamed_replies_append_to_the_chat_as_a_fresh_read_would(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        let message = |id: &str, kind: &str, body: &str, at: &str| -> MessageRow {
            serde_json::from_value(json!({"id": id, "session_id": "r1", "author_kind": kind, "body": body, "created_at": at})).unwrap()
        };
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            let collections = Store::global(cx).collections().clone();
            let root: SessionRow = serde_json::from_value(json!({"id": "r1", "team_id": "t1", "root_id": "r1", "status": "running", "shared": true, "created_at": "2026-11-01T09:00:00.000Z"})).unwrap();
            collections.sessions.update(cx, |c, cx| {
                c.seed(root.key(), root);
                cx.notify();
            });
            let seed = |rows: Vec<MessageRow>, cx: &mut App| {
                collections.messages.update(cx, |c, cx| {
                    for row in rows {
                        c.seed(row.key(), row);
                    }
                    cx.notify();
                })
            };
            seed(vec![message("m1", "user", "Fix the list", "2026-11-01T09:00:01.000Z"), message("m2", "agent", "On it", "2026-11-01T09:00:02.000Z")], cx);
            let now = 1_793_000_000_000;
            let fresh = |cx: &App| chat_view(root_id(), None, now, false, false, cx).map(|(view, _)| view);
            fn root_id() -> &'static str {
                "r1"
            }
            assert_eq!(root_chat_view("r1", now, cx).as_deref(), fresh(cx).as_ref());
            assert_eq!(messages_of_session("r1", cx).len(), 2);
            // Two replies stream in, one empty: appended.
            seed(vec![message("m3", "agent", "Found it", "2026-11-01T09:00:03.000Z"), message("m4", "agent", " ", "2026-11-01T09:00:04.000Z")], cx);
            seed(vec![message("m5", "agent", "Pushed", "2026-11-01T09:00:05.000Z")], cx);
            assert_eq!(root_chat_view("r1", now, cx).as_deref(), fresh(cx).as_ref());
            let ids: Vec<String> = messages_of_session("r1", cx).iter().map(|m| m.id.clone()).collect();
            assert_eq!(ids, ["m1", "m2", "m3", "m4", "m5"]);
            // A person's message and an edit of an earlier row: afresh.
            seed(vec![message("m6", "user", "Thanks", "2026-11-01T09:00:06.000Z"), message("m2", "agent", "On it, edited", "2026-11-01T09:00:02.000Z")], cx);
            assert_eq!(root_chat_view("r1", now, cx).as_deref(), fresh(cx).as_ref());
            let bodies: Vec<String> = messages_of_session("r1", cx).iter().map(|m| m.body.clone().unwrap_or_default()).collect();
            assert_eq!(bodies[1], "On it, edited");
            assert_eq!(bodies.len(), 6);
        });
    }

    /// #LOGS-3: a working session's second ticks re-render its chat; within
    /// one clock bucket they reuse the derive (no re-read of the chat), and
    /// the card's elapsed time still follows the clock (painted live).
    #[gpui::test]
    fn a_working_tick_reuses_the_chat_and_paints_the_elapsed_live(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            let collections = Store::global(cx).collections().clone();
            let started = "2026-11-01T09:00:00.000Z";
            let rows: Vec<SessionRow> = vec![
                row(json!({"id": "r1", "team_id": "t1", "root_id": "r1", "status": "running", "busy": true, "shared": true, "started_at": started, "created_at": started})),
                row(json!({"id": "c1", "team_id": "t1", "root_id": "r1", "parent_id": "r1", "depth": 1, "status": "running", "busy": true, "shared": true, "started_at": started, "created_at": started})),
            ];
            collections.sessions.update(cx, |c, cx| {
                for row in rows {
                    c.seed(row.key(), row);
                }
                cx.notify();
            });
            let start = domain::session_tree::iso_ms(started).unwrap();
            let bucket = start - start.rem_euclid(CHAT_VIEW_CLOCK_MS) + CHAT_VIEW_CLOCK_MS;
            let first = root_chat_view("r1", bucket + 1_000, cx).expect("the chat");
            let ticked = root_chat_view("r1", bucket + 2_000, cx).expect("the chat");
            assert!(Rc::ptr_eq(&first, &ticked), "a second's tick re-derived the chat");
            let next = root_chat_view("r1", bucket + CHAT_VIEW_CLOCK_MS + 1_000, cx).expect("the chat");
            assert!(!Rc::ptr_eq(&first, &next), "the next bucket re-reads the clock");
            // The card as the derive left it (its clock long past).
            let child = collections.sessions.read(cx).get("c1").cloned().expect("the child");
            let card = domain::session_tree::session_card(&child, "Child", bucket);
            let before = now_ms();
            let live = live_card_elapsed(&card, cx).expect("a working card counts");
            let after = now_ms();
            let at = |now: i64| domain::session_tree::format_elapsed(now - start);
            assert!(live == at(before) || live == at(after), "{live} is not the live elapsed time");
        });
    }

    /// #LOGS-3: deltas a chat reads nothing of keep its derive (another
    /// chat's session or digest, a PR no card names, its own root's digest
    /// as a reply streams in); what it shows re-derives it (a tree session's
    /// state, a PR a card numbers, a session a card names). Each answer
    /// equals a fresh read of the same store.
    #[gpui::test]
    fn a_chat_rederives_only_on_what_it_reads(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            let c = Store::global(cx).collections().clone();
            let sessions = |rows: Vec<serde_json::Value>, cx: &mut App| {
                c.sessions.update(cx, |c, cx| {
                    for value in rows {
                        let row = row(value);
                        c.seed(row.key(), row);
                    }
                    cx.notify();
                })
            };
            let pulls = |rows: Vec<serde_json::Value>, cx: &mut App| {
                c.pull_requests.update(cx, |c, cx| {
                    for value in rows {
                        let row: PullRequestRow = serde_json::from_value(value).unwrap();
                        c.seed(row.key(), row);
                    }
                    cx.notify();
                })
            };
            let at = "2026-11-01T09:00:00.000Z";
            let digest = |n: u32| json!({"first": {"id": format!("d{n}"), "authorKind": "user", "body": format!("Prompt {n}"), "createdAt": at}});
            sessions(vec![
                json!({"id": "r1", "team_id": "t1", "root_id": "r1", "status": "idle", "shared": true, "title": "Mine", "created_at": at, "updated_at": at, "digest": digest(1)}),
                json!({"id": "c1", "team_id": "t1", "root_id": "r1", "parent_id": "r1", "depth": 1, "status": "running", "busy": true, "shared": true, "created_at": at, "updated_at": at}),
                json!({"id": "named", "team_id": "t1", "root_id": "named", "status": "idle", "shared": true, "title": "Named", "created_at": at, "updated_at": at}),
                json!({"id": "other", "team_id": "t1", "root_id": "other", "status": "running", "busy": true, "shared": true, "created_at": at, "updated_at": at, "digest": digest(2)}),
            ], cx);
            pulls(vec![
                json!({"id": "pr-named", "team_id": "t1", "root_session_id": "r1", "number": 7, "state": "open", "created_at": at}),
                json!({"id": "pr-other", "team_id": "t1", "root_session_id": "other", "number": 8, "state": "open", "created_at": at}),
            ], cx);
            c.messages.update(cx, |m, cx| {
                for value in [
                    json!({"id": "m1", "session_id": "r1", "author_kind": "user", "body": "Go", "created_at": at}),
                    json!({"id": "m2", "session_id": "r1", "author_kind": "agent", "body": "", "card": {"kind": "session", "sessionId": "named"}, "created_at": "2026-11-01T09:00:01.000Z"}),
                    json!({"id": "m3", "session_id": "r1", "author_kind": "agent", "body": "", "card": {"kind": "event", "pullRequestId": "pr-named", "text": "Opened {pr}"}, "created_at": "2026-11-01T09:00:02.000Z"}),
                ] {
                    let row: MessageRow = serde_json::from_value(value).unwrap();
                    m.seed(row.key(), row);
                }
                cx.notify();
            });
            let now = domain::session_tree::iso_ms(at).unwrap() + 1_000;
            let fresh = |cx: &App| chat_view("r1", None, now, false, false, cx).map(|(view, _)| view);
            let first = root_chat_view("r1", now, cx).expect("the chat");
            assert_eq!(Some(&*first), fresh(cx).as_ref());
            let kept = |cx: &mut App, what: &str| {
                let view = root_chat_view("r1", now, cx).expect("the chat");
                assert!(Rc::ptr_eq(&first, &view), "{what} re-derived the chat");
                assert_eq!(Some(&*view), fresh(cx).as_ref(), "{what}");
            };
            // Another chat streams (its row and digest), a PR no card names moves.
            sessions(vec![json!({"id": "other", "team_id": "t1", "root_id": "other", "status": "running", "busy": true, "shared": true, "created_at": at, "updated_at": "2026-11-01T09:05:00.000Z", "digest": digest(3)})], cx);
            pulls(vec![json!({"id": "pr-other", "team_id": "t1", "root_session_id": "other", "number": 8, "state": "merged", "created_at": at})], cx);
            kept(cx, "another chat");
            // Its own root's digest (and the bump it brings) as a reply lands.
            sessions(vec![json!({"id": "r1", "team_id": "t1", "root_id": "r1", "status": "idle", "shared": true, "title": "Mine", "created_at": at, "updated_at": "2026-11-01T09:06:00.000Z", "digest": digest(4)})], cx);
            kept(cx, "its own digest");
            // What it shows: a card's session, then a tree session's state.
            let rederived = |cx: &mut App, what: &str, was: &Rc<SessionChatView>| {
                let view = root_chat_view("r1", now, cx).expect("the chat");
                assert!(!Rc::ptr_eq(was, &view), "{what} kept a stale chat");
                assert_eq!(Some(&*view), fresh(cx).as_ref(), "{what}");
                view
            };
            sessions(vec![json!({"id": "named", "team_id": "t1", "root_id": "named", "status": "idle", "shared": true, "title": "Renamed", "created_at": at, "updated_at": at})], cx);
            let second = rederived(cx, "a named session", &first);
            pulls(vec![json!({"id": "pr-named", "team_id": "t1", "root_session_id": "r1", "number": 9, "state": "open", "created_at": at})], cx);
            let third = rederived(cx, "a numbered PR", &second);
            sessions(vec![json!({"id": "c1", "team_id": "t1", "root_id": "r1", "parent_id": "r1", "depth": 1, "status": "idle", "busy": false, "shared": true, "created_at": at, "updated_at": at})], cx);
            rederived(cx, "a tree session's state", &third);
        });
    }

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

    /// The switcher counts a chat ONCE however many of my sessions run in
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
    /// still asking for a merge, every visible chat (not only mine).
    #[test]
    fn the_rail_lists_needs_you_running_and_ready_to_merge() {
        let band = |key: ChatsListKey, ids: &[&str]| ChatsListGroup {
            key,
            title: key.title().to_string(),
            rows: ids
                .iter()
                .map(|id| domain::session_tree::ChatsListRow {
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
                    offline: false,
                })
                .collect(),
        };
        let groups = [band(ChatsListKey::Running, &["r1"]), band(ChatsListKey::Recent, &["r2", "r3"])];
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
        let band = |key: ChatsListKey| ChatsListGroup {
            key,
            title: key.title().to_string(),
            rows: vec![domain::session_tree::ChatsListRow {
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
                offline: false,
            }],
        };
        assert_eq!(chats_badge_of(&[]), (false, false));
        assert_eq!(chats_badge_of(&[band(ChatsListKey::Recent)]), (false, false));
        assert_eq!(chats_badge_of(&[band(ChatsListKey::Running)]), (false, true));
        assert_eq!(
            chats_badge_of(&[band(ChatsListKey::NeedsYou), band(ChatsListKey::Running)]),
            (true, true)
        );
    }

}
