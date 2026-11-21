//! The server and the client against each other on loopback: the bearer
//! gate, the provider seeing its run, a nested client call from inside a
//! provider, a raw route, and the client's error mapping.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use emcp::{
    Caller, Client, ClientError, DeliveryVerdict, RawRequest, RawResponse, RawRoute, Server, ServerInfo, SessionId, ToolProvider,
    Upstream,
};
use serde_json::{json, Value};

const T: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<String>>,
    ended: Mutex<Vec<String>>,
    /// Where `relay` forwards to (a nested client call).
    relay: Mutex<Option<Upstream>>,
}

impl ToolProvider for Fake {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![
            json!({ "name": "echo", "description": "Echo.", "inputSchema": { "type": "object" } }),
            json!({ "name": "bare" }),
        ])
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        self.calls.lock().unwrap().push(format!("{}/{} {name} {arguments}", caller.session_id, caller.label));
        match name {
            "relay" => {
                let upstream = self.relay.lock().unwrap().clone().unwrap();
                let inner = Client::connect(&upstream, "relay", T).unwrap().call("echo", json!({ "nested": true }), T).unwrap();
                json!({ "content": [{ "type": "text", "text": "relayed" }], "structuredContent": inner["structuredContent"] })
            }
            "refuse" => json!({ "content": [{ "type": "text", "text": "no" }], "isError": true }),
            _ => json!({
                "content": [{ "type": "text", "text": format!("{name} done") }],
                "structuredContent": { "args": arguments },
                "_meta": { "exponential/calls": [1, 2] },
            }),
        }
    }

    fn ended(&self, session_id: &SessionId) {
        self.ended.lock().unwrap().push(session_id.to_string());
    }
}

fn id(session_id: &str) -> SessionId {
    SessionId::new(session_id).unwrap()
}

fn info() -> ServerInfo {
    ServerInfo { name: "fake".into(), version: "1.0.0".into(), instructions: Some("Be kind.".into()) }
}

fn upstream(url: &str, token: &str) -> Upstream {
    Upstream {
        name: "fake".into(),
        url: url.to_string(),
        headers: vec![("Authorization".into(), emcp::ecore::bearer_header(token)), ("X-Exp-Session-Id".into(), "run-1".into())],
    }
}

#[test]
fn tools_and_calls_reach_the_run() {
    let fake = Arc::new(Fake::default());
    let server = Server::serve(info(), fake.clone()).unwrap();
    let token = server.grant(&id("s1"), "EXP-1");
    let client = Client::connect(&upstream(server.url(), token.as_str()), "test", T).unwrap();
    let tools = client.list_tools(T).unwrap();
    let names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["echo", "bare"]);
    assert_eq!(tools[1]["inputSchema"], json!({ "type": "object" }));
    let result = client.call("echo", json!({ "x": 1 }), T).unwrap();
    assert_eq!(result["structuredContent"], json!({ "args": { "x": 1 } }));
    assert_eq!(result["_meta"]["exponential/calls"], json!([1, 2]));
    let refused = client.call("refuse", json!({}), T).unwrap();
    assert_eq!(refused["isError"], json!(true));
    assert_eq!(fake.calls.lock().unwrap()[0], r#"s1/EXP-1 echo {"x":1}"#);
}

/// A raw `initialize` (the client keeps no instructions): the result.
fn initialize(url: &str, token: &str) -> Value {
    use std::io::{Read, Write};
    let (host, path) = url.strip_prefix("http://").unwrap().split_once('/').unwrap();
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "raw", "version": "1" } } }).to_string();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    stream.set_read_timeout(Some(T)).unwrap();
    write!(stream, "POST /{path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let json = &answer[answer.find("{\"jsonrpc\"").unwrap_or_else(|| panic!("no JSON-RPC answer in {answer}"))..];
    serde_json::from_str::<Value>(json[..=json.rfind('}').unwrap()].trim()).unwrap()["result"].clone()
}

/// One raw JSON-RPC request on `version` (MCP 2026-07-28 carries it in the
/// request's `_meta` and the headers; an older peer in the header only):
/// the whole answer.
fn rpc(url: &str, token: &str, version: &str, method: &str, mut params: Value) -> Value {
    use std::io::{Read, Write};
    let mut headers = format!("MCP-Protocol-Version: {version}\r\n");
    if version >= "2026-07-28" {
        params["_meta"] = json!({ "io.modelcontextprotocol/protocolVersion": version, "io.modelcontextprotocol/clientCapabilities": {} });
        headers += &format!("Mcp-Method: {method}\r\n");
        if let Some(name) = params["name"].as_str() {
            headers += &format!("Mcp-Name: {name}\r\n");
        }
    }
    let (host, path) = url.strip_prefix("http://").unwrap().split_once('/').unwrap();
    let body = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params }).to_string();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    stream.set_read_timeout(Some(T)).unwrap();
    write!(stream, "POST /{path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\n{headers}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let body = answer.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or_default();
    let json = &body[body.find('{').unwrap_or_else(|| panic!("no JSON-RPC answer in {answer}"))..];
    serde_json::from_str::<Value>(json[..=json.rfind('}').unwrap()].trim()).unwrap()
}

