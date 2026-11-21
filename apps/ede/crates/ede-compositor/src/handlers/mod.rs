//! The smithay handler impls: one module per protocol family.

mod compositor;
mod decoration;
mod dmabuf;
mod input_method;
pub(crate) mod layer_shell;
mod xdg_shell;
#[cfg(feature = "xwayland")]
pub(crate) mod xwayland;

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
            SelectionHandler, SelectionSource, SelectionTarget,
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
        if let Some(id) = crate::spaces::space_of_seat(seat) {
            return self.space_cursor_image(id, image);
        }
        self.desk.cursor_status = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
        // A space's seat: the person's text input and X11 focus stay; the
        // space's own X11 focus follows its keyboard (#EDE-138).
        if let Some(_desk) = crate::spaces::space_of_seat(seat) {
            #[cfg(feature = "xwayland")]
            if self.try_desk(_desk).is_some_and(|d| d.seat == *seat) {
                self.space_sync_x11_focus(_desk, focused);
            }
            return;
        }
        self.keep_text_input_off_protected(focused);
        #[cfg(feature = "xwayland")]
        self.sync_x11_focus(focused);
    }
}

impl SelectionHandler for Ede {
    /// A compositor-side selection: Xwayland's or ours ([`crate::clipboard`]).
    type SelectionUserData = crate::clipboard::Selection;

    /// A Wayland client set (or cleared) the clipboard: the serial moves
    /// and Xwayland's CLIPBOARD follows.
    fn new_selection(&mut self, ty: SelectionTarget, source: Option<SelectionSource>, seat: Seat<Self>) {
        if ty != SelectionTarget::Clipboard {
            return;
        }
        let mimes = source.map(|s| s.mime_types());
        // #EDE-126: a space's copy is the space's: the person's serial,
        // owner and Xwayland stay (nobody's once its space is gone).
        // #EDE-138: the space's own Xwayland follows it.
        if let Some(id) = crate::spaces::space_of_seat(&seat) {
            if let Some(desk) = self.try_desk_mut(id) {
                desk.clipboard.changed(mimes.clone().map_or(crate::clipboard::Owner::Empty, crate::clipboard::Owner::Client));
                self.xwm_selection(id, mimes);
            }
            return;
        }
        self.desk.clipboard.changed(mimes.clone().map_or(crate::clipboard::Owner::Empty, crate::clipboard::Owner::Client));
        self.xwm_selection(crate::desk::DeskId::PERSON, mimes);
    }

    /// A Wayland client reads a compositor-side selection.
    fn send_selection(&mut self, ty: SelectionTarget, mime_type: String, fd: std::os::fd::OwnedFd, seat: Seat<Self>, user_data: &Self::SelectionUserData) {
        if ty != SelectionTarget::Clipboard {
            return;
        }
        let space = crate::spaces::space_of_seat(&seat);
        match user_data {
            crate::clipboard::Selection::Ours(owned) => self.serve_owned(space.unwrap_or(crate::desk::DeskId::PERSON), owned, &mime_type, fd),
            // #EDE-138: the seat's desk's own Xwayland holds it (a space's
            // seat a space's, gone with a closed space).
            #[cfg(feature = "xwayland")]
            crate::clipboard::Selection::X11 => {
                if let Some(xwm) = self.desk_xwm(space.unwrap_or(crate::desk::DeskId::PERSON)) {
                    if let Err(e) = xwm.send_selection(ty, mime_type, fd) {
                        tracing::warn!("clipboard from X11: {e:?}");
                    }
                }
            }
            #[cfg(not(feature = "xwayland"))]
            crate::clipboard::Selection::X11 => drop(fd),
        }
    }
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
