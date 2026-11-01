//! Shape modifiers, ported from lottie-web (MIT): trim, round corners,
//! pucker/bloat, zig-zag, offset path, twist. Each works on paths in their
//! LOCAL space (lottie-web applies modifiers before transforms).
//!
//! Sources (lottie-web `player/js/utils/`): `shapes/TrimModifier.js` +
//! `bez.js` (trim), `shapes/RoundCornersModifier.js`,
//! `shapes/PuckerAndBloatModifier.js`, `shapes/ZigZagModifier.js`,
//! `shapes/OffsetPathModifier.js` + `PolynomialBezier.js`. Twist has no
//! lottie-web (nor lottie-android/ios, rlottie, ThorVG, Skottie) counterpart;
//! see [`twist`].

use crate::geom::{BPath, Pt, KAPPA};
use crate::shapes::Geo;

/// lottie-web `getDefaultCurveSegments()`.
const CURVE_SEGMENTS: usize = 150;

// ---------------------------------------------------------------------------
// Trim (TrimModifier.js + bez.js)
// ---------------------------------------------------------------------------

/// lottie-web `bezierLengthPool` element: cumulative sample lengths (stored
/// in Float32Arrays there, so rounded through f32 here too).
struct BezLen {
    percents: Vec<f64>,
    lengths: Vec<f64>,
    added: f64,
}

/// `bez.getBezierLength(pt1, pt2, pt3, pt4)`: pt3/pt4 = absolute controls.
fn bezier_length(p1: Pt, p2: Pt, c1: Pt, c2: Pt) -> BezLen {
    let n = CURVE_SEGMENTS;
    let mut percents = Vec::with_capacity(n);
    let mut lengths = Vec::with_capacity(n);
    let mut added = 0.0;
    let mut last: Option<Pt> = None;
    for k in 0..n {
        let perc = k as f64 / (n - 1) as f64;
        let u = 1.0 - perc;
        let mut pt = [0.0; 2];
        for d in 0..2 {
            pt[d] = u * u * u * p1[d] + 3.0 * u * u * perc * c1[d] + 3.0 * u * perc * perc * c2[d] + perc * perc * perc * p2[d];
        }
        if let Some(l) = last {
            let dist = ((pt[0] - l[0]).powi(2) + (pt[1] - l[1]).powi(2)).sqrt();
            if dist != 0.0 {
                added += dist;
            }
        }
        last = Some(pt);
        percents.push(perc as f32 as f64);
        lengths.push(added as f32 as f64);
    }
    BezLen { percents, lengths, added }
}

/// `bez.getSegmentsLength(shapeData)`.
fn segments_length(p: &BPath) -> (Vec<BezLen>, f64) {
    let n = p.len();
    let mut out = Vec::with_capacity(n);
    let mut total = 0.0;
    for k in 0..n.saturating_sub(1) {
        let l = bezier_length(p.v[k], p.v[k + 1], p.o[k], p.i[k + 1]);
        total += l.added;
        out.push(l);
    }
    if p.closed && n > 0 {
        let k = n - 1;
        let l = bezier_length(p.v[k], p.v[0], p.o[k], p.i[0]);
        total += l.added;
        out.push(l);
    }
    (out, total)
}

/// `bez.getDistancePerc`: arc-length fraction → curve parameter.
fn distance_perc(perc: f64, d: &BezLen) -> f64 {
    let len = d.percents.len() as isize;
    let mut init = (((len - 1) as f64) * perc).floor() as isize;
    init = init.clamp(0, len - 1);
    let length_pos = perc * d.added;
    let at = |k: isize| d.lengths[k as usize];
    if init == len - 1 || init == 0 || length_pos == at(init) {
        return d.percents[init as usize];
    }
    let dir = if at(init) > length_pos { -1 } else { 1 };
    let mut l_perc = 0.0;
    loop {
        if at(init) <= length_pos && at(init + 1) > length_pos {
            l_perc = (length_pos - at(init)) / (at(init + 1) - at(init));
            break;
        }
        init += dir;
        if init < 0 || init >= len - 1 {
            if init == len - 1 {
                return d.percents[init as usize];
            }
            init = init.clamp(0, len - 2);
            break;
        }
    }
    let k = init as usize;
    d.percents[k] + (d.percents[k + 1] - d.percents[k]) * l_perc
}

