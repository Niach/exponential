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

#[cfg(test)]
mod tests {
    use super::*;

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
