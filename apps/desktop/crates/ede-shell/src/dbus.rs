//! The shell's D-Bus faces (#EDE-12), one thread with a blocking zbus
//! connection: the notification server (`org.freedesktop.Notifications`,
//! what every app's `notify-send`, Firefox or Thunderbird talks to) and the
//! tray (`org.kde.StatusNotifierWatcher` + a StatusNotifierHost, so items
//! like a network applet or Steam register here; their menus come over
//! `com.canonical.dbusmenu`). Both follow the freedesktop and KDE
//! specifications; nothing is shared with Cinnamon's implementations.
//!
//! Events reach the main loop over a channel ([`Event`]); the main loop
//! acts back through [`Dbus`]. Notification signals go out at once; every
//! tray call (reading the items, activating one, its menu, a menu row) runs
//! on the tray thread, each bounded by [`TRAY_CALL_TIMEOUT`], its answer
//! back as an [`Event`] (#EDE-135: a hung tray app froze the whole shell).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

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
/// How long a call to another app on the bus may take (a hung tray item).
pub const TRAY_CALL_TIMEOUT: Duration = Duration::from_millis(500);

/// What the main loop hears.
#[derive(Debug)]
pub enum Event {
    /// A new notification, or `replaces_id` again with new content.
    Notified(Notification),
    /// An app asked to close one (`CloseNotification`).
    CloseRequested(u32),
    /// Items registered, left or changed: ask [`Dbus::tray_refresh`].
    TrayChanged,
    /// The tray as read on the tray thread.
    TrayItems(Vec<TrayItem>),
}

/// What the tray thread does, in order; refreshes asked together read once.
#[derive(Debug, PartialEq)]
enum TrayRequest {
    Refresh,
    Activate(String),
    MenuEvent(String, i32),
    /// The middle button: `SecondaryActivate`.
    SecondaryActivate(String),
    /// The right button on an item without a dbusmenu: `ContextMenu`.
    ContextMenu(String),
    /// One wheel step: `Scroll(delta, orientation)`.
    Scroll(String, i32, &'static str),
}

/// #EDE-247: what a tray item does on input, Cinnamon's way (its
/// xapp-sn-watcher, `sn-item.c`), the same for third-party items and ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayInput {
    /// The middle button (on press): `SecondaryActivate(x, y)`.
    Middle,
    /// The right button (on release) on an item WITHOUT a dbusmenu:
    /// `ContextMenu(x, y)` (one with a menu opens it in the taskbar).
    Right,
    /// A wheel step: `Scroll(±1, "vertical" | "horizontal")`; up and left
    /// are -1, as Cinnamon's xapp-status applet sends them.
    Scroll { delta: i32, horizontal: bool },
}

impl TrayInput {
    /// The taskbar's `auxPress {button}` / `scroll {direction}` payload.
    pub fn from_event(button: Option<&str>, direction: Option<&str>) -> Option<TrayInput> {
        match (button, direction) {
            (Some("middle"), _) => Some(TrayInput::Middle),
            (Some("right"), _) => Some(TrayInput::Right),
            (_, Some("up")) => Some(TrayInput::Scroll { delta: -1, horizontal: false }),
            (_, Some("down")) => Some(TrayInput::Scroll { delta: 1, horizontal: false }),
            (_, Some("left")) => Some(TrayInput::Scroll { delta: -1, horizontal: true }),
            (_, Some("right")) => Some(TrayInput::Scroll { delta: 1, horizontal: true }),
            _ => None,
        }
    }
}

/// The object path libappindicator (Ayatana) registers its items under:
/// Cinnamon's `APPINDICATOR_PATH_PREFIX`.
const APPINDICATOR_PATH_PREFIX: &str = "/org/ayatana/NotificationItem/";

/// `org.x.apps.statusicon` `left-click-activate-apps`' default: the
/// AppIndicator items whose left click is `SecondaryActivate`, not the menu.
const LEFT_CLICK_ACTIVATE_APPS: &[&str] = &["onboard"];

/// An item registered at an AppIndicator path (Steam, Electron apps,
/// nm-applet's indicator): libappindicator has no `Activate`.
fn is_appindicator(service: &str) -> bool {
    service.split_once('/').is_some_and(|(_, path)| format!("/{path}").starts_with(APPINDICATOR_PATH_PREFIX))
}

