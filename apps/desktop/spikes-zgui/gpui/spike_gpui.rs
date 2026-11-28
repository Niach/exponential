//! gpui → zgui spike, the gpui half. NOT a production file: copied into
//! `apps/desktop/crates/eui-shot/examples/` for the run only (eui-shot links
//! gpui's headless renderer under test-support), never committed there.
//!
//! ```text
//! cp apps/desktop/spikes-zgui/gpui/spike_gpui.rs apps/desktop/crates/eui-shot/examples/
//! RUSTUP_TOOLCHAIN=1.96.0 cargo build --release -p eui-shot --example spike_gpui
//! SPIKE_T0=$(date +%s%N) target/release/examples/spike_gpui bench
//! target/release/examples/spike_gpui shot apps/desktop/spikes-zgui/shots/gpui.png
//! target/release/examples/spike_gpui window 25      # real window, exits after 25 s
//! ```
//!
//! Every `frame` = the core pass (`layout_now`) + gpui's layout, prepaint and
//! paint of the element tree (`Window::draw`), headless, no GPU submit (as
//! `eui-gpui/examples/bench_list.rs`). The test window's scale factor is 2.

use std::borrow::Cow;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use eui_core::theme::Mode;
use eui_core::types::FlatComponent;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{px, size, AppContext as _, HeadlessAppContext};
use serde_json::{json, Value};

fn since_t0_ms() -> Option<f64> {
    let t0: u128 = std::env::var("SPIKE_T0").ok()?.parse().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some((now.saturating_sub(t0)) as f64 / 1e6)
}

fn rss_mb() -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|k| k.parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn fonts() -> Vec<Cow<'static, [u8]>> {
    let dir = repo().join("apps/desktop/assets/fonts");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("fonts dir").flatten() {
        let p = e.path();
        if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Inter") && n.ends_with(".ttf")) {
            out.push(Cow::Owned(std::fs::read(&p).expect("font")));
        }
    }
    out
}

fn surface250() -> (Vec<FlatComponent>, f32, f32) {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("apps/desktop/spikes-zgui/surface250.json")).unwrap()).unwrap();
    let comps = serde_json::from_value(v["components"].clone()).unwrap();
    (comps, v["viewport"]["width"].as_f64().unwrap() as f32, v["viewport"]["height"].as_f64().unwrap() as f32)
}

fn list_components() -> Vec<FlatComponent> {
    serde_json::from_value(json!([
        {"id": "root", "component": "List", "direction": "vertical", "divided": true, "style": {"height": 800},
         "children": {"componentId": "bench_row", "path": "/rows", "key": "id"}},
        {"id": "bench_row", "component": "Row", "title": {"@path": "title"}, "meta": {"@path": "meta"}, "pressable": true}
    ]))
    .unwrap()
}

struct NoHost;
impl HostPlugin for NoHost {}

fn options(w: f32, id: &str) -> SurfaceViewOptions {
    SurfaceViewOptions {
        surface_id: id.into(),
        theme: eui_core::themes::builtin_theme("exponential"),
        mode: Mode::Dark,
        host: Rc::new(NoHost),
        width: Some(w),
        ..Default::default()
    }
}

fn context(renderer: bool) -> HeadlessAppContext {
    let text = gpui_platform::current_platform(true).text_system();
    let mut cx = if renderer {
        HeadlessAppContext::with_platform(text, Arc::new(()), gpui_platform::current_headless_renderer)
    } else {
        HeadlessAppContext::with_platform(text, Arc::new(()), || Ok(None))
    };
    cx.update(|cx| {
        let _ = cx.text_system().add_fonts(fonts());
        gpui_component::init(cx);
    });
    cx
}

type Frame<'a> = &'a mut dyn FnMut(&mut SurfaceView, &mut gpui::Window, &mut gpui::Context<SurfaceView>);

fn frame(cx: &mut HeadlessAppContext, window: gpui::WindowHandle<SurfaceView>, f: Frame) {
    window
        .update(cx, |view, window, cx| {
            f(view, window, cx);
            view.layout_now(window, cx);
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx)).unwrap();
}

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.sort_by(f64::total_cmp);
    let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).min(v.len()) - 1];
    (mean, p95, v[v.len() / 2])
}

