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
        let p = place_overlay(a["x"].as_f64().unwrap(), a["y"].as_f64().unwrap(), a["width"].as_f64().unwrap(), a["height"].as_f64().unwrap(), s["width"].as_f64().unwrap(), s["height"].as_f64().unwrap(), v["width"].as_f64().unwrap(), v["height"].as_f64().unwrap(), c["side"].as_str().unwrap().into()).unwrap();
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
    surface.apply(json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": ["dlg", "t"]},
        {"id": "t", "component": "Text", "text": {"path": "/title"}},
        {"id": "dlg", "component": "Dialog", "title": "Hi", "open": {"path": "/open"}, "slots": {"trigger": "b"}, "children": ["body"]},
        {"id": "b", "component": "Button", "label": "Open", "on": {"press": {"event": {"name": "opened"}}}},
        {"id": "body", "component": "Text", "text": "Body"}
    ]}}).to_string()).unwrap();
    surface.set_data("/title".into(), Some("\"Hello\"".into())).unwrap();
    surface.set_viewport(400.0, 700.0, None);
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
    assert!(surface.set_theme_json(json!({"id": "x", "name": "X", "extends": "neutral"}).to_string()).is_ok());
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
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || s2.layout(m2)));
    assert!(caught.is_err(), "the measurer's panic propagates");
    for _ in 0..2 {
        let out = surface.layout(m.clone());
        assert!(!out.reentrant, "not mistaken for a re-entrant call");
        assert!(!out.frames.is_empty() && out.surface_height >= 20.0, "a real layout: {} frames", out.frames.len());
    }
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
    assert_eq!(t["delay_ms"], json!(150));
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
    let loaded = Theme::load(r##"{"id":"t","name":"T","extends":"neutral","modes":{"light":{"color":{"primary":"#ff0000"}},"dark":{"color":{"primary":"#00ff00"}}}}"##.into(), None).unwrap();
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
        let got = mcp_action_call_json(c["message"].to_string(), c.get("tool").and_then(Value::as_str).map(str::to_string)).unwrap();
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
        eq(&client_capabilities_json(ids), &c["expected"]["clientCapabilities"], "clientCapabilities");
    }
    assert!(combine_decisions("allow".into(), "maybe".into()).is_err());
    eq(&package_policy_json(Some("[\"harness.toast\"]".into())).unwrap(), &json!({"allow": ["harness.toast"], "default": "deny"}), "packagePolicy");
}

#[test]
fn the_host_router_fixture_replays_through_the_facade() {
    let f = fixture("host-router.json");
    let packages = &f["packages"];
    for v in f["validation"].as_array().unwrap() {
        let id = v["package"].as_str().unwrap();
        eq(&validate_package_json(packages[id].to_string(), None).unwrap(), &v["expected"], id);
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
            eq(&router.route(step["message"].to_string()), &step["expected"], &format!("{name}: step {i}"));
        }
    }
    // State + the client messages.
    let router = HostRouter::new(vec![]);
    router.install_package(packages["acme.devices"].to_string()).unwrap();
    router.route(json!({"version": "v0.9", "applyTemplate": {"surfaceId": "d", "templateId": "list"}}).to_string());
    assert_eq!(router.surface_ids(), ["d"]);
    assert_eq!(router.package_id_of("d".into()).as_deref(), Some("acme.devices"));
    router.register_extension("https://acme.example/catalog/v1".into());
    assert_eq!(router.supported_catalog_ids().len(), 4);
    let ops: Value = serde_json::from_str(&router.route("not json".into())).unwrap();
    assert_eq!(ops[0]["message"]["error"]["code"], "INVALID_MESSAGE");
    let msgs = template_messages_json(packages["acme.devices"].to_string(), "list".into(), "x".into(), Some("{\"filter\":\"online\"}".into())).unwrap().unwrap();
    let msgs: Value = serde_json::from_str(&msgs).unwrap();
    assert_eq!(msgs.as_array().unwrap().len(), 4);
    assert_eq!(msgs[2]["updateDataModel"]["value"], json!({"title": "Devices", "filter": "online"}));
    assert!(template_messages_json(packages["acme.devices"].to_string(), "nope".into(), "x".into(), None).unwrap().is_none());
    eq(
        &action_message_json("s".into(), "btn".into(), "save".into(), "{\"id\":1}".into(), Some("{\"value\":\"a\"}".into()), "t".into()).unwrap(),
        &json!({"version": "v0.9", "action": {"name": "save", "surfaceId": "s", "sourceComponentId": "btn", "timestamp": "t", "context": {"id": 1}, "payload": {"value": "a"}}}),
        "actionMessage",
    );
    eq(&error_message_json("FUNCTION_DENIED".into(), "s".into(), "m".into(), None), &json!({"version": "v0.9", "error": {"code": "FUNCTION_DENIED", "surfaceId": "s", "message": "m"}}), "errorMessage");
    let contract: Value = serde_json::from_str(&host_contract_json()).unwrap();
    assert_eq!(contract["transport"]["mcpActionTool"], "a2ui_event");
}