/// #EDE-247: whether a left press opens the item's menu (the taskbar's
/// `menuOnPress`): `ItemIsMenu` (the spec), or Cinnamon's rule: an
/// AppIndicator item opens its menu on a left press, unless its Id is one of
/// [`LEFT_CLICK_ACTIVATE_APPS`].
pub fn left_opens_menu(service: &str, id: &str, item_is_menu: bool) -> bool {
    item_is_menu || (is_appindicator(service) && !LEFT_CLICK_ACTIVATE_APPS.contains(&id.to_lowercase().as_str()))
}

/// A whitelisted AppIndicator item's left press: `SecondaryActivate`.
fn left_is_secondary(service: &str, id: &str) -> bool {
    is_appindicator(service) && LEFT_CLICK_ACTIVATE_APPS.contains(&id.to_lowercase().as_str())
}

/// A `Menu` path that names a dbusmenu (`/` and `/NO_DBUSMENU` say none).
fn names_a_menu(path: &str) -> bool {
    !path.is_empty() && path != "/" && path != "/NO_DBUSMENU"
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    /// A `file:` or `data:` URL, or empty.
    pub icon: String,
    /// `(key, label)` pairs, as the app sent them (with its `default`
    /// action, the press on the notification itself, if it gave one).
    pub actions: Vec<(String, String)>,
    /// The `resident` hint: an invoked action leaves it in place.
    pub resident: bool,
    /// 0 low, 1 normal, 2 critical.
    pub urgency: u8,
    /// `None` = stays until closed.
    pub expires_at: Option<Instant>,
    /// Local `HH:MM`.
    pub at: String,
    /// #EDE-65: when it arrived (epoch ms), to merge it with the inbox.
    pub received_ms: i64,
}

/// The spec's key of the press on the notification itself (#EDE-196): the
/// toast's or the bell row's press, never a button of its own.
pub const DEFAULT_ACTION: &str = "default";

impl Notification {
    /// The row `/notifications` carries: its `actions` are the buttons, so
    /// the `default` action is not one of them (`default` says whether the
    /// app gave it).
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "app": self.app,
            "summary": self.summary,
            "body": self.body,
            "icon": self.icon,
            "actions": self.actions.iter().filter(|(key, _)| key != DEFAULT_ACTION).map(|(key, label)| json!({"key": key, "label": label})).collect::<Vec<_>>(),
            "default": self.has_action(DEFAULT_ACTION),
            "urgency": self.urgency,
            "at": self.at,
        })
    }

    /// Whether the app offered the action `key`.
    pub fn has_action(&self, key: &str) -> bool {
        self.actions.iter().any(|(k, _)| k == key)
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

fn hint_bool(hints: &HashMap<String, OwnedValue>, key: &str) -> bool {
    hints.get(key).and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false)
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
            resident: hint_bool(&hints, "resident"),
            urgency,
            expires_at,
            at: (self.clock)(),
            received_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
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
    /// #EDE-237: its dbusmenu, read with the item (`AboutToShow` first), so
    /// the taskbar's Menu opens on it at once; empty = none (a press activates).
    pub menu: Vec<MenuEntry>,
    /// `ItemIsMenu`: a left click opens the menu too (Cinnamon's xapp
    /// status applet); otherwise it activates and only a right click opens it.
    pub item_is_menu: bool,
}

impl TrayItem {
    /// `/tray`'s row: `menu` = the taskbar Menu's `items` ([`menu_items`]).
    pub fn to_json(&self) -> Value {
        json!({"service": self.service, "title": self.title, "status": self.status, "icon": self.icon, "hasMenu": !self.menu.is_empty(), "menuOnPress": self.item_is_menu && !self.menu.is_empty(), "menu": menu_items(&self.menu)})
    }
}

/// #EDE-237: a dbusmenu as genUI Menu items, the look every panel menu
/// shares: a row's value is its dbusmenu id, a toggle a checkbox row with
/// its state, a separator a separator, a submenu Cinnamon's way: its title
/// as a group label over its rows, inline.
pub fn menu_items(entries: &[MenuEntry]) -> Vec<Value> {
    entries
        .iter()
        .map(|e| match () {
            _ if e.separator => json!({"kind": "separator"}),
            _ if e.submenu => json!({"kind": "label", "label": e.label}),
            _ => {
                let mut row = json!({"label": e.label, "value": e.id.to_string()});
                if let Some(checked) = e.checked {
                    row["kind"] = json!("checkbox");
                    row["checked"] = json!(checked);
                }
                if !e.enabled {
                    row["disabled"] = json!(true);
                }
                row
            }
        })
        .collect()
}

