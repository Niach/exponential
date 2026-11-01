//! The `raster` reference painter (tiny-skia): paints a [`DisplayList`]
//! into RGBA pixels. It is the gpui painter's backend, the per-renderer
//! pixel-parity reference and the GIF preview path.
//!
//! Every `layer` paints into its own offscreen surface, BOUNDED to a live
//! rect: the device bbox of its clip intersected with the clip in force and
//! (for blends where transparent pixels leave the destination unchanged)
//! with the bbox of what the layer actually draws, filter spread included
//! (a pre-pass over the list). Only live rects are cleared, filtered and
//! composited, clips are built only over them, draws outside the clip are
//! skipped and layers or clips with an empty bbox are skipped wholesale.
//! Rasterization stays at absolute device coordinates, so the result is
//! bit-identical to painting every layer full-size
//! ([`paint_into_unbounded`], the reference the tests compare against).

use crate::display::{Blend, Brush, Cap, DisplayList, FillRule, Filter, Join, Op, Stop, Verb};
use tiny_skia::{
    BlendMode, GradientStop, IntSize, LineCap, LineJoin, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Point, RadialGradient, Shader, SpreadMode, Stroke, StrokeDash, Transform,
};

/// Decoded image assets (premultiplied RGBA pixmaps), by `Op::Image::asset`.
pub type Images<'a> = &'a [Option<Pixmap>];

/// Paint `list` into a `width × height` pixmap, scaled uniformly to fit
/// (contain, centred) on a transparent background.
pub fn paint(list: &DisplayList, width: u32, height: u32, images: Images<'_>) -> Pixmap {
    let mut target = Pixmap::new(width.max(1), height.max(1)).expect("non-zero pixmap");
    let base = fit(list, width as f32, height as f32);
    paint_into(list, &mut target, base, images);
    target
}

/// The contain-fit transform of `list` into `width × height`.
pub fn fit(list: &DisplayList, width: f32, height: f32) -> Transform {
    if list.width <= 0.0 || list.height <= 0.0 {
        return Transform::identity();
    }
    let s = (width / list.width).min(height / list.height);
    let tx = (width - list.width * s) / 2.0;
    let ty = (height - list.height * s) / 2.0;
    Transform::from_row(s, 0.0, 0.0, s, tx, ty)
}

/// Paint `list` into `target` under `base`.
pub fn paint_into(list: &DisplayList, target: &mut Pixmap, base: Transform, images: Images<'_>) {
    Painter::new(list, target, base, images).run();
}

/// [`paint_into`] with every layer a full-size pixmap, every clip a
/// full-size mask and nothing skipped: the straightforward reference the
/// bounded painter matches bit for bit (tests, debugging).
pub fn paint_into_unbounded(list: &DisplayList, target: &mut Pixmap, base: Transform, images: Images<'_>) {
    paint_reference(list, target, base, images);
}

// --- Image assets ----------------------------------------------------------

/// Decode `motion`'s image assets (in [`crate::Motion::images`] order) into
/// premultiplied pixmaps: `data:` URLs (base64 PNG/JPEG/WebP) and file
/// paths, relative ones against `base_dir`. `None` where an asset is missing
/// or undecodable.
pub fn decode_images(motion: &crate::Motion, base_dir: Option<&std::path::Path>) -> Vec<Option<Pixmap>> {
    motion
        .images()
        .iter()
        .map(|(_, location)| decode_location(location, base_dir))
        .collect()
}

fn decode_location(location: &str, base_dir: Option<&std::path::Path>) -> Option<Pixmap> {
    let bytes = if let Some(rest) = location.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',')?;
        if !meta.ends_with(";base64") {
            return None;
        }
        use base64::Engine;
        let clean: Vec<u8> = data.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(&clean)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&clean))
            .ok()?
    } else {
        if location.is_empty() {
            return None;
        }
        let path = std::path::Path::new(location);
        let full = match base_dir {
            Some(dir) if path.is_relative() => dir.join(path),
            _ => path.to_path_buf(),
        };
        std::fs::read(full).ok()?
    };
    decode_bytes(&bytes)
}

/// Decode PNG/JPEG/WebP bytes into a premultiplied pixmap.
pub fn decode_bytes(bytes: &[u8]) -> Option<Pixmap> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut data = img.into_raw();
    for px in data.chunks_exact_mut(4) {
        let a = px[3];
        if a < 255 {
            for c in &mut px[..3] {
                *c = tiny_skia_premultiply(*c, a);
            }
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
}

fn tiny_skia_premultiply(c: u8, a: u8) -> u8 {
    let prod = u32::from(c) * u32::from(a) + 128;
    ((prod + (prod >> 8)) >> 8) as u8
}

// --- Device rects ----------------------------------------------------------

/// An integer device rect `[x0, x1) × [y0, y1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IRect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

const BIG: i32 = 1 << 28;

impl IRect {
    const EMPTY: IRect = IRect { x0: BIG, y0: BIG, x1: -BIG, y1: -BIG };
    const INF: IRect = IRect { x0: -BIG, y0: -BIG, x1: BIG, y1: BIG };

    fn wh(x: i32, y: i32, w: u32, h: u32) -> IRect {
        IRect { x0: x, y0: y, x1: x + w as i32, y1: y + h as i32 }
    }

    fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    fn intersect(&self, o: &IRect) -> IRect {
        IRect { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }

    fn union(&self, o: &IRect) -> IRect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        IRect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }

    fn contains(&self, o: &IRect) -> bool {
        o.is_empty() || (self.x0 <= o.x0 && self.y0 <= o.y0 && self.x1 >= o.x1 && self.y1 >= o.y1)
    }

    fn expand(&self, ex: i32, ey: i32) -> IRect {
        if self.is_empty() {
            return *self;
        }
        IRect {
            x0: (self.x0 - ex).max(-BIG),
            y0: (self.y0 - ey).max(-BIG),
            x1: (self.x1 + ex).min(BIG),
            y1: (self.y1 + ey).min(BIG),
        }
    }

    fn shift(&self, dx: i32, dy: i32) -> IRect {
        if self.is_empty() {
            return *self;
        }
        IRect { x0: self.x0 + dx, y0: self.y0 + dy, x1: self.x1 + dx, y1: self.y1 + dy }
    }

    fn width(&self) -> u32 {
        (self.x1 - self.x0).max(0) as u32
    }

    fn height(&self) -> u32 {
        (self.y1 - self.y0).max(0) as u32
    }

    /// The pixels a float rect touches, plus `margin` (anti-aliasing).
    fn round_out(x0: f32, y0: f32, x1: f32, y1: f32, margin: i32) -> IRect {
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return IRect::INF;
        }
        let c = |v: f32| v.clamp(-BIG as f32, BIG as f32);
        IRect {
            x0: c(x0.floor()) as i32 - margin,
            y0: c(y0.floor()) as i32 - margin,
            x1: c(x1.ceil()) as i32 + margin,
            y1: c(y1.ceil()) as i32 + margin,
        }
        .intersect(&IRect::INF)
    }
}

/// The device bbox of local rect `(l, t, r, b)` under `t`.
fn map_bbox(l: f32, top: f32, r: f32, b: f32, t: &Transform, margin: i32) -> IRect {
    let mut pts = [
        Point::from_xy(l, top),
        Point::from_xy(r, top),
        Point::from_xy(r, b),
        Point::from_xy(l, b),
    ];
    t.map_points(&mut pts);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    IRect::round_out(x0, y0, x1, y1, margin)
}

fn path_bbox(path: &Path, t: &Transform, pad: f32, margin: i32) -> IRect {
    let r = path.bounds();
    map_bbox(r.left() - pad, r.top() - pad, r.right() + pad, r.bottom() + pad, t, margin)
}

/// A device-space axis-aligned rectangle (float corners).
#[derive(Clone, Copy, Debug, PartialEq)]
struct FRect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl FRect {
    /// The pixels it covers exactly when its corners are integers (its
    /// anti-aliased coverage is then exactly 255 inside, 0 outside).
    fn exact(&self) -> Option<IRect> {
        let int = |v: f32| v == v.round() && v.abs() <= BIG as f32;
        (int(self.x0) && int(self.y0) && int(self.x1) && int(self.y1))
            .then(|| IRect { x0: self.x0 as i32, y0: self.y0 as i32, x1: self.x1 as i32, y1: self.y1 as i32 })
    }

    /// The pixels it covers fully.
    fn interior(&self) -> IRect {
        let c = |v: f32| v.clamp(-BIG as f32, BIG as f32);
        IRect { x0: c(self.x0.ceil()) as i32, y0: c(self.y0.ceil()) as i32, x1: c(self.x1.floor()) as i32, y1: c(self.y1.floor()) as i32 }
    }
}

/// A 4-corner rect path that stays axis-aligned under `t`, in device space
/// (corners mapped exactly as tiny-skia maps them).
fn axis_rect(path: &[Verb], t: &Transform) -> Option<FRect> {
    let pts: [(f32, f32); 4] = match path {
        [Verb::M { x, y }, Verb::L { x: x1, y: y1 }, Verb::L { x: x2, y: y2 }, Verb::L { x: x3, y: y3 }, Verb::Z] => {
            [(*x, *y), (*x1, *y1), (*x2, *y2), (*x3, *y3)]
        }
        [Verb::M { x, y }, Verb::L { x: x1, y: y1 }, Verb::L { x: x2, y: y2 }, Verb::L { x: x3, y: y3 }, Verb::L { x: x4, y: y4 }, Verb::Z]
            if (x4, y4) == (x, y) =>
        {
            [(*x, *y), (*x1, *y1), (*x2, *y2), (*x3, *y3)]
        }
        _ => return None,
    };
    if t.kx != 0.0 || t.ky != 0.0 {
        return None;
    }
    let mut m = pts.map(|(x, y)| Point::from_xy(x, y));
    t.map_points(&mut m);
    if m.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    let (x0, x1) = (m.iter().map(|p| p.x).fold(f32::MAX, f32::min), m.iter().map(|p| p.x).fold(f32::MIN, f32::max));
    let (y0, y1) = (m.iter().map(|p| p.y).fold(f32::MAX, f32::min), m.iter().map(|p| p.y).fold(f32::MIN, f32::max));
    // Each corner on the bbox's corners, consecutive corners sharing an edge.
    if m.iter().any(|p| (p.x != x0 && p.x != x1) || (p.y != y0 && p.y != y1)) {
        return None;
    }
    for k in 0..4 {
        let (a, b) = (m[k], m[(k + 1) % 4]);
        if a.x != b.x && a.y != b.y {
            return None;
        }
    }
    (x1 > x0 && y1 > y0).then_some(FRect { x0, y0, x1, y1 })
}

