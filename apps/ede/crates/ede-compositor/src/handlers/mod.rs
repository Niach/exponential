//! The smithay handler impls: one module per protocol family.

mod compositor;
mod dmabuf;
mod input_method;
mod layer_shell;
mod xdg_shell;
#[cfg(feature = "xwayland")]
mod xwayland;

use smithay::{
    delegate_data_device, delegate_output, delegate_seat,
    input::{Seat, SeatHandler, SeatState},
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler},
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
    }
}
delegate_seat!(Ede);

impl SelectionHandler for Ede {
    type SelectionUserData = ();
}

impl DataDeviceHandler for Ede {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for Ede {}
impl ServerDndGrabHandler for Ede {}
delegate_data_device!(Ede);

impl OutputHandler for Ede {}
delegate_output!(Ede);
