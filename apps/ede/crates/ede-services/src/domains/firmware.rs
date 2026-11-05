//! Firmware through fwupd (`fwupdmgr --json`): the devices it knows and
//! the updates it offers.

use ede_protocol::Capability;
use serde_json::{json, Value};

use super::command;
use crate::{unavailable, ActionSpec, Service};

pub struct Firmware;

impl Service for Firmware {
    fn name(&self) -> &'static str {
        "firmware"
    }
    fn description(&self) -> &'static str {
        "Firmware versions and updates through fwupd."
    }

    fn snapshot(&self) -> Value {
        let Some(text) = command("fwupdmgr", &["get-devices", "--json"]) else { return unavailable("fwupd is not available") };
        let parsed: Value = serde_json::from_str(&text).unwrap_or(json!({}));
        let devices: Vec<Value> = parsed["Devices"]
            .as_array()
            .map(|list| list.iter().map(|d| json!({"id": d["DeviceId"], "name": d["Name"], "vendor": d["Vendor"], "version": d["Version"], "updatable": d["Flags"].as_array().is_some_and(|f| f.iter().any(|x| x == "updatable"))})).collect())
            .unwrap_or_default();
        let updates: Vec<Value> = command("fwupdmgr", &["get-updates", "--json"])
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["Devices"].as_array().cloned())
            .map(|list| list.iter().map(|d| json!({"id": d["DeviceId"], "name": d["Name"], "version": d["Releases"][0]["Version"]})).collect())
            .unwrap_or_default();
        json!({"available": true, "devices": devices, "updates": updates})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![ActionSpec::new("refresh", "Refresh the firmware metadata.", json!({}), &[], Some(Capability::System))]
    }

    fn call(&self, action: &str, _args: &Value) -> Result<Value, String> {
        match action {
            "refresh" => command("fwupdmgr", &["refresh", "--force"]).map(|_| json!({"done": "refresh"})).ok_or_else(|| "fwupdmgr refresh failed".into()),
            other => Err(format!("no action {other}")),
        }
    }
}
