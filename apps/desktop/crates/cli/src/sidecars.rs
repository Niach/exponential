//! Process-wide agent sidecars — the CLI's mirror of the desktop's
//! `ui/src/steer_wiring.rs` `PtySidecars` host: ONE claude
//! hooks loopback server whose events fan out per worktree (bound by
//! claude's own session id once seen), and ONE pi observer server that
//! routes internally by canonicalized worktree.
//!
//! EXP-758: both are bound LAZILY, on first use. Both are PTY-era wiring the
//! ACP engine never reads (`coding::launcher`'s `write_hook_settings` /
//! `apply_observer_env` are Terminal-transport only), and each is
//! agent-specific on top: hooks are claude's, the observer is pi's. A daemon
//! that starts no session, or only sessions of the other agent, used to bind
//! two loopback ports and a router thread for their whole life anyway.
//! EXP-761: WHEN to bind is the launcher's call, not this host's — it is a
//! [`coding::SidecarSource`], asked from `prepare_with_hooks`'s Terminal arm
//! only, and only for the launched CLI's own sidecar. (EXP-758 bound per
//! agent ahead of `prepare`, which could not know the transport yet, so a
//! claude launch that resolved to ACP still bound the hooks server.) A
//! terminal-default device warms both at boot ([`Sidecars::ensure_started_if`])
//! so the first launch does not pay for it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

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

impl Sidecars {
    /// A sidecar host that has bound NOTHING yet.
    pub fn new() -> Self {
        Self::default()
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

    /// EXP-758: BINDS the hooks server on the first call.
    pub fn hook_setup(&self) -> Option<coding::HookSetup> {
        self.hooks.get_or_init(Hooks::start).setup.clone()
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
    /// delivery — no cwd guess, no insertion-order race. EXP-761: never
    /// binds — a PTY claude launch already asked for the server; `None`
    /// when it never came up (or the launch was not a PTY one).
    pub fn subscribe_hooks(
        &self,
        worktree: &Path,
        session_id: Option<&str>,
    ) -> Option<flume::Receiver<HookEvent>> {
        let hooks = self.hooks.get()?;
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
        // EXP-761: read-only, like `subscribe_hooks` — a pi PTY launch bound it.
        self.observer
            .get()
            .and_then(Option::as_ref)
            .map(|server| server.subscribe(worktree))
    }
}

/// EXP-761: the launcher asks here, on its Terminal arm only.
impl coding::SidecarSource for Sidecars {
    fn hooks(&self) -> Option<coding::HookSetup> {
        self.hook_setup()
    }
    fn observer(&self) -> Option<coding::ObserverSetup> {
        self.observer_setup()
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
    use coding::CodingAgent;

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

    /// EXP-758/EXP-761: a host nobody asked anything of binds nothing: no
    /// loopback port, no router thread. The daemon builds one at boot and,
    /// on an all-ACP device, is never asked — the launcher's gate
    /// (`coding::resolve_pty_sidecars`, run here against the real host)
    /// asks on the Terminal arm alone, so an ACP launch of ANY agent leaves
    /// both handles unset, and a subscription never binds either.
    #[test]
    fn an_acp_launch_leaves_the_sidecar_handles_unset() {
        let sidecars = Sidecars::new();
        assert!(sidecars.hooks.get().is_none());
        assert!(sidecars.observer.get().is_none());
        let every_agent =
            [Some(CodingAgent::Claude), Some(CodingAgent::Pi), Some(CodingAgent::Codex), None];
        for agent in every_agent {
            let wired =
                coding::resolve_pty_sidecars(&sidecars, coding::LaunchTransport::Acp, agent);
            assert!(wired.0.is_none() && wired.1.is_none(), "{agent:?}");
        }
        assert!(sidecars.hooks.get().is_none(), "an ACP launch must not bind hooks");
        assert!(sidecars.observer.get().is_none(), "…nor the observer");
        // Subscribing (the emitter attach) reads, never binds.
        assert!(sidecars.subscribe_hooks(&std::env::temp_dir(), Some("sess-a")).is_none());
        assert!(sidecars.subscribe_pi(&std::env::temp_dir()).is_none());
        assert!(sidecars.hooks.get().is_none() && sidecars.observer.get().is_none());
        // A codex PTY launch asks for neither, so it binds neither.
        let wired = coding::resolve_pty_sidecars(
            &sidecars,
            coding::LaunchTransport::Terminal,
            Some(CodingAgent::Codex),
        );
        assert!(wired.0.is_none() && wired.1.is_none());
        assert!(sidecars.hooks.get().is_none(), "codex must not bind hooks");
        assert!(sidecars.observer.get().is_none());
    }

    /// EXP-758: the first ask binds, later ones reuse: a second claude PTY
    /// launch must not open a second loopback port, and a claude launch
    /// never binds pi's observer. (A failed bind is the documented degrade:
    /// the cell is still initialised, to `None`.)
    #[test]
    fn a_pty_launch_binds_its_agents_sidecar_once() {
        let sidecars = Sidecars::new();
        let claude = |sidecars: &Sidecars| {
            coding::resolve_pty_sidecars(
                sidecars,
                coding::LaunchTransport::Terminal,
                Some(CodingAgent::Claude),
            )
        };
        let first = claude(&sidecars);
        assert!(sidecars.hooks.get().is_some(), "claude binds the hooks server");
        assert!(sidecars.observer.get().is_none(), "and only that one");
        let second = claude(&sidecars);
        assert_eq!(
            first.0.map(|setup| setup.port),
            second.0.map(|setup| setup.port),
            "the same server serves every claude launch"
        );
        // Now the subscription finds it.
        assert!(sidecars.subscribe_hooks(&std::env::temp_dir(), Some("sess-a")).is_some());
        let pi = coding::resolve_pty_sidecars(
            &sidecars,
            coding::LaunchTransport::Terminal,
            Some(CodingAgent::Pi),
        );
        assert!(pi.0.is_none() && pi.1.is_some());
        assert!(sidecars.observer.get().is_some());
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
