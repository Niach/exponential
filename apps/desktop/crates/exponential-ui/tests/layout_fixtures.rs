//! The layout contracts: `layout-geometry.json` (taffy's frames at 900/390,
//! LTR/RTL, reproduced EXACTLY), `overlay-geometry.json` (anchor placement
//! with flip and shift), the windowed list (10,000 rows, constant work per
//! scroll step) and the measure protocol (≤ 3 upcalls per pass).

use std::collections::HashMap;

use exponential_ui::measure::{FixedMeasure, HeightRequest, Intrinsics, LeafRequest, Measure};
use exponential_ui::overlay::{place_overlay, OverlaySide, PlaceOptions, Rect, Size, OVERLAY_OFFSET, OVERLAY_PADDING};
use exponential_ui::surface::{OutEvent, Surface, SurfaceOptions};
use exponential_ui::theme::Mode;
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture(name: &str) -> Value {
    let path = format!("{FIXTURES}/{name}");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("json")
}

fn geometry_surface(direction: &str) -> (Surface, FixedMeasure) {
    let fx = fixture("layout-geometry.json");
    let mut tree: NestedNode = serde_json::from_value(fx["surface"].clone()).expect("surface");
    tree.style.get_or_insert_with(Default::default).insert("direction".into(), json!(direction));
    let sizes: HashMap<String, (f32, f32)> = fx["measures"].as_object().unwrap().iter().map(|(id, m)| (id.clone(), (m["w"].as_f64().unwrap() as f32, m["h"].as_f64().unwrap() as f32))).collect();
    let mut surface = Surface::new("geometry", SurfaceOptions { theme: None, ..SurfaceOptions::default() });
    let outcome = surface.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    (surface, FixedMeasure::with_sizes(sizes))
}

#[test]
fn layout_geometry_every_case_reproduces_taffys_frames_exactly() {
    let fx = fixture("layout-geometry.json");
    let cases = fx["cases"].as_object().unwrap();
    assert_eq!(cases.len(), 4);
    for (name, case) in cases {
        let width = case["width"].as_f64().unwrap() as f32;
        let direction = case["direction"].as_str().unwrap();
        let (mut surface, mut measure) = geometry_surface(direction);
        surface.set_viewport(width, 0.0, None);
        let out = surface.layout(&mut measure);
        let nodes = surface.nodes();
        let expected = case["frames"].as_array().unwrap();
        assert_eq!(out.frames.len(), expected.len(), "{name}: node count");
        for (i, want) in expected.iter().enumerate() {
            let got = out.frames[i];
            let node = &nodes[got.index as usize];
            assert_eq!(node.id, want["id"].as_str().unwrap(), "{name}: order at {i}");
            for (k, v) in [("x", got.x), ("y", got.y), ("w", got.w), ("h", got.h)] {
                let e = want[k].as_f64().unwrap() as f32;
                assert!((v - e).abs() <= 0.001, "{name}: {}.{k} = {v}, expected {e}", node.id);
            }
        }
        assert!(out.upcalls <= 3, "{name}: {} upcalls", out.upcalls);
        assert_eq!(out.upcalls, 1, "{name}: fixed boxes never need a height round");
    }
}

