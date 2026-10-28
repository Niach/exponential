//! NEW-17 (FEED-89/90) — every agent run in its OWN systemd scope on Linux.
//!
//! On a small Linux runner the agent CLIs and everything they start (a
//! Playwright `chrome-headless` alone takes 3-4 GB) used to live in the
//! daemon's own `exponential-daemon.service` cgroup. One OOM kill in that
//! cgroup made systemd stop the WHOLE unit (`OOMPolicy=stop`, the service
//! default): every run on the machine died in the same second, and each row
//! read `endedBy=client`.
//!
//! [`spawn_lines`](crate::transport::spawn_lines) therefore launches the
//! agent through `systemd-run --scope` when this host can: a transient scope
//! per run, `OOMPolicy=continue` (the kernel kills the offending process —
//! the test browser — and the run's agent lives on to see its command fail),
//! and an optional `MemoryMax=` per run. `systemd-run --scope` registers the
//! scope and then EXECs the command in place, so the child's pid, process
//! group, pipes and environment are exactly the unwrapped ones.
//!
//! Configuration (environment of the daemon / app process):
//! - `EXP_RUN_SCOPE=off` (or `0`/`false`/`no`): never wrap;
//! - `EXP_RUN_MEMORY_MAX=<systemd value>` (`4G`, `60%`): each run's
//!   `MemoryMax=`; unset or empty = no per-run limit (isolation only).
//!
//! Anything that keeps a scope from working (not Linux, no `systemd-run`, no
//! reachable user manager, an older systemd that refuses a property) falls
//! back SILENTLY to the plain spawn: one probe per process
//! ([`ScopeMode::probe`]), its verdict cached.

use std::sync::OnceLock;

/// The disable switch.
pub const RUN_SCOPE_ENV: &str = "EXP_RUN_SCOPE";
/// The per-run memory cap.
pub const RUN_MEMORY_MAX_ENV: &str = "EXP_RUN_MEMORY_MAX";

/// How this host wraps a run (the probe's verdict).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeMode {
    /// Spawn as is.
    Off,
    /// `systemd-run [--user] --scope …`: `user` = the user manager (a user
    /// service, a desktop session) rather than the system one (root);
    /// `oom_policy` = this systemd takes `OOMPolicy=` on a scope.
    Systemd { user: bool, oom_policy: bool, memory_max: Option<String> },
}

impl ScopeMode {
    /// The process-wide verdict (probed once).
    pub fn current() -> &'static ScopeMode {
        static MODE: OnceLock<ScopeMode> = OnceLock::new();
        MODE.get_or_init(|| {
            let mode = Self::probe(
                std::env::var(RUN_SCOPE_ENV).ok().as_deref(),
                std::env::var(RUN_MEMORY_MAX_ENV).ok().as_deref(),
                run_ok,
            );
            if mode != ScopeMode::Off {
                log::info!("engine: agent runs get their own systemd scope ({mode:?})");
            }
            mode
        })
    }

    /// Decide the mode: `switch` / `memory_max` = the two variables, `try_run`
    /// runs a probe argv and says whether it exited 0. Pure but for
    /// `try_run`. Linux only; the user manager first, then (as root) the
    /// system one; `OOMPolicy=` dropped when this systemd refuses it on a
    /// scope; a `MemoryMax=` this systemd refuses drops the scope entirely
    /// (an asked-for cap is never silently half-applied, nor fatal).
    pub fn probe(
        switch: Option<&str>,
        memory_max: Option<&str>,
        mut try_run: impl FnMut(&[String]) -> bool,
    ) -> ScopeMode {
        if !cfg!(target_os = "linux") || switch_off(switch) {
            return ScopeMode::Off;
        }
        let memory_max = memory_max.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        let managers: &[bool] = if is_root() { &[true, false] } else { &[true] };
        for &user in managers {
            for oom_policy in [true, false] {
                let mode = ScopeMode::Systemd { user, oom_policy, memory_max: memory_max.clone() };
                if try_run(&mode.wrap("true", &[])) {
                    return mode;
                }
            }
        }
        ScopeMode::Off
    }

    /// The argv that runs `program args` under this mode (program first).
    pub fn wrap(&self, program: &str, args: &[String]) -> Vec<String> {
        match self {
            ScopeMode::Off => std::iter::once(program.to_string()).chain(args.iter().cloned()).collect(),
            ScopeMode::Systemd { user, oom_policy, memory_max } => {
                let mut argv = vec!["systemd-run".to_string()];
                if *user {
                    argv.push("--user".to_string());
                }
                argv.extend(["--scope", "--quiet", "--collect"].map(str::to_string));
                if *oom_policy {
                    argv.extend(["-p".to_string(), "OOMPolicy=continue".to_string()]);
                }
                if let Some(max) = memory_max {
                    argv.extend(["-p".to_string(), format!("MemoryMax={max}")]);
                }
                argv.push("--".to_string());
                argv.push(program.to_string());
                argv.extend(args.iter().cloned());
                argv
            }
        }
    }
}

fn switch_off(value: Option<&str>) -> bool {
    value
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "off" | "0" | "false" | "no"))
}

#[cfg(unix)]
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(unix))]
fn is_root() -> bool {
    false
}

/// Run a probe argv quietly; exit 0 within a few seconds = true.
fn run_ok(argv: &[String]) -> bool {
    let Some((program, args)) = argv.split_first() else { return false };
    let child = terminal::process::background_command(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return false };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn off_runs_the_program_as_is() {
        assert_eq!(ScopeMode::Off.wrap("/bin/claude", &s(&["--x"])), s(&["/bin/claude", "--x"]));
    }

    #[test]
    fn a_scope_execs_the_program_after_its_properties() {
        let mode = ScopeMode::Systemd { user: true, oom_policy: true, memory_max: Some("4G".to_string()) };
        assert_eq!(
            mode.wrap("/bin/claude", &s(&["--resume", "x"])),
            s(&[
                "systemd-run", "--user", "--scope", "--quiet", "--collect", "-p", "OOMPolicy=continue", "-p",
                "MemoryMax=4G", "--", "/bin/claude", "--resume", "x",
            ])
        );
        let system = ScopeMode::Systemd { user: false, oom_policy: false, memory_max: None };
        assert_eq!(
            system.wrap("codex", &[]),
            s(&["systemd-run", "--scope", "--quiet", "--collect", "--", "codex"])
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_probe_takes_the_richest_mode_that_runs_and_falls_back_silently() {
        // Everything works: the user manager with OOMPolicy.
        let all = ScopeMode::probe(None, Some(" 60% "), |_| true);
        assert_eq!(all, ScopeMode::Systemd { user: true, oom_policy: true, memory_max: Some("60%".to_string()) });
        // An older systemd refuses OOMPolicy on a scope.
        let older = ScopeMode::probe(None, None, |argv| !argv.iter().any(|arg| arg.starts_with("OOMPolicy")));
        assert_eq!(older, ScopeMode::Systemd { user: true, oom_policy: false, memory_max: None });
        // No systemd at all, or the switch is off: the plain spawn.
        if !is_root() {
            assert_eq!(ScopeMode::probe(None, None, |_| false), ScopeMode::Off);
        }
        let mut probed = false;
        assert_eq!(ScopeMode::probe(Some("off"), None, |_| { probed = true; true }), ScopeMode::Off);
        assert!(!probed, "a disabled scope never probes");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn no_scope_off_linux() {
        assert_eq!(ScopeMode::probe(None, Some("4G"), |_| true), ScopeMode::Off);
    }
}
