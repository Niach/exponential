//! C ABI surface. Mirrors `include/agent_core.h` exactly.
//!
//! The boundary is synchronous and thread-safe: each call returns immediately,
//! work happens on the core's own background thread(s), and outbound events flow
//! back through the single registered callback. The callback may fire on a
//! background thread — the host is responsible for marshalling onto its UI thread
//! (DispatchQueue.main on macOS, g_idle_add on GTK).

use crate::agent_run;
use crate::dispatcher::Dispatcher;
use crate::run_pipeline::{self, Config, Emit, HostEvent};
use crate::state::State;
use crate::{electric, pr_poll};
use serde::Deserialize;
use serde_json::json;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Return code: success.
const OK: c_int = 0;
/// Return code: a null/invalid handle was passed.
const ERR_INVALID_HANDLE: c_int = -1;
/// Return code: the operation is not implemented in this milestone.
const ERR_NOT_IMPLEMENTED: c_int = -2;
/// Return code: bad/missing config (couldn't start the loop).
const ERR_CONFIG: c_int = -3;

/// The host's `agent_core_create` config JSON (camelCase). `apiKey` is the
/// agent `expk_` (MCP + assigned-issues shape); `githubToken` powers git/PRs.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoreConfigDto {
    base_url: String,
    api_key: String,
    bot_user_id: String,
    #[serde(default)]
    github_token: String,
    repos_root: String,
    worktrees_root: String,
    #[serde(default = "default_prefix")]
    branch_prefix: String,
    #[serde(default = "default_driver")]
    driver: String,
    db_path: String,
    #[serde(default = "default_concurrency")]
    max_concurrent: usize,
    #[serde(default = "default_timeout")]
    timeout_s: u64,
    /// Wall-clock cap for headless runs (seconds). Interactive sessions are
    /// untimed (the user owns them).
    #[serde(default = "default_run_timeout")]
    run_timeout_s: u64,
    /// Route the claude driver's plan/code stages through the host terminal as
    /// live interactive sessions (the launch default).
    #[serde(default = "default_true")]
    interactive: bool,
}
fn default_prefix() -> String { "agent".into() }
fn default_driver() -> String { "claude".into() }
fn default_concurrency() -> usize { 2 }
fn default_timeout() -> u64 { 30 }
fn default_run_timeout() -> u64 { agent_run::HEADLESS_RUN_TIMEOUT_S }
fn default_true() -> bool { true }

/// Live loop handles, set on start, torn down on stop.
struct Runtime {
    stop: Arc<AtomicBool>,
    dispatcher: Dispatcher,
    #[allow(dead_code)] // kept alive; the loops exit on their own poll cycle
    workers: Vec<JoinHandle<()>>,
    // Kept so the host-triggered interactive entry points can run a pipeline
    // slice (build worktree + emit an interactive run_request).
    config: Arc<Config>,
    state: Arc<Mutex<State>>,
    emit: run_pipeline::Emit,
    // The single interactive-session slot (the host dock mounts exactly one
    // terminal) — shared by the dispatcher pipeline and the host-triggered
    // entry points so a second session can never clobber the mounted one.
    interactive_slot: Arc<run_pipeline::InteractiveSlot>,
}

/// Matches `AgentCoreEventCallback` in the C header. `Option<..>` so a NULL
/// function pointer maps cleanly to "no callback".
type EventCallback = extern "C" fn(ctx: *mut c_void, event_json: *const c_char, len: usize);

/// Holds the host's callback + opaque context pointer. The context is an opaque
/// host object (a Swift/Zig pointer); we never deref it, only hand it back.
struct CallbackSlot {
    cb: Option<EventCallback>,
    ctx: *mut c_void,
}
// SAFETY: `ctx` is an opaque pointer we only pass back to the host's callback;
// the host owns its lifetime and thread-safety. We guard access with a Mutex.
unsafe impl Send for CallbackSlot {}

/// The opaque handle behind `AgentCore*` in C.
pub struct AgentCore {
    callback: Arc<Mutex<CallbackSlot>>,
    /// Parsed pipeline config (None if the config JSON was missing/invalid).
    config: Option<Arc<Config>>,
    db_path: String,
    max_concurrent: usize,
    runtime: Mutex<Option<Runtime>>,
}

/// Emit one JSON event to the host. The C string is borrowed for the call only.
fn emit(slot: &Arc<Mutex<CallbackSlot>>, json: &str) {
    let Ok(guard) = slot.lock() else { return };
    let Some(cb) = guard.cb else { return };
    // Interior NULs would be a bug in our own JSON; default to empty if so.
    let c = CString::new(json).unwrap_or_default();
    cb(guard.ctx, c.as_ptr(), json.len());
}

