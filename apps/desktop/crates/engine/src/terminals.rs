//! EXP-750 — the ACP `terminal/*` client capability.
//!
//! An agent that wants a LIVE command runs it through us: `terminal/create`
//! spawns it on a PTY we own, `terminal/output` reads the retained buffer,
//! `terminal/wait_for_exit` resolves once the child is reaped AND its output
//! is drained, and the tool call that embeds the terminal
//! (`ToolCallContent::Terminal`) hangs a card off the feed row that streams
//! while the command runs.
//!
//! Three rules hold the whole module up:
//!
//! - **Output is LOCAL.** Every byte here becomes a
//!   [`LocalFeedEvent::Output`] and nothing else. The relay's vocabulary has
//!   no room for a command's stdout, and a terminal is exactly where a
//!   secret-bearing command line would land on the wire if it ever rode one.
//! - **Nothing is emitted before the terminal is BOUND.** The agent creates
//!   the terminal first and publishes the tool call that embeds it after, so
//!   a chunk forwarded in between would key on an id no renderer knows. The
//!   buffer accumulates instead, and [`Terminals::bind`] flushes it as ONE
//!   chunk — so nothing is lost and the UI never sees an unbound key.
//! - **The registry outlives nothing.** `kill_all` runs at both end-of-run
//!   sites and on `Drop`: an agent that dies mid-command must not leave its
//!   child behind — SIGHUP first, SIGKILL to the child's group after a
//!   grace, because a HUP-ignoring server would otherwise outlive the run.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use terminal::pty::{ChildExit, Pty, SpawnSpec};

use crate::local::LocalFeedEvent;

/// The PTY a `terminal/create` command runs on. Wide and short on purpose: a
/// terminal here is a scrollback buffer, not a grid anyone paints, so the
/// only thing the geometry decides is where a TUI-aware command wraps.
const TERMINAL_COLS: u16 = 200;
const TERMINAL_ROWS: u16 = 50;

/// What we retain when the agent names no `outputByteLimit`. The ACP default
/// is "the client decides", and an unbounded buffer of a `bun install` is
/// megabytes of nothing kept for the whole session.
const DEFAULT_OUTPUT_BYTE_LIMIT: usize = 1 << 20;

/// How long the reaped child's exit waits for the reader thread to hit EOF
/// before it resolves anyway. On Linux `waitpid` routinely returns with tens
/// of kilobytes still sitting in the master, so publishing the exit right
/// there truncates what `terminal/output` answers and puts the exit code in
/// FRONT of the last chunks on the feed. The bound is the escape hatch for a
/// master that never EOFs (a grandchild still holding the slave open).
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// How long a killed child gets to die of SIGHUP before it is SIGKILLed.
const KILL_GRACE: Duration = Duration::from_millis(500);

/// The escalation thread's poll step while it waits out [`KILL_GRACE`].
const KILL_POLL: Duration = Duration::from_millis(10);

/// Turns the PTY's byte stream into text ACROSS reads. A `read` boundary
/// lands wherever the kernel put it, so both of these straddle it routinely
/// on any chatty command:
///
/// - a multibyte codepoint (`━`, `✓`, `é`) — decoded per read, its halves
///   become two U+FFFD in the buffer the agent reads back verbatim;
/// - the line discipline's `\r\n` — split, the `\r` stays in the buffer.
///
/// So the incomplete tail and a trailing `\r` are HELD until the next read
/// decides what they are. [`ChunkDecoder::flush`] releases whatever the EOF
/// left over.
#[derive(Default)]
struct ChunkDecoder {
    /// The tail bytes of a codepoint the next read completes.
    carry: Vec<u8>,
    /// A `\r` at the end of a read: dropped if the next read starts with
    /// `\n` (the pair is one newline), emitted as itself otherwise.
    pending_cr: bool,
}

