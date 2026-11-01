//! The eUI Motion authoring dialect (EXP-1603): readable JSON an LLM writes
//! with minimal guidance, compiling 1:1 to Lottie.
//!
//! - times in milliseconds (`"time": "frames"` switches to frames);
//! - SVG path data (`"d"`) for paths and masks;
//! - CSS easing names (`linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`,
//!   `hold`) or `[x1, y1, x2, y2]`;
//! - colours: `"#rrggbb[aa]"`, `[r, g, b, a]` (0–1), `"$color.<token>"`
//!   (a Lottie slot the host fills from the theme) or `{"light", "dark"}`;
//! - any value animates as `{"keys": [{"t", "v", "ease"}, …]}`;
//! - `"lottie": {…}` on the root, a layer or a shape passes raw Lottie keys
//!   through (what the friendly fields do not cover: text animators,
//!   effects, 3D).
//!
//! `decompile` converts Lottie back. The pair is stable: for a canonical
//! dialect document `decompile(compile(d)) == d`, and `compile(decompile(l))`
//! renders exactly like `l`.

use serde_json::{json, Map, Value};

use crate::validate::Issue;

#[cfg(feature = "decompile")]
pub use crate::decompile::decompile;

pub(crate) type Obj = Map<String, Value>;

/// The generator tag compiled files carry in `meta.g`.
pub const GENERATOR: &str = "eUI Motion";

fn issue(code: &str, path: &str, message: impl Into<String>) -> Issue {
    Issue { code: code.into(), path: path.into(), message: message.into() }
}

// --- numbers -------------------------------------------------------------

/// A JSON number, integral values as integers (canonical output).
pub(crate) fn n(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

pub(crate) fn round_to(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    let r = (v * f).round() / f;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

pub(crate) fn f(v: &Value) -> Option<f64> {
    v.as_f64()
}

// --- colours -------------------------------------------------------------

pub(crate) const EASES: [(&str, [f64; 4]); 5] = [
    ("linear", [0.0, 0.0, 1.0, 1.0]),
    ("ease", [0.25, 0.1, 0.25, 1.0]),
    ("ease-in", [0.42, 0.0, 1.0, 1.0]),
    ("ease-out", [0.0, 0.0, 0.58, 1.0]),
    ("ease-in-out", [0.42, 0.0, 0.58, 1.0]),
];

/// `#rgb`, `#rrggbb`, `#rrggbbaa` → straight RGBA 0–1.
pub fn parse_hex(s: &str) -> Option<[f64; 4]> {
    let h = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|b| b as f64 / 255.0);
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 1].repeat(2), 16).ok().map(|b| b as f64 / 255.0);
            Some([d(0)?, d(1)?, d(2)?, 1.0])
        }
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 1.0]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

/// RGBA 0–1 → `#rrggbb[aa]` when every channel is an exact byte, else `None`.
pub fn format_hex(c: &[f64]) -> Option<String> {
    let mut out = String::from("#");
    for (k, v) in c.iter().take(4).enumerate() {
        let b = v * 255.0;
        if (b - b.round()).abs() > 1e-6 || !(0.0..=255.0).contains(&b.round()) {
            return None;
        }
        if k == 3 && b.round() == 255.0 {
            continue;
        }
        out.push_str(&format!("{:02x}", b.round() as u8));
    }
    if c.len() < 3 {
        return None;
    }
    Some(out)
}

/// A colour value → (Lottie `k` array, slot id).
fn compile_color(v: &Value, path: &str, issues: &mut Vec<Issue>) -> Option<(Vec<f64>, Option<String>)> {
    match v {
        Value::String(s) if s.starts_with("$color.") => Some((vec![0.5, 0.5, 0.5, 1.0], Some(s.clone()))),
        Value::String(s) => match parse_hex(s) {
            Some(c) => Some((c.to_vec(), None)),
            None => {
                issues.push(issue("color", path, format!("`{s}` is not a colour (use #rrggbb, $color.<token> or {{light, dark}})")));
                None
            }
        },
        Value::Array(a) => {
            let c: Vec<f64> = a.iter().filter_map(f).collect();
            if c.len() != 3 && c.len() != 4 {
                issues.push(issue("color", path, "a colour array needs [r, g, b] or [r, g, b, a] in 0–1"));
                return None;
            }
            Some((c, None))
        }
        Value::Object(o) if o.contains_key("light") && o.contains_key("dark") => {
            let light = compile_color(&o["light"], &format!("{path}/light"), issues)?;
            let dark = compile_color(&o["dark"], &format!("{path}/dark"), issues)?;
            let (l, d) = (format_hex(&light.0)?, format_hex(&dark.0)?);
            Some((light.0, Some(format!("ld:{l}:{d}"))))
        }
        _ => {
            issues.push(issue("color", path, "expected a colour"));
            None
        }
    }
}

#[cfg_attr(not(feature = "decompile"), allow(dead_code))]
pub(crate) fn decompile_color(k: &[f64], sid: Option<&str>) -> Value {
    if let Some(sid) = sid {
        if sid.starts_with("$color.") {
            return json!(sid);
        }
        if let Some(rest) = sid.strip_prefix("ld:") {
            if let Some((l, d)) = rest.split_once(':') {
                return json!({"light": l, "dark": d});
            }
        }
    }
    match format_hex(k) {
        Some(h) => json!(h),
        None => Value::Array(k.iter().map(|v| n(*v)).collect()),
    }
}

// --- SVG path data -------------------------------------------------------

/// One Lottie bezier: `{c, v, i, o}` with relative tangents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bez {
    pub closed: bool,
    pub v: Vec<[f64; 2]>,
    pub i: Vec<[f64; 2]>,
    pub o: Vec<[f64; 2]>,
}

impl Bez {
    fn to_json(&self) -> Value {
        let pts = |p: &[[f64; 2]]| Value::Array(p.iter().map(|q| json!([n(q[0]), n(q[1])])).collect());
        json!({"c": self.closed, "v": pts(&self.v), "i": pts(&self.i), "o": pts(&self.o)})
    }

    #[cfg_attr(not(feature = "decompile"), allow(dead_code))]
    pub(crate) fn from_json(v: &Value) -> Option<Bez> {
        let v = match v {
            Value::Array(a) => a.first()?,
            o => o,
        };
        let pts = |k: &str| -> Vec<[f64; 2]> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|p| [p.get(0).and_then(f).unwrap_or(0.0), p.get(1).and_then(f).unwrap_or(0.0)]).collect())
                .unwrap_or_default()
        };
        let vs = pts("v");
        let mut is = pts("i");
        let mut os = pts("o");
        is.resize(vs.len(), [0.0, 0.0]);
        os.resize(vs.len(), [0.0, 0.0]);
        Some(Bez { closed: v.get("c").and_then(Value::as_bool).unwrap_or(false), v: vs, i: is, o: os })
    }
}

fn num_str(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        // Shortest round-trip representation (serde_json's).
        serde_json::to_string(&v).unwrap_or_else(|_| v.to_string())
    }
}

/// Lottie bezier → SVG path data (absolute `M`, `L`, `C`, `Z`).
pub fn bez_to_d(b: &Bez, out: &mut String) {
    let nv = b.v.len();
    if nv == 0 {
        return;
    }
    let p = |q: [f64; 2]| format!("{} {}", num_str(q[0]), num_str(q[1]));
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(&format!("M{}", p(b.v[0])));
    let seg = |a: usize, c: usize, out: &mut String| {
        let (oa, ic) = (b.o[a], b.i[c]);
        if oa == [0.0, 0.0] && ic == [0.0, 0.0] {
            out.push_str(&format!(" L{}", p(b.v[c])));
        } else {
            let c1 = [b.v[a][0] + oa[0], b.v[a][1] + oa[1]];
            let c2 = [b.v[c][0] + ic[0], b.v[c][1] + ic[1]];
            out.push_str(&format!(" C{} {} {}", p(c1), p(c2), p(b.v[c])));
        }
    };
    for k in 1..nv {
        seg(k - 1, k, out);
    }
    if b.closed {
        let last = nv - 1;
        let straight = b.o[last] == [0.0, 0.0] && b.i[0] == [0.0, 0.0];
        // A last vertex sitting on the first is a real (duplicate) vertex:
        // spell the closing segment out so parsing does not merge it away.
        if !straight || (nv > 1 && b.v[last] == b.v[0]) {
            seg(last, 0, out);
        }
        out.push_str(" Z");
    }
}

