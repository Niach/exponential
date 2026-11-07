//! Round 8: replays `packages/exponential-ui/fixtures/canvas.json`, THE
//! Canvas contract (compile + validation, the sampled display lists with
//! their text ops, the fit mapping, the pointer inversion) every renderer
//! shares: here the engine itself (gpui paints these lists), in TS over the
//! WASM `CanvasPlayer`, in Swift and Kotlin over uniffi.
//!
//! `UPDATE_CANVAS_FIXTURES=1 cargo test -p eui-motion --test canvas`
//! regenerates every expected value from the engine (inputs kept) and
//! rewrites `$comment`.

mod common;

use common::{close, fixture_path, write_fixture};
use eui_motion::canvas::{fit_matrix, to_drawing, Canvas, CanvasLimits, Fit};
use eui_motion::{DisplayList, Op};
use serde_json::{json, Map, Value};

const COMMENT: &str = "Round 8 (docs/round-8-contract.md §1): THE Canvas contract, replayed by the engine (apps/desktop/crates/eui-motion/tests/canvas.rs; gpui paints these lists), the TS core over the WASM CanvasPlayer (src/canvas.test.ts) and the Swift and Kotlin painters over uniffi. AUTO-GENERATED expected values: `UPDATE_CANVAS_FIXTURES=1 cargo test -p eui-motion --test canvas` (inputs kept). A Canvas node's `{size, shapes}` compiles (eui-motion `canvas::Canvas::compile`) into a ONE-FRAME Motion and samples into the Motion display list (fixtures/motion-frames.json documents every op) plus `text` ops. `compile`: `canvas` = the node's {size, shapes}, `slots` (optional) = the host's slot overrides, exactly a Motion player's (`$color.<token>` → {p: {a: 0, k: [r, g, b, a]}} from the theme, `$mode`: dark picks the dark half of {light, dark} pairs); expected `slotIds` = every slot the canvas declares (sorted: the token slots a host fills, `ld:<light>:<dark>` pairs), `list` (serde JSON) and `packed` (the f32 stream, VERSION 2). THE TEXT RULE: every `text` op comes after every other op (after the frame's final `pop`), in SOURCE order, whatever the shapes around it; {op: text, transform, text, font: sans|mono, weight: regular|medium|semibold|bold, size, align: start|center|end, baseline: top|middle|alphabetic, color}: the anchor is the op's local origin, so a painter maps (0, 0) through fit × transform and sets ONE line (never wrapped; a line break in the source became a space) UPRIGHT: a text op takes ONLY the translation and the uniform scale √|det| of (fit × its transform), rotation and skew never apply to text (gpui cannot rotate text, so one look everywhere), so the line is horizontal at font px `size` × √|det|, in the theme's `$type.family.sans` / `$type.family.mono` at that CSS weight; `align` = where x is: start = the line's left edge, center = its middle, end = its right edge (physical: a drawing never mirrors); `baseline` = where y is: top = baseline at y + ascent, middle = baseline at y + (ascent − descent) / 2, alphabetic = the baseline (ascent, descent = the font's own hhea ascender and descender magnitudes at that size, what every native text system reports; no leading); the colour is straight RGBA, the groups' opacity already multiplied into alpha. Packed text: 7, t[6], font (0 sans, 1 mono), weight (400, 500, 600, 700), size, align (0 start, 1 center, 2 end), baseline (0 top, 1 middle, 2 alphabetic), r, g, b, a, n, then n UTF-16 code units. Text is never outlined by the engine and the tiny-skia reference skips it (the gpui painter rasters the shapes, then draws native text on top). `refuse`: the expected `issues` [{code, path, message}]; a replay compares `code` and `path` (in order), the message is the engine's: `canvas-animated` (any `{keys}`: a Canvas is static, Motion animates), `canvas-shape-type` (outside rect, ellipse, star, polygon, path, fill, stroke, gradient-fill, trim, repeater, round-corners, group, text), `canvas-lottie` (raw `lottie` keys), `canvas-text` (a text field), `color`, `size`, `shapes`, the dialect's own codes with their paths under /shapes, and the budget: `limit-ops` (ops per frame > limits.json canvas.maxOps), `limit-texts` (> canvas.maxTexts), `limit-text` (characters > canvas.maxTextChars), `limit-shapes` / `limit-bytes` (the Motion limits on the compiled one-frame Lottie; vertices are not counted, as for a Motion). `fit`: drawing `size` [w, h] in a `box` [w, h] px under `fit` → the `matrix` [a, b, c, d, e, f] (drawing units → box px): contain = uniform min(bw/w, bh/h) centred, cover = uniform max(…) centred (the overflow clipped by the box), fill = bw/w, bh/h from the top-left. `pointer`: what `press` / `move` carry with `pointer: true`: a point `at` [x, y] box px (relative to the Canvas box's top-left) → `drawing` [x, y] = the fit mapping inverted, rounded to two decimals (compare within 0.01); a point in a contain letterbox maps outside 0..size, never clamped. `upright`: a `canvas` in a `box` [w, h] px under `fit` → its `texts` in source order, {text, anchor [x, y] box px = (fit × the op's transform) applied to (0, 0), size = the op's size × √|det(fit × transform)|} (compare within 1e-3): the text-is-upright rule, whatever rotation, skew or non-uniform scale its groups carry.";

