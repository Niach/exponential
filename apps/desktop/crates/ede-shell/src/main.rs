//! `exponential-shell`: eDE's shell, drawn from genUI documents on disk (#EDE-10).
//! Its own process next to `exponential-compositor`: a wlr-layer-shell client
//! (gpui's `WindowKind::LayerShell`) painting eui-gpui surfaces, so a shell
//! crash never takes the session down and the compositor never links gpui.
//!
//! Since #EDE-50 the shell is `compositor + uicore`: the pure shell model
//! (`uicore-model`: rail, the taskbar's groups and pins, the start menu,
//! Alt-Tab), what the documents bind (`uicore-data`: the [`Feed`], the
//! desktop entries, the window list as reducer events) and the gpui host
//! (`uicore-gpui`: [`Surfaces`], one layer-shell surface per document, popup
//! documents and hosted Popover / Menu layers as `xdg_popup`s) are crates the
//! IDE will host too. This binary is what is eDE-specific: the ede-protocol
//! connection, the session's notification server and tray host ([`dbus`]),
//! the services socket, the hotkeys, the `ede.*` host functions.
//!
//! The documents come from `exponential-ui` (`$EDE_UI_DIR`, else
//! `~/.config/exponential-desktop/ui`; the supervisor `exponential-ui supervise --
//! exponential-shell` hands the directory over and owns the history): every
//! document with a layer `surface` header is a layer surface, every popup
//! document opens from its anchor component in its parent as an `xdg_popup`
//! of that surface. A change on disk redraws the surface; after each draw the
//! shell prints the document's frame line on stdout, which is how the
//! supervisor knows an edit reached the screen. The data every document may
//! bind: `/time` + `/date` (local), `/windows` (the active workspace's
//! windows over ede-protocol: id, title, app, focused, minimized), `/shell`
//! (the model: rail, taskbar buttons, workspaces, the start menu's categories
//! and apps, chat, Alt-Tab), `/apps` + `/query` (the launcher), `/switcher`
//! (Alt+Tab with thumbnails), `/previews` (a taskbar group's windows with
//! thumbnails), `/osd`, `/notifications`, `/tray`, `/traymenu`,
//! `/services/<domain>`, `/outputs`, `/config`, `/user`. The host functions a
//! document may call are the `ede.*` names its `surface.functions` lists
//! (#EDE-8's Tier C rule, checked again here): `ede.launch {command}`,
//! `ede.focus {window}`, `ede.close {window}`, `ede.minimize {window}`,
//! `ede.lock`, `ede.logout`, `ede.power {action}`, `ede.select {window}`,
//! `ede.notify_close {id}`, `ede.notify_action {id, key}`, `ede.notify_clear`,
//! `ede.tray {service}`, `ede.tray_menu {service, id}`, `ede.open_document
//! {name}`, `ede.close_document {name}`, `ede.service {domain, action,
//! args}`, `ede.output {name, mode?, scale?}`, `ede.config {…}`, and the
//! shell model's: `ede.rail` (toggle), `ede.pin {app}` / `ede.unpin {app}`,
//! `ede.taskbar {app}` (Mint's click), `ede.taskbar_scroll {app, direction}`,
//! `ede.previews {app|null}` (the hover previews), `ede.start_menu {open?,
//! query?, category?}`, `ede.chat` (toggle), `ede.workspace {index}`,
//! `ede.sidebar {section}` (fold a section).
//!
//! `exponential-shell [--windowed] [--popup] [--open <document>] [--exit-after <ms>]`
//!   --windowed    normal windows instead of layer surfaces (macOS, any desktop)
//!   --popup NAME  open a popup document right away (the goldens; `all` = every one)
//!   --open NAME   open an on-demand layer document right away (the goldens)
//!   --exit-after  quit after this many milliseconds (the headless check)
//! `EDE_SHELL_DEMO=1` feeds fixed sample data (apps, windows) instead of the
//! machine's, so a golden never depends on what is installed;
//! `EDE_SHELL_RAIL=1` starts with the sidebar as the icon rail.

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
    surface::{Layer as DocLayer, SurfaceHeader},
    Change, Document, Runtime,
};
use eui_core::{host::MediaOptions, surface::Layer};
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin, InputEvent};
use gpui::App;
use serde_json::{json, Value};
use uicore_data::{apps, clock, Feed};
use uicore_gpui::{LayerShellHost, Surfaces, WindowedHost};
use uicore_model::{layout::shell_layout, reduce, ShellEvent, ShellState, StartMenuApp, TaskbarPress};

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

/// What the hosted-popups hook reports: a document's hosted layers.
type HostedReport = (String, Vec<Layer>);

