//! EXP-1287: the script worker, a child process per `exec`.
//!
//! The host (the IDE or the `exponential` daemon) re-runs ITS OWN executable
//! as `<exe> __codemode-worker` and speaks JSON lines over the child's
//! stdio: one `Start` line in (the script, the catalog's names, the
//! limits), then `Call` and `Print` lines out and `Answer` lines in while
//! the script runs, and ONE `Done` line at the end. The worker holds only
//! the engine ([`crate::codemode::runtime::Engine`]): the run's credentials, the
//! upstream clients, the call pool, the console and the call log stay in
//! the host ([`crate::codemode::runtime::Dispatcher`]), which is what every `Call`
//! is routed to.
//!
//! Why a process: boa cannot interrupt a synchronous loop and has no heap
//! limit, so a script that spins or allocates without awaiting used to keep
//! a core and grow memory until the host exited (a laptop into swap, an
//! IDE with it). The supervisor here kills the child at the script's
//! deadline, at the run's revoke and when the child's resident memory
//! passes [`Limits::memory_bytes`] (polled every [`TICK`]); on Linux the
//! child also caps its own data segment, a backstop under a burst faster
//! than the poll. A child whose host dies sees EOF on stdin and exits.

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::codemode::catalog::Catalog;
use crate::codemode::runtime::{
    self, CallHost, CallRequest, CallStats, Completion, Console, Dispatcher, Engine, EngineIo, ExecError, ExecOutcome,
    Limits, Prelude,
};

/// The argv marker; a host checks for it FIRST in `main`, before any logger
/// or HTTP client (the worker needs neither).
pub const ARGV_MARKER: &str = "__codemode-worker";
/// The wire shape; a mismatch (an updated binary under an old host) makes
/// the worker answer `Done` with an error and the host run in process.
const PROTOCOL: u32 = 1;
/// How often the supervisor looks at the clock, the revoke flag and the
/// worker's resident memory while nothing arrives from it.
const TICK: Duration = Duration::from_millis(50);
/// The worker's stderr kept for an error text.
const STDERR_CAP: usize = 8 * 1024;
/// Linux: the data-segment rlimit sits this far above the resident ceiling
/// (the engine's own stacks and arenas count toward it as well).
#[cfg(target_os = "linux")]
const RLIMIT_HEADROOM: u64 = 256 * 1024 * 1024;

/// Whether THIS process was started as the worker.
pub fn requested() -> bool {
    std::env::args_os().nth(1).is_some_and(|arg| arg == ARGV_MARKER)
}

/// Run the worker to completion and exit the process. Call it from `main`
/// when [`requested`] answers; it never returns.
pub fn run_and_exit() -> ! {
    std::process::exit(run_worker())
}

// ---------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    protocol: u32,
    script: String,
    prelude: Prelude,
    limits: Limits,
}

/// Host → worker.
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum ToWorker {
    Start(Request),
    /// One call's answer, shaped (`Result` = serde's `{"Ok": …}`/`{"Err": …}`).
    Answer { id: u64, result: Result<Value, String> },
}

/// Worker → host.
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum FromWorker {
    Call(CallRequest),
    Print { text: String },
    /// The engine's verdict: the result and whether it was cut, or boa's
    /// error text (the host decides whether its clock or a revoke explains
    /// it).
    Done { outcome: Result<(Value, bool), String> },
}

fn write_line<W: Write, T: Serialize>(out: &mut W, message: &T) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, message).map_err(std::io::Error::other)?;
    out.write_all(b"\n")?;
    out.flush()
}

// ---------------------------------------------------------------------------
// The worker side
// ---------------------------------------------------------------------------

/// The engine's io in the worker: every call and print is one line on
/// stdout.
struct PipeIo {
    out: Mutex<BufWriter<std::io::Stdout>>,
}

impl PipeIo {
    fn send(&self, message: &FromWorker) -> Result<(), String> {
        write_line(&mut *self.out.lock().unwrap(), message).map_err(|err| format!("the host is gone: {err}"))
    }
}

impl EngineIo for PipeIo {
    fn call(&self, request: CallRequest) -> Result<(), String> {
        self.send(&FromWorker::Call(request))
    }

    fn print(&self, text: &str) {
        let _ = self.send(&FromWorker::Print { text: text.to_string() });
    }
}

