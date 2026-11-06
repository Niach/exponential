//! Replays `packages/eengine/fixtures` (the TS twin `@exp/eengine` replays the same cases).

use eengine::broker::{agent_tool_list, broker_audit_row, broker_decide, capability_name_valid, parse_capability_call, scope_id_valid, BrokerCall, CallFrom, CallOutcome, Grants, ManifestEntry, Verdict, CATALOG, RESERVED_NAMES};
use eengine::consent::{consent_answer, consent_sheet, grant_write, grants_on_package, grants_record, manage_sheet, ConsentAnswer, GrantVia};
use eengine::contract::{APP_RECORDS, CAPABILITIES, CONSENT, LIFECYCLE, OUTBOX, PROVIDERS};
use eengine::lifecycle::{lifecycle_step, LifecycleContext, LifecycleEvent, LifecycleState};
use eengine::outbox::{outbox_rows, OutboxEntry, OutboxMode, OutboxRow};
use eengine::providers::{engine_record_refusal, engine_viewer, network_allows, validate_host_config, EngineIdentity, NetworkPolicy, NetworkTarget};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

fn de<T: DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{v}: {e}"))
}

fn cases<'a>(fixture: &'a Value, key: &str) -> &'a Vec<Value> {
    fixture[key].as_array().unwrap_or_else(|| panic!("{key} is a list"))
}

#[test]
fn the_catalog_is_well_formed() {
    for c in CATALOG.iter() {
        assert!(capability_name_valid(&c.name), "{}", c.name);
        for s in &c.scopes {
            assert!(scope_id_valid(&s.id) && s.id.starts_with(&format!("{}.", c.name)), "{}", s.id);
        }
    }
    for name in RESERVED_NAMES.iter() {
        assert!(!capability_name_valid(name), "{name}");
    }
    let mut declarable: Vec<&str> = CATALOG.iter().flat_map(|c| c.scopes.iter().filter(|s| !s.host_only).map(|s| s.id.as_str())).collect();
    declarable.sort();
    assert_eq!(declarable, eui_core::host::CAPABILITY_SCOPES, "the declarable scopes are genUI's format 5 list");
}

#[test]
fn parse_cases() {
    for c in cases(&CAPABILITIES, "parseCases") {
        let from: CallFrom = de(&c["from"]);
        let got = parse_capability_call(from, c["name"].as_str().unwrap()).map(|(capability, tool)| json!({"capability": capability, "tool": tool}));
        assert_eq!(got.unwrap_or(Value::Null), c["expected"], "{c}");
    }
}

#[test]
fn decide_cases() {
    for c in cases(&CAPABILITIES, "decideCases") {
        let manifest: Vec<ManifestEntry> = de(&c["manifest"]);
        let grants: Grants = de(&c["grants"]);
        let provided: Vec<String> = de(&c["provided"]);
        let call: BrokerCall = de(&c["call"]);
        let got = broker_decide(&manifest, &grants, &provided, c["foreground"].as_bool().unwrap(), &call);
        assert_eq!(got, de::<Verdict>(&c["expected"]), "{}", c["name"]);
    }
}

#[test]
fn list_cases() {
    for c in cases(&CAPABILITIES, "listCases") {
        let got = agent_tool_list(&de::<Vec<ManifestEntry>>(&c["manifest"]), &de::<Vec<String>>(&c["provided"]));
        assert_eq!(got, de::<Vec<String>>(&c["expected"]), "{}", c["name"]);
    }
}