/// The shell's state, shared by the host plugin and the main loop.
struct Shell {
    options: Options,
    ui: Arc<Mutex<Runtime>>,
    surfaces: Surfaces,
    /// What every document binds.
    feed: Feed,
    /// The shell model (#EDE-50): the taskbar, the start menu, the rail, Alt-Tab.
    model: ShellState,
    ede: Option<ede_protocol::Client>,
    /// The installed applications, scanned at start (the launcher, the start menu).
    apps: Vec<apps::App>,
    menu_apps: Vec<StartMenuApp>,
    /// The compositor's last window list (ede-protocol), to shape `/windows`.
    windows: Vec<ede_protocol::WindowInfo>,
    /// Live thumbnails by window id, refreshed by the capture thread while
    /// the switcher or the previews are open.
    thumbnails: HashMap<String, String>,
    /// The windows the capture thread photographs.
    capture_ids: Arc<Mutex<Vec<String>>>,
    /// The thumbnail thread runs while this is set.
    capturing: Arc<AtomicBool>,
    thumbs_tx: mpsc::Sender<(String, String)>,
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
/// popup document; the `ede.*` host functions go to the compositor, the
/// system and the shell model, each only from a document whose header
/// allows it; the hosted popups of the document reach the main loop.
struct Host {
    shell: SharedShell,
    document: String,
    /// The hosted-popups hook fires inside a view update, while the shell
    /// may be borrowed: it reports over its own channel, never through the shell.
    hosted_tx: mpsc::Sender<HostedReport>,
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

/// Mint's click on the focused window's button: the compositor's minimize
/// (#EDE-48); a `focus` restores it.
fn minimize(ede: &mut ede_protocol::Client, id: ede_protocol::WindowId) -> Result<(), ede_protocol::Error> {
    ede.minimize(id)
}

fn document(shell: &Shell, name: &str) -> Option<Document> {
    shell.ui.lock().unwrap_or_else(|p| p.into_inner()).document(name).cloned()
}

impl HostPlugin for Host {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
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
        // The sidebar's sections fold on their own change event (the
        // component id names the section: pinned, workspaces, sessions).
        if event.name == "sidebar" {
            let section = event.component_id.trim_end_matches("-head").to_string();
            let shell = self.shell.clone();
            cx.defer(move |cx| dispatch(&shell, ShellEvent::SidebarToggle { section }, cx));
        }
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        let (name, args) = (event.name.as_str(), &event.args);
        if !allowed(&self.shell.borrow(), &event.surface_id, name) {
            tracing::warn!(document = %event.surface_id, function = name, "refused: not in the document's surface.functions");
            return;
        }
        tracing::info!(document = %event.surface_id, function = name, %args, "host function");
        let shell = self.shell.clone();
        match name {
            "ede.launch" => {
                if let Some(command) = args.get("command").and_then(Value::as_str) {
                    system::launch(command);
                } else if let Some(app) = args.get("app").and_then(Value::as_str) {
                    launch_app(&self.shell.borrow(), app);
                }
                cx.defer(move |cx| {
                    close(&shell, "menu", cx);
                    close(&shell, "start-menu", cx);
                    dispatch(&shell, ShellEvent::StartMenuClose, cx);
                });
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
            "ede.minimize" => {
                if let Some(id) = window_arg(args) {
                    with_ede(&self.shell, |ede| minimize(ede, id));
                }
            }
            "ede.lock" => {
                with_ede(&self.shell, |ede| ede.lock());
                cx.defer(move |cx| {
                    close(&shell, "menu", cx);
                    close(&shell, "start-menu", cx);
                });
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
                    cx.defer(move |cx| switcher_commit(&shell, Some(id), cx));
                }
            }
            "ede.notify_close" => {
                if let Some(id) = args.get("id").and_then(Value::as_u64) {
                    cx.defer(move |cx| notification_close(&shell, id as u32, 2, cx));
                }
            }
            "ede.notify_action" => {
                let (id, key) = (args.get("id").and_then(Value::as_u64), args.get("key").and_then(Value::as_str).map(String::from));
                if let (Some(id), Some(key)) = (id, key) {
                    if let Some(d) = self.shell.borrow().dbus.as_ref() {
                        d.action_invoked(id as u32, &key);
                    }
                    cx.defer(move |cx| notification_close(&shell, id as u32, 2, cx));
                }
            }
            "ede.notify_clear" => {
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
                    cx.defer(move |cx| tray_press(&shell, &service, cx));
                }
            }
            "ede.open_document" | "ede.close_document" => {
                if let Some(doc) = args.get("name").and_then(Value::as_str).map(String::from) {
                    let open_it = name == "ede.open_document";
                    cx.defer(move |cx| {
                        if open_it {
                            close(&shell, "menu", cx);
                            close(&shell, "start-menu", cx);
                            dispatch(&shell, ShellEvent::StartMenuClose, cx);
                            open_on_demand(&shell, &doc, cx);
                            refresh(&shell, cx);
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
                    cx.defer(move |cx| {
                        outputs_refresh(&shell);
                        refresh(&shell, cx);
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
                cx.defer(move |cx| {
                    shell.borrow_mut().feed.set("/config", settings::config_json());
                    refresh(&shell, cx);
                });
            }
            "ede.tray_menu" => {
                let (service, id) = (args.get("service").and_then(Value::as_str).map(String::from), args.get("id").and_then(Value::as_i64));
                if let (Some(service), Some(id)) = (service, id) {
                    if let Some(d) = self.shell.borrow().dbus.as_ref() {
                        d.tray_menu_event(&service, id as i32);
                    }
                    cx.defer(move |cx| close(&shell, "tray-menu", cx));
                }
            }
            // The shell model (#EDE-50).
            "ede.rail" => cx.defer(move |cx| rail_toggle(&shell, cx)),
            "ede.pin" | "ede.unpin" => {
                if let Some(app) = args.get("app").and_then(Value::as_str).map(String::from) {
                    let pin = name == "ede.pin";
                    cx.defer(move |cx| {
                        dispatch(&shell, if pin { ShellEvent::TaskbarPin { app } } else { ShellEvent::TaskbarUnpin { app } }, cx);
                        pins_persist(&shell);
                    });
                }
            }
            "ede.taskbar" => {
                if let Some(app) = args.get("app").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| taskbar_press(&shell, &app, cx));
                }
            }
            "ede.taskbar_scroll" => {
                let (app, direction) = (args.get("app").and_then(Value::as_str).map(String::from), args.get("direction").and_then(Value::as_str).map(String::from));
                if let (Some(app), Some(direction)) = (app, direction) {
                    let direction = if direction == "up" { uicore_model::ScrollDirection::Up } else { uicore_model::ScrollDirection::Down };
                    if let Some(TaskbarPress::Focus { id }) = uicore_model::taskbar_scroll(&self.shell.borrow().model, &app, direction) {
                        if let Ok(id) = ede_protocol::WindowId::new(&id) {
                            with_ede(&self.shell, |ede| ede.focus(id));
                        }
                    }
                }
            }
            "ede.previews" => {
                // A hover Popover's change: open with the app, closed = that app's previews go.
                let app = args.get("app").and_then(Value::as_str).map(String::from);
                let open = args.get("open").and_then(Value::as_bool).unwrap_or(app.is_some());
                cx.defer(move |cx| {
                    let current = shell.borrow().model.taskbar.previews.clone();
                    if open {
                        previews_set(&shell, app, cx);
                    } else if current == app || app.is_none() {
                        previews_set(&shell, None, cx);
                    }
                });
            }
            "ede.switcher" => {
                let args = args.clone();
                cx.defer(move |cx| match args.get("action").and_then(Value::as_str) {
                    Some("pick") => {
                        let kind: Option<uicore_model::SwitcherKind> = args.get("kind").cloned().and_then(|k| serde_json::from_value(k).ok());
                        if let (Some(entry_kind), Some(id)) = (kind, args.get("id").and_then(Value::as_str)) {
                            dispatch(&shell, ShellEvent::SwitcherPick { entry_kind, id: id.into() }, cx);
                        }
                    }
                    Some("connect") => launch_app(&shell.borrow(), "at.exponential"),
                    // Local desktops and Share as a team come with #EDE-54 / #EDE-57.
                    Some(other) => tracing::info!(action = other, "the switcher action comes with Local desktops (#EDE-54)"),
                    None => {}
                });
            }
            "ede.taskbar_menu" => {
                let (app, value) = (args.get("app").and_then(Value::as_str).map(String::from), args.get("value").and_then(Value::as_str).map(String::from));
                if let (Some(app), Some(value)) = (app, value) {
                    cx.defer(move |cx| match value.as_str() {
                        "pin" => {
                            dispatch(&shell, ShellEvent::TaskbarPin { app }, cx);
                            pins_persist(&shell);
                        }
                        "unpin" => {
                            dispatch(&shell, ShellEvent::TaskbarUnpin { app }, cx);
                            pins_persist(&shell);
                        }
                        "close" => {
                            let ids: Vec<ede_protocol::WindowId> = shell.borrow().model.windows.iter().filter(|w| w.app == app).filter_map(|w| ede_protocol::WindowId::new(&w.id).ok()).collect();
                            for id in ids {
                                with_ede(&shell, |ede| ede.close_window(id));
                            }
                        }
                        other => tracing::warn!(value = other, "unknown taskbar menu entry"),
                    });
                }
            }
            "ede.start_menu" => {
                let args = args.clone();
                cx.defer(move |cx| {
                    if args.get("toggle").and_then(Value::as_bool) == Some(true) {
                        start_menu_toggle(&shell, cx);
                        return;
                    }
                    if let Some(open) = args.get("open").and_then(Value::as_bool) {
                        dispatch(&shell, if open { ShellEvent::StartMenuOpen } else { ShellEvent::StartMenuClose }, cx);
                    }
                    if let Some(query) = args.get("query").and_then(Value::as_str) {
                        dispatch(&shell, ShellEvent::StartMenuQuery { query: query.into() }, cx);
                    }
                    if let Some(category) = args.get("category").and_then(Value::as_str) {
                        dispatch(&shell, ShellEvent::StartMenuCategory { category: category.into() }, cx);
                    }
                });
            }
            "ede.chat" => cx.defer(move |cx| chat_toggle(&shell, cx)),
            "ede.workspace" => {
                if let Some(index) = args.get("index").and_then(Value::as_u64) {
                    with_ede(&self.shell, |ede| ede.switch_workspace(index as usize));
                }
            }
            "ede.sidebar" => {
                if let Some(section) = args.get("section").and_then(Value::as_str).map(String::from) {
                    cx.defer(move |cx| dispatch(&shell, ShellEvent::SidebarToggle { section }, cx));
                }
            }
            other => tracing::warn!(function = other, "unknown host function"),
        }
    }

    /// The launcher's search box (`/query` filters `/apps`) and the start
    /// menu's (`/shell/startMenu/query`).
    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        let query = event.value.as_str().unwrap_or_default().to_string();
        let shell = self.shell.clone();
        match event.path.as_deref() {
            Some("/query") => cx.defer(move |cx| {
                {
                    let mut s = shell.borrow_mut();
                    let filtered = apps::filter(&s.apps, &query);
                    s.feed.set("/query", json!(query));
                    s.feed.set("/apps", Value::Array(filtered));
                }
                refresh(&shell, cx);
            }),
            Some("/shell/startMenu/query") => cx.defer(move |cx| dispatch(&shell, ShellEvent::StartMenuQuery { query }, cx)),
            _ => {}
        }
    }

    /// VAPP-281: the document's hosted Popover / Menu layers become popup
    /// windows (the main loop opens and closes them).
    fn on_hosted_popups(&self, layers: &[Layer]) {
        let _ = self.hosted_tx.send((self.document.clone(), layers.to_vec()));
    }

    /// App icons are files in the icon themes; thumbnails are `data:` URLs.
    fn media_options(&self) -> MediaOptions {
        MediaOptions { schemes: Some(vec!["https".into(), "http".into(), "data".into(), "file".into()]), ..Default::default() }
    }
}

/// Launches the desktop entry `app` (its id, with or without `.desktop`).
fn launch_app(shell: &Shell, app: &str) {
    let wanted = app.trim_end_matches(".desktop");
    match shell.apps.iter().find(|a| a.id.trim_end_matches(".desktop") == wanted) {
        Some(a) => system::launch(&a.exec),
        None => tracing::warn!(app, "no desktop entry to launch"),
    }
}

/// The installed entry for an app id (a window's app_id, a pin), if any.
fn app_entry<'a>(shell: &'a Shell, app: &str) -> Option<&'a apps::App> {
    let wanted = app.trim_end_matches(".desktop");
    shell.apps.iter().find(|a| a.id.trim_end_matches(".desktop").eq_ignore_ascii_case(wanted)).or_else(|| shell.apps.iter().find(|a| a.id.trim_end_matches(".desktop").rsplit('.').next().is_some_and(|tail| tail.eq_ignore_ascii_case(wanted))))
}

fn app_icon(shell: &Shell, app: &str) -> String {
    app_entry(shell, app).and_then(|a| a.icon.as_ref()).map(|p| format!("file://{}", p.display())).unwrap_or_default()
}

fn app_name(shell: &Shell, app: &str) -> String {
    app_entry(shell, app).map(|a| a.name.clone()).unwrap_or_else(|| app.to_string())
}

/// Mint's category glyphs for the start menu's column.
fn category_icon(id: &str) -> &'static str {
    match id {
        "all" => "nav-apps",
        "favourites" => "ui-pin",
        "exponential" => "brand-exponential",
        "accessories" => "ui-properties",
        "administration" => "nav-settings",
        "education" => "ui-help",
        "games" => "ui-run",
        "graphics" => "ui-edit",
        "internet" => "ui-link",
        "office" => "ui-file",
        "programming" => "nav-terminal",
        "science" => "ui-info",
        "sound-video" => "media-volume-up",
        _ => "nav-apps",
    }
}