/// A dbusmenu row, flattened one level (a submenu's rows follow it).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub checked: Option<bool>,
    pub depth: u32,
    /// The title of a submenu (its rows follow, one level deeper).
    pub submenu: bool,
}

/// The main loop's handle.
pub struct Dbus {
    conn: Connection,
    items: Arc<Mutex<Vec<String>>>,
    tx: mpsc::Sender<Event>,
    tray: mpsc::Sender<TrayRequest>,
    /// #EDE-247: the taskbar's output scale the icons are drawn at.
    tray_scale: Arc<Mutex<f64>>,
}

impl Dbus {
    /// Serves both objects on the session bus and takes the well-known
    /// names when nobody else holds them (another desktop's server keeps
    /// them while eDE runs nested; the objects still answer on the unique
    /// name). `None` = no session bus.
    pub fn start(tx: mpsc::Sender<Event>, clock: fn() -> String) -> Option<Dbus> {
        Self::start_on(zbus::blocking::connection::Builder::session(), tx, clock)
    }

    /// [`Dbus::start`] on the bus `builder` reaches (a test's private bus).
    pub(crate) fn start_on(builder: zbus::Result<zbus::blocking::connection::Builder<'static>>, tx: mpsc::Sender<Event>, clock: fn() -> String) -> Option<Dbus> {
        let items = Arc::new(Mutex::new(Vec::new()));
        let notifications = Notifications { tx: tx.clone(), next_id: Arc::new(AtomicU32::new(1)), clock };
        let watcher = Watcher { items: items.clone(), tx: tx.clone() };
        let conn = match builder.and_then(|b| b.method_timeout(TRAY_CALL_TIMEOUT).serve_at(NOTIFICATIONS_PATH, notifications)).and_then(|b| b.serve_at(WATCHER_PATH, watcher)).and_then(|b| b.build()) {
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
        let tray_scale = Arc::new(Mutex::new(1.0));
        let tray = Tray { conn: conn.clone(), items: items.clone(), scale: tray_scale.clone() }.spawn(tx.clone());
        let dbus = Dbus { conn, items, tx, tray, tray_scale };
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

    /// `ActionInvoked(id, key)`: the person pressed the action `key` (the
    /// spec's `default` = the notification itself).
    pub fn action_invoked(&self, id: u32, key: &str) {
        let _ = self.conn.emit_signal(None::<BusName<'_>>, NOTIFICATIONS_PATH, NOTIFICATIONS_NAME, "ActionInvoked", &(id, key));
    }

    /// Reads the tray again: [`Event::TrayItems`].
    pub fn tray_refresh(&self) {
        let _ = self.tray.send(TrayRequest::Refresh);
    }

    /// A left click: `Activate`.
    pub fn tray_activate(&self, service: &str) {
        let _ = self.tray.send(TrayRequest::Activate(service.to_string()));
    }

    /// A menu row clicked.
    pub fn tray_menu_event(&self, service: &str, id: i32) {
        let _ = self.tray.send(TrayRequest::MenuEvent(service.to_string(), id));
    }

    /// #EDE-247: the middle or right button or the wheel on an item.
    pub fn tray_input(&self, service: &str, input: TrayInput) {
        let service = service.to_string();
        let _ = self.tray.send(match input {
            TrayInput::Middle => TrayRequest::SecondaryActivate(service),
            TrayInput::Right => TrayRequest::ContextMenu(service),
            TrayInput::Scroll { delta, horizontal } => TrayRequest::Scroll(service, delta, if horizontal { "horizontal" } else { "vertical" }),
        });
    }

    /// #EDE-247: the taskbar's output scale: the icons are read again at it
    /// when it changes (the rule's size in device pixels, [`crate::tray_icon`]).
    pub fn tray_scale(&self, scale: f64) {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let mut now = self.tray_scale.lock().unwrap_or_else(|p| p.into_inner());
        if (*now - scale).abs() > f64::EPSILON {
            *now = scale;
            let _ = self.tray.send(TrayRequest::Refresh);
        }
    }
}

/// The tray thread's side: the calls to the items (#EDE-135).
struct Tray {
    conn: Connection,
    items: Arc<Mutex<Vec<String>>>,
    scale: Arc<Mutex<f64>>,
}

/// The requests one turn of the tray thread runs: in order, each refresh
/// dropped but the last (one read answers them all).
fn coalesce(mut batch: Vec<TrayRequest>) -> Vec<TrayRequest> {
    if let Some(last) = batch.iter().rposition(|r| *r == TrayRequest::Refresh) {
        let mut i = 0;
        batch.retain(|r| {
            i += 1;
            *r != TrayRequest::Refresh || i - 1 == last
        });
    }
    batch
}

impl Tray {
    fn spawn(self, tx: mpsc::Sender<Event>) -> mpsc::Sender<TrayRequest> {
        let (requests, rx) = mpsc::channel::<TrayRequest>();
        std::thread::Builder::new()
            .name("ede-shell-tray".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut batch = vec![first];
                    batch.extend(rx.try_iter());
                    for request in coalesce(batch) {
                        let event = match request {
                            TrayRequest::Refresh => Some(Event::TrayItems(self.tray_items())),
                            TrayRequest::Activate(service) => {
                                self.tray_activate(&service, 0, 0);
                                None
                            }
                            TrayRequest::MenuEvent(service, id) => {
                                self.tray_menu_event(&service, id);
                                None
                            }
                            TrayRequest::SecondaryActivate(service) => {
                                self.tray_call(&service, "SecondaryActivate", &(0i32, 0i32));
                                None
                            }
                            // Cinnamon calls ContextMenu only on an item without a
                            // dbusmenu; one with a menu opens it in the taskbar.
                            TrayRequest::ContextMenu(service) => {
                                let menu = self.item_proxy(&service).and_then(|p| p.get_property::<OwnedObjectPath>("Menu").ok());
                                if !menu.is_some_and(|m| names_a_menu(m.as_str())) {
                                    self.tray_call(&service, "ContextMenu", &(0i32, 0i32));
                                }
                                None
                            }
                            TrayRequest::Scroll(service, delta, orientation) => {
                                self.tray_call(&service, "Scroll", &(delta, orientation));
                                None
                            }
                        };
                        if let Some(event) = event {
                            if tx.send(event).is_err() {
                                return;
                            }
                        }
                    }
                }
            })
            .expect("the tray thread");
        requests
    }

    fn item_proxy(&self, service: &str) -> Option<Proxy<'static>> {
        let (name, path) = service.split_once('/').map(|(n, p)| (n.to_string(), format!("/{p}")))?;
        uncached_proxy(&self.conn, name, path, ITEM_INTERFACE)
    }

