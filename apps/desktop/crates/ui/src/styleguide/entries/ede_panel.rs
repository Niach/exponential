//! #EDE-10 / #EDE-11 / #EDE-12: eDE's factory shell documents (apps/ede/ui)
//! under the gpui painter, the way exponential-shell draws them on exponential-compositor
//! (the shots `ede-panel`, `ede-panel-menu`, `ede-switcher` and `ede-osd` are
//! the compositor's own captures). The `surface` header (where the shell
//! puts a document, which host functions it may call) is not part of the tree.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::{json, Value};

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "ede-panel";
pub(crate) const OWNER: &str = "EDE-10";

const PANEL: &str = include_str!("../../../../../../ede/ui/panel.json");
const MENU: &str = include_str!("../../../../../../ede/ui/menu.json");
const SWITCHER: &str = include_str!("../../../../../../ede/ui/switcher.json");
const OSD: &str = include_str!("../../../../../../ede/ui/osd.json");
const NOTIFICATIONS: &str = include_str!("../../../../../../ede/ui/notifications.json");
const CENTER: &str = include_str!("../../../../../../ede/ui/notification-center.json");
const TRAY_MENU: &str = include_str!("../../../../../../ede/ui/tray-menu.json");
const SETTINGS: &str = include_str!("../../../../../../ede/ui/settings.json");

fn document(source: &str) -> (Value, Value) {
    let doc: Value = serde_json::from_str(source).expect("an eDE factory document");
    (doc["root"].clone(), doc.get("data").cloned().unwrap_or(Value::Null))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let (panel, mut data) = document(PANEL);
    if let Some(data) = data.as_object_mut() {
        data.insert("time".into(), json!("12:00"));
        data.insert("windows".into(), json!([{"id": "w1", "title": "Firefox", "focused": true}, {"id": "w2", "title": "Terminal", "focused": false}]));
        data.insert("tray".into(), json!([{"service": "demo/network", "title": "Network", "status": "Active", "icon": ""}]));
        data.insert("notifications".into(), json!({"count": 2}));
    }
    let (menu, _) = document(MENU);
    let apps = json!({"query": "", "apps": [
        {"id": "org.gnome.Calculator.desktop", "name": "Calculator", "comment": "Perform arithmetic, scientific or financial calculations", "exec": "gnome-calculator", "icon": ""},
        {"id": "nemo.desktop", "name": "Files", "comment": "Access and organize files", "exec": "nemo", "icon": ""},
        {"id": "firefox.desktop", "name": "Firefox", "comment": "Browse the web", "exec": "firefox", "icon": ""},
        {"id": "terminal.desktop", "name": "Terminal", "comment": "Use the command line", "exec": "x-terminal-emulator", "icon": ""}
    ]});
    let (switcher, _) = document(SWITCHER);
    let windows = json!({"switcher": {"windows": [
        {"id": "w1", "title": "Firefox", "selected": false, "thumbnail": ""},
        {"id": "w2", "title": "Terminal", "selected": true, "thumbnail": ""},
        {"id": "w3", "title": "Files", "selected": false, "thumbnail": ""}
    ]}});
    let (osd, _) = document(OSD);
    let level = json!({"osd": {"icon": "media-volume-down", "level": 45, "label": "45%"}});
    let sample_notifications = json!({"notifications": {
        "toasts": [{"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [{"key": "open", "label": "Open"}, {"key": "archive", "label": "Archive"}], "urgency": 1, "at": "11:58"}],
        "list": [
            {"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [], "urgency": 1, "at": "11:58"},
            {"id": 2, "app": "Software", "summary": "Updates installed", "body": "12 packages were updated.", "icon": "", "actions": [], "urgency": 0, "at": "11:40"}
        ],
        "count": 2
    }});
    let (notifications, _) = document(NOTIFICATIONS);
    let (center, _) = document(CENTER);
    let (tray_menu, _) = document(TRAY_MENU);
    let (settings, mut settings_data) = document(SETTINGS);
    if let Some(d) = settings_data.as_object_mut() {
        d.insert("services".into(), json!({
            "network": {"available": true, "wifiEnabled": true, "devices": [{"path": "/dev/1", "interface": "wlp3s0", "state": "connected", "connection": "Home"}], "accessPoints": [{"ssid": "Home", "strength": 82, "active": true}, {"ssid": "Cafe Nero", "strength": 47, "active": false}]},
            "sound": {"available": true, "volume": 40, "muted": false, "micVolume": 100, "sinks": [{"id": 55, "name": "Built-in Audio Analog Stereo", "volume": 40, "default": true}]},
            "bluetooth": {"available": true, "powered": true, "devices": [{"path": "/bt/1", "name": "Keyboard K380", "address": "F4:73:35:12:AB:01", "connected": true}]},
            "power": {"available": true, "batteryText": "73% · discharging", "profile": "balanced"},
            "updates": {"available": true, "countText": "2", "packages": [{"name": "firefox", "from": "129.0", "version": "130.0"}]},
            "printers": {"available": true, "printers": [{"name": "HP_LaserJet", "state": "idle", "default": true}]},
            "about": {"available": true, "hostname": "ubuntu-wayland", "os": "Ubuntu 26.04 LTS", "kernel": "7.0.0-38-generic", "cpu": "13th Gen Intel Core i9-13900K", "memoryText": "7.2 GB", "ede": "0.1.0"}
        }));
        d.insert("outputs".into(), json!([{"name": "HEADLESS-1", "summary": "Exponential Desktop (headless) · 1280×720 at 1× (0, 0)", "current": "1280x720@60", "modeOptions": [{"label": "1280x720@60 (preferred)", "value": "1280x720@60"}], "scale": 1, "scaleText": "1"}]));
        d.insert("user".into(), json!({"name": "Danny", "login": "danny"}));
    }
    let sample_tray_menu = json!({"traymenu": {"service": "demo/network", "title": "Network", "items": [
        {"id": 1, "label": "Connect to Wi-Fi", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0},
        {"id": 2, "label": "Airplane mode", "enabled": true, "separator": false, "checked": false, "checkable": true, "depth": 0},
        {"id": 3, "label": "", "enabled": true, "separator": true, "checked": false, "checkable": false, "depth": 0},
        {"id": 4, "label": "Settings…", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0}
    ]}});
    v_flex()
        .gap_3()
        .child(div().w_full().child(styleguide_surface_in("sg-ede-panel", panel, None, Some(data), window, cx)))
        .child(div().w(px(360.0)).child(styleguide_surface_in("sg-ede-menu", menu, None, Some(apps), window, cx)))
        .child(div().w(px(760.0)).child(styleguide_surface_in("sg-ede-switcher", switcher, None, Some(windows), window, cx)))
        .child(div().w(px(280.0)).child(styleguide_surface_in("sg-ede-osd", osd, None, Some(level), window, cx)))
        .child(div().w(px(380.0)).child(styleguide_surface_in("sg-ede-notifications", notifications, None, Some(sample_notifications.clone()), window, cx)))
        .child(div().w(px(380.0)).child(styleguide_surface_in("sg-ede-notification-center", center, None, Some(sample_notifications), window, cx)))
        .child(div().w(px(280.0)).child(styleguide_surface_in("sg-ede-tray-menu", tray_menu, None, Some(sample_tray_menu), window, cx)))
        .child(div().w(px(960.0)).child(styleguide_surface_in("sg-ede-settings", settings, None, Some(settings_data), window, cx)))
}
