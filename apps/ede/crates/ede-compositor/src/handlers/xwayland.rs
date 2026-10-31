//! Xwayland: X11 apps (and most games) as ordinary eDE windows. smithay's
//! X11Wm is the window manager; mapped X11 windows join the same space and
//! get the same placement, focus, tiling, fullscreen and Alt+F4 as Wayland
//! ones. Override-redirect windows (menus, tooltips) map where they ask.
//! Clipboard flows both ways through the Wayland data device.

use std::{os::unix::io::OwnedFd, process::Stdio};

use smithay::{
    delegate_xwayland_shell,
    desktop::Window,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    input::pointer::{Focus, GrabStartData},
    utils::{Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        selection::{
            data_device::{clear_data_device_selection, current_data_device_selection_userdata, request_data_device_client_selection, set_data_device_selection},
            SelectionTarget,
        },
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, XwmId},
        X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler,
    },
};

use crate::{
    grabs::{MoveSurfaceGrab, BTN_LEFT},
    layout::Tile,
    state::{window_state, Ede},
};

impl Ede {
    /// Starts Xwayland on the next free display; `DISPLAY` is set for
    /// children once it is ready.
    pub fn start_xwayland(&mut self) {
        let spawned = XWayland::spawn(&self.display_handle, None, std::iter::empty::<(String, String)>(), true, Stdio::null(), Stdio::null(), |_| ());
        let (xwayland, client) = match spawned {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!("Xwayland not started (X11 apps won't run): {e}");
                self.run_autostart();
                return;
            }
        };
        let inserted = self.loop_handle.insert_source(xwayland, move |event, _, state| match event {
            XWaylandEvent::Ready { x11_socket, display_number } => {
                match X11Wm::start_wm(state.loop_handle.clone(), x11_socket, client.clone()) {
                    Ok(wm) => {
                        tracing::info!("Xwayland ready on :{display_number}");
                        state.xwm = Some(wm);
                        state.xdisplay = Some(display_number);
                    }
                    Err(e) => tracing::error!("X11 window manager failed: {e}"),
                }
                state.run_autostart();
            }
            XWaylandEvent::Error => {
                tracing::warn!("Xwayland crashed on startup");
                state.run_autostart();
            }
        });
        if let Err(e) = inserted {
            tracing::error!("Xwayland source: {e}");
        }
    }

    fn x11_window(&self, surface: &X11Surface) -> Option<Window> {
        self.space.elements().find(|w| w.x11_surface() == Some(surface)).cloned()
    }
}

impl XWaylandShellHandler for Ede {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(&mut self, _xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        if self.pending_x11_focus.as_ref() == Some(&surface) {
            self.pending_x11_focus = None;
            let keyboard = self.seat.get_keyboard().unwrap();
            keyboard.set_focus(self, Some(wl_surface), SERIAL_COUNTER.next_serial());
        }
    }
}
delegate_xwayland_shell!(Ede);

impl XwmHandler for Ede {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().expect("X11 events only arrive once the WM runs")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}
    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Err(e) = surface.set_mapped(true) {
            tracing::warn!("X11 map: {e}");
            return;
        }
        let wants_fullscreen = surface.is_fullscreen();
        let window = Window::new_x11_window(surface);
        // Same rules as a Wayland toplevel's first frame.
        self.place_new_window(window.clone());
        if wants_fullscreen {
            self.set_fullscreen(&window, true);
        }
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        let loc = surface.geometry().loc;
        self.space.map_element(Window::new_x11_window(surface), loc, true);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = self.x11_window(&surface) {
            let had_focus = self.focused_window().as_ref() == Some(&window);
            self.space.unmap_elem(&window);
            if had_focus {
                let next = self.space.elements().rev().find(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect())).cloned();
                self.focus(next);
            }
        }
        if !surface.is_override_redirect() {
            let _ = surface.set_mapped(false);
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        surface: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        // Tiled and fullscreen windows keep their geometry; floating ones
        // may resize, and move only while not yet mapped.
        let mapped = self.x11_window(&surface);
        if let Some(window) = &mapped {
            let st = window_state(window);
            if st.tile.is_some() || st.fullscreen.is_some() {
                let _ = surface.configure(self.space.element_geometry(window));
                return;
            }
        }
        let mut geo = surface.geometry();
        if mapped.is_none() {
            geo.loc = (x.unwrap_or(geo.loc.x), y.unwrap_or(geo.loc.y)).into();
        }
        if let Some(w) = w {
            geo.size.w = w as i32;
        }
        if let Some(h) = h {
            geo.size.h = h as i32;
        }
        let _ = surface.configure(geo);
    }

    fn configure_notify(&mut self, _xwm: XwmId, surface: X11Surface, geometry: Rectangle<i32, Logical>, _above: Option<u32>) {
        if let Some(window) = self.x11_window(&surface) {
            if surface.is_override_redirect() {
                self.space.map_element(window, geometry.loc, false);
            }
        }
    }

    fn maximize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(w) = self.x11_window(&surface) {
            self.tile(&w, Tile::Maximized);
        }
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(w) = self.x11_window(&surface) {
            self.untile(&w, None);
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(w) = self.x11_window(&surface) {
            self.set_fullscreen(&w, true);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(w) = self.x11_window(&surface) {
            self.set_fullscreen(&w, false);
        }
    }

    fn resize_request(&mut self, _xwm: XwmId, _surface: X11Surface, _button: u32, _edges: X11ResizeEdge) {
        // X11 windows resize from their own frame or by the app; interactive
        // edge resizing for them comes with server-side decorations.
    }

    fn move_request(&mut self, _xwm: XwmId, surface: X11Surface, _button: u32) {
        let Some(window) = self.x11_window(&surface) else { return };
        let Some(loc) = self.space.element_location(&window) else { return };
        let pointer = self.seat.get_pointer().unwrap();
        let start = pointer
            .grab_start_data()
            .unwrap_or(GrabStartData { focus: None, button: BTN_LEFT, location: pointer.current_location() });
        pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), SERIAL_COUNTER.next_serial(), Focus::Clear);
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        // Only the focused X11 client may read the clipboard.
        self.focused_window().and_then(|w| w.x11_surface().and_then(|x| x.xwm_id())) == Some(xwm)
    }

    fn send_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_type: String, fd: OwnedFd) {
        if selection == SelectionTarget::Clipboard {
            if let Err(e) = request_data_device_client_selection(&self.seat, mime_type, fd) {
                tracing::warn!("clipboard to X11: {e:?}");
            }
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        if selection == SelectionTarget::Clipboard {
            set_data_device_selection(&self.display_handle, &self.seat, mime_types, ());
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        if selection == SelectionTarget::Clipboard && current_data_device_selection_userdata(&self.seat).is_some() {
            clear_data_device_selection(&self.display_handle, &self.seat);
        }
    }
}

/// Xwayland's own client data carries its compositor state.
pub fn client_compositor_state(client: &smithay::reexports::wayland_server::Client) -> Option<&smithay::wayland::compositor::CompositorClientState> {
    client.get_data::<smithay::xwayland::XWaylandClientData>().map(|d| &d.compositor_state)
}
