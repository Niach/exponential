//! Replays `packages/eengine/fixtures/bundle.json`.

use eengine::runner::RunnerConfig;
use eengine_bundle::contract::BUNDLE;
use eengine_bundle::manifest::platform_manifest;
use eengine_bundle::prepare::prepare_value;
use serde_json::Value;

fn cases(key: &str) -> &'static Vec<Value> {
    BUNDLE[key].as_array().unwrap_or_else(|| panic!("{key} is a list"))
}

#[test]
fn prepare_cases() {
    for c in cases("prepareCases") {
        let bytes = match c.get("fileText").and_then(Value::as_str) {
            Some(text) => text.as_bytes().to_vec(),
            None => serde_json::to_vec(&c["file"]).unwrap(),
        };
        let allow = c.get("allowUnsigned").and_then(Value::as_bool).unwrap_or(false);
        let mut got = prepare_value(&bytes, &c["bundleToml"], allow);
        let expected = &c["expected"];
        // A refusal's detail is compared only where the case states it.
        if let Some(refused) = got.get_mut("refused").and_then(Value::as_object_mut) {
            if expected["refused"].get("detail").is_none() {
                refused.remove("detail");
            }
        }
        assert_eq!(&got, expected, "{}", c["name"]);
    }
}

#[test]
fn permission_cases() {
    for c in cases("permissionCases") {
        let scopes: Vec<String> = serde_json::from_value(c["scopes"].clone()).unwrap();
        let config: RunnerConfig = serde_json::from_value(c["config"].clone()).unwrap();
        let whys = serde_json::from_value(c["whys"].clone()).unwrap();
        assert_eq!(platform_manifest(&scopes, &config, &whys), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn every_target_has_outputs_icons_signing_and_a_location() {
    for t in BUNDLE["targets"].as_array().unwrap() {
        let t = t.as_str().unwrap();
        for key in ["outputs", "icons", "signing"] {
            assert!(!BUNDLE[key][t].is_null(), "{key}.{t}");
        }
        assert!(eengine::contract::RUNNER["bundle"]["locations"][t].is_string(), "runner.json bundle.locations.{t}");
    }
}

#[test]
fn icons_render_at_every_size() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="5" fill="#09090b"/><circle cx="12" cy="12" r="6" fill="#fafafa"/></svg>"##;
    let icon = eengine_bundle::icons::Icon::from_svg(svg).unwrap();
    for t in BUNDLE["targets"].as_array().unwrap() {
        for (_, size) in eengine_bundle::icons::sizes(t.as_str().unwrap()) {
            let png = icon.png(size).unwrap();
            assert_eq!(&png[1..4], b"PNG");
        }
    }
    let rgba = icon.rgba(24).unwrap();
    let centre = ((12 * 24 + 12) * 4) as usize;
    assert_eq!(&rgba[centre..centre + 4], &[0xfa, 0xfa, 0xfa, 0xff]);
}
