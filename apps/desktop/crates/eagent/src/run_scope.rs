//! NEW-17 (FEED-89/90) — every agent run in its OWN systemd scope on Linux.
//!
//! On a small Linux runner the agent CLIs and everything they start (a
//! Playwright `chrome-headless` alone takes 3-4 GB) used to live in the
//! daemon's own `exponential-daemon.service` cgroup. One OOM kill in that
//! cgroup made systemd stop the WHOLE unit (`OOMPolicy=stop`, the service
//! default): every run on the machine died in the same second, and each row
//! read `endedBy=client`.
//!
//! `engine::transport::spawn_lines` therefore launches the
//! agent through `systemd-run --scope` when this host can: a transient scope
//! per run, `OOMPolicy=continue` (the kernel kills the offending process —
//! the test browser — and the run's agent lives on to see its command fail),
//! and an optional `MemoryMax=` per run. `systemd-run --scope` registers the
//! scope and then EXECs the command in place, so the child's pid, process
//! group, pipes and environment are exactly the unwrapped ones.
//!
//! The argv must also reach the child byte for byte. systemd 258 turned
//! `--expand-environment=` on for scopes, so a bare wrap resolves every
//! `${VAR}` from the environment `systemd-run` got: `Bearer ${EXP_MCP_TOKEN}`
//! in `--mcp-config` would exec with the raw key, readable in `ps` by every
//! user on the machine. The wrap therefore turns expansion off, and no mode is
//! taken unless a probe saw a `${…}` arrive verbatim ([`ScopeMode::probe`]):
//! a host that cannot prove it runs the plain spawn. systemd older than 254
//! refuses the flag and never expands, so it wraps without it.
//!
//! PERF-4: a run is BACKGROUND work. Its agent and every cargo/gradle build it
//! starts must never starve the desktop: on Danny's 32-core Mint a 180 ms frame
//! took 416 ms (p90 692 ms) next to one agent-like scope of 48 busy threads at
//! the default weight, and 181 ms with that scope at `CPUWeight=idle`. The wrap
//! therefore asks for `CPUWeight=idle` (SCHED_IDLE against the scope's
//! siblings: games, browsers, other apps) and `--nice=10`
//! ([`RUN_NICE`]; it orders the run's own processes and is what macOS and a
//! host without a scope get, see `engine::transport::spawn_lines`). A systemd
//! that refuses `idle` (before 252) wraps without it.
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
/// The nice level of every agent run (PERF-4): below the desktop, above a
/// lane's `nice 19` build (scripts/lanes/lane.sh).
pub const RUN_NICE: i32 = 10;

/// The nice level a run actually asks for: [`RUN_NICE`], or this process's own
/// when that is lower still (a daemon under `Nice=19`, a lane): an
/// unprivileged process cannot raise a priority back, and a `--nice` it may
/// not set fails the whole wrap.
pub fn run_nice() -> i32 {
    #[cfg(unix)]
    {
        // `nice(0)` = the current level; its -1 error value is harmless here.
        let own = unsafe { libc::nice(0) };
        own.max(RUN_NICE)
    }
    #[cfg(not(unix))]
    {
        RUN_NICE
    }
}