impl ChunkDecoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        let mut input = std::mem::take(&mut self.carry);
        input.extend_from_slice(bytes);
        let mut text = String::with_capacity(input.len() + 1);
        let mut rest: &[u8] = &input;
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    rest = &rest[rest.len()..];
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    text.push_str(std::str::from_utf8(&rest[..valid_up_to]).unwrap_or_default());
                    match err.error_len() {
                        // Genuinely invalid bytes — exactly what
                        // `from_utf8_lossy` would have made of them.
                        Some(len) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            rest = &rest[valid_up_to + len..];
                        }
                        // Truncated: the next read finishes this codepoint.
                        None => {
                            rest = &rest[valid_up_to..];
                            break;
                        }
                    }
                }
            }
        }
        self.carry.extend_from_slice(rest);
        self.newlines(text)
    }

    /// EOF: hand back the `\r` we were holding and a replacement character
    /// for a codepoint the child never finished writing.
    fn flush(&mut self) -> String {
        let mut text = String::new();
        if self.pending_cr {
            self.pending_cr = false;
            text.push('\r');
        }
        if !std::mem::take(&mut self.carry).is_empty() {
            text.push(char::REPLACEMENT_CHARACTER);
        }
        text
    }

    /// The line discipline turns every `\n` into `\r\n`; a scrollback card
    /// wants neither the carriage return nor a phantom blank line.
    fn newlines(&mut self, decoded: String) -> String {
        let mut text = String::with_capacity(decoded.len() + 1);
        if self.pending_cr {
            self.pending_cr = false;
            // Held over from the last read: only a `\n` right here makes it
            // half of a pair.
            if !decoded.starts_with('\n') {
                text.push('\r');
            }
        }
        text.push_str(&decoded);
        if text.ends_with('\r') {
            text.pop();
            self.pending_cr = true;
        }
        text.replace("\r\n", "\n")
    }
}

/// Where a terminal's chunks go: the session's local feed. Built by the
/// caller (from a `Weak<SessionCtx>`, so a live terminal never keeps the
/// session it belongs to alive).
pub(crate) type TerminalSink = Arc<dyn Fn(LocalFeedEvent) + Send + Sync>;

/// The retained output of one terminal — the ACP contract's own shape:
/// append, and once over `limit` drop from the START at a **char boundary**
/// (a buffer cut mid-codepoint is not a valid `String` and the agent reads
/// this back verbatim).
#[derive(Debug)]
pub(crate) struct OutputBuffer {
    text: String,
    truncated: bool,
    limit: usize,
}

impl OutputBuffer {
    fn new(limit: Option<u64>) -> OutputBuffer {
        let limit = limit
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(DEFAULT_OUTPUT_BYTE_LIMIT);
        OutputBuffer {
            text: String::new(),
            truncated: false,
            limit,
        }
    }

    fn push(&mut self, chunk: &str) {
        self.text.push_str(chunk);
        if self.text.len() <= self.limit {
            return;
        }
        // Keep the TAIL: a command's verdict is at the end. `floor_char_
        // boundary` is unstable, so walk forward to the first boundary at or
        // after the cut — which is why the retained output may be slightly
        // SHORTER than the limit, exactly as the spec allows.
        let mut cut = self.text.len() - self.limit;
        while cut < self.text.len() && !self.text.is_char_boundary(cut) {
            cut += 1;
        }
        self.text.drain(..cut);
        self.truncated = true;
    }
}

/// One terminal's mutable half. ONE lock covers the buffer, the bound tool
/// call and the exit together: the reader thread appends and forwards under
/// it, so a `bind` flush racing an arriving chunk can neither duplicate nor
/// drop it. The sink is a channel push (or a println on the CLI) and never
/// re-enters this module, so calling it under the lock is safe.
struct TerminalState {
    buffer: OutputBuffer,
    /// The ACP tool call this terminal renders under, once the agent
    /// published the `ToolCallContent::Terminal` that names it.
    bound: Option<String>,
    exit: Option<ChildExit>,
}

