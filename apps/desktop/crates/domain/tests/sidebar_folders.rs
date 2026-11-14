//! #EXP-1832: replays EVERY list of `sidebar-folders.json` through
//! `domain::sidebar_folders`, comparing the WHOLE expected value.

use domain::rows::SidebarLayoutRow;
use domain::sidebar_folders::*;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/sidebar-folders.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn items(case: &Value) -> Vec<String> {
    serde_json::from_value(case["items"].clone()).unwrap()
}

fn entries(case: &Value) -> Vec<SidebarEntry> {
    serde_json::from_value(case["entries"].clone()).unwrap()
}

fn cases<'a>(f: &'a Value, list: &str) -> &'a Vec<Value> {
    let cases = f[list].as_array().unwrap();
    assert!(!cases.is_empty(), "{list} is empty");
    cases
}

#[test]
fn constants() {
    let f = fixture();
    assert_eq!(f["maxDepth"], json!(SIDEBAR_MAX_DEPTH));
    assert_eq!(f["defaultFolderName"], json!(SIDEBAR_FOLDER_DEFAULT_NAME));
}

#[test]
fn tree() {
    let f = fixture();
    for case in cases(&f, "tree") {
        let got = serde_json::to_value(sidebar_tree(&items(case), &entries(case))).unwrap();
        assert_eq!(got, case["expected"], "{}", case["name"]);
    }
}

#[test]
fn moves() {
    let f = fixture();
    for case in cases(&f, "move") {
        let mv = SidebarMove::from_context(&case["move"]).expect("a move");
        let got = sidebar_move(&items(case), &entries(case), &mv, case["newFolderId"].as_str().unwrap());
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn ungroup() {
    let f = fixture();
    for case in cases(&f, "ungroup") {
        let got = sidebar_ungroup(&items(case), &entries(case), case["folderId"].as_str().unwrap());
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn rows() {
    let f = fixture();
    for case in cases(&f, "rows") {
        let closed: Vec<String> = serde_json::from_value(case["closed"].clone()).unwrap();
        let tree = sidebar_tree(&items(case), &entries(case));
        let got = sidebar_rows(&tree, |id| !closed.iter().any(|c| c == id));
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn folder_names() {
    let f = fixture();
    for case in cases(&f, "folderNames") {
        assert_eq!(json!(sidebar_folder_name(case["input"].as_str().unwrap())), case["expected"], "{}", case["input"]);
    }
}

#[test]
fn open_keys() {
    let f = fixture();
    for case in cases(&f, "openKeys") {
        let got = sidebar_folder_open_key(case["teamId"].as_str().unwrap(), case["folderId"].as_str().unwrap());
        assert_eq!(json!(got), case["expected"]);
    }
}

/// The writes go out as the server's `sidebarLayout.save` input reads them.
#[test]
fn writes_serialize_camel_case_with_whole_sort_orders() {
    let write = SidebarEntryWrite {
        id: None,
        kind: SidebarEntryKind::Item,
        target_id: Some("b-1".into()),
        parent_id: None,
        name: None,
        sort_order: 2.0,
    };
    assert_eq!(
        serde_json::to_value(&write).unwrap(),
        json!({"id": null, "kind": "item", "targetId": "b-1", "parentId": null, "name": null, "sortOrder": 2})
    );
    let half = SidebarEntryWrite { sort_order: 1.5, ..write };
    assert_eq!(serde_json::to_value(&half).unwrap()["sortOrder"], json!(1.5));
    assert_eq!(SidebarSection::Agent.as_str(), "agent");
    assert_eq!(serde_json::to_value(SidebarSection::App).unwrap(), json!("app"));
}

/// A List `move` context reads `{id, before | after | into}`; no id = none.
#[test]
fn move_from_context() {
    let mv = SidebarMove::from_context(&json!({"id": "a", "into": "b"})).unwrap();
    assert_eq!(mv, SidebarMove { id: "a".into(), into: Some("b".into()), ..Default::default() });
    assert!(SidebarMove::from_context(&json!({"before": "b"})).is_none());
}

/// The synced rows of one team's section become entries (an unknown kind is
/// skipped, a missing sort order reads 0).
#[test]
fn entries_from_synced_rows() {
    let rows: Vec<SidebarLayoutRow> = serde_json::from_value(json!([
        {"id": "e-1", "team_id": "t-1", "section": "board", "kind": "item", "target_id": "b-1", "sort_order": "2"},
        {"id": "f-1", "team_id": "t-1", "section": "board", "kind": "folder", "name": "Ops"},
        {"id": "e-2", "team_id": "t-1", "section": "agent", "kind": "item", "target_id": "a-1", "sort_order": 1},
        {"id": "e-3", "team_id": "t-2", "section": "board", "kind": "item", "target_id": "b-2", "sort_order": 1},
        {"id": "e-4", "team_id": "t-1", "section": "board", "kind": "nope", "target_id": "b-3", "sort_order": 1}
    ]))
    .unwrap();
    let got = sidebar_entries(&rows, "t-1", SidebarSection::Board);
    assert_eq!(
        serde_json::to_value(got).unwrap(),
        json!([
            {"id": "e-1", "kind": "item", "targetId": "b-1", "parentId": null, "name": null, "sortOrder": 2},
            {"id": "f-1", "kind": "folder", "targetId": null, "parentId": null, "name": "Ops", "sortOrder": 0}
        ])
    );
}