/// How this host wraps a run (the probe's verdict).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeMode {
    /// Spawn as is.
    Off,
    /// `systemd-run [--user] --scope …`: `user` = the user manager (a user
    /// service, a desktop session) rather than the system one (root);
    /// `oom_policy` = this systemd takes `OOMPolicy=` on a scope;
    /// `no_expand` = pass `--expand-environment=no` (refused before 254);
    /// `background` = `CPUWeight=idle` + `--nice` (PERF-4; `idle` is refused
    /// before 252).
    Systemd { user: bool, oom_policy: bool, no_expand: bool, background: bool, memory_max: Option<String> },
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
    /// scope, then the background priority; a `MemoryMax=` this systemd refuses drops the scope entirely
    /// (an asked-for cap is never silently half-applied, nor fatal). Every
    /// candidate runs [`verbatim_probe`], so a flag that merely timed out
    /// cannot fall through to a bare wrap that expands: that wrap fails its
    /// own probe, and the host ends on the plain spawn.
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
            for no_expand in [true, false] {
                for oom_policy in [true, false] {
                    for background in [true, false] {
                        let mode = ScopeMode::Systemd {
                            user,
                            oom_policy,
                            no_expand,
                            background,
                            memory_max: memory_max.clone(),
                        };
                        let (program, args) = verbatim_probe();
                        if try_run(&mode.wrap(program, &args)) {
                            return mode;
                        }
                    }
                }
            }
        }
        ScopeMode::Off
    }

    /// The argv that runs `program args` under this mode (program first).
    pub fn wrap(&self, program: &str, args: &[String]) -> Vec<String> {
        match self {
            ScopeMode::Off => std::iter::once(program.to_string()).chain(args.iter().cloned()).collect(),
            ScopeMode::Systemd { user, oom_policy, no_expand, background, memory_max } => {
                let mut argv = vec!["systemd-run".to_string()];
                if *user {
                    argv.push("--user".to_string());
                }
                argv.extend(["--scope", "--quiet", "--collect"].map(str::to_string));
                if *no_expand {
                    argv.push("--expand-environment=no".to_string());
                }
                if *oom_policy {
                    argv.extend(["-p".to_string(), "OOMPolicy=continue".to_string()]);
                }
                if *background {
                    argv.extend([format!("--nice={}", run_nice()), "-p".to_string(), "CPUWeight=idle".to_string()]);
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

/// The command every candidate mode must run: it exits 0 only when its `$0`
/// still starts with `$`, the `${…}` it was given. An expanding systemd hands
/// it the empty string (the variable is unset) and it exits 1. The script
/// itself spells no `${…}`, so expansion cannot rewrite both sides alike.
fn verbatim_probe() -> (&'static str, Vec<String>) {
    (
        "sh",
        vec![
            "-c".to_string(),
            r#"case "$0" in \$*) exit 0;; esac; exit 1"#.to_string(),
            "${EXP_RUN_SCOPE_PROBE}".to_string(),
        ],
    )
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
    let child = crate::process::background_command(program)
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
        let mode = ScopeMode::Systemd {
            user: true,
            oom_policy: true,
            no_expand: true,
            background: true,
            memory_max: Some("4G".to_string()),
        };
        // 10, or the test's own level when it already runs lower (a lane).
        let nice = format!("--nice={}", run_nice());
        assert_eq!(
            mode.wrap("/bin/claude", &s(&["--resume", "x"])),
            s(&[
                "systemd-run", "--user", "--scope", "--quiet", "--collect", "--expand-environment=no", "-p",
                "OOMPolicy=continue", &nice, "-p", "CPUWeight=idle", "-p", "MemoryMax=4G", "--",
                "/bin/claude", "--resume", "x",
            ])
        );
        let system =
            ScopeMode::Systemd { user: false, oom_policy: false, no_expand: false, background: false, memory_max: None };
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
        assert_eq!(
            all,
            ScopeMode::Systemd {
                user: true,
                oom_policy: true,
                no_expand: true,
                background: true,
                memory_max: Some("60%".to_string())
            }
        );
        // An older systemd refuses OOMPolicy on a scope.
        let older = ScopeMode::probe(None, None, |argv| !argv.iter().any(|arg| arg.starts_with("OOMPolicy")));
        assert_eq!(
            older,
            ScopeMode::Systemd { user: true, oom_policy: false, no_expand: true, background: true, memory_max: None }
        );
        // PERF-4: a systemd before 252 refuses `CPUWeight=idle`: the wrap
        // keeps everything else.
        let no_idle = ScopeMode::probe(None, None, |argv| !argv.iter().any(|arg| arg == "CPUWeight=idle"));
        assert_eq!(
            no_idle,
            ScopeMode::Systemd { user: true, oom_policy: true, no_expand: true, background: false, memory_max: None }
        );
        // No systemd at all, or the switch is off: the plain spawn.
        if !is_root() {
            assert_eq!(ScopeMode::probe(None, None, |_| false), ScopeMode::Off);
        }
        let mut probed = false;
        assert_eq!(ScopeMode::probe(Some("off"), None, |_| { probed = true; true }), ScopeMode::Off);
        assert!(!probed, "a disabled scope never probes");
    }

    /// What each systemd does with a candidate wrap (measured on 252, 257 and
    /// 259): before 254 the flag is an unknown option; from 258 a wrap without
    /// the flag expands, so its probe exits 1.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_wrap_that_could_expand_is_never_taken() {
        let has_flag = |argv: &[String]| argv.iter().any(|arg| arg == "--expand-environment=no");
        let expanding = ScopeMode::probe(None, None, |argv| has_flag(argv));
        assert_eq!(
            expanding,
            ScopeMode::Systemd { user: true, oom_policy: true, no_expand: true, background: true, memory_max: None }
        );
        let ancient = ScopeMode::probe(None, None, |argv| !has_flag(argv));
        assert_eq!(
            ancient,
            ScopeMode::Systemd { user: true, oom_policy: true, no_expand: false, background: true, memory_max: None }
        );
        // The flagged probes time out on an expanding systemd: the bare ones
        // exit 1 by themselves, and the host runs the plain spawn.
        if !is_root() {
            assert_eq!(ScopeMode::probe(None, None, |_| false), ScopeMode::Off);
        }
    }

    /// The real wrap on this host, secret-shaped argument and all: whatever
    /// systemd this is (hosted `ubuntu-latest` = 259), the child's argv keeps
    /// the reference and never holds the value. A plain spawn host passes too,
    /// so the gate's `desktop-scope` job demands a real wrap with
    /// `EXP_RUN_SCOPE_EXPECT_WRAP=1`.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_real_wrap_hands_the_child_the_reference_not_the_secret() {
        if std::env::var_os("EXP_RUN_SCOPE_EXPECT_WRAP").is_some() {
            assert_ne!(ScopeMode::current(), &ScopeMode::Off, "this check needs a real systemd");
        }
        let out = std::env::temp_dir().join(format!("exp-run-scope-{}", std::process::id()));
        let script = format!(r#"printf %s "$0" > '{}'"#, out.display());
        let args = s(&["-c", &script, "Bearer ${EXP_RUN_SCOPE_TEST_TOKEN}"]);
        let argv = ScopeMode::current().wrap("sh", &args);
        let status = crate::process::background_command(&argv[0])
            .args(&argv[1..])
            .env("EXP_RUN_SCOPE_TEST_TOKEN", "expu_must-not-appear")
            .status()
            .expect("the wrap runs");
        assert!(status.success(), "{argv:?}");
        let seen = std::fs::read_to_string(&out).expect("the child wrote its argv");
        let _ = std::fs::remove_file(&out);
        assert_eq!(seen, "Bearer ${EXP_RUN_SCOPE_TEST_TOKEN}", "{:?}", ScopeMode::current());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn no_scope_off_linux() {
        assert_eq!(ScopeMode::probe(None, Some("4G"), |_| true), ScopeMode::Off);
    }
}
