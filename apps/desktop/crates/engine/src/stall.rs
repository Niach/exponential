//! FEED-25 — the stall watchdog: no live turn may wedge forever.
//!
//! The reported run (claude, ACP) issued seven parallel MCP `tools/call`s and
//! never saw a single result: the CLI's HTTP client lost the responses, its
//! own per-call timeout defaults to 27 hours, and nothing above it had a
//! clock — so the turn never ended, every later steer message sat in the
//! CLI's queue, and the row stayed `running` (`needs_input` true, zero CPU)
//! for hours until a human noticed. `coding::launcher` now bounds claude's
//! MCP calls, but that closes ONE hole in ONE agent; this is the generic
//! backstop the engine owns for every adapter.
//!
//! The rule, evaluated on the lifecycle's one-second tick:
//!
//! 1. A LIVE session with a turn in flight (`TurnSignal` not idle) that is
//!    not waiting on a person (`needs_input` false) and has produced no event
//!    for [`STALL_AFTER`] is stalled → `session/cancel` — as
//!    `EngineCommand::Interrupt`, NOT the user's Stop (EXP-784): the
//!    notification carries `CANCEL_QUEUED_META_KEY: false`, which claude maps
//!    onto `interrupt.cancel_queued = false`, so only the wedged turn is
//!    aborted. A CLI whose promise is merely lost unwinds on the abort,
//!    answers the prompt with `Cancelled`, and the user messages that queued
//!    up behind it FLOW — a Stop (`cancel_queued = true`) would have thrown
//!    those away, which is the opposite of rescuing the run.
//! 2. A stalled turn that stays silent for another [`STALL_KILL_GRACE`] after
//!    the interrupt is dead → the run ends with a
//!    [`crate::local::EnginePhase::Failed`] banner that says why, so the
//!    session tab / CLI show the reason instead of an empty "ended", and the
//!    user resumes it (`resumed_from_id`) rather than hunting a zombie.
//!
//! Silence is measured on the mapper's output (see `SessionCtx::dispatch`)
//! and on the bytes an agent's `terminal/*` command writes (which never reach
//! the mapper at all, so a long PTY command would otherwise read as a wedge):
//! the worktree `diff` ticker does not count (a wedged run has no new diff,
//! and a busy one has events anyway), nor does the interrupt's own
//! bookkeeping — nor the watchdog's OWN notice, or the interrupt it announces
//! could never escalate. Only what the AGENT says or does (or a turn-end
//! edge) resets the clock. `needs_input` gates it because a permission or
//! question card can legitimately sit for a day. The thresholds are deliberately generous:
//! a foreground Bash call caps at 10 minutes, a Task subagent streams its
//! tool events, API retry storms settle within minutes, compaction has its
//! own 5-minute bound — twenty silent minutes mid-turn is not any of those.

use std::time::{Duration, Instant};

/// How long a live turn may go without any event before it is interrupted.
pub const STALL_AFTER: Duration = Duration::from_secs(20 * 60);

/// How long after the interrupt the turn gets to settle before the run ends.
pub const STALL_KILL_GRACE: Duration = Duration::from_secs(5 * 60);

/// What the lifecycle should do after one tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StallAction {
    None,
    /// Send `session/cancel`: the turn has been silent for [`STALL_AFTER`].
    Interrupt,
    /// End the run: the interrupt changed nothing for [`STALL_KILL_GRACE`].
    End,
}

/// One tick's view of the session.
#[derive(Clone, Copy, Debug)]
pub struct StallInput {
    pub now: Instant,
    /// `EnginePhase::Live` — connecting and ended sessions are never stalled.
    pub live: bool,
    /// `TurnSignal::is_idle()`: between turns nothing is expected.
    pub idle: bool,
    /// The EXP-214 flag: a card is waiting on a person.
    pub needs_input: bool,
    /// When the agent last said anything (`SessionCtx::last_activity`).
    pub last_activity: Instant,
}

#[derive(Debug, Default)]
pub struct StallWatchdog {
    interrupted_at: Option<Instant>,
    ended: bool,
}

impl StallWatchdog {
    pub fn new() -> Self {
        Self::default()
    }

    /// The action for this tick. At most one `Interrupt` per silent stretch,
    /// and `End` exactly once for the life of the watchdog.
    pub fn tick(&mut self, input: StallInput) -> StallAction {
        if self.ended {
            return StallAction::None;
        }
        if !input.live || input.idle || input.needs_input {
            self.interrupted_at = None;
            return StallAction::None;
        }
        let silent = input.now.saturating_duration_since(input.last_activity);
        match self.interrupted_at {
            None if silent >= STALL_AFTER => {
                self.interrupted_at = Some(input.now);
                StallAction::Interrupt
            }
            None => StallAction::None,
            Some(interrupted_at) => {
                if input.last_activity > interrupted_at {
                    // The interrupt got a reaction: the turn is settling (or
                    // the agent woke up). Start over from the new activity.
                    self.interrupted_at = None;
                    return StallAction::None;
                }
                if input.now.saturating_duration_since(interrupted_at) >= STALL_KILL_GRACE {
                    self.ended = true;
                    StallAction::End
                } else {
                    StallAction::None
                }
            }
        }
    }

    /// The transcript notice for a turn this watchdog interrupted. Without it
    /// the cancel is a `log::warn!` nobody reads and the feed just stops
    /// mid-turn — an unattended run loses that work with no trace at all.
    pub fn interrupt_notice(silent_for: Duration) -> String {
        let minutes = silent_for.as_secs() / 60;
        format!(
            "Interrupted after {minutes} minutes without progress: the turn produced nothing, so \
             the engine cancelled it. Send a message to pick it back up."
        )
    }

