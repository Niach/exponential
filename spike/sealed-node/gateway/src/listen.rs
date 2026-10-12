//! Listeners. A vsock connection is wrapped as a `TcpStream` through its raw
//! fd: std only needs read/write/dup on it, which every stream socket has.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::os::fd::FromRawFd;

pub type Conn = TcpStream;

pub enum Listener {
    Tcp(TcpListener),
    #[allow(dead_code)]
    Vsock(i32),
}

impl Listener {
    pub fn bind(spec: &str) -> io::Result<Self> {
        if let Some(addr) = spec.strip_prefix("tcp:") {
            return Ok(Self::Tcp(TcpListener::bind(addr)?));
        }
        if let Some(port) = spec.strip_prefix("vsock:") {
            let port: u32 = port.parse().map_err(|_| io::Error::other("bad vsock port"))?;
            return vsock_listen(port).map(Self::Vsock);
        }
        Err(io::Error::other("listen spec must be tcp:<addr> or vsock:<port>"))
    }

    pub fn accept(&self) -> io::Result<Conn> {
        match self {
            Self::Tcp(l) => l.accept().map(|(s, _)| s),
            Self::Vsock(fd) => {
                let c = vsock_accept(*fd)?;
                // SAFETY: c is a fresh connected stream socket we own.
                Ok(unsafe { TcpStream::from_raw_fd(c) })
            }
        }
    }
}

/// Serve `n` vsock ports `base..base+n`, one connection per port at a time.
///
/// Hermit's vsock has no accept queue: `accept` turns the bound port's only
/// socket into the connection (the returned fd aliases the listener), so a
/// port carries exactly one connection per bind. Concurrency therefore comes
/// from a pool of ports, each re-bound after its connection ends; the host
/// relay only dials ports it knows are free.
pub fn vsock_pool<F>(base: u32, n: u32, handler: F) -> !
where
    F: Fn(Conn) + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    for port in base..base + n {
        let handler = handler.clone();
        std::thread::spawn(move || loop {
            let l = match vsock_listen(port) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[gateway] vsock pool bind {port}: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    continue;
                }
            };
            match vsock_accept(l) {
                // SAFETY: c is a connected stream socket we own.
                Ok(c) => handler(unsafe { TcpStream::from_raw_fd(c) }),
                Err(e) => eprintln!("[gateway] vsock pool accept {port}: {e}"),
            }
            vsock_close(l);
        });
    }
    loop {
        std::thread::park();
    }
}

/// Token provisioning: listen once on `port`, accept the host's single
/// connection, read one newline-terminated line, close the listener.
///
/// The gateway is the vsock SERVER here on purpose: a host-side vsock
/// listener would be reachable from the agent VM too (every guest can dial
/// CID 2), while the gateway's CID is unreachable from the agent VM.
pub fn vsock_receive_line(port: u32) -> io::Result<String> {
    use std::io::Read;
    let l = vsock_listen(port)?;
    let fd = vsock_accept(l)?;
    // SAFETY: fresh connected stream socket we own.
    let mut s = unsafe { TcpStream::from_raw_fd(fd) };
    let mut out = Vec::new();
    let mut b = [0u8; 512];
    while !out.contains(&b'\n') {
        let n = s.read(&mut b)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&b[..n]);
    }
    drop(s);
    vsock_close(l);
    String::from_utf8(out).map_err(|_| io::Error::other("token is not utf-8"))
}

#[cfg(target_os = "linux")]
fn vsock_close(fd: i32) {
    unsafe { libc::close(fd) };
}

#[cfg(target_os = "hermit")]
fn vsock_close(fd: i32) {
    unsafe { hermit_abi::close(fd) };
}

#[cfg(target_os = "linux")]
fn vsock_listen(port: u32) -> io::Result<i32> {
    unsafe {
        let fd = libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut addr: libc::sockaddr_vm = std::mem::zeroed();
        addr.svm_family = libc::AF_VSOCK as _;
        addr.svm_port = port;
        addr.svm_cid = libc::VMADDR_CID_ANY;
        if libc::bind(fd, &addr as *const _ as *const libc::sockaddr, std::mem::size_of_val(&addr) as _) < 0
            || libc::listen(fd, 64) < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(fd)
    }
}

#[cfg(target_os = "linux")]
fn vsock_accept(fd: i32) -> io::Result<i32> {
    let c = unsafe { libc::accept4(fd, std::ptr::null_mut(), std::ptr::null_mut(), libc::SOCK_CLOEXEC) };
    if c < 0 { Err(io::Error::last_os_error()) } else { Ok(c) }
}

#[cfg(target_os = "hermit")]
fn vsock_listen(port: u32) -> io::Result<i32> {
    use hermit_abi as h;
    unsafe {
        let fd = h::socket(h::AF_VSOCK, h::SOCK_STREAM, 0);
        if fd < 0 {
            return Err(io::Error::other(format!("vsock socket: {fd}")));
        }
        let mut addr: h::sockaddr_vm = std::mem::zeroed();
        addr.svm_len = std::mem::size_of::<h::sockaddr_vm>() as _;
        addr.svm_family = h::AF_VSOCK as _;
        addr.svm_port = port;
        addr.svm_cid = u32::MAX; // VMADDR_CID_ANY
        let r = h::bind(fd, &addr as *const _ as *const h::sockaddr, std::mem::size_of_val(&addr) as _);
        if r < 0 {
            return Err(io::Error::other(format!("vsock bind: {r}")));
        }
        let r = h::listen(fd, 64);
        if r < 0 {
            return Err(io::Error::other(format!("vsock listen: {r}")));
        }
        Ok(fd)
    }
}

#[cfg(target_os = "hermit")]
fn vsock_accept(fd: i32) -> io::Result<i32> {
    let c = unsafe { hermit_abi::accept(fd, std::ptr::null_mut(), std::ptr::null_mut()) };
    if c < 0 { Err(io::Error::other(format!("vsock accept: {c}"))) } else { Ok(c) }
}
