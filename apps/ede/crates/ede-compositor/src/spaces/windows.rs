//! #EDE-125: a space's windows. A toplevel without a parent fills its
//! space's output as a maximized window, one with a parent is centred on
//! it; each takes the space's keyboard as it maps. No frames, no tiles,
//! no workspaces, no events: none of the person's window code runs.

use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
    desktop::Window,
    input::pointer::CursorImageStatus,
    reexports::{wayland_protocols::xdg::shell::server::xdg_toplevel, wayland_server::protocol::wl_surface::WlSurface},
    utils::SERIAL_COUNTER,
    wayland::{
        compositor::{get_parent, is_sync_subsurface},
        seat::WaylandFocus,
        shell::xdg::ToplevelSurface,
    },
};

use crate::{
    desk::{DeskId, DeskTag},
    state::Ede,
};

impl Ede {
    /// The window of desk `id` whose toplevel is `surface`, mapped or not.
    fn space_window(&self, id: DeskId, surface: &WlSurface) -> Option<Window> {
        let desk = self.try_desk(id)?;
        desk.space.elements().chain(desk.unmapped.iter()).find(|w| w.wl_surface().is_some_and(|s| *s == *surface)).cloned()
    }

    pub(crate) fn space_new_toplevel(&mut self, id: DeskId, surface: ToplevelSurface) {
        let Some(desk) = self.try_desk_mut(id) else { return };
        let window = Window::new_wayland_window(surface);
        window.user_data().insert_if_missing(|| DeskTag(id));
        desk.unmapped.push(window);
    }

    pub(crate) fn space_toplevel_destroyed(&mut self, id: DeskId, surface: ToplevelSurface) {
        let Some(window) = self.space_window(id, surface.wl_surface()) else { return };
        let had_focus = self.desk(id).seat.get_keyboard().unwrap().current_focus().as_ref() == Some(surface.wl_surface());
        let desk = self.desk_mut(id);
        desk.unmapped.retain(|w| w != &window);
        desk.space.unmap_elem(&window);
        if had_focus {
            let next = desk.space.elements().last().cloned();
            self.space_focus(id, next);
        }
        self.space_schedule(id);
    }

    /// A space's window is as large as it gets and never fullscreen: the
    /// answer to `set_maximized`, `set_fullscreen` and `unset_fullscreen`
    /// is the state it has.
    pub(crate) fn space_keeps_state(&mut self, id: DeskId, surface: ToplevelSurface) {
        if self.try_desk(id).is_some() && surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    pub(crate) fn space_cursor_image(&mut self, id: DeskId, image: CursorImageStatus) {
        if let Some(desk) = self.try_desk_mut(id) {
            desk.cursor_status = image;
        }
    }

    /// A commit of a surface made in space `id`.
    pub(crate) fn space_commit(&mut self, id: DeskId, surface: &WlSurface) {
        let Some(size) = self.spaces.get(id).and_then(|s| s.output.current_mode()).map(|m| m.size.to_logical(1)) else { return };
        // Every commit may change the scene and may wait for a frame callback.
        self.space_schedule(id);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.space_window(id, &root) {
                window.on_commit();
            }
        }
        let Some(window) = self.desk(id).unmapped.iter().find(|w| w.wl_surface().is_some_and(|s| *s == *surface)).cloned() else { return };
        let toplevel = window.toplevel().unwrap().clone();
        let fills = toplevel.parent().is_none();
        if !toplevel.is_initial_configure_sent() {
            toplevel.with_pending_state(|s| {
                s.bounds = Some(size);
                if fills {
                    s.size = Some(size);
                    s.states.set(xdg_toplevel::State::Maximized);
                }
            });
            toplevel.send_configure();
        } else if with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false) {
            let own = window.geometry().size;
            let at = if fills { (0, 0) } else { ((size.w - own.w).max(0) / 2, (size.h - own.h).max(0) / 2) };
            let desk = self.desk_mut(id);
            desk.unmapped.retain(|w| w != &window);
            desk.space.map_element(window.clone(), at, true);
            self.space_focus(id, Some(window));
        }
    }

    /// Raises `window` in space `id` and gives it the space's keyboard;
    /// `None` clears it. Never [`Ede::focus`]: that one answers to the
    /// person's lock and prompts and tells the shell.
    pub fn space_focus(&mut self, id: DeskId, window: Option<Window>) {
        let Some(desk) = self.try_desk_mut(id) else { return };
        let keyboard = desk.seat.get_keyboard().unwrap();
        match &window {
            Some(window) => desk.space.raise_element(window, true),
            None => desk.space.elements().for_each(|w| {
                w.set_activated(false);
            }),
        }
        let toplevels: Vec<_> = desk.space.elements().filter_map(|w| w.toplevel().cloned()).collect();
        let surface = window.as_ref().and_then(|w| w.wl_surface()).map(|s| s.into_owned());
        keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
        for toplevel in toplevels {
            toplevel.send_pending_configure();
        }
    }
}
