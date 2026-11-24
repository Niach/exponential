//! #EXP-2124: replays EVERY case of `sidebar-menus.json` through
//! `domain::sidebar_menus`, comparing the WHOLE expected value.

use domain::sidebar_menus::*;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/sidebar-menus.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn cases<'a>(f: &'a Value, list: &str) -> &'a Vec<Value> {
    let cases = f[list].as_array().unwrap();
    assert!(!cases.is_empty(), "{list} is empty");
    cases
}

#[test]
fn menu_cases() {
    let f = fixture();
    for case in cases(&f, "menuCases") {
        let input: SidebarMenuInput = serde_json::from_value(case["input"].clone()).unwrap();
        let got = sidebar_item_menu(case["kind"].as_str().unwrap(), input);
        assert_eq!(json!(got), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn every_kind_has_a_case() {
    let f = fixture();
    let covered: Vec<&str> = cases(&f, "menuCases").iter().filter_map(|case| case["kind"].as_str()).collect();
    for kind in sidebar_menu_kinds() {
        assert!(covered.contains(&kind), "{kind} has no case");
    }
}

#[test]
fn new_cases() {
    let f = fixture();
    for case in cases(&f, "newCases") {
        let got = sidebar_new_kinds(case["owner"].as_bool().unwrap());
        assert_eq!(json!(got), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn new_tab_cases() {
    let f = fixture();
    for case in cases(&f, "newTabCases") {
        let teams: Vec<NewTabTeam> = serde_json::from_value(case["teams"].clone()).unwrap();
        let got = new_tab_menu(case["assistant"].as_bool().unwrap(), &teams);
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn copy_icons_and_shortcuts() {
    let f = fixture();
    for (id, label) in f["copy"].as_object().unwrap() {
        assert_eq!(json!(sidebar_menu_copy(id)), *label, "{id}");
    }
    for (id, icon) in f["icons"].as_object().unwrap() {
        assert_eq!(json!(sidebar_menu_icon(id)), *icon, "{id}");
    }
    for (kind, label) in f["newCopy"].as_object().unwrap() {
        assert_eq!(json!(sidebar_new_copy(kind)), *label, "{kind}");
        assert_eq!(json!(sidebar_new_icon(kind)), f["newIcons"][kind], "{kind}");
    }
    let shortcut = sidebar_menu_shortcut("new-chat").unwrap();
    assert!(shortcut == f["shortcuts"]["new-chat"]["apple"] || shortcut == f["shortcuts"]["new-chat"]["other"]);
    assert_eq!(sidebar_menu_shortcut("open"), None);
    assert_eq!(json!(sidebar_tab_pages()), f["tabs"]["pages"]);
}

#[test]
fn only_danger_rows_are_destructive() {
    assert!(sidebar_menu_destructive("board", "archive"));
    assert!(sidebar_menu_destructive("active-session", "delete"));
    assert!(!sidebar_menu_destructive("board", "settings"));
    let items = sidebar_menu_items("board", SidebarMenuInput { owner: true, ..Default::default() }, &[], |id| (id == "move").then(|| json!({"@path": "moveItems"})));
    assert_eq!(items.last().unwrap(), &json!({"label": "Archive", "value": "archive", "icon": "ui-archive", "destructive": true}));
    let mv = items.iter().find(|item| item["label"] == "Move to folder").unwrap();
    assert_eq!(mv["kind"], "submenu");
    assert_eq!(mv["items"], json!({"@path": "moveItems"}));
}

/// A verb the row has no action for is not drawn, nor a separator left
/// doubled or at an end.
#[test]
fn a_verb_without_an_action_is_not_drawn() {
    let placed = sidebar_item_menu_without("active-session", SidebarMenuInput { can_delete: true, ..Default::default() }, &["add-to-sidebar"]);
    assert_eq!(placed, ["open", "open-new-tab", "separator", "copy-link", "separator", "delete"]);
    let bare = sidebar_item_menu_without("active-session", SidebarMenuInput::default(), &["add-to-sidebar", "copy-link"]);
    assert_eq!(bare, ["open", "open-new-tab"]);
}

/// THE New list is ONE piece: the IDE's team menu and new-tab menu draw
/// uicore's `new_items`, which must be the table's `newKinds` + copy + icons.
#[test]
fn the_ide_new_list_is_the_table() {
    for owner in [true, false] {
        let got: Vec<Value> = uicore_model::ide::new_items(owner).into_iter().map(|e| json!({"label": e.label, "value": e.value, "icon": e.icon})).collect();
        let want: Vec<Value> = sidebar_new_kinds(owner).into_iter().map(|k| json!({"label": sidebar_new_copy(k), "value": k, "icon": sidebar_new_icon(k)})).collect();
        assert_eq!(got, want, "owner {owner}");
    }
}
