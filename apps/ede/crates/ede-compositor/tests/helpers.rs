//! #EDE-172 (P4), headless: the compositor starts its helpers itself
//! (`[session] helpers = "compositor"`). A helper says hello over the
//! channel at fd 3 with no token and gets a pinned environment (what the
//! compositor's own holds to load code from the person's paths does not
//! reach it); the lock unlocks over its channel and on no other connection,
//! its own process's included; a helper that refuses ptrace after its first
//! roundtrips is refused ptrace, `pidfd_getfd` and its `/proc` environment
//! by another process of the person; a helper started again holds its grant
//! anew and what the dead one held is let go; and the shell's space view is
//! black in captures through the Wayland client the compositor made.
//!
//! The helpers are this test binary started again (`EDE172_HELPER_ROLE`).

mod common;

use std::{
    ffi::OsString,
    io::Read,
    os::{
        fd::{FromRawFd, RawFd},
        unix::net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{
    backend::headless::Harness,
    helpers::{Channel, Kind, Spec, Wayland},
    lock::LockState,
    Config,
};
use ede_protocol::{Client, Error, ErrorCode, InputEvent, SPACE_VIEW_NAMESPACE};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

const ROLE: &str = "EDE172_HELPER_ROLE";
const OUT: &str = "EDE172_HELPER_OUT";
const PROBED: &str = "EDE172_HELPER_PROBED";
const GREEN: u32 = 0xff00_ff00;
const BLACK: u32 = 0xff00_0000;
const KEY_A: u32 = 30;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, &[(1280, 800)])
}

/// This test binary again, running only the role test `name`.
fn role_argv(name: &str) -> Vec<String> {
    [std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(), name.into(), "--ignored".into(), "--nocapture".into(), "--test-threads=1".into()].into()
}

fn spec(name: &str, role: &str, out: &Path, channel: Option<Channel>, wayland: Option<Wayland>) -> Spec {
    Spec { name: name.into(), argv: role_argv(&format!("role_{role}")), channel, wayland, env: vec![(ROLE.into(), role.into()), (OUT.into(), out.to_string_lossy().into_owned())] }
}

/// What the test binary's own environment is, as the compositor's would be.
fn base_env() -> Vec<(OsString, OsString)> {
    std::env::vars_os().filter(|(k, _)| k != "LD_PRELOAD").collect()
}

/// The lines a role wrote to `out` once one starts with `what`.
fn report(out: &Path, what: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let text = std::fs::read_to_string(out).unwrap_or_default();
        if text.lines().any(|l| l.starts_with(what)) {
            return text;
        }
        assert!(Instant::now() < deadline, "the helper never said {what:?}; it said {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A role appends a line, whole.
fn say(line: &str) {
    use std::io::Write;
    let out = std::env::var_os(OUT).expect("where to report");
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(out).unwrap();
    file.write_all(format!("{line}\n").as_bytes()).unwrap();
}

/// A role waits to be ended (its parent-death signal, a kill).
fn stay() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// The Wayland connection a role was handed at `WAYLAND_SOCKET`.
fn inherited_wayland() -> Option<UnixStream> {
    let fd: RawFd = std::env::var("WAYLAND_SOCKET").ok()?.parse().ok()?;
    // Safety: the compositor handed this descriptor to this process for this use.
    Some(unsafe { UnixStream::from_raw_fd(fd) })
}

/// A helper says hello over fd 3 with no token: it holds the root grant
/// (the shell's supervisor's), names none, and finds no token anywhere it
/// was started with; its environment is pinned.
#[test]
fn a_helper_says_hello_over_fd_3_with_no_token_in_a_pinned_environment() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    // The compositor's environment holds what loads code into a process.
    let mut env = base_env();
    for (k, v) in [("LD_PRELOAD", "/nonexistent/libhook.so"), ("LD_AUDIT", "/nonexistent/libaudit.so"), ("LD_LIBRARY_PATH", "/nonexistent"), ("GCONV_PATH", "/nonexistent"), ("VK_ICD_FILENAMES", "/nonexistent/icd.json"), ("VK_ADD_IMPLICIT_LAYER_PATH", "/nonexistent"), ("PIPEWIRE_MODULE_DIR", "/nonexistent")] {
        env.push((k.into(), v.into()));
    }
    let root = h.with(|ede| ede.ipc.root_grant().token.as_str().to_string());
    let helper = spec("test-shell", "report", &out, Some(Channel::Root), Some(Wayland::AtStart(Kind::Shell)));
    h.with(move |ede| {
        ede.helpers.base_env = Some(env);
        ede.start_helpers_with(vec![helper]);
    });
    let said = report(&out, "done");
    let pid = h.with(|ede| ede.helpers.pid("test-shell")).expect("the helper runs");
    assert!(said.contains("hello exponential-shell 6\n"), "the root grant over its channel, its hello naming no token: {said}");
    assert!(said.contains("wayland ok\n"), "its Wayland connection: {said}");
    assert!(said.contains("stdin /dev/null\n"), "nothing on its stdin: {said}");
    assert!(said.contains("control-fd-variable gone\n"), "{said}");
    for dropped in ["LD_PRELOAD", "LD_AUDIT", "LD_LIBRARY_PATH", "GCONV_PATH", "VK_ICD_FILENAMES", "VK_ADD_IMPLICIT_LAYER_PATH", "PIPEWIRE_MODULE_DIR", "EDE_TOKEN"] {
        assert!(!said.contains(&format!("env {dropped}=")), "{dropped} reached the helper: {said}");
    }
    assert!(said.contains("env VK_LOADER_LAYERS_DISABLE=~implicit~\n") && said.contains("env EDE_HELPERS_SPAWNED=1\n"), "{said}");
    // The root token is in neither its environment nor its arguments.
    for file in ["environ", "cmdline"] {
        let bytes = std::fs::read(format!("/proc/{pid}/{file}")).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(&root), "the root token is in the helper's {file}");
    }
}

