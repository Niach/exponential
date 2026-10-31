//! EXP-1489: WHEN a pending update restarts its host — the one policy the
//! CLI daemon and the desktop app share. A live session never holds an
//! update for long: its agent is a child of the host speaking ACP over
//! pipes, so it cannot outlive the restart and be re-adopted; instead the
//! host waits for the next pause between turns, at most [`MAX_WAIT`], then
//! suspends every run. A run cut mid-turn reads `restarted` (resume) and the
//! new process continues it by itself (pickup `Continue` / NEW-5's
//! re-claim), so nothing waits for a person.

use std::time::Duration;

/// How long every live session must sit between turns before the restart
/// goes through: enough for a turn's follow-up (a queued message, a
/// child's report) to start first.
pub const TURN_GAP: Duration = Duration::from_secs(60);

/// The longest a live turn may hold a ready update. Past it the host
/// restarts anyway and the cut turns continue on the new version.
pub const MAX_WAIT: Duration = Duration::from_secs(10 * 60);

/// What a ready update may do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartGate {
    /// Suspend every live session and restart.
    Now,
    /// A turn is in flight (or just ended) and the cap is not reached.
    Wait,
}

/// The gate for one tick. `idle_for` = each live session's stretch
/// between turns (`None` = a turn is in flight); `waited` = how long the
/// update has been pending; `required` = the server refuses this build
/// (below `CLIENT_MIN_VERSION`, HTTP 426): restart at once, the host is
/// dead weight until it does. Pure.
pub fn restart_gate(
    idle_for: impl IntoIterator<Item = Option<Duration>>,
    waited: Duration,
    required: bool,
) -> RestartGate {
    if required || waited >= MAX_WAIT {
        return RestartGate::Now;
    }
    let paused = idle_for
        .into_iter()
        .all(|idle| idle.is_some_and(|idle| idle >= TURN_GAP));
    if paused {
        RestartGate::Now
    } else {
        RestartGate::Wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    #[test]
    fn nothing_live_restarts_at_once() {
        assert_eq!(restart_gate([], Duration::ZERO, false), RestartGate::Now);
    }

    #[test]
    fn a_pause_between_turns_lets_it_through() {
        assert_eq!(restart_gate([Some(TURN_GAP), Some(TURN_GAP * 90)], SEC, false), RestartGate::Now);
        // A turn just ended: its follow-up gets the gap first.
        assert_eq!(restart_gate([Some(TURN_GAP - SEC)], SEC, false), RestartGate::Wait);
        assert_eq!(restart_gate([Some(TURN_GAP), None], SEC, false), RestartGate::Wait);
    }

    /// No 2h idle grace any more: a chat parked between turns waits the
    /// gap like a team agent does.
    #[test]
    fn an_idle_chat_never_holds_it_past_the_gap() {
        assert_eq!(restart_gate([Some(TURN_GAP)], Duration::ZERO, false), RestartGate::Now);
    }

    #[test]
    fn a_turn_holds_it_at_most_the_cap() {
        assert!(MAX_WAIT <= Duration::from_secs(10 * 60));
        assert_eq!(restart_gate([None], MAX_WAIT - SEC, false), RestartGate::Wait);
        assert_eq!(restart_gate([None, None], MAX_WAIT, false), RestartGate::Now);
    }

    #[test]
    fn a_refused_build_restarts_at_once_whatever_runs() {
        assert_eq!(restart_gate([None], Duration::ZERO, true), RestartGate::Now);
    }
}
