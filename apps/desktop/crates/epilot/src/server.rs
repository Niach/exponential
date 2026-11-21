//! The computer-use MCP endpoint, served by the `emcp` crate: Streamable
//! HTTP on loopback, one listener per host process; each run reaches it
//! with its own bearer token ([`crate::grant`]), which is what names the
//! run. The tools are [`crate::tools`]'s: the server only routes a call to
//! the run.

use std::sync::Arc;

use serde_json::Value;

/// What the endpoint asks of the host ([`crate::host::Host`]; a fake in tests).
pub(crate) trait ToolHost: Send + Sync {
    /// The `tools/list` tools array.
    fn tools(&self) -> Result<Value, String>;
    /// One `tools/call`, answered as an MCP call result (never an error at
    /// the JSON-RPC level: a refusal is content the agent must read).
    fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value;
    /// The run ended.
    fn end(&self, session_id: &str);
    /// #EDE-126: the tools of ONE run (a space run's differ); the default
    /// is [`Self::tools`].
    fn tools_for(&self, _session_id: &str) -> Result<Value, String> {
        self.tools()
    }
    /// What ONE run reads at `initialize`; `None` = [`INSTRUCTIONS`].
    fn instructions_for(&self, _session_id: &str) -> Option<String> {
        None
    }
}

/// The config key and server name the agents see (`mcp__computer__click`).
pub const SERVER_NAME: &str = "computer";

struct Provider(Arc<dyn ToolHost>);

impl emcp::ToolProvider for Provider {
    fn tools(&self) -> Result<Vec<Value>, String> {
        self.0.tools().map(|tools| tools.as_array().cloned().unwrap_or_default())
    }

    fn call(&self, caller: &emcp::Caller, name: &str, arguments: Value) -> Value {
        self.0.call(caller.session_id.as_str(), &caller.label, name, arguments)
    }

    fn tools_for(&self, caller: &emcp::Caller) -> Result<Vec<Value>, String> {
        self.0.tools_for(caller.session_id.as_str()).map(|tools| tools.as_array().cloned().unwrap_or_default())
    }

    fn instructions_for(&self, caller: &emcp::Caller) -> Option<String> {
        self.0.instructions_for(caller.session_id.as_str())
    }

    fn ended(&self, session_id: &emcp::SessionId) {
        self.0.end(session_id.as_str());
    }
}

/// What the agent reads at `initialize`; the per-tool schemas carry the
/// rest. The one rule on top of the schemas: this is the person's own
/// screen, taken over.
const INSTRUCTIONS: &str = "This computer's desktop, in the foreground: the person's own screen, pointer and \
keyboard. While you drive, their input is locked and a banner tells them so; their Esc aborts \
every call until they speak again. Take a screenshot, act on what it shows (every coordinate is in \
the screenshot's pixels), take another to verify; an unverified effect is not success. Say in your \
reply that you are taking the screen before the first action. Enter text with paste_text (sensitive: true \
for any secret, best from a file), not type.";

/// #EDE-126: what a run in an agent space reads instead: nothing of the
/// person's is taken, held or aborted there. #EXP-1514: the person at the
/// machine can watch it (the live view).
pub(crate) const SPACE_INSTRUCTIONS: &str = "An agent space on this computer: a desktop of your own (one screen, its own pointer, keyboard and clipboard), apart from the person's. Nothing you do here reaches their screen or input, and nothing of theirs is in it. The person at this computer can watch your space. It starts empty: start programs with launch_app. Take a screenshot, act on what it shows (every coordinate is in the screenshot's pixels), take another to verify; an unverified effect is not success. Enter text with paste_text (sensitive: true for any secret, best from a file), not type. When a call says the space is new and empty, what ran in it is gone: start it again.";

