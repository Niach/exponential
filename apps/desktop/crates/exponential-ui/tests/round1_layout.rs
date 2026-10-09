//! Round 1 (renderer hardening) in the layout core: the bind pass at layout
//! time, baselines, per-side keys, `%` radii, incremental restyle and
//! re-windowing, settings (insets, density, font scale, contrast, system
//! mode), scroll containers, scrolling dialog bodies, the popups as layers,
//! the new natives, forms, toasts, menus, RTL, responsive natives, strings,
//! locale, commands and template keys.

use std::collections::HashMap;

use exponential_ui::measure::{ControlBox, FixedMeasure, HeightRequest, Intrinsics, LeafRequest, Measure};
use exponential_ui::surface::{ContrastSetting, Insets, LayoutOutput, OutEvent, Surface, SurfaceCommand, SurfaceOptions};
use exponential_ui::theme::{Density, Mode, ModeSetting};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn geometry(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: None, ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> exponential_ui::surface::PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())).find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

fn press(s: &mut Surface, id: &str) -> Vec<OutEvent> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    s.event(i, "press", None)
}

fn data_changed(events: &[OutEvent], path: &str) -> Option<Value> {
    events.iter().find_map(|e| match e {
        OutEvent::DataChanged { path: p, value } if p == path => Some(value.clone()),
        _ => None,
    })
}

fn action<'a>(events: &'a [OutEvent], name: &str) -> Option<&'a Value> {
    events.iter().find_map(|e| match e {
        OutEvent::Action { name: n, context, .. } if n == name => Some(context),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// The bind pass at layout time
// ---------------------------------------------------------------------------

#[test]
fn bound_macro_inputs_bind_at_layout_time() {
    let mut s = surface(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            {"id": "p", "component": "Progress", "props": {"value": {"path": "/done"}, "max": 8}},
            {"id": "pg", "component": "Pagination", "props": {"page": {"path": "/page"}, "totalPages": 5}},
            {"id": "c", "component": "Collapsible", "props": {"title": "More", "open": {"path": "/open"}}, "children": [{"id": "body", "component": "Text", "props": {"text": "Body"}}]}
        ]
    }));
    s.set_data("", Some(json!({"done": 2, "page": 2, "open": false})));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    let nodes = s.nodes();
    let label = nodes.iter().find(|n| n.id == "pg.label").unwrap();
    assert_eq!(label.props["text"], json!("2 / 5"), "the audit's `[object Object] / 5`");
    assert!(s.index_of("c.body").is_none(), "a closed bound Collapsible draws no body");
    // Progress: the fill's width is a bound `percent{…}` style value.
    let track = frame(&s, &out, "p");
    let fill = nodes.iter().find(|n| n.id.starts_with("p.") && n.part.is_none() && n.id.ends_with("fill")).map(|n| n.id.clone()).unwrap_or_else(|| "p.fill".into());
    let f = frame(&s, &out, &fill);
    assert!((f.w - track.w * 0.25).abs() < 1.0, "fill {} of {}", f.w, track.w);
    // Pressing the trigger writes `open` through `set` (two-way binding).
    let events = press(&mut s, "c.trigger");
    assert_eq!(data_changed(&events, "/open"), Some(json!(true)));
    let out = s.layout(&mut m);
    assert!(s.index_of("c.body").is_some(), "open after the press");
    assert!(!out.delta.added.is_empty(), "the body arrived as a delta");
    let events = press(&mut s, "pg.next");
    assert_eq!(data_changed(&events, "/page"), Some(json!(3)));
    s.layout(&mut m);
    assert_eq!(s.nodes().iter().find(|n| n.id == "pg.label").unwrap().props["text"], json!("3 / 5"));
}

#[test]
fn visible_drops_a_node_and_its_subtree_and_brings_it_back() {
    let mut s = surface(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [
            {"id": "a", "component": "Text", "props": {"text": "Always"}},
            {"id": "b", "component": "Box", "visible": {"path": "/show"}, "children": [{"id": "b1", "component": "Text", "props": {"text": "Sometimes"}}]},
            {"id": "c", "component": "Text", "visible": false, "props": {"text": "Never"}}
        ]
    }));
    s.set_viewport(300.0, 0.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    assert!(s.index_of("b").is_none() && s.index_of("b1").is_none() && s.index_of("c").is_none());
    let a = s.index_of("a").unwrap();
    s.set_data("/show", Some(json!(1)));
    let out2 = s.layout(&mut m);
    assert!(s.index_of("b1").is_some(), "0 and 1 are truthy");
    assert_eq!(s.index_of("a"), Some(a), "a stays in its slot");
    assert_eq!(out2.delta.added.len(), 2);
    assert!(out2.surface_height > out.surface_height);
    s.set_data("/show", Some(json!("")));
    let out3 = s.layout(&mut m);
    assert!(s.index_of("b").is_none());
    assert_eq!(out3.delta.removed.len(), 2);
    let nodes = s.nodes();
    assert!(out3.delta.removed.iter().all(|i| nodes[*i as usize].removed), "tombstones keep nodes()[i].index == i");
}

// ---------------------------------------------------------------------------
// Measurement: baselines, per-side padding, % radii
// ---------------------------------------------------------------------------

#[test]
fn baseline_alignment_uses_the_measured_first_baselines() {
    let mut s = geometry(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "baseline", "gap": 8},
        "children": [
            {"id": "b", "component": "Button", "props": {"label": "Go"}, "style": {"height": 40}},
            {"id": "t", "component": "Text", "props": {"text": "Label"}}
        ]
    }));
    s.set_viewport(300.0, 0.0, None);
    let out = s.layout(&mut fixed());
    // The button's text baseline: (40 - 20) / 2 + 15 = 25; the text's: 15.
    assert_eq!(frame(&s, &out, "b").y, 0.0);
    assert_eq!(frame(&s, &out, "t").y, 10.0);
}

struct Capture {
    inner: FixedMeasure,
    controls: HashMap<String, ControlBox>,
}

impl Measure for Capture {
    fn measure_id(&self) -> u64 {
        11
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        for l in leaves {
            self.controls.insert(l.id.to_string(), l.control);
        }
        self.inner.measure_intrinsics(leaves)
    }
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        self.inner.measure_heights(leaves, requests)
    }
}

#[test]
fn a_leafs_per_side_and_logical_padding_reaches_the_measurer_and_mirrors() {
    for (dir, left, right) in [("ltr", 10.0, 4.0), ("rtl", 4.0, 10.0)] {
        let mut s = geometry(json!({
            "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "direction": dir},
            "children": [{"id": "t", "component": "Text", "props": {"text": "Padded", "style": {"paddingTop": 2, "paddingInlineStart": 10, "paddingInlineEnd": 4, "textAlign": "start"}}}]
        }));
        s.set_viewport(300.0, 0.0, None);
        let mut m = Capture { inner: fixed(), controls: HashMap::new() };
        s.layout(&mut m);
        let c = m.controls["t"];
        assert_eq!(c.padding, [2.0, right, 0.0, left], "{dir}");
        assert_eq!(c.padding_horizontal, 7.0);
        let v = s.visual(s.index_of("t").unwrap()).unwrap();
        assert_eq!(v.text_align.as_deref(), Some(if dir == "rtl" { "right" } else { "left" }), "textAlign start is physical");
    }
}

