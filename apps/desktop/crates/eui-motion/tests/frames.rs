//! EXP-1602: replays `packages/exponential-ui/fixtures/motion-frames.json`,
//! the display-list contract every eUI Motion painter shares (later also
//! replayed over WASM in TS and over uniffi in Swift and Kotlin).
//!
//! `UPDATE_MOTION_FIXTURES=1 cargo test -p eui-motion --test frames`
//! regenerates every expected `list` and `packed` from the engine (the
//! inputs, `id`/`covers`/`lottie`/`frame`, are kept) and rewrites `$comment`.

mod common;

use common::{close, fixture_path, write_fixture};
use eui_motion::{DisplayList, Motion};
use serde_json::{json, Map, Value};

/// The fixture's spec (written into `$comment` on regeneration).
const COMMENT: &str = "EXP-1602: eUI Motion display lists, the contract every painter replays (apps/desktop/crates/eui-motion/src/display.rs; packed encoding = README.md › Packed frames). AUTO-GENERATED expected values: `UPDATE_MOTION_FIXTURES=1 cargo test -p eui-motion --test frames` (inputs kept). Each case: `id`, `covers` (what it exercises), `lottie` (a small inline Lottie document; load it as UTF-8 JSON with the default limits), `frames`: per sampled composition frame `frame` (Motion.sample(frame)) the expected `list` and `packed`. `list` = the DisplayList as serde JSON: {width, height, ops}; every op is an object tagged by `op`: `layer` {blend, alpha, transform, clip, filters?} opens an isolated group composited on its `pop` with `alpha` and `blend`, clipped to `clip` (empty = unclipped); `clip` {transform, path} intersects the clip until its `pop`; `pop` closes the innermost layer or clip; `fill` {transform, rule: nonzero|evenodd, brush, path}; `stroke` {transform, style: {width, cap: butt|round|square, join: miter|round|bevel, miter, dashes? (omitted when solid), dash_offset}, brush, path}; `image` {transform, asset (index into the motion's image assets), alpha, width, height} draws the asset into the rect 0,0,width,height. `transform` = [a, b, c, d, e, f] (x' = a·x + c·y + e, y' = b·x + d·y + f), in composition units, never inherited: there is no ambient transform. `path` = verbs tagged by `v`: {v:m,x,y} | {v:l,x,y} | {v:c,x1,y1,x2,y2,x,y} | {v:z}. `brush` tagged by `kind`: solid {color} | linear {start, end, stops} | radial {center, focal, radius, stops}; colours are straight sRGB RGBA in 0..1, stops {offset, color}. `blend` (kebab-case): normal, multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity, then the compositing operators dst-in, dst-out (alpha mattes, masks), luma-in, luma-out (luma mattes), xor (merge paths); `filters` (omitted when empty) tagged by `kind`: blur {sigma_x, sigma_y} | color-matrix {m[20]} | gradient-map {stops, amount} | drop-shadow {color, dx, dy, sigma}. `packed` = the same frame as the f32 stream [VERSION=2, width, height, op…] (README.md › Packed frames; layer = 1, blend code, alpha, t[6], clip path, filter count, filters; version 2 = round 8: adds the `text` op only a Canvas emits, fixtures/canvas.json; a Motion's frames are unchanged apart from the version word). Replays compare `list` structurally (same keys, same array lengths, same strings) and `packed` element by element; numbers match within 1e-4 × max(1, |expected|).";

fn sample(lottie: &Value, frame: f64) -> (Value, Value) {
    let motion = Motion::load(serde_json::to_string(lottie).unwrap().as_bytes()).unwrap_or_else(|e| panic!("refused: {e:?}"));
    let mut list = DisplayList::default();
    motion.sample(frame, &mut list);
    // Through the text form: f32s print as their shortest representation.
    let list_json: Value = serde_json::from_str(&serde_json::to_string(&list).unwrap()).unwrap();
    let mut packed = Vec::new();
    list.encode(&mut packed);
    let packed_json = Value::Array(
        packed
            .iter()
            .map(|x| {
                let v: f64 = format!("{x}").parse().unwrap();
                json!(v)
            })
            .collect(),
    );
    (list_json, packed_json)
}

#[test]
fn display_lists_match_the_fixture() {
    let path = fixture_path("motion-frames.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let update = std::env::var("UPDATE_MOTION_FIXTURES").is_ok();
    let mut cases_out = Vec::new();
    let mut problems = Vec::new();
    let cases = fixture["cases"].as_array().expect("cases");
    assert!(cases.len() >= 20, "the fixture covers every op, brush, blend, mask and matte");
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let lottie = &case["lottie"];
        let mut frames_out = Vec::new();
        for f in case["frames"].as_array().unwrap() {
            let frame = f["frame"].as_f64().unwrap();
            let (list, packed) = sample(lottie, frame);
            if update {
                let mut m = Map::new();
                m.insert("frame".into(), f["frame"].clone());
                m.insert("list".into(), list);
                m.insert("packed".into(), packed);
                frames_out.push(Value::Object(m));
                continue;
            }
            if let Err(e) = close(&list, &f["list"], "list") {
                problems.push(format!("{id} @ {frame}: {e}"));
            }
            if let Err(e) = close(&packed, &f["packed"], "packed") {
                problems.push(format!("{id} @ {frame}: {e}"));
            }
        }
        if update {
            let mut c = Map::new();
            c.insert("id".into(), case["id"].clone());
            c.insert("covers".into(), case["covers"].clone());
            c.insert("lottie".into(), lottie.clone());
            c.insert("frames".into(), Value::Array(frames_out));
            cases_out.push(Value::Object(c));
        }
    }
    if update {
        let mut root = Map::new();
        root.insert("$comment".into(), json!(COMMENT));
        root.insert("cases".into(), Value::Array(cases_out));
        write_fixture(&path, &Value::Object(root), &["lottie"]);
        return;
    }
    assert!(problems.is_empty(), "{} mismatch(es) (regenerate with UPDATE_MOTION_FIXTURES=1 if the change is intended):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn every_op_brush_and_operator_is_covered() {
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path("motion-frames.json")).unwrap()).unwrap();
    let text = serde_json::to_string(&fixture["cases"]).unwrap();
    let mut missing = Vec::new();
    for needle in [
        r#""op":"layer""#,
        r#""op":"clip""#,
        r#""op":"pop""#,
        r#""op":"fill""#,
        r#""op":"stroke""#,
        r#""op":"image""#,
        r#""kind":"solid""#,
        r#""kind":"linear""#,
        r#""kind":"radial""#,
        r#""v":"m""#,
        r#""v":"l""#,
        r#""v":"c""#,
        r#""v":"z""#,
        r#""rule":"evenodd""#,
        r#""blend":"multiply""#,
        r#""blend":"difference""#,
        r#""blend":"dst-in""#,
        r#""blend":"dst-out""#,
        r#""blend":"luma-in""#,
        r#""blend":"luma-out""#,
        r#""dashes""#,
    ] {
        if !text.contains(needle) {
            missing.push(needle);
        }
    }
    assert!(missing.is_empty(), "no expected list contains {missing:?}");
}
