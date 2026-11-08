//! Apps on the Engine (#VAPP-273): the shell hosts `eengine` for every
//! package in `$XDG_CONFIG_HOME/exponential-desktop/apps/*.app.json` (next
//! to the `ui/` documents; `EDE_APPS_DIR` overrides). One `SqliteRecordStore`
//! and one `FsBlobStore` under the data directory serve them all, keyed by
//! app id; the identity is `local` (this device, no account); the host kind
//! is `ede`. Every app runs the desktop runner's core (`eengine_runner`:
//! the same `app`, `consent`, `permissions` and `toast` surfaces as an
//! exported app) in ONE genUI host per app; its window is a plain toplevel
//! of the shell process (the compositor and the taskbar see it by its app
//! id), and each `screens[].widget` is a layer surface on the `bottom`
//! layer, stacked down the top-right of the ground (shell-layout.json
//! `widgets`), live while the window is closed. The capabilities: the
//! desktop's natives (files, notifications, photos, print) plus eDE's own
//! `windows`, `focus` and `launch` over ede-protocol ([`capabilities`]).

pub mod capabilities;
pub mod exposes;
pub mod shell_edit;
pub mod synced;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use eengine::{FormFactor, FsBlobStore, NetworkPolicy, RunnerConfig, SqliteRecordStore};
use eengine_runner::paint::{self, Ctx, Inbound, RunnerView, WidgetView};
use eengine_runner::runner::{Providers, Runner};
use eengine_runner::vault::RunnerVault;
use eui_core::host::PackageWidget;
use eui_core::theme::ResolvedTheme;
use eui_gpui::host::HostPlugin;
use eui_gpui::runtime::ExponentialHost;
use gpui::{
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, prelude::*, px, size, AnyWindowHandle, App, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowId, WindowKind, WindowOptions,
};
use serde_json::{json, Value};
use uicore_data::apps;
use uicore_model::layout::shell_layout;

/// One package file of the apps directory.
#[derive(Debug, Clone)]
pub struct EngineApp {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub path: PathBuf,
    /// The package file as read (a signed file or a bare package).
    pub file: Value,
    /// #VAPP-273: on a connected team, the server's app id (the key of its
    /// records in the synced store); `None` on a Local desktop.
    pub store_key: Option<String>,
}

/// The factory apps the package installs (`$EDE_FACTORY_APPS_DIR`, else
/// `/usr/share/exponential-desktop/apps`); a person's file of the same name
/// in [`apps_dir`] replaces one.
pub const FACTORY_APPS_DIR: &str = "/usr/share/exponential-desktop/apps";

pub fn factory_apps_dir() -> PathBuf {
    std::env::var_os("EDE_FACTORY_APPS_DIR").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(FACTORY_APPS_DIR))
}

/// `$EDE_APPS_DIR`, else the `apps` directory next to the shell's `ui` one.
pub fn apps_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EDE_APPS_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let ui = ede_ui::default_dir();
    ui.parent().map(Path::to_path_buf).unwrap_or(ui).join("apps")
}

/// `$EDE_APPS_DATA_DIR`, else `$XDG_DATA_HOME/exponential-desktop/apps`: the
/// records store, the files and each app's vault file.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EDE_APPS_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local").join("share"));
    data.join("exponential-desktop").join("apps")
}

/// The apps of the factory directory, then the person's own, a file of the
/// same name in the person's directory replacing the factory one; by file name.
pub fn scan_installed(factory: &Path, personal: &Path) -> Vec<EngineApp> {
    let mut apps = scan(factory);
    for app in scan(personal) {
        apps.retain(|a| a.path.file_name() != app.path.file_name());
        apps.push(app);
    }
    apps.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
    apps
}

/// Every `*.app.json` of `dir` that parses and names a package id, by file
/// name; whether it VALIDATES is the Engine's call at boot (a refused one is
/// logged and skipped, the others run).
pub fn scan(dir: &Path) -> Vec<EngineApp> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".app.json"))).collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let file: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(path = %path.display(), "not an app package: {e}");
                continue;
            }
        };
        let pkg = eengine::package::package_of(&file);
        let Some(id) = pkg.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
            tracing::warn!(path = %path.display(), "an app package without an id");
            continue;
        };
        let text = |k: &str| pkg.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        out.push(EngineApp { id: id.to_string(), name: if text("name").is_empty() { id.to_string() } else { text("name") }, description: text("description"), version: if text("version").is_empty() { "0.0.0".into() } else { text("version") }, path, file, store_key: None });
    }
    out
}

