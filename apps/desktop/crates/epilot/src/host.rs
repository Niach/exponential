//! The host behind the hub: one desktop, many runs, ONE lock.
//!
//! The lock (the person's keyboard and pointer taken, see `backend`) engages
//! on a run's first DRIVING call (`work_screen::drives`; a look or a wait
//! never takes or renews it, #EXP-1898) and stays while runs keep driving: a model thinks
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
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::backend::{self, LockGuard, Target};
use crate::server::ToolHost;
use crate::tools;

/// A lock nobody has used for this long releases itself.
pub(crate) const IDLE: Duration = Duration::from_secs(30);
const JANITOR_TICK: Duration = Duration::from_millis(500);
/// How long a run's end waits for its call in flight to return (the tools
/// poll the stop between steps): a call stuck in the OS must not keep the
/// person's input locked.
const END_WAIT: Duration = Duration::from_secs(5);

/// What a call answers when its run ended, or the switch went off, under it.
pub(crate) const ENDED: &str = "Computer use ended for this session (the session ended or the switch was turned off): the call was stopped and nothing more was done.";

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
    /// Counts every engage and release: the banner hears them in this
    /// order ([`Host::announce`]).
    seq: u64,
}

/// A hold taken out of [`LockState`].
struct Taken {
    guard: Box<dyn LockGuard>,
    users: HashSet<String>,
    seq: u64,
}

/// The calls inside [`Host::call_on`] (#EXP-1894).
#[derive(Default)]
struct Calls {
    /// Per run: its calls, queued ones included, and whether its end told
    /// them to stop.
    pending: HashMap<String, (usize, bool)>,
    /// The run whose call drives right now.
    running: Option<String>,
    /// The runs with a live grant ([`Host::admit`]); a call of any other
    /// answers [`ENDED`] (#EXP-1898).
    granted: HashSet<String>,
}

/// One call from its arrival to its return.
struct Arrived<'a>(&'a Host, &'a str);

impl Drop for Arrived<'_> {
    fn drop(&mut self) {
        let mut calls = self.0.in_flight.lock().unwrap();
        if let Some(pending) = calls.pending.get_mut(self.1) {
            pending.0 -= 1;
            if pending.0 == 0 {
                calls.pending.remove(self.1);
            }
        }
    }
}

/// The call that drives, until it returns.
struct Running<'a>(&'a Host);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.in_flight.lock().unwrap().running = None;
        self.0.returned.notify_all();
    }
}

#[derive(Default)]
struct Run {
    aborted: bool,
    hook: Option<AbortHook>,
    /// The display the run works on (`tools::CallContext`).
    display: Option<usize>,
    /// `tools::CallContext::copy_mark` and the hold it was armed under
    /// ([`LockState::seq`]). #EXP-1920: good only while that hold lasts;
    /// once it went the person had their input back, and a changed
    /// clipboard is theirs.
    copy_mark: Option<(u64, u64)>,
}

/// Why a call did not get the lock.
#[derive(Debug)]
enum Refused {
    /// The person's Esc aborted the run before the lease.
    Aborted,
    /// The run ended before the lease.
    Ended,
    /// The OS refused the lock.
    Os(String),
}

#[derive(Default)]
pub(crate) struct Host {
    /// Held for a whole DRIVING call: they queue, so two runs (or one run's
    /// parallel calls) never drive the one pointer, keyboard and clipboard
    /// at the same time (#EXP-1893). A look or a wait is not in the queue
    /// (#EXP-1898).
    calls: Mutex<()>,
    lock: Mutex<LockState>,
    runs: Mutex<HashMap<String, Run>>,
    presence: Mutex<Option<Presence>>,
    /// The [`LockState::seq`] the banner heard last; held while it hears.
    announced: Mutex<u64>,
    in_flight: Mutex<Calls>,
    /// The running call returned.
    returned: Condvar,
    janitor: Mutex<bool>,
    /// For the lock's Esc callback.
    weak: Mutex<Weak<Host>>,
    /// Whose input this host's lock takes (#EXP-1924): everything above is
    /// this target's alone.
    target: Target,
}

