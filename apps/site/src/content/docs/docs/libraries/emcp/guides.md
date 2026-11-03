---
title: eMCP guides
description: Depend on emcp, serve your own tools on a loopback MCP server, call any MCP server, and connect one behind OAuth.
---

## Install

One crate in the Exponential repo, taken as a git dependency on an `emcp-v<version>` tag: the contract is compiled in from the repo's fixtures, which only a checkout carries. The policy: [Install from a release tag](/docs/libraries/install/).

```toml
[dependencies]
emcp = { git = "https://github.com/Niach/exponential", tag = "emcp-v0.2.0" }
# with code mode:
emcp = { git = "https://github.com/Niach/exponential", tag = "emcp-v0.2.0", features = ["codemode"] }
```

Everything is blocking from your side: the SDK runs on eMCP's own threads, outside your executor.

## Serve tools

```rust
use std::sync::Arc;
use emcp::{Caller, Server, ServerInfo, SessionId, ToolProvider};
use serde_json::{json, Value};

struct Clock;

impl ToolProvider for Clock {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![json!({ "name": "now", "description": "The time", "inputSchema": { "type": "object" } })])
    }

    fn call(&self, caller: &Caller, _name: &str, _arguments: Value) -> Value {
        json!({ "content": [{ "type": "text", "text": format!("noon, {}", caller.label) }] })
    }
}

let info = ServerInfo { name: "clock".into(), version: "0.1.0".into(), instructions: None };
let server = Server::serve(info, Arc::new(Clock))?;
let run = SessionId::new("run-1")?;            // eCore's id rule
let token = server.grant(&run, "EXP-12");      // an ecore::Token; an earlier one of run-1 dies
// MCP config: { "clock": { "type": "http", "url": server.url(),
//               "headers": { "Authorization": token.header() } } }
server.revoke(&run);                           // the provider hears ended(&run)
```

## Call a server

```rust
use std::time::Duration;
use emcp::{Client, Upstream};
use serde_json::json;

let upstream = Upstream {
    name: "exponential".into(),
    url: "https://your-instance/api/mcp".into(),
    headers: vec![("Authorization".into(), format!("Bearer {key}"))],
};
let client = Client::connect(&upstream, "my-app", Duration::from_secs(10))?;
let tools = client.list_tools(Duration::from_secs(10))?;
let issue = client.call("exponential_issues", json!({ "op": "get", "id": "EXP-12" }), Duration::from_secs(30))?;
```

## Connect a server behind OAuth

```rust
use emcp::oauth;

let authorization = oauth::begin(&config, vault.clone(), "linear")?;
open_browser(authorization.url());
authorization.finish(&code, &state)?;          // the callback's code and state

let client = oauth::connect(&config.url, vault, "linear", "my-app", timeout)?;
oauth::disconnect(&*vault, "linear")?;          // the credential is deleted
```

The hub, code mode and the full public API: [Reference](/docs/libraries/emcp/reference/).