/// SAFETY: `p` must be NULL or a valid NUL-terminated C string.
unsafe fn cstr_to_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok().map(|s| s.to_owned())
}

#[no_mangle]
pub extern "C" fn agent_core_create(config_json: *const c_char) -> *mut AgentCore {
    let raw = unsafe { cstr_to_string(config_json) }.unwrap_or_default();
    let parsed: Option<CoreConfigDto> = serde_json::from_str(&raw).ok();
    let (config, db_path, max_concurrent) = match parsed {
        Some(dto) => {
            let cfg = Config {
                base_url: dto.base_url,
                api_key: dto.api_key,
                bot_user_id: dto.bot_user_id,
                github_token: dto.github_token,
                repos_root: dto.repos_root,
                worktrees_root: dto.worktrees_root,
                branch_prefix: dto.branch_prefix,
                driver: dto.driver,
                timeout_s: dto.timeout_s,
                run_timeout_s: dto.run_timeout_s,
                interactive: dto.interactive,
            };
            (Some(Arc::new(cfg)), dto.db_path, dto.max_concurrent.max(1))
        }
        None => (None, String::new(), 2),
    };
    let core = Box::new(AgentCore {
        callback: Arc::new(Mutex::new(CallbackSlot { cb: None, ctx: std::ptr::null_mut() })),
        config,
        db_path,
        max_concurrent,
        runtime: Mutex::new(None),
    });
    Box::into_raw(core)
}

#[no_mangle]
pub extern "C" fn agent_core_set_event_callback(
    core: *mut AgentCore,
    ctx: *mut c_void,
    cb: Option<EventCallback>,
) {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return;
    };
    if let Ok(mut guard) = core.callback.lock() {
        guard.cb = cb;
        guard.ctx = ctx;
    }
}

