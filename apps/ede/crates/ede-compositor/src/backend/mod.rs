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
pub fn output_elements<R>(state: &mut Ede, output: &Output, renderer: &mut R) -> Vec<OutputElements<R>>
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
pub fn capture_elements<R>(state: &mut Ede, output: &Output, renderer: &mut R) -> Vec<OutputElements<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    let redact = !state.debug_captures;
    scene(state, output, renderer, redact)
}

fn scene<R>(state: &mut Ede, output: &Output, renderer: &mut R, redact: bool) -> Vec<OutputElements<R>>
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
        return render_elements_from_surface_tree(renderer, &lock, (0, 0), scale, 1.0, Kind::Unspecified)
            .into_iter()
            .map(OutputElements::Surface)
            .collect();
    }

    let mut elements = Vec::new();
    // The recovery screen (#EDE-16) sits above everything but the cursor.
    elements.extend(state.recovery_elements(renderer, output).into_iter().map(OutputElements::Texture));
    let focused = state.focused_window();
    let state = &*state;
    let window = |renderer: &mut R, elements: &mut Vec<_>, w: &smithay::desktop::Window| {
        let Some(loc) = state.space.element_location(w) else { return };
        let loc = loc - w.geometry().loc - out.loc;
        if redact && state.is_protected_window(w) {
            let mut bbox = w.bbox();
            bbox.loc += loc;
            bbox.loc.y -= state.frame_height(w);
            bbox.size.h += state.frame_height(w);
            let geo = bbox.to_physical_precise_round(scale);
            elements.push(OutputElements::Solid(SolidColorRenderElement::new(Id::new(), geo, CommitCounter::default(), [0.0, 0.0, 0.0, 1.0], Kind::Unspecified)));
            return;
        }
        // The frame (Tier A) at the window's own place, above its surface.
        let window_loc = loc + w.geometry().loc;
        elements.extend(state.frame_elements(renderer, w, window_loc, scale, focused.as_ref() == Some(w), redact).into_iter().map(OutputElements::Texture));
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
    // The popups of a layer (the panel's menu) sit above every surface of
    // that layer, not only their parent: a toast mapped after the panel
    // must not cover the panel's open menu (#EDE-12).
    let layer = |renderer: &mut R, elements: &mut Vec<_>, upper: bool| {
        use smithay::desktop::PopupManager;
        let on_layer = || layers.layers().rev().filter(|l| matches!(l.layer(), Layer::Overlay | Layer::Top) == upper);
        for l in on_layer() {
            let Some(geo) = layers.layer_geometry(l) else { continue };
            for (popup, location) in PopupManager::popups_for_surface(l.wl_surface()) {
                let offset = (geo.loc + location - popup.geometry().loc).to_physical_precise_round(scale);
                elements.extend(render_elements_from_surface_tree(renderer, popup.wl_surface(), offset, scale, 1.0, Kind::Unspecified).into_iter().map(OutputElements::Surface));
            }
        }
        for l in on_layer() {
            let Some(geo) = layers.layer_geometry(l) else { continue };
            elements.extend(
                render_elements_from_surface_tree(renderer, l.wl_surface(), geo.loc.to_physical_precise_round(scale), scale, 1.0, Kind::Unspecified)
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
    state: &mut Ede,
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
    state: &mut Ede,
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
pub fn draw_output(renderer: &mut smithay::backend::renderer::pixman::PixmanRenderer, state: &mut Ede, output: &Output, shared: bool) -> Result<headless::Frame, String> {
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
pub fn capture_output(state: &mut Ede, output: &Output) -> Result<headless::Frame, String> {
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

/// #LOGS-2: the main loop's heartbeat for the hang watchdog
/// (`ede_observability::LoopWatch`): a timer every `TICK`, inserted only
/// when diagnostics or `EXP_FRAME_STATS=1` listen (an idle desktop stays
/// idle otherwise).
pub fn watch_main_loop(handle: &smithay::reexports::calloop::LoopHandle<'static, Ede>) {
    use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
    let Some(mut watch) = ede_observability::LoopWatch::start() else { return };
    let inserted = handle.insert_source(Timer::from_duration(ede_observability::TICK), move |_, _, _| {
        watch.tick();
        TimeoutAction::ToDuration(ede_observability::TICK)
    });
    if let Err(e) = inserted {
        tracing::warn!("main-loop watchdog: {e}");
    }
}

/// After a frame went out: frame callbacks so clients draw the next one,
/// then housekeeping and a flush.
pub fn frame_done(state: &mut Ede, output: &Output) {
    state.frame_serial += 1;
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
    /// (`Memory` = the cursor, a window frame or the recovery screen;
    /// `Solid` = a protected window in a capture).
    pub OutputElements<R> where R: ImportAll + ImportMem;
    Memory=smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement<R>,
    Texture=smithay::backend::renderer::element::texture::TextureRenderElement<<R as smithay::backend::renderer::RendererSuper>::TextureId>,
    Surface=WaylandSurfaceRenderElement<R>,
    Solid=smithay::backend::renderer::element::solid::SolidColorRenderElement,
}

/// Where a cursor image goes on an output of `scale`: the pointer at `pos`
/// (output-local, logical) less the image's `hotspot` (logical). Returns the
/// image's top-left in the output's physical pixels and the hotspot in
/// physical pixels, the plane's HOTSPOT_X/Y (a host cursor in a VM puts it
/// under the host pointer). Both are in physical pixels: the elements are
/// laid out in them, so a logical position at a scale above 1 drew the
/// cursor towards the top left.
pub fn cursor_placement(
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
    hotspot: smithay::utils::Point<f64, smithay::utils::Logical>,
    scale: f64,
) -> (smithay::utils::Point<i32, smithay::utils::Physical>, smithay::utils::Point<i32, smithay::utils::Physical>) {
    let hotspot = hotspot.to_physical(scale).to_i32_round();
    (pos.to_physical(scale).to_i32_round() - hotspot, hotspot)
}

/// The scale an xcursor image is picked and uploaded at on an output of
/// `scale`: the next whole one (1.5 takes the 2x image, drawn at 1.5).
pub fn cursor_buffer_scale(scale: f64) -> i32 {
    (scale.ceil() as i32).max(1)
}

/// The pointer at `pos` (output-local, logical) on an output of `scale`:
/// the client's cursor surface if it set one, nothing if it hid it, else
/// `fallback` (uploaded at `cursor_buffer_scale`) with its hotspot in
/// logical pixels. Also returns the hotspot in physical pixels (`None` =
/// no cursor).
#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub fn cursor_elements<R>(
    state: &Ede,
    renderer: &mut R,
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
    scale: f64,
    fallback: &smithay::backend::renderer::element::memory::MemoryRenderBuffer,
    hotspot: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> (Vec<OutputElements<R>>, Option<smithay::utils::Point<i32, smithay::utils::Physical>>)
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
        CursorImageStatus::Hidden => (vec![], None),
        CursorImageStatus::Surface(surface) => {
            let hotspot = with_states(surface, |s| {
                s.data_map.get::<std::sync::Mutex<CursorImageAttributes>>().map(|a| a.lock().unwrap().hotspot).unwrap_or_default()
            });
            let (at, hotspot) = cursor_placement(pos, hotspot.to_f64(), scale);
            let elements = render_elements_from_surface_tree(renderer, surface, at, scale, 1.0, Kind::Cursor)
                .into_iter()
                .map(OutputElements::Surface)
                .collect();
            (elements, Some(hotspot))
        }
        CursorImageStatus::Named(_) => {
            let (at, hotspot) = cursor_placement(pos, hotspot, scale);
            let elements = MemoryRenderBufferRenderElement::from_buffer(renderer, at.to_f64(), fallback, None, None, None, Kind::Cursor)
                .map(|e| vec![OutputElements::Memory(e)])
                .unwrap_or_default();
            (elements, Some(hotspot))
        }
    }
}

/// The node a card renders through: the one EGL's device names, else the
/// card's own render node, else the card itself. Mesa's software device
/// (llvmpipe on vmwgfx without 3D, a firmware framebuffer) names no node at
/// all, and the card's render node is where the primary GPU is looked up
/// (#EDE-40: registering the renderer under the card left every frame
/// without one).
pub fn render_node_for<T: Copy>(egl: Option<T>, own_render: Option<T>, card: T) -> T {
    egl.or(own_render).unwrap_or(card)
}

/// The reason the session stops when no card could be driven: every
/// card's own failure, and the usual cause when they read like a locked
/// device (another display server holds the seat's GPUs: eDE started from
/// inside a running desktop).
pub fn no_gpu_message(failures: &[String]) -> String {
    let busy = failures.iter().any(|f| f.contains("ermission denied") || f.contains("EACCES") || f.contains("EBUSY") || f.contains("usy"));
    let hint = if busy || failures.is_empty() { "; is another display server running on this seat? (nested: run inside X11/Wayland without --udev)" } else { "" };
    if failures.is_empty() {
        return format!("no GPU could be opened{hint}");
    }
    format!("no GPU could be driven: {}{hint}", failures.join("; "))
}

/// `Some(node)` when `primary` is not a node a renderer was registered
/// under: the first registered one renders instead (`None` = fine as is).
pub fn reconcile_primary<T: Copy + PartialEq>(primary: T, registered: &[T]) -> Option<T> {
    if registered.contains(&primary) {
        None
    } else {
        registered.first().copied()
    }
}

/// The mode an output switches to after its connector changed while
/// connected (hotplug with a new mode list: a VM window resized, a monitor
/// re-reading its EDID): the `[[outputs]]` mode when the connector still
/// offers it (`pinned`), else the connector's new `preferred` mode; `None`
/// when that is the `current` mode already.
pub fn mode_to_follow(current: Option<smithay::output::Mode>, preferred: smithay::output::Mode, pinned: Option<smithay::output::Mode>) -> Option<smithay::output::Mode> {
    let wanted = pinned.unwrap_or(preferred);
    (current != Some(wanted)).then_some(wanted)
}

/// Whether the outputs ever drew (udev, #EDE-40). A session whose outputs
/// get no frame at all would stay black forever with the log filling up;
/// after [`FATAL_FAILURES`] attempts without one frame it stops itself with
/// the reason, and exponential-session shows it and falls back.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrameHealth {
    pub drawn: bool,
    pub failures: u32,
}

pub const FATAL_FAILURES: u32 = 20;
/// After the first, a repeated failure is logged this often.
const LOG_EVERY: u32 = 600;

/// What to do with one failed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Log,
    Quiet,
    /// Nothing was ever drawn and this keeps happening: stop the session.
    Fatal,
}

impl FrameHealth {
    pub fn drew(&mut self) {
        self.drawn = true;
        self.failures = 0;
    }

    pub fn failed(&mut self, _error: &str) -> Failure {
        self.failures += 1;
        if !self.drawn && self.failures >= FATAL_FAILURES {
            Failure::Fatal
        } else if self.failures == 1 || self.failures % LOG_EVERY == 0 {
            Failure::Log
        } else {
            Failure::Quiet
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_sits_under_the_pointer_at_every_scale() {
        // 4K at 200%: the pointer at logical (960, 540) is physical (1920, 1080).
        let (at, hot) = cursor_placement((960.0, 540.0).into(), (4.0, 4.0).into(), 2.0);
        assert_eq!((at.x, at.y, hot.x, hot.y), (1912, 1072, 8, 8));
        assert_eq!(at + hot, (1920, 1080).into(), "the hotspot lands exactly on the pointer");
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 3.0] {
            for (pos, hotspot) in [((0.0, 0.0), (0.0, 0.0)), ((1280.4, 719.6), (4.0, 3.0)), ((10.0, 3000.0), (12.5, 12.5))] {
                let (at, hot) = cursor_placement(pos.into(), hotspot.into(), scale);
                let pointer = smithay::utils::Point::<f64, smithay::utils::Logical>::from(pos).to_physical(scale).to_i32_round();
                assert_eq!(at + hot, pointer, "scale {scale}, pointer {pos:?}");
                assert_eq!(hot, smithay::utils::Point::<f64, smithay::utils::Logical>::from(hotspot).to_physical(scale).to_i32_round());
            }
        }
        // At 100% nothing moves (the case that always looked right).
        let (at, hot) = cursor_placement((100.0, 50.0).into(), (1.0, 1.0).into(), 1.0);
        assert_eq!((at.x, at.y, hot.x, hot.y), (99, 49, 1, 1));
    }

    #[test]
    fn an_xcursor_image_is_picked_at_the_next_whole_scale() {
        assert_eq!(cursor_buffer_scale(1.0), 1);
        assert_eq!(cursor_buffer_scale(1.25), 2);
        assert_eq!(cursor_buffer_scale(1.5), 2);
        assert_eq!(cursor_buffer_scale(2.0), 2);
        assert_eq!(cursor_buffer_scale(2.5), 3);
        assert_eq!(cursor_buffer_scale(0.5), 1);
    }

    #[test]
    fn a_card_renders_through_egls_node_else_its_own_render_node_else_itself() {
        assert_eq!(render_node_for(Some("renderD129"), Some("renderD128"), "card0"), "renderD129");
        // Mesa's software device names no node: the card's render node, where the primary GPU lives.
        assert_eq!(render_node_for(None, Some("renderD128"), "card0"), "renderD128");
        assert_eq!(render_node_for(None, None, "card0"), "card0");
    }

    #[test]
    fn a_changed_connector_follows_its_new_preferred_mode_unless_the_config_pins_one() {
        let mode = |w, h| smithay::output::Mode { size: (w, h).into(), refresh: 60_000 };
        // VirtualBox: the window grew, vmwgfx's preferred mode followed.
        assert_eq!(mode_to_follow(Some(mode(2560, 1440)), mode(3942, 2148), None), Some(mode(3942, 2148)));
        // The same layout pushed again (VBoxDRMClient repeats it): nothing to do.
        assert_eq!(mode_to_follow(Some(mode(3942, 2148)), mode(3942, 2148), None), None);
        // `[[outputs]]` pins a mode the connector still offers: it stays.
        assert_eq!(mode_to_follow(Some(mode(1920, 1080)), mode(3942, 2148), Some(mode(1920, 1080))), None);
        // The pinned mode went away with the old window size: the preferred one.
        assert_eq!(mode_to_follow(Some(mode(3200, 1824)), mode(1280, 800), None), Some(mode(1280, 800)));
        // A pinned mode that just appeared (the window is that size again) wins.
        assert_eq!(mode_to_follow(Some(mode(1280, 800)), mode(3200, 1824), Some(mode(3200, 1824))), Some(mode(3200, 1824)));
        assert_eq!(mode_to_follow(None, mode(1280, 800), None), Some(mode(1280, 800)));
    }

    #[test]
    fn the_no_gpu_message_carries_every_cards_reason() {
        let broken = no_gpu_message(&["/dev/dri/card2: nvidia-drm renders in software only (Mesa's llvmpipe): …".into()]);
        assert!(broken.starts_with("no GPU could be driven: /dev/dri/card2: nvidia-drm renders in software"), "{broken}");
        assert!(!broken.contains("another display server"), "a driver failure is not a busy seat: {broken}");
        let busy = no_gpu_message(&["/dev/dri/card0: Permission denied (os error 13)".into()]);
        assert!(busy.contains("another display server"), "{busy}");
        assert!(no_gpu_message(&[]).contains("another display server"));
    }

    #[test]
    fn the_primary_gpu_is_one_the_renderers_know() {
        assert_eq!(reconcile_primary("renderD128", &["renderD128"]), None);
        assert_eq!(reconcile_primary("renderD128", &["card0", "renderD129"]), Some("card0"));
        assert_eq!(reconcile_primary("renderD128", &[]), None, "no devices: the caller already fails");
    }

    #[test]
    fn a_session_that_never_draws_stops_after_twenty_failed_frames() {
        let mut h = FrameHealth::default();
        assert_eq!(h.failed("x"), Failure::Log, "the first failure is logged");
        for _ in 1..FATAL_FAILURES - 1 {
            assert_eq!(h.failed("x"), Failure::Quiet);
        }
        assert_eq!(h.failed("x"), Failure::Fatal);
    }

    #[test]
    fn a_session_that_drew_once_only_logs_later_failures() {
        let mut h = FrameHealth::default();
        h.drew();
        assert_eq!(h.failed("x"), Failure::Log);
        for _ in 0..5 * FATAL_FAILURES {
            assert_ne!(h.failed("x"), Failure::Fatal);
        }
        h.drew();
        assert_eq!(h.failures, 0);
        assert_eq!(h.failed("x"), Failure::Log);
    }
}
