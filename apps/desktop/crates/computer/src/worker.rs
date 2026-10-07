//! The private worker: the cua platform runtime in its own process.
//!
//! The host re-runs ITS OWN executable as `__private-worker --generation
//! <id>` ([`crate::channel`] spawns it the way cua's `PrivateWorkerClient`
//! would) and speaks cua's line-delimited channel over the child's
//! stdin/stdout. The worker builds the one unrestricted runtime, binds a
//! trusted session per run and routes every tool call to it; on macOS its
//! main thread runs the agent cursor's AppKit loop, which a GUI host (the
//! IDE) cannot lend and a headless one (the daemon) has no use for itself.
//!
//! EXP-1236: after the handshake every request runs as its own task and
//! replies go out in completion order, one whole line each, so a script
//! driving several windows at once is served at once. The host's channel
//! routes replies by id; cua's own client would not, which is why the two
//! sides ship together.
//!
//! A child of the host stays in the host's TCC responsibility chain: the
//! permissions the person granted the app answer the worker's checks too,
//! and no second prompt names a second program.

#[cfg(feature = "cua")]
use std::collections::HashMap;
#[cfg(feature = "cua")]
use std::future::Future;
#[cfg(feature = "cua")]
use std::io::{BufRead, Write};
#[cfg(feature = "cua")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "cua")]
use std::time::Duration;

#[cfg(feature = "cua")]
use cua_driver_sdk::worker::{
    ActionCompletion, ChannelRequest, ChannelResponse, WorkerInitialization,
    PRIVATE_WORKER_PROTOCOL_VERSION,
};
#[cfg(feature = "cua")]
use cua_driver_sdk::{CuaDriver, CuaDriverSession, DriverHostOptions};
#[cfg(feature = "cua")]
use serde_json::Value;
#[cfg(feature = "cua")]
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// The argv marker cua's worker client passes; a host checks for it before
/// anything else in `main`.
pub const ARGV_MARKER: &str = "__private-worker";

/// `Some(generation)` when this process was started as the worker. Mirrors
/// cua's own check: `<exe> __private-worker --generation <id>`. A binary
/// built without cua never is one.
pub fn requested_generation() -> Option<String> {
    if cfg!(not(feature = "cua")) {
        return None;
    }
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
#[cfg(not(feature = "cua"))]
pub fn run_and_exit(_generation: String) -> ! {
    std::process::exit(2)
}

/// Run the worker to completion and exit the process. Call it from `main`
/// when [`requested_generation`] answers; it never returns.
#[cfg(feature = "cua")]
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

#[cfg(feature = "cua")]
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
    let outcome = runtime.block_on(serve(generation, ready));
    // A plain drop would wait for every blocking task; the stdout writer
    // may sit on a pipe nobody reads any more.
    runtime.shutdown_timeout(WRITER_DRAIN);
    match outcome {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("[computer worker] {err}");
            1
        }
    }
}

/// How long the last replies may take to leave after a shutdown; the host
/// waits that long for the exit too.
#[cfg(feature = "cua")]
const WRITER_DRAIN: Duration = Duration::from_secs(2);

/// `ready` is told once (true = the runtime stands, false = it never will).
#[cfg(feature = "cua")]
struct Ready(Option<std::sync::mpsc::SyncSender<bool>>);

#[cfg(feature = "cua")]
impl Ready {
    fn up(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(true);
        }
    }
}

#[cfg(feature = "cua")]
impl Drop for Ready {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(false);
        }
    }
}

#[cfg(feature = "cua")]
type Sessions = Arc<Mutex<HashMap<String, Arc<CuaDriverSession>>>>;

