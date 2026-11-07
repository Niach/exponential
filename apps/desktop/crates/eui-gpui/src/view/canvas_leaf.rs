//! Round 8: the surface's Canvas leaves (`packages/exponential-ui/docs/
//! round-8-contract.md` §1). Each Canvas node keeps a [`CanvasLeaf`]: its
//! compiled [`CanvasPlayer`] (recompiled when `size` / `shapes` change,
//! resampled when the theme or the mode changes; no playback), the image of
//! its last raster and its pointer state. [`CanvasElement`] paints the
//! shapes' raster, then every `text` op natively with gpui's text system
//! ABOVE it ([`paint_text_ops`], shared with the Motion painter), clipped by
//! the box only. It repaints the raster only when the box size, the fit or
//! the slots change.
//!
//! With `pointer: true` the element takes the pointer: a press fires
//! `press {x, y}`, a move `move {x, y}` (at most once per frame: a move
//! inside the frame step is held and the LAST one fires at the step), and
//! leaving fires `leave`; x/y in drawing units (`canvas::to_drawing`: the
//! fit inverted, two decimals, never clamped). Without it the drawing is
//! inert like an image (the core drops the events too). A Canvas is never a
//! tab stop and has no button role (`state.rs`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use eui_core::surface::PlacedNode;
use eui_motion::canvas::{fit_matrix, Fit};
use eui_motion::display::TextFont;
use eui_motion::{DisplayList, Op};
use gpui::{
    point, px, AnyElement, App, Bounds, ContentMask, Context, Corners, Element, ElementId, Font, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, RenderImage, SharedString, Style, TextRun, WeakEntity, Window,
};
use serde_json::{json, Value};

use super::motion_leaf::FRAME_STEP;
use super::SurfaceView;
use crate::canvas::{fit_of, line_origin, pointer_point, text_anchor, CanvasPlayer, PlacedText};
use crate::motion::theme_slots;

/// One Canvas node's state.
pub(crate) struct CanvasLeaf {
    /// Hash of the drawing (`size`, `shapes`) the player compiled.
    key: u64,
    /// Hash of the theme and mode the player sampled with (0 = none yet).
    slots_key: u64,
    player: Option<CanvasPlayer>,
    /// A compile refusal: the box stays empty.
    pub error: Option<String>,
    image: Option<Arc<RenderImage>>,
    /// Rasters made (a new image), for the repaint-on-change check.
    rasters: u64,
    placed: Vec<PlacedText>,
    hovered: bool,
    /// When the last `move` fired.
    last_move: Option<Instant>,
    /// A move inside the frame step, fired at the step (the last wins).
    queued: Option<[f64; 2]>,
    armed: bool,
}

impl CanvasLeaf {
    fn new() -> Self {
        Self { key: 0, slots_key: 0, player: None, error: None, image: None, rasters: 0, placed: Vec::new(), hovered: false, last_move: None, queued: None, armed: false }
    }
}

/// The view's Canvas leaves by node id.
pub(crate) type CanvasLeaves = RefCell<HashMap<String, Rc<RefCell<CanvasLeaf>>>>;

/// The theme families a drawing's text is set in (`$type.family.sans` /
/// `.mono`, through the surface's font mapping and fallbacks).
#[derive(Clone)]
pub(crate) struct TextFonts {
    pub sans: Font,
    pub mono: Font,
}

/// What the pointer did over a Canvas (box px already mapped to drawing
/// units).
#[derive(Clone, Copy, Debug)]
pub(crate) enum CanvasPointer {
    Press([f64; 2]),
    Move([f64; 2]),
    Leave,
}

fn hash_of(parts: &[Option<&Value>]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.map(Value::to_string).hash(&mut h);
    }
    h.finish()
}

