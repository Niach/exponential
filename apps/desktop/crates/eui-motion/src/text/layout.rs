//! Letter layout and the per-letter animator pass (lottie-web's
//! `completeTextData` + `getMeasures`), then glyph emission.

use super::font::FontFace;
use super::{Animator, Doc, Selector, TextData};
use crate::anim::cubic_ease;
use crate::display::{Brush, Cap, FillRule, Join, Op, StrokeStyle, Verb};
use crate::geom::{self, Affine, BPath};
use crate::model::{Animation, Char, Item};
use crate::render::Emitter;
use crate::shapes::{compose, transform_matrix};
use ttf_parser::GlyphId;

const NEWLINE: &str = "\r";

/// Where a letter's outline comes from.
#[derive(Clone, Copy, Debug)]
enum Glyph {
    None,
    Char(usize),
    Font(GlyphId),
}

#[derive(Clone, Debug)]
struct Letter {
    /// The letter ("" for a line break).
    val: String,
    /// Advance (with kerning to the next letter on the line).
    l: f64,
    /// Anchor-group width.
    an: f64,
    /// Offset of the letter in its anchor group.
    add: f64,
    n: bool,
    line: usize,
    ind: usize,
    extra: f64,
    an_indexes: Vec<usize>,
    animator_justify_offset: f64,
    glyph: Glyph,
    /// Small caps scale.
    scale: f64,
}

/// The glyph sources of one layout.
struct Glyphs<'a> {
    chars: &'a [Char],
    family: String,
    style: String,
    face: Option<FontFace<'a>>,
}

impl Glyphs<'_> {
    fn glyph(&self, ch: &str) -> Glyph {
        if !self.chars.is_empty() {
            let exact = self.chars.iter().position(|c| c.ch == ch && c.style == self.style && c.family == self.family);
            if let Some(i) = exact.or_else(|| self.chars.iter().position(|c| c.ch == ch && c.family == self.family)) {
                return Glyph::Char(i);
            }
        }
        match (&self.face, ch.chars().next()) {
            (Some(f), Some(c)) => f.glyph(c).map(Glyph::Font).unwrap_or(Glyph::None),
            _ => Glyph::None,
        }
    }

    /// Advance in ems (chars: `w` / 100).
    fn advance(&self, g: Glyph) -> f64 {
        match g {
            Glyph::None => 0.0,
            Glyph::Char(i) => self.chars[i].width / 100.0,
            Glyph::Font(id) => self.face.as_ref().map(|f| f.advance(id)).unwrap_or(0.0),
        }
    }

    fn kern(&self, a: Glyph, b: Glyph) -> f64 {
        match (a, b, &self.face) {
            (Glyph::Font(x), Glyph::Font(y), Some(f)) => f.kern(x, y),
            _ => 0.0,
        }
    }
}

/// lottie-web `buildFinalText`, simplified: combining marks, variation
/// selectors, skin-tone modifiers and ZWJ sequences stay on the previous
/// letter; `\r\n`, `\n` and U+0003 become line breaks (as lottie-ios/android).
fn split(text: &str, caps: u8) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut join_next = false;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' || c == '\n' || c == '\u{3}' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            out.push((NEWLINE.into(), 1.0));
            join_next = false;
            continue;
        }
        let cp = c as u32;
        // Combining marks, variation selectors, skin tones, tag sequences, Devanagari signs.
        let combining = matches!(
            cp,
            0x300..=0x36F
                | 0x1AB0..=0x1AFF
                | 0x20D0..=0x20FF
                | 0xFE20..=0xFE2F
                | 0xFE00..=0xFE0F
                | 0x1F3FB..=0x1F3FF
                | 0xE0020..=0xE007F
                | 0x900..=0x903
                | 0x93A..=0x94F
                | 0x951..=0x957
                | 0x962
                | 0x963
        );
        let zwj = cp == 0x200D;
        if (combining || zwj || join_next) && out.last().is_some_and(|l| l.0 != NEWLINE) {
            out.last_mut().unwrap().0.push(c);
            join_next = zwj;
            continue;
        }
        join_next = false;
        match caps {
            1 => out.push((c.to_uppercase().collect(), 1.0)),
            2 if c.is_lowercase() => out.push((c.to_uppercase().collect(), 0.7)),
            _ => out.push((c.to_string(), 1.0)),
        }
    }
    out
}