struct TerminalHandle {
    state: Mutex<TerminalState>,
    /// Held until the child is reaped; dropping it is the signal
    /// [`TerminalHandle::exited`] waits on. Memoryful, like
    /// [`crate::host::ChildExitLink`]: a waiter arriving after the exit
    /// resolves immediately and keeps resolving.
    gate: Mutex<Option<flume::Sender<()>>>,
    signal: flume::Receiver<()>,
    /// Set the moment `waitpid` returned — BEFORE the drain the exit waits
    /// out, because this is what makes a pid safe to signal: once the child
    /// is reaped its number may belong to somebody else.
    reaped: Arc<AtomicBool>,
    /// Behind a lock because `Pty` is `Send` but not `Sync`, and every handle
    /// is shared with its reader and wait threads.
    pty: Mutex<Pty>,
    sink: TerminalSink,
}

impl TerminalHandle {
    /// Publish the exit: the closing feed event, then the gate. Called by the
    /// wait thread once BOTH edges landed — the child reaped and the reader
    /// at EOF — so the exit code is the last thing the feed sees and a
    /// `terminal/output` taken after `wait_for_exit` is complete.
    fn record_exit(&self, exit: ChildExit) {
        {
            let mut state = self.lock();
            if state.exit.is_none() {
                state.exit = Some(exit.clone());
                // The exit code closes the card — but only a BOUND terminal
                // has one; an unbound exit rides its `bind` flush instead.
                if let Some(tool_call_id) = state.bound.clone() {
                    (self.sink)(LocalFeedEvent::Output {
                        tool_call_id,
                        chunk: String::new(),
                        exit_code: Some(exit.code),
                    });
                }
            }
        }
        // Dropped last: a waiter woken by this must find the exit recorded.
        if let Ok(mut gate) = self.gate.lock() {
            gate.take();
        }
    }

    /// Name the tool call this terminal belongs to and flush everything that
    /// arrived before the agent published it — as ONE chunk, carrying the
    /// exit code when the command has already finished.
    fn bind(&self, tool_call_id: &str) {
        let mut state = self.lock();
        if state.bound.is_some() {
            return;
        }
        state.bound = Some(tool_call_id.to_string());
        let chunk = state.buffer.text.clone();
        let exit_code = state.exit.as_ref().map(|exit| exit.code);
        if chunk.is_empty() && exit_code.is_none() {
            return;
        }
        (self.sink)(LocalFeedEvent::Output {
            tool_call_id: tool_call_id.to_string(),
            chunk,
            exit_code,
        });
    }

    fn push(&self, chunk: &str) {
        let mut state = self.lock();
        state.buffer.push(chunk);
        if let Some(tool_call_id) = state.bound.clone() {
            (self.sink)(LocalFeedEvent::Output {
                tool_call_id,
                chunk: chunk.to_string(),
                exit_code: None,
            });
        }
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let state = self.lock();
        TerminalSnapshot {
            output: state.buffer.text.clone(),
            truncated: state.buffer.truncated,
            exit: state.exit.clone(),
        }
    }

    /// `terminal/kill`, `release`, cancel and end-of-run all land here.
    /// portable-pty's signaller sends ONE SIGHUP, which a HUP-ignoring child
    /// (a dev server, `trap '' HUP`) survives — so after a grace the child
    /// and its group get SIGKILL. Never blocks the caller: the escalation
    /// rides its own thread and signals nothing once the child is reaped.
    fn kill(&self) {
        let pid = match self.pty.lock() {
            Ok(pty) => {
                pty.kill();
                pty.process_id()
            }
            Err(_) => None,
        };
        if let Some(pid) = pid {
            if !self.reaped.load(Ordering::SeqCst) {
                escalate_kill(pid, Arc::clone(&self.reaped));
            }
        }
    }

