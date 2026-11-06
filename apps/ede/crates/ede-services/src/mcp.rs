//! The MCP face: eMCP's loopback server (`emcp::Server`, Streamable HTTP
//! with a bearer per agent session) with one tool per action
//! (`<domain>_<action>`) and `<domain>_get` per domain. A grant names the
//! capabilities the session may use; a call outside them is refused as
//! content (`isError`) and audited as `denied`. Every call appends an
//! `mcp_calls`-shaped row (audit.json's columns, `args_hash` canonical) to
//! `$XDG_STATE_HOME/exponential-desktop/mcp-audit.jsonl`.

use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use ede_protocol::Capability;
use emcp::contract::audit::{args_hash, CallStatus};
use serde_json::{json, Value};

use crate::{CallError, Registry};

pub const SERVER_NAME: &str = "exponential-services";

pub struct Mcp {
    server: emcp::Server,
    registry: Arc<Registry>,
    inner: Arc<Inner>,
}

struct Inner {
    grants: Mutex<HashMap<String, Vec<Capability>>>,
    audit: Mutex<Option<std::fs::File>>,
    audit_path: PathBuf,
}

struct Provider {
    registry: Arc<Registry>,
    inner: Arc<Inner>,
}

/// `$XDG_STATE_HOME/exponential-desktop/mcp-audit.jsonl`.
pub fn audit_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    base.join("exponential-desktop").join("mcp-audit.jsonl")
}

/// `<domain>_<action>` and `<domain>_get`.
pub fn tools(registry: &Registry) -> Vec<Value> {
    let mut out = Vec::new();
    for name in registry.names() {
        let s = registry.service(name).unwrap();
        out.push(json!({"name": format!("{name}_get"), "description": format!("{} The current state.", s.description()), "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}}));
        for a in s.actions() {
            let cap = a.capability.map(|c| format!(" Needs the {} capability.", c.as_str())).unwrap_or_default();
            out.push(json!({"name": format!("{name}_{}", a.name), "description": format!("{}{cap}", a.description), "inputSchema": a.schema}));
        }
    }
    out
}

/// `(domain, action)` of a tool name; `None` for `<domain>_get`.
pub fn split_tool(registry: &Registry, tool: &str) -> Option<(String, Option<String>)> {
    for name in registry.names() {
        if let Some(rest) = tool.strip_prefix(name).and_then(|r| r.strip_prefix('_')) {
            return Some((name.to_string(), if rest == "get" { None } else { Some(rest.to_string()) }));
        }
    }
    None
}

impl Inner {
    fn audit(&self, session: &str, tool: &str, args: &Value, status: CallStatus, error: Option<&str>, latency_ms: u64) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        let row = json!({
            "id": format!("{:x}-{}", now, std::process::id()),
            "team_id": Value::Null,
            "user_id": std::env::var("USER").ok(),
            "client_id": session,
            "app_id": Value::Null,
            "tool": tool,
            "status": status.as_str(),
            "error": error,
            "latency_ms": latency_ms,
            "args_hash": args_hash(Some(args)),
            "created_at": emcp::contract::hub::format_utc_ms(now),
        });
        let mut file = self.audit.lock().unwrap_or_else(|p| p.into_inner());
        if file.is_none() {
            if let Some(dir) = self.audit_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            *file = std::fs::OpenOptions::new().create(true).append(true).open(&self.audit_path).ok();
        }
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "{row}");
        }
    }
}

fn result(text: String, structured: Value, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text}], "structuredContent": structured, "isError": is_error})
}

impl emcp::ToolProvider for Provider {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(tools(&self.registry))
    }

    fn call(&self, caller: &emcp::Caller, name: &str, arguments: Value) -> Value {
        let started = Instant::now();
        let session = caller.session_id.as_str().to_string();
        let held = self.inner.grants.lock().unwrap_or_else(|p| p.into_inner()).get(&session).cloned().unwrap_or_default();
        let (status, error, out) = match split_tool(&self.registry, name) {
            None => (CallStatus::Invalid, Some("unknown_tool"), result(format!("no tool {name}"), Value::Null, true)),
            Some((domain, None)) => match self.registry.snapshot(&domain) {
                Some(snapshot) => (CallStatus::Ok, None, result(snapshot.to_string(), snapshot, false)),
                None => (CallStatus::Failed, Some("failed"), result(format!("no domain {domain}"), Value::Null, true)),
            },
            Some((domain, Some(action))) => match self.registry.call(&domain, &action, &arguments, &held) {
                Ok(v) => (CallStatus::Ok, None, result(v.to_string(), v, false)),
                Err(CallError::Denied(cap)) => (CallStatus::Denied, Some("denied"), result(format!("{name} needs the {} capability, which this session does not hold", cap.as_str()), Value::Null, true)),
                Err(CallError::Invalid(m)) => (CallStatus::Invalid, Some("invalid"), result(m, Value::Null, true)),
                Err(CallError::Unknown(m)) => (CallStatus::Invalid, Some("unknown_tool"), result(m, Value::Null, true)),
                Err(CallError::Failed(m)) => (CallStatus::Failed, Some("failed"), result(m, Value::Null, true)),
            },
        };
        self.inner.audit(&session, name, &arguments, status, error, started.elapsed().as_millis() as u64);
        out
    }

    fn ended(&self, session_id: &emcp::SessionId) {
        self.inner.grants.lock().unwrap_or_else(|p| p.into_inner()).remove(session_id.as_str());
    }
}

