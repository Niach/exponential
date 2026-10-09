//! VAPP-91: the host fixtures (`fixtures/host-{transport,policy,router}.json`)
//! replayed by the Rust core's `host` module with the structure of
//! `packages/exponential-ui/src/host/host.test.ts`, the contract constants
//! gated against `catalog/host.json`, and the surface's `functionCall` events.

mod support;

use exponential_ui::host::{self, HOST_ERROR_CODES, OP_KINDS};
use exponential_ui::surface::{OutEvent, Surface, SurfaceOptions};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};
use support::{fixture, read, Case};

fn assert_all(cases: Vec<Case>, expected_count: usize) {
    assert_eq!(cases.len(), expected_count);
    let failed: Vec<String> = cases.iter().filter_map(|(name, r)| r.as_ref().err().map(|e| format!("{name}: {e}"))).collect();
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}

fn count(file: &str, keys: &[&str]) -> usize {
    let f = fixture(file);
    keys.iter().map(|k| f[*k].as_array().unwrap().len()).sum()
}

#[test]
fn host_transport_every_decoder_case_equals_the_reference() {
    assert_all(support::transport_cases(), count("host-transport.json", &["jsonl", "sse", "mcp", "mcpAction"]));
}

#[test]
fn host_policy_every_function_url_media_source_and_negotiation_case_equals_the_reference() {
    assert_all(support::policy_cases(), count("host-policy.json", &["functions", "combine", "urls", "media", "sources", "negotiation"]));
}

#[test]
fn host_router_every_validation_and_flow_equals_the_reference() {
    assert_all(support::router_cases(), count("host-router.json", &["validation", "flows"]));
}

