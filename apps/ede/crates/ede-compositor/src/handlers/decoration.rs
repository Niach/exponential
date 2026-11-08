//! xdg-decoration (#EDE-16): a client that leaves the choice to us gets a
//! server-side frame ([`crate::frames`]); one that insists on drawing its
//! own (GTK 4, libadwaita) keeps it.

use smithay::{
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    wayland::shell::xdg::{decoration::XdgDecorationHandler, ToplevelSurface},
};

use crate::state::{set_window_state, window_state, Ede};

impl Ede {
    fn decorate(&mut self, toplevel: &ToplevelSurface, server_side: bool) {
        toplevel.with_pending_state(|s| s.decoration_mode = Some(if server_side { Mode::ServerSide } else { Mode::ClientSide }));
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        if let Some(window) = self.window_for_surface(toplevel.wl_surface()) {
            let mut st = window_state(&window);
            st.decorated = server_side;
            set_window_state(&window, st);
            if self.space.element_location(&window).is_some() {
                if let Some(tile) = st.tile {
                    self.tile(&window, tile);
                }
            }
        }
    }
}

impl XdgDecorationHandler for Ede {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.decorate(&toplevel, true);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        self.decorate(&toplevel, mode != Mode::ClientSide);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.decorate(&toplevel, true);
    }
}
