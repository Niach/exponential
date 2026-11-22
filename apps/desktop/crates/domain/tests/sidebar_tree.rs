//! #EXP-2027: replays EVERY list of `sidebar-tree.json` through
//! `domain::sidebar_tree`, comparing the WHOLE expected value.

use domain::sidebar_folders::{SidebarEntry, SidebarMove, SidebarRow};
use domain::sidebar_tree::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/sidebar-tree.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn field<T: DeserializeOwned>(case: &Value, key: &str) -> T {
    serde_json::from_value(case[key].clone()).unwrap_or_else(|err| panic!("{} {key}: {err}", case["name"]))
}

fn cases<'a>(f: &'a Value, list: &str) -> &'a Vec<Value> {
    let cases = f[list].as_array().unwrap();
    assert!(!cases.is_empty(), "{list} is empty");
    cases
}

#[test]
fn constants() {
    let f = fixture();
    assert_eq!(f["maxDepth"], json!(SIDEBAR_TREE_MAX_DEPTH));
    assert_eq!(serde_json::to_value(sidebar_tree_copy()).unwrap(), f["copy"]);
    assert_eq!(serde_json::to_value(sidebar_teams_menu()).unwrap(), f["teamsMenu"]);
}

#[test]
fn open_keys() {
    let f = fixture();
    assert_eq!(json!(SIDEBAR_DEVICES_OPEN_KEY), f["openKeys"]["devices"]);
    let team = f["openKeys"]["team"].as_str().unwrap().replace("{teamId}", "t-acme");
    assert_eq!(sidebar_team_open_key("t-acme"), team);
}

#[test]
fn root() {
    let f = fixture();
    for case in cases(&f, "root") {
        let got = sidebar_root_rows(field(case, "needsYou"), field(case, "drafts"));
        assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        let names: Vec<&str> = got.iter().map(|row| row.as_str()).collect();
        assert_eq!(json!(names), case["expected"], "{} as_str", case["name"]);
    }
}

#[test]
fn devices() {
    let f = fixture();
    for case in cases(&f, "devices") {
        let devices: Vec<SidebarDeviceInput> = field(case, "devices");
        let viewer: Vec<String> = field(case, "viewerTeamIds");
        let got = sidebar_device_rows(&devices, &viewer);
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn teams() {
    let f = fixture();
    for case in cases(&f, "teams") {
        let teams: Vec<SidebarTeamInput> = field(case, "teams");
        assert_eq!(json!(sidebar_teams(&teams)), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn team_menu() {
    let f = fixture();
    for case in cases(&f, "teamMenu") {
        let got = sidebar_team_menu(field(case, "owner"));
        assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        for (item, expected) in got.iter().zip(case["expected"].as_array().unwrap()) {
            assert_eq!(json!(item.id.as_str()), expected["id"]);
            let kinds: Vec<&str> = item.children.iter().map(|kind| kind.as_str()).collect();
            assert_eq!(json!(kinds), expected["children"]);
        }
    }
}

#[test]
fn team_items() {
    let f = fixture();
    for case in cases(&f, "teamItems") {
        let boards: Vec<SidebarBoardInput> = field(case, "boards");
        let agents: Vec<SidebarAgentInput> = field(case, "agents");
        let apps: Vec<SidebarAppInput> = field(case, "apps");
        let sessions: Vec<SidebarSessionInput> = field(case, "sessions");
        let entries: Vec<SidebarEntry> = field(case, "entries");
        let team_id: String = field(case, "teamId");
        let got = sidebar_team_items(&team_id, &boards, &agents, &apps, &sessions, &entries);
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn team_rows() {
    let f = fixture();
    for case in cases(&f, "teamRows") {
        let rows: Vec<SidebarRow> = field(case, "rows");
        let team_id: String = field(case, "teamId");
        let got = sidebar_team_rows(&team_id, field(case, "open"), &rows);
        assert!(got.iter().all(|row| row.depth < SIDEBAR_TREE_MAX_DEPTH), "{}", case["name"]);
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn drop() {
    let f = fixture();
    for case in cases(&f, "drop") {
        let items: Vec<String> = field(case, "items");
        let entries: Vec<SidebarEntry> = field(case, "entries");
        let session: SidebarSessionInput = field(case, "session");
        let mv = SidebarMove::from_context(&case["move"]).expect("a move");
        let team_id: String = field(case, "teamId");
        let got = sidebar_drop_session(&team_id, &items, &entries, &session, &mv, case["newFolderId"].as_str().unwrap());
        assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
    }
}
