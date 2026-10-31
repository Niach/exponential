//! EXP-1236: the code-mode MCP endpoint, served by the `emcp` crate:
//! Streamable HTTP on loopback, one listener per host process; each run
//! reaches it with its own bearer token ([`crate::codemode::grant`]), which names the
//! run and, with it, the upstream servers its scripts may call. The gateway
//! (`/gateway/*`, EXP-1458) shares the listener as a raw route.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Client, ClientError};
use serde_json::{json, Value};

use crate::codemode::catalog::Catalog;
use crate::codemode::runtime::{self, CallHost, ExecError, ExecOutcome, Limits};
use crate::codemode::Upstream;

/// The config key and server name the agents see (`mcp__codemode__exec`).
pub const SERVER_NAME: &str = "codemode";
/// The `clientInfo.name` of the host's upstream calls.
pub(crate) const CLIENT_NAME: &str = "exponential-codemode";
/// A script bigger than this is a mistake, not a program.
const SCRIPT_MAX_BYTES: usize = 256 * 1024;
/// EXP-1279: where an exec's call log rides in its result's `_meta`
/// (`script-card.json` `metaKey`/`metaCountKey`/`metaElapsedKey`): the
/// `[{tool, subject, ok, ms}]` log (at most [`crate::codemode::LOG_CAP`]), every call
/// the script made, and its wall time. Never in the text content: the model
/// would pay tokens for it.
pub const META_CALLS_KEY: &str = "exponential/calls";
pub const META_COUNT_KEY: &str = "exponential/callCount";
pub const META_ELAPSED_KEY: &str = "exponential/elapsedMs";
/// One nested call's ceiling: past the longest legitimate upstream call
/// (`exponential_sessions_get` waits for idle up to 600 s, plus its 10 s
/// grace and a poll), so a hung upstream cannot pin the script forever and
/// a slow-but-honest one is not cut short.
const CALL_TIMEOUT: Duration = Duration::from_secs(660);
/// The handshake with an upstream before its first call.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
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
    /// The loaded catalog and when it expires (`None` = never: every
    /// upstream answered).
    catalog: Mutex<Option<(Arc<Catalog>, Option<Instant>)>>,
    clients: Mutex<HashMap<String, Client>>,
    /// Set by a revoke: a script in flight ends at its next tick.
    cancelled: Arc<AtomicBool>,
}

