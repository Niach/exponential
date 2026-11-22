//! #EDE-172, eDE's acceptance test (L-69: it landed with the first PR,
//! ignored, and runs from eDE's flip on, P14). A compositor started as in
//! production (headless here) with a shell stand-in on its channel; a
//! process of the same user that holds no descriptor of the control plane
//! (a child of this test, never started by the compositor) gets nothing:
//!
//! - every 64-hex string it can read under `XDG_RUNTIME_DIR` and in every
//!   readable `/proc/*/environ` is refused at `hello` (the root token file
//!   stays, design v3: its value works only from eDE's own binaries and
//!   what the person allowed);
//! - an anonymous connection is refused `input`, `capture` and `hold_input`;
//! - a copy of it named like the app (`exponential`) presenting the file's
//!   token is asked about, the person (`--consent-answer deny`) says Don't
//!   allow, and it is refused;
//! - the Wayland virtual keyboard and input-method managers are not offered;
//! - there is no services socket to reach and it holds no socket;
//! - `ptrace`, `pidfd_getfd` and `/proc/<pid>/environ` of the shell stand-in
//!   are refused;
//!
//! and the person's focused window saw no key.
//!
//! The roles run as this test binary started again (`EDE172_ROLE`).

mod common;

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use common::Session;
use ede_protocol::{read_frame, write_frame, Call, CaptureTarget, Client, InputEvent, Message, Request, Token, PROTOCOL_VERSION};

const ROLE: &str = "EDE172_ROLE";
const SHELL_PID: &str = "EDE172_SHELL_PID";
const BLUE: u32 = 0xff00_00ff;

/// This test binary again, running only the test `name` (its role is set
/// after the environment is: [`ROLE`]).
fn role(name: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", name, "--ignored", "--nocapture", "--test-threads=1"]);
    command
}

