//! `exponential-session [--safe-mode]`: the session the login manager starts.
//! `exponential-session --ready`: run by the compositor once it is up.
//! `exponential-session --device`: exponential-device.service's command; it
//! becomes the Exponential app (its launcher entry) so this machine is an
//! online device for the whole session, unless the CLI daemon's own unit runs.

use std::{
    process::{Child, Command, ExitCode, ExitStatus},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use ede_session::{
    application_dirs, compositor_args, device_start, failure_file, load_crashes, migrate_legacy_dirs, recovery_args, state_file, store_crashes, take_failure, xdg_homes, DeviceStart, Exit, Next, Policy, DAEMON_UNIT, DEVICE_ENV, DEVICE_DESKTOP_ENTRIES,
    EXPORTED_ENV, FATAL_EXIT, SESSION_ENV,
};

/// SIGTERM/SIGHUP/SIGINT arrived: the login manager ends the session.
static TERMINATED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_terminate(_: libc::c_int) {
    TERMINATED.store(true, Ordering::SeqCst);
}

fn main() -> ExitCode {
    // #ERR-12: the crash monitor (this binary) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    // #LOGS-2: the log as before (stdout), plus Sentry when diagnostics are on.
    let _observability = ede_observability::init("session", "info", ede_observability::Output::Stdout);
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--ready") => ready(),
        Some("--device") => device(),
        None | Some("--safe-mode") => {
            ede_observability::watch_native_crashes("session");
            run(!args.is_empty())
        }
        Some(other) => {
            eprintln!("unknown argument {other:?}; usage: exponential-session [--safe-mode|--ready|--device]");
            ExitCode::FAILURE
        }
    }
}

/// The compositor is up: hand its displays to systemd and D-Bus activation,
/// then start the session target (autostart apps, polkit agent, keyring).
fn ready() -> ExitCode {
    let vars: Vec<&str> = EXPORTED_ENV.iter().copied().filter(|v| std::env::var_os(v).is_some()).collect();
    let dbus = Command::new("dbus-update-activation-environment").arg("--systemd").args(&vars).status();
    if !dbus.is_ok_and(|s| s.success()) {
        // Without the D-Bus tool, systemd alone still gets them.
        let _ = Command::new("systemctl").args(["--user", "import-environment"]).args(&vars).status();
    }
    match Command::new("systemctl").args(["--user", "start", "--no-block", "exponential-session.target"]).status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        other => {
            tracing::error!("exponential-session.target did not start: {other:?}");
            ExitCode::FAILURE
        }
    }
}

/// exponential-device.service: become the Exponential app so the machine
/// stays online as a device (the unit restarts it when it crashes). The CLI
/// daemon's own unit (`exponential daemon install`) already starts at login:
/// nothing to do then, and nothing when neither is installed.
fn device() -> ExitCode {
    let daemon_unit = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))
        .is_some_and(|config| config.join("systemd/user").join(DAEMON_UNIT).exists());
    let dirs = application_dirs();
    let entries: Vec<String> = DEVICE_DESKTOP_ENTRIES
        .iter()
        .filter_map(|name| dirs.iter().find_map(|dir| std::fs::read_to_string(dir.join(name)).ok()))
        .collect();
    match device_start(daemon_unit, &entries) {
        DeviceStart::DaemonUnit => {
            tracing::info!("the exponential daemon runs as its own unit ({DAEMON_UNIT}); nothing to start");
            ExitCode::SUCCESS
        }
        DeviceStart::Nothing => {
            tracing::info!("no Exponential app or daemon is installed for this user; this machine is not a device");
            ExitCode::SUCCESS
        }
        DeviceStart::App(argv) => {
            use std::os::unix::process::CommandExt as _;
            tracing::info!("starting the Exponential app: {}", argv.join(" "));
            // #EDE-64: the app runs as the session's device: no window at
            // login, and closing its last window keeps it online.
            let err = Command::new(&argv[0]).args(&argv[1..]).env(DEVICE_ENV, "1").exec();
            tracing::error!("{}: {err}", argv[0]);
            ExitCode::FAILURE
        }
    }
}

