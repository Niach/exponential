//! Bluetooth over BlueZ: adapters and devices, power, connect, disconnect.

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::Value as ZValue;

use super::{bool_of, managed_objects, set_prop, str_of, system_bus};
use crate::{unavailable, ActionSpec, Service};

const BLUEZ: &str = "org.bluez";

pub struct Bluetooth;

impl Service for Bluetooth {
    fn name(&self) -> &'static str {
        "bluetooth"
    }
    fn description(&self) -> &'static str {
        "Bluetooth adapters and devices through BlueZ."
    }

    fn snapshot(&self) -> Value {
        let Ok(conn) = system_bus() else { return unavailable("no system bus") };
        let Ok(objects) = managed_objects(&conn, BLUEZ, "/") else { return unavailable("BlueZ is not running") };
        let mut adapters = Vec::new();
        let mut devices = Vec::new();
        for (path, ifaces) in &objects {
            if let Some(a) = ifaces.get("org.bluez.Adapter1") {
                adapters.push(json!({"path": path.as_str(), "name": str_of(a, "Alias"), "powered": bool_of(a, "Powered"), "discovering": bool_of(a, "Discovering")}));
            }
            if let Some(d) = ifaces.get("org.bluez.Device1") {
                let name = str_of(d, "Alias");
                if name.is_empty() {
                    continue;
                }
                devices.push(json!({"path": path.as_str(), "name": name, "address": str_of(d, "Address"), "paired": bool_of(d, "Paired"), "connected": bool_of(d, "Connected"), "trusted": bool_of(d, "Trusted"), "icon": str_of(d, "Icon")}));
            }
        }
        if adapters.is_empty() {
            return json!({"available": true, "adapters": [], "devices": [], "powered": false, "discovering": false, "reason": "no Bluetooth adapter"});
        }
        devices.sort_by(|a, b| b["connected"].as_bool().cmp(&a["connected"].as_bool()).then(b["paired"].as_bool().cmp(&a["paired"].as_bool())));
        let (powered, discovering) = adapter_flags(&adapters);
        json!({"available": true, "powered": powered, "discovering": discovering, "adapters": adapters, "devices": devices})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("power", "Turn the adapter on or off.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::System)),
            ActionSpec::new("connect", "Connect a paired device (its object path from the snapshot).", json!({"device": {"type": "string"}}), &["device"], Some(Capability::System)),
            ActionSpec::new("disconnect", "Disconnect a device.", json!({"device": {"type": "string"}}), &["device"], Some(Capability::System)),
            ActionSpec::new("discover", "Scan for devices for a while.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::System)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let conn = system_bus()?;
        let objects = managed_objects(&conn, BLUEZ, "/")?;
        let adapter = objects.iter().find(|(_, i)| i.contains_key("org.bluez.Adapter1")).map(|(p, _)| p.as_str().to_string()).ok_or("no Bluetooth adapter")?;
        match action {
            "power" => {
                let on = args["on"].as_bool().unwrap_or(true);
                set_prop(&conn, BLUEZ, &adapter, "org.bluez.Adapter1", "Powered", ZValue::from(on))?;
                Ok(json!({"powered": on}))
            }
            "discover" => {
                let on = args["on"].as_bool().unwrap_or(true);
                conn.call_method(Some(BLUEZ), adapter.as_str(), Some("org.bluez.Adapter1"), if on { "StartDiscovery" } else { "StopDiscovery" }, &()).map_err(|e| e.to_string())?;
                Ok(json!({"discovering": on}))
            }
            "connect" | "disconnect" => {
                let device = args["device"].as_str().ok_or("device")?;
                conn.call_method(Some(BLUEZ), device, Some("org.bluez.Device1"), if action == "connect" { "Connect" } else { "Disconnect" }, &()).map_err(|e| e.to_string())?;
                Ok(json!({"device": device, "connected": action == "connect"}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}

/// The snapshot's top-level `powered` and `discovering`: any adapter.
pub fn adapter_flags(adapters: &[Value]) -> (bool, bool) {
    (adapters.iter().any(|a| a["powered"].as_bool() == Some(true)), adapters.iter().any(|a| a["discovering"].as_bool() == Some(true)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovering_follows_any_adapter() {
        let adapters = vec![json!({"path": "/org/bluez/hci0", "powered": true, "discovering": false}), json!({"path": "/org/bluez/hci1", "powered": false, "discovering": true})];
        assert_eq!(adapter_flags(&adapters), (true, true));
        assert_eq!(adapter_flags(&adapters[..1]), (true, false));
        assert_eq!(adapter_flags(&[]), (false, false));
    }
}
