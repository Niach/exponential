//! Shape layers: After Effects' stack model, as lottie-web renders it.
//!
//! Items are processed top to bottom into one geometry arena. A style
//! (fill, stroke, gradient) paints every path above it in its group and the
//! nested groups above it; a modifier rewrites every path above it, so a
//! style sees the FINAL path whatever the order of styles and modifiers
//! (lottie-web, lottie-android). A repeater clones everything above it.
//! Items higher in the list are drawn ON TOP, so paint nodes replay in reverse.

use crate::display::{Blend, Brush, Cap, FillRule, Join, Op, Stop, StrokeStyle, Verb};
use crate::geom::{self, Affine, BPath};
use crate::model::{Gradient, Item, StrokeItem, Transform};
use crate::modifiers;
use crate::render::Emitter;

/// A path on the stack: local geometry + local → layer-space transform.
#[derive(Clone, Debug)]
pub struct Geo {
    pub path: BPath,
    /// Further subpaths of the same geometry (a trim that splits a path in two).
    pub more: Vec<BPath>,
    pub xf: Affine,
    /// Merge-paths set id and mode (1 merge, 2 add, 3 subtract, 4 intersect, 5 exclude).
    pub merge: Option<(u32, u8)>,
}

#[derive(Clone, Debug)]
enum Paint {
    Fill { brush: Brush, rule: FillRule },
    Stroke { brush: Brush, style: StrokeStyle },
}

/// A collected paint, layer-space transform of its style group.
#[derive(Clone, Debug)]
enum Node {
    Paint { paint: Paint, xf: Affine, geos: Vec<usize> },
    /// An isolated group (group or repeater-copy opacity < 100 %).
    Group { alpha: f64, nodes: Vec<Node> },
}

pub struct ShapeCtx {
    next_merge: u32,
    scratch: Vec<f64>,
}

impl ShapeCtx {
    pub fn new() -> Self {
        Self { next_merge: 0, scratch: Vec::new() }
    }
}

/// The matrix of a layer `ks` or group `tr` transform at `frame` (no auto-orient).
pub fn transform_matrix(t: &Transform, frame: f64) -> Affine {
    let anchor = t.anchor.as_ref().map(|p| p.vec2_at(frame)).unwrap_or([0.0, 0.0]);
    let pos = match &t.position {
        crate::model::Position::None => [0.0, 0.0],
        crate::model::Position::Joined(p) => p.vec2_at(frame),
        crate::model::Position::Split(x, y) => [x.scalar_at(frame), y.scalar_at(frame)],
    };
    let scale = t.scale.as_ref().map(|p| p.vec2_at(frame)).unwrap_or([100.0, 100.0]);
    let rot = t.rotation.as_ref().map(|p| p.scalar_at(frame)).unwrap_or(0.0);
    let skew = t.skew.as_ref().map(|p| p.scalar_at(frame)).unwrap_or(0.0);
    let skew_axis = t.skew_axis.as_ref().map(|p| p.scalar_at(frame)).unwrap_or(0.0);
    compose(anchor, pos, scale, rot, skew, skew_axis)
}

/// T(p) · R(r) · Skew(sk, sa) · S(s) · T(−a), lottie-web's order.
pub fn compose(anchor: [f64; 2], pos: [f64; 2], scale: [f64; 2], rot: f64, skew: f64, skew_axis: f64) -> Affine {
    let mut m = Affine::translate(pos[0], pos[1]).then(&Affine::rotate_deg(rot));
    if skew != 0.0 {
        // R(−sa) · ShearX(−tan sk) · R(sa)
        let shear = Affine([1.0, 0.0, -skew.to_radians().tan(), 1.0, 0.0, 0.0]);
        m = m
            .then(&Affine::rotate_deg(-skew_axis))
            .then(&shear)
            .then(&Affine::rotate_deg(skew_axis));
    }
    m.then(&Affine::scale(scale[0] / 100.0, scale[1] / 100.0))
        .then(&Affine::translate(-anchor[0], -anchor[1]))
}

