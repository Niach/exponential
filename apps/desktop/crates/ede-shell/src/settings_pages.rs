//! The settings pages the shell feeds itself (#EDE-47): what no daemon
//! carries yet, as `/pages` (Keyboard's layouts to pick from, Date & Time,
//! Startup apps, Default apps, Workspaces, Windows, Mouse & Touchpad), and
//! ONE host function `ede_settings {page, key, value}` writing them:
//! `compositor.toml` keys the compositor already reads (`[keyboard]`,
//! `[windows]`; main reloads it), the XDG autostart overrides, `xdg-mime
//! default`, `timedatectl`. A setting the machine cannot apply yet is fed as
//! a value the page shows read-only (Mouse & Touchpad, the workspace count
//! and wrap-around: the compositor has no key for them, `handle` refuses
//! them). `EDE_SHELL_DEMO` feeds fixed sample data; every reader's pure part
//! works on text or a directory and is tested.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::settings;

fn demo() -> bool {
    std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn config_home() -> PathBuf {
    env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

fn config_dirs() -> Vec<PathBuf> {
    std::env::var_os("XDG_CONFIG_DIRS").filter(|v| !v.is_empty()).map(|v| std::env::split_paths(&v).collect()).unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")])
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"))];
    dirs.extend(std::env::var_os("XDG_DATA_DIRS").filter(|v| !v.is_empty()).map(|v| std::env::split_paths(&v).collect::<Vec<_>>()).unwrap_or_else(|| vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")]));
    dirs
}

/// `/pages`: every page's data, the demo's sample under `EDE_SHELL_DEMO`.
pub fn pages_json() -> Value {
    if demo() {
        return demo_pages();
    }
    let entries = desktop_entries(&data_dirs());
    json!({
        "keyboard": {"layouts": std::fs::read_to_string(XKB_LAYOUTS).map(|t| parse_layouts(&t)).unwrap_or_default()},
        "datetime": datetime_json(),
        "startup": {"entries": autostart_entries(&config_dirs().iter().map(|d| d.join("autostart")).collect::<Vec<_>>(), &config_home().join("autostart"))},
        "defaults": {"roles": default_apps(&entries, &mimeapps_defaults())},
        "workspaces": workspaces_json(),
        "windows": windows_json(&settings::read_table()),
        "mouse": mouse_json(),
    })
}

/// A golden's state under the demo: `EDE_SHELL_SETTINGS_PAGE=<page>[,switcher]`
/// feeds `/settings` so a capture opens on that page (the nav groups folded
/// and unfolded around it), `,switcher` with the Team switcher's Popover
/// open. Unset = the document keeps its own state.
pub fn demo_state() -> Option<Value> {
    let spec = std::env::var("EDE_SHELL_SETTINGS_PAGE").ok().filter(|v| !v.is_empty() && demo())?;
    Some(demo_state_for(&spec))
}

/// `EDE_SHELL_SHARE=picking|running|done|failed` (the goldens): `/sharing`.
pub fn demo_sharing() -> Option<Value> {
    let state = std::env::var("EDE_SHELL_SHARE").ok().filter(|v| !v.is_empty() && demo())?;
    let accounts = json!([{"label": "danny@example.com", "value": "demo"}, {"label": "danny@yourev.at", "value": "demo-2"}]);
    Some(match state.as_str() {
        "running" => json!({"state": "running", "accounts": accounts, "summary": "", "progress": 62, "label": "Uploading 3 of 5 files", "teamName": "", "error": ""}),
        "done" => json!({"state": "done", "accounts": accounts, "summary": "", "progress": 100, "label": "", "teamName": "Local", "error": ""}),
        "failed" => json!({"state": "failed", "accounts": accounts, "summary": "", "progress": 0, "label": "", "teamName": "", "error": "Your plan allows up to 250 MB of attachment storage. Upgrade to upload more."}),
        _ => json!({"state": "picking", "accounts": accounts, "summary": "12 issues, 31 comments, 4 sessions and 5 files (2.1 MB)", "progress": 0, "label": "", "teamName": "", "error": ""}),
    })
}

fn demo_state_for(spec: &str) -> Value {
    let (page, switcher) = spec.split_once(',').map(|(p, s)| (p, s == "switcher")).unwrap_or((spec, false));
    let coding = ["tools", "coding-clis", "worktrees", "sessions"].contains(&page);
    let personal = ["account", "devices", "security", "whats-new", "about"].contains(&page);
    let team = page.starts_with("local-") || page.starts_with("team-");
    // The groups fold like the sidebar's sections: the page's group open, the ones above a Team page folded so the switcher shows.
    json!({"page": page, "deviceOpen": !team, "personalOpen": personal || !team, "codingOpen": coding, "team": {"kind": "", "id": "", "name": ""}, "switcherOpen": switcher})
}

/// `ede_settings {page, key, value}`: `Ok(true)` when the compositor must
/// reload its configuration, `Err` when the page refuses the key.
pub fn handle(args: &Value) -> Result<bool, String> {
    let page = args.get("page").and_then(Value::as_str).ok_or("page is a string")?;
    let key = args.get("key").and_then(Value::as_str).ok_or("key is a string")?;
    let value = args.get("value").cloned().unwrap_or(Value::Null);
    match page {
        "keyboard" | "windows" => {
            settings::persist_config(&config_change(page, key, &value)?)?;
            Ok(true)
        }
        "datetime" => {
            let (verb, arg) = timedate_command(key, &value)?;
            run("timedatectl", &[verb, &arg])?;
            Ok(false)
        }
        "startup" => {
            let on = value.as_bool().ok_or("value is a boolean")?;
            autostart_toggle(&config_dirs().iter().map(|d| d.join("autostart")).collect::<Vec<_>>(), &config_home().join("autostart"), key, on)?;
            Ok(false)
        }
        "defaults" => {
            let app = value.as_str().filter(|a| !a.is_empty()).ok_or("value is a desktop entry id")?;
            let mimes = role_mimes(key).ok_or_else(|| format!("{key}: no such role"))?;
            let mut args: Vec<&str> = vec!["default", app];
            args.extend(mimes.iter().copied());
            run("xdg-mime", &args)?;
            Ok(false)
        }
        other => Err(format!("{other}: the compositor applies no such setting yet")),
    }
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{program} {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

// ---- compositor.toml: [keyboard] and [windows] ----------------------------

/// The keys a page may write, with their TOML type (`s` string, `i`
/// integer, `b` boolean): `[keyboard]` and `[windows]` as the compositor
/// reads them (`deny_unknown_fields`: nothing else goes in).
const CONFIG_KEYS: &[(&str, &str, char)] = &[
    ("keyboard", "layout", 's'),
    ("keyboard", "variant", 's'),
    ("keyboard", "repeat_rate", 'i'),
    ("keyboard", "repeat_delay", 'i'),
    ("windows", "edge_tiling", 'b'),
    ("windows", "snap_threshold", 'i'),
    ("windows", "edge_tiling_px", 'i'),
    ("windows", "corner_px", 'i'),
    ("windows", "panel_margin", 'i'),
];

/// `{section: {key: value}}` for `settings::persist_config`, the value
/// coerced to the key's type (a Slider reports `12.0`; the compositor's
/// field is an integer).
pub fn config_change(section: &str, key: &str, value: &Value) -> Result<Value, String> {
    let (_, _, ty) = CONFIG_KEYS.iter().find(|(s, k, _)| *s == section && *k == key).ok_or_else(|| format!("[{section}] {key}: not a key the compositor reads"))?;
    let coerced = match ty {
        's' => Value::String(value.as_str().ok_or_else(|| format!("{key} is a string"))?.to_string()),
        'i' => json!(value.as_f64().ok_or_else(|| format!("{key} is a number"))?.round() as i64),
        _ => Value::Bool(value.as_bool().ok_or_else(|| format!("{key} is a boolean"))?),
    };
    Ok(json!({section: {key: coerced}}))
}

/// The Windows page: `[windows]` with the compositor's defaults, plus what
/// the compositor does without a key (read-only rows).
pub fn windows_json(table: &toml::Table) -> Value {
    let w = table.get("windows").and_then(|v| v.as_table()).cloned().unwrap_or_default();
    let int = |k: &str, d: i64| w.get(k).and_then(|v| v.as_integer()).unwrap_or(d);
    json!({
        "edge_tiling": w.get("edge_tiling").and_then(|v| v.as_bool()).unwrap_or(true),
        "snap_threshold": int("snap_threshold", 12),
        "edge_tiling_px": int("edge_tiling_px", 4),
        "corner_px": int("corner_px", 120),
        "panel_margin": int("panel_margin", 10),
        "titleBar": "Maximize",
        "altTab": "Most recent first",
    })
}

// ---- keyboard layouts: xkb's evdev.lst ----------------------------------------

const XKB_LAYOUTS: &str = "/usr/share/X11/xkb/rules/evdev.lst";

/// The `! layout` block of `evdev.lst` (`  us              English (US)`)
/// as Select options sorted by name.
pub fn parse_layouts(text: &str) -> Vec<Value> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with('!') {
            inside = line.trim() == "! layout";
            continue;
        }
        if !inside {
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            inside = false;
            continue;
        }
        let Some((code, name)) = line.split_once(char::is_whitespace) else { continue };
        out.push((name.trim().to_string(), code.to_string()));
    }
    out.sort();
    out.into_iter().map(|(label, value)| json!({"label": label, "value": value})).collect()
}

// ---- date & time -------------------------------------------------------------

const ZONE_TAB: &str = "/usr/share/zoneinfo/zone1970.tab";

/// `zone1970.tab` (`codes<TAB>coordinates<TAB>TZ<TAB>comments`) as Select
/// options sorted by zone, `Europe/Vienna` shown as `Europe/Vienna`.
pub fn parse_zones(text: &str) -> Vec<Value> {
    let mut zones: Vec<String> = text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split('\t').nth(2)).map(str::to_string).collect();
    zones.sort();
    zones.dedup();
    zones.into_iter().map(|z| json!({"label": z.replace('_', " "), "value": z})).collect()
}

/// The zone from `/etc/localtime`'s target (`…/zoneinfo/Europe/Vienna`).
pub fn zone_from_link(target: &Path) -> Option<String> {
    let text = target.to_string_lossy();
    text.split_once("zoneinfo/").map(|(_, z)| z.trim_start_matches("posix/").to_string()).filter(|z| !z.is_empty())
}

/// `timedatectl show -p NTP --value` → whether the clock syncs.
pub fn parse_ntp(text: &str) -> bool {
    matches!(text.trim(), "yes" | "true" | "1")
}

/// `ede_settings {page: datetime}`: the `timedatectl` verb and argument.
pub fn timedate_command(key: &str, value: &Value) -> Result<(&'static str, String), String> {
    match key {
        "timeZone" => Ok(("set-timezone", value.as_str().filter(|z| !z.is_empty() && !z.contains("..") && !z.starts_with('/')).ok_or("timeZone is a zone name")?.to_string())),
        "ntp" => Ok(("set-ntp", if value.as_bool().ok_or("ntp is a boolean")? { "true".into() } else { "false".into() })),
        other => Err(format!("datetime: {other} is not a setting")),
    }
}

fn datetime_json() -> Value {
    let zone = std::fs::read_link("/etc/localtime").ok().and_then(|t| zone_from_link(&t)).or_else(|| std::fs::read_to_string("/etc/timezone").ok().map(|t| t.trim().to_string())).unwrap_or_else(|| "UTC".into());
    let ntp = Command::new("timedatectl").args(["show", "-p", "NTP", "--value"]).output().ok().map(|o| parse_ntp(&String::from_utf8_lossy(&o.stdout))).unwrap_or(false);
    json!({
        "timeZone": zone,
        "zones": std::fs::read_to_string(ZONE_TAB).map(|t| parse_zones(&t)).unwrap_or_default(),
        "ntp": ntp,
        // The shell's clock draws HH:MM; there is no 12-hour clock yet.
        "clock": "24-hour",
    })
}

// ---- desktop entries (autostart, default apps) --------------------------------

/// What a page needs from a `.desktop` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DesktopFile {
    pub name: String,
    pub mime: Vec<String>,
    pub hidden: bool,
    pub no_display: bool,
    /// `X-GNOME-Autostart-enabled`, when the file says.
    pub autostart_enabled: Option<bool>,
}

/// The `[Desktop Entry]` group's keys a page reads.
pub fn parse_desktop(text: &str) -> DesktopFile {
    let mut out = DesktopFile::default();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if !inside || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Name" => out.name = value.to_string(),
            "MimeType" => out.mime = value.split(';').filter(|m| !m.is_empty()).map(str::to_string).collect(),
            "Hidden" => out.hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => out.no_display = value.eq_ignore_ascii_case("true"),
            "X-GNOME-Autostart-enabled" => out.autostart_enabled = Some(value.eq_ignore_ascii_case("true")),
            _ => {}
        }
    }
    out
}

