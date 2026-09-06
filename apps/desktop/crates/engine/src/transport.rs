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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

/// A live agent child: its stdout as lines, its stdin as a line writer, and
/// the one-shot exit. Dropping this KILLS the child (and its process group
/// on unix) unless it already exited — codex/pi/external ACP children carry
/// no `claude-hooks` reaper anchor, so this drop is what keeps them from
/// escaping (EXP-300).
pub struct ChildLines {
    /// One element per line of stdout, `\n`-delimited, trailing `\r` stripped.
    /// The sender is dropped on EOF, so a disconnected receiver IS the
    /// "child closed stdout" edge.
    pub lines: flume::Receiver<String>,
    pub writer: LineWriter,
    /// Fires exactly once, after `wait()` reaped the child.
    pub exit: flume::Receiver<ChildExit>,
    /// The child's pid — log lines and the reaper's protection checks.
    pub pid: u32,
    _guard: ChildGuard,
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
#[derive(Clone)]
pub struct LineWriter(Arc<Mutex<std::process::ChildStdin>>);

impl LineWriter {
    /// One line + `\n`, flushed. `line` must not itself contain a newline.
    pub fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut stdin = self.0.lock().map_err(|_| {
            std::io::Error::other("engine: agent stdin mutex poisoned")
        })?;
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }
}

/// Kills the child (whole process group on unix) on every exit path,
/// including a panic — unless the wait thread already reaped it, in which
/// case the pid may belong to somebody else by now and must NOT be signalled.
struct ChildGuard {
    pid: u32,
    exited: Arc<AtomicBool>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(unix)]
        unsafe {
            // Own process group (set at spawn): an agent CLI that spawned
            // tool subprocesses holds the pipes through them.
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

    let (line_tx, lines) = flume::unbounded();
    std::thread::Builder::new()
        .name(format!("acp-lines-{pid}"))
        .spawn(move || {
            // BufReader::lines() splits on `\n` and strips a trailing `\r`.
            // NEVER a splitter that also breaks on U+2028/U+2029: those are
            // legal inside JSON strings and pi's own jsonl reader is
            // deliberately LF-only for exactly that reason.
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line_tx.send(line).is_err() {
                    break;
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

    Ok(ChildLines {
        lines,
        writer: LineWriter(Arc::new(Mutex::new(stdin))),
        exit,
        pid,
        _guard: ChildGuard { pid, exited },
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
    use std::time::Duration;

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
}