fn run_worker() -> i32 {
    let io = Arc::new(PipeIo { out: Mutex::new(BufWriter::new(std::io::stdout())) });
    let mut first = String::new();
    if std::io::stdin().lock().read_line(&mut first).is_err() || first.trim().is_empty() {
        return 2;
    }
    let request = match serde_json::from_str::<ToWorker>(&first) {
        Ok(ToWorker::Start(request)) if request.protocol == PROTOCOL => request,
        Ok(ToWorker::Start(request)) => {
            let _ = io.send(&FromWorker::Done {
                outcome: Err(format!("codemode worker protocol {} does not match the host's {}", PROTOCOL, request.protocol)),
            });
            return 2;
        }
        _ => {
            let _ = io.send(&FromWorker::Done { outcome: Err("the worker expected a start line".into()) });
            return 2;
        }
    };
    apply_memory_limit(request.limits.memory_bytes);
    let (completions_tx, completions_rx) = mpsc::channel::<Completion>();
    // Answers arrive on stdin; EOF means the host is done with us (it read
    // our `Done`) or gone, and either way this process has nothing left to
    // do — exit, whatever the engine thread is busy with.
    let _ = std::thread::Builder::new().name("codemode-worker-stdin".into()).spawn(move || {
        for line in BufReader::new(std::io::stdin()).lines() {
            let Ok(line) = line else { break };
            if let Ok(ToWorker::Answer { id, result }) = serde_json::from_str::<ToWorker>(&line) {
                if completions_tx.send((id, result)).is_err() {
                    break;
                }
            }
        }
        std::process::exit(0);
    });
    let outcome = Engine::run(
        &request.script,
        &request.prelude,
        io.clone(),
        completions_rx,
        Arc::new(AtomicBool::new(false)),
        request.limits,
    );
    let outcome = outcome.map_err(|error| match error {
        ExecError::Script(text) => text,
        ExecError::Timeout => "the script's deadline passed".to_string(),
        ExecError::Cancelled => "the session ended".to_string(),
        ExecError::OutOfMemory { .. } => "the script ran out of memory".to_string(),
    });
    let _ = io.send(&FromWorker::Done { outcome });
    0
}

