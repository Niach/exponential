//! The launcher and what it builds (see the crate doc). Plain syscalls: the
//! launcher is single-threaded until its space's process tree stands.

use std::ffi::{CStr, CString};
use std::io::{self, BufRead as _, Read as _, Write as _};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{net, Spec, HOST_DIR, PROBE_ARG, PROGRAM, SPACE_HOME, TOOLS_DIR};

/// The launcher's exit code when the space could not be built.
const SETUP_FAILED: i32 = 125;
const TUN: &str = "exp0";

/// The system a space sees, read-only (a link of the host's root is kept a
/// link: `/bin -> usr/bin`).
const SYSTEM: [&str; 9] = ["usr", "bin", "sbin", "lib", "lib32", "lib64", "libx32", "libo32", "nix"];
/// Of `/etc` only what programs need to run: not the whole directory, where
/// a machine keeps its own secrets (npmrc, gitconfig, mounted credentials).
const ETC: [&str; 26] = [
    "alternatives", "ssl", "pki", "ca-certificates", "ca-certificates.conf", "crypto-policies", "ld.so.cache",
    "ld.so.conf", "ld.so.conf.d", "localtime", "timezone", "os-release", "lsb-release", "debian_version",
    "redhat-release", "mime.types", "magic", "protocols", "services", "fonts", "terminfo", "bash.bashrc", "shells",
    "gai.conf", "static", "openssl",
];

type Step<T> = Result<T, String>;

fn os(step: &str) -> String {
    format!("{step}: {}", io::Error::last_os_error())
}

fn check(result: libc::c_int, step: &str) -> Step<()> {
    if result < 0 { Err(os(step)) } else { Ok(()) }
}

fn cstr(path: &Path) -> Step<CString> {
    use std::os::unix::ffi::OsStrExt as _;
    CString::new(path.as_os_str().as_bytes()).map_err(|_| "a path with a NUL byte".to_string())
}

fn c(text: &str) -> CString {
    CString::new(text).unwrap_or_default()
}

pub fn unsupported() -> Option<&'static str> {
    let (real, effective) = unsafe { (libc::getuid(), libc::geteuid()) };
    if real == 0 || effective == 0 {
        // A namespace whose only user IS the host's root is no boundary.
        return Some("The daemon runs as root");
    }
    if real != effective {
        return Some("The daemon runs with changed privileges");
    }
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return Some("Not on this processor");
    }
    if !Path::new("/dev/net/tun").exists() {
        return Some("No /dev/net/tun");
    }
    None
}

// ── the launcher ──────────────────────────────────────────────────────────

pub fn launcher_main() -> ! {
    match launch() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("member-space: {err}");
            std::process::exit(SETUP_FAILED);
        }
    }
}

fn read_spec() -> Step<Spec> {
    // Byte-wise: what follows the line on stdin is read later.
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut stdin = unsafe { std::fs::File::from_raw_fd(libc::dup(0)) };
    loop {
        match stdin.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => break,
            Ok(1) if line.len() < 1 << 20 => line.push(byte[0]),
            _ => return Err("no spec".into()),
        }
    }
    serde_json::from_slice(&line).map_err(|err| format!("the spec: {err}"))
}

/// The space's dirs and tools, as the host resolved them: no link may stand
/// for another place, and nothing of the host may lie in what a space sees.
fn validate(spec: &Spec) -> Step<()> {
    if let Some(reason) = unsupported() {
        return Err(reason.to_string());
    }
    for path in [&spec.data, &spec.host].into_iter().chain(spec.tools.iter().map(|(_, path)| path)) {
        let real = path.canonicalize().map_err(|err| format!("{}: {err}", path.display()))?;
        if real != *path {
            return Err(format!("{} is not a real path", path.display()));
        }
    }
    for dir in [&spec.data, &spec.host] {
        if SYSTEM.iter().any(|root| dir.starts_with(Path::new("/").join(root))) {
            return Err(format!("{} lies in the system", dir.display()));
        }
    }
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if SYSTEM.iter().any(|root| home.starts_with(Path::new("/").join(root))) {
            return Err(format!("the home directory {} lies in the system", home.display()));
        }
    }
    for (name, path) in &spec.tools {
        let plain = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !plain || !crate::bindable(path) {
            return Err(format!("{} is no tool", path.display()));
        }
    }
    if !spec.tools.iter().any(|(name, _)| Path::new(TOOLS_DIR).join(name) == Path::new(PROGRAM)) {
        return Err("the program is missing".into());
    }
    Ok(())
}