/// Render a shape list (a layer's `shapes`) in layer space `layer_xf`.
pub fn render(items: &[Item], frame: f64, layer_xf: &Affine, em: &mut Emitter) {
    let mut ctx = ShapeCtx::new();
    let mut geos = Vec::new();
    let mut nodes = Vec::new();
    process(&mut ctx, items, frame, &Affine::IDENTITY, &mut geos, &mut nodes);
    emit_nodes(&nodes, &geos, layer_xf, em);
}

fn process(ctx: &mut ShapeCtx, items: &[Item], frame: f64, group_xf: &Affine, geos: &mut Vec<Geo>, nodes: &mut Vec<Node>) {
    let geo_start = geos.len();
    let node_start = nodes.len();
    for item in items {
        match item {
            Item::Group { items, transform } => {
                let (m, alpha) = match transform {
                    Some(t) => (
                        transform_matrix(t, frame),
                        t.opacity.as_ref().map(|o| o.scalar_at(frame) / 100.0).unwrap_or(1.0),
                    ),
                    None => (Affine::IDENTITY, 1.0),
                };
                let xf = group_xf.then(&m);
                let mut child_nodes = Vec::new();
                process(ctx, items, frame, &xf, geos, &mut child_nodes);
                if alpha <= 0.0 {
                    continue;
                }
                if alpha < 1.0 {
                    nodes.push(Node::Group { alpha, nodes: child_nodes });
                } else {
                    nodes.extend(child_nodes);
                }
            }
            Item::Rect { position, size, roundness, reversed } => {
                let p = position.vec2_at(frame);
                let s = size.vec2_at(frame);
                let r = roundness.scalar_at(frame);
                push(geos, geom::rect(p, s, r, !reversed), group_xf);
            }
            Item::Ellipse { position, size, reversed } => {
                push(geos, geom::ellipse(position.vec2_at(frame), size.vec2_at(frame), !reversed), group_xf);
            }
            Item::Star(s) => {
                let path = geom::star(
                    s.polygon,
                    s.position.vec2_at(frame),
                    s.points.scalar_at(frame),
                    s.outer_radius.scalar_at(frame),
                    s.outer_roundness.scalar_at(frame),
                    s.inner_radius.scalar_at(frame),
                    s.inner_roundness.scalar_at(frame),
                    s.rotation.scalar_at(frame),
                    s.reversed,
                );
                push(geos, path, group_xf);
            }
            Item::Path { shape, .. } => {
                shape.eval_into(frame, &mut ctx.scratch);
                push(geos, BPath::from_flat(&ctx.scratch), group_xf);
            }
            Item::Fill { color, opacity, evenodd } => {
                let c = color.eval(frame);
                let a = opacity.scalar_at(frame) / 100.0;
                let brush = Brush::Solid { color: rgba(&c, a) };
                let rule = if *evenodd { FillRule::Evenodd } else { FillRule::Nonzero };
                paint(nodes, geo_start..geos.len(), Paint::Fill { brush, rule }, group_xf);
            }
            Item::GradientFill { gradient, opacity, evenodd } => {
                let a = opacity.scalar_at(frame) / 100.0;
                let brush = gradient_brush(gradient, frame, a);
                let rule = if *evenodd { FillRule::Evenodd } else { FillRule::Nonzero };
                paint(nodes, geo_start..geos.len(), Paint::Fill { brush, rule }, group_xf);
            }
            Item::Stroke(s) | Item::GradientStroke(s) => {
                let a = s.opacity.scalar_at(frame) / 100.0;
                let brush = match (&s.color, &s.gradient) {
                    (_, Some(g)) => gradient_brush(g, frame, a),
                    (Some(c), _) => Brush::Solid { color: rgba(&c.eval(frame), a) },
                    _ => continue,
                };
                let style = stroke_style(s, frame);
                if style.width <= 0.0 {
                    continue;
                }
                paint(nodes, geo_start..geos.len(), Paint::Stroke { brush, style }, group_xf);
            }
            Item::Trim { start, end, offset, individually } => {
                modifiers::trim(
                    &mut geos[geo_start..],
                    start.scalar_at(frame),
                    end.scalar_at(frame),
                    offset.scalar_at(frame),
                    *individually,
                );
            }
            Item::RoundCorners { radius } => {
                let r = radius.scalar_at(frame);
                for g in &mut geos[geo_start..] {
                    modifiers::round_corners(&mut g.path, r);
                    for p in &mut g.more {
                        modifiers::round_corners(p, r);
                    }
                }
            }
            Item::PuckerBloat { amount } => {
                let a = amount.scalar_at(frame);
                for g in &mut geos[geo_start..] {
                    modifiers::pucker_bloat(&mut g.path, a);
                    for p in &mut g.more {
                        modifiers::pucker_bloat(p, a);
                    }
                }
            }
            Item::ZigZag { size, ridges, smooth } => {
                let (s, r) = (size.scalar_at(frame), ridges.scalar_at(frame));
                for g in &mut geos[geo_start..] {
                    modifiers::zig_zag(&mut g.path, s, r, *smooth);
                    for p in &mut g.more {
                        modifiers::zig_zag(p, s, r, *smooth);
                    }
                }
            }
            Item::OffsetPath { amount, join, miter } => {
                let (a, ml) = (amount.scalar_at(frame), miter.scalar_at(frame));
                for g in &mut geos[geo_start..] {
                    modifiers::offset_path(&mut g.path, a, *join, ml);
                    for p in &mut g.more {
                        modifiers::offset_path(p, a, *join, ml);
                    }
                }
            }
            Item::Twist { angle, center } => {
                let (a, c) = (angle.scalar_at(frame), center.vec2_at(frame));
                for g in &mut geos[geo_start..] {
                    modifiers::twist(&mut g.path, a, c);
                    for p in &mut g.more {
                        modifiers::twist(p, a, c);
                    }
                }
            }
            Item::Merge { mode } => {
                let id = ctx.next_merge;
                ctx.next_merge += 1;
                for (k, g) in geos[geo_start..].iter_mut().enumerate() {
                    // The first path is the base; later ones combine with `mode`.
                    g.merge = Some((id, if k == 0 { 1 } else { *mode }));
                }
            }
            Item::Repeater { copies, offset, below, transform } => {
                repeat(
                    ctx,
                    geos,
                    nodes,
                    geo_start,
                    node_start,
                    group_xf,
                    copies.scalar_at(frame),
                    offset.scalar_at(frame),
                    *below,
                    transform,
                    frame,
                );
            }
        }
    }
}

