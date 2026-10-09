//! Motion: a node whose resolved style carries a `transition` animates its
//! colours, opacity, radii and transform (and a leaf its size) from the
//! style it SHOWED to the new one, eased by the transition's cubic bezier;
//! a newly opened overlay fades and rises in. Reduced motion: the core
//! resolves every transition to 0 ms and the view starts no enter
//! animation, so nothing here runs.

use std::collections::HashMap;
use std::time::Instant;

use exponential_ui::style::Transition;
use exponential_ui::surface::Frame;

use crate::paint::{cubic_bezier, PaintStyle};

struct StyleAnim {
    from: PaintStyle,
    start: Instant,
    duration_ms: f32,
    easing: [f32; 4],
}

struct FrameAnim {
    from: Frame,
    start: Instant,
    duration_ms: f32,
    easing: [f32; 4],
}

/// The enter animation of an overlay layer (fade + a short rise).
pub(crate) const ENTER_EASING: [f32; 4] = [0.0, 0.0, 0.2, 1.0];

#[derive(Default)]
pub(crate) struct Motion {
    styles: HashMap<u32, StyleAnim>,
    frames: HashMap<u32, FrameAnim>,
    layers: HashMap<String, (Instant, f32)>,
    /// The instant the frame being built paints at.
    pub now: Option<Instant>,
}

fn progress(start: Instant, duration_ms: f32, easing: [f32; 4], now: Instant) -> Option<f32> {
    if duration_ms <= 0.0 {
        return None;
    }
    let t = now.saturating_duration_since(start).as_secs_f32() * 1000.0 / duration_ms;
    (t < 1.0).then(|| cubic_bezier(easing, t))
}

impl Motion {
    /// A node's resolved style changed: animate from what it showed.
    pub fn style_changed(&mut self, index: u32, old: &PaintStyle, new: &PaintStyle, now: Instant) {
        let Some(t) = new.transition.or(old.transition) else {
            self.styles.remove(&index);
            return;
        };
        if old == new || t.duration_ms <= 0.0 {
            return;
        }
        let from = self.style(index, old, now);
        self.styles.insert(index, StyleAnim { from, start: now, duration_ms: t.duration_ms, easing: t.easing });
    }

    /// The style node `index` shows at `now` (its target when idle).
    pub fn style(&self, index: u32, target: &PaintStyle, now: Instant) -> PaintStyle {
        match self.styles.get(&index) {
            Some(a) => match progress(a.start, a.duration_ms, a.easing, now) {
                Some(p) => a.from.lerp(target, p),
                None => target.clone(),
            },
            None => target.clone(),
        }
    }

    /// A transitioned node's frame size changed (a progress fill).
    pub fn frame_changed(&mut self, index: u32, old: Frame, new: Frame, t: Transition, now: Instant) {
        if t.duration_ms <= 0.0 || old == new {
            return;
        }
        let from = self.frame(index, old, now);
        self.frames.insert(index, FrameAnim { from, start: now, duration_ms: t.duration_ms, easing: t.easing });
    }

    /// The frame node `index` shows at `now`.
    pub fn frame(&self, index: u32, target: Frame, now: Instant) -> Frame {
        match self.frames.get(&index).and_then(|a| progress(a.start, a.duration_ms, a.easing, now).map(|p| (a, p))) {
            Some((a, p)) => Frame { x: target.x, y: target.y, w: a.from.w + (target.w - a.from.w) * p, h: a.from.h + (target.h - a.from.h) * p },
            None => target,
        }
    }

    /// An overlay opened: it enters over `duration_ms`.
    pub fn layer_opened(&mut self, owner: &str, duration_ms: f32, now: Instant) {
        if duration_ms > 0.0 {
            self.layers.insert(owner.to_string(), (now, duration_ms));
        }
    }

    /// An open layer's enter progress (`None` = fully in).
    pub fn layer_progress(&self, owner: &str, now: Instant) -> Option<f32> {
        let (start, d) = self.layers.get(owner)?;
        progress(*start, *d, ENTER_EASING, now)
    }

    /// Drop finished animations; whether any still runs.
    pub fn active(&mut self, now: Instant) -> bool {
        self.styles.retain(|_, a| progress(a.start, a.duration_ms, a.easing, now).is_some());
        self.frames.retain(|_, a| progress(a.start, a.duration_ms, a.easing, now).is_some());
        self.layers.retain(|_, (s, d)| progress(*s, *d, ENTER_EASING, now).is_some());
        !(self.styles.is_empty() && self.frames.is_empty() && self.layers.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_style_change_eases_from_what_was_shown() {
        let mut m = Motion::default();
        let t0 = Instant::now();
        let a = PaintStyle { opacity: Some(0.0), ..Default::default() };
        let b = PaintStyle { opacity: Some(1.0), transition: Some(Transition { duration_ms: 100.0, easing: [0.0, 0.0, 1.0, 1.0] }), ..Default::default() };
        m.style_changed(1, &a, &b, t0);
        let mid = m.style(1, &b, t0 + Duration::from_millis(50)).opacity.unwrap();
        assert!((mid - 0.5).abs() < 0.05, "{mid}");
        assert_eq!(m.style(1, &b, t0 + Duration::from_millis(150)), b);
        assert!(m.active(t0 + Duration::from_millis(10)));
        assert!(!m.active(t0 + Duration::from_millis(200)));
        // No transition: a jump.
        let plain = PaintStyle { opacity: Some(1.0), ..Default::default() };
        m.style_changed(2, &a, &plain, t0);
        assert_eq!(m.style(2, &plain, t0), plain);
    }

    #[test]
    fn frames_and_layers_enter() {
        let mut m = Motion::default();
        let t0 = Instant::now();
        let t = Transition { duration_ms: 100.0, easing: [0.0, 0.0, 1.0, 1.0] };
        m.frame_changed(3, Frame { x: 0.0, y: 0.0, w: 0.0, h: 8.0 }, Frame { x: 0.0, y: 0.0, w: 100.0, h: 8.0 }, t, t0);
        let f = m.frame(3, Frame { x: 0.0, y: 0.0, w: 100.0, h: 8.0 }, t0 + Duration::from_millis(50));
        assert!((f.w - 50.0).abs() < 5.0, "{}", f.w);
        m.layer_opened("dlg", 120.0, t0);
        assert!(m.layer_progress("dlg", t0 + Duration::from_millis(60)).is_some());
        assert!(m.layer_progress("dlg", t0 + Duration::from_millis(200)).is_none());
    }
}