// --- Content bounds pre-pass -----------------------------------------------

/// Blends where a transparent source pixel CHANGES the destination (they
/// clear outside the layer's content), so their surface cannot shrink to it.
fn clears_outside(blend: Blend) -> bool {
    matches!(blend, Blend::DstIn | Blend::LumaIn)
}

/// Blends that never add coverage to the destination.
fn only_removes(blend: Blend) -> bool {
    matches!(blend, Blend::DstIn | Blend::LumaIn | Blend::DstOut | Blend::LumaOut)
}

/// The linear part of `base` applied to a display-list vector.
fn device_vec(base: &Transform, dx: f32, dy: f32) -> (f32, f32) {
    (base.sx * dx + base.kx * dy, base.ky * dx + base.sy * dy)
}

/// What `filters` do to a content bbox (device pixels).
fn expand_by_filters(mut b: IRect, filters: &[Filter], base: &Transform, scale: f32) -> IRect {
    for f in filters {
        match f {
            Filter::Blur { sigma_x, sigma_y } => {
                b = b.expand(blur_extent(sigma_x * scale), blur_extent(sigma_y * scale));
            }
            Filter::DropShadow { dx, dy, sigma, .. } => {
                let (ddx, ddy) = device_vec(base, *dx, *dy);
                if !(ddx.is_finite() && ddy.is_finite()) {
                    return IRect::INF;
                }
                let e = blur_extent(sigma * scale);
                let (fx, fy) = (ddx.floor() as i32, ddy.floor() as i32);
                // Floor shift, then one more pixel for the fractional part.
                let mut s = b.shift(fx, fy);
                s.x1 += 1;
                s.y1 += 1;
                b = b.union(&s.expand(e, e));
            }
            Filter::ColorMatrix { m } => {
                if !is_identity(m) && m[19] > 0.0 {
                    return IRect::INF;
                }
            }
            Filter::GradientMap { .. } => {}
        }
    }
    b
}

/// Whether `filters` move pixels (a blur or shadow reads beyond each pixel).
fn spreads(filters: &[Filter]) -> bool {
    filters.iter().any(|f| match f {
        Filter::Blur { sigma_x, sigma_y } => *sigma_x > 0.0 || *sigma_y > 0.0,
        Filter::DropShadow { .. } => true,
        _ => false,
    })
}

fn stroke_pad(style: &crate::display::StrokeStyle) -> f32 {
    let half = style.width.max(0.0) / 2.0;
    let join = if style.join == Join::Miter { style.miter.max(1.0) } else { 1.0 };
    let cap = if style.cap == Cap::Square { std::f32::consts::SQRT_2 } else { 1.0 };
    half * join.max(cap)
}

/// What a `Layer` op draws: the device bbox of its content BEFORE its
/// filters, inside its own clip. `wide` always holds; `narrow` also counts
/// mattes (a direct dst-in child whose clip covers the layer's) as clearing
/// everything outside the matte, which holds when the layer's clip covers
/// its bounds fully (no anti-aliased edge for the matte to leave residue in).
#[derive(Clone, Copy)]
struct Content {
    wide: IRect,
    narrow: IRect,
}

/// Per op index (`IRect::INF` for non-layers). A layer adds to its parent's
/// bbox what it composites (filters applied), and a spreading layer that
/// only removes adds its own content too (its blur reads it back into the
/// parent, so the parent's clip must cover it).
fn content_bounds(list: &DisplayList, base: &Transform, scale: f32) -> Vec<Content> {
    struct Acc {
        idx: usize,
        layer: bool,
        wide: IRect,
        narrow: IRect,
        clip: IRect,
        /// The pixels the clip path surely covers fully (rect clips).
        inside: IRect,
        rect: bool,
    }
    let mut out = vec![Content { wide: IRect::INF, narrow: IRect::INF }; list.ops.len()];
    let mut stack: Vec<Acc> = Vec::new();
    let add = |stack: &mut Vec<Acc>, w: IRect, n: IRect| {
        if let Some(top) = stack.last_mut() {
            top.wide = top.wide.union(&w);
            top.narrow = top.narrow.union(&n);
        }
    };
    for (idx, op) in list.ops.iter().enumerate() {
        match op {
            Op::Layer { transform, clip, .. } | Op::Clip { transform, path: clip } => {
                let t = base.pre_concat(tf(transform));
                let c = build(clip).map(|p| path_bbox(&p, &t, 0.0, 1)).unwrap_or(IRect::EMPTY);
                let rect = axis_rect(clip, &t);
                let inside = rect.map(|r| r.exact().unwrap_or_else(|| r.interior())).unwrap_or(IRect::EMPTY);
                let layer = matches!(op, Op::Layer { .. });
                stack.push(Acc { idx, layer, wide: IRect::EMPTY, narrow: IRect::EMPTY, clip: c, inside, rect: rect.is_some() });
            }
            Op::Pop => {
                let Some(acc) = stack.pop() else { continue };
                match &list.ops[acc.idx] {
                    Op::Layer { blend, filters, .. } => {
                        let (pre_w, pre_n) = (acc.wide.intersect(&acc.clip), acc.narrow.intersect(&acc.clip));
                        out[acc.idx] = Content { wide: pre_w, narrow: pre_n };
                        let post_w = expand_by_filters(pre_w, filters, base, scale);
                        let post_n = expand_by_filters(pre_n, filters, base, scale);
                        if !only_removes(*blend) {
                            add(&mut stack, post_w.intersect(&acc.clip), post_n.intersect(&acc.clip));
                        } else if spreads(filters) {
                            add(&mut stack, pre_w, pre_n);
                        }
                        if clears_outside(*blend) {
                            if let Some(parent) = stack.last_mut() {
                                if parent.layer && parent.rect && acc.inside.contains(&parent.inside) {
                                    parent.narrow = parent.narrow.intersect(&post_n);
                                }
                            }
                        }
                    }
                    _ => add(&mut stack, acc.wide.intersect(&acc.clip), acc.narrow.intersect(&acc.clip)),
                }
            }
            Op::Fill { transform, path, .. } => {
                let t = base.pre_concat(tf(transform));
                if let Some(p) = build(path) {
                    let r = path_bbox(&p, &t, 0.0, 1);
                    add(&mut stack, r, r);
                }
            }
            Op::Stroke { transform, style, path, .. } => {
                let t = base.pre_concat(tf(transform));
                if let Some(p) = build(path) {
                    // +1 device pixel for hairlines.
                    let r = path_bbox(&p, &t, stroke_pad(style), 2);
                    add(&mut stack, r, r);
                }
            }
            Op::Image { transform, asset: _, width, height, .. } => {
                let t = base.pre_concat(tf(transform));
                // Bilinear sampling bleeds one pixel.
                let r = map_bbox(0.0, 0.0, width.max(0.0), height.max(0.0), &t, 2);
                add(&mut stack, r, r);
            }
        }
    }
    out
}

// --- The painter -----------------------------------------------------------
//
// Every surface is a full-size pixmap in DEVICE coordinates (tiny-skia
// rasterizes at the same absolute coordinates as the reference, so the
// result is bit-identical), but only its `live` rect is ever cleared,
// filtered or composited, and draws whose bbox misses the clip are skipped.
// Pixmaps and masks come from pools that hold them all-zero: a surface
// tracks the rect it `dirty`-ed and clears just that when it goes back.

/// The clip in force: `bounds` (device, ⊆ the surface's live rect) and
/// `mask` (full-size, zero outside `bounds`; `None` = the clip is exactly
/// the integer rect `bounds`: nested clips and composites need no mask).
struct Clip {
    bounds: IRect,
    mask: Option<Mask>,
    /// `bounds` as a mask (when `mask` is `None`), built on the first draw:
    /// draws always go through a mask, like the reference's.
    rect: Option<Mask>,
}

struct Surface {
    pixmap: Pixmap,
    /// What is read and composited: the layer's content (+ filter spread).
    live: IRect,
    /// Everything ever written (cleared on release).
    dirty: IRect,
}

enum Frame<'l> {
    Layer {
        surface: Surface,
        blend: Blend,
        alpha: f32,
        filters: &'l [Filter],
        /// The parent's clip, restored on `Pop`.
        saved: Clip,
    },
    Clip {
        saved: Clip,
    },
}

/// How a clip path covers the clip's bounds.
enum Cover {
    /// Fully (an integer rect, whose bounds stop at its edges, or an
    /// axis-aligned rect whose interior contains them).
    Full,
    /// Rasterize it.
    Path,
}

/// A thread's zeroed full-size buffers, kept between frames.
#[derive(Default)]
struct Pool {
    size: (u32, u32),
    pixmaps: Vec<Pixmap>,
    masks: Vec<Mask>,
}

/// Most buffers of each kind a thread keeps (the layer nesting depth).
const POOL_CAP: usize = 8;

thread_local! {
    static POOL: std::cell::RefCell<Pool> = std::cell::RefCell::new(Pool::default());
}

struct Painter<'a> {
    list: &'a DisplayList,
    target: &'a mut Pixmap,
    base: Transform,
    images: Images<'a>,
    scale: f32,
    full: IRect,
    content: Vec<Content>,
    pixmaps: Vec<Pixmap>,
    masks: Vec<Mask>,
}

impl<'a> Painter<'a> {
    fn new(list: &'a DisplayList, target: &'a mut Pixmap, base: Transform, images: Images<'a>) -> Self {
        let scale = (base.sx * base.sy - base.kx * base.ky).abs().sqrt();
        let content = content_bounds(list, &base, scale);
        let full = IRect::wh(0, 0, target.width(), target.height());
        // Reuse the zeroed buffers of this thread's previous frame at this size.
        let (pixmaps, masks) = POOL.with(|p| {
            let mut p = p.borrow_mut();
            if p.size == (full.width(), full.height()) {
                (std::mem::take(&mut p.pixmaps), std::mem::take(&mut p.masks))
            } else {
                (Vec::new(), Vec::new())
            }
        });
        Painter { list, target, base, images, scale, full, content, pixmaps, masks }
    }

