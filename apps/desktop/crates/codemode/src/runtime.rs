//! EXP-1236: the script engine behind `exec`. One boa `Context` per script,
//! on its own thread, with an event loop driven BY HAND: every
//! `tools.<server>.<tool>(args)` the script awaits becomes a pending JS
//! promise plus a job on a bounded pool of call threads (at most
//! [`Limits::max_in_flight`] of them, the rest queue in order); the loop
//! drains microtasks, waits for completions and settles promises as they
//! land, so N awaited calls under `Promise.all` really run at once. The
//! engine's own job executor is replaced by one that looks at the script's
//! deadline between jobs ([`DeadlineExecutor`]).
//!
//! Nothing else reaches the script: no filesystem, no network, no timers
//! besides `sleep`, no imports (the context carries boa's idle module
//! loader, so `import()` rejects instead of reading a file next to the host
//! cwd). Output = what the script printed plus the value it returned,
//! capped, so a script that reads a hundred windows puts one summary into
//! the model's context instead of a hundred screenshots.
//!
//! # The deadline and what it cannot stop
//!
//! boa 0.22 has no preemptive interrupt: a running function is only ever
//! stopped by its own limits (a per-call-frame loop ceiling, a recursion
//! ceiling, a value-stack ceiling). So the deadline is observed at every
//! point the engine hands control back to us: between jobs (each `await`
//! continuation), in every native (`tools.*`, `sleep`, `console.*`), while
//! waiting for a call, and when a pending native future is polled. A script
//! that awaits anything, or calls anything, ends within one tick of its
//! deadline; `run_script` itself ALWAYS returns at the deadline.
//!
//! The residual: a synchronous loop that never awaits (`while (true) {}`,
//! a busy-wait in a helper function, a pathological regex) keeps its
//! abandoned engine thread busy until boa's per-frame ceiling ends it
//! ([`Limits::loop_iterations`] iterations per call frame, seconds in
//! practice). A loop that re-enters a helper on every iteration gets a
//! fresh frame ceiling each time, so such a thread can outlive the script
//! by a long time, pinning one core until the host process exits. The
//! caller got its timeout answer regardless; nothing the thread does can
//! reach a tool (every native refuses past the deadline) or the result.
//! A true kill needs a child process per script, which the hosts do not
//! wire today.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll, Waker};
use std::time::{Duration, Instant};

use boa_engine::builtins::promise::{PromiseState, ResolvingFunctions};
use boa_engine::job::{GenericJob, Job, JobExecutor, NativeAsyncJob, PromiseJob};
use boa_engine::module::IdleModuleLoader;
use boa_engine::object::builtins::JsPromise;
use boa_engine::{js_string, Context, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Source};
use serde_json::{json, Value};

use crate::catalog::Catalog;

/// What the engine asks of the run: ONE MCP `tools/call`, answered as the
/// raw MCP call result (`content`, `structuredContent`, `isError`), or a
/// transport-level refusal as text.
pub trait CallHost: Send + Sync {
    fn call(&self, server: &str, tool: &str, args: Value) -> Result<Value, String>;
}

/// The ceilings one script runs under.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Wall clock for the whole script.
    pub timeout: Duration,
    /// Nested calls in flight at once = the size of the call pool; the rest
    /// queue in order.
    pub max_in_flight: usize,
    /// Bytes of console output kept.
    pub console_cap: usize,
    /// Bytes of the serialized return value kept.
    pub result_cap: usize,
    /// boa's per-call-frame loop iteration ceiling: a `while (true) {}`
    /// ends here (see the module docs for what it does not bound).
    pub loop_iterations: u64,
    /// boa's recursion ceiling.
    pub recursion: usize,
    /// boa's value-stack ceiling (entries).
    pub stack_size: usize,
}

/// `timeout_ms` default and bounds (`exec`'s schema says the same).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);
pub const MIN_TIMEOUT: Duration = Duration::from_secs(1);
pub const MAX_TIMEOUT: Duration = Duration::from_secs(1800);
/// Nested calls in flight at once.
pub const MAX_IN_FLIGHT: usize = 16;

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_in_flight: MAX_IN_FLIGHT,
            console_cap: 48 * 1024,
            result_cap: 16 * 1024,
            loop_iterations: 10_000_000,
            recursion: 256,
            stack_size: 10 * 1024,
        }
    }
}

/// How a script ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecError {
    /// The script threw (or failed to parse); boa's message.
    Script(String),
    /// [`Limits::timeout`] passed.
    Timeout,
    /// The run ended (its grant was revoked) while the script ran.
    Cancelled,
}

/// Counts for the result's footer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallStats {
    pub count: usize,
    pub failed: usize,
}

/// Everything `exec` reports back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecOutcome {
    /// The returned value as JSON (`null` when the script returned nothing).
    pub result: Value,
    /// Everything the script printed, in order.
    pub console: String,
    pub error: Option<ExecError>,
    pub calls: CallStats,
    /// Whether the console or the result were cut at their caps.
    pub truncated: bool,
    pub elapsed: Duration,
    /// Call threads the script had alive at its peak (≤ `max_in_flight`).
    pub workers: usize,
}

impl ExecOutcome {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }
}

/// Run `script` to completion (or to a limit) and report. Blocks the caller
/// for at most `limits.timeout`; on timeout the engine thread is told to
/// stop (every native and job boundary sees it) and abandoned, see the
/// module docs for the one case it keeps running.
pub fn run_script(
    script: &str,
    catalog: Arc<Catalog>,
    host: Arc<dyn CallHost>,
    cancelled: Arc<AtomicBool>,
    limits: Limits,
) -> ExecOutcome {
    run_script_observed(script, catalog, host, cancelled, limits).0
}