fn strip_comments(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(o.iter().filter(|(k, _)| *k != "$comment").map(|(k, v)| (k.clone(), strip_comments(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(strip_comments).collect()),
        other => other.clone(),
    }
}

#[test]
fn the_contract_constants_match_catalog_host_json() {
    let contract = strip_comments(&read("catalog/host.json"));
    if let Err(e) = support::same(&host::host_contract(), &contract) {
        panic!("src/host/contract.rs drifted from catalog/host.json: {e}");
    }
    let messages = &contract["messages"];
    let all: Vec<&str> = messages["a2ui"].as_array().unwrap().iter().chain(messages["extensions"].as_array().unwrap()).map(|v| v.as_str().unwrap()).collect();
    assert_eq!(host::MESSAGE_KINDS, all.as_slice());
    for code in HOST_ERROR_CODES {
        assert!([host::VALIDATION_FAILED, host::INVALID_MESSAGE, host::UNSUPPORTED_CATALOG, host::SURFACE_NOT_FOUND, host::TEMPLATE_NOT_FOUND, host::FUNCTION_NOT_FOUND, host::FUNCTION_DENIED, host::RENDER_FAILED].contains(code));
    }
    assert_eq!(contract["a2uiVersion"], json!(exponential_ui::catalog::A2UI_VERSION));
}

#[test]
fn the_built_ins_are_the_catalogs() {
    let catalog = read("catalog/core.catalog.json");
    let names: Vec<&str> = catalog["functions"]["names"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(host::BUILTIN_FUNCTIONS, names.as_slice());
    // The basic catalog's 14 plus the 17 core functions (`set` incl.; round 2
    // adds formatPercent + formatRelativeTime).
    assert_eq!(host::BUILTIN_FUNCTIONS.len(), 32);
}

#[test]
fn every_error_code_the_router_emits_is_in_the_contract() {
    let f = fixture("host-router.json");
    for flow in f["flows"].as_array().unwrap() {
        for step in flow["steps"].as_array().unwrap() {
            for op in step["expected"].as_array().unwrap() {
                assert!(OP_KINDS.contains(&op["op"].as_str().unwrap()));
                if let Some(code) = op["message"]["error"]["code"].as_str() {
                    assert!(HOST_ERROR_CODES.contains(&code), "{code}");
                }
            }
        }
    }
    let mut kinds = OP_KINDS.to_vec();
    kinds.sort();
    assert_eq!(kinds, ["bind", "components", "create", "data", "delete", "send"]);
}

#[test]
fn the_router_tracks_surfaces_packages_and_extensions() {
    let f = fixture("host-router.json");
    let mut router = host::HostRouter::new::<&str>(&[]);
    assert!(router.install_package(&f["packages"]["acme.devices"]).is_empty());
    router.route(&json!({"version": "v0.9", "applyTemplate": {"surfaceId": "d", "templateId": "list"}}));
    router.route(&json!({"version": "v0.9", "createSurface": {"surfaceId": "free", "catalogId": exponential_ui::CORE_CATALOG_ID}}));
    assert_eq!(router.surface_ids(), ["d", "free"]);
    assert_eq!(router.package_id_of("d").as_deref(), Some("acme.devices"));
    assert_eq!(router.package_id_of("free"), None);
    router.register_extension("https://acme.example/catalog/v1");
    router.register_extension("https://acme.example/catalog/v1");
    assert_eq!(router.supported_catalog_ids().last().map(String::as_str), Some("https://acme.example/catalog/v1"));
    assert_eq!(router.supported_catalog_ids().len(), 4);
    router.route(&json!({"version": "v0.9", "deleteSurface": {"surfaceId": "d"}}));
    assert_eq!(router.surface_ids(), ["free"]);
}

#[test]
fn client_messages_match_the_reference_shapes() {
    assert_eq!(
        host::action_message("s", "btn", "save", json!({"id": 1}), Some(json!({"value": "a"})), "2026-10-08T00:00:00.000Z"),
        json!({"version": "v0.9", "action": {"name": "save", "surfaceId": "s", "sourceComponentId": "btn", "timestamp": "2026-10-08T00:00:00.000Z", "context": {"id": 1}, "payload": {"value": "a"}}})
    );
    assert_eq!(
        host::action_message("s", "btn", "save", Value::Null, Some(json!({})), "t"),
        json!({"version": "v0.9", "action": {"name": "save", "surfaceId": "s", "sourceComponentId": "btn", "timestamp": "t", "context": {}}})
    );
    assert_eq!(
        host::error_message("VALIDATION_FAILED", "s", "no resolver for the source scheme acme", Some("/x")),
        json!({"version": "v0.9", "error": {"code": "VALIDATION_FAILED", "surfaceId": "s", "message": "no resolver for the source scheme acme", "path": "/x"}})
    );
    assert_eq!(host::error_message("FUNCTION_DENIED", "s", "m", None)["error"].get("path"), None);
    let policy = host::package_policy(Some(&["harness.toast".to_string()]));
    assert_eq!(serde_json::to_value(&policy).unwrap(), json!({"allow": ["harness.toast"], "default": "deny"}));
    assert_eq!(host::decide_function(Some(&policy), "harness.mcp", true), host::FunctionDecision::Deny);
    assert_eq!(host::normalize_path(Some("/")), "");
    assert_eq!(host::merge_data(Some(&json!({"a": 1, "b": 1})), Some(&json!({"b": 2}))), Some(json!({"a": 1, "b": 2})));
    assert_eq!(host::merge_data(Some(&json!({"a": 1})), None), Some(json!({"a": 1})));
}

// ---------------------------------------------------------------------------
// The surface's `functionCall` actions
// ---------------------------------------------------------------------------

fn button_surface(on_press: Value) -> (Surface, u32) {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"},
        "children": [{"id": "b", "component": "Button", "props": {"label": "Go"}, "on": {"press": on_press}}]
    }))
    .unwrap();
    let mut surface = Surface::new("s", SurfaceOptions::default());
    let outcome = surface.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    surface.set_data("/m", Some(json!("hi")));
    surface.set_viewport(400.0, 0.0, None);
    surface.layout(&mut exponential_ui::measure::FixedMeasure::default());
    let index = surface.index_of("b").unwrap();
    (surface, index)
}

