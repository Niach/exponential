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

use crate::process::background_command;
use crate::spawn::{login_path, ChildExit, SpawnSpec};

use super::child_exit::ChildExitLink;

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
/// shorter than [`super::child_exit::CHILD_EXIT_GRACE`]-scale patience: this one
/// blocks the thread that is tearing the run down.
pub const CHILD_TERM_GRACE: Duration = Duration::from_millis(1500);

/// How often the terminate wait re-checks the reaped flag.
const TERM_POLL: Duration = Duration::from_millis(10);

/// A live agent child: its stdout as lines, its stdin as a line writer, and
/// the one-shot exit. Dropping this ENDS the child (and its process group on
/// unix) unless it already exited: a codex ACP child carries no
/// `claude-hooks` reaper anchor, so this drop is what keeps it from
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
    /// EXP-784: how many stdout lines the reader thread has DROPPED because
    /// the channel was full. Nonzero means the session is already broken
    /// (see [`STDOUT_LINES_CAP`]); the count is forwarded into the exit link
    /// so the ended banner can say so instead of the run just going quiet.
    dropped: Arc<AtomicU64>,
    /// Set by the reader thread at EOF: the pipe is drained and `dropped`
    /// is final. The exit forwarder waits on it (bounded) so the count it
    /// records is the last one, not a snapshot taken while the reader was
    /// still catching up on what the child wrote before exiting.
    reader_done: Arc<AtomicBool>,
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
        forward_exit_with_drops(
            self.exit.clone(),
            link.clone(),
            Some((Arc::clone(&self.dropped), Arc::clone(&self.reader_done))),
        );
    }

    /// EXP-784: stdout lines dropped so far (see [`STDOUT_LINES_CAP`]).
    pub fn dropped_lines(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Whether the reader thread has reached EOF, after which
    /// [`Self::dropped_lines`] is final.
    pub fn stdout_drained(&self) -> bool {
        self.reader_done.load(Ordering::SeqCst)
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
    forward_exit_with_drops(exit, link, None);
}

/// [`forward_exit`], also folding the reader thread's final drop count into
/// the link BEFORE the exit is recorded — the end sequence reads both in one
/// go, and a count that landed after the exit would be missed.
fn forward_exit_with_drops(
    exit: flume::Receiver<ChildExit>,
    link: ChildExitLink,
    dropped: Option<(Arc<AtomicU64>, Arc<AtomicBool>)>,
) {
    // #PERF-12: a task on the caller's runtime (the adapters forward from
    // inside their connection), never a thread per child parked on `recv`.
    // Off a runtime, the thread it always was.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            if let Ok(exit) = exit.recv_async().await {
                if let Some((dropped, reader_done)) = &dropped {
                    let deadline = std::time::Instant::now() + READER_DRAIN_GRACE;
                    while !reader_done.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    link.record_dropped_lines(dropped.load(Ordering::Relaxed));
                }
                link.record(exit);
            }
        });
        return;
    }
    let _ = std::thread::Builder::new()
        .name("exit-forward".to_string())
        .spawn(move || {
            if let Ok(exit) = exit.recv() {
                if let Some((dropped, reader_done)) = &dropped {
                    // The child is gone but its last writes may still sit in
                    // the pipe; give the reader a moment to drain them (a
                    // grandchild holding stdout open is why this is bounded).
                    let deadline = std::time::Instant::now() + READER_DRAIN_GRACE;
                    while !reader_done.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    link.record_dropped_lines(dropped.load(Ordering::Relaxed));
                }
                link.record(exit);
            }
        });
}

/// #PERF-12: an agent's stderr, read on the runtime instead of a thread.
#[cfg(unix)]
mod stderr_task {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::process::ChildStderr;
    use std::sync::Arc;

    use tokio::io::unix::AsyncFd;

    use crate::wire::Redactor;

