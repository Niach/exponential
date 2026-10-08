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
        {"id": "hc", "component": "HoverCard", "slots": {"trigger": {"id": "who", "component": "Text", "props": {"text": "@ada"}}}, "children": [{"id": "card", "component": "Text", "props": {"text": "Ada"}}]}]}).to_string()).unwrap();
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