struct Layout {
    letters: Vec<Letter>,
    line_widths: Vec<f64>,
    box_width: f64,
    justify_offset: f64,
    y_offset: f64,
    ascent: f64,
    size: f64,
    total_chars: Vec<usize>,
}

fn layout(text: &TextData, d: &Doc, g: &Glyphs<'_>, ascent_pct: f64) -> Layout {
    let size = d.size;
    let tracking = d.tracking / 1000.0 * size;
    let mut final_text = split(&d.text, d.caps);
    let glyph_of = |s: &str| if s == NEWLINE { Glyph::None } else { g.glyph(s) };
    // Advance of letter `i` given its successor (kerning stays on the line).
    let measure = |ft: &[(String, f64)], glyphs: &[Glyph], i: usize| -> f64 {
        if ft[i].0 == NEWLINE {
            return 0.0;
        }
        let mut w = g.advance(glyphs[i]);
        if let Some(next) = ft.get(i + 1) {
            if next.0 != NEWLINE && next.1 == ft[i].1 {
                w += g.kern(glyphs[i], glyphs[i + 1]);
            }
        }
        w * size * ft[i].1
    };
    let mut glyphs: Vec<Glyph> = final_text.iter().map(|(s, _)| glyph_of(s)).collect();
    if let Some([box_w, _]) = d.box_size {
        // Word wrap at the box width (lottie-web; no font shrinking).
        let mut line_width = 0.0;
        let mut last_space: Option<usize> = None;
        let mut i = 0;
        while i < final_text.len() {
            let is_space = final_text[i].0 == " ";
            let mut new_line = false;
            if is_space {
                last_space = Some(i);
            } else if final_text[i].0 == NEWLINE {
                line_width = 0.0;
                new_line = true;
                last_space = None;
            }
            let c = if new_line { 0.0 } else { measure(&final_text, &glyphs, i) };
            if line_width + c > box_w && !is_space && line_width > 0.0 {
                match last_space {
                    Some(s) => {
                        final_text[s] = (NEWLINE.into(), 1.0);
                        glyphs[s] = Glyph::None;
                        i = s;
                    }
                    None => {
                        final_text.insert(i, (NEWLINE.into(), 1.0));
                        glyphs.insert(i, Glyph::None);
                    }
                }
                last_space = None;
                line_width = 0.0;
            } else {
                line_width += c + tracking;
            }
            i += 1;
        }
    }
    let len = final_text.len();
    let mut letters: Vec<Letter> = Vec::with_capacity(len);
    let mut line_widths = Vec::new();
    let mut line_width = -tracking;
    let mut max_line_width: f64 = 0.0;
    let mut current_line = 0;
    let mut current_size = 0.0;
    let mut current_pos = 0;
    let mut index = 0;
    let mut uncollapsed = 0.0;
    for i in 0..len {
        let (cur, scale) = &final_text[i];
        let new_line = cur == NEWLINE;
        let val = if new_line {
            uncollapsed = 0.0;
            line_widths.push(line_width);
            max_line_width = max_line_width.max(line_width);
            line_width = -2.0 * tracking;
            current_line += 1;
            String::new()
        } else {
            cur.clone()
        };
        let c = if new_line { 0.0 } else { measure(&final_text, &glyphs, i) };
        if cur == " " {
            uncollapsed += c + tracking;
        } else {
            line_width += c + tracking + uncollapsed;
            uncollapsed = 0.0;
        }
        letters.push(Letter {
            val: val.clone(),
            l: c,
            an: c,
            add: current_size,
            n: new_line,
            line: current_line,
            ind: 0,
            extra: 0.0,
            an_indexes: Vec::new(),
            animator_justify_offset: 0.0,
            glyph: glyphs[i],
            scale: *scale,
        });
        match text.grouping {
            2 => {
                current_size += c;
                if val.is_empty() || val == " " || i == len - 1 {
                    if val.is_empty() || val == " " {
                        current_size -= c;
                    }
                    while current_pos <= i {
                        letters[current_pos].an = current_size;
                        letters[current_pos].ind = index;
                        letters[current_pos].extra = c;
                        current_pos += 1;
                    }
                    index += 1;
                    current_size = 0.0;
                }
            }
            3 => {
                current_size += c;
                if val.is_empty() || i == len - 1 {
                    if val.is_empty() {
                        current_size -= c;
                    }
                    while current_pos <= i {
                        letters[current_pos].an = current_size;
                        letters[current_pos].ind = index;
                        letters[current_pos].extra = c;
                        current_pos += 1;
                    }
                    current_size = 0.0;
                    index += 1;
                }
            }
            _ => {
                letters[index].ind = index;
                letters[index].extra = 0.0;
                index += 1;
            }
        }
    }
    max_line_width = max_line_width.max(line_width);
    line_widths.push(line_width);
    let (box_width, justify_offset) = match d.box_size {
        Some([w, _]) => (w, 0.0),
        None => (
            max_line_width,
            match d.justify {
                1 => -max_line_width,
                2 => -max_line_width / 2.0,
                _ => 0.0,
            },
        ),
    };
    // Selector indices per animator.
    let mut total_chars = Vec::with_capacity(text.animators.len());
    for (j, a) in text.animators.iter().enumerate() {
        let based = a.selector.based;
        let mut ind = 0;
        let mut indexes = Vec::new();
        for i in 0..len {
            let l = &mut letters[i];
            l.an_indexes.push(ind);
            let counts = match based {
                2 => !l.val.is_empty() && l.val != " ",
                3 => l.n || l.val == " " || i == len - 1,
                4 => l.n || i == len - 1,
                _ => !l.val.is_empty(),
            };
            if counts {
                if a.selector.randomize {
                    indexes.push(ind);
                }
                ind += 1;
            }
        }
        total_chars.push(ind);
        if a.selector.randomize && !indexes.is_empty() {
            // lottie-web draws with Math.random; a fixed seed keeps every
            // platform (and every frame) identical.
            let mut seed: u64 = 0x9E37_79B9_7F4A_7C15 ^ ((len as u64) << 8) ^ j as u64;
            let mut current: Option<usize> = None;
            let mut new_ind = 0;
            for l in letters.iter_mut() {
                if current != Some(l.an_indexes[j]) {
                    current = Some(l.an_indexes[j]);
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    if !indexes.is_empty() {
                        let k = ((seed >> 33) as usize) % indexes.len();
                        new_ind = indexes.remove(k);
                    }
                }
                l.an_indexes[j] = new_ind;
            }
        }
    }
    let line_height = if d.line_height != 0.0 { d.line_height } else { size * 1.2 };
    Layout {
        letters,
        line_widths,
        box_width,
        justify_offset,
        y_offset: line_height,
        ascent: ascent_pct * size / 100.0,
        size,
        total_chars,
    }
}

