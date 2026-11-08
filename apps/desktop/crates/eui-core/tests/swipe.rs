//! FEED-200: `fixtures/swipe.json` replayed against the core's pure swipe
//! rules (the TS twin is packages/exponential-ui/src/swipe.test.ts).

use eui_core::swipe::{swipe_blocks, swipe_indicator, swipe_lock, swipe_offset, swipe_progress, swipe_settle, swipe_step, SWIPE_DISTANCE, SWIPE_RESISTANCE, SWIPE_SLOP, SWIPE_VELOCITY};
use serde_json::Value;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/swipe.json");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE).expect("fixture")).expect("json")
}

fn cases<'a>(f: &'a Value, group: &str) -> &'a Vec<Value> {
    f[group].as_array().expect(group)
}

fn n(v: &Value) -> f64 {
    v.as_f64().unwrap() 
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn constants() {
    let c = &fixture()["constants"];
    assert_eq!((n(&c["slop"]), n(&c["distance"]), n(&c["velocity"]), n(&c["resistance"])), (SWIPE_SLOP, SWIPE_DISTANCE, SWIPE_VELOCITY, SWIPE_RESISTANCE));
}

#[test]
fn every_group_replays() {
    let f = fixture();
    for c in cases(&f, "lock") {
        assert_eq!(swipe_lock(n(&c["dx"]), n(&c["dy"])), c["lock"].as_str().unwrap(), "{}", c["name"]);
    }
    for c in cases(&f, "step") {
        assert_eq!(swipe_step(n(&c["offset"]), c["rtl"].as_bool().unwrap()), c["step"].as_i64().unwrap(), "{}", c["name"]);
    }
    for c in cases(&f, "offset") {
        let got = swipe_offset(c["index"].as_u64().unwrap() as usize, c["count"].as_u64().unwrap() as usize, n(&c["offset"]), n(&c["width"]), c["rtl"].as_bool().unwrap());
        assert!(near(got, n(&c["drawn"])), "{}: {got}", c["name"]);
    }
    for c in cases(&f, "settle") {
        let got = swipe_settle(c["index"].as_u64().unwrap() as usize, c["count"].as_u64().unwrap() as usize, n(&c["offset"]), n(&c["velocity"]), n(&c["width"]), c["rtl"].as_bool().unwrap());
        assert_eq!(got as u64, c["settle"].as_u64().unwrap(), "{}", c["name"]);
    }
    for c in cases(&f, "progress") {
        assert!(near(swipe_progress(n(&c["offset"]), n(&c["width"])), n(&c["progress"])), "{}", c["name"]);
    }
    for c in cases(&f, "indicator") {
        let span = |v: &Value| (n(&v["x"]), n(&v["w"]));
        let (x, w) = swipe_indicator(span(&c["from"]), span(&c["to"]), n(&c["progress"]));
        let (ex, ew) = span(&c["indicator"]);
        assert!(near(x, ex) && near(w, ew), "{}", c["name"]);
    }
    for c in cases(&f, "blocks") {
        assert_eq!(swipe_blocks(c["component"].as_str().unwrap(), c["direction"].as_str(), c["overflowX"].as_str()), c["blocks"].as_bool().unwrap(), "{}", c["name"]);
    }
}

mod surface {
    use std::collections::HashMap;

    use eui_core::measure::FixedMeasure;
    use eui_core::style::TransformOp;
    use eui_core::surface::{LayoutOutput, OutEvent, Surface, SurfaceOptions};
    use eui_core::types::NestedNode;
    use serde_json::{json, Value};

    fn tree(variant: &str, direction: &str) -> Value {
        let tabs: Vec<Value> = ["Dashboard", "Activity", "Income", "Expenses"].iter().enumerate().map(|(i, l)| json!({"label": l, "value": format!("t{i}")})).collect();
        let panels: Vec<Value> = (0..4).map(|i| json!({"id": format!("p{i}"), "component": "Text", "props": {"text": format!("Panel {i}")}})).collect();
        json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": direction},
            "children": [{"id": "tabs", "component": "Tabs", "props": {"variant": variant, "fill": true, "swipeable": true, "tabs": tabs, "value": "t1"}, "on": {"change": {"event": {"name": "changed"}}}, "children": panels}]})
    }

    fn lay_out(variant: &str, direction: &str) -> (Surface, LayoutOutput) {
        let tree: NestedNode = serde_json::from_value(tree(variant, direction)).expect("tree");
        let mut s = Surface::new("t", SurfaceOptions::default());
        assert!(s.set_nested(tree).issues.is_empty());
        s.set_viewport(400.0, 0.0, None);
        let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
        (s, out)
    }

    fn relayout(s: &mut Surface) -> LayoutOutput {
        s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true })
    }

    fn index(s: &mut Surface, id: &str) -> u32 {
        s.nodes().iter().find(|n| n.id == id).unwrap_or_else(|| panic!("{id}")).index
    }

    fn shown(s: &mut Surface, out: &LayoutOutput, id: &str) -> bool {
        s.nodes().iter().find(|n| n.id == id).is_some_and(|n| out.frames.iter().any(|f| f.index == n.index && f.w > 0.0))
    }

    fn dx(s: &mut Surface, id: &str) -> f32 {
        let i = index(s, id);
        s.visual(i).and_then(|v| v.transform.clone()).unwrap_or_default().iter().find_map(|op| if let TransformOp::Translate { x, .. } = op { Some(*x) } else { None }).unwrap_or(0.0)
    }

    fn animated(s: &mut Surface, id: &str) -> bool {
        let i = index(s, id);
        s.visual(i).and_then(|v| v.transition.clone()).is_some_and(|t| t.duration_ms > 0.0)
    }

    fn swipe(s: &mut Surface, phase: &str, offset: f32, velocity: f32) -> Vec<OutEvent> {
        let i = index(s, "tabs");
        s.event(i, "swipe", Some(json!({"phase": phase, "offset": offset, "velocity": velocity})))
    }

    fn tab_x(s: &mut Surface, out: &LayoutOutput, id: &str) -> f32 {
        let (l, t) = (index(s, "tabs.list"), index(s, id));
        let list = out.frames.iter().find(|f| f.index == l).unwrap();
        out.frames.iter().find(|f| f.index == t).unwrap().x - list.x
    }

    #[test]
    fn the_neighbours_wait_one_width_aside_and_the_thumb_sits_on_the_active_tab() {
        let (mut s, out) = lay_out("pill", "ltr");
        assert!(shown(&mut s, &out, "p0") && shown(&mut s, &out, "p1") && shown(&mut s, &out, "p2") && !shown(&mut s, &out, "p3"));
        let width = out.frames.iter().find(|f| f.index == index(&mut s, "tabs.content")).unwrap().w;
        assert!(width > 300.0, "{width}");
        assert_eq!((dx(&mut s, "p0"), dx(&mut s, "p1"), dx(&mut s, "p2")), (-width, 0.0, width));
        assert!((dx(&mut s, "tabs.thumb") - tab_x(&mut s, &out, "tabs.tab.1")).abs() < 0.01);
    }

    #[test]
    fn a_swipe_follows_the_finger_then_settles_on_the_next_tab() {
        let (mut s, out) = lay_out("pill", "ltr");
        let width = out.frames.iter().find(|f| f.index == index(&mut s, "tabs.content")).unwrap().w;
        let (t1, t2) = (tab_x(&mut s, &out, "tabs.tab.1"), tab_x(&mut s, &out, "tabs.tab.2"));
        assert_eq!(swipe(&mut s, "move", -width / 4.0, 0.0), vec![OutEvent::Relayout]);
        relayout(&mut s);
        assert_eq!(dx(&mut s, "p1"), -width / 4.0);
        assert_eq!(dx(&mut s, "p2"), width * 0.75);
        assert!(!animated(&mut s, "p1"), "no transition under the finger");
        assert!((dx(&mut s, "tabs.thumb") - (t1 + (t2 - t1) * 0.25)).abs() < 0.01);
        let out_events = swipe(&mut s, "end", -width / 4.0, -0.6);
        assert!(out_events.iter().any(|e| matches!(e, OutEvent::Action { event, context, .. } if event == "change" && context["value"] == "t2")), "{out_events:?}");
        let out = relayout(&mut s);
        assert_eq!((dx(&mut s, "p1"), dx(&mut s, "p2"), dx(&mut s, "p3")), (-width, 0.0, width));
        assert!(!shown(&mut s, &out, "p0"));
        assert!(animated(&mut s, "p2"), "the settle animates");
        assert!((dx(&mut s, "tabs.thumb") - t2).abs() < 0.01);
    }

    #[test]
    fn a_short_slow_swipe_springs_back_and_the_first_page_resists() {
        let (mut s, out) = lay_out("strip", "ltr");
        let width = out.frames.iter().find(|f| f.index == index(&mut s, "tabs.content")).unwrap().w;
        let events = swipe(&mut s, "end", -width / 4.0, -0.1);
        assert!(!events.iter().any(|e| matches!(e, OutEvent::Action { .. })));
        relayout(&mut s);
        assert_eq!(dx(&mut s, "p1"), 0.0);
        // Page 0, dragged right: no neighbour there, nothing pages.
        let i = index(&mut s, "tabs.tab.0");
        s.event(i, "press", None);
        relayout(&mut s);
        let events = swipe(&mut s, "end", width, 1.0);
        assert!(!events.iter().any(|e| matches!(e, OutEvent::Action { .. })));
        relayout(&mut s);
        assert_eq!((dx(&mut s, "p0"), dx(&mut s, "p1")), (0.0, width));
        swipe(&mut s, "move", 100.0, 0.0);
        relayout(&mut s);
        assert!((dx(&mut s, "p0") - 30.0).abs() < 0.01);
    }

    #[test]
    fn rtl_reveals_the_next_tab_to_the_left() {
        let (mut s, out) = lay_out("pill", "rtl");
        let width = out.frames.iter().find(|f| f.index == index(&mut s, "tabs.content")).unwrap().w;
        assert_eq!((dx(&mut s, "p0"), dx(&mut s, "p2")), (width, -width));
        let events = swipe(&mut s, "end", width * 0.6, 0.0);
        assert!(events.iter().any(|e| matches!(e, OutEvent::Action { context, .. } if context["value"] == "t2")));
    }
}
