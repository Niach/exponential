//! EXP-1586: fixtures/zoom.json replays through the core's JSON entry
//! (`zoom::zoom_request`, the same one the FFI's `zoom_json` serves SwiftUI
//! and Compose), every number within 1e-6.

mod support;

use serde_json::{json, Map, Value};

fn close(got: &Value, want: &Value, at: &str) -> Result<(), String> {
    match (got, want) {
        (Value::Number(g), Value::Number(w)) => {
            let (g, w) = (g.as_f64().unwrap(), w.as_f64().unwrap());
            if (g - w).abs() <= 1e-6 { Ok(()) } else { Err(format!("{at}: {g} vs {w}")) }
        }
        (Value::Array(g), Value::Array(w)) => {
            if g.len() != w.len() {
                return Err(format!("{at}: length {} vs {}", g.len(), w.len()));
            }
            g.iter().zip(w).enumerate().try_for_each(|(i, (g, w))| close(g, w, &format!("{at}[{i}]")))
        }
        (Value::Object(g), Value::Object(w)) => {
            let mut gk: Vec<_> = g.keys().collect();
            let mut wk: Vec<_> = w.keys().collect();
            gk.sort();
            wk.sort();
            if gk != wk {
                return Err(format!("{at}: keys {gk:?} vs {wk:?}"));
            }
            w.iter().try_for_each(|(k, w)| close(&g[k], w, &format!("{at}.{k}")))
        }
        _ if got == want => Ok(()),
        _ => Err(format!("{at}: {got} vs {want}")),
    }
}

/// The request one fixture case sends: the case + `op` + its frame object.
pub fn request(op: &str, case: &Value, frames: &Value) -> Value {
    let mut req: Map<String, Value> = case.as_object().cloned().unwrap_or_default();
    req.remove("expected");
    req.insert("op".into(), json!(op));
    if let Some(name) = case.get("frame").and_then(Value::as_str) {
        let mut frame = frames[name].clone();
        if let Some(vp) = case.get("viewport") {
            frame["viewport"] = vp.clone();
        }
        req.insert("frame".into(), frame);
    }
    Value::Object(req)
}

#[test]
fn zoom_json_every_group_within_1e_6() {
    let f = support::fixture("zoom.json");
    let groups = [
        "rest", "clamp", "zoomTo", "wheel", "scroll", "ladder", "steps", "doubleTap", "keys", "rubberBand", "drag", "pinch",
        "momentum", "momentumActive", "panRoom", "resize", "menu", "bounds",
    ];
    let mut failed = Vec::new();
    let mut count = 0;
    for op in groups {
        for c in f[op].as_array().unwrap_or_else(|| panic!("zoom.json has no {op}")) {
            count += 1;
            let name = format!("{op} {}", c["name"].as_str().unwrap_or(""));
            match eui_core::zoom::zoom_request(&request(op, c, &f["frames"])) {
                Ok(got) => {
                    if let Err(e) = close(&got, &c["expected"], &name) {
                        failed.push(e);
                    }
                }
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
    }
    for p in f["percent"].as_array().unwrap() {
        count += 1;
        let got = eui_core::zoom::zoom_request(&json!({"op": "percent", "scale": p[0]})).unwrap();
        if got != p[1] {
            failed.push(format!("percent {}: {got} vs {}", p[0], p[1]));
        }
    }
    assert!(count >= 80, "{count} cases");
    assert!(failed.is_empty(), "zoom.json:\n  {}", failed.join("\n  "));
}
