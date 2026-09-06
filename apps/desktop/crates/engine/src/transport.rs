//! EXP-746 — the child-process line transport every stdio adapter rides.
//!
//! One agent CLI reachable as LF-delimited JSON lines: a `std::process`
//! child in its OWN process group, one reader thread, one writer behind a
//! mutex, one wait thread, and a kill-on-drop guard. Deliberately NOT
//! `tokio::process`: the ACP graph's own I/O is `async-io`/`async-process`
//! and the engine already owns a plain OS thread per session, so a second
//! async I/O stack would buy nothing (D1).
//!
//! Landed in P0 with real bodies (lane S1 depends on it for the spike).

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use terminal::process::background_command;
use terminal::pty::{login_path, ChildExit, SpawnSpec};

use crate::host::ChildExitLink;

/// What to do with the child's stderr. Agent CLIs log diagnostics there and
/// a wedged one can produce a lot of it, so nothing is ever buffered for
/// replay: it is either dropped at the OS level or logged line by line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StderrPolicy {
    /// `Stdio::null()` — the child writes into the void.
    Drop,
    /// Piped and forwarded to `log::warn!` by a reader thread.
    Log,
}

/// How many stdout lines may sit unread before the reader thread starts
/// dropping them (EXP-758). The channel used to be UNBOUNDED: an adapter
/// that stopped draining (a wedged turn, a dispatch loop blocked on a
/// handler) let a chatty CLI grow it until the host ran out of memory. 4096
/// lines is far more backlog than any live session builds up, so a drop here
/// means the session is already broken.
pub const STDOUT_LINES_CAP: usize = 4096;

/// One warning per this many dropped lines: a flooding child would otherwise
/// turn the log into the same unbounded buffer the channel just stopped
/// being.
const DROP_LOG_EVERY: u64 = 1000;

/// How long [`ChildLines::terminate`] waits for a SIGTERM'd child to reap
/// itself before the guard resorts to SIGKILL (EXP-758).
///
/// The end sequence used to be SIGKILL and nothing else: claude never ran its
/// session-end hooks and codex tools died mid-write. Now the child first sees
/// its stdin close (EOF is what every agent CLI shuts down on) and then a
/// SIGTERM, and only a child that ignores both is killed. Deliberately
/// shorter than [`crate::host::CHILD_EXIT_GRACE`]-scale patience: this one
/// blocks the thread that is tearing the run down.
pub const CHILD_TERM_GRACE: Duration = Duration::from_millis(1500);

/// How often the terminate wait re-checks the reaped flag.
const TERM_POLL: Duration = Duration::from_millis(10);

/// A live agent child: its stdout as lines, its stdin as a line writer, and
/// the one-shot exit. Dropping this ENDS the child (and its process group on
/// unix) unless it already exited: codex/pi/external ACP children carry no
/// `claude-hooks` reaper anchor, so this drop is what keeps them from
/// escaping (EXP-300). EXP-758: the drop asks first (stdin EOF, SIGTERM,
/// [`CHILD_TERM_GRACE`]) and only then kills.
pub struct ChildLines {
    /// One element per line of stdout, `\n`-delimited, trailing `\r` stripped.
    /// Bounded at [`STDOUT_LINES_CAP`]. The sender is dropped on EOF, so a
    /// disconnected receiver IS the "child closed stdout" edge.
    pub lines: flume::Receiver<String>,
    pub writer: LineWriter,
    /// Fires exactly once, after `wait()` reaped the child.
    pub exit: flume::Receiver<ChildExit>,
    /// The child's pid — log lines and the reaper's protection checks.
    pub pid: u32,
    guard: ChildGuard,
}

impl ChildLines {
    /// Forward the one-shot exit into the engine's [`ChildExitLink`], so the
    /// end sequence publishes `exit:<code>` as the bye outcome instead of a
    /// plain `ended`. The forwarder is the ONE consumer of `exit` (flume hands
    /// each message to a single receiver): an adapter that wants the code too
    /// reads the link, never a second clone of the receiver.
    pub fn forward_exit(&self, link: &ChildExitLink) {
        link.record_pid(self.pid);
        forward_exit(self.exit.clone(), link.clone());
    }