/// JS `Math.round` (half toward +∞).
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `bez.getNewSegment`: the sub-curve between two arc-length fractions, as
/// `[x0, x1, x2, x3, y0, y1, y2, y3]` rounded to 1/1000 in a Float32Array.
fn new_segment(p1: Pt, p2: Pt, c1: Pt, c2: Pt, start: f64, end: f64, d: &BezLen) -> [f64; 8] {
    let start = start.clamp(0.0, 1.0);
    let t0 = distance_perc(start, d);
    let end = if end > 1.0 { 1.0 } else { end };
    let t1 = distance_perc(end, d);
    let (u0, u1) = (1.0 - t0, 1.0 - t1);
    let w = [
        [u0 * u0 * u0, t0 * u0 * u0 * 3.0, t0 * t0 * u0 * 3.0, t0 * t0 * t0],
        [u0 * u0 * u1, t0 * u0 * u1 + u0 * t0 * u1 + u0 * u0 * t1, t0 * t0 * u1 + u0 * t0 * t1 + t0 * u0 * t1, t0 * t0 * t1],
        [u0 * u1 * u1, t0 * u1 * u1 + u0 * t1 * u1 + u0 * u1 * t1, t0 * t1 * u1 + u0 * t1 * t1 + t0 * u1 * t1, t0 * t1 * t1],
        [u1 * u1 * u1, t1 * u1 * u1 + u1 * t1 * u1 + u1 * u1 * t1, t1 * t1 * u1 + u1 * t1 * t1 + t1 * u1 * t1, t1 * t1 * t1],
    ];
    let mut out = [0.0; 8];
    for d in 0..2 {
        for (k, w) in w.iter().enumerate() {
            let v = w[0] * p1[d] + w[1] * c1[d] + w[2] * c2[d] + w[3] * p2[d];
            out[d * 4 + k] = (js_round(v * 1000.0) / 1000.0) as f32 as f64;
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Slot {
    V,
    I,
    O,
}

/// ShapePath `setXYAt`: grows the path to `pos + 1` vertices as needed.
fn set_at(p: &mut BPath, slot: Slot, pos: usize, pt: Pt) {
    if p.v.len() <= pos {
        p.v.resize(pos + 1, [0.0; 2]);
        p.i.resize(pos + 1, [0.0; 2]);
        p.o.resize(pos + 1, [0.0; 2]);
    }
    match slot {
        Slot::V => p.v[pos] = pt,
        Slot::I => p.i[pos] = pt,
        Slot::O => p.o[pos] = pt,
    }
}

/// `TrimModifier.addSegment`.
fn add_segment(v0: Pt, o0: Pt, i1: Pt, v1: Pt, out: &mut BPath, pos: usize, new_shape: bool) {
    set_at(out, Slot::O, pos, o0);
    set_at(out, Slot::I, pos + 1, i1);
    if new_shape {
        set_at(out, Slot::V, pos, v0);
    }
    set_at(out, Slot::V, pos + 1, v1);
}

/// `TrimModifier.addSegmentFromArray`.
fn add_segment_from_array(p: &[f64; 8], out: &mut BPath, pos: usize, new_shape: bool) {
    set_at(out, Slot::O, pos, [p[1], p[5]]);
    set_at(out, Slot::I, pos + 1, [p[2], p[6]]);
    if new_shape {
        set_at(out, Slot::V, pos, [p[0], p[4]]);
    }
    set_at(out, Slot::V, pos + 1, [p[3], p[7]]);
}

/// `TrimModifier.addShapes`: the part of a shape's subpaths `src` (one
/// shared length axis, in order) between lengths `s` and `e`. With `cont`,
/// the first piece continues that path (a closed shape's wrap-around).
fn add_shapes(src: &[BPath], lens: &[Vec<BezLen>], s: f64, e: f64, cont: Option<BPath>) -> Vec<BPath> {
    let (mut out, mut seg_count, mut init_pos) = match cont {
        Some(p) => {
            let n = p.len();
            (p, n, n)
        }
        None => (BPath::default(), 0, 0),
    };
    let mut shapes = Vec::new();
    let mut new_shape = true;
    let mut added = 0.0;
    for (k, (sp, lens)) in src.iter().zip(lens.iter()).enumerate() {
        out.closed = sp.closed;
        let jlen = if sp.closed { lens.len() } else { lens.len() + 1 };
        let mut j = 1;
        while j < jlen {
            let cur = &lens[j - 1];
            if added + cur.added < s {
                added += cur.added;
                out.closed = false;
            } else if added > e {
                out.closed = false;
                break;
            } else {
                if s <= added && e >= added + cur.added {
                    add_segment(sp.v[j - 1], sp.o[j - 1], sp.i[j], sp.v[j], &mut out, seg_count, new_shape);
                } else {
                    let seg = new_segment(sp.v[j - 1], sp.v[j], sp.o[j - 1], sp.i[j], (s - added) / cur.added, (e - added) / cur.added, cur);
                    add_segment_from_array(&seg, &mut out, seg_count, new_shape);
                    out.closed = false;
                }
                new_shape = false;
                added += cur.added;
                seg_count += 1;
            }
            j += 1;
        }
        if sp.closed && !lens.is_empty() {
            let cur = &lens[j - 1];
            if added <= e {
                let sl = cur.added;
                if s <= added && e >= added + sl {
                    add_segment(sp.v[j - 1], sp.o[j - 1], sp.i[0], sp.v[0], &mut out, seg_count, new_shape);
                } else {
                    let seg = new_segment(sp.v[j - 1], sp.v[0], sp.o[j - 1], sp.i[0], (s - added) / sl, (e - added) / sl, cur);
                    add_segment_from_array(&seg, &mut out, seg_count, new_shape);
                    out.closed = false;
                }
            } else {
                out.closed = false;
            }
            // (lottie-web also bumps newShape/segmentCount here; both reset before reuse.)
            added += cur.added;
        }
        let n = out.len();
        if n > 0 {
            if init_pos < n {
                out.i[init_pos] = out.v[init_pos];
            }
            out.o[n - 1] = out.v[n - 1];
        }
        let stop = added > e;
        if stop || k + 1 == src.len() {
            break;
        }
        shapes.push(std::mem::take(&mut out));
        new_shape = true;
        seg_count = 0;
        // lottie-web keeps the stale `initPos` here (pooled paths hide it);
        // a fresh path starts at 0.
        init_pos = 0;
    }
    shapes.push(out);
    shapes
}

/// `TrimModifier.calculateShapeEdges` (m = 2): this shape's `[s, e]` ranges
/// given the lengths of the shapes before it.
fn shape_edges(s: f64, e: f64, shape_len: f64, added_len: f64, total: f64) -> Vec<[f64; 2]> {
    let segs: Vec<[f64; 2]> = if e <= 1.0 {
        vec![[s, e]]
    } else if s >= 1.0 {
        vec![[s - 1.0, e - 1.0]]
    } else {
        vec![[s, 1.0], [0.0, e - 1.0]]
    };
    let mut out = Vec::new();
    for [ss, se] in segs {
        if !(se * total < added_len || ss * total > added_len + shape_len) {
            let a = if ss * total <= added_len { 0.0 } else { (ss * total - added_len) / shape_len };
            let b = if se * total >= added_len + shape_len { 1.0 } else { (se * total - added_len) / shape_len };
            out.push([a, b]);
        }
    }
    if out.is_empty() {
        out.push([0.0, 0.0]);
    }
    out
}

/// Trim paths. `start`, `end` in percent, `offset` in degrees (360 = one
/// full length); `individually` = lottie `m: 2` (trim the paths as one
/// sequence), else each path is trimmed on its own (`m: 1`, simultaneously).
pub fn trim(geos: &mut [Geo], start: f64, end: f64, offset: f64, individually: bool) {
    let (sv, ev) = (start / 100.0, end / 100.0);
    let mut o = (offset % 360.0) / 360.0;
    if o < 0.0 {
        o += 1.0;
    }
    let edge = |v: f64| {
        if v > 1.0 {
            1.0 + o
        } else if v < 0.0 {
            o
        } else {
            v + o
        }
    };
    let (mut s, mut e) = (edge(sv), edge(ev));
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    s = js_round(s * 10000.0) * 0.0001;
    e = js_round(e * 10000.0) * 0.0001;

    if e == s {
        for g in geos.iter_mut() {
            g.path = BPath::default();
            g.more.clear();
        }
        return;
    }
    if (e == 1.0 && s == 0.0) || (e == 0.0 && s == 1.0) {
        return;
    }

    // A geo's subpaths (`path` + `more`) are one lottie-web shape.
    let data: Vec<(Vec<BPath>, Vec<Vec<BezLen>>, f64)> = geos
        .iter_mut()
        .map(|g| {
            let mut src = vec![std::mem::take(&mut g.path)];
            src.append(&mut g.more);
            let (lens, totals): (Vec<_>, Vec<_>) = src.iter().map(segments_length).unzip();
            let total = totals.iter().sum();
            (src, lens, total)
        })
        .collect();
    let total_modifier: f64 = data.iter().map(|d| d.2).sum();
    let multi = individually && geos.len() > 1;
    let mut added_length = 0.0;
    // lottie-web walks its shape list backwards, which is item order.
    for (g, (src, lens, total)) in geos.iter_mut().zip(data.iter()) {
        let total = *total;
        let last_closed = src.last().is_some_and(|p| p.closed);
        let edges = if multi {
            let edges = shape_edges(s, e, total, added_length, total_modifier);
            added_length += total;
            edges
        } else {
            vec![[s, e]]
        };
        let mut pieces: Vec<BPath> = Vec::new();
        for [ss, se] in edges {
            let segs: Vec<[f64; 2]> = if se <= 1.0 {
                vec![[total * ss, total * se]]
            } else if ss >= 1.0 {
                vec![[total * (ss - 1.0), total * (se - 1.0)]]
            } else {
                vec![[total * ss, total], [0.0, total * (se - 1.0)]]
            };
            let mut shapes = add_shapes(src, lens, segs[0][0], segs[0][1], None);
            if segs[0][0] != segs[0][1] {
                if segs.len() > 1 {
                    if last_closed {
                        let last = shapes.pop();
                        pieces.append(&mut shapes);
                        shapes = add_shapes(src, lens, segs[1][0], segs[1][1], last);
                    } else {
                        pieces.append(&mut shapes);
                        shapes = add_shapes(src, lens, segs[1][0], segs[1][1], None);
                    }
                }
                pieces.append(&mut shapes);
            }
        }
        let mut pieces = pieces.into_iter().filter(|p| !p.is_empty());
        g.path = pieces.next().unwrap_or_default();
        g.more = pieces.collect();
    }
}

// ---------------------------------------------------------------------------
// Round corners (RoundCornersModifier.js)
// ---------------------------------------------------------------------------

/// Round corners with radius `r`.
pub fn round_corners(path: &mut BPath, r: f64) {
    if r == 0.0 {
        return;
    }
    let len = path.len();
    let mut out = BPath::with_capacity(len * 2, path.closed);
    for k in 0..len {
        let cv = path.v[k];
        let co = path.o[k];
        let ci = path.i[k];
        if cv == co && cv == ci {
            if (k == 0 || k == len - 1) && !path.closed {
                out.push(cv, co, ci);
            } else {
                let closer = if k == 0 { path.v[len - 1] } else { path.v[k - 1] };
                let dist = (cv[0] - closer[0]).hypot(cv[1] - closer[1]);
                let perc = if dist != 0.0 { (dist / 2.0).min(r) / dist } else { 0.0 };
                let ix = cv[0] + (closer[0] - cv[0]) * perc;
                let iy = cv[1] - (cv[1] - closer[1]) * perc;
                let (vx, vy) = (ix, iy);
                let ox = vx - (vx - cv[0]) * KAPPA;
                let oy = vy - (vy - cv[1]) * KAPPA;
                out.push([vx, vy], [ox, oy], [ix, iy]);

                let closer = if k == len - 1 { path.v[0] } else { path.v[k + 1] };
                let dist = (cv[0] - closer[0]).hypot(cv[1] - closer[1]);
                let perc = if dist != 0.0 { (dist / 2.0).min(r) / dist } else { 0.0 };
                let ox = cv[0] + (closer[0] - cv[0]) * perc;
                let oy = cv[1] + (closer[1] - cv[1]) * perc;
                let (vx, vy) = (ox, oy);
                let ix = vx - (vx - cv[0]) * KAPPA;
                let iy = vy - (vy - cv[1]) * KAPPA;
                out.push([vx, vy], [ox, oy], [ix, iy]);
            }
        } else {
            out.push(cv, co, ci);
        }
    }
    *path = out;
}

// ---------------------------------------------------------------------------
// Pucker / bloat (PuckerAndBloatModifier.js)
// ---------------------------------------------------------------------------

/// Pucker (negative) / bloat (positive) by `amount` percent.
pub fn pucker_bloat(path: &mut BPath, amount: f64) {
    let n = path.len();
    if amount == 0.0 || n == 0 {
        return;
    }
    let pc = amount / 100.0;
    let mut c = [0.0; 2];
    for v in &path.v {
        c[0] += v[0];
        c[1] += v[1];
    }
    c[0] /= n as f64;
    c[1] /= n as f64;
    for k in 0..n {
        let v = path.v[k];
        path.v[k] = [v[0] + (c[0] - v[0]) * pc, v[1] + (c[1] - v[1]) * pc];
        let o = path.o[k];
        path.o[k] = [o[0] + (c[0] - o[0]) * -pc, o[1] + (c[1] - o[1]) * -pc];
        let i = path.i[k];
        path.i[k] = [i[0] + (c[0] - i[0]) * -pc, i[1] + (c[1] - i[1]) * -pc];
    }
}

// ---------------------------------------------------------------------------
// PolynomialBezier.js
// ---------------------------------------------------------------------------

fn float_equal(a: f64, b: f64) -> bool {
    (a - b).abs() * 100000.0 <= a.abs().min(b.abs())
}

fn float_zero(f: f64) -> bool {
    f.abs() <= 0.00001
}

fn lerp_point(a: Pt, b: Pt, t: f64) -> Pt {
    [a[0] * (1.0 - t) + b[0] * t, a[1] * (1.0 - t) + b[1] * t]
}

fn point_equal(a: Pt, b: Pt) -> bool {
    float_equal(a[0], b[0]) && float_equal(a[1], b[1])
}

fn point_distance(a: Pt, b: Pt) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn polar_offset(p: Pt, angle: f64, length: f64) -> Pt {
    [p[0] + angle.cos() * length, p[1] - angle.sin() * length]
}

fn line_intersection(s1: Pt, e1: Pt, s2: Pt, e2: Pt) -> Option<Pt> {
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let h = |p: Pt| [p[0], p[1], 1.0];
    let r = cross(cross(h(s1), h(e1)), cross(h(s2), h(e2)));
    if float_zero(r[2]) {
        return None;
    }
    Some([r[0] / r[2], r[1] / r[2]])
}

fn quad_roots(a: f64, b: f64, c: f64) -> Vec<f64> {
    if a == 0.0 {
        return Vec::new();
    }
    let s = b * b - 4.0 * a * c;
    if s < 0.0 {
        return Vec::new();
    }
    let single = -b / (2.0 * a);
    if s == 0.0 {
        return vec![single];
    }
    let delta = s.sqrt() / (2.0 * a);
    vec![single - delta, single + delta]
}

#[derive(Clone, Copy, Debug)]
struct Poly {
    a: Pt,
    b: Pt,
    c: Pt,
    d: Pt,
    points: [Pt; 4],
}

impl Poly {
    fn new(p0: Pt, mut p1: Pt, mut p2: Pt, p3: Pt, linearize: bool) -> Self {
        if linearize && point_equal(p0, p1) {
            p1 = lerp_point(p0, p3, 1.0 / 3.0);
        }
        if linearize && point_equal(p2, p3) {
            p2 = lerp_point(p0, p3, 2.0 / 3.0);
        }
        let coeff = |k: usize| {
            let (p0, p1, p2, p3) = (p0[k], p1[k], p2[k], p3[k]);
            [-p0 + 3.0 * p1 - 3.0 * p2 + p3, 3.0 * p0 - 6.0 * p1 + 3.0 * p2, -3.0 * p0 + 3.0 * p1, p0]
        };
        let (x, y) = (coeff(0), coeff(1));
        Poly { a: [x[0], y[0]], b: [x[1], y[1]], c: [x[2], y[2]], d: [x[3], y[3]], points: [p0, p1, p2, p3] }
    }

    fn single(p: Pt) -> Self {
        Poly::new(p, p, p, p, false)
    }

    /// `PolynomialBezier.shapeSegment`.
    fn segment(p: &BPath, k: usize) -> Self {
        let n = (k + 1) % p.len();
        Poly::new(p.v[k], p.o[k], p.i[n], p.v[n], true)
    }

    /// `PolynomialBezier.shapeSegmentInverted`.
    fn segment_inverted(p: &BPath, k: usize) -> Self {
        let n = (k + 1) % p.len();
        Poly::new(p.v[n], p.i[n], p.o[k], p.v[k], true)
    }

    fn point(&self, t: f64) -> Pt {
        let f = |k: usize| ((self.a[k] * t + self.b[k]) * t + self.c[k]) * t + self.d[k];
        [f(0), f(1)]
    }

    fn derivative(&self, t: f64) -> Pt {
        let f = |k: usize| (3.0 * t * self.a[k] + 2.0 * self.b[k]) * t + self.c[k];
        [f(0), f(1)]
    }

    fn tangent_angle(&self, t: f64) -> f64 {
        let p = self.derivative(t);
        p[1].atan2(p[0])
    }

    fn normal_angle(&self, t: f64) -> f64 {
        let p = self.derivative(t);
        p[0].atan2(p[1])
    }

    fn inflection_points(&self) -> Vec<f64> {
        let denom = self.a[1] * self.b[0] - self.a[0] * self.b[1];
        if float_zero(denom) {
            return Vec::new();
        }
        let tcusp = (-0.5 * (self.a[1] * self.c[0] - self.a[0] * self.c[1])) / denom;
        let square = tcusp * tcusp - ((1.0 / 3.0) * (self.b[1] * self.c[0] - self.b[0] * self.c[1])) / denom;
        if square < 0.0 {
            return Vec::new();
        }
        let root = square.sqrt();
        if float_zero(root) {
            if root > 0.0 && root < 1.0 {
                return vec![tcusp];
            }
            return Vec::new();
        }
        [tcusp - root, tcusp + root].into_iter().filter(|r| *r > 0.0 && *r < 1.0).collect()
    }

    fn split(&self, t: f64) -> (Poly, Poly) {
        let p = &self.points;
        if t <= 0.0 {
            return (Poly::single(p[0]), *self);
        }
        if t >= 1.0 {
            return (*self, Poly::single(p[3]));
        }
        let p10 = lerp_point(p[0], p[1], t);
        let p11 = lerp_point(p[1], p[2], t);
        let p12 = lerp_point(p[2], p[3], t);
        let p20 = lerp_point(p10, p11, t);
        let p21 = lerp_point(p11, p12, t);
        let p3 = lerp_point(p20, p21, t);
        (Poly::new(p[0], p10, p20, p3, true), Poly::new(p3, p21, p12, p[3], true))
    }

    fn extrema(&self, k: usize) -> (f64, f64) {
        let mut min = self.points[0][k];
        let mut max = self.points[3][k];
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        for f in quad_roots(3.0 * self.a[k], 2.0 * self.b[k], self.c[k]) {
            if f > 0.0 && f < 1.0 {
                let val = self.point(f)[k];
                if val < min {
                    min = val;
                } else if val > max {
                    max = val;
                }
            }
        }
        (min, max)
    }

    /// `PolynomialBezier.intersections` (tolerance 2, max recursion 7).
    fn intersections(&self, other: &Poly) -> Vec<[f64; 2]> {
        let mut out = Vec::new();
        intersects_impl(&IData::new(*self, 0.0, 1.0), &IData::new(*other, 0.0, 1.0), 0, 2.0, &mut out, 7);
        out
    }
}

struct IData {
    cx: f64,
    cy: f64,
    width: f64,
    height: f64,
    bez: Poly,
    t: f64,
    t1: f64,
    t2: f64,
}

impl IData {
    fn new(bez: Poly, t1: f64, t2: f64) -> Self {
        let (x0, x1) = bez.extrema(0);
        let (y0, y1) = bez.extrema(1);
        IData { cx: (x1 + x0) / 2.0, cy: (y1 + y0) / 2.0, width: x1 - x0, height: y1 - y0, bez, t: (t1 + t2) / 2.0, t1, t2 }
    }

    fn split(&self) -> (IData, IData) {
        let (a, b) = self.bez.split(0.5);
        (IData::new(a, self.t1, self.t), IData::new(b, self.t, self.t2))
    }
}

fn box_intersect(a: &IData, b: &IData) -> bool {
    (a.cx - b.cx).abs() * 2.0 < a.width + b.width && (a.cy - b.cy).abs() * 2.0 < a.height + b.height
}

fn intersects_impl(d1: &IData, d2: &IData, depth: u32, tol: f64, out: &mut Vec<[f64; 2]>, max_rec: u32) {
    if !box_intersect(d1, d2) {
        return;
    }
    if depth >= max_rec || (d1.width <= tol && d1.height <= tol && d2.width <= tol && d2.height <= tol) {
        out.push([d1.t, d2.t]);
        return;
    }
    let (a0, a1) = d1.split();
    let (b0, b1) = d2.split();
    intersects_impl(&a0, &b0, depth + 1, tol, out, max_rec);
    intersects_impl(&a0, &b1, depth + 1, tol, out, max_rec);
    intersects_impl(&a1, &b0, depth + 1, tol, out, max_rec);
    intersects_impl(&a1, &b1, depth + 1, tol, out, max_rec);
}

// ---------------------------------------------------------------------------
// Zig-zag (ZigZagModifier.js)
// ---------------------------------------------------------------------------

fn zz_set_point(out: &mut BPath, point: Pt, angle: f64, dir: f64, amp: f64, out_amp: f64, in_amp: f64) {
    let ang_o = angle - std::f64::consts::FRAC_PI_2;
    let ang_i = angle + std::f64::consts::FRAC_PI_2;
    let px = point[0] + angle.cos() * dir * amp;
    let py = point[1] - angle.sin() * dir * amp;
    out.push(
        [px, py],
        [px + ang_o.cos() * out_amp, py - ang_o.sin() * out_amp],
        [px + ang_i.cos() * in_amp, py - ang_i.sin() * in_amp],
    );
}

fn zz_projecting_angle(path: &BPath, cur: usize) -> f64 {
    let n = path.len();
    let prev = path.v[if cur == 0 { n - 1 } else { cur - 1 }];
    let next = path.v[(cur + 1) % n];
    // getPerpendicularVector: (next - prev) rotated by -π/2.
    let vec = [next[0] - prev[0], next[1] - prev[1]];
    let rot = -std::f64::consts::FRAC_PI_2;
    let pv = [rot.cos() * vec[0] - rot.sin() * vec[1], rot.sin() * vec[0] + rot.cos() * vec[1]];
    0f64.atan2(1.0) - pv[1].atan2(pv[0])
}

fn zz_corner(out: &mut BPath, path: &BPath, cur: usize, amp: f64, freq: f64, smooth: bool, dir: f64) {
    let n = path.len();
    let angle = zz_projecting_angle(path, cur);
    let point = path.v[cur % n];
    let prev = path.v[if cur == 0 { n - 1 } else { cur - 1 }];
    let next = path.v[(cur + 1) % n];
    let prev_dist = if smooth { point_distance(point, prev) } else { 0.0 };
    let next_dist = if smooth { point_distance(point, next) } else { 0.0 };
    zz_set_point(out, point, angle, dir, amp, next_dist / ((freq + 1.0) * 2.0), prev_dist / ((freq + 1.0) * 2.0));
}

fn zz_segment(out: &mut BPath, seg: &Poly, amp: f64, freq: f64, smooth: bool, mut dir: f64) -> f64 {
    let mut k = 0.0;
    while k < freq {
        let t = (k + 1.0) / (freq + 1.0);
        let dist = if smooth { point_distance(seg.points[3], seg.points[0]) } else { 0.0 };
        let h = dist / ((freq + 1.0) * 2.0);
        zz_set_point(out, seg.point(t), seg.normal_angle(t), dir, amp, h, h);
        dir = -dir;
        k += 1.0;
    }
    dir
}

/// Zig-zag: `size` amplitude, `ridges` per segment, `smooth` = rounded points.
pub fn zig_zag(path: &mut BPath, size: f64, ridges: f64, smooth: bool) {
    if size == 0.0 {
        return;
    }
    let freq = js_round(ridges).max(0.0);
    let mut count = path.len();
    let mut out = BPath::with_capacity(count * (freq as usize + 1), path.closed);
    if !path.closed {
        count = count.saturating_sub(1);
    }
    if count == 0 {
        *path = out;
        return;
    }
    let mut dir = -1.0;
    let mut seg = Poly::segment(path, 0);
    zz_corner(&mut out, path, 0, size, freq, smooth, dir);
    for k in 0..count {
        dir = zz_segment(&mut out, &seg, size, freq, smooth, -dir);
        if !(k == count - 1 && !path.closed) {
            seg = Poly::segment(path, (k + 1) % count);
        }
        zz_corner(&mut out, path, k + 1, size, freq, smooth, dir);
    }
    *path = out;
}

// ---------------------------------------------------------------------------
// Offset path (OffsetPathModifier.js)
// ---------------------------------------------------------------------------

fn linear_offset(p1: Pt, p2: Pt, amount: f64) -> (Pt, Pt) {
    let angle = (p2[0] - p1[0]).atan2(p2[1] - p1[1]);
    (polar_offset(p1, angle, amount), polar_offset(p2, angle, amount))
}

fn offset_segment(seg: &Poly, amount: f64) -> Poly {
    let p = &seg.points;
    let (p0, p1a) = linear_offset(p[0], p[1], amount);
    let (p1b, p2b) = linear_offset(p[1], p[2], amount);
    let (p2a, p3) = linear_offset(p[2], p[3], amount);
    let p1 = line_intersection(p0, p1a, p1b, p2b).unwrap_or(p1a);
    let p2 = line_intersection(p2a, p3, p1b, p2b).unwrap_or(p2a);
    Poly::new(p0, p1, p2, p3, false)
}

/// `joinLines`; returns the new last point.
fn join_lines(out: &mut BPath, seg1: &Poly, seg2: &Poly, join: u8, miter: f64) -> Pt {
    let p0 = seg1.points[3];
    let p1 = seg2.points[0];
    if join == 3 || point_equal(p0, p1) {
        return p0;
    }
    if join == 2 {
        let angle_out = -seg1.tangent_angle(1.0);
        let angle_in = -seg2.tangent_angle(0.0) + std::f64::consts::PI;
        let perp = angle_out + std::f64::consts::FRAC_PI_2;
        let center = line_intersection(p0, polar_offset(p0, perp, 100.0), p1, polar_offset(p1, perp, 100.0));
        let radius = match center {
            Some(c) => point_distance(c, p0),
            None => point_distance(p0, p1) / 2.0,
        };
        let tan = polar_offset(p0, angle_out, 2.0 * radius * KAPPA);
        let last = out.len() - 1;
        out.o[last] = tan;
        let tan = polar_offset(p1, angle_in, 2.0 * radius * KAPPA);
        out.push(p1, p1, tan);
        return p1;
    }
    let t0 = if point_equal(p0, seg1.points[2]) { seg1.points[0] } else { seg1.points[2] };
    let t1 = if point_equal(p1, seg2.points[1]) { seg2.points[3] } else { seg2.points[1] };
    if let Some(x) = line_intersection(t0, p0, p1, t1) {
        if point_distance(x, p0) < miter {
            out.push(x, x, x);
            return x;
        }
    }
    p0
}

fn get_intersection(a: &Poly, b: &Poly) -> Option<[f64; 2]> {
    let mut x = a.intersections(b);
    if !x.is_empty() && float_equal(x[0][0], 1.0) {
        x.remove(0);
    }
    x.first().copied()
}

fn prune_segment_intersection(a: &[Poly], b: &[Poly]) -> (Vec<Poly>, Vec<Poly>) {
    let mut outa = a.to_vec();
    let mut outb = b.to_vec();
    if let Some(x) = get_intersection(&a[a.len() - 1], &b[0]) {
        outa[a.len() - 1] = a[a.len() - 1].split(x[0]).0;
        outb[0] = b[0].split(x[1]).1;
    }
    if a.len() > 1 && b.len() > 1 {
        if let Some(x) = get_intersection(&a[0], &b[b.len() - 1]) {
            return (vec![a[0].split(x[0]).0], vec![b[b.len() - 1].split(x[1]).1]);
        }
    }
    (outa, outb)
}

fn prune_intersections(segs: &mut [Vec<Poly>]) {
    for k in 1..segs.len() {
        let (a, b) = prune_segment_intersection(&segs[k - 1], &segs[k]);
        segs[k - 1] = a;
        segs[k] = b;
    }
    if segs.len() > 1 {
        let last = segs.len() - 1;
        let (a, b) = prune_segment_intersection(&segs[last], &segs[0]);
        segs[last] = a;
        segs[0] = b;
    }
}

fn offset_segment_split(seg: &Poly, amount: f64) -> Vec<Poly> {
    let flex = seg.inflection_points();
    if flex.is_empty() {
        return vec![offset_segment(seg, amount)];
    }
    if flex.len() == 1 || float_equal(flex[1], 1.0) {
        let (l, r) = seg.split(flex[0]);
        return vec![offset_segment(&l, amount), offset_segment(&r, amount)];
    }
    let (l, rest) = seg.split(flex[0]);
    let t = (flex[1] - flex[0]) / (1.0 - flex[0]);
    let (m, r) = rest.split(t);
    vec![offset_segment(&l, amount), offset_segment(&m, amount), offset_segment(&r, amount)]
}

/// Offset path by `amount` with line join `join` (1 miter, 2 round, 3 bevel).
pub fn offset_path(path: &mut BPath, amount: f64, join: u8, miter: f64) {
    if amount == 0.0 || path.is_empty() {
        return;
    }
    let mut out = BPath::with_capacity(path.len() * 2, path.closed);
    let mut count = path.len();
    if !path.closed {
        count -= 1;
    }
    let mut multi: Vec<Vec<Poly>> = Vec::with_capacity(count * 2);
    for k in 0..count {
        multi.push(offset_segment_split(&Poly::segment(path, k), amount));
    }
    if !path.closed {
        for k in (0..count).rev() {
            multi.push(offset_segment_split(&Poly::segment_inverted(path, k), amount));
        }
    }
    prune_intersections(&mut multi);

    let mut last_point: Option<Pt> = None;
    let mut last_seg: Option<Poly> = None;
    for ms in &multi {
        if let Some(ls) = &last_seg {
            last_point = Some(join_lines(&mut out, ls, &ms[0], join, miter));
        }
        last_seg = ms.last().copied();
        for seg in ms {
            let p = &seg.points;
            match last_point {
                Some(lp) if point_equal(p[0], lp) => {
                    let n = out.len() - 1;
                    out.o[n] = p[1];
                }
                _ => out.push(p[0], p[1], p[0]),
            }
            out.push(p[3], p[3], p[2]);
            last_point = Some(p[3]);
        }
    }
    if let (Some(ls), Some(first)) = (&last_seg, multi.first()) {
        join_lines(&mut out, ls, &first[0], join, miter);
    }
    *path = out;
}

// ---------------------------------------------------------------------------
// Twist
// ---------------------------------------------------------------------------

/// Twist by `angle` degrees around `center`.
///
/// No reference player implements Lottie `tw` (lottie-web, lottie-android,
/// lottie-ios, rlottie and Skottie ignore it; ThorVG logs "Twist(tw) is not
/// supported yet"; the lottie-docs schema only names `a` angle and `c`
/// center). This follows After Effects' documented behaviour ("rotates a
/// path more sharply in the center than at the edges"; positive =
/// clockwise): every point turns by `angle · (1 − d / R)` about `center`
/// (local space), `d` its distance from `center` and `R` the farthest
/// vertex. Segments are first split in four (de Casteljau) so straight edges
/// bend.
pub fn twist(path: &mut BPath, angle: f64, center: [f64; 2]) {
    let n = path.len();
    if angle == 0.0 || n == 0 {
        return;
    }
    // Subdivide every segment into 4 pieces.
    let mut sub = BPath::with_capacity(n * 4, path.closed);
    let segs = if path.closed { n } else { n - 1 };
    if segs == 0 {
        return;
    }
    let mut pending_in = path.i[0];
    for k in 0..segs {
        let nk = (k + 1) % n;
        let mut seg = Poly::new(path.v[k], path.o[k], path.i[nk], path.v[nk], false);
        // Split at 1/4, then 1/3 of the rest, then 1/2 of the rest.
        for frac in [0.25, 1.0 / 3.0, 0.5] {
            let (a, b) = seg.split(frac);
            sub.push(a.points[0], a.points[1], pending_in);
            pending_in = a.points[2];
            seg = b;
        }
        sub.push(seg.points[0], seg.points[1], pending_in);
        pending_in = seg.points[2];
    }
    if path.closed {
        sub.i[0] = pending_in;
    } else {
        sub.push(path.v[n - 1], path.o[n - 1], pending_in);
    }
    let r = sub
        .v
        .iter()
        .map(|v| (v[0] - center[0]).hypot(v[1] - center[1]))
        .fold(0.0, f64::max);
    if r <= 0.0 {
        return;
    }
    let rad = angle.to_radians();
    let rotate = |p: Pt| {
        let (dx, dy) = (p[0] - center[0], p[1] - center[1]);
        let d = dx.hypot(dy);
        let a = rad * (1.0 - d / r).max(0.0);
        let (s, c) = a.sin_cos();
        // Clockwise in a y-down space, like `Affine::rotate_deg`.
        [center[0] + dx * c - dy * s, center[1] + dx * s + dy * c]
    };
    for p in sub.v.iter_mut().chain(sub.i.iter_mut()).chain(sub.o.iter_mut()) {
        *p = rotate(*p);
    }
    *path = sub;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{ellipse, rect, Affine};

    fn geo(path: BPath) -> Geo {
        Geo { path, more: Vec::new(), xf: Affine::IDENTITY, merge: None }
    }

    fn square() -> BPath {
        rect([0.0, 0.0], [100.0, 100.0], 0.0, true)
    }

    fn length(p: &BPath) -> f64 {
        segments_length(p).1
    }

    fn bbox(p: &BPath) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for v in &p.v {
            b[0] = b[0].min(v[0]);
            b[1] = b[1].min(v[1]);
            b[2] = b[2].max(v[0]);
            b[3] = b[3].max(v[1]);
        }
        b
    }

    #[test]
    fn trim_half_square() {
        let mut g = [geo(square())];
        trim(&mut g, 0.0, 50.0, 0.0, false);
        assert!(!g[0].path.closed);
        assert!((length(&g[0].path) - 200.0).abs() < 0.5, "{}", length(&g[0].path));
    }

    #[test]
    fn trim_full_and_empty() {
        let mut g = [geo(square())];
        trim(&mut g, 0.0, 100.0, 0.0, false);
        assert_eq!(g[0].path, square());
        trim(&mut g, 30.0, 30.0, 0.0, false);
        assert!(g[0].path.is_empty());
    }

    #[test]
    fn trim_wraps_closed_seam_into_one_path() {
        let mut g = [geo(square())];
        // 80%..120% after a 72° (20%) offset on 60..100.
        trim(&mut g, 60.0, 100.0, 72.0, false);
        let p = &g[0].path;
        assert!(!p.closed);
        assert!((length(p) - 160.0).abs() < 0.5, "{}", length(p));
        // Starts at 80% of the perimeter, ends at 20%.
        assert!(point_distance(p.v[0], [-30.0, -50.0]) < 0.01, "{:?}", p.v[0]);
        assert!(point_distance(*p.v.last().unwrap(), [50.0, 30.0]) < 0.01, "{:?}", p.v);
    }

    #[test]
    fn trim_individually_splits_the_range() {
        let mut g = [geo(square()), geo(square())];
        trim(&mut g, 0.0, 50.0, 0.0, true);
        assert!((length(&g[0].path) - 400.0).abs() < 0.5);
        assert!(g[0].path.closed);
        assert!(length(&g[1].path) < 0.5);
        let mut g = [geo(square()), geo(square())];
        trim(&mut g, 0.0, 50.0, 0.0, false);
        assert!((length(&g[1].path) - 200.0).abs() < 0.5);
    }

    fn line(x0: f64, x1: f64) -> BPath {
        let mut p = BPath::default();
        p.push([x0, 0.0], [x0, 0.0], [x0, 0.0]);
        p.push([x1, 0.0], [x1, 0.0], [x1, 0.0]);
        p
    }

    #[test]
    fn trim_wrapped_open_path_gives_two_pieces() {
        let mut g = [geo(line(0.0, 100.0))];
        // 70%..110% after a 36° (10%) offset on 60..100.
        trim(&mut g, 60.0, 100.0, 36.0, false);
        let (a, b) = (&g[0].path, &g[0].more);
        assert_eq!(b.len(), 1);
        assert!(!a.closed && !b[0].closed);
        assert!((a.v[0][0] - 70.0).abs() < 0.01 && (a.v.last().unwrap()[0] - 100.0).abs() < 0.01, "{a:?}");
        assert!(b[0].v[0][0].abs() < 0.01 && (b[0].v.last().unwrap()[0] - 10.0).abs() < 0.01, "{b:?}");
    }

    #[test]
    fn trim_treats_more_as_the_same_shape() {
        let mut gg = geo(line(0.0, 100.0));
        gg.more.push(line(200.0, 300.0));
        let mut g = [gg];
        // 25%..75% of 200 total = 50..100 on the first, 0..50 on the second.
        trim(&mut g, 25.0, 75.0, 0.0, false);
        let (a, b) = (&g[0].path, &g[0].more);
        assert_eq!(b.len(), 1);
        assert!((a.v[0][0] - 50.0).abs() < 0.01 && (a.v.last().unwrap()[0] - 100.0).abs() < 0.01, "{a:?}");
        assert!((b[0].v[0][0] - 200.0).abs() < 0.01 && (b[0].v.last().unwrap()[0] - 250.0).abs() < 0.01, "{b:?}");
        // Trimmed to nothing clears `more` too.
        trim(&mut g, 40.0, 40.0, 0.0, false);
        assert!(g[0].path.is_empty() && g[0].more.is_empty());
    }

    #[test]
    fn round_corners_square_has_eight_vertices() {
        let mut p = square();
        round_corners(&mut p, 10.0);
        assert_eq!(p.len(), 8);
        assert!(p.closed);
        let l = length(&p);
        // 4 × 80 straight + 4 quarter arcs of r = 10.
        assert!((l - (320.0 + 2.0 * std::f64::consts::PI * 10.0)).abs() < 1.0, "{l}");
    }

    #[test]
    fn round_corners_skips_smooth_vertices() {
        let mut p = ellipse([0.0, 0.0], [50.0, 50.0], true);
        let before = p.clone();
        round_corners(&mut p, 10.0);
        assert_eq!(p, before);
    }

    #[test]
    fn pucker_bloat_zero_is_identity() {
        let mut p = square();
        pucker_bloat(&mut p, 0.0);
        assert_eq!(p, square());
        pucker_bloat(&mut p, 50.0);
        assert_eq!(p.v[0], [25.0, -25.0]);
        assert_eq!(p.o[0], [75.0, -75.0]);
    }

    #[test]
    fn zig_zag_vertex_count() {
        let mut p = square();
        zig_zag(&mut p, 5.0, 3.0, false);
        // 4 segments × 3 ridges + 5 corners (the first corner repeats).
        assert_eq!(p.len(), 4 * 3 + 5);
        let mut open = BPath::default();
        open.push([0.0, 0.0], [0.0, 0.0], [0.0, 0.0]);
        open.push([100.0, 0.0], [100.0, 0.0], [100.0, 0.0]);
        zig_zag(&mut open, 5.0, 4.0, true);
        assert_eq!(open.len(), 4 + 2);
        // Ridges alternate sides of the line.
        assert!(open.v[1][1] * open.v[2][1] < 0.0);
    }

    #[test]
    fn offset_square_grows_bbox() {
        let mut p = square();
        offset_path(&mut p, 10.0, 1, 100.0);
        let b = bbox(&p);
        for (got, want) in b.iter().zip([-60.0, -60.0, 60.0, 60.0]) {
            assert!((got - want).abs() < 1e-6, "{b:?}");
        }
        let mut p = square();
        offset_path(&mut p, -10.0, 1, 100.0);
        let b = bbox(&p);
        assert!((b[2] - 40.0).abs() < 1e-6, "{b:?}");
    }

    #[test]
    fn offset_round_join_adds_arcs() {
        let mut p = square();
        offset_path(&mut p, 10.0, 2, 4.0);
        assert!(p.closed);
        // Start + 4 edge ends + 4 round-join ends (the last repeats the start).
        assert_eq!(p.len(), 9);
        // lottie-web's join radius is half the chord (its perpendiculars are
        // parallel), so the arcs bulge past a true r = 10 quarter circle.
        let l = length(&p);
        assert!(l > 400.0 + 2.0 * std::f64::consts::PI * 10.0 && l < 470.0, "{l}");
    }

    #[test]
    fn twist_keeps_edges_and_turns_inside() {
        let mut p = square();
        twist(&mut p, 0.0, [0.0, 0.0]);
        assert_eq!(p, square());
        twist(&mut p, 90.0, [0.0, 0.0]);
        assert_eq!(p.len(), 16);
        // Corners (farthest) stay; edge midpoints rotate.
        assert!(point_distance(p.v[0], square().v[0]) < 1e-9);
        assert!(point_distance(p.v[2], [50.0, 0.0]) > 1.0);
        assert!(p.v.iter().all(|v| v[0].hypot(v[1]) <= 50.0 * 2f64.sqrt() + 1e-9));
    }
}
