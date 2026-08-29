//! EXP-533: the "catching up" model — what the rail's sync spinner means.
//! gpui-free (the collections layer is the only gpui seam, §3.1), so the
//! predicate is unit-testable without an `App`.
//!
//! Two independent reasons to say "syncing":
//!
//! 1. **A core shape has not reached head.** The five shapes a board is made
//!    of (teams/boards/issues/issue_labels/labels — the Android parity set):
//!    while any of them is still snapshotting or refetching, what the user
//!    sees is genuinely incomplete.
//! 2. **A restart is in flight.** After a wake or a Retry every shape re-polls
//!    from its persisted cursor; the shapes stay `Live` throughout (the
//!    cursor state never leaves live), so nothing in (1) would ever show. A
//!    [`CatchUp`] stamp tracks which core shapes have reported back since the
//!    restart and clears itself as they do.
//!
//! The stamp is BOUNDED by [`CATCHING_UP_WINDOW`]: an offline machine would
//! otherwise spin forever, and the offline banner — not a spinner — owns that
//! story.

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

/// Where a shape stands in its sync lifecycle — drives the debug board's
/// status line and the §4.1 `is_ready` skeleton-vs-empty distinction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeSyncPhase {
    /// No pipeline yet / no persisted cursor state.
    Waiting,
    /// Initial snapshot in progress (rows may be arriving).
    Snapshot,
    /// Caught up to head (`up-to-date` seen) — long-polling live.
    Live,
    /// A 409 / must-refetch was seen; the atomic re-snapshot is pending.
    /// Stale rows stay visible until it lands (§5.6c).
    Refetching,
}

impl ShapeSyncPhase {
    pub fn label(&self) -> &'static str {
        match self {
            ShapeSyncPhase::Waiting => "waiting",
            ShapeSyncPhase::Snapshot => "snapshot",
            ShapeSyncPhase::Live => "live",
            ShapeSyncPhase::Refetching => "refetching",
        }
    }
}

/// One entry of the per-shape status line (the Phase-2 gate's "renders a
/// board" evidence surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeStatus {
    pub name: &'static str,
    pub phase: ShapeSyncPhase,
    pub rows: usize,
}

/// The not-yet-live shapes of a status snapshot, in the snapshot's own
/// (stable) order. Split out from the `Store` accessor so the predicate is
/// unit-testable without an `App`.
pub fn not_ready_names(statuses: &[ShapeStatus]) -> Vec<&'static str> {
    statuses
        .iter()
        .filter(|status| {
            !matches!(
                status.phase,
                ShapeSyncPhase::Live | ShapeSyncPhase::Refetching
            )
        })
        .map(|status| status.name)
        .collect()
}

/// The shapes a board is MADE of — the ones whose lag the user can actually
/// see. Byte-parity with the Android core set. Notifications, devices,
/// support and friends sync on their own schedule and must never park a
/// spinner on the rail.
pub const CORE_SHAPES: [&str; 5] = ["teams", "boards", "issues", "issue_labels", "labels"];

/// How long a restart may claim "catching up" before the stamp expires. A
/// healthy pipeline re-reaches head in well under this; an offline one never
/// will, and the offline banner (12s grace) owns that story instead.
pub const CATCHING_UP_WINDOW: Duration = Duration::from_secs(15);

/// The post-restart stamp (reason 2 in the module docs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatchUp {
    /// When the restart happened; `None` = no restart in flight.
    pub started_at: Option<SystemTime>,
    /// Core shapes that have NOT reported a successful batch since then.
    pub pending: BTreeSet<&'static str>,
}

impl CatchUp {
    /// Stamp a fresh restart: every core shape owes a report.
    pub fn begin(&mut self, now: SystemTime) {
        self.started_at = Some(now);
        self.pending = CORE_SHAPES.iter().copied().collect();
    }

    /// Record one shape's successful batch. Returns whether this actually
    /// SHRANK the pending set — the caller only needs to repaint on a real
    /// change (every shape heartbeats roughly per minute otherwise).
    pub fn record_success(&mut self, shape: &str) -> bool {
        self.pending.remove(shape)
    }

