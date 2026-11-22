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
//!
//! #EDE-108 S2: a BACKGROUND CATCH-UP (`steer::enet_catchup`) fills it over
//! eNet for every chat the account reads, from the device running each
//! session or another of the account's devices, and this device answers the
//! same pages off this store ([`serving_slot`]) for its other devices.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chat_store::sqlite::SqliteStore;
use chat_store::{StoredEvent, StoredMessage};
use domain::rows::MessageRow;
use gpui::{App, Subscription, Task};
use steer::enet_catchup::{CatchUpHandle, CatchUpSession, ChatStoreSlot};
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
/// #EXP-2018 (chat-store.json MESSAGES BY ARRIVAL): the arrival position
/// (the sync store's `messages` rowid) the catch-up took last. A new key: the
/// old `messages.createdAt` watermark is left behind, so the first pass after
/// the update takes every row once (the rows it had skipped included).
const MESSAGES_MARK: &str = "messages.arrival";
/// Messages read per page of the catch-up.
const MESSAGES_PAGE: usize = 1000;
/// How often the app looks at whether one of its windows is active (the
/// catch-up's idleness on battery).
const ACTIVITY_POLL: Duration = Duration::from_secs(30);

/// The active account's store and its writer.
struct ChatStoreGlobal {
    account_id: String,
    store: Arc<SqliteStore>,
    writes: flume::Sender<Write>,
    catch_up: Option<Task<()>>,
    _messages: Option<Subscription>,
    /// #EDE-108: the eNet catch-up loop, once the account's node is up.
    enet: Option<CatchUpHandle>,
    /// The chats the loop walks (the synced `sessions` rows).
    sessions: Arc<Mutex<Vec<CatchUpSession>>>,
    /// Ms of the last time an app window was active.
    last_active: Arc<AtomicU64>,
    _sessions: Option<Subscription>,
    _activity: Option<Task<()>>,
    /// #EXP-2021: tells the open views which chats the store took rows of.
    changed: flume::Sender<Vec<String>>,
    _changed: Option<Task<()>>,
}

impl gpui::Global for ChatStoreGlobal {}

/// #EXP-2021 (find-in-chat.json THE STORE CATCHING UP): the store took in
/// rows of these chats (root and session ids) at revision `rev`; an open
/// find bar of one of them asks the store again. Views observe it as a
/// global.
#[derive(Default)]
pub(crate) struct ChatStoreChanged {
    pub(crate) rev: u64,
    pub(crate) ids: std::collections::HashSet<String>,
}

impl gpui::Global for ChatStoreChanged {}

impl ChatStoreChanged {
    /// Did the last change touch the chat `root_id` or its session `session_id`?
    pub(crate) fn touches(&self, root_id: Option<&str>, session_id: &str) -> bool {
        self.ids.contains(session_id) || root_id.is_some_and(|root| self.ids.contains(root))
    }
}

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
    let (changed, changed_rx) = flume::unbounded::<Vec<String>>();
    let told = changed.clone();
    if let Err(err) = std::thread::Builder::new().name("chat-store".into()).spawn(move || {
        run_writer(&writer, &rx, &move |ids| {
            let _ = told.send(ids);
        })
    }) {
        log::warn!("[chat-store] no writer thread: {err}");
        return;
    }
    let messages = Store::try_global(cx).map(|sync| sync.collections().messages.clone());
    let messages = messages.map(|messages| cx.observe(&messages, |_, cx| schedule_catch_up(cx)));
    let sessions = Store::try_global(cx).map(|sync| sync.collections().sessions.clone());
    let sessions = sessions.map(|sessions| cx.observe(&sessions, |_, cx| sessions_changed(cx)));
    *lock(&serving_slot()) = Some(Arc::clone(&store));
    let last_active = Arc::new(AtomicU64::new(now_ms() as u64));
    let activity = {
        let last_active = Arc::clone(&last_active);
        cx.spawn(async move |cx| loop {
            if cx.update(|cx| cx.active_window().is_some()) {
                last_active.store(now_ms() as u64, Ordering::Relaxed);
            }
            cx.background_executor().timer(ACTIVITY_POLL).await;
        })
    };
    let changed_task = cx.spawn(async move |cx| {
        while let Ok(first) = changed_rx.recv_async().await {
            let ids: std::collections::HashSet<String> = std::iter::once(first).chain(changed_rx.try_iter()).flatten().collect();
            let _ = cx.update(|cx| {
                let rev = cx.try_global::<ChatStoreChanged>().map_or(0, |changed| changed.rev) + 1;
                cx.set_global(ChatStoreChanged { rev, ids });
            });
        }
    });
    cx.set_global(ChatStoreGlobal {
        account_id: account_id.to_string(),
        store,
        writes,
        catch_up: None,
        _messages: messages,
        enet: None,
        sessions: Arc::new(Mutex::new(Vec::new())),
        last_active,
        _sessions: sessions,
        _activity: Some(activity),
        changed,
        _changed: Some(changed_task),
    });
    sessions_changed(cx);
    schedule_catch_up(cx);
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// #EDE-108: the store this device answers other devices' catch-up pages
/// from (the node's `StoredJournals` holds the same slot).
pub(crate) fn serving_slot() -> ChatStoreSlot {
    static SLOT: OnceLock<ChatStoreSlot> = OnceLock::new();
    Arc::clone(SLOT.get_or_init(Default::default))
}

