//! `exponential-shell`: eDE's shell, drawn from genUI documents on disk (#EDE-10).
//! Its own process next to `exponential-compositor`: a wlr-layer-shell client
//! (gpui's `WindowKind::LayerShell`) painting eui-gpui surfaces, so a shell
//! crash never takes the session down and the compositor never links gpui.
//!
//! The documents come from `exponential-ui` (`$EDE_UI_DIR`, else `~/.config/exponential-desktop/ui`;
//! the supervisor `exponential-ui supervise -- exponential-shell` hands the directory over
//! and owns the history): every document with a layer `surface` header is a
//! layer surface, every popup document opens from its anchor component in
//! its parent as an `xdg_popup` of that surface. A change on disk redraws
//! the surface; after each draw the shell prints the document's frame line
//! on stdout, which is how the supervisor knows an edit reached the screen.
//! The data every document may bind: `/time` (local, every minute),
//! `/windows` (the open windows over ede-protocol), `/apps` + `/query` (the
//! launcher, [`apps`]), `/switcher` (the Alt+Tab switcher with thumbnails,
//! [`thumbs`]), `/osd` (the media keys' level). The host functions a
//! document may call are the `ede.*` names its `surface.functions` lists
//! (#EDE-8's Tier C rule, checked again here): `ede.launch {command}`,
//! `ede.focus {window}`, `ede.close {window}`, `ede.lock`, `ede.logout`,
//! `ede.power {action}`, `ede.select {window}`, `ede.notify_close {id}`,
//! `ede.notify_action {id, key}`, `ede.notify_clear`, `ede.tray {service}`,
//! `ede.tray_menu {service, id}`. The keys the compositor keeps (Alt+Tab,
//! Super, the media keys) arrive as `hotkey` events (#EDE-12) and open the
//! on-demand surfaces (`switcher`, `osd`) or the menu. The shell is also
//! the session's notification server and tray host ([`dbus`]):
//! `/notifications` (toasts, the history, the count) and `/tray` (the
//! items), `/traymenu` (an item's menu).
//!
//! `exponential-shell [--windowed] [--popup] [--open <document>] [--exit-after <ms>]`
//!   --windowed    normal windows instead of layer surfaces (macOS, any desktop)
//!   --popup NAME  open a popup document right away (the goldens; `all` = every one)
//!   --open NAME   open an on-demand layer document right away (the goldens)
//!   --exit-after  quit after this many milliseconds (the headless check)
//! `EDE_SHELL_DEMO=1` feeds fixed sample data (apps, switcher windows) instead
//! of the machine's, so a golden never depends on what is installed.

mod apps;
mod assets;
mod dbus;
mod settings;
mod system;
mod thumbs;

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ede_ui::{
    supervise::FrameLine,
    surface::{Edge, Keyboard, Layer as DocLayer, Placement, SurfaceHeader},
    Change, Document, Runtime,
};
use eui_core::{host::MediaOptions, surface::{SurfaceCommand, SurfaceSettings}, theme::{Mode, ModeSetting}};
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin, InputEvent};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions}, point, popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions}, prelude::*, px, size, App, Bounds, Entity, Pixels, SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use serde_json::{json, Value};

/// A layer surface stretched on neither axis gets this size.
const DEFAULT_SIZE: (f32, f32) = (1280.0, 40.0);

struct Options {
    windowed: bool,
    popup: Vec<String>,
    open: Vec<String>,
    exit_after: Option<u64>,
}

fn parse_args() -> Options {
    let mut o = Options { windowed: false, popup: Vec::new(), open: Vec::new(), exit_after: None };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--windowed" => o.windowed = true,
            "--popup" => o.popup.extend(args.next()),
            "--open" => o.open.extend(args.next()),
            "--exit-after" => o.exit_after = args.next().and_then(|v| v.parse().ok()),
            "-h" | "--help" => {
                println!("usage: exponential-shell [--windowed] [--popup <document>] [--open <document>] [--exit-after <ms>]");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other:?} (try --help)");
                std::process::exit(2);
            }
        }
    }
    o
}

/// One document on screen.
struct Surface {
    view: Entity<SurfaceView>,
}

impl Render for Surface {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

struct Open {
    handle: WindowHandle<Surface>,
    view: Entity<SurfaceView>,
}

/// What every document may bind: the clock, the open windows, the
/// launcher's apps and query, the switcher, the OSD.
#[derive(Default, Clone)]
struct ShellData {
    time: String,
    windows: Vec<Value>,
    query: String,
    apps: Vec<Value>,
    switcher: Value,
    osd: Value,
    notifications: Value,
    tray: Vec<Value>,
    traymenu: Value,
    /// `/services/<domain>` from exponential-services (#EDE-13).
    services: Value,
    /// `/outputs` (the compositor's, with the options a page picks from),
    /// `/config` (compositor.toml), `/user`: the settings pages (#EDE-14).
    outputs: Vec<Value>,
    config: Value,
    user: Value,
}

/// The notifications heard this session, newest first.
#[derive(Default)]
struct Notifications {
    list: Vec<dbus::Notification>,
    /// Ids still showing as toasts.
    toasts: Vec<u32>,
}

impl Notifications {
    fn to_json(&self) -> Value {
        let toasts: Vec<Value> = self.list.iter().filter(|n| self.toasts.contains(&n.id)).take(3).map(dbus::Notification::to_json).collect();
        json!({"toasts": toasts, "list": self.list.iter().map(dbus::Notification::to_json).collect::<Vec<_>>(), "count": self.list.len()})
    }

    /// Expired toasts leave the stack (they stay in the history); the ids
    /// that left.
    fn expire(&mut self, now: Instant) -> Vec<u32> {
        let gone: Vec<u32> = self.list.iter().filter(|n| self.toasts.contains(&n.id) && n.expires_at.is_some_and(|t| now >= t)).map(|n| n.id).collect();
        self.toasts.retain(|id| !gone.contains(id));
        gone
    }

    fn push(&mut self, n: dbus::Notification) {
        self.list.retain(|x| x.id != n.id);
        self.toasts.retain(|id| *id != n.id);
        self.toasts.insert(0, n.id);
        self.list.insert(0, n);
        self.list.truncate(100);
    }