#[test]
#[ignore = "#EDE-172: a role of the helper tests"]
fn role_report() {
    if std::env::var(ROLE).as_deref() != Ok("report") {
        return;
    }
    say(&format!("stdin {}", std::fs::read_link("/proc/self/fd/0").map(|p| p.display().to_string()).unwrap_or_default()));
    for (k, v) in std::env::vars() {
        say(&format!("env {k}={v}"));
    }
    match Client::inherited() {
        Some(Ok(client)) => say(&format!("hello {} {}", client.hello.session_id.as_str(), client.hello.capabilities.len())),
        Some(Err(e)) => say(&format!("hello failed {e}")),
        None => say("hello none"),
    }
    say(&format!("control-fd-variable {}", if std::env::var_os(ede_protocol::CONTROL_FD_ENV).is_some() { "kept" } else { "gone" }));
    if let Some(stream) = inherited_wayland() {
        let session = Session::connect(stream);
        say(&format!("wayland {}", if session.state.layer_shell.is_some() { "ok" } else { "no layer shell" }));
    }
    say("done");
    stay();
}

/// The sealed lock's state, on the compositor thread.
fn lock_state<R: Send + 'static>(h: &Harness, f: impl FnOnce(&mut ede_compositor::sealed::Sealed) -> R + Send + 'static) -> R {
    h.with(|ede| match &mut ede.lock {
        LockState::Sealed(s) => f(s),
        _ => panic!("not sealed-locked"),
    })
}

/// The lock client gets its grant on a channel at fd 3, nothing on its
/// stdin, and `unlock` counts on that connection only: a second channel of
/// the same process (the same grant) is refused, and the grant's token is
/// no way in for anyone.
#[test]
fn the_lock_unlocks_over_its_channel_and_no_second_connection_of_its_process_does() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let mut env = base_env();
    env.push((ROLE.into(), "lock".into()));
    env.push((OUT.into(), out.clone().into()));
    h.with(move |ede| {
        ede.helpers.on = true;
        ede.helpers.base_env = Some(env);
        ede.begin_sealed_lock_with(role_argv("role_lock"));
    });
    assert!(h.with(|ede| ede.is_locked()));
    let (token, conn) = lock_state(&h, |s| (s.token.clone(), s.conn));
    assert!(conn.is_some(), "the lock client holds a channel");
    match h.ipc_with(token) {
        Err(Error::Failed(f)) => assert_eq!(f.code, ErrorCode::Unauthorized),
        Ok(_) => panic!("the lock's grant was taken by its token"),
        Err(e) => panic!("{e}"),
    }
    let said = report(&out, "first");
    assert!(said.contains("second Forbidden\n"), "a second connection of the lock client unlocked: {said}");
    assert!(said.contains("first unlocked\n"), "the lock client's channel did not unlock: {said}");
    assert!(said.contains("stdin /dev/null\n"), "{said}");
    assert!(!h.with(|ede| ede.is_locked()));
}

