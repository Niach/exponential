//! The data source, ede-protocol framing (a `u32` length then JSON).
//! Calls: `{"id", "op": "list" | "get" {domain} | "call" {domain, action,
//! args} | "subscribe"}`; replies `{"id", "result"}` or `{"id", "error"}`;
//! after `subscribe`, `{"event": "services.changed", "domain", "snapshot"}`
//! whenever a domain's snapshot moves (a subscriber subscribes, then gets:
//! the events heard during its gets wait for it, #EDE-135).
//!
//! Its connections come one of two ways:
//! - **#EDE-172 (P13), the compositor's intake** ([`serve_intake`],
//!   `[session] helpers = "compositor"`): the compositor hands each over
//!   with the capabilities of the principal that asked for it
//!   ([`ede_protocol::Request::OpenServices`]), and its `call` holds
//!   exactly those. No socket path exists.
//! - **`$XDG_RUNTIME_DIR/exponential-desktop/services.sock`** ([`serve`];
//!   the directory is 0700, the compositor's): any process of the person,
//!   and `call` holds every capability (the shell's own buttons). Until
//!   eDE's flip (P14).
//!
//! #EDE-172: `grant` (an MCP bearer over the socket) is gone; the MCP face
//! ([`crate::mcp`]) serves nothing until agents reach it through a relay.

use std::{
    collections::VecDeque,
    io::Read,
    os::{
        fd::OwnedFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ede_protocol::{read_frame, write_frame, Capability};
use serde_json::{json, Value};

use crate::Registry;

/// Every subscriber's stream.
#[derive(Clone, Default)]
pub struct Subscribers(Arc<Mutex<Vec<UnixStream>>>);

impl Subscribers {
    pub fn broadcast(&self, event: &Value) {
        let bytes = serde_json::to_vec(event).unwrap_or_default();
        let mut subs = self.0.lock().unwrap_or_else(|p| p.into_inner());
        subs.retain_mut(|s| write_frame(s, &bytes).is_ok());
    }

    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).is_empty()
    }
}

/// How often a domain's poll thread checks for listeners and nudges.
const POLL_TICK: Duration = Duration::from_millis(200);

/// One domain's poll (#EDE-135): its own thread, so a slow or hung domain
/// holds up no other; it looks only while someone subscribes, every
/// [`crate::Service::interval`] or at once after an action.
fn poll(name: &'static str, registry: &Registry, subscribers: &Subscribers) {
    let interval = registry.interval(name).unwrap_or(Duration::from_secs(2));
    let mut last: Option<Instant> = None;
    loop {
        if subscribers.is_empty() {
            thread::sleep(POLL_TICK);
            continue;
        }
        let due = last.is_none_or(|t| t.elapsed() >= interval) | registry.take_nudge(name);
        if due {
            last = Some(Instant::now());
            if let Some(snapshot) = registry.refresh_one(name) {
                subscribers.broadcast(&json!({"event": "services.changed", "domain": name, "snapshot": snapshot}));
            }
        }
        thread::sleep(POLL_TICK);
    }
}

/// The poll: each domain's changed snapshots to every subscriber.
fn start_polls(registry: &Arc<Registry>, subscribers: &Subscribers) -> std::io::Result<()> {
    for name in registry.names() {
        let registry = registry.clone();
        let subscribers = subscribers.clone();
        thread::Builder::new().name(format!("ede-services-poll-{name}")).spawn(move || poll(name, &registry, &subscribers))?;
    }
    Ok(())
}

/// One connection, served on its own thread with `held` for its calls.
fn spawn_client(stream: UnixStream, registry: &Arc<Registry>, subscribers: &Subscribers, held: Vec<Capability>) -> std::io::Result<()> {
    let registry = registry.clone();
    let subscribers = subscribers.clone();
    thread::Builder::new().name("ede-services-client".into()).spawn(move || serve_client(stream, &registry, &subscribers, &held))?;
    Ok(())
}

/// Serves `services.sock` until the listener fails: every connection's
/// `call` holds every capability.
pub fn serve(path: &Path, registry: Arc<Registry>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    let subscribers = Subscribers::default();
    start_polls(&registry, &subscribers)?;
    tracing::info!(path = %path.display(), "services socket");
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        spawn_client(stream, &registry, &subscribers, Capability::ALL.to_vec())?;
    }
    Ok(())
}

/// #EDE-172 (P13): serves the connections the compositor hands over
/// `intake` ([`ede_protocol::intake`]), each with the capabilities it
/// names, until the compositor closes it (`Ok`: it ended, or started
/// another daemon).
pub fn serve_intake(intake: OwnedFd, registry: Arc<Registry>) -> std::io::Result<()> {
    let subscribers = Subscribers::default();
    start_polls(&registry, &subscribers)?;
    tracing::info!("services served over the compositor's intake");
    while let Some((handed, connection)) = ede_protocol::intake::receive(&intake)? {
        tracing::info!(session = handed.session_id.as_str(), capabilities = ?handed.capabilities, "a connection from the compositor");
        spawn_client(UnixStream::from(connection), &registry, &subscribers, handed.capabilities)?;
    }
    tracing::info!("the compositor closed the intake");
    Ok(())
}

