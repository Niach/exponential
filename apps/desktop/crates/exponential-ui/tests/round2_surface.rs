//! Round 2 (docs/round-2-contract.md) in the layout core: lifted templates
//! and accumulated instance ids, layout props through `resolve_node_props`,
//! display strings, the surface formatter, Resizable (layout, drag from the
//! start sizes, keys), horizontal / sectioned / unbounded windowing, sticky
//! headers and `position: sticky`, `scrollToIndex`, and the new Visual
//! fields (direction, physical align, backdrop blur, animation).

use std::collections::HashMap;
use std::sync::Arc;

use exponential_ui::format::{DateOptions, DateValue, Formatter, NumberOptions, PluralCategory, RelativeUnit};
use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{LayoutOutput, OutEvent, Surface, SurfaceCommand, SurfaceOptions};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

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

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> exponential_ui::surface::PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

fn prop(s: &mut Surface, id: &str, key: &str) -> Value {
    s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}")).props.get(key).cloned().unwrap_or(Value::Null)
}

fn rows(n: usize, extent: f32) -> Vec<Value> {
    (0..n).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": extent, "width": extent, "flexShrink": 0}})).collect()
}

#[test]
fn nested_templates_accumulate_instance_suffixes_and_render_only_per_item() {
    let mut s = surface(json!({
        "id": "root", "component": "Box",
        "children": [
            {"id": "boards", "component": "List", "template": {"component": "board", "path": "/boards", "key": "id"}},
            {"id": "board", "component": "Box", "children": [{"id": "issues", "component": "List", "template": {"component": "issue", "path": "issues", "key": "id"}}]},
            {"id": "issue", "component": "Text", "props": {"text": {"path": "t"}}}
        ]
    }));
    s.set_data("", Some(json!({"boards": [{"id": "ops", "issues": [{"id": "1", "t": "Ship"}, {"id": "2", "t": "Test"}]}, {"id": "web", "issues": [{"id": "1", "t": "Fix"}]}]})));
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut fixed());
    let ids: Vec<String> = s.nodes().into_iter().map(|n| n.id).collect();
    for id in ["board.ops", "issues.ops", "issue.ops.1", "issue.ops.2", "board.web", "issue.web.1"] {
        assert!(ids.contains(&id.to_string()), "{id} in {ids:?}");
    }
    assert!(!ids.contains(&"board".to_string()) && !ids.contains(&"issue".to_string()), "templates never render in place");
    assert_eq!(prop(&mut s, "issue.web.1", "text"), json!("Fix"));
}

#[test]
fn layout_props_resolve_along_the_schema_and_numbers_show_as_display_strings() {
    let mut s = surface(json!({
        "id": "root", "component": "Box",
        "children": [
            {"id": "count", "component": "Text", "props": {"text": {"path": "/n"}}},
            {"id": "flag", "component": "Text", "props": {"text": {"path": "/ok"}}},
            {"id": "t", "component": "Table", "props": {"columns": [{"key": "name", "label": "Name"}], "rows": [{"id": "a", "name": {"path": "/n"}}]}}
        ]
    }));
    s.set_data("", Some(json!({"n": 412, "ok": true})));
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut fixed());
    assert_eq!(prop(&mut s, "count", "text"), json!("412"));
    assert_eq!(prop(&mut s, "flag", "text"), json!("true"));
    // A literal Table row holding `{path}` is DATA (bind-time.json literal-rows).
    assert_eq!(prop(&mut s, "t", "rows"), json!([{"id": "a", "name": {"path": "/n"}}]));
}

struct German;
impl Formatter for German {
    fn locale(&self) -> String {
        "de-DE".into()
    }
    fn number(&self, v: f64, o: NumberOptions) -> String {
        format!("{:.*}", o.decimals.unwrap_or(0) as usize, v).replace('.', ",")
    }
    fn currency(&self, v: f64, code: &str, _: NumberOptions) -> String {
        format!("{v} {code}")
    }
    fn percent(&self, v: f64, _: Option<u32>) -> String {
        format!("{} %", v * 100.0)
    }
    fn date(&self, _: DateValue, o: &DateOptions) -> String {
        format!("D[{}]", o.format.clone().unwrap_or_default())
    }
    fn relative_time(&self, v: i64, unit: RelativeUnit) -> String {
        format!("{v} {}", unit.as_str())
    }
    fn plural(&self, _: f64) -> PluralCategory {
        PluralCategory::Other
    }
}

