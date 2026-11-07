//! VAPP-289 (round 9 §15): replays `fixtures/a2ui10-messages.json`,
//! `a2ui10-functions.json` (its `sequences` in `calls_fixture.rs`) and
//! `a2ui10-conformance.json`, byte for byte.

use eui_core::catalog::{check_catalog_definition, CORE_CATALOG_ID};
use eui_core::data::{check_result, evaluate_check, resolve_with_failure, shown_result, Position, ResolveContext};
use eui_core::expr::{coerce_boolean, present_opt};
use eui_core::host::{
    action_message_with, call_agent_function_message, decide_route, error_message_to, renderer_capabilities, renderer_function_response, routed_call_key, ErrorTarget, HostRouter, RouteDecisionInput,
    RouteOptions,
};
use eui_core::ident::{is_identifier, mangle};
use eui_core::reducer::{card_problems, card_v10, reduce_nested, ReduceOptions};
use eui_core::NestedNode;
use serde_json::{json, Map, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()).unwrap()
}

fn list<'a>(v: &'a Value, k: &str) -> &'a Vec<Value> {
    v[k].as_array().unwrap_or_else(|| panic!("{k}"))
}

fn bytes(v: &Value) -> String {
    serde_json::to_string(&eui_core::json::canonical(v)).unwrap()
}

/// A case's difference, byte level.
fn check(failed: &mut Vec<String>, name: &str, got: &Value, want: &Value) {
    if bytes(got) != bytes(want) {
        failed.push(format!("{name}:\n  got  {}\n  want {}", bytes(got), bytes(want)));
    }
}

