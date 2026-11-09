//! The facade round trip with a Rust `Measurer` (what Swift/Kotlin implement),
//! and the shared fixtures replayed through the free functions exactly as
//! the Swift and Kotlin suites do (tests/swift, tests/kotlin).

use std::sync::{Arc, Mutex};

use exponential_ui_ffi::*;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/{name}")).expect("fixture")).expect("json")
}

struct Counting {
    intrinsics: Mutex<u32>,
    heights: Mutex<u32>,
}

impl Measurer for Counting {
    fn measure_id(&self) -> u64 {
        3
    }
    fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics> {
        *self.intrinsics.lock().unwrap() += 1;
        leaves
            .iter()
            .map(|l| {
                let chars = l.text.chars().count() as f32;
                let w = l.control.width.unwrap_or(8.0 * chars + 2.0 * l.control.padding_horizontal);
                let h = l.control.height.unwrap_or(l.text_style.line_height + 2.0 * l.control.padding_vertical);
                let min = if l.component == "Text" { l.text.split_whitespace().map(|w| w.chars().count() as f32 * 8.0).fold(0.0, f32::max) } else { w };
                FfiIntrinsics { min_content_width: min, max_content_width: w, height_at_max_content: h, baseline: None }
            })
            .collect()
    }
    fn measure_heights(&self, leaves: Vec<FfiLeaf>, requests: Vec<FfiHeightRequest>) -> Vec<f32> {
        *self.heights.lock().unwrap() += 1;
        requests
            .iter()
            .map(|r| {
                let l = leaves.iter().find(|l| l.index == r.index).unwrap();
                let per_line = (r.width / 8.0).floor().max(1.0);
                let lines = (l.text.chars().count() as f32 / per_line).ceil().max(1.0);
                lines * l.text_style.line_height
            })
            .collect()
    }
}

#[test]
fn a_surface_lays_out_through_the_foreign_measurer_in_three_upcalls_or_fewer() {
    let surface = Surface::new("bench".into(), core_catalog_id(), None, "light".into()).unwrap();
    let outcome = surface.set_nested(bench_tree_json(200)).unwrap();
    assert_eq!(outcome.issues_json, "[]");
    assert!(surface.node_count() >= 200);
    surface.set_viewport(390.0, 0.0, None);
    let m = Arc::new(Counting { intrinsics: Mutex::new(0), heights: Mutex::new(0) });
    let out = surface.layout(m.clone());
    assert!(out.upcalls <= 3, "{} upcalls", out.upcalls);
    assert_eq!(out.upcalls, *m.intrinsics.lock().unwrap() + *m.heights.lock().unwrap());
    assert_eq!(out.frames.len() as u32, surface.node_count());
    assert!(out.surface_height > 1000.0);
    let nodes = surface.nodes();
    assert_eq!(nodes[0].id, "root");
    assert!(nodes.iter().any(|n| n.component == "Button" && n.is_leaf));
    let visuals = surface.visuals();
    assert_eq!(visuals.len(), nodes.len());
    let again = surface.layout(m.clone());
    assert_eq!(again.upcalls, 0);
    // Pressing a button fires nothing (no handler) but is harmless; a tab press relayouts.
    let btn = nodes.iter().find(|n| n.component == "Button").unwrap();
    assert!(surface.event(btn.index, "press".into(), None).unwrap().is_empty());
}

#[test]
fn the_catalog_fixtures_replay_through_the_facade() {
    let macros = fixture("catalog-macros.json");
    for c in macros["cases"].as_array().unwrap() {
        let got = reduce_nested_json(c["input"].to_string(), core_catalog_id(), None).unwrap();
        let expected = json!({"root": c["expected"], "issues": []}).to_string();
        assert!(json_equal(got.clone(), expected.clone()), "{}: {}", c["name"], json_diff(got, expected));
    }
    let basic = fixture("catalog-basic-map.json");
    for c in basic["cases"].as_array().unwrap() {
        let got = reduce_surface_json(c["components"].to_string(), basic_catalog_id(), None).unwrap();
        assert!(json_equal(got.clone(), c["expected"].to_string()), "{}: {}", c["name"], json_diff(got, c["expected"].to_string()));
    }
    let ext = fixture("catalog-extension.json");
    assert!(extension_errors(ext["extension"].to_string()).unwrap().is_empty());
    for c in ext["cases"].as_array().unwrap() {
        let got = reduce_surface_json(c["components"].to_string(), c["catalogId"].as_str().unwrap().into(), Some(json!([ext["extension"]]).to_string())).unwrap();
        assert!(json_equal(got.clone(), c["expected"].to_string()), "{}: {}", c["name"], json_diff(got, c["expected"].to_string()));
    }
    let components = fixture("catalog-components.json");
    for c in components["cases"].as_array().unwrap() {
        let got: Value = serde_json::from_str(&reduce_nested_json(c["node"].to_string(), core_catalog_id(), None).unwrap()).unwrap();
        assert_eq!(got["issues"], json!([]), "{}", c["name"]);
    }
}

#[test]
fn the_theme_fixtures_replay_through_the_facade() {
    let resolved = fixture("theme-resolved.json");
    for (id, expected) in resolved["themes"].as_object().unwrap() {
        let got = builtin_theme_json(id.clone()).unwrap();
        assert!(json_equal(got.clone(), expected.to_string()), "{id}: {}", json_diff(got, expected.to_string()));
    }
    let recipes = fixture("theme-recipes.json");
    let themes: std::collections::HashMap<String, String> = builtin_theme_ids().into_iter().map(|id| (id.clone(), builtin_theme_json(id).unwrap())).collect();
    for c in recipes["cases"].as_array().unwrap() {
        for (theme_id, by_mode) in c["visuals"].as_object().unwrap() {
            for (mode, by_state) in by_mode.as_object().unwrap() {
                for (state, style) in by_state.as_object().unwrap() {
                    let states = if state == "default" { vec![] } else { vec![state.clone()] };
                    let got = resolve_recipe_json(themes[theme_id].clone(), c["component"].as_str().unwrap().into(), c["part"].as_str().unwrap().into(), c["props"].to_string(), states, mode.clone()).unwrap();
                    assert!(json_equal(got.clone(), style.to_string()), "{theme_id}/{mode}/{state} {}/{}: {}", c["component"], c["part"], json_diff(got, style.to_string()));
                }
            }
        }
    }
    let extends = fixture("theme-extends.json");
    for c in extends["cases"].as_array().unwrap() {
        let theme = load_theme_json(c["theme"].to_string(), None).unwrap();
        let parsed: Value = serde_json::from_str(&theme).unwrap();
        assert_eq!(parsed["chain"], c["expected"]["chain"], "{}", c["name"]);
        for p in c["expected"]["probes"].as_array().unwrap() {
            let states: Vec<String> = p.get("states").and_then(Value::as_array).map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect()).unwrap_or_default();
            let got = resolve_recipe_json(theme.clone(), p["component"].as_str().unwrap().into(), p["part"].as_str().unwrap().into(), p["props"].to_string(), states, p["mode"].as_str().unwrap().into()).unwrap();
            assert!(json_equal(got.clone(), p["style"].to_string()), "{}: {}", c["name"], json_diff(got, p["style"].to_string()));
        }
    }
    let invalid = fixture("theme-invalid.json");
    for c in invalid["cases"].as_array().unwrap() {
        let issues = theme_issues_json(c["theme"].to_string(), None);
        assert!(json_equal(issues.clone(), c["issues"].to_string()), "{}: {}", c["name"], json_diff(issues, c["issues"].to_string()));
    }
    let geometry = fixture("control-geometry.json");
    for (theme_id, by_component) in geometry["themes"].as_object().unwrap() {
        for (component, entry) in by_component.as_object().unwrap() {
            for (name, case) in entry["cases"].as_object().unwrap() {
                let got = control_geometry_json(themes[theme_id].clone(), component.clone(), case["props"].to_string()).unwrap();
                assert!(json_equal(got.clone(), case["geometry"].to_string()), "{theme_id} {component} {name}: {}", json_diff(got, case["geometry"].to_string()));
            }
        }
    }
}

#[test]
fn the_layout_and_overlay_fixtures_replay_through_the_facade() {
    let overlay = fixture("overlay-geometry.json");
    for c in overlay["cases"].as_array().unwrap() {
        let a = &c["anchor"];
        let s = &c["size"];
        let v = &c["viewport"];
        let p = place_overlay_aligned(a["x"].as_f64().unwrap(), a["y"].as_f64().unwrap(), a["width"].as_f64().unwrap(), a["height"].as_f64().unwrap(), s["width"].as_f64().unwrap(), s["height"].as_f64().unwrap(), v["width"].as_f64().unwrap(), v["height"].as_f64().unwrap(), c["side"].as_str().unwrap().into(), c["align"].as_str().unwrap_or("center").into()).unwrap();
        if c.get("align").is_none() {
            // The side-only entry point = centre alignment.
            let q = place_overlay(a["x"].as_f64().unwrap(), a["y"].as_f64().unwrap(), a["width"].as_f64().unwrap(), a["height"].as_f64().unwrap(), s["width"].as_f64().unwrap(), s["height"].as_f64().unwrap(), v["width"].as_f64().unwrap(), v["height"].as_f64().unwrap(), c["side"].as_str().unwrap().into()).unwrap();
            assert_eq!((q.x, q.y), (p.x, p.y));
        }
        assert_eq!((p.x, p.y, p.side.as_str(), p.flipped), (c["expected"]["x"].as_f64().unwrap(), c["expected"]["y"].as_f64().unwrap(), c["expected"]["side"].as_str().unwrap(), c["expected"]["flipped"].as_bool().unwrap()), "{}", c["name"]);
    }
    let layout = fixture("layout-geometry.json");
    for (name, case) in layout["cases"].as_object().unwrap() {
        let mut tree = layout["surface"].clone();
        tree["style"]["direction"] = case["direction"].clone();
        let surface = Surface::new("geometry".into(), core_catalog_id(), Some("".into()), "light".into()).unwrap();
        assert_eq!(surface.set_nested(tree.to_string()).unwrap().issues_json, "[]");
        surface.set_viewport(case["width"].as_f64().unwrap() as f32, 0.0, None);
        let out = surface.layout_fixed(Some(layout["measures"].to_string()), false).unwrap();
        let nodes = surface.nodes();
        let expected = case["frames"].as_array().unwrap();
        assert_eq!(out.frames.len(), expected.len(), "{name}");
        for (i, want) in expected.iter().enumerate() {
            let f = out.frames[i];
            assert_eq!(nodes[f.index as usize].id, want["id"].as_str().unwrap(), "{name} order");
            for (k, v) in [("x", f.x), ("y", f.y), ("w", f.w), ("h", f.h)] {
                assert!((v - want[k].as_f64().unwrap() as f32).abs() <= 0.001, "{name}: {}.{k}", want["id"]);
            }
        }
        assert_eq!(out.upcalls, 1);
    }
}

#[test]
fn messages_events_and_overlays_cross_the_facade_as_json() {
    let surface = Surface::new("s".into(), core_catalog_id(), None, "dark".into()).unwrap();
    surface.apply(json!({"version": "1.0", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": ["dlg", "t"]},
        {"id": "t", "component": "Text", "text": {"@path": "/title"}},
        {"id": "dlg", "component": "Dialog", "title": "Hi", "open": {"@path": "/open"}, "slots": {"trigger": "b"}, "children": ["body"]},
        {"id": "b", "component": "Button", "label": "Open", "on": {"press": {"event": {"name": "opened"}}}},
        {"id": "body", "component": "Text", "text": "Body"}
    ]}}).to_string()).unwrap();
    surface.set_data("/title".into(), Some("\"Hello\"".into())).unwrap();
    surface.set_viewport(400.0, 700.0, None);
    assert!(surface.set_layer_viewport(1000.0, 800.0), "VAPP-253: the layer viewport goes through");
    assert!(surface.set_layer_viewport(0.0, 0.0), "and unsets");
    assert!(surface.set_popup_bounds(-10.0, -200.0, 390.0, 844.0), "#EXP-1629: the popup bounds go through");
    assert!(!surface.set_popup_bounds(-10.0, -200.0, 390.0, 844.0), "unchanged");
    assert!(surface.set_popup_bounds(0.0, 0.0, 0.0, 0.0), "and unset");
    let out = surface.layout_fixed(None, true).unwrap();
    assert!(out.layers.is_empty());
    let b = surface.index_of("b".into()).unwrap();
    let events = surface.event(b, "press".into(), None).unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"action") && kinds.contains(&"dataChanged") && kinds.contains(&"relayout"), "{kinds:?}");
    let out = surface.layout_fixed(None, true).unwrap();
    assert_eq!(out.layers.len(), 1);
    assert_eq!(out.layers[0].position, "centered");
    assert_eq!(serde_json::from_str::<Value>(&surface.data_json()).unwrap()["open"], json!(true));
    surface.set_builtin_theme("playful".into()).unwrap();
    assert!(surface.set_theme_json(json!({"$schema": "https://ui.exponential.at/schemas/theme/v1.json", "id": "x", "name": "X", "extends": "neutral"}).to_string()).is_ok());
    assert!(matches!(surface.set_theme_json("{\"id\":\"bad\"}".into()), Err(UiError::Theme { .. })));
}

