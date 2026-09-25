//! EXP-1082 — the ACCOUNT ROTATION seams (EXP-1005 implements them).
//!
//! A workflow runs many unattended starts back to back; when one login's
//! usage window is spent the next start (or the stalled run) should move to
//! another signed-in profile of the same agent instead of parking until the
//! reset. Two PURE pickers decide that from plain data — the per-profile
//! usage [`crate::agent_usage::collect_now`] reads — and the hosts
//! (`ui::workflow_host`, the CLI daemon) call [`pick_start_account`] before
//! every ENGINE start, through `workflows::apply_engine_start`. Both return
//! `None` today, which keeps every launch on the account it already had. The
//! device opt-out is `Settings.auto_rotate_accounts` (default on,
//! claude-only).
//!
//! A START pick only: a resume (merge-upstream, review findings, a refused
//! land, a conflict relaunch) keeps its RECORDED account (EXP-906) and never
//! calls [`pick_start_account`]; rotating a run that hit a wall is the
//! mid-run switch ([`pick_rotation_target`], EXP-1005), not a start pick.

use std::collections::BTreeMap;

use crate::agent_accounts::Health;
use crate::CodingAgent;

/// One usage window: `percent` used (0–100) and when it resets (unix ms).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub percent: u8,
    pub resets_at: Option<i64>,
}

/// The windows the pickers weigh: the 5-hour session, the weekly, and the
/// model-scoped weeklies keyed by model alias.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageWindows {
    pub session: Option<Window>,
    pub weekly: Option<Window>,
    pub model: BTreeMap<String, Window>,
}

/// One account profile of one agent on this device, with its usage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileUsage {
    /// A device-local profile id (`system` = the ambient login).
    pub profile_id: String,
    pub agent: CodingAgent,
    pub signed_in: bool,
    pub health: Health,
    pub windows: UsageWindows,
}

/// The brake on a rotating run: no rotation before `cooldown_until` (unix
/// ms), and at most `cap` rotations per wall.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RotationGuard {
    pub cooldown_until: Option<i64>,
    pub rotations_this_wall: u32,
    pub cap: u32,
}

/// The account a fresh start should run on: the signed-in, healthy profile
/// of `agent` with the MOST HEADROOM — lowest session (5h) percent, then
/// weekly, then the `model` window — skipping any profile whose window sits
/// at 100 % with a reset still in the future. `None` = keep the launch's own
/// account. STUB (EXP-1005).
pub fn pick_start_account(
    _profiles: &[ProfileUsage],
    _agent: CodingAgent,
    _model: Option<&str>,
    _now_ms: i64,
) -> Option<String> {
    None
}

/// The profile a run that hit `window_hit` on `current` should move to:
/// never one that hit the SAME window inside its own reset, never while
/// `guard.cooldown_until` is in the future or `rotations_this_wall >= cap`.
/// `None` = wait for the reset. STUB (EXP-1005).
pub fn pick_rotation_target(
    _current: &str,
    _profiles: &[ProfileUsage],
    _window_hit: &str,
    _guard: &RotationGuard,
    _now_ms: i64,
) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    fn profile(id: &str, session: u8, weekly: u8) -> ProfileUsage {
        ProfileUsage {
            profile_id: id.to_string(),
            agent: CodingAgent::Claude,
            signed_in: true,
            health: Health::Ok,
            windows: UsageWindows {
                session: Some(Window {
                    percent: session,
                    resets_at: Some(NOW + 3_600_000),
                }),
                weekly: Some(Window {
                    percent: weekly,
                    resets_at: Some(NOW + 86_400_000),
                }),
                model: BTreeMap::new(),
            },
        }
    }

    #[test]
    #[ignore = "EXP-1005 implements the picker"]
    fn start_picks_the_profile_with_the_most_headroom() {
        // (profiles, expected): lowest session first, weekly breaks a tie,
        // unhealthy and spent-until-reset profiles are skipped.
        let mut unhealthy = profile("c", 0, 0);
        unhealthy.health = Health::NeedsRelogin;
        let table: Vec<(Vec<ProfileUsage>, Option<&str>)> = vec![
            (vec![profile("a", 80, 10), profile("b", 20, 90)], Some("b")),
            (vec![profile("a", 20, 50), profile("b", 20, 10)], Some("b")),
            (vec![profile("a", 60, 10), unhealthy], Some("a")),
            (vec![profile("a", 100, 10)], None),
            (Vec::new(), None),
        ];
        for (profiles, expected) in table {
            assert_eq!(
                pick_start_account(&profiles, CodingAgent::Claude, None, NOW).as_deref(),
                expected
            );
        }
    }

    #[test]
    #[ignore = "EXP-1005 implements the picker"]
    fn rotation_respects_the_guard_and_the_same_window() {
        let profiles = vec![
            profile("a", 100, 10),
            profile("b", 100, 10),
            profile("c", 5, 5),
        ];
        let open = RotationGuard {
            cooldown_until: None,
            rotations_this_wall: 0,
            cap: 3,
        };
        // Into the one profile that did NOT hit the session window.
        assert_eq!(
            pick_rotation_target("a", &profiles, "session", &open, NOW).as_deref(),
            Some("c")
        );
        // Cooling down, or at the cap: stay.
        let cooling = RotationGuard {
            cooldown_until: Some(NOW + 1),
            ..open.clone()
        };
        assert_eq!(
            pick_rotation_target("a", &profiles, "session", &cooling, NOW),
            None
        );
        let capped = RotationGuard {
            rotations_this_wall: 3,
            ..open
        };
        assert_eq!(
            pick_rotation_target("a", &profiles, "session", &capped, NOW),
            None
        );
    }

    #[test]
    fn the_stubs_keep_todays_account() {
        let profiles = vec![profile("a", 0, 0)];
        assert_eq!(
            pick_start_account(&profiles, CodingAgent::Claude, None, NOW),
            None
        );
        assert_eq!(
            pick_rotation_target("a", &profiles, "session", &RotationGuard::default(), NOW),
            None
        );
    }
}
