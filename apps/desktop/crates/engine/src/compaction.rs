//! EXP-936: the host's half of `exponential_sessions_compact` — the run asks
//! to compact its own context, the HOST decides and executes.
//!
//! The web server only checks ownership and the agent, then relays a
//! `compact_request` to the run's publisher and awaits the verdict; this
//! policy is what answers it. It lives on the device because the device holds
//! every fact the verdict needs: the latest `usage` slot (the context meter),
//! the compaction fold (`compaction started`/`ended`) and the turn edges.
//!
//! The guardrails, so an agent cannot compact itself into a loop:
//!
//! - `too_early` below [`COMPACT_MIN_CONTEXT_FRACTION`] of the context window
//!   (or before any measurement at all — a run with no meter has nothing to
//!   show for its claim);
//! - `cooldown` while a compaction is open or one is already pending, and
//!   after a compaction until EITHER [`COMPACT_COOLDOWN_TURNS`] turns ended or
//!   [`COMPACT_COOLDOWN`] elapsed — either lifts it, because an unattended run
//!   works in ONE long turn and a turn count alone would lock it out for good,
//!   while a chatty run turns over fast and a clock alone would not slow it.
//!
//! An accepted ask is TWO steps, both taken on an idle edge by the command
//! loop ([`CompactionPolicy::take_step`]): the agent's own `/compact <keep>`
//! once the running turn ends (the ask arrives mid-turn — it IS a tool call
//! of that turn), then [`COMPACT_CONTINUE_PROMPT`] once the compaction turn
//! ends, because `/compact` leaves every agent idle and an unattended run
//! that asked to compact must not stop there. A compaction the agent ran on
//! its own (auto-compaction) while the first step waited fulfils the ask and
//! the step is dropped; a Stop drops whatever is pending.

use std::time::{Duration, Instant};

use steer::frames::CompactionPhase;
use steer::{ActivityEvent, CompactRefusal, CompactVerdict, TurnState};

/// Below this share of the context window an ask is `too_early`.
pub const COMPACT_MIN_CONTEXT_FRACTION: f64 = 0.5;
/// After a compaction, `cooldown` until this many turns ended …
pub const COMPACT_COOLDOWN_TURNS: u32 = 20;
/// … or this long passed, whichever comes first.
pub const COMPACT_COOLDOWN: Duration = Duration::from_secs(10 * 60);

/// The prompt that follows the compaction turn, so the run picks its work
/// back up instead of idling on the summary. Reads as the user's row on
/// every client's feed, after the "Context compacted" strip.
pub const COMPACT_CONTINUE_PROMPT: &str = "Your context was compacted at your own request (exponential_sessions_compact). Continue the task where you left off, working from the summary above.";

/// What the command loop owes next for an accepted ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactionStep {
    /// Send the agent's `/compact <keep>` (the text is [`compact_command`]).
    Compact { keep: Option<String> },
    /// The compaction turn ended: send [`COMPACT_CONTINUE_PROMPT`].
    Continue,
}

/// The `/compact` line the agent gets. `keep` rides verbatim as the
/// command's argument — claude reads it as the summary's instructions, codex
/// ignores it (its `thread/compact/start` takes none).
pub fn compact_command(keep: Option<&str>) -> String {
    match keep.map(str::trim).filter(|keep| !keep.is_empty()) {
        Some(keep) => format!("/compact {keep}"),
        None => "/compact".to_string(),
    }
}

#[derive(Debug)]
struct LastCompaction {
    ended_at: Instant,
    turns_since: u32,
}

/// The per-session compaction state the verdict reads and the two steps
/// advance. Fed by [`CompactionPolicy::observe`] with every wire event the
/// session dispatches; every method is cheap (the publisher's hook runs it
/// on the socket loop).
#[derive(Debug, Default)]
pub struct CompactionPolicy {
    context_used: i64,
    context_size: i64,
    compacting: bool,
    last_compaction: Option<LastCompaction>,
    pending: Option<CompactionStep>,
}

impl CompactionPolicy {
    /// Fold one dispatched wire event into the state. Only the `usage` slot,
    /// the compaction fold and the turn edges matter; everything else is a
    /// no-op.
    pub fn observe(&mut self, event: &ActivityEvent) {
        self.observe_at(event, Instant::now());
    }

