//! The shell's D-Bus faces (#EDE-12), one thread with a blocking zbus
//! connection: the notification server (`org.freedesktop.Notifications`,
//! what every app's `notify-send`, Firefox or Thunderbird talks to) and the
//! tray (`org.kde.StatusNotifierWatcher` + a StatusNotifierHost, so items
//! like a network applet or Steam register here; their menus come over
//! `com.canonical.dbusmenu`). Both follow the freedesktop and KDE
//! specifications; nothing is shared with Cinnamon's implementations.
//!
//! Events reach the main loop over a channel ([`Event`]); the main loop
//! acts back through [`Dbus`] (closing a notification, activating an item,
//! clicking a menu row), each a short blocking call.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use base64::Engine;
use serde_json::{json, Value};
use zbus::{
    blocking::{Connection, Proxy},
    names::BusName,
    zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue},
};

pub const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
pub const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
pub const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
/// A toast without an expiry from the app stays this long; critical ones stay.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(6);

/// What the main loop hears.
#[derive(Debug)]
pub enum Event {
    /// A new notification, or `replaces_id` again with new content.
    Notified(Notification),
    /// An app asked to close one (`CloseNotification`).
    CloseRequested(u32),
    /// Items registered, left or changed: read [`Dbus::tray_items`] again.
    TrayChanged,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    /// A `file:` or `data:` URL, or empty.
    pub icon: String,
    /// `(key, label)` pairs.
    pub actions: Vec<(String, String)>,
    /// 0 low, 1 normal, 2 critical.
    pub urgency: u8,
    /// `None` = stays until closed.
    pub expires_at: Option<Instant>,
    /// Local `HH:MM`.
    pub at: String,
}

impl Notification {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "app": self.app,
            "summary": self.summary,
            "body": self.body,
            "icon": self.icon,
            "actions": self.actions.iter().map(|(key, label)| json!({"key": key, "label": label})).collect::<Vec<_>>(),
            "urgency": self.urgency,
            "at": self.at,
        })
    }
}

/// The notification server object.
struct Notifications {
    tx: mpsc::Sender<Event>,
    next_id: Arc<AtomicU32>,
    clock: fn() -> String,
}

