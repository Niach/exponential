//! Round 4 (VAPP-103): the Rust core replays
//! `packages/exponential-ui/fixtures/round4-contract.json` with the test
//! names of `src/round4.test.ts`: the validator's issues, the `filter`
//! function, the own-write-before-context rule and the Form-closes-its-
//! Dialog rule, the last two through a live `Surface`.

use std::collections::HashMap;

use exponential_ui::expr::core_function;
use exponential_ui::measure::FixedMeasure;
use exponential_ui::reducer::{reduce_nested, ReduceOptions};
use exponential_ui::surface::{OutEvent, Surface, SurfaceOptions};
use exponential_ui::types::{NestedNode, UiNode};
use exponential_ui::validate::is_callable_name;
use serde_json::{json, Value};

const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");

fn fixture() -> Value {
    let path = format!("{PKG}/fixtures/round4-contract.json");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

fn cases<'a>(f: &'a Value, key: &str) -> &'a Vec<Value> {
    f[key].as_array().unwrap_or_else(|| panic!("{key}"))
}

fn nested(v: &Value) -> NestedNode {
    serde_json::from_value(v.clone()).expect("nested input")
}

fn live(c: &Value) -> Surface {
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(nested(&c["input"]));
    assert!(outcome.issues.is_empty(), "{}: {:?}", c["name"], outcome.issues);
    for (k, v) in c["data"].as_object().unwrap() {
        s.set_data(&format!("/{k}"), Some(v.clone())).unwrap();
    }
    s.set_viewport(390.0, 800.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    s
}

fn data_of(s: &Surface, c: &Value) -> Value {
    let keys = c["expected"]["data"].as_object().unwrap().keys();
    Value::Object(keys.map(|k| (k.clone(), s.get_data(&format!("/{k}")).cloned().unwrap_or(Value::Null))).collect())
}

fn actions(events: &[OutEvent]) -> Vec<(String, Value)> {
    events
        .iter()
        .filter_map(|e| match e {
            OutEvent::Action { name, context, .. } => Some((name.clone(), context.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn validate_the_reducer_reports_bad_style_keys_and_tokens_literal_colours_unknown_functions_and_dangling_slot_columns() {
    let f = fixture();
    let options = ReduceOptions::new(f["catalogId"].as_str().unwrap());
    for c in cases(&f, "validate") {
        let issues = serde_json::to_value(reduce_nested(&nested(&c["input"]), &options).issues).unwrap();
        assert_eq!(issues, c["issues"], "{}", c["name"]);
    }
}

/// The Unknown placeholders of a reduced result, per tree (`root`, then each
/// lifted template), pre-order: children before slots.
fn placeholders(result: &exponential_ui::reducer::ReduceResult) -> Value {
    fn walk(tree: &str, n: &UiNode, out: &mut Vec<Value>) {
        if n.component == "Unknown" {
            out.push(json!({"tree": tree, "id": n.id, "component": n.props.get("component").cloned().unwrap_or(Value::Null)}));
        }
        for c in &n.children {
            walk(tree, c, out);
        }
        for c in n.slots.iter().flat_map(|s| s.values()) {
            walk(tree, c, out);
        }
    }
    let mut out = Vec::new();
    walk("root", &result.root, &mut out);
    for (id, n) in result.templates.iter().flatten() {
        walk(id, n, &mut out);
    }
    Value::Array(out)
}

#[test]
fn depth_max_depth_holds_after_expansion_each_lifted_template_from_its_own_level_1() {
    let f = fixture();
    let options = ReduceOptions::new(f["catalogId"].as_str().unwrap());
    for c in cases(&f, "depth") {
        let result = reduce_nested(&nested(&c["input"]), &options);
        assert_eq!(serde_json::to_value(&result.issues).unwrap(), c["issues"], "{}", c["name"]);
        assert_eq!(placeholders(&result), c["placeholders"], "{}", c["name"]);
    }
}

/// `{"$fill": n}` = n zeros.
fn expand_fill(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(expand_fill).collect()),
        Value::Object(o) => match o.get("$fill").and_then(Value::as_u64) {
            Some(n) => Value::Array(vec![json!(0); n as usize]),
            None => Value::Object(o.iter().map(|(k, x)| (k.clone(), expand_fill(x))).collect()),
        },
        _ => v.clone(),
    }
}

#[test]
fn template_budget_items_charge_max_template_items_and_their_subtree_nodes_depth_first() {
    let f = fixture();
    for c in cases(&f, "templateBudget") {
        let mut s = Surface::new("t", SurfaceOptions::default());
        s.set_viewport(400.0, 800.0, None);
        let outcome = s.set_nested(nested(&c["input"]));
        assert!(outcome.issues.is_empty(), "{}: {:?}", c["name"], outcome.issues);
        for (k, v) in expand_fill(&c["data"]).as_object().unwrap() {
            s.set_data(&format!("/{k}"), Some(v.clone())).unwrap();
        }
        s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
        for site in c["sites"].as_array().unwrap() {
            let (template, instance) = (site["template"].as_str().unwrap(), site["instance"].as_str().unwrap());
            let built = (0..site["items"].as_u64().unwrap()).filter(|i| s.index_of(&format!("{template}{instance}.{i}")).and_then(|i| s.layout_node(i)).is_some()).count();
            assert_eq!(built as u64, site["built"].as_u64().unwrap(), "{}: {template}{instance}", c["name"]);
        }
        assert_eq!(serde_json::to_value(s.issues()).unwrap(), c["issues"], "{}", c["name"]);
    }
}

#[test]
fn filter_the_core_function_keeps_the_matching_items_in_order() {
    let f = fixture();
    for c in cases(&f, "filter") {
        let got = core_function("filter", c["args"].as_object().unwrap()).expect("filter is a core function");
        assert_eq!(got, Some(c["expected"].clone()), "{}", c["name"]);
    }
}

#[test]
fn own_write_the_context_resolves_after_the_components_own_write() {
    let f = fixture();
    for c in cases(&f, "ownWrite") {
        let mut s = live(c);
        let i = s.index_of(c["target"].as_str().unwrap()).unwrap_or_else(|| panic!("{}: no target", c["name"]));
        let events = s.event(i, c["event"].as_str().unwrap(), Some(c["payload"].clone()));
        assert_eq!(data_of(&s, c), c["expected"]["data"], "{}", c["name"]);
        let sent = actions(&events);
        let want = &c["expected"]["action"];
        let (name, context) = sent.iter().find(|(n, _)| n == want["name"].as_str().unwrap()).unwrap_or_else(|| panic!("{}: {sent:?}", c["name"]));
        assert_eq!((name.as_str(), context), (want["name"].as_str().unwrap(), &want["context"]), "{}", c["name"]);
    }
}

#[test]
fn close_on_submit_the_nearest_dialog_or_drawer_around_the_form_closes() {
    let f = fixture();
    for c in cases(&f, "closeOnSubmit") {
        let mut s = live(c);
        let form = c["form"].as_str().unwrap();
        let i = s.index_of(form).unwrap_or_else(|| panic!("{}: no form", c["name"]));
        let events = s.event(i, "submit", None);
        let names: Vec<String> = actions(&events).into_iter().map(|(n, _)| n).collect();
        assert_eq!(serde_json::to_value(&names).unwrap(), c["actions"], "{}", c["name"]);
        assert_eq!(data_of(&s, c), c["expected"]["data"], "{}", c["name"]);
        if let Some(overlay) = c["expected"]["overlay"].as_str() {
            s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
            assert!(s.index_of(form).is_none(), "{}: {overlay} is closed", c["name"]);
        }
    }
}

#[test]
fn host_functions_are_namespaced() {
    for ok in ["openUrl", "filter", "set", "formatRelativeTime", "app.toast", "harness.openIssue", "cart.add", "a.b.c"] {
        assert!(is_callable_name(ok), "{ok}");
    }
    for bad in ["opneUrl", "toast", ".toast", "app.", "app..toast", ""] {
        assert!(!is_callable_name(bad), "{bad}");
    }
}
