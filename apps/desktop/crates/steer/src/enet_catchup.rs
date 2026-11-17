//! #EDE-108 S2 BACKGROUND CATCH-UP over eNet (`catchUp` in
//! `packages/domain-contract/fixtures/chat-store.json`; the pure rules are
//! `chat_store::catchup`). Transcripts travel device to device only: never
//! the server, never the sealed relay tunnel.
//!
//! * HOLDER half ([`serve`], from `enet_steer::serve_envelope`): a
//!   `history_page` that names a `deviceId` is answered without a join, to
//!   the asker only, with one `history_chunk`: off this device's JOURNAL when
//!   it ran the session (`fullTo` = its last seq), else off its CHAT STORE
//!   (`fullTo` = the store's own `completeTo`), under the join's access rule.
//! * ASKER half ([`ask_page`]): sends one such page and awaits its chunk
//!   (routed by `requestId` ahead of any viewer subscription).
//! * The LOOP ([`spawn`]): for every chat the account reads, walks the
//!   events it lacks from the runner, then the account's other devices,
//!   [`CONCURRENCY`] sessions at once, through a byte bucket, idle-only on
//!   battery, resuming from the store's watermarks after a restart.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use chat_store::catchup::{Power, CONCURRENCY};
use chat_store::catchup::{self, chunk_frame, Answer, Ask, Install, PageAsk};
pub use chat_store::catchup::{page_frame, PageAnswer};
pub(crate) use chat_store::catchup::is_page;
use chat_store::catchup::parse_page;
#[cfg(test)]
use chat_store::catchup::PAGE_FRAME_MAX;
use chat_store::sqlite::SqliteStore;
use chat_store::StoredEvent;
use enet_core::{EndpointId, Node};
use serde_json::{json, Value};

use crate::enet::InstallNode;
use crate::enet_steer::{FeedPeer, StoredJournals};

/// The chat store a holder answers from (set once the account's store is
/// open; shared with the node's [`StoredJournals`]).
pub type ChatStoreSlot = Arc<Mutex<Option<Arc<SqliteStore>>>>;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

// ---------------------------------------------------------------------------
// Holder
// ---------------------------------------------------------------------------

/// THE HOLDER: answers `frames` (catch-up pages of `session_id` from `peer`).
/// `audience` = the live room's, else the synced row's (`stored`); `user_of`
/// resolves the asker's user in a team. Only eNet peers are served.
pub(crate) fn serve(
    peer: &FeedPeer,
    session_id: &str,
    frames: Vec<Value>,
    audience: Option<coding::SteerAudience>,
    user_of: &dyn Fn(&str) -> Option<String>,
    stored: Option<&StoredJournals>,
) {
    if !matches!(peer, FeedPeer::Enet(_)) {
        log::debug!("enet catch-up: a page over the relay tunnel for {session_id}; ignored (eNet only)");
        return;
    }
    let audience = audience.or_else(|| stored.and_then(|stored| (stored.session)(session_id)).map(|row| row.audience));
    for frame in frames {
        let Some(page) = parse_page(&frame) else { continue };
        let refusal = match &audience {
            None => Some(crate::enet_steer::ERROR_NO_SUCH_SESSION),
            Some(audience) => {
                let user = user_of(&audience.team_id);
                (!crate::enet_steer::viewer_allowed(user.as_deref(), &audience.starter_user_id, audience.shared))
                    .then_some(crate::enet_steer::ERROR_UNAUTHORIZED)
            }
        };
        let peer = peer.clone();
        let session_id = session_id.to_string();
        let source = stored.map(|stored| (stored.data_dir.clone(), stored.device_id.clone(), stored.chat_store.clone()));
        tokio::spawn(async move {
            let frame = match refusal {
                Some(code) => {
                    log::info!("enet catch-up: {} refused for {session_id} ({code})", peer.label());
                    chunk_frame(&page.request_id, &page.device_id, &[], &empty(&page.ask), Some(code))
                }
                None => {
                    let id = session_id.clone();
                    tokio::task::spawn_blocking(move || answer_frame(&id, &page, source))
                        .await
                        .unwrap_or_default()
                }
            };
            if frame.is_empty() {
                return;
            }
            if let Err(err) = peer.send(&session_id, &[frame]).await {
                log::debug!("enet catch-up: answering {} failed: {err}", peer.label());
            }
        });
    }
}

