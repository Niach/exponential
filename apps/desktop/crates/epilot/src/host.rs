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
    /// A call that leased it is still running: the janitor leaves it alone
    /// however long the call takes (#EXP-1893).
    in_call: bool,
    /// Runs that called while this lock was held (all aborted by one Esc).
    users: HashSet<String>,
}

#[derive(Default)]
struct Run {
    aborted: bool,
    hook: Option<AbortHook>,
    /// The display the run works on (`tools::CallContext`).
    display: Option<usize>,
    /// `tools::CallContext::copy_mark`.
    copy_mark: Option<u64>,
}

/// Why a call did not get the lock.
#[derive(Debug)]
enum Refused {
    /// The person's Esc aborted the run before the lease.
    Aborted,
    /// The OS refused the lock.
    Os(String),
}

#[derive(Default)]
pub(crate) struct Host {
    /// Held for a whole call: calls queue, so two runs (or one run's
    /// parallel calls) never drive the one pointer, keyboard and clipboard
    /// at the same time (#EXP-1893).
    calls: Mutex<()>,
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

    /// Hold the lock for `session_id`'s call, until the call's end
    /// ([`Host::call_on`]): engage it if nobody does.
    fn lease(&self, session_id: &str, label: &str, backend: &dyn backend::Backend) -> Result<(), Refused> {
        let mut lock = self.lock.lock().unwrap();
        // Read under the lock's mutex, where `abort` writes it: an Esc that
        // landed since the call's first check never gets the lock re-taken.
        if self.aborted(session_id) {
            return Err(Refused::Aborted);
        }
        let engaged = if lock.held.is_none() {
            let weak = self.weak.lock().unwrap().clone();
            let guard = backend.lock(Box::new(move || {
                if let Some(host) = weak.upgrade() {
                    host.abort();
                }
            }));
            lock.held = Some(guard.map_err(Refused::Os)?);
            lock.label = label.to_string();
            lock.users.clear();
            true
        } else {
            false
        };
        lock.last_used = Some(Instant::now());
        lock.in_call = true;
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

    /// Take the lock out of `lock`, under its mutex (whoever decides the
    /// release does it there: a run leasing meanwhile engages anew);
    /// `None` when nothing was held. [`Host::released`] follows.
    fn take(lock: &mut LockState) -> Option<(Box<dyn LockGuard>, HashSet<String>)> {
        let guard = lock.held.take()?;
        let users = std::mem::take(&mut lock.users);
        lock.last_used = None;
        lock.in_call = false;
        Some((guard, users))
    }

    /// The lock went ([`Host::take`]): say so.
    fn released(&self) {
        log::info!("[computer] input unlocked");
        crate::work_screen::set_banner(None);
        self.announce(None);
    }

    /// Let the person's input back; `None` when nothing was held.
    fn release(&self) -> Option<(Box<dyn LockGuard>, HashSet<String>)> {
        let taken = Self::take(&mut self.lock.lock().unwrap())?;
        self.released();
        Some(taken)
    }

    /// The person pressed Esc (on the lock's thread; a hold that died
    /// reads the same): unlock, abort every run that used it, interrupt
    /// their turns.
    pub(crate) fn abort(&self) {
        let (guard, users, hooks) = {
            let mut lock = self.lock.lock().unwrap();
            let Some((guard, users)) = Self::take(&mut lock) else { return };
            // Marked before the lock's mutex goes: `lease` reads it there.
            let mut runs = self.runs.lock().unwrap();
            let hooks: Vec<AbortHook> = users
                .iter()
                .filter_map(|session_id| {
                    let run = runs.entry(session_id.clone()).or_default();
                    run.aborted = true;
                    run.hook.clone()
                })
                .collect();
            (guard, users, hooks)
        };
        self.released();
        // Releasing on the lock's own thread: the guard must not join it.
        drop(guard);
        log::warn!("[computer] Esc: computer use aborted for {} run(s)", users.len());
        for hook in hooks {
            hook();
        }
    }

    /// The run ended: its abort state goes, and the lock with it when it
    /// was the last user.
    pub(crate) fn end(&self, session_id: &str) {
        self.runs.lock().unwrap().remove(session_id);
        let taken = {
            let mut lock = self.lock.lock().unwrap();
            lock.users.remove(session_id);
            if lock.users.is_empty() {
                Self::take(&mut lock)
            } else {
                None
            }
        };
        if taken.is_some() {
            self.released();
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
            host.sweep(Instant::now());
        });
    }

