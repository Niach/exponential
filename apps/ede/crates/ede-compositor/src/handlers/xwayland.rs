//! Xwayland: X11 apps (and most games) as ordinary eDE windows. smithay's
//! X11Wm is the window manager; mapped X11 windows join the same space and
//! get the same placement, focus, tiling, fullscreen and Alt+F4 as Wayland
//! ones. Override-redirect windows (menus, tooltips) map where they ask.
//! Clipboard flows both ways through the Wayland data device.

use std::{os::unix::io::OwnedFd, process::Stdio};

use smithay::{
    desktop::Window,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    input::{
        keyboard::KeyboardTarget,
        pointer::{Focus, GrabStartData},
    },
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
    desk::DeskId,
    grabs::{MoveSurfaceGrab, BTN_LEFT},
    layout::Tile,
    state::{set_window_state, window_state, Ede},
};

impl Ede {
    /// Starts Xwayland on the next free display; `DISPLAY` is set for
    /// children once it is ready.
    pub fn start_xwayland(&mut self) {
        // X sockets live in /tmp/.X11-unix, which systemd-tmpfiles creates on
        // a desktop but a container or minimal system may lack.
        let dir = std::path::Path::new("/tmp/.X11-unix");
        if !dir.exists() {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::create_dir(dir).and_then(|()| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777))) {
                tracing::warn!("{}: {e}", dir.display());
            }
        }
        // smithay runs `Xwayland` from PATH: the .deb's own 24.1+ goes first.
        // The rest of the environment passes through (GPU drivers found via
        // LD_LIBRARY_PATH and the like).
        let path = crate::doctor::xwayland_search_path(std::env::var_os("PATH").as_deref(), std::path::Path::new(crate::doctor::BUNDLED_XWAYLAND_DIR));
        let spawned = XWayland::spawn_with_inherited_env(&self.display_handle, None, [("PATH", path)], std::iter::empty::<&str>(), true, Stdio::null(), Stdio::inherit(), |_| ());
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
                match X11Wm::start_wm(state.loop_handle.clone(), &state.display_handle, x11_socket, client.clone()) {
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

    /// #EDE-88: X gives keys to its own input focus (by default the window
    /// under X's pointer), so the X11 window whose surface now has the
    /// keyboard takes X's focus too, and the one that lost it gives it up.
    pub(crate) fn sync_x11_focus(&mut self, focused: Option<&WlSurface>) {
        let x11 = focused.and_then(|f| {
            self.desk.space.elements().find_map(|w| w.x11_surface().filter(|x| x.wl_surface().as_ref() == Some(f)).cloned())
        });
        if self.desk.x11_focus.1 == x11 {
            return;
        }
        let seat = self.desk.x11_focus.0.clone();
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(old) = std::mem::replace(&mut self.desk.x11_focus.1, x11.clone()) {
            KeyboardTarget::leave(&old, &seat, self, serial);
        }
        if let Some(new) = x11 {
            KeyboardTarget::enter(&new, &seat, self, Vec::new(), serial);
        }
    }

    fn x11_window(&self, surface: &X11Surface) -> Option<Window> {
        self.desk.space.elements().find(|w| w.x11_surface() == Some(surface)).cloned()
    }
}