fn empty(ask: &Ask) -> Answer {
    catchup::finish(Vec::new(), ask.limit(), None, None, ask)
}

type Source = (std::path::PathBuf, Option<String>, Option<ChatStoreSlot>);

/// The holder's chunk for one page (blocking: a file or the store is read).
fn answer_frame(session_id: &str, page: &PageAsk, source: Option<Source>) -> String {
    let (answer, events) = held_page(session_id, &page.device_id, &page.ask, source);
    chunk_frame(&page.request_id, &page.device_id, &events, &answer, None)
}

/// The page off the journal (this device ran it) or the chat store.
pub fn held_page(session_id: &str, device_id: &str, ask: &Ask, source: Option<Source>) -> (Answer, Vec<(u64, Value)>) {
    let Some((data_dir, own_device, store)) = source else { return (empty(ask), Vec::new()) };
    if own_device.as_deref() == Some(device_id) {
        if let Some(page) = journal_page(&data_dir, session_id, ask) {
            return page;
        }
    }
    let store = store.and_then(|slot| lock(&slot).clone());
    match store.map(|store| store.answer_page(session_id, device_id, ask)) {
        Some(Ok((answer, events))) => (answer, events.into_iter().map(|event| (event.seq, event.event)).collect()),
        Some(Err(err)) => {
            log::warn!("enet catch-up: reading the store for {session_id} failed: {err}");
            (empty(ask), Vec::new())
        }
        None => (empty(ask), Vec::new()),
    }
}

/// The runner's page off its journal file (`None` = no file here).
fn journal_page(data_dir: &Path, session_id: &str, ask: &Ask) -> Option<(Answer, Vec<(u64, Value)>)> {
    let wire = journal_read(data_dir, session_id)?;
    // Kind and time matter only to a retention cutoff (phones, the web).
    let held = wire.iter().map(|(seq, event)| {
        let kind = event.get("kind").and_then(Value::as_str).unwrap_or_default();
        (*seq, kind, event.get("at").and_then(Value::as_i64).unwrap_or(i64::MAX))
    });
    let last = wire.last().map(|(seq, _)| *seq);
    let answer = catchup::answer(held, last, ask);
    let taken: HashSet<u64> = answer.seqs.iter().copied().collect();
    let page = wire.iter().filter(|(seq, _)| taken.contains(seq)).cloned().collect();
    Some((answer, page))
}

type JournalRead = (std::path::PathBuf, u64, Option<SystemTime>, Arc<Vec<(u64, Value)>>);

/// The journal as the wire carries it, read once per file version: a walk
/// asks page after page of the same file (a 16 MiB journal is ~85 pages).
fn journal_read(data_dir: &Path, session_id: &str) -> Option<Arc<Vec<(u64, Value)>>> {
    static LAST: OnceLock<Mutex<Option<JournalRead>>> = OnceLock::new();
    let path = crate::history::journal_path(data_dir, session_id)?;
    let meta = std::fs::metadata(&path).ok().filter(|meta| meta.is_file())?;
    let (len, modified) = (meta.len(), meta.modified().ok());
    let last = LAST.get_or_init(Default::default);
    if let Some((p, l, m, read)) = lock(last).as_ref() {
        if *p == path && *l == len && *m == modified {
            return Some(Arc::clone(read));
        }
    }
    let events = crate::history::read_journal_seq(data_dir, session_id)?;
    let wire: Arc<Vec<(u64, Value)>> =
        Arc::new(events.iter().map(|(seq, event)| (*seq, serde_json::to_value(event).unwrap_or_default())).collect());
    *lock(last) = Some((path, len, modified, Arc::clone(&wire)));
    Some(wire)
}

// ---------------------------------------------------------------------------
// Asker
// ---------------------------------------------------------------------------

type Pending = HashMap<String, flume::Sender<(Value, usize)>>;