    /// The janitor's tick at `now`: never during a call, however long.
    fn sweep(&self, now: Instant) {
        let taken = {
            let mut lock = self.lock.lock().unwrap();
            if !lock.in_call && lock.last_used.is_some_and(|at| now.saturating_duration_since(at) > IDLE) {
                Self::take(&mut lock)
            } else {
                None
            }
        };
        if taken.is_some() {
            self.released();
        }
    }

    /// One call on `backend` (the OS's, a fake in tests).
    fn call_on(&self, backend: &dyn backend::Backend, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
        // One call at a time; a panic in an earlier call must not end
        // computer use for good.
        let _queued = self.calls.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // Esc while this call queued.
        if self.aborted(session_id) {
            return tools::error(tools::ABORTED);
        }
        // NEW-59: the lock engages only once the work-screen gate let the
        // call through: a refusal leaves the person's input alone.
        let engage = || {
            self.lease(session_id, label, backend).map_err(|refused| match refused {
                Refused::Aborted => tools::ABORTED.to_string(),
                Refused::Os(reason) => {
                    format!("Could not lock the person's input ({reason}); nothing was done. Tell them and try again.")
                }
            })
        };
        let interrupted = || self.aborted(session_id);
        let (mut display, mut copy_mark) =
            self.runs.lock().unwrap().get(session_id).map(|run| (run.display, run.copy_mark)).unwrap_or_default();
        let mut ctx = tools::CallContext {
            interrupted: &interrupted,
            display: &mut display,
            copy_mark: &mut copy_mark,
            picked: crate::work_screen::picked(),
            engage: &engage,
        };
        let result = tools::call(backend, name, &arguments, &mut ctx);
        {
            let mut runs = self.runs.lock().unwrap();
            let run = runs.entry(session_id.to_string()).or_default();
            run.display = display;
            run.copy_mark = copy_mark;
        }
        // The call is over: the idle clock starts now.
        {
            let mut lock = self.lock.lock().unwrap();
            lock.in_call = false;
            if !interrupted() {
                lock.last_used = Some(Instant::now());
            }
        }
        // Esc during the action: what it did is moot, the answer is the abort.
        if interrupted() {
            return tools::error(tools::ABORTED);
        }
        result
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
        self.call_on(&*backend, session_id, label, name, arguments)
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
        let Err(Refused::Os(err)) = host.lease("s3", "EXP-3", &refusing) else { panic!("the lock was not refused") };
        assert!(err.contains("keyboard"));
        assert!(!host.engaged());
        host.lease("s3", "EXP-3", &backend).unwrap();
        host.shutdown();
        assert!(!host.engaged());
    }

    /// The fake backend with a hook at the window list (read by the gate,
    /// between a call's abort check and its lease) and one at every typed
    /// character (the middle of an action).
    #[derive(Default)]
    struct Hooked {
        inner: tools::tests::FakeBackend,
        on_windows: Option<Box<dyn Fn() + Send + Sync>>,
        on_type: Option<Box<dyn Fn() + Send + Sync>>,
    }