    /// EXP-758: ask the child to stop and wait up to [`CHILD_TERM_GRACE`] for
    /// it: stdin closes (EOF), then SIGTERM to the process group, so the CLI
    /// runs its own shutdown (claude's session-end hooks, codex flushing a
    /// half-written tool result) instead of dying mid-syscall.
    ///
    /// Idempotent, and the guard's drop runs it anyway, so an adapter that
    /// wants the grace to overlap its OWN teardown calls this early and an
    /// adapter that does nothing still gets it. Returns once the child is
    /// reaped or the grace is up; the caller's drop is what escalates.
    pub fn terminate(&self) {
        self.guard.terminate();
    }
}

/// [`ChildLines::forward_exit`] for a receiver already split off its
/// `ChildLines` (the codex router owns that child whole).
pub fn forward_exit(exit: flume::Receiver<ChildExit>, link: ChildExitLink) {
    let _ = std::thread::Builder::new()
        .name("exit-forward".to_string())
        .spawn(move || {
            if let Ok(exit) = exit.recv() {
                link.record(exit);
            }
        });
}

/// Clonable, mutex-serialized stdin. An adapter writes from the connection
/// actor AND from spawned turn tasks, so the mutex is load-bearing: two
/// interleaved half-lines would be unparseable JSON at the other end.
///
/// The handle is an `Option` because CLOSING it is a signal (EXP-758): the
/// teardown takes the `ChildStdin` out and drops it, and the EOF that reaches
/// the child is the first half of the graceful stop.
#[derive(Clone)]
pub struct LineWriter(Arc<Mutex<Option<std::process::ChildStdin>>>);

impl LineWriter {
    /// One line + `\n`, flushed. `line` must not itself contain a newline.
    pub fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut stdin = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("engine: agent stdin mutex poisoned"))?;
        let stdin = stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::other("engine: agent stdin is closed"))?;
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }

    /// EXP-758: close the child's stdin, so it sees EOF and shuts itself
    /// down. Idempotent; every later `write_line` fails instead of writing
    /// into a pipe nobody reads.
    pub fn close(&self) {
        if let Ok(mut stdin) = self.0.lock() {
            drop(stdin.take());
        }
    }
}

/// Ends the child (whole process group on unix) on every exit path, including
/// a panic, unless the wait thread already reaped it, in which case the pid
/// may belong to somebody else by now and must NOT be signalled.
///
/// EXP-758: the sequence is stdin EOF → SIGTERM → up to [`CHILD_TERM_GRACE`]
/// → SIGKILL, never the bare SIGKILL it used to be.
struct ChildGuard {
    pid: u32,
    exited: Arc<AtomicBool>,
    /// The same handle [`LineWriter`] writes through: closing it is what the
    /// child reads as "no more requests are coming".
    stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
    /// The graceful half runs once, whoever asks for it first.
    asked: AtomicBool,
}

