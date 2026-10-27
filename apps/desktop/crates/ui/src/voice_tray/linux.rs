//! NEW-68: the Linux voice tray, our own StatusNotifierItem + dbusmenu over
//! zbus (no GTK). Not `ksni`: its Unlicense is deliberately off deny.toml's
//! allow-list (a public-domain dedication does not hold in Germany).
//!
//! Two objects on one session-bus connection, owned by a worker thread (the
//! D-Bus calls block): `org.kde.StatusNotifierItem` at `/StatusNotifierItem`
//! (the app icon, Title, ToolTip; Activate brings the IDE up) and
//! `com.canonical.dbusmenu` at `/MenuBar` (the fixture's rows: GetLayout,
//! Event, AboutToShow; LayoutUpdated on every new menu). The item registers
//! with `org.kde.StatusNotifierWatcher` and registers again whenever a
//! watcher appears or its host changes (a panel restart); while no host
//! shows it (GNOME without the AppIndicator extension) the last window quits
//! as before, and losing the host with no window open brings one back.
//!
//! #NEW-76: the watcher's listener (its own session-bus connection) runs only
//! while a tray is wanted, i.e. while this IDE is the voice device; hiding
//! the tray stops it. #NEW-56: a new icon (the microphone dot) is announced
//! with `NewIcon`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use domain::voice::{TrayIcon, TrayItem};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::TrayEvent;

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

/// One icon frame as SNI wants it: (width, height, ARGB32 big-endian).
type Pixmap = (i32, i32, Vec<u8>);

/// What the worker thread acts on.
enum Command {
    /// A new menu (None = no tray).
    Menu(Option<Vec<TrayItem>>),
    /// A watcher came or went, or its host changed: register again and
    /// re-read whether a host shows us.
    Recheck,
    /// #NEW-56: draw this icon.
    Icon(TrayIcon),
}

pub(super) struct Backend {
    tx: flume::Sender<Command>,
    registered: Arc<AtomicBool>,
}

impl Backend {
    pub(super) fn new(clicks: flume::Sender<TrayEvent>) -> Self {
        let (tx, rx) = flume::unbounded();
        let registered = Arc::new(AtomicBool::new(false));
        let flag = registered.clone();
        let spawned = std::thread::Builder::new().name("voice-tray".into()).spawn(move || run(rx, clicks, flag));
        if let Err(err) = spawned {
            log::warn!("[ui] voice tray: thread failed to start: {err}");
        }
        Self { tx, registered }
    }

    pub(super) fn set(&mut self, menu: Option<Vec<TrayItem>>) {
        let _ = self.tx.send(Command::Menu(menu));
    }

    pub(super) fn set_icon(&mut self, icon: TrayIcon) {
        let _ = self.tx.send(Command::Icon(icon));
    }

    /// A host shows the item right now.
    pub(super) fn registered(&self) -> bool {
        self.registered.load(Ordering::SeqCst)
    }
}

/// The watcher's listener; dropping it ends its thread and closes its
/// session-bus connection (#NEW-76).
struct Watch {
    _stop: flume::Sender<()>,
}

/// Recheck whenever `org.kde.StatusNotifierWatcher` changes owner (a panel
/// restarts, the AppIndicator extension toggles) or announces a host change.
fn watch(tx: flume::Sender<Command>) -> Option<Watch> {
    let (stop, stopped) = flume::bounded::<()>(1);
    let spawned = std::thread::Builder::new().name("voice-tray-watch".into()).spawn(move || {
        async_io::block_on(async move {
            use futures_util::StreamExt as _;
            let connection = match zbus::Connection::session().await {
                Ok(connection) => connection,
                Err(err) => {
                    log::info!("[ui] voice tray: cannot watch the tray host: {err}");
                    return;
                }
            };
            let mut streams = Vec::new();
            for rule in watch_rules() {
                match rule {
                    Ok(rule) => match zbus::MessageStream::for_match_rule(rule, &connection, None).await {
                        Ok(stream) => streams.push(stream),
                        Err(err) => log::info!("[ui] voice tray: cannot watch the tray host: {err}"),
                    },
                    Err(err) => log::info!("[ui] voice tray: cannot watch the tray host: {err}"),
                }
            }
            let mut signals = futures_util::stream::select_all(streams);
            loop {
                let signal = std::pin::pin!(signals.next());
                let stopping = std::pin::pin!(stopped.recv_async());
                match futures_util::future::select(signal, stopping).await {
                    futures_util::future::Either::Left((Some(_), _)) => {
                        if tx.send(Command::Recheck).is_err() {
                            break;
                        }
                    }
                    // The streams ended, or the tray is no longer wanted.
                    _ => break,
                }
            }
        });
    });
    match spawned {
        Ok(_) => Some(Watch { _stop: stop }),
        Err(err) => {
            log::warn!("[ui] voice tray: watch thread failed to start: {err}");
            None
        }
    }
}

