//! Network over NetworkManager: the state, every device, the Wi-Fi access
//! points in reach; connect, disconnect, Wi-Fi on and off.

use std::collections::HashMap;

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};

use super::{prop, set_prop, system_bus};
use crate::{unavailable, ActionSpec, Service};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

pub struct Network;

fn device_type(t: u32) -> &'static str {
    match t {
        1 => "ethernet",
        2 => "wifi",
        5 => "bluetooth",
        13 => "bridge",
        14 => "generic",
        16 => "tun",
        29 => "wireguard",
        32 => "loopback",
        _ => "other",
    }
}

fn device_state(s: u32) -> &'static str {
    match s {
        10 => "unmanaged",
        20 => "unavailable",
        30 => "disconnected",
        40..=90 => "connecting",
        100 => "connected",
        110 => "deactivating",
        120 => "failed",
        _ => "unknown",
    }
}

pub fn nm_state(s: u32) -> &'static str {
    match s {
        10 => "asleep",
        20 => "disconnected",
        30 => "disconnecting",
        40 => "connecting",
        50 => "local",
        60 => "site",
        70 => "global",
        _ => "unknown",
    }
}

impl Service for Network {
    fn name(&self) -> &'static str {
        "network"
    }
    fn description(&self) -> &'static str {
        "Wired and wireless networking through NetworkManager."
    }

    fn snapshot(&self) -> Value {
        let Ok(conn) = system_bus() else { return unavailable("no system bus") };
        let Some(state) = prop::<u32>(&conn, NM, NM_PATH, NM, "State") else { return unavailable("NetworkManager is not running") };
        let wifi_enabled = prop::<bool>(&conn, NM, NM_PATH, NM, "WirelessEnabled").unwrap_or(false);
        let primary_type = prop::<String>(&conn, NM, NM_PATH, NM, "PrimaryConnectionType").unwrap_or_default();
        let paths: Vec<OwnedObjectPath> = conn.call_method(Some(NM), NM_PATH, Some(NM), "GetDevices", &()).and_then(|r| r.body().deserialize()).unwrap_or_default();
        let mut devices = Vec::new();
        let mut access_points = Vec::new();
        for path in paths {
            let p = path.as_str();
            let dev = format!("{NM}.Device");
            let interface = prop::<String>(&conn, NM, p, &dev, "Interface").unwrap_or_default();
            let kind = device_type(prop::<u32>(&conn, NM, p, &dev, "DeviceType").unwrap_or(0));
            if kind == "loopback" {
                continue;
            }
            let dstate = device_state(prop::<u32>(&conn, NM, p, &dev, "State").unwrap_or(0));
            let active: Option<OwnedObjectPath> = prop(&conn, NM, p, &dev, "ActiveConnection");
            let connection = active.filter(|a| a.as_str() != "/").and_then(|a| prop::<String>(&conn, NM, a.as_str(), &format!("{NM}.Connection.Active"), "Id")).unwrap_or_default();
            let ip4: Option<OwnedObjectPath> = prop(&conn, NM, p, &dev, "Ip4Config");
            let addresses: Vec<String> = ip4
                .filter(|a| a.as_str() != "/")
                .and_then(|a| prop::<Vec<HashMap<String, OwnedValue>>>(&conn, NM, a.as_str(), &format!("{NM}.IP4Config"), "AddressData"))
                .map(|list| list.iter().map(|m| super::str_of(m, "address")).filter(|a| !a.is_empty()).collect())
                .unwrap_or_default();
            devices.push(json!({"path": p, "interface": interface, "type": kind, "state": dstate, "connection": connection, "addresses": addresses}));
            if kind == "wifi" {
                let wireless = format!("{NM}.Device.Wireless");
                let active_ap: Option<OwnedObjectPath> = prop(&conn, NM, p, &wireless, "ActiveAccessPoint");
                let aps: Vec<OwnedObjectPath> = conn.call_method(Some(NM), p, Some(wireless.as_str()), "GetAllAccessPoints", &()).and_then(|r| r.body().deserialize()).unwrap_or_default();
                for ap in aps {
                    let a = ap.as_str();
                    let ap_iface = format!("{NM}.AccessPoint");
                    let ssid = prop::<Vec<u8>>(&conn, NM, a, &ap_iface, "Ssid").map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                    if ssid.is_empty() {
                        continue;
                    }
                    let strength = prop::<u8>(&conn, NM, a, &ap_iface, "Strength").unwrap_or(0);
                    let secured = prop::<u32>(&conn, NM, a, &ap_iface, "RsnFlags").unwrap_or(0) != 0 || prop::<u32>(&conn, NM, a, &ap_iface, "WpaFlags").unwrap_or(0) != 0;
                    access_points.push(json!({"ssid": ssid, "strength": strength, "secured": secured, "active": active_ap.as_ref().is_some_and(|x| x.as_str() == a), "device": p}));
                }
            }
        }
        access_points.sort_by(|a, b| b["strength"].as_u64().cmp(&a["strength"].as_u64()));
        access_points.dedup_by(|a, b| a["ssid"] == b["ssid"]);
        json!({"available": true, "state": nm_state(state), "connected": state >= 50, "primary": primary_type, "wifiEnabled": wifi_enabled, "devices": devices, "accessPoints": access_points})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("wifi_enable", "Turn Wi-Fi on or off.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::System)),
            ActionSpec::new("connect", "Join a Wi-Fi network by SSID (a saved one, or a new one with its password).", json!({"ssid": {"type": "string"}, "password": {"type": "string"}}), &["ssid"], Some(Capability::System)),
            ActionSpec::new("disconnect", "Disconnect a device (its object path from the snapshot).", json!({"device": {"type": "string"}}), &["device"], Some(Capability::System)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let conn = system_bus()?;
        match action {
            "wifi_enable" => {
                let on = args["on"].as_bool().unwrap_or(true);
                set_prop(&conn, NM, NM_PATH, NM, "WirelessEnabled", ZValue::from(on))?;
                Ok(json!({"wifiEnabled": on}))
            }
            "disconnect" => {
                let device = args["device"].as_str().ok_or("device")?;
                conn.call_method(Some(NM), device, Some(format!("{NM}.Device").as_str()), "Disconnect", &()).map_err(|e| e.to_string())?;
                Ok(json!({"disconnected": device}))
            }
            "connect" => {
                let ssid = args["ssid"].as_str().ok_or("ssid")?;
                let snapshot = self.snapshot();
                let ap = snapshot["accessPoints"].as_array().and_then(|l| l.iter().find(|a| a["ssid"] == ssid)).cloned().ok_or_else(|| format!("no network {ssid} in reach"))?;
                let device = ap["device"].as_str().unwrap_or("/").to_string();
                let device_path = OwnedObjectPath::try_from(device.clone()).map_err(|e| e.to_string())?;
                // A saved connection with that id activates as is.
                let settings = format!("{NM}.Settings");
                let saved: Vec<OwnedObjectPath> = conn.call_method(Some(NM), "/org/freedesktop/NetworkManager/Settings", Some(settings.as_str()), "ListConnections", &()).and_then(|r| r.body().deserialize()).unwrap_or_default();
                for c in saved {
                    let s: HashMap<String, HashMap<String, OwnedValue>> = conn.call_method(Some(NM), c.as_str(), Some(format!("{NM}.Settings.Connection").as_str()), "GetSettings", &()).and_then(|r| r.body().deserialize()).unwrap_or_default();
                    if s.get("connection").map(|m| super::str_of(m, "id")) == Some(ssid.to_string()) {
                        let root = OwnedObjectPath::try_from("/").unwrap();
                        conn.call_method(Some(NM), NM_PATH, Some(NM), "ActivateConnection", &(c, device_path, root)).map_err(|e| e.to_string())?;
                        return Ok(json!({"connecting": ssid, "saved": true}));
                    }
                }
                let mut connection: HashMap<&str, HashMap<&str, ZValue<'_>>> = HashMap::new();
                connection.insert("connection", HashMap::from([("id", ZValue::from(ssid)), ("type", ZValue::from("802-11-wireless"))]));
                connection.insert("802-11-wireless", HashMap::from([("ssid", ZValue::from(ssid.as_bytes().to_vec())), ("mode", ZValue::from("infrastructure"))]));
                if let Some(password) = args["password"].as_str().filter(|p| !p.is_empty()) {
                    connection.insert("802-11-wireless-security", HashMap::from([("key-mgmt", ZValue::from("wpa-psk")), ("psk", ZValue::from(password))]));
                } else if ap["secured"].as_bool().unwrap_or(false) {
                    return Err(format!("{ssid} needs a password"));
                }
                let root = OwnedObjectPath::try_from("/").unwrap();
                conn.call_method(Some(NM), NM_PATH, Some(NM), "AddAndActivateConnection", &(connection, device_path, root)).map_err(|e| e.to_string())?;
                Ok(json!({"connecting": ssid, "saved": false}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}
