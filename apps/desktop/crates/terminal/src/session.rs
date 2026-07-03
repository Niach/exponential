// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! One `Terminal` session = PTY + emulator + read loop + wait thread
//! (masterplan-v3 §6.2's `Terminal`, gpui-free).
//!
//! The gpui glue (`element.rs` paint, `tab.rs`/`manager.rs` tabs) wraps this
//! object in a later step: its foreground task blocks on `wake_rx()`, calls
//! `pump()` (which writes the §6.6 reply-required answers back to the PTY and
//! tracks the title), then repaints. Headless tests drive the exact same
//! surface.

use crate::emulator::{Emulator, EmulatorSignal, TermHandle};
use crate::pty::{self, ChildExit, ExitSlot, Pty, SpawnSpec};
use crate::read_loop::{spawn_read_loop, RawSink, SinkSet, Wake};
use alacritty_terminal::term::TermMode;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Notified with the new `(cols, rows)` whenever a genuine integer cell change
/// is applied (§6.10 step 3 / §8.4 resize-up). The steer layer installs one so
/// remote viewers reflow when the LOCAL window resizes the grid; `crates/terminal`
/// stays gpui-free and steer-free — it only invokes the callback (no ui dep).
pub type ResizeObserver = Box<dyn Fn(u16, u16) + Send + Sync>;

pub struct Terminal {
    pty: Pty,
    emulator: Emulator,
    sinks: SinkSet,
    wake_rx: flume::Receiver<Wake>,
    exit_slot: ExitSlot,
    title: Option<String>,
    read_thread: Option<JoinHandle<()>>,
    wait_thread: Option<JoinHandle<()>>,
    /// §6.10 step 3 seam — set by the steer wiring on a published session.
    resize_observer: Option<ResizeObserver>,
}

impl Terminal {
    /// Spawn `spec` into a fresh PTY at `cols`×`rows`, wire the emulator, the
    /// single read loop (§6.4, with the steer tee), and the wait thread
    /// (§6.7).
    pub fn spawn(spec: &SpawnSpec, cols: u16, rows: u16) -> anyhow::Result<Self> {
        let emulator = Emulator::new(cols, rows);
        let mut pty = pty::open(spec, cols.max(1), rows.max(1))?;
        let (wake_tx, wake_rx) = flume::unbounded();
        let sinks = SinkSet::new();
        let reader = pty.take_reader();
        let read_thread = spawn_read_loop(reader, emulator.term(), sinks.clone(), wake_tx.clone());
        let (exit_slot, wait_thread) = pty.spawn_wait_thread(wake_tx)?;
        Ok(Self {
            pty,
            emulator,
            sinks,
            wake_rx,
            exit_slot,
            title: None,
            read_thread: Some(read_thread),
            wait_thread: Some(wait_thread),
            resize_observer: None,
        })
    }

    /// User input (§6.5): bytes straight to the shared writer. The key
    /// handler calls this with `keys::to_esc_str` output; remote steer input
    /// arrives via `Pty::writer_write` on the same writer — identically.
    pub fn write(&self, bytes: &[u8]) {
        self.pty.writer_write(bytes);
    }

    /// Paste (§6.5): bracketed when the child turned `BRACKETED_PASTE` on —
    /// stops shells from running pasted newlines and lets editors/`claude`
    /// treat the block as literal input.
    pub fn paste(&self, text: &str) {
        let bracketed = {
            let term = self.emulator.term();
            let mode = *term.lock().mode();
            mode.contains(TermMode::BRACKETED_PASTE)
        };
        if bracketed {
            self.write(b"\x1b[200~");
            self.write(text.as_bytes());
            self.write(b"\x1b[201~");
        } else {
            self.write(text.as_bytes());
        }
    }

