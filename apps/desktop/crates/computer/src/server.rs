//! The computer-use MCP endpoint: streamable HTTP on loopback, answered with
//! plain JSON (no SSE stream; a `GET` is refused, which the transport
//! allows). One listener per host process; each run reaches it with its own
//! bearer token ([`crate::grant`]), which is what names the run. The tools
//! are cua's, verbatim: the hub only routes a call to the run's session.

// Without cua linked nothing serves; the hub still compiles (and its tests
// run against the fake host) so the two builds share one file.
#![cfg_attr(not(feature = "cua"), allow(dead_code))]

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

/// What the hub asks of the driver ([`crate::driver::Driver`] when cua is
/// linked; a fake in tests).
pub(crate) trait ToolHost: Send + Sync {
    /// cua's `tools/list` tools array.
    fn tools(&self) -> Result<Value, String>;
    /// One `tools/call`, answered as an MCP call result (never an error at
    /// the JSON-RPC level: a refusal is content the agent must read).
    fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value;
    /// The run ended.
    fn end(&self, session_id: &str);
}

/// The config key and server name the agents see (`mcp__computer__click`).
pub const SERVER_NAME: &str = "computer";
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const BODY_MAX_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) struct Run {
    pub session_id: String,
    /// What the agent cursor's badge shows for this run.
    pub label: String,
}

pub(crate) struct Hub {
    pub host: Arc<dyn ToolHost>,
    /// Token → run.
    runs: Mutex<HashMap<String, Run>>,
}

impl Hub {
    pub fn new(host: Arc<dyn ToolHost>) -> Self {
        Self { host, runs: Mutex::default() }
    }

    /// A fresh token for `session_id`; an earlier one of the same run dies.
    pub fn grant(&self, session_id: &str, label: &str) -> String {
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|_, run| run.session_id != session_id);
        runs.insert(token.clone(), Run { session_id: session_id.to_string(), label: label.to_string() });
        token
    }

    pub fn revoke(&self, session_id: &str) {
        self.runs.lock().unwrap().retain(|_, run| run.session_id != session_id);
        self.host.end(session_id);
    }

    pub fn revoke_all(&self) {
        let runs = std::mem::take(&mut *self.runs.lock().unwrap());
        for run in runs.values() {
            self.host.end(&run.session_id);
        }
    }

    fn run(&self, token: &str) -> Option<(String, String)> {
        self.runs.lock().unwrap().get(token).map(|run| (run.session_id.clone(), run.label.clone()))
    }

    /// One JSON-RPC message → its response (`None` for a notification).
    pub fn handle(&self, token: &str, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version = PROTOCOL_VERSIONS
                    .iter()
                    .find(|version| **version == asked)
                    .unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => self
                .host
                .tools()
                .map(|tools| json!({ "tools": tools }))
                .map_err(|text| json!({ "code": -32603, "message": text })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match self.run(token) {
                    Some((session_id, label)) => Ok(self.host.call(&session_id, &label, name, arguments)),
                    None => Err(json!({ "code": -32000, "message": "This run's computer-use grant ended." })),
                }
            }
            _ => Err(json!({ "code": -32601, "message": format!("Method not found: {method}") })),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        })
    }
}

/// What the agent reads at `initialize`; the per-tool schemas carry the
/// rest. The one Exponential rule on top of cua's: say so in the run before
/// the first foreground action, because that one moves the person's pointer.
const INSTRUCTIONS: &str = "This computer's desktop, through the cua driver. Find the target with \
list_apps / list_windows, observe it with get_window_state (accessibility tree + screenshot), act \
with click / type_text / press_key / hotkey on an exact target {kind:\"window\", pid, window_id} and \
delivery_mode \"background\" (the window is driven in place; the person's pointer, focus and \
keyboard stay theirs), then verify with verify_state or a fresh get_window_state. A \
background_unavailable refusal is the only reason to switch to delivery_mode \"foreground\" or a \
desktop target (get_desktop_state + {kind:\"desktop\", display_id:\"primary\"}): tell the person in \
your reply first, because that takes over their pointer and keyboard. Use returned element tokens, \
never invented indices; an unverifiable effect is not success.";

/// Bind loopback and serve `hub` on a background thread; returns the URL.
pub(crate) fn serve(hub: Arc<Hub>) -> Result<String, String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|err| format!("Could not start the computer-use server: {err}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or("The computer-use server has no TCP address.")?;
    std::thread::Builder::new()
        .name("computer-use-mcp".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let hub = hub.clone();
                // A tool call may run for seconds (a bounded verify poll):
                // never hold the accept loop for it.
                let _ = std::thread::Builder::new()
                    .name("computer-use-call".into())
                    .spawn(move || respond(&hub, request));
            }
        })
        .map_err(|err| format!("Could not start the computer-use server: {err}"))?;
    Ok(format!("http://127.0.0.1:{port}/mcp"))
}

