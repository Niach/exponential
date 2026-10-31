//! The `screen` MCP endpoint, served by the `emcp` crate: Streamable HTTP
//! on loopback, one listener per process, each run admitted by its own
//! bearer token. The one tool is [`crate::tool`]'s.

use std::sync::Arc;

use serde_json::Value;

/// Answers a `tools/call` (name, arguments) as an MCP call result.
pub(crate) type Call = Arc<dyn Fn(&str, &Value) -> Value + Send + Sync>;

struct Provider {
    call: Call,
}

impl emcp::ToolProvider for Provider {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(crate::tool::tools().as_array().cloned().unwrap_or_default())
    }

    fn call(&self, _caller: &emcp::Caller, name: &str, arguments: Value) -> Value {
        (self.call)(name, &arguments)
    }
}

/// Bind loopback and serve `call`; grants and revokes go through the
/// returned server.
pub(crate) fn serve(call: Call) -> Result<emcp::Server, String> {
    let info = emcp::ServerInfo { name: crate::SERVER_NAME.into(), version: env!("CARGO_PKG_VERSION").into(), instructions: None };
    emcp::Server::serve(info, Arc::new(Provider { call }))
        .map_err(|err| format!("Could not start the screen-memory server: {err}"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    pub(crate) fn client(url: &str, token: &str) -> Result<emcp::Client, emcp::ClientError> {
        let upstream = emcp::Upstream {
            name: "screen".into(),
            url: url.to_string(),
            headers: vec![("Authorization".into(), emcp::ecore::bearer_header(token))],
        };
        emcp::Client::connect(&upstream, "test", Duration::from_secs(10))
    }

    fn id(session_id: &str) -> emcp::SessionId {
        emcp::SessionId::new(session_id).unwrap()
    }

    #[test]
    fn the_endpoint_lists_the_one_tool_and_wants_the_runs_token() {
        let server = serve(Arc::new(|name: &str, _: &Value| json!({ "content": [{ "type": "text", "text": name }] }))).unwrap();
        let old = server.grant(&id("s1"), "s1");
        let token = server.grant(&id("s1"), "s1");
        assert!(matches!(client(server.url(), "nope"), Err(emcp::ClientError::Http(401))));
        assert!(matches!(client(server.url(), old.as_str()), Err(emcp::ClientError::Http(401))));
        let live = client(server.url(), token.as_str()).unwrap();
        let names: Vec<String> =
            live.list_tools(Duration::from_secs(10)).unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["get_screenshots"]);
        let result = live.call("get_screenshots", json!({}), Duration::from_secs(10)).unwrap();
        assert_eq!(result["content"][0]["text"], "get_screenshots");
        server.revoke(&id("s1"));
        assert!(matches!(client(server.url(), token.as_str()), Err(emcp::ClientError::Http(401))));
    }
}