    /// Removes it everywhere; `true` if it was there.
    fn remove(&mut self, id: u32) -> bool {
        let before = self.list.len();
        self.list.retain(|x| x.id != id);
        self.toasts.retain(|x| *x != id);
        before != self.list.len()
    }
}

/// The Alt+Tab switcher while it is open: the windows most recent first
/// and the selected one.
#[derive(Default)]
struct Switcher {
    windows: Vec<Value>,
    selected: usize,
    /// Thumbnails by window id, refreshed by the capture thread.
    thumbnails: HashMap<String, String>,
}

impl Switcher {
    fn to_json(&self) -> Value {
        let windows: Vec<Value> = self
            .windows
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let id = w["id"].as_str().unwrap_or_default();
                json!({"id": id, "title": w["title"], "selected": i == self.selected, "thumbnail": self.thumbnails.get(id).cloned().unwrap_or_default()})
            })
            .collect();
        json!({ "windows": windows })
    }
}

/// The shell's state, shared by the host plugin and the main loop.
struct Shell {
    options: Options,
    ui: Arc<Mutex<Runtime>>,
    open: HashMap<String, Open>,
    data: ShellData,
    ede: Option<ede_protocol::Client>,
    theme: Arc<eui_core::theme::ResolvedTheme>,
    /// The installed applications, scanned at start (the launcher).
    apps: Vec<apps::App>,
    switcher: Option<Switcher>,
    /// The thumbnail thread runs while this is set.
    capturing: Arc<AtomicBool>,
    /// The OSD closes when this passes.
    osd_until: Option<Instant>,
    notifications: Notifications,
    dbus: Option<dbus::Dbus>,
    tray: Vec<dbus::TrayItem>,
    /// Watches while the shell lives (dropping it ends the hot reload).
    _watcher: ede_ui::Watcher,
}

type SharedShell = Rc<RefCell<Shell>>;

/// Actions from the documents: a press on a popup's anchor opens that
/// popup document; the `ede.*` host functions go to the compositor and the
/// system, each only from a document whose header allows it.
struct Host {
    shell: SharedShell,
}

/// Does `document`'s header list `function`? (Tier C: a document grants no
/// privileges; `exponential-ui` refuses such a document already, this is the
/// shell's own check.)
fn allowed(shell: &Shell, document: &str, function: &str) -> bool {
    let ui = shell.ui.lock().unwrap_or_else(|p| p.into_inner());
    ui.document(document).is_some_and(|d| d.surface.functions().iter().any(|f| f == function))
}

fn with_ede<R>(shell: &SharedShell, f: impl FnOnce(&mut ede_protocol::Client) -> Result<R, ede_protocol::Error>) -> Option<R> {
    let mut s = shell.borrow_mut();
    let Some(ede) = s.ede.as_mut() else {
        tracing::warn!("no ede-protocol connection");
        return None;
    };
    match f(ede) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!("ede-protocol: {e}");
            None
        }
    }
}

fn window_arg(args: &Value) -> Option<ede_protocol::WindowId> {
    args.get("window").and_then(Value::as_str).and_then(|s| ede_protocol::WindowId::new(s).ok())
}