fn bench(part: &str) {
    if part == "list" { return benchlist(); }
    let t_main = since_t0_ms();
    let (comps, w, h) = surface250();
    let started = Instant::now();
    let mut cx = context(false);
    let opts = options(w, "s250");
    let window = cx.open_window(size(px(w), px(h)), move |window, cx| cx.new(|cx| SurfaceView::new(opts, window, cx))).unwrap();
    frame(&mut cx, window, &mut |view, _, cx| {
        view.set_components(comps.clone(), cx);
        view.set_viewport_height(h, cx);
    });
    let first_in_process = started.elapsed().as_secs_f64() * 1000.0;
    let first_from_t0 = since_t0_ms();
    let rss_first = rss_mb();

    // Delta: ONE text prop of one Text in the 250-component surface, as an
    // updateComponents payload, → the next frame done. 20 warm-up, 200 timed.
    let mut deltas = Vec::new();
    for k in 0..220 {
        let c: FlatComponent = serde_json::from_value(json!({"id": "t17", "component": "Text", "text": format!("Item 17 changed {k}"), "variant": "caption"})).unwrap();
        let t = Instant::now();
        frame(&mut cx, window, &mut |view, _, cx| {
            view.update_components(vec![c.clone()], cx);
        });
        if k >= 20 {
            deltas.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let (delta_mean, delta_p95, delta_med) = stats(deltas);

    println!(
        "{}",
        json!({
            "renderer": "gpui", "t0_to_main_ms": t_main, "first_frame_in_process_ms": first_in_process, "first_frame_from_t0_ms": first_from_t0,
            "rss_after_first_frame_mb": rss_first,
            "delta_mean_ms": delta_mean, "delta_p95_ms": delta_p95, "delta_median_ms": delta_med
        })
    );
    std::mem::forget(cx);
}

fn benchlist() {
    let mut cx = context(false);
    // Scroll: a 10k-row List (bench-list.json's shape), 390 × 800, 100 steps of one viewport.
    let (lw, lh) = (390.0_f32, 800.0_f32);
    let rows: Vec<Value> = (0..10_000).map(|i| json!({"id": format!("r{i}"), "title": format!("Row {i}"), "meta": (i % 97).to_string()})).collect();
    let lopts = options(lw, "list");
    let lwin = cx.open_window(size(px(lw), px(lh)), move |window, cx| cx.new(|cx| SurfaceView::new(lopts, window, cx))).unwrap();
    let t = Instant::now();
    frame(&mut cx, lwin, &mut |view, _, cx| {
        view.set_data("/rows", Some(Value::Array(rows.clone())), cx).unwrap();
        view.set_components(list_components(), cx);
        view.set_viewport_height(lh, cx);
    });
    let list_first = t.elapsed().as_secs_f64() * 1000.0;
    let rss_list_first = rss_mb();
    let mut steps = Vec::new();
    for k in 1..=100 {
        let t = Instant::now();
        frame(&mut cx, lwin, &mut |view, _, _| {
            view.surface_mut().scroll("root", k as f32 * lh);
        });
        steps.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let (scroll_mean, scroll_p95, scroll_med) = stats(steps);
    let rss_scrolled = rss_mb();
    println!("{}", json!({"renderer": "gpui", "list_first_frame_ms": list_first, "rss_after_list_first_mb": rss_list_first,
        "scroll_mean_ms": scroll_mean, "scroll_p95_ms": scroll_p95, "scroll_median_ms": scroll_med, "rss_after_scroll_mb": rss_scrolled}));
    std::mem::forget(cx);
}

fn shot(out: &str) {
    let (comps, w, h) = surface250();
    let mut cx = context(true);
    let opts = options(w, "s250");
    let window = cx.open_window(size(px(w), px(h)), move |window, cx| cx.new(|cx| SurfaceView::new(opts, window, cx))).unwrap();
    for i in 0..3 {
        cx.run_until_parked();
        frame(&mut cx, window, &mut |view, window, cx| {
            if i == 0 {
                view.set_components(comps.clone(), cx);
                view.set_viewport_height(h, cx);
            }
            window.refresh();
        });
    }
    let img = cx.capture_screenshot(window.into()).unwrap();
    img.save(out).unwrap();
    eprintln!("gpui shot {}x{} → {out}", img.width(), img.height());
    std::mem::forget(cx);
}

/// A real window on the live display; exits by itself after `secs`.
fn window_mode(secs: u64) {
    use gpui::{div, point, prelude::*, App, Bounds, WindowBounds, WindowOptions};
    let (comps, w, h) = surface250();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        std::process::exit(0);
    });
    struct Shell {
        view: gpui::Entity<SurfaceView>,
        first: bool,
    }
    impl Render for Shell {
        fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            if self.first {
                self.first = false;
                eprintln!("SPIKE first_render_from_t0_ms {:?}", since_t0_ms());
            }
            div().size_full().bg(gpui::rgb(0x0c0c0f)).child(self.view.clone())
        }
    }
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(fonts());
        let bounds = Bounds { origin: point(px(0.0), px(0.0)), size: size(px(w.min(900.0)), px(h.min(900.0))) };
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), focus: false, ..Default::default() }, move |window, cx| {
            let view = cx.new(|cx| {
                let mut v = SurfaceView::new(options(w.min(900.0), "win"), window, cx);
                v.set_components(comps, cx);
                v
            });
            cx.new(|_| Shell { view, first: true })
        })
        .unwrap();
    });
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") => bench(args.get(1).map(String::as_str).unwrap_or("250")),
        Some("shot") => shot(args.get(1).map(String::as_str).unwrap_or("gpui.png")),
        Some("window") => window_mode(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(25)),
        _ => eprintln!("usage: spike_gpui bench | shot <png> | window <secs>"),
    }
}