impl ChildGuard {
    /// Stdin EOF, SIGTERM, then wait out [`CHILD_TERM_GRACE`]. Returns early
    /// the moment the wait thread reports the child reaped.
    fn terminate(&self) {
        if self.asked.swap(true, Ordering::SeqCst) {
            return;
        }
        self.stdin_eof();
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(unix)]
        unsafe {
            // Own process group (set at spawn): an agent CLI that spawned
            // tool subprocesses holds the pipes through them.
            libc::killpg(self.pid as i32, libc::SIGTERM);
            libc::kill(self.pid as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + CHILD_TERM_GRACE;
        while Instant::now() < deadline {
            if self.exited.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(TERM_POLL);
        }
    }

    fn stdin_eof(&self) {
        if let Ok(mut stdin) = self.stdin.lock() {
            drop(stdin.take());
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        // Asks first (a no-op when an adapter already did), so the drop only
        // ever kills a child that stayed put through EOF and SIGTERM.
        self.terminate();
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(unix)]
        unsafe {
            libc::killpg(self.pid as i32, libc::SIGKILL);
            libc::kill(self.pid as i32, libc::SIGKILL);
        }
        // The wait thread owns the `Child` and reaps it; nothing to do here.
    }
}

/// Spawn `spec` as a line-delimited JSON child.
///
/// The `SpawnSpec` comes straight off `coding::PreparedLaunch::spawn`, so an
/// ACP launch inherits the exact program/cwd/env the PTY launch would have
/// used minus the hook/observer env `prepare` skips on the Acp arm (D2).
/// `PATH` is seeded from [`login_path`] first so a spec-provided `PATH`
/// still wins.
pub fn spawn_lines(spec: &SpawnSpec, stderr: StderrPolicy) -> std::io::Result<ChildLines> {
    let mut cmd = background_command(&spec.program);
    cmd.env("PATH", login_path());
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    cmd.args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(match stderr {
            StderrPolicy::Drop => Stdio::null(),
            StderrPolicy::Log => Stdio::piped(),
        });
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn()?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout piped above");
    let stdin = child.stdin.take().expect("stdin piped above");
    let child_stderr = child.stderr.take();

    // EXP-758: BOUNDED. A reader thread that can outrun its consumer for
    // ever is a memory leak with a JSON parser in front of it.
    let (line_tx, lines) = flume::bounded(STDOUT_LINES_CAP);
    let dropped = Arc::new(AtomicU64::new(0));
    let dropped_in_thread = Arc::clone(&dropped);
    std::thread::Builder::new()
        .name(format!("acp-lines-{pid}"))
        .spawn(move || {
            // BufReader::lines() splits on `\n` and strips a trailing `\r`.
            // NEVER a splitter that also breaks on U+2028/U+2029: those are
            // legal inside JSON strings and pi's own jsonl reader is
            // deliberately LF-only for exactly that reason.
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                // `try_send`, never `send`: blocking here would stall the
                // reader and, with it, the child's own stdout pipe.
                match line_tx.try_send(line) {
                    Ok(()) => {}
                    Err(flume::TrySendError::Disconnected(_)) => break,
                    Err(flume::TrySendError::Full(_)) => {
                        let count = dropped_in_thread.fetch_add(1, Ordering::Relaxed) + 1;
                        if count % DROP_LOG_EVERY == 1 {
                            log::warn!(
                                "engine: agent {pid} stdout backlog is full, dropped {count} lines"
                            );
                        }
                    }
                }
            }
        })?;

    if let Some(child_stderr) = child_stderr {
        std::thread::Builder::new()
            .name(format!("acp-stderr-{pid}"))
            .spawn(move || {
                for line in BufReader::new(child_stderr).lines() {
                    let Ok(line) = line else { break };
                    log::warn!("engine: agent stderr: {line}");
                }
            })?;
    }

    let exited = Arc::new(AtomicBool::new(false));
    let (exit_tx, exit) = flume::unbounded();
    let exited_in_thread = exited.clone();
    std::thread::Builder::new()
        .name(format!("acp-wait-{pid}"))
        .spawn(move || {
            let captured = match child.wait() {
                Ok(status) => child_exit(status),
                Err(err) => {
                    log::warn!("engine: agent child wait: {err}");
                    ChildExit { code: -1, success: false, signal: None }
                }
            };
            // Set BEFORE announcing the exit: the guard must never signal a
            // pid the OS is free to hand to another process.
            exited_in_thread.store(true, Ordering::SeqCst);
            let _ = exit_tx.send(captured);
        })?;

    // ONE handle behind ONE mutex: the writer writes through it and the
    // guard's teardown takes it out to give the child EOF.
    let stdin = Arc::new(Mutex::new(Some(stdin)));
    Ok(ChildLines {
        lines,
        writer: LineWriter(Arc::clone(&stdin)),
        exit,
        pid,
        guard: ChildGuard {
            pid,
            exited,
            stdin,
            asked: AtomicBool::new(false),
        },
    })
}

