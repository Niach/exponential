//! The host behind the hub: one desktop, many runs, ONE lock.
//!
//! The lock (the person's keyboard and pointer taken, see `backend`) engages
//! on a run's first call and stays while runs keep calling: a model thinks
//! for seconds between a screenshot and the click it decides on, and an
//! unlocked gap there would let the person's own keystrokes land in the
//! window the agent is about to drive. It releases when every run that used
//! it has ended, after [`IDLE`] without a call, or on Esc.
//!
//! Esc = abort: the lock goes, every run that used it is marked aborted
//! (its calls answer [`tools::ABORTED`] until its next turn,
//! [`Host::turn_started`]), and the host's hook for each (the engine's
//! Cancel) interrupts the turn. The presence observer (the IDE's banner)
//! hears every engage and release.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::backend::{self, LockGuard};
use crate::server::ToolHost;
use crate::tools;

/// A lock nobody has used for this long releases itself.
pub(crate) const IDLE: Duration = Duration::from_secs(30);
const JANITOR_TICK: Duration = Duration::from_millis(500);

pub(crate) type Presence = Arc<dyn Fn(Option<String>) + Send + Sync>;
pub(crate) type AbortHook = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct LockState {
    held: Option<Box<dyn LockGuard>>,
    /// What the banner says: the label of the run that engaged it.
    label: String,
    last_used: Option<Instant>,
    /// Runs that called while this lock was held (all aborted by one Esc).
    users: HashSet<String>,
}

#[derive(Default)]
struct Run {
    aborted: bool,
    hook: Option<AbortHook>,
    /// The display the run works on (`tools::CallContext`).
    display: Option<usize>,
}

#[derive(Default)]
pub(crate) struct Host {
    lock: Mutex<LockState>,
    runs: Mutex<HashMap<String, Run>>,
    presence: Mutex<Option<Presence>>,
    janitor: Mutex<bool>,
    /// For the lock's Esc callback.
    weak: Mutex<Weak<Host>>,
}

impl Host {
    pub(crate) fn new() -> Arc<Host> {
        let host = Arc::new(Host::default());
        *host.weak.lock().unwrap() = Arc::downgrade(&host);
        host
    }

    pub(crate) fn set_presence(&self, presence: Presence) {
        let engaged = {
            let lock = self.lock.lock().unwrap();
            lock.held.as_ref().map(|_| lock.label.clone())
        };
        *self.presence.lock().unwrap() = Some(presence.clone());
        presence(engaged);
    }

    fn announce(&self, label: Option<String>) {
        let presence = self.presence.lock().unwrap().clone();
        if let Some(presence) = presence {
            presence(label);
        }
    }

    /// Whether the person's input is taken right now.
    pub(crate) fn engaged(&self) -> bool {
        self.lock.lock().unwrap().held.is_some()
    }

    pub(crate) fn set_hook(&self, session_id: &str, hook: AbortHook) {
        self.runs.lock().unwrap().entry(session_id.to_string()).or_default().hook = Some(hook);
    }

    /// A new turn of the run: the person spoke again, an earlier Esc is
    /// answered.
    pub(crate) fn turn_started(&self, session_id: &str) {
        if let Some(run) = self.runs.lock().unwrap().get_mut(session_id) {
            run.aborted = false;
        }
    }

    fn aborted(&self, session_id: &str) -> bool {
        self.runs.lock().unwrap().get(session_id).is_some_and(|run| run.aborted)
    }

    /// Hold the lock for `session_id`'s call: engage it if nobody does.
    fn lease(&self, session_id: &str, label: &str, backend: &dyn backend::Backend) -> Result<(), String> {
        let mut lock = self.lock.lock().unwrap();
        let engaged = if lock.held.is_none() {
            let weak = self.weak.lock().unwrap().clone();
            let guard = backend.lock(Box::new(move || {
                if let Some(host) = weak.upgrade() {
                    host.abort();
                }
            }))?;
            lock.held = Some(guard);
            lock.label = label.to_string();
            lock.users.clear();
            true
        } else {
            false
        };
        lock.last_used = Some(Instant::now());
        lock.users.insert(session_id.to_string());
        let label = lock.label.clone();
        drop(lock);
        if engaged {
            log::info!("[computer] input locked for {label}");
            self.announce(Some(label));
            self.start_janitor();
        }
        Ok(())
    }

    /// Let the person's input back; `None` when nothing was held.
    fn release(&self) -> Option<(Box<dyn LockGuard>, HashSet<String>)> {
        let mut lock = self.lock.lock().unwrap();
        let guard = lock.held.take()?;
        let users = std::mem::take(&mut lock.users);
        lock.last_used = None;
        drop(lock);
        log::info!("[computer] input unlocked");
        crate::work_screen::set_banner(None);
        self.announce(None);
        Some((guard, users))
    }