    /// Hand the (all-zero) buffers back to the thread's pool.
    fn finish(self) {
        let size = (self.full.width(), self.full.height());
        let (mut pixmaps, mut masks) = (self.pixmaps, self.masks);
        pixmaps.truncate(POOL_CAP);
        masks.truncate(POOL_CAP);
        POOL.with(|p| *p.borrow_mut() = Pool { size, pixmaps, masks });
    }

    fn take_pixmap(&mut self) -> Pixmap {
        self.pixmaps
            .pop()
            .unwrap_or_else(|| Pixmap::new(self.full.width(), self.full.height()).expect("non-zero pixmap"))
    }

    fn give_pixmap(&mut self, mut p: Pixmap, dirty: IRect) {
        clear_rect(p.data_mut(), self.full.width() as usize, 4, &dirty.intersect(&self.full));
        self.pixmaps.push(p);
    }

    fn take_mask(&mut self) -> Mask {
        self.masks
            .pop()
            .unwrap_or_else(|| Mask::new(self.full.width(), self.full.height()).expect("non-zero mask"))
    }

    fn give_mask(&mut self, mut m: Mask, dirty: IRect) {
        clear_rect(m.data_mut(), self.full.width() as usize, 1, &dirty.intersect(&self.full));
        self.masks.push(m);
    }

    fn give_clip(&mut self, c: Clip) {
        for m in [c.mask, c.rect].into_iter().flatten() {
            self.give_mask(m, c.bounds);
        }
    }

    /// `parent ∩ path` over the live rect `live`; `None` when empty.
    fn clip(&mut self, parent: &Clip, path: &[Verb], t: &Transform, live: &IRect) -> Option<Clip> {
        let built = build(path)?;
        let bbox = path_bbox(&built, t, 0.0, 1);
        let rect = axis_rect(path, t);
        let exact = rect.and_then(|r| r.exact());
        let region = match exact {
            Some(r) => r,
            None => bbox,
        };
        let bounds = parent.bounds.intersect(&region).intersect(live);
        if bounds.is_empty() {
            return None;
        }
        let cover = match (exact, rect) {
            (Some(_), _) => Cover::Full,
            (None, Some(r)) if r.interior().contains(&bounds) => Cover::Full,
            _ => Cover::Path,
        };
        if !matches!(cover, Cover::Path) && parent.mask.is_none() {
            return Some(Clip { bounds, mask: None, rect: None });
        }
        let stride = self.full.width() as usize;
        let mut m = self.take_mask();
        let coverage = match cover {
            Cover::Path => {
                let mut c = self.take_mask();
                c.fill_path(&built, tiny_skia::FillRule::Winding, true, *t);
                Some(c)
            }
            _ => None,
        };
        {
            let out = m.data_mut();
            for y in bounds.y0..bounds.y1 {
                let row = y as usize * stride;
                let (a, b) = (row + bounds.x0 as usize, row + bounds.x1 as usize);
                match (&parent.mask, &coverage) {
                    (Some(p), Some(c)) => {
                        for ((o, p), c) in out[a..b].iter_mut().zip(&p.data()[a..b]).zip(&c.data()[a..b]) {
                            *o = tiny_skia_premultiply(*p, *c);
                        }
                    }
                    (Some(p), None) => out[a..b].copy_from_slice(&p.data()[a..b]),
                    (None, Some(c)) => out[a..b].copy_from_slice(&c.data()[a..b]),
                    (None, None) => out[a..b].fill(255),
                }
            }
        }
        if let Some(c) = coverage {
            self.give_mask(c, bbox);
        }
        Some(Clip { bounds, mask: Some(m), rect: None })
    }

    /// The clip as a mask (a rect clip's built once).
    fn draw_mask<'c>(&mut self, clip: &'c mut Clip) -> &'c Mask {
        if clip.mask.is_none() && clip.rect.is_none() {
            let mut m = self.take_mask();
            let stride = self.full.width() as usize;
            let b = clip.bounds;
            for y in b.y0..b.y1 {
                let row = y as usize * stride;
                m.data_mut()[row + b.x0 as usize..row + b.x1 as usize].fill(255);
            }
            clip.rect = Some(m);
        }
        clip.mask.as_ref().or(clip.rect.as_ref()).expect("a mask")
    }

    fn run(mut self) {
        let list = self.list;
        let full = self.full;
        let mut stack: Vec<Frame<'a>> = Vec::new();
        let mut clip = Clip { bounds: full, mask: None, rect: None };
        let mut root_dirty = IRect::EMPTY;
        // Ops inside a skipped (empty) layer or clip.
        let mut skip = 0usize;
        for (idx, op) in list.ops.iter().enumerate() {
            if skip > 0 {
                match op {
                    Op::Layer { .. } | Op::Clip { .. } => skip += 1,
                    Op::Pop => skip -= 1,
                    _ => {}
                }
                continue;
            }
            let live = top_live(&stack, full);
            match op {
                Op::Layer { blend, alpha, transform, clip: path, filters } => {
                    let t = self.base.pre_concat(tf(transform));
                    let path_box = match axis_rect(path, &t).and_then(|r| r.exact()) {
                        Some(r) => r,
                        None => build(path).map(|p| path_bbox(&p, &t, 0.0, 1)).unwrap_or(IRect::EMPTY),
                    };
                    // The live rect: what the layer draws inside its clip, its
                    // filters' spread included (a spreading filter may read
                    // beyond the parent's live rect, never beyond the frame);
                    // dst-in-like blends also clear the rest of the clip.
                    let clip_r = clip.bounds.intersect(&path_box);
                    // A clip without a mask covers its bounds fully: mattes clear exactly.
                    let full_cover = clip.mask.is_none()
                        && axis_rect(path, &t).is_some_and(|r| r.exact().is_some() || r.interior().contains(&clip_r));
                    let content = if full_cover { self.content[idx].narrow } else { self.content[idx].wide };
                    let mut s = expand_by_filters(content.intersect(&clip_r), filters, &self.base, self.scale);
                    if clears_outside(*blend) {
                        s = s.union(&clip_r.intersect(&live));
                    }
                    let s = if spreads(filters) { s.intersect(&full) } else { s.intersect(&clip_r).intersect(&live) };
                    if s.is_empty() || clip_r.is_empty() {
                        skip = 1;
                        continue;
                    }
                    let Some(layer_clip) = self.clip(&clip, path, &t, &s) else {
                        skip = 1;
                        continue;
                    };
                    let pixmap = self.take_pixmap();
                    let saved = std::mem::replace(&mut clip, layer_clip);
                    stack.push(Frame::Layer {
                        surface: Surface { pixmap, live: s, dirty: IRect::EMPTY },
                        blend: *blend,
                        alpha: *alpha,
                        filters,
                        saved,
                    });
                }
                Op::Clip { transform, path } => {
                    let t = self.base.pre_concat(tf(transform));
                    let Some(next) = self.clip(&clip, path, &t, &live) else {
                        skip = 1;
                        continue;
                    };
                    let saved = std::mem::replace(&mut clip, next);
                    stack.push(Frame::Clip { saved });
                }
                Op::Pop => match stack.pop() {
                    Some(Frame::Clip { saved }) => {
                        let inner = std::mem::replace(&mut clip, saved);
                        self.give_clip(inner);
                    }
                    Some(Frame::Layer { surface, blend, alpha, filters, saved }) => {
                        let layer_clip = std::mem::replace(&mut clip, saved);
                        let Surface { mut pixmap, live: s, mut dirty } = surface;
                        let luma = matches!(blend, Blend::LumaIn | Blend::LumaOut);
                        if luma || !filters.is_empty() {
                            let mut window = read_window(&pixmap, &s);
                            if luma {
                                luma_mask(&mut window);
                            }
                            apply_filters(&mut window, filters, &self.base, self.scale);
                            write_window(&mut pixmap, &s, &window);
                            dirty = dirty.union(&s);
                        }
                        let mut layer_clip = layer_clip;
                        let region = layer_clip.bounds.intersect(&s).intersect(&full);
                        if !region.is_empty() {
                            let mask = if fast_blend(blend) { layer_clip.mask.as_ref() } else { Some(self.draw_mask(&mut layer_clip)) };
                            let (dst, _, dst_dirty) = top(&mut stack, self.target, full, &mut root_dirty);
                            composite(dst, &pixmap, &region, blend, alpha, mask);
                            *dst_dirty = dst_dirty.union(&region);
                        }
                        self.give_pixmap(pixmap, dirty);
                        self.give_clip(layer_clip);
                    }
                    None => {}
                },
                Op::Fill { transform, rule, brush, path } => {
                    let Some(p) = build(path) else { continue };
                    let t = self.base.pre_concat(tf(transform));
                    let bbox = path_bbox(&p, &t, 0.0, 1).intersect(&clip.bounds);
                    if bbox.is_empty() {
                        continue;
                    }
                    let Some(paint) = make_paint(brush) else { continue };
                    let mask = self.draw_mask(&mut clip);
                    let (dst, _, dirty) = top(&mut stack, self.target, full, &mut root_dirty);
                    dst.fill_path(&p, &paint, fill_rule(*rule), t, Some(mask));
                    *dirty = dirty.union(&path_bbox(&p, &t, 0.0, 1));
                }
                Op::Stroke { transform, style, brush, path } => {
                    let Some(p) = build(path) else { continue };
                    let t = self.base.pre_concat(tf(transform));
                    let touched = path_bbox(&p, &t, stroke_pad(style), 2);
                    if touched.intersect(&clip.bounds).is_empty() {
                        continue;
                    }
                    let Some(paint) = make_paint(brush) else { continue };
                    let stroke = make_stroke(style);
                    let mask = self.draw_mask(&mut clip);
                    let (dst, _, dirty) = top(&mut stack, self.target, full, &mut root_dirty);
                    dst.stroke_path(&p, &paint, &stroke, t, Some(mask));
                    *dirty = dirty.union(&touched);
                }
                Op::Image { transform, asset, alpha, width, height } => {
                    let Some(Some(img)) = self.images.get(*asset as usize) else { continue };
                    let t0 = self.base.pre_concat(tf(transform));
                    let touched = map_bbox(0.0, 0.0, width.max(0.0), height.max(0.0), &t0, 2);
                    if touched.intersect(&clip.bounds).is_empty() {
                        continue;
                    }
                    let (sx, sy) = image_scale(img, *width, *height);
                    let t = t0.pre_concat(Transform::from_scale(sx, sy));
                    let paint = image_paint(*alpha);
                    let mask = self.draw_mask(&mut clip);
                    let (dst, _, dirty) = top(&mut stack, self.target, full, &mut root_dirty);
                    dst.draw_pixmap(0, 0, img.as_ref(), &paint, t, Some(mask));
                    *dirty = dirty.union(&touched);
                }
            }
        }
        self.finish();
    }
}


