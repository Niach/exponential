//! The settings pages' data (#EDE-14): the compositor's outputs with the
//! options a page picks from, `compositor.toml` read and written for the
//! Input and Theme pages and the `[[outputs]]` entries, the person, and
//! the derived texts the pages show. Writes keep the file's other keys:
//! the TOML is edited as a table, never regenerated from a struct.

use std::path::PathBuf;

use serde_json::{json, Value};

/// `$XDG_CONFIG_HOME/exponential-desktop/shell.json`: what the shell keeps
/// for itself (#EDE-50): the taskbar's pins, the rail.
pub fn shell_path() -> PathBuf {
    config_path().with_file_name("shell.json")
}

/// The shell's own file as JSON (an object; `{}` when there is none).
pub fn shell_json() -> Value {
    std::fs::read_to_string(shell_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

/// Merges `changes` (an object) into the shell's own file.
pub fn persist_shell(changes: Value) {
    let mut current = shell_json();
    if let (Some(cur), Some(ch)) = (current.as_object_mut(), changes.as_object()) {
        for (k, v) in ch {
            cur.insert(k.clone(), v.clone());
        }
    }
    let path = shell_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&current).unwrap_or_default()) {
        tracing::warn!(path = %path.display(), "shell.json: {e}");
    }
}

/// `$XDG_CONFIG_HOME/exponential-desktop/compositor.toml`.
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("exponential-desktop").join("compositor.toml")
}

pub(crate) fn read_table() -> toml::Table {
    std::fs::read_to_string(config_path()).ok().and_then(|t| t.parse::<toml::Table>().ok()).unwrap_or_default()
}

fn write_table(table: &toml::Table) -> Result<(), String> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, toml::to_string(table).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// What the Input and Theme pages bind, with the compositor's defaults.
pub fn config_json() -> Value {
    let t = read_table();
    let kb = t.get("keyboard").and_then(|v| v.as_table()).cloned().unwrap_or_default();
    let theme = t.get("theme").and_then(|v| v.as_table()).cloned().unwrap_or_default();
    let diagnostics = t.get("diagnostics").and_then(|v| v.as_table()).cloned().unwrap_or_default();
    json!({
        "keyboard": {
            "layout": kb.get("layout").and_then(|v| v.as_str()).filter(|l| !l.is_empty()).unwrap_or("us"),
            "repeat_rate": kb.get("repeat_rate").and_then(|v| v.as_integer()).unwrap_or(30),
            "repeat_delay": kb.get("repeat_delay").and_then(|v| v.as_integer()).unwrap_or(400),
        },
        "theme": {
            "bridge": theme.get("bridge").and_then(|v| v.as_bool()).unwrap_or(true),
            // What the bridge resolved (#EDE-19): the installed themes and fonts it picked.
            "summary": theme_summary(),
        },
        // #LOGS-2: Settings › About's "Send diagnostics" (ede-observability reads it).
        "diagnostics": {
            "send": diagnostics.get("send").and_then(|v| v.as_bool()).unwrap_or(true),
        },
    })
}

/// The bridge's choice as one line; the demo never reads the machine.
fn theme_summary() -> String {
    if std::env::var_os("EDE_SHELL_DEMO").is_some() {
        return "GTK Adwaita-dark · icons Papirus-Dark · cursor Adwaita 24 px · Inter 10 · Qt via qt6ct".into();
    }
    let paths = ede_theme::Paths::from_env();
    ede_theme::Choice::resolve(&ede_theme::Overrides::read(&paths), &ede_theme::Installed::scan(&paths)).summary()
}

/// `ede_config {keyboard: {layout?, repeat_rate?, repeat_delay?}, theme: {bridge?}, diagnostics: {send?}}`;
/// `ede_settings` (settings_pages) adds `windows`.
pub fn persist_config(args: &Value) -> Result<(), String> {
    let mut t = read_table();
    for section in ["keyboard", "theme", "windows", "diagnostics"] {
        let Some(changes) = args.get(section).and_then(Value::as_object) else { continue };
        let entry = t.entry(section).or_insert(toml::Value::Table(toml::Table::new()));
        let Some(table) = entry.as_table_mut() else { return Err(format!("[{section}] is not a table")) };
        for (k, v) in changes {
            let value = match v {
                Value::String(s) => toml::Value::String(s.clone()),
                Value::Bool(b) => toml::Value::Boolean(*b),
                Value::Number(n) if n.is_i64() => toml::Value::Integer(n.as_i64().unwrap()),
                Value::Number(n) => toml::Value::Float(n.as_f64().unwrap_or(0.0)),
                _ => continue,
            };
            table.insert(k.clone(), value);
        }
    }
    write_table(&t)
}

