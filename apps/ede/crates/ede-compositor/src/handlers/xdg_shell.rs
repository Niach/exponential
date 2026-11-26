//! xdg-shell: toplevels float, open centred on the pointer's output once
//! their first buffer arrives, and maximize into the work area.

use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
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
        seat::WaylandFocus,
        shell::xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState, XdgToplevelSurfaceData},
    },
};

use crate::{
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab},
    layout::Tile,
    spaces::space_of_surface,
    state::{set_window_state, window_state, Ede},
};

impl XdgShellHandler for Ede {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        if let Some(id) = space_of_surface(surface.wl_surface()) {
            return self.space_new_toplevel(id, surface);
        }
        self.desk.unmapped.push(Window::new_wayland_window(surface));
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = space_of_surface(surface.wl_surface()) {
            return self.space_toplevel_destroyed(id, surface);
        }
        let wl = surface.wl_surface().clone();
        self.desk.unmapped.retain(|w| w.toplevel().is_some_and(|t| t.wl_surface() != &wl));
        if let Some(parked) = self.desk.workspaces.parked.iter().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &wl)).cloned() {
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &parked);
            self.forget_parked(&parked);
        }
        if let Some(minimized) = self.desk.minimized.iter().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &wl)).cloned() {
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &minimized);
            self.forget_minimized(&minimized);
        }
        let gone = self.desk.space.elements().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &wl)).cloned();
        if let Some(window) = gone {
            let desk = self.desk_of_window(&window);
            let had_focus = self.focused_window(desk).as_ref() == Some(&window);
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &window);
            self.desk.space.unmap_elem(&window);
            if had_focus {
                // Focus falls to the window now on top, as on Cinnamon.
                let next = self.desk.space.elements().last().cloned();
                self.focus(desk, next);
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
        let Some(loc) = self.desk.space.element_location(&window) else { return };
        let pointer = seat.get_pointer().unwrap();
        pointer.set_grab(self, MoveSurfaceGrab::new(start_data, window, loc), serial, Focus::Clear);
    }

    fn resize_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial, edges: xdg_toplevel::ResizeEdge) {
        let Some(seat) = Seat::from_resource(&seat) else { return };
        let Some(start_data) = check_grab(&seat, surface.wl_surface(), serial) else { return };
        let Some(window) = self.window_for_surface(surface.wl_surface()) else { return };
        let Some(loc) = self.desk.space.element_location(&window) else { return };
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
        if let Some(id) = space_of_surface(surface.wl_surface()) {
            return self.space_keeps_state(id, surface);
        }
        match self.window_for_surface(surface.wl_surface()) {
            Some(window) if self.desk.space.element_location(&window).is_some() => self.tile(&window, Tile::Maximized),
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

    // The app's own minimize button (#EDE-48): into the taskbar.
    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            self.minimize_window(&window);
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<wl_output::WlOutput>) {
        if let Some(id) = space_of_surface(surface.wl_surface()) {
            return self.space_keeps_state(id, surface);
        }
        match self.window_for_surface(surface.wl_surface()) {
            Some(window) if self.desk.space.element_location(&window).is_some() => self.set_fullscreen(&window, true),
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
        if let Some(id) = space_of_surface(surface.wl_surface()) {
            return self.space_keeps_state(id, surface);
        }
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            self.set_fullscreen(&window, false);
        }
    }

    // A mapped window's title or app id changed (the taskbar re-reads it);
    // an unmapped one is announced whole with `window.opened`.
    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.mapped_window_changed(surface.wl_surface());
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.mapped_window_changed(surface.wl_surface());
    }

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // A popup that grabs asks for the keyboard (#EDE-86: the shell's
        // start menu hangs off the taskbar, a layer that never takes it
        // itself); it gets it as it maps (`handle_commit`). An outside
        // click reaches the shell as `popups.dismiss`.
        with_states(surface.wl_surface(), |states| states.data_map.insert_if_missing(|| PopupGrab));
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        // The shell's menu closed while it held the keyboard: the top window
        // gets it back, then an Exclusive prompt that is still up (#EDE-86).
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        if keyboard.current_focus().as_ref() == Some(surface.wl_surface()) {
            self.focus_after_unlock(None);
        }
    }
}

/// Marks a shell popup that already took the keyboard once.
struct PopupFocused;

/// Marks a popup that asked for the grab (`xdg_popup.grab`).
struct PopupGrab;

