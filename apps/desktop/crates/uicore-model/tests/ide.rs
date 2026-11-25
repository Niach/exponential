//! Replays `packages/uicore/fixtures/ide-sidebar.json` against the
//! TypeScript reference.

use serde_json::Value;
use uicore_model::ide::{ide_sidebar, ide_tab_placement, ide_tabs, IdeSidebarInput, IdeTabInput};

#[test]
fn every_ide_sidebar_case_replays() {
    let f: Value = serde_json::from_str(include_str!("../../../../../packages/uicore/fixtures/ide-sidebar.json")).expect("ide-sidebar.json");
    let cases = f["cases"].as_array().expect("cases");
    assert!(cases.len() >= 12, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let mut input = f["base"].clone();
        for (k, v) in case["input"].as_object().expect("input") {
            input[k] = v.clone();
        }
        let input: IdeSidebarInput = serde_json::from_value(input).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(serde_json::to_value(ide_sidebar(&input)).unwrap(), case["expected"], "{name}");
    }
}

#[test]
fn every_ide_tabs_case_replays() {
    let f: Value = serde_json::from_str(include_str!("../../../../../packages/uicore/fixtures/ide-sidebar.json")).expect("ide-sidebar.json");
    for case in f["tabs"].as_array().expect("tabs") {
        let name = case["name"].as_str().unwrap_or("?");
        let tabs: Vec<IdeTabInput> = serde_json::from_value(case["tabs"].clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(serde_json::to_value(ide_tabs(&tabs, case["active"].as_str())).unwrap(), case["expected"], "{name}");
    }
}

#[test]
fn every_ide_tab_placement_case_replays() {
    let f: Value = serde_json::from_str(include_str!("../../../../../packages/uicore/fixtures/ide-sidebar.json")).expect("ide-sidebar.json");
    for case in f["placement"].as_array().expect("placement") {
        let name = case["name"].as_str().unwrap_or("?");
        let ede = case["ede"].as_bool().expect("ede");
        assert_eq!(serde_json::to_value(ide_tab_placement(ede)).unwrap(), case["expected"], "{name}");
    }
}

/// nav-badges.json's rules, as this crate (below `domain`) states them.
#[test]
fn the_badge_rules_are_nav_badges() {
    use serde_json::json;
    use uicore_model::ide::{count_label, row_badge, sum_badges, team_badge, IdeCount};
    let f: Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/nav-badges.json")).expect("nav-badges.json");
    let cases = |list: &str| f[list].as_array().unwrap_or_else(|| panic!("{list}")).clone();
    let n = |v: &Value| v.as_i64().unwrap().max(0) as usize;
    let badges = |v: &Value| -> Vec<IdeCount> { v.as_array().unwrap().iter().map(|b| IdeCount { count: n(&b["count"]), tone: serde_json::from_value(b["tone"].clone()).unwrap() }).collect() };
    for c in cases("labelCases") {
        assert_eq!(count_label(n(&c["count"])).unwrap_or_default(), c["expected"].as_str().unwrap(), "{}", c["name"]);
    }
    for c in cases("rowCases") {
        assert_eq!(json!(row_badge(n(&c["input"]["unreadCount"]), c["input"]["needsYou"].as_bool().unwrap())), c["expected"], "{}", c["name"]);
    }
    for c in cases("sumCases") {
        assert_eq!(json!(sum_badges(badges(&c["input"]))), c["expected"], "{}", c["name"]);
    }
    for c in cases("teamCases") {
        let i = &c["input"];
        assert_eq!(json!(team_badge(n(&i["needsYou"]), i["open"].as_bool().unwrap(), badges(&i["rows"]))), c["expected"], "{}", c["name"]);
    }
}