/// [`run_script`] plus a flag the engine thread sets when it has really
/// ended (for tests of the abandoned-thread path).
pub(crate) fn run_script_observed(
    script: &str,
    catalog: Arc<Catalog>,
    host: Arc<dyn CallHost>,
    cancelled: Arc<AtomicBool>,
    limits: Limits,
) -> (ExecOutcome, Arc<AtomicBool>) {
    let started = Instant::now();
    let console = Arc::new(Mutex::new(Console::new(limits.console_cap)));
    let stats = Arc::new(Mutex::new(CallStats::default()));
    let engine_exited = Arc::new(AtomicBool::new(false));
    let (done_tx, done_rx) = mpsc::channel::<EngineReport>();
    let thread = {
        let script = script.to_string();
        let console = console.clone();
        let stats = stats.clone();
        let cancelled = cancelled.clone();
        let limits = limits.clone();
        let exited = engine_exited.clone();
        std::thread::Builder::new().name("codemode-script".into()).spawn(move || {
            let report = Engine::run(&script, catalog, host, console, stats, cancelled, limits);
            let _ = done_tx.send(report);
            exited.store(true, Ordering::Release);
        })
    };
    let (outcome, workers) = match thread {
        Ok(_) => match done_rx.recv_timeout(limits.timeout) {
            Ok(report) => (report.outcome, report.workers),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                (Err(ExecError::Script("the script engine crashed; see the host log".into())), 0)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Stop everything still in flight from feeding a script
                // nobody waits for; the engine thread sees the flag at its
                // next native call or job boundary and returns.
                cancelled.store(true, Ordering::Release);
                (Err(ExecError::Timeout), 0)
            }
        },
        Err(err) => (Err(ExecError::Script(format!("could not start the script thread: {err}"))), 0),
    };
    let (console_text, console_cut) = console.lock().unwrap().take();
    let calls = stats.lock().unwrap().clone();
    let (result, error, result_cut) = match outcome {
        Ok((value, cut)) => (value, None, cut),
        Err(error) => (Value::Null, Some(error), false),
    };
    (
        ExecOutcome {
            result,
            console: console_text,
            error,
            calls,
            truncated: console_cut || result_cut,
            elapsed: started.elapsed(),
            workers,
        },
        engine_exited,
    )
}

/// The script's captured output, cut at its cap with a note.
struct Console {
    text: String,
    cap: usize,
    dropped: usize,
}

impl Console {
    fn new(cap: usize) -> Self {
        Self { text: String::new(), cap, dropped: 0 }
    }

    fn push(&mut self, line: &str) {
        let room = self.cap.saturating_sub(self.text.len());
        if room == 0 {
            self.dropped += line.len() + 1;
            return;
        }
        if line.len() < room {
            self.text.push_str(line);
            self.text.push('\n');
        } else {
            let mut end = room.saturating_sub(1);
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            self.text.push_str(&line[..end]);
            self.text.push('\n');
            self.dropped += line.len() - end;
        }
    }

    fn take(&mut self) -> (String, bool) {
        let mut text = std::mem::take(&mut self.text);
        let cut = self.dropped > 0;
        if cut {
            text.push_str(&format!("… [console truncated, {} more bytes]\n", self.dropped));
        }
        (text, cut)
    }
}

/// A completion for one pending promise.
type Completion = (u64, Result<Value, String>);

/// What the engine thread hands back.
struct EngineReport {
    outcome: Result<(Value, bool), ExecError>,
    workers: usize,
}