    /// The person pressed Esc (on the lock's thread): unlock, abort every
    /// run that used it, interrupt their turns.
    pub(crate) fn abort(&self) {
        let Some((guard, users)) = self.release() else { return };
        // Releasing on the lock's own thread: the guard must not join it.
        drop(guard);
        let hooks: Vec<AbortHook> = {
            let mut runs = self.runs.lock().unwrap();
            users
                .iter()
                .filter_map(|session_id| {
                    let run = runs.entry(session_id.clone()).or_default();
                    run.aborted = true;
                    run.hook.clone()
                })
                .collect()
        };
        log::warn!("[computer] Esc: computer use aborted for {} run(s)", users.len());
        for hook in hooks {
            hook();
        }
    }

    /// The run ended: its abort state goes, and the lock with it when it
    /// was the last user.
    pub(crate) fn end(&self, session_id: &str) {
        self.runs.lock().unwrap().remove(session_id);
        let last = {
            let mut lock = self.lock.lock().unwrap();
            lock.users.remove(session_id);
            lock.held.is_some() && lock.users.is_empty()
        };
        if last {
            self.release();
        }
    }

    /// The switch turned off: everything goes.
    pub(crate) fn shutdown(&self) {
        self.runs.lock().unwrap().clear();
        self.release();
    }

    /// One thread for the host's lifetime: releases a lock idle past [`IDLE`].
    fn start_janitor(&self) {
        let mut started = self.janitor.lock().unwrap();
        if *started {
            return;
        }
        *started = true;
        let weak = self.weak.lock().unwrap().clone();
        let _ = std::thread::Builder::new().name("computer-use-idle".into()).spawn(move || loop {
            std::thread::sleep(JANITOR_TICK);
            let Some(host) = weak.upgrade() else { return };
            let idle = {
                let lock = host.lock.lock().unwrap();
                lock.held.is_some() && lock.last_used.is_some_and(|at| at.elapsed() > IDLE)
            };
            if idle {
                host.release();
            }
        });
    }
}

impl ToolHost for Host {
    fn tools(&self) -> Result<Value, String> {
        Ok(tools::list())
    }

    fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
        if self.aborted(session_id) {
            return tools::error(tools::ABORTED);
        }
        let backend = match backend::open() {
            Ok(backend) => backend,
            Err(reason) => return tools::error(reason),
        };
        // NEW-59: the lock engages only once the work-screen gate let the
        // call through: a refusal leaves the person's input alone.
        let engage = || {
            self.lease(session_id, label, &*backend).map_err(|reason| {
                format!("Could not lock the person's input ({reason}); nothing was done. Tell them and try again.")
            })
        };
        let interrupted = || self.aborted(session_id);
        let mut display = self.runs.lock().unwrap().get(session_id).and_then(|run| run.display);
        let mut ctx = tools::CallContext {
            interrupted: &interrupted,
            display: &mut display,
            picked: crate::work_screen::picked(),
            engage: &engage,
        };
        let result = tools::call(&*backend, name, &arguments, &mut ctx);
        self.runs.lock().unwrap().entry(session_id.to_string()).or_default().display = display;
        // Esc during the action: what it did is moot, the answer is the abort.
        if interrupted() {
            return tools::error(tools::ABORTED);
        }
        self.lock.lock().unwrap().last_used = Some(Instant::now());
        result
    }

    fn end(&self, session_id: &str) {
        Host::end(self, session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn esc_aborts_every_user_of_the_lock_until_their_next_turn() {
        let host = Host::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        host.set_presence({
            let seen = seen.clone();
            Arc::new(move |label| seen.lock().unwrap().push(label))
        });
        let cancelled = Arc::new(Mutex::new(0));
        host.set_hook("s1", {
            let cancelled = cancelled.clone();
            Arc::new(move || *cancelled.lock().unwrap() += 1)
        });
        let backend = tools::tests::FakeBackend::default();
        assert!(host.lease("s1", "EXP-1", &backend).is_ok());
        assert!(host.lease("s2", "Chat", &backend).is_ok());
        assert!(host.engaged());
        host.abort();
        assert!(!host.engaged());
        assert_eq!(*cancelled.lock().unwrap(), 1);
        assert!(host.aborted("s1") && host.aborted("s2"));
        assert_eq!(host.call("s1", "EXP-1", "cursor_position", json!({}))["isError"], true);
        host.turn_started("s1");
        assert!(!host.aborted("s1"));
        assert_eq!(seen.lock().unwrap().as_slice(), [None, Some("EXP-1".to_string()), None]);
    }

    #[test]
    fn the_last_users_end_releases_and_a_refused_lock_does_nothing() {
        let host = Host::new();
        let backend = tools::tests::FakeBackend::default();
        host.lease("s1", "EXP-1", &backend).unwrap();
        host.lease("s2", "EXP-2", &backend).unwrap();
        host.end("s1");
        assert!(host.engaged());
        host.end("s2");
        assert!(!host.engaged());
        let refusing = tools::tests::FakeBackend { fail_lock: true, ..Default::default() };
        let err = host.lease("s3", "EXP-3", &refusing).unwrap_err();
        assert!(err.contains("keyboard"));
        assert!(!host.engaged());
        host.lease("s3", "EXP-3", &backend).unwrap();
        host.shutdown();
        assert!(!host.engaged());
    }
}
