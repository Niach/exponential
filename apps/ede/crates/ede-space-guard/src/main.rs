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
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd},
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use ede_space_guard::{client, guard::Guard, protocol, sys, Request};

/// The service ends after this long without a request (the socket stays).
const IDLE: Duration = Duration::from_secs(600);
/// A client gets this long to send its line.
const READ: Duration = Duration::from_secs(2);
/// Requests read at once; more are answered "busy".
const MAX_READING: usize = 16;

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
            let listener = UnixListener::bind(&path).unwrap_or_else(|e| fail(&format!("{}: {e}", path.display())));
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666));
            serve(listener)
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
        Some(other) => fail(&format!("unknown command {other}")),
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

fn serve(listener: UnixListener) -> ! {
    let guard = Arc::new(Mutex::new(Guard::default()));
    let reading = Arc::new(AtomicUsize::new(0));
    loop {
        let mut poll = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd of a descriptor we hold.
        let ready = unsafe { libc::poll(&mut poll, 1, IDLE.as_millis() as i32) };
        if ready == 0 && reading.load(Ordering::SeqCst) == 0 {
            std::process::exit(0);
        }
        let Ok((stream, _)) = listener.accept() else { continue };
        if reading.fetch_add(1, Ordering::SeqCst) >= MAX_READING {
            reading.fetch_sub(1, Ordering::SeqCst);
            let _ = (&stream).write_all(protocol::reply(&Err("busy".into())).as_bytes());
            continue;
        }
        let (guard, reading) = (guard.clone(), reading.clone());
        std::thread::spawn(move || {
            answer(&stream, &guard);
            reading.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

fn answer(stream: &UnixStream, guard: &Mutex<Guard>) {
    let _ = stream.set_read_timeout(Some(READ));
    let _ = stream.set_write_timeout(Some(READ));
    let mut line = String::new();
    let read = BufReader::new(Read::take(stream, protocol::MAX_LINE as u64)).read_line(&mut line);
    let result = match (read, sys::peer(stream.as_fd())) {
        (Ok(_), Ok((pid, uid))) => Request::parse(&line).and_then(|request| guard.lock().unwrap_or_else(|p| p.into_inner()).handle(&request, pid, uid)),
        (Err(e), _) | (_, Err(e)) => Err(e.to_string()),
    };
    if let Err(reason) = &result {
        eprintln!("refused: {reason}");
    }
    let _ = (&*stream).write_all(protocol::reply(&result).as_bytes());
}