impl HostPlugin for Host {
    fn icon(&self, name: &str) -> Option<SharedString> {
        assets::icon_path(name)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        tracing::info!(surface = %event.surface_id, component = %event.component_id, name = %event.name, context = %event.context, "action");
        // The view is mid-update while it dispatches; a popup reads its
        // painted bounds, so that waits for the update to end.
        let popups = popups_of(&self.shell.borrow(), &event.surface_id, &event.component_id);
        if !popups.is_empty() {
            let shell = self.shell.clone();
            cx.defer(move |cx| {
                for name in &popups {
                    toggle_popup(&shell, name, cx);
                }
            });
        }
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        let (name, args) = (event.name.as_str(), &event.args);
        if !allowed(&self.shell.borrow(), &event.surface_id, name) {
            tracing::warn!(document = %event.surface_id, function = name, "refused: not in the document's surface.functions");
            return;
        }
        tracing::info!(document = %event.surface_id, function = name, %args, "host function");
        match name {
            "ede.launch" => {
                if let Some(command) = args.get("command").and_then(Value::as_str) {
                    system::launch(command);
                }
                let shell = self.shell.clone();
                cx.defer(move |cx| close(&shell, "menu", cx));
            }
            "ede.focus" => {
                if let Some(id) = window_arg(args) {
                    with_ede(&self.shell, |ede| ede.focus(id));
                }
            }
            "ede.close" => {
                if let Some(id) = window_arg(args) {
                    with_ede(&self.shell, |ede| ede.close_window(id));
                }
            }
            "ede.lock" => {
                with_ede(&self.shell, |ede| ede.lock());
                let shell = self.shell.clone();
                cx.defer(move |cx| close(&shell, "menu", cx));
            }
            "ede.logout" => {
                with_ede(&self.shell, |ede| ede.logout());
            }
            "ede.power" => {
                if let Some(action) = args.get("action").and_then(Value::as_str) {
                    system::power(action);
                }
            }
            "ede.select" => {
                if let Some(id) = window_arg(args) {
                    let shell = self.shell.clone();
                    cx.defer(move |cx| switcher_commit(&shell, Some(id), cx));
                }
            }
            "ede.notify_close" => {
                if let Some(id) = args.get("id").and_then(Value::as_u64) {
                    let shell = self.shell.clone();
                    cx.defer(move |cx| notification_close(&shell, id as u32, 2, cx));
                }
            }
            "ede.notify_action" => {
                let (id, key) = (args.get("id").and_then(Value::as_u64), args.get("key").and_then(Value::as_str).map(String::from));
                if let (Some(id), Some(key)) = (id, key) {
                    if let Some(d) = self.shell.borrow().dbus.as_ref() {
                        d.action_invoked(id as u32, &key);
                    }
                    let shell = self.shell.clone();
                    cx.defer(move |cx| notification_close(&shell, id as u32, 2, cx));
                }
            }
            "ede.notify_clear" => {
                let shell = self.shell.clone();
                cx.defer(move |cx| {
                    let ids: Vec<u32> = shell.borrow().notifications.list.iter().map(|n| n.id).collect();
                    for id in ids {
                        notification_close(&shell, id, 2, cx);
                    }
                    close(&shell, "notification-center", cx);
                });
            }
            "ede.tray" => {
                if let Some(service) = args.get("service").and_then(Value::as_str).map(String::from) {
                    let shell = self.shell.clone();
                    cx.defer(move |cx| tray_press(&shell, &service, cx));
                }
            }
            "ede.open_document" | "ede.close_document" => {
                if let Some(doc) = args.get("name").and_then(Value::as_str).map(String::from) {
                    let shell = self.shell.clone();
                    let open_it = name == "ede.open_document";
                    cx.defer(move |cx| {
                        if open_it {
                            close(&shell, "menu", cx);
                            open_on_demand(&shell, &doc, cx);
                            refresh_data(&shell, cx);
                        } else {
                            close(&shell, &doc, cx);
                        }
                    });
                }
            }
            "ede.service" => {
                let (domain, action) = (args.get("domain").and_then(Value::as_str).map(String::from), args.get("action").and_then(Value::as_str).map(String::from));
                if let (Some(domain), Some(action)) = (domain, action) {
                    let call_args = args.get("args").cloned().unwrap_or_else(|| json!({}));
                    // Off the main loop: a daemon round trip, and polkit may ask.
                    std::thread::spawn(move || match ede_services::socket::Client::connect_default().map_err(|e| e.to_string()).and_then(|mut c| c.call(json!({"op": "call", "domain": domain, "action": action, "args": call_args}))) {
                        Ok(v) => tracing::info!(%domain, %action, result = %v, "service"),
                        Err(e) => tracing::warn!(%domain, %action, "service: {e}"),
                    });
                }
            }
            "ede.output" => {
                if let Some(output) = args.get("name").and_then(Value::as_str).map(String::from) {
                    let mode = args.get("mode").and_then(Value::as_str).map(String::from);
                    let scale = args.get("scale").and_then(|s| s.as_f64().or_else(|| s.as_str().and_then(|t| t.parse().ok())));
                    with_ede(&self.shell, |ede| ede.set_output(&output, mode.as_deref(), scale, None));
                    if let Err(e) = settings::persist_output(&output, mode.as_deref(), scale) {
                        tracing::warn!("compositor.toml: {e}");
                    }
                    let shell = self.shell.clone();
                    cx.defer(move |cx| {
                        outputs_refresh(&shell);
                        refresh_data(&shell, cx);
                    });
                }
            }
            "ede.config" => {
                match settings::persist_config(args) {
                    Ok(()) => {
                        with_ede(&self.shell, |ede| ede.call(ede_protocol::Request::ReloadConfig));
                        if args.get("theme").is_some() {
                            // #EDE-19: apply or remove, as the switch says.
                            system::launch("exponential-theme sync");
                        }
                    }
                    Err(e) => tracing::warn!("compositor.toml: {e}"),
                }
                let shell = self.shell.clone();
                cx.defer(move |cx| {
                    shell.borrow_mut().data.config = settings::config_json();
                    refresh_data(&shell, cx);
                });
            }
            "ede.tray_menu" => {
                let (service, id) = (args.get("service").and_then(Value::as_str).map(String::from), args.get("id").and_then(Value::as_i64));
                if let (Some(service), Some(id)) = (service, id) {
                    if let Some(d) = self.shell.borrow().dbus.as_ref() {
                        d.tray_menu_event(&service, id as i32);
                    }
                    let shell = self.shell.clone();
                    cx.defer(move |cx| close(&shell, "tray-menu", cx));
                }
            }
            other => tracing::warn!(function = other, "unknown host function"),
        }
    }

    /// The launcher's search box: `/query` filters `/apps`.
    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        if event.path.as_deref() != Some("/query") {
            return;
        }
        let query = event.value.as_str().unwrap_or_default().to_string();
        let shell = self.shell.clone();
        cx.defer(move |cx| {
            {
                let mut s = shell.borrow_mut();
                s.data.query = query.clone();
                s.data.apps = apps::filter(&s.apps, &query);
            }
            refresh_data(&shell, cx);
        });
    }

    /// App icons are files in the icon themes; thumbnails are `data:` URLs.
    fn media_options(&self) -> MediaOptions {
        MediaOptions { schemes: Some(vec!["https".into(), "http".into(), "data".into(), "file".into()]), ..Default::default() }
    }
}

/// The popup documents anchored at `component` of `parent`.
fn popups_of(shell: &Shell, parent: &str, component: &str) -> Vec<String> {
    let ui = shell.ui.lock().unwrap_or_else(|p| p.into_inner());
    ui.documents()
        .filter(|d| matches!(&d.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == parent && popup.anchor == component))
        .map(|d| d.name.clone())
        .collect()
}

fn layer_options(header: &SurfaceHeader) -> (WindowKind, Bounds<Pixels>) {
    let SurfaceHeader::Layer { layer, anchor, size: s, exclusive_zone, margin, keyboard, .. } = header else {
        unreachable!("a layer header");
    };
    let mut anchors = Anchor::empty();
    for edge in anchor {
        anchors |= match edge {
            Edge::Top => Anchor::TOP,
            Edge::Bottom => Anchor::BOTTOM,
            Edge::Left => Anchor::LEFT,
            Edge::Right => Anchor::RIGHT,
        };
    }
    let width = s.width.map(|w| w as f32).unwrap_or(DEFAULT_SIZE.0);
    let height = s.height.map(|h| h as f32).unwrap_or(DEFAULT_SIZE.1);
    let kind = WindowKind::LayerShell(LayerShellOptions {
        namespace: "exponential-shell".into(),
        layer: match layer {
            DocLayer::Background => Layer::Background,
            DocLayer::Bottom => Layer::Bottom,
            DocLayer::Top => Layer::Top,
            DocLayer::Overlay => Layer::Overlay,
        },
        anchor: anchors,
        exclusive_zone: (*exclusive_zone != 0).then(|| px(*exclusive_zone as f32)),
        exclusive_edge: None,
        margin: Some((px(margin.top as f32), px(margin.right as f32), px(margin.bottom as f32), px(margin.left as f32))),
        keyboard_interactivity: match keyboard {
            Keyboard::None => KeyboardInteractivity::None,
            Keyboard::OnDemand => KeyboardInteractivity::OnDemand,
            Keyboard::Exclusive => KeyboardInteractivity::Exclusive,
        },
    });
    (kind, Bounds { origin: point(px(0.0), px(0.0)), size: size(px(width), px(height)) })
}

