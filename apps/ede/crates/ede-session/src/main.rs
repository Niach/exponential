//! `ede-session [--safe-mode]`: the session LightDM starts.
//! `ede-session --ready`: run by the compositor once it is up.

use std::{
    process::{Command, ExitCode},
    time::Instant,
};

use ede_session::{compositor_args, load_crashes, state_file, store_crashes, Exit, Next, Policy, EXPORTED_ENV, SESSION_ENV};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--ready") => ready(),
        None | Some("--safe-mode") => run(!args.is_empty()),
        Some(other) => {
            eprintln!("unknown argument {other:?}; usage: ede-session [--safe-mode]");
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
    match Command::new("systemctl").args(["--user", "start", "--no-block", "ede-session.target"]).status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        other => {
            tracing::error!("ede-session.target did not start: {other:?}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut safe_mode: bool) -> ExitCode {
    for (k, v) in SESSION_ENV {
        std::env::set_var(k, v);
    }
    let me = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "ede-session".into());
    // ede-compositor next to us (the .deb puts both in /usr/bin), else PATH.
    let compositor = std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("ede-compositor")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ede-compositor".into());

    let crashes_path = state_file();
    let carried = crashes_path.as_deref().map(load_crashes).unwrap_or(0);
    let mut policy = Policy { quick_crashes_in_a_row: carried, ..Default::default() };
    if carried >= ede_session::CRASHES_FOR_SAFE_MODE {
        tracing::warn!("{carried} quick crashes at the last logins: starting in safe mode");
        safe_mode = true;
    }
    policy.safe_mode = safe_mode;

    loop {
        let mut cmd = Command::new(&compositor);
        cmd.args(compositor_args(policy.safe_mode, &me));
        if policy.safe_mode {
            cmd.env("EDE_SAFE_MODE", "1");
        } else {
            cmd.env_remove("EDE_SAFE_MODE");
        }
        let started = Instant::now();
        let status = cmd.status();
        let exit = match status {
            Ok(s) if s.success() => Exit::Clean,
            Ok(s) => {
                tracing::error!("ede-compositor ended: {s}");
                Exit::Crashed { ran: started.elapsed() }
            }
            Err(e) => {
                tracing::error!("{compositor}: {e}");
                Exit::Crashed { ran: started.elapsed() }
            }
        };
        stop_session_units();
        let next = policy.after(exit);
        if let Some(path) = &crashes_path {
            store_crashes(path, policy.quick_crashes_in_a_row);
        }
        match next {
            Next::Logout => {
                return if exit == Exit::Clean { ExitCode::SUCCESS } else { ExitCode::FAILURE };
            }
            Next::Restart { safe_mode } => {
                tracing::warn!("restarting ede-compositor{}", if safe_mode { " in safe mode" } else { "" });
            }
        }
    }
}

/// The compositor is gone: its apps and services go with it.
fn stop_session_units() {
    let _ = Command::new("systemctl").args(["--user", "stop", "ede-session.target"]).status();
    let _ = Command::new("systemctl").args(["--user", "unset-environment"]).args(["WAYLAND_DISPLAY", "DISPLAY", "EDE_SAFE_MODE"]).status();
}
