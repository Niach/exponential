//! VAPP-91: the gpui host runtime. Router ops through the host, sources and
//! their cancellation, the function gate (consent, package narrowing), the
//! URL policy, the in-memory transport round trip, painter presses becoming
//! A2UI client messages / host function calls, and catalog negotiation.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use exponential_ui::host::{FunctionDecision, FunctionPolicy, MediaOptions, MediaRule, UrlPolicy};
use exponential_ui_gpui::host::FunctionCallEvent;
use exponential_ui_gpui::runtime::{sync_function, Emit, ExponentialHost, FunctionOutcome, HostIssue, HostOptions, HostPolicy, SourceResolver, MAX_ISSUES, PACKAGE_INVALID};
use exponential_ui_gpui::transport::{MemoryTransport, TransportStatus};
use exponential_ui_gpui::view::SurfaceView;
use gpui::{div, prelude::*, App, Entity, Task, TestAppContext, Window};
use serde_json::{json, Value};

const CORE: &str = "https://ui.exponential.at/catalogs/core/v1";

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn init(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
}

fn create(sid: &str) -> Value {
    json!({"version": "v0.9", "createSurface": {"surfaceId": sid, "catalogId": CORE}})
}

fn components(sid: &str, list: Value) -> Value {
    json!({"version": "v0.9", "updateComponents": {"surfaceId": sid, "components": list}})
}

fn host_with(cx: &mut TestAppContext, options: HostOptions) -> Entity<ExponentialHost> {
    cx.new(|cx| ExponentialHost::new(options, cx))
}

fn data(host: &Entity<ExponentialHost>, sid: &str, cx: &mut TestAppContext) -> Value {
    let view = host.read_with(cx, |h, _| h.surface(sid)).expect("surface");
    view.read_with(cx, |v, _| v.surface().data().clone())
}

fn sent_errors(t: &MemoryTransport) -> Vec<(String, String)> {
    t.sent().iter().filter_map(|m| m.get("error")).map(|e| (e["code"].as_str().unwrap().to_string(), e["message"].as_str().unwrap().to_string())).collect()
}