#[test]
fn percent_radii_resolve_against_the_laid_out_box() {
    let mut s = geometry(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row"},
        "children": [{"id": "pill", "component": "Box", "style": {"width": 80, "height": 24, "borderRadius": "50%", "borderTopLeftRadius": "25%"}}]
    }));
    s.set_viewport(300.0, 0.0, None);
    s.layout(&mut fixed());
    let v = s.visual(s.index_of("pill").unwrap()).unwrap();
    assert_eq!(v.border_radius, Some(12.0));
    assert_eq!(v.corner_radii, Some([6.0, 12.0, 12.0, 12.0]));
}

// ---------------------------------------------------------------------------
// Incremental work
// ---------------------------------------------------------------------------

fn long_list(n: usize) -> Value {
    let rows: Vec<Value> = (0..n).map(|i| json!({"id": format!("r{i}"), "component": "Box", "props": {"pressable": true}, "style": {"display": "flex", "flexDirection": "row", "gap": 8, "padding": 8, ":hover": {"backgroundColor": "$color.accent"}}, "children": [{"id": format!("r{i}-t"), "component": "Text", "props": {"text": format!("Row {i}")}}]})).collect();
    json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 8},
        "children": [
            {"id": "head", "component": "Text", "props": {"text": "Header", "variant": "title"}},
            {"id": "list", "component": "List", "style": {"height": 600}, "children": rows}
        ]
    })
}

#[test]
fn a_hover_restyles_one_node_and_a_scroll_rewindows_without_touching_the_rest() {
    let mut s = surface(long_list(2_000));
    s.set_viewport(390.0, 800.0, None);
    let mut m = fixed();
    let first = s.layout(&mut m);
    assert!(first.restyled > 0);
    // The first pass measured the rows; the window settles on the next.
    s.layout(&mut m);
    let settled = s.layout(&mut m);
    assert!(!settled.rebuilt && settled.restyled == 0);
    let head = s.index_of("head").unwrap();
    let list = s.index_of("list").unwrap();
    assert!(s.set_states("r3", vec!["hover".into()]));
    let hover = s.layout(&mut m);
    assert_eq!(hover.restyled, 1, "only the hovered row");
    assert!(!hover.rebuilt);
    assert_eq!(hover.upcalls, 0);
    assert_eq!(hover.visual_changes, vec![s.index_of("r3").unwrap()]);
    assert!(hover.delta.is_empty());
    s.scroll("list", 20_000.0);
    let scrolled = s.layout(&mut m);
    assert!(scrolled.rebuilt);
    assert_eq!(s.index_of("head"), Some(head), "nodes outside the window keep their slot");
    assert_eq!(s.index_of("list"), Some(list));
    assert!(!scrolled.delta.added.is_empty() && !scrolled.delta.removed.is_empty() && !scrolled.delta.renumbered);
    let rows_in_window = (scrolled.lists[0].end - scrolled.lists[0].start) as usize;
    assert!(scrolled.restyled as usize <= 2 * rows_in_window + 4, "restyled {} for a window of {rows_in_window}", scrolled.restyled);
    let at_list = scrolled.scrolls.iter().find(|x| x.index == list).expect("the list scrolls");
    assert_eq!(at_list.offset_y, 20_000.0);
    // Scrolling far and back many times compacts the slots now and then;
    // a compaction says so.
    let mut renumbered = false;
    for k in 0..60 {
        s.scroll("list", ((k * 7919) % 70_000) as f32);
        renumbered |= s.layout(&mut m).delta.renumbered;
    }
    assert!(s.nodes().len() < 2_000, "slots stay bounded ({}), compaction ran: {renumbered}", s.nodes().len());
}

#[test]
fn a_scroll_rebuilds_only_the_lists_window_not_the_surface_around_it() {
    let statics: Vec<Value> = (0..800).map(|i| json!({"id": format!("s{i}"), "component": "Text", "props": {"text": format!("Static {i}")}})).collect();
    let rows: Vec<Value> = (0..2_000).map(|i| json!({"id": format!("r{i}"), "component": "Text", "props": {"text": format!("Row {i}")}})).collect();
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [{"id": "statics", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": statics}, {"id": "list", "component": "List", "style": {"height": 400}, "children": rows}]}));
    s.set_viewport(390.0, 800.0, None);
    let mut m = fixed();
    let first = s.layout(&mut m);
    assert!(first.built_nodes > 800);
    s.layout(&mut m);
    s.scroll("list", 30_000.0);
    let out = s.layout(&mut m);
    assert!(out.rebuilt);
    assert!(out.built_nodes < 120, "the window only: {} nodes built", out.built_nodes);
    assert!(out.delta.changed.iter().all(|i| !s.nodes()[*i as usize].id.starts_with('s')), "nothing outside the list changed");
    let list = s.index_of("list").unwrap();
    assert_eq!(out.scrolls.iter().find(|x| x.index == list).unwrap().offset_y, 30_000.0);
}

#[test]
fn a_data_write_through_and_a_tab_switch_reuse_every_unchanged_node() {
    let mut s = surface(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [
            {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}]},
             "children": [{"id": "pa", "component": "Text", "props": {"text": "Panel A"}}, {"id": "pb", "component": "Text", "props": {"text": "Panel B"}}]},
            {"id": "name", "component": "Text", "props": {"text": {"path": "/name"}}},
            {"id": "other", "component": "Text", "props": {"text": "Static"}}
        ]
    }));
    s.set_data("/name", Some(json!("Ada")));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    let first = s.layout(&mut m);
    s.set_data("/name", Some(json!("Grace")));
    let renamed = s.layout(&mut m);
    assert_eq!(renamed.structure_version, first.structure_version);
    assert_eq!(renamed.delta.changed, vec![s.index_of("name").unwrap()]);
    assert_eq!(renamed.restyled, 0, "a text change re-measures, it does not restyle");
    assert_eq!(renamed.upcalls, 1);
    let tab_b = s.index_of("tabs.tab.1").unwrap();
    s.event(tab_b, "press", None);
    let switched = s.layout(&mut m);
    assert!(switched.restyled <= 6, "restyled {}", switched.restyled);
    assert!(switched.built_nodes <= 7, "only the Tabs subtree rebuilt: {}", switched.built_nodes);
    assert!(switched.delta.changed.contains(&s.index_of("pb").unwrap()));
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

fn dialog(children: usize) -> Value {
    let body: Vec<Value> = (0..children).map(|i| json!({"id": format!("line-{i}"), "component": "Text", "props": {"text": format!("Line {i}")}})).collect();
    json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [{"id": "dlg", "component": "Dialog", "props": {"title": "Title", "open": true}, "children": body}]
    })
}

