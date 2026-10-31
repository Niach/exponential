//! xdg-shell: toplevels float, open centred on the pointer's output once
//! their first buffer arrives, and maximize into the work area.

use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
    delegate_xdg_shell,
    desktop::{find_popup_root_surface, get_popup_toplevel_coords, PopupKind, Window},
    input::{
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        Seat,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{wl_output, wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState, XdgToplevelSurfaceData},
    },
};

use crate::{
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab},
    layout::Tile,
    state::{set_window_state, window_state, Ede},
};

impl XdgShellHandler for Ede {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.unmapped.push(Window::new_wayland_window(surface));
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let wl = surface.wl_surface().clone();
        self.unmapped.retain(|w| w.toplevel().is_some_and(|t| t.wl_surface() != &wl));
        let gone = self.space.elements().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &wl)).cloned();
        if let Some(window) = gone {
            let had_focus = self.focused_window().as_ref() == Some(&window);
            self.space.unmap_elem(&window);
            if had_focus {
                // Focus falls to the window now on top, as on Cinnamon.
                let next = self.space.elements().last().cloned();
                self.focus(next);
            }
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        if let Err(e) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            tracing::warn!("popup refused: {e}");
        }
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::from_resource(&seat) else { return };
        let Some(start_data) = check_grab(&seat, surface.wl_surface(), serial) else { return };
        let Some(window) = self.window_for_surface(surface.wl_surface()) else { return };
        let Some(loc) = self.space.element_location(&window) else { return };
        let pointer = seat.get_pointer().unwrap();
        pointer.set_grab(self, MoveSurfaceGrab::new(start_data, window, loc), serial, Focus::Clear);
    }

    fn resize_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial, edges: xdg_toplevel::ResizeEdge) {
        let Some(seat) = Seat::from_resource(&seat) else { return };
        let Some(start_data) = check_grab(&seat, surface.wl_surface(), serial) else { return };
        let Some(window) = self.window_for_surface(surface.wl_surface()) else { return };
        let Some(loc) = self.space.element_location(&window) else { return };
        // Resizing a tiled window makes it float at its new size.
        let mut st = window_state(&window);
        if st.tile.take().is_some() {
            st.restore = None;
            set_window_state(&window, st);
        }
        let size = window.geometry().size;
        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Maximized);
            state.states.set(xdg_toplevel::State::Resizing);
        });
        surface.send_pending_configure();
        let grab = ResizeSurfaceGrab::start(start_data, window, edges.into(), Rectangle::new(loc, size));
        let pointer = seat.get_pointer().unwrap();
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        match self.window_for_surface(surface.wl_surface()) {
            Some(window) if self.space.element_location(&window).is_some() => self.tile(&window, Tile::Maximized),
            // Not mapped yet: it opens maximized once its first buffer arrives.
            _ => {
                surface.with_pending_state(|s| s.states.set(xdg_toplevel::State::Maximized));
                if surface.is_initial_configure_sent() {
                    surface.send_pending_configure();
                }
            }
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            self.untile(&window, None);
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<wl_output::WlOutput>) {
        match self.window_for_surface(surface.wl_surface()) {
            Some(window) if self.space.element_location(&window).is_some() => self.set_fullscreen(&window, true),
            // Not mapped yet: it opens fullscreen once its first buffer arrives.
            _ => {
                surface.with_pending_state(|s| s.states.set(xdg_toplevel::State::Fullscreen));
                if surface.is_initial_configure_sent() {
                    surface.send_pending_configure();
                }
            }
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            self.set_fullscreen(&window, false);
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // Popup grabs (menus that close on an outside click) come with the shell.
    }
}
delegate_xdg_shell!(Ede);

fn check_grab(seat: &Seat<Ede>, surface: &WlSurface, serial: Serial) -> Option<PointerGrabStartData<Ede>> {
    let pointer = seat.get_pointer()?;
    // Only a surface holding the click that started this may grab.
    if !pointer.has_grab(serial) {
        return None;
    }
    let start_data = pointer.grab_start_data()?;
    let (focus, _) = start_data.focus.as_ref()?;
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }
    Some(start_data)
}

/// Called on every `wl_surface.commit`.
pub fn handle_commit(state: &mut Ede, surface: &WlSurface) {
    if let Some(window) = state.unmapped.iter().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface)).cloned() {
        let toplevel = window.toplevel().unwrap().clone();
        let initial_configure_sent =
            with_states(surface, |states| states.data_map.get::<XdgToplevelSurfaceData>().unwrap().lock().unwrap().initial_configure_sent);
        if !initial_configure_sent {
            // Bounds = the work area of the pointer's output, so the client
            // never opens bigger than the screen.
            let pointer = state.seat.get_pointer().unwrap().current_location();
            let bounds = state.output_at(pointer).and_then(|o| state.work_area(&o)).map(|a| a.size);
            toplevel.with_pending_state(|s| s.bounds = bounds);
            toplevel.send_configure();
        } else if with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false) {
            let (wants_max, wants_full) = toplevel.with_pending_state(|s| {
                let full = s.states.contains(xdg_toplevel::State::Fullscreen);
                s.states.unset(xdg_toplevel::State::Fullscreen);
                (s.states.contains(xdg_toplevel::State::Maximized), full)
            });
            state.place_new_window(window.clone());
            if wants_max {
                state.tile(&window, Tile::Maximized);
            }
            if wants_full {
                state.set_fullscreen(&window, true);
            }
        }
    }

    state.popups.commit(surface);
    if let Some(PopupKind::Xdg(ref xdg)) = state.popups.find_popup(surface) {
        if !xdg.is_initial_configure_sent() {
            // The initial configure is always allowed.
            xdg.send_configure().expect("initial configure failed");
        }
    }
}

impl Ede {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else { return };
        let Some(window) = self.window_for_surface(&root) else { return };
        let Some(window_geo) = self.space.element_geometry(&window) else { return };
        let Some(output) = self.output_at(window_geo.loc.to_f64()) else { return };
        let Some(output_geo) = self.space.output_geometry(&output) else { return };

        // The positioner's target is relative to the parent's geometry.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