/// A measurer that reads the surface (and re-enters `layout`) from inside
/// its upcall: the facade holds no lock across it.
struct Peeking {
    surface: Mutex<Option<Arc<Surface>>>,
    peeked: Mutex<usize>,
    reentrant: Mutex<bool>,
}

impl Measurer for Peeking {
    fn measure_id(&self) -> u64 {
        9
    }
    fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics> {
        if let Some(s) = self.surface.lock().unwrap().clone() {
            *self.peeked.lock().unwrap() += s.nodes().len();
            *self.reentrant.lock().unwrap() = s.layout_fixed(None, false).map(|l| l.reentrant).unwrap_or(false);
        }
        leaves.iter().map(|l| FfiIntrinsics { min_content_width: 10.0, max_content_width: 8.0 * l.text.chars().count() as f32, height_at_max_content: 20.0, baseline: Some(15.0) }).collect()
    }
    fn measure_heights(&self, _leaves: Vec<FfiLeaf>, requests: Vec<FfiHeightRequest>) -> Vec<f32> {
        requests.iter().map(|_| 40.0).collect()
    }
}

#[test]
fn the_measurer_runs_without_the_surface_locked() {
    let surface = Surface::new("p".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_nested(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "baseline"}, "children": [
        {"id": "a", "component": "Text", "props": {"text": "Alpha"}}, {"id": "b", "component": "Text", "props": {"text": "Beta gamma delta"}}]}).to_string()).unwrap();
    surface.set_viewport(300.0, 0.0, None);
    let m = Arc::new(Peeking { surface: Mutex::new(Some(surface.clone())), peeked: Mutex::new(0), reentrant: Mutex::new(false) });
    let out = surface.layout(m.clone());
    assert!(!out.reentrant);
    assert!(*m.peeked.lock().unwrap() > 0, "nodes() answered from inside the upcall");
    assert!(*m.reentrant.lock().unwrap(), "a nested layout_fixed returned instead of deadlocking");
    // A layout re-entered from its own measurer returns the previous result.
    struct Reenter(Mutex<Option<Arc<Surface>>>, Mutex<Option<bool>>);
    impl Measurer for Reenter {
        fn measure_id(&self) -> u64 {
            10
        }
        fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics> {
            if let Some(s) = self.0.lock().unwrap().take() {
                let again = s.layout(Arc::new(Reenter(Mutex::new(None), Mutex::new(None))));
                *self.1.lock().unwrap() = Some(again.reentrant);
            }
            leaves.iter().map(|_| FfiIntrinsics { min_content_width: 10.0, max_content_width: 10.0, height_at_max_content: 20.0, baseline: None }).collect()
        }
        fn measure_heights(&self, _l: Vec<FfiLeaf>, r: Vec<FfiHeightRequest>) -> Vec<f32> {
            r.iter().map(|_| 20.0).collect()
        }
    }
    surface.invalidate_measures();
    let r = Arc::new(Reenter(Mutex::new(Some(surface.clone())), Mutex::new(None)));
    let out = surface.layout(r.clone());
    assert_eq!(*r.1.lock().unwrap(), Some(true));
    assert!(!out.reentrant && !out.frames.is_empty());
    m.surface.lock().unwrap().take();
}

#[test]
fn settings_deltas_scrolls_and_commands_cross_the_facade() {
    let surface = Surface::new("s".into(), core_catalog_id(), None, "light".into()).unwrap();
    let rows: Vec<Value> = (0..300).map(|i| json!({"id": format!("r{i}"), "component": "Text", "props": {"text": format!("Row {i}")}})).collect();
    surface.set_nested(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [
        {"id": "list", "component": "List", "style": {"height": 400}, "children": rows},
        {"id": "t", "component": "Toast", "props": {"title": "Saved", "open": true, "duration": 3000}}]}).to_string()).unwrap();
    surface.set_viewport(390.0, 844.0, None);
    let mut settings = surface.settings();
    assert_eq!(settings.locale, "en-US");
    settings.mode = "system".into();
    settings.system_dark = true;
    settings.font_scale = 1.25;
    settings.inset_bottom = 34.0;
    settings.strings_json = json!({"dismiss": "Schließen"}).to_string();
    surface.set_settings(settings).unwrap();
    assert!(surface.set_settings(FfiSettings { mode: "dim".into(), ..surface.settings() }).is_err());
    let first = surface.layout_fixed(None, true).unwrap();
    assert_eq!(first.toasts.len(), 1);
    assert_eq!(first.toasts[0].duration_ms, 3000.0);
    let toast = first.layers.iter().find(|l| l.class == "toast").unwrap();
    assert!(toast.frames[0].y + toast.frames[0].h <= 844.0 - 34.0 - 8.0 + 0.01, "above the home indicator");
    let nodes = surface.nodes();
    assert!(nodes.iter().any(|n| n.id == "t.close" && serde_json::from_str::<Value>(&n.props_json).unwrap()["label"] == json!("Schließen")));
    surface.layout_fixed(None, true).unwrap();
    assert!(surface.scroll("list".into(), 4000.0));
    let scrolled = surface.layout_fixed(None, true).unwrap();
    assert!(!scrolled.delta.added.is_empty() && !scrolled.delta.removed.is_empty());
    let patched = surface.nodes_at(scrolled.delta.added.clone());
    assert_eq!(patched.len(), scrolled.delta.added.len());
    // VAPP-131 / VAPP-138: paint_at = visual + a leaf's text_style, in order,
    // a slot past the end left out.
    let mut asked: Vec<u32> = scrolled.delta.added.iter().chain(&scrolled.delta.changed).rev().copied().collect();
    asked.push(u32::MAX);
    let paints = surface.paint_at(asked.clone());
    assert_eq!(paints.iter().map(|p| p.index).collect::<Vec<_>>(), asked[..asked.len() - 1]);
    for p in &paints {
        assert_eq!(format!("{:?}", p.visual), format!("{:?}", surface.visual(p.index).unwrap()));
        let leaf = surface.nodes_at(vec![p.index])[0].is_leaf;
        assert_eq!(format!("{:?}", p.text_style), format!("{:?}", if leaf { surface.text_style(p.index) } else { None }));
    }
    assert!(paints.iter().any(|p| p.text_style.is_some()), "the rows' text styles came along");
    assert!(scrolled.scrolls.iter().any(|s| s.scroll_y && s.offset_y == 4000.0));
    let events = surface.command_json(json!({"announce": {"text": "Hi", "live": "polite"}}).to_string()).unwrap();
    assert_eq!(events[0].kind, "announce");
    let dismissed = surface.dismiss_toast("t".into());
    assert!(dismissed.iter().any(|e| e.kind == "relayout"));
    assert!(surface.layout_fixed(None, true).unwrap().toasts.is_empty());
}

#[test]
fn the_round_1_fixtures_replay_through_the_free_functions() {
    let bind = fixture("bind-time.json");
    for c in bind["cases"].as_array().unwrap() {
        for d in c["datasets"].as_array().unwrap() {
            let got = bind_tree_json(c["expanded"].to_string(), d["data"].to_string(), String::new(), None).unwrap();
            let got = got.map(|g| serde_json::from_str::<Value>(&g).unwrap()).unwrap_or(Value::Null);
            assert!(json_equal(got.to_string(), d["bound"].to_string()), "{}: {}", c["name"], json_diff(got.to_string(), d["bound"].to_string()));
            for p in d["presses"].as_array().unwrap() {
                fn find<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
                    if n["id"] == id {
                        return Some(n);
                    }
                    n["slots"].as_object().into_iter().flat_map(|s| s.values()).chain(n["children"].as_array().into_iter().flatten()).find_map(|c| find(c, id))
                }
                let node = find(&c["expanded"], p["id"].as_str().unwrap()).unwrap();
                let got = run_action_json(node["on"]["press"].to_string(), d["data"].to_string(), String::new(), None).unwrap();
                assert!(json_equal(got.clone(), p["outcome"].to_string()), "{} {}: {}", c["name"], p["id"], json_diff(got, p["outcome"].to_string()));
            }
        }
    }
    let code = fixture("code-tokens.json");
    for c in code["cases"].as_array().unwrap() {
        let got = tokenize_code_json(c["code"].as_str().unwrap().into(), c["language"].as_str().unwrap().into());
        assert!(json_equal(got.clone(), c["expected"].to_string()), "{}: {}", c["name"], json_diff(got, c["expected"].to_string()));
    }
    assert_eq!(code_text("\u{1b}[32mok\u{1b}[0m\r\n".into(), "ansi".into()), "ok\n");
    let shimmer = fixture("text-shimmer.json");
    for c in shimmer["cases"].as_array().unwrap() {
        let got = text_shimmer_json(c["color"].as_str().unwrap().into(), c["dark"].as_bool().unwrap());
        assert!(json_equal(got.clone(), c["expected"].to_string()), "{}: {}", c["name"], json_diff(got, c["expected"].to_string()));
    }
    let conditions = fixture("style-conditions.json");
    for c in conditions["cases"].as_array().unwrap() {
        for (ctx, expected) in c["contexts"].as_array().unwrap().iter().zip(c["expected"].as_array().unwrap()) {
            let mut ctx = ctx.clone();
            if ctx.get("breakpoints").is_none() {
                ctx["breakpoints"] = conditions["breakpoints"].clone();
            }
            let got = resolve_conditions_json(c["style"].to_string(), ctx.to_string()).unwrap();
            assert!(json_equal(got.clone(), expected.to_string()), "{}: {}", c["name"], json_diff(got, expected.to_string()));
        }
    }
    assert_eq!(week_start("en-US".into()), 0);
    assert_eq!(text_direction("he".into()), "rtl");
    assert!(nice_ticks_json(0.0, 8.0, 5).contains("\"step\":2"));
    assert_eq!(format_string("Page {page} of {total}".into(), json!({"page": 2, "total": 5}).to_string()).unwrap(), "Page 2 of 5");
    assert!(component_a11y_json("Select".into()).unwrap().contains("combobox"));
}

/// A host measurer that throws (a Kotlin/Swift exception in a callback is a
/// panic on the Rust side) must not leave the surface stuck in a pass: the
/// next `layout` runs normally instead of answering `reentrant` forever.
/// Every other non-throwing call shields its body the same way.
#[test]
fn a_panicking_measurer_does_not_freeze_the_surface() {
    struct Throwing(Mutex<u32>);
    impl Measurer for Throwing {
        fn measure_id(&self) -> u64 {
            11
        }
        fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics> {
            let mut calls = self.0.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                drop(calls);
                panic!("the host measurer threw");
            }
            leaves.iter().map(|_| FfiIntrinsics { min_content_width: 10.0, max_content_width: 40.0, height_at_max_content: 20.0, baseline: None }).collect()
        }
        fn measure_heights(&self, _l: Vec<FfiLeaf>, r: Vec<FfiHeightRequest>) -> Vec<f32> {
            r.iter().map(|_| 20.0).collect()
        }
    }
    let surface = Surface::new("x".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_nested(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [{"id": "a", "component": "Text", "props": {"text": "Alpha"}}]}).to_string()).unwrap();
    surface.set_viewport(300.0, 0.0, None);
    let m = Arc::new(Throwing(Mutex::new(0)));
    let s2 = surface.clone();
    let m2 = m.clone();
    // VAPP-103 rfix: `layout` cannot throw, so a panic must not cross the
    // FFI (Swift `try!` crashes): the last layout comes back `reentrant`
    // and the panic is an issue of the surface.
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || s2.layout(m2)));
    let out = caught.expect("the panic stays on the Rust side");
    assert!(out.reentrant && out.frames.is_empty(), "nothing new to paint");
    let issues: Value = serde_json::from_str(&surface.issues_json()).unwrap();
    assert!(issues.as_array().unwrap().iter().any(|i| i["id"] == "ffi.layout" && i["message"].as_str().unwrap().contains("the host measurer threw")), "{issues}");
    for _ in 0..2 {
        let out = surface.layout(m.clone());
        assert!(!out.reentrant, "not mistaken for a re-entrant call");
        assert!(!out.frames.is_empty() && out.surface_height >= 20.0, "a real layout: {} frames", out.frames.len());
    }
}