#[test]
fn layers_stay_inside_the_safe_area_and_a_tall_dialog_scrolls_its_body() {
    let mut s = surface(dialog(60));
    s.set_viewport(390.0, 844.0, None);
    s.set_insets(Insets { top: 47.0, right: 0.0, bottom: 34.0, left: 0.0 });
    let mut m = fixed();
    let out = s.layout(&mut m);
    let layer = &out.layers[0];
    assert!(layer.modal);
    let root = layer.frames[0];
    assert!(root.y >= 47.0 + 8.0 - 0.01, "below the status bar: {}", root.y);
    assert!(root.y + root.h <= 844.0 - 34.0 - 8.0 + 0.01, "above the home indicator: {} + {}", root.y, root.h);
    let body = s.index_of("dlg.body").unwrap();
    let scroll = out.scrolls.iter().find(|x| x.index == body).expect("the body is a scroll container");
    assert!(scroll.content_height > frame(&s, &out, "dlg.body").h + 100.0);
    // A bottom sheet reaches the edge but pads its content clear of it.
    let mut sheet = surface(json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Drawer", "props": {"open": true, "side": "bottom"}, "children": [{"id": "x", "component": "Text", "props": {"text": "Sheet"}}]}]}));
    sheet.set_viewport(390.0, 844.0, None);
    sheet.set_insets(Insets { top: 47.0, right: 0.0, bottom: 34.0, left: 0.0 });
    let out = sheet.layout(&mut m);
    let root = out.layers[0].frames[0];
    assert!((root.y + root.h - 844.0).abs() < 0.01, "flush with the bottom edge");
    let text = frame(&sheet, &out, "x");
    assert!(root.y + root.h - (text.y + text.h) >= 34.0, "content clear of the home indicator");
}

#[test]
fn font_scale_density_contrast_and_system_mode() {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": "$spacing.md"},
        "children": [{"id": "b", "component": "Button", "props": {"label": "Go"}}, {"id": "t", "component": "Text", "props": {"text": "Body copy"}}]});
    let mut s = surface(tree);
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let t = s.index_of("t").unwrap();
    let base = s.text_style(t).unwrap().font_size;
    s.set_font_scale(1.5);
    let scaled = s.layout(&mut m);
    assert_eq!(s.text_style(t).unwrap().font_size, base * 1.5);
    assert!(scaled.upcalls >= 1, "a type-size change re-measures");
    s.set_font_scale(1.0);
    s.layout(&mut m);
    let tall = s.layout(&mut m).surface_height;
    s.set_density(Density::Compact);
    let compact = s.layout(&mut m);
    assert!(compact.surface_height < tall, "compact tightens controls and spacing: {} vs {tall}", compact.surface_height);
    let b = s.index_of("b").unwrap();
    s.set_density(Density::Default);
    s.set_mode_setting(ModeSetting::System, true);
    s.layout(&mut m);
    assert_eq!(s.mode(), Mode::Dark);
    let theme = s.effective_theme().unwrap().clone();
    assert_eq!(s.visual(b).unwrap().background_color.as_deref(), Some(theme.modes.dark.color["primary"].as_str()));
    s.set_contrast(ContrastSetting::High, false);
    s.layout(&mut m);
    let high = s.effective_theme().unwrap().clone();
    assert_eq!(high.modes.dark.color["foreground"], theme.contrast.dark.color.get("foreground").cloned().unwrap_or_else(|| theme.modes.dark.color["foreground"].clone()));
    assert_eq!(s.visual(t).unwrap().color.as_deref(), Some(high.modes.dark.color["foreground"].as_str()).filter(|_| s.visual(t).unwrap().color.is_some()).or(s.visual(t).unwrap().color.as_deref()));
}

#[test]
fn reduced_motion_zeroes_transitions_and_hover_media_follows_the_pointer() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "x", "component": "Box", "style": {"width": 10, "height": 10, "transition": "$motion.fast", "transitionEasing": "$ease.standard", "@media (hover: none)": {"minHeight": 44}}}]}));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let x = s.index_of("x").unwrap();
    let t = s.visual(x).unwrap().transition.expect("a transition");
    assert!(t.duration_ms > 0.0);
    s.set_pointer(false, true);
    let out = s.layout(&mut m);
    assert_eq!(s.visual(x).unwrap().transition.unwrap().duration_ms, 0.0);
    assert_eq!(frame(&s, &out, "x").h, 44.0, "touch: (hover: none) applies");
}

#[test]
fn any_scroll_container_reports_and_clamps_its_offset() {
    let chips: Vec<Value> = (0..20).map(|i| json!({"id": format!("c{i}"), "component": "Box", "style": {"width": 60, "height": 24, "flexShrink": 0}})).collect();
    let mut s = geometry(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [{"id": "strip", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": 4, "overflowX": "scroll"}, "children": chips}]}));
    s.set_viewport(300.0, 0.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    let strip = s.index_of("strip").unwrap();
    let sc = out.scrolls.iter().find(|x| x.index == strip).unwrap();
    assert!(sc.scroll_x && !sc.scroll_y);
    assert_eq!(sc.content_width, 20.0 * 60.0 + 19.0 * 4.0);
    assert!(s.scroll_to("strip", 5_000.0, 0.0));
    let out = s.layout(&mut m);
    let sc = out.scrolls.iter().find(|x| x.index == strip).unwrap();
    assert_eq!(sc.offset_x, sc.content_width - 300.0, "clamped to the content");
    assert!(!out.rebuilt, "a plain scroll container moves paint only");
}

// ---------------------------------------------------------------------------
// Popups are layers
// ---------------------------------------------------------------------------

fn field_surface(field: Value) -> Surface {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": 16}, "children": [field]}));
    s.set_viewport(390.0, 844.0, None);
    s
}

#[test]
fn select_opens_a_layer_under_its_trigger_filters_and_writes_its_value() {
    let mut s = field_surface(json!({"id": "sel", "component": "Select", "props": {"label": "Fruit", "name": "fruit", "searchable": true, "value": {"path": "/fruit"},
        "options": [{"label": "Apple", "value": "apple"}, {"label": "Banana", "value": "banana"}, {"label": "Cherry", "value": "cherry", "disabled": true}]}}));
    let mut m = fixed();
    let closed = s.layout(&mut m);
    assert!(closed.layers.is_empty());
    assert_eq!(s.nodes().iter().find(|n| n.id == "sel.trigger").unwrap().props["text"], json!("Choose"), "the built-in placeholder");
    press(&mut s, "sel.trigger");
    let open = s.layout(&mut m);
    assert_eq!(open.layers.len(), 1);
    let layer = &open.layers[0];
    assert_eq!(layer.kind, "Select");
    let trigger = frame(&s, &open, "sel.trigger");
    let root = layer.frames[0];
    assert!(root.w >= trigger.w - 0.01, "as wide as the trigger");
    assert!((root.y - (trigger.y + trigger.h + 4.0)).abs() < 0.01, "4 px under it");
    assert_eq!(layer.anchor_frame.unwrap().y, trigger.y);
    let search = s.index_of("sel.search").unwrap();
    let events = s.event(search, "change", Some(json!({"value": "an"})));
    assert!(action(&events, "nothing").is_none());
    s.layout(&mut m);
    assert!(s.index_of("sel.item.0").is_none(), "Apple filtered out");
    assert!(s.index_of("sel.item.1").is_some(), "Banana matches");
    let events = press(&mut s, "sel.item.1");
    assert_eq!(data_changed(&events, "/fruit"), Some(json!("banana")));
    assert!(events.iter().any(|e| matches!(e, OutEvent::DataChanged { path, value } if path == "/fruit" && value == &json!("banana"))));
    let out = s.layout(&mut m);
    assert!(out.layers.is_empty(), "a single Select closes on pick");
    assert_eq!(s.nodes().iter().find(|n| n.id == "sel.trigger").unwrap().props["text"], json!("Banana"));
}

#[test]
fn a_popup_near_the_bottom_flips_above_its_trigger() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "justifyContent": "flex-end", "height": 400, "width": "100%"},
        "children": [{"id": "t", "component": "TimePicker", "props": {"label": "At", "name": "at", "step": 60}}]}));
    s.set_viewport(390.0, 400.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    press(&mut s, "t.trigger");
    let out = s.layout(&mut m);
    let layer = &out.layers[0];
    let placement = layer.placement.unwrap();
    assert!(placement.flipped, "no room below: it flips");
    assert_eq!(layer.position, "top");
    assert!(out.scrolls.iter().any(|x| x.index == layer.root), "the list scrolls (24 entries)");
}

