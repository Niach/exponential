//! Animated properties: keyframes with cubic-Bézier easing (per dimension),
//! holds, legacy `s`/`e` pairs and spatial (motion-path) tangents, evaluated
//! the way lottie-web does.
//!
//! Every value is a flat `Vec<f64>`: scalars have one component, vectors and
//! colours several, shapes `[closed, n, (vx, vy, ix, iy, ox, oy) × n]`,
//! gradients the raw `g.k` array.

/// One keyframe.
#[derive(Clone, Debug, Default)]
pub struct Keyframe {
    pub t: f64,
    pub s: Vec<f64>,
    /// Legacy explicit end value.
    pub e: Option<Vec<f64>>,
    /// Out easing (this keyframe → the next): per-dimension x and y.
    pub o: (Vec<f64>, Vec<f64>),
    /// In easing of the next keyframe, stored on this one (Lottie's `i`).
    pub i: (Vec<f64>, Vec<f64>),
    pub hold: bool,
    /// Spatial out tangent (relative to `s`).
    pub to: Option<[f64; 2]>,
    /// Spatial in tangent (relative to the end value).
    pub ti: Option<[f64; 2]>,
}

/// A fixed or keyframed value.
#[derive(Clone, Debug, Default)]
pub struct Prop {
    pub value: Vec<f64>,
    pub keyframes: Vec<Keyframe>,
}

impl Prop {
    pub fn fixed(value: Vec<f64>) -> Self {
        Self {
            value,
            keyframes: Vec::new(),
        }
    }

    pub fn scalar(v: f64) -> Self {
        Self::fixed(vec![v])
    }

    pub fn is_animated(&self) -> bool {
        !self.keyframes.is_empty()
    }

    /// The value at `frame` into `out`.
    pub fn eval_into(&self, frame: f64, out: &mut Vec<f64>) {
        out.clear();
        let kfs = &self.keyframes;
        if kfs.is_empty() {
            out.extend_from_slice(&self.value);
            return;
        }
        if frame <= kfs[0].t || kfs.len() == 1 {
            out.extend_from_slice(&kfs[0].s);
            return;
        }
        let last = kfs.len() - 1;
        if frame >= kfs[last].t {
            // The last keyframe's start value; legacy files end with a bare
            // `{t}` and the previous keyframe's `e` is the final value.
            let k = &kfs[last];
            if !k.s.is_empty() {
                out.extend_from_slice(&k.s);
            } else if let Some(e) = &kfs[last - 1].e {
                out.extend_from_slice(e);
            } else {
                out.extend_from_slice(&kfs[last - 1].s);
            }
            return;
        }
        // Segment: the last keyframe with t <= frame.
        let ix = match kfs.binary_search_by(|k| k.t.partial_cmp(&frame).unwrap_or(std::cmp::Ordering::Less)) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
        .min(last - 1);
        let a = &kfs[ix];
        let b = &kfs[ix + 1];
        let end: &[f64] = match &a.e {
            Some(e) => e,
            None if !b.s.is_empty() => &b.s,
            None => &a.s,
        };
        if a.hold || b.t <= a.t {
            out.extend_from_slice(&a.s);
            return;
        }
        let x = (frame - a.t) / (b.t - a.t);
        let spatial = match (a.to, a.ti) {
            (Some(to), Some(ti)) if a.s.len() >= 2 && end.len() >= 2 => {
                if to[0] == 0.0 && to[1] == 0.0 && ti[0] == 0.0 && ti[1] == 0.0 {
                    None
                } else {
                    Some((to, ti))
                }
            }
            _ => None,
        };
        if let Some((to, ti)) = spatial {
            let p = ease(&a.o, &a.i, 0, x);
            let p0 = (a.s[0], a.s[1]);
            let p3 = (end[0], end[1]);
            let p1 = (p0.0 + to[0], p0.1 + to[1]);
            let p2 = (p3.0 + ti[0], p3.1 + ti[1]);
            let (px, py) = along_cubic(p0, p1, p2, p3, p);
            out.push(px);
            out.push(py);
            for d in 2..a.s.len().min(end.len()) {
                out.push(a.s[d] + (end[d] - a.s[d]) * p);
            }
            return;
        }
        let n = a.s.len().min(end.len());
        let per_dim = a.o.0.len() > 1 || a.i.0.len() > 1;
        let p0 = ease(&a.o, &a.i, 0, x);
        for d in 0..n {
            let p = if per_dim { ease(&a.o, &a.i, d, x) } else { p0 };
            out.push(a.s[d] + (end[d] - a.s[d]) * p);
        }
        // Shapes keep their own header (closed, count) from the start value.
        if a.s.len() > n {
            out.extend_from_slice(&a.s[n..]);
        }
    }

    pub fn eval(&self, frame: f64) -> Vec<f64> {
        let mut v = Vec::new();
        self.eval_into(frame, &mut v);
        v
    }

