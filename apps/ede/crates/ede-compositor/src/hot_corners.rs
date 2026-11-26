//! Hot corners (#EDE-246) as Cinnamon has them (`hotCorner.js`): the four
//! corners of the primary output, top-left, top-right, bottom-left,
//! bottom-right, each `action:enabled:delay` (`org.cinnamon
//! hotcorner-layout`, the same strings in `[hot_corners] layout`). Mint
//! ships them all off. An enabled corner is a 2×2 px spot on top of
//! everything: the pointer resting there `delay` ms runs its action, and a
//! click on it runs it too (unless the hover just did, within 500 ms), the
//! click never reaching what lies under it. `expo` and `scale` are the
//! shell's overlays (Ctrl+Alt+Up/Down), `desktop` shows the desktop
//! ([`Ede::toggle_desktop`]), anything else is a command. Over a fullscreen
//! window the corners sleep unless `fullscreen = true`. Only the person's own
//! pointer reaches them; a driven one never does.

use std::time::{Duration, Instant};

use smithay::{
    desktop::Window,
    reexports::calloop::{
        timer::{TimeoutAction, Timer},
        RegistrationToken,
    },
    utils::{IsAlive, Logical, Point},
};

use crate::{
    hotkeys::Hotkey,
    state::{window_state, Ede},
};

/// Cinnamon's `CORNER_ACTOR_SIZE`.
pub const CORNER_PX: f64 = 2.0;
/// Cinnamon's `HOT_CORNER_ACTIVATION_TIMEOUT`: a click this soon after the
/// hover ran the action runs nothing.
pub const ACTIVATION_TIMEOUT: Duration = Duration::from_millis(500);

/// One corner's `action:enabled:delay`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corner {
    pub action: String,
    pub enabled: bool,
    pub delay_ms: u64,
}

impl Corner {
    /// Cinnamon's parse: the command may hold colons, so the last two
    /// fields are the flag and the delay.
    pub fn parse(s: &str) -> Option<Corner> {
        let mut parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 {
            return None;
        }
        let (delay, enabled) = if parts.len() >= 3 { (parts.pop().unwrap(), parts.pop().unwrap()) } else { ("0", parts.pop().unwrap()) };
        Some(Corner { action: parts.join(":"), enabled: enabled == "true", delay_ms: delay.parse().unwrap_or(0) })
    }
}

/// The pointer's state against the corners.
#[derive(Debug, Default)]
pub struct HotCorners {
    /// The enabled corner the pointer is in.
    inside: Option<usize>,
    timer: Option<RegistrationToken>,
    hovered_at: Option<Instant>,
    /// A press the corner took: its release is the click.
    pressed: Option<u32>,
}

/// Which corner of `area` `pos` is in: 0 top-left, 1 top-right, 2
/// bottom-left, 3 bottom-right.
pub fn corner_at(area: smithay::utils::Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
    let (x0, y0) = (area.loc.x as f64, area.loc.y as f64);
    let (x1, y1) = (x0 + area.size.w as f64, y0 + area.size.h as f64);
    if pos.x < x0 || pos.y < y0 || pos.x >= x1 || pos.y >= y1 {
        return None;
    }
    let left = pos.x < x0 + CORNER_PX;
    let right = pos.x >= x1 - CORNER_PX;
    let top = pos.y < y0 + CORNER_PX;
    let bottom = pos.y >= y1 - CORNER_PX;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(0),
        (_, true, true, _) => Some(1),
        (true, _, _, true) => Some(2),
        (_, true, _, true) => Some(3),
        _ => None,
    }
}

impl Ede {
    /// The enabled corner under `pos`, if the corners are awake.
    fn hot_corner_at(&self, pos: Point<f64, Logical>) -> Option<usize> {
        let output = self.desk.space.outputs().next()?;
        let area = self.desk.space.output_geometry(output)?;
        let i = corner_at(area, pos)?;
        let corner = self.config.hot_corners.corner(i)?;
        if !corner.enabled || self.is_locked() {
            return None;
        }
        let fullscreen = self.desk.space.elements().any(|w| window_state(w).fullscreen.is_some() && self.desk.space.element_geometry(w).is_some_and(|g| g.overlaps(area)));
        (!fullscreen || self.config.hot_corners.fullscreen).then_some(i)
    }