    pub fn observe_at(&mut self, event: &ActivityEvent, now: Instant) {
        match event {
            ActivityEvent::Usage {
                context_used,
                context_size,
                ..
            } => {
                self.context_used = *context_used;
                self.context_size = *context_size;
            }
            ActivityEvent::Compaction { phase, .. } => match phase {
                CompactionPhase::Started => self.compacting = true,
                CompactionPhase::Ended => {
                    self.compacting = false;
                    self.last_compaction = Some(LastCompaction {
                        ended_at: now,
                        turns_since: 0,
                    });
                    // The agent compacted on its own while the ask waited for
                    // the turn boundary: the ask is fulfilled, not owed twice.
                    if matches!(self.pending, Some(CompactionStep::Compact { .. })) {
                        self.pending = None;
                    }
                }
            },
            ActivityEvent::Turn {
                state: TurnState::Ended,
                ..
            } => {
                if let Some(last) = &mut self.last_compaction {
                    last.turns_since = last.turns_since.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    /// The verdict for an ask made now, WITHOUT committing to it.
    pub fn decide(&self) -> Result<(), CompactRefusal> {
        self.decide_at(Instant::now())
    }

    pub fn decide_at(&self, now: Instant) -> Result<(), CompactRefusal> {
        if self.compacting || self.pending.is_some() {
            return Err(CompactRefusal::Cooldown);
        }
        if let Some(last) = &self.last_compaction {
            let turns_fresh = last.turns_since < COMPACT_COOLDOWN_TURNS;
            let clock_fresh = now.saturating_duration_since(last.ended_at) < COMPACT_COOLDOWN;
            if turns_fresh && clock_fresh {
                return Err(CompactRefusal::Cooldown);
            }
        }
        if self.context_size <= 0
            || (self.context_used as f64) < (self.context_size as f64) * COMPACT_MIN_CONTEXT_FRACTION
        {
            return Err(CompactRefusal::TooEarly);
        }
        Ok(())
    }

    /// The publisher hook's entry: decide, and on acceptance owe the
    /// `/compact` step. The caller then nudges the command loop
    /// ([`CompactionPolicy::has_pending`] is what the loop reads).
    pub fn request(&mut self, keep: Option<String>) -> CompactVerdict {
        self.request_at(keep, Instant::now())
    }

    pub fn request_at(&mut self, keep: Option<String>, now: Instant) -> CompactVerdict {
        match self.decide_at(now) {
            Ok(()) => {
                self.pending = Some(CompactionStep::Compact { keep });
                CompactVerdict::Accepted
            }
            Err(reason) => CompactVerdict::Refused(reason),
        }
    }

    /// Is a step owed? The dispatch path reads this on every idle edge.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The command loop, idle: take the step owed now and advance. The
    /// `/compact` step leaves [`CompactionStep::Continue`] behind; the
    /// continuation clears the slate.
    pub fn take_step(&mut self) -> Option<CompactionStep> {
        let step = self.pending.take()?;
        if matches!(step, CompactionStep::Compact { .. }) {
            self.pending = Some(CompactionStep::Continue);
        }
        Some(step)
    }

    /// A Stop: whatever was owed is dropped — the person said stop, and a
    /// `/compact` after that would read as the run ignoring them.
    pub fn cancel(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(used: i64, size: i64) -> ActivityEvent {
        ActivityEvent::Usage {
            context_used: used,
            context_size: size,
            cost_usd: None,
            at: None,
        }
    }

    fn compaction(phase: CompactionPhase) -> ActivityEvent {
        ActivityEvent::compaction(phase, None)
    }

    fn turn_ended() -> ActivityEvent {
        ActivityEvent::turn_at(TurnState::Ended, None, None)
    }

    #[test]
    fn the_command_carries_keep_verbatim_and_drops_a_blank_one() {
        assert_eq!(compact_command(None), "/compact");
        assert_eq!(compact_command(Some("  ")), "/compact");
        assert_eq!(
            compact_command(Some("open threads, file paths")),
            "/compact open threads, file paths"
        );
    }

    #[test]
    fn too_early_below_half_the_window_or_before_any_measurement() {
        let mut policy = CompactionPolicy::default();
        assert_eq!(policy.decide(), Err(CompactRefusal::TooEarly));
        policy.observe(&usage(99_999, 200_000));
        assert_eq!(policy.decide(), Err(CompactRefusal::TooEarly));
        policy.observe(&usage(100_000, 200_000));
        assert_eq!(policy.decide(), Ok(()));
        // A cleared meter (zero size) is "unknown" again.
        policy.observe(&usage(0, 0));
        assert_eq!(policy.decide(), Err(CompactRefusal::TooEarly));
    }

    #[test]
    fn an_accepted_ask_owes_compact_then_continue_and_blocks_a_second_ask() {
        let mut policy = CompactionPolicy::default();
        policy.observe(&usage(150_000, 200_000));
        assert_eq!(
            policy.request(Some("open threads".to_string())),
            CompactVerdict::Accepted
        );
        assert!(policy.has_pending());
        // While the first waits for the turn boundary, a second is cooldown.
        assert_eq!(
            policy.request(None),
            CompactVerdict::Refused(CompactRefusal::Cooldown)
        );
        assert_eq!(
            policy.take_step(),
            Some(CompactionStep::Compact {
                keep: Some("open threads".to_string())
            })
        );
        // The compaction turn: still pending (the continuation), still
        // cooldown for anyone asking.
        assert!(policy.has_pending());
        policy.observe(&compaction(CompactionPhase::Started));
        assert_eq!(policy.decide(), Err(CompactRefusal::Cooldown));
        policy.observe(&compaction(CompactionPhase::Ended));
        policy.observe(&turn_ended());
        assert_eq!(policy.take_step(), Some(CompactionStep::Continue));
        assert!(!policy.has_pending());
        assert_eq!(policy.take_step(), None);
    }

    #[test]
    fn cooldown_lifts_after_twenty_turns_or_ten_minutes_whichever_first() {
        let t0 = Instant::now();
        let mut policy = CompactionPolicy::default();
        policy.observe_at(&usage(150_000, 200_000), t0);
        policy.observe_at(&compaction(CompactionPhase::Started), t0);
        policy.observe_at(&compaction(CompactionPhase::Ended), t0);
        // Right after a compaction: cooldown, even with the meter still high
        // (a compaction that freed little must not be re-run at once).
        assert_eq!(policy.decide_at(t0), Err(CompactRefusal::Cooldown));
        // 19 turns later, 1 minute in: still cooling.
        for _ in 0..19 {
            policy.observe_at(&turn_ended(), t0);
        }
        assert_eq!(
            policy.decide_at(t0 + Duration::from_secs(60)),
            Err(CompactRefusal::Cooldown)
        );
        // The 20th turn lifts it.
        policy.observe_at(&turn_ended(), t0);
        assert_eq!(policy.decide_at(t0 + Duration::from_secs(60)), Ok(()));

        // Or the clock alone: one long unattended turn, ten minutes on.
        let mut policy = CompactionPolicy::default();
        policy.observe_at(&usage(150_000, 200_000), t0);
        policy.observe_at(&compaction(CompactionPhase::Started), t0);
        policy.observe_at(&compaction(CompactionPhase::Ended), t0);
        assert_eq!(
            policy.decide_at(t0 + COMPACT_COOLDOWN - Duration::from_secs(1)),
            Err(CompactRefusal::Cooldown)
        );
        assert_eq!(policy.decide_at(t0 + COMPACT_COOLDOWN), Ok(()));
        // The meter still gates once the cooldown is over.
        policy.observe_at(&usage(20_000, 200_000), t0);
        assert_eq!(
            policy.decide_at(t0 + COMPACT_COOLDOWN),
            Err(CompactRefusal::TooEarly)
        );
    }

    #[test]
    fn an_auto_compaction_fulfils_a_waiting_ask_and_a_stop_drops_it() {
        let mut policy = CompactionPolicy::default();
        policy.observe(&usage(190_000, 200_000));
        assert_eq!(policy.request(None), CompactVerdict::Accepted);
        // Claude auto-compacted before the turn ended: nothing left to send.
        policy.observe(&compaction(CompactionPhase::Started));
        policy.observe(&compaction(CompactionPhase::Ended));
        assert!(!policy.has_pending());
        assert_eq!(policy.take_step(), None);

        // But OUR compaction's end keeps the continuation owed.
        let mut policy = CompactionPolicy::default();
        policy.observe(&usage(190_000, 200_000));
        assert_eq!(policy.request(None), CompactVerdict::Accepted);
        assert!(matches!(
            policy.take_step(),
            Some(CompactionStep::Compact { keep: None })
        ));
        policy.observe(&compaction(CompactionPhase::Started));
        policy.observe(&compaction(CompactionPhase::Ended));
        assert_eq!(policy.take_step(), Some(CompactionStep::Continue));

        // A Stop drops whatever is owed.
        let mut policy = CompactionPolicy::default();
        policy.observe(&usage(190_000, 200_000));
        assert_eq!(policy.request(None), CompactVerdict::Accepted);
        policy.cancel();
        assert!(!policy.has_pending());
        assert_eq!(policy.take_step(), None);
    }
}
