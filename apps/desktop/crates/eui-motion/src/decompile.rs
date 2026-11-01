//! Lottie → dialect (the inverse of `dialect::compile`). Friendly fields for
//! everything the dialect names; any property or key it cannot express
//! (expressions, foreign slots, 3D, effects) passes through raw under
//! `"lottie"`, so `compile(decompile(l))` renders exactly like `l`.

use serde_json::{json, Value};

use crate::dialect::{
    bez_to_d, decompile_color, f, n, round_to, scale_values, Bez, Obj, Val, BLENDS, EASES, LAYER_TYPES, MASK_MODES, MATTES,
};

/// Integral floats → integers everywhere (canonical output).
fn canon(v: Value) -> Value {
    match v {
        Value::Number(x) => match x.as_f64() {
            Some(v) if !x.is_i64() && !x.is_u64() => n(v),
            _ => Value::Number(x),
        },
        Value::Array(a) => Value::Array(a.into_iter().map(canon).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, canon(v))).collect()),
        other => other,
    }
}

struct Decompiler {
    fps: f64,
    ms: bool,
    inexact: bool,
}

/// Keys that never matter for rendering and are dropped.
const META_KEYS: [&str; 8] = ["mn", "ix", "cix", "np", "cl", "hix", "ind", "closed"];

fn floats_of(v: &Value) -> Vec<f64> {
    match v {
        Value::Number(x) => vec![x.as_f64().unwrap_or(0.0)],
        Value::Array(a) => a.iter().filter_map(f).collect(),
        _ => Vec::new(),
    }
}

fn plain_value(v: &Value) -> Value {
    match v {
        Value::Array(a) if a.len() == 1 && a[0].is_number() => canon(a[0].clone()),
        other => canon(other.clone()),
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(x)) => x.as_f64().unwrap_or(0.0) != 0.0,
        _ => false,
    }
}

impl Decompiler {
    fn time(&mut self, frames: f64) -> Value {
        if !self.ms {
            return n(frames);
        }
        let ms = round_to(frames * 1000.0 / self.fps, 6);
        if round_to(ms * self.fps / 1000.0, 4) != frames {
            self.inexact = true;
        }
        n(ms)
    }

    fn ease(&self, kf: &Obj) -> Option<Value> {
        if truthy(kf.get("h")) {
            return Some(json!("hold"));
        }
        let (o, i) = (kf.get("o")?, kf.get("i")?);
        let comp = |v: &Value, k: &str| floats_of(v.get(k).unwrap_or(&Value::Null));
        let (ox, oy, ix, iy) = (comp(o, "x"), comp(o, "y"), comp(i, "x"), comp(i, "y"));
        let scalar_form = |v: &Value| v.get("x").map(Value::is_number).unwrap_or(false) && v.get("y").map(Value::is_number).unwrap_or(false);
        let single = [&ox, &oy, &ix, &iy].iter().all(|v| v.len() == 1);
        // Only the scalar `{x: n, y: n}` form round-trips as [x1, y1, x2, y2].
        if single && scalar_form(o) && scalar_form(i) {
            let p = [ox[0], oy[0], ix[0], iy[0]];
            if p == [0.0, 0.0, 1.0, 1.0] {
                return None;
            }
            if let Some((name, _)) = EASES.iter().find(|(_, q)| *q == p) {
                return Some(json!(name));
            }
            return Some(Value::Array(p.iter().map(|x| n(*x)).collect()));
        }
        Some(json!({"o": canon(o.clone()), "i": canon(i.clone())}))
    }

    /// A Lottie property → a dialect value; `None` = not expressible
    /// (expression, foreign slot): the caller passes the raw property through.
    fn prop(&mut self, p: &Value, kind: Val) -> Option<Value> {
        let Some(o) = p.as_object() else {
            return Some(plain_value(p));
        };
        if o.get("x").map(Value::is_string).unwrap_or(false) {
            return None;
        }
        let sid = o.get("sid").and_then(Value::as_str);
        if let Some(sid) = sid {
            if !(kind == Val::Color && (sid.starts_with("$color.") || sid.starts_with("ld:"))) {
                return None;
            }
        }
        let k = o.get("k")?;
        let keyed = matches!(k, Value::Array(a) if matches!(a.first(), Some(Value::Object(x)) if x.contains_key("t")));
        if !keyed {
            return self.static_value(k, kind, sid);
        }
        let kfs = k.as_array()?;
        let mut keys = Vec::new();
        for (j, kf) in kfs.iter().enumerate() {
            let kf = kf.as_object()?;
            let mut key = Obj::new();
            key.insert("t".into(), self.time(kf.get("t").and_then(f).unwrap_or(0.0)));
            if let Some(s) = kf.get("s") {
                key.insert("v".into(), self.static_value(s, kind, None)?);
            }
            if let Some(e) = kf.get("e") {
                key.insert("end".into(), self.static_value(e, kind, None)?);
            }
            if j + 1 < kfs.len() {
                if let Some(e) = self.ease(kf) {
                    key.insert("ease".into(), e);
                }
            } else if truthy(kf.get("h")) {
                key.insert("ease".into(), json!("hold"));
            }
            for t in ["to", "ti"] {
                if let Some(v) = kf.get(t) {
                    key.insert(t.into(), canon(v.clone()));
                }
            }
            keys.push(Value::Object(key));
        }
        Some(json!({"keys": keys}))
    }