/// `std::process` reports a signal as a raw number where `portable_pty`
/// reports a name; the code is what the publisher's `exit:<code>` outcome
/// uses either way, so the number is kept verbatim.
fn child_exit(status: std::process::ExitStatus) -> ChildExit {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt as _;
        status.signal().map(|signal| signal.to_string())
    };
    #[cfg(not(unix))]
    let signal = None;
    ChildExit {
        code: status.code().unwrap_or(-1),
        success: status.success(),
        signal,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn spawn_lines_round_trips_a_line_through_a_child() {
        let spec = SpawnSpec::new("sh").args(["-c", "cat"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        child
            .writer
            .write_line(r#"{"jsonrpc":"2.0","method":"initialize"}"#)
            .expect("stdin accepts a line");
        let echoed = child
            .lines
            .recv_timeout(Duration::from_secs(10))
            .expect("cat echoes the line back");
        assert_eq!(echoed, r#"{"jsonrpc":"2.0","method":"initialize"}"#);
    }

    #[test]
    fn spawn_lines_reports_the_child_exit() {
        let spec = SpawnSpec::new("sh").args(["-c", "exit 3"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let exit = child
            .exit
            .recv_timeout(Duration::from_secs(10))
            .expect("the wait thread reports the exit");
        assert_eq!(exit.code, 3);
        assert!(!exit.success);
    }

    /// EXP-758: the teardown ASKS first. A child that handles SIGTERM runs
    /// its own shutdown (claude's session-end hooks) and its exit code is
    /// what the run reports; a SIGKILL would have replaced both with a
    /// signal death.
    #[test]
    fn dropping_a_child_terminates_it_before_it_kills_it() {
        let spec = SpawnSpec::new("sh").args(["-c", "trap 'exit 3' TERM; sleep 30"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let exit = child.exit.clone();
        // The trap has to be installed before the signal lands; `sleep`
        // running IS that proof, so wait for the process group to settle.
        std::thread::sleep(Duration::from_millis(250));

        let started = Instant::now();
        drop(child);
        // The graceful half returns as soon as the child is reaped, so the
        // drop never sits out the whole grace for a well-behaved CLI.
        assert!(
            started.elapsed() < CHILD_TERM_GRACE,
            "the drop waited out the full grace: {:?}",
            started.elapsed()
        );

        let exit = exit
            .recv_timeout(Duration::from_secs(10))
            .expect("the wait thread reports the exit");
        assert_eq!(exit.code, 3, "the child's own exit code, not a signal death");
        assert_eq!(exit.signal, None);
    }

    /// ... and a child that ignores the ask still dies, one grace later.
    #[test]
    fn a_child_that_ignores_sigterm_is_killed_after_the_grace() {
        // `sleep` dies with the process group, so the loop is what keeps this
        // shell alive through a SIGTERM it ignores.
        let spec = SpawnSpec::new("sh").args(["-c", "trap '' TERM; while :; do sleep 0.2; done"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let exit = child.exit.clone();
        std::thread::sleep(Duration::from_millis(250));

        let started = Instant::now();
        drop(child);
        assert!(
            started.elapsed() >= CHILD_TERM_GRACE,
            "the grace was not honoured: {:?}",
            started.elapsed()
        );

        let exit = exit
            .recv_timeout(Duration::from_secs(10))
            .expect("the wait thread reports the exit");
        assert_eq!(
            exit.signal.as_deref(),
            Some(libc::SIGKILL.to_string()).as_deref(),
            "the guard escalated to SIGKILL"
        );
    }

    /// EXP-758: a child that floods stdout can no longer grow the channel
    /// without bound. The cap is the invariant; dropped lines are logged.
    #[test]
    fn a_flooding_childs_stdout_is_bounded() {
        let flood = STDOUT_LINES_CAP * 4;
        let spec = SpawnSpec::new("sh").args(["-c", &format!("seq 1 {flood}")]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        // Nobody drains `lines`, so the reader thread fills the channel and
        // drops the rest instead of buffering the flood.
        let exit = child
            .exit
            .recv_timeout(Duration::from_secs(30))
            .expect("the wait thread reports the exit");
        assert_eq!(exit.code, 0);
        assert!(
            child.lines.len() <= STDOUT_LINES_CAP,
            "the line channel grew past its cap: {}",
            child.lines.len()
        );
        // What arrived first is still what a reader sees first.
        assert_eq!(
            child.lines.recv_timeout(Duration::from_secs(1)).as_deref(),
            Ok("1")
        );
    }

    /// Closing stdin is the EOF half of the graceful stop, and a write after
    /// it fails instead of panicking on a taken handle.
    #[test]
    fn closing_the_writer_ends_a_child_that_reads_stdin() {
        let spec = SpawnSpec::new("sh").args(["-c", "cat"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        child.writer.write_line("hello").expect("stdin accepts a line");
        child.writer.close();
        assert!(child.writer.write_line("late").is_err());
        let exit = child
            .exit
            .recv_timeout(Duration::from_secs(10))
            .expect("cat exits on EOF");
        assert_eq!(exit.code, 0);
    }
}
