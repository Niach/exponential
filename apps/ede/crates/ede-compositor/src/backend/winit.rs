//! Nested: one winit window = one output. On Cinnamon/X11 (the dev setup)
//! winit opens an X11 window; inside another Wayland compositor it opens a
//! Wayland one.

use smithay::{
    backend::{
        renderer::{damage::OutputDamageTracker, gles::GlesRenderer},
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::Transform,
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

    event_loop.handle().insert_source(winit, move |event, _, state| match event {
        WinitEvent::Resized { size, .. } => {
            let mode = Mode { size, refresh: 60_000 };
            output.change_current_state(Some(mode), None, None, None);
            output.set_preferred(mode);
            smithay::desktop::layer_map_for_output(&output).arrange();
        }
        WinitEvent::Input(event) => state.process_input_event(event),
        WinitEvent::Redraw => {
            let age = backend.buffer_age().unwrap_or(0);
            let damage = {
                let (renderer, mut framebuffer) = match backend.bind() {
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
            // Damage tracking: only the rectangles that changed go out.
            if let Some(damage) = damage {
                if let Err(e) = backend.submit(Some(&damage)) {
                    tracing::warn!("submit failed: {e}");
                }
            }
            super::frame_done(state, &output);
            backend.window().request_redraw();
        }
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        _ => (),
    })?;

    Ok(())
}
