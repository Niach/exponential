//! Motion (EXP-523) — the shared duration/easing tokens as gpui-ready values.
//!
//! Durations and cubic-bezier control points are generated into
//! [`crate::tokens::motion`] from `packages/design-tokens/tokens.json`, the same
//! source the web CSS vars and the iOS/Android motion tokens come from. This
//! module is the hand-written half: it wraps the millisecond constants in
//! [`Duration`] and turns the control points into an easing closure that gpui's
//! `Animation::with_easing` (and `gpui_component::animation::EffectTransition`)
//! can take.
//!
//! **Never call `gpui_component::animation::cubic_bezier` with these control
//! points.** That helper computes its own `x(t)` and throws it away
//! (`let _x = …`), evaluating `y` over the RAW progress — a visibly different
//! curve from CSS `cubic-bezier()`, SwiftUI `Animation.timingCurve` and Compose
//! `CubicBezierEasing`. [`ease`] below solves `x(t) = progress` first, which is
//! what keeps the desktop's feel identical to the other three clients.
//!
//! gpui exposes NO OS reduce-motion signal — unlike iOS
//! (`accessibilityReduceMotion`), Android (`ANIMATOR_DURATION_SCALE`) and web
//! (`prefers-reduced-motion`), there is nothing here to honour, and the
//! accessibility surface is AccessKit tree-building only. If a reduce-motion
//! opt-out is ever wanted, the hook is a `reduceMotion` key in the shared
//! `{data_dir}/settings.json`; do not add it speculatively.

use crate::tokens::motion as m;
use std::time::Duration;

/// Hover / opacity micro-feedback.
pub const FAST: Duration = Duration::from_millis(m::duration::FAST_MS);
/// The default: enter/exit, expand/collapse, position changes.
pub const STANDARD: Duration = Duration::from_millis(m::duration::STANDARD_MS);
/// Large surfaces — docks, sheets, full-column swaps.
pub const SLOW: Duration = Duration::from_millis(m::duration::SLOW_MS);

/// EXP-1243: a pressed button's scale (`motion.scale.press`). gpui has NO
/// element transform (only SVG paths rotate/scale), so the desktop cannot
/// scale a button: a press shows the control's own active tint instead.
pub const PRESS_SCALE: f32 = m::scale::PRESS;
/// EXP-1243: where a popover, menu or picker scales in from
/// (`motion.scale.popoverFrom`, never 0). Same gpui limit: the desktop's
/// overlays enter as a fade (plus the SDK's short rise), never a scale.
pub const POPOVER_FROM_SCALE: f32 = m::scale::POPOVER_FROM;

/// EXP-1243: an overlay's ENTER — [`STANDARD`] on [`decelerate`].
pub const ENTER: Duration = STANDARD;
/// EXP-1243: an overlay's EXIT — faster than the enter, [`FAST`] on
/// [`accelerate`], opacity only.
pub const EXIT: Duration = FAST;

/// EXP-1243: the motion rules, in the web styleguide's `tokens-motion`
/// order (`apps/styleguide/src/components.tsx`): what moves, and how. The
/// IDE styleguide lists them under its feedback-states entry.
pub fn rules() -> Vec<(&'static str, String)> {
    let (fast, standard, slow) = (
        m::duration::FAST_MS,
        m::duration::STANDARD_MS,
        m::duration::SLOW_MS,
    );
    let (press, from) = (m::scale::PRESS, m::scale::POPOVER_FROM);
    vec![
        (
            "Popover, menu, tooltip",
            format!("enter · decelerate {standard} · scale {from} → 1 + fade, from the trigger"),
        ),
        ("Popover exit", format!("accelerate {fast} · fade only")),
        ("Dialog", format!("enter · decelerate {standard} · scale {from} → 1 + fade")),
        (
            "Sheet",
            format!("enter · decelerate {slow} · slide · exit accelerate {standard}"),
        ),
        ("Button press", format!("scale {press}")),
        ("Keyboard, typeahead, tabs, toggles", "none".to_string()),
        ("Toasts", "toast-stack.json".to_string()),
        ("Reduce motion", "opacity only".to_string()),
    ]
}

