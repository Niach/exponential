//! Power: the battery through UPower, the profile through
//! power-profiles-daemon, suspend/reboot/power off through logind.

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::zvariant::{OwnedValue, Value as ZValue};

use super::{prop, set_prop, system_bus};
use crate::{unavailable, ActionSpec, Service};

const UPOWER: &str = "org.freedesktop.UPower";
const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const PROFILES: &str = "org.freedesktop.UPower.PowerProfiles";
const PROFILES_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const LOGIN: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

pub struct Power;

pub fn battery_state(s: u32) -> &'static str {
    match s {
        1 => "charging",
        2 => "discharging",
        3 => "empty",
        4 => "full",
        5 => "pending-charge",
        6 => "pending-discharge",
        _ => "unknown",
    }
}

impl Service for Power {
    fn name(&self) -> &'static str {
        "power"
    }
    fn description(&self) -> &'static str {
        "Battery, power profile, suspend, restart and shutdown (UPower, power-profiles-daemon, logind)."
    }

    fn snapshot(&self) -> Value {
        let Ok(conn) = system_bus() else { return unavailable("no system bus") };
        let present = prop::<bool>(&conn, UPOWER, DISPLAY, DEVICE, "IsPresent");
        let battery = match present {
            Some(true) => json!({
                "present": true,
                "percent": prop::<f64>(&conn, UPOWER, DISPLAY, DEVICE, "Percentage").unwrap_or(0.0).round() as u32,
                "state": battery_state(prop::<u32>(&conn, UPOWER, DISPLAY, DEVICE, "State").unwrap_or(0)),
                "secondsToEmpty": prop::<i64>(&conn, UPOWER, DISPLAY, DEVICE, "TimeToEmpty").unwrap_or(0),
                "secondsToFull": prop::<i64>(&conn, UPOWER, DISPLAY, DEVICE, "TimeToFull").unwrap_or(0),
            }),
            Some(false) => json!({"present": false}),
            None => json!({"present": false, "reason": "UPower is not running"}),
        };
        let on_battery = prop::<bool>(&conn, UPOWER, "/org/freedesktop/UPower", UPOWER, "OnBattery").unwrap_or(false);
        let profile = prop::<String>(&conn, PROFILES, PROFILES_PATH, PROFILES, "ActiveProfile");
        let profiles: Vec<String> = prop::<Vec<std::collections::HashMap<String, OwnedValue>>>(&conn, PROFILES, PROFILES_PATH, PROFILES, "Profiles")
            .map(|list| list.iter().map(|m| super::str_of(m, "Profile")).filter(|p| !p.is_empty()).collect())
            .unwrap_or_default();
        let can = |what: &str| -> bool {
            conn.call_method(Some(LOGIN), LOGIN_PATH, Some(MANAGER), what, &()).and_then(|r| r.body().deserialize::<String>()).map(|s| s == "yes").unwrap_or(false)
        };
        json!({
            "available": true,
            "battery": battery,
            "onBattery": on_battery,
            "profile": profile.unwrap_or_default(),
            "profiles": profiles,
            "canSuspend": can("CanSuspend"),
            "canHibernate": can("CanHibernate"),
            "canReboot": can("CanReboot"),
            "canPowerOff": can("CanPowerOff"),
        })
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("set_profile", "The power profile: power-saver, balanced or performance.", json!({"profile": {"type": "string"}}), &["profile"], Some(Capability::System)),
            ActionSpec::new("suspend", "Suspend to RAM.", json!({}), &[], Some(Capability::System)),
            ActionSpec::new("hibernate", "Hibernate.", json!({}), &[], Some(Capability::System)),
            ActionSpec::new("reboot", "Restart the machine.", json!({}), &[], Some(Capability::System)),
            ActionSpec::new("poweroff", "Shut the machine down.", json!({}), &[], Some(Capability::System)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let conn = system_bus()?;
        match action {
            "set_profile" => {
                let profile = args["profile"].as_str().ok_or("profile")?;
                set_prop(&conn, PROFILES, PROFILES_PATH, PROFILES, "ActiveProfile", ZValue::from(profile))?;
                Ok(json!({"profile": profile}))
            }
            "suspend" | "hibernate" | "reboot" | "poweroff" => {
                let method = match action {
                    "suspend" => "Suspend",
                    "hibernate" => "Hibernate",
                    "reboot" => "Reboot",
                    _ => "PowerOff",
                };
                conn.call_method(Some(LOGIN), LOGIN_PATH, Some(MANAGER), method, &(false,)).map_err(|e| e.to_string())?;
                Ok(json!({"action": action}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}
