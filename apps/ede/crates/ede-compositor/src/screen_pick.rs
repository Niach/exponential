//! The person picks a part of their screen (`ede-protocol`'s
//! `pick_region`): Cinnamon's area screenshot (gnome-screenshot's pick), the
//! Screenshot portal's `interactive` mode and its PickColor.
//!
//! A crosshair; `area`: the left button drags a rubber band and its release
//! answers the rectangle, a click without a drag answers the window under
//! the pointer (its frame, our title bar included) or, on bare desktop, the
//! output; `point`: a click answers that pixel. Escape or the right button
//! cancel (`null`). Everything the person does meanwhile is the pick's: a
//! pointer and a keyboard grab keep every button and key from the clients.
//!
//! The band is drawn on the person's own monitors only
//! ([`crate::backend::output_elements`]), never in what a capture sees, so a
//! screenshot of the picked area never has the band in it. No dimming: the
//! gnome-screenshot look, a translucent fill with a one-pixel border.
//!
//! The grabs' callbacks run with smithay's pointer or keyboard lock held:
//! the answer goes out from an idle callback ([`Ede::pick_finish`]), where
//! both devices may be touched again and the caller's next calls served.

use std::collections::HashSet;

use ede_protocol::{ErrorCode, Failure, PickMode, PickedRegion};
use smithay::{
    backend::input::KeyState,
    backend::renderer::{
        element::{solid::SolidColorRenderElement, Id, Kind},
        utils::CommitCounter,
    },
    input::{
        keyboard::{GrabStartData as KeyboardGrabStartData, KeyboardGrab, KeyboardInnerHandle, Keycode, ModifiersState},
        pointer::{
            AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, Focus, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
            GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent,
            PointerGrab, PointerInnerHandle, RelativeMotionEvent,
        },
    },
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Physical, Point, Rectangle, Serial, SERIAL_COUNTER},
};

use crate::{grabs::BTN_LEFT, state::Ede};

/// linux/input-event-codes.h
const BTN_RIGHT: u32 = 0x111;
/// Escape's evdev code + 8 (the xkb keycode), whatever the layout makes of it.
const ESC_KEYCODE: u32 = 1 + 8;
/// Less movement than this (logical px) between press and release is a
/// click, not a drag (gnome-screenshot's threshold).
pub const CLICK_SLOP: f64 = 4.0;
/// The band's colour: the theme's selection (`primary`, ede-theme's
/// `theme_selected_bg_color`, #e5e5e5 on the zinc palette), sRGB.
const SELECTION: [f32; 3] = [229.0 / 255.0, 229.0 / 255.0, 229.0 / 255.0];
/// The fill's opacity; the border is opaque.
const FILL_ALPHA: f32 = 0.25;

/// A pick in progress (one at a time, the person's desk only).
pub struct Pick {
    /// The connection and call that asked; the answer goes there.
    pub conn: u64,
    call: u64,
    pub mode: PickMode,
    /// Where the left button went down (global logical); `None` before.
    anchor: Option<Point<f64, Logical>>,
    /// The pointer now.
    at: Point<f64, Logical>,
    /// The answer, once the person gave it (`Some(None)` = cancelled); it
    /// goes out when the button or key that gave it is up again, so its
    /// release never reaches a client.
    decided: Option<Option<PickedRegion>>,
    /// The fill and the four border lines, stable across frames so the
    /// damage tracker redraws only where the band moved.
    ids: [Id; 5],
}

impl Pick {
    /// Is the band being dragged (drawn) now?
    pub fn dragging(&self) -> bool {
        self.mode == PickMode::Area && self.anchor.is_some() && self.decided.is_none()
    }
}

impl Ede {
    /// Starts a pick for call `call` of connection `conn`; its answer comes
    /// later ([`Ede::ipc_reply_later`]). The caller checked it is the
    /// person's desk and a person's grant.
    pub(crate) fn pick_region_start(&mut self, conn: u64, call: u64, mode: PickMode) -> Result<(), Failure> {
        if self.is_locked() {
            return Err(Failure::new(ErrorCode::Forbidden, "the session is locked"));
        }
        if self.screen_pick.is_some() {
            return Err(Failure::new(ErrorCode::Conflict, "another pick is running"));
        }
        let pointer = self.desk.seat.get_pointer().unwrap();
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        let at = pointer.current_location();
        self.screen_pick = Some(Pick { conn, call, mode, anchor: None, at, decided: None, ids: std::array::from_fn(|_| Id::new()) });
        let serial = SERIAL_COUNTER.next_serial();
        // The clients lose the pointer (a leave): none sets the cursor
        // while the crosshair shows, none sees a button.
        let start = PointerGrabStartData { focus: None, button: BTN_LEFT, location: at };
        pointer.set_grab(self, PickPointer { start }, serial, Focus::Clear);
        // An input method's keyboard grab gives way; it grabs again with
        // the next text field it serves.
        let start = KeyboardGrabStartData { focus: keyboard.current_focus() };
        keyboard.set_grab(self, PickKeyboard { start, down: HashSet::new() }, serial);
        self.desk.cursor_status = CursorImageStatus::Named(CursorIcon::Crosshair);
        Ok(())
    }

