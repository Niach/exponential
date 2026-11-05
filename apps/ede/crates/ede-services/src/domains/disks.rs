//! Disks over udisks2: drives and their filesystems with mount points;
//! mount and unmount.

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::OwnedValue;

use super::{bool_of, managed_objects, str_of, system_bus, u64_of};
use crate::{unavailable, ActionSpec, Service};

const UDISKS: &str = "org.freedesktop.UDisks2";

pub struct Disks;

fn bytes_string(props: &std::collections::HashMap<String, OwnedValue>, key: &str) -> String {
    props.get(key).and_then(|v| Vec::<u8>::try_from(v.clone()).ok()).map(|b| String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(&b)).into_owned()).unwrap_or_default()
}

impl Service for Disks {
    fn name(&self) -> &'static str {
        "disks"
    }
    fn description(&self) -> &'static str {
        "Drives and filesystems through udisks2."
    }

    fn snapshot(&self) -> Value {
        let Ok(conn) = system_bus() else { return unavailable("no system bus") };
        let Ok(objects) = managed_objects(&conn, UDISKS, "/org/freedesktop/UDisks2") else { return unavailable("udisks2 is not running") };
        let mut drives = Vec::new();
        let mut volumes = Vec::new();
        for (path, ifaces) in &objects {
            if let Some(d) = ifaces.get("org.freedesktop.UDisks2.Drive") {
                drives.push(json!({"path": path.as_str(), "vendor": str_of(d, "Vendor"), "model": str_of(d, "Model"), "removable": bool_of(d, "Removable"), "size": u64_of(d, "Size")}));
            }
            if let Some(b) = ifaces.get("org.freedesktop.UDisks2.Block") {
                let device = bytes_string(b, "Device");
                // Loop devices (snaps) and ignored blocks are not the person's disks.
                if bool_of(b, "HintIgnore") || device.starts_with("/dev/loop") || !ifaces.contains_key("org.freedesktop.UDisks2.Filesystem") {
                    continue;
                }
                let mounts: Vec<String> = ifaces
                    .get("org.freedesktop.UDisks2.Filesystem")
                    .and_then(|f| f.get("MountPoints"))
                    .and_then(|v| Vec::<Vec<u8>>::try_from(v.clone()).ok())
                    .map(|list| list.iter().map(|m| String::from_utf8_lossy(m.strip_suffix(&[0]).unwrap_or(m)).into_owned()).collect())
                    .unwrap_or_default();
                volumes.push(json!({"path": path.as_str(), "device": device, "label": str_of(b, "IdLabel"), "type": str_of(b, "IdType"), "size": u64_of(b, "Size"), "drive": b.get("Drive").and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v.clone()).ok()).map(|p| p.as_str().to_string()).unwrap_or_default(), "mounted": !mounts.is_empty(), "mountPoints": mounts}));
            }
        }
        json!({"available": true, "drives": drives, "volumes": volumes})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("mount", "Mount a filesystem (its object path from the snapshot).", json!({"volume": {"type": "string"}}), &["volume"], Some(Capability::System)),
            ActionSpec::new("unmount", "Unmount a filesystem.", json!({"volume": {"type": "string"}}), &["volume"], Some(Capability::System)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let conn = system_bus()?;
        let volume = args["volume"].as_str().ok_or("volume")?;
        let options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> = std::collections::HashMap::new();
        match action {
            "mount" => {
                let reply = conn.call_method(Some(UDISKS), volume, Some("org.freedesktop.UDisks2.Filesystem"), "Mount", &(options,)).map_err(|e| e.to_string())?;
                let at: String = reply.body().deserialize().unwrap_or_default();
                Ok(json!({"mounted": at}))
            }
            "unmount" => {
                conn.call_method(Some(UDISKS), volume, Some("org.freedesktop.UDisks2.Filesystem"), "Unmount", &(options,)).map_err(|e| e.to_string())?;
                Ok(json!({"unmounted": volume}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}