    /// Every registered item, read fresh (a dead one is dropped, a
    /// `Passive` one hidden as Cinnamon does). #EDE-247: its icon drawn at the
    /// panel's icon size and the taskbar's scale ([`crate::tray_icon`]),
    /// `NeedsAttention` with the attention icon first.
    fn tray_items(&self) -> Vec<TrayItem> {
        let services = self.items.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let scale = *self.scale.lock().unwrap_or_else(|p| p.into_inner());
        services
            .iter()
            .filter_map(|service| {
                let proxy = self.item_proxy(service)?;
                let title: String = proxy.get_property("Title").unwrap_or_default();
                let status: String = proxy.get_property("Status").unwrap_or_else(|_| "Active".into());
                if status == "Passive" {
                    return None;
                }
                let pixmaps = |prop: &str| proxy.get_property::<Vec<(i32, i32, Vec<u8>)>>(prop).unwrap_or_default();
                let named = |prop: &str| proxy.get_property::<String>(prop).unwrap_or_default();
                let theme_path = named("IconThemePath");
                let source = |name: &str, pixmap: &str| crate::tray_icon::Source { name: named(name), theme_path: theme_path.clone(), pixmaps: pixmaps(pixmap) };
                let draw = |s: &crate::tray_icon::Source| crate::tray_icon::render(s, crate::tray_icon::PANEL_HEIGHT, scale);
                let icon = (status == "NeedsAttention")
                    .then(|| draw(&source("AttentionIconName", "AttentionIconPixmap")))
                    .flatten()
                    .or_else(|| draw(&source("IconName", "IconPixmap")))
                    .unwrap_or_default();
                let menu: Option<OwnedObjectPath> = proxy.get_property("Menu").ok();
                let item_is_menu: bool = proxy.get_property("ItemIsMenu").unwrap_or(false);
                let has_menu = menu.as_ref().is_some_and(|m| names_a_menu(m.as_str()));
                let id: String = proxy.get_property("Id").unwrap_or_default();
                if title.is_empty() && icon.is_empty() && id.is_empty() {
                    return None;
                }
                let menu = if has_menu { self.tray_menu(service).unwrap_or_default() } else { Vec::new() };
                let item_is_menu = left_opens_menu(service, &id, item_is_menu);
                Some(TrayItem { service: service.clone(), title: if title.is_empty() { id } else { title }, status, icon, menu, item_is_menu })
            })
            .collect()
    }

