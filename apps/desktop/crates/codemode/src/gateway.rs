//! EXP-1458: the device MCP GATEWAY. A launched session's agent CLI reaches
//! Exponential's `/api/mcp` through this loopback route instead of the
//! instance directly: every JSON-RPC message is forwarded upstream with the
//! run's real credentials (the `expu_` key and `X-Exp-Session-Id` live in
//! the grant only; the agent holds a loopback token), and a `tools/call` is
//! handed to the run's [`GatewayTransform`] on the way up (seal the content
//! its arguments carry) and on the way back (open the sealed content its
//! result carries). The transform lives in `coding::mcp_gateway`; this
//! crate knows nothing about keys.
//!
//! Transport: streamable HTTP POST, answered as the upstream answered it
//! (plain JSON or an SSE body; `Mcp-Session-Id` passed both ways,
//! `MCP-Protocol-Version` and `Accept` passed up). A `GET` (the optional
//! server stream) is refused with 405, which the transport allows.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::client::McpClient;
use crate::Upstream;

/// The route prefix on the code-mode listener: a grant's base URL is
/// `http://127.0.0.1:<port>/gateway`, its endpoint `<base>/api/mcp`.
pub const ROUTE: &str = "/gateway";

/// One forwarded request's ceiling: past the longest legitimate upstream
/// call (`exponential_sessions get` waits for idle up to 600 s).
const FORWARD_TIMEOUT: Duration = Duration::from_secs(660);
/// The transform's own lookups (reading a card before re-sealing it).
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(30);

/// What a `tools/call` goes through on its way up and back.
pub trait GatewayTransform: Send + Sync {
    /// The arguments to send upstream instead of `args`. `Err` = refuse the
    /// call: the agent reads the text as the tool's error, nothing is sent.
    fn request(&self, tool: &str, args: Value) -> Result<Value, String>;
    /// The MCP call result the agent reads instead of `result` (as the
    /// upstream answered `args_sent`).
    fn result(&self, tool: &str, args_sent: &Value, result: Value) -> Value;
    /// [`Self::request`] with the run's upstream at hand, for a transform
    /// that must read something first (the card an answer re-seals). The
    /// gateway calls this one; the default ignores the upstream.
    fn request_with(&self, tool: &str, args: Value, _upstream: &dyn GatewayUpstream) -> Result<Value, String> {
        self.request(tool, args)
    }
}

/// The run's upstream MCP server, for a transform's own reads: one
/// `tools/call`, answered as the MCP call result.
pub trait GatewayUpstream {
    fn call(&self, tool: &str, args: Value) -> Result<Value, String>;
}

/// One admitted run.
pub(crate) struct GatewayRun {
    pub session_id: String,
    label: String,
    upstream: Upstream,
    transform: Arc<dyn GatewayTransform>,
    http: reqwest::blocking::Client,
    /// The transform's own client (lazily initialized once).
    lookups: Mutex<Option<Arc<McpClient>>>,
}

impl GatewayUpstream for GatewayRun {
    fn call(&self, tool: &str, args: Value) -> Result<Value, String> {
        let client = {
            let mut slot = self.lookups.lock().unwrap();
            match slot.as_ref() {
                Some(client) => client.clone(),
                None => {
                    let client = Arc::new(McpClient::new(self.http.clone(), self.upstream.clone()));
                    if let Err(err) = client.initialize(LOOKUP_TIMEOUT) {
                        return Err(format!("upstream initialize failed: {err}"));
                    }
                    *slot = Some(client.clone());
                    client
                }
            }
        };
        client.call(tool, args, LOOKUP_TIMEOUT).map_err(|err| format!("{tool}: {err}"))
    }
}

/// Token → run.
#[derive(Default)]
pub(crate) struct Gateways {
    runs: Mutex<HashMap<String, Arc<GatewayRun>>>,
}

impl Gateways {
    /// A fresh token for `session_id`; an earlier gateway token of the same
    /// run dies.
    pub fn grant(&self, session_id: &str, label: &str, upstream: Upstream, transform: Arc<dyn GatewayTransform>) -> String {
        let token = format!("gw{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        let run = GatewayRun {
            session_id: session_id.to_string(),
            label: label.to_string(),
            upstream,
            transform,
            http,
            lookups: Mutex::new(None),
        };
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|_, run| run.session_id != session_id);
        runs.insert(token.clone(), Arc::new(run));
        token
    }

    pub fn revoke(&self, session_id: &str) {
        self.runs.lock().unwrap().retain(|_, run| run.session_id != session_id);
    }

    pub fn run(&self, token: &str) -> Option<Arc<GatewayRun>> {
        self.runs.lock().unwrap().get(token).cloned()
    }

    #[cfg(test)]
    pub fn has_token(&self, token: &str) -> bool {
        self.runs.lock().unwrap().contains_key(token)
    }
}