fn done(failed: Vec<String>) {
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

#[test]
fn ids() {
    let mut failed = Vec::new();
    for c in list(&fixture("a2ui10-messages.json"), "ids") {
        let id = c["id"].as_str().unwrap();
        check(&mut failed, id, &json!({"identifier": is_identifier(id), "mangle": mangle(id)}), &c["expected"]);
    }
    done(failed);
}

#[test]
fn flows() {
    let f = fixture("a2ui10-messages.json");
    let mut failed = Vec::new();
    for flow in list(&f, "flows") {
        let name = flow["name"].as_str().unwrap();
        let mut router = HostRouter::new(&strs(&flow["extensionIds"]));
        for id in strs(&flow["inlineCatalogs"]) {
            router.register_inline_catalog(&id);
        }
        for id in strs(&flow["packages"]) {
            let issues = router.install_package(&f["packages"][&id]);
            check(&mut failed, &format!("{name}: install {id}"), &serde_json::to_value(issues).unwrap(), &flow["installIssues"][&id]);
        }
        for (i, s) in list(flow, "steps").iter().enumerate() {
            let mut options = RouteOptions::default();
            options.trusted = s["trusted"] == json!(true);
            if s["acceptV09"] == json!(false) {
                options.accept_v09 = false;
            }
            check(&mut failed, &format!("{name} [{i}] {}", s["message"].to_string().chars().take(90).collect::<String>()), &Value::Array(router.route(&s["message"], options)), &s["expected"]);
        }
    }
    done(failed);
}

#[test]
fn mint() {
    let mut failed = Vec::new();
    for c in list(&fixture("a2ui10-messages.json"), "mint") {
        let mut r = HostRouter::new::<&str>(&[]);
        for s in list(c, "steps") {
            let op_names = |ops: Vec<Value>| Value::Array(ops.iter().map(|o| o["op"].clone()).collect());
            let got = if let Some(id) = s.get("create") {
                op_names(r.route(&json!({"version": "1.0", "createSurface": {"surfaceId": id, "catalogId": CORE_CATALOG_ID}}), RouteOptions::default()))
            } else if let Some(id) = s.get("delete") {
                op_names(r.route(&json!({"version": "1.0", "deleteSurface": {"surfaceId": id}}), RouteOptions::default()))
            } else {
                json!(r.mint_surface_id(s["mint"].as_str().unwrap()))
            };
            check(&mut failed, &s.to_string(), &got, &s["expected"]);
        }
    }
    done(failed);
}

#[test]
fn client_messages() {
    let f = fixture("a2ui10-messages.json");
    let mut failed = Vec::new();
    for c in list(&f, "errors") {
        let target = match c["target"].get("functionCallId").and_then(Value::as_str) {
            Some(id) => ErrorTarget::FunctionCall(id),
            None => ErrorTarget::Surface(c["target"]["surfaceId"].as_str().unwrap()),
        };
        let got = error_message_to(c["code"].as_str().unwrap(), target, c["message"].as_str().unwrap(), c.get("path").and_then(Value::as_str));
        check(&mut failed, c["name"].as_str().unwrap(), &got, &c["expected"]);
    }
    for c in list(&f, "actions") {
        let i = &c["input"];
        let s = |k: &str| i[k].as_str().unwrap();
        let got = action_message_with(s("surfaceId"), s("sourceComponentId"), s("name"), i["context"].clone(), i.get("payload").cloned(), s("timestamp"), i.get("userMessage").and_then(Value::as_str));
        check(&mut failed, c["name"].as_str().unwrap(), &got, &c["expected"]);
    }
    for c in list(&f, "calls") {
        let s = |k: &str| c[k].as_str().unwrap();
        let got = call_agent_function_message(s("surfaceId"), s("functionCallId"), s("name"), s("catalogId"), c["args"].as_object().unwrap().clone());
        check(&mut failed, c["title"].as_str().unwrap(), &got, &c["expected"]);
    }
    for c in list(&f, "responses") {
        let id = c["functionCallId"].as_str().unwrap();
        let r = &c["result"];
        let got = match r.get("error") {
            Some(e) => renderer_function_response(id, Err((e["code"].as_str().unwrap(), e["message"].as_str().unwrap()))),
            None => renderer_function_response(id, Ok(r.get("value").cloned())),
        };
        check(&mut failed, c["name"].as_str().unwrap(), &got, &c["expected"]);
    }
    for c in list(&f, "capabilities") {
        let got = renderer_capabilities(&strs(&c["extensionIds"]), c.get("inline").and_then(Value::as_array).cloned());
        check(&mut failed, c["name"].as_str().unwrap(), &got, &c["expected"]);
    }
    done(failed);
}

#[test]
fn cards() {
    let mut failed = Vec::new();
    for c in list(&fixture("a2ui10-messages.json"), "cards") {
        let name = c["name"].as_str().unwrap();
        let got = match card_v10(&c["input"], c.get("acceptV09").and_then(Value::as_bool).unwrap_or(true)) {
            Err(error) => json!({"error": error}),
            Ok(card) => {
                let root: NestedNode = serde_json::from_value(card["root"].clone()).unwrap();
                let r = reduce_nested(&root, &ReduceOptions::new(CORE_CATALOG_ID));
                // `root` compares as JSON: `UiNode` serializes in field order.
                let root = serde_json::to_value(&r.root).unwrap();
                if !eui_core::json::equal(&root, &c["expected"]["root"]) {
                    failed.push(format!("{name}: root:\n  got  {root}\n  want {}", c["expected"]["root"]));
                }
                json!({"card": card, "problems": card_problems(&card["root"]), "issues": r.issues})
            }
        };
        let mut want = c["expected"].clone();
        if let Some(o) = want.as_object_mut() {
            o.shift_remove("root");
        }
        check(&mut failed, name, &got, &want);
    }
    done(failed);
}

#[test]
fn conformance() {
    let mut failed = Vec::new();
    let doc = fixture("a2ui10-conformance.json");
    for c in list(&doc, "cases") {
        let name = format!("{} #{} {}", c["file"].as_str().unwrap(), c["index"], c["description"].as_str().unwrap());
        let got = match c["expect"].as_str().unwrap() {
            "skip" => continue,
            "catalog-ok" | "catalog-error" => {
                let issues = check_catalog_definition(&c["data"], false);
                if issues.is_empty() { "catalog-ok".to_string() } else { "catalog-error".to_string() }
            }
            _ => {
                let mut r = HostRouter::new::<&str>(&[]);
                let messages = c["data"].as_array().cloned().unwrap_or_else(|| vec![c["data"].clone()]);
                // A case without its createSurface runs on a surface of the
                // catalog it tests (the fixture's generator).
                for m in &messages {
                    let sid = ["updateComponents", "updateDataModel", "deleteSurface"].iter().find_map(|k| m.get(*k).and_then(|b| b.get("surfaceId")).and_then(Value::as_str));
                    if let Some(sid) = sid.filter(|s| !r.was_created(s)) {
                        r.route(&json!({"version": "1.0", "createSurface": {"surfaceId": sid, "catalogId": eui_core::A2UI_BASIC_CATALOG_ID}}), RouteOptions::default());
                    }
                }
                let mut first = "ok".to_string();
                for m in &messages {
                    let ops = r.route(m, RouteOptions::default());
                    let code = ops.iter().find_map(|op| (op["op"] == "send").then(|| op["message"]["error"]["code"].as_str()).flatten());
                    if let (Some(code), "ok") = (code, first.as_str()) {
                        first = code.to_string();
                    }
                }
                first
            }
        };
        check(&mut failed, &name, &json!(got), &c["expect"]);
    }
    done(failed);
}

#[test]
fn coercion() {
    let mut failed = Vec::new();
    for c in list(&fixture("a2ui10-functions.json"), "coercion") {
        let v = Some(&c["value"]);
        check(&mut failed, &c["value"].to_string(), &json!({"boolean": coerce_boolean(v), "present": present_opt(v)}), &c["expected"]);
    }
    done(failed);
}

fn scope_string(c: &Value) -> String {
    c["scope"]["base"].as_str().unwrap_or("").to_string()
}

#[test]
fn checks_and_results() {
    let f = fixture("a2ui10-functions.json");
    let mut failed = Vec::new();
    let empty = json!({});
    for c in list(&f, "checks") {
        let data = c.get("data").unwrap_or(&empty);
        let got = evaluate_check(&c["rule"], &ResolveContext::new(data, "").with_catalog(CORE_CATALOG_ID, None));
        check(&mut failed, c["name"].as_str().unwrap(), &got, &c["expected"]);
    }
    for c in list(&f, "results") {
        check(&mut failed, c["name"].as_str().unwrap(), &check_result(c.get("condition"), c.get("message"), None), &c["expected"]);
    }
    for c in list(&f, "shown") {
        let results = c["results"].as_array().unwrap();
        check(&mut failed, c["name"].as_str().unwrap(), &shown_result(results).cloned().unwrap_or(Value::Null), &c["expected"]);
    }
    done(failed);
}

#[test]
fn evaluations() {
    let mut failed = Vec::new();
    let empty = json!({});
    for c in list(&fixture("a2ui10-functions.json"), "evaluations") {
        let data = c.get("data").unwrap_or(&empty);
        let scope = scope_string(c);
        let available: Option<Vec<String>> = c.get("available").map(strs);
        let mut ctx = ResolveContext::new(data, &scope).with_catalog(c.get("catalogId").and_then(Value::as_str).unwrap_or(CORE_CATALOG_ID), available.as_deref());
        if let Some(i) = c["scope"].get("index").and_then(Value::as_u64) {
            ctx = ctx.with_index(i as usize);
        }
        if let Some(item) = c["scope"].get("item") {
            ctx = ctx.with_item(item);
        }
        if c.get("position").and_then(Value::as_str) == Some("action") {
            ctx = ctx.with_position(Position::Action);
        }
        let (value, failure) = resolve_with_failure(&c["value"], &ctx);
        let mut got = Map::new();
        got.insert("value".into(), value.unwrap_or(Value::Null));
        if let Some(failure) = failure {
            got.insert("failure".into(), json!({"code": failure.code, "message": failure.message}));
        }
        check(&mut failed, c["name"].as_str().unwrap(), &Value::Object(got), &c["expected"]);
    }
    done(failed);
}

#[test]
fn routing_and_keys() {
    let f = fixture("a2ui10-functions.json");
    let mut failed = Vec::new();
    for c in list(&f, "routing") {
        let input: RouteDecisionInput = serde_json::from_value(c["input"].clone()).unwrap();
        check(&mut failed, c["name"].as_str().unwrap(), &serde_json::to_value(decide_route(&input)).unwrap(), &c["expected"]);
    }
    for c in list(&f, "keys") {
        let got = routed_call_key(c["catalogId"].as_str().unwrap(), c["fn"].as_str().unwrap(), &c["args"]);
        check(&mut failed, c["name"].as_str().unwrap(), &json!(got), &c["expected"]);
    }
    done(failed);
}
