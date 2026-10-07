//! EXP-1236: the host's own MCP client over streamable HTTP, blocking.
//! Exponential's `/api/mcp` (stateless, `enableJsonResponse`) and the
//! loopback servers answer a POST with plain JSON; a third-party server may
//! answer with an SSE body even for a POST, or hand out an `Mcp-Session-Id`,
//! so both are handled. Header VALUES here are the run's secrets: never
//! logged, never in an error text.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::Upstream;

/// The MCP protocol version this client speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    /// A non-2xx status.
    Http(u16),
    /// Could not connect, read or parse.
    Transport(String),
    /// The server answered a JSON-RPC error.
    Rpc { code: i64, message: String },
    Timeout,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Http(status) => write!(f, "HTTP {status}"),
            ClientError::Transport(text) => write!(f, "{text}"),
            ClientError::Rpc { code, message } => write!(f, "{message} (code {code})"),
            ClientError::Timeout => write!(f, "timed out"),
        }
    }
}

pub struct McpClient {
    http: reqwest::blocking::Client,
    upstream: Upstream,
    session: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl McpClient {
    pub fn new(http: reqwest::blocking::Client, upstream: Upstream) -> Self {
        Self { http, upstream, session: Mutex::new(None), next_id: AtomicU64::new(1) }
    }

    /// `initialize` + `notifications/initialized`.
    pub fn initialize(&self, timeout: Duration) -> Result<(), ClientError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "exponential-codemode", "version": env!("CARGO_PKG_VERSION") },
            }),
            timeout,
        )?;
        let _ = self.post(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), timeout);
        Ok(())
    }

    /// Every tool, following `nextCursor`.
    pub fn list_tools(&self, timeout: Duration) -> Result<Vec<Value>, ClientError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..32 {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let result = self.request("tools/list", params, timeout)?;
            if let Some(page) = result.get("tools").and_then(Value::as_array) {
                tools.extend(page.iter().cloned());
            }
            cursor = result.get("nextCursor").and_then(Value::as_str).map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }

    /// One `tools/call`; the result is the MCP call result as the server
    /// sent it (`content`, `structuredContent`, `isError`).
    pub fn call(&self, tool: &str, arguments: Value, timeout: Duration) -> Result<Value, ClientError> {
        self.request("tools/call", json!({ "name": tool, "arguments": arguments }), timeout)
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let response = self.post(message, timeout)?;
        if let Some(error) = response.get("error") {
            return Err(ClientError::Rpc {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: error.get("message").and_then(Value::as_str).unwrap_or("error").to_string(),
            });
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// POST one JSON-RPC message, answer its response message (`Null` for an
    /// accepted notification).
    fn post(&self, message: Value, timeout: Duration) -> Result<Value, ClientError> {
        let mut request = self
            .http
            .post(&self.upstream.url)
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (name, value) in &self.upstream.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if let Some(session) = self.session.lock().unwrap().clone() {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request.body(message.to_string()).send().map_err(|err| {
            if err.is_timeout() {
                ClientError::Timeout
            } else {
                // reqwest's Display may echo the URL; the URL carries no secret.
                ClientError::Transport(format!("{}: {err}", self.upstream.name))
            }
        })?;
        if let Some(session) = response.headers().get("mcp-session-id").and_then(|value| value.to_str().ok()) {
            *self.session.lock().unwrap() = Some(session.to_string());
        }
        let status = response.status();
        if status.as_u16() == 202 {
            return Ok(Value::Null);
        }
        if !status.is_success() {
            return Err(ClientError::Http(status.as_u16()));
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let body = response.text().map_err(|err| ClientError::Transport(format!("read: {err}")))?;
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        let wanted = message.get("id").cloned();
        if content_type.starts_with("text/event-stream") {
            return parse_sse(&body, wanted.as_ref());
        }
        serde_json::from_str::<Value>(&body).map_err(|err| ClientError::Transport(format!("parse: {err}")))
    }
}

/// The response message out of an SSE body: the `data:` payload whose `id`
/// matches, else the last message.
fn parse_sse(body: &str, wanted: Option<&Value>) -> Result<Value, ClientError> {
    let mut last = None;
    for event in body.split("\n\n") {
        let data: String = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            if wanted.is_some() && value.get("id") == wanted {
                return Ok(value);
            }
            last = Some(value);
        }
    }
    last.ok_or_else(|| ClientError::Transport("empty event stream".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A fake MCP server answering plain JSON (and SSE for `tools/list`),
    /// recording the headers it saw.
    fn serve(seen: Arc<Mutex<Vec<(String, String)>>>, sse: bool) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for mut request in server.incoming_requests() {
                for header in request.headers() {
                    seen.lock().unwrap().push((header.field.as_str().to_string().to_ascii_lowercase(), header.value.to_string()));
                }
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let message: Value = serde_json::from_str(&body).unwrap();
                let id = message.get("id").cloned();
                let Some(id) = id else {
                    let _ = request.respond(tiny_http::Response::empty(202));
                    continue;
                };
                let method = message["method"].as_str().unwrap_or_default();
                let reply = match method {
                    "initialize" => json!({ "jsonrpc": "2.0", "id": id, "result": { "protocolVersion": PROTOCOL_VERSION } }),
                    "tools/list" => json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": [{ "name": "ping", "description": "Pong." }] } }),
                    "tools/call" => {
                        let name = message["params"]["name"].as_str().unwrap_or_default();
                        if name == "boom" {
                            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": "bad args" } })
                        } else {
                            json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": "pong" }] } })
                        }
                    }
                    _ => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "nope" } }),
                };
                let response = if sse && method == "tools/list" {
                    let text = format!("event: message\ndata: {}\n\n", reply);
                    tiny_http::Response::from_string(text).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
                    )
                } else {
                    tiny_http::Response::from_string(reply.to_string())
                        .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap())
                        .with_header(tiny_http::Header::from_bytes("Mcp-Session-Id", "s-1").unwrap())
                };
                let _ = request.respond(response);
            }
        });
        format!("http://127.0.0.1:{port}/mcp")
    }

    fn make_client(url: String) -> McpClient {
        McpClient::new(
            reqwest::blocking::Client::new(),
            Upstream {
                name: "fake".into(),
                url,
                headers: vec![("Authorization".into(), "Bearer sekrit".into()), ("X-Exp-Session-Id".into(), "run-1".into())],
            },
        )
    }

    #[test]
    fn sends_headers_verbatim_echoes_the_session_and_maps_errors() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let client = make_client(serve(seen.clone(), false));
        let timeout = Duration::from_secs(5);
        client.initialize(timeout).unwrap();
        let tools = client.list_tools(timeout).unwrap();
        assert_eq!(tools[0]["name"], json!("ping"));
        let result = client.call("ping", json!({}), timeout).unwrap();
        assert_eq!(result["content"][0]["text"], json!("pong"));
        assert_eq!(
            client.call("boom", json!({}), timeout),
            Err(ClientError::Rpc { code: -32602, message: "bad args".into() })
        );
        let seen = seen.lock().unwrap();
        assert!(seen.contains(&("authorization".into(), "Bearer sekrit".into())));
        assert!(seen.contains(&("x-exp-session-id".into(), "run-1".into())));
        assert!(seen.contains(&("mcp-session-id".into(), "s-1".into())), "the session id is echoed after the first reply");
    }

    #[test]
    fn an_sse_body_is_parsed_to_the_matching_message() {
        let client = make_client(serve(Arc::new(Mutex::new(Vec::new())), true));
        let tools = client.list_tools(Duration::from_secs(5)).unwrap();
        assert_eq!(tools[0]["description"], json!("Pong."));
    }

    #[test]
    fn a_dead_port_is_a_transport_error_and_a_bad_status_is_http() {
        let client = make_client("http://127.0.0.1:9/mcp".into());
        assert!(matches!(client.initialize(Duration::from_secs(2)), Err(ClientError::Transport(_))));
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let _ = request.respond(tiny_http::Response::empty(500));
            }
        });
        let client = make_client(format!("http://127.0.0.1:{port}/mcp"));
        assert_eq!(client.initialize(Duration::from_secs(2)), Err(ClientError::Http(500)));
    }

    #[test]
    fn sse_parsing_prefers_the_wanted_id() {
        let body = "data: {\"id\":1,\"result\":\"a\"}\n\ndata: {\"id\":2,\"result\":\"b\"}\n\n";
        assert_eq!(parse_sse(body, Some(&json!(1))).unwrap()["result"], json!("a"));
        assert_eq!(parse_sse(body, Some(&json!(9))).unwrap()["result"], json!("b"));
        assert!(parse_sse("", None).is_err());
    }
}