/// VAPP-107: a throwing call never unwinds across the FFI either: a host
/// callback that panics inside a `Result` entry answers the typed
/// `UiError::Panic` (Swift `UiError.Panic`, Kotlin `UiException.Panic`), the
/// surface records it as an issue and stays usable.
#[test]
fn a_panic_inside_a_throwing_call_is_a_typed_error() {
    struct Boom;
    impl HostFormatter for Boom {
        fn locale(&self) -> String {
            "x-boom".into()
        }
        fn number(&self, _: f64, _: Option<u32>, _: bool) -> String {
            panic!("the host formatter threw")
        }
        fn currency(&self, _: f64, _: String, _: Option<u32>, _: bool) -> String {
            panic!("the host formatter threw")
        }
        fn percent(&self, _: f64, _: Option<u32>) -> String {
            panic!("the host formatter threw")
        }
        fn date(&self, _: f64, _: bool, _: Option<String>, _: Option<String>, _: bool) -> String {
            panic!("the host formatter threw")
        }
        fn relative_time(&self, _: i64, _: String) -> String {
            panic!("the host formatter threw")
        }
        fn plural(&self, _: f64) -> String {
            panic!("the host formatter threw")
        }
        fn bytes(&self, _: f64, _: String) -> String {
            panic!("the host formatter threw")
        }
    }
    let call = json!({"@call": "formatNumber", "args": {"value": 1234.5}}).to_string();
    let err = std::panic::catch_unwind(|| format_call_json(call.clone(), Some(Arc::new(Boom)), None, None)).expect("the panic stays on the Rust side").unwrap_err();
    assert!(matches!(&err, UiError::Panic { reason } if reason.contains("the host formatter threw")), "{err}");
    let next = format_call_json(call, Some(Arc::new(Loud)), None, None).unwrap();
    assert!(next.as_deref().is_some_and(|s| s.contains("1234.5")), "the next call is fine: {next:?}");
    // A surface method: the error AND an issue of the surface, which stays usable.
    let surface = Surface::new("p".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_formatter(Some(Arc::new(Boom)));
    let tree = json!({"id": "root", "component": "Box", "children": [{"id": "n", "component": "Text", "props": {"text": {"@call": "formatNumber", "args": {"value": 1}}}}]}).to_string();
    surface.set_viewport(300.0, 0.0, None);
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| surface.set_nested(tree.clone()).and_then(|_| surface.layout_fixed(None, true)))).expect("the panic stays on the Rust side");
    assert!(matches!(&first, Err(UiError::Panic { reason }) if reason.contains("the host formatter threw")), "{first:?}");
    let issues: Value = serde_json::from_str(&surface.issues_json()).unwrap();
    assert!(issues.as_array().unwrap().iter().any(|i| i["id"].as_str().unwrap().starts_with("ffi.") && i["message"].as_str().unwrap().contains("the host formatter threw")), "{issues}");
    surface.set_formatter(Some(Arc::new(Loud)));
    surface.set_nested(tree).unwrap();
    assert!(!surface.layout_fixed(None, true).unwrap().frames.is_empty(), "usable after the panic");
}

