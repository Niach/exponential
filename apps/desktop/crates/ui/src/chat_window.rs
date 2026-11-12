//! #EXP-1380 (chats.json WINDOW) on the desktop, as the phones do it: a chat
//! (a root's tree) reads its NEWEST [`chat_message_limit`] messages plus
//! every waiting card its digests name (`waiting`, `waits[]`) however old,
//! never the whole transcript. The page is read from the account's SQLite
//! OFF the main thread ([`sync::message_pages`], the `root_id` index);
//! `Load earlier` widens it by `transcript.windowStep` ([`load_earlier`])
//! and re-reads. Between reads the page follows the messages collection's
//! change log ([`sync::Collection::changed_since`]): a live row of the tree
//! lands at once, an edit / re-opened seal replaces the page's copy, a
//! delete inside the page re-reads it. The Results tab reads its candidates
//! the same way over the WHOLE chat ([`result_cards`], results-tab.json).
//!
//! Without a store on disk (headless tests) the page is cut from the
//! collection itself, synchronously.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, EntityId};
use sync::protocol::RowKey;
use sync::store::ShapeStore;
use sync::Store;

use domain::rows::{MessageRow, SessionRow};

/// The newest messages a chat loads at first (Android
/// `Chats.CHAT_MESSAGE_LIMIT`, web `CHAT_MESSAGE_LIMIT`).
pub(crate) fn chat_message_limit() -> usize {
    let transcript = &domain::chats::CHATS.transcript;
    transcript.window + transcript.window_step
}

/// A chat's loaded messages ([`chat_messages`], [`result_cards`]).
#[derive(Clone, Default)]
pub(crate) struct ChatPage {
    /// Oldest first.
    pub rows: Rc<Vec<MessageRow>>,
    /// Older messages exist unloaded.
    pub more: bool,
    /// Moves whenever `rows` does (memo stamps).
    pub version: u64,
    /// A read landed (before: the first one is in flight, the chat shows
    /// its skeleton like the phones' `isReady`).
    pub ready: bool,
}

/// The chat `root_id`'s newest page (plus its waiting cards and the live
/// rows since), oldest first. Empty until the first read lands.
pub(crate) fn chat_messages(root_id: &str, cx: &App) -> ChatPage {
    read(Query::Newest, root_id, cx)
}

/// Every result candidate of `root_id`'s tree (session-authored card rows),
/// the WHOLE chat, oldest first.
pub(crate) fn result_cards(root_id: &str, cx: &App) -> ChatPage {
    read(Query::Results, root_id, cx)
}

/// `Load earlier` (scroll to the top): one `windowStep` more of the chat,
/// when older messages exist. True = widened (a re-read follows).
pub(crate) fn load_earlier(root_id: &str, cx: &App) -> bool {
    let widened = ENTRIES.with(|entries| {
        let mut entries = entries.borrow_mut();
        let Some(entry) = entries.get_mut(&(Query::Newest, root_id.to_string())) else { return false };
        if !entry.live.as_ref().is_some_and(|live| live.newest >= live.limit && live.limit >= entry.limit) {
            return false;
        }
        entry.limit = widened_limit(entry.limit);
        true
    });
    if widened {
        // The next read sees the wider limit.
        read(Query::Newest, root_id, cx);
    }
    widened
}