    /// A left click: `Activate`; a whitelisted AppIndicator item's
    /// `SecondaryActivate` (Cinnamon's `left-click-activate-apps`). Neither
    /// falls back to the menu when it fails: Cinnamon's does not either.
    fn tray_activate(&self, service: &str, x: i32, y: i32) {
        let secondary = self.item_proxy(service).is_some_and(|p| left_is_secondary(service, &p.get_property::<String>("Id").unwrap_or_default()));
        self.tray_call(service, if secondary { "SecondaryActivate" } else { "Activate" }, &(x, y));
    }

    /// `method` on the item, bounded by [`TRAY_CALL_TIMEOUT`]; a failure is logged.
    fn tray_call<B>(&self, service: &str, method: &str, body: &B)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        if let Some(proxy) = self.item_proxy(service) {
            if let Err(e) = proxy.call_method(method, body) {
                tracing::info!(service, method, "tray item: {e}");
            }
        }
    }

    fn menu_proxy(&self, service: &str) -> Option<Proxy<'static>> {
        let proxy = self.item_proxy(service)?;
        let menu: OwnedObjectPath = proxy.get_property("Menu").ok()?;
        if !names_a_menu(menu.as_str()) {
            return None;
        }
        let name = service.split_once('/')?.0.to_string();
        uncached_proxy(&self.conn, name, menu.as_str().to_string(), MENU_INTERFACE)
    }

    /// The item's dbusmenu, two levels deep.
    fn tray_menu(&self, service: &str) -> Option<Vec<MenuEntry>> {
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
    fn tray_menu_event(&self, service: &str, id: i32) {
        if let Some(proxy) = self.menu_proxy(service) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
            if let Err(e) = proxy.call_method("Event", &(id, "clicked", ZValue::from(0i32), now)) {
                tracing::info!(service, id, "menu Event: {e}");
            }
        }
    }
}

/// A proxy that reads each property when asked: a caching one waits for its
/// `GetAll` without the connection's call timeout (a hung item hung the read).
fn uncached_proxy(conn: &Connection, name: String, path: String, interface: &'static str) -> Option<Proxy<'static>> {
    zbus::blocking::proxy::Builder::<Proxy<'static>>::new(conn)
        .destination(name)
        .and_then(|b| b.path(path))
        .and_then(|b| b.interface(interface))
        .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
        .and_then(|b| b.build())
        .ok()
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
        let submenu = get_str("children-display").as_deref() == Some("submenu") && depth < 1;
        out.push(MenuEntry { id: *id, label: get_str("label").unwrap_or_default().replace("__", "\u{0}").replace('_', "").replace('\u{0}', "_"), enabled: get_bool("enabled", true), separator, checked, depth, submenu });
        if submenu {
            flatten_layout(&child, depth + 1, out);
        }
    }
}

#[cfg(test)]
pub(crate) mod private_bus {
    //! A dbus-daemon of the test's own (never the session bus).

    pub struct PrivateBus {
        child: std::process::Child,
        pub address: String,
        _dir: tempfile::TempDir,
    }

