//! The private worker: the cua platform runtime in its own process.
//!
//! The host re-runs ITS OWN executable as `__private-worker --generation
//! <id>` (cua's `PrivateWorkerClient` spawns it) and speaks cua's
//! line-delimited channel over the child's stdin/stdout. The worker builds
//! the one unrestricted runtime, binds a trusted session per run and routes
//! every tool call to it; on macOS its main thread runs the agent cursor's
//! AppKit loop, which a GUI host (the IDE) cannot lend and a headless one
//! (the daemon) has no use for itself.
//!
//! A child of the host stays in the host's TCC responsibility chain: the
//! permissions the person granted the app answer the worker's checks too,
//! and no second prompt names a second program.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;

use cua_driver_sdk::worker::{
    ActionCompletion, ChannelRequest, ChannelResponse, WorkerInitialization,
    PRIVATE_WORKER_PROTOCOL_VERSION,
};
use cua_driver_sdk::{CuaDriver, CuaDriverSession, DriverHostOptions};
use serde_json::Value;

/// The argv marker cua's worker client passes; a host checks for it before
/// anything else in `main`.
pub const ARGV_MARKER: &str = "__private-worker";

/// `Some(generation)` when this process was started as the worker. Mirrors
/// cua's own check: `<exe> __private-worker --generation <id>`.
pub fn requested_generation() -> Option<String> {
    let mut args = std::env::args();
    let _executable = args.next();
    if args.next().as_deref() != Some(ARGV_MARKER) {
        return None;
    }
    match (args.next().as_deref(), args.next()) {
        (Some("--generation"), Some(generation))
            if !generation.is_empty()
                && generation.len() <= 128
                && generation.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') =>
        {
            Some(generation)
        }
        _ => Some(String::new()),
    }
}

/// Run the worker to completion and exit the process. Call it from `main`
/// when [`requested_generation`] answers; it never returns.
pub fn run_and_exit(generation: String) -> ! {
    // The native Wayland backend is opt-in by env in cua; a Wayland session
    // is one we want it on for (the worker's environment is the allowlisted
    // subset of the host's, so set it here, in the worker itself).
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        std::env::set_var("CUA_DRIVER_RS_ENABLE_WAYLAND", "1");
    }
    let code = {
        #[cfg(target_os = "macos")]
        {
            // The channel loop runs on a thread; the main thread belongs to
            // the agent cursor's AppKit loop once the runtime stands (it
            // parks when the process has no window server, e.g. under SSH).
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("computer-worker".into())
                .spawn(move || {
                    let code = run(generation, Some(ready_tx));
                    std::process::exit(code);
                })
                .expect("spawn the worker thread");
            if ready_rx.recv().unwrap_or(false) {
                platform_macos::cursor::overlay::run_on_main_thread();
            }
            0
        }
        #[cfg(not(target_os = "macos"))]
        {
            run(generation, None)
        }
    };
    std::process::exit(code)
}

fn run(generation: String, ready: Option<std::sync::mpsc::SyncSender<bool>>) -> i32 {
    if generation.is_empty() {
        eprintln!("[computer worker] missing --generation");
        return 2;
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("[computer worker] no runtime: {err}");
            return 1;
        }
    };
    match runtime.block_on(serve(generation, ready)) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("[computer worker] {err}");
            1
        }
    }
}

/// `ready` is told once (true = the runtime stands, false = it never will).
struct Ready(Option<std::sync::mpsc::SyncSender<bool>>);

impl Ready {
    fn up(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(true);
        }
    }
}

impl Drop for Ready {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(false);
        }
    }
}

