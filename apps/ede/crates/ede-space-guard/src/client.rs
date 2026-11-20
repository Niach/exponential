//! Asking the guard: what `exponential-compositor space-enter` does
//! before it becomes a space's program.

use std::{
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::Path,
    time::{Duration, Instant},
};

use crate::protocol::{parse_reply, Request, MAX_LINE};

/// How long one question may take in all: the connect, the first start of
/// the service, its answer, and the retries after `busy`.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Asks the guard at `socket`; `Err` says why it said no or could not be
/// asked. A `busy` guard is asked again, with a growing pause, until the
/// whole question has taken [`TIMEOUT`].
pub fn ask(socket: &Path, request: &Request) -> Result<(), String> {
    let deadline = Instant::now() + TIMEOUT;
    let mut pause = Duration::from_millis(50);
    loop {
        match ask_once(socket, request, deadline) {
            Err(reason) if reason == "busy" && Instant::now() + pause < deadline => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(800));
            }
            other => return other.map_err(|reason| format!("the space guard: {reason}")),
        }
    }
}

fn left(deadline: Instant) -> Result<Duration, String> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err("no answer in time".into())
    } else {
        Ok(left)
    }
}

fn ask_once(socket: &Path, request: &Request, deadline: Instant) -> Result<(), String> {
    let stream = connect(socket, deadline)?;
    stream.set_write_timeout(Some(left(deadline)?)).map_err(|e| e.to_string())?;
    (&stream).write_all(request.line().as_bytes()).map_err(|e| e.to_string())?;
    let mut line = Vec::new();
    let mut buf = [0u8; 64];
    while !line.contains(&b'\n') {
        stream.set_read_timeout(Some(left(deadline)?)).map_err(|e| e.to_string())?;
        match (&stream).read(&mut buf) {
            Ok(0) => break,
            Ok(n) => line.extend_from_slice(&buf[..n]),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Err("no answer in time".into()),
            Err(e) => return Err(e.to_string()),
        }
        if line.len() > MAX_LINE {
            return Err("an answer too long".into());
        }
    }
    parse_reply(&String::from_utf8_lossy(&line))
}

/// Connects to `socket` before `deadline`, also when the guard's backlog
/// is full (a non-blocking connect answers EAGAIN then; it is retried).
fn connect(socket: &Path, deadline: Instant) -> Result<UnixStream, String> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = socket.as_os_str().as_bytes();
    // SAFETY: a zeroed sockaddr_un filled within its size.
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= addr.sun_path.len() {
        return Err(format!("{}: path too long", socket.display()));
    }
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dst, src) in addr.sun_path.iter_mut().zip(bytes) {
        *dst = *src as libc::c_char;
    }
    let len = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
    loop {
        // SAFETY: a new socket of ours.
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK, 0) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: the descriptor just made, owned from here on.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        // SAFETY: a sockaddr_un of `len` bytes.
        let r = unsafe { libc::connect(owned.as_raw_fd(), (&addr as *const libc::sockaddr_un).cast(), len) };
        let error = std::io::Error::last_os_error();
        if r == 0 {
            let stream = UnixStream::from(owned);
            stream.set_nonblocking(false).map_err(|e| e.to_string())?;
            return Ok(stream);
        }
        if error.raw_os_error() != Some(libc::EAGAIN) {
            return Err(format!("{}: {error}", socket.display()));
        }
        left(deadline)?;
        std::thread::sleep(Duration::from_millis(20));
    }
}