/// A CSS-accurate cubic-bezier easing over control points `[x1, y1, x2, y2]`
/// (P0 = (0,0) and P3 = (1,1) implicit): Newton–Raphson solves `x(t) =
/// progress`, then the result is evaluated through `y(t)`.
pub fn ease(c: [f32; 4]) -> impl Fn(f32) -> f32 {
    move |progress: f32| {
        let x = progress.clamp(0.0, 1.0);
        // Cubic Bezier basis with P0 = 0 and P3 = 1 implicit.
        let bez = |p1: f32, p2: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
        };
        let dbez = |p1: f32, p2: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
        };
        // 8 iterations seeded at t = x — WebKit's UnitBezier budget, far
        // inside a frame's precision even for the flattest curve we ship.
        let mut t = x;
        for _ in 0..8 {
            let dx = dbez(c[0], c[2], t);
            if dx.abs() < 1e-6 {
                break;
            }
            t = (t - (bez(c[0], c[2], t) - x) / dx).clamp(0.0, 1.0);
        }
        bez(c[1], c[3], t)
    }
}

/// Default curve — for anything that both enters and leaves.
pub fn standard() -> impl Fn(f32) -> f32 {
    ease(m::ease::STANDARD)
}

/// For things arriving on screen.
pub fn decelerate() -> impl Fn(f32) -> f32 {
    ease(m::ease::DECELERATE)
}

/// For things leaving it.
pub fn accelerate() -> impl Fn(f32) -> f32 {
    ease(m::ease::ACCELERATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_come_from_the_shared_tokens() {
        assert_eq!(STANDARD, Duration::from_millis(m::duration::STANDARD_MS));
        assert!(FAST < STANDARD && STANDARD < SLOW);
    }

    /// EXP-1243: the scales are the shared tokens, and an overlay leaves
    /// faster than it arrives.
    #[test]
    fn the_scales_and_the_enter_exit_pair_are_the_rules() {
        assert_eq!(PRESS_SCALE, 0.97);
        assert_eq!(POPOVER_FROM_SCALE, 0.96);
        assert!(EXIT < ENTER);
        let rules = rules();
        assert_eq!(rules.len(), 8);
        assert_eq!(rules[0].1, "enter · decelerate 180 · scale 0.96 → 1 + fade, from the trigger");
        assert_eq!(rules[1].1, "accelerate 120 · fade only");
        assert_eq!(rules[4].1, "scale 0.97");
        assert_eq!(rules[7], ("Reduce motion", "opacity only".to_string()));
    }

    #[test]
    fn every_curve_is_pinned_at_both_ends() {
        for c in [m::ease::STANDARD, m::ease::DECELERATE, m::ease::ACCELERATE] {
            let f = ease(c);
            assert!(f(0.0).abs() < 1e-3, "{c:?} must start at 0");
            assert!((f(1.0) - 1.0).abs() < 1e-3, "{c:?} must end at 1");
        }
    }

    #[test]
    fn every_curve_is_monotonic_and_in_range() {
        for c in [m::ease::STANDARD, m::ease::DECELERATE, m::ease::ACCELERATE] {
            let f = ease(c);
            let mut prev = f(0.0);
            for i in 1..=100 {
                let v = f(i as f32 / 100.0);
                assert!(v >= prev - 1e-4, "{c:?} regressed at {i}: {prev} -> {v}");
                assert!((-1e-4..=1.0 + 1e-4).contains(&v), "{c:?} out of range: {v}");
                prev = v;
            }
        }
    }

    #[test]
    fn solving_for_x_differs_from_evaluating_y_over_raw_progress() {
        // The bug this module exists to avoid: gpui-component's `cubic_bezier`
        // skips the solve. On our `standard` curve the two disagree by a lot at
        // mid-flight, which is exactly where the eye is.
        let c = m::ease::STANDARD;
        let solved = ease(c)(0.5);
        let u: f32 = 0.5;
        let raw = 3.0 * (1.0 - u) * (1.0 - u) * u * c[1]
            + 3.0 * (1.0 - u) * u * u * c[3]
            + u * u * u;
        assert!(
            (solved - raw).abs() > 0.05,
            "expected a visible difference, got solved={solved} raw={raw}"
        );
    }

    #[test]
    fn easing_clamps_out_of_range_progress() {
        let f = ease(m::ease::STANDARD);
        assert!(f(-1.0).abs() < 1e-3);
        assert!((f(2.0) - 1.0).abs() < 1e-3);
    }
}