/// The home directory the machine says this user has.
fn home_dir() -> Option<PathBuf> {
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_dir.is_null() {
            return None;
        }
        let dir = CStr::from_ptr((*entry).pw_dir).to_string_lossy().into_owned();
        (dir.len() > 1).then(|| PathBuf::from(dir))
    }
}

fn pair(kind: libc::c_int) -> Step<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as RawFd; 2];
    check(unsafe { libc::socketpair(libc::AF_UNIX, kind | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr()) }, "socketpair")?;
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn launch() -> Step<i32> {
    let spec = read_spec()?;
    validate(&spec)?;
    let (tun_out, tun_in) = pair(libc::SOCK_SEQPACKET)?;
    let (stop_out, stop_in) = pair(libc::SOCK_STREAM)?;
    let me = unsafe { libc::getpid() };
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(os("fork"));
    }
    if child == 0 {
        drop((tun_out, stop_out));
        let code = enter(&spec, me, tun_in, stop_in).unwrap_or_else(|err| {
            eprintln!("member-space: {err}");
            SETUP_FAILED
        });
        unsafe { libc::_exit(code) };
    }
    drop((tun_in, stop_in));
    let tun = match receive_fd(&tun_out) {
        Ok(tun) => tun,
        // The child says why on stderr.
        Err(_) => return Ok(wait_code(child)),
    };
    drop(tun_out);
    // The host asks for the end (a `stop` line) or is gone (end of file).
    let grace = Duration::from_secs(spec.grace_secs);
    std::thread::spawn(move || {
        let mut line = String::new();
        let mut stdin = io::stdin().lock();
        while matches!(stdin.read_line(&mut line), Ok(n) if n > 0) {
            if line.trim() == "stop" {
                break;
            }
            line.clear();
        }
        drop(stop_out);
        std::thread::sleep(grace + Duration::from_secs(5));
        unsafe { libc::kill(child, libc::SIGKILL) };
    });
    std::thread::spawn(move || {
        let code = wait_code(child);
        std::process::exit(code);
    });
    // The network, on this thread (the child's parent-death signal is this
    // thread's): without it the space ends.
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|err| format!("the network: {err}"))?;
    let ended = runtime.block_on(net::stack::serve(tun, spec.instance.clone()));
    unsafe { libc::kill(child, libc::SIGKILL) };
    Err(format!("the network ended: {ended:?}"))
}

fn wait_code(pid: libc::pid_t) -> i32 {
    let mut status = 0;
    loop {
        let got = unsafe { libc::waitpid(pid, &mut status, 0) };
        if got == pid {
            break;
        }
        if got < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return SETUP_FAILED;
        }
    }
    if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { 128 + libc::WTERMSIG(status) }
}

fn send_fd(socket: &OwnedFd, fd: RawFd) -> Step<()> {
    unsafe {
        let mut byte = 1u8;
        let mut iov = libc::iovec { iov_base: (&mut byte as *mut u8).cast(), iov_len: 1 };
        let mut control = [0u64; 8];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = libc::CMSG_SPACE(4) as _;
        let header = libc::CMSG_FIRSTHDR(&msg);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(4) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(header).cast::<RawFd>(), fd);
        if libc::sendmsg(socket.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) != 1 {
            return Err(os("the network device"));
        }
    }
    Ok(())
}

fn receive_fd(socket: &OwnedFd) -> Step<OwnedFd> {
    unsafe {
        let mut byte = 0u8;
        let mut iov = libc::iovec { iov_base: (&mut byte as *mut u8).cast(), iov_len: 1 };
        let mut control = [0u64; 8];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = libc::CMSG_SPACE(4) as _;
        if libc::recvmsg(socket.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC) != 1 {
            return Err(os("no network device"));
        }
        let header = libc::CMSG_FIRSTHDR(&msg);
        if header.is_null() || (*header).cmsg_type != libc::SCM_RIGHTS {
            return Err("no network device".into());
        }
        Ok(OwnedFd::from_raw_fd(std::ptr::read_unaligned(libc::CMSG_DATA(header).cast::<RawFd>())))
    }
}

// ── the namespaces ────────────────────────────────────────────────────────

