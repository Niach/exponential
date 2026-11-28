//! gpui → zgui spike, the zgui half. The SAME flat A2UI components as the gpui
//! half (`../surface250.json`: Box, Text, Row, Button) mapped to zgui
//! elements by a hand-written mapper, ONE `RwSignal` per bound string prop,
//! so an `updateComponents` delta is a signal write. The 10k list is
//! `zgui_ui::virtualize::VirtualList` (zgui ships one).
//!
//! ```text
//! SPIKE_T0=$(date +%s%N) target/release/zspike bench250     # cold start + delta + RSS
//! target/release/zspike benchlist                           # 10k-row scroll + RSS
//! target/release/zspike shot ../shots/zgui.ppm              # wgpu offscreen at 2x
//! target/release/zspike window 25                           # real window, exits after 25 s
//! ```
//!
//! A bench `frame` = `Harness::settle`: reactive flush, restyle, layout,
//! paint into the display list, handed to a renderer that drops it (no GPU
//! submit), the same cut as the gpui half's `Window::draw`. Scale factor 2.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use zgui::app::Fonts;
use zgui::geom::{Device, DevicePx, Point, Scale, Size};
use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, AnyView, BuildCx, ScrollBehavior, ScrollTarget};
use zgui_platform_headless::Harness;
use zgui_ui::virtualize::{VirtualList, VirtualListProps};

const SHEET: &str = "
:root { background-color: #0a0a0a; color: #fafafa; font-family: Inter; font-size: 14px }
.root { flex-wrap: wrap; gap: 8px; padding: 16px; width: 100%; background-color: #0a0a0a }
.title { width: 100%; font-size: 18px; font-weight: 600 }
.card { gap: 4px; width: 196px; padding: 8px; border: 1px solid #ffffff1a; border-radius: 8px }
.caption { font-size: 12px }
.erow { height: 36px; align-items: center; justify-content: space-between; padding-left: 12px; padding-right: 12px; border-radius: 6px }
.meta { color: #a1a1a1 }
.button { height: 28px; align-items: center; justify-content: center; border: 1px solid #ffffff26; background-color: #ffffff0f; border-radius: 8px; font-weight: 500 }
.list { height: 800px; width: 390px }
.lrow { height: 40px; align-items: center; justify-content: space-between; padding-left: 12px; padding-right: 12px; border-bottom: 1px solid #ffffff1a }
";

const SCALE: f64 = 2.0;

fn since_t0_ms() -> Option<f64> {
    let t0: u128 = std::env::var("SPIKE_T0").ok()?.parse().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some(now.saturating_sub(t0) as f64 / 1e6)
}

fn rss_mb() -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|k| k.parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
}

fn here() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.sort_by(f64::total_cmp);
    let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).min(v.len()) - 1];
    (mean, p95, v[v.len() / 2])
}

/// The system faces plus the conformance Inter files the gpui half loads.
fn fonts() -> Fonts {
    let fonts = Fonts::system();
    let dir = here().join("../assets/fonts");
    for e in std::fs::read_dir(&dir).expect("fonts dir").flatten() {
        let p = e.path();
        if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Inter") && n.ends_with(".ttf")) {
            let bytes: Vec<u8> = std::fs::read(&p).expect("font");
            fonts.register(Arc::new(bytes), None).expect("Inter registers");
        }
    }
    fonts
}

// ---------------------------------------------------------------- the mapper

/// The surface: flat components by id, a signal per bound string prop.
struct Model {
    components: HashMap<String, Value>,
    signals: HashMap<(String, &'static str), RwSignal<String, LocalStorage>>,
}

const BOUND: [&str; 4] = ["text", "title", "meta", "label"];

impl Model {
    fn load() -> (Self, f32, f32) {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(here().join("surface250.json")).unwrap()).unwrap();
        let mut components = HashMap::new();
        let mut signals = HashMap::new();
        for c in v["components"].as_array().unwrap() {
            let id = c["id"].as_str().unwrap().to_string();
            for prop in BOUND {
                if let Some(s) = c[prop].as_str() {
                    signals.insert((id.clone(), prop), RwSignal::new_local(s.to_string()));
                }
            }
            components.insert(id, c.clone());
        }
        (Self { components, signals }, v["viewport"]["width"].as_f64().unwrap() as f32, v["viewport"]["height"].as_f64().unwrap() as f32)
    }

    fn sig(&self, id: &str, prop: &'static str) -> RwSignal<String, LocalStorage> {
        self.signals[&(id.to_string(), prop)]
    }

    fn build(&self, id: &str) -> AnyView {
        let c = &self.components[id];
        let kids = || -> Vec<AnyView> { c["children"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(|k| self.build(k)).collect()).unwrap_or_default() };
        match c["component"].as_str().unwrap() {
            "Box" => {
                let class = if id == "root" { "root" } else { "card" };
                if c["style"]["flexDirection"] == "row" {
                    AnyView::new(zgui::elements::row().class(class).children(kids()))
                } else {
                    AnyView::new(zgui::elements::column().class(class).children(kids()))
                }
            }
            "Text" => {
                let s = self.sig(id, "text");
                let class = if c["variant"] == "title" { "title" } else { "caption" };
                AnyView::new(zgui::elements::text().class(class).child(move || s.get()))
            }
            "Row" => {
                let (t, m) = (self.sig(id, "title"), self.sig(id, "meta"));
                AnyView::new(
                    zgui::elements::row()
                        .class("erow")
                        .child(zgui::elements::text().child(move || t.get()))
                        .child(zgui::elements::text().class("meta").child(move || m.get())),
                )
            }
            "Button" => {
                let l = self.sig(id, "label");
                AnyView::new(zgui::elements::control().class("button").child(zgui::elements::text().child(move || l.get())))
            }
            other => AnyView::new(format!("unknown {other}")),
        }
    }

    /// An `updateComponents` payload: each bound prop that changed = one signal write.
    fn update(&self, component: &Value) {
        let id = component["id"].as_str().unwrap();
        for prop in BOUND {
            if let (Some(new), Some(sig)) = (component[prop].as_str(), self.signals.get(&(id.to_string(), prop))) {
                if sig.with_untracked(|old| old != new) {
                    sig.set(new.to_string());
                }
            }
        }
    }
}

// ------------------------------------------------------------- renderers

struct NullSink;
impl zgui::atlas::TextureSink for NullSink {
    fn create_texture(&mut self, _: zgui::atlas::TextureId, _: Size<i32, Device>, _: zgui::atlas::TextureFormat) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }
    fn write_texture(&mut self, _: zgui::atlas::TextureId, _: zgui::geom::Rect<i32, Device>, _: zgui::atlas::TextureFormat, _: &[u8]) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }
    fn destroy_texture(&mut self, _: zgui::atlas::TextureId) {}
}

