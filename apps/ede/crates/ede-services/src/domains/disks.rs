//! Disks over udisks2: drives and their filesystems with mount points;
//! mount and unmount.

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::OwnedValue;

use super::{bool_of, by_path, call_asking, managed_objects, polled_system_bus, str_of, system_bus, u64_of, Managed};
use crate::{unavailable, ActionSpec, Service};

const UDISKS: &str = "org.freedesktop.UDisks2";

pub struct Disks;

fn bytes_string(props: &std::collections::HashMap<String, OwnedValue>, key: &str) -> String {
    props.get(key).and_then(|v| Vec::<u8>::try_from(v.clone()).ok()).map(|b| String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(&b)).into_owned()).unwrap_or_default()
}

/// The drives and the person's filesystems.
pub fn rows(objects: &Managed) -> (Vec<Value>, Vec<Value>) {
    let mut drives = Vec::new();
    let mut volumes = Vec::new();
    for (path, ifaces) in by_path(objects) {
        if let Some(d) = ifaces.get("org.freedesktop.UDisks2.Drive") {
            drives.push(json!({"path": path.as_str(), "vendor": str_of(d, "Vendor"), "model": str_of(d, "Model"), "removable": bool_of(d, "Removable"), "ejectable": bool_of(d, "Ejectable"), "size": u64_of(d, "Size")}));
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
    (drives, volumes)
}

fn no_options() -> std::collections::HashMap<&'static str, zbus::zvariant::Value<'static>> {
    std::collections::HashMap::new()
}

/// #EDE-245: Cinnamon's removable-drives applet: the mounted filesystems of
/// removable or ejectable drives (USB sticks, SD cards, discs), named by
/// label, else the mount point's last segment.
pub fn removable(drives: &[Value], volumes: &[Value]) -> Vec<Value> {
    volumes
        .iter()
        .filter(|v| v["mounted"] == true)
        .filter(|v| drives.iter().any(|d| d["path"] == v["drive"] && (d["removable"] == true || d["ejectable"] == true)))
        .map(|v| {
            let at = v["mountPoints"][0].as_str().unwrap_or_default();
            let label = v["label"].as_str().filter(|l| !l.is_empty()).map(String::from).unwrap_or_else(|| at.rsplit('/').next().unwrap_or(at).to_string());
            json!({"volume": v["path"], "name": label, "mountPoint": at, "uri": format!("file://{at}")})
        })
        .collect()
}

impl Service for Disks {
    fn name(&self) -> &'static str {
        "disks"
    }
    fn description(&self) -> &'static str {
        "Drives and filesystems through udisks2."
    }

    fn snapshot(&self) -> Value {
        let Ok(conn) = polled_system_bus() else { return unavailable("no system bus") };
        let Ok(objects) = managed_objects(&conn, UDISKS, "/org/freedesktop/UDisks2") else { return unavailable("udisks2 is not running") };
        let (drives, volumes) = rows(&objects);
        let removable = removable(&drives, &volumes);
        json!({"available": true, "drives": drives, "volumes": volumes, "removable": removable})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("mount", "Mount a filesystem (its object path from the snapshot).", json!({"volume": {"type": "string"}}), &["volume"], Some(Capability::System)),
            ActionSpec::new("unmount", "Unmount a filesystem.", json!({"volume": {"type": "string"}}), &["volume"], Some(Capability::System)),
            ActionSpec::new("eject", "Unmount a removable filesystem and eject (else power off) its drive, as Cinnamon's removable-drives applet does.", json!({"volume": {"type": "string"}}), &["volume"], Some(Capability::System)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let conn = system_bus()?;
        let volume = args["volume"].as_str().ok_or("volume")?;
        let options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> = std::collections::HashMap::new();
        match action {
            "mount" => {
                let at: String = call_asking(&conn, UDISKS, volume, "org.freedesktop.UDisks2.Filesystem", "Mount", &(options,))?;
                Ok(json!({"mounted": at}))
            }
            "unmount" => {
                call_asking::<_, ()>(&conn, UDISKS, volume, "org.freedesktop.UDisks2.Filesystem", "Unmount", &(options,))?;
                Ok(json!({"unmounted": volume}))
            }
            "eject" => {
                let drive = managed_objects(&conn, UDISKS, "/org/freedesktop/UDisks2")?
                    .iter()
                    .find(|(p, _)| p.as_str() == volume)
                    .and_then(|(_, ifaces)| ifaces.get("org.freedesktop.UDisks2.Block")?.get("Drive").cloned())
                    .and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v).ok())
                    .ok_or("no drive for that volume")?;
                call_asking::<_, ()>(&conn, UDISKS, volume, "org.freedesktop.UDisks2.Filesystem", "Unmount", &(no_options(),))?;
                let ejected = call_asking::<_, ()>(&conn, UDISKS, drive.as_str(), "org.freedesktop.UDisks2.Drive", "Eject", &(no_options(),));
                if ejected.is_err() {
                    call_asking::<_, ()>(&conn, UDISKS, drive.as_str(), "org.freedesktop.UDisks2.Drive", "PowerOff", &(no_options(),))?;
                }
                Ok(json!({"ejected": volume}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{OwnedObjectPath, Value as ZValue};

    /// #EDE-135: the same objects in another order give the same rows.
    #[test]
    fn the_rows_do_not_follow_the_maps_order() {
        let props = |list: Vec<(&str, ZValue<'static>)>| -> std::collections::HashMap<String, OwnedValue> { list.into_iter().map(|(k, v)| (k.to_string(), OwnedValue::try_from(v).unwrap())).collect() };
        let entry = |i: usize| {
            let mut ifaces = std::collections::HashMap::new();
            if i % 2 == 0 {
                ifaces.insert("org.freedesktop.UDisks2.Drive".to_string(), props(vec![("Model", ZValue::from(format!("Disk {i}")))]));
            } else {
                ifaces.insert("org.freedesktop.UDisks2.Block".to_string(), props(vec![("Device", ZValue::from(format!("/dev/sd{i}\0").into_bytes())), ("IdLabel", ZValue::from(format!("Vol {i}")))]));
                ifaces.insert("org.freedesktop.UDisks2.Filesystem".to_string(), props(vec![]));
            }
            (OwnedObjectPath::try_from(format!("/org/freedesktop/UDisks2/x/n{i:02}")).unwrap(), ifaces)
        };
        let forward: Managed = (0..16).map(entry).collect();
        let backward: Managed = (0..16).rev().map(entry).collect();
        let (drives, volumes) = rows(&forward);
        assert_eq!((drives.len(), volumes.len()), (8, 8));
        assert_eq!(json!(rows(&forward)), json!(rows(&backward)));
        assert_eq!(volumes[0]["label"], "Vol 1");
    }

    /// #EDE-245: only mounted filesystems of removable or ejectable drives.
    #[test]
    fn removable_lists_the_mounted_filesystems_of_removable_drives() {
        let drives = vec![json!({"path": "/d/usb", "removable": true, "ejectable": false}), json!({"path": "/d/nvme", "removable": false, "ejectable": false}), json!({"path": "/d/dvd", "removable": false, "ejectable": true})];
        let vol = |path: &str, drive: &str, label: &str, at: Option<&str>| json!({"path": path, "drive": drive, "label": label, "mounted": at.is_some(), "mountPoints": at.into_iter().collect::<Vec<_>>()});
        let volumes = vec![vol("/b/sda1", "/d/usb", "STICK", Some("/media/me/STICK")), vol("/b/sda2", "/d/usb", "", Some("/media/me/1234-ABCD")), vol("/b/sdb1", "/d/usb", "OFF", None), vol("/b/nvme", "/d/nvme", "root", Some("/")), vol("/b/sr0", "/d/dvd", "Film", Some("/media/me/Film"))];
        let names: Vec<&str> = Vec::leak(removable(&drives, &volumes)).iter().map(|r| r["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["STICK", "1234-ABCD", "Film"]);
        assert_eq!(removable(&drives, &volumes)[0]["uri"], "file:///media/me/STICK");
    }
}
