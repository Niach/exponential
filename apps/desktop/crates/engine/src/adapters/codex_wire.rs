//! EXP-746 — codex's private wire: a long-lived `codex app-server` JSON-RPC
//! connection. Owned by lane E3.
//!
//! `coding::codex_app_server` deliberately is NOT this: it is a one-shot
//! probe whose `route_line` returns `None` for anything carrying a `method`,
//! which would silently drop every server-initiated approval and hang the
//! session with no symptom. Hence the 3-way [`classify_line`]. What IS reused
//! verbatim: `rpc_request`, `rpc_notification` and `ChildGuard`.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;
use std::sync::Arc;

use serde_json::Value;

use crate::transport::LineWriter;

/// A request the SERVER sent us (an approval, an elicitation). `id` stays a
/// `Value` because the app-server's id shape is not ours to normalize.
#[derive(Clone, Debug)]
pub struct ServerRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

/// The three things a line off the app-server can be, plus junk.
#[derive(Clone, Debug)]
pub enum Incoming {
    /// An answer to one of OUR requests. `Err` carries the JSON-RPC error.
    Response {
        id: u64,
        result: Result<Value, Value>,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A log line, a malformed frame — never mistaken for an answer.
    Junk,
}

/// Classify one line. The rule `route_line` gets wrong: a frame with BOTH an
/// `id` and a `method` is a server REQUEST, not our response.
// EXP-746 E3: fill
#[allow(unused_variables)]
pub fn classify_line(line: &str) -> Incoming {
    todo!("EXP-746 E3: id+method = server request, id only = response, method only = notification")
}

/// The live connection: id → waiter registry, the two inbound channels, and
/// the shared writer.
pub struct AppServer {
    #[allow(dead_code)]
    next_id: AtomicU64,
    #[allow(dead_code)]
    pending: Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Result<Value, Value>>>>,
    #[allow(dead_code)]
    writer: LineWriter,
}

impl AppServer {
    /// Spawn `codex app-server --listen stdio://` and start routing its lines.
    /// Returns the connection plus the notification stream, the server-request
    /// stream and the child's exit.
    // EXP-746 E3: fill
    #[allow(unused_variables, clippy::type_complexity)]
    pub fn spawn(
        spec: &terminal::pty::SpawnSpec,
    ) -> std::io::Result<(
        Arc<AppServer>,
        flume::Receiver<(String, Value)>,
        flume::Receiver<ServerRequest>,
        flume::Receiver<terminal::pty::ChildExit>,
    )> {
        todo!("EXP-746 E3: spawn_lines + a router thread over classify_line")
    }

    /// One request, awaited. `Err` is the app-server's own JSON-RPC error.
    // EXP-746 E3: fill
    #[allow(unused_variables)]
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, Value> {
        todo!("EXP-746 E3: register a waiter, write the line, await the oneshot")
    }

    // EXP-746 E3: fill
    #[allow(unused_variables)]
    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        todo!("EXP-746 E3: write one notification line")
    }

    /// Answer a server-initiated request (an approval decision).
    // EXP-746 E3: fill
    #[allow(unused_variables)]
    pub fn respond(&self, id: Value, result: Value) -> std::io::Result<()> {
        todo!("EXP-746 E3: write one response line")
    }
}
