//! Process-wide agent sidecars — the CLI's mirror of the desktop's
//! `ui/src/steer_wiring.rs` HookSidecar + PiObserverGlobal: ONE claude
//! hooks loopback server whose events fan out per worktree (bound by
//! claude's own session id once seen), and ONE pi observer server that
//! routes internally by canonicalized worktree.
//!
//! EXP-758: both are bound LAZILY, on first use. Both are PTY-era wiring the
//! ACP engine never reads (`coding::launcher`'s `write_hook_settings` /
//! `apply_observer_env` are Terminal-transport only), and each is
//! agent-specific on top: hooks are claude's, the observer is pi's. A daemon
//! that starts no session, or only sessions of the other agent, used to bind
//! two loopback ports and a router thread for their whole life anyway. The
//! launch path asks through [`Sidecars::for_launch`], which binds only what
//! the launch's agent could actually use; a terminal-default device warms
//! both at boot ([`Sidecars::ensure_started_if`]) so the first launch does
//! not pay for it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use coding::CodingAgent;
use steer::{HookEvent, HookServer};

struct HookSubscriber {
    worktree: PathBuf,
    tx: flume::Sender<HookEvent>,
    /// claude session ids already routed here — the tie-breaker when two
    /// runs share a cwd (two action runs on one trunk clone).
    bound: HashSet<String>,
}

/// The claude hooks sidecar, once bound. `setup` is `None` when the bind
/// failed, the documented degrade (grid-only detection), never an error.
struct Hooks {
    setup: Option<coding::HookSetup>,
    subscribers: Arc<Mutex<Vec<HookSubscriber>>>,
    _server: Option<HookServer>,
}

#[derive(Default)]
pub struct Sidecars {
    /// EXP-758: bound on first use. `OnceLock` and not `Mutex<Option<..>>`
    /// because a bound sidecar is never rebound and every reader wants a
    /// shared reference to it.
    hooks: OnceLock<Hooks>,
    observer: OnceLock<Option<Arc<steer::pi_observer::ObserverServer>>>,
}

/// EXP-758: what one launch may read out of the sidecars, resolved (and
/// bound) once, per launch, by [`Sidecars::for_launch`].
pub struct LaunchSidecars {
    pub hooks: Option<coding::HookSetup>,
    pub observer: Option<coding::ObserverSetup>,
}

impl Sidecars {
    /// A sidecar host that has bound NOTHING yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// EXP-758: the setups `prepare` needs for a launch of `agent`, binding
    /// only the sidecar that agent can use: hooks are claude-only wiring and
    /// the observer is pi-only (`coding::launcher` filters on exactly that),
    /// so a codex device binds neither, ever. `None` (an external ACP agent,
    /// `launch::request_agent`) binds nothing at all.
    ///
    /// The transport is NOT known here: `prepare` decides it (it needs the
    /// doctor report) and it needs these values in the same call. So a claude
    /// launch that ends up on the ACP arm still binds the hooks server once.
    /// Erring that way is deliberate: the other way round would lose hook
    /// detection on exactly the launches that FELL BACK to the terminal.
    pub fn for_launch(&self, agent: Option<CodingAgent>) -> LaunchSidecars {
        let (wants_hooks, wants_observer) = wanted_sidecars(agent);
        LaunchSidecars {
            hooks: wants_hooks.then(|| self.hook_setup()).flatten(),
            observer: wants_observer.then(|| self.observer_setup()).flatten(),
        }
    }

    /// EXP-758: bind both up front when this device starts every run in a
    /// terminal (`Settings::start_in_terminal`), because that device's launches all
    /// want them, so the first one should not pay the bind.
    pub fn ensure_started_if(&self, start_in_terminal: bool) {
        if start_in_terminal {
            let _ = self.hook_setup();
            let _ = self.observer_setup();
        }
    }

    fn hooks(&self) -> &Hooks {
        self.hooks.get_or_init(Hooks::start)
    }

    /// EXP-758: BINDS the hooks server on the first call.
    pub fn hook_setup(&self) -> Option<coding::HookSetup> {
        self.hooks().setup.clone()
    }

