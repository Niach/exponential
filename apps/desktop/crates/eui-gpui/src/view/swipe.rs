//! FEED-200: `Tabs.swipeable` on the desktop. gpui has no touch, but macOS
//! hands it CLEAN two-finger trackpad scrolls: `ScrollDelta::Pixels`
//! (`hasPreciseScrollingDeltas`, the content's direction: natural
//! scrolling, fingers left = negative x) with the finger phase
//! (`NSEventPhase` Began/MayBegin → `Started`, Ended → `Ended`, the rest,
//! momentum included, → `Moved`). A horizontal-dominant one over a
//! swipeable Tabs' content pages it like a finger: `swipe {phase:
//! start|move|end, offset, velocity}` to the core (`surface::tabs`), which
//! draws the panels and the thumb by paint-time transforms. Momentum after
//! the fingers lift arrives as `Moved` without a gesture and never
//! re-triggers (it is swallowed while it stays horizontal). A mouse wheel
//! (`Lines`) and a platform without phases (no `Started`) never swipe.
//!
//! Hit rule: a gesture STARTING over a node `swipe::swipe_blocks` names
//! (inside the content) is that node's. Both sides listen in gpui's
//! CAPTURE phase, in paint order: the surface clears the hovered blockers,
//! each blocker under the pointer adds itself, the content's probe (painted
//! after its subtree) reads them, and a horizontal swipe stops the event
//! before any scroller inside sees it.

use std::time::{Duration, Instant};

use eui_core::swipe::{swipe_blocks, swipe_lock};
use gpui::{canvas, prelude::*, AnyElement, Context, DispatchPhase, HitboxBehavior, ScrollDelta, ScrollWheelEvent, TouchPhase};
use serde_json::{json, Value};

use super::SurfaceView;

/// A fingers-down gesture with no event for this long ended without a
/// phase (macOS reports a cancelled scroll as `Moved`): it settles.
const SWIPE_WATCHDOG: Duration = Duration::from_millis(500);
/// Momentum events (phase-less `Moved`) further apart than this from the
/// last swallowed one are a new input, not the fling's tail.
const MOMENTUM_GAP_MS: f32 = 150.0;
/// The window the release velocity is measured over.
const VELOCITY_WINDOW_MS: f32 = 100.0;

/// One two-finger gesture over a swipeable Tabs' content.
#[derive(Debug, Clone)]
pub(crate) struct WheelSwipe {
    /// The content node's id.
    pub content: String,
    pub dx: f32,
    pub dy: f32,
    /// `swipe::swipe_lock` of the travel (`blocked` = it started over a
    /// node that pans itself).
    pub lock: &'static str,
    /// (when, offset) of the recent moves, for the release velocity.
    pub samples: Vec<(Instant, f32)>,
}

/// The release velocity (px/ms) of `samples` ending at `offset` at `now`.
pub(crate) fn release_velocity(samples: &[(Instant, f32)], offset: f32, now: Instant) -> f32 {
    let Some(&(t0, o0)) = samples.iter().find(|(t, _)| now.saturating_duration_since(*t).as_secs_f32() * 1000.0 <= VELOCITY_WINDOW_MS) else { return 0.0 };
    let dt = now.saturating_duration_since(t0).as_secs_f32() * 1000.0;
    if dt < 1.0 {
        0.0
    } else {
        (offset - o0) / dt
    }
}

impl SurfaceView {
    /// Is node `index` the content part of a `swipeable` Tabs?
    pub(crate) fn is_swipe_content(&self, index: u32) -> bool {
        let Some(n) = self.cache.node(index) else { return false };
        n.owner_component.as_deref() == Some("Tabs") && n.part.as_deref() == Some("content") && self.owner_props(index).get("swipeable").and_then(Value::as_bool) == Some(true)
    }

    /// Does node `index` pan by itself inside a swipeable Tabs' content?
    pub(crate) fn is_swipe_blocker(&self, index: u32) -> bool {
        let Some(n) = self.cache.node(index) else { return false };
        let overflow_x = self.surface.visual(index).and_then(|v| v.overflow_x.as_deref());
        if !swipe_blocks(&n.component, n.props.get("direction").and_then(Value::as_str), overflow_x) {
            return false;
        }
        let mut at = n.parent;
        while let Some(p) = at {
            if self.is_swipe_content(p) {
                return true;
            }
            at = self.cache.node(p).and_then(|x| x.parent);
        }
        false
    }

    /// Is node `index` inside (or) `ancestor`?
    fn within(&self, mut index: u32, ancestor: u32) -> bool {
        loop {
            if index == ancestor {
                return true;
            }
            match self.cache.node(index).and_then(|n| n.parent) {
                Some(p) => index = p,
                None => return false,
            }
        }
    }

