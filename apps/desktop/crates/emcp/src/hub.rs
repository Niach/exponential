//! eMCP's LOCAL HUB (#EXP-1507): several MCP servers behind ONE loopback
//! endpoint. A host MOUNTS servers under a name: an upstream (any
//! Streamable HTTP server, its headers resolved) or an in-process
//! [`ToolProvider`]. It admits each agent run with a [`HubGrant`] (a level
//! per server) and gets a bearer token back; the run sees `<server>__<tool>`
//! for exactly the tools its grant covers, and every `tools/call`, refused
//! ones included, becomes one `mcp_calls` row handed to the host's
//! [`AuditSink`]. The rules are `packages/emcp/fixtures/hub.json`
//! ([`crate::contract::hub`]).
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use emcp::hub::{Hub, HubGrant, Mount, MemoryAudit};
//! # fn main() -> Result<(), String> {
//! # let (upstream, provider): (emcp::Upstream, Arc<dyn emcp::ToolProvider>) = todo!();
//! let info = emcp::ServerInfo { name: "hub".into(), version: "1.0.0".into(), instructions: None };
//! let hub = Hub::serve(info, Arc::new(MemoryAudit::default()))?;
//! hub.mount("linear", Mount::Upstream(upstream))?;
//! hub.mount("computer", Mount::Provider(provider))?;
//! let grant: HubGrant = serde_json::from_value(serde_json::json!({
//!     "servers": { "linear": "read", "computer": "write" }, "teamId": null, "userId": null,
//! })).unwrap();
//! let token = hub.grant("run-1", "EXP-12", grant);
//! // MCP config: { "hub": { "type": "http", "url": hub.url(), "headers": { "Authorization": "Bearer <token>" } } }
//! hub.revoke("run-1");
//! # Ok(()) }
//! ```

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

pub use crate::contract::hub::{AuditRow, HubGrant, HubOutcome};
use crate::contract::hub::{self as rules, RowInput};
use crate::server::{error_result, Refusal, UNKNOWN_TOOL_CODE};
use crate::{Caller, Client, ClientError, Server, ServerInfo, ToolProvider, Upstream};

/// The `clientInfo.name` of the hub's upstream calls.
const CLIENT_NAME: &str = "emcp-hub";
/// The handshake with an upstream, and its `tools/list`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// One forwarded call's ceiling.
const CALL_TIMEOUT: Duration = Duration::from_secs(660);

/// Where the hub writes its audit rows. Called after each call, on the
/// call's thread: an implementation that does I/O keeps it short (a failed
/// write is the sink's to log; the call still answers).
pub trait AuditSink: Send + Sync + 'static {
    fn record(&self, row: AuditRow);
}

/// Rows kept in memory (tests, a host that ships them elsewhere).
#[derive(Default)]
pub struct MemoryAudit {
    rows: Mutex<Vec<AuditRow>>,
}

impl MemoryAudit {
    pub fn rows(&self) -> Vec<AuditRow> {
        self.rows.lock().unwrap().clone()
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, row: AuditRow) {
        self.rows.lock().unwrap().push(row);
    }
}

/// One JSON object per line (the row by its column names), appended to a
/// file. Retention is the host's (audit.json `retentionDays`).
pub struct JsonlAudit {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonlAudit {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), lock: Mutex::new(()) }
    }
}

impl AuditSink for JsonlAudit {
    fn record(&self, row: AuditRow) {
        let _held = self.lock.lock().unwrap();
        let line = format!("{}\n", row.to_json());
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if let Err(err) = written {
            log::warn!("[emcp] hub audit: could not write {}: {err}", self.path.display());
        }
    }
}

/// A server behind the hub.
#[derive(Clone)]
pub enum Mount {
    /// A Streamable HTTP MCP server, its headers resolved.
    Upstream(Upstream),
    /// Tools served in this process; they see the run as their caller.
    Provider(Arc<dyn ToolProvider>),
}

struct Mounted {
    mount: Mount,
    /// The upstream's connected client (one handshake per mount).
    client: Mutex<Option<Client>>,
    /// The tools, listed once ([`Hub::refresh`] lists them again).
    tools: Mutex<Option<Vec<Value>>>,
}

impl Mounted {
    fn client(&self, name: &str, upstream: &Upstream) -> Result<Client, ClientError> {
        let mut slot = self.client.lock().unwrap();
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let client = Client::connect(upstream, CLIENT_NAME, CONNECT_TIMEOUT).inspect_err(|err| {
            log::warn!("[emcp] hub: initialize {name} failed: {err}");
        })?;
        *slot = Some(client.clone());
        Ok(client)
    }