/// The launcher's entry for an app: the start menu lists it with the
/// installed applications; `ede_launch {app: <id>}` opens its window.
pub fn launcher_entry(app: &EngineApp) -> apps::App {
    apps::App {
        id: app.id.clone(),
        name: app.name.clone(),
        comment: app.description.clone(),
        exec: String::new(),
        exec_url: None,
        icon: None,
        keywords: format!("{} {}", app.name, app.id).to_lowercase(),
        categories: vec!["Utility".into()],
    }
}

/// Icons by concept from the shell's embedded set; an `app-file:` src from
/// the app's own files (VAPP-271).
struct Plugin {
    files: Arc<dyn eui_gpui::host::AppFiles>,
}

impl HostPlugin for Plugin {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        crate::assets::icon_path(name)
    }

    fn app_files(&self) -> Option<Arc<dyn eui_gpui::host::AppFiles>> {
        Some(self.files.clone())
    }
}

/// One app as it runs: its runner context, its window (when open) and its widget surfaces.
pub struct Running {
    pub app: EngineApp,
    pub ctx: Rc<Ctx>,
    /// The viewer the instance writes as (the device id on a Local desktop).
    pub me: String,
    pub theme: Arc<ResolvedTheme>,
    pub window: Option<AnyWindowHandle>,
    /// `(widget, surface id)` for every `screens[].widget`.
    pub widgets: Vec<(PackageWidget, String)>,
    pub widget_windows: Vec<AnyWindowHandle>,
    _store: Option<eengine::Subscription>,
}

#[derive(Default)]
pub struct Engine {
    pub running: Vec<Running>,
    /// The most an app window takes (the ground's free area: the output
    /// minus the sidebar, the taskbar and the panel margins); the runner's
    /// 1024×720 when it fits. The shell sets it at start and on an output change.
    pub window_max: (u32, u32),
}

/// The default window of an exported app (the desktop runner's).
pub const WINDOW_SIZE: (u32, u32) = (1024, 720);

pub type SharedEngine = Rc<RefCell<Engine>>;

impl Engine {
    pub fn has(&self, app_id: &str) -> bool {
        self.running.iter().any(|r| r.app.id == app_id)
    }
}

/// What every app shares: the stores under the data directory and eDE's
/// capabilities; on a connected team (#VAPP-273 `synced`) the viewer and
/// the team's synced store instead of the local records.
pub struct Stores {
    pub data_dir: PathBuf,
    pub storage: Arc<SqliteRecordStore>,
    pub blobs: Arc<FsBlobStore>,
    pub capabilities: Vec<Arc<dyn eengine::Capability>>,
    pub identity: Option<Arc<dyn eengine::Identity>>,
    pub synced: Option<Arc<synced::TeamStores>>,
}

impl Stores {
    /// Opens (or creates) the stores under `data_dir`.
    pub fn open(data_dir: PathBuf, capabilities: Vec<Arc<dyn eengine::Capability>>) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        let storage = SqliteRecordStore::open(data_dir.join("records.sqlite")).map_err(|e| e.to_string())?;
        let blobs = FsBlobStore::new(data_dir.join("files"));
        Ok(Stores { data_dir, storage: Arc::new(storage), blobs: Arc::new(blobs), capabilities, identity: None, synced: None })
    }

    /// The same stores for a connected team: the viewer and the team's synced store.
    pub fn connected(mut self, identity: Arc<dyn eengine::Identity>, team: Arc<synced::TeamStores>) -> Self {
        self.identity = Some(identity);
        self.synced = Some(team);
        self
    }
}

/// The surface a widget paints on: one per screen, never the window's `app`.
fn widget_surface_id(widget: &PackageWidget) -> String {
    format!("widget-{}", widget.screen)
}

