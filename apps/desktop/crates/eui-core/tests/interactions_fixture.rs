//! `fixtures/interactions-round1.json` (hand-authored, round 1 review): the
//! interaction semantics the reference renderer has and the core must match
//! — NumberField stepping, rounding and text; Form check semantics; the
//! unbound Table sort; ChoiceTiles picks; Select with an empty option
//! value (VAPP-146). The React half is
//! `packages/exponential-ui-react/src/interactions-fixture.test.tsx`.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/interactions-round1.json")).expect("fixture")).expect("json")
}

fn surface(tree: Value) -> Surface {
    let mut s = surface_with_issues(tree);
    assert!(s.issues().is_empty(), "{:?}", s.issues());
    s
}

/// A check without a message is reported by validation (`message:
/// required`) and still renders; the fixture pins how it reads.
fn surface_with_issues(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_viewport(400.0, 0.0, None);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn text_of(s: &mut Surface, id: &str) -> Value {
    s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no {id}")).props.get("text").cloned().unwrap_or(Value::Null)
}

#[test]
fn number_field_cases_step_round_and_show_like_the_reference() {
    let f = fixture();
    for c in f["numberField"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut props = c["props"].as_object().cloned().unwrap();
        props.insert("label".into(), json!("N"));
        props.insert("name".into(), json!("n"));
        props.insert("value".into(), json!({"path": "/v"}));
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "n", "component": "NumberField", "props": props, "on": {"change": {"event": {"name": "change"}}}}]}));
        if !c["value"].is_null() {
            s.set_data("/v", Some(c["value"].clone())).unwrap();
        }
        let mut m = fixed();
        s.layout(&mut m);
        let events = match c["op"].as_str().unwrap() {
            "increment" | "decrement" => {
                let i = s.index_of(&format!("n.{}", c["op"].as_str().unwrap())).unwrap();
                s.event(i, "press", None)
            }
            "commit" => {
                let i = s.index_of("n.input").unwrap();
                s.event(i, "commit", Some(json!({"value": c["input"]})))
            }
            _ => vec![],
        };
        let mut changed = events.iter().filter_map(|e| match e {
            OutEvent::Action { name, context, .. } if name == "change" => Some(context["value"].clone()),
            _ => None,
        });
        let value = changed.next_back().unwrap_or(c["value"].clone());
        assert_eq!(eui_core::json::canonical(&value), eui_core::json::canonical(&c["expect"]["value"]), "{name}: value");
        if c["op"] != "none" && c["expect"]["value"] != c["value"] {
            assert_eq!(eui_core::json::canonical(s.get_data("/v").unwrap()), eui_core::json::canonical(&c["expect"]["value"]), "{name}: /v");
        }
        s.layout(&mut m);
        assert_eq!(text_of(&mut s, "n.input"), c["expect"]["text"], "{name}: text");
    }
}