    /// EXP-758: BINDS the pi observer server on the first call. A failed bind
    /// degrades to `None` (diffs-only pi feed), never an error.
    pub fn observer_setup(&self) -> Option<coding::ObserverSetup> {
        self.observer_server()
            .as_ref()
            .map(|server| coding::ObserverSetup {
                port: server.port(),
                token: server.token().to_string(),
            })
    }

    fn observer_server(&self) -> &Option<Arc<steer::pi_observer::ObserverServer>> {
        self.observer
            .get_or_init(|| match steer::pi_observer::ObserverServer::start() {
                Ok(server) => Some(Arc::new(server)),
                Err(err) => {
                    log::warn!("pi observer sidecar failed to bind: {err}");
                    None
                }
            })
    }

    /// Subscribe a session's worktree to the claude hook stream; dropping
    /// the receiver unsubscribes on the next delivery. `session_id`
    /// (EXP-443) is the launcher-minted `--session-id`: pre-seeding `bound`
    /// makes [`route_hook_event`]'s rule 1 authoritative from the first
    /// delivery — no cwd guess, no insertion-order race.
    pub fn subscribe_hooks(
        &self,
        worktree: &Path,
        session_id: Option<&str>,
    ) -> Option<flume::Receiver<HookEvent>> {
        let hooks = self.hooks();
        hooks.setup.as_ref()?;
        let (tx, rx) = flume::unbounded();
        let mut subscribers = lock(&hooks.subscribers);
        subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
        subscribers.push(HookSubscriber {
            worktree: canonical(worktree),
            tx,
            bound: session_id
                .map(|id| HashSet::from([id.to_string()]))
                .unwrap_or_default(),
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
        self.observer_server()
            .as_ref()
            .map(|server| server.subscribe(worktree))
    }
}

impl Hooks {
    /// Bind the hooks loopback server; a failed bind degrades to `None`
    /// (grid-only claude detection), never an error.
    fn start() -> Self {
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
                log::warn!(
                    "hooks sidecar failed to bind: {err} — every session degrades to grid-only \
                     detection (no question identity, no transcript pin)"
                );
                (None, Arc::new(Mutex::new(Vec::new())), None)
            }
        };
        Self {
            setup: hook_setup,
            subscribers: hook_subscribers,
            _server: hook_server,
        }
    }
}