#[test]
fn a_host_formatter_formats_cells_numbers_and_the_bind_functions() {
    let mut s = surface(json!({
        "id": "root", "component": "Box",
        "children": [
            {"id": "f", "component": "Text", "props": {"text": {"call": "formatNumber", "args": {"value": 2.5, "decimals": 1}}}},
            {"id": "rel", "component": "Text", "props": {"text": {"call": "formatRelativeTime", "args": {"value": 0}}}},
            {"id": "t", "component": "Table", "props": {"columns": [{"key": "p", "label": "P", "type": "percent"}, {"key": "c", "label": "C", "type": "currency", "currency": "EUR"}], "rows": [{"id": "a", "p": 0.5, "c": 3}]}}
        ]
    }));
    s.set_formatter(Arc::new(German));
    s.set_clock(Some(120_000.0));
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut fixed());
    assert_eq!(prop(&mut s, "f", "text"), json!("2,5"));
    assert_eq!(prop(&mut s, "rel", "text"), json!("-2 minute"));
    assert_eq!(prop(&mut s, "t.cell.a.0", "text"), json!("50 %"));
    assert_eq!(prop(&mut s, "t.cell.a.1", "text"), json!("3 EUR"));
}

#[test]
fn resizable_panels_share_the_axis_minus_the_handles_and_follow_drags_and_keys() {
    let mut s = surface(json!({
        "id": "root", "component": "Box", "style": {"width": 401},
        "children": [{"id": "split", "component": "Resizable", "props": {"sizes": {"path": "/ui/split"}, "panels": [{"min": 20}, {"min": 30}], "handle": true}, "style": {"height": 200},
            "children": [{"id": "a", "component": "Box"}, {"id": "b", "component": "Box"}]}]
    }));
    s.set_data("", Some(json!({"ui": {"split": [30, 70]}})));
    s.set_viewport(401.0, 0.0, None);
    let out = s.layout(&mut fixed());
    let (a, b, h) = (frame(&s, &out, "split.panel.0"), frame(&s, &out, "split.panel.1"), frame(&s, &out, "split.handle.0"));
    let near = |x: f32, y: f32| (x - y).abs() < 0.01;
    assert!(near(a.w, 120.0) && near(h.w, 1.0) && near(b.w, 280.0), "panelExtents([30, 70], 401, 1): {} {} {}", a.w, h.w, b.w);
    assert!(s.index_of("split.grip.0").is_some());
    let handle = s.index_of("split.handle.0").unwrap();
    // A drag: deltas from the START sizes, no write until the end.
    s.event(handle, "drag", Some(json!({"phase": "start"})));
    let ev = s.event(handle, "drag", Some(json!({"phase": "move", "delta": 40})));
    assert!(ev.iter().any(|e| matches!(e, OutEvent::Relayout)));
    let out = s.layout(&mut fixed());
    assert!((frame(&s, &out, "split.panel.0").w - 160.0).abs() < 0.01);
    let ev = s.event(handle, "drag", Some(json!({"phase": "end", "delta": 40})));
    assert!(ev.iter().any(|e| matches!(e, OutEvent::DataChanged { path, value } if path == "/ui/split" && value == &json!([40.0, 60.0]))), "{ev:?}");
    s.layout(&mut fixed());
    // Keys: ArrowLeft moves the handle 10 points left; Home = the first panel's min.
    let handle = s.index_of("split.handle.0").unwrap();
    s.event(handle, "key", Some(json!({"key": "ArrowLeft"})));
    assert_eq!(s.data()["ui"]["split"], json!([30.0, 70.0]));
    s.layout(&mut fixed());
    let handle = s.index_of("split.handle.0").unwrap();
    s.event(handle, "key", Some(json!({"key": "Home"})));
    assert_eq!(s.data()["ui"]["split"], json!([20.0, 80.0]));
    let nodes = s.nodes();
    let h = nodes.iter().find(|n| n.id == "split.handle.0").unwrap();
    assert_eq!(h.accessibility.as_ref().unwrap()["role"], json!("separator"));
    assert_eq!(h.accessibility.as_ref().unwrap()["label"], json!("Resize"));
}