/// Plain text out of the body's allowed markup (`<b>`, `<i>`, `<u>`, `<a>`, `<img>`).
pub fn strip_markup(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// `app_icon` (a name, an absolute path or a `file://` URL) and the
/// `image-path` hint as a picture URL for `Image`.
pub fn icon_url(app_icon: &str, image_path: Option<&str>) -> String {
    let name = image_path.filter(|p| !p.is_empty()).unwrap_or(app_icon);
    if name.is_empty() {
        return String::new();
    }
    if name.starts_with("file://") {
        return name.to_string();
    }
    crate::apps::icon_file(name).map(|p| format!("file://{}", p.display())).unwrap_or_default()
}

fn hint_str(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    hints.get(key).and_then(|v| String::try_from(v.clone()).ok())
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<String> {
        ["body", "body-markup", "actions", "persistence", "icon-static"].iter().map(|s| s.to_string()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(&self, app_name: String, replaces_id: u32, app_icon: String, summary: String, body: String, actions: Vec<String>, hints: HashMap<String, OwnedValue>, expire_timeout: i32) -> u32 {
        let id = if replaces_id != 0 { replaces_id } else { self.next_id.fetch_add(1, Ordering::SeqCst) };
        let urgency = hints.get("urgency").and_then(|v| u8::try_from(v.clone()).ok()).unwrap_or(1);
        let expires_at = match (expire_timeout, urgency) {
            (_, 2) | (0, _) => None,
            (ms, _) if ms > 0 => Some(Instant::now() + Duration::from_millis(ms as u64)),
            _ => Some(Instant::now() + DEFAULT_TIMEOUT),
        };
        let image_path = hint_str(&hints, "image-path").or_else(|| hint_str(&hints, "image_path"));
        let app = if app_name.is_empty() { hint_str(&hints, "desktop-entry").unwrap_or_default() } else { app_name };
        let n = Notification {
            id,
            app,
            summary,
            body: strip_markup(&body),
            icon: icon_url(&app_icon, image_path.as_deref()),
            actions: actions.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect(),
            urgency,
            expires_at,
            at: (self.clock)(),
        };
        let _ = self.tx.send(Event::Notified(n));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send(Event::CloseRequested(id));
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("Exponential Desktop".into(), "Exponential".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }
}

/// The watcher object: who is in the tray.
struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
    tx: mpsc::Sender<Event>,
}

/// `service` as the spec allows it: a bus name, or an object path from
/// `sender` (then `sender/path`); both forms resolve to `(bus name, path)`.
pub fn item_address(service: &str, sender: &str) -> (String, String) {
    if let Some(path) = service.strip_prefix('/') {
        return (sender.to_string(), format!("/{path}"));
    }
    match service.split_once('/') {
        Some((name, path)) => (name.to_string(), format!("/{path}")),
        None => (service.to_string(), "/StatusNotifierItem".to_string()),
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&mut self, service: String, #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection) {
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let (name, path) = item_address(&service, &sender);
        let key = format!("{name}{path}");
        {
            let mut items = self.items.lock().unwrap_or_else(|p| p.into_inner());
            if items.contains(&key) {
                return;
            }
            items.push(key.clone());
        }
        tracing::info!(item = %key, "tray item registered");
        let blocking = Connection::from(connection.clone());
        let _ = blocking.emit_signal(None::<BusName<'_>>, WATCHER_PATH, WATCHER_NAME, "StatusNotifierItemRegistered", &(key.as_str(),));
        let _ = self.tx.send(Event::TrayChanged);
    }

    fn register_status_notifier_host(&self, _service: String, #[zbus(connection)] connection: &zbus::Connection) {
        let blocking = Connection::from(connection.clone());
        let _ = blocking.emit_signal(None::<BusName<'_>>, WATCHER_PATH, WATCHER_NAME, "StatusNotifierHostRegistered", &());
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        // The shell itself draws the items.
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

/// One item as the panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TrayItem {
    /// `busname/path`, the handle the documents pass back.
    pub service: String,
    pub title: String,
    pub status: String,
    /// A `file:` or `data:` URL, or empty.
    pub icon: String,
    pub has_menu: bool,
}

impl TrayItem {
    pub fn to_json(&self) -> Value {
        json!({"service": self.service, "title": self.title, "status": self.status, "icon": self.icon})
    }
}

/// A dbusmenu row, flattened one level (a submenu's rows follow it, indented).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub checked: Option<bool>,
    pub depth: u32,
}

impl MenuEntry {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "label": self.label, "enabled": self.enabled, "separator": self.separator, "checked": self.checked.unwrap_or(false), "checkable": self.checked.is_some(), "depth": self.depth})
    }
}

/// The main loop's handle.
pub struct Dbus {
    conn: Connection,
    items: Arc<Mutex<Vec<String>>>,
    tx: mpsc::Sender<Event>,
}

impl Dbus {
    /// Serves both objects on the session bus and takes the well-known
    /// names when nobody else holds them (another desktop's server keeps
    /// them while eDE runs nested; the objects still answer on the unique
    /// name). `None` = no session bus.
    pub fn start(tx: mpsc::Sender<Event>, clock: fn() -> String) -> Option<Dbus> {
        let items = Arc::new(Mutex::new(Vec::new()));
        let notifications = Notifications { tx: tx.clone(), next_id: Arc::new(AtomicU32::new(1)), clock };
        let watcher = Watcher { items: items.clone(), tx: tx.clone() };
        let conn = match zbus::blocking::connection::Builder::session().and_then(|b| b.serve_at(NOTIFICATIONS_PATH, notifications)).and_then(|b| b.serve_at(WATCHER_PATH, watcher)).and_then(|b| b.build()) {
            Ok(c) => c,
            Err(e) => {
                tracing::info!("no session bus: {e} (no notifications, no tray)");
                return None;
            }
        };
        for name in [NOTIFICATIONS_NAME, WATCHER_NAME] {
            match conn.request_name(name) {
                Ok(()) => tracing::info!(name, "serving"),
                Err(e) => tracing::warn!(name, "not ours: {e} (another desktop's server holds it; the object answers on {})", conn.unique_name().map(|n| n.to_string()).unwrap_or_default()),
            }
        }
        let host = format!("org.kde.StatusNotifierHost-{}", std::process::id());
        if conn.request_name(host.as_str()).is_ok() {
            let _ = conn.call_method(Some(WATCHER_NAME), WATCHER_PATH, Some(WATCHER_NAME), "RegisterStatusNotifierHost", &(host.as_str(),));
        }
        let dbus = Dbus { conn, items, tx };
        tracing::info!(unique = %dbus.unique_name(), "session bus");
        dbus.watch_bus();
        Some(dbus)
    }

    pub fn unique_name(&self) -> String {
        self.conn.unique_name().map(|n| n.to_string()).unwrap_or_default()
    }

    /// Items whose bus name left are forgotten; an item's `New*` signal
    /// re-reads the tray.
    fn watch_bus(&self) {
        let items = self.items.clone();
        let tx = self.tx.clone();
        let conn = self.conn.inner().clone();
        std::thread::Builder::new()
            .name("ede-shell-dbus-watch".into())
            .spawn(move || {
                zbus::block_on(async move {
                    let rules = [
                        zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface("org.freedesktop.DBus").and_then(|b| b.member("NameOwnerChanged")).map(|b| b.build()),
                        zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface(ITEM_INTERFACE).map(|b| b.build()),
                    ];
                    let mut streams = Vec::new();
                    for rule in rules.into_iter().flatten() {
                        if let Ok(s) = zbus::MessageStream::for_match_rule(rule, &conn, None).await {
                            streams.push(s);
                        }
                    }
                    use futures_util::StreamExt;
                    let mut all = futures_util::stream::select_all(streams);
                    while let Some(Ok(msg)) = all.next().await {
                        let header = msg.header();
                        if header.member().is_some_and(|m| m == "NameOwnerChanged") {
                            let Ok((name, _old, new)) = msg.body().deserialize::<(String, String, String)>() else { continue };
                            if new.is_empty() {
                                let mut items = items.lock().unwrap_or_else(|p| p.into_inner());
                                let before = items.len();
                                items.retain(|i| !i.starts_with(&format!("{name}/")));
                                if items.len() != before {
                                    let _ = Connection::from(conn.clone()).emit_signal(None::<BusName<'_>>, WATCHER_PATH, WATCHER_NAME, "StatusNotifierItemUnregistered", &(name.as_str(),));
                                    let _ = tx.send(Event::TrayChanged);
                                }
                            }
                        } else if header.interface().is_some_and(|i| i == ITEM_INTERFACE) {
                            let _ = tx.send(Event::TrayChanged);
                        }
                    }
                });
            })
            .ok();
    }

    /// `NotificationClosed(id, reason)`: 1 expired, 2 dismissed, 3 closed by the app.
    pub fn notification_closed(&self, id: u32, reason: u32) {
        let _ = self.conn.emit_signal(None::<BusName<'_>>, NOTIFICATIONS_PATH, NOTIFICATIONS_NAME, "NotificationClosed", &(id, reason));
    }

    pub fn action_invoked(&self, id: u32, key: &str) {
        let _ = self.conn.emit_signal(None::<BusName<'_>>, NOTIFICATIONS_PATH, NOTIFICATIONS_NAME, "ActionInvoked", &(id, key));
    }

    fn item_proxy(&self, service: &str) -> Option<Proxy<'_>> {
        let (name, path) = service.split_once('/').map(|(n, p)| (n.to_string(), format!("/{p}")))?;
        Proxy::new(&self.conn, name, path, ITEM_INTERFACE).ok()
    }

    /// Every registered item, read fresh (a dead one is dropped).
    pub fn tray_items(&self) -> Vec<TrayItem> {
        let services = self.items.lock().unwrap_or_else(|p| p.into_inner()).clone();
        services
            .iter()
            .filter_map(|service| {
                let proxy = self.item_proxy(service)?;
                let title: String = proxy.get_property("Title").unwrap_or_default();
                let status: String = proxy.get_property("Status").unwrap_or_else(|_| "Active".into());
                let icon_name: String = proxy.get_property("IconName").unwrap_or_default();
                let theme_path: String = proxy.get_property("IconThemePath").unwrap_or_default();
                let mut icon = String::new();
                if !icon_name.is_empty() {
                    icon = icon_in_path(&theme_path, &icon_name).or_else(|| crate::apps::icon_file(&icon_name)).map(|p| format!("file://{}", p.display())).unwrap_or_default();
                }
                if icon.is_empty() {
                    if let Ok(pixmaps) = proxy.get_property::<Vec<(i32, i32, Vec<u8>)>>("IconPixmap") {
                        icon = pixmap_data_url(&pixmaps).unwrap_or_default();
                    }
                }
                let menu: Option<OwnedObjectPath> = proxy.get_property("Menu").ok();
                let item_is_menu: bool = proxy.get_property("ItemIsMenu").unwrap_or(false);
                let has_menu = menu.as_ref().is_some_and(|m| m.as_str() != "/") && (item_is_menu || menu.is_some());
                let id: String = proxy.get_property("Id").unwrap_or_default();
                if title.is_empty() && icon.is_empty() && id.is_empty() {
                    return None;
                }
                Some(TrayItem { service: service.clone(), title: if title.is_empty() { id } else { title }, status, icon, has_menu })
            })
            .collect()
    }

    /// A left click: `Activate`.
    pub fn tray_activate(&self, service: &str, x: i32, y: i32) {
        if let Some(proxy) = self.item_proxy(service) {
            if let Err(e) = proxy.call_method("Activate", &(x, y)) {
                tracing::info!(service, "Activate: {e}");
            }
        }
    }

    fn menu_proxy(&self, service: &str) -> Option<Proxy<'_>> {
        let proxy = self.item_proxy(service)?;
        let menu: OwnedObjectPath = proxy.get_property("Menu").ok()?;
        if menu.as_str() == "/" {
            return None;
        }
        let name = service.split_once('/')?.0.to_string();
        Proxy::new(&self.conn, name, menu.as_str().to_string(), MENU_INTERFACE).ok()
    }

    /// The item's dbusmenu, two levels deep.
    pub fn tray_menu(&self, service: &str) -> Option<Vec<MenuEntry>> {
        let proxy = self.menu_proxy(service)?;
        let _ = proxy.call_method("AboutToShow", &(0i32,));
        let props = ["label", "enabled", "visible", "type", "toggle-type", "toggle-state", "children-display"];
        let reply = proxy.call_method("GetLayout", &(0i32, 2i32, props.to_vec())).ok()?;
        let (_, root): (u32, Layout) = reply.body().deserialize().ok()?;
        let mut out = Vec::new();
        flatten_layout(&root, 0, &mut out);
        Some(out)
    }

    /// A row clicked.
    pub fn tray_menu_event(&self, service: &str, id: i32) {
        if let Some(proxy) = self.menu_proxy(service) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
            if let Err(e) = proxy.call_method("Event", &(id, "clicked", ZValue::from(0i32), now)) {
                tracing::info!(service, id, "menu Event: {e}");
            }
        }
    }
}

/// dbusmenu's layout node: `(id, properties, children)`.
type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

fn flatten_layout(node: &Layout, depth: u32, out: &mut Vec<MenuEntry>) {
    for child in &node.2 {
        let Ok(child) = Layout::try_from(child.clone()) else { continue };
        let (id, props, _) = &child;
        let get_str = |k: &str| props.get(k).and_then(|v| String::try_from(v.clone()).ok());
        let get_bool = |k: &str, default: bool| props.get(k).and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(default);
        if !get_bool("visible", true) {
            continue;
        }
        let separator = get_str("type").as_deref() == Some("separator");
        let checked = match get_str("toggle-type").as_deref() {
            Some("checkmark") | Some("radio") => Some(props.get("toggle-state").and_then(|v| i32::try_from(v.clone()).ok()) == Some(1)),
            _ => None,
        };
        out.push(MenuEntry { id: *id, label: get_str("label").unwrap_or_default().replace("__", "\u{0}").replace('_', "").replace('\u{0}', "_"), enabled: get_bool("enabled", true), separator, checked, depth });
        if get_str("children-display").as_deref() == Some("submenu") && depth < 1 {
            flatten_layout(&child, depth + 1, out);
        }
    }
}

/// An icon named by an item with its own theme path (`IconThemePath`).
fn icon_in_path(theme_path: &str, name: &str) -> Option<std::path::PathBuf> {
    if theme_path.is_empty() {
        return None;
    }
    let root = std::path::Path::new(theme_path);
    let mut stack = vec![root.to_path_buf()];
    let mut best: Option<std::path::PathBuf> = None;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_stem().is_some_and(|s| s == name) && p.extension().is_some_and(|x| x == "svg" || x == "png") {
                if p.extension().is_some_and(|x| x == "svg") {
                    return Some(p);
                }
                best.get_or_insert(p);
            }
        }
    }
    best
}

