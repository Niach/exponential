//! #EDE-114 — the phones' CHAT STORE (phase S3 of
//! `packages/domain-contract/fixtures/chat-store.json`): the `chat-store`
//! crate's SQLite adapter behind UniFFI, linked into the apps' one library
//! (`exponential-mobile-ffi`). JSON strings cross for the wire events and
//! the answers, records for everything the apps build.
//!
//! * [`ChatStore`]: one account's store at
//!   `{data_dir}/accounts/<account>/chat-store.sqlite`, `data_dir` = the
//!   app's PROTECTED directory (iOS: Data Protection, complete until first
//!   unlock, in the app container, shared with nothing; Android: app-private
//!   no-backup files). Fed by what the phone receives (the steer feed, the
//!   synced messages opened with the tree key, catch-up pages); a chat OPENS
//!   from it ([`ChatStore::open_tail`]) before the live feed attaches.
//! * [`CatchUp`]: the `catchUp` walk with no I/O and no clock: the app asks
//!   it what to send ([`CatchUp::next_ask`]), carries the frame over eNet,
//!   and hands back the chunk ([`CatchUp::take_answer`]); the walk, the
//!   holders, the watermarks, the bucket and the retries are the crate's.
//! * [`delete_account_store`]: #EDE-120 SIGN-OUT, a deliberate sign-out only.
//!
//! Blocking: the apps call it off their main thread.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chat_store::catchup::{self, Ask, Bucket, Holder, Install, PageAnswer};
use chat_store::sqlite::SqliteStore;
use chat_store::{open, StoredEvent, StoredMessage};
use serde_json::{json, Value};

uniffi::setup_scaffolding!();

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, uniffi::Error)]
pub enum ChatStoreError {
    /// The store failed (the file, SQLite).
    Store { reason: String },
    /// A JSON argument did not parse.
    Json { reason: String },
}

impl std::fmt::Display for ChatStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatStoreError::Store { reason } => write!(f, "chat store: {reason}"),
            ChatStoreError::Json { reason } => write!(f, "chat store json: {reason}"),
        }
    }
}

impl From<chat_store::sqlite::Error> for ChatStoreError {
    fn from(err: chat_store::sqlite::Error) -> Self {
        ChatStoreError::Store { reason: err.to_string() }
    }
}

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, ChatStoreError> {
    serde_json::from_str(json).map_err(|err| ChatStoreError::Json { reason: err.to_string() })
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The store's file for `account_id` under `data_dir` (the desktop's layout).
fn store_path(data_dir: &str, account_id: &str) -> PathBuf {
    Path::new(data_dir).join("accounts").join(account_id).join("chat-store.sqlite")
}

/// RETENTION: the days a phone keeps tool steps by default (`retentionDays`).
#[uniffi::export]
pub fn phone_retention_days() -> u32 {
    chat_store::PHONE_RETENTION_DAYS
}

/// Is `frame_json` a catch-up page (a `history_page` naming a device)?
#[uniffi::export]
pub fn is_catch_up_page(frame_json: String) -> bool {
    serde_json::from_str::<Value>(&frame_json).is_ok_and(|frame| catchup::is_page(&frame))
}

/// #EDE-120 SIGN-OUT: deletes `account_id`'s store files (the database, its
/// WAL and SHM; never another file) once its [`ChatStore`] is closed. Only
/// for a deliberate sign-out: an expired session keeps the store. Returns
/// how many files went.
#[uniffi::export]
pub fn delete_account_store(data_dir: String, account_id: String) -> Result<u32, ChatStoreError> {
    chat_store::sqlite::delete_account_store(Path::new(&data_dir), &account_id)
        .map(|gone| gone.len() as u32)
        .map_err(|(_, err)| ChatStoreError::Store { reason: err.to_string() })
}

/// The holder's access verdict for a page (the join's rule, the app's rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PageAccess {
    Allowed,
    Unauthorized,
    NoSuchSession,
}

/// One account's chat store.
#[derive(uniffi::Object)]
pub struct ChatStore {
    store: SqliteStore,
}

#[uniffi::export]
impl ChatStore {
    /// Opens (creating) `account_id`'s store under `data_dir`.
    #[uniffi::constructor]
    pub fn open(data_dir: String, account_id: String) -> Result<Arc<ChatStore>, ChatStoreError> {
        if !chat_store::leave::safe_account(&account_id) {
            return Err(ChatStoreError::Store { reason: format!("not an account id: {account_id}") });
        }
        Ok(Arc::new(ChatStore { store: SqliteStore::open(&store_path(&data_dir, &account_id))? }))
    }