#[test]
fn layout_geometry_rtl_mirrors_every_flow_frame() {
    for width in [390.0f32, 900.0] {
        let (mut ltr, mut m1) = geometry_surface("ltr");
        let (mut rtl, mut m2) = geometry_surface("rtl");
        ltr.set_viewport(width, 0.0, None);
        rtl.set_viewport(width, 0.0, None);
        let a = ltr.layout(&mut m1);
        let b = rtl.layout(&mut m2);
        let nodes = ltr.nodes();
        for (fa, fb) in a.frames.iter().zip(&b.frames) {
            let node = &nodes[fa.index as usize];
            if node.id == "media-badge" {
                // Physical `right: 8` stays at the parent's right edge under RTL (CSS semantics).
                let p = node.parent.unwrap();
                let pa = a.frames.iter().find(|f| f.index == p).unwrap();
                let pb = b.frames.iter().find(|f| f.index == p).unwrap();
                assert!(((pa.x + pa.w - (fa.x + fa.w)) - (pb.x + pb.w - (fb.x + fb.w))).abs() < 0.01, "{}: physical inset", node.id);
                continue;
            }
            assert!((fa.w - fb.w).abs() < 0.01, "{}: width", node.id);
            assert!((fa.y - fb.y).abs() < 0.01, "{}: y", node.id);
            // A flow node mirrors inside its parent: x' = px + (px + pw) - (x + w).
            // Physical margins and insets stay physical (CSS), so those nodes are
            // covered by the frame fixture above, not by the mirror rule.
            let style = ltr.layout_node(fa.index).unwrap().base_style.clone();
            if ["marginLeft", "marginRight", "left", "right", "position"].iter().any(|k| style.contains_key(*k)) {
                continue;
            }
            if let Some(p) = node.parent {
                let pa = a.frames.iter().find(|f| f.index == p).unwrap();
                let pb = b.frames.iter().find(|f| f.index == p).unwrap();
                let mirrored = pb.x + (pa.x + pa.w - (fa.x + fa.w));
                assert!((fb.x - mirrored).abs() < 0.01, "{}: x {} vs mirrored {}", node.id, fb.x, mirrored);
            }
        }
    }
}

#[test]
fn layout_second_pass_at_the_same_width_costs_no_upcall_and_viewport_changes_restyle_incrementally() {
    let (mut surface, mut measure) = geometry_surface("ltr");
    surface.set_viewport(900.0, 0.0, None);
    let first = surface.layout(&mut measure);
    assert_eq!(first.upcalls, 1);
    let second = surface.layout(&mut measure);
    assert_eq!(second.upcalls, 0, "the memo answers a repeat pass");
    assert_eq!(first.frames, second.frames);
    assert!(second.visual_changes.is_empty());
    surface.set_viewport(390.0, 0.0, None);
    let narrow = surface.layout(&mut measure);
    assert_eq!(narrow.upcalls, 0, "a width change re-uses the intrinsics");
    assert!(narrow.surface_height > first.surface_height);
    // Pressed → only the visual changes, geometry stays.
    let scan = surface.index_of("hdr-scan").unwrap();
    assert!(surface.set_pressed(&["hdr-scan".to_string()]));
    let pressed = surface.layout(&mut measure);
    assert_eq!(pressed.frames, narrow.frames);
    assert_eq!(pressed.visual_changes, vec![scan]);
    assert_eq!(surface.visual(scan).unwrap().opacity, Some(0.6));
}

#[test]
fn overlay_geometry_constants_and_every_case_places_as_recorded() {
    let fx = fixture("overlay-geometry.json");
    assert_eq!(fx["offset"].as_f64().unwrap(), OVERLAY_OFFSET);
    assert_eq!(fx["padding"].as_f64().unwrap(), OVERLAY_PADDING);
    let cases = fx["cases"].as_array().unwrap();
    assert!(cases.len() > 8);
    for c in cases {
        let rect = |v: &Value| Rect { x: v["x"].as_f64().unwrap(), y: v["y"].as_f64().unwrap(), width: v["width"].as_f64().unwrap(), height: v["height"].as_f64().unwrap() };
        let size = |v: &Value| Size { width: v["width"].as_f64().unwrap(), height: v["height"].as_f64().unwrap() };
        let got = place_overlay(&rect(&c["anchor"]), &size(&c["size"]), &size(&c["viewport"]), &PlaceOptions::side(OverlaySide::parse(c["side"].as_str().unwrap()).unwrap()));
        let name = c["name"].as_str().unwrap();
        assert_eq!(got.x, c["expected"]["x"].as_f64().unwrap(), "{name}: x");
        assert_eq!(got.y, c["expected"]["y"].as_f64().unwrap(), "{name}: y");
        assert_eq!(got.side.as_str(), c["expected"]["side"].as_str().unwrap(), "{name}: side");
        assert_eq!(got.flipped, c["expected"]["flipped"].as_bool().unwrap(), "{name}: flipped");
    }
}

