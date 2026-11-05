//! `ede-shell`: eDE's shell, drawn from genUI documents (#EDE-10). Its own
//! process next to `ede-compositor`: a wlr-layer-shell client (gpui's
//! `WindowKind::LayerShell`) painting eui-gpui surfaces, so a shell crash
//! never takes the session down and the compositor never links gpui.
//!
//! This is the #EDE-5 spike cut: the factory panel document in a top layer
//! surface, a clock fed through `data`, and the menu document as the panel's
//! own popup surface (an `xdg_popup` child of the layer surface), which is
//! what lets a popover leave the 40 px panel: eui-gpui paints overlays
//! inside their window.
//!
//! `ede-shell [--windowed] [--popup] [--exit-after <ms>]`
//!   --windowed    a normal window instead of a layer surface (macOS, any desktop)
//!   --popup       open the menu popup right away (the headless check)
//!   --exit-after  quit after this many milliseconds (the headless check)

use std::{rc::Rc, time::Duration};

use eui_core::{surface::SurfaceSettings, theme::{Mode, ModeSetting}, NestedNode};
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions}, point, popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions}, prelude::*, px, size, AnyWindowHandle, App, Bounds, Entity, Hsla, Window, WindowBounds, WindowKind, WindowOptions,
};
use serde_json::{json, Value};

const PANEL: &str = include_str!("../../../../ui/panel.json");
const MENU: &str = include_str!("../../../../ui/menu.json");
const PANEL_HEIGHT: f32 = 40.0;
const MENU_SIZE: (f32, f32) = (280.0, 200.0);

/// The IDE's bundled faces, so the shell draws the same type everywhere.
const FONTS: &[&[u8]] = &[
    include_bytes!("../../../../../desktop/assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../../../desktop/assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../../../desktop/assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../../../desktop/assets/fonts/JetBrainsMono-Regular.ttf"),
];

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

/// One document on screen: the surface view inside the window's background.
struct Surface {
    view: Entity<SurfaceView>,
    bg: Hsla,
}

impl Render for Surface {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().bg(self.bg).child(self.view.clone())
    }
}

/// What the panel's buttons do. The spike logs every action and opens the
/// menu on `menu`; #EDE-10 wires focus/launch to ede-protocol.
struct ShellHost {
    on_menu: Rc<dyn Fn(&mut App)>,
}

impl HostPlugin for ShellHost {
    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        tracing::info!(surface = %event.surface_id, component = %event.component_id, name = %event.name, context = %event.context, "action");
        if event.name == "menu" {
            (self.on_menu)(cx);
        }
    }
}

fn document(source: &str) -> (NestedNode, Value) {
    let v: Value = serde_json::from_str(source).expect("the factory document parses");
    let tree: NestedNode = serde_json::from_value(v["root"].clone()).expect("the factory document is a nested tree");
    (tree, v.get("data").cloned().unwrap_or(Value::Null))
}

fn background(theme: &eui_core::theme::ResolvedTheme, mode: Mode, token: &str) -> Hsla {
    theme.modes.get(mode).color.get(token).and_then(|c| eui_gpui::paint::color::parse_hex(c)).unwrap_or(gpui::black())
}

fn open_surface(name: &str, source: &str, kind: WindowKind, bounds: Bounds<gpui::Pixels>, bg_token: &str, host: Rc<dyn HostPlugin>, cx: &mut App) -> (gpui::WindowHandle<Surface>, Entity<SurfaceView>) {
    let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
    let mode = Mode::Dark;
    let bg = background(&theme, mode, bg_token);
    let (tree, data) = document(source);
    let surface_id = name.to_string();
    let mut out: Option<Entity<SurfaceView>> = None;
    let handle = cx
        .open_window(
            WindowOptions { kind, window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, app_id: Some("at.exponential.ede.shell".into()), focus: false, is_resizable: false, ..Default::default() },
            |window, cx| {
                let view = cx.new(|cx| {
                    let mut v = SurfaceView::new(
                        SurfaceViewOptions { surface_id: surface_id.clone(), theme: Some(theme.clone()), mode, host, settings: Some(SurfaceSettings { locale: "en-US".into(), mode: ModeSetting::Dark, ..SurfaceSettings::default() }), ..Default::default() },
                        window,
                        cx,
                    );
                    if let Some(obj) = data.as_object() {
                        for (k, val) in obj {
                            v.set_data(&format!("/{k}"), Some(val.clone()), cx).expect("factory data");
                        }
                    }
                    v.set_nested(tree.clone(), cx);
                    v
                });
                out = Some(view.clone());
                cx.new(|_| Surface { view, bg })
            },
        )
        .expect("a window");
    (handle, out.expect("the view"))
}

