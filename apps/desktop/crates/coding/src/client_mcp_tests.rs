use super::*;
use crate::mcp_gateway::OUTSIDE_SESSION_TOOL;
use crate::sealing::{self, test_support::FakeService};
use std::sync::atomic::Ordering;

type Calls = Arc<Mutex<Vec<(String, Value, Vec<(String, String)>)>>>;

/// A fake instance: `/api/mcp` records every tools/call with its headers;
/// tRPC `sessions.sendTarget` answers `target`.
fn instance(target: Value) -> (String, Calls) {
    let calls: Calls = Arc::default();
    let record = calls.clone();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let json_header = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
            if request.url().starts_with("/api/trpc/") {
                let _ = request.respond(tiny_http::Response::from_string(json!({ "result": { "data": target } }).to_string()).with_header(json_header));
                continue;
            }
            let Ok(message) = serde_json::from_str::<Value>(&body) else {
                let _ = request.respond(tiny_http::Response::empty(405));
                continue;
            };
            let Some(id) = message.get("id").cloned() else {
                let _ = request.respond(tiny_http::Response::empty(202));
                continue;
            };
            let reply = if message["method"] == "tools/call" {
                let headers = request.headers().iter().map(|h| (h.field.to_string().to_lowercase(), h.value.to_string())).collect();
                let tool = message["params"]["name"].as_str().unwrap().to_string();
                let args = message["params"]["arguments"].clone();
                record.lock().unwrap().push((tool, args.clone(), headers));
                // A read echoes a sealed body back, to be opened here.
                let text = json!({ "sessionId": "conv-1", "rootId": "conv-1", "messageId": "m", "body": args["text"] }).to_string();
                json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": text }] } })
            } else {
                json!({ "jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": "2025-06-18", "capabilities": { "tools": {} }, "serverInfo": { "name": "fake", "version": "1" },
                } })
            };
            let _ = request.respond(tiny_http::Response::from_string(reply.to_string()).with_header(json_header));
        }
    });
    (format!("http://127.0.0.1:{port}"), calls)
}

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("client-mcp-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("accounts/acc")).unwrap();
    dir
}

fn serve(dir: &Path, url: &str) -> ClientMcp {
    let trpc = Arc::new(TrpcClient::new(url, Arc::new(api::StaticToken("account-token".into()))));
    ClientMcp::start(endpoint_path(&dir.join("accounts/acc/seal-keys"), false), false, trpc)
}

fn send_line(id: u64, args: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": "exponential_sessions_send", "arguments": args } }).to_string()
}

/// (is an error, text) of the one answer to `line`.
fn answer(bridge: &Bridge, line: &str) -> (bool, String) {
    let answers = bridge.forward(line);
    assert_eq!(answers.len(), 1, "{answers:?}");
    let reply: Value = serde_json::from_str(&answers[0]).unwrap();
    (reply["result"]["isError"] == true, reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string())
}

fn service(tree: &sealing::TreeKey) -> Arc<FakeService> {
    let install = enet_core::PushKeypair::generate();
    let fake = Arc::new(FakeService { install_public: Some(install.public()), ..Default::default() });
    fake.keyring.insert_secret("conv-1", 1, tree.secret);
    sealing::install(fake.clone());
    fake
}