#[test]
fn resizable_commits_nothing_when_the_sizes_do_not_change() {
    let mut s = surface(json!({
        "id": "root", "component": "Box", "style": {"width": 401},
        "children": [{"id": "split", "component": "Resizable", "props": {"sizes": {"path": "/ui/split"}, "panels": [{"min": 30}, {}], "handle": true}, "on": {"change": {"event": {"name": "resized"}}}, "style": {"height": 200},
            "children": [{"id": "a", "component": "Box"}, {"id": "b", "component": "Box"}]}]
    }));
    s.set_data("", Some(json!({"ui": {"split": [30, 70]}})));
    s.set_viewport(401.0, 0.0, None);
    s.layout(&mut fixed());
    let handle = s.index_of("split.handle.0").unwrap();
    // Keys outside RESIZE_KEYS (Tab, Shift) and an arrow at the limit: nothing.
    for key in ["Tab", "Shift", "a", "ArrowLeft", "Home"] {
        assert!(s.event(handle, "key", Some(json!({"key": key}))).is_empty(), "{key}");
    }
    // A press with no movement: nothing written, nothing fired.
    assert!(s.event(handle, "drag", Some(json!({"phase": "start"}))).is_empty());
    assert!(s.event(handle, "drag", Some(json!({"phase": "end", "delta": 0}))).is_empty());
    // A drag that comes back to where it started: the preview resets, no commit.
    s.event(handle, "drag", Some(json!({"phase": "start"})));
    s.event(handle, "drag", Some(json!({"phase": "move", "delta": 40})));
    s.layout(&mut fixed());
    let handle = s.index_of("split.handle.0").unwrap();
    let ev = s.event(handle, "drag", Some(json!({"phase": "end", "delta": 0})));
    assert!(ev.iter().all(|e| matches!(e, OutEvent::Relayout)), "{ev:?}");
    let out = s.layout(&mut fixed());
    assert!((frame(&s, &out, "split.panel.0").w - 120.0).abs() < 0.01);
    assert_eq!(s.data()["ui"]["split"], json!([30, 70]));
}

#[test]
fn a_horizontal_list_windows_on_x() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"width": 390},
        "children": [{"id": "l", "component": "List", "props": {"direction": "horizontal"}, "style": {"width": 390, "height": 80}, "children": rows(500, 160.0)}]}));
    s.set_viewport(390.0, 800.0, None);
    s.layout(&mut fixed());
    let out = s.layout(&mut fixed());
    let l = &out.lists[0];
    assert!(l.windowed && l.horizontal);
    assert!(l.end - l.start <= 3 + 2 * 5, "{}..{}", l.start, l.end);
    s.scroll_to("l", 8400.0, 0.0);
    let out = s.layout(&mut fixed());
    let l = &out.lists[0];
    assert!(l.start >= 40 && l.end <= 62, "x window {}..{}", l.start, l.end);
}

#[test]
fn an_unbounded_list_windows_against_its_scrolling_ancestor() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"height": 400, "overflowY": "auto"},
        "children": [{"id": "head", "component": "Box", "style": {"height": 100, "flexShrink": 0}}, {"id": "l", "component": "List", "style": {"flexShrink": 0}, "children": rows(1000, 40.0)}]}));
    s.set_viewport(390.0, 400.0, None);
    s.layout(&mut fixed());
    s.layout(&mut fixed());
    s.scroll_to("root", 0.0, 20_100.0);
    let out = s.layout(&mut fixed());
    let l = &out.lists[0];
    // The ancestor shows list content 20000..20400 = items 500..510.
    assert!(l.start <= 500 && l.end >= 510 && l.end - l.start <= 10 + 2 * 5 + 1, "{}..{}", l.start, l.end);
}

