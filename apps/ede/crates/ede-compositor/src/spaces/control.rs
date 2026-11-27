//! #EDE-266: who drives an agent space ([`ede_protocol::Request::ControlSpace`],
//! the computer-use tray's Pause, Take over and Hand back).
//!
//! [`SpaceControl::Agent`] is the default: the space's session drives its
//! own seat. While it is anything else the space grant's driving ops
//! (input, focus, close, launch, clipboard set) are `conflict`; what only
//! looks (captures, lists, the clipboard's reads) still answers (L-10). The
//! switch away from the agent lets go of what it had down (L-01/L-02).
//!
//! [`SpaceControl::Person`] names a rect of one of the shell's layer
//! surfaces (the pinned view). The person's pointer over the space's
//! picture in that rect (fit = contain, the letterbox does nothing) drives
//! the space's pointer through the `*_on` twins; their own cursor keeps
//! showing and the layer under it sees nothing. What the pointer is over
//! comes from [`Ede::surface_under`], the list the person's frame draws
//! and hits from (L-08): a popup over the view, the lock, a fullscreen
//! window take it as always. A press in the picture makes the space
//! "keyed": the person's keys go to the space's keyboard after the
//! compositor's bindings and the shell's hotkeys had theirs. A press
//! anywhere else, the switch away, the view's layer going, the lock or the
//! close end it, and what the space still has down is let go. A space
//! never takes the hold or Esc (L-39): neither reaches this path.

