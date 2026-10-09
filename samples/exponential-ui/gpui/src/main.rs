//! VAPP-91 sample: a plain gpui window hosting an Exponential UI surface
//! streamed from the local A2UI JSONL server (samples/exponential-ui/server),
//! with the server's custom theme and ONE custom extension component
//! (TrendLine) painted natively. No Exponential account, no backend of ours:
//! the SDK's public API only (`ExponentialHost` + a transport + the
//! `SurfaceView` it creates per surface).
//!
//! `cargo run` = the surface once (`?once=1`, deterministic);
//! `cargo run -- --live` = the live stream (a data tick every 3 s);
//! `--dark` = dark mode; `EXP_SAMPLE_SERVER` = another server url.

use std::rc::Rc;
use std::sync::Arc;

use exponential_ui::extension::parse_extension;
use exponential_ui::measure::LeafRequest;
use exponential_ui::theme::Mode;
use exponential_ui::themes::theme_or_default;
use exponential_ui_gpui::extension::{ExtensionPainter, PaintContext};
use exponential_ui_gpui::paint::color::parse_hex;
use exponential_ui_gpui::runtime::{ExponentialHost, HostOptions};
use exponential_ui_gpui::transport::{HttpTransportOptions, JsonlStreamTransport};
use gpui::{canvas, div, point, prelude::*, px, size, AnyElement, App, Bounds, Entity, Hsla, PathBuilder, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions};
use serde_json::Value;

const SURFACE: &str = "greenhouse";

/// The extension's native painter: a polyline of the bound readings in
/// `color` (else the theme's primary), `height` tall, full width.
struct TrendLine;

impl TrendLine {
    fn values(props: &serde_json::Map<String, Value>) -> Vec<f32> {
        props.get("values").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).map(|v| v as f32).collect()).unwrap_or_default()
    }

    fn height(props: &serde_json::Map<String, Value>) -> f32 {
        props.get("height").and_then(Value::as_f64).unwrap_or(48.0) as f32
    }
}

impl ExtensionPainter for TrendLine {
    fn measure(&self, leaf: &LeafRequest, wrap: Option<f32>, _: &mut Window, _: &mut App) -> Option<(f32, f32)> {
        // Fills the width it is offered (min-content: 0).
        Some((wrap.unwrap_or(240.0), Self::height(leaf.props)))
    }

    fn paint(&self, ctx: PaintContext, _: &mut Window, _: &mut App) -> AnyElement {
        let values = Self::values(&ctx.node.props);
        let primary = ctx.theme.and_then(|t| t.modes.get(ctx.mode).color.get("primary").cloned());
        let color: Hsla = ctx.node.props.get("color").and_then(Value::as_str).map(str::to_string).or(primary).and_then(|c| parse_hex(&c)).unwrap_or(gpui::blue());
        let line = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if values.len() < 2 {
                    return;
                }
                let (min, max) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let inset = 2.0;
                let at = |i: usize, v: f32| {
                    let x = i as f32 / (values.len() - 1) as f32 * w;
                    let y = if max == min { h / 2.0 } else { inset + (1.0 - (v - min) / (max - min)) * (h - 2.0 * inset) };
                    point(bounds.origin.x + px(x), bounds.origin.y + px(y))
                };
                let mut path = PathBuilder::stroke(px(2.5));
                path.move_to(at(0, values[0]));
                for (i, v) in values.iter().enumerate().skip(1) {
                    path.line_to(at(i, *v));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            },
        )
        .size_full();
        div().size_full().child(line).children(ctx.children).into_any_element()
    }
}

struct SampleWindow {
    host: Entity<ExponentialHost>,
    background: Hsla,
    muted: Hsla,
}

impl Render for SampleWindow {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let host = self.host.read(cx);
        let status = SharedString::from(format!("transport: {}", host.status().as_str()));
        let surface = host.surface(SURFACE);
        div()
            .id("sample")
            .size_full()
            .overflow_y_scroll()
            .bg(self.background)
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(480.0))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(surface.clone().map(|s| s.into_any_element()))
                    .when(surface.is_none(), |d| d.child(div().text_color(self.muted).child("Waiting for the surface…")))
                    .child(div().text_size(px(12.0)).text_color(self.muted).child(status)),
            )
    }
}

fn fetch(url: &str) -> Result<String, String> {
    let res = reqwest::blocking::get(url).map_err(|e| format!("{url}: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("{url}: HTTP {}", res.status()));
    }
    res.text().map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let mode = if args.iter().any(|a| a == "--dark") { Mode::Dark } else { Mode::Light };
    let server = std::env::var("EXP_SAMPLE_SERVER").unwrap_or_else(|_| "http://localhost:4190".into());

    // The theme + the extension catalog, fetched at runtime (the server owns
    // them; nothing about them is compiled in).
    let theme_json: Value = serde_json::from_str(&fetch(&format!("{server}/theme.json")).expect("the theme")).expect("theme JSON");
    // An unusable server theme never fails the app: the default theme
    // paints and the issues are logged.
    let (theme, issues) = theme_or_default(&theme_json);
    if !issues.is_empty() {
        eprintln!("theme.json refused: {issues:?}");
    }
    let extension = parse_extension(&fetch(&format!("{server}/extension.json")).expect("the extension")).expect("a valid extension");

    let color = |name: &str| theme.modes.get(mode).color.get(name).and_then(|c| parse_hex(c));
    let background = color("background").unwrap_or(gpui::white());
    let muted = color("mutedForeground").unwrap_or(gpui::black().opacity(0.6));

    let stream = format!("{server}/a2ui.jsonl{}", if live { "" } else { "?once=1" });
    let transport = JsonlStreamTransport::new(HttpTransportOptions::new(stream).post_url(format!("{server}/action")).reconnect_ms(if live { 2000 } else { 0 }));

    gpui_platform::application().with_assets(gpui_component_assets::Assets).run(move |cx| {
        gpui_component::init(cx);
        let host = cx.new(|cx| {
            ExponentialHost::new(
                HostOptions {
                    transport: Some(Box::new(transport)),
                    extensions: vec![extension],
                    painters: vec![("TrendLine".into(), Rc::new(TrendLine))],
                    theme: Some(theme.clone()),
                    mode,
                    ..Default::default()
                },
                cx,
            )
        });
        host.update(cx, |h, cx| h.connect(cx));
        let bounds = Bounds::centered(None, size(px(540.0), px(620.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Exponential UI · gpui sample".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(options, |_, cx| {
            cx.new(|cx| {
                cx.observe(&host, |_, _, cx| cx.notify()).detach();
                SampleWindow { host: host.clone(), background, muted }
            })
        })
        .expect("open the window");
        cx.activate(true);
    });
}
