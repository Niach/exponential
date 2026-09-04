//! `SyncManager` — per-account pipeline reconcile (masterplan-v3 §5.10).
//! gpui-free; a direct port of the proven iOS `SyncManager.reconcile`.
//!
//! One manager owns every running sync pipeline. A pipeline is one account's
//! 16 shape threads (one dedicated `std::thread` per shape, §5.3) plus its
//! per-account rusqlite/WAL store (§5.4). Reconciling against the signed-in
//! account set:
//!
//! * **login / token refresh** → [`SyncManager::start_account`] spawns the 15
//!   threads against `{data_dir}/accounts/{id}/sync-v2.sqlite`;
//! * **logout** → [`SyncManager::stop_account`] flips the shared stop flag and
//!   joins within a short grace window; the SQLite DB stays on disk for
//!   offline resume ("Delete local data" is a separate, explicit action);
//! * **hard 401** → the pipeline tears *itself* down (the first thread to see
//!   it flips the shared stop flag and emits [`ShapeDelta::Unauthorized`]
//!   once, §5.6b); a later reconcile sweeps the dead entry.
//!
//! The outward boundary is a single `flume` channel of [`ShapeDelta`]s — the
//! collections layer (the only gpui seam, §5.8) drains it on one foreground
//! task. Nothing in this module may `use gpui`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crate::client::{
    ShapeClient, ShapeClientConfig, ShapeDelta, ShapeTransport, TokenFn, UnauthorizedFn,
    HttpTransport, UpgradeRequiredFn, UNAUTHORIZED_GRACE,
};
use crate::shapes::SHAPES;
use crate::store::{ShapeStore, StoreError};
use crate::wake::WakeWatchdog;

/// How long `stop_account` waits for the shape threads to exit before
/// detaching them. Threads check their stop flag between every sleep slice
/// and before every request, so anything not blocked in an in-flight live
/// read exits well inside this window; a thread that IS mid-read (up to the
/// 90s timeout) is detached — it discards its result (stop is re-checked
/// before apply) and exits at its next loop boundary. Quit therefore never
/// waits on a long-poll (§5.3 "quit exits in <500ms").
const STOP_GRACE: Duration = Duration::from_millis(300);

/// Everything the manager needs to run one account's pipeline. The app shell
/// builds these from the `api` crate's `AuthStore` (`token` =
/// `AuthStore::token_provider_fn(account_id)`).
pub struct AccountSyncConfig {
    pub account_id: String,
    /// Normalized instance base URL (`https://app.exponential.at`).
    pub base_url: String,
    /// Full path of the per-account SQLite file (§5.4):
    /// `{data_dir}/accounts/{account_id}/sync-v2.sqlite`.
    pub db_path: PathBuf,
    /// Call-time session-token access (§5.7) — never captured once.
    pub token: TokenFn,
    /// Shape-name subset to sync; `None` = all [`SHAPES`] (the desktop). The
    /// EXP-530 CLI daemon syncs a 6-shape subset into its own store
    /// (`sync-cli.sqlite`) — threads are spawned only for named shapes, and
    /// unknown names are ignored.
    pub shapes: Option<&'static [&'static str]>,
}

struct AccountPipeline {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    store: Arc<ShapeStore>,
    /// Retained so [`SyncManager::restart_account`] can rebuild the pipeline
    /// without the caller re-supplying credentials (EXP-470).
    base_url: String,
    db_path: PathBuf,
    token: TokenFn,
    shapes: Option<&'static [&'static str]>,
}

impl AccountPipeline {
    /// Live = told to run AND not self-torn-down (a hard 401 flips `stop`
    /// from inside the pipeline, §5.6b).
    fn is_live(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }
}

/// The per-account pipeline reconciler (§5.10). Create once, share via `Arc`.
pub struct SyncManager {
    transport: Arc<dyn ShapeTransport>,
    on_unauthorized: Option<UnauthorizedFn>,
    on_upgrade_required: Option<UpgradeRequiredFn>,
    unauthorized_grace: Duration,
    deltas_tx: flume::Sender<ShapeDelta>,
    deltas_rx: flume::Receiver<ShapeDelta>,
    pipelines: Mutex<HashMap<String, AccountPipeline>>,
    /// EXP-533: when [`SyncManager::restart_account`] last rebuilt ANY
    /// pipeline. The wake watchdog and the window-activation kick both restart
    /// on the same wake; without a stamp the second one stops threads the first
    /// just spawned and respawns all 20. Read via
    /// [`SyncManager::restarted_within`].
    last_restart_at: Mutex<Option<Instant>>,
}

