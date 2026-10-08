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
                FfiIntrinsics { min_content_width: min, max_content_width: w, height_at_max_content: h }
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
