//! Linux/BSD single-instance guard + `exponential://` deep-link delivery.
//!
//! gpui's Linux backend stores the `on_open_urls` callback but NEVER invokes
//! it (only the macOS backend does — verified against the pinned gpui rev), so
//! the browser's `exponential://oauth-return#token=…` OAuth callback (§5.7)
//! has no way to reach a running window on Linux. We bridge it ourselves with
//! the exact mechanism Zed uses (`crates/zed/src/zed/open_listener.rs`
//! `listen_for_cli_connections` + `crates/cli` `launch`): a `UnixDatagram`
//! socket at `<data_dir>/exp-desktop.sock`.
//!
//! - The FIRST process `bind`s the socket and spawns a reader thread that
//!   feeds every received URL into the same channel `on_open_urls` uses →
//!   `ui::handle_open_urls` adopts the token in the EXISTING window.
//! - Every LATER launch (the browser runs the `.desktop` `Exec=exp-desktop %U`
//!   when it opens `exponential://…`) `connect`s to that socket, `send`s the
//!   URL, and exits — so the callback signs the running window in instead of
//!   spawning a second app.
//! - Who is primary is an exclusive lock on `exp-desktop.lock` held for the
//!   process's life (#EDE-64): the holder removes a dead instance's socket
//!   file and binds; every other launch forwards.
//!
//! macOS keeps the native `on_open_urls` path and never calls into here.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use flume::Sender;

/// Datagram socket shared by all launches of the app (per data dir, so a
/// `--user-data-dir`-style override would isolate instances just like Zed).
fn socket_path() -> PathBuf {
    api::default_data_dir().join("exp-desktop.sock")
}

/// `exponential://` URLs handed to us on the command line — the browser
/// deep-link launch (`%U` in the `.desktop` file expands to the callback URL).
fn deep_link_args() -> Vec<String> {
    let prefix = format!("{}://", api::login::OAUTH_CALLBACK_SCHEME);
    std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with(&prefix))
        .collect()
}

/// NEW-68: what a plain second launch (no deep link) sends: bring a window
/// up. Without it a relaunch of an IDE kept running by the voice tray with
/// every window closed would exit and show nothing.
pub(crate) const OPEN_WINDOW: &str = "exponential-desktop:open-window";

/// Result of the single-instance guard.
pub enum Instance {
    /// We own the socket; deep links now flow into `url_tx`. Continue booting.
    Primary,
    /// Another instance is already live; we forwarded our deep-link args (if
    /// any) to it. The caller MUST return from `main` immediately.
    Forwarded,
}

/// Become the primary instance, or forward our `exponential://` args to the one that
/// already is. Best-effort: any socket error degrades to a plain (non-single)
/// launch rather than blocking startup.
pub fn acquire(url_tx: Sender<Vec<String>>, component: bool) -> Instance {
    let (instance, lock) = acquire_at(&socket_path(), &deep_link_args(), url_tx, component);
    // The primary holds the lock for the life of the process.
    HOLDS_LOCK.store(lock.is_some(), std::sync::atomic::Ordering::SeqCst);
    std::mem::forget(lock);
    instance
}

/// This process holds `exp-desktop.lock`: it alone binds the sockets, the
/// session bus included (#EDE-227).
static HOLDS_LOCK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn holds_lock() -> bool {
    HOLDS_LOCK.load(std::sync::atomic::Ordering::SeqCst)
}

/// #EDE-227: the session bus beside the link socket `sock_path`.
fn bus_path(sock_path: &Path) -> PathBuf {
    sock_path.with_file_name(api::bus::SOCKET)
}

/// #EDE-227: the launch token this process was started with (a press in
/// eDE's menu, a browser's xdg-activation): it raises the running
/// instance's window when the launch is forwarded. Read before gpui takes
/// the variable.
fn activation_token() -> Option<String> {
    ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"].iter().find_map(|name| std::env::var(name).ok().filter(|token| !token.is_empty()))
}

/// #EDE-227: eDE's component found the lock held. A standalone app that
/// speaks the bus is asked to hand the login over (it quits); an older one
/// is waited for (the person quits it, or it was the device of the last
/// login). Another component (a second `exponential-app` in the same
/// session) takes our launch like any second launch does: `false`.
fn ask_for_handover(sock_path: &Path) -> bool {
    use api::bus::{Reply, Request, Role};
    let Ok(mut bus) = api::bus::Client::connect(&bus_path(sock_path), api::bus::CALL_TIMEOUT) else {
        eprintln!("[exponential-app] an Exponential app without a session bus holds the data directory; waiting for it to quit");
        return true;
    };
    match bus.call(&Request::Hello) {
        Ok(Reply::Hello { role: Role::Component, pid, .. }) => {
            eprintln!("[exponential-app] eDE's component runs already (pid {pid})");
            false
        }
        Ok(Reply::Hello { pid, .. }) => {
            let reply = bus.call(&Request::Handover);
            eprintln!("[exponential-app] the standalone app (pid {pid}) hands the login over: {reply:?}");
            true
        }
        other => {
            eprintln!("[exponential-app] the running app answers {other:?}; waiting for it to quit");
            true
        }
    }
}

