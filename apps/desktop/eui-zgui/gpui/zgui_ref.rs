//! #EDE-279: the eui-gpui half of the zgui trial (the reference). NOT a
//! production file: copied into `apps/desktop/crates/eui-shot/examples/`
//! for the run only (eui-shot links gpui's headless renderer under
//! test-support and the IDE's icon rule), never committed there.
//!
//! ```text
//! cp apps/desktop/eui-zgui/gpui/zgui_ref.rs apps/desktop/crates/eui-shot/examples/
//! cargo build --release -p eui-shot --example zgui_ref
//! zgui_ref shots <dir>                        # the 20 trial specimens + <dir>/sizes.json
//! zgui_ref shot <id> <out.png> [WxH]
//! SPIKE_T0=$(date +%s%N) zgui_ref bench250    # cold start, one-prop delta, RSS
//! zgui_ref benchlist                          # 10k fixed rows, 100 viewports
//! zgui_ref benchvar                           # 10k chat rows of 1-8 wrapped lines, 100 viewports
//! zgui_ref window 15                          # real window, exits after 15 s
//! ```
//!
//! The workloads are `apps/desktop/eui-zgui/src/bench.rs`'s (the same
//! flat components / data); a frame = `layout_now` + gpui's `Window::draw`,
//! headless, no GPU submit (as `eui-gpui/examples/bench_list.rs`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use eui_core::theme::Mode;
use eui_core::types::FlatComponent;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{px, size, AppContext as _, HeadlessAppContext, SharedString};
use serde_json::{json, Value};

const TRIAL: [&str; 20] = [
    "exponential-ui-text", "exponential-ui-heading", "exponential-ui-button", "exponential-ui-badge", "exponential-ui-row", "exponential-ui-list", "exponential-ui-card", "exponential-ui-stack", "exponential-ui-grid", "exponential-ui-input",
    "exponential-ui-textarea", "exponential-ui-checkbox", "exponential-ui-switch", "exponential-ui-select", "exponential-ui-tabs", "exponential-ui-dialog", "exponential-ui-markdown", "exponential-ui-code-block", "exponential-ui-progress", "exponential-ui-icon",
];

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
    let mut out = Vec::new();
    for dir in ["apps/desktop/assets/fonts", "apps/desktop/crates/eui-gpui/assets/fonts"] {
        for e in std::fs::read_dir(repo().join(dir)).into_iter().flatten().flatten() {
            if e.path().extension().is_some_and(|x| x == "ttf") {
                out.push(Cow::Owned(std::fs::read(e.path()).expect("font")));
            }
        }
    }
    out
}

/// The IDE's icon rule (as `exponential-shot`): concept → lucide SVG asset.
struct Icons(HashMap<String, String>);
impl HostPlugin for Icons {
    fn icon(&self, name: &str) -> Option<SharedString> {
        let lucide = self.0.get(name).map(String::as_str).unwrap_or(name.strip_prefix("ui-").unwrap_or(name));
        let path = format!("icons/{lucide}.svg");
        repo().join("apps/desktop/assets").join(&path).exists().then(|| SharedString::from(path))
    }
}
fn icons() -> Icons {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("packages/icons/icons.json")).unwrap()).unwrap();
    Icons(v["semantic"].as_object().map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default())
}

struct Assets;
impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(std::fs::read(repo().join("apps/desktop/assets").join(path)).ok().map(Cow::Owned))
    }
    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

fn options(w: f32, id: &str) -> SurfaceViewOptions {
    SurfaceViewOptions { surface_id: id.into(), theme: eui_core::themes::builtin_theme("exponential"), mode: Mode::Dark, host: Rc::new(icons()), width: Some(w), ..Default::default() }
}

fn context(renderer: bool) -> HeadlessAppContext {
    let text = gpui_platform::current_platform(true).text_system();
    let mut cx = if renderer { HeadlessAppContext::with_platform(text, Arc::new(Assets), gpui_platform::current_headless_renderer) } else { HeadlessAppContext::with_platform(text, Arc::new(Assets), || Ok(None)) };
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

fn stats(mut v: Vec<f64>) -> (f64, f64, f64, f64) {
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.sort_by(f64::total_cmp);
    let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).min(v.len()) - 1];
    (mean, p95, v[v.len() - 1], v[v.len() / 2])
}

fn specimen(id: &str) -> (eui_core::types::NestedNode, Vec<(String, Vec<String>)>) {
    let all: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("packages/exponential-ui/fixtures/specimens.json")).unwrap()).unwrap();
    let e = all["specimens"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap().clone();
    let states: Vec<(String, Vec<String>)> = e.get("states").and_then(|s| s.as_object()).map(|m| m.iter().map(|(k, v)| (k.clone(), serde_json::from_value(v.clone()).unwrap_or_default())).collect()).unwrap_or_default();
    (serde_json::from_value(e["node"].clone()).unwrap(), states)
}

