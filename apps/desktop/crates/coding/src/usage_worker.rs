//! #FEED-211 — the agent status collection, OFF every beat.
//!
//! A collection pass ([`crate::agent_usage::collect_if_due`]) talks to the
//! network, the credential store and the agent CLIs. It used to run INSIDE
//! the desktop's heartbeat body (and on the daemon's one serialized device
//! worker), so one slow pass — a refresh lock, a timeout, a CLI probe —
//! held the heartbeat (or every queued command) with it: healthy runs read
//! `device_offline` while the machine worked.
//!
//! The [`UsageWorker`] owns the pass instead: [`UsageWorker::start`] runs it
//! on its own thread (at most one at a time), and the beat only ever reads
//! [`UsageWorker::latest`] — the last payload, with `stale` set on its
//! numbers once no pass has landed for [`STALE_AFTER`], so a stuck pass dims
//! the bars instead of freezing them as current.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::agent_usage::AgentStatusPayload;

/// How old the last landed pass may be before its numbers ship `stale`.
/// Passes start on every beat (30 s) and a slow one is bounded by its own
/// timeouts, so a payload this old means the collector is stuck.
pub const STALE_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct State {
    latest: Option<(AgentStatusPayload, Instant)>,
    running: bool,
}

/// The one collection slot of a process: the last payload plus whether a
/// pass is in flight. Cheap to clone (shared state).
#[derive(Clone, Default)]
pub struct UsageWorker {
    state: Arc<Mutex<State>>,
}

impl UsageWorker {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The last collected payload; its numbers marked `stale` once it is
    /// older than [`STALE_AFTER`]. `None` before the first pass lands.
    pub fn latest(&self) -> Option<AgentStatusPayload> {
        self.latest_at(Instant::now())
    }

    fn latest_at(&self, now: Instant) -> Option<AgentStatusPayload> {
        let state = self.lock();
        let (payload, at) = state.latest.as_ref()?;
        let mut payload = payload.clone();
        if now.saturating_duration_since(*at) >= STALE_AFTER {
            mark_stale(&mut payload);
        }
        Some(payload)
    }

    /// Whether a pass is in flight right now.
    pub fn running(&self) -> bool {
        self.lock().running
    }

    /// Store a landed payload (a pass's, or a forced refresh's). `true` when
    /// it differs from the one held — the identity (sans `checkedAt`) or
    /// the numbers — i.e. when a beat should carry it soon.
    pub fn publish(&self, payload: AgentStatusPayload) -> bool {
        self.publish_at(payload, Instant::now())
    }

    fn publish_at(&self, payload: AgentStatusPayload, at: Instant) -> bool {
        let mut state = self.lock();
        let changed = state
            .latest
            .as_ref()
            .is_none_or(|(previous, _)| status_changed(previous, &payload));
        state.latest = Some((payload, at));
        changed
    }

    /// Run `collect` on its own thread unless a pass is already in flight
    /// (`false` = one is; nothing started). The payload is published, then
    /// `done(changed)` runs on that thread — the caller's beat-now nudge.
    pub fn start(
        &self,
        collect: impl FnOnce() -> AgentStatusPayload + Send + 'static,
        done: impl FnOnce(bool) + Send + 'static,
    ) -> bool {
        {
            let mut state = self.lock();
            if state.running {
                return false;
            }
            state.running = true;
        }
        let worker = self.clone();
        let spawned = std::thread::Builder::new()
            .name("exp-agent-usage".to_string())
            .spawn(move || {
                // Clears `running` however the pass ends, a panic included:
                // a dead pass must never stop every later one.
                let _running = RunningGuard(worker.clone());
                let started = Instant::now();
                let payload = collect();
                let took = started.elapsed();
                if took >= Duration::from_secs(20) {
                    log::warn!("[agent-usage] a collection pass took {}s", took.as_secs());
                }
                let changed = worker.publish(payload);
                done(changed);
            });
        if spawned.is_err() {
            self.lock().running = false;
            log::warn!("[agent-usage] could not start the collection thread");
            return false;
        }
        true
    }
}

struct RunningGuard(UsageWorker);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.lock().running = false;
    }
}

/// Whether `next` says something `previous` did not: the account identity
/// (`accounts_key`, which ignores the `checkedAt` every pass restamps) or the
/// usage numbers.
pub fn status_changed(previous: &AgentStatusPayload, next: &AgentStatusPayload) -> bool {
    crate::agent_accounts::accounts_key(&previous.accounts)
        != crate::agent_accounts::accounts_key(&next.accounts)
        || previous.usage != next.usage
}

/// Dim every number the payload carries: the top-level usage and each
/// profile row's.
fn mark_stale(payload: &mut AgentStatusPayload) {
    for usage in payload.usage.values_mut() {
        usage.stale = true;
    }
    for account in payload.accounts.values_mut() {
        for row in &mut account.profiles {
            if let Some(usage) = row.usage.as_mut() {
                usage.stale = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_usage::{AgentUsage, UsageWindow};
    use std::sync::mpsc;

    fn payload(percent: u8) -> AgentStatusPayload {
        let mut payload = AgentStatusPayload::default();
        payload.usage.insert(
            "claude".to_string(),
            AgentUsage {
                fetched_at: "2026-11-12T00:00:00.000Z".to_string(),
                stale: false,
                windows: vec![UsageWindow {
                    key: "session".to_string(),
                    label: "5h".to_string(),
                    percent,
                    resets_at: None,
                }],
            },
        );
        payload
    }

    #[test]
    fn nothing_before_the_first_pass_lands() {
        assert_eq!(UsageWorker::new().latest(), None);
    }

    #[test]
    fn a_fresh_payload_ships_as_is_and_an_old_one_ships_stale() {
        let worker = UsageWorker::new();
        let at = Instant::now();
        assert!(worker.publish_at(payload(40), at));
        let fresh = worker.latest_at(at + Duration::from_secs(60)).unwrap();
        assert!(!fresh.usage["claude"].stale);
        let old = worker.latest_at(at + STALE_AFTER).unwrap();
        assert!(old.usage["claude"].stale, "a stuck collector dims its numbers");
        assert_eq!(old.usage["claude"].windows[0].percent, 40, "the last values stay");
    }

    #[test]
    fn publish_reports_only_a_real_change() {
        let worker = UsageWorker::new();
        assert!(worker.publish(payload(40)));
        assert!(!worker.publish(payload(40)));
        assert!(worker.publish(payload(41)));
    }

    /// The beat never waits on a pass: `start` returns at once, a second
    /// start while one runs is refused, and the slot reopens when it ends.
    #[test]
    fn start_returns_at_once_and_runs_one_pass_at_a_time() {
        let worker = UsageWorker::new();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<bool>();
        let begun = Instant::now();
        assert!(worker.start(
            move || {
                release_rx.recv().unwrap();
                payload(7)
            },
            move |changed| done_tx.send(changed).unwrap(),
        ));
        assert!(begun.elapsed() < Duration::from_secs(1), "start never blocks");
        assert!(worker.running());
        assert!(!worker.start(|| payload(8), |_| {}), "one pass at a time");
        assert_eq!(worker.latest(), None, "no payload until the pass lands");
        release_tx.send(()).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap());
        assert_eq!(worker.latest().unwrap().usage["claude"].windows[0].percent, 7);
        // `running` clears after `done` — wait for the guard.
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!worker.running());
    }

    #[test]
    fn a_panicking_pass_frees_the_slot() {
        let worker = UsageWorker::new();
        assert!(worker.start(|| panic!("probe blew up"), |_| {}));
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!worker.running());
        assert!(worker.start(|| payload(1), |_| {}));
    }
}
