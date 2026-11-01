//! Lottie JSON → [`Animation`]. Lenient: unknown layer and shape types are
//! skipped (the spec asks players to), legacy keyframes (`s`/`e` pairs,
//! array-wrapped scalars, 0–255 colours) are accepted, slots (`sid`) are
//! resolved against the file's `slots` merged with the host's overrides.

use crate::anim::{Keyframe, Prop};
use crate::display::Blend;
use crate::model::*;
use serde_json::{Map, Value};
use std::collections::HashMap;

type Obj = Map<String, Value>;

pub(crate) struct Importer<'a> {
    slots: HashMap<String, &'a Value>,
    image_ids: HashMap<String, usize>,
}

/// Import a parsed Lottie document. `overrides` maps slot ids to slot
/// values (`{"p": <property>}` or a bare property / value).
pub fn import(root: &Value, overrides: &Map<String, Value>) -> Animation {
    let mut slots: HashMap<String, &Value> = HashMap::new();
    if let Some(Value::Object(s)) = root.get("slots") {
        for (k, v) in s {
            slots.insert(k.clone(), v.get("p").unwrap_or(v));
        }
    }
    for (k, v) in overrides {
        slots.insert(k.clone(), v.get("p").unwrap_or(v));
    }
    // `{"$mode": "dark"}`: `{light, dark}` colours (slots `ld:<light>:<dark>`)
    // take their dark half unless the host overrode the slot itself.
    let dark = overrides.get("$mode").and_then(Value::as_str) == Some("dark");
    let mut owned: Vec<(String, Value)> = Vec::new();
    if let Some(Value::Object(s)) = root.get("slots") {
        for sid in s.keys() {
            let Some(rest) = sid.strip_prefix("ld:") else { continue };
            if overrides.contains_key(sid) {
                continue;
            }
            let pick = if dark { rest.split(':').nth(1) } else { rest.split(':').next() };
            if let Some(c) = pick.and_then(crate::dialect::parse_hex) {
                owned.push((sid.clone(), Value::Array(c.iter().map(|x| Value::from(*x)).collect())));
            }
        }
    }
    let owned: Vec<(String, Value)> = owned.into_iter().map(|(k, v)| (k, serde_json::json!({"a": 0, "k": v}))).collect();
    for (k, v) in &owned {
        slots.insert(k.clone(), v);
    }
    let mut anim = Animation {
        width: num(root, "w", 0.0),
        height: num(root, "h", 0.0),
        frame_rate: num(root, "fr", 30.0),
        in_point: num(root, "ip", 0.0),
        out_point: num(root, "op", 0.0),
        ..Default::default()
    };
    let assets: &[Value] = root.get("assets").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    // Pass 1: image assets (layers reference them by id).
    for a in assets {
        if a.get("layers").is_none() && a.get("p").is_some() {
            let id = string(a, "id");
            let p = string(a, "p");
            let u = string(a, "u");
            let location = if p.starts_with("data:") || a.get("e").and_then(Value::as_i64) == Some(1) {
                p
            } else {
                format!("{u}{p}")
            };
            anim.image_ids.insert(id.clone(), anim.images.len());
            anim.images.push(ImageAsset {
                id,
                width: num(a, "w", 0.0),
                height: num(a, "h", 0.0),
                location,
            });
        }
    }
    let im = Importer { slots, image_ids: anim.image_ids.clone() };
    // Pass 2: precompositions.
    for a in assets {
        if let Some(Value::Array(layers)) = a.get("layers") {
            anim.precomps.insert(string(a, "id"), im.layers(layers));
        }
    }
    if let Some(Value::Array(layers)) = root.get("layers") {
        anim.layers = im.layers(layers);
    }
    if let Some(Value::Array(markers)) = root.get("markers") {
        for m in markers {
            anim.markers.push(Marker {
                name: m
                    .get("cm")
                    .or_else(|| m.get("nm"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                time: num(m, "tm", 0.0),
                duration: num(m, "dr", 0.0),
            });
        }
    }
    if let Some(Value::Array(list)) = root.get("fonts").and_then(|f| f.get("list")) {
        for f in list {
            anim.fonts.push(Font {
                name: string(f, "fName"),
                family: string(f, "fFamily"),
                style: string(f, "fStyle"),
                ascent: num(f, "ascent", 75.0),
            });
        }
    }
    if let Some(Value::Array(chars)) = root.get("chars") {
        for c in chars {
            let shapes = c
                .get("data")
                .and_then(|d| d.get("shapes"))
                .and_then(Value::as_array)
                .map(|s| im.items(s))
                .unwrap_or_default();
            anim.chars.push(Char {
                ch: string(c, "ch"),
                family: string(c, "fFamily"),
                style: string(c, "style"),
                size: num(c, "size", 100.0),
                width: num(c, "w", 0.0),
                shapes,
            });
        }
    }
    anim
}

/// Old exports put the closed flag on the item (`closed` on paths, `cl` on
/// masks) instead of `c` in each shape value: inject it.
pub(crate) fn with_closed(prop: Option<&Value>, closed: bool) -> Option<Value> {
    let mut p = prop?.clone();
    fn fix(v: &mut Value, closed: bool) {
        match v {
            Value::Object(o) if o.contains_key("v") && !o.contains_key("c") => {
                o.insert("c".into(), Value::Bool(closed));
            }
            Value::Array(a) => {
                for x in a {
                    fix(x, closed);
                }
            }
            _ => {}
        }
    }
    if let Some(k) = p.get_mut("k") {
        match k {
            Value::Array(kfs) if kfs.first().is_some_and(|f| f.get("t").is_some()) => {
                for kf in kfs {
                    for key in ["s", "e"] {
                        if let Some(x) = kf.get_mut(key) {
                            fix(x, closed);
                        }
                    }
                }
            }
            other => fix(other, closed),
        }
    }
    Some(p)
}

pub(crate) fn num(v: &Value, k: &str, d: f64) -> f64 {
    match v.get(k) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(d),
        Some(Value::Array(a)) => a.first().and_then(Value::as_f64).unwrap_or(d),
        Some(Value::Bool(b)) => *b as i32 as f64,
        _ => d,
    }
}

pub(crate) fn string(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn flag(v: &Value, k: &str) -> bool {
    match v.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        _ => false,
    }
}

fn floats(v: &Value, out: &mut Vec<f64>) {
    match v {
        Value::Number(n) => out.push(n.as_f64().unwrap_or(0.0)),
        Value::Array(a) => {
            for x in a {
                if let Some(f) = x.as_f64() {
                    out.push(f);
                }
            }
        }
        _ => {}
    }
}

/// `{i, o, v, c}` → `[closed, n, (vx, vy, ix, iy, ox, oy) × n]`.
fn shape_value(v: &Value, out: &mut Vec<f64>) {
    let v = match v {
        Value::Array(a) => match a.first() {
            Some(first @ Value::Object(_)) => first,
            _ => return,
        },
        other => other,
    };
    let pts = |k: &str| -> Vec<[f64; 2]> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|p| {
                        [
                            p.get(0).and_then(Value::as_f64).unwrap_or(0.0),
                            p.get(1).and_then(Value::as_f64).unwrap_or(0.0),
                        ]
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let (vs, is, os) = (pts("v"), pts("i"), pts("o"));
    out.push(if flag(v, "c") { 1.0 } else { 0.0 });
    out.push(vs.len() as f64);
    for (k, p) in vs.iter().enumerate() {
        let i = is.get(k).copied().unwrap_or([0.0, 0.0]);
        let o = os.get(k).copied().unwrap_or([0.0, 0.0]);
        out.extend_from_slice(&[p[0], p[1], i[0], i[1], o[0], o[1]]);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Plain,
    Color,
    Shape,
}

fn ease_pair(v: Option<&Value>) -> (Vec<f64>, Vec<f64>) {
    let mut x = Vec::new();
    let mut y = Vec::new();
    if let Some(v) = v {
        if let Some(xv) = v.get("x") {
            floats(xv, &mut x);
        }
        if let Some(yv) = v.get("y") {
            floats(yv, &mut y);
        }
    }
    (x, y)
}

fn normalize_color(v: &mut [f64]) {
    if v.iter().take(3).any(|c| *c > 1.0) {
        for c in v.iter_mut().take(4) {
            *c /= 255.0;
        }
    }
}

impl Importer<'_> {
    /// The value of slot `sid` (the file's `slots` merged with the host's overrides).
    pub(crate) fn slot(&self, sid: &str) -> Option<&Value> {
        self.slots.get(sid).copied()
    }

    /// An animatable property. Missing → `None`.
    pub(crate) fn prop_opt(&self, v: Option<&Value>, kind: Kind) -> Option<Prop> {
        let v = v?;
        let v = match v.get("sid").and_then(Value::as_str) {
            Some(sid) => self.slots.get(sid).copied().unwrap_or(v),
            None => v,
        };
        Some(self.prop_value(v, kind))
    }

    pub(crate) fn prop(&self, v: Option<&Value>, kind: Kind, default: &[f64]) -> Prop {
        self.prop_opt(v, kind)
            .unwrap_or_else(|| Prop::fixed(default.to_vec()))
    }

    fn prop_value(&self, v: &Value, kind: Kind) -> Prop {
        let k = match v {
            Value::Object(o) if o.contains_key("k") => &o["k"],
            // A bare value (slot overrides, legacy).
            other => other,
        };
        let keyframed = match k {
            Value::Array(a) => matches!(a.first(), Some(Value::Object(o)) if o.contains_key("t")),
            _ => false,
        };
        let read = |val: &Value, out: &mut Vec<f64>| match kind {
            Kind::Shape => shape_value(val, out),
            _ => {
                floats(val, out);
                if kind == Kind::Color {
                    normalize_color(out);
                }
            }
        };
        if !keyframed {
            let mut value = Vec::new();
            read(k, &mut value);
            return Prop::fixed(value);
        }
        let mut keyframes = Vec::new();
        for kf in k.as_array().into_iter().flatten() {
            let mut s = Vec::new();
            if let Some(sv) = kf.get("s") {
                read(sv, &mut s);
            }
            let e = kf.get("e").map(|ev| {
                let mut e = Vec::new();
                read(ev, &mut e);
                e
            });
            let tan = |key: &str| {
                let mut t = Vec::new();
                floats(kf.get(key)?, &mut t);
                (t.len() >= 2).then(|| [t[0], t[1]])
            };
            keyframes.push(Keyframe {
                t: num(kf, "t", 0.0),
                s,
                e,
                o: ease_pair(kf.get("o")),
                i: ease_pair(kf.get("i")),
                hold: flag(kf, "h"),
                to: tan("to"),
                ti: tan("ti"),
            });
        }
        let value = keyframes.first().map(|k| k.s.clone()).unwrap_or_default();
        Prop { value, keyframes }
    }

    pub(crate) fn transform(&self, v: Option<&Value>) -> Transform {
        let Some(v) = v else {
            return Transform::default();
        };
        let p = |k: &str| self.prop_opt(v.get(k), Kind::Plain);
        let position = match v.get("p") {
            Some(pv) if flag(pv, "s") => Position::Split(
                self.prop(pv.get("x"), Kind::Plain, &[0.0]),
                self.prop(pv.get("y"), Kind::Plain, &[0.0]),
            ),
            Some(pv) => Position::Joined(self.prop_value(pv, Kind::Plain)),
            None => Position::None,
        };
        Transform {
            anchor: p("a"),
            position,
            scale: p("s"),
            rotation: p("r").or_else(|| p("rz")),
            skew: p("sk"),
            skew_axis: p("sa"),
            opacity: p("o"),
            start_opacity: p("so"),
            end_opacity: p("eo"),
        }
    }

    pub(crate) fn layers(&self, layers: &[Value]) -> Vec<Layer> {
        layers.iter().filter_map(|l| self.layer(l)).collect()
    }

    fn layer(&self, v: &Value) -> Option<Layer> {
        let o: &Obj = v.as_object()?;
        let ty = o.get("ty").and_then(Value::as_i64).unwrap_or(-1);
        let kind = match ty {
            0 => LayerKind::Precomp {
                id: string(v, "refId"),
                width: num(v, "w", 0.0),
                height: num(v, "h", 0.0),
                time_remap: self.prop_opt(v.get("tm"), Kind::Plain),
            },
            1 => LayerKind::Solid {
                color: hex_color(&string(v, "sc")),
                width: num(v, "sw", 0.0),
                height: num(v, "sh", 0.0),
            },
            2 => match self.image_ids.get(&string(v, "refId")) {
                Some(&asset) => LayerKind::Image { asset },
                None => LayerKind::Other,
            },
            3 => LayerKind::Null,
            4 => LayerKind::Shape(
                o.get("shapes")
                    .and_then(Value::as_array)
                    .map(|s| self.items(s))
                    .unwrap_or_default(),
            ),
            5 => match crate::text::import(self, v) {
                Some(t) => LayerKind::Text(Box::new(t)),
                None => LayerKind::Other,
            },
            _ => LayerKind::Other,
        };
        let name = string(v, "nm");
        let masks = o
            .get("masksProperties")
            .and_then(Value::as_array)
            .map(|ms| {
                ms.iter()
                    .map(|m| Mask {
                        mode: match m.get("mode").and_then(Value::as_str).unwrap_or("a") {
                            "n" => MaskMode::None,
                            "s" => MaskMode::Subtract,
                            "i" => MaskMode::Intersect,
                            "l" => MaskMode::Lighten,
                            "d" => MaskMode::Darken,
                            "f" => MaskMode::Difference,
                            _ => MaskMode::Add,
                        },
                        inverted: flag(m, "inv"),
                        path: self.prop(with_closed(m.get("pt"), m.get("cl").and_then(Value::as_bool).unwrap_or(true)).as_ref(), Kind::Shape, &[]),
                        opacity: self.prop_opt(m.get("o"), Kind::Plain),
                        expansion: self.prop_opt(m.get("x"), Kind::Plain),
                        feather: self.prop_opt(m.get("f"), Kind::Plain),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let effects = o
            .get("ef")
            .and_then(Value::as_array)
            .map(|efs| {
                efs.iter()
                    .map(|e| Effect {
                        ty: e.get("ty").and_then(Value::as_i64).unwrap_or(-1),
                        name: string(e, "nm"),
                        enabled: e.get("en").map(|_| flag(e, "en")).unwrap_or(true),
                        values: e
                            .get("ef")
                            .and_then(Value::as_array)
                            .map(|cs| {
                                cs.iter()
                                    .map(|c| {
                                        let kind = if c.get("ty").and_then(Value::as_i64) == Some(2) {
                                            Kind::Color
                                        } else {
                                            Kind::Plain
                                        };
                                        self.prop(c.get("v"), kind, &[0.0])
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Layer {
            kind,
            name,
            index: o.get("ind").and_then(Value::as_i64),
            parent: o.get("parent").and_then(Value::as_i64),
            in_point: num(v, "ip", 0.0),
            out_point: num(v, "op", f64::MAX),
            start_time: num(v, "st", 0.0),
            stretch: {
                let s = num(v, "sr", 1.0);
                if s == 0.0 {
                    1.0
                } else {
                    s
                }
            },
            hidden: flag(v, "hd"),
            transform: self.transform(o.get("ks")),
            auto_orient: flag(v, "ao"),
            blend: blend_mode(num(v, "bm", 0.0) as i64),
            masks,
            matte_mode: match o.get("tt").and_then(Value::as_i64) {
                Some(1) => Some(MatteMode::Alpha),
                Some(2) => Some(MatteMode::AlphaInverted),
                Some(3) => Some(MatteMode::Luma),
                Some(4) => Some(MatteMode::LumaInverted),
                _ => None,
            },
            matte_parent: o.get("tp").and_then(Value::as_i64),
            is_matte: flag(v, "td"),
            effects,
            three_d: flag(v, "ddd"),
        })
    }

    pub(crate) fn items(&self, items: &[Value]) -> Vec<Item> {
        items.iter().filter_map(|i| self.item(i)).collect()
    }

    fn gradient(&self, v: &Value) -> Gradient {
        let g = v.get("g");
        Gradient {
            radial: num(v, "t", 1.0) as i64 == 2,
            start: self.prop(v.get("s"), Kind::Plain, &[0.0, 0.0]),
            end: self.prop(v.get("e"), Kind::Plain, &[0.0, 0.0]),
            highlight_length: self.prop(v.get("h"), Kind::Plain, &[0.0]),
            highlight_angle: self.prop(v.get("a"), Kind::Plain, &[0.0]),
            count: g.map(|g| num(g, "p", 0.0) as usize).unwrap_or(0),
            stops: self.prop(g.and_then(|g| g.get("k")), Kind::Plain, &[]),
        }
    }

    fn stroke(&self, v: &Value, gradient: bool) -> StrokeItem {
        StrokeItem {
            color: (!gradient).then(|| self.prop(v.get("c"), Kind::Color, &[0.0, 0.0, 0.0, 1.0])),
            gradient: gradient.then(|| self.gradient(v)),
            opacity: self.prop(v.get("o"), Kind::Plain, &[100.0]),
            width: self.prop(v.get("w"), Kind::Plain, &[1.0]),
            cap: num(v, "lc", 2.0) as u8,
            join: num(v, "lj", 2.0) as u8,
            miter: num(v, "ml", 4.0),
            dashes: v
                .get("d")
                .and_then(Value::as_array)
                .map(|ds| {
                    ds.iter()
                        .map(|d| {
                            (
                                string(d, "n").chars().next().unwrap_or('d'),
                                self.prop(d.get("v"), Kind::Plain, &[0.0]),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn item(&self, v: &Value) -> Option<Item> {
        if flag(v, "hd") {
            return None;
        }
        let ty = v.get("ty").and_then(Value::as_str)?;
        let p = |k: &str, d: &[f64]| self.prop(v.get(k), Kind::Plain, d);
        let d = num(v, "d", 0.0) as i64;
        let reversed = d == 3;
        Some(match ty {
            "gr" => {
                let raw = v.get("it").and_then(Value::as_array).cloned().unwrap_or_default();
                let transform = raw
                    .iter()
                    .find(|i| i.get("ty").and_then(Value::as_str) == Some("tr"))
                    .map(|t| self.transform(Some(t)));
                let items = raw
                    .iter()
                    .filter(|i| i.get("ty").and_then(Value::as_str) != Some("tr"))
                    .filter_map(|i| self.item(i))
                    .collect();
                Item::Group { items, transform }
            }
            "rc" => Item::Rect {
                position: p("p", &[0.0, 0.0]),
                size: p("s", &[0.0, 0.0]),
                roundness: p("r", &[0.0]),
                // lottie-web: a rect is clockwise only for d = 1 or 2.
                reversed: !(d == 1 || d == 2),
            },
            "el" => Item::Ellipse {
                position: p("p", &[0.0, 0.0]),
                size: p("s", &[0.0, 0.0]),
                reversed,
            },
            "sr" => Item::Star(Box::new(Star {
                polygon: num(v, "sy", 1.0) as i64 == 2,
                position: p("p", &[0.0, 0.0]),
                outer_radius: p("or", &[0.0]),
                outer_roundness: p("os", &[0.0]),
                inner_radius: p("ir", &[0.0]),
                inner_roundness: p("is", &[0.0]),
                rotation: p("r", &[0.0]),
                points: p("pt", &[5.0]),
                reversed,
            })),
            "sh" => Item::Path {
                shape: self.prop(with_closed(v.get("ks"), flag(v, "closed")).as_ref(), Kind::Shape, &[]),
                reversed,
            },
            "fl" => Item::Fill {
                color: self.prop(v.get("c"), Kind::Color, &[0.0, 0.0, 0.0, 1.0]),
                opacity: p("o", &[100.0]),
                evenodd: num(v, "r", 1.0) as i64 == 2,
            },
            "st" => Item::Stroke(Box::new(self.stroke(v, false))),
            "gf" => Item::GradientFill {
                gradient: Box::new(self.gradient(v)),
                opacity: p("o", &[100.0]),
                evenodd: num(v, "r", 1.0) as i64 == 2,
            },
            "gs" => Item::GradientStroke(Box::new(self.stroke(v, true))),
            "tm" => Item::Trim {
                start: p("s", &[0.0]),
                end: p("e", &[100.0]),
                offset: p("o", &[0.0]),
                individually: num(v, "m", 1.0) as i64 == 2,
            },
            "rp" => Item::Repeater {
                copies: p("c", &[1.0]),
                offset: p("o", &[0.0]),
                below: num(v, "m", 1.0) as i64 == 2,
                transform: self.transform(v.get("tr")),
            },
            "rd" => Item::RoundCorners { radius: p("r", &[0.0]) },
            "pb" => Item::PuckerBloat { amount: p("a", &[0.0]) },
            "zz" => Item::ZigZag {
                size: p("s", &[0.0]),
                ridges: p("r", &[0.0]),
                smooth: num(v, "pt", 1.0) as i64 == 2,
            },
            "op" => Item::OffsetPath {
                amount: p("a", &[0.0]),
                join: num(v, "lj", 1.0) as u8,
                miter: p("ml", &[4.0]),
            },
            "tw" => Item::Twist {
                angle: p("a", &[0.0]),
                center: p("c", &[0.0, 0.0]),
            },
            "mm" => Item::Merge { mode: num(v, "mm", 1.0) as u8 },
            _ => return None,
        })
    }
}

fn hex_color(s: &str) -> [f64; 4] {
    let h = s.trim_start_matches('#');
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    if h.len() >= 6 {
        [c(0), c(2), c(4), 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub(crate) fn blend_mode(bm: i64) -> Blend {
    match bm {
        1 => Blend::Multiply,
        2 => Blend::Screen,
        3 => Blend::Overlay,
        4 => Blend::Darken,
        5 => Blend::Lighten,
        6 => Blend::ColorDodge,
        7 => Blend::ColorBurn,
        8 => Blend::HardLight,
        9 => Blend::SoftLight,
        10 => Blend::Difference,
        11 => Blend::Exclusion,
        12 => Blend::Hue,
        13 => Blend::Saturation,
        14 => Blend::Color,
        15 => Blend::Luminosity,
        _ => Blend::Normal,
    }
}
