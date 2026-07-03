// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! PTY ownership (masterplan-v3 §6.3) — **we hold the master**.
//!
//! portable-pty hands us the master directly, so the raw child output has
//! exactly one reader — the read loop (§6.4) — and that is the only place the
//! steer tee needs to live. This module owns: open/spawn (argv + cwd + env
//! overlay + the §6.12 PATH augmentation), the ONE shared writer (local keys,
//! paste, remote steer input, and §6.6 event replies all funnel through it),
//! resize (TIOCSWINSZ → SIGWINCH), child-exit watching (exit-code capture for
//! the play/stop strip, §6.7), and kill.

use crate::read_loop::Wake;
use anyhow::Context as _;
use portable_pty::{
    native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize,
};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
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
/// (§6.7). The terminal crate only *captures* this; ending the
/// `coding_sessions` row is §07's launcher job (this crate has no api/tRPC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildExit {
    pub code: i32,
    pub success: bool,
    pub signal: Option<String>,
}

/// Shared slot the wait thread fills with the captured exit (§6.7).
pub type ExitSlot = Arc<Mutex<Option<ChildExit>>>;

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    /// The ONE shared writer (§6.3): local keystrokes, bracketed paste, remote
    /// steer input, and terminal-event replies (DA/DSR answers, §6.6) all
    /// funnel through this. The child cannot tell them apart — by design.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Kill handle cloned off the child before the wait thread takes it.
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    /// Taken exactly once by `spawn_wait_thread` (§6.7).
    child: Option<Box<dyn Child + Send + Sync>>,
    /// Taken exactly once by the read loop (§6.4) — never cloned again: two
    /// concurrent blocking reads on one master race and split the stream.
    reader: Option<Box<dyn Read + Send>>,
    process_id: Option<u32>,
}

/// Open a PTY at `cols`×`rows` and spawn `spec` with the slave as its
/// controlling tty (§6.3).
pub fn open(spec: &SpawnSpec, cols: u16, rows: u16) -> anyhow::Result<Pty> {
    let cmd = build_command(spec);
    let pair = native_pty_system().openpty(PtySize {
        rows,
        cols,
        // Character cells only — TUIs wanting pixel geometry (sixel) are out
        // of scope for v1 (§6.3).
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("spawn `{}` into pty", spec.program))?;
    let reader = pair.master.try_clone_reader()?; // the ONE reader (§6.4)
    let writer = pair.master.take_writer()?;
    // MUST drop the slave (§6.3): as long as we hold it open, the master's
    // reader never sees EOF on child exit — the read loop would block forever
    // and the play→stop flip / coding_sessions end signal would never fire.
    drop(pair.slave);
    let killer = child.clone_killer();
    let process_id = child.process_id();
    Ok(Pty {
        master: pair.master,
        writer: Arc::new(Mutex::new(writer)),
        killer: Mutex::new(killer),
        child: Some(child),
        reader: Some(reader),
        process_id,
    })
}

impl Pty {
    /// Take the single blocking reader — exactly once, by the read loop.
    pub fn take_reader(&mut self) -> Box<dyn Read + Send> {
        self.reader.take().expect("pty reader taken twice")
    }

    /// The shared writer handle (local keys + paste + steer input + replies).
    pub fn writer(&self) -> Arc<Mutex<Box<dyn Write + Send>>> {
        self.writer.clone()
    }

    /// Write raw bytes to the child — the steer crate's inject path (§6.14)
    /// and the event-reply path (§6.6) both land here.
    pub fn writer_write(&self, bytes: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    /// Resize the PTY winsize — portable-pty issues TIOCSWINSZ, which delivers
    /// **SIGWINCH** to the child so `claude`/`vim` reflow (§6.10 step 1).
    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
        Ok(())
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process_id
    }

    /// Kill the child (closing a tab kills its child, §6.13). Errors are
    /// expected when the child already exited — logged, not surfaced.
    pub fn kill(&self) {
        if let Ok(mut killer) = self.killer.lock() {
            if let Err(e) = killer.kill() {
                log::debug!("pty kill (child likely already exited): {e}");
            }
        }
    }

    /// Spawn the dedicated wait thread (§6.7): blocking `child.wait()` →
    /// captured `ChildExit` into the returned slot → `Wake::ChildExited` up
    /// the wake channel. Callable exactly once.
    pub fn spawn_wait_thread(
        &mut self,
        wake: flume::Sender<Wake>,
    ) -> anyhow::Result<(ExitSlot, JoinHandle<()>)> {
        let mut child = self.child.take().context("pty wait thread already spawned")?;
        let slot: ExitSlot = Arc::new(Mutex::new(None));
        let slot_in_thread = slot.clone();
        let handle = std::thread::Builder::new()
            .name("pty-wait".into())
            .spawn(move || {
                let exit = match child.wait() {
                    Ok(status) => ChildExit {
                        code: status.exit_code() as i32,
                        success: status.success(),
                        signal: status.signal().map(str::to_owned),
                    },
                    Err(e) => {
                        log::warn!("pty child wait: {e}");
                        ChildExit { code: -1, success: false, signal: None }
                    }
                };
                if let Ok(mut slot) = slot_in_thread.lock() {
                    *slot = Some(exit);
                }
                let _ = wake.try_send(Wake::ChildExited);
            })
            .context("spawn pty-wait thread")?;
        Ok((slot, handle))
    }
}

/// Build the `CommandBuilder` for a spawn: argv, cwd, then the safe base env
/// (`TERM`/`COLORTERM` + augmented `PATH`, §6.12), then the caller's overlay.
pub fn build_command(spec: &SpawnSpec) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&spec.program);
    cmd.args(&spec.args);
    if let Some(cwd) = &spec.cwd {
        cmd.cwd(cwd);
    }
    // §6.12: full color capability set into our emulator…
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // …and THE critical fix (EXP-2b/EXP-4/EXP-5): a .app/.desktop launch has a
    // minimal PATH without Homebrew/npm-global/~/.claude/local — bare
    // `claude`/`git` would fail to resolve.
    cmd.env("PATH", login_path());
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    cmd
}

/// The user's REAL interactive PATH, resolved once and cached for the process
/// lifetime (§6.12). Reused for the `claude`, run-config, and shell spawns.
///
/// Resolution shells out (`$SHELL -lic`, `npm config get prefix`) and is
/// therefore **bounded** ([`run_captured`]) — a pathological rc file must
/// never wedge the `OnceLock` and with it every future spawn. Call
/// [`prewarm_login_path`] at app init so the one-time cost lands on a
/// background thread instead of the first spawn on the gpui foreground.
pub fn login_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(resolve_login_path)
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

    #[test]
    fn dedup_prepend_orders_and_dedupes() {
        let prepend = vec!["/opt/homebrew/bin".to_owned(), "/usr/local/bin".to_owned()];
        let base = "/usr/bin:/opt/homebrew/bin::/bin";
        assert_eq!(
            dedup_prepend(&prepend, base),
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
        );
    }

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