/// In the launcher's child: every namespace, the network device, then the
/// space's first process.
fn enter(spec: &Spec, parent: libc::pid_t, tun_in: OwnedFd, stop: OwnedFd) -> Step<i32> {
    unsafe {
        check(libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL), "the parent-death signal")?;
        if libc::getppid() != parent {
            return Err("the launcher is gone".into());
        }
        let (uid, gid) = (libc::getuid(), libc::getgid());
        let namespaces = libc::CLONE_NEWUSER
            | libc::CLONE_NEWNS
            | libc::CLONE_NEWPID
            | libc::CLONE_NEWNET
            | libc::CLONE_NEWIPC
            | libc::CLONE_NEWUTS
            | libc::CLONE_NEWCGROUP;
        check(libc::unshare(namespaces), "namespaces")?;
        // One user, no groups: nothing to change to.
        for (file, text) in [
            ("/proc/self/setgroups", "deny".to_string()),
            ("/proc/self/uid_map", format!("{uid} {uid} 1\n")),
            ("/proc/self/gid_map", format!("{gid} {gid} 1\n")),
        ] {
            std::fs::write(file, text).map_err(|err| format!("{file}: {err}"))?;
        }
        let tun = network()?;
        send_fd(&tun_in, tun.as_raw_fd())?;
        drop((tun, tun_in));
        // The parent-death signal is cleared by a change of credentials.
        check(libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL), "the parent-death signal")?;
        let me = libc::getpid();
        let first = libc::fork();
        check(first, "fork")?;
        if first == 0 {
            let code = first_process(spec, me, stop).unwrap_or_else(|err| {
                eprintln!("member-space: {err}");
                SETUP_FAILED
            });
            libc::_exit(code);
        }
        drop(stop);
        Ok(wait_code(first))
    }
}

fn ifreq(name: &str) -> libc::ifreq {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    for (slot, byte) in request.ifr_name.iter_mut().zip(name.bytes()) {
        *slot = byte as libc::c_char;
    }
    request
}

fn sockaddr(ip: std::net::Ipv4Addr) -> libc::sockaddr {
    let mut v4: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    v4.sin_family = libc::AF_INET as _;
    v4.sin_addr.s_addr = u32::from(ip).to_be();
    unsafe { std::mem::transmute(v4) }
}

/// Loopback up, the TUN device with the space's address and the default route.
fn network() -> Step<OwnedFd> {
    const TUNSETIFF: libc::c_ulong = 0x4004_54ca;
    unsafe {
        let tun = libc::open(c"/dev/net/tun".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        check(tun, "/dev/net/tun")?;
        let tun = OwnedFd::from_raw_fd(tun);
        let mut request = ifreq(TUN);
        request.ifr_ifru.ifru_flags = (libc::IFF_TUN | libc::IFF_NO_PI) as _;
        check(libc::ioctl(tun.as_raw_fd(), TUNSETIFF as _, &mut request), "the network device")?;
        let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        check(socket, "socket")?;
        let socket = OwnedFd::from_raw_fd(socket);
        let at = socket.as_raw_fd();
        for name in ["lo", TUN] {
            let mut request = ifreq(name);
            if name == TUN {
                request.ifr_ifru.ifru_addr = sockaddr(net::SPACE_ADDR);
                check(libc::ioctl(at, libc::SIOCSIFADDR as _, &mut request), "the network address")?;
                request.ifr_ifru.ifru_netmask = sockaddr(std::net::Ipv4Addr::new(255, 255, 255, 0));
                check(libc::ioctl(at, libc::SIOCSIFNETMASK as _, &mut request), "the network mask")?;
                request.ifr_ifru.ifru_mtu = i32::from(net::stack::MTU);
                check(libc::ioctl(at, libc::SIOCSIFMTU as _, &mut request), "the network MTU")?;
            }
            request.ifr_ifru.ifru_flags = (libc::IFF_UP | libc::IFF_RUNNING) as _;
            check(libc::ioctl(at, libc::SIOCSIFFLAGS as _, &mut request), "the network")?;
        }
        let name = c(TUN);
        let any = sockaddr(std::net::Ipv4Addr::UNSPECIFIED);
        let mut route: libc::rtentry = std::mem::zeroed();
        route.rt_dst = any;
        route.rt_gateway = any;
        route.rt_genmask = any;
        route.rt_flags = libc::RTF_UP as _;
        route.rt_dev = name.as_ptr().cast_mut();
        check(libc::ioctl(at, libc::SIOCADDRT as _, &mut route), "the network route")?;
        Ok(tun)
    }
}

// ── the private root ──────────────────────────────────────────────────────

fn mount(source: &Path, target: &Path, kind: &str, flags: libc::c_ulong, data: &str) -> Step<()> {
    let (from, to, fs, options) = (cstr(source)?, cstr(target)?, c(kind), c(data));
    let result = unsafe { libc::mount(from.as_ptr(), to.as_ptr(), fs.as_ptr(), flags, options.as_ptr().cast()) };
    check(result, &format!("mount {}", target.display()))
}

#[repr(C)]
struct MountAttr {
    attr_set: u64,
    attr_clr: u64,
    propagation: u64,
    userns_fd: u64,
}

/// Sets mount attributes on `target` and everything below it.
fn restrict(target: &Path, read_only: bool, recursive: bool) -> Step<()> {
    const RDONLY: u64 = 1;
    const NOSUID: u64 = 2;
    const NODEV: u64 = 4;
    const AT_RECURSIVE: libc::c_uint = 0x8000;
    let path = cstr(target)?;
    let attr = MountAttr { attr_set: NOSUID | NODEV | if read_only { RDONLY } else { 0 }, attr_clr: 0, propagation: 0, userns_fd: 0 };
    let result = unsafe {
        libc::syscall(
            libc::SYS_mount_setattr,
            libc::AT_FDCWD,
            path.as_ptr(),
            if recursive { AT_RECURSIVE } else { 0 },
            &attr,
            std::mem::size_of::<MountAttr>(),
        )
    };
    check(result as libc::c_int, &format!("restrict {}", target.display()))
}

fn mkdir(path: &Path, mode: u32) -> Step<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    std::fs::DirBuilder::new().recursive(true).mode(mode).create(path).map_err(|err| format!("{}: {err}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|err| format!("{}: {err}", path.display()))
}

fn write(path: &Path, text: &str) -> Step<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|err| format!("{}: {err}", path.display()))
}

