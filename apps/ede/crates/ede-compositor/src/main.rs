//! `exponential-compositor [--udev | --headless WxH] [--no-xwayland] [--safe-mode] [--config PATH] [--debug-captures] [-- command args…]`
//! `--debug-captures`: ede-protocol captures show protected surfaces and the
//! lock (pictures of them in a VM); refused with --udev, a real screen is
//! never captured unredacted.
//! `exponential-compositor doctor`: what this machine is missing for eDE.
//! `exponential-compositor recovery --reason … [--next …]`: the message a failed
//! session leaves on the displays, without a GPU ([`ede_compositor::fallback`]).
//!
//! A run that cannot drive the display ends with [`FATAL_EXIT`] and the
//! reason in [`failure_file`] (#EDE-40), which exponential-session shows; a crash
//! ends any other way.
//!
//! Inside a running X11 or Wayland session it nests in a window (winit);
//! from a TTY (neither `DISPLAY` nor `WAYLAND_DISPLAY` set) it takes the
//! seat itself (udev). Either way it serves `$XDG_RUNTIME_DIR/wayland-N`, and
//! anything after `--` starts as a client with `WAYLAND_DISPLAY` pointing at
//! us.

use ede_compositor::{Config, Ede};
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

/// The exit code of a session that stopped itself: the reason is in
/// [`failure_file`], and the same start would fail the same way.
pub const FATAL_EXIT: i32 = 3;

/// `$XDG_RUNTIME_DIR/exponential-desktop/failure`: why the last run could not drive the
/// display (exponential-session reads and removes it).
#[cfg(feature = "udev")]
fn failure_file() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("exponential-desktop").join("failure"))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("exponential-compositor: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let doctor = args.peek().map(String::as_str) == Some("doctor");
    let recovery = args.peek().map(String::as_str) == Some("recovery");
    // The doctor's table stands alone: smithay's renderer chatter (and its
    // warning that a render node has no DRM master) stays behind RUST_LOG.
    let default_log = if doctor { "error" } else { "info" };
    // Logs go to stderr: the doctor's stdout is its table or its JSON (a
    // RUST_LOG inherited from a unit must not end up in front of it). The
    // doctor reports nothing to Sentry; every other run does when
    // diagnostics are on (#LOGS-2, ede-observability).
    let _observability = if doctor {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| default_log.into()))
            .with_writer(std::io::stderr)
            .init();
        None
    } else {
        Some(ede_observability::init("compositor", default_log, ede_observability::Output::Stderr))
    };

    if doctor {
        let checks = ede_compositor::doctor::run_checks();
        // `doctor --json`: what Settings shows (#EDE-35, exponential-services' about domain).
        let ok = if std::env::args().any(|a| a == "--json") {
            println!("{}", ede_compositor::doctor::to_json(&checks));
            !checks.iter().any(|c| c.level == ede_compositor::doctor::Level::Fail)
        } else {
            ede_compositor::doctor::report(&checks)
        };
        std::process::exit(if ok { 0 } else { 1 });
    }
    if recovery {
        let rest: Vec<String> = args.skip(1).collect();
        return ede_compositor::fallback::main(&rest).map_err(Into::into);
    }
    let mut headless: Option<(i32, i32)> = None;
    let mut no_xwayland = false;
    let mut safe_mode = false;
    let mut debug_captures = false;
    let mut udev = std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none();
    let mut config_path = Config::default_path();
    let mut command: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => {
                let size = args.next().ok_or("--headless needs WxH")?;
                let (w, h) = size.split_once('x').ok_or("--headless needs WxH")?;
                headless = Some((w.parse()?, h.parse()?));
            }
            "--udev" => udev = true,
            "--no-xwayland" => no_xwayland = true,
            // Stock settings: the config file is not read (exponential-session's
            // crash recovery, or the "safe mode" login entry).
            "--safe-mode" => safe_mode = true,
            "--debug-captures" => debug_captures = true,
            "--config" => config_path = Some(args.next().ok_or("--config needs a path")?.into()),
            "--" => command.extend(args.by_ref()),
            "-h" | "--help" => {
                println!("usage: exponential-compositor [--udev | --headless WxH] [--no-xwayland] [--safe-mode] [--config PATH] [-- command args…]\n       exponential-compositor doctor [--json]\n       exponential-compositor recovery --reason TEXT [--next TEXT] [--timeout SECS] [--png PATH --size WxH]");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other:?} (try --help)").into()),
        }
    }

    if safe_mode {
        config_path = None;
    }
    let mut config = match &config_path {
        Some(path) => {
            let (config, err) = Config::load(path);
            if let Some(err) = err {
                tracing::warn!("config ignored: {err}");
            }
            config
        }
        None => Config::default(),
    }
    .with_system_keyboard();
    if no_xwayland {
        config.xwayland = false;
    }

    if debug_captures && udev && headless.is_none() {
        return Err("--debug-captures never applies to a real screen (udev)".into());
    }
    if udev && headless.is_none() {
        #[cfg(feature = "udev")]
        {
            // Safe mode (the flag, or exponential-session's variable) lets a
            // real GPU run in software, so a broken driver still gets a
            // desktop to fix it from (#EDE-59).
            let safe = safe_mode || std::env::var_os("EDE_SAFE_MODE").is_some_and(|v| !v.is_empty());
            if let Err(e) = ede_compositor::backend::udev::run(config, config_path, command, safe) {
                tracing::error!("{e}");
                if let Some(path) = failure_file() {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&path, e.to_string());
                }
                // `exit` skips the guard's drop: send what is queued first.
                ede_observability::flush();
                std::process::exit(FATAL_EXIT);
            }
            return Ok(());
        }
        #[cfg(not(feature = "udev"))]
        return Err("no display to nest in, and built without the udev backend (cargo build --features udev)".into());
    }

    let mut event_loop: EventLoop<'static, Ede> = EventLoop::try_new()?;
    let display: Display<Ede> = Display::new()?;
    let mut state = Ede::new(display, event_loop.handle(), event_loop.get_signal(), config);
    state.config_path = config_path;
    state.debug_captures = debug_captures;
    let socket = state.listen()?;
    tracing::info!("listening on {}", socket.to_string_lossy());
    if !command.is_empty() {
        state.autostart.push(command);
    }

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

    state.start_session();
    ede_compositor::backend::watch_main_loop(&event_loop.handle());

    event_loop.run(None, &mut state, |state| {
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}
