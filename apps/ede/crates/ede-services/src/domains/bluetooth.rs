//! Bluetooth over BlueZ: adapters and devices, power, connect, disconnect.

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::Value as ZValue;

use super::{bool_of, by_path, call_asking, managed_objects, polled_system_bus, set_prop, str_of, system_bus, Managed};
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
        let Ok(conn) = polled_system_bus() else { return unavailable("no system bus") };
        let Ok(objects) = managed_objects(&conn, BLUEZ, "/") else { return unavailable("BlueZ is not running") };
        let (adapters, devices) = rows(&objects);
        if adapters.is_empty() {
            return json!({"available": true, "adapters": [], "devices": [], "powered": false, "discovering": false, "reason": "no Bluetooth adapter"});
        }
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
                call_asking::<_, ()>(&conn, BLUEZ, &adapter, "org.bluez.Adapter1", if on { "StartDiscovery" } else { "StopDiscovery" }, &())?;
                Ok(json!({"discovering": on}))
            }
            "connect" | "disconnect" => {
                let device = args["device"].as_str().ok_or("device")?;
                call_asking::<_, ()>(&conn, BLUEZ, device, "org.bluez.Device1", if action == "connect" { "Connect" } else { "Disconnect" }, &())?;
                Ok(json!({"device": device, "connected": action == "connect"}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}

/// The adapters and the named devices, connected then paired first.
pub fn rows(objects: &Managed) -> (Vec<Value>, Vec<Value>) {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();
    for (path, ifaces) in by_path(objects) {
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
    devices.sort_by(|a, b| b["connected"].as_bool().cmp(&a["connected"].as_bool()).then(b["paired"].as_bool().cmp(&a["paired"].as_bool())));
    (adapters, devices)
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

    /// #EDE-135: the same objects in another order give the same rows.
    #[test]
    fn the_rows_do_not_follow_the_maps_order() {
        use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};
        let entry = |i: usize| {
            let props: std::collections::HashMap<String, OwnedValue> = [("Alias", ZValue::from(format!("Headset {i}"))), ("Paired", ZValue::from(true)), ("Connected", ZValue::from(false))].into_iter().map(|(k, v)| (k.to_string(), OwnedValue::try_from(v).unwrap())).collect();
            (OwnedObjectPath::try_from(format!("/org/bluez/hci0/dev_{i:02}")).unwrap(), [("org.bluez.Device1".to_string(), props)].into_iter().collect())
        };
        let forward: Managed = (0..16).map(entry).collect();
        let backward: Managed = (0..16).rev().map(entry).collect();
        assert_eq!(json!(rows(&forward)), json!(rows(&backward)));
        assert_eq!(rows(&forward).1[0]["name"], "Headset 0", "ties stay in path order");
    }
}