/// The engine's per-thread state the native functions reach through
/// [`ENGINE`] (boa natives are plain `fn` pointers; captures would need the
/// GC's `Trace`).
struct EngineState {
    host: Arc<dyn CallHost>,
    console: Arc<Mutex<Console>>,
    stats: Arc<Mutex<CallStats>>,
    pending: HashMap<u64, ResolvingFunctions>,
    /// `sleep` promises by their due time; the run loop settles them (no
    /// thread per sleep).
    sleeps: Vec<(Instant, u64)>,
    next_id: u64,
    completions: Sender<Completion>,
    pool: Pool,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

impl EngineState {
    fn ended(&self) -> Option<ExecError> {
        ended(&self.cancelled, self.deadline)
    }
}

/// Which limit ended the script, if one did: a revoke wins over the clock.
fn ended(cancelled: &AtomicBool, deadline: Instant) -> Option<ExecError> {
    if cancelled.load(Ordering::Acquire) {
        Some(ExecError::Cancelled)
    } else if Instant::now() >= deadline {
        Some(ExecError::Timeout)
    } else {
        None
    }
}

thread_local! {
    static ENGINE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

/// Live call-pool threads across the process (diagnostics + tests).
pub(crate) static LIVE_WORKERS: AtomicUsize = AtomicUsize::new(0);

/// One unit of work for the call pool.
type PoolJob = Box<dyn FnOnce() + Send>;

/// Bookkeeping the pool and its workers share.
#[derive(Default)]
struct PoolState {
    /// Workers started (and not yet exited).
    spawned: usize,
    /// Workers waiting for a job.
    idle: usize,
    /// Jobs sent and not yet taken by a worker.
    queued: usize,
    /// The most workers alive at once.
    peak: usize,
}

/// A bounded pool of call threads: at most `limit` run at once, the rest
/// queue in order. Workers start on demand and exit once the script's state
/// (the only sender) is gone, so an idle script costs no threads. A pool
/// that cannot start its first worker refuses the job, which rejects the
/// promise with the reason instead of surfacing as a timeout.
struct Pool {
    limit: usize,
    tx: Sender<PoolJob>,
    rx: Arc<Mutex<Receiver<PoolJob>>>,
    shared: Arc<Mutex<PoolState>>,
}

impl Pool {
    fn new(limit: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        Self { limit: limit.max(1), tx, rx: Arc::new(Mutex::new(rx)), shared: Arc::default() }
    }

    fn submit(&self, job: PoolJob) -> Result<(), String> {
        {
            let mut state = self.shared.lock().unwrap();
            state.queued += 1;
            if state.queued > state.idle && state.spawned < self.limit {
                let rx = self.rx.clone();
                let shared = self.shared.clone();
                match std::thread::Builder::new().name("codemode-call".into()).spawn(move || worker(rx, shared)) {
                    Ok(_) => {
                        state.spawned += 1;
                        state.peak = state.peak.max(state.spawned);
                    }
                    Err(err) if state.spawned == 0 => {
                        state.queued -= 1;
                        return Err(format!("could not start a call thread: {err}"));
                    }
                    Err(err) => log::warn!("[codemode] could not start another call thread: {err}"),
                }
            }
        }
        self.tx.send(job).map_err(|_| "the call pool is closed".to_string())
    }

    fn peak(&self) -> usize {
        self.shared.lock().unwrap().peak
    }
}

fn worker(rx: Arc<Mutex<Receiver<PoolJob>>>, shared: Arc<Mutex<PoolState>>) {
    LIVE_WORKERS.fetch_add(1, Ordering::AcqRel);
    loop {
        shared.lock().unwrap().idle += 1;
        let job = rx.lock().unwrap().recv();
        shared.lock().unwrap().idle -= 1;
        match job {
            Ok(job) => {
                shared.lock().unwrap().queued -= 1;
                job();
            }
            // The sender is gone: the script ended.
            Err(_) => break,
        }
    }
    shared.lock().unwrap().spawned -= 1;
    LIVE_WORKERS.fetch_sub(1, Ordering::AcqRel);
}

/// boa's job executor, replaced: the same queues the engine's default one
/// keeps, drained in the same order, but the script's deadline (and the
/// run's revoke) is looked at before EVERY job, and a native future that
/// stays pending is polled against the deadline instead of being awaited
/// forever. Timeout/interval jobs run at once (nothing in this runtime
/// enqueues them: there is no `setTimeout`).
struct DeadlineExecutor {
    promise_jobs: RefCell<VecDeque<PromiseJob>>,
    generic_jobs: RefCell<VecDeque<GenericJob>>,
    async_jobs: RefCell<VecDeque<NativeAsyncJob>>,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

impl DeadlineExecutor {
    fn new(deadline: Instant, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            promise_jobs: RefCell::default(),
            generic_jobs: RefCell::default(),
            async_jobs: RefCell::default(),
            deadline,
            cancelled,
        }
    }

    fn check(&self) -> JsResult<()> {
        match ended(&self.cancelled, self.deadline) {
            Some(_) => Err(JsNativeError::error().with_message("the script's deadline passed").into()),
            None => Ok(()),
        }
    }

    fn is_empty(&self) -> bool {
        self.promise_jobs.borrow().is_empty() && self.generic_jobs.borrow().is_empty() && self.async_jobs.borrow().is_empty()
    }
}

impl JobExecutor for DeadlineExecutor {
    fn enqueue_job(self: Rc<Self>, job: Job, context: &mut Context) {
        match job {
            Job::PromiseJob(job) => self.promise_jobs.borrow_mut().push_back(job),
            Job::GenericJob(job) => self.generic_jobs.borrow_mut().push_back(job),
            Job::AsyncJob(job) | Job::FinalizationRegistryCleanupJob(job) => self.async_jobs.borrow_mut().push_back(job),
            Job::TimeoutJob(job) => self.generic_jobs.borrow_mut().push_back(GenericJob::new(move |context| job.call(context), context.realm().clone())),
            Job::IntervalJob(job) => self.generic_jobs.borrow_mut().push_back(GenericJob::new(move |context| job.call(context), context.realm().clone())),
            // `Job` is non-exhaustive: a kind a later boa adds has nothing
            // in this runtime that could enqueue it.
            other => log::warn!("[codemode] dropped an unsupported job kind: {other:?}"),
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        let context = RefCell::new(context);
        let mut futures: Vec<Pin<Box<dyn Future<Output = JsResult<JsValue>> + '_>>> = Vec::new();
        let waker = Waker::noop();
        let mut task = TaskContext::from_waker(waker);
        loop {
            self.check()?;
            // Each queue is taken into a local FIRST: a `for` over the
            // `borrow_mut()` temporary would hold the borrow through the
            // loop body, where a job enqueues the next one.
            let async_jobs = std::mem::take(&mut *self.async_jobs.borrow_mut());
            for job in async_jobs {
                futures.push(Box::pin(job.call(&context)));
            }
            let mut failed = None;
            futures.retain_mut(|future| match future.as_mut().poll(&mut task) {
                Poll::Ready(Ok(_)) => false,
                Poll::Ready(Err(err)) => {
                    failed.get_or_insert(err);
                    false
                }
                Poll::Pending => true,
            });
            if let Some(err) = failed {
                return Err(err);
            }
            let promise_jobs = std::mem::take(&mut *self.promise_jobs.borrow_mut());
            for job in promise_jobs {
                self.check()?;
                job.call(&mut context.borrow_mut())?;
            }
            let generic_jobs = std::mem::take(&mut *self.generic_jobs.borrow_mut());
            for job in generic_jobs {
                self.check()?;
                job.call(&mut context.borrow_mut())?;
            }
            context.borrow_mut().clear_kept_objects();
            if self.is_empty() {
                if futures.is_empty() {
                    return Ok(());
                }
                // A native future with nothing to wake it: poll it again
                // shortly, against the deadline.
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

struct Engine;

impl Engine {
    #[allow(clippy::too_many_arguments)]
    fn run(
        script: &str,
        catalog: Arc<Catalog>,
        host: Arc<dyn CallHost>,
        console: Arc<Mutex<Console>>,
        stats: Arc<Mutex<CallStats>>,
        cancelled: Arc<AtomicBool>,
        limits: Limits,
    ) -> EngineReport {
        let (completions_tx, completions_rx) = mpsc::channel::<Completion>();
        let deadline = Instant::now() + limits.timeout;
        ENGINE.with(|slot| {
            *slot.borrow_mut() = Some(EngineState {
                host,
                console,
                stats,
                pending: HashMap::new(),
                sleeps: Vec::new(),
                next_id: 1,
                completions: completions_tx,
                pool: Pool::new(limits.max_in_flight),
                deadline,
                cancelled: cancelled.clone(),
            });
        });
        let outcome = Self::run_inner(script, &catalog, &completions_rx, &cancelled, &limits, deadline);
        // Drop the resolvers, the pool's sender and the completions sender
        // with the state: the workers exit after their current call and a
        // late completion just finds a closed channel.
        let workers = ENGINE.with(|slot| slot.borrow_mut().take().map(|state| state.pool.peak()).unwrap_or(0));
        EngineReport { outcome, workers }
    }

    fn run_inner(
        script: &str,
        catalog: &Catalog,
        completions: &Receiver<Completion>,
        cancelled: &Arc<AtomicBool>,
        limits: &Limits,
        deadline: Instant,
    ) -> Result<(Value, bool), ExecError> {
        let script_error = |err: JsError| ExecError::Script(err.to_string());
        // An error after the deadline (or a revoke) is reported as that,
        // whatever boa says: the natives and the executor refuse then.
        let ended_or = |err: JsError| ended(cancelled, deadline).unwrap_or_else(|| ExecError::Script(err.to_string()));
        let mut context = Context::builder()
            .module_loader(Rc::new(IdleModuleLoader))
            .job_executor(Rc::new(DeadlineExecutor::new(deadline, cancelled.clone())))
            .build()
            .map_err(script_error)?;
        context.runtime_limits_mut().set_loop_iteration_limit(limits.loop_iterations);
        context.runtime_limits_mut().set_recursion_limit(limits.recursion);
        context.runtime_limits_mut().set_stack_size_limit(limits.stack_size);
        context
            .register_global_builtin_callable(js_string!("__codemode_call"), 4, NativeFunction::from_fn_ptr(native_call))
            .map_err(script_error)?;
        context
            .register_global_builtin_callable(js_string!("__codemode_sleep"), 1, NativeFunction::from_fn_ptr(native_sleep))
            .map_err(script_error)?;
        context
            .register_global_builtin_callable(js_string!("__codemode_print"), 2, NativeFunction::from_fn_ptr(native_print))
            .map_err(script_error)?;
        context.eval(Source::from_bytes(prelude(catalog).as_bytes())).map_err(script_error)?;
        // ONE wrapper line before the script: a syntax error's line number is
        // off by exactly one, which `script_error_text` corrects.
        let wrapped = format!("globalThis.__codemode_main = (async () => {{\n{script}\n}})();");
        context
            .eval(Source::from_bytes(wrapped.as_bytes()))
            .map_err(|err| ended(cancelled, deadline).unwrap_or_else(|| ExecError::Script(script_error_text(err.to_string()))))?;
        let main = context
            .global_object()
            .get(js_string!("__codemode_main"), &mut context)
            .map_err(script_error)?;
        let promise = main
            .as_object()
            .ok_or_else(|| ExecError::Script("the script did not produce a promise".into()))
            .and_then(|object| JsPromise::from_object(object).map_err(script_error))?;
        loop {
            context.run_jobs().map_err(ended_or)?;
            match promise.state() {
                PromiseState::Fulfilled(value) => {
                    let json = value.to_json(&mut context).map_err(script_error)?.unwrap_or(Value::Null);
                    return Ok(cap_result(json, limits.result_cap));
                }
                PromiseState::Rejected(reason) => {
                    return Err(ended(cancelled, deadline)
                        .unwrap_or_else(|| ExecError::Script(script_error_text(reason_text(&reason, &mut context)))));
                }
                PromiseState::Pending => {}
            }
            let (pending, next_sleep) = ENGINE.with(|slot| {
                let slot = slot.borrow();
                let state = slot.as_ref();
                (
                    state.map(|state| state.pending.len()).unwrap_or(0),
                    state.and_then(|state| state.sleeps.iter().map(|(due, _)| *due).min()),
                )
            });
            if pending == 0 {
                return Err(ExecError::Script("the script's promise never settles (an await on nothing?)".into()));
            }
            if let Some(end) = ended(cancelled, deadline) {
                return Err(end);
            }
            let now = Instant::now();
            let mut tick = (deadline - now).min(Duration::from_millis(250));
            if let Some(due) = next_sleep {
                tick = tick.min(due.saturating_duration_since(now));
            }
            match completions.recv_timeout(tick) {
                Ok(completion) => {
                    settle(completion, &mut context).map_err(script_error)?;
                    while let Ok(more) = completions.try_recv() {
                        settle(more, &mut context).map_err(script_error)?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ExecError::Script("the call channel closed".into()));
                }
            }
            for id in due_sleeps(Instant::now()) {
                settle((id, Ok(Value::Null)), &mut context).map_err(script_error)?;
            }
        }
    }
}

/// The sleeps due by `now`, taken out of the state.
fn due_sleeps(now: Instant) -> Vec<u64> {
    ENGINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            return Vec::new();
        };
        let (due, later): (Vec<_>, Vec<_>) = state.sleeps.drain(..).partition(|(at, _)| *at <= now);
        state.sleeps = later;
        due.into_iter().map(|(_, id)| id).collect()
    })
}

/// The refusal every native answers once the script is over.
fn ended_error(state: &EngineState) -> JsResult<()> {
    match state.ended() {
        Some(ExecError::Cancelled) => Err(JsNativeError::error().with_message("the run ended").into()),
        Some(_) => Err(JsNativeError::error().with_message("the script's deadline passed").into()),
        None => Ok(()),
    }
}

/// `tools.<server>.<tool>(args)` → a pending promise; the call runs on the
/// pool and lands in the completions channel.
fn native_call(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let server = string_arg(args, 0, context)?;
    let tool = string_arg(args, 1, context)?;
    let payload = match args.get(2) {
        Some(value) if !value.is_undefined() && !value.is_null() => value.to_json(context)?.unwrap_or(json!({})),
        _ => json!({}),
    };
    let raw = args.get(3).is_some_and(JsValue::to_boolean);
    let (promise, resolvers) = JsPromise::new_pending(context);
    ENGINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot.as_mut().expect("the engine state lives for the script");
        ended_error(state)?;
        let id = state.next_id;
        state.next_id += 1;
        state.pending.insert(id, resolvers);
        state.stats.lock().unwrap().count += 1;
        let (host, tx, stats, cancelled, deadline) = (
            state.host.clone(),
            state.completions.clone(),
            state.stats.clone(),
            state.cancelled.clone(),
            state.deadline,
        );
        let job: PoolJob = Box::new(move || {
            // Queued past the end: do not spend the upstream's time on it.
            let out = match ended(&cancelled, deadline) {
                Some(_) => Err("the script's deadline passed before the call started".to_string()),
                None => host.call(&server, &tool, payload).and_then(|result| shape(result, raw)),
            };
            if out.is_err() {
                stats.lock().unwrap().failed += 1;
            }
            let _ = tx.send((id, out));
        });
        if let Err(why) = state.pool.submit(job) {
            // No thread could run it: the promise rejects with the reason
            // at once instead of the script waiting for a timeout.
            if let Some(resolvers) = state.pending.remove(&id) {
                state.stats.lock().unwrap().failed += 1;
                let error = JsNativeError::error().with_message(why).into_opaque(context);
                resolvers.reject.call(&JsValue::undefined(), &[error.into()], context)?;
            }
        }
        Ok::<_, JsError>(())
    })?;
    Ok(promise.into())
}

/// `sleep(ms)`: a promise the run loop settles when it is due. Capped at
/// what is left of the script's own deadline, so a sleep never outlives it.
fn native_sleep(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ms = args.first().map(|value| value.to_number(context)).transpose()?.unwrap_or(0.0);
    let ms = if ms.is_finite() && ms > 0.0 { ms as u64 } else { 0 };
    let (promise, resolvers) = JsPromise::new_pending(context);
    ENGINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot.as_mut().expect("the engine state lives for the script");
        ended_error(state)?;
        let id = state.next_id;
        state.next_id += 1;
        state.pending.insert(id, resolvers);
        let now = Instant::now();
        let wait = Duration::from_millis(ms).min(state.deadline.saturating_duration_since(now));
        state.sleeps.push((now + wait, id));
        Ok::<_, JsError>(())
    })?;
    Ok(promise.into())
}