fn desktop_files(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "desktop"))
        .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), std::fs::read_to_string(&p).ok()?)))
        .collect();
    out.sort();
    out
}

/// The installed applications: `<dir>/applications/*.desktop` of the data
/// directories in order, the first file of a name wins, hidden ones out.
pub fn desktop_entries(data_dirs: &[PathBuf]) -> Vec<(String, DesktopFile)> {
    let mut seen = BTreeMap::new();
    for dir in data_dirs {
        for (id, text) in desktop_files(&dir.join("applications")) {
            seen.entry(id).or_insert_with(|| parse_desktop(&text));
        }
    }
    seen.into_iter().filter(|(_, d)| !d.hidden && !d.no_display && !d.name.is_empty()).collect()
}

/// The autostart entries: the system directories' files, each overridden
/// by the user's file of the same name; `enabled` = not hidden and not
/// switched off; sorted by name.
pub fn autostart_entries(system_dirs: &[PathBuf], user_dir: &Path) -> Vec<Value> {
    let mut files: BTreeMap<String, (DesktopFile, bool)> = BTreeMap::new();
    for dir in system_dirs {
        for (id, text) in desktop_files(dir) {
            files.entry(id).or_insert_with(|| (parse_desktop(&text), false));
        }
    }
    for (id, text) in desktop_files(user_dir) {
        files.insert(id, (parse_desktop(&text), true));
    }
    let mut out: Vec<(String, String, bool, bool)> = files
        .into_iter()
        .filter(|(_, (d, _))| !d.name.is_empty())
        .map(|(id, (d, user))| (d.name.clone(), id, !d.hidden && d.autostart_enabled != Some(false), user))
        .collect();
    out.sort();
    out.into_iter().map(|(name, id, enabled, user)| json!({"id": id, "name": name, "enabled": enabled, "user": user})).collect()
}