/// Bind loopback and serve `host`; grants and revokes go through the
/// returned server.
pub(crate) fn serve(host: Arc<dyn ToolHost>) -> Result<emcp::Server, String> {
    let info = emcp::ServerInfo {
        name: SERVER_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        instructions: Some(INSTRUCTIONS.into()),
    };
    emcp::Server::serve(info, Arc::new(Provider(host))).map_err(|err| format!("Could not start the computer-use server: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;
    use std::time::Duration;

    const T: Duration = Duration::from_secs(10);

    /// A host that records what it was asked.
    #[derive(Default)]
    struct FakeHost {
        calls: Mutex<Vec<String>>,
        ended: Mutex<Vec<String>>,
    }

    impl ToolHost for FakeHost {
        fn tools(&self) -> Result<Value, String> {
            Ok(json!([{ "name": "click", "inputSchema": { "type": "object" } }, { "name": "list_windows" }]))
        }

        fn call(&self, session_id: &str, label: &str, name: &str, arguments: Value) -> Value {
            self.calls.lock().unwrap().push(format!("{session_id}/{label} {name} {arguments}"));
            json!({ "content": [{ "type": "text", "text": format!("{name} done") }], "structuredContent": { "effect": "confirmed" } })
        }

        fn end(&self, session_id: &str) {
            self.ended.lock().unwrap().push(session_id.to_string());
        }
    }

    fn id(session_id: &str) -> emcp::SessionId {
        emcp::SessionId::new(session_id).unwrap()
    }

    fn client(server: &emcp::Server, token: &str) -> Result<emcp::Client, emcp::ClientError> {
        let upstream = emcp::Upstream {
            name: SERVER_NAME.into(),
            url: server.url().to_string(),
            headers: vec![("Authorization".into(), emcp::ecore::bearer_header(token))],
        };
        emcp::Client::connect(&upstream, "test", T)
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

    /// #EDE-126: a space run's `initialize` and tools are the space's; a
    /// person run reads today's text, byte for byte, and today's tools.
    #[test]
    fn a_space_run_reads_the_space_text_and_tools_a_person_run_the_old_ones() {
        let hosts = Arc::new(crate::host::Hosts::default());
        let server = serve(hosts.clone()).unwrap();
        hosts.admit("p1", crate::backend::Target::Person);
        hosts.admit_space("s1");
        let (person, space) = (server.grant(&id("p1"), "EXP-1"), server.grant(&id("s1"), "EXP-2"));
        assert_eq!(initialize(server.url(), person.as_str())["instructions"], INSTRUCTIONS);
        let told = initialize(server.url(), space.as_str())["instructions"].as_str().unwrap().to_string();
        assert_eq!(told, SPACE_INSTRUCTIONS);
        assert!(!told.contains("locked") && !told.contains("Esc") && told.contains("launch_app"), "{told}");
        // #EXP-1514: the live view: never "they do not see it".
        assert!(told.contains("The person at this computer can watch your space.") && !told.contains("not see"), "{told}");
        let names = |token: &emcp::Token| -> Vec<String> {
            client(&server, token.as_str()).unwrap().list_tools(T).unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect()
        };
        assert_eq!(names(&person), crate::tools::names());
        assert!(!names(&person).contains(&"launch_app".to_string()) && names(&space).contains(&"launch_app".to_string()));
    }

    #[test]
    fn calls_reach_the_runs_session_with_its_label_and_the_tools_are_listed() {
        let host = Arc::new(FakeHost::default());
        let server = serve(host.clone()).unwrap();
        let (one, two) = (server.grant(&id("s1"), "EXP-1"), server.grant(&id("s2"), "Chat"));
        let first = client(&server, one.as_str()).unwrap();
        let names: Vec<String> =
            first.list_tools(T).unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["click", "list_windows"]);
        let args = json!({ "x": 10, "y": 10 });
        let reply = first.call("click", args.clone(), T).unwrap();
        assert_eq!(reply["structuredContent"]["effect"], "confirmed");
        client(&server, two.as_str()).unwrap().call("list_windows", json!({}), T).unwrap();
        assert_eq!(
            host.calls.lock().unwrap().as_slice(),
            [format!("s1/EXP-1 click {args}"), "s2/Chat list_windows {}".to_string()]
        );
    }

    #[test]
    fn a_new_grant_replaces_the_runs_old_token_and_a_revoke_ends_its_session() {
        let host = Arc::new(FakeHost::default());
        let server = serve(host.clone()).unwrap();
        let old = server.grant(&id("s1"), "EXP-1");
        let new = server.grant(&id("s1"), "EXP-1");
        assert!(matches!(client(&server, old.as_str()), Err(emcp::ClientError::Http(401))));
        assert!(client(&server, new.as_str()).is_ok());
        server.revoke(&id("s1"));
        assert!(matches!(client(&server, new.as_str()), Err(emcp::ClientError::Http(401))));
        assert_eq!(host.ended.lock().unwrap().as_slice(), ["s1"]);
    }
}
