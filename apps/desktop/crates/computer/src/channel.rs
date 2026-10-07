//! EXP-1236: the host's side of the private-worker channel, multiplexed.
//!
//! cua's own `PrivateWorkerClient` serialises the channel: it holds the
//! process lock from write to read, reads exactly ONE stdout line per
//! request and reaps the worker on a reply whose id is not the one it is
//! waiting for. A code-mode script driving several windows under
//! `Promise.all` needs the opposite: many requests in flight, replies in
//! any order. cua is pinned, so this client replaces it on the host while
//! keeping the wire protocol byte for byte (cua's `ChannelRequest` /
//! `ChannelResponse`, the `initialize` handshake with its readiness proof,
//! the allowlisted environment, the argv). Only who waits for what changes:
//! ONE reader thread routes every reply to its request by id, and a caller
//! waits on its own slot with no lock held. A timeout fails only that call
//! (a slow `verify_state` in one window must not reap the others); the
//! worker is stopped only when the pipe itself fails.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cua_driver_sdk::worker::{
    ActionCompletion, ChannelRequest, ChannelResponse, WorkerInitialization,
    PRIVATE_WORKER_PROTOCOL_VERSION,
};
use cua_driver_sdk::ConfiguredDriverOptions;
use serde_json::Value;

/// Why a request did not come back with a result.
#[derive(Debug)]
pub(crate) enum ChannelError {
    /// No reply within the ceiling. The worker lives on and may still be
    /// acting; only this call gave up.
    Timeout(Duration),
    /// The pipe is gone (exit, EOF, a failed write): the worker is dead or
    /// was stopped.
    Closed(String),
    /// The worker said something the protocol does not allow.
    Protocol(String),
    /// The worker answered with a refusal: its `error_code` and text.
    Worker { code: String, text: String, completion: ActionCompletion },
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::Timeout(after) => {
                write!(f, "the private worker did not answer within {} s", after.as_secs())
            }
            ChannelError::Closed(why) | ChannelError::Protocol(why) => f.write_str(why),
            ChannelError::Worker { code, text, .. } => write!(f, "{code}: {text}"),
        }
    }
}

/// Reply routing, shared by the reader thread and every caller: request id
/// to the caller's one-shot slot.
struct Router {
    generation: String,
    pending: Mutex<HashMap<u64, SyncSender<ChannelResponse>>>,
    /// The reader ended (EOF, a read error, a protocol violation).
    closed: AtomicBool,
}

impl Router {
    fn new(generation: String) -> Self {
        Self { generation, pending: Mutex::default(), closed: AtomicBool::new(false) }
    }

    fn register(&self, id: u64) -> Receiver<ChannelResponse> {
        let (tx, rx) = sync_channel(1);
        self.pending.lock().unwrap().insert(id, tx);
        rx
    }

    fn forget(&self, id: u64) {
        self.pending.lock().unwrap().remove(&id);
    }

    /// One stdout line to its waiting caller. `Err` = the line is not a reply
    /// of this worker generation; the reader then stops listening and every
    /// waiting caller learns the channel is closed.
    fn dispatch_line(&self, line: &str) -> Result<(), String> {
        let response: ChannelResponse = serde_json::from_str(line)
            .map_err(|err| format!("parse private worker response: {err}"))?;
        if response.protocol_version != PRIVATE_WORKER_PROTOCOL_VERSION
            || response.generation != self.generation
        {
            return Err("private worker response identity mismatch".to_string());
        }
        match self.pending.lock().unwrap().remove(&response.request_id) {
            // A caller that timed out has left its slot: a late reply is
            // dropped on the floor, which is the point of not reaping.
            Some(slot) => drop(slot.send(response)),
            None => log::debug!(
                "[computer] private worker reply {} has no waiting request ({})",
                response.request_id,
                response.error_code.as_deref().unwrap_or("ok")
            ),
        }
        Ok(())
    }