/// The content height of a specimen at width `w` (a measure pass, no paint).
fn content_height(id: &str, w: f32) -> f32 {
    let (node, states) = specimen(id);
    let mut cx = context(false);
    let opts = options(w, "h");
    let window = cx.open_window(size(px(w), px(4000.0)), move |window, cx| cx.new(|cx| SurfaceView::new(opts, window, cx))).unwrap();
    let mut h = 0.0;
    frame(&mut cx, window, &mut |view, _, cx| {
        view.set_forced_states(states.clone(), cx);
        view.set_nested(node.clone(), cx);
    });
    window.update(&mut cx, |view, _, _| h = view.surface_height()).unwrap();
    std::mem::forget(cx);
    h.ceil().max(200.0)
}

/// The page behind the surface: the theme's `background` (as `exponential-shot`'s Frame).
struct Page {
    view: gpui::Entity<SurfaceView>,
}
impl gpui::Render for Page {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::{div, ParentElement as _, Styled as _};
        let bg = eui_core::themes::builtin_theme("exponential").and_then(|t| t.modes.get(Mode::Dark).color.get("background").cloned()).and_then(|h| eui_gpui::paint::color::parse_hex(&h)).unwrap_or(gpui::black());
        div().size_full().bg(bg).child(self.view.clone())
    }
}

fn shot(id: &str, out: &str, w: f32, h: f32) {
    let (node, states) = specimen(id);
    let mut cx = context(true);
    let opts = options(w, "shot");
    let window = cx.open_window(size(px(w), px(h)), move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(opts, window, cx));
        cx.new(|_| Page { view })
    }).unwrap();
    for i in 0..3 {
        cx.run_until_parked();
        window
            .update(&mut cx, |page, window, cx| {
                page.view.update(cx, |view, cx| {
                    if i == 0 {
                        view.set_forced_states(states.clone(), cx);
                        view.set_nested(node.clone(), cx);
                        view.set_viewport_height(h, cx);
                    }
                    view.layout_now(window, cx);
                    cx.notify();
                });
                window.refresh();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx)).unwrap();
    }
    let img = cx.capture_screenshot(window.into()).unwrap();
    img.save(out).unwrap();
    eprintln!("gpui {id} {}x{} → {out}", img.width(), img.height());
    std::mem::forget(cx);
}

// ---------------------------------------------------------------- benches

fn surface250() -> (Vec<FlatComponent>, f32, f32) {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("apps/desktop/spikes-zgui/surface250.json")).unwrap()).unwrap();
    (serde_json::from_value(v["components"].clone()).unwrap(), v["viewport"]["width"].as_f64().unwrap() as f32, v["viewport"]["height"].as_f64().unwrap() as f32)
}

/// The bench workloads, byte-identical to eui-zgui's `bench.rs`.
fn list_components(variable: bool) -> Vec<FlatComponent> {
    let row = if variable {
        json!({"id": "bench_row", "component": "Box", "style": {"padding": 12, "borderRadius": 12, "backgroundColor": "$color.muted", "marginBottom": 8}, "children": ["bench_text"]})
    } else {
        json!({"id": "bench_row", "component": "Row", "title": {"@path": "title"}, "meta": {"@path": "meta"}, "pressable": true})
    };
    let mut c = vec![json!({"id": "root", "component": "List", "direction": "vertical", "divided": !variable, "style": {"height": 800}, "children": {"componentId": "bench_row", "path": "/rows", "key": "id"}}), row];
    if variable {
        c.push(json!({"id": "bench_text", "component": "Text", "text": {"@path": "text"}}));
    }
    serde_json::from_value(Value::Array(c)).unwrap()
}

const WORDS: [&str; 16] = ["the", "renderer", "paints", "every", "frame", "from", "eui-core", "a", "chat", "message", "wraps", "onto", "several", "lines", "when", "long"];

fn rows(variable: bool) -> Value {
    Value::Array(
        (0..10_000)
            .map(|i: usize| {
                if variable {
                    // 1..=8 wrapped lines at 390 px: ~9 words a line.
                    let lines = 1 + (i * 7919) % 8;
                    let n = lines * 9 - 4;
                    let text: Vec<&str> = (0..n).map(|k| WORDS[(i + k * 3) % WORDS.len()]).collect();
                    json!({"id": format!("r{i}"), "text": format!("#{i} {}", text.join(" "))})
                } else {
                    json!({"id": format!("r{i}"), "title": format!("Row {i}"), "meta": (i % 97).to_string()})
                }
            })
            .collect(),
    )
}

