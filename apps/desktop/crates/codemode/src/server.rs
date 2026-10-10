//! EXP-1236: the code-mode MCP endpoint: streamable HTTP on loopback,
//! answered with plain JSON (a `GET` is refused, which the transport
//! allows). One listener per host process; each run reaches it with its
//! own bearer token ([`crate::grant`]), which names the run and, with it,
//! the upstream servers its scripts may call.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::catalog::Catalog;
use crate::client::{ClientError, McpClient};
use crate::runtime::{self, CallHost, ExecError, ExecOutcome, Limits};
use crate::Upstream;

/// The config key and server name the agents see (`mcp__codemode__exec`).
pub const SERVER_NAME: &str = "codemode";
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// A script plus its arguments; nothing legitimate is bigger.
const BODY_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// A script bigger than this is a mistake, not a program.
const SCRIPT_MAX_BYTES: usize = 256 * 1024;
/// One nested call's ceiling: past the longest legitimate upstream call
/// (`exponential_sessions_get` waits for idle up to 600 s, plus its 10 s
/// grace and a poll), so a hung upstream cannot pin the script forever and
/// a slow-but-honest one is not cut short.
const CALL_TIMEOUT: Duration = Duration::from_secs(660);
/// A catalog loaded while some upstream was unreachable is kept this long
/// before those servers are tried again: a dead team server must not add
/// its connect timeout to EVERY exec/describe.
const UNREACHABLE_TTL: Duration = Duration::from_secs(60);

/// One admitted run.
pub(crate) struct Run {
    pub session_id: String,
    /// The run's name, for logs.
    pub label: String,
    upstreams: Vec<Upstream>,
    direct_only: Vec<String>,
    http: reqwest::blocking::Client,
    /// The loaded catalog and when it expires (`None` = never: every
    /// upstream answered).
    catalog: Mutex<Option<(Arc<Catalog>, Option<Instant>)>>,
    clients: Mutex<HashMap<String, Arc<McpClient>>>,
    /// Set by a revoke: a script in flight ends at its next tick.
    cancelled: Arc<AtomicBool>,
}

impl Run {
    fn new(session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            session_id: session_id.to_string(),
            label: label.to_string(),
            upstreams,
            direct_only,
            http,
            catalog: Mutex::new(None),
            clients: Mutex::new(HashMap::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The run's tool catalog, loaded on first use. A load with every
    /// upstream answering is kept for the run; one with unreachable servers
    /// is kept for [`UNREACHABLE_TTL`], then they are tried again.
    fn catalog(&self) -> Arc<Catalog> {
        if let Some((catalog, expires)) = self.catalog.lock().unwrap().clone() {
            if expires.is_none_or(|at| Instant::now() < at) {
                return catalog;
            }
        }
        let loaded = Arc::new(Catalog::load(&self.upstreams, self.direct_only.clone(), &self.http));
        let expires = if loaded.unreachable.is_empty() {
            None
        } else {
            log::warn!(
                "[codemode] {}: unreachable upstreams: {}",
                self.label,
                loaded.unreachable.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(", ")
            );
            Some(Instant::now() + UNREACHABLE_TTL)
        };
        *self.catalog.lock().unwrap() = Some((loaded.clone(), expires));
        loaded
    }

    fn client(&self, server: &str) -> Option<Arc<McpClient>> {
        if let Some(client) = self.clients.lock().unwrap().get(server) {
            return Some(client.clone());
        }
        let upstream = self.upstreams.iter().find(|upstream| upstream.name == server)?.clone();
        let client = Arc::new(McpClient::new(self.http.clone(), upstream));
        // The catalog load initialized its own client; this one is for the
        // calls and does its own handshake once.
        if let Err(err) = client.initialize(Duration::from_secs(20)) {
            log::warn!("[codemode] {}: initialize {server} failed: {err}", self.label);
        }
        self.clients.lock().unwrap().insert(server.to_string(), client.clone());
        Some(client)
    }
}

impl CallHost for Run {
    fn call(&self, server: &str, tool: &str, args: Value) -> Result<Value, String> {
        if self.direct_only.iter().any(|name| name == server) {
            return Err(format!("{server} is a stdio server: call its tools directly, not from a script"));
        }
        let client = self
            .client(server)
            .ok_or_else(|| format!("tool_unavailable: no server `{server}` in this run (see ALL_TOOLS)"))?;
        match client.call(tool, args, CALL_TIMEOUT) {
            Ok(result) => Ok(result),
            Err(ClientError::Timeout) => Err(format!("{server}.{tool} did not answer within {} s", CALL_TIMEOUT.as_secs())),
            Err(err) => Err(format!("{server}.{tool} failed: {err}")),
        }
    }
}

pub(crate) struct Hub {
    /// Token → run.
    runs: Mutex<HashMap<String, Arc<Run>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self { runs: Mutex::default() }
    }

    /// A fresh token for `session_id`; an earlier one of the same run dies.
    pub fn grant(&self, session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) -> String {
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|_, run| {
            let stale = run.session_id == session_id;
            if stale {
                run.cancelled.store(true, Ordering::Release);
            }
            !stale
        });
        runs.insert(token.clone(), Arc::new(Run::new(session_id, label, upstreams, direct_only)));
        token
    }