    /// The person's pointer at `pos`: entering a corner arms its delay,
    /// leaving disarms it.
    pub fn hot_corners_motion(&mut self, pos: Point<f64, Logical>) {
        let now = self.hot_corner_at(pos);
        if now == self.corners.inside {
            return;
        }
        if let Some(t) = self.corners.timer.take() {
            self.loop_handle.remove(t);
        }
        self.corners.inside = now;
        let Some(i) = now else { return };
        let delay = self.config.hot_corners.corner(i).map(|c| c.delay_ms).unwrap_or(0);
        self.corners.timer = self
            .loop_handle
            .insert_source(Timer::from_duration(Duration::from_millis(delay)), move |_, _, ede| {
                ede.corners.timer = None;
                if ede.corners.inside == Some(i) {
                    ede.corners.hovered_at = Some(Instant::now());
                    ede.run_hot_corner(i);
                }
                TimeoutAction::Drop
            })
            .ok();
    }

    /// A press or release of the person's: `true` = the corner took it
    /// (Cinnamon's corner actor stops the event), the release running the
    /// action.
    pub fn hot_corners_button(&mut self, button: u32, pressed: bool) -> bool {
        if pressed {
            let pos = self.desk.seat.get_pointer().unwrap().current_location();
            if self.corners.pressed.is_none() && self.hot_corner_at(pos).is_some() {
                self.corners.pressed = Some(button);
                return true;
            }
            return false;
        }
        if self.corners.pressed != Some(button) {
            return false;
        }
        self.corners.pressed = None;
        let pos = self.desk.seat.get_pointer().unwrap().current_location();
        if let Some(i) = self.hot_corner_at(pos) {
            if let Some(t) = self.corners.timer.take() {
                self.loop_handle.remove(t);
            }
            if !self.corners.hovered_at.is_some_and(|at| at.elapsed() < ACTIVATION_TIMEOUT) {
                self.run_hot_corner(i);
            }
        }
        true
    }

    fn run_hot_corner(&mut self, i: usize) {
        let Some(corner) = self.config.hot_corners.corner(i) else { return };
        tracing::debug!(corner = i, action = %corner.action, "hot corner");
        match corner.action.as_str() {
            "expo" => self.hotkey(Hotkey::ExpoToggle),
            "scale" => self.hotkey(Hotkey::ScaleToggle),
            "desktop" => self.toggle_desktop(),
            "" => {}
            command => self.spawn_for_person(&["sh".to_string(), "-c".to_string(), command.to_string()]),
        }
    }

    /// Show the desktop, Muffin's `toggle_desktop` (Super+D, the panel's
    /// corner bar, a hot corner): every window of the active workspace goes
    /// to the taskbar; the next toggle brings those back, the one on top
    /// focused. Once the person raised a window in between, the desktop no
    /// longer shows and the toggle hides everything again, as Muffin does.
    pub fn toggle_desktop(&mut self) {
        let showing = !self.desktop_shown.is_empty() && self.desk.space.elements().next().is_none();
        if showing {
            let back: Vec<Window> = std::mem::take(&mut self.desktop_shown).into_iter().filter(IsAlive::alive).collect();
            for w in &back {
                self.restore_window(w);
            }
            let top = self.desk.space.elements().last().cloned();
            self.focus(crate::desk::DeskId::PERSON, top);
            return;
        }
        let shown: Vec<Window> = self.desk.space.elements().cloned().collect();
        for w in &shown {
            self.minimize_window(w);
        }
        self.desktop_shown = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::utils::Rectangle;

    #[test]
    fn parses_cinnamons_layout_strings() {
        assert_eq!(Corner::parse("expo:false:0"), Some(Corner { action: "expo".into(), enabled: false, delay_ms: 0 }));
        assert_eq!(Corner::parse("scale:true:250"), Some(Corner { action: "scale".into(), enabled: true, delay_ms: 250 }));
        assert_eq!(Corner::parse("xdg-open http://x:8080:true:0").unwrap().action, "xdg-open http://x:8080", "a command keeps its colons");
        assert_eq!(Corner::parse("desktop:true"), Some(Corner { action: "desktop".into(), enabled: true, delay_ms: 0 }));
        assert_eq!(Corner::parse("desktop"), None);
    }

    #[test]
    fn the_corners_are_two_pixels_of_the_output() {
        let area = Rectangle::new((0, 0).into(), (800, 600).into());
        let at = |x: f64, y: f64| corner_at(area, (x, y).into());
        assert_eq!(at(0.0, 0.0), Some(0));
        assert_eq!(at(1.9, 1.9), Some(0));
        assert_eq!(at(2.0, 0.0), None);
        assert_eq!(at(799.0, 0.0), Some(1));
        assert_eq!(at(0.0, 599.5), Some(2));
        assert_eq!(at(798.5, 598.5), Some(3));
        assert_eq!(at(400.0, 300.0), None);
        assert_eq!(at(800.0, 0.0), None, "off the output");
    }
}