/// What the gateway answers the agent.
pub(crate) struct Reply {
    pub status: u16,
    pub content_type: Option<String>,
    pub session: Option<String>,
    pub body: String,
}

impl Reply {
    fn empty(status: u16) -> Self {
        Reply { status, content_type: None, session: None, body: String::new() }
    }
}

/// The headers of the agent's request the gateway passes upstream.
#[derive(Default)]
pub(crate) struct Passed {
    pub session: Option<String>,
    pub protocol: Option<String>,
    pub accept: Option<String>,
}

/// A tools/call the gateway transformed: its JSON-RPC id, tool name and the
/// arguments actually sent (for [`GatewayTransform::result`]).
struct Pending {
    id: Value,
    tool: String,
    sent: Value,
}

fn rpc_error_result(id: &Value, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "content": [{ "type": "text", "text": text }], "isError": true },
    })
}

/// Transforms one message on the way up. `Ok(message)` = forward it,
/// `Err(reply)` = answer the agent at once (a refused call).
fn outgoing(run: &GatewayRun, message: Value, pending: &mut Vec<Pending>) -> Result<Value, Value> {
    if message.get("method").and_then(Value::as_str) != Some("tools/call") {
        return Ok(message);
    }
    let Some(id) = message.get("id").cloned() else { return Ok(message) };
    let tool = message.pointer("/params/name").and_then(Value::as_str).unwrap_or_default().to_string();
    let args = message.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
    match run.transform.request_with(&tool, args, run) {
        Ok(sent) => {
            let mut message = message;
            message["params"]["arguments"] = sent.clone();
            pending.push(Pending { id, tool, sent });
            Ok(message)
        }
        Err(text) => {
            log::info!("[gateway] {}: {tool} refused before upstream: {text}", run.label);
            Err(rpc_error_result(&id, &text))
        }
    }
}

/// Transforms one response message on the way back.
fn incoming(run: &GatewayRun, mut message: Value, pending: &[Pending]) -> Value {
    let Some(id) = message.get("id") else { return message };
    let Some(call) = pending.iter().find(|call| &call.id == id) else { return message };
    if let Some(result) = message.get_mut("result") {
        let taken = std::mem::take(result);
        *result = run.transform.result(&call.tool, &call.sent, taken);
    }
    message
}

/// One POST from the agent: forward, transform, answer.
pub(crate) fn handle(run: &GatewayRun, body: &str, passed: &Passed) -> Reply {
    let Ok(parsed) = serde_json::from_str::<Value>(body) else {
        return Reply::empty(400);
    };
    let batch = parsed.is_array();
    let messages: Vec<Value> = match parsed {
        Value::Array(items) => items,
        other => vec![other],
    };
    let mut pending = Vec::new();
    let mut forward = Vec::new();
    let mut answered = Vec::new();
    for message in messages {
        match outgoing(run, message, &mut pending) {
            Ok(message) => forward.push(message),
            Err(reply) => answered.push(reply),
        }
    }
    if forward.is_empty() {
        let body = if batch { Value::Array(answered).to_string() } else { answered.pop().unwrap_or(Value::Null).to_string() };
        return Reply { status: 200, content_type: Some("application/json".into()), session: passed.session.clone(), body };
    }
    let upstream_body = if batch { Value::Array(forward).to_string() } else { forward.pop().unwrap_or(Value::Null).to_string() };
    let mut request = run
        .http
        .post(&run.upstream.url)
        .timeout(FORWARD_TIMEOUT)
        .header("Content-Type", "application/json")
        .header("Accept", passed.accept.as_deref().unwrap_or("application/json, text/event-stream"));
    for (name, value) in &run.upstream.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    if let Some(session) = &passed.session {
        request = request.header("Mcp-Session-Id", session.as_str());
    }
    if let Some(protocol) = &passed.protocol {
        request = request.header("MCP-Protocol-Version", protocol.as_str());
    }
    let response = match request.body(upstream_body).send() {
        Ok(response) => response,
        Err(err) => {
            // The URL carries no secret; reqwest's Display never prints headers.
            log::warn!("[gateway] {}: upstream unreachable: {err}", run.label);
            return Reply::empty(if err.is_timeout() { 504 } else { 502 });
        }
    };
    let status = response.status().as_u16();
    let session = response.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()).map(str::to_string);
    let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).map(str::to_string);
    let text = response.text().unwrap_or_default();
    let sse = content_type.as_deref().is_some_and(|t| t.to_ascii_lowercase().starts_with("text/event-stream"));
    let body = if !(200..300).contains(&status) || text.trim().is_empty() || pending.is_empty() && answered.is_empty() {
        text
    } else if sse {
        transform_sse(run, &text, &pending, &mut answered)
    } else {
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Array(items)) => {
                let mut items: Vec<Value> = items.into_iter().map(|m| incoming(run, m, &pending)).collect();
                items.append(&mut answered);
                Value::Array(items).to_string()
            }
            Ok(message) if answered.is_empty() => incoming(run, message, &pending).to_string(),
            Ok(message) => {
                let mut items = vec![incoming(run, message, &pending)];
                items.append(&mut answered);
                Value::Array(items).to_string()
            }
            Err(_) => text,
        }
    };
    Reply { status, content_type, session, body }
}