/// Composite `src` onto `dst` (same size) over `region` with `blend`, `alpha`
/// and the clip `mask`: `out = lerp(dst, blend(src × alpha, dst), mask)`.
/// Normal, the Porter-Duff operators and the luma mattes (already turned
/// into alpha) run in 8-bit integer math here; the other blend modes go
/// through tiny-skia's pipeline. Dst-in (mattes, mask coverage) applies
/// wherever the clip's coverage is non-zero: the content it cuts was drawn
/// through the same anti-aliased edge, so lerping by coverage again would
/// leave a faint ring of unmatted content along the edge (and tiny-skia's
/// own way, scaling the source by coverage, clears outside the clip).
fn composite(dst: &mut Pixmap, src: &Pixmap, region: &IRect, blend: Blend, alpha: f32, mask: Option<&Mask>) {
    #[derive(Clone, Copy, PartialEq)]
    enum K {
        Over,
        In,
        Out,
        Xor,
    }
    let k = match blend {
        Blend::Normal => K::Over,
        Blend::DstIn | Blend::LumaIn => K::In,
        Blend::DstOut | Blend::LumaOut => K::Out,
        Blend::Xor => K::Xor,
        _ => {
            let paint = Paint {
                shader: tiny_skia::Pattern::new(
                    src.as_ref(),
                    SpreadMode::Pad,
                    tiny_skia::FilterQuality::Nearest,
                    alpha.clamp(0.0, 1.0),
                    Transform::identity(),
                ),
                blend_mode: blend_mode(blend),
                anti_alias: false,
                force_hq_pipeline: false,
            };
            if let Some(r) = tiny_skia::Rect::from_ltrb(region.x0 as f32, region.y0 as f32, region.x1 as f32, region.y1 as f32) {
                dst.fill_rect(r, &paint, Transform::identity(), mask);
            }
            return;
        }
    };
    let o = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    if o == 0 && matches!(k, K::Over | K::Xor) {
        return;
    }
    let stride = dst.width() as usize;
    let sdata = src.data();
    let mdata = mask.map(|m| m.data());
    let ddata = dst.data_mut();
    let (x0, x1) = (region.x0 as usize, region.x1 as usize);
    for y in region.y0..region.y1 {
        let row = y as usize * stride;
        let (a, b) = (row + x0, row + x1);
        let srow = &sdata[a * 4..b * 4];
        let drow = &mut ddata[a * 4..b * 4];
        let mrow = mdata.map(|m| &m[a..b]);
        // The common cases: opaque-alpha source-over or dst-in where the clip covers fully.
        if o == 255 && matches!(k, K::Over | K::In) {
            for (j, (sp, dp)) in srow.chunks_exact(4).zip(drow.chunks_exact_mut(4)).enumerate() {
                let c = mrow.map_or(255, |m| m[j]);
                if c == 0 {
                    continue;
                }
                let sa = sp[3];
                if c == 255 {
                    match (k, sa) {
                        (K::Over, 0) | (K::In, 255) => {}
                        (K::Over, 255) => dp.copy_from_slice(sp),
                        (K::In, 0) => dp.fill(0),
                        (K::Over, _) => {
                            let inv = 255 - sa as u32;
                            for ch in 0..4 {
                                dp[ch] = (sp[ch] as u32 + div255(dp[ch] as u32 * inv)).min(255) as u8;
                            }
                        }
                        _ => {
                            for d in dp.iter_mut() {
                                *d = div255(*d as u32 * sa as u32) as u8;
                            }
                        }
                    }
                    continue;
                }
                partial(k, sp, dp, o, c as u32);
            }
            continue;
        }
        for (j, (sp, dp)) in srow.chunks_exact(4).zip(drow.chunks_exact_mut(4)).enumerate() {
            let c = mrow.map_or(255, |m| m[j] as u32);
            if c != 0 {
                partial(k, sp, dp, o, c);
            }
        }
    }

    /// One pixel, any alpha `o` and coverage `c`.
    fn partial(k: K, sp: &[u8], dp: &mut [u8], o: u32, c: u32) {
        // Alpha and (except dst-in) coverage scale the source, like tiny-skia.
        let so = if k == K::In { o } else { div255(o * c) };
        let sa = div255(sp[3] as u32 * so);
        if sa == 0 && matches!(k, K::Over | K::Xor) {
            return;
        }
        let da = dp[3] as u32;
        for ch in 0..4 {
            let s = if so == 255 { sp[ch] as u32 } else { div255(sp[ch] as u32 * so) };
            let d = dp[ch] as u32;
            let r = match k {
                K::Over => (s + div255(d * (255 - sa))).min(255),
                K::Out => div255(d * (255 - sa)),
                K::Xor => (div255(s * (255 - da)) + div255(d * (255 - sa))).min(255),
                // Wherever the clip reaches (the content under it was drawn
                // through the same edge): outside it the destination stays.
                K::In => div255(d * sa),
            };
            dp[ch] = r as u8;
        }
    }
}

/// Blends [`composite`] runs itself (no mask means full coverage there).
fn fast_blend(blend: Blend) -> bool {
    matches!(blend, Blend::Normal | Blend::DstIn | Blend::LumaIn | Blend::DstOut | Blend::LumaOut | Blend::Xor)
}

/// `x / 255` rounded, exact for `x ≤ 255 · 255`.
fn div255(x: u32) -> u32 {
    let x = x + 128;
    (x + (x >> 8)) >> 8
}

/// The live rect of the innermost surface.
fn top_live(stack: &[Frame<'_>], full: IRect) -> IRect {
    stack
        .iter()
        .rev()
        .find_map(|f| match f {
            Frame::Layer { surface, .. } => Some(surface.live),
            _ => None,
        })
        .unwrap_or(full)
}

/// The innermost surface: its pixmap, live rect and dirty rect.
fn top<'s>(
    stack: &'s mut [Frame<'_>],
    target: &'s mut Pixmap,
    full: IRect,
    root_dirty: &'s mut IRect,
) -> (&'s mut Pixmap, IRect, &'s mut IRect) {
    match stack.iter_mut().rev().find_map(|f| match f {
        Frame::Layer { surface, .. } => Some(surface),
        _ => None,
    }) {
        Some(s) => (&mut s.pixmap, s.live, &mut s.dirty),
        None => (target, full, root_dirty),
    }
}

/// Zero rect `r` of a `stride`-wide buffer with `ch` bytes per pixel.
fn clear_rect(data: &mut [u8], stride: usize, ch: usize, r: &IRect) {
    if r.is_empty() {
        return;
    }
    let row = stride * ch;
    for y in r.y0..r.y1 {
        let o = y as usize * row;
        data[o + r.x0 as usize * ch..o + r.x1 as usize * ch].fill(0);
    }
}

/// Copy rect `r` of `p` into its own pixmap.
fn read_window(p: &Pixmap, r: &IRect) -> Pixmap {
    let mut out = Pixmap::new(r.width(), r.height()).expect("non-zero window");
    let (pw, ow) = (p.width() as usize * 4, r.width() as usize * 4);
    let src = p.data();
    for (y, row) in out.data_mut().chunks_exact_mut(ow).enumerate() {
        let o = (r.y0 as usize + y) * pw + r.x0 as usize * 4;
        row.copy_from_slice(&src[o..o + ow]);
    }
    out
}

fn write_window(p: &mut Pixmap, r: &IRect, w: &Pixmap) {
    let (pw, ow) = (p.width() as usize * 4, r.width() as usize * 4);
    let dst = p.data_mut();
    for (y, row) in w.data().chunks_exact(ow).enumerate() {
        let o = (r.y0 as usize + y) * pw + r.x0 as usize * 4;
        dst[o..o + ow].copy_from_slice(row);
    }
}

/// Apply `filters` in order to `p` (sigmas and offsets in display-list
/// units: × `base`'s scale / linear part).
fn apply_filters(p: &mut Pixmap, filters: &[Filter], base: &Transform, scale: f32) {
    for f in filters {
        match f {
            Filter::Blur { sigma_x, sigma_y } => {
                let (w, h) = (p.width() as usize, p.height() as usize);
                blur(p.data_mut(), w, h, 4, sigma_x * scale, sigma_y * scale);
            }
            Filter::ColorMatrix { m } => color_matrix(p, m),
            Filter::GradientMap { stops, amount } => gradient_map(p, stops, *amount),
            Filter::DropShadow { color, dx, dy, sigma } => {
                let (ddx, ddy) = device_vec(base, *dx, *dy);
                let mut out = Pixmap::new(p.width(), p.height()).expect("non-zero pixmap");
                drop_shadow(p, &mut out, *color, ddx, ddy, sigma * scale);
                *p = out;
            }
        }
    }
}

fn make_stroke(style: &crate::display::StrokeStyle) -> Stroke {
    Stroke {
        width: style.width,
        miter_limit: style.miter,
        line_cap: match style.cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
            Cap::Square => LineCap::Square,
        },
        line_join: match style.join {
            Join::Miter => LineJoin::Miter,
            Join::Round => LineJoin::Round,
            Join::Bevel => LineJoin::Bevel,
        },
        dash: if style.dashes.is_empty() {
            None
        } else {
            let mut d = style.dashes.clone();
            if d.len() % 2 == 1 {
                d.extend_from_within(..);
            }
            StrokeDash::new(d, style.dash_offset)
        },
    }
}