#[test]
fn a_hover_card_closes_through_the_hover_timer() {
    let surface = Surface::new("h".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_nested(json!({"id": "root", "component": "Box", "children": [
        {"id": "hc", "component": "Popover", "props": {"openOn": "hover"}, "slots": {"trigger": {"id": "who", "component": "Text", "props": {"text": "@ada"}}}, "children": [{"id": "card", "component": "Text", "props": {"text": "Ada"}}]}]}).to_string()).unwrap();
    surface.set_viewport(400.0, 600.0, None);
    assert_eq!(surface.settings().hover_close_ms, 0, "off by default (the host delays)");
    surface.set_settings(FfiSettings { hover_close_ms: 150, ..surface.settings() }).unwrap();
    surface.layout_fixed(None, true).unwrap();
    surface.set_states("who".into(), vec!["hover".into()]);
    assert_eq!(surface.layout_fixed(None, true).unwrap().layers.len(), 1);
    surface.take_events();
    surface.set_states("who".into(), vec![]);
    let timer = surface.take_events().into_iter().find(|e| e.kind == "hoverTimer").expect("a hover timer");
    let t: Value = serde_json::from_str(&timer.json).unwrap();
    assert_eq!(t["owner"], json!("hc"));
    assert_eq!(t["delayMs"], json!(150));
    assert_eq!(surface.layout_fixed(None, true).unwrap().layers.len(), 1, "still open until the timer fires");
    assert!(surface.hover_timeout("hc".into()).iter().any(|e| e.kind == "relayout"));
    assert!(surface.layout_fixed(None, true).unwrap().layers.is_empty());
}

#[test]
fn a_theme_object_resolves_parts_colors_and_tokens_for_painters() {
    // VAPP-88: the SwiftUI / Compose painters query sub-parts the core does
    // not synthesize through a `Theme` object instead of re-parsing the
    // resolved theme JSON per call.
    let theme = Theme::builtin("exponential".into()).unwrap();
    assert_eq!(theme.id(), "exponential");
    let on = theme.resolve_part("Checkbox".into(), "box".into(), r#"{"checked":true}"#.into(), vec!["checked".into()], "dark".into()).unwrap();
    let off = theme.resolve_part("Checkbox".into(), "box".into(), r#"{"checked":false}"#.into(), vec![], "dark".into()).unwrap();
    assert_ne!(on.visual.background_color, off.visual.background_color);
    let style: Value = serde_json::from_str(&on.style_json).unwrap();
    assert!(style.get("width").is_some(), "the flat style map carries the geometry keys");
    let trigger = theme.resolve_part("Select".into(), "trigger".into(), r#"{"options":[]}"#.into(), vec![], "light".into()).unwrap();
    assert_eq!(trigger.visual.border_width, Some(1.0));
    assert!(theme.color("foreground".into(), "dark".into()).unwrap().starts_with('#'));
    assert!(theme.color("foreground".into(), "sideways".into()).is_none());
    assert_eq!(theme.spacing("sm".into()), Some(8.0));
    assert_eq!(theme.font_family("sans".into()).as_deref(), Some("Inter"));
    assert!(theme.control("input".into()).is_some());
    assert_eq!(theme.control("nope".into()), None);
    let loaded = Theme::load(r##"{"$schema":"https://ui.exponential.at/schemas/theme/v1.json","id":"t","name":"T","extends":"neutral","modes":{"light":{"color":{"primary":"#ff0000"}},"dark":{"color":{"primary":"#00ff00"}}}}"##.into(), None).unwrap();
    assert_eq!(loaded.color("primary".into(), "light".into()).as_deref(), Some("#ff0000"));
    // A surface built on the object paints with it and hands it back.
    let s = Surface::with_theme("s".into(), core_catalog_id(), Some(theme.clone()), "dark".into()).unwrap();
    assert_eq!(s.mode(), "dark");
    assert_eq!(s.theme().unwrap().id(), "exponential");
    s.set_theme(loaded);
    assert_eq!(s.theme().unwrap().id(), "t");
    let geometry = Surface::with_theme("g".into(), core_catalog_id(), None, "light".into()).unwrap();
    assert!(geometry.theme().is_none());
}

#[test]
fn nodes_carry_the_part_states_and_the_macro_name() {
    let s = Surface::new("s".into(), core_catalog_id(), None, "dark".into()).unwrap();
    s.set_nested(
        serde_json::json!({
            "id": "root", "component": "Card", "props": {"title": "T"},
            "children": [{"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}], "value": "b"}, "children": [{"id": "pa", "component": "Text", "props": {"text": "a"}}, {"id": "pb", "component": "Text", "props": {"text": "b"}}]}]
        })
        .to_string(),
    )
    .unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout_fixed(None, true).unwrap();
    let nodes = s.nodes();
    let root = &nodes[0];
    assert_eq!(root.macro_name.as_deref(), Some("Card"));
    let tab_b = nodes.iter().find(|n| n.id == "tabs.tab.1").expect("tab b");
    assert_eq!(tab_b.part_states, vec!["selected".to_string()]);
    assert!(tab_b.macro_name.is_none(), "a part names no macro");
    let tab_a = nodes.iter().find(|n| n.id == "tabs.tab.0").unwrap();
    assert!(tab_a.part_states.is_empty());
}

// ---------------------------------------------------------------------------
// The host API (VAPP-91): the host fixtures through the facade, as the Swift
// and Kotlin suites replay them.
// ---------------------------------------------------------------------------

fn eq(got: &str, expected: &Value, label: &str) {
    assert!(json_equal(got.to_string(), expected.to_string()), "{label}: {}", json_diff(got.to_string(), expected.to_string()));
}

fn feed_decoder(push: impl Fn(String) -> String, end: impl Fn() -> String, chunks: &Value) -> Value {
    let mut messages = Vec::new();
    let mut issues = Vec::new();
    let mut take = |s: String| {
        let v: Value = serde_json::from_str(&s).unwrap();
        messages.extend(v["messages"].as_array().unwrap().iter().cloned());
        issues.extend(v["issues"].as_array().unwrap().iter().cloned());
    };
    for c in chunks.as_array().unwrap() {
        take(push(c.as_str().unwrap().to_string()));
    }
    take(end());
    json!({"messages": messages, "issues": issues})
}

#[test]
fn the_host_transport_fixture_replays_through_the_facade() {
    let f = fixture("host-transport.json");
    for c in f["jsonl"].as_array().unwrap() {
        let d = JsonlDecoder::new();
        let got = feed_decoder(|s| d.push(s), || d.end(), &c["chunks"]);
        eq(&got.to_string(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in f["sse"].as_array().unwrap() {
        let d = SseDecoder::new();
        let got = feed_decoder(|s| d.push(s), || d.end(), &c["chunks"]);
        eq(&got.to_string(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in f["mcp"].as_array().unwrap() {
        eq(&messages_from_mcp_result_json(c["result"].to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in f["mcpAction"].as_array().unwrap() {
        let got = mcp_action_call_json(c["message"].to_string(), c.get("tool").and_then(Value::as_str).map(str::to_string), c.get("metadata").map(Value::to_string)).unwrap();
        eq(&got, &c["expected"], c["name"].as_str().unwrap());
    }
    eq(&decode_jsonl_json("[{\"a\":1}]".into()), &json!({"messages": [{"a": 1}], "issues": []}), "decodeJsonl");
}

#[test]
fn the_host_policy_fixture_replays_through_the_facade() {
    let f = fixture("host-policy.json");
    for c in f["functions"].as_array().unwrap() {
        let got = decide_function(c.get("policy").map(Value::to_string), c["fn"].as_str().unwrap().into(), c["registered"].as_bool().unwrap()).unwrap();
        assert_eq!(got, c["expected"].as_str().unwrap(), "{}", c["name"]);
    }
    for c in f["combine"].as_array().unwrap() {
        assert_eq!(combine_decisions(c["a"].as_str().unwrap().into(), c["b"].as_str().unwrap().into()).unwrap(), c["expected"].as_str().unwrap());
    }
    for c in f["urls"].as_array().unwrap() {
        eq(&decide_url_json(c.get("policy").map(Value::to_string), c["url"].as_str().unwrap().into()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in f["media"].as_array().unwrap() {
        let got = media_request_json(c["url"].as_str().unwrap().into(), c["options"].to_string()).unwrap();
        eq(&got.unwrap_or_else(|| "null".into()), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in f["sources"].as_array().unwrap() {
        eq(&parse_source_json(c["uri"].as_str().unwrap().into()).unwrap_or_else(|| "null".into()), &c["expected"], c["uri"].as_str().unwrap());
    }
    for c in f["negotiation"].as_array().unwrap() {
        let ids: Vec<String> = serde_json::from_value(c["extensionIds"].clone()).unwrap();
        assert_eq!(json!(supported_catalog_ids_for(ids.clone())), c["expected"]["supportedCatalogIds"]);
        eq(&renderer_capabilities_json(ids, None).unwrap(), &c["expected"]["rendererCapabilities"], "clientCapabilities");
    }
    assert!(combine_decisions("allow".into(), "maybe".into()).is_err());
    eq(&package_policy_json(Some("[\"harness_toast\"]".into())).unwrap(), &json!({"allow": ["harness_toast"], "default": "deny"}), "packagePolicy");
}

#[test]
fn the_host_router_fixture_replays_through_the_facade() {
    let f = fixture("host-router.json");
    let packages = &f["packages"];
    for v in f["validation"].as_array().unwrap() {
        let id = v["package"].as_str().unwrap();
        eq(&validate_package_json(packages[id].to_string(), None).unwrap(), &v["expected"], id);
    }
    // VAPP-218: the screens a host opens, the navigate targets.
    for c in f["screens"].as_array().unwrap() {
        let pkg = packages[c["package"].as_str().unwrap()].to_string();
        eq(&resolve_screen_json(pkg, c["screen"].as_str().map(str::to_string), Some(c["params"].to_string())).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in f["navigate"].as_array().unwrap() {
        let pkg = packages[c["package"].as_str().unwrap()].to_string();
        eq(&navigate_target_json(pkg, c["args"].to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for flow in f["flows"].as_array().unwrap() {
        let name = flow["name"].as_str().unwrap();
        let ext: Vec<String> = flow.get("extensionIds").map(|e| serde_json::from_value(e.clone()).unwrap()).unwrap_or_default();
        let router = HostRouter::new(ext);
        for id in flow.get("packages").and_then(Value::as_array).into_iter().flatten() {
            let id = id.as_str().unwrap();
            eq(&router.install_package(packages[id].to_string()).unwrap(), &flow["installIssues"][id], &format!("{name}: install {id}"));
        }
        for (i, step) in flow["steps"].as_array().unwrap().iter().enumerate() {
            eq(&router.route_json(step["message"].to_string(), step["trusted"] == json!(true)), &step["expected"], &format!("{name}: step {i}"));
        }
    }
    // State + the client messages.
    let router = HostRouter::new(vec![]);
    router.install_package(packages["acme.devices"].to_string()).unwrap();
    router.route_json(json!({"version": a2ui_version(), "applyTemplate": {"surfaceId": "d", "templateId": "list"}}).to_string(), true);
    assert_eq!(router.surface_ids(), ["d"]);
    assert_eq!(router.package_id_of("d".into()).as_deref(), Some("acme.devices"));
    router.register_extension("https://acme.example/catalog/v1".into());
    assert_eq!(router.supported_catalog_ids().len(), 4);
    let ops: Value = serde_json::from_str(&router.route_json("not json".into(), false)).unwrap();
    assert_eq!(ops[0]["message"]["error"]["code"], "INVALID_MESSAGE");
    let msgs = template_messages_json(packages["acme.devices"].to_string(), "list".into(), "x".into(), Some("{\"filter\":\"online\"}".into())).unwrap().unwrap();
    let msgs: Value = serde_json::from_str(&msgs).unwrap();
    assert_eq!(msgs.as_array().unwrap().len(), 2);
    assert_eq!(msgs[0]["createSurface"]["dataModel"], json!({"title": "Devices", "filter": "online"}));
    assert!(template_messages_json(packages["acme.devices"].to_string(), "nope".into(), "x".into(), None).unwrap().is_none());
    eq(
        &action_message_json("s".into(), "btn".into(), "save".into(), "{\"id\":1}".into(), Some("{\"value\":\"a\"}".into()), "t".into()).unwrap(),
        &json!({"version": "1.0", "action": {"name": "save", "surfaceId": "s", "sourceComponentId": "btn", "timestamp": "t", "context": {"id": 1}, "metadata": {"extensions": {"at_exponential_payload": {"value": "a"}}}}}),
        "actionMessage",
    );
    eq(&error_message_json("INVALID_FUNCTION_CALL".into(), "s".into(), "m".into(), None), &json!({"version": "1.0", "error": {"code": "INVALID_FUNCTION_CALL", "surfaceId": "s", "message": "m"}}), "errorMessage");
    let contract: Value = serde_json::from_str(&host_contract_json()).unwrap();
    assert_eq!(contract["transport"]["mcpActionTool"], "a2ui_event");
}

#[test]
fn a_host_function_call_crosses_the_facade_as_a_function_call_event() {
    let surface = Surface::new("s".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface
        .set_nested(
            json!({"id": "root", "component": "Box", "children": [
                {"id": "b", "component": "Button", "props": {"label": "Go"}, "on": {"press": {"functionCall": {"@call": "harness_toast", "args": {"message": {"@path": "/m"}}}}}}
            ]})
            .to_string(),
        )
        .unwrap();
    surface.set_data("/m".into(), Some("\"hi\"".into())).unwrap();
    surface.set_viewport(400.0, 0.0, None);
    surface.layout_fixed(None, false).unwrap();
    let events = surface.event(surface.index_of("b".into()).unwrap(), "press".into(), None).unwrap();
    let call = events.iter().find(|e| e.kind == "functionCall").expect("functionCall");
    eq(&call.json, &json!({"kind": "functionCall", "componentId": "b", "name": "harness_toast", "args": {"message": "hi"}}), "functionCall");
}

// ---------------------------------------------------------------------------
// Round 2 (docs/round-2-contract.md): the round-2 fixtures through the free
// functions, the foreign formatter, rows/sections, scrollToIndex.
// ---------------------------------------------------------------------------

struct Loud;

impl HostFormatter for Loud {
    fn locale(&self) -> String {
        "x-loud".into()
    }
    fn number(&self, value: f64, decimals: Option<u32>, _grouping: bool) -> String {
        format!("#{value}/{}", decimals.map(|d| d.to_string()).unwrap_or_default())
    }
    fn currency(&self, value: f64, code: String, _: Option<u32>, _: bool) -> String {
        format!("{code}{value}")
    }
    fn percent(&self, value: f64, _: Option<u32>) -> String {
        format!("{value}pc")
    }
    fn date(&self, epoch_ms: f64, date_only: bool, format: Option<String>, style: Option<String>, time: bool) -> String {
        format!("{epoch_ms}|{date_only}|{}|{}|{time}", format.unwrap_or_default(), style.unwrap_or_default())
    }
    fn relative_time(&self, value: i64, unit: String) -> String {
        format!("{value} {unit}")
    }
    fn plural(&self, value: f64) -> String {
        if value == 2.0 { "two".into() } else { "other".into() }
    }
    fn bytes(&self, value: f64, unit: String) -> String {
        format!("{value}~{unit}")
    }
}

#[test]
fn the_round_2_fixtures_replay_through_the_free_functions() {
    let format = fixture("format.json");
    for c in format["calls"].as_array().unwrap() {
        let got = format_call_json(c["call"].to_string(), None, None, None).unwrap().unwrap_or_else(|| "null".into());
        eq(&got, &c["expected"], c["name"].as_str().unwrap());
    }
    for c in format["zoned"].as_array().unwrap() {
        let got = format_call_json(c["call"].to_string(), None, None, Some(c["offsetMinutes"].as_i64().unwrap() as i32)).unwrap().unwrap_or_else(|| "null".into());
        eq(&got, &c["expected"], c["name"].as_str().unwrap());
    }
    for d in format["display"].as_array().unwrap() {
        assert_eq!(display_string_json(d["value"].to_string()).unwrap(), d["expected"].as_str().unwrap(), "{}", d["name"]);
    }
    let resizable = fixture("resizable.json");
    let near = |got: &str, want: &Value, label: &str| {
        let g: Value = serde_json::from_str(got).unwrap();
        let (g, w) = (g.as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>()).unwrap_or_else(|| vec![g.as_f64().unwrap()]), want.as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>()).unwrap_or_else(|| vec![want.as_f64().unwrap()]));
        assert!(g.len() == w.len() && g.iter().zip(&w).all(|(a, b)| (a - b).abs() <= 1e-6), "{label}: {g:?} / {w:?}");
    };
    for (op, key) in [("normalize", "normalize"), ("resize", "resize"), ("key", "keys"), ("extents", "extents"), ("drag", "drag")] {
        for c in resizable[key].as_array().unwrap() {
            let mut req = c.clone();
            req["op"] = json!(op);
            near(&resizable_json(req.to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
        }
    }
    let list = fixture("virtual-list.json");
    let expand = |e: &Value| e.as_array().cloned().unwrap_or_else(|| vec![e["extent"].clone(); e["count"].as_u64().unwrap() as usize]);
    for c in list["windows"].as_array().unwrap() {
        let req = json!({"op": "window", "extents": expand(&c["extents"]), "gap": c["gap"], "scroll": c["scroll"], "viewport": c["viewport"], "overscan": c.get("overscan")});
        eq(&list_json(req.to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in list["scrollTo"].as_array().unwrap() {
        let req = json!({"op": "scrollTo", "extents": expand(&c["extents"]), "gap": c["gap"], "index": c["index"], "viewport": c["viewport"], "scroll": c["scroll"], "align": c["align"], "inset": c.get("inset")});
        eq(&list_json(req.to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in list["sectionedScrollTo"].as_array().unwrap() {
        let sections: Value = serde_json::from_str(&list_json(json!({"op": "sections", "items": c["items"], "sectionBy": c["sectionBy"]}).to_string()).unwrap()).unwrap();
        let rows = sections["rows"].as_array().unwrap().clone();
        let extents: Vec<Value> = rows.iter().map(|r| if r.get("header").is_some() { c["headerExtent"].clone() } else { c["itemExtent"].clone() }).collect();
        let req = json!({"op": "scrollToItem", "rows": rows, "rowExtents": extents, "gap": c["gap"], "index": c["index"], "viewport": c["viewport"], "scroll": c["scroll"], "align": c["align"], "stickyHeaders": c["stickyHeaders"]});
        eq(&list_json(req.to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    let items = fixture("template-items.json");
    for c in items["keys"].as_array().unwrap() {
        eq(&list_json(json!({"op": "keys", "items": c["items"], "key": c.get("key")}).to_string()).unwrap(), &c["expected"], c["name"].as_str().unwrap());
    }
    for c in items["reduce"].as_array().unwrap() {
        let got = match c.get("nested") {
            Some(n) => reduce_nested_json(n.to_string(), c["catalogId"].as_str().unwrap().into(), None).unwrap(),
            None => reduce_surface_json(c["components"].to_string(), c["catalogId"].as_str().unwrap().into(), None).unwrap(),
        };
        eq(&got, &c["expected"], c["name"].as_str().unwrap());
    }
    let anim = fixture("animations.json");
    let pulse = &anim["themes"]["neutral"]["pulse"];
    for f in pulse["frames"].as_array().unwrap() {
        let got: Value = serde_json::from_str(&animation_frame_json("pulse".into(), pulse["timing"].to_string(), f["t"].as_f64().unwrap(), false).unwrap().unwrap()).unwrap();
        assert!((got["opacity"].as_f64().unwrap() - f["frame"]["opacity"].as_f64().unwrap()).abs() < 1e-3);
    }
    assert_eq!(relative_time_unit_json(-86_400_000.0), json!({"value": -1, "unit": "day"}).to_string());
    assert_eq!(parse_date_value_json(json!("2026-10-14").to_string()).unwrap().unwrap(), json!({"ms": 1_791_936_000_000.0_f64, "dateOnly": true}).to_string());
    assert_eq!(format_pattern_json("EEE d MMM".into(), json!({"year": 2026, "month": 10, "day": 14, "weekday": 3}).to_string(), None).unwrap(), "Wed 14 Oct");
}

#[test]
fn a_foreign_formatter_localizes_what_the_core_decided() {
    let call = |c: Value, now: Option<f64>| format_call_json(c.to_string(), Some(Arc::new(Loud)), now, None).unwrap().unwrap();
    assert_eq!(call(json!({"@call": "formatNumber", "args": {"value": 3, "decimals": 2}}), None), json!("#3/2").to_string());
    assert_eq!(call(json!({"@call": "formatCurrency", "args": {"value": 3, "currency": "eur"}}), None), json!("EUR3").to_string());
    assert_eq!(call(json!({"@call": "formatDate", "args": {"value": "2026-10-14", "style": "long", "time": true}}), None), json!("1791936000000|true||long|true").to_string());
    assert_eq!(call(json!({"@call": "formatRelativeTime", "args": {"value": 0}}), Some(7_200_000.0)), json!("-2 hour").to_string());
    assert_eq!(call(json!({"@call": "pluralize", "args": {"value": 2, "two": "a pair", "other": "many"}}), None), json!("a pair").to_string());
    // On a surface: Table cells and bound format calls.
    let surface = Surface::new("f".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface
        .set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "n", "component": "Text", "props": {"text": {"@call": "formatNumber", "args": {"value": 1.5}}}}]}).to_string())
        .unwrap();
    surface.set_formatter(Some(Arc::new(Loud)));
    surface.set_viewport(390.0, 0.0, None);
    surface.layout_fixed(None, true).unwrap();
    let n = surface.nodes().into_iter().find(|n| n.id == "n").unwrap();
    assert!(n.props_json.contains("\"#1.5/\""), "{}", n.props_json);
    // A FileUpload's sizes: the value scaled, the unit the host names.
    surface
        .set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "f", "component": "FileUpload", "props": {"files": [{"name": "a.png", "size": 48213}]}}]}).to_string())
        .unwrap();
    surface.layout_fixed(None, true).unwrap();
    let meta = surface.nodes().into_iter().find(|n| n.id == "f.fileMeta.0").expect("the size line");
    assert!(meta.props_json.contains("\"47.1~kilobyte\""), "{}", meta.props_json);
    // The English fallback in the host's zone (`HostZone` = the platform's
    // UTC offset per instant).
    struct Berlin;
    impl HostZone for Berlin {
        fn offset_minutes(&self, _: f64) -> i32 {
            120
        }
    }
    let zoned = Surface::new("z".into(), core_catalog_id(), None, "light".into()).unwrap();
    zoned.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Text", "props": {"text": {"@call": "formatDate", "args": {"value": "2026-10-14T22:30:00Z", "format": "yyyy-MM-dd HH:mm"}}}}]}).to_string()).unwrap();
    zoned.set_fallback_zone(Some(Arc::new(Berlin)));
    zoned.set_viewport(390.0, 0.0, None);
    zoned.layout_fixed(None, true).unwrap();
    assert!(zoned.nodes().into_iter().find(|n| n.id == "d").unwrap().props_json.contains("2026-10-15 00:30"));
    assert!(!zoned.uses_clock(), "no relative time on screen");
    let ticking = Surface::new("c".into(), core_catalog_id(), None, "light".into()).unwrap();
    ticking.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "r", "component": "Text", "props": {"text": {"@call": "formatRelativeTime", "args": {"value": 0}}}}]}).to_string()).unwrap();
    ticking.set_viewport(390.0, 0.0, None);
    ticking.layout_fixed(None, true).unwrap();
    assert!(ticking.uses_clock());
    ticking.set_clock(Some(0.0));
    assert!(!ticking.uses_clock());
    // `timeZone` is the host's (round 2 §3): the facade keeps it for
    // `settings()`, the core takes the zone only through a formatter or a
    // `HostZone`, so on its own it shifts nothing (instants stay UTC).
    let mut settings = surface.settings();
    settings.time_zone = Some("Europe/Vienna".into());
    surface.set_settings(settings).unwrap();
    assert_eq!(surface.settings().time_zone.as_deref(), Some("Europe/Vienna"));
    let utc = Surface::new("u".into(), core_catalog_id(), None, "light".into()).unwrap();
    utc.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Text", "props": {"text": {"@call": "formatDate", "args": {"value": "2026-10-14T22:30:00Z", "format": "yyyy-MM-dd HH:mm"}}}}]}).to_string()).unwrap();
    let mut settings = utc.settings();
    settings.time_zone = Some("Asia/Tokyo".into());
    utc.set_settings(settings).unwrap();
    utc.set_viewport(390.0, 0.0, None);
    utc.layout_fixed(None, true).unwrap();
    assert!(utc.nodes().into_iter().find(|n| n.id == "d").unwrap().props_json.contains("2026-10-14 22:30"));
}

#[test]
fn row_slots_and_section_headers_bind_over_the_facade() {
    let bind = fixture("bind-time.json");
    let case = bind["extra"].as_array().unwrap().iter().find(|c| c["name"] == "Table/slot-cell:literal-rows").unwrap();
    let d = &case["datasets"][0];
    let bound: Value = serde_json::from_str(&bind_tree_json(case["expanded"].to_string(), d["data"].to_string(), String::new(), None).unwrap().unwrap()).unwrap();
    for t in d["rowSlots"].as_array().unwrap() {
        fn find<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
            if n["id"] == id {
                return Some(n);
            }
            n["slots"].as_object().into_iter().flat_map(|s| s.values()).chain(n["children"].as_array().into_iter().flatten()).find_map(|c| find(c, id))
        }
        let id = t["id"].as_str().unwrap();
        let node = find(&bound, id).unwrap();
        let rows_prop = find(&case["expanded"], id).unwrap()["props"]["rows"].clone();
        for r in t["rows"].as_array().unwrap() {
            for (slot, expected) in r["slots"].as_object().unwrap() {
                let got = bind_row_slot_json(node["slots"][slot].to_string(), rows_prop.to_string(), node["props"]["rows"].to_string(), r["index"].as_u64().unwrap() as u32, d["data"].to_string(), None, None).unwrap();
                eq(&got.unwrap_or_else(|| "null".into()), expected, &format!("row {}", r["index"]));
            }
        }
    }
    let header = json!({"id": "h", "component": "Text", "props": {"text": {"@call": "concat", "args": {"values": [{"@path": "value"}, " · ", {"@path": "count"}, " · ", {"@path": "/title"}]}}}});
    let got: Value = serde_json::from_str(&bind_section_header_json(header.to_string(), json!({"value": "Today", "count": 2}).to_string(), 0, json!({"title": "Inbox"}).to_string(), None, None).unwrap().unwrap()).unwrap();
    assert_eq!(got["props"]["text"], json!("Today · 2 · Inbox"));
}

#[test]
fn scroll_to_index_resizable_and_sticky_cross_the_facade() {
    let rows: Vec<Value> = (0..1000).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 40, "flexShrink": 0}})).collect();
    let surface = Surface::new("l".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "l", "component": "List", "style": {"height": 400}, "children": rows}]}).to_string()).unwrap();
    surface.set_viewport(390.0, 800.0, None);
    surface.layout_fixed(None, true).unwrap();
    let events = surface.scroll_to_index("l".into(), 500, Some("start".into()));
    assert!(events.iter().any(|e| e.kind == "relayout"));
    let out = surface.layout_fixed(None, true).unwrap();
    assert!(out.lists[0].start <= 500 && out.lists[0].end > 500);
    assert!(surface.index_of("r500".into()).is_some());
    let events = surface.command_json(json!({"scrollToIndex": {"id": "l", "index": 10, "align": "start"}}).to_string()).unwrap();
    assert!(events.iter().any(|e| e.kind == "relayout"));

    let split = Surface::new("r".into(), core_catalog_id(), None, "light".into()).unwrap();
    split.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "s", "component": "Resizable", "props": {"sizes": [50, 50]}, "style": {"height": 100}, "children": [{"id": "a", "component": "Box"}, {"id": "b", "component": "Box", "style": {"backdropBlur": "$blur.sm", "animation": "pulse"}}]}]}).to_string()).unwrap();
    split.set_viewport(401.0, 0.0, None);
    split.layout_fixed(None, true).unwrap();
    let handle = split.index_of("s.handle.0".into()).unwrap();
    let events = split.event(handle, "key".into(), Some(json!({"key": "ArrowRight"}).to_string())).unwrap();
    let action = events.iter().find(|e| e.kind == "relayout");
    assert!(action.is_some());
    split.layout_fixed(None, true).unwrap();
    let s = split.nodes().into_iter().find(|n| n.id == "s").unwrap();
    assert!(s.props_json.contains("\"sizes\":[60.0,40.0]"), "{}", s.props_json);
    let b = split.visual(split.index_of("b".into()).unwrap()).unwrap();
    assert_eq!(b.backdrop_blur, Some(4.0));
    assert!(b.animation_json.unwrap().contains("\"name\":\"pulse\""));

    // `position: sticky` against the host scroll: `FfiLayout.sticky` carries
    // the node and its offset, `FfiVisual.sticky` flags it.
    let page = Surface::new("p".into(), core_catalog_id(), None, "light".into()).unwrap();
    page.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "bar", "component": "Box", "style": {"position": "sticky", "top": 0, "height": 20}}, {"id": "body", "component": "Box", "style": {"height": 2000, "flexShrink": 0}}]}).to_string()).unwrap();
    page.set_viewport(390.0, 0.0, None);
    page.layout_fixed(None, true).unwrap();
    assert!(page.set_surface_scroll(0.0, 300.0));
    let out = page.layout_fixed(None, true).unwrap();
    let bar = page.index_of("bar".into()).unwrap();
    let pin = out.sticky.iter().find(|s| s.index == bar).unwrap_or_else(|| panic!("{:?}", out.sticky));
    assert_eq!((pin.dx, pin.dy), (0.0, 300.0));
    assert!(page.visual(bar).unwrap().sticky);
    assert!(!page.visual(page.index_of("body".into()).unwrap()).unwrap().sticky);
}

#[test]
fn list_and_resizable_helpers_take_the_gap_and_the_handle_extent() {
    let sticky = |req: Value| list_json(req.to_string());
    // Rows of 40 with a 10 gap: header 2 starts at 100, not 80.
    let got: Value = serde_json::from_str(&sticky(json!({"op": "sticky", "rowExtents": [40, 40, 40, 40], "headerRows": [2, 0], "scroll": 90, "gap": 10})).unwrap()).unwrap();
    assert_eq!(got, json!({"row": 0, "offset": 60.0}), "header 0 pushed back by header 2: min(90, 100 − 40)");
    let got: Value = serde_json::from_str(&sticky(json!({"op": "sticky", "rowExtents": [40, 40, 40], "rowOffsets": [0, 60, 120, 160], "headerRows": [1], "scroll": 70})).unwrap()).unwrap();
    assert_eq!(got, json!({"row": 1, "offset": 70.0}));
    assert!(sticky(json!({"op": "sticky", "rowExtents": [40], "headerRows": [3], "scroll": 0})).is_err(), "an out-of-range header is an error, not a panic");
    let drag = |extent: Option<f64>| {
        let mut req = json!({"op": "drag", "px": 40, "container": 404, "panels": 2, "orientation": "horizontal"});
        if let Some(e) = extent {
            req["handleExtent"] = json!(e);
        }
        resizable_json(req.to_string()).unwrap().parse::<f64>().unwrap()
    };
    assert!((drag(None) - 40.0 / 403.0 * 100.0).abs() < 1e-6);
    assert!((drag(Some(4.0)) - 10.0).abs() < 1e-6, "{}", drag(Some(4.0)));
}

#[test]
fn bind_helpers_format_through_the_host_formatter() {
    let slot = json!({"id": "c", "component": "Text", "props": {"text": {"@call": "formatNumber", "args": {"value": {"@path": "n"}}}}});
    let rows = json!([{"n": 2.5}]);
    let got: Value = serde_json::from_str(&bind_row_slot_json(slot.to_string(), rows.to_string(), rows.to_string(), 0, "{}".into(), None, Some(Arc::new(Loud))).unwrap().unwrap()).unwrap();
    assert_eq!(got["props"]["text"], json!("#2.5/"));
    let got: Value = serde_json::from_str(&bind_row_slot_json(slot.to_string(), rows.to_string(), rows.to_string(), 0, "{}".into(), None, None).unwrap().unwrap()).unwrap();
    assert_eq!(got["props"]["text"], json!("2.5"), "None = the English fallback");
    // A section header: the formatter and `options.now`.
    let header = json!({"id": "h", "component": "Text", "props": {"text": {"@call": "formatRelativeTime", "args": {"value": 0}}}});
    let got: Value = serde_json::from_str(&bind_section_header_json(header.to_string(), json!({"value": "a", "count": 1}).to_string(), 0, "{}".into(), Some(json!({"now": 120000}).to_string()), Some(Arc::new(Loud))).unwrap().unwrap()).unwrap();
    assert_eq!(got["props"]["text"], json!("-2 minute"));
}

// ---------------------------------------------------------------------------
// VAPP-99 round 2: the SwiftUI painter's asks (VAPP-100) through the facade.
// ---------------------------------------------------------------------------

/// Records every leaf it was asked; Markdown grows by `extra` once loaded.
struct Recording {
    leaves: Mutex<Vec<FfiLeaf>>,
    extra: f32,
}

impl Measurer for Recording {
    fn measure_id(&self) -> u64 {
        11
    }
    fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics> {
        let out = leaves
            .iter()
            .map(|l| {
                let w = l.control.width.unwrap_or(8.0 * l.text.chars().count() as f32 + 2.0 * l.control.padding_horizontal);
                let extra = if l.component == "Markdown" { self.extra } else { 0.0 };
                let h = l.control.height.unwrap_or(l.text_style.line_height + 2.0 * l.control.padding_vertical) + extra;
                FfiIntrinsics { min_content_width: w, max_content_width: w, height_at_max_content: h, baseline: None }
            })
            .collect();
        self.leaves.lock().unwrap().extend(leaves);
        out
    }
    fn measure_heights(&self, leaves: Vec<FfiLeaf>, requests: Vec<FfiHeightRequest>) -> Vec<f32> {
        requests.iter().map(|r| leaves.iter().find(|l| l.index == r.index).map(|l| l.text_style.line_height).unwrap_or(0.0)).collect()
    }
}

fn recording(extra: f32) -> Arc<Recording> {
    Arc::new(Recording { leaves: Mutex::new(Vec::new()), extra })
}

fn frame_of(s: &Surface, out: &FfiLayout, id: &str) -> FfiFrame {
    let i = s.index_of(id.into()).unwrap_or_else(|| panic!("no node {id}"));
    *out.frames.iter().find(|f| f.index == i).unwrap_or_else(|| panic!("{id} not drawn"))
}

#[test]
fn text_styles_carry_letter_spacing_transform_and_font_style() {
    let theme = Theme::load(
        r#"{"$schema":"https://ui.exponential.at/schemas/theme/v1.json","id":"caps","name":"Caps","extends":"neutral","recipes":{"Text":{"root":[{"when":{"variant":"caption"},"style":{"letterSpacing":0.5,"textTransform":"uppercase","fontStyle":"italic"}}]}}}"#.into(),
        None,
    )
    .unwrap();
    let s = Surface::with_theme("t".into(), core_catalog_id(), Some(theme), "light".into()).unwrap();
    s.set_nested(json!({"id": "root", "component": "Box", "children": [
        {"id": "cap", "component": "Text", "props": {"text": "Overline", "variant": "caption"}},
        {"id": "body", "component": "Text", "props": {"text": "Body"}}
    ]}).to_string())
    .unwrap();
    s.set_viewport(400.0, 0.0, None);
    let m = recording(0.0);
    s.layout(m.clone());
    let cap = s.text_style(s.index_of("cap".into()).unwrap()).unwrap();
    assert_eq!((cap.letter_spacing, cap.text_transform.as_deref(), cap.font_style.as_deref()), (Some(0.5), Some("uppercase"), Some("italic")));
    let body = s.text_style(s.index_of("body".into()).unwrap()).unwrap();
    assert_eq!((body.letter_spacing, body.text_transform, body.font_style), (None, None, None));
    let asked = m.leaves.lock().unwrap().iter().find(|l| l.id == "cap").cloned().expect("measured");
    assert_eq!(asked.text_style.letter_spacing, Some(0.5), "the measurer gets them too");
    assert_eq!(asked.text_style.text_transform.as_deref(), Some("uppercase"));
    // The font scale scales the tracking like the size.
    s.set_font_scale(2.0);
    s.layout(recording(0.0));
    assert_eq!(s.text_style(s.index_of("cap".into()).unwrap()).unwrap().letter_spacing, Some(1.0));
}

#[test]
fn leaves_name_their_owner_and_show_numbers_as_text() {
    let s = Surface::new("t".into(), core_catalog_id(), Some("neutral".into()), "dark".into()).unwrap();
    s.set_nested(json!({"id": "root", "component": "Box", "children": [
        {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "A", "value": "a"}], "value": "a"}, "children": [{"id": "pa", "component": "Text", "props": {"text": "a"}}]},
        {"id": "steps", "component": "Stepper", "props": {"steps": [{"label": "One"}, {"label": "Two"}], "current": 1}},
        {"id": "n", "component": "Text", "props": {"text": {"@path": "/n"}}}
    ]}).to_string())
    .unwrap();
    s.set_data("/n".into(), Some("412".into())).unwrap();
    s.set_viewport(400.0, 0.0, None);
    let m = recording(0.0);
    s.layout(m.clone());
    let leaves = m.leaves.lock().unwrap();
    let by = |id: &str| leaves.iter().find(|l| l.id == id).unwrap_or_else(|| panic!("{id} not measured"));
    assert_eq!(by("tabs.tab.0").owner_component.as_deref(), Some("Tabs"));
    assert_eq!(by("steps.step.1.marker.number").owner_component.as_deref(), Some("Stepper"));
    assert_eq!(by("pa").owner_component, None, "a plain node");
    assert_eq!(by("n").text, "412", "a bound number shows as its display string");
}

