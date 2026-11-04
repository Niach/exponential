//! VAPP-217 (eApps U2): `fixtures/record-macros.json` replayed through the
//! Rust core (the TS reference replays it in `src/host/record-macros.test.ts`):
//! the host fill of RecordForm / RecordTable, the filled template reducing
//! clean, the install issues and template_messages.

mod support;

use eui_core::host::{self, record_macros::fill_record_macros, template_messages, validate_package};
use eui_core::reducer::{reduce_surface, ReduceOptions};
use eui_core::types::FlatComponent;
use serde_json::{json, Value};
use support::fixture;

#[test]
fn record_macros_fixture() {
    let f = fixture("record-macros.json");
    let records = &f["records"];
    for c in f["fill"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let components = c["components"].as_array().unwrap();
        let bindings = c.get("bindings").and_then(Value::as_array).cloned().unwrap_or_default();
        let read_only = c.get("options").and_then(|o| o.get("readOnly")).and_then(Value::as_bool).unwrap_or(false);
        let (got, binds) = fill_record_macros(components, &bindings, Some(records), read_only);
        assert_eq!(Value::Array(got.clone()), c["expected"]["components"], "fill: {name}");
        assert_eq!(Value::Array(binds), c["expected"]["bindings"], "bindings: {name}");
        let flat: Vec<FlatComponent> = got.iter().map(|v| serde_json::from_value(v.clone()).unwrap()).collect();
        let reduced = reduce_surface(&flat, &ReduceOptions::new(eui_core::catalog::CORE_CATALOG_ID));
        assert!(reduced.issues.is_empty(), "reduces clean: {name}: {:?}", reduced.issues);
    }
    for c in f["install"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let issues = validate_package(&c["package"], &host::supported_catalog_ids::<&str>(&[]));
        assert_eq!(serde_json::to_value(issues).unwrap(), c["issues"], "install: {name}");
    }
    let pkg = &f["install"][0]["package"];
    let messages = template_messages(pkg, "main", "s", None).unwrap();
    let ids: Vec<&str> = messages[1]["updateComponents"]["components"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["root", "m", "m__row"]);
    assert!(messages.iter().any(|m| m["bindDataModel"]["source"] == json!("records:deal?sort=updatedAt&order=desc")));
}
