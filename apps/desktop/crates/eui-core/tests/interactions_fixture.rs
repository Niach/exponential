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
        props.insert("value".into(), json!({"@path": "/v"}));
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
                {"id": "agree", "component": "Checkbox", "props": {"label": "I agree", "name": "agree", "checked": {"@path": "/agreed"}, "checks": c["checks"]}},
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

fn states(s: &mut Surface, id: &str) -> Vec<String> {
    s.nodes().into_iter().find(|n| n.id == id).map(|n| n.states.clone()).unwrap_or_default()
}

/// Round 9 §6.6: what a field does with each severity.
#[test]
fn check_severities_show_one_message_and_only_an_error_blocks() {
    let f = fixture();
    for c in f["checkSeverities"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface_with_issues(json!({"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"submit": {"event": {"name": "submit"}}, "invalid": {"event": {"name": "invalid"}}},
            "children": [
                {"id": "x", "component": "Input", "props": {"label": "X", "name": "x", "value": "v", "validateOn": "submit", "checks": c["checks"]}},
                {"id": "go", "component": "Button", "props": {"label": "Send", "submit": true}}
            ]}));
        s.set_data("", Some(c["data"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let go = s.index_of("go").unwrap();
        let events = s.event(go, "press", None);
        let errors: Vec<Value> = events
            .iter()
            .filter_map(|e| match e {
                OutEvent::Action { name, context, .. } if name == "invalid" => Some(context["errors"].as_array().unwrap().iter().map(|e| e["message"].clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .next()
            .unwrap_or_default();
        assert_eq!(json!(errors), c["errors"], "{name}: errors");
        let submitted = events.iter().any(|e| matches!(e, OutEvent::Action { name, .. } if name == "submit"));
        assert_eq!(json!(submitted), c["submitted"], "{name}: submitted");
        s.layout(&mut m);
        let part = s.nodes().into_iter().find(|n| n.id == "x.error");
        match c["shown"].as_object() {
            None => assert!(part.is_none(), "{name}: no message"),
            Some(shown) => {
                let part = part.unwrap_or_else(|| panic!("{name}: no x.error"));
                assert_eq!(part.props["text"], shown["message"], "{name}: message");
                let state = if shown["severity"] == "error" { "invalid" } else { shown["severity"].as_str().unwrap() };
                assert_eq!(part.states, vec![state.to_string()], "{name}: part state");
            }
        }
        assert_eq!(json!(states(&mut s, "x.field").contains(&"invalid".to_string())), c["invalid"], "{name}: field invalid");
    }
}

#[test]
fn button_checks_disable_on_an_error_alone() {
    let f = fixture();
    for c in f["buttonChecks"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface_with_issues(json!({"id": "root", "component": "Box", "children": [
            {"id": "b", "component": "Button", "props": {"label": "Go", "checks": c["checks"]}, "on": {"press": {"event": {"name": "press"}}}}
        ]}));
        s.set_data("", Some(c["data"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let b = s.index_of("b").unwrap();
        let events = s.event(b, "press", None);
        let pressed = events.iter().any(|e| matches!(e, OutEvent::Action { name, .. } if name == "press"));
        assert_eq!(json!(!pressed), c["disabled"], "{name}: press");
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

/// VAPP-206: a `rowPress` action's relative paths read the pressed row.
#[test]
fn row_press_cases_read_the_pressed_row() {
    let f = fixture();
    for c in f["rowPress"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Table", "props": {"columns": [{"key": "name", "label": "Name"}], "rows": {"@path": "/rows"}}, "on": {"rowPress": {"functionCall": {"@call": "set", "args": {"path": "/picked", "value": {"@path": c["value"]}}}}}}]}));
        s.set_data("/rows", Some(c["rows"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let key = c["rows"][c["press"].as_u64().unwrap() as usize]["id"].clone();
        let index = s.nodes().iter().position(|n| n.part.as_deref() == Some("row") && n.props.get("key") == Some(&key)).unwrap_or_else(|| panic!("{name}: no row"));
        s.event(index as _, "press", None);
        assert_eq!(s.get_data("/picked"), Some(&c["expect"]), "{name}");
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
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "c", "component": "ChoiceTiles", "props": {"name": "c", "type": c["type"], "options": options, "value": {"@path": "/v"}}, "on": {"change": {"event": {"name": "change"}}}}]}));
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
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "s", "component": "Select", "props": {"label": "Status", "name": "s", "options": options, "value": {"@path": "/v"}}, "on": {"change": {"event": {"name": "change"}}}}]}));
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

/// #VAPP-284: a foldable Row: the head presses or folds, the fold button
/// folds, ArrowRight / ArrowLeft open / close (swapped in rtl) through
/// `event(index, "key", {key})`, the one call every painter forwards.
#[test]
fn fold_row_cases_press_fold_and_take_the_arrows_like_the_reference() {
    let f = fixture();
    let fold = &f["foldRow"];
    for c in fold["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut row = fold["surface"].clone();
        if c["pressable"] == json!(false) {
            row["on"].as_object_mut().unwrap().remove("press");
        }
        let tree = if c["rtl"] == json!(true) { json!({"id": "rtl", "component": "Box", "style": {"direction": "rtl"}, "children": [row]}) } else { row };
        let mut s = surface(tree);
        if !c["open"].is_null() {
            s.set_data("/open", Some(c["open"].clone())).unwrap();
        }
        let mut m = fixed();
        s.layout(&mut m);
        let id = |part: &str| if part == "head" { "issues.head".to_string() } else { "issues.head.fold".to_string() };
        let step = &c["step"];
        let (target, event, payload) = match step.get("press") {
            Some(p) => (id(p.as_str().unwrap()), "press", None),
            None => (id(step["on"].as_str().unwrap()), "key", Some(json!({"key": step["key"]}))),
        };
        let index = s.nodes().iter().position(|n| n.id == target).unwrap_or_else(|| panic!("{name}: no {target}"));
        let out = s.event(index as _, event, payload);
        let names: Vec<String> = out
            .iter()
            .filter_map(|e| match e {
                OutEvent::Action { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(json!(names), c["events"], "{name}: events");
        s.layout(&mut m);
        let head = s.nodes().into_iter().find(|n| n.id == "issues.head").unwrap();
        let expanded = head.accessibility.as_ref().and_then(|a| a.get("expanded")).cloned();
        assert_eq!(expanded, Some(c["after"].clone()), "{name}: expanded after");
    }
}

/// #VAPP-285: the Tree: a leaf press selects, a branch press folds, a lazy
/// branch fetches, the arrows (`event(index, "key")`) follow `tree_key` over
/// the visible rows and answer with a `Focus` event.
#[test]
fn tree_cases_select_fold_fetch_and_move_like_the_reference() {
    let f = fixture();
    let t = &f["tree"];
    let row_ids = t["rowIds"].as_object().unwrap();
    let item_of = |node: &str| row_ids.iter().find(|(_, id)| id.as_str() == Some(node)).map(|(item, _)| item.clone());
    for c in t["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let tree = if c["rtl"] == json!(true) { json!({"id": "rtl", "component": "Box", "style": {"direction": "rtl"}, "children": [t["surface"].clone()]}) } else { t["surface"].clone() };
        let mut s = surface(tree);
        s.set_data("/open", Some(c["open"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let step = &c["step"];
        let (item, event, payload) = match step.get("press") {
            Some(p) => (p.as_str().unwrap(), "press", None),
            None => (step["on"].as_str().unwrap(), "key", Some(json!({"key": step["key"]}))),
        };
        let target = row_ids[item].as_str().unwrap();
        let index = s.nodes().iter().position(|n| n.id == target).unwrap_or_else(|| panic!("{name}: no {target}"));
        let out = s.event(index as _, event, payload);
        let names: Vec<String> = out.iter().filter_map(|e| if let OutEvent::Action { name, .. } = e { Some(name.clone()) } else { None }).collect();
        assert_eq!(json!(names), c["events"], "{name}: events");
        let focus = out.iter().find_map(|e| if let OutEvent::Focus { id, .. } = e { item_of(id) } else { None });
        assert_eq!(json!(focus), c["after"]["focus"], "{name}: focus");
        assert_eq!(s.get_data("/open"), Some(&c["after"]["open"]), "{name}: /open");
        assert_eq!(s.get_data("/picked").cloned().unwrap_or(Value::Null), c["after"]["picked"], "{name}: /picked");
        if let Some(visible) = c["after"].get("visible") {
            s.layout(&mut m);
            // Slots are not tree order: sort the shown rows by their index.
            let mut rows: Vec<(usize, String)> = s
                .nodes()
                .into_iter()
                .filter(|n| !n.hidden && n.id.starts_with("files.row.") && n.id["files.row.".len()..].chars().all(|ch| ch.is_ascii_digit()))
                .filter_map(|n| Some((n.id["files.row.".len()..].parse().ok()?, item_of(&n.id)?)))
                .collect();
            rows.sort();
            let shown: Vec<String> = rows.into_iter().map(|(_, item)| item).collect();
            assert_eq!(json!(shown), *visible, "{name}: visible");
        }
    }
}

/// #VAPP-288 fieldWrites at the core (it has no focus): the value at /draft
/// after each case's steps, and the write epoch: an action's `set` (the
/// press) moves it, the field's own edit (`type` = its `change`) and a
/// host's plain `set_data` (`hostWrite`) never do. The focus half (what a
/// focused field shows) replays in each painter.
#[test]
fn field_write_cases_move_the_write_epoch_only_on_a_programmatic_write() {
    let f = fixture();
    for c in f["fieldWrites"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut save = json!({"id": "save", "component": "Button", "props": {"label": "Save"}});
        if let Some(path) = c.get("pressPath") {
            save["on"] = json!({"press": {"functionCall": {"@call": "set", "args": {"path": path, "value": c["pressValue"]}}}});
        }
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "draft", "component": "Input", "props": {"label": "Draft", "name": "draft", "value": {"@path": "/draft"}}}, save]}));
        s.set_data("/draft", Some(c["seed"].clone())).unwrap();
        let mut m = fixed();
        s.layout(&mut m);
        let mut draft = c["seed"].clone();
        for step in c["steps"].as_array().unwrap() {
            let before = s.write_epoch();
            let index = |s: &mut Surface, pick: &dyn Fn(&eui_core::surface::PlacedNode) -> bool| s.nodes().iter().position(|n| pick(n)).unwrap_or_else(|| panic!("{name}: no node")) as u32;
            let moves = match step["op"].as_str().unwrap() {
                "type" => {
                    let i = index(&mut s, &|n| n.component == "Input" && n.part.as_deref() == Some("field"));
                    s.event(i as _, "change", Some(json!({"value": step["text"]})));
                    draft = step["text"].clone();
                    false
                }
                "hostWrite" => {
                    s.set_data(step["path"].as_str().unwrap(), Some(step["value"].clone())).unwrap();
                    if step["path"] == json!("/draft") {
                        draft = step["value"].clone();
                    }
                    false
                }
                "press" => {
                    let i = index(&mut s, &|n| n.id == "save");
                    s.event(i as _, "press", None);
                    if c["pressPath"] == json!("/draft") {
                        draft = c["pressValue"].clone();
                    }
                    c.get("pressPath").is_some()
                }
                _ => false, // focus / blur: the painter's half.
            };
            assert_eq!(s.write_epoch() != before, moves, "{name}: epoch after {step}");
            s.layout(&mut m);
        }
        assert_eq!(s.get_data("/draft"), Some(&draft), "{name}: /draft");
        if let Some(data) = c["expect"].get("data") {
            assert_eq!(s.get_data("/draft"), Some(data), "{name}: expect.data");
        }
    }
}

/// #VAPP-288: [`Surface::assign_data`] is a programmatic write, a refused
/// one bumps nothing.
#[test]
fn assign_data_bumps_the_write_epoch_and_set_data_does_not() {
    let mut s = surface(json!({"id": "root", "component": "Box"}));
    assert_eq!(s.write_epoch(), 0);
    s.set_data("/a", Some(json!(1))).unwrap();
    assert_eq!(s.write_epoch(), 0);
    s.assign_data("/a", Some(json!(2))).unwrap();
    assert_eq!((s.write_epoch(), s.get_data("/a")), (1, Some(&json!(2))));
    s.set_data("/list", Some(json!([]))).unwrap();
    assert!(s.assign_data("/list/9", Some(json!(1))).is_err());
    assert_eq!(s.write_epoch(), 1);
}

/// Round 8: `checkbox`: a mixed box resolves to checked (both paths
/// written); a plain toggle leaves `indeterminate` alone; `change` carries
/// both. The state = what the `box` part says (mixed wins).
#[test]
fn checkbox_cases_resolve_mixed_like_the_reference() {
    let f = fixture();
    let section = &f["checkbox"];
    for c in section["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut tree = section["surface"].clone();
        if let Some(Value::Object(u)) = c.get("unbound") {
            for (k, v) in u {
                tree["props"][k] = v.clone();
            }
        }
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [tree]}));
        if let Some(Value::Object(data)) = c.get("data") {
            for (k, v) in data {
                s.set_data(&format!("/{k}"), Some(v.clone())).unwrap();
            }
        }
        let mut m = fixed();
        let state = |s: &mut Surface, m: &mut FixedMeasure| -> (Value, Value, String) {
            s.layout(m);
            let b = s.nodes().into_iter().find(|n| n.id == "c.box").expect("c.box");
            let (ch, mx) = (b.props.get("checked").cloned().unwrap_or(json!(false)), b.props.get("indeterminate").cloned().unwrap_or(json!(false)));
            let st = if mx == json!(true) { "mixed" } else if ch == json!(true) { "true" } else { "false" };
            (ch, mx, st.to_string())
        };
        assert_eq!(state(&mut s, &mut m).2, c["before"].as_str().unwrap(), "{name}: before");
        for (k, step) in c["steps"].as_array().unwrap().iter().enumerate() {
            let idx = s.index_of("c.box").expect("box");
            let mut change = Value::Null;
            for e in s.event(idx, "press", None) {
                if let OutEvent::Action { name, context, .. } = e {
                    if name == "change" {
                        change = json!({"checked": context["checked"], "indeterminate": context["indeterminate"]});
                    }
                }
            }
            assert_eq!(change, step["change"], "{name} #{k}: change");
            let (ch, mx, st) = state(&mut s, &mut m);
            if c.get("unbound").is_some() {
                assert_eq!(ch, step["checked"], "{name} #{k}: checked");
                assert_eq!(mx, step["mixed"], "{name} #{k}: mixed");
            } else {
                assert_eq!(s.get_data("/checked").cloned().unwrap_or(Value::Null), step["checked"], "{name} #{k}: /checked");
                assert_eq!(s.get_data("/mixed").cloned().unwrap_or(Value::Null), step["mixed"], "{name} #{k}: /mixed");
            }
            assert_eq!(st, step["state"].as_str().unwrap(), "{name} #{k}: state");
        }
    }
}

/// #EXP-1739 overlayAnchor: an open anchored Popover closes once its anchor
/// scrolls (`scroll` on the container), moves (a node inserted above it) or
/// leaves the tree; the core raises the close through `take_events` and the
/// next pass drops the layer. A pass that moves nothing keeps it.
#[test]
fn overlay_anchor_cases_close_on_scroll_move_and_removal_like_the_reference() {
    let f = fixture();
    let fx = &f["overlayAnchor"];
    for c in fx["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let base = fx["surface"].clone();
        let mut s = surface(base.clone());
        let mut m = fixed();
        let open = |s: &mut Surface, m: &mut FixedMeasure| s.layout(m).layers.iter().any(|l| l.owner == "pop");
        assert!(open(&mut s, &mut m), "{name}: opens against its anchor");
        assert!(open(&mut s, &mut m), "{name}: a second pass keeps it");
        let step = &c["step"];
        let mut tree = base.clone();
        let children = tree["children"].as_array_mut().unwrap();
        if step["insertAbove"] == json!(true) {
            children.insert(0, fx["above"].clone());
        }
        if step["removeAnchor"] == json!(true) {
            children.retain(|child| child["id"] != json!("target"));
        }
        if tree != base {
            s.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap());
        }
        if let Some(container) = step["scroll"].as_str() {
            s.scroll(container, step["y"].as_f64().unwrap() as f32);
        }
        s.layout(&mut m);
        let _ = s.take_events();
        assert_eq!(open(&mut s, &mut m), c["after"].as_bool().unwrap(), "{name}");
    }
}

/// #EXP-1629 keyRecorder at the core: what every painter sends for a step
/// (`press`, `record` with the canonical payload, `blur`) and the guards the
/// core keeps (a press only while idle; `record` and `blur` only while
/// recording). The capture half (platform keys → payload, focus) replays in
/// each painter.
#[test]
fn key_recorder_cases_start_record_and_stop_like_the_reference() {
    let f = fixture();
    let k = &f["keyRecorder"];
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface(k["surface"].clone());
        let mut m = fixed();
        s.layout(&mut m);
        let mut fired = vec![];
        let flags = |mods: &Value| {
            let held = |m: &str| mods.as_array().is_some_and(|a| a.iter().any(|x| x == m));
            json!({"ctrl": held("ctrl"), "alt": held("alt"), "shift": held("shift"), "meta": held("meta")})
        };
        for step in c["steps"].as_array().unwrap() {
            let field = s.nodes().iter().position(|n| n.id == "ptt.field").unwrap_or_else(|| panic!("{name}: no field")) as u32;
            let (kind, value) = step.as_object().unwrap().iter().next().unwrap();
            let out = match kind.as_str() {
                "press" => s.event(field, "press", None),
                "blur" => s.event(field, "blur", None),
                "down" | "up" => {
                    let mut p = flags(&step["mods"]);
                    p["type"] = json!(kind);
                    p["key"] = value.clone();
                    s.event(field, "record", Some(p))
                }
                "modifiers" => {
                    let mut p = flags(value);
                    p["type"] = json!("modifiers");
                    s.event(field, "record", Some(p))
                }
                "mouse" => {
                    let button = value.as_u64().unwrap();
                    let mut p = flags(&Value::Null);
                    p["type"] = json!("mouse");
                    p["button"] = json!(button);
                    if (3..=5).contains(&button) { s.event(field, "record", Some(p)) } else { vec![] }
                }
                "hostWrite" => {
                    s.set_data(value["path"].as_str().unwrap(), Some(value["value"].clone())).unwrap();
                    vec![]
                }
                other => panic!("{name}: step {other}"),
            };
            fired.extend(out.into_iter().filter_map(|e| if let OutEvent::Action { name, context, .. } = e { Some(json!({"name": name, "context": context})) } else { None }));
            s.layout(&mut m);
        }
        assert_eq!(json!(fired), c["events"], "{name}: events");
        assert_eq!(s.get_data("/recording").cloned().unwrap_or(Value::Null), c["recording"], "{name}: /recording");
    }
}

/// #EXP-1760 pickerPick at the core: a press on the trigger opens the
/// IconPicker / ColorPicker popover; a pick (a cell, a swatch, the No icon
/// row) writes `value`, fires the author's `change` {value} and closes it.
/// gpui, SwiftUI and Compose send the same `press` through the core.
#[test]
fn picker_pick_cases_write_fire_and_close_like_the_reference() {
    let f = fixture();
    let k = &f["pickerPick"];
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let tree = k["surfaces"][c["surface"].as_str().unwrap()].clone();
        let owner = tree["id"].as_str().unwrap().to_string();
        let mut s = surface(tree);
        for (path, value) in c["data"].as_object().unwrap() {
            s.set_data(&format!("/{path}"), Some(value.clone())).unwrap();
        }
        let mut m = fixed();
        s.layout(&mut m);
        let mut fired = vec![];
        for step in c["steps"].as_array().unwrap() {
            let id = step["press"].as_str().unwrap_or_else(|| panic!("{name}: step {step}"));
            let at = s.nodes().iter().position(|n| n.id == id).unwrap_or_else(|| panic!("{name}: no {id}")) as u32;
            let out = s.event(at, "press", None);
            fired.extend(out.into_iter().filter_map(|e| if let OutEvent::Action { name, context, .. } = e { Some(json!({"name": name, "context": context})) } else { None }));
            s.layout(&mut m);
        }
        let _ = s.take_events();
        assert_eq!(json!(fired), c["events"], "{name}: events");
        assert_eq!(s.get_data("/value").cloned().unwrap_or(Value::Null), c["value"], "{name}: /value");
        let open = s.layout(&mut m).layers.iter().any(|l| l.owner == owner);
        assert_eq!(open, c["open"].as_bool().unwrap(), "{name}: open");
    }
}

/// #EDE-65 composerKeys at the core: what a painter sends for each step
/// (`change` for the field's edit; with menuKeys on, a menu key without a
/// modifier or an IME composition = `key` {key}; else the field's own Enter
/// = `submit` {value} then `change` "") fires and writes like the
/// reference. The capture half (which platform keys a Composer takes)
/// replays in each painter.
#[test]
fn composer_key_cases_fire_key_or_submit_like_the_reference() {
    let f = fixture();
    let k = &f["composerKeys"];
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface(k["surface"].clone());
        for (path, value) in c["data"].as_object().unwrap() {
            s.set_data(&format!("/{path}"), Some(value.clone())).unwrap();
        }
        let mut m = fixed();
        s.layout(&mut m);
        let mut fired = vec![];
        let mut text = String::new();
        for step in c["steps"].as_array().unwrap() {
            let composer = s.nodes().iter().position(|n| n.id == "c").unwrap_or_else(|| panic!("{name}: no composer")) as u32;
            let out = if let Some(t) = step["type"].as_str() {
                text = t.to_string();
                s.event(composer, "change", Some(json!({"value": t})))
            } else if let Some(key) = step["key"].as_str() {
                let plain = step["mods"].as_array().is_none_or(|a| a.is_empty()) && step["composing"].as_bool() != Some(true);
                let menu = s.get_data("/menu") == Some(&json!(true));
                if menu && plain {
                    s.event(composer, "key", Some(json!({"key": key})))
                } else if key == "Enter" && plain && !text.trim().is_empty() {
                    let mut out = s.event(composer, "submit", Some(json!({"value": text.trim()})));
                    text.clear();
                    out.extend(s.event(composer, "change", Some(json!({"value": ""}))));
                    out
                } else {
                    vec![]
                }
            } else if let Some(w) = step.get("hostWrite") {
                s.set_data(w["path"].as_str().unwrap(), Some(w["value"].clone())).unwrap();
                vec![]
            } else {
                panic!("{name}: step {step}")
            };
            fired.extend(out.into_iter().filter_map(|e| if let OutEvent::Action { name, context, .. } = e { Some(json!({"name": name, "context": context})) } else { None }));
            s.layout(&mut m);
        }
        assert_eq!(json!(fired), c["events"], "{name}: events");
        assert_eq!(s.get_data("/draft").cloned().unwrap_or(Value::Null), c["draft"], "{name}: /draft");
    }
}

/// #EDE-65 R composerSlots at the core: the slots sit in the Composer's
/// controls row (the send disc's row at the bottom of its content box):
/// `leading` at the start edge, `trailing` one gap before the disc, every
/// slot centred on it, the composer's size unchanged; a slot child is a
/// live node. The attach control is painted by each painter (its place
/// before `trailing` replays there).
#[test]
fn composer_slot_cases_sit_in_the_controls_row_like_the_reference() {
    let f = fixture();
    let k = &f["composerSlots"];
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let rtl = c["direction"] == "rtl";
        let mut composer = k["surface"].clone();
        for (p, v) in c["props"].as_object().unwrap() {
            composer["props"][p] = v.clone();
        }
        let slots = c["slots"].as_object().unwrap();
        if !slots.is_empty() {
            composer["slots"] = c["slots"].clone();
        }
        let plain = {
            let mut p = k["surface"].clone();
            for (key, v) in c["props"].as_object().unwrap() {
                p["props"][key] = v.clone();
            }
            p
        };
        let run = |tree: Value| {
            let root = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": if rtl { "rtl" } else { "ltr" }}, "children": [tree]});
            let mut s = Surface::new("t", SurfaceOptions { theme: eui_core::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
            s.set_nested(serde_json::from_value(root).unwrap());
            assert!(s.issues().is_empty(), "{name}: {:?}", s.issues());
            s.set_viewport(400.0, 0.0, None);
            let out = s.layout(&mut fixed());
            let nodes = s.nodes();
            let frames: HashMap<String, (f32, f32, f32, f32)> = out.frames.iter().map(|f| (nodes[f.index as usize].id.clone(), (f.x, f.y, f.w, f.h))).collect();
            (s, frames)
        };
        let (mut s, frames) = run(composer);
        let (_, bare) = run(plain);
        let frame = |id: &str| *frames.get(id).unwrap_or_else(|| panic!("{name}: no frame for {id}"));
        let cf = frame("c");
        let ci = s.index_of("c").unwrap();
        let v = s.visual(ci).cloned().unwrap();
        let pad = v.padding.unwrap_or([0.0; 4]);
        let b = v.border_width.unwrap_or(0.0);
        let (start, end) = if rtl { (cf.0 + cf.2 - pad[1] - b, cf.0 + pad[3] + b) } else { (cf.0 + pad[3] + b, cf.0 + cf.2 - pad[1] - b) };
        let token = |t: &str| eui_core::theme::resolve_token(s.theme().unwrap(), &json!(t), s.mode()).and_then(|v| v.as_f64()).unwrap() as f32;
        let xs = token("$spacing.xs");
        let near = |a: f32, b: f32, what: &str| assert!((a - b).abs() < 0.01, "{name}: {what}: {a} vs {b}");
        if slots.contains_key("above") {
            // #EXP-1762: `above` sits at the content top, the content box's
            // full width, and SIZES the composer: + its height + the root's gap.
            let above = frame("c.above");
            let strip = frame("strip");
            let gap = token(if c["props"]["layout"] == "flat" { "$spacing.xs" } else { "$spacing.sm" });
            let cw = cf.2 - pad[1] - pad[3] - 2.0 * b;
            near(above.1, cf.1 + pad[0] + b, "above at the content top");
            near(above.0, cf.0 + pad[3] + b, "above starts at the content's left edge");
            near(above.2, cw, "above spans the content width");
            near(strip.2, cw, "the slot child spans the content width");
            near(cf.3, bare["c"].3 + above.3 + gap, "above grows the composer by its height + the gap");
            assert_eq!((cf.0, cf.1, cf.2), (bare["c"].0, bare["c"].1, bare["c"].2), "{name}: the width is unchanged");
            let stack: Vec<&str> = c["stack"].as_array().unwrap().iter().map(|o| o.as_str().unwrap()).collect();
            assert_eq!(stack.first(), Some(&"above"), "{name}: stack");
        } else {
            assert_eq!(cf, bare["c"], "{name}: the slots never size the composer");
        }
        if slots.is_empty() {
            assert!(!frames.contains_key("c.bar"), "{name}: no row without slots");
            continue;
        }
        let send = frame("c.sendSpace");
        let edge = |f: (f32, f32, f32, f32), lead: bool| if rtl == lead { f.0 + f.2 } else { f.0 };
        near(edge(send, false), end, "send ends at the content end");
        near(send.1 + send.3, cf.1 + cf.3 - pad[2] - b, "the row sits at the content bottom");
        let centre = send.1 + send.3 / 2.0;
        let order: Vec<&str> = c["order"].as_array().unwrap().iter().map(|o| o.as_str().unwrap()).filter(|o| *o != "attachment").collect();
        for (slot, id) in [("leading", "plus"), ("trailing", "ring")] {
            if !slots.contains_key(slot) {
                assert!(!frames.contains_key(&format!("c.{slot}")), "{name}: no {slot} wrapper");
                continue;
            }
            assert!(order.contains(&slot), "{name}: {slot} in order");
            let w = frame(&format!("c.{slot}"));
            let child = frame(id);
            near(child.1 + child.3 / 2.0, centre, &format!("{slot} centred on the disc"));
            assert!(child.0 >= cf.0 - 0.01 && child.0 + child.2 <= cf.0 + cf.2 + 0.01 && child.1 >= cf.1 - 0.01 && child.1 + child.3 <= cf.1 + cf.3 + 0.01, "{name}: {slot} inside the composer");
            if slot == "leading" {
                near(edge(w, true), start, "leading starts at the content start");
            } else {
                let gap = if rtl { w.0 - (send.0 + send.2) } else { send.0 - (w.0 + w.2) };
                near(gap, xs, "trailing ends one gap before send");
            }
        }
        let mut fired = vec![];
        for step in c["steps"].as_array().unwrap() {
            let id = step["press"].as_str().unwrap();
            let i = s.index_of(id).unwrap_or_else(|| panic!("{name}: no {id}"));
            fired.extend(s.event(i, "press", None).into_iter().filter_map(|e| if let OutEvent::Action { name, context, .. } = e { Some(json!({"name": name, "context": context})) } else { None }));
        }
        assert_eq!(json!(fired), c["events"], "{name}: events");
    }
}

/// #EXP-1762 composerSlots `layout: "flat"` at the core: the field shares
/// the row with the slots, so the Composer's measure gets the width they
/// leave (each slot wrapper + one row gap off its width; the
/// measure keeps the disc's own share); a stacked one gets the full width.
#[test]
fn a_flat_composer_measures_its_field_at_the_width_the_slots_leave() {
    use eui_core::measure::DirectMeasure;
    let f = fixture();
    let k = &f["composerSlots"];
    let both = k["cases"][0]["slots"].clone();
    for layout in ["flat", "stacked"] {
        let mut composer = k["surface"].clone();
        composer["props"]["layout"] = json!(layout);
        composer["slots"] = both.clone();
        let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [composer]}));
        s.set_viewport(400.0, 0.0, None);
        let wraps = std::cell::RefCell::new(Vec::new());
        let mut m = DirectMeasure(
            |leaf: &eui_core::measure::LeafRequest, wrap: Option<f32>| match leaf.component {
                "Composer" => {
                    if let Some(w) = wrap.filter(|w| *w > 0.0) {
                        wraps.borrow_mut().push(w);
                    }
                    // Like a painter's: max-content wide, 120 at min-content.
                    (match wrap { None => 10_000.0, Some(w) if w <= 0.0 => 120.0, Some(w) => w }, 36.0)
                }
                "Ring" => (16.0, 16.0),
                _ => (36.0, 36.0),
            },
            7,
        );
        let out = s.layout(&mut m);
        let nodes = s.nodes();
        let frame = |id: &str| out.frames.iter().find(|f| nodes[f.index as usize].id == id).map(|f| (f.x, f.w)).unwrap_or_else(|| panic!("{layout}: no frame for {id}"));
        // A leaf's measure gets its border-box width (each painter takes its
        // own padding + border off), less the slots on a flat one.
        let (_, cw) = frame("c");
        let xs = eui_core::theme::resolve_token(s.theme().unwrap(), &json!("$spacing.xs"), s.mode()).and_then(|v| v.as_f64()).unwrap() as f32;
        let want = if layout == "flat" { cw - (frame("c.leading").1 + xs) - (frame("c.trailing").1 + xs) } else { cw };
        let last = *wraps.borrow().last().unwrap_or_else(|| panic!("{layout}: measured at a width"));
        assert!((last - want).abs() < 0.01, "{layout}: the field measures at {last}, want {want}");
        assert!((frame("c").1 - 400.0).abs() < 0.01, "{layout}: the composer keeps its full width");
    }
}