    /// Resize both sides together (§6.10): PTY winsize (TIOCSWINSZ →
    /// SIGWINCH) + `Term::resize` reflow — and only on an integer cell
    /// change. A 0-sized layout (collapsed dock, §6.9) is ignored rather than
    /// thrashing the child.
    pub fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        if (cols, rows) == self.emulator.size() {
            return Ok(());
        }
        // PTY winsize first (§6.10 order), but reshape the emulator even when
        // TIOCSWINSZ fails (master gone after child exit): the element retries
        // on every prepaint until `size()` matches, so leaving the emulator
        // stale would warn-log every frame forever.
        let pty_result = self.pty.resize(cols, rows);
        self.emulator.resize(cols, rows);
        // §6.10 step 3 — forward the genuine local geometry change to the steer
        // publisher (if one is attached) so remote viewers reflow (§8.4). We are
        // past the no-op guard above, so this only fires on a real cell change;
        // the publisher additionally clamps against its last-sent geometry, so a
        // resize that came DOWN from a steerer can't ping-pong back up.
        if let Some(observer) = &self.resize_observer {
            observer(cols, rows);
        }
        pty_result
    }

    /// Install the §6.10-step-3 resize observer (steer wiring, on a published
    /// session). Replaces any prior observer; cleared with [`Terminal::
    /// clear_resize_observer`] on teardown.
    pub fn set_resize_observer(&mut self, observer: ResizeObserver) {
        self.resize_observer = Some(observer);
    }

    /// Drop the resize observer (publisher teardown) so a stale notifier can't
    /// keep forwarding into a dead channel.
    pub fn clear_resize_observer(&mut self) {
        self.resize_observer = None;
    }

    /// Drain pending emulator events (§6.6): reply-required answers are
    /// written back to the PTY, the tab title is tracked, and the outward
    /// signals are handed to the caller (repaint/bell/title for the gpui
    /// layer).
    pub fn pump(&mut self) -> Vec<EmulatorSignal> {
        let pty = &self.pty;
        let signals = self.emulator.drain_events(&mut |bytes| pty.writer_write(bytes));
        for signal in &signals {
            if let EmulatorSignal::Title(title) = signal {
                self.title = title.clone();
            }
        }
        signals
    }

    /// The wake channel (§6.11): `Output` → repaint, `Eof`/`ChildExited` →
    /// pump + check `exit()`. Single consumer expected — the receiver is a
    /// handle onto the session's one queue, not a broadcast.
    pub fn wake_rx(&self) -> flume::Receiver<Wake> {
        self.wake_rx.clone()
    }

    /// Shared handle for painting (`renderable_content`) and tests.
    pub fn term(&self) -> TermHandle {
        self.emulator.term()
    }

    pub fn size(&self) -> (u16, u16) {
        self.emulator.size()
    }

    /// Latest OSC title (`None` until set or after a reset) — updated by
    /// `pump()`.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The captured exit (§6.7), `None` while running. The play/stop strip
    /// reads the code from here; §07's launcher ends the `coding_sessions`
    /// row when this flips (this crate only signals — it has no tRPC).
    pub fn exit(&self) -> Option<ChildExit> {
        self.exit_slot.lock().ok().and_then(|slot| slot.clone())
    }

    pub fn is_running(&self) -> bool {
        self.exit().is_none()
    }

    pub fn process_id(&self) -> Option<u32> {
        self.pty.process_id()
    }

    /// Attach a raw-output sink (§6.14) — the steer publisher's tap point.
    pub fn attach_sink(&self, sink: Arc<dyn RawSink>) {
        self.sinks.attach(sink);
    }

    pub fn detach_sink(&self, sink: &Arc<dyn RawSink>) {
        self.sinks.detach(sink);
    }

    /// The ONE shared PTY writer (§6.3/§6.14) as a `Send + Sync` handle — the
    /// steer publisher's inject path. Remote `input` frames written here land
    /// exactly like local keystrokes; the child cannot tell them apart (§8.4).
    pub fn writer(&self) -> Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>> {
        self.pty.writer()
    }

    /// Plain-text snapshot of the visible screen (test/debug helper, §6.9
    /// spacer-aware).
    pub fn screen_lines(&self) -> Vec<String> {
        self.emulator.screen_lines()
    }

    /// Kill the child (tab close, §6.13). Guarded on liveness to avoid
    /// signalling a reaped (potentially reused) pid.
    pub fn kill(&self) {
        if self.is_running() {
            self.pty.kill();
        }
    }

    /// Kill (if needed) and join both PTY threads — the §6.13 tab-close path.
    /// Also runs on `Drop`.
    ///
    /// Joins are **bounded**: shutdown runs on the gpui foreground, and the
    /// read thread only unblocks on master-EOF, which requires *every* slave
    /// fd to close. The reader is a dup of the master (portable-pty
    /// `try_clone_reader`), so nothing we drop here can force it — and an
    /// orphaned grandchild that inherited the slave (`sleep 100 &` in a shell
    /// tab survives the shell's SIGKILL in its own process group) would hold
    /// it open indefinitely. Never trade that for a frozen UI: after the
    /// deadline the thread is detached — it holds only `Arc`s and exits on
    /// the eventual EOF (or process exit).
    pub fn shutdown(&mut self) {
        self.kill();
        if let Some(handle) = self.wait_thread.take() {
            join_bounded(handle, "pty-wait");
        }
        if let Some(handle) = self.read_thread.take() {
            join_bounded(handle, "pty-read");
        }
    }
}

/// Join with a deadline; detach (drop the handle) when it can't be met.
fn join_bounded(handle: JoinHandle<()>, what: &str) {
    const JOIN_TIMEOUT: Duration = Duration::from_secs(1);
    let deadline = Instant::now() + JOIN_TIMEOUT;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            log::warn!(
                "terminal shutdown: {what} thread still blocked after \
                 {JOIN_TIMEOUT:?}; detaching (a grandchild likely holds the \
                 pty slave open)"
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = handle.join();
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}