#[gpui::test]
fn router_ops_reach_surfaces_through_the_memory_transport(cx: &mut TestAppContext) {
    init(cx);
    let t = MemoryTransport::new();
    // Queued before the host connects.
    t.feed([create("s1"), components("s1", json!([{"id": "root", "component": "Stack", "children": ["a", "b"]}, {"id": "a", "component": "Text", "text": "A"}, {"id": "b", "component": "Text", "text": {"path": "/b"}}]))]);
    let host = host_with(cx, HostOptions { transport: Some(Box::new(t.clone())), ..Default::default() });
    assert!(host.read_with(cx, |h, _| h.has_transport()));
    host.update(cx, |h, cx| h.connect(cx));
    cx.run_until_parked();
    assert_eq!(host.read_with(cx, |h, _| h.status()), TransportStatus::Open);
    assert_eq!(host.read_with(cx, |h, _| h.surface_ids()), vec!["s1".to_string()]);

    t.feed([json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/b", "value": "B"}})]);
    cx.run_until_parked();
    assert_eq!(data(&host, "s1", cx), json!({"b": "B"}));

    // `components` merges by id: only `a` changes, `root` and `b` stay.
    t.feed([components("s1", json!([{"id": "a", "component": "Text", "text": "A2"}]))]);
    cx.run_until_parked();
    let view = host.read_with(cx, |h, _| h.surface("s1")).unwrap();
    let ids: Vec<String> = view.update(cx, |v, _| v.surface_mut().nodes().into_iter().map(|n| n.id).collect());
    assert!(ids.contains(&"root".to_string()) && ids.contains(&"a".to_string()) && ids.contains(&"b".to_string()), "{ids:?}");

    // No value = remove the path; path "/" = the whole model.
    t.feed([json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/b"}})]);
    cx.run_until_parked();
    assert_eq!(data(&host, "s1", cx), json!({}));
    t.feed([json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/", "value": {"x": 1}}})]);
    cx.run_until_parked();
    assert_eq!(data(&host, "s1", cx), json!({"x": 1}));

    // Errors go back on the transport.
    t.feed([components("nope", json!([]))]);
    cx.run_until_parked();
    assert_eq!(sent_errors(&t), vec![("SURFACE_NOT_FOUND".to_string(), "no surface nope; send createSurface first".to_string())]);

    t.feed([json!({"version": "v0.9", "deleteSurface": {"surfaceId": "s1"}})]);
    cx.run_until_parked();
    assert!(host.read_with(cx, |h, _| h.surface("s1")).is_none());

    // After close nothing is delivered any more.
    host.update(cx, |h, cx| h.close(cx));
    t.feed([create("s2")]);
    cx.run_until_parked();
    assert!(host.read_with(cx, |h, _| h.surface_ids()).is_empty());
}

#[gpui::test]
fn unsupported_catalogs_are_recorded_and_reported(cx: &mut TestAppContext) {
    init(cx);
    let t = MemoryTransport::new();
    let host = host_with(cx, HostOptions { transport: Some(Box::new(t.clone())), ..Default::default() });
    host.update(cx, |h, cx| h.connect(cx));
    t.feed([json!({"version": "v0.9", "createSurface": {"surfaceId": "x", "catalogId": "https://example.com/catalogs/other/v1"}})]);
    cx.run_until_parked();
    assert_eq!(host.read_with(cx, |h, _| h.unsupported_catalog().map(str::to_string)), Some("https://example.com/catalogs/other/v1".to_string()));
    assert_eq!(sent_errors(&t)[0].0, "UNSUPPORTED_CATALOG");
    // A registered extension catalog negotiates.
    let ext: exponential_ui::ExtensionDef = serde_json::from_value(fixture("catalog-extension.json")["extension"].clone()).unwrap();
    let ext_id = ext.id.clone();
    host.update(cx, |h, _| h.register_extension(ext));
    assert!(host.read_with(cx, |h, _| h.supported_catalog_ids()).contains(&ext_id));
    let caps = host.read_with(cx, |h, _| h.client_capabilities());
    assert!(caps.to_string().contains(&ext_id), "{caps}");
}

struct Probe {
    emit: Mutex<Option<Emit>>,
    cancels: AtomicUsize,
    params: Mutex<Vec<String>>,
}

#[gpui::test]
fn sources_bind_emit_and_cancel(cx: &mut TestAppContext) {
    init(cx);
    let probe = Arc::new(Probe { emit: Mutex::new(None), cancels: AtomicUsize::new(0), params: Mutex::new(Vec::new()) });
    let p = probe.clone();
    let resolver: SourceResolver = Box::new(move |source, emit| {
        p.params.lock().unwrap().push(format!("{}:{} {:?}", source.scheme, source.name, source.params));
        emit(json!(["first"]));
        *p.emit.lock().unwrap() = Some(emit);
        let p = p.clone();
        Box::new(move || {
            p.cancels.fetch_add(1, Ordering::SeqCst);
        })
    });
    let t = MemoryTransport::new();
    let host = host_with(cx, HostOptions { transport: Some(Box::new(t.clone())), sources: HashMap::from([("EXP".to_string(), resolver)]), ..Default::default() });
    host.update(cx, |h, cx| h.connect(cx));
    t.feed([create("s"), json!({"version": "v0.9", "bindDataModel": {"surfaceId": "s", "path": "/devices", "source": "exp:devices?board=b1"}})]);
    cx.run_until_parked();
    assert_eq!(*probe.params.lock().unwrap(), vec!["exp:devices {\"board\": \"b1\"}".to_string()]);
    assert_eq!(data(&host, "s", cx), json!({"devices": ["first"]}));

    // Emit is Send + Sync: a resolver may call it from any thread (the
    // host's channel lands it on the main thread; the deterministic test
    // scheduler forbids a foreign-thread wake, so it is called here).
    fn send_sync<T: Send + Sync>(_: &T) {}
    let emit = probe.emit.lock().unwrap().clone().unwrap();
    send_sync(&emit);
    emit(json!(["second"]));
    cx.run_until_parked();
    assert_eq!(data(&host, "s", cx), json!({"devices": ["second"]}));

    // An unknown scheme is a VALIDATION_FAILED error naming the path.
    t.feed([json!({"version": "v0.9", "bindDataModel": {"surfaceId": "s", "path": "/x", "source": "nope:thing"}})]);
    cx.run_until_parked();
    let err = t.sent().into_iter().find_map(|m| m.get("error").cloned()).unwrap();
    assert_eq!(err, json!({"code": "VALIDATION_FAILED", "surfaceId": "s", "message": "no resolver for the source scheme nope", "path": "/x"}));

    // Deleting the surface cancels; a late emit is dropped.
    t.feed([json!({"version": "v0.9", "deleteSurface": {"surfaceId": "s"}})]);
    cx.run_until_parked();
    assert_eq!(probe.cancels.load(Ordering::SeqCst), 1);
    t.feed([create("s")]);
    cx.run_until_parked();
    (probe.emit.lock().unwrap().clone().unwrap())(json!(["late"]));
    cx.run_until_parked();
    assert_eq!(data(&host, "s", cx), json!({}));
}

fn outcome(task: Task<FunctionOutcome>, cx: &mut TestAppContext) -> FunctionOutcome {
    let slot = Rc::new(RefCell::new(None));
    let s = slot.clone();
    cx.update(|cx| cx.spawn(async move |_| *s.borrow_mut() = Some(task.await)).detach());
    cx.run_until_parked();
    let out = slot.borrow_mut().take();
    out.expect("the call finished")
}

fn call(sid: &str, name: &str) -> FunctionCallEvent {
    FunctionCallEvent { surface_id: sid.into(), component_id: "btn".into(), name: name.into(), args: json!({"n": 2}) }
}

#[gpui::test]
fn the_function_gate_consent_and_package_narrowing(cx: &mut TestAppContext) {
    init(cx);
    let t = MemoryTransport::new();
    let asked = Rc::new(RefCell::new(Vec::<String>::new()));
    let a = asked.clone();
    let consent = Rc::new(move |c: &FunctionCallEvent, _: &mut App| {
        a.borrow_mut().push(c.name.clone());
        Task::ready(c.name == "harness.confirm")
    });
    let double = sync_function(|args, _call, _cx| Ok(json!(args["n"].as_i64().unwrap() * 2)));
    let fail = sync_function(|_, _, _| Err("boom".to_string()));
    let mut functions = HashMap::new();
    for name in ["harness.toast", "harness.confirm", "harness.refuse", "admin.wipe"] {
        functions.insert(name.to_string(), double.clone());
    }
    functions.insert("harness.fail".to_string(), fail);
    let policy = HostPolicy {
        functions: Some(FunctionPolicy { allow: Some(vec!["harness.toast".into(), "harness.fail".into()]), ask: Some(vec!["harness.*".into()]), deny: Some(vec!["admin.*".into()]), default: None }),
        on_function_call: Some(consent),
        ..Default::default()
    };
    let host = host_with(cx, HostOptions { transport: Some(Box::new(t.clone())), functions, policy, packages: vec![fixture("host-router.json")["packages"]["acme.devices"].clone()], ..Default::default() });
    host.update(cx, |h, cx| h.connect(cx));
    t.feed([create("plain")]);
    cx.run_until_parked();

    let run = |name: &str, sid: &str, cx: &mut TestAppContext| {
        let task = host.update(cx, |h, cx| h.call_function(call(sid, name), cx));
        outcome(task, cx)
    };
    assert_eq!(run("harness.toast", "plain", cx), FunctionOutcome { decision: FunctionDecision::Allow, result: Some(json!(4)), error: None });
    assert_eq!(run("harness.fail", "plain", cx).error.as_deref(), Some("boom"));
    assert_eq!(run("harness.confirm", "plain", cx).result, Some(json!(4)));
    assert_eq!(run("harness.refuse", "plain", cx).decision, FunctionDecision::Deny);
    assert_eq!(*asked.borrow(), vec!["harness.confirm".to_string(), "harness.refuse".to_string()]);
    assert_eq!(run("admin.wipe", "plain", cx).decision, FunctionDecision::Deny);
    assert_eq!(run("missing.fn", "plain", cx).decision, FunctionDecision::NotFound);
    // A built-in other than openUrl is allowed and does nothing.
    assert_eq!(run("formatDate", "plain", cx), FunctionOutcome { decision: FunctionDecision::Allow, result: None, error: None });
    assert_eq!(
        sent_errors(&t),
        vec![
            ("FUNCTION_DENIED".to_string(), "harness.refuse was not allowed".to_string()),
            ("FUNCTION_DENIED".to_string(), "admin.wipe was not allowed".to_string()),
            ("FUNCTION_NOT_FOUND".to_string(), "no function missing.fn".to_string()),
        ]
    );

    // A template's surface carries its package: `functions` narrows the gate.
    t.feed([json!({"version": "v0.9", "applyTemplate": {"surfaceId": "pkg", "templateId": "list"}})]);
    cx.run_until_parked();
    assert_eq!(host.read_with(cx, |h, _| h.package_id_of("pkg").map(str::to_string)), Some("acme.devices".to_string()));
    assert_eq!(data(&host, "pkg", cx)["title"], json!("Devices"));
    assert_eq!(host.read_with(cx, |h, _| h.decide("pkg", "harness.toast")), FunctionDecision::Allow);
    assert_eq!(host.read_with(cx, |h, _| h.decide("pkg", "harness.confirm")), FunctionDecision::Deny);
    assert_eq!(host.read_with(cx, |h, _| h.decide("plain", "harness.confirm")), FunctionDecision::Ask);
    // The template's `exp:` binding had no resolver.
    assert!(sent_errors(&t).iter().any(|(c, m)| c == "VALIDATION_FAILED" && m.contains("exp")));
}

#[gpui::test]
fn urls_pass_the_url_policy_and_media_gets_its_headers(cx: &mut TestAppContext) {
    init(cx);
    let opened = Rc::new(RefCell::new(Vec::<String>::new()));
    let o = opened.clone();
    let policy = HostPolicy {
        urls: Some(UrlPolicy { schemes: None, hosts: Some(vec!["*.example.com".into()]), base_url: None }),
        open_url: Some(Rc::new(move |url: &str, _: &mut App| o.borrow_mut().push(url.to_string()))),
        media: Some(MediaOptions { base_url: Some("https://app.example.com/".into()), rules: Some(vec![MediaRule { prefix: "https://app.example.com/api/attachments/".into(), headers: [("authorization".to_string(), "Bearer t".to_string())].into() }]), ..Default::default() }),
        ..Default::default()
    };
    let host = host_with(cx, HostOptions { policy, ..Default::default() });
    let ok = host.update(cx, |h, cx| [h.open_url("/docs", cx), h.open_url("https://evil.test/", cx), h.open_url("javascript:alert(1)", cx), h.open_url("mailto:a@b.c", cx)]);
    assert_eq!(ok, [true, false, false, true]);
    assert_eq!(*opened.borrow(), vec!["https://app.example.com/docs".to_string(), "mailto:a@b.c".to_string()]);

    // The openUrl built-in as a function call goes through the same policy.
    let task = host.update(cx, |h, cx| h.call_function(FunctionCallEvent { surface_id: "s".into(), component_id: "b".into(), name: "openUrl".into(), args: json!({"url": "https://www.example.com/x"}) }, cx));
    assert_eq!(outcome(task, cx).decision, FunctionDecision::Allow);
    assert_eq!(opened.borrow().last().unwrap(), "https://www.example.com/x");

    let req = host.read_with(cx, |h, _| h.media_request("/api/attachments/42")).unwrap();
    assert_eq!(req.url, "https://app.example.com/api/attachments/42");
    assert_eq!(req.headers.get("authorization").map(String::as_str), Some("Bearer t"));
    // The painter's plugin answers the same request.
    let plugin = host.read_with(cx, |h, _| h.plugin(Rc::new(exponential_ui_gpui::host::NoHost)));
    assert_eq!(plugin.media_request("/api/attachments/42"), Some(req));
}

struct Holder(Option<Entity<SurfaceView>>);

impl Render for Holder {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().children(self.0.clone())
    }
}

#[gpui::test]
fn painter_presses_become_client_messages_and_function_calls(cx: &mut TestAppContext) {
    init(cx);
    let t = MemoryTransport::new();
    let calls = Rc::new(RefCell::new(Vec::<Value>::new()));
    let c = calls.clone();
    let toast = sync_function(move |args, call, _| {
        c.borrow_mut().push(json!({"args": args, "component": call.component_id}));
        Ok(Value::Null)
    });
    let host = host_with(cx, HostOptions { transport: Some(Box::new(t.clone())), functions: HashMap::from([("harness.toast".to_string(), toast)]), ..Default::default() });
    host.update(cx, |h, cx| h.connect(cx));
    t.feed([
        create("s"),
        components(
            "s",
            json!([
                {"id": "root", "component": "Stack", "direction": "horizontal", "children": ["save", "toast"]},
                {"id": "save", "component": "Button", "label": "Save", "on": {"press": {"event": {"name": "save", "context": {"title": {"path": "/title"}}}}}},
                {"id": "toast", "component": "Button", "label": "Toast", "on": {"press": {"functionCall": {"call": "harness.toast", "args": {"text": {"path": "/title"}}}}}}
            ]),
        ),
        json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s", "value": {"title": "Hello"}}}),
    ]);
    cx.run_until_parked();
    let view = host.read_with(cx, |h, _| h.surface("s")).unwrap();
    let v2 = view.clone();
    let (_, vcx) = cx.add_window_view(move |_, _| Holder(Some(v2)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();
    for id in ["save", "toast"] {
        vcx.update(|window, cx| {
            view.update(cx, |v, cx| {
                let i = v.index_of(id).unwrap();
                v.press(i, window, cx);
            })
        });
    }
    vcx.run_until_parked();
    let action = t.sent().into_iter().find_map(|m| m.get("action").cloned()).expect("an action message");
    assert_eq!(action["name"], json!("save"));
    assert_eq!(action["surfaceId"], json!("s"));
    assert_eq!(action["sourceComponentId"], json!("save"));
    assert_eq!(action["context"], json!({"title": "Hello"}));
    assert!(action["timestamp"].as_str().unwrap().ends_with('Z'));
    assert_eq!(*calls.borrow(), vec![json!({"args": {"text": "Hello"}, "component": "toast"})]);
}

#[gpui::test]
fn create_surface_theme_applies_a_builtin_or_a_theme_json_and_an_unusable_one_is_an_issue(cx: &mut TestAppContext) {
    init(cx);
    let t = MemoryTransport::new();
    let host = host_with(cx, HostOptions { transport: Some(Box::new(t.clone())), ..Default::default() });
    host.update(cx, |h, cx| h.connect(cx));
    let path = format!("{}/../../../../packages/exponential-ui/themes/neutral.theme.json", env!("CARGO_MANIFEST_DIR"));
    let mut acme: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    acme["id"] = json!("acme");
    let with_theme = |sid: &str, theme: Value| json!({"version": "v0.9", "createSurface": {"surfaceId": sid, "catalogId": CORE, "theme": theme}});
    t.feed([with_theme("a", json!("neutral")), with_theme("b", acme), create("c"), with_theme("d", json!("nope")), with_theme("e", json!({"id": "broken"}))]);
    cx.run_until_parked();
    let theme_id = |sid: &str, cx: &mut TestAppContext| host.read_with(cx, |h, _| h.surface_theme(sid).map(|t| t.id.clone()));
    assert_eq!(theme_id("a", cx).as_deref(), Some("neutral"));
    assert_eq!(theme_id("b", cx).as_deref(), Some("acme"));
    for sid in ["c", "d", "e"] {
        assert_eq!(theme_id(sid, cx), None, "{sid}");
        assert!(host.read_with(cx, |h, _| h.surface(sid).is_some()), "the surface {sid} is still created");
    }
    let sent: Vec<(String, String, String)> = t.sent().iter().filter_map(|m| m.get("error")).map(|e| (e["code"].as_str().unwrap().into(), e["surfaceId"].as_str().unwrap().into(), e["path"].as_str().unwrap_or("").into())).collect();
    assert_eq!(sent, vec![("VALIDATION_FAILED".into(), "d".into(), "/createSurface/theme".into()), ("VALIDATION_FAILED".into(), "e".into(), "/createSurface/theme".into())]);
    let issues = host.read_with(cx, |h, _| h.issues().to_vec());
    assert_eq!(issues.len(), 2);
    assert!(issues[0].message.contains("unknown built-in theme \"nope\""), "{:?}", issues[0]);
    assert_eq!(issues[1].path.as_deref(), Some("/createSurface/theme"));
}

#[gpui::test]
fn host_issues_record_sent_errors_and_package_problems_without_a_transport(cx: &mut TestAppContext) {
    init(cx);
    let seen = Rc::new(RefCell::new(Vec::<HostIssue>::new()));
    let s = seen.clone();
    let host = host_with(cx, HostOptions { on_issue: Some(Rc::new(move |i: &HostIssue| s.borrow_mut().push(i.clone()))), ..Default::default() });
    assert!(!host.read_with(cx, |h, _| h.has_transport()));
    host.update(cx, |h, cx| {
        h.receive(&json!({"version": "v0.9", "applyTemplate": {"surfaceId": "x", "templateId": "missing"}}), cx);
    });
    let issues = host.read_with(cx, |h, _| h.issues().to_vec());
    assert_eq!(issues.len(), 1);
    assert_eq!((issues[0].code.as_str(), issues[0].surface_id.as_deref()), ("TEMPLATE_NOT_FOUND", Some("x")));
    assert_eq!(*seen.borrow(), issues, "on_issue sees every issue");

    // FUNCTION_DENIED lands there too.
    let task = host.update(cx, |h, cx| h.call_function(call("x", "admin.wipe"), cx));
    let _ = outcome(task, cx);
    assert!(host.read_with(cx, |h, _| h.issues().iter().any(|i| i.code == "FUNCTION_NOT_FOUND" || i.code == "FUNCTION_DENIED")));

    // A package that fails validation: PACKAGE_INVALID, not installed.
    let bad = json!({"id": "acme.bad", "templates": "nope"});
    let returned = host.update(cx, |h, _| h.install_package(&bad));
    assert!(!returned.is_empty());
    let pkg_issues: Vec<HostIssue> = host.read_with(cx, |h, _| h.issues().iter().filter(|i| i.code == PACKAGE_INVALID).cloned().collect());
    assert_eq!(pkg_issues.len(), returned.len());
    assert!(pkg_issues.iter().all(|i| i.package_id.as_deref() == Some("acme.bad")));
    assert!(host.read_with(cx, |h, _| h.router().package("acme.bad").is_none()));

    // Capped: the oldest drop.
    host.update(cx, |h, _| {
        for _ in 0..MAX_ISSUES + 20 {
            h.install_package(&json!({"id": "acme.flood"}));
        }
    });
    let issues = host.read_with(cx, |h, _| h.issues().to_vec());
    assert_eq!(issues.len(), MAX_ISSUES);
    assert!(issues.iter().all(|i| i.package_id.as_deref() == Some("acme.flood")));
}

#[test]
fn a_constructor_package_that_fails_validation_is_a_hard_error() {
    let options = HostOptions { packages: vec![json!({"id": "acme.bad", "templates": "nope"})], ..Default::default() };
    let err = options.validate_packages().expect_err("an invalid package");
    assert_eq!(err.package_id, "acme.bad");
    assert!(err.to_string().starts_with("exponential-ui: package acme.bad is unusable: "), "{err}");
    assert!(HostOptions { packages: vec![fixture("host-router.json")["packages"]["acme.devices"].clone()], ..Default::default() }.validate_packages().is_ok());
}

#[gpui::test]
#[should_panic(expected = "package acme.bad is unusable")]
fn the_host_panics_on_a_constructor_package_that_fails_validation(cx: &mut TestAppContext) {
    init(cx);
    host_with(cx, HostOptions { packages: vec![json!({"id": "acme.bad", "templates": "nope"})], ..Default::default() });
}