/// One `windowStep` more than `limit`.
pub(crate) fn widened_limit(limit: usize) -> usize {
    limit + domain::chats::CHATS.transcript.window_step
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Query {
    Newest,
    Results,
}

/// The rows one read loaded, kept current by the change log.
struct Live {
    /// The limit it was read with (Results: none).
    limit: usize,
    /// The store it came from (`None` = cut from the collection).
    source: Option<usize>,
    rows: HashMap<String, MessageRow>,
    /// How many rows the newest read returned.
    newest: usize,
    /// The messages collection (entity, revision) `rows` is current to.
    at: (EntityId, u64),
    /// The change log could not tell (or a row of the page went): read
    /// again, keep answering with these meanwhile.
    stale: bool,
    /// Bumps with every change of `rows`.
    rows_version: u64,
    /// The answer, and what it was cut from (`rows_version`, the sessions
    /// revision: the waiting cards).
    out: Rc<Vec<MessageRow>>,
    out_at: Option<(u64, u64)>,
    version: u64,
}

struct Entry {
    limit: usize,
    live: Option<Live>,
    /// The read in flight.
    loading: Option<u64>,
    generation: u64,
}

thread_local! {
    static ENTRIES: RefCell<HashMap<(Query, String), Entry>> = Default::default();
    static VERSIONS: Cell<u64> = const { Cell::new(0) };
}

fn next_version() -> u64 {
    VERSIONS.with(|versions| {
        versions.set(versions.get() + 1);
        versions.get()
    })
}

/// The active account's store on disk.
fn active_store(store: &Store, cx: &App) -> Option<Arc<ShapeStore>> {
    let account_id = store.session(cx).account_id()?.to_string();
    store.manager().store(&account_id)
}

/// Whether `message` lives in `root_id`'s tree.
fn in_tree(message: &MessageRow, root_id: &str, sessions: &sync::Collection<SessionRow>) -> bool {
    message.root_id.as_deref() == Some(root_id)
        || message.session_id == root_id
        || sessions.get(&message.session_id).is_some_and(|session| session.root_id_or_self() == root_id)
}

/// A result candidate: a session posted it, with a card.
fn is_candidate(message: &MessageRow) -> bool {
    message.author_session_id.as_deref().is_some_and(|author| !author.is_empty())
        && message.card.as_ref().is_some_and(|card| !card.is_null())
}

fn order(a: &MessageRow, b: &MessageRow) -> std::cmp::Ordering {
    a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id))
}

