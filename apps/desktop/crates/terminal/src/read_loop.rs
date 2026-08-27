// Clean reimplementation from the VT spec + rio-vt (MIT). NOT derived from Zed's GPL terminal crates.
//! The child-output read loop (masterplan-v3 §6.4) — one blocking read thread.
//!
//! This module is greenfield: Zed's alacritty event loop keeps the byte stream
//! private, so there is no analog. Because we own the PTY master (§6.3), the
//! child's raw output has **exactly one reader** — this loop. Never clone a
//! second reader: two concurrent blocking reads race and split the stream.
//!
//! It used to fan the raw bytes out to the steer publisher as well (the "steer
//! tee"). EXP-249 removed the binary PTY mirror — remote clients see the
//! scrubbed activity stream, never terminal bytes — so the emulator is now the
//! only consumer.

use crate::emulator::TermHandle;
use std::io::Read;
use std::thread::JoinHandle;
use rio_vt::performer::handler::Processor;

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

/// Spawn the single reader thread (§6.4): blocking `read()` → (a) feed the
/// emulator under the `FairMutex` — held ONLY around `advance` — then (b) send
/// a wake outward.
pub fn spawn_read_loop(
    mut reader: Box<dyn Read + Send>,
    term: TermHandle,
    wake: flume::Sender<Wake>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("pty-read".into())
        .spawn(move || {
            // One long-lived Processor: an escape sequence can straddle a
            // read() boundary, so the partial-parse state must persist.
            let mut processor = Processor::default();
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
                        // (a) emulator under the Term lock — held ONLY here,
                        // never across read().
                        {
                            let mut term = term.lock();
                            processor.advance(&mut *term, chunk);
                        }
                        // (b) wake the foreground to repaint. No `\n`→`\r\n`
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
    fn read_loop_feeds_the_emulator_without_a_pty() {
        let emulator = Emulator::new(40, 5);
        let payload = b"read me\r\nsecond line".to_vec();
        let reader: Box<dyn Read + Send> = Box::new(std::io::Cursor::new(payload));
        let (wake_tx, wake_rx) = flume::unbounded();

        let handle = spawn_read_loop(reader, emulator.term(), wake_tx);
        handle.join().expect("read loop join");

        let lines = emulator.screen_lines();
        assert_eq!(lines[0], "read me");
        assert_eq!(lines[1], "second line");
        // Wake ordering: at least one Output, then a final Eof.
        let wakes: Vec<Wake> = wake_rx.drain().collect();
        assert!(wakes.contains(&Wake::Output));
        assert_eq!(*wakes.last().expect("at least one wake"), Wake::Eof);
    }
}
