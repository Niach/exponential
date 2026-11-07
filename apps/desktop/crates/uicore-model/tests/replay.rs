//! Replays `packages/uicore/fixtures/shell.json`: every `reduce` case and
//! every selector case, byte for byte against the TypeScript reference.

use serde_json::{json, Value};
use uicore_model::*;

const FIXTURE: &str = include_str!("../../../../../packages/uicore/fixtures/shell.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("shell.json")
}

#[test]
fn the_initial_state_is_the_fixtures() {
    let f = fixture();
    assert_eq!(serde_json::to_value(ShellState::default()).unwrap(), f["initial"]);
}

#[test]
fn every_reduce_case_replays() {
    let f = fixture();
    let cases = f["reduce"].as_array().expect("reduce cases");
    assert!(cases.len() >= 60, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let state = hydrate(&case["state"]).unwrap_or_else(|e| panic!("{name}: state: {e}"));
        let event: ShellEvent = serde_json::from_value(case["event"].clone()).unwrap_or_else(|e| panic!("{name}: event: {e}"));
        let after = serde_json::to_value(reduce(&state, &event)).unwrap();
        let before = serde_json::to_value(&state).unwrap();
        let expected = case["expected"].as_object().expect("expected");
        for (key, value) in before.as_object().unwrap() {
            let want = expected.get(key).unwrap_or(value);
            assert_eq!(&after[key], want, "{name}: {key}");
        }
    }
}

fn apps_of(f: &Value, case: &Value) -> (Vec<StartMenuApp>, Vec<String>) {
    let apps = case["args"].get("apps").unwrap_or(&f["apps"]).clone();
    let favourites = case["args"].get("favourites").unwrap_or(&f["favourites"]).clone();
    (serde_json::from_value(apps).unwrap(), serde_json::from_value(favourites).unwrap())
}

#[test]
fn every_selector_case_replays() {
    let f = fixture();
    let selectors = f["selectors"].as_object().expect("selectors");
    let mut total = 0;
    for (selector, cases) in selectors {
        for case in cases.as_array().unwrap() {
            total += 1;
            let name = case["name"].as_str().unwrap_or("?");
            let state = hydrate(&case["state"]).unwrap_or_else(|e| panic!("{name}: state: {e}"));
            let got = match selector.as_str() {
                "taskbarButtons" => json!(taskbar_buttons(&state)),
                "switcherSections" => json!(switcher_sections(&state)),
                "taskbarPress" => json!(taskbar_press(&state, case["args"]["app"].as_str().unwrap())),
                "taskbarScroll" => {
                    let direction: ScrollDirection = serde_json::from_value(case["args"]["direction"].clone()).unwrap();
                    json!(taskbar_scroll(&state, case["args"]["app"].as_str().unwrap(), direction))
                }
                "workspaceBadges" => json!(workspace_badges(&state)),
                "altTabOrder" => json!(alt_tab_order(&state)),
                "startMenuApps" => {
                    let (apps, favourites) = apps_of(&f, case);
                    json!(start_menu_apps(&state, &apps, &favourites))
                }
                "startMenuCategories" => {
                    let (apps, favourites) = apps_of(&f, case);
                    json!(start_menu_categories(&apps, &favourites))
                }
                other => panic!("unknown selector {other}"),
            };
            assert_eq!(got, case["expected"], "{selector}: {name}");
        }
    }
    assert!(total >= 40, "{total} selector cases");
}