fn image_scale(img: &Pixmap, width: f32, height: f32) -> (f32, f32) {
    let sx = if width > 0.0 { width / img.width() as f32 } else { 1.0 };
    let sy = if height > 0.0 { height / img.height() as f32 } else { 1.0 };
    (sx, sy)
}

fn image_paint(alpha: f32) -> PixmapPaint {
    PixmapPaint { opacity: alpha.clamp(0.0, 1.0), blend_mode: BlendMode::SourceOver, quality: tiny_skia::FilterQuality::Bilinear }
}

// --- The reference painter -------------------------------------------------

/// The straightforward painter: every layer a full-size pixmap, every clip a
/// full-size mask. [`paint_into`] must match it bit for bit.
fn paint_reference(list: &DisplayList, target: &mut Pixmap, base: Transform, images: Images<'_>) {
    enum RFrame<'l> {
        Layer { pixmap: Pixmap, blend: Blend, alpha: f32, filters: &'l [Filter], clip: Option<Mask>, saved: Option<Mask> },
        Clip { saved: Option<Mask> },
    }
    let (w, h) = (target.width(), target.height());
    let scale = (base.sx * base.sy - base.kx * base.ky).abs().sqrt();
    let mut stack: Vec<RFrame> = Vec::new();
    let mut clip: Option<Mask> = None;
    let intersect = |current: &Option<Mask>, path: &[Verb], t: Transform| -> Option<Mask> {
        let mut mask = match current {
            Some(m) => m.clone(),
            None => {
                let mut m = Mask::new(w, h)?;
                m.data_mut().fill(255);
                m
            }
        };
        match build(path) {
            Some(p) => mask.intersect_path(&p, tiny_skia::FillRule::Winding, true, t),
            None => mask.data_mut().fill(0),
        }
        Some(mask)
    };
    macro_rules! top {
        () => {
            match stack.iter_mut().rev().find_map(|f| match f {
                RFrame::Layer { pixmap, .. } => Some(pixmap),
                _ => None,
            }) {
                Some(p) => p,
                None => &mut *target,
            }
        };
    }
    for op in &list.ops {
        match op {
            Op::Layer { blend, alpha, transform, clip: path, filters } => {
                let layer_clip = intersect(&clip, path, base.pre_concat(tf(transform)));
                let saved = std::mem::replace(&mut clip, layer_clip.clone());
                stack.push(RFrame::Layer {
                    pixmap: Pixmap::new(w, h).expect("non-zero pixmap"),
                    blend: *blend,
                    alpha: *alpha,
                    filters,
                    clip: layer_clip,
                    saved,
                });
            }
            Op::Clip { transform, path } => {
                let next = intersect(&clip, path, base.pre_concat(tf(transform)));
                let saved = std::mem::replace(&mut clip, next);
                stack.push(RFrame::Clip { saved });
            }
            Op::Pop => match stack.pop() {
                Some(RFrame::Clip { saved }) => clip = saved,
                Some(RFrame::Layer { mut pixmap, blend, alpha, filters, clip: layer_clip, saved }) => {
                    clip = saved;
                    if matches!(blend, Blend::LumaIn | Blend::LumaOut) {
                        luma_mask(&mut pixmap);
                    }
                    apply_filters(&mut pixmap, filters, &base, scale);
                    composite(top!(), &pixmap, &IRect::wh(0, 0, w, h), blend, alpha, layer_clip.as_ref());
                }
                None => {}
            },
            Op::Fill { transform, rule, brush, path } => {
                let (Some(p), Some(paint)) = (build(path), make_paint(brush)) else { continue };
                top!().fill_path(&p, &paint, fill_rule(*rule), base.pre_concat(tf(transform)), clip.as_ref());
            }
            Op::Stroke { transform, style, brush, path } => {
                let (Some(p), Some(paint)) = (build(path), make_paint(brush)) else { continue };
                top!().stroke_path(&p, &paint, &make_stroke(style), base.pre_concat(tf(transform)), clip.as_ref());
            }
            Op::Image { transform, asset, alpha, width, height } => {
                let Some(Some(img)) = images.get(*asset as usize) else { continue };
                let (sx, sy) = image_scale(img, *width, *height);
                let t = base.pre_concat(tf(transform)).pre_concat(Transform::from_scale(sx, sy));
                top!().draw_pixmap(0, 0, img.as_ref(), &image_paint(*alpha), t, clip.as_ref());
            }
        }
    }
}

fn tf(t: &[f32; 6]) -> Transform {
    Transform::from_row(t[0], t[1], t[2], t[3], t[4], t[5])
}

fn fill_rule(r: FillRule) -> tiny_skia::FillRule {
    match r {
        FillRule::Nonzero => tiny_skia::FillRule::Winding,
        FillRule::Evenodd => tiny_skia::FillRule::EvenOdd,
    }
}

pub(crate) fn build(path: &[Verb]) -> Option<Path> {
    let mut pb = PathBuilder::with_capacity(path.len(), path.len() * 3);
    for v in path {
        match *v {
            Verb::M { x, y } => pb.move_to(x, y),
            Verb::L { x, y } => pb.line_to(x, y),
            Verb::C { x1, y1, x2, y2, x, y } => pb.cubic_to(x1, y1, x2, y2, x, y),
            Verb::Z => pb.close(),
        }
    }
    pb.finish()
}

fn color(c: [f32; 4]) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), c[3].clamp(0.0, 1.0))
        .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

fn stops(s: &[Stop]) -> Vec<GradientStop> {
    s.iter().map(|s| GradientStop::new(s.offset, color(s.color))).collect()
}

fn make_paint(brush: &Brush) -> Option<Paint<'static>> {
    let shader = match brush {
        Brush::Solid { color: c } => Shader::SolidColor(color(*c)),
        Brush::Linear { start, end, stops: s } => LinearGradient::new(
            Point::from_xy(start[0], start[1]),
            Point::from_xy(end[0], end[1]),
            stops(s),
            SpreadMode::Pad,
            Transform::identity(),
        )?,
        Brush::Radial { center, focal, radius, stops: s } => RadialGradient::new(
            Point::from_xy(focal[0], focal[1]),
            Point::from_xy(center[0], center[1]),
            *radius,
            stops(s),
            SpreadMode::Pad,
            Transform::identity(),
        )?,
    };
    Some(Paint { shader, anti_alias: true, ..Paint::default() })
}

fn blend_mode(b: Blend) -> BlendMode {
    match b {
        Blend::Normal => BlendMode::SourceOver,
        Blend::Multiply => BlendMode::Multiply,
        Blend::Screen => BlendMode::Screen,
        Blend::Overlay => BlendMode::Overlay,
        Blend::Darken => BlendMode::Darken,
        Blend::Lighten => BlendMode::Lighten,
        Blend::ColorDodge => BlendMode::ColorDodge,
        Blend::ColorBurn => BlendMode::ColorBurn,
        Blend::HardLight => BlendMode::HardLight,
        Blend::SoftLight => BlendMode::SoftLight,
        Blend::Difference => BlendMode::Difference,
        Blend::Exclusion => BlendMode::Exclusion,
        Blend::Hue => BlendMode::Hue,
        Blend::Saturation => BlendMode::Saturation,
        Blend::Color => BlendMode::Color,
        Blend::Luminosity => BlendMode::Luminosity,
        Blend::DstIn => BlendMode::DestinationIn,
        Blend::DstOut => BlendMode::DestinationOut,
        Blend::Xor => BlendMode::Xor,
        // Luma blends convert the layer to a luminance mask first (`luma_mask`).
        Blend::LumaIn => BlendMode::DestinationIn,
        Blend::LumaOut => BlendMode::DestinationOut,
    }
}

/// Replace a premultiplied layer by a mask whose alpha is its luminance
/// (Rec. 709 on premultiplied RGB: the matte over transparent black).
fn luma_mask(p: &mut Pixmap) {
    for px in p.pixels_mut() {
        let l = 0.2125 * px.red() as f32 + 0.7154 * px.green() as f32 + 0.0721 * px.blue() as f32;
        let a = l.round().clamp(0.0, 255.0) as u8;
        *px = tiny_skia::PremultipliedColorU8::from_rgba(a, a, a, a).unwrap();
    }
}

// --- Filters ---------------------------------------------------------------

fn is_identity(m: &[f32; 20]) -> bool {
    *m == crate::effects::IDENTITY_MATRIX
}

/// SVG's three-box approximation size for `sigma` (`feGaussianBlur`).
fn box_size(sigma: f32) -> usize {
    (sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0 + 0.5).floor().max(0.0) as usize
}

/// Below this sigma the blur is a true gaussian kernel, above it three box blurs.
const BOX_SIGMA: f32 = 2.0;

/// How far (pixels) a blur of `sigma` spreads content, per side.
fn blur_extent(sigma: f32) -> i32 {
    if !(sigma > 0.0) {
        return 0;
    }
    if !sigma.is_finite() {
        return BIG;
    }
    if sigma < BOX_SIGMA {
        return (3.0 * sigma).ceil() as i32;
    }
    (3 * box_size(sigma) / 2 + 1).min(BIG as usize) as i32
}

/// Gaussian blur of a `w × h` buffer with `ch` interleaved channels
/// (premultiplied), transparent outside. Separable: horizontal then vertical.
fn blur(buf: &mut [u8], w: usize, h: usize, ch: usize, sigma_x: f32, sigma_y: f32) {
    if w == 0 || h == 0 {
        return;
    }
    let mut tmp = Vec::new();
    if sigma_x > 0.0 && sigma_x.is_finite() {
        for pass in passes(sigma_x) {
            match pass {
                Pass::Box(l, r) => box_h(buf, w, h, ch, l, r, &mut tmp),
                Pass::Kernel(k) => kernel_h(buf, w, h, ch, &k, &mut tmp),
            }
        }
    }
    if sigma_y > 0.0 && sigma_y.is_finite() {
        for pass in passes(sigma_y) {
            match pass {
                Pass::Box(l, r) => box_v(buf, w, h, ch, l, r, &mut tmp),
                Pass::Kernel(k) => kernel_v(buf, w, h, ch, &k, &mut tmp),
            }
        }
    }
}

