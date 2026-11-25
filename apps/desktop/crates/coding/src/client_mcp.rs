//! #FEED-311: the person's OWN MCP clients and scripts on this device, sealed
//! by the key holder that runs here (the desktop app or the CLI daemon).
//!
//! HOLDER ([`serve`]): while a process's key holder runs, the loopback MCP
//! gateway serves one more grant, the PERSON's: no `X-Exp-Session-Id`, the
//! account's own credential upstream, every call through
//! [`SealingTransform::outside`] (content sealed here, results opened here;
//! an install that waits for approval is refused on the device). Address and
//! token (new on every start) are written to a 0600 file beside the
//! account's keyring, removed when the holder stops. The file grants what
//! the account token beside it already grants.
//!
//! CLIENT ([`call_tool`], [`run_stdio`]): finds the live holder's file on
//! EVERY call, so a restarted or updated holder needs no new setup. The
//! stdio server is `exponential mcp serve` and the desktop binary's `mcp`.

use std::io::{BufRead as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::trpc::TrpcClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp_gateway::SealingTransform;

/// Beside the keyring, in `accounts/<id>/`.
const CLI_FILE: &str = "mcp-cli.json";
const DESKTOP_FILE: &str = "mcp-desktop.json";

/// What a client reads without a running holder.
pub const NO_HOLDER: &str = "No Exponential key holder runs on this device, so nothing was sent. Start the Exponential desktop app or `exponential daemon`, signed in and online, then try again.";

/// Past the gateway's own ceiling (a `sessions get` waits up to 600 s).
const FORWARD_TIMEOUT: Duration = Duration::from_secs(670);
/// How long a client's `initialize` waits for a holder that is starting.
const HOLDER_WAIT: Duration = Duration::from_secs(15);

/// The endpoint file.
#[derive(Clone, Serialize, Deserialize)]
pub struct Endpoint {
    v: u32,
    pub url: String,
    token: String,
    pid: u32,
}

/// The file of a holder: `keyring` = its keyring's path.
pub fn endpoint_path(keyring: &Path, daemon: bool) -> PathBuf {
    keyring.with_file_name(if daemon { CLI_FILE } else { DESKTOP_FILE })
}

// --- holder --------------------------------------------------------------

/// The person's grant, served until [`ClientMcp::stop`].
pub struct ClientMcp {
    path: PathBuf,
    run: String,
    trpc: Arc<TrpcClient>,
    /// The upstream headers and the token of the grant that serves now.
    served: Mutex<Option<(Vec<(String, String)>, String)>>,
}

impl ClientMcp {
    /// Serves the person's grant and writes `path`. A failure is logged: the
    /// device then has no local client access, nothing else changes.
    pub fn start(path: PathBuf, daemon: bool, trpc: Arc<TrpcClient>) -> Self {
        let run = format!("client-{}", if daemon { "cli" } else { "desktop" });
        let this = Self { path, run, trpc, served: Mutex::new(None) };
        this.refresh();
        this
    }

    /// Grants again when the account's credential changed since.
    pub fn refresh(&self) {
        let base = self.trpc.base_url().trim_end_matches('/').to_string();
        // A Local desktop has no instance to forward to.
        if !base.starts_with("http") {
            return;
        }
        let headers = self.trpc.auth_headers();
        let mut served = self.served.lock().unwrap_or_else(|p| p.into_inner());
        if served.as_ref().is_some_and(|(known, _)| *known == headers) {
            return;
        }
        let upstream = codemode::Upstream {
            name: crate::mcp_servers::RESERVED_CONFIG_KEY.to_string(),
            url: format!("{base}/api/mcp"),
            headers: headers.clone(),
        };
        let transform = Arc::new(SealingTransform::outside(Some(Arc::clone(&self.trpc))));
        let grant = match codemode::gateway_grant(&self.run, &self.run, upstream, transform) {
            Ok(grant) => grant,
            Err(err) => {
                log::warn!("client-mcp: no local endpoint: {err}");
                return;
            }
        };
        let endpoint = Endpoint { v: 1, url: format!("{}/api/mcp", grant.url), token: grant.token.clone(), pid: std::process::id() };
        let text = serde_json::to_string(&endpoint).unwrap_or_default();
        if let Err(err) = crate::sealing::write_private(&self.path, &text) {
            log::warn!("client-mcp: writing {} failed: {err}", self.path.display());
            codemode::revoke(&self.run);
            return;
        }
        *served = Some((headers, grant.token));
        log::info!("client-mcp: serving this device's MCP clients");
    }

    /// Ends the grant; the file goes while it is still this process's.
    pub fn stop(&self) {
        let Some((_, token)) = self.served.lock().unwrap_or_else(|p| p.into_inner()).take() else { return };
        codemode::revoke(&self.run);
        if read_endpoint(&self.path).is_some_and(|endpoint| endpoint.token == token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Drop for ClientMcp {
    fn drop(&mut self) {
        self.stop();
    }
}

// --- client --------------------------------------------------------------

fn read_endpoint(path: &Path) -> Option<Endpoint> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The live holders of `account` (an account dir), the daemon's first. A
/// dead process's file is skipped: its port may be anyone's by now, and
/// plaintext never goes there.
fn live_endpoints(account: &Path) -> Vec<Endpoint> {
    [CLI_FILE, DESKTOP_FILE]
        .iter()
        .filter_map(|file| read_endpoint(&account.join(file)))
        .filter(|endpoint| endpoint.url.starts_with("http://127.0.0.1:") && crate::process::is_alive(endpoint.pid))
        .collect()
}

/// The endpoint a client of `data_dir` uses: `account`'s (an account id), or
/// the only account a holder runs for.
fn find_endpoint(data_dir: &Path, account: Option<&str>) -> Result<Endpoint, String> {
    let accounts = data_dir.join("accounts");
    let dirs: Vec<PathBuf> = match account {
        Some(id) => vec![accounts.join(id)],
        None => std::fs::read_dir(&accounts)
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default(),
    };
    let mut found: Vec<Endpoint> = dirs.iter().filter_map(|dir| live_endpoints(dir).into_iter().next()).collect();
    match found.len() {
        0 => Err(NO_HOLDER.to_string()),
        1 => Ok(found.remove(0)),
        _ => Err("Several accounts run on this device: set EXP_ACCOUNT=<email> to pick one.".to_string()),
    }
}

/// `EXP_ACCOUNT` (an id or email) → the account id, or the only signed-in one.
fn selected_account(data_dir: &Path) -> Result<Option<String>, String> {
    let signed_in = api::accounts::AuthStore::load(data_dir.to_path_buf()).signed_in_accounts();
    match std::env::var("EXP_ACCOUNT").ok().filter(|value| !value.is_empty()) {
        Some(selector) => signed_in
            .iter()
            .find(|account| account.id == selector || account.email == selector)
            .map(|account| Some(account.id.clone()))
            .ok_or_else(|| format!("EXP_ACCOUNT `{selector}` does not match a signed-in account")),
        None if signed_in.len() == 1 => Ok(Some(signed_in[0].id.clone())),
        None => Ok(None),
    }
}

/// One `tools/call` through the holder of `account` on this device: the
/// tool's decoded JSON output. `None` = no holder runs here.
pub fn call_tool(data_dir: &Path, account: &str, tool: &str, args: &Value) -> Option<Result<Value, api::ApiError>> {
    let endpoint = find_endpoint(data_dir, Some(account)).ok()?;
    let upstream = codemode::Upstream {
        name: "exponential".to_string(),
        url: endpoint.url,
        headers: vec![("Authorization".to_string(), format!("Bearer {}", endpoint.token))],
    };
    Some(api::mcp_tools::call_tool_at(&upstream, tool, args))
}

/// A forwarded message's answers, one JSON-RPC message each.
fn messages_of(content_type: &str, body: &str) -> Vec<String> {
    let compact = |text: &str| serde_json::from_str::<Value>(text).ok().map(|value| value.to_string());
    if content_type.to_ascii_lowercase().starts_with("text/event-stream") {
        return body
            .replace("\r\n", "\n")
            .split("\n\n")
            .filter_map(|event| {
                let data: Vec<&str> =
                    event.lines().filter_map(|line| line.strip_prefix("data:")).map(|line| line.strip_prefix(' ').unwrap_or(line)).collect();
                compact(&data.join("\n"))
            })
            .collect();
    }
    compact(body).into_iter().collect()
}

/// The stdio server's forwarding half.
struct Bridge {
    data_dir: PathBuf,
    http: reqwest::blocking::Client,
    /// The MCP session the upstream named, when it names one.
    session: Mutex<Option<String>>,
    protocol: Mutex<Option<String>>,
}

impl Bridge {
    fn new(data_dir: PathBuf) -> Self {
        let http = exp_http::blocking()
            .connect_timeout(Duration::from_secs(10))
            .timeout(FORWARD_TIMEOUT)
            .no_proxy()
            .build()
            .expect("an HTTP client");
        Self { data_dir, http, session: Mutex::default(), protocol: Mutex::default() }
    }

    fn endpoint(&self, wait: Duration) -> Result<Endpoint, String> {
        let account = selected_account(&self.data_dir)?;
        let deadline = Instant::now() + wait;
        loop {
            match find_endpoint(&self.data_dir, account.as_deref()) {
                Ok(endpoint) => return Ok(endpoint),
                Err(err) if Instant::now() >= deadline => return Err(err),
                Err(_) => std::thread::sleep(Duration::from_millis(250)),
            }
        }
    }

    fn post(&self, endpoint: &Endpoint, line: &str) -> reqwest::Result<reqwest::blocking::Response> {
        let mut request = self
            .http
            .post(&endpoint.url)
            .header("Authorization", format!("Bearer {}", endpoint.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        if let Some(session) = self.session.lock().unwrap().as_deref() {
            request = request.header("Mcp-Session-Id", session);
        }
        if let Some(protocol) = self.protocol.lock().unwrap().as_deref() {
            request = request.header("MCP-Protocol-Version", protocol);
        }
        request.body(line.to_string()).send()
    }

    /// The answers to one line of the client.
    fn forward(&self, line: &str) -> Vec<String> {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return vec![json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }).to_string()];
        };
        let id = message.get("id").filter(|id| !id.is_null()).cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
        let initialize = method == "initialize";
        if initialize {
            *self.session.lock().unwrap() = None;
            *self.protocol.lock().unwrap() = None;
        }
        match self.exchange(line, initialize) {
            Ok(answers) => {
                if initialize {
                    let version = answers
                        .iter()
                        .filter_map(|answer| serde_json::from_str::<Value>(answer).ok())
                        .find_map(|answer| answer.pointer("/result/protocolVersion").and_then(Value::as_str).map(str::to_string));
                    *self.protocol.lock().unwrap() = version;
                }
                answers
            }
            // A notification takes no answer.
            Err(reason) => id.map(|id| refusal(&id, &method, &reason).to_string()).into_iter().collect(),
        }
    }

    fn exchange(&self, line: &str, initialize: bool) -> Result<Vec<String>, String> {
        let mut endpoint = self.endpoint(if initialize { HOLDER_WAIT } else { Duration::ZERO })?;
        let mut response = self.post(&endpoint, line);
        // The holder restarted under us: a refused connection or token means
        // this message was never taken; its new endpoint takes it.
        let stale = match &response {
            Err(err) => err.is_connect(),
            Ok(response) => response.status().as_u16() == 401,
        };
        if stale {
            let next = self.endpoint(Duration::ZERO)?;
            if next.token != endpoint.token {
                endpoint = next;
                response = self.post(&endpoint, line);
            }
        }
        let response = response.map_err(|err| {
            if err.is_connect() {
                NO_HOLDER.to_string()
            } else {
                "The Exponential app or daemon on this device stopped answering; check whether the call arrived before sending it again.".to_string()
            }
        })?;
        let status = response.status();
        if let Some(session) = response.headers().get("mcp-session-id").and_then(|value| value.to_str().ok()) {
            *self.session.lock().unwrap() = Some(session.to_string());
        }
        let content_type =
            response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or_default().to_string();
        let body = response.text().unwrap_or_default();
        let answers = messages_of(&content_type, &body);
        if !status.is_success() && answers.iter().all(|answer| !answer.contains("\"jsonrpc\"")) {
            return Err(match status.as_u16() {
                401 => "Exponential did not accept this device's sign-in: sign in again in the desktop app or with `exponential login`.".to_string(),
                426 => "The Exponential app or CLI on this device is too old: update it.".to_string(),
                code => format!("Exponential answered with HTTP {code}."),
            });
        }
        Ok(answers)
    }
}

/// A request that could not be forwarded, as the client reads it: a tool
/// call's error is its result (the agent reads it), anything else fails.
fn refusal(id: &Value, method: &str, reason: &str) -> Value {
    if method == "tools/call" {
        return json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": reason }], "isError": true } });
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": reason } })
}

/// The MCP stdio server: one JSON-RPC message per line, forwarded to the key
/// holder on this device, until stdin ends. Calls run beside each other;
/// anything else keeps its order.
pub fn run_stdio(data_dir: PathBuf) {
    let bridge = Arc::new(Bridge::new(data_dir));
    let write = |answers: Vec<String>| {
        let mut out = std::io::stdout().lock();
        for answer in answers {
            let _ = writeln!(out, "{answer}");
        }
        let _ = out.flush();
    };
    let mut calls = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if line.contains("\"tools/call\"") {
            let bridge = Arc::clone(&bridge);
            calls.retain(|call: &std::thread::JoinHandle<()>| !call.is_finished());
            calls.push(std::thread::spawn(move || write(bridge.forward(&line))));
        } else {
            write(bridge.forward(&line));
        }
    }
    for call in calls {
        let _ = call.join();
    }
}

#[cfg(test)]
#[path = "client_mcp_tests.rs"]
mod tests;