impl Ede {
    pub fn mapped_window_changed(&mut self, surface: &WlSurface) {
        let mapped = self.desk.space.elements().find(|w| w.wl_surface().is_some_and(|s| *s == *surface)).cloned();
        if let Some(w) = mapped {
            self.window_event(ede_protocol::events::WINDOW_CHANGED, &w);
        }
    }
}

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
    if let Some(window) = state.desk.unmapped.iter().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface)).cloned() {
        let toplevel = window.toplevel().unwrap().clone();
        let initial_configure_sent =
            with_states(surface, |states| states.data_map.get::<XdgToplevelSurfaceData>().unwrap().lock().unwrap().initial_configure_sent);
        if !initial_configure_sent {
            // Bounds = the work area of the pointer's output, so the client
            // never opens bigger than the screen.
            let pointer = state.desk.seat.get_pointer().unwrap().current_location();
            let bounds = state.output_at(pointer).and_then(|o| state.work_area(&o)).map(|a| a.size);
            toplevel.with_pending_state(|s| s.bounds = bounds);
            toplevel.send_configure();
        } else if with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false) {
            let (wants_max, wants_full) = toplevel.with_pending_state(|s| {
                let full = s.states.contains(xdg_toplevel::State::Fullscreen);
                s.states.unset(xdg_toplevel::State::Fullscreen);
                (s.states.contains(xdg_toplevel::State::Maximized), full)
            });
            state.place_new_window(state.desk_of_window(&window), window.clone());
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
        } else if with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false) {
            // A popup of a layer surface that takes the keyboard (the shell's
            // menu with its search box, #EDE-12), or one that grabs under
            // any layer surface (#EDE-86), gets it when it maps; a window's
            // popup keeps its window's focus.
            let (first, grabs) = with_states(surface, |states| (states.data_map.insert_if_missing(|| PopupFocused), states.data_map.get::<PopupGrab>().is_some()));
            if first && state.layer_root_takes_keyboard(&PopupKind::Xdg(xdg.clone()), grabs) {
                let keyboard = state.desk.seat.get_keyboard().unwrap();
                keyboard.set_focus(state, Some(surface.clone()), smithay::utils::SERIAL_COUNTER.next_serial());
            }
        }
    }
}