enum Pass {
    /// A box over `[i − l, i + r]`.
    Box(usize, usize),
    /// A symmetric kernel, 16.16 weights summing to 65536, radius = len / 2.
    Kernel(Vec<u32>),
}

fn passes(sigma: f32) -> Vec<Pass> {
    if sigma < BOX_SIGMA {
        let r = (3.0 * sigma).ceil() as usize;
        let ws: Vec<f32> = (0..=2 * r)
            .map(|i| {
                let x = i as f32 - r as f32;
                (-x * x / (2.0 * sigma * sigma)).exp()
            })
            .collect();
        let sum: f32 = ws.iter().sum();
        let mut k: Vec<u32> = ws.iter().map(|w| (w / sum * 65536.0).round() as u32).collect();
        let total: u32 = k.iter().sum();
        k[r] = (k[r] as i64 + 65536 - total as i64).max(0) as u32;
        return vec![Pass::Kernel(k)];
    }
    let d = box_size(sigma);
    if d <= 1 {
        return Vec::new();
    }
    if d % 2 == 1 {
        let r = d / 2;
        vec![Pass::Box(r, r), Pass::Box(r, r), Pass::Box(r, r)]
    } else {
        let r = d / 2;
        vec![Pass::Box(r, r - 1), Pass::Box(r - 1, r), Pass::Box(r, r)]
    }
}

fn box_h(buf: &mut [u8], w: usize, h: usize, ch: usize, l: usize, r: usize, tmp: &mut Vec<u8>) {
    let d = (l + r + 1) as u32;
    let half = d / 2;
    let stride = w * ch;
    tmp.clear();
    tmp.resize(stride, 0);
    for y in 0..h {
        let row = &mut buf[y * stride..(y + 1) * stride];
        tmp.copy_from_slice(row);
        for c in 0..ch {
            // Window [x − l, x + r]; start with x = 0.
            let mut sum: u32 = (0..=r.min(w - 1)).map(|i| tmp[i * ch + c] as u32).sum();
            for x in 0..w {
                row[x * ch + c] = ((sum + half) / d) as u8;
                let add = x + r + 1;
                if add < w {
                    sum += tmp[add * ch + c] as u32;
                }
                if x >= l {
                    sum -= tmp[(x - l) * ch + c] as u32;
                }
            }
        }
    }
}

fn box_v(buf: &mut [u8], w: usize, h: usize, ch: usize, l: usize, r: usize, tmp: &mut Vec<u8>) {
    let d = (l + r + 1) as u32;
    let half = d / 2;
    let stride = w * ch;
    tmp.clear();
    tmp.extend_from_slice(buf);
    let mut sums = vec![0u32; stride];
    for y in 0..=r.min(h - 1) {
        for (s, v) in sums.iter_mut().zip(&tmp[y * stride..(y + 1) * stride]) {
            *s += *v as u32;
        }
    }
    for y in 0..h {
        let out = &mut buf[y * stride..(y + 1) * stride];
        for (o, s) in out.iter_mut().zip(&sums) {
            *o = ((s + half) / d) as u8;
        }
        let add = y + r + 1;
        if add < h {
            for (s, v) in sums.iter_mut().zip(&tmp[add * stride..(add + 1) * stride]) {
                *s += *v as u32;
            }
        }
        if y >= l {
            let sub = y - l;
            for (s, v) in sums.iter_mut().zip(&tmp[sub * stride..(sub + 1) * stride]) {
                *s -= *v as u32;
            }
        }
    }
}

fn kernel_h(buf: &mut [u8], w: usize, h: usize, ch: usize, k: &[u32], tmp: &mut Vec<u8>) {
    let r = k.len() / 2;
    let stride = w * ch;
    tmp.clear();
    tmp.resize(stride, 0);
    for y in 0..h {
        let row = &mut buf[y * stride..(y + 1) * stride];
        tmp.copy_from_slice(row);
        for x in 0..w {
            let lo = x.saturating_sub(r);
            let hi = (x + r).min(w - 1);
            for c in 0..ch {
                let mut acc: u32 = 0;
                for i in lo..=hi {
                    acc += k[i + r - x] * tmp[i * ch + c] as u32;
                }
                row[x * ch + c] = ((acc + 32768) >> 16).min(255) as u8;
            }
        }
    }
}

fn kernel_v(buf: &mut [u8], w: usize, h: usize, ch: usize, k: &[u32], tmp: &mut Vec<u8>) {
    let r = k.len() / 2;
    let stride = w * ch;
    tmp.clear();
    tmp.extend_from_slice(buf);
    let mut acc = vec![0u32; stride];
    for y in 0..h {
        acc.fill(0);
        let lo = y.saturating_sub(r);
        let hi = (y + r).min(h - 1);
        for i in lo..=hi {
            let wgt = k[i + r - y];
            for (a, v) in acc.iter_mut().zip(&tmp[i * stride..(i + 1) * stride]) {
                *a += wgt * *v as u32;
            }
        }
        for (o, a) in buf[y * stride..(y + 1) * stride].iter_mut().zip(&acc) {
            *o = ((a + 32768) >> 16).min(255) as u8;
        }
    }
}

/// SVG `feColorMatrix` on un-premultiplied RGBA (offsets in `0..=1`).
fn color_matrix(p: &mut Pixmap, m: &[f32; 20]) {
    if is_identity(m) {
        return;
    }
    let clear_alpha = (m[19] * 255.0).round().clamp(0.0, 255.0);
    for px in p.data_mut().chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 && clear_alpha == 0.0 {
            continue;
        }
        let (r, g, b, af) = if a == 0 {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            let a = a as f32;
            (px[0] as f32 / a, px[1] as f32 / a, px[2] as f32 / a, a / 255.0)
        };
        let row = |k: usize| (m[k] * r + m[k + 1] * g + m[k + 2] * b + m[k + 3] * af + m[k + 4]).clamp(0.0, 1.0);
        let (nr, ng, nb, na) = (row(0), row(5), row(10), row(15));
        let a8 = (na * 255.0).round();
        px[0] = (nr * a8).round().min(a8) as u8;
        px[1] = (ng * a8).round().min(a8) as u8;
        px[2] = (nb * a8).round().min(a8) as u8;
        px[3] = a8 as u8;
    }
}

/// Map each pixel's Rec. 709 luminance through `stops`, then mix with the
/// original by `amount` (alpha × the stop's alpha, mixed the same way).
fn gradient_map(p: &mut Pixmap, stops: &[Stop], amount: f32) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || stops.is_empty() {
        return;
    }
    let mut sorted: Vec<Stop> = stops.to_vec();
    sorted.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let lut: Vec<[f32; 4]> = (0..256)
        .map(|i| {
            let t = i as f32 / 255.0;
            let first = &sorted[0];
            let last = &sorted[sorted.len() - 1];
            if t <= first.offset {
                return first.color;
            }
            if t >= last.offset {
                return last.color;
            }
            for w in sorted.windows(2) {
                if t <= w[1].offset {
                    let span = w[1].offset - w[0].offset;
                    let f = if span > 0.0 { (t - w[0].offset) / span } else { 1.0 };
                    let (a, b) = (w[0].color, w[1].color);
                    return [
                        a[0] + (b[0] - a[0]) * f,
                        a[1] + (b[1] - a[1]) * f,
                        a[2] + (b[2] - a[2]) * f,
                        a[3] + (b[3] - a[3]) * f,
                    ];
                }
            }
            last.color
        })
        .collect();
    for px in p.data_mut().chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 {
            continue;
        }
        let af = a as f32;
        let (r, g, b) = (px[0] as f32 / af, px[1] as f32 / af, px[2] as f32 / af);
        let l = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);
        let c = lut[(l * 255.0).round() as usize];
        let mix = |o: f32, n: f32| (o + (n.clamp(0.0, 1.0) - o) * amount).clamp(0.0, 1.0);
        let na = (af / 255.0 * (1.0 + (c[3].clamp(0.0, 1.0) - 1.0) * amount)).clamp(0.0, 1.0);
        let a8 = (na * 255.0).round();
        px[0] = (mix(r, c[0]) * a8).round().min(a8) as u8;
        px[1] = (mix(g, c[1]) * a8).round().min(a8) as u8;
        px[2] = (mix(b, c[2]) * a8).round().min(a8) as u8;
        px[3] = a8 as u8;
    }
}

