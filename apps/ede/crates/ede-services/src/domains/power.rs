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

/// UPower's state as the applet shows it: `charging`, `discharging`,
/// `full`, `empty` or `unknown` (a pending charge or discharge counts as
/// the one it is waiting for).
pub fn battery_state(s: u32) -> &'static str {
    match s {
        1 | 5 => "charging",
        2 | 6 => "discharging",
        3 => "empty",
        4 => "full",
        _ => "unknown",
    }
}

/// `h:mm` of a duration in seconds.
fn hours_minutes(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 30) / 60;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// What the battery applet says under the percentage (#EDE-43): the time
/// left while discharging, the time to full while charging, nothing when
/// UPower has no estimate yet.
pub fn time_text(state: &str, seconds_to_empty: i64, seconds_to_full: i64) -> String {
    match state {
        "discharging" if seconds_to_empty > 0 => format!("{} left", hours_minutes(seconds_to_empty)),
        "charging" if seconds_to_full > 0 => format!("{} until full", hours_minutes(seconds_to_full)),
        "full" => "Fully charged".into(),
        _ => String::new(),
    }
}

/// The one-line battery text: `85% · 2:10 left`, `100% · Fully charged`,
/// `No battery`.
pub fn battery_text(present: bool, percent: u32, time: &str) -> String {
    match (present, time.is_empty()) {
        (false, _) => "No battery".into(),
        (true, true) => format!("{percent}%"),
        (true, false) => format!("{percent}% · {time}"),
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
        let (battery, battery_text) = match present {
            Some(true) => {
                let percent = prop::<f64>(&conn, UPOWER, DISPLAY, DEVICE, "Percentage").unwrap_or(0.0).round() as u32;
                let state = battery_state(prop::<u32>(&conn, UPOWER, DISPLAY, DEVICE, "State").unwrap_or(0));
                let to_empty = prop::<i64>(&conn, UPOWER, DISPLAY, DEVICE, "TimeToEmpty").unwrap_or(0);
                let to_full = prop::<i64>(&conn, UPOWER, DISPLAY, DEVICE, "TimeToFull").unwrap_or(0);
                let time = time_text(state, to_empty, to_full);
                let text = battery_text(true, percent, &time);
                (json!({"present": true, "percent": percent, "state": state, "timeText": time, "secondsToEmpty": to_empty, "secondsToFull": to_full}), text)
            }
            Some(false) => (json!({"present": false, "percent": 0, "state": "unknown", "timeText": ""}), battery_text(false, 0, "")),
            None => (json!({"present": false, "percent": 0, "state": "unknown", "timeText": "", "reason": "UPower is not running"}), battery_text(false, 0, "")),
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
            "batteryText": battery_text,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_battery_texts_follow_the_state() {
        assert_eq!(battery_state(1), "charging");
        assert_eq!(battery_state(5), "charging", "pending charge shows as charging");
        assert_eq!(battery_state(6), "discharging");
        assert_eq!(battery_state(0), "unknown");
        assert_eq!(time_text("discharging", 7_800, 0), "2:10 left");
        assert_eq!(time_text("charging", 0, 2_700), "0:45 until full");
        assert_eq!(time_text("discharging", 0, 0), "", "no estimate yet");
        assert_eq!(time_text("full", 0, 0), "Fully charged");
        assert_eq!(time_text("unknown", 100, 100), "");
        assert_eq!(battery_text(true, 85, "2:10 left"), "85% · 2:10 left");
        assert_eq!(battery_text(true, 42, ""), "42%");
        assert_eq!(battery_text(false, 0, ""), "No battery");
    }
}
