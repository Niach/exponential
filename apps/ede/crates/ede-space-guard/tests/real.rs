//! #EDE-146, #EDE-148 on a real kernel and a real user manager (CI's
//! `ede-scope` leg): the guard, run as root, guards a space-named slice of
//! this user when a process inside it asks; then nothing in that slice opens
//! a sound, camera, uinput or (for writing) rfkill node, nor connects or
//! sends to the system bus, the person's session bus, user manager, sound
//! servers, tmux server or Wayland socket (also through a snap's link to
//! it; with the NUL in the address's length, without
//! it as libdbus connects, and the whole struct as libpulse and GLib pass
//! it), while the space's own tmux server and Wayland socket are reached; outside it everything is as
//! before, and the user cannot take the programs off. polkit, with the
//! package's rule installed, challenges the probe action for a process of
//! the slice of all spaces and allows it outside. #EDE-157: where the
//! kernel runs BPF LSM programs, a process of the slice cannot signal a
//! process outside it (`kill -0` still answers), and signals one inside;
//! elsewhere the guard says signals stay open.
//!
//! Runs only with `EDE_SPACE_GUARD_REAL=1` (it uses passwordless sudo to
//! start the guard and to make device nodes); otherwise it says so and
//! passes. Run it with `--test-threads=1`.

use std::{
    os::{
        fd::AsFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
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

/// What connecting to each path gives, as `path nul|bare|full errno`
/// lines: the address's length with the NUL (CPython, sd-bus, std), without
/// it (libdbus), the whole struct (libpulse, GLib, tmux); and a datagram's
/// `sendto` to it (`send`).
fn connects(paths: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    for path in paths {
        for (how, kind) in [("nul", 0), ("bare", 1), ("full", 2), ("send", 3)] {
            // SAFETY: sockets of ours and a sockaddr_un filled within its size.
            let errno = unsafe {
                let ty = if kind == 3 { libc::SOCK_DGRAM } else { libc::SOCK_STREAM };
                let fd = libc::socket(libc::AF_UNIX, ty | libc::SOCK_CLOEXEC, 0);
                let mut addr: libc::sockaddr_un = std::mem::zeroed();
                addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
                for (dst, src) in addr.sun_path.iter_mut().zip(path.bytes()) {
                    *dst = src as libc::c_char;
                }
                let len = match kind {
                    0 | 3 => 2 + path.len() + 1,
                    1 => 2 + path.len(),
                    _ => std::mem::size_of::<libc::sockaddr_un>(),
                };
                let r = if kind == 3 {
                    libc::sendto(fd, b"x".as_ptr().cast(), 1, 0, (&addr as *const libc::sockaddr_un).cast(), len as libc::socklen_t) as i32
                } else {
                    libc::connect(fd, (&addr as *const libc::sockaddr_un).cast(), len as libc::socklen_t)
                };
                let errno = if r >= 0 { 0 } else { *libc::__errno_location() };
                libc::close(fd);
                errno
            };
            lines.push(format!("{path} {how} {errno}"));
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
        let paths: Vec<String> = std::env::var("EDE_GUARD_HELPER_CONNECT").unwrap_or_default().split('|').filter(|p| !p.is_empty()).map(String::from).collect();
        for line in connects(&paths) {
            println!("CONNECT {line}");
        }
        if let Some(outside) = std::env::var("EDE_GUARD_HELPER_KILL").ok().and_then(|pid| pid.parse::<i32>().ok()) {
            for line in kills(outside) {
                println!("KILL {line}");
            }
        }
    }
}

/// #EDE-157: what signalling gives, as `what errno` lines: a process
/// outside (`outside0` = signal 0, `outside` = SIGTERM), and a sleeper of
/// the helper's own (`inside <errno> <signal it ended by>`).
fn kills(outside: i32) -> Vec<String> {
    let errno = |r: i32| if r == 0 { 0 } else { std::io::Error::last_os_error().raw_os_error().unwrap_or(-1) };
    // SAFETY: kill(2) of given pids.
    let mut lines = vec![format!("outside0 {}", errno(unsafe { libc::kill(outside, 0) })), format!("outside {}", errno(unsafe { libc::kill(outside, libc::SIGTERM) }))];
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    // SAFETY: as above, our own child.
    let sent = errno(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) });
    if sent != 0 {
        let _ = child.kill();
    }
    let status = child.wait().unwrap();
    lines.push(format!("inside {sent} {}", std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0)));
    lines
}