/// The file with `Hidden` and `X-GNOME-Autostart-enabled` set for `on`, the
/// other lines kept (added to `[Desktop Entry]` when missing).
pub fn set_enabled(text: &str, on: bool) -> String {
    let (hidden, enabled) = (format!("Hidden={}", !on), format!("X-GNOME-Autostart-enabled={on}"));
    let mut out: Vec<String> = Vec::new();
    let (mut saw_hidden, mut saw_enabled, mut in_entry, mut inserted) = (false, false, false, false);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_entry && !inserted {
                // Before the blank lines that end the group.
                let mut blanks = Vec::new();
                while out.last().is_some_and(|l| l.trim().is_empty()) {
                    blanks.push(out.pop().unwrap());
                }
                if !saw_hidden {
                    out.push(hidden.clone());
                }
                if !saw_enabled {
                    out.push(enabled.clone());
                }
                out.extend(blanks);
                inserted = true;
            }
            in_entry = trimmed == "[Desktop Entry]";
            out.push(line.to_string());
            continue;
        }
        if in_entry {
            if trimmed.starts_with("Hidden=") {
                saw_hidden = true;
                out.push(hidden.clone());
                continue;
            }
            if trimmed.starts_with("X-GNOME-Autostart-enabled=") {
                saw_enabled = true;
                out.push(enabled.clone());
                continue;
            }
        }
        out.push(line.to_string());
    }
    if !inserted {
        if !in_entry {
            out.push("[Desktop Entry]".into());
        }
        if !saw_hidden {
            out.push(hidden);
        }
        if !saw_enabled {
            out.push(enabled);
        }
    }
    out.join("\n") + "\n"
}

