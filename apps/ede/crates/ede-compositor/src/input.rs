//! Input, backend-agnostic: every backend turns its events into these calls,
//! and the headless tests call them directly.
//!
//! Compositor bindings (Mint-like): Alt+drag moves a window, Alt+F4 closes
//! it, Super+Left/Right tile to a half, Super+Up maximizes, Super+Down
//! restores, Super+L locks, Super+Shift+E ends the session, Ctrl+Alt+F1…F12
//! switch VTs. While locked only VT switching stays.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
        PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{keysyms, FilterResult, Keycode, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, Focus, GrabStartData, MotionEvent, RelativeMotionEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::{
    grabs::{MoveSurfaceGrab, BTN_LEFT},
    layout::Tile,
    state::{window_state, Ede},
};

/// What a key press did when the compositor kept it for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Super+Shift+E: end the session (ede-session logs out on a clean exit).
    Quit,
    /// Ctrl+Alt+F1…F12 (xkb's XF86Switch_VT_n).
    SwitchVt(i32),
    Close,
    Tile(Tile),
    Restore,
    /// Super+L: start the lock client.
    Lock,
}

impl Ede {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { .. }
            | InputEvent::PointerMotion { .. }
            | InputEvent::PointerMotionAbsolute { .. }
            | InputEvent::PointerButton { .. }
            | InputEvent::PointerAxis { .. } => self.note_activity(),
            _ => {}
        }
        match event {
            InputEvent::Keyboard { event, .. } => self.keyboard_key(event.key_code(), event.state(), event.time_msec()),
            InputEvent::PointerMotion { event, .. } => {
                let pos = self.seat.get_pointer().unwrap().current_location() + event.delta();
                self.pointer_motion_to(pos, event.time_msec());
                let pointer = self.seat.get_pointer().unwrap();
                let under = self.surface_under(pointer.current_location());
                pointer.relative_motion(
                    self,
                    under,
                    &RelativeMotionEvent { delta: event.delta(), delta_unaccel: event.delta_unaccel(), utime: event.time() },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.space.outputs().next().cloned() else { return };
                let Some(geo) = self.space.output_geometry(&output) else { return };
                let pos = event.position_transformed(geo.size) + geo.loc.to_f64();
                self.pointer_motion_to(pos, event.time_msec());
            }
            InputEvent::PointerButton { event, .. } => self.pointer_button(event.button_code(), event.state(), event.time_msec()),
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                let amount = |axis| event.amount(axis).unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * 15.0 / 120.);
                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                for axis in [Axis::Horizontal, Axis::Vertical] {
                    let value = amount(axis);
                    if value != 0.0 {
                        frame = frame.value(axis, value);
                        if let Some(discrete) = event.amount_v120(axis) {
                            frame = frame.v120(axis, discrete as i32);
                        }
                    }
                    if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                        frame = frame.stop(axis);
                    }
                }
                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    /// Moves the pointer to `pos` (clamped onto the outputs) and updates focus.
    pub fn pointer_motion_to(&mut self, pos: Point<f64, Logical>, time: u32) {
        let pos = self.clamp_to_outputs(pos);
        let pointer = self.seat.get_pointer().unwrap();
        let under = self.surface_under(pos);
        pointer.motion(self, under, &MotionEvent { location: pos, serial: SERIAL_COUNTER.next_serial(), time });
        pointer.frame(self);
    }

    pub fn pointer_button(&mut self, button: u32, state: ButtonState, time: u32) {
        let pointer = self.seat.get_pointer().unwrap();
        let serial = SERIAL_COUNTER.next_serial();

        if state == ButtonState::Pressed && self.is_locked() {
            self.lock_pointer_focus();
        } else if state == ButtonState::Pressed && !pointer.is_grabbed() {
            let location = pointer.current_location();
            let window = self.space.element_under(location).map(|(w, _)| w.clone());
            self.focus(window.clone());
            let alt = self.seat.get_keyboard().unwrap().modifier_state().alt;
            if let (Some(window), true, BTN_LEFT) = (window, alt, button) {
                // Alt+drag: move from anywhere in the window.
                if let Some(loc) = self.space.element_location(&window) {
                    let start = GrabStartData { focus: None, button, location };
                    pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), serial, Focus::Clear);
                }
            }
        }

        pointer.button(self, &ButtonEvent { button, state, serial, time });
        pointer.frame(self);
    }

    /// A key from the person's keyboard.
    pub fn keyboard_key(&mut self, keycode: Keycode, state: KeyState, time: u32) {
        self.note_activity();
        self.key(keycode, state, time, true);
    }

    pub(crate) fn key(&mut self, keycode: Keycode, state: KeyState, time: u32, bindings: bool) {
        self.release_ime_grab_if_protected();
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let locked = self.is_locked();
        let action = keyboard.input::<Action, _>(self, keycode, state, serial, time, |_, mods, handle| {
            if state != KeyState::Pressed || !bindings {
                return FilterResult::Forward;
            }
            match binding(mods, handle.modified_sym().raw()) {
                Some(action @ Action::SwitchVt(_)) => FilterResult::Intercept(action),
                Some(_) if locked => FilterResult::Forward,
                Some(action) => FilterResult::Intercept(action),
                None => FilterResult::Forward,
            }
        });
        if let Some(action) = action {
            self.run(action);
        }
    }

    pub fn run(&mut self, action: Action) {
        match action {
            Action::Quit => self.loop_signal.stop(),
            Action::SwitchVt(vt) => {
                if let Some(switch) = self.vt_switch.as_mut() {
                    switch(vt);
                }
            }
            Action::Lock => self.request_lock(),
            Action::Close => self.close_focused(),
            Action::Tile(tile) => {
                if let Some(w) = self.focused_window() {
                    self.tile(&w, tile);
                }
            }
            Action::Restore => {
                if let Some(w) = self.focused_window() {
                    if window_state(&w).tile.is_some() {
                        self.untile(&w, None);
                    }
                }
            }
        }
    }

    fn clamp_to_outputs(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.space.output_under(pos).next().is_some() {
            return pos;
        }
        // Off every output: pull it to the nearest point of the nearest one.
        self.space
            .outputs()
            .filter_map(|o| self.space.output_geometry(o))
            .map(|g| {
                let x = pos.x.clamp(g.loc.x as f64, (g.loc.x + g.size.w) as f64 - 1.0);
                let y = pos.y.clamp(g.loc.y as f64, (g.loc.y + g.size.h) as f64 - 1.0);
                Point::from((x, y))
            })
            .min_by(|a, b| {
                let d = |p: &Point<f64, Logical>| (p.x - pos.x).powi(2) + (p.y - pos.y).powi(2);
                d(a).total_cmp(&d(b))
            })
            .unwrap_or(pos)
    }
}

fn binding(mods: &ModifiersState, sym: u32) -> Option<Action> {
    if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
        return Some(Action::SwitchVt((sym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32));
    }
    match (mods.alt, mods.logo, mods.ctrl, mods.shift, sym) {
        (false, true, false, true, keysyms::KEY_E) => Some(Action::Quit),
        (true, false, false, false, keysyms::KEY_F4) => Some(Action::Close),
        (false, true, false, false, keysyms::KEY_l) => Some(Action::Lock),
        (false, true, false, false, keysyms::KEY_Left) => Some(Action::Tile(Tile::Left)),
        (false, true, false, false, keysyms::KEY_Right) => Some(Action::Tile(Tile::Right)),
        (false, true, false, false, keysyms::KEY_Up) => Some(Action::Tile(Tile::Maximized)),
        (false, true, false, false, keysyms::KEY_Down) => Some(Action::Restore),
        _ => None,
    }
}
