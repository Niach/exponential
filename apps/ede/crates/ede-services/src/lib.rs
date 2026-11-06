//! `exponential-services` (#EDE-13): the desktop's services, one per domain over
//! the daemons the distribution already runs. Each [`Service`] gives a
//! JSON `snapshot` (what a settings page or the panel binds) and
//! `actions` (what a page's button or an agent's tool calls), with the
//! eCore capability each action needs. Two faces share one [`Registry`]:
//!
//! - the **data source** ([`socket`]): `$XDG_RUNTIME_DIR/exponential-desktop/services.sock`,
//!   ede-protocol framing, `get`, `call`, `subscribe` (a `services.changed`
//!   event per domain whose snapshot moved; polled every 2 s);
//! - the **MCP face** ([`mcp`]): eMCP's loopback server with one tool per
//!   action plus `<domain>_get`, grants per agent session with the
//!   capabilities they may use, and an `mcp_calls`-shaped audit row per
//!   call in `$XDG_STATE_HOME/exponential-desktop/mcp-audit.jsonl` (audit.json's columns).
//!
//! Behaviour follows what Cinnamon's applets and settings do with the same
//! daemons; no code is shared with them.

pub mod domains;
pub mod mcp;
pub mod socket;

use std::{collections::BTreeMap, sync::Mutex};

use ede_protocol::Capability;
use serde_json::{json, Value};

/// One action a domain offers.
#[derive(Debug, Clone)]
pub struct ActionSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON schema of the arguments (`{"type": "object", …}`).
    pub schema: Value,
    /// `None` = any caller; `Some(cap)` = the grant must hold it.
    pub capability: Option<Capability>,
}

impl ActionSpec {
    pub fn new(name: &'static str, description: &'static str, properties: Value, required: &[&str], capability: Option<Capability>) -> Self {
        Self { name, description, schema: json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false}), capability }
    }
}

/// A domain over a daemon.
pub trait Service: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// The current state as a page binds it; `available: false` with a
    /// `reason` when the daemon is missing.
    fn snapshot(&self) -> Value;
    fn actions(&self) -> Vec<ActionSpec>;
    fn call(&self, action: &str, args: &Value) -> Result<Value, String>;
}

/// Every domain, and the last snapshot of each.
pub struct Registry {
    services: Vec<Box<dyn Service>>,
    snapshots: Mutex<BTreeMap<String, Value>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(domains::all())
    }
}

impl Registry {
    pub fn new(services: Vec<Box<dyn Service>>) -> Self {
        Self { services, snapshots: Mutex::new(BTreeMap::new()) }
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.services.iter().map(|s| s.name()).collect()
    }

    pub fn service(&self, name: &str) -> Option<&dyn Service> {
        self.services.iter().find(|s| s.name() == name).map(|s| s.as_ref())
    }

    /// A fresh snapshot of `name`, remembered.
    pub fn snapshot(&self, name: &str) -> Option<Value> {
        let s = self.service(name)?;
        let value = s.snapshot();
        self.snapshots.lock().unwrap_or_else(|p| p.into_inner()).insert(name.to_string(), value.clone());
        Some(value)
    }

    /// Refreshes every domain; the names whose snapshot changed.
    pub fn refresh(&self) -> Vec<(String, Value)> {
        let mut changed = Vec::new();
        for s in &self.services {
            let value = s.snapshot();
            let mut snaps = self.snapshots.lock().unwrap_or_else(|p| p.into_inner());
            if snaps.get(s.name()) != Some(&value) {
                snaps.insert(s.name().to_string(), value.clone());
                changed.push((s.name().to_string(), value));
            }
        }
        changed
    }

    pub fn last(&self, name: &str) -> Option<Value> {
        self.snapshots.lock().unwrap_or_else(|p| p.into_inner()).get(name).cloned()
    }

    /// Runs `action` of `domain` for a caller holding `held` capabilities.
    pub fn call(&self, domain: &str, action: &str, args: &Value, held: &[Capability]) -> Result<Value, CallError> {
        let s = self.service(domain).ok_or_else(|| CallError::Unknown(format!("no domain {domain}")))?;
        let spec = s.actions().into_iter().find(|a| a.name == action).ok_or_else(|| CallError::Unknown(format!("{domain} has no action {action}")))?;
        if let Some(cap) = spec.capability {
            if !held.contains(&cap) {
                return Err(CallError::Denied(cap));
            }
        }
        if let Some(missing) = required_missing(&spec.schema, args) {
            return Err(CallError::Invalid(format!("missing argument {missing}")));
        }
        s.call(action, args).map_err(CallError::Failed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    Unknown(String),
    Denied(Capability),
    Invalid(String),
    Failed(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Unknown(m) | CallError::Invalid(m) | CallError::Failed(m) => f.write_str(m),
            CallError::Denied(c) => write!(f, "needs the {} capability", c.as_str()),
        }
    }
}

fn required_missing(schema: &Value, args: &Value) -> Option<String> {
    schema["required"].as_array()?.iter().filter_map(Value::as_str).find(|k| args.get(k).is_none()).map(String::from)
}

/// `$XDG_RUNTIME_DIR/exponential-desktop/services.sock`.
pub fn socket_path() -> std::path::PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    runtime.join("exponential-desktop").join("services.sock")
}

/// A snapshot for a domain whose daemon is not there.
pub fn unavailable(reason: impl Into<String>) -> Value {
    json!({ "available": false, "reason": reason.into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;
    impl Service for Echo {
        fn name(&self) -> &'static str {
            "echo"
        }
        fn description(&self) -> &'static str {
            "a test"
        }
        fn snapshot(&self) -> Value {
            json!({"available": true, "n": 1})
        }
        fn actions(&self) -> Vec<ActionSpec> {
            vec![ActionSpec::new("say", "say it", json!({"text": {"type": "string"}}), &["text"], Some(Capability::System)), ActionSpec::new("ping", "pong", json!({}), &[], None)]
        }
        fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
            Ok(json!({"action": action, "args": args}))
        }
    }

    #[test]
    fn calls_check_the_capability_and_the_required_arguments() {
        let r = Registry::new(vec![Box::new(Echo)]);
        assert_eq!(r.call("echo", "ping", &json!({}), &[]).unwrap()["action"], "ping");
        assert_eq!(r.call("echo", "say", &json!({"text": "hi"}), &[]), Err(CallError::Denied(Capability::System)));
        assert_eq!(r.call("echo", "say", &json!({}), &[Capability::System]), Err(CallError::Invalid("missing argument text".into())));
        assert!(r.call("echo", "say", &json!({"text": "hi"}), &[Capability::System]).is_ok());
        assert!(matches!(r.call("nope", "x", &json!({}), &[]), Err(CallError::Unknown(_))));
        assert_eq!(r.refresh().len(), 1, "the first refresh reports every domain");
        assert!(r.refresh().is_empty(), "nothing moved");
        assert_eq!(r.last("echo").unwrap()["n"], 1);
    }
}
