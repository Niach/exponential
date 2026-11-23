//! Background child-process helpers shared by every crate that shells out
//! (`coding`, `ui`, `steer`) — EXP-419.
//!
//! The GUI app is `windows_subsystem = "windows"`, so it owns no console;
//! every console-subsystem child (`git`, `claude`, `reg.exe`, …) spawned with
//! a plain `std::process::Command` allocates a **visible** conhost window for
//! the few milliseconds it runs. [`background_command`] is the one Command
//! constructor for captured/background spawns; interactive children go
//! through the ConPTY path in `terminal::pty` instead.
//!
//! [`refresh_windows_path`] solves the sibling problem: a running Windows
//! process never sees PATH edits an installer wrote to the registry, so
//! "install git → Check tools" stayed red until an app restart. Re-reading
//! the registry and updating the process env lets the doctor — and every
//! later spawn, including clones and agent launches — pick the tool up live.

use std::ffi::OsStr;

/// A `std::process::Command` that never flashes a console window on Windows.
/// Use for every spawn whose output is captured or ignored.
///
/// On unix the child starts with the termination signals at their DEFAULT
/// action ([`DEFAULT_SIGNALS`]): an ignored disposition survives `exec`, and
/// the desktop app has run with SIGTERM ignored. A git that dies (`fetch` of
/// a ref origin lacks) SIGTERMs its `git-remote-https` from its exit handler
/// and waits for it; an ignoring helper never exits, so every such fetch
/// hung until the 180 s idle kill, which held every fresh session's launch
/// for three minutes (`restore_wip` fetches a WIP ref a new branch never has).
///
/// EXP-2060: and the spawn must stay a `posix_spawn`. Any `pre_exec` turns
/// std's `posix_spawn` path off, so every git the app ran was a full
/// `fork()` of the 4 GB, 450-thread process: ~30 a second with 25 sessions,
/// each copying ~12 MB of page tables under the mmap write lock that every
/// page fault of the UI thread waits on. So the reset runs only when it is
/// needed, and it is not needed once [`catch_ignored_signals`] ran:
///
/// - none of [`DEFAULT_SIGNALS`] is `SIG_IGN` here (the normal case): a plain
///   `Command`, which std spawns with `posix_spawn` (`CLONE_VM|CLONE_VFORK`
///   on glibc, no page-table copy). The child gets each of them at its
///   default all the same: a `SIG_DFL` is inherited as `SIG_DFL`, and a
///   CAUGHT signal resets to `SIG_DFL` on `execve` (POSIX; glibc's spawn
///   child resets it before the exec already). Everything else is what the
///   fork path gives: std resets SIGPIPE on both paths, and both inherit
///   the signal mask and every other disposition;
/// - one is `SIG_IGN` (a host that never called [`catch_ignored_signals`]):
///   the old `pre_exec` reset, a fork.
///
/// [`forks_on_this_thread`] is how tests prove a spawn path never forks.
pub fn background_command(program: impl AsRef<OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        if DEFAULT_SIGNALS.into_iter().any(ignored) {
            reset_in_child(&mut cmd);
        }
    }
    cmd
}

/// The fallback reset: `SIG_DFL` for [`DEFAULT_SIGNALS`] between fork and
/// exec (a `pre_exec`, so a fork).
#[cfg(unix)]
fn reset_in_child(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: only `signal(2)` runs between fork and exec; it is
    // async-signal-safe and touches no memory of the parent.
    unsafe {
        cmd.pre_exec(|| {
            for signal in DEFAULT_SIGNALS {
                libc::signal(signal, libc::SIG_DFL);
            }
            Ok(())
        });
    }
}

/// The signals [`background_command`] resets to their default action.
#[cfg(unix)]
const DEFAULT_SIGNALS: [libc::c_int; 4] = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP, libc::SIGQUIT];