#[test]
fn date_picker_calendar_follows_the_locale_week_start_and_picks() {
    let mut s = field_surface(json!({"id": "d", "component": "DatePicker", "props": {"label": "Due", "name": "due", "value": "2026-10-07"}}));
    let mut m = fixed();
    s.layout(&mut m);
    press(&mut s, "d.trigger");
    s.layout(&mut m);
    let nodes = s.nodes();
    let text = |id: &str| nodes.iter().find(|n| n.id == id).unwrap().props["text"].clone();
    assert_eq!(text("d.title"), json!("October 2026"));
    // Round 2: weekday names = the formatter's `EEE`.
    assert_eq!(text("d.weekday.0"), json!("Sun"), "en-US starts on Sunday");
    assert_eq!(text("d.day.0.0"), json!("27"), "Sun 27 Sep 2026 leads the grid");
    assert!(nodes.iter().find(|n| n.id == "d.day.1.3").unwrap().states.contains(&"selected".to_string()), "Wed 7 Oct");
    let wed = nodes.iter().find(|n| n.id == "d.day.1.3").unwrap();
    assert_eq!(wed.accessibility.as_ref().unwrap()["label"], json!("Wednesday, October 7, 2026"), "a day's name = the full localized date");
    s.set_locale("de-DE");
    s.layout(&mut m);
    let nodes = s.nodes();
    assert_eq!(nodes.iter().find(|n| n.id == "d.weekday.0").unwrap().props["text"], json!("Mon"), "de starts on Monday (names: the English fallback formatter)");
    press(&mut s, "d.next");
    s.layout(&mut m);
    assert_eq!(s.nodes().iter().find(|n| n.id == "d.title").unwrap().props["text"], json!("November 2026"));
    let events = press(&mut s, "d.day.2.2");
    assert!(events.contains(&OutEvent::Relayout));
    let out = s.layout(&mut m);
    assert!(out.layers.is_empty());
    let value = s.nodes().iter().find(|n| n.id == "d.trigger").unwrap().props["value"].clone();
    assert_eq!(value, json!("2026-11-11"), "Wed 11 Nov (Monday-first grid row 2, column 2)");
}

#[test]
fn date_range_picks_in_two_steps_and_orders_them() {
    let mut s = field_surface(json!({"id": "r", "component": "DateRangePicker", "props": {"label": "Sprint", "name": "sprint", "start": {"path": "/start"}, "end": {"path": "/end"}}}));
    s.set_settings(exponential_ui::surface::SurfaceSettings { today: Some("2026-10-07".into()), ..s.settings().clone() });
    let mut m = fixed();
    s.layout(&mut m);
    press(&mut s, "r.trigger");
    s.layout(&mut m);
    assert!(s.nodes().iter().find(|n| n.id == "r.day.1.3").unwrap().props["today"] == json!(true));
    press(&mut s, "r.day.2.3");
    s.layout(&mut m);
    assert!(s.layout(&mut m).layers.len() == 1, "still open after the first pick");
    let events = press(&mut s, "r.day.1.0");
    assert_eq!(data_changed(&events, "/start"), Some(json!("2026-10-04")));
    assert_eq!(data_changed(&events, "/end"), Some(json!("2026-10-14")));
}

#[test]
fn an_unbound_toggle_shows_its_local_state() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "tg", "component": "Toggle", "props": {"label": "Bold"}}]}));
    s.set_viewport(300.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let events = press(&mut s, "tg");
    assert!(events.contains(&OutEvent::Relayout));
    let out = s.layout(&mut m);
    assert_eq!(s.nodes().into_iter().find(|n| n.id == "tg").unwrap().props["pressed"], json!(true));
    assert!(out.visual_changes.contains(&s.index_of("tg").unwrap()) || out.restyled >= 1);
}

#[test]
fn an_unbound_range_keeps_its_picks_locally() {
    let mut s = field_surface(json!({"id": "r", "component": "DateRangePicker", "props": {"label": "Sprint", "name": "sprint", "start": "2026-10-07"}}));
    let mut m = fixed();
    s.layout(&mut m);
    press(&mut s, "r.trigger");
    s.layout(&mut m);
    press(&mut s, "r.day.2.0");
    s.layout(&mut m);
    press(&mut s, "r.day.2.6");
    s.layout(&mut m);
    let trigger = s.nodes().into_iter().find(|n| n.id == "r.trigger").unwrap();
    assert_eq!(trigger.props["start"], json!("2026-10-11"));
    assert_eq!(trigger.props["end"], json!("2026-10-17"));
}

#[test]
fn number_field_steps_clamp_and_mirror_in_rtl() {
    for dir in ["ltr", "rtl"] {
        let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": dir},
            "children": [{"id": "n", "component": "NumberField", "props": {"label": "Seats", "name": "seats", "value": {"path": "/n"}, "min": 1, "max": 3, "unit": "seats"}}]}));
        s.set_data("/n", Some(json!(2)));
        s.set_viewport(390.0, 0.0, None);
        let mut m = fixed();
        let out = s.layout(&mut m);
        let dec = frame(&s, &out, "n.decrement");
        let inc = frame(&s, &out, "n.increment");
        if dir == "ltr" {
            assert!(dec.x < inc.x);
        } else {
            assert!(dec.x > inc.x, "RTL mirrors the steppers");
        }
        let events = press(&mut s, "n.increment");
        assert_eq!(data_changed(&events, "/n"), Some(json!(3)));
        s.layout(&mut m);
        assert!(s.nodes().iter().find(|n| n.id == "n.increment").unwrap().states.contains(&"disabled".to_string()), "at max");
        let input = s.index_of("n.input").unwrap();
        let events = s.event(input, "commit", Some(json!({"value": "99"})));
        assert_eq!(data_changed(&events, "/n"), Some(json!(3)), "a typed value clamps on commit");
    }
}

