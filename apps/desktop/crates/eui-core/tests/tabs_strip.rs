//! VAPP-279 (docs/round-7-contract.md): a `variant: strip` Tabs in the core.
//! `fixtures/tabs-strip.json` replays the pure rules (the TS twin is
//! packages/exponential-ui/src/tabs-strip.test.ts); its `layout` cases are
//! WRITTEN here (EXP_UI_WRITE_FIXTURES=1): the strip at a wide and a narrow
//! width with the fixed measure, the default theme — which tabs keep an item,
//! the `+N` text and every drawn item's frame. The `tabsReorder` cases of
//! `fixtures/interactions-round1.json` replay the drag and the keyboard move.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{LayoutOutput, OutEvent, Surface, SurfaceOptions};
use eui_core::tabs_strip::{partition_tabs, tab_drop_slot, tab_key_slot, tab_reorder};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/{name}")).expect("fixture")).expect("json")
}

fn f32s(v: &Value) -> Vec<f32> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()
}

fn usizes(v: &Value) -> Vec<usize> {
    v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect()
}

#[test]
fn the_pure_rules_replay_the_fixture() {
    let f = read("tabs-strip.json");
    for c in f["partition"].as_array().unwrap() {
        let active = c["active"].as_u64().map(|a| a as usize);
        let got = partition_tabs(&f32s(&c["widths"]), c["available"].as_f64().unwrap() as f32, c["gap"].as_f64().unwrap() as f32, c["overflowW"].as_f64().unwrap() as f32, active);
        assert_eq!(got, usizes(&c["visible"]), "{}", c["name"]);
    }
    for c in f["dropSlot"].as_array().unwrap() {
        let got = tab_drop_slot(&f32s(&c["centers"]), c["from"].as_u64().unwrap() as usize, c["delta"].as_f64().unwrap() as f32, c["rtl"].as_bool().unwrap());
        assert_eq!(got as u64, c["slot"].as_u64().unwrap(), "{}", c["name"]);
    }
    for c in f["keySlot"].as_array().unwrap() {
        let got = tab_key_slot(c["count"].as_u64().unwrap() as usize, c["from"].as_u64().unwrap() as usize, c["key"].as_str().unwrap(), c["rtl"].as_bool().unwrap());
        assert_eq!(got.map(|s| s as u64), c["slot"].as_u64(), "{}", c["name"]);
    }
    for c in f["reorder"].as_array().unwrap() {
        let got = tab_reorder(&usizes(&c["visible"]), c["fromSlot"].as_u64().unwrap() as usize, c["toSlot"].as_u64().unwrap() as usize);
        let want = c["expect"].as_object().map(|o| (o["from"].as_u64().unwrap() as usize, o["to"].as_u64().unwrap() as usize));
        assert_eq!(got, want, "{}", c["name"]);
    }
}

/// The strip every layout case lays out: six tabs (icons on two), the
/// fifth active, closable on hover, addable.
fn strip_tree(direction: &str) -> Value {
    let tabs: Vec<Value> = [("Inbox", Some("message-circle")), ("Release notes", Some("pencil")), ("Calendar", None), ("Settings", None), ("Search", None), ("Drafts", None)]
        .iter()
        .enumerate()
        .map(|(i, (label, icon))| {
            let mut t = json!({"label": label, "value": format!("t{i}")});
            if let Some(icon) = icon {
                t["icon"] = json!(icon);
            }
            t
        })
        .collect();
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": direction},
        "children": [{"id": "tabs", "component": "Tabs", "props": {"variant": "strip", "tabs": tabs, "value": "t4", "closable": "hover", "addable": true}, "on": {"reorder": {"event": {"name": "reorder"}}, "more": {"event": {"name": "more"}}}}]})
}