fn bind(source: &Path, target: &Path, read_only: bool) -> Step<()> {
    let meta = std::fs::metadata(source).map_err(|err| format!("{}: {err}", source.display()))?;
    if meta.is_dir() {
        std::fs::create_dir_all(target).map_err(|err| format!("{}: {err}", target.display()))?;
    } else {
        write(target, "")?;
    }
    mount(source, target, "", libc::MS_BIND | libc::MS_REC, "")?;
    // Private like the rest: nothing mounted on the host later shows up here.
    mount(Path::new(""), target, "", libc::MS_REC | libc::MS_PRIVATE, "")?;
    restrict(target, read_only, true)
}

/// `name` of the old root in the new one: a link stays a link, the rest is
/// bound read-only, what is missing stays missing.
fn system_entry(old: &Path, new: &Path) -> Step<()> {
    let Ok(meta) = std::fs::symlink_metadata(old) else { return Ok(()) };
    if meta.is_symlink() {
        let link = std::fs::read_link(old).map_err(|err| format!("{}: {err}", old.display()))?;
        return std::os::unix::fs::symlink(link, new).map_err(|err| format!("{}: {err}", new.display()));
    }
    bind(old, new, true)
}

fn pivot(new_root: &CStr, put_old: &CStr) -> Step<()> {
    let result = unsafe { libc::syscall(libc::SYS_pivot_root, new_root.as_ptr(), put_old.as_ptr()) };
    check(result as libc::c_int, "pivot_root")
}