/// What the documents need on top of the reducer's `/shell`: names and
/// icons for apps, the sidebar's lists, the start menu's favourites.
fn decorate_shell(shell: &Shell, mut value: Value) -> Value {
    if let Some(buttons) = value["taskbar"]["buttons"].as_array_mut() {
        for b in buttons {
            let app = b["app"].as_str().unwrap_or_default().to_string();
            b["name"] = json!(app_name(shell, &app));
            b["icon"] = json!(app_icon(shell, &app));
        }
    }
    let pinned: Vec<Value> = shell
        .model
        .taskbar
        .pinned
        .iter()
        .map(|app| json!({"app": app, "name": app_name(shell, app), "icon": app_icon(shell, app), "focused": shell.model.windows.iter().any(|w| &w.app == app && w.focused)}))
        .collect();
    value["sidebar"]["pinned"] = Value::Array(pinned);
    let workspaces: Vec<Value> = uicore_model::workspace_badges(&shell.model)
        .into_iter()
        .map(|b| json!({"index": b.index, "label": format!("Workspace {}", b.index + 1), "active": b.active, "count": b.count, "countText": if b.count == 0 { String::new() } else { b.count.to_string() }}))
        .collect();
    value["sidebar"]["workspaces"] = Value::Array(workspaces);
    value["sessions"] = json!([]);
    if let Some(entries) = value["switcher"]["entries"].as_array_mut() {
        for e in entries {
            let current = e["kind"] == shell.model.switcher.current.kind.clone().into_json() && e["id"] == json!(shell.model.switcher.current.id);
            e["current"] = json!(current);
        }
    }
    let current_category = shell.model.start_menu.category.clone();
    if let Some(categories) = value["startMenu"]["categories"].as_array_mut() {
        for c in categories {
            let id = c["id"].as_str().unwrap_or_default().to_string();
            c["icon"] = json!(category_icon(&id));
            c["selected"] = json!(id == current_category);
        }
    }
    if let Some(apps) = value["startMenu"]["apps"].as_array_mut() {
        for a in apps {
            let id = a["id"].as_str().unwrap_or_default().to_string();
            a["icon"] = json!(app_icon(shell, &id));
        }
    }
    let favourites: Vec<Value> = {
        let mut state = shell.model.clone();
        state.start_menu.query.clear();
        state.start_menu.category = "favourites".into();
        uicore_model::start_menu_apps(&state, &shell.menu_apps, &shell.model.taskbar.pinned).into_iter().map(|e| json!({"id": e.id, "name": e.name, "icon": app_icon(shell, &e.id)})).collect()
    };
    value["startMenu"]["favourites"] = Value::Array(favourites);
    value
}