fn push(geos: &mut Vec<Geo>, path: BPath, xf: &Affine) {
    if !path.is_empty() {
        geos.push(Geo { path, more: Vec::new(), xf: *xf, merge: None });
    }
}

fn paint(nodes: &mut Vec<Node>, scope: std::ops::Range<usize>, paint: Paint, xf: &Affine) {
    if scope.is_empty() {
        return;
    }
    nodes.push(Node::Paint { paint, xf: *xf, geos: scope.collect() });
}

pub(crate) fn rgba(c: &[f64], alpha: f64) -> [f32; 4] {
    let g = |i: usize, d: f64| c.get(i).copied().unwrap_or(d).clamp(0.0, 1.0) as f32;
    [g(0, 0.0), g(1, 0.0), g(2, 0.0), (g(3, 1.0) as f64 * alpha.clamp(0.0, 1.0)) as f32]
}

fn stroke_style(s: &StrokeItem, frame: f64) -> StrokeStyle {
    let mut dashes = Vec::new();
    let mut dash_offset = 0.0;
    for (kind, v) in &s.dashes {
        let x = v.scalar_at(frame) as f32;
        if *kind == 'o' {
            dash_offset = x;
        } else {
            dashes.push(x.max(0.0));
        }
    }
    if dashes.iter().all(|d| *d == 0.0) {
        dashes.clear();
    }
    StrokeStyle {
        width: s.width.scalar_at(frame) as f32,
        cap: match s.cap {
            1 => Cap::Butt,
            3 => Cap::Square,
            _ => Cap::Round,
        },
        join: match s.join {
            1 => Join::Miter,
            3 => Join::Bevel,
            _ => Join::Round,
        },
        miter: s.miter as f32,
        dashes,
        dash_offset,
    }
}

