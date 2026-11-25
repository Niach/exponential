//! #EXP-2138 (`fixtures/held-gone.json`): deleting a chat, its agent or its
//! team removes what THIS DEVICE holds of it. The host half of
//! [`domain::held_gone`], shared by the desktop app and the CLI daemon: it
//! asks `sessions.heldGone` (once signed in and synced, then every
//! [`held_gone::ASK_EVERY_HOURS`]; a delete's kill asks at once), keeps the
//! answers in `accounts/<account>/held-gone.json` and removes what the rules
//! say. A row missing from sync removes nothing, and nothing but this, the
//! `Keep session history` window and a sign-out does.
//!
//! The file is per ACCOUNT (its id names the instance and the user), so a
//! first answer never counts for another login; the desktop app and the
//! daemon on one data dir share it under its lock.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use api::error::ApiError;
use api::trpc::TrpcClient;
use chat_store::sqlite::SqliteStore;
use domain::held_gone::{self, GoneAnswers, HeldAnswer, HeldRecord};
use serde::{Deserialize, Serialize};

use crate::publisher::{control, is_publishing, ControlCmd};

/// How often the thread looks whether the account is synced yet.
const READY_POLL: Duration = Duration::from_secs(5);
/// A failed ask (offline, an older server) is tried again after this.
const ASK_RETRY: Duration = Duration::from_secs(15 * 60);
/// Removals that could not finish (a live run, a file that would not go) and
/// notices nobody confirmed yet.
const NOTICE_RETRY: Duration = Duration::from_secs(60);
/// A deleted tree sends one kill per session: they are asked about together.
const NOTICE_COALESCE: Duration = Duration::from_millis(700);
/// How long a pass waits for the deleted runs that are live here to end.
const END_WAIT: Duration = Duration::from_secs(20);

/// `sessions.heldGone`, injectable for tests. Blocking.
pub trait HeldGoneApi: Send + Sync + 'static {
    fn gone(&self, ids: &[String]) -> Result<HeldAnswer, ApiError>;
}

impl HeldGoneApi for Arc<TrpcClient> {
    fn gone(&self, ids: &[String]) -> Result<HeldAnswer, ApiError> {
        api::sessions::held_gone(self, ids)
    }
}

/// What a host hands over.
#[derive(Clone)]
pub struct HeldGoneDeps {
    pub data_dir: PathBuf,
    pub account_id: String,
    pub api: Arc<dyn HeldGoneApi>,
    /// Signed in and the sessions shape is up to date.
    pub ready: Arc<dyn Fn() -> bool + Send + Sync>,
    /// The account's open chat store; `None` = this host has none open (its
    /// file is opened for the pass when there is one).
    pub store: Arc<dyn Fn() -> Option<Arc<SqliteStore>> + Send + Sync>,
}

/// Stops the account's thread (sign-out, account switch, quit).
pub struct HeldGoneHandle {
    stopped: Arc<AtomicBool>,
    account_id: String,
    wake: flume::Sender<String>,
}

impl HeldGoneHandle {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let mut slots = lock(slots());
        if slots.get(&self.account_id).is_some_and(|sender| sender.same_channel(&self.wake)) {
            slots.remove(&self.account_id);
        }
        // Wakes the thread so it sees the flag.
        let _ = self.wake.send(String::new());
    }
}

impl Drop for HeldGoneHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn slots() -> &'static Mutex<HashMap<String, flume::Sender<String>>> {
    static SLOTS: OnceLock<Mutex<HashMap<String, flume::Sender<String>>>> = OnceLock::new();
    SLOTS.get_or_init(Default::default)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A delete's kill named `session_id` on `account_id`'s control socket
/// (non-blocking). It removes nothing by itself: the server is asked.
pub fn notice(account_id: &str, session_id: String) {
    if let Some(sender) = lock(slots()).get(account_id) {
        let _ = sender.send(session_id);
    }
}