/// EXP-2060: call first thing in a host's `main` (the desktop app, the CLI).
/// Each of [`DEFAULT_SIGNALS`] this process INHERITED as `SIG_IGN` (a
/// `nohup`, a shell's background job, an agent's shell) gets a handler that
/// does nothing instead. The process still shrugs the signal off, but a
/// caught signal resets to its default on `execve`, so every child starts at
/// the default without a `pre_exec`, and [`background_command`] keeps std's
/// `posix_spawn` path. Defaults and real handlers are left alone; no-op
/// elsewhere. Idempotent.
pub fn catch_ignored_signals() {
    #[cfg(unix)]
    {
        for signal in DEFAULT_SIGNALS.into_iter().filter(|signal| ignored(*signal)) {
            // SAFETY: a zeroed `sigaction` is a valid value; the handler only
            // returns (async-signal-safe), and `SA_RESTART` keeps the calls
            // it interrupts restarting as far as the kernel restarts them.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = shrug as extern "C" fn(libc::c_int) as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART;
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                    log::warn!("process: signal {signal} stays ignored: {}", std::io::Error::last_os_error());
                }
            }
        }
    }
}

/// [`catch_ignored_signals`]'s handler: the signal is caught and dropped.
#[cfg(unix)]
extern "C" fn shrug(_signal: libc::c_int) {}

/// Is `signal`'s disposition `SIG_IGN` right now? A failed query counts as
/// ignored (the reset then runs: slower, never wrong).
#[cfg(unix)]
fn ignored(signal: libc::c_int) -> bool {
    // SAFETY: a query (no new action) into an owned, zeroed struct.
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        libc::sigaction(signal, std::ptr::null(), &mut current) != 0 || current.sa_sigaction == libc::SIG_IGN
    }
}

/// EXP-2060, for tests: how many `fork()`s the CALLING thread has made since
/// the first call (a `pthread_atfork` hook, registered on that first call;
/// `posix_spawn` runs no fork hooks, std's fork path always does). A test
/// reads it around a spawn path to prove it forks nothing. Always 0 off unix.
#[cfg(unix)]
#[doc(hidden)]
pub fn forks_on_this_thread() -> u64 {
    static HOOKED: std::sync::Once = std::sync::Once::new();
    // SAFETY: the prepare hook only bumps a const-initialised thread-local
    // counter (no allocation, no lock).
    HOOKED.call_once(|| unsafe {
        libc::pthread_atfork(Some(count_fork), None, None);
    });
    FORKS.with(std::cell::Cell::get)
}

#[cfg(not(unix))]
#[doc(hidden)]
pub fn forks_on_this_thread() -> u64 {
    0
}

/// Tests that change this process's dispositions, or count forks (which
/// needs the fast path), run one at a time across the crate.
#[cfg(all(test, unix))]
pub(crate) fn signals_lock() -> std::sync::MutexGuard<'static, ()> {
    static SIGNALS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SIGNALS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(unix)]
thread_local! {
    static FORKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(unix)]
unsafe extern "C" fn count_fork() {
    FORKS.with(|forks| forks.set(forks.get() + 1));
}

/// Re-read the machine+user `Path` from the registry and update this
/// process's `PATH` so children inherit a fresh view (Windows only; no-op
/// elsewhere). Installers edit the registry and broadcast a change message,
/// but an already-running process's environment block is never updated —
/// without this, a freshly installed `git`/`claude` is invisible until
/// restart.
///
/// Entries the process already carries but the registry doesn't (launcher or
/// app-injected dirs) are preserved, appended after the registry entries.
pub fn refresh_windows_path() {
    #[cfg(windows)]
    {
        let system = query_registry_path(
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        );
        let user = query_registry_path(r"HKCU\Environment");
        if system.is_none() && user.is_none() {
            return; // reg.exe unavailable/failed — keep the current env
        }
        let process = std::env::var("PATH").unwrap_or_default();
        let merged = merge_windows_path(
            system.as_deref().unwrap_or(""),
            user.as_deref().unwrap_or(""),
            &process,
        );
        if !merged.is_empty() {
            // Safe pre-2024-edition; the Windows env APIs are thread-safe.
            std::env::set_var("PATH", merged);
        }
    }
}