#[test]
fn a_host_function_call_crosses_the_facade_as_a_function_call_event() {
    let surface = Surface::new("s".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface
        .set_nested(
            json!({"id": "root", "component": "Box", "children": [
                {"id": "b", "component": "Button", "props": {"label": "Go"}, "on": {"press": {"functionCall": {"call": "harness.toast", "args": {"message": {"path": "/m"}}}}}}
            ]})
            .to_string(),
        )
        .unwrap();
    surface.set_data("/m".into(), Some("\"hi\"".into())).unwrap();
    surface.set_viewport(400.0, 0.0, None);
    surface.layout_fixed(None, false).unwrap();
    let events = surface.event(surface.index_of("b".into()).unwrap(), "press".into(), None).unwrap();
    let call = events.iter().find(|e| e.kind == "functionCall").expect("functionCall");
    eq(&call.json, &json!({"kind": "functionCall", "componentId": "b", "name": "harness.toast", "args": {"message": "hi"}}), "functionCall");
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
    assert_eq!(call(json!({"call": "formatNumber", "args": {"value": 3, "decimals": 2}}), None), json!("#3/2").to_string());
    assert_eq!(call(json!({"call": "formatCurrency", "args": {"value": 3, "currency": "eur"}}), None), json!("EUR3").to_string());
    assert_eq!(call(json!({"call": "formatDate", "args": {"value": "2026-10-14", "style": "long", "time": true}}), None), json!("1791936000000|true||long|true").to_string());
    assert_eq!(call(json!({"call": "formatRelativeTime", "args": {"value": 0}}), Some(7_200_000.0)), json!("-2 hour").to_string());
    assert_eq!(call(json!({"call": "pluralize", "args": {"value": 2, "two": "a pair", "other": "many"}}), None), json!("a pair").to_string());
    // On a surface: Table cells and bound format calls.
    let surface = Surface::new("f".into(), core_catalog_id(), None, "light".into()).unwrap();
    surface
        .set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "n", "component": "Text", "props": {"text": {"call": "formatNumber", "args": {"value": 1.5}}}}]}).to_string())
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
    zoned.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Text", "props": {"text": {"call": "formatDate", "args": {"value": "2026-10-14T22:30:00Z", "format": "yyyy-MM-dd HH:mm"}}}}]}).to_string()).unwrap();
    zoned.set_fallback_zone(Some(Arc::new(Berlin)));
    zoned.set_viewport(390.0, 0.0, None);
    zoned.layout_fixed(None, true).unwrap();
    assert!(zoned.nodes().into_iter().find(|n| n.id == "d").unwrap().props_json.contains("2026-10-15 00:30"));
    assert!(!zoned.uses_clock(), "no relative time on screen");
    let ticking = Surface::new("c".into(), core_catalog_id(), None, "light".into()).unwrap();
    ticking.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "r", "component": "Text", "props": {"text": {"call": "formatRelativeTime", "args": {"value": 0}}}}]}).to_string()).unwrap();
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
    utc.set_nested(json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Text", "props": {"text": {"call": "formatDate", "args": {"value": "2026-10-14T22:30:00Z", "format": "yyyy-MM-dd HH:mm"}}}}]}).to_string()).unwrap();
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
    let header = json!({"id": "h", "component": "Text", "props": {"text": {"call": "concat", "args": {"values": [{"path": "value"}, " · ", {"path": "count"}, " · ", {"path": "/title"}]}}}});
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
    let slot = json!({"id": "c", "component": "Text", "props": {"text": {"call": "formatNumber", "args": {"value": {"path": "n"}}}}});
    let rows = json!([{"n": 2.5}]);
    let got: Value = serde_json::from_str(&bind_row_slot_json(slot.to_string(), rows.to_string(), rows.to_string(), 0, "{}".into(), None, Some(Arc::new(Loud))).unwrap().unwrap()).unwrap();
    assert_eq!(got["props"]["text"], json!("#2.5/"));
    let got: Value = serde_json::from_str(&bind_row_slot_json(slot.to_string(), rows.to_string(), rows.to_string(), 0, "{}".into(), None, None).unwrap().unwrap()).unwrap();
    assert_eq!(got["props"]["text"], json!("2.5"), "None = the English fallback");
    // A section header: the formatter and `options.now`.
    let header = json!({"id": "h", "component": "Text", "props": {"text": {"call": "formatRelativeTime", "args": {"value": 0}}}});
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
        r#"{"id":"caps","name":"Caps","extends":"neutral","recipes":{"Text":{"root":[{"when":{"variant":"caption"},"style":{"letterSpacing":0.5,"textTransform":"uppercase","fontStyle":"italic"}}]}}}"#.into(),
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
        {"id": "n", "component": "Text", "props": {"text": {"path": "/n"}}}
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
