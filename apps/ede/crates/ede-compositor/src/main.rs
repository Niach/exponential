//! `ede-compositor [--headless WxH] [-- command args…]`
//!
//! Nested by default: opens a window on the running X11 or Wayland session
//! and serves `$XDG_RUNTIME_DIR/wayland-N`. Anything after `--` starts as a
//! client with `WAYLAND_DISPLAY` pointing at us.

use std::process::Command;

use ede_compositor::{Config, Ede};
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let mut args = std::env::args().skip(1);
    let mut headless: Option<(i32, i32)> = None;
    let mut config_path = Config::default_path();
    let mut command: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => {
                let size = args.next().ok_or("--headless needs WxH")?;
                let (w, h) = size.split_once('x').ok_or("--headless needs WxH")?;
                headless = Some((w.parse()?, h.parse()?));
            }
            "--config" => config_path = Some(args.next().ok_or("--config needs a path")?.into()),
            "--" => command.extend(args.by_ref()),
            "-h" | "--help" => {
                println!("usage: ede-compositor [--headless WxH] [--config PATH] [-- command args…]");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other:?} (try --help)").into()),
        }
    }

    let config = match &config_path {
        Some(path) => {
            let (config, err) = Config::load(path);
            if let Some(err) = err {
                tracing::warn!("config ignored: {err}");
            }
            config
        }
        None => Config::default(),
    };

    let mut event_loop: EventLoop<'static, Ede> = EventLoop::try_new()?;
    let display: Display<Ede> = Display::new()?;
    let mut state = Ede::new(display, event_loop.handle(), event_loop.get_signal(), config);
    let socket = state.listen()?;
    tracing::info!("listening on {}", socket.to_string_lossy());

    // Kept alive for the whole run: the headless backend owns the outputs.
    let _headless = match headless {
        Some(size) => {
            let mut h = ede_compositor::backend::headless::Headless {
                renderer: smithay::backend::renderer::pixman::PixmanRenderer::new()?,
                outputs: Vec::new(),
            };
            h.add_output(&mut state, "HEADLESS-1", size);
            let outputs = h.outputs.clone();
            event_loop.handle().insert_source(
                smithay::reexports::calloop::timer::Timer::from_duration(std::time::Duration::from_millis(16)),
                move |_, _, state| {
                    for o in &outputs {
                        ede_compositor::backend::frame_done(state, o);
                    }
                    smithay::reexports::calloop::timer::TimeoutAction::ToDuration(std::time::Duration::from_millis(16))
                },
            )?;
            Some(h)
        }
        #[cfg(feature = "winit")]
        None => {
            ede_compositor::backend::winit::init(&mut event_loop, &mut state)?;
            None
        }
        #[cfg(not(feature = "winit"))]
        None => return Err("built without the winit backend: pass --headless WxH".into()),
    };

    if let Some((program, rest)) = command.split_first() {
        // Our socket only: never hand the child the parent session's display.
        Command::new(program).args(rest).env("WAYLAND_DISPLAY", &socket).env_remove("DISPLAY").spawn()?;
    }

    event_loop.run(None, &mut state, |state| {
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}