/// Switches an autostart entry: the user's copy (the system file copied
/// into `user_dir` when there is none) gets `Hidden` and
/// `X-GNOME-Autostart-enabled` set.
pub fn autostart_toggle(system_dirs: &[PathBuf], user_dir: &Path, id: &str, on: bool) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || !id.ends_with(".desktop") {
        return Err(format!("{id}: not an autostart entry"));
    }
    let user_file = user_dir.join(id);
    let source = std::fs::read_to_string(&user_file).ok().or_else(|| system_dirs.iter().find_map(|d| std::fs::read_to_string(d.join(id)).ok())).ok_or_else(|| format!("{id}: no such autostart entry"))?;
    std::fs::create_dir_all(user_dir).map_err(|e| e.to_string())?;
    std::fs::write(&user_file, set_enabled(&source, on)).map_err(|e| e.to_string())
}

// ---- default apps: xdg-mime ----------------------------------------------------

/// The roles the Default apps page shows: id, label, the MIME types the
/// role stands for (the first one names the current app).
const ROLES: &[(&str, &str, &[&str])] = &[
    ("browser", "Web browser", &["x-scheme-handler/http", "x-scheme-handler/https", "text/html"]),
    ("mail", "Mail", &["x-scheme-handler/mailto"]),
    ("files", "Files", &["inode/directory"]),
    ("editor", "Text editor", &["text/plain"]),
    ("images", "Images", &["image/png", "image/jpeg"]),
    ("music", "Music", &["audio/mpeg", "audio/flac"]),
    ("video", "Video", &["video/mp4", "video/x-matroska"]),
    ("documents", "Documents", &["application/pdf"]),
];

