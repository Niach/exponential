//! VAPP-289 (round 9 §9.3): replays `fixtures/regex.json` (the result and the
//! exact step count), like `src/regex.test.ts`.

use eui_core::regex::regex_step_count;
use serde_json::{json, Value};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/regex.json");

fn repeated(v: &Value, n: Option<u64>) -> Value {
    match (v.as_str(), n) {
        (Some(s), Some(n)) => Value::String(s.repeat(n as usize)),
        _ => v.clone(),
    }
}

#[test]
fn replays_every_case() {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    let cases = doc["cases"].as_array().unwrap();
    let mut failed = Vec::new();
    for c in cases {
        let value = repeated(c.get("value").unwrap_or(&Value::Null), c["repeat"]["value"].as_u64());
        let pattern = repeated(&c["pattern"], c["repeat"]["pattern"].as_u64());
        let got = regex_step_count(&value, &pattern);
        let shown = match &got {
            Ok((valid, _)) => json!({"valid": valid}),
            Err(error) => json!({"error": error}),
        };
        if shown != c["expected"] {
            failed.push(format!("{}: got {shown} want {}", c["name"], c["expected"]));
        } else if let (Ok((_, steps)), Some(want)) = (&got, c["steps"].as_u64()) {
            if *steps != want {
                failed.push(format!("{}: {steps} steps, want {want}", c["name"]));
            }
        }
    }
    assert!(failed.is_empty(), "{} of {} cases failed:\n{}", failed.len(), cases.len(), failed.join("\n"));
}