/// Boots every app: one Engine instance and one genUI host each, the
/// first screen opened on `app` (the window shows it when it opens) and
/// every widget's template applied on its own surface. A package the
/// Engine refuses is logged and left out.
pub fn boot(apps: Vec<EngineApp>, stores: &Stores, cx: &mut App) -> SharedEngine {
    let engine: SharedEngine = Rc::new(RefCell::new(Engine { running: Vec::new(), window_max: WINDOW_SIZE }));
    for app in apps {
        let (tx, rx) = flume::unbounded::<Inbound>();
        let app_dir = stores.data_dir.join(&app.id);
        let _ = std::fs::create_dir_all(&app_dir);
        let on_tap: eengine_desktop::OnTap = {
            let tx = tx.clone();
            Arc::new(move |tap: Value| {
                let _ = tx.send(Inbound::Tap(tap));
            })
        };
        let mut desktop = eengine_desktop::DesktopConfig::new(app.name.clone(), on_tap);
        desktop.app_id = Some(app.id.clone());
        let mut capabilities = eengine_desktop::desktop_capabilities(desktop);
        capabilities.extend(stores.capabilities.iter().cloned());
        let synced = match (&stores.synced, &app.store_key) {
            (Some(team), Some(key)) => Some(eengine_runner::runner::Synced { store: team.store.clone(), app: key.clone(), online: team.online.clone() }),
            _ => None,
        };
        let vault = Arc::new(RunnerVault::platform(&app.id, &app.name, &app_dir));
        // The viewer: the member on a connected team, else this device.
        let me = match &stores.identity {
            Some(identity) => eengine::providers::engine_viewer(&identity.identity()).id,
            None => eengine_runner::vault::device_id(vault.as_ref()),
        };
        let providers = Providers {
            host_kind: "ede".into(),
            platform: "linux".into(),
            locale: paint::locale(),
            storage: stores.storage.clone(),
            blobs: stores.blobs.clone(),
            vault,
            capabilities,
            identity: stores.identity.clone(),
            synced,
        };
        if let Some(team) = &stores.synced {
            team.pumps.lock().unwrap_or_else(|p| p.into_inner()).push(tx.clone());
        }
        let config = RunnerConfig { id: app.id.clone(), name: app.name.clone(), version: app.version.clone(), network: NetworkPolicy::Any };
        let mut runner = match Runner::boot(config, &app.file, providers, FormFactor::Wide) {
            Ok(r) => r,
            Err(kind) => {
                tracing::warn!(app = %app.id, path = %app.path.display(), ?kind, "the Engine refused the package");
                continue;
            }
        };
        let theme = paint::theme_of(Some(&app.file));
        let ctx_slot: Rc<RefCell<Option<Rc<Ctx>>>> = Rc::new(RefCell::new(None));
        let plugin = Rc::new(Plugin { files: runner.app_files() });
        let options = paint::host_options(theme.clone(), plugin, Some(&runner), tx.clone(), ctx_slot.clone());
        let host = cx.new(|cx| ExponentialHost::new(options, cx));
        let ctx = Rc::new(Ctx::new(host, tx.clone()));
        *ctx_slot.borrow_mut() = Some(ctx.clone());
        let store = {
            let tx = tx.clone();
            runner.storage().subscribe(runner.app_id(), Box::new(move |_| {
                let _ = tx.send(Inbound::StoreChanged);
            }))
        };
        let widgets: Vec<(PackageWidget, String)> = eui_core::host::package_widgets(runner.instance().package()).into_iter().map(|w| (w.clone(), widget_surface_id(&w))).collect();
        if !widgets.is_empty() {
            ctx.always_live.set(true);
        }
        let paints = runner.launch(Vec::new());
        let app_id = runner.app_id().to_string();
        *ctx.runner.borrow_mut() = Some(runner);
        ctx.apply(paints, cx);
        for (widget, surface_id) in &widgets {
            let message = json!({"version": eui_core::catalog::A2UI_VERSION, "applyTemplate": {"surfaceId": surface_id, "templateId": widget.template, "packageId": app_id, "data": {"params": widget.params}}});
            ctx.receive(&message, cx);
        }
        ctx.refresh();
        tracing::info!(app = %app_id, widgets = widgets.len(), "app booted");
        // The pump: every inbound event of this app on the main thread, in
        // order; a navigation (a widget's press, a notification tap) opens the window.
        {
            let (ctx, engine, app_id) = (ctx.clone(), engine.clone(), app_id.clone());
            cx.spawn(async move |cx| {
                while let Ok(event) = rx.recv_async().await {
                    let navigated = matches!(event, Inbound::Navigate { .. } | Inbound::Link(_) | Inbound::Tap(_));
                    let _ = cx.update(|cx| {
                        ctx.handle(event, cx);
                        if navigated {
                            open_window(&engine, &app_id, cx);
                        }
                    });
                }
            })
            .detach();
        }
        engine.borrow_mut().running.push(Running { app, ctx, me, theme, window: None, widgets, widget_windows: Vec::new(), _store: Some(store) });
    }
    {
        // A closed window: the app goes to the background and may be opened again.
        let engine = engine.clone();
        cx.on_window_closed(move |_, id: WindowId| {
            let mut e = engine.borrow_mut();
            for r in &mut e.running {
                if r.window.is_some_and(|h| h.window_id() == id) {
                    r.window = None;
                    r.ctx.window.set(None);
                    let _ = r.ctx.tx.send(Inbound::Activation(false));
                }
            }
        })
        .detach();
    }
    engine
}

