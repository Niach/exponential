//! VAPP-262 (feature `hub`): the app's capabilities as MCP tools on an
//! in-process MCP Proxy hub (`emcp::hub`, packages/emcp hub.json), for
//! spawned agents (loopback + bearer token) and in-process providers alike.
//! Each capability mounts under its catalog name, so an agent sees
//! `<capability>__<tool>`; every call goes through the instance's ONE
//! [`Broker`], exactly like the app's UI. The hub lists the catalog's
//! app-facing tool objects (capabilities.json `tools`: description and
//! schemas) of the tools the manifest declares; nothing it serves writes the
//! package, the grants or the host config.

use std::collections::BTreeMap;
use std::sync::Arc;

use emcp::contract::access::Level;
use emcp::hub::{Hub, HubGrant, Mount};
use emcp::{Caller, Refusal, ToolProvider};
use serde_json::{json, Value};

use crate::broker::{agent_tool_list, CallFrom};
use crate::capability::{Broker, BrokerAnswer, Capability};
use crate::tools::tool_def;

/// One capability behind the hub, gated by the broker.
pub struct HubCapability {
    broker: Arc<Broker>,
    capability: Arc<dyn Capability>,
}

impl HubCapability {
    pub fn new(broker: Arc<Broker>, capability: Arc<dyn Capability>) -> Self {
        HubCapability { broker, capability }
    }

    fn qualified(&self, tool: &str) -> String {
        format!("{}__{tool}", self.capability.name())
    }
}

fn text_error(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

impl ToolProvider for HubCapability {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(listed_tools(&self.broker).into_iter().filter_map(|(capability, object)| (capability == self.capability.name()).then_some(object)).collect())
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        self.try_call(caller, name, arguments).unwrap_or_else(|_| text_error(&format!("Unknown tool: {name}")))
    }

    fn try_call(&self, caller: &Caller, name: &str, arguments: Value) -> Result<Value, Refusal> {
        match self.broker.call(CallFrom::Agent, Some(caller.session_id.as_str()), &self.qualified(name), &arguments) {
            BrokerAnswer::Result(result) => Ok(result),
            BrokerAnswer::Refused(failure) if failure.code == "UNKNOWN_TOOL" => Err(Refusal::UnknownTool),
            // The refusal is content the agent reads (consentNeeded, consentDenied, …), never a hint at a tool it lacks.
            BrokerAnswer::Refused(failure) => Ok(text_error(&failure.message)),
            // No inline ConsentUi: the person has not decided yet.
            BrokerAnswer::Ask(..) => Ok(text_error("consentNeeded")),
        }
    }
}

/// What an agent's tools/list shows for `broker`: `(capability, tool object)`
/// for each tool `agent_tool_list` allows that the v1 catalog defines, the
/// object named by its bare tool (the hub qualifies it as `<capability>__<tool>`).
pub fn listed_tools(broker: &Broker) -> Vec<(String, Value)> {
    let provided: Vec<String> = broker.capabilities().iter().map(|c| c.name().to_string()).collect();
    agent_tool_list(broker.manifest(), &provided)
        .into_iter()
        .filter_map(|qualified| {
            let (capability, tool) = qualified.split_once("__")?;
            Some((capability.to_string(), tool_def(capability, tool)?.tool_object(tool)))
        })
        .collect()
}

/// Mount every capability of `broker` on `hub` under its catalog name.
pub fn mount_capabilities(hub: &Hub, broker: &Arc<Broker>) -> Result<(), String> {
    for capability in broker.capabilities() {
        hub.mount(capability.name(), Mount::Provider(Arc::new(HubCapability::new(broker.clone(), capability.clone()))))?;
    }
    Ok(())
}

/// An agent run's hub grant over the app's capabilities: `write` on each
/// mounted capability (the broker is the gate), no team, no user.
pub fn agent_grant(broker: &Broker) -> HubGrant {
    let servers: BTreeMap<String, Level> = broker.capabilities().iter().map(|c| (c.name().to_string(), Level::Write)).collect();
    HubGrant { servers, team_id: None, user_id: None }
}
