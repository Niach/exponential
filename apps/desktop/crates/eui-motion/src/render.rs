//! Layers → display list: time mapping (start time, stretch, time remap),
//! parenting, auto-orient, masks, track mattes, precomps, blend modes and
//! effects, following lottie-web's semantics.

use crate::display::{Blend, Brush, DisplayList, Filter, FillRule, Op, Verb};
use crate::geom::{Affine, BPath};
use crate::model::{Animation, Layer, LayerKind, MaskMode, MatteMode};
use crate::shapes;

/// Precomp nesting cap (a cycle guard; lottie files nest a handful deep).
const MAX_DEPTH: usize = 32;

/// Collects ops. Layers and clips nest; `pop` closes the innermost.
pub struct Emitter<'a> {
    pub ops: &'a mut Vec<Op>,
    /// The composition rect at identity: the clip of full-surface layers.
    pub full: Vec<Verb>,
    depth: usize,
}

impl Emitter<'_> {
    pub fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// Open an isolated full-surface layer.
    pub fn begin_layer(&mut self, blend: Blend, alpha: f32, filters: Vec<Filter>) {
        self.ops.push(Op::Layer {
            blend,
            alpha,
            transform: crate::display::IDENTITY,
            clip: self.full.clone(),
            filters,
        });
        self.depth += 1;
    }

    pub fn clip(&mut self, transform: &Affine, path: Vec<Verb>) {
        self.ops.push(Op::Clip { transform: transform.to_dl(), path });
        self.depth += 1;
    }

    pub fn pop(&mut self) {
        if self.depth > 0 {
            self.ops.push(Op::Pop);
            self.depth -= 1;
        }
    }

    /// Fill the full surface with opaque white (mask coverage).
    pub fn fill_full(&mut self, alpha: f32) {
        self.ops.push(Op::Fill {
            transform: crate::display::IDENTITY,
            rule: FillRule::Nonzero,
            brush: Brush::Solid { color: [1.0, 1.0, 1.0, alpha] },
            path: self.full.clone(),
        });
    }

    /// Drop everything after `mark` (a layer that turned out empty).
    pub fn truncate(&mut self, mark: usize, depth: usize) {
        self.ops.truncate(mark);
        self.depth = depth;
    }

    pub fn mark(&self) -> (usize, usize) {
        (self.ops.len(), self.depth)
    }
}

pub(crate) fn rect_verbs(w: f64, h: f64) -> Vec<Verb> {
    let (w, h) = (w as f32, h as f32);
    vec![
        Verb::M { x: 0.0, y: 0.0 },
        Verb::L { x: w, y: 0.0 },
        Verb::L { x: w, y: h },
        Verb::L { x: 0.0, y: h },
        Verb::Z,
    ]
}

/// Sample `frame` of `anim` into `list`.
pub fn render(anim: &Animation, frame: f64, list: &mut DisplayList) {
    list.clear();
    list.width = anim.width as f32;
    list.height = anim.height as f32;
    let full = rect_verbs(anim.width, anim.height);
    let mut em = Emitter { ops: &mut list.ops, full: full.clone(), depth: 0 };
    em.clip(&Affine::IDENTITY, full);
    render_comp(anim, &anim.layers, frame, &Affine::IDENTITY, &mut em, 0);
    em.pop();
}

fn layer_by_index(layers: &[Layer], ind: i64) -> Option<usize> {
    layers.iter().position(|l| l.index == Some(ind))
}

/// A layer's own matrix at its local frame (with auto-orient).
fn own_matrix(layer: &Layer, local: f64) -> Affine {
    let mut m = shapes::transform_matrix(&layer.transform, local);
    if layer.auto_orient {
        if let crate::model::Position::Joined(p) = &layer.transform.position {
            if p.is_animated() {
                let a = p.vec2_at(local);
                let b = p.vec2_at(local + 0.01);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let (dx, dy) = if dx == 0.0 && dy == 0.0 {
                    let c = p.vec2_at(local - 0.01);
                    (a[0] - c[0], a[1] - c[1])
                } else {
                    (dx, dy)
                };
                if dx != 0.0 || dy != 0.0 {
                    let angle = dy.atan2(dx).to_degrees();
                    let mut t = layer.transform.clone();
                    let r = t.rotation.as_ref().map(|r| r.scalar_at(local)).unwrap_or(0.0);
                    t.rotation = Some(crate::anim::Prop::scalar(r + angle));
                    m = shapes::transform_matrix(&t, local);
                }
            }
        }
    }
    m
}