fn placement(p: Placement) -> (PopupAnchor, PopupGravity) {
    match p {
        Placement::BottomStart => (PopupAnchor::BottomLeft, PopupGravity::BottomRight),
        Placement::BottomEnd => (PopupAnchor::BottomRight, PopupGravity::BottomLeft),
        Placement::TopStart => (PopupAnchor::TopLeft, PopupGravity::TopRight),
        Placement::TopEnd => (PopupAnchor::TopRight, PopupGravity::TopLeft),
        Placement::LeftStart => (PopupAnchor::TopLeft, PopupGravity::BottomLeft),
        Placement::RightStart => (PopupAnchor::TopRight, PopupGravity::BottomRight),
    }
}

/// Opens `doc` in a window of `kind`; the view records bounds so popups
/// can anchor at a component.
fn open_window(shell: &SharedShell, doc: &Document, kind: WindowKind, bounds: Bounds<Pixels>, cx: &mut App) -> Option<Open> {
    let (theme, data) = {
        let s = shell.borrow();
        (s.theme.clone(), s.data.clone())
    };
    let host: Rc<dyn HostPlugin> = Rc::new(Host { shell: shell.clone() });
    let name = doc.name.clone();
    let digest = doc.digest.clone();
    let doc = doc.clone();
    let view_out: Rc<RefCell<Option<Entity<SurfaceView>>>> = Rc::new(RefCell::new(None));
    let out = view_out.clone();
    let opened = cx.open_window(
        WindowOptions {
            kind,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("at.exponential.ede.shell".into()),
            focus: false,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                let mut v = SurfaceView::new(
                    SurfaceViewOptions { surface_id: name.clone(), theme: Some(theme), mode: Mode::Dark, host, settings: Some(SurfaceSettings { locale: "en-US".into(), mode: ModeSetting::Dark, ..SurfaceSettings::default() }), ..Default::default() },
                    window,
                    cx,
                );
                v.record_bounds(true);
                load(&mut v, &doc, &data, cx);
                v
            });
            *out.borrow_mut() = Some(view.clone());
            let n = name.clone();
            window.on_next_frame(move |_, _| println!("{}", FrameLine::line_for_digest(&n, &digest)));
            cx.new(|_| Surface { view })
        },
    );
    match opened {
        Ok(handle) => view_out.borrow().clone().map(|view| Open { handle, view }),
        Err(e) => {
            tracing::warn!("window: {e}");
            None
        }
    }
}

/// The document's tree and data into a view, then the shell's data on top.
fn load(view: &mut SurfaceView, doc: &Document, data: &ShellData, cx: &mut gpui::Context<SurfaceView>) {
    if let Some(obj) = doc.data.as_ref().and_then(Value::as_object) {
        for (k, val) in obj {
            if let Err(e) = view.set_data(&format!("/{k}"), Some(val.clone()), cx) {
                tracing::warn!(document = %doc.name, "data /{k}: {e}");
            }
        }
    }
    match &doc.body {
        ede_ui::Body::Nested(tree) => {
            view.set_nested(tree.clone(), cx);
        }
        ede_ui::Body::Flat(components) => {
            view.set_components(components.clone(), cx);
        }
    }
    feed(view, data, cx);
}

fn feed(view: &mut SurfaceView, data: &ShellData, cx: &mut gpui::Context<SurfaceView>) {
    let _ = view.set_data("/time", Some(json!(data.time)), cx);
    let _ = view.set_data("/windows", Some(Value::Array(data.windows.clone())), cx);
    let _ = view.set_data("/apps", Some(Value::Array(data.apps.clone())), cx);
    let _ = view.set_data("/query", Some(json!(data.query)), cx);
    let _ = view.set_data("/switcher", Some(data.switcher.clone()), cx);
    let _ = view.set_data("/osd", Some(data.osd.clone()), cx);
    let _ = view.set_data("/notifications", Some(data.notifications.clone()), cx);
    let _ = view.set_data("/tray", Some(Value::Array(data.tray.clone())), cx);
    let _ = view.set_data("/traymenu", Some(data.traymenu.clone()), cx);
    let _ = view.set_data("/services", Some(data.services.clone()), cx);
    let _ = view.set_data("/outputs", Some(Value::Array(data.outputs.clone())), cx);
    let _ = view.set_data("/config", Some(data.config.clone()), cx);
    let _ = view.set_data("/user", Some(data.user.clone()), cx);
}

fn open_layer(shell: &SharedShell, doc: &Document, cx: &mut App) {
    let windowed = shell.borrow().options.windowed;
    let (kind, bounds) = layer_options(&doc.surface);
    let kind = if windowed { WindowKind::Normal } else { kind };
    if let Some(open) = open_window(shell, doc, kind, bounds, cx) {
        tracing::info!(document = %doc.name, "surface opened");
        shell.borrow_mut().open.insert(doc.name.clone(), open);
    }
}

fn close(shell: &SharedShell, name: &str, cx: &mut App) {
    if let Some(open) = shell.borrow_mut().open.remove(name) {
        let _ = open.handle.update(cx, |_, window, _| window.remove_window());
    }
}

fn toggle_popup(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().open.contains_key(name) {
        close(shell, name, cx);
        return;
    }
    let doc = shell.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).document(name).cloned();
    let Some(doc) = doc else { return };
    let SurfaceHeader::Popup { popup, .. } = &doc.surface else { return };
    let windowed = shell.borrow().options.windowed;
    let parent = shell.borrow().open.get(&popup.parent).map(|o| (o.handle, o.view.clone()));
    let Some((parent_handle, parent_view)) = parent else {
        tracing::warn!(document = name, parent = %popup.parent, "popup: parent surface not open");
        return;
    };
    let anchor_rect = parent_view.read(cx).painted_bounds(&popup.anchor).unwrap_or(Bounds { origin: point(px(0.0), px(0.0)), size: size(px(120.0), px(DEFAULT_SIZE.1)) });
    let (anchor, gravity) = placement(popup.placement);
    let bounds = Bounds { origin: point(anchor_rect.origin.x, anchor_rect.origin.y + anchor_rect.size.height + px(4.0)), size: size(px(popup.size.width as f32), px(popup.size.height as f32)) };
    let kind = if windowed {
        WindowKind::PopUp
    } else {
        WindowKind::AnchoredPopup(PopupOptions { parent: parent_handle.into(), anchor_rect, anchor, gravity, constraint_adjustment: PopupConstraintAdjustment::SLIDE_X | PopupConstraintAdjustment::FLIP_Y, offset: point(px(0.0), px(4.0)), grab: false })
    };
    if let Some(open) = open_window(shell, &doc, kind, bounds, cx) {
        tracing::info!(document = name, "popup opened");
        shell.borrow_mut().open.insert(name.to_string(), open);
    }
}