/// #EDE-64: who is primary is decided by an exclusive lock next to the
/// socket, never by the socket file alone. Five clicks on eDE's sidebar in
/// two seconds started five cold launches that each found no live socket (or
/// a stale one), unlinked each other's freshly bound socket and ALL booted as
/// primaries: three IDE windows. The lock holder is the one primary; a launch
/// that can't take it forwards (retrying while the holder binds) and exits.
fn acquire_at(sock_path: &Path, args: &[String], url_tx: Sender<Vec<String>>, component: bool) -> (Instance, Option<File>) {
    if let Some(parent) = sock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(sock_path.with_extension("lock"));
    match lock.as_ref().map(|file| file.try_lock()) {
        Ok(Ok(())) => {}
        // #EDE-227: eDE's component takes the login from a standalone app.
        Ok(Err(TryLockError::WouldBlock)) if component && ask_for_handover(sock_path) => {
            if let Ok(file) = lock.as_ref() {
                if let Err(err) = file.lock() {
                    eprintln!("[exponential-app] exp-desktop.lock: {err}");
                }
            }
        }
        Ok(Err(TryLockError::WouldBlock)) => {
            // A live primary: it binds right after taking the lock.
            let deadline = Instant::now() + FORWARD_PATIENCE;
            loop {
                if forward(sock_path, args) {
                    return (Instance::Forwarded, None);
                }
                if Instant::now() >= deadline {
                    // Its socket never came up (unbindable data dir): a plain
                    // launch, as before.
                    eprintln!("[exp-desktop] the running instance takes no launches; starting another");
                    return (Instance::Primary, None);
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        // No locking here (an odd filesystem): the socket alone decides.
        _ => {
            if forward(sock_path, args) {
                return (Instance::Forwarded, None);
            }
        }
    }

    // Primary: whatever socket file is left belongs to a dead instance.
    let _ = std::fs::remove_file(sock_path);
    match UnixDatagram::bind(sock_path) {
        Ok(listener) => {
            let tx = url_tx.clone();
            thread::spawn(move || {
                // exponential:// callback URLs are short (a session token in the
                // fragment); 4 KiB is comfortably above any real callback.
                let mut buf = [0u8; 4096];
                while let Ok(len) = listener.recv(&mut buf) {
                    let url = String::from_utf8_lossy(&buf[..len]).into_owned();
                    // An empty batch = "open a window" (see `OPEN_WINDOW`).
                    let batch = if url == OPEN_WINDOW { Vec::new() } else { vec![url] };
                    if tx.send(batch).is_err() {
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
        let _ = url_tx.send(args.to_vec());
    }

    (Instance::Primary, lock.ok())
}

/// How long a second launch waits for the primary's socket.
const FORWARD_PATIENCE: Duration = Duration::from_secs(5);

/// Hand our URLs (or, with none, a request for a window) to the live
/// instance; `false` = nobody listens on the socket.
fn forward(sock_path: &Path, args: &[String]) -> bool {
    if forward_on_bus(sock_path, args) {
        return true;
    }
    let Ok(sock) = UnixDatagram::unbound() else { return false };
    if sock.connect(sock_path).is_err() {
        return false;
    }
    if args.is_empty() {
        let _ = sock.send(OPEN_WINDOW.as_bytes());
    }
    for url in args {
        let _ = sock.send(url.as_bytes());
    }
    true
}

/// #EDE-227: the same over the session bus, with this launch's token so
/// the running instance's window comes to the front; `false` = no bus (an
/// older app) or it took nothing.
fn forward_on_bus(sock_path: &Path, args: &[String]) -> bool {
    use api::bus::{Reply, Request};
    let Ok(mut bus) = api::bus::Client::connect(&bus_path(sock_path), api::bus::CALL_TIMEOUT) else { return false };
    let token = activation_token();
    let links: Vec<Option<String>> = if args.is_empty() { vec![None] } else { args.iter().cloned().map(Some).collect() };
    let mut took = false;
    for (i, link) in links.into_iter().enumerate() {
        // The token raises one window: the first link's.
        let request = Request::Open { link, activation_token: if i == 0 { token.clone() } else { None } };
        match bus.call(&request) {
            Ok(Reply::Done) => took = true,
            other => {
                eprintln!("[exp-desktop] the session bus answered {other:?}");
                return took;
            }
        }
    }
    took
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_socket(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-single-instance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("exp-desktop.sock")
    }

    #[test]
    fn a_second_launch_asks_the_first_for_a_window_and_exits() {
        let sock = temp_socket("second");
        let (tx, rx) = flume::unbounded();
        let (first, _lock) = acquire_at(&sock, &[], tx.clone(), false);
        assert!(matches!(first, Instance::Primary));
        let (second, _) = acquire_at(&sock, &[], tx.clone(), false);
        assert!(matches!(second, Instance::Forwarded));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Vec::<String>::new(), "an empty batch = open (or raise) a window");
        let url = "exponential://issue/EXP-1".to_string();
        let (third, _) = acquire_at(&sock, std::slice::from_ref(&url), tx, false);
        assert!(matches!(third, Instance::Forwarded));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), vec![url]);
    }

    #[test]
    fn five_simultaneous_cold_launches_make_one_primary() {
        let sock = temp_socket("race");
        // A dead instance's socket file, as after a crash or a quit.
        drop(UnixDatagram::bind(&sock).unwrap());
        let (tx, rx) = flume::unbounded();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(5));
        let launches: Vec<_> = (0..5)
            .map(|_| {
                let (sock, tx, barrier) = (sock.clone(), tx.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    acquire_at(&sock, &[], tx, false)
                })
            })
            .collect();
        let results: Vec<_> = launches.into_iter().map(|l| l.join().unwrap()).collect();
        let primaries = results.iter().filter(|(i, _)| matches!(i, Instance::Primary)).count();
        assert_eq!(primaries, 1, "one IDE, however fast the clicks");
        for _ in 0..4 {
            assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Vec::<String>::new());
        }
    }

    /// #EDE-227: a standalone app holds the data directory when eDE's
    /// component starts: asked over the bus, it hands over (lets the lock
    /// go) and the component becomes the primary.
    #[test]
    fn the_component_takes_the_login_from_a_standalone_app() {
        use api::bus::{Reply, Request, Role};
        let sock = temp_socket("handover");
        let (tx, _rx) = flume::unbounded();
        let (standalone, lock) = acquire_at(&sock, &[], tx.clone(), false);
        assert!(matches!(standalone, Instance::Primary));
        let held = std::sync::Arc::new(std::sync::Mutex::new(lock));
        let asked = held.clone();
        api::bus::serve(&bus_path(&sock), move |_, request| match request {
            Request::Hello => Reply::Hello { version: api::bus::VERSION, pid: std::process::id(), role: Role::Standalone },
            Request::Handover => {
                // It quits: the kernel drops its lock.
                asked.lock().unwrap().take();
                Reply::Done
            }
            _ => Reply::Done,
        })
        .unwrap();
        let (component, lock) = acquire_at(&sock, &[], tx, true);
        assert!(matches!(component, Instance::Primary));
        assert!(lock.is_some(), "the component holds the lock");
        assert!(held.lock().unwrap().is_none(), "the standalone app let it go");
    }

    /// #EDE-227: a second launch hands its link over the bus, with the
    /// launch's token, when the running app serves one.
    #[test]
    fn a_second_launch_forwards_over_the_bus() {
        use api::bus::{Reply, Request};
        let sock = temp_socket("bus");
        let (tx, _rx) = flume::unbounded();
        let (_first, _lock) = acquire_at(&sock, &[], tx.clone(), false);
        let (seen_tx, seen) = flume::unbounded();
        api::bus::serve(&bus_path(&sock), move |_, request| {
            let _ = seen_tx.send(request);
            Reply::Done
        })
        .unwrap();
        let url = "exponential://issue/EXP-2".to_string();
        let (second, _) = acquire_at(&sock, std::slice::from_ref(&url), tx, false);
        assert!(matches!(second, Instance::Forwarded));
        match seen.recv_timeout(Duration::from_secs(2)).unwrap() {
            Request::Open { link, .. } => assert_eq!(link, Some(url)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_dead_primarys_lock_and_socket_are_taken_over() {
        let sock = temp_socket("takeover");
        let (tx, _rx) = flume::unbounded();
        let (first, lock) = acquire_at(&sock, &[], tx.clone(), false);
        assert!(matches!(first, Instance::Primary));
        drop(lock); // the process ended: the kernel drops its lock
        let (next, _lock) = acquire_at(&sock, &[], tx, false);
        assert!(matches!(next, Instance::Primary));
    }
}