impl SyncManager {
    /// Production manager: the shared blocking `reqwest`/rustls transport (§5.3), one
    /// shared connection pool across all shape threads.
    pub fn new() -> Self {
        Self::with_transport(Arc::new(HttpTransport::new()))
    }

    /// Test seam (§5.3 testing guidance): inject any [`ShapeTransport`].
    pub fn with_transport(transport: Arc<dyn ShapeTransport>) -> Self {
        let (deltas_tx, deltas_rx) = flume::unbounded();
        Self {
            transport,
            on_unauthorized: None,
            on_upgrade_required: None,
            unauthorized_grace: UNAUTHORIZED_GRACE,
            deltas_tx,
            deltas_rx,
            pipelines: Mutex::new(HashMap::new()),
            last_restart_at: Mutex::new(None),
        }
    }

    /// Wire the §5.6b 401 hook (the app shell passes
    /// `AuthStore::unauthorized_handler_fn()` — it deletes the stored token
    /// and emits the auth event that routes the UI to login). Builder-style;
    /// call before the first `start_account`.
    pub fn on_unauthorized(mut self, hook: UnauthorizedFn) -> Self {
        self.on_unauthorized = Some(hook);
        self
    }

    /// Wire the EXP-104 426 hook (the app shell passes a closure that gates
    /// the app into the blocking "Update required" state). Builder-style; call
    /// before the first `start_account`.
    pub fn on_upgrade_required(mut self, hook: UpgradeRequiredFn) -> Self {
        self.on_upgrade_required = Some(hook);
        self
    }

    /// Override the EXP-229 401 grace window (production default
    /// [`UNAUTHORIZED_GRACE`]). `Duration::ZERO` = first 401 is terminal —
    /// the teardown tests pin the legacy semantics through this.
    /// Builder-style; call before the first `start_account`.
    pub fn unauthorized_grace(mut self, grace: Duration) -> Self {
        self.unauthorized_grace = grace;
        self
    }

    /// The outward change-notification stream (§5.8). Drain from ONE
    /// foreground task — flume is MPMC: cloned receivers steal, they don't
    /// broadcast.
    pub fn deltas(&self) -> flume::Receiver<ShapeDelta> {
        self.deltas_rx.clone()
    }

    /// Start (or restart) one account's pipeline: open the per-account store
    /// and spawn the 16 shape threads (§5.3), each named after its shape.
    /// Returns `Ok(false)` when the account is already running (no-op); a
    /// dead entry (self-torn-down after a 401) is swept and restarted.
    pub fn start_account(&self, config: AccountSyncConfig) -> Result<bool, StoreError> {
        let mut pipelines = self.pipelines.lock().expect("pipelines poisoned");
        if let Some(existing) = pipelines.get(&config.account_id) {
            if existing.is_live() {
                return Ok(false);
            }
            // 401-dead pipeline: sweep it, then start fresh (re-login path).
            let dead = pipelines
                .remove(&config.account_id)
                .expect("checked present");
            stop_pipeline(dead);
        }

        let store = Arc::new(ShapeStore::open(&config.db_path)?);
        let stop = Arc::new(AtomicBool::new(false));
        // Shared by the 15 threads so the 401 signal fires exactly once per
        // account (§5.6b).
        let unauthorized_reported = Arc::new(AtomicBool::new(false));
        // Same one-shot dedupe for the EXP-104 426 gate.
        let upgrade_required_reported = Arc::new(AtomicBool::new(false));

        let mut threads = Vec::with_capacity(SHAPES.len());
        for spec in SHAPES
            .iter()
            .filter(|spec| config.shapes.is_none_or(|subset| subset.contains(&spec.name)))
        {
            let client = ShapeClient::new(ShapeClientConfig {
                account_id: config.account_id.clone(),
                base_url: config.base_url.clone(),
                spec,
                store: Arc::clone(&store),
                token: Arc::clone(&config.token),
                transport: Arc::clone(&self.transport),
                deltas: self.deltas_tx.clone(),
                unauthorized_reported: Arc::clone(&unauthorized_reported),
                on_unauthorized: self.on_unauthorized.clone(),
                upgrade_required_reported: Arc::clone(&upgrade_required_reported),
                on_upgrade_required: self.on_upgrade_required.clone(),
                unauthorized_grace: self.unauthorized_grace,
            });
            let thread_stop = Arc::clone(&stop);
            // Named per shape; truncated to 15 bytes so Linux's
            // pthread_setname_np limit doesn't silently drop the name.
            let mut name = format!("sync-{}", spec.name);
            name.truncate(15);
            let handle = std::thread::Builder::new()
                .name(name)
                .spawn(move || client.run(&thread_stop))
                .expect("spawn shape thread");
            threads.push(handle);
        }

        pipelines.insert(
            config.account_id.clone(),
            AccountPipeline {
                stop,
                threads,
                store,
                base_url: config.base_url,
                db_path: config.db_path,
                token: config.token,
                shapes: config.shapes,
            },
        );
        Ok(true)
    }

