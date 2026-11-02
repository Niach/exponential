//! EXP-1604: THE gpui Motion painter (`eui-motion/README.md` › "Painting a
//! Motion"). The engine samples the display list, its `raster` feature
//! (tiny-skia, the parity reference itself) paints it into ONE reused
//! pixmap at the element's device size, and the pixels reach gpui as a
//! [`RenderImage`]. A painter never interprets Lottie or the clock: the
//! source compiles in the engine, the frame comes from
//! [`eui_motion::playback`].
//!
//! [`MotionPlayer`] is gpui-free apart from the image it hands out (the
//! pixel-parity test drives it headless); the view's leaf
//! (`view/motion_leaf.rs`) owns one per Motion node and its clock.

use std::sync::Arc;

use eui_core::theme::{Mode, ResolvedTheme};
use eui_motion::playback::{Playback, Timeline};
use eui_motion::{DisplayList, Limits, Motion};
use gpui::RenderImage;
use serde_json::{json, Map, Value};
use tiny_skia::{Pixmap, Transform};

use crate::paint::parts::theme_color;

/// How the composition fills the box (`fit`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Uniform, centred, all of it visible.
    #[default]
    Contain,
    /// Uniform, centred, filling the box (clipped).
    Cover,
    /// Stretched to the box.
    Fill,
}

impl Fit {
    pub fn from_props(props: &Map<String, Value>) -> Self {
        match props.get("fit").and_then(Value::as_str) {
            Some("cover") => Fit::Cover,
            Some("fill") => Fit::Fill,
            _ => Fit::Contain,
        }
    }
}

/// The dialect document of a Motion node: the props the README names
/// (`size`, `fps`, `duration`, `start`, `time`, `layers`, `assets`,
/// `markers`).
const DOCUMENT_KEYS: [&str; 8] = ["size", "fps", "duration", "start", "time", "layers", "assets", "markers"];