/// Starts the account's thread.
pub fn spawn(deps: HeldGoneDeps) -> HeldGoneHandle {
    let (wake, inbox) = flume::unbounded::<String>();
    let stopped = Arc::new(AtomicBool::new(false));
    let handle = HeldGoneHandle { stopped: Arc::clone(&stopped), account_id: deps.account_id.clone(), wake: wake.clone() };
    lock(slots()).insert(deps.account_id.clone(), wake);
    if let Err(err) = std::thread::Builder::new().name("held-gone".into()).spawn(move || run(&deps, &inbox, &stopped)) {
        log::warn!("held-gone: no thread: {err}");
    }
    handle
}

fn run(deps: &HeldGoneDeps, inbox: &flume::Receiver<String>, stopped: &AtomicBool) {
    let mut full_at = Instant::now();
    let mut notices_at: Option<Instant> = None;
    loop {
        let now = Instant::now();
        let next = if (deps.ready)() { notices_at.map_or(full_at, |at| at.min(full_at)) } else { now + READY_POLL };
        let mut heard = Vec::new();
        match inbox.recv_timeout(next.saturating_duration_since(now)) {
            Ok(first) => {
                std::thread::sleep(NOTICE_COALESCE);
                heard = std::iter::once(first).chain(inbox.try_iter()).filter(|id| !id.is_empty()).collect();
            }
            Err(flume::RecvTimeoutError::Timeout) => {}
            Err(flume::RecvTimeoutError::Disconnected) => return,
        }
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        if !heard.is_empty() {
            deps.update(|held| held.notices.extend(heard));
            notices_at = Some(Instant::now());
        }
        if !(deps.ready)() {
            continue;
        }
        let now = Instant::now();
        if now >= full_at {
            let done = pass(deps, Ask::Everything, now_ms(), END_WAIT, stopped);
            full_at = Instant::now()
                + if done.answered { Duration::from_secs(held_gone::ASK_EVERY_HOURS * 3600) } else { ASK_RETRY };
            notices_at = done.notices_left.then(|| Instant::now() + NOTICE_RETRY);
        } else if notices_at.is_some_and(|at| now >= at) {
            let done = pass(deps, Ask::Notices, now_ms(), END_WAIT, stopped);
            notices_at = done.notices_left.then(|| Instant::now() + NOTICE_RETRY);
        }
    }
}

/// What a pass asks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask {
    Everything,
    Notices,
}

/// What a pass did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Done {
    answered: bool,
    removed: Vec<String>,
    uncached: Vec<String>,
    notices_left: bool,
}

/// `accounts/<account>/held-gone.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Held {
    #[serde(default)]
    state: BTreeMap<String, GoneAnswers>,
    /// Deletes announced and not settled yet.
    #[serde(default)]
    notices: BTreeSet<String>,
}

impl HeldGoneDeps {
    fn path(&self) -> PathBuf {
        self.data_dir.join("accounts").join(&self.account_id).join("held-gone.json")
    }

    /// An unreadable file starts over (every first answer asked again).
    fn load(&self) -> Held {
        std::fs::read_to_string(self.path()).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
    }

    /// One load-modify-save under the file's lock; nothing is written when
    /// nothing changed.
    fn update<T>(&self, change: impl FnOnce(&mut Held) -> T) -> T {
        let path = self.path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _guard = api::settings_lock::locked_at(&path.with_extension("json.lock"));
        let before = self.load();
        let mut held = before.clone();
        let out = change(&mut held);
        if held != before {
            let json = serde_json::to_string(&held).unwrap_or_default();
            if let Err(err) = api::atomic_file::write_atomic(&path, &json) {
                log::warn!("held-gone: storing the answers failed: {err}");
            }
        }
        out
    }

    fn records(&self) -> Vec<(HeldRecord, PathBuf)> {
        coding::run_registry::all(&self.data_dir)
            .into_iter()
            .map(|record| {
                let held = HeldRecord {
                    id: record.session_id.clone(),
                    account: record.account_id.clone(),
                    aside: record.is_aside(),
                    cwd: record.cwd.to_string_lossy().into_owned(),
                    scratch: coding::scratch::is_scratch_dir(&self.data_dir, &record.cwd),
                };
                (held, record.cwd)
            })
            .collect()
    }