/// `[[outputs]]` for `name`: the mode and/or scale, the rest kept.
pub fn persist_output(name: &str, mode: Option<&str>, scale: Option<f64>) -> Result<(), String> {
    let mut t = read_table();
    let outputs = t.entry("outputs").or_insert(toml::Value::Array(Vec::new()));
    let Some(list) = outputs.as_array_mut() else { return Err("outputs is not a list".into()) };
    let entry = match list.iter_mut().find(|o| o.get("name").and_then(|n| n.as_str()) == Some(name)) {
        Some(e) => e,
        None => {
            let mut table = toml::Table::new();
            table.insert("name".into(), toml::Value::String(name.into()));
            list.push(toml::Value::Table(table));
            list.last_mut().unwrap()
        }
    };
    let table = entry.as_table_mut().ok_or("an output entry is not a table")?;
    if let Some(m) = mode {
        table.insert("mode".into(), toml::Value::String(m.into()));
    }
    if let Some(s) = scale {
        table.insert("scale".into(), toml::Value::Float(s));
    }
    write_table(&t)
}

fn mode_label(m: &ede_protocol::ModeInfo) -> String {
    format!("{}x{}@{}", m.width, m.height, (m.refresh as f64 / 1000.0).round() as i64)
}

/// An output as the Displays page binds it.
pub fn output_json(o: &ede_protocol::OutputInfo) -> Value {
    let current = o.modes.iter().find(|m| m.current).map(mode_label).unwrap_or_default();
    let options: Vec<Value> = o.modes.iter().map(|m| json!({"label": format!("{}{}", mode_label(m), if m.preferred { " (preferred)" } else { "" }), "value": mode_label(m)})).collect();
    let scale_text = if (o.scale - o.scale.round()).abs() < 0.01 { format!("{}", o.scale.round() as i64) } else { format!("{}", o.scale) };
    json!({
        "name": o.name,
        "summary": format!("{} {} · {}×{} at {}× ({}, {})", o.make, o.model, o.width, o.height, o.scale, o.x, o.y).trim().to_string(),
        "current": current,
        "modeOptions": options,
        "scale": o.scale,
        "scaleText": scale_text,
        "width": o.width,
        "height": o.height,
    })
}

/// The person: login and full name from the password database.
pub fn user_json() -> Value {
    let login = std::env::var("USER").unwrap_or_default();
    // SAFETY: getpwuid returns a static buffer or null; the strings are copied out.
    let name = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() { String::new() } else { std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().split(',').next().unwrap_or("").to_string() }
    };
    let host = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
    json!({"login": login, "name": if name.is_empty() { login.clone() } else { name }, "host": host})
}

/// Texts the pages show, derived from a snapshot.
pub fn decorate(domain: &str, mut snapshot: Value) -> Value {
    match domain {
        // The sound applet's output picker (#EDE-43): the sinks as Select
        // options, the default one as the value (a Select holds strings).
        "sound" => {
            let sinks = snapshot["sinks"].as_array().cloned().unwrap_or_default();
            snapshot["sinkOptions"] = Value::Array(sinks.iter().map(|s| json!({"label": s["name"], "value": s["id"].as_u64().map(|id| id.to_string()).unwrap_or_default()})).collect());
            snapshot["defaultSink"] = json!(sinks.iter().find(|s| s["default"] == true).and_then(|s| s["id"].as_u64()).map(|id| id.to_string()).unwrap_or_default());
        }
        // The seek slider's end label: `1:23 / 8:05`.
        "media" => {
            if let Some(players) = snapshot["players"].as_array_mut() {
                for p in players.iter_mut() {
                    let text = format!("{} / {}", minutes_seconds(p["position"].as_i64().unwrap_or(0)), minutes_seconds(p["length"].as_i64().unwrap_or(0)));
                    p["positionText"] = json!(text);
                }
            }
        }
        // `power` carries its own `batteryText` since #EDE-43; an older
        // daemon's snapshot gets it here.
        "power" => {
            if snapshot.get("batteryText").is_none() {
                let b = &snapshot["battery"];
                let text = match (b["present"].as_bool() == Some(true), b["timeText"].as_str().unwrap_or("")) {
                    (false, _) => "No battery".to_string(),
                    (true, "") => format!("{}%", b["percent"].as_u64().unwrap_or(0)),
                    (true, time) => format!("{}% · {time}", b["percent"].as_u64().unwrap_or(0)),
                };
                snapshot["batteryText"] = json!(text);
            }
        }
        "updates" => {
            let n = snapshot["count"].as_u64().unwrap_or(0);
            snapshot["countText"] = json!(if n == 0 { "Up to date".to_string() } else { n.to_string() });
        }
        "about" => {
            let mb = snapshot["memoryMb"].as_u64().unwrap_or(0);
            snapshot["memoryText"] = json!(format!("{:.1} GB", mb as f64 / 1024.0));
        }
        _ => {}
    }
    snapshot
}