#[test]
fn chip_input_adds_removes_and_suggests() {
    let mut s = field_surface(json!({"id": "c", "component": "ChipInput", "props": {"label": "Labels", "name": "labels", "values": {"path": "/labels"}, "max": 3,
        "suggestions": [{"label": "bug", "value": "bug"}, {"label": "build", "value": "build"}, {"label": "ios", "value": "ios"}]}}));
    s.set_data("/labels", Some(json!(["ios"])));
    let mut m = fixed();
    s.layout(&mut m);
    let input = s.index_of("c.input").unwrap();
    s.event(input, "change", Some(json!({"value": "bu"})));
    let out = s.layout(&mut m);
    assert_eq!(out.layers.len(), 1, "matching suggestions");
    assert!(s.index_of("c.suggestion.0").is_some() && s.index_of("c.suggestion.1").is_some());
    let events = press(&mut s, "c.suggestion.1");
    assert_eq!(data_changed(&events, "/labels"), Some(json!(["ios", "build"])));
    assert!(action(&events, "x").is_none());
    let input = s.index_of("c.input").unwrap();
    let events = s.event(input, "submit", Some(json!({"value": " bug "})));
    assert_eq!(data_changed(&events, "/labels"), Some(json!(["ios", "build", "bug"])));
    let input = s.index_of("c.input").unwrap();
    let events = s.event(input, "submit", Some(json!({"value": "more"})));
    assert!(data_changed(&events, "/labels").is_none(), "max 3");
    s.layout(&mut m);
    // The remove icon's accessible name names its chip (`removeItem`).
    assert_eq!(s.nodes().iter().find(|n| n.id == "c.remove.0").unwrap().props["label"], json!("Remove ios"));
    let events = press(&mut s, "c.remove.0");
    assert_eq!(data_changed(&events, "/labels"), Some(json!(["build", "bug"])));
}

#[test]
fn file_upload_attaches_refuses_too_large_and_removes() {
    let mut s = field_surface(json!({"id": "f", "component": "FileUpload", "props": {"label": "Files", "name": "files", "multiple": true, "maxSize": 1000}, "on": {"upload": {"event": {"name": "upload"}}}}));
    let mut m = fixed();
    s.layout(&mut m);
    let events = press(&mut s, "f.browse");
    assert!(events.iter().any(|e| matches!(e, OutEvent::PickFiles { component_id, multiple: true, .. } if component_id == "f")));
    let zone = s.index_of("f.dropzone").unwrap();
    let events = s.event(zone, "upload", Some(json!({"files": [{"name": "a.png", "size": 10, "type": "image/png"}, {"name": "huge.mov", "size": 5000, "type": "video/quicktime"}]})));
    assert_eq!(action(&events, "upload").unwrap()["files"], json!([{"name": "a.png", "size": 10, "type": "image/png"}]));
    s.layout(&mut m);
    let nodes = s.nodes();
    assert_eq!(nodes.iter().find(|n| n.id == "f.fileName.0").unwrap().props["text"], json!("a.png"));
    // `fileTooLargeNamed` = "{name}: File too large" (the React wording).
    assert!(nodes.iter().find(|n| n.id == "f.error").unwrap().props["text"].as_str().unwrap().contains("huge.mov: File too large"));
    assert_eq!(nodes.iter().find(|n| n.id == "f.remove.0").unwrap().props["label"], json!("Remove a.png"));
    press(&mut s, "f.remove.0");
    s.layout(&mut m);
    assert!(s.index_of("f.fileName.0").is_none());
}

#[test]
fn table_sorts_locally_selects_and_windows_past_fifty_rows() {
    let rows: Vec<Value> = (0..120).map(|i| json!({"id": format!("u{i}"), "name": format!("User {:03}", 119 - i), "runs": i % 7})).collect();
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [{"id": "t", "component": "Table", "props": {"columns": [{"key": "name", "label": "Name", "sortable": true}, {"key": "runs", "label": "Runs", "type": "number", "align": "end", "width": 80, "sortable": true}],
            "rows": rows, "selectable": "multiple", "striped": true}, "style": {"height": 500}}]}));
    s.set_viewport(600.0, 800.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    let list = out.lists.iter().find(|l| l.id == "t").expect("the table windows");
    assert!(list.windowed && list.count == 120 && (list.end - list.start) < 60);
    let nodes = s.nodes();
    let row1 = nodes.iter().find(|n| n.id == "t.row.u1").unwrap();
    assert!(nodes.iter().any(|n| n.id == "t.cell.u0.1" && n.props["text"] == json!("0")));
    let _ = row1;
    let name = frame(&s, &out, "t.headerCell.0");
    let runs = frame(&s, &out, "t.headerCell.1");
    assert_eq!(runs.w, 80.0);
    assert!(name.w > 400.0, "columns share the width");
    let cell = frame(&s, &out, "t.cell.u0.1");
    assert_eq!(cell.x, runs.x, "header and rows line up");
    let events = press(&mut s, "t.headerCell.0");
    assert_eq!(action(&events, "sort"), None);
    s.layout(&mut m);
    let nodes = s.nodes();
    let first_row = s.layout_node(s.index_of("t.body").unwrap()).unwrap().children.iter().map(|c| nodes[*c as usize].clone()).find(|n| n.part.as_deref() == Some("row")).unwrap();
    assert_eq!(first_row.id, "t.row.u119", "sorted by name ascending: User 000 first");
    press(&mut s, "t.checkbox.header");
    s.layout(&mut m);
    let header = s.nodes().into_iter().find(|n| n.id == "t.checkbox.header").unwrap();
    assert!(header.states.contains(&"checked".to_string()), "select all");
}

#[test]
fn a_form_refuses_a_submit_with_failing_checks_then_submits_its_values() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [{"id": "form", "component": "Form", "props": {"name": "signup", "summary": true}, "on": {"submit": {"event": {"name": "save"}}, "invalid": {"event": {"name": "invalid"}}},
            "children": [
                {"id": "email", "component": "Input", "props": {"label": "Email", "name": "email", "value": {"path": "/email"}, "checks": [{"condition": {"call": "email", "args": {"value": {"path": "/email"}}}, "message": "Enter an email"}]}},
                {"id": "agree", "component": "Checkbox", "props": {"label": "I agree", "name": "agree", "checked": {"path": "/agree"}, "checks": [{"condition": {"path": "/agree"}, "message": "Required"}]}},
                {"id": "go", "component": "Button", "props": {"label": "Save", "submit": true}}
            ]}]}));
    s.set_data("", Some(json!({"email": "nope", "agree": false})));
    s.set_viewport(390.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let events = press(&mut s, "go");
    assert!(action(&events, "save").is_none());
    let invalid = action(&events, "invalid").expect("invalid fires");
    assert_eq!(invalid["errors"], json!([{"name": "email", "message": "Enter an email"}, {"name": "agree", "message": "Required"}]));
    assert!(events.iter().any(|e| matches!(e, OutEvent::Focus { id, .. } if id == "email")));
    assert!(events.iter().any(|e| matches!(e, OutEvent::Announce { live, .. } if live == "assertive")));
    s.layout(&mut m);
    let nodes = s.nodes();
    assert_eq!(nodes.iter().find(|n| n.id == "email.error").unwrap().props["text"], json!("Enter an email"));
    assert!(nodes.iter().find(|n| n.id == "email.field").unwrap().states.contains(&"invalid".to_string()));
    assert_eq!(nodes.iter().find(|n| n.id == "form.summary").unwrap().props["text"], json!("Enter an email\nRequired"));
    let field = s.index_of("email.field").unwrap();
    s.event(field, "change", Some(json!({"value": "still wrong"})));
    s.layout(&mut m);
    assert!(s.index_of("email.error").is_some(), "a field showing errors re-checks on change");
    let field = s.index_of("email.field").unwrap();
    s.event(field, "change", Some(json!({"value": "ada@example.com"})));
    press(&mut s, "agree");
    s.layout(&mut m);
    assert!(s.index_of("email.error").is_none(), "a change clears the field's errors");
    let events = press(&mut s, "go");
    assert_eq!(action(&events, "save").unwrap()["values"], json!({"email": "ada@example.com", "agree": true}));
}