/// #EDE-212: with the helpers on, a lock whose client cannot start is a
/// lock all the same; the start that works later gets a channel of its own
/// and unlocks over it.
#[test]
fn a_lock_whose_client_cannot_start_on_its_channel_stays_locked_until_one_does() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let mut env = base_env();
    env.push((ROLE.into(), "lock".into()));
    env.push((OUT.into(), out.clone().into()));
    h.with(move |ede| {
        ede.helpers.on = true;
        ede.helpers.base_env = Some(env);
        ede.begin_sealed_lock_with(vec!["/nonexistent/exponential-sealed".into(), "lock".into()]);
    });
    assert!(h.with(|ede| ede.is_locked()), "a lock client that cannot start left the session unlocked");
    assert!(lock_state(&h, |s| s.child.is_none() && s.conn.is_none()));
    let token = lock_state(&h, |s| s.token.clone());
    assert!(h.ipc_with(token).is_err(), "a lock with no client holds no grant to take");

    lock_state(&h, |s| s.command = role_argv("role_lock"));
    // Started once it can (it may unlock at once: the lock is then gone).
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(|ede| matches!(&ede.lock, LockState::Sealed(s) if s.child.is_none())) {
        assert!(Instant::now() < deadline, "the lock client was never started once it could");
        h.with(|ede| ede.sealed_tick());
        std::thread::sleep(Duration::from_millis(20));
    }
    let said = report(&out, "first");
    assert!(said.contains("first unlocked\n"), "the started lock client did not unlock over its channel: {said}");
    assert!(!h.with(|ede| ede.is_locked()));
}

#[test]
#[ignore = "#EDE-172: a role of the helper tests"]
fn role_lock() {
    if std::env::var(ROLE).as_deref() != Ok("lock") {
        return;
    }
    say(&format!("stdin {}", std::fs::read_link("/proc/self/fd/0").map(|p| p.display().to_string()).unwrap_or_default()));
    let mut channel = Client::inherited().expect("a channel at fd 3").expect("hello over it");
    let held = channel.hello.capabilities.clone();
    let (mut second, _) = channel.open_channel(None, held, None).expect("a second connection on the lock's grant");
    let outcome = |r: Result<(), Error>| match r {
        Ok(()) => "unlocked".to_string(),
        Err(Error::Failed(f)) => format!("{:?}", f.code),
        Err(e) => e.to_string(),
    };
    say(&format!("second {}", outcome(second.unlock())));
    say(&format!("first {}", outcome(channel.unlock())));
}

/// Not root, and no `CAP_SYS_PTRACE` (that reads every process).
fn may_be_refused() -> bool {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let effective = status.lines().find_map(|l| l.strip_prefix("CapEff:")).and_then(|v| u64::from_str_radix(v.trim(), 16).ok()).unwrap_or(0);
    // Safety: geteuid only reads.
    (unsafe { libc::geteuid() }) != 0 && effective & (1 << 19) == 0
}

/// What process `pid` let this one do: attach, take its fd 3 (`fd`), read
/// its environment.
fn probe(pid: i32, fd: i32) -> [Result<(), std::io::Error>; 3] {
    // Safety: ptrace, waitpid and pidfd syscalls on another process, each
    // checked; an attach is undone at once.
    let attach = unsafe {
        if libc::ptrace(libc::PTRACE_ATTACH, pid, 0, 0) == 0 {
            libc::waitpid(pid, std::ptr::null_mut(), 0);
            libc::ptrace(libc::PTRACE_DETACH, pid, 0, 0);
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    };
    let getfd = unsafe {
        let pidfd = libc::syscall(libc::SYS_pidfd_open, pid, 0) as i32;
        if pidfd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            let taken = libc::syscall(libc::SYS_pidfd_getfd, pidfd, fd, 0) as i32;
            let out = if taken >= 0 {
                libc::close(taken);
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            };
            libc::close(pidfd);
            out
        }
    };
    let environ = std::fs::File::open(format!("/proc/{pid}/environ")).and_then(|mut f| f.read_to_end(&mut Vec::new())).map(|_| ());
    [attach, getfd, environ]
}

fn refused(e: &Result<(), std::io::Error>) -> bool {
    matches!(e, Err(e) if matches!(e.raw_os_error(), Some(libc::EPERM) | Some(libc::EACCES)))
}

