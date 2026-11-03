//! VAPP-239 (eApps M2, `packages/emcp/fixtures/upstream.json` `device`): run
//! ONE app call the server routed to this device. The host (the steer control
//! channel of the desktop app or the CLI daemon) claims the call over its own
//! API and hands the claim here; this connects the server (http, or stdio by
//! spawning its command), checks the tool's LIVE annotations against the
//! call's veto, runs exactly that tool and answers the fixture's
//! [`DeviceAnswer`]. The host seals it to the call's reply key.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::client::{Client, ClientError, StdioUpstream, Upstream};
use crate::contract::exposes::{upstream_call_error, ToolAccess};
use crate::contract::upstream::{upstream_read_error, DeviceAnswer, DEVICE_MAX_ANSWER_BYTES};

/// One `{name, value}` header or env pair as the server resolved it.
#[derive(Clone, Deserialize)]
pub struct NamedValue {
    pub name: String,
    pub value: String,
}

/// The server with the member's credential resolved at claim time
/// (`ResolvedMcpServer`).
#[derive(Clone, Deserialize)]
pub struct ClaimedServer {
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub headers: Vec<NamedValue>,
    #[serde(default)]
    pub env: Vec<NamedValue>,
}

/// The check the device runs before the call.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Veto {
    /// A read binding: `upstream_read_error` with the package's `sources`.
    Read { sources: Vec<String> },
    /// An exposes / automation run: `upstream_call_error` with its access.
    Call { access: String },
}

/// tRPC `mcpServers.claimDeviceCall`'s answer.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedCall {
    pub server: Option<ClaimedServer>,
    #[serde(default)]
    pub skipped: Option<String>,
    pub tool: String,
    #[serde(default)]
    pub args: Value,
    pub veto: Veto,
    pub reply_public: String,
}

/// The `clientInfo.name` device calls connect with.
pub const DEVICE_CLIENT_NAME: &str = "exponential-device";

fn failed(error: &ClientError) -> DeviceAnswer {
    match error {
        ClientError::Http(401) | ClientError::Http(403) => DeviceAnswer::err("refused", error.to_string()),
        _ => DeviceAnswer::err("failed", error.to_string()),
    }
}

/// The veto's verdict on the tool's annotations: `None` = it runs.
pub fn veto_error(veto: &Veto, server: &str, tool: &str, annotations: Option<&Value>) -> Option<&'static str> {
    match veto {
        Veto::Read { sources } => upstream_read_error(server, tool, annotations, sources),
        Veto::Call { access } => {
            let access = if access == "read" { ToolAccess::Read } else { ToolAccess::Write };
            upstream_call_error(access, tool, annotations)
        }
    }
}

fn connect(server: &ClaimedServer, timeout: Duration) -> Result<Client, DeviceAnswer> {
    if server.transport == "stdio" {
        let Some(command) = server.command.clone().filter(|c| !c.is_empty()) else {
            return Err(DeviceAnswer::err("failed", "the server has no command"));
        };
        let upstream = StdioUpstream {
            name: server.name.clone(),
            command,
            args: server.args.clone(),
            env: server.env.iter().map(|e| (e.name.clone(), e.value.clone())).collect(),
        };
        return Client::spawn(&upstream, DEVICE_CLIENT_NAME, timeout).map_err(|e| failed(&e));
    }
    let Some(url) = server.url.clone().filter(|u| !u.is_empty()) else {
        return Err(DeviceAnswer::err("failed", "the server has no URL"));
    };
    let upstream = Upstream { name: server.name.clone(), url, headers: server.headers.iter().map(|h| (h.name.clone(), h.value.clone())).collect() };
    Client::connect(&upstream, DEVICE_CLIENT_NAME, timeout).map_err(|e| failed(&e))
}

/// Run the claimed call; never panics, every failure is an answer. The
/// answer stays under `maxAnswerBytes` of JSON.
pub fn run_device_call(call: &ClaimedCall, timeout: Duration) -> DeviceAnswer {
    let Some(server) = call.server.as_ref() else {
        return DeviceAnswer::err("refused", call.skipped.clone().unwrap_or_else(|| "not connected".to_string()));
    };
    let client = match connect(server, timeout) {
        Ok(client) => client,
        Err(answer) => return answer,
    };
    let tools = match client.list_tools(timeout) {
        Ok(tools) => tools,
        Err(e) => return failed(&e),
    };
    let Some(listed) = tools.iter().find(|t| t["name"].as_str() == Some(call.tool.as_str())) else {
        return DeviceAnswer::err("failed", format!("{} has no tool {}", server.name, call.tool));
    };
    let annotations = listed.get("annotations").filter(|a| a.is_object());
    if let Some(error) = veto_error(&call.veto, &server.name, &call.tool, annotations) {
        return DeviceAnswer::err("vetoed", error);
    }
    let args = if call.args.is_object() { call.args.clone() } else { Value::Object(Default::default()) };
    let answer = match client.call(&call.tool, args, timeout) {
        Ok(result) => DeviceAnswer::ok(result),
        Err(e) => failed(&e),
    };
    match serde_json::to_vec(&answer) {
        Ok(bytes) if bytes.len() <= DEVICE_MAX_ANSWER_BYTES => answer,
        _ => DeviceAnswer::err("failed", "the answer is too large"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_veto_runs_the_fixture_rules() {
        let read = Veto::Read { sources: vec!["upstream:ha/*".into()] };
        assert_eq!(veto_error(&read, "ha", "get_state", Some(&json!({"readOnlyHint": true}))), None);
        assert_eq!(veto_error(&read, "ha", "get_state", None), Some("notReadOnly"));
        assert_eq!(veto_error(&read, "ha", "delete_scene", Some(&json!({"readOnlyHint": true}))), Some("destructive"));
        let call = Veto::Call { access: "read".into() };
        assert_eq!(veto_error(&call, "ha", "turn_on", Some(&json!({"readOnlyHint": false}))), Some("accessTooLow"));
        let write = Veto::Call { access: "write".into() };
        assert_eq!(veto_error(&write, "ha", "turn_on", Some(&json!({"readOnlyHint": false}))), None);
    }

    #[test]
    fn the_claim_parses_and_a_skipped_server_is_refused() {
        let claim: ClaimedCall = serde_json::from_value(json!({
            "server": null, "skipped": "not connected", "tool": "t", "args": {},
            "veto": {"kind": "call", "access": "write"}, "replyPublic": "x"
        }))
        .unwrap();
        assert_eq!(claim.veto, Veto::Call { access: "write".into() });
        assert_eq!(run_device_call(&claim, Duration::from_secs(1)), DeviceAnswer::err("refused", "not connected"));
        let claim: ClaimedCall = serde_json::from_value(json!({
            "server": {"name": "ha", "transport": "stdio", "url": null, "command": "/nonexistent/ha-mcp", "args": [], "headers": [], "env": [{"name": "TOKEN", "value": "s"}], "actor": null},
            "skipped": null, "tool": "t", "args": {}, "veto": {"kind": "read", "sources": []}, "replyPublic": "x"
        }))
        .unwrap();
        match run_device_call(&claim, Duration::from_secs(5)) {
            DeviceAnswer::Err { reason, .. } => assert_eq!(reason, "failed"),
            other => panic!("{other:?}"),
        }
    }
}