/// SVG path data → Lottie beziers (one per subpath). Supports M L H V C S Q
/// T A Z, absolute and relative.
pub fn d_to_bez(d: &str) -> Result<Vec<Bez>, String> {
    let mut toks = Tokens::new(d);
    let mut out: Vec<Bez> = Vec::new();
    let mut cur = Bez::default();
    let mut pos = [0.0f64; 2];
    let mut start = [0.0f64; 2];
    let mut last_ctrl: Option<[f64; 2]> = None;
    let mut last_q: Option<[f64; 2]> = None;
    let mut cmd = ' ';
    let flush = |cur: &mut Bez, out: &mut Vec<Bez>| {
        if !cur.v.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    // Append a cubic from `pos` to `p` with absolute controls.
    fn cubic(cur: &mut Bez, pos: [f64; 2], c1: [f64; 2], c2: [f64; 2], p: [f64; 2]) {
        if cur.v.is_empty() {
            cur.v.push(pos);
            cur.i.push([0.0, 0.0]);
            cur.o.push([0.0, 0.0]);
        }
        let k = cur.v.len() - 1;
        cur.o[k] = [c1[0] - pos[0], c1[1] - pos[1]];
        cur.v.push(p);
        cur.i.push([c2[0] - p[0], c2[1] - p[1]]);
        cur.o.push([0.0, 0.0]);
    }
    fn line(cur: &mut Bez, pos: [f64; 2], p: [f64; 2]) {
        if cur.v.is_empty() {
            cur.v.push(pos);
            cur.i.push([0.0, 0.0]);
            cur.o.push([0.0, 0.0]);
        }
        cur.v.push(p);
        cur.i.push([0.0, 0.0]);
        cur.o.push([0.0, 0.0]);
    }
    loop {
        toks.skip_ws();
        if toks.done() {
            break;
        }
        if let Some(c) = toks.command() {
            cmd = c;
        } else if cmd == ' ' {
            return Err(format!("path data must start with a command at `{}`", toks.rest()));
        } else if cmd == 'M' {
            cmd = 'L';
        } else if cmd == 'm' {
            cmd = 'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { pos } else { [0.0, 0.0] };
        let pt = |toks: &mut Tokens| -> Result<[f64; 2], String> { Ok([toks.number()? + base[0], toks.number()? + base[1]]) };
        let upper = cmd.to_ascii_uppercase();
        match upper {
            'M' => {
                flush(&mut cur, &mut out);
                pos = pt(&mut toks)?;
                start = pos;
                cur.v.push(pos);
                cur.i.push([0.0, 0.0]);
                cur.o.push([0.0, 0.0]);
                last_ctrl = None;
                last_q = None;
            }
            'L' => {
                let p = pt(&mut toks)?;
                line(&mut cur, pos, p);
                pos = p;
                last_ctrl = None;
                last_q = None;
            }
            'H' => {
                let x = toks.number()? + base[0];
                let p = [x, pos[1]];
                line(&mut cur, pos, p);
                pos = p;
                last_ctrl = None;
                last_q = None;
            }
            'V' => {
                let y = toks.number()? + base[1];
                let p = [pos[0], y];
                line(&mut cur, pos, p);
                pos = p;
                last_ctrl = None;
                last_q = None;
            }
            'C' => {
                let (c1, c2, p) = (pt(&mut toks)?, pt(&mut toks)?, pt(&mut toks)?);
                cubic(&mut cur, pos, c1, c2, p);
                pos = p;
                last_ctrl = Some(c2);
                last_q = None;
            }
            'S' => {
                let c1 = match last_ctrl {
                    Some(c) => [2.0 * pos[0] - c[0], 2.0 * pos[1] - c[1]],
                    None => pos,
                };
                let (c2, p) = (pt(&mut toks)?, pt(&mut toks)?);
                cubic(&mut cur, pos, c1, c2, p);
                pos = p;
                last_ctrl = Some(c2);
                last_q = None;
            }
            'Q' | 'T' => {
                let q = if upper == 'Q' {
                    pt(&mut toks)?
                } else {
                    match last_q {
                        Some(c) => [2.0 * pos[0] - c[0], 2.0 * pos[1] - c[1]],
                        None => pos,
                    }
                };
                let p = pt(&mut toks)?;
                let c1 = [pos[0] + 2.0 / 3.0 * (q[0] - pos[0]), pos[1] + 2.0 / 3.0 * (q[1] - pos[1])];
                let c2 = [p[0] + 2.0 / 3.0 * (q[0] - p[0]), p[1] + 2.0 / 3.0 * (q[1] - p[1])];
                cubic(&mut cur, pos, c1, c2, p);
                pos = p;
                last_q = Some(q);
                last_ctrl = None;
            }
            'A' => {
                let (rx, ry, rot) = (toks.number()?, toks.number()?, toks.number()?);
                let (large, sweep) = (toks.flag()?, toks.flag()?);
                let p = pt(&mut toks)?;
                for (c1, c2, e) in arc_to_cubics(pos, rx, ry, rot, large, sweep, p) {
                    cubic(&mut cur, pos, c1, c2, e);
                    pos = e;
                }
                pos = p;
                last_ctrl = None;
                last_q = None;
            }
            'Z' => {
                if !cur.v.is_empty() {
                    // A segment that returned to the start merges into vertex 0.
                    let k = cur.v.len() - 1;
                    if k > 0 && (cur.v[k][0] - start[0]).abs() < 1e-9 && (cur.v[k][1] - start[1]).abs() < 1e-9 {
                        cur.i[0] = cur.i[k];
                        cur.v.pop();
                        cur.i.pop();
                        let o = cur.o.pop().unwrap_or([0.0, 0.0]);
                        let _ = o;
                    }
                    cur.closed = true;
                    flush(&mut cur, &mut out);
                }
                pos = start;
                last_ctrl = None;
                last_q = None;
            }
            other => return Err(format!("unknown path command `{other}`")),
        }
    }
    flush(&mut cur, &mut out);
    Ok(out)
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Tokens<'a> {
    fn new(s: &'a str) -> Self {
        Self { s: s.as_bytes(), i: 0 }
    }
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn rest(&self) -> String {
        String::from_utf8_lossy(&self.s[self.i..self.s.len().min(self.i + 12)]).into_owned()
    }
    fn command(&mut self) -> Option<char> {
        let c = *self.s.get(self.i)? as char;
        if "MmLlHhVvCcSsQqTtAaZz".contains(c) {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }
    fn number(&mut self) -> Result<f64, String> {
        self.skip_ws();
        let st = self.i;
        let s = self.s;
        let mut j = self.i;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let mut dot = false;
        while j < s.len() && (s[j].is_ascii_digit() || (s[j] == b'.' && !dot)) {
            if s[j] == b'.' {
                dot = true;
            }
            j += 1;
        }
        if j < s.len() && (s[j] == b'e' || s[j] == b'E') {
            let mut k = j + 1;
            if k < s.len() && (s[k] == b'+' || s[k] == b'-') {
                k += 1;
            }
            if k < s.len() && s[k].is_ascii_digit() {
                j = k;
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
            }
        }
        let t = std::str::from_utf8(&s[st..j]).unwrap_or("");
        let v = t.parse::<f64>().map_err(|_| format!("expected a number at `{}`", self.rest()))?;
        self.i = j;
        Ok(v)
    }
    fn flag(&mut self) -> Result<bool, String> {
        self.skip_ws();
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(format!("expected an arc flag at `{}`", self.rest())),
        }
    }
}

/// SVG arc → cubic segments (control 1, control 2, end), W3C F.6.5.
fn arc_to_cubics(p0: [f64; 2], rx: f64, ry: f64, rot_deg: f64, large: bool, sweep: bool, p1: [f64; 2]) -> Vec<([f64; 2], [f64; 2], [f64; 2])> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || p0 == p1 {
        return vec![(p0, p1, p1)];
    }
    let phi = rot_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let dx = (p0[0] - p1[0]) / 2.0;
    let dy = (p0[1] - p1[1]) / 2.0;
    let x1 = cp * dx + sp * dy;
    let y1 = -sp * dx + cp * dy;
    let lam = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lam > 1.0 {
        rx *= lam.sqrt();
        ry *= lam.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = if den == 0.0 { 0.0 } else { (num / den).max(0.0).sqrt() };
    if large == sweep {
        co = -co;
    }
    let cxp = co * rx * y1 / ry;
    let cyp = -co * ry * x1 / rx;
    let cx = cp * cxp - sp * cyp + (p0[0] + p1[0]) / 2.0;
    let cy = sp * cxp + cp * cyp + (p0[1] + p1[1]) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let t1 = ang(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut dt = ang((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
    if !sweep && dt > 0.0 {
        dt -= std::f64::consts::TAU;
    } else if sweep && dt < 0.0 {
        dt += std::f64::consts::TAU;
    }
    let segs = (dt.abs() / (std::f64::consts::FRAC_PI_2)).ceil().max(1.0) as usize;
    let step = dt / segs as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |t: f64| {
        let (s, c) = t.sin_cos();
        [cx + rx * c * cp - ry * s * sp, cy + rx * c * sp + ry * s * cp]
    };
    let deriv = |t: f64| {
        let (s, c) = t.sin_cos();
        [-rx * s * cp - ry * c * sp, -rx * s * sp + ry * c * cp]
    };
    let mut out = Vec::with_capacity(segs);
    for j in 0..segs {
        let a = t1 + step * j as f64;
        let b = a + step;
        let (pa, pb) = (at(a), at(b));
        let (da, db) = (deriv(a), deriv(b));
        let end = if j + 1 == segs { p1 } else { pb };
        out.push(([pa[0] + k * da[0], pa[1] + k * da[1]], [pb[0] - k * db[0], pb[1] - k * db[1]], end));
    }
    out
}

// --- compile ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Val {
    Plain,
    Color,
    Path,
}

struct Compiler {
    fps: f64,
    ms: bool,
    issues: Vec<Issue>,
    slots: Obj,
}

impl Compiler {
    fn time(&self, v: f64) -> f64 {
        if self.ms {
            round_to(v * self.fps / 1000.0, 4)
        } else {
            v
        }
    }

    fn time_of(&mut self, v: Option<&Value>, path: &str) -> Option<f64> {
        let v = v?;
        match f(v) {
            Some(t) => Some(self.time(t)),
            None => {
                self.issues.push(issue("time", path, "expected a number"));
                None
            }
        }
    }

    /// Compile a static value of `kind`; returns (k, sid).
    fn value(&mut self, v: &Value, kind: Val, path: &str) -> Option<(Value, Option<String>)> {
        match kind {
            Val::Color => compile_color(v, path, &mut self.issues).map(|(c, sid)| (Value::Array(c.into_iter().map(n).collect()), sid)),
            Val::Path => match v {
                Value::String(d) => match d_to_bez(d) {
                    Ok(b) if b.len() == 1 => Some((b[0].to_json(), None)),
                    Ok(b) if b.is_empty() => {
                        self.issues.push(issue("path", path, "empty path data"));
                        None
                    }
                    Ok(_) => {
                        self.issues.push(issue("path", path, "one subpath per animated value (split it into several paths)"));
                        None
                    }
                    Err(e) => {
                        self.issues.push(issue("path", path, e));
                        None
                    }
                },
                other => Some((other.clone(), None)),
            },
            Val::Plain => match v {
                Value::Number(_) | Value::Array(_) => Some((v.clone(), None)),
                _ => {
                    self.issues.push(issue("value", path, "expected a number or an array of numbers"));
                    None
                }
            },
        }
    }

    /// A dialect value (static or `{keys}`) → a Lottie animatable property.
    fn prop(&mut self, v: &Value, kind: Val, path: &str) -> Value {
        let Some(keys) = v.get("keys").and_then(Value::as_array) else {
            if let Value::Object(o) = v {
                if !(kind == Val::Color && o.contains_key("light")) && kind != Val::Path {
                    self.issues.push(issue("value", path, "an object value must be {\"keys\": [...]}"));
                    return json!({"a": 0, "k": 0});
                }
            }
            return match self.value(v, kind, path) {
                Some((k, sid)) => {
                    let mut o = Obj::new();
                    o.insert("a".into(), json!(0));
                    o.insert("k".into(), k.clone());
                    if let Some(sid) = sid {
                        self.slots.insert(sid.clone(), json!({"p": {"a": 0, "k": k}}));
                        o.insert("sid".into(), json!(sid));
                    }
                    Value::Object(o)
                }
                None => json!({"a": 0, "k": 0}),
            };
        };
        if keys.is_empty() {
            self.issues.push(issue("keys", path, "keys must not be empty"));
            return json!({"a": 0, "k": 0});
        }
        let mut out = Vec::new();
        let mut last_t = f64::NEG_INFINITY;
        for (j, key) in keys.iter().enumerate() {
            let kp = format!("{path}/keys/{j}");
            let Some(t) = self.time_of(key.get("t"), &format!("{kp}/t")) else { continue };
            // Out-of-order keys are kept as written (Lottie allows them;
            // players use the first key at or before the time).
            let _ = last_t;
            last_t = t;
            let mut kf = Obj::new();
            kf.insert("t".into(), n(t));
            if let Some(v) = key.get("v") {
                if kind == Val::Color && matches!(v, Value::String(s) if s.starts_with('$')) || matches!(v, Value::Object(_)) && kind == Val::Color {
                    self.issues.push(issue("color-animated", &format!("{kp}/v"), "theme tokens and light/dark pairs cannot be animated; use literal colours in keys"));
                }
                if let Some((k, _)) = self.value(v, kind, &format!("{kp}/v")) {
                    let s = match (kind, k) {
                        (Val::Path, k) => json!([k]),
                        (_, Value::Number(x)) => json!([x]),
                        (_, k) => k,
                    };
                    kf.insert("s".into(), s);
                }
            } else if j + 1 < keys.len() {
                self.issues.push(issue("keys", &kp, "a key needs `v`"));
            }
            if let Some(e) = key.get("end") {
                if let Some((k, _)) = self.value(e, kind, &format!("{kp}/end")) {
                    kf.insert("e".into(), if let Value::Number(x) = k { json!([x]) } else if kind == Val::Path { json!([k]) } else { k });
                }
            }
            let last = j + 1 == keys.len();
            match key.get("ease") {
                Some(Value::String(s)) if s == "hold" => {
                    kf.insert("h".into(), json!(1));
                }
                Some(e) if !last => match ease_points(e) {
                    Ok(Some(EaseSpec::Uniform(p))) => {
                        kf.insert("o".into(), json!({"x": n(p[0]), "y": n(p[1])}));
                        kf.insert("i".into(), json!({"x": n(p[2]), "y": n(p[3])}));
                    }
                    Ok(Some(EaseSpec::Raw(o, i))) => {
                        kf.insert("o".into(), o);
                        kf.insert("i".into(), i);
                    }
                    Ok(None) => {}
                    Err(m) => self.issues.push(issue("ease", &format!("{kp}/ease"), m)),
                },
                None if !last => {
                    kf.insert("o".into(), json!({"x": 0, "y": 0}));
                    kf.insert("i".into(), json!({"x": 1, "y": 1}));
                }
                _ => {}
            }
            for k in ["to", "ti"] {
                if let Some(t) = key.get(k) {
                    kf.insert(k.into(), t.clone());
                }
            }
            out.push(Value::Object(kf));
        }
        json!({"a": 1, "k": out})
    }

    fn opt_prop(&mut self, o: &Obj, key: &str, kind: Val, path: &str, out: &mut Obj, lottie_key: &str) {
        if let Some(v) = o.get(key) {
            let p = self.prop(v, kind, &format!("{path}/{key}"));
            out.insert(lottie_key.into(), p);
        }
    }
}

enum EaseSpec {
    Uniform([f64; 4]),
    Raw(Value, Value),
}

fn ease_points(e: &Value) -> Result<Option<EaseSpec>, String> {
    match e {
        Value::String(s) => EASES
            .iter()
            .find(|(name, _)| name == s)
            .map(|(_, p)| Some(EaseSpec::Uniform(*p)))
            .ok_or_else(|| format!("unknown ease `{s}` (linear, ease, ease-in, ease-out, ease-in-out, hold or [x1, y1, x2, y2])")),
        Value::Array(a) if a.len() == 4 && a.iter().all(|x| x.is_number()) => {
            let p: Vec<f64> = a.iter().filter_map(f).collect();
            Ok(Some(EaseSpec::Uniform([p[0], p[1], p[2], p[3]])))
        }
        Value::Object(o) if o.contains_key("o") && o.contains_key("i") => Ok(Some(EaseSpec::Raw(o["o"].clone(), o["i"].clone()))),
        _ => Err("ease must be a name, [x1, y1, x2, y2] or {o, i}".into()),
    }
}

/// Compile a dialect document into Lottie JSON.
pub fn compile(doc: &Value) -> Result<Value, Vec<Issue>> {
    let Some(root) = doc.as_object() else {
        return Err(vec![issue("root", "", "expected an object")]);
    };
    let fps = root.get("fps").and_then(f).unwrap_or(60.0);
    let ms = root.get("time").and_then(Value::as_str) != Some("frames");
    let mut c = Compiler { fps, ms, issues: Vec::new(), slots: Obj::new() };
    if !(fps > 0.0 && fps <= 120.0) {
        c.issues.push(issue("fps", "/fps", "fps must be in (0, 120]"));
    }
    let size = root.get("size").and_then(Value::as_array);
    let (w, h) = match size {
        Some(a) if a.len() == 2 => (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)),
        _ => {
            c.issues.push(issue("size", "/size", "size [width, height] is required"));
            (0.0, 0.0)
        }
    };
    let ip = c.time_of(root.get("start"), "/start").unwrap_or(0.0);
    let op = match c.time_of(root.get("duration"), "/duration") {
        Some(d) if ms => round_to(ip + d, 4),
        Some(d) => ip + d,
        None => {
            c.issues.push(issue("duration", "/duration", "duration is required"));
            ip + 1.0
        }
    };
    let mut out = Obj::new();
    out.insert("v".into(), json!("5.12.0"));
    out.insert("fr".into(), n(fps));
    out.insert("ip".into(), n(ip));
    out.insert("op".into(), n(op));
    out.insert("w".into(), n(w));
    out.insert("h".into(), n(h));
    if let Some(name) = root.get("name") {
        out.insert("nm".into(), name.clone());
    }
    out.insert("ddd".into(), json!(0));
    let mut assets = Vec::new();
    if let Some(Value::Object(a)) = root.get("assets") {
        for (id, asset) in a {
            let path = format!("/assets/{id}");
            if let Some(layers) = asset.get("layers").and_then(Value::as_array) {
                let ls = c.layers(layers, &format!("{path}/layers"), op);
                assets.push(json!({"id": id, "layers": ls}));
            } else if let Some(src) = asset.get("image").and_then(Value::as_str) {
                let (iw, ih) = asset
                    .get("size")
                    .and_then(Value::as_array)
                    .map(|s| (s.first().and_then(f).unwrap_or(0.0), s.get(1).and_then(f).unwrap_or(0.0)))
                    .unwrap_or((0.0, 0.0));
                let embedded = src.starts_with("data:");
                assets.push(json!({"id": id, "w": n(iw), "h": n(ih), "u": "", "p": src, "e": if embedded { 1 } else { 0 }}));
            } else {
                c.issues.push(issue("asset", &path, "an asset needs `layers` or `image`"));
            }
        }
    }
    out.insert("assets".into(), Value::Array(assets));
    let layers = root.get("layers").and_then(Value::as_array).cloned().unwrap_or_default();
    let ls = c.layers(&layers, "/layers", op);
    out.insert("layers".into(), Value::Array(ls));
    if let Some(Value::Array(ms_)) = root.get("markers") {
        let mut mk = Vec::new();
        for (j, m) in ms_.iter().enumerate() {
            let p = format!("/markers/{j}");
            let tm = c.time_of(m.get("time"), &format!("{p}/time")).unwrap_or(0.0);
            let dr = c.time_of(m.get("duration"), &format!("{p}/duration")).unwrap_or(0.0);
            mk.push(json!({"cm": m.get("name").cloned().unwrap_or(json!("")), "tm": n(tm), "dr": n(dr)}));
        }
        out.insert("markers".into(), Value::Array(mk));
    }
    if !c.slots.is_empty() {
        out.insert("slots".into(), Value::Object(std::mem::take(&mut c.slots)));
    }
    let mut meta = Obj::new();
    meta.insert("g".into(), json!(GENERATOR));
    if !ms {
        meta.insert("time".into(), json!("frames"));
    }
    out.insert("meta".into(), Value::Object(meta));
    if let Some(Value::Object(raw)) = root.get("lottie") {
        for (k, v) in raw {
            out.insert(k.clone(), v.clone());
        }
    }
    if c.issues.is_empty() {
        Ok(Value::Object(out))
    } else {
        Err(c.issues)
    }
}

pub(crate) const LAYER_TYPES: [(&str, i64); 6] = [("precomp", 0), ("solid", 1), ("image", 2), ("null", 3), ("shape", 4), ("text", 5)];
pub(crate) const BLENDS: [&str; 16] = [
    "normal", "multiply", "screen", "overlay", "darken", "lighten", "color-dodge", "color-burn", "hard-light", "soft-light", "difference",
    "exclusion", "hue", "saturation", "color", "luminosity",
];
pub(crate) const MATTES: [&str; 4] = ["alpha", "alpha-inverted", "luma", "luma-inverted"];
pub(crate) const MASK_MODES: [(&str, &str); 7] = [
    ("add", "a"),
    ("subtract", "s"),
    ("intersect", "i"),
    ("lighten", "l"),
    ("darken", "d"),
    ("difference", "f"),
    ("none", "n"),
];

impl Compiler {
    fn layers(&mut self, layers: &[Value], path: &str, comp_op: f64) -> Vec<Value> {
        // Ids: numbers are kept as `ind`; names map to fresh indices and ride along in `ln`.
        let mut used: Vec<i64> = layers.iter().filter_map(|l| l.get("id").and_then(Value::as_i64)).collect();
        let mut next = used.iter().copied().max().unwrap_or(0) + 1;
        let mut ids: Vec<(String, i64)> = Vec::new();
        let mut inds = Vec::new();
        for l in layers {
            let ind = match l.get("id") {
                Some(Value::Number(x)) => x.as_i64().unwrap_or(0),
                Some(Value::String(s)) => {
                    let i = next;
                    next += 1;
                    ids.push((s.clone(), i));
                    i
                }
                _ => {
                    let i = next;
                    next += 1;
                    i
                }
            };
            used.push(ind);
            inds.push(ind);
        }
        let resolve = |v: &Value| -> Option<i64> {
            match v {
                Value::Number(x) => x.as_i64(),
                Value::String(s) => ids.iter().find(|(k, _)| k == s).map(|(_, i)| *i),
                _ => None,
            }
        };
        let mattes: Vec<Option<i64>> = layers.iter().map(|l| l.get("matte").and_then(|m| m.get("source")).and_then(&resolve)).collect();
        let mut out = Vec::new();
        for (j, l) in layers.iter().enumerate() {
            let p = format!("{path}/{j}");
            let Some(o) = l.as_object() else {
                self.issues.push(issue("layer", &p, "expected an object"));
                continue;
            };
            if o.get("type").and_then(Value::as_str) == Some("lottie") {
                // Raw Lottie, verbatim (what the dialect cannot name).
                out.push(o.get("lottie").cloned().unwrap_or(json!({})));
                continue;
            }
            let mut lo = Obj::new();
            let ty = o.get("type").and_then(Value::as_str).unwrap_or("shape");
            let Some(&(_, tyn)) = LAYER_TYPES.iter().find(|(k, _)| *k == ty) else {
                self.issues.push(issue("layer-type", &format!("{p}/type"), format!("unknown layer type `{ty}`")));
                continue;
            };
            lo.insert("ddd".into(), json!(0));
            lo.insert("ind".into(), json!(inds[j]));
            lo.insert("ty".into(), json!(tyn));
            if let Some(name) = o.get("name") {
                lo.insert("nm".into(), name.clone());
            }
            if let Some(Value::String(s)) = o.get("id") {
                lo.insert("ln".into(), json!(s));
            }
            if let Some(parent) = o.get("parent").filter(|p| !p.is_null()) {
                match resolve(parent) {
                    Some(i) => {
                        lo.insert("parent".into(), json!(i));
                    }
                    None => self.issues.push(issue("parent", &format!("{p}/parent"), "no layer with this id")),
                }
            }
            if let Some(m) = o.get("matte").filter(|m| !m.is_null()) {
                let mode = m.get("mode").and_then(Value::as_str).unwrap_or("alpha");
                match (MATTES.iter().position(|x| *x == mode), mattes[j]) {
                    (Some(k), Some(src)) => {
                        lo.insert("tt".into(), json!(k + 1));
                        lo.insert("tp".into(), json!(src));
                    }
                    (None, _) => self.issues.push(issue("matte", &format!("{p}/matte/mode"), format!("unknown matte mode `{mode}`"))),
                    (_, None) => self.issues.push(issue("matte", &format!("{p}/matte/source"), "no layer with this id")),
                }
            }
            if mattes.contains(&Some(inds[j])) {
                lo.insert("td".into(), json!(1));
            }
            if o.get("hidden").and_then(Value::as_bool) == Some(true) {
                lo.insert("hd".into(), json!(true));
            }
            if o.get("autoOrient").and_then(Value::as_bool) == Some(true) {
                lo.insert("ao".into(), json!(1));
            }
            if let Some(b) = o.get("blend").and_then(Value::as_str) {
                match BLENDS.iter().position(|x| *x == b) {
                    Some(k) if k > 0 => {
                        lo.insert("bm".into(), json!(k));
                    }
                    Some(_) => {}
                    None => self.issues.push(issue("blend", &format!("{p}/blend"), format!("unknown blend mode `{b}`"))),
                }
            }
            let ks = self.transform(o.get("transform"), &format!("{p}/transform"), false);
            lo.insert("ks".into(), ks);
            match ty {
                "shape" => {
                    let shapes = o.get("shapes").and_then(Value::as_array).cloned().unwrap_or_default();
                    let s = self.shapes(&shapes, &format!("{p}/shapes"));
                    lo.insert("shapes".into(), Value::Array(s));
                }
                "solid" => {
                    let c = o.get("color").and_then(|c| compile_color(c, &format!("{p}/color"), &mut self.issues));
                    let hex = c.and_then(|(c, _)| format_hex(&c[..3])).unwrap_or_else(|| "#000000".into());
                    let (sw, sh) = size_of(o.get("size"));
                    lo.insert("sc".into(), json!(hex));
                    lo.insert("sw".into(), n(sw));
                    lo.insert("sh".into(), n(sh));
                }
                "image" | "precomp" => {
                    match o.get("asset").and_then(Value::as_str) {
                        Some(a) => {
                            lo.insert("refId".into(), json!(a));
                        }
                        None => self.issues.push(issue("asset", &format!("{p}/asset"), "an image or precomp layer needs `asset`")),
                    }
                    if ty == "precomp" {
                        let (pw, ph) = size_of(o.get("size"));
                        lo.insert("w".into(), n(pw));
                        lo.insert("h".into(), n(ph));
                        if let Some(tm) = o.get("timeRemap") {
                            // Time remap values are SECONDS in Lottie; the dialect uses the document's time unit.
                            let remapped = scale_values(tm, if self.ms { 0.001 } else { 1.0 / self.fps });
                            let pr = self.prop(&remapped, Val::Plain, &format!("{p}/timeRemap"));
                            lo.insert("tm".into(), pr);
                        }
                    }
                }
                "text" => {
                    if let Some(t) = o.get("text") {
                        let td = self.text(t, &format!("{p}/text"));
                        lo.insert("t".into(), td);
                    }
                }
                _ => {}
            }
            if let Some(Value::Array(masks)) = o.get("masks") {
                let mut mo = Vec::new();
                for (k, m) in masks.iter().enumerate() {
                    let mp = format!("{p}/masks/{k}");
                    let mode = m.get("mode").and_then(Value::as_str).unwrap_or("add");
                    let code = MASK_MODES.iter().find(|(x, _)| *x == mode).map(|(_, c)| *c);
                    let Some(code) = code else {
                        self.issues.push(issue("mask-mode", &format!("{mp}/mode"), format!("unknown mask mode `{mode}`")));
                        continue;
                    };
                    let mut mm = Obj::new();
                    mm.insert("inv".into(), json!(m.get("inverted").and_then(Value::as_bool).unwrap_or(false)));
                    mm.insert("mode".into(), json!(code));
                    let pt = self.prop(m.get("path").unwrap_or(&Value::Null), Val::Path, &format!("{mp}/path"));
                    mm.insert("pt".into(), pt);
                    let o_ = m.get("opacity").cloned().unwrap_or(json!(100));
                    mm.insert("o".into(), self.prop(&o_, Val::Plain, &format!("{mp}/opacity")));
                    if let Some(x) = m.get("expansion") {
                        mm.insert("x".into(), self.prop(x, Val::Plain, &format!("{mp}/expansion")));
                    }
                    if let Some(x) = m.get("feather") {
                        mm.insert("f".into(), self.prop(x, Val::Plain, &format!("{mp}/feather")));
                    }
                    mo.push(Value::Object(mm));
                }
                lo.insert("hasMask".into(), json!(true));
                lo.insert("masksProperties".into(), Value::Array(mo));
            }
            let ip = self.time_of(o.get("in"), &format!("{p}/in")).unwrap_or(0.0);
            let op = self.time_of(o.get("out"), &format!("{p}/out")).unwrap_or(comp_op);
            let st = self.time_of(o.get("start"), &format!("{p}/start")).unwrap_or(0.0);
            lo.insert("ip".into(), n(ip));
            lo.insert("op".into(), n(op));
            lo.insert("st".into(), n(st));
            if let Some(sr) = o.get("stretch").and_then(f) {
                lo.insert("sr".into(), n(sr));
            }
            if let Some(Value::Object(raw)) = o.get("lottie") {
                for (k, v) in raw {
                    lo.insert(k.clone(), v.clone());
                }
            }
            out.push(Value::Object(lo));
        }
        out
    }

    /// `transform` → `ks` / `tr`. Defaults are always written (lottie-web wants them).
    fn transform(&mut self, t: Option<&Value>, path: &str, shape: bool) -> Value {
        let empty = Obj::new();
        let o = t.and_then(Value::as_object).unwrap_or(&empty);
        let mut out = Obj::new();
        if shape {
            out.insert("ty".into(), json!("tr"));
        }
        let put = |c: &mut Compiler, out: &mut Obj, key: &str, lk: &str, default: Value| {
            let v = o.get(key).cloned().unwrap_or(default);
            let p = c.prop(&v, Val::Plain, &format!("{path}/{key}"));
            out.insert(lk.into(), p);
        };
        put(self, &mut out, "anchor", "a", json!([0, 0]));
        if o.contains_key("positionX") || o.contains_key("positionY") {
            let x = o.get("positionX").cloned().unwrap_or(json!(0));
            let y = o.get("positionY").cloned().unwrap_or(json!(0));
            let px = self.prop(&x, Val::Plain, &format!("{path}/positionX"));
            let py = self.prop(&y, Val::Plain, &format!("{path}/positionY"));
            out.insert("p".into(), json!({"s": true, "x": px, "y": py}));
        } else {
            put(self, &mut out, "position", "p", json!([0, 0]));
        }
        put(self, &mut out, "scale", "s", json!([100, 100]));
        put(self, &mut out, "rotation", "r", json!(0));
        put(self, &mut out, "opacity", "o", json!(100));
        if o.contains_key("skew") || shape {
            put(self, &mut out, "skew", "sk", json!(0));
            put(self, &mut out, "skewAxis", "sa", json!(0));
        }
        if let Some(v) = o.get("startOpacity") {
            let p = self.prop(v, Val::Plain, &format!("{path}/startOpacity"));
            out.insert("so".into(), p);
        }
        if let Some(v) = o.get("endOpacity") {
            let p = self.prop(v, Val::Plain, &format!("{path}/endOpacity"));
            out.insert("eo".into(), p);
        }
        if let Some(Value::Object(raw)) = o.get("lottie") {
            for (k, v) in raw {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out)
    }

    fn text(&mut self, t: &Value, path: &str) -> Value {
        let s = |k: &str| t.get(k);
        let mut doc = Obj::new();
        doc.insert("t".into(), s("text").cloned().unwrap_or(json!("")));
        doc.insert("f".into(), s("font").cloned().unwrap_or(json!("sans")));
        doc.insert("s".into(), s("size").cloned().unwrap_or(json!(24)));
        let fc = s("color").and_then(|c| compile_color(c, &format!("{path}/color"), &mut self.issues)).map(|(c, _)| c).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
        doc.insert("fc".into(), Value::Array(fc.into_iter().take(3).map(n).collect()));
        if let Some(sc) = s("strokeColor").and_then(|c| compile_color(c, &format!("{path}/strokeColor"), &mut self.issues)) {
            doc.insert("sc".into(), Value::Array(sc.0.into_iter().take(3).map(n).collect()));
            doc.insert("sw".into(), s("strokeWidth").cloned().unwrap_or(json!(1)));
        }
        let j = match s("align").and_then(Value::as_str).unwrap_or("left") {
            "right" => 1,
            "center" => 2,
            _ => 0,
        };
        doc.insert("j".into(), json!(j));
        if let Some(v) = s("tracking") {
            doc.insert("tr".into(), v.clone());
        }
        if let Some(v) = s("lineHeight") {
            doc.insert("lh".into(), v.clone());
        }
        if let Some(v) = s("box") {
            doc.insert("sz".into(), v.clone());
        }
        let mut out = Obj::new();
        out.insert("d".into(), json!({"k": [{"s": doc, "t": 0}]}));
        out.insert("p".into(), json!({}));
        out.insert("m".into(), json!({"g": 1, "a": {"a": 0, "k": [0, 0]}}));
        out.insert("a".into(), s("animators").cloned().unwrap_or(json!([])));
        Value::Object(out)
    }

    fn shapes(&mut self, shapes: &[Value], path: &str) -> Vec<Value> {
        let mut out = Vec::new();
        for (j, s) in shapes.iter().enumerate() {
            let p = format!("{path}/{j}");
            let Some(o) = s.as_object() else {
                self.issues.push(issue("shape", &p, "expected an object"));
                continue;
            };
            let ty = o.get("type").and_then(Value::as_str).unwrap_or("");
            if ty == "lottie" {
                out.push(o.get("lottie").cloned().unwrap_or(json!({})));
                continue;
            }
            let mut so = Obj::new();
            let code = match ty {
                "group" => "gr",
                "rect" => "rc",
                "ellipse" => "el",
                "star" | "polygon" => "sr",
                "path" => "sh",
                "fill" => "fl",
                "stroke" => "st",
                "gradient-fill" => "gf",
                "gradient-stroke" => "gs",
                "trim" => "tm",
                "repeater" => "rp",
                "round-corners" => "rd",
                "pucker-bloat" => "pb",
                "zig-zag" => "zz",
                "offset-path" => "op",
                "twist" => "tw",
                "merge" => "mm",
                other => {
                    self.issues.push(issue("shape-type", &format!("{p}/type"), format!("unknown shape type `{other}`")));
                    continue;
                }
            };
            so.insert("ty".into(), json!(code));
            if let Some(name) = o.get("name") {
                so.insert("nm".into(), name.clone());
            }
            if o.get("hidden").and_then(Value::as_bool) == Some(true) {
                so.insert("hd".into(), json!(true));
            }
            let dir = |so: &mut Obj| {
                let ccw = o.get("direction").and_then(Value::as_str) == Some("ccw");
                so.insert("d".into(), json!(if ccw { 3 } else { 1 }));
            };
            match code {
                "gr" => {
                    let items = o.get("shapes").and_then(Value::as_array).cloned().unwrap_or_default();
                    let mut it = self.shapes(&items, &format!("{p}/shapes"));
                    it.push(self.transform(o.get("transform"), &format!("{p}/transform"), true));
                    so.insert("it".into(), Value::Array(it));
                }
                "rc" => {
                    dir(&mut so);
                    let pos = o.get("position").cloned().unwrap_or(json!([0, 0]));
                    so.insert("p".into(), self.prop(&pos, Val::Plain, &format!("{p}/position")));
                    self.opt_prop(o, "size", Val::Plain, &p, &mut so, "s");
                    let r = o.get("radius").cloned().unwrap_or(json!(0));
                    so.insert("r".into(), self.prop(&r, Val::Plain, &format!("{p}/radius")));
                }
                "el" => {
                    dir(&mut so);
                    let pos = o.get("position").cloned().unwrap_or(json!([0, 0]));
                    so.insert("p".into(), self.prop(&pos, Val::Plain, &format!("{p}/position")));
                    self.opt_prop(o, "size", Val::Plain, &p, &mut so, "s");
                }
                "sr" => {
                    dir(&mut so);
                    so.insert("sy".into(), json!(if ty == "polygon" { 2 } else { 1 }));
                    let pos = o.get("position").cloned().unwrap_or(json!([0, 0]));
                    so.insert("p".into(), self.prop(&pos, Val::Plain, &format!("{p}/position")));
                    for (k, lk, d) in [
                        ("points", "pt", json!(5)),
                        ("outerRadius", "or", json!(50)),
                        ("outerRoundness", "os", json!(0)),
                        ("rotation", "r", json!(0)),
                    ] {
                        let v = o.get(k).cloned().unwrap_or(d);
                        so.insert(lk.into(), self.prop(&v, Val::Plain, &format!("{p}/{k}")));
                    }
                    if ty == "star" {
                        let ir = o.get("innerRadius").cloned().unwrap_or(json!(25));
                        so.insert("ir".into(), self.prop(&ir, Val::Plain, &format!("{p}/innerRadius")));
                        let is = o.get("innerRoundness").cloned().unwrap_or(json!(0));
                        so.insert("is".into(), self.prop(&is, Val::Plain, &format!("{p}/innerRoundness")));
                    }
                }
                "sh" => {
                    // Several subpaths compile to consecutive `sh` items; the
                    // continuations carry `mn: "eui:cont"` so decompile rejoins them.
                    if o.get("d").is_none() && o.get("lottie").and_then(|r| r.get("ks")).is_some() {
                        // The path is a raw property (a foreign slot or an expression).
                        let mut item = so.clone();
                        if let Some(Value::Object(raw)) = o.get("lottie") {
                            for (rk, rv) in raw {
                                item.insert(rk.clone(), rv.clone());
                            }
                        }
                        out.push(Value::Object(item));
                        continue;
                    }
                    let d = o.get("d").cloned().unwrap_or(Value::Null);
                    let pieces = self.split_path(&d, &format!("{p}/d"));
                    for (k, piece) in pieces.into_iter().enumerate() {
                        let mut item = so.clone();
                        item.insert("ks".into(), self.prop(&piece, Val::Path, &format!("{p}/d")));
                        if k > 0 {
                            item.insert("mn".into(), json!("eui:cont"));
                        } else if let Some(Value::Object(raw)) = o.get("lottie") {
                            for (rk, rv) in raw {
                                item.insert(rk.clone(), rv.clone());
                            }
                        }
                        out.push(Value::Object(item));
                    }
                    continue;
                }
                "fl" => {
                    let c = o.get("color").cloned().unwrap_or(json!("#000000"));
                    so.insert("c".into(), self.prop(&c, Val::Color, &format!("{p}/color")));
                    let op = o.get("opacity").cloned().unwrap_or(json!(100));
                    so.insert("o".into(), self.prop(&op, Val::Plain, &format!("{p}/opacity")));
                    so.insert("r".into(), json!(if o.get("rule").and_then(Value::as_str) == Some("evenodd") { 2 } else { 1 }));
                }
                "st" | "gs" => {
                    if code == "st" {
                        let c = o.get("color").cloned().unwrap_or(json!("#000000"));
                        so.insert("c".into(), self.prop(&c, Val::Color, &format!("{p}/color")));
                    } else {
                        self.gradient(o, &p, &mut so);
                    }
                    let op = o.get("opacity").cloned().unwrap_or(json!(100));
                    so.insert("o".into(), self.prop(&op, Val::Plain, &format!("{p}/opacity")));
                    let w = o.get("width").cloned().unwrap_or(json!(1));
                    so.insert("w".into(), self.prop(&w, Val::Plain, &format!("{p}/width")));
                    let cap = match o.get("cap").and_then(Value::as_str).unwrap_or("round") {
                        "butt" => 1,
                        "square" => 3,
                        _ => 2,
                    };
                    let join = match o.get("join").and_then(Value::as_str).unwrap_or("round") {
                        "miter" => 1,
                        "bevel" => 3,
                        _ => 2,
                    };
                    so.insert("lc".into(), json!(cap));
                    so.insert("lj".into(), json!(join));
                    if let Some(ml) = o.get("miterLimit") {
                        so.insert("ml".into(), ml.clone());
                    }
                    if let Some(Value::Array(d)) = o.get("dash") {
                        let mut ds = Vec::new();
                        for (k, v) in d.iter().enumerate() {
                            let (nm, key) = if k % 2 == 0 { ("d", "d") } else { ("g", "g") };
                            let _ = key;
                            ds.push(json!({"n": nm, "v": self.prop(v, Val::Plain, &format!("{p}/dash/{k}"))}));
                        }
                        if let Some(off) = o.get("dashOffset") {
                            ds.push(json!({"n": "o", "v": self.prop(off, Val::Plain, &format!("{p}/dashOffset"))}));
                        }
                        so.insert("d".into(), Value::Array(ds));
                    }
                }
                "gf" => {
                    self.gradient(o, &p, &mut so);
                    let op = o.get("opacity").cloned().unwrap_or(json!(100));
                    so.insert("o".into(), self.prop(&op, Val::Plain, &format!("{p}/opacity")));
                    so.insert("r".into(), json!(if o.get("rule").and_then(Value::as_str) == Some("evenodd") { 2 } else { 1 }));
                }
                "tm" => {
                    for (k, lk, d) in [("start", "s", json!(0)), ("end", "e", json!(100)), ("offset", "o", json!(0))] {
                        let v = o.get(k).cloned().unwrap_or(d);
                        so.insert(lk.into(), self.prop(&v, Val::Plain, &format!("{p}/{k}")));
                    }
                    so.insert("m".into(), json!(if o.get("mode").and_then(Value::as_str) == Some("individual") { 2 } else { 1 }));
                }
                "rp" => {
                    let c = o.get("copies").cloned().unwrap_or(json!(3));
                    so.insert("c".into(), self.prop(&c, Val::Plain, &format!("{p}/copies")));
                    let off = o.get("offset").cloned().unwrap_or(json!(0));
                    so.insert("o".into(), self.prop(&off, Val::Plain, &format!("{p}/offset")));
                    so.insert("m".into(), json!(if o.get("composite").and_then(Value::as_str) == Some("below") { 2 } else { 1 }));
                    let mut tr = self.transform(o.get("transform"), &format!("{p}/transform"), true);
                    if let Value::Object(t) = &mut tr {
                        t.entry("so").or_insert(json!({"a": 0, "k": 100}));
                        t.entry("eo").or_insert(json!({"a": 0, "k": 100}));
                    }
                    so.insert("tr".into(), tr);
                }
                "rd" => self.opt_prop(o, "radius", Val::Plain, &p, &mut so, "r"),
                "pb" => self.opt_prop(o, "amount", Val::Plain, &p, &mut so, "a"),
                "zz" => {
                    self.opt_prop(o, "size", Val::Plain, &p, &mut so, "s");
                    self.opt_prop(o, "ridges", Val::Plain, &p, &mut so, "r");
                    so.insert("pt".into(), json!({"a": 0, "k": if o.get("smooth").and_then(Value::as_bool) == Some(true) { 2 } else { 1 }}));
                }
                "op" => {
                    self.opt_prop(o, "amount", Val::Plain, &p, &mut so, "a");
                    let join = match o.get("join").and_then(Value::as_str).unwrap_or("miter") {
                        "round" => 2,
                        "bevel" => 3,
                        _ => 1,
                    };
                    so.insert("lj".into(), json!(join));
                    self.opt_prop(o, "miterLimit", Val::Plain, &p, &mut so, "ml");
                }
                "tw" => {
                    self.opt_prop(o, "angle", Val::Plain, &p, &mut so, "a");
                    self.opt_prop(o, "center", Val::Plain, &p, &mut so, "c");
                }
                "mm" => {
                    let m = match o.get("mode").and_then(Value::as_str).unwrap_or("merge") {
                        "add" => 2,
                        "subtract" => 3,
                        "intersect" => 4,
                        "exclude" => 5,
                        _ => 1,
                    };
                    so.insert("mm".into(), json!(m));
                }
                _ => {}
            }
            if let Some(Value::Object(raw)) = o.get("lottie") {
                for (k, v) in raw {
                    so.insert(k.clone(), v.clone());
                }
            }
            out.push(Value::Object(so));
        }
        out
    }

    /// A (possibly animated) multi-subpath `d` → one value per subpath.
    fn split_path(&mut self, d: &Value, path: &str) -> Vec<Value> {
        match d {
            Value::String(s) => match d_to_bez(s) {
                Ok(b) if !b.is_empty() => b.iter().map(|x| x.to_json()).collect(),
                // An empty path stays an empty (invisible) shape.
                Ok(_) => vec![Bez::default().to_json()],
                Err(e) => {
                    self.issues.push(issue("path", path, e));
                    vec![]
                }
            },
            Value::Object(o) if o.contains_key("keys") => {
                let keys = o["keys"].as_array().cloned().unwrap_or_default();
                let mut per: Vec<Vec<Value>> = Vec::new();
                for (j, k) in keys.iter().enumerate() {
                    if k.get("v").is_none() && j + 1 == keys.len() && !per.is_empty() {
                        // A legacy final key carries only its time.
                        for s in per.iter_mut() {
                            s.push(k.clone());
                        }
                        continue;
                    }
                    let parsed = |v: Option<&Value>| -> Result<Vec<Bez>, String> {
                        match v.and_then(Value::as_str) {
                            Some(s) => d_to_bez(s),
                            None => Err("a path key needs `v` with path data".into()),
                        }
                    };
                    match parsed(k.get("v")) {
                        Ok(b) => {
                            if per.is_empty() {
                                per = vec![Vec::new(); b.len()];
                            }
                            if b.len() != per.len() {
                                self.issues.push(issue("path", &format!("{path}/keys/{j}"), "every key must have the same number of subpaths"));
                                return vec![];
                            }
                            for (s, bz) in b.into_iter().enumerate() {
                                let mut key = k.clone();
                                key["v"] = bz.to_json();
                                if let Some(e) = k.get("end").and_then(Value::as_str) {
                                    if let Ok(eb) = d_to_bez(e) {
                                        if let Some(x) = eb.get(s) {
                                            key["end"] = x.to_json();
                                        }
                                    }
                                }
                                per[s].push(key);
                            }
                        }
                        Err(e) => {
                            self.issues.push(issue("path", &format!("{path}/keys/{j}"), e));
                            return vec![];
                        }
                    }
                }
                per.into_iter().map(|keys| json!({"keys": keys})).collect()
            }
            _ => {
                self.issues.push(issue("path", path, "`d` must be SVG path data or {\"keys\": [...]}"));
                vec![]
            }
        }
    }

    fn gradient(&mut self, o: &Obj, p: &str, so: &mut Obj) {
        let kind = o.get("kind").and_then(Value::as_str).unwrap_or("linear");
        so.insert("t".into(), json!(if kind == "radial" { 2 } else { 1 }));
        let s = o.get("start").cloned().unwrap_or(json!([0, 0]));
        so.insert("s".into(), self.prop(&s, Val::Plain, &format!("{p}/start")));
        let e = o.get("end").cloned().unwrap_or(json!([100, 0]));
        so.insert("e".into(), self.prop(&e, Val::Plain, &format!("{p}/end")));
        if kind == "radial" {
            let h = o.get("highlight").cloned().unwrap_or(json!(0));
            so.insert("h".into(), self.prop(&h, Val::Plain, &format!("{p}/highlight")));
            let a = o.get("highlightAngle").cloned().unwrap_or(json!(0));
            so.insert("a".into(), self.prop(&a, Val::Plain, &format!("{p}/highlightAngle")));
        }
        // stops: [[offset, colour], …] (static) or {"keys": [{t, v: [[offset, colour], …]}]}
        let stops = o.get("stops").cloned().unwrap_or(json!([[0, "#000000"], [1, "#ffffff"]]));
        let flat = |c: &mut Compiler, v: &Value, path: &str| -> (usize, Vec<f64>) {
            let mut cols = Vec::new();
            let mut alphas = Vec::new();
            let mut need_alpha = false;
            for (k, st) in v.as_array().into_iter().flatten().enumerate() {
                let off = st.get(0).and_then(f).unwrap_or(0.0);
                let col = st.get(1).and_then(|x| compile_color(x, &format!("{path}/{k}/1"), &mut c.issues)).map(|x| x.0).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
                cols.extend([off, col[0], col[1], col[2]]);
                alphas.extend([off, col[3]]);
                if col[3] != 1.0 {
                    need_alpha = true;
                }
            }
            let count = cols.len() / 4;
            if need_alpha {
                cols.extend(alphas);
            }
            (count, cols)
        };
        let (count, prop) = if let Some(keys) = stops.get("keys").and_then(Value::as_array) {
            let mut count = 0;
            let mut nk = Vec::new();
            for (j, k) in keys.iter().enumerate() {
                let (c, v) = flat(self, k.get("v").unwrap_or(&Value::Null), &format!("{p}/stops/keys/{j}/v"));
                count = c;
                let mut key = k.clone();
                key["v"] = Value::Array(v.into_iter().map(n).collect());
                nk.push(key);
            }
            (count, self.prop(&json!({"keys": nk}), Val::Plain, &format!("{p}/stops")))
        } else {
            let (c, v) = flat(self, &stops, &format!("{p}/stops"));
            (c, json!({"a": 0, "k": v.into_iter().map(n).collect::<Vec<_>>()}))
        };
        so.insert("g".into(), json!({"p": count, "k": prop}));
    }
}

fn size_of(v: Option<&Value>) -> (f64, f64) {
    v.and_then(Value::as_array)
        .map(|a| (a.first().and_then(f).unwrap_or(0.0), a.get(1).and_then(f).unwrap_or(0.0)))
        .unwrap_or((0.0, 0.0))
}

/// Multiply every number of a (possibly keyed) value by `k`, keys' `t` excluded.
pub(crate) fn scale_values(v: &Value, k: f64) -> Value {
    match v {
        Value::Number(x) => n(x.as_f64().unwrap_or(0.0) * k),
        Value::Array(a) => Value::Array(a.iter().map(|x| scale_values(x, k)).collect()),
        Value::Object(o) if o.contains_key("keys") => {
            let keys: Vec<Value> = o["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|key| {
                    let mut key = key.clone();
                    if let Some(v) = key.get("v").cloned() {
                        key["v"] = scale_values(&v, k);
                    }
                    key
                })
                .collect();
            json!({"keys": keys})
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_round_trips() {
        let d = "M0 0 L10 0 C10 5 5 10 0 10 Z";
        let b = d_to_bez(d).unwrap();
        assert_eq!(b.len(), 1);
        assert!(b[0].closed);
        let mut out = String::new();
        bez_to_d(&b[0], &mut out);
        assert_eq!(out, d);
    }

    #[test]
    fn closing_curve_merges_into_the_first_vertex() {
        let b = d_to_bez("M0 0 C5 -5 10 -5 10 0 C10 5 5 5 0 0 Z").unwrap();
        assert_eq!(b[0].v.len(), 2);
        let mut out = String::new();
        bez_to_d(&b[0], &mut out);
        assert_eq!(out, "M0 0 C5 -5 10 -5 10 0 C10 5 5 5 0 0 Z");
    }

    #[test]
    fn relative_and_shorthand_commands() {
        let b = d_to_bez("m10 10 h10 v10 h-10 z").unwrap();
        assert_eq!(b[0].v, vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]]);
        let arc = d_to_bez("M0 0 A10 10 0 0 1 20 0").unwrap();
        assert!(arc[0].v.len() >= 3);
        assert!((arc[0].v.last().unwrap()[0] - 20.0).abs() < 1e-9);
    }

    #[test]
    fn hex_colours_are_exact() {
        let c = parse_hex("#ff8000").unwrap();
        assert_eq!(format_hex(&c).as_deref(), Some("#ff8000"));
        assert_eq!(format_hex(&[0.3, 0.3, 0.3, 1.0]), None);
    }
}