/// A surface with one anchored popover near each viewport edge; the layer
/// manager must land where `place_overlay` says (flip at the four edges).
fn popover_surface(anchor_x: f32, anchor_y: f32, side: &str, direction: &str) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box",
        "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": direction},
        "children": [
            {"id": "pop", "component": "Popover", "props": {"side": side, "open": true},
             "style": {"position": "absolute", "left": anchor_x, "top": anchor_y},
             "slots": {"trigger": {"id": "trigger", "component": "Button", "props": {"label": "Open"}}},
             "children": [{"id": "body", "component": "Text", "props": {"text": "Twelve chars"}}]}
        ]
    })).unwrap();
    let mut surface = Surface::new("overlay", SurfaceOptions { theme: None, ..SurfaceOptions::default() });
    let outcome = surface.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    surface
}

#[test]
fn overlay_layers_flip_at_the_four_edges_at_390_and_900_ltr_and_rtl() {
    for (width, height) in [(390.0f32, 844.0f32), (900.0, 600.0)] {
        for direction in ["ltr", "rtl"] {
            for (side, ax, ay, expect_flip) in [
                ("bottom", 100.0, height - 40.0, true),
                ("top", 100.0, 4.0, true),
                ("right", width - 70.0, 200.0, true),
                ("left", 4.0, 200.0, true),
                ("bottom", 100.0, 100.0, false),
            ] {
                let mut surface = popover_surface(ax, ay, side, direction);
                surface.set_viewport(width, height, None);
                let mut measure = FixedMeasure::default();
                let out = surface.layout(&mut measure);
                assert_eq!(out.layers.len(), 1, "{width} {direction} {side}: one open layer");
                let layer = &out.layers[0];
                assert_eq!(layer.kind, "Popover");
                let placement = layer.placement.expect("anchored placement");
                assert_eq!(placement.flipped, expect_flip, "{width} {direction} {side} at ({ax},{ay}): flip");
                let anchor = layer.anchor_frame.unwrap();
                let root = layer.frames[0];
                // Where place_overlay says it lands, given the laid-out size.
                let want = place_overlay(
                    &Rect { x: anchor.x as f64, y: anchor.y as f64, width: anchor.w as f64, height: anchor.h as f64 },
                    &Size { width: root.w as f64, height: root.h as f64 },
                    &Size { width: width as f64, height: height as f64 },
                    &PlaceOptions::side(OverlaySide::parse(side).unwrap()),
                );
                assert!((root.x as f64 - want.x).abs() < 0.01 && (root.y as f64 - want.y).abs() < 0.01, "{width} {direction} {side}: frame ({},{}) vs ({},{})", root.x, root.y, want.x, want.y);
                assert!(root.x >= OVERLAY_PADDING as f32 - 0.01 && root.x + root.w <= width - OVERLAY_PADDING as f32 + 0.01, "{width} {direction} {side}: inside the viewport");
                assert!(out.frames.iter().all(|f| f.index != layer.root), "layer frames are not in the main list");
            }
        }
    }
}

#[test]
fn overlay_open_is_local_state_and_writes_through_a_binding() {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [
            {"id": "dlg", "component": "Dialog", "props": {"title": "Delete?", "open": {"path": "/ui/open"}},
             "slots": {"trigger": {"id": "t", "component": "Button", "props": {"label": "Delete"}}},
             "children": [{"id": "msg", "component": "Text", "props": {"text": "Sure?"}}]}
        ]
    })).unwrap();
    let mut surface = Surface::new("dialog", SurfaceOptions::default());
    surface.set_nested(tree);
    surface.set_viewport(600.0, 800.0, None);
    let mut measure = FixedMeasure::default();
    let closed = surface.layout(&mut measure);
    assert!(closed.layers.is_empty());
    let trigger = surface.index_of("t").unwrap();
    let events = surface.event(trigger, "press", None);
    assert!(events.iter().any(|e| matches!(e, OutEvent::DataChanged { path, value } if path == "/ui/open" && *value == json!(true))));
    assert!(events.iter().any(|e| matches!(e, OutEvent::Relayout)));
    let open = surface.layout(&mut measure);
    assert_eq!(open.layers.len(), 1);
    assert_eq!(open.layers[0].position, "centered");
    let nodes = surface.nodes();
    let title = nodes.iter().find(|n| n.id == "dlg.title").expect("title part");
    assert_eq!(title.component, "Text");
    assert_eq!(title.owner_component.as_deref(), Some("Dialog"));
    assert_eq!(surface.get_data("/ui/open"), Some(&json!(true)));
    // A producer closing it through the data model wins.
    surface.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "dialog", "path": "/ui/open", "value": false}})).unwrap();
    surface.set_open("dlg", false);
    assert!(surface.layout(&mut measure).layers.is_empty());
}

