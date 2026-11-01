//! Layer effects (`ef`) lowered to display-list filters and ops: drop
//! shadow, gaussian blur, fill, tint, tritone, stroke — the set lottie-web,
//! lottie-ios and lottie-android all support. Parameter meaning and value
//! scaling follow lottie-web's SVG effects (`svgElements/effects/*.js`):
//!
//! | ty | effect        | lowering |
//! |----|---------------|----------|
//! | 25 | Drop Shadow   | `DropShadow` (σ = softness / 4, offset = distance at direction − 90°) |
//! | 29 | Gaussian Blur | `Blur` (σ = blurriness × 0.3, per dimension) |
//! | 21 | Fill          | `ColorMatrix` (rgb := colour, alpha × opacity) |
//! | 20 | Tint          | `GradientMap` black→white, mixed by amount |
//! | 23 | Tritone       | `GradientMap` dark→mid→bright, mixed by 1 − blend |
//! | 22 | Stroke        | `Stroke` ops along the layer's masks ([`paint_after`]) |
//!
//! Sigmas and offsets are in display-list (root composition) units: the
//! layer-space values times the layer matrix. Disabled and unknown effects
//! are skipped.

use crate::display::{Blend, Brush, Cap, Filter, Join, Op, Stop, StrokeStyle};
use crate::geom::{Affine, BPath, Pt};
use crate::model::{Animation, Effect, Layer};
use crate::render::Emitter;

pub const TY_TINT: i64 = 20;
pub const TY_FILL: i64 = 21;
pub const TY_STROKE: i64 = 22;
pub const TY_TRITONE: i64 = 23;
pub const TY_DROP_SHADOW: i64 = 25;
pub const TY_GAUSSIAN_BLUR: i64 = 29;

/// lottie-web's `kBlurrinessToSigma` (empirical, matches AE).
const BLURRINESS_TO_SIGMA: f64 = 0.3;

/// The identity colour matrix (an isolation marker; painters skip it).
pub const IDENTITY_MATRIX: [f32; 20] = [
    1.0, 0.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0,
];

fn scalar(e: &Effect, i: usize, frame: f64, default: f64) -> f64 {
    e.values.get(i).map(|p| p.scalar_at(frame)).filter(|v| v.is_finite()).unwrap_or(default)
}

fn color(e: &Effect, i: usize, frame: f64) -> [f32; 4] {
    let v = e.values.get(i).map(|p| p.eval(frame)).unwrap_or_default();
    let c = |k: usize, d: f64| v.get(k).copied().filter(|x| x.is_finite()).unwrap_or(d).clamp(0.0, 1.0) as f32;
    [c(0, 0.0), c(1, 0.0), c(2, 0.0), c(3, 1.0)]
}

fn opaque(c: [f32; 4]) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

/// Axis scales and the uniform (area) scale of `m`'s linear part.
fn scales(m: &Affine) -> (f64, f64, f64) {
    let a = m.0;
    let sx = a[0].hypot(a[1]);
    let sy = a[2].hypot(a[3]);
    (sx, sy, m.determinant().abs().sqrt())
}

fn active(layer: &Layer) -> impl Iterator<Item = &Effect> {
    layer.effects.iter().filter(|e| e.enabled)
}

/// The filters a layer's effects apply to its content, in order.
pub fn filters(layer: &Layer, frame: f64, m: &Affine, _anim: &Animation) -> Vec<Filter> {
    let mut out = Vec::new();
    for e in active(layer) {
        if let Some(f) = lower(e, frame, m) {
            out.push(f);
        }
    }
    out
}

/// Whether the layer's effects need an isolated group even without filters:
/// a stroke painted "on transparent" or "revealing the original" clears or
/// masks the layer's own content in `paint_after`.
pub fn isolates(layer: &Layer, frame: f64) -> bool {
    active(layer).any(|e| e.ty == TY_STROKE && stroke_style_of(e, frame) != 1)
}

