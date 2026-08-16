//! EXP-501: per-account aggregate sync health — the pure model behind the
//! offline banner. Direct port of the proven iOS `SyncDebug` health machinery
//! (`apps/ios/ExpCore/Sources/Electric/SyncDebug.swift`), minus the
//! `unauthorized` case: on desktop a hard 401 already owns its own surface
//! ([`crate::SessionPhase::AuthExpired`] routes to login), so this model only
//! ever answers "can't reach the server" — it must never double-report auth.
//!
//! Times are wall-clock [`SystemTime`], NOT `Instant`: the 300s staleness
//! window exists exactly for suspend gaps, and a monotonic clock that pauses
//! with the machine would make an hours-old streak look fresh on wake — the
//! precise bug the window prevents. Clock skew saturates to zero.

use std::time::{Duration, SystemTime};

/// How long a failure streak must persist (with no intervening success)
/// before the banner may alarm. TIME-based by design (EXP-44): on app wake
/// every shape long-poll fails simultaneously before the first fresh success,
/// so any consecutive-failure COUNT would trip instantly on healthy servers.
pub const FAILURE_STREAK_GRACE: Duration = Duration::from_secs(12);

/// An error older than this no longer alarms ([`AccountHealth::health`]'s
/// staleness guard), and a failure GAP this long breaks the streak's
/// continuity. While genuinely failing, the retry loops report at most ~30s
/// apart ([`crate::client`]'s backoff cap) — a far longer gap means they
/// weren't running (machine suspended mid-outage), so the wake burst's first
/// fresh failure must RESTART the debounce instead of inheriting an hours-old
/// streak start (which would flash the banner immediately on resume).
pub const ERROR_STALENESS_WINDOW: Duration = Duration::from_secs(300);

/// What the offline banner should say, if anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncHealth {
    #[default]
    Ok,
    /// The failure streak persisted past [`FAILURE_STREAK_GRACE`] — the
    /// server is unreachable and the boards are showing cached data.
    Offline,
}

/// One account's aggregate poll health. Every `Applied` delta (row batch or
/// idle `up-to-date` heartbeat — both are 2xx polls) records a success; every
/// `PollFailed` records a failure. Keyed per account by the caller — only the
/// ACTIVE account's entry may drive the banner (a background account's outage
/// must never alarm while the active one syncs fine).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountHealth {
    pub last_success_at: Option<SystemTime>,
    pub last_error_at: Option<SystemTime>,
    /// Start of the CURRENT uninterrupted failure streak: set on the first
    /// failure after a success (or ever), left alone while failures repeat,
    /// cleared by ANY success, and RESTARTED when a failure lands after a
    /// [`ERROR_STALENESS_WINDOW`]-sized quiet gap.
    pub failure_streak_started_at: Option<SystemTime>,
    /// The most recent failure's display string, for diagnostics.
    pub last_error: Option<String>,
}

impl AccountHealth {
    pub fn record_success(&mut self, now: SystemTime) {
        self.last_success_at = Some(now);
        self.failure_streak_started_at = None;
    }

    pub fn record_failure(&mut self, now: SystemTime, error: String) {
        if Self::streak_broken(self.last_error_at, self.failure_streak_started_at, now) {
            self.failure_streak_started_at = Some(now);
        }
        self.last_error_at = Some(now);
        self.last_error = Some(error);
    }

    /// Whether a fresh failure starts a NEW streak instead of extending the
    /// current one (see [`ERROR_STALENESS_WINDOW`]).
    fn streak_broken(
        previous_error_at: Option<SystemTime>,
        streak_started_at: Option<SystemTime>,
        now: SystemTime,
    ) -> bool {
        if streak_started_at.is_none() {
            return true;
        }
        let Some(previous_error_at) = previous_error_at else {
            return true;
        };
        elapsed(previous_error_at, now) >= ERROR_STALENESS_WINDOW
    }