async fn serve(
    generation: String,
    ready: Option<std::sync::mpsc::SyncSender<bool>>,
) -> Result<(), String> {
    let mut ready = Ready(ready);
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let refuse = |out: &mut dyn Write, id: u64, code: &str, text: String| {
        write_line(out, &ChannelResponse::error(id, &generation, code, text, ActionCompletion::NotStarted))
    };

    let Some(first) = lines.next() else { return Ok(()) };
    let first = first.map_err(|err| err.to_string())?;
    let init: ChannelRequest = serde_json::from_str(&first).map_err(|err| err.to_string())?;
    if init.protocol_version != PRIVATE_WORKER_PROTOCOL_VERSION
        || init.request_id != 0
        || init.generation != generation
        || init.operation != "initialize"
    {
        return refuse(&mut out, init.request_id, "invalid_initialization", "worker identity mismatch".into());
    }
    let options: WorkerInitialization = match init.arguments.map(serde_json::from_value) {
        Some(Ok(options)) => options,
        other => {
            let why = other.and_then(Result::err).map(|e| e.to_string()).unwrap_or_default();
            return refuse(&mut out, 0, "invalid_initialization", format!("worker options: {why}"));
        }
    };
    let host_bundle_id = options.host_bundle_id.clone();
    let driver = match CuaDriver::try_create_configured_for_host(
        options.configured_driver,
        DriverHostOptions {
            cursor: cursor_overlay::CursorConfig::default(),
            // The host shows readiness and asks the OS; the runtime only
            // reports.
            host_owns_permission_ux: true,
            host_bundle_id: Some(host_bundle_id.clone()),
            claude_code_compatibility: false,
            prepare_desktop_environment: true,
            register_host_tools: None,
            authorization_host: None,
            activity_observer: None,
        },
    ) {
        Ok(driver) => driver,
        Err(err) => return refuse(&mut out, 0, "runtime_initialization_failed", err.to_string()),
    };
    ready.up();
    let metadata = driver.metadata().await.map_err(|err| err.to_string())?;
    write_line(
        &mut out,
        &ChannelResponse::ok(
            0,
            &generation,
            serde_json::json!({
                "ready": true,
                "pid": std::process::id(),
                "host_bundle_id": host_bundle_id,
                "metadata": metadata,
            }),
        ),
    )?;

    let mut sessions: HashMap<String, Arc<CuaDriverSession>> = HashMap::new();
    for line in lines {
        let Ok(line) = line else { break };
        let request: ChannelRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(err) => {
                refuse(&mut out, 0, "invalid_request", format!("parse request: {err}"))?;
                continue;
            }
        };
        if request.protocol_version != PRIVATE_WORKER_PROTOCOL_VERSION || request.generation != generation {
            refuse(&mut out, request.request_id, "generation_mismatch", "another runtime generation".into())?;
            continue;
        }
        let response = handle(&driver, &mut sessions, &generation, request).await;
        let shutdown = response
            .result
            .as_ref()
            .and_then(|value| value.get("shutdown"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        write_line(&mut out, &response)?;
        if shutdown {
            break;
        }
    }
    sessions.clear();
    driver.shutdown().await.map_err(|err| err.to_string())
}

async fn handle(
    driver: &Arc<CuaDriver>,
    sessions: &mut HashMap<String, Arc<CuaDriverSession>>,
    generation: &str,
    request: ChannelRequest,
) -> ChannelResponse {
    let id = request.request_id;
    let invalid = |text: &str| {
        ChannelResponse::error(id, generation, "invalid_request", text, ActionCompletion::NotStarted)
    };
    let parse = |json: String| serde_json::from_str::<Value>(&json).map_err(|err| err.to_string());
    let result: Result<Value, String> = match request.operation.as_str() {
        "metadata" => driver
            .metadata()
            .await
            .map_err(|err| err.to_string())
            .and_then(|metadata| serde_json::to_value(metadata).map_err(|err| err.to_string())),
        "list" => driver.list_tools_json().await.map_err(|err| err.to_string()).and_then(parse),
        "sessions_list" => {
            driver.list_host_sessions_json().await.map_err(|err| err.to_string()).and_then(parse)
        }
        "call" => {
            let name = request.name.as_deref().unwrap_or("");
            let arguments = request.arguments.unwrap_or_else(|| Value::Object(Default::default()));
            let outcome = match request.session_handle.as_deref() {
                Some(handle) => match sessions.get(handle) {
                    Some(session) => session.call_tool(name.to_owned(), arguments.to_string()).await,
                    None => {
                        return ChannelResponse::error(
                            id,
                            generation,
                            "session_not_bound",
                            "no such session on this channel",
                            ActionCompletion::NotStarted,
                        )
                    }
                },
                None => driver.call_tool_from_trusted_adapter(name, arguments).await,
            };
            outcome.map_err(|err| err.to_string()).and_then(|result| parse(result.raw_json))
        }
        "bind_session" => match request.arguments.map(serde_json::from_value) {
            Some(Ok(options)) => match driver.create_trusted_session(options) {
                Ok(session) => {
                    let handle = uuid::Uuid::new_v4().to_string();
                    sessions.insert(handle.clone(), session);
                    Ok(serde_json::json!({ "session_handle": handle }))
                }
                Err(err) => Err(err.to_string()),
            },
            Some(Err(err)) => Err(err.to_string()),
            None => return invalid("bind_session omitted options"),
        },
        "close_session" => {
            let Some(handle) = request.session_handle.as_deref() else {
                return invalid("close_session omitted session_handle");
            };
            if let Some(session) = sessions.remove(handle) {
                session.close();
            }
            Ok(serde_json::json!({ "closed": true }))
        }
        "shutdown" => {
            sessions.clear();
            driver
                .shutdown()
                .await
                .map(|()| serde_json::json!({ "shutdown": true }))
                .map_err(|err| err.to_string())
        }
        other => Err(format!("unknown worker operation: {other}")),
    };
    match result {
        Ok(value) => ChannelResponse::ok(id, generation, value),
        Err(error) => ChannelResponse::error(
            id,
            generation,
            "worker_request_failed",
            error,
            ActionCompletion::Completed,
        ),
    }
}

fn write_line(out: &mut dyn Write, response: &ChannelResponse) -> Result<(), String> {
    serde_json::to_writer(&mut *out, response).map_err(|err| err.to_string())?;
    out.write_all(b"\n").and_then(|()| out.flush()).map_err(|err| err.to_string())
}