#[test]
fn an_unbounded_list_windows_against_the_host_viewport() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "l", "component": "List", "children": rows(1000, 40.0)}]}));
    s.set_viewport(390.0, 0.0, Some(800.0));
    s.layout(&mut fixed());
    s.layout(&mut fixed());
    assert!(s.set_surface_scroll(0.0, 20_000.0));
    let out = s.layout(&mut fixed());
    let l = &out.lists[0];
    assert!(l.start <= 500 && l.end >= 520, "{}..{}", l.start, l.end);
}

#[test]
fn sections_get_headers_and_the_sticky_one_pins() {
    let items: Vec<Value> = (0..120).map(|i| json!({"id": format!("i{i}"), "day": format!("d{}", i / 10)})).collect();
    let mut s = surface(json!({"id": "root", "component": "Box",
        "children": [
            {"id": "l", "component": "List", "props": {"sectionBy": "day", "stickyHeaders": true}, "style": {"height": 400}, "template": {"component": "row", "path": "/items", "key": "id"},
             "slots": {"section": {"id": "hdr", "component": "Text", "props": {"text": {"call": "concat", "args": {"values": [{"path": "value"}, " (", {"path": "count"}, ")"]}}}}}},
            {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}}
        ]}));
    s.set_data("", Some(json!({"items": items})));
    s.set_viewport(390.0, 800.0, None);
    s.layout(&mut fixed());
    let out = s.layout(&mut fixed());
    assert_eq!(out.lists[0].count, 132, "120 items + 12 headers");
    assert_eq!(prop(&mut s, "hdr.0", "text"), json!("d0 (10)"));
    let nodes = s.nodes();
    let header = nodes.iter().find(|n| n.id == "l.section.0").unwrap();
    assert_eq!(header.accessibility.as_ref().unwrap()["level"], json!(3));
    // Scrolled into section 3 (rows 33..): its header pins at the top.
    s.scroll_to("l", 0.0, 1400.0);
    let out = s.layout(&mut fixed());
    let pinned = s.index_of("l.section.3").expect("the pinned header is rendered");
    assert!(out.sticky.iter().any(|st| st.index == pinned), "{:?}", out.sticky);
    // scrollToIndex: data item 60 (section 6) at the start, under the header.
    let ev = s.command(&SurfaceCommand::ScrollToIndex { id: "l".into(), index: 60, align: Some("start".into()) });
    assert!(ev.iter().any(|e| matches!(e, OutEvent::Relayout)));
    s.layout(&mut fixed());
    // Item 60 sits at row 66 (6 headers before it... plus its own) → 66 × 40 − the header extent.
    let y = s.scroll_offset("l").1;
    assert!((y - (66.0 * 40.0 - 40.0)).abs() < 41.0, "{y}");
    assert!(s.index_of("row.i60").is_some(), "rendered after the jump");
}

#[test]
fn scroll_to_index_on_a_windowed_list_renders_the_item() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "l", "component": "List", "style": {"height": 400}, "children": rows(1000, 40.0)}]}));
    s.set_viewport(390.0, 800.0, None);
    s.layout(&mut fixed());
    s.layout(&mut fixed());
    s.command(&SurfaceCommand::ScrollToIndex { id: "l".into(), index: 500, align: Some("start".into()) });
    s.layout(&mut fixed());
    assert_eq!(s.scroll_offset("l").1, 20_000.0);
    assert!(s.index_of("r500").is_some());
    let r = s.nodes().into_iter().find(|n| n.id == "r500").unwrap();
    assert_eq!(r.accessibility.unwrap(), json!({"posInSet": 501, "setSize": 1000}), "its place in the whole list");
}

