//! Windows single-instance guard + `exponential://` deep-link delivery.
//!
//! Same job as `single_instance.rs`/`desktop_integration.rs` on Linux: gpui
//! only invokes `on_open_urls` on macOS, so the browser's
//! `exponential://oauth-return#token=…` OAuth callback (§5.7) must be bridged
//! by us. Windows has no Unix datagram sockets, so the bridge is a loopback
//! TCP listener on an ephemeral port, advertised via
//! `<data_dir>/exp-desktop.port`:
//!
//! - The FIRST process binds `127.0.0.1:0`, writes the port file, and spawns
//!   an accept thread that feeds every received `exponential://` line into the
//!   same channel `on_open_urls` uses → `ui::handle_open_urls` adopts the
//!   token in the EXISTING window.
//! - Every LATER launch (the browser runs the registered protocol command
//!   `exp-desktop.exe "%1"` when it opens `exponential://…`) connects to that
//!   port, sends its URL args, and exits.
//! - A failed connect means the port file is stale (crashed instance) — we
//!   delete it and become primary.
//!
//! Scheme registration is per-user (HKCU\Software\Classes\exponential — no
//! elevation needed; scheme literal centralized in
//! `api::login::OAUTH_CALLBACK_SCHEME`, EXP-41), re-asserted each launch via
//! `reg.exe` so a moved binary heals itself, mirroring the Linux
//! `.desktop`/`mimeapps.list` self-registration. The pre-rename
//! `HKCU\Software\Classes\exp` key is deleted in the same pass.

use std::io::{BufRead, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use flume::Sender;

/// Suppress the console-window flash when spawning `reg.exe` from a GUI app.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn port_file() -> PathBuf {
    api::default_data_dir().join("exp-desktop.port")
}

/// `exponential://` URLs handed to us on the command line — the protocol
/// handler launch (`"%1"` in the registry command expands to the callback
/// URL).
fn deep_link_args() -> Vec<String> {
    let prefix = format!("{}://", api::login::OAUTH_CALLBACK_SCHEME);
    std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with(&prefix))
        .collect()
}

/// Result of the single-instance guard (same contract as the Linux version).
pub enum Instance {
    /// We own the listener; deep links now flow into `url_tx`. Continue booting.
    Primary,
    /// Another instance is already live; we forwarded our deep-link args (if
    /// any) to it. The caller MUST return from `main` immediately.
    Forwarded,
}

/// Become the primary instance, or forward our `exponential://` args to the
/// one that already is. Best-effort: any socket error degrades to a plain
/// (non-single) launch rather than blocking startup.
pub fn acquire(url_tx: Sender<Vec<String>>) -> Instance {
    let port_path = port_file();
    let args = deep_link_args();

    // Is an instance already listening on the advertised port?
    if let Ok(text) = std::fs::read_to_string(&port_path) {
        if let Ok(port) = text.trim().parse::<u16>() {
            let addr = (std::net::Ipv4Addr::LOCALHOST, port);
            if let Ok(mut stream) =
                TcpStream::connect_timeout(&addr.into(), Duration::from_millis(500))
            {
                for url in &args {
                    let _ = writeln!(stream, "{url}");
                }
                let _ = stream.flush();
                return Instance::Forwarded;
            }
            // Nobody listening — the file outlived a crashed instance.
            let _ = std::fs::remove_file(&port_path);
        }
    }

    // Become primary: bind an ephemeral loopback port and advertise it.
    match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => {
            let advertised = listener
                .local_addr()
                .ok()
                .map(|addr| addr.port())
                .and_then(|port| {
                    if let Some(parent) = port_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(&port_path, port.to_string()).ok()
                })
                .is_some();
            if advertised {
                let tx = url_tx.clone();
                thread::spawn(move || {
                    let prefix = format!("{}://", api::login::OAUTH_CALLBACK_SCHEME);
                    for stream in listener.incoming().flatten() {
                        let tx = tx.clone();
                        let prefix = prefix.clone();
                        thread::spawn(move || {
                            let reader = BufReader::new(stream);
                            for line in reader.lines().map_while(Result::ok) {
                                let url = line.trim().to_string();
                                // Only ever forward our own scheme — the port is
                                // world-connectable on loopback.
                                if url.starts_with(&prefix) {
                                    let _ = tx.send(vec![url]);
                                }
                            }
                        });
                    }
                });
            }
        }
        Err(err) => {
            eprintln!("[exp-desktop] single-instance listener bind failed: {err:#}");
        }
    }

    // Our OWN cold-start deep link: the app wasn't running when the browser
    // launched us with the callback URL. Feed it in once we boot.
    if !args.is_empty() {
        let _ = url_tx.send(args);
    }

    Instance::Primary
}

/// Register (or re-assert) this binary as the per-user `exponential://`
/// protocol handler. HKCU\Software\Classes needs no elevation; last launch
/// wins, which heals moved/updated installs — the same posture as the Linux
/// `.desktop` self-registration and the macOS `LSSetDefaultHandlerForURLScheme`
/// call. Also deletes the stale pre-rename `HKCU\Software\Classes\exp` key
/// left behind by ≤0.5.x installs (EXP-41 cleanup, best-effort).
pub fn ensure_scheme_registered() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.display().to_string();
    let command = format!("\"{exe}\" \"%1\"");
    let scheme = api::login::OAUTH_CALLBACK_SCHEME;
    let class_key = format!(r"HKCU\Software\Classes\{scheme}");
    let command_key = format!(r"HKCU\Software\Classes\{scheme}\shell\open\command");
    let entries: [(&str, Option<&str>, &str); 3] = [
        (&class_key, None, "URL:Exponential"),
        (&class_key, Some("URL Protocol"), ""),
        (&command_key, None, &command),
    ];
    for (key, value_name, data) in entries {
        let mut cmd = std::process::Command::new("reg.exe");
        cmd.arg("add").arg(key);
        match value_name {
            Some(name) => {
                cmd.args(["/v", name]);
            }
            None => {
                cmd.arg("/ve");
            }
        }
        cmd.args(["/t", "REG_SZ", "/d", data, "/f"]);
        {
            use std::os::windows::process::CommandExt as _;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Err(err) = cmd.output() {
            eprintln!("[exp-desktop] {scheme}:// scheme registration failed ({key}): {err:#}");
            return;
        }
    }

    // Drop the old `exp://` class so stale links stop launching us with URLs
    // the scheme filter above would silently discard. `reg.exe delete` errors
    // (key absent — the common case) are ignored.
    let mut cleanup = std::process::Command::new("reg.exe");
    cleanup.args(["delete", r"HKCU\Software\Classes\exp", "/f"]);
    {
        use std::os::windows::process::CommandExt as _;
        cleanup.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cleanup.output();
}