/// `fd` at 3 in the child (the compositor's `--control-fd 3`, the shell's
/// inherited channel).
fn at_fd_3(command: &mut Command, fd: i32) {
    // Safety: dup2 and fcntl only, between fork and exec.
    unsafe {
        command.pre_exec(move || {
            let ok = if fd == 3 { libc::fcntl(3, libc::F_SETFD, 0) == 0 } else { libc::dup2(fd, 3) == 3 };
            if ok {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wayland_socket(runtime: &Path) -> Option<String> {
    std::fs::read_dir(runtime).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).find(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))
}

#[test]
fn a_process_of_the_person_without_a_channel_gets_nothing() {
    use std::os::fd::AsRawFd;
    let dir = tempfile::tempdir().unwrap();
    let runtime = dir.path().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let env = |command: &mut Command| {
        command.env_clear().env("PATH", std::env::var_os("PATH").unwrap_or_default()).env("HOME", dir.path()).env("XDG_RUNTIME_DIR", &runtime).env("EXP_DIAGNOSTICS", "0").env("RUST_LOG", "warn");
    };

    // The compositor, its channel at fd 3, as the session starts it.
    let (compositor_end, shell_end) = UnixStream::pair().unwrap();
    let mut compositor = Command::new(env!("CARGO_BIN_EXE_exponential-compositor"));
    // The root token's gate on (P5'; on by default in a real session since
    // eDE's flip, P14), the person saying Don't allow to every question.
    compositor.args(["--headless", "800x600", "--no-xwayland", "--safe-mode", "--control-fd", "3", "--consent-answer", "deny"]);
    env(&mut compositor);
    at_fd_3(&mut compositor, compositor_end.as_raw_fd());
    let _compositor = Killed(compositor.spawn().unwrap());
    drop(compositor_end);
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(name) = wayland_socket(&runtime).filter(|n| ede_protocol::socket_path(&runtime, n).exists()) {
            break name;
        }
        assert!(Instant::now() < deadline, "the compositor never listened");
        std::thread::sleep(Duration::from_millis(20));
    };

    // The shell stand-in holds the channel and refuses ptrace.
    let mut shell = role("role_shell_stand_in");
    env(&mut shell);
    shell.env(ROLE, "shell").env("WAYLAND_DISPLAY", &display).env(ede_protocol::CONTROL_FD_ENV, "3").stdin(Stdio::piped()).stdout(Stdio::piped());
    at_fd_3(&mut shell, shell_end.as_raw_fd());
    let mut shell = Killed(shell.spawn().unwrap());
    drop(shell_end);
    let mut said = BufReader::new(shell.0.stdout.take().unwrap()).lines();
    // libtest's own words come first, on the same line ("test … ... ").
    assert!(said.any(|line| line.is_ok_and(|l| l.ends_with("ready"))), "the shell stand-in never said ready");

    // The person's focused window.
    let mut app = Session::connect(UnixStream::connect(runtime.join(&display)).unwrap());
    app.keyboard();
    let window = app.toplevel("person");
    app.draw(&window, 300, 200, BLUE);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.keyboard_focus.is_none() {
        assert!(Instant::now() < deadline, "the person's window never took focus");
        std::thread::sleep(Duration::from_millis(10));
        app.roundtrip();
    }

    // The foreign process: a child of this test, same user, no descriptor.
    let mut foreign = role("role_foreign");
    env(&mut foreign);
    foreign.env(ROLE, "foreign").env("WAYLAND_DISPLAY", &display).env(SHELL_PID, shell.0.id().to_string()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    let output = foreign.output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    let failures: Vec<&str> = report.lines().filter_map(|l| l.split_once("GOT: ").map(|(_, what)| what)).collect();
    assert!(output.status.success(), "the foreign process failed to run: {report}");
    assert!(report.lines().any(|l| l.ends_with("DONE")), "the foreign process did not finish: {report}");

    let mut failures: Vec<String> = failures.into_iter().map(String::from).collect();
    // The same, named like the app: asked, refused (the person said no).
    let named = dir.path().join("named");
    std::fs::create_dir(&named).unwrap();
    let app_copy = named.join("exponential");
    std::fs::copy(std::env::current_exe().unwrap(), &app_copy).unwrap();
    let mut asking = Command::new(&app_copy);
    asking.args(["--exact", "role_named_like_the_app", "--ignored", "--nocapture", "--test-threads=1"]);
    env(&mut asking);
    asking.env(ROLE, "named").env("WAYLAND_DISPLAY", &display).stdout(Stdio::piped()).stderr(Stdio::inherit());
    let output = asking.output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    failures.extend(report.lines().filter_map(|l| l.split_once("GOT: ").map(|(_, what)| what.to_string())));
    assert!(report.lines().any(|l| l.ends_with("DONE")), "the copy named like the app did not finish: {report}");

    std::thread::sleep(Duration::from_millis(200));
    app.roundtrip();
    if !app.state.keys.is_empty() {
        failures.push(format!("the person's window saw keys {:?}", app.state.keys));
    }
    drop(shell.0.stdin.take());
    assert!(failures.is_empty(), "a process without a channel got through:\n{}", failures.join("\n"));
}

/// The shell stand-in: says hello over its inherited channel, refuses
/// ptrace (non-dumpable, as the shell is after its first roundtrip),
/// subscribes, says `ready`, and holds the channel until stdin closes.
#[test]
#[ignore = "#EDE-172: a role of the acceptance test"]
fn role_shell_stand_in() {
    if std::env::var(ROLE).as_deref() != Ok("shell") {
        return;
    }
    let mut client = Client::inherited().expect("a channel at fd 3").expect("hello over the channel");
    // Safety: a plain prctl on this process.
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) }, 0);
    client.subscribe().unwrap();
    println!("ready");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    while std::io::stdin().read_line(&mut line).is_ok_and(|n| n > 0) {
        line.clear();
    }
}

/// A copy of this binary named `exponential` (the CLI's name): it reads
/// the root token file and says hello with it; the person says no.
#[test]
#[ignore = "#EDE-172: a role of the acceptance test"]
fn role_named_like_the_app() {
    if std::env::var(ROLE).as_deref() != Ok("named") {
        return;
    }
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    let display = std::env::var("WAYLAND_DISPLAY").unwrap();
    let token = std::fs::read_to_string(ede_protocol::token_path(&runtime, &display)).unwrap();
    match Client::connect(&ede_protocol::socket_path(&runtime, &display), Token::new(token.trim()).unwrap()) {
        Ok(client) => println!("\nGOT: the file's token from a copy named like the app, refused by the person ({:?})", client.hello.capabilities),
        Err(e) => eprintln!("named like the app, refused: {e}"),
    }
    println!("\nDONE");
}

/// Every 64-hex run in `bytes` (and each 64-hex window of a longer run).
fn hex64(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..=bytes.len() {
        let hex = i < bytes.len() && matches!(bytes[i], b'0'..=b'9' | b'a'..=b'f');
        if !hex {
            if i - start >= 64 {
                for at in start..=i - 64 {
                    out.push(String::from_utf8_lossy(&bytes[at..at + 64]).into_owned());
                }
            }
            start = i + 1;
        }
    }
    out
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => files_under(&path, out),
            Ok(t) if t.is_file() => out.push(path),
            _ => {}
        }
    }
}