    /// Restart one LIVE account's pipeline in place (EXP-470). After a
    /// membership change (team create/join) every team-scoped shape's
    /// identity rotates server-side, but the threads parked in a blocking
    /// live long-poll only notice at their next poll boundary — up to the
    /// server's hold time (~60s). A restart makes every shape re-poll
    /// immediately: the fresh request 409s, refetches, and delivers the new
    /// team's data within a couple of seconds. Detached stragglers discard
    /// their in-flight results by design (`stop` is re-checked before
    /// apply). No-op (`false`) when no live pipeline exists.
    pub fn restart_account(&self, account_id: &str) -> bool {
        let config = {
            let pipelines = self.pipelines.lock().expect("pipelines poisoned");
            let Some(pipeline) = pipelines.get(account_id) else {
                return false;
            };
            if !pipeline.is_live() {
                return false;
            }
            AccountSyncConfig {
                account_id: account_id.to_string(),
                base_url: pipeline.base_url.clone(),
                db_path: pipeline.db_path.clone(),
                token: Arc::clone(&pipeline.token),
                shapes: pipeline.shapes,
            }
        };
        self.stop_account(account_id);
        match self.start_account(config) {
            Ok(_) => {
                // EXP-533: stamped for every restart path so a second kicker
                // (watchdog vs. window activation, both fired by one wake) can
                // see that the pipeline is already fresh and stand down.
                *self
                    .last_restart_at
                    .lock()
                    .expect("last_restart_at poisoned") = Some(Instant::now());
                // EXP-533: emitted HERE, not by a shape thread, so every
                // restart path (wake watchdog, offline-banner Retry, window
                // activation, team create/join) stamps the catch-up exactly
                // once. Best-effort like every other delta send.
                let _ = self.deltas_tx.send(ShapeDelta::PipelineRestarted {
                    account_id: account_id.to_string(),
                });
                true
            }
            Err(err) => {
                log::warn!("[sync {account_id}] restart failed: {err}");
                false
            }
        }
    }

    /// EXP-533: did a [`SyncManager::restart_account`] succeed within `window`?
    /// The debounce every other restart trigger consults, so the wake watchdog
    /// and the window-activation kick don't both rebuild the same pipeline.
    pub fn restarted_within(&self, window: Duration) -> bool {
        self.last_restart_at
            .lock()
            .expect("last_restart_at poisoned")
            .is_some_and(|at| at.elapsed() < window)
    }

    /// EXP-533: the machine woke from suspend — every live pipeline's threads
    /// are parked in reads on an h2 connection that died with the lid. Restart
    /// them all so the first fresh poll is a short catch-up instead of a 90s
    /// wait on a dead socket.
    pub fn on_wake_jump(&self) {
        let accounts = self.running_accounts();
        if accounts.is_empty() {
            return;
        }
        log::info!(
            "[sync] wake detected — restarting {} pipeline(s)",
            accounts.len()
        );
        for account_id in accounts {
            self.restart_account(&account_id);
        }
    }

    /// Stop one account's pipeline (§5.10 `sign_out`): flip the shared stop
    /// flag, join within [`STOP_GRACE`], detach stragglers blocked in a live
    /// read. The SQLite DB stays on disk for offline resume. Returns whether
    /// a pipeline existed.
    pub fn stop_account(&self, account_id: &str) -> bool {
        let pipeline = self
            .pipelines
            .lock()
            .expect("pipelines poisoned")
            .remove(account_id);
        match pipeline {
            Some(pipeline) => {
                stop_pipeline(pipeline);
                true
            }
            None => false,
        }
    }