struct Run {
    dir: PathBuf,
    socket_dir: PathBuf,
    guard: Child,
    slice: String,
    runtime_dirs: Vec<PathBuf>,
    /// Files, then directories, the test made outside its own directories.
    leftovers: Vec<PathBuf>,
    /// The polkit files the test installed where none was (an installed
    /// package's stay).
    polkit_files: Vec<PathBuf>,
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = Command::new("systemctl").args(["--user", "stop", &self.slice]).status();
        let _ = Command::new("sudo").args(["-n", "kill", &self.guard.id().to_string()]).status();
        let _ = self.guard.wait();
        let _ = Command::new("sudo").args(["-n", "rm", "-rf"]).arg(&self.dir).arg(&self.socket_dir).args(&self.polkit_files).status();
        for dir in &self.runtime_dirs {
            let _ = std::fs::remove_dir_all(dir);
        }
        for path in &self.leftovers {
            let _ = std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path));
        }
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
    // The guard's socket where any user can reach it (the flood below).
    let socket_dir = PathBuf::from(format!("/tmp/ede-space-guard-{me}"));
    std::fs::create_dir_all(&socket_dir).unwrap();
    // Whatever the umask (a 077 one made it the test user's only).
    std::fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let socket = socket_dir.join("guard.sock");
    let guard = Command::new("sudo").args(["-n", GUARD, "--socket"]).arg(&socket).spawn().unwrap();
    let slice = format!("app-exponential_p{me}.slice");
    // Stand-ins in the runtime directory: one of the person's sound servers
    // (an unused name), and a socket of the space's own.
    let runtime = PathBuf::from(format!("/run/user/{uid}"));
    let test_dir = runtime.join(format!("ede-guard-test-{me}"));
    // #EDE-145: the directory of spaces is named after the person's Wayland
    // socket (a stand-in of an unused name); a snap's link to it, as the
    // snap's desktop launcher makes it, and one to the space's own socket.
    let display = format!("ede-guard-wayland-{me}");
    let spaces_dir = runtime.join(format!("exponential-desktop/{display}.spaces"));
    let person_wayland = runtime.join(&display);
    let snap_dir = runtime.join(format!("snap.ede-guard-test-{me}"));
    let space_wayland = spaces_dir.join("space-1/wayland-0");
    // The package's polkit rule and probe action, installed for the test.
    let rule = PathBuf::from("/etc/polkit-1/rules.d/00-00-exponential-agent-spaces.rules");
    let action = PathBuf::from("/usr/share/polkit-1/actions/at.exponential.desktop.space-probe.policy");
    // The person's tmux server by its default directory (a stand-in in
    // it), and the space's own where a launch's TMUX_TMPDIR puts it
    // (#EDE-138): `tmux` in a space works, the person's is out of reach.
    let tmux_dir = PathBuf::from(format!("/tmp/tmux-{uid}"));
    let person_tmux = tmux_dir.join(format!("ede-guard-test-{me}"));
    let space_tmux = spaces_dir.join(format!("space-1/tmux-{uid}/default"));
    let mut leftovers = vec![person_tmux.clone(), person_wayland.clone()];
    if !tmux_dir.exists() {
        leftovers.push(tmux_dir.clone());
    }
    let run = Run { dir: dir.clone(), socket_dir: socket_dir.clone(), guard, slice, runtime_dirs: vec![test_dir.clone(), spaces_dir.clone(), snap_dir.clone()], leftovers, polkit_files: [&rule, &action].into_iter().filter(|p| !p.exists()).cloned().collect() };
    std::fs::create_dir_all(test_dir.join("pulse")).unwrap();
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&tmux_dir).unwrap();
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(space_tmux.parent().unwrap()).unwrap();
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&snap_dir).unwrap();
    let listeners: Vec<std::os::unix::net::UnixListener> = [test_dir.join("pulse/native"), test_dir.join("pipewire-0"), spaces_dir.join("space-1/bus"), person_tmux.clone(), space_tmux.clone(), person_wayland.clone(), space_wayland.clone()].iter().map(|p| std::os::unix::net::UnixListener::bind(p).unwrap()).collect();
    std::os::unix::fs::symlink(&person_wayland, snap_dir.join(&display)).unwrap();
    std::os::unix::fs::symlink(&space_wayland, snap_dir.join("wayland-0")).unwrap();
    let person = |p: &Path| p.to_string_lossy().into_owned();
    let refused: Vec<String> = ["/run/dbus/system_bus_socket".to_string(), format!("/run/user/{uid}/bus"), format!("/run/user/{uid}/systemd/private"), format!("{}/../bus", person(&test_dir)), person(&test_dir.join("pulse/native")), person(&test_dir.join("pipewire-0")), format!("{}/../pulse/native", person(&test_dir)), person(&person_tmux), person(&person_wayland), person(&snap_dir.join(&display))]
        .into_iter()
        .filter(|p| Path::new(p).exists() || p.contains("/../"))
        .collect();
    let reached = vec![person(&spaces_dir.join("space-1/bus")), person(&space_tmux), person(&space_wayland), person(&snap_dir.join("wayland-0")), "/run/systemd/journal/stdout".to_string()];
    assert!(refused.contains(&person(&person_wayland)) && refused.contains(&person(&snap_dir.join(&display))), "{refused:?}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < deadline, "the guard never listened");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(ede_space_guard::client::ask(&socket, &Request::Probe), Ok(()), "this kernel does not load the device program");
    // #EDE-145: this guard refuses the person's Wayland socket, and says so.
    assert_eq!(ede_space_guard::client::probe(&socket), Ok(ede_space_guard::client::Probed::Wayland));

    // Review B1/M1: another user (nobody: no runtime directory, the shared
    // bucket) holding 16 silent connections does not refuse this user, and
    // 16 silent connections of this user's own are let go after the
    // request deadline (the client asks again after `busy`).
    let flood = "import socket, sys, time\nconns = []\nfor _ in range(16):\n    s = socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); conns.append(s)\nprint('held', flush=True)\ntime.sleep(30)\n";
    let mut nobody = Command::new("sudo").args(["-n", "-u", "nobody", "python3", "-c", flood]).arg(&socket).stdout(Stdio::piped()).spawn().unwrap();
    let mut held = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(nobody.stdout.as_mut().unwrap()), &mut held).unwrap();
    assert_eq!(held.trim(), "held");
    let asked = Instant::now();
    assert_eq!(ede_space_guard::client::ask(&socket, &Request::Probe), Ok(()), "another user's flood refused this user");
    assert!(asked.elapsed() < Duration::from_millis(900), "this user waited for another's flood: {:?}", asked.elapsed());
    let own: Vec<std::os::unix::net::UnixStream> = (0..16).map(|_| std::os::unix::net::UnixStream::connect(&socket).unwrap()).collect();
    assert_eq!(ede_space_guard::client::ask(&socket, &Request::Probe), Ok(()), "this user's own silent connections were never let go");
    drop(own);
    let _ = Command::new("sudo").args(["-n", "kill", &nobody.id().to_string()]).status();
    let _ = Command::new("sudo").args(["-n", "pkill", "-u", "nobody", "-f", "import socket, sys, time"]).status();
    let _ = nobody.wait();

    // Before: every node opens as its driver allows (no driver here: not EPERM).
    let outside = opens(&dir);
    assert_eq!(errno(&outside, "null", "rw"), 0, "{} does not allow device nodes (nodev?): set EDE_GUARD_NODES_DIR", dir.display());
    for (name, ..) in NODES {
        assert_ne!(errno(&outside, name, "rw"), libc::EPERM, "{name} is refused outside any space: {outside:?}");
    }

    // A process that is not in a space slice is refused.
    let spaces = person(&spaces_dir);
    let not_in_a_space = ede_space_guard::client::ask(&socket, &Request::Enter { spaces: spaces.clone() });
    assert!(not_in_a_space.as_ref().is_err_and(|r| r.contains("is not in an agent space")), "{not_in_a_space:?}");

    // #EDE-157: whether this kernel runs the signal program (`bpf` among
    // its LSMs), and a sleeper of the person's (this test's) outside.
    let bpf_lsm = std::fs::read_to_string("/sys/kernel/security/lsm").is_ok_and(|lsm| ede_space_guard::signals::bpf_lsm_active(&lsm));
    let signals = ede_space_guard::client::probe_signals(&socket);
    eprintln!("this kernel's BPF LSM: {}; the guard: signals {signals:?}", if bpf_lsm { "on" } else { "off" });
    if bpf_lsm {
        assert_eq!(signals, Ok(()), "the BPF LSM is on, but the guard leaves signals open");
    } else {
        assert!(signals.as_ref().is_err_and(|why| why.contains("no BPF LSM")), "{signals:?}");
    }
    let mut sleeper = Command::new("sleep").arg("120").spawn().unwrap();

    // A launch into space 1: the guard is asked from inside the scope,
    // then the helper opens the nodes.
    let inside = |unit: &str, kill: Option<u32>| {
        let out = Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect", &format!("--slice=app-exponential_p{me}-s1.slice"), &format!("--unit={unit}")])
            .arg(format!("--setenv=EDE_GUARD_HELPER_DIR={}", dir.display()))
            .arg(format!("--setenv=EDE_GUARD_HELPER_CONNECT={}", refused.iter().chain(&reached).cloned().collect::<Vec<_>>().join("|")))
            .arg(format!("--setenv=EDE_GUARD_HELPER_KILL={}", kill.map(|pid| pid.to_string()).unwrap_or_default()))
            .args(["--", GUARD, "enter", "--socket"])
            .arg(&socket)
            .args([&spaces, "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "the_helper", "--nocapture", "--test-threads=1"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        // The test harness prints `test the_helper ... ` before the first line.
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let take = |prefix: &str| stdout.lines().filter_map(|line| line.find(prefix).map(|at| line[at + prefix.len()..].to_string())).collect::<Vec<_>>();
        (take("OPEN "), take("CONNECT "), take("KILL "))
    };
    let (lines, connected, killed) = inside(&format!("exponential-p{me}-s1-1.scope"), Some(sleeper.id()));
    // #EDE-157: inside, a signal of the space's own is delivered; outside,
    // `kill -0` answers, and a signal is refused where the BPF LSM runs.
    assert!(killed.contains(&"inside 0 15".to_string()), "{killed:?}");
    assert!(killed.contains(&"outside0 0".to_string()), "{killed:?}");
    if bpf_lsm {
        assert!(killed.contains(&format!("outside {}", libc::EPERM)), "a space signalled the person's process: {killed:?}");
        assert!(sleeper.try_wait().unwrap().is_none(), "the person's sleeper ended");
    } else {
        assert!(killed.contains(&"outside 0".to_string()), "{killed:?}");
    }
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    for (name, mode) in [("snd", "r"), ("snd", "rw"), ("video", "r"), ("video", "rw"), ("uinput", "rw"), ("rfkill", "rw")] {
        assert_eq!(errno(&lines, name, mode), libc::EPERM, "{name} {mode} is open in a space: {lines:?}");
    }
    for (name, mode) in [("null", "rw"), ("rfkill", "r")] {
        assert_ne!(errno(&lines, name, mode), libc::EPERM, "{name} {mode} is closed in a space: {lines:?}");
    }
    // The person's buses, user manager, sound servers and Wayland socket,
    // every way of passing the address; the space's own sockets (its
    // Wayland socket also through a snap's link) and journald reachable.
    assert!(refused.len() >= 6, "too few of the person's sockets on this machine: {refused:?}");
    for path in &refused {
        for how in ["nul", "bare", "full", "send"] {
            assert_eq!(errno(&connected, path, how), libc::EPERM, "{path} ({how}) is reachable from a space: {connected:?}");
        }
    }
    for path in &reached {
        for how in ["nul", "bare", "full"] {
            assert_eq!(errno(&connected, path, how), 0, "{path} ({how}) is closed to a space: {connected:?}");
        }
    }
    // From outside, the same sockets connect.
    for line in connects(&refused).into_iter().filter(|line| !line.contains("/../") && !line.contains(" send ")) {
        assert!(line.ends_with(" 0"), "outside a space: {line}");
    }

    // polkit with the package's rule: the probe action is challenged for a
    // process of the slice of all spaces (as the compositor's probe asks)
    // and allowed outside it.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/polkit");
    sudo(&["install", "-Dm644", &here.join("00-00-exponential-agent-spaces.rules").to_string_lossy(), &rule.to_string_lossy()]);
    sudo(&["install", "-Dm644", &here.join("at.exponential.desktop.space-probe.policy").to_string_lossy(), &action.to_string_lossy()]);
    std::thread::sleep(Duration::from_secs(2));
    let pkcheck = |slice: Option<String>| {
        let mut cmd = Command::new("systemd-run");
        cmd.args(["--user", "--scope", "--quiet", "--collect"]);
        if let Some(slice) = slice {
            cmd.arg(format!("--slice={slice}"));
        }
        cmd.args(["--expand-environment=no", "--", "/bin/sh", "-c", "/usr/bin/pkcheck --action-id at.exponential.desktop.space-probe --process $$ >/dev/null 2>&1; echo pkcheck=$?"]);
        String::from_utf8_lossy(&cmd.output().unwrap().stdout).trim().to_string()
    };
    assert_eq!(pkcheck(Some(format!("app-exponential_p{me}.slice"))), "pkcheck=2", "polkit did not challenge a process of the spaces' slice");
    assert_eq!(pkcheck(None), "pkcheck=0", "polkit refused the person");

    // The user takes the program off: refused, and the next launch of the
    // space finds the slice guarded.
    let components: Vec<String> = ["user.slice".to_string(), format!("user-{uid}.slice"), format!("user@{uid}.service"), "app.slice".into(), format!("app-exponential_p{me}.slice"), format!("app-exponential_p{me}-s1.slice")].into();
    let slice = sys::open_cgroup(Path::new("/sys/fs/cgroup"), &components, uid).unwrap();
    assert!(sys::prog_detach_any(slice.as_fd(), sys::BPF_CGROUP_DEVICE).is_err(), "the user detached the guard's program");
    assert!(sys::prog_detach_any(slice.as_fd(), sys::BPF_CGROUP_UNIX_CONNECT).is_err(), "the user detached the guard's connect program");
    if bpf_lsm {
        assert!(sys::prog_detach_any(slice.as_fd(), sys::BPF_LSM_CGROUP).is_err(), "the user detached the guard's signal program");
    }
    let (again, connected_again, _) = inside(&format!("exponential-p{me}-s1-2.scope"), None);
    assert_eq!(errno(&again, "snd", "rw"), libc::EPERM, "{again:?}");
    assert_eq!(errno(&connected_again, "/run/dbus/system_bus_socket", "bare"), libc::EPERM, "{connected_again:?}");

    // Outside, still as before.
    assert_eq!(opens(&dir), outside);
    drop(listeners);
    drop(run);
}