/// Set every `text` op of `ops` natively in `bounds` (box px; `fit` = the
/// drawing → box mapping): one line per op at the anchor through `fit ×
/// transform`, the font px scaled by it, the theme's family at the CSS
/// weight, `align` physical, `baseline` from the font's own ascent and
/// descent at that size. Clipped by the box only.
pub(crate) fn paint_text_ops<'a>(ops: impl Iterator<Item = &'a Op>, fit: [f64; 6], bounds: Bounds<Pixels>, fonts: &TextFonts, window: &mut Window, cx: &mut App) -> Vec<PlacedText> {
    let mut placed = Vec::new();
    let ops: Vec<&Op> = ops.collect();
    if ops.is_empty() {
        return placed;
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for op in ops {
            let Op::Text { transform, text, font, weight, size, align, baseline, color } = op else { continue };
            let (x, y, font_px) = text_anchor(fit, *transform, *size);
            if !(font_px > 0.01) || !font_px.is_finite() || text.is_empty() {
                continue;
            }
            let base = match font {
                TextFont::Sans => &fonts.sans,
                TextFont::Mono => &fonts.mono,
            };
            let f = Font { weight: FontWeight(weight.css() as f32), ..base.clone() };
            let line_text: String = text.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
            let ink: gpui::Hsla = gpui::Rgba { r: color[0], g: color[1], b: color[2], a: color[3] }.into();
            let run = TextRun { len: line_text.len(), font: f.clone(), color: ink, background_color: None, underline: None, strikethrough: None };
            let ts = window.text_system().clone();
            let line = ts.shape_line(SharedString::from(line_text.clone()), px(font_px), &[run], None);
            let id = ts.resolve_font(&f);
            let ascent = f32::from(ts.ascent(id, px(font_px)));
            let descent = f32::from(ts.descent(id, px(font_px))).abs();
            let width = f32::from(line.width);
            let (left, base_y) = line_origin(x, y, width, ascent, descent, *align, *baseline);
            if color[3] > 0.0 {
                let origin = point(bounds.origin.x + px(left), bounds.origin.y + px(base_y) - line.ascent);
                let _ = line.paint(origin, line.ascent + line.descent, gpui::TextAlign::Left, None, window, cx);
            }
            placed.push(PlacedText { text: line_text, x: left, baseline: base_y, size: font_px, width, mono: *font == TextFont::Mono, weight: weight.css() });
        }
    });
    placed
}

impl SurfaceView {
    /// The fonts a Canvas (or a Motion's text ops) sets its text in.
    pub(crate) fn drawing_fonts(&self) -> TextFonts {
        let sans = self.fonts.font(None, 400, false);
        let mono_name = self.fonts.mono.clone();
        let mono = self.fonts.font(Some(mono_name.as_ref()), 400, false);
        TextFonts { sans, mono }
    }

    /// The compile refusal of Canvas `id`, if any.
    pub fn canvas_error(&self, id: &str) -> Option<String> {
        self.canvas_leaves.borrow().get(id).and_then(|l| l.borrow().error.clone())
    }

    /// The display list Canvas `id` sampled (tests, automation).
    pub fn canvas_list(&self, id: &str) -> Option<DisplayList> {
        self.canvas_leaves.borrow().get(id).and_then(|l| l.borrow().player.as_ref().map(|p| p.list().clone()))
    }

    /// How many rasters Canvas `id` made (a new image per size, fit, theme
    /// or drawing change; none per plain repaint).
    pub fn canvas_rasters(&self, id: &str) -> Option<u64> {
        self.canvas_leaves.borrow().get(id).map(|l| l.borrow().rasters)
    }

    /// The text Canvas `id` set in its last paint.
    pub fn canvas_texts(&self, id: &str) -> Vec<PlacedText> {
        self.canvas_leaves.borrow().get(id).map(|l| l.borrow().placed.clone()).unwrap_or_default()
    }

    /// The element painting Canvas `n`.
    pub(super) fn canvas_element(&self, n: &PlacedNode, cx: &mut Context<Self>) -> AnyElement {
        let leaf = self.canvas_leaves.borrow_mut().entry(n.id.clone()).or_insert_with(|| Rc::new(RefCell::new(CanvasLeaf::new()))).clone();
        self.prune_canvas_leaves(cx);
        self.load_canvas(&leaf, n, cx);
        let pointer = n.props.get("pointer").and_then(Value::as_bool) == Some(true) && !self.ghosting.get();
        CanvasElement { id: ElementId::Name(format!("{}.canvas", n.id).into()), node_id: n.id.clone(), leaf, fit: fit_of(&n.props), pointer, fonts: self.drawing_fonts(), view: cx.entity().downgrade() }.into_any_element()
    }