fn role_mimes(role: &str) -> Option<&'static [&'static str]> {
    ROLES.iter().find(|(id, _, _)| *id == role).map(|(_, _, m)| *m)
}

/// `[Default Applications]` of a `mimeapps.list`: MIME type → the first
/// desktop entry id.
pub fn parse_mimeapps(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Default Applications]";
            continue;
        }
        if !inside || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((mime, apps)) = line.split_once('=') {
            if let Some(first) = apps.split(';').find(|a| !a.is_empty()) {
                out.entry(mime.trim().to_string()).or_insert_with(|| first.trim().to_string());
            }
        }
    }
    out
}

fn mimeapps_defaults() -> BTreeMap<String, String> {
    let mut files = vec![config_home().join("mimeapps.list")];
    files.extend(config_dirs().into_iter().map(|d| d.join("mimeapps.list")));
    files.extend(data_dirs().into_iter().map(|d| d.join("applications/mimeapps.list")));
    let mut out = BTreeMap::new();
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            for (k, v) in parse_mimeapps(&text) {
                out.entry(k).or_insert(v);
            }
        }
    }
    out
}

/// The Default apps page's roles: each with the apps that handle its MIME
/// types as options (by name) and the current pick when it is one of them.
pub fn default_apps(entries: &[(String, DesktopFile)], defaults: &BTreeMap<String, String>) -> Vec<Value> {
    ROLES
        .iter()
        .map(|(id, label, mimes)| {
            let mut options: Vec<(String, String)> = entries.iter().filter(|(_, d)| d.mime.iter().any(|m| mimes.contains(&m.as_str()))).map(|(id, d)| (d.name.clone(), id.clone())).collect();
            options.sort();
            let current = defaults.get(mimes[0]).filter(|app| options.iter().any(|(_, id)| id == *app)).cloned().unwrap_or_default();
            json!({
                "id": id,
                "label": label,
                "value": current,
                "options": options.into_iter().map(|(label, value)| json!({"label": label, "value": value})).collect::<Vec<_>>(),
            })
        })
        .collect()
}