/// The layer's matrix in its composition, parents included.
fn world_matrix(layers: &[Layer], idx: usize, comp_frame: f64) -> Affine {
    let mut m = Affine::IDENTITY;
    let mut cur = Some(idx);
    let mut guard = 0;
    while let Some(i) = cur {
        let l = &layers[i];
        m = own_matrix(l, comp_frame - l.start_time).then(&m);
        cur = l.parent.and_then(|p| layer_by_index(layers, p));
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    m
}

fn render_comp(anim: &Animation, layers: &[Layer], frame: f64, comp_xf: &Affine, em: &mut Emitter, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for idx in (0..layers.len()).rev() {
        let layer = &layers[idx];
        if layer.is_matte || layer.hidden {
            continue;
        }
        if !(layer.in_point <= frame && frame < layer.out_point) {
            continue;
        }
        draw_matted(anim, layers, idx, frame, comp_xf, em, depth, 0);
    }
}

/// Draw a layer through its track matte (if any). The matte source is itself
/// drawn through ITS matte (chained mattes), and the layer's blend mode
/// applies to the matted result.
#[allow(clippy::too_many_arguments)]
fn draw_matted(anim: &Animation, layers: &[Layer], idx: usize, frame: f64, comp_xf: &Affine, em: &mut Emitter, depth: usize, chain: usize) {
    let layer = &layers[idx];
    let Some(mode) = layer.matte_mode.filter(|_| chain < 8) else {
        draw_layer(anim, layers, idx, frame, comp_xf, em, depth, true);
        return;
    };
    let source = match layer.matte_parent {
        Some(tp) => layer_by_index(layers, tp),
        None => idx.checked_sub(1),
    };
    em.begin_layer(layer.blend, 1.0, Vec::new());
    draw_layer(anim, layers, idx, frame, comp_xf, em, depth, false);
    let blend = match mode {
        MatteMode::Alpha => Blend::DstIn,
        MatteMode::AlphaInverted => Blend::DstOut,
        MatteMode::Luma => Blend::LumaIn,
        MatteMode::LumaInverted => Blend::LumaOut,
    };
    em.begin_layer(blend, 1.0, Vec::new());
    if let Some(src) = source.filter(|s| *s != idx) {
        let s = &layers[src];
        if s.in_point <= frame && frame < s.out_point {
            draw_matted(anim, layers, src, frame, comp_xf, em, depth, chain + 1);
        }
    }
    em.pop();
    em.pop();
}

#[allow(clippy::too_many_arguments)]
fn draw_layer(anim: &Animation, layers: &[Layer], idx: usize, frame: f64, comp_xf: &Affine, em: &mut Emitter, depth: usize, with_blend: bool) {
    let layer = &layers[idx];
    let local = frame - layer.start_time;
    let opacity = layer
        .transform
        .opacity
        .as_ref()
        .map(|o| o.scalar_at(local) / 100.0)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let m = comp_xf.then(&world_matrix(layers, idx, frame));
    if !m.is_finite() {
        return;
    }
    let filters = crate::effects::filters(layer, local, &m, anim);
    let blend = if with_blend { layer.blend } else { Blend::Normal };
    let isolated = opacity < 1.0 || blend != Blend::Normal || !filters.is_empty() || crate::effects::isolates(layer, local);
    let mark = em.mark();
    if isolated {
        em.begin_layer(blend, opacity as f32, filters);
    }
    let masked = begin_masks(layer, local, &m, em);
    match &layer.kind {
        LayerKind::Shape(items) => shapes::render(items, local, &m, em),
        LayerKind::Solid { color, width, height } => {
            em.push(Op::Fill {
                transform: m.to_dl(),
                rule: FillRule::Nonzero,
                brush: Brush::Solid { color: color.map(|c| c as f32) },
                path: rect_verbs(*width, *height),
            });
        }
        LayerKind::Image { asset } => {
            if let Some(img) = anim.images.get(*asset) {
                em.clip(&m, rect_verbs(img.width, img.height));
                em.push(Op::Image {
                    transform: m.to_dl(),
                    asset: *asset as u32,
                    alpha: 1.0,
                    width: img.width as f32,
                    height: img.height as f32,
                });
                em.pop();
            }
        }
        LayerKind::Precomp { id, width, height, time_remap } => {
            if let Some(children) = anim.precomps.get(id) {
                let child_frame = match time_remap {
                    Some(tm) => {
                        let mut t = tm.scalar_at(local) * anim.frame_rate;
                        // lottie-web: the out point itself shows the last frame.
                        if t == layer.out_point {
                            t -= 1.0;
                        }
                        t
                    }
                    None => local / layer.stretch,
                };
                em.clip(&m, rect_verbs(*width, *height));
                render_comp(anim, children, child_frame, &m, em, depth + 1);
                em.pop();
            }
        }
        LayerKind::Text(text) => crate::text::render(text, anim, local, &m, em),
        LayerKind::Null | LayerKind::Other => {}
    }
    crate::effects::paint_after(layer, local, &m, em);
    end_masks(layer, local, &m, masked, em);
    if isolated {
        em.pop();
    }
    let _ = mark;
}

#[derive(Clone, Copy)]
enum Masking {
    None,
    /// One additive mask at full opacity: a plain clip.
    Clip,
    /// Coverage composited with dst-in after the content.
    Composite,
}

fn active_masks(layer: &Layer) -> impl Iterator<Item = &crate::model::Mask> {
    layer.masks.iter().filter(|m| m.mode != MaskMode::None)
}

fn begin_masks(layer: &Layer, local: f64, m: &Affine, em: &mut Emitter) -> Masking {
    let active: Vec<_> = active_masks(layer).collect();
    if active.is_empty() {
        return Masking::None;
    }
    let simple = active.len() == 1 && {
        let k = active[0];
        k.mode == MaskMode::Add
            && !k.inverted
            && k.opacity.as_ref().map(|o| o.scalar_at(local) >= 100.0).unwrap_or(true)
            && k.expansion.as_ref().map(|x| x.scalar_at(local) == 0.0).unwrap_or(true)
            && k.feather.as_ref().map(|f| f.vec2_at(local) == [0.0, 0.0]).unwrap_or(true)
    };
    if simple {
        let mut v = Vec::new();
        BPath::from_flat(&active[0].path.eval(local)).to_verbs(&Affine::IDENTITY, &mut v);
        em.clip(m, v);
        return Masking::Clip;
    }
    em.begin_layer(Blend::Normal, 1.0, Vec::new());
    Masking::Composite
}

fn end_masks(layer: &Layer, local: f64, m: &Affine, masking: Masking, em: &mut Emitter) {
    match masking {
        Masking::None => {}
        Masking::Clip => em.pop(),
        Masking::Composite => {
            // Feather blurs the whole coverage (the largest feather of the set).
            let feather = active_masks(layer)
                .filter_map(|k| k.feather.as_ref().map(|f| f.vec2_at(local)))
                .fold(0.0f64, |a, f| a.max(f[0].max(f[1])));
            let scale = m.determinant().abs().sqrt();
            let filters = if feather > 0.0 {
                let s = (feather * scale / 2.0) as f32;
                vec![Filter::Blur { sigma_x: s, sigma_y: s }]
            } else {
                Vec::new()
            };
            em.begin_layer(Blend::DstIn, 1.0, filters);
            for (k, mask) in active_masks(layer).enumerate() {
                if k == 0 && matches!(mask.mode, MaskMode::Subtract | MaskMode::Intersect | MaskMode::Darken) {
                    em.fill_full(1.0);
                }
                let alpha = mask.opacity.as_ref().map(|o| o.scalar_at(local) / 100.0).unwrap_or(1.0).clamp(0.0, 1.0) as f32;
                let blend = match mask.mode {
                    MaskMode::Subtract => Blend::DstOut,
                    MaskMode::Intersect | MaskMode::Darken => Blend::DstIn,
                    MaskMode::Difference => Blend::Xor,
                    _ => Blend::Normal,
                };
                em.begin_layer(blend, alpha, Vec::new());
                let mut v = Vec::new();
                BPath::from_flat(&mask.path.eval(local)).to_verbs(&Affine::IDENTITY, &mut v);
                let expansion = mask.expansion.as_ref().map(|x| x.scalar_at(local)).unwrap_or(0.0);
                let white = Brush::Solid { color: [1.0, 1.0, 1.0, 1.0] };
                let shape = |em: &mut Emitter, v: &Vec<Verb>| {
                    em.push(Op::Fill { transform: m.to_dl(), rule: FillRule::Nonzero, brush: white.clone(), path: v.clone() });
                    if expansion != 0.0 {
                        let stroke = Op::Stroke {
                            transform: m.to_dl(),
                            style: crate::display::StrokeStyle {
                                width: (expansion.abs() * 2.0) as f32,
                                cap: crate::display::Cap::Round,
                                join: crate::display::Join::Round,
                                miter: 4.0,
                                dashes: Vec::new(),
                                dash_offset: 0.0,
                            },
                            brush: white.clone(),
                            path: v.clone(),
                        };
                        if expansion > 0.0 {
                            em.push(stroke);
                        } else {
                            em.begin_layer(Blend::DstOut, 1.0, Vec::new());
                            em.push(stroke);
                            em.pop();
                        }
                    }
                };
                if mask.inverted {
                    em.fill_full(1.0);
                    em.begin_layer(Blend::DstOut, 1.0, Vec::new());
                    shape(em, &v);
                    em.pop();
                } else {
                    shape(em, &v);
                }
                em.pop();
            }
            em.pop(); // the dst-in coverage
            em.pop(); // the masked group
        }
    }
}
