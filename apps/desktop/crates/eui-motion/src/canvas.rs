//! Canvas (eUI round 8, `packages/exponential-ui/docs/round-8-contract.md`):
//! a STATIC vector drawing (charts, plots, diagrams, illustrations) in
//! Motion's shape vocabulary plus native `text`, compiled into a one-frame
//! Motion and sampled into the same [`DisplayList`] every Motion painter
//! replays. Text is never outlined here: it comes out as [`Op::Text`] ops
//! AFTER every shape op (source order), and each painter sets it with its
//! native text system in the theme's family.
//!
//! ```
//! let doc = serde_json::json!({
//!     "size": [100, 50],
//!     "shapes": [
//!         {"type": "rect", "position": [50, 25], "size": [80, 30]},
//!         {"type": "fill", "color": "#3366cc"},
//!         {"type": "text", "text": "42", "position": [50, 25], "size": 12, "align": "center", "baseline": "middle", "color": "#ffffff"}
//!     ]
//! });
//! let canvas = eui_motion::canvas::Canvas::compile(&doc).unwrap();
//! let mut list = eui_motion::DisplayList::default();
//! canvas.render(&serde_json::Map::new(), &mut list);
//! assert!(matches!(list.ops.last(), Some(eui_motion::Op::Text { .. })));
//! ```
//!
//! The document is the Canvas node's `{size, shapes}`:
//! - geometry `rect {position (centre), size, radius}`, `ellipse {position,
//!   size}`, `star` / `polygon {position, points, outerRadius, innerRadius,
//!   outerRoundness, innerRoundness, rotation}`, `path {d}`; styles `fill
//!   {color, opacity, rule}`, `stroke {color, width, cap, join, dash,
//!   dashOffset, miterLimit, opacity}`, `gradient-fill {kind, start, end,
//!   stops, highlight, highlightAngle, opacity, rule}`; modifiers `trim`,
//!   `repeater`, `round-corners`; `group {shapes, transform}` — exactly the
//!   dialect's (`dialect.rs`), the same colour forms (`$color.<token>`,
//!   `#hex`, `[r, g, b, a]`, `{light, dark}`);
//! - `text {text, position [x, y], size, weight, align, baseline, font,
//!   color}` (defaults: `[0, 0]`, 14, `regular`, `start`, `alphabetic`,
//!   `sans`, `$color.foreground`); line breaks become spaces (one line);
//!   inside groups it follows the groups' transforms and opacity; modifiers
//!   (trim, repeater, round-corners) never touch it;
//! - every value is STATIC: an animated `{keys}` value is refused
//!   (`canvas-animated`: Motion is for animation), as are raw `lottie` keys.

use serde_json::{json, Map, Value};

use crate::display::{DisplayList, Op, TextAlign, TextBaseline, TextFont, TextWeight};
use crate::geom::Affine;
use crate::validate::{Issue, Limits};

/// The canvas budget (`catalog/limits.json` › `canvas`) on top of the Motion
/// [`Limits`] (bytes and shapes: the compiled Lottie is validated like any;
/// vertices are not counted, as for a Motion: `max_ops` bounds the frame).
#[derive(Clone, Debug)]
pub struct CanvasLimits {
    /// Display-list ops one canvas frame may hold (text ops included).
    pub max_ops: usize,
    /// `text` shapes in one canvas.
    pub max_texts: usize,
    /// Characters (Unicode scalar values) in one `text` shape.
    pub max_text_chars: usize,
}

impl Default for CanvasLimits {
    fn default() -> Self {
        Self { max_ops: 4_000, max_texts: 500, max_text_chars: 1_000 }
    }
}

/// The shape types a canvas takes (Motion's documented vocabulary + `text`).
pub const SHAPE_TYPES: [&str; 13] =
    ["rect", "ellipse", "star", "polygon", "path", "fill", "stroke", "gradient-fill", "trim", "repeater", "round-corners", "group", "text"];