fn run(mut safe_mode: bool) -> ExitCode {
    // SAFETY: installing a handler that only stores a flag.
    unsafe {
        for signal in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
            libc::signal(signal, on_terminate as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
    }
    // The upgrade from 0.1.x (`ede`): its directories move before anything reads them.
    for dir in migrate_legacy_dirs(&xdg_homes()) {
        tracing::info!("moved the ede directory to {}", dir.display());
    }
    for (k, v) in SESSION_ENV {
        std::env::set_var(k, v);
    }
    // The theme bridge's variables (#EDE-19): the compositor's cursor theme
    // and size are read once at start, before any unit runs.
    for (k, v) in ede_theme::environment(&ede_theme::Paths::from_env()) {
        std::env::set_var(k, v);
    }
    let me = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "exponential-session".into());
    // exponential-compositor next to us (the .deb puts both in /usr/bin), else PATH.
    let compositor = std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("exponential-compositor")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "exponential-compositor".into());

    let crashes_path = state_file();
    let carried = crashes_path.as_deref().map(load_crashes).unwrap_or(0);
    let mut policy = Policy { quick_crashes_in_a_row: carried, ..Default::default() };
    if carried >= ede_session::CRASHES_FOR_SAFE_MODE {
        tracing::warn!("{carried} quick crashes at the last logins: starting in safe mode");
        safe_mode = true;
    }
    policy.safe_mode = safe_mode;
    let failure = failure_file();
    if let Some(path) = &failure {
        let _ = std::fs::remove_file(path);
    }

    loop {
        let mut cmd = Command::new(&compositor);
        cmd.args(compositor_args(policy.safe_mode, &me));
        if policy.safe_mode {
            cmd.env("EDE_SAFE_MODE", "1");
        } else {
            cmd.env_remove("EDE_SAFE_MODE");
        }
        let started = Instant::now();
        let status = cmd.spawn().and_then(wait_or_terminate);
        let ran = started.elapsed();
        if TERMINATED.load(Ordering::SeqCst) {
            tracing::info!("the session was ended; stopping");
            stop_session_units();
            return ExitCode::SUCCESS;
        }
        let (exit, detail) = match status {
            Ok(s) if s.success() => (Exit::Clean, String::new()),
            Ok(s) if s.code() == Some(FATAL_EXIT) => {
                let reason = failure.as_deref().and_then(take_failure).unwrap_or_else(|| "exponential-compositor could not drive the display".into());
                tracing::error!("exponential-compositor stopped: {reason}");
                (Exit::Fatal { ran }, reason)
            }
            Ok(s) => {
                tracing::error!("exponential-compositor ended: {s}");
                (Exit::Crashed { ran }, format!("exponential-compositor ended: {s}"))
            }
            Err(e) => {
                tracing::error!("{compositor}: {e}");
                (Exit::Crashed { ran }, format!("{compositor}: {e}"))
            }
        };
        stop_session_units();
        let next = policy.after(exit);
        if let Some(path) = &crashes_path {
            store_crashes(path, policy.quick_crashes_in_a_row);
        }
        if exit != Exit::Clean {
            // What happened, on the displays (#EDE-40): no GPU needed, so it
            // shows even when rendering is what failed.
            let shown = Command::new(&compositor).args(recovery_args(exit, next, &detail)).spawn().and_then(wait_or_terminate);
            if let Err(e) = shown {
                tracing::warn!("the recovery message could not show: {e}");
            }
            if TERMINATED.load(Ordering::SeqCst) {
                return ExitCode::FAILURE;
            }
        }
        match next {
            Next::Logout => {
                return if exit == Exit::Clean { ExitCode::SUCCESS } else { ExitCode::FAILURE };
            }
            Next::Restart { safe_mode } => {
                tracing::warn!("restarting exponential-compositor{}", if safe_mode { " in safe mode" } else { "" });
            }
        }
    }
}

/// Waits for `child`; when the session is being ended meanwhile, the child
/// gets SIGTERM (then SIGKILL after five seconds) and its status is returned.
fn wait_or_terminate(mut child: Child) -> std::io::Result<ExitStatus> {
    let mut asked: Option<Instant> = None;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if TERMINATED.load(Ordering::SeqCst) {
            match asked {
                None => {
                    // SAFETY: signalling our own child by pid.
                    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
                    asked = Some(Instant::now());
                }
                Some(at) if at.elapsed() > Duration::from_secs(5) => {
                    let _ = child.kill();
                    return child.wait();
                }
                Some(_) => {}
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The compositor is gone: its apps and services go with it. Both targets
/// stop: the units are `PartOf` them, and `exponential-session --ready` starts them
/// again on a restart.
fn stop_session_units() {
    let _ = Command::new("systemctl").args(["--user", "stop", "exponential-session.target", "graphical-session.target"]).status();
    let _ = Command::new("systemctl").args(["--user", "unset-environment"]).args(["WAYLAND_DISPLAY", "DISPLAY", "EDE_SAFE_MODE"]).status();
}