fn private_root(spec: &Spec) -> Step<()> {
    let root = Path::new("/");
    mount(Path::new("none"), root, "", libc::MS_REC | libc::MS_PRIVATE, "")?;
    // A staging tmpfs becomes the root; the machine's lies under /oldroot
    // until the space's is complete.
    mount(Path::new("tmpfs"), Path::new("/tmp"), "tmpfs", libc::MS_NOSUID | libc::MS_NODEV, "mode=755")?;
    mkdir(Path::new("/tmp/newroot"), 0o755)?;
    mkdir(Path::new("/tmp/oldroot"), 0o755)?;
    mount(Path::new("tmpfs"), Path::new("/tmp/newroot"), "tmpfs", libc::MS_NOSUID | libc::MS_NODEV, "mode=755,size=16m")?;
    check(unsafe { libc::chdir(c"/tmp".as_ptr()) }, "chdir")?;
    pivot(c"/tmp", c"/tmp/oldroot")?;
    check(unsafe { libc::chdir(c"/".as_ptr()) }, "chdir")?;
    let (old, new) = (Path::new("/oldroot"), Path::new("/newroot"));
    let of = |path: &Path| old.join(path.strip_prefix("/").unwrap_or(path));

    for name in SYSTEM {
        system_entry(&old.join(name), &new.join(name))?;
    }
    mkdir(&new.join("etc"), 0o755)?;
    for name in ETC {
        system_entry(&old.join("etc").join(name), &new.join("etc").join(name))?;
    }
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    for (name, text) in [
        ("passwd", format!("space:x:{uid}:{gid}::{SPACE_HOME}:/bin/sh\nnobody:x:65534:65534::/nonexistent:/bin/false\n")),
        ("group", format!("space:x:{gid}:\nnogroup:x:65534:\n")),
        ("hosts", "127.0.0.1 localhost space\n::1 localhost\n".to_string()),
        ("hostname", "space\n".to_string()),
        ("nsswitch.conf", "passwd: files\ngroup: files\nhosts: files dns\n".to_string()),
        ("resolv.conf", format!("nameserver {}\n", net::DNS_ADDR)),
    ] {
        write(&new.join("etc").join(name), &text)?;
    }

    for (name, source) in &spec.tools {
        bind(&of(source), &new.join("exp/bin").join(name), true)?;
    }
    bind(&of(&spec.host), &new.join(HOST_DIR.trim_start_matches('/')), true)?;
    bind(&of(&spec.data), &new.join(SPACE_HOME.trim_start_matches('/')), false)?;

    let tmpfs = |path: &str, options: &str| -> Step<()> {
        let target = new.join(path);
        mkdir(&target, 0o755)?;
        mount(Path::new("tmpfs"), &target, "tmpfs", libc::MS_NOSUID | libc::MS_NODEV, options)
    };
    tmpfs("tmp", "mode=1777,size=4g")?;
    tmpfs("var/tmp", "mode=1777,size=1g")?;
    tmpfs("run", "mode=755,size=16m")?;
    mkdir(&new.join("proc"), 0o755)?;
    mount(Path::new("proc"), &new.join("proc"), "proc", libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC, "")?;
    devices(&old.join("dev"), &new.join("dev"))?;

    // The machine's root goes; the space's becomes read-only but for its own
    // mounts.
    check(unsafe { libc::chdir(c"/newroot".as_ptr()) }, "chdir")?;
    pivot(c".", c".")?;
    check(unsafe { libc::umount2(c".".as_ptr(), libc::MNT_DETACH) }, "leaving the machine's root")?;
    check(unsafe { libc::chdir(c"/".as_ptr()) }, "chdir")?;
    restrict(root, true, false)
}

fn devices(old: &Path, new: &Path) -> Step<()> {
    mkdir(new, 0o755)?;
    mount(Path::new("tmpfs"), new, "tmpfs", libc::MS_NOSUID, "mode=755,size=1m")?;
    for name in ["null", "zero", "full", "random", "urandom", "tty"] {
        write(&new.join(name), "")?;
        // A device file: bound as it is (nodev would make it useless).
        mount(&old.join(name), &new.join(name), "", libc::MS_BIND, "")?;
    }
    mkdir(&new.join("pts"), 0o755)?;
    mount(Path::new("devpts"), &new.join("pts"), "devpts", libc::MS_NOSUID | libc::MS_NOEXEC, "newinstance,ptmxmode=0666,mode=620")?;
    mkdir(&new.join("shm"), 0o1777)?;
    mount(Path::new("tmpfs"), &new.join("shm"), "tmpfs", libc::MS_NOSUID | libc::MS_NODEV, "mode=1777,size=1g")?;
    for (link, target) in [
        ("ptmx", "pts/ptmx"),
        ("fd", "/proc/self/fd"),
        ("stdin", "/proc/self/fd/0"),
        ("stdout", "/proc/self/fd/1"),
        ("stderr", "/proc/self/fd/2"),
    ] {
        std::os::unix::fs::symlink(target, new.join(link)).map_err(|err| format!("/dev/{link}: {err}"))?;
    }
    Ok(())
}

// ── the space's processes ─────────────────────────────────────────────────