impl Ede {
    /// The output a layer surface `root` stands on, in the coordinates of
    /// `popup`'s parent: what [`layer_popup_target_rect`] gives the positioner.
    fn layer_popup_target(&self, root: &WlSurface, popup: &PopupSurface) -> Option<Rectangle<i32, smithay::utils::Logical>> {
        self.desk.space.outputs().find_map(|o| {
            let map = smithay::desktop::layer_map_for_output(o);
            let layer = map.layer_for_surface(root, smithay::desktop::WindowSurfaceType::TOPLEVEL)?;
            let layer_geo = map.layer_geometry(layer)?;
            let output_size = self.desk.space.output_geometry(o)?.size;
            Some(layer_popup_target_rect(output_size, layer_geo.loc, get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()))))
        })
    }

    /// Is `popup` under a layer surface whose keyboard interactivity is not
    /// `none`, or under any layer surface when it `grabs`?
    pub fn layer_root_takes_keyboard(&self, popup: &PopupKind, grabs: bool) -> bool {
        let Ok(root) = find_popup_root_surface(popup) else { return false };
        self.desk.space.outputs().any(|o| {
            let map = smithay::desktop::layer_map_for_output(o);
            map.layer_for_surface(&root, smithay::desktop::WindowSurfaceType::TOPLEVEL).is_some_and(|l| grabs || l.can_receive_keyboard_focus())
        })
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        if let Some(id) = space_of_surface(popup.wl_surface()) {
            return self.space_unconstrain_popup(id, popup);
        }
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else { return };
        // A panel's popup (the taskbar's context menus, its previews, the
        // start menu): kept on the panel's output (#EDE-237: a menu at the
        // pointer opened downwards off the screen).
        if let Some(target) = self.layer_popup_target(&root, popup) {
            popup.with_pending_state(|state| state.geometry = state.positioner.get_unconstrained_geometry(target));
            return;
        }
        let Some(window) = self.window_for_surface(&root) else { return };
        let Some(window_geo) = self.desk.space.element_geometry(&window) else { return };
        let Some(output) = self.output_at(window_geo.loc.to_f64()) else { return };
        let Some(output_geo) = self.desk.space.output_geometry(&output) else { return };

        // The positioner's target is relative to the parent's geometry.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

/// The output (`output_size`, its own origin) as a positioner's constraint
/// rectangle for a popup of a layer surface at `layer_loc` on it, whose
/// parent sits `parent_offset` into that layer (a nested popup's parent).
pub fn layer_popup_target_rect(output_size: smithay::utils::Size<i32, smithay::utils::Logical>, layer_loc: smithay::utils::Point<i32, smithay::utils::Logical>, parent_offset: smithay::utils::Point<i32, smithay::utils::Logical>) -> Rectangle<i32, smithay::utils::Logical> {
    let mut target = Rectangle::new((0, 0).into(), output_size);
    target.loc -= layer_loc;
    target.loc -= parent_offset;
    target
}

#[cfg(test)]
mod tests {
    use smithay::{
        reexports::wayland_protocols::xdg::shell::server::xdg_positioner::{Anchor, ConstraintAdjustment, Gravity},
        utils::{Logical, Rectangle},
        wayland::shell::xdg::PositionerState,
    };

    use super::layer_popup_target_rect;

    const OUTPUT: (i32, i32) = (1920, 1080);
    const PANEL: i32 = 48;
    const MENU: (i32, i32) = (220, 150);

    /// The shell's positioner for a panel's context menu (uicore-gpui
    /// `panel_positioner`): the anchor is the panel's whole height at the
    /// pointer; above it for a bottom panel, below it for a top one; only
    /// sliding along the panel, never flipping into it.
    fn menu_at(pointer_x: i32, bottom_panel: bool) -> PositionerState {
        let (anchor_edges, gravity, offset) = if bottom_panel { (Anchor::TopLeft, Gravity::TopRight, (0, -4)) } else { (Anchor::BottomLeft, Gravity::BottomRight, (0, 4)) };
        PositionerState {
            rect_size: MENU.into(),
            anchor_rect: Rectangle::new((pointer_x, 0).into(), (0, PANEL).into()),
            anchor_edges,
            gravity,
            constraint_adjustment: ConstraintAdjustment::SlideX,
            offset: offset.into(),
            ..Default::default()
        }
    }

    fn placed(pointer_x: i32, bottom_panel: bool) -> Rectangle<i32, Logical> {
        let layer_loc = if bottom_panel { (0, OUTPUT.1 - PANEL) } else { (0, 0) };
        let target = layer_popup_target_rect(OUTPUT.into(), layer_loc.into(), (0, 0).into());
        let mut geo = menu_at(pointer_x, bottom_panel).get_unconstrained_geometry(target);
        // Back to output coordinates.
        geo.loc += smithay::utils::Point::from(layer_loc);
        geo
    }

    fn on_output(geo: Rectangle<i32, Logical>) -> bool {
        geo.loc.x >= 0 && geo.loc.y >= 0 && geo.loc.x + geo.size.w <= OUTPUT.0 && geo.loc.y + geo.size.h <= OUTPUT.1
    }

    #[test]
    fn a_bottom_panels_menu_opens_upwards_on_its_top_edge() {
        let geo = placed(400, true);
        assert_eq!(geo.loc.y + geo.size.h, OUTPUT.1 - PANEL - 4, "the menu stands on the panel: {geo:?}");
        assert_eq!(geo.loc.x, 400, "at the pointer");
        assert!(on_output(geo), "{geo:?}");
    }

    #[test]
    fn a_top_panels_menu_opens_downwards_under_it() {
        let geo = placed(400, false);
        assert_eq!(geo.loc.y, PANEL + 4, "the menu hangs under the panel: {geo:?}");
        assert!(on_output(geo), "{geo:?}");
    }

    #[test]
    fn a_menu_at_the_right_edge_slides_left_onto_the_output() {
        let geo = placed(OUTPUT.0 - 10, true);
        assert_eq!(geo.loc.x + geo.size.w, OUTPUT.0, "flush with the right edge: {geo:?}");
        assert_eq!(geo.loc.y + geo.size.h, OUTPUT.1 - PANEL - 4, "still above the panel: {geo:?}");
        let top = placed(OUTPUT.0 - 10, false);
        assert_eq!(top.loc.x + top.size.w, OUTPUT.0, "{top:?}");
        assert_eq!(top.loc.y, PANEL + 4, "{top:?}");
    }

    #[test]
    fn a_menu_at_the_left_edge_stays_on_the_output() {
        let geo = placed(0, true);
        assert_eq!(geo.loc.x, 0, "{geo:?}");
        assert!(on_output(geo), "{geo:?}");
        // A pointer left of the output (a panel wider than it): slides back.
        let mut state = menu_at(-30, true);
        state.anchor_rect.loc.x = -30;
        let target = layer_popup_target_rect(OUTPUT.into(), (0, OUTPUT.1 - PANEL).into(), (0, 0).into());
        assert_eq!(state.get_unconstrained_geometry(target).loc.x, 0);
    }

    #[test]
    fn the_target_is_the_output_in_the_parents_coordinates() {
        let target = layer_popup_target_rect(OUTPUT.into(), (0, OUTPUT.1 - PANEL).into(), (10, 5).into());
        assert_eq!(target, Rectangle::new((-10, -(OUTPUT.1 - PANEL) - 5).into(), OUTPUT.into()));
    }
}