/// The largest pixmap up to 64 px as a PNG data URL (SNI pixmaps are ARGB32
/// in network byte order).
pub fn pixmap_data_url(pixmaps: &[(i32, i32, Vec<u8>)]) -> Option<String> {
    let (w, h, argb) = pixmaps.iter().filter(|(w, h, b)| *w > 0 && *h > 0 && b.len() == (*w * *h * 4) as usize).filter(|(w, _, _)| *w <= 64).max_by_key(|(w, _, _)| *w).or_else(|| pixmaps.iter().find(|(w, h, b)| *w > 0 && *h > 0 && b.len() == (*w * *h * 4) as usize))?;
    let mut rgba = Vec::with_capacity(argb.len());
    for px in argb.chunks_exact(4) {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, *w as u32, *h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_addresses_follow_the_spec() {
        assert_eq!(item_address(":1.42", ":1.9"), (":1.42".into(), "/StatusNotifierItem".into()));
        assert_eq!(item_address("/org/ayatana/NotificationItem/nm", ":1.9"), (":1.9".into(), "/org/ayatana/NotificationItem/nm".into()));
        assert_eq!(item_address("org.kde.StatusNotifierItem-7-1/StatusNotifierItem", ":1.9"), ("org.kde.StatusNotifierItem-7-1".into(), "/StatusNotifierItem".into()));
    }

    #[test]
    fn bodies_lose_their_markup_and_pixmaps_become_pictures() {
        assert_eq!(strip_markup("<b>Build</b> done &amp; <a href=\"x\">shipped</a>"), "Build done & shipped");
        let px = vec![(2, 2, vec![0xff, 0x10, 0x20, 0x30].repeat(4))];
        assert!(pixmap_data_url(&px).unwrap().starts_with("data:image/png;base64,"));
        assert!(pixmap_data_url(&[(2, 2, vec![0u8; 3])]).is_none());
    }

    #[test]
    fn a_layout_flattens_with_submenus_one_level_deep() {
        let leaf = |id: i32, label: &str| -> OwnedValue {
            let mut props = HashMap::new();
            props.insert("label".to_string(), OwnedValue::try_from(ZValue::from(label)).unwrap());
            OwnedValue::try_from(ZValue::from((id, props, Vec::<OwnedValue>::new()))).unwrap()
        };
        let mut sub_props = HashMap::new();
        sub_props.insert("label".to_string(), OwnedValue::try_from(ZValue::from("_More")).unwrap());
        sub_props.insert("children-display".to_string(), OwnedValue::try_from(ZValue::from("submenu")).unwrap());
        let sub = OwnedValue::try_from(ZValue::from((2i32, sub_props, vec![leaf(3, "Inner")]))).unwrap();
        let root: Layout = (0, HashMap::new(), vec![leaf(1, "Quit"), sub]);
        let mut out = Vec::new();
        flatten_layout(&root, 0, &mut out);
        let labels: Vec<(i32, &str, u32)> = out.iter().map(|e| (e.id, e.label.as_str(), e.depth)).collect();
        assert_eq!(labels, [(1, "Quit", 0), (2, "More", 0), (3, "Inner", 1)]);
    }
}
