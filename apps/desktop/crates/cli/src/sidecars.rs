//! Process-wide agent sidecars — the CLI's mirror of the desktop's
//! `ui/src/steer_wiring.rs` HookSidecar + PiObserverGlobal: ONE claude
//! hooks loopback server whose events fan out per worktree (bound by
//! claude's own session id once seen), and ONE pi observer server that
//! routes internally by canonicalized worktree.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use steer::{HookEvent, HookServer};

struct HookSubscriber {
    worktree: PathBuf,
    tx: flume::Sender<HookEvent>,
    /// claude session ids already routed here — the tie-breaker when two
    /// runs share a cwd (two action runs on one trunk clone).
    bound: HashSet<String>,
}

pub struct Sidecars {
    hook_setup: Option<coding::HookSetup>,
    hook_subscribers: Arc<Mutex<Vec<HookSubscriber>>>,
    _hook_server: Option<HookServer>,
    observer: Option<Arc<steer::pi_observer::ObserverServer>>,
}

impl Sidecars {
    /// Start both sidecars; a failed bind degrades that sidecar to `None`
    /// (grid-only claude detection / diffs-only pi feed), never an error.
    pub fn start() -> Self {
        let (hook_setup, hook_subscribers, hook_server) = match HookServer::start() {
            Ok(server) => {
                let setup = coding::HookSetup {
                    port: server.port(),
                    token: server.token().to_string(),
                    settings_json: server.settings_json(),
                };
                let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::new(Mutex::new(Vec::new()));
                let events = server.events().clone();
                let routed = Arc::clone(&subscribers);
                let spawned = std::thread::Builder::new()
                    .name("exp-hook-router".to_string())
                    .spawn(move || {
                        while let Ok(event) = events.recv() {
                            route_hook_event(&routed, event);
                        }
                    })
                    .is_ok();
                if spawned {
                    (Some(setup), subscribers, Some(server))
                } else {
                    (None, Arc::new(Mutex::new(Vec::new())), None)
                }
            }
            Err(err) => {
                log::warn!("hooks sidecar failed to bind: {err}");
                (None, Arc::new(Mutex::new(Vec::new())), None)
            }
        };
        let observer = match steer::pi_observer::ObserverServer::start() {
            Ok(server) => Some(Arc::new(server)),
            Err(err) => {
                log::warn!("pi observer sidecar failed to bind: {err}");
                None
            }
        };
        Self {
            hook_setup,
            hook_subscribers,
            _hook_server: hook_server,
            observer,
        }
    }

    pub fn hook_setup(&self) -> Option<coding::HookSetup> {
        self.hook_setup.clone()
    }

    pub fn observer_setup(&self) -> Option<coding::ObserverSetup> {
        self.observer.as_ref().map(|server| coding::ObserverSetup {
            port: server.port(),
            token: server.token().to_string(),
        })
    }

    /// Subscribe a session's worktree to the claude hook stream; dropping
    /// the receiver unsubscribes on the next delivery.
    pub fn subscribe_hooks(&self, worktree: &Path) -> Option<flume::Receiver<HookEvent>> {
        self.hook_setup.as_ref()?;
        let (tx, rx) = flume::unbounded();
        let mut subscribers = lock(&self.hook_subscribers);
        subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
        subscribers.push(HookSubscriber {
            worktree: canonical(worktree),
            tx,
            bound: HashSet::new(),
        });
        Some(rx)
    }

    /// A pi session's slice of the observer sidecar.
    pub fn subscribe_pi(
        &self,
        worktree: &Path,
    ) -> Option<(
        flume::Receiver<steer::pi_observer::PiEvent>,
        steer::pi_observer::PiSteerHandle,
    )> {
        self.observer
            .as_ref()
            .map(|server| server.subscribe(worktree))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The hook payload's `cwd` and our worktree must compare equal through
/// symlinked temp/home dirs.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Deliver one hook event to at most ONE session: a bound claude session id
/// wins outright, else the cwd picks the session (preferring one with no
/// bound id yet so two runs sharing a trunk clone split).
fn route_hook_event(subscribers: &Arc<Mutex<Vec<HookSubscriber>>>, event: HookEvent) {
    let mut subscribers = lock(subscribers);
    subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
    let session_id = event.context.session_id.clone();
    let target = session_id
        .as_ref()
        .and_then(|id| {
            subscribers
                .iter()
                .position(|subscriber| subscriber.bound.contains(id))
        })
        .or_else(|| {
            let cwd = canonical(Path::new(event.context.cwd.as_deref()?));
            subscribers
                .iter()
                .position(|subscriber| subscriber.worktree == cwd && subscriber.bound.is_empty())
                .or_else(|| {
                    subscribers
                        .iter()
                        .position(|subscriber| subscriber.worktree == cwd)
                })
        });
    let Some(target) = target else {
        log::debug!("hook event with no matching session, dropped");
        return;
    };
    if let Some(id) = session_id {
        subscribers[target].bound.insert(id);
    }
    let _ = subscribers[target].tx.send(event);
}