/// Every app's windows and widgets go (another desktop was picked); the
/// instances with them.
pub fn shutdown(engine: &SharedEngine, cx: &mut App) {
    let running = std::mem::take(&mut engine.borrow_mut().running);
    for r in running {
        for handle in r.window.into_iter().chain(r.widget_windows) {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        let paints = r.ctx.runner.borrow_mut().as_mut().map(Runner::terminate).unwrap_or_default();
        r.ctx.apply(paints, cx);
    }
}

/// Every running app's exposed tools for the desktop agent's hub
/// (`exposes`), writing through each app's pump; `exponential` = the
/// desktop's noun subset the `action` runs call.
pub fn apps_provider(engine: &Engine, exponential: Arc<dyn emcp::server::ToolProvider>) -> exposes::AppsProvider {
    let apps = engine
        .running
        .iter()
        .map(|r| {
            let storage = r.ctx.runner.borrow().as_ref().map(|runner| runner.storage().clone());
            let package = r.ctx.runner.borrow().as_ref().map(|runner| runner.instance().package().clone()).unwrap_or(Value::Null);
            (r, storage, package)
        })
        .filter_map(|(r, storage, package)| {
            Some(Arc::new(exposes::AppTools { id: r.app.id.clone(), server: exposes::server_name(&r.app.id), package, storage: storage?, me: r.me.clone(), writes: Arc::new(exposes::PumpWrites(r.ctx.tx.clone())) }))
        })
        .collect();
    exposes::AppsProvider { apps, exponential }
}

/// The app's window: a plain toplevel of the shell process (its app id is
/// the package id, so the taskbar groups it under the launcher's entry);
/// an open one comes to the front.
pub fn open_window(engine: &SharedEngine, app_id: &str, cx: &mut App) {
    let Some(i) = engine.borrow().running.iter().position(|r| r.app.id == app_id) else { return };
    let (ctx, theme, title, existing) = {
        let e = engine.borrow();
        let r = &e.running[i];
        (r.ctx.clone(), r.theme.clone(), r.app.name.clone(), r.window)
    };
    if let Some(window) = existing {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let max = engine.borrow().window_max;
    let (w, h) = (WINDOW_SIZE.0.min(max.0.max(320)), WINDOW_SIZE.1.min(max.1.max(240)));
    let bounds = Bounds::centered(None, size(px(w as f32), px(h as f32)), cx);
    let options = WindowOptions {
        kind: WindowKind::Normal,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some(title.into()), ..Default::default() }),
        app_id: Some(app_id.to_string()),
        ..Default::default()
    };
    let host = ctx.host.clone();
    let view_ctx = ctx.clone();
    let opened = cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let mut subs = vec![cx.observe(&host, |_, _, cx| cx.notify())];
            let tx = view_ctx.tx.clone();
            subs.push(cx.observe_window_activation(window, move |_, window, _| {
                let _ = tx.send(Inbound::Activation(window.is_window_active()));
            }));
            RunnerView::new(view_ctx.clone(), theme.clone(), None, subs)
        })
    });
    match opened {
        Ok(handle) => {
            let any: AnyWindowHandle = handle.into();
            ctx.window.set(Some(any));
            engine.borrow_mut().running[i].window = Some(any);
            let _ = any.update(cx, |_, window, _| window.activate_window());
            tracing::info!(app = app_id, "app window opened");
        }
        Err(e) => tracing::warn!(app = app_id, "app window: {e}"),
    }
}