    pub fn revoke(&self, session_id: &str) {
        self.runs.lock().unwrap().retain(|_, run| {
            let gone = run.session_id == session_id;
            if gone {
                run.cancelled.store(true, Ordering::Release);
            }
            !gone
        });
    }

    #[cfg(test)]
    pub fn has_token(&self, token: &str) -> bool {
        self.runs.lock().unwrap().contains_key(token)
    }

    fn run(&self, token: &str) -> Option<Arc<Run>> {
        self.runs.lock().unwrap().get(token).cloned()
    }

    /// One JSON-RPC message → its response (`None` for a notification).
    pub fn handle(&self, token: &str, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version = PROTOCOL_VERSIONS.iter().find(|version| **version == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match self.run(token) {
                    Some(run) => Ok(call_tool(&run, name, &arguments)),
                    None => Err(json!({ "code": -32000, "message": "This run's code-mode grant ended." })),
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

/// `exec` or `describe`, answered as an MCP call result (a refusal is
/// content the agent must read, never a JSON-RPC error).
fn call_tool(run: &Arc<Run>, name: &str, arguments: &Value) -> Value {
    match name {
        "exec" => exec(run, arguments),
        "describe" => {
            let names: Vec<String> = arguments
                .get("names")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            let answer = run.catalog().describe(&names);
            json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&answer).unwrap_or_default() }],
                "structuredContent": answer,
            })
        }
        other => error_result(format!("Unknown tool `{other}`; this server has `exec` and `describe`.")),
    }
}

fn exec(run: &Arc<Run>, arguments: &Value) -> Value {
    let Some(script) = arguments.get("script").and_then(Value::as_str) else {
        return error_result("`script` (a string) is required.".to_string());
    };
    if script.len() > SCRIPT_MAX_BYTES {
        return error_result(format!("The script is {} bytes; the cap is {SCRIPT_MAX_BYTES}.", script.len()));
    }
    let timeout = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(runtime::DEFAULT_TIMEOUT)
        .clamp(runtime::MIN_TIMEOUT, runtime::MAX_TIMEOUT);
    let catalog = run.catalog();
    let host: Arc<dyn CallHost> = run.clone();
    let outcome = runtime::run_script(script, catalog, host, run.cancelled.clone(), Limits { timeout, ..Limits::default() });
    log::info!(
        "[codemode] {}: exec {} in {:?}, {} calls ({} failed, {} threads){}",
        run.label,
        if outcome.ok() { "ok" } else { "failed" },
        outcome.elapsed,
        outcome.calls.count,
        outcome.calls.failed,
        outcome.workers,
        if outcome.truncated { ", truncated" } else { "" }
    );
    outcome_result(&outcome)
}