    /// A store in memory (tests).
    #[uniffi::constructor]
    pub fn in_memory() -> Result<Arc<ChatStore>, ChatStoreError> {
        Ok(Arc::new(ChatStore { store: SqliteStore::open_in_memory()? }))
    }

    /// A feed's events as received (`[{seq, event}]`, the wire `activity`
    /// events), keyed by the device the session is on; DEDUP keeps the
    /// first. Returns how many were new.
    pub fn put_feed(&self, root_id: String, session_id: String, device_id: String, events_json: String, now_ms: i64) -> Result<u32, ChatStoreError> {
        let rows: Vec<Value> = parse(&events_json)?;
        let events: Vec<StoredEvent> = rows
            .into_iter()
            .filter_map(|row| {
                let seq = row.get("seq")?.as_u64()?;
                let event = row.get("event")?.clone();
                event.get("kind")?.as_str()?;
                Some(StoredEvent { root_id: root_id.clone(), session_id: session_id.clone(), device_id: device_id.clone(), seq, stored_at: now_ms, event })
            })
            .collect();
        Ok(self.store.put_events(&events)? as u32)
    }

    /// Synced messages (`[StoredMessage]`: `body` = the OPENED body, null
    /// while the chat key is missing); latest wins by id.
    pub fn put_messages(&self, messages_json: String) -> Result<(), ChatStoreError> {
        let messages: Vec<StoredMessage> = parse(&messages_json)?;
        Ok(self.store.put_messages(&messages)?)
    }

    /// The messages stored without a body (to open again once a key arrives).
    pub fn locked_message_ids(&self) -> Result<Vec<String>, ChatStoreError> {
        Ok(self.store.locked_message_ids()?)
    }

    /// OPEN: `{events: [{seq, event}], truncated}`, the session's stored
    /// events on `device_id` through the REPLAY TAIL of `tail` events (the
    /// contract's `steerFeed.replayTail`). Empty `events` = nothing stored.
    pub fn open_tail(&self, session_id: String, device_id: String, tail: u32) -> Result<String, ChatStoreError> {
        let held = self.store.read_open(&session_id, &device_id, tail as usize)?;
        let wire: Vec<&Value> = held.iter().map(|event| &event.event).collect();
        let (kept, truncated) = open::replay_tail(&wire, tail as usize);
        let events: Vec<Value> = kept.into_iter().map(|ix| json!({ "seq": held[ix].seq, "event": held[ix].event })).collect();
        Ok(json!({ "events": events, "truncated": truncated }).to_string())
    }

    /// An older page: the newest `limit` events below `before_seq`, oldest
    /// first, `{events, seqs}` like a `history_chunk`.
    pub fn page(&self, session_id: String, device_id: String, before_seq: u64, limit: u32) -> Result<String, ChatStoreError> {
        let before = before_seq.min(i64::MAX as u64);
        let held = self.store.read_events(&session_id, &device_id, Some(before), Some(limit as usize))?;
        Ok(json!({
            "events": held.iter().map(|event| &event.event).collect::<Vec<_>>(),
            "seqs": held.iter().map(|event| event.seq).collect::<Vec<_>>(),
        })
        .to_string())
    }

    /// How many events the store holds for a session on a device.
    pub fn count(&self, session_id: String, device_id: String) -> Result<u64, ChatStoreError> {
        Ok(self.store.span(&session_id, &device_id)?.map_or(0, |span| span.count))
    }

    /// FIND IN CHAT: `{hits, total}`.
    pub fn search_chat(&self, root_id: String, query: String, limit: u32) -> Result<String, ChatStoreError> {
        Ok(serde_json::to_string(&self.store.search_chat(&root_id, &query, limit as usize)?).unwrap_or_default())
    }

    /// GLOBAL: `{chats, total}`.
    pub fn search_all(&self, query: String, limit: u32) -> Result<String, ChatStoreError> {
        Ok(serde_json::to_string(&self.store.search_all(&query, limit as usize)?).unwrap_or_default())
    }

    /// RETENTION: drops every non-prose event older than `window_days`
    /// (`None` = keep everything); returns how many went.
    pub fn apply_retention(&self, now_ms: i64, window_days: Option<u32>) -> Result<u64, ChatStoreError> {
        Ok(self.store.apply_retention(now_ms, window_days)? as u64)
    }

    pub fn meta(&self, key: String) -> Result<Option<String>, ChatStoreError> {
        Ok(self.store.meta(&key)?)
    }

