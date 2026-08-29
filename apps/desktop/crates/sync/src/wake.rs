//! EXP-533: suspend detection — the pure model behind "the lid was closed for
//! four hours and the board is still showing yesterday".
//!
//! macOS (and Linux with `CLOCK_MONOTONIC`) PAUSES [`Instant`] while the
//! machine is suspended; [`SystemTime`] does not. Sampling both on a 1s tick
//! and diffing the two deltas therefore measures exactly the time the machine
//! spent asleep — the same asymmetry [`crate::health`] documents for its
//! staleness window, used here as a positive signal instead of a hazard.
//!
//! Why this matters for sync: every shape thread is parked in a blocking read
//! on ONE shared h2 connection (EXP-304). Suspend kills that connection
//! silently — the socket's peer is long gone, but the parked reads only find
//! out at their timeout, up to
//! [`LIVE_READ_TIMEOUT`](crate::client::LIVE_READ_TIMEOUT) (90s) after wake.
//! A watchdog that notices the jump restarts the pipeline immediately instead.

use std::time::{Duration, Instant, SystemTime};

/// How much wall time must pass with the monotonic clock standing still
/// before we call it a suspend. Comfortably above any scheduling hiccup or
/// NTP step a laptop hands us in normal operation, and well under the time a
/// user would notice stale data.
pub const WAKE_JUMP: Duration = Duration::from_secs(60);

/// Pairs the two clocks so [`WakeWatchdog::tick`] can diff their deltas.
/// Pure: the caller owns the timer thread and decides what a wake means.
#[derive(Clone, Copy, Debug)]
pub struct WakeWatchdog {
    wall: SystemTime,
    mono: Instant,
}

impl WakeWatchdog {
    pub fn new(wall: SystemTime, mono: Instant) -> Self {
        Self { wall, mono }
    }

    /// Fold in one sample. Returns `true` when the wall clock ran at least
    /// [`WAKE_JUMP`] further than the monotonic clock since the previous
    /// sample — i.e. the machine was asleep.
    ///
    /// The baseline ALWAYS advances, fired or not: two consecutive suspends
    /// must each fire on their own, and a backwards wall step (NTP) must not
    /// leave a stale baseline that fires on the next ordinary tick.
    pub fn tick(&mut self, wall: SystemTime, mono: Instant) -> bool {
        // Both deltas saturate to zero: a backwards wall step yields a zero
        // wall delta (never a fire), and `Instant` is monotonic by contract.
        let wall_delta = wall.duration_since(self.wall).unwrap_or(Duration::ZERO);
        let mono_delta = mono.saturating_duration_since(self.mono);
        self.wall = wall;
        self.mono = mono;
        wall_delta.saturating_sub(mono_delta) >= WAKE_JUMP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> (SystemTime, Instant) {
        (SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000), Instant::now())
    }

    #[test]
    fn a_four_hour_lid_close_fires() {
        let (wall, mono) = base();
        let mut watchdog = WakeWatchdog::new(wall, mono);
        // Wall advanced 4h; the monotonic clock slept through it (the 1s tick
        // thread was frozen with the machine, so ~0 monotonic elapsed).
        assert!(watchdog.tick(
            wall + Duration::from_secs(4 * 3600),
            mono + Duration::from_millis(20)
        ));
    }

    #[test]
    fn an_ordinary_tick_does_not_fire() {
        let (wall, mono) = base();
        let mut watchdog = WakeWatchdog::new(wall, mono);
        for i in 1..=10u64 {
            let step = Duration::from_secs(i);
            assert!(
                !watchdog.tick(wall + step, mono + step),
                "ordinary 1s tick #{i} must not look like a suspend"
            );
        }
        // Even a chunky 55s stall of the tick thread stays under the jump.
        assert!(!watchdog.tick(
            wall + Duration::from_secs(65),
            mono + Duration::from_secs(65)
        ));
    }

    #[test]
    fn a_backwards_ntp_step_neither_fires_nor_arms_the_next_tick() {
        let (wall, mono) = base();
        let mut watchdog = WakeWatchdog::new(wall, mono);
        // NTP yanks the wall clock two hours BACKWARDS.
        assert!(!watchdog.tick(
            wall - Duration::from_secs(2 * 3600),
            mono + Duration::from_secs(1)
        ));
        // The baseline moved with it, so the next ordinary tick is ordinary —
        // a stale baseline here would have shown a 2h forward "jump".
        assert!(!watchdog.tick(
            wall - Duration::from_secs(2 * 3600) + Duration::from_secs(1),
            mono + Duration::from_secs(2)
        ));
    }

    #[test]
    fn two_consecutive_suspends_both_fire() {
        let (wall, mono) = base();
        let mut watchdog = WakeWatchdog::new(wall, mono);
        assert!(watchdog.tick(wall + Duration::from_secs(3600), mono + Duration::from_secs(1)));
        // Awake for a while, then asleep again.
        assert!(!watchdog.tick(
            wall + Duration::from_secs(3610),
            mono + Duration::from_secs(11)
        ));
        assert!(watchdog.tick(
            wall + Duration::from_secs(7200),
            mono + Duration::from_secs(12)
        ));
    }
}