fn respond(hub: &Hub, mut request: tiny_http::Request) {
    let json_header = || {
        tiny_http::Header::from_bytes("Content-Type", "application/json").expect("static header")
    };
    let plain = |status: u16| tiny_http::Response::empty(status).boxed();
    let token = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .and_then(|header| header.value.as_str().strip_prefix("Bearer "))
        .map(str::to_string)
        .filter(|token| hub.runs.lock().unwrap().contains_key(token));
    // Read the body before answering anything: a refusal sent over an unread
    // body makes the socket reset under the client instead of delivering it.
    let mut body = String::new();
    let read = request.as_reader().take(BODY_MAX_BYTES).read_to_string(&mut body).is_ok();
    let response = match (token, request.method()) {
        (None, _) => plain(401),
        (Some(token), tiny_http::Method::Post) => {
            match read.then(|| serde_json::from_str::<Value>(&body).ok()).flatten() {
                None => plain(400),
                Some(message) => match hub.handle(&token, &message) {
                    // A notification or a response of the client's: accepted.
                    None => plain(202),
                    Some(reply) => tiny_http::Response::from_string(reply.to_string())
                        .with_header(json_header())
                        .boxed(),
                },
            }
        }
        // No server-initiated stream, no session to delete.
        (Some(_), _) => plain(405),
    };
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host that records what it was asked and answers like cua would.
    #[derive(Default)]
    struct FakeHost {
        calls: Mutex<Vec<String>>,
        ended: Mutex<Vec<String>>,
    }

    impl ToolHost for FakeHost {
        fn tools(&self) -> Result<Value, String> {
            Ok(json!([{ "name": "click", "inputSchema": { "type": "object" } }, { "name": "list_windows" }]))
        }

        fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
            self.calls.lock().unwrap().push(format!("{session_id}/{label} {name} {arguments}"));
            json!({ "content": [{ "type": "text", "text": format!("{name} done") }], "structuredContent": { "effect": "confirmed" } })
        }

        fn end(&self, session_id: &str) {
            self.ended.lock().unwrap().push(session_id.to_string());
        }
    }

    fn hub() -> (Hub, Arc<FakeHost>) {
        let host = Arc::new(FakeHost::default());
        (Hub::new(host.clone()), host)
    }

    fn call(hub: &Hub, token: &str, name: &str, arguments: Value) -> Value {
        hub.handle(
            token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
        .unwrap()
    }

    #[test]
    fn initialize_echoes_a_known_protocol_version_and_lists_cuas_tools() {
        let (hub, _) = hub();
        let token = hub.grant("s1", "EXP-1");
        let init = hub
            .handle(&token, &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }))
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "computer");
        assert!(init["result"]["instructions"].as_str().unwrap().contains("delivery_mode"));
        assert!(hub.handle(&token, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let tools = hub.handle(&token, &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["click", "list_windows"]);
        let unknown = hub.handle(&token, &json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn a_call_goes_to_the_runs_session_with_its_label_and_the_result_passes_through() {
        let (hub, host) = hub();
        let (one, two) = (hub.grant("s1", "EXP-1"), hub.grant("s2", "Chat"));
        let args = json!({ "target": { "kind": "window", "pid": 7, "window_id": 9 }, "x": 10, "y": 10, "delivery_mode": "background" });
        let reply = call(&hub, &one, "click", args.clone());
        assert_eq!(reply["result"]["structuredContent"]["effect"], "confirmed");
        call(&hub, &two, "list_windows", json!({}));
        assert_eq!(
            host.calls.lock().unwrap().as_slice(),
            [format!("s1/EXP-1 click {args}"), "s2/Chat list_windows {}".to_string()]
        );
    }

    #[test]
    fn a_new_grant_replaces_the_runs_old_token_and_a_revoke_ends_its_session() {
        let (hub, host) = hub();
        let old = hub.grant("s1", "EXP-1");
        let new = hub.grant("s1", "EXP-1");
        assert!(!hub.runs.lock().unwrap().contains_key(&old));
        assert!(hub.runs.lock().unwrap().contains_key(&new));
        let refused = call(&hub, &old, "click", json!({}));
        assert_eq!(refused["error"]["code"], -32000);
        hub.revoke("s1");
        assert!(hub.runs.lock().unwrap().is_empty());
        assert_eq!(host.ended.lock().unwrap().as_slice(), ["s1"]);
    }

    #[test]
    fn the_endpoint_wants_the_runs_bearer_token() {
        let (hub, _) = hub();
        let hub = Arc::new(hub);
        let token = hub.grant("s1", "EXP-1");
        let url = serve(hub).unwrap();
        let addr = url.trim_start_matches("http://").trim_end_matches("/mcp").to_string();
        let post = |auth: &str| {
            use std::io::Write;
            let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
            let mut stream = std::net::TcpStream::connect(&addr).unwrap();
            write!(
                stream,
                "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {auth}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(post("nope").starts_with("HTTP/1.1 401"));
        let ok = post(&token);
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        // Compared as JSON: the key order follows serde_json's features.
        let body: Value = serde_json::from_str(ok.rsplit("\r\n\r\n").next().unwrap()).unwrap();
        assert_eq!(body, json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    }
}