trait IntoJson {
    fn into_json(self) -> Value;
}
impl IntoJson for uicore_model::SwitcherKind {
    fn into_json(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// `/applets`: what the taskbar's applet buttons show, from the services.
fn applets_json(services: &Value) -> Value {
    let network = &services["network"];
    let network_icon = if network["available"].as_bool() != Some(true) {
        "ui-offline"
    } else if network["wiredConnected"].as_bool() == Some(true) {
        "ede-wired"
    } else if network["wifiEnabled"].as_bool() == Some(false) {
        "ui-offline"
    } else {
        "ede-wifi"
    };
    let sound = &services["sound"];
    let volume = sound["volume"].as_u64().unwrap_or(0);
    let sound_icon = if sound["muted"].as_bool() == Some(true) {
        "media-volume-mute"
    } else if volume == 0 {
        "media-volume-off"
    } else if volume < 50 {
        "media-volume-down"
    } else {
        "media-volume-up"
    };
    let bluetooth = &services["bluetooth"];
    let battery = &services["power"]["battery"];
    let charging = battery["state"].as_str() == Some("charging") || battery["state"].as_str() == Some("full");
    json!({
        "network": {"icon": network_icon},
        "sound": {"icon": sound_icon},
        "bluetooth": {"show": bluetooth["available"].as_bool() == Some(true) && bluetooth["powered"].as_bool() == Some(true)},
        "battery": {"show": battery["present"].as_bool() == Some(true), "icon": if charging { "ede-battery-charging" } else { "ede-battery" }},
    })
}

/// The popup documents anchored at `component` of `parent`.
fn popups_of(shell: &Shell, parent: &str, component: &str) -> Vec<String> {
    let ui = shell.ui.lock().unwrap_or_else(|p| p.into_inner());
    ui.documents()
        .filter(|d| matches!(&d.surface, SurfaceHeader::Popup { popup, .. } if popup.parent == parent && popup.anchor == component))
        .map(|d| d.name.clone())
        .collect()
}

/// The first output's logical size (the goldens' headless output, the
/// session's primary), 1280×720 without a compositor.
fn output_size(shell: &Shell) -> (u32, u32) {
    shell
        .feed
        .get("/outputs")
        .and_then(Value::as_array)
        .and_then(|o| o.first())
        .and_then(|o| Some((o["width"].as_u64()? as u32, o["height"].as_u64()? as u32)))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((1280, 720))
}

/// The sidebar's header for the model's `rail` (the width and exclusive
/// zone of `shell-layout.json`), and a stretched axis (anchored to both
/// of its edges) sized to the output, since the window takes the size it
/// asks for; any other document's as written.
fn header_for(shell: &Shell, doc: &Document) -> SurfaceHeader {
    let mut header = doc.surface.clone();
    if let SurfaceHeader::Layer { size, exclusive_zone, anchor, .. } = &mut header {
        let (out_w, out_h) = output_size(shell);
        if anchor.contains(&ede_ui::surface::Edge::Left) && anchor.contains(&ede_ui::surface::Edge::Right) {
            size.width = Some(out_w.saturating_sub(if doc.name == "taskbar" { if shell.model.rail { shell_layout().rail_width } else { shell_layout().sidebar_width } } else { 0 }).max(1));
        }
        if anchor.contains(&ede_ui::surface::Edge::Top) && anchor.contains(&ede_ui::surface::Edge::Bottom) {
            size.height = Some(out_h.max(1));
        }
        if doc.name == "sidebar" {
            let width = if shell.model.rail { shell_layout().rail_width } else { shell_layout().sidebar_width };
            size.width = Some(width);
            if *exclusive_zone > 0 {
                *exclusive_zone = width as i32;
            }
        }
    }
    header
}

/// A layer document's root takes the surface's size in px (`100%` of a
/// layer window is whatever it was opened at).
fn size_root(doc: &mut Document, header: &SurfaceHeader) {
    let SurfaceHeader::Layer { size, .. } = header else { return };
    if let ede_ui::Body::Nested(root) = &mut doc.body {
        let style = root.style.get_or_insert_with(Default::default);
        if let Some(w) = size.width {
            style.insert("width".into(), json!(w));
        }
        if let Some(h) = size.height {
            style.insert("height".into(), json!(h));
        }
    }
}

/// The order layer documents open in: the ground first, then the sidebar
/// (its exclusive zone takes the left edge), then the taskbar (the rest of
/// the bottom edge), then the others by name. smithay arranges layer
/// surfaces in the order they were mapped.
fn layer_rank(doc: &Document) -> (u8, String) {
    let rank = match (&doc.surface, doc.name.as_str()) {
        (SurfaceHeader::Layer { layer: DocLayer::Background, .. }, _) => 0,
        (_, "sidebar") => 1,
        (_, "taskbar") => 2,
        (_, "panel") => 2,
        _ => 3,
    };
    (rank, doc.name.clone())
}

fn open_layer(shell: &SharedShell, doc: &Document, cx: &mut App) {
    let (header, feed) = {
        let s = shell.borrow();
        (header_for(&s, doc), s.feed.clone())
    };
    let mut doc = doc.clone();
    size_root(&mut doc, &header);
    doc.surface = header;
    shell.borrow_mut().surfaces.open_layer(&doc, &feed, cx);
}

fn close(shell: &SharedShell, name: &str, cx: &mut App) {
    shell.borrow_mut().surfaces.close(name, cx);
}

fn toggle_popup(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().surfaces.is_open(name) {
        close(shell, name, cx);
        return;
    }
    let Some(doc) = document(&shell.borrow(), name) else { return };
    let feed = shell.borrow().feed.clone();
    shell.borrow_mut().surfaces.open_popup(&doc, &feed, cx);
}

/// A change on disk: redraw an open surface, open a new layer surface,
/// close a removed one. Popups re-open from their anchor.
fn apply_change(shell: &SharedShell, change: Change, cx: &mut App) {
    match change {
        Change::Applied { name, .. } => {
            let Some(doc) = document(&shell.borrow(), &name) else { return };
            let open = shell.borrow().surfaces.is_open(&name);
            match (open, &doc.surface) {
                (true, _) => {
                    let (header, feed) = {
                        let s = shell.borrow();
                        (header_for(&s, &doc), s.feed.clone())
                    };
                    let mut doc = doc.clone();
                    size_root(&mut doc, &header);
                    shell.borrow_mut().surfaces.redraw(&doc, &feed, cx);
                    tracing::info!(document = %name, "redrawn");
                }
                // An on-demand surface (the switcher, the OSD) waits for its hotkey.
                (false, SurfaceHeader::Layer { on_demand: false, .. }) => open_layer(shell, &doc, cx),
                (false, SurfaceHeader::Layer { on_demand: true, .. }) | (false, SurfaceHeader::Popup { .. }) => {}
            }
        }
        Change::Removed { name, .. } => close(shell, &name, cx),
        Change::Rejected { name, issues, .. } => tracing::warn!(document = %name, ?issues, "refused; the last good version stays"),
        Change::ThemeApplied { .. } | Change::ThemeRejected { .. } => {}
    }
}

/// Pushes the feed into every open view.
fn refresh(shell: &SharedShell, cx: &mut App) {
    let feed = shell.borrow().feed.clone();
    shell.borrow().surfaces.feed_all(&feed, cx);
}

/// `/shell` from the model, then every view.
fn model_changed(shell: &SharedShell, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        let favourites: Vec<String> = s.model.taskbar.pinned.clone();
        let value = uicore_data::shell_feed(&s.model, &s.menu_apps, &favourites);
        let value = decorate_shell(&s, value);
        s.feed.set("/shell", value);
    }
    refresh(shell, cx);
}

/// One event through the reducer, then `/shell` into every view.
fn dispatch(shell: &SharedShell, event: ShellEvent, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        s.model = reduce(&s.model, &event);
    }
    model_changed(shell, cx);
}