/// A selector evaluated at a frame (`finalS` / `finalE` + easing).
struct Sel<'a> {
    sel: &'a Selector,
    s: f64,
    e: f64,
    ease: [f64; 4],
    smooth: f64,
    amount: f64,
}

impl<'a> Sel<'a> {
    fn new(sel: &'a Selector, frame: f64, total: usize, text_len: usize) -> Self {
        let index_units = sel.units == 2;
        let divisor = if index_units { 1.0 } else { 100.0 / total as f64 };
        let o = sel.offset.scalar_at(frame) / divisor;
        let e_raw = match &sel.end {
            Some(e) => e.scalar_at(frame),
            None if index_units => text_len as f64,
            None => 100.0,
        };
        let mut s = sel.start.scalar_at(frame) / divisor + o;
        let mut e = e_raw / divisor + o;
        if s > e {
            std::mem::swap(&mut s, &mut e);
        }
        let (ne, xe) = (sel.ease_low.scalar_at(frame), sel.ease_high.scalar_at(frame));
        let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 1.0, 1.0);
        if ne > 0.0 {
            x1 = ne / 100.0;
        } else {
            y1 = -ne / 100.0;
        }
        if xe > 0.0 {
            x2 = 1.0 - xe / 100.0;
        } else {
            y2 = 1.0 + xe / 100.0;
        }
        Self {
            sel,
            s,
            e,
            ease: [x1, y1, x2, y2],
            smooth: sel.smooth.scalar_at(frame),
            amount: sel.amount.scalar_at(frame) / 100.0,
        }
    }

    /// lottie-web `getMult`.
    fn mult(&self, ind: usize) -> f64 {
        let [x1, y1, x2, y2] = self.ease;
        let easer = |x: f64| cubic_ease(x1, y1, x2, y2, x);
        let (s, e) = (self.s, self.e);
        let mut ind = ind as f64;
        let ramp = |ind: f64| (0.5 / (e - s) + (ind - s) / (e - s)).clamp(0.0, 1.0);
        let mut mult = match self.sel.shape {
            2 => easer(if e == s { (ind >= e) as u8 as f64 } else { ramp(ind) }),
            3 => easer(if e == s { (ind < e) as u8 as f64 } else { 1.0 - ramp(ind) }),
            4 => easer(if e == s {
                0.0
            } else {
                let m = ramp(ind);
                if m < 0.5 {
                    m * 2.0
                } else {
                    1.0 - 2.0 * (m - 0.5)
                }
            }),
            5 => easer(if e == s {
                0.0
            } else {
                let tot = e - s;
                ind = (ind + 0.5 - s).max(0.0).min(e - s);
                let x = -tot / 2.0 + ind;
                let a = tot / 2.0;
                (1.0 - (x * x) / (a * a)).max(0.0).sqrt()
            }),
            6 => easer(if e == s {
                0.0
            } else {
                ind = (ind + 0.5 - s).max(0.0).min(e - s);
                (1.0 + (std::f64::consts::PI + std::f64::consts::PI * 2.0 * ind / (e - s)).cos()) / 2.0
            }),
            _ => {
                let mut m = 0.0;
                if ind >= s.floor() {
                    m = if ind - s < 0.0 {
                        (e.min(1.0) - (s - ind)).min(1.0).max(0.0)
                    } else {
                        (e - ind).min(1.0).max(0.0)
                    };
                }
                easer(m)
            }
        };
        if self.smooth != 100.0 {
            let mut sm = self.smooth * 0.01;
            if sm == 0.0 {
                sm = 1e-8;
            }
            let threshold = 0.5 - sm * 0.5;
            mult = if mult < threshold { 0.0 } else { ((mult - threshold) / sm).min(1.0) };
        }
        mult * self.amount
    }
}