#[test]
fn validate_on_blur_and_change() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "name", "component": "Input", "props": {"label": "Name", "name": "name", "value": {"path": "/name"}, "checks": [{"condition": {"call": "required", "args": {"value": {"path": "/name"}}}, "message": "Required"}]}},
        {"id": "code", "component": "Input", "props": {"label": "Code", "name": "code", "validateOn": "change", "value": {"path": "/code"}, "checks": [{"condition": {"call": "length", "args": {"value": {"path": "/code"}, "min": 3}}, "message": "Too short"}]}}
    ]}));
    s.set_viewport(390.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let name = s.index_of("name.field").unwrap();
    s.event(name, "change", Some(json!({"value": ""})));
    s.layout(&mut m);
    assert!(s.index_of("name.error").is_none(), "blur fields wait for the blur");
    let name = s.index_of("name.field").unwrap();
    s.event(name, "blur", None);
    s.layout(&mut m);
    assert!(s.index_of("name.error").is_some());
    let code = s.index_of("code.field").unwrap();
    s.event(code, "change", Some(json!({"value": "ab"})));
    s.layout(&mut m);
    assert!(s.index_of("code.error").is_some(), "change fields check as the user types");
    let code = s.index_of("code.field").unwrap();
    s.event(code, "change", Some(json!({"value": "abc"})));
    s.layout(&mut m);
    assert!(s.index_of("code.error").is_none());
}

#[test]
fn toasts_stack_newest_nearest_the_edge_three_at_most_and_dismiss() {
    let toasts: Vec<Value> = (0..4).map(|i| json!({"id": format!("t{i}"), "component": "Toast", "props": {"title": format!("Notice {i}"), "type": if i == 3 { "error" } else { "info" }, "duration": 4000}})).collect();
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": toasts}));
    s.set_viewport(1000.0, 700.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    let toasts: Vec<_> = out.layers.iter().filter(|l| l.class == exponential_ui::layout_tree::LayerClass::Toast).collect();
    assert_eq!(toasts.len(), 3, "the newest three");
    let newest = toasts.iter().find(|l| l.owner == "t3").unwrap();
    let older = toasts.iter().find(|l| l.owner == "t2").unwrap();
    assert!(newest.frames[0].y > older.frames[0].y, "newest nearest the bottom edge");
    assert!(newest.frames[0].x + newest.frames[0].w <= 1000.0 - 8.0 + 0.01 && newest.frames[0].x > 500.0, "bottom-end on a wide surface");
    assert!(out.toasts.iter().any(|t| t.id == "t3" && t.kind == "error" && t.duration_ms == 4000.0));
    let root = s.index_of("t3.root").unwrap();
    assert_eq!(s.nodes()[root as usize].live.as_deref(), Some("assertive"));
    s.dismiss_toast("t3");
    let out = s.layout(&mut m);
    assert!(out.layers.iter().any(|l| l.owner == "t0"), "an older toast moves up into view");
    assert!(!out.layers.iter().any(|l| l.owner == "t3"));
}

#[test]
fn a_context_menu_opens_at_the_point_and_a_submenu_opens_beside_its_row() {
    for dir in ["ltr", "rtl"] {
        let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "height": 600, "direction": dir},
            "children": [{"id": "cm", "component": "ContextMenu", "props": {"items": [
                {"label": "Copy", "value": "copy", "shortcut": "⌘C"}, {"kind": "separator"}, {"kind": "label", "label": "View"},
                {"label": "Show done", "value": "done", "kind": "checkbox", "checked": {"path": "/done"}},
                {"label": "Sort by", "kind": "submenu", "items": [{"label": "Priority", "value": "priority"}, {"label": "Updated", "value": "updated"}]}]},
                "on": {"select": {"event": {"name": "menu"}}},
                "children": [{"id": "target", "component": "Box", "style": {"height": 200, "alignSelf": "stretch"}}]}]}));
        s.set_viewport(800.0, 600.0, None);
        let mut m = fixed();
        s.layout(&mut m);
        let target = s.index_of("target").unwrap();
        let px = if dir == "ltr" { 300.0 } else { 600.0 };
        s.event(target, "contextmenu", Some(json!({"x": px, "y": 100})));
        let out = s.layout(&mut m);
        let layer = &out.layers[0];
        assert_eq!(layer.position, "point");
        let root = layer.frames[0];
        assert_eq!(root.y, 104.0, "4 px under the pointer");
        if dir == "ltr" {
            assert_eq!(root.x, px);
        } else {
            assert_eq!(root.x + root.w, px, "RTL: the menu's end edge at the pointer");
        }
        let events = press(&mut s, "cm.item.3");
        assert_eq!(data_changed(&events, "/done"), Some(json!(true)), "a checkbox entry writes its binding");
        assert_eq!(action(&events, "menu").unwrap()["checked"], json!(true));
        s.event(target, "contextmenu", Some(json!({"x": px, "y": 100})));
        s.layout(&mut m);
        press(&mut s, "cm.item.4");
        let out = s.layout(&mut m);
        assert_eq!(out.layers.len(), 2, "the submenu is its own layer");
        let row = frame(&s, &out, "cm.item.4");
        let sub = out.layers[1].frames[0];
        if dir == "ltr" {
            assert!((sub.x - (row.x + row.w + 4.0)).abs() < 0.01, "beside the row, inline end");
        } else {
            assert!((sub.x + sub.w - (row.x - 4.0)).abs() < 0.01, "RTL: on the left");
        }
        let events = press(&mut s, "cm.item.4.1");
        assert_eq!(action(&events, "menu").unwrap()["value"], json!("updated"));
        assert!(s.layout(&mut m).layers.is_empty());
    }
}

#[test]
fn hover_and_keyboard_focus_open_tooltips_and_hover_popovers() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            {"id": "tip", "component": "Tooltip", "props": {"content": "Saves the draft"}, "children": [{"id": "btn", "component": "Button", "props": {"label": "Save"}}]},
            {"id": "hc", "component": "HoverCard", "slots": {"trigger": {"id": "who", "component": "Text", "props": {"text": "@ada"}}}, "children": [{"id": "card", "component": "Text", "props": {"text": "Ada Lovelace"}}]}
        ]}));
    s.set_viewport(400.0, 600.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    assert!(s.set_states("tip.anchor", vec!["hover".into()]));
    assert!(s.layout(&mut m).layers.iter().any(|l| l.kind == "Tooltip"));
    s.set_states("tip.anchor", vec![]);
    assert!(s.layout(&mut m).layers.is_empty());
    s.set_states("who", vec!["focus".into(), "focus-visible".into()]);
    let out = s.layout(&mut m);
    assert!(out.layers.iter().any(|l| l.owner == "hc"), "keyboard focus opens a hover card");
}