    /// Cancels the pick now (its caller's connection went): never from
    /// inside a grab's callback.
    pub(crate) fn pick_cancel(&mut self) {
        if let Some(pick) = self.screen_pick.as_mut() {
            pick.decided = Some(None);
        }
        self.pick_finish();
    }

    /// The pick is over: its grabs go (the one a callback already let go
    /// of aside), the pointer finds what is under it again (that client
    /// sets its own cursor back) and the caller gets the answer.
    pub(crate) fn pick_finish(&mut self) {
        let Some(pick) = self.screen_pick.take() else { return };
        let time = self.start_time.elapsed().as_millis() as u32;
        let pointer = self.desk.seat.get_pointer().unwrap();
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        self.desk.cursor_status = CursorImageStatus::default_named();
        if keyboard.with_grab(|_, g| g.is::<PickKeyboard>()).unwrap_or(false) {
            keyboard.unset_grab(self);
        }
        if pointer.with_grab(|_, g| g.is::<PickPointer>()).unwrap_or(false) {
            pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), smithay::backend::input::InputTime::from_millis(time));
        }
        if !self.is_locked() {
            self.pointer_motion_to(pointer.current_location(), time);
        }
        let answer = pick.decided.flatten();
        self.ipc_reply_later(pick.conn, pick.call, Ok(serde_json::to_value(answer).unwrap()));
    }

    /// A grab's callback let go of its own device: the rest of the ending
    /// waits for the loop's next idle moment.
    fn pick_end_later(&mut self) {
        self.loop_handle.insert_idle(|state| state.pick_finish());
    }

    /// The left button went up at `at` after going down at `anchor`.
    fn pick_area(&self, anchor: Point<f64, Logical>, at: Point<f64, Logical>) -> Option<PickedRegion> {
        let bounds = self.pick_bounds()?;
        if (at.x - anchor.x).hypot(at.y - anchor.y) < CLICK_SLOP {
            if let Some((rect, window)) = self.pick_window_at(at) {
                let rect = rect.intersection(bounds)?;
                return Some(PickedRegion { x: rect.loc.x, y: rect.loc.y, width: rect.size.w, height: rect.size.h, window: Some(window) });
            }
            let output = self.desk.space.output_under(at).next().and_then(|o| self.desk.space.output_geometry(o))?;
            return Some(PickedRegion { x: output.loc.x, y: output.loc.y, width: output.size.w, height: output.size.h, window: None });
        }
        let rect = band(anchor, at).intersection(bounds)?;
        Some(PickedRegion { x: rect.loc.x, y: rect.loc.y, width: rect.size.w, height: rect.size.h, window: None })
    }

    /// The topmost window under `at` with its frame, our title bar
    /// included: what the person sees as the window. A shell layer above
    /// the windows there (the panel) is no window: bare desktop.
    fn pick_window_at(&self, at: Point<f64, Logical>) -> Option<(Rectangle<i32, Logical>, ede_protocol::WindowId)> {
        if self.layer_above_windows(at) {
            return None;
        }
        self.desk
            .space
            .elements()
            .rev()
            .filter(|w| !crate::ipc::x11_override_redirect(w))
            .find_map(|w| self.framed_geometry(w).filter(|f| f.to_f64().contains(at)).map(|f| (f, crate::ipc::window_id(w))))
    }

    /// The outputs' bounds (they touch: their union).
    fn pick_bounds(&self) -> Option<Rectangle<i32, Logical>> {
        self.desk.space.outputs().filter_map(|o| self.desk.space.output_geometry(o)).reduce(|a, b| a.merge(b))
    }

    /// The band on `output`, front to back (border, then fill), in that
    /// output's physical pixels at its own scale and place: the person's
    /// monitors only, never a capture's scene.
    pub fn screen_pick_elements(&self, output: &Output) -> Vec<SolidColorRenderElement> {
        let Some(pick) = self.screen_pick.as_ref().filter(|p| p.dragging()) else { return vec![] };
        if self.is_locked() {
            return vec![];
        }
        let Some(out) = self.desk.space.output_geometry(output) else { return vec![] };
        let Some(anchor) = pick.anchor else { return vec![] };
        let scale = output.current_scale().fractional_scale();
        let rect = band(anchor, pick.at);
        if !rect.overlaps(out) {
            return vec![];
        }
        // Edges converted one by one (rounded), so neighbouring outputs
        // and fractional scales agree on where a logical edge falls.
        let edge = |v: i32, origin: i32| ((f64::from(v - origin)) * scale).round() as i32;
        let (x0, y0) = (edge(rect.loc.x, out.loc.x), edge(rect.loc.y, out.loc.y));
        let (x1, y1) = (edge(rect.loc.x + rect.size.w, out.loc.x), edge(rect.loc.y + rect.size.h, out.loc.y));
        let (w, h) = ((x1 - x0).max(1), (y1 - y0).max(1));
        let [r, g, b] = SELECTION;
        let line = [r, g, b, 1.0];
        // Premultiplied, as smithay blends a solid colour.
        let fill = [r * FILL_ALPHA, g * FILL_ALPHA, b * FILL_ALPHA, FILL_ALPHA];
        let solid = |id: &Id, (x, y, w, h): (i32, i32, i32, i32), colour: [f32; 4]| {
            let geo: Rectangle<i32, Physical> = Rectangle::new((x, y).into(), (w.max(0), h.max(0)).into());
            SolidColorRenderElement::new(id.clone(), geo, CommitCounter::default(), colour, Kind::Unspecified)
        };
        let mut elements = vec![
            solid(&pick.ids[0], (x0, y0, w, 1), line),
            solid(&pick.ids[1], (x0, y0 + h - 1, w, 1), line),
            solid(&pick.ids[2], (x0, y0, 1, h), line),
            solid(&pick.ids[3], (x0 + w - 1, y0, 1, h), line),
        ];
        if w > 2 && h > 2 {
            elements.push(solid(&pick.ids[4], (x0 + 1, y0 + 1, w - 2, h - 2), fill));
        }
        elements
    }
}