    /// The pipe ended: every waiting caller's slot drops, which its
    /// `recv_timeout` reports as disconnected.
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.pending.lock().unwrap().clear();
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// What a spawn needs; the same record cua's `PrivateWorkerOptions` carried.
pub(crate) struct SpawnOptions {
    pub binary_path: PathBuf,
    pub host_bundle_id: String,
    pub configured_driver: ConfiguredDriverOptions,
    pub startup_timeout: Duration,
    pub inherit_stderr: bool,
}

/// One private worker process and the multiplexed channel to it.
pub(crate) struct WorkerChannel {
    router: Arc<Router>,
    child: Mutex<Child>,
    /// `None` once stopped or shutting down (dropping it is the EOF the
    /// worker's stdin loop ends on).
    stdin: Mutex<Option<ChildStdin>>,
    /// Request ids start at 1; 0 is the handshake's.
    next_id: AtomicU64,
    stopped: AtomicBool,
}

impl WorkerChannel {
    /// Spawn `<binary> __private-worker --generation <id>` the way cua does
    /// and run the `initialize` handshake. The readiness proof must come
    /// back the way cua checks it: `ready`, a pid that is not ours, our
    /// host bundle id.
    pub fn spawn(options: SpawnOptions) -> Result<Arc<Self>, ChannelError> {
        let generation = uuid::Uuid::new_v4().to_string();
        let mut command = Command::new(&options.binary_path);
        command
            .arg(crate::worker::ARGV_MARKER)
            .arg("--generation")
            .arg(&generation)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if options.inherit_stderr { Stdio::inherit() } else { Stdio::null() })
            .env_clear();
        for (name, value) in worker_environment() {
            command.env(name, value);
        }
        let mut child = command.spawn().map_err(|err| {
            ChannelError::Closed(format!(
                "spawn private worker {}: {err}",
                options.binary_path.display()
            ))
        })?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ChannelError::Closed("private worker pipes were not piped".to_string()));
        };
        let channel = Self::attach(child, stdin, stdout, generation);

        let initialization = serde_json::to_value(WorkerInitialization {
            configured_driver: options.configured_driver,
            host_bundle_id: options.host_bundle_id.clone(),
        })
        .map_err(|err| {
            ChannelError::Protocol(format!("serialize private worker initialization: {err}"))
        })?;
        let ready = channel
            .request_with_id(0, "initialize", None, Some(initialization), None, options.startup_timeout)
            .inspect_err(|_| channel.stop())?;
        if ready.get("ready").and_then(Value::as_bool) != Some(true)
            || ready.get("pid").and_then(Value::as_u64) == Some(u64::from(std::process::id()))
            || ready.get("host_bundle_id").and_then(Value::as_str) != Some(options.host_bundle_id.as_str())
        {
            channel.stop();
            return Err(ChannelError::Protocol(
                "private worker readiness proof did not match the spawned host generation".to_string(),
            ));
        }
        Ok(channel)
    }

    /// Wrap a spawned child: start the reader thread, no handshake (tests
    /// attach a fake worker here).
    fn attach(child: Child, stdin: ChildStdin, stdout: ChildStdout, generation: String) -> Arc<Self> {
        let router = Arc::new(Router::new(generation));
        let reader_router = router.clone();
        std::thread::Builder::new()
            .name("computer-channel-reader".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let line = match line {
                        Ok(line) => line,
                        Err(err) => {
                            log::warn!("[computer] private worker stdout: {err}");
                            break;
                        }
                    };
                    if let Err(why) = reader_router.dispatch_line(&line) {
                        log::warn!("[computer] private worker channel: {why}");
                        break;
                    }
                }
                reader_router.close();
            })
            .expect("spawn the computer channel reader");
        Arc::new(Self {
            router,
            child: Mutex::new(child),
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
        })
    }

    /// `false` once stopped, once the pipe ended, or once the process is gone.
    pub fn is_alive(&self) -> bool {
        if self.stopped.load(Ordering::Acquire) || self.router.is_closed() {
            return false;
        }
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// One worker operation, awaited on this thread with no lock held past
    /// the write: any number of callers overlap.
    pub fn request(
        &self,
        operation: &str,
        name: Option<String>,
        arguments: Option<Value>,
        session_handle: Option<String>,
        timeout: Duration,
    ) -> Result<Value, ChannelError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.request_with_id(id, operation, name, arguments, session_handle, timeout)
    }

    fn request_with_id(
        &self,
        id: u64,
        operation: &str,
        name: Option<String>,
        arguments: Option<Value>,
        session_handle: Option<String>,
        timeout: Duration,
    ) -> Result<Value, ChannelError> {
        if self.stopped.load(Ordering::Acquire) || self.router.is_closed() {
            return Err(ChannelError::Closed("the private worker channel is closed".to_string()));
        }
        let request = ChannelRequest {
            protocol_version: PRIVATE_WORKER_PROTOCOL_VERSION,
            request_id: id,
            generation: self.router.generation.clone(),
            operation: operation.to_string(),
            name,
            arguments,
            session_handle,
        };
        let mut line = serde_json::to_vec(&request)
            .map_err(|err| ChannelError::Protocol(format!("serialize private worker request: {err}")))?;
        line.push(b'\n');
        // The slot exists before the line is out: a reply can never beat it.
        let slot = self.router.register(id);
        let written = {
            let mut stdin = self.stdin.lock().unwrap();
            match stdin.as_mut() {
                Some(stdin) => stdin.write_all(&line).and_then(|()| stdin.flush()),
                None => {
                    self.router.forget(id);
                    return Err(ChannelError::Closed("the private worker channel is closed".to_string()));
                }
            }
        };
        if let Err(err) = written {
            self.router.forget(id);
            self.stop();
            return Err(ChannelError::Closed(format!("write private worker request: {err}")));
        }
        match slot.recv_timeout(timeout) {
            Ok(response) => interpret(response),
            Err(RecvTimeoutError::Timeout) => {
                self.router.forget(id);
                Err(ChannelError::Timeout(timeout))
            }
            Err(RecvTimeoutError::Disconnected) => Err(ChannelError::Closed(
                "private worker channel closed before completion was reported".to_string(),
            )),
        }
    }

    /// Kill the worker (it broke, or a graceful shutdown ran out of time).
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.stdin.lock().unwrap().take();
        let mut child = self.child.lock().unwrap();
        if !matches!(child.try_wait(), Ok(Some(_))) {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.router.close();
    }

    /// Ask the worker to shut down, close its stdin and wait up to `timeout`
    /// for it to exit; kill it past that.
    pub fn shutdown(&self, timeout: Duration) -> Result<(), ChannelError> {
        let answer = self.request("shutdown", None, None, None, timeout);
        self.stopped.store(true, Ordering::Release);
        self.stdin.lock().unwrap().take();
        let deadline = Instant::now() + timeout;
        loop {
            let mut child = self.child.lock().unwrap();
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    drop(child);
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                Err(err) => {
                    self.router.close();
                    return Err(ChannelError::Closed(format!("wait for private worker: {err}")));
                }
            }
        }
        self.router.close();
        answer.map(|_| ())
    }
}

