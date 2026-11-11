//! #LOGS-3: a derivation that re-reads the whole store on every synced row
//! (the rail's rows, a long chat's transcript) costs a few ms on a small
//! account and tens to hundreds of ms on a large one on a slow machine;
//! a busy team syncs several rows a second, and the main thread spent whole
//! seconds deriving (mint, 0.14.147: hangs of 2-12 s, 50-140 an hour).
//!
//! [`DutyCycle`] bounds it: a run that took longer than [`CHEAP`] waits
//! [`IDLE_PER_BUSY`] times its cost before the next one, so the derivation
//! holds at most a tenth of the main thread whatever the sync rate. The
//! rows are the same; under load they arrive coalesced. A cheap run (a small
//! account, every test) never waits.

use std::time::{Duration, Instant};

/// A run this cheap never delays the next.
pub(crate) const CHEAP: Duration = Duration::from_millis(4);
/// Idle time owed per unit of an expensive run's cost (busy share 1/10).
pub(crate) const IDLE_PER_BUSY: u32 = 9;
/// The longest a deferred run waits (a person's own change shows within it).
pub(crate) const MAX_WAIT: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
pub(crate) struct DutyCycle {
    /// Before this instant a run must wait; `None` = run now.
    next_at: Option<Instant>,
    /// A wake-up is already scheduled.
    scheduled: bool,
}

impl DutyCycle {
    /// When the next run may start, given that the last one ended `now`
    /// after taking `cost`. Pure.
    pub(crate) fn next_after(now: Instant, cost: Duration) -> Option<Instant> {
        (cost > CHEAP).then(|| now + (cost * IDLE_PER_BUSY).min(MAX_WAIT))
    }

    /// How long a run asked for at `now` must wait (`None` = run now).
    pub(crate) fn wait_at(&self, now: Instant) -> Option<Duration> {
        self.next_at.and_then(|at| at.checked_duration_since(now)).filter(|wait| !wait.is_zero())
    }

    /// Whether a run may start now. When not, `notify` re-renders the view
    /// once the wait is over (one wake-up at a time).
    pub(crate) fn ready<V: 'static>(&mut self, cx: &mut gpui::Context<V>, pick: fn(&mut V) -> &mut DutyCycle) -> bool {
        let Some(wait) = self.wait_at(cx.background_executor().now()) else { return true };
        if !self.scheduled {
            self.scheduled = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |this, cx| {
                    pick(this).scheduled = false;
                    cx.notify();
                });
            })
            .detach();
        }
        false
    }

    /// Record a run that took `cost` (real time: the main thread's) and
    /// ended at `now` (the executor's clock: the app's, or a test's).
    pub(crate) fn ran(&mut self, cost: Duration, now: Instant) {
        self.next_at = next_run(cost, now);
    }
}

/// [`DutyCycle::next_after`] for a run of `cost` that ended at `now` (the
/// executor's clock), when the cycle is on (`None` in unit tests unless
/// enabled).
pub(crate) fn next_run(cost: Duration, now: Instant) -> Option<Instant> {
    if !enabled() {
        return None;
    }
    #[cfg(test)]
    let cost = cost.max(COST_FLOOR.with(std::cell::Cell::get));
    DutyCycle::next_after(now, cost)
}

/// Unit tests run debug builds against a fake clock: a test turns the
/// cycle on ([`enable_for_test`]) where it means to drive it.
#[cfg(test)]
thread_local! {
    static ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
thread_local! {
    /// A test's runs cost at least this (the fake clock stands still).
    static COST_FLOOR: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) };
}

#[cfg(test)]
pub(crate) fn enable_for_test(on: bool) {
    ENABLED.with(|enabled| enabled.set(on));
}

#[cfg(test)]
pub(crate) fn cost_floor_for_test(floor: Duration) {
    COST_FLOOR.with(|cost| cost.set(floor));
}

fn enabled() -> bool {
    #[cfg(test)]
    return ENABLED.with(std::cell::Cell::get);
    #[cfg(not(test))]
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cheap_run_never_waits_an_expensive_one_owes_nine_times_its_cost() {
        let now = Instant::now();
        assert_eq!(DutyCycle::next_after(now, Duration::from_millis(3)), None);
        assert_eq!(DutyCycle::next_after(now, Duration::from_millis(20)), Some(now + Duration::from_millis(180)));
        assert_eq!(DutyCycle::next_after(now, Duration::from_secs(1)), Some(now + MAX_WAIT));
        let cycle = DutyCycle { next_at: Some(now + Duration::from_millis(180)), scheduled: false };
        assert_eq!(cycle.wait_at(now), Some(Duration::from_millis(180)));
        assert_eq!(cycle.wait_at(now + Duration::from_millis(200)), None);
        assert_eq!(DutyCycle::default().wait_at(now), None);
    }
}
