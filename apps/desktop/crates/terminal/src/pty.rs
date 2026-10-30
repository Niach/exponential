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


/// EXP-1426 part 4: `SpawnSpec`, `ChildExit` and the login `PATH` moved to
/// the agent runtime (the agent transport spawns with them over pipes); the
/// PTY half below keeps using them under their old paths.
pub use exponential_agent_runtime::spawn::{login_path, prewarm_login_path, ChildExit, SpawnSpec};
/// EXP-1426 part 4b: the headless PTY (open/spawn, the shared writer, resize,
/// the wait thread, kill) moved too — the engine's ACP `terminal/*`
/// capability opens one with no emulator. Only the cell metrics stay here.
pub use exponential_agent_runtime::spawn::{build_command, winsize, ExitSlot, Pty};

/// Open a PTY at `cols`×`rows` and spawn `spec` with the slave as its
/// controlling tty (§6.3). The pixel extent starts at the emulator's
/// default cell size; the paint side corrects it via [`Pty::resize`] once
/// it knows the real metrics.
pub fn open(spec: &SpawnSpec, cols: u16, rows: u16) -> anyhow::Result<Pty> {
    Ok(exponential_agent_runtime::spawn::open_pty(
        spec,
        cols,
        rows,
        crate::emulator::DEFAULT_CELL_PX,
    )?)
}
