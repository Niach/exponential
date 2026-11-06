//! VAPP-262: capabilities on the in-process hub go through the same broker as the app's UI.
#![cfg(feature = "hub")]

use std::sync::Arc;

use eengine::broker::{GrantValue, Grants, ManifestEntry};
use eengine::capability::{Broker, BrokerSetup, Capability};
use eengine::hub::{agent_grant, HubCapability};
use eengine::providers::Failure;
use eengine::stores::{FsBlobStore, MemoryVault};
use eengine::tools::TOOLS;
use emcp::{Caller, Refusal, SessionId, ToolProvider};
use serde_json::{json, Value};

struct Photos;

impl Capability for Photos {
    fn name(&self) -> &str {
        "photos"
    }

    fn tools(&self) -> Vec<Value> {
        vec![]
    }

    /// Native results per capabilities.json's `nativeOutputSchema`.
    fn call(&self, tool: &str, _arguments: &Value) -> Result<Value, Failure> {
        Ok(match tool {
            "list" => json!({"structuredContent": {"photos": [{"id": "ph-1", "name": "a.jpg", "mimeType": "image/jpeg", "takenAt": null}], "cursor": null}}),
            _ => json!({"structuredContent": {"photos": []}}),
        })
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
        blobs: Arc::new(FsBlobStore::new(std::env::temp_dir().join("eengine-hub-test-blobs"))),
        screens: vec!["albums".into()],
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
    let tools = hub.tools().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["get", "list", "pick"], "the catalog's photos tools, not the native's own list");
    let pick = TOOLS.iter().find(|t| t.name == "photos.pick").unwrap();
    assert_eq!(tools[2], json!({"name": "pick", "description": pick.description, "inputSchema": pick.input_schema, "outputSchema": pick.output_schema}));
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
    let listed = granted.try_call(&caller(), "list", json!({})).unwrap();
    assert_eq!(listed["structuredContent"]["photos"][0]["id"], json!("ph-1"));
    assert_eq!(listed["content"][0]["text"], json!(listed["structuredContent"].to_string()));
    let invalid = granted.try_call(&caller(), "pick", json!({"limit": 99})).unwrap();
    assert_eq!((invalid["isError"].clone(), invalid["content"][0]["text"].clone()), (json!(true), json!("invalidParams: /limit maximum")));
}