#[test]
fn check_cases_fail_and_pass_like_the_reference() {
    let f = fixture();
    for c in f["checks"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface_with_issues(json!({"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"submit": {"event": {"name": "submit"}}, "invalid": {"event": {"name": "invalid"}}},
            "children": [
                {"id": "agree", "component": "Checkbox", "props": {"label": "I agree", "name": "agree", "checked": {"path": "/agreed"}, "checks": c["checks"]}},
                {"id": "go", "component": "Button", "props": {"label": "Send", "submit": true}}
            ]}));
        s.set_data("", Some(c["data"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let failing: Vec<String> = s.failing_checks("agree");
        assert_eq!(json!(failing), c["failing"], "{name}: failing_checks");
        let go = s.index_of("go").unwrap();
        let events = s.event(go, "press", None);
        let invalid: Vec<Value> = events
            .iter()
            .filter_map(|e| match e {
                OutEvent::Action { name, context, .. } if name == "invalid" => Some(context["errors"].as_array().unwrap().iter().map(|e| e["message"].clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .next()
            .unwrap_or_default();
        assert_eq!(json!(invalid), c["failing"], "{name}: invalid errors");
        let submitted = events.iter().any(|e| matches!(e, OutEvent::Action { name, .. } if name == "submit"));
        assert_eq!(json!(submitted), c["submitted"], "{name}: submitted");
    }
}

#[test]
fn table_sort_cases_order_rows_like_the_reference() {
    let f = fixture();
    for c in f["tableSort"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut column = c["column"].clone();
        column["label"] = json!("V");
        column["sortable"] = json!(true);
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Table", "props": {"columns": [column], "rows": c["rows"], "sort": {"key": c["column"]["key"], "direction": c["direction"]}}}]}));
        let mut m = fixed();
        s.layout(&mut m);
        let nodes = s.nodes();
        let body = nodes.iter().find(|n| n.id == "t.body").unwrap();
        let order: Vec<Value> = body
            .children
            .iter()
            .map(|i| &nodes[*i as usize])
            .filter(|n| n.part.as_deref() == Some("row"))
            .map(|n| n.props["index"].clone())
            .collect();
        assert_eq!(json!(order), c["order"], "{name}");
    }
}

#[test]
fn choice_tiles_cases_pick_and_toggle_like_the_reference() {
    let f = fixture();
    for c in f["choiceTiles"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let options: Vec<Value> = ["a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let mut o = json!({"label": v.to_uppercase(), "value": v, "image": format!("https://example.com/{v}.png")});
                if c["disabled"].as_u64() == Some(i as u64) {
                    o["disabled"] = json!(true);
                }
                o
            })
            .collect();
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "c", "component": "ChoiceTiles", "props": {"name": "c", "type": c["type"], "options": options, "value": {"path": "/v"}}, "on": {"change": {"event": {"name": "change"}}}}]}));
        if !c["value"].is_null() {
            s.set_data("/v", Some(c["value"].clone())).unwrap();
        }
        let mut m = fixed();
        let mut changes = vec![];
        // Press a different part each time: every part of a tile picks it.
        let parts = ["tile", "label", "picture"];
        for (k, i) in c["presses"].as_array().unwrap().iter().enumerate() {
            s.layout(&mut m);
            let idx = s.index_of(&format!("c.{}.{}", parts[k % parts.len()], i)).unwrap_or_else(|| panic!("{name}: no tile {i}"));
            for e in s.event(idx, "press", None) {
                if let OutEvent::Action { name, context, .. } = e {
                    if name == "change" {
                        changes.push(context["value"].clone());
                    }
                }
            }
        }
        assert_eq!(json!(changes), c["expect"]["changes"], "{name}: changes");
        let v = s.get_data("/v").cloned().unwrap_or(Value::Null);
        let expected = if c["expect"]["value"] == c["value"] && c["value"].is_null() { Value::Null } else { c["expect"]["value"].clone() };
        assert_eq!(v, expected, "{name}: /v");
        // The picked tiles carry `selected`.
        s.layout(&mut m);
        let picked: Vec<String> = s.nodes().into_iter().filter(|n| n.part.as_deref() == Some("tile") && n.props.get("selected") == Some(&json!(true))).map(|n| n.props["value"].as_str().unwrap().to_string()).collect();
        assert_eq!(picked.join(","), c["expect"]["value"].as_str().unwrap(), "{name}: selected tiles");
    }
}

#[test]
fn select_cases_take_an_empty_option_value_like_the_reference() {
    let f = fixture();
    for c in f["select"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let options = json!([{"label": "Any", "value": ""}, {"label": "Open", "value": "open"}, {"label": "Done", "value": "done"}]);
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "s", "component": "Select", "props": {"label": "Status", "name": "s", "options": options, "value": {"path": "/v"}}, "on": {"change": {"event": {"name": "change"}}}}]}));
        if !c["value"].is_null() {
            s.set_data("/v", Some(c["value"].clone())).unwrap();
        }
        let mut m = fixed();
        let mut changes = vec![];
        for i in c["presses"].as_array().unwrap() {
            s.layout(&mut m);
            let trigger = s.index_of("s.trigger").unwrap_or_else(|| panic!("{name}: no trigger"));
            s.event(trigger, "press", None);
            s.layout(&mut m);
            let item = s.index_of(&format!("s.item.{i}")).unwrap_or_else(|| panic!("{name}: no option {i}"));
            for e in s.event(item, "press", None) {
                // The open/close `change {open}` is not a pick.
                if let OutEvent::Action { name, context, .. } = e {
                    if name == "change" && context.get("value").is_some() {
                        changes.push(context["value"].clone());
                    }
                }
            }
        }
        assert_eq!(json!(changes), c["expect"]["changes"], "{name}: changes");
        assert_eq!(s.get_data("/v").cloned().unwrap_or(Value::Null), c["expect"]["value"], "{name}: /v");
        s.layout(&mut m);
        let trigger = s.nodes().into_iter().find(|n| n.id == "s.trigger").unwrap_or_else(|| panic!("{name}: no trigger"));
        assert_eq!(trigger.props["text"], c["expect"]["shows"], "{name}: the trigger shows");
        assert_eq!(trigger.props["placeholder"], json!(false), "{name}: never the placeholder");
        assert!(s.index_of("s.item.0").is_none(), "{name}: the list closed");
    }
}

