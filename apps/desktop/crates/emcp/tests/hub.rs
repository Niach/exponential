//! The local hub over real sockets (#EXP-1507): hub.json's list and call
//! cases replayed through `tools/list` and `tools/call` on the loopback
//! endpoint, the fixture's servers mounted as in-process providers and as
//! upstreams, and every call's `mcp_calls` row read back from the sink.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use emcp::contract::fixture;
use emcp::hub::{AuditRow, Hub, HubGrant, JsonlAudit, MemoryAudit, Mount};
use emcp::{Caller, Client, ClientError, Refusal, Server, ServerInfo, ToolProvider, Upstream};
use serde_json::{json, Value};

const T: Duration = Duration::from_secs(10);

/// One of the fixture's servers: its tools, answering as `mode` says
/// (`ok`, `isError`, or `invalidParams` = refuse as -32602).
struct FixtureServer {
    tools: Vec<Value>,
    mode: Mutex<String>,
    seen: Mutex<Vec<(String, String, Value)>>,
}

impl FixtureServer {
    fn new(name: &str) -> Arc<Self> {
        let server = fixture::hub()["servers"].as_array().unwrap().iter().find(|server| server["name"] == name).unwrap();
        let tools = server["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| {
                let mut tool = tool.clone();
                tool["description"] = json!(format!("The {} tool.", tool["name"].as_str().unwrap()));
                tool["inputSchema"] = json!({ "type": "object" });
                tool
            })
            .collect();
        Arc::new(Self { tools, mode: Mutex::new("ok".into()), seen: Mutex::default() })
    }
}

impl ToolProvider for FixtureServer {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(self.tools.clone())
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        self.seen.lock().unwrap().push((caller.session_id.clone(), name.to_string(), arguments.clone()));
        let failed = *self.mode.lock().unwrap() == "isError";
        json!({ "content": [{ "type": "text", "text": format!("{name} answered") }], "isError": failed })
    }

    fn try_call(&self, caller: &Caller, name: &str, arguments: Value) -> Result<Value, Refusal> {
        if *self.mode.lock().unwrap() == "invalidParams" {
            return Err(Refusal::UnknownTool);
        }
        Ok(self.call(caller, name, arguments))
    }
}

fn info(name: &str) -> ServerInfo {
    ServerInfo { name: name.into(), version: "1.0.0".into(), instructions: None }
}

fn grant(value: &Value) -> HubGrant {
    serde_json::from_value(value.clone()).unwrap()
}

fn connect(hub: &Hub, token: &str) -> Client {
    let upstream = Upstream { name: "hub".into(), url: hub.url().into(), headers: vec![("Authorization".into(), format!("Bearer {token}"))] };
    Client::connect(&upstream, "hub-test", T).unwrap()
}

const NAMES: [&str; 3] = ["files", "computer", "linear"];

#[test]
fn the_list_cases_through_tools_list() {
    let hub = Hub::serve(info("hub"), Arc::new(MemoryAudit::default())).unwrap();
    for name in NAMES {
        hub.mount(name, Mount::Provider(FixtureServer::new(name))).unwrap();
    }
    for (i, case) in fixture::hub()["listCases"].as_array().unwrap().iter().enumerate() {
        let token = hub.grant(&format!("run-{i}"), "list", grant(&case["grant"]));
        let tools = connect(&hub, &token).list_tools(T).unwrap();
        let names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
        let expected: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|name| name.as_str().unwrap()).collect();
        assert_eq!(names, expected, "{}", case["name"]);
        // The tool object is the server's, renamed.
        for tool in &tools {
            assert!(tool["description"].as_str().unwrap().starts_with("The "), "{tool}");
        }
    }
}

/// Mounts the fixture's servers for one call case: providers, except that
/// the routed server is an upstream (a real emcp server) when the case
/// needs a protocol answer (`invalidParams`) or a dead connection
/// (`unreachable`).
fn mount_for(hub: &Hub, case: &Value) -> (Vec<Arc<FixtureServer>>, Option<(Server, Arc<FixtureServer>)>) {
    let upstream = case["upstream"].as_str().unwrap_or("ok");
    let routed = case["expected"]["routed"]["server"].as_str();
    let mut providers = Vec::new();
    let mut remote = None;
    for name in NAMES {
        let server = FixtureServer::new(name);
        *server.mode.lock().unwrap() = upstream.to_string();
        if Some(name) == routed && matches!(upstream, "invalidParams" | "unreachable") {
            let served = Server::serve(info(name), server.clone()).unwrap();
            let token = served.grant("hub", "hub");
            let url = served.url().to_string();
            hub.mount(name, Mount::Upstream(Upstream { name: name.into(), url, headers: vec![("Authorization".into(), format!("Bearer {token}"))] }))
                .unwrap();
            remote = Some((served, server));
        } else {
            hub.mount(name, Mount::Provider(server.clone())).unwrap();
            providers.push(server);
        }
    }
    (providers, remote)
}