/// MCP 2026-07-28: every result says `resultType` (Claude Code drops a
/// server whose `tools/list` lacks it, and with it every tool); a peer on
/// an older revision reads the old shape. So does the answer to a run whose
/// grant ended.
#[test]
fn a_2026_07_28_peer_reads_the_result_type_an_older_one_the_old_shape() {
    let server = Server::serve(info(), Arc::new(Fake::default())).unwrap();
    for (version, said) in [("2026-07-28", Some("complete")), ("2025-11-25", None), ("2025-06-18", None)] {
        let token = server.grant(&id("s1"), "EXP-1");
        let listed = rpc(server.url(), token.as_str(), version, "tools/list", json!({}));
        assert_eq!(listed["result"]["tools"][0]["name"], "echo", "{version}: {listed}");
        assert_eq!(listed["result"]["resultType"].as_str(), said, "{version}: {listed}");
        let called = rpc(server.url(), token.as_str(), version, "tools/call", json!({ "name": "echo", "arguments": {} }));
        assert_eq!(called["result"]["structuredContent"], json!({ "args": {} }), "{version}: {called}");
        assert_eq!(called["result"]["resultType"].as_str(), said, "{version}: {called}");
        server.revoke(&id("s1"));
        let ended = rpc(server.url(), token.as_str(), version, "tools/call", json!({ "name": "echo", "arguments": {} }));
        assert_eq!(ended["result"]["isError"], json!(true), "{version}: {ended}");
        assert_eq!(ended["result"]["resultType"].as_str(), said, "{version}: {ended}");
    }
}

/// #EDE-126: a provider may give ONE run its own `initialize` text (a run
/// in an agent space is told another story than one on the person's
/// screen); every other run reads the server's, byte for byte.
#[test]
fn a_run_reads_its_own_instructions_or_the_servers() {
    struct PerRun;
    impl ToolProvider for PerRun {
        fn tools(&self) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }
        fn call(&self, _caller: &Caller, _name: &str, _arguments: Value) -> Value {
            json!({})
        }
        fn instructions_for(&self, caller: &Caller) -> Option<String> {
            (caller.session_id.as_str() == "s2").then(|| format!("Only for {}.", caller.label))
        }
    }
    let server = Server::serve(info(), Arc::new(PerRun)).unwrap();
    let (one, two) = (server.grant(&id("s1"), "EXP-1"), server.grant(&id("s2"), "EXP-2"));
    let plain = initialize(server.url(), one.as_str());
    assert_eq!(plain["instructions"], "Be kind.", "{plain}");
    assert_eq!(plain["serverInfo"]["name"], "fake");
    assert_eq!(initialize(server.url(), two.as_str())["instructions"], "Only for EXP-2.");
    // The default hook: the server's text for everyone.
    let server = Server::serve(info(), Arc::new(Fake::default())).unwrap();
    let token = server.grant(&id("s2"), "EXP-2");
    assert_eq!(initialize(server.url(), token.as_str())["instructions"], "Be kind.");
}

#[test]
fn the_endpoint_wants_a_live_token() {
    let fake = Arc::new(Fake::default());
    let server = Server::serve(info(), fake.clone()).unwrap();
    assert!(matches!(Client::connect(&upstream(server.url(), "nope"), "test", T), Err(ClientError::Http(401))));
    let old = server.grant(&id("s1"), "EXP-1");
    let new = server.grant(&id("s1"), "EXP-1");
    assert!(server.admitted(&old).is_none());
    assert!(Client::connect(&upstream(server.url(), new.as_str()), "test", T).is_ok());
    server.revoke(&id("s1"));
    assert!(server.admitted(&new).is_none());
    assert_eq!(fake.ended.lock().unwrap().as_slice(), ["s1"]);
    assert!(matches!(Client::connect(&upstream(server.url(), new.as_str()), "test", T), Err(ClientError::Http(401))));
}

/// A run whose grant ended reads why on its next call (a bare 401 reads as
/// "sign in" to an MCP client); the call never reaches the provider. A token
/// that was never granted stays a 401, and so does a new handshake.
#[test]
fn a_revoked_run_reads_why_its_tools_stopped() {
    let fake = Arc::new(Fake::default());
    let server = Server::serve(info(), fake.clone()).unwrap();
    for all in [false, true] {
        let token = server.grant(&id("s1"), "EXP-1");
        let client = Client::connect(&upstream(server.url(), token.as_str()), "test", T).unwrap();
        match all {
            false => server.revoke(&id("s1")),
            true => server.revoke_all(),
        }
        let refused = client.call("echo", json!({}), T).unwrap();
        assert_eq!(refused["isError"], json!(true));
        let text = refused["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fake") && text.contains("ended this session's access"), "{text}");
        assert!(matches!(client.list_tools(T), Err(ClientError::Http(401))));
        assert!(matches!(Client::connect(&upstream(server.url(), token.as_str()), "test", T), Err(ClientError::Http(401))));
    }
    assert!(fake.calls.lock().unwrap().is_empty());
}