#[test]
fn the_effective_theme_object_is_what_the_core_resolves_against() {
    let s = Surface::new("t".into(), core_catalog_id(), Some("neutral".into()), "light".into()).unwrap();
    s.set_nested(json!({"id": "b", "component": "Button", "props": {"label": "Go", "variant": "outline"}}).to_string()).unwrap();
    s.set_viewport(300.0, 0.0, None);
    let plain = s.effective_theme().unwrap();
    assert_eq!(plain.resolved_json(), s.theme().unwrap().resolved_json(), "default density, normal contrast = the theme");
    s.set_settings(FfiSettings { density: "compact".into(), contrast: "high".into(), ..s.settings() }).unwrap();
    let eff = s.effective_theme().unwrap();
    assert_eq!(eff.resolved_json(), s.effective_theme_json().unwrap());
    assert!(eff.control("input".into()).unwrap() < plain.control("input".into()).unwrap(), "density scales the controls");
    s.layout(recording(0.0));
    let b = s.index_of("b".into()).unwrap();
    let core = s.visual(b).unwrap();
    let part = eff.resolve_part("Button".into(), "root".into(), r#"{"label":"Go","variant":"outline"}"#.into(), vec![], "light".into()).unwrap();
    assert_eq!(part.visual.border_color, core.border_color, "a painter's part = the core's");
    let base = plain.resolve_part("Button".into(), "root".into(), r#"{"label":"Go","variant":"outline"}"#.into(), vec![], "light".into()).unwrap();
    assert_ne!(base.visual.border_color, core.border_color, "the high-contrast overlay is in the effective theme only");
    let geometry = Surface::new("g".into(), core_catalog_id(), Some(String::new()), "light".into()).unwrap();
    assert!(geometry.effective_theme().is_none());
}

#[test]
fn hover_crosses_the_facade_and_resolves_recipes() {
    let s = Surface::new("t".into(), core_catalog_id(), Some("neutral".into()), "light".into()).unwrap();
    s.set_nested(json!({"id": "pill", "component": "Chip", "props": {"label": "All", "pressable": true}}).to_string()).unwrap();
    s.set_viewport(300.0, 0.0, None);
    s.layout(recording(0.0));
    let i = s.index_of("pill".into()).unwrap();
    let rest = s.visual(i).unwrap().background_color;
    assert!(!s.nodes()[i as usize].hovered);
    assert!(s.set_hover("pill".into(), true));
    let out = s.layout(recording(0.0));
    assert!(out.visual_changes.contains(&i));
    assert_eq!(s.visual(i).unwrap().background_color, Theme::builtin("neutral".into()).unwrap().color("accent".into(), "light".into()));
    let n = &s.nodes_at(vec![i])[0];
    assert!(n.hovered);
    assert_eq!(n.interaction_states, vec!["hover".to_string()]);
    assert!(n.states.is_empty(), "`states` stays the core's");
    assert!(s.set_hovered(vec![]));
    s.layout(recording(0.0));
    assert_eq!(s.visual(i).unwrap().background_color, rest);
    assert!(!s.nodes()[i as usize].hovered);
}

#[test]
fn hover_styled_names_the_nodes_a_painter_tracks_the_pointer_over() {
    let s = Surface::new("t".into(), core_catalog_id(), Some("neutral".into()), "light".into()).unwrap();
    s.set_nested(json!({"id": "root", "component": "Box", "children": [
        {"id": "plain", "component": "Box", "style": {"width": 10, "height": 10}},
        {"id": "styled", "component": "Box", "style": {"width": 10, "height": 10, ":hover": {"opacity": 0.5}}},
        {"id": "b", "component": "Button", "props": {"label": "Go", "variant": "default"}}
    ]}).to_string()).unwrap();
    s.set_viewport(300.0, 0.0, None);
    s.layout(recording(0.0));
    let styled = |id: &str| {
        let i = s.index_of(id.into()).unwrap();
        let all = s.nodes()[i as usize].hover_styled;
        assert_eq!(s.nodes_at(vec![i])[0].hover_styled, all, "{id}: nodes and nodes_at agree");
        all
    };
    assert!(styled("styled"), "a `:hover` style block");
    assert!(styled("b"), "a recipe rule on `state: hover`");
    assert!(!styled("plain") && !styled("root"));
}

#[test]
fn contract_heights_and_a_loaded_markdown_cross_the_facade() {
    let s = Surface::new("t".into(), core_catalog_id(), Some("neutral".into()), "dark".into()).unwrap();
    s.set_nested(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start"}, "children": [
        {"id": "chart", "component": "Chart", "props": {"kind": "bar", "title": "This week", "height": 160, "categories": ["Mon"], "series": [{"name": "Runs", "values": [3]}, {"name": "Merges", "values": [1]}]}},
        {"id": "badge", "component": "Badge", "props": {"text": "3", "variant": "secondary"}},
        {"id": "md", "component": "Markdown", "props": {"text": "![chart](https://example.com/c.png)"}}
    ]}).to_string())
    .unwrap();
    s.set_viewport(400.0, 0.0, None);
    let out = s.layout(recording(0.0));
    assert_eq!(frame_of(&s, &out, "chart").h, 160.0, "Chart `height` = the whole box");
    let (badge, label) = (frame_of(&s, &out, "badge"), frame_of(&s, &out, "badge.label"));
    assert_eq!(badge.h, label.h + 2.0 * (label.y - badge.y), "Badge = content + padding, no 32 px minimum");
    let md = frame_of(&s, &out, "md").h;
    // The picture loaded: `mark_dirty` re-asks the Markdown.
    let loaded = recording(80.0);
    assert_eq!(frame_of(&s, &s.layout(loaded.clone()), "md").h, md, "memoized until marked");
    assert!(s.mark_dirty(s.index_of("md".into()).unwrap()));
    assert_eq!(frame_of(&s, &s.layout(loaded), "md").h, md + 80.0);
}

