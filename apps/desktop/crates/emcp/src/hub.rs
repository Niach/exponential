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
//! #NEW-148: the hub is the MCP 2026-07-28 CLIENT of the team servers a
//! launcher mounts FOR ONE RUN ([`Hub::mount_for`]): every upstream request
//! declares elicitation (form + url) and the tasks and skills extensions;
//! an `input_required` answer is fulfilled through the host
//! ([`HubHost::elicit`]) and retried, a task is polled by the hub (its id
//! persisted first, [`Hub::set_task_store`]) with every change one TraceItem
//! ([`HubHost::task_trace`]), a 401 asks the host for fresh credentials
//! ([`HubHost::refresh`]) and retries once, and [`Hub::skills`] fetches and
//! verifies a server's skills. The rules are
//! `packages/emcp/fixtures/hub-client.json` ([`crate::contract::hub_client`]).
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
//! let run = emcp::SessionId::new("run-1").unwrap();
//! let token = hub.grant(&run, "EXP-12", grant);
//! // MCP config: { "hub": { "type": "http", "url": hub.url(), "headers": { "Authorization": token.header() } } }
//! hub.revoke(&run);
//! # Ok(()) }
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub use crate::contract::hub::{AuditRow, HubGrant, HubOutcome};
use crate::contract::hub::{self as rules, RowInput};
use crate::contract::hub_client::{self as client_rules, copy, SkillEntry, TaskStatus};
pub use crate::contract::hub_client::TaskTrace;
use crate::server::{error_result, Refusal, UNKNOWN_TOOL_CODE};
use crate::{CallOutcome, Caller, Client, ClientError, Server, ServerInfo, SessionId, StdioUpstream, Token, ToolProvider, Upstream};

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
    /// #NEW-148: a stdio MCP server the HUB spawns with its env (a team
    /// server's credential among it), on the first call.
    Stdio(StdioUpstream),
    /// Tools served in this process; they see the run as their caller.
    Provider(Arc<dyn ToolProvider>),
}

/// What [`HubHost::elicit`] got from the person.
#[derive(Clone, Debug, PartialEq)]
pub enum Elicited {
    /// The ElicitResult JSON (`{action, content?}`).
    Answered(Value),
    /// No live session to ask: answered `cancel` (`copy.noSession`).
    NoSession,
    /// Nobody answered within the wait (the host withdrew its card): the
    /// call answers `copy.noAnswer`.
    TimedOut,
}

/// #NEW-148: what the hub asks of its host (the launcher) for the team
/// servers it is the client of. Called on the hub's threads; no lock held.
pub trait HubHost: Send + Sync + 'static {
    /// ONE `elicitation/create` of `server` into the run's session: `params`
    /// are the request's params, their `_meta` naming the server under
    /// [`client_rules::SERVER_META_KEY`]. The HOST bounds the wait by `wait`.
    fn elicit(&self, session: &SessionId, server: &str, params: Value, wait: Duration) -> Elicited;
    /// A task's status or message changed: ONE TraceItem in the session;
    /// `result_text` on a terminal status (the result's text, the error, or
    /// `copy.cancelled`).
    fn task_trace(&self, session: &SessionId, trace: TaskTrace, result_text: Option<String>);
    /// The run's mount of `server` answered 401: its credentials again
    /// (tRPC `mcpServers.resolveForLaunch`), `None` = could not.
    fn refresh(&self, session: &SessionId, server: &str) -> Option<Mount>;
}

/// #NEW-148: one persisted task (`tasks.recordKeys`), one object of the
/// store file's array.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub session_id: String,
    pub server_id: String,
    pub server: String,
    pub task_id: String,
    pub tool: String,
    /// ISO-8601 UTC with milliseconds.
    pub created_at: String,
    /// The clamped interval the poller waits between `tasks/get`s.
    pub poll_interval_ms: u64,
}

impl TaskRecord {
    fn key(&self) -> (String, String, String) {
        (self.session_id.clone(), self.server_id.clone(), self.task_id.clone())
    }
}

/// #NEW-148: one verified skill of a team server ([`Hub::skills`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedSkill {
    /// Its SKILL.md URI.
    pub uri: String,
    pub name: String,
    pub description: String,
    /// `sha256:` + hex of its manifest (hub-client.json `digestSetCases`).
    pub digest_set: String,
    /// (path relative to the skill dir, e.g. `SKILL.md`,
    /// `references/x.md`; its verified bytes), in manifest order.
    pub files: Vec<(String, Vec<u8>)>,
}

/// Consecutive failed polls after which a poller stops (its record stays
/// for a resume).
const POLL_FAILURES_MAX: u32 = 30;
/// One `tasks/*` request's ceiling.
const TASK_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a sleeping poller checks it was stopped.
const POLL_SLICE: Duration = Duration::from_millis(50);