    /// The probe a blocker paints: a hitbox over its box that, in the
    /// capture phase, reports itself hovered for this wheel event.
    pub(crate) fn swipe_blocker_probe(&self, index: u32) -> AnyElement {
        let hits = self.swipe_hits.clone();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |_: &ScrollWheelEvent, phase, window, _| {
                    if phase == DispatchPhase::Capture && hitbox.should_handle_scroll(window) {
                        hits.borrow_mut().push(index);
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    /// The content's probe (painted AFTER its subtree, so its capture
    /// listener runs after the blockers'): feeds [`Self::wheel_swipe`].
    pub(crate) fn swipe_content_probe(&self, index: u32) -> AnyElement {
        let this = self.this.clone();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                let this = this.clone();
                window.on_mouse_event(move |ev: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let hovered = hitbox.should_handle_scroll(window);
                    let stop = this.update(cx, |v, cx| v.wheel_swipe(index, hovered, ev, cx)).unwrap_or(false);
                    if stop {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    /// One wheel event over (or for the live gesture of) swipeable content
    /// `index`; true = it was the pager's (stop it).
    pub(crate) fn wheel_swipe(&mut self, index: u32, hovered: bool, ev: &ScrollWheelEvent, cx: &mut Context<Self>) -> bool {
        let ScrollDelta::Pixels(d) = ev.delta else { return false };
        let (dx, dy) = (f32::from(d.x), f32::from(d.y));
        let Some(id) = self.cache.node(index).map(|n| n.id.clone()) else { return false };
        let mine = self.wheel_swipe.as_ref().is_some_and(|s| s.content == id);
        if !hovered && !mine {
            return false;
        }
        match ev.touch_phase {
            TouchPhase::Started => {
                if !hovered {
                    return false;
                }
                let hits: Vec<u32> = self.swipe_hits.borrow().iter().copied().filter(|&b| b != index && self.within(b, index)).collect();
                // A nested swipeable Tabs under the pointer took it.
                if hits.iter().any(|&b| self.is_swipe_content(b)) {
                    return false;
                }
                // A new gesture (MayBegin then Began both start one).
                self.end_wheel_swipe(0.0, cx);
                self.wheel_momentum = None;
                let lock = if hits.is_empty() { "pending" } else { "blocked" };
                self.wheel_swipe = Some(WheelSwipe { content: id, dx, dy, lock, samples: Vec::new() });
                self.swipe_hits.borrow_mut().push(index);
            }
            TouchPhase::Moved => {
                if !mine {
                    // Momentum after a horizontal swipe: never re-triggers,
                    // never scrolls what is around (a chain of events each
                    // within MOMENTUM_GAP_MS of the last).
                    let now = self.clock();
                    let tail = self.wheel_momentum.is_some_and(|t| now.saturating_duration_since(t).as_secs_f32() * 1000.0 <= MOMENTUM_GAP_MS);
                    self.wheel_momentum = tail.then_some(now);
                    return tail && dx.abs() >= dy.abs();
                }
                if let Some(s) = self.wheel_swipe.as_mut() {
                    s.dx += dx;
                    s.dy += dy;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if !mine {
                    return false;
                }
                if let Some(s) = self.wheel_swipe.as_mut() {
                    s.dx += dx;
                    s.dy += dy;
                }
                let horizontal = self.wheel_swipe.as_ref().is_some_and(|s| s.lock == "horizontal");
                if ev.touch_phase == TouchPhase::Cancelled {
                    if horizontal {
                        self.fire(index, "swipe", Some(json!({"phase": "cancel", "offset": 0, "velocity": 0})), cx);
                    }
                    self.wheel_swipe = None;
                    self.wheel_watchdog = None;
                } else {
                    self.end_wheel_swipe(1.0, cx);
                }
                self.wheel_momentum = horizontal.then(|| self.clock());
                return horizontal;
            }
        }
        let now = self.clock();
        let Some(s) = self.wheel_swipe.as_mut() else { return false };
        let mut started = false;
        if s.lock == "pending" {
            s.lock = swipe_lock(s.dx.into(), s.dy.into());
            started = s.lock == "horizontal";
        }
        if s.lock != "horizontal" {
            return false;
        }
        let offset = s.dx;
        s.samples.retain(|(t, _)| now.saturating_duration_since(*t).as_secs_f32() * 1000.0 <= VELOCITY_WINDOW_MS);
        s.samples.push((now, offset));
        if started {
            self.fire(index, "swipe", Some(json!({"phase": "start", "offset": 0, "velocity": 0})), cx);
        }
        self.fire(index, "swipe", Some(json!({"phase": "move", "offset": offset, "velocity": 0})), cx);
        self.arm_wheel_watchdog(cx);
        true
    }

    /// The live gesture's release: a horizontal one ends (`velocity_scale`
    /// 0 = settle on distance alone, the watchdog's case).
    fn end_wheel_swipe(&mut self, velocity_scale: f32, cx: &mut Context<Self>) {
        self.wheel_watchdog = None;
        let Some(s) = self.wheel_swipe.take() else { return };
        if s.lock != "horizontal" {
            return;
        }
        let Some(index) = self.cache.index_of(&s.content) else { return };
        let velocity = release_velocity(&s.samples, s.dx, self.clock()) * velocity_scale;
        self.fire(index, "swipe", Some(json!({"phase": "end", "offset": s.dx, "velocity": velocity})), cx);
    }

    fn arm_wheel_watchdog(&mut self, cx: &mut Context<Self>) {
        self.wheel_watchdog = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SWIPE_WATCHDOG).await;
            let _ = this.update(cx, |this, cx| {
                this.wheel_watchdog = None;
                let now = this.clock();
                this.end_wheel_swipe(0.0, cx);
                this.wheel_momentum = Some(now);
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_velocity_reads_the_last_100_ms() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let samples = vec![(at(0), 0.0), (at(150), -20.0), (at(200), -60.0)];
        // Only the samples within 100 ms of the release count.
        let v = release_velocity(&samples, -100.0, at(250));
        assert!((v - (-80.0 / 100.0)).abs() < 1e-3, "{v}");
        assert_eq!(release_velocity(&[], -10.0, at(0)), 0.0);
        assert_eq!(release_velocity(&samples, -10.0, at(1000)), 0.0, "a stale gesture flings nothing");
    }
}
