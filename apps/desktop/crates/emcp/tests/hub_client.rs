//! #NEW-148: the hub as the MCP 2026-07-28 client of a team server
//! (`packages/emcp/fixtures/hub-client.json`), against a FAKE upstream: an
//! rmcp server in this process (stateless Streamable HTTP behind a bearer
//! check), driven through the hub's own loopback endpoint like a CLI would.
//! MRTR elicitation through a fake [`HubHost`], tasks polled, answered,
//! cancelled and resumed, the 401 refresh, skills fetched and verified,
//! and run-scoped mounts.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bytes::Bytes;
use emcp::contract::hub_client::{self, sha256_digest};
use emcp::hub::{Elicited, Hub, HubGrant, HubHost, MemoryAudit, Mount, TaskRecord, TaskTrace};
use emcp::{Caller, Client, ServerInfo, SessionId, Token, ToolProvider, Upstream};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CancelTaskParams, GetTaskParams, GetTaskResult, ListSkillsResult, ListToolsResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ServerConfig, UpdateTaskParams,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};

const T: Duration = Duration::from_secs(20);

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap())
}

const SKILL_MD: &str = "---\nname: code-review\ndescription: Review a diff\n---\n\nRead the diff.\n";
const CHECKLIST: &str = "# Checklist\n- tests\n";
const TAMPERED: &str = "---\nname: tampered\ndescription: d\n---\n\nrm -rf\n";

/// What a scripted task does on each `tasks/get`.
#[derive(Clone, Copy, PartialEq)]
enum Script {
    /// working "Encoding 40%", then completed.
    Complete,
    /// input_required (q1) until `tasks/update`, then completed.
    Input,
    /// working forever (until cancelled).
    Forever,
}

#[derive(Default)]
struct FakeState {
    token: Mutex<String>,
    tasks: Mutex<HashMap<String, (Script, usize, Option<Value>, bool)>>,
    next_task: Mutex<usize>,
    /// Every tools/call's (tool, inputResponses, requestState, declared caps).
    calls: Mutex<Vec<(String, Option<Value>, Option<String>, Value)>>,
    updates: Mutex<Vec<(String, Value)>>,
    cancels: Mutex<Vec<String>>,
    /// The task store file, read at each task's FIRST tasks/get.
    store: Mutex<Option<PathBuf>>,
    stored_at_first_poll: Mutex<Vec<(String, bool)>>,
}

#[derive(Clone)]
struct Fake(Arc<FakeState>);

fn ok_text(text: &str) -> CallToolResponse {
    CallToolResponse::Complete(serde_json::from_value(json!({ "content": [{ "type": "text", "text": text }] })).unwrap())
}

fn elicit_q1() -> Value {
    json!({ "q1": { "method": "elicitation/create", "params": {
        "mode": "form", "message": "Which team?",
        "requestedSchema": { "type": "object", "properties": { "team": { "type": "string" } }, "required": ["team"] },
    } } })
}

impl Fake {
    fn new_task(&self, script: Script) -> String {
        let mut next = self.0.next_task.lock().unwrap();
        *next += 1;
        let id = format!("t-{next}");
        self.0.tasks.lock().unwrap().insert(id.clone(), (script, 0, None, false));
        id
    }
}

fn now() -> &'static str {
    "2026-11-26T10:00:00.000Z"
}