    /// The banner text for a run this watchdog ended.
    pub fn end_reason(silent_for: Duration) -> String {
        let minutes = silent_for.as_secs() / 60;
        format!(
            "The agent stopped responding: no activity for {minutes} minutes and the interrupt \
             changed nothing, so the run was ended. Resume it to continue where it left off."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(now: Instant, last_activity: Instant) -> StallInput {
        StallInput {
            now,
            live: true,
            idle: false,
            needs_input: false,
            last_activity,
        }
    }

    #[test]
    fn a_silent_live_turn_is_interrupted_once_then_ended() {
        let start = Instant::now();
        let mut dog = StallWatchdog::new();
        assert_eq!(dog.tick(input(start + STALL_AFTER / 2, start)), StallAction::None);
        assert_eq!(dog.tick(input(start + STALL_AFTER, start)), StallAction::Interrupt);
        // Still silent, inside the grace: nothing more is sent.
        let after = start + STALL_AFTER + Duration::from_secs(1);
        assert_eq!(dog.tick(input(after, start)), StallAction::None);
        assert_eq!(
            dog.tick(input(start + STALL_AFTER + STALL_KILL_GRACE - Duration::from_secs(1), start)),
            StallAction::None
        );
        assert_eq!(
            dog.tick(input(start + STALL_AFTER + STALL_KILL_GRACE, start)),
            StallAction::End
        );
        // Ended is terminal.
        assert_eq!(
            dog.tick(input(start + STALL_AFTER + STALL_KILL_GRACE + Duration::from_secs(60), start)),
            StallAction::None
        );
    }

    #[test]
    fn activity_after_the_interrupt_starts_over() {
        let start = Instant::now();
        let mut dog = StallWatchdog::new();
        let interrupted = start + STALL_AFTER;
        assert_eq!(dog.tick(input(interrupted, start)), StallAction::Interrupt);
        // The agent answered the cancel (a tool error, a stop): the clock
        // restarts from that event, and a new silence needs a new full stretch.
        let woke = interrupted + Duration::from_secs(5);
        assert_eq!(dog.tick(input(woke + Duration::from_secs(1), woke)), StallAction::None);
        assert_eq!(
            dog.tick(input(woke + STALL_KILL_GRACE + Duration::from_secs(1), woke)),
            StallAction::None,
            "the kill grace never outlives the interrupt it belonged to"
        );
        assert_eq!(dog.tick(input(woke + STALL_AFTER, woke)), StallAction::Interrupt);
    }

    #[test]
    fn idle_connecting_and_waiting_sessions_are_never_stalled() {
        let start = Instant::now();
        let long_ago = start + 10 * STALL_AFTER;
        let mut dog = StallWatchdog::new();
        let idle = StallInput {
            idle: true,
            ..input(long_ago, start)
        };
        assert_eq!(dog.tick(idle), StallAction::None);
        let connecting = StallInput {
            live: false,
            ..input(long_ago, start)
        };
        assert_eq!(dog.tick(connecting), StallAction::None);
        let waiting = StallInput {
            needs_input: true,
            ..input(long_ago, start)
        };
        assert_eq!(dog.tick(waiting), StallAction::None);
    }

    #[test]
    fn a_pending_card_or_turn_end_forgets_a_pending_interrupt() {
        let start = Instant::now();
        let mut dog = StallWatchdog::new();
        assert_eq!(dog.tick(input(start + STALL_AFTER, start)), StallAction::Interrupt);
        // The cancel dismissed the turn: idle, no events needed. Deep into
        // what would have been the grace, nothing ends.
        let idle = StallInput {
            idle: true,
            ..input(start + STALL_AFTER + STALL_KILL_GRACE, start)
        };
        assert_eq!(dog.tick(idle), StallAction::None);
        // The next turn starts silent again: a fresh stretch, not the old
        // grace.
        let next = start + STALL_AFTER + STALL_KILL_GRACE + Duration::from_secs(1);
        assert_eq!(dog.tick(input(next, next)), StallAction::None);
        assert_eq!(dog.tick(input(next + STALL_AFTER, next)), StallAction::Interrupt);
    }

    /// A `terminal/*` command's output is activity like anything else the
    /// mapper emits (`SessionCtx::touch_activity_throttled`): a `bun test`
    /// behind a raised `BASH_MAX_TIMEOUT_MS` streams for half an hour without
    /// one ACP notification, and it must not be interrupted for it.
    #[test]
    fn streaming_terminal_output_keeps_the_turn_alive() {
        let start = Instant::now();
        let mut dog = StallWatchdog::new();
        // One chunk every five minutes for an hour: the clock never gets
        // STALL_AFTER of silence, so nothing is ever sent.
        let step = Duration::from_secs(5 * 60);
        let mut last_activity = start;
        for tick in 1..=12u32 {
            let now = start + step * tick;
            assert_eq!(dog.tick(input(now, last_activity)), StallAction::None);
            last_activity = now;
        }
        // The command finishes and the agent goes quiet for real.
        assert_eq!(
            dog.tick(input(last_activity + STALL_AFTER, last_activity)),
            StallAction::Interrupt
        );
    }

    #[test]
    fn the_reason_names_the_silence() {
        let reason = StallWatchdog::end_reason(Duration::from_secs(25 * 60 + 30));
        assert!(reason.starts_with("The agent stopped responding: no activity for 25 minutes"));
        assert!(reason.contains("Resume it"));
    }

    #[test]
    fn the_interrupt_notice_names_the_silence() {
        let notice = StallWatchdog::interrupt_notice(STALL_AFTER + Duration::from_secs(42));
        assert!(notice.starts_with("Interrupted after 20 minutes without progress"));
    }
}