#[no_mangle]
pub extern "C" fn agent_core_start(core: *mut AgentCore) -> c_int {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return ERR_INVALID_HANDLE;
    };
    // Idempotent: a second start while running is a no-op.
    {
        if core.runtime.lock().unwrap().is_some() {
            return OK;
        }
    }
    emit(&core.callback, r#"{"type":"log","level":"info","message":"agent-core starting"}"#);

    let Some(config) = core.config.as_ref().map(Arc::clone) else {
        emit(&core.callback, r#"{"type":"log","level":"error","message":"no/invalid config; idle"}"#);
        return OK; // nothing to run, but not a hard error
    };
    let state = match State::open(&core.db_path) {
        Ok(s) => {
            // Startup sweep: no interactive session survives a restart, so any
            // lingering interactive_owned flag is stale and would block the
            // issue's background pipeline forever.
            let _ = s.clear_interactive_owned_all();
            Arc::new(Mutex::new(s))
        }
        Err(e) => {
            emit(&core.callback, &format!(r#"{{"type":"log","level":"error","message":"state open failed: {e}"}}"#));
            return ERR_CONFIG;
        }
    };

    let stop = Arc::new(AtomicBool::new(false));

    // Host-event emitter: serialise core events for the host. `run_request`
    // demands a response (agent_core_submit_run_result); the rest are
    // fire-and-forget UI signals (toasts / run indicators / terminal teardown).
    let slot = core.callback.clone();
    let emit_cb: Emit = Arc::new(move |ev: &HostEvent| {
        let v = match ev {
            HostEvent::RunRequest(r) => {
                let env: serde_json::Map<String, serde_json::Value> =
                    r.env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
                json!({
                    "type": "run_request",
                    "runId": r.run_id,
                    "issueId": r.issue_id, "issueIdentifier": r.issue_identifier,
                    "cwd": r.cwd, "mode": r.mode,
                    "program": r.program, "argv": r.argv, "env": env,
                    "mcpConfigPath": r.mcp_config_path,
                    "systemPrompt": r.system_prompt, "userPrompt": r.user_prompt,
                    // Interactive: the host launches the CLI in the visible terminal
                    // with NO output capture; the plan is delivered via MCP.
                    "interactive": r.interactive,
                    "continueSessionId": r.continue_session_id,
                })
            }
            HostEvent::RunStarted { issue_id, issue_identifier, run_id, mode } => json!({
                "type": "run_started",
                "issueId": issue_id, "issueIdentifier": issue_identifier,
                "runId": run_id, "mode": mode,
            }),
            HostEvent::RunFinished { issue_id, run_id, exit_code, outcome } => json!({
                "type": "run_finished",
                "issueId": issue_id, "runId": run_id,
                "exitCode": exit_code, "outcome": outcome,
            }),
            HostEvent::RunCancelled { issue_id, run_id } => json!({
                "type": "run_cancelled",
                "issueId": issue_id, "runId": run_id,
            }),
            HostEvent::AgentError { issue_id, code, message } => json!({
                "type": "agent_error",
                "issueId": issue_id, "code": code, "message": message,
            }),
        };
        emit(&slot, &v.to_string());
    });

    let interactive_slot = Arc::new(run_pipeline::InteractiveSlot::default());
    let pipeline = run_pipeline::build_pipeline(Arc::clone(&config), Arc::clone(&state), emit_cb.clone(), Arc::clone(&interactive_slot));
    let dispatcher = Dispatcher::start(Arc::clone(&state), core.max_concurrent, pipeline);

    // Electric assigned-issues loop → dispatcher.
    let electric_h = {
        let (c, s, st, d) = (Arc::clone(&config), Arc::clone(&state), Arc::clone(&stop), dispatcher.clone());
        thread::spawn(move || {
            electric::run_loop(&c.base_url, Some(&c.api_key), &c.bot_user_id, &s, &st, move |ev| d.enqueue(ev));
        })
    };
    // PR reconcile loop.
    let prpoll_h = {
        let (c, s, st) = (Arc::clone(&config), Arc::clone(&state), Arc::clone(&stop));
        thread::spawn(move || pr_poll::run_loop(&c, &s, &st))
    };

    *core.runtime.lock().unwrap() = Some(Runtime {
        stop,
        dispatcher,
        workers: vec![electric_h, prpoll_h],
        config: Arc::clone(&config),
        state: Arc::clone(&state),
        emit: emit_cb,
        interactive_slot,
    });
    emit(&core.callback, r#"{"type":"log","level":"info","message":"agent loop started"}"#);
    OK
}

#[no_mangle]
pub extern "C" fn agent_core_stop(core: *mut AgentCore) -> c_int {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return ERR_INVALID_HANDLE;
    };
    let rt = core.runtime.lock().unwrap().take();
    if let Some(rt) = rt {
        // Unblock any pipeline parked in request_run, then drain in-flight work.
        agent_run::cancel_all();
        rt.dispatcher.stop();
        // The electric/pr_poll loops exit on their next poll/tick; we don't join
        // (an in-flight ~60s long-poll would block). Their Arc clones keep the
        // shared state alive until they finish, which is harmless.
        rt.stop.store(true, Ordering::SeqCst);
    }
    OK
}

#[no_mangle]
pub extern "C" fn agent_core_free(core: *mut AgentCore) {
    if core.is_null() {
        return;
    }
    // Make sure the worker is stopped before dropping the box it borrows from.
    agent_core_stop(core);
    // SAFETY: `core` was produced by Box::into_raw in agent_core_create and is
    // freed exactly once here.
    unsafe { drop(Box::from_raw(core)) };
}

#[no_mangle]
pub extern "C" fn agent_core_claim_setup(
    _base_url: *const c_char,
    _setup_token: *const c_char,
    out_json: *mut *mut c_char,
) -> c_int {
    if !out_json.is_null() {
        unsafe { *out_json = std::ptr::null_mut() };
    }
    ERR_NOT_IMPLEMENTED // implemented in M5
}

#[no_mangle]
pub extern "C" fn agent_core_github_device_login(core: *mut AgentCore) -> c_int {
    if unsafe { core.as_ref() }.is_none() {
        return ERR_INVALID_HANDLE;
    }
    ERR_NOT_IMPLEMENTED // implemented in M5
}

#[no_mangle]
pub extern "C" fn agent_core_uninstall(core: *mut AgentCore) -> c_int {
    if unsafe { core.as_ref() }.is_none() {
        return ERR_INVALID_HANDLE;
    }
    ERR_NOT_IMPLEMENTED // implemented in M5
}

#[no_mangle]
pub extern "C" fn agent_core_submit_run_result(
    core: *mut AgentCore,
    run_id: *const c_char,
    exit_code: c_int,
    final_text: *const c_char,
    // The CLI session id (claude), so an interactive run can later be continued.
    // Pass NULL for headless runs / hosts that don't surface it.
    session_id: *const c_char,
) -> c_int {
    if unsafe { core.as_ref() }.is_none() {
        return ERR_INVALID_HANDLE;
    }
    let Some(run_id) = (unsafe { cstr_to_string(run_id) }) else {
        return ERR_CONFIG;
    };
    let text = unsafe { cstr_to_string(final_text) }.unwrap_or_default();
    let session = unsafe { cstr_to_string(session_id) }.filter(|s| !s.is_empty());
    agent_run::submit_result(&run_id, exit_code as i32, text, session);
    OK
}

#[no_mangle]
pub extern "C" fn agent_core_cancel_run(core: *mut AgentCore, run_id: *const c_char) -> c_int {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return ERR_INVALID_HANDLE;
    };
    if let Some(id) = unsafe { cstr_to_string(run_id) } {
        if agent_run::cancel(&id) {
            emit(&core.callback, &json!({"type": "run_cancelled", "issueId": null, "runId": id}).to_string());
        }
    }
    OK
}

/// Host-triggered: cancel the run currently in flight for an issue (the desktop
/// "Cancel" button). The host knows the issue id, not the run_id; the core maps
/// it and resolves the run with the cancel sentinel, unblocking the parked
/// pipeline thread so the issue lands in `cancelled` (retryable). Emits a
/// `run_cancelled` event so the host tears down the matching terminal (killing
/// the CLI child). No-op if nothing is in flight for that issue.
#[no_mangle]
pub extern "C" fn agent_core_cancel_issue(core: *mut AgentCore, issue_id: *const c_char) -> c_int {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return ERR_INVALID_HANDLE;
    };
    if let Some(id) = unsafe { cstr_to_string(issue_id) } {
        let run_id = agent_run::run_for_issue(&id);
        if agent_run::cancel_issue(&id) {
            emit(&core.callback, &json!({"type": "run_cancelled", "issueId": id, "runId": run_id}).to_string());
        }
    }
    OK
}

/// Host-triggered: start an INTERACTIVE plan session for one issue (the desktop
/// "AI" button). Runs on a worker thread (it blocks on the host's terminal run);
/// the host receives a `run_request` with `interactive: true` and launches the
/// CLI in the embedded terminal. Returns immediately.
#[no_mangle]
pub extern "C" fn agent_core_request_interactive(core: *mut AgentCore, issue_id: *const c_char) -> c_int {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return ERR_INVALID_HANDLE;
    };
    let Some(issue_id) = (unsafe { cstr_to_string(issue_id) }) else {
        return ERR_CONFIG;
    };
    emit(&core.callback, &format!(r#"{{"type":"log","level":"info","message":"request_interactive issue={issue_id}"}}"#));
    let guard = core.runtime.lock().unwrap();
    let Some(rt) = guard.as_ref() else {
        emit(&core.callback, r#"{"type":"log","level":"error","message":"request_interactive: runtime not started"}"#);
        return ERR_INVALID_HANDLE; // not started
    };
    let (config, state, emit, slot) = (Arc::clone(&rt.config), Arc::clone(&rt.state), rt.emit.clone(), Arc::clone(&rt.interactive_slot));
    drop(guard);
    // Claim the single interactive-session slot BEFORE spawning (and before any
    // slow I/O): a busy dock fails fast instead of clobbering the live session.
    let Some(slot_guard) = slot.try_claim(&issue_id) else {
        (emit)(&HostEvent::AgentError {
            issue_id: &issue_id,
            code: run_pipeline::ERROR_CODE_INTERACTIVE_BUSY,
            message: "another interactive session is active",
        });
        return OK;
    };
    thread::spawn(move || run_pipeline::run_interactive_plan(config, state, emit, slot_guard, &issue_id));
    OK
}

/// Host-triggered: continue an interactive session after the user approved the
/// plan (the desktop "Approve & continue here"). The host has already approved
/// with the human session; this resumes the same claude session in the reused
/// worktree to implement it. Runs on a worker thread.
#[no_mangle]
pub extern "C" fn agent_core_approve_interactive(core: *mut AgentCore, issue_id: *const c_char) -> c_int {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return ERR_INVALID_HANDLE;
    };
    let Some(issue_id) = (unsafe { cstr_to_string(issue_id) }) else {
        return ERR_CONFIG;
    };
    let guard = core.runtime.lock().unwrap();
    let Some(rt) = guard.as_ref() else {
        return ERR_INVALID_HANDLE;
    };
    let (config, state, emit, slot) = (Arc::clone(&rt.config), Arc::clone(&rt.state), rt.emit.clone(), Arc::clone(&rt.interactive_slot));
    drop(guard);
    // Same gate as request_interactive: never mount a second session.
    let Some(slot_guard) = slot.try_claim(&issue_id) else {
        (emit)(&HostEvent::AgentError {
            issue_id: &issue_id,
            code: run_pipeline::ERROR_CODE_INTERACTIVE_BUSY,
            message: "another interactive session is active",
        });
        return OK;
    };
    thread::spawn(move || run_pipeline::run_interactive_continue(config, state, emit, slot_guard, &issue_id));
    OK
}

#[no_mangle]
pub extern "C" fn agent_core_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    // SAFETY: `s` must have been produced by CString::into_raw in this crate.
    unsafe { drop(CString::from_raw(s)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    static EVENTS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn counting_cb(_ctx: *mut c_void, json: *const c_char, _len: usize) {
        assert!(!json.is_null());
        EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn lifecycle_emits_startup_log_and_frees_cleanly() {
        EVENTS.store(0, Ordering::SeqCst);
        let core = agent_core_create(std::ptr::null());
        assert!(!core.is_null());
        agent_core_set_event_callback(core, std::ptr::null_mut(), Some(counting_cb));
        assert_eq!(agent_core_start(core), OK);
        // The startup log fires synchronously on the worker; give it a moment.
        thread::sleep(Duration::from_millis(80));
        assert_eq!(agent_core_stop(core), OK);
        agent_core_free(core);
        assert!(
            EVENTS.load(Ordering::SeqCst) >= 1,
            "expected at least the startup log event"
        );
    }

    #[test]
    fn null_handle_is_rejected_not_crashed() {
        assert_eq!(agent_core_start(std::ptr::null_mut()), ERR_INVALID_HANDLE);
        assert_eq!(agent_core_stop(std::ptr::null_mut()), ERR_INVALID_HANDLE);
        assert_eq!(
            agent_core_uninstall(std::ptr::null_mut()),
            ERR_INVALID_HANDLE
        );
        // free(NULL) is a no-op.
        agent_core_free(std::ptr::null_mut());
    }

    static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    extern "C" fn capturing_cb(_ctx: *mut c_void, json: *const c_char, _len: usize) {
        let s = unsafe { CStr::from_ptr(json) }.to_string_lossy().into_owned();
        CAPTURED.lock().unwrap().push(s);
    }

    #[test]
    fn second_interactive_request_fails_fast_without_clobbering() {
        // A listener that accepts and holds connections keeps the first
        // session parked in its slow I/O (mcp get_issue) — the exact window
        // where a second press used to clobber it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let mut held = Vec::new();
            for s in listener.incoming() {
                if let Ok(s) = s {
                    held.push(s);
                }
            }
        });

        let cfg = json!({
            "baseUrl": format!("http://{addr}"),
            "apiKey": "expk_test",
            "botUserId": "bot",
            "reposRoot": "/tmp/agent-core-ffi-test/repos",
            "worktreesRoot": "/tmp/agent-core-ffi-test/worktrees",
            "dbPath": ":memory:",
            "timeoutS": 5,
        })
        .to_string();
        let c_cfg = CString::new(cfg).unwrap();
        let core = agent_core_create(c_cfg.as_ptr());
        agent_core_set_event_callback(core, std::ptr::null_mut(), Some(capturing_cb));
        assert_eq!(agent_core_start(core), OK);

        let a = CString::new("ffi-test-issue-a").unwrap();
        let b = CString::new("ffi-test-issue-b").unwrap();
        // First press claims the global slot synchronously, then parks in I/O.
        assert_eq!(agent_core_request_interactive(core, a.as_ptr()), OK);
        // Second press (another issue) fails fast: agent_error, no run mounted.
        assert_eq!(agent_core_request_interactive(core, b.as_ptr()), OK);
        // Same-issue double-press hits the same gate (approve path included).
        assert_eq!(agent_core_approve_interactive(core, a.as_ptr()), OK);

        let events = CAPTURED.lock().unwrap().clone();
        let busy: Vec<&String> = events.iter().filter(|e| e.contains(r#""interactive_session_active""#)).collect();
        assert_eq!(busy.len(), 2, "both rejected presses emit agent_error: {events:?}");
        assert!(busy[0].contains("ffi-test-issue-b"));
        assert!(busy[1].contains("ffi-test-issue-a"));
        assert!(
            !events.iter().any(|e| e.contains(r#""run_request""#)),
            "no session may be mounted while the first one holds the slot: {events:?}"
        );

        // Deliberately leaked: agent_core_stop drains the PROCESS-GLOBAL run
        // registry (agent_run::cancel_all), which would cancel the agent_run
        // tests running in parallel. Teardown isn't under test here.
    }
}