/// PID 1 of the space: builds the root, gives up every privilege, starts the
/// program, reaps, and ends it when the host says so.
fn first_process(spec: &Spec, parent: libc::pid_t, stop: OwnedFd) -> Step<i32> {
    unsafe {
        check(libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL), "the parent-death signal")?;
        // In the new PID namespace the parent reads 0; its death before the
        // line above is seen on the stop socket.
        let _ = parent;
        check(libc::setsid(), "setsid")?;
        check(libc::sethostname(c"space".as_ptr().cast(), 5), "the host name")?;
        private_root(spec)?;
        only_these_fds(stop.as_raw_fd())?;
        give_up_privileges()?;
        check(libc::chdir(c"/space".as_ptr()), "chdir")?;
        libc::umask(0o077);

        let argv: Vec<CString> = std::iter::once(PROGRAM.to_string()).chain(spec.args.iter().cloned()).map(|arg| c(&arg)).collect();
        let envp: Vec<CString> = spec.env.iter().map(|(key, value)| c(&format!("{key}={value}"))).collect();
        let pointers = |list: &[CString]| -> Vec<*const libc::c_char> {
            list.iter().map(|text| text.as_ptr()).chain(std::iter::once(std::ptr::null())).collect()
        };
        let (argv_p, envp_p) = (pointers(&argv), pointers(&envp));

        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGCHLD);
        check(libc::sigprocmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()), "signals")?;
        let program = libc::fork();
        check(program, "fork")?;
        if program == 0 {
            libc::sigprocmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
            libc::setpgid(0, 0);
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY);
            libc::dup2(null, 0);
            libc::execve(argv_p[0], argv_p.as_ptr(), envp_p.as_ptr());
            eprintln!("member-space: the program: {}", io::Error::last_os_error());
            libc::_exit(SETUP_FAILED);
        }
        let signals = libc::signalfd(-1, &set, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK);
        check(signals, "signals")?;
        let grace = Duration::from_secs(spec.grace_secs);
        let mut stopping: Option<Instant> = None;
        loop {
            let mut fds = [
                libc::pollfd { fd: signals, events: libc::POLLIN, revents: 0 },
                // Once seen, the closed socket is not asked again.
                libc::pollfd { fd: if stopping.is_some() { -1 } else { stop.as_raw_fd() }, events: libc::POLLIN, revents: 0 },
            ];
            libc::poll(fds.as_mut_ptr(), 2, if stopping.is_some() { 200 } else { -1 });
            let mut info: libc::signalfd_siginfo = std::mem::zeroed();
            libc::read(signals, (&mut info as *mut libc::signalfd_siginfo).cast(), std::mem::size_of_val(&info));
            loop {
                let mut status = 0;
                let got = libc::waitpid(-1, &mut status, libc::WNOHANG);
                if got <= 0 {
                    break;
                }
                if got == program {
                    // PID 1's exit kills whatever is left.
                    return Ok(if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { 128 + libc::WTERMSIG(status) });
                }
            }
            if fds[1].revents != 0 && stopping.is_none() {
                libc::kill(program, libc::SIGTERM);
                stopping = Some(Instant::now());
            }
            if stopping.is_some_and(|since| since.elapsed() >= grace) {
                return Ok(128 + libc::SIGKILL);
            }
        }
    }
}

/// Standard streams and `keep` only: nothing of the host is inherited.
fn only_these_fds(keep: RawFd) -> Step<()> {
    let close = |from: u32, to: u32| -> Step<()> {
        if from > to {
            return Ok(());
        }
        check(unsafe { libc::syscall(libc::SYS_close_range, from, to, 0) } as libc::c_int, "descriptors")
    };
    close(3, (keep - 1) as u32)?;
    close((keep + 1) as u32, u32::MAX)
}

fn give_up_privileges() -> Step<()> {
    unsafe {
        let none = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        check(libc::setrlimit(libc::RLIMIT_CORE, &none), "limits")?;
        for capability in 0..64 {
            // Unknown ones answer EINVAL.
            libc::prctl(libc::PR_CAPBSET_DROP, capability, 0, 0, 0);
        }
        libc::prctl(libc::PR_CAP_AMBIENT, libc::PR_CAP_AMBIENT_CLEAR_ALL, 0, 0, 0);
        check(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), "no_new_privs")?;
        #[repr(C)]
        struct Header {
            version: u32,
            pid: i32,
        }
        let mut header = Header { version: 0x2008_0522, pid: 0 };
        let empty = [0u32; 6];
        check(libc::syscall(libc::SYS_capset, &mut header, empty.as_ptr()) as libc::c_int, "capabilities")?;
        let program = filter();
        let prog = libc::sock_fprog { len: program.len() as u16, filter: program.as_ptr().cast_mut() };
        check(libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &prog, 0, 0), "seccomp")
    }
}

