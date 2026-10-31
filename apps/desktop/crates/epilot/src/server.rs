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
reply that you are taking the screen before the first action.";

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