/// #EXP-1762 composerSlots `field` at the core: every built-in theme gives a
/// flat Composer's field a `paddingTop` (where its text starts below the
/// row's top), and the fixture's row grows past the send disc exactly when
/// paddingTop + its lines (+ paddingBottom) are taller (neutral: 8 + n × 20
/// against 36). The painters place the text (gpui, SwiftUI, Compose, web).
#[test]
fn a_flat_composer_field_starts_its_text_at_the_recipe_top_padding() {
    use eui_core::theme::{recipe_style, resolve_token, Mode, RecipeQuery};
    let f = fixture();
    let k = &f["composerSlots"];
    let flat = |part: &str| RecipeQuery { component: "Composer".into(), part: part.into(), props: serde_json::from_value(json!({"layout": "flat", "attachments": false, "busy": false})).unwrap(), states: vec![] };
    for id in ["neutral", "exponential", "playful", "orange"] {
        let theme = eui_core::themes::builtin_theme(id).unwrap();
        let px = |part: &str, key: &str| recipe_style(&theme, &flat(part)).get(key).and_then(|v| if v.is_number() { Some(v.clone()) } else { resolve_token(&theme, v, Mode::Light) }).and_then(|v| v.as_f64()).map(|v| v as f32);
        let pad = px("field", "paddingTop").unwrap_or_else(|| panic!("{id}: a flat field has a paddingTop"));
        assert!(pad > 0.0, "{id}: paddingTop {pad}");
        if id == "exponential" {
            assert!((pad - 4.25).abs() < 1e-4, "exponential: the IDE's 4.25, got {pad}");
        }
        if id != "neutral" {
            continue;
        }
        assert!((pad - 8.0).abs() < 1e-4, "neutral: $spacing.sm, got {pad}");
        let line = px("field", "lineHeight").unwrap();
        let disc = px("send", "height").unwrap();
        let mut seen = 0;
        for c in k["cases"].as_array().unwrap().iter().filter(|c| c.get("field").is_some()) {
            let rows = c["field"]["rows"].as_f64().unwrap() as f32;
            let lines = c["data"]["draft"].as_str().unwrap().split('\n').count() as f32;
            assert_eq!(rows, lines, "{}: rows = the draft's lines", c["name"]);
            let bottom = px("field", "paddingBottom").unwrap_or(0.0);
            assert_eq!(pad + rows * line + bottom > disc + 0.5, c["field"]["grown"] == true, "{}: grown", c["name"]);
            seen += 1;
        }
        assert_eq!(seen, 2, "a one-line and a grown case");
    }
}