struct Mounted {
    /// Replaced by a 401 refresh ([`HubHost::refresh`]).
    mount: RwLock<Mount>,
    /// A run-scoped mount ([`Hub::mount_for`]): (the run, the server id).
    scope: Option<(SessionId, String)>,
    /// The upstream's connected client (one handshake per mount).
    client: Mutex<Option<Client>>,
    /// The tools, listed once ([`Hub::refresh`] lists them again).
    tools: Mutex<Option<Vec<Value>>>,
}

impl Mounted {
    fn new(mount: Mount, scope: Option<(SessionId, String)>) -> Arc<Mounted> {
        Arc::new(Mounted { mount: RwLock::new(mount), scope, client: Mutex::new(None), tools: Mutex::new(None) })
    }

    fn provider(&self) -> Option<Arc<dyn ToolProvider>> {
        match &*self.mount.read().unwrap() {
            Mount::Provider(provider) => Some(provider.clone()),
            Mount::Upstream(_) | Mount::Stdio(_) => None,
        }
    }

    /// The server id a task record names: the run-scoped mount's, else the
    /// mount name.
    fn server_id(&self, name: &str) -> String {
        self.scope.as_ref().map(|(_, id)| id.clone()).unwrap_or_else(|| name.to_string())
    }

    fn client(&self, name: &str) -> Result<Client, ClientError> {
        let mut slot = self.client.lock().unwrap();
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let capabilities = client_rules::client_capabilities();
        let client = match &*self.mount.read().unwrap() {
            Mount::Upstream(upstream) => Client::connect_with(upstream, CLIENT_NAME, &capabilities, CONNECT_TIMEOUT),
            Mount::Stdio(stdio) => Client::spawn_with(stdio, CLIENT_NAME, &capabilities, CONNECT_TIMEOUT),
            Mount::Provider(_) => return Err(ClientError::Transport(format!("{name} is not an MCP server"))),
        }
        .inspect_err(|err| log::warn!("[emcp] hub: initialize {name} failed: {err}"))?;
        *slot = Some(client.clone());
        Ok(client)
    }
}

fn is_error(result: &Value) -> bool {
    result.get("isError").and_then(Value::as_bool) == Some(true)
}

fn error_value(text: String) -> Value {
    serde_json::to_value(error_result(text)).unwrap_or_default()
}

fn text_value(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|at| at.as_millis() as i64).unwrap_or(0)
}

/// A call result's first text (a task's trace line).
fn first_text(result: &Value) -> Option<String> {
    result.get("content")?.as_array()?.iter().find_map(|item| item.get("text")?.as_str().map(str::to_string))
}

/// The persisted task records: memory, mirrored to the store file (temp +
/// rename) when there is one.
#[derive(Default)]
struct TaskStore {
    path: Option<PathBuf>,
    records: Vec<TaskRecord>,
}

impl TaskStore {
    fn write(&self) {
        let Some(path) = &self.path else { return };
        let text = serde_json::to_string_pretty(&self.records).unwrap_or_else(|_| "[]".into());
        let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
        let written = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
        if let Err(err) = written {
            let _ = std::fs::remove_file(&temp);
            log::warn!("[emcp] hub: could not write the task store {}: {err}", path.display());
        }
    }

    fn add(&mut self, record: TaskRecord) {
        self.records.retain(|known| known.key() != record.key());
        self.records.push(record);
        self.write();
    }

    fn remove(&mut self, key: &(String, String, String)) {
        let before = self.records.len();
        self.records.retain(|known| known.key() != *key);
        if self.records.len() != before {
            self.write();
        }
    }
}

/// A running poller.
struct Live {
    stop: Arc<AtomicBool>,
    record: TaskRecord,
    mounted: Arc<Mounted>,
}

/// The poller's final answer to a waiting call.
type Waiter = SyncSender<(Value, HubOutcome)>;

/// What pollers and calls share (no mount table: a poller holds its mount).
struct Core {
    host: RwLock<Option<Arc<dyn HubHost>>>,
    store: Mutex<TaskStore>,
    live: Mutex<HashMap<(String, String, String), Live>>,
    call_wait: RwLock<Duration>,
    ask_wait: RwLock<Duration>,
}

/// Why an input request could not be answered.
enum Unanswered {
    /// The call answers this isError text.
    Refused(String),
    /// The person did not answer in time (`copy.noAnswer`).
    TimedOut,
}

impl Core {
    fn host(&self) -> Option<Arc<dyn HubHost>> {
        self.host.read().unwrap().clone()
    }