#[cfg(feature = "cua")]
async fn serve(
    generation: String,
    ready: Option<std::sync::mpsc::SyncSender<bool>>,
) -> Result<(), String> {
    let mut ready = Ready(ready);
    let refuse = |id: u64, code: &str, text: String| {
        let stdout = std::io::stdout();
        write_line(&mut stdout.lock(), &ChannelResponse::error(id, &generation, code, text, ActionCompletion::NotStarted))
    };

    // The handshake is the one request answered in line: nothing else may be
    // in flight before the runtime stands.
    let first = {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        lines.next()
    };
    let Some(first) = first else { return Ok(()) };
    let first = first.map_err(|err| err.to_string())?;
    let init: ChannelRequest = serde_json::from_str(&first).map_err(|err| err.to_string())?;
    if init.protocol_version != PRIVATE_WORKER_PROTOCOL_VERSION
        || init.request_id != 0
        || init.generation != generation
        || init.operation != "initialize"
    {
        return refuse(init.request_id, "invalid_initialization", "worker identity mismatch".into());
    }
    let options: WorkerInitialization = match init.arguments.map(serde_json::from_value) {
        Some(Ok(options)) => options,
        other => {
            let why = other.and_then(Result::err).map(|e| e.to_string()).unwrap_or_default();
            return refuse(0, "invalid_initialization", format!("worker options: {why}"));
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
        Err(err) => return refuse(0, "runtime_initialization_failed", err.to_string()),
    };
    ready.up();
    let metadata = driver.metadata().await.map_err(|err| err.to_string())?;
    write_line(
        &mut std::io::stdout().lock(),
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

    // From here the channel is concurrent (EXP-1236): one thread reads
    // stdin, one owns stdout, every request in between is its own task.
    let (lines_tx, lines_rx) = unbounded_channel::<String>();
    std::thread::Builder::new()
        .name("computer-worker-stdin".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                if lines_tx.send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|err| format!("spawn the stdin reader: {err}"))?;
    let (out_tx, out_rx) = unbounded_channel::<ChannelResponse>();
    let writer = spawn_writer(out_rx, Box::new(std::io::stdout()));

    let sessions: Sessions = Arc::default();
    let generation: Arc<str> = generation.into();
    let handler = {
        let (driver, sessions, generation) = (driver.clone(), sessions.clone(), generation.clone());
        move |request: ChannelRequest| {
            let (driver, sessions, generation) = (driver.clone(), sessions.clone(), generation.clone());
            async move { handle(&driver, &sessions, &generation, request).await }
        }
    };
    let shutdown = pump(lines_rx, out_tx.clone(), generation.clone(), handler).await;

    // Stdin ended or the host asked: take the runtime down (in-flight
    // actions fail at once), answer the shutdown request LAST, let the
    // writer drain.
    sessions.lock().unwrap().clear();
    let outcome = driver.shutdown().await.map_err(|err| err.to_string());
    if let Some(request) = shutdown {
        let response = match &outcome {
            Ok(()) => ChannelResponse::ok(request.request_id, &*generation, serde_json::json!({ "shutdown": true })),
            Err(err) => ChannelResponse::error(
                request.request_id,
                &*generation,
                "worker_request_failed",
                err.clone(),
                ActionCompletion::Completed,
            ),
        };
        let _ = out_tx.send(response);
    }
    drop(out_tx);
    let _ = tokio::time::timeout(WRITER_DRAIN, writer).await;
    outcome
}

/// The request loop: every line is parsed and checked here, then handled
/// on its own task, its reply sent to the writer whenever it is done.
/// Returns when stdin ends (`None`) or a `shutdown` request arrives (the
/// request, for the caller to answer once the runtime is down).
#[cfg(feature = "cua")]
async fn pump<H, Fut>(
    mut requests: UnboundedReceiver<String>,
    responses: UnboundedSender<ChannelResponse>,
    generation: Arc<str>,
    handler: H,
) -> Option<ChannelRequest>
where
    H: Fn(ChannelRequest) -> Fut + Send + 'static,
    Fut: Future<Output = ChannelResponse> + Send + 'static,
{
    let refuse = |id: u64, code: &str, text: String| {
        let _ = responses.send(ChannelResponse::error(id, &*generation, code, text, ActionCompletion::NotStarted));
    };
    while let Some(line) = requests.recv().await {
        let request: ChannelRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(err) => {
                refuse(0, "invalid_request", format!("parse request: {err}"));
                continue;
            }
        };
        if request.protocol_version != PRIVATE_WORKER_PROTOCOL_VERSION || *request.generation != *generation {
            refuse(request.request_id, "generation_mismatch", "another runtime generation".into());
            continue;
        }
        if request.operation == "shutdown" {
            return Some(request);
        }
        let responses = responses.clone();
        let reply = handler(request);
        tokio::spawn(async move {
            let _ = responses.send(reply.await);
        });
    }
    None
}

/// The one owner of stdout: replies leave as whole lines, in the order they
/// finish. Ends when every sender is gone.
#[cfg(feature = "cua")]
fn spawn_writer(
    mut responses: UnboundedReceiver<ChannelResponse>,
    mut out: Box<dyn Write + Send>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        while let Some(response) = responses.blocking_recv() {
            if let Err(err) = write_line(&mut *out, &response) {
                eprintln!("[computer worker] stdout: {err}");
                break;
            }
        }
    })
}