impl ServerHandler for Fake {
    fn get_info(&self) -> ServerConfig {
        serde_json::from_value(json!({
            "protocolVersion": rmcp::model::ProtocolVersion::LATEST,
            "capabilities": { "tools": {}, "resources": {}, "extensions": {
                "io.modelcontextprotocol/tasks": {}, "io.modelcontextprotocol/skills": {},
            } },
            "serverInfo": { "name": "acme", "version": "1" },
        }))
        .unwrap()
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, ErrorData> {
        let tools = ["ask", "slow", "ask_task", "forever", "echo"]
            .iter()
            .map(|name| serde_json::from_value(json!({ "name": name, "description": "d", "inputSchema": { "type": "object" } })).unwrap())
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData> {
        let declared = context.client_capabilities().map(|caps| serde_json::to_value(caps).unwrap()).unwrap_or(Value::Null);
        let responses = request.input_responses.as_ref().map(|r| serde_json::to_value(r).unwrap());
        self.0.calls.lock().unwrap().push((request.name.to_string(), responses.clone(), request.request_state.clone(), declared));
        let task = |id: String| {
            CallToolResponse::Task(
                serde_json::from_value(json!({ "resultType": "task", "taskId": id, "status": "working", "createdAt": now(), "lastUpdatedAt": now(), "ttlMs": null, "pollIntervalMs": 100 }))
                    .unwrap(),
            )
        };
        Ok(match request.name.as_ref() {
            "ask" => match (responses, request.request_state.as_deref()) {
                (Some(responses), Some("s1")) => ok_text(&format!("got {}", responses["q1"]["content"]["team"].as_str().unwrap_or("nothing"))),
                _ => CallToolResponse::InputRequired(
                    serde_json::from_value(json!({ "resultType": "input_required", "inputRequests": elicit_q1(), "requestState": "s1" })).unwrap(),
                ),
            },
            "slow" => task(self.new_task(Script::Complete)),
            "ask_task" => task(self.new_task(Script::Input)),
            "forever" => task(self.new_task(Script::Forever)),
            _ => ok_text("echo"),
        })
    }

    async fn get_task(&self, request: GetTaskParams, _context: RequestContext<RoleServer>) -> Result<GetTaskResult, ErrorData> {
        let id = request.task_id;
        let first = {
            let tasks = self.0.tasks.lock().unwrap();
            tasks.get(&id).map(|task| task.1 == 0)
        };
        if first == Some(true) {
            let stored = self.0.store.lock().unwrap().as_ref().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
            self.0.stored_at_first_poll.lock().unwrap().push((id.clone(), stored.contains(&format!("\"{id}\""))));
        }
        let mut tasks = self.0.tasks.lock().unwrap();
        let Some(task) = tasks.get_mut(&id) else { return Err(ErrorData::invalid_params("no such task", None)) };
        task.1 += 1;
        let base = json!({ "taskId": id, "createdAt": now(), "lastUpdatedAt": now(), "ttlMs": null, "pollIntervalMs": 100 });
        let mut wire = base.clone();
        let fields = if task.3 {
            json!({ "status": "cancelled" })
        } else {
            match task.0 {
                Script::Complete if task.1 == 1 => json!({ "status": "working", "statusMessage": "Encoding 40%" }),
                Script::Complete => json!({ "status": "completed", "statusMessage": "Done", "result": { "content": [{ "type": "text", "text": "exported" }] } }),
                Script::Input => match &task.2 {
                    None => json!({ "status": "input_required", "inputRequests": elicit_q1() }),
                    Some(answer) => json!({ "status": "completed", "result": { "content": [{ "type": "text", "text": format!("answered {}", answer["q1"]["content"]["team"].as_str().unwrap_or("?")) }] } }),
                },
                Script::Forever => json!({ "status": "working" }),
            }
        };
        for (key, value) in fields.as_object().unwrap() {
            wire[key] = value.clone();
        }
        Ok(serde_json::from_value(wire).unwrap())
    }

    async fn update_task(&self, request: UpdateTaskParams, _context: RequestContext<RoleServer>) -> Result<(), ErrorData> {
        let responses = serde_json::to_value(&request.input_responses).unwrap();
        self.0.updates.lock().unwrap().push((request.task_id.clone(), responses.clone()));
        if let Some(task) = self.0.tasks.lock().unwrap().get_mut(&request.task_id) {
            task.2 = Some(responses);
        }
        Ok(())
    }

    async fn cancel_task(&self, request: CancelTaskParams, _context: RequestContext<RoleServer>) -> Result<(), ErrorData> {
        self.0.cancels.lock().unwrap().push(request.task_id.clone());
        if let Some(task) = self.0.tasks.lock().unwrap().get_mut(&request.task_id) {
            task.3 = true;
        }
        Ok(())
    }

    async fn list_skills(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListSkillsResult, ErrorData> {
        let row = |uri: &str, bytes: &str| json!({ "uri": uri, "digest": sha256_digest(bytes.as_bytes()), "size": bytes.len() });
        Ok(serde_json::from_value(json!({ "resultType": "complete", "ttlMs": 0, "cacheScope": "private", "skills": [
            { "uri": "skill://code-review/SKILL.md", "frontmatter": { "name": "code-review", "description": "Review a diff" },
              "resources": [row("skill://code-review/SKILL.md", SKILL_MD), row("skill://code-review/references/checklist.md", CHECKLIST)] },
            // The manifest promises other bytes than the read returns.
            { "uri": "skill://tampered/SKILL.md", "frontmatter": { "name": "tampered", "description": "d" },
              "resources": [row("skill://tampered/SKILL.md", "---\nname: tampered\ndescription: d\n---\n\nfine\n")] },
        ] }))
        .unwrap())
    }

    async fn read_resource(&self, request: ReadResourceRequestParams, _context: RequestContext<RoleServer>) -> Result<ReadResourceResponse, ErrorData> {
        let text = match request.uri.as_str() {
            "skill://code-review/SKILL.md" => SKILL_MD,
            "skill://code-review/references/checklist.md" => CHECKLIST,
            "skill://tampered/SKILL.md" => TAMPERED,
            _ => return Err(ErrorData::invalid_params("not served", None)),
        };
        Ok(ReadResourceResponse::Complete(
            serde_json::from_value(json!({ "contents": [{ "uri": request.uri, "mimeType": "text/markdown", "text": text }] })).unwrap(),
        ))
    }
}

/// Serve the fake on `127.0.0.1:0/mcp` behind `Authorization: Bearer
/// <state.token>` (anything else = 401).
fn serve_fake(token: &str) -> (Fake, String) {
    let fake = Fake(Arc::new(FakeState::default()));
    *fake.0.token.lock().unwrap() = token.to_string();
    let listener = runtime().block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handler = fake.clone();
    let config = StreamableHttpServerConfig::default().with_legacy_session_mode(false).with_json_response(true).with_sse_keep_alive(None);
    let service = Arc::new(StreamableHttpService::new(move || Ok(handler.clone()), Arc::new(NeverSessionManager::default()), config));
    let state = fake.0.clone();
    runtime().spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let (service, state) = (service.clone(), state.clone());
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let route = hyper::service::service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
                    let (service, state) = (service.clone(), state.clone());
                    async move {
                        let want = format!("Bearer {}", state.token.lock().unwrap());
                        let given = request.headers().get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
                        if given != want {
                            let mut response: hyper::Response<BoxBody<Bytes, Infallible>> = hyper::Response::new(Full::new(Bytes::new()).boxed());
                            *response.status_mut() = hyper::StatusCode::UNAUTHORIZED;
                            return Ok::<_, Infallible>(response);
                        }
                        Ok(service.handle(request).await)
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, route).await;
            });
        }
    });
    (fake, format!("http://127.0.0.1:{port}/mcp"))
}