    /// Resolves once the child is reaped and drained — immediately, if it
    /// already was.
    async fn exited(&self) {
        let _ = self.signal.recv_async().await;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TerminalState> {
        self.state.lock().unwrap_or_else(|err| err.into_inner())
    }
}

/// Block until the reader thread hit EOF — it drops its end of `edge`, and
/// nothing is ever sent on it — or `grace` expires. `true` = the master is
/// drained; `false` = the fallback fired and there may be bytes we will
/// never see (a grandchild holding the slave open).
fn await_drain(edge: &flume::Receiver<()>, grace: Duration) -> bool {
    matches!(
        edge.recv_timeout(grace),
        Err(flume::RecvTimeoutError::Disconnected)
    )
}

/// SIGKILL the child (and its group) unless it dies of the SIGHUP first.
/// portable-pty `setsid`s every child, so it leads its own group and the
/// group signal reaches what a shell spawned without touching anything else.
#[cfg(unix)]
fn escalate_kill(pid: u32, reaped: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("acp-term-kill".into())
        .spawn(move || {
            let deadline = Instant::now() + KILL_GRACE;
            while Instant::now() < deadline {
                if reaped.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(KILL_POLL);
            }
            if reaped.load(Ordering::SeqCst) {
                return;
            }
            unsafe {
                libc::killpg(pid as i32, libc::SIGKILL);
                libc::kill(pid as i32, libc::SIGKILL);
            }
        });
}

/// Windows has no SIGHUP to ignore: portable-pty's killer is already a
/// `TerminateProcess`.
#[cfg(not(unix))]
fn escalate_kill(_pid: u32, _reaped: Arc<AtomicBool>) {}

/// What `terminal/output` answers with.
pub(crate) struct TerminalSnapshot {
    pub(crate) output: String,
    pub(crate) truncated: bool,
    pub(crate) exit: Option<ChildExit>,
}

/// Every terminal this session created, owned by its `SessionCtx`.
#[derive(Default)]
pub(crate) struct Terminals {
    live: Mutex<HashMap<String, Arc<TerminalHandle>>>,
    next: AtomicU64,
}

impl Terminals {
    /// Spawn `spec` on a PTY and start streaming it. The returned id is what
    /// the agent embeds in its tool call; nothing reaches the feed until
    /// [`Terminals::bind`] names that call.
    pub(crate) fn create(
        &self,
        spec: &SpawnSpec,
        output_byte_limit: Option<u64>,
        sink: TerminalSink,
    ) -> anyhow::Result<String> {
        let mut pty = terminal::pty::open(spec, TERMINAL_COLS, TERMINAL_ROWS)?;
        let mut reader = pty.take_reader();
        let (wake, woken) = flume::unbounded();
        let (exit_slot, _wait) = pty.spawn_wait_thread(wake)?;
        let id = format!("term-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let (gate, signal) = flume::bounded(0);
        // Nothing is ever SENT on this: the reader thread DROPPING it is the
        // EOF edge, so a panicking reader releases the exit too.
        let (drained, drain_edge) = flume::bounded::<()>(0);
        let reaped = Arc::new(AtomicBool::new(false));
        let handle = Arc::new(TerminalHandle {
            state: Mutex::new(TerminalState {
                buffer: OutputBuffer::new(output_byte_limit),
                bound: None,
                exit: None,
            }),
            gate: Mutex::new(Some(gate)),
            signal,
            reaped: Arc::clone(&reaped),
            pty: Mutex::new(pty),
            sink,
        });

        let reading = Arc::clone(&handle);
        let name = format!("acp-term-{id}");
        std::thread::Builder::new()
            .name(name.clone())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                let mut decoder = ChunkDecoder::default();
                loop {
                    match reader.read(&mut buf) {
                        // EOF, or the master's EIO once the child side closed.
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            let chunk = decoder.decode(&buf[..read]);
                            // Empty when the read was nothing but a held `\r`
                            // or half a codepoint — no row for that.
                            if !chunk.is_empty() {
                                reading.push(&chunk);
                            }
                        }
                    }
                }
                let tail = decoder.flush();
                if !tail.is_empty() {
                    reading.push(&tail);
                }
                // Last: the exit below is published only after this drop, so
                // no chunk can land behind the exit code.
                drop(drained);
            })
            .map_err(|err| anyhow::anyhow!("spawn {name}: {err}"))?;