/// Gradient stops from `g.k`: `count` colour stops `[offset, r, g, b]`, then
/// optional alpha stops `[offset, alpha]`, merged on the union of offsets.
pub(crate) fn gradient_stops(raw: &[f64], count: usize, alpha: f64) -> Vec<Stop> {
    let colors: Vec<(f64, [f64; 3])> = (0..count)
        .filter_map(|k| {
            let b = k * 4;
            raw.get(b + 3).map(|_| (raw[b], [raw[b + 1], raw[b + 2], raw[b + 3]]))
        })
        .collect();
    let rest = raw.get(count * 4..).unwrap_or(&[]);
    let alphas: Vec<(f64, f64)> = rest.chunks_exact(2).map(|c| (c[0], c[1])).collect();
    if colors.is_empty() {
        return Vec::new();
    }
    let color_at = |t: f64| -> [f64; 3] {
        if t <= colors[0].0 {
            return colors[0].1;
        }
        for w in colors.windows(2) {
            if t <= w[1].0 {
                let f = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 0.0 };
                return [0, 1, 2].map(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * f);
            }
        }
        colors[colors.len() - 1].1
    };
    let alpha_at = |t: f64| -> f64 {
        if alphas.is_empty() {
            return 1.0;
        }
        if t <= alphas[0].0 {
            return alphas[0].1;
        }
        for w in alphas.windows(2) {
            if t <= w[1].0 {
                let f = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 0.0 };
                return w[0].1 + (w[1].1 - w[0].1) * f;
            }
        }
        alphas[alphas.len() - 1].1
    };
    // Colour and alpha stops merged by offset, coincident stops kept (hard edges).
    let mut events: Vec<(f64, Option<[f64; 3]>, Option<f64>)> = colors.iter().map(|c| (c.0, Some(c.1), None)).collect();
    for a in &alphas {
        if !colors.iter().any(|c| (c.0 - a.0).abs() < 1e-9) {
            events.push((a.0, None, Some(a.1)));
        }
    }
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    events
        .into_iter()
        .map(|(t, c, a)| {
            let c = c.unwrap_or_else(|| color_at(t));
            let a = a.unwrap_or_else(|| alpha_at(t));
            Stop {
                offset: t.clamp(0.0, 1.0) as f32,
                color: [c[0] as f32, c[1] as f32, c[2] as f32, (a * alpha) as f32],
            }
        })
        .collect()
}

pub(crate) fn gradient_brush(g: &Gradient, frame: f64, alpha: f64) -> Brush {
    let raw = g.stops.eval(frame);
    let stops = gradient_stops(&raw, g.count, alpha);
    let s = g.start.vec2_at(frame);
    let e = g.end.vec2_at(frame);
    if !g.radial {
        return Brush::Linear { start: [s[0] as f32, s[1] as f32], end: [e[0] as f32, e[1] as f32], stops };
    }
    let r = ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2)).sqrt();
    let ang = (e[1] - s[1]).atan2(e[0] - s[0]);
    let h = (g.highlight_length.scalar_at(frame) / 100.0).clamp(-0.99, 0.99);
    let a = g.highlight_angle.scalar_at(frame).to_radians();
    let dist = r * h;
    let focal = [s[0] + (ang + a).cos() * dist, s[1] + (ang + a).sin() * dist];
    Brush::Radial {
        center: [s[0] as f32, s[1] as f32],
        focal: [focal[0] as f32, focal[1] as f32],
        radius: r as f32,
        stops,
    }
}

/// lottie-web's repeater step: translate `p·perc`, rotate `r·perc` and scale
/// `lerp(1, s, perc)` about the anchor; `inv` steps backwards.
struct Steps {
    p: [f64; 2],
    r: f64,
    s: [f64; 2],
}

