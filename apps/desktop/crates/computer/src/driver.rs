//! The host's handle on the cua driver: one private worker (see
//! [`crate::worker`]) for as long as the device's switch is on, one trusted
//! unrestricted session per run inside it.
//!
//! Unrestricted by decision (EXP-1196): the device switch is the one gate,
//! sharing already keeps other people off the machine, and cua's own
//! bounded/standard modes would put approval prompts in front of the owner
//! of the hardware. Whatever cua can do on this OS, a run can.
//!
//! EXP-1236: the host speaks to the worker over [`crate::channel`], not
//! cua's `CuaDriver`, so calls from several runs (or one run's `Promise.all`
//! over several windows) overlap instead of queueing; the worker end is the
//! concurrent loop in `worker.rs`. The host holds no tokio runtime any more:
//! a call blocks its own HTTP thread and nothing else.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cua_driver_sdk::worker::ActionCompletion;
use cua_driver_sdk::{
    ConfiguredDriverOptions, RuntimeAuthorizationOptions, SessionPermissionMode, TrustedSessionOptions,
};
use serde_json::{json, Value};

use crate::channel::{ChannelError, SpawnOptions, WorkerChannel};
use crate::server::ToolHost;

/// How long a run's session may live and idle (cua's ceilings; a run that
/// outlives them is bound again on its next call).
const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const SESSION_IDLE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const WORKER_STARTUP: Duration = Duration::from_secs(30);
/// cua's own per-request ceiling. Past it the CALL fails and the agent is
/// told to look again; the worker and every other call in flight live on.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const BIND_TIMEOUT: Duration = Duration::from_secs(30);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

struct Live {
    channel: Arc<WorkerChannel>,
    tools: Value,
    /// Run (session id) to the worker's session handle.
    sessions: HashMap<String, String>,
}

pub(crate) struct Driver {
    host_bundle_id: String,
    live: Mutex<Option<Live>>,
}

impl Driver {
    pub fn new(host_bundle_id: &str) -> Result<Self, String> {
        Ok(Self { host_bundle_id: host_bundle_id.to_string(), live: Mutex::new(None) })
    }

    pub fn is_live(&self) -> bool {
        self.live.lock().unwrap().is_some()
    }

    /// Spawn the worker unless it runs; the first call pays the start. A
    /// worker that died behind our back (a crash, an OS kill) is replaced.
    pub fn start(&self) -> Result<(), String> {
        let mut live = self.live.lock().unwrap();
        if let Some(current) = live.as_ref() {
            if current.channel.is_alive() {
                return Ok(());
            }
            log::warn!("[computer] cua worker is gone; starting another");
            if let Some(dead) = live.take() {
                dead.channel.stop();
            }
        }
        let exe = std::env::current_exe().map_err(|err| format!("own executable: {err}"))?;
        let channel = WorkerChannel::spawn(SpawnOptions {
            binary_path: exe,
            host_bundle_id: self.host_bundle_id.clone(),
            startup_timeout: WORKER_STARTUP,
            configured_driver: ConfiguredDriverOptions {
                claude_code_compatibility: false,
                authorization: RuntimeAuthorizationOptions {
                    allowed_modes: vec![SessionPermissionMode::Unrestricted],
                    compatibility_mode: SessionPermissionMode::Unrestricted,
                    compatibility_capability_manifest_path: None,
                    compatibility_bounded_manifest_path: None,
                    unrestricted_acknowledged: true,
                    max_session_ttl_seconds: SESSION_TTL.as_secs(),
                    max_idle_ttl_seconds: SESSION_IDLE_TTL.as_secs(),
                },
            },
            inherit_stderr: true,
        })
        .map_err(|err| format!("Could not start the computer-use worker: {err}"))?;
        let tools = channel
            .request("list", None, None, None, WORKER_STARTUP)
            .map_err(|err| {
                channel.stop();
                format!("Could not list the computer-use tools: {err}")
            })?
            .get("tools")
            .cloned()
            .ok_or("The computer-use tool list has no tools.")?;
        log::info!("[computer] cua worker up, {} tools", tools.as_array().map_or(0, Vec::len));
        *live = Some(Live { channel, tools, sessions: HashMap::new() });
        Ok(())
    }

    /// Shut the worker down (the switch turned off, or it broke).
    pub fn stop(&self) {
        let Some(live) = self.live.lock().unwrap().take() else { return };
        if let Err(err) = live.channel.shutdown(SHUTDOWN_TIMEOUT) {
            log::warn!("[computer] cua worker shutdown: {err}");
        }
    }

    /// `channel` broke: drop it, unless another start already replaced it
    /// (a concurrent call must not kill the new worker).
    fn lose(&self, channel: &Arc<WorkerChannel>) {
        let mut live = self.live.lock().unwrap();
        if live.as_ref().is_some_and(|live| Arc::ptr_eq(&live.channel, channel)) {
            live.take();
        }
        drop(live);
        channel.stop();
    }