/// `console.log(...)`: one line into the capped buffer.
fn native_print(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let text = string_arg(args, 1, context)?;
    ENGINE.with(|slot| {
        let slot = slot.borrow();
        let Some(state) = slot.as_ref() else {
            return Ok(());
        };
        ended_error(state)?;
        state.console.lock().unwrap().push(&text);
        Ok::<_, JsError>(())
    })?;
    Ok(JsValue::undefined())
}

fn string_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    Ok(args
        .get(index)
        .map(|value| value.to_string(context))
        .transpose()?
        .map(|text| text.to_std_string_escaped())
        .unwrap_or_default())
}

/// Settle the promise `id` with what its thread produced.
fn settle((id, out): Completion, context: &mut Context) -> JsResult<()> {
    let Some(resolvers) = ENGINE.with(|slot| slot.borrow_mut().as_mut().and_then(|state| state.pending.remove(&id)))
    else {
        return Ok(());
    };
    match out {
        Ok(value) => {
            let value = JsValue::from_json(&value, context)?;
            resolvers.resolve.call(&JsValue::undefined(), &[value], context)?;
        }
        Err(text) => {
            let error = JsNativeError::error().with_message(text).into_opaque(context);
            resolvers.reject.call(&JsValue::undefined(), &[error.into()], context)?;
        }
    }
    Ok(())
}

