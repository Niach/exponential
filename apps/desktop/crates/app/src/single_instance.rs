//! Linux/BSD single-instance guard + `exp://` deep-link delivery.
//!
//! gpui's Linux backend stores the `on_open_urls` callback but NEVER invokes
//! it (only the macOS backend does — verified against the pinned gpui rev), so
//! the browser's `exp://oauth-return#token=…` OAuth callback (§5.7) has no way
//! to reach a running window on Linux. We bridge it ourselves with the exact
//! mechanism Zed uses (`crates/zed/src/zed/open_listener.rs`
//! `listen_for_cli_connections` + `crates/cli` `launch`): a `UnixDatagram`
//! socket at `<data_dir>/exp-desktop.sock`.
//!
//! - The FIRST process `bind`s the socket and spawns a reader thread that
//!   feeds every received URL into the same channel `on_open_urls` uses →
//!   `ui::handle_open_urls` adopts the token in the EXISTING window.
//! - Every LATER launch (the browser runs the `.desktop` `Exec=exp-desktop %U`
//!   when it opens `exp://…`) `connect`s to that socket, `send`s the URL, and
//!   exits — so the callback signs the running window in instead of spawning a
//!   second app.
//! - A `ConnectionRefused` on `connect` means the socket file outlived a
//!   crashed instance; we remove it and become primary (Zed's stale-socket
//!   handling).
//!
//! macOS keeps the native `on_open_urls` path and never calls into here.

use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::thread;

use flume::Sender;

/// Datagram socket shared by all launches of the app (per data dir, so a
/// `--user-data-dir`-style override would isolate instances just like Zed).
fn socket_path() -> PathBuf {
    api::default_data_dir().join("exp-desktop.sock")
}

/// `exp://` URLs handed to us on the command line — the browser deep-link
/// launch (`%U` in the `.desktop` file expands to the callback URL).
fn deep_link_args() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with("exp://"))
        .collect()
}

/// Result of the single-instance guard.
pub enum Instance {
    /// We own the socket; deep links now flow into `url_tx`. Continue booting.
    Primary,
    /// Another instance is already live; we forwarded our deep-link args (if
    /// any) to it. The caller MUST return from `main` immediately.
    Forwarded,
}

/// Become the primary instance, or forward our `exp://` args to the one that
/// already is. Best-effort: any socket error degrades to a plain (non-single)
/// launch rather than blocking startup.
pub fn acquire(url_tx: Sender<Vec<String>>) -> Instance {
    let sock_path = socket_path();
    let args = deep_link_args();

    // Is an instance already listening on the socket?
    if let Ok(sock) = UnixDatagram::unbound() {
        match sock.connect(&sock_path) {
            Ok(()) => {
                // Live instance — hand off our URLs and bow out.
                for url in &args {
                    let _ = sock.send(url.as_bytes());
                }
                return Instance::Forwarded;
            }
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                // Socket file outlived a crashed instance — clear it so our
                // bind below succeeds.
                let _ = std::fs::remove_file(&sock_path);
            }
            // NotFound / anything else → no instance; we are first.
            Err(_) => {}
        }
    }

    // Become primary: bind and listen.
    if let Some(parent) = sock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match UnixDatagram::bind(&sock_path) {
        Ok(listener) => {
            let tx = url_tx.clone();
            thread::spawn(move || {
                // exp:// callback URLs are short (a session token in the
                // fragment); 4 KiB is comfortably above any real callback.
                let mut buf = [0u8; 4096];
                while let Ok(len) = listener.recv(&mut buf) {
                    let url = String::from_utf8_lossy(&buf[..len]).into_owned();
                    if tx.send(vec![url]).is_err() {
                        break; // app is shutting down.
                    }
                }
            });
        }
        Err(err) => {
            // Can't bind (e.g. read-only data dir) — carry on as a normal
            // launch; deep links just won't be delivered cross-process.
            eprintln!("[exp-desktop] single-instance socket bind failed: {err:#}");
        }
    }

    // Our OWN cold-start deep link: the app wasn't running when the user
    // clicked "Sign in with Google", so the browser launched us WITH the
    // callback URL. Feed it into the same channel once we boot.
    if !args.is_empty() {
        let _ = url_tx.send(args);
    }

    Instance::Primary
}