    pub fn set_meta(&self, key: String, value: String) -> Result<(), ChatStoreError> {
        Ok(self.store.set_meta(&key, &value)?)
    }

    /// THE HOLDER: the `history_chunk` (JSON) answering a device-naming
    /// `history_page` of `session_id` off this store (`fullTo` = its own
    /// `completeTo`), or the refusal `access` says; `None` = not such a page.
    pub fn answer_page(&self, session_id: String, page_json: String, access: PageAccess) -> Result<Option<String>, ChatStoreError> {
        let frame: Value = parse(&page_json)?;
        let Some(page) = catchup::parse_page(&frame) else { return Ok(None) };
        let code = match access {
            PageAccess::Allowed => None,
            PageAccess::Unauthorized => Some("unauthorized"),
            PageAccess::NoSuchSession => Some("no_such_session"),
        };
        if let Some(code) = code {
            return Ok(catchup::refusal_frame(&frame, code));
        }
        let (answer, events) = self.store.answer_page(&session_id, &page.device_id, &page.ask)?;
        let events: Vec<(u64, &Value)> = events.iter().map(|event| (event.seq, &event.event)).collect();
        Ok(Some(catchup::chunk_frame(&page.request_id, &page.device_id, &events, &answer, None, catchup::PAGE_FRAME_MAX)))
    }
}

// ---------------------------------------------------------------------------
// The catch-up walk (sans I/O)
// ---------------------------------------------------------------------------

/// One chat session the account reads (a synced `sessions` row).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CatchUpSession {
    pub session_id: String,
    pub root_id: String,
    /// The device the session is on now.
    pub device_id: String,
    /// The row's `updatedAt` (a caught-up session waits for a change).
    pub updated_at: Option<String>,
    /// A running turn: walked every pass.
    pub busy: bool,
}

/// One install the registry lists.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CatchUpInstall {
    pub endpoint_id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub last_seen_at: Option<String>,
}

/// One page to send: `frame` goes to `peer` in a steer envelope of
/// `session_id`, after `wait_ms` (the byte bucket).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CatchUpAsk {
    pub request_id: String,
    pub session_id: String,
    pub peer: String,
    pub frame: String,
    pub wait_ms: u64,
}

/// What one answer did.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CatchUpTaken {
    pub session_id: String,
    pub root_id: String,
    /// Events new to the store (a view of the chat may refresh).
    pub added: u32,
}

/// Running totals (logs, the measurements).
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CatchUpStats {
    pub passes: u64,
    pub asks: u64,
    pub events: u64,
    pub bytes: u64,
    pub caught_up: u64,
}

/// One session's walk over its holders.
struct Walk {
    session: CatchUpSession,
    holders: Vec<Holder>,
    holder: usize,
    complete: Option<u64>,
    cursor: Option<u64>,
    last: Option<u64>,
    /// This holder's walk ended; `true` = it ran to `done`.
    ended: Option<bool>,
    asking: Option<String>,
}

#[derive(Default)]
struct WalkerState {
    self_endpoint: String,
    installs: Vec<Install>,
    window_days: Option<u32>,
    queue: VecDeque<CatchUpSession>,
    walks: HashMap<String, Walk>,
    pending: HashMap<String, String>,
    bucket: Option<Bucket>,
    /// Endpoint → ms until which it is not asked.
    backoff: HashMap<String, i64>,
    /// Session → (attempts, ms of the next try): sessions readers alone served.
    retries: HashMap<String, (u32, i64)>,
    next_request: u64,
    stats: CatchUpStats,
    /// A page's `limit` (0 = `pageMax`; the fixture's walks use smaller).
    page_limit: usize,
}

/// The `catchUp` walk for one account's store: HOLDERS, PROGRESS, WALK,
/// retries, the byte bucket and `minAt` (the phone's RETENTION cutoff). The
/// app runs passes ([`CatchUp::begin_pass`]) when its OS lets it, at most
/// [`CatchUp::concurrency`] walks at once, and carries every ask.
#[derive(uniffi::Object)]
pub struct CatchUp {
    store: Arc<ChatStore>,
    own_device_id: String,
    user_id: String,
    state: Mutex<WalkerState>,
}

fn checked_key(session_id: &str) -> String {
    format!("catchup.checked.{session_id}")
}

fn stamp(session: &CatchUpSession) -> String {
    format!("{}|{}", session.device_id, session.updated_at.as_deref().unwrap_or_default())
}