/// The waiting cards `root_id`'s tree's digests name (`waiting`, `waits`).
fn waiting_ids(root_id: &str, sessions: &sync::Collection<SessionRow>) -> Vec<String> {
    let mut ids: Vec<String> = sessions
        .iter()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .filter_map(|session| domain::chats::ConversationDigest::decode(session.digest.as_ref()))
        .flat_map(|digest| digest.waiting.into_iter().chain(digest.waits).map(|message| message.id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Writes the rows `keys` name (the collection's copies) into `live`.
/// True = a row of it moved.
fn follow(live: &mut Live, query: Query, root_id: &str, keys: Vec<&RowKey>, messages: &sync::Collection<MessageRow>, sessions: &sync::Collection<SessionRow>) -> bool {
    let mut moved = false;
    for key in keys {
        let RowKey::Single(id) = key else { continue };
        match messages.get_key(key) {
            Some(row) if in_tree(row, root_id, sessions) && (query == Query::Newest || is_candidate(row)) => {
                live.rows.insert(id.clone(), row.clone());
                moved = true;
            }
            _ => {
                if live.rows.remove(id).is_some() {
                    moved = true;
                    // The page is one short: the next older row is unread.
                    if query == Query::Newest && live.newest >= live.limit {
                        live.stale = true;
                    }
                }
            }
        }
    }
    moved
}

/// The answer of `rows`: the newest `limit` (Results: all) plus the
/// `waiting` cards, oldest first; a waiting card the page does not hold
/// comes from `extra`.
fn cut<'a>(rows: impl Iterator<Item = &'a MessageRow>, limit: Option<usize>, waiting: &[String], extra: impl Fn(&str) -> Option<MessageRow>) -> Vec<MessageRow> {
    let mut sorted: Vec<&MessageRow> = rows.collect();
    sorted.sort_by(|a, b| order(a, b));
    let from = limit.map_or(0, |limit| sorted.len().saturating_sub(limit));
    let waiting_set: HashSet<&str> = waiting.iter().map(String::as_str).collect();
    let held: HashSet<&str> = sorted.iter().map(|row| row.id.as_str()).collect();
    let mut out: Vec<MessageRow> = sorted
        .iter()
        .enumerate()
        .filter(|(index, row)| *index >= from || waiting_set.contains(row.id.as_str()))
        .map(|(_, row)| (*row).clone())
        .collect();
    let mut added = false;
    for id in waiting.iter().filter(|id| !held.contains(id.as_str())) {
        if let Some(row) = extra(id) {
            out.push(row);
            added = true;
        }
    }
    if added {
        out.sort_by(order);
    }
    out
}

fn read(query: Query, root_id: &str, cx: &App) -> ChatPage {
    let Some(store) = Store::try_global(cx) else {
        return ChatPage::default();
    };
    let collections = store.collections();
    let collection = &collections.messages;
    let messages = collection.read(cx);
    let sessions = collections.sessions.read(cx);
    let sqlite = active_store(store, cx);
    let source = sqlite.as_ref().map(|sqlite| Arc::as_ptr(sqlite) as usize);
    let now = (collection.entity_id(), messages.revision());
    ENTRIES.with(|entries| {
        let mut entries = entries.borrow_mut();
        if entries.len() > 64 && !entries.contains_key(&(query, root_id.to_string())) {
            entries.retain(|_, entry| entry.loading.is_some());
        }
        let entry = entries
            .entry((query, root_id.to_string()))
            .or_insert_with(|| Entry { limit: chat_message_limit(), live: None, loading: None, generation: 0 });
        // Follow the change log since the rows were current.
        if let Some(live) = entry.live.as_mut().filter(|live| live.at != now && !live.stale) {
            match (live.at.0 == now.0).then(|| messages.changed_since(live.at.1)).flatten() {
                Some(keys) => {
                    if follow(live, query, root_id, keys, &messages, &sessions) {
                        live.rows_version = next_version();
                    }
                    live.at = now;
                }
                None => live.stale = true,
            }
        }
        let current = entry
            .live
            .as_ref()
            .is_some_and(|live| !live.stale && live.source == source && (query == Query::Results || live.limit == entry.limit));
        if !current {
            match sqlite {
                // Headless: cut from the collection.
                None => entry.live = Some(from_collection(query, root_id, entry.limit, now, &messages, &sessions)),
                Some(sqlite) if entry.loading.is_none() => {
                    entry.generation += 1;
                    entry.loading = Some(entry.generation);
                    let named = if query == Query::Newest { waiting_ids(root_id, &sessions) } else { Vec::new() };
                    spawn_read(query, root_id, entry.limit, named, sqlite, source, now, entry.generation, cx);
                }
                Some(_) => {}
            }
        }
        let Some(live) = entry.live.as_mut() else {
            return ChatPage::default();
        };
        let at = (live.rows_version, sessions.revision());
        if live.out_at != Some(at) {
            let (limit, waiting) = match query {
                Query::Newest => (Some(live.limit), waiting_ids(root_id, &sessions)),
                Query::Results => (None, Vec::new()),
            };
            let out = cut(live.rows.values(), limit, &waiting, |id| messages.get(id).cloned());
            if *live.out != out || live.version == 0 {
                live.out = Rc::new(out);
                live.version = next_version();
            }
            live.out_at = Some(at);
        }
        ChatPage { rows: live.out.clone(), more: query == Query::Newest && live.newest >= live.limit, version: live.version, ready: true }
    })
}

/// The headless page: the tree's rows of the collection itself.
fn from_collection(query: Query, root_id: &str, limit: usize, now: (EntityId, u64), messages: &sync::Collection<MessageRow>, sessions: &sync::Collection<SessionRow>) -> Live {
    let mut tree: Vec<&str> = sessions
        .iter()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .map(|session| session.id.as_str())
        .collect();
    if !tree.contains(&root_id) {
        tree.push(root_id);
    }
    let mut rows: Vec<&MessageRow> = tree
        .iter()
        .flat_map(|id| messages.in_group(id))
        .filter(|row| query == Query::Newest || is_candidate(row))
        .collect();
    rows.sort_by(|a, b| order(a, b));
    let (rows, newest) = match query {
        Query::Newest => {
            let from = rows.len().saturating_sub(limit);
            let newest = rows.len() - from;
            (rows.split_off(from), newest)
        }
        Query::Results => (rows, 0),
    };
    Live {
        limit,
        source: None,
        rows: rows.into_iter().map(|row| (row.id.clone(), row.clone())).collect(),
        newest,
        at: now,
        stale: false,
        rows_version: next_version(),
        out: Rc::default(),
        out_at: None,
        version: 0,
    }
}

/// Reads the page on the background executor, then installs it and wakes
/// the messages observers (the chat re-derives off its new version).
#[allow(clippy::too_many_arguments)]
fn spawn_read(query: Query, root_id: &str, limit: usize, named: Vec<String>, sqlite: Arc<ShapeStore>, source: Option<usize>, at: (EntityId, u64), generation: u64, cx: &App) {
    let root = root_id.to_string();
    let read = cx.background_executor().spawn({
        let root = root.clone();
        async move {
            let _span = tracing::debug_span!("chat.page", ?query, limit).entered();
            match query {
                Query::Newest => sync::message_pages::newest_page(&sqlite, &root, limit, &named).map(|page| (page.rows, page.newest)),
                Query::Results => sync::message_pages::result_cards(&sqlite, &root).map(|rows| (rows, 0)),
            }
        }
    });
    cx.spawn(async move |cx| {
        let read = read.await;
        cx.update(|cx| {
            let installed = ENTRIES.with(|entries| {
                let mut entries = entries.borrow_mut();
                let Some(entry) = entries.get_mut(&(query, root.clone())) else { return false };
                if entry.loading != Some(generation) {
                    return false;
                }
                entry.loading = None;
                let (rows, newest) = match read {
                    Ok(page) => page,
                    Err(err) => {
                        log::warn!("[chat] reading {root}'s messages: {err}");
                        return false;
                    }
                };
                entry.live = Some(Live {
                    limit,
                    source,
                    rows: rows.into_iter().map(|row| (row.id.clone(), row)).collect(),
                    newest,
                    at,
                    stale: false,
                    rows_version: next_version(),
                    out: Rc::default(),
                    out_at: None,
                    version: 0,
                });
                true
            });
            if installed {
                if let Some(store) = Store::try_global(cx) {
                    store.collections().messages.clone().update(cx, |_, cx| cx.notify());
                }
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(id: &str, at: &str) -> MessageRow {
        serde_json::from_value(json!({"id": id, "session_id": "r1", "root_id": "r1", "author_kind": "agent", "body": id, "created_at": at})).unwrap()
    }

    fn at(n: usize) -> String {
        format!("2026-11-12 05:{:02}:{:02}.000000+00", n / 60, n % 60)
    }

    #[test]
    fn the_limit_is_the_phones() {
        let transcript = &domain::chats::CHATS.transcript;
        assert_eq!(chat_message_limit(), transcript.window + transcript.window_step);
        assert_eq!(widened_limit(chat_message_limit()), chat_message_limit() + transcript.window_step);
    }

    /// The answer = the newest `limit` of the page + live rows, oldest
    /// first, plus every waiting card however old (from the page, else the
    /// collection).
    #[test]
    fn the_cut_keeps_the_newest_and_every_waiting_card() {
        let rows: Vec<MessageRow> = (0..10).map(|n| message(&format!("m{n}"), &at(n))).collect();
        let waiting = vec!["m1".to_string(), "old".to_string()];
        let out = cut(rows.iter(), Some(3), &waiting, |id| (id == "old").then(|| message("old", &at(0))));
        let ids: Vec<&str> = out.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["old", "m1", "m7", "m8", "m9"]);
        let all = cut(rows.iter(), None, &[], |_| None);
        assert_eq!(all.len(), 10);
    }

    fn put(store: &ShapeStore, shape: &str, rows: Vec<serde_json::Value>) {
        use sync::protocol::ShapeMessage;
        let spec = sync::shapes::shape_by_name(shape).unwrap();
        let msgs: Vec<ShapeMessage> = rows
            .into_iter()
            .map(|row| ShapeMessage::Insert { key: RowKey::Single(row["id"].as_str().unwrap().to_string()), value: row.as_object().unwrap().clone() })
            .collect();
        store.apply_batch(spec, &msgs, None).unwrap();
    }

    /// A 300-message chat on a store on disk: the first read answers at once
    /// (empty) and reads the page in the background; the page = the newest
    /// `limit` + the old waiting card; a live row lands with no re-read;
    /// `Load earlier` widens it; Results reads the WHOLE chat.
    #[gpui::test]
    fn a_chat_reads_its_newest_page_off_the_main_thread(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("local.sqlite");
        let disk = ShapeStore::open(&db).unwrap();
        let card = json!({"kind": "question", "title": "Which?", "questions": [{"id": "q", "prompt": "Which?", "options": [{"label": "A"}]}]});
        let report = json!({"kind": "report", "final": true, "summary": "Done"});
        put(&disk, "sessions", vec![
            json!({"id": "r1", "team_id": "t1", "root_id": "r1", "status": "idle", "shared": "t", "created_at": at(0),
                   "digest": json!({"waiting": {"id": "ask", "authorKind": "agent", "createdAt": at(0)}}).to_string()}),
            json!({"id": "c1", "team_id": "t1", "root_id": "r1", "parent_id": "r1", "status": "idle", "shared": "t", "created_at": at(0)}),
        ]);
        let mut rows: Vec<serde_json::Value> = (1..=300)
            .map(|n| json!({"id": format!("m{n:03}"), "session_id": "r1", "root_id": "r1", "author_kind": "agent", "body": format!("{n}"), "created_at": at(n)}))
            .collect();
        rows.push(json!({"id": "ask", "session_id": "r1", "root_id": "r1", "author_kind": "agent", "card": card.to_string(), "created_at": at(0)}));
        rows.push(json!({"id": "old-report", "session_id": "c1", "root_id": "r1", "author_session_id": "c1", "card": report.to_string(), "created_at": at(0)}));
        put(&disk, "messages", rows);
        let (changes, rx) = flume::unbounded::<(String, Vec<String>)>();
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            Store::global(cx).clone().connect_local("local-test", &db, rx, cx).unwrap();
        });
        let limit = chat_message_limit();
        // Off the main thread: nothing yet, the read runs in the background.
        let first = cx.update(|cx| chat_messages("r1", cx));
        assert!(first.rows.is_empty() && !first.ready);
        assert!(cx.update(|cx| crate::session_data::root_chat_view("r1", 0, cx)).is_none(), "the skeleton, not an empty chat");
        cx.run_until_parked();
        let page = cx.update(|cx| chat_messages("r1", cx));
        assert!(page.more);
        assert_eq!(page.rows.len(), limit + 1);
        assert_eq!(page.rows[0].id, "ask");
        assert_eq!(page.rows.last().unwrap().id, "m300");
        assert_eq!(page.rows[1].id, format!("m{:03}", 301 - limit));
        // The chat's own messages and model read the page, never the 302 rows.
        let own = cx.update(|cx| crate::session_data::messages_of_session("r1", cx));
        assert_eq!(own.len(), limit + 1);
        assert!(cx.update(|cx| crate::session_data::root_chat_view("r1", 0, cx)).is_some());

        // A live row: there at once (the change log), no re-read.
        put(&disk, "messages", vec![json!({"id": "m301", "session_id": "r1", "root_id": "r1", "author_kind": "agent", "body": "live", "created_at": at(301)})]);
        changes.send(("messages".into(), vec!["m301".into()])).unwrap();
        cx.run_until_parked();
        let live = cx.update(|cx| chat_messages("r1", cx));
        assert_eq!(live.rows.last().unwrap().id, "m301");
        assert_eq!(live.rows.len(), limit + 1, "still the newest {limit} + the waiting card");
        assert_ne!(live.version, page.version);

        // Load earlier: one step more, read again.
        assert!(cx.update(|cx| load_earlier("r1", cx)));
        cx.run_until_parked();
        let wider = cx.update(|cx| chat_messages("r1", cx));
        assert_eq!(wider.rows.len(), widened_limit(limit) + 1);
        assert!(wider.more);

        // Results: the whole chat (a card far outside the page).
        assert!(!wider.rows.iter().any(|row| row.id == "old-report"));
        cx.update(|cx| result_cards("r1", cx));
        cx.run_until_parked();
        let results = cx.update(|cx| result_cards("r1", cx));
        let ids: Vec<&str> = results.rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["old-report"]);
        assert!(!results.more);
        drop(dir);
    }
}
