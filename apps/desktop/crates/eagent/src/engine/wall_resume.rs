//! EXP-2099 — a turn that ended on the agent's usage wall carries on by
//! itself once the wall's reset has passed.
//!
//! The input that hit the wall IS in the agent's transcript (the CLI records
//! the user message, then answers it with its synthetic notice), so the host
//! counts it taken in and nothing delivers it again. What was missing is the
//! next turn: only account rotation made one, when another login had room.
//!
//! The rule, fed by the stream every viewer gets ([`WallResume::observe`])
//! and evaluated on the lifecycle's one-second tick ([`WallResume::tick`]):
//!
//! 1. A turn that ENDS while the agent's limit NOTICE stands (the agent said
//!    nothing after it) arms a continuation at the wall's reset, when the
//!    notice names one that is still ahead. A bare `rejected` window report
//!    without the notice never arms: the run may be working on paid overage.
//! 2. ANY turn's start takes the armed continuation: the person's newer
//!    message, a rotation's switch prompt, a restart's continuation. If that
//!    turn meets the wall as well, its end arms again.
//! 3. A person's Stop drops it.
//! 4. Once the reset (plus the clock-skew grace every wall gets) has passed
//!    and the run is idle with nothing open on a person, the engine sends
//!    [`WALL_RESUME_PROMPT`], once.
//!
//! The armed reset is the host's to keep ([`RunBackend::wall_resume`]): a run
//! with no process when the reset comes is continued from there.
//!
//! [`RunBackend::wall_resume`]: super::host::RunBackend::wall_resume

use crate::wire::frames::{rate_limit_clears, rate_limit_expired, rate_limit_is_wall, TurnState};
use crate::wire::ActivityEvent as E;

#[derive(Debug, Default)]
pub struct WallResume {
    in_turn: bool,
    /// The limit notice stands; its reset (unix ms), when it named one.
    notice: Option<Option<i64>>,
    /// The reset (unix ms) a continuation waits for.
    armed: Option<i64>,
    /// The armed continuation was sent; its turn has not started yet.
    sent: bool,
    /// A Stop was pressed since the last turn started.
    stopped: bool,
}

impl WallResume {
    /// The reset (unix ms) a continuation waits for, for the host to keep.
    pub fn armed(&self) -> Option<i64> {
        self.armed
    }

    /// The reset an earlier process of this run was waiting for. `seeded` =
    /// this run opens with a turn of its own, which takes it.
    pub fn load(&mut self, resets_at: Option<i64>, seeded: bool) {
        self.armed = resets_at;
        self.sent = seeded;
    }

    /// Fold in one wire event of a LIVE run (never a history replay).
    /// `now_ms` = the wall clock.
    pub fn observe(&mut self, event: &E, now_ms: i64) {
        match event {
            E::Turn { state: TurnState::Started, .. } => {
                self.in_turn = true;
                self.armed = None;
                self.sent = false;
                self.stopped = false;
            }
            E::Turn { state: TurnState::Ended, .. } => {
                if std::mem::take(&mut self.in_turn) {
                    self.arm(now_ms);
                }
            }
            E::RateLimit { status, resets_at, message, .. } => {
                if rate_limit_clears(status) || !rate_limit_is_wall(status, message.as_deref()) {
                    self.notice = None;
                } else if message.as_deref().is_some_and(|text| !text.trim().is_empty()) {
                    self.notice = Some(*resets_at);
                } else if let Some(reset) = &mut self.notice {
                    // A window report under a standing notice: its reset only.
                    *reset = resets_at.or(*reset);
                }
            }
            _ => {}
        }
    }

    fn arm(&mut self, now_ms: i64) {
        if self.stopped {
            return;
        }
        // A reset already behind us is a stale stamp: sending on it would
        // loop on a wall that is still up.
        self.armed = self.notice.flatten().filter(|reset| *reset > now_ms);
        self.sent = false;
    }

    /// A person pressed Stop: what waited is theirs to restart.
    pub fn stop(&mut self) {
        self.armed = None;
        self.sent = false;
        self.stopped = true;
    }

    /// True = send [`WALL_RESUME_PROMPT`] now. The armed reset stays until
    /// the turn starts, so a prompt that never became one is not lost.
    pub fn tick(&mut self, now_ms: i64, ready: bool) -> bool {
        if !ready || self.in_turn || self.sent || !rate_limit_expired(self.armed, now_ms) {
            return false;
        }
        self.sent = true;
        true
    }
}