/// The most an app window takes on an output of `out` px: the ground's
/// free area inside the rail (#EDE-65: the sidebar for good), the taskbar
/// and the panel margins (shell-layout.json), never below 320×240.
pub fn window_max(out: (u32, u32)) -> (u32, u32) {
    let layout = shell_layout();
    (out.0.saturating_sub(layout.rail_width + 2 * layout.panel_margin).max(320), out.1.saturating_sub(layout.taskbar_height + 2 * layout.panel_margin).max(240))
}

/// Where the `n`th widget of the column goes: its size for its class and
/// the top and right margins from the output's edges (the panel's margin,
/// then the widgets' own, then the widgets above it with the gap).
pub fn widget_slot(n: usize, sizes: &[String]) -> (uicore_model::layout::Size, u32, u32) {
    let layout = shell_layout();
    let margin = layout.panel_margin + layout.widgets.margin;
    let above: u32 = sizes.iter().take(n).map(|s| layout.widgets.size_px(s).height + layout.widgets.gap).sum();
    (layout.widgets.size_px(sizes.get(n).map(String::as_str).unwrap_or("s")), margin + above, margin)
}

/// Every widget on screen: one layer surface each on the `bottom` layer
/// (over the ground, under the windows), anchored top-right.
pub fn open_widgets(engine: &SharedEngine, cx: &mut App) {
    let plan: Vec<(usize, Rc<Ctx>, String, String, String)> = {
        let e = engine.borrow();
        e.running.iter().enumerate().flat_map(|(i, r)| r.widgets.iter().map(move |(w, sid)| (i, r.ctx.clone(), r.app.id.clone(), sid.clone(), w.size.clone()))).collect()
    };
    let sizes: Vec<String> = plan.iter().map(|p| p.4.clone()).collect();
    for (n, (i, ctx, app_id, surface_id, _)) in plan.into_iter().enumerate() {
        let (sz, top, right) = widget_slot(n, &sizes);
        let options = WindowOptions {
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "exponential-shell:widget".into(),
                layer: Layer::Bottom,
                anchor: Anchor::TOP | Anchor::RIGHT,
                exclusive_zone: None,
                exclusive_edge: None,
                margin: Some((px(top as f32), px(right as f32), px(0.0), px(0.0))),
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.0), px(0.0)), size: size(px(sz.width as f32), px(sz.height as f32)) })),
            titlebar: None,
            app_id: Some(app_id.clone()),
            focus: false,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let host = ctx.host.clone();
        let view_ctx = ctx.clone();
        let sid = surface_id.clone();
        match cx.open_window(options, |_, cx| cx.new(|cx| WidgetView::new(view_ctx, sid, vec![cx.observe(&host, |_, _, cx| cx.notify())]))) {
            Ok(handle) => {
                engine.borrow_mut().running[i].widget_windows.push(handle.into());
                tracing::info!(app = %app_id, surface = %surface_id, "widget opened");
            }
            Err(e) => tracing::warn!(app = %app_id, surface = %surface_id, "widget: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../../../ede/apps/notes.app.json");

    #[test]
    fn the_sample_app_scans_validates_and_declares_a_widget() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.app.json"), SAMPLE).unwrap();
        std::fs::write(dir.path().join("broken.app.json"), "{").unwrap();
        std::fs::write(dir.path().join("nameless.app.json"), "{\"formatVersion\": 6}").unwrap();
        std::fs::write(dir.path().join("readme.json"), "{}").unwrap();
        let apps = scan(dir.path());
        assert_eq!(apps.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["at.exponential.notes"]);
        let app = &apps[0];
        assert_eq!((app.name.as_str(), app.version.as_str()), ("Notes", "1.0.0"));
        let pkg = eengine::package::package_of(&app.file);
        let issues = eengine::package::validate(pkg);
        assert!(issues.is_empty(), "{issues:?}");
        let widgets = eui_core::host::package_widgets(pkg);
        assert_eq!(widgets.len(), 1);
        assert_eq!((widgets[0].screen.as_str(), widgets[0].template.as_str(), widgets[0].size.as_str()), ("notes", "tile", "s"));
        let entry = launcher_entry(app);
        assert_eq!((entry.id.as_str(), entry.name.as_str(), entry.exec.as_str()), ("at.exponential.notes", "Notes", ""));
        assert!(entry.keywords.contains("notes"));
    }

    #[test]
    fn an_app_window_fits_the_ground() {
        assert_eq!(window_max((1920, 1080)), (1844, 1012));
        assert_eq!(window_max((1280, 720)), (1204, 652));
        assert_eq!(window_max((300, 200)), (320, 240));
    }

    #[test]
    fn a_persons_app_replaces_the_factory_one_by_file_name() {
        let factory = tempfile::tempdir().unwrap();
        let personal = tempfile::tempdir().unwrap();
        std::fs::write(factory.path().join("notes.app.json"), SAMPLE).unwrap();
        std::fs::write(factory.path().join("clock.app.json"), SAMPLE.replace("at.exponential.notes", "at.exponential.clock")).unwrap();
        std::fs::write(personal.path().join("notes.app.json"), SAMPLE.replace("\"name\": \"Notes\"", "\"name\": \"My notes\"")).unwrap();
        let apps = scan_installed(factory.path(), personal.path());
        assert_eq!(apps.iter().map(|a| (a.id.as_str(), a.name.as_str())).collect::<Vec<_>>(), [("at.exponential.clock", "Notes"), ("at.exponential.notes", "My notes")]);
        assert!(apps[1].path.starts_with(personal.path()));
        assert!(scan_installed(Path::new("/nonexistent"), personal.path()).len() == 1);
    }

    #[test]
    fn widgets_stack_down_the_top_right_of_the_panel() {
        let sizes: Vec<String> = ["s", "m", "l"].iter().map(|s| s.to_string()).collect();
        let (first, top, right) = widget_slot(0, &sizes);
        assert_eq!((first.width, first.height, top, right), (160, 160, 26, 26));
        let (second, top, _) = widget_slot(1, &sizes);
        assert_eq!((second.width, second.height, top), (336, 160, 26 + 160 + 16));
        let (third, top, _) = widget_slot(2, &sizes);
        assert_eq!((third.width, third.height, top), (336, 336, 26 + 160 + 16 + 160 + 16));
    }

    #[test]
    fn the_sample_app_boots_on_the_engine_and_lists_its_notes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.app.json"), SAMPLE).unwrap();
        let app = scan(dir.path()).remove(0);
        let stores = Stores::open(dir.path().join("data"), Vec::new()).unwrap();
        let providers = Providers {
            host_kind: "ede".into(),
            platform: "linux".into(),
            locale: "en-US".into(),
            storage: stores.storage.clone(),
            blobs: stores.blobs.clone(),
            vault: Arc::new(eengine::MemoryVault::default()),
            capabilities: Vec::new(),
            identity: None,
            synced: None,
        };
        let config = RunnerConfig { id: app.id.clone(), name: app.name.clone(), version: app.version.clone(), network: NetworkPolicy::Any };
        let mut runner = Runner::boot(config, &app.file, providers, FormFactor::Wide).expect("boots");
        let paints = runner.launch(Vec::new());
        assert!(paints.iter().any(|p| matches!(p, eengine_runner::runner::Paint::Install(_))));
        assert_eq!(runner.instance().state().screen.as_deref(), Some("notes"));
        // The genUI host runs `actions_add`'s plan; its records step is this call.
        let answer = runner.dispatch("records_upsert", &json!({"kind": "note", "data": {"text": "Buy milk"}}));
        let eengine_runner::runner::Dispatch::Done(eengine::CallResult::Value(_)) = answer else { panic!("the write lands") };
        let notes = runner.records("records:note").expect("a records source");
        assert_eq!(notes["count"], 1);
        assert_eq!(notes["rows"][0]["text"], "Buy milk");
    }
}
