//! The kitchen sink in a real window, outside the IDE (a visual check of the
//! painter on any desktop): `cargo run -p exponential-ui-gpui --example
//! kitchen_sink`. Env: `XUI_THEME=neutral|exponential|playful` (default
//! `exponential`), `XUI_MODE=light|dark|system`, `XUI_LOCALE=<bcp47>` (`ar`
//! paints RTL), `XUI_DENSITY=compact|comfortable`, `XUI_OPEN=1` (open every
//! overlay), `XUI_SCROLL=<px>` (pre-scroll), `XUI_SIZE=<w>x<h>`.

use exponential_ui::surface::SurfaceSettings;
use exponential_ui::theme::{Density, Mode, ModeSetting};
use exponential_ui::NestedNode;
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, size, App, Bounds, Entity, ScrollHandle, TitlebarOptions, Window, WindowBounds, WindowOptions};
use serde_json::{json, Value};

const KITCHEN_SINK: &str = include_str!("../../../../../packages/exponential-ui/fixtures/kitchen-sink.json");

struct Shell {
    view: Entity<SurfaceView>,
    scroll: ScrollHandle,
    pending: Option<f32>,
    bg: gpui::Hsla,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if let Some(y) = self.pending {
            if self.scroll.max_offset().y > px(0.0) {
                self.scroll.set_offset(gpui::point(px(0.0), px(-y)));
                self.pending = None;
                cx.notify();
            }
        }
        div().size_full().bg(self.bg).child(div().id("scroll").size_full().overflow_y_scroll().track_scroll(&self.scroll).child(div().w_full().p(px(16.0)).child(self.view.clone())))
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn main() {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    app.run(|cx: &mut App| {
        gpui_component::init(cx);
        let fonts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts");
        let mut data = Vec::new();
        for entry in std::fs::read_dir(fonts).into_iter().flatten().flatten() {
            if entry.path().extension().is_some_and(|e| e == "ttf") {
                if let Ok(bytes) = std::fs::read(entry.path()) {
                    data.push(std::borrow::Cow::Owned(bytes));
                }
            }
        }
        let _ = cx.text_system().add_fonts(data);
        let theme_id = env("XUI_THEME").unwrap_or_else(|| "exponential".into());
        let theme = exponential_ui::themes::builtin_theme(&theme_id).or_else(|| exponential_ui::themes::builtin_theme("exponential"));
        let locale = env("XUI_LOCALE").unwrap_or_else(|| "en-US".into());
        let mode_env = env("XUI_MODE").unwrap_or_else(|| "dark".into());
        let mode = if mode_env == "light" { Mode::Light } else { Mode::Dark };
        let settings = SurfaceSettings {
            locale: locale.clone(),
            mode: match mode_env.as_str() {
                "system" => ModeSetting::System,
                "light" => ModeSetting::Light,
                _ => ModeSetting::Dark,
            },
            density: env("XUI_DENSITY").and_then(|d| Density::parse(&d)).unwrap_or_default(),
            ..SurfaceSettings::default()
        };
        let (w, h) = env("XUI_SIZE").and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)))).unwrap_or((1440.0, 900.0));
        let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
        let bg = theme.as_ref().and_then(|t| t.modes.get(mode).color.get("background").cloned()).and_then(|c| exponential_ui_gpui::paint::color::parse_hex(&c)).unwrap_or(gpui::black());
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: Some(TitlebarOptions { title: Some("Exponential UI kitchen sink".into()), ..Default::default() }), ..Default::default() },
            move |window, cx| {
                let view = cx.new(|cx| {
                    let mut v = SurfaceView::new(SurfaceViewOptions { surface_id: "kitchen-sink".into(), theme, mode, settings: Some(settings), ..Default::default() }, window, cx);
                    v.set_data("/draft", Some(json!({"title": ""})), cx);
                    v.set_data("/ui", Some(json!({"confirmOpen": false})), cx);
                    let posts: Vec<Value> = (0..40).map(|i| json!({"title": format!("Post {}", i + 1), "score": format!("{} points", (40 - i) * 7)})).collect();
                    v.set_data("/posts", Some(Value::Array(posts)), cx);
                    // The fixture's root pins `direction: ltr`; an RTL locale
                    // lets the locale decide (the core's rule).
                    let mut raw: Value = serde_json::from_str(KITCHEN_SINK).expect("the kitchen sink parses");
                    if exponential_ui::locale::text_direction(&locale) == "rtl" {
                        if let Some(style) = raw.get_mut("style").and_then(Value::as_object_mut) {
                            style.remove("direction");
                        }
                    }
                    let tree: NestedNode = serde_json::from_value(raw).expect("the kitchen sink parses");
                    v.set_nested(tree, cx);
                    if env("XUI_OPEN").is_some() {
                        let nodes = v.surface_mut().nodes();
                        let ids: Vec<String> = nodes.iter().filter(|n| !n.removed).filter_map(|n| n.trigger_for.clone()).collect();
                        for id in ids {
                            v.surface_mut().set_open(&id, true);
                        }
                    }
                    v
                });
                let shell = cx.new(|_| Shell { view, scroll: ScrollHandle::new(), pending: env("XUI_SCROLL").and_then(|s| s.parse().ok()), bg });
                cx.new(|cx| gpui_component::Root::new(shell, window, cx))
            },
        )
        .expect("a window");
        cx.activate(true);
    });
}