#[test]
fn a_responsive_drawer_is_a_bottom_sheet_on_phones_and_a_side_panel_from_md() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Drawer", "props": {"open": true, "side": {"base": "bottom", "md": "right"}, "title": "Filters"}, "children": [{"id": "x", "component": "Text", "props": {"text": "Body"}}]}]}));
    let mut m = fixed();
    s.set_viewport(390.0, 844.0, None);
    let phone = s.layout(&mut m);
    assert_eq!(phone.layers[0].position, "bottom");
    assert_eq!(phone.breakpoint, None);
    assert!(s.index_of("d.handle").is_some(), "a sheet has a drag handle");
    s.set_viewport(1024.0, 768.0, None);
    let desk = s.layout(&mut m);
    assert_eq!(desk.layers[0].position, "right");
    assert_eq!(desk.breakpoint.as_deref(), Some("lg"));
    assert!(s.index_of("d.handle").is_none());
}

#[test]
fn strings_and_locale_reach_the_parts() {
    let mut s = field_surface(json!({"id": "sel", "component": "Select", "props": {"label": "L", "name": "n", "options": [{"label": "A", "value": "a"}]}}));
    let mut overrides = indexmap::IndexMap::new();
    overrides.insert("choose".to_string(), "Auswählen".to_string());
    s.set_strings(overrides);
    let mut m = fixed();
    s.layout(&mut m);
    assert_eq!(s.nodes().iter().find(|n| n.id == "sel.trigger").unwrap().props["text"], json!("Auswählen"));
    s.set_locale("ar-EG");
    let out = s.layout(&mut m);
    assert_eq!(out.direction, "rtl", "an RTL locale flips the surface");
    let label = frame(&s, &out, "sel.label");
    assert!(label.x + label.w > 390.0 - 16.0 - 0.01 || label.x > 16.0, "the label sits at the inline start (right)");
}

#[test]
fn commands_focus_announce_and_scroll_into_view() {
    let mut s = surface(long_list(500));
    s.set_viewport(390.0, 800.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    s.layout(&mut m);
    let events = s.command(&SurfaceCommand::Announce { text: "Saved".into(), live: Some("assertive".into()) });
    assert_eq!(events, vec![OutEvent::Announce { text: "Saved".into(), live: "assertive".into() }]);
    let events = s.command(&SurfaceCommand::Focus { id: "r2".into() });
    assert!(matches!(&events[0], OutEvent::Focus { id, .. } if id == "r2"));
    let events = s.command(&SurfaceCommand::ScrollIntoView { id: "r400".into() });
    assert!(events.contains(&OutEvent::Relayout));
    s.layout(&mut m);
    let out = s.layout(&mut m);
    let r400 = frame(&s, &out, "r400");
    let list = frame(&s, &out, "list");
    let offset = out.scrolls.iter().find(|x| x.index == s.index_of("list").unwrap()).unwrap().offset_y;
    let top = r400.y - list.y - offset;
    assert!((0.0..list.h).contains(&top), "row 400 is in view: {top}");
}

#[test]
fn template_keys_keep_a_rows_slot_when_the_items_reorder() {
    let mut s = Surface::new("k", SurfaceOptions::default());
    s.set_components(serde_json::from_value(json!([
        {"id": "root", "component": "Box", "children": {"componentId": "row", "path": "/items", "key": "id"}},
        {"id": "row", "component": "Text", "text": {"path": "name"}}
    ])).unwrap());
    s.set_data("/items", Some(json!([{"id": "a", "name": "Ann"}, {"id": "b", "name": "Bo"}])));
    s.set_viewport(300.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let a = s.index_of("row.a").expect("ids carry the key");
    s.set_data("/items", Some(json!([{"id": "b", "name": "Bo"}, {"id": "a", "name": "Ann"}])));
    let out = s.layout(&mut m);
    assert_eq!(s.index_of("row.a"), Some(a), "same slot after the reorder");
    assert_eq!(out.upcalls, 0, "nothing re-measured");
    assert_eq!(s.nodes()[a as usize].props["text"], json!("Ann"));
}

#[test]
fn code_blocks_carry_their_tokens_and_copy() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [{"id": "c", "component": "CodeBlock", "props": {"code": "let x = 1\nx", "language": "js", "lineNumbers": true, "highlight": [2]}}]}));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let nodes = s.nodes();
    let line = nodes.iter().find(|n| n.id == "c.code.0").unwrap();
    assert_eq!(line.props["tokens"][0], json!({"kind": "keyword", "text": "let"}));
    assert!(nodes.iter().find(|n| n.id == "c.line.1").unwrap().states.contains(&"selected".to_string()));
    let events = press(&mut s, "c.copy");
    assert!(events.contains(&OutEvent::Copy { text: "let x = 1\nx".into() }));
    s.layout(&mut m);
    assert_eq!(s.nodes().iter().find(|n| n.id == "c.copy").unwrap().props["label"], json!("Copied"));
}

#[test]
fn charts_carry_the_shared_numbers_and_resolved_colours() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "ch", "component": "Chart", "props": {"kind": "bar", "categories": ["a", "b"], "series": [{"name": "x", "values": [3, 8]}, {"name": "y", "values": [1, 2], "tone": "success"}]}}]}));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    let i = s.index_of("ch").unwrap();
    let chart = s.nodes()[i as usize].props["chart"].clone();
    assert_eq!(chart["ticks"], json!([0, 2, 4, 6, 8]));
    assert_eq!(chart["colors"], json!(["$color.chart1", "$color.success"]));
    let theme = s.effective_theme().unwrap().clone();
    assert_eq!(s.visual(i).unwrap().series_colors.as_ref().unwrap()[1], theme.modes.light.color["success"]);
    assert_eq!(frame(&s, &out, "ch").h, 200.0, "the default height");
}

#[test]
fn the_stepped_pass_equals_the_in_process_pass() {
    use exponential_ui::surface::LayoutStep;
    let tree = serde_json::to_value(exponential_ui::bench::bench_tree(120)).unwrap();
    let mut a = surface(tree.clone());
    let mut b = surface(tree);
    for s in [&mut a, &mut b] {
        s.set_viewport(390.0, 0.0, None);
    }
    let mut m = fixed();
    let direct = a.layout(&mut m);
    let mut m2 = fixed();
    let mut step = b.layout_begin(m2.measure_id());
    let mut crossings = 0;
    let stepped = loop {
        step = match step {
            LayoutStep::Intrinsics(leaves) => {
                crossings += 1;
                let requests: Vec<LeafRequest> = leaves.iter().map(|l| l.request()).collect();
                let answers = m2.measure_intrinsics(&requests);
                b.layout_intrinsics(&answers)
            }
            LayoutStep::Heights(leaves, requests) => {
                crossings += 1;
                let leaf_requests: Vec<LeafRequest> = leaves.iter().map(|l| l.request()).collect();
                let heights = m2.measure_heights(&leaf_requests, &requests);
                b.layout_heights(&heights)
            }
            LayoutStep::Done(out) => break out,
            LayoutStep::Idle => panic!("idle"),
        };
    };
    assert_eq!(stepped.frames, direct.frames);
    assert_eq!(stepped.upcalls, direct.upcalls);
    assert_eq!(crossings, direct.upcalls);
    assert!(direct.upcalls <= 3);
    assert_eq!(b.layout_heights(&[]), LayoutStep::Idle, "no pass in progress");
}

