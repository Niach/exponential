//! Real spaces on this machine: a program inside one looks for everything it
//! must not reach. Skipped, saying why, where the machine cannot run spaces
//! (the same answer the doctor item gives).

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("skipped: Linux only");
}

#[cfg(target_os = "linux")]
fn main() {
    member_space::run_if_requested();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("__inside") => linux::inside(&args[2..]),
        _ => linux::all(),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::{Read as _, Write as _};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
    use std::os::linux::net::SocketAddrExt as _;
    use std::os::unix::net::{SocketAddr as UnixAddr, UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::process::Child;
    use std::time::{Duration, Instant};

    use member_space::{environment, spec_line, Spec};

    struct Dirs(PathBuf);

    impl Dirs {
        fn new(tag: &str) -> Dirs {
            let dir = std::env::temp_dir().join(format!("exp-space-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            for sub in ["a/data", "a/host", "b/data", "hostdata"] {
                std::fs::create_dir_all(dir.join(sub)).unwrap();
            }
            Dirs(dir.canonicalize().unwrap())
        }
        fn spec(&self, args: &[String]) -> Spec {
            Spec {
                data: self.0.join("a/data"),
                host: self.0.join("a/host"),
                tools: vec![("exponential".into(), exe())],
                args: std::iter::once("__inside".to_string()).chain(args.iter().cloned()).collect(),
                env: environment(std::env::vars()),
                instance: None,
                grace_secs: 2,
            }
        }
    }

    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn exe() -> PathBuf {
        std::env::current_exe().unwrap().canonicalize().unwrap()
    }

    fn launch(spec: &Spec) -> Child {
        let mut child = member_space::command(&exe()).spawn().unwrap();
        child.stdin.as_mut().unwrap().write_all(spec_line(spec).as_bytes()).unwrap();
        child
    }

    /// (exit code, stdout, stderr) within 30 s.
    fn finish(mut child: Child) -> (Option<i32>, String, String) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let code = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status.code();
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let (mut out, mut err) = (String::new(), String::new());
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        (code, out, err)
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|text| text.to_string()).collect()
    }

    pub fn all() {
        if let Err(reason) = member_space::probe(&exe()) {
            println!("skipped: this machine runs no member spaces: {reason}");
            return;
        }
        println!("probe ... ok");
        let tests: [(&str, fn()); 9] = [
            ("one_run_at_a_time", one_run_at_a_time),
            ("a_group_signal_still_saves", a_group_signal_still_saves),
            ("sees_nothing_of_the_host", sees_nothing_of_the_host),
            ("the_environment_does_not_leak", the_environment_does_not_leak),
            ("only_the_instance_is_reachable", only_the_instance_is_reachable),
            ("stops_gracefully", stops_gracefully),
            ("a_program_that_will_not_stop_is_killed", a_program_that_will_not_stop_is_killed),
            ("dies_with_its_host", dies_with_its_host),
            ("a_link_is_no_directory", a_link_is_no_directory),
        ];
        for (name, test) in tests {
            test();
            println!("{name} ... ok");
        }
    }

    fn sees_nothing_of_the_host() {
        let dirs = Dirs::new("sees");
        std::fs::write(dirs.0.join("b/data/secret"), "another space").unwrap();
        std::fs::write(dirs.0.join("hostdata/token"), "the host daemon").unwrap();
        std::fs::write(dirs.0.join("a/host/space.json"), "mine").unwrap();
        let name = format!("exp-space-test-{}", std::process::id());
        let _abstract = UnixListener::bind_addr(&UnixAddr::from_abstract_name(&name).unwrap()).unwrap();
        let home = std::env::var("HOME").unwrap_or_default();
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or("/run/user".into());
        let mut list = vec!["look".to_string(), std::process::id().to_string(), name];
        let b = dirs.0.join("b/data/secret").to_string_lossy().into_owned();
        let host = dirs.0.join("hostdata/token").to_string_lossy().into_owned();
        // The links a member's code would plant to reach out of its directory.
        let a = dirs.0.join("a/data");
        std::os::unix::fs::symlink(&b, a.join("planted")).unwrap();
        std::os::unix::fs::symlink("/proc/1/root/home", a.join("planted2")).unwrap();
        for path in [&b, &host, &home, &runtime] {
            list.push(path.clone());
        }
        let (code, out, err) = finish(launch(&dirs.spec(&list)));
        assert_eq!((code, out.trim()), (Some(0), "ok"), "{err}");
        assert_eq!(std::fs::read_to_string(a.join("written")).unwrap(), "x", "its own directory is read-write");
    }

    fn the_environment_does_not_leak() {
        let dirs = Dirs::new("env");
        std::env::set_var("EXP_TEST_SECRET", "leaks");
        let (code, out, err) = finish(launch(&dirs.spec(&args(&["env"]))));
        assert_eq!(code, Some(0), "{err}");
        assert!(!out.contains("leaks") && !out.contains("SSH_AUTH_SOCK") && !out.contains("DBUS"), "{out}");
        let home = std::env::var("HOME").unwrap();
        assert!(!out.contains(&home), "{out}");
    }

    fn only_the_instance_is_reachable() {
        let dirs = Dirs::new("net");
        let answering = || {
            let listener = TcpListener::bind("0.0.0.0:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut stream = stream;
                    let _ = stream.write_all(b"hello");
                }
            });
            port
        };
        let (instance, other) = (answering(), answering());
        // The machine's address on its network, when it has one.
        let lan = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|socket| socket.connect("192.0.2.1:9").and_then(|()| socket.local_addr()))
            .map_or("127.0.0.2".to_string(), |addr| addr.ip().to_string());
        let mut spec = dirs.spec(&args(&["net", &instance.to_string(), &other.to_string(), &lan]));
        spec.instance = Some(("127.0.0.1".into(), instance));
        let (code, out, err) = finish(launch(&spec));
        assert_eq!((code, out.trim()), (Some(0), "ok"), "{err}");
    }

    fn stops_gracefully() {
        let dirs = Dirs::new("stop");
        let mut child = launch(&dirs.spec(&args(&["wait"])));
        started(&dirs);
        child.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
        let (code, _, err) = finish(child);
        assert_eq!(code, Some(0), "{err}");
        assert!(dirs.0.join("a/data/saved").exists(), "SIGTERM reached the program");
    }

    fn a_program_that_will_not_stop_is_killed() {
        let dirs = Dirs::new("kill");
        let mut child = launch(&dirs.spec(&args(&["stubborn"])));
        started(&dirs);
        let since = Instant::now();
        child.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
        let (code, _, _) = finish(child);
        assert!(code.is_some() && code != Some(0));
        assert!(since.elapsed() >= Duration::from_secs(2) && since.elapsed() < Duration::from_secs(20));
    }

    fn dies_with_its_host() {
        let dirs = Dirs::new("death");
        let mut child = launch(&dirs.spec(&args(&["beat"])));
        started(&dirs);
        // SIGKILL of the launcher: nobody asks anything to stop.
        child.kill().unwrap();
        child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let beat = || std::fs::read_to_string(dirs.0.join("a/data/beat")).unwrap_or_default();
        let before = beat();
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(beat(), before, "the space's processes are gone");
    }

    fn one_run_at_a_time() {
        let dirs = Dirs::new("lock");
        let mut first = launch(&dirs.spec(&args(&["wait"])));
        started(&dirs);
        let (code, _, err) = finish(launch(&dirs.spec(&args(&["env"]))));
        assert_eq!(code, Some(125));
        assert!(err.contains("already running"), "{err}");
        first.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
        assert_eq!(finish(first).0, Some(0));
        assert_eq!(finish(launch(&dirs.spec(&args(&["env"])))).0, Some(0), "free again");
    }

    fn a_group_signal_still_saves() {
        let dirs = Dirs::new("group");
        let mut child = launch(&dirs.spec(&args(&["wait"])));
        started(&dirs);
        // A service stop signals the launcher too; the host then closes stdin.
        unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
        std::thread::sleep(Duration::from_millis(200));
        assert!(child.try_wait().unwrap().is_none());
        drop(child.stdin.take());
        assert_eq!(finish(child).0, Some(0));
        assert!(dirs.0.join("a/data/saved").exists());
    }

    fn a_link_is_no_directory() {
        let dirs = Dirs::new("link");
        std::os::unix::fs::symlink("/", dirs.0.join("a/link")).unwrap();
        let mut spec = dirs.spec(&args(&["env"]));
        spec.data = dirs.0.join("a/link");
        let (code, out, err) = finish(launch(&spec));
        assert_eq!((code, out.as_str()), (Some(125), ""), "{err}");
        // And a script's package tree is no tool.
        let script = dirs.0.join("tool");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let mut spec = dirs.spec(&args(&["env"]));
        spec.tools.push(("claude".into(), script));
        assert_eq!(finish(launch(&spec)).0, Some(125));
    }

    fn started(dirs: &Dirs) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !dirs.0.join("a/data/started").exists() {
            assert!(Instant::now() < deadline, "the program started");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // ── inside ────────────────────────────────────────────────────────────

    pub fn inside(args: &[String]) {
        match args[0].as_str() {
            "look" => look(&args[1..]),
            "env" => {
                for (key, value) in std::env::vars() {
                    println!("{key}={value}");
                }
                for pid in ["self", "1"] {
                    println!("{}", String::from_utf8_lossy(&std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default()));
                }
            }
            "net" => net(&args[1..]),
            "wait" | "stubborn" | "beat" => run(&args[0]),
            other => panic!("{other}"),
        }
    }

    fn look(args: &[String]) {
        let host_pid: i32 = args[0].parse().unwrap();
        for path in &args[2..] {
            assert!(!Path::new(path).exists(), "{path} is visible");
        }
        for path in ["/var/run/docker.sock", "/run/docker.sock", "/run/dbus", "/run/user", "/home", "/root", "/tmp/.X11-unix"] {
            assert!(!Path::new(path).exists(), "{path} is visible");
        }
        for planted in ["/space/planted", "/space/planted2"] {
            assert!(std::fs::read(planted).is_err() && std::fs::read_dir(planted).is_err(), "{planted}");
        }
        // No process of the host: not listed, not signalled, not traced.
        assert!(!Path::new(&format!("/proc/{host_pid}")).exists());
        unsafe {
            assert_eq!(libc::kill(host_pid, 0), -1);
            assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
            assert_eq!(libc::ptrace(libc::PTRACE_SEIZE, host_pid, 0, 0), -1);
            let mut byte = 0u8;
            let local = libc::iovec { iov_base: (&mut byte as *mut u8).cast(), iov_len: 1 };
            let remote = libc::iovec { iov_base: 0x1000 as *mut _, iov_len: 1 };
            assert_eq!(libc::process_vm_readv(host_pid, &local, 1, &remote, 1, 0), -1);
        }
        // No way back: no namespace, no mount, no privilege.
        unsafe {
            assert_eq!(libc::unshare(libc::CLONE_NEWUSER), -1);
            assert_eq!(libc::syscall(libc::SYS_clone, libc::CLONE_NEWUSER as u64 | libc::SIGCHLD as u64, 0, 0, 0, 0), -1);
            assert_eq!(libc::mount(c"none".as_ptr(), c"/tmp".as_ptr(), c"tmpfs".as_ptr(), 0, std::ptr::null()), -1);
            assert_eq!(libc::setns(0, 0), -1);
            assert_eq!(libc::syscall(libc::SYS_keyctl, 0, 0, 0, 0, 0), -1);
            assert_eq!(libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0), -1);
        }
        assert!(std::fs::metadata("/usr/bin/env").is_ok());
        assert!(UnixStream::connect_addr(&UnixAddr::from_abstract_name(&args[1]).unwrap()).is_err(), "an abstract socket of the host");
        assert_eq!(std::fs::read_to_string("/exp/host/space.json").unwrap(), "mine");
        assert!(std::fs::write("/exp/host/space.json", "y").is_err());
        assert!(std::fs::write("/usr/bin/x", "y").is_err());
        std::fs::write("/space/written", "x").unwrap();
        std::fs::write("/tmp/x", "y").unwrap();
        // Ordinary programs run.
        assert!(std::process::Command::new("/bin/sh").args(["-c", "true"]).status().unwrap().success());
        println!("ok");
    }

    fn hello(addr: SocketAddr) -> bool {
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(3)) else { return false };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let mut answer = [0u8; 5];
        stream.read_exact(&mut answer).is_ok() && &answer == b"hello"
    }

    fn net(args: &[String]) {
        let (instance, other): (u16, u16) = (args[0].parse().unwrap(), args[1].parse().unwrap());
        let lan: Ipv4Addr = args[2].parse().unwrap();
        let stand_in = member_space::net::INSTANCE_ADDR;
        assert!(hello((stand_in, instance).into()), "the instance");
        for addr in [
            SocketAddr::from((stand_in, other)),
            ([127, 0, 0, 1], instance).into(),
            ([127, 0, 0, 1], other).into(),
            (lan, instance).into(),
            (lan, other).into(),
            ([198, 18, 0, 2], other).into(),
            ([0, 0, 0, 0], other).into(),
        ] {
            assert!(!hello(addr), "{addr}");
        }
        println!("ok");
    }

    fn run(what: &str) {
        static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        extern "C" fn stop(_: libc::c_int) {
            STOP.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        unsafe {
            libc::signal(libc::SIGTERM, stop as extern "C" fn(libc::c_int) as *const () as libc::sighandler_t);
        }
        std::fs::write("/space/started", "x").unwrap();
        for beat in 0.. {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::write("/space/beat", beat.to_string()).unwrap();
            if what == "wait" && STOP.load(std::sync::atomic::Ordering::SeqCst) {
                std::fs::write("/space/saved", "x").unwrap();
                return;
            }
        }
    }
}
