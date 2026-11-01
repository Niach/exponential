//! The smithay handler impls: one module per protocol family.

mod compositor;
mod dmabuf;
mod input_method;
mod layer_shell;
mod xdg_shell;
#[cfg(feature = "xwayland")]
mod xwayland;

use smithay::{
    delegate_data_device,
    input::{Seat, SeatHandler, SeatState},
    reexports::{
        wayland_protocols::xdg::xdg_output::zv1::server::{zxdg_output_manager_v1::ZxdgOutputManagerV1, zxdg_output_v1::ZxdgOutputV1},
        wayland_server::{
            delegate_dispatch, delegate_global_dispatch,
            protocol::{wl_keyboard::WlKeyboard, wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat, wl_surface::WlSurface, wl_touch::WlTouch},
            Client, DataInit, DisplayHandle, GlobalDispatch, New, Resource,
        },
    },
    wayland::{
        output::{OutputHandler, OutputManagerState, OutputUserData, WlOutputData, XdgOutputUserData},
        seat::{KeyboardUserData, PointerUserData, SeatGlobalData, SeatUserData, TouchUserData},
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

    fn cursor_image(&mut self, seat: &Seat<Self>, image: smithay::input::pointer::CursorImageStatus) {
        // An agent space's pointer is never drawn.
        if seat == &self.seat {
            self.cursor_status = image;
        }
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}
// `delegate_seat!` by hand: an agent space's seat is visible to its own
// clients only, the person's seat to everyone else (crate::spaces).
impl GlobalDispatch<WlSeat, SeatGlobalData<Ede>> for Ede {
    fn bind(state: &mut Self, dh: &DisplayHandle, client: &Client, resource: New<WlSeat>, data: &SeatGlobalData<Ede>, init: &mut DataInit<'_, Self>) {
        <SeatState<Ede> as GlobalDispatch<WlSeat, SeatGlobalData<Ede>, Ede>>::bind(state, dh, client, resource, data, init)
    }

    fn can_view(client: Client, data: &SeatGlobalData<Ede>) -> bool {
        crate::spaces::global_visible(&client, data)
    }
}
delegate_dispatch!(Ede: [WlSeat: SeatUserData<Ede>] => SeatState<Ede>);
delegate_dispatch!(Ede: [WlPointer: PointerUserData<Ede>] => SeatState<Ede>);
delegate_dispatch!(Ede: [WlKeyboard: KeyboardUserData<Ede>] => SeatState<Ede>);
delegate_dispatch!(Ede: [WlTouch: TouchUserData<Ede>] => SeatState<Ede>);

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

// `delegate_output!` by hand, filtered like the seats.
impl GlobalDispatch<WlOutput, WlOutputData> for Ede {
    fn bind(state: &mut Self, dh: &DisplayHandle, client: &Client, resource: New<WlOutput>, data: &WlOutputData, init: &mut DataInit<'_, Self>) {
        <OutputManagerState as GlobalDispatch<WlOutput, WlOutputData, Ede>>::bind(state, dh, client, resource, data, init)
    }

    fn can_view(client: Client, data: &WlOutputData) -> bool {
        crate::spaces::global_visible(&client, data)
    }
}
delegate_global_dispatch!(Ede: [ZxdgOutputManagerV1: ()] => OutputManagerState);
delegate_dispatch!(Ede: [WlOutput: OutputUserData] => OutputManagerState);
delegate_dispatch!(Ede: [ZxdgOutputV1: XdgOutputUserData] => OutputManagerState);
delegate_dispatch!(Ede: [ZxdgOutputManagerV1: ()] => OutputManagerState);
