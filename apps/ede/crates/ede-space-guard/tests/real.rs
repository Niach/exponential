//! #EDE-148 on a real kernel and a real user manager (CI's `ede-scope`
//! leg): the guard, run as root, guards a space-named slice of this user
//! when a process inside it asks; then nothing in that slice opens a sound,
//! camera, uinput or (for writing) rfkill node, outside it everything is as
//! before, and the user cannot take the program off again.
//!
//! Runs only with `EDE_SPACE_GUARD_REAL=1` (it uses passwordless sudo to
//! start the guard and to make device nodes); otherwise it says so and
//! passes. Run it with `--test-threads=1`.

use std::{
    os::{fd::AsFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use ede_space_guard::{sys, Request};

const GUARD: &str = env!("CARGO_BIN_EXE_exponential-space-guard");
/// The test's device nodes: (file, major, minor).
const NODES: [(&str, u32, u32); 5] = [("snd", 116, 0), ("video", 81, 0), ("uinput", 10, 223), ("rfkill", 10, 242), ("null", 1, 3)];

fn ok(cmd: &mut Command) -> bool {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn sudo(args: &[&str]) {
    let status = Command::new("sudo").arg("-n").args(args).status().unwrap();
    assert!(status.success(), "sudo {args:?}");
}

/// What opening each node gives, as `name mode errno` lines (`0` = it opened).
fn opens(dir: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    for (name, ..) in NODES {
        for (mode, write) in [("r", false), ("rw", true)] {
            let result = std::fs::OpenOptions::new().read(true).write(write).custom_flags(libc::O_NONBLOCK).open(dir.join(name));
            lines.push(format!("{name} {mode} {}", result.err().and_then(|e| e.raw_os_error()).unwrap_or(0)));
        }
    }
    lines
}

/// The helper run inside the slice: this test binary, `the_helper` only.
#[test]
fn the_helper() {
    if let Ok(dir) = std::env::var("EDE_GUARD_HELPER_DIR") {
        for line in opens(Path::new(&dir)) {
            println!("OPEN {line}");
        }
    }
}

struct Run {
    dir: PathBuf,
    guard: Child,
    slice: String,
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = Command::new("systemctl").args(["--user", "stop", &self.slice]).status();
        let _ = Command::new("sudo").args(["-n", "kill", &self.guard.id().to_string()]).status();
        let _ = self.guard.wait();
        let _ = Command::new("sudo").args(["-n", "rm", "-rf"]).arg(&self.dir).status();
    }
}

fn errno(lines: &[String], name: &str, mode: &str) -> i32 {
    let prefix = format!("{name} {mode} ");
    lines.iter().find_map(|line| line.strip_prefix(&prefix)).unwrap_or_else(|| panic!("no {prefix}in {lines:?}")).parse().unwrap()
}

#[test]
fn a_guarded_space_slice_opens_no_microphone_camera_or_uinput_and_the_user_cannot_undo_it() {
    if std::env::var("EDE_SPACE_GUARD_REAL").as_deref() != Ok("1") {
        eprintln!("skipped: set EDE_SPACE_GUARD_REAL=1 (needs passwordless sudo and a systemd user manager)");
        return;
    }
    assert!(ok(Command::new("sudo").args(["-n", "true"])), "no passwordless sudo");
    assert!(ok(Command::new("systemd-run").args(["--user", "--scope", "--quiet", "--collect", "true"])), "no systemd user manager");

    let me = std::process::id();
    let uid = unsafe { libc::getuid() };
    // Device nodes need a file system without `nodev` (a tmpfs /tmp has it).
    let nodes_root = std::env::var_os("EDE_GUARD_NODES_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")));
    let dir = nodes_root.join(format!("space-guard-{me}"));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, major, minor) in NODES {
        sudo(&["mknod", "-m", "666", &dir.join(name).to_string_lossy(), "c", &major.to_string(), &minor.to_string()]);
    }
    let socket = dir.join("guard.sock");
    let guard = Command::new("sudo").args(["-n", GUARD, "--socket"]).arg(&socket).spawn().unwrap();
    let slice = format!("app-exponential_p{me}.slice");
    let run = Run { dir: dir.clone(), guard, slice };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < deadline, "the guard never listened");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(ede_space_guard::client::ask(&socket, &Request::Probe), Ok(()), "this kernel does not load the device program");

    // Before: every node opens as its driver allows (no driver here: not EPERM).
    let outside = opens(&dir);
    assert_eq!(errno(&outside, "null", "rw"), 0, "{} does not allow device nodes (nodev?): set EDE_GUARD_NODES_DIR", dir.display());
    for (name, ..) in NODES {
        assert_ne!(errno(&outside, name, "rw"), libc::EPERM, "{name} is refused outside any space: {outside:?}");
    }

    // A process that is not in a space slice is refused.
    let spaces = format!("/run/user/{uid}/exponential-desktop/wayland-test.spaces");
    let refused = ede_space_guard::client::ask(&socket, &Request::Enter { spaces: spaces.clone() });
    assert!(refused.as_ref().is_err_and(|r| r.contains("is not in an agent space")), "{refused:?}");

    // A launch into space 1: the guard is asked from inside the scope,
    // then the helper opens the nodes.
    let inside = |unit: &str| {
        let out = Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect", &format!("--slice=app-exponential_p{me}-s1.slice"), &format!("--unit={unit}")])
            .arg(format!("--setenv=EDE_GUARD_HELPER_DIR={}", dir.display()))
            .args(["--", GUARD, "enter", "--socket"])
            .arg(&socket)
            .args([&spaces, "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "the_helper", "--nocapture", "--test-threads=1"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        // The test harness prints `test the_helper ... ` before the first line.
        String::from_utf8_lossy(&out.stdout).lines().filter_map(|line| line.find("OPEN ").map(|at| line[at + 5..].to_string())).collect::<Vec<_>>()
    };
    let lines = inside(&format!("exponential-p{me}-s1-1.scope"));
    for (name, mode) in [("snd", "r"), ("snd", "rw"), ("video", "r"), ("video", "rw"), ("uinput", "rw"), ("rfkill", "rw")] {
        assert_eq!(errno(&lines, name, mode), libc::EPERM, "{name} {mode} is open in a space: {lines:?}");
    }
    for (name, mode) in [("null", "rw"), ("rfkill", "r")] {
        assert_ne!(errno(&lines, name, mode), libc::EPERM, "{name} {mode} is closed in a space: {lines:?}");
    }

    // The user takes the program off: refused, and the next launch of the
    // space finds the slice guarded.
    let components: Vec<String> = ["user.slice".to_string(), format!("user-{uid}.slice"), format!("user@{uid}.service"), "app.slice".into(), format!("app-exponential_p{me}.slice"), format!("app-exponential_p{me}-s1.slice")].into();
    let slice = sys::open_cgroup(Path::new("/sys/fs/cgroup"), &components, uid).unwrap();
    assert!(sys::prog_detach_any(slice.as_fd(), sys::BPF_CGROUP_DEVICE).is_err(), "the user detached the guard's program");
    let again = inside(&format!("exponential-p{me}-s1-2.scope"));
    assert_eq!(errno(&again, "snd", "rw"), libc::EPERM, "{again:?}");

    // Outside, still as before.
    assert_eq!(opens(&dir), outside);
    drop(run);
}
