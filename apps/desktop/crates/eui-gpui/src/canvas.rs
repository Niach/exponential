//! eUI round 8: THE gpui Canvas painter (`packages/exponential-ui/docs/
//! round-8-contract.md` §1). A Canvas node's `{size, shapes}` compiles in
//! the engine (`eui_motion::canvas::Canvas::compile`) into a one-frame
//! Motion; [`CanvasPlayer`] samples it into the Motion display list (v2)
//! with the theme's slots, rasters the shapes with tiny-skia (the parity
//! reference, exactly like [`crate::motion::MotionPlayer`]) and hands the
//! `text` ops to the view, which sets them with gpui's text system ABOVE
//! the image (the contract puts every text op after the frame's final
//! `pop`, so drawing the raster first and the text on top is the same
//! picture).
//!
//! Gpui-free apart from the [`RenderImage`]: the geometry of a text op
//! ([`text_anchor`], [`line_origin`]) is pure so the fixture replay
//! (`tests/canvas.rs`) checks it headless.

use std::sync::Arc;

use eui_motion::canvas::{fit_matrix, Canvas, Fit};
use eui_motion::display::{TextAlign, TextBaseline};
use eui_motion::{DisplayList, Op};
use gpui::RenderImage;
use serde_json::{Map, Value};
use tiny_skia::{Pixmap, Transform};

/// The node's fit (`fit`: contain | cover | fill; default contain).
pub fn fit_of(props: &Map<String, Value>) -> Fit {
    props.get("fit").and_then(Value::as_str).map_or(Fit::Contain, Fit::parse)
}

/// The drawing document of a Canvas node: its `{size, shapes}`.
pub fn document(props: &Map<String, Value>) -> Value {
    let mut doc = Map::new();
    for k in ["size", "shapes"] {
        if let Some(v) = props.get(k) {
            doc.insert(k.to_string(), v.clone());
        }
    }
    Value::Object(doc)
}

/// One painted text op (tests, automation): the line's left edge and
/// baseline in box px, the font px, the shaped width.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedText {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
    pub width: f32,
    pub mono: bool,
    pub weight: u16,
}

/// One compiled Canvas: the engine's drawing, the sampled list (shapes +
/// text ops) and the reused raster of the last paint.
pub struct CanvasPlayer {
    canvas: Canvas,
    list: DisplayList,
    pixmap: Option<Pixmap>,
    /// (width, height, fit) of the pixmap's contents; `None` = stale.
    painted: Option<(u32, u32, Fit)>,
    image: Option<Arc<RenderImage>>,
    /// Wall time of the last raster (+ conversion), µs.
    pub last_paint_us: u64,
}

impl CanvasPlayer {
    /// Compile the node's `{size, shapes}` (every refusal at once, as
    /// `code path: message` joined by `; `). Sampled with no slots until
    /// [`CanvasPlayer::set_slots`].
    pub fn compile(props: &Map<String, Value>) -> Result<Self, String> {
        let canvas = Canvas::compile(&document(props)).map_err(|issues| issues.iter().map(|i| format!("{} {}: {}", i.code, i.path, i.message)).collect::<Vec<_>>().join("; "))?;
        let mut list = DisplayList::default();
        canvas.render(&Map::new(), &mut list);
        Ok(Self { canvas, list, pixmap: None, painted: None, image: None, last_paint_us: 0 })
    }

    /// The drawing's coordinate space `[w, h]`.
    pub fn size(&self) -> (f32, f32) {
        self.canvas.size()
    }

    /// The compiled one-frame Lottie (its `slots` name every colour slot,
    /// text colours included: what [`crate::motion::theme_slots`] fills).
    pub fn lottie(&self) -> &Value {
        self.canvas.lottie()
    }

    /// Every slot id a host may fill (sorted).
    pub fn slot_ids(&self) -> Vec<String> {
        self.canvas.slot_ids()
    }

    /// Resample with the host's slot overrides (a theme or mode change):
    /// no playback, one frame; the raster goes stale.
    pub fn set_slots(&mut self, slots: &Map<String, Value>) {
        self.list.clear();
        self.canvas.render(slots, &mut self.list);
        self.painted = None;
    }

    /// The sampled display list (v2: shapes, then the text ops).
    pub fn list(&self) -> &DisplayList {
        &self.list
    }