fn pending() -> &'static Mutex<Pending> {
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    PENDING.get_or_init(Default::default)
}

/// The viewer half's router: takes the chunks that answer a pending page
/// out of `frames` (they never reach a viewer subscription).
pub(crate) fn take_answers(frames: &mut Vec<Value>) {
    let mut pending = lock(pending());
    if pending.is_empty() {
        return;
    }
    frames.retain(|frame| {
        if frame.get("t").and_then(Value::as_str) != Some("history_chunk") {
            return true;
        }
        let Some(id) = frame.get("requestId").and_then(Value::as_str) else { return true };
        match pending.remove(id) {
            Some(tx) => {
                let bytes = frame.to_string().len();
                let _ = tx.send((frame.clone(), bytes));
                false
            }
            None => true,
        }
    });
}

/// Asks `peer` for one page of `session_id`'s events on `device_id`.
pub async fn ask_page(node: &Node, peer: EndpointId, session_id: &str, device_id: &str, ask: &Ask) -> Result<PageAnswer, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = flume::bounded(1);
    lock(pending()).insert(request_id.clone(), tx);
    let frame = page_frame(&request_id, device_id, ask).to_string();
    let result = async {
        crate::enet_steer::send_envelope(node, peer, session_id, &[frame]).await?;
        match tokio::time::timeout(Duration::from_millis(catchup::ASK_TIMEOUT_MS), rx.recv_async()).await {
            Ok(Ok(answer)) => Ok(answer),
            _ => Err("no answer in time".to_string()),
        }
    }
    .await;
    lock(pending()).remove(&request_id);
    let (frame, bytes) = result?;
    catchup::parse_chunk(&frame, bytes)
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

/// One chat session the account reads, as the loop sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct CatchUpSession {
    pub session_id: String,
    pub root_id: String,
    /// The device the session is on now (its events' key).
    pub device_id: String,
    /// The synced row's `updatedAt` (a caught-up session waits for a change).
    pub updated_at: Option<String>,
    /// A running turn: walked every pass.
    pub busy: bool,
}

/// What the loop needs.
#[derive(Clone)]
pub struct CatchUpDeps {
    pub install: InstallNode,
    pub store: Arc<SqliteStore>,
    pub own_device_id: String,
    pub user_id: String,
    /// The account's chats (the synced `sessions` rows), any thread.
    pub sessions: Arc<dyn Fn() -> Vec<CatchUpSession> + Send + Sync>,
    /// POWER: whether a pass may run now ([`may_run_now`]).
    pub may_run: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Told the sessions whose stored events grew (a view may refresh).
    pub on_stored: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

/// Running totals (logs, the throughput the tests measure).
#[derive(Debug, Default)]
pub struct CatchUpStats {
    pub passes: AtomicU64,
    pub asks: AtomicU64,
    pub events: AtomicU64,
    pub bytes: AtomicU64,
    pub caught_up: AtomicU64,
}

/// The running loop; dropping it stops it.
pub struct CatchUpHandle {
    kick: Arc<tokio::sync::Notify>,
    stopped: Arc<AtomicBool>,
    pub stats: Arc<CatchUpStats>,
}

impl CatchUpHandle {
    /// Run a pass now (the account's sessions changed).
    pub fn kick(&self) {
        self.kick.notify_one();
    }
}

impl Drop for CatchUpHandle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.kick.notify_one();
    }
}

/// Starts the loop on `runtime`.
pub fn spawn(runtime: &tokio::runtime::Handle, deps: CatchUpDeps) -> CatchUpHandle {
    let kick = Arc::new(tokio::sync::Notify::new());
    let stopped = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(CatchUpStats::default());
    let state = Arc::new(Loop {
        deps,
        bucket: tokio::sync::Mutex::new(catchup::Bucket::new(now_ms())),
        endpoint_backoff: Mutex::new(HashMap::new()),
        retries: Mutex::new(HashMap::new()),
        stats: Arc::clone(&stats),
        stopped: Arc::clone(&stopped),
    });
    runtime.spawn(run_loop(state, Arc::clone(&kick)));
    CatchUpHandle { kick, stopped, stats }
}

