//! #EXP-2109: replays EVERY list of `header-actions.json` through
//! `domain::header_actions`.

use domain::header_actions::*;
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/header-actions.json")).unwrap()
}

fn cases<'a>(f: &'a Value, list: &str) -> &'a Vec<Value> {
    let cases = f[list].as_array().unwrap();
    assert!(!cases.is_empty(), "{list} is empty");
    cases
}

#[test]
fn order_cases() {
    for case in cases(&fixture(), "orderCases") {
        let actions: Vec<HeaderAction> = serde_json::from_value(case["actions"].clone()).unwrap();
        assert_eq!(json!(order_header_actions(&actions)), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn menu_cases() {
    for case in cases(&fixture(), "menuCases") {
        let entries: Vec<&str> = case["entries"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
        assert_eq!(json!(header_menu(&entries)), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn every_pages_lists_are_in_the_one_order() {
    let f = fixture();
    for (page, forms) in f["pages"].as_object().unwrap() {
        assert_eq!(json!(header_actions_phone(page)), forms["phone"], "{page} phone");
        assert_eq!(json!(header_actions_wide(page)), forms["wide"], "{page} wide");
    }
}

#[test]
fn constants() {
    let f = fixture();
    let h = &*HEADER_ACTIONS;
    assert_eq!(json!([h.circle, h.glyph, h.wide_circle, h.wide_glyph, h.gap]), json!([f["circle"].as_f64(), f["glyph"].as_f64(), f["wideCircle"].as_f64(), f["wideGlyph"].as_f64(), f["gap"].as_f64()]));
    assert_eq!(json!(h.order), f["order"]);
    assert_eq!(json!(h.icons), f["icons"]);
    assert_eq!(json!(h.adds), f["adds"]);
    assert_eq!(json!(h.rows), f["rows"]);
    assert_eq!(json!(h.menu_drops), f["menuDrops"]);
}

/// The session screen's menu entries carry the fixture's ids.
#[test]
fn the_session_menu_drops_its_new_conversation() {
    use domain::session_screen::{header_session_menu, SessionScreenMenuEntry as E};
    assert_eq!(
        header_session_menu(&[E::LinkIssue, E::Separator, E::Compact, E::NewConversation, E::Separator, E::Settings]),
        [E::LinkIssue, E::Separator, E::Compact, E::Separator, E::Settings]
    );
}
