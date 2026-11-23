//! #EDE-125: driven input by desk. The `drive_*` paths share their lock
//! and protected checks; below them the person's functions keep every
//! line, and a space has twins that know no hotkeys, no bindings, no
//! frames and no Alt+drag. `*_on(PERSON, ..)` is the old function.
//!
//! A twin tells the space's own seat it was active, so the space's
//! clients see their user at work, and never the person's idle clock: the
//! person's screen still locks while an agent types.

use smithay::{
    backend::input::{ButtonState, InputTime, KeyState},
    desktop::WindowSurfaceType,
    input::{
        keyboard::{FilterResult, Keycode},
        pointer::{ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::{desk::DeskId, state::Ede};

impl Ede {
    pub(crate) fn key_on(&mut self, desk: DeskId, keycode: Keycode, state: KeyState, time: u32, bindings: bool) {
        match desk {
            DeskId::PERSON => self.key(keycode, state, time, bindings),
            // A space has no bindings to fire.
            space => self.space_key(space, keycode, state, time),
        }
    }

    pub(crate) fn pointer_motion_on(&mut self, desk: DeskId, pos: Point<f64, Logical>, time: u32) {
        match desk {
            DeskId::PERSON => self.pointer_motion_to(pos, time),
            space => self.space_pointer_motion_to(space, pos, time),
        }
    }

    pub(crate) fn pointer_button_on(&mut self, desk: DeskId, button: u32, state: ButtonState, time: u32) {
        match desk {
            DeskId::PERSON => self.pointer_button(button, state, time),
            space => self.space_pointer_button(space, button, state, time),
        }
    }

    pub(crate) fn surface_under_on(&self, desk: DeskId, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        match desk {
            DeskId::PERSON => self.surface_under(pos),
            space => self.space_surface_under(space, pos),
        }
    }

    /// The space's seat did something: its clients' idle notifications
    /// resume and its clock runs fast.
    pub(crate) fn space_active(&mut self, id: DeskId) {
        if let Some(seat) = self.try_desk(id).map(|d| d.seat.clone()) {
            self.idle_notifier.notify_activity(&seat);
        }
        self.space_used(id);
    }

    fn space_key(&mut self, id: DeskId, keycode: Keycode, state: KeyState, time: u32) {
        let Some(keyboard) = self.try_desk(id).and_then(|d| d.seat.get_keyboard()) else { return };
        keyboard.input::<(), _>(self, keycode, state, SERIAL_COUNTER.next_serial(), InputTime::from_millis(time), |_, _, _| FilterResult::Forward);
        self.space_active(id);
    }

    /// The topmost surface of a window of space `id` under `pos`.
    fn space_surface_under(&self, id: DeskId, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (window, at) = self.try_desk(id)?.space.element_under(pos)?;
        window.surface_under(pos - at.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + at).to_f64()))
    }

    fn space_pointer_motion_to(&mut self, id: DeskId, pos: Point<f64, Logical>, time: u32) {
        let Some(size) = self.spaces.get(id).and_then(|s| s.output.current_mode()).map(|m| m.size) else { return };
        let Some(pointer) = self.try_desk(id).and_then(|d| d.seat.get_pointer()) else { return };
        let location = Point::from((pos.x.clamp(0.0, size.w as f64 - 1.0), pos.y.clamp(0.0, size.h as f64 - 1.0)));
        let under = self.space_surface_under(id, location);
        pointer.motion(self, under, &MotionEvent { location, serial: SERIAL_COUNTER.next_serial(), time: InputTime::from_millis(time) });
        pointer.frame(self);
        self.space_active(id);
    }

    /// A press gives the window under the pointer the space's keyboard,
    /// then the button goes to what the pointer is on.
    fn space_pointer_button(&mut self, id: DeskId, button: u32, state: ButtonState, time: u32) {
        let Some(desk) = self.try_desk(id) else { return };
        let pointer = desk.seat.get_pointer().unwrap();
        if state == ButtonState::Pressed && !pointer.is_grabbed() {
            if let Some(window) = desk.space.element_under(pointer.current_location()).map(|(w, _)| w.clone()) {
                self.space_focus(id, Some(window));
            }
        }
        pointer.button(self, &ButtonEvent { button, state, serial: SERIAL_COUNTER.next_serial(), time: InputTime::from_millis(time) });
        pointer.frame(self);
        self.space_active(id);
    }
}
