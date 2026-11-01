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

/// Effect types the engine lowers (drop shadow, blur, fill, tint, tritone, stroke).
const EFFECTS: [i64; 6] = [20, 21, 22, 23, 25, 29];

/// What the engine reports but does not render (or renders flattened).
fn notes_for_layer(layer: &serde_json::Map<String, Value>) -> Vec<(&'static str, String)> {
    let mut notes = Vec::new();
    if layer.get("ty").and_then(Value::as_i64) == Some(13) {
        notes.push(("camera", "3D camera layers are not rendered".into()));
    }
    if layer.get("ddd").and_then(Value::as_i64) == Some(1) {
        notes.push(("3d-layer", "3D layer drawn flattened (z, x/y rotation ignored)".into()));
    }
    if let Some(Value::Array(ef)) = layer.get("ef") {
        for e in ef {
            let ty = e.get("ty").and_then(Value::as_i64).unwrap_or(-1);
            if !EFFECTS.contains(&ty) {
                notes.push(("effect", format!("effect type {ty} is not supported")));
            }
        }
    }
    if let Some(t) = layer.get("t") {
        if t.get("p").and_then(|p| p.get("m")).is_some() {
            notes.push(("text-path", "text on a path is drawn on its baseline".into()));
        }
    }
    notes
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
                for (code, msg) in notes_for_layer(map) {
                    report.warnings.push(issue(code, path, msg));
                }
            }
            if let Some(Value::String(_)) = map.get("ty") {
                if path.contains("/shapes/") || path.contains("/it/") {
                    census.shapes += 1;
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
