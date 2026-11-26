//! EXP-1489 / #EXP-2144: WHEN a pending update restarts its host — the one
//! policy the CLI daemon and the desktop app share. A session's agent is a
//! child of the host speaking ACP over pipes, so it cannot outlive the
//! restart and be re-adopted; instead the host
//!
//! 1. waits for a moment every session is between turns ([`TURN_GAP`]);
//! 2. past [`DRAIN_AFTER`] DRAINS: no new turn starts (messages wait in their
//!    rows, the new process delivers them) until every running turn reached
//!    its edge;
//! 3. past [`MAX_WAIT`] restarts anyway: a run cut mid-turn reads `restarted`
//!    (resume) and the new process continues it by itself (pickup `Continue`
//!    / NEW-5's re-claim), so no turn is lost and nothing waits for a person.
//!
//! What a resume cannot give back (a person mid-sentence or in a call, a
//! start whose first message was already taken) holds it longer, at most
//! [`HOLD_MAX`]: no focused window, no endless turn and no open call keeps a
//! build forever.

use std::time::{Duration, Instant};

/// How long every live session must sit between turns before the restart
/// goes through: enough for a turn's follow-up (a queued message, a
/// child's report) to start first.
pub const TURN_GAP: Duration = Duration::from_secs(10);

/// The same while draining: nothing new starts, so an edge is an edge.
pub const DRAIN_GAP: Duration = Duration::from_secs(3);

/// How long the host waits for such a moment on its own before it makes one.
pub const DRAIN_AFTER: Duration = Duration::from_secs(45 * 60);

/// The longest a live turn may hold a ready update. Past it the host
/// restarts anyway and the cut turns continue on the new version.
/// EXP-1675: an hour. A continuation loses the turn's live tool state, so
/// it is the last resort. A person's "Update now" and a build the server
/// refuses still go at once.
pub const MAX_WAIT: Duration = Duration::from_secs(60 * 60);

/// A key or a click this recent = a person works in the app right now. A
/// focused window alone says nothing (it stays focused over night).
pub const TYPING_GAP: Duration = Duration::from_secs(60);

/// The longest anything a resume cannot give back holds it.
pub const HOLD_MAX: Duration = Duration::from_secs(4 * 60 * 60);

/// What a ready update may do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartGate {
    /// Suspend every live session and restart.
    Now,
    /// Not yet, and new turns start as usual.
    Wait,
    /// Not yet: new turns wait for the restart, running ones finish.
    Drain,
}

/// What the host sees right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Observed {
    /// The server refuses this build (below `CLIENT_MIN_VERSION`, HTTP 426),
    /// or a person asked for the update now: nothing is waited for.
    pub required: bool,
    /// Some live session has a turn or a background task (EXP-1675: a build,
    /// a test run) in flight.
    pub working: bool,
    /// Something whose cut the new process could not make good: a session
    /// between its claim and its first turn, an agent sign-in or CLI update.
    pub unresumable: bool,
    /// How long ago a person last typed or clicked in the app.
    pub typed_ago: Option<Duration>,
    /// A voice call, a meeting recording or a Local desktop's turn runs here.
    pub engaged: bool,
}

/// The gate for one tick. `rested` = how long no session has been working
/// (read from [`Watch`]); `waited` = how long the update has been ready.
/// Pure.
pub fn decide(seen: &Observed, rested: Duration, waited: Duration) -> RestartGate {
    if seen.required {
        return RestartGate::Now;
    }
    let holdable = waited < HOLD_MAX;
    // A drain would mute the very chat a call speaks to, or the answer to
    // what the person just typed: neither holds any turns.
    if holdable && (seen.engaged || seen.typed_ago.is_some_and(|ago| ago < TYPING_GAP)) {
        return RestartGate::Wait;
    }
    let draining = waited >= DRAIN_AFTER;
    let wait = if draining { RestartGate::Drain } else { RestartGate::Wait };
    if holdable && seen.unresumable {
        return wait;
    }
    let gap = if draining { DRAIN_GAP } else { TURN_GAP };
    if waited >= MAX_WAIT || (!seen.working && rested >= gap) {
        RestartGate::Now
    } else {
        wait
    }
}

/// [`decide`] over time: the caller ticks it with its clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct Watch {
    ready_since: Option<Instant>,
    last_working: Option<Instant>,
    speed: u32,
}

impl Watch {
    /// DEV: a watch whose wait passes `speed` times as fast.
    pub fn at_speed(speed: u32) -> Self {
        Watch { speed, ..Watch::default() }
    }