/// EXP-758: `(hooks, observer)`, which sidecars a launch of `agent` can use,
/// decided WITHOUT touching either server. The rule is `coding::launcher`'s:
/// `write_hook_settings` wires hooks for claude alone and `apply_observer_env`
/// the observer for pi alone, so every other launch (codex, and an external
/// ACP agent, `None`) needs neither.
fn wanted_sidecars(agent: Option<CodingAgent>) -> (bool, bool) {
    (
        agent == Some(CodingAgent::Claude),
        agent == Some(CodingAgent::Pi),
    )
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
///
/// EXP-443: sessions launch with `bound` pre-seeded by the minted
/// `--session-id`, so rule 1 is authoritative from the first delivery. The
/// cwd fallback stays for the ids a pre-seed cannot know: a `/clear`-minted
/// rotation and pre-EXP-443 resumes.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn subscriber(
        subscribers: &Arc<Mutex<Vec<HookSubscriber>>>,
        worktree: &Path,
        session_id: Option<&str>,
    ) -> flume::Receiver<HookEvent> {
        let (tx, rx) = flume::unbounded();
        lock(subscribers).push(HookSubscriber {
            // Like the real `subscribe`: on macOS `temp_dir()` sits behind
            // the `/var` → `/private/var` symlink, and the router compares
            // CANONICAL paths — a raw path here fails every cwd match.
            worktree: canonical(worktree),
            tx,
            bound: session_id
                .map(|id| HashSet::from([id.to_string()]))
                .unwrap_or_default(),
        });
        rx
    }

    fn event(session_id: Option<&str>, cwd: &Path) -> HookEvent {
        steer::hooks::parse_hook_event(
            serde_json::json!({
                "hook_event_name": "Stop",
                "session_id": session_id,
                "cwd": cwd.to_string_lossy(),
            })
            .to_string()
            .as_bytes(),
        )
        .expect("fixture parses")
    }

    /// EXP-758: a host nobody asked anything of binds nothing: no loopback
    /// port, no router thread. The daemon builds one at boot and (on an
    /// all-ACP device) never queries it.
    #[test]
    fn a_fresh_host_binds_nothing() {
        let sidecars = Sidecars::new();
        assert!(sidecars.hooks.get().is_none());
        assert!(sidecars.observer.get().is_none());
        // A codex launch asks for neither, so it binds neither.
        let wired = sidecars.for_launch(Some(CodingAgent::Codex));
        assert!(wired.hooks.is_none() && wired.observer.is_none());
        assert!(sidecars.hooks.get().is_none(), "codex must not bind hooks");
        assert!(sidecars.observer.get().is_none());
    }

    /// EXP-758: hooks are claude's wiring and the observer is pi's
    /// (`coding::launcher` filters on exactly that); an external ACP agent
    /// (`None`) has neither.
    #[test]
    fn only_the_agents_own_sidecar_is_wanted() {
        assert_eq!(wanted_sidecars(Some(CodingAgent::Claude)), (true, false));
        assert_eq!(wanted_sidecars(Some(CodingAgent::Pi)), (false, true));
        assert_eq!(wanted_sidecars(Some(CodingAgent::Codex)), (false, false));
        assert_eq!(wanted_sidecars(None), (false, false));
    }

    /// EXP-758: the first query binds, later ones reuse: a second claude
    /// launch must not open a second loopback port. (A failed bind is the
    /// documented degrade: the cell is still initialised, to `None`.)
    #[test]
    fn querying_binds_once() {
        let sidecars = Sidecars::new();
        let first = sidecars.for_launch(Some(CodingAgent::Claude));
        assert!(sidecars.hooks.get().is_some(), "claude binds the hooks server");
        assert!(sidecars.observer.get().is_none(), "and only that one");
        let second = sidecars.for_launch(Some(CodingAgent::Claude));
        assert_eq!(
            first.hooks.map(|setup| setup.port),
            second.hooks.map(|setup| setup.port),
            "the same server serves every claude launch"
        );
    }

    /// EXP-443: a pre-seeded bound id wins over an earlier same-cwd
    /// subscriber — the insertion-order race the seed removes.
    #[test]
    fn router_prefers_the_bound_session_id() {
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::default();
        let cwd = std::env::temp_dir();
        let first = subscriber(&subscribers, &cwd, Some("sess-a"));
        let second = subscriber(&subscribers, &cwd, Some("sess-b"));
        route_hook_event(&subscribers, event(Some("sess-b"), &cwd));
        assert!(second.try_recv().is_ok(), "bound id must win");
        assert!(first.try_recv().is_err());
    }

    /// An unknown id lands on the same-cwd subscriber with an EMPTY bound
    /// set first — a seeded subscriber never absorbs a stranger's id while
    /// an unseeded one exists.
    #[test]
    fn router_gives_an_unknown_id_to_the_unbound_subscriber() {
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::default();
        let cwd = std::env::temp_dir();
        let seeded = subscriber(&subscribers, &cwd, Some("sess-a"));
        let unseeded = subscriber(&subscribers, &cwd, None);
        route_hook_event(&subscribers, event(Some("sess-c"), &cwd));
        assert!(unseeded.try_recv().is_ok());
        assert!(seeded.try_recv().is_err());
    }

    /// The cwd fallback stays: with only a seeded subscriber present, a
    /// fresh id (a /clear rotation) still binds by cwd and sticks.
    #[test]
    fn router_binds_a_rotated_id_by_cwd_as_last_resort() {
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::default();
        let cwd = std::env::temp_dir();
        let seeded = subscriber(&subscribers, &cwd, Some("sess-a"));
        route_hook_event(&subscribers, event(Some("sess-rotated"), &cwd));
        assert!(seeded.try_recv().is_ok(), "cwd fallback must deliver");
        // The rotation is now bound: it wins rule 1 on the next delivery
        // even after an unseeded subscriber appears.
        let unseeded = subscriber(&subscribers, &cwd, None);
        route_hook_event(&subscribers, event(Some("sess-rotated"), &cwd));
        assert!(seeded.try_recv().is_ok());
        assert!(unseeded.try_recv().is_err());
    }
}
