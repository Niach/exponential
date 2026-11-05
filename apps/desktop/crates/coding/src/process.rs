//! Is a recorded pid still a live process on THIS machine? (EXP-766)
//!
//! The one answer three callers share: the session registry's live-instance
//! guard (`ui::session_registry`), the prune's cross-process busy check
//! ([`crate::prune`]) and anything else that reads a pid off disk. It lives
//! here because `coding` is the lowest crate all of them depend on.
//!
//! Liveness is deliberately CONSERVATIVE: a pid the OS still knows counts as
//! alive even when we may not signal it, and an unanswerable probe reads as
//! alive too, because every caller uses the answer to decide whether to keep
//! someone else's work.

/// `kill(pid, 0)` delivers no signal and only reports reachability: `Ok` =
/// alive, `EPERM` = alive but owned by another user, `ESRCH` = gone.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    // `kill` reads a SIGNED pid: 0 means "our whole process group" and any
    // negative value means "that process group", so only values that survive
    // the round-trip into a positive `pid_t` may be probed. A recorded pid
    // outside that range is corrupt — treat it as gone.
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: `kill` with signal 0 has no side effects beyond setting errno.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Windows has no `kill(pid, 0)`; `tasklist` is the answer without pulling in
/// a win32 crate. It prints one CSV-less row per match and
/// `INFO: No tasks are running …` for none, so a match is a row carrying the
/// pid as its own field. A tasklist that cannot run at all (locked-down
/// PATH) reports ALIVE — the callers all keep work on a maybe.
#[cfg(not(unix))]
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let mut command = terminal::process::background_command("tasklist");
    command.args(["/FI", &format!("PID eq {pid}"), "/NH"]);
    let Ok(output) = command.output() else {
        return true;
    };
    if !output.status.success() {
        return true;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let wanted = pid.to_string();
    stdout
        .lines()
        .filter(|line| !line.trim_start().starts_with("INFO:"))
        .any(|line| line.split_whitespace().any(|field| field == wanted))
}

/// FEED-189: the flag a relaunched Windows app carries: `--relaunch-after
/// <pid>` = wait for the app it replaces to go, then boot normally.
pub const RELAUNCH_AFTER_ARG: &str = "--relaunch-after";

/// FEED-189: how long a relaunched app waits for the one it replaces. The
/// quit hook's pushes and suspends take seconds; past this the old process is
/// hung, and the new one boots anyway.
const RELAUNCH_EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// FEED-189: start `exe` so it takes over once `pid` has exited. Our own
/// binary does the waiting ([`await_relaunch`]), so a relaunch depends on
/// nothing but the file the update just verified and put in place.
pub fn spawn_after_exit(exe: &std::path::Path, pid: u32) -> std::io::Result<()> {
    let mut command = terminal::process::background_command(exe);
    command
        .arg(RELAUNCH_AFTER_ARG)
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn().map(drop)
}

/// The pid `--relaunch-after <pid>` names, if this launch is a relaunch. Pure.
pub fn relaunch_pid(args: impl IntoIterator<Item = String>) -> Option<u32> {
    let mut args = args.into_iter();
    args.find(|arg| arg == RELAUNCH_AFTER_ARG)?;
    args.next()?.parse().ok()
}

/// Polls `alive` until it reads false (`true`) or `attempts` ran out (`false`).
fn wait_for_exit(
    alive: impl Fn() -> bool,
    attempts: u32,
    poll: std::time::Duration,
) -> bool {
    for _ in 0..attempts {
        if !alive() {
            return true;
        }
        std::thread::sleep(poll);
    }
    !alive()
}

/// FEED-189: a relaunch's first step, before the single-instance guard (which
/// would otherwise forward to the app being replaced and exit). `true` = this
/// is a relaunch and the old process is gone.
pub fn await_relaunch(args: impl IntoIterator<Item = String>) -> bool {
    let Some(pid) = relaunch_pid(args) else {
        return false;
    };
    let poll = std::time::Duration::from_millis(500);
    let attempts = (RELAUNCH_EXIT_WAIT.as_millis() / poll.as_millis()) as u32;
    let gone = wait_for_exit(|| is_alive(pid), attempts, poll);
    if gone {
        log::info!("relaunch: pid {pid} exited, starting");
    } else {
        log::warn!("relaunch: pid {pid} still runs after {RELAUNCH_EXIT_WAIT:?}, starting anyway");
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FEED-189: only a well-formed flag makes a launch a relaunch; a deep
    /// link or a plain launch passes through.
    #[test]
    fn relaunch_flag_names_the_pid_to_wait_for() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(relaunch_pid(args(&["exp", RELAUNCH_AFTER_ARG, "42"])), Some(42));
        assert_eq!(relaunch_pid(args(&["exp"])), None);
        assert_eq!(relaunch_pid(args(&["exp", "exponential://oauth-return"])), None);
        assert_eq!(relaunch_pid(args(&["exp", RELAUNCH_AFTER_ARG])), None);
        assert_eq!(relaunch_pid(args(&["exp", RELAUNCH_AFTER_ARG, "x"])), None);
    }

    #[test]
    fn waiting_for_an_exit_is_bounded() {
        let no_wait = std::time::Duration::ZERO;
        assert!(wait_for_exit(|| false, 3, no_wait));
        assert!(!wait_for_exit(|| true, 3, no_wait));
        let polls = std::cell::Cell::new(0);
        let exits_on_third = || {
            polls.set(polls.get() + 1);
            polls.get() < 3
        };
        assert!(wait_for_exit(exits_on_third, 5, no_wait));
    }

    #[test]
    fn our_own_process_is_alive() {
        assert!(is_alive(std::process::id()));
    }

    #[test]
    fn nonsense_pids_are_not_alive() {
        assert!(!is_alive(0));
        #[cfg(unix)]
        {
            assert!(!is_alive(u32::MAX - 1));
            assert!(!is_alive(i32::MAX as u32 + 1), "out of pid_t range");
        }
    }
}