/// VAPP-162: `buttonHold` (round-5 contract §9), through `Surface::hold`.
/// A `hold: false` case is the host's plain press: pointer down paints
/// `pressed` (the host's state), release over the box dispatches `press`.
#[test]
fn button_hold_cases_fire_start_end_and_cancel_like_the_reference() {
    use eui_core::hold::HoldInput;
    let f = fixture();
    let cases = f["buttonHold"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let mut props = json!({"label": "Hold to talk", "holdingLabel": "Holding…", "hold": true}).as_object().cloned().unwrap();
        for (k, v) in c.get("props").and_then(Value::as_object).into_iter().flatten() {
            if v.is_null() {
                props.remove(k);
            } else {
                props.insert(k.clone(), v.clone());
            }
        }
        let plain = props.get("hold") != Some(&json!(true));
        let on = json!({"press": {"event": {"name": "press"}}, "holdStart": {"event": {"name": "holdStart"}}, "holdEnd": {"event": {"name": "holdEnd"}}, "holdCancel": {"event": {"name": "holdCancel"}}});
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "b", "component": "Button", "props": props, "on": on, "style": {"width": 120, "height": 40}}]}));
        let mut m = FixedMeasure::with_sizes(HashMap::from([("b".to_string(), (120.0, 40.0))]));
        s.layout(&mut m);
        let i = s.index_of("b").unwrap();
        let frame = s.last_frame(i).unwrap();
        assert_eq!((frame.w, frame.h), (120.0, 40.0), "{name}");
        let mut host_pressed = false;
        for (k, step) in c["steps"].as_array().unwrap().iter().enumerate() {
            let input: HoldInput = serde_json::from_value(step.clone()).unwrap_or_else(|e| panic!("{name} step {k}: {e}"));
            let events = if plain {
                match &input {
                    HoldInput::PointerDown { .. } => {
                        host_pressed = true;
                        vec![]
                    }
                    HoldInput::PointerUp { x, y } => {
                        host_pressed = false;
                        if (0.0..=frame.w).contains(x) && (0.0..=frame.h).contains(y) { s.event(i, "press", None) } else { vec![] }
                    }
                    _ => vec![],
                }
            } else {
                s.hold(i, &input)
            };
            s.layout(&mut m);
            let fired: Vec<&str> = events.iter().filter_map(|e| match e {
                OutEvent::Action { name, .. } => Some(name.as_str()),
                _ => None,
            }).collect();
            let expected: Vec<&str> = step["events"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
            assert_eq!(fired, expected, "{name} step {k}: events");
            let node = s.nodes().into_iter().find(|n| n.id == "b").unwrap();
            let pressed = host_pressed || node.states.iter().any(|st| st == "pressed");
            assert_eq!(pressed, step["pressed"].as_bool().unwrap(), "{name} step {k}: pressed");
            assert_eq!(node.props.get("label"), Some(&step["label"]), "{name} step {k}: label");
        }
    }
}