#[cfg(feature = "cua")]
async fn handle(
    driver: &Arc<CuaDriver>,
    sessions: &Sessions,
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
            // The map lock is held for the lookup only, never across the
            // action: other runs' calls go on while this one acts.
            let session = match request.session_handle.as_deref() {
                Some(handle) => {
                    let found = sessions.lock().unwrap().get(handle).cloned();
                    match found {
                        Some(session) => Some(session),
                        None => {
                            return ChannelResponse::error(
                                id,
                                generation,
                                "session_not_bound",
                                "no such session on this channel",
                                ActionCompletion::NotStarted,
                            )
                        }
                    }
                }
                None => None,
            };
            let outcome = match session {
                Some(session) => session.call_tool(name.to_owned(), arguments.to_string()).await,
                None => driver.call_tool_from_trusted_adapter(name, arguments).await,
            };
            outcome.map_err(|err| err.to_string()).and_then(|result| parse(result.raw_json))
        }
        "bind_session" => match request.arguments.map(serde_json::from_value) {
            Some(Ok(options)) => match driver.create_trusted_session(options) {
                Ok(session) => {
                    let handle = uuid::Uuid::new_v4().to_string();
                    sessions.lock().unwrap().insert(handle.clone(), session);
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
            let removed = sessions.lock().unwrap().remove(handle);
            if let Some(session) = removed {
                session.close();
            }
            Ok(serde_json::json!({ "closed": true }))
        }
        // `shutdown` never reaches here: the pump ends on it.
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

/// One reply as ONE write: the line is built first so two writers (the
/// handshake, then the writer thread) could never interleave halves.
#[cfg(feature = "cua")]
fn write_line(out: &mut dyn Write, response: &ChannelResponse) -> Result<(), String> {
    let mut line = serde_json::to_vec(response).map_err(|err| err.to_string())?;
    line.push(b'\n');
    out.write_all(&line).and_then(|()| out.flush()).map_err(|err| err.to_string())
}

#[cfg(all(test, feature = "cua"))]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::time::Instant;

    /// A `Write` the test can read back.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn request(id: u64, generation: &str, operation: &str) -> String {
        serde_json::to_string(&ChannelRequest {
            protocol_version: PRIVATE_WORKER_PROTOCOL_VERSION,
            request_id: id,
            generation: generation.to_string(),
            operation: operation.to_string(),
            name: None,
            arguments: None,
            session_handle: None,
        })
        .unwrap()
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap()
    }

    #[test]
    fn requests_run_at_once_and_replies_leave_as_whole_lines() {
        runtime().block_on(async {
            let (lines_tx, lines_rx) = unbounded_channel();
            let (out_tx, out_rx) = unbounded_channel();
            let captured = Captured::default();
            let writer = spawn_writer(out_rx, Box::new(captured.clone()));
            for id in [1, 2] {
                lines_tx.send(request(id, "g1", "call")).unwrap();
            }
            lines_tx.send(request(3, "g1", "shutdown")).unwrap();
            let started = Instant::now();
            let handler = |request: ChannelRequest| async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                ChannelResponse::ok(request.request_id, "g1", serde_json::json!({ "n": request.request_id }))
            };
            let shutdown = pump(lines_rx, out_tx.clone(), "g1".into(), handler).await;
            assert_eq!(shutdown.map(|request| request.request_id), Some(3));
            drop(out_tx);
            // The writer ends once the two in-flight tasks have replied.
            writer.await.unwrap();
            let elapsed = started.elapsed();
            // Sequential handling would take 600 ms.
            assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
            let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
            assert!(text.ends_with('\n'));
            let lines: Vec<&str> = text.lines().collect();
            assert_eq!(lines.len(), 2, "{text}");
            let ids: HashSet<u64> = lines
                .iter()
                .map(|line| serde_json::from_str::<ChannelResponse>(line).unwrap().request_id)
                .collect();
            assert_eq!(ids, HashSet::from([1, 2]));
        });
    }

    #[test]
    fn a_foreign_generation_and_garbage_are_refused_in_line() {
        runtime().block_on(async {
            let (lines_tx, lines_rx) = unbounded_channel();
            let (out_tx, mut out_rx) = unbounded_channel();
            lines_tx.send(request(7, "other", "call")).unwrap();
            lines_tx.send("{ not json".to_string()).unwrap();
            drop(lines_tx);
            let handler = |_: ChannelRequest| async { unreachable!("refused before handling") };
            let shutdown = pump(lines_rx, out_tx, "g1".into(), handler).await;
            assert!(shutdown.is_none());
            let first = out_rx.recv().await.unwrap();
            assert_eq!((first.request_id, first.error_code.as_deref()), (7, Some("generation_mismatch")));
            let second = out_rx.recv().await.unwrap();
            assert_eq!((second.request_id, second.error_code.as_deref()), (0, Some("invalid_request")));
            assert_eq!(second.generation, "g1");
            assert!(out_rx.recv().await.is_none());
        });
    }
}
