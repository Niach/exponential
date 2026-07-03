// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! THE STEER TEE (masterplan-v3 §6.4) — one blocking read thread, software
//! fan-out.
//!
//! This module is greenfield: Zed's alacritty event loop keeps the byte
//! stream private, so there is no analog. Because we own the PTY master
//! (§6.3), the child's raw output has **exactly one reader** — this loop —
//! and the fan-out to the steer publisher (§08) happens in software, inside
//! this one thread, after the single `read()`. Never clone a second reader:
//! two concurrent blocking reads race and split the stream.

use crate::emulator::TermHandle;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use vte::ansi::{Processor, StdSyncHandler};

/// Anything that wants the raw child bytes besides the emulator — the steer
/// publisher (§08) is the intended consumer. `on_output` is called on the
/// read thread while the sink registry lock is held: implementations MUST be
/// cheap and non-blocking (e.g. a channel `try_send`).
///
/// The whole seam between `terminal` and `steer` (§6.14) is:
/// `RawSink::on_output` (out), `Pty::writer_write` (in), `Pty::resize`.
pub trait RawSink: Send + Sync {
    fn on_output(&self, chunk: &[u8]);
}

/// Attach/detach registry for [`RawSink`]s (§6.14's setter): the steer
/// publisher attaches when a room is claimed and detaches when it closes.
/// Cloneable — all clones share the same registry.
#[derive(Clone, Default)]
pub struct SinkSet {
    inner: Arc<Mutex<Vec<Arc<dyn RawSink>>>>,
}

impl SinkSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach(&self, sink: Arc<dyn RawSink>) {
        if let Ok(mut sinks) = self.inner.lock() {
            sinks.push(sink);
        }
    }

    /// Detach by identity (`Arc::ptr_eq`).
    pub fn detach(&self, sink: &Arc<dyn RawSink>) {
        if let Ok(mut sinks) = self.inner.lock() {
            sinks.retain(|s| !Arc::ptr_eq(s, sink));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().map(|sinks| sinks.is_empty()).unwrap_or(true)
    }

    fn fan_out(&self, chunk: &[u8]) {
        if let Ok(sinks) = self.inner.lock() {
            for sink in sinks.iter() {
                sink.on_output(chunk);
            }
        }
    }
}

/// A ready-made capturing sink: the "stub relay consumer" the Phase-4 gate
/// requires (§6.16 #8) and the test double for the §08 publisher.
#[derive(Default)]
pub struct CaptureSink(Mutex<Vec<u8>>);

impl CaptureSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.0.lock().map(|b| b.clone()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.0.lock().map(|b| b.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl RawSink for CaptureSink {
    fn on_output(&self, chunk: &[u8]) {
        if let Ok(mut bytes) = self.0.lock() {
            bytes.extend_from_slice(chunk);
        }
    }
}

/// Wake signals from the PTY threads to the foreground, drained by one
/// `cx.spawn` task in the gpui layer (coalesce bursts: several queued
/// `Output`s need only one `notify()`). Plain std threads can't touch gpui
/// entities (`!Send`) — this channel is the bridge (§6.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// New bytes were advanced into the emulator — repaint.
    Output,
    /// The read loop hit EOF: the child side of the PTY closed (§6.3).
    Eof,
    /// The wait thread reaped the child — the captured [`crate::ChildExit`]
    /// is in the session's exit slot (§6.7). EOF and ChildExited normally
    /// coincide, but a double-forking child can close the PTY before the
    /// tracked pid exits, so both edges are surfaced and §07 dedupes.
    ChildExited,
}

/// Spawn the single reader thread (§6.4): blocking `read()` → (a) tee the raw
/// chunk to the sinks FIRST, (b) feed the emulator under the `FairMutex` —
/// held ONLY around `advance`, (c) send a wake outward.
pub fn spawn_read_loop(
    mut reader: Box<dyn Read + Send>,
    term: TermHandle,
    sinks: SinkSet,
    wake: flume::Sender<Wake>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("pty-read".into())
        .spawn(move || {
            // Turbofish REQUIRED: the `T: Timeout = StdSyncHandler` default
            // type param does not participate in fn-call inference (E0283).
            // One long-lived Processor: an escape sequence can straddle a
            // read() boundary, so the partial-parse state must persist.
            let mut processor = Processor::<StdSyncHandler>::new();
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    // EOF: child exited (we dropped the slave at open, §6.3).
                    Ok(0) => break,
                    // macOS reports EIO on the master after child exit —
                    // treat any error as end-of-stream.
                    Err(e) => {
                        log::debug!("pty read ended: {e}");
                        break;
                    }
                    Ok(n) => {
                        let chunk = &buf[..n];
                        // (a) steer tee first — cheap, non-blocking send.
                        sinks.fan_out(chunk);
                        // (b) emulator under the Term lock — held ONLY here,
                        // never across read() or the sink send.
                        {
                            let mut term = term.lock();
                            processor.advance(&mut *term, chunk);
                        }
                        // (c) wake the foreground to repaint. No `\n`→`\r\n`
                        // fixup anywhere: the PTY's ONLCR line discipline
                        // already emitted `\r\n` (§6.4).
                        let _ = wake.try_send(Wake::Output);
                    }
                }
            }
            let _ = wake.try_send(Wake::Eof);
        })
        .expect("spawn pty-read thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::Emulator;

    #[test]
    fn read_loop_tees_and_feeds_emulator_without_a_pty() {
        let emulator = Emulator::new(40, 5);
        let sinks = SinkSet::new();
        let capture = Arc::new(CaptureSink::new());
        sinks.attach(capture.clone());

        let payload = b"tee me\r\nsecond line".to_vec();
        let reader: Box<dyn Read + Send> = Box::new(std::io::Cursor::new(payload.clone()));
        let (wake_tx, wake_rx) = flume::unbounded();

        let handle = spawn_read_loop(reader, emulator.term(), sinks, wake_tx);
        handle.join().expect("read loop join");

        // Tee saw every raw byte the loop consumed, unmodified.
        assert_eq!(capture.bytes(), payload);
        // The emulator was fed from the SAME single read.
        let lines = emulator.screen_lines();
        assert_eq!(lines[0], "tee me");
        assert_eq!(lines[1], "second line");
        // Wake ordering: at least one Output, then a final Eof.
        let wakes: Vec<Wake> = wake_rx.drain().collect();
        assert!(wakes.contains(&Wake::Output));
        assert_eq!(*wakes.last().expect("at least one wake"), Wake::Eof);
    }

    #[test]
    fn sink_set_attach_detach() {
        let sinks = SinkSet::new();
        assert!(sinks.is_empty());
        let a_capture = Arc::new(CaptureSink::new());
        let b_capture = Arc::new(CaptureSink::new());
        let a: Arc<dyn RawSink> = a_capture.clone();
        let b: Arc<dyn RawSink> = b_capture.clone();
        sinks.attach(a.clone());
        sinks.attach(b.clone());
        sinks.fan_out(b"x");
        sinks.detach(&a);
        sinks.fan_out(b"y");
        assert_eq!(a_capture.bytes(), b"x"); // detached before "y"
        assert_eq!(b_capture.bytes(), b"xy");
        assert!(!sinks.is_empty());
        sinks.detach(&b);
        assert!(sinks.is_empty());
    }
}