/// `m:ss` of a length in seconds (the media card).
fn minutes_seconds(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn demo_services() -> Value {
    let mut v = json!({
        "network": {"available": true, "state": "global", "connected": true, "primary": "802-11-wireless", "wifiEnabled": true, "wiredConnected": false,
            "devices": [{"path": "/dev/1", "interface": "wlp3s0", "type": "wifi", "state": "connected", "connection": "Home", "addresses": ["192.168.1.23"]}, {"path": "/dev/2", "interface": "enp0s31f6", "type": "ethernet", "state": "disconnected", "connection": "", "addresses": []}],
            "wired": [{"path": "/dev/2", "interface": "enp0s31f6", "state": "disconnected", "connection": ""}],
            "accessPoints": [{"ssid": "Home", "strength": 82, "secured": true, "active": true, "device": "/dev/1"}, {"ssid": "Cafe Nero", "strength": 47, "secured": false, "active": false, "device": "/dev/1"}, {"ssid": "FRITZ!Box 7590", "strength": 31, "secured": true, "active": false, "device": "/dev/1"}],
            "vpn": [{"id": "6f2b1c0e-office", "name": "Office", "type": "vpn", "active": false}, {"id": "9a4d7e21-home", "name": "Home WG", "type": "wireguard", "active": true}]},
        "sound": {"available": true, "volume": 40, "muted": false, "micVolume": 100, "micMuted": false,
            "sinks": [{"id": 55, "name": "Built-in Audio Analog Stereo", "volume": 40, "muted": false, "default": true}, {"id": 57, "name": "HDMI Out", "volume": 100, "muted": false, "default": false}], "sources": [{"id": 56, "name": "Built-in Microphone", "volume": 100, "muted": false, "default": true}],
            "streams": [{"id": 71, "app": "Firefox", "name": "Kurzgesagt – In a Nutshell", "volume": 80, "muted": false}, {"id": 74, "app": "Spotify", "name": "Spotify", "volume": 65, "muted": false}]},
        "media": {"available": true,
            "players": [{"name": "org.mpris.MediaPlayer2.spotify", "identity": "Spotify", "status": "playing", "title": "Weightless", "artist": "Marconi Union", "album": "Weightless (Ambient Transmissions Vol. 2)", "artUrl": "", "position": 83, "length": 485, "canGoNext": true, "canGoPrevious": true, "canSeek": true}],
            "active": {"name": "org.mpris.MediaPlayer2.spotify", "identity": "Spotify", "status": "playing", "title": "Weightless", "artist": "Marconi Union", "album": "Weightless (Ambient Transmissions Vol. 2)", "artUrl": "", "position": 83, "length": 485, "canGoNext": true, "canGoPrevious": true, "canSeek": true}},
        "bluetooth": {"available": true, "powered": true, "discovering": false, "adapters": [{"path": "/org/bluez/hci0", "name": "hci0", "powered": true, "discovering": false}], "devices": [{"path": "/bt/1", "name": "Keyboard K380", "address": "F4:73:35:12:AB:01", "paired": true, "connected": true, "trusted": true, "icon": "input-keyboard"}, {"path": "/bt/2", "name": "WH-1000XM4", "address": "38:18:4C:00:11:22", "paired": true, "connected": false, "trusted": true, "icon": "audio-headset"}]},
        "power": {"available": true, "battery": {"present": true, "percent": 73, "state": "discharging", "timeText": "2:10 left", "secondsToEmpty": 7800, "secondsToFull": 0}, "batteryText": "73% · 2:10 left", "onBattery": true, "profile": "balanced", "profiles": ["power-saver", "balanced", "performance"], "canSuspend": true, "canHibernate": false, "canReboot": true, "canPowerOff": true},
        "updates": {"available": true, "count": 2, "security": 1, "packages": [{"name": "firefox", "suite": "noble-security", "version": "130.0", "from": "129.0"}, {"name": "vim", "suite": "noble", "version": "2:9.1", "from": "2:9.0"}]},
        "printers": {"available": true, "printers": [{"name": "HP_LaserJet", "state": "idle", "default": true}]},
        "about": {"available": true, "hostname": "ubuntu-wayland", "os": "Ubuntu 26.04 LTS", "kernel": "7.0.0-38-generic", "cpu": "13th Gen Intel Core i9-13900K", "cpus": 6, "memoryMb": 7423, "ede": "0.2.0", "checking": false, "checks": [{"name": "render", "level": "ok", "status": "OK", "detail": "renderD128 (virtio_gpu)"}, {"name": "seat", "level": "ok", "status": "OK", "detail": "logind session 2 on seat0"}, {"name": "Xwayland", "level": "ok", "status": "OK", "detail": "/usr/bin/Xwayland 24.1.10, explicit sync"}]},
    });
    for d in ["sound", "media", "power", "updates", "about"] {
        let s = v[d].clone();
        v[d] = decorate(d, s);
    }
    v
}

pub fn demo_outputs() -> Vec<Value> {
    vec![json!({"name": "HEADLESS-1", "summary": "Exponential Desktop (headless) · 1280×720 at 1× (0, 0)", "current": "1280x720@60", "modeOptions": [{"label": "1280x720@60 (preferred)", "value": "1280x720@60"}], "scale": 1.0, "scaleText": "1", "width": 1280, "height": 720})]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_edits_keep_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        std::fs::create_dir_all(dir.path().join("exponential-desktop")).unwrap();
        std::fs::write(config_path(), "background = \"#102030\"\n[windows]\nsnap_threshold = 4\n").unwrap();
        persist_config(&json!({"keyboard": {"layout": "de", "repeat_rate": 40}})).unwrap();
        persist_output("DP-1", Some("2560x1440@60"), Some(1.5)).unwrap();
        persist_output("DP-1", None, Some(2.0)).unwrap();
        let text = std::fs::read_to_string(config_path()).unwrap();
        assert!(text.contains("background = \"#102030\""), "{text}");
        assert!(text.contains("snap_threshold = 4"));
        assert!(text.contains("layout = \"de\"") && text.contains("repeat_rate = 40"));
        assert!(text.contains("mode = \"2560x1440@60\"") && text.contains("scale = 2.0"), "{text}");
        assert_eq!(text.matches("name = \"DP-1\"").count(), 1);
        assert_eq!(config_json()["keyboard"]["layout"], "de");
        assert_eq!(config_json()["keyboard"]["repeat_delay"], 400, "the default when unset");
        // #LOGS-2: Send diagnostics, on until switched off.
        assert_eq!(config_json()["diagnostics"]["send"], true);
        persist_config(&json!({"diagnostics": {"send": false}})).unwrap();
        assert!(std::fs::read_to_string(config_path()).unwrap().contains("[diagnostics]\nsend = false"));
        assert_eq!(config_json()["diagnostics"]["send"], false);
    }

    #[test]
    fn pages_get_their_texts() {
        // `power` carries its own `batteryText` since #EDE-43: passed through.
        assert_eq!(decorate("power", json!({"batteryText": "No battery"}))["batteryText"], "No battery");
        assert_eq!(decorate("updates", json!({"count": 0}))["countText"], "Up to date");
        assert_eq!(decorate("about", json!({"memoryMb": 7423}))["memoryText"], "7.2 GB");
        assert_eq!(decorate("power", json!({"battery": {"present": true, "percent": 73, "timeText": "2:10 left"}, "batteryText": "daemon's"}))["batteryText"], "daemon's");
        let sound = decorate("sound", json!({"sinks": [{"id": 55, "name": "Built-in", "default": true}, {"id": 57, "name": "HDMI", "default": false}]}));
        assert_eq!(sound["sinkOptions"], json!([{"label": "Built-in", "value": "55"}, {"label": "HDMI", "value": "57"}]));
        assert_eq!(sound["defaultSink"], "55");
        assert_eq!(decorate("media", json!({"players": [{"position": 83, "length": 485}]}))["players"][0]["positionText"], "1:23 / 8:05");
    }

    /// #EDE-43: the applets' goldens show every section.
    #[test]
    fn the_demo_feeds_every_applet() {
        let s = demo_services();
        assert!(!s["sound"]["streams"].as_array().unwrap().is_empty() && !s["sound"]["sinkOptions"].as_array().unwrap().is_empty());
        assert_eq!(s["media"]["players"][0]["positionText"], "1:23 / 8:05");
        assert!(!s["network"]["vpn"].as_array().unwrap().is_empty() && !s["network"]["wired"].as_array().unwrap().is_empty());
        assert_eq!(s["bluetooth"]["discovering"], false);
        assert_eq!(s["power"]["batteryText"], "73% · 2:10 left");
    }
}