#[test]
fn position_sticky_pins_inside_the_nearest_scroller_while_its_parent_is_in_view() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "height": 300, "overflowY": "auto"},
        "children": [{"id": "sec", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": [
            {"id": "bar", "component": "Box", "style": {"position": "sticky", "top": 0, "height": 20, "backdropBlur": "$blur.md", "backgroundColor": "#ffffff80"}},
            {"id": "body", "component": "Box", "style": {"height": 400, "flexShrink": 0}}]},
            {"id": "tail", "component": "Box", "style": {"height": 1000, "flexShrink": 0}}]}));
    s.set_viewport(390.0, 300.0, None);
    s.layout(&mut fixed());
    s.scroll_to("root", 0.0, 200.0);
    let out = s.layout(&mut fixed());
    let bar = s.index_of("bar").unwrap();
    let st = out.sticky.iter().find(|x| x.index == bar).expect("pinned");
    assert_eq!(st.dy, 200.0);
    let v = s.visual(bar).unwrap();
    assert!(v.sticky);
    assert_eq!(v.backdrop_blur, Some(12.0));
    // Past its parent's end it leaves with the parent.
    s.scroll_to("root", 0.0, 1000.0);
    let out = s.layout(&mut fixed());
    let st = out.sticky.iter().find(|x| x.index == bar).expect("pinned");
    assert_eq!(st.dy, 400.0, "clamped to the parent's bottom (420 − 20)");
}

#[test]
fn visuals_carry_direction_physical_align_and_animation_timing() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "rtl", "component": "Box", "style": {"direction": "rtl"}, "children": [{"id": "t", "component": "Text", "props": {"text": "x"}}, {"id": "e", "component": "Text", "props": {"text": "x", "align": "end"}}]},
        {"id": "spin", "component": "Box", "style": {"animation": "spin", "width": 10, "height": 10}},
        {"id": "fade", "component": "Box", "style": {"animation": "fade-in", "animationDuration": "$motion.fast", "width": 10, "height": 10}}
    ]}));
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut fixed());
    let v = |s: &Surface, id: &str| s.visual(s.index_of(id).unwrap()).cloned().unwrap();
    assert_eq!(v(&s, "t").direction.as_deref(), Some("rtl"));
    assert_eq!(v(&s, "t").text_align.as_deref(), Some("right"));
    assert_eq!(v(&s, "e").text_align.as_deref(), Some("left"));
    let spin = v(&s, "spin").animation.unwrap();
    assert_eq!(spin.timing.duration_ms, 1120.0);
    let fade = v(&s, "fade").animation.unwrap();
    assert_eq!(fade.timing.duration_ms, 120.0);
    let frame = exponential_ui::animation::frame_with_timing("fade-in", &fade.timing, 60.0, false).unwrap();
    assert!((frame.opacity - 0.8392).abs() < 1e-3);
}

#[test]
fn chart_tick_and_value_labels_use_the_formatter_default_digits() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "c", "component": "Chart", "props": {"kind": "bar", "series": [{"name": "a", "values": [0.1, 0.35, 0.9]}, {"name": "b", "values": [1234.5, 2, 0.5]}], "min": 0, "max": 1}}
    ]}));
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut fixed());
    let chart = prop(&mut s, "c", "chart");
    // Like the reference's `formatter.number(v)`: no padding to the step's decimals.
    assert_eq!(chart["tickLabels"][0], json!("0"));
    assert!(chart["tickLabels"].as_array().unwrap().iter().all(|t| !t.as_str().unwrap().ends_with(".0")), "{}", chart["tickLabels"]);
    assert_eq!(chart["valueLabels"], json!([["0.1", "0.35", "0.9"], ["1,234.5", "2", "0.5"]]));
}