/// The watcher's owner changing, and its host registering (#NEW-76: never
/// every signal of the watcher, or another app's item would re-register ours).
fn watch_rules() -> [zbus::Result<zbus::MatchRule<'static>>; 2] {
    [
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .and_then(|rule| rule.interface("org.freedesktop.DBus"))
            .and_then(|rule| rule.member("NameOwnerChanged"))
            .and_then(|rule| rule.add_arg(WATCHER))
            .map(|rule| rule.build()),
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(WATCHER)
            .and_then(|rule| rule.member("StatusNotifierHostRegistered"))
            .map(|rule| rule.build()),
    ]
}

/// The menu the dbusmenu object serves, shared with its zbus handlers.
#[derive(Default)]
struct MenuState {
    items: Vec<TrayItem>,
    revision: u32,
}

/// The item's live connection and the well-known name it registered.
struct Live {
    connection: Connection,
    name: String,
}

/// The icon in `state` as SNI pixmaps.
fn pixmaps(state: TrayIcon) -> Vec<Pixmap> {
    super::icon_frames(state, &[22, 32, 48, 64])
        .into_iter()
        .map(|frame| {
            let argb = frame.rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
            (frame.width as i32, frame.height as i32, argb)
        })
        .collect()
}

fn run(rx: flume::Receiver<Command>, clicks: flume::Sender<TrayEvent>, registered: Arc<AtomicBool>) {
    let mut drawn = TrayIcon::App;
    let icon = Arc::new(Mutex::new(pixmaps(drawn)));
    let state = Arc::new(Mutex::new(MenuState::default()));
    let mut live: Option<Live> = None;
    // #NEW-76: the watcher's listener, only while a tray is wanted.
    let mut watching: Option<Watch> = None;
    let (recheck, rechecks) = flume::unbounded::<Command>();
    // Whether a tray is wanted (this IDE is the voice device).
    let mut wanted = false;
    // A fresh well-known name per connection (`-{pid}-{n}`).
    let mut serial = 0u32;
    // Flip the flag; losing a host tells the IDE (no window = bring one up).
    let set_registered = |now: bool| {
        if registered.swap(now, Ordering::SeqCst) && !now {
            let _ = clicks.send(TrayEvent::HostLost);
        }
    };
    loop {
        let command = flume::Selector::new()
            .recv(&rx, |command| command.ok())
            .recv(&rechecks, |command| command.ok())
            .wait();
        let Some(command) = command else { break };
        match command {
            Command::Icon(next) => {
                if next != drawn {
                    drawn = next;
                    *icon.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = pixmaps(next);
                    if let Some(live) = live.as_ref() {
                        let announced = live.connection.emit_signal(
                            None::<zbus::names::BusName<'_>>,
                            ITEM_PATH,
                            "org.kde.StatusNotifierItem",
                            "NewIcon",
                            &(),
                        );
                        if let Err(err) = announced {
                            log::warn!("[ui] voice tray: NewIcon: {err}");
                        }
                    }
                }
                continue;
            }
            Command::Menu(None) => {
                wanted = false;
                watching = None;
                // Closing the connection drops the name; the watcher forgets it.
                live = None;
                // Hidden on purpose: not a lost host.
                registered.store(false, Ordering::SeqCst);
                continue;
            }
            Command::Menu(Some(items)) => {
                wanted = true;
                if watching.is_none() {
                    watching = watch(recheck.clone());
                }
                let revision = {
                    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.items = items;
                    state.revision += 1;
                    state.revision
                };
                if let Some(live) = live.as_ref() {
                    let updated = live.connection.emit_signal(
                        None::<zbus::names::BusName<'_>>,
                        MENU_PATH,
                        MENU_INTERFACE,
                        "LayoutUpdated",
                        &(revision, 0i32),
                    );
                    if let Err(err) = updated {
                        log::warn!("[ui] voice tray: LayoutUpdated: {err}");
                    }
                    continue;
                }
            }
            Command::Recheck => {
                if !wanted {
                    continue;
                }
                if let Some(live) = live.as_ref() {
                    set_registered(register(live).is_ok_and(|()| host_registered(&live.connection)));
                    continue;
                }
            }
        }
        // Wanted with no connection yet (the first menu, or a watcher that
        // was missing until now).
        serial += 1;
        match connect(serial, &icon, &state, &clicks) {
            Ok(opened) => {
                let shown = register(&opened).is_ok_and(|()| host_registered(&opened.connection));
                live = Some(opened);
                set_registered(shown);
            }
            Err(err) => {
                log::info!("[ui] voice tray: no session bus: {err}");
                set_registered(false);
            }
        }
    }
}