#[test]
fn tabs_selection_is_local_state_and_hides_inactive_panels() {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [{"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}], "value": "a"},
            "children": [{"id": "pa", "component": "Text", "props": {"text": "Panel A"}}, {"id": "pb", "component": "Text", "props": {"text": "Panel B is longer"}}]}]
    })).unwrap();
    let mut surface = Surface::new("tabs", SurfaceOptions::default());
    surface.set_nested(tree);
    surface.set_viewport(400.0, 0.0, None);
    let mut measure = FixedMeasure::default();
    let first = surface.layout(&mut measure);
    let nodes = surface.nodes();
    let pb = nodes.iter().find(|n| n.id == "pb").unwrap();
    assert!(pb.hidden);
    let tab_b = nodes.iter().find(|n| n.id == "tabs.tab.1").unwrap();
    assert!(tab_b.pressable);
    let events = surface.event(tab_b.index, "press", None);
    assert!(events.iter().any(|e| matches!(e, OutEvent::Relayout)));
    let second = surface.layout(&mut measure);
    let nodes = surface.nodes();
    assert!(!nodes.iter().find(|n| n.id == "pb").unwrap().hidden);
    assert!(nodes.iter().find(|n| n.id == "pa").unwrap().hidden);
    assert!(second.frames.iter().find(|f| f.index == pb.index).unwrap().w > 0.0);
    assert_eq!(first.structure_version, second.structure_version, "a selection change keeps the tree");
}

struct Counting {
    inner: FixedMeasure,
    intrinsics_calls: u32,
    heights_calls: u32,
    leaves_measured: u32,
}

impl Measure for Counting {
    fn measure_id(&self) -> u64 {
        7
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        self.intrinsics_calls += 1;
        self.leaves_measured += leaves.len() as u32;
        self.inner.measure_intrinsics(leaves)
    }
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        self.heights_calls += 1;
        self.inner.measure_heights(leaves, requests)
    }
}

#[test]
fn windowed_list_lays_out_only_the_visible_rows_and_scrolls_in_constant_work() {
    let rows: Vec<Value> = (0..10_000).map(|i| json!({"id": format!("r{i}"), "component": "Text", "props": {"text": format!("Row number {i} with some words that wrap when narrow")}})).collect();
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [{"id": "list", "component": "List", "props": {"divided": true}, "style": {"height": 600}, "children": rows}]
    })).unwrap();
    let mut surface = Surface::new("list", SurfaceOptions::default());
    surface.set_nested(tree);
    surface.set_viewport(390.0, 600.0, None);
    let mut measure = Counting { inner: FixedMeasure { sizes: HashMap::new(), wrap: true }, intrinsics_calls: 0, heights_calls: 0, leaves_measured: 0 };
    let first = surface.layout(&mut measure);
    assert_eq!(first.lists.len(), 1);
    let list = &first.lists[0];
    assert!(list.windowed);
    assert_eq!(list.count, 10_000);
    let rendered = list.end - list.start;
    assert!(rendered < 60, "rendered {rendered} rows");
    assert!(surface.node_count() < 200, "{} layout nodes", surface.node_count());
    assert!(list.content_height > 10_000.0 * 20.0);
    assert!(first.upcalls <= 3);
    let baseline_leaves = measure.leaves_measured;
    // Scroll to the middle: a bounded number of new rows, constant per step.
    let mut costs = Vec::new();
    for step in 1..=20u32 {
        let before = measure.leaves_measured;
        assert!(surface.scroll("list", step as f32 * 10_000.0));
        let out = surface.layout(&mut measure);
        let l = &out.lists[0];
        assert!(l.end > l.start && l.end <= 10_000);
        costs.push(measure.leaves_measured - before);
        assert!(out.upcalls <= 3);
    }
    let max = *costs.iter().max().unwrap();
    assert!(max <= 120, "a scroll step measured {max} leaves (baseline {baseline_leaves})");
    // A scroll back over measured rows costs nothing new.
    let before = measure.leaves_measured;
    surface.scroll("list", 10_000.0);
    surface.layout(&mut measure);
    assert_eq!(measure.leaves_measured - before, 0, "row heights are memoized per key");
}

