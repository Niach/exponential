//! Text layers: the document (keyframed text, font, size, fill, stroke,
//! justification, tracking, line height, baseline shift, caps, boxed text),
//! animators with range selectors, glyphs from the file's `chars` or
//! registered fonts, emitted as paths so every painter shows the same glyphs.
//!
//! The layout and animator math is a port of lottie-web (MIT):
//! `TextProperty.completeTextData` (letters, line widths, anchor grouping,
//! box wrapping), `TextAnimatorProperty.getMeasures` (the per-letter matrix,
//! opacity and colours) and `TextSelectorProperty.getMult` (range selectors).
//! Glyphs paint with the DOCUMENT fill / stroke like lottie-web's glyph mode.

mod font;
mod layout;

pub use font::FontBook;

use crate::anim::Prop;
use crate::geom::Affine;
use crate::import::{num, string, Importer, Kind};
use crate::model::{Animation, LayerKind};
use crate::render::Emitter;
use serde_json::Value;

/// A text layer's data (`t`).
#[derive(Clone, Debug, Default)]
pub struct TextData {
    /// Document keyframes (`t.d.k`), held until the next one: (frame, document).
    pub(crate) docs: Vec<(f64, Doc)>,
    /// Animators (`t.a`), applied in order.
    pub(crate) animators: Vec<Animator>,
    /// Anchor point grouping (`t.m.g`): 1 characters, 2 words, 3 lines, 4 all.
    pub(crate) grouping: u8,
    /// Grouping alignment (`t.m.a`), percent of the letter box.
    pub(crate) alignment: Option<Prop>,
}

/// A text document (one `t.d.k[].s`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Doc {
    pub text: String,
    pub font: String,
    pub size: f64,
    pub fill: Option<[f64; 3]>,
    pub stroke: Option<[f64; 3]>,
    pub stroke_width: f64,
    /// `of`: stroke painted over the fill.
    pub stroke_over_fill: bool,
    /// `j`: 0 left, 1 right, 2 centre (3..6 justify: drawn left, like lottie-web).
    pub justify: u8,
    /// `tr`, thousandths of an em.
    pub tracking: f64,
    /// `lh`, 0 = 1.2 × size.
    pub line_height: f64,
    /// `ls`, baseline shift (up).
    pub baseline_shift: f64,
    /// `ca`: 0 none, 1 all caps, 2 small caps.
    pub caps: u8,
    /// `sz`: box size (boxed text wraps at its width).
    pub box_size: Option<[f64; 2]>,
    /// `ps`: box top-left.
    pub box_pos: Option<[f64; 2]>,
}

/// A range selector (`t.a[].s`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Selector {
    pub start: Prop,
    pub end: Option<Prop>,
    pub offset: Prop,
    /// `r`: 1 percent, 2 index.
    pub units: u8,
    /// `b`: 1 characters, 2 characters excluding spaces, 3 words, 4 lines.
    pub based: u8,
    /// `sh`: 1 square, 2 ramp up, 3 ramp down, 4 triangle, 5 round, 6 smooth.
    pub shape: u8,
    /// `xe` / `ne`: ease high / low (percent).
    pub ease_high: Prop,
    pub ease_low: Prop,
    /// `a`: amount (percent).
    pub amount: Prop,
    /// `sm`: smoothness (percent).
    pub smooth: Prop,
    /// `rn`: randomize order (deterministic here).
    pub randomize: bool,
}

/// An animator's properties (`t.a[].a`), each optional.
#[derive(Clone, Debug, Default)]
pub(crate) struct AnimProps {
    pub position: Option<Prop>,
    pub anchor: Option<Prop>,
    pub scale: Option<Prop>,
    pub rotation: Option<Prop>,
    pub rotation_x: Option<Prop>,
    pub rotation_y: Option<Prop>,
    pub skew: Option<Prop>,
    pub skew_axis: Option<Prop>,
    pub opacity: Option<Prop>,
    pub stroke_width: Option<Prop>,
    pub stroke_color: Option<Prop>,
    pub fill_color: Option<Prop>,
    pub fill_hue: Option<Prop>,
    pub fill_saturation: Option<Prop>,
    pub fill_brightness: Option<Prop>,
    pub tracking: Option<Prop>,
    /// lottie-android / lottie-ios extras: fill and stroke opacity (percent).
    pub fill_opacity: Option<Prop>,
    pub stroke_opacity: Option<Prop>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Animator {
    pub selector: Selector,
    pub props: AnimProps,
}

fn flag(v: &Value, k: &str, d: bool) -> bool {
    match v.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        _ => d,
    }
}

fn color(v: &Value, k: &str) -> Option<[f64; 3]> {
    let a = v.get(k)?.as_array()?;
    let mut c: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
    if c.len() < 3 {
        return None;
    }
    if c.iter().take(3).any(|x| *x > 1.0) {
        for x in c.iter_mut() {
            *x /= 255.0;
        }
    }
    Some([c[0], c[1], c[2]])
}