/// The MCP shape of an outcome: `structuredContent` for a client that reads
/// it, the same JSON as text for one that does not; `isError` only when the
/// script failed.
pub(crate) fn outcome_result(outcome: &ExecOutcome) -> Value {
    let error = outcome.error.as_ref().map(|error| match error {
        ExecError::Script(text) => text.clone(),
        ExecError::Timeout => format!("timed out after {:?}; split the work or raise timeout_ms", outcome.elapsed),
        ExecError::Cancelled => "the run ended while the script ran".to_string(),
    });
    let structured = json!({
        "ok": outcome.ok(),
        "result": outcome.result,
        "console": outcome.console,
        "error": error,
        "calls": { "count": outcome.calls.count, "failed": outcome.calls.failed },
        "elapsedMs": outcome.elapsed.as_millis() as u64,
        "truncated": outcome.truncated,
    });
    let mut result = json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&structured).unwrap_or_default() }],
        "structuredContent": structured,
    });
    if !outcome.ok() {
        result["isError"] = json!(true);
    }
    result
}

fn error_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// The two tools, as the agent reads them.
fn tool_definitions() -> Value {
    json!([
        {
            "name": "exec",
            "description": EXEC_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "JavaScript, the body of an async function: top-level await and return work."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": runtime::MIN_TIMEOUT.as_millis() as u64,
                        "maximum": runtime::MAX_TIMEOUT.as_millis() as u64,
                        "default": runtime::DEFAULT_TIMEOUT.as_millis() as u64,
                        "description": "Wall clock for the whole script."
                    }
                },
                "required": ["script"]
            }
        },
        {
            "name": "describe",
            "description": DESCRIBE_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "names": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tools as `mcp__<server>__<tool>`, `<server>.<tool>` or a bare tool name when only one server has it. Omit for every tool's name and one-line summary."
                    }
                }
            }
        }
    ])
}

/// What the agent reads at `initialize`.
const INSTRUCTIONS: &str = "Code mode. `exec` runs JavaScript that calls this run's OTHER MCP tools as \
`await tools.<server>.<tool>(args)` (or `tools.mcp__<server>__<tool>`), in parallel with `Promise.all`; \
only the script's console output and return value come back, so a hundred intermediate results stay out \
of your context. `describe` lists the tools with their input schemas. Use a script when one step is many \
calls or several windows at once; a single call stays a direct tool call.";

const EXEC_DESCRIPTION: &str = "Run JavaScript that calls this run's MCP tools in bulk and in parallel; only \
the script's console output and return value come back (intermediate results stay out of your context). \
Every tool is `await tools.<server>.<tool>(args)` or `await tools.mcp__<server>__<tool>(args)`, e.g. \
`tools.computer.list_windows({})`, `tools.exponential.exponential_issues_list({boardId})`. \
`Promise.all([...])` runs calls at once (16 in flight); `sleep(ms)` waits. A call answers its \
structuredContent, else its text parsed as JSON, else the text; a tool refusal (isError) REJECTS with its \
text (Promise.allSettled tolerates it); `tools.<s>.<t>.raw(args)` returns the whole MCP result instead, \
images replaced by `[image omitted; N bytes]`. The script is the body of an async function: top-level \
`await` and `return` work. Back comes: console lines + the returned value as JSON (64 KB cap). \
`ALL_TOOLS` is the name list with one-line summaries; `describe` has the schemas. No fs, network, imports \
or timers besides sleep. timeout_ms ends the script at any await or call; a synchronous loop that never \
awaits is bounded only by a 10M-iterations-per-function ceiling, so keep loops short and await inside \
them. One call = a direct tool call, not a script.";

const DESCRIBE_DESCRIPTION: &str = "Input schemas of this run's MCP tools for use inside `exec`. `names` as \
`mcp__<server>__<tool>`, `<server>.<tool>` or a bare tool name when unambiguous; omit it for every tool's \
name and one-line summary (what `ALL_TOOLS` shows in a script). Stdio team servers are direct-call only \
and listed as such.";

/// Bind loopback and serve `hub` on a background thread; returns the URL.
pub(crate) fn serve(hub: Arc<Hub>) -> Result<String, String> {
    let server = tiny_http::Server::http("127.0.0.1:0").map_err(|err| format!("Could not start the code-mode server: {err}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or("The code-mode server has no TCP address.")?;
    std::thread::Builder::new()
        .name("codemode-mcp".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let hub = hub.clone();
                // An exec runs for minutes: never hold the accept loop for it.
                let _ = std::thread::Builder::new().name("codemode-call".into()).spawn(move || respond(&hub, request));
            }
        })
        .map_err(|err| format!("Could not start the code-mode server: {err}"))?;
    Ok(format!("http://127.0.0.1:{port}/mcp"))
}

