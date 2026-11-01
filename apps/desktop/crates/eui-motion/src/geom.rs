//! Geometry: lottie-web's ShapePath (closed flag + vertices with ABSOLUTE
//! in/out tangents), the primitive builders (rect, ellipse, star, polygon)
//! ported from lottie-web's `ShapeProperty.js` (MIT), 2D affines, and the
//! conversion into display-list verbs.

use crate::display::{Transform as DlTransform, Verb};

pub type Pt = [f64; 2];

/// lottie-web's `roundCorner` / ellipse kappa.
pub const KAPPA: f64 = 0.5519;

/// One subpath: `v[k]` vertices, `i[k]` / `o[k]` absolute in / out tangents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BPath {
    pub closed: bool,
    pub v: Vec<Pt>,
    pub i: Vec<Pt>,
    pub o: Vec<Pt>,
}

impl BPath {
    pub fn with_capacity(n: usize, closed: bool) -> Self {
        Self {
            closed,
            v: Vec::with_capacity(n),
            i: Vec::with_capacity(n),
            o: Vec::with_capacity(n),
        }
    }

    pub fn len(&self) -> usize {
        self.v.len()
    }

    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }

    /// lottie-web `setTripleAt(vX, vY, oX, oY, iX, iY)` appended.
    pub fn push(&mut self, v: Pt, o: Pt, i: Pt) {
        self.v.push(v);
        self.o.push(o);
        self.i.push(i);
    }

    /// From the flat animated value `[closed, n, (vx, vy, ix, iy, ox, oy) × n]`
    /// (Lottie JSON tangents are RELATIVE).
    pub fn from_flat(flat: &[f64]) -> Self {
        if flat.len() < 2 {
            return Self::default();
        }
        let n = (flat[1] as usize).min((flat.len() - 2) / 6);
        let mut p = Self::with_capacity(n, flat[0] != 0.0);
        for k in 0..n {
            let b = 2 + k * 6;
            let v = [flat[b], flat[b + 1]];
            p.v.push(v);
            p.i.push([v[0] + flat[b + 2], v[1] + flat[b + 3]]);
            p.o.push([v[0] + flat[b + 4], v[1] + flat[b + 5]]);
        }
        p
    }

    pub fn transform(&mut self, m: &Affine) {
        for p in self.v.iter_mut().chain(self.i.iter_mut()).chain(self.o.iter_mut()) {
            *p = m.apply(*p);
        }
    }

    /// Reverse direction (in and out tangents swap).
    pub fn reverse(&mut self) {
        if self.v.is_empty() {
            return;
        }
        // lottie-web `reversePath`: the first vertex stays first.
        let n = self.v.len();
        let mut r = Self::with_capacity(n, self.closed);
        r.push(self.v[0], self.i[0], self.o[0]);
        for k in (1..n).rev() {
            r.push(self.v[k], self.i[k], self.o[k]);
        }
        *self = r;
    }

    /// Append as display-list verbs under `m`.
    pub fn to_verbs(&self, m: &Affine, out: &mut Vec<Verb>) {
        let n = self.v.len();
        if n == 0 {
            return;
        }
        let f = |p: Pt| {
            let q = m.apply(p);
            (q[0] as f32, q[1] as f32)
        };
        let (x, y) = f(self.v[0]);
        out.push(Verb::M { x, y });
        let seg = |a: usize, b: usize, out: &mut Vec<Verb>| {
            let (x1, y1) = f(self.o[a]);
            let (x2, y2) = f(self.i[b]);
            let (x, y) = f(self.v[b]);
            if self.o[a] == self.v[a] && self.i[b] == self.v[b] {
                out.push(Verb::L { x, y });
            } else {
                out.push(Verb::C { x1, y1, x2, y2, x, y });
            }
        };
        for k in 1..n {
            seg(k - 1, k, out);
        }
        if self.closed {
            seg(n - 1, 0, out);
            out.push(Verb::Z);
        }
    }
}

