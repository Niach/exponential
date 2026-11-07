//! Round 9 §14.1: a Local desktop's store keeps loading its format 5 apps.

use elocal::a2ui10::{upgrade_once, MARKER};
use elocal::LocalStore;
use eui_core::host::validate_package;
use serde_json::{json, Map, Value};

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

fn v5() -> Value {
    json!({
        "formatVersion": 5, "id": "local.notes", "name": "Notes", "version": "1.0.0",
        "catalogId": "https://ui.exponential.at/catalogs/core/v1",
        "functions": ["records.*"], "sources": ["records:*"],
        "records": {"note": {"schema": {"type": "object", "properties": {"text": {"type": "string"}}}}},
        "templates": {"main": {"components": [
            {"id": "root", "component": "Button", "label": {"path": "/label"}, "on": {"press": {"functionCall": {"call": "records.upsert", "args": {"kind": "note", "data": {}}}}}}
        ]}}
    })
}

#[test]
fn a_store_upgrades_once_and_sealed_cards_stay() {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(&dir.path().join("local.sqlite")).unwrap();
    store.write("apps", obj(json!({"id": "a1", "package": v5(), "draft_package": v5(), "granted_tools": ["exponential.issues.update", "exponential_issues_update"]}))).unwrap();
    let old = json!({"root": {"id": "my-text", "component": "Text", "props": {"text": {"path": "/t"}}}, "data": {"t": "x"}});
    store.write("messages", obj(json!({"id": "m1", "card": {"kind": "surface", "surface": old}}))).unwrap();
    store.write("messages", obj(json!({"id": "m2", "card": {"kind": "surface", "sealed": "v1:1:abc"}}))).unwrap();
    store.write("messages", obj(json!({"id": "m3", "card": null, "body": "hi"}))).unwrap();

    upgrade_once(&store, dir.path()).unwrap();

    let app = store.row("apps", "a1").unwrap().unwrap();
    assert_eq!(app["package"]["formatVersion"], json!(6));
    assert_eq!(app["draft_package"]["formatVersion"], json!(6));
    assert_eq!(app["package"]["functions"], json!(["records_*"]));
    assert_eq!(validate_package(&app["package"], &eui_core::host::supported_catalog_ids::<&str>(&[])), vec![]);
    assert_eq!(app["granted_tools"], json!(["exponential_issues_update"]));
    let card = store.row("messages", "m1").unwrap().unwrap();
    assert_eq!(card["card"]["surface"]["version"], json!("1.0"));
    assert_eq!(card["card"]["surface"]["root"]["id"], json!("my_text"));
    assert_eq!(card["card"]["surface"]["root"]["props"]["text"], json!({"@path": "/t"}));
    assert_eq!(store.row("messages", "m2").unwrap().unwrap()["card"]["sealed"], json!("v1:1:abc"));
    assert!(dir.path().join(MARKER).exists());

    // The marker skips it: a later format 5 row stays what it is.
    store.write("apps", obj(json!({"id": "a2", "package": v5()}))).unwrap();
    upgrade_once(&store, dir.path()).unwrap();
    assert_eq!(store.row("apps", "a2").unwrap().unwrap()["package"]["formatVersion"], json!(5));
}