/// The normalized rectangle between two pointer positions: whole logical
/// pixels covering both (at least 1x1).
fn band(a: Point<f64, Logical>, b: Point<f64, Logical>) -> Rectangle<i32, Logical> {
    let (x0, y0) = (a.x.min(b.x).floor() as i32, a.y.min(b.y).floor() as i32);
    let (x1, y1) = (a.x.max(b.x).ceil() as i32, a.y.max(b.y).ceil() as i32);
    Rectangle::new((x0, y0).into(), ((x1 - x0).max(1), (y1 - y0).max(1)).into())
}

/// The pointer while a pick runs: no client has it, no button reaches one.
pub struct PickPointer {
    start: PointerGrabStartData<Ede>,
}

impl PickPointer {
    /// Lets go of the pointer here (its lock is held), the rest later.
    fn end(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>, serial: Serial, time: smithay::backend::input::InputTime) {
        handle.unset_grab(self, data, serial, time, true);
        data.pick_end_later();
    }
}

impl PointerGrab<Ede> for PickPointer {
    fn motion(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>, focus: Option<(WlSurface, Point<f64, Logical>)>, event: &MotionEvent) {
        // The session locked meanwhile: the lock screen takes the pointer.
        if data.is_locked() {
            if let Some(pick) = data.screen_pick.as_mut() {
                pick.decided = Some(None);
            }
            self.end(data, handle, event.serial, event.time);
            handle.motion(data, focus, event);
            return;
        }
        handle.motion(data, None, event);
        if let Some(pick) = data.screen_pick.as_mut() {
            pick.at = event.location;
        }
    }

    fn relative_motion(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>, _focus: Option<(WlSurface, Point<f64, Logical>)>, event: &RelativeMotionEvent) {
        handle.relative_motion(data, None, event);
    }