fn respond(hub: &Hub, mut request: tiny_http::Request) {
    let json_header = || tiny_http::Header::from_bytes("Content-Type", "application/json").expect("static header");
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
        (Some(token), tiny_http::Method::Post) => match read.then(|| serde_json::from_str::<Value>(&body).ok()).flatten() {
            None => plain(400),
            Some(message) => match hub.handle(&token, &message) {
                None => plain(202),
                Some(reply) => tiny_http::Response::from_string(reply.to_string()).with_header(json_header()).boxed(),
            },
        },
        (Some(_), _) => plain(405),
    };
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake upstream MCP server with two tools; `slow` sleeps.
    fn upstream(name: &str) -> (Upstream, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let record = seen.clone();
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
                let message: Value = serde_json::from_str(&body).unwrap();
                let Some(id) = message.get("id").cloned() else {
                    let _ = request.respond(tiny_http::Response::empty(202));
                    continue;
                };
                let reply = match message["method"].as_str().unwrap_or_default() {
                    "initialize" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
                    "tools/list" => {
                        record.lock().unwrap().push("tools/list".to_string());
                        json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": [
                        { "name": "list_windows", "description": "List windows.", "inputSchema": { "type": "object" } },
                        { "name": "slow", "description": "Sleep ms.", "inputSchema": { "type": "object" } },
                    ] } })
                    }
                    "tools/call" => {
                        let name = message["params"]["name"].as_str().unwrap_or_default().to_string();
                        record.lock().unwrap().push(format!("{auth} {name}"));
                        if name == "slow" {
                            std::thread::sleep(Duration::from_millis(300));
                        }
                        json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": "[{\"title\":\"A\"},{\"title\":\"B\"}]" }] } })
                    }
                    _ => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "nope" } }),
                };
                let _ = request.respond(
                    tiny_http::Response::from_string(reply.to_string())
                        .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap()),
                );
            }
        });
        (
            Upstream {
                name: name.into(),
                url: format!("http://127.0.0.1:{port}/mcp"),
                headers: vec![("Authorization".into(), format!("Bearer {name}-secret"))],
            },
            seen,
        )
    }

    fn call(hub: &Hub, token: &str, name: &str, arguments: Value) -> Value {
        hub.handle(
            token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
        .unwrap()
    }

    #[test]
    fn initialize_lists_exec_and_describe() {
        let hub = Hub::new();
        let token = hub.grant("s1", "EXP-1", Vec::new(), Vec::new());
        let init = hub
            .handle(&token, &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }))
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "codemode");
        assert!(init["result"]["instructions"].as_str().unwrap().contains("Promise.all"));
        assert!(hub.handle(&token, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let tools = hub.handle(&token, &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["exec", "describe"]);
        assert_eq!(tools["result"]["tools"][0]["inputSchema"]["required"], json!(["script"]));
        let unknown = hub.handle(&token, &json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn exec_calls_the_runs_upstreams_with_their_headers_in_parallel_and_describe_has_the_schemas() {
        let (computer, seen) = upstream("computer");
        let hub = Hub::new();
        let token = hub.grant("s1", "EXP-1", vec![computer], vec!["playwright".into()]);
        let started = std::time::Instant::now();
        let reply = call(
            &hub,
            &token,
            "exec",
            json!({ "script": "const [a, b, w] = await Promise.all([tools.computer.slow({}), tools.computer.slow({}), tools.computer.list_windows({})]); console.log('n', w.length); return w.map((x) => x.title);" }),
        );
        let result = &reply["result"];
        assert!(result.get("isError").is_none(), "{reply}");
        assert_eq!(result["structuredContent"]["ok"], json!(true));
        assert_eq!(result["structuredContent"]["result"], json!(["A", "B"]));
        assert_eq!(result["structuredContent"]["console"], json!("n 2\n"));
        assert_eq!(result["structuredContent"]["calls"]["count"], json!(3));
        assert!(started.elapsed() < Duration::from_millis(1000), "two 300 ms calls overlapped: {:?}", started.elapsed());
        let seen = seen.lock().unwrap();
        assert!(seen.iter().filter(|line| *line != "tools/list").all(|line| line.starts_with("Bearer computer-secret ")), "{seen:?}");

        let described = call(&hub, &token, "describe", json!({ "names": ["computer.slow", "ghost"] }));
        assert_eq!(described["result"]["structuredContent"]["tools"][0]["name"], json!("mcp__computer__slow"));
        assert_eq!(described["result"]["structuredContent"]["unknown"], json!(["ghost"]));
        assert_eq!(described["result"]["structuredContent"]["directOnly"], json!(["playwright"]));

        let failed = call(&hub, &token, "exec", json!({ "script": "await tools.playwright.click({});" }));
        assert_eq!(failed["result"]["isError"], json!(true));
        assert!(failed["result"]["structuredContent"]["error"].as_str().unwrap().contains("direct-call") || failed["result"]["structuredContent"]["error"].as_str().unwrap().contains("TypeError"), "{failed}");
    }

    /// F32: a dead upstream does not make every exec/describe reload the
    /// catalog (and wait for it); the partial catalog is kept for a while,
    /// then the dead server is tried again.
    #[test]
    fn a_partial_catalog_is_cached_for_a_while_and_the_dead_upstream_retried_later() {
        let (computer, seen) = upstream("computer");
        let dead = Upstream { name: "linear".into(), url: "http://127.0.0.1:1/mcp".into(), headers: Vec::new() };
        let hub = Hub::new();
        let token = hub.grant("s1", "EXP-1", vec![computer, dead], Vec::new());
        let lists = || seen.lock().unwrap().iter().filter(|line| *line == "tools/list").count();
        let first = call(&hub, &token, "describe", json!({}));
        let all = first["result"]["structuredContent"].to_string();
        assert!(all.contains("mcp__computer__slow") && all.contains("linear"), "{all}");
        assert_eq!(lists(), 1);
        call(&hub, &token, "describe", json!({}));
        call(&hub, &token, "exec", json!({ "script": "return ALL_TOOLS.includes('unreachable: linear');" }));
        assert_eq!(lists(), 1, "the partial catalog was reloaded");
        let run = hub.run(&token).unwrap();
        let (catalog, expires) = run.catalog.lock().unwrap().clone().unwrap();
        assert_eq!(catalog.unreachable.len(), 1);
        assert!(expires.is_some(), "a partial catalog expires");
        *run.catalog.lock().unwrap() = Some((catalog, Some(Instant::now() - Duration::from_secs(1))));
        call(&hub, &token, "describe", json!({}));
        assert_eq!(lists(), 2, "an expired partial catalog is reloaded");
    }

    #[test]
    fn exec_refuses_a_missing_or_huge_script_and_unknown_tools() {
        let hub = Hub::new();
        let token = hub.grant("s1", "EXP-1", Vec::new(), Vec::new());
        assert_eq!(call(&hub, &token, "exec", json!({}))["result"]["isError"], json!(true));
        let huge = "x".repeat(SCRIPT_MAX_BYTES + 1);
        assert_eq!(call(&hub, &token, "exec", json!({ "script": huge }))["result"]["isError"], json!(true));
        assert_eq!(call(&hub, &token, "nope", json!({}))["result"]["isError"], json!(true));
    }

    #[test]
    fn a_new_grant_replaces_the_runs_old_token_and_a_revoke_cancels_it() {
        let hub = Hub::new();
        let old = hub.grant("s1", "EXP-1", Vec::new(), Vec::new());
        let new = hub.grant("s1", "EXP-1", Vec::new(), Vec::new());
        assert!(!hub.has_token(&old));
        assert!(hub.has_token(&new));
        let refused = call(&hub, &old, "exec", json!({ "script": "return 1" }));
        assert_eq!(refused["error"]["code"], -32000);
        let run = hub.run(&new).unwrap();
        hub.revoke("s1");
        assert!(hub.runs.lock().unwrap().is_empty());
        assert!(run.cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn the_endpoint_wants_the_runs_bearer_token() {
        let hub = Arc::new(Hub::new());
        let token = hub.grant("s1", "EXP-1", Vec::new(), Vec::new());
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
        let body: Value = serde_json::from_str(ok.rsplit("\r\n\r\n").next().unwrap()).unwrap();
        assert_eq!(body, json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    }
}