/// Accepts a frame and drops it: the number is the pipeline's, as gpui's headless `draw`.
struct Nowhere {
    target: Option<RenderTarget>,
    sink: NullSink,
}
impl Renderer for Nowhere {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }
    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }
    fn target(&self) -> Option<RenderTarget> {
        self.target
    }
    fn draw(&mut self, _: &zgui::scene::Scene, _: &zgui::bits::DamageSet) -> zgui::render::FrameOutcome {
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }
    fn register_external(&mut self, _: zgui::render::ExternalTexture) -> zgui::render::TextureHandle {
        zgui::render::TextureHandle(0)
    }
    fn release_external(&mut self, _: zgui::render::TextureHandle) {}
    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }
    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.sink
    }
}

type Readback = Arc<Mutex<Option<zgui_render_wgpu::Pixels>>>;

/// The real wgpu renderer into a texture, keeping a copy of the presented frame.
struct Recording {
    inner: zgui_render_wgpu::WgpuRenderer,
    into: Readback,
}
impl Renderer for Recording {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.inner.capabilities()
    }
    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }
    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }
    fn draw(&mut self, scene: &zgui::scene::Scene, damage: &zgui::bits::DamageSet) -> zgui::render::FrameOutcome {
        let out = self.inner.draw(scene, damage);
        if let Some(p) = self.inner.read_presented() {
            *self.into.lock().unwrap() = Some(p);
        }
        out
    }
    fn register_external(&mut self, t: zgui::render::ExternalTexture) -> zgui::render::TextureHandle {
        self.inner.register_external(t)
    }
    fn release_external(&mut self, h: zgui::render::TextureHandle) {
        self.inner.release_external(h);
    }
    fn memory(&self) -> zgui::render::MemoryReport {
        self.inner.memory()
    }
    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.inner.texture_sink()
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.inner.as_any_mut()
    }
}

fn runtime(w: f32, h: f32, renderer: zgui::runtime::RendererFactory, build: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static) -> Runtime {
    let fonts = fonts();
    let (metrics, shaping, raster) = (fonts.clone(), fonts.clone(), fonts.clone());
    zgui::runtime::App::new()
        .with_title("zspike")
        .with_size(w, h)
        .with_stylesheet(SHEET)
        .with_renderer(renderer)
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(build)
        .expect("the reactive runtime installs")
}

fn nowhere() -> zgui::runtime::RendererFactory {
    Box::new(|_: &Arc<dyn Surface>, target: RenderTarget| -> Result<Box<dyn Renderer>, AppError> { Ok(Box::new(Nowhere { target: Some(target), sink: NullSink })) })
}

fn open(runtime: Runtime, w: f32, h: f32) -> Harness<Runtime> {
    let mut harness = Harness::new(runtime);
    let size = Size::new(DevicePx((w as f64 * SCALE) as f32), DevicePx((h as f64 * SCALE) as f32));
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged { scale_factor: SCALE, size });
    harness.settle(512);
    harness
}

// ------------------------------------------------------------------ modes

