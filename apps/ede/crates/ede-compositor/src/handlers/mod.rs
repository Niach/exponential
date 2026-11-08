//! The smithay handler impls: one module per protocol family.

mod compositor;
mod decoration;
mod dmabuf;
mod input_method;
mod layer_shell;
mod xdg_shell;
#[cfg(feature = "xwayland")]
mod xwayland;

use smithay::{
    input::{
        dnd::{DnDGrab, DndGrabHandler, GrabType, Source},
        pointer::Focus,
        Seat, SeatHandler, SeatState,
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::Serial,
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{set_data_device_focus, DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler},
            SelectionHandler,
        },
    },
};

use crate::state::Ede;

impl SeatHandler for Ede {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Ede> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: smithay::input::pointer::CursorImageStatus) {
        self.cursor_status = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
        self.keep_text_input_off_protected(focused);
    }
}

impl SelectionHandler for Ede {
    type SelectionUserData = ();
}

impl DataDeviceHandler for Ede {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for Ede {}

// A client's drag runs as smithay's DnD grab on the device that started it.
impl WaylandDndGrabHandler for Ede {
    fn dnd_requested<S: Source>(&mut self, source: S, _icon: Option<WlSurface>, seat: Seat<Self>, serial: Serial, type_: GrabType) {
        match type_ {
            GrabType::Pointer => {
                let pointer = seat.get_pointer().unwrap();
                let Some(start) = pointer.grab_start_data() else { return source.cancel() };
                pointer.set_grab(self, DnDGrab::new_pointer(&self.display_handle, start, source, seat), serial, Focus::Keep);
            }
            GrabType::Touch => match seat.get_touch() {
                Some(touch) => {
                    let Some(start) = touch.grab_start_data() else { return source.cancel() };
                    touch.set_grab(self, DnDGrab::new_touch(&self.display_handle, start, source, seat), serial);
                }
                None => source.cancel(),
            },
        }
    }
}

impl OutputHandler for Ede {}

impl smithay::wayland::pointer_constraints::PointerConstraintsHandler for Ede {}

smithay::delegate_dispatch2!(Ede);
