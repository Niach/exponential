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
    exchange(&connect(socket, deadline)?, request, deadline)
}

/// Writes `request` and reads the guard's answer. A guard with no room
/// for this user answers `busy` and closes without reading: the write can
/// fail then (EPIPE) while the answer waits to be read, so it is read all
/// the same; with no answer the write's error is the reason.
fn exchange(stream: &UnixStream, request: &Request, deadline: Instant) -> Result<(), String> {
    stream.set_write_timeout(Some(left(deadline)?)).map_err(|e| e.to_string())?;
    let written = (&*stream).write_all(request.line().as_bytes());
    match (read_answer(stream, deadline), written) {
        (Ok(line), _) if !line.is_empty() => parse_reply(&String::from_utf8_lossy(&line)),
        (_, Err(e)) => Err(e.to_string()),
        (Ok(line), Ok(())) => parse_reply(&String::from_utf8_lossy(&line)),
        (Err(reason), Ok(())) => Err(reason),
    }
}

/// The answer's line (empty when the guard closed without one).
fn read_answer(stream: &UnixStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    let mut buf = [0u8; 64];
    while !line.contains(&b'\n') {
        stream.set_read_timeout(Some(left(deadline)?)).map_err(|e| e.to_string())?;
        match (&*stream).read(&mut buf) {
            Ok(0) => break,
            Ok(n) => line.extend_from_slice(&buf[..n]),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Err("no answer in time".into()),
            Err(e) => return Err(e.to_string()),
        }
        if line.len() > MAX_LINE {
            return Err("an answer too long".into());
        }
    }
    Ok(line)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::reply;

    /// A guard with no room for the caller answers `busy` and closes
    /// without reading (review B1): the request's write can come after the
    /// close and fail (EPIPE) while the answer waits to be read. It is read
    /// all the same, so the question is asked again.
    #[test]
    fn a_busy_guard_that_closed_before_the_request_is_answered_busy() {
        let (ours, guard) = UnixStream::pair().unwrap();
        (&guard).write_all(reply(&Err("busy".into())).as_bytes()).unwrap();
        drop(guard);
        assert_eq!(exchange(&ours, &Request::Probe, Instant::now() + Duration::from_secs(1)), Err("busy".to_string()));
    }

    /// A guard that is gone without an answer is no answer.
    #[test]
    fn a_guard_gone_without_an_answer_says_so() {
        let (ours, guard) = UnixStream::pair().unwrap();
        drop(guard);
        let answer = exchange(&ours, &Request::Probe, Instant::now() + Duration::from_secs(1));
        assert!(answer.as_ref().is_err_and(|reason| reason != "busy"), "{answer:?}");
    }
}
