//! `exponential-space-guard`: the service (`exponential-space-guard.socket`
//! hands it its listening socket; it exits after [`IDLE`] without a
//! request), and the two questions it answers, for tests and checks:
//!
//! ```text
//! exponential-space-guard                                   the service (socket activation)
//! exponential-space-guard --socket PATH                     the service on a socket of its own (tests)
//! exponential-space-guard probe [--socket PATH]             can it do its work here? (prints ok or why)
//! exponential-space-guard enter [--socket PATH] SPACES -- ARGV…   guard this process's space slice, then become ARGV
//! ```

use std::{
    collections::HashMap,
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd},
        unix::{
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ede_space_guard::{client, guard::Guard, protocol, slots, sys, Request};

/// The service ends after this long without a request (the socket stays).
const IDLE: Duration = Duration::from_secs(600);
/// From accept to the request's newline, all reads together.
const REQUEST_DEADLINE: Duration = Duration::from_secs(1);
/// Refusal lines one user may put in the journal per minute.
const LOG_PER_MINUTE: u32 = 10;

fn fail(message: &str) -> ! {
    eprintln!("exponential-space-guard: {message}");
    std::process::exit(127)
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let socket_arg = |args: &mut Vec<String>| -> PathBuf {
        match args.iter().position(|a| a == "--socket") {
            Some(at) if at + 1 < args.len() && args[..at].iter().all(|a| a != "--") => {
                let path = args.remove(at + 1);
                args.remove(at);
                path.into()
            }
            _ => protocol::SOCKET.into(),
        }
    };
    match args.first().map(String::as_str) {
        None => serve(from_systemd().unwrap_or_else(|e| fail(&e))),
        Some("--socket") => {
            let path = socket_arg(&mut args);
            let _ = std::fs::remove_file(&path);
            // Anyone may connect: the mode comes from the umask at bind, not
            // from a chmod by path afterwards.
            // SAFETY: umask only changes this process's file mode mask.
            let old = unsafe { libc::umask(0) };
            let listener = UnixListener::bind(&path);
            // SAFETY: as above.
            unsafe { libc::umask(old) };
            serve(listener.unwrap_or_else(|e| fail(&format!("{}: {e}", path.display()))))
        }
        Some("probe") => {
            args.remove(0);
            let socket = socket_arg(&mut args);
            match client::ask(&socket, &Request::Probe) {
                Ok(()) => println!("ok"),
                Err(reason) => fail(&reason),
            }
        }
        Some("enter") => {
            args.remove(0);
            let socket = socket_arg(&mut args);
            let (Some(spaces), Some("--"), Some(argv)) = (args.first(), args.get(1).map(String::as_str), args.get(2..).filter(|argv| !argv.is_empty())) else {
                fail("usage: enter [--socket PATH] SPACES -- program [arguments]")
            };
            if let Err(reason) = client::ask(&socket, &Request::Enter { spaces: spaces.clone() }) {
                fail(&reason)
            }
            fail(&format!("{}: {}", argv[0], Command::new(&argv[0]).args(&argv[1..]).exec()))
        }
        Some(other) => fail(&format!("unknown command {other:?}")),
    }
}

/// The listening socket systemd passed (`LISTEN_FDS=1`, descriptor 3).
fn from_systemd() -> Result<UnixListener, String> {
    let ours = std::env::var("LISTEN_PID").ok().and_then(|pid| pid.parse::<u32>().ok()) == Some(std::process::id());
    if !ours || std::env::var("LISTEN_FDS").as_deref() != Ok("1") {
        return Err("no socket from systemd (start exponential-space-guard.socket, or pass --socket PATH)".into());
    }
    // SAFETY: systemd's socket activation hands descriptor 3 to us alone.
    Ok(unsafe { UnixListener::from_raw_fd(3) })
}

/// Refusal lines per user and minute, so no user can flood the journal
/// (and hide the guard's other lines behind its rate limit).
#[derive(Default)]
struct LogBudget(Mutex<HashMap<u32, (Instant, u32)>>);

impl LogBudget {
    fn refused(&self, uid: u32, pid: u32, reason: &str) {
        let mut budget = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let entry = budget.entry(uid).or_insert((Instant::now(), 0));
        if entry.0.elapsed() > Duration::from_secs(60) {
            *entry = (Instant::now(), 0);
        }
        entry.1 += 1;
        if entry.1 <= LOG_PER_MINUTE {
            // Escaped: nothing a caller sends starts a line of its own.
            eprintln!("refused user {uid} process {pid}: {}", reason.escape_default());
        }
    }
}

fn serve(listener: UnixListener) -> ! {
    let guard = Arc::new(Mutex::new(Guard::default()));
    let slots = slots::Slots::default();
    let log = Arc::new(LogBudget::default());
    loop {
        let mut poll = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd of a descriptor we hold.
        let ready = unsafe { libc::poll(&mut poll, 1, IDLE.as_millis() as i32) };
        if ready <= 0 {
            if ready == 0 && slots.held() == 0 {
                std::process::exit(0);
            }
            continue;
        }
        let Ok((stream, _)) = listener.accept() else { continue };
        // Whose request it is decides whether there is room, before a
        // single byte is read.
        let Ok((pid, uid)) = sys::peer(stream.as_fd()) else { continue };
        let has_runtime = Path::new(&format!("/run/user/{uid}")).is_dir();
        let Some(slot) = slots.take(slots::bucket(uid, has_runtime)) else {
            let _ = (&stream).write_all(protocol::reply(&Err("busy".into())).as_bytes());
            continue;
        };
        let (guard, log) = (guard.clone(), log.clone());
        let spawned = std::thread::Builder::new().name("request".into()).spawn(move || {
            let _slot = slot;
            answer(&stream, &guard, &log, pid, uid);
        });
        if let Err(e) = spawned {
            eprintln!("no thread for a request: {e}");
        }
    }
}

/// The request's line, read within [`REQUEST_DEADLINE`] from now.
fn read_request(stream: &UnixStream) -> Result<String, String> {
    let deadline = Instant::now() + REQUEST_DEADLINE;
    let mut line = Vec::new();
    let mut byte = [0u8; 64];
    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("no request in time".into());
        }
        let mut poll = libc::pollfd { fd: stream.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd of a descriptor we hold.
        if unsafe { libc::poll(&mut poll, 1, left.as_millis().max(1) as i32) } <= 0 {
            continue;
        }
        match (&*stream).read(&mut byte) {
            Ok(0) => return Err("the request ended early".into()),
            Ok(n) => {
                line.extend_from_slice(&byte[..n]);
                if let Some(end) = line.iter().position(|b| *b == b'\n') {
                    line.truncate(end + 1);
                    return String::from_utf8(line).map_err(|_| "not text".into());
                }
                if line.len() > protocol::MAX_LINE {
                    return Err("the request is too long".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn answer(stream: &UnixStream, guard: &Mutex<Guard>, log: &LogBudget, pid: u32, uid: u32) {
    let result = read_request(stream).and_then(|line| Request::parse(&line)).and_then(|request| guard.lock().unwrap_or_else(|p| p.into_inner()).handle(&request, pid, uid));
    if let Err(reason) = &result {
        log.refused(uid, pid, reason);
    }
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_write_timeout(Some(REQUEST_DEADLINE));
    let _ = (&*stream).write_all(protocol::reply(&result).as_bytes());
}