/// A change on disk: redraw an open surface, open a new layer surface,
/// close a removed one. Popups re-open from their anchor.
fn apply_change(shell: &SharedShell, change: Change, cx: &mut App) {
    match change {
        Change::Applied { name, .. } => {
            let doc = shell.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).document(&name).cloned();
            let Some(doc) = doc else { return };
            let existing = shell.borrow().open.get(&name).map(|o| (o.handle, o.view.clone()));
            match (existing, &doc.surface) {
                (Some((handle, view)), _) => {
                    let data = shell.borrow().data.clone();
                    let digest = doc.digest.clone();
                    let n = name.clone();
                    let _ = handle.update(cx, |_, window, cx| {
                        view.update(cx, |v, cx| load(v, &doc, &data, cx));
                        window.on_next_frame(move |_, _| println!("{}", FrameLine::line_for_digest(&n, &digest)));
                    });
                    tracing::info!(document = %name, "redrawn");
                }
                // An on-demand surface (the switcher, the OSD) waits for its hotkey.
                (None, SurfaceHeader::Layer { on_demand: false, .. }) => open_layer(shell, &doc, cx),
                (None, SurfaceHeader::Layer { on_demand: true, .. }) | (None, SurfaceHeader::Popup { .. }) => {}
            }
        }
        Change::Removed { name, .. } => close(shell, &name, cx),
        Change::Rejected { name, issues, .. } => tracing::warn!(document = %name, ?issues, "refused; the last good version stays"),
        Change::ThemeApplied { .. } | Change::ThemeRejected { .. } => {}
    }
}

/// Pushes the shell's data into every open view.
fn refresh_data(shell: &SharedShell, cx: &mut App) {
    let (views, data): (Vec<(WindowHandle<Surface>, Entity<SurfaceView>)>, ShellData) = {
        let s = shell.borrow();
        (s.open.values().map(|o| (o.handle, o.view.clone())).collect(), s.data.clone())
    };
    for (handle, view) in views {
        let data = data.clone();
        let _ = handle.update(cx, |_, _, cx| view.update(cx, |v, cx| feed(v, &data, cx)));
    }
}

/// A key the compositor kept for the shell (#EDE-12).
fn hotkey(shell: &SharedShell, name: &str, thumbs: &mpsc::Sender<(String, String)>, cx: &mut App) {
    tracing::info!(name, "hotkey");
    match name {
        "switcher.next" | "switcher.prev" => switcher_step(shell, name == "switcher.prev", thumbs, cx),
        "switcher.commit" => switcher_commit(shell, None, cx),
        "switcher.cancel" => switcher_close(shell, cx),
        "menu.toggle" => menu_toggle(shell, cx),
        "volume.up" | "volume.down" => osd_show(shell, system::volume_step(name == "volume.up"), cx),
        "volume.mute" => osd_show(shell, system::volume_toggle_mute(), cx),
        "mic.mute" => osd_show(shell, system::mic_toggle_mute(), cx),
        "brightness.up" | "brightness.down" => osd_show(shell, system::brightness_step(name == "brightness.up"), cx),
        "media.play" | "media.next" | "media.prev" => system::media(name),
        "terminal" => match system::terminal_command() {
            Some(t) => system::launch(t),
            None => tracing::warn!("no terminal emulator installed"),
        },
        "screenshot" => screenshot(shell),
        "popups.dismiss" => {
            let open: Vec<String> = {
                let s = shell.borrow();
                let ui = s.ui.lock().unwrap_or_else(|p| p.into_inner());
                s.open.keys().filter(|n| ui.document(n).is_some_and(|d| matches!(d.surface, SurfaceHeader::Popup { .. }))).cloned().collect()
            };
            for name in open {
                close(shell, &name, cx);
            }
        }
        "agent.toggle" => tracing::info!("the agent panel comes with M2"),
        other => tracing::info!(name = other, "hotkey without a handler"),
    }
}

fn menu_toggle(shell: &SharedShell, cx: &mut App) {
    let was_open = shell.borrow().open.contains_key("menu");
    if !was_open {
        // A fresh search every time.
        let mut s = shell.borrow_mut();
        s.data.query.clear();
        s.data.apps = apps::filter(&s.apps, "");
    }
    toggle_popup(shell, "menu", cx);
    if !was_open {
        refresh_data(shell, cx);
        focus_component(shell, "menu", "search", cx);
    }
}

/// Keyboard focus into a component of an open document (the search box).
fn focus_component(shell: &SharedShell, document: &str, component: &str, cx: &mut App) {
    let open = shell.borrow().open.get(document).map(|o| (o.handle, o.view.clone()));
    let Some((handle, view)) = open else { return };
    let id = component.to_string();
    let _ = handle.update(cx, |_, _, cx| {
        view.update(cx, |v, cx| {
            v.command(&SurfaceCommand::Focus { id }, cx);
            cx.notify();
        })
    });
}

/// Opens an on-demand layer document by name, if it is not open.
fn open_on_demand(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().open.contains_key(name) {
        return;
    }
    let doc = shell.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).document(name).cloned();
    match doc {
        Some(doc) if matches!(doc.surface, SurfaceHeader::Layer { .. }) => open_layer(shell, &doc, cx),
        _ => tracing::warn!(document = name, "no such layer document"),
    }
}

/// Alt+Tab: open the switcher on the first step, move the selection after.
fn switcher_step(shell: &SharedShell, reverse: bool, thumbs: &mpsc::Sender<(String, String)>, cx: &mut App) {
    let fresh = shell.borrow().switcher.is_none();
    if fresh {
        // The compositor lists bottom to top: most recent first here.
        let mut windows = shell.borrow().data.windows.clone();
        windows.reverse();
        let n = windows.len();
        let selected = if n > 1 { if reverse { n - 1 } else { 1 } } else { 0 };
        shell.borrow_mut().switcher = Some(Switcher { windows, selected, thumbnails: HashMap::new() });
        start_thumbnails(shell, thumbs);
        open_on_demand(shell, "switcher", cx);
    } else {
        let mut s = shell.borrow_mut();
        let sw = s.switcher.as_mut().unwrap();
        let n = sw.windows.len();
        if n > 0 {
            sw.selected = if reverse { (sw.selected + n - 1) % n } else { (sw.selected + 1) % n };
        }
    }
    let json = shell.borrow().switcher.as_ref().map(Switcher::to_json).unwrap_or_default();
    shell.borrow_mut().data.switcher = json;
    refresh_data(shell, cx);
}