fn pair(v: &Value, k: &str) -> Option<[f64; 2]> {
    let a = v.get(k)?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

fn doc(s: &Value) -> Doc {
    let size = num(s, "s", 10.0);
    Doc {
        text: string(s, "t"),
        font: string(s, "f"),
        size,
        fill: color(s, "fc"),
        stroke: color(s, "sc"),
        stroke_width: num(s, "sw", 0.0),
        stroke_over_fill: flag(s, "of", true),
        justify: num(s, "j", 0.0) as u8,
        tracking: num(s, "tr", 0.0),
        line_height: num(s, "lh", 0.0),
        baseline_shift: num(s, "ls", 0.0),
        caps: num(s, "ca", 0.0) as u8,
        box_size: pair(s, "sz"),
        box_pos: pair(s, "ps"),
    }
}

pub(crate) fn import(im: &Importer<'_>, layer: &Value) -> Option<TextData> {
    let t = layer.get("t")?;
    let d = t.get("d")?;
    // A slotted text document (`t.d.sid`): the slot's value replaces it.
    let d = match d.get("sid").and_then(Value::as_str).and_then(|sid| im.slot(sid)) {
        Some(slot) => slot,
        None => d,
    };
    let keys = d.get("k").unwrap_or(d);
    let mut docs = Vec::new();
    match keys {
        Value::Array(ks) => {
            for k in ks {
                if let Some(s) = k.get("s") {
                    docs.push((num(k, "t", 0.0), doc(s)));
                }
            }
        }
        // A bare document (lenient).
        Value::Object(_) => docs.push((0.0, doc(keys))),
        _ => {}
    }
    if docs.is_empty() {
        return None;
    }
    let mut animators = Vec::new();
    for a in t.get("a").and_then(Value::as_array).into_iter().flatten() {
        let null = Value::Null;
        let s = a.get("s").unwrap_or(&null);
        let p = a.get("a").unwrap_or(&null);
        let pr = |k: &str, kind: Kind| im.prop_opt(p.get(k), kind);
        animators.push(Animator {
            selector: Selector {
                start: im.prop(s.get("s"), Kind::Plain, &[0.0]),
                end: im.prop_opt(s.get("e"), Kind::Plain),
                offset: im.prop(s.get("o"), Kind::Plain, &[0.0]),
                units: num(s, "r", 1.0) as u8,
                based: num(s, "b", 1.0) as u8,
                shape: num(s, "sh", 1.0) as u8,
                ease_high: im.prop(s.get("xe"), Kind::Plain, &[0.0]),
                ease_low: im.prop(s.get("ne"), Kind::Plain, &[0.0]),
                amount: im.prop(s.get("a"), Kind::Plain, &[100.0]),
                smooth: im.prop(s.get("sm"), Kind::Plain, &[100.0]),
                randomize: num(s, "rn", 0.0) == 1.0,
            },
            props: AnimProps {
                position: pr("p", Kind::Plain),
                anchor: pr("a", Kind::Plain),
                scale: pr("s", Kind::Plain),
                rotation: pr("r", Kind::Plain).or_else(|| pr("rz", Kind::Plain)),
                rotation_x: pr("rx", Kind::Plain),
                rotation_y: pr("ry", Kind::Plain),
                skew: pr("sk", Kind::Plain),
                skew_axis: pr("sa", Kind::Plain),
                opacity: pr("o", Kind::Plain),
                stroke_width: pr("sw", Kind::Plain),
                stroke_color: pr("sc", Kind::Color),
                fill_color: pr("fc", Kind::Color),
                fill_hue: pr("fh", Kind::Plain),
                fill_saturation: pr("fs", Kind::Plain),
                fill_brightness: pr("fb", Kind::Plain),
                tracking: pr("t", Kind::Plain),
                fill_opacity: pr("fo", Kind::Plain),
                stroke_opacity: pr("so", Kind::Plain),
            },
        });
    }
    let m = t.get("m");
    Some(TextData {
        docs,
        animators,
        grouping: m.map(|m| num(m, "g", 1.0)).unwrap_or(1.0) as u8,
        alignment: m.and_then(|m| im.prop_opt(m.get("a"), Kind::Plain)),
    })
}

/// Render a text layer at layer-local `frame` under the layer matrix `m`.
pub fn render(text: &TextData, anim: &Animation, frame: f64, m: &Affine, em: &mut Emitter) {
    layout::render(text, anim, frame, m, em);
}

/// True when the animation has text layers but no glyphs to draw them with:
/// no `chars` in the file and no font registered (`Motion::add_font`).
pub fn needs_fonts(anim: &Animation) -> bool {
    if !anim.chars.is_empty() || !anim.font_book.is_empty() {
        return false;
    }
    let has_text = |layers: &[crate::model::Layer]| layers.iter().any(|l| matches!(l.kind, LayerKind::Text(_)));
    has_text(&anim.layers) || anim.precomps.values().any(|l| has_text(l))
}

#[cfg(test)]
mod tests;