/// One effect's filter (`None` for no-ops, strokes and unknown effects).
pub fn lower(e: &Effect, frame: f64, m: &Affine) -> Option<Filter> {
    let (sx, sy, s) = scales(m);
    match e.ty {
        TY_DROP_SHADOW => {
            let c = color(e, 0, frame);
            let opacity = (scalar(e, 1, frame, 255.0) / 255.0).clamp(0.0, 1.0) as f32;
            let angle = (scalar(e, 2, frame, 135.0) - 90.0).to_radians();
            let distance = scalar(e, 3, frame, 0.0);
            let softness = scalar(e, 4, frame, 0.0).max(0.0);
            let (lx, ly) = (distance * angle.cos(), distance * angle.sin());
            let a = m.0;
            let (dx, dy) = (a[0] * lx + a[2] * ly, a[1] * lx + a[3] * ly);
            let color = [c[0], c[1], c[2], c[3] * opacity];
            (color[3] > 0.0).then_some(Filter::DropShadow {
                color,
                dx: dx as f32,
                dy: dy as f32,
                sigma: (softness / 4.0 * s) as f32,
            })
        }
        TY_GAUSSIAN_BLUR => {
            let sigma = scalar(e, 0, frame, 0.0).max(0.0) * BLURRINESS_TO_SIGMA;
            let dims = scalar(e, 1, frame, 1.0).round() as i64;
            let sigma_x = if dims == 3 { 0.0 } else { sigma * sx };
            let sigma_y = if dims == 2 { 0.0 } else { sigma * sy };
            (sigma_x > 0.0 || sigma_y > 0.0).then_some(Filter::Blur { sigma_x: sigma_x as f32, sigma_y: sigma_y as f32 })
        }
        TY_FILL => {
            let c = color(e, 2, frame);
            let opacity = scalar(e, 6, frame, 1.0).clamp(0.0, 1.0) as f32;
            #[rustfmt::skip]
            let m = [
                0.0, 0.0, 0.0, 0.0, c[0],
                0.0, 0.0, 0.0, 0.0, c[1],
                0.0, 0.0, 0.0, 0.0, c[2],
                0.0, 0.0, 0.0, opacity, 0.0,
            ];
            Some(Filter::ColorMatrix { m })
        }
        TY_TINT => {
            let amount = (scalar(e, 2, frame, 100.0) / 100.0).clamp(0.0, 1.0) as f32;
            (amount > 0.0).then(|| Filter::GradientMap {
                stops: vec![
                    Stop { offset: 0.0, color: opaque(color(e, 0, frame)) },
                    Stop { offset: 1.0, color: opaque(color(e, 1, frame)) },
                ],
                amount,
            })
        }
        TY_TRITONE => {
            let amount = (1.0 - scalar(e, 3, frame, 0.0) / 100.0).clamp(0.0, 1.0) as f32;
            (amount > 0.0).then(|| Filter::GradientMap {
                stops: vec![
                    Stop { offset: 0.0, color: opaque(color(e, 2, frame)) },
                    Stop { offset: 0.5, color: opaque(color(e, 1, frame)) },
                    Stop { offset: 1.0, color: opaque(color(e, 0, frame)) },
                ],
                amount,
            })
        }
        _ => None,
    }
}

/// The Stroke effect's paint style: 1 on original, 2 on transparent, 3 reveal original.
fn stroke_style_of(e: &Effect, frame: f64) -> i64 {
    match scalar(e, 10, frame, 1.0).round() as i64 {
        2 => 2,
        3 => 3,
        _ => 1,
    }
}