fn upstream(url: &str, token: &str) -> Mount {
    Mount::Upstream(Upstream { name: "acme".into(), url: url.into(), headers: vec![("Authorization".into(), format!("Bearer {token}"))] })
}

#[derive(Default)]
struct Host {
    asked: Mutex<Vec<(String, String, Value)>>,
    answers: Mutex<Vec<Elicited>>,
    traces: Mutex<Vec<(String, TaskTrace, Option<String>)>>,
    refreshes: Mutex<usize>,
    fresh: Mutex<Option<Mount>>,
}

impl HubHost for Host {
    fn elicit(&self, session: &SessionId, server: &str, params: Value, _wait: Duration) -> Elicited {
        self.asked.lock().unwrap().push((session.to_string(), server.to_string(), params));
        let mut answers = self.answers.lock().unwrap();
        match answers.is_empty() {
            true => Elicited::Answered(json!({ "action": "accept", "content": { "team": "ios" } })),
            false => answers.remove(0),
        }
    }

    fn task_trace(&self, session: &SessionId, trace: TaskTrace, result_text: Option<String>) {
        self.traces.lock().unwrap().push((session.to_string(), trace, result_text));
    }

    fn refresh(&self, _session: &SessionId, _server: &str) -> Option<Mount> {
        *self.refreshes.lock().unwrap() += 1;
        self.fresh.lock().unwrap().take()
    }
}

fn id(text: &str) -> SessionId {
    SessionId::new(text).unwrap()
}

fn info() -> ServerInfo {
    ServerInfo { name: "hub".into(), version: "1".into(), instructions: None }
}

fn write_grant(servers: &[&str]) -> HubGrant {
    let servers: serde_json::Map<String, Value> = servers.iter().map(|name| (name.to_string(), json!("write"))).collect();
    serde_json::from_value(json!({ "servers": servers, "teamId": "t1", "userId": "u1" })).unwrap()
}

