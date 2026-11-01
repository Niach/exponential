//! Nested: one winit window = one output. On Cinnamon/X11 (the dev setup)
//! winit opens an X11 window; inside another Wayland compositor it opens a
//! Wayland one.

use std::{cell::RefCell, rc::Rc, time::Duration};

use smithay::{
    backend::{
        egl::EGLDevice,
        renderer::{damage::OutputDamageTracker, gles::GlesRenderer, ImportDma},
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::{
        timer::{TimeoutAction, Timer},
        EventLoop,
    },
    utils::Transform,
    wayland::dmabuf::DmabufFeedbackBuilder,
};

use crate::state::Ede;

pub fn init(event_loop: &mut EventLoop<'static, Ede>, state: &mut Ede) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init::<GlesRenderer>()?;
    backend.window().set_title("eDE");

    let mode = Mode { size: backend.window_size(), refresh: 60_000 };
    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Exponential".into(), model: "eDE nested".into() },
    );
    let _global = output.create_global::<Ede>(&state.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);
    state.add_output(&output);

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    // linux-dmabuf v4 when the EGL device names its render node, else v3.
    let formats = backend.renderer().dmabuf_formats();
    let node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .ok()
        .and_then(|d| d.try_get_render_node().ok().flatten());
    let feedback = node.and_then(|n| DmabufFeedbackBuilder::new(n.dev_id(), formats.clone()).build().ok());
    let global = match &feedback {
        Some(f) => state.dmabuf_state.create_global_with_default_feedback::<Ede>(&state.display_handle, f),
        None => state.dmabuf_state.create_global::<Ede>(&state.display_handle, formats),
    };
    let _ = global;
    let backend = Rc::new(RefCell::new(backend));
    let importer = backend.clone();
    state.dmabuf_importer = Some(Box::new(move |dmabuf| importer.borrow_mut().renderer().import_dmabuf(dmabuf, None).is_ok()));

    event_loop.handle().insert_source(winit, move |event, _, state| {
        let mut backend_ref = backend.borrow_mut();
        let backend_mut = &mut *backend_ref;
        match event {
        WinitEvent::Resized { size, .. } => {
            let mode = Mode { size, refresh: 60_000 };
            output.change_current_state(Some(mode), None, None, None);
            output.set_preferred(mode);
            smithay::desktop::layer_map_for_output(&output).arrange();
        }
        WinitEvent::Input(event) => state.process_input_event(event),
        WinitEvent::Redraw => {
            let age = backend_mut.buffer_age().unwrap_or(0);
            let damage = {
                let (renderer, mut framebuffer) = match backend_mut.bind() {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!("bind failed: {e}");
                        return;
                    }
                };
                match super::render(state, &output, renderer, &mut framebuffer, age, &mut damage_tracker) {
                    Ok(r) => r.damage.cloned(),
                    Err(e) => {
                        tracing::error!("render failed: {e:?}");
                        None
                    }
                }
            };
            // Damage tracking: only the rectangles that changed go out, and
            // the swap paces the next frame. A clean frame swaps nothing, so
            // look again one refresh later instead of spinning (a spin
            // starves winit's input).
            match damage {
                Some(damage) => {
                    if let Err(e) = backend_mut.submit(Some(&damage)) {
                        tracing::warn!("submit failed: {e}");
                    }
                    backend_mut.window().request_redraw();
                }
                None => {
                    let b = backend.clone();
                    let _ = state.loop_handle.insert_source(Timer::from_duration(Duration::from_millis(16)), move |_, _, _| {
                        b.borrow().window().request_redraw();
                        TimeoutAction::Drop
                    });
                }
            }
            super::frame_done(state, &output);
        }
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        _ => (),
        }
    })?;

    Ok(())
}