    fn static_value(&mut self, v: &Value, kind: Val, sid: Option<&str>) -> Option<Value> {
        match kind {
            Val::Color => Some(decompile_color(&floats_of(v), sid)),
            Val::Path => {
                let b = Bez::from_json(v)?;
                let mut d = String::new();
                bez_to_d(&b, &mut d);
                Some(json!(d))
            }
            Val::Plain => Some(plain_value(v)),
        }
    }

    /// Put `key` as a friendly field (omitted when it equals `default`), or
    /// pass the raw property through.
    #[allow(clippy::too_many_arguments)]
    fn field(&mut self, src: &Obj, lk: &str, out: &mut Obj, raw: &mut Obj, key: &str, kind: Val, default: Option<Value>) {
        let Some(p) = src.get(lk) else { return };
        match self.prop(p, kind) {
            Some(v) => {
                if default.as_ref() != Some(&v) {
                    out.insert(key.into(), v);
                }
            }
            None => {
                raw.insert(lk.into(), canon(p.clone()));
            }
        }
    }

    fn transform(&mut self, ks: &Value) -> Option<Value> {
        let o = ks.as_object()?;
        let mut out = Obj::new();
        let mut raw = Obj::new();
        self.field(o, "a", &mut out, &mut raw, "anchor", Val::Plain, Some(json!([0, 0])));
        match o.get("p") {
            Some(p) if truthy(p.get("s")) => {
                let po = p.as_object()?;
                let mut sub = Obj::new();
                self.field(po, "x", &mut out, &mut sub, "positionX", Val::Plain, None);
                self.field(po, "y", &mut out, &mut sub, "positionY", Val::Plain, None);
                if !sub.is_empty() || po.contains_key("z") {
                    out.remove("positionX");
                    out.remove("positionY");
                    raw.insert("p".into(), canon(p.clone()));
                }
            }
            Some(_) => self.field(o, "p", &mut out, &mut raw, "position", Val::Plain, Some(json!([0, 0]))),
            None => {}
        }
        self.field(o, "s", &mut out, &mut raw, "scale", Val::Plain, Some(json!([100, 100])));
        self.field(o, "r", &mut out, &mut raw, "rotation", Val::Plain, Some(json!(0)));
        self.field(o, "o", &mut out, &mut raw, "opacity", Val::Plain, Some(json!(100)));
        self.field(o, "sk", &mut out, &mut raw, "skew", Val::Plain, Some(json!(0)));
        self.field(o, "sa", &mut out, &mut raw, "skewAxis", Val::Plain, Some(json!(0)));
        self.field(o, "so", &mut out, &mut raw, "startOpacity", Val::Plain, Some(json!(100)));
        self.field(o, "eo", &mut out, &mut raw, "endOpacity", Val::Plain, Some(json!(100)));
        let known = ["a", "p", "s", "r", "o", "sk", "sa", "so", "eo", "ty", "nm"];
        for (k, v) in o {
            if !known.contains(&k.as_str()) && !META_KEYS.contains(&k.as_str()) {
                raw.insert(k.clone(), canon(v.clone()));
            }
        }
        if !raw.is_empty() {
            out.insert("lottie".into(), Value::Object(raw));
        }
        Some(Value::Object(out))
    }
}

/// Convert Lottie JSON into the dialect (the inverse of [`crate::dialect::compile`]).
pub fn decompile(lottie: &Value) -> Value {
    let fps = lottie.get("fr").and_then(f).filter(|x| *x > 0.0).unwrap_or(60.0);
    let frames = lottie.get("meta").and_then(|m| m.get("time")).and_then(Value::as_str) == Some("frames");
    let mut d = Decompiler { fps, ms: !frames, inexact: false };
    let out = decompile_with(&mut d, lottie);
    if d.inexact {
        let mut d = Decompiler { fps, ms: false, inexact: false };
        return decompile_with(&mut d, lottie);
    }
    out
}

fn pair(o: &Obj, a: &str, b: &str) -> Value {
    json!([canon(o.get(a).cloned().unwrap_or(json!(0))), canon(o.get(b).cloned().unwrap_or(json!(0)))])
}