fn repeater_matrix(t: &Transform, frame: f64, steps: &[(f64, bool)]) -> Affine {
    let a = t.anchor.as_ref().map(|p| p.vec2_at(frame)).unwrap_or([0.0, 0.0]);
    let p = match &t.position {
        crate::model::Position::Joined(p) => p.vec2_at(frame),
        crate::model::Position::Split(x, y) => [x.scalar_at(frame), y.scalar_at(frame)],
        crate::model::Position::None => [0.0, 0.0],
    };
    let s = t.scale.as_ref().map(|p| p.vec2_at(frame)).unwrap_or([100.0, 100.0]);
    let r = t.rotation.as_ref().map(|p| p.scalar_at(frame)).unwrap_or(0.0);
    let mut acc = Steps { p: [0.0, 0.0], r: 0.0, s: [1.0, 1.0] };
    for (perc, inv) in steps {
        let dir = if *inv { -1.0 } else { 1.0 };
        acc.p[0] += p[0] * dir * perc;
        acc.p[1] += p[1] * dir * perc;
        acc.r += r * dir * perc;
        for k in 0..2 {
            let sv = s[k] / 100.0;
            let f = sv + (1.0 - sv) * (1.0 - perc);
            acc.s[k] *= if *inv { 1.0 / f } else { f };
        }
    }
    // Column order: P · S · R, each of S and R about the anchor.
    Affine::translate(acc.p[0], acc.p[1])
        .then(&Affine::translate(a[0], a[1]))
        .then(&Affine::scale(acc.s[0], acc.s[1]))
        .then(&Affine::rotate_deg(acc.r))
        .then(&Affine::translate(-a[0], -a[1]))
}

#[allow(clippy::too_many_arguments)]
fn repeat(
    _ctx: &mut ShapeCtx,
    geos: &mut Vec<Geo>,
    nodes: &mut Vec<Node>,
    geo_start: usize,
    node_start: usize,
    group_xf: &Affine,
    copies: f64,
    offset: f64,
    below: bool,
    t: &Transform,
    frame: f64,
) {
    let n = copies.ceil().clamp(0.0, 1000.0) as usize;
    let base_geos: Vec<Geo> = geos.drain(geo_start..).collect();
    let base_nodes: Vec<Node> = nodes.drain(node_start..).collect();
    if n == 0 {
        return;
    }
    let so = t.start_opacity.as_ref().map(|p| p.scalar_at(frame)).unwrap_or(100.0) / 100.0;
    let eo = t.end_opacity.as_ref().map(|p| p.scalar_at(frame)).unwrap_or(100.0) / 100.0;
    // The offset's steps, then one more step per copy after the first.
    let mut steps: Vec<(f64, bool)> = Vec::new();
    let whole = offset.trunc();
    let frac = offset - whole;
    for _ in 0..(whole.abs() as usize).min(1000) {
        steps.push((1.0, offset < 0.0));
    }
    if frac != 0.0 {
        steps.push((frac.abs(), offset < 0.0));
    }
    let iteration_nonzero = offset != 0.0;
    let inv_group = group_xf.invert().unwrap_or(Affine::IDENTITY);
    // Copy g (0 = top of the list) gets transform step order k.
    let mut worlds = vec![Affine::IDENTITY; n];
    for k in 0..n {
        let g = if below { n - 1 - k } else { k };
        if k > 0 {
            steps.push((1.0, false));
        }
        let m = if k == 0 && !iteration_nonzero { Affine::IDENTITY } else { repeater_matrix(t, frame, &steps) };
        // In layer space: G · M · G⁻¹.
        worlds[g] = group_xf.then(&m).then(&inv_group);
    }
    for (g, world) in worlds.iter().enumerate() {
        let alpha = if n == 1 { so } else { so + (eo - so) * (g as f64 / (n - 1) as f64) };
        let base = geos.len();
        geos.extend(base_geos.iter().map(|geo| Geo { path: geo.path.clone(), more: geo.more.clone(), xf: world.then(&geo.xf), merge: geo.merge }));
        let remap = |i: usize| i - geo_start + base;
        let cn: Vec<Node> = base_nodes.iter().map(|node| transform_node(node, world, &remap)).collect();
        if alpha < 1.0 {
            nodes.push(Node::Group { alpha: alpha.max(0.0), nodes: cn });
        } else {
            nodes.extend(cn);
        }
    }
}