    /// The account's chat store: the host's open one, else its file (none, or
    /// one that will not open = the run records alone).
    fn open_store(&self) -> Option<Arc<SqliteStore>> {
        if let Some(store) = (self.store)() {
            return Some(store);
        }
        let path = self.data_dir.join("accounts").join(&self.account_id).join("chat-store.sqlite");
        path.is_file().then(|| SqliteStore::open(&path).ok().map(Arc::new)).flatten()
    }

    /// Whether another account's chat store on this machine names `id` (one
    /// that cannot be read counts as naming it).
    fn another_store_names(&self, id: &str) -> bool {
        let Ok(accounts) = std::fs::read_dir(self.data_dir.join("accounts")) else { return false };
        accounts.flatten().filter(|entry| entry.file_name().to_string_lossy() != self.account_id.as_str()).any(|entry| {
            let path = entry.path().join("chat-store.sqlite");
            path.is_file() && SqliteStore::open(&path).and_then(|store| store.names(id)).unwrap_or(true)
        })
    }

    /// The whole ask, in batches; `None` when any of them failed.
    fn ask(&self, ids: &[String]) -> Option<HeldAnswer> {
        let mut whole = HeldAnswer::default();
        for batch in ids.chunks(held_gone::BATCH) {
            match self.api.gone(batch) {
                Ok(answer) => {
                    whole.gone.extend(answer.gone.into_iter().filter(|id| batch.contains(id)));
                    whole.unreadable.extend(answer.unreadable.into_iter().filter(|id| batch.contains(id)));
                }
                Err(err) => {
                    log::info!("held-gone: the ask failed, nothing changes: {err}");
                    return None;
                }
            }
        }
        Some(whole)
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

/// Whether a run of `id` is live: here, or in the other host of this data dir.
fn is_live(data_dir: &Path, id: &str) -> bool {
    is_publishing(id) || coding::run_registry::live_host_runs(data_dir).iter().any(|(session, _)| session == id)
}

/// One pass: ask, decide ([`held_gone::pass`]), remove. Blocking.
fn pass(deps: &HeldGoneDeps, ask: Ask, now: i64, end_wait: Duration, stopped: &AtomicBool) -> Done {
    let store = deps.open_store();
    let records: Vec<HeldRecord> = deps.records().into_iter().map(|(record, _)| record).collect();
    let stored = match store.as_ref().map(|store| store.session_ids()) {
        Some(Ok(ids)) => ids,
        Some(Err(err)) => {
            log::warn!("held-gone: reading the chat store failed: {err}");
            return Done { notices_left: !deps.load().notices.is_empty(), ..Default::default() };
        }
        None => Vec::new(),
    };
    let holds = held_gone::candidates(&deps.account_id, &records, &stored);
    // Only a delete announced BEFORE the ask counts as confirmed by it.
    let noticed = deps.load().notices;
    let asked: Vec<String> = match ask {
        Ask::Everything => holds.clone(),
        Ask::Notices => holds.iter().filter(|id| noticed.contains(*id)).cloned().collect(),
    };
    let answer = deps.ask(&asked);
    if stopped.load(Ordering::SeqCst) {
        return Done::default();
    }
    let decided = deps.update(|held| {
        let decided = held_gone::pass(now, &holds, &stored, &asked, answer.as_ref(), &held.state, &noticed);
        let newer: Vec<String> = held.notices.difference(&noticed).cloned().collect();
        held.state = decided.state.clone();
        held.notices = decided.notices.iter().cloned().chain(newer).collect();
        decided
    });
    if decided.capped {
        log::warn!(
            "held-gone: more than {} and more than half of the {} held sessions are gone or unreadable at once: nothing but announced deletes is removed",
            held_gone::CAP_COUNT,
            holds.len()
        );
    }
    let removed = remove_all(deps, &decided.remove, store.as_deref(), end_wait, stopped);
    // Not deleted, only not this account's to read any more: its cached rows
    // go; the run this device hosted (record, journal, scratch dir) stays.
    let uncached: Vec<String> = decided
        .uncache
        .into_iter()
        .filter(|_| !stopped.load(Ordering::SeqCst))
        .filter(|id| match store.as_ref().map(|store| store.forget_session(id)) {
            Some(Ok(())) => true,
            Some(Err(err)) => {
                log::warn!("held-gone: removing {id} from the chat store failed: {err}");
                false
            }
            None => false,
        })
        .collect();
    let notices_left = deps.update(|held| {
        for id in removed.iter().chain(&uncached) {
            held.state.remove(id);
        }
        for id in &removed {
            held.notices.remove(id);
        }
        !held.notices.is_empty()
    });
    if let Some(answer) = &answer {
        log::info!(
            "held-gone: asked about {} of {} held: {} gone, {} unreadable; removed {}, uncached {}",
            asked.len(),
            holds.len(),
            answer.gone.len(),
            answer.unreadable.len(),
            removed.len(),
            uncached.len()
        );
    }
    Done { answered: answer.is_some(), removed, uncached, notices_left }
}

/// Ends what is live of `ids`, then removes the ones that are over; the rest
/// waits for the next pass. Returns the removed ones.
fn remove_all(deps: &HeldGoneDeps, ids: &[String], store: Option<&SqliteStore>, end_wait: Duration, stopped: &AtomicBool) -> Vec<String> {
    let mut live: Vec<&String> = ids.iter().filter(|id| is_live(&deps.data_dir, id)).collect();
    for id in &live {
        let routed = control(id, ControlCmd::Kill);
        log::info!("held-gone: {id} is gone and still runs (here: {routed}); it ends first");
    }
    let deadline = Instant::now() + end_wait;
    while !live.is_empty() && Instant::now() < deadline && !stopped.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200).min(end_wait));
        live.retain(|id| is_live(&deps.data_dir, id));
    }
    let mut removed = Vec::new();
    for id in ids.iter().filter(|id| !live.contains(id)) {
        if stopped.load(Ordering::SeqCst) {
            break;
        }
        if remove_one(deps, id, store) {
            removed.push(id.clone());
        }
    }
    removed
}