/// Thumbnails at about 5 fps while the switcher is open, on a thread
/// with its own ede-protocol connection.
fn start_thumbnails(shell: &SharedShell, thumbs: &mpsc::Sender<(String, String)>) {
    let flag = shell.borrow().capturing.clone();
    flag.store(true, Ordering::SeqCst);
    let ids: Vec<String> = shell.borrow().switcher.as_ref().map(|sw| sw.windows.iter().filter_map(|w| w["id"].as_str().map(String::from)).collect()).unwrap_or_default();
    let tx = thumbs.clone();
    let _ = std::thread::Builder::new().name("ede-shell-thumbs".into()).spawn(move || {
        let Ok(mut ede) = ede_protocol::Client::connect_env() else { return };
        while flag.load(Ordering::SeqCst) {
            let started = Instant::now();
            for id in &ids {
                if !flag.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(window) = ede_protocol::WindowId::new(id) else { continue };
                // A protected window refuses the capture: no picture, as intended.
                if let Ok(capture) = ede.capture(ede_protocol::CaptureTarget::Window { window }) {
                    if let Some(url) = thumbs::data_url(&capture) {
                        if tx.send((id.clone(), url)).is_err() {
                            return;
                        }
                    }
                }
            }
            if let Some(rest) = Duration::from_millis(200).checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    });
}

/// Alt released (or a tile clicked): the selected window gets focus.
fn switcher_commit(shell: &SharedShell, pick: Option<ede_protocol::WindowId>, cx: &mut App) {
    let target = pick.or_else(|| {
        let s = shell.borrow();
        let sw = s.switcher.as_ref()?;
        sw.windows.get(sw.selected).and_then(|w| w["id"].as_str()).and_then(|id| ede_protocol::WindowId::new(id).ok())
    });
    if let Some(id) = target {
        with_ede(shell, |ede| ede.focus(id));
    }
    switcher_close(shell, cx);
}

fn switcher_close(shell: &SharedShell, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        s.capturing.store(false, Ordering::SeqCst);
        s.switcher = None;
        s.data.switcher = json!({"windows": []});
    }
    close(shell, "switcher", cx);
}

/// The OSD with a level, for 1.5 s.
fn osd_show(shell: &SharedShell, level: Option<system::Level>, cx: &mut App) {
    let Some(level) = level else { return };
    {
        let mut s = shell.borrow_mut();
        s.data.osd = json!({"icon": level.icon, "level": level.percent.min(100), "label": level.label()});
        s.osd_until = Some(Instant::now() + Duration::from_millis(1500));
    }
    open_on_demand(shell, "osd", cx);
    refresh_data(shell, cx);
}

/// Print: the first output to `~/Pictures/Screenshots`.
fn screenshot(shell: &SharedShell) {
    let path = system::screenshot_path();
    let saved = with_ede(shell, |ede| {
        let outputs = ede.outputs()?;
        let Some(first) = outputs.first() else { return Ok(false) };
        let capture = ede.capture(ede_protocol::CaptureTarget::Output { name: first.name.clone() })?;
        Ok(thumbs::write_png(&path, &capture).is_ok())
    });
    if saved == Some(true) {
        tracing::info!(path = %path.display(), "screenshot saved");
    }
}

/// Something on the session bus (#EDE-12).
fn dbus_event(shell: &SharedShell, event: dbus::Event, cx: &mut App) {
    match event {
        dbus::Event::Notified(n) => {
            tracing::info!(id = n.id, app = %n.app, summary = %n.summary, "notification");
            shell.borrow_mut().notifications.push(n);
            notifications_changed(shell, cx);
        }
        dbus::Event::CloseRequested(id) => notification_close(shell, id, 3, cx),
        dbus::Event::TrayChanged => {
            tray_refresh(shell);
            refresh_data(shell, cx);
        }
    }
}

/// A notification leaves (reason: 1 expired, 2 dismissed, 3 the app closed it).
fn notification_close(shell: &SharedShell, id: u32, reason: u32, cx: &mut App) {
    let was_there = shell.borrow_mut().notifications.remove(id);
    if was_there {
        if let Some(d) = shell.borrow().dbus.as_ref() {
            d.notification_closed(id, reason);
        }
    }
    notifications_changed(shell, cx);
}

/// The toast surface shows while toasts exist; every document sees the data.
fn notifications_changed(shell: &SharedShell, cx: &mut App) {
    let (json, any_toast) = {
        let s = shell.borrow();
        (s.notifications.to_json(), !s.notifications.toasts.is_empty())
    };
    shell.borrow_mut().data.notifications = json;
    if any_toast {
        open_on_demand(shell, "notifications", cx);
    } else {
        close(shell, "notifications", cx);
    }
    refresh_data(shell, cx);
}

/// The outputs as the Displays page binds them (#EDE-14).
fn outputs_refresh(shell: &SharedShell) {
    let outputs = with_ede(shell, |ede| ede.outputs()).unwrap_or_default();
    shell.borrow_mut().data.outputs = outputs.iter().map(settings::output_json).collect();
}

/// Reads the tray items again (a short blocking round on the bus).
fn tray_refresh(shell: &SharedShell) {
    let items = shell.borrow().dbus.as_ref().map(|d| d.tray_items()).unwrap_or_default();
    let mut s = shell.borrow_mut();
    s.data.tray = items.iter().map(dbus::TrayItem::to_json).collect();
    s.tray = items;
}