    /// Whether the rail should show its sync spinner.
    pub fn is_catching_up(&self, statuses: &[ShapeStatus], now: SystemTime) -> bool {
        if statuses
            .iter()
            .any(|status| CORE_SHAPES.contains(&status.name) && status.phase != ShapeSyncPhase::Live)
        {
            return true;
        }
        let Some(started_at) = self.started_at else {
            return false;
        };
        if self.pending.is_empty() {
            return false;
        }
        // Clock skew saturates to zero — a backwards step must not expire a
        // stamp that was made a moment ago.
        now.duration_since(started_at).unwrap_or(Duration::ZERO) < CATCHING_UP_WINDOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn status(name: &'static str, phase: ShapeSyncPhase) -> ShapeStatus {
        ShapeStatus {
            name,
            phase,
            rows: 0,
        }
    }

    fn all_live() -> Vec<ShapeStatus> {
        CORE_SHAPES
            .iter()
            .map(|name| status(name, ShapeSyncPhase::Live))
            .collect()
    }

    /// EXP-633: the ready predicate IS `Collection::is_ready` — Live and
    /// Refetching count as ready (a refetch replays rows the collection
    /// already has), Waiting and Snapshot do not.
    #[test]
    fn not_ready_names_lists_shapes_before_their_first_up_to_date() {
        let statuses = [
            status("issues", ShapeSyncPhase::Live),
            status("comments", ShapeSyncPhase::Waiting),
            status("labels", ShapeSyncPhase::Refetching),
            status("issue_events", ShapeSyncPhase::Snapshot),
        ];
        assert_eq!(not_ready_names(&statuses), vec!["comments", "issue_events"]);
        let all_live = [
            status("issues", ShapeSyncPhase::Live),
            status("labels", ShapeSyncPhase::Refetching),
        ];
        assert!(not_ready_names(&all_live).is_empty());
        assert!(not_ready_names(&[]).is_empty());
    }

    #[test]
    fn a_core_shape_short_of_live_is_catching_up_without_any_kick() {
        let idle = CatchUp::default();
        for phase in [
            ShapeSyncPhase::Waiting,
            ShapeSyncPhase::Snapshot,
            // A refetch is "ready" for the skeleton test (stale rows stay
            // visible) but it IS an in-flight resync — the spinner says so.
            ShapeSyncPhase::Refetching,
        ] {
            let mut statuses = all_live();
            statuses[2] = status("issues", phase);
            assert!(
                idle.is_catching_up(&statuses, at(1_000)),
                "issues in {phase:?} must show the spinner"
            );
        }
    }

    #[test]
    fn all_core_shapes_live_with_no_kick_is_quiet() {
        let idle = CatchUp::default();
        assert!(!idle.is_catching_up(&all_live(), at(1_000)));
        // A NON-core shape still snapshotting must not park the spinner.
        let mut statuses = all_live();
        statuses.push(status("notifications", ShapeSyncPhase::Snapshot));
        assert!(!idle.is_catching_up(&statuses, at(1_000)));
    }

    #[test]
    fn a_restart_is_catching_up_until_every_core_shape_reports() {
        let mut catch_up = CatchUp::default();
        catch_up.begin(at(1_000));
        assert!(catch_up.is_catching_up(&all_live(), at(1_001)));
        for name in CORE_SHAPES.iter().take(CORE_SHAPES.len() - 1) {
            assert!(catch_up.record_success(name));
            assert!(catch_up.is_catching_up(&all_live(), at(1_002)));
        }
        assert!(catch_up.record_success(CORE_SHAPES[CORE_SHAPES.len() - 1]));
        assert!(!catch_up.is_catching_up(&all_live(), at(1_002)));
    }

    #[test]
    fn the_window_bounds_a_restart_that_never_completes() {
        // Offline: the shapes stay Live (their cursor state never leaves
        // live) and nothing ever reports. The spinner must stop; the offline
        // banner owns that story.
        let mut catch_up = CatchUp::default();
        catch_up.begin(at(1_000));
        assert!(catch_up.is_catching_up(&all_live(), at(1_014)));
        assert!(!catch_up.is_catching_up(&all_live(), at(1_015)));
        assert!(!catch_up.pending.is_empty());
    }

    #[test]
    fn record_success_reports_only_real_shrinks() {
        let mut catch_up = CatchUp::default();
        // Nothing pending yet (no restart) — nothing to shrink.
        assert!(!catch_up.record_success("issues"));
        catch_up.begin(at(1_000));
        assert!(catch_up.record_success("issues"));
        // The per-minute heartbeat of an already-reported shape is not a
        // change.
        assert!(!catch_up.record_success("issues"));
        // Non-core successes are ignored entirely.
        assert!(!catch_up.record_success("notifications"));
    }
}
