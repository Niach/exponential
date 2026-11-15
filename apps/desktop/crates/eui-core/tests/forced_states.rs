//! #EXP-1781: `fixtures/forced-states.json` replayed against the core (the
//! TS reference is packages/exponential-ui/src/forced-states.test.ts): the
//! host's forced states join the style resolution and nothing else, so gpui
//! and the natives (through the FFI) paint what these visuals say.

use std::collections::HashMap;

use eui_core::forced::forced_states_of;
use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use indexmap::IndexMap;
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn map(v: &Value) -> IndexMap<String, Vec<String>> {
    v.as_object().unwrap().iter().map(|(k, s)| (k.clone(), s.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect())).collect()
}

#[test]
fn every_forced_states_case_replays() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/forced-states.json")).unwrap()).unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 7);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let tree: NestedNode = serde_json::from_value(c["tree"].clone()).unwrap();
        let mut s = Surface::new("t", SurfaceOptions::default());
        assert!(s.set_nested(tree).issues.is_empty(), "{name}");
        s.set_viewport(480.0, 900.0, None);
        let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
        s.layout(&mut m);
        let _ = s.take_events();
        let mut dropped = Vec::new();
        for step in c["steps"].as_array().unwrap() {
            if let Some(forced) = step.get("forced") {
                dropped = s.set_forced_states(&map(forced));
            }
            if let Some(host) = step.get("host") {
                let states = host["states"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
                s.set_states(host["id"].as_str().unwrap(), states);
            }
            s.layout(&mut m);
        }
        let out = s.layout(&mut m);
        let e = &c["expect"];
        let want_dropped: Vec<String> = e["dropped"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        assert_eq!(dropped, want_dropped, "{name}: dropped");
        for (id, want) in e["states"].as_object().unwrap() {
            let want: Vec<String> = want.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
            assert_eq!(forced_states_of(s.forced_states(), id), want, "{name}: states of {id}");
        }
        for (id, want) in e["visuals"].as_object().unwrap() {
            let i = s.index_of(id).unwrap_or_else(|| panic!("{name}: no node {id}"));
            let got = serde_json::to_value(s.visual(i).unwrap()).unwrap();
            for (k, v) in want.as_object().unwrap() {
                let key = match k.as_str() {
                    "backgroundColor" => "background_color",
                    "borderColor" => "border_color",
                    other => other,
                };
                let g = &got[key];
                let same = match (g.as_f64(), v.as_f64()) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-4,
                    _ => g == v,
                };
                assert!(same, "{name}: {id}.{k} = {g}, want {v}");
            }
        }
        assert_eq!(out.layers.len() as u64, e["layers"].as_u64().unwrap(), "{name}: layers");
        let events = s.take_events();
        assert!(events.is_empty() && e["events"].as_array().unwrap().is_empty(), "{name}: events {events:?}");
    }
}

#[test]
fn forced_states_never_reach_the_host_states() {
    // Paint only: the FFI's `interaction_states` (the host's own) stay the host's.
    let tree: NestedNode = serde_json::from_value(serde_json::json!({"id": "btn", "component": "Button", "props": {"label": "Save"}})).unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_forced_states(&IndexMap::from([("btn".to_string(), vec!["hover".to_string()])]));
    assert!(s.host_states("btn").is_empty());
}
