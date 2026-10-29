// Clean reimplementation from the VT spec + rio-vt (MIT). NOT derived from Zed's GPL terminal crates.
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
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// EXP-1426 part 4: `SpawnSpec`, `ChildExit` and the login `PATH` moved to
/// the agent runtime (the agent transport spawns with them over pipes); the
/// PTY half below keeps using them under their old paths.
pub use exponential_agent_runtime::spawn::{login_path, prewarm_login_path, ChildExit, SpawnSpec};

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
/// controlling tty (§6.3). The pixel extent starts at the emulator's
/// default cell size; the paint side corrects it via [`Pty::resize`] once
/// it knows the real metrics.
pub fn open(spec: &SpawnSpec, cols: u16, rows: u16) -> anyhow::Result<Pty> {
    let cmd = build_command(spec);
    let pair = native_pty_system().openpty(winsize(cols, rows, crate::emulator::DEFAULT_CELL_PX))?;
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
    /// `cell_px` fills the pixel pair (see [`winsize`]).
    pub fn resize(&self, cols: u16, rows: u16, cell_px: (u32, u32)) -> anyhow::Result<()> {
        self.master.resize(winsize(cols, rows, cell_px))?;
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
}