impl Mcp {
    /// Serves on loopback; the URL is [`Mcp::url`].
    pub fn serve(registry: Arc<Registry>) -> Result<Self, String> {
        Self::serve_at(registry, audit_path())
    }

    pub fn serve_at(registry: Arc<Registry>, audit_path: PathBuf) -> Result<Self, String> {
        let inner = Arc::new(Inner { grants: Mutex::new(HashMap::new()), audit: Mutex::new(None), audit_path });
        let provider = Provider { registry: registry.clone(), inner: inner.clone() };
        let info = emcp::ServerInfo { name: SERVER_NAME.into(), version: env!("CARGO_PKG_VERSION").into(), instructions: Some("This desktop's services: network, sound, media, bluetooth, power, disks, updates, firmware, about. `<domain>_get` reads; an action needs the capability its description names, which your grant holds or not.".into()) };
        let server = emcp::Server::serve(info, Arc::new(provider)).map_err(|e| format!("MCP server: {e}"))?;
        Ok(Self { server, registry, inner })
    }

    pub fn url(&self) -> String {
        self.server.url().to_string()
    }

    /// A bearer for `session` holding `capabilities`; `(url, token)`.
    pub fn grant(&self, session: &str, label: &str, capabilities: Vec<Capability>) -> Result<(String, String), String> {
        let id = emcp::SessionId::new(session).map_err(|e| e.to_string())?;
        self.inner.grants.lock().unwrap_or_else(|p| p.into_inner()).insert(session.to_string(), capabilities);
        let token = self.server.grant(&id, label);
        Ok((self.url(), token.as_str().to_string()))
    }

    pub fn revoke(&self, session: &str) {
        if let Ok(id) = emcp::SessionId::new(session) {
            self.server.revoke(&id);
        }
        self.inner.grants.lock().unwrap_or_else(|p| p.into_inner()).remove(session);
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionSpec, Service};

    struct Lamp;
    impl Service for Lamp {
        fn name(&self) -> &'static str {
            "lamp"
        }
        fn description(&self) -> &'static str {
            "A lamp."
        }
        fn snapshot(&self) -> Value {
            json!({"available": true, "on": false})
        }
        fn actions(&self) -> Vec<ActionSpec> {
            vec![ActionSpec::new("switch", "Switch it.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::System))]
        }
        fn call(&self, _: &str, args: &Value) -> Result<Value, String> {
            Ok(json!({"on": args["on"]}))
        }
    }

    #[test]
    fn tools_and_names() {
        let r = Registry::new(vec![Box::new(Lamp)]);
        let t = tools(&r);
        let names: Vec<&str> = t.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["lamp_get", "lamp_switch"]);
        assert!(t[1]["description"].as_str().unwrap().contains("system capability"));
        assert_eq!(split_tool(&r, "lamp_get"), Some(("lamp".into(), None)));
        assert_eq!(split_tool(&r, "lamp_switch"), Some(("lamp".into(), Some("switch".into()))));
        assert_eq!(split_tool(&r, "nope_x"), None);
    }

    #[test]
    fn calls_are_gated_by_the_grant_and_audited() {
        let dir = tempfile::tempdir().unwrap();
        let audit = dir.path().join("audit.jsonl");
        let mcp = Mcp::serve_at(Arc::new(Registry::new(vec![Box::new(Lamp)])), audit.clone()).unwrap();
        let (url, token) = mcp.grant("agent-1", "test", vec![]).unwrap();
        assert!(url.starts_with("http://127.0.0.1:"));
        let provider = Provider { registry: mcp.registry.clone(), inner: mcp.inner.clone() };
        let caller = emcp::Caller { session_id: emcp::SessionId::new("agent-1").unwrap(), label: "test".into() };
        use emcp::ToolProvider;
        let denied = provider.call(&caller, "lamp_switch", json!({"on": true}));
        assert_eq!(denied["isError"], true);
        let read = provider.call(&caller, "lamp_get", json!({}));
        assert_eq!(read["structuredContent"]["on"], false);
        mcp.grant("agent-1", "test", vec![Capability::System]).unwrap();
        let ok = provider.call(&caller, "lamp_switch", json!({"on": true}));
        assert_eq!(ok["structuredContent"]["on"], true);
        let rows: Vec<Value> = std::fs::read_to_string(&audit).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let statuses: Vec<&str> = rows.iter().map(|r| r["status"].as_str().unwrap()).collect();
        assert_eq!(statuses, ["denied", "ok", "ok"]);
        assert_eq!(rows[0]["error"], "denied");
        assert!(rows[0]["args_hash"].as_str().unwrap().len() > 10);
        assert_eq!(rows[2]["args_hash"], rows[0]["args_hash"], "the same arguments hash the same");
        let _ = token;
    }
}