/// What a script gets back from one call. `raw` = the whole MCP result with
/// images replaced by a note (a screenshot is tens of KB of base64 the
/// script could never use); otherwise the useful part: `isError` REJECTS
/// with the text (so `Promise.allSettled` can tolerate it),
/// `structuredContent` wins, one text item that parses as JSON is parsed,
/// else the concatenated text.
pub fn shape(result: Value, raw: bool) -> Result<Value, String> {
    if raw {
        return Ok(strip_images(result));
    }
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                    Some("text") => item.get("text").and_then(Value::as_str).map(str::to_string),
                    Some("image") => Some(image_note(item)),
                    Some(other) => Some(format!("[{other} omitted]")),
                    None => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        return Err(if text.is_empty() { "the tool refused the call".to_string() } else { text });
    }
    if let Some(structured) = result.get("structuredContent") {
        if !structured.is_null() {
            return Ok(structured.clone());
        }
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(text.trim()) {
        if parsed.is_object() || parsed.is_array() {
            return Ok(parsed);
        }
    }
    Ok(Value::String(text))
}

fn image_note(item: &Value) -> String {
    let bytes = item.get("data").and_then(Value::as_str).map(|data| data.len() * 3 / 4).unwrap_or(0);
    format!("[image omitted; {bytes} bytes]")
}

fn strip_images(mut result: Value) -> Value {
    if let Some(items) = result.get_mut("content").and_then(Value::as_array_mut) {
        for item in items.iter_mut() {
            if item.get("type").and_then(Value::as_str) == Some("image") {
                let note = image_note(item);
                let mime = item.get("mimeType").cloned().unwrap_or(Value::Null);
                *item = json!({ "type": "image", "note": note, "mimeType": mime });
            }
        }
    }
    result
}

/// The return value, cut at its cap (a note replaces the overflow).
fn cap_result(value: Value, cap: usize) -> (Value, bool) {
    let serialized = value.to_string();
    if serialized.len() <= cap {
        return (value, false);
    }
    let mut end = cap;
    while !serialized.is_char_boundary(end) {
        end -= 1;
    }
    let note = format!("{}… [result truncated, {} more bytes]", &serialized[..end], serialized.len() - end);
    (Value::String(note), true)
}

fn reason_text(reason: &JsValue, context: &mut Context) -> String {
    if let Some(object) = reason.as_object() {
        let message = object.get(js_string!("message"), context).ok().filter(|value| !value.is_undefined());
        let name = object.get(js_string!("name"), context).ok().filter(|value| !value.is_undefined());
        if let Some(message) = message {
            let message = message.to_string(context).map(|text| text.to_std_string_escaped()).unwrap_or_default();
            let name = name
                .and_then(|name| name.to_string(context).ok())
                .map(|text| text.to_std_string_escaped())
                .unwrap_or_else(|| "Error".to_string());
            return format!("{name}: {message}");
        }
    }
    reason
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .unwrap_or_else(|_| "the script rejected".to_string())
}

/// boa reports a syntax error's position as `at line N, col M`; the script
/// starts on line 2 of the wrapper, so N - 1 is the script's own line.
fn script_error_text(text: String) -> String {
    let Some(at) = text.find("at line ") else {
        return text;
    };
    let digits_start = at + "at line ".len();
    let digits: String = text[digits_start..].chars().take_while(char::is_ascii_digit).collect();
    match digits.parse::<u64>() {
        Ok(line) if line > 1 => {
            format!("{}at line {}{}", &text[..at], line - 1, &text[digits_start + digits.len()..])
        }
        _ => text,
    }
}