    impl PrivateBus {
        pub fn start() -> PrivateBus {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("bus");
            let config = dir.path().join("bus.conf");
            std::fs::write(&config, format!(r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path={}</listen><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#, socket.display())).unwrap();
            let child = std::process::Command::new("dbus-daemon").arg("--nofork").arg(format!("--config-file={}", config.display())).stdout(std::process::Stdio::null()).spawn().expect("dbus-daemon");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !socket.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            PrivateBus { child, address: format!("unix:path={}", socket.display()), _dir: dir }
        }

        pub fn builder(&self) -> zbus::Result<zbus::blocking::connection::Builder<'static>> {
            zbus::blocking::connection::Builder::address(self.address.as_str())
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tray item whose app stopped answering (SIGSTOP, a debugger).
    struct HungItem;

    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl HungItem {
        #[zbus(property)]
        async fn title(&self) -> String {
            std::future::pending::<()>().await;
            String::new()
        }

        async fn activate(&self, _x: i32, _y: i32) {
            std::future::pending::<()>().await;
        }
    }

    fn clock() -> String {
        "12:00".into()
    }

    /// The next event that is not a [`Event::TrayChanged`], within `t`.
    fn next_answer(rx: &mpsc::Receiver<Event>, t: Duration) -> Option<Event> {
        let deadline = Instant::now() + t;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(Event::TrayChanged) => continue,
                Ok(e) => return Some(e),
                Err(_) => return None,
            }
        }
        None
    }

    /// #EDE-135: a hung tray item answers nothing; the shell's calls return
    /// at once and the tray thread gives up on it.
    #[test]
    fn a_hung_tray_item_does_not_hang_the_tray() {
        let bus = private_bus::PrivateBus::start();
        let (tx, rx) = mpsc::channel();
        let dbus = Dbus::start_on(bus.builder(), tx, clock).expect("the shell on the private bus");
        let item = bus.builder().and_then(|b| b.serve_at("/StatusNotifierItem", HungItem)).and_then(|b| b.build()).unwrap();
        let me = item.unique_name().unwrap().to_string();
        item.call_method(Some(WATCHER_NAME), WATCHER_PATH, Some(WATCHER_NAME), "RegisterStatusNotifierItem", &(me.as_str(),)).unwrap();
        let service = format!("{me}/StatusNotifierItem");

        // A press and a read, as the main loop makes them: neither waits.
        let asked = Instant::now();
        dbus.tray_activate(&service);
        dbus.tray_menu_event(&service, 1);
        dbus.tray_refresh();
        dbus.tray_refresh();
        assert!(asked.elapsed() < Duration::from_millis(100), "the shell waited {:?}", asked.elapsed());
        // The read comes back once, without the item that never answered its title.
        match next_answer(&rx, Duration::from_secs(10)) {
            Some(Event::TrayItems(items)) => assert!(items.iter().all(|i| i.service != service), "an item with nothing to show: {items:?}"),
            other => panic!("the tray read hung: {other:?}"),
        }
        assert!(next_answer(&rx, Duration::from_millis(1500)).is_none(), "two refreshes asked together read once");
    }

    #[test]
    fn refreshes_asked_together_read_once_after_the_presses() {
        use TrayRequest::*;
        let batch = vec![Refresh, Activate("a".into()), Refresh, MenuEvent("b".into(), 3), Refresh];
        assert_eq!(coalesce(batch), vec![Activate("a".into()), MenuEvent("b".into(), 3), Refresh]);
        assert_eq!(coalesce(vec![MenuEvent("a".into(), 1)]), vec![MenuEvent("a".into(), 1)]);
    }

    #[test]
    fn item_addresses_follow_the_spec() {
        assert_eq!(item_address(":1.42", ":1.9"), (":1.42".into(), "/StatusNotifierItem".into()));
        assert_eq!(item_address("/org/ayatana/NotificationItem/nm", ":1.9"), (":1.9".into(), "/org/ayatana/NotificationItem/nm".into()));
        assert_eq!(item_address("org.kde.StatusNotifierItem-7-1/StatusNotifierItem", ":1.9"), ("org.kde.StatusNotifierItem-7-1".into(), "/StatusNotifierItem".into()));
    }

    #[test]
    fn bodies_lose_their_markup() {
        assert_eq!(strip_markup("<b>Build</b> done &amp; <a href=\"x\">shipped</a>"), "Build done & shipped");
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
        assert_eq!(out.iter().map(|e| e.submenu).collect::<Vec<_>>(), [false, true, false]);
    }