#[test]
fn audit_cases() {
    for c in cases(&CAPABILITIES, "auditCases") {
        let call: BrokerCall = de(&c["call"]);
        let verdict: Verdict = de(&c["verdict"]);
        let outcome: Option<CallOutcome> = de(&c["outcome"]);
        let row = broker_audit_row(c["appId"].as_str().unwrap(), c["client"].as_str(), &call, &verdict, outcome);
        assert_eq!(serde_json::to_value(row).unwrap(), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn consent_cases() {
    for c in cases(&CONSENT, "answerCases") {
        let (verdict, reason, grants) = consent_answer(&de(&c["grants"]), c["scope"].as_str().unwrap(), de::<ConsentAnswer>(&c["answer"]));
        assert_eq!(json!({"verdict": verdict, "reason": reason, "grants": grants}), c["expected"], "{c}");
    }
    for c in cases(&CONSENT, "writeCases") {
        let w = &c["write"];
        let value = de(&w["value"]);
        let (grants, refused) = grant_write(&de::<Vec<ManifestEntry>>(&c["manifest"]), &de(&c["grants"]), de::<GrantVia>(&w["via"]), w["gesture"].as_bool().unwrap(), w["scope"].as_str().unwrap(), value);
        assert_eq!(json!({"grants": grants, "refused": refused}), c["expected"], "{}", c["name"]);
    }
    for c in cases(&CONSENT, "storeCases") {
        let manifest: Vec<ManifestEntry> = de(&c["manifest"]);
        let grants: Grants = de(&c["grants"]);
        let (key, value) = grants_record(c["app"].as_str().unwrap(), c["signer"].as_str().unwrap(), &manifest, &grants);
        assert_eq!(json!({"key": key, "value": value}), c["expected"], "{}", c["name"]);
        assert_eq!(grants_on_package(Some(&value), c["signer"].as_str().unwrap(), &manifest), grants, "{}", c["name"]);
    }
    for c in cases(&CONSENT, "reconcileCases") {
        let got = grants_on_package(c["stored"].as_str(), c["signer"].as_str().unwrap(), &de::<Vec<ManifestEntry>>(&c["manifest"]));
        assert_eq!(got, de::<Grants>(&c["expected"]), "{}", c["name"]);
    }
    for c in cases(&CONSENT, "sheetCases") {
        let (components, data) = consent_sheet(c["app"].as_str().unwrap(), &de(&c["entry"]));
        assert_eq!(Value::Array(components), c["expected"], "{}", c["name"]);
        assert_eq!(data, c["data"], "{}", c["name"]);
    }
    for c in cases(&CONSENT, "manageCases") {
        assert_eq!(Value::Array(manage_sheet(&de::<Vec<ManifestEntry>>(&c["manifest"]), &de(&c["grants"]))), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn lifecycle_cases() {
    let pkg = &LIFECYCLE["package"];
    for c in cases(&LIFECYCLE, "cases") {
        let last = c["lastScreen"].as_object().map(|o| (o["screen"].as_str().unwrap(), &o["params"]));
        let cx = LifecycleContext { pkg, last_screen: last };
        let mut state = LifecycleState::default();
        for step in c["steps"].as_array().unwrap() {
            let event: LifecycleEvent = de(&step["event"]);
            let (next, effects) = lifecycle_step(&state, &event, &cx);
            assert_eq!(serde_json::to_value(&effects).unwrap(), step["effects"], "{}: {}", c["name"], step["event"]);
            assert_eq!(serde_json::to_value(&next).unwrap(), step["state"], "{}: {}", c["name"], step["event"]);
            state = next;
        }
    }
}

#[test]
fn provider_cases() {
    for c in cases(&PROVIDERS, "validateCases") {
        let got = validate_host_config(c["config"].as_object().unwrap());
        assert_eq!(serde_json::to_value(got).unwrap(), c["expected"], "{}", c["name"]);
    }
    for (host, config) in PROVIDERS["hosts"].as_object().unwrap() {
        assert!(validate_host_config(config.as_object().unwrap()).is_empty(), "host {host}");
    }
    for c in cases(&PROVIDERS, "identityCases") {
        let identity: EngineIdentity = de(&c["identity"]);
        assert_eq!(serde_json::to_value(engine_viewer(&identity)).unwrap(), c["expected"]["viewer"], "{}", c["name"]);
        let policy = eui_core::host::record_policy(Some(&json!({"policy": c["policy"]})));
        let op = eui_core::host::RecordOp::parse(c["op"].as_str().unwrap()).unwrap();
        assert_eq!(engine_record_refusal(&identity, &policy, op, Some(&c["row"])), c["expected"]["refusal"].as_str(), "{}", c["name"]);
    }
    for c in cases(&PROVIDERS, "networkCases") {
        let policy: NetworkPolicy = de(&c["policy"]);
        let target: NetworkTarget = de(&c["target"]);
        let got = network_allows(policy, target, c["declared"].as_bool().unwrap());
        assert_eq!(json!({"allowed": got.is_ok(), "reason": got.err()}), c["expected"], "{c}");
    }
}

#[test]
fn outbox_is_o1_by_reference() {
    assert_eq!(OUTBOX["$ref"], "../../domain-contract/fixtures/app-records.json#/outbox");
    for c in cases(&OUTBOX, "modeCases") {
        assert_eq!(OutboxMode::of(c["sync"].as_bool().unwrap()).as_str(), c["expected"], "{}", c["name"]);
    }
    for c in cases(&APP_RECORDS["outbox"], "rowsCases") {
        let rows: Vec<OutboxRow> = de(&c["rows"]);
        let entries: Vec<OutboxEntry> = de(&c["entries"]);
        let got = outbox_rows(&rows, &entries, c["me"].as_str());
        assert_eq!(serde_json::to_value(got).unwrap(), c["expected"], "{}", c["name"]);
    }
}

// ---------------------------------------------------------------------------
// Capabilities v1 (VAPP-265): the tools, the bridge, the PDF.

use std::collections::HashMap;
use std::sync::Mutex;

use eengine::capability::Capability;
use eengine::providers::{BlobStore, Failure};
use eengine::tools::{base64_decode, base64_encode, bridge_call, notification_id, tool_def, validate_arguments, BridgeContext, TOOLS};
use sha2::{Digest, Sha256};

/// A BlobStore issuing `blob-1`, `blob-2`, … and recording every put.
#[derive(Default)]
struct Blobs {
    files: Mutex<HashMap<String, Vec<u8>>>,
    puts: Mutex<Vec<(String, String, Vec<u8>)>>,
}

impl Blobs {
    fn seeded(case: &Value) -> Self {
        let blobs = Blobs::default();
        for seed in [&CAPABILITIES["v1"]["blobs"], &case["blobs"]] {
            for (id, data) in seed.as_object().into_iter().flatten() {
                blobs.files.lock().unwrap().insert(id.clone(), base64_decode(data.as_str().unwrap()).unwrap());
            }
        }
        blobs
    }
}

impl BlobStore for Blobs {
    fn put(&self, app: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<String, Failure> {
        assert_eq!(app, CAPABILITIES["v1"]["context"]["app"]);
        let mut puts = self.puts.lock().unwrap();
        puts.push((name.into(), mime.into(), bytes.to_vec()));
        let id = format!("blob-{}", puts.len());
        self.files.lock().unwrap().insert(id.clone(), bytes.to_vec());
        Ok(id)
    }

    fn get(&self, _app: &str, id: &str) -> Result<Option<Vec<u8>>, Failure> {
        Ok(self.files.lock().unwrap().get(id).cloned())
    }

    fn delete(&self, _app: &str, id: &str) -> Result<(), Failure> {
        self.files.lock().unwrap().remove(id);
        Ok(())
    }
}

/// A native that must get exactly `native` and answers `nativeResult`
/// (`{isError: text}` = an MCP error result); `native: null` = never called.
struct Native {
    name: String,
    tool: String,
    case: Value,
    called: Mutex<bool>,
}

impl Capability for Native {
    fn name(&self) -> &str {
        &self.name
    }

    fn tools(&self) -> Vec<Value> {
        vec![]
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        let c = &self.case;
        assert!(!c["native"].is_null(), "{}: the native ran", c["name"]);
        assert_eq!(tool, self.tool, "{}", c["name"]);
        assert_eq!(arguments, &c["native"], "{}: the native's arguments", c["name"]);
        *self.called.lock().unwrap() = true;
        let result = &c["nativeResult"];
        Ok(match result.get("isError").and_then(Value::as_str) {
            Some(text) => json!({"content": [{"type": "text", "text": text}], "isError": true}),
            None => result.clone(),
        })
    }
}

fn context_screens() -> Vec<String> {
    de(&CAPABILITIES["v1"]["context"]["screens"])
}

fn bridge(case: &Value, blobs: &Blobs, tool: &str, arguments: &Value) -> (Value, CallOutcome, bool) {
    let (capability, tool) = tool.split_once('.').unwrap();
    let native = Native { name: capability.into(), tool: tool.into(), case: case.clone(), called: Mutex::new(false) };
    let screens = context_screens();
    let cx = BridgeContext { app: CAPABILITIES["v1"]["context"]["app"].as_str().unwrap(), screens: &screens, blobs };
    let (result, outcome) = bridge_call(&cx, &native, tool, arguments);
    let called = *native.called.lock().unwrap();
    (result, outcome, called)
}

#[test]
fn the_v1_tools_are_catalog_tools() {
    let names: Vec<&str> = TOOLS.iter().map(|t| t.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "tools are sorted by name");
    for t in TOOLS.iter() {
        let (capability, tool) = t.name.split_once('.').unwrap();
        let def = eengine::broker::capability_def(capability).unwrap_or_else(|| panic!("{}", t.name));
        assert!(def.scopes.iter().any(|s| s.tools.iter().any(|x| x == tool)), "{} is a catalog tool", t.name);
        assert_eq!(t.native_output_schema.is_some(), t.runs == eengine::tools::Runs::Native, "{}", t.name);
    }
}

#[test]
fn validate_cases() {
    for c in cases(&CAPABILITIES, "validateCases") {
        let (capability, tool) = c["tool"].as_str().unwrap().split_once('.').unwrap();
        let got = match validate_arguments(tool_def(capability, tool).unwrap(), &c["arguments"]) {
            Ok(arguments) => json!({"arguments": arguments}),
            Err(error) => json!({"error": error}),
        };
        assert_eq!(got, c["expected"], "{c}");
    }
}

#[test]
fn bridge_cases() {
    for c in cases(&CAPABILITIES, "bridgeCases") {
        let name = &c["name"];
        let blobs = Blobs::seeded(c);
        let (result, outcome, called) = bridge(c, &blobs, c["tool"].as_str().unwrap(), &c["arguments"]);
        assert_eq!(called, !c["native"].is_null(), "{name}: the native ran");
        let puts: Vec<Value> = blobs.puts.lock().unwrap().iter().map(|(n, m, b)| json!({"name": n, "mimeType": m, "base64": base64_encode(b)})).collect();
        assert_eq!(Value::Array(puts), c["puts"], "{name}: puts");
        let expected = &c["expected"];
        match expected.get("isError").and_then(Value::as_str) {
            Some(text) => assert_eq!(result, json!({"content": [{"type": "text", "text": text}], "isError": true}), "{name}"),
            None => {
                let structured = &expected["structuredContent"];
                assert_eq!(result, json!({"content": [{"type": "text", "text": structured.to_string()}], "structuredContent": structured}), "{name}");
            }
        }
        let want: CallOutcome = expected.get("outcome").map_or(CallOutcome::Ok, de);
        assert_eq!(outcome, want, "{name}: outcome");
    }
}

#[test]
fn a_generated_notification_id_never_collides() {
    let id = notification_id();
    assert_eq!(id.len(), 34, "{id}");
    assert!(id.starts_with("n-") && id[2..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{id}");
    assert_ne!(id, notification_id());
    // An absent id is generated, and the app's own id is what it gets back.
    struct Any(Mutex<Option<Value>>);
    impl Capability for Any {
        fn name(&self) -> &str {
            "notifications"
        }
        fn tools(&self) -> Vec<Value> {
            vec![]
        }
        fn call(&self, _tool: &str, arguments: &Value) -> Result<Value, Failure> {
            *self.0.lock().unwrap() = Some(arguments.clone());
            Ok(json!({"structuredContent": {}}))
        }
    }
    let native = Any(Mutex::new(None));
    let blobs = Blobs::default();
    let screens = context_screens();
    let cx = BridgeContext { app: "acme.album", screens: &screens, blobs: &blobs };
    let (result, outcome) = bridge_call(&cx, &native, "post", &json!({"title": "Hi"}));
    assert_eq!(outcome, CallOutcome::Ok);
    let own = result["structuredContent"]["id"].as_str().unwrap().to_string();
    assert!(own.starts_with("n-") && own.len() == 34, "{own}");
    assert_eq!(native.0.lock().unwrap().as_ref().unwrap()["id"], json!(format!("acme.album/{own}")));
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn pdf_cases() {
    let out = std::env::var("EENGINE_PDF_OUT").ok();
    for (i, c) in cases(&CAPABILITIES, "pdfCases").iter().enumerate() {
        let name = &c["name"];
        let blobs = Blobs::seeded(c);
        let case = json!({"name": name, "native": null});
        let (result, outcome, called) = bridge(&case, &blobs, "print.pdf", &c["arguments"]);
        assert!(!called, "{name}: print.pdf makes no native call");
        let expected = &c["expected"];
        if let Some(text) = expected.get("isError").and_then(Value::as_str) {
            assert_eq!(result, json!({"content": [{"type": "text", "text": text}], "isError": true}), "{name}");
            assert_eq!(outcome, CallOutcome::IsError, "{name}");
            assert!(blobs.puts.lock().unwrap().is_empty(), "{name}: nothing stored");
            continue;
        }
        assert_eq!(outcome, CallOutcome::Ok, "{name}: {result}");
        let puts = blobs.puts.lock().unwrap();
        assert_eq!(puts.len(), 1, "{name}");
        let (file, mime, bytes) = &puts[0];
        if let Some(dir) = &out {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(format!("{dir}/case-{i}.pdf"), bytes).unwrap();
            println!("case-{i}: {} {} {}", bytes.len(), sha256_hex(bytes), name);
        }
        assert_eq!((file.as_str(), mime.as_str()), (expected["name"].as_str().unwrap(), "application/pdf"), "{name}");
        let structured = json!({"file": {"name": expected["name"], "mimeType": "application/pdf", "size": bytes.len(), "fileId": "blob-1"}, "pages": expected["pages"]});
        assert_eq!(result["structuredContent"], structured, "{name}");
        assert_eq!(json!({"size": bytes.len(), "sha256": sha256_hex(bytes)}), json!({"size": expected["size"], "sha256": expected["sha256"]}), "{name}: the bytes are locked");
    }
}