#[test]
fn wrapping_text_takes_one_height_round_and_no_more() {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [{"id": "p", "component": "Text", "props": {"text": "a long paragraph of words that must wrap onto several lines at a narrow width to test the second phase"}}]
    })).unwrap();
    let mut surface = Surface::new("wrap", SurfaceOptions::default());
    surface.set_nested(tree);
    surface.set_viewport(200.0, 0.0, None);
    let mut measure = Counting { inner: FixedMeasure { sizes: HashMap::new(), wrap: true }, intrinsics_calls: 0, heights_calls: 0, leaves_measured: 0 };
    let out = surface.layout(&mut measure);
    assert_eq!(out.upcalls, 2, "intrinsics + heights");
    assert_eq!(measure.heights_calls, 1);
    let p = out.frames[1];
    assert_eq!(p.w, 200.0);
    assert!(p.h >= 60.0, "wrapped height {}", p.h);
    assert_eq!(surface.layout(&mut measure).upcalls, 0);
}

#[test]
fn a_theme_switch_re_resolves_visuals_and_relayouts_only_where_metrics_changed() {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": "$spacing.md"},
        "children": [
            {"id": "b", "component": "Button", "props": {"label": "Go", "variant": "default"}},
            {"id": "t", "component": "Text", "props": {"text": "Body"}}
        ]
    })).unwrap();
    let mut surface = Surface::new("theme", SurfaceOptions::default());
    surface.set_nested(tree);
    surface.set_viewport(400.0, 0.0, None);
    let mut measure = FixedMeasure::default();
    let neutral = exponential_ui::themes::builtin_theme("neutral").unwrap();
    surface.set_theme(Some(neutral.clone()));
    let a = surface.layout(&mut measure);
    let b = surface.index_of("b").unwrap();
    let primary_light = neutral.modes.light.color["primary"].clone();
    assert_eq!(surface.visual(b).unwrap().background_color.as_deref(), Some(primary_light.as_str()));
    assert_eq!(surface.visual(b).unwrap().border_radius, Some(neutral.tokens.radius["md"] as f32));
    surface.set_mode(Mode::Dark);
    let dark = surface.layout(&mut measure);
    assert_eq!(dark.frames, a.frames, "a mode switch changes no metric");
    assert!(dark.visual_changes.contains(&b));
    assert_eq!(surface.visual(b).unwrap().background_color.as_deref(), Some(neutral.modes.dark.color["primary"].as_str()));
    let playful = exponential_ui::themes::builtin_theme("playful").unwrap();
    surface.set_theme(Some(playful.clone()));
    let p = surface.layout(&mut measure);
    assert_ne!(surface.visual(b).unwrap().border_radius, Some(neutral.tokens.radius["md"] as f32));
    assert!(p.visual_changes.contains(&b));
}