/// A raw call on `stream`: `Some(reply)`, `None` = refused or unanswered.
fn raw_call(stream: &mut UnixStream, id: u64, call: serde_json::Value) -> Option<serde_json::Value> {
    let mut call = call;
    call["id"] = id.into();
    write_frame(stream, &serde_json::to_vec(&call).unwrap()).ok()?;
    loop {
        match serde_json::from_slice::<Message>(&read_frame(stream).ok()?).ok()? {
            Message::Reply { id: got, result } if got == id => return Some(result),
            Message::Error { .. } => return None,
            _ => {}
        }
    }
}

/// The foreign process: prints `GOT: …` for each thing it got, then `DONE`.
#[test]
#[ignore = "#EDE-172: a role of the acceptance test"]
fn role_foreign() {
    if std::env::var(ROLE).as_deref() != Ok("foreign") {
        return;
    }
    let got = |what: String| println!("GOT: {what}");
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    let display = std::env::var("WAYLAND_DISPLAY").unwrap();
    let shell: i32 = std::env::var(SHELL_PID).unwrap().parse().unwrap();
    let socket = ede_protocol::socket_path(&runtime, &display);

    // No socket of anyone's among its descriptors.
    for fd in 3..1024 {
        // Safety: fstat on numbers; one not open fails.
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut stat) } == 0 && stat.st_mode & libc::S_IFMT == libc::S_IFSOCK {
            got(format!("it holds a socket at fd {fd}"));
        }
    }

    // Every 64-hex string it can read.
    let mut sources = Vec::new();
    files_under(&runtime, &mut sources);
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            sources.push(entry.path().join("environ"));
        }
    }
    let mut tried = std::collections::HashSet::new();
    for source in sources {
        let Ok(bytes) = std::fs::read(&source) else { continue };
        for candidate in hex64(&bytes) {
            if !tried.insert(candidate.clone()) {
                continue;
            }
            let Ok(token) = Token::new(candidate.clone()) else { continue };
            if let Ok(client) = Client::connect(&socket, token) {
                got(format!("hello with the string in {} (capabilities {:?})", source.display(), client.hello.capabilities));
            }
        }
    }

    // Anonymous: no input, no capture, no hold.
    match UnixStream::connect(&socket) {
        Ok(mut stream) => {
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let _ = raw_call(&mut stream, 1, serde_json::to_value(Call { id: 0, request: Request::Hello { version: PROTOCOL_VERSION, token: None } }).unwrap());
            let input = Request::Input { event: InputEvent::Key { key: 30, pressed: true }, bindings: false };
            let output = Request::Capture { target: CaptureTarget::Output { name: "HEADLESS-1".into() }, since: None };
            for (n, request) in [input, output, Request::HoldInput].into_iter().enumerate() {
                let op = serde_json::to_value(Call { id: 0, request }).unwrap();
                if raw_call(&mut stream, 10 + n as u64, op.clone()).is_some() {
                    got(format!("an anonymous {}", op["op"]));
                }
            }
        }
        Err(e) => got(format!("the control socket is not there to be refused on: {e}")),
    }

    // Wayland: no virtual keyboard, no input-method manager.
    let mut wayland = Session::connect(UnixStream::connect(runtime.join(&display)).unwrap());
    wayland.roundtrip();
    for global in ["zwp_virtual_keyboard_manager_v1", "zwp_input_method_manager_v2"] {
        if wayland.state.globals.iter().any(|g| g == global) {
            got(format!("the Wayland global {global}"));
        }
    }

    // No services socket.
    let services = runtime.join("exponential-desktop").join("services.sock");
    if UnixStream::connect(&services).is_ok() {
        got(format!("a connection to {}", services.display()));
    }

    // The shell stand-in: no ptrace, no descriptor, no environment.
    // Safety: ptrace, waitpid and pidfd syscalls on another process, each
    // checked; a success is undone at once.
    unsafe {
        if libc::ptrace(libc::PTRACE_ATTACH, shell, 0, 0) == 0 {
            libc::waitpid(shell, std::ptr::null_mut(), 0);
            libc::ptrace(libc::PTRACE_DETACH, shell, 0, 0);
            got("ptrace of the shell".into());
        }
        let pidfd = libc::syscall(libc::SYS_pidfd_open, shell, 0) as i32;
        if pidfd >= 0 {
            let taken = libc::syscall(libc::SYS_pidfd_getfd, pidfd, 3, 0) as i32;
            if taken >= 0 {
                libc::close(taken);
                got("the shell's fd 3 by pidfd_getfd".into());
            }
            libc::close(pidfd);
        }
    }
    if std::fs::read(format!("/proc/{shell}/environ")).is_ok() {
        got("the shell's environment".into());
    }
    println!("DONE");
}