impl Drop for WorkerChannel {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A reply into a result, the way cua reads one.
fn interpret(response: ChannelResponse) -> Result<Value, ChannelError> {
    if response.ok {
        if response.completion != ActionCompletion::Completed {
            return Err(ChannelError::Protocol(
                "private worker reported success without completed execution".to_string(),
            ));
        }
        return Ok(response.result.unwrap_or(Value::Null));
    }
    Err(ChannelError::Worker {
        code: response.error_code.unwrap_or_else(|| "worker_error".to_string()),
        text: response.error.unwrap_or_else(|| "request failed".to_string()),
        completion: response.completion,
    })
}

/// The worker's environment: cua's `embedded::safe_environment` with no
/// overrides, mirrored because cua keeps it `pub(crate)`. The host's
/// variables the allowlist names plus the managed-authorization ones (those
/// under their canonical upper-case name), nothing else: no tokens, no
/// agent credentials reach the worker.
fn worker_environment() -> Vec<(String, String)> {
    merge_safe_environment(std::env::vars())
}

fn merge_safe_environment(inherited: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut values = BTreeMap::new();
    for (name, value) in inherited {
        let managed = inherited_managed_environment_name(&name);
        if allowed_environment_name(&name) || managed {
            let name = if managed { name.to_ascii_uppercase() } else { name };
            values.insert(name.to_ascii_uppercase(), (name, value));
        }
    }
    values.into_values().collect()
}

/// cua `embedded::allowed_environment_name`, verbatim.
fn allowed_environment_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("LC_")
        || matches!(
            upper.as_str(),
            "CUA_DRIVER_WINDOW_CHANGE_TIMEOUT_MS"
                | "CUA_DRIVER_WINDOW_CHANGE_POLL_MS"
                | "CUA_DRIVER_KEY_GAP_MS"
                | "PATH"
                | "HOME"
                | "USER"
                | "LOGNAME"
                | "SHELL"
                | "TMPDIR"
                | "TMP"
                | "TEMP"
                | "LANG"
                | "SYSTEMROOT"
                | "WINDIR"
                | "COMSPEC"
                | "PATHEXT"
                | "APPDATA"
                | "LOCALAPPDATA"
                | "PROGRAMDATA"
                | "DISPLAY"
                | "WAYLAND_DISPLAY"
                | "XDG_RUNTIME_DIR"
                | "XDG_SESSION_TYPE"
                | "DBUS_SESSION_BUS_ADDRESS"
                | "XAUTHORITY"
                | "CUA_LOG"
                | "CUA_DRIVER_RS_TELEMETRY_ENABLED"
                | "CUA_TELEMETRY_ENABLED"
        )
}