    /// (Re)compile the leaf's drawing when `size` / `shapes` changed;
    /// resample it when the theme or the mode changed.
    fn load_canvas(&self, leaf: &Rc<RefCell<CanvasLeaf>>, n: &PlacedNode, cx: &mut Context<Self>) {
        let mut l = leaf.borrow_mut();
        let key = hash_of(&[n.props.get("size"), n.props.get("shapes")]);
        if key != l.key || (l.player.is_none() && l.error.is_none()) {
            l.key = key;
            l.slots_key = 0;
            if let Some(old) = l.image.take() {
                cx.drop_image(old, None);
            }
            match CanvasPlayer::compile(&n.props) {
                Ok(p) => {
                    l.player = Some(p);
                    l.error = None;
                }
                Err(e) => {
                    l.player = None;
                    l.error = Some(e.clone());
                    let host = self.host.clone();
                    let error = crate::host::PaintError { surface_id: self.surface.id.clone(), component_id: n.id.clone(), message: format!("Canvas refused: {e}") };
                    cx.defer(move |cx| host.on_paint_error(&error, cx));
                }
            }
        }
        let mode = self.surface.mode();
        let theme = self.surface.theme().cloned();
        let theme_id = theme.as_ref().map_or(0, |t| Arc::as_ptr(t) as usize as u64);
        let slots_key = (theme_id.wrapping_mul(31) ^ ((mode == eui_core::theme::Mode::Dark) as u64)) | 1 << 63;
        if l.slots_key != slots_key {
            if let Some(p) = l.player.as_mut() {
                let slots = theme_slots(p.lottie(), theme.as_deref(), mode, None);
                p.set_slots(&slots);
                l.slots_key = slots_key;
            }
        }
    }

    /// Forget leaves whose node left the tree (their images leave the atlas).
    fn prune_canvas_leaves(&self, cx: &mut Context<Self>) {
        let mut leaves = self.canvas_leaves.borrow_mut();
        let live: std::collections::HashSet<&str> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "Canvas").map(|n| n.id.as_str()).collect();
        if leaves.len() <= live.len() {
            return;
        }
        let gone: Vec<String> = leaves.keys().filter(|k| !live.contains(k.as_str())).cloned().collect();
        for id in gone {
            if let Some(l) = leaves.remove(&id) {
                if let Some(image) = l.borrow_mut().image.take() {
                    cx.drop_image(image, None);
                }
            }
        }
    }

    /// The pointer over Canvas `id` (drawing units): `press` fires at once,
    /// `move` at most once per frame step (the last held move fires at the
    /// step), `leave` once when the pointer goes.
    pub(crate) fn canvas_pointer(&mut self, id: &str, ev: CanvasPointer, cx: &mut Context<Self>) {
        let Some(leaf) = self.canvas_leaves.borrow().get(id).cloned() else { return };
        let Some(index) = self.cache.index_of(id) else { return };
        let xy = |p: [f64; 2]| Some(json!({"x": p[0], "y": p[1]}));
        match ev {
            CanvasPointer::Press(p) => self.fire(index, "press", xy(p), cx),
            CanvasPointer::Move(p) => {
                let now = self.clock();
                let wait = {
                    let mut l = leaf.borrow_mut();
                    l.hovered = true;
                    match l.last_move {
                        Some(last) if now.saturating_duration_since(last) < FRAME_STEP => {
                            l.queued = Some(p);
                            if l.armed {
                                return;
                            }
                            l.armed = true;
                            Some(FRAME_STEP - now.saturating_duration_since(last))
                        }
                        _ => {
                            l.last_move = Some(now);
                            None
                        }
                    }
                };
                match wait {
                    None => self.fire(index, "move", xy(p), cx),
                    Some(wait) => {
                        let id = id.to_string();
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(wait).await;
                            let _ = this.update(cx, |this, cx| this.flush_canvas_move(&id, cx));
                        })
                        .detach();
                    }
                }
            }
            CanvasPointer::Leave => {
                {
                    let mut l = leaf.borrow_mut();
                    if !l.hovered {
                        return;
                    }
                    l.hovered = false;
                    l.queued = None;
                }
                self.fire(index, "leave", None, cx);
            }
        }
    }

    /// The held move of Canvas `id` fires (its frame step came).
    fn flush_canvas_move(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(leaf) = self.canvas_leaves.borrow().get(id).cloned() else { return };
        let queued = {
            let mut l = leaf.borrow_mut();
            l.armed = false;
            let q = l.queued.take();
            if q.is_some() {
                l.last_move = Some(self.clock());
            }
            q
        };
        if let (Some(p), Some(index)) = (queued, self.cache.index_of(id)) {
            self.fire(index, "move", Some(json!({"x": p[0], "y": p[1]})), cx);
        }
    }
}