/// `reg.exe query <key> /v Path` → the expanded value. Spawned hidden; any
/// failure (missing value, weird output) is `None` — the caller treats that
/// as "leave the env alone".
#[cfg(windows)]
fn query_registry_path(key: &str) -> Option<String> {
    let output = background_command("reg.exe")
        .args(["query", key, "/v", "Path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = parse_reg_query_value(&stdout)?;
    Some(expand_env_placeholders(&raw, |name| std::env::var(name).ok()))
}

/// Pull the data out of a `reg.exe query … /v Path` output line:
///
/// ```text
/// HKEY_CURRENT_USER\Environment
///     Path    REG_EXPAND_SZ    %USERPROFILE%\.local\bin;C:\tools
/// ```
///
/// The data may itself contain spaces, so split on the `REG_*` type token
/// rather than whitespace columns.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_reg_query_value(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        // Value-name match is case-insensitive ("Path" vs "PATH").
        let Some(rest) = trimmed
            .strip_prefix("Path")
            .or_else(|| trimmed.strip_prefix("PATH"))
            .or_else(|| trimmed.strip_prefix("path"))
        else {
            continue;
        };
        let rest = rest.trim_start();
        for reg_type in ["REG_EXPAND_SZ", "REG_SZ"] {
            if let Some(value) = rest.strip_prefix(reg_type) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        // A near-miss (e.g. a PATHEXT row) must not abort the parse — the
        // real Path row may still follow.
        continue;
    }
    None
}

/// `%NAME%` → `lookup(NAME)`, unknown names left verbatim (matching
/// `ExpandEnvironmentStrings`). Lookup is case-insensitive on real Windows;
/// the injected `lookup` decides (tests pass exact names).
#[cfg_attr(not(windows), allow(dead_code))]
fn expand_env_placeholders(value: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name) {
                    Some(replacement) if !name.is_empty() => out.push_str(&replacement),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                // Unmatched trailing % — keep verbatim.
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Windows PATH semantics: system entries, then user entries, then whatever
/// extra entries the process already had — `;`-joined, deduped
/// case-insensitively (first occurrence wins), empties dropped.
#[cfg_attr(not(windows), allow(dead_code))]
fn merge_windows_path(system: &str, user: &str, process: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut parts: Vec<&str> = Vec::new();
    for part in system.split(';').chain(user.split(';')).chain(process.split(';')) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let key = part.to_ascii_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            parts.push(part);
        }
    }
    parts.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `signal`'s whole disposition, to put back after a test.
    #[cfg(unix)]
    fn saved(signal: libc::c_int) -> libc::sigaction {
        unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            assert_eq!(libc::sigaction(signal, std::ptr::null(), &mut current), 0);
            current
        }
    }

    #[cfg(unix)]
    fn restore(signal: libc::c_int, action: &libc::sigaction) {
        assert_eq!(unsafe { libc::sigaction(signal, action, std::ptr::null_mut()) }, 0);
    }

    /// `sh` that SIGTERMs itself: dies of it at the default, prints when
    /// the signal is ignored.
    #[cfg(unix)]
    fn self_terminating_sh() -> std::process::Output {
        background_command("/bin/sh")
            .args(["-c", "kill -TERM $$; echo alive"])
            .output()
            .expect("sh runs")
    }

    /// A SIGTERM this process ignores does not reach the child: it dies of
    /// its own `kill -TERM $$` instead of printing.
    #[cfg(unix)]
    #[test]
    fn a_child_gets_sigterm_back_even_when_the_host_ignores_it() {
        use std::os::unix::process::ExitStatusExt;
        let _lock = signals_lock();
        let previous = saved(libc::SIGTERM);
        unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
        let output = self_terminating_sh();
        restore(libc::SIGTERM, &previous);
        assert_eq!(output.status.signal(), Some(libc::SIGTERM));
        assert!(output.stdout.is_empty());
    }

    /// EXP-2060: the host's way. An inherited `SIG_IGN` becomes a no-op
    /// handler: the child still starts at the default (handlers reset on
    /// exec), and the spawn is a `posix_spawn`, not a fork of this process.
    #[cfg(unix)]
    #[test]
    fn an_inherited_ignore_reaches_the_child_at_its_default_without_a_fork() {
        use std::os::unix::process::ExitStatusExt;
        let _lock = signals_lock();
        catch_ignored_signals();
        let previous = saved(libc::SIGTERM);
        unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
        catch_ignored_signals();
        let caught = saved(libc::SIGTERM).sa_sigaction;
        let before = forks_on_this_thread();
        let output = self_terminating_sh();
        let forks = forks_on_this_thread() - before;
        restore(libc::SIGTERM, &previous);
        assert!(caught != libc::SIG_IGN && caught != libc::SIG_DFL, "the ignore became a handler");
        assert_eq!(output.status.signal(), Some(libc::SIGTERM));
        assert!(output.stdout.is_empty());
        assert_eq!(forks, 0, "a background spawn forked the host");
    }

    /// A default stays a default, a real handler stays that handler.
    #[cfg(unix)]
    #[test]
    fn catching_leaves_defaults_and_handlers_alone() {
        extern "C" fn mine(_: libc::c_int) {}
        let _lock = signals_lock();
        let previous = saved(libc::SIGTERM);
        unsafe { libc::signal(libc::SIGTERM, libc::SIG_DFL) };
        catch_ignored_signals();
        let after_default = saved(libc::SIGTERM).sa_sigaction;
        unsafe { libc::signal(libc::SIGTERM, mine as extern "C" fn(libc::c_int) as libc::sighandler_t) };
        catch_ignored_signals();
        let after_handler = saved(libc::SIGTERM).sa_sigaction;
        restore(libc::SIGTERM, &previous);
        assert_eq!(after_default, libc::SIG_DFL);
        assert_eq!(after_handler, mine as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }

    /// The probe itself: a `pre_exec` spawn is a fork and counts, a
    /// background spawn does not (what every hot path's test relies on).
    #[cfg(unix)]
    #[test]
    fn the_fork_probe_tells_a_fork_from_a_spawn() {
        use std::os::unix::process::CommandExt;
        let _lock = signals_lock();
        catch_ignored_signals();
        let before = forks_on_this_thread();
        let status = background_command("/bin/sh").args(["-c", "exit 0"]).status().expect("sh runs");
        assert!(status.success());
        assert_eq!(forks_on_this_thread(), before, "a background spawn forked");
        let mut forked = std::process::Command::new("/bin/sh");
        forked.args(["-c", "exit 0"]);
        unsafe { forked.pre_exec(|| Ok(())) };
        assert!(forked.status().expect("sh runs").success());
        assert_eq!(forks_on_this_thread(), before + 1, "the probe missed a fork");
    }

    #[test]
    fn parses_reg_query_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Environment\r\n    Path    REG_EXPAND_SZ    %USERPROFILE%\\.local\\bin;C:\\Program Files\\Git\\cmd\r\n\r\n";
        assert_eq!(
            parse_reg_query_value(out).as_deref(),
            Some("%USERPROFILE%\\.local\\bin;C:\\Program Files\\Git\\cmd")
        );
    }

    #[test]
    fn parses_reg_sz_and_case_insensitive_name() {
        let out = "HKEY_LOCAL_MACHINE\\...\\Environment\n    PATH    REG_SZ    C:\\Windows;C:\\Windows\\system32\n";
        assert_eq!(
            parse_reg_query_value(out).as_deref(),
            Some("C:\\Windows;C:\\Windows\\system32")
        );
    }

    #[test]
    fn reg_parse_skips_near_miss_rows() {
        let out = "HKEY_CURRENT_USER\\Environment\n    PATHEXT    REG_SZ    .COM;.EXE\n    Path    REG_EXPAND_SZ    C:\\tools\n";
        assert_eq!(parse_reg_query_value(out).as_deref(), Some("C:\\tools"));
    }

    #[test]
    fn reg_parse_misses_are_none() {
        assert_eq!(parse_reg_query_value(""), None);
        assert_eq!(parse_reg_query_value("ERROR: The system was unable to find the specified registry key or value."), None);
    }

    #[test]
    fn expands_known_placeholders_keeps_unknown() {
        let expanded = expand_env_placeholders(
            "%USERPROFILE%\\.local\\bin;%NOPE%\\x;50%",
            |name| (name == "USERPROFILE").then(|| "C:\\Users\\d".to_string()),
        );
        assert_eq!(expanded, "C:\\Users\\d\\.local\\bin;%NOPE%\\x;50%");
    }

    #[test]
    fn merges_system_user_process_dedup_case_insensitive() {
        let merged = merge_windows_path(
            "C:\\Windows;C:\\Windows\\system32",
            "C:\\Users\\d\\.local\\bin;c:\\windows",
            "C:\\Windows\\system32;C:\\app-injected",
        );
        assert_eq!(
            merged,
            "C:\\Windows;C:\\Windows\\system32;C:\\Users\\d\\.local\\bin;C:\\app-injected"
        );
    }

    #[test]
    fn merge_all_empty_is_empty() {
        assert_eq!(merge_windows_path("", "", ""), "");
    }
}
