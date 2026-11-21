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

use crate::protocol::{parse_reply, Request, MAX_LINE, NOT_A_REQUEST};

/// How long one question may take in all: the connect, the first start of
/// the service, its answer, and the retries after `busy`.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Asks the guard at `socket`; `Err` says why it said no or could not be
/// asked. A `busy` guard is asked again, with a growing pause, until the
/// whole question has taken [`TIMEOUT`].
pub fn ask(socket: &Path, request: &Request) -> Result<(), String> {
    ask_by(socket, request, Instant::now() + TIMEOUT).map_err(|reason| format!("the space guard: {reason}"))
}

/// What a guard that answers its probe refuses besides what every guard
/// does (#EDE-145).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probed {
    /// The person's Wayland socket too: a guard of this build.
    Wayland,
    /// Not the person's Wayland socket: a guard from before, which answers
    /// `probe wayland` with [`NOT_A_REQUEST`].
    Older,
}

/// The session's probe of the guard at `socket`: does it do its work on
/// this machine, and does it refuse the person's Wayland socket too? It
/// asks `probe wayland`, and `probe` of a guard from before; both within
/// [`TIMEOUT`].
pub fn probe(socket: &Path) -> Result<Probed, String> {
    let deadline = Instant::now() + TIMEOUT;
    let probed = match ask_by(socket, &Request::ProbeWayland, deadline) {
        Ok(()) => Ok(Probed::Wayland),
        Err(reason) if reason == NOT_A_REQUEST => ask_by(socket, &Request::Probe, deadline).map(|()| Probed::Older),
        Err(reason) => Err(reason),
    };
    probed.map_err(|reason| format!("the space guard: {reason}"))
}

fn ask_by(socket: &Path, request: &Request, deadline: Instant) -> Result<(), String> {
    let mut pause = Duration::from_millis(50);
    loop {
        match ask_once(socket, request, deadline) {
            Err(reason) if reason == "busy" && Instant::now() + pause < deadline => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(800));
            }
            other => return other,
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

    /// A guard on a socket of the test's own: `answer` gives the reply
    /// line for each request line; the lines asked come back, each before
    /// its answer goes out.
    fn fake_guard(name: &str, answer: fn(&str) -> String) -> (std::path::PathBuf, std::sync::mpsc::Receiver<String>) {
        use std::io::BufRead;
        let socket = std::env::temp_dir().join(format!("ede-guard-client-{}-{name}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let mut line = String::new();
                let _ = std::io::BufReader::new(&stream).read_line(&mut line);
                let reply = answer(&line);
                if tx.send(line).is_err() {
                    return;
                }
                let _ = (&stream).write_all(reply.as_bytes());
            }
        });
        (socket, rx)
    }

    /// #EDE-145: the session tells a guard that refuses the person's
    /// Wayland socket from one from before, which knows only `probe`; a
    /// refusal of either is the probe's answer.
    #[test]
    fn the_probe_tells_a_guard_that_refuses_the_wayland_socket_from_one_from_before() {
        // This build's guard: the requests as it parses them.
        let (current, asked) = fake_guard("current", |line| reply(&Request::parse(line).map(drop)));
        assert_eq!(probe(&current), Ok(Probed::Wayland));
        assert_eq!(asked.try_iter().collect::<Vec<_>>(), ["probe wayland\n"]);
        // A guard from before: `probe` and `enter` only.
        let (older, asked) = fake_guard("older", |line| reply(&if line == "probe\n" { Ok(()) } else { Err(NOT_A_REQUEST.into()) }));
        assert_eq!(probe(&older), Ok(Probed::Older));
        assert_eq!(asked.try_iter().collect::<Vec<_>>(), ["probe wayland\n", "probe\n"]);
        // Either one refusing: the probe failed, asked once of this build's.
        let (refusing, asked) = fake_guard("refusing", |_| reply(&Err("this kernel refuses the device program".into())));
        assert_eq!(probe(&refusing), Err("the space guard: this kernel refuses the device program".into()));
        assert_eq!(asked.try_iter().count(), 1);
        let (older_refusing, _asked) = fake_guard("older-refusing", |line| reply(&Err(if line == "probe\n" { "this kernel has no unix connect hook" } else { NOT_A_REQUEST }.into())));
        assert_eq!(probe(&older_refusing), Err("the space guard: this kernel has no unix connect hook".into()));
        for socket in [current, older, refusing, older_refusing] {
            let _ = std::fs::remove_file(socket);
        }
    }
}