/// Removes one gone session's files and rows; `false` = something would not
/// go and the next pass tries again.
fn remove_one(deps: &HeldGoneDeps, id: &str, store: Option<&SqliteStore>) -> bool {
    let all = deps.records();
    let records: Vec<HeldRecord> = all.iter().map(|(record, _)| record.clone()).collect();
    let recorded = records.iter().any(|record| record.id == id);
    let named = match store.map(|store| store.names(id)) {
        Some(Ok(named)) => named,
        Some(Err(_)) => return false,
        None => false,
    };
    let other = !recorded && named && deps.another_store_names(id);
    let goes = held_gone::removal(&deps.account_id, id, &records, named, other);
    if goes.scratch {
        if let Some((_, cwd)) = all.iter().find(|(record, _)| record.id == id) {
            coding::scratch::reclaim(&deps.data_dir, cwd, SystemTime::now());
            if cwd.exists() {
                return false;
            }
        }
    }
    if goes.journal {
        crate::history::remove_journal(&deps.data_dir, id);
        if crate::history::journal_path(&deps.data_dir, id).is_some_and(|path| path.exists()) {
            return false;
        }
    }
    if goes.store {
        if let Some(Err(err)) = store.map(|store| store.forget_session(id)) {
            log::warn!("held-gone: removing {id} from the chat store failed: {err}");
            return false;
        }
    }
    // Last: the record is what names the rest.
    if goes.record {
        coding::run_registry::remove(&deps.data_dir, id);
    }
    log::info!("held-gone: {id} was deleted: {goes:?}");
    true
}

#[cfg(test)]
mod tests;
