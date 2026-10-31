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
    desktop::layer_map_for_output,
    output::Output,
};

use crate::state::Ede;

/// What `output` shows, front to back: the lock surface while the session
/// is locked, a fullscreen window alone, else the layers and windows.
pub fn output_elements<R>(state: &Ede, output: &Output, renderer: &mut R) -> Vec<OutputElements<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    scene(state, output, renderer, false)
}

/// What anything outside the person's own monitors may see of `output`
/// (screen capture, a lent view, an agent's screenshot): the same scene, but
/// protected windows are solid black and a locked session is black
/// altogether. Every capture path goes through here (#NEW-106).
pub fn capture_elements<R>(state: &Ede, output: &Output, renderer: &mut R) -> Vec<OutputElements<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    scene(state, output, renderer, true)
}

fn scene<R>(state: &Ede, output: &Output, renderer: &mut R, redact: bool) -> Vec<OutputElements<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    use smithay::{
        backend::renderer::element::{solid::SolidColorRenderElement, surface::render_elements_from_surface_tree, AsRenderElements, Id, Kind},
        backend::renderer::utils::CommitCounter,
        wayland::shell::wlr_layer::Layer,
    };
    let Some(out) = state.space.output_geometry(output) else { return vec![] };
    let scale = output.current_scale().fractional_scale();
    if state.is_locked() {
        if redact {
            return vec![];
        }
        let Some(lock) = state.lock_surface_on(output) else { return vec![] };
        return render_elements_from_surface_tree(renderer, lock.wl_surface(), (0, 0), scale, 1.0, Kind::Unspecified)
            .into_iter()
            .map(OutputElements::Surface)
            .collect();
    }

    let mut elements = Vec::new();
    let window = |renderer: &mut R, elements: &mut Vec<_>, w: &smithay::desktop::Window| {
        let Some(loc) = state.space.element_location(w) else { return };
        let loc = loc - w.geometry().loc - out.loc;
        if redact && state.is_protected_window(w) {
            let mut bbox = w.bbox();
            bbox.loc += loc;
            let geo = bbox.to_physical_precise_round(scale);
            elements.push(OutputElements::Solid(SolidColorRenderElement::new(Id::new(), geo, CommitCounter::default(), [0.0, 0.0, 0.0, 1.0], Kind::Unspecified)));
            return;
        }
        elements.extend(
            AsRenderElements::<R>::render_elements::<WaylandSurfaceRenderElement<R>>(w, renderer, loc.to_physical_precise_round(scale), scale.into(), 1.0)
                .into_iter()
                .map(OutputElements::Surface),
        );
    };
    if let Some(w) = state.fullscreen_on(output) {
        window(renderer, &mut elements, &w);
        return elements;
    }

    let layers = layer_map_for_output(output);
    let layer = |renderer: &mut R, elements: &mut Vec<_>, upper: bool| {
        for l in layers.layers().rev().filter(|l| matches!(l.layer(), Layer::Overlay | Layer::Top) == upper) {
            let Some(geo) = layers.layer_geometry(l) else { continue };
            elements.extend(
                AsRenderElements::<R>::render_elements::<WaylandSurfaceRenderElement<R>>(l, renderer, geo.loc.to_physical_precise_round(scale), scale.into(), 1.0)
                    .into_iter()
                    .map(OutputElements::Surface),
            );
        }
    };
    layer(renderer, &mut elements, true);
    for w in state.space.elements().rev() {
        let Some(bbox) = state.space.element_bbox(w) else { continue };
        if out.overlaps(bbox) {
            window(renderer, &mut elements, w);
        }
    }
    layer(renderer, &mut elements, false);
    elements
}

/// Draws `output` (damage-tracked) into `framebuffer`.
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
    let elements = output_elements(state, output, renderer);
    damage_tracker.render_output(renderer, framebuffer, age, &elements, state.clear_color())
}

/// Draws what a capture of `output` may see ([`capture_elements`]).
pub fn render_capture<'a, 'd, R>(
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
    let elements = capture_elements(state, output, renderer);
    damage_tracker.render_output(renderer, framebuffer, age, &elements, state.clear_color())
}

