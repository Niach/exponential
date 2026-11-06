//! #EDE-10 / #EDE-11 / #EDE-12 / #EDE-42: eDE's factory shell documents
//! (apps/ede/ui) under the gpui painter, the way exponential-shell draws them on
//! exponential-compositor: the IDE look (the ground, the sidebar and its rail,
//! Mint's taskbar, the start menu, the chat) and the overlays (the shots
//! `ede-*` are the compositor's own captures). The `surface` header (where
//! the shell puts a document, which host functions it may call) is not part
//! of the tree.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::{json, Value};

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "ede-panel";
pub(crate) const OWNER: &str = "EDE-42";

const GROUND: &str = include_str!("../../../../../../ede/ui/ground.json");
const SIDEBAR: &str = include_str!("../../../../../../ede/ui/sidebar.json");
const TASKBAR: &str = include_str!("../../../../../../ede/ui/taskbar.json");
const START_MENU: &str = include_str!("../../../../../../ede/ui/start-menu.json");
const CHAT: &str = include_str!("../../../../../../ede/ui/chat.json");
const EXPO: &str = include_str!("../../../../../../ede/ui/expo.json");
const SCALE: &str = include_str!("../../../../../../ede/ui/scale.json");
const HISTORY: &str = include_str!("../../../../../../ede/ui/history.json");
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