/// `/windows` and the model from the compositor's list.
fn windows_changed(shell: &SharedShell, list: Vec<ede_protocol::WindowInfo>, workspaces: Vec<ede_protocol::WorkspaceInfo>, cx: &mut App) {
    {
        let mut s = shell.borrow_mut();
        for e in uicore_data::workspace_events(&s.model, &workspaces) {
            s.model = reduce(&s.model, &e);
        }
        for e in uicore_data::window_events(&s.model, &list) {
            s.model = reduce(&s.model, &e);
        }
        let active = s.model.workspaces.active;
        let feed = uicore_data::windows_feed(&list, active);
        s.feed.set("/windows", Value::Array(feed));
        s.windows = list;
    }
    previews_refresh(shell);
    model_changed(shell, cx);
}

/// The sidebar as the rail or the full column: the sidebar re-opens at its
/// new width, and the taskbar after it (the arrangement follows the
/// mapping order).
fn rail_toggle(shell: &SharedShell, cx: &mut App) {
    dispatch(shell, ShellEvent::RailToggle, cx);
    let reopen: Vec<Document> = ["sidebar", "taskbar"].iter().filter_map(|n| document(&shell.borrow(), n)).filter(|d| shell.borrow().surfaces.is_open(&d.name)).collect();
    for d in &reopen {
        close(shell, &d.name, cx);
    }
    for d in &reopen {
        open_layer(shell, d, cx);
    }
    let rail = shell.borrow().model.rail;
    settings::persist_shell(json!({"rail": rail}));
}

/// Mint's click on a taskbar button: launch, focus, minimize, or the
/// previews of a group.
fn taskbar_press(shell: &SharedShell, app: &str, cx: &mut App) {
    let press = uicore_model::taskbar_press(&shell.borrow().model, app);
    match press {
        TaskbarPress::Launch => {
            let exec = shell.borrow().apps.iter().find(|a| a.id.trim_end_matches(".desktop") == app).map(|a| a.exec.clone());
            match exec {
                Some(exec) => system::launch(&exec),
                None => tracing::warn!(app, "no desktop entry to launch"),
            }
        }
        TaskbarPress::Focus { id } => {
            if let Ok(id) = ede_protocol::WindowId::new(&id) {
                with_ede(shell, |ede| ede.focus(id));
            }
        }
        TaskbarPress::Minimize { id } => {
            if let Ok(id) = ede_protocol::WindowId::new(&id) {
                with_ede(shell, |ede| minimize(ede, id));
            }
        }
        TaskbarPress::Previews => {
            let open = shell.borrow().model.taskbar.previews.as_deref() == Some(app);
            previews_set(shell, (!open).then(|| app.to_string()), cx);
        }
    }
}

/// The hover previews of a group: `/previews` lists its windows with live
/// thumbnails while `app` is set.
fn previews_set(shell: &SharedShell, app: Option<String>, cx: &mut App) {
    dispatch(shell, ShellEvent::TaskbarPreviews { app }, cx);
    previews_refresh(shell);
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

fn previews_refresh(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let value = match s.model.taskbar.previews.clone() {
        None => json!({"app": null, "windows": []}),
        Some(app) => {
            let windows: Vec<Value> = s
                .model
                .windows
                .iter()
                .filter(|w| w.app == app && w.workspace == s.model.workspaces.active)
                .map(|w| json!({"id": w.id, "title": w.title, "focused": w.focused, "minimized": w.minimized, "thumbnail": s.thumbnails.get(&w.id).cloned().unwrap_or_default()}))
                .collect();
            json!({"app": app, "windows": windows})
        }
    };
    s.feed.set("/previews", value);
}

/// The chat surface (the Assistant's composer) opens or closes with the model.
fn chat_toggle(shell: &SharedShell, cx: &mut App) {
    dispatch(shell, ShellEvent::ChatToggle, cx);
    let open = shell.borrow().model.chat.open;
    if open {
        open_on_demand(shell, "chat", cx);
        refresh(shell, cx);
    } else {
        close(shell, "chat", cx);
    }
}

/// The start menu: the popup document `start-menu` (or the older `menu`),
/// with a fresh search every time.
fn start_menu_toggle(shell: &SharedShell, cx: &mut App) {
    let name = if document(&shell.borrow(), "start-menu").is_some() { "start-menu" } else { "menu" };
    let was_open = shell.borrow().surfaces.is_open(name);
    if was_open {
        close(shell, name, cx);
        dispatch(shell, ShellEvent::StartMenuClose, cx);
        return;
    }
    {
        let mut s = shell.borrow_mut();
        let filtered = apps::filter(&s.apps, "");
        s.feed.set("/query", json!(""));
        s.feed.set("/apps", Value::Array(filtered));
    }
    dispatch(shell, ShellEvent::StartMenuOpen, cx);
    toggle_popup(shell, name, cx);
    refresh(shell, cx);
    shell.borrow().surfaces.focus_component(name, "search", cx);
}

/// Opens an on-demand layer document by name, if it is not open.
fn open_on_demand(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().surfaces.is_open(name) {
        return;
    }
    let doc = document(&shell.borrow(), name);
    match doc {
        Some(doc) if matches!(doc.surface, SurfaceHeader::Layer { .. }) => open_layer(shell, &doc, cx),
        _ => tracing::warn!(document = name, "no such layer document"),
    }
}

/// A key the compositor kept for the shell (#EDE-12).
fn hotkey(shell: &SharedShell, name: &str, cx: &mut App) {
    tracing::info!(name, "hotkey");
    match name {
        "switcher.next" | "switcher.prev" => switcher_step(shell, name == "switcher.prev", cx),
        "switcher.commit" => switcher_commit(shell, None, cx),
        "switcher.cancel" => switcher_close(shell, cx),
        "menu.toggle" => start_menu_toggle(shell, cx),
        "agent.toggle" => chat_toggle(shell, cx),
        "expo.toggle" => overlay_toggle(shell, "expo", cx),
        "scale.toggle" => overlay_toggle(shell, "scale", cx),
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
            let names: Vec<String> = {
                let s = shell.borrow();
                let ui = s.ui.lock().unwrap_or_else(|p| p.into_inner());
                s.surfaces.open_names().filter(|n| ui.document(n).is_some_and(|d| matches!(d.surface, SurfaceHeader::Popup { .. }))).map(String::from).collect()
            };
            for n in &names {
                close(shell, n, cx);
            }
            shell.borrow_mut().surfaces.dismiss_hosted(cx);
            let (previews, menu_open) = {
                let s = shell.borrow();
                (s.model.taskbar.previews.is_some(), s.model.start_menu.open)
            };
            if !names.is_empty() || previews || menu_open {
                dispatch(shell, ShellEvent::StartMenuClose, cx);
                previews_set(shell, None, cx);
            }
        }
        other => tracing::info!(name = other, "hotkey without a handler"),
    }
}