fn connect(hub: &Hub, token: &Token) -> Client {
    Client::connect(&Upstream { name: "hub".into(), url: hub.url().into(), headers: vec![("Authorization".into(), token.header())] }, "cli", T).unwrap()
}

fn hub_with_host() -> (Hub, Arc<Host>) {
    let hub = Hub::serve(info(), Arc::new(MemoryAudit::default())).unwrap();
    let host = Arc::new(Host::default());
    hub.set_host(host.clone());
    (hub, host)
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap_or_default()
}

fn temp_store() -> PathBuf {
    std::env::temp_dir().join(format!("emcp-hub-tasks-{}.json", uuid::Uuid::new_v4()))
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + T;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn an_input_required_call_is_answered_through_the_host_and_retried() {
    let (fake, url) = serve_fake("k1");
    let (hub, host) = hub_with_host();
    let run = id("run-mrtr");
    hub.mount_for(&run, "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    let client = connect(&hub, &hub.grant(&run, "EXP-1", write_grant(&["acme"])));
    let result = client.call("acme__ask", json!({}), T).unwrap();
    assert_eq!(text(&result), "got ios", "{result}");
    assert_ne!(result["isError"], json!(true));
    // ONE question, its _meta naming the server for THE elicitation card.
    let asked = host.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].0.as_str(), asked[0].1.as_str()), ("run-mrtr", "acme"));
    assert_eq!(asked[0].2["_meta"][hub_client::SERVER_META_KEY], "acme");
    assert_eq!(asked[0].2["message"], "Which team?");
    // The retry: the answer under its key, the state echoed; every request
    // declared hub-client.json's capabilities.
    let calls = fake.0.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].1.as_ref().unwrap()["q1"], json!({ "action": "accept", "content": { "team": "ios" } }));
    assert_eq!(calls[1].2.as_deref(), Some("s1"));
    for call in &calls {
        assert_eq!(call.3, hub_client::client_capabilities(), "declared capabilities");
    }
    // Nobody answers in time: the call says so (an error the agent reads).
    host.answers.lock().unwrap().push(Elicited::TimedOut);
    let result = client.call("acme__ask", json!({}), T).unwrap();
    assert_eq!(result["isError"], json!(true));
    assert_eq!(text(&result), hub_client::copy::no_answer("acme"));
    // No live session: cancel goes back to the server.
    host.answers.lock().unwrap().push(Elicited::NoSession);
    let result = client.call("acme__ask", json!({}), T).unwrap();
    assert_eq!(text(&result), "got nothing");
    assert_eq!(fake.0.calls.lock().unwrap().last().unwrap().1.as_ref().unwrap()["q1"], json!({ "action": "cancel" }));
}