    /// One tick at `now`. `ready` = an update waits for the restart; its
    /// clock starts at the first such tick and stops with it.
    pub fn tick(&mut self, now: Instant, ready: bool, seen: &Observed) -> RestartGate {
        if seen.working {
            self.last_working = Some(now);
        }
        if !ready {
            self.ready_since = None;
            return RestartGate::Wait;
        }
        let since = *self.ready_since.get_or_insert(now);
        let rested = self.last_working.map_or(Duration::MAX, |at| now.saturating_duration_since(at));
        decide(seen, rested, now.saturating_duration_since(since) * self.speed.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);
    const MIN: Duration = Duration::from_secs(60);
    const WORKING: Observed =
        Observed { required: false, working: true, unresumable: false, typed_ago: None, engaged: false };
    const RESTING: Observed = Observed { working: false, ..WORKING };

    /// Ticks every 5 s with what `seen` says at that offset; the offset and
    /// the gates up to the restart.
    fn run(mut seen: impl FnMut(Duration) -> Observed) -> (Duration, Vec<RestartGate>) {
        let start = Instant::now();
        let mut watch = Watch::default();
        let mut gates = Vec::new();
        let mut at = Duration::ZERO;
        while at <= HOLD_MAX * 2 {
            let gate = watch.tick(start + at, true, &seen(at));
            if gate == RestartGate::Now {
                return (at, gates);
            }
            gates.push(gate);
            at += 5 * SEC;
        }
        panic!("never restarted");
    }

    #[test]
    fn nothing_live_restarts_at_once() {
        assert_eq!(run(|_| RESTING).0, Duration::ZERO);
    }

    #[test]
    fn a_pause_between_turns_lets_it_through_after_the_gap() {
        let (at, gates) = run(|at| if at < 5 * MIN { WORKING } else { RESTING });
        assert_eq!(at, 5 * MIN + TURN_GAP - 5 * SEC);
        assert!(gates.iter().all(|gate| *gate == RestartGate::Wait));
    }

    /// A turn's follow-up starts within the gap: no restart between them.
    #[test]
    fn back_to_back_turns_are_no_pause_until_the_drain_makes_one() {
        // A 5 s breath every minute; draining, the next turn never starts.
        let mut held = false;
        let start = Instant::now();
        let mut watch = Watch::default();
        let mut at = Duration::ZERO;
        loop {
            let breath = at.as_secs() % 60 >= 55;
            let seen = if breath || (held && at > DRAIN_AFTER + MIN) { RESTING } else { WORKING };
            match watch.tick(start + at, true, &seen) {
                RestartGate::Now => break,
                gate => held = gate == RestartGate::Drain,
            }
            at += 5 * SEC;
        }
        assert!(held);
        assert!(at > DRAIN_AFTER && at <= DRAIN_AFTER + MIN + 10 * SEC, "{at:?}");
    }

    /// One endless turn (or background task) is cut at the cap, never later.
    #[test]
    fn an_endless_turn_drains_then_is_cut_at_the_cap() {
        let (at, gates) = run(|_| WORKING);
        assert_eq!(at, MAX_WAIT);
        let first = gates.iter().position(|gate| *gate == RestartGate::Drain).unwrap();
        assert_eq!(first as u64 * 5, DRAIN_AFTER.as_secs());
        assert!(gates[first..].iter().all(|gate| *gate == RestartGate::Drain));
    }

    /// #EXP-2144: the MacBook. The window stays focused all day, nobody
    /// types: the build that read that as a person never restarted.
    #[test]
    fn a_focused_window_nobody_types_in_holds_nothing() {
        let idle = Observed { typed_ago: Some(TYPING_GAP), ..RESTING };
        assert_eq!(run(|_| idle).0, Duration::ZERO);
        let (at, _) = run(|at| Observed { typed_ago: Some(at), ..RESTING });
        assert_eq!(at, TYPING_GAP, "a minute after the last key");
    }

    #[test]
    fn typing_holds_it_bounded() {
        let typing = Observed { typed_ago: Some(SEC), ..WORKING };
        let (at, gates) = run(|_| typing);
        assert_eq!(at, HOLD_MAX);
        assert!(gates.iter().all(|gate| *gate == RestartGate::Wait), "their messages still get a turn");
        // Stopped after the cap: the next tick past the gap goes.
        let (at, _) = run(|at| if at < 2 * MAX_WAIT { typing } else { Observed { typed_ago: Some(MIN), ..WORKING } });
        assert_eq!(at, 2 * MAX_WAIT);
    }

    /// A call holds it, and never holds the turns of the chat it talks to.
    #[test]
    fn a_call_holds_it_bounded_and_drains_nothing() {
        let call = Observed { engaged: true, ..WORKING };
        let (at, gates) = run(|_| call);
        assert_eq!(at, HOLD_MAX);
        assert!(gates.iter().all(|gate| *gate == RestartGate::Wait));
    }

    /// A session between its claim and its first turn has nothing to resume
    /// from: waited for past the cap, with no new start beside it.
    #[test]
    fn a_start_in_flight_is_never_cut_before_the_bound() {
        let starting = Observed { unresumable: true, ..WORKING };
        let (at, gates) = run(|_| starting);
        assert_eq!(at, HOLD_MAX);
        assert_eq!(*gates.last().unwrap(), RestartGate::Drain);
        assert_eq!(run(|at| if at < 65 * MIN { starting } else { WORKING }).0, 65 * MIN);
    }

    #[test]
    fn a_refused_build_restarts_at_once_whatever_runs() {
        let seen = Observed { required: true, working: true, unresumable: true, typed_ago: Some(SEC), engaged: true };
        assert_eq!(run(|_| seen).0, Duration::ZERO);
    }

    /// The clock is the update's: work before it counts for the gap only.
    #[test]
    fn the_clock_starts_with_the_ready_update() {
        let start = Instant::now();
        let mut watch = Watch::default();
        assert_eq!(watch.tick(start, false, &WORKING), RestartGate::Wait);
        assert_eq!(watch.tick(start + MAX_WAIT * 3, true, &WORKING), RestartGate::Wait);
        assert_eq!(watch.tick(start + MAX_WAIT * 3 + DRAIN_AFTER, true, &WORKING), RestartGate::Drain);
        assert_eq!(watch.tick(start + MAX_WAIT * 4, true, &WORKING), RestartGate::Now);
    }
}
