//! The data source: `$XDG_RUNTIME_DIR/ede/services.sock` (the directory
//! is 0700, the compositor's), ede-protocol framing (a `u32` length then
//! JSON). Calls: `{"id", "op": "list" | "get" {domain} | "call" {domain,
//! action, args} | "subscribe" | "grant" {session, label, capabilities}}`;
//! replies `{"id", "result"}` or `{"id", "error"}`; after `subscribe`,
//! `{"event": "services.changed", "domain", "snapshot"}` whenever a
//! domain's snapshot moves. Same-user processes reach it: the shell, the
//! settings pages, ede-tools. `call` from the socket holds every
//! capability (the shell's own buttons); agents go through the MCP face.

use std::{
    io::Read,
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

use ede_protocol::{read_frame, write_frame, Capability};
use serde_json::{json, Value};

use crate::{mcp::Mcp, Registry};

/// Every subscriber's stream.
#[derive(Clone, Default)]
pub struct Subscribers(Arc<Mutex<Vec<UnixStream>>>);

impl Subscribers {
    pub fn broadcast(&self, event: &Value) {
        let bytes = serde_json::to_vec(event).unwrap_or_default();
        let mut subs = self.0.lock().unwrap_or_else(|p| p.into_inner());
        subs.retain_mut(|s| write_frame(s, &bytes).is_ok());
    }
}

/// Serves until the listener fails; `refresh` runs on its own thread.
pub fn serve(path: &Path, registry: Arc<Registry>, mcp: Option<Arc<Mcp>>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    let subscribers = Subscribers::default();
    // The poll: snapshots every 2 s, the changed ones to every subscriber.
    {
        let registry = registry.clone();
        let subscribers = subscribers.clone();
        thread::Builder::new().name("ede-services-poll".into()).spawn(move || loop {
            for (domain, snapshot) in registry.refresh() {
                subscribers.broadcast(&json!({"event": "services.changed", "domain": domain, "snapshot": snapshot}));
            }
            thread::sleep(Duration::from_secs(2));
        })?;
    }
    tracing::info!(path = %path.display(), "services socket");
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let registry = registry.clone();
        let subscribers = subscribers.clone();
        let mcp = mcp.clone();
        thread::Builder::new().name("ede-services-client".into()).spawn(move || serve_client(stream, &registry, &subscribers, mcp.as_deref()))?;
    }
    Ok(())
}

fn serve_client(mut stream: UnixStream, registry: &Registry, subscribers: &Subscribers, mcp: Option<&Mcp>) {
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
        let reply = match handle(&call, registry, mcp) {
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

/// `Ok(None)` = subscribe.
pub fn handle(call: &Value, registry: &Registry, mcp: Option<&Mcp>) -> Result<Option<Value>, String> {
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
            registry.call(domain, action, &args, &Capability::ALL).map(Some).map_err(|e| e.to_string())
        }
        "subscribe" => Ok(None),
        "grant" => {
            let mcp = mcp.ok_or("no MCP face")?;
            let session = call["session"].as_str().ok_or("session")?;
            let label = call["label"].as_str().unwrap_or("agent");
            let capabilities: Vec<Capability> = call["capabilities"].as_array().map(|l| l.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect()).unwrap_or_default();
            let (url, token) = mcp.grant(session, label, capabilities)?;
            Ok(Some(json!({"url": url, "token": token})))
        }
        other => Err(format!("unknown op {other:?}")),
    }
}

/// A client of the data source (the shell, the settings pages, tests).
pub struct Client {
    stream: UnixStream,
    next: u64,
}

impl Client {
    pub fn connect(path: &Path) -> std::io::Result<Self> {
        Ok(Self { stream: UnixStream::connect(path)?, next: 1 })
    }

    pub fn connect_default() -> std::io::Result<Self> {
        Self::connect(&crate::socket_path())
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

    pub fn get(&mut self, domain: &str) -> Result<Value, String> {
        self.call(json!({"op": "get", "domain": domain}))
    }

    pub fn subscribe(&mut self) -> Result<(), String> {
        self.call(json!({"op": "subscribe"})).map(|_| ())
    }

    /// The next `services.changed` event (blocks).
    pub fn next_event(&mut self) -> Result<Value, String> {
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