fn serve_client(mut stream: UnixStream, registry: &Registry, subscribers: &Subscribers, held: &[Capability]) {
    loop {
        let Ok(frame) = read_frame(&mut stream) else { return };
        let call: Value = match serde_json::from_slice(&frame) {
            Ok(v) => v,
            Err(e) => {
                let _ = write_frame(&mut stream, &serde_json::to_vec(&json!({"id": 0, "error": e.to_string()})).unwrap());
                continue;
            }
        };
        let id = call["id"].clone();
        let reply = match handle(&call, registry, held) {
            Ok(Some(result)) => json!({"id": id, "result": result}),
            Ok(None) => {
                // subscribe: this stream joins the broadcast.
                if let Ok(clone) = stream.try_clone() {
                    subscribers.0.lock().unwrap_or_else(|p| p.into_inner()).push(clone);
                }
                json!({"id": id, "result": null})
            }
            Err(e) => json!({"id": id, "error": e}),
        };
        if write_frame(&mut stream, &serde_json::to_vec(&reply).unwrap()).is_err() {
            return;
        }
    }
}

/// `Ok(None)` = subscribe. `held` = what this connection's `call` holds.
pub fn handle(call: &Value, registry: &Registry, held: &[Capability]) -> Result<Option<Value>, String> {
    match call["op"].as_str().unwrap_or_default() {
        "list" => Ok(Some(json!(registry.names()))),
        "get" => {
            let domain = call["domain"].as_str().ok_or("domain")?;
            registry.snapshot(domain).map(Some).ok_or_else(|| format!("no domain {domain}"))
        }
        "call" => {
            let domain = call["domain"].as_str().ok_or("domain")?;
            let action = call["action"].as_str().ok_or("action")?;
            let args = call.get("args").cloned().unwrap_or(json!({}));
            registry.call(domain, action, &args, held).map(Some).map_err(|e| e.to_string())
        }
        "subscribe" => Ok(None),
        // #EDE-172: no MCP bearer is minted over a connection any more.
        "grant" => Err("grant is gone: the services' MCP face mints no bearers over a socket (#EDE-172)".into()),
        other => Err(format!("unknown op {other:?}")),
    }
}

/// A client of the data source (the shell, the settings pages, tests).
pub struct Client {
    stream: UnixStream,
    next: u64,
    /// Events heard while a call waited for its reply.
    pending: VecDeque<Value>,
}

impl Client {
    pub fn connect(path: &Path) -> std::io::Result<Self> {
        Ok(Self { stream: UnixStream::connect(path)?, next: 1, pending: VecDeque::new() })
    }

    pub fn connect_default() -> std::io::Result<Self> {
        Self::connect(&crate::socket_path())
    }

    /// #EDE-172 (P13): a connection the compositor handed over
    /// ([`ede_protocol::Client::open_services`]).
    pub fn from_fd(fd: OwnedFd) -> Self {
        Self { stream: UnixStream::from(fd), next: 1, pending: VecDeque::new() }
    }

    pub fn call(&mut self, mut call: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        call["id"] = json!(id);
        write_frame(&mut self.stream, &serde_json::to_vec(&call).unwrap()).map_err(|e| e.to_string())?;
        loop {
            let frame = read_frame(&mut self.stream).map_err(|e| e.to_string())?;
            let reply: Value = serde_json::from_slice(&frame).map_err(|e| e.to_string())?;
            if reply.get("event").is_some() {
                self.pending.push_back(reply);
                continue;
            }
            if reply["id"] == json!(id) {
                return match reply.get("error") {
                    Some(e) => Err(e.as_str().unwrap_or("error").to_string()),
                    None => Ok(reply["result"].clone()),
                };
            }
        }
    }

    /// A fresh snapshot of `domain`; the events of `domain` heard before it
    /// are older and dropped.
    pub fn get(&mut self, domain: &str) -> Result<Value, String> {
        let value = self.call(json!({"op": "get", "domain": domain}))?;
        self.pending.retain(|e| e["domain"] != domain);
        Ok(value)
    }

    pub fn subscribe(&mut self) -> Result<(), String> {
        self.call(json!({"op": "subscribe"})).map(|_| ())
    }

    /// The next `services.changed` event (blocks).
    pub fn next_event(&mut self) -> Result<Value, String> {
        if let Some(event) = self.pending.pop_front() {
            return Ok(event);
        }
        loop {
            let frame = read_frame(&mut self.stream).map_err(|e| e.to_string())?;
            let v: Value = serde_json::from_slice(&frame).map_err(|e| e.to_string())?;
            if v.get("event").is_some() {
                return Ok(v);
            }
        }
    }

    pub fn set_read_timeout(&self, t: Option<Duration>) -> std::io::Result<()> {
        self.stream.set_read_timeout(t)
    }
}

/// A sink for `Read` so unused imports stay honest in tests.
#[allow(dead_code)]
fn _read(_: &mut dyn Read) {}

/// A one-shot channel-free helper for tests: the first event within `t`.
pub fn wait_event(client: &mut Client, t: Duration) -> Option<Value> {
    let (tx, rx) = mpsc::channel();
    let _ = client.set_read_timeout(Some(t));
    let _ = tx.send(client.next_event().ok());
    rx.recv().ok().flatten()
}