/// lottie-web `convertRectToPath`; `cw` = `d` is 1 or 2.
pub fn rect(p: Pt, s: Pt, r: f64, cw: bool) -> BPath {
    let (p0, p1) = (p[0], p[1]);
    let (v0, v1) = (s[0] / 2.0, s[1] / 2.0);
    let round = v0.min(v1).min(r);
    let c = round * (1.0 - KAPPA);
    let mut b = BPath::with_capacity(8, true);
    // setTripleAt(vX, vY, oX, oY, iX, iY)
    let mut t = |vx: f64, vy: f64, ox: f64, oy: f64, ix: f64, iy: f64| b.push([vx, vy], [ox, oy], [ix, iy]);
    if cw {
        t(p0 + v0, p1 - v1 + round, p0 + v0, p1 - v1 + round, p0 + v0, p1 - v1 + c);
        t(p0 + v0, p1 + v1 - round, p0 + v0, p1 + v1 - c, p0 + v0, p1 + v1 - round);
        if round != 0.0 {
            t(p0 + v0 - round, p1 + v1, p0 + v0 - round, p1 + v1, p0 + v0 - c, p1 + v1);
            t(p0 - v0 + round, p1 + v1, p0 - v0 + c, p1 + v1, p0 - v0 + round, p1 + v1);
            t(p0 - v0, p1 + v1 - round, p0 - v0, p1 + v1 - round, p0 - v0, p1 + v1 - c);
            t(p0 - v0, p1 - v1 + round, p0 - v0, p1 - v1 + c, p0 - v0, p1 - v1 + round);
            t(p0 - v0 + round, p1 - v1, p0 - v0 + round, p1 - v1, p0 - v0 + c, p1 - v1);
            t(p0 + v0 - round, p1 - v1, p0 + v0 - c, p1 - v1, p0 + v0 - round, p1 - v1);
        } else {
            t(p0 - v0, p1 + v1, p0 - v0 + c, p1 + v1, p0 - v0, p1 + v1);
            t(p0 - v0, p1 - v1, p0 - v0, p1 - v1 + c, p0 - v0, p1 - v1);
        }
    } else {
        t(p0 + v0, p1 - v1 + round, p0 + v0, p1 - v1 + c, p0 + v0, p1 - v1 + round);
        if round != 0.0 {
            t(p0 + v0 - round, p1 - v1, p0 + v0 - round, p1 - v1, p0 + v0 - c, p1 - v1);
            t(p0 - v0 + round, p1 - v1, p0 - v0 + c, p1 - v1, p0 - v0 + round, p1 - v1);
            t(p0 - v0, p1 - v1 + round, p0 - v0, p1 - v1 + round, p0 - v0, p1 - v1 + c);
            t(p0 - v0, p1 + v1 - round, p0 - v0, p1 + v1 - c, p0 - v0, p1 + v1 - round);
            t(p0 - v0 + round, p1 + v1, p0 - v0 + round, p1 + v1, p0 - v0 + c, p1 + v1);
            t(p0 + v0 - round, p1 + v1, p0 + v0 - c, p1 + v1, p0 + v0 - round, p1 + v1);
            t(p0 + v0, p1 + v1 - round, p0 + v0, p1 + v1 - round, p0 + v0, p1 + v1 - c);
        } else {
            t(p0 - v0, p1 - v1, p0 - v0 + c, p1 - v1, p0 - v0, p1 - v1);
            t(p0 - v0, p1 + v1, p0 - v0, p1 + v1 - c, p0 - v0, p1 + v1);
            t(p0 + v0, p1 + v1, p0 + v0 - c, p1 + v1, p0 + v0, p1 + v1);
        }
    }
    b
}

/// lottie-web `convertEllToPath`.
pub fn ellipse(p: Pt, s: Pt, cw: bool) -> BPath {
    let (p0, p1) = (p[0], p[1]);
    let (s0, s1) = (s[0] / 2.0, s[1] / 2.0);
    let k = KAPPA;
    let sx = if cw { 1.0 } else { -1.0 };
    let mut b = BPath::with_capacity(4, true);
    b.v = vec![[p0, p1 - s1], [p0 + sx * s0, p1], [p0, p1 + s1], [p0 - sx * s0, p1]];
    b.i = vec![
        [p0 - sx * s0 * k, p1 - s1],
        [p0 + sx * s0, p1 - s1 * k],
        [p0 + sx * s0 * k, p1 + s1],
        [p0 - sx * s0, p1 + s1 * k],
    ];
    b.o = vec![
        [p0 + sx * s0 * k, p1 - s1],
        [p0 + sx * s0, p1 + s1 * k],
        [p0 - sx * s0 * k, p1 + s1],
        [p0 - sx * s0, p1 - s1 * k],
    ];
    b
}