    /// `f` on the mount's client. A 401 on a run-scoped mount asks the host
    /// for fresh credentials, replaces the mount, reconnects and retries
    /// ONCE; a dead connection is dialled again next time.
    fn with_client<T>(&self, name: &str, mounted: &Mounted, f: impl Fn(&Client) -> Result<T, ClientError>) -> Result<T, ClientError> {
        let first = mounted.client(name).and_then(|client| f(&client));
        let answered = match first {
            Err(ClientError::Unauthorized) => {
                *mounted.client.lock().unwrap() = None;
                let fresh = match (&mounted.scope, self.host()) {
                    (Some((session, _)), Some(host)) => host.refresh(session, name),
                    _ => None,
                };
                let Some(fresh) = fresh else { return Err(ClientError::Unauthorized) };
                *mounted.mount.write().unwrap() = fresh;
                mounted.client(name).and_then(|client| f(&client))
            }
            other => other,
        };
        if matches!(answered, Err(ClientError::Http(_) | ClientError::Transport(_) | ClientError::Unauthorized)) {
            *mounted.client.lock().unwrap() = None;
        }
        answered
    }

    /// MRTR's (and a task's) input requests answered by key: elicitation
    /// through the host, roots = `mrtr.roots`, sampling refused.
    fn answer_inputs(&self, session: &SessionId, server: &str, requests: &BTreeMap<String, Value>) -> Result<Map<String, Value>, Unanswered> {
        let mut answers = Map::new();
        for (key, request) in requests {
            let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
            let answer = match method {
                "elicitation/create" => self.elicit(session, server, request.get("params").cloned().unwrap_or_else(|| json!({})))?,
                "roots/list" => client_rules::roots(),
                "sampling/createMessage" => return Err(Unanswered::Refused(copy::sampling(server))),
                other => return Err(Unanswered::Refused(format!("{server} asked for `{other}`, which this device does not offer."))),
            };
            answers.insert(key.clone(), answer);
        }
        Ok(answers)
    }

    fn elicit(&self, session: &SessionId, server: &str, mut params: Value) -> Result<Value, Unanswered> {
        if !params.is_object() {
            params = json!({});
        }
        if !params.get("_meta").is_some_and(Value::is_object) {
            params["_meta"] = json!({});
        }
        params["_meta"][client_rules::SERVER_META_KEY] = json!(server);
        let Some(host) = self.host() else { return Ok(client_rules::elicit_result(None)) };
        let wait = *self.ask_wait.read().unwrap();
        match host.elicit(session, server, params, wait) {
            Elicited::Answered(answer) => Ok(client_rules::normalize_elicit_result(&answer)),
            Elicited::NoSession => Ok(client_rules::elicit_result(None)),
            Elicited::TimedOut => Err(Unanswered::TimedOut),
        }
    }

    fn trace(&self, record: &TaskRecord, message: Option<&str>, status: TaskStatus, result_text: Option<String>) {
        let Some(host) = self.host() else { return };
        let Ok(session) = SessionId::new(&record.session_id) else { return };
        host.task_trace(&session, client_rules::trace(&record.server, &record.task_id, &record.tool, message, status), result_text);
    }

    fn forget(&self, key: &(String, String, String)) {
        self.live.lock().unwrap().remove(key);
        self.store.lock().unwrap().remove(key);
    }

