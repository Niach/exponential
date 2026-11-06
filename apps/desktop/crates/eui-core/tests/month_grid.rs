//! #VAPP-282: fixtures/month-grid.json through the core: a DatePicker
//! `inline: true` lays the month grid out IN FLOW (no trigger, no layer),
//! its `day` nodes (what SwiftUI, Compose and gpui paint) carry `outside`,
//! `today` + the `isToday` recipe prop, `selected`, `mark`; the steppers fire
//! `month {month}`.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/month-grid.json")).expect("month-grid.json")
}

fn inline_surface(props: &Value) -> Surface {
    let mut p = props.clone();
    p["name"] = json!("day");
    p["inline"] = json!(true);
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": 16},
        "children": [{"id": "d", "component": "DatePicker", "props": p, "on": {"month": {"event": {"name": "month"}}, "change": {"event": {"name": "picked"}}}}]
    }))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(390.0, 844.0, None);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn press(s: &mut Surface, id: &str) -> Vec<OutEvent> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    s.event(i, "press", None)
}

#[test]
fn month_grid_fixture_replays_through_the_inline_date_picker() {
    let f = fixture();
    for case in f["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let expect = &case["expect"];
        let mut s = inline_surface(&case["props"]);
        let mut m = fixed();
        let out = s.layout(&mut m);
        assert!(out.layers.is_empty(), "{name}: inline = no layer");
        assert!(s.index_of("d.trigger").is_none(), "{name}: inline = no trigger");
        let mut events = vec![];
        for step in case["steps"].as_array().cloned().unwrap_or_default() {
            for e in press(&mut s, &format!("d.{}", step.as_str().unwrap())) {
                if let OutEvent::Action { name, context, .. } = e {
                    events.push(json!({"name": name, "payload": {"month": context["month"]}}));
                }
            }
            s.layout(&mut m);
        }
        assert_eq!(Value::Array(events), expect.get("events").cloned().unwrap_or(json!([])), "{name}: month events");
        let nodes = s.nodes();
        let node = |id: &str| nodes.iter().find(|n| n.id == id && !n.removed).unwrap_or_else(|| panic!("{name}: no {id}"));
        assert_eq!(node("d.title").props["month"], json!(expect["month"].as_str().unwrap()[5..7].parse::<u32>().unwrap()), "{name}: month");
        assert_eq!(node("d.title").props["text"], expect["title"], "{name}: title");
        let mut cells = vec![];
        for r in 0..6 {
            for c in 0..7 {
                cells.push((format!("{r}.{c}"), node(&format!("d.day.{r}.{c}"))));
            }
        }
        assert_eq!(cells[0].1.props["date"], expect["first"], "{name}: first");
        assert_eq!(cells[41].1.props["date"], expect["last"], "{name}: last");
        let outside = |i: usize| cells[i].1.props["outside"] == json!(true);
        let leading = (0..42).take_while(|&i| outside(i)).count();
        let trailing = (0..42).rev().take_while(|&i| outside(i)).count();
        assert_eq!(json!(leading), expect["leading"], "{name}: leading");
        assert_eq!(json!(trailing), expect["trailing"], "{name}: trailing");
        let today: Vec<&String> = cells.iter().filter(|(_, n)| n.props["today"] == json!(true)).map(|(p, _)| p).collect();
        assert_eq!(today.first().map(|p| json!(p)).unwrap_or(Value::Null), expect["today"], "{name}: today");
        let selected: Vec<&String> = cells.iter().filter(|(_, n)| n.states.contains(&"selected".to_string()) && n.props["outside"] != json!(true)).map(|(p, _)| p).collect();
        assert_eq!(selected.first().map(|p| json!(p)).unwrap_or(Value::Null), expect["selected"], "{name}: selected");
        let marks: serde_json::Map<String, Value> = cells.iter().filter_map(|(p, n)| n.props.get("mark").map(|t| (p.clone(), t.clone()))).collect();
        assert_eq!(Value::Object(marks), expect["marks"], "{name}: marks");
    }
}

#[test]
fn inline_grid_paints_today_ringed_outside_dimmed_and_picks_without_closing() {
    let mut s = inline_surface(&json!({"value": "2026-10-14", "today": "2026-10-08", "firstDayOfWeek": 0}));
    let mut m = fixed();
    let out = s.layout(&mut m);
    let today = s.index_of("d.day.1.4").unwrap();
    let plain = s.index_of("d.day.1.5").unwrap();
    let outside = s.index_of("d.day.0.0").unwrap();
    let v = |i| s.visual(i).cloned().unwrap_or_default();
    assert_eq!(v(today).border_width, Some(1.0), "today: the hairline ring");
    assert!(v(today).border_color.is_some());
    assert_ne!(v(plain).border_width, Some(1.0), "an ordinary day has no ring");
    assert_eq!(v(outside).opacity, Some(0.5), "outside days dim ($opacity.tertiary)");
    let cal = s.index_of("d.calendar").unwrap();
    let frame = out.frames.iter().find(|f| f.index == cal).expect("the calendar is in flow");
    assert!(frame.w < 300.0, "start-aligned, sized to the grid (not the 358 column): {}", frame.w);
    assert_eq!(v(cal).border_width.unwrap_or(0.0), 0.0, "inline = no popover frame");
    let events = press(&mut s, "d.day.2.5");
    assert!(events.iter().any(|e| matches!(e, OutEvent::Action { name, context, .. } if name == "picked" && context["value"] == json!("2026-10-16"))));
    let out = s.layout(&mut m);
    assert!(out.layers.is_empty());
    assert!(s.nodes().iter().find(|n| n.id == "d.day.2.5").unwrap().states.contains(&"selected".to_string()), "still painted, now selected");
}
