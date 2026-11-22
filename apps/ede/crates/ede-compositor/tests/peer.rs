//! #EDE-172 (P2): peer identity. A process is one of eDE's binaries only
//! when it runs that root-owned file in the compositor's user namespace: a
//! copy, a rename, or the genuine file run from a child user namespace is
//! not; a connection's peer is the process that connected; a copied
//! `exponential-sealed` draws an ordinary surface, visible in captures.

mod common;

use std::{
    io::{BufRead, BufReader},
    os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{
    backend::headless::Harness,
    peer::{Installed, Peer},
    Config,
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

const ROLE: &str = "EDE172_PEER_ROLE";
const RED: u32 = 0xffff_0000;

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A process of `program`, read as a peer once it runs it (not the
/// wrapper that started it).
fn running(command: &mut Command, program: &str) -> (Killed, Option<Peer>) {
    let child = Killed(command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let pid = child.0.id();
    let mut peer = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let p = Peer::of_pid(pid);
        if p.as_ref().and_then(Peer::exe_path).is_some_and(|e| e.file_name().is_some_and(|n| n == program)) {
            peer = p;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    (child, peer)
}

/// The genuine root-owned `sleep` is it; a copy is not; the genuine file
/// run in a child user namespace is not.
#[test]
fn a_real_binary_a_copy_and_a_user_namespace_child() {
    let sleep = std::fs::canonicalize("/usr/bin/sleep").unwrap();
    let installed = Installed::of(&[("sleep".into(), sleep.clone())]);
    if installed.paths().is_empty() {
        eprintln!("skipped: {} is not root-owned here", sleep.display());
        return;
    }
    let (_real, peer) = running(Command::new(&sleep).arg("30"), "sleep");
    let peer = peer.expect("the genuine sleep");
    assert!(peer.same_userns && installed.is(&peer, "sleep"), "{peer:?}");
    assert!(!installed.is(&peer, "exponential-sealed"), "one name, one file");
    assert!(peer.pidfd.is_some(), "a pidfd for the very process");

    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("sleep");
    std::fs::copy(&sleep, &copy).unwrap();
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_copy, peer) = running(Command::new(&copy).arg("30"), "sleep");
    let peer = peer.expect("the copy");
    assert!(!installed.is(&peer, "sleep"), "a copy is another file: {peer:?}");
    assert!(Installed::of(&[("sleep".into(), copy.clone())]).paths().is_empty(), "nor is a file the person owns ever installed");

    match Command::new("unshare").args(["--user", "--map-root-user", "--"]).arg(&sleep).arg("30").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(child) => {
            let child = Killed(child);
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut peer = None;
            while Instant::now() < deadline && peer.is_none() {
                peer = Peer::of_pid(child.0.id()).filter(|p| p.exe_path().as_deref() == Some(sleep.as_path()));
                std::thread::sleep(Duration::from_millis(20));
            }
            match peer {
                Some(peer) => {
                    assert!(!peer.same_userns, "{peer:?}");
                    assert!(!installed.is(&peer, "sleep"), "the genuine file in a child user namespace is not it");
                }
                None => eprintln!("skipped the user namespace child: unshare --user did not run here"),
            }
        }
        Err(e) => eprintln!("skipped the user namespace child: {e}"),
    }
}

/// A connection's peer is the process that connected (this test binary
/// started again), read with a pidfd; once that process is gone the peer
/// says so.
#[test]
fn a_connections_peer_is_the_process_that_connected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("peer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "role_connect", "--ignored", "--nocapture", "--test-threads=1"]).env(ROLE, "connect").env("EDE172_SOCKET", &path);
    let mut child = Killed(command.stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap());
    let (stream, _) = listener.accept().unwrap();
    let peer = Peer::of_socket(std::os::fd::AsFd::as_fd(&stream)).expect("a peer");
    assert_eq!(peer.pid, child.0.id());
    assert_eq!(peer.exe_path(), std::env::current_exe().ok(), "{peer:?}");
    assert_eq!(peer.parent.as_ref().map(|p| p.0), Some(std::process::id()), "its parent, this test");
    assert!(peer.same_userns && peer.alive());
    drop(child.0.stdin.take());
    let _ = child.0.wait();
    assert!(!peer.alive(), "the peer knows its process ended");
}

/// The connecting role: connect, then wait for stdin to close.
#[test]
#[ignore = "#EDE-172: a role of a_connections_peer_is_the_process_that_connected"]
fn role_connect() {
    if std::env::var(ROLE).as_deref() != Ok("connect") {
        return;
    }
    let _stream = UnixStream::connect(std::env::var_os("EDE172_SOCKET").unwrap()).unwrap();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

/// The layer a copy of `exponential-sealed` maps (this test binary under
/// that name, in a directory the person owns) is an ordinary one: drawn in
/// a capture, not black. The same surface from the sealed stand-in (a
/// test's own word, by pid) is black.
#[test]
fn a_copied_exponential_sealed_gets_no_protected_surface() {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("exponential-sealed");
    std::fs::copy(std::env::current_exe().unwrap(), &copy).unwrap();
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = dir.path().join("wayland-test");
    let listener = UnixListener::bind(&path).unwrap();

    let draw = |stand_in: bool| {
        let mut command = Command::new(&copy);
        command.args(["--exact", "role_draw_a_layer", "--ignored", "--nocapture", "--test-threads=1"]).env(ROLE, "layer").env("EDE172_SOCKET", &path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = Killed(command.spawn().unwrap());
        let pid = child.0.id();
        if stand_in {
            h.with(move |ede| ede.sealed_stand_in = Some(pid));
        }
        let (stream, _) = listener.accept().unwrap();
        h.with(move |ede| ede.insert_person_client(stream));
        let mut said = BufReader::new(child.0.stdout.take().unwrap()).lines();
        assert!(said.any(|l| l.is_ok_and(|l| l.ends_with("drawn"))), "the copy never drew");
        // What a capture over ede-protocol shows (redacted, as a screenshot
        // or a share would see it), once the layer drew.
        until("the layer never drew", || h.capture(0).at(400, 10) == RED);
        let red = h.ipc().capture(ede_protocol::CaptureTarget::Output { name: "HEADLESS-1".into() }).unwrap().at(400, 10) == RED;
        let sealed = h.with(|ede| ede.desk.space.outputs().any(|o| smithay::desktop::layer_map_for_output(o).layers().any(|l| ede_compositor::sealed::layer_origin(l).sealed)));
        h.with(|ede| ede.sealed_stand_in = None);
        drop(child);
        until("the layer outlived its process", || !h.with(|ede| ede.desk.space.outputs().any(|o| smithay::desktop::layer_map_for_output(o).layers().next().is_some())));
        (red, sealed)
    };
    assert_eq!(draw(false), (true, false), "a copy named exponential-sealed is no sealed surface");
    assert_eq!(draw(true), (false, true), "the stand-in's is: black in the capture");
}

/// The copy's role: a red layer across the top of the output, then wait.
#[test]
#[ignore = "#EDE-172: a role of a_copied_exponential_sealed_gets_no_protected_surface"]
fn role_draw_a_layer() {
    if std::env::var(ROLE).as_deref() != Ok("layer") {
        return;
    }
    let path = PathBuf::from(std::env::var_os("EDE172_SOCKET").unwrap());
    let mut app = Session::connect(UnixStream::connect(Path::new(&path)).unwrap());
    let layer = app.layer("ede172-copy", Anchor::Top | Anchor::Left | Anchor::Right, (0, 40), 0);
    app.draw_layer(&layer, RED);
    app.roundtrip();
    println!("drawn");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}
