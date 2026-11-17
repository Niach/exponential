//! #EDE-105 — the desktop's CHAT STORE (`fixtures/chat-store.json`): every
//! chat this account can see, on disk, with its search index
//! ([`chat_store::sqlite::SqliteStore`], `accounts/<id>/chat-store.sqlite`).
//!
//! What feeds it, all off the main thread on ONE writer thread:
//!
//! * every feed a session view RECEIVES (`ViewerEvent::Activity` and the
//!   history pages it scrolls in, [`record`]), keyed by the device the
//!   session is on;
//! * this device's own journals (`{data_dir}/journal/<id>.jsonl`), re-read
//!   when one changed, at most every [`SLOW_CATCH_UP`];
//! * the synced `messages` (opened with the chat key; a row whose key is not
//!   here yet is stored without a body and tried again every
//!   [`SLOW_CATCH_UP`]), a few seconds after the collection moved.
//!
//! A session view of another device's session opens FROM it ([`read_tail`])
//! before the live feed attaches; the room's replay replaces that seed.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chat_store::sqlite::SqliteStore;
use chat_store::{StoredEvent, StoredMessage};
use domain::rows::MessageRow;
use gpui::{App, Subscription, Task};
use steer::frames::ActivityEvent;
use sync::store::ShapeStore;
use sync::Store;

use crate::AuthContext;

/// How long the synced messages may sit before the store takes them in.
const MESSAGES_DEBOUNCE: Duration = Duration::from_secs(3);
/// The journals and the still-sealed messages are looked at no more often.
const SLOW_CATCH_UP: Duration = Duration::from_secs(5 * 60);
/// Message ids read per retry of the sealed ones.
const LOCKED_CHUNK: usize = 500;
const MESSAGES_MARK: &str = "messages.createdAt";

/// The active account's store and its writer.
struct ChatStoreGlobal {
    account_id: String,
    store: Arc<SqliteStore>,
    writes: flume::Sender<Write>,
    catch_up: Option<Task<()>>,
    _messages: Option<Subscription>,
}

impl gpui::Global for ChatStoreGlobal {}

/// One job for the writer thread.
enum Write {
    /// A feed's events, as received.
    Feed { root_id: String, session_id: String, device_id: String, events: Vec<(u64, ActivityEvent)> },
    /// Take in the synced messages and (when due) the journals.
    CatchUp { sqlite: Option<Arc<ShapeStore>>, data_dir: PathBuf, device_id: String },
}

/// The store's file for `account_id`, beside its sync store.
pub(crate) fn store_path(data_dir: &Path, account_id: &str) -> PathBuf {
    data_dir.join("accounts").join(account_id).join("chat-store.sqlite")
}

/// Opens `account_id`'s store and starts feeding it (sign-in, account
/// switch). A store that cannot open leaves the app as it was without one.
pub(crate) fn start(account_id: &str, cx: &mut App) {
    if cx.try_global::<ChatStoreGlobal>().is_some_and(|global| global.account_id == account_id) {
        return;
    }
    stop(cx);
    let Some(data_dir) = cx.try_global::<AuthContext>().map(|auth| auth.data_dir.clone()) else { return };
    let store = match SqliteStore::open(&store_path(&data_dir, account_id)) {
        Ok(store) => Arc::new(store),
        Err(err) => {
            log::warn!("[chat-store] opening the store failed: {err}");
            return;
        }
    };
    let (writes, rx) = flume::unbounded::<Write>();
    let writer = Arc::clone(&store);
    if let Err(err) = std::thread::Builder::new().name("chat-store".into()).spawn(move || run_writer(&writer, &rx)) {
        log::warn!("[chat-store] no writer thread: {err}");
        return;
    }
    let messages = Store::try_global(cx).map(|sync| sync.collections().messages.clone());
    let messages = messages.map(|messages| cx.observe(&messages, |_, cx| schedule_catch_up(cx)));
    cx.set_global(ChatStoreGlobal { account_id: account_id.to_string(), store, writes, catch_up: None, _messages: messages });
    schedule_catch_up(cx);
}

/// Sign-out: nothing more is written (the file stays, like the sync store).
pub(crate) fn stop(cx: &mut App) {
    if cx.has_global::<ChatStoreGlobal>() {
        cx.remove_global::<ChatStoreGlobal>();
    }
}

/// The active account's store.
pub(crate) fn store(cx: &App) -> Option<Arc<SqliteStore>> {
    cx.try_global::<ChatStoreGlobal>().map(|global| Arc::clone(&global.store))
}