        let waiting = Arc::clone(&handle);
        std::thread::Builder::new()
            .name(format!("acp-term-wait-{id}"))
            .spawn(move || {
                // The wake channel is the pty wait thread's own edge; a send
                // error means it went away without one, and the slot then
                // answers for it.
                let _ = woken.recv();
                let exit = exit_slot
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone())
                    .unwrap_or(ChildExit {
                        code: -1,
                        success: false,
                        signal: None,
                    });
                // Reaped: the pid is no longer ours to signal.
                reaped.store(true, Ordering::SeqCst);
                // …but the master may still hold what the child wrote right
                // before exiting, so the exit resolves on the LATER of the
                // two edges (bounded by DRAIN_GRACE).
                await_drain(&drain_edge, DRAIN_GRACE);
                waiting.record_exit(exit);
            })
            .map_err(|err| anyhow::anyhow!("spawn acp-term-wait-{id}: {err}"))?;

        self.insert(id.clone(), handle);
        Ok(id)
    }

    /// The `ToolCallContent::Terminal` edge: this terminal renders under
    /// `tool_call_id` from now on, and everything buffered so far flushes.
    pub(crate) fn bind(&self, terminal_id: &str, tool_call_id: &str) {
        if let Some(handle) = self.get(terminal_id) {
            handle.bind(tool_call_id);
        }
    }

    pub(crate) fn snapshot(&self, terminal_id: &str) -> Option<TerminalSnapshot> {
        Some(self.get(terminal_id)?.snapshot())
    }

    /// `terminal/wait_for_exit`. `None` = no such terminal.
    pub(crate) async fn wait(&self, terminal_id: &str) -> Option<ChildExit> {
        let handle = self.get(terminal_id)?;
        handle.exited().await;
        Some(handle.snapshot().exit.unwrap_or(ChildExit {
            code: -1,
            success: false,
            signal: None,
        }))
    }

    /// `terminal/kill` — the child dies, the terminal stays readable.
    pub(crate) fn kill(&self, terminal_id: &str) -> bool {
        match self.get(terminal_id) {
            Some(handle) => {
                handle.kill();
                true
            }
            None => false,
        }
    }

    /// `terminal/release` — kill whatever is still running, then forget it.
    pub(crate) fn release(&self, terminal_id: &str) -> bool {
        let handle = self
            .live
            .lock()
            .ok()
            .and_then(|mut live| live.remove(terminal_id));
        match handle {
            Some(handle) => {
                handle.kill();
                true
            }
            None => false,
        }
    }

    /// End of run (and a cancelled turn): nothing an agent spawned keeps
    /// running. The rows STAY — a killed terminal is still readable, and an
    /// agent that releases one after the cancel must not get an error for
    /// it; the whole registry goes away with the session.
    pub(crate) fn kill_all(&self) {
        let handles: Vec<Arc<TerminalHandle>> = match self.live.lock() {
            Ok(live) => live.values().cloned().collect(),
            Err(_) => Vec::new(),
        };
        for handle in handles {
            handle.kill();
        }
    }

    fn get(&self, terminal_id: &str) -> Option<Arc<TerminalHandle>> {
        self.live.lock().ok()?.get(terminal_id).cloned()
    }

    fn insert(&self, id: String, handle: Arc<TerminalHandle>) {
        if let Ok(mut live) = self.live.lock() {
            live.insert(id, handle);
        }
    }
}

