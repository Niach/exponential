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
            let options = RouteOptions { trusted: c["trusted"] == json!(true), ..Default::default() };
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
            sent.extend(calls.lock().unwrap().take_outbox());
        }
        // `sendDataModel rides beside the message`: the TS recording ended
        // before its un-awaited press sent anything; here it left at once.
        if name == "sendDataModel rides beside the message" {
            let message = &sent[0];
            let meta = calls.lock().unwrap().data_model_metadata(message, &|_| Some(data.clone()));
            assert_eq!(meta, Some(json!({"a2uiRendererDataModel": {"version": "1.0", "surfaces": {"s": {"a": 1}}}})));
            continue;
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