/// Linux: cap the data segment (brk + private anonymous mappings, which is
/// where every allocation of ours lands) so a burst the host's poll would
/// miss fails its allocation instead of swapping the machine. macOS does
/// not enforce `RLIMIT_DATA` and ignores `RLIMIT_AS`; there the poll is
/// the ceiling.
#[cfg(target_os = "linux")]
fn apply_memory_limit(bytes: u64) {
    let cap = bytes.saturating_add(RLIMIT_HEADROOM) as libc::rlim_t;
    let limit = libc::rlimit { rlim_cur: cap, rlim_max: cap };
    // SAFETY: a plain syscall on our own process with a valid struct.
    unsafe {
        libc::setrlimit(libc::RLIMIT_DATA, &limit);
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_memory_limit(_bytes: u64) {}

// ---------------------------------------------------------------------------
// The host side
// ---------------------------------------------------------------------------

/// How to start a worker: the program, its leading arguments and extra
/// environment. Production = this executable with [`ARGV_MARKER`].
#[derive(Clone, Debug)]
pub(crate) struct WorkerCommand {
    pub program: PathBuf,
    pub args: Vec<std::ffi::OsString>,
    pub env: Vec<(String, String)>,
}

impl WorkerCommand {
    /// This process's own executable as the worker, or `None` when it
    /// cannot be found on disk.
    pub(crate) fn own_executable() -> Option<Self> {
        let exe = heal_deleted_suffix(std::env::current_exe().ok()?);
        exe.is_file().then(|| Self { program: exe, args: vec![ARGV_MARKER.into()], env: Vec::new() })
    }

    fn spawn(&self) -> Result<Child, String> {
        Command::new(&self.program)
            .args(&self.args)
            .envs(self.env.iter().map(|(name, value)| (name.as_str(), value.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("spawn {}: {err}", self.program.display()))
    }
}

/// Linux: a binary replaced on disk while running (the CLI's self-update)
/// reads its own path as `… (deleted)`; the file at the plain path is the
/// new build, which speaks the same protocol or says so.
fn heal_deleted_suffix(exe: PathBuf) -> PathBuf {
    let Some(text) = exe.to_str() else { return exe };
    match text.strip_suffix(" (deleted)") {
        Some(plain) => Path::new(plain).to_path_buf(),
        None => exe,
    }
}

/// What the supervisor loop reacts to.
enum Event {
    Message(FromWorker),
    Completion(Completion),
    /// The worker's stdout closed: it exited (its stderr, capped).
    Exited { stderr: String },
}

/// Spawn a worker for `script` and supervise it to the end. `Err` only when
/// the worker could not be started (the caller falls back); once it runs,
/// every way it can end is an [`ExecOutcome`].
pub(crate) fn supervise(
    command: &WorkerCommand,
    script: &str,
    catalog: &Catalog,
    host: Arc<dyn CallHost>,
    cancelled: Arc<AtomicBool>,
    limits: Limits,
) -> Result<ExecOutcome, String> {
    let started = Instant::now();
    let deadline = started + limits.timeout;
    let mut child = command.spawn()?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the worker's pipes were not piped".into());
    };
    let (events_tx, events_rx) = mpsc::channel::<Event>();
    spawn_readers(stdout, stderr, events_tx.clone());

    let console = Arc::new(Mutex::new(Console::new(limits.console_cap)));
    let stats = Arc::new(Mutex::new(CallStats::default()));
    let dispatcher = Dispatcher::new(
        host,
        console.clone(),
        stats.clone(),
        Arc::new({
            let events_tx = events_tx.clone();
            move |completion| {
                let _ = events_tx.send(Event::Completion(completion));
            }
        }),
        cancelled.clone(),
        deadline,
        limits.max_in_flight,
    );
    drop(events_tx);

    let start = ToWorker::Start(Request {
        protocol: PROTOCOL,
        script: script.to_string(),
        prelude: Prelude::of(catalog),
        limits: limits.clone(),
    });
    let mut verdict: Option<Result<(Value, bool), ExecError>> = None;
    if let Err(err) = write_line(&mut stdin, &start) {
        verdict = Some(Err(ExecError::Script(format!("could not hand the script to the worker: {err}"))));
    }
    let pid = child.id();
    let mut last_probe = Instant::now();
    while verdict.is_none() {
        if let Some(why) = runtime::ended(&cancelled, deadline) {
            verdict = Some(Err(why));
            break;
        }
        let now = Instant::now();
        if now.duration_since(last_probe) >= TICK {
            last_probe = now;
            if let Some(resident) = resident_bytes(pid) {
                if resident > limits.memory_bytes {
                    log::warn!(
                        "[codemode] the script worker held {} MB (ceiling {} MB); killed",
                        resident / (1024 * 1024),
                        limits.memory_bytes / (1024 * 1024)
                    );
                    verdict = Some(Err(ExecError::OutOfMemory { ceiling_bytes: limits.memory_bytes }));
                    break;
                }
            }
        }
        let tick = deadline.saturating_duration_since(now).min(TICK);
        match events_rx.recv_timeout(tick) {
            Ok(Event::Message(FromWorker::Call(request))) => {
                let id = request.id;
                if let Err(why) = dispatcher.call(request) {
                    let _ = write_line(&mut stdin, &ToWorker::Answer { id, result: Err(why) });
                }
            }
            Ok(Event::Message(FromWorker::Print { text })) => console.lock().unwrap().push(&text),
            Ok(Event::Message(FromWorker::Done { outcome })) => {
                verdict = Some(outcome.map_err(|text| runtime::ended(&cancelled, deadline).unwrap_or(ExecError::Script(text))));
            }
            Ok(Event::Completion((id, result))) => {
                // A worker that is gone shows up as `Exited` next.
                let _ = write_line(&mut stdin, &ToWorker::Answer { id, result });
            }
            Ok(Event::Exited { stderr }) => {
                let status = child.wait().ok();
                verdict = Some(Err(exit_error(status, &stderr, limits.memory_bytes)));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                verdict = Some(Err(ExecError::Script("the worker's channel closed".into())));
            }
        }
    }
    // Whatever ended it: the worker dies with the script. Dropping stdin is
    // the EOF a worker that is still alive and well-behaved exits on;
    // `kill` is for the one that is not.
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let verdict = verdict.unwrap_or(Err(ExecError::Script("the worker stopped without a result".into())));
    Ok(runtime::finish(verdict, &console, &stats, started, dispatcher.workers()))
}

/// The worker's stdout as `Event::Message`s (a line that is not ours is
/// skipped), its stderr into a capped buffer, and ONE `Exited` once both
/// pipes closed.
fn spawn_readers(stdout: std::process::ChildStdout, stderr: std::process::ChildStderr, events: Sender<Event>) {
    let stderr_text = Arc::new(Mutex::new(String::new()));
    let stderr_thread = {
        let stderr_text = stderr_text.clone();
        std::thread::Builder::new().name("codemode-worker-stderr".into()).spawn(move || {
            let mut buffer = Vec::new();
            let _ = stderr.take(STDERR_CAP as u64).read_to_end(&mut buffer);
            *stderr_text.lock().unwrap() = String::from_utf8_lossy(&buffer).into_owned();
        })
    };
    let _ = std::thread::Builder::new().name("codemode-worker-stdout".into()).spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<FromWorker>(&line) {
                Ok(message) => {
                    if events.send(Event::Message(message)).is_err() {
                        return;
                    }
                }
                Err(_) if line.trim().is_empty() => {}
                Err(_) => log::debug!("[codemode] worker said: {}", line.chars().take(200).collect::<String>()),
            }
        }
        if let Ok(thread) = stderr_thread {
            let _ = thread.join();
        }
        let stderr = std::mem::take(&mut *stderr_text.lock().unwrap());
        let _ = events.send(Event::Exited { stderr });
    });
}