/// A catch-up after [`MESSAGES_DEBOUNCE`] (one pending at a time).
fn schedule_catch_up(cx: &mut App) {
    let Some(global) = cx.try_global::<ChatStoreGlobal>() else { return };
    if global.catch_up.is_some() {
        return;
    }
    let task = cx.spawn(async move |cx| {
        cx.background_executor().timer(MESSAGES_DEBOUNCE).await;
        let _ = cx.update(|cx| {
            let device_id = crate::queries::own_device_id(cx);
            let data_dir = crate::coding_flow::coding_data_dir(cx);
            let sqlite = Store::try_global(cx).and_then(|sync| {
                let account_id = sync.session(cx).account_id()?.to_string();
                sync.manager().store(&account_id)
            });
            if let Some(global) = cx.try_global::<ChatStoreGlobal>() {
                let _ = global.writes.send(Write::CatchUp { sqlite, data_dir, device_id });
            }
            if cx.has_global::<ChatStoreGlobal>() {
                cx.global_mut::<ChatStoreGlobal>().catch_up = None;
            }
        });
    });
    cx.global_mut::<ChatStoreGlobal>().catch_up = Some(task);
}

/// Stores what a session view received from `session_id`'s feed (its live
/// events and the history pages it asked for), as the device the session
/// is on sent them.
pub(crate) fn record(session_id: &str, events: Vec<(u64, ActivityEvent)>, cx: &App) {
    if events.is_empty() {
        return;
    }
    let Some(global) = cx.try_global::<ChatStoreGlobal>() else { return };
    let Some(row) = crate::session_data::session(session_id, cx) else { return };
    let Some(device_id) = row.device_id.clone().filter(|device| !device.is_empty()) else { return };
    let _ = global.writes.send(Write::Feed {
        root_id: row.root_id_or_self().to_string(),
        session_id: session_id.to_string(),
        device_id,
        events,
    });
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

fn stored_events(root_id: &str, session_id: &str, device_id: &str, events: &[(u64, ActivityEvent)]) -> Vec<StoredEvent> {
    let stored_at = now_ms();
    events
        .iter()
        .filter_map(|(seq, event)| {
            Some(StoredEvent {
                root_id: root_id.to_string(),
                session_id: session_id.to_string(),
                device_id: device_id.to_string(),
                seq: *seq,
                stored_at,
                event: serde_json::to_value(event).ok()?,
            })
        })
        .collect()
}

/// The writer thread: one job at a time, feeds coalesced.
fn run_writer(store: &SqliteStore, rx: &flume::Receiver<Write>) {
    let mut slow_at: Option<Instant> = None;
    while let Ok(first) = rx.recv() {
        for write in std::iter::once(first).chain(rx.try_iter()) {
            match write {
                Write::Feed { root_id, session_id, device_id, events } => {
                    let stored = stored_events(&root_id, &session_id, &device_id, &events);
                    if let Err(err) = store.put_events(&stored) {
                        log::warn!("[chat-store] storing {session_id}'s feed failed: {err}");
                    }
                }
                Write::CatchUp { sqlite, data_dir, device_id } => {
                    let slow = slow_at.is_none_or(|at| at.elapsed() >= SLOW_CATCH_UP);
                    if slow {
                        slow_at = Some(Instant::now());
                    }
                    if let Some(sqlite) = &sqlite {
                        if let Err(err) = catch_up_messages(store, sqlite, slow) {
                            log::warn!("[chat-store] messages catch-up failed: {err}");
                        }
                    }
                    if slow {
                        catch_up_journals(store, &data_dir, &device_id);
                    }
                }
            }
        }
    }
}

/// A synced message as the store keeps it (`None` = it belongs to no chat).
fn stored_message(row: &MessageRow) -> Option<StoredMessage> {
    if row.session_id.is_empty() {
        return None;
    }
    let root_id = row
        .root_id
        .clone()
        .filter(|root| !root.is_empty())
        .or_else(|| sync::sealed::Opener::global().root_of(&row.session_id))
        .unwrap_or_else(|| row.session_id.clone());
    Some(StoredMessage {
        id: row.id.clone(),
        root_id,
        session_id: row.session_id.clone(),
        author_kind: row.author_kind.clone().unwrap_or_default(),
        body: row.locked().is_none().then(|| row.body_text().to_string()),
        created_at: domain::session_tree::iso_ms(row.created_at.as_deref()?)?,
    })
}

/// The synced messages since the last catch-up (the mark is inclusive, so a
/// row landing in the same instant is not lost); `slow` = also the rows
/// stored sealed, opened again.
fn catch_up_messages(store: &SqliteStore, sqlite: &ShapeStore, slow: bool) -> Result<(), String> {
    let mark = store.meta(MESSAGES_MARK).map_err(|err| err.to_string())?.unwrap_or_default();
    let rows = sync::message_pages::messages_since(sqlite, &mark).map_err(|err| err.to_string())?;
    let newest = rows.iter().filter_map(|row| row.created_at.as_deref()).max().map(str::to_string);
    let stored: Vec<StoredMessage> = rows.iter().filter_map(stored_message).collect();
    store.put_messages(&stored).map_err(|err| err.to_string())?;
    if let Some(newest) = newest.filter(|newest| *newest > mark) {
        store.set_meta(MESSAGES_MARK, &newest).map_err(|err| err.to_string())?;
    }
    if !slow {
        return Ok(());
    }
    let locked = store.locked_message_ids().map_err(|err| err.to_string())?;
    for chunk in locked.chunks(LOCKED_CHUNK) {
        let ids: Vec<&str> = chunk.iter().map(String::as_str).collect();
        let rows = sync::message_pages::messages_by_ids(sqlite, &ids).map_err(|err| err.to_string())?;
        let opened: Vec<StoredMessage> = rows.iter().filter_map(stored_message).filter(|message| message.body.is_some()).collect();
        store.put_messages(&opened).map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// This device's journals that changed since they were last taken in. A
/// journal whose session the synced rows have not named a root for yet
/// waits for the next pass.
fn catch_up_journals(store: &SqliteStore, data_dir: &Path, device_id: &str) {
    let Ok(entries) = std::fs::read_dir(steer::journal_dir(data_dir)) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(session_id) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string) else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().ok().and_then(|at| at.duration_since(UNIX_EPOCH).ok()).map_or(0, |at| at.as_millis());
        let stamp = format!("{}:{modified}", meta.len());
        let key = format!("journal.{session_id}");
        if store.meta(&key).ok().flatten().as_deref() == Some(stamp.as_str()) {
            continue;
        }
        let Some(root_id) = sync::sealed::Opener::global().root_of(&session_id) else { continue };
        let Some(events) = steer::read_journal_seq(data_dir, &session_id) else { continue };
        let stored = stored_events(&root_id, &session_id, device_id, &events);
        match store.put_events(&stored) {
            Ok(_) => {
                let _ = store.set_meta(&key, &stamp);
            }
            Err(err) => log::warn!("[chat-store] taking in {session_id}'s journal failed: {err}"),
        }
    }
}

/// OPEN: `session_id`'s stored events on `device_id` (off the main thread),
/// in seq order; `None` when the store holds none.
pub(crate) fn read_events(store: &SqliteStore, session_id: &str, device_id: &str) -> Option<Vec<(u64, ActivityEvent)>> {
    let events = store.read_events(session_id, device_id, None, None).ok()?;
    let events: Vec<(u64, ActivityEvent)> =
        events.into_iter().filter_map(|stored| Some((stored.seq, serde_json::from_value(stored.event).ok()?))).collect();
    (!events.is_empty()).then_some(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feed_round_trips_through_the_store() {
        let store = SqliteStore::open_in_memory().unwrap();
        let events = vec![
            (1, ActivityEvent::user_message("Fix the café")),
            (2, ActivityEvent::tool("Bash", Some("cargo test".into()))),
            (3, ActivityEvent::narration("All green.")),
        ];
        store.put_events(&stored_events("r", "s", "d", &events)).unwrap();
        assert_eq!(read_events(&store, "s", "d").unwrap(), events);
        assert!(read_events(&store, "s", "other").is_none());
        let found = store.search_chat("r", "CAFE", 10).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.hits[0].seq, Some(1));
    }

    #[test]
    fn a_message_without_a_session_is_not_a_chat_message() {
        let row = MessageRow { id: "m".into(), body: Some("hi".into()), created_at: Some("2026-11-17T10:00:00.000Z".into()), ..Default::default() };
        assert!(stored_message(&row).is_none());
        let row = MessageRow { session_id: "s".into(), ..row };
        let stored = stored_message(&row).unwrap();
        assert_eq!(stored.root_id, "s");
        assert_eq!(stored.body.as_deref(), Some("hi"));
    }
}