#[test]
fn the_call_cases_through_tools_call_with_their_audit_rows() {
    let audit = Arc::new(MemoryAudit::default());
    let hub = Hub::serve(info("hub"), audit.clone()).unwrap();
    let mut replayed = 0;
    for case in fixture::hub()["callCases"].as_array().unwrap() {
        // rpcError and timeout have no loopback stand-in; the pure replay
        // (tests/fixtures.rs) covers them.
        if matches!(case["upstream"].as_str(), Some("rpcError" | "timeout")) {
            continue;
        }
        let (_providers, remote) = mount_for(&hub, case);
        let token = hub.grant(s(&case["run"]), "call", grant(&case["grant"]));
        let client = connect(&hub, &token);
        // The hub lists (and caches) every server before the call.
        client.list_tools(T).unwrap();
        if case["upstream"] == "unreachable" {
            // The upstream goes away after it was listed.
            remote.as_ref().unwrap().0.revoke_all();
        }
        let before = audit.rows().len();
        let args = if case["call"]["arguments"].is_null() { json!({}) } else { case["call"]["arguments"].clone() };
        let answered = client.call(s(&case["call"]["name"]), args.clone(), T);
        match case["expected"]["routed"].is_null() {
            true => assert!(matches!(answered, Err(ClientError::Rpc { code: -32602, .. })), "{}: {answered:?}", case["name"]),
            false => {
                let result = answered.unwrap_or_else(|err| panic!("{}: {err}", case["name"]));
                let want_error = !matches!(case["upstream"].as_str(), Some("ok"));
                assert_eq!(result["isError"].as_bool().unwrap_or(false), want_error, "{}", case["name"]);
            }
        }
        let rows = audit.rows();
        assert_eq!(rows.len(), before + 1, "{}: one row per call", case["name"]);
        check_row(&rows[before], &case["expected"]["row"], &args, s(&case["name"]));
        replayed += 1;
        for name in NAMES {
            hub.unmount(name);
        }
        hub.revoke(s(&case["run"]));
    }
    assert!(replayed >= 7, "replayed {replayed} cases");
}

fn s(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn check_row(row: &AuditRow, want: &Value, args: &Value, name: &str) {
    assert_eq!(row.team_id.as_deref(), want["teamId"].as_str(), "{name}");
    assert_eq!(row.user_id.as_deref(), want["userId"].as_str(), "{name}");
    assert_eq!(row.client_id, s(&want["clientId"]), "{name}");
    assert_eq!(row.app_id, None, "{name}");
    assert_eq!(row.tool, s(&want["tool"]), "{name}");
    assert_eq!(row.status.as_str(), s(&want["status"]), "{name}");
    assert_eq!(row.error, want["error"].as_str(), "{name}");
    // The hub hashes the arguments as received (`{}` for none).
    assert_eq!(row.args_hash, emcp::contract::audit::args_hash(Some(args)), "{name}");
    assert_eq!(row.args_hash, s(&want["argsHash"]), "{name}");
    assert!(emcp::contract::audit::parse_utc_ms(&row.created_at).is_some(), "{name}: {}", row.created_at);
    assert!(uuid_like(&row.id), "{name}: {}", row.id);
}

fn uuid_like(id: &str) -> bool {
    id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4
}

#[test]
fn a_provider_sees_the_run_and_a_revoked_run_is_refused() {
    let hub = Hub::serve(info("hub"), Arc::new(MemoryAudit::default())).unwrap();
    let files = FixtureServer::new("files");
    hub.mount("files", Mount::Provider(files.clone())).unwrap();
    let token = hub.grant("run-7", "EXP-7", grant(&json!({ "servers": { "files": "read" }, "teamId": null, "userId": null })));
    let client = connect(&hub, &token);
    client.call("files__read_file", json!({ "path": "/a" }), T).unwrap();
    assert_eq!(files.seen.lock().unwrap()[0], ("run-7".to_string(), "read_file".to_string(), json!({ "path": "/a" })));
    hub.revoke("run-7");
    assert!(matches!(client.call("files__read_file", json!({}), T), Err(ClientError::Http(401) | ClientError::Transport(_))));
}

#[test]
fn a_new_grant_replaces_the_old_one_and_names_are_checked() {
    let hub = Hub::serve(info("hub"), Arc::new(MemoryAudit::default())).unwrap();
    assert!(hub.mount("My_Server", Mount::Provider(FixtureServer::new("files"))).is_err());
    hub.mount("files", Mount::Provider(FixtureServer::new("files"))).unwrap();
    assert_eq!(hub.mounted(), vec!["files".to_string()]);
    let old = hub.grant("run-1", "a", grant(&json!({ "servers": { "files": "write" }, "teamId": null, "userId": null })));
    let new = hub.grant("run-1", "b", grant(&json!({ "servers": { "files": "read" }, "teamId": null, "userId": null })));
    assert!(Client::connect(
        &Upstream { name: "hub".into(), url: hub.url().into(), headers: vec![("Authorization".into(), format!("Bearer {old}"))] },
        "t",
        T
    )
    .is_err());
    let names: Vec<String> =
        connect(&hub, &new).list_tools(T).unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["files__read_file".to_string()]);
}

#[test]
fn the_jsonl_sink_appends_rows_by_column() {
    let path = std::env::temp_dir().join(format!("emcp-hub-{}.jsonl", uuid::Uuid::new_v4()));
    let hub = Hub::serve(info("hub"), Arc::new(JsonlAudit::new(&path))).unwrap();
    hub.mount("files", Mount::Provider(FixtureServer::new("files"))).unwrap();
    let token = hub.grant("run-1", "a", grant(&json!({ "servers": { "files": "read" }, "teamId": "t1", "userId": "u1" })));
    let client = connect(&hub, &token);
    client.call("files__read_file", json!({ "path": "/a" }), T).unwrap();
    let _ = client.call("files__write_file", json!({}), T);
    let text = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let _ = std::fs::remove_file(&path);
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0]["status"].as_str(), rows[0]["team_id"].as_str(), rows[0]["client_id"].as_str()), (Some("ok"), Some("t1"), Some("run-1")));
    assert_eq!((rows[1]["status"].as_str(), rows[1]["error"].as_str()), (Some("denied"), Some("unknownTool")));
}