fn surface(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn round(v: f32) -> Value {
    json!((v * 1000.0).round() / 1000.0)
}

/// What a case records: the visible tab values, the `+N` text (null =
/// folded) and each drawn item's / the more button's frame.
fn observe(s: &mut Surface, out: &LayoutOutput) -> Value {
    let nodes = s.nodes();
    let style_of = |index: u32| out.frames.iter().find(|f| f.index == index);
    let mut visible = vec![];
    let mut frames = serde_json::Map::new();
    for n in &nodes {
        let drawn = |n: &eui_core::surface::PlacedNode| !s.visual(n.index).is_some_and(|v| v.visibility_hidden);
        if n.part.as_deref() == Some("item") && n.owner.as_deref() == Some("tabs") {
            if drawn(n) {
                visible.push(n.props.get("value").cloned().unwrap_or(Value::Null));
                if let Some(f) = style_of(n.index) {
                    frames.insert(n.id.clone(), json!([round(f.x), round(f.y), round(f.w), round(f.h)]));
                }
            }
        }
        if n.id == "tabs.more" && drawn(n) {
            if let Some(f) = style_of(n.index) {
                frames.insert(n.id.clone(), json!([round(f.x), round(f.y), round(f.w), round(f.h)]));
            }
        }
    }
    let more = nodes.iter().find(|n| n.id == "tabs.more").filter(|n| !s.visual(n.index).is_some_and(|v| v.visibility_hidden)).and_then(|n| n.props.get("label").cloned()).unwrap_or(Value::Null);
    json!({"visible": visible, "more": more, "frames": frames})
}

fn lay_out(width: f32, direction: &str) -> (Surface, LayoutOutput) {
    let mut s = surface(strip_tree(direction));
    s.set_viewport(width, 0.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    (s, out)
}

const CASES: &[(&str, f32, &str)] = &[("900", 900.0, "ltr"), ("320", 320.0, "ltr"), ("320-rtl", 320.0, "rtl"), ("160", 160.0, "ltr")];

#[test]
fn the_strip_folds_what_does_not_fit_into_more() {
    let path = format!("{FIXTURES}/tabs-strip.json");
    let mut f = read("tabs-strip.json");
    let write = std::env::var("EXP_UI_WRITE_FIXTURES").is_ok();
    let mut written = serde_json::Map::new();
    for (name, width, direction) in CASES {
        let (mut s, out) = lay_out(*width, direction);
        let got = observe(&mut s, &out);
        if write {
            written.insert(name.to_string(), json!({"width": width, "direction": direction, "expect": got}));
        } else {
            assert_eq!(got, f["layout"]["cases"][name]["expect"], "case {name}");
        }
    }
    if write {
        f["layout"] = json!({"$comment": "Written by apps/desktop/crates/eui-core/tests/tabs_strip.rs (EXP_UI_WRITE_FIXTURES=1): six strip tabs (`Inbox` + `Release notes` with icons, the fifth `Search` active, closable on hover, addable) in a full-width root at each case's width and direction, the default theme (exponential, light), the fixed measure (8 px per character on one 20 px line). `visible` = the tab values that keep an item, `more` = the `+N` text (null = folded), `frames` = each drawn item's and the `+N` button's [x, y, w, h] relative to the surface.", "cases": written});
        std::fs::write(&path, serde_json::to_string_pretty(&f).unwrap() + "\n").unwrap();
    }
    // Wide: every tab, no `+N`; narrow: the active one survives.
    let wide = &f["layout"]["cases"]["900"]["expect"];
    assert_eq!(wide["visible"].as_array().unwrap().len(), 6);
    assert_eq!(wide["more"], Value::Null);
    let narrow = &f["layout"]["cases"]["320"]["expect"];
    assert!(narrow["visible"].as_array().unwrap().contains(&json!("t4")), "the active tab stays");
    assert!(narrow["more"].as_str().is_some_and(|t| t.starts_with('+')));
}

fn action<'a>(events: &'a [OutEvent], name: &str) -> Option<&'a Value> {
    events.iter().find_map(|e| match e {
        OutEvent::Action { name: n, context, .. } if n == name => Some(context),
        _ => None,
    })
}

#[test]
fn more_fires_the_folded_values() {
    let (mut s, _) = lay_out(320.0, "ltr");
    let i = s.index_of("tabs.more").unwrap();
    let out = s.event(i, "press", None);
    let ctx = action(&out, "more").expect("more fired");
    let folded = ctx["values"].as_array().unwrap();
    assert!(!folded.is_empty() && !folded.contains(&json!("t4")));
}

/// `fixtures/interactions-round1.json` `tabsReorder`.
#[test]
fn reorder_cases_replay_like_the_reference() {
    let f = read("interactions-round1.json");
    for c in f["tabsReorder"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let direction = c["direction"].as_str().unwrap_or("ltr");
        let (mut s, _) = lay_out(900.0, direction);
        let tab = s.index_of(&format!("tabs.tab.{}", c["tab"].as_u64().unwrap())).unwrap();
        let mut fired = vec![];
        for step in c["steps"].as_array().unwrap() {
            let out = match step["do"].as_str().unwrap() {
                "drag" => s.event(tab, "drag", Some(json!({"phase": step["phase"], "delta": step["delta"]}))),
                "key" => s.event(tab, "key", Some(json!({"key": step["key"]}))),
                other => panic!("{name}: unknown step {other}"),
            };
            let mut m = fixed();
            let laid = s.layout(&mut m);
            if let Some(order) = step.get("order") {
                // The live preview: the item values in strip (x) order.
                let nodes = s.nodes();
                let mut items: Vec<(f32, Value)> = nodes
                    .iter()
                    .filter(|n| n.part.as_deref() == Some("item") && n.owner.as_deref() == Some("tabs"))
                    .filter_map(|n| laid.frames.iter().find(|f| f.index == n.index).map(|f| (f.x, n.props["value"].clone())))
                    .collect();
                items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let mut got: Vec<Value> = items.into_iter().map(|(_, v)| v).collect();
                if direction == "rtl" {
                    got.reverse();
                }
                assert_eq!(Value::Array(got), *order, "{name}: preview after {step}");
            }
            if let Some(ctx) = action(&out, "reorder") {
                fired.push(json!({"from": ctx["from"], "to": ctx["to"], "value": ctx["value"]}));
            }
        }
        assert_eq!(Value::Array(fired), c["expect"], "{name}");
    }
}