    impl backend::Backend for Hooked {
        fn displays(&self) -> Result<Vec<backend::DisplayInfo>, String> {
            self.inner.displays()
        }
        fn screenshot(&self, display: &backend::DisplayInfo) -> Result<backend::Image, String> {
            self.inner.screenshot(display)
        }
        fn cursor_position(&self) -> Result<(i32, i32), String> {
            self.inner.cursor_position()
        }
        fn list_windows(&self) -> Result<Vec<backend::WindowInfo>, String> {
            if let Some(hook) = &self.on_windows {
                hook();
            }
            self.inner.list_windows()
        }
        fn activate_window(&self, id: u64) -> Result<(), String> {
            self.inner.activate_window(id)
        }
        fn mouse_move(&self, x: i32, y: i32) -> Result<(), String> {
            self.inner.mouse_move(x, y)
        }
        fn mouse_button(&self, button: backend::Button, down: bool) -> Result<(), String> {
            self.inner.mouse_button(button, down)
        }
        fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
            self.inner.scroll(dx, dy)
        }
        fn key(&self, key: crate::keys::Key, down: bool) -> Result<(), String> {
            self.inner.key(key, down)
        }
        fn type_char(&self, ch: char) -> Result<(), String> {
            if let Some(hook) = &self.on_type {
                hook();
            }
            self.inner.type_char(ch)
        }
        fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
            self.inner.lock(on_esc)
        }
    }

    fn typed(backend: &Hooked) -> Vec<String> {
        backend.inner.log.lock().unwrap().iter().filter(|line| line.starts_with("type ")).cloned().collect()
    }

    /// #EXP-1893: 40 s into one call (a long `type`, a drag, a wait) the
    /// janitor leaves the hold alone; idle past the call's end it releases.
    #[test]
    fn a_call_longer_than_idle_keeps_the_hold() {
        let host = Host::new();
        let held_mid_call = Arc::new(Mutex::new(Vec::new()));
        let backend = Hooked {
            on_type: Some({
                let (host, held) = (host.clone(), held_mid_call.clone());
                Box::new(move || {
                    host.sweep(Instant::now() + Duration::from_secs(40));
                    held.lock().unwrap().push(host.engaged());
                })
            }),
            ..Default::default()
        };
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "ab" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert_eq!(held_mid_call.lock().unwrap().as_slice(), [true, true]);
        assert_eq!(typed(&backend), ["type a", "type b"]);
        assert!(host.engaged());
        host.sweep(Instant::now() + Duration::from_secs(20));
        assert!(host.engaged());
        host.sweep(Instant::now() + Duration::from_secs(40));
        assert!(!host.engaged(), "an idle hold still releases");
    }

    /// #EXP-1893: calls queue; two runs never drive the one pointer and
    /// keyboard at the same time.
    #[test]
    fn two_runs_calls_never_overlap() {
        let host = Host::new();
        let inside = Arc::new(Mutex::new((0, 0)));
        let backend = Hooked {
            on_type: Some({
                let inside = inside.clone();
                Box::new(move || {
                    {
                        let mut inside = inside.lock().unwrap();
                        inside.0 += 1;
                        inside.1 = inside.1.max(inside.0);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                    inside.lock().unwrap().0 -= 1;
                })
            }),
            ..Default::default()
        };
        let start = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for (session_id, text) in [("s1", "aaaaaa"), ("s2", "bbbbbb")] {
                let (host, backend, start) = (&host, &backend, &start);
                scope.spawn(move || {
                    start.wait();
                    let reply = host.call_on(backend, session_id, session_id, "type", json!({ "text": text }));
                    assert_eq!(reply["isError"], Value::Null, "{reply}");
                });
            }
        });
        assert_eq!(inside.lock().unwrap().1, 1, "two calls typed at once");
        let typed = typed(&backend).concat();
        assert!(typed == "type a".repeat(6) + &"type b".repeat(6) || typed == "type b".repeat(6) + &"type a".repeat(6), "{typed}");
        host.shutdown();
    }

    /// #EXP-1893: the person's Esc lands after a call's abort check and
    /// before its lease: the call answers the abort, does nothing and the
    /// hold stays released.
    #[test]
    fn esc_between_the_abort_check_and_the_lease_leaves_the_hold_released() {
        let host = Host::new();
        let armed = Arc::new(Mutex::new(false));
        let backend = Hooked {
            on_windows: Some({
                let (host, armed) = (host.clone(), armed.clone());
                Box::new(move || {
                    if std::mem::take(&mut *armed.lock().unwrap()) {
                        host.abort();
                    }
                })
            }),
            ..Default::default()
        };
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "a" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert!(host.engaged());
        *armed.lock().unwrap() = true;
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "b" }));
        assert_eq!(reply["isError"], true);
        assert_eq!(reply["content"][0]["text"], tools::ABORTED);
        assert!(!host.engaged(), "the call took the hold again after Esc");
        assert_eq!(typed(&backend), ["type a"]);
        assert_eq!(backend.inner.log.lock().unwrap().iter().filter(|line| *line == "lock").count(), 1);
    }

    /// #EXP-1893: the last user's end and another run's lease race: either
    /// order leaves the leasing run with the hold.
    #[test]
    fn an_end_never_takes_the_hold_from_a_run_that_just_leased() {
        let host = Host::new();
        let backend = tools::tests::FakeBackend::default();
        for round in 0..5_000 {
            host.lease("s1", "EXP-1", &backend).unwrap();
            let start = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    start.wait();
                    host.end("s1");
                });
                scope.spawn(|| {
                    start.wait();
                    host.lease("s2", "EXP-2", &backend).unwrap();
                });
            });
            assert!(host.engaged(), "round {round}: s2 leased and lost the hold");
            host.shutdown();
        }
    }
}