fn decompile_with(d: &mut Decompiler, l: &Value) -> Value {
    let empty = Obj::new();
    let root = l.as_object().unwrap_or(&empty);
    let mut out = Obj::new();
    out.insert("size".into(), pair(root, "w", "h"));
    out.insert("fps".into(), n(d.fps));
    let ip = root.get("ip").and_then(f).unwrap_or(0.0);
    let op = root.get("op").and_then(f).unwrap_or(0.0);
    out.insert("duration".into(), d.time(op - ip));
    if ip != 0.0 {
        out.insert("start".into(), d.time(ip));
    }
    if !d.ms {
        out.insert("time".into(), json!("frames"));
    }
    if let Some(nm) = root.get("nm") {
        out.insert("name".into(), nm.clone());
    }
    if let Some(Value::Array(assets)) = root.get("assets") {
        let mut ao = Obj::new();
        for a in assets {
            let Some(ao_) = a.as_object() else { continue };
            let id = ao_.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            if let Some(Value::Array(layers)) = ao_.get("layers") {
                ao.insert(id, json!({"layers": decompile_layers(d, layers, op)}));
            } else if let Some(p) = ao_.get("p").and_then(Value::as_str) {
                let u = ao_.get("u").and_then(Value::as_str).unwrap_or("");
                let src = if p.starts_with("data:") { p.to_string() } else { format!("{u}{p}") };
                ao.insert(id, json!({"image": src, "size": pair(ao_, "w", "h")}));
            }
        }
        if !ao.is_empty() {
            out.insert("assets".into(), Value::Object(ao));
        }
    }
    let layers = root.get("layers").and_then(Value::as_array).cloned().unwrap_or_default();
    out.insert("layers".into(), Value::Array(decompile_layers(d, &layers, op)));
    if let Some(Value::Array(ms)) = root.get("markers") {
        let mk: Vec<Value> = ms
            .iter()
            .map(|m| {
                let mut o = Obj::new();
                o.insert("name".into(), m.get("cm").or_else(|| m.get("nm")).cloned().unwrap_or(json!("")));
                o.insert("time".into(), d.time(m.get("tm").and_then(f).unwrap_or(0.0)));
                o.insert("duration".into(), d.time(m.get("dr").and_then(f).unwrap_or(0.0)));
                Value::Object(o)
            })
            .collect();
        out.insert("markers".into(), Value::Array(mk));
    }
    let known = ["v", "fr", "ip", "op", "w", "h", "nm", "ddd", "assets", "layers", "markers", "meta"];
    let mut raw = Obj::new();
    for (k, v) in root {
        if known.contains(&k.as_str()) {
            continue;
        }
        if k == "slots" {
            // Our own token / light-dark slots are rebuilt by `compile`.
            let foreign: Obj = v
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(sid, _)| !(sid.starts_with("$color.") || sid.starts_with("ld:")))
                .map(|(k, v)| (k.clone(), canon(v.clone())))
                .collect();
            if !foreign.is_empty() {
                raw.insert(k.clone(), Value::Object(foreign));
            }
            continue;
        }
        raw.insert(k.clone(), canon(v.clone()));
    }
    if !raw.is_empty() {
        out.insert("lottie".into(), Value::Object(raw));
    }
    Value::Object(out)
}

fn layer_id(l: &Value) -> Option<Value> {
    if let Some(ln) = l.get("ln").and_then(Value::as_str) {
        return Some(json!(ln));
    }
    l.get("ind").and_then(Value::as_i64).map(|i| json!(i))
}

