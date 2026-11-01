//! wlr-layer-shell: the panel, wallpaper, OSD and notifications of the
//! future `ede-shell` are layer surfaces; exclusive zones shrink the work
//! area windows place and tile into.

use smithay::{
    delegate_layer_shell,
    desktop::{layer_map_for_output, LayerSurface, WindowSurfaceType},
    output::Output,
    reexports::wayland_server::protocol::{wl_output, wl_surface::WlSurface},
    wayland::{
        compositor::with_states,
        shell::wlr_layer::{Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState},
    },
};

use crate::state::Ede;

impl WlrLayerShellHandler for Ede {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(&mut self, surface: WlrLayerSurface, wl_output: Option<wl_output::WlOutput>, _layer: Layer, namespace: String) {
        let Some(output) = wl_output.as_ref().and_then(Output::from_resource).or_else(|| self.space.outputs().next().cloned()) else {
            // No output yet: nothing can show it.
            surface.send_close();
            return;
        };
        let mut map = layer_map_for_output(&output);
        if let Err(e) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            tracing::warn!("layer surface refused: {e}");
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let found = self.space.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer = map.layers().find(|l| l.layer_surface() == &surface).cloned();
            layer.map(|l| (o.clone(), l))
        });
        if let Some((output, layer)) = found {
            let mut map = layer_map_for_output(&output);
            map.unmap_layer(&layer);
        }
    }
}
delegate_layer_shell!(Ede);

pub fn handle_commit(state: &mut Ede, surface: &WlSurface) {
    let Some(output) = state
        .space
        .outputs()
        .find(|o| layer_map_for_output(o).layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).is_some())
        .cloned()
    else {
        return;
    };
    let initial_configure_sent =
        with_states(surface, |states| states.data_map.get::<LayerSurfaceData>().unwrap().lock().unwrap().initial_configure_sent);
    let mut map = layer_map_for_output(&output);
    // Arrange first so the initial configure carries the size the client asked for.
    map.arrange();
    if !initial_configure_sent {
        if let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL) {
            layer.layer_surface().send_configure();
        }
    }
}