    /// Start polling `record` (already persisted). `first` = the state the
    /// call's answer carried (traced before any poll); `waiter` = the call
    /// waiting for the final result. The shared status is what a call that
    /// stops waiting reports.
    fn start_poller(
        self: &Arc<Self>,
        record: TaskRecord,
        mounted: Arc<Mounted>,
        first: Option<Value>,
        waiter: Option<Waiter>,
    ) -> Arc<Mutex<String>> {
        let status = Arc::new(Mutex::new(
            first.as_ref().and_then(|task| task.get("status")).and_then(Value::as_str).unwrap_or("working").to_string(),
        ));
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut live = self.live.lock().unwrap();
            if let Some(old) = live.insert(record.key(), Live { stop: stop.clone(), record: record.clone(), mounted: mounted.clone() }) {
                old.stop.store(true, Ordering::SeqCst);
            }
        }
        let (core, shared) = (self.clone(), status.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("emcp-task-{}", record.task_id))
            .spawn(move || core.poll(record, mounted, first, waiter, stop, shared));
        if let Err(err) = spawned {
            log::warn!("[emcp] hub: could not start a task poller: {err}");
        }
        status
    }

    fn poll(
        &self,
        record: TaskRecord,
        mounted: Arc<Mounted>,
        mut current: Option<Value>,
        mut waiter: Option<Waiter>,
        stop: Arc<AtomicBool>,
        shared: Arc<Mutex<String>>,
    ) {
        let key = record.key();
        let server = record.server.clone();
        let Ok(session) = SessionId::new(&record.session_id) else { return };
        let mut interval = Duration::from_millis(record.poll_interval_ms);
        let mut last: Option<(TaskStatus, Option<String>)> = None;
        let mut asked: HashSet<String> = HashSet::new();
        let mut failures = 0u32;
        let answer = |waiter: &mut Option<Waiter>, value: Value, outcome: HubOutcome| {
            if let Some(waiter) = waiter.take() {
                let _ = waiter.try_send((value, outcome));
            }
        };
        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let task = match current.take() {
                Some(task) => task,
                None => {
                    let deadline = Instant::now() + interval;
                    while Instant::now() < deadline {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        std::thread::sleep(POLL_SLICE.min(deadline.saturating_duration_since(Instant::now())));
                    }
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    match self.with_client(&server, &mounted, |client| client.get_task(&record.task_id, TASK_TIMEOUT)) {
                        Ok(task) => {
                            failures = 0;
                            task
                        }
                        Err(ClientError::Unauthorized) => {
                            // The record stays: a later mount (signed in
                            // again) resumes it.
                            answer(&mut waiter, error_value(copy::sign_in(&server)), HubOutcome::IsError);
                            self.live.lock().unwrap().remove(&key);
                            return;
                        }
                        Err(err) => {
                            failures += 1;
                            if failures >= POLL_FAILURES_MAX {
                                log::warn!("[emcp] hub: polling {server} task {} stopped: {err}", record.task_id);
                                answer(&mut waiter, error_value(format!("{server}: polling task {} failed: {err}", record.task_id)), HubOutcome::Unreachable);
                                self.live.lock().unwrap().remove(&key);
                                return;
                            }
                            continue;
                        }
                    }
                }
            };
            if stop.load(Ordering::SeqCst) {
                return;
            }
            if let Some(ms) = task.get("pollIntervalMs").and_then(Value::as_u64) {
                interval = Duration::from_millis(client_rules::clamp_poll(Some(ms)));
            }
            let Some(status) = task.get("status").and_then(Value::as_str).and_then(TaskStatus::parse) else { continue };
            let message = task.get("statusMessage").and_then(Value::as_str).map(str::to_string);
            *shared.lock().unwrap() = status.as_str().to_string();
            let fin = match status {
                TaskStatus::Completed => {
                    let result = task.get("result").filter(|result| result.is_object()).cloned().unwrap_or_else(|| json!({ "content": [] }));
                    let outcome = if is_error(&result) { HubOutcome::IsError } else { HubOutcome::Ok };
                    Some((first_text(&result), result, outcome))
                }
                TaskStatus::Failed => {
                    let error = task.pointer("/error/message").and_then(Value::as_str).unwrap_or("the task failed").to_string();
                    let text = format!("{server}: {error}");
                    Some((Some(text.clone()), error_value(text), HubOutcome::IsError))
                }
                TaskStatus::Cancelled => {
                    let text = copy::cancelled(&server);
                    Some((Some(text.clone()), error_value(text), HubOutcome::IsError))
                }
                TaskStatus::Working | TaskStatus::InputRequired => None,
            };
            let state = (status, message.clone());
            if last.as_ref() != Some(&state) {
                self.trace(&record, message.as_deref(), status, fin.as_ref().and_then(|(text, _, _)| text.clone()));
                last = Some(state);
            }
            if let Some((_, result, outcome)) = fin {
                self.forget(&key);
                answer(&mut waiter, result, outcome);
                return;
            }
            if status != TaskStatus::InputRequired {
                continue;
            }
            let requests: BTreeMap<String, Value> = task
                .get("inputRequests")
                .and_then(Value::as_object)
                .map(|requests| requests.iter().filter(|(key, _)| !asked.contains(*key)).map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            if requests.is_empty() {
                continue;
            }
            // Asked once: a question nobody answered in time is not asked
            // again on every poll (the task waits, its trace `pending`).
            asked.extend(requests.keys().cloned());
            match self.answer_inputs(&session, &server, &requests) {
                Ok(answers) => {
                    let sent = self.with_client(&server, &mounted, |client| client.update_task(&record.task_id, answers.clone(), TASK_TIMEOUT));
                    if let Err(err) = sent {
                        log::warn!("[emcp] hub: tasks/update on {server} failed: {err}");
                    }
                }
                Err(Unanswered::TimedOut) => answer(&mut waiter, error_value(copy::no_answer(&server)), HubOutcome::IsError),
                Err(Unanswered::Refused(text)) => answer(&mut waiter, error_value(text), HubOutcome::IsError),
            }
        }
    }

    /// A `Task` answer: persisted, then driven by a poller; the call waits
    /// at most `tasks.callWaitMs` for its end.
    fn drive_task(self: &Arc<Self>, session: &SessionId, name: &str, mounted: Arc<Mounted>, tool: &str, task: Value) -> (Value, HubOutcome) {
        let Some(task_id) = task.get("taskId").and_then(Value::as_str).map(str::to_string) else {
            return (error_value(format!("{name}.{tool} answered a task without an id")), HubOutcome::RpcError);
        };
        let record = TaskRecord {
            session_id: session.as_str().to_string(),
            server_id: mounted.server_id(name),
            server: name.to_string(),
            task_id: task_id.clone(),
            tool: tool.to_string(),
            created_at: rules::format_utc_ms(now_ms()),
            poll_interval_ms: client_rules::clamp_poll(task.get("pollIntervalMs").and_then(Value::as_u64)),
        };
        self.store.lock().unwrap().add(record.clone());
        let (sender, receiver) = sync_channel(1);
        let status = self.start_poller(record, mounted, Some(task), Some(sender));
        let wait = *self.call_wait.read().unwrap();
        match receiver.recv_timeout(wait) {
            Ok(answer) => answer,
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
                let status = status.lock().unwrap().clone();
                (text_value(copy::still_running(name, &task_id, &status)), HubOutcome::Ok)
            }
        }
    }

    /// One upstream `tools/call` through MRTR and tasks.
    fn call_upstream(self: &Arc<Self>, name: &str, mounted: Arc<Mounted>, caller: &Caller, tool: &str, arguments: Value) -> (Value, HubOutcome) {
        let (mut responses, mut state, mut rounds) = (None::<Map<String, Value>>, None::<String>, 0usize);
        loop {
            let called = self.with_client(name, &mounted, |client| {
                client.call_once(tool, arguments.clone(), responses.clone(), state.clone(), CALL_TIMEOUT)
            });
            match called {
                Ok(CallOutcome::Complete(result)) => {
                    let outcome = if is_error(&result) { HubOutcome::IsError } else { HubOutcome::Ok };
                    return (result, outcome);
                }
                Ok(CallOutcome::Task(task)) => return self.drive_task(&caller.session_id, name, mounted, tool, task),
                Ok(CallOutcome::InputRequired { requests, state: echoed }) => {
                    if rounds >= client_rules::MAX_ROUNDS {
                        return (error_value(copy::too_many_rounds(name, rounds)), HubOutcome::IsError);
                    }
                    match self.answer_inputs(&caller.session_id, name, &requests) {
                        Ok(answers) => responses = Some(answers),
                        Err(Unanswered::TimedOut) => return (error_value(copy::no_answer(name)), HubOutcome::IsError),
                        Err(Unanswered::Refused(text)) => return (error_value(text), HubOutcome::IsError),
                    }
                    state = echoed;
                    rounds += 1;
                }
                Err(err) => {
                    let outcome = match &err {
                        ClientError::Rpc { code, .. } if *code == UNKNOWN_TOOL_CODE as i64 => HubOutcome::InvalidParams,
                        ClientError::Rpc { .. } => HubOutcome::RpcError,
                        ClientError::Timeout => HubOutcome::Timeout,
                        ClientError::Http(_) | ClientError::Transport(_) | ClientError::Unauthorized => HubOutcome::Unreachable,
                    };
                    let text = match (&err, &mounted.scope) {
                        (ClientError::Unauthorized, Some(_)) => copy::sign_in(name),
                        _ => format!("{name}.{tool} failed: {err}"),
                    };
                    return (error_value(text), outcome);
                }
            }
        }
    }
}