fn decompile_layers(d: &mut Decompiler, layers: &[Value], comp_op: f64) -> Vec<Value> {
    let id_of = |ind: i64| -> Option<Value> { layers.iter().find(|l| l.get("ind").and_then(Value::as_i64) == Some(ind)).and_then(layer_id) };
    // Matte sources: explicit `tp`, else the layer above (legacy).
    let sources: Vec<Option<Value>> = layers
        .iter()
        .enumerate()
        .map(|(j, l)| {
            l.get("tt")?;
            match l.get("tp").and_then(Value::as_i64) {
                Some(tp) => id_of(tp),
                None => j.checked_sub(1).and_then(|k| layer_id(&layers[k])),
            }
        })
        .collect();
    let mut out = Vec::new();
    for (j, l) in layers.iter().enumerate() {
        let Some(o) = l.as_object() else { continue };
        let mut lo = Obj::new();
        let mut raw = Obj::new();
        let ty = o.get("ty").and_then(Value::as_i64).unwrap_or(-1);
        let Some(tyname) = LAYER_TYPES.iter().find(|(_, c)| *c == ty).map(|(k, _)| *k) else {
            // Unknown layer types survive whole, as a null layer's raw keys.
            out.push(json!({"type": "lottie", "lottie": canon(l.clone())}));
            continue;
        };
        lo.insert("type".into(), json!(tyname));
        if let Some(id) = layer_id(l) {
            lo.insert("id".into(), id);
        }
        if let Some(nm) = o.get("nm") {
            lo.insert("name".into(), nm.clone());
        }
        if let Some(p) = o.get("parent").and_then(Value::as_i64) {
            match id_of(p) {
                Some(id) => {
                    lo.insert("parent".into(), id);
                }
                None => {
                    raw.insert("parent".into(), json!(p));
                }
            }
        }
        if let Some(tt) = o.get("tt").and_then(Value::as_i64) {
            match (MATTES.get((tt - 1).max(0) as usize).filter(|_| tt >= 1), sources[j].clone()) {
                (Some(mode), Some(src)) => {
                    lo.insert("matte".into(), json!({"source": src, "mode": mode}));
                }
                _ => {
                    raw.insert("tt".into(), json!(tt));
                    if let Some(tp) = o.get("tp") {
                        raw.insert("tp".into(), tp.clone());
                    }
                }
            }
        }
        let referenced = layer_id(l).map(|id| sources.iter().any(|s| s.as_ref() == Some(&id))).unwrap_or(false);
        if truthy(o.get("td")) && !referenced {
            raw.insert("td".into(), json!(1));
        }
        if referenced && !truthy(o.get("td")) {
            // A matte source that is ALSO drawn (no `td`): keep it visible.
            raw.insert("td".into(), json!(0));
        }
        if o.get("hd").and_then(Value::as_bool) == Some(true) {
            lo.insert("hidden".into(), json!(true));
        }
        if truthy(o.get("ao")) {
            lo.insert("autoOrient".into(), json!(true));
        }
        let bm = o.get("bm").and_then(f).unwrap_or(0.0) as usize;
        if bm > 0 {
            match BLENDS.get(bm) {
                Some(b) => {
                    lo.insert("blend".into(), json!(b));
                }
                None => {
                    raw.insert("bm".into(), json!(bm));
                }
            }
        }
        if let Some(ks) = o.get("ks") {
            if let Some(Value::Object(t)) = d.transform(ks) {
                if !t.is_empty() {
                    lo.insert("transform".into(), Value::Object(t));
                }
            }
        }
        match tyname {
            "shape" => {
                let shapes = o.get("shapes").and_then(Value::as_array).cloned().unwrap_or_default();
                lo.insert("shapes".into(), Value::Array(decompile_shapes(d, &shapes)));
            }
            "solid" => {
                let sc = o.get("sc").and_then(Value::as_str).unwrap_or("#000000");
                lo.insert("color".into(), json!(sc.to_ascii_lowercase()));
                lo.insert("size".into(), pair(o, "sw", "sh"));
            }
            "image" | "precomp" => {
                if let Some(r) = o.get("refId") {
                    lo.insert("asset".into(), r.clone());
                }
                if tyname == "precomp" {
                    lo.insert("size".into(), pair(o, "w", "h"));
                    if let Some(tm) = o.get("tm") {
                        match d.prop(tm, Val::Plain) {
                            Some(v) => {
                                // Lottie remaps to SECONDS; the dialect uses its time unit.
                                let k = if d.ms { 1000.0 } else { d.fps };
                                lo.insert("timeRemap".into(), canon(scale_values(&v, k)));
                            }
                            None => {
                                raw.insert("tm".into(), canon(tm.clone()));
                            }
                        }
                    }
                }
            }
            "text" => {
                if let Some(t) = o.get("t") {
                    match decompile_text(t) {
                        Some(v) => {
                            lo.insert("text".into(), v);
                        }
                        None => {
                            raw.insert("t".into(), canon(t.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(Value::Array(masks)) = o.get("masksProperties") {
            let mut mo = Vec::new();
            for m in masks {
                let Some(mobj) = m.as_object() else { continue };
                let mut mm = Obj::new();
                let mut mraw = Obj::new();
                let code = mobj.get("mode").and_then(Value::as_str).unwrap_or("a");
                let mode = MASK_MODES.iter().find(|(_, c)| *c == code).map(|(k, _)| *k).unwrap_or("add");
                if mode != "add" {
                    mm.insert("mode".into(), json!(mode));
                }
                let mut fixed = mobj.clone();
                if let Some(pt) = crate::import::with_closed(mobj.get("pt"), mobj.get("cl").and_then(Value::as_bool).unwrap_or(true)) {
                    fixed.insert("pt".into(), pt);
                }
                d.field(&fixed, "pt", &mut mm, &mut mraw, "path", Val::Path, None);
                if mobj.get("inv").and_then(Value::as_bool) == Some(true) {
                    mm.insert("inverted".into(), json!(true));
                }
                d.field(mobj, "o", &mut mm, &mut mraw, "opacity", Val::Plain, Some(json!(100)));
                d.field(mobj, "x", &mut mm, &mut mraw, "expansion", Val::Plain, None);
                d.field(mobj, "f", &mut mm, &mut mraw, "feather", Val::Plain, None);
                if !mraw.is_empty() {
                    mm.insert("lottie".into(), Value::Object(mraw));
                }
                mo.push(Value::Object(mm));
            }
            lo.insert("masks".into(), Value::Array(mo));
        }
        let ip = o.get("ip").and_then(f).unwrap_or(0.0);
        let op = o.get("op").and_then(f).unwrap_or(comp_op);
        let st = o.get("st").and_then(f).unwrap_or(0.0);
        if ip != 0.0 {
            lo.insert("in".into(), d.time(ip));
        }
        if op != comp_op {
            lo.insert("out".into(), d.time(op));
        }
        if st != 0.0 {
            lo.insert("start".into(), d.time(st));
        }
        if let Some(sr) = o.get("sr").and_then(f) {
            if sr != 1.0 {
                lo.insert("stretch".into(), n(sr));
            }
        }
        let known = [
            "ddd", "ind", "ty", "nm", "ln", "parent", "tt", "tp", "td", "hd", "ao", "bm", "ks", "shapes", "sc", "sw", "sh", "refId", "w", "h",
            "tm", "t", "masksProperties", "hasMask", "ip", "op", "st", "sr",
        ];
        for (k, v) in o {
            if known.contains(&k.as_str()) || META_KEYS.contains(&k.as_str()) {
                continue;
            }
            raw.insert(k.clone(), canon(v.clone()));
        }
        if truthy(o.get("ddd")) {
            raw.insert("ddd".into(), json!(1));
        }
        if !raw.is_empty() {
            lo.insert("lottie".into(), Value::Object(raw));
        }
        out.push(Value::Object(lo));
    }
    out
}

fn decompile_text(t: &Value) -> Option<Value> {
    let kfs = t.get("d")?.get("k")?.as_array()?;
    if kfs.len() != 1 || kfs[0].get("t").and_then(f).unwrap_or(0.0) != 0.0 {
        return None;
    }
    if t.get("p").and_then(Value::as_object).map(|p| !p.is_empty()).unwrap_or(false) {
        return None;
    }
    let m_default = t
        .get("m")
        .map(|m| {
            m.get("g").and_then(f).unwrap_or(1.0) == 1.0
                && m.get("a").and_then(|a| a.get("k")).map(|k| floats_of(k).iter().all(|x| *x == 0.0)).unwrap_or(true)
        })
        .unwrap_or(true);
    if !m_default {
        return None;
    }
    let doc = kfs[0].get("s")?.as_object()?;
    let allowed = ["t", "f", "s", "fc", "sc", "sw", "j", "tr", "lh", "sz"];
    if doc.keys().any(|k| !allowed.contains(&k.as_str())) {
        return None;
    }
    let mut o = Obj::new();
    o.insert("text".into(), doc.get("t").cloned().unwrap_or(json!("")));
    o.insert("font".into(), doc.get("f").cloned().unwrap_or(json!("sans")));
    o.insert("size".into(), canon(doc.get("s").cloned().unwrap_or(json!(24))));
    let fc = doc.get("fc").map(floats_of).unwrap_or(vec![0.0, 0.0, 0.0]);
    o.insert("color".into(), decompile_color(&fc, None));
    if let Some(sc) = doc.get("sc") {
        o.insert("strokeColor".into(), decompile_color(&floats_of(sc), None));
        o.insert("strokeWidth".into(), canon(doc.get("sw").cloned().unwrap_or(json!(1))));
    }
    match doc.get("j").and_then(f).unwrap_or(0.0) as i64 {
        0 => {}
        1 => {
            o.insert("align".into(), json!("right"));
        }
        2 => {
            o.insert("align".into(), json!("center"));
        }
        _ => return None,
    }
    for (k, key) in [("tr", "tracking"), ("lh", "lineHeight"), ("sz", "box")] {
        if let Some(v) = doc.get(k) {
            o.insert(key.into(), canon(v.clone()));
        }
    }
    if let Some(Value::Array(a)) = t.get("a") {
        if !a.is_empty() {
            o.insert("animators".into(), canon(Value::Array(a.clone())));
        }
    }
    Some(Value::Object(o))
}

fn shape_name(ty: &str, o: &Obj) -> Option<&'static str> {
    Some(match ty {
        "gr" => "group",
        "rc" => "rect",
        "el" => "ellipse",
        "sr" if o.get("sy").and_then(f) == Some(2.0) => "polygon",
        "sr" => "star",
        "sh" => "path",
        "fl" => "fill",
        "st" => "stroke",
        "gf" => "gradient-fill",
        "gs" => "gradient-stroke",
        "tm" => "trim",
        "rp" => "repeater",
        "rd" => "round-corners",
        "pb" => "pucker-bloat",
        "zz" => "zig-zag",
        "op" => "offset-path",
        "tw" => "twist",
        "mm" => "merge",
        _ => return None,
    })
}

fn decompile_shapes(d: &mut Decompiler, shapes: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for s in shapes {
        let Some(o) = s.as_object() else { continue };
        let ty = o.get("ty").and_then(Value::as_str).unwrap_or("");
        if ty == "tr" {
            continue;
        }
        let Some(name) = shape_name(ty, o) else {
            // Unknown items survive raw inside an empty group (skipped by players).
            out.push(json!({"type": "lottie", "lottie": canon(s.clone())}));
            continue;
        };
        // Continuation subpaths rejoin the previous path item.
        if ty == "sh" && o.get("mn").and_then(Value::as_str) == Some("eui:cont") {
            if let (Some(prev), Some(ks)) = (out.last_mut(), o.get("ks")) {
                if prev.get("type").and_then(Value::as_str) == Some("path") {
                    if let Some(v) = d.prop(ks, Val::Path) {
                        if let Some(joined) = join_paths(prev.get("d"), &v) {
                            prev["d"] = joined;
                            continue;
                        }
                    }
                }
            }
        }
        let mut so = Obj::new();
        let mut raw = Obj::new();
        so.insert("type".into(), json!(name));
        if let Some(nm) = o.get("nm") {
            so.insert("name".into(), nm.clone());
        }
        if o.get("hd").and_then(Value::as_bool) == Some(true) {
            so.insert("hidden".into(), json!(true));
        }
        let dv = o.get("d").and_then(f);
        let ccw = match ty {
            // lottie-web: a rect without `d` (or `d: 3`) is counter-clockwise.
            "rc" => !(dv == Some(1.0) || dv == Some(2.0)),
            "el" | "sr" => dv == Some(3.0),
            _ => false,
        };
        if ccw {
            so.insert("direction".into(), json!("ccw"));
        }
        let mut consumed: Vec<&str> = vec!["ty", "nm", "hd", "d", "mn"];
        match ty {
            "gr" => {
                let items = o.get("it").and_then(Value::as_array).cloned().unwrap_or_default();
                so.insert("shapes".into(), Value::Array(decompile_shapes(d, &items)));
                if let Some(tr) = items.iter().find(|i| i.get("ty").and_then(Value::as_str) == Some("tr")) {
                    if let Some(Value::Object(t)) = d.transform(tr) {
                        if !t.is_empty() {
                            so.insert("transform".into(), Value::Object(t));
                        }
                    }
                }
                consumed.push("it");
            }
            "rc" => {
                d.field(o, "p", &mut so, &mut raw, "position", Val::Plain, Some(json!([0, 0])));
                d.field(o, "s", &mut so, &mut raw, "size", Val::Plain, None);
                d.field(o, "r", &mut so, &mut raw, "radius", Val::Plain, Some(json!(0)));
                consumed.extend(["p", "s", "r"]);
            }
            "el" => {
                d.field(o, "p", &mut so, &mut raw, "position", Val::Plain, Some(json!([0, 0])));
                d.field(o, "s", &mut so, &mut raw, "size", Val::Plain, None);
                consumed.extend(["p", "s"]);
            }
            "sr" => {
                d.field(o, "p", &mut so, &mut raw, "position", Val::Plain, Some(json!([0, 0])));
                d.field(o, "pt", &mut so, &mut raw, "points", Val::Plain, Some(json!(5)));
                d.field(o, "or", &mut so, &mut raw, "outerRadius", Val::Plain, Some(json!(50)));
                d.field(o, "os", &mut so, &mut raw, "outerRoundness", Val::Plain, Some(json!(0)));
                d.field(o, "r", &mut so, &mut raw, "rotation", Val::Plain, Some(json!(0)));
                if name == "star" {
                    d.field(o, "ir", &mut so, &mut raw, "innerRadius", Val::Plain, Some(json!(25)));
                    d.field(o, "is", &mut so, &mut raw, "innerRoundness", Val::Plain, Some(json!(0)));
                }
                consumed.extend(["p", "pt", "or", "os", "r", "ir", "is", "sy"]);
            }
            "sh" => {
                let closed = o.get("closed").and_then(Value::as_bool).unwrap_or(false);
                let mut fixed = o.clone();
                if let Some(ks) = crate::import::with_closed(o.get("ks"), closed) {
                    fixed.insert("ks".into(), ks);
                }
                d.field(&fixed, "ks", &mut so, &mut raw, "d", Val::Path, None);
                consumed.push("ks");
            }
            "fl" | "gf" => {
                if ty == "fl" {
                    d.field(o, "c", &mut so, &mut raw, "color", Val::Color, None);
                    consumed.push("c");
                } else {
                    decompile_gradient(d, o, &mut so, &mut raw);
                    consumed.extend(["t", "s", "e", "h", "a", "g"]);
                }
                d.field(o, "o", &mut so, &mut raw, "opacity", Val::Plain, Some(json!(100)));
                if o.get("r").and_then(f) == Some(2.0) {
                    so.insert("rule".into(), json!("evenodd"));
                }
                consumed.extend(["o", "r"]);
            }
            "st" | "gs" => {
                if ty == "st" {
                    d.field(o, "c", &mut so, &mut raw, "color", Val::Color, None);
                    consumed.push("c");
                } else {
                    decompile_gradient(d, o, &mut so, &mut raw);
                    consumed.extend(["t", "s", "e", "h", "a", "g"]);
                }
                d.field(o, "o", &mut so, &mut raw, "opacity", Val::Plain, Some(json!(100)));
                d.field(o, "w", &mut so, &mut raw, "width", Val::Plain, None);
                match o.get("lc").and_then(f).unwrap_or(2.0) as i64 {
                    1 => {
                        so.insert("cap".into(), json!("butt"));
                    }
                    3 => {
                        so.insert("cap".into(), json!("square"));
                    }
                    _ => {}
                }
                match o.get("lj").and_then(f).unwrap_or(2.0) as i64 {
                    1 => {
                        so.insert("join".into(), json!("miter"));
                    }
                    3 => {
                        so.insert("join".into(), json!("bevel"));
                    }
                    _ => {}
                }
                if let Some(ml) = o.get("ml") {
                    so.insert("miterLimit".into(), canon(ml.clone()));
                }
                if let Some(Value::Array(ds)) = o.get("d") {
                    let mut dash = Vec::new();
                    let mut offset = None;
                    let mut ok = true;
                    for e in ds {
                        let kind = e.get("n").and_then(Value::as_str).unwrap_or("d");
                        let v = e.get("v").and_then(|v| d.prop(v, Val::Plain));
                        match (kind, v) {
                            ("o", Some(v)) if offset.is_none() => offset = Some(v),
                            ("d", Some(v)) if dash.len() % 2 == 0 && offset.is_none() => dash.push(v),
                            ("g", Some(v)) if dash.len() % 2 == 1 && offset.is_none() => dash.push(v),
                            _ => ok = false,
                        }
                    }
                    if ok {
                        so.insert("dash".into(), Value::Array(dash));
                        if let Some(v) = offset {
                            so.insert("dashOffset".into(), v);
                        }
                    } else {
                        raw.insert("d".into(), canon(Value::Array(ds.clone())));
                    }
                }
                consumed.extend(["o", "w", "lc", "lj", "ml"]);
            }
            "tm" => {
                d.field(o, "s", &mut so, &mut raw, "start", Val::Plain, Some(json!(0)));
                d.field(o, "e", &mut so, &mut raw, "end", Val::Plain, Some(json!(100)));
                d.field(o, "o", &mut so, &mut raw, "offset", Val::Plain, Some(json!(0)));
                if o.get("m").and_then(f) == Some(2.0) {
                    so.insert("mode".into(), json!("individual"));
                }
                consumed.extend(["s", "e", "o", "m"]);
            }
            "rp" => {
                d.field(o, "c", &mut so, &mut raw, "copies", Val::Plain, None);
                d.field(o, "o", &mut so, &mut raw, "offset", Val::Plain, Some(json!(0)));
                if o.get("m").and_then(f) == Some(2.0) {
                    so.insert("composite".into(), json!("below"));
                }
                if let Some(tr) = o.get("tr") {
                    if let Some(Value::Object(t)) = d.transform(tr) {
                        if !t.is_empty() {
                            so.insert("transform".into(), Value::Object(t));
                        }
                    }
                }
                consumed.extend(["c", "o", "m", "tr"]);
            }
            "rd" => {
                d.field(o, "r", &mut so, &mut raw, "radius", Val::Plain, None);
                consumed.push("r");
            }
            "pb" => {
                d.field(o, "a", &mut so, &mut raw, "amount", Val::Plain, None);
                consumed.push("a");
            }
            "zz" => {
                d.field(o, "s", &mut so, &mut raw, "size", Val::Plain, None);
                d.field(o, "r", &mut so, &mut raw, "ridges", Val::Plain, None);
                let pt = o.get("pt").map(|p| floats_of(p.get("k").unwrap_or(p))).unwrap_or_default();
                if pt.first() == Some(&2.0) {
                    so.insert("smooth".into(), json!(true));
                }
                consumed.extend(["s", "r", "pt"]);
            }
            "op" => {
                d.field(o, "a", &mut so, &mut raw, "amount", Val::Plain, None);
                match o.get("lj").and_then(f).unwrap_or(1.0) as i64 {
                    2 => {
                        so.insert("join".into(), json!("round"));
                    }
                    3 => {
                        so.insert("join".into(), json!("bevel"));
                    }
                    _ => {}
                }
                d.field(o, "ml", &mut so, &mut raw, "miterLimit", Val::Plain, None);
                consumed.extend(["a", "lj", "ml"]);
            }
            "tw" => {
                d.field(o, "a", &mut so, &mut raw, "angle", Val::Plain, None);
                d.field(o, "c", &mut so, &mut raw, "center", Val::Plain, None);
                consumed.extend(["a", "c"]);
            }
            "mm" => {
                let m = ["merge", "merge", "add", "subtract", "intersect", "exclude"];
                let k = o.get("mm").and_then(f).unwrap_or(1.0) as usize;
                if k > 1 {
                    so.insert("mode".into(), json!(m.get(k).copied().unwrap_or("merge")));
                }
                consumed.push("mm");
            }
            _ => {}
        }
        for (k, v) in o {
            if consumed.contains(&k.as_str()) || META_KEYS.contains(&k.as_str()) {
                continue;
            }
            if (k == "bm" && v.as_f64() == Some(0.0)) || (k == "hd" && v.as_bool() == Some(false)) {
                continue;
            }
            raw.insert(k.clone(), canon(v.clone()));
        }
        if !raw.is_empty() {
            so.insert("lottie".into(), Value::Object(raw));
        }
        out.push(Value::Object(so));
    }
    out
}

/// Join a continuation subpath onto a path item's `d` (static or keyed).
fn join_paths(prev: Option<&Value>, next: &Value) -> Option<Value> {
    match (prev?, next) {
        (Value::String(a), Value::String(b)) => Some(json!(format!("{a} {b}"))),
        (Value::Object(a), Value::Object(b)) => {
            let (ka, kb) = (a.get("keys")?.as_array()?, b.get("keys")?.as_array()?);
            if ka.len() != kb.len() {
                return None;
            }
            let mut keys = Vec::new();
            for (x, y) in ka.iter().zip(kb) {
                if x.get("t") != y.get("t") || x.get("ease") != y.get("ease") || x.get("to") != y.get("to") || x.get("ti") != y.get("ti") {
                    return None;
                }
                let mut k = x.clone();
                if let (Some(va), Some(vb)) = (x.get("v").and_then(Value::as_str), y.get("v").and_then(Value::as_str)) {
                    k["v"] = json!(format!("{va} {vb}"));
                } else if x.get("v").is_some() != y.get("v").is_some() {
                    return None;
                }
                match (x.get("end").and_then(Value::as_str), y.get("end").and_then(Value::as_str)) {
                    (Some(ea), Some(eb)) => k["end"] = json!(format!("{ea} {eb}")),
                    (None, None) => {}
                    _ => return None,
                }
                keys.push(k);
            }
            Some(json!({"keys": keys}))
        }
        _ => None,
    }
}

fn decompile_gradient(d: &mut Decompiler, o: &Obj, so: &mut Obj, raw: &mut Obj) {
    let radial = o.get("t").and_then(f) == Some(2.0);
    if radial {
        so.insert("kind".into(), json!("radial"));
    }
    d.field(o, "s", so, raw, "start", Val::Plain, None);
    d.field(o, "e", so, raw, "end", Val::Plain, None);
    if radial {
        d.field(o, "h", so, raw, "highlight", Val::Plain, Some(json!(0)));
        d.field(o, "a", so, raw, "highlightAngle", Val::Plain, Some(json!(0)));
    }
    let Some(g) = o.get("g") else { return };
    let count = g.get("p").and_then(f).unwrap_or(0.0) as usize;
    let stops_of = |flat: &[f64]| -> Option<Value> {
        let cols = flat.get(..count * 4)?;
        let rest = &flat[count * 4..];
        let alphas: Vec<(f64, f64)> = rest.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        if rest.len() % 2 != 0 || (!alphas.is_empty() && (alphas.len() != count || alphas.iter().zip(cols.chunks_exact(4)).any(|(a, c)| a.0 != c[0]))) {
            return None;
        }
        let mut out = Vec::new();
        for (k, c) in cols.chunks_exact(4).enumerate() {
            let a = alphas.get(k).map(|x| x.1).unwrap_or(1.0);
            out.push(json!([n(c[0]), decompile_color(&[c[1], c[2], c[3], a], None)]));
        }
        Some(Value::Array(out))
    };
    let stops = g.get("k").and_then(|k| match d.prop(k, Val::Plain) {
        Some(Value::Object(keyed)) => {
            let mut ok = true;
            let keys: Vec<Value> = keyed
                .get("keys")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|key| {
                    let mut key = key.clone();
                    for field in ["v", "end"] {
                        if let Some(v) = key.get(field).cloned() {
                            match stops_of(&floats_of(&v)) {
                                Some(s) => key[field] = s,
                                None => ok = false,
                            }
                        }
                    }
                    key
                })
                .collect();
            ok.then(|| json!({"keys": keys}))
        }
        Some(v) => stops_of(&floats_of(&v)),
        None => None,
    });
    match stops {
        Some(s) => {
            so.insert("stops".into(), s);
        }
        None => {
            raw.insert("g".into(), canon(g.clone()));
        }
    }
}