impl Drop for Terminals {
    fn drop(&mut self) {
        self.kill_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn recording() -> (TerminalSink, Arc<Mutex<Vec<LocalFeedEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let sink: TerminalSink = Arc::new(move |event| {
            sink_events
                .lock()
                .expect("the event log is not poisoned")
                .push(event);
        });
        (sink, events)
    }

    fn until(what: &str, mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if check() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    /// `wait` is the module's only async surface; a current-thread runtime
    /// per test is cheaper than threading one through the fixtures.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a test runtime")
            .block_on(future)
    }

    /// The whole round trip on a real PTY: the chunks stream into the feed
    /// once bound, and the exit code closes the card.
    #[cfg(unix)]
    #[test]
    fn a_terminal_streams_output_and_reports_its_exit() {
        let terminals = Terminals::default();
        let (sink, events) = recording();
        let id = terminals
            .create(
                &SpawnSpec::new("sh")
                    .arg("-c")
                    .arg("printf a; printf b; exit 3"),
                None,
                sink,
            )
            .expect("the terminal spawns");
        terminals.bind(&id, "tc-1");

        let exit = block_on(terminals.wait(&id)).expect("the terminal is known");
        assert_eq!(exit.code, 3);
        let snapshot = terminals.snapshot(&id).expect("the terminal is known");
        assert_eq!(snapshot.output, "ab");
        assert!(!snapshot.truncated);
        assert_eq!(snapshot.exit.map(|exit| exit.code), Some(3));

        // Everything the feed saw belongs to the bound call, and the last
        // event carries the exit code.
        let events = events.lock().expect("the event log is not poisoned");
        let outputs: Vec<(String, String, Option<i32>)> = events
            .iter()
            .map(|event| match event {
                LocalFeedEvent::Output {
                    tool_call_id,
                    chunk,
                    exit_code,
                } => (tool_call_id.clone(), chunk.clone(), *exit_code),
                other => panic!("a terminal emits Output only, got {other:?}"),
            })
            .collect();
        assert!(outputs.iter().all(|(id, ..)| id == "tc-1"));
        assert_eq!(
            outputs.iter().map(|(_, chunk, _)| chunk.as_str()).collect::<String>(),
            "ab"
        );
        assert_eq!(outputs.last().map(|(.., code)| *code), Some(Some(3)));
    }

    /// The bug this guards (review C1): `waitpid` returns while the master
    /// still holds what the child wrote right before exiting, so the exit
    /// used to resolve over an undrained buffer — the agent's follow-up
    /// `terminal/output` read a truncated summary and the feed got the exit
    /// code BEFORE the last chunks (CLI prints `(exit 0)`, then more output;
    /// the card is closed while lines keep appending).
    #[cfg(unix)]
    #[test]
    fn the_exit_waits_for_the_last_chunk_the_child_wrote() {
        let terminals = Terminals::default();
        let (sink, events) = recording();
        // Well over one 8 KB read, written in one go right before the exit.
        let payload: String = (0..200).map(|line| format!("summary line {line:0>48}\n")).collect();
        assert!(payload.len() > 8192);
        let id = terminals
            .create(
                // `$0` carries the payload, so nothing has to survive
                // quoting.
                &SpawnSpec::new("sh")
                    .arg("-c")
                    .arg("printf %s \"$0\"; exit 3")
                    .arg(&payload),
                None,
                sink,
            )
            .expect("the terminal spawns");
        terminals.bind(&id, "tc-1");

        let exit = block_on(terminals.wait(&id)).expect("the terminal is known");
        assert_eq!(exit.code, 3);
        // No polling: what `terminal/output` answers right after the wait is
        // already the whole thing. (macOS drains the master when the session
        // leader exits, so this only LOSES on Linux — where the reviewer's
        // probe left ~3.5 KB unread in 8 of 300 runs; the drain edge itself
        // is locked deterministically by the unit test below.)
        let snapshot = terminals.snapshot(&id).expect("the terminal is known");
        assert_eq!(snapshot.output, payload);

        let events = events.lock().expect("the event log is not poisoned");
        let chunks: Vec<(String, Option<i32>)> = events
            .iter()
            .map(|event| match event {
                LocalFeedEvent::Output { chunk, exit_code, .. } => (chunk.clone(), *exit_code),
                other => panic!("a terminal emits Output only, got {other:?}"),
            })
            .collect();
        assert_eq!(
            chunks.iter().map(|(chunk, _)| chunk.as_str()).collect::<String>(),
            payload
        );
        // The exit code is the LAST event, and the only one carrying one.
        assert_eq!(chunks.last().map(|(_, code)| *code), Some(Some(3)));
        assert!(chunks[..chunks.len() - 1].iter().all(|(_, code)| code.is_none()));
    }

    /// The bug this guards (review C2): a PTY read boundary lands mid-
    /// codepoint on any wide-character output, and decoding each read on its
    /// own turned both halves into U+FFFD in the buffer the agent reads back.
    #[cfg(unix)]
    #[test]
    fn a_multibyte_stream_crossing_read_boundaries_is_not_mangled() {
        let terminals = Terminals::default();
        let (sink, _events) = recording();
        // 12 KB of a 3-byte codepoint: every 8192-byte read ends mid-`━`.
        let payload = "━".repeat(4000);
        let id = terminals
            .create(
                &SpawnSpec::new("sh")
                    .arg("-c")
                    .arg("printf %s \"$0\"")
                    .arg(&payload),
                None,
                sink,
            )
            .expect("the terminal spawns");

        let _ = block_on(terminals.wait(&id));
        let snapshot = terminals.snapshot(&id).expect("the terminal is known");
        assert!(
            !snapshot.output.contains(char::REPLACEMENT_CHARACTER),
            "no codepoint was split into replacement characters"
        );
        assert_eq!(snapshot.output, payload);
    }

    /// The bug this guards (review C3): portable-pty's signaller sends ONE
    /// SIGHUP, so a child that ignores it (any HUP-reload server) survived
    /// `terminal/kill`, the cancel and the end of the run — `wait_for_exit`
    /// never resolved and the PTY leaked for the process lifetime.
    #[cfg(unix)]
    #[test]
    fn a_hup_ignoring_child_is_killed_after_the_grace() {
        let terminals = Terminals::default();
        let (sink, _events) = recording();
        let id = terminals
            .create(
                &SpawnSpec::new("sh")
                    .arg("-c")
                    .arg("trap '' HUP; printf ready; sleep 30"),
                None,
                sink,
            )
            .expect("the terminal spawns");
        let handle = terminals.get(&id).expect("the terminal is known");
        // Only kill once the trap is actually installed.
        until("the trap", || {
            terminals
                .snapshot(&id)
                .is_some_and(|snapshot| snapshot.output.contains("ready"))
        });

        assert!(terminals.kill(&id), "kill finds the terminal");
        let started = Instant::now();
        block_on(handle.exited());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the HUP-ignoring child was escalated to SIGKILL, not waited out"
        );
        assert!(handle.snapshot().exit.is_some(), "the exit is recorded");
    }