    /// The run's worker session, bound on first use. The bind happens under
    /// the lock (a few milliseconds inside the worker, never behind another
    /// run's action now that the worker is concurrent) so two first calls
    /// of one run cannot bind twice.
    fn session(&self, session_id: &str, label: &str) -> Result<(Arc<WorkerChannel>, String), String> {
        self.start()?;
        let mut guard = self.live.lock().unwrap();
        let live = guard.as_mut().ok_or("The computer-use worker is not running.")?;
        if let Some(handle) = live.sessions.get(session_id) {
            return Ok((live.channel.clone(), handle.clone()));
        }
        // The public session names the cursor badge AND must be unique
        // among live sessions (two chats are both "Chat"): the run's id
        // tails the label.
        let short = session_id.get(..8).unwrap_or(session_id);
        let public_session = if label.is_empty() { short.to_string() } else { format!("{label} {short}") };
        let options = serde_json::to_value(TrustedSessionOptions {
            public_session,
            mode: SessionPermissionMode::Unrestricted,
            ttl_seconds: SESSION_TTL.as_secs(),
            idle_ttl_seconds: SESSION_IDLE_TTL.as_secs(),
            capability_manifest_path: None,
            bounded_manifest_path: None,
        })
        .map_err(|err| format!("Could not open a computer-use session: {err}"))?;
        let channel = live.channel.clone();
        let bound = channel.request("bind_session", None, Some(options), None, BIND_TIMEOUT);
        let handle = match bound {
            Ok(value) => value
                .get("session_handle")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or("The computer-use worker bound a session without a handle.")?,
            Err(err) if worker_lost(&err) => {
                drop(guard);
                self.lose(&channel);
                return Err(format!("Computer use restarted ({err}). Call again."));
            }
            Err(err) => return Err(format!("Could not open a computer-use session: {err}")),
        };
        live.sessions.insert(session_id.to_string(), handle.clone());
        Ok((channel, handle))
    }

    /// Wayland's portals consent on first use (the screenshot portal, the
    /// remote-desktop one behind libei input) and cua persists what they
    /// grant: make that first use NOW, while the person who flipped the
    /// switch is there, so no dialog interrupts a run. One desktop capture
    /// and one pointer move to where the pointer already is; nothing the
    /// person can see change. Other sessions: nothing to warm.
    pub fn warm_up(&self) {
        if cfg!(not(target_os = "linux")) || std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return;
        }
        const SETUP: &str = "computer-use-setup";
        let shot = self.call(SETUP, "Setup", "get_desktop_state", json!({ "max_image_dimension": 64 }));
        let position = self.call(SETUP, "Setup", "get_cursor_position", json!({}));
        let point = position.pointer("/structuredContent").and_then(|value| {
            Some((value.get("x")?.as_f64()?, value.get("y")?.as_f64()?))
        });
        let moved = point.map(|(x, y)| {
            self.call(
                SETUP,
                "Setup",
                "move_cursor",
                json!({ "x": x, "y": y, "target": { "kind": "desktop", "display_id": "primary" } }),
            )
        });
        for (name, result) in [("capture", Some(shot)), ("input", moved)] {
            if let Some(result) = result {
                if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
                    log::warn!("[computer] wayland warm-up, {name}: {}", result["content"][0]["text"]);
                }
            }
        }
        self.end(SETUP);
    }

    fn forget_session(&self, session_id: &str) {
        if let Some(live) = self.live.lock().unwrap().as_mut() {
            live.sessions.remove(session_id);
        }
    }
}

/// `true` when the worker itself is gone or wedged, not just this call.
fn worker_lost(err: &ChannelError) -> bool {
    match err {
        ChannelError::Closed(_) | ChannelError::Protocol(_) => true,
        // The worker does not know what state its action left; cua treats
        // that as an interrupted runtime too.
        ChannelError::Worker { completion: ActionCompletion::Unknown, .. } => true,
        // Our lines reach a worker of another generation: not ours any more.
        ChannelError::Worker { code, .. } => code == "generation_mismatch",
        ChannelError::Timeout(_) => false,
    }
}

fn error_result(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": true })
}

impl ToolHost for Driver {
    fn tools(&self) -> Result<Value, String> {
        self.start()?;
        Ok(self.live.lock().unwrap().as_ref().map(|live| live.tools.clone()).unwrap_or(json!([])))
    }

    fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
        let (channel, handle) = match self.session(session_id, label) {
            Ok(session) => session,
            Err(text) => return error_result(text),
        };
        // No lock is held from here: this is where calls overlap (EXP-1236).
        match channel.request("call", Some(name.to_string()), Some(arguments), Some(handle), CALL_TIMEOUT) {
            // The worker already parsed cua's MCP result (content, isError,
            // structuredContent): it passes through.
            Ok(result) => result,
            Err(ChannelError::Timeout(after)) => error_result(format!(
                "Computer use did not answer {name} within {} s; the action may still be running. Observe the current state and call again.",
                after.as_secs()
            )),
            Err(err) if worker_lost(&err) => {
                log::warn!("[computer] cua worker lost on {name}: {err}");
                self.lose(&channel);
                error_result(format!("Computer use restarted ({err}). Observe the current state and call again."))
            }
            Err(ChannelError::Worker { code, text, .. }) => {
                // The session may have expired inside the worker: bind anew
                // next time.
                if code == "session_not_bound" || text.contains("session") {
                    self.forget_session(session_id);
                }
                error_result(text)
            }
            Err(err) => error_result(err.to_string()),
        }
    }

    fn end(&self, session_id: &str) {
        let bound = self.live.lock().unwrap().as_mut().and_then(|live| {
            live.sessions.remove(session_id).map(|handle| (live.channel.clone(), handle))
        });
        if let Some((channel, handle)) = bound {
            if let Err(err) = channel.request("close_session", None, None, Some(handle), CLOSE_TIMEOUT) {
                log::debug!("[computer] close session {session_id}: {err}");
            }
        }
    }
}