use ede_protocol::{ErrorCode, Failure, SpaceControl, SpaceId, SpaceViewRect};
use smithay::{
    backend::input::{ButtonState, KeyState},
    desktop::{layer_map_for_output, WindowSurfaceType},
    input::{
        keyboard::Keycode,
        pointer::{AxisFrame, CursorImageStatus, MotionEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::{desk::DeskId, state::Ede};

/// The person's hand on a space ([`SpaceControl::Person`]).
#[derive(Debug, Default)]
pub struct PersonDrive {
    /// The last motion of the person's pointer went to a view's rect, not
    /// to the surface under it.
    pub(crate) routed: bool,
    /// Buttons the person pressed into a space: their releases go there,
    /// and the pointer follows that space while one is down (a drag).
    pub(crate) buttons: Vec<(DeskId, u32)>,
    /// The space the person's keys go to.
    pub(crate) keyed: Option<DeskId>,
    /// The keys they pressed into it, still down there.
    pub(crate) keys: Vec<Keycode>,
}

/// Where a picture of `size` sits in `rect` (x, y, width, height), fit =
/// contain: the largest scale that shows all of it, centred, the rest of
/// the rect letterbox. `(x, y, scale)`; `None` = nothing to show.
pub fn contain(rect: (f64, f64, f64, f64), size: (i32, i32)) -> Option<(f64, f64, f64)> {
    let (x, y, w, h) = rect;
    let (sw, sh) = (size.0 as f64, size.1 as f64);
    if w <= 0.0 || h <= 0.0 || sw <= 0.0 || sh <= 0.0 {
        return None;
    }
    let scale = (w / sw).min(h / sh);
    Some((x + (w - sw * scale) / 2.0, y + (h - sh * scale) / 2.0, scale))
}

/// `pos` in `rect` as a point of the space's output of `size`, fit =
/// contain: `None` outside the picture (the letterbox, or outside the rect).
pub fn map_contain(rect: (f64, f64, f64, f64), size: (i32, i32), pos: (f64, f64)) -> Option<(f64, f64)> {
    let (ox, oy, scale) = contain(rect, size)?;
    let (x, y) = ((pos.0 - ox) / scale, (pos.1 - oy) / scale);
    ((0.0..size.0 as f64).contains(&x) && (0.0..size.1 as f64).contains(&y)).then_some((x, y))
}

/// The same point unclamped: a drag that started in the picture follows
/// the pointer out of it (the space's own motion clamps it to its output).
fn map_unclamped(rect: (f64, f64, f64, f64), size: (i32, i32), pos: (f64, f64)) -> Option<(f64, f64)> {
    let (ox, oy, scale) = contain(rect, size)?;
    Some(((pos.0 - ox) / scale, (pos.1 - oy) / scale))
}

fn refused(control: SpaceControl) -> Failure {
    let message = match control {
        SpaceControl::Person => "the person has taken over this space; wait until they hand it back",
        _ => "the person paused computer use in this space; wait and try again",
    };
    Failure::new(ErrorCode::Conflict, message)
}

impl Ede {
    /// The space grant drives its space: `conflict` while its agent does not.
    pub(crate) fn space_driving_refused(&self, desk: DeskId) -> Result<(), Failure> {
        match self.spaces.get(desk).map(|s| s.control) {
            Some(control @ (SpaceControl::Paused | SpaceControl::Person)) => Err(refused(control)),
            _ => Ok(()),
        }
    }

    /// `control_space`: who drives `space` now.
    pub(crate) fn space_control(&mut self, space: &SpaceId, control: SpaceControl, view: Option<SpaceViewRect>) -> Result<(), Failure> {
        let Some(desk) = self.spaces.iter().find(|s| &s.id == space).map(|s| s.desk.id) else {
            return Err(Failure::new(ErrorCode::NotFound, "no such space"));
        };
        let view = match (control, view) {
            (SpaceControl::Person, None) => return Err(Failure::new(ErrorCode::Invalid, "taking over a space needs the view it is driven through")),
            (SpaceControl::Person, Some(v)) if v.width <= 0 || v.height <= 0 || v.namespace.is_empty() => return Err(Failure::new(ErrorCode::Invalid, "the view needs a namespace and a size")),
            (SpaceControl::Person, view) => view,
            (_, _) => None,
        };
        let from = self.spaces.get(desk).expect("found above").control;
        // What the agent had down is let go before anyone else drives (L-01/L-02).
        if from == SpaceControl::Agent && control != SpaceControl::Agent {
            self.release_driven_of(desk, None);
        }
        // The person's hand leaves: their keys and buttons in it let go.
        if from == SpaceControl::Person && control != SpaceControl::Person {
            self.space_person_end(desk);
        }
        let s = self.spaces.get_mut(desk).expect("found above");
        s.control = control;
        s.view = view;
        if from != control {
            tracing::info!(space = space.as_str(), from = ?from, to = ?control, "who drives the space");
        }
        Ok(())
    }

    /// The space under the person's pointer at `pos` through its view:
    /// `Some((desk, Some(point)))` on its picture, `Some((desk, None))` on
    /// the view's letterbox, `None` = not over a view.
    fn space_view_at(&self, pos: Point<f64, Logical>) -> Option<(DeskId, Option<Point<f64, Logical>>)> {
        let (desk, rect, size) = self.space_view_rect_at(pos)?;
        Some((desk, map_contain(rect, size, (pos.x, pos.y)).map(Point::from)))
    }

    /// The space whose view's rect is under `pos` (the surface the
    /// person's pointer hits there is that layer's own), its rect in
    /// global logical px and its output's size.
    fn space_view_rect_at(&self, pos: Point<f64, Logical>) -> Option<(DeskId, (f64, f64, f64, f64), (i32, i32))> {
        if self.spaces.iter().all(|s| s.control != SpaceControl::Person) {
            return None;
        }
        let (surface, _) = self.surface_under(pos)?;
        let output = self.desk.space.output_under(pos).next()?;
        let output_geo = self.desk.space.output_geometry(output)?;
        let map = layer_map_for_output(output);
        // The layer's own surface (or a subsurface of it), never a popup over it.
        let layer = map.layer_for_surface(&surface, WindowSurfaceType::TOPLEVEL | WindowSurfaceType::SUBSURFACE)?;
        if !crate::sealed::layer_origin(layer).shell {
            return None;
        }
        let at = map.layer_geometry(layer)?.loc + output_geo.loc;
        let namespace = layer.namespace().to_string();
        drop(map);
        self.spaces.iter().filter(|s| s.control == SpaceControl::Person).find_map(|s| {
            let view = s.view.as_ref().filter(|v| v.namespace == namespace)?;
            let rect = ((at.x + view.x) as f64, (at.y + view.y) as f64, view.width as f64, view.height as f64);
            let inside = pos.x >= rect.0 && pos.y >= rect.1 && pos.x < rect.0 + rect.2 && pos.y < rect.1 + rect.3;
            let size = s.output.current_mode().map(|m| (m.size.w, m.size.h))?;
            inside.then_some((s.desk.id, rect, size))
        })
    }

    /// The rect of `desk`'s view wherever its layer is mapped now, for a
    /// drag that left it; `None` = its layer is gone.
    fn space_view_rect_of(&self, desk: DeskId) -> Option<((f64, f64, f64, f64), (i32, i32))> {
        let space = self.spaces.get(desk).filter(|s| s.control == SpaceControl::Person)?;
        let view = space.view.as_ref()?;
        let size = space.output.current_mode().map(|m| (m.size.w, m.size.h))?;
        self.desk.space.outputs().find_map(|output| {
            let output_geo = self.desk.space.output_geometry(output)?;
            let map = layer_map_for_output(output);
            let layer = map.layers().find(|l| l.namespace() == view.namespace && crate::sealed::layer_origin(l).shell)?;
            let at = map.layer_geometry(layer)?.loc + output_geo.loc;
            Some((((at.x + view.x) as f64, (at.y + view.y) as f64, view.width as f64, view.height as f64), size))
        })
    }

    /// The person's pointer moved to `pos` (on the outputs): `true` = it
    /// went to a space through its view (the person's pointer moved
    /// along, focusing nothing), `false` = the person's own path takes it.
    pub(crate) fn space_view_motion(&mut self, pos: Point<f64, Logical>, time: u32) -> bool {
        let pointer = self.desk.seat.get_pointer().unwrap();
        if self.is_locked() {
            self.space_person_end_all();
            self.spaces.person.routed = false;
            return false;
        }
        // A drag pressed into a space follows the pointer there.
        let dragging = self.spaces.person.buttons.first().map(|(d, _)| *d);
        let target = match dragging {
            Some(desk) => match self.space_view_rect_of(desk) {
                Some((rect, size)) => Some((desk, map_unclamped(rect, size, (pos.x, pos.y)).map(Point::from))),
                None => {
                    self.space_person_end(desk);
                    None
                }
            },
            None if pointer.is_grabbed() => None,
            None => self.space_view_at(pos),
        };
        let Some((desk, at)) = target else {
            self.spaces.person.routed = false;
            return false;
        };
        if !self.spaces.person.routed {
            // The person's own cursor keeps showing over the picture.
            self.desk.cursor_status = CursorImageStatus::default_named();
        }
        self.spaces.person.routed = true;
        pointer.motion(self, None, &MotionEvent { location: pos, serial: SERIAL_COUNTER.next_serial(), time: smithay::backend::input::InputTime::from_millis(time) });
        pointer.frame(self);
        if let Some(at) = at {
            self.pointer_motion_on(desk, at, time);
        }
        true
    }

    /// Is the person's pointer routed to a space now? Then nothing of the
    /// person's (a refocus, a relative motion) reaches the layer under it.
    pub(crate) fn space_view_routed(&self) -> bool {
        self.spaces.person.routed
    }

    /// A button of the person's: `true` = a space's (pressed on its picture,
    /// or the release of one that was), or the letterbox's (nothing).
    pub(crate) fn space_view_button(&mut self, button: u32, state: ButtonState, time: u32) -> bool {
        if state == ButtonState::Released {
            let Some(at) = self.spaces.person.buttons.iter().position(|(_, b)| *b == button) else { return false };
            let (desk, _) = self.spaces.person.buttons.remove(at);
            if self.try_desk(desk).is_some() {
                self.pointer_button_on(desk, button, state, time);
            }
            return true;
        }
        let pointer = self.desk.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let target = if self.is_locked() || pointer.is_grabbed() { None } else { self.space_view_at(location) };
        let Some((desk, at)) = target else {
            // A press anywhere else ends the keys' way into a space.
            self.space_keyed_end();
            self.spaces.person.routed = false;
            return false;
        };
        let hit = self.surface_under(location).map(|(s, _)| s);
        self.hotkeys_pointer_pressed(hit.as_ref());
        let Some(at) = at else {
            self.space_keyed_end();
            return true;
        };
        if self.spaces.person.keyed != Some(desk) {
            self.space_keyed_end();
            self.spaces.person.keyed = Some(desk);
        }
        self.pointer_motion_on(desk, at, time);
        self.pointer_button_on(desk, button, state, time);
        self.spaces.person.buttons.retain(|(_, b)| *b != button);
        self.spaces.person.buttons.push((desk, button));
        true
    }

    /// The person's wheel: `true` = it scrolled a space (or the letterbox).
    pub fn space_view_axis(&mut self, frame: AxisFrame) -> bool {
        let pointer = self.desk.seat.get_pointer().unwrap();
        if self.is_locked() || pointer.is_grabbed() {
            return false;
        }
        let Some((desk, at)) = self.space_view_at(pointer.current_location()) else { return false };
        if at.is_some() {
            self.pointer_axis_on(desk, frame);
        }
        true
    }

    /// The space the person's keys go to now, if any.
    pub fn space_keyed(&self) -> Option<DeskId> {
        self.spaces.person.keyed
    }

    /// Does the person's key go to a keyed space? Asked by the person's
    /// key filter once bindings and hotkeys let it pass: a press while
    /// keyed, the release of a key pressed into the space.
    pub(crate) fn space_keyed_takes(&self, keycode: Keycode, state: KeyState) -> bool {
        let Some(desk) = self.spaces.person.keyed else { return false };
        if self.spaces.get(desk).is_none_or(|s| s.control != SpaceControl::Person) {
            return false;
        }
        match state {
            KeyState::Pressed => self.space_view_rect_of(desk).is_some(),
            KeyState::Released => self.spaces.person.keys.contains(&keycode),
        }
    }

    /// The person's key, taken for the keyed space.
    pub(crate) fn space_keyed_key(&mut self, keycode: Keycode, state: KeyState, time: u32) {
        let Some(desk) = self.spaces.person.keyed else { return };
        let keys = &mut self.spaces.person.keys;
        keys.retain(|k| *k != keycode);
        if state == KeyState::Pressed {
            keys.push(keycode);
        }
        self.key_on(desk, keycode, state, time, false);
    }

    /// The keys' way into a space ends: what the person has down there is let go.
    pub(crate) fn space_keyed_end(&mut self) {
        let Some(desk) = self.spaces.person.keyed.take() else { return };
        let keys = std::mem::take(&mut self.spaces.person.keys);
        if self.try_desk(desk).is_none() {
            return;
        }
        let time = crate::ipc::now_ms() as u32;
        for key in keys.into_iter().rev() {
            self.key_on(desk, key, KeyState::Released, time, false);
        }
    }

    /// The person's hand leaves `desk`: keys and buttons there let go.
    pub(crate) fn space_person_end(&mut self, desk: DeskId) {
        if self.spaces.person.keyed == Some(desk) {
            self.space_keyed_end();
        }
        let (buttons, kept) = std::mem::take(&mut self.spaces.person.buttons).into_iter().partition::<Vec<_>, _>(|(d, _)| *d == desk);
        self.spaces.person.buttons = kept;
        if self.try_desk(desk).is_some() {
            let time = crate::ipc::now_ms() as u32;
            for (_, button) in buttons {
                self.pointer_button_on(desk, button, ButtonState::Released, time);
            }
        }
    }

    fn space_person_end_all(&mut self) {
        let mut desks: Vec<DeskId> = self.spaces.person.buttons.iter().map(|(d, _)| *d).chain(self.spaces.person.keyed).collect();
        desks.dedup();
        for desk in desks {
            self.space_person_end(desk);
        }
    }

    /// A layer surface went: a keyed space whose view went with it is no
    /// longer keyed.
    pub(crate) fn space_view_layer_gone(&mut self) {
        let desks: Vec<DeskId> = self.spaces.person.buttons.iter().map(|(d, _)| *d).chain(self.spaces.person.keyed).collect();
        for desk in desks {
            if self.space_view_rect_of(desk).is_none() {
                self.space_person_end(desk);
            }
        }
        self.spaces.person.routed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{contain, map_contain};

    #[test]
    fn a_wider_rect_letterboxes_left_and_right() {
        // 1280x800 into 400x200: scale 0.25, 320x200 centred at x 40.
        let rect = (100.0, 50.0, 400.0, 200.0);
        assert_eq!(contain(rect, (1280, 800)), Some((140.0, 50.0, 0.25)));
        assert_eq!(map_contain(rect, (1280, 800), (140.0, 50.0)), Some((0.0, 0.0)));
        assert_eq!(map_contain(rect, (1280, 800), (300.0, 150.0)), Some((640.0, 400.0)));
        assert_eq!(map_contain(rect, (1280, 800), (459.0, 249.0)), Some((1276.0, 796.0)));
        assert_eq!(map_contain(rect, (1280, 800), (139.0, 150.0)), None, "the left letterbox");
        assert_eq!(map_contain(rect, (1280, 800), (460.0, 150.0)), None, "the right letterbox");
        assert_eq!(map_contain(rect, (1280, 800), (300.0, 250.0)), None, "below the rect");
    }

    #[test]
    fn a_taller_rect_letterboxes_top_and_bottom() {
        // 640x480 into 320x480: scale 0.5, 320x240 centred at y 120.
        let rect = (0.0, 0.0, 320.0, 480.0);
        assert_eq!(contain(rect, (640, 480)), Some((0.0, 120.0, 0.5)));
        assert_eq!(map_contain(rect, (640, 480), (160.0, 240.0)), Some((320.0, 240.0)));
        assert_eq!(map_contain(rect, (640, 480), (160.0, 119.0)), None);
        assert_eq!(map_contain(rect, (640, 480), (160.0, 360.0)), None, "the end is outside");
    }

    #[test]
    fn an_exact_fit_maps_one_to_one_scaled() {
        let rect = (10.0, 20.0, 640.0, 400.0);
        assert_eq!(contain(rect, (1280, 800)), Some((10.0, 20.0, 0.5)));
        assert_eq!(map_contain(rect, (1280, 800), (10.5, 20.5)), Some((1.0, 1.0)));
    }

    #[test]
    fn an_empty_rect_shows_nothing() {
        assert_eq!(contain((0.0, 0.0, 0.0, 100.0), (640, 480)), None);
        assert_eq!(map_contain((0.0, 0.0, 100.0, 100.0), (0, 0), (1.0, 1.0)), None);
    }
}