/// A helper refuses ptrace once its first roundtrips are done: its parent
/// (the compositor, which Yama lets attach to its children) and another
/// process of the person get `EPERM` for ptrace and `pidfd_getfd`, and
/// cannot read its environment. A process of the same tree that did not
/// refuse (the control) is open to its parent, so this machine lets the
/// test tell the two apart.
#[test]
fn another_process_of_the_person_is_refused_ptrace_pidfd_getfd_and_the_environment() {
    if !may_be_refused() {
        eprintln!("skipped: root or CAP_SYS_PTRACE reads every process");
        return;
    }
    let mut control = Command::new("sleep").arg("30").stdin(Stdio::null()).spawn().unwrap();
    let opened = probe(control.id() as i32, 0);
    let _ = control.kill();
    let _ = control.wait();
    if !opened.iter().all(Result::is_ok) {
        eprintln!("skipped: this machine refuses a parent ptrace or pidfd_getfd of a dumpable child ({opened:?})");
        return;
    }

    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let helper = spec("test-refusing", "refuse", &out, Some(Channel::Root), Some(Wayland::AtStart(Kind::Shell)));
    h.with(move |ede| {
        ede.helpers.base_env = Some(base_env());
        ede.start_helpers_with(vec![helper]);
    });
    report(&out, "ready");
    let pid = h.with(|ede| ede.helpers.pid("test-refusing")).expect("the helper runs") as i32;
    let parent = probe(pid, 3);
    assert!(parent.iter().all(refused), "its parent got through: {parent:?}");

    // Another process of the person, no ancestor of it.
    let mut foreign = Command::new(std::env::current_exe().unwrap());
    foreign.args(["--exact", "role_foreign", "--ignored", "--nocapture", "--test-threads=1"]).env(ROLE, "foreign").env(OUT, dir.path().join("foreign")).env(PROBED, pid.to_string());
    assert!(foreign.status().unwrap().success());
    let said = report(&dir.path().join("foreign"), "probed");
    assert!(said.contains("probed refused refused refused\n"), "another process got through: {said}");
}

#[test]
#[ignore = "#EDE-172: a role of the helper tests"]
fn role_refuse() {
    if std::env::var(ROLE).as_deref() != Ok("refuse") {
        return;
    }
    let _channel = Client::inherited().expect("a channel").expect("hello");
    let _wayland = Session::connect(inherited_wayland().expect("a Wayland connection"));
    ede_protocol::process::refuse_ptrace().unwrap();
    say("ready");
    stay();
}

#[test]
#[ignore = "#EDE-172: a role of the helper tests"]
fn role_foreign() {
    if std::env::var(ROLE).as_deref() != Ok("foreign") {
        return;
    }
    let pid: i32 = std::env::var(PROBED).unwrap().parse().unwrap();
    let got = probe(pid, 3);
    say(&format!("probed {}", got.iter().map(|r| if refused(r) { "refused" } else { "open" }).collect::<Vec<_>>().join(" ")));
}

/// A helper that ends is started again on a channel of its own holding the
/// same grant (root), and what the dead one held (the person's input, a
/// pressed key) is let go.
#[test]
fn a_helper_started_again_holds_root_and_the_dead_ones_holds_are_released() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let helper = spec("test-holder", "hold", &out, Some(Channel::Root), None);
    h.with(move |ede| {
        ede.helpers.base_env = Some(base_env());
        ede.start_helpers_with(vec![helper]);
    });
    report(&out, "held");
    assert!(h.with(|ede| ede.person_input_held() && !ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()), "the helper holds the person's input");
    let first = h.with(|ede| ede.helpers.pid("test-holder")).unwrap();
    // Safety: killing our own child.
    unsafe { libc::kill(first as i32, libc::SIGKILL) };
    let deadline = Instant::now() + Duration::from_secs(20);
    while !std::fs::read_to_string(&out).unwrap_or_default().contains("again") {
        assert!(Instant::now() < deadline, "the helper was not started again");
        h.with(|ede| ede.helpers_tick());
        std::thread::sleep(Duration::from_millis(50));
    }
    let said = report(&out, "again");
    assert!(said.contains("again exponential-shell 6\n"), "the new one holds root: {said}");
    assert_ne!(h.with(|ede| ede.helpers.pid("test-holder")), Some(first));
    assert!(h.with(|ede| !ede.person_input_held() && ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()), "the dead helper's hold or key outlived it");
}

#[test]
#[ignore = "#EDE-172: a role of the helper tests"]
fn role_hold() {
    if std::env::var(ROLE).as_deref() != Ok("hold") {
        return;
    }
    let mut channel = Client::inherited().expect("a channel").expect("hello");
    let out = PathBuf::from(std::env::var_os(OUT).unwrap());
    if std::fs::read_to_string(&out).unwrap_or_default().contains("held") {
        say(&format!("again {} {}", channel.hello.session_id.as_str(), channel.hello.capabilities.len()));
        stay();
    }
    channel.hold_input().unwrap();
    channel.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap();
    say("held");
    stay();
}