/// #EDE-108: the account's chats changed: the loop gets the new list (and
/// starts, once the account's eNet node is up) and runs a pass.
fn sessions_changed(cx: &mut App) {
    let Some(global) = cx.try_global::<ChatStoreGlobal>() else { return };
    let list: Vec<CatchUpSession> = Store::try_global(cx)
        .map(|sync| {
            sync.collections()
                .sessions
                .read(cx)
                .iter()
                .filter_map(|row| {
                    Some(CatchUpSession {
                        session_id: row.id.clone(),
                        root_id: row.root_id_or_self().to_string(),
                        device_id: row.device_id.clone().filter(|device| !device.is_empty())?,
                        updated_at: row.updated_at.clone(),
                        busy: row.busy,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    *lock(&global.sessions) = list;
    match &global.enet {
        Some(enet) => enet.kick(),
        None => start_enet_catch_up(cx),
    }
}

/// Starts the eNet catch-up for the active account once its node runs.
fn start_enet_catch_up(cx: &mut App) {
    let Some(global) = cx.try_global::<ChatStoreGlobal>() else { return };
    let (Some(install), Some(runtime)) = (crate::enet_wiring::node(&global.account_id, cx), crate::steer_wiring::runtime(cx)) else {
        return;
    };
    let sessions = Arc::clone(&global.sessions);
    let last_active = Arc::clone(&global.last_active);
    let changed = global.changed.clone();
    let deps = steer::enet_catchup::CatchUpDeps {
        user_id: install.user_id().to_string(),
        install,
        store: Arc::clone(&global.store),
        own_device_id: crate::queries::own_device_id(cx),
        sessions: Arc::new(move || lock(&sessions).clone()),
        may_run: Arc::new(move || {
            let idle = (now_ms() as u64).saturating_sub(last_active.load(Ordering::Relaxed));
            steer::enet_catchup::may_run_now(idle)
        }),
        on_stored: Some(Arc::new(move |session_id: &str| {
            let _ = changed.send(vec![session_id.to_string()]);
        })),
    };
    let handle = steer::enet_catchup::spawn(runtime.handle(), deps);
    cx.global_mut::<ChatStoreGlobal>().enet = Some(handle);
}

/// An account switch (or an expiry's re-sign-in as someone else): nothing
/// more is written; the file stays for the account's next sign-in.
pub(crate) fn stop(cx: &mut App) {
    *lock(&serving_slot()) = None;
    if cx.has_global::<ChatStoreGlobal>() {
        cx.remove_global::<ChatStoreGlobal>();
    }
}

/// #EDE-120 SIGN-OUT (`chat-store.json` `signOut`): a DELIBERATE sign-out
/// stops `account_id`'s store (the writer thread, the catch-up loop, the
/// store this device serves pages from) and deletes its files
/// (`accounts/<id>/chat-store.sqlite` + `-wal`/`-shm`), never another
/// account's. Off the main thread: once every holder let go of the store (its
/// connection closed), at most [`CLOSE_WAIT`]; a file still held is tried
/// again [`WIPE_TRIES`] times. A session expiry never comes here.
pub(crate) fn sign_out(account_id: &str, cx: &mut App) {
    let held = cx
        .try_global::<ChatStoreGlobal>()
        .filter(|global| global.account_id == account_id)
        .map(|global| Arc::downgrade(&global.store));
    if held.is_some() {
        stop(cx);
    }
    let Some(data_dir) = cx.try_global::<AuthContext>().map(|auth| auth.data_dir.clone()) else { return };
    let account = account_id.to_string();
    let spawned = std::thread::Builder::new()
        .name("chat-store-wipe".into())
        .spawn(move || wipe(&data_dir, &account, held.as_ref()));
    if let Err(err) = spawned {
        log::warn!("[chat-store] no thread to delete the store: {err}");
    }
}

/// How long a sign-out waits for the store's holders to let go.
const CLOSE_WAIT: Duration = Duration::from_secs(10);
/// Attempts at deleting a store file that is still held.
const WIPE_TRIES: u32 = 5;

/// Deletes `account`'s store once `held` (the stopped store) is closed.
fn wipe(data_dir: &Path, account: &str, held: Option<&std::sync::Weak<SqliteStore>>) {
    let deadline = Instant::now() + CLOSE_WAIT;
    while held.is_some_and(|store| store.strong_count() > 0) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    for attempt in 1..=WIPE_TRIES {
        match chat_store::sqlite::delete_account_store(data_dir, account) {
            Ok(_) => return,
            Err((_, err)) if attempt == WIPE_TRIES => log::warn!("[chat-store] deleting the signed-out store failed: {err}"),
            Err(_) => std::thread::sleep(Duration::from_millis(500) * attempt),
        }
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
                if cx.global::<ChatStoreGlobal>().enet.is_none() {
                    start_enet_catch_up(cx);
                }
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

/// The writer thread: one job at a time, feeds coalesced; `changed` is told
/// the chats (root and session ids) each job took rows of (#EXP-2021).
fn run_writer(store: &SqliteStore, rx: &flume::Receiver<Write>, changed: &dyn Fn(Vec<String>)) {
    let mut slow_at: Option<Instant> = None;
    while let Ok(first) = rx.recv() {
        for write in std::iter::once(first).chain(rx.try_iter()) {
            match write {
                Write::Feed { root_id, session_id, device_id, events } => {
                    let stored = stored_events(&root_id, &session_id, &device_id, &events);
                    match store.put_events(&stored) {
                        Ok(added) if added > 0 => changed(vec![root_id, session_id]),
                        Ok(_) => {}
                        Err(err) => log::warn!("[chat-store] storing {session_id}'s feed failed: {err}"),
                    }
                }
                Write::CatchUp { sqlite, data_dir, device_id } => {
                    let slow = slow_at.is_none_or(|at| at.elapsed() >= SLOW_CATCH_UP);
                    if slow {
                        slow_at = Some(Instant::now());
                    }
                    let mut roots = std::collections::BTreeSet::new();
                    if let Some(sqlite) = &sqlite {
                        if let Err(err) = catch_up_messages(store, sqlite, slow, &mut roots) {
                            log::warn!("[chat-store] messages catch-up failed: {err}");
                        }
                    }
                    if slow {
                        catch_up_journals(store, &data_dir, &device_id, &mut roots);
                    }
                    if !roots.is_empty() {
                        changed(roots.into_iter().collect());
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

/// #EXP-2018: the synced messages that ARRIVED since the last catch-up, by
/// the sync store's arrival order (never a `created_at` watermark: a row
/// reaching this device late with an older `created_at` is taken too). The
/// mark is inclusive (a reused position is not lost); a mark above the
/// table's newest position (a refetch emptied and refilled it) starts over.
/// `slow` = also the rows stored sealed, opened again.
/// `roots` = the chats it stored rows of.
fn catch_up_messages(
    store: &SqliteStore,
    sqlite: &ShapeStore,
    slow: bool,
    roots: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    let stored_mark = store.meta(MESSAGES_MARK).map_err(|err| err.to_string())?.and_then(|mark| mark.parse::<i64>().ok());
    let newest = sqlite.messages_arrival_max().map_err(|err| err.to_string())?;
    let mut mark = stored_mark.filter(|mark| *mark <= newest).unwrap_or(0);
    loop {
        let page = sync::message_pages::messages_by_arrival(sqlite, mark, MESSAGES_PAGE).map_err(|err| err.to_string())?;
        let stored: Vec<(i64, StoredMessage)> =
            page.rows.iter().filter_map(|(at, row)| Some((*at, stored_message(row)?))).collect();
        store.put_messages(stored.iter().map(|(_, message)| message)).map_err(|err| err.to_string())?;
        // The inclusive mark's own row comes back every pass: not a change.
        roots.extend(stored.iter().filter(|(at, _)| *at > mark).map(|(_, message)| message.root_id.clone()));
        let Some(last) = page.last else { break };
        let moved = last > mark;
        mark = last;
        if !moved || page.read < MESSAGES_PAGE {
            break;
        }
    }
    if stored_mark != Some(mark) {
        store.set_meta(MESSAGES_MARK, &mark.to_string()).map_err(|err| err.to_string())?;
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
        roots.extend(opened.iter().map(|message| message.root_id.clone()));
    }
    Ok(())
}

/// This device's journals that changed since they were last taken in. A
/// journal whose session the synced rows have not named a root for yet
/// waits for the next pass.
fn catch_up_journals(store: &SqliteStore, data_dir: &Path, device_id: &str, roots: &mut std::collections::BTreeSet<String>) {
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
            Ok(added) => {
                if added > 0 {
                    roots.insert(root_id.clone());
                }
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

    /// #EDE-120: a sign-out's wipe waits for the writer to let go of the
    /// store, then deletes it (and its WAL/SHM) and nothing else.
    #[test]
    fn sign_out_wipes_the_store_once_the_writer_let_go() {
        let data_dir = std::env::temp_dir().join(format!("ui-chat-store-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&data_dir);
        let store = Arc::new(SqliteStore::open(&store_path(&data_dir, "u1")).unwrap());
        store.set_meta("k", "v").unwrap();
        let other = SqliteStore::open(&store_path(&data_dir, "u2")).unwrap();
        other.set_meta("k", "v").unwrap();
        std::fs::write(data_dir.join("accounts/u1/sync.sqlite"), b"sync").unwrap();
        let held = Arc::downgrade(&store);
        let (writes, rx) = flume::unbounded::<Write>();
        let writer = std::thread::spawn(move || run_writer(&store, &rx, &|_| {}));
        drop(writes);
        wipe(&data_dir, "u1", Some(&held));
        writer.join().unwrap();
        assert_eq!(held.strong_count(), 0);
        for name in ["chat-store.sqlite", "chat-store.sqlite-wal", "chat-store.sqlite-shm"] {
            assert!(!data_dir.join("accounts/u1").join(name).exists(), "{name}");
        }
        assert!(data_dir.join("accounts/u1/sync.sqlite").exists());
        assert!(store_path(&data_dir, "u2").exists());
        drop(other);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// #EDE-120 follow-up: the real sign-out path with the account id the
    /// desktop really uses (`account_id_for`: `app.exponential.at--<userId>`),
    /// not a bare `u1` (the earlier test used bare ids, which hid that the
    /// safe-segment rule rejected every real, dotted account dir).
    #[test]
    fn sign_out_wipes_a_real_account_dir() {
        let account = api::accounts::account_id_for("https://app.exponential.at", "8Jk3xQ2Lm9");
        assert!(account.contains('.'), "{account}");
        let data_dir = std::env::temp_dir().join(format!("ui-chat-store-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&data_dir);
        let store = Arc::new(SqliteStore::open(&store_path(&data_dir, &account)).unwrap());
        store.set_meta("k", "v").unwrap();
        std::fs::write(data_dir.join("accounts").join(&account).join("sync.sqlite"), b"sync").unwrap();
        let held = Arc::downgrade(&store);
        let (writes, rx) = flume::unbounded::<Write>();
        let writer = std::thread::spawn(move || run_writer(&store, &rx, &|_| {}));
        drop(writes);
        wipe(&data_dir, &account, Some(&held));
        writer.join().unwrap();
        assert!(!store_path(&data_dir, &account).exists());
        assert!(data_dir.join("accounts").join(&account).join("sync.sqlite").exists());
        let _ = std::fs::remove_dir_all(&data_dir);
    }

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

    /// #EXP-2018 (chat-store.json MESSAGES BY ARRIVAL): a message that
    /// syncs AFTER a catch-up with an OLDER `created_at` is still taken in
    /// and found; an old `createdAt` watermark is dropped (a full pass).
    #[test]
    fn a_late_older_message_is_taken_in_by_arrival() {
        let dir = std::env::temp_dir().join(format!("ui-chat-store-arrival-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        let sqlite = ShapeStore::open(&dir.join("sync.sqlite")).unwrap();
        let spec = sync::shapes::shape_by_name("messages").unwrap();
        let insert = |id: &str, session: &str, body: &str, at: &str| {
            let value = serde_json::json!({"id": id, "session_id": session, "root_id": session, "author_kind": "user", "body": body, "created_at": at});
            let msg = sync::protocol::ShapeMessage::Insert { key: sync::protocol::RowKey::Single(id.into()), value: value.as_object().unwrap().clone() };
            sqlite.apply_batch(spec, &[msg], None).unwrap();
        };
        let store = SqliteStore::open_in_memory().unwrap();
        store.set_meta("messages.createdAt", "2026-11-22 12:00:00+00").unwrap();
        insert("m1", "r1", "the newest wombat", "2026-11-22 12:00:00+00");
        catch_up_messages(&store, &sqlite, false, &mut Default::default()).unwrap();
        assert_eq!(store.search_chat("r1", "wombat", 10).unwrap().total, 1);
        // A chat that became visible later: its rows are hours older.
        insert("m0", "r2", "an early wombat", "2026-11-22 08:00:00+00");
        catch_up_messages(&store, &sqlite, false, &mut Default::default()).unwrap();
        assert_eq!(store.search_chat("r2", "wombat", 10).unwrap().total, 1, "the late older row is indexed");
        // A refetch emptied the table and refilled it: the mark starts over.
        sqlite.apply_batch(spec, &[sync::protocol::ShapeMessage::MustRefetch], None).unwrap();
        insert("m9", "r3", "a refetched wombat", "2026-11-22 07:00:00+00");
        catch_up_messages(&store, &sqlite, false, &mut Default::default()).unwrap();
        assert_eq!(store.search_chat("r3", "wombat", 10).unwrap().total, 1);
        let _ = std::fs::remove_dir_all(&dir);
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