    /// #EDE-237: a tray item's dbusmenu as the panel's Menu items: ids as
    /// values, toggles as checkbox rows, a submenu's title as a group label.
    #[test]
    fn a_dbusmenu_becomes_menu_items() {
        let row = |id: i32, label: &str| MenuEntry { id, label: label.into(), enabled: true, separator: false, checked: None, depth: 0, submenu: false };
        let entries = vec![
            row(1, "Show window"),
            MenuEntry { checked: Some(true), ..row(2, "Mute") },
            MenuEntry { separator: true, ..row(3, "") },
            MenuEntry { submenu: true, ..row(4, "Devices") },
            MenuEntry { depth: 1, enabled: false, ..row(5, "Headset") },
        ];
        assert_eq!(
            menu_items(&entries),
            vec![
                json!({"label": "Show window", "value": "1"}),
                json!({"label": "Mute", "value": "2", "kind": "checkbox", "checked": true}),
                json!({"kind": "separator"}),
                json!({"kind": "label", "label": "Devices"}),
                json!({"label": "Headset", "value": "5", "disabled": true}),
            ]
        );
        let item = TrayItem { service: "a/b".into(), title: "T".into(), status: "Active".into(), icon: String::new(), menu: entries, item_is_menu: true };
        assert_eq!(item.to_json()["menuOnPress"], json!(true));
        assert_eq!(item.to_json()["hasMenu"], json!(true));
        assert_eq!(TrayItem { menu: Vec::new(), ..item }.to_json()["hasMenu"], json!(false));
    }

    /// An item that writes down every call it gets.
    struct Recorder {
        id: &'static str,
        status: &'static str,
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl Recorder {
        #[zbus(property)]
        fn id(&self) -> String {
            self.id.into()
        }
        #[zbus(property)]
        fn title(&self) -> String {
            self.id.into()
        }
        #[zbus(property)]
        fn status(&self) -> String {
            self.status.into()
        }
        #[zbus(property)]
        fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
            vec![(24, 24, vec![0xff; 24 * 24 * 4])]
        }
        fn activate(&self, x: i32, y: i32) {
            self.calls.lock().unwrap().push(format!("Activate {x} {y}"));
        }
        fn secondary_activate(&self, x: i32, y: i32) {
            self.calls.lock().unwrap().push(format!("SecondaryActivate {x} {y}"));
        }
        fn context_menu(&self, x: i32, y: i32) {
            self.calls.lock().unwrap().push(format!("ContextMenu {x} {y}"));
        }
        fn scroll(&self, delta: i32, orientation: String) {
            self.calls.lock().unwrap().push(format!("Scroll {delta} {orientation}"));
        }
    }

    /// `path` served by a Recorder on its own connection, registered with
    /// the watcher the way its kind does (an AppIndicator by its path).
    fn recorder(bus: &private_bus::PrivateBus, path: &str, id: &'static str, status: &'static str) -> (zbus::blocking::Connection, String, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let conn = bus.builder().and_then(|b| b.serve_at(path, Recorder { id, status, calls: calls.clone() })).and_then(|b| b.build()).unwrap();
        let me = conn.unique_name().unwrap().to_string();
        let registered = if path == "/StatusNotifierItem" { me.clone() } else { path.to_string() };
        conn.call_method(Some(WATCHER_NAME), WATCHER_PATH, Some(WATCHER_NAME), "RegisterStatusNotifierItem", &(registered.as_str(),)).unwrap();
        (conn, format!("{me}{path}"), calls)
    }

    fn wait_for(calls: &Arc<Mutex<Vec<String>>>, n: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while calls.lock().unwrap().len() < n && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        calls.lock().unwrap().clone()
    }