#[uniffi::export]
impl CatchUp {
    #[uniffi::constructor]
    pub fn new(store: Arc<ChatStore>, own_device_id: String, user_id: String, window_days: Option<u32>) -> Arc<CatchUp> {
        let state = WalkerState { window_days, ..WalkerState::default() };
        Arc::new(CatchUp { store, own_device_id, user_id, state: Mutex::new(state) })
    }

    /// `concurrency`: walks at once.
    pub fn concurrency(&self) -> u32 {
        catchup::CONCURRENCY as u32
    }

    /// `passIntervalMs`.
    pub fn pass_interval_ms(&self) -> u64 {
        catchup::PASS_INTERVAL_MS
    }

    /// `askTimeoutMs`: an ask without its chunk by then is a failed one.
    pub fn ask_timeout_ms(&self) -> u64 {
        catchup::ASK_TIMEOUT_MS
    }

    /// This install's eNet endpoint (never asked) and the registry's installs.
    pub fn set_installs(&self, self_endpoint: String, installs: Vec<CatchUpInstall>) {
        let mut state = lock(&self.state);
        state.self_endpoint = self_endpoint;
        state.installs = installs
            .into_iter()
            .map(|i| Install { endpoint_id: i.endpoint_id, user_id: i.user_id, device_id: i.device_id, last_seen_at: i.last_seen_at })
            .collect();
    }

    /// The storage setting moved (`None` = keep everything).
    pub fn set_window_days(&self, window_days: Option<u32>) {
        lock(&self.state).window_days = window_days;
    }