#[test]
fn a_host_function_call_fires_function_call_with_the_resolved_args() {
    let (mut surface, b) = button_surface(json!({"functionCall": {"call": "harness.toast", "args": {"message": {"path": "/m"}, "n": 2}}}));
    let events = surface.event(b, "press", None);
    let call = events.iter().find(|e| matches!(e, OutEvent::FunctionCall { .. })).expect("a FunctionCall");
    assert_eq!(call, &OutEvent::FunctionCall { component_id: "b".into(), name: "harness.toast".into(), args: json!({"message": "hi", "n": 2}) });
    assert_eq!(serde_json::to_value(call).unwrap(), json!({"kind": "functionCall", "componentId": "b", "name": "harness.toast", "args": {"message": "hi", "n": 2}}));
    // No args = `{}`; a legacy `function` key is not an action (round 4).
    let (mut surface, b) = button_surface(json!({"functionCall": {"call": "acme.refresh"}}));
    let events = surface.event(b, "press", None);
    assert!(events.contains(&OutEvent::FunctionCall { component_id: "b".into(), name: "acme.refresh".into(), args: json!({}) }));
    let (mut surface, b) = button_surface(json!({"function": {"call": "acme.refresh"}}));
    assert!(!surface.event(b, "press", None).iter().any(|e| matches!(e, OutEvent::FunctionCall { .. })));
}

#[test]
fn open_url_through_function_call_is_still_open_url_and_other_built_ins_do_nothing() {
    let (mut surface, b) = button_surface(json!({"functionCall": {"call": "openUrl", "args": {"url": "https://exponential.at"}}}));
    let events = surface.event(b, "press", None);
    assert!(events.contains(&OutEvent::OpenUrl { url: "https://exponential.at".into() }));
    assert!(!events.iter().any(|e| matches!(e, OutEvent::FunctionCall { .. })));
    let (mut surface, b) = button_surface(json!({"functionCall": {"call": "formatString", "args": {"value": "x"}}}));
    let events = surface.event(b, "press", None);
    assert!(!events.iter().any(|e| matches!(e, OutEvent::FunctionCall { .. } | OutEvent::OpenUrl { .. })));
}

#[test]
fn an_event_and_a_function_call_on_one_handler_both_fire() {
    let (mut surface, b) = button_surface(json!({"event": {"name": "go"}, "functionCall": {"call": "harness.toast", "args": {"message": "x"}}}));
    let events = surface.event(b, "press", None);
    assert!(events.iter().any(|e| matches!(e, OutEvent::Action { name, .. } if name == "go")));
    assert!(events.iter().any(|e| matches!(e, OutEvent::FunctionCall { name, .. } if name == "harness.toast")));
}

#[test]
fn a_basic_catalog_button_action_function_call_reaches_the_host() {
    let mut surface = Surface::new("s1", SurfaceOptions { catalog_id: exponential_ui::A2UI_BASIC_CATALOG_ID.into(), ..SurfaceOptions::default() });
    let outcome = surface
        .apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s1", "components": [
            {"id": "root", "component": "Column", "children": ["btn"]},
            {"id": "btn", "component": "Button", "child": "l", "action": {"functionCall": {"call": "harness.toast", "args": {"message": "hey"}}}},
            {"id": "l", "component": "Text", "text": "Go"}
        ]}}))
        .unwrap();
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    surface.set_viewport(400.0, 0.0, None);
    surface.layout(&mut exponential_ui::measure::FixedMeasure::default());
    let btn = surface.index_of("btn").unwrap();
    let events = surface.event(btn, "press", None);
    assert!(events.contains(&OutEvent::FunctionCall { component_id: "btn".into(), name: "harness.toast".into(), args: json!({"message": "hey"}) }), "{events:?}");
}
