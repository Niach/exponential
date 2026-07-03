//! `SyncManager` — per-account pipeline reconcile (masterplan-v3 §5.10).
//! gpui-free; a direct port of the proven iOS `SyncManager.reconcile`.
//!
//! One manager owns every running sync pipeline. A pipeline is one account's
//! 14 shape threads (one dedicated `std::thread` per shape, §5.3) plus its
//! per-account rusqlite/WAL store (§5.4). Reconciling against the signed-in
//! account set:
//!
//! * **login / token refresh** → [`SyncManager::start_account`] spawns the 14
//!   threads against `{data_dir}/accounts/{id}/sync.sqlite`;
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
use std::time::{Duration, Instant};

use crate::client::{
    ShapeClient, ShapeClientConfig, ShapeDelta, ShapeTransport, TokenFn, UnauthorizedFn,
    UreqTransport,
};
use crate::shapes::SHAPES;
use crate::store::{ShapeStore, StoreError};

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
    /// `{data_dir}/accounts/{account_id}/sync.sqlite`.
    pub db_path: PathBuf,
    /// Call-time session-token access (§5.7) — never captured once.
    pub token: TokenFn,
}

struct AccountPipeline {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    store: Arc<ShapeStore>,
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
    deltas_tx: flume::Sender<ShapeDelta>,
    deltas_rx: flume::Receiver<ShapeDelta>,
    pipelines: Mutex<HashMap<String, AccountPipeline>>,
}

impl SyncManager {
    /// Production manager: blocking `ureq`/rustls transport (§5.3), one
    /// shared connection pool across all shape threads.
    pub fn new() -> Self {
        Self::with_transport(Arc::new(UreqTransport::new()))
    }

    /// Test seam (§5.3 testing guidance): inject any [`ShapeTransport`].
    pub fn with_transport(transport: Arc<dyn ShapeTransport>) -> Self {
        let (deltas_tx, deltas_rx) = flume::unbounded();
        Self {
            transport,
            on_unauthorized: None,
            deltas_tx,
            deltas_rx,
            pipelines: Mutex::new(HashMap::new()),
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

    /// The outward change-notification stream (§5.8). Drain from ONE
    /// foreground task — flume is MPMC: cloned receivers steal, they don't
    /// broadcast.
    pub fn deltas(&self) -> flume::Receiver<ShapeDelta> {
        self.deltas_rx.clone()
    }

    /// Start (or restart) one account's pipeline: open the per-account store
    /// and spawn the 14 shape threads (§5.3), each named after its shape.
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
        // Shared by the 14 threads so the 401 signal fires exactly once per
        // account (§5.6b).
        let unauthorized_reported = Arc::new(AtomicBool::new(false));

        let mut threads = Vec::with_capacity(SHAPES.len());
        for spec in &SHAPES {
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
            },
        );
        Ok(true)
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

    /// Port of iOS "wait up to ~5s for the workspaces shape to land" (§5.10):
    /// block until the account's `workspaces` shape reaches head (its first
    /// `up-to-date`), so the app shell can show a spinner until the first
    /// board is renderable rather than an empty state. Returns `false` on
    /// timeout or when no pipeline/store exists.
    pub fn wait_for_first_sync(&self, account_id: &str, timeout: Duration) -> bool {
        let Some(store) = self.store(account_id) else {
            return false;
        };
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(state)) = store.shape_state("workspaces") {
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