fn bench250() {
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
    let (dm, dp, dx, dmed) = stats(deltas);
    println!("{}", json!({"renderer": "gpui", "t0_to_main_ms": t_main, "first_frame_in_process_ms": first_in_process, "first_frame_from_t0_ms": first_from_t0, "rss_after_first_frame_mb": rss_first, "delta_mean_ms": dm, "delta_p95_ms": dp, "delta_max_ms": dx, "delta_median_ms": dmed}));
    std::mem::forget(cx);
}

fn benchlist(variable: bool, step: f32) {
    let mut cx = context(false);
    let (lw, lh) = (390.0_f32, 800.0_f32);
    let data = rows(variable);
    let lopts = options(lw, "list");
    let lwin = cx.open_window(size(px(lw), px(lh)), move |window, cx| cx.new(|cx| SurfaceView::new(lopts, window, cx))).unwrap();
    let t = Instant::now();
    frame(&mut cx, lwin, &mut |view, _, cx| {
        view.set_data("/rows", Some(data.clone()), cx).unwrap();
        view.set_components(list_components(variable), cx);
        view.set_viewport_height(lh, cx);
    });
    let list_first = t.elapsed().as_secs_f64() * 1000.0;
    let rss_list_first = rss_mb();
    let mut steps = Vec::new();
    let n = (100.0 * lh / step) as usize;
    for k in 1..=n {
        let t = Instant::now();
        frame(&mut cx, lwin, &mut |view, _, _| {
            view.surface_mut().scroll("root", k as f32 * step);
        });
        steps.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let mut reached = 0.0;
    lwin.update(&mut cx, |view, _, _| reached = view.surface_mut().scroll_offset("root").1).unwrap();
    let (m, p, x, med) = stats(steps);
    println!("{}", json!({"renderer": "gpui", "variable": variable, "step_px": step, "list_first_frame_ms": list_first, "rss_after_list_first_mb": rss_list_first, "scroll_mean_ms": m, "scroll_p95_ms": p, "scroll_max_ms": x, "scroll_median_ms": med, "scroll_reached_px": reached, "rss_after_scroll_mb": rss_mb()}));
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
                eprintln!("REF first_render_from_t0_ms {:?}", since_t0_ms());
            }
            div().size_full().bg(gpui::rgb(0x0a0a0a)).child(self.view.clone())
        }
    }
    gpui_platform::application().with_assets(Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(fonts());
        let bounds = Bounds { origin: point(px(0.0), px(0.0)), size: size(px(w), px(h.min(900.0))) };
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), focus: false, ..Default::default() }, move |window, cx| {
            let view = cx.new(|cx| {
                let mut v = SurfaceView::new(options(w, "win"), window, cx);
                v.set_components(comps, cx);
                v
            });
            cx.new(|_| Shell { view, first: true })
        })
        .unwrap();
    });
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("shots") => {
            let dir = a.get(1).cloned().unwrap_or("shots-gpui".into());
            std::fs::create_dir_all(&dir).unwrap();
            let mut sizes = serde_json::Map::new();
            for id in TRIAL {
                let h = content_height(id, 720.0);
                sizes.insert(id.into(), json!([720.0, h]));
                let st = std::process::Command::new(std::env::current_exe().unwrap()).args(["shot", id, &format!("{dir}/{id}.png"), &format!("720x{h}")]).status().unwrap();
                if !st.success() {
                    eprintln!("gpui {id} FAILED");
                }
            }
            std::fs::write(format!("{dir}/sizes.json"), serde_json::to_string_pretty(&Value::Object(sizes)).unwrap()).unwrap();
        }
        Some("shot") => {
            let (w, h) = a.get(3).and_then(|s| s.split_once('x')).map(|(w, h)| (w.parse().unwrap(), h.parse().unwrap())).unwrap_or((720.0, 0.0));
            let h = if h > 0.0 { h } else { content_height(&a[1], w) };
            shot(&a[1], &a[2], w, h)
        }
        Some("bench250") => bench250(),
        Some("benchlist") => benchlist(false, 800.0),
        Some("benchvar") => benchlist(true, 800.0),
        Some("smoothlist") => benchlist(false, 40.0),
        Some("smoothvar") => benchlist(true, 40.0),
        Some("window") => window_mode(a.get(1).and_then(|s| s.parse().ok()).unwrap_or(15)),
        _ => eprintln!("usage: zgui_ref shots <dir> | shot <id> <out> [WxH] | bench250 | benchlist | benchvar | window <secs>"),
    }
}
