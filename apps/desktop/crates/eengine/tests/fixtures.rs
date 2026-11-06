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
