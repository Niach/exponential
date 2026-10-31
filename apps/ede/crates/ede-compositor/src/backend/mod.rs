//! Backends own the outputs and the renderer; everything else is [`Ede`].
//!
//! - `winit`: nested in a window on X11 or Wayland, for development.
//! - `headless`: virtual outputs drawn by pixman into memory, for CI.

pub mod headless;
#[cfg(feature = "winit")]
pub mod winit;

use std::time::Duration;

use smithay::{
    backend::renderer::{
        damage::{Error as DamageError, OutputDamageTracker, RenderOutputResult},
        element::surface::WaylandSurfaceRenderElement,
        ImportAll, ImportMem, Renderer,
    },
    desktop::{layer_map_for_output, space::render_output},
    output::Output,
};

use crate::state::Ede;

/// Draws `output` (layers + windows, damage-tracked) into `framebuffer`.
pub fn render<'a, 'd, R>(
    state: &Ede,
    output: &Output,
    renderer: &mut R,
    framebuffer: &mut R::Framebuffer<'a>,
    age: usize,
    damage_tracker: &'d mut OutputDamageTracker,
) -> Result<RenderOutputResult<'d>, DamageError<R::Error>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    let clear = state.config.background_rgba().unwrap_or([0.04, 0.04, 0.04, 1.0]);
    render_output::<_, WaylandSurfaceRenderElement<R>, _, _>(output, renderer, framebuffer, 1.0, age, [&state.space], &[], damage_tracker, clear)
}

/// After a frame went out: frame callbacks so clients draw the next one,
/// then housekeeping and a flush.
pub fn frame_done(state: &mut Ede, output: &Output) {
    let now = state.start_time.elapsed();
    for window in state.space.elements() {
        window.send_frame(output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
    for layer in layer_map_for_output(output).layers() {
        layer.send_frame(output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
    state.space.refresh();
    state.popups.cleanup();
    layer_map_for_output(output).cleanup();
    let _ = state.display_handle.flush_clients();
}