/// A tray item pressed: its menu as a popup, else `Activate`.
fn tray_press(shell: &SharedShell, service: &str, cx: &mut App) {
    let (has_menu, title) = shell.borrow().tray.iter().find(|i| i.service == service).map(|i| (i.has_menu, i.title.clone())).unwrap_or((false, String::new()));
    let menu = if has_menu { shell.borrow().dbus.as_ref().and_then(|d| d.tray_menu(service)) } else { None };
    match menu {
        Some(entries) if !entries.is_empty() => {
            let items: Vec<Value> = entries
                .iter()
                .map(|e| {
                    let mut v = e.to_json();
                    if e.depth > 0 {
                        v["label"] = json!(format!("    {}", e.label));
                    }
                    v
                })
                .collect();
            shell.borrow_mut().data.traymenu = json!({"service": service, "title": title, "items": items});
            if shell.borrow().open.contains_key("tray-menu") {
                close(shell, "tray-menu", cx);
            }
            toggle_popup(shell, "tray-menu", cx);
            refresh_data(shell, cx);
        }
        _ => {
            if let Some(d) = shell.borrow().dbus.as_ref() {
                d.tray_activate(service, 0, 0);
            }
        }
    }
}

/// `HH:MM` local time; `EDE_SHELL_CLOCK` pins it (the goldens, #EDE-11).
fn clock() -> String {
    if let Some(fixed) = std::env::var_os("EDE_SHELL_CLOCK").filter(|v| !v.is_empty()) {
        return fixed.to_string_lossy().into_owned();
    }
    // SAFETY: time() with a null pointer only returns; localtime_r writes
    // into the tm we own.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        libc::localtime_r(&now, &mut tm);
    }
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// The open windows for the taskbar, over ede-protocol: the active
/// workspace's (#EDE-18), protected ones without their title.
fn windows_json(ede: &mut ede_protocol::Client) -> Option<Vec<Value>> {
    let windows = ede.windows().ok()?;
    let active = ede.workspaces().ok().and_then(|ws| ws.iter().find(|w| w.active).map(|w| w.index)).unwrap_or(0);
    Some(
        windows
            .into_iter()
            .filter(|w| !w.protected && w.workspace == active)
            .map(|w| json!({"id": w.id, "title": w.title.unwrap_or_else(|| w.app_id.unwrap_or_else(|| "Window".into())), "focused": w.focused, "workspace": w.workspace}))
            .collect(),
    )
}

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let options = parse_args();
    let dir = ede_ui::default_dir();
    let (ui, initial) = match Runtime::open_readonly(&dir) {
        Ok(opened) => opened,
        Err(e) => {
            eprintln!("exponential-shell: {}: {e}", dir.display());
            std::process::exit(2);
        }
    };
    let ui = Arc::new(Mutex::new(ui));
    let (changes_tx, changes_rx) = mpsc::channel::<Vec<Change>>();
    let watcher = match ede_ui::watch(ui.clone(), move |changes| {
        let _ = changes_tx.send(changes);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("exponential-shell: watch {}: {e}", dir.display());
            std::process::exit(2);
        }
    };

    // Window events and hotkeys from the compositor on their own connection.
    let (windows_tx, windows_rx) = mpsc::channel::<Vec<Value>>();
    let (hotkeys_tx, hotkeys_rx) = mpsc::channel::<String>();
    let (outputs_tx, outputs_rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("ede-shell-events".into())
        .spawn(move || {
            let Ok(mut events) = ede_protocol::Client::connect_env() else {
                tracing::info!("no ede-protocol socket: the taskbar stays empty");
                return;
            };
            let Ok(mut calls) = ede_protocol::Client::connect_env() else { return };
            if let Some(list) = windows_json(&mut calls) {
                let _ = windows_tx.send(list);
            }
            if events.subscribe().is_err() {
                return;
            }
            while let Ok(event) = events.next_event() {
                if event.kind == ede_protocol::events::HOTKEY {
                    if let Some(name) = event.data.get("name").and_then(Value::as_str) {
                        if hotkeys_tx.send(name.to_string()).is_err() {
                            return;
                        }
                    }
                } else if event.kind.starts_with("output.") {
                    if outputs_tx.send(()).is_err() {
                        return;
                    }
                } else if event.kind.starts_with("window.") || event.kind.starts_with("workspace.") {
                    if let Some(list) = windows_json(&mut calls) {
                        if windows_tx.send(list).is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .expect("the events thread");
    let demo = std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0");
    let installed = if demo { apps::demo() } else { apps::scan() };
    tracing::info!(apps = installed.len(), demo, "desktop entries");
    // Thumbnails from the capture thread while the switcher is open.
    let (thumbs_tx, thumbs_rx) = mpsc::channel::<(String, String)>();
    // The services data source: every domain's snapshot, then its changes.
    let (services_tx, services_rx) = mpsc::channel::<(String, Value)>();
    if !demo {
        std::thread::Builder::new()
            .name("ede-shell-services".into())
            .spawn(move || {
                // The daemon may start after the shell: try for a while.
                let mut client = None;
                for _ in 0..30 {
                    match ede_services::socket::Client::connect_default() {
                        Ok(c) => {
                            client = Some(c);
                            break;
                        }
                        Err(_) => std::thread::sleep(Duration::from_secs(1)),
                    }
                }
                let Some(mut client) = client else {
                    tracing::info!("no exponential-services socket: /services stays empty");
                    return;
                };
                let domains: Vec<String> = client.call(json!({"op": "list"})).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
                for d in &domains {
                    if let Ok(snapshot) = client.get(d) {
                        let _ = services_tx.send((d.clone(), snapshot));
                    }
                }
                if client.subscribe().is_err() {
                    return;
                }
                while let Ok(event) = client.next_event() {
                    if let (Some(d), Some(s)) = (event["domain"].as_str(), event.get("snapshot")) {
                        if services_tx.send((d.to_string(), s.clone())).is_err() {
                            return;
                        }
                    }
                }
            })
            .expect("the services thread");
    }
    // Notifications and tray changes from the session bus.
    let (dbus_tx, dbus_rx) = mpsc::channel::<dbus::Event>();
    let dbus = if demo { None } else { dbus::Dbus::start(dbus_tx, clock) };

    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
        let (switcher, osd) = if demo {
            (
                json!({"windows": [{"id": "w1", "title": "Firefox", "selected": false, "thumbnail": ""}, {"id": "w2", "title": "Terminal", "selected": true, "thumbnail": ""}, {"id": "w3", "title": "Files", "selected": false, "thumbnail": ""}]}),
                json!({"icon": "media-volume-down", "level": 45, "label": "45%"}),
            )
        } else {
            (json!({"windows": []}), json!({"icon": "media-volume-up", "level": 0, "label": ""}))
        };
        let (notifications, tray) = if demo {
            (
                json!({"toasts": [{"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [{"key": "open", "label": "Open"}, {"key": "archive", "label": "Archive"}], "urgency": 1, "at": "11:58"}], "list": [{"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [], "urgency": 1, "at": "11:58"}, {"id": 2, "app": "Software", "summary": "Updates installed", "body": "12 packages were updated.", "icon": "", "actions": [], "urgency": 0, "at": "11:40"}], "count": 2}),
                vec![json!({"service": "demo/network", "title": "Network", "status": "Active", "icon": ""}), json!({"service": "demo/sound", "title": "Sound", "status": "Active", "icon": ""})],
            )
        } else {
            (json!({"toasts": [], "list": [], "count": 0}), Vec::new())
        };
        let traymenu = if demo {
            json!({"service": "demo/network", "title": "Network", "items": [
                {"id": 1, "label": "Connect to Wi-Fi", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0},
                {"id": 2, "label": "Airplane mode", "enabled": true, "separator": false, "checked": false, "checkable": true, "depth": 0},
                {"id": 3, "label": "", "enabled": true, "separator": true, "checked": false, "checkable": false, "depth": 0},
                {"id": 4, "label": "Settings…", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0}
            ]})
        } else {
            json!({"service": "", "title": "", "items": []})
        };
        let (services, outputs) = if demo { (settings::demo_services(), settings::demo_outputs()) } else { (json!({}), Vec::new()) };
        let data = ShellData { time: clock(), apps: apps::filter(&installed, ""), switcher, osd, notifications, tray, traymenu, services, outputs, config: settings::config_json(), user: settings::user_json(), ..Default::default() };
        let shell: SharedShell = Rc::new(RefCell::new(Shell { options, ui: ui.clone(), open: HashMap::new(), data, ede: ede_protocol::Client::connect_env().ok(), theme, apps: installed, switcher: None, capturing: Arc::new(AtomicBool::new(false)), osd_until: None, notifications: Notifications::default(), dbus, tray: Vec::new(), _watcher: watcher }));
        if demo {
            // The toasts golden: the sample toast as an open surface.
            let s = shell.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let wanted = s.borrow().options.open.iter().any(|d| d == "notifications");
                if wanted {
                    cx.update(|cx| refresh_data(&s, cx));
                }
            })
            .detach();
        } else {
            // The tray and the outputs as they stand at start.
            tray_refresh(&shell);
            outputs_refresh(&shell);
        }

        // Every layer document on screen; popups wait for their anchor.
        let docs: Vec<Document> = ui.lock().unwrap_or_else(|p| p.into_inner()).documents().cloned().collect();
        for doc in &docs {
            if matches!(doc.surface, SurfaceHeader::Layer { on_demand: false, .. }) {
                open_layer(&shell, doc, cx);
            }
        }
        for change in initial {
            if let Change::Rejected { name, issues, .. } = change {
                tracing::warn!(document = %name, ?issues, "refused at start");
            }
        }
        if shell.borrow().open.is_empty() {
            tracing::warn!(dir = %dir.display(), "no layer document to draw (is the factory directory installed? exponential-ui --factory apps/ede/ui check)");
        }

        // The main loop's inputs: file changes, window lists, the clock.
        let s = shell.clone();
        cx.spawn(async move |cx| {
            let mut minute = clock();
            loop {
                let mut dirty = false;
                while let Ok(changes) = changes_rx.try_recv() {
                    for change in changes {
                        cx.update(|cx| apply_change(&s, change, cx));
                    }
                }
                while let Ok(list) = windows_rx.try_recv() {
                    s.borrow_mut().data.windows = list;
                    dirty = true;
                }
                while let Ok(name) = hotkeys_rx.try_recv() {
                    cx.update(|cx| hotkey(&s, &name, &thumbs_tx, cx));
                }
                while let Ok((id, url)) = thumbs_rx.try_recv() {
                    let mut sh = s.borrow_mut();
                    if let Some(sw) = sh.switcher.as_mut() {
                        sw.thumbnails.insert(id, url);
                        sh.data.switcher = sh.switcher.as_ref().unwrap().to_json();
                        dirty = true;
                    }
                }
                while let Ok(event) = dbus_rx.try_recv() {
                    cx.update(|cx| dbus_event(&s, event, cx));
                }
                while let Ok((domain, snapshot)) = services_rx.try_recv() {
                    s.borrow_mut().data.services[domain] = settings::decorate(&domain, snapshot);
                    dirty = true;
                }
                while let Ok(()) = outputs_rx.try_recv() {
                    outputs_refresh(&s);
                    dirty = true;
                }
                let expired = s.borrow_mut().notifications.expire(Instant::now());
                if !expired.is_empty() {
                    for id in &expired {
                        if let Some(d) = s.borrow().dbus.as_ref() {
                            d.notification_closed(*id, 1);
                        }
                    }
                    cx.update(|cx| notifications_changed(&s, cx));
                }
                let osd_over = s.borrow().osd_until.is_some_and(|t| Instant::now() >= t);
                if osd_over {
                    s.borrow_mut().osd_until = None;
                    cx.update(|cx| close(&s, "osd", cx));
                }
                let now = clock();
                if now != minute {
                    minute = now.clone();
                    s.borrow_mut().data.time = now;
                    dirty = true;
                }
                if dirty {
                    cx.update(|cx| refresh_data(&s, cx));
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();

        let open_now = shell.borrow().options.open.clone();
        for name in open_now {
            open_on_demand(&shell, &name, cx);
        }
        let wanted_popups = shell.borrow().options.popup.clone();
        if !wanted_popups.is_empty() {
            let s = shell.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(700)).await;
                let all = wanted_popups.iter().any(|p| p == "all");
                let names: Vec<String> = s.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).documents().filter(|d| matches!(d.surface, SurfaceHeader::Popup { .. }) && (all || wanted_popups.contains(&d.name))).map(|d| d.name.clone()).collect();
                cx.update(|cx| {
                    for name in names {
                        toggle_popup(&s, &name, cx);
                    }
                });
            })
            .detach();
        }
        let exit_after = shell.borrow().options.exit_after;
        if let Some(ms) = exit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}