/// Expo / Scale (#EDE-49): an on-demand overlay document with every
/// window's thumbnail, toggled by its hotkey.
fn overlay_toggle(shell: &SharedShell, name: &str, cx: &mut App) {
    if shell.borrow().surfaces.is_open(name) {
        close(shell, name, cx);
        thumbnails_retarget(shell);
        return;
    }
    if document(&shell.borrow(), name).is_none() {
        tracing::info!(document = name, "no overlay document");
        return;
    }
    open_on_demand(shell, name, cx);
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

/// Alt+Tab: open the switcher on the first step, move the selection after.
fn switcher_step(shell: &SharedShell, reverse: bool, cx: &mut App) {
    let fresh = !shell.borrow().model.alt_tab.open;
    dispatch(shell, if reverse { ShellEvent::AltTabPrev } else { ShellEvent::AltTabNext }, cx);
    if !shell.borrow().model.alt_tab.open {
        return;
    }
    if fresh {
        open_on_demand(shell, "switcher", cx);
    }
    switcher_refresh(shell);
    thumbnails_retarget(shell);
    refresh(shell, cx);
}

/// `/switcher`: the Alt-Tab order with the selection and the thumbnails.
fn switcher_refresh(shell: &SharedShell) {
    let mut s = shell.borrow_mut();
    let order = uicore_model::alt_tab_order(&s.model);
    let selected = s.model.alt_tab.selected;
    let windows: Vec<Value> = order
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let w = s.model.windows.iter().find(|w| &w.id == id)?;
            Some(json!({"id": id, "title": w.title, "app": w.app, "selected": i == selected, "thumbnail": s.thumbnails.get(id).cloned().unwrap_or_default()}))
        })
        .collect();
    s.feed.set("/switcher", json!({"windows": windows}));
}