    /// The exit's second edge, deterministically: it waits for the reader
    /// thread's EOF (the PTY test above only catches the race when the
    /// kernel loses it), and the grace is what keeps a master that never
    /// EOFs from wedging `wait_for_exit` forever.
    #[test]
    fn the_exit_waits_for_the_readers_eof_but_not_forever() {
        let (drained, edge) = flume::bounded::<()>(0);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(drained);
        });
        let started = Instant::now();
        assert!(await_drain(&edge, Duration::from_secs(5)), "the EOF edge");
        assert!(started.elapsed() >= Duration::from_millis(50));

        // A reader that never finishes: the exit resolves on the grace.
        let (_never, edge) = flume::bounded::<()>(0);
        let started = Instant::now();
        assert!(!await_drain(&edge, Duration::from_millis(50)), "the grace");
        assert!(started.elapsed() >= Duration::from_millis(50));
    }

    /// A codepoint the read boundary cut in half is held, not replaced.
    #[test]
    fn a_codepoint_split_across_reads_survives() {
        let mut decoder = ChunkDecoder::default();
        let heavy = "━".as_bytes();
        assert_eq!(decoder.decode(&heavy[..2]), "");
        assert_eq!(decoder.decode(&[heavy[2], b'x']), "━x");
        assert_eq!(decoder.flush(), "");
    }

    /// A `\r` is held until the next read says whether it was half of a pair.
    #[test]
    fn a_crlf_split_across_reads_becomes_one_newline() {
        let mut decoder = ChunkDecoder::default();
        assert_eq!(decoder.decode(b"a\r"), "a");
        assert_eq!(decoder.decode(b"\nb"), "\nb");
        // A carriage return the child meant (a progress line) still lands.
        assert_eq!(decoder.decode(b"c\r"), "c");
        assert_eq!(decoder.decode(b"d"), "\rd");
        // …as does one the EOF caught.
        assert_eq!(decoder.decode(b"e\r"), "e");
        assert_eq!(decoder.flush(), "\r");
    }

    /// Bytes that are not UTF-8 at all still decode the way
    /// `from_utf8_lossy` did, and a tail the child never finished writing
    /// becomes one replacement character at EOF.
    #[test]
    fn invalid_bytes_still_decode_lossily() {
        let mut decoder = ChunkDecoder::default();
        assert_eq!(decoder.decode(b"a\xffb"), "a\u{fffd}b");
        assert_eq!(decoder.decode(&[0xe2, 0x94]), "");
        assert_eq!(decoder.flush(), "\u{fffd}");
    }

    /// The ACP truncation contract: drop from the START, never mid-codepoint
    /// — `é` is two bytes, so a 3-byte window over `héllo` keeps `llo`.
    #[cfg(unix)]
    #[test]
    fn output_is_truncated_from_the_start_at_a_char_boundary() {
        let terminals = Terminals::default();
        let (sink, _events) = recording();
        let id = terminals
            .create(
                &SpawnSpec::new("sh").arg("-c").arg("printf 'héllo'"),
                Some(3),
                sink,
            )
            .expect("the terminal spawns");
        let _ = block_on(terminals.wait(&id));
        until("the truncated output", || {
            terminals
                .snapshot(&id)
                .is_some_and(|snapshot| snapshot.truncated)
        });
        let snapshot = terminals.snapshot(&id).expect("the terminal is known");
        assert_eq!(snapshot.output, "llo");
    }

    /// Releasing a terminal kills whatever it is still running — an agent
    /// that walks away must not leave a `sleep 30` behind.
    #[cfg(unix)]
    #[test]
    fn release_kills_a_running_child() {
        let terminals = Terminals::default();
        let (sink, _events) = recording();
        let id = terminals
            .create(&SpawnSpec::new("sh").arg("-c").arg("sleep 30"), None, sink)
            .expect("the terminal spawns");
        let handle = terminals.get(&id).expect("the terminal is known");
        assert!(terminals.release(&id), "release finds the terminal");
        // Gone from the registry, and the child is reaped rather than left
        // running for the next half minute.
        assert!(terminals.snapshot(&id).is_none());
        let started = Instant::now();
        block_on(handle.exited());
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the killed child was reaped promptly"
        );
    }

    #[test]
    fn an_unknown_terminal_answers_nothing() {
        let terminals = Terminals::default();
        assert!(terminals.snapshot("term-404").is_none());
        assert!(!terminals.kill("term-404"));
        assert!(!terminals.release("term-404"));
        assert!(block_on(terminals.wait("term-404")).is_none());
    }
}