#[test]
fn a_divided_sectioned_list_spaces_every_boundary_like_its_offsets() {
    let items: Vec<Value> = (0..9).map(|i| json!({"id": format!("i{i}"), "day": format!("d{}", i / 3)})).collect();
    let mut s = surface(json!({"id": "root", "component": "Box",
        "children": [
            {"id": "l", "component": "List", "props": {"sectionBy": "day", "divided": true, "gap": "md"}, "template": {"component": "row", "path": "/items", "key": "id"},
             "slots": {"section": {"id": "hdr", "component": "Box", "style": {"height": 24}}}},
            {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}}
        ]}));
    s.set_data("", Some(json!({"items": items})));
    s.set_viewport(390.0, 0.0, Some(800.0));
    s.layout(&mut fixed());
    let out = s.layout(&mut fixed());
    let list = frame(&s, &out, "l");
    let (a, b) = (frame(&s, &out, "row.i2"), frame(&s, &out, "l.section.1"));
    let (c, d) = (frame(&s, &out, "row.i3"), frame(&s, &out, "row.i4"));
    let step = |from: exponential_ui::surface::PlacedFrame, to: exponential_ui::surface::PlacedFrame| to.y - (from.y + from.h);
    // Every boundary = gap + hairline: item → header, header → item, item → item.
    let gap = step(c, d);
    assert!(gap > 1.0, "{gap}");
    assert!((step(a, b) - gap).abs() < 0.01 && (step(b, c) - gap).abs() < 0.01, "{} {} {gap}", step(a, b), step(b, c));
    // The window's content extent is the laid-out one.
    let last = frame(&s, &out, "row.i8");
    assert!((out.lists[0].content_height - (last.y + last.h - list.y)).abs() < 0.01, "{} vs {}", out.lists[0].content_height, last.y + last.h - list.y);
    // Dividers carry the item's index: `divider.4` sits before item 4; none before a header's first item.
    let ids: Vec<String> = s.nodes().into_iter().map(|n| n.id).collect();
    assert!(ids.contains(&"l.divider.4".to_string()) && ids.contains(&"l.divider.1".to_string()), "{ids:?}");
    assert!(!ids.contains(&"l.divider.3".to_string()), "item 3 opens section 1");
}

#[test]
fn position_sticky_pins_against_a_host_scrolled_auto_height_surface() {
    // No scroller in the tree and no viewport height: the host scrolls the
    // surface (`set_surface_scroll`); `top` (here via `inset`) still pins.
    for style in [json!({"position": "sticky", "top": 0, "height": 20}), json!({"position": "sticky", "inset": 0, "height": 20})] {
        let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
            "children": [{"id": "bar", "component": "Box", "style": style}, {"id": "body", "component": "Box", "style": {"height": 2000, "flexShrink": 0}}]}));
        s.set_viewport(390.0, 0.0, None);
        s.layout(&mut fixed());
        assert!(s.set_surface_scroll(0.0, 500.0));
        let out = s.layout(&mut fixed());
        let bar = s.index_of("bar").unwrap();
        let st = out.sticky.iter().find(|x| x.index == bar).unwrap_or_else(|| panic!("pinned with {style}: {:?}", out.sticky));
        assert_eq!((st.dx, st.dy), (0.0, 500.0));
    }
}

#[test]
fn a_number_field_parses_text_in_the_host_formatters_separators() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "n", "component": "NumberField", "props": {"label": "N", "name": "n", "value": {"path": "/v"}, "step": 0.01}}]}));
    s.set_formatter(Arc::new(German));
    s.set_data("/v", Some(json!(1)));
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut fixed());
    assert_eq!(prop(&mut s, "n.input", "text"), json!("1,00"), "shown in the host's separators");
    let input = s.index_of("n.input").unwrap();
    s.event(input, "commit", Some(json!({"value": "2,50"})));
    assert_eq!(s.get_data("/v"), Some(&json!(2.5)), "the shown text parses back");
    // Unreadable text changes nothing; empty text clears.
    s.layout(&mut fixed());
    let input = s.index_of("n.input").unwrap();
    assert!(s.event(input, "change", Some(json!({"value": "zwei"}))).is_empty());
    assert_eq!(s.get_data("/v"), Some(&json!(2.5)));
    s.event(input, "commit", Some(json!({"value": " "})));
    assert_eq!(s.get_data("/v"), Some(&Value::Null));
}