/// The node's inline source as Lottie JSON: `layers` (the dialect,
/// compiled), else `lottie` as is. `Ok(None)` = no inline source (a `src`
/// the host fetches, or nothing).
pub fn inline_lottie(props: &Map<String, Value>) -> Result<Option<Value>, String> {
    if props.get("layers").is_some_and(Value::is_array) {
        let doc: Map<String, Value> = DOCUMENT_KEYS.iter().filter_map(|k| props.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
        return eui_motion::dialect::compile(&Value::Object(doc)).map(Some).map_err(issues_text);
    }
    match props.get("lottie") {
        Some(l @ Value::Object(_)) => Ok(Some(l.clone())),
        _ => Ok(None),
    }
}

/// A fetched `src`: Lottie JSON, or a dotLottie archive (its first
/// animation, its themes ignored: the theme's slots apply).
pub fn fetched_lottie(bytes: &[u8]) -> Result<Value, String> {
    fetched_motion(bytes).map(|(a, _)| a)
}

/// A fetched `src` with its state machine (EXP-1605): a dotLottie
/// archive's FIRST state machine (`s/`), `None` for plain Lottie JSON.
pub fn fetched_motion(bytes: &[u8]) -> Result<(Value, Option<Value>), String> {
    if bytes.starts_with(b"PK") {
        let dl = eui_motion::dotlottie::read(bytes).map_err(issues_text)?;
        let machine = dl.state_machines.into_iter().next().map(|(_, m)| m);
        let animation = dl.animations.into_iter().next().map(|(_, a)| a).ok_or_else(|| "the .lottie file has no animation".to_string())?;
        return Ok((animation, machine));
    }
    serde_json::from_slice(bytes).map(|a| (a, None)).map_err(|e| e.to_string())
}

/// The node's own state machine (`stateMachine`: dotLottie 2 JSON, an
/// object or its text).
pub fn machine_prop(props: &Map<String, Value>) -> Option<Value> {
    match props.get("stateMachine")? {
        v @ Value::Object(_) => Some(v.clone()),
        Value::String(s) => serde_json::from_str(s).ok(),
        _ => None,
    }
}

fn issues_text(issues: Vec<eui_motion::Issue>) -> String {
    issues.iter().map(|i| format!("{} {}", i.path, i.message)).collect::<Vec<_>>().join("; ")
}

/// The slot overrides for `lottie` (README step 2): every `$color.<token>`
/// slot of the file gets the theme's resolved colour for `mode`, `$mode`
/// picks the `{light, dark}` halves, the node's own `slots` prop last.
pub fn theme_slots(lottie: &Value, theme: Option<&ResolvedTheme>, mode: Mode, node_slots: Option<&Value>) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(Value::Object(slots)) = lottie.get("slots") {
        for sid in slots.keys() {
            let Some(token) = sid.strip_prefix("$color.") else { continue };
            if let Some(c) = theme_color(theme, mode, token) {
                let c = c.to_rgb();
                out.insert(sid.clone(), json!({"p": {"a": 0, "k": [c.r, c.g, c.b, c.a]}}));
            }
        }
    }
    out.insert("$mode".into(), json!(if mode == Mode::Dark { "dark" } else { "light" }));
    if let Some(Value::Object(own)) = node_slots {
        for (k, v) in own {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// One loaded Motion: the engine, its clock facts, its decoded images, and
/// the reused buffers of the last painted frame.
pub struct MotionPlayer {
    motion: Motion,
    pub timeline: Timeline,
    pub playback: Playback,
    images: Vec<Option<Pixmap>>,
    list: DisplayList,
    pixmap: Option<Pixmap>,
    /// (frame, width, height, fit) of the pixmap's contents.
    painted: Option<(u64, u32, u32, Fit)>,
    image: Option<Arc<RenderImage>>,
    /// Wall time of the last paint (sample + raster + conversion), µs.
    pub last_paint_us: u64,
}

impl MotionPlayer {
    /// Load `lottie` with the slot overrides; the node's playback props.
    pub fn load(lottie: &Value, slots: &Map<String, Value>, props: &Map<String, Value>) -> Result<Self, String> {
        let bytes = serde_json::to_vec(lottie).map_err(|e| e.to_string())?;
        let motion = Motion::load_with(&bytes, &Limits::default(), slots).map_err(issues_text)?;
        let images = eui_motion::raster::decode_images(&motion, None);
        Ok(Self {
            timeline: motion.timeline(),
            playback: Playback::from_json(&Value::Object(props.clone())),
            motion,
            images,
            list: DisplayList::default(),
            pixmap: None,
            painted: None,
            image: None,
            last_paint_us: 0,
        })
    }

    /// Composition width × height.
    pub fn size(&self) -> (f32, f32) {
        self.motion.size()
    }

    /// Re-read the playback props (they change without a reload).
    pub fn set_playback(&mut self, props: &Map<String, Value>) {
        self.playback = Playback::from_json(&Value::Object(props.clone()));
    }

    /// Paint `frame` into the reused `width × height` pixmap (premultiplied
    /// RGBA, transparent background) under `fit`; repaints only when the
    /// frame, the size or the fit changed. `None` for an empty box.
    pub fn paint(&mut self, frame: f64, width: u32, height: u32, fit: Fit) -> Option<&Pixmap> {
        if width == 0 || height == 0 {
            return None;
        }
        let key = (frame.to_bits(), width, height, fit);
        if self.painted == Some(key) {
            return self.pixmap.as_ref();
        }
        let started = std::time::Instant::now();
        if self.pixmap.as_ref().is_none_or(|p| p.width() != width || p.height() != height) {
            self.pixmap = Pixmap::new(width, height);
        }
        let pixmap = self.pixmap.as_mut()?;
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        self.motion.sample(frame, &mut self.list);
        let base = fit_transform(self.list.width, self.list.height, width as f32, height as f32, fit);
        eui_motion::raster::paint_into(&self.list, pixmap, base, &self.images);
        self.painted = Some(key);
        self.image = None;
        self.last_paint_us = started.elapsed().as_micros() as u64;
        self.pixmap.as_ref()
    }

    /// The painted frame as a gpui image (straight-alpha BGRA, what gpui's
    /// sprite atlas takes). `replaced` receives the previous image when a
    /// new one is made, for the caller to drop from the atlas.
    pub fn render_image(&mut self, frame: f64, width: u32, height: u32, fit: Fit, replaced: &mut Option<Arc<RenderImage>>) -> Option<Arc<RenderImage>> {
        let started = std::time::Instant::now();
        let fresh = self.painted != Some((frame.to_bits(), width, height, fit)) || self.image.is_none();
        let old = self.image.take();
        let pixmap = self.paint(frame, width, height, fit)?;
        if !fresh {
            self.image = old;
            return self.image.clone();
        }
        let image = Arc::new(RenderImage::new([image::Frame::new(to_bgra(pixmap))]));
        *replaced = old;
        self.image = Some(image.clone());
        self.last_paint_us = started.elapsed().as_micros() as u64;
        Some(image)
    }
}

/// The fit transform of a `cw × ch` composition into a `w × h` box.
pub fn fit_transform(cw: f32, ch: f32, w: f32, h: f32, fit: Fit) -> Transform {
    if cw <= 0.0 || ch <= 0.0 {
        return Transform::identity();
    }
    let (sx, sy) = match fit {
        Fit::Fill => (w / cw, h / ch),
        Fit::Contain => {
            let s = (w / cw).min(h / ch);
            (s, s)
        }
        Fit::Cover => {
            let s = (w / cw).max(h / ch);
            (s, s)
        }
    };
    Transform::from_row(sx, 0.0, 0.0, sy, (w - cw * sx) / 2.0, (h - ch * sy) / 2.0)
}

/// Premultiplied RGBA → straight-alpha BGRA.
fn to_bgra(pixmap: &Pixmap) -> image::RgbaImage {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = vec![0u8; (w * h * 4) as usize];
    for (src, dst) in pixmap.pixels().iter().zip(out.chunks_exact_mut(4)) {
        let a = src.alpha();
        if a == 0 {
            continue;
        }
        let c = if a == 255 { tiny_skia::ColorU8::from_rgba(src.red(), src.green(), src.blue(), 255) } else { src.demultiply() };
        dst.copy_from_slice(&[c.blue(), c.green(), c.red(), a]);
    }
    image::RgbaImage::from_raw(w, h, out).expect("w × h × 4 bytes")
}

/// Straight-alpha RGBA bytes of a painted pixmap (tests: the parity PNGs
/// store straight alpha).
pub fn straight_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}