/// A provider whose call says it began and returns when told to.
struct Slow {
    began: Mutex<std::sync::mpsc::Sender<()>>,
    finish: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl ToolProvider for Slow {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }

    fn call(&self, _caller: &Caller, _name: &str, _arguments: Value) -> Value {
        self.began.lock().unwrap().send(()).unwrap();
        self.finish.lock().unwrap().recv_timeout(T).unwrap();
        json!({ "content": [{ "type": "text", "text": "the screen" }] })
    }
}

/// #EXP-1894: a run that ended while its call ran never gets the result.
#[test]
fn a_result_for_a_run_that_ended_meanwhile_is_dropped() {
    let (began, has_begun) = std::sync::mpsc::channel();
    let (finish, finished) = std::sync::mpsc::channel();
    let server = Server::serve(info(), Arc::new(Slow { began: Mutex::new(began), finish: Mutex::new(finished) })).unwrap();
    for ends in [false, true] {
        let token = server.grant(&id("s1"), "EXP-1");
        let client = Client::connect(&upstream(server.url(), token.as_str()), "test", T).unwrap();
        let call = std::thread::spawn(move || client.call("look", json!({}), T));
        has_begun.recv_timeout(T).unwrap();
        if ends {
            server.revoke(&id("s1"));
        }
        finish.send(()).unwrap();
        let reply = call.join().unwrap();
        if ends {
            assert!(matches!(reply, Err(ClientError::Rpc { .. })), "{reply:?}");
        } else {
            assert_eq!(reply.unwrap()["content"][0]["text"], "the screen");
        }
    }
}

/// The bearer is eCore's: the scheme matched case-insensitively, anything
/// but a 64-hex token refused before a lookup, and a refused call never ran.
#[test]
fn the_bearer_is_an_ecore_token() {
    let server = Server::serve(info(), Arc::new(Fake::default())).unwrap();
    let token = server.grant(&id("s1"), "EXP-1");
    assert!(emcp::ecore::is_valid_token(token.as_str()));
    let lower = Upstream { headers: vec![("Authorization".into(), format!("bearer {}", token.as_str()))], ..upstream(server.url(), "") };
    assert!(Client::connect(&lower, "test", T).is_ok());
    let upper = token.as_str().to_uppercase();
    let refused = Client::connect(&upstream(server.url(), &upper), "test", T).err().unwrap();
    assert_eq!(refused, ClientError::Http(401));
    assert_eq!(refused.delivery(), Some(DeliveryVerdict::NotDelivered));
    assert_eq!(ClientError::Timeout.delivery(), None);
}

#[test]
fn a_provider_may_call_a_client_from_inside_a_call() {
    let fake = Arc::new(Fake::default());
    let server = Server::serve(info(), fake.clone()).unwrap();
    let inner = server.grant(&id("inner"), "Inner");
    *fake.relay.lock().unwrap() = Some(upstream(server.url(), inner.as_str()));
    let outer = server.grant(&id("outer"), "Outer");
    let result = Client::connect(&upstream(server.url(), outer.as_str()), "test", T).unwrap().call("relay", json!({}), T).unwrap();
    assert_eq!(result["structuredContent"], json!({ "args": { "nested": true } }));
}

struct Echo;

impl RawRoute for Echo {
    fn handle(&self, request: RawRequest) -> RawResponse {
        if request.bearer() != Some("raw") {
            return RawResponse::empty(401);
        }
        RawResponse {
            status: 200,
            headers: vec![("Content-Type".into(), "text/plain".into())],
            body: format!("{} {} {}", request.method, request.path, String::from_utf8(request.body.unwrap()).unwrap()).into_bytes(),
        }
    }
}

#[test]
fn a_raw_route_shares_the_listener() {
    let server = Server::serve_with(info(), Arc::new(Fake::default()), vec![("/gateway".into(), Arc::new(Echo))]).unwrap();
    let url = format!("{}/gateway/api/mcp", server.base());
    let post = |token: &str| {
        reqwest_blocking(&url, token)
    };
    assert_eq!(post("nope").0, 401);
    assert_eq!(post("raw"), (200, "POST /gateway/api/mcp hello".to_string()));
}

/// A plain HTTP/1.1 POST without a client library.
fn reqwest_blocking(url: &str, token: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let rest = url.trim_start_matches("http://");
    let (addr, path) = rest.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    write!(stream, "POST /{path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    let status = reply[9..12].parse().unwrap();
    (status, reply.split_once("\r\n\r\n").map(|(_, body)| body.to_string()).unwrap_or_default())
}

#[test]
fn a_dead_port_is_a_transport_error() {
    let dead = upstream("http://127.0.0.1:9/mcp", "x");
    assert!(matches!(Client::connect(&dead, "test", Duration::from_secs(3)), Err(ClientError::Transport(_))));
}