    fn button(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>, event: &ButtonEvent) {
        use smithay::backend::input::ButtonState;
        let at = handle.current_location();
        let locked = data.is_locked();
        let Some((mode, anchor, open)) = data.screen_pick.as_ref().map(|p| (p.mode, p.anchor, p.decided.is_none())) else { return };
        let mut pressed_at = None;
        let decided = match (event.state, event.button, mode) {
            _ if locked => Some(None),
            _ if !open => None,
            (ButtonState::Pressed, BTN_RIGHT, _) => Some(None),
            (ButtonState::Pressed, BTN_LEFT, PickMode::Point) => {
                pressed_at = Some(at);
                let (x, y) = (at.x.floor() as i32, at.y.floor() as i32);
                Some(Some(PickedRegion { x, y, width: 1, height: 1, window: None }))
            }
            (ButtonState::Pressed, BTN_LEFT, PickMode::Area) => {
                pressed_at = Some(at);
                None
            }
            (ButtonState::Released, BTN_LEFT, PickMode::Area) => anchor.map(|anchor| data.pick_area(anchor, at)),
            _ => None,
        };
        if let Some(pick) = data.screen_pick.as_mut() {
            if let Some(at) = pressed_at {
                pick.anchor = Some(at);
                pick.at = at;
            }
            if decided.is_some() {
                pick.decided = decided;
            }
        }
        // Answered once every button is up: none of their releases reaches
        // the window under the pointer afterwards.
        let decided = data.screen_pick.as_ref().is_some_and(|p| p.decided.is_some());
        if decided && handle.current_pressed().is_empty() {
            self.end(data, handle, event.serial, event.time);
        }
    }

    fn axis(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _details: AxisFrame) {}

    fn frame(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GestureSwipeBeginEvent) {}
    fn gesture_swipe_update(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GestureSwipeUpdateEvent) {}
    fn gesture_swipe_end(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GestureSwipeEndEvent) {}
    fn gesture_pinch_begin(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GesturePinchBeginEvent) {}
    fn gesture_pinch_update(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GesturePinchUpdateEvent) {}
    fn gesture_pinch_end(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GesturePinchEndEvent) {}
    fn gesture_hold_begin(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GestureHoldBeginEvent) {}
    fn gesture_hold_end(&mut self, _data: &mut Ede, _handle: &mut PointerInnerHandle<'_, Ede>, _event: &GestureHoldEndEvent) {}

    fn start_data(&self) -> &PointerGrabStartData<Ede> {
        &self.start
    }

    fn unset(&mut self, _data: &mut Ede) {}
}

/// The keyboard while a pick runs: Escape cancels, no key reaches a client.
/// A key that was down before the pick (the Shift of Shift+Print) still
/// has its release delivered, so no client keeps it held.
pub struct PickKeyboard {
    start: KeyboardGrabStartData<Ede>,
    /// The keys pressed during the pick: theirs and their releases are the pick's.
    down: HashSet<Keycode>,
}

impl KeyboardGrab<Ede> for PickKeyboard {
    fn input(
        &mut self,
        data: &mut Ede,
        handle: &mut KeyboardInnerHandle<'_, Ede>,
        keycode: Keycode,
        state: KeyState,
        modifiers: Option<ModifiersState>,
        serial: Serial,
        time: smithay::backend::input::InputTime,
    ) {
        // The session locked meanwhile: the lock screen takes the keys.
        if data.is_locked() {
            if let Some(pick) = data.screen_pick.as_mut() {
                pick.decided = Some(None);
            }
            handle.unset_grab(self, data, serial, false);
            data.pick_end_later();
            handle.input(data, keycode, state, modifiers, serial, time);
            return;
        }
        match state {
            KeyState::Pressed => {
                self.down.insert(keycode);
                // By its key, not its symbol: xkb's state is the keyboard's,
                // whose lock this callback runs under.
                let escape = keycode.raw() == ESC_KEYCODE;
                if let Some(pick) = data.screen_pick.as_mut().filter(|p| p.decided.is_none() && escape) {
                    pick.decided = Some(None);
                }
            }
            KeyState::Released if !self.down.remove(&keycode) => {
                // Down before the pick: its release is the client's.
                handle.input(data, keycode, state, modifiers, serial, time);
            }
            KeyState::Released => {}
        }
        let decided = data.screen_pick.as_ref().is_some_and(|p| p.decided.is_some());
        if decided && self.down.is_empty() {
            handle.unset_grab(self, data, serial, false);
            data.pick_end_later();
        }
    }

    fn set_focus(&mut self, data: &mut Ede, handle: &mut KeyboardInnerHandle<'_, Ede>, focus: Option<WlSurface>, serial: Serial) {
        // Focus still moves (a window that maps, the lock screen); only the
        // keys are the pick's.
        handle.set_focus(data, focus, serial);
    }

    fn start_data(&self) -> &KeyboardGrabStartData<Ede> {
        &self.start
    }

    fn unset(&mut self, _data: &mut Ede) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_is_normalized_and_covers_both_ends() {
        let r = band((300.5, 250.0).into(), (100.0, 50.2).into());
        assert_eq!((r.loc.x, r.loc.y, r.size.w, r.size.h), (100, 50, 201, 200));
        let dot = band((7.3, 9.9).into(), (7.3, 9.9).into());
        assert_eq!((dot.size.w, dot.size.h), (1, 1));
    }
}