    /// #EDE-247: Cinnamon's click semantics (xapp-sn-watcher's sn-item.c),
    /// the same for every item: left = Activate, middle = SecondaryActivate,
    /// right without a dbusmenu = ContextMenu, the wheel = Scroll(±1, axis)
    /// with up and left negative.
    #[test]
    fn tray_input_reaches_the_item_as_cinnamon_sends_it() {
        let bus = private_bus::PrivateBus::start();
        let (tx, _rx) = mpsc::channel();
        let dbus = Dbus::start_on(bus.builder(), tx, clock).expect("the shell on the private bus");
        let (_c, service, calls) = recorder(&bus, "/StatusNotifierItem", "plain", "Active");
        dbus.tray_activate(&service);
        dbus.tray_input(&service, TrayInput::Middle);
        dbus.tray_input(&service, TrayInput::Right);
        for (button, direction) in [(None, Some("up")), (None, Some("down")), (None, Some("left")), (None, Some("right"))] {
            dbus.tray_input(&service, TrayInput::from_event(button, direction).unwrap());
        }
        assert_eq!(
            wait_for(&calls, 7),
            ["Activate 0 0", "SecondaryActivate 0 0", "ContextMenu 0 0", "Scroll -1 vertical", "Scroll 1 vertical", "Scroll -1 horizontal", "Scroll 1 horizontal"]
        );
        // Cinnamon's left-click-activate-apps: such an AppIndicator's left press is SecondaryActivate.
        let (_c2, onboard, calls) = recorder(&bus, "/org/ayatana/NotificationItem/onboard", "onboard", "Active");
        dbus.tray_activate(&onboard);
        assert_eq!(wait_for(&calls, 1), ["SecondaryActivate 0 0"]);
    }

    #[test]
    fn the_taskbar_s_events_name_the_input() {
        assert_eq!(TrayInput::from_event(Some("middle"), None), Some(TrayInput::Middle));
        assert_eq!(TrayInput::from_event(Some("right"), None), Some(TrayInput::Right));
        assert_eq!(TrayInput::from_event(None, Some("up")), Some(TrayInput::Scroll { delta: -1, horizontal: false }));
        assert_eq!(TrayInput::from_event(Some("left"), None), None, "a left press is the press event, not auxPress");
        assert_eq!(TrayInput::from_event(None, None), None);
    }

    /// #EDE-247: an AppIndicator item's left press opens its menu (Steam,
    /// Discord's Electron, nm-applet), unless whitelisted; ItemIsMenu too.
    #[test]
    fn appindicators_open_their_menu_on_a_left_press() {
        assert!(left_opens_menu(":1.4/org/ayatana/NotificationItem/steam", "steam", false));
        assert!(!left_opens_menu(":1.4/org/ayatana/NotificationItem/onboard", "Onboard", false));
        assert!(!left_opens_menu(":1.5/StatusNotifierItem", "discord_status_icon_1", false));
        assert!(left_opens_menu(":1.5/StatusNotifierItem", "x", true));
        assert!(!names_a_menu("/") && !names_a_menu("/NO_DBUSMENU") && !names_a_menu(""));
        assert!(names_a_menu("/MenuBar"));
    }

    /// #EDE-247: a Passive item is hidden (Cinnamon), the icon is drawn at
    /// the taskbar's scale: 24 px slots, 30 device pixels at 1.25.
    #[test]
    fn tray_items_hide_passive_ones_and_draw_at_the_scale() {
        let bus = private_bus::PrivateBus::start();
        let (tx, rx) = mpsc::channel();
        let dbus = Dbus::start_on(bus.builder(), tx, clock).expect("the shell on the private bus");
        let (_a, active, _) = recorder(&bus, "/org/ayatana/NotificationItem/steam", "steam", "Active");
        let (_p, passive, _) = recorder(&bus, "/StatusNotifierItem", "quiet", "Passive");
        dbus.tray_scale(1.25);
        let deadline = Instant::now() + Duration::from_secs(10);
        let items = loop {
            dbus.tray_refresh();
            match next_answer(&rx, Duration::from_secs(5)) {
                Some(Event::TrayItems(items)) if items.iter().any(|i| i.service == active) => break items,
                _ if Instant::now() > deadline => panic!("the items never came"),
                _ => {}
            }
        };
        assert!(items.iter().all(|i| i.service != passive), "Passive is hidden");
        let steam = items.iter().find(|i| i.service == active).unwrap();
        assert!(steam.item_is_menu, "an AppIndicator opens its menu on a left press");
        use base64::Engine as _;
        let b64 = steam.icon.strip_prefix("data:image/png;base64,").expect("a PNG data URL");
        let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8().dimensions(), (30, 30));
    }
}
