//! EXP-1236: the script engine behind `exec`. One boa `Context` per script,
//! on its own thread, with an event loop driven BY HAND: every
//! `tools.<server>.<tool>(args)` the script awaits becomes a pending JS
//! promise plus a std thread doing the (blocking) MCP call; the loop drains
//! microtasks, waits for completions and settles promises as they land, so
//! N awaited calls under `Promise.all` really run at once. The engine's own
//! job executor is never relied on for that (it blocks on async jobs).
//!
//! Nothing else reaches the script: no filesystem, no network, no timers
//! besides `sleep`, no imports. Output = what the script printed plus the
//! value it returned, capped, so a script that reads a hundred windows puts
//! one summary into the model's context instead of a hundred screenshots.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use boa_engine::builtins::promise::{PromiseState, ResolvingFunctions};
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
    /// Nested calls in flight at once; the rest queue.
    pub max_in_flight: usize,
    /// Bytes of console output kept.
    pub console_cap: usize,
    /// Bytes of the serialized return value kept.
    pub result_cap: usize,
    /// boa's per-loop iteration ceiling: a `while (true) {}` ends here.
    pub loop_iterations: u64,
    /// boa's recursion ceiling.
    pub recursion: usize,
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
}

impl ExecOutcome {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }
}

/// Run `script` to completion (or to a limit) and report. Blocks the caller
/// for at most `limits.timeout` (plus a grace); the engine thread is
/// abandoned on timeout, which the loop-iteration ceiling bounds.
pub fn run_script(
    script: &str,
    catalog: Arc<Catalog>,
    host: Arc<dyn CallHost>,
    cancelled: Arc<AtomicBool>,
    limits: Limits,
) -> ExecOutcome {
    let started = Instant::now();
    let console = Arc::new(Mutex::new(Console::new(limits.console_cap)));
    let stats = Arc::new(Mutex::new(CallStats::default()));
    let (done_tx, done_rx) = mpsc::channel::<Result<(Value, bool), ExecError>>();
    let thread = {
        let script = script.to_string();
        let console = console.clone();
        let stats = stats.clone();
        let cancelled = cancelled.clone();
        let limits = limits.clone();
        std::thread::Builder::new().name("codemode-script".into()).spawn(move || {
            let outcome = Engine::run(&script, catalog, host, console, stats, cancelled, limits);
            let _ = done_tx.send(outcome);
        })
    };
    let outcome = match thread {
        Ok(_) => match done_rx.recv_timeout(limits.timeout) {
            Ok(outcome) => outcome,
            Err(_) => {
                // Stop everything still in flight from feeding a script
                // nobody waits for; the engine thread sees the flag at its
                // next tick and returns.
                cancelled.store(true, Ordering::Release);
                Err(ExecError::Timeout)
            }
        },
        Err(err) => Err(ExecError::Script(format!("could not start the script thread: {err}"))),
    };
    let (console_text, console_cut) = console.lock().unwrap().take();
    let calls = stats.lock().unwrap().clone();
    let (result, error, result_cut) = match outcome {
        Ok((value, cut)) => (value, None, cut),
        Err(error) => (Value::Null, Some(error), false),
    };
    ExecOutcome {
        result,
        console: console_text,
        error,
        calls,
        truncated: console_cut || result_cut,
        elapsed: started.elapsed(),
    }
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

/// The engine's per-thread state the native functions reach through
/// [`ENGINE`] (boa natives are plain `fn` pointers; captures would need the
/// GC's `Trace`).
struct EngineState {
    host: Arc<dyn CallHost>,
    console: Arc<Mutex<Console>>,
    stats: Arc<Mutex<CallStats>>,
    pending: HashMap<u64, ResolvingFunctions>,
    next_id: u64,
    completions: Sender<Completion>,
    gate: Arc<Gate>,
    deadline: Instant,
}

thread_local! {
    static ENGINE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

/// A counting semaphore for the calls in flight.
struct Gate {
    limit: usize,
    busy: Mutex<usize>,
    freed: Condvar,
}

impl Gate {
    fn acquire(&self) {
        let mut busy = self.busy.lock().unwrap();
        while *busy >= self.limit {
            busy = self.freed.wait(busy).unwrap();
        }
        *busy += 1;
    }

    fn release(&self) {
        *self.busy.lock().unwrap() -= 1;
        self.freed.notify_one();
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
    ) -> Result<(Value, bool), ExecError> {
        let (completions_tx, completions_rx) = mpsc::channel::<Completion>();
        let deadline = Instant::now() + limits.timeout;
        ENGINE.with(|slot| {
            *slot.borrow_mut() = Some(EngineState {
                host,
                console,
                stats,
                pending: HashMap::new(),
                next_id: 1,
                completions: completions_tx,
                gate: Arc::new(Gate {
                    limit: limits.max_in_flight.max(1),
                    busy: Mutex::new(0),
                    freed: Condvar::new(),
                }),
                deadline,
            });
        });
        let outcome = Self::run_inner(script, &catalog, &completions_rx, &cancelled, &limits, deadline);
        // Drop the resolvers (and the sender) with the state so a late
        // completion thread just finds a closed channel.
        ENGINE.with(|slot| slot.borrow_mut().take());
        outcome
    }

    fn run_inner(
        script: &str,
        catalog: &Catalog,
        completions: &Receiver<Completion>,
        cancelled: &AtomicBool,
        limits: &Limits,
        deadline: Instant,
    ) -> Result<(Value, bool), ExecError> {
        let mut context = Context::default();
        context.runtime_limits_mut().set_loop_iteration_limit(limits.loop_iterations);
        context.runtime_limits_mut().set_recursion_limit(limits.recursion);
        let script_error = |err: JsError| ExecError::Script(err.to_string());
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
            .map_err(|err| ExecError::Script(script_error_text(err.to_string())))?;
        let main = context
            .global_object()
            .get(js_string!("__codemode_main"), &mut context)
            .map_err(script_error)?;
        let promise = main
            .as_object()
            .ok_or_else(|| ExecError::Script("the script did not produce a promise".into()))
            .and_then(|object| JsPromise::from_object(object).map_err(script_error))?;
        loop {
            context.run_jobs().map_err(script_error)?;
            match promise.state() {
                PromiseState::Fulfilled(value) => {
                    let json = value.to_json(&mut context).map_err(script_error)?.unwrap_or(Value::Null);
                    return Ok(cap_result(json, limits.result_cap));
                }
                PromiseState::Rejected(reason) => {
                    return Err(ExecError::Script(script_error_text(reason_text(&reason, &mut context))));
                }
                PromiseState::Pending => {}
            }
            let pending = ENGINE.with(|slot| slot.borrow().as_ref().map(|state| state.pending.len()).unwrap_or(0));
            if pending == 0 {
                return Err(ExecError::Script("the script's promise never settles (an await on nothing?)".into()));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(ExecError::Cancelled);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(ExecError::Timeout);
            }
            let tick = (deadline - now).min(Duration::from_millis(250));
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
        }
    }
}

/// `tools.<server>.<tool>(args)` → a pending promise; the call runs on its
/// own thread and lands in the completions channel.
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
        let id = state.next_id;
        state.next_id += 1;
        state.pending.insert(id, resolvers);
        state.stats.lock().unwrap().count += 1;
        let (host, tx, gate, stats) =
            (state.host.clone(), state.completions.clone(), state.gate.clone(), state.stats.clone());
        std::thread::spawn(move || {
            gate.acquire();
            let out = host.call(&server, &tool, payload).and_then(|result| shape(result, raw));
            gate.release();
            if out.is_err() {
                stats.lock().unwrap().failed += 1;
            }
            let _ = tx.send((id, out));
        });
    });
    Ok(promise.into())
}

/// `sleep(ms)`: a promise a thread settles later. Capped at what is left of
/// the script's own deadline, so a sleep never outlives it.
fn native_sleep(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ms = args.first().map(|value| value.to_number(context)).transpose()?.unwrap_or(0.0);
    let ms = if ms.is_finite() && ms > 0.0 { ms as u64 } else { 0 };
    let (promise, resolvers) = JsPromise::new_pending(context);
    ENGINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot.as_mut().expect("the engine state lives for the script");
        let id = state.next_id;
        state.next_id += 1;
        state.pending.insert(id, resolvers);
        let tx = state.completions.clone();
        let wait = Duration::from_millis(ms).min(state.deadline.saturating_duration_since(Instant::now()));
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            let _ = tx.send((id, Ok(Value::Null)));
        });
    });
    Ok(promise.into())
}

/// `console.log(...)`: one line into the capped buffer.
fn native_print(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let text = string_arg(args, 1, context)?;
    ENGINE.with(|slot| {
        if let Some(state) = slot.borrow().as_ref() {
            state.console.lock().unwrap().push(&text);
        }
    });
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
        assert_eq!(outcome.calls.count, 40);
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