/// Effects that draw on top of the content: Stroke (ty 22) strokes the
/// layer's masks (one, or all with "All Masks") with round caps, `brush
/// size × 2` wide; start/end and spacing become dashes like lottie-web.
/// "On transparent" clears the content first; "reveal original" keeps the
/// content only under the stroke.
pub fn paint_after(layer: &Layer, frame: f64, m: &Affine, em: &mut Emitter) {
    for e in active(layer).filter(|e| e.ty == TY_STROKE) {
        let paths = stroke_paths(layer, e, frame);
        if paths.is_empty() {
            continue;
        }
        let style = stroke_style_of(e, frame);
        let c = color(e, 3, frame);
        let brush_size = scalar(e, 4, frame, 1.0).max(0.0);
        let opacity = scalar(e, 6, frame, 1.0).clamp(0.0, 1.0) as f32;
        if brush_size <= 0.0 {
            if style == 3 {
                // Revealed by nothing: the content disappears.
                em.begin_layer(Blend::DstIn, 1.0, Vec::new());
                em.pop();
            }
            continue;
        }
        let start = scalar(e, 7, frame, 0.0);
        let end = scalar(e, 8, frame, 100.0);
        let spacing = scalar(e, 9, frame, 0.0).max(0.0);
        let width = brush_size * 2.0;
        let (rgba, wrap) = match style {
            2 => {
                em.begin_layer(Blend::DstIn, 1.0, Vec::new());
                em.pop();
                ([c[0], c[1], c[2], opacity], false)
            }
            3 => ([1.0, 1.0, 1.0, opacity], true),
            _ => ([c[0], c[1], c[2], opacity], false),
        };
        if wrap {
            em.begin_layer(Blend::DstIn, 1.0, Vec::new());
        }
        for path in &paths {
            let Some((dashes, dash_offset)) = stroke_dashes(path, start, end, width, spacing) else {
                continue;
            };
            let mut verbs = Vec::new();
            path.to_verbs(&Affine::IDENTITY, &mut verbs);
            em.push(Op::Stroke {
                transform: m.to_dl(),
                style: StrokeStyle { width: width as f32, cap: Cap::Round, join: Join::Round, miter: 4.0, dashes, dash_offset },
                brush: Brush::Solid { color: rgba },
                path: verbs,
            });
        }
        if wrap {
            em.pop();
        }
    }
}

/// The mask paths a Stroke effect follows (layer space).
fn stroke_paths(layer: &Layer, e: &Effect, frame: f64) -> Vec<BPath> {
    let all = scalar(e, 1, frame, 0.0).round() as i64 == 1;
    let range = if all {
        0..layer.masks.len()
    } else {
        let k = scalar(e, 0, frame, 1.0).round() as i64 - 1;
        if k < 0 || k as usize >= layer.masks.len() {
            return Vec::new();
        }
        k as usize..k as usize + 1
    };
    layer.masks[range]
        .iter()
        .map(|mask| BPath::from_flat(&mask.path.eval(frame)))
        .filter(|p| !p.is_empty())
        .collect()
}

/// Most dots a stroke effect enumerates before falling back to a solid stroke.
const MAX_DOTS: usize = 1024;

/// lottie-web's stroke-effect dash array as (dashes, offset), `None` when
/// nothing is drawn. Spacing `0` and the full range = a solid stroke;
/// spacing `> 0` = dots of length 1 every `1 + width · spacing%`; a partial
/// range draws only `[start, end]` of the path.
pub fn stroke_dashes(path: &BPath, start: f64, end: f64, width: f64, spacing: f64) -> Option<(Vec<f32>, f32)> {
    let gap = width * spacing * 0.01;
    let full = start == 0.0 && end == 100.0;
    if full {
        // lottie-web: `1 gap` dashes with `stroke-dashoffset: 1`.
        return Some(if gap > 0.0 { (vec![1.0, gap as f32], 1.0) } else { (Vec::new(), 0.0) });
    }
    let s = (start.min(end) * 0.01).clamp(0.0, 1.0);
    let e = (start.max(end) * 0.01).clamp(0.0, 1.0);
    let l = length(path);
    let line = l * (e - s);
    if l <= 0.0 || line <= 0.0 {
        return None;
    }
    // The pattern is "on" for the range, then off past the path's end; the
    // offset makes it start inside that long gap, `l·s` before the range.
    let tail = l * 2.0 + 1.0;
    let mut dashes: Vec<f32> = Vec::new();
    let units = if gap > 0.0 { (line / (1.0 + gap)).floor() as usize } else { 0 };
    if gap > 0.0 && units > 0 && units <= MAX_DOTS {
        for _ in 0..units {
            dashes.extend_from_slice(&[1.0, gap as f32]);
        }
        *dashes.last_mut().unwrap() += tail as f32;
    } else if gap > 0.0 && units == 0 {
        return None;
    } else {
        dashes.extend_from_slice(&[line as f32, tail as f32]);
    }
    let period: f64 = dashes.iter().map(|&d| d as f64).sum();
    Some((dashes, (period - l * s) as f32))
}