/// `content` over its shadow (its alpha blurred by `sigma`, shifted by
/// `(dx, dy)` device pixels with bilinear sub-pixel weights, tinted
/// `color`) into `out` (same size, zeroed).
fn drop_shadow(content: &Pixmap, out: &mut Pixmap, color: [f32; 4], dx: f32, dy: f32, sigma: f32) {
    let (w, h) = (content.width() as usize, content.height() as usize);
    let alpha: Vec<u8> = content.data().chunks_exact(4).map(|p| p[3]).collect();
    let (dx, dy) = if dx.is_finite() && dy.is_finite() { (dx, dy) } else { (0.0, 0.0) };
    let (ix, iy) = (dx.floor(), dy.floor());
    let (fx, fy) = (dx - ix, dy - iy);
    let (ix, iy) = (ix.clamp(-(BIG as f32), BIG as f32) as i64, iy.clamp(-(BIG as f32), BIG as f32) as i64);
    // Shift first (bilinear weights of the four source pixels, 0..=256),
    // then blur: the blur's spread then stays inside the bounded surface.
    let wx1 = (fx * 256.0).round() as u32;
    let wy1 = (fy * 256.0).round() as u32;
    let (wx0, wy0) = (256 - wx1, 256 - wy1);
    let at = |x: i64, y: i64| -> u32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            0
        } else {
            alpha[y as usize * w + x as usize] as u32
        }
    };
    let mut shifted = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            // out(x) = src(x − dx): src(x − ix) weighted 1 − fx, src(x − ix − 1) weighted fx.
            let sx = x as i64 - ix;
            let sy = y as i64 - iy;
            let v = (at(sx, sy) * wx0 + at(sx - 1, sy) * wx1) * wy0 + (at(sx, sy - 1) * wx0 + at(sx - 1, sy - 1) * wx1) * wy1;
            shifted[y * w + x] = ((v + 32768) >> 16).min(255) as u8;
        }
    }
    blur(&mut shifted, w, h, 1, sigma, sigma);
    let ca = color[3].clamp(0.0, 1.0);
    let rgb = [color[0].clamp(0.0, 1.0), color[1].clamp(0.0, 1.0), color[2].clamp(0.0, 1.0)];
    let data = out.data_mut();
    for (k, a) in shifted.iter().enumerate() {
        if *a == 0 {
            continue;
        }
        let a8 = (*a as f32 * ca).round().min(255.0);
        let o = k * 4;
        data[o] = (rgb[0] * a8).round() as u8;
        data[o + 1] = (rgb[1] * a8).round() as u8;
        data[o + 2] = (rgb[2] * a8).round() as u8;
        data[o + 3] = a8 as u8;
    }
    out.draw_pixmap(0, 0, content.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{Op, IDENTITY};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Verb> {
        vec![
            Verb::M { x, y },
            Verb::L { x: x + w, y },
            Verb::L { x: x + w, y: y + h },
            Verb::L { x, y: y + h },
            Verb::Z,
        ]
    }

    fn circle(cx: f32, cy: f32, r: f32) -> Vec<Verb> {
        let k = 0.5523 * r;
        vec![
            Verb::M { x: cx, y: cy - r },
            Verb::C { x1: cx + k, y1: cy - r, x2: cx + r, y2: cy - k, x: cx + r, y: cy },
            Verb::C { x1: cx + r, y1: cy + k, x2: cx + k, y2: cy + r, x: cx, y: cy + r },
            Verb::C { x1: cx - k, y1: cy + r, x2: cx - r, y2: cy + k, x: cx - r, y: cy },
            Verb::C { x1: cx - r, y1: cy - k, x2: cx - k, y2: cy - r, x: cx, y: cy - r },
            Verb::Z,
        ]
    }

    fn fill(path: Vec<Verb>, c: [f32; 4]) -> Op {
        Op::Fill { transform: IDENTITY, rule: FillRule::Nonzero, brush: Brush::Solid { color: c }, path }
    }

    fn layer(blend: Blend, alpha: f32, clip: Vec<Verb>, filters: Vec<Filter>) -> Op {
        Op::Layer { blend, alpha, transform: IDENTITY, clip, filters }
    }

    fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = p.pixel(x, y).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        for d in p.data_mut().chunks_exact_mut(4) {
            d.copy_from_slice(&c);
        }
        p
    }

    /// Paint with both painters, assert identical, return the bounded one.
    fn both(list: &DisplayList, w: u32, h: u32) -> Pixmap {
        let base = fit(list, w as f32, h as f32);
        let mut a = Pixmap::new(w, h).unwrap();
        paint_into(list, &mut a, base, &[]);
        let mut b = Pixmap::new(w, h).unwrap();
        paint_into_unbounded(list, &mut b, base, &[]);
        let diff = a.data().iter().zip(b.data()).filter(|(x, y)| x != y).count();
        for (i, (x, y)) in a.data().chunks(4).zip(b.data().chunks(4)).enumerate().filter(|(_, (x, y))| x != y).take(8) {
            eprintln!("DIFF {w}x{h} ({}, {}) bounded {x:?} unbounded {y:?}", i as u32 % w, i as u32 / w);
        }
        assert_eq!(diff, 0, "bounded and unbounded differ in {diff} bytes");
        a
    }

    #[test]
    fn integer_rects_clip_exactly() {
        // The no-mask fast path relies on an integer rect's coverage being 0/255.
        let mut m = Mask::new(20, 20).unwrap();
        m.fill_path(&build(&rect(3.0, 4.0, 10.0, 7.0)).unwrap(), tiny_skia::FillRule::Winding, true, Transform::identity());
        assert!(m.data().iter().all(|v| *v == 0 || *v == 255));
        assert_eq!(m.data().iter().filter(|v| **v == 255).count(), 70);
        let exact = |p: &[Verb], t: Transform| axis_rect(p, &t).and_then(|r| r.exact());
        assert_eq!(exact(&rect(1.0, 2.0, 3.0, 4.0), Transform::from_row(2.0, 0.0, 0.0, 2.0, 1.0, 0.0)), Some(IRect { x0: 3, y0: 4, x1: 9, y1: 12 }));
        assert_eq!(exact(&rect(1.0, 2.0, 3.0, 4.0), Transform::from_row(2.0, 0.0, 0.0, 2.0, 0.5, 0.0)), None);
        assert!(axis_rect(&circle(5.0, 5.0, 3.0), &Transform::identity()).is_none());
        // A fractional rect covers its interior pixels fully.
        let frac = rect(2.3, 3.6, 10.2, 8.1);
        let r = axis_rect(&frac, &Transform::identity()).unwrap();
        assert_eq!(r.interior(), IRect { x0: 3, y0: 4, x1: 12, y1: 11 });
        let mut m = Mask::new(20, 20).unwrap();
        m.fill_path(&build(&frac).unwrap(), tiny_skia::FillRule::Winding, true, Transform::identity());
        for y in 4..11 {
            for x in 3..12 {
                assert_eq!(m.data()[y * 20 + x], 255, "({x}, {y})");
            }
        }
    }

    #[test]
    fn matte_bounded_matches_unbounded() {
        // An alpha matte, an inverted luma matte and a feathered mask group, under a fractional fit.
        let full = rect(0.0, 0.0, 100.0, 80.0);
        let list = DisplayList {
            width: 100.0,
            height: 80.0,
            ops: vec![
                Op::Clip { transform: IDENTITY, path: full.clone() },
                fill(rect(0.0, 0.0, 100.0, 80.0), [0.1, 0.1, 0.3, 1.0]),
                layer(Blend::Normal, 1.0, full.clone(), vec![]),
                fill(circle(30.0, 30.0, 20.0), [1.0, 0.0, 0.0, 1.0]),
                layer(Blend::DstIn, 1.0, full.clone(), vec![]),
                fill(rect(20.5, 10.25, 30.0, 25.0), [1.0, 1.0, 1.0, 0.8]),
                Op::Pop,
                Op::Pop,
                layer(Blend::Normal, 0.7, full.clone(), vec![]),
                fill(circle(70.0, 50.0, 15.0), [0.0, 1.0, 0.0, 1.0]),
                layer(Blend::LumaOut, 1.0, full.clone(), vec![]),
                fill(circle(75.0, 45.0, 8.0), [0.9, 0.9, 0.9, 1.0]),
                Op::Pop,
                Op::Pop,
                layer(Blend::Multiply, 1.0, circle(50.0, 40.0, 30.0), vec![]),
                Op::Clip { transform: [1.0, 0.0, 0.0, 1.0, 0.3, 0.7], path: rect(10.0, 10.0, 60.0, 50.0) },
                fill(rect(0.0, 30.0, 100.0, 20.0), [1.0, 1.0, 0.0, 1.0]),
                layer(Blend::DstIn, 1.0, full.clone(), vec![Filter::Blur { sigma_x: 3.0, sigma_y: 3.0 }]),
                fill(circle(50.0, 40.0, 20.0), [1.0, 1.0, 1.0, 1.0]),
                Op::Pop,
                Op::Pop,
                Op::Pop,
                // A layer entirely outside the frame is skipped.
                layer(Blend::Normal, 1.0, rect(200.0, 200.0, 10.0, 10.0), vec![]),
                fill(rect(200.0, 200.0, 10.0, 10.0), [1.0, 1.0, 1.0, 1.0]),
                Op::Pop,
                Op::Pop,
            ],
        };
        for (w, h) in [(100, 80), (333, 211), (64, 64)] {
            let p = both(&list, w, h);
            assert!(p.data().chunks_exact(4).any(|c| c[0] > 200 && c[1] < 50), "red survives the matte");
        }
    }

    #[test]
    fn effects_bounded_match_unbounded() {
        let full = rect(0.0, 0.0, 100.0, 80.0);
        let list = DisplayList {
            width: 100.0,
            height: 80.0,
            ops: vec![
                Op::Clip { transform: IDENTITY, path: full.clone() },
                layer(
                    Blend::Normal,
                    1.0,
                    full.clone(),
                    vec![
                        Filter::DropShadow { color: [0.0, 0.0, 0.0, 0.6], dx: 3.3, dy: -4.7, sigma: 2.5 },
                        Filter::GradientMap { stops: vec![Stop { offset: 0.0, color: [0.0, 0.0, 1.0, 1.0] }, Stop { offset: 1.0, color: [1.0, 1.0, 0.0, 1.0] }], amount: 0.5 },
                    ],
                ),
                fill(circle(30.0, 40.0, 12.0), [1.0, 0.0, 0.0, 1.0]),
                Op::Pop,
                layer(Blend::Screen, 0.8, full.clone(), vec![Filter::Blur { sigma_x: 1.2, sigma_y: 6.0 }]),
                fill(rect(60.0, 20.0, 20.0, 30.0), [0.0, 0.8, 0.2, 1.0]),
                Op::Pop,
                layer(Blend::Normal, 1.0, full.clone(), vec![Filter::ColorMatrix { m: [0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.0] }]),
                fill(circle(70.0, 65.0, 6.0), [1.0, 0.0, 0.0, 1.0]),
                Op::Pop,
                Op::Pop,
            ],
        };
        both(&list, 100, 80);
        both(&list, 257, 199);
    }

    #[test]
    fn composite_matches_tiny_skia() {
        // Every (src, dst, mask, alpha) mix on a 16×16 grid against tiny-skia's pipeline.
        let (w, h) = (16u32, 16u32);
        let mut src = Pixmap::new(w, h).unwrap();
        let mut dst = Pixmap::new(w, h).unwrap();
        let mut mask = Mask::new(w, h).unwrap();
        for i in 0..(w * h) as usize {
            let (sa, da) = ((i * 17 % 256) as u8, (i * 29 % 256) as u8);
            let pre = |a: u8, k: usize| [(a as usize * (k % 7) / 6) as u8, (a as usize * (k % 5) / 4) as u8, (a as usize * (k % 3) / 2) as u8, a];
            src.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&pre(sa, i));
            dst.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&pre(da, i + 3));
            mask.data_mut()[i] = [0, 255, 128, 37][i % 4];
        }
        let tiny = |blend: Blend, alpha: f32, m: Option<&Mask>| {
            let mut theirs = dst.clone();
            let paint = PixmapPaint { opacity: alpha, blend_mode: blend_mode(blend), quality: tiny_skia::FilterQuality::Nearest };
            theirs.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::identity(), m);
            theirs
        };
        for blend in [Blend::Normal, Blend::DstIn, Blend::DstOut, Blend::Xor] {
            for alpha in [1.0, 0.6] {
                for m in [None, Some(&mask)] {
                    let mut ours = dst.clone();
                    composite(&mut ours, &src, &IRect::wh(0, 0, w, h), blend, alpha, m);
                    let theirs = match (blend, m) {
                        // dst-in: tiny-skia's unclipped result wherever the clip reaches.
                        (Blend::DstIn, Some(m)) => {
                            let mut t = tiny(blend, alpha, None);
                            for (i, px) in t.data_mut().chunks_exact_mut(4).enumerate() {
                                if m.data()[i] == 0 {
                                    px.copy_from_slice(&dst.data()[i * 4..i * 4 + 4]);
                                }
                            }
                            t
                        }
                        _ => tiny(blend, alpha, m),
                    };
                    let worst = ours.data().iter().zip(theirs.data()).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
                    assert!(worst <= 1, "{blend:?} {alpha} mask {}: off by {worst}", m.is_some());
                }
            }
        }
    }

    #[test]
    fn blur_spreads_and_conserves() {
        for sigma in [1.0f32, 4.0] {
            let mut p = Pixmap::new(41, 41).unwrap();
            for y in 18..23 {
                for x in 18..23 {
                    p.data_mut()[(y * 41 + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
            let before: u32 = p.data().iter().map(|v| *v as u32).sum();
            blur(p.data_mut(), 41, 41, 4, sigma, sigma);
            let after: u32 = p.data().iter().map(|v| *v as u32).sum();
            assert!((before as f32 - after as f32).abs() / (before as f32) < 0.05, "{sigma}: {before} {after}");
            assert!(px(&p, 20, 20)[3] < 255, "{sigma}: centre softened");
            let e = blur_extent(sigma) as u32;
            assert!(px(&p, 20 + 2 + e / 2, 20)[3] > 0, "{sigma}: spread");
            assert_eq!(px(&p, 20 + 3 + e, 20)[3], 0, "{sigma}: extent bounds the spread");
            // Premultiplied stays valid and symmetric.
            let (l, r) = (px(&p, 17, 20), px(&p, 23, 20));
            assert!(l.iter().zip(r).all(|(a, b)| (*a as i32 - b as i32).abs() <= 1), "{l:?} {r:?}");
            assert!(p.data().chunks_exact(4).all(|c| c[0] <= c[3]));
        }
        // Horizontal only leaves rows apart untouched.
        let mut p = Pixmap::new(9, 9).unwrap();
        p.data_mut()[(4 * 9 + 4) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
        blur(p.data_mut(), 9, 9, 4, 1.0, 0.0);
        assert_eq!(px(&p, 4, 3), [0, 0, 0, 0]);
        assert!(px(&p, 3, 4)[3] > 0);
    }

    #[test]
    fn color_matrix_works_on_straight_rgba() {
        // Half-transparent red → the Fill effect's matrix: green at alpha × 0.5.
        let mut p = solid(2, 2, [128, 0, 0, 128]);
        #[rustfmt::skip]
        let m = [0.0, 0.0, 0.0, 0.0, 0.0,  0.0, 0.0, 0.0, 0.0, 1.0,  0.0, 0.0, 0.0, 0.0, 0.0,  0.0, 0.0, 0.0, 0.5, 0.0];
        color_matrix(&mut p, &m);
        assert_eq!(px(&p, 0, 0), [0, 64, 0, 64]);
        // Identity is a no-op; transparent stays transparent unless the alpha row adds.
        let mut t = solid(1, 1, [10, 20, 30, 40]);
        color_matrix(&mut t, &crate::effects::IDENTITY_MATRIX);
        assert_eq!(px(&t, 0, 0), [10, 20, 30, 40]);
        let mut z = Pixmap::new(1, 1).unwrap();
        color_matrix(&mut z, &m);
        assert_eq!(px(&z, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn gradient_map_maps_luminance() {
        let stops = vec![Stop { offset: 0.0, color: [1.0, 0.0, 0.0, 1.0] }, Stop { offset: 1.0, color: [0.0, 0.0, 1.0, 1.0] }];
        let mut p = Pixmap::new(3, 1).unwrap();
        p.data_mut().copy_from_slice(&[0, 0, 0, 255, 255, 255, 255, 255, 64, 64, 64, 128]);
        gradient_map(&mut p, &stops, 1.0);
        assert_eq!(px(&p, 0, 0), [255, 0, 0, 255]); // black → first stop
        assert_eq!(px(&p, 1, 0), [0, 0, 255, 255]); // white → last stop
        let mid = px(&p, 2, 0); // half-grey at alpha 128 → purple, alpha kept
        assert_eq!(mid[3], 128);
        assert!((mid[0] as i32 - 64).abs() <= 1 && (mid[2] as i32 - 64).abs() <= 1, "{mid:?}");
        // amount mixes with the original.
        let mut q = solid(1, 1, [255, 255, 255, 255]);
        gradient_map(&mut q, &stops, 0.5);
        assert_eq!(px(&q, 0, 0), [128, 128, 255, 255]);
    }

    #[test]
    fn drop_shadow_goes_under_and_offsets() {
        // A 2×2 white square at (2,2) in 10×10, shadow red, dx 3 dy 0, no blur.
        let mut content = Pixmap::new(10, 10).unwrap();
        for y in 2..4 {
            for x in 2..4 {
                content.data_mut()[(y * 10 + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let mut out = Pixmap::new(10, 10).unwrap();
        drop_shadow(&content, &mut out, [1.0, 0.0, 0.0, 1.0], 3.0, 0.0, 0.0);
        assert_eq!(px(&out, 2, 2), [255, 255, 255, 255], "content on top");
        assert_eq!(px(&out, 5, 2), [255, 0, 0, 255], "shadow offset");
        assert_eq!(px(&out, 6, 2), [255, 0, 0, 255]);
        assert_eq!(px(&out, 7, 2), [0, 0, 0, 0]);
        // Half-pixel offset splits the shadow across two pixels.
        let mut out = Pixmap::new(10, 10).unwrap();
        drop_shadow(&content, &mut out, [0.0, 0.0, 0.0, 1.0], 2.5, 0.0, 0.0);
        assert_eq!(px(&out, 4, 2)[3], 128);
        assert_eq!(px(&out, 5, 2)[3], 255);
        assert_eq!(px(&out, 6, 2)[3], 128);
        assert_eq!(px(&out, 7, 2)[3], 0);
    }

    #[test]
    fn layer_filters_apply_before_compositing() {
        let full = rect(0.0, 0.0, 20.0, 20.0);
        let list = DisplayList {
            width: 20.0,
            height: 20.0,
            ops: vec![
                layer(Blend::Normal, 1.0, full.clone(), vec![Filter::DropShadow { color: [0.0, 0.0, 1.0, 1.0], dx: 4.0, dy: 4.0, sigma: 0.0 }]),
                fill(rect(2.0, 2.0, 4.0, 4.0), [1.0, 0.0, 0.0, 1.0]),
                Op::Pop,
            ],
        };
        let p = both(&list, 40, 40); // 2× fit: the offset scales to 8 px
        assert_eq!(px(&p, 5, 5), [255, 0, 0, 255]);
        assert_eq!(px(&p, 18, 18), [0, 0, 255, 255]);
        assert_eq!(px(&p, 21, 21), [0, 0, 0, 0]);
    }

    #[test]
    fn decodes_data_url_images() {
        use base64::Engine;
        // A 1×1 PNG, half-transparent white, encoded on the fly.
        let mut png = Vec::new();
        image::RgbaImage::from_raw(1, 1, vec![255, 255, 255, 128])
            .unwrap()
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
        let p = decode_location(&url, None).expect("decodes");
        assert_eq!(px(&p, 0, 0), [128, 128, 128, 128], "premultiplied");
        assert!(decode_location("data:image/png;base64,!!!", None).is_none());
        let dir = std::env::temp_dir().join(format!("eui-motion-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), &png).unwrap();
        assert!(decode_location("a.png", Some(&dir)).is_some());
        assert!(decode_location("missing.png", Some(&dir)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bounded == unbounded over the spike corpus (`EUI_MOTION_CORPUS=dir`).
    #[test]
    #[ignore]
    fn corpus_bounded_matches_unbounded() {
        let Ok(dir) = std::env::var("EUI_MOTION_CORPUS") else { return };
        let only = std::env::var("ONLY").ok();
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        let unlimited = crate::Limits { max_bytes: usize::MAX, max_shapes: usize::MAX, max_seconds: f64::MAX, max_vertices: usize::MAX };
        let mut failed = 0;
        for path in files {
            if only.as_ref().is_some_and(|o| !path.to_string_lossy().contains(o.as_str())) { continue; }
            let Ok(motion) = crate::Motion::load_with(&std::fs::read(&path).unwrap(), &unlimited, &Default::default()) else { continue };
            let (a, b) = motion.frames();
            let mut list = DisplayList::default();
            for f in [0.0, 0.5, 0.9] {
                motion.sample(a + (b - a) * f, &mut list);
                let base = fit(&list, 300.0, 200.0);
                let mut x = Pixmap::new(300, 200).unwrap();
                paint_into(&list, &mut x, base, &[]);
                let mut y = Pixmap::new(300, 200).unwrap();
                paint_into_unbounded(&list, &mut y, base, &[]);
                let diff = x.data().iter().zip(y.data()).filter(|(p, q)| p != q).count();
                let worst = x.data().iter().zip(y.data()).map(|(p, q)| (*p as i32 - *q as i32).abs()).max().unwrap_or(0);
                println!("{} @{f}: {diff} bytes differ (max {worst})", path.display());
                for (i, (p, q)) in x.data().chunks(4).zip(y.data().chunks(4)).enumerate().filter(|(_, (p, q))| p != q).take(5) {
                    println!("  ({}, {}) bounded {p:?} unbounded {q:?}", i % 300, i / 300);
                }
                failed += (diff > 0) as usize;
            }
        }
        assert_eq!(failed, 0);
    }
}
