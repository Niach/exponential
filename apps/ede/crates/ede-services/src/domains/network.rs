//! Network over NetworkManager: the state, every device, the Wi-Fi access
//! points in reach, the wired devices and the VPN connections (#EDE-43's
//! applet: switch rows for Wired, Wireless and VPN); connect, disconnect,
//! Wi-Fi on and off, wired on and off, a VPN up or down.

use std::collections::HashMap;

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};

use super::{prop, set_prop, system_bus};
use crate::{unavailable, ActionSpec, Service};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";

pub struct Network;

/// A saved connection as NetworkManager's settings list it.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedConnection {
    pub path: String,
    pub uuid: String,
    pub id: String,
    pub kind: String,
}

/// NetworkManager's connection types that are a VPN for the applet.
pub fn is_vpn(kind: &str) -> bool {
    matches!(kind, "vpn" | "wireguard")
}

/// The snapshot's `vpn` list: every saved VPN connection, `active` when
/// its uuid is among the active connections.
pub fn vpn_entries(saved: &[SavedConnection], active_uuids: &[String]) -> Vec<Value> {
    saved
        .iter()
        .filter(|c| is_vpn(&c.kind))
        .map(|c| json!({"id": c.uuid, "name": c.id, "type": c.kind, "active": active_uuids.contains(&c.uuid)}))
        .collect()
}

/// The snapshot's `wired` list: the ethernet devices.
pub fn wired_entries(devices: &[Value]) -> Vec<Value> {
    devices
        .iter()
        .filter(|d| d["type"] == "ethernet")
        .map(|d| json!({"path": d["path"], "interface": d["interface"], "state": d["state"], "connection": d["connection"]}))
        .collect()
}

fn saved_connections(conn: &zbus::blocking::Connection) -> Vec<SavedConnection> {
    let settings = format!("{NM}.Settings");
    let paths: Vec<OwnedObjectPath> = conn.call_method(Some(NM), SETTINGS_PATH, Some(settings.as_str()), "ListConnections", &()).and_then(|r| r.body().deserialize()).unwrap_or_default();
    paths
        .into_iter()
        .filter_map(|c| {
            let s: HashMap<String, HashMap<String, OwnedValue>> = conn.call_method(Some(NM), c.as_str(), Some(format!("{NM}.Settings.Connection").as_str()), "GetSettings", &()).and_then(|r| r.body().deserialize()).ok()?;
            let connection = s.get("connection")?;
            Some(SavedConnection { path: c.as_str().to_string(), uuid: super::str_of(connection, "uuid"), id: super::str_of(connection, "id"), kind: super::str_of(connection, "type") })
        })
        .collect()
}

/// `(path, uuid)` of every active connection.
fn active_connections(conn: &zbus::blocking::Connection) -> Vec<(String, String)> {
    let active: Vec<OwnedObjectPath> = prop(conn, NM, NM_PATH, NM, "ActiveConnections").unwrap_or_default();
    active.into_iter().map(|a| (a.as_str().to_string(), prop::<String>(conn, NM, a.as_str(), &format!("{NM}.Connection.Active"), "Uuid").unwrap_or_default())).collect()
}