// ---- read-only pages -----------------------------------------------------------

/// The compositor lists at least four workspaces and at most nine, growing
/// as windows are parked, and Super+Ctrl+Left/Right wrap around
/// (`input.rs` `rem_euclid`); no key changes either, so the page reads.
fn workspaces_json() -> Value {
    json!({"listedText": "4", "maxText": "9", "wrapText": "On"})
}

/// libinput's defaults: the compositor sets no pointer option yet.
fn mouse_json() -> Value {
    json!({"speedText": "Default", "naturalText": "Off", "tapText": "Off", "scrollText": "Two fingers"})
}

// ---- the demo -----------------------------------------------------------------------

fn demo_pages() -> Value {
    let layouts = parse_layouts("! layout\n  us              English (US)\n  gb              English (UK)\n  de              German\n  ch              German (Switzerland)\n  fr              French\n  es              Spanish\n  it              Italian\n  nl              Dutch\n  se              Swedish\n  pl              Polish\n  jp              Japanese\n\n! variant\n");
    let zones = parse_zones("#codes\tcoords\tTZ\tcomments\nAT\t+4813+01620\tEurope/Vienna\nDE\t+5230+01322\tEurope/Berlin\nGB\t+513030-0000731\tEurope/London\nUS\t+404251-0740023\tAmerica/New_York\nUS\t+340308-1181434\tAmerica/Los_Angeles\nJP\t+353916+1394441\tAsia/Tokyo\nAU\t-3352+15113\tAustralia/Sydney\n");
    json!({
        "keyboard": {"layouts": layouts},
        "datetime": {"timeZone": "Europe/Vienna", "zones": zones, "ntp": true, "clock": "24-hour"},
        "startup": {"entries": [
            {"id": "at-spi-dbus-bus.desktop", "name": "AT-SPI D-Bus Bus", "enabled": true, "user": false},
            {"id": "nm-applet.desktop", "name": "Network", "enabled": true, "user": false},
            {"id": "xdg-user-dirs.desktop", "name": "User folders update", "enabled": false, "user": true}
        ]},
        "defaults": {"roles": [
            {"id": "browser", "label": "Web browser", "value": "firefox.desktop", "options": [{"label": "Chromium", "value": "chromium.desktop"}, {"label": "Firefox", "value": "firefox.desktop"}]},
            {"id": "mail", "label": "Mail", "value": "thunderbird.desktop", "options": [{"label": "Thunderbird", "value": "thunderbird.desktop"}]},
            {"id": "files", "label": "Files", "value": "nemo.desktop", "options": [{"label": "Files", "value": "nemo.desktop"}]},
            {"id": "editor", "label": "Text editor", "value": "org.gnome.TextEditor.desktop", "options": [{"label": "Text Editor", "value": "org.gnome.TextEditor.desktop"}, {"label": "Vim", "value": "vim.desktop"}]},
            {"id": "images", "label": "Images", "value": "", "options": [{"label": "Image Viewer", "value": "org.gnome.Loupe.desktop"}]},
            {"id": "music", "label": "Music", "value": "", "options": []},
            {"id": "video", "label": "Video", "value": "", "options": [{"label": "Celluloid", "value": "io.github.celluloid_player.Celluloid.desktop"}]},
            {"id": "documents", "label": "Documents", "value": "org.gnome.Evince.desktop", "options": [{"label": "Document Viewer", "value": "org.gnome.Evince.desktop"}]}
        ]},
        "workspaces": workspaces_json(),
        "windows": windows_json(&toml::Table::new()),
        "mouse": mouse_json(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_come_from_the_lst_block_sorted_by_name() {
        let layouts = parse_layouts("! model\n  pc105           Generic 105-key PC\n\n! layout\n  us              English (US)\n  de              German\n  al              Albanian\n\n! variant\n  nodeadkeys      de: German (no dead keys)\n");
        assert_eq!(layouts, vec![json!({"label": "Albanian", "value": "al"}), json!({"label": "English (US)", "value": "us"}), json!({"label": "German", "value": "de"})]);
    }

    #[test]
    fn zones_and_the_link_target() {
        let zones = parse_zones("# comment\nAT\t+4813+01620\tEurope/Vienna\nUS\t+404251-0740023\tAmerica/New_York\tEastern (most areas)\nAT\t+4813+01620\tEurope/Vienna\n");
        assert_eq!(zones, vec![json!({"label": "America/New York", "value": "America/New_York"}), json!({"label": "Europe/Vienna", "value": "Europe/Vienna"})]);
        assert_eq!(zone_from_link(Path::new("/usr/share/zoneinfo/Europe/Berlin")).as_deref(), Some("Europe/Berlin"));
        assert_eq!(zone_from_link(Path::new("../usr/share/zoneinfo/posix/UTC")).as_deref(), Some("UTC"));
        assert_eq!(zone_from_link(Path::new("/etc/localtime")), None);
        assert!(parse_ntp("yes\n") && !parse_ntp("no"));
        assert_eq!(timedate_command("timeZone", &json!("Europe/Vienna")).unwrap(), ("set-timezone", "Europe/Vienna".into()));
        assert_eq!(timedate_command("ntp", &json!(false)).unwrap(), ("set-ntp", "false".into()));
        assert!(timedate_command("timeZone", &json!("../etc")).is_err());
        assert!(timedate_command("clock", &json!("12-hour")).is_err());
    }

    #[test]
    fn config_changes_are_typed_and_coerced() {
        assert_eq!(config_change("windows", "snap_threshold", &json!(12.0)).unwrap(), json!({"windows": {"snap_threshold": 12}}));
        assert_eq!(config_change("windows", "edge_tiling", &json!(false)).unwrap(), json!({"windows": {"edge_tiling": false}}));
        assert_eq!(config_change("keyboard", "layout", &json!("de")).unwrap(), json!({"keyboard": {"layout": "de"}}));
        assert!(config_change("windows", "tap_to_click", &json!(true)).is_err(), "not a compositor key");
        assert!(config_change("keyboard", "repeat_rate", &json!("fast")).is_err());
        let mut t = toml::Table::new();
        t.insert("windows".into(), toml::Value::Table([("snap_threshold".to_string(), toml::Value::Integer(4)), ("edge_tiling".to_string(), toml::Value::Boolean(false))].into_iter().collect()));
        let w = windows_json(&t);
        assert_eq!((w["snap_threshold"].as_i64(), w["edge_tiling"].as_bool(), w["panel_margin"].as_i64()), (Some(4), Some(false), Some(10)));
    }

    #[test]
    fn desktop_files_parse_and_autostart_toggles() {
        let dir = tempfile::tempdir().unwrap();
        let (system, user) = (dir.path().join("etc/xdg/autostart"), dir.path().join("home/autostart"));
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("nm-applet.desktop"), "[Desktop Entry]\nName=Network\nExec=nm-applet\n").unwrap();
        std::fs::write(system.join("hidden.desktop"), "[Desktop Entry]\nName=Hidden\nHidden=true\n").unwrap();
        std::fs::write(system.join("nameless.desktop"), "[Desktop Entry]\nExec=x\n").unwrap();
        let entries = autostart_entries(&[system.clone()], &user);
        assert_eq!(entries, vec![json!({"id": "hidden.desktop", "name": "Hidden", "enabled": false, "user": false}), json!({"id": "nm-applet.desktop", "name": "Network", "enabled": true, "user": false})]);
        autostart_toggle(&[system.clone()], &user, "nm-applet.desktop", false).unwrap();
        let text = std::fs::read_to_string(user.join("nm-applet.desktop")).unwrap();
        assert_eq!(text, "[Desktop Entry]\nName=Network\nExec=nm-applet\nHidden=true\nX-GNOME-Autostart-enabled=false\n");
        assert_eq!(autostart_entries(&[system.clone()], &user)[1], json!({"id": "nm-applet.desktop", "name": "Network", "enabled": false, "user": true}));
        autostart_toggle(&[system.clone()], &user, "nm-applet.desktop", true).unwrap();
        let text = std::fs::read_to_string(user.join("nm-applet.desktop")).unwrap();
        assert_eq!(text, "[Desktop Entry]\nName=Network\nExec=nm-applet\nHidden=false\nX-GNOME-Autostart-enabled=true\n");
        assert!(autostart_toggle(&[system.clone()], &user, "missing.desktop", true).is_err());
        assert!(autostart_toggle(&[system], &user, "../x.desktop", true).is_err());
        assert_eq!(set_enabled("[Desktop Entry]\nName=A\nHidden=true\n\n[Desktop Action x]\nName=B\n", true), "[Desktop Entry]\nName=A\nHidden=false\nX-GNOME-Autostart-enabled=true\n\n[Desktop Action x]\nName=B\n");
        let d = parse_desktop("[Desktop Entry]\nName=Firefox\nMimeType=text/html;x-scheme-handler/http;\nNoDisplay=false\n[Desktop Action new]\nName=New window\n");
        assert_eq!(d, DesktopFile { name: "Firefox".into(), mime: vec!["text/html".into(), "x-scheme-handler/http".into()], ..Default::default() });
    }

    #[test]
    fn default_apps_match_roles_by_mime() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("share/applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(apps.join("firefox.desktop"), "[Desktop Entry]\nName=Firefox\nMimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;\n").unwrap();
        std::fs::write(apps.join("chromium.desktop"), "[Desktop Entry]\nName=Chromium\nMimeType=x-scheme-handler/http;\n").unwrap();
        std::fs::write(apps.join("nemo.desktop"), "[Desktop Entry]\nName=Files\nMimeType=inode/directory;\n").unwrap();
        std::fs::write(apps.join("ghost.desktop"), "[Desktop Entry]\nName=Ghost\nNoDisplay=true\nMimeType=inode/directory;\n").unwrap();
        let entries = desktop_entries(&[dir.path().join("share")]);
        assert_eq!(entries.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["chromium.desktop", "firefox.desktop", "nemo.desktop"]);
        let defaults = parse_mimeapps("[Added Associations]\ntext/html=x.desktop;\n[Default Applications]\nx-scheme-handler/http=chromium.desktop;firefox.desktop\ninode/directory=gone.desktop\n");
        let roles = default_apps(&entries, &defaults);
        assert_eq!(roles[0]["id"], "browser");
        assert_eq!(roles[0]["value"], "chromium.desktop");
        assert_eq!(roles[0]["options"], json!([{"label": "Chromium", "value": "chromium.desktop"}, {"label": "Firefox", "value": "firefox.desktop"}]));
        assert_eq!(roles[2]["value"], "", "a default that is not installed shows no pick");
        assert_eq!(roles[2]["options"], json!([{"label": "Files", "value": "nemo.desktop"}]));
        assert_eq!(role_mimes("mail"), Some(&["x-scheme-handler/mailto"][..]));
    }

    #[test]
    fn the_demo_carries_every_page_and_the_read_only_ones_refuse_writes() {
        let pages = demo_pages();
        for page in ["keyboard", "datetime", "startup", "defaults", "workspaces", "windows", "mouse"] {
            assert!(pages[page].is_object(), "{page}");
        }
        assert_eq!(pages["keyboard"]["layouts"][0]["label"], "Dutch");
        assert!(handle(&json!({"page": "mouse", "key": "tap", "value": true})).is_err());
        assert!(handle(&json!({"page": "workspaces", "key": "count", "value": 6})).is_err());
        assert!(handle(&json!({"page": "windows"})).is_err(), "a key is required");
        assert_eq!(demo_state_for("worktrees"), json!({"page": "worktrees", "deviceOpen": true, "personalOpen": true, "codingOpen": true, "team": {"kind": "", "id": "", "name": ""}, "switcherOpen": false}));
        let team = demo_state_for("local-general,switcher");
        assert_eq!((team["switcherOpen"].as_bool(), team["deviceOpen"].as_bool(), team["personalOpen"].as_bool()), (Some(true), Some(false), Some(false)));
    }
}