/// The turn the engine (or the pickup, for a run with no process) sends. Says
/// who is speaking; true of an input the notice answered whole and of a turn
/// cut half way.
pub const WALL_RESUME_PROMPT: &str = "[Exponential: your usage limit stopped the last turn and has \
     reset now. This is automatic, not a person.] Your last input may not have been answered: carry \
     it out, or continue where you left off. If subagents were cut off, check what they left on disk \
     and resume them rather than starting over.";

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;
    const RESET: i64 = NOW + 3_600_000;
    /// Past the reset and its grace.
    const AFTER: i64 = RESET + 60_001;

    fn turn(state: TurnState) -> E {
        E::Turn { state, started_at: None, tokens: None, writing: None, at: None }
    }

    fn notice(reset: Option<i64>) -> E {
        E::rate_limit("rejected", reset, Some("You've hit your limit".to_string()))
    }

    fn run(events: &[E]) -> WallResume {
        let mut wall = WallResume::default();
        for event in events {
            wall.observe(event, NOW);
        }
        wall
    }

    fn walled() -> WallResume {
        run(&[turn(TurnState::Started), notice(Some(RESET)), turn(TurnState::Ended)])
    }

    #[test]
    fn a_turn_the_wall_ended_continues_once_after_the_reset() {
        let mut wall = walled();
        assert_eq!(wall.armed(), Some(RESET));
        assert!(!wall.tick(RESET, true), "the skew grace");
        assert!(!wall.tick(AFTER, false), "busy, or a card is open");
        assert!(wall.tick(AFTER, true));
        assert!(!wall.tick(AFTER + 1, true), "once");
        assert_eq!(wall.armed(), Some(RESET), "kept until its turn starts");
        wall.observe(&turn(TurnState::Started), AFTER);
        wall.observe(&E::rate_limit("", None, None), AFTER);
        wall.observe(&turn(TurnState::Ended), AFTER);
        assert_eq!(wall.armed(), None);
        assert!(!wall.tick(AFTER * 2, true));
    }

    #[test]
    fn a_newer_turn_takes_it_and_one_that_walls_again_arms_again() {
        let mut wall = walled();
        wall.observe(&turn(TurnState::Started), NOW);
        assert_eq!(wall.armed(), None, "the newer turn is the continuation");
        // The same notice, deduped by the adapter: nothing lifted it.
        wall.observe(&turn(TurnState::Ended), NOW);
        assert_eq!(wall.armed(), Some(RESET));
        // Answered this time (a rotation's switch prompt, a person's message).
        wall.observe(&turn(TurnState::Started), NOW);
        wall.observe(&E::rate_limit("", None, None), NOW);
        wall.observe(&turn(TurnState::Ended), NOW);
        assert!(!wall.tick(AFTER, true));
    }

    #[test]
    fn a_stop_drops_it() {
        let mut idle = walled();
        idle.stop();
        assert!(!idle.tick(AFTER, true));
        // Mid-turn: the cancelled turn's end does not arm it back.
        let mut mid = run(&[turn(TurnState::Started), notice(Some(RESET))]);
        mid.stop();
        mid.observe(&turn(TurnState::Ended), NOW);
        assert_eq!(mid.armed(), None);
    }

    #[test]
    fn only_a_standing_notice_with_a_reset_ahead_arms() {
        let ended = |event: E| run(&[turn(TurnState::Started), event, turn(TurnState::Ended)]).armed();
        assert_eq!(ended(E::rate_limit("rejected", Some(RESET), None)), None, "paid overage answers on");
        assert_eq!(ended(notice(None)), None, "no reset to wait for");
        assert_eq!(ended(notice(Some(NOW - 1))), None, "a stale stamp");
        let lifted = run(&[
            turn(TurnState::Started),
            notice(Some(RESET)),
            E::rate_limit("", None, None),
            turn(TurnState::Ended),
        ]);
        assert_eq!(lifted.armed(), None, "the agent spoke after it");
    }

    #[test]
    fn a_reset_an_earlier_process_waited_for_still_fires() {
        let mut wall = WallResume::default();
        wall.load(Some(RESET), false);
        assert!(wall.tick(AFTER, true));
        assert!(!wall.tick(AFTER, true));
        // A run that opens with a seed turn (a rotation's switch prompt).
        let mut seeded = WallResume::default();
        seeded.load(Some(RESET), true);
        assert!(!seeded.tick(AFTER, true));
        seeded.observe(&turn(TurnState::Started), AFTER);
        assert_eq!(seeded.armed(), None);
    }
}