/// The windows the capture thread photographs: the switcher's while it is
/// open, the previews' while they show, every window while Expo or Scale
/// is up; nothing otherwise (the thread stops).
fn thumbnails_retarget(shell: &SharedShell) {
    let ids: Vec<String> = {
        let s = shell.borrow();
        if s.surfaces.is_open("expo") || s.surfaces.is_open("scale") {
            s.model.windows.iter().map(|w| w.id.clone()).collect()
        } else if s.model.alt_tab.open {
            uicore_model::alt_tab_order(&s.model)
        } else if let Some(app) = s.model.taskbar.previews.as_deref() {
            s.model.windows.iter().filter(|w| w.app == app).map(|w| w.id.clone()).collect()
        } else {
            Vec::new()
        }
    };
    let (flag, targets, tx) = {
        let s = shell.borrow();
        (s.capturing.clone(), s.capture_ids.clone(), s.thumbs_tx.clone())
    };
    *targets.lock().unwrap_or_else(|p| p.into_inner()) = ids.clone();
    if ids.is_empty() {
        flag.store(false, Ordering::SeqCst);
        return;
    }
    if flag.swap(true, Ordering::SeqCst) {
        return;
    }
    // Thumbnails at about 5 fps on a thread with its own ede-protocol connection.
    let _ = std::thread::Builder::new().name("ede-shell-thumbs".into()).spawn(move || {
        let Ok(mut ede) = ede_protocol::Client::connect_env() else {
            flag.store(false, Ordering::SeqCst);
            return;
        };
        while flag.load(Ordering::SeqCst) {
            let started = Instant::now();
            let ids = targets.lock().unwrap_or_else(|p| p.into_inner()).clone();
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

/// A thumbnail arrived: the documents that show it.
fn thumbnail_arrived(shell: &SharedShell, id: String, url: String) {
    shell.borrow_mut().thumbnails.insert(id, url);
    switcher_refresh(shell);
    previews_refresh(shell);
    let thumbs: Value = {
        let s = shell.borrow();
        json!(s.thumbnails)
    };
    shell.borrow_mut().feed.set("/thumbnails", thumbs);
}

/// Alt released (or a tile clicked): the selected window gets focus.
fn switcher_commit(shell: &SharedShell, pick: Option<ede_protocol::WindowId>, cx: &mut App) {
    let target = pick.or_else(|| {
        let s = shell.borrow();
        if !s.model.alt_tab.open {
            return None;
        }
        uicore_model::alt_tab_order(&s.model).get(s.model.alt_tab.selected).and_then(|id| ede_protocol::WindowId::new(id).ok())
    });
    if let Some(id) = target {
        with_ede(shell, |ede| ede.focus(id));
    }
    dispatch(shell, ShellEvent::AltTabCommit, cx);
    switcher_close(shell, cx);
}

fn switcher_close(shell: &SharedShell, cx: &mut App) {
    dispatch(shell, ShellEvent::AltTabCancel, cx);
    shell.borrow_mut().feed.set("/switcher", json!({"windows": []}));
    close(shell, "switcher", cx);
    thumbnails_retarget(shell);
}

/// The OSD with a level, for 1.5 s.
fn osd_show(shell: &SharedShell, level: Option<system::Level>, cx: &mut App) {
    let Some(level) = level else { return };
    {
        let mut s = shell.borrow_mut();
        s.feed.set("/osd", json!({"icon": level.icon, "level": level.percent.min(100), "label": level.label()}));
        s.osd_until = Some(Instant::now() + Duration::from_millis(1500));
    }
    open_on_demand(shell, "osd", cx);
    refresh(shell, cx);
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
            refresh(shell, cx);
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
    let any_toast = {
        let mut s = shell.borrow_mut();
        let json = s.notifications.to_json();
        s.feed.set("/notifications", json);
        !s.notifications.toasts.is_empty()
    };
    if any_toast {
        open_on_demand(shell, "notifications", cx);
    } else {
        close(shell, "notifications", cx);
    }
    refresh(shell, cx);
}

/// The outputs as the Displays page binds them (#EDE-14).
fn outputs_refresh(shell: &SharedShell) {
    let outputs = with_ede(shell, |ede| ede.outputs()).unwrap_or_default();
    shell.borrow_mut().feed.set("/outputs", Value::Array(outputs.iter().map(settings::output_json).collect()));
}

/// Reads the tray items again (a short blocking round on the bus).
fn tray_refresh(shell: &SharedShell) {
    let items = shell.borrow().dbus.as_ref().map(|d| d.tray_items()).unwrap_or_default();
    let mut s = shell.borrow_mut();
    s.feed.set("/tray", Value::Array(items.iter().map(dbus::TrayItem::to_json).collect()));
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
            shell.borrow_mut().feed.set("/traymenu", json!({"service": service, "title": title, "items": items}));
            if shell.borrow().surfaces.is_open("tray-menu") {
                close(shell, "tray-menu", cx);
            }
            toggle_popup(shell, "tray-menu", cx);
            refresh(shell, cx);
        }
        _ => {
            if let Some(d) = shell.borrow().dbus.as_ref() {
                d.tray_activate(service, 0, 0);
            }
        }
    }
}

/// The taskbar's pins, kept in the shell's own file.
fn pins_persist(shell: &SharedShell) {
    let pinned = shell.borrow().model.taskbar.pinned.clone();
    settings::persist_shell(json!({"pinned": pinned}));
}

/// The compositor's window and workspace lists (one round trip each).
fn lists(calls: &mut ede_protocol::Client) -> Option<(Vec<ede_protocol::WindowInfo>, Vec<ede_protocol::WorkspaceInfo>)> {
    let windows = calls.windows().ok()?;
    let workspaces = calls.workspaces().unwrap_or_default();
    Some((windows, workspaces))
}

/// The sample windows of the goldens (`EDE_SHELL_DEMO=1`): three apps, one grouped.
fn demo_windows() -> Vec<ede_protocol::WindowInfo> {
    let w = |id: &str, app: &str, title: &str, focused: bool| ede_protocol::WindowInfo { id: ede_protocol::WindowId::new(id).unwrap(), title: Some(title.into()), app_id: Some(app.into()), x: 0, y: 0, width: 800, height: 600, focused, fullscreen: false, protected: false, x11: false, workspace: 0, minimized: false };
    vec![w("w1", "firefox", "Firefox", false), w("w2", "terminal", "Terminal", true), w("w3", "nemo", "Files", false), w("w4", "firefox", "Exponential — Firefox", false)]
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
    let demo = std::env::var_os("EDE_SHELL_DEMO").is_some_and(|v| !v.is_empty() && v != "0");

    // Window events and hotkeys from the compositor on their own connection.
    let (windows_tx, windows_rx) = mpsc::channel::<(Vec<ede_protocol::WindowInfo>, Vec<ede_protocol::WorkspaceInfo>)>();
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
            if !demo {
                if let Some(list) = lists(&mut calls) {
                    let _ = windows_tx.send(list);
                }
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
                } else if !demo && (event.kind.starts_with("window.") || event.kind.starts_with("workspace.")) {
                    if let Some(list) = lists(&mut calls) {
                        if windows_tx.send(list).is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .expect("the events thread");
    let installed = if demo { apps::demo() } else { apps::scan() };
    tracing::info!(apps = installed.len(), demo, "desktop entries");
    // Thumbnails from the capture thread while the switcher or the previews are open.
    let (thumbs_tx, thumbs_rx) = mpsc::channel::<(String, String)>();
    // The hosted popups of every surface (VAPP-281).
    let (hosted_tx, hosted_rx) = mpsc::channel::<HostedReport>();
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
    let dbus = if demo { None } else { dbus::Dbus::start(dbus_tx, clock::time) };

    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
        let mut feed = Feed::new();
        feed.set("/time", json!(clock::time()));
        feed.set("/date", json!(clock::date()));
        feed.set("/windows", json!([]));
        feed.set("/apps", Value::Array(apps::filter(&installed, "")));
        feed.set("/query", json!(""));
        feed.set("/switcher", json!({"windows": []}));
        feed.set("/previews", json!({"app": null, "windows": []}));
        feed.set("/thumbnails", json!({}));
        feed.set("/osd", json!({"icon": "media-volume-up", "level": 0, "label": ""}));
        feed.set("/traymenu", json!({"service": "", "title": "", "items": []}));
        feed.set("/config", settings::config_json());
        feed.set("/user", settings::user_json());
        if demo {
            feed.set("/osd", json!({"icon": "media-volume-down", "level": 45, "label": "45%"}));
            feed.set("/notifications", json!({"toasts": [{"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [{"key": "open", "label": "Open"}, {"key": "archive", "label": "Archive"}], "urgency": 1, "at": "11:58"}], "list": [{"id": 1, "app": "Thunderbird", "summary": "Release notes for 1.4", "body": "Danny: the M1 demo runs on Friday, bring the VM.", "icon": "", "actions": [], "urgency": 1, "at": "11:58"}, {"id": 2, "app": "Software", "summary": "Updates installed", "body": "12 packages were updated.", "icon": "", "actions": [], "urgency": 0, "at": "11:40"}], "count": 2}));
            feed.set("/tray", json!([{"service": "demo/network", "title": "Network", "status": "Active", "icon": ""}, {"service": "demo/sound", "title": "Sound", "status": "Active", "icon": ""}]));
            feed.set(
                "/traymenu",
                json!({"service": "demo/network", "title": "Network", "items": [
                    {"id": 1, "label": "Connect to Wi-Fi", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0},
                    {"id": 2, "label": "Airplane mode", "enabled": true, "separator": false, "checked": false, "checkable": true, "depth": 0},
                    {"id": 3, "label": "", "enabled": true, "separator": true, "checked": false, "checkable": false, "depth": 0},
                    {"id": 4, "label": "Settings…", "enabled": true, "separator": false, "checked": false, "checkable": false, "depth": 0}
                ]}),
            );
            feed.set("/services", settings::demo_services());
            feed.set("/outputs", Value::Array(settings::demo_outputs()));
        } else {
            feed.set("/notifications", json!({"toasts": [], "list": [], "count": 0}));
            feed.set("/tray", json!([]));
            feed.set("/services", json!({}));
            feed.set("/outputs", json!([]));
        }
        // The model: the pins and the rail from the shell's own file (the demo: fixed pins).
        let mut model = ShellState::default();
        let saved = if demo { json!({"pinned": ["firefox", "nemo", "org.gnome.Calculator"]}) } else { settings::shell_json() };
        for app in saved.get("pinned").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            model = reduce(&model, &ShellEvent::TaskbarPin { app: app.into() });
        }
        let rail = std::env::var_os("EDE_SHELL_RAIL").is_some_and(|v| !v.is_empty() && v != "0") || saved.get("rail").and_then(Value::as_bool).unwrap_or(false);
        if rail {
            model = reduce(&model, &ShellEvent::RailToggle);
        }
        let menu_apps: Vec<StartMenuApp> = installed.iter().map(apps::App::start_menu_app).collect();
        let favourites = model.taskbar.pinned.clone();
        feed.set("/shell", uicore_data::shell_feed(&model, &menu_apps, &favourites));
        let applets = applets_json(feed.get("/services").unwrap_or(&Value::Null));
        feed.set("/applets", applets);

        let shell_cell: Rc<RefCell<Option<SharedShell>>> = Rc::new(RefCell::new(None));
        let plugin_for: uicore_gpui::PluginFor = {
            let cell = shell_cell.clone();
            let hosted_tx = hosted_tx.clone();
            Rc::new(move |document: &str| {
                let shell = cell.borrow().clone().expect("the shell exists before a surface opens");
                Rc::new(Host { shell, document: document.to_string(), hosted_tx: hosted_tx.clone() }) as Rc<dyn HostPlugin>
            })
        };
        let on_frame: uicore_gpui::OnFrame = Rc::new(|name, digest| println!("{}", FrameLine::line_for_digest(name, digest)));
        let host: Box<dyn uicore_gpui::ShellHost> = if options.windowed { Box::new(WindowedHost) } else { Box::new(LayerShellHost::default()) };
        let surfaces = Surfaces::new(host, theme, plugin_for, on_frame, "at.exponential.ede.shell");
        let shell: SharedShell = Rc::new(RefCell::new(Shell {
            options,
            ui: ui.clone(),
            surfaces,
            feed,
            model,
            ede: ede_protocol::Client::connect_env().ok(),
            apps: installed,
            menu_apps,
            windows: Vec::new(),
            thumbnails: HashMap::new(),
            capture_ids: Arc::new(Mutex::new(Vec::new())),
            capturing: Arc::new(AtomicBool::new(false)),
            thumbs_tx,
            osd_until: None,
            notifications: Notifications::default(),
            dbus,
            tray: Vec::new(),
            _watcher: watcher,
        }));
        *shell_cell.borrow_mut() = Some(shell.clone());
        model_changed(&shell, cx);
        if !demo {
            // The outputs first: a stretched surface takes the output's size.
            outputs_refresh(&shell);
        }
        if demo {
            // The sample windows, before any document opens (the goldens).
            windows_changed(&shell, demo_windows(), vec![ede_protocol::WorkspaceInfo { index: 0, active: true, windows: vec![] }, ede_protocol::WorkspaceInfo { index: 1, active: false, windows: vec![] }], cx);
        } else {
            // The tray as it stands at start.
            tray_refresh(&shell);
        }

        // Every layer document on screen, the ground, the sidebar and the
        // taskbar first; popups wait for their anchor.
        let mut docs: Vec<Document> = ui.lock().unwrap_or_else(|p| p.into_inner()).documents().cloned().collect();
        docs.sort_by_key(layer_rank);
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
        if shell.borrow().surfaces.open_names().next().is_none() {
            tracing::warn!(dir = %dir.display(), "no layer document to draw (is the factory directory installed? exponential-ui --factory apps/ede/ui check)");
        }

        // The main loop's inputs: file changes, window lists, hotkeys,
        // thumbnails, the bus, the services, the hosted popups, the clock.
        let s = shell.clone();
        cx.spawn(async move |cx| {
            let mut minute = clock::time();
            loop {
                let mut dirty = false;
                while let Ok(changes) = changes_rx.try_recv() {
                    for change in changes {
                        cx.update(|cx| apply_change(&s, change, cx));
                    }
                }
                while let Ok((windows, workspaces)) = windows_rx.try_recv() {
                    cx.update(|cx| windows_changed(&s, windows, workspaces, cx));
                }
                while let Ok(name) = hotkeys_rx.try_recv() {
                    cx.update(|cx| hotkey(&s, &name, cx));
                }
                while let Ok((id, url)) = thumbs_rx.try_recv() {
                    thumbnail_arrived(&s, id, url);
                    dirty = true;
                }
                while let Ok(event) = dbus_rx.try_recv() {
                    cx.update(|cx| dbus_event(&s, event, cx));
                }
                while let Ok((domain, snapshot)) = services_rx.try_recv() {
                    let mut sh = s.borrow_mut();
                    let mut services = sh.feed.get("/services").cloned().unwrap_or_else(|| json!({}));
                    services[domain.as_str()] = settings::decorate(&domain, snapshot);
                    let applets = applets_json(&services);
                    sh.feed.set("/services", services);
                    sh.feed.set("/applets", applets);
                    dirty = true;
                }
                while let Ok(()) = outputs_rx.try_recv() {
                    outputs_refresh(&s);
                    dirty = true;
                }
                while let Ok((document, layers)) = hosted_rx.try_recv() {
                    cx.update(|cx| s.borrow_mut().surfaces.sync_hosted(&document, &layers, cx));
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
                let now = clock::time();
                if now != minute {
                    minute = now.clone();
                    let mut sh = s.borrow_mut();
                    sh.feed.set("/time", json!(now));
                    sh.feed.set("/date", json!(clock::date()));
                    dirty = true;
                }
                if dirty {
                    cx.update(|cx| refresh(&s, cx));
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();

        let open_now = shell.borrow().options.open.clone();
        for name in open_now {
            // The goldens: the switcher opens as Alt+Tab would (the sample windows, the second selected).
            if name == "switcher" {
                switcher_step(&shell, false, cx);
            } else {
                open_on_demand(&shell, &name, cx);
            }
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
                        if name == "start-menu" || name == "menu" {
                            start_menu_toggle(&s, cx);
                        } else {
                            toggle_popup(&s, &name, cx);
                        }
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