/// What a worker that ended without a `Done` means: an allocation failure
/// (the rlimit, or the kernel's own killer) is the memory ceiling, anything
/// else a crash with its first stderr line.
fn exit_error(status: Option<std::process::ExitStatus>, stderr: &str, ceiling_bytes: u64) -> ExecError {
    if stderr.contains("memory allocation of") || oom_killed(status) {
        return ExecError::OutOfMemory { ceiling_bytes };
    }
    let first = stderr.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
    let status = status.map(|status| format!(" ({status})")).unwrap_or_default();
    ExecError::Script(match first.is_empty() {
        true => format!("the script engine exited unexpectedly{status}"),
        false => format!("the script engine exited unexpectedly{status}: {first}"),
    })
}

/// Linux's OOM killer and a failed allocation's abort both end a process by
/// signal; neither leaves a `Done`.
fn oom_killed(status: Option<std::process::ExitStatus>) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        return status.and_then(|status| status.signal()).is_some_and(|signal| signal == libc::SIGKILL || signal == libc::SIGABRT);
    }
    #[allow(unreachable_code)]
    {
        let _ = status;
        false
    }
}

/// The resident set of `pid`, in bytes; `None` where this host cannot read
/// it (then the deadline is the only kill).
fn resident_bytes(pid: u32) -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: `info` is a zeroed, correctly sized out-parameter.
        let got = unsafe {
            libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
        };
        return (got == size).then_some(info.pti_resident_size);
    }
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        // SAFETY: a plain sysconf query.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        return (page > 0).then(|| pages * page as u64);
    }
    #[allow(unreachable_code)]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codemode::catalog::ToolSpec;
    use crate::codemode::runtime::Isolation;
    use serde_json::json;
    use std::sync::atomic::Ordering;

    /// The worker entry for the tests below: the test binary re-run with
    /// `--exact` on THIS test and the env marker becomes a worker (libtest's
    /// own lines on stdout are skipped by the host). A plain test run finds
    /// the marker unset and does nothing.
    #[test]
    fn worker_entry() {
        if std::env::var_os("EXP_CODEMODE_WORKER_TEST").is_some() {
            run_and_exit();
        }
    }

    fn test_command() -> WorkerCommand {
        WorkerCommand {
            program: std::env::current_exe().unwrap(),
            args: ["--exact", "codemode::worker::tests::worker_entry", "--nocapture", "--quiet", "--test-threads=1"]
                .into_iter()
                .map(Into::into)
                .collect(),
            env: vec![("EXP_CODEMODE_WORKER_TEST".into(), "1".into())],
        }
    }

    /// Answers `echo` with its arguments, `meet` with its arguments once `n`
    /// meet calls are in flight at once (a rendezvous: calls that never
    /// overlap refuse after [`MEET_WAIT`]), `fail` with a refusal; records
    /// every tool name.
    #[derive(Default)]
    struct FakeHost {
        seen: Mutex<Vec<String>>,
        arrived: Mutex<u64>,
        met: std::sync::Condvar,
    }

    /// How long a `meet` call waits for its party before it refuses; a
    /// safety net for a regression to sequential calls, never a race bound.
    const MEET_WAIT: Duration = Duration::from_secs(60);

    impl CallHost for FakeHost {
        fn call(&self, _server: &str, tool: &str, args: Value) -> Result<Value, String> {
            self.seen.lock().unwrap().push(tool.to_string());
            match tool {
                "echo" => Ok(json!({ "content": [{ "type": "text", "text": "ok" }], "structuredContent": args })),
                "meet" => {
                    let party = args.get("n").and_then(Value::as_u64).unwrap_or(2);
                    let mut arrived = self.arrived.lock().unwrap();
                    *arrived += 1;
                    self.met.notify_all();
                    let (arrived, waited) = self.met.wait_timeout_while(arrived, MEET_WAIT, |arrived| *arrived < party).unwrap();
                    if waited.timed_out() {
                        return Err(format!("met {} of {party}: the calls never overlapped", *arrived));
                    }
                    Ok(json!({ "content": [{ "type": "text", "text": "met" }], "structuredContent": args }))
                }
                "fail" => Ok(json!({ "content": [{ "type": "text", "text": "no such window" }], "isError": true })),
                other => Err(format!("tool_unavailable: {other}")),
            }
        }
    }

    fn catalog() -> Arc<Catalog> {
        Arc::new(Catalog {
            tools: ["echo", "meet", "fail"]
                .into_iter()
                .map(|name| ToolSpec {
                    server: "t".into(),
                    name: name.into(),
                    description: format!("the {name} tool"),
                    input_schema: json!({ "type": "object" }),
                })
                .collect(),
            unreachable: Vec::new(),
            direct_only: Vec::new(),
        })
    }

    fn run(script: &str, limits: Limits) -> (ExecOutcome, Arc<FakeHost>) {
        run_cancellable(script, limits, Arc::new(AtomicBool::new(false)))
    }

    fn run_cancellable(script: &str, limits: Limits, cancelled: Arc<AtomicBool>) -> (ExecOutcome, Arc<FakeHost>) {
        let host = Arc::new(FakeHost::default());
        let dyn_host: Arc<dyn CallHost> = host.clone();
        let outcome = runtime::run_script_with(Isolation::Worker(test_command()), script, catalog(), dyn_host, cancelled, limits);
        (outcome, host)
    }

    /// The whole loop across the pipe: calls run in the host (with its
    /// credentials, in parallel), prints land in the host's console, the
    /// result and the log come back. The three `meet` calls only return once
    /// all three are in flight together, so overlap is proven by the
    /// rendezvous itself, never by a wall-clock span.
    #[test]
    fn a_script_runs_in_the_worker_with_calls_console_and_the_call_log() {
        let (outcome, host) = run(
            r#"console.log("start", {a: 1});
const [a, b, c] = await Promise.all([tools.t.meet({n: 3, x: 1}), tools.t.meet({n: 3, y: 2}), tools.t.meet({n: 3, z: 3})]);
const d = await tools.t.echo({w: 4});
const settled = await Promise.allSettled([tools.t.fail({windowId: "w-9"})]);
console.error("warned");
return { a, b, c, d, rejected: settled[0].status === "rejected" ? settled[0].reason.message : null };"#,
            Limits::default(),
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(
            outcome.result,
            json!({ "a": { "n": 3, "x": 1 }, "b": { "n": 3, "y": 2 }, "c": { "n": 3, "z": 3 }, "d": { "w": 4 }, "rejected": "no such window" })
        );
        assert_eq!(outcome.console, "start {\"a\":1}\nwarned\n");
        assert_eq!((outcome.calls.count, outcome.calls.failed), (5, 1));
        // The rendezvous held three pool threads at once; a thread that has
        // delivered but not yet parked may let the later calls start one more.
        assert!(outcome.workers >= 3, "{} threads", outcome.workers);
        let log: Vec<(&str, Option<&str>, bool)> =
            outcome.calls.log.iter().map(|call| (call.tool.as_str(), call.subject.as_deref(), call.ok)).collect();
        assert_eq!(
            log,
            [("t.meet", None, true), ("t.meet", None, true), ("t.meet", None, true), ("t.echo", None, true), ("t.fail", Some("w-9"), false)]
        );
        assert_eq!(host.seen.lock().unwrap().len(), 5);
    }

    /// THE point of the issue: a synchronous loop nothing in boa can
    /// interrupt dies with its process at the deadline.
    #[test]
    fn a_sync_spin_is_killed_at_the_deadline() {
        let (outcome, _) = run(
            "const f = () => { for (let i = 0; i < 1000; i++) {} }; while (true) { f(); } return 1;",
            Limits { timeout: Duration::from_millis(400), ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::Timeout));
        assert!(outcome.elapsed < Duration::from_millis(1500), "{:?}", outcome.elapsed);
    }

    /// And one that allocates without awaiting dies at the memory ceiling,
    /// long before its deadline.
    #[test]
    fn an_allocating_loop_is_killed_at_the_memory_ceiling() {
        let ceiling = 96 * 1024 * 1024;
        let (outcome, _) = run(
            "const a = []; for (;;) { a.push('x'.repeat(1e6)); } return a.length;",
            Limits { timeout: Duration::from_secs(60), memory_bytes: ceiling, loop_iterations: u64::MAX, ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::OutOfMemory { ceiling_bytes: ceiling }), "{outcome:?}");
        assert!(outcome.elapsed < Duration::from_secs(30), "{:?}", outcome.elapsed);
    }

    #[test]
    fn the_timeout_returns_the_console_so_far_and_a_revoke_cancels() {
        let (outcome, _) = run(
            "console.log('before'); await sleep(5000); console.log('after'); return 1;",
            Limits { timeout: Duration::from_millis(400), ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::Timeout));
        assert_eq!(outcome.console, "before\n");
        assert!(outcome.elapsed < Duration::from_millis(1500), "{:?}", outcome.elapsed);

        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::Release);
        });
        let (outcome, _) = run_cancellable("await sleep(5000); return 1;", Limits::default(), cancelled);
        assert_eq!(outcome.error, Some(ExecError::Cancelled));
        assert!(outcome.elapsed < Duration::from_millis(1500), "{:?}", outcome.elapsed);
    }

    /// A worker that ends without a verdict (here: the harness finds no
    /// such test and exits) is an error at once, never a wait for the
    /// deadline.
    #[test]
    fn a_worker_that_exits_without_a_result_is_an_error_not_a_hang() {
        let mut command = test_command();
        command.args[1] = "worker::tests::no_such_test".into();
        let host: Arc<dyn CallHost> = Arc::new(FakeHost::default());
        let outcome = runtime::run_script_with(
            Isolation::Worker(command),
            "return 1;",
            catalog(),
            host,
            Arc::new(AtomicBool::new(false)),
            Limits { timeout: Duration::from_secs(30), ..Limits::default() },
        );
        let Some(ExecError::Script(text)) = &outcome.error else { panic!("{outcome:?}") };
        assert!(text.contains("exited unexpectedly"), "{text}");
        assert!(outcome.elapsed < Duration::from_secs(10), "{:?}", outcome.elapsed);
    }

    /// No worker binary = the in-process engine, not a refusal.
    #[test]
    fn a_worker_that_cannot_spawn_falls_back_to_the_in_process_engine() {
        let command = WorkerCommand { program: "/nonexistent/codemode-worker".into(), args: Vec::new(), env: Vec::new() };
        let host: Arc<dyn CallHost> = Arc::new(FakeHost::default());
        let outcome = runtime::run_script_with(
            Isolation::Worker(command),
            "return (await tools.t.echo({ok: true})).ok;",
            catalog(),
            host,
            Arc::new(AtomicBool::new(false)),
            Limits::default(),
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(outcome.result, json!(true));
    }

    #[test]
    fn the_wire_round_trips_and_a_deleted_suffix_heals() {
        let call = FromWorker::Call(CallRequest {
            id: 3,
            server: "exponential".into(),
            tool: "exponential_issues_get".into(),
            args: json!({ "id": "EXP-1" }),
            raw: false,
            order: vec!["id".into()],
        });
        let line = serde_json::to_string(&call).unwrap();
        assert!(line.starts_with(r#"{"t":"call","id":3"#), "{line}");
        let answer = ToWorker::Answer { id: 3, result: Ok(Value::Null) };
        let text = serde_json::to_string(&answer).unwrap();
        assert_eq!(text, r#"{"t":"answer","id":3,"result":{"Ok":null}}"#);
        assert!(matches!(serde_json::from_str::<ToWorker>(&text), Ok(ToWorker::Answer { id: 3, result: Ok(Value::Null) })));
        assert_eq!(heal_deleted_suffix("/usr/bin/exponential (deleted)".into()), PathBuf::from("/usr/bin/exponential"));
        assert_eq!(heal_deleted_suffix("/usr/bin/exponential".into()), PathBuf::from("/usr/bin/exponential"));
    }

    #[test]
    fn the_hosts_own_resident_size_is_readable() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let bytes = resident_bytes(std::process::id()).expect("own rss");
            assert!(bytes > 1024 * 1024, "{bytes}");
        }
    }
}