    /// Starts a pass over the account's chats: queues every session due
    /// (another device's, not waiting on a retry, changed since it was last
    /// caught up or busy), newest chat first. Returns how many.
    pub fn begin_pass(&self, sessions: Vec<CatchUpSession>, now_ms: i64) -> u32 {
        let mut sessions: Vec<CatchUpSession> = sessions
            .into_iter()
            .filter(|session| !session.device_id.is_empty() && session.device_id != self.own_device_id)
            .collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.session_id.cmp(&b.session_id)));
        let mut state = lock(&self.state);
        let due: VecDeque<CatchUpSession> = sessions
            .into_iter()
            .filter(|session| !state.walks.contains_key(&session.session_id))
            .filter(|session| state.retries.get(&session.session_id).is_none_or(|(_, next)| *next <= now_ms))
            .filter(|session| {
                session.busy || self.store.store.meta(&checked_key(&session.session_id)).ok().flatten() != Some(stamp(session))
            })
            .collect();
        state.stats.passes += 1;
        let count = due.len() as u32;
        state.queue = due;
        count
    }

    /// The next session to walk (`None` = the pass has nothing left to start).
    pub fn next_walk(&self, now_ms: i64) -> Option<String> {
        let mut state = lock(&self.state);
        let session = state.queue.pop_front()?;
        state.backoff.retain(|_, until| *until > now_ms);
        let skip: Vec<String> = state.backoff.keys().cloned().collect();
        let holders = catchup::holders(&state.installs, &session.device_id, &state.self_endpoint, &self.user_id, &skip);
        let complete = self.store.store.complete_to(&session.session_id, &session.device_id).ok().flatten();
        let id = session.session_id.clone();
        state.walks.insert(
            id.clone(),
            Walk { session, holders, holder: 0, complete, cursor: complete, last: None, ended: None, asking: None },
        );
        Some(id)
    }

    /// The next page of `session_id`'s walk (`None` = the walk is over: its
    /// outcome is recorded).
    pub fn next_ask(&self, session_id: String, now_ms: i64) -> Option<CatchUpAsk> {
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        let walk = state.walks.get_mut(&session_id)?;
        loop {
            if let Some(finished) = walk.ended.take() {
                let runner = walk.holders.get(walk.holder).is_some_and(|holder| holder.runner);
                if finished && runner && catchup::caught_up(walk.complete, walk.last) {
                    let walk = state.walks.remove(&session_id)?;
                    self.finish(state, walk, true, now_ms);
                    return None;
                }
                walk.holder += 1;
                walk.cursor = walk.complete;
                walk.last = None;
            }
            if walk.holder >= walk.holders.len() {
                let walk = state.walks.remove(&session_id)?;
                self.finish(state, walk, false, now_ms);
                return None;
            }
            break;
        }
        let (sid, did) = (walk.session.session_id.clone(), walk.session.device_id.clone());
        let have = self.store.store.have_runs(&sid, &did, walk.cursor, catchup::HAVE_RUNS_MAX).unwrap_or_default();
        let min_at = state.window_days.map(|days| now_ms - i64::from(days) * DAY_MS);
        let limit = if state.page_limit == 0 { catchup::PAGE_MAX } else { state.page_limit };
        let ask = Ask { after_seq: walk.cursor, limit, have, min_at };
        state.next_request += 1;
        let request_id = format!("cu-{now_ms:x}-{}", state.next_request);
        walk.asking = Some(request_id.clone());
        let peer = walk.holders[walk.holder].endpoint_id.clone();
        state.pending.insert(request_id.clone(), sid.clone());
        state.stats.asks += 1;
        let wait_ms = state.bucket.get_or_insert_with(|| Bucket::new(now_ms)).wait_ms(now_ms);
        let frame = catchup::page_frame(&request_id, &did, &ask).to_string();
        Some(CatchUpAsk { request_id, session_id: sid, peer, frame, wait_ms })
    }

    /// The chunk answering `request_id` (`None` = no answer in time, or the
    /// peer could not be reached): stores its events, moves the watermark.
    /// `None` back = not a pending ask.
    pub fn take_answer(&self, request_id: String, chunk_json: Option<String>, now_ms: i64) -> Option<CatchUpTaken> {
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        let session_id = state.pending.remove(&request_id)?;
        let walk = state.walks.get_mut(&session_id)?;
        if walk.asking.as_deref() != Some(request_id.as_str()) {
            return None;
        }
        walk.asking = None;
        let mut taken = CatchUpTaken { session_id: session_id.clone(), root_id: walk.session.root_id.clone(), added: 0 };
        let peer = walk.holders[walk.holder].endpoint_id.clone();
        let page = chunk_json
            .ok_or_else(|| "no answer".to_string())
            .and_then(|json| {
                let frame: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
                catchup::parse_chunk(&frame, json.len())
            });
        let page: PageAnswer = match page {
            Ok(page) => page,
            Err(_) => {
                state.backoff.insert(peer, now_ms + catchup::ENDPOINT_BACKOFF_MS as i64);
                walk.ended = Some(false);
                return Some(taken);
            }
        };
        state.bucket.get_or_insert_with(|| Bucket::new(now_ms)).spend(now_ms, page.bytes);
        state.stats.bytes += page.bytes as u64;
        let (sid, did) = (walk.session.session_id.clone(), walk.session.device_id.clone());
        if !page.events.is_empty() {
            let events: Vec<StoredEvent> = page
                .events
                .iter()
                .map(|(seq, event)| StoredEvent {
                    root_id: walk.session.root_id.clone(),
                    session_id: sid.clone(),
                    device_id: did.clone(),
                    seq: *seq,
                    stored_at: now_ms,
                    event: event.clone(),
                })
                .collect();
            match self.store.store.put_events(&events) {
                Ok(added) => {
                    taken.added = added as u32;
                    state.stats.events += added as u64;
                }
                Err(_) => {
                    walk.ended = Some(false);
                    walk.holder = walk.holders.len();
                    return Some(taken);
                }
            }
        }
        let answer = &page.answer;
        walk.last = answer.last_seq;
        let (complete, next) = catchup::progress(walk.complete, walk.cursor, answer.through_seq, answer.done, answer.full_to);
        if complete != walk.complete {
            if let Some(seq) = complete {
                let _ = self.store.store.set_complete_to(&sid, &did, seq);
            }
            walk.complete = complete;
        }
        match next {
            Some(after) if after != walk.cursor || !page.events.is_empty() => walk.cursor = after,
            // Done, or a holder that does not move: done with it.
            _ => walk.ended = Some(true),
        }
        Some(taken)
    }

    /// Drops every walk (the OS ended the background window, the account
    /// left): watermarks stay, so the next pass resumes there.
    pub fn stop(&self) {
        let mut state = lock(&self.state);
        state.queue.clear();
        state.walks.clear();
        state.pending.clear();
    }

    pub fn stats(&self) -> CatchUpStats {
        lock(&self.state).stats.clone()
    }
}

impl CatchUp {
    fn finish(&self, state: &mut WalkerState, walk: Walk, caught_up: bool, now_ms: i64) {
        let id = walk.session.session_id.clone();
        if caught_up {
            state.retries.remove(&id);
            state.stats.caught_up += 1;
            if !walk.session.busy {
                let _ = self.store.store.set_meta(&checked_key(&id), &stamp(&walk.session));
            }
        } else {
            let attempts = state.retries.get(&id).map_or(0, |(attempts, _)| attempts + 1);
            state.retries.insert(id, (attempts, now_ms + catchup::retry_ms(attempts) as i64));
        }
    }
}

#[cfg(test)]
mod tests;
