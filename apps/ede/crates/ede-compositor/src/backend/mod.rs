//! Backends own the outputs and the renderer; everything else is [`Ede`].
//!
//! - `winit`: nested in a window on X11 or Wayland, for development.
//! - `headless`: virtual outputs drawn by pixman into memory, for CI.
//! - `udev`: the real session (DRM/KMS, libinput), behind the `udev` feature.

#[cfg(feature = "udev")]
mod cursor;
pub mod headless;
#[cfg(feature = "udev")]
pub mod udev;
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

smithay::backend::renderer::element::render_elements! {
    /// Everything an output shows: the cursor on top, then layers + windows.
    pub OutputElements<R, E> where R: ImportAll + ImportMem;
    Space=smithay::desktop::space::SpaceRenderElements<R, E>,
    Cursor=smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement<R>,
    CursorSurface=WaylandSurfaceRenderElement<R>,
}

/// The pointer at `pos` (output-local): the client's cursor surface if it
/// set one, nothing if it hid it, else `fallback` with its hotspot.
#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub fn cursor_elements<R>(
    state: &Ede,
    renderer: &mut R,
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
    fallback: &smithay::backend::renderer::element::memory::MemoryRenderBuffer,
    hotspot: (i32, i32),
) -> Vec<OutputElements<R, WaylandSurfaceRenderElement<R>>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    use smithay::{
        backend::renderer::element::{memory::MemoryRenderBufferRenderElement, surface::render_elements_from_surface_tree, Kind},
        input::pointer::{CursorImageAttributes, CursorImageStatus},
        wayland::compositor::with_states,
    };
    match &state.cursor_status {
        CursorImageStatus::Hidden => vec![],
        CursorImageStatus::Surface(surface) => {
            let hotspot = with_states(surface, |s| {
                s.data_map.get::<std::sync::Mutex<CursorImageAttributes>>().map(|a| a.lock().unwrap().hotspot).unwrap_or_default()
            });
            let at = (pos - hotspot.to_f64()).to_physical(1.0).to_i32_round();
            render_elements_from_surface_tree(renderer, surface, at, 1.0, 1.0, Kind::Cursor)
                .into_iter()
                .map(OutputElements::CursorSurface)
                .collect()
        }
        CursorImageStatus::Named(_) => {
            let at = (pos - smithay::utils::Point::from((hotspot.0 as f64, hotspot.1 as f64))).to_physical(1.0);
            MemoryRenderBufferRenderElement::from_buffer(renderer, at, fallback, None, None, None, Kind::Cursor)
                .map(|e| vec![OutputElements::Cursor(e)])
                .unwrap_or_default()
        }
    }
}