    /// Quit path: stop every pipeline.
    pub fn stop_all(&self) {
        let pipelines: Vec<AccountPipeline> = {
            let mut map = self.pipelines.lock().expect("pipelines poisoned");
            map.drain().map(|(_, p)| p).collect()
        };
        for pipeline in pipelines {
            stop_pipeline(pipeline);
        }
    }

    /// Reconcile the running set against the signed-in set (§5.10): stop
    /// pipelines whose account is no longer signed in, start pipelines for
    /// newly signed-in accounts (and restart 401-dead ones — the caller only
    /// passes accounts that hold a resolved token, so `requireAuth` shapes
    /// are never polled anonymously, §5.9).
    pub fn reconcile(&self, configs: Vec<AccountSyncConfig>) {
        let wanted: Vec<&str> = configs.iter().map(|c| c.account_id.as_str()).collect();
        let to_stop: Vec<String> = {
            let pipelines = self.pipelines.lock().expect("pipelines poisoned");
            pipelines
                .keys()
                .filter(|id| !wanted.contains(&id.as_str()))
                .cloned()
                .collect()
        };
        for account_id in to_stop {
            self.stop_account(&account_id);
        }
        for config in configs {
            let account_id = config.account_id.clone();
            if let Err(err) = self.start_account(config) {
                log::warn!("[sync {account_id}] failed to start pipeline: {err}");
            }
        }
    }

    /// Accounts with a LIVE pipeline (told to run and not self-torn-down).
    pub fn running_accounts(&self) -> Vec<String> {
        self.pipelines
            .lock()
            .expect("pipelines poisoned")
            .iter()
            .filter(|(_, p)| p.is_live())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The per-account store, for the collections layer's hydration reads
    /// (§5.8 — the read-only WAL connection never blocks the writer).
    pub fn store(&self, account_id: &str) -> Option<Arc<ShapeStore>> {
        self.pipelines
            .lock()
            .expect("pipelines poisoned")
            .get(account_id)
            .map(|p| Arc::clone(&p.store))
    }

    /// Port of iOS "wait up to ~5s for the teams shape to land" (§5.10):
    /// block until the account's `teams` shape reaches head (its first
    /// `up-to-date`), so the app shell can show a spinner until the first
    /// board is renderable rather than an empty state. Returns `false` on
    /// timeout or when no pipeline/store exists.
    pub fn wait_for_first_sync(&self, account_id: &str, timeout: Duration) -> bool {
        let Some(store) = self.store(account_id) else {
            return false;
        };
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(state)) = store.shape_state("teams") {
                if state.is_live {
                    return true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SyncManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// EXP-533: start the detached suspend watchdog for a manager. One 1s-tick
/// thread per manager, holding only a [`Weak`] — quit never waits on it, and
/// it exits on its own once the manager is dropped. Call once, right after
/// the manager is wrapped in its `Arc` (the desktop's `Store::open`, the CLI
/// daemon's sync setup).
pub fn spawn_wake_watchdog(manager: &Arc<SyncManager>) {
    let weak = Arc::downgrade(manager);
    let spawned = std::thread::Builder::new()
        .name("sync-wake".to_string())
        .spawn(move || {
            let mut watchdog = WakeWatchdog::new(SystemTime::now(), Instant::now());
            loop {
                std::thread::sleep(WAKE_TICK);
                let Some(manager) = weak.upgrade() else {
                    return;
                };
                if watchdog.tick(SystemTime::now(), Instant::now()) {
                    manager.on_wake_jump();
                }
            }
        });
    if let Err(err) = spawned {
        log::warn!("[sync] wake watchdog failed to spawn: {err}");
    }
}

/// How often the watchdog samples the two clocks. Cheap enough to be
/// invisible and fine-grained enough that a wake is noticed within a second.
const WAKE_TICK: Duration = Duration::from_secs(1);

/// Flip the stop flag, then join each thread within the shared grace window;
/// stragglers (blocked in an in-flight live read, up to the 90s timeout) are
/// detached — they hold their own `Arc<ShapeStore>` clone, re-check the stop
/// flag before applying anything, and exit at their next loop boundary.
fn stop_pipeline(pipeline: AccountPipeline) {
    pipeline.stop.store(true, Ordering::Relaxed);
    let deadline = Instant::now() + STOP_GRACE;
    for handle in pipeline.threads {
        while !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            log::debug!(
                "[sync] detaching shape thread {:?} still blocked in a live read",
                handle.thread().name()
            );
            drop(handle);
        }
    }
}
