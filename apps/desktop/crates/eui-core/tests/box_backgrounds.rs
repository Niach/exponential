//! genUI round 4: `fixtures/box-backgrounds.json` on the Rust core: the
//! validator (`validate`), the radial geometry (`radial`), the picture
//! placement (`picture`) and the painted `Visual` (`visual`). The TS
//! reference, Swift and Kotlin replay the same file.

use eui_core::style::{picture_rect, radial_radius, radial_t, visual, BoxKind};
use eui_core::style_check::validate_style;
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture() -> Value {
    let path = format!("{FIXTURES}/box-backgrounds.json");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("json")
}

fn f(v: &Value) -> f32 {
    v.as_f64().expect("number") as f32
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// JSON equality with numbers compared as floats (an f32 `180.0` = `180`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-6,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| same(p, q))),
        _ => a == b,
    }
}

#[test]
fn validate_cases() {
    let file = fixture();
    let cases = file["validate"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let got = serde_json::to_value(validate_style(&c["style"], "style", None)).unwrap();
        assert_eq!(got, c["issues"], "{}", c["name"]);
    }
}

#[test]
fn radial_geometry() {
    let file = fixture();
    for c in file["radial"].as_array().unwrap() {
        let (w, h, cx, cy) = (f(&c["size"][0]), f(&c["size"][1]), f(&c["centre"][0]), f(&c["centre"][1]));
        assert!(close(radial_radius(w, h, cx, cy), f(&c["radius"])), "{}: radius", c["name"]);
        for s in c["samples"].as_array().unwrap() {
            assert!(close(radial_t(w, h, cx, cy, f(&s[0]), f(&s[1])), f(&s[2])), "{}: t at {s}", c["name"]);
        }
    }
}

#[test]
fn picture_placement() {
    let file = fixture();
    for c in file["picture"].as_array().unwrap() {
        let got = picture_rect(c["fit"].as_str().unwrap(), (f(&c["size"][0]), f(&c["size"][1])), (f(&c["image"][0]), f(&c["image"][1])), (f(&c["focal"][0]), f(&c["focal"][1])));
        let want = (f(&c["rect"][0]), f(&c["rect"][1]), f(&c["rect"][2]), f(&c["rect"][3]));
        assert!(close(got.0, want.0) && close(got.1, want.1) && close(got.2, want.2) && close(got.3, want.3), "{}: {got:?} / {want:?}", c["name"]);
    }
}

#[test]
fn painted_visual() {
    let file = fixture();
    for c in file["visual"].as_array().unwrap() {
        let v = visual(c["style"].as_object().unwrap(), BoxKind::Container);
        for key in ["backgroundImage", "backgroundGradient"] {
            let got = match key {
                "backgroundImage" => serde_json::to_value(&v.background_image).unwrap(),
                _ => serde_json::to_value(&v.background_gradient).unwrap(),
            };
            let want = c.get(key).cloned().unwrap_or(Value::Null);
            assert!(same(&got, &want), "{}: {key} = {got} / {want}", c["name"]);
        }
    }
}