    /// PURE READ — mirrors iOS `SyncDebug.health(forAccountId:)` exactly. All
    /// state mutation stays in the `record_*` methods; render paths
    /// re-evaluate this freely.
    pub fn health(&self, now: SystemTime) -> SyncHealth {
        let Some(err) = self.last_error_at else {
            return SyncHealth::Ok;
        };
        // ANY success after the last failure clears instantly.
        if self.last_success_at.is_some_and(|ok| ok > err) {
            return SyncHealth::Ok;
        }
        // Staleness guard: an error that stopped repeating long ago (the
        // retry loops died with the machine suspended) mustn't alarm on wake.
        if elapsed(err, now) >= ERROR_STALENESS_WINDOW {
            return SyncHealth::Ok;
        }
        // Alarm only once the streak persisted through the grace window —
        // the wake-up burst resolves via a 2xx (streak cleared) well inside
        // it, while a genuine outage keeps the streak alive.
        match self.failure_streak_started_at {
            Some(start) if elapsed(start, now) >= FAILURE_STREAK_GRACE => SyncHealth::Offline,
            _ => SyncHealth::Ok,
        }
    }
}

/// `now - t`, saturating to zero on clock skew.
fn elapsed(t: SystemTime, now: SystemTime) -> Duration {
    now.duration_since(t).unwrap_or(Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn err(h: &mut AccountHealth, secs: u64) {
        h.record_failure(at(secs), "http 500".to_string());
    }

    #[test]
    fn no_error_is_ok() {
        let h = AccountHealth::default();
        assert_eq!(h.health(at(1_000)), SyncHealth::Ok);
        let mut h = AccountHealth::default();
        h.record_success(at(500));
        assert_eq!(h.health(at(1_000)), SyncHealth::Ok);
    }

    #[test]
    fn failure_within_grace_is_ok() {
        let mut h = AccountHealth::default();
        err(&mut h, 1_000);
        assert_eq!(h.health(at(1_005)), SyncHealth::Ok);
        assert_eq!(h.health(at(1_011)), SyncHealth::Ok);
    }

    #[test]
    fn streak_past_grace_is_offline() {
        let mut h = AccountHealth::default();
        err(&mut h, 1_000);
        err(&mut h, 1_010);
        assert_eq!(h.health(at(1_012)), SyncHealth::Offline);
    }

    #[test]
    fn success_after_error_clears_instantly() {
        let mut h = AccountHealth::default();
        err(&mut h, 1_000);
        err(&mut h, 1_020);
        assert_eq!(h.health(at(1_020)), SyncHealth::Offline);
        h.record_success(at(1_021));
        assert_eq!(h.health(at(1_021)), SyncHealth::Ok);
    }

    #[test]
    fn error_past_staleness_window_is_ok() {
        // The streak persisted past the grace, but the machine then suspended:
        // on wake the hours-old error must not alarm.
        let mut h = AccountHealth::default();
        err(&mut h, 1_000);
        err(&mut h, 1_020);
        assert_eq!(h.health(at(1_030)), SyncHealth::Offline);
        assert_eq!(h.health(at(1_020 + 300)), SyncHealth::Ok);
    }

    #[test]
    fn gap_past_staleness_restarts_streak() {
        let mut h = AccountHealth::default();
        err(&mut h, 1_000);
        err(&mut h, 1_020);
        // First fresh failure after a suspend-sized gap: a new streak with a
        // fresh grace window, not an instant alarm off the old streak start.
        err(&mut h, 1_020 + 400);
        assert_eq!(h.failure_streak_started_at, Some(at(1_420)));
        assert_eq!(h.health(at(1_421)), SyncHealth::Ok);
        // ...and if the failures keep coming, the new streak alarms normally.
        err(&mut h, 1_430);
        assert_eq!(h.health(at(1_433)), SyncHealth::Offline);
    }

    #[test]
    fn success_resets_grace_for_next_failure() {
        let mut h = AccountHealth::default();
        err(&mut h, 1_000);
        err(&mut h, 1_015);
        h.record_success(at(1_016));
        // A lone new failure starts a fresh streak — grace applies again.
        err(&mut h, 1_017);
        assert_eq!(h.failure_streak_started_at, Some(at(1_017)));
        assert_eq!(h.health(at(1_020)), SyncHealth::Ok);
        assert_eq!(h.health(at(1_030)), SyncHealth::Offline);
    }
}