    /// Starts the reader as a task on the current runtime; `Err` hands the
    /// pipe back (no runtime here, or the fd would not register) for the
    /// thread instead.
    pub(super) fn spawn(stderr: ChildStderr, redactor: Arc<Redactor>) -> Result<(), ChildStderr> {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return Err(stderr);
        };
        let fd = stderr.as_raw_fd();
        // SAFETY: `fd` is the open pipe `stderr` owns; only its flags change.
        let nonblocking = unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            flags >= 0 && libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0
        };
        if !nonblocking {
            return Err(stderr);
        }
        let _entered = runtime.enter();
        let mut stderr = match AsyncFd::try_new(stderr) {
            Ok(stderr) => stderr,
            Err(err) => {
                let (stderr, _) = err.into_parts();
                // The thread reads blocking.
                // SAFETY: as above.
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    if flags >= 0 {
                        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
                    }
                }
                return Err(stderr);
            }
        };
        runtime.spawn(async move {
            let mut pending: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let Ok(mut ready) = stderr.readable_mut().await else { break };
                match ready.try_io(|inner| inner.get_mut().read(&mut chunk)) {
                    Ok(Ok(0)) => break,
                    Ok(Ok(read)) => {
                        pending.extend_from_slice(&chunk[..read]);
                        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                            let line: Vec<u8> = pending.drain(..=end).collect();
                            log_line(&redactor, &line[..line.len() - 1]);
                        }
                    }
                    Ok(Err(err)) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    Ok(Err(_)) => break,
                    // Not readable after all: wait for the next edge.
                    Err(_) => {}
                }
            }
            if !pending.is_empty() {
                log_line(&redactor, &pending);
            }
        });
        Ok(())
    }

    fn log_line(redactor: &Redactor, line: &[u8]) {
        let line = String::from_utf8_lossy(line);
        let line = line.strip_suffix('\r').unwrap_or(&line);
        log::warn!("engine: agent stderr: {}", redactor.redact(line));
    }
}

/// How long the exit forwarder waits for the stdout reader to reach EOF
/// before recording the drop count (see [`forward_exit_with_drops`]).
const READER_DRAIN_GRACE: Duration = Duration::from_secs(2);

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
/// → SIGKILL, never the bare SIGKILL it used to be. On Windows (EXP-766) the
/// same shape without signals: stdin EOF → the grace → `taskkill /T /F`.
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
        // EXP-766: Windows has no signals, so before this the escalation ended
        // at stdin EOF and an agent CLI that ignores it (or a tool subprocess
        // holding the pipes) simply stayed. `/T` takes the whole tree, which
        // is what `killpg` does everywhere else.
        #[cfg(windows)]
        {
            let _ = background_command("taskkill")
                .args(["/PID", &self.pid.to_string(), "/T", "/F"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        // The wait thread owns the `Child` and reaps it; nothing to do here.
    }
}

/// EXP-1910: how long an exited agent's process group gets between SIGTERM
/// and SIGKILL. Short: it delays the exit's announcement.
#[cfg(unix)]
const GROUP_TERM_GRACE: Duration = Duration::from_millis(200);

/// EXP-1910: wait for `pid` to exit WITHOUT reaping it, then end what is left
/// of its process group (SIGTERM, [`GROUP_TERM_GRACE`], SIGKILL). The zombie
/// still pins the pid and the group id, so neither can name a stranger; the
/// caller reaps afterwards. [`ChildGuard`] never signals a group whose
/// leader is gone, so an agent that died by itself used to leave its tool
/// subprocesses (a build) running for good.
#[cfg(unix)]
fn end_group_of_exited(pid: u32) {
    // SAFETY: `waitid` fills a zeroed `siginfo_t`; WNOWAIT leaves the child
    // a zombie for `Child::wait`. `killpg` on our own child's group (set at
    // spawn, `process_group(0)`) while that zombie holds the id.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        loop {
            let waited = libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOWAIT);
            if waited == 0 {
                break;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return;
            }
        }
        if libc::killpg(pid as i32, libc::SIGTERM) != 0 {
            return;
        }
        std::thread::sleep(GROUP_TERM_GRACE);
        libc::killpg(pid as i32, libc::SIGKILL);
    }
}

