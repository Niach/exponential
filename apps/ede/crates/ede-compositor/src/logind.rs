//! logind (the real session only): lock on `loginctl lock-session` and
//! before the machine suspends.
//!
//! The signals come from `gdbus monitor` on the system bus (no D-Bus
//! library in the compositor). A delay inhibitor (`systemd-inhibit
//! --mode=delay`) holds suspend until the lock is confirmed on every output,
//! or 2 s, so the screen never wakes up showing the desktop.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use smithay::reexports::calloop::{
    channel::{self, Event as ChannelEvent},
    timer::{TimeoutAction, Timer},
};

use crate::state::Ede;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// `loginctl lock-session` (or the greeter) asks this session to lock.
    Lock,
    /// true = about to suspend, false = resumed.
    PrepareForSleep(bool),
}

/// logind's object path for session `id` (sd-bus label escaping: anything
/// but `[A-Za-z0-9]`, and a leading digit, becomes `_xx`).
pub fn session_path(id: &str) -> String {
    let mut path = String::from("/org/freedesktop/login1/session/");
    for (i, b) in id.bytes().enumerate() {
        if b.is_ascii_alphanumeric() && !(i == 0 && b.is_ascii_digit()) {
            path.push(b as char);
        } else {
            path.push_str(&format!("_{b:02x}"));
        }
    }
    path
}

/// One line of `gdbus monitor --system --dest org.freedesktop.login1`.
pub fn parse(line: &str, our_session: &str) -> Option<Signal> {
    let (path, rest) = line.split_once(": ")?;
    if path == "/org/freedesktop/login1" {
        let args = rest.strip_prefix("org.freedesktop.login1.Manager.PrepareForSleep ")?;
        return Some(Signal::PrepareForSleep(args.starts_with("(true")));
    }
    (path == our_session && rest.starts_with("org.freedesktop.login1.Session.Lock ")).then_some(Signal::Lock)
}

/// A child of ours (the inhibitor, the monitor) that never outlives us:
/// dropping it kills and reaps it.
pub struct Held(pub Child);