/// cua `embedded::inherited_managed_environment_name`, verbatim.
fn inherited_managed_environment_name(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "CUA_DRIVER_PERMISSION_MODE"
            | "CUA_DRIVER_DANGEROUSLY_BYPASS_APPROVALS"
            | "CUA_DRIVER_DISABLE_UNRESTRICTED"
            | "CUA_DRIVER_SESSION_POLICY_FILE"
            | "CUA_DRIVER_SESSION_POLICY_APPROVED"
            | "CUA_DRIVER_CAPABILITY_MANIFEST_FILE"
            | "CUA_DRIVER_CAPABILITY_MANIFEST_APPROVED"
            | "CUA_DRIVER_POLICY_FILE"
            | "CUA_DRIVER_MANAGED_POLICY_FILE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(id: u64, generation: &str) -> String {
        serde_json::to_string(&ChannelResponse::ok(id, generation, json!({ "n": id }))).unwrap()
    }

    #[test]
    fn replies_route_by_id_in_any_order() {
        let router = Router::new("g1".to_string());
        let one = router.register(1);
        let two = router.register(2);
        router.dispatch_line(&ok(2, "g1")).unwrap();
        router.dispatch_line(&ok(1, "g1")).unwrap();
        assert_eq!(two.recv_timeout(Duration::from_secs(1)).unwrap().result, Some(json!({ "n": 2 })));
        assert_eq!(one.recv_timeout(Duration::from_secs(1)).unwrap().result, Some(json!({ "n": 1 })));
        // A reply nobody waits for (a caller that timed out) is not an error.
        router.dispatch_line(&ok(9, "g1")).unwrap();
        // Another generation or garbage is.
        assert!(router.dispatch_line(&ok(3, "g2")).is_err());
        assert!(router.dispatch_line("not json").is_err());
    }

    #[test]
    fn a_closed_router_fails_every_waiting_request() {
        let router = Router::new("g1".to_string());
        let one = router.register(1);
        let two = router.register(2);
        router.close();
        assert!(router.is_closed());
        for slot in [one, two] {
            assert!(matches!(slot.recv_timeout(Duration::from_secs(1)), Err(RecvTimeoutError::Disconnected)));
        }
    }

    #[test]
    fn a_refusal_and_a_half_done_success_are_told_apart() {
        let refused = ChannelResponse::error(1, "g", "session_not_bound", "gone", ActionCompletion::NotStarted);
        match interpret(refused) {
            Err(ChannelError::Worker { code, text, completion }) => {
                assert_eq!((code.as_str(), text.as_str(), completion), ("session_not_bound", "gone", ActionCompletion::NotStarted));
            }
            other => panic!("{other:?}"),
        }
        let mut odd = ChannelResponse::ok(1, "g", json!({}));
        odd.completion = ActionCompletion::Unknown;
        assert!(matches!(interpret(odd), Err(ChannelError::Protocol(_))));
        assert_eq!(interpret(ChannelResponse::ok(1, "g", json!({ "a": 1 }))).unwrap(), json!({ "a": 1 }));
    }

    #[test]
    fn the_worker_environment_is_cuas_allowlist() {
        let merged = merge_safe_environment([
            ("PATH".to_string(), "/bin".to_string()),
            ("lc_all".to_string(), "C".to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), "nope".to_string()),
            ("EXP_COMPUTER_TOKEN".to_string(), "nope".to_string()),
            ("cua_driver_disable_unrestricted".to_string(), "1".to_string()),
        ]);
        assert_eq!(
            merged,
            [
                ("CUA_DRIVER_DISABLE_UNRESTRICTED".to_string(), "1".to_string()),
                ("lc_all".to_string(), "C".to_string()),
                ("PATH".to_string(), "/bin".to_string()),
            ]
        );
    }

    /// A fake worker (`sh`) that ignores its requests and answers two of
    /// them in reverse order after a pause: both callers, waiting at the
    /// same time, get their own result; once it exits the channel is closed.
    #[cfg(unix)]
    #[test]
    fn two_overlapping_requests_get_their_own_replies_and_eof_closes_the_channel() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(r#"sleep 0.3; printf '%s\n' "$R2" "$R1""#)
            .env("R1", ok(1, "g1"))
            .env("R2", ok(2, "g1"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        let channel = WorkerChannel::attach(child, stdin, stdout, "g1".to_string());
        assert!(channel.is_alive());
        let started = Instant::now();
        let workers: Vec<_> = [1u64, 2]
            .into_iter()
            .map(|id| {
                let channel = channel.clone();
                std::thread::spawn(move || {
                    channel.request_with_id(id, "call", Some("click".into()), None, None, Duration::from_secs(5))
                })
            })
            .collect();
        for (id, worker) in [1u64, 2].into_iter().zip(workers) {
            assert_eq!(worker.join().unwrap().unwrap(), json!({ "n": id }));
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        // The fake exits after answering: EOF closes the channel for good.
        let late = channel.request("call", None, None, None, Duration::from_secs(2));
        assert!(matches!(late, Err(ChannelError::Closed(_))), "{late:?}");
        assert!(!channel.is_alive());
    }

    /// A caller that gives up does not take the worker down with it.
    #[cfg(unix)]
    #[test]
    fn a_timeout_fails_only_its_own_request() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(r#"sleep 0.4; printf '%s\n' "$R2"; sleep 5"#)
            .env("R2", ok(2, "g1"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        let channel = WorkerChannel::attach(child, stdin, stdout, "g1".to_string());
        let slow = channel.request_with_id(1, "call", None, None, None, Duration::from_millis(50));
        assert!(matches!(slow, Err(ChannelError::Timeout(_))), "{slow:?}");
        assert!(channel.is_alive());
        let answered = channel.request_with_id(2, "call", None, None, None, Duration::from_secs(5)).unwrap();
        assert_eq!(answered, json!({ "n": 2 }));
        channel.stop();
        assert!(!channel.is_alive());
    }
}