impl Run {
    fn new(session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) -> Self {
        Self {
            session_id: session_id.to_string(),
            label: label.to_string(),
            upstreams,
            direct_only,
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
        let loaded = Arc::new(Catalog::load(&self.upstreams, self.direct_only.clone()));
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

    /// The run's connected client for `server` (one handshake per server
    /// per run; a failed one is tried again on the next call).
    fn client(&self, server: &str) -> Result<Client, String> {
        if let Some(client) = self.clients.lock().unwrap().get(server) {
            return Ok(client.clone());
        }
        let upstream = self
            .upstreams
            .iter()
            .find(|upstream| upstream.name == server)
            .ok_or_else(|| format!("tool_unavailable: no server `{server}` in this session (see ALL_TOOLS)"))?;
        let client = Client::connect(upstream, CLIENT_NAME, CONNECT_TIMEOUT).map_err(|err| {
            log::warn!("[codemode] {}: initialize {server} failed: {err}", self.label);
            format!("{server} is unreachable: {err}")
        })?;
        self.clients.lock().unwrap().insert(server.to_string(), client.clone());
        Ok(client)
    }
}

impl CallHost for Run {
    fn call(&self, server: &str, tool: &str, args: Value) -> Result<Value, String> {
        if self.direct_only.iter().any(|name| name == server) {
            return Err(format!("{server} is a stdio server: call its tools directly, not from a script"));
        }
        let client = self.client(server)?;
        match client.call(tool, args, CALL_TIMEOUT) {
            Ok(result) => Ok(result),
            Err(ClientError::Timeout) => Err(format!("{server}.{tool} did not answer within {} s", CALL_TIMEOUT.as_secs())),
            Err(err) => Err(format!("{server}.{tool} failed: {err}")),
        }
    }
}

/// The admitted runs, by session id; the tokens are the emcp server's.
pub(crate) struct Hub {
    runs: Mutex<HashMap<String, Arc<Run>>>,
    /// EXP-1458: the MCP gateway's runs (their own tokens, `/gateway/*`).
    pub(crate) gateways: crate::codemode::gateway::Gateways,
}

impl Hub {
    pub fn new() -> Self {
        Self { runs: Mutex::default(), gateways: crate::codemode::gateway::Gateways::default() }
    }

    /// Admit `session_id`; a run it held before is cancelled. The caller
    /// mints the run's token on the server.
    pub fn admit(&self, session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) {
        let run = Arc::new(Run::new(session_id, label, upstreams, direct_only));
        if let Some(old) = self.runs.lock().unwrap().insert(session_id.to_string(), run) {
            old.cancelled.store(true, Ordering::Release);
        }
    }

    /// The run ended: a script in flight ends at its next tick.
    pub fn end(&self, session_id: &str) {
        if let Some(run) = self.runs.lock().unwrap().remove(session_id) {
            run.cancelled.store(true, Ordering::Release);
        }
    }

    fn run(&self, session_id: &str) -> Option<Arc<Run>> {
        self.runs.lock().unwrap().get(session_id).cloned()
    }
}

impl crate::ToolProvider for Hub {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(tool_definitions().as_array().cloned().unwrap_or_default())
    }

    fn call(&self, caller: &crate::Caller, name: &str, arguments: Value) -> Value {
        match self.run(caller.session_id.as_str()) {
            Some(run) => call_tool(&run, name, &arguments),
            None => error_result("This session's code-mode grant ended.".to_string()),
        }
    }

    fn ended(&self, session_id: &crate::SessionId) {
        self.gateways.revoke(session_id.as_str());
        self.end(session_id.as_str());
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
    crate::codemode::reports::publish(
        &run.session_id,
        script,
        crate::codemode::ScriptReport {
            count: outcome.calls.count,
            elapsed_ms: outcome.elapsed.as_millis() as u64,
            calls: outcome.calls.log.clone(),
        },
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
        ExecError::Cancelled => "the session ended while the script ran".to_string(),
        ExecError::OutOfMemory { ceiling_bytes } => format!(
            "the script held more than {} MB of memory and was stopped; keep less in memory (summarize as you go, return less)",
            ceiling_bytes / (1024 * 1024)
        ),
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
    let calls: Vec<Value> = outcome
        .calls
        .log
        .iter()
        .map(|call| json!({ "tool": call.tool, "subject": call.subject, "ok": call.ok, "ms": call.ms }))
        .collect();
    let mut result = json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&structured).unwrap_or_default() }],
        "structuredContent": structured,
        "_meta": {
            META_CALLS_KEY: calls,
            META_COUNT_KEY: outcome.calls.count,
            META_ELAPSED_KEY: outcome.elapsed.as_millis() as u64,
        },
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
const INSTRUCTIONS: &str = "Code mode. `exec` runs JavaScript that calls this session's OTHER MCP tools as \
`await tools.<server>.<tool>(args)` (or `tools.mcp__<server>__<tool>`), in parallel with `Promise.all`; \
only the script's console output and return value come back, so a hundred intermediate results stay out \
of your context. `describe` lists the tools with their input schemas. Use a script when one step is many \
calls or several windows at once; a single call stays a direct tool call.";

const EXEC_DESCRIPTION: &str = "Run JavaScript that calls this session's MCP tools in bulk and in parallel; only \
the script's console output and return value come back (intermediate results stay out of your context). \
Every tool is `await tools.<server>.<tool>(args)` or `await tools.mcp__<server>__<tool>(args)`, e.g. \
`tools.computer.list_windows({})`, `tools.exponential.exponential_issues_list({boardId})`. \
`Promise.all([...])` runs calls at once (16 in flight); `sleep(ms)` waits. A call answers its \
structuredContent, else its text parsed as JSON, else the text; a tool refusal (isError) REJECTS with its \
text (Promise.allSettled tolerates it); `tools.<s>.<t>.raw(args)` returns the whole MCP result instead, \
images replaced by `[image omitted; N bytes]`. The script is the body of an async function: top-level \
`await` and `return` work. Back comes: console lines + the returned value as JSON (64 KB cap). \
`ALL_TOOLS` is the name list with one-line summaries; `describe` has the schemas. No fs, network, imports \
or timers besides sleep. The script runs in its own process: timeout_ms ends it (a loop that never awaits \
included) and so does holding more than 768 MB of memory, so keep loops short and summarize as you go. \
One call = a direct tool call, not a script.";

const DESCRIBE_DESCRIPTION: &str = "Input schemas of this session's MCP tools for use inside `exec`. `names` as \
`mcp__<server>__<tool>`, `<server>.<tool>` or a bare tool name when unambiguous; omit it for every tool's \
name and one-line summary (what `ALL_TOOLS` shows in a script). Stdio team servers are direct-call only \
and listed as such.";

/// Bind loopback and serve `hub` ([`INSTRUCTIONS`] at `initialize`), the
/// gateway on `/gateway/*`.
pub(crate) fn serve(hub: Arc<Hub>) -> Result<crate::Server, String> {
    let info = crate::ServerInfo {
        name: SERVER_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        instructions: Some(INSTRUCTIONS.into()),
    };
    let gateway: Arc<dyn crate::RawRoute> = Arc::new(crate::codemode::gateway::Route(hub.clone()));
    crate::Server::serve_with(info, hub, vec![(crate::codemode::gateway::ROUTE.to_string(), gateway)])
        .map_err(|err| format!("Could not start the code-mode server: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake upstream MCP server (an emcp server) with two tools; `slow`
    /// sleeps. Records `tools/list` and every call with the run its token
    /// named, so a call proves the run's headers rode along.
    struct FakeUpstream {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl crate::ToolProvider for FakeUpstream {
        fn tools(&self) -> Result<Vec<Value>, String> {
            self.seen.lock().unwrap().push("tools/list".to_string());
            Ok(vec![
                json!({ "name": "list_windows", "description": "List windows.", "inputSchema": { "type": "object" } }),
                json!({ "name": "slow", "description": "Sleep ms.", "inputSchema": { "type": "object" } }),
            ])
        }

        fn call(&self, caller: &crate::Caller, name: &str, _arguments: Value) -> Value {
            self.seen.lock().unwrap().push(format!("{} {name}", caller.label));
            if name == "slow" {
                std::thread::sleep(Duration::from_millis(300));
            }
            json!({ "content": [{ "type": "text", "text": "[{\"title\":\"A\"},{\"title\":\"B\"}]" }] })
        }
    }

    fn upstream(name: &str) -> (Upstream, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let info = crate::ServerInfo { name: name.into(), version: "1".into(), instructions: None };
        let server = crate::Server::serve(info, Arc::new(FakeUpstream { seen: seen.clone() })).unwrap();
        let token = server.grant(&id("upstream-run"), &format!("{name}-secret"));
        let url = server.url().to_string();
        // The fake lives for the test process.
        std::mem::forget(server);
        (Upstream { name: name.into(), url, headers: vec![("Authorization".into(), token.header())] }, seen)
    }

    fn id(session_id: &str) -> crate::SessionId {
        crate::SessionId::new(session_id).unwrap()
    }

    fn caller(session_id: &str) -> crate::Caller {
        crate::Caller { session_id: id(session_id), label: "EXP-1".into() }
    }

    /// One `tools/call` as the server hands it to the hub, wrapped like the
    /// old JSON-RPC answer so the assertions read `["result"]`.
    fn call(hub: &Hub, session_id: &str, name: &str, arguments: Value) -> Value {
        json!({ "result": crate::ToolProvider::call(hub, &caller(session_id), name, arguments) })
    }

    fn connect(server: &crate::Server, token: &str) -> Result<crate::Client, ClientError> {
        let upstream = Upstream {
            name: SERVER_NAME.into(),
            url: server.url().to_string(),
            headers: vec![("Authorization".into(), crate::ecore::bearer_header(token))],
        };
        Client::connect(&upstream, "test", Duration::from_secs(10))
    }

    #[test]
    fn the_endpoint_lists_exec_and_describe_to_the_runs_token_only() {
        let hub = Arc::new(Hub::new());
        let server = serve(hub.clone()).unwrap();
        hub.admit("s1", "EXP-1", Vec::new(), Vec::new());
        let token = server.grant(&id("s1"), "EXP-1");
        assert!(matches!(connect(&server, "nope"), Err(ClientError::Http(401))));
        let client = connect(&server, token.as_str()).unwrap();
        let tools = client.list_tools(Duration::from_secs(10)).unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["exec", "describe"]);
        assert_eq!(tools[0]["inputSchema"]["required"], json!(["script"]));
        let reply = client.call("exec", json!({ "script": "return 1 + 1" }), Duration::from_secs(30)).unwrap();
        assert_eq!(reply["structuredContent"]["result"], json!(2), "{reply}");
        assert!(reply["_meta"][META_COUNT_KEY].is_u64(), "the call log's _meta survives the transport: {reply}");
        server.revoke(&id("s1"));
        assert!(hub.run("s1").is_none());
        assert!(matches!(connect(&server, token.as_str()), Err(ClientError::Http(401))));
    }

    #[test]
    fn exec_calls_the_runs_upstreams_with_their_headers_in_parallel_and_describe_has_the_schemas() {
        let (computer, seen) = upstream("computer");
        let hub = Hub::new();
        hub.admit("s1", "EXP-1", vec![computer], vec!["playwright".into()]);
        let token = "s1";
        let started = std::time::Instant::now();
        let script = "const [a, b, w] = await Promise.all([tools.computer.slow({}), tools.computer.slow({}), tools.computer.list_windows({})]); console.log('n', w.length); return w.map((x) => x.title);";
        let reply = call(&hub, &token, "exec", json!({ "script": script }));
        let result = &reply["result"];
        assert!(result.get("isError").is_none(), "{reply}");
        assert_eq!(result["structuredContent"]["ok"], json!(true));
        assert_eq!(result["structuredContent"]["result"], json!(["A", "B"]));
        assert_eq!(result["structuredContent"]["console"], json!("n 2\n"));
        assert_eq!(result["structuredContent"]["calls"]["count"], json!(3));
        // EXP-1279: the call log rides `_meta`, never the text the model reads.
        let meta = &result["_meta"];
        assert_eq!(meta[META_COUNT_KEY], json!(3));
        assert!(meta[META_ELAPSED_KEY].as_u64().is_some(), "{meta}");
        let calls = meta[META_CALLS_KEY].as_array().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|call| call["ok"] == json!(true) && call["ms"].is_u64()), "{calls:?}");
        assert_eq!(calls.iter().filter(|call| call["tool"] == json!("computer.slow")).count(), 2);
        assert_eq!(calls[0]["subject"], Value::Null);
        assert!(!result["content"][0]["text"].as_str().unwrap().contains("exponential/"));
        // The engine takes the same log off the side channel, once.
        let report = crate::codemode::take_report("s1", script).expect("the exec filed its log");
        assert_eq!((report.count, report.calls.len()), (3, 3));
        assert_eq!(crate::codemode::take_report("s1", script), None);
        assert!(started.elapsed() < Duration::from_millis(1000), "two 300 ms calls overlapped: {:?}", started.elapsed());
        let seen = seen.lock().unwrap();
        assert!(seen.iter().filter(|line| *line != "tools/list").all(|line| line.starts_with("computer-secret ")), "{seen:?}");

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
        hub.admit("s1", "EXP-1", vec![computer, dead], Vec::new());
        let token = "s1";
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

    /// EXP-1279: the `_meta` keys and the log cap are the contract's.
    #[test]
    fn the_meta_keys_and_the_log_cap_are_the_script_card_contract() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/script-card.json"
        ))
        .unwrap();
        assert_eq!(fixture["metaKey"], json!(META_CALLS_KEY));
        assert_eq!(fixture["metaCountKey"], json!(META_COUNT_KEY));
        assert_eq!(fixture["metaElapsedKey"], json!(META_ELAPSED_KEY));
        assert_eq!(fixture["logCap"], json!(crate::codemode::LOG_CAP));
        assert_eq!(fixture["toolNameSuffix"], json!(format!("{SERVER_NAME}__exec")));
    }

    #[test]
    fn exec_refuses_a_missing_or_huge_script_and_unknown_tools() {
        let hub = Hub::new();
        hub.admit("s1", "EXP-1", Vec::new(), Vec::new());
        let token = "s1";
        assert_eq!(call(&hub, &token, "exec", json!({}))["result"]["isError"], json!(true));
        let huge = "x".repeat(SCRIPT_MAX_BYTES + 1);
        assert_eq!(call(&hub, &token, "exec", json!({ "script": huge }))["result"]["isError"], json!(true));
        assert_eq!(call(&hub, &token, "nope", json!({}))["result"]["isError"], json!(true));
    }

    #[test]
    fn a_new_admission_cancels_the_runs_old_one_and_an_end_cancels_it() {
        let hub = Hub::new();
        hub.admit("s1", "EXP-1", Vec::new(), Vec::new());
        let old = hub.run("s1").unwrap();
        hub.admit("s1", "EXP-1", Vec::new(), Vec::new());
        assert!(old.cancelled.load(Ordering::Acquire));
        let run = hub.run("s1").unwrap();
        crate::ToolProvider::ended(&hub, &id("s1"));
        assert!(hub.runs.lock().unwrap().is_empty());
        assert!(run.cancelled.load(Ordering::Acquire));
        let refused = call(&hub, "s1", "exec", json!({ "script": "return 1" }));
        assert_eq!(refused["result"]["isError"], json!(true));
    }
}
