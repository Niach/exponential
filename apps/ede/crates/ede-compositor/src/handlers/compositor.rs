use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler, CompositorState},
        shm::{ShmHandler, ShmState},
    },
};

use super::{layer_shell, xdg_shell};
use crate::{
    grabs::resize,
    state::{ClientState, Ede},
};

impl CompositorHandler for Ede {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().expect("every client is inserted with ClientState").compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.window_for_surface(&root) {
                window.on_commit();
            }
        }

        xdg_shell::handle_commit(self, surface);
        layer_shell::handle_commit(self, surface);
        resize::handle_commit(&mut self.space, surface);
    }
}

impl BufferHandler for Ede {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for Ede {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(Ede);
delegate_shm!(Ede);