fn root_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").unwrap()
}

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
        let active_uuids: Vec<String> = active_connections(&conn).into_iter().map(|(_, uuid)| uuid).collect();
        let vpn = vpn_entries(&saved_connections(&conn), &active_uuids);
        let wired = wired_entries(&devices);
        let wired_connected = wired.iter().any(|d| d["state"] == "connected");
        json!({"available": true, "state": nm_state(state), "connected": state >= 50, "primary": primary_type, "wifiEnabled": wifi_enabled, "wiredConnected": wired_connected, "devices": devices, "wired": wired, "accessPoints": access_points, "vpn": vpn})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("wifi_enable", "Turn Wi-Fi on or off.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::System)),
            ActionSpec::new("connect", "Join a Wi-Fi network by SSID (a saved one, or a new one with its password).", json!({"ssid": {"type": "string"}, "password": {"type": "string"}}), &["ssid"], Some(Capability::System)),
            ActionSpec::new("disconnect", "Disconnect a device (its object path from the snapshot).", json!({"device": {"type": "string"}}), &["device"], Some(Capability::System)),
            ActionSpec::new("wired_enable", "Connect (on) or disconnect (off) the wired devices; off also stops them from reconnecting on their own.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::System)),
            ActionSpec::new("vpn_connect", "Bring a saved VPN up, by its id from the snapshot's vpn list.", json!({"id": {"type": "string"}}), &["id"], Some(Capability::System)),
            ActionSpec::new("vpn_disconnect", "Take an active VPN down, by its id.", json!({"id": {"type": "string"}}), &["id"], Some(Capability::System)),
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
            "wired_enable" => {
                let on = args["on"].as_bool().unwrap_or(true);
                let dev = format!("{NM}.Device");
                let snapshot = self.snapshot();
                let wired = snapshot["wired"].as_array().cloned().unwrap_or_default();
                if wired.is_empty() {
                    return Err("no wired device".into());
                }
                let mut touched = Vec::new();
                for d in &wired {
                    let path = d["path"].as_str().unwrap_or("/");
                    let path_obj = OwnedObjectPath::try_from(path.to_string()).map_err(|e| e.to_string())?;
                    set_prop(&conn, NM, path, &dev, "Autoconnect", ZValue::from(on))?;
                    if on && d["state"] == "disconnected" {
                        // "/" = NetworkManager picks the device's best saved connection.
                        if conn.call_method(Some(NM), NM_PATH, Some(NM), "ActivateConnection", &(root_path(), path_obj, root_path())).is_ok() {
                            touched.push(path.to_string());
                        }
                    } else if !on && d["state"] != "disconnected" && d["state"] != "unavailable" && d["state"] != "unmanaged" {
                        conn.call_method(Some(NM), path, Some(dev.as_str()), "Disconnect", &()).map_err(|e| e.to_string())?;
                        touched.push(path.to_string());
                    }
                }
                Ok(json!({"wiredEnabled": on, "devices": touched}))
            }
            "vpn_connect" => {
                let id = args["id"].as_str().ok_or("id")?;
                let saved = saved_connections(&conn).into_iter().find(|c| c.uuid == id && is_vpn(&c.kind)).ok_or_else(|| format!("no VPN connection {id}"))?;
                let path = OwnedObjectPath::try_from(saved.path.clone()).map_err(|e| e.to_string())?;
                conn.call_method(Some(NM), NM_PATH, Some(NM), "ActivateConnection", &(path, root_path(), root_path())).map_err(|e| e.to_string())?;
                Ok(json!({"id": id, "name": saved.id, "connecting": true}))
            }
            "vpn_disconnect" => {
                let id = args["id"].as_str().ok_or("id")?;
                let (path, _) = active_connections(&conn).into_iter().find(|(_, uuid)| uuid == id).ok_or_else(|| format!("VPN {id} is not active"))?;
                let path = OwnedObjectPath::try_from(path).map_err(|e| e.to_string())?;
                conn.call_method(Some(NM), NM_PATH, Some(NM), "DeactivateConnection", &(path,)).map_err(|e| e.to_string())?;
                Ok(json!({"id": id, "active": false}))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vpn_and_wired_lists_come_from_the_saved_connections_and_devices() {
        let saved = vec![
            SavedConnection { path: "/c/1".into(), uuid: "aaa".into(), id: "Office".into(), kind: "vpn".into() },
            SavedConnection { path: "/c/2".into(), uuid: "bbb".into(), id: "Home WG".into(), kind: "wireguard".into() },
            SavedConnection { path: "/c/3".into(), uuid: "ccc".into(), id: "Wired connection 1".into(), kind: "802-3-ethernet".into() },
        ];
        let vpn = vpn_entries(&saved, &["bbb".into(), "ccc".into()]);
        assert_eq!(vpn, vec![json!({"id": "aaa", "name": "Office", "type": "vpn", "active": false}), json!({"id": "bbb", "name": "Home WG", "type": "wireguard", "active": true})]);
        assert!(is_vpn("wireguard") && !is_vpn("802-11-wireless"));
        let devices = vec![
            json!({"path": "/d/1", "interface": "enp3s0", "type": "ethernet", "state": "connected", "connection": "Wired connection 1", "addresses": ["10.0.0.2"]}),
            json!({"path": "/d/2", "interface": "wlp2s0", "type": "wifi", "state": "disconnected", "connection": "", "addresses": []}),
        ];
        assert_eq!(wired_entries(&devices), vec![json!({"path": "/d/1", "interface": "enp3s0", "state": "connected", "connection": "Wired connection 1"})]);
    }
}