    /// Component 0 (0 when empty).
    pub fn scalar_at(&self, frame: f64) -> f64 {
        if self.keyframes.is_empty() {
            return self.value.first().copied().unwrap_or(0.0);
        }
        let mut v = Vec::with_capacity(4);
        self.eval_into(frame, &mut v);
        v.first().copied().unwrap_or(0.0)
    }

    /// Components 0 and 1.
    pub fn vec2_at(&self, frame: f64) -> [f64; 2] {
        let mut v = Vec::with_capacity(4);
        self.eval_into(frame, &mut v);
        [
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
        ]
    }
}

fn pick(v: &[f64], d: usize, default: f64) -> f64 {
    v.get(d).or_else(|| v.first()).copied().unwrap_or(default)
}

/// The eased progress of dimension `d` at linear progress `x`.
pub fn ease(o: &(Vec<f64>, Vec<f64>), i: &(Vec<f64>, Vec<f64>), d: usize, x: f64) -> f64 {
    let (x1, y1) = (pick(&o.0, d, 0.0), pick(&o.1, d, 0.0));
    let (x2, y2) = (pick(&i.0, d, 1.0), pick(&i.1, d, 1.0));
    cubic_ease(x1, y1, x2, y2, x)
}

/// CSS-style `cubic-bezier(x1, y1, x2, y2)` at `x`.
pub fn cubic_ease(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    if x1 == y1 && x2 == y2 {
        return x;
    }
    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);
    let bx = |t: f64| ((1.0 - 3.0 * x2 + 3.0 * x1) * t + (3.0 * x2 - 6.0 * x1)) * t * t + 3.0 * x1 * t;
    let by = |t: f64| ((1.0 - 3.0 * y2 + 3.0 * y1) * t + (3.0 * y2 - 6.0 * y1)) * t * t + 3.0 * y1 * t;
    let dx = |t: f64| (3.0 * (1.0 - 3.0 * x2 + 3.0 * x1) * t + 2.0 * (3.0 * x2 - 6.0 * x1)) * t + 3.0 * x1;
    // Newton, then bisection.
    let mut t = x;
    for _ in 0..8 {
        let err = bx(t) - x;
        if err.abs() < 1e-7 {
            return by(t);
        }
        let d = dx(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    t = x;
    for _ in 0..40 {
        let v = bx(t);
        if (v - x).abs() < 1e-7 {
            break;
        }
        if v < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    by(t)
}

/// The point at arc-length fraction `p` of a cubic (lottie-web samples the
/// curve; 64 segments match it within a fraction of a pixel).
fn along_cubic(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p: f64) -> (f64, f64) {
    const N: usize = 64;
    let at = |t: f64| {
        let mt = 1.0 - t;
        let a = mt * mt * mt;
        let b = 3.0 * mt * mt * t;
        let c = 3.0 * mt * t * t;
        let d = t * t * t;
        (
            a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
        )
    };
    if p <= 0.0 {
        return p0;
    }
    if p >= 1.0 {
        return p3;
    }
    let mut lens = [0.0f64; N + 1];
    let mut prev = p0;
    for k in 1..=N {
        let q = at(k as f64 / N as f64);
        lens[k] = lens[k - 1] + ((q.0 - prev.0).powi(2) + (q.1 - prev.1).powi(2)).sqrt();
        prev = q;
    }
    let total = lens[N];
    if total <= 0.0 {
        return p0;
    }
    let target = p * total;
    let k = lens.partition_point(|l| *l < target).clamp(1, N);
    let seg = lens[k] - lens[k - 1];
    let f = if seg > 0.0 { (target - lens[k - 1]) / seg } else { 0.0 };
    at((k as f64 - 1.0 + f) / N as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_hold() {
        let p = Prop {
            value: vec![],
            keyframes: vec![
                Keyframe {
                    t: 0.0,
                    s: vec![0.0],
                    o: (vec![0.0], vec![0.0]),
                    i: (vec![1.0], vec![1.0]),
                    ..Default::default()
                },
                Keyframe {
                    t: 10.0,
                    s: vec![100.0],
                    hold: true,
                    ..Default::default()
                },
                Keyframe {
                    t: 20.0,
                    s: vec![0.0],
                    ..Default::default()
                },
            ],
        };
        assert_eq!(p.scalar_at(-1.0), 0.0);
        assert!((p.scalar_at(5.0) - 50.0).abs() < 1e-6);
        assert_eq!(p.scalar_at(15.0), 100.0);
        assert_eq!(p.scalar_at(25.0), 0.0);
    }

    #[test]
    fn ease_in_out_is_symmetric() {
        let v = cubic_ease(0.42, 0.0, 0.58, 1.0, 0.5);
        assert!((v - 0.5).abs() < 1e-6);
        assert!(cubic_ease(0.42, 0.0, 0.58, 1.0, 0.25) < 0.25);
    }
}