/// `then` in lottie-web's call order: `op` applies after `m`.
fn post(m: &mut Affine, op: Affine) {
    *m = op.then(m);
}

fn rgb_to_hsv(c: [f64; 3]) -> [f64; 3] {
    let (r, g, b) = (c[0] * 255.0, c[1] * 255.0, c[2] * 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max };
    let v = max / 255.0;
    let h = if max == min {
        0.0
    } else if max == r {
        ((g - b) + d * if g < b { 6.0 } else { 0.0 }) / (6.0 * d)
    } else if max == g {
        ((b - r) + d * 2.0) / (6.0 * d)
    } else {
        ((r - g) + d * 4.0) / (6.0 * d)
    };
    [h, s, v]
}

fn hsv_to_rgb([h, s, v]: [f64; 3]) -> [f64; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match (i as i64).rem_euclid(6) {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn add_hue(c: [f64; 3], deg: f64) -> [f64; 3] {
    let mut hsv = rgb_to_hsv(c);
    hsv[0] += deg / 360.0;
    if hsv[0] > 1.0 {
        hsv[0] -= 1.0;
    } else if hsv[0] < 0.0 {
        hsv[0] += 1.0;
    }
    hsv_to_rgb(hsv)
}

fn add_hsv(c: [f64; 3], k: usize, off: f64) -> [f64; 3] {
    let mut hsv = rgb_to_hsv(c);
    hsv[k] = (hsv[k] + off).clamp(0.0, 1.0);
    hsv_to_rgb(hsv)
}

/// One laid-out, animated letter ready to paint.
struct Placed {
    glyph: Glyph,
    scale: f64,
    m: Affine,
    opacity: f64,
    fill: Option<[f64; 4]>,
    stroke: Option<([f64; 4], f64)>,
}

fn place(text: &TextData, d: &Doc, lay: &mut Layout, frame: f64) -> Vec<Placed> {
    let alignment = text.alignment.as_ref().map(|a| a.vec2_at(frame)).unwrap_or([0.0, 0.0]);
    let len = lay.letters.len();
    let sels: Vec<Sel<'_>> = text
        .animators
        .iter()
        .zip(&lay.total_chars)
        .map(|(a, &t)| Sel::new(&a.selector, frame, t, len))
        .collect();
    let anims: &[Animator] = &text.animators;
    let at = |p: &Option<crate::anim::Prop>| p.as_ref().map(|p| p.eval(frame));
    // Animated values once per frame.
    struct Vals {
        p: Option<Vec<f64>>,
        a: Option<Vec<f64>>,
        s: Option<Vec<f64>>,
        r: Option<f64>,
        rx: Option<f64>,
        ry: Option<f64>,
        sk: Option<f64>,
        sa: f64,
        o: Option<f64>,
        sw: Option<f64>,
        sc: Option<Vec<f64>>,
        fc: Option<Vec<f64>>,
        fh: Option<f64>,
        fs: Option<f64>,
        fb: Option<f64>,
        t: Option<f64>,
        fo: Option<f64>,
        so: Option<f64>,
    }
    let sc = |p: &Option<crate::anim::Prop>| p.as_ref().map(|p| p.scalar_at(frame));
    let vals: Vec<Vals> = anims
        .iter()
        .map(|a| {
            let p = &a.props;
            Vals {
                p: at(&p.position),
                a: at(&p.anchor),
                s: at(&p.scale),
                r: sc(&p.rotation),
                rx: sc(&p.rotation_x),
                ry: sc(&p.rotation_y),
                sk: sc(&p.skew),
                sa: sc(&p.skew_axis).unwrap_or(0.0),
                o: sc(&p.opacity),
                sw: sc(&p.stroke_width),
                sc: at(&p.stroke_color),
                fc: at(&p.fill_color),
                fh: sc(&p.fill_hue),
                fs: sc(&p.fill_saturation),
                fb: sc(&p.fill_brightness),
                t: sc(&p.tracking),
                fo: sc(&p.fill_opacity),
                so: sc(&p.stroke_opacity),
            }
        })
        .collect();
    let v2 = |v: &[f64], k: usize, d: f64| v.get(k).copied().unwrap_or(d);
    let stroke_color_anim = vals.iter().any(|v| v.sc.is_some());
    let stroke_width_anim = vals.iter().any(|v| v.sw.is_some());
    // Tracking animators shift right / centre justified lines.
    let letters = &mut lay.letters;
    if d.justify == 1 || d.justify == 2 {
        let mult_j = if d.justify == 2 { -0.5 } else { -1.0 };
        let mut off = 0.0;
        let mut first = 0.0;
        let mut last = 0;
        let mut new_line = true;
        for i in 0..len {
            if letters[i].n {
                if off != 0.0 {
                    off += first;
                }
                while last < i {
                    letters[last].animator_justify_offset = off;
                    last += 1;
                }
                off = 0.0;
                new_line = true;
            } else {
                for (j, v) in vals.iter().enumerate() {
                    if let Some(t) = v.t {
                        if new_line && d.justify == 2 {
                            first += t * mult_j;
                        }
                        off += t * sels[j].mult(letters[i].an_indexes[j]) * mult_j;
                    }
                }
                new_line = false;
            }
        }
        if off != 0.0 {
            off += first;
        }
        while last < len {
            letters[last].animator_justify_offset = off;
            last += 1;
        }
    }
    let y_off = lay.size * 1.2 * 0.714;
    let (mut x_pos, mut y_pos) = (0.0, 0.0);
    let mut first_line = true;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let l = &letters[i];
        if l.n {
            x_pos = 0.0;
            y_pos += lay.y_offset;
            y_pos += if first_line { 1.0 } else { 0.0 };
            first_line = false;
            continue;
        }
        let mults: Vec<f64> = sels.iter().enumerate().map(|(j, s)| s.mult(l.an_indexes[j])).collect();
        let mut m = Affine::IDENTITY;
        let offf = l.an / 2.0 - l.add;
        post(&mut m, Affine::translate(-offf, 0.0));
        post(&mut m, Affine::translate(-alignment[0] * l.an * 0.005, -alignment[1] * y_off * 0.01));
        for (j, v) in vals.iter().enumerate() {
            if let Some(t) = v.t {
                if x_pos != 0.0 || d.justify != 0 {
                    x_pos += t * mults[j];
                }
            }
        }
        let mut sw = d.stroke_width;
        let mut sc = d.stroke.unwrap_or([0.0, 0.0, 0.0]);
        let mut fc = d.fill.unwrap_or([0.0, 0.0, 0.0]);
        let (mut fo, mut so) = (1.0, 1.0);
        for (j, v) in vals.iter().enumerate() {
            if let Some(a) = &v.a {
                post(&mut m, Affine::translate(-v2(a, 0, 0.0) * mults[j], -v2(a, 1, 0.0) * mults[j]));
            }
        }
        for (j, v) in vals.iter().enumerate() {
            if let Some(s) = &v.s {
                let (sx, sy) = (v2(s, 0, 100.0) / 100.0, v2(s, 1, 100.0) / 100.0);
                post(&mut m, Affine::scale(1.0 + (sx - 1.0) * mults[j], 1.0 + (sy - 1.0) * mults[j]));
            }
        }
        let mut opacity = 1.0;
        for (j, v) in vals.iter().enumerate() {
            let mult = mults[j];
            if let Some(sk) = v.sk {
                post(&mut m, compose([0.0; 2], [0.0; 2], [100.0; 2], 0.0, sk * mult, v.sa * mult));
            }
            if let Some(r) = v.r {
                post(&mut m, Affine::rotate_deg(r * mult));
            }
            // 3D rotations, flattened like lottie-web's 2D projection.
            if let Some(ry) = v.ry {
                post(&mut m, Affine::scale((ry * mult).to_radians().cos(), 1.0));
            }
            if let Some(rx) = v.rx {
                post(&mut m, Affine::scale(1.0, (rx * mult).to_radians().cos()));
            }
            if let Some(o) = v.o {
                opacity += (o / 100.0 * mult - opacity) * mult;
            }
            if stroke_width_anim {
                if let Some(w) = v.sw {
                    sw += w * mult;
                }
            }
            if let Some(c) = &v.sc {
                for k in 0..3 {
                    sc[k] += (v2(c, k, 0.0) - sc[k]) * mult;
                }
            }
            if d.fill.is_some() {
                if let Some(c) = &v.fc {
                    for k in 0..3 {
                        fc[k] += (v2(c, k, 0.0) - fc[k]) * mult;
                    }
                }
                if let Some(h) = v.fh {
                    fc = add_hue(fc, h * mult);
                }
                if let Some(s) = v.fs {
                    fc = add_hsv(fc, 1, s / 100.0 * mult);
                }
                if let Some(b) = v.fb {
                    fc = add_hsv(fc, 2, b / 100.0 * mult);
                }
            }
            if let Some(f) = v.fo {
                fo += (f / 100.0 - fo) * mult;
            }
            if let Some(s) = v.so {
                so += (s / 100.0 - so) * mult;
            }
        }
        for (j, v) in vals.iter().enumerate() {
            if let Some(p) = &v.p {
                post(&mut m, Affine::translate(v2(p, 0, 0.0) * mults[j], v2(p, 1, 0.0) * mults[j]));
            }
        }
        post(&mut m, Affine::translate(x_pos, y_pos));
        if let Some(ps) = d.box_pos {
            post(&mut m, Affine::translate(ps[0], ps[1] + lay.ascent));
        }
        let line_w = lay.line_widths.get(l.line).copied().unwrap_or(0.0);
        match d.justify {
            1 => post(
                &mut m,
                Affine::translate(l.animator_justify_offset + lay.justify_offset + (lay.box_width - line_w), 0.0),
            ),
            2 => post(
                &mut m,
                Affine::translate(l.animator_justify_offset + lay.justify_offset + (lay.box_width - line_w) / 2.0, 0.0),
            ),
            _ => {}
        }
        post(&mut m, Affine::translate(0.0, -d.baseline_shift));
        post(&mut m, Affine::translate(offf, 0.0));
        post(&mut m, Affine::translate(alignment[0] * l.an * 0.005, alignment[1] * y_off * 0.01));
        x_pos += l.l + d.tracking * 0.001 * lay.size;
        let fill = d.fill.map(|_| [fc[0], fc[1], fc[2], fo]);
        let stroke = if d.stroke.is_some() || stroke_color_anim {
            // No document stroke but an animated colour: SVG's default width 1.
            let w = if stroke_width_anim || d.stroke.is_some() { sw.max(0.0) } else { 1.0 };
            Some(([sc[0], sc[1], sc[2], so], w))
        } else {
            None
        };
        out.push(Placed { glyph: l.glyph, scale: l.scale, m, opacity, fill, stroke });
    }
    out
}

/// A `chars` glyph's paths in letter space at `size`: the first group's
/// scale becomes the size (lottie-web `buildShapeData`).
fn char_paths(c: &Char, size: f64, frame: f64, out: &mut Vec<Verb>, pre: &Affine) {
    for (k, item) in c.shapes.iter().enumerate() {
        match item {
            Item::Group { items, transform } if k == 0 => {
                let m = match transform {
                    Some(t) => {
                        let mut t = t.clone();
                        t.scale = Some(crate::anim::Prop::fixed(vec![size, size]));
                        transform_matrix(&t, frame)
                    }
                    None => Affine::scale(size / 100.0, size / 100.0),
                };
                collect(items, frame, &pre.then(&m), out);
            }
            other => collect(std::slice::from_ref(other), frame, &pre.then(&Affine::scale(size / 100.0, size / 100.0)), out),
        }
    }
}

fn collect(items: &[Item], frame: f64, xf: &Affine, out: &mut Vec<Verb>) {
    for item in items {
        let path = match item {
            Item::Group { items, transform } => {
                let m = transform.as_ref().map(|t| transform_matrix(t, frame)).unwrap_or(Affine::IDENTITY);
                collect(items, frame, &xf.then(&m), out);
                continue;
            }
            Item::Path { shape, .. } => BPath::from_flat(&shape.eval(frame)),
            Item::Rect { position, size, roundness, reversed } => geom::rect(
                position.vec2_at(frame),
                size.vec2_at(frame),
                roundness.scalar_at(frame),
                !reversed,
            ),
            Item::Ellipse { position, size, reversed } => {
                geom::ellipse(position.vec2_at(frame), size.vec2_at(frame), !reversed)
            }
            _ => continue,
        };
        path.to_verbs(xf, out);
    }
}

pub(super) fn render(text: &TextData, anim: &Animation, frame: f64, layer_m: &Affine, em: &mut Emitter) {
    // The document holding at `frame` (layer-local).
    let mut di = 0;
    while di + 1 < text.docs.len() && text.docs[di + 1].0 <= frame {
        di += 1;
    }
    let Some((_, d)) = text.docs.get(di) else { return };
    // lottie-web `getFontByName`: by name, else the first font.
    let font = anim.fonts.iter().find(|f| f.name == d.font).or(anim.fonts.first());
    let (family, style) = font.map(|f| (f.family.clone(), f.style.clone())).unwrap_or_default();
    let book_font = anim.font_book.find(&d.font, &family, &style);
    let face = book_font.and_then(|i| anim.font_book.face(i));
    if anim.chars.is_empty() && face.is_none() {
        return;
    }
    let ascent = match (font, book_font) {
        (Some(f), _) => f.ascent,
        (None, Some(i)) => anim.font_book.ascent_percent(i),
        _ => 75.0,
    };
    let upem = face.as_ref().map(|f| f.units_per_em).unwrap_or(1000.0);
    let glyphs = Glyphs { chars: &anim.chars, family, style, face };
    let mut lay = layout(text, d, &glyphs, ascent);
    let placed = place(text, d, &mut lay, frame);
    let mut verbs = Vec::new();
    for p in placed {
        if p.opacity <= 0.0 {
            continue;
        }
        verbs.clear();
        let size = lay.size * p.scale;
        match p.glyph {
            Glyph::None => continue,
            Glyph::Char(i) => char_paths(&anim.chars[i], size, frame, &mut verbs, &Affine::IDENTITY),
            Glyph::Font(g) => {
                if let Some(f) = &glyphs.face {
                    let k = size / upem;
                    let m = Affine::scale(k, -k);
                    for path in f.outline(g).iter() {
                        path.to_verbs(&m, &mut verbs);
                    }
                }
            }
        }
        if verbs.is_empty() {
            continue;
        }
        let m = layer_m.then(&p.m);
        if !m.is_finite() {
            continue;
        }
        let fill = p.fill.filter(|c| c[3] > 0.0);
        let stroke = p.stroke.filter(|(c, w)| c[3] > 0.0 && *w > 0.0);
        let both = fill.is_some() && stroke.is_some();
        // Letter opacity is a group opacity: isolate when fill and stroke overlap.
        let isolate = both && p.opacity < 1.0;
        let alpha = if isolate { 1.0 } else { p.opacity };
        if isolate {
            em.begin_layer(crate::display::Blend::Normal, p.opacity as f32, Vec::new());
        }
        let rgba = |c: [f64; 4]| {
            [
                c[0].clamp(0.0, 1.0) as f32,
                c[1].clamp(0.0, 1.0) as f32,
                c[2].clamp(0.0, 1.0) as f32,
                (c[3].clamp(0.0, 1.0) * alpha) as f32,
            ]
        };
        let fill_op = fill.map(|c| Op::Fill {
            transform: m.to_dl(),
            rule: FillRule::Nonzero,
            brush: Brush::Solid { color: rgba(c) },
            path: verbs.clone(),
        });
        let stroke_op = stroke.map(|(c, w)| Op::Stroke {
            transform: m.to_dl(),
            style: StrokeStyle {
                width: w as f32,
                cap: Cap::Butt,
                join: Join::Round,
                miter: 4.0,
                dashes: Vec::new(),
                dash_offset: 0.0,
            },
            brush: Brush::Solid { color: rgba(c) },
            path: verbs.clone(),
        });
        let (first, second) = if d.stroke_over_fill { (fill_op, stroke_op) } else { (stroke_op, fill_op) };
        for op in [first, second].into_iter().flatten() {
            em.push(op);
        }
        if isolate {
            em.pop();
        }
    }
}