struct Inner {
    mounts: RwLock<BTreeMap<String, Arc<Mounted>>>,
    /// #NEW-148: run-scoped mounts, by run, then name.
    run_mounts: RwLock<HashMap<String, BTreeMap<String, Arc<Mounted>>>>,
    grants: Mutex<HashMap<String, HubGrant>>,
    audit: Arc<dyn AuditSink>,
    core: Arc<Core>,
}

impl Inner {
    fn grant_of(&self, caller: &Caller) -> HubGrant {
        self.grants.lock().unwrap().get(caller.session_id.as_str()).cloned().unwrap_or_default()
    }

    /// The run's mounts: the global ones, its own run-scoped mounts in
    /// place of a global mount of the same name.
    fn mounts_of(&self, session: &SessionId) -> BTreeMap<String, Arc<Mounted>> {
        let mut mounts = self.mounts.read().unwrap().clone();
        if let Some(own) = self.run_mounts.read().unwrap().get(session.as_str()) {
            mounts.extend(own.iter().map(|(name, mounted)| (name.clone(), mounted.clone())));
        }
        mounts
    }

    /// The tools; `None` while the server cannot be listed (it is skipped
    /// and tried again on the next list).
    fn tools(&self, name: &str, mounted: &Mounted) -> Option<Vec<Value>> {
        if let Some(tools) = mounted.tools.lock().unwrap().clone() {
            return Some(tools);
        }
        let listed = match mounted.provider() {
            Some(provider) => provider.tools(),
            None => self.core.with_client(name, mounted, |client| client.list_tools(CONNECT_TIMEOUT)).map_err(|err| err.to_string()),
        };
        match listed {
            Ok(tools) => {
                *mounted.tools.lock().unwrap() = Some(tools.clone());
                Some(tools)
            }
            Err(err) => {
                log::warn!("[emcp] hub: listing {name} failed: {err}");
                None
            }
        }
    }

