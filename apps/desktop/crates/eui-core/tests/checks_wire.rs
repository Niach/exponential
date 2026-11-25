//! #AXC-19, round 9 §5.1: the envelope mixins ride the wire under `@`
//! (`@checks`, `@accessibility`, `@metadata`, `@catalogId`); a bare one is an
//! ordinary prop or refused; inside, the check prop is `checks`. Twin of packages/exponential-ui/src/checks-wire.test.ts.

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

fn refusal() -> eui_core::types::ReduceIssue {
    eui_core::types::ReduceIssue::at("root", "checks is not a declared prop (write @checks)", "/checks", "INVALID_MESSAGE")
}

fn entry_refusal(entry: &Value) -> Option<(String, String)> {
    eui_core::host::check_entry(entry, Some(CORE_CATALOG_ID), &eui_core::catalog::CatalogView::core())
}

#[test]
fn refuses_the_bare_checks() {
    let components = input(json!({"checks": required("/t", "Old")}));
    let r = reduce(components.clone(), CORE_CATALOG_ID);
    assert_eq!(r.issues, vec![refusal()]);
    assert!(r.root.props.get("checks").is_none());
    assert_eq!(entry_refusal(&components[0]), Some(("/checks".into(), refusal().message)));
}

#[test]
fn refuses_the_bare_checks_beside_at_checks_too() {
    let components = input(json!({"checks": required("/t", "Old"), "@checks": required("/t", "New")}));
    let r = reduce(components.clone(), CORE_CATALOG_ID);
    assert_eq!(r.issues, vec![refusal()]);
    assert_eq!(r.root.props.get("checks"), Some(&required("/t", "New")));
    assert_eq!(entry_refusal(&components[0]), Some(("/checks".into(), refusal().message)));
}

#[test]
fn a_nested_node_carries_at_checks_next_to_props() {
    let nested = |v: Value| {
        let node: eui_core::NestedNode = serde_json::from_value(v).unwrap();
        eui_core::reducer::reduce_nested(&node, &ReduceOptions { expand: false, ..ReduceOptions::new(CORE_CATALOG_ID) })
    };
    let props = json!({"label": "Title", "name": "title"});
    let good = nested(json!({"id": "root", "component": "Input", "props": props, "@checks": required("/t", "Needed")}));
    assert!(good.issues.is_empty(), "{:?}", good.issues);
    assert_eq!(good.root.props.get("checks"), Some(&required("/t", "Needed")));
    let bad = nested(json!({"id": "root", "component": "Input", "props": {"label": "Title", "name": "title", "checks": required("/t", "Old")}}));
    assert_eq!(bad.issues, vec![refusal()]);
    assert!(bad.root.props.get("checks").is_none());
}

#[test]
fn a_basic_catalog_text_field_maps_at_checks_and_refuses_the_bare_one() {
    let flat = |key: &str| {
        let mut c = json!({"id": "root", "component": "TextField", "label": "Email", "value": {"@path": "/e"}});
        c[key] = required("/e", "Needed");
        json!([c])
    };
    assert_eq!(reduce(flat("@checks"), A2UI_BASIC_CATALOG_ID).root.props.get("checks"), Some(&required("/e", "Needed")));
    let bare = reduce(flat("checks"), A2UI_BASIC_CATALOG_ID);
    assert_eq!(bare.issues, vec![refusal()]);
    assert!(bare.root.props.get("checks").is_none());
}
