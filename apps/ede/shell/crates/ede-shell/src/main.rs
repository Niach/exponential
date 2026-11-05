//! `ede-shell`: eDE's shell, drawn from genUI documents on disk (#EDE-10).
//! Its own process next to `ede-compositor`: a wlr-layer-shell client
//! (gpui's `WindowKind::LayerShell`) painting eui-gpui surfaces, so a shell
//! crash never takes the session down and the compositor never links gpui.
//!
//! The documents come from `ede-ui` (`$EDE_UI_DIR`, else `~/.config/ede/ui`;
//! the supervisor `ede-ui supervise -- ede-shell` hands the directory over
//! and owns the history): every document with a layer `surface` header is a
//! layer surface, every popup document opens from its anchor component in
//! its parent as an `xdg_popup` of that surface. A change on disk redraws
//! the surface; after each draw the shell prints the document's frame line
//! on stdout, which is how the supervisor knows an edit reached the screen.
//! The panel's data: `/time` (local, every minute), `/windows` (the open
//! windows over ede-protocol); its actions: `focus {window}`, `launch
//! {command}`.
//!
//! `ede-shell [--windowed] [--popup] [--exit-after <ms>]`
//!   --windowed    normal windows instead of layer surfaces (macOS, any desktop)
//!   --popup       open every popup document right away (the headless check)
//!   --exit-after  quit after this many milliseconds (the headless check)

mod assets;

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use ede_ui::{
    supervise::FrameLine,
    surface::{Edge, Keyboard, Layer as DocLayer, Placement, SurfaceHeader},
    Change, Document, Runtime,
};
use eui_core::{surface::SurfaceSettings, theme::{Mode, ModeSetting}};
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions}, point, popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions}, prelude::*, px, size, App, Bounds, Entity, Pixels, SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use serde_json::{json, Value};

/// A layer surface stretched on neither axis gets this size.
const DEFAULT_SIZE: (f32, f32) = (1280.0, 40.0);

struct Options {
    windowed: bool,
    popup: bool,
    exit_after: Option<u64>,
}

fn parse_args() -> Options {
    let mut o = Options { windowed: false, popup: false, exit_after: None };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--windowed" => o.windowed = true,
            "--popup" => o.popup = true,
            "--exit-after" => o.exit_after = args.next().and_then(|v| v.parse().ok()),
            "-h" | "--help" => {
                println!("usage: ede-shell [--windowed] [--popup] [--exit-after <ms>]");
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

/// What every document may bind: the clock and the open windows.
#[derive(Default, Clone)]
struct ShellData {
    time: String,
    windows: Vec<Value>,
}

/// The shell's state, shared by the host plugin and the main loop.
struct Shell {
    options: Options,
    ui: Arc<Mutex<Runtime>>,
    open: HashMap<String, Open>,
    data: ShellData,
    ede: Option<ede_protocol::Client>,
    theme: Arc<eui_core::theme::ResolvedTheme>,
    /// Watches while the shell lives (dropping it ends the hot reload).
    _watcher: ede_ui::Watcher,
}

type SharedShell = Rc<RefCell<Shell>>;

/// Actions from the documents: `menu`-style presses open their popup
/// document, `focus` and `launch` go to the compositor and the system.
struct Host {
    shell: SharedShell,
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
        match event.name.as_str() {
            "focus" => {
                if let Some(id) = event.context.get("window").and_then(Value::as_str).and_then(|s| ede_protocol::WindowId::new(s).ok()) {
                    let mut shell = self.shell.borrow_mut();
                    if let Some(ede) = shell.ede.as_mut() {
                        if let Err(e) = ede.call(ede_protocol::Request::FocusWindow { window: id }) {
                            tracing::warn!("focus: {e}");
                        }
                    }
                }
            }
            "launch" => {
                if let Some(command) = event.context.get("command").and_then(Value::as_str) {
                    match std::process::Command::new("sh").arg("-c").arg(command).spawn() {
                        Ok(_) => tracing::info!(command, "launched"),
                        Err(e) => tracing::warn!(command, "launch failed: {e}"),
                    }
                }
            }
            _ => {}
        }
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
        namespace: "ede-shell".into(),
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
                (None, SurfaceHeader::Layer { .. }) => open_layer(shell, &doc, cx),
                (None, SurfaceHeader::Popup { .. }) => {}
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

/// The open windows for the taskbar, over ede-protocol.
fn windows_json(ede: &mut ede_protocol::Client) -> Option<Vec<Value>> {
    let windows = ede.windows().ok()?;
    Some(windows.into_iter().filter(|w| !w.protected).map(|w| json!({"id": w.id, "title": w.title.unwrap_or_else(|| w.app_id.unwrap_or_else(|| "Window".into())), "focused": w.focused})).collect())
}

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let options = parse_args();
    let dir = ede_ui::default_dir();
    let (ui, initial) = match Runtime::open_readonly(&dir) {
        Ok(opened) => opened,
        Err(e) => {
            eprintln!("ede-shell: {}: {e}", dir.display());
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
            eprintln!("ede-shell: watch {}: {e}", dir.display());
            std::process::exit(2);
        }
    };

    // Window events from the compositor on their own connection.
    let (windows_tx, windows_rx) = mpsc::channel::<Vec<Value>>();
    std::thread::Builder::new()
        .name("ede-shell-windows".into())
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
                if event.kind.starts_with("window.") {
                    if let Some(list) = windows_json(&mut calls) {
                        if windows_tx.send(list).is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .expect("the windows thread");

    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
        let shell: SharedShell = Rc::new(RefCell::new(Shell { options, ui: ui.clone(), open: HashMap::new(), data: ShellData { time: clock(), windows: Vec::new() }, ede: ede_protocol::Client::connect_env().ok(), theme, _watcher: watcher }));

        // Every layer document on screen; popups wait for their anchor.
        let docs: Vec<Document> = ui.lock().unwrap_or_else(|p| p.into_inner()).documents().cloned().collect();
        for doc in &docs {
            if matches!(doc.surface, SurfaceHeader::Layer { .. }) {
                open_layer(&shell, doc, cx);
            }
        }
        for change in initial {
            if let Change::Rejected { name, issues, .. } = change {
                tracing::warn!(document = %name, ?issues, "refused at start");
            }
        }
        if shell.borrow().open.is_empty() {
            tracing::warn!(dir = %dir.display(), "no layer document to draw (is the factory directory installed? ede-ui --factory apps/ede/ui check)");
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

        if shell.borrow().options.popup {
            let s = shell.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(700)).await;
                let names: Vec<String> = s.borrow().ui.lock().unwrap_or_else(|p| p.into_inner()).documents().filter(|d| matches!(d.surface, SurfaceHeader::Popup { .. })).map(|d| d.name.clone()).collect();
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