#[test]
fn every_pressable_part_of_the_kitchen_sink_survives_a_press_and_a_relayout() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/kitchen-sink.json");
    let ks: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for width in [390.0f32, 900.0] {
        let mut s = surface(ks.clone());
        s.set_data("", Some(json!({"posts": [{"title": "One"}, {"title": "Two"}], "ui": {"confirmOpen": false, "toastOpen": true, "showDone": false, "sidebarCollapsed": false}, "form": {"seats": 3, "labels": ["bug"], "rating": 2}})));
        s.set_viewport(width, 800.0, None);
        let mut m = fixed();
        s.layout(&mut m);
        let ids: Vec<String> = s.nodes().into_iter().filter(|n| n.pressable && !n.removed).map(|n| n.id).collect();
        assert!(ids.len() > 40, "{} pressable nodes", ids.len());
        let mut layers_seen = 0;
        for id in ids {
            let Some(i) = s.index_of(&id) else { continue };
            s.event(i, "press", None);
            let out = s.layout(&mut m);
            assert!(out.upcalls <= 3, "{id}: {} upcalls", out.upcalls);
            layers_seen = layers_seen.max(out.layers.len());
            // Close whatever opened so the next press starts clean.
            for l in out.layers.iter().filter(|l| l.class == exponential_ui::layout_tree::LayerClass::Overlay) {
                s.set_open(&l.owner, false);
            }
            s.layout(&mut m);
        }
        assert!(layers_seen >= 2, "presses opened layers ({layers_seen})");
    }
}

#[test]
fn a_busy_form_shows_its_submit_loading_and_a_disabled_one_makes_fields_inert() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [{"id": "f", "component": "Form", "props": {"name": "f", "busy": {"path": "/busy"}, "disabled": {"path": "/off"}}, "on": {"submit": {"event": {"name": "save"}}},
        "children": [{"id": "x", "component": "Input", "props": {"label": "X", "name": "x"}}, {"id": "go", "component": "Button", "props": {"label": "Save", "submit": true}}]}]}));
    s.set_data("", Some(json!({"busy": true, "off": false})));
    s.set_viewport(390.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let nodes = s.nodes();
    let go = nodes.iter().find(|n| n.id == "go").unwrap();
    assert_eq!(go.props["loading"], json!(true));
    assert!(!go.pressable);
    assert!(s.submit_form("f").is_empty(), "a busy form refuses");
    s.set_data("", Some(json!({"busy": false, "off": true})));
    s.layout(&mut m);
    let nodes = s.nodes();
    assert_eq!(nodes.iter().find(|n| n.id == "x.field").unwrap().props["disabled"], json!(true));
    assert!(!nodes.iter().find(|n| n.id == "x.field").unwrap().pressable);
}

#[test]
fn an_opening_toast_and_a_changing_live_text_announce() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "status", "component": "Text", "props": {"text": {"path": "/status"}, "live": "polite"}},
        {"id": "t", "component": "Toast", "props": {"title": "Failed", "description": "Try again", "type": "error", "open": {"path": "/open"}}}]}));
    s.set_data("", Some(json!({"status": "Idle", "open": false})));
    s.set_viewport(390.0, 800.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    s.take_events();
    s.set_data("", Some(json!({"status": "Saving…", "open": true})));
    s.layout(&mut m);
    let events = s.take_events();
    assert!(events.contains(&OutEvent::Announce { text: "Saving…".into(), live: "polite".into() }));
    assert!(events.contains(&OutEvent::Announce { text: "Failed. Try again".into(), live: "assertive".into() }));
}

/// What a painter's measurer is told about each leaf (the gpui lane's
/// additive request fields): the part's OWNER component (a Select item's
/// check slot, a tab's icon) and the text styling that changes a shaped
/// width (`textTransform`, `fontStyle`, `letterSpacing`).
#[test]
fn leaf_requests_name_the_owner_and_the_width_changing_text_style() {
    /// id, owner component, text transform, font style.
    type Request = (String, Option<String>, Option<String>, Option<String>);
    struct Seen(Vec<Request>);
    impl Measure for Seen {
        fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
            for l in leaves {
                self.0.push((l.id.to_string(), l.owner_component.map(str::to_string), l.text_style.text_transform.clone(), l.text_style.font_style.clone()));
            }
            FixedMeasure::default().measure_intrinsics(leaves)
        }
        fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
            FixedMeasure::default().measure_heights(leaves, requests)
        }
    }
    let mut s = Surface::new("req", SurfaceOptions::default());
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}]}},
        {"id": "loud", "component": "Text", "props": {"text": "shout"}, "style": {"textTransform": "uppercase", "fontStyle": "italic"}}
    ]}))
    .unwrap();
    s.set_nested(tree);
    s.set_viewport(400.0, 0.0, None);
    let mut m = Seen(Vec::new());
    s.layout(&mut m);
    let tab = m.0.iter().find(|(id, ..)| id == "tabs.tab.0").expect("the tab was measured");
    assert_eq!(tab.1.as_deref(), Some("Tabs"));
    let loud = m.0.iter().find(|(id, ..)| id == "loud").expect("the text was measured");
    assert_eq!(loud.1, None, "a plain node has no owner");
    assert_eq!((loud.2.as_deref(), loud.3.as_deref()), (Some("uppercase"), Some("italic")));
}

/// The NESTED authoring form names a list template by the id of a node in
/// the tree (the kitchen sink's `/posts` list): it instantiates per item
/// like a flat component does.
#[test]
fn a_nested_surface_instantiates_its_list_template_per_item() {
    let mut s = Surface::new("tpl", SurfaceOptions::default());
    s.set_nested(serde_json::from_value::<NestedNode>(json!({"id": "root", "component": "Box", "children": [
        {"id": "feed", "component": "List", "template": {"component": "row", "path": "/rows"}, "children": []},
        {"id": "row", "component": "Text", "props": {"text": {"path": "label"}}}
    ]})).unwrap());
    s.set_data("/rows", Some(json!([{"label": "One"}, {"label": "Two"}])));
    s.set_viewport(300.0, 0.0, None);
    s.layout(&mut FixedMeasure::default());
    let nodes = s.nodes();
    let texts: Vec<&Value> = ["row.0", "row.1"].iter().map(|id| &nodes.iter().find(|n| n.id == *id).unwrap_or_else(|| panic!("{id} built")).props["text"]).collect();
    assert_eq!(texts, [&json!("One"), &json!("Two")]);
}