impl Host {
    /// The person's seat's host.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new() -> Arc<Host> {
        Host::on(Target::Person)
    }

    /// `target`'s host.
    pub(crate) fn on(target: Target) -> Arc<Host> {
        let host = Arc::new(Host { target, ..Host::default() });
        *host.weak.lock().unwrap() = Arc::downgrade(&host);
        host
    }

    pub(crate) fn set_presence(&self, presence: Presence) {
        let _order = self.announced.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let engaged = {
            let lock = self.lock.lock().unwrap();
            lock.held.as_ref().map(|_| lock.label.clone())
        };
        *self.presence.lock().unwrap() = Some(presence.clone());
        presence(engaged);
    }

    /// Tell the banner of the engage (`Some`) or release that was the
    /// hold's change number `seq`. The changes happen under the lock's
    /// mutex and are told outside it, so a release can get here after the
    /// engage that followed it: an older change than the last one told is
    /// dropped, and the banner ends on what the hold is (#EXP-1894).
    fn announce(&self, seq: u64, label: Option<String>) {
        let mut announced = self.announced.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if seq <= *announced {
            return;
        }
        *announced = seq;
        if label.is_none() {
            crate::work_screen::set_banner(self.target, None);
        }
        let presence = self.presence.lock().unwrap().clone();
        if let Some(presence) = presence {
            presence(label);
        }
    }

    /// Whether the person's input is taken right now.
    pub(crate) fn engaged(&self) -> bool {
        self.lock.lock().unwrap().held.is_some()
    }

    /// The run got its grant (`crate::grant`): its calls run until its end
    /// or the switch going off ([`Host::stop_calls`]).
    pub(crate) fn admit(&self, session_id: &str) {
        self.in_flight.lock().unwrap().granted.insert(session_id.to_string());
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
    /// ([`Host::call_on`]): engage it if nobody does. Answers the hold's
    /// [`LockState::seq`].
    fn lease(&self, session_id: &str, label: &str, backend: &dyn backend::Backend) -> Result<u64, Refused> {
        let mut lock = self.lock.lock().unwrap();
        // Read under the lock's mutex, where `abort` writes it: an Esc that
        // landed since the call's first check never gets the lock re-taken.
        if self.aborted(session_id) {
            return Err(Refused::Aborted);
        }
        if self.stopped(session_id) {
            return Err(Refused::Ended);
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
            lock.seq += 1;
            true
        } else {
            false
        };
        lock.last_used = Some(Instant::now());
        lock.in_call = true;
        lock.users.insert(session_id.to_string());
        let (label, seq) = (lock.label.clone(), lock.seq);
        drop(lock);
        if engaged {
            log::info!("[computer] input locked for {label}");
            self.announce(seq, Some(label));
            self.start_janitor();
        }
        Ok(seq)
    }

    /// Take the lock out of `lock`, under its mutex (whoever decides the
    /// release does it there: a run leasing meanwhile engages anew);
    /// `None` when nothing was held. [`Host::released`] follows.
    fn take(lock: &mut LockState) -> Option<Taken> {
        let guard = lock.held.take()?;
        let users = std::mem::take(&mut lock.users);
        lock.last_used = None;
        lock.in_call = false;
        lock.seq += 1;
        Some(Taken { guard, users, seq: lock.seq })
    }

    /// The lock went ([`Host::take`]): say so.
    fn released(&self, taken: &Taken) {
        log::info!("[computer] input unlocked");
        self.announce(taken.seq, None);
    }

    /// Whether `session_id`'s end told its calls to stop.
    fn stopped(&self, session_id: &str) -> bool {
        self.in_flight.lock().unwrap().pending.get(session_id).is_some_and(|pending| pending.1)
    }

    /// #EXP-1894: tell `session_id`'s calls (every run's with `None`) to
    /// stop: the one driving reads it between its steps
    /// (`tools::CallContext::interrupted`), a queued one answers
    /// [`ENDED`] and does nothing. Returns once the driving one returned,
    /// so the hold never goes from under an action; [`END_WAIT`] at most.
    fn stop_calls(&self, session_id: Option<&str>) {
        let ours = |id: &String| session_id.is_none_or(|session_id| session_id == id);
        let mut calls = self.in_flight.lock().unwrap();
        // The grant goes with the stop, under one mutex: a call arriving
        // from here on answers the end and does nothing (#EXP-1898).
        calls.granted.retain(|id| !ours(id));
        for (id, pending) in calls.pending.iter_mut() {
            if ours(id) {
                pending.1 = true;
            }
        }
        let (_calls, wait) =
            self.returned.wait_timeout_while(calls, END_WAIT, |calls| calls.running.as_ref().is_some_and(ours)).unwrap();
        if wait.timed_out() {
            log::warn!("[computer] a call did not stop within {END_WAIT:?} of its run's end");
        }
    }

    /// The person pressed Esc (on the lock's thread; a hold that died
    /// reads the same): unlock, abort every run that used it, interrupt
    /// their turns.
    pub(crate) fn abort(&self) {
        let (taken, hooks) = {
            let mut lock = self.lock.lock().unwrap();
            let Some(taken) = Self::take(&mut lock) else { return };
            // Marked before the lock's mutex goes: `lease` reads it there.
            let mut runs = self.runs.lock().unwrap();
            let hooks: Vec<AbortHook> = taken
                .users
                .iter()
                .filter_map(|session_id| {
                    let run = runs.entry(session_id.clone()).or_default();
                    run.aborted = true;
                    run.hook.clone()
                })
                .collect();
            (taken, hooks)
        };
        self.released(&taken);
        let Taken { guard, users, .. } = taken;
        // Releasing on the lock's own thread: the guard must not join it.
        drop(guard);
        log::warn!("[computer] Esc: computer use aborted for {} run(s)", users.len());
        for hook in hooks {
            hook();
        }
    }

    /// The run ended: its call in flight stops first ([`Host::stop_calls`]),
    /// then its abort state goes, and the lock with it when it was the last
    /// user.
    pub(crate) fn end(&self, session_id: &str) {
        self.stop_calls(Some(session_id));
        // #EXP-1925: the run's own desktop grant goes with it.
        backend::end_run(self.target, session_id);
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
        if let Some(taken) = taken {
            self.released(&taken);
        }
    }

    /// The switch turned off: every call stops, then everything goes.
    pub(crate) fn shutdown(&self) {
        self.stop_calls(None);
        self.runs.lock().unwrap().clear();
        let taken = Self::take(&mut self.lock.lock().unwrap());
        if let Some(taken) = taken {
            self.released(&taken);
        }
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
        if let Some(taken) = taken {
            self.released(&taken);
        }
    }

    /// One call on `backend` (the OS's, a fake in tests).
    fn call_on(&self, backend: &dyn backend::Backend, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
        {
            let mut calls = self.in_flight.lock().unwrap();
            // #EXP-1898: admitted by the HTTP layer just before the run's
            // grant went, here after its end.
            if !calls.granted.contains(session_id) {
                return tools::error(ENDED);
            }
            // Counted from here, so the run's end reaches it while it queues.
            calls.pending.entry(session_id.to_string()).or_default().0 += 1;
        }
        let _arrived = Arrived(self, session_id);
        // #EXP-1898: a look or a wait reads, it does not drive: no queue,
        // no hold, and the run's end does not wait for it.
        let drives = crate::work_screen::drives(name);
        // One driving call at a time; a panic in an earlier call must not
        // end computer use for good.
        let _queued = drives.then(|| self.calls.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        // The run ended while this call queued; else it drives from here,
        // and the run's end waits for it (dropped before `_queued`).
        let _running = {
            let mut calls = self.in_flight.lock().unwrap();
            if calls.pending.get(session_id).is_some_and(|pending| pending.1) {
                return tools::error(ENDED);
            }
            drives.then(|| {
                calls.running = Some(session_id.to_string());
                Running(self)
            })
        };
        // Esc while this call queued.
        if self.aborted(session_id) {
            return tools::error(tools::ABORTED);
        }
        // NEW-59: the lock engages only once the work-screen gate let the
        // call through: a refusal leaves the person's input alone.
        // #EXP-1920: the hold as it is now; the run's copy mark is good
        // only when it was armed under this very hold.
        let hold = {
            let lock = self.lock.lock().unwrap();
            lock.held.as_ref().map(|_| lock.seq)
        };
        let (mut display, armed) =
            self.runs.lock().unwrap().get(session_id).map(|run| (run.display, run.copy_mark)).unwrap_or_default();
        let mut copy_mark = armed.filter(|(_, under)| hold == Some(*under)).map(|(mark, _)| mark);
        let leased = std::cell::Cell::new(None);
        let engage = || {
            // #EDE-126: a space has no person to hold out: no lock, no
            // hold thread, no Esc. Only the run's end stops it.
            if self.target != Target::Person {
                return if self.stopped(session_id) { Err(ENDED.to_string()) } else { Ok(()) };
            }
            let seq = self.lease(session_id, label, backend).map_err(|refused| match refused {
                Refused::Aborted => tools::ABORTED.to_string(),
                Refused::Ended => ENDED.to_string(),
                Refused::Os(reason) => {
                    format!("Could not lock the person's input ({reason}); nothing was done. Tell them and try again.")
                }
            })?;
            leased.set(Some(seq));
            // The hold went and came back since the mark was read above.
            if name == "read_clipboard" && hold.is_some_and(|hold| hold != seq) {
                return Err(tools::READ_REFUSED.to_string());
            }
            Ok(())
        };
        // The person's Esc, or the run's end (#EXP-1894).
        let interrupted = || self.aborted(session_id) || self.stopped(session_id);
        let mut ctx = tools::CallContext {
            interrupted: &interrupted,
            display: &mut display,
            copy_mark: &mut copy_mark,
            picked: crate::work_screen::picked(self.target),
            engage: &engage,
        };
        let result = tools::call(backend, name, &arguments, &mut ctx);
        // An ended run keeps no state.
        if !self.stopped(session_id) {
            let mut runs = self.runs.lock().unwrap();
            let run = runs.entry(session_id.to_string()).or_default();
            run.display = display;
            // A look runs beside the run's driving call: the mark is not its.
            if drives {
                let under = leased.get().or(hold).unwrap_or_default();
                run.copy_mark = copy_mark.map(|mark| (mark, under));
            }
        }
        // #EXP-1920: a paste or a copy changes the clipboard and leaves the
        // person's on it: no other run's mark may read that as its own copy.
        if matches!(name, "paste_text" | "copy_selection") {
            for (other, run) in self.runs.lock().unwrap().iter_mut() {
                if other != session_id {
                    run.copy_mark = None;
                }
            }
        }
        // The driving call is over: the idle clock starts now.
        if drives {
            let mut lock = self.lock.lock().unwrap();
            lock.in_call = false;
            if !interrupted() {
                lock.last_used = Some(Instant::now());
            }
        }
        if self.stopped(session_id) {
            return tools::error(ENDED);
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
        if !self.in_flight.lock().unwrap().granted.contains(session_id) {
            return tools::error(ENDED);
        }
        let backend = match backend::open(self.target, Some(session_id)) {
            Ok(backend) => backend,
            Err(reason) => return tools::error(reason),
        };
        self.call_on(&*backend, session_id, label, name, arguments)
    }

    fn end(&self, session_id: &str) {
        Host::end(self, session_id);
    }
}

/// Every target's host, made on its first run, and each run's target
/// (#EXP-1924): the endpoint's [`ToolHost`]; a call or an end goes to the
/// run's target's host only.
#[derive(Default)]
pub(crate) struct Hosts {
    hosts: Mutex<HashMap<Target, Arc<Host>>>,
    runs: Mutex<HashMap<String, Target>>,
    /// The banner: the person's seat's ([`Host::set_presence`]).
    presence: Mutex<Option<Presence>>,
    /// The last [`Target::Space`] number handed out.
    spaces: std::sync::atomic::AtomicU32,
}

impl Hosts {
    fn host(&self, target: Target) -> Arc<Host> {
        if let Some(host) = self.hosts.lock().unwrap().get(&target) {
            return host.clone();
        }
        let (host, made) = match self.hosts.lock().unwrap().entry(target) {
            std::collections::hash_map::Entry::Occupied(host) => (host.get().clone(), false),
            std::collections::hash_map::Entry::Vacant(slot) => (slot.insert(Host::on(target)).clone(), true),
        };
        // Told outside the map's mutex: the banner may read the host.
        if made && target == Target::Person {
            if let Some(presence) = self.presence.lock().unwrap().clone() {
                host.set_presence(presence);
            }
        }
        host
    }

    /// The run's target's host; `None` = never admitted.
    fn of(&self, session_id: &str) -> Option<Arc<Host>> {
        let target = *self.runs.lock().unwrap().get(session_id)?;
        Some(self.host(target))
    }

    /// The banner hears the person's seat's host (made here when it is not
    /// yet), at once with its state.
    pub(crate) fn set_presence(&self, presence: Presence) {
        *self.presence.lock().unwrap() = Some(presence.clone());
        let person = self.hosts.lock().unwrap().get(&Target::Person).cloned();
        match person {
            Some(person) => person.set_presence(presence),
            None => drop(self.host(Target::Person)),
        }
    }

    /// Whether the person's input is taken right now ([`Host::engaged`]).
    pub(crate) fn engaged(&self) -> bool {
        self.hosts.lock().unwrap().get(&Target::Person).is_some_and(|host| host.engaged())
    }

    /// [`Host::admit`] on `target`'s host.
    pub(crate) fn admit(&self, session_id: &str, target: Target) {
        self.runs.lock().unwrap().insert(session_id.to_string(), target);
        self.host(target).admit(session_id);
    }

    /// #EDE-126: admit `session_id` as a space run: a target, and so a
    /// host, of its own; the one it has when it was admitted before.
    pub(crate) fn admit_space(&self, session_id: &str) -> Target {
        let known = self.runs.lock().unwrap().get(session_id).copied().filter(|target| *target != Target::Person);
        let target = known.unwrap_or_else(|| Target::Space(self.spaces.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1));
        self.admit(session_id, target);
        target
    }

    pub(crate) fn set_hook(&self, session_id: &str, hook: AbortHook) {
        if let Some(host) = self.of(session_id) {
            host.set_hook(session_id, hook);
        }
    }

    pub(crate) fn turn_started(&self, session_id: &str) {
        if let Some(host) = self.of(session_id) {
            host.turn_started(session_id);
        }
    }

    pub(crate) fn shutdown(&self) {
        let hosts: Vec<Arc<Host>> = self.hosts.lock().unwrap().values().cloned().collect();
        for host in hosts {
            host.shutdown();
        }
    }
}

impl ToolHost for Hosts {
    fn tools(&self) -> Result<Value, String> {
        Ok(tools::list())
    }

    fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
        match self.of(session_id) {
            Some(host) => host.call(session_id, label, name, arguments),
            None => tools::error(ENDED),
        }
    }

    fn end(&self, session_id: &str) {
        // Forgotten first: a grant for the same id from here on is a new run.
        let target = self.runs.lock().unwrap().remove(session_id);
        if let Some(target) = target {
            self.host(target).end(session_id);
            // A space's host is its one run's: it goes with it.
            if target != Target::Person {
                self.hosts.lock().unwrap().remove(&target);
            }
        }
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
        fn clipboard(&self) -> Result<Box<dyn backend::Clipboard + '_>, String> {
            self.inner.clipboard()
        }
    }

    /// A host whose runs `s1` and `s2` hold a grant.
    fn granted() -> Arc<Host> {
        let host = Host::new();
        host.admit("s1");
        host.admit("s2");
        host
    }

    fn typed(backend: &Hooked) -> Vec<String> {
        backend.inner.log.lock().unwrap().iter().filter(|line| line.starts_with("type ")).cloned().collect()
    }

    /// #EXP-1893: 40 s into one call (a long `type`, a drag, a wait) the
    /// janitor leaves the hold alone; idle past the call's end it releases.
    #[test]
    fn a_call_longer_than_idle_keeps_the_hold() {
        let host = granted();
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

    /// #EXP-1920: `read_clipboard`'s mark is good for the hold it was armed
    /// under and until another run's paste or copy: once the person had
    /// their input back, or another run put their clipboard back, a changed
    /// clipboard is theirs, never the run's own copy.
    #[test]
    fn read_clipboard_never_reads_past_the_hold_or_another_runs_paste() {
        let host = granted();
        let backend = Hooked::default();
        let person_copies = |text: &str| {
            let mut clip = backend.inner.clip.lock().unwrap();
            clip.text = Some(text.to_string());
            clip.count += 1;
        };
        let refused = |reply: &Value| reply["isError"] == true && !reply.to_string().contains("password");
        // A click that copies nothing; the hold goes (idle); the person
        // copies; the run reads.
        host.call_on(&backend, "s1", "EXP-1", "left_click", json!({ "x": 1, "y": 1 }));
        host.sweep(Instant::now() + Duration::from_secs(40));
        assert!(!host.engaged());
        person_copies("the person's password");
        let reply = host.call_on(&backend, "s1", "EXP-1", "read_clipboard", json!({}));
        assert!(refused(&reply), "after the hold went: {reply}");
        // The same through a call that arms nothing in between.
        host.call_on(&backend, "s1", "EXP-1", "left_click", json!({ "x": 1, "y": 1 }));
        host.sweep(Instant::now() + Duration::from_secs(40));
        person_copies("another password");
        host.call_on(&backend, "s1", "EXP-1", "mouse_move", json!({ "x": 2, "y": 2 }));
        let reply = host.call_on(&backend, "s1", "EXP-1", "read_clipboard", json!({}));
        assert!(refused(&reply), "after a later hold: {reply}");
        // Under one hold: run 1 clicks, run 2 pastes (its set and restore
        // change the clipboard and leave the person's on it), run 1 reads.
        host.call_on(&backend, "s1", "EXP-1", "left_click", json!({ "x": 1, "y": 1 }));
        let pasted = host.call_on(&backend, "s2", "EXP-2", "paste_text", json!({ "text": "hello" }));
        assert_eq!(pasted["isError"], Value::Null, "{pasted}");
        let reply = host.call_on(&backend, "s1", "EXP-1", "read_clipboard", json!({}));
        assert!(refused(&reply), "after another run's paste: {reply}");
        // The run's own copy under the hold still reads, once.
        backend.inner.clip.lock().unwrap().copies = Some("a token".into());
        host.call_on(&backend, "s1", "EXP-1", "key", json!({ "combo": "ctrl+c" }));
        assert!(host.engaged());
        assert_eq!(host.call_on(&backend, "s1", "EXP-1", "read_clipboard", json!({}))["content"][0]["text"], "a token");
        assert_eq!(host.call_on(&backend, "s1", "EXP-1", "read_clipboard", json!({}))["isError"], true);
    }

    /// #EXP-1893: calls queue; two runs never drive the one pointer and
    /// keyboard at the same time.
    #[test]
    fn two_runs_calls_never_overlap() {
        let host = granted();
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
        let host = granted();
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

    /// #EXP-1898: looking and waiting never take the hold, and never renew
    /// one an input tool took.
    #[test]
    fn looking_and_waiting_never_take_or_extend_the_hold() {
        let host = granted();
        let backend = tools::tests::FakeBackend::default();
        let looks = [("screenshot", json!({})), ("list_windows", json!({})), ("cursor_position", json!({})), ("wait", json!({ "seconds": 0 }))];
        for (name, arguments) in &looks {
            let reply = host.call_on(&backend, "s1", "EXP-1", name, arguments.clone());
            assert_eq!(reply["isError"], Value::Null, "{name}: {reply}");
            assert!(!host.engaged(), "{name} took the person's input");
        }
        assert!(!backend.log.lock().unwrap().iter().any(|line| line == "lock"));
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "a" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert!(host.engaged(), "an input tool takes the hold");
        // 10 s after the input call, a look; past its 30 s the hold is idle.
        // #EXP-1905: 20 s of room before the real janitor releases it (the
        // four looks took 3.5 s on a loaded CI runner, and 1 s was not enough).
        let used = Instant::now().checked_sub(Duration::from_secs(10)).expect("a clock past 10 s");
        for (name, arguments) in &looks {
            host.lock.lock().unwrap().last_used = Some(used);
            host.call_on(&backend, "s1", "EXP-1", name, arguments.clone());
            assert_eq!(host.lock.lock().unwrap().last_used, Some(used), "{name} renewed the hold");
        }
        host.sweep(used + IDLE + Duration::from_secs(1));
        assert!(!host.engaged(), "a look kept the hold past its 30 s");
    }

    /// #EXP-1898: a look or a wait is not in the queue: run A's long `wait`
    /// never stalls run B's click.
    #[test]
    fn a_wait_never_stalls_another_runs_click() {
        let host = granted();
        let (waiting, entered) = std::sync::mpsc::channel();
        let waiter = Hooked {
            on_windows: Some({
                let waiting = Mutex::new(waiting);
                Box::new(move || {
                    let _ = waiting.lock().unwrap().send(());
                })
            }),
            ..Default::default()
        };
        let backend = tools::tests::FakeBackend::default();
        std::thread::scope(|scope| {
            let wait = scope.spawn(|| host.call_on(&waiter, "s1", "EXP-1", "wait", json!({ "seconds": 5 })));
            entered.recv_timeout(Duration::from_secs(5)).expect("the wait never started");
            let began = Instant::now();
            let reply = host.call_on(&backend, "s2", "EXP-2", "left_click", json!({ "x": 1, "y": 1 }));
            let took = began.elapsed();
            assert_eq!(reply["isError"], Value::Null, "{reply}");
            assert!(!wait.is_finished(), "the wait was over before the click");
            // The wait's run ends: it stops between two of its 50 ms steps.
            host.end("s1");
            assert_eq!(wait.join().unwrap()["content"][0]["text"], ENDED);
            assert!(took < Duration::from_secs(2), "the click queued behind another run's wait for {took:?}");
        });
        host.shutdown();
    }

    /// #EXP-1898: a screenshot in the middle of another run's `type`
    /// returns without waiting for it.
    #[test]
    fn a_screenshot_never_waits_for_another_runs_type() {
        let host = granted();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let backend = Hooked {
            on_type: Some({
                let (host, shown) = (host.clone(), shown.clone());
                Box::new(move || {
                    let (done, shot) = std::sync::mpsc::channel();
                    let host = host.clone();
                    std::thread::spawn(move || {
                        let backend = tools::tests::FakeBackend::default();
                        let _ = done.send(host.call_on(&backend, "s2", "EXP-2", "screenshot", json!({})));
                    });
                    shown.lock().unwrap().push(shot.recv_timeout(Duration::from_secs(2)).ok());
                })
            }),
            ..Default::default()
        };
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "a" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        let shown = shown.lock().unwrap();
        let shot = shown[0].as_ref().expect("the screenshot waited for the other run's type");
        assert_eq!(shot["isError"], Value::Null, "{shot}");
        host.shutdown();
    }

    /// #EXP-1898: a call the HTTP layer admitted just before the run's grant
    /// went reaches the host after the run's end: it answers the end, takes
    /// no hold and touches nothing.
    #[test]
    fn a_call_after_the_runs_end_answers_the_end_and_does_nothing() {
        let host = granted();
        let backend = tools::tests::FakeBackend::default();
        host.end("s1");
        for name in ["type", "screenshot"] {
            let reply = host.call_on(&backend, "s1", "EXP-1", name, json!({ "text": "a" }));
            assert_eq!(reply["isError"], true, "{name}: {reply}");
            assert_eq!(reply["content"][0]["text"], ENDED);
        }
        // The other run's grant stands; a new grant admits the run again.
        assert_eq!(host.call_on(&backend, "s2", "EXP-2", "cursor_position", json!({}))["isError"], Value::Null);
        assert!(backend.log.lock().unwrap().is_empty(), "{:?}", backend.log.lock().unwrap());
        host.shutdown();
        let reply = host.call_on(&backend, "s2", "EXP-2", "type", json!({ "text": "a" }));
        assert_eq!(reply["content"][0]["text"], ENDED, "the switch going off ends every grant");
        host.admit("s1");
        assert_eq!(host.call_on(&backend, "s1", "EXP-1", "cursor_position", json!({}))["isError"], Value::Null);
        host.end("s1");
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "a" }));
        assert_eq!(reply["isError"], true, "{reply}");
        assert_eq!(reply["content"][0]["text"], ENDED);
        assert!(!host.engaged(), "a call after the run's end took the person's input");
        assert!(backend.log.lock().unwrap().is_empty(), "{:?}", backend.log.lock().unwrap());
    }

    /// #EXP-1894: `stop` (the run's end, the switch going off) lands on
    /// another thread in the middle of a long `type`. Every character sent
    /// was sent under the hold and before `stop` returned; the rest is not
    /// sent.
    fn a_long_type_stops_at(stop: fn(&Host)) -> Value {
        use std::sync::atomic::{AtomicBool, Ordering};
        let host = granted();
        let returned = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let stopper = Arc::new(Mutex::new(None));
        let backend = Hooked {
            on_type: Some({
                let (host, returned, sent, stopper) = (host.clone(), returned.clone(), sent.clone(), stopper.clone());
                Box::new(move || {
                    let count = {
                        let mut sent = sent.lock().unwrap();
                        sent.push((host.engaged(), returned.load(Ordering::SeqCst)));
                        sent.len()
                    };
                    if count == 2 {
                        let (host, returned) = (host.clone(), returned.clone());
                        *stopper.lock().unwrap() = Some(std::thread::spawn(move || {
                            stop(&host);
                            returned.store(true, Ordering::SeqCst);
                        }));
                    }
                    // Room for the stop to land: it waits for this call.
                    let until = Instant::now() + Duration::from_millis(50);
                    while count >= 2 && !returned.load(Ordering::SeqCst) && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                })
            }),
            ..Default::default()
        };
        let reply = host.call_on(&backend, "s1", "EXP-1", "type", json!({ "text": "a".repeat(40) }));
        stopper.lock().unwrap().take().expect("the stop never ran").join().unwrap();
        let sent = sent.lock().unwrap();
        assert!(sent.len() < 40, "the call typed to its end");
        assert!(sent.iter().all(|(held, _)| *held), "typed with the person's input unlocked: {sent:?}");
        assert!(sent.iter().all(|(_, returned)| !*returned), "typed after the stop returned: {sent:?}");
        assert!(!host.engaged());
        reply
    }

    #[test]
    fn a_runs_end_stops_its_call_in_flight_before_the_hold_goes() {
        let reply = a_long_type_stops_at(|host| host.end("s1"));
        assert_eq!(reply["isError"], true, "{reply}");
        assert_eq!(reply["content"][0]["text"], ENDED);
    }

    #[test]
    fn the_switch_going_off_stops_a_call_in_flight_before_the_hold_goes() {
        let reply = a_long_type_stops_at(|host| host.shutdown());
        assert_eq!(reply["isError"], true, "{reply}");
    }

    /// #EXP-1894: the last user's end and another run's engage race: the
    /// banner ends on what the hold is, never on the release that came first.
    #[test]
    fn the_banner_ends_on_the_holds_state() {
        let host = Host::new();
        let last = Arc::new(Mutex::new(None));
        host.set_presence({
            let last = last.clone();
            Arc::new(move |label| *last.lock().unwrap() = label)
        });
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
            assert!(host.engaged());
            assert!(last.lock().unwrap().is_some(), "round {round}: the hold is on and the banner says released");
            host.shutdown();
            assert_eq!(*last.lock().unwrap(), None, "round {round}");
        }
    }

    /// #EXP-1924: two targets' hosts share no hold, no Esc and no queue.
    #[test]
    fn two_targets_share_no_lock_state_queue_or_abort() {
        let (a, b) = (Host::new(), Host::on(Target::Person));
        a.admit("s1");
        b.admit("s2");
        let backend = tools::tests::FakeBackend::default();
        a.lease("s1", "EXP-1", &backend).unwrap();
        assert!(a.engaged() && !b.engaged(), "a lease on one host engaged the other");
        // A driving call holds `a`'s queue: `b`'s driving call still runs.
        let (typing, inside) = std::sync::mpsc::channel();
        let (go, release) = std::sync::mpsc::channel::<()>();
        let blocked = Hooked {
            on_type: Some({
                let (typing, release) = (Mutex::new(typing), Mutex::new(release));
                Box::new(move || {
                    let _ = typing.lock().unwrap().send(());
                    let _ = release.lock().unwrap().recv_timeout(Duration::from_secs(5));
                })
            }),
            ..Default::default()
        };
        std::thread::scope(|scope| {
            let typed = scope.spawn(|| a.call_on(&blocked, "s1", "EXP-1", "type", json!({ "text": "a" })));
            inside.recv_timeout(Duration::from_secs(5)).expect("the type never started");
            let began = Instant::now();
            let reply = b.call_on(&backend, "s2", "EXP-2", "left_click", json!({ "x": 1, "y": 1 }));
            assert_eq!(reply["isError"], Value::Null, "{reply}");
            assert!(began.elapsed() < Duration::from_secs(2), "the other host's call queued behind this one");
            assert!(!typed.is_finished());
            go.send(()).unwrap();
            assert_eq!(typed.join().unwrap()["isError"], Value::Null);
        });
        assert!(b.engaged());
        // Esc on `a` aborts its users only; `b` keeps its hold.
        a.abort();
        assert!(a.aborted("s1") && !a.engaged());
        assert!(!b.aborted("s2") && b.engaged(), "an Esc on one host reached the other");
        a.shutdown();
        b.shutdown();
    }

    /// #EDE-126: a space has no person to hold out: a driving call takes
    /// no lock, starts no janitor and is no banner; the run's end still
    /// stops its calls.
    #[test]
    fn a_space_run_takes_no_hold_and_its_end_stops_it() {
        let host = Host::on(Target::Space(43));
        host.admit("s1");
        let backend = tools::tests::FakeBackend { space: Some(43), ..Default::default() };
        let click = json!({ "x": 10, "y": 10 });
        let reply = host.call_on(&backend, "s1", "EXP-1", "left_click", click.clone());
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert!(backend.log.lock().unwrap().iter().any(|line| line == "Left down"));
        assert!(!backend.log.lock().unwrap().iter().any(|line| line == "lock"), "a space run took the hold");
        assert!(!host.engaged() && !*host.janitor.lock().unwrap());
        host.end("s1");
        assert_eq!(host.call_on(&backend, "s1", "EXP-1", "left_click", click)["content"][0]["text"], ENDED);
    }

    /// #EDE-126: each space run has a target, and so a host, of its own; a
    /// run granted again keeps its target, and its end takes its host along.
    #[test]
    fn each_space_run_has_its_own_host() {
        let hosts = Hosts::default();
        hosts.admit("p", Target::Person);
        let (one, two) = (hosts.admit_space("s1"), hosts.admit_space("s2"));
        assert!(matches!((one, two), (Target::Space(a), Target::Space(b)) if a != b));
        assert_eq!(hosts.admit_space("s1"), one, "the same run, the same space");
        let (a, b, person) = (hosts.of("s1").unwrap(), hosts.of("s2").unwrap(), hosts.of("p").unwrap());
        assert!(!Arc::ptr_eq(&a, &b) && !Arc::ptr_eq(&a, &person) && !Arc::ptr_eq(&b, &person));
        assert_eq!((a.target, b.target), (one, two));
        ToolHost::end(&hosts, "s1");
        assert!(hosts.of("s1").is_none());
        assert_eq!(hosts.hosts.lock().unwrap().len(), 2, "an ended space run leaves no host");
        assert_ne!(hosts.admit_space("s1"), one, "a new run, a new space");
        ToolHost::end(&hosts, "p");
        assert!(hosts.hosts.lock().unwrap().contains_key(&Target::Person), "the person's host stays");
    }

    /// #EXP-1924: `Hosts` sends each run's admit, call, turn and end to its
    /// target's host, and a shutdown to every host. (No call here reaches
    /// a backend: each answers before one opens.)
    #[test]
    fn hosts_route_each_run_to_its_targets_host() {
        let hosts = Hosts::default();
        hosts.admit("s1", Target::Person);
        hosts.admit("s2", Target::Person);
        let person = hosts.host(Target::Person);
        assert!(Arc::ptr_eq(&person, &hosts.host(Target::Person)));
        assert!(person.in_flight.lock().unwrap().granted.contains("s1"));
        let cancelled = Arc::new(Mutex::new(0));
        hosts.set_hook("s1", {
            let cancelled = cancelled.clone();
            Arc::new(move || *cancelled.lock().unwrap() += 1)
        });
        let backend = tools::tests::FakeBackend::default();
        person.lease("s1", "EXP-1", &backend).unwrap();
        assert!(hosts.engaged());
        person.abort();
        assert_eq!(*cancelled.lock().unwrap(), 1);
        assert_eq!(hosts.call("s1", "EXP-1", "cursor_position", json!({}))["content"][0]["text"], tools::ABORTED);
        hosts.turn_started("s1");
        assert!(!person.aborted("s1"));
        hosts.end("s2");
        assert!(!person.in_flight.lock().unwrap().granted.contains("s2"));
        assert_eq!(hosts.call("s2", "EXP-2", "cursor_position", json!({}))["content"][0]["text"], ENDED);
        assert_eq!(hosts.call("s9", "EXP-9", "cursor_position", json!({}))["content"][0]["text"], ENDED);
        person.lease("s1", "EXP-1", &backend).unwrap();
        hosts.shutdown();
        assert!(!hosts.engaged() && !person.engaged());
        assert!(person.in_flight.lock().unwrap().granted.is_empty());
    }
}