/// VAPP-103: the facade refuses an oversized message BEFORE parsing it, and a
/// refused data write is an error, never a silent gap.
#[test]
fn the_facade_refuses_oversized_messages_and_bad_writes() {
    let router = HostRouter::new(vec![]);
    let big = format!(r#"{{"version":"1.0","updateComponents":{{"surfaceId":"s","components":[{{"id":"root","component":"Text","text":"{}"}}]}}}}"#, "x".repeat(4_194_304));
    let ops: Value = serde_json::from_str(&router.route_json(big, false)).unwrap();
    assert_eq!(ops[0]["message"]["error"]["code"], "INVALID_MESSAGE");
    assert_eq!(ops[0]["message"]["error"]["message"], "message larger than 4194304 bytes");
    let surface = Surface::new("s".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_data("/a".into(), Some("[]".into())).unwrap();
    assert!(surface.set_data("/a/4000000000".into(), Some("1".into())).is_err());
    assert_eq!(surface.data_json(), r#"{"a":[]}"#);
}

/// #VAPP-288: `assign_data` is a programmatic write (it bumps `write_epoch`),
/// `set_data` is not; a refused `assign_data` bumps nothing.
#[test]
fn assign_data_moves_the_write_epoch_and_set_data_does_not() {
    let surface = Surface::new("s".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_data("/draft".into(), Some("\"abc\"".into())).unwrap();
    assert_eq!(surface.write_epoch(), 0);
    surface.assign_data("/draft".into(), Some("\"\"".into())).unwrap();
    assert_eq!(surface.write_epoch(), 1);
    assert_eq!(surface.data_json(), r#"{"draft":""}"#);
    surface.set_data("/a".into(), Some("[]".into())).unwrap();
    assert!(surface.assign_data("/a/9".into(), Some("1".into())).is_err());
    assert_eq!(surface.write_epoch(), 1);
}

/// R8 F53: a Swift/Kotlin measurer answering NaN or infinity is sanitised
/// at the facade (and again in the core): the layout finishes with finite
/// frames instead of a panic across the FFI.
#[test]
fn a_nan_measurer_never_panics_the_facade() {
    struct Nan;
    impl Measurer for Nan {
        fn measure_id(&self) -> u64 {
            11
        }
        fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics> {
            leaves.iter().map(|_| FfiIntrinsics { min_content_width: f32::NAN, max_content_width: f32::INFINITY, height_at_max_content: f32::NAN, baseline: Some(f32::NAN) }).collect()
        }
        fn measure_heights(&self, _leaves: Vec<FfiLeaf>, requests: Vec<FfiHeightRequest>) -> Vec<f32> {
            requests.iter().map(|_| f32::NAN).collect()
        }
    }
    let surface = Surface::new("nan".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface.set_nested(bench_tree_json(20)).unwrap();
    surface.set_viewport(390.0, 0.0, None);
    let out = surface.layout(Arc::new(Nan));
    assert!(!out.frames.is_empty());
    assert!(out.frames.iter().all(|f| f.x.is_finite() && f.y.is_finite() && f.w.is_finite() && f.h.is_finite()), "{:?}", out.frames.first());
}

/// VAPP-162: the round-5 calls the Swift and Compose painters make.
#[test]
fn round5_qr_matrix_bars_badge_and_hold_cross_the_facade() {
    for c in fixture("qr-code.json")["cases"].as_array().unwrap() {
        let got: Value = serde_json::from_str(&qr_matrix_json(c["value"].as_str().unwrap().into(), c["level"].as_str().unwrap_or("M").into())).unwrap();
        assert_eq!(got, c["expected"], "{}", c["name"]);
    }
    let bars: Value = serde_json::from_str(&voice_wave_bars_json(json!({"size": "md", "activity": "idle"}).to_string())).unwrap();
    assert_eq!((bars["width"].as_f64(), bars["bars"].as_array().map(Vec::len)), (Some(117.0), Some(20)));
    assert_eq!(voice_wave_bars_json("nope".into()), "{}");
    assert_eq!(segmented_badge_text(Some(120.0)).as_deref(), Some("99+"));
    assert_eq!(segmented_badge_text(Some(0.0)), None);

    let surface = Surface::new("hold".into(), core_catalog_id(), None, "light".into()).unwrap();
    let tree = json!({"id": "root", "component": "Box", "children": [{"id": "b", "component": "Button", "style": {"width": 120, "height": 40},
        "props": {"label": "Hold to talk", "holdingLabel": "Holding…", "hold": true}, "on": {"holdStart": {"event": {"name": "holdStart"}}, "holdEnd": {"event": {"name": "holdEnd"}}}}]});
    surface.set_nested(tree.to_string()).unwrap();
    surface.set_viewport(400.0, 0.0, None);
    surface.layout_fixed(None, false).unwrap();
    let i = surface.index_of("b".into()).unwrap();
    let fired = surface.hold(i, json!({"do": "pointerdown", "x": 60, "y": 20}).to_string()).unwrap();
    assert!(fired.iter().any(|e| e.json.contains("holdStart")), "{fired:?}");
    assert!(surface.is_held("b".into()));
    assert!(surface.event(i, "press".into(), None).unwrap().is_empty(), "a hold Button never fires press");
    let fired = surface.hold(i, json!({"do": "pointerup", "x": 60, "y": 20}).to_string()).unwrap();
    assert!(fired.iter().any(|e| e.json.contains("holdEnd")), "{fired:?}");
    assert!(!surface.is_held("b".into()));
    assert!(surface.hold(i, "{\"do\": \"nope\"}".into()).is_err());
}

/// EXP-1605: a `.lottie`'s first state machine (manifest order), "" without one.
#[test]
fn dotlottie_machine_reads_the_first() {
    let anim = json!({"v": "5.12.0", "fr": 30, "ip": 0, "op": 30, "w": 10, "h": 10, "layers": []});
    let a = json!({"initial": "a", "states": [{"name": "a", "type": "PlaybackState"}]});
    let b = json!({"initial": "b", "states": [{"name": "b", "type": "PlaybackState"}]});
    let file = eui_motion::dotlottie::write("x", &anim, &[], &[("z".into(), a.clone()), ("b".into(), b)]);
    let got: Value = serde_json::from_str(&motion_read_dotlottie_machine(file).unwrap()).unwrap();
    assert_eq!(got, a);
    assert_eq!(motion_read_dotlottie_machine(eui_motion::dotlottie::write("x", &anim, &[], &[])).unwrap(), "");
}

/// Round 8: `fixtures/canvas.json` through the uniffi `CanvasPlayer` and
/// `canvas_to_drawing`, as the Swift and Compose painters replay it.
#[test]
fn the_canvas_fixture_replays_through_the_canvas_player() {
    let f = fixture("canvas.json");
    for case in f["compile"].as_array().unwrap() {
        let player = CanvasPlayer::new(case["canvas"].to_string()).unwrap_or_else(|e| panic!("{}: {e:?}", case["id"]));
        assert_eq!(json!(player.slot_ids()), case["slotIds"], "{}", case["id"]);
        let slots = case.get("slots").map(|s| s.to_string()).unwrap_or_default();
        let got = player.sample(slots);
        let want = case["packed"].as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{}", case["id"]);
        for (g, w) in got.iter().zip(want) {
            let w = w.as_f64().unwrap();
            assert!((*g as f64 - w).abs() <= 1e-4 * w.abs().max(1.0), "{}: {g} != {w}", case["id"]);
        }
    }
    for case in f["refuse"].as_array().unwrap() {
        let Err(UiError::Invalid { reason }) = CanvasPlayer::new(case["canvas"].to_string()) else { panic!("{} compiled", case["id"]) };
        let got: Value = serde_json::from_str(&reason).unwrap();
        let codes = |v: &Value| v.as_array().unwrap().iter().map(|i| (i["code"].clone(), i["path"].clone())).collect::<Vec<_>>();
        assert_eq!(codes(&got), codes(&case["issues"]), "{}", case["id"]);
    }
    for case in f["pointer"].as_array().unwrap() {
        let n = |v: &Value, i: usize| v[i].as_f64().unwrap();
        for p in case["points"].as_array().unwrap() {
            let d = canvas_to_drawing(n(&p["at"], 0), n(&p["at"], 1), n(&case["size"], 0), n(&case["size"], 1), n(&case["box"], 0), n(&case["box"], 1), case["fit"].as_str().unwrap().into());
            assert!((d[0] - n(&p["drawing"], 0)).abs() <= 0.01 && (d[1] - n(&p["drawing"], 1)).abs() <= 0.01, "{case} {p}: {d:?}");
        }
    }
}

/// eApps R8 (VAPP-233): `fixtures/record-migrations.json` through the free
/// functions, as the Swift (`RecordMigrationsTests`) and Compose
/// (`RecordMigrationsTest`) suites replay it.
#[test]
fn the_record_migrations_fixture_replays_through_the_free_functions() {
    let f = fixture("record-migrations.json");
    for c in f["migrateCases"].as_array().unwrap() {
        let got = migrate_record_json(c["package"].to_string(), c["version"].as_str().unwrap().into(), c["kind"].as_str().unwrap().into(), c["data"].to_string(), 0.0).unwrap();
        assert!(eui_core::json::equal(&serde_json::from_str::<Value>(&got).unwrap(), &c["expected"]), "migrate: {}", c["name"]);
    }
    for c in f["refusalCases"].as_array().unwrap() {
        let got = migration_refusal_json(c["before"].to_string(), c["after"].to_string()).unwrap();
        assert_eq!(json!(got), c["expected"], "refusal: {}", c["name"]);
    }
}

/// eApps R6 (VAPP-222): `fixtures/record-transitions.json` through the free
/// functions, exactly as the Swift (`RecordTransitionsTests`) and Compose
/// (`RecordTransitionsTest`) suites replay it.
#[test]
fn the_record_transitions_fixture_replays_through_the_free_functions() {
    let f = fixture("record-transitions.json");
    let kind_of = |c: &Value| match c.get("transitions") {
        Some(t) => json!({ "transitions": t }),
        None => f["kind"].clone(),
    };
    let opt = |v: Option<&Value>| v.filter(|v| !v.is_null()).map(Value::to_string);
    for c in f["validateCases"].as_array().unwrap() {
        let name = c["kindName"].as_str().unwrap();
        let got = validate_record_transitions_json(c["kind"].to_string(), format!("/records/{name}"), name.into()).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap(), c["expected"], "validate: {}", c["name"]);
    }
    for c in f["refusalCases"].as_array().unwrap() {
        let got = transition_refusal_json(kind_of(c).to_string(), opt(c.get("before")), c["after"].to_string(), c["viewer"].to_string(), opt(c.get("row"))).unwrap();
        assert_eq!(got.map_or(Value::Null, |g| serde_json::from_str(&g).unwrap()), c["expected"], "refusal: {}", c["name"]);
    }
    for c in f["fromCases"].as_array().unwrap() {
        let got = transitions_from_json(kind_of(c).to_string(), opt(c.get("record")), opt(c.get("viewer")), opt(c.get("row"))).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap(), c["expected"], "from: {}", c["name"]);
    }
}

/// VAPP-189: `fixtures/records-query.json` through the free functions (VAPP-190:
/// + `parse_records_json`, `date_bucket`),
/// exactly as the Swift (`RecordsQueryTests`) and Compose (`RecordsQueryTest`)
/// suites replay it.
#[test]
fn the_records_query_fixture_replays_through_the_free_functions() {
    let f = fixture("records-query.json");
    let cases = |key: &str| f[key].as_array().unwrap().clone();
    let canon = |s: &str| serde_json::from_str::<Value>(s).unwrap();
    for c in cases("parseCases") {
        let got = parse_records_query_json(c["params"].to_string(), c["kind"].as_str().map(str::to_string)).unwrap();
        assert_eq!(canon(&got), c["expected"], "parse: {}", c["name"]);
    }
    for c in cases("sourceCases") {
        let got = parse_records_source_json(c["source"].as_str().unwrap().to_string()).map(|s| canon(&s)).unwrap_or(Value::Null);
        assert_eq!(got, c["expected"], "source: {}", c["name"]);
    }
    for c in cases("jsonCases") {
        let got = parse_records_json(c["query"].to_string(), c["kind"].as_str().map(str::to_string)).unwrap();
        assert_eq!(canon(&got), c["expected"], "json: {}", c["name"]);
    }
    for c in cases("bucketCases") {
        let got = date_bucket(c["date"].as_str().unwrap().into(), c["bucket"].as_str().unwrap().into());
        assert_eq!(serde_json::to_value(got).unwrap(), c["expected"], "bucket: {} {}", c["date"], c["bucket"]);
    }
    assert_eq!(date_bucket("2026-10-05".into(), "fortnight".into()), None);
}

/// VAPP-212: `valueCases` through the handle, the one way to the value: seeded
/// at once, then the same BYTES row by row, with a row that came and went, and
/// under an overlay that changes nothing.
#[test]
fn the_records_query_fixture_replays_through_the_record_store() {
    let f = fixture("records-query.json");
    let canon = |s: &str| serde_json::from_str::<Value>(s).unwrap();
    for c in f["valueCases"].as_array().unwrap() {
        let query = match c.get("query") {
            Some(json) => canon(&parse_records_json(json.to_string(), c["kind"].as_str().map(str::to_string)).unwrap())["query"].to_string(),
            None => parse_records_query_json(c["params"].to_string(), c["kind"].as_str().map(str::to_string)).unwrap(),
        };
        let ctx = json!({"me": c["me"], "now": c["now"], "today": c.get("today"), "names": c["names"], "computed": c.get("computed"), "refs": c.get("refs"), "issues": c.get("issues"), "transitions": c.get("transitions"), "role": c.get("role")}).to_string();
        let seeded = RecordStore::new();
        seeded.apply_json(json!({"upserts": c["rows"]}).to_string()).unwrap();
        assert_eq!(seeded.count(), c["rows"].as_array().unwrap().len() as u64, "count: {}", c["name"]);
        let want = seeded.value_json(query.clone(), ctx.clone(), None).unwrap();
        assert!(eui_core::json::equal(&canon(&want), &c["expected"]), "value: {}\n got {want}", c["name"]);

        let deltas = RecordStore::new();
        let ghost = json!({"id": "ghost", "kind": "ghost", "data": {}, "createdBy": null, "createdAt": "2020-01-01T00:00:00.000Z", "updatedAt": "2020-01-01T00:00:00.000Z"});
        deltas.apply_json(json!({"upserts": [ghost]}).to_string()).unwrap();
        for row in c["rows"].as_array().unwrap() {
            deltas.apply_json(json!({"upserts": [row]}).to_string()).unwrap();
        }
        deltas.apply_json(json!({"deletes": ["ghost"]}).to_string()).unwrap();
        assert_eq!(deltas.value_json(query.clone(), ctx.clone(), None).unwrap(), want, "deltas: {}", c["name"]);
        assert_eq!(deltas.value_json(query, ctx, Some(r#"{"upserts":[],"deletes":["no-such-row"]}"#.into())).unwrap(), want, "empty overlay: {}", c["name"]);
    }
}

fn store_row(id: &str, title: &str, created_at: &str) -> Value {
    json!({"id": id, "kind": "note", "data": {"title": title}, "createdBy": "u1", "createdAt": created_at, "updatedAt": created_at})
}

/// `id: title` of what a store answers for the kind `note`, sorted by id.
fn store_rows(store: &RecordStore, overlay: Option<Value>) -> Vec<String> {
    let query = parse_records_query_json(json!({"sort": "id", "order": "asc"}).to_string(), Some("note".into())).unwrap();
    let ctx = json!({"now": "2026-11-05T00:00:00.000Z"}).to_string();
    let out: Value = serde_json::from_str(&store.value_json(query, ctx, overlay.map(|o| o.to_string())).unwrap()).unwrap();
    out["rows"].as_array().unwrap().iter().map(|r| format!("{}: {}", r["id"].as_str().unwrap(), r["title"].as_str().unwrap())).collect()
}

/// VAPP-212: an upsert replaces, a delete removes, deletes go first.
#[test]
fn a_record_store_applies_deltas() {
    let store = RecordStore::new();
    store.apply_json(json!({"upserts": [store_row("a", "1", "2026-01-01"), store_row("b", "1", "2026-01-02"), store_row("c", "1", "2026-01-03")]}).to_string()).unwrap();
    assert_eq!(store_rows(&store, None), ["a: 1", "b: 1", "c: 1"]);
    store.apply_json(json!({"upserts": [store_row("b", "2", "2026-01-02")], "deletes": ["a"]}).to_string()).unwrap();
    assert_eq!(store_rows(&store, None), ["b: 2", "c: 1"]);
    assert_eq!(store.count(), 2);
    store.apply_json(json!({"upserts": [store_row("c", "3", "2026-01-03")], "deletes": ["c"]}).to_string()).unwrap();
    assert_eq!(store_rows(&store, None), ["b: 2", "c: 3"], "the upsert wins");
    assert!(store.apply_json("{".into()).is_err());
    assert!(store.value_json("{}".into(), "{}".into(), None).is_err());
}

/// VAPP-212: an overlay changes one query, not the store.
#[test]
fn a_record_store_overlay_lays_over_one_query() {
    let store = RecordStore::newest_first();
    store.apply_json(json!({"upserts": [store_row("a", "1", "2026-01-03"), store_row("b", "1", "2026-01-02"), store_row("c", "1", "2026-01-01")]}).to_string()).unwrap();
    let overlay = json!({"upserts": [store_row("b", "patched", "2026-01-02"), store_row("new", "1", "2026-01-09")], "deletes": ["a"]});
    assert_eq!(store_rows(&store, Some(overlay)), ["b: patched", "c: 1", "new: 1"]);
    assert_eq!(store_rows(&store, None), ["a: 1", "b: 1", "c: 1"]);
    assert_eq!(store.count(), 3);
}

/// VAPP-191: `fixtures/expressions.json` (`eval`, `check`, `packages`)
/// through the free functions, exactly as the Swift (`ExpressionsTests`) and
/// Compose (`ExpressionsTest`) suites replay it; the team zone is a foreign
/// `HostZone` built from the fixture's transitions.
#[test]
fn the_expressions_fixture_replays_through_the_free_functions() {
    struct FixtureZone(i32, Vec<(f64, i32)>);
    impl HostZone for FixtureZone {
        fn offset_minutes(&self, ms: f64) -> i32 {
            self.1.iter().filter(|(at, _)| ms >= *at).last().map_or(self.0, |(_, o)| *o)
        }
    }
    let f = fixture("expressions.json");
    let instant = |s: &str| eui_core::cel::parse_timestamp(s).unwrap();
    let zone = |name: &str| -> Arc<dyn HostZone> {
        let z = &f["zones"][name];
        let steps = z["transitions"].as_array().unwrap().iter().map(|t| (instant(t[0].as_str().unwrap()), t[1].as_i64().unwrap() as i32)).collect();
        Arc::new(FixtureZone(z["offset"].as_i64().unwrap() as i32, steps))
    };
    let canon = |s: &str| eui_core::json::canonical(&serde_json::from_str::<Value>(s).unwrap());
    for c in f["eval"].as_array().unwrap() {
        let now = instant(c.get("now").unwrap_or(&f["env"]["now"]).as_str().unwrap());
        let z = zone(c.get("zone").unwrap_or(&f["env"]["zone"]).as_str().unwrap());
        let got = evaluate_expression_json(c["expr"].as_str().unwrap().into(), f["vars"].to_string(), Some(now), Some(z), c.get("maxSteps").and_then(Value::as_u64).map(|s| s as u32)).unwrap();
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "value": c["value"] }),
        };
        assert_eq!(canon(&got), eui_core::json::canonical(&want), "eval: {}", c["name"]);
    }
    for c in f["check"].as_array().unwrap() {
        let got = check_expression_json(c["expr"].as_str().unwrap().into(), f["checkTypes"].to_string()).unwrap();
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "type": c["type"] }),
        };
        assert_eq!(canon(&got), want, "check: {}", c["name"]);
    }
    for c in f["packages"].as_array().unwrap() {
        let got = validate_package_json(c["package"].to_string(), None).unwrap();
        assert_eq!(canon(&got), c["issues"], "package: {}", c["name"]);
    }
    assert!(evaluate_expression_json("1".into(), "[]".into(), None, None, None).is_err(), "vars must be an object");
}

/// VAPP-194: fixtures/actions.json through the three facade functions
/// (the whole file replays in eui-core's tests/actions.rs).
#[test]
fn the_actions_fixture_replays_through_the_facade() {
    struct Fixed(i32);
    impl HostZone for Fixed {
        fn offset_minutes(&self, _: f64) -> i32 {
            self.0
        }
    }
    let f = fixture("actions.json");
    for c in f["effects"].as_array().unwrap() {
        assert_eq!(function_effect(c["name"].as_str().unwrap().into()), c["effect"].as_str().unwrap(), "{}", c["name"]);
    }
    let pkg = f["package"].to_string();
    let now = eui_core::cel::parse_timestamp(f["env"]["now"].as_str().unwrap());
    let zone: Arc<dyn HostZone> = Arc::new(Fixed(f["env"]["offset"].as_i64().unwrap() as i32));
    let mut n = 0;
    for c in f["run"].as_array().unwrap() {
        let got = run_package_action_json(
            pkg.clone(),
            c["action"].as_str().unwrap().into(),
            c.get("params").unwrap_or(&Value::Null).to_string(),
            c.get("data").unwrap_or(&Value::Null).to_string(),
            c.get("me").and_then(Value::as_str).map(String::from),
            now,
            Some(zone.clone()),
            c.get("maxSteps").and_then(Value::as_u64).map(|s| s as u32),
            None,
        )
        .unwrap();
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "plan": c["plan"] }),
        };
        eq(&got, &want, &format!("run: {}", c["name"]));
        n += 1;
    }
    // VAPP-271: `results.run` (a pending result call, then the answered runs).
    let r = &f["results"];
    let rpkg = r["package"].to_string();
    let mut pending = 0;
    for c in r["run"].as_array().unwrap() {
        let got = run_package_action_json(rpkg.clone(), c["action"].as_str().unwrap().into(), "null".into(), "null".into(), None, now, Some(zone.clone()), None, c.get("results").map(Value::to_string)).unwrap();
        let want = if let Some(e) = c.get("error") {
            json!({ "error": e })
        } else if let Some(p) = c.get("pending") {
            pending += 1;
            json!({ "pending": p })
        } else {
            json!({ "plan": c["plan"] })
        };
        eq(&got, &want, &format!("results run: {}", c["name"]));
        n += 1;
    }
    assert!(pending >= 2, "{pending}");
    let b = &f["batch"];
    for c in b["cases"].as_array().unwrap() {
        let got = apply_record_batch_json(b["rows"].to_string(), c["writes"].to_string(), b["records"].to_string(), b["me"].as_str().map(String::from), b["now"].as_str().unwrap().into(), b["newIds"].to_string()).unwrap();
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "rows": c["rows"], "ids": c["ids"] }),
        };
        eq(&got, &want, &format!("batch: {}", c["name"]));
        n += 1;
    }
    assert!(n >= 25, "{n}");
    // New ids past the list fall back to r<n>; a non-JSON package is a typed error.
    let out: Value = serde_json::from_str(&apply_record_batch_json("[]".into(), json!([{"op": "upsert", "kind": "other", "data": {"x": 1}}, {"op": "upsert", "kind": "other", "data": {}}]).to_string(), b["records"].to_string(), None, "t".into(), "[\"a\"]".into()).unwrap()).unwrap();
    assert_eq!(out["ids"], json!(["a", "r2"]));
    assert!(run_package_action_json("{".into(), "log".into(), "null".into(), "null".into(), None, None, None, None, None).is_err());
    assert!(run_package_action_json(rpkg, "addPhotos".into(), "null".into(), "null".into(), None, None, None, None, Some("[1]".into())).is_err(), "results is an object");
}