/// Draws `output` in software (pixman) into memory: `shared` = what a
/// capture may see ([`capture_elements`]).
pub fn draw_output(renderer: &mut smithay::backend::renderer::pixman::PixmanRenderer, state: &Ede, output: &Output, shared: bool) -> Result<headless::Frame, String> {
    let mode = output.current_mode().ok_or("output has no mode")?;
    let size = (mode.size.w, mode.size.h);
    draw_offscreen(renderer, size, state.clear_color(), |renderer| {
        if shared {
            capture_elements(state, output, renderer)
        } else {
            output_elements(state, output, renderer)
        }
    })
}

fn draw_offscreen(
    renderer: &mut smithay::backend::renderer::pixman::PixmanRenderer,
    (w, h): (i32, i32),
    clear: [f32; 4],
    elements: impl FnOnce(&mut smithay::backend::renderer::pixman::PixmanRenderer) -> Vec<OutputElements<smithay::backend::renderer::pixman::PixmanRenderer>>,
) -> Result<headless::Frame, String> {
    use smithay::{
        backend::{
            allocator::Fourcc,
            renderer::{Bind, Offscreen},
        },
        utils::{Physical, Rectangle, Transform},
    };
    let size: smithay::utils::Size<i32, smithay::utils::Buffer> = (w, h).into();
    let mut image = Offscreen::<smithay::reexports::pixman::Image<'static, 'static>>::create_buffer(renderer, Fourcc::Argb8888, size).map_err(|e| e.to_string())?;
    {
        let elements = elements(renderer);
        let mut fb = renderer.bind(&mut image).map_err(|e| e.to_string())?;
        let mut damage_tracker = OutputDamageTracker::new(Rectangle::<i32, Physical>::from_size((w, h).into()).size, 1.0, Transform::Normal);
        damage_tracker.render_output(renderer, &mut fb, 0, &elements, clear).map_err(|e| format!("{e:?}"))?;
    }
    let stride = image.stride() / 4;
    // Safety: the image is ours, `h` rows of `stride` u32s, and no renderer
    // holds it any more (the framebuffer above is dropped).
    let data = unsafe { std::slice::from_raw_parts(image.data(), stride * h as usize) };
    let pixels = data.chunks(stride).flat_map(|row| row[..w as usize].iter().copied()).collect();
    Ok(headless::Frame { width: w, height: h, pixels })
}

/// An output as `ede-protocol` captures it, drawn in software. Clients
/// whose buffers pixman cannot read (a GPU-only dmabuf) are missing from it
/// until the backends read back from their own renderer.
pub fn capture_output(state: &Ede, output: &Output) -> Result<headless::Frame, String> {
    let mut renderer = smithay::backend::renderer::pixman::PixmanRenderer::new().map_err(|e| e.to_string())?;
    draw_output(&mut renderer, state, output, true)
}

/// One window alone (popups included) at its own size, on transparent black.
/// The caller checks it may be captured.
pub fn capture_window(window: &smithay::desktop::Window) -> Result<headless::Frame, String> {
    use smithay::{backend::renderer::element::AsRenderElements, desktop::space::SpaceElement};
    let mut renderer = smithay::backend::renderer::pixman::PixmanRenderer::new().map_err(|e| e.to_string())?;
    let bbox = SpaceElement::bbox(window);
    let size = (bbox.size.w.max(1), bbox.size.h.max(1));
    draw_offscreen(&mut renderer, size, [0.0, 0.0, 0.0, 0.0], |renderer| {
        let at = smithay::utils::Point::<i32, smithay::utils::Logical>::from((-bbox.loc.x, -bbox.loc.y)).to_physical(1);
        AsRenderElements::<_>::render_elements::<WaylandSurfaceRenderElement<_>>(window, renderer, at, 1.0.into(), 1.0)
            .into_iter()
            .map(OutputElements::Surface)
            .collect()
    })
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
    state.lock_frame_done(output, now);
    state.space.refresh();
    state.popups.cleanup();
    layer_map_for_output(output).cleanup();
    let _ = state.display_handle.flush_clients();
}

smithay::backend::renderer::element::render_elements! {
    /// Everything an output shows: the cursor on top, then layers + windows
    /// (`Solid` = a protected window in a capture).
    pub OutputElements<R> where R: ImportAll + ImportMem;
    Cursor=smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement<R>,
    Surface=WaylandSurfaceRenderElement<R>,
    Solid=smithay::backend::renderer::element::solid::SolidColorRenderElement,
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
) -> Vec<OutputElements<R>>
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
                .map(OutputElements::Surface)
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
