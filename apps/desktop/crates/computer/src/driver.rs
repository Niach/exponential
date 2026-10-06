//! The host's handle on the cua driver: one private worker (see
//! [`crate::worker`]) for as long as the device's switch is on, one trusted
//! unrestricted session per run inside it.
//!
//! Unrestricted by decision (EXP-1196): the device switch is the one gate,
//! sharing already keeps other people off the machine, and cua's own
//! bounded/standard modes would put approval prompts in front of the owner
//! of the hardware. Whatever cua can do on this OS, a run can.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cua_driver_sdk::{
    ConfiguredDriverOptions, CuaDriver, CuaDriverSession, DriverError, PrivateWorkerOptions,
    RuntimeAuthorizationOptions, SessionPermissionMode, TrustedSessionOptions,
};
use serde_json::{json, Value};

use crate::server::ToolHost;

/// How long a run's session may live and idle (cua's ceilings; a run that
/// outlives them is bound again on its next call).
const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const SESSION_IDLE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const WORKER_STARTUP: Duration = Duration::from_secs(30);

struct Live {
    cua: Arc<CuaDriver>,
    tools: Value,
    sessions: HashMap<String, Arc<CuaDriverSession>>,
}

pub(crate) struct Driver {
    host_bundle_id: String,
    runtime: tokio::runtime::Runtime,
    live: Mutex<Option<Live>>,
}

impl Driver {
    pub fn new(host_bundle_id: &str) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("computer-driver")
            .enable_all()
            .build()
            .map_err(|err| format!("Could not start the computer-use runtime: {err}"))?;
        Ok(Self { host_bundle_id: host_bundle_id.to_string(), runtime, live: Mutex::new(None) })
    }

    pub fn is_live(&self) -> bool {
        self.live.lock().unwrap().is_some()
    }

    /// Spawn the worker unless it runs; the first call pays the start.
    pub fn start(&self) -> Result<(), String> {
        let mut live = self.live.lock().unwrap();
        if live.is_some() {
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|err| format!("own executable: {err}"))?;
        let cua = CuaDriver::create_private_worker(PrivateWorkerOptions {
            binary_path: exe.to_string_lossy().into_owned(),
            host_bundle_id: self.host_bundle_id.clone(),
            startup_timeout_ms: Some(WORKER_STARTUP.as_millis() as u64),
            shutdown_timeout_ms: Some(2_000),
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
            environment: Vec::new(),
            inherit_stderr: true,
        })
        .map_err(|err| format!("Could not start the computer-use worker: {err}"))?;
        let tools = self
            .runtime
            .block_on(cua.list_tools_json())
            .map_err(|err| format!("Could not list the computer-use tools: {err}"))
            .and_then(|json| serde_json::from_str::<Value>(&json).map_err(|err| err.to_string()))?
            .get("tools")
            .cloned()
            .ok_or("The computer-use tool list has no tools.")?;
        log::info!("[computer] cua worker up, {} tools", tools.as_array().map_or(0, Vec::len));
        *live = Some(Live { cua, tools, sessions: HashMap::new() });
        Ok(())
    }

    /// Shut the worker down (the switch turned off, or it broke).
    pub fn stop(&self) {
        let Some(live) = self.live.lock().unwrap().take() else { return };
        drop(live.sessions);
        if let Err(err) = self.runtime.block_on(live.cua.shutdown()) {
            log::warn!("[computer] cua worker shutdown: {err}");
        }
    }

    fn session(&self, session_id: &str, label: &str) -> Result<Arc<CuaDriverSession>, String> {
        self.start()?;
        let mut guard = self.live.lock().unwrap();
        let live = guard.as_mut().ok_or("The computer-use worker is not running.")?;
        if let Some(session) = live.sessions.get(session_id) {
            return Ok(session.clone());
        }
        let session = live
            .cua
            .create_trusted_session(TrustedSessionOptions {
                public_session: label.to_string(),
                mode: SessionPermissionMode::Unrestricted,
                ttl_seconds: SESSION_TTL.as_secs(),
                idle_ttl_seconds: SESSION_IDLE_TTL.as_secs(),
                capability_manifest_path: None,
                bounded_manifest_path: None,
            })
            .map_err(|err| format!("Could not open a computer-use session: {err}"))?;
        live.sessions.insert(session_id.to_string(), session.clone());
        Ok(session)
    }

    fn forget_session(&self, session_id: &str) {
        if let Some(live) = self.live.lock().unwrap().as_mut() {
            live.sessions.remove(session_id);
        }
    }
}

/// `true` when the worker itself is gone or wedged, not just this call.
fn worker_lost(err: &DriverError) -> bool {
    match err {
        DriverError::Shutdown
        | DriverError::ActionInterrupted { .. }
        | DriverError::Protocol { .. }
        | DriverError::Transport { .. } => true,
        DriverError::Worker { reason } => !reason.starts_with("worker_request_failed:"),
        _ => false,
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
        let session = match self.session(session_id, label) {
            Ok(session) => session,
            Err(text) => return error_result(text),
        };
        let outcome = self.runtime.block_on(session.call_tool(name.to_string(), arguments.to_string()));
        match outcome {
            Ok(result) => serde_json::from_str(&result.raw_json).unwrap_or_else(|_| {
                json!({ "content": [{ "type": "text", "text": result.text }], "isError": result.is_error })
            }),
            Err(err) if worker_lost(&err) => {
                log::warn!("[computer] cua worker lost on {name}: {err}");
                self.stop();
                error_result(format!("Computer use restarted ({err}). Observe the current state and call again."))
            }
            Err(DriverError::Worker { reason }) => {
                // The session may have expired inside the worker: bind anew
                // next time.
                if reason.contains("session") {
                    self.forget_session(session_id);
                }
                error_result(reason.trim_start_matches("worker_request_failed:").trim().to_string())
            }
            Err(err) => error_result(err.to_string()),
        }
    }

    fn end(&self, session_id: &str) {
        let session = self.live.lock().unwrap().as_mut().and_then(|live| live.sessions.remove(session_id));
        if let Some(session) = session {
            session.close();
        }
    }
}