/// #EDE-65 R composerPaste at the core: what a painter sends for a paste
/// (image files = `attach` {files: the images}; files without one =
/// nothing; else the field's own edit = `change`) fires like the reference. Which clipboard a paste carries and
/// the bytes' upload replay in each painter.
#[test]
fn composer_paste_cases_fire_attach_or_change_like_the_reference() {
    let f = fixture();
    let k = &f["composerPaste"];
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut s = surface(k["surface"].clone());
        for (path, value) in c["data"].as_object().unwrap() {
            s.set_data(&format!("/{path}"), Some(value.clone())).unwrap();
        }
        s.layout(&mut fixed());
        let composer = s.index_of("c").unwrap();
        let clip = &c["clipboard"];
        let images: Vec<Value> = clip["files"].as_array().into_iter().flatten().filter(|f| f["type"].as_str().is_some_and(|t| t.starts_with("image/"))).cloned().collect();
        let out = if !images.is_empty() {
            s.event(composer, "attach", Some(json!({"files": images})))
        } else if clip["files"].as_array().is_some_and(|f| !f.is_empty()) {
            vec![]
        } else if let Some(t) = clip["text"].as_str() {
            let draft = s.get_data("/draft").and_then(Value::as_str).unwrap_or("").to_string();
            s.event(composer, "change", Some(json!({"value": format!("{draft}{t}")})))
        } else {
            vec![]
        };
        let fired: Vec<Value> = out.into_iter().filter_map(|e| if let OutEvent::Action { name, context, .. } = e { Some(json!({"name": name, "context": context})) } else { None }).collect();
        assert_eq!(json!(fired), c["events"], "{name}: events");
        assert_eq!(s.get_data("/draft").cloned().unwrap_or(Value::Null), c["draft"], "{name}: /draft");
    }
}