/// Arc length of a path (each cubic flattened into 16 chords).
pub fn length(p: &BPath) -> f64 {
    let n = p.v.len();
    if n < 2 {
        return 0.0;
    }
    let seg = |a: usize, b: usize| cubic_length(p.v[a], p.o[a], p.i[b], p.v[b]);
    let mut total: f64 = (1..n).map(|k| seg(k - 1, k)).sum();
    if p.closed {
        total += seg(n - 1, 0);
    }
    total
}

fn cubic_length(p0: Pt, p1: Pt, p2: Pt, p3: Pt) -> f64 {
    if p1 == p0 && p2 == p3 {
        return (p3[0] - p0[0]).hypot(p3[1] - p0[1]);
    }
    let at = |t: f64| {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        [a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]]
    };
    let mut prev = p0;
    let mut total = 0.0;
    for k in 1..=16 {
        let q = at(k as f64 / 16.0);
        total += (q[0] - prev[0]).hypot(q[1] - prev[1]);
        prev = q;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Prop;
    use crate::model::{LayerKind, Mask, MaskMode, Transform};

    fn effect(ty: i64, values: Vec<Vec<f64>>) -> Effect {
        Effect { ty, name: String::new(), enabled: true, values: values.into_iter().map(Prop::fixed).collect() }
    }

    fn layer(effects: Vec<Effect>, masks: Vec<Mask>) -> Layer {
        Layer {
            kind: LayerKind::Null,
            name: String::new(),
            index: None,
            parent: None,
            in_point: 0.0,
            out_point: 10.0,
            start_time: 0.0,
            stretch: 1.0,
            hidden: false,
            transform: Transform::default(),
            auto_orient: false,
            blend: Blend::Normal,
            masks,
            matte_mode: None,
            matte_parent: None,
            is_matte: false,
            effects,
            three_d: false,
        }
    }

    fn square_mask(mode: MaskMode) -> Mask {
        // closed, 4 vertices, relative tangents 0
        let mut flat = vec![1.0, 4.0];
        for [x, y] in [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]] {
            flat.extend_from_slice(&[x, y, 0.0, 0.0, 0.0, 0.0]);
        }
        Mask { mode, inverted: false, path: Prop::fixed(flat), opacity: None, expansion: None, feather: None }
    }

    fn run(l: &Layer, m: &Affine) -> Vec<Filter> {
        filters(l, 0.0, m, &Animation::default())
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn drop_shadow_lowers_like_lottie_web() {
        // colour red, opacity 127.5, direction 180° (straight down), distance 10, softness 8.
        let e = effect(TY_DROP_SHADOW, vec![vec![1.0, 0.0, 0.0, 1.0], vec![127.5], vec![180.0], vec![10.0], vec![8.0]]);
        let f = run(&layer(vec![e], vec![]), &Affine::scale(2.0, 2.0));
        let [Filter::DropShadow { color, dx, dy, sigma }] = f.as_slice() else { panic!("{f:?}") };
        assert_eq!(*color, [1.0, 0.0, 0.0, 0.5]);
        assert!(close(*dx, 0.0) && close(*dy, 20.0), "{dx} {dy}");
        assert!(close(*sigma, 4.0), "{sigma}"); // 8 / 4 × 2
        // Direction 90° points right; rotation of the layer rotates the offset.
        let e = effect(TY_DROP_SHADOW, vec![vec![0.0, 0.0, 0.0, 1.0], vec![255.0], vec![90.0], vec![5.0], vec![0.0]]);
        let f = run(&layer(vec![e], vec![]), &Affine::rotate_deg(90.0));
        let [Filter::DropShadow { dx, dy, .. }] = f.as_slice() else { panic!() };
        assert!(close(*dx, 0.0) && close(*dy, 5.0), "{dx} {dy}");
    }

    #[test]
    fn gaussian_blur_lowers_per_dimension() {
        let both = effect(TY_GAUSSIAN_BLUR, vec![vec![10.0], vec![1.0], vec![0.0]]);
        let f = run(&layer(vec![both], vec![]), &Affine::scale(2.0, 1.0));
        assert_eq!(f, vec![Filter::Blur { sigma_x: 6.0, sigma_y: 3.0 }]);
        let h = effect(TY_GAUSSIAN_BLUR, vec![vec![10.0], vec![2.0], vec![0.0]]);
        assert_eq!(run(&layer(vec![h], vec![]), &Affine::IDENTITY), vec![Filter::Blur { sigma_x: 3.0, sigma_y: 0.0 }]);
        let v = effect(TY_GAUSSIAN_BLUR, vec![vec![10.0], vec![3.0], vec![0.0]]);
        assert_eq!(run(&layer(vec![v], vec![]), &Affine::IDENTITY), vec![Filter::Blur { sigma_x: 0.0, sigma_y: 3.0 }]);
        let zero = effect(TY_GAUSSIAN_BLUR, vec![vec![0.0], vec![1.0], vec![0.0]]);
        assert!(run(&layer(vec![zero], vec![]), &Affine::IDENTITY).is_empty());
    }

    #[test]
    fn fill_lowers_to_a_colour_matrix() {
        let e = effect(
            TY_FILL,
            vec![vec![0.0], vec![0.0], vec![0.2, 0.4, 0.6, 1.0], vec![0.0], vec![0.0], vec![0.0], vec![0.5]],
        );
        let f = run(&layer(vec![e], vec![]), &Affine::IDENTITY);
        let [Filter::ColorMatrix { m }] = f.as_slice() else { panic!("{f:?}") };
        assert_eq!(m[4], 0.2);
        assert_eq!(m[9], 0.4);
        assert_eq!(m[14], 0.6);
        assert_eq!(m[18], 0.5);
        assert_eq!(m.iter().filter(|v| **v != 0.0).count(), 4);
    }

    #[test]
    fn tint_and_tritone_lower_to_gradient_maps() {
        let tint = effect(TY_TINT, vec![vec![0.1, 0.0, 0.0, 1.0], vec![0.0, 0.0, 0.9, 1.0], vec![40.0]]);
        let f = run(&layer(vec![tint], vec![]), &Affine::IDENTITY);
        assert_eq!(
            f,
            vec![Filter::GradientMap {
                stops: vec![
                    Stop { offset: 0.0, color: [0.1, 0.0, 0.0, 1.0] },
                    Stop { offset: 1.0, color: [0.0, 0.0, 0.9, 1.0] },
                ],
                amount: 0.4,
            }]
        );
        let tritone = effect(
            TY_TRITONE,
            vec![vec![1.0, 1.0, 1.0, 1.0], vec![0.5, 0.0, 0.0, 1.0], vec![0.0, 0.0, 0.0, 1.0], vec![25.0]],
        );
        let f = run(&layer(vec![tritone], vec![]), &Affine::IDENTITY);
        let [Filter::GradientMap { stops, amount }] = f.as_slice() else { panic!("{f:?}") };
        assert_eq!(*amount, 0.75);
        assert_eq!(stops.iter().map(|s| s.offset).collect::<Vec<_>>(), vec![0.0, 0.5, 1.0]);
        assert_eq!(stops[0].color, [0.0, 0.0, 0.0, 1.0]); // dark
        assert_eq!(stops[1].color, [0.5, 0.0, 0.0, 1.0]); // mid
        assert_eq!(stops[2].color, [1.0, 1.0, 1.0, 1.0]); // bright
    }

    #[test]
    fn disabled_and_unknown_effects_are_skipped() {
        let mut e = effect(TY_GAUSSIAN_BLUR, vec![vec![10.0], vec![1.0], vec![0.0]]);
        e.enabled = false;
        let unknown = effect(5, vec![vec![1.0]]);
        assert!(run(&layer(vec![e, unknown], vec![]), &Affine::IDENTITY).is_empty());
    }

    fn stroke_effect(style: f64, start: f64, end: f64, spacing: f64) -> Effect {
        effect(
            TY_STROKE,
            vec![
                vec![1.0],                // path: mask 1
                vec![0.0],                // all masks
                vec![0.0],                // sequentially
                vec![0.0, 1.0, 0.0, 1.0], // colour
                vec![2.0],                // brush size
                vec![1.0],                // hardness
                vec![0.5],                // opacity
                vec![start],
                vec![end],
                vec![spacing],
                vec![style],
            ],
        )
    }

    /// Sample a 100×100 file whose one shape layer (scaled 200%) carries
    /// `effects` (JSON) and `masks` (JSON), returning the ops.
    fn emit_json(effects: &str, masks: &str) -> Vec<Op> {
        let json = format!(
            r#"{{"v":"5.7.0","fr":30,"ip":0,"op":30,"w":100,"h":100,"layers":[
             {{"ty":4,"ind":1,"ip":0,"op":30,"st":0,
               "ks":{{"o":{{"a":0,"k":100}},"p":{{"a":0,"k":[0,0]}},"a":{{"a":0,"k":[0,0]}},"s":{{"a":0,"k":[200,200]}},"r":{{"a":0,"k":0}}}},
               "hasMask":true,"masksProperties":[{masks}],"ef":[{effects}],
               "shapes":[{{"ty":"rc","p":{{"a":0,"k":[5,5]}},"s":{{"a":0,"k":[10,10]}},"r":{{"a":0,"k":0}}}},
                         {{"ty":"fl","c":{{"a":0,"k":[1,0,0,1]}},"o":{{"a":0,"k":100}}}}]}}]}}"#
        );
        let motion = crate::Motion::load(json.as_bytes()).expect("loads");
        let mut list = crate::DisplayList::default();
        motion.sample(0.0, &mut list);
        list.ops
    }

    const MASK_NONE: &str = r#"{"mode":"n","inv":false,"o":{"a":0,"k":100},"pt":{"a":0,"k":{"c":true,"v":[[0,0],[10,0],[10,10],[0,10]],"i":[[0,0],[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0],[0,0]]}}}"#;

    fn stroke_json(all: u8, path: u8, style: u8) -> String {
        let v = |ty: u8, k: &str| format!(r#"{{"ty":{ty},"v":{{"a":0,"k":{k}}}}}"#);
        let controls = [
            v(11, &path.to_string()),
            v(7, &all.to_string()),
            v(7, "0"),
            v(2, "[0,1,0,1]"),
            v(0, "2"),
            v(0, "1"),
            v(0, "0.5"),
            v(0, "0"),
            v(0, "100"),
            v(0, "0"),
            v(7, &style.to_string()),
        ];
        format!(r#"{{"ty":22,"nm":"Stroke","en":1,"ef":[{}]}}"#, controls.join(","))
    }

    fn emit(style: u8) -> Vec<Op> {
        emit_json(&stroke_json(0, 1, style), MASK_NONE)
    }

    fn strokes(ops: &[Op]) -> usize {
        ops.iter().filter(|o| matches!(o, Op::Stroke { .. })).count()
    }

    #[test]
    fn stroke_on_original_strokes_the_mask() {
        let l = layer(vec![stroke_effect(1.0, 0.0, 100.0, 0.0)], vec![square_mask(MaskMode::None)]);
        assert!(run(&l, &Affine::IDENTITY).is_empty(), "on original needs no isolation");
        let ops = emit(1);
        assert!(!ops.iter().any(|o| matches!(o, Op::Layer { .. })), "{ops:?}");
        let Some(Op::Stroke { style, brush, transform, path }) = ops.iter().find(|o| matches!(o, Op::Stroke { .. })) else {
            panic!("{ops:?}")
        };
        assert_eq!(strokes(&ops), 1);
        assert_eq!(style.width, 4.0);
        assert_eq!(style.cap, Cap::Round);
        assert!(style.dashes.is_empty());
        assert_eq!(*brush, Brush::Solid { color: [0.0, 1.0, 0.0, 0.5] });
        assert_eq!(*transform, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
        assert_eq!(path.len(), 6); // M + 3 L + closing L + Z
        // The stroke comes after the content's fill.
        let fill = ops.iter().position(|o| matches!(o, Op::Fill { .. })).unwrap();
        let stroke = ops.iter().position(|o| matches!(o, Op::Stroke { .. })).unwrap();
        assert!(fill < stroke);
    }

    #[test]
    fn stroke_on_transparent_and_reveal_isolate_and_clear() {
        let l = layer(vec![stroke_effect(2.0, 0.0, 100.0, 0.0)], vec![square_mask(MaskMode::None)]);
        assert!(run(&l, &Affine::IDENTITY).is_empty() && isolates(&l, 0.0));
        let ops = emit(2);
        let k = ops.iter().position(|o| matches!(o, Op::Layer { blend: Blend::Normal, .. })).expect("isolated");
        let fill = ops.iter().position(|o| matches!(o, Op::Fill { .. })).unwrap();
        assert!(k < fill);
        // fill, [dst-in layer, pop] clears it, then the stroke on transparent.
        assert!(matches!(ops[fill + 1], Op::Layer { blend: Blend::DstIn, .. }), "{ops:?}");
        assert!(matches!(ops[fill + 2], Op::Pop));
        assert!(matches!(ops[fill + 3], Op::Stroke { .. }));

        let ops = emit(3);
        let fill = ops.iter().position(|o| matches!(o, Op::Fill { .. })).unwrap();
        assert!(matches!(ops[fill + 1], Op::Layer { blend: Blend::DstIn, .. }), "{ops:?}");
        let Op::Stroke { brush, .. } = &ops[fill + 2] else { panic!("{ops:?}") };
        assert_eq!(*brush, Brush::Solid { color: [1.0, 1.0, 1.0, 0.5] });
        assert!(matches!(ops[fill + 3], Op::Pop));
    }

    #[test]
    fn stroke_range_and_spacing_become_dashes() {
        let mut sq = BPath::default();
        sq.closed = true;
        for v in [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]] {
            sq.push(v, v, v);
        }
        assert!((length(&sq) - 40.0).abs() < 1e-9);
        // 25%..75% solid: off 10, on 20.
        let (d, off) = stroke_dashes(&sq, 25.0, 75.0, 4.0, 0.0).unwrap();
        assert_eq!(d, vec![20.0, 81.0]);
        assert!(close(off, 101.0 - 10.0));
        // spacing 50% of width 4 = gaps of 2: dots every 3 over 20 → 6 dots.
        let (d, _) = stroke_dashes(&sq, 25.0, 75.0, 4.0, 50.0).unwrap();
        assert_eq!(d.len(), 12);
        assert_eq!(&d[..2], &[1.0, 2.0]);
        // Full range with spacing: periodic dots.
        assert_eq!(stroke_dashes(&sq, 0.0, 100.0, 4.0, 50.0), Some((vec![1.0, 2.0], 1.0)));
        // Empty range draws nothing.
        assert_eq!(stroke_dashes(&sq, 30.0, 30.0, 4.0, 0.0), None);
    }

    #[test]
    fn stroke_all_masks_and_missing_mask() {
        let masks = format!("{MASK_NONE},{MASK_NONE}");
        assert_eq!(strokes(&emit_json(&stroke_json(1, 1, 1), &masks)), 2);
        assert_eq!(strokes(&emit_json(&stroke_json(0, 2, 1), &masks)), 1);
        assert_eq!(strokes(&emit_json(&stroke_json(0, 3, 1), &masks)), 0);
    }
}