/// The shell model as the documents read it (`/shell`), the sample the goldens use.
fn sample_shell(rail: bool) -> Value {
    let apps = json!([
        {"id": "org.gnome.Calculator", "name": "Calculator", "comment": "Perform arithmetic, scientific or financial calculations", "icon": "", "category": "accessories"},
        {"id": "at.exponential", "name": "Exponential", "comment": "Your team, agents and boards", "icon": "", "category": "exponential"},
        {"id": "nemo", "name": "Files", "comment": "Access and organize files", "icon": "", "category": "accessories"},
        {"id": "firefox", "name": "Firefox", "comment": "Browse the web", "icon": "", "category": "internet"},
        {"id": "terminal", "name": "Terminal", "comment": "Use the command line", "icon": "", "category": "administration"}
    ]);
    json!({
        "rail": rail,
        "switcher": {"current": {"kind": "local", "id": "local", "name": "Local"}, "entries": [{"kind": "local", "id": "local", "name": "Local", "current": true}]},
        "sidebar": {"pinnedOpen": true, "workspacesOpen": true, "sessionsOpen": true, "pinned": [{"app": "firefox", "name": "Firefox", "icon": "", "focused": false}, {"app": "nemo", "name": "Files", "icon": "", "focused": false}], "workspaces": [{"index": 0, "label": "Workspace 1", "active": true, "count": 3, "countText": "3"}, {"index": 1, "label": "Workspace 2", "active": false, "count": 1, "countText": "1"}]},
        "sessions": [],
        "taskbar": {"pinned": ["firefox", "nemo"], "hover": null, "previews": null, "menu": null, "buttons": [
            {"app": "firefox", "name": "Firefox", "icon": "", "pinned": true, "windows": ["w1", "w4"], "focused": false, "minimized": false, "running": true},
            {"app": "nemo", "name": "Files", "icon": "", "pinned": true, "windows": ["w3"], "focused": false, "minimized": false, "running": true},
            {"app": "org.gnome.Calculator", "name": "Calculator", "icon": "", "pinned": true, "windows": [], "focused": false, "minimized": false, "running": false},
            {"app": "terminal", "name": "Terminal", "icon": "", "pinned": false, "windows": ["w2"], "focused": true, "minimized": false, "running": true}
        ]},
        "startMenu": {"open": true, "query": "", "category": "all", "categories": [
            {"id": "all", "label": "All applications", "count": 5, "icon": "nav-apps", "selected": true}, {"id": "favourites", "label": "Favourites", "count": 2, "icon": "ui-pin", "selected": false}, {"id": "exponential", "label": "Exponential", "count": 1, "icon": "brand-exponential", "selected": false}, {"id": "accessories", "label": "Accessories", "count": 3, "icon": "ui-properties", "selected": false}, {"id": "internet", "label": "Internet", "count": 1, "icon": "ui-link", "selected": false}, {"id": "administration", "label": "Administration", "count": 1, "icon": "nav-settings", "selected": false}
        ], "apps": apps, "favourites": [{"id": "firefox", "name": "Firefox", "icon": ""}, {"id": "nemo", "name": "Files", "icon": ""}]},
        "chat": {"open": false},
        "altTab": {"open": false, "selected": 0, "order": []}
    })
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let (ground, _) = document(GROUND);
    let (sidebar, _) = document(SIDEBAR);
    let (taskbar, mut taskbar_data) = document(TASKBAR);
    if let Some(data) = taskbar_data.as_object_mut() {
        data.insert("time".into(), json!("12:00"));
        data.insert("shell".into(), sample_shell(false));
        data.insert("tray".into(), json!([{"service": "demo/network", "title": "Network", "status": "Active", "icon": ""}]));
        data.insert("notifications".into(), json!({"count": 2}));
        data.insert("applets".into(), json!({"network": {"icon": "ede-wifi"}, "sound": {"icon": "media-volume-up"}, "bluetooth": {"show": true}, "battery": {"show": true, "icon": "ede-battery"}}));
    }
    let (start_menu, _) = document(START_MENU);
    let (chat, _) = document(CHAT);
    let (expo, _) = document(EXPO);
    let (scale, _) = document(SCALE);
    let (history, _) = document(HISTORY);
    let expo_data = json!({"expo": {"workspaces": [
        {"index": 0, "label": "Workspace 1", "active": true, "count": 3, "countText": "3", "windows": [{"id": "w1", "title": "Firefox", "focused": false, "thumbnail": ""}, {"id": "w2", "title": "Terminal", "focused": true, "thumbnail": ""}, {"id": "w3", "title": "Files", "focused": false, "thumbnail": ""}]},
        {"index": 1, "label": "Workspace 2", "active": false, "count": 1, "countText": "1", "windows": [{"id": "w4", "title": "Exponential — Firefox", "focused": false, "thumbnail": ""}]}
    ]}});
    let scale_data = json!({"scale": {"label": "Workspace 1", "windows": expo_data["expo"]["workspaces"][0]["windows"]}});
    let history_data = json!({"history": {"entries": [
        {"id": "a1b2c3d4e5", "short": "a1b2c3d", "summary": "Edit taskbar.json", "at": "2026-11-06 11:58"},
        {"id": "f6a7b8c9d0", "short": "f6a7b8c", "summary": "Edit sidebar.json", "at": "2026-11-06 11:40"},
        {"id": "0f1e2d3c4b", "short": "0f1e2d3", "summary": "Factory sidebar.json", "at": "2026-11-06 09:00"}
    ]}});
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
        .child(div().w(px(640.0)).h(px(360.0)).child(styleguide_surface_in("sg-ede-ground", ground, None, Some(json!({"shell": sample_shell(false)})), window, cx)))
        .child(
            gpui_component::h_flex()
                .gap_3()
                .child(div().w(px(272.0)).h(px(560.0)).child(styleguide_surface_in("sg-ede-sidebar", sidebar.clone(), None, Some(json!({"shell": sample_shell(false)})), window, cx)))
                .child(div().w(px(56.0)).h(px(560.0)).child(styleguide_surface_in("sg-ede-rail", sidebar, None, Some(json!({"shell": sample_shell(true)})), window, cx))),
        )
        .child(div().w(px(1008.0)).child(styleguide_surface_in("sg-ede-taskbar", taskbar, None, Some(taskbar_data), window, cx)))
        .child(div().w(px(760.0)).child(styleguide_surface_in("sg-ede-start-menu", start_menu, None, Some(json!({"user": {"name": "Danny", "host": "ubuntu-wayland"}, "shell": sample_shell(false)})), window, cx)))
        .child(div().w(px(720.0)).child(styleguide_surface_in("sg-ede-chat", chat, None, Some(json!({"chat": {"draft": ""}})), window, cx)))
        .child(div().w(px(1040.0)).child(styleguide_surface_in("sg-ede-expo", expo, None, Some(expo_data), window, cx)))
        .child(div().w(px(1040.0)).child(styleguide_surface_in("sg-ede-scale", scale, None, Some(scale_data), window, cx)))
        .child(div().w(px(720.0)).child(styleguide_surface_in("sg-ede-history", history, None, Some(history_data), window, cx)))
        .child(div().w(px(760.0)).child(styleguide_surface_in("sg-ede-switcher", switcher, None, Some(windows), window, cx)))
        .child(div().w(px(280.0)).child(styleguide_surface_in("sg-ede-osd", osd, None, Some(level), window, cx)))
        .child(div().w(px(380.0)).child(styleguide_surface_in("sg-ede-notifications", notifications, None, Some(sample_notifications.clone()), window, cx)))
        .child(div().w(px(380.0)).child(styleguide_surface_in("sg-ede-notification-center", center, None, Some(sample_notifications), window, cx)))
        .child(div().w(px(280.0)).child(styleguide_surface_in("sg-ede-tray-menu", tray_menu, None, Some(sample_tray_menu), window, cx)))
        .child(div().w(px(960.0)).child(styleguide_surface_in("sg-ede-settings", settings, None, Some(settings_data), window, cx)))
}
