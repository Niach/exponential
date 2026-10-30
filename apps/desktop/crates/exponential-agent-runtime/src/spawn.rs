//! What a host spawns and how it ended, plus the user's real login `PATH`
//! (EXP-1426 part 4, moved out of `terminal::pty`, which re-exports all of
//! it). The agent transport (`engine::transport`) spawns over plain pipes
//! with these; the PTY emulator in `terminal` opens the same `SpawnSpec` on
//! a pseudo-terminal. Pure std: no PTY, no UI.

#[cfg(not(windows))]
use std::collections::HashSet;
#[cfg(not(windows))]
use std::io::Read;
use std::path::PathBuf;
#[cfg(not(windows))]
use std::sync::OnceLock;
#[cfg(not(windows))]
use std::time::{Duration, Instant};

/// What to spawn into the PTY: argv + cwd + an env overlay.
///
/// `build_command` always applies `TERM=xterm-256color`, `COLORTERM=truecolor`
/// and the augmented login `PATH` (§6.12) first; entries in `env` are applied
/// after and can override any of them.
#[derive(Debug, Clone, Default)]
pub struct SpawnSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

impl SpawnSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self { program: program.into(), ..Self::default() }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

/// Captured child exit — feeds the tab play→stop flip and the exit-code badge
/// (§6.7). The spawner only *captures* this; ending the run is the host's
/// job (this crate has no api/tRPC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildExit {
    pub code: i32,
    pub success: bool,
    pub signal: Option<String>,
}

/// The user's REAL interactive PATH (§6.12). Reused for the `claude`,
/// run-config, and shell spawns.
///
/// Unix: resolved once and cached for the process lifetime — resolution
/// shells out (`$SHELL -lic`, `npm config get prefix`) and is therefore
/// **bounded** ([`run_captured`]); a pathological rc file must never wedge
/// the `OnceLock` and with it every future spawn. Call
/// [`prewarm_login_path`] at app init so the one-time cost lands on a
/// background thread instead of the first spawn on the gpui foreground.
///
/// Windows: read LIVE from the process env on every call (there is no
/// login-shell split to probe, and caching would defeat EXP-419's
/// [`crate::process::refresh_windows_path`] — a doctor re-check must see a
/// just-installed tool without an app restart).
#[cfg(not(windows))]
pub fn login_path() -> String {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(resolve_login_path).clone()
}

#[cfg(windows)]
pub fn login_path() -> String {
    resolve_login_path()
}

/// Kick the one-time [`login_path`] resolution on a background thread (called
/// from the terminal dock's init). Idempotent; concurrent callers of
/// `login_path` block on the same `OnceLock` — bounded, see above.
pub fn prewarm_login_path() {
    let _ = std::thread::Builder::new()
        .name("login-path".into())
        .spawn(|| {
            let _ = login_path();
        });
}

/// Run `cmd` with stdin nulled, capturing stdout, with a hard deadline. The
/// child is killed on timeout, and the pipe is drained on a helper thread
/// that is *detached* if a daemonized grandchild inherited the write end
/// (`Command::output` would block on that to EOF). `None` = caller falls
/// back; never hangs.
#[cfg(not(windows))]
fn run_captured(mut cmd: std::process::Command, timeout: Duration) -> Option<String> {
    use std::process::Stdio;

    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::Builder::new()
        .name("login-path-read".into())
        .spawn(move || {
            let mut out = String::new();
            let _ = stdout.read_to_string(&mut out);
            out
        })
        .ok()?;

    let deadline = Instant::now() + timeout;
    let exited_ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    if !exited_ok {
        return None; // reader thread ends on pipe EOF; harmless to leave
    }
    // Child exited; the pipe closes unless a grandchild inherited it — bound
    // the drain the same way.
    let read_deadline = Instant::now() + Duration::from_secs(1);
    while !reader.is_finished() {
        if Instant::now() >= read_deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    reader.join().ok()
}

/// Windows has no login-shell/rc-file PATH split — the process environment
/// already carries the user's full PATH (registry-backed), and it is
/// `;`-separated, so the unix probe/join below would corrupt it.
#[cfg(windows)]
fn resolve_login_path() -> String {
    std::env::var("PATH").unwrap_or_default()
}

#[cfg(not(windows))]
fn resolve_login_path() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    // login+interactive so rc files run and export the user's real PATH.
    // stdin is nulled, so an rc that tries to read won't hang us; the whole
    // invocation is deadline-bounded (see `run_captured`).
    let mut cmd = std::process::Command::new(&shell);
    cmd.args(["-lic", "printf %s \"$PATH\""]);
    let base = run_captured(cmd, Duration::from_secs(8))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());

    // Prepend the usual tool dirs defensively (some setups don't export via rc).
    let mut prepend: Vec<String> = vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into()];
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            prepend.push(format!("{home}/.local/bin"));
            prepend.push(format!("{home}/.claude/local"));
        }
    }
    if let Some(npm_bin) = npm_global_bin(&base) {
        prepend.push(npm_bin);
    }
    dedup_prepend(&prepend, &base)
}

/// `npm config get prefix`/bin — where npm-global installs (like `claude` via
/// npm) put their binaries. Resolved against the shell-derived base PATH so
/// npm itself is findable. Cached transitively via `login_path`; bounded like
/// the shell probe.
#[cfg(not(windows))]
fn npm_global_bin(base_path: &str) -> Option<String> {
    let mut cmd = std::process::Command::new("npm");
    cmd.env("PATH", base_path).args(["config", "get", "prefix"]);
    let prefix = run_captured(cmd, Duration::from_secs(5))?;
    let prefix = prefix.trim();
    if prefix.is_empty() || prefix == "undefined" {
        return None;
    }
    Some(format!("{prefix}/bin"))
}

/// `prepend` entries first (in order), then `base` — deduped, first
/// occurrence wins, empties dropped.
#[cfg(not(windows))]
fn dedup_prepend(prepend: &[String], base: &str) -> String {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut parts: Vec<&str> = Vec::new();
    for part in prepend.iter().map(String::as_str).chain(base.split(':')) {
        if part.is_empty() {
            continue;
        }
        if seen.insert(part) {
            parts.push(part);
        }
    }
    parts.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn dedup_prepend_orders_and_dedupes() {
        let prepend = vec!["/opt/homebrew/bin".to_owned(), "/usr/local/bin".to_owned()];
        let base = "/usr/bin:/opt/homebrew/bin::/bin";
        assert_eq!(
            dedup_prepend(&prepend, base),
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn dedup_prepend_empty_base() {
        let prepend = vec!["/a".to_owned()];
        assert_eq!(dedup_prepend(&prepend, ""), "/a");
    }

    #[test]
    fn spawn_spec_builder() {
        let spec = SpawnSpec::new("claude")
            .arg("--dangerously-skip-permissions")
            .cwd("/tmp")
            .env("FOO", "bar");
        assert_eq!(spec.program, "claude");
        assert_eq!(spec.args, vec!["--dangerously-skip-permissions".to_owned()]);
        assert_eq!(spec.cwd.as_deref(), Some(std::path::Path::new("/tmp")));
        assert_eq!(spec.env, vec![("FOO".to_owned(), "bar".to_owned())]);
    }
}