struct Loop {
    deps: CatchUpDeps,
    bucket: tokio::sync::Mutex<catchup::Bucket>,
    /// Endpoint → until when it is not asked.
    endpoint_backoff: Mutex<HashMap<String, Instant>>,
    /// Session → (attempts, next try) for sessions readers alone served.
    retries: Mutex<HashMap<String, (u32, Instant)>>,
    stats: Arc<CatchUpStats>,
    stopped: Arc<AtomicBool>,
}

/// How a walk ended.
#[derive(Debug, PartialEq, Eq)]
enum Walked {
    CaughtUp,
    Partial,
}

async fn run_loop(state: Arc<Loop>, kick: Arc<tokio::sync::Notify>) {
    let interval = Duration::from_millis(catchup::PASS_INTERVAL_MS);
    // Power and idleness are looked at again this often while a pass waits.
    let gate_poll = Duration::from_secs(60);
    loop {
        if state.stopped.load(Ordering::SeqCst) {
            return;
        }
        if state.deps.install.node().is_none() || !(state.deps.may_run)() {
            let _ = tokio::time::timeout(gate_poll, kick.notified()).await;
            continue;
        }
        let started = Instant::now();
        let (events, bytes) = (state.stats.events.load(Ordering::Relaxed), state.stats.bytes.load(Ordering::Relaxed));
        let walked = pass(&state).await;
        state.stats.passes.fetch_add(1, Ordering::Relaxed);
        if walked > 0 {
            let events = state.stats.events.load(Ordering::Relaxed) - events;
            let bytes = state.stats.bytes.load(Ordering::Relaxed) - bytes;
            let ms = started.elapsed().as_millis().max(1);
            log::info!(
                "[catch-up] pass: {walked} session(s), {events} event(s), {bytes} byte(s) in {ms} ms ({} events/s)",
                u128::from(events) * 1000 / ms
            );
        }
        let _ = tokio::time::timeout(interval, kick.notified()).await;
    }
}

fn checked_key(session_id: &str) -> String {
    format!("catchup.checked.{session_id}")
}

fn stamp(session: &CatchUpSession) -> String {
    format!("{}|{}", session.device_id, session.updated_at.as_deref().unwrap_or_default())
}

/// One pass: every session due, newest chat first, [`CONCURRENCY`] at once.
/// Returns how many it walked.
async fn pass(state: &Arc<Loop>) -> usize {
    let mut sessions: Vec<CatchUpSession> = (state.deps.sessions)()
        .into_iter()
        .filter(|session| !session.device_id.is_empty() && session.device_id != state.deps.own_device_id)
        .collect();
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.session_id.cmp(&b.session_id)));
    let store = Arc::clone(&state.deps.store);
    let due: Vec<CatchUpSession> = {
        let retries = lock(&state.retries);
        let now = Instant::now();
        sessions
            .into_iter()
            .filter(|session| retries.get(&session.session_id).is_none_or(|(_, next)| *next <= now))
            .filter(|session| session.busy || store.meta(&checked_key(&session.session_id)).ok().flatten() != Some(stamp(session)))
            .collect()
    };
    let walked = due.len();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut tasks = Vec::new();
    for session in due {
        if state.stopped.load(Ordering::SeqCst) {
            break;
        }
        let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else { break };
        let state = Arc::clone(state);
        tasks.push(tokio::spawn(async move {
            let walked = walk(&state, &session).await;
            let mut retries = lock(&state.retries);
            match walked {
                Walked::CaughtUp => {
                    retries.remove(&session.session_id);
                    state.stats.caught_up.fetch_add(1, Ordering::Relaxed);
                    if !session.busy {
                        let _ = state.deps.store.set_meta(&checked_key(&session.session_id), &stamp(&session));
                    }
                }
                Walked::Partial => {
                    let attempts = retries.get(&session.session_id).map_or(0, |(attempts, _)| attempts + 1);
                    let next = Instant::now() + Duration::from_millis(catchup::retry_ms(attempts));
                    retries.insert(session.session_id.clone(), (attempts, next));
                }
            }
            drop(permit);
        }));
    }
    for task in tasks {
        let _ = task.await;
    }
    walked
}