/// How the drawing fills its box (the catalog's `motionFit`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Uniform, centred, all of it visible.
    #[default]
    Contain,
    /// Uniform, centred, filling the box (overflow clipped).
    Cover,
    /// Stretched to the box.
    Fill,
}

impl Fit {
    /// `contain` | `cover` | `fill`; anything else = `contain`.
    pub fn parse(s: &str) -> Fit {
        match s {
            "cover" => Fit::Cover,
            "fill" => Fit::Fill,
            _ => Fit::Contain,
        }
    }
}

/// THE fit mapping: drawing units → box px, `[a, b, c, d, e, f]` like the
/// display list's transforms. `contain` = uniform `min(bw/cw, bh/ch)`,
/// centred; `cover` = uniform `max(…)`, centred; `fill` = `bw/cw`, `bh/ch`
/// from the top-left. A drawing without a size maps as the identity.
pub fn fit_matrix(cw: f64, ch: f64, bw: f64, bh: f64, fit: Fit) -> [f64; 6] {
    if !(cw > 0.0) || !(ch > 0.0) {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    match fit {
        Fit::Fill => [bw / cw, 0.0, 0.0, bh / ch, 0.0, 0.0],
        Fit::Contain | Fit::Cover => {
            let s = if fit == Fit::Cover { (bw / cw).max(bh / ch) } else { (bw / cw).min(bh / ch) };
            [s, 0.0, 0.0, s, (bw - cw * s) / 2.0, (bh - ch * s) / 2.0]
        }
    }
}

/// A pointer at box px `(x, y)` → drawing units (the fit mapping inverted),
/// rounded to two decimals: what `press` / `move` carry. Points in a
/// `contain` letterbox map outside `0..size` (never clamped). `None` when the
/// box is empty.
pub fn to_drawing(x: f64, y: f64, cw: f64, ch: f64, bw: f64, bh: f64, fit: Fit) -> Option<[f64; 2]> {
    let inv = Affine(fit_matrix(cw, ch, bw, bh, fit)).invert()?;
    let p = inv.apply([x, y]);
    let r = |v: f64| {
        let v = (v * 100.0).round() / 100.0;
        if v == 0.0 {
            0.0
        } else {
            v
        }
    };
    Some([r(p[0]), r(p[1])])
}

/// A text colour as written: literal, a slot (token / light-dark pair).
enum TextColor {
    Rgba([f64; 4]),
    /// `$color.<token>`: the host's slot override, else the compile placeholder.
    Token(String),
    /// `{light, dark}`: the slot `ld:<light>:<dark>`; the host may override it,
    /// else `$mode` picks a half.
    LightDark { sid: String, light: [f64; 4], dark: [f64; 4] },
}

struct CanvasText {
    transform: Affine,
    text: String,
    font: TextFont,
    weight: TextWeight,
    size: f64,
    align: TextAlign,
    baseline: TextBaseline,
    color: TextColor,
    /// The enclosing groups' opacity (0–1).
    alpha: f64,
}

/// A compiled, validated canvas.
pub struct Canvas {
    lottie: Value,
    texts: Vec<CanvasText>,
    width: f64,
    height: f64,
}

/// The placeholder a `$color` token shows until the host fills its slot
/// (the dialect's).
const PLACEHOLDER: [f64; 4] = [0.5, 0.5, 0.5, 1.0];

fn issue(code: &str, path: &str, message: impl Into<String>) -> Issue {
    Issue { code: code.into(), path: path.into(), message: message.into() }
}

impl Canvas {
    /// Compile `{size, shapes}` with the default limits.
    pub fn compile(doc: &Value) -> Result<Canvas, Vec<Issue>> {
        Self::compile_with(doc, &Limits::default(), &CanvasLimits::default())
    }

    /// Compile and validate: the vocabulary, static values, the Motion
    /// limits and the canvas budget. Every finding at once.
    pub fn compile_with(doc: &Value, limits: &Limits, budget: &CanvasLimits) -> Result<Canvas, Vec<Issue>> {
        let mut issues = Vec::new();
        let Some(root) = doc.as_object() else {
            return Err(vec![issue("root", "", "expected an object {size, shapes}")]);
        };
        let size = root.get("size").and_then(Value::as_array).filter(|a| a.len() == 2);
        let (w, h) = match size.map(|a| (a[0].as_f64(), a[1].as_f64())) {
            Some((Some(w), Some(h))) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => (w, h),
            _ => {
                issues.push(issue("size", "/size", "size [width, height] is required (positive numbers)"));
                (0.0, 0.0)
            }
        };
        let mut shapes = match root.get("shapes") {
            Some(Value::Array(a)) => a.clone(),
            _ => {
                issues.push(issue("shapes", "/shapes", "shapes [...] is required"));
                Vec::new()
            }
        };
        let mut texts = Vec::new();
        walk(&mut shapes, "/shapes", &Affine::IDENTITY, 1.0, &mut texts, &mut issues, budget);
        if texts.len() > budget.max_texts {
            issues.push(issue("limit-texts", "/shapes", "too many text shapes"));
        }
        if !issues.is_empty() {
            return Err(issues);
        }
        let dialect = json!({
            "size": [w, h],
            "time": "frames",
            "fps": 60,
            "duration": 1,
            "layers": [{"type": "shape", "shapes": shapes}],
        });
        let mut lottie = match crate::dialect::compile(&dialect) {
            Ok(l) => l,
            Err(mut found) => {
                for i in &mut found {
                    if let Some(rest) = i.path.strip_prefix("/layers/0/shapes") {
                        i.path = ["/shapes", rest].concat();
                    }
                }
                return Err(found);
            }
        };
        // The text colours' slots join the shapes' (hosts fill every
        // `$color.<token>` slot from the theme; `$mode` picks the pairs).
        if let Value::Object(o) = &mut lottie {
            for t in &texts {
                let (sid, c) = match &t.color {
                    TextColor::Token(sid) => (sid, PLACEHOLDER),
                    TextColor::LightDark { sid, light, .. } => (sid, *light),
                    TextColor::Rgba(_) => continue,
                };
                if !o.contains_key("slots") {
                    o.insert("slots".into(), json!({}));
                }
                if let Some(Value::Object(s)) = o.get_mut("slots") {
                    if !s.contains_key(sid) {
                        s.insert(sid.clone(), json!({"p": {"a": 0, "k": c}}));
                    }
                }
            }
        }
        // `to_string`, not `to_vec`: the serializer the WASM already carries.
        let bytes = lottie.to_string().into_bytes();
        let report = crate::validate::check(&bytes, limits);
        if !report.errors.is_empty() {
            return Err(report.errors);
        }
        let canvas = Canvas { lottie, texts, width: w, height: h };
        let mut list = DisplayList::default();
        canvas.render(&Map::new(), &mut list);
        let mut over = Vec::new();
        if list.ops.len() > budget.max_ops {
            over.push(issue("limit-ops", "/shapes", "too many drawing ops"));
        }
        if !over.is_empty() {
            return Err(over);
        }
        Ok(canvas)
    }

    /// The drawing's `[width, height]` (its coordinate space).
    pub fn size(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }

    /// The one-frame Lottie the shapes compiled to (its `slots` name every
    /// colour slot of the canvas, text colours included).
    pub fn lottie(&self) -> &Value {
        &self.lottie
    }

    /// Every slot id a host may fill (`$color.<token>` from the theme,
    /// `ld:<light>:<dark>` pairs), sorted: the Motion player's slot contract.
    pub fn slot_ids(&self) -> Vec<String> {
        #[allow(unused_mut)]
        let mut ids: Vec<String> = self.lottie.get("slots").and_then(Value::as_object).map(|s| s.keys().cloned().collect()).unwrap_or_default();
        // The WASM facade builds without `ordered` and nothing else in its
        // graph turns on serde_json's `preserve_order`: its map is a
        // BTreeMap, already sorted (the sort would cost it ~2 KB gz).
        // Natives may unify `preserve_order` in (eui-core does): sort.
        #[cfg(any(feature = "ordered", not(target_arch = "wasm32")))]
        ids.sort();
        ids
    }

    /// Sample the drawing with the host's slot overrides (`{"$color.primary":
    /// {"p": {"a": 0, "k": [r, g, b, a]}}, "$mode": "dark"}`, the Motion
    /// player's): the shapes' ops, then one `Text` op per text shape.
    pub fn render(&self, slots: &Map<String, Value>, list: &mut DisplayList) {
        let anim = crate::import::import(&self.lottie, slots);
        crate::render::render(&anim, 0.0, list);
        let dark = slots.get("$mode").and_then(Value::as_str) == Some("dark");
        for t in &self.texts {
            let c = match &t.color {
                TextColor::Rgba(c) => *c,
                TextColor::Token(sid) => slot_color(slots.get(sid)).unwrap_or(PLACEHOLDER),
                TextColor::LightDark { sid, light, dark: d } => slot_color(slots.get(sid)).unwrap_or(if dark { *d } else { *light }),
            };
            list.ops.push(Op::Text {
                transform: t.transform.to_dl(),
                text: t.text.clone(),
                font: t.font,
                weight: t.weight,
                size: t.size as f32,
                align: t.align,
                baseline: t.baseline,
                color: [c[0] as f32, c[1] as f32, c[2] as f32, (c[3] * t.alpha) as f32],
            });
        }
    }
}

/// A slot override → RGBA: `{"p": {"k": [...]}}`, `{"k": [...]}` or `[...]`.
fn slot_color(v: Option<&Value>) -> Option<[f64; 4]> {
    let v = v?;
    let v = v.get("p").unwrap_or(v);
    let k = v.get("k").unwrap_or(v);
    let a: Vec<f64> = k.as_array()?.iter().filter_map(Value::as_f64).collect();
    match a.len() {
        3 => Some([a[0], a[1], a[2], 1.0]),
        4 => Some([a[0], a[1], a[2], a[3]]),
        _ => None,
    }
}

/// `<p>/<k>` (one helper instead of a `format!` per site: the WASM budget).
fn at(p: &str, k: &str) -> String {
    let mut s = String::with_capacity(p.len() + k.len() + 1);
    s.push_str(p);
    s.push('/');
    s.push_str(k);
    s
}

const NOT_CANVAS: &str = "not a Canvas shape type";
const ANIMATED: &str = "Canvas values are static; use Motion to animate";
const RAW: &str = "no raw Lottie in a Canvas";

/// Every `{keys}` (an animated value) or raw `lottie` key under `v`, the
/// keys in `skip` aside.
fn refuse_dynamic(v: &Value, path: &str, skip: &[&str], issues: &mut Vec<Issue>) {
    match v {
        Value::Object(o) => {
            if o.contains_key("keys") {
                issues.push(issue("canvas-animated", path, ANIMATED));
                return;
            }
            for (k, child) in o {
                if k == "lottie" {
                    issues.push(issue("canvas-lottie", &at(path, k), RAW));
                } else if !skip.contains(&k.as_str()) {
                    refuse_dynamic(child, &at(path, k), &[], issues);
                }
            }
        }
        Value::Array(a) => {
            for (i, child) in a.iter().enumerate() {
                refuse_dynamic(child, &at(path, &i.to_string()), &[], issues);
            }
        }
        _ => {}
    }
}

/// `[x, y]` (finite numbers) or `None`.
fn pair(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    match (a.len(), a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)) {
        (2, Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some([x, y]),
        _ => None,
    }
}

/// A group's static transform (the dialect's keys) → (matrix, opacity 0–1).
fn group_matrix(t: Option<&Value>) -> (Affine, f64) {
    let Some(o) = t.and_then(Value::as_object) else { return (Affine::IDENTITY, 1.0) };
    let one = |k: &str, d: f64| o.get(k).and_then(Value::as_f64).unwrap_or(d);
    let pos = if o.contains_key("positionX") || o.contains_key("positionY") { [one("positionX", 0.0), one("positionY", 0.0)] } else { pair(o.get("position")).unwrap_or([0.0, 0.0]) };
    let m = crate::shapes::compose(pair(o.get("anchor")).unwrap_or([0.0, 0.0]), pos, pair(o.get("scale")).unwrap_or([100.0, 100.0]), one("rotation", 0.0), one("skew", 0.0), one("skewAxis", 0.0));
    (m, (one("opacity", 100.0) / 100.0).clamp(0.0, 1.0))
}

/// Validate a shape list IN PLACE and collect its texts (with their world
/// transforms): every text (and every refused item) becomes an empty group,
/// so the dialect compiles the rest and issue paths keep their indexes.
fn walk(shapes: &mut [Value], path: &str, world: &Affine, alpha: f64, texts: &mut Vec<CanvasText>, issues: &mut Vec<Issue>, budget: &CanvasLimits) {
    for (j, s) in shapes.iter_mut().enumerate() {
        let p = at(path, &j.to_string());
        let ty = s.get("type").and_then(Value::as_str).unwrap_or("");
        let known = s.is_object() && SHAPE_TYPES.contains(&ty);
        if !known {
            issues.push(issue("canvas-shape-type", &at(&p, "type"), NOT_CANVAS));
        }
        let before = issues.len();
        if known {
            refuse_dynamic(s, &p, &["shapes"], issues);
        }
        let hidden = s.get("hidden").and_then(Value::as_bool) == Some(true);
        if ty == "group" && known {
            let (m, a) = group_matrix(s.get("transform"));
            if let Some(Value::Array(inner)) = s.get_mut("shapes") {
                let mut sink = Vec::new();
                walk(inner, &at(&p, "shapes"), &world.then(&m), alpha * a, if hidden { &mut sink } else { &mut *texts }, issues, budget);
            }
            continue;
        }
        if ty == "text" && known && issues.len() == before {
            if let Some(t) = s.as_object().and_then(|o| text_shape(o, &p, world, alpha, issues, budget)) {
                if !hidden {
                    texts.push(t);
                }
            }
        }
        if ty == "text" || !known {
            let mut g = Map::new();
            g.insert("type".into(), Value::from("group"));
            *s = Value::Object(g);
        }
    }
}

fn text_shape(o: &Map<String, Value>, p: &str, world: &Affine, alpha: f64, issues: &mut Vec<Issue>, budget: &CanvasLimits) -> Option<CanvasText> {
    let before = issues.len();
    let mut bad = |k: &str, msg: &str| issues.push(issue("canvas-text", &at(p, k), msg));
    let text: String = match o.get("text") {
        // One line: a line break (\n, \r\n) becomes a space, a lone \r goes.
        Some(Value::String(s)) => s.chars().filter(|c| *c != '\r').map(|c| if c == '\n' { ' ' } else { c }).collect(),
        _ => {
            bad("text", "needs a string");
            String::new()
        }
    };
    let position = match o.get("position") {
        None => Some([0.0, 0.0]),
        v => pair(v),
    };
    if position.is_none() {
        bad("position", "expected [x, y]");
    }
    let size = o.get("size").map_or(Some(14.0), |v| v.as_f64().filter(|s| s.is_finite() && *s > 0.0));
    if size.is_none() {
        bad("size", "expected font px > 0");
    }
    let mut pick = |key: &str, names: &[&str]| -> usize {
        match o.get(key) {
            None => 0,
            Some(v) => v.as_str().and_then(|s| names.iter().position(|n| *n == s)).unwrap_or_else(|| {
                bad(key, "unknown value");
                0
            }),
        }
    };
    let weight = [TextWeight::Regular, TextWeight::Medium, TextWeight::Semibold, TextWeight::Bold][pick("weight", &["regular", "medium", "semibold", "bold"])];
    let align = [TextAlign::Start, TextAlign::Center, TextAlign::End][pick("align", &["start", "center", "end"])];
    let baseline = [TextBaseline::Alphabetic, TextBaseline::Top, TextBaseline::Middle][pick("baseline", &["alphabetic", "top", "middle"])];
    let font = [TextFont::Sans, TextFont::Mono][pick("font", &["sans", "mono"])];
    let color = text_color(o.get("color"));
    if color.is_none() {
        issues.push(issue("color", &at(p, "color"), "expected a colour"));
    }
    let chars = text.chars().count();
    if chars > budget.max_text_chars {
        issues.push(issue("limit-text", &at(p, "text"), "too long"));
    }
    if issues.len() > before {
        return None;
    }
    let position = position?;
    Some(CanvasText { transform: world.then(&Affine::translate(position[0], position[1])), text, font, weight, size: size?, align, baseline, color: color?, alpha })
}

/// A text colour (unset = `$color.foreground`); `None` when it is not one.
fn text_color(v: Option<&Value>) -> Option<TextColor> {
    let hex = |v: &Value| v.as_str().and_then(crate::dialect::parse_hex);
    match v {
        None => Some(TextColor::Token("$color.foreground".into())),
        Some(Value::String(s)) if s.starts_with("$color.") && s.len() > 7 => Some(TextColor::Token(s.clone())),
        Some(a @ Value::Array(_)) => slot_color(Some(a)).map(TextColor::Rgba),
        Some(Value::Object(o)) => {
            let (light, dark) = (hex(o.get("light")?)?, hex(o.get("dark")?)?);
            let sid = ["ld:", &crate::dialect::format_hex(&light)?, ":", &crate::dialect::format_hex(&dark)?].concat();
            Some(TextColor::LightDark { sid, light, dark })
        }
        Some(v) => hex(v).map(TextColor::Rgba),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_letterboxes_and_inverts() {
        let m = fit_matrix(100.0, 50.0, 200.0, 200.0, Fit::Contain);
        assert_eq!(m, [2.0, 0.0, 0.0, 2.0, 0.0, 50.0]);
        assert_eq!(to_drawing(100.0, 100.0, 100.0, 50.0, 200.0, 200.0, Fit::Contain), Some([50.0, 25.0]));
        assert_eq!(to_drawing(0.0, 0.0, 100.0, 50.0, 200.0, 200.0, Fit::Contain), Some([0.0, -25.0]));
    }

    #[test]
    fn texts_follow_every_shape_op() {
        let doc = json!({"size": [10, 10], "shapes": [
            {"type": "text", "text": "a"},
            {"type": "rect", "position": [5, 5], "size": [4, 4]},
            {"type": "fill", "color": "#ff0000"},
        ]});
        let c = Canvas::compile(&doc).unwrap();
        let mut list = DisplayList::default();
        c.render(&Map::new(), &mut list);
        assert!(matches!(list.ops.last(), Some(Op::Text { .. })));
        assert!(list.ops.iter().take(list.ops.len() - 1).all(|o| !matches!(o, Op::Text { .. })));
    }

    #[test]
    fn animated_values_are_refused() {
        let doc = json!({"size": [10, 10], "shapes": [{"type": "rect", "size": {"keys": [{"t": 0, "v": [1, 1]}]}}]});
        let e = Canvas::compile(&doc).err().expect("refused");
        assert_eq!(e[0].code, "canvas-animated");
        assert_eq!(e[0].path, "/shapes/0/size");
    }
}