/// The JS that turns the catalog into `tools`, plus `console`, `sleep` and
/// `ALL_TOOLS`. Catalog data lands as JSON literals, never spliced text.
fn prelude(catalog: &Catalog) -> String {
    let pairs: Vec<(String, String)> =
        catalog.tools.iter().map(|tool| (tool.server.clone(), tool.name.clone())).collect();
    let pairs = serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".to_string());
    let all_tools = serde_json::to_string(&catalog.all_tools_text()).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"(() => {{
const __pairs = {pairs};
const __t = Object.create(null);
for (const [server, name] of __pairs) {{
  const f = (args) => __codemode_call(server, name, args ?? {{}}, false);
  f.raw = (args) => __codemode_call(server, name, args ?? {{}}, true);
  __t["mcp__" + server + "__" + name] = f;
  (__t[server] ??= Object.create(null))[name] = f;
}}
for (const server of Object.keys(__t)) {{ if (typeof __t[server] === "object") Object.freeze(__t[server]); }}
globalThis.tools = Object.freeze(__t);
globalThis.sleep = (ms) => __codemode_sleep(Number(ms) || 0);
const __fmt = (a) => a.map((x) => {{
  if (typeof x === "string") return x;
  if (x instanceof Error) return String(x);
  try {{ return JSON.stringify(x); }} catch {{ return String(x); }}
}}).join(" ");
globalThis.console = Object.freeze({{
  log: (...a) => __codemode_print(1, __fmt(a)),
  info: (...a) => __codemode_print(1, __fmt(a)),
  warn: (...a) => __codemode_print(2, __fmt(a)),
  error: (...a) => __codemode_print(2, __fmt(a)),
}});
globalThis.ALL_TOOLS = {all_tools};
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ToolSpec;
    use std::sync::Mutex;

    /// Records every call's window; `slow` sleeps `ms`, `fail` refuses,
    /// `img` answers an image, `echo` answers its arguments as structured
    /// content, `text` answers JSON text.
    struct FakeHost {
        calls: Mutex<Vec<(String, Instant, Instant)>>,
    }

    impl FakeHost {
        fn new() -> Arc<Self> {
            Arc::new(Self { calls: Mutex::default() })
        }

        fn max_concurrency(&self) -> usize {
            let calls = self.calls.lock().unwrap();
            calls
                .iter()
                .map(|(_, start, _)| calls.iter().filter(|(_, s, e)| s <= start && start < e).count())
                .max()
                .unwrap_or(0)
        }
    }

    impl CallHost for FakeHost {
        fn call(&self, _server: &str, tool: &str, args: Value) -> Result<Value, String> {
            let start = Instant::now();
            let result = match tool {
                "slow" => {
                    let ms = args.get("ms").and_then(Value::as_u64).unwrap_or(100);
                    std::thread::sleep(Duration::from_millis(ms));
                    Ok(json!({ "content": [{ "type": "text", "text": format!("slept {ms}") }] }))
                }
                "fail" => Ok(json!({ "content": [{ "type": "text", "text": "no such window" }], "isError": true })),
                "img" => Ok(json!({ "content": [
                    { "type": "text", "text": "a frame" },
                    { "type": "image", "data": "AAAA".repeat(300), "mimeType": "image/png" }
                ] })),
                "echo" => Ok(json!({ "content": [{ "type": "text", "text": "ok" }], "structuredContent": args })),
                "text" => Ok(json!({ "content": [{ "type": "text", "text": "{\"windows\":[1,2]}" }] })),
                "big" => Ok(json!({ "content": [{ "type": "text", "text": "x".repeat(100_000) }] })),
                other => Err(format!("tool_unavailable: {other}")),
            };
            self.calls.lock().unwrap().push((tool.to_string(), start, Instant::now()));
            result
        }
    }

    fn catalog() -> Arc<Catalog> {
        Arc::new(Catalog {
            tools: ["slow", "fail", "img", "echo", "text", "big"]
                .into_iter()
                .map(|name| ToolSpec {
                    server: "t".into(),
                    name: name.into(),
                    description: format!("the {name} tool"),
                    input_schema: json!({ "type": "object" }),
                })
                .collect(),
            unreachable: Vec::new(),
            direct_only: vec!["playwright".into()],
        })
    }

    fn run(script: &str, host: &Arc<FakeHost>) -> ExecOutcome {
        run_with(script, host, Limits::default())
    }

    fn run_with(script: &str, host: &Arc<FakeHost>, limits: Limits) -> ExecOutcome {
        let host: Arc<dyn CallHost> = host.clone();
        run_script(script, catalog(), host, Arc::new(AtomicBool::new(false)), limits)
    }

    /// THE point of the design: awaited calls under `Promise.all` overlap.
    #[test]
    fn two_calls_under_promise_all_overlap() {
        let host = FakeHost::new();
        let outcome = run(
            "const [a, b] = await Promise.all([tools.t.slow({ms: 300}), tools.mcp__t__slow({ms: 300})]);\nreturn [a, b];",
            &host,
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert!(outcome.elapsed < Duration::from_millis(550), "took {:?}", outcome.elapsed);
        assert_eq!(host.max_concurrency(), 2);
        assert_eq!(outcome.result, json!(["slept 300", "slept 300"]));
        assert_eq!(outcome.calls, CallStats { count: 2, failed: 0 });
        assert_eq!(outcome.workers, 2);

        let sequential = run("await tools.t.slow({ms: 300}); await tools.t.slow({ms: 300}); return 1;", &host);
        assert!(sequential.elapsed >= Duration::from_millis(600), "took {:?}", sequential.elapsed);
    }

    #[test]
    fn in_flight_is_capped() {
        let host = FakeHost::new();
        let outcome = run_with(
            "await Promise.all(Array.from({length: 40}, () => tools.t.slow({ms: 40}))); return 'done';",
            &host,
            Limits { max_in_flight: 4, ..Limits::default() },
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert!(host.max_concurrency() <= 4, "{}", host.max_concurrency());
        assert_eq!(outcome.workers, 4);
        assert_eq!(outcome.calls.count, 40);
    }

    /// F30: thousands of pending calls = `max_in_flight` threads, not
    /// thousands, and the pool's threads are gone once the script is.
    #[test]
    fn two_thousand_pending_calls_use_a_bounded_pool_that_ends_with_the_script() {
        let host = FakeHost::new();
        let outcome = run_with(
            "const r = await Promise.all(Array.from({length: 2000}, (_, i) => tools.t.echo({i}))); return r.length;",
            &host,
            Limits { max_in_flight: 8, ..Limits::default() },
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(outcome.result, json!(2000));
        assert_eq!(outcome.calls, CallStats { count: 2000, failed: 0 });
        assert!(outcome.workers <= 8, "{} threads", outcome.workers);
        assert!(host.max_concurrency() <= 8, "{}", host.max_concurrency());
        let waited = Instant::now();
        while LIVE_WORKERS.load(Ordering::Acquire) > 0 {
            assert!(waited.elapsed() < Duration::from_secs(10), "pool threads outlived the script");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn console_return_value_and_shaping() {
        let host = FakeHost::new();
        let outcome = run(
            r#"console.log("start", {a: 1});
const echoed = await tools.t.echo({x: 2});
const parsed = await tools.t.text({});
console.error("warned");
return { echoed, parsed, n: parsed.windows.length };"#,
            &host,
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(outcome.console, "start {\"a\":1}\nwarned\n");
        assert_eq!(outcome.result, json!({ "echoed": { "x": 2 }, "parsed": { "windows": [1, 2] }, "n": 2 }));
        assert!(!outcome.truncated);
    }

    #[test]
    fn a_refusal_rejects_and_all_settled_tolerates_it() {
        let host = FakeHost::new();
        let outcome = run(
            r#"const settled = await Promise.allSettled([tools.t.fail({}), tools.t.echo({ok: true})]);
return settled.map((s) => s.status === "rejected" ? "rejected: " + s.reason.message : s.value);"#,
            &host,
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(outcome.result, json!(["rejected: no such window", { "ok": true }]));
        assert_eq!(outcome.calls, CallStats { count: 2, failed: 1 });

        let thrown = run("await tools.t.fail({}); return 1;", &host);
        assert_eq!(thrown.error, Some(ExecError::Script("Error: no such window".into())));
    }

    #[test]
    fn images_become_notes_and_raw_returns_the_whole_result() {
        let host = FakeHost::new();
        let outcome = run("return [await tools.t.img({}), await tools.t.img.raw({})];", &host);
        assert_eq!(outcome.error, None, "{outcome:?}");
        let result = outcome.result.as_array().unwrap();
        assert_eq!(result[0], json!("a frame\n[image omitted; 900 bytes]"));
        assert_eq!(result[1]["content"][1]["note"], json!("[image omitted; 900 bytes]"));
        assert_eq!(result[1]["content"][1]["mimeType"], json!("image/png"));
        assert!(result[1]["content"][1].get("data").is_none());
    }

    #[test]
    fn unknown_tools_reject_and_the_catalog_is_visible() {
        let host = FakeHost::new();
        let outcome = run(
            r#"const names = Object.keys(tools).filter((k) => k.startsWith("mcp__")).sort();
try { await tools.nope.x({}); } catch (e) { console.log("caught", String(e)); }
return { names, hasAll: ALL_TOOLS.includes("mcp__t__slow"), direct: ALL_TOOLS.includes("playwright") };"#,
            &host,
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(outcome.result["hasAll"], json!(true));
        assert_eq!(outcome.result["direct"], json!(true));
        assert_eq!(outcome.result["names"].as_array().unwrap().len(), 6);
        assert!(outcome.console.contains("caught TypeError"), "{}", outcome.console);
    }

    /// F28: the context has no module loader, so `import()` cannot read a
    /// file next to the host's cwd (or anywhere), JSON attribute or not.
    #[test]
    fn imports_reject_instead_of_reading_host_files() {
        let host = FakeHost::new();
        for script in [
            "await import('/etc/hosts'); return 1;",
            "await import('/etc/hosts', { with: { type: 'json' } }); return 1;",
            "await import('./helper.js'); return 1;",
            "const m = await import('../Cargo.toml', { with: { type: 'json' } }); return m;",
        ] {
            let outcome = run(script, &host);
            let Some(ExecError::Script(text)) = &outcome.error else { panic!("{script}: {outcome:?}") };
            assert!(text.contains("module resolution is disabled"), "{script}: {text}");
            assert_eq!(outcome.result, Value::Null);
        }
        let caught = run(
            "try { await import('/etc/hosts'); } catch (e) { return String(e); } return 'read it';",
            &host,
        );
        assert_eq!(caught.error, None, "{caught:?}");
        assert!(caught.result.as_str().unwrap().contains("disabled"), "{caught:?}");
    }

    #[test]
    fn a_syntax_error_names_the_scripts_own_line() {
        let host = FakeHost::new();
        let outcome = run("const a = 1;\nconst b = ;\nreturn a;", &host);
        let Some(ExecError::Script(text)) = outcome.error else { panic!("{outcome:?}") };
        assert!(text.contains("at line 2"), "{text}");
    }

    #[test]
    fn a_hot_loop_ends_at_the_iteration_limit() {
        let host = FakeHost::new();
        let outcome = run_with(
            "let i = 0; while (true) { i++; } return i;",
            &host,
            Limits { loop_iterations: 100_000, ..Limits::default() },
        );
        assert!(matches!(outcome.error, Some(ExecError::Script(_))), "{outcome:?}");
    }

    /// F29: a synchronous spin cannot be interrupted, but the caller still
    /// gets its timeout at the deadline, and the abandoned engine thread
    /// ends at boa's per-frame ceiling. (A spin that re-enters a helper on
    /// every iteration gets a fresh ceiling per call and cannot be bounded
    /// this way; that residual is documented, not asserted.)
    #[test]
    fn a_sync_spin_times_out_at_the_deadline_and_its_thread_ends_at_the_frame_ceiling() {
        let host = FakeHost::new();
        let host_dyn: Arc<dyn CallHost> = host.clone();
        let (outcome, engine_exited) = run_script_observed(
            "while (true) {} return 1;",
            catalog(),
            host_dyn,
            Arc::new(AtomicBool::new(false)),
            Limits { timeout: Duration::from_millis(300), loop_iterations: 20_000_000, ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::Timeout));
        assert!(outcome.elapsed < Duration::from_millis(1300), "{:?}", outcome.elapsed);
        let waited = Instant::now();
        while !engine_exited.load(Ordering::Acquire) {
            assert!(waited.elapsed() < Duration::from_secs(120), "the engine thread never ended");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// F29: an async spin (`await` on every iteration) is stopped by the
    /// executor at the deadline, so the engine thread ends with the call.
    #[test]
    fn an_async_spin_ends_at_the_deadline() {
        let host = FakeHost::new();
        let host_dyn: Arc<dyn CallHost> = host.clone();
        let (outcome, engine_exited) = run_script_observed(
            "let i = 0; while (true) { await null; i++; } return i;",
            catalog(),
            host_dyn,
            Arc::new(AtomicBool::new(false)),
            Limits { timeout: Duration::from_millis(300), ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::Timeout));
        assert!(outcome.elapsed < Duration::from_millis(1300), "{:?}", outcome.elapsed);
        let waited = Instant::now();
        while !engine_exited.load(Ordering::Acquire) {
            assert!(waited.elapsed() < Duration::from_secs(5), "the engine thread outlived the deadline");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// F29: a native called after the deadline refuses, so a helper that
    /// spins between awaited calls cannot keep spending the upstream.
    #[test]
    fn calls_after_the_deadline_are_refused_and_the_thread_ends() {
        let host = FakeHost::new();
        let host_dyn: Arc<dyn CallHost> = host.clone();
        let (outcome, engine_exited) = run_script_observed(
            "await tools.t.slow({ms: 600}); await tools.t.echo({late: true}); return 1;",
            catalog(),
            host_dyn,
            Arc::new(AtomicBool::new(false)),
            Limits { timeout: Duration::from_millis(300), ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::Timeout));
        let waited = Instant::now();
        while !engine_exited.load(Ordering::Acquire) {
            assert!(waited.elapsed() < Duration::from_secs(5), "the engine thread outlived the call");
            std::thread::sleep(Duration::from_millis(20));
        }
        let calls = host.calls.lock().unwrap();
        assert!(calls.iter().all(|(tool, _, _)| tool == "slow"), "the late echo ran: {calls:?}");
    }

    #[test]
    fn the_timeout_returns_the_console_so_far() {
        let host = FakeHost::new();
        let outcome = run_with(
            "console.log('before'); await sleep(5000); console.log('after'); return 1;",
            &host,
            Limits { timeout: Duration::from_millis(400), ..Limits::default() },
        );
        assert_eq!(outcome.error, Some(ExecError::Timeout));
        assert_eq!(outcome.console, "before\n");
        assert!(outcome.elapsed < Duration::from_millis(1500), "{:?}", outcome.elapsed);
    }

    #[test]
    fn sleeps_settle_in_order_without_a_thread_each() {
        let host = FakeHost::new();
        let outcome = run(
            "const order = []; await Promise.all([sleep(120).then(() => order.push(120)), sleep(40).then(() => order.push(40)), sleep(80).then(() => order.push(80))]); await sleep(30); return order;",
            &host,
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert_eq!(outcome.result, json!([40, 80, 120]));
        assert!(outcome.elapsed >= Duration::from_millis(150), "{:?}", outcome.elapsed);
        assert!(outcome.elapsed < Duration::from_millis(600), "{:?}", outcome.elapsed);
        assert_eq!(outcome.workers, 0);
    }

    #[test]
    fn a_revoked_run_cancels_the_script() {
        let host = FakeHost::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::Release);
        });
        let host_dyn: Arc<dyn CallHost> = host.clone();
        let outcome = run_script("await sleep(5000); return 1;", catalog(), host_dyn, cancelled, Limits::default());
        assert_eq!(outcome.error, Some(ExecError::Cancelled));
        assert!(outcome.elapsed < Duration::from_millis(1500), "{:?}", outcome.elapsed);
    }

    #[test]
    fn output_is_capped_with_a_note() {
        let host = FakeHost::new();
        let outcome = run_with(
            "for (let i = 0; i < 50; i++) console.log('line ' + i + ' ' + 'y'.repeat(100));\nreturn await tools.t.big({});",
            &host,
            Limits { console_cap: 1000, result_cap: 500, ..Limits::default() },
        );
        assert_eq!(outcome.error, None, "{outcome:?}");
        assert!(outcome.truncated);
        assert!(outcome.console.ends_with("more bytes]\n"), "{}", outcome.console);
        assert!(outcome.console.len() < 1100);
        let result = outcome.result.as_str().unwrap();
        assert!(result.contains("[result truncated"), "{result}");
    }

    #[test]
    fn shape_picks_structured_then_json_then_text() {
        assert_eq!(shape(json!({ "content": [{ "type": "text", "text": "hi" }] }), false), Ok(json!("hi")));
        assert_eq!(
            shape(json!({ "content": [{ "type": "text", "text": "[1,2]" }], "structuredContent": { "a": 1 } }), false),
            Ok(json!({ "a": 1 }))
        );
        assert_eq!(shape(json!({ "content": [{ "type": "text", "text": " [1,2] " }] }), false), Ok(json!([1, 2])));
        assert_eq!(shape(json!({ "content": [{ "type": "text", "text": "42" }] }), false), Ok(json!("42")));
        assert_eq!(shape(json!({ "content": [], "isError": true }), false), Err("the tool refused the call".into()));
    }

    #[test]
    fn syntax_error_lines_shift_by_the_wrapper_line() {
        assert_eq!(script_error_text("SyntaxError: x at line 3, col 9".into()), "SyntaxError: x at line 2, col 9");
        assert_eq!(script_error_text("TypeError: y".into()), "TypeError: y");
    }
}