    /// The tools; `None` while the server cannot be listed (it is skipped
    /// and tried again on the next list).
    fn tools(&self, name: &str) -> Option<Vec<Value>> {
        if let Some(tools) = self.tools.lock().unwrap().clone() {
            return Some(tools);
        }
        let listed = match &self.mount {
            Mount::Provider(provider) => provider.tools().map_err(|err| err.to_string()),
            Mount::Upstream(upstream) => self
                .client(name, upstream)
                .and_then(|client| client.list_tools(CONNECT_TIMEOUT))
                .map_err(|err| err.to_string()),
        };
        match listed {
            Ok(tools) => {
                *self.tools.lock().unwrap() = Some(tools.clone());
                Some(tools)
            }
            Err(err) => {
                log::warn!("[emcp] hub: listing {name} failed: {err}");
                None
            }
        }
    }

    fn call(&self, name: &str, caller: &Caller, tool: &str, arguments: Value) -> (Value, HubOutcome) {
        match &self.mount {
            Mount::Provider(provider) => {
                let result = provider.call(caller, tool, arguments);
                let outcome = if is_error(&result) { HubOutcome::IsError } else { HubOutcome::Ok };
                (result, outcome)
            }
            Mount::Upstream(upstream) => {
                let called = self.client(name, upstream).and_then(|client| client.call(tool, arguments, CALL_TIMEOUT));
                match called {
                    Ok(result) => {
                        let outcome = if is_error(&result) { HubOutcome::IsError } else { HubOutcome::Ok };
                        (result, outcome)
                    }
                    Err(err) => {
                        let outcome = match &err {
                            ClientError::Rpc { code, .. } if *code == UNKNOWN_TOOL_CODE as i64 => HubOutcome::InvalidParams,
                            ClientError::Rpc { .. } => HubOutcome::RpcError,
                            ClientError::Timeout => HubOutcome::Timeout,
                            ClientError::Http(_) | ClientError::Transport(_) => {
                                // A dead connection is dialled again next time.
                                *self.client.lock().unwrap() = None;
                                HubOutcome::Unreachable
                            }
                        };
                        (serde_json::to_value(error_result(format!("{name}.{tool} failed: {err}"))).unwrap_or_default(), outcome)
                    }
                }
            }
        }
    }
}

fn is_error(result: &Value) -> bool {
    result.get("isError").and_then(Value::as_bool) == Some(true)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|at| at.as_millis() as i64).unwrap_or(0)
}

struct Inner {
    mounts: RwLock<BTreeMap<String, Arc<Mounted>>>,
    grants: Mutex<HashMap<String, HubGrant>>,
    audit: Arc<dyn AuditSink>,
}

impl Inner {
    fn grant_of(&self, caller: &Caller) -> HubGrant {
        self.grants.lock().unwrap().get(&caller.session_id).cloned().unwrap_or_default()
    }

    /// Every listable mount's tools, only the servers the grant names (a
    /// server outside it is never even listed).
    fn catalog(&self, grant: &HubGrant) -> Vec<(String, Arc<Mounted>, Vec<Value>)> {
        let mounts: Vec<(String, Arc<Mounted>)> = self
            .mounts
            .read()
            .unwrap()
            .iter()
            .filter(|(name, _)| grant.level(name) != crate::contract::access::Level::None)
            .map(|(name, mounted)| (name.clone(), mounted.clone()))
            .collect();
        mounts.into_iter().filter_map(|(name, mounted)| Some((name.clone(), mounted.clone(), mounted.tools(&name)?))).collect()
    }
}