/// VAPP-196: fixtures/package-tests.json `runs` through run_package_tests_json
/// and `validate` through validate_package_json (the whole file replays in
/// eui-core's tests/package_tests.rs).
#[test]
fn the_package_tests_fixture_replays_through_the_facade() {
    let f = fixture("package-tests.json");
    let base = f["package"].as_object().unwrap().clone();
    let mut pkg = base.clone();
    pkg.insert("tests".into(), f["runs"]["tests"].clone());
    let got = run_package_tests_json(Value::Object(pkg).to_string()).unwrap();
    eq(&got, &f["runs"]["results"], "runs");
    let mut n = 0;
    for c in f["validate"].as_array().unwrap() {
        let mut pkg = base.clone();
        for (k, v) in c["with"].as_object().unwrap() {
            pkg.insert(k.clone(), v.clone());
        }
        let got = validate_package_json(Value::Object(pkg).to_string(), None).unwrap();
        eq(&got, &c["issues"], &format!("validate: {}", c["name"]));
        n += 1;
    }
    assert!(n >= 10, "{n}");
    // No tests = no results; a malformed list runs none; a non-JSON package is a typed error.
    assert_eq!(run_package_tests_json(f["package"].to_string()).unwrap(), "[]");
    let mut bad = base.clone();
    bad.insert("tests".into(), json!([{"name": ""}]));
    assert_eq!(run_package_tests_json(Value::Object(bad).to_string()).unwrap(), "[]");
    assert!(run_package_tests_json("{".into()).is_err());
}

