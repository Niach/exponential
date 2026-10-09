//! `fixtures/interactions-round1.json` (hand-authored, round 1 review): the
//! interaction semantics the reference renderer has and the core must match
//! — NumberField stepping, rounding and text; Form check semantics; the
//! unbound Table sort. The React half is
//! `packages/exponential-ui-react/src/interactions-fixture.test.tsx`.

use std::collections::HashMap;

use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{OutEvent, Surface, SurfaceOptions};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/interactions-round1.json")).expect("fixture")).expect("json")
}

fn surface(tree: Value) -> Surface {
    let mut s = surface_with_issues(tree);
    assert!(s.issues().is_empty(), "{:?}", s.issues());
    s
}

/// A check without a message is reported by validation (`message:
/// required`) and still renders; the fixture pins how it reads.
fn surface_with_issues(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_viewport(400.0, 0.0, None);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn text_of(s: &mut Surface, id: &str) -> Value {
    s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no {id}")).props.get("text").cloned().unwrap_or(Value::Null)
}

#[test]
fn number_field_cases_step_round_and_show_like_the_reference() {
    let f = fixture();
    for c in f["numberField"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut props = c["props"].as_object().cloned().unwrap();
        props.insert("label".into(), json!("N"));
        props.insert("name".into(), json!("n"));
        props.insert("value".into(), json!({"path": "/v"}));
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "n", "component": "NumberField", "props": props, "on": {"change": {"event": {"name": "change"}}}}]}));
        if !c["value"].is_null() {
            s.set_data("/v", Some(c["value"].clone())).unwrap();
        }
        let mut m = fixed();
        s.layout(&mut m);
        let events = match c["op"].as_str().unwrap() {
            "increment" | "decrement" => {
                let i = s.index_of(&format!("n.{}", c["op"].as_str().unwrap())).unwrap();
                s.event(i, "press", None)
            }
            "commit" => {
                let i = s.index_of("n.input").unwrap();
                s.event(i, "commit", Some(json!({"value": c["input"]})))
            }
            _ => vec![],
        };
        let mut changed = events.iter().filter_map(|e| match e {
            OutEvent::Action { name, context, .. } if name == "change" => Some(context["value"].clone()),
            _ => None,
        });
        let value = changed.next_back().unwrap_or(c["value"].clone());
        assert_eq!(exponential_ui::json::canonical(&value), exponential_ui::json::canonical(&c["expect"]["value"]), "{name}: value");
        if c["op"] != "none" && c["expect"]["value"] != c["value"] {
            assert_eq!(exponential_ui::json::canonical(s.get_data("/v").unwrap()), exponential_ui::json::canonical(&c["expect"]["value"]), "{name}: /v");
        }
        s.layout(&mut m);
        assert_eq!(text_of(&mut s, "n.input"), c["expect"]["text"], "{name}: text");
    }
}

#[test]
fn check_cases_fail_and_pass_like_the_reference() {
    let f = fixture();
    for c in f["checks"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface_with_issues(json!({"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"submit": {"event": {"name": "submit"}}, "invalid": {"event": {"name": "invalid"}}},
            "children": [
                {"id": "agree", "component": "Checkbox", "props": {"label": "I agree", "name": "agree", "checked": {"path": "/agreed"}, "checks": c["checks"]}},
                {"id": "go", "component": "Button", "props": {"label": "Send", "submit": true}}
            ]}));
        s.set_data("", Some(c["data"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let failing: Vec<String> = s.failing_checks("agree");
        assert_eq!(json!(failing), c["failing"], "{name}: failing_checks");
        let go = s.index_of("go").unwrap();
        let events = s.event(go, "press", None);
        let invalid: Vec<Value> = events
            .iter()
            .filter_map(|e| match e {
                OutEvent::Action { name, context, .. } if name == "invalid" => Some(context["errors"].as_array().unwrap().iter().map(|e| e["message"].clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .next()
            .unwrap_or_default();
        assert_eq!(json!(invalid), c["failing"], "{name}: invalid errors");
        let submitted = events.iter().any(|e| matches!(e, OutEvent::Action { name, .. } if name == "submit"));
        assert_eq!(json!(submitted), c["submitted"], "{name}: submitted");
    }
}

#[test]
fn table_sort_cases_order_rows_like_the_reference() {
    let f = fixture();
    for c in f["tableSort"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut column = c["column"].clone();
        column["label"] = json!("V");
        column["sortable"] = json!(true);
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Table", "props": {"columns": [column], "rows": c["rows"], "sort": {"key": c["column"]["key"], "direction": c["direction"]}}}]}));
        let mut m = fixed();
        s.layout(&mut m);
        let nodes = s.nodes();
        let body = nodes.iter().find(|n| n.id == "t.body").unwrap();
        let order: Vec<Value> = body
            .children
            .iter()
            .map(|i| &nodes[*i as usize])
            .filter(|n| n.part.as_deref() == Some("row"))
            .map(|n| n.props["index"].clone())
            .collect();
        assert_eq!(json!(order), c["order"], "{name}");
    }
}