/// An SSE body with every matching `data:` message transformed; refused
/// calls of a batch ride as extra events at the end.
fn transform_sse(run: &GatewayRun, body: &str, pending: &[Pending], answered: &mut Vec<Value>) -> String {
    let normalized = body.replace("\r\n", "\n");
    let mut out = String::new();
    for event in normalized.split("\n\n") {
        if event.trim().is_empty() {
            continue;
        }
        let data: String = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|line| line.strip_prefix(' ').unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        let transformed = serde_json::from_str::<Value>(&data).ok().map(|message| match message {
            Value::Array(items) => Value::Array(items.into_iter().map(|m| incoming(run, m, pending)).collect()),
            message => incoming(run, message, pending),
        });
        match transformed {
            Some(message) => {
                for line in event.lines().filter(|line| !line.starts_with("data:")) {
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str("data: ");
                out.push_str(&message.to_string());
                out.push_str("\n\n");
            }
            None => {
                out.push_str(event);
                out.push_str("\n\n");
            }
        }
    }
    for reply in answered.drain(..) {
        out.push_str("event: message\ndata: ");
        out.push_str(&reply.to_string());
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seals `text` as `sealed:<text>` and opens `sealed:` back; refuses
    /// the tool `nope`; reads `peek` upstream first.
    struct Fake;

    impl GatewayTransform for Fake {
        fn request(&self, tool: &str, mut args: Value) -> Result<Value, String> {
            if tool == "nope" {
                return Err("refused here".into());
            }
            if let Some(text) = args.get("text").and_then(Value::as_str) {
                args["text"] = json!(format!("sealed:{text}"));
            }
            Ok(args)
        }
        fn result(&self, _tool: &str, sent: &Value, mut result: Value) -> Value {
            result["sentWas"] = sent.clone();
            if let Some(text) = result.pointer("/content/0/text").and_then(Value::as_str).map(str::to_string) {
                result["content"][0]["text"] = json!(text.replace("sealed:", ""));
            }
            result
        }
        fn request_with(&self, tool: &str, args: Value, upstream: &dyn GatewayUpstream) -> Result<Value, String> {
            if tool == "peek" {
                let seen = upstream.call("echo", json!({ "text": "looked" }))?;
                return Ok(json!({ "peeked": seen["content"][0]["text"] }));
            }
            self.request(tool, args)
        }
    }

    /// A fake upstream echoing the call's `text` (SSE when asked for
    /// `sse`), recording every body and the auth header.
    fn upstream(sse: bool) -> (Upstream, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let auth = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string())
                    .unwrap_or_default();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                record.lock().unwrap().push((auth, body.clone()));
                let message: Value = serde_json::from_str(&body).unwrap();
                let Some(id) = message.get("id").cloned() else {
                    let _ = request.respond(tiny_http::Response::empty(202));
                    continue;
                };
                let reply = match message["method"].as_str().unwrap_or_default() {
                    "initialize" => json!({ "jsonrpc": "2.0", "id": id, "result": { "protocolVersion": "2025-06-18" } }),
                    _ => {
                        let args = &message["params"]["arguments"];
                        let text = args.get("text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| args.to_string());
                        json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": text }] } })
                    }
                };
                let response = if sse {
                    tiny_http::Response::from_string(format!("event: message\ndata: {reply}\n\n"))
                        .with_header(tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap())
                } else {
                    tiny_http::Response::from_string(reply.to_string())
                        .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap())
                        .with_header(tiny_http::Header::from_bytes("Mcp-Session-Id", "up-1").unwrap())
                };
                let _ = request.respond(response);
            }
        });
        (
            Upstream {
                name: "exponential".into(),
                url: format!("http://127.0.0.1:{port}/api/mcp"),
                headers: vec![("Authorization".into(), "Bearer expu_real".into()), ("X-Exp-Session-Id".into(), "s-1".into())],
            },
            seen,
        )
    }

    fn post(url: &str, token: &str, body: Value) -> (u16, Option<String>, String) {
        let response = reqwest::blocking::Client::new()
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(body.to_string())
            .send()
            .unwrap();
        let status = response.status().as_u16();
        let session = response.headers().get("mcp-session-id").map(|v| v.to_str().unwrap().to_string());
        (status, session, response.text().unwrap())
    }

    fn call(id: u64, tool: &str, args: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": tool, "arguments": args } })
    }

    #[test]
    fn forwards_with_the_grants_credentials_and_transforms_both_ways() {
        let (up, seen) = upstream(false);
        let grant = crate::gateway_grant("gw-run-1", "test", up, Arc::new(Fake)).unwrap();
        assert!(grant.url.ends_with(ROUTE), "{}", grant.url);
        let endpoint = format!("{}/api/mcp", grant.url);
        let (status, session, body) = post(&endpoint, &grant.token, call(1, "say", json!({ "text": "hi" })));
        assert_eq!(status, 200);
        assert_eq!(session.as_deref(), Some("up-1"));
        let reply: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(reply["result"]["content"][0]["text"], "hi", "opened on the way back");
        assert_eq!(reply["result"]["sentWas"]["text"], "sealed:hi");
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen[0].0, "Bearer expu_real", "the real key rides upstream, never the loopback token");
            assert!(seen[0].1.contains("sealed:hi") && !seen[0].1.contains("\"hi\""));
        }
        // A refused call never reaches the upstream.
        let before = seen.lock().unwrap().len();
        let (_, _, body) = post(&endpoint, &grant.token, call(2, "nope", json!({})));
        let reply: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(reply["result"]["isError"], true);
        assert_eq!(reply["result"]["content"][0]["text"], "refused here");
        assert_eq!(seen.lock().unwrap().len(), before);
        // Other methods pass through untouched.
        let (_, _, body) = post(&endpoint, &grant.token, json!({ "jsonrpc": "2.0", "id": 3, "method": "initialize", "params": {} }));
        assert!(body.contains("2025-06-18"));
        // The transform's own reads go through the same upstream.
        let (_, _, body) = post(&endpoint, &grant.token, call(4, "peek", json!({})));
        assert!(seen.lock().unwrap().iter().any(|(_, b)| b.contains("looked")), "{body}");
        // A wrong token is refused; a revoke kills the token.
        assert_eq!(post(&endpoint, "nope", call(5, "say", json!({ "text": "x" }))).0, 401);
        crate::revoke("gw-run-1");
        assert_eq!(post(&endpoint, &grant.token, call(6, "say", json!({ "text": "x" }))).0, 401);
    }

    #[test]
    fn an_sse_answer_is_transformed_in_place() {
        let (up, _) = upstream(true);
        let grant = crate::gateway_grant("gw-run-2", "test", up, Arc::new(Fake)).unwrap();
        let (status, _, body) = post(&format!("{}/api/mcp", grant.url), &grant.token, call(1, "say", json!({ "text": "yo" })));
        assert_eq!(status, 200);
        let data = body.lines().find_map(|l| l.strip_prefix("data: ")).unwrap();
        let reply: Value = serde_json::from_str(data).unwrap();
        assert_eq!(reply["result"]["content"][0]["text"], "yo");
        assert!(body.starts_with("event: message\n"));
        crate::revoke("gw-run-2");
    }

    #[test]
    fn a_new_grant_replaces_the_runs_old_gateway_token() {
        let gateways = Gateways::default();
        let (up, _) = upstream(false);
        let first = gateways.grant("s", "l", up.clone(), Arc::new(Fake));
        let second = gateways.grant("s", "l", up, Arc::new(Fake));
        assert!(!gateways.has_token(&first) && gateways.has_token(&second));
        gateways.revoke("s");
        assert!(!gateways.has_token(&second));
    }
}
