//! The window host: zgui's runtime around one root view, run headless
//! (`zgui-platform-headless`, a renderer that drops the frame or the real
//! wgpu renderer into a texture + readback) or in a real window (winit /
//! Wayland, Metal on macOS). No platform-specific code here.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use zgui::app::Fonts;
use zgui::geom::{Device, DevicePx, Scale, Size};
use zgui::platform::{Surface, SurfaceEvent};
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, AnyView, BuildCx, View};
use zgui_platform_headless::Harness;

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

/// Accepts a frame and drops it (the bench cut: the CPU pipeline, as gpui's headless `Window::draw`).
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

pub type Readback = Arc<Mutex<Option<zgui_render_wgpu::Pixels>>>;

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

pub enum Paint {
    /// Drop every frame (benches).
    Nowhere,
    /// wgpu offscreen with a readback of the last presented frame.
    Offscreen(Readback),
}

const SHEET: &str = ":root { margin: 0; padding: 0 }";

/// A runtime over `build` (the root view) with the eUI fonts.
pub fn runtime(fonts: &Fonts, w: f32, h: f32, paint: Paint, build: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static) -> Runtime {
    let factory: zgui::runtime::RendererFactory = match paint {
        Paint::Nowhere => Box::new(|_: &Arc<dyn Surface>, target: RenderTarget| -> Result<Box<dyn Renderer>, AppError> { Ok(Box::new(Nowhere { target: Some(target), sink: NullSink })) }),
        Paint::Offscreen(into) => Box::new(move |surface: &Arc<dyn Surface>, _target| {
            let size = surface.size();
            let target = RenderTarget::new(Size::new(size.width.0 as i32, size.height.0 as i32), Scale::new(surface.scale_factor() as f32));
            let inner = zgui_render_wgpu::Builder::new().offscreen(target, zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm, false).map_err(AppError::GpuUnavailable)?;
            Ok(Box::new(Recording { inner, into: into.clone() }) as Box<dyn Renderer>)
        }),
    };
    let (metrics, shaping, raster) = (fonts.clone(), fonts.clone(), fonts.clone());
    zgui::runtime::App::new()
        .with_title("eui-zgui")
        .with_size(w, h)
        .with_stylesheet(SHEET)
        .with_renderer(factory)
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(build)
        .expect("the reactive runtime installs")
}

/// A headless harness at `scale`, first frame settled.
pub fn open(runtime: Runtime, w: f32, h: f32, scale: f64) -> Harness<Runtime> {
    let mut harness = Harness::new(runtime);
    let size = Size::new(DevicePx((w as f64 * scale) as f32), DevicePx((h as f64 * scale) as f32));
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged { scale_factor: scale, size });
    harness.settle(512);
    harness
}

/// One frame: advance the virtual clock a vsync and settle.
pub fn frame(harness: &mut Harness<Runtime>) -> u64 {
    harness.advance(Duration::from_micros(16_667));
    harness.settle(64)
}

/// Mount `view` headless and offscreen, settle, read back RGBA at `scale`.
pub fn shot(fonts: &Fonts, w: f32, h: f32, scale: f64, view: impl Fn() -> AnyView + 'static) -> image::RgbaImage {
    let readback: Readback = Arc::new(Mutex::new(None));
    let rt = runtime(fonts, w, h, Paint::Offscreen(readback.clone()), move |cx| Box::new(view().build(cx)));
    let mut harness = open(rt, w, h, scale);
    for _ in 0..4 {
        harness.advance(Duration::from_millis(100));
        harness.settle(64);
    }
    let pixels = readback.lock().unwrap().take().expect("a frame was drawn");
    let (dw, dh) = ((w as f64 * scale) as u32, (h as f64 * scale) as u32);
    let mut img = image::RgbaImage::new(dw, dh);
    for y in 0..dh {
        for x in 0..dw {
            let p = pixels.rgba(x as i32, y as i32);
            img.put_pixel(x, y, image::Rgba([p[0], p[1], p[2], 255]));
        }
    }
    std::mem::forget(harness);
    img
}