impl ToolProvider for Inner {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }

    fn tools_for(&self, caller: &Caller) -> Result<Vec<Value>, String> {
        let grant = self.grant_of(caller);
        let catalog = self.catalog(&grant);
        let servers: Vec<(String, Vec<Value>)> = catalog.iter().map(|(name, _, tools)| (name.clone(), tools.clone())).collect();
        let by_name: HashMap<(&str, &str), &Value> = catalog
            .iter()
            .flat_map(|(server, _, tools)| {
                tools.iter().filter_map(move |tool| Some(((server.as_str(), tool.get("name")?.as_str()?), tool)))
            })
            .collect();
        Ok(rules::tool_list(&grant, &servers)
            .into_iter()
            .filter_map(|listed| {
                let mut tool = (*by_name.get(&(listed.app.as_str(), listed.tool.as_str()))?).clone();
                tool["name"] = json!(listed.name);
                Some(tool)
            })
            .collect())
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        self.try_call(caller, name, arguments)
            .unwrap_or_else(|_| serde_json::to_value(error_result(format!("Unknown tool: {name}"))).unwrap_or_default())
    }

    fn try_call(&self, caller: &Caller, name: &str, arguments: Value) -> Result<Value, Refusal> {
        let started = Instant::now();
        let created_ms = now_ms();
        let grant = self.grant_of(caller);
        let catalog = self.catalog(&grant);
        let servers: Vec<(String, Vec<Value>)> = catalog.iter().map(|(name, _, tools)| (name.clone(), tools.clone())).collect();
        let hash_args = arguments.clone();
        let (answer, outcome) = match rules::route(&grant, &servers, name) {
            None => (Err(Refusal::UnknownTool), HubOutcome::Unknown),
            Some((server, tool)) => {
                let mounted = catalog.iter().find(|(name, _, _)| *name == server).map(|(_, mounted, _)| mounted.clone());
                match mounted {
                    // Unmounted between the list and the call.
                    None => (Err(Refusal::UnknownTool), HubOutcome::Unknown),
                    Some(mounted) => {
                        let (result, outcome) = mounted.call(&server, caller, &tool, arguments);
                        (Ok(result), outcome)
                    }
                }
            }
        };
        self.audit.record(rules::audit_row(RowInput {
            id: uuid::Uuid::new_v4().to_string(),
            grant: &grant,
            run: &caller.session_id,
            name,
            args: Some(&hash_args),
            outcome,
            latency_ms: started.elapsed().as_millis() as u64,
            created_ms,
        }));
        answer
    }

    fn ended(&self, session_id: &str) {
        self.grants.lock().unwrap().remove(session_id);
    }
}

/// A running hub: one loopback listener for the process (hosts keep it in a
/// `OnceLock`); dropping it stops nothing.
pub struct Hub {
    server: Server,
    inner: Arc<Inner>,
}

impl Hub {
    /// Bind `127.0.0.1:0` and serve an empty hub; rows go to `audit`.
    pub fn serve(info: ServerInfo, audit: Arc<dyn AuditSink>) -> Result<Hub, String> {
        let inner = Arc::new(Inner { mounts: RwLock::default(), grants: Mutex::default(), audit });
        let server = Server::serve(info, inner.clone())?;
        Ok(Hub { server, inner })
    }

    /// `http://127.0.0.1:<port>/mcp`.
    pub fn url(&self) -> &str {
        self.server.url()
    }

    /// Put a server behind the hub under `name` (hub.json
    /// `serverNamePattern`); a mount of the same name is replaced.
    pub fn mount(&self, name: &str, mount: Mount) -> Result<(), String> {
        if !rules::server_name_valid(name) {
            return Err(format!("`{name}` is not a hub server name (lowercase letters, digits and `-`, at most {})", rules::SERVER_NAME_MAX));
        }
        let mounted = Arc::new(Mounted { mount, client: Mutex::new(None), tools: Mutex::new(None) });
        self.inner.mounts.write().unwrap().insert(name.to_string(), mounted);
        Ok(())
    }

    /// Take a server out; its tools leave every run's list.
    pub fn unmount(&self, name: &str) {
        self.inner.mounts.write().unwrap().remove(name);
    }

    /// The mounted names, sorted.
    pub fn mounted(&self) -> Vec<String> {
        self.inner.mounts.read().unwrap().keys().cloned().collect()
    }

    /// List a server's tools again on the next `tools/list` (it announced
    /// `list_changed`, or the host knows it changed).
    pub fn refresh(&self, name: &str) {
        if let Some(mounted) = self.inner.mounts.read().unwrap().get(name) {
            *mounted.tools.lock().unwrap() = None;
        }
    }

    /// Admit the run `session_id` with `grant`; an earlier token of the run
    /// dies and its grant is replaced.
    pub fn grant(&self, session_id: &str, label: &str, grant: HubGrant) -> String {
        self.inner.grants.lock().unwrap().insert(session_id.to_string(), grant);
        self.server.grant(session_id, label)
    }

    /// The run's token dies and its grant is dropped.
    pub fn revoke(&self, session_id: &str) {
        self.server.revoke(session_id);
    }

    /// Every run ends (the host is shutting down).
    pub fn revoke_all(&self) {
        self.server.revoke_all();
    }
}