impl XWaylandShellHandler for Ede {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(&mut self, _xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        if self.desk.pending_x11_focus.as_ref() == Some(&surface) {
            self.desk.pending_x11_focus = None;
            let keyboard = self.desk.seat.get_keyboard().unwrap();
            keyboard.set_focus(self, Some(wl_surface), SERIAL_COUNTER.next_serial());
        }
    }
}

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
        // #EDE-88: every X11 window gets our title bar unless its Motif
        // hints ask for no decorations (smithay's `is_decorated` = "the
        // client draws its own") or it is no application window (a dock,
        // a splash, a menu that is not override-redirect). Set on the
        // Window that maps, before it is placed (the frame counts there).
        use smithay::xwayland::xwm::WmWindowType as T;
        let decorated = !surface.is_decorated() && matches!(surface.window_type(), None | Some(T::Normal | T::Dialog | T::Utility));
        let window = Window::new_x11_window(surface);
        let mut st = window_state(&window);
        st.decorated = decorated;
        set_window_state(&window, st);
        // Same rules as a Wayland toplevel's first frame.
        self.place_new_window(self.desk_of_window(&window), window.clone());
        if wants_fullscreen {
            self.set_fullscreen(&window, true);
        }
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        let loc = surface.last_configure().loc;
        self.desk.space.map_element(Window::new_x11_window(surface), loc, true);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        // Parked on another workspace: forget it there.
        if let Some(parked) = self.desk.workspaces.parked.iter().find(|w| w.x11_surface() == Some(&surface)).cloned() {
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &parked);
            self.forget_parked(&parked);
        }
        if let Some(minimized) = self.desk.minimized.iter().find(|w| w.x11_surface() == Some(&surface)).cloned() {
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &minimized);
            self.forget_minimized(&minimized);
        }
        if let Some(window) = self.x11_window(&surface) {
            let desk = self.desk_of_window(&window);
            let had_focus = self.focused_window(desk).as_ref() == Some(&window);
            if !surface.is_override_redirect() {
                self.window_event(ede_protocol::events::WINDOW_CLOSED, &window);
            }
            self.desk.space.unmap_elem(&window);
            if had_focus {
                let next = self.desk.space.elements().rev().find(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect())).cloned();
                self.focus(desk, next);
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
                let _ = surface.configure(self.desk.space.element_geometry(window));
                return;
            }
        }
        let mut geo = surface.last_configure();
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
                self.desk.space.map_element(window, geometry.loc, false);
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

    // _NET_WM_STATE_HIDDEN / WM_CHANGE_STATE Iconic (#EDE-48): the app's own
    // minimize goes into the taskbar; the X client asking back restores.
    fn minimize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(w) = self.x11_window(&surface) {
            self.minimize_window(&w);
        }
    }

    fn unminimize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(w) = self.desk.minimized.iter().find(|w| w.x11_surface() == Some(&surface)).cloned() {
            self.restore_window(&w);
            // #EDE-82: a client's own request brings it in front only when
            // its app is the one typed into.
            if self.takes_keyboard(&w) {
                self.focus(self.desk_of_window(&w), Some(w));
            } else {
                self.keep_behind(&w);
            }
        }
    }

    fn resize_request(&mut self, _xwm: XwmId, _surface: X11Surface, _button: u32, _edges: X11ResizeEdge) {
        // X11 windows resize from their own frame or by the app; interactive
        // edge resizing for them comes with server-side decorations.
    }

    fn move_request(&mut self, _xwm: XwmId, surface: X11Surface, _button: u32) {
        let Some(window) = self.x11_window(&surface) else { return };
        let Some(loc) = self.desk.space.element_location(&window) else { return };
        let pointer = self.desk.seat.get_pointer().unwrap();
        let start = pointer
            .grab_start_data()
            .unwrap_or(GrabStartData { focus: None, button: BTN_LEFT, location: pointer.current_location() });
        pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), SERIAL_COUNTER.next_serial(), Focus::Clear);
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        // Only the focused X11 client may read the clipboard.
        self.focused_window(DeskId::PERSON).and_then(|w| w.x11_surface().and_then(|x| x.xwm_id())) == Some(xwm)
    }

    fn send_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_type: String, fd: OwnedFd) {
        if selection != SelectionTarget::Clipboard {
            return;
        }
        // Ours ([`crate::clipboard`]) is served from memory; a Wayland
        // client's through its source.
        let ours = match current_data_device_selection_userdata(&self.desk.seat).as_deref() {
            Some(crate::clipboard::Selection::Ours(owned)) => Some(owned.clone()),
            _ => None,
        };
        if let Some(owned) = ours {
            self.serve_owned(&owned, &mime_type, fd);
        } else if let Err(e) = request_data_device_client_selection(&self.desk.seat, mime_type, fd) {
            tracing::warn!("clipboard to X11: {e:?}");
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        if selection == SelectionTarget::Clipboard {
            self.desk.clipboard.changed(crate::clipboard::Owner::X11(mime_types.clone()));
            set_data_device_selection(&self.display_handle, &self.desk.seat, mime_types, crate::clipboard::Selection::X11);
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        let x11 = matches!(current_data_device_selection_userdata(&self.desk.seat).as_deref(), Some(crate::clipboard::Selection::X11));
        if selection == SelectionTarget::Clipboard && x11 {
            self.desk.clipboard.changed(crate::clipboard::Owner::Empty);
            clear_data_device_selection(&self.display_handle, &self.desk.seat);
        }
    }
}

/// Xwayland's own client data carries its compositor state.
pub fn client_compositor_state(client: &smithay::reexports::wayland_server::Client) -> Option<&smithay::wayland::compositor::CompositorClientState> {
    client.get_data::<smithay::xwayland::XWaylandClientData>().map(|d| &d.compositor_state)
}