/// Denied: new namespaces and mounts, keyrings, BPF, io_uring, userfaultfd,
/// perf, kernel modules, and every socket family but unix, IP and netlink
/// (vsock reaches the host whatever the network namespace).
fn filter() -> Vec<libc::sock_filter> {
    const ALLOW: u32 = 0x7fff_0000;
    const KILL: u32 = 0x8000_0000;
    let errno = |code: libc::c_int| 0x0005_0000 | code as u32;
    const LOAD: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
    const JEQ: u16 = 0x15;
    const JGE: u16 = 0x35;
    const JSET: u16 = 0x45;
    const RET: u16 = 0x06;
    let op = |code: u16, jt: u8, jf: u8, k: u32| libc::sock_filter { code, jt, jf, k };
    #[cfg(target_arch = "x86_64")]
    const ARCH: u32 = 0xc000_003e;
    #[cfg(target_arch = "aarch64")]
    const ARCH: u32 = 0xc000_00b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    const ARCH: u32 = 0;
    let mut out = vec![op(LOAD, 0, 0, 4), op(JEQ, 1, 0, ARCH), op(RET, 0, 0, KILL), op(LOAD, 0, 0, 0)];
    // x32 calls carry other numbers.
    if cfg!(target_arch = "x86_64") {
        out.push(op(JGE, 0, 1, 0x4000_0000));
        out.push(op(RET, 0, 0, errno(libc::ENOSYS)));
    }
    let denied: &[libc::c_long] = &[
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_chroot,
        libc::SYS_open_tree,
        libc::SYS_move_mount,
        libc::SYS_fsopen,
        libc::SYS_fsconfig,
        libc::SYS_fsmount,
        libc::SYS_fspick,
        libc::SYS_mount_setattr,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_userfaultfd,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
        libc::SYS_open_by_handle_at,
        libc::SYS_name_to_handle_at,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
        libc::SYS_kexec_load,
        libc::SYS_reboot,
        libc::SYS_swapon,
        libc::SYS_swapoff,
        libc::SYS_acct,
        libc::SYS_quotactl,
        libc::SYS_syslog,
        libc::SYS_settimeofday,
        libc::SYS_clock_settime,
        libc::SYS_nfsservctl,
        libc::SYS_vhangup,
        libc::SYS_lookup_dcookie,
        libc::SYS_mbind,
        libc::SYS_migrate_pages,
        libc::SYS_move_pages,
    ];
    for call in denied {
        out.push(op(JEQ, 0, 1, *call as u32));
        out.push(op(RET, 0, 0, errno(libc::EPERM)));
    }
    // glibc falls back to clone, whose flags can be read.
    out.push(op(JEQ, 0, 1, libc::SYS_clone3 as u32));
    out.push(op(RET, 0, 0, errno(libc::ENOSYS)));
    let namespaces = (libc::CLONE_NEWUSER
        | libc::CLONE_NEWNS
        | libc::CLONE_NEWPID
        | libc::CLONE_NEWNET
        | libc::CLONE_NEWIPC
        | libc::CLONE_NEWUTS
        | libc::CLONE_NEWCGROUP) as u32;
    // clone(flags, …): args[0] at offset 16 (little endian).
    out.push(op(JEQ, 0, 4, libc::SYS_clone as u32));
    out.push(op(LOAD, 0, 0, 16));
    out.push(op(JSET, 0, 1, namespaces));
    out.push(op(RET, 0, 0, errno(libc::EPERM)));
    out.push(op(RET, 0, 0, ALLOW));
    // ioctl(fd, TIOCSTI | TIOCLINUX): no keystrokes into a terminal.
    out.push(op(JEQ, 0, 5, libc::SYS_ioctl as u32));
    out.push(op(LOAD, 0, 0, 24));
    out.push(op(JEQ, 1, 0, libc::TIOCSTI as u32));
    out.push(op(JEQ, 0, 1, libc::TIOCLINUX as u32));
    out.push(op(RET, 0, 0, errno(libc::EPERM)));
    out.push(op(RET, 0, 0, ALLOW));
    // socket(family, …).
    out.push(op(JEQ, 0, 6, libc::SYS_socket as u32));
    out.push(op(LOAD, 0, 0, 16));
    out.push(op(JEQ, 4, 0, libc::AF_UNIX as u32));
    out.push(op(JEQ, 3, 0, libc::AF_INET as u32));
    out.push(op(JEQ, 2, 0, libc::AF_INET6 as u32));
    out.push(op(JEQ, 1, 0, libc::AF_NETLINK as u32));
    out.push(op(RET, 0, 0, errno(libc::EAFNOSUPPORT)));
    out.push(op(RET, 0, 0, ALLOW));
    out
}

// ── the probe ─────────────────────────────────────────────────────────────