    /// The text ops, in source order (the tail of the list).
    pub fn texts(&self) -> impl Iterator<Item = &Op> {
        self.list.ops.iter().filter(|op| matches!(op, Op::Text { .. }))
    }

    /// Raster the shapes into the reused `width × height` pixmap (device
    /// px) under `fit`; repaints only when the size, the fit or the slots
    /// changed. `None` for an empty box.
    pub fn paint(&mut self, width: u32, height: u32, fit: Fit) -> Option<&Pixmap> {
        if width == 0 || height == 0 {
            return None;
        }
        if self.painted == Some((width, height, fit)) {
            return self.pixmap.as_ref();
        }
        let started = std::time::Instant::now();
        if self.pixmap.as_ref().is_none_or(|p| p.width() != width || p.height() != height) {
            self.pixmap = Pixmap::new(width, height);
        }
        let pixmap = self.pixmap.as_mut()?;
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        let m = fit_matrix(self.list.width as f64, self.list.height as f64, width as f64, height as f64, fit);
        let base = Transform::from_row(m[0] as f32, m[1] as f32, m[2] as f32, m[3] as f32, m[4] as f32, m[5] as f32);
        // The raster skips the text ops (they are the painter's).
        eui_motion::raster::paint_into(&self.list, pixmap, base, &[]);
        self.painted = Some((width, height, fit));
        self.image = None;
        self.last_paint_us = started.elapsed().as_micros() as u64;
        self.pixmap.as_ref()
    }

    /// The shapes as a gpui image (straight-alpha BGRA). `replaced`
    /// receives the previous image when a new one is made (the caller
    /// drops it from the atlas).
    pub fn render_image(&mut self, width: u32, height: u32, fit: Fit, replaced: &mut Option<Arc<RenderImage>>) -> Option<Arc<RenderImage>> {
        let fresh = self.painted != Some((width, height, fit)) || self.image.is_none();
        if !fresh {
            return self.image.clone();
        }
        let old = self.image.take();
        let started = std::time::Instant::now();
        let pixmap = self.paint(width, height, fit)?;
        let image = Arc::new(RenderImage::new([image::Frame::new(crate::motion::to_bgra(pixmap))]));
        *replaced = old;
        self.image = Some(image.clone());
        self.last_paint_us = started.elapsed().as_micros() as u64;
        Some(image)
    }

}

/// `a ∘ b` for `[a, b, c, d, e, f]` transforms (`b` applies first).
pub fn concat(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Where a text op sets its line in box px: the op's local origin through
/// `fit × transform` (`(x, y)`), and the font px (`size` × the combined
/// transform's uniform scale, `√|det|`; gpui sets text upright, so a
/// rotation or skew in a group keeps the position and the scale only).
pub fn text_anchor(fit: [f64; 6], transform: [f32; 6], size: f32) -> (f32, f32, f32) {
    let t = transform.map(f64::from);
    let m = concat(fit, t);
    let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
    (m[4] as f32, m[5] as f32, (size as f64 * scale) as f32)
}

/// The line's left edge and baseline for an anchor at `(x, y)`, a shaped
/// line `width` wide and the font's `ascent` / `descent` magnitudes at
/// that size: `align` is physical (start = left edge at x, end = right
/// edge at x); `baseline` top = y + ascent, middle = y + (ascent −
/// descent) / 2, alphabetic = y.
pub fn line_origin(x: f32, y: f32, width: f32, ascent: f32, descent: f32, align: TextAlign, baseline: TextBaseline) -> (f32, f32) {
    let left = match align {
        TextAlign::Start => x,
        TextAlign::Center => x - width / 2.0,
        TextAlign::End => x - width,
    };
    let base = match baseline {
        TextBaseline::Top => y + ascent,
        TextBaseline::Middle => y + (ascent - descent) / 2.0,
        TextBaseline::Alphabetic => y,
    };
    (left, base)
}

/// A pointer at box px → drawing units (the fit mapping inverted, two
/// decimals, never clamped): what `press` / `move` carry.
pub fn pointer_point(player_size: (f32, f32), box_size: (f32, f32), fit: Fit, at: (f32, f32)) -> Option<[f64; 2]> {
    eui_motion::canvas::to_drawing(at.0 as f64, at.1 as f64, player_size.0 as f64, player_size.1 as f64, box_size.0 as f64, box_size.1 as f64, fit)
}
