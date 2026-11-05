//! The domains. Each talks to its daemon over the system bus (zbus,
//! blocking) or a command the distribution ships, and answers
//! `unavailable` when it is not there, so a page can say so.

pub mod about;
pub mod bluetooth;
pub mod disks;
pub mod firmware;
pub mod network;
pub mod power;
pub mod printers;
pub mod sound;
pub mod updates;

use std::collections::HashMap;

use zbus::{blocking::Connection, zvariant::{OwnedObjectPath, OwnedValue}};

use crate::Service;

pub fn all() -> Vec<Box<dyn Service>> {
    vec![
        Box::new(network::Network),
        Box::new(sound::Sound),
        Box::new(bluetooth::Bluetooth),
        Box::new(power::Power),
        Box::new(disks::Disks),
        Box::new(updates::Updates),
        Box::new(firmware::Firmware),
        Box::new(printers::Printers),
        Box::new(about::About),
    ]
}

/// `org.freedesktop.DBus.ObjectManager.GetManagedObjects` of `dest` at `path`.
pub type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

pub fn managed_objects(conn: &Connection, dest: &str, path: &str) -> Result<Managed, String> {
    conn.call_method(Some(dest), path, Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &()).and_then(|r| r.body().deserialize::<Managed>()).map_err(|e| e.to_string())
}

pub fn system_bus() -> Result<Connection, String> {
    Connection::system().map_err(|e| format!("no system bus: {e}"))
}

/// A property read as `T`.
pub fn prop<T: TryFrom<OwnedValue>>(conn: &Connection, dest: &str, path: &str, iface: &str, name: &str) -> Option<T> {
    let reply = conn.call_method(Some(dest), path, Some("org.freedesktop.DBus.Properties"), "Get", &(iface, name)).ok()?;
    let value: OwnedValue = reply.body().deserialize().ok()?;
    T::try_from(value).ok()
}

pub fn set_prop<'a>(conn: &Connection, dest: &str, path: &str, iface: &str, name: &str, value: zbus::zvariant::Value<'a>) -> Result<(), String> {
    conn.call_method(Some(dest), path, Some("org.freedesktop.DBus.Properties"), "Set", &(iface, name, value)).map(|_| ()).map_err(|e| e.to_string())
}

pub fn str_of(props: &HashMap<String, OwnedValue>, key: &str) -> String {
    props.get(key).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default()
}

pub fn bool_of(props: &HashMap<String, OwnedValue>, key: &str) -> bool {
    props.get(key).and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false)
}

pub fn u64_of(props: &HashMap<String, OwnedValue>, key: &str) -> u64 {
    props.get(key).and_then(|v| u64::try_from(v.clone()).ok().or_else(|| u32::try_from(v.clone()).ok().map(u64::from))).unwrap_or(0)
}

/// A command's stdout, `None` when it is missing or failed.
pub fn command(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