fn clock() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Local time without a tz crate: the TZ offset from libc's localtime is
    // #EDE-10's job; the spike shows UTC.
    format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let options = parse_args();
    gpui_platform::application().with_assets(gpui_component_assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(FONTS.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect());

        let width = 1280.0;
        let panel_kind = if options.windowed {
            WindowKind::Normal
        } else {
            WindowKind::LayerShell(LayerShellOptions {
                namespace: "ede-panel".into(),
                layer: Layer::Top,
                anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                exclusive_zone: Some(px(PANEL_HEIGHT)),
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                ..Default::default()
            })
        };
        let panel_bounds = Bounds { origin: point(px(0.0), px(0.0)), size: size(px(width), px(PANEL_HEIGHT)) };

        // The menu: a popup surface anchored under the panel's first button.
        let windowed = options.windowed;
        let menu_handle: Rc<std::cell::RefCell<Option<AnyWindowHandle>>> = Rc::new(std::cell::RefCell::new(None));
        let panel_handle: Rc<std::cell::RefCell<Option<AnyWindowHandle>>> = Rc::new(std::cell::RefCell::new(None));
        let open_menu: Rc<dyn Fn(&mut App)> = {
            let menu_handle = menu_handle.clone();
            let panel_handle = panel_handle.clone();
            Rc::new(move |cx: &mut App| {
                if let Some(open) = menu_handle.borrow_mut().take() {
                    let _ = open.update(cx, |_, window, _| window.remove_window());
                    return;
                }
                let Some(parent) = *panel_handle.borrow() else { return };
                let kind = if windowed {
                    WindowKind::PopUp
                } else {
                    WindowKind::AnchoredPopup(PopupOptions {
                        parent,
                        anchor_rect: Bounds { origin: point(px(8.0), px(0.0)), size: size(px(120.0), px(PANEL_HEIGHT)) },
                        anchor: PopupAnchor::BottomLeft,
                        gravity: PopupGravity::BottomRight,
                        constraint_adjustment: PopupConstraintAdjustment::SLIDE_X | PopupConstraintAdjustment::FLIP_Y,
                        offset: point(px(0.0), px(4.0)),
                        grab: false,
                    })
                };
                let bounds = Bounds { origin: point(px(8.0), px(PANEL_HEIGHT + 4.0)), size: size(px(MENU_SIZE.0), px(MENU_SIZE.1)) };
                let (handle, _) = open_surface("menu", MENU, kind, bounds, "popover", Rc::new(ShellHost { on_menu: Rc::new(|_| {}) }), cx);
                *menu_handle.borrow_mut() = Some(handle.into());
                tracing::info!("menu popup opened");
            })
        };

        let (panel, panel_view) = open_surface("panel", PANEL, panel_kind, panel_bounds, "card", Rc::new(ShellHost { on_menu: open_menu.clone() }), cx);
        *panel_handle.borrow_mut() = Some(panel.into());
        tracing::info!(windowed = options.windowed, "panel surface opened");

        // The clock: data in, the painter redraws what changed.
        let view = panel_view.clone();
        cx.spawn(async move |cx| loop {
            let now = clock();
            if view.update(cx, |v, cx| v.set_data("/time", Some(json!(now)), cx)).is_err() {
                break;
            }
            cx.background_executor().timer(Duration::from_secs(1)).await;
        })
        .detach();

        if options.popup {
            let open_menu = open_menu.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(700)).await;
                cx.update(|cx| open_menu(cx));
            })
            .detach();
        }
        if let Some(ms) = options.exit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
        cx.activate(true);
    });
}