/// Spawn `spec` as a line-delimited JSON child.
///
/// The `SpawnSpec` comes straight off `coding::PreparedLaunch::spawn`, so an
/// ACP launch inherits the exact program/cwd/env the PTY launch would have
/// used minus the hook/observer env `prepare` skips on the Acp arm (D2).
/// `PATH` is seeded from [`login_path`] first so a spec-provided `PATH`
/// still wins.
///
/// NEW-17: on Linux the child runs in its own systemd scope when the host
/// can ([`crate::run_scope`]) — `systemd-run --scope` execs it in place, so
/// the pid and pipes below are the agent's own.
pub fn spawn_lines(spec: &SpawnSpec, stderr: StderrPolicy) -> std::io::Result<ChildLines> {
    let mode = crate::run_scope::ScopeMode::current();
    // #EXP-1868: a session's run gets the scope its end stops by name.
    let session =
        spec.env.iter().find(|(key, _)| key == crate::run_scope::SESSION_ENV).map(|(_, value)| value.as_str());
    let argv = mode.wrap_for(&spec.program, &spec.args, session);
    // #FEED-308: measured as spawned (the scope wrap included), so a line
    // the system would refuse fails here, naming the argument.
    crate::command_line::check_spawn(&argv[0], &argv[1..], &spec.env)?;
    let mut cmd = background_command(&argv[0]);
    cmd.env("PATH", login_path());
    for key in &spec.env_remove {
        cmd.env_remove(key);
    }
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    cmd.args(&argv[1..])
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
        // PERF-4: a run is background work; without a scope (macOS, a host
        // without a user manager) the child gets the run's nice level here,
        // and its builds inherit it.
        if *mode == crate::run_scope::ScopeMode::Off {
            // SAFETY: only `setpriority(2)` runs between fork and exec; it is
            // async-signal-safe and touches no memory of the parent.
            let nice = crate::run_scope::run_nice();
            unsafe {
                cmd.pre_exec(move || {
                    libc::setpriority(libc::PRIO_PROCESS, 0, nice);
                    Ok(())
                });
            }
        }
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
    let reader_done = Arc::new(AtomicBool::new(false));
    let reader_done_in_thread = Arc::clone(&reader_done);
    std::thread::Builder::new()
        .name(format!("acp-lines-{pid}"))
        .spawn(move || {
            let _done = ReaderDone(reader_done_in_thread);
            // BufReader::lines() splits on `\n` and strips a trailing `\r`.
            // NEVER a splitter that also breaks on U+2028/U+2029: those are
            // legal inside JSON strings and a jsonl reader is
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
        // The child's env carries `EXP_MCP_TOKEN` (the user's `expu_` MCP
        // key) and the launcher's other secrets, and an agent CLI that dumps
        // its config on stderr would write them straight into the daemon's
        // journal. Same masking the wire gets (`steer::Redactor`), off this
        // spawn's OWN env.
        let redactor = Arc::new(crate::wire::Redactor::new(secret_env_values(&spec.env)));
        // #PERF-12: on the caller's runtime when there is one (the
        // adapters spawn from inside their connection) — a thread per
        // child for a stream that is silent nearly always was one of the
        // ~25 a hosted session held.
        #[cfg(unix)]
        let child_stderr = match stderr_task::spawn(child_stderr, Arc::clone(&redactor)) {
            Ok(()) => None,
            Err(child_stderr) => Some(child_stderr),
        };
        #[cfg(not(unix))]
        let child_stderr = Some(child_stderr);
        if let Some(child_stderr) = child_stderr {
            std::thread::Builder::new()
                .name(format!("acp-stderr-{pid}"))
                .spawn(move || {
                    for line in BufReader::new(child_stderr).lines() {
                        let Ok(line) = line else { break };
                        log::warn!("engine: agent stderr: {}", redactor.redact(&line));
                    }
                })?;
        }
    }

    let exited = Arc::new(AtomicBool::new(false));
    let (exit_tx, exit) = flume::unbounded();
    let exited_in_thread = exited.clone();
    std::thread::Builder::new()
        .name(format!("acp-wait-{pid}"))
        .spawn(move || {
            // EXP-1910: the agent went away by itself (a crash, the OOM
            // killer) while its tools still run in its process group.
            #[cfg(unix)]
            end_group_of_exited(pid);
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
        dropped,
        reader_done,
        guard: ChildGuard {
            pid,
            exited,
            stdin,
            asked: AtomicBool::new(false),
        },
    })
}

/// Flips the reader-done flag when the reader thread ends, however it ends.
struct ReaderDone(Arc<AtomicBool>);

impl Drop for ReaderDone {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// The values of `env` that must never reach a log line: anything under a
/// secret-shaped NAME plus anything with a credential PREFIX whatever it is
/// called (`expu_` personal keys, `ghs_` installation tokens). Names, not
/// values, decide the first half: a spec-provided token is opaque, and the
/// generic patterns `steer::Redactor` also applies cannot know it.
fn secret_env_values(env: &[(String, String)]) -> Vec<String> {
    const SECRET_NAME_PARTS: [&str; 4] = ["TOKEN", "SECRET", "KEY", "PASSWORD"];
    const SECRET_VALUE_PREFIXES: [&str; 2] = ["expu_", "ghs_"];
    env.iter()
        .filter(|(key, value)| {
            let key = key.to_ascii_uppercase();
            SECRET_NAME_PARTS.iter().any(|part| key.contains(part))
                || SECRET_VALUE_PREFIXES.iter().any(|prefix| value.starts_with(prefix))
        })
        .map(|(_, value)| value.clone())
        .collect()
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

    /// The stderr journal is a LOG of the agent's own diagnostics, and the
    /// child's env holds the user's `expu_` MCP key: a CLI that prints its
    /// config would otherwise write the key into the daemon's journal.
    #[test]
    fn stderr_masking_takes_every_secret_out_of_the_spawn_env() {
        let env = vec![
            ("EXP_MCP_TOKEN".to_string(), "expu_supersecretvalue".to_string()),
            ("GIT_ASKPASS_TOKEN".to_string(), "ghs_installationtoken".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "sk-ant-notarealkey".to_string()),
            ("BEARER".to_string(), "ghs_prefixedbutplainlynamed".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ];
        let secrets = secret_env_values(&env);
        assert_eq!(secrets.len(), 4, "PATH is not a secret: {secrets:?}");
        assert!(!secrets.iter().any(|value| value == "/usr/bin"));

        let redactor = crate::wire::Redactor::new(secrets);
        let masked = redactor.redact("config: token=expu_supersecretvalue path=/usr/bin");
        assert!(!masked.contains("expu_supersecretvalue"), "{masked}");
        assert!(masked.contains("/usr/bin"), "{masked}");
    }

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

    /// #EXP-1868, end to end on a host that scopes runs: a leftover the
    /// session's agent `setsid`ed away (a vite, a gradle daemon) outlives the
    /// agent's process group kill, and the session's scope stop takes it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_sessions_setsid_leftover_dies_with_its_scope() {
        if *crate::run_scope::ScopeMode::current() == crate::run_scope::ScopeMode::Off {
            return;
        }
        let session = format!("00000000-0000-4000-8000-{:012}", std::process::id());
        let spec = SpawnSpec::new("sh")
            .args(["-c", "setsid sleep 300 >/dev/null 2>&1 & echo $!; cat"])
            .env(crate::run_scope::SESSION_ENV, &session);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let leftover: i32 =
            child.lines.recv_timeout(Duration::from_secs(10)).expect("the leftover's pid").trim().parse().unwrap();
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) == 0 };
        // #EXP-1902: the shell prints the pid at the fork. Until the leftover
        // has reached `setsid(2)` it is still in the agent's group, and the
        // kill below would rightly take it.
        let led = std::time::Instant::now() + Duration::from_secs(30);
        while unsafe { libc::getsid(leftover) } != leftover {
            assert!(std::time::Instant::now() < led, "the leftover leads its own session");
            std::thread::sleep(Duration::from_millis(10));
        }
        let exit = child.exit.clone();
        drop(child);
        exit.recv_timeout(Duration::from_secs(30)).expect("the agent is reaped after its group kill");
        assert!(alive(leftover), "the agent's group kill misses a setsid leftover");
        assert!(crate::run_scope::stop_session_scopes(&session), "the stop is asked");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while alive(leftover) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let gone = !alive(leftover);
        if !gone {
            unsafe { libc::kill(leftover, libc::SIGKILL) };
        }
        assert!(gone, "the session's scope stop takes its leftover");
    }

    /// PERF-4: a run starts at the run's nice level, scope or not (a scope
    /// from a systemd that refuses `CPUWeight=idle` keeps the default).
    #[cfg(unix)]
    #[test]
    fn spawn_lines_runs_the_child_at_background_priority() {
        let spec = SpawnSpec::new("sh").args(["-c", "ps -o ni= -p $$"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let nice = child.lines.recv_timeout(Duration::from_secs(10)).expect("ps prints the nice level");
        let expected = match crate::run_scope::ScopeMode::current() {
            crate::run_scope::ScopeMode::Systemd { background: false, .. } => 0,
            _ => crate::run_scope::run_nice(),
        };
        assert_eq!(nice.trim().parse::<i32>().expect("a number"), expected);
    }

    /// #FEED-308: what `CreateProcess` answered with "os error 206" is
    /// refused before the spawn, naming the argument; the same text as a
    /// file path starts.
    #[cfg(windows)]
    #[test]
    fn a_command_line_over_the_windows_limit_is_refused_by_name() {
        let long = SpawnSpec::new("cmd.exe").args(["/C", "echo", "--append-system-prompt", &"x".repeat(40_000)]);
        let err = spawn_lines(&long, StderrPolicy::Drop).err().expect("refused");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        let message = err.to_string();
        assert!(message.contains("over Windows' limit of 30,000"), "{message}");
        assert!(message.contains("the value of --append-system-prompt"), "{message}");
        assert_ne!(err.raw_os_error(), Some(206));

        let dir = std::env::temp_dir().join(format!("exp-command-line-{}", std::process::id()));
        let file = crate::command_line::write_launch_file(&dir, "prompt", "md", &"x".repeat(40_000)).unwrap();
        let short = SpawnSpec::new("cmd.exe").args(["/C", "echo", "--append-system-prompt-file", &file.display().to_string()]);
        let child = spawn_lines(&short, StderrPolicy::Drop).expect("a path on the line starts");
        let echoed = child.lines.recv_timeout(Duration::from_secs(30)).expect("cmd echoes its line");
        assert!(echoed.contains("--append-system-prompt-file"), "{echoed}");
        let _ = std::fs::remove_dir_all(&dir);
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

    /// EXP-1910: an agent that goes away by itself (a crash, the OOM
    /// killer) takes its process group with it: a tool subprocess it left
    /// running (a build) used to stay for good, since the guard never
    /// signals a group whose leader was already reaped.
    #[cfg(unix)]
    #[test]
    fn an_agent_that_exits_by_itself_takes_its_process_group_with_it() {
        let spec = SpawnSpec::new("sh").args(["-c", "sleep 300 >/dev/null 2>&1 & echo $!; exit 3"]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let leftover: i32 = child
            .lines
            .recv_timeout(Duration::from_secs(10))
            .expect("the leftover's pid")
            .trim()
            .parse()
            .expect("a pid");
        let exit = child.exit.recv_timeout(Duration::from_secs(10)).expect("the exit");
        assert_eq!(exit.code, 3, "the agent's own exit code is still what the run reports");
        // Gone, or a zombie its new parent has not reaped yet.
        let gone = || {
            // SAFETY: signal 0 only probes a pid this test's own child printed.
            let dead = unsafe { libc::kill(leftover, 0) } != 0;
            dead || std::fs::read_to_string(format!("/proc/{leftover}/stat"))
                .is_ok_and(|stat| stat.rsplit(')').next().is_some_and(|rest| rest.trim_start().starts_with('Z')))
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gone() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let survived = !gone();
        if survived {
            // SAFETY: still the test's own `sleep` (a reaped pid is never signalled).
            unsafe { libc::kill(leftover, libc::SIGKILL) };
        }
        assert!(!survived, "the agent's leftover outlived it");
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
        // EXP-784: the drops are COUNTED, not just logged. The count is
        // final once the reader reached EOF, which can trail the exit — and
        // nothing may be received before then, or a freed slot admits one
        // more line.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !child.stdout_drained() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(child.stdout_drained(), "the reader reaches EOF");
        let dropped = child.dropped_lines();
        assert_eq!(dropped, (flood - STDOUT_LINES_CAP) as u64, "every line past the cap");
        // What arrived first is still what a reader sees first.
        assert_eq!(
            child.lines.recv_timeout(Duration::from_secs(1)).as_deref(),
            Ok("1")
        );
    }

    /// EXP-784: the final drop count rides the exit link, ahead of the exit
    /// itself, so the end sequence can put it on the banner.
    #[test]
    fn the_drop_count_reaches_the_exit_link_before_the_exit() {
        let flood = STDOUT_LINES_CAP * 2;
        let spec = SpawnSpec::new("sh").args(["-c", &format!("seq 1 {flood}")]);
        let child = spawn_lines(&spec, StderrPolicy::Drop).expect("sh spawns");
        let link = ChildExitLink::new();
        child.forward_exit(&link);
        let deadline = Instant::now() + Duration::from_secs(30);
        while link.get().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(link.get().map(|exit| exit.code), Some(0));
        assert_eq!(link.dropped_lines(), STDOUT_LINES_CAP as u64);

        // A child that never floods reports zero.
        let quiet = spawn_lines(&SpawnSpec::new("sh").args(["-c", "echo one"]), StderrPolicy::Drop)
            .expect("sh spawns");
        let quiet_link = ChildExitLink::new();
        quiet.forward_exit(&quiet_link);
        let deadline = Instant::now() + Duration::from_secs(10);
        while quiet_link.get().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(quiet_link.dropped_lines(), 0);
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