    /// Every listable mount's tools, only the servers the grant names (a
    /// server outside it is never even listed).
    fn catalog(&self, caller: &Caller, grant: &HubGrant) -> Vec<(String, Arc<Mounted>, Vec<Value>)> {
        let mounts: Vec<(String, Arc<Mounted>)> = self
            .mounts_of(&caller.session_id)
            .into_iter()
            .filter(|(name, _)| grant.level(name) != crate::contract::access::Level::None)
            .collect();
        mounts.into_iter().filter_map(|(name, mounted)| Some((name.clone(), mounted.clone(), self.tools(&name, &mounted)?))).collect()
    }

    fn call_mount(&self, name: &str, mounted: Arc<Mounted>, caller: &Caller, tool: &str, arguments: Value) -> (Value, HubOutcome) {
        match mounted.provider() {
            Some(provider) => {
                let result = provider.call(caller, tool, arguments);
                let outcome = if is_error(&result) { HubOutcome::IsError } else { HubOutcome::Ok };
                (result, outcome)
            }
            None => self.core.call_upstream(name, mounted, caller, tool, arguments),
        }
    }
}

impl ToolProvider for Inner {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }

    fn tools_for(&self, caller: &Caller) -> Result<Vec<Value>, String> {
        let grant = self.grant_of(caller);
        let catalog = self.catalog(caller, &grant);
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
        let catalog = self.catalog(caller, &grant);
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
                        let (result, outcome) = self.call_mount(&server, mounted, caller, &tool, arguments);
                        (Ok(result), outcome)
                    }
                }
            }
        };
        self.audit.record(rules::audit_row(RowInput {
            id: uuid::Uuid::new_v4().to_string(),
            grant: &grant,
            run: caller.session_id.as_str(),
            name,
            args: Some(&hash_args),
            outcome,
            latency_ms: started.elapsed().as_millis() as u64,
            created_ms,
        }));
        answer
    }

    fn ended(&self, session_id: &SessionId) {
        self.grants.lock().unwrap().remove(session_id.as_str());
        // #NEW-148: the run's own mounts go with its grant (their pollers
        // hold their mount and go on; the host's Stop = `cancel_tasks`).
        let own = self.run_mounts.write().unwrap().remove(session_id.as_str()).unwrap_or_default();
        // #EXP-1918: a mounted provider cleans up after the run too (the
        // computer server gives the input back); called outside the lock.
        let mut providers: Vec<Arc<dyn ToolProvider>> = self.mounts.read().unwrap().values().filter_map(|mounted| mounted.provider()).collect();
        providers.extend(own.values().filter_map(|mounted| mounted.provider()));
        for provider in providers {
            provider.ended(session_id);
        }
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
        let core = Arc::new(Core {
            host: RwLock::new(None),
            store: Mutex::default(),
            live: Mutex::default(),
            call_wait: RwLock::new(Duration::from_millis(client_rules::CALL_WAIT_MS)),
            ask_wait: RwLock::new(Duration::from_millis(client_rules::ASK_WAIT_MS)),
        });
        let inner = Arc::new(Inner { mounts: RwLock::default(), run_mounts: RwLock::default(), grants: Mutex::default(), audit, core });
        let server = Server::serve(info, inner.clone())?;
        Ok(Hub { server, inner })
    }

    /// `http://127.0.0.1:<port>/mcp`.
    pub fn url(&self) -> &str {
        self.server.url()
    }

    /// #NEW-148: the launcher behind the hub's team-server client
    /// (elicitation, task traces, 401 refresh). No host = every question
    /// answers cancel, no refresh, traces dropped.
    pub fn set_host(&self, host: Arc<dyn HubHost>) {
        *self.inner.core.host.write().unwrap() = Some(host);
    }

    /// #NEW-148: persist task records in `path` (hub-client.json
    /// `tasks.storeFile` under the data dir); the records already there are
    /// loaded (a mount of their run resumes them). No store = memory only.
    pub fn set_task_store(&self, path: impl Into<PathBuf>) -> Result<(), String> {
        let path = path.into();
        let found: Vec<TaskRecord> = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(format!("{}: {err}", path.display())),
        };
        let mut store = self.inner.core.store.lock().unwrap();
        for record in found {
            if !store.records.iter().any(|known| known.key() == record.key()) {
                store.records.push(record);
            }
        }
        store.path = Some(path);
        store.write();
        Ok(())
    }

    /// The persisted task records.
    pub fn task_records(&self) -> Vec<TaskRecord> {
        self.inner.core.store.lock().unwrap().records.clone()
    }

    /// How long a call waits for its task (`tasks.callWaitMs`; tests).
    pub fn set_call_wait(&self, wait: Duration) {
        *self.inner.core.call_wait.write().unwrap() = wait;
    }

    /// How long one question waits for the person (`mrtr.askWaitMs`; tests).
    pub fn set_ask_wait(&self, wait: Duration) {
        *self.inner.core.ask_wait.write().unwrap() = wait;
    }

    /// Put a server behind the hub under `name` (hub.json
    /// `serverNamePattern`); a mount of the same name is replaced.
    pub fn mount(&self, name: &str, mount: Mount) -> Result<(), String> {
        check_name(name)?;
        self.inner.mounts.write().unwrap().insert(name.to_string(), Mounted::new(mount, None));
        Ok(())
    }

    /// #NEW-148: mount `name` FOR ONE RUN (a team server under its config
    /// key, `server_id` its row): only `session`'s grant sees it, in place
    /// of a global mount of the same name; it goes with the run's grant
    /// ([`Self::revoke`]). The run's persisted tasks on `server_id` are
    /// polled again (a resume; their results reach only the trace).
    pub fn mount_for(&self, session: &SessionId, name: &str, server_id: &str, mount: Mount) -> Result<(), String> {
        check_name(name)?;
        let mounted = Mounted::new(mount, Some((session.clone(), server_id.to_string())));
        self.inner
            .run_mounts
            .write()
            .unwrap()
            .entry(session.as_str().to_string())
            .or_default()
            .insert(name.to_string(), mounted.clone());
        if mounted.provider().is_some() {
            return Ok(());
        }
        let core = &self.inner.core;
        let resumable: Vec<TaskRecord> = {
            let live = core.live.lock().unwrap();
            core.store
                .lock()
                .unwrap()
                .records
                .iter()
                .filter(|record| record.session_id == session.as_str() && record.server_id == server_id && !live.contains_key(&record.key()))
                .cloned()
                .collect()
        };
        for mut record in resumable {
            record.server = name.to_string();
            core.start_poller(record, mounted.clone(), None, None);
        }
        Ok(())
    }

    /// #NEW-148: take a run's own mount out.
    pub fn unmount_for(&self, session: &SessionId, name: &str) {
        if let Some(own) = self.inner.run_mounts.write().unwrap().get_mut(session.as_str()) {
            own.remove(name);
        }
    }

    /// Take a server out; its tools leave every run's list.
    pub fn unmount(&self, name: &str) {
        self.inner.mounts.write().unwrap().remove(name);
    }

    /// The mounted names, sorted (the global mounts).
    pub fn mounted(&self) -> Vec<String> {
        self.inner.mounts.read().unwrap().keys().cloned().collect()
    }

    /// List a server's tools again on the next `tools/list` (it announced
    /// `list_changed`, or the host knows it changed).
    pub fn refresh(&self, name: &str) {
        if let Some(mounted) = self.inner.mounts.read().unwrap().get(name) {
            *mounted.tools.lock().unwrap() = None;
        }
        for own in self.inner.run_mounts.read().unwrap().values() {
            if let Some(mounted) = own.get(name) {
                *mounted.tools.lock().unwrap() = None;
            }
        }
    }

    /// #NEW-148: STOP: `tasks/cancel` for every live task of the run's
    /// session; their pollers stop and every record of the session ends
    /// (each live task traced `cancelled`).
    pub fn cancel_tasks(&self, session: &SessionId) {
        let core = &self.inner.core;
        let lives: Vec<Live> = {
            let mut live = core.live.lock().unwrap();
            let keys: Vec<_> = live.keys().filter(|key| key.0 == session.as_str()).cloned().collect();
            keys.into_iter().filter_map(|key| live.remove(&key)).collect()
        };
        for live in &lives {
            live.stop.store(true, Ordering::SeqCst);
        }
        for live in lives {
            let record = &live.record;
            if let Err(err) = core.with_client(&record.server, &live.mounted, |client| client.cancel_task(&record.task_id, TASK_TIMEOUT)) {
                log::warn!("[emcp] hub: tasks/cancel on {} failed: {err}", record.server);
            }
            core.trace(record, None, TaskStatus::Cancelled, Some(copy::cancelled(&record.server)));
        }
        let mut store = core.store.lock().unwrap();
        let before = store.records.len();
        store.records.retain(|record| record.session_id != session.as_str());
        if store.records.len() != before {
            store.write();
        }
    }

    /// #NEW-148: the skills of the run's mount `server` (else the global
    /// one): `skills/list` when it declares the extension, each skill
    /// checked against skills.json `limits`, every file read and VERIFIED
    /// (size, then digest). A skill with any problem is dropped with a
    /// warning. `Err` = the server could not be listed at all.
    pub fn skills(&self, session: &SessionId, server: &str) -> Result<(Vec<FetchedSkill>, Vec<String>), String> {
        let mounted = self.inner.mounts_of(session).get(server).cloned().ok_or_else(|| format!("{server} is not mounted"))?;
        if mounted.provider().is_some() {
            return Err(format!("{server} is not an MCP server"));
        }
        let core = &self.inner.core;
        let supports = core.with_client(server, &mounted, |client| Ok(client.supports_skills())).map_err(|err| format!("{server}: {err}"))?;
        if !supports {
            return Ok((Vec::new(), Vec::new()));
        }
        let listed = core.with_client(server, &mounted, |client| client.list_skills(CONNECT_TIMEOUT)).map_err(|err| format!("{server}: {err}"))?;
        let (mut skills, mut warnings) = (Vec::new(), Vec::new());
        for skill in listed {
            match fetch_skill(core, server, &mounted, &skill) {
                Ok(fetched) => skills.push(fetched),
                Err(reason) => {
                    let uri = skill.get("uri").and_then(Value::as_str).unwrap_or("?");
                    warnings.push(format!("{server}: the skill {uri} was dropped: {reason}"));
                }
            }
        }
        Ok((skills, warnings))
    }

    /// Admit the run `session_id` with `grant`; an earlier token of the run
    /// dies and its grant is replaced.
    pub fn grant(&self, session_id: &SessionId, label: &str, grant: HubGrant) -> Token {
        self.inner.grants.lock().unwrap().insert(session_id.as_str().to_string(), grant);
        self.server.grant(session_id, label)
    }

    /// The run's token dies, its grant and its own mounts are dropped and
    /// every mounted provider hears it ended.
    pub fn revoke(&self, session_id: &SessionId) {
        self.server.revoke(session_id);
        self.inner.run_mounts.write().unwrap().remove(session_id.as_str());
    }

    /// Every run ends (the host is shutting down).
    pub fn revoke_all(&self) {
        self.server.revoke_all();
        self.inner.run_mounts.write().unwrap().clear();
    }
}

