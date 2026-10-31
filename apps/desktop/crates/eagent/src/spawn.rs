//! What a host spawns and how it ended, plus the user's real login `PATH`
//! (EXP-1426 part 4, moved out of `terminal::pty`, which re-exports all of
//! it). The agent transport (`engine::transport`) spawns over plain pipes
//! with these; the PTY emulator in `terminal` opens the same `SpawnSpec` on
//! a pseudo-terminal. Pure std: no PTY, no UI.

#[cfg(not(windows))]
use std::collections::HashSet;
use std::io::Read;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
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

// ---------------------------------------------------------------------------
// The headless PTY (EXP-1426 part 4b, out of `terminal::pty`, which re-exports
// it): the ACP `terminal/*` capability and the emulator both open one.
// ---------------------------------------------------------------------------

/// The cell size a PTY's pixel extent starts at before an emulator measured
/// its font (`terminal::emulator::DEFAULT_CELL_PX` is this value).
pub const DEFAULT_CELL_PX: (u32, u32) = (8, 16);

fn pty_err(err: impl std::fmt::Display) -> io::Error {
    io::Error::other(err.to_string())
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

/// The winsize the kernel hands a `TIOCGWINSZ` caller: cells plus the pixel
/// extent those cells cover. Pixel-aware TUIs (libvaxis, chafa, timg, …)
/// read `ws_xpixel`/`ws_ypixel` to size and scale inline images — with the
/// pixel pair left at 0 they never learn the cell size (rio-vt has no
/// in-band resize) and either fall back to unscaled output or divide by
/// zero. The emulator's `cell_px` is the source of truth (§6.3).
pub fn winsize(cols: u16, rows: u16, cell_px: (u32, u32)) -> PtySize {
    let clamp = |px: u32| px.min(u16::MAX as u32) as u16;
    PtySize {
        rows,
        cols,
        pixel_width: clamp(cols as u32 * cell_px.0),
        pixel_height: clamp(rows as u32 * cell_px.1),
    }
}

/// Open a PTY at `cols`×`rows` and spawn `spec` with the slave as its
/// controlling tty (§6.3). `cell_px` seeds the pixel extent ([`winsize`]);
/// an emulator corrects it via [`Pty::resize`] once it knows the real
/// metrics (`terminal::pty::open` passes the emulator's default cell).
pub fn open_pty(spec: &SpawnSpec, cols: u16, rows: u16, cell_px: (u32, u32)) -> io::Result<Pty> {
    let cmd = build_command(spec);
    let pair = native_pty_system().openpty(winsize(cols, rows, cell_px)).map_err(pty_err)?;
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|err| io::Error::other(format!("spawn `{}` into pty: {err}", spec.program)))?;
    let reader = pair.master.try_clone_reader().map_err(pty_err)?; // the ONE reader (§6.4)
    let writer = pair.master.take_writer().map_err(pty_err)?;
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
    /// `cell_px` fills the pixel pair (see [`winsize`]).
    pub fn resize(&self, cols: u16, rows: u16, cell_px: (u32, u32)) -> io::Result<()> {
        self.master.resize(winsize(cols, rows, cell_px)).map_err(pty_err)?;
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
    /// captured `ChildExit` into the returned slot → `on_exit` (the emulator's
    /// `Wake::ChildExited`). Callable exactly once.
    pub fn spawn_wait_thread(
        &mut self,
        on_exit: impl FnOnce() + Send + 'static,
    ) -> io::Result<(ExitSlot, JoinHandle<()>)> {
        let mut child = self
            .child
            .take()
            .ok_or_else(|| io::Error::other("pty wait thread already spawned"))?;
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
                on_exit();
            })
            .map_err(|err| io::Error::other(format!("spawn pty-wait thread: {err}")))?;
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
    // …and THE critical fix: a .app/.desktop launch has a
    // minimal PATH without Homebrew/npm-global/~/.claude/local — bare
    // `claude`/`git` would fail to resolve.
    cmd.env("PATH", login_path());
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    #[cfg(target_os = "macos")]
    utf8_ctype_fallback(&mut cmd);
    cmd
}

/// EXP-1304: a .app launch (launchd) carries no `LANG`/`LC_*`, so the child
/// shell runs in the C locale and readline reads each byte of typed UTF-8 as
/// a meta key (a CJK char's `0x89` byte = M-TAB = completion). Like
/// Terminal.app, give the child a UTF-8 character type when nothing names
/// one; `LC_CTYPE` alone leaves the message language untouched, and any
/// locale the parent env or the spec sets wins.
#[cfg(target_os = "macos")]
fn utf8_ctype_fallback(cmd: &mut CommandBuilder) {
    let named = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .any(|key| cmd.get_env(key).is_some_and(|value| !value.is_empty()));
    if !named {
        cmd.env("LC_CTYPE", "UTF-8");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn utf8_ctype_fallback_only_without_a_named_locale() {
        let ctype = |vars: &[(&str, &str)]| {
            let mut cmd = CommandBuilder::new("sh");
            cmd.env_clear();
            for (key, value) in vars {
                cmd.env(key, value);
            }
            utf8_ctype_fallback(&mut cmd);
            cmd.get_env("LC_CTYPE").map(|v| v.to_string_lossy().into_owned())
        };
        assert_eq!(ctype(&[]).as_deref(), Some("UTF-8"));
        assert_eq!(ctype(&[("LANG", "")]).as_deref(), Some("UTF-8"));
        assert_eq!(ctype(&[("LANG", "de_AT.UTF-8")]), None);
        assert_eq!(ctype(&[("LC_ALL", "C")]), None);
        assert_eq!(ctype(&[("LC_CTYPE", "C")]).as_deref(), Some("C"));
    }

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