#[test]
fn uses_clock_flags_relative_times_without_now() {
    let tree = |props: Value| surface(json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Text", "props": props}]}));
    let mut s = tree(json!({"text": {"call": "formatRelativeTime", "args": {"value": 0}}}));
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut fixed());
    assert!(s.uses_clock());
    s.set_clock(Some(0.0));
    assert!(!s.uses_clock(), "a pinned clock never moves");
    let s = tree(json!({"text": {"call": "formatRelativeTime", "args": {"value": 0, "now": 60000}}}));
    assert!(!s.uses_clock(), "an explicit now");
    let mut table = surface(json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Table", "props": {"columns": [{"key": "at", "label": "When", "type": "relativeTime"}], "rows": [{"id": "a", "at": 0}]}}]}));
    table.set_viewport(400.0, 0.0, None);
    table.layout(&mut fixed());
    assert!(table.uses_clock());
}

/// The a11y position of node `id` as `(posInSet, setSize)`.
fn position(s: &mut Surface, id: &str) -> (Value, Value) {
    let a = s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}")).accessibility.unwrap_or(Value::Null);
    (a["posInSet"].clone(), a["setSize"].clone())
}

#[test]
fn every_list_item_carries_its_place_in_the_whole_list_windowed_or_not() {
    // Parity: a short (flat) list and a long (windowed) one give an item the
    // same `(i + 1, n)` the reference's `aria-posinset`/`aria-setsize` do.
    for n in [3, 120] {
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "l", "component": "List", "style": {"height": 400}, "children": rows(n, 40.0)}]}));
        s.set_viewport(390.0, 800.0, None);
        s.layout(&mut fixed());
        let out = s.layout(&mut fixed());
        assert_eq!(out.lists.first().map(|l| l.windowed), Some(n > 50), "n = {n}");
        for i in [0, 2] {
            assert_eq!(position(&mut s, &format!("r{i}")), (json!(i + 1), json!(n)), "n = {n}, item {i}");
        }
    }
    // A template list after one static child: children first, then the
    // items; sectioned the same, and a header is no item.
    let items: Vec<Value> = (0..4).map(|i| json!({"id": format!("i{i}"), "day": format!("d{}", i / 2)})).collect();
    for props in [json!({}), json!({"sectionBy": "day"})] {
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [
            {"id": "l", "component": "List", "props": props, "children": [{"id": "lead", "component": "Box", "style": {"height": 40}}], "template": {"component": "row", "path": "/items", "key": "id"}},
            {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}}
        ]}));
        s.set_data("", Some(json!({"items": items})));
        s.layout(&mut fixed());
        assert_eq!(position(&mut s, "lead"), (json!(1), json!(5)), "{props}");
        assert_eq!(position(&mut s, "row.i0"), (json!(2), json!(5)), "{props}");
        assert_eq!(position(&mut s, "row.i3"), (json!(5), json!(5)), "{props}");
    }
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "l", "component": "List", "props": {"sectionBy": "day"}, "template": {"component": "row", "path": "/items", "key": "id"}},
        {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}}
    ]}));
    s.set_data("", Some(json!({"items": items})));
    s.layout(&mut fixed());
    assert_eq!(position(&mut s, "l.section.1"), (Value::Null, Value::Null), "a header is not counted");
}

/// VAPP-100: a node is `hover_styled` when its style has a `:hover` block or
/// its recipe has a hover rule its props match; painters track the pointer
/// over exactly these (plus pressables, triggers, fields).
#[test]
fn hover_styled_marks_hover_blocks_and_hover_recipes() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "plain", "component": "Box", "style": {"width": 10, "height": 10}},
        {"id": "styled", "component": "Box", "style": {"width": 10, "height": 10, ":hover": {"opacity": 0.5}}},
        {"id": "b", "component": "Button", "props": {"label": "Go", "variant": "default"}},
        {"id": "t", "component": "Text", "props": {"text": "Body"}}
    ]}));
    s.set_viewport(400.0, 0.0, None);
    s.set_theme(None);
    s.layout(&mut fixed());
    let at = |s: &Surface, id: &str| s.hover_styled(s.index_of(id).unwrap());
    assert!(at(&s, "styled"));
    assert!(!at(&s, "plain") && !at(&s, "t"));
    // Without a theme no recipe applies; the neutral Button has a hover rule.
    assert!(!at(&s, "b"));
    s.set_theme(Some(exponential_ui::themes::builtin_theme("neutral").unwrap()));
    s.layout(&mut fixed());
    assert!(at(&s, "b"));
    assert!(!at(&s, "plain") && !at(&s, "t"));
}
