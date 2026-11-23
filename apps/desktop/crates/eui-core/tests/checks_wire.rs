//! #AXC-19: A2UI 1.0's check mixin rides the wire as `@checks` (what the
//! Express compiler emits); the legacy bare `checks` still reads; inside, the
//! prop is `checks`. Twin of packages/exponential-ui/src/checks-wire.test.ts.

use eui_core::reducer::{reduce_surface, ReduceOptions};
use eui_core::types::FlatComponent;
use eui_core::{A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID};
use serde_json::{json, Value};

fn required(path: &str, message: &str) -> Value {
    json!([{"condition": {"@call": "required", "args": {"value": {"@path": path}}}, "message": message}])
}

fn reduce(components: Value, catalog: &str) -> eui_core::reducer::ReduceResult {
    let flat: Vec<FlatComponent> = serde_json::from_value(components).unwrap();
    reduce_surface(&flat, &ReduceOptions { expand: false, ..ReduceOptions::new(catalog) })
}

fn input(extra: Value) -> Value {
    let mut c = json!({"id": "root", "component": "Input", "label": "Title", "name": "title", "value": {"@path": "/t"}});
    c.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    json!([c])
}

#[test]
fn reads_at_checks_as_the_checks_prop() {
    let r = reduce(input(json!({"@checks": required("/t", "Needed")})), CORE_CATALOG_ID);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert_eq!(r.root.props.get("checks"), Some(&required("/t", "Needed")));
    assert!(r.root.props.get("@checks").is_none());
}

#[test]
fn still_reads_the_legacy_bare_checks() {
    let r = reduce(input(json!({"checks": required("/t", "Old")})), CORE_CATALOG_ID);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert_eq!(r.root.props.get("checks"), Some(&required("/t", "Old")));
}

#[test]
fn at_checks_wins_over_checks() {
    let r = reduce(input(json!({"checks": required("/t", "Old"), "@checks": required("/t", "New")})), CORE_CATALOG_ID);
    assert_eq!(r.root.props.get("checks"), Some(&required("/t", "New")));
}

#[test]
fn a_basic_catalog_text_field_maps_at_checks() {
    let flat = json!([{"id": "root", "component": "TextField", "label": "Email", "value": {"@path": "/e"}, "@checks": required("/e", "Needed")}]);
    let r = reduce(flat, A2UI_BASIC_CATALOG_ID);
    assert_eq!(r.root.props.get("checks"), Some(&required("/e", "Needed")));
}