fn connect(
    serial: u32,
    icon: &Arc<Mutex<Vec<Pixmap>>>,
    state: &Arc<Mutex<MenuState>>,
    clicks: &flume::Sender<TrayEvent>,
) -> zbus::Result<Live> {
    let name = format!("org.kde.StatusNotifierItem-{}-{serial}", std::process::id());
    let item = Item { icon: icon.clone(), clicks: clicks.clone() };
    let menu = DbusMenu { state: state.clone(), clicks: clicks.clone() };
    let connection = zbus::blocking::connection::Builder::session()?
        .name(name.as_str())?
        .serve_at(ITEM_PATH, item)?
        .serve_at(MENU_PATH, menu)?
        .build()?;
    Ok(Live { connection, name })
}

/// Register with the watcher (again: a restarted watcher forgot us).
fn register(live: &Live) -> zbus::Result<()> {
    live.connection
        .call_method(Some(WATCHER), "/StatusNotifierWatcher", Some(WATCHER), "RegisterStatusNotifierItem", &(live.name.as_str(),))
        .map(|_| ())
        .inspect_err(|err| log::info!("[ui] voice tray: no tray host: {err}"))
}

/// A host draws registered items (GNOME's watcher without the extension
/// has none).
fn host_registered(connection: &Connection) -> bool {
    let reply = connection.call_method(
        Some(WATCHER),
        "/StatusNotifierWatcher",
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(WATCHER, "IsStatusNotifierHostRegistered"),
    );
    reply
        .and_then(|reply| reply.body().deserialize::<OwnedValue>())
        .ok()
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

/// `org.kde.StatusNotifierItem`.
struct Item {
    /// #NEW-56: swapped for the microphone dot (then `NewIcon`).
    icon: Arc<Mutex<Vec<Pixmap>>>,
    clicks: flume::Sender<TrayEvent>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    /// A left click: bring the IDE up (the menu is a right click).
    fn activate(&self, _x: i32, _y: i32) {
        let _ = self.clicks.send(TrayEvent::Activate);
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {}

    /// The host draws the menu from `Menu`.
    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: String) {}

    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "exponential-voice"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        super::TITLE
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        self.icon.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    /// (icon name, icon pixmaps, title, description).
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        (String::new(), Vec::new(), super::TITLE.to_string(), String::new())
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("a valid object path")
    }
}

/// `com.canonical.dbusmenu`: id 0 = the root, row `n` of the menu = id `n + 1`.
struct DbusMenu {
    state: Arc<Mutex<MenuState>>,
    clicks: flume::Sender<TrayEvent>,
}

type Props = HashMap<String, OwnedValue>;
/// `(ia{sv}av)`.
type Layout = (i32, Props, Vec<OwnedValue>);

fn owned<'a>(value: impl Into<Value<'a>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("no file descriptors in menu values")
}

/// dbusmenu reads `_` as a mnemonic marker.
fn label(text: &str) -> String {
    text.replace('_', "__")
}

fn item_props(item: &TrayItem) -> Props {
    let mut props = Props::new();
    match item {
        TrayItem::Separator => {
            props.insert("type".into(), owned("separator"));
        }
        TrayItem::Action { label: text, enabled, .. } => {
            props.insert("label".into(), owned(label(text)));
            props.insert("enabled".into(), owned(*enabled));
        }
        TrayItem::Check { label: text, checked, enabled, .. } => {
            props.insert("label".into(), owned(label(text)));
            props.insert("enabled".into(), owned(*enabled));
            props.insert("toggle-type".into(), owned("checkmark"));
            props.insert("toggle-state".into(), owned(i32::from(*checked)));
        }
    }
    props
}

fn root_props() -> Props {
    let mut props = Props::new();
    props.insert("children-display".into(), owned("submenu"));
    props
}