/// The shell's supervisor asks its channel for a Wayland connection per
/// shell it starts (`open_wayland`): through that client the compositor
/// made, the shell's space view is black in a capture of the person's
/// output while the screen shows it. Only that channel gets one: a channel
/// opened on it does not, nor does any other root connection.
#[test]
fn the_space_view_is_black_through_the_client_the_compositor_made() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let helper = spec("test-supervisor", "space_view", &out, Some(Channel::Root), Some(Wayland::Asked(Kind::Shell)));
    h.with(move |ede| {
        ede.helpers.base_env = Some(base_env());
        ede.start_helpers_with(vec![helper]);
    });
    let said = report(&out, "drawn");
    assert!(said.contains("sibling Forbidden\n"), "a channel opened on the supervisor's got a Wayland connection: {said}");
    // Bottom right of the 1280×800 output: x 880..1280, y 500..800.
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.capture(0).at(1080, 650) != GREEN {
        assert!(Instant::now() < deadline, "the view never showed on the screen");
        std::thread::sleep(Duration::from_millis(20));
    }
    let shared = h.capture_shared(0);
    assert_eq!((shared.at(881, 501), shared.at(1080, 650), shared.at(1279, 799)), (BLACK, BLACK, BLACK), "the view leaked into a capture");
    let mut root = h.root_channel();
    match root.open_wayland() {
        Err(Error::Failed(f)) => assert_eq!(f.code, ErrorCode::Forbidden),
        Ok(_) => panic!("a root channel that is no helper's got a Wayland connection"),
        Err(e) => panic!("{e}"),
    }
}

#[test]
#[ignore = "#EDE-172: a role of the helper tests"]
fn role_space_view() {
    if std::env::var(ROLE).as_deref() != Ok("space_view") {
        return;
    }
    let mut channel = Client::inherited().expect("a channel").expect("hello");
    let held = channel.hello.capabilities.clone();
    let (mut sibling, _) = channel.open_channel(None, held, None).unwrap();
    say(&format!(
        "sibling {}",
        match sibling.open_wayland() {
            Ok(_) => "got one".to_string(),
            Err(Error::Failed(f)) => format!("{:?}", f.code),
            Err(e) => e.to_string(),
        }
    ));
    let fd = channel.open_wayland().expect("a Wayland connection for the shell");
    let mut shell = Session::connect(UnixStream::from(fd));
    let view = shell.layer(SPACE_VIEW_NAMESPACE, Anchor::Bottom | Anchor::Right, (400, 300), 0);
    shell.draw_layer(&view, GREEN);
    shell.roundtrip();
    say("drawn");
    loop {
        std::thread::sleep(Duration::from_millis(50));
        shell.roundtrip();
    }
}

/// The environment a helper gets, on its own: what loads code from the
/// person's paths goes, the pinned Vulkan settings come.
#[test]
fn the_pinned_environment() {
    let base: Vec<(OsString, OsString)> = [("PATH", "/usr/bin"), ("LD_PRELOAD", "x.so"), ("VK_ICD_FILENAMES", "y.json"), ("VK_LOADER_LAYERS_DISABLE", "nothing"), ("HOME", "/home/p"), ("EDE_TOKEN", "a"), ("WAYLAND_SOCKET", "7")].into_iter().map(|(k, v)| (k.into(), v.into())).collect();
    let env = ede_compositor::helpers::pinned_env(base);
    let get = |name: &str| env.iter().filter(|(k, _)| k == name).map(|(_, v)| v.to_string_lossy().into_owned()).collect::<Vec<_>>();
    assert_eq!(get("PATH"), ["/usr/bin"]);
    assert_eq!(get("HOME"), ["/home/p"]);
    for gone in ["LD_PRELOAD", "VK_ICD_FILENAMES", "EDE_TOKEN", "WAYLAND_SOCKET"] {
        assert!(get(gone).is_empty(), "{gone}");
    }
    assert_eq!(get("VK_LOADER_LAYERS_DISABLE"), ["~implicit~"], "the pinned value, not the person's");
    assert_eq!(get("VK_LOADER_LAYERS_ALLOW"), ["VK_LAYER_MESA_device_select"]);
    // What the person's apps get back from a helper's launch: the person's
    // own, never what the compositor hands out itself.
    let saved: serde_json::Value = serde_json::from_str(&get(ede_protocol::process::PERSON_ENV)[0]).unwrap();
    assert_eq!(saved, serde_json::json!({"LD_PRELOAD": "x.so", "VK_ICD_FILENAMES": "y.json", "VK_LOADER_LAYERS_DISABLE": "nothing"}));
}