#[test]
fn a_task_is_persisted_polled_traced_and_its_result_returned() {
    let (fake, url) = serve_fake("k1");
    let (hub, host) = hub_with_host();
    let store = temp_store();
    hub.set_task_store(&store).unwrap();
    *fake.0.store.lock().unwrap() = Some(store.clone());
    let run = id("run-task");
    hub.mount_for(&run, "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    let client = connect(&hub, &hub.grant(&run, "EXP-1", write_grant(&["acme"])));
    let result = client.call("acme__slow", json!({}), T).unwrap();
    assert_eq!(text(&result), "exported", "{result}");
    // The record was on disk before the first poll, and is gone now.
    assert_eq!(fake.0.stored_at_first_poll.lock().unwrap().as_slice(), [("t-1".to_string(), true)]);
    let on_disk: Vec<TaskRecord> = serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
    assert!(on_disk.is_empty(), "{on_disk:?}");
    assert!(hub.task_records().is_empty());
    let traces: Vec<(String, String, String, Option<String>)> = host
        .traces
        .lock()
        .unwrap()
        .iter()
        .map(|(session, trace, result)| (session.clone(), trace.title.clone(), trace.status.clone(), result.clone()))
        .collect();
    assert_eq!(
        traces,
        [
            ("run-task".into(), "acme: slow".into(), "running".into(), None),
            ("run-task".into(), "acme: Encoding 40%".into(), "running".into(), None),
            ("run-task".into(), "acme: Done".into(), "done".into(), Some("exported".into())),
        ]
    );
    assert!(host.traces.lock().unwrap().iter().all(|(_, trace, _)| trace.id == "mcp-task:acme:t-1"));
    let _ = std::fs::remove_file(&store);
}

#[test]
fn a_task_asking_for_input_is_answered_with_tasks_update() {
    let (fake, url) = serve_fake("k1");
    let (hub, host) = hub_with_host();
    let run = id("run-task-input");
    hub.mount_for(&run, "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    let client = connect(&hub, &hub.grant(&run, "EXP-1", write_grant(&["acme"])));
    let result = client.call("acme__ask_task", json!({}), T).unwrap();
    assert_eq!(text(&result), "answered ios", "{result}");
    assert_eq!(
        fake.0.updates.lock().unwrap().as_slice(),
        [("t-1".to_string(), json!({ "q1": { "action": "accept", "content": { "team": "ios" } } }))]
    );
    assert_eq!(host.asked.lock().unwrap()[0].2["_meta"][hub_client::SERVER_META_KEY], "acme");
    let statuses: Vec<String> = host.traces.lock().unwrap().iter().map(|(_, trace, _)| trace.status.clone()).collect();
    assert_eq!(statuses, ["running", "pending", "done"]);
}

#[test]
fn a_long_task_answers_still_running_and_stop_cancels_it() {
    let (fake, url) = serve_fake("k1");
    let (hub, host) = hub_with_host();
    let store = temp_store();
    hub.set_task_store(&store).unwrap();
    hub.set_call_wait(Duration::from_millis(300));
    let run = id("run-forever");
    hub.mount_for(&run, "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    let client = connect(&hub, &hub.grant(&run, "EXP-1", write_grant(&["acme"])));
    let result = client.call("acme__forever", json!({}), T).unwrap();
    assert_ne!(result["isError"], json!(true), "{result}");
    assert_eq!(text(&result), hub_client::copy::still_running("acme", "t-1", "working"));
    assert_eq!(hub.task_records().len(), 1);
    hub.cancel_tasks(&run);
    assert_eq!(fake.0.cancels.lock().unwrap().as_slice(), ["t-1"]);
    assert!(hub.task_records().is_empty());
    let on_disk: Vec<TaskRecord> = serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
    assert!(on_disk.is_empty());
    let last = host.traces.lock().unwrap().last().cloned().unwrap();
    assert_eq!((last.1.status.as_str(), last.2.as_deref()), ("skipped", Some(hub_client::copy::cancelled("acme").as_str())));
    // The poller stopped: no more polls after a pause.
    std::thread::sleep(Duration::from_millis(200));
    let polls = fake.0.tasks.lock().unwrap()["t-1"].1;
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(fake.0.tasks.lock().unwrap()["t-1"].1, polls);
    let _ = std::fs::remove_file(&store);
}

#[test]
fn a_persisted_task_resumes_when_its_run_mounts_the_server_again() {
    let (fake, url) = serve_fake("k1");
    let resumed = fake.new_task(Script::Complete);
    let store = temp_store();
    let record = TaskRecord {
        session_id: "run-resume".into(),
        server_id: "srv-acme".into(),
        server: "acme".into(),
        task_id: resumed.clone(),
        tool: "slow".into(),
        created_at: now().into(),
        poll_interval_ms: 500,
    };
    std::fs::write(&store, serde_json::to_string(&vec![record.clone()]).unwrap()).unwrap();
    let (hub, host) = hub_with_host();
    hub.set_task_store(&store).unwrap();
    assert_eq!(hub.task_records(), vec![record]);
    // Another run's mount of the same server resumes nothing.
    hub.mount_for(&id("run-other"), "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(fake.0.tasks.lock().unwrap()[&resumed].1, 0);
    hub.mount_for(&id("run-resume"), "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    wait_until("the resumed task to end", || hub.task_records().is_empty());
    let traces = host.traces.lock().unwrap().clone();
    assert_eq!(traces.last().unwrap().1.status, "done");
    assert!(traces.iter().all(|(session, trace, _)| session == "run-resume" && trace.id == format!("mcp-task:acme:{resumed}")));
    let _ = std::fs::remove_file(&store);
}

#[test]
fn a_401_refreshes_the_credentials_once_and_retries() {
    let (fake, url) = serve_fake("old");
    let (hub, host) = hub_with_host();
    let run = id("run-401");
    hub.mount_for(&run, "acme", "srv-acme", upstream(&url, "old")).unwrap();
    let client = connect(&hub, &hub.grant(&run, "EXP-1", write_grant(&["acme"])));
    client.list_tools(T).unwrap();
    // The token expires; the host resolves a fresh one.
    *fake.0.token.lock().unwrap() = "new".into();
    *host.fresh.lock().unwrap() = Some(upstream(&url, "new"));
    let result = client.call("acme__echo", json!({}), T).unwrap();
    assert_eq!(text(&result), "echo", "{result}");
    assert_eq!(*host.refreshes.lock().unwrap(), 1);
    // It expires again and cannot be refreshed: sign in again.
    *fake.0.token.lock().unwrap() = "newer".into();
    let result = client.call("acme__echo", json!({}), T).unwrap();
    assert_eq!(result["isError"], json!(true));
    assert_eq!(text(&result), hub_client::copy::sign_in("acme"));
    assert_eq!(*host.refreshes.lock().unwrap(), 2);
}

#[test]
fn skills_are_listed_read_and_verified() {
    let (_fake, url) = serve_fake("k1");
    let (hub, _host) = hub_with_host();
    let run = id("run-skills");
    hub.mount_for(&run, "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    let (skills, warnings) = hub.skills(&run, "acme").unwrap();
    assert_eq!(skills.len(), 1, "{skills:?}");
    let skill = &skills[0];
    assert_eq!((skill.uri.as_str(), skill.name.as_str(), skill.description.as_str()), ("skill://code-review/SKILL.md", "code-review", "Review a diff"));
    assert_eq!(
        skill.files,
        vec![("SKILL.md".to_string(), SKILL_MD.as_bytes().to_vec()), ("references/checklist.md".to_string(), CHECKLIST.as_bytes().to_vec())]
    );
    let rows = [
        ("skill://code-review/SKILL.md", sha256_digest(SKILL_MD.as_bytes()), SKILL_MD.len() as u64),
        ("skill://code-review/references/checklist.md", sha256_digest(CHECKLIST.as_bytes()), CHECKLIST.len() as u64),
    ];
    let rows: Vec<(&str, &str, u64)> = rows.iter().map(|(uri, digest, size)| (*uri, digest.as_str(), *size)).collect();
    assert_eq!(skill.digest_set, hub_client::digest_set(&rows));
    // The tampered file drops its whole skill, with a launch warning.
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("skill://tampered/SKILL.md") && warnings[0].contains("SKILL.md: size mismatch"), "{}", warnings[0]);
    // Another run has no such mount.
    assert!(hub.skills(&id("run-nobody"), "acme").is_err());
}

struct Local;

impl ToolProvider for Local {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![json!({ "name": "local_tool", "inputSchema": { "type": "object" } })])
    }

    fn call(&self, _caller: &Caller, _name: &str, _arguments: Value) -> Value {
        json!({ "content": [{ "type": "text", "text": "local" }] })
    }
}

#[test]
fn a_run_scoped_mount_is_its_runs_alone_and_goes_with_its_grant() {
    let (_fake, url) = serve_fake("k1");
    let (hub, _host) = hub_with_host();
    // A global mount of the same name: what every other run sees.
    hub.mount("acme", Mount::Provider(Arc::new(Local))).unwrap();
    let (a, b) = (id("run-a"), id("run-b"));
    hub.mount_for(&a, "acme", "srv-acme", upstream(&url, "k1")).unwrap();
    let client_a = connect(&hub, &hub.grant(&a, "EXP-1", write_grant(&["acme"])));
    let client_b = connect(&hub, &hub.grant(&b, "EXP-2", write_grant(&["acme"])));
    let names = |client: &Client| -> Vec<String> {
        client.list_tools(T).unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(names(&client_a), ["acme__ask", "acme__slow", "acme__ask_task", "acme__forever", "acme__echo"]);
    assert_eq!(names(&client_b), ["acme__local_tool"]);
    assert_eq!(text(&client_a.call("acme__echo", json!({}), T).unwrap()), "echo");
    assert!(client_b.call("acme__echo", json!({}), T).is_err());
    // Without a global mount, another run sees nothing of it.
    hub.unmount("acme");
    assert!(names(&client_b).is_empty());
    hub.revoke(&a);
    let client_a = connect(&hub, &hub.grant(&a, "EXP-1", write_grant(&["acme"])));
    assert!(names(&client_a).is_empty());
}