impl DbusMenu {
    fn items(&self) -> (Vec<TrayItem>, u32) {
        let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.items.clone(), state.revision)
    }

    fn props_of(&self, id: i32, items: &[TrayItem]) -> Option<Props> {
        if id == 0 {
            return Some(root_props());
        }
        items.get(usize::try_from(id - 1).ok()?).map(item_props)
    }

    fn clicked(&self, id: i32, event_id: &str) -> bool {
        let (items, _) = self.items();
        let Some(item) = usize::try_from(id - 1).ok().and_then(|index| items.get(index)) else { return false };
        if event_id == "clicked" {
            match item {
                TrayItem::Action { id, enabled: true, .. } | TrayItem::Check { id, enabled: true, .. } => {
                    let _ = self.clicks.send(TrayEvent::Item(*id));
                }
                _ => {}
            }
        }
        true
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    /// The tree under `parent_id` (the whole menu is one level deep).
    fn get_layout(&self, parent_id: i32, _recursion_depth: i32, _property_names: Vec<String>) -> zbus::fdo::Result<(u32, Layout)> {
        let (items, revision) = self.items();
        let Some(props) = self.props_of(parent_id, &items) else {
            return Err(zbus::fdo::Error::InvalidArgs(format!("no menu item {parent_id}")));
        };
        let children = if parent_id == 0 {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| owned((index as i32 + 1, item_props(item), Vec::<OwnedValue>::new())))
                .collect()
        } else {
            Vec::new()
        };
        Ok((revision, (parent_id, props, children)))
    }

    fn get_group_properties(&self, ids: Vec<i32>, _property_names: Vec<String>) -> Vec<(i32, Props)> {
        let (items, _) = self.items();
        let ids = if ids.is_empty() { (0..=items.len() as i32).collect() } else { ids };
        ids.into_iter().filter_map(|id| self.props_of(id, &items).map(|props| (id, props))).collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let (items, _) = self.items();
        self.props_of(id, &items)
            .and_then(|mut props| props.remove(&name))
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name} on {id}")))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) -> zbus::fdo::Result<()> {
        if self.clicked(id, &event_id) {
            Ok(())
        } else {
            Err(zbus::fdo::Error::InvalidArgs(format!("no menu item {id}")))
        }
    }

    /// Returns the ids that were not found.
    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        events.into_iter().filter(|(id, event_id, _, _)| !self.clicked(*id, event_id)).map(|(id, ..)| id).collect()
    }

    /// The layout is always current: nothing to refresh.
    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    /// (ids needing an update, ids not found).
    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::voice::TrayItemId;

    fn menu(items: Vec<TrayItem>) -> (DbusMenu, flume::Receiver<TrayEvent>) {
        let (tx, rx) = flume::unbounded();
        let state = Arc::new(Mutex::new(MenuState { items, revision: 7 }));
        (DbusMenu { state, clicks: tx }, rx)
    }

    #[test]
    fn rows_map_to_dbusmenu_properties() {
        let check = TrayItem::Check { id: TrayItemId::WakeWord, label: "Wake word · hey_ops".into(), checked: true, enabled: true };
        let props = item_props(&check);
        assert_eq!(props["label"], owned("Wake word · hey__ops"), "`_` is not a mnemonic");
        assert_eq!(props["toggle-type"], owned("checkmark"));
        assert_eq!(props["toggle-state"], owned(1i32));
        assert_eq!(item_props(&TrayItem::Separator)["type"], owned("separator"));
    }

    #[test]
    fn clicks_route_enabled_rows_only() {
        let (menu, rx) = menu(vec![
            TrayItem::Check { id: TrayItemId::SpeakReplies, label: "Speak replies".into(), checked: false, enabled: false },
            TrayItem::Separator,
            TrayItem::Action { id: TrayItemId::Quit, label: "Quit".into(), enabled: true },
        ]);
        assert!(menu.clicked(1, "clicked"));
        assert!(rx.try_recv().is_err(), "a disabled row does nothing");
        assert!(menu.clicked(3, "clicked"));
        assert_eq!(rx.try_recv().ok(), Some(TrayEvent::Item(TrayItemId::Quit)));
        assert!(menu.clicked(3, "hovered"));
        assert!(rx.try_recv().is_err(), "only clicks act");
        assert!(!menu.clicked(9, "clicked"), "unknown id");
    }

    #[test]
    fn the_layout_lists_every_row_under_the_root() {
        let (menu, _rx) = menu(vec![TrayItem::Separator, TrayItem::Action { id: TrayItemId::Quit, label: "Quit".into(), enabled: true }]);
        let (revision, (id, props, children)) = menu.get_layout(0, -1, Vec::new()).unwrap();
        assert_eq!((revision, id, children.len()), (7, 0, 2));
        assert_eq!(props["children-display"], owned("submenu"));
        assert!(menu.get_layout(5, -1, Vec::new()).is_err());
    }
}
