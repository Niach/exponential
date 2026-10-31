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

/// Holds suspend back until we let go (a `systemd-inhibit` child).
fn take_inhibitor() -> Option<Child> {
    Command::new("systemd-inhibit")
        .args(["--what=sleep", "--mode=delay", "--who=eDE", "--why=Locking the screen", "sleep", "infinity"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| tracing::warn!("systemd-inhibit: {e}"))
        .ok()
}

/// Starts listening, when a lock command is configured and `[lock]
/// on_suspend` is on.
pub fn start(state: &mut Ede) {
    if state.config.lock.command.is_empty() || !state.config.lock.on_suspend {
        return;
    }
    let our_session = std::env::var("XDG_SESSION_ID").map(|id| session_path(&id)).unwrap_or_default();
    let mut monitor = match Command::new("gdbus")
        .args(["monitor", "--system", "--dest", "org.freedesktop.login1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!("gdbus monitor: {e}; no lock on suspend or loginctl lock-session");
            return;
        }
    };
    let stdout = monitor.stdout.take().unwrap();
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
            let _ = monitor.kill();
        })
        .expect("spawn the logind reader");

    let mut inhibitor = take_inhibitor();
    state
        .loop_handle
        .insert_source(rx, move |event, _, state| match event {
            ChannelEvent::Msg(Signal::Lock) => state.request_lock(),
            ChannelEvent::Msg(Signal::PrepareForSleep(true)) => {
                state.request_lock();
                let Some(mut held) = inhibitor.take() else { return };
                let since = Instant::now();
                let _ = state.loop_handle.insert_source(Timer::from_duration(Duration::from_millis(50)), move |_, _, state| {
                    if state.lock_confirmed() || since.elapsed() > Duration::from_secs(2) {
                        let _ = held.kill();
                        let _ = held.wait();
                        return TimeoutAction::Drop;
                    }
                    TimeoutAction::ToDuration(Duration::from_millis(50))
                });
            }
            ChannelEvent::Msg(Signal::PrepareForSleep(false)) => {
                if inhibitor.is_none() {
                    inhibitor = take_inhibitor();
                }
            }
            ChannelEvent::Closed => {}
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
}
