//! Validation: the eUI limits (`limits.json` › motion), expression refusal,
//! and the feature census that reports what the engine renders only
//! approximately (the parity-gap list).

use serde::Serialize;
use serde_json::Value;

/// One finding. `path` is a JSON pointer into the source.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Issue {
    pub code: String,
    pub path: String,
    pub message: String,
}

/// Input limits (EXP-1584 budget).
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_shapes: usize,
    pub max_seconds: f64,
    /// Checked per sampled frame by the player, not here.
    pub max_vertices: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 2 * 1024 * 1024,
            max_shapes: 5_000,
            max_seconds: 120.0,
            max_vertices: 100_000,
        }
    }
}

/// Errors refuse the animation; warnings are features rendered approximately.
#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
}

fn issue(code: &str, path: &str, message: impl Into<String>) -> Issue {
    Issue {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

/// Validate a Lottie source.
pub fn check(source: &[u8], limits: &Limits) -> Report {
    let mut report = Report::default();
    if source.len() > limits.max_bytes {
        report.errors.push(issue(
            "limit-bytes",
            "",
            format!("{} bytes > {}", source.len(), limits.max_bytes),
        ));
        return report;
    }
    let root: Value = match serde_json::from_slice(source) {
        Ok(v) => v,
        Err(e) => {
            report.errors.push(issue("parse", "", e.to_string()));
            return report;
        }
    };
    let fr = root.get("fr").and_then(Value::as_f64).unwrap_or(0.0);
    let ip = root.get("ip").and_then(Value::as_f64).unwrap_or(0.0);
    let op = root.get("op").and_then(Value::as_f64).unwrap_or(0.0);
    if fr <= 0.0 || op <= ip {
        report
            .errors
            .push(issue("timing", "", "needs fr > 0 and op > ip"));
    } else if (op - ip) / fr > limits.max_seconds {
        report.errors.push(issue(
            "limit-duration",
            "",
            format!("{:.1} s > {} s", (op - ip) / fr, limits.max_seconds),
        ));
    }
    let mut census = Census::default();
    walk(&root, "", &mut census, &mut report);
    if census.shapes > limits.max_shapes {
        report.errors.push(issue(
            "limit-shapes",
            "",
            format!("{} shapes > {}", census.shapes, limits.max_shapes),
        ));
    }
    report
}

#[derive(Default)]
struct Census {
    shapes: usize,
}

/// Features the spike engine (velato 0.12) does not render faithfully.
/// Every entry is a parity gap M1 closes; the list shrinks to empty.
fn gap_for_layer(layer: &serde_json::Map<String, Value>) -> Vec<(&'static str, String)> {
    let mut gaps = Vec::new();
    match layer.get("ty").and_then(Value::as_i64) {
        Some(5) => gaps.push(("gap-text", "text layer".into())),
        Some(13) => gaps.push(("camera", "3D camera layers are not rendered".into())),
        Some(1) => {}
        _ => {}
    }
    if layer.get("ddd").and_then(Value::as_i64) == Some(1) {
        gaps.push(("gap-3d", "3D layer flattened".into()));
    }
    if layer.contains_key("tm") {
        gaps.push(("gap-time-remap", "time remap ignored".into()));
    }
    if let Some(Value::Array(ef)) = layer.get("ef") {
        if !ef.is_empty() {
            gaps.push(("gap-effects", "layer effects ignored".into()));
        }
    }
    match layer.get("tt").and_then(Value::as_i64) {
        Some(3) | Some(4) => gaps.push(("gap-luma-matte", "luma matte drawn as alpha".into())),
        _ => {}
    }
    if let Some(Value::Array(masks)) = layer.get("masksProperties") {
        for m in masks {
            let mode = m.get("mode").and_then(Value::as_str).unwrap_or("a");
            if mode != "a" {
                gaps.push(("gap-mask-mode", format!("mask mode `{mode}` drawn as add")));
            }
            if m.get("inv").and_then(Value::as_bool) == Some(true) {
                gaps.push(("gap-mask-invert", "inverted mask".into()));
            }
            if m.get("x").is_some_and(|x| !is_zero_prop(x)) {
                gaps.push(("gap-mask-expansion", "mask expansion".into()));
            }
            if m.get("f").is_some_and(|x| !is_zero_prop(x)) {
                gaps.push(("gap-mask-feather", "mask feather".into()));
            }
        }
    }
    if layer.get("bm").and_then(Value::as_i64).unwrap_or(0) != 0 {
        // velato maps blend modes; recorded so the corpus shows coverage.
    }
    gaps
}

fn is_zero_prop(v: &Value) -> bool {
    match v.get("k") {
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::Array(a)) => a.iter().all(|x| x.as_f64() == Some(0.0)),
        _ => false,
    }
}

fn gap_for_shape(ty: &str) -> Option<(&'static str, &'static str)> {
    Some(match ty {
        "mm" => ("gap-merge-paths", "merge paths ignored"),
        "zz" => ("gap-zig-zag", "zig-zag ignored"),
        "op" => ("gap-offset-path", "offset path ignored"),
        "pb" => ("gap-pucker-bloat", "pucker/bloat ignored"),
        "tw" => ("gap-twist", "twist ignored"),
        "rd" => ("gap-round-corners", "round corners ignored"),
        _ => return None,
    })
}

fn walk(v: &Value, path: &str, census: &mut Census, report: &mut Report) {
    match v {
        Value::Object(map) => {
            // An animated property carrying an expression: `{"a":…,"k":…,"x":"…"}`.
            if map.contains_key("k") {
                if let Some(Value::String(_)) = map.get("x") {
                    report.errors.push(issue(
                        "expression",
                        &format!("{path}/x"),
                        "expressions are not supported",
                    ));
                }
            }
            if map.contains_key("ty") && map.contains_key("ks") && map.contains_key("ip") {
                for (code, msg) in gap_for_layer(map) {
                    report.warnings.push(issue(code, path, msg));
                }
            }
            if let Some(Value::String(ty)) = map.get("ty") {
                if path.contains("/shapes/") || path.contains("/it/") {
                    census.shapes += 1;
                    if let Some((code, msg)) = gap_for_shape(ty) {
                        report.warnings.push(issue(code, path, msg));
                    }
                    if (ty == "st" || ty == "gs") && map.get("d").is_some() {
                        report
                            .warnings
                            .push(issue("gap-dashes", path, "dashes ignored"));
                    }
                }
            }
            for (k, child) in map {
                walk(child, &format!("{path}/{k}"), census, report);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                walk(child, &format!("{path}/{i}"), census, report);
            }
        }
        _ => {}
    }
}
