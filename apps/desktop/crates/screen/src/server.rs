//! The `screen` MCP endpoint, the computer crate's topology: streamable
//! HTTP on loopback answered with plain JSON, one listener per process,
//! each run admitted by its own bearer token. The one tool is
//! [`crate::tool`]'s.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const BODY_MAX_BYTES: u64 = 1024 * 1024;

/// Answers a `tools/call` (name, arguments) as an MCP call result.
pub(crate) type Call = Arc<dyn Fn(&str, &Value) -> Value + Send + Sync>;

pub(crate) struct Hub {
    /// Token → session id.
    runs: Mutex<HashMap<String, String>>,
    call: Call,
}

impl Hub {
    pub(crate) fn new(call: Call) -> Hub {
        Hub { runs: Mutex::default(), call }
    }

    /// A fresh token for `session_id`; an earlier one of the same run dies.
    pub(crate) fn grant(&self, session_id: &str) -> String {
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|_, run| run != session_id);
        runs.insert(token.clone(), session_id.to_string());
        token
    }

    pub(crate) fn revoke(&self, session_id: &str) {
        self.runs.lock().unwrap().retain(|_, run| run != session_id);
    }

    fn admitted(&self, token: &str) -> bool {
        self.runs.lock().unwrap().contains_key(token)
    }

    /// One JSON-RPC message → its response (`None` for a notification).
    pub(crate) fn handle(&self, token: &str, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version =
                    PROTOCOL_VERSIONS.iter().find(|version| **version == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": crate::SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": crate::tool::tools() })),
            "tools/call" => {
                if self.admitted(token) {
                    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                    Ok((self.call)(name, &arguments))
                } else {
                    Err(json!({ "code": -32000, "message": "This session's screen-memory grant ended." }))
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

/// Bind loopback and serve `hub` on a background thread; returns the URL.
pub(crate) fn serve(hub: Arc<Hub>) -> Result<String, String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|err| format!("Could not start the screen-memory server: {err}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or("The screen-memory server has no TCP address.")?;
    std::thread::Builder::new()
        .name("screen-memory-mcp".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let hub = hub.clone();
                // Decoding and fitting four frames takes a moment: never
                // hold the accept loop for it.
                let _ = std::thread::Builder::new().name("screen-memory-call".into()).spawn(move || respond(&hub, request));
            }
        })
        .map_err(|err| format!("Could not start the screen-memory server: {err}"))?;
    Ok(format!("http://127.0.0.1:{port}/mcp"))
}

fn respond(hub: &Hub, mut request: tiny_http::Request) {
    let plain = |status: u16| tiny_http::Response::empty(status).boxed();
    let token = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .and_then(|header| header.value.as_str().strip_prefix("Bearer "))
        .map(str::to_string)
        .filter(|token| hub.admitted(token));
    // Read the body before answering: a refusal over an unread body resets
    // the socket under the client.
    let mut body = String::new();
    let read = request.as_reader().take(BODY_MAX_BYTES).read_to_string(&mut body).is_ok();
    let response = match (token, request.method()) {
        (None, _) => plain(401),
        (Some(token), tiny_http::Method::Post) => match read.then(|| serde_json::from_str::<Value>(&body).ok()).flatten() {
            None => plain(400),
            Some(message) => match hub.handle(&token, &message) {
                None => plain(202),
                Some(reply) => tiny_http::Response::from_string(reply.to_string())
                    .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").expect("static header"))
                    .boxed(),
            },
        },
        (Some(_), _) => plain(405),
    };
    let _ = request.respond(response);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// POST one JSON-RPC message to `url` with `token`; (status line, body).
    pub(crate) fn post(url: &str, token: &str, body: &Value) -> (String, String) {
        use std::io::Write;
        let addr = url.trim_start_matches("http://").trim_end_matches("/mcp").to_string();
        let body = body.to_string();
        let mut stream = std::net::TcpStream::connect(&addr).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        let (head, body) = reply.split_once("\r\n\r\n").unwrap_or((&reply, ""));
        (head.lines().next().unwrap_or_default().to_string(), body.to_string())
    }

    #[test]
    fn the_endpoint_lists_the_one_tool_and_wants_the_runs_token() {
        let hub = Arc::new(Hub::new(Arc::new(|name: &str, _: &Value| json!({ "content": [{ "type": "text", "text": name }] }))));
        let old = hub.grant("s1");
        let token = hub.grant("s1");
        let url = serve(hub.clone()).unwrap();
        let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
        assert!(post(&url, "nope", &ping).0.starts_with("HTTP/1.1 401"));
        assert!(post(&url, &old, &ping).0.starts_with("HTTP/1.1 401"));
        let (status, body) = post(&url, &token, &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }));
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        let init: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "screen");
        let (_, body) = post(&url, &token, &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
        let list: Value = serde_json::from_str(&body).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["get_screenshots"]);
        hub.revoke("s1");
        assert!(post(&url, &token, &ping).0.starts_with("HTTP/1.1 401"));
    }
}