/// lottie-web `convertStarToPath` / `convertPolygonToPath`. Angles in degrees,
/// roundness in percent.
#[allow(clippy::too_many_arguments)]
pub fn star(
    polygon: bool,
    p: Pt,
    points: f64,
    outer_r: f64,
    outer_round: f64,
    inner_r: f64,
    inner_round: f64,
    rotation_deg: f64,
    reversed: bool,
) -> BPath {
    let dir = if reversed { -1.0 } else { 1.0 };
    let pi = std::f64::consts::PI;
    let mut cur = -pi / 2.0 + rotation_deg.to_radians();
    let mut b;
    let emit = |b: &mut BPath, rad: f64, roundness: f64, perim: f64, ang: f64| {
        let x = rad * ang.cos();
        let y = rad * ang.sin();
        let len = (x * x + y * y).sqrt();
        let (ox, oy) = if len == 0.0 { (0.0, 0.0) } else { (y / len, -x / len) };
        let (x, y) = (x + p[0], y + p[1]);
        let d = perim * roundness * dir;
        b.push([x, y], [x - ox * d, y - oy * d], [x + ox * d, y + oy * d]);
    };
    if polygon {
        let n = points.floor().max(0.0) as usize;
        if n == 0 {
            return BPath::default();
        }
        let angle = 2.0 * pi / n as f64;
        let perim = 2.0 * pi * outer_r / (n as f64 * 4.0);
        b = BPath::with_capacity(n, true);
        for _ in 0..n {
            emit(&mut b, outer_r, outer_round / 100.0, perim, cur);
            cur += angle * dir;
        }
    } else {
        let n = (points.floor().max(0.0) as usize) * 2;
        if n == 0 {
            return BPath::default();
        }
        let angle = 2.0 * pi / n as f64;
        let long_perim = 2.0 * pi * outer_r / (n as f64 * 2.0);
        let short_perim = 2.0 * pi * inner_r / (n as f64 * 2.0);
        b = BPath::with_capacity(n, true);
        let mut long = true;
        for _ in 0..n {
            let (rad, round, perim) = if long {
                (outer_r, outer_round / 100.0, long_perim)
            } else {
                (inner_r, inner_round / 100.0, short_perim)
            };
            emit(&mut b, rad, round, perim, cur);
            long = !long;
            cur += angle * dir;
        }
    }
    b
}

/// A 2D affine `[a, b, c, d, e, f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(x: f64, y: f64) -> Self {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn scale(x: f64, y: f64) -> Self {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// Clockwise in a y-down space, degrees.
    pub fn rotate_deg(deg: f64) -> Self {
        let (s, c) = deg.to_radians().sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    /// `self · other` (apply `other` first).
    pub fn then(&self, other: &Affine) -> Affine {
        // self ∘ other
        let a = self.0;
        let b = other.0;
        Affine([
            a[0] * b[0] + a[2] * b[1],
            a[1] * b[0] + a[3] * b[1],
            a[0] * b[2] + a[2] * b[3],
            a[1] * b[2] + a[3] * b[3],
            a[0] * b[4] + a[2] * b[5] + a[4],
            a[1] * b[4] + a[3] * b[5] + a[5],
        ])
    }

    pub fn apply(&self, p: Pt) -> Pt {
        let a = self.0;
        [a[0] * p[0] + a[2] * p[1] + a[4], a[1] * p[0] + a[3] * p[1] + a[5]]
    }

    pub fn determinant(&self) -> f64 {
        self.0[0] * self.0[3] - self.0[1] * self.0[2]
    }

    pub fn invert(&self) -> Option<Affine> {
        let det = self.determinant();
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let a = self.0;
        let inv = 1.0 / det;
        Some(Affine([
            a[3] * inv,
            -a[1] * inv,
            -a[2] * inv,
            a[0] * inv,
            (a[2] * a[5] - a[3] * a[4]) * inv,
            (a[1] * a[4] - a[0] * a[5]) * inv,
        ]))
    }

    pub fn is_finite(&self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    pub fn to_dl(&self) -> DlTransform {
        self.0.map(|v| v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_composes_left_to_right() {
        let m = Affine::translate(10.0, 0.0).then(&Affine::scale(2.0, 2.0));
        assert_eq!(m.apply([1.0, 1.0]), [12.0, 2.0]);
        let inv = m.invert().unwrap();
        let p = inv.apply(m.apply([3.0, 4.0]));
        assert!((p[0] - 3.0).abs() < 1e-9 && (p[1] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn ellipse_starts_at_top() {
        let e = ellipse([0.0, 0.0], [10.0, 20.0], true);
        assert_eq!(e.v[0], [0.0, -10.0]);
        assert_eq!(e.v[1], [5.0, 0.0]);
    }
}