/// The element painting one Canvas leaf into its bounds.
pub(crate) struct CanvasElement {
    id: ElementId,
    node_id: String,
    leaf: Rc<RefCell<CanvasLeaf>>,
    fit: Fit,
    pointer: bool,
    fonts: TextFonts,
    view: WeakEntity<SurfaceView>,
}

impl IntoElement for CanvasElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for CanvasElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = gpui::relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) -> Option<Hitbox> {
        self.pointer.then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal))
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), hitbox: &mut Option<Hitbox>, window: &mut Window, cx: &mut App) {
        let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let scale = window.scale_factor();
        let (w, h) = ((bw * scale).round() as u32, (bh * scale).round() as u32);
        let size = {
            let mut guard = self.leaf.borrow_mut();
            let leaf = &mut *guard;
            let Some(player) = leaf.player.as_mut() else { return };
            let mut replaced = None;
            let image = player.render_image(w, h, self.fit, &mut replaced);
            if let Some(old) = replaced {
                cx.drop_image(old, Some(window));
            }
            if image.as_ref().map(Arc::as_ptr) != leaf.image.as_ref().map(Arc::as_ptr) && image.is_some() {
                leaf.rasters += 1;
            }
            leaf.image = image.clone();
            if let Some(image) = image {
                let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
            }
            let size = player.size();
            let fit = fit_matrix(size.0 as f64, size.1 as f64, bw as f64, bh as f64, self.fit);
            leaf.placed = paint_text_ops(player.texts(), fit, bounds, &self.fonts, window, cx);
            size
        };
        let Some(hitbox) = hitbox.take() else { return };
        let fit = self.fit;
        let at = move |p: gpui::Point<Pixels>| pointer_point(size, (bw, bh), fit, (f32::from(p.x - bounds.origin.x), f32::from(p.y - bounds.origin.y)));
        {
            let (view, id, hitbox) = (self.view.clone(), self.node_id.clone(), hitbox.clone());
            window.on_mouse_event(move |ev: &MouseDownEvent, phase, window, cx| {
                if !phase.bubble() || ev.button != MouseButton::Left || !hitbox.is_hovered(window) {
                    return;
                }
                if let Some(p) = at(ev.position) {
                    let id = id.clone();
                    let _ = view.update(cx, |this, cx| this.canvas_pointer(&id, CanvasPointer::Press(p), cx));
                }
            });
        }
        let (view, id, leaf) = (self.view.clone(), self.node_id.clone(), self.leaf.clone());
        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }
            let ev = if hitbox.is_hovered(window) {
                match at(ev.position) {
                    Some(p) => CanvasPointer::Move(p),
                    None => return,
                }
            } else if leaf.borrow().hovered {
                CanvasPointer::Leave
            } else {
                return;
            };
            let id = id.clone();
            let _ = view.update(cx, |this, cx| this.canvas_pointer(&id, ev, cx));
        });
    }
}