#[test]
fn a_local_clients_message_to_an_agent_leaves_sealed_and_comes_back_opened() {
    let _serial = crate::sealing::test_support::serial();
    let tree = sealing::generate_tree_key();
    service(&tree);
    let (url, calls) = instance(json!({ "rootId": "conv-1", "epoch": 1, "publicKey": tree.public_key(), "brief": null }));
    let dir = data_dir("sealed");
    let held = serve(&dir, &url);
    let bridge = Bridge::new(dir.clone());

    assert!(bridge.forward(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }).to_string())[0].contains("2025-06-18"));
    assert!(bridge.forward(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string()).is_empty());
    let (failed, text) = answer(&bridge, &send_line(1, json!({ "agentId": "a1", "text": "geheim: bitte mergen" })));
    assert!(!failed, "{text}");
    assert!(text.contains("geheim: bitte mergen"), "opened on the way back: {text}");
    let (_, sent, headers) = calls.lock().unwrap().pop().unwrap();
    assert!(sent["text"].as_str().unwrap().starts_with("exp-sealed:v1:1:"), "{sent}");
    assert!(!sent.to_string().contains("geheim"));
    let opened = enet_core::sealed::open_body(sent["text"].as_str().unwrap(), &|epoch| (epoch == 1).then_some(tree.secret));
    assert_eq!(opened.text.as_deref(), Some("geheim: bitte mergen"));
    // The person, not a session; the loopback token never leaves.
    assert!(headers.iter().any(|(name, value)| name == "authorization" && value == "Bearer account-token"));
    assert!(!headers.iter().any(|(name, _)| name == "x-exp-session-id"));

    // The CLI's way in: the same grant.
    let sent = call_tool(&dir, "acc", "exponential_sessions_send", &json!({ "agentId": "a1", "text": "noch etwas" })).unwrap().unwrap();
    assert_eq!(sent["sessionId"], "conv-1");
    assert!(!calls.lock().unwrap().pop().unwrap().1.to_string().contains("noch etwas"));
    drop(held);
    sealing::uninstall();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_install_that_waits_for_approval_seals_nothing_for_a_client() {
    let _serial = crate::sealing::test_support::serial();
    let tree = sealing::generate_tree_key();
    let fake = service(&tree);
    let (url, calls) = instance(json!({ "rootId": "conv-1", "epoch": 1, "publicKey": tree.public_key(), "brief": null }));
    let dir = data_dir("waiting");
    let held = serve(&dir, &url);
    let bridge = Bridge::new(dir.clone());
    for (waits, code) in [(true, None), (false, Some("ABCDE FGHJK")), (true, Some("ABCDE FGHJK"))] {
        fake.waits.store(waits, Ordering::SeqCst);
        sealing::set_install_waiting(code.map(str::to_string));
        // Every way content leaves, an agent's chat, a session, a new root, an answer.
        for args in [
            json!({ "agentId": "a1", "text": "x" }),
            json!({ "sessionId": "conv-1", "text": "x" }),
            json!({ "deviceId": "d1", "text": "x", "name": "n" }),
            json!({ "sessionId": "conv-1", "answer": { "dismissed": true } }),
        ] {
            let (failed, text) = answer(&bridge, &send_line(1, args));
            assert!(failed && text.starts_with("This device waits for approval"), "{text}");
            assert!(code.is_none_or(|code| text.contains(code)), "{text}");
        }
        let end = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "exponential_sessions_write", "arguments": { "op": "end", "id": "conv-1", "summary": "x" } } });
        assert!(answer(&bridge, &end.to_string()).0);
        let cli = call_tool(&dir, "acc", "exponential_sessions_send", &json!({ "agentId": "a1", "text": "x" })).unwrap();
        assert!(cli.is_err());
    }
    assert!(calls.lock().unwrap().is_empty(), "nothing reached the instance");
    assert!(fake.created.lock().unwrap().is_empty());
    // Approved: the same client sends at once.
    fake.waits.store(false, Ordering::SeqCst);
    sealing::set_install_waiting(None);
    assert!(!answer(&bridge, &send_line(3, json!({ "agentId": "a1", "text": "x" }))).0);
    drop(held);
    sealing::uninstall();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_sessions_own_tools_and_a_missing_holder_are_refused() {
    let _serial = crate::sealing::test_support::serial();
    let tree = sealing::generate_tree_key();
    service(&tree);
    let (url, calls) = instance(json!({ "rootId": "conv-1", "epoch": 1, "publicKey": tree.public_key(), "brief": null }));
    let dir = data_dir("refused");
    let held = serve(&dir, &url);
    let bridge = Bridge::new(dir.clone());
    for (tool, args) in [
        ("exponential_show", json!({ "askText": "x" })),
        ("exponential_session_self", json!({ "op": "suggest", "replies": ["x"] })),
        ("exponential_sessions_write", json!({ "op": "escalate", "messageId": "m" })),
    ] {
        let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": args } });
        assert_eq!(answer(&bridge, &line.to_string()), (true, OUTSIDE_SESSION_TOOL.to_string()));
    }
    // The endpoint is up but its key holder is gone: nothing leaves in clear.
    sealing::uninstall();
    let (failed, text) = answer(&bridge, &send_line(2, json!({ "agentId": "a1", "text": "x" })));
    assert!(failed && text.contains("Nothing left this device"), "{text}");
    assert!(calls.lock().unwrap().is_empty());
    // No holder at all.
    drop(held);
    assert_eq!(answer(&bridge, &send_line(3, json!({ "agentId": "a1", "text": "x" }))), (true, NO_HOLDER.to_string()));
    assert!(call_tool(&dir, "acc", "exponential_sessions_send", &json!({})).is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_endpoint_file_is_private_current_and_gone_with_its_holder() {
    let _serial = crate::sealing::test_support::serial();
    let tree = sealing::generate_tree_key();
    service(&tree);
    let (url, _) = instance(json!({ "rootId": "conv-1", "epoch": 1, "publicKey": tree.public_key(), "brief": null }));
    let dir = data_dir("file");
    let path = dir.join("accounts/acc").join(DESKTOP_FILE);
    let held = serve(&dir, &url);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("account-token"));
    let first = read_endpoint(&path).unwrap();
    let bridge = Bridge::new(dir.clone());
    assert!(!answer(&bridge, &send_line(1, json!({ "agentId": "a1", "text": "x" }))).0);

    // A restarted holder: a new token, the same bridge goes on.
    drop(held);
    assert!(!path.exists(), "removed with its holder");
    let held = serve(&dir, &url);
    assert_ne!(read_endpoint(&path).unwrap().token, first.token);
    assert!(!answer(&bridge, &send_line(2, json!({ "agentId": "a1", "text": "x" }))).0);
    // The old token is dead.
    assert_eq!(bridge.post(&first, &send_line(3, json!({ "agentId": "a1", "text": "x" }))).unwrap().status().as_u16(), 401);

    // A file another process wrote meanwhile is left alone.
    let other = Endpoint { token: "other".into(), ..first.clone() };
    std::fs::write(&path, serde_json::to_string(&other).unwrap()).unwrap();
    drop(held);
    assert!(path.exists());
    // A dead process's file, or an address off this machine, is never used.
    for endpoint in [Endpoint { pid: 0x7fff_fff0, ..first.clone() }, Endpoint { url: "http://10.0.0.1:1/x".into(), ..first }] {
        std::fs::write(&path, serde_json::to_string(&endpoint).unwrap()).unwrap();
        assert!(call_tool(&dir, "acc", "exponential_sessions_send", &json!({})).is_none());
    }
    sealing::uninstall();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_sse_answer_becomes_one_line_per_message() {
    let body = "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\r\ndata: \"id\":1}\r\n\r\ndata: {\"id\":2}\n\n";
    assert_eq!(messages_of("text/event-stream", body), vec![r#"{"jsonrpc":"2.0","id":1}"#, r#"{"id":2}"#]);
    assert!(messages_of("application/json", "").is_empty());
}