fn packed_json(list: &DisplayList) -> Value {
    let mut packed = Vec::new();
    list.encode(&mut packed);
    Value::Array(
        packed
            .iter()
            .map(|x| {
                let v: f64 = format!("{x}").parse().unwrap();
                json!(v)
            })
            .collect(),
    )
}

fn sample(canvas: &Value, slots: Option<&Value>) -> Result<(Value, Value, Value), Value> {
    let c = Canvas::compile(canvas).map_err(|issues| serde_json::to_value(&issues).unwrap())?;
    let slots: Map<String, Value> = slots.and_then(Value::as_object).cloned().unwrap_or_default();
    let mut list = DisplayList::default();
    c.render(&slots, &mut list);
    let list_json: Value = serde_json::from_str(&serde_json::to_string(&list).unwrap()).unwrap();
    Ok((json!(c.slot_ids()), list_json, packed_json(&list)))
}

/// `a × b` for `[a, b, c, d, e, f]` affine matrices.
fn upright_concat(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3], a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]]
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

#[test]
fn canvas_fixture() {
    let path = fixture_path("canvas.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let update = std::env::var("UPDATE_CANVAS_FIXTURES").is_ok();
    let mut problems = Vec::new();
    let mut out = Map::new();
    out.insert("$comment".into(), json!(COMMENT));
    let budget = CanvasLimits::default();
    out.insert("limits".into(), json!({"maxOps": budget.max_ops, "maxTexts": budget.max_texts, "maxTextChars": budget.max_text_chars}));

    let mut compiled = Vec::new();
    for case in fixture["compile"].as_array().expect("compile") {
        let id = case["id"].as_str().unwrap();
        match sample(&case["canvas"], case.get("slots")) {
            Ok((slot_ids, list, packed)) => {
                if update {
                    let mut m = Map::new();
                    for k in ["id", "covers", "canvas", "slots"] {
                        if let Some(v) = case.get(k) {
                            m.insert(k.into(), v.clone());
                        }
                    }
                    m.insert("slotIds".into(), slot_ids);
                    m.insert("list".into(), list);
                    m.insert("packed".into(), packed);
                    compiled.push(Value::Object(m));
                } else {
                    for (got, want, at) in [(&slot_ids, &case["slotIds"], "slotIds"), (&list, &case["list"], "list"), (&packed, &case["packed"], "packed")] {
                        if let Err(e) = close(got, want, at) {
                            problems.push(format!("{id}: {e}"));
                        }
                    }
                }
            }
            Err(issues) => problems.push(format!("{id}: refused {issues}")),
        }
    }

    let mut refused = Vec::new();
    for case in fixture["refuse"].as_array().expect("refuse") {
        let id = case["id"].as_str().unwrap();
        match sample(&case["canvas"], None) {
            Ok(_) => problems.push(format!("{id}: compiled, expected a refusal")),
            Err(issues) => {
                if update {
                    let mut m = Map::new();
                    for k in ["id", "covers", "canvas"] {
                        m.insert(k.into(), case[k].clone());
                    }
                    m.insert("issues".into(), issues);
                    refused.push(Value::Object(m));
                } else {
                    let pick = |v: &Value| -> Vec<(String, String)> {
                        v.as_array().into_iter().flatten().map(|i| (i["code"].as_str().unwrap_or("").to_string(), i["path"].as_str().unwrap_or("").to_string())).collect()
                    };
                    if pick(&issues) != pick(&case["issues"]) {
                        problems.push(format!("{id}: issues {:?} != {:?}", pick(&issues), pick(&case["issues"])));
                    }
                }
            }
        }
    }

    let mut fits = Vec::new();
    for case in fixture["fit"].as_array().expect("fit") {
        let n = |v: &Value, i: usize| v[i].as_f64().unwrap();
        let m = fit_matrix(n(&case["size"], 0), n(&case["size"], 1), n(&case["box"], 0), n(&case["box"], 1), Fit::parse(case["fit"].as_str().unwrap()));
        let got = json!(m);
        if update {
            let mut c = case.as_object().unwrap().clone();
            c.insert("matrix".into(), got);
            fits.push(Value::Object(c));
        } else if let Err(e) = close(&got, &case["matrix"], "matrix") {
            problems.push(format!("fit {case}: {e}"));
        }
    }

    let mut pointers = Vec::new();
    for case in fixture["pointer"].as_array().expect("pointer") {
        let n = |v: &Value, i: usize| v[i].as_f64().unwrap();
        let fit = Fit::parse(case["fit"].as_str().unwrap());
        let mut points = Vec::new();
        for p in case["points"].as_array().unwrap() {
            let d = to_drawing(n(&p["at"], 0), n(&p["at"], 1), n(&case["size"], 0), n(&case["size"], 1), n(&case["box"], 0), n(&case["box"], 1), fit).unwrap();
            if update {
                points.push(json!({"at": p["at"], "drawing": d}));
            } else {
                let want = &p["drawing"];
                if (d[0] - want[0].as_f64().unwrap()).abs() > 0.01 || (d[1] - want[1].as_f64().unwrap()).abs() > 0.01 {
                    problems.push(format!("pointer {} {} at {}: {d:?} != {want}", case["fit"], case["box"], p["at"]));
                }
            }
        }
        if update {
            let mut c = case.as_object().unwrap().clone();
            c.insert("points".into(), Value::Array(points));
            pointers.push(Value::Object(c));
        }
    }

    let mut uprights = Vec::new();
    for case in fixture["upright"].as_array().expect("upright") {
        let id = case["id"].as_str().unwrap();
        let n = |v: &Value, i: usize| v[i].as_f64().unwrap();
        let c = Canvas::compile(&case["canvas"]).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        let mut list = DisplayList::default();
        c.render(&Map::new(), &mut list);
        let fit = fit_matrix(n(&case["canvas"]["size"], 0), n(&case["canvas"]["size"], 1), n(&case["box"], 0), n(&case["box"], 1), Fit::parse(case["fit"].as_str().unwrap()));
        let texts: Vec<Value> = list
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Text { transform, text, size, .. } => {
                    let m = upright_concat(fit, transform.map(f64::from));
                    let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
                    Some(json!({"text": text, "anchor": [round6(m[4]), round6(m[5])], "size": round6(*size as f64 * scale)}))
                }
                _ => None,
            })
            .collect();
        if update {
            let mut m = case.as_object().unwrap().clone();
            m.insert("texts".into(), Value::Array(texts));
            uprights.push(Value::Object(m));
        } else if let Err(e) = close(&json!(texts), &case["texts"], "texts") {
            problems.push(format!("upright {id}: {e}"));
        }
    }

    if update {
        out.insert("compile".into(), Value::Array(compiled));
        out.insert("refuse".into(), Value::Array(refused));
        out.insert("fit".into(), Value::Array(fits));
        out.insert("pointer".into(), Value::Array(pointers));
        out.insert("upright".into(), Value::Array(uprights));
        write_fixture(&path, &Value::Object(out), &["canvas", "slots", "slotIds", "packed", "path", "transform", "color", "issues", "matrix", "size", "box", "at", "drawing", "anchor"]);
        return;
    }
    assert!(problems.is_empty(), "{} mismatch(es) (regenerate with UPDATE_CANVAS_FIXTURES=1 if intended):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn every_shape_kind_and_text_field_is_covered() {
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path("canvas.json")).unwrap()).unwrap();
    let inputs = serde_json::to_string(&fixture["compile"]).unwrap();
    for t in eui_motion::canvas::SHAPE_TYPES {
        assert!(inputs.contains(&format!(r#""type":"{t}""#)), "no compile case draws a `{t}`");
    }
    for needle in [
        r#""weight":"medium""#,
        r#""weight":"semibold""#,
        r#""weight":"bold""#,
        r#""align":"center""#,
        r#""align":"end""#,
        r#""baseline":"top""#,
        r#""baseline":"middle""#,
        r#""font":"mono""#,
    ] {
        assert!(inputs.contains(needle), "no compile case sets {needle}");
    }
    let refusals = serde_json::to_string(&fixture["refuse"]).unwrap();
    for code in ["canvas-animated", "canvas-shape-type", "canvas-lottie", "canvas-text", "limit-ops"] {
        assert!(refusals.contains(&format!(r#""code":"{code}""#)), "no refusal case yields `{code}`");
    }
}

#[test]
fn limits_match_the_catalog() {
    let limits: Value = serde_json::from_slice(&std::fs::read(fixture_path("../catalog/limits.json")).unwrap()).unwrap();
    let c = &limits["motion"]["canvas"];
    let d = CanvasLimits::default();
    assert_eq!(c["maxOps"].as_u64(), Some(d.max_ops as u64));
    assert_eq!(c["maxTexts"].as_u64(), Some(d.max_texts as u64));
    assert_eq!(c["maxTextChars"].as_u64(), Some(d.max_text_chars as u64));
}

#[test]
fn text_limits_refuse() {
    let many: Vec<Value> = (0..501).map(|i| json!({"type": "text", "text": "x", "position": [i, 0]})).collect();
    let e = Canvas::compile(&json!({"size": [10, 10], "shapes": many})).err().expect("refused");
    assert_eq!(e[0].code, "limit-texts");
    let long = "y".repeat(1001);
    let e = Canvas::compile(&json!({"size": [10, 10], "shapes": [{"type": "text", "text": long}]})).err().expect("refused");
    assert_eq!(e[0].code, "limit-text");
}