/// WALK one session over its holders (`walkCases`).
async fn walk(state: &Loop, session: &CatchUpSession) -> Walked {
    let Some(node) = state.deps.install.node() else { return Walked::Partial };
    let installs: Vec<Install> = state
        .deps
        .install
        .installs()
        .into_iter()
        .map(|entry| Install { endpoint_id: entry.endpoint_id, user_id: entry.user_id, device_id: entry.device_id, last_seen_at: entry.last_seen_at })
        .collect();
    let skip: Vec<String> = {
        let mut backoff = lock(&state.endpoint_backoff);
        let now = Instant::now();
        backoff.retain(|_, until| *until > now);
        backoff.keys().cloned().collect()
    };
    let self_endpoint = crate::enet::endpoint_hex(&node.endpoint_id());
    let holders = catchup::holders(&installs, &session.device_id, &self_endpoint, &state.deps.user_id, &skip);
    let store = &state.deps.store;
    let (sid, did) = (session.session_id.as_str(), session.device_id.as_str());
    let mut complete = store.complete_to(sid, did).ok().flatten();
    for holder in holders {
        let Some(peer) = crate::enet::parse_endpoint(&holder.endpoint_id) else { continue };
        let mut cursor = complete;
        let mut last = None;
        let finished = loop {
            if state.stopped.load(Ordering::SeqCst) {
                return Walked::Partial;
            }
            let wait = state.bucket.lock().await.wait_ms(now_ms());
            if wait > 0 {
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
            let have = store.have_runs(sid, did, cursor, catchup::HAVE_RUNS_MAX).unwrap_or_default();
            let ask = Ask { after_seq: cursor, limit: catchup::PAGE_MAX, have, min_at: None };
            state.stats.asks.fetch_add(1, Ordering::Relaxed);
            let page = match ask_page(&node, peer, sid, did, &ask).await {
                Ok(page) => page,
                Err(err) => {
                    log::debug!("[catch-up] {} did not answer {sid}: {err}", holder.endpoint_id);
                    let until = Instant::now() + Duration::from_millis(catchup::ENDPOINT_BACKOFF_MS);
                    lock(&state.endpoint_backoff).insert(holder.endpoint_id.clone(), until);
                    break false;
                }
            };
            state.bucket.lock().await.spend(now_ms(), page.bytes);
            state.stats.bytes.fetch_add(page.bytes as u64, Ordering::Relaxed);
            if !page.events.is_empty() {
                let stored_at = now_ms();
                let events: Vec<StoredEvent> = page
                    .events
                    .iter()
                    .map(|(seq, event)| StoredEvent {
                        root_id: session.root_id.clone(),
                        session_id: sid.to_string(),
                        device_id: did.to_string(),
                        seq: *seq,
                        stored_at,
                        event: event.clone(),
                    })
                    .collect();
                match store.put_events(&events) {
                    Ok(added) => {
                        state.stats.events.fetch_add(added as u64, Ordering::Relaxed);
                        if added > 0 {
                            if let Some(on_stored) = &state.deps.on_stored {
                                on_stored(sid);
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!("[catch-up] storing {sid}'s page failed: {err}");
                        return Walked::Partial;
                    }
                }
            }
            let answer = &page.answer;
            last = answer.last_seq;
            let (next_complete, next) = catchup::progress(complete, cursor, answer.through_seq, answer.done, answer.full_to);
            if next_complete != complete {
                if let Some(seq) = next_complete {
                    let _ = store.set_complete_to(sid, did, seq);
                }
                complete = next_complete;
            }
            match next {
                Some(after) if after != cursor || !page.events.is_empty() => cursor = after,
                // A holder that does not move: done with it.
                Some(_) => break true,
                None => break true,
            }
        };
        if finished && holder.runner && catchup::caught_up(complete, last) {
            return Walked::CaughtUp;
        }
    }
    Walked::Partial
}

/// POWER on this machine: Linux reads `/sys/class/power_supply`, macOS asks
/// `pmset`; elsewhere unknown (treated as mains).
pub fn power() -> Power {
    #[cfg(target_os = "linux")]
    {
        let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else { return Power::Unknown };
        let (mut battery, mut discharging, mut mains) = (false, false, false);
        for entry in entries.flatten() {
            let read = |name: &str| std::fs::read_to_string(entry.path().join(name)).unwrap_or_default().trim().to_string();
            match read("type").as_str() {
                "Battery" => {
                    battery = true;
                    discharging |= read("status") == "Discharging";
                }
                "Mains" | "USB" => mains |= read("online") == "1",
                _ => {}
            }
        }
        if !battery {
            return Power::Unknown;
        }
        if discharging && !mains {
            Power::Battery
        } else {
            Power::Ac
        }
    }
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("pmset").args(["-g", "ps"]).output() {
            Ok(out) if String::from_utf8_lossy(&out.stdout).contains("'Battery Power'") => Power::Battery,
            Ok(out) if out.status.success() => Power::Ac,
            _ => Power::Unknown,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Power::Unknown
    }
}

/// [`catchup::may_run`] with this machine's power and the person's idle time.
pub fn may_run_now(idle_ms: u64) -> bool {
    catchup::may_run(power(), idle_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_round_trips_through_its_frame() {
        let ask = Ask { after_seq: Some(4), limit: 50, have: vec![[6, 9]], min_at: None };
        let frame = page_frame("r1", "d1", &ask);
        assert!(is_page(&frame));
        let parsed = parse_page(&frame).unwrap();
        assert_eq!((parsed.request_id.as_str(), parsed.device_id.as_str()), ("r1", "d1"));
        assert_eq!(parsed.ask, ask);
        // A viewer's page (no device) is not a catch-up page.
        assert!(!is_page(&json!({ "t": "history_page", "requestId": "x", "beforeSeq": 9, "limit": 5 })));
    }

    #[test]
    fn an_oversize_page_is_cut_from_its_end() {
        // The shared frames size against this envelope.
        assert_eq!(catchup::CHAT_MAX_BYTES, crate::enet_steer::CHAT_MAX_BYTES);
        let big = "x".repeat(PAGE_FRAME_MAX / 3);
        let events: Vec<(u64, Value)> = (0..5).map(|seq| (seq, json!({ "kind": "narration", "text": big }))).collect();
        let answer = Answer { seqs: (0..5).collect(), through_seq: Some(4), done: true, last_seq: Some(4), full_to: Some(4) };
        let frame: Value = serde_json::from_str(&chunk_frame("r", "d", &events, &answer, None)).unwrap();
        assert!(frame.to_string().len() < PAGE_FRAME_MAX);
        assert_eq!(frame["seqs"], json!([0, 1]));
        assert_eq!((frame["throughSeq"].clone(), frame["done"].clone()), (json!(1), json!(false)));
        // One event over the envelope alone is skipped, the walk moves on.
        let huge = vec![(7, json!({ "kind": "narration", "text": "x".repeat(PAGE_FRAME_MAX) }))];
        let answer = Answer { seqs: vec![7], through_seq: Some(7), done: true, last_seq: Some(9), full_to: Some(9) };
        let frame: Value = serde_json::from_str(&chunk_frame("r", "d", &huge, &answer, None)).unwrap();
        assert_eq!((frame["seqs"].clone(), frame["throughSeq"].clone(), frame["done"].clone()), (json!([]), json!(7), json!(false)));
    }

    #[test]
    fn answers_are_routed_by_request_id_only() {
        let (tx, rx) = flume::bounded(1);
        lock(pending()).insert("mine".into(), tx);
        let mut frames = vec![
            json!({ "t": "history_chunk", "requestId": "mine", "seqs": [], "events": [], "done": true }),
            json!({ "t": "history_chunk", "requestId": "a-viewers", "seqs": [], "events": [], "done": true }),
            json!({ "t": "keepalive" }),
        ];
        take_answers(&mut frames);
        assert_eq!(frames.len(), 2);
        assert_eq!(rx.try_recv().unwrap().0["requestId"], "mine");
    }
}