pub fn probe(exe: &Path) -> Result<(), String> {
    let exe = exe.canonicalize().map_err(|err| format!("{}: {err}", exe.display()))?;
    let dir = std::env::temp_dir().join(format!("exp-space-probe-{}-{}", std::process::id(), nonce()));
    let result = (|| {
        for sub in ["", "data", "host"] {
            mkdir(&dir.join(sub), 0o700)?;
        }
        write(&dir.join("host/secret"), "host")?;
        let dir = dir.canonicalize().map_err(|err| err.to_string())?;
        // What a space must not reach: a service on this machine's loopback.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|err| err.to_string())?;
        let port = listener.local_addr().map_err(|err| err.to_string())?.port();
        let spec = Spec {
            data: dir.join("data"),
            host: dir.join("host"),
            tools: vec![("exponential".into(), exe.clone())],
            args: vec![PROBE_ARG.into(), port.to_string(), std::process::id().to_string()],
            env: crate::environment(std::iter::empty()),
            instance: None,
            grace_secs: 1,
        };
        let mut child = crate::command(&exe).spawn().map_err(|err| format!("the launcher: {err}"))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin.write_all(crate::spec_line(&spec).as_bytes()).map_err(|err| err.to_string())?;
        let (mut out, mut err) = (String::new(), String::new());
        let deadline = Instant::now() + Duration::from_secs(20);
        let done = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("The test space did not answer".to_string());
                }
            }
        };
        drop(stdin);
        // Bounded: both lines are short.
        child.stdout.take().map(|pipe| pipe.take(4096).read_to_string(&mut out));
        child.stderr.take().map(|pipe| pipe.take(4096).read_to_string(&mut err));
        if done && out.trim() == "ok" {
            return Ok(());
        }
        let line = err.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("The test space failed");
        Err(explain(line.trim_start_matches("member-space: ")))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn nonce() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos())
}

/// What a failed step most likely means on this machine.
fn explain(line: &str) -> String {
    let says = |path: &str, value: &str| std::fs::read_to_string(path).is_ok_and(|text| text.trim() == value);
    if says("/proc/sys/kernel/apparmor_restrict_unprivileged_userns", "1") {
        return "AppArmor blocks user namespaces (kernel.apparmor_restrict_unprivileged_userns)".into();
    }
    if says("/proc/sys/kernel/unprivileged_userns_clone", "0") || says("/proc/sys/user/max_user_namespaces", "0") {
        return "User namespaces are off".into();
    }
    if line.starts_with("namespaces:") {
        return "User namespaces are not allowed here".into();
    }
    line.chars().take(160).collect()
}

/// Inside the test space: everything the space must be, checked from where
/// a member's code would stand.
pub fn probe_main() -> ! {
    match probe_inside() {
        Ok(()) => {
            println!("ok");
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("member-space: {err}");
            std::process::exit(1);
        }
    }
}

fn probe_inside() -> Step<()> {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let port: u16 = args.first().and_then(|text| text.parse().ok()).ok_or("no port")?;
    for gone in ["/home", "/root", "/run/user", "/var/run", "/opt", "/srv", "/mnt", "/media", "/sys", "/boot", "/oldroot", "/etc/shadow"] {
        if Path::new(gone).exists() {
            return Err(format!("{gone} is visible"));
        }
    }
    let status = std::fs::read_to_string("/proc/self/status").map_err(|err| format!("/proc: {err}"))?;
    for want in ["CapEff:\t0000000000000000", "CapPrm:\t0000000000000000", "CapBnd:\t0000000000000000", "NoNewPrivs:\t1", "Seccomp:\t2"] {
        if !status.lines().any(|line| line == want) {
            return Err(format!("{want} does not hold"));
        }
    }
    let processes = std::fs::read_dir("/proc")
        .map_err(|err| format!("/proc: {err}"))?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .count();
    if processes > 2 {
        return Err("other processes are visible".into());
    }
    write(&Path::new(SPACE_HOME).join("probe"), "x")?;
    write(Path::new("/tmp/probe"), "x")?;
    for read_only in ["/usr/probe", "/exp/host/secret", "/exp/bin/exponential", "/etc/probe", "/probe"] {
        if std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(read_only).is_ok() {
            return Err(format!("{read_only} is writable"));
        }
    }
    if unsafe { libc::unshare(libc::CLONE_NEWUSER) } == 0 {
        return Err("a new namespace is allowed".into());
    }
    if unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) } >= 0 {
        return Err("a vsock is allowed".into());
    }
    let local = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|err| format!("the space's loopback: {err}"))?;
    let own = local.local_addr().map_err(|err| err.to_string())?;
    std::net::TcpStream::connect_timeout(&own, Duration::from_secs(2)).map_err(|err| format!("the space's loopback: {err}"))?;
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if own.port() != port && std::net::TcpStream::connect_timeout(&host, Duration::from_secs(2)).is_ok() {
        return Err("the machine's loopback is reachable".into());
    }
    let routes = std::fs::read_to_string("/proc/net/route").map_err(|err| format!("the network: {err}"))?;
    if !routes.lines().any(|line| line.starts_with(TUN)) {
        return Err("the network has no route".into());
    }
    Ok(())
}

