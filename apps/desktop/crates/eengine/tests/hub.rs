//! VAPP-262: capabilities on the in-process hub go through the same broker as the app's UI.
#![cfg(feature = "hub")]

use std::sync::Arc;

use eengine::broker::{GrantValue, Grants, ManifestEntry};
use eengine::capability::{Broker, BrokerSetup, Capability};
use eengine::hub::{agent_grant, HubCapability};
use eengine::providers::Failure;
use eengine::stores::MemoryVault;
use emcp::{Caller, Refusal, SessionId, ToolProvider};
use serde_json::{json, Value};

struct Photos;

impl Capability for Photos {
    fn name(&self) -> &str {
        "photos"
    }

    fn tools(&self) -> Vec<Value> {
        vec![json!({"name": "pick"}), json!({"name": "list"}), json!({"name": "delete"})]
    }

    fn call(&self, tool: &str, _arguments: &Value) -> Result<Value, Failure> {
        Ok(json!({"content": [{"type": "text", "text": tool}]}))
    }
}

fn broker(grants: Grants, foreground: bool) -> Arc<Broker> {
    let broker = Arc::new(Broker::new(BrokerSetup {
        app_id: "acme.album".into(),
        app_name: "Album".into(),
        signer: String::new(),
        manifest: vec![ManifestEntry { id: "photos.read".into(), why: "Pick photos.".into(), origins: None }],
        grants,
        capabilities: vec![Arc::new(Photos)],
        vault: Arc::new(MemoryVault::default()),
        consent: None,
        audit: None,
    }));
    broker.set_foreground(foreground);
    broker
}

fn caller() -> Caller {
    Caller { session_id: SessionId::new("run-1").unwrap(), label: "run".into() }
}

#[test]
fn the_hub_lists_only_declared_catalog_tools() {
    let hub = HubCapability::new(broker(Grants::new(), true), Arc::new(Photos));
    let names: Vec<String> = hub.tools().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["pick".to_string(), "list".to_string()], "`delete` is no catalog tool");
    assert_eq!(agent_grant(&broker(Grants::new(), true)).servers.len(), 1);
}

#[test]
fn agent_calls_meet_the_same_consent() {
    let undecided = HubCapability::new(broker(Grants::new(), false), Arc::new(Photos));
    let refused = undecided.try_call(&caller(), "pick", json!({})).unwrap();
    assert_eq!(refused["isError"], json!(true));
    assert_eq!(refused["content"][0]["text"], json!("consentNeeded"));
    assert_eq!(undecided.try_call(&caller(), "delete", json!({})), Err(Refusal::UnknownTool));

    let mut grants = Grants::new();
    grants.insert("photos.read".into(), GrantValue::Granted);
    let granted = HubCapability::new(broker(grants, false), Arc::new(Photos));
    assert_eq!(granted.try_call(&caller(), "list", json!({})).unwrap()["content"][0]["text"], json!("list"));
}