/// Round 9: the wire constants, the host-internal gate, the id mint, the two
/// `call` / `respond` ops and the 0.9 bridge reach the bindings.
#[test]
fn round_9_crosses_the_facade() {
    assert_eq!(a2ui_version(), "1.0");
    assert!(accept_v09() || !accept_v09());
    let router = HostRouter::new(vec![]);
    let core = core_catalog_id();
    let ops = |json: &str| serde_json::from_str::<Value>(json).unwrap();
    let bound = json!({"version": "1.0", "bindDataModel": {"surfaceId": "x", "path": "/a", "source": "exp:devices"}}).to_string();
    assert_eq!(ops(&router.route_json(bound, false))[0]["message"]["error"]["message"], "bindDataModel is host-internal");
    let created = json!({"version": "1.0", "createSurface": {"surfaceId": "s", "catalogId": core}}).to_string();
    assert_eq!(ops(&router.route_json(created.clone(), false))[0]["op"], "create");
    assert_eq!(ops(&router.route_json(created, false))[0]["message"]["error"]["code"], "INTEGRITY_ERROR");
    assert_eq!(router.mint_surface_id("s".into()), "s#1");
    let called = json!({"version": "1.0", "callRendererFunction": {"functionCallId": "c1", "callFunction": {"@call": "formatString", "catalogId": core, "args": {"value": "x"}}}}).to_string();
    let called = ops(&router.route_json(called, false));
    assert_eq!((called[0]["op"].as_str(), called[0]["functionCallId"].as_str()), (Some("call"), Some("c1")));
    let answered = json!({"version": "1.0", "agentFunctionResponse": {"functionCallId": "s:1", "value": 3}}).to_string();
    assert_eq!(ops(&router.route_json(answered, false))[0], json!({"op": "respond", "functionCallId": "s:1", "value": 3}));
    router.register_inline_catalog("https://acme.example/inline".into());
    assert!(router.available_catalog_ids().contains(&"https://acme.example/inline".to_string()));

    let caps = ops(&renderer_capabilities_json(vec![], Some("[{\"catalogId\": \"x\"}]".into())).unwrap());
    assert_eq!(caps["1.0"]["inlineCatalogs"], json!([{"catalogId": "x"}]));
    assert!(caps["1.0"]["supportedCatalogIds"].as_array().unwrap().contains(&json!(core)));
    eq(&function_error_message_json("EXPRESSION_ERROR".into(), "s:1".into(), "m".into(), None), &json!({"version": "1.0", "error": {"code": "EXPRESSION_ERROR", "functionCallId": "s:1", "message": "m"}}), "functionError");
    eq(&call_agent_function_message_json("s".into(), "s:1".into(), "lookup".into(), core.clone(), "{}".into()).unwrap(), &json!({"version": "1.0", "callAgentFunction": {"surfaceId": "s", "functionCallId": "s:1", "callFunction": {"@call": "lookup", "catalogId": core, "args": {}}}}), "callAgentFunction");
    eq(&renderer_function_value_json("c".into(), None).unwrap(), &json!({"version": "1.0", "rendererFunctionResponse": {"functionCallId": "c", "value": null}}), "value");
    eq(&renderer_function_error_json("c".into(), "EXECUTION_ERROR".into(), "m".into()), &json!({"version": "1.0", "rendererFunctionResponse": {"functionCallId": "c", "error": {"code": "EXECUTION_ERROR", "message": "m"}}}), "error");
    eq(&action_message_with_user_message_json("s".into(), "b".into(), "go".into(), "{}".into(), None, "t".into(), Some("hi".into())).unwrap(), &json!({"version": "1.0", "action": {"name": "go", "surfaceId": "s", "sourceComponentId": "b", "timestamp": "t", "context": {}, "userMessage": "hi"}}), "userMessage");

    // The two smoke cases the Swift and Kotlin suites replay too (§15).
    let fixture = fixture("upgrade09.json");
    let case = |name: &str| fixture["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).cloned().unwrap_or_else(|| panic!("{name}"));
    let client = case("client action: payload → extensions, instance id → sourceComponentId + at_exponential_instance");
    let request = json!({"kind": "client", "input": client["input"]});
    assert_eq!(upgrade09_json(request.to_string()).unwrap(), client["expected"].to_string());
    let card = fixture["cases"].as_array().unwrap().iter().find(|c| c["kind"] == "card").unwrap();
    assert_eq!(upgrade_card09(card["input"].to_string()).unwrap(), card["expected"].to_string());
    assert!(upgrade_card09("not json".into()).is_err());
}