fn bench250() {
    let t_main = since_t0_ms();
    let started = Instant::now();
    let (model, w, h) = Model::load();
    let model = std::rc::Rc::new(model);
    let m = model.clone();
    let rt = runtime(w, h, nowhere(), move |cx| Box::new(m.build("root").build(cx)));
    let mut harness = open(rt, w, h);
    let first_in_process = started.elapsed().as_secs_f64() * 1000.0;
    let first_from_t0 = since_t0_ms();
    let rss_first = rss_mb();
    let frames_first = harness.frames_requested();

    let mut deltas = Vec::new();
    let mut frames = 0;
    for k in 0..220 {
        let c = json!({"id": "t17", "component": "Text", "text": format!("Item 17 changed {k}"), "variant": "caption"});
        let t = Instant::now();
        model.update(&c);
        frames += harness.settle(64);
        if k >= 20 {
            deltas.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let (dm, dp, dmed) = stats(deltas);
    println!(
        "{}",
        json!({"renderer": "zgui", "t0_to_main_ms": t_main, "first_frame_in_process_ms": first_in_process, "first_frame_from_t0_ms": first_from_t0,
               "frames_to_first_settle": frames_first, "rss_after_first_frame_mb": rss_first,
               "delta_mean_ms": dm, "delta_p95_ms": dp, "delta_median_ms": dmed, "delta_frames": frames})
    );
    std::mem::forget(harness);
}

fn benchlist() {
    let (lw, lh) = (390.0_f32, 800.0_f32);
    let port = NodeRef::new();
    let started = Instant::now();
    let rt = runtime(lw, lh, nowhere(), move |cx| {
        let count = RwSignal::new_local(10_000_usize);
        Box::new(
            view! {
                VirtualList(
                    count = count,
                    row_size = 40.0,
                    node_ref = port,
                    class = "list",
                    row = move |i: usize| view! { row(class = "lrow") {
                        text {{move || format!("Row {i}")}}
                        text(class = "meta") {{move || format!("{}", i % 97)}}
                    } }
                )
            }
            .into_view()
            .build(cx),
        )
    });
    let mut harness = open(rt, lw, lh);
    let list_first = started.elapsed().as_secs_f64() * 1000.0;
    let rss_first = rss_mb();
    let mut steps = Vec::new();
    let mut frames = 0;
    for k in 1..=100 {
        let y = (k as f64 * lh as f64 * SCALE) as f32;
        let t = Instant::now();
        port.scroll_to(ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(y))), ScrollBehavior::Instant);
        // scroll_to is carried out after the next frame's layout: ask for one.
        harness.advance(Duration::from_micros(16_667));
        frames += harness.settle(64);
        steps.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let reached = port.scroll_offset().y.0 / SCALE as f32;
    let (sm, sp, smed) = stats(steps);
    println!(
        "{}",
        json!({"renderer": "zgui", "list_first_frame_ms": list_first, "rss_after_list_first_mb": rss_first,
               "scroll_mean_ms": sm, "scroll_p95_ms": sp, "scroll_median_ms": smed, "scroll_frames": frames,
               "scroll_reached_css_px": reached, "rss_after_scroll_mb": rss_mb()})
    );
    std::mem::forget(harness);
}

fn shot(out: &str) {
    let (model, w, h) = Model::load();
    let readback: Readback = Arc::new(Mutex::new(None));
    let into = readback.clone();
    let factory: zgui::runtime::RendererFactory = Box::new(move |surface: &Arc<dyn Surface>, _target| {
        let size = surface.size();
        let target = RenderTarget::new(Size::new(size.width.0 as i32, size.height.0 as i32), Scale::new(surface.scale_factor() as f32));
        let inner = zgui_render_wgpu::Builder::new()
            .offscreen(target, zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm, false)
            .map_err(AppError::GpuUnavailable)?;
        Ok(Box::new(Recording { inner, into: into.clone() }) as Box<dyn Renderer>)
    });
    let rt = runtime(w, h, factory, move |cx| Box::new(model.build("root").build(cx)));
    let mut harness = open(rt, w, h);
    for _ in 0..4 {
        harness.advance(Duration::from_millis(100));
        harness.settle(64);
    }
    let pixels = readback.lock().unwrap().take().expect("a frame was drawn");
    let (dw, dh) = ((w as f64 * SCALE) as i32, (h as f64 * SCALE) as i32);
    let mut ppm = format!("P6\n{dw} {dh}\n255\n").into_bytes();
    for y in 0..dh {
        for x in 0..dw {
            let p = pixels.rgba(x, y);
            ppm.extend_from_slice(&p[..3]);
        }
    }
    std::fs::write(out, ppm).unwrap();
    eprintln!("zgui shot {dw}x{dh} → {out}");
    std::mem::forget(harness);
}

/// A real window on the live display (Wayland if there is one, else winit/X11); exits by itself.
fn window(secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        std::process::exit(0);
    });
    let (model, w, h) = Model::load();
    let _ = (w, h);
    zgui::app()
        .with_title("zspike")
        .with_fonts(fonts())
        .with_stylesheet(SHEET)
        .run(move || {
            eprintln!("SPIKE first_build_from_t0_ms {:?}", since_t0_ms());
            model.build("root")
        })
        .expect("the window runs");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench250") => bench250(),
        Some("benchlist") => benchlist(),
        Some("shot") => shot(args.get(1).map(String::as_str).unwrap_or("zgui.ppm")),
        Some("window") => window(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(25)),
        _ => eprintln!("usage: zspike bench250 | benchlist | shot <ppm> | window <secs>"),
    }
}
