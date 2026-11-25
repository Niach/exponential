//! VAPP-289: `fixtures/a2ui10-functions.json` `sequences` over the core's
//! [`CallHub`] (the TS `ExponentialHost`, `scripts/generate-a2ui10.ts`).

use eui_core::catalog::CORE_CATALOG_ID;
use eui_core::data::{resolve_with_failure, ResolveContext, RoutedCall};
use eui_core::host::{route_handle, CallHub, HostRouter, PressRoute, RouteOptions};
use serde_json::{json, Map, Value};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/a2ui10-functions.json");

#[test]
fn sequences() {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    for c in doc["sequences"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let calls = CallHub::shared(c["agent"] == json!(true));
        let mut router = HostRouter::new::<&str>(&[]);
        let empty = json!({});
        let data = c.get("data").unwrap_or(&empty);
        {
            let mut hub = calls.lock().unwrap();
            hub.set_policy(c.get("policy").map(|p| serde_json::from_value(p.clone()).unwrap()));
            let names: Vec<String> = c.get("registered").and_then(Value::as_array).map(|l| l.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default();
            hub.set_registered(&names);
            let mut body = json!({"surfaceId": "s", "catalogId": CORE_CATALOG_ID, "dataModel": data});
            if c.get("sendDataModel").is_some() {
                body["sendDataModel"] = json!(true);
            }
            let options = RouteOptions { trusted: c["trusted"] == json!(true) };
            let ops = router.route(&json!({"version": "1.0", "createSurface": body}), options);
            hub.observe(&ops, &router);
        }
        let route = route_handle(&calls, "s");
        let mut sent: Vec<Value> = Vec::new();
        let mut ran: Vec<Value> = Vec::new();
        for step in c["steps"].as_array().unwrap() {
            if let Some(read) = step.get("read") {
                let hook = |call: RoutedCall| route(call);
                let ctx = ResolveContext::new(data, "").with_catalog(CORE_CATALOG_ID, None).with_route(&hook);
                let (value, failure) = resolve_with_failure(read, &ctx);
                let mut got = Map::new();
                got.insert("value".into(), value.unwrap_or(Value::Null));
                if let Some(failure) = failure {
                    calls.lock().unwrap().report_failure("s", &failure, None);
                    got.insert("failure".into(), serde_json::to_value(&failure).unwrap());
                }
                assert_eq!(Value::Object(got), step["expected"], "{name}: {read}");
            } else if let Some(press) = step.get("press").or_else(|| step.get("pressAsync")) {
                let press = press.as_str().unwrap();
                let args = step.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
                let mut hub = calls.lock().unwrap();
                let mut outcome = hub.press("s", press, None, &args, None);
                if outcome == PressRoute::Ask {
                    outcome = hub.press("s", press, None, &args, Some(c["consent"] == json!(true)));
                }
                if outcome == PressRoute::Local {
                    ran.push(json!({"name": press, "args": args}));
                }
                if let Some(expected) = step.get("expected") {
                    let decision = match outcome {
                        PressRoute::Deny => "deny",
                        PressRoute::NotFound => "not_found",
                        _ => "allow",
                    };
                    assert_eq!(expected["decision"], decision, "{name}: {press}");
                }
            } else {
                let ops = router.route(&json!({"version": "1.0", "agentFunctionResponse": step["respond"]}), RouteOptions::default());
                calls.lock().unwrap().observe(&ops, &router);
            }
            let hub = &mut *calls.lock().unwrap();
            for message in hub.take_outbox() {
                // §1.3: `{message, metadata}` when it has metadata.
                sent.push(match hub.data_model_metadata(&message, &|_| Some(data.clone())) {
                    Some(metadata) => json!({"message": message, "metadata": metadata}),
                    None => message,
                });
            }
        }
        assert_eq!(Value::Array(sent), c["expectedSent"], "{name}: sent");
        assert_eq!(Value::Array(ran), c["expectedRan"], "{name}: ran");
    }
}

#[test]
fn a_timeout_is_a_failure_with_its_id() {
    let calls = CallHub::shared(true);
    let mut router = HostRouter::new::<&str>(&[]);
    let mut hub = calls.lock().unwrap();
    hub.clock_ms = Some(1000.0);
    let ops = router.route(&json!({"version": "1.0", "createSurface": {"surfaceId": "s", "catalogId": CORE_CATALOG_ID}}), RouteOptions::default());
    hub.observe(&ops, &router);
    let PressRoute::Agent { function_call_id } = hub.press("s", "lookup", None, &Map::new(), None) else { panic!() };
    hub.take_outbox();
    assert_eq!(hub.next_deadline_ms(), Some(31000.0));
    hub.clock_ms = Some(31000.0);
    let settled = hub.expire();
    assert_eq!(settled[0].function_call_id, function_call_id);
    assert_eq!(settled[0].error.as_deref(), Some("lookup timed out"));
    assert_eq!(hub.take_outbox(), vec![json!({"version": "1.0", "error": {"code": "EXPRESSION_ERROR", "functionCallId": "s:1", "message": "lookup timed out"}})]);
}

#[test]
fn issues_past_the_cap_are_summed_up_once() {
    let mut router = HostRouter::new::<&str>(&[]);
    let mut hub = CallHub::new(false);
    let ops = router.route(&json!({"version": "1.0", "createSurface": {"surfaceId": "s", "catalogId": CORE_CATALOG_ID}}), RouteOptions::default());
    hub.observe(&ops, &router);
    let issues: Vec<_> = (0..35).map(|i| eui_core::ReduceIssue::new(format!("c{i}"), "bad")).collect();
    hub.send_issues("s", &issues);
    let out = hub.take_outbox();
    assert_eq!(out.len(), 33);
    assert_eq!(out[32]["error"]["message"], "3 more issues were dropped");
    hub.send_issues("s", &issues);
    assert_eq!(hub.take_outbox().len(), 3, "the rest, each once");
}

/// Round 9 §5.5: a basic surface's calls resolve to the basic catalog on the
/// wire; its reduced tree still reads the core's functions.
#[test]
fn a_call_on_a_basic_surface_names_the_basic_catalog() {
    use eui_core::surface::{Surface, SurfaceOptions};
    let basic = eui_core::A2UI_BASIC_CATALOG_ID;
    let calls = CallHub::shared(true);
    let mut router = HostRouter::new::<&str>(&[]);
    let ops = router.route(&json!({"version": "1.0", "createSurface": {"surfaceId": "s", "catalogId": basic}}), RouteOptions::default());
    calls.lock().unwrap().observe(&ops, &router);
    let mut s = Surface::new("s", SurfaceOptions { catalog_id: basic.into(), ..SurfaceOptions::default() });
    s.set_call_route(Some(route_handle(&calls, "s")));
    s.apply(&json!({"version": "1.0", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Text", "text": {"@call": "lookup"}, "visible": {"@call": "cond", "args": {"if": true, "then": true, "else": false}}}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut eui_core::measure::FixedMeasure { sizes: Default::default(), wrap: true });
    assert!(s.take_failures().is_empty());
    let sent = calls.lock().unwrap().take_outbox();
    assert_eq!(sent[0]["callAgentFunction"]["callFunction"]["catalogId"], basic);
}