impl Held {
    pub fn spawn(command: &mut Command) -> std::io::Result<Self> {
        command.spawn().map(Self)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// What the inhibitor runs while it holds suspend back: it reads our end
/// of a pipe, so it ends when we let go, exit or die (`systemd-inhibit`
/// runs it as a child of its own, which a kill would not reach).
const HOLD: &[&str] = &["cat"];

/// Starts `command` the way the inhibitor runs.
fn hold(command: &mut Command) -> std::io::Result<Held> {
    Held::spawn(command.stdin(Stdio::piped()).stdout(Stdio::null()))
}

/// Holds suspend back until we let go (a `systemd-inhibit` child).
fn take_inhibitor() -> Option<Held> {
    hold(Command::new("systemd-inhibit").args(["--what=sleep", "--mode=delay", "--who=Exponential Desktop", "--why=Locking the screen"]).args(HOLD))
        .map_err(|e| tracing::warn!("systemd-inhibit: {e}"))
        .ok()
}

/// Whether the real session listens to logind.
pub fn listens(config: &crate::Config) -> bool {
    config.lock.on_suspend
}

/// What logind's signals do, with the inhibitor that holds suspend back.
pub struct Listener {
    inhibitor: Option<Held>,
    take: fn() -> Option<Held>,
}

impl Listener {
    /// `take` takes the inhibitor: now, and again after each resume.
    pub fn new(take: fn() -> Option<Held>) -> Self {
        Self { inhibitor: take(), take }
    }

    pub fn handle(&mut self, state: &mut Ede, signal: Signal) {
        match signal {
            Signal::Lock => state.request_lock(),
            Signal::PrepareForSleep(true) => {
                state.request_lock();
                let Some(held) = self.inhibitor.take() else { return };
                let since = Instant::now();
                let _ = state.loop_handle.insert_source(Timer::from_duration(Duration::from_millis(50)), move |_, _, state| {
                    if state.lock_confirmed() || since.elapsed() > Duration::from_secs(2) {
                        // Dropping the source drops `held`: suspend goes on.
                        let _ = &held;
                        return TimeoutAction::Drop;
                    }
                    TimeoutAction::ToDuration(Duration::from_millis(50))
                });
            }
            Signal::PrepareForSleep(false) => {
                if self.inhibitor.is_none() {
                    self.inhibitor = (self.take)();
                }
            }
        }
    }
}

/// Starts listening when `[lock] on_suspend` is on, whichever lock is
/// configured (`[lock] command` or the sealed lock).
pub fn start(state: &mut Ede) {
    if !listens(&state.config) {
        return;
    }
    let our_session = std::env::var("XDG_SESSION_ID").map(|id| session_path(&id)).unwrap_or_default();
    let mut monitor = match Held::spawn(Command::new("gdbus").args(["monitor", "--system", "--dest", "org.freedesktop.login1"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())) {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!("gdbus monitor: {e}; no lock on suspend or loginctl lock-session");
            return;
        }
    };
    let stdout = monitor.0.stdout.take().unwrap();
    let (tx, rx) = channel::channel::<Signal>();
    std::thread::Builder::new()
        .name("ede-logind".into())
        .spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(signal) = parse(&line, &our_session) {
                    if tx.send(signal).is_err() {
                        break;
                    }
                }
            }
        })
        .expect("spawn the logind reader");

    let mut listener = Listener::new(take_inhibitor);
    state
        .loop_handle
        .insert_source(rx, move |event, _, state| {
            // The monitor lives as long as this source: it ends with the loop.
            let _ = &monitor;
            match event {
                ChannelEvent::Msg(signal) => listener.handle(state, signal),
                ChannelEvent::Closed => {}
            }
        })
        .expect("logind channel");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_paths_escape_like_sd_bus() {
        assert_eq!(session_path("2"), "/org/freedesktop/login1/session/_32");
        assert_eq!(session_path("c2"), "/org/freedesktop/login1/session/c2");
        assert_eq!(session_path("a-b"), "/org/freedesktop/login1/session/a_2db");
    }

    #[test]
    fn reads_the_signals_gdbus_prints() {
        let ours = session_path("2");
        assert_eq!(parse("/org/freedesktop/login1: org.freedesktop.login1.Manager.PrepareForSleep (true,)", &ours), Some(Signal::PrepareForSleep(true)));
        assert_eq!(parse("/org/freedesktop/login1: org.freedesktop.login1.Manager.PrepareForSleep (false,)", &ours), Some(Signal::PrepareForSleep(false)));
        assert_eq!(parse("/org/freedesktop/login1/session/_32: org.freedesktop.login1.Session.Lock ()", &ours), Some(Signal::Lock));
        // Another session's lock is not ours.
        assert_eq!(parse("/org/freedesktop/login1/session/_33: org.freedesktop.login1.Session.Lock ()", &ours), None);
        assert_eq!(parse("The name org.freedesktop.login1 is owned by :1.5", &ours), None);
    }

    fn gone(pid: u32) -> bool {
        !std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    fn gone_within(pid: u32, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while !gone(pid) {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    /// #EDE-70: a compositor that goes away takes its children with it.
    #[test]
    fn a_held_child_ends_when_it_is_dropped() {
        let held = Held::spawn(Command::new("sleep").arg("30")).unwrap();
        let pid = held.0.id();
        drop(held);
        assert!(gone(pid), "the child outlived its owner");
    }

    /// #EDE-70: `systemd-inhibit` runs what holds the inhibitor as a child
    /// of its own (`sh` stands in for it here); that one ends too.
    #[test]
    fn what_the_inhibitor_runs_ends_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");
        let script = format!("sh -c 'echo $$ > {}; exec {}'; true", pidfile.display(), HOLD.join(" "));
        let held = hold(Command::new("sh").args(["-c", &script])).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid: u32 = loop {
            if let Some(pid) = std::fs::read_to_string(&pidfile).ok().and_then(|s| s.trim().parse().ok()) {
                break pid;
            }
            assert!(Instant::now() < deadline, "the stand-in never started");
            std::thread::sleep(Duration::from_millis(10));
        };
        drop(held);
        let ended = gone_within(pid, Duration::from_secs(2));
        if !ended {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        assert!(ended, "what the inhibitor ran outlived it");
    }
}