fn check_name(name: &str) -> Result<(), String> {
    match rules::server_name_valid(name) {
        true => Ok(()),
        false => Err(format!("`{name}` is not a hub server name (lowercase letters, digits and `-`, at most {})", rules::SERVER_NAME_MAX)),
    }
}

/// One listed skill read and verified, or why it is dropped.
fn fetch_skill(core: &Core, server: &str, mounted: &Mounted, skill: &Value) -> Result<FetchedSkill, String> {
    let uri = skill.get("uri").and_then(Value::as_str).ok_or("no uri")?.to_string();
    let root = uri.strip_suffix("SKILL.md").filter(|root| root.ends_with('/')).ok_or("its uri is not a SKILL.md")?;
    let name = skill.pointer("/frontmatter/name").and_then(Value::as_str).ok_or("no name")?.to_string();
    let description = skill.pointer("/frontmatter/description").and_then(Value::as_str).unwrap_or_default().to_string();
    let entries: Vec<SkillEntry> = match skill.get("resources") {
        Some(Value::Array(rows)) => rows
            .iter()
            .map(|row| serde_json::from_value(row.clone()).map_err(|err| format!("a malformed manifest row ({err})")))
            .collect::<Result<_, _>>()?,
        _ => return Err("its manifest is not a file list".into()),
    };
    if entries.len() > client_rules::SKILL_MAX_FILES {
        return Err(format!("{} files, over {}", entries.len(), client_rules::SKILL_MAX_FILES));
    }
    let total: u64 = entries.iter().map(|entry| entry.size).sum();
    if total > client_rules::SKILL_MAX_BYTES {
        return Err(format!("{total} bytes, over {}", client_rules::SKILL_MAX_BYTES));
    }
    let mut files = Vec::new();
    for entry in &entries {
        let path = entry.uri.strip_prefix(root).ok_or_else(|| format!("{} is outside the skill", entry.uri))?;
        if !client_rules::relative_path_ok(path) {
            return Err(format!("{} is not a safe path", entry.uri));
        }
        if files.iter().any(|(known, _): &(String, Vec<u8>)| known == path) {
            return Err(format!("{path} is listed twice"));
        }
        let bytes = core.with_client(server, mounted, |client| client.read_resource(&entry.uri, CONNECT_TIMEOUT)).map_err(|err| format!("{path}: {err}"))?;
        client_rules::verify(entry, &bytes).map_err(|mismatch| format!("{path}: {} mismatch", mismatch.as_str()))?;
        files.push((path.to_string(), bytes));
    }
    if !files.iter().any(|(path, _)| path == "SKILL.md") {
        return Err("no SKILL.md in its manifest".into());
    }
    Ok(FetchedSkill { uri, name, description, digest_set: client_rules::digest_set_of(&entries), files })
}

/// The task store's path for a data dir (hub-client.json `tasks.storeFile`).
pub fn task_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(client_rules::TASK_STORE_FILE)
}