fn transform_node(node: &Node, world: &Affine, remap: &dyn Fn(usize) -> usize) -> Node {
    match node {
        Node::Paint { paint, xf, geos } => Node::Paint {
            paint: paint.clone(),
            xf: world.then(xf),
            geos: geos.iter().map(|&i| remap(i)).collect(),
        },
        Node::Group { alpha, nodes } => Node::Group {
            alpha: *alpha,
            nodes: nodes.iter().map(|n| transform_node(n, world, remap)).collect(),
        },
    }
}

fn emit_nodes(nodes: &[Node], arena: &[Geo], layer_xf: &Affine, em: &mut Emitter) {
    for node in nodes.iter().rev() {
        match node {
            Node::Group { alpha, nodes } => {
                if *alpha <= 0.0 {
                    continue;
                }
                em.begin_layer(Blend::Normal, *alpha as f32, Vec::new());
                emit_nodes(nodes, arena, layer_xf, em);
                em.pop();
            }
            Node::Paint { paint, xf, geos } => {
                let set: Vec<Geo> = geos.iter().filter_map(|&i| arena.get(i).cloned()).collect();
                emit_paint(paint, xf, &set, layer_xf, em)
            }
        }
    }
}

/// Paths of `geos` in the style group's space (`style_xf`), as verbs.
fn verbs_in(geos: &[Geo], inv_style: &Affine, out: &mut Vec<Verb>) {
    for g in geos {
        let m = inv_style.then(&g.xf);
        g.path.to_verbs(&m, out);
        for p in &g.more {
            p.to_verbs(&m, out);
        }
    }
}

fn emit_paint(paint: &Paint, xf: &Affine, geos: &[Geo], layer_xf: &Affine, em: &mut Emitter) {
    let Some(inv) = xf.invert() else { return };
    let world = layer_xf.then(xf);
    if !world.is_finite() {
        return;
    }
    // Plain paths, and merge-path sets drawn as compositing groups.
    let mut plain: Vec<Verb> = Vec::new();
    let mut k = 0;
    let mut merged_sets: Vec<&[Geo]> = Vec::new();
    while k < geos.len() {
        match geos[k].merge {
            None => {
                verbs_in(&geos[k..k + 1], &inv, &mut plain);
                k += 1;
            }
            Some((id, _)) => {
                let end = geos[k..].iter().position(|g| g.merge.map(|m| m.0) != Some(id)).map(|p| k + p).unwrap_or(geos.len());
                merged_sets.push(&geos[k..end]);
                k = end;
            }
        }
    }
    let t = world.to_dl();
    let op = |path: Vec<Verb>| match paint {
        Paint::Fill { brush, rule } => Op::Fill { transform: t, rule: *rule, brush: brush.clone(), path },
        Paint::Stroke { brush, style } => Op::Stroke { transform: t, style: style.clone(), brush: brush.clone(), path },
    };
    if !plain.is_empty() {
        em.push(op(plain));
    }
    for set in merged_sets {
        match paint {
            // Strokes outline every merged path (no boolean geometry).
            Paint::Stroke { .. } => {
                let mut v = Vec::new();
                verbs_in(set, &inv, &mut v);
                em.push(op(v));
            }
            Paint::Fill { .. } if set.iter().all(|g| g.merge.map(|m| m.1 <= 1).unwrap_or(true)) => {
                // Merge mode 1: one compound path (the fill rule decides holes).
                let mut v = Vec::new();
                verbs_in(set, &inv, &mut v);
                em.push(op(v));
            }
            Paint::Fill { .. } => {
                em.begin_layer(Blend::Normal, 1.0, Vec::new());
                for g in set {
                    let mut v = Vec::new();
                    verbs_in(std::slice::from_ref(g), &inv, &mut v);
                    let blend = match g.merge.map(|m| m.1).unwrap_or(1) {
                        3 => Some(Blend::DstOut),
                        4 => Some(Blend::DstIn),
                        5 => Some(Blend::Xor),
                        _ => None,
                    };
                    match blend {
                        Some(b) => {
                            em.begin_layer(b, 1.0, Vec::new());
                            em.push(op(v));
                            em.pop();
                        }
                        None => em.push(op(v)),
                    }
                }
                em.pop();
            }
        }
    }
}
