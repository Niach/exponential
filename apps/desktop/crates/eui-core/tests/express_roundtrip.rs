//! #AXC-12: `fixtures/express-roundtrip.json`: each sample surface's
//! createSurface → Express (`decompile`) → messages (`compile`) with zero
//! diffs, and the reduced surface keeps the nested fixture's node count
//! (dashboard 56, demo-surface 12, kitchen-sink 251).
#![cfg(feature = "express")]

use eui_core::express::{compile, decompile};
use eui_core::reducer::{reduce_surface, ReduceOptions};
use eui_core::types::{FlatComponent, UiNode};
use serde_json::Value;

fn fixture() -> Value {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/express-roundtrip.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

fn case(name: &str) -> (Vec<Value>, usize) {
    let doc = fixture();
    let case = doc["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap_or_else(|| panic!("no case {name}"));
    (case["messages"].as_array().unwrap().clone(), case["nodes"].as_u64().unwrap() as usize)
}

fn count(n: &UiNode) -> usize {
    1 + n.children.iter().map(count).sum::<usize>() + n.slots.as_ref().map_or(0, |s| s.values().map(count).sum())
}

fn nodes(messages: &[Value]) -> usize {
    let create = &messages[0]["createSurface"];
    let components: Vec<FlatComponent> = serde_json::from_value(create["components"].clone()).unwrap();
    let catalog = create["catalogId"].as_str().unwrap();
    let options = ReduceOptions { expand: false, ..ReduceOptions::new(catalog) };
    let result = reduce_surface(&components, &options);
    assert!(result.issues.is_empty(), "reduce issues: {:?}", result.issues);
    count(&result.root)
}

/// The first path where two values differ (`None` = equal; key order aside).
fn first_diff(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    None => return Some(format!("{at}/{k}: missing after the round trip")),
                    Some(w) => {
                        if let Some(d) = first_diff(v, w, &format!("{at}/{k}")) {
                            return Some(d);
                        }
                    }
                }
            }
            y.keys().find(|k| !x.contains_key(*k)).map(|k| format!("{at}/{k}: added by the round trip"))
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{at}: {} items, {} after the round trip", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (v, w))| first_diff(v, w, &format!("{at}/{i}")))
        }
        (Value::Number(x), Value::Number(y)) if x.as_f64() == y.as_f64() => None,
        _ if a == b => None,
        _ => Some(format!("{at}: {a} became {b}")),
    }
}

fn round_trip(name: &str) {
    let (messages, expected_nodes) = case(name);
    assert_eq!(nodes(&messages), expected_nodes, "{name}: the fixture's node count");
    let express = decompile(&messages).unwrap_or_else(|e| panic!("{name}: decompile: {e}"));
    let back = compile(&express, "main").unwrap_or_else(|e| panic!("{name}: compile: {e}\n{express}"));
    if let Some(diff) = first_diff(&Value::Array(messages.clone()), &Value::Array(back.clone()), "") {
        // The Express statement of the component that differs, if any.
        let index = diff.split('/').nth(4).and_then(|i| i.parse::<usize>().ok());
        let id = index.and_then(|i| messages[0]["createSurface"]["components"][i]["id"].as_str().map(str::to_string)).unwrap_or_default();
        let line = express.lines().find(|l| !id.is_empty() && l.starts_with(&format!("{id} ="))).unwrap_or("");
        panic!("{name}: the round trip differs at {diff}\n  express: {line}");
    }
    assert_eq!(nodes(&back), expected_nodes, "{name}: node count after the round trip");
}

#[test]
fn dashboard_round_trips_through_express() {
    round_trip("dashboard");
}

#[test]
fn demo_surface_round_trips_through_express() {
    round_trip("demo-surface");
}

#[test]
fn kitchen_sink_round_trips_through_express() {
    round_trip("kitchen-sink");
}

/// The block compiles the same with and without its `<a2ui>` tags.
#[test]
fn sentinel_tags_are_optional() {
    let (messages, _) = case("demo-surface");
    let express = decompile(&messages).unwrap();
    assert_eq!(compile(&format!("<a2ui>\n{express}\n</a2ui>"), "main").unwrap(), compile(&express, "main").unwrap());
}

/// Express compiles to the existing catalogue only.
#[test]
fn an_unknown_component_is_an_error() {
    let err = compile("root = Sparkle(text=\"hi\")", "main").unwrap_err();
    assert!(err.message.contains("Sparkle"), "{err}");
}