#[test]
fn a2ui_messages_drive_the_surface_including_templates_and_basic_components() {
    let mut surface = Surface::new("s1", SurfaceOptions { catalog_id: exponential_ui::catalog::A2UI_BASIC_CATALOG_ID.into(), ..SurfaceOptions::default() });
    surface.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s1", "catalogId": exponential_ui::catalog::A2UI_BASIC_CATALOG_ID}})).unwrap();
    let outcome = surface.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s1", "components": [
        {"id": "root", "component": "Column", "children": ["title", "items", "btn"]},
        {"id": "title", "component": "Text", "text": {"path": "/title"}, "variant": "h2"},
        {"id": "items", "component": "List", "children": {"componentId": "row", "path": "/rows"}},
        {"id": "row", "component": "Text", "text": {"path": "name"}},
        {"id": "btn", "component": "Button", "child": "btn-label", "action": {"event": {"name": "go", "context": {"n": {"path": "/count"}}}}},
        {"id": "btn-label", "component": "Text", "text": "Go"}
    ]}})).unwrap();
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    surface.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "value": {"title": "Hello", "count": 2, "rows": [{"name": "one"}, {"name": "two"}, {"name": "three"}]}}})).unwrap();
    surface.set_viewport(400.0, 0.0, None);
    let mut measure = FixedMeasure::default();
    let out = surface.layout(&mut measure);
    let nodes = surface.nodes();
    let title = nodes.iter().find(|n| n.id == "title").unwrap();
    assert_eq!(title.props["text"], json!("Hello"));
    assert_eq!(nodes.iter().filter(|n| n.id.starts_with("row.")).count(), 3);
    assert_eq!(nodes.iter().find(|n| n.id == "row.2").unwrap().props["text"], json!("three"));
    assert!(out.surface_height > 0.0);
    let btn = surface.index_of("btn").unwrap();
    let events = surface.event(btn, "press", None);
    assert!(events.iter().any(|e| matches!(e, OutEvent::Action { name, context, .. } if name == "go" && context["n"] == json!(2))));
    // A data change re-resolves bindings without rebuilding the taffy tree.
    let v1 = out.structure_version;
    surface.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/title", "value": "Changed"}})).unwrap();
    let out2 = surface.layout(&mut measure);
    assert_eq!(out2.structure_version, v1);
    assert_eq!(surface.nodes().iter().find(|n| n.id == "title").unwrap().props["text"], json!("Changed"));
    // Adding a row changes the structure.
    surface.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/rows/-", "value": {"name": "four"}}})).unwrap();
    let out3 = surface.layout(&mut measure);
    assert_eq!(out3.structure_version, v1 + 1);
    surface.apply(&json!({"version": "v0.9", "deleteSurface": {"surfaceId": "s1"}})).unwrap();
    assert_eq!(surface.layout(&mut measure).frames.len(), 0);
}

#[test]
fn extension_natives_arrive_as_extension_leaves_the_host_paints() {
    let fx = fixture("catalog-extension.json");
    let ext = exponential_ui::extension::parse_extension(&fx["extension"].to_string()).unwrap();
    let mut surface = Surface::new("ext", SurfaceOptions { catalog_id: ext.id.clone(), extensions: vec![ext], ..SurfaceOptions::default() });
    let case = &fx["cases"][0];
    let components: Vec<exponential_ui::types::FlatComponent> = serde_json::from_value(case["components"].clone()).unwrap();
    let outcome = surface.set_components(components);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    surface.set_viewport(400.0, 0.0, None);
    let mut measure = FixedMeasure::default();
    let out = surface.layout(&mut measure);
    let nodes = surface.nodes();
    let spark = nodes.iter().find(|n| n.component == "Extension").expect("an Extension leaf");
    assert_eq!(spark.extension_kind.as_deref(), Some("Sparkline"));
    assert_eq!(spark.catalog_id.as_deref(), Some("https://ui.exponential.at/catalogs/example/v1"));
    assert!(out.frames.iter().any(|f| f.index == spark.index));
    assert!(nodes.iter().all(|n| n.component != "StatCard"), "the extension macro expanded");
}

#[test]
fn kitchen_sink_lays_out_with_the_default_theme_in_three_upcalls_or_fewer() {
    let ks = fixture("kitchen-sink.json");
    let tree: NestedNode = serde_json::from_value(ks).unwrap();
    let mut surface = Surface::new("ks", SurfaceOptions::default());
    let outcome = surface.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    surface.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "ks", "value": {"posts": [{"title": "One"}, {"title": "Two"}], "ui": {"confirmOpen": false}}}})).unwrap();
    for width in [390.0f32, 900.0] {
        surface.set_viewport(width, 0.0, None);
        let mut measure = FixedMeasure { sizes: HashMap::new(), wrap: true };
        let out = surface.layout(&mut measure);
        assert!(out.upcalls <= 3, "{width}: {} upcalls", out.upcalls);
        assert!(out.surface_height > 400.0, "{width}: height {}", out.surface_height);
        assert!(out.frames.len() > 100);
        let again = surface.layout(&mut measure);
        assert_eq!(again.upcalls, 0);
    }
    let nodes = surface.nodes();
    assert!(nodes.iter().any(|n| n.owner_component.as_deref() == Some("Tabs") && n.part.as_deref() == Some("tab")));
    assert!(nodes.iter().any(|n| n.owner_component.as_deref() == Some("Input") && n.part.as_deref() == Some("field")));
}
