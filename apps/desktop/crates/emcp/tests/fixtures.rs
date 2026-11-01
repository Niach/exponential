//! Replays every case of `packages/emcp/fixtures/*.json` (#EXP-1490), the
//! same cases the TypeScript references and the server broker replay.

use std::collections::HashMap;

use emcp::contract::access::{self, AccessInput, Grant, Level};
use emcp::contract::audit::{self, CallOutcome};
use emcp::contract::exposes::{self, ExposeContext, ExposeError, ToolAccess};
use emcp::contract::fixture;
use emcp::contract::upstream::{self, UpstreamState};
use emcp::contract::vault::{self, Slot};
use serde_json::Value;

fn s(value: &Value) -> &str {
    value.as_str().unwrap_or_else(|| panic!("not a string: {value}"))
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().unwrap().iter().map(|item| s(item).to_string()).collect()
}

fn opt_str(value: &Value) -> Option<&str> {
    value.as_str()
}

// ---------------------------------------------------------------- vault

fn slot(value: &Value) -> Slot<'_> {
    Slot { table: s(&value["table"]), column: s(&value["column"]), row_id: s(&value["rowId"]) }
}

fn vault_keyring() -> vault::Keyring {
    let f = fixture::vault();
    vault::parse_keyring(opt_str(&f["keys"]["k1"]), opt_str(&f["keys"]["k0"])).expect("the fixture keyring")
}

#[test]
fn vault_constants_match() {
    let f = fixture::vault();
    assert_eq!(f["version"], vault::VERSION);
    assert_eq!(vault::fixture_constants(), (32, 12, 16));
}

#[test]
fn vault_keyring_cases() {
    for case in fixture::vault()["keyringCases"].as_array().unwrap() {
        let parsed = vault::parse_keyring(opt_str(&case["current"]), opt_str(&case["retired"]));
        let got = parsed.map(|ring| {
            serde_json::json!({ "current": ring.current.kid, "retired": ring.retired.iter().map(|k| k.kid.clone()).collect::<Vec<_>>() })
        });
        let expected = (!case["expected"].is_null()).then(|| case["expected"].clone());
        assert_eq!(got, expected, "{}", case["name"]);
    }
}

#[test]
fn vault_aad_cases() {
    for case in fixture::vault()["aadCases"].as_array().unwrap() {
        let got = vault::aad(s(&case["kid"]), Slot { table: s(&case["table"]), column: s(&case["column"]), row_id: s(&case["rowId"]) });
        assert_eq!(got, s(&case["expected"]));
    }
}

fn seal_vectors() -> Vec<String> {
    let f = fixture::vault();
    use base64::Engine;
    f["sealVectors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|vector| {
            let key = vault::parse_key(opt_str(&f["keys"][s(&vector["key"])])).unwrap();
            let iv: [u8; 12] =
                base64::engine::general_purpose::STANDARD.decode(s(&vector["iv"])).unwrap().try_into().unwrap();
            vault::seal_with(&key, &iv, s(&vector["plaintext"]), slot(&vector["slot"]))
        })
        .collect()
}

#[test]
fn vault_seal_vectors_are_exact() {
    let sealed = seal_vectors();
    for (vector, got) in fixture::vault()["sealVectors"].as_array().unwrap().iter().zip(&sealed) {
        assert_eq!(got, s(&vector["expected"]));
    }
}

#[test]
fn vault_open_cases() {
    let ring = vault_keyring();
    let vectors = fixture::vault()["sealVectors"].as_array().unwrap();
    for case in fixture::vault()["openCases"].as_array().unwrap() {
        let mut value = s(&vectors[case["vector"].as_u64().unwrap() as usize]["expected"]).to_string();
        if let Some(replace) = case["replace"].as_array() {
            value = value.replacen(s(&replace[0]), s(&replace[1]), 1);
        }
        if case["tamperLast"] == true {
            let last = value.pop().unwrap();
            value.push(if last == 'A' { 'B' } else { 'A' });
        }
        let got = vault::open(&ring, &value, slot(&case["slot"]));
        assert_eq!(got.as_deref(), opt_str(&case["expected"]), "{}", case["name"]);
    }
}

#[test]
fn vault_malformed_and_rotation_cases() {
    let ring = vault_keyring();
    let any = Slot { table: "accounts", column: "access_token", row_id: "acc_1" };
    for value in fixture::vault()["malformedCases"].as_array().unwrap() {
        assert_eq!(vault::open(&ring, s(value), any), None, "{value}");
    }
    let vectors = fixture::vault()["sealVectors"].as_array().unwrap();
    for case in fixture::vault()["rotationCases"].as_array().unwrap() {
        let value = match case["vector"].as_u64() {
            Some(i) => s(&vectors[i as usize]["expected"]).to_string(),
            None => s(&case["value"]).to_string(),
        };
        assert_eq!(vault::needs_rotation(&ring, &value), case["expected"] == true, "{value}");
    }
    // A random seal opens in its slot and nowhere else.
    let sealed = vault::seal(&ring, "fresh", any);
    assert_eq!(vault::open(&ring, &sealed, any).as_deref(), Some("fresh"));
    assert_eq!(vault::open(&ring, &sealed, Slot { row_id: "acc_2", ..any }), None);
}

// ---------------------------------------------------------------- audit

#[test]
fn audit_canonical_and_hash_cases() {
    let f = fixture::audit();
    assert_eq!(f["hashPrefix"], audit::HASH_PREFIX);
    assert_eq!(f["retentionDays"], audit::RETENTION_DAYS);
    for case in f["canonicalCases"].as_array().unwrap() {
        assert_eq!(audit::canonical_json(&case["value"]), s(&case["expected"]));
    }
    for case in f["hashCases"].as_array().unwrap() {
        let args = (!case["args"].is_null()).then_some(&case["args"]);
        assert_eq!(audit::args_hash(args), s(&case["expected"]), "{}", case["args"]);
    }
}

#[test]
fn audit_status_and_retention_cases() {
    let f = fixture::audit();
    let statuses: Vec<String> = strings(&f["statuses"]);
    for case in f["statusCases"].as_array().unwrap() {
        let o = &case["outcome"];
        let outcome = CallOutcome {
            denied: o["denied"] == true,
            invalid: o["invalid"] == true,
            upstream_error: o["upstreamError"] == true,
            error: o["error"] == true,
        };
        let got = audit::call_status(outcome).as_str();
        assert!(statuses.iter().any(|status| status == got));
        assert_eq!(got, s(&case["expected"]), "{o}");
    }
    for case in f["retentionCases"].as_array().unwrap() {
        let created = audit::parse_utc_ms(s(&case["createdAt"])).unwrap();
        let now = audit::parse_utc_ms(s(&case["now"])).unwrap();
        assert_eq!(audit::audit_expired(created, now), case["expected"] == true, "{case}");
    }
    assert_eq!(audit::parse_utc_ms("1970-01-01T00:00:01.5Z"), Some(1500));
}

// ---------------------------------------------------------------- access

fn grant(value: &Value) -> Grant {
    serde_json::from_value(value.clone()).unwrap()
}

fn level(value: &Value) -> Level {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn access_cases() {
    let f = fixture::access();
    for case in f["grantCases"].as_array().unwrap() {
        assert_eq!(access::grant_errors(&grant(&case["grant"])), strings(&case["expected"]), "{}", case["name"]);
    }
    for case in f["policyCases"].as_array().unwrap() {
        let row = (!case["row"].is_null()).then(|| level(&case["row"]));
        assert_eq!(access::member_policy(row), level(&case["expected"]));
    }
    for case in f["accessCases"].as_array().unwrap() {
        let input: AccessInput = serde_json::from_value(case["input"].clone()).unwrap();
        assert_eq!(access::effective_access(&input), level(&case["expected"]), "{}", case["name"]);
    }
    for case in f["visibleCases"].as_array().unwrap() {
        let tool = if case["access"] == "read" { ToolAccess::Read } else { ToolAccess::Write };
        assert_eq!(access::tool_visible(level(&case["level"]), tool), case["expected"] == true, "{case}");
    }
}

// ---------------------------------------------------------------- upstream

fn state(value: &Value) -> UpstreamState {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn upstream_cases() {
    let f = fixture::upstream();
    for name in f["states"].as_array().unwrap() {
        let st = state(name);
        assert_eq!(st.as_str(), s(name));
        assert_eq!(st.label(), s(&f["labels"][s(name)]));
    }
    assert_eq!(f["errors"]["disconnected"], upstream::ERROR_DISCONNECTED);
    assert_eq!(f["errors"]["missing"], upstream::ERROR_MISSING);
    assert_eq!(f["errors"]["failed"], upstream::ERROR_FAILED);
    for case in f["flowCases"].as_array().unwrap() {
        let end = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .fold(UpstreamState::Disconnected, |at, event| upstream::upstream_step(at, s(event)));
        assert_eq!(end, state(&case["expected"]), "{}", case["name"]);
    }
    for case in f["callableCases"].as_array().unwrap() {
        assert_eq!(upstream::upstream_callable(state(&case["state"])), case["expected"] == true);
    }
    for case in f["missingCases"].as_array().unwrap() {
        let declared: Vec<Vec<String>> = case["declared"].as_array().unwrap().iter().map(strings).collect();
        let states: HashMap<String, UpstreamState> =
            case["states"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), state(v))).collect();
        let got = upstream::missing_upstreams(&declared, &states, &strings(&case["servers"]));
        assert_eq!(got, strings(&case["expected"]), "{}", case["name"]);
    }
}

// ---------------------------------------------------------------- exposes

fn context(value: &Value) -> ExposeContext {
    ExposeContext { kinds: strings(&value["kinds"]), upstreams: strings(&value["upstreams"]) }
}

fn expected_errors(value: &Value) -> Vec<ExposeError> {
    let codes = fixture::exposes()["errors"].as_array().unwrap();
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|error| {
            let code = codes.iter().find(|code| **code == error["code"]).unwrap_or_else(|| panic!("unlisted {error}"));
            ExposeError { index: error["index"].as_u64().map(|i| i as usize), code: leak(s(code)) }
        })
        .collect()
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_string().into_boxed_str())
}

#[test]
fn exposes_constants_match() {
    let f = fixture::exposes();
    let limits = &f["limits"];
    assert_eq!(limits["nameMax"], exposes::NAME_MAX);
    assert_eq!(limits["titleMax"], exposes::TITLE_MAX);
    assert_eq!(limits["descriptionMax"], exposes::DESCRIPTION_MAX);
    assert_eq!(limits["maxTools"], exposes::MAX_TOOLS);
    assert_eq!(limits["separator"], exposes::SEPARATOR);
    assert_eq!(limits["toolNameMax"], exposes::TOOL_NAME_MAX);
    assert_eq!(limits["slugHashLength"], exposes::SLUG_HASH_LENGTH);
    assert_eq!(f["destructiveWords"], fixture::app_actions()["destructiveWords"]);
    assert_eq!(f["provenance"]["app"], exposes::META_APP);
    assert_eq!(f["provenance"]["tool"], exposes::META_TOOL);
    assert_eq!(f["provenance"]["via"], exposes::META_VIA);
    assert_eq!(f["provenance"]["upstream"], exposes::META_UPSTREAM);
}

#[test]
fn exposes_action_and_upstream_access_cases() {
    let f = fixture::exposes();
    for case in f["actionAccessCases"].as_array().unwrap() {
        assert_eq!(exposes::action_access(s(&case["name"])).as_str(), s(&case["expected"]), "{}", case["name"]);
    }
    for case in f["upstreamCases"].as_array().unwrap() {
        let annotations = (!case["annotations"].is_null()).then_some(&case["annotations"]);
        assert_eq!(exposes::upstream_destructive(s(&case["tool"]), annotations), case["destructive"] == true, "{case}");
        assert_eq!(exposes::upstream_access(annotations).as_str(), s(&case["access"]), "{case}");
    }
    for case in f["upstreamCallCases"].as_array().unwrap() {
        let access = if case["access"] == "read" { ToolAccess::Read } else { ToolAccess::Write };
        let annotations = (!case["annotations"].is_null()).then_some(&case["annotations"]);
        assert_eq!(exposes::upstream_call_error(access, s(&case["tool"]), annotations), opt_str(&case["expected"]), "{}", case["name"]);
    }
}

#[test]
fn exposes_validate_cases() {
    let f = fixture::exposes();
    for case in f["validateCases"].as_array().unwrap() {
        let got = exposes::validate_exposes(case["exposes"].as_array().unwrap(), &context(&case["context"]));
        assert_eq!(got, expected_errors(&case["expected"]), "{}", case["name"]);
    }
    let one = &f["validateCases"][1]["exposes"][0];
    let count = f["tooManyCase"]["count"].as_u64().unwrap() as usize;
    let many: Vec<Value> = (0..count)
        .map(|i| {
            let mut expose = one.clone();
            expose["name"] = format!("t{i}").into();
            expose
        })
        .collect();
    assert_eq!(exposes::validate_exposes(&many, &ExposeContext::default()), expected_errors(&f["tooManyCase"]["expected"]));
}

#[test]
fn exposes_resolve_cases() {
    for case in fixture::exposes()["resolveCases"].as_array().unwrap() {
        let got = exposes::resolve_args(&case["args"], &case["input"]);
        assert_eq!(got.as_ref(), Some(&case["expected"]), "{}", case["name"]);
    }
}

#[test]
fn exposes_naming_cases() {
    let f = fixture::exposes();
    for case in f["hashCases"].as_array().unwrap() {
        assert_eq!(exposes::short_hash(s(&case["text"]), exposes::SLUG_HASH_LENGTH), s(&case["expected"]));
    }
    for case in f["nameCases"].as_array().unwrap() {
        let got = exposes::qualified_tool_name(s(&case["slug"]), s(&case["name"]));
        assert_eq!(got, s(&case["expected"]), "{case}");
        assert!(got.len() <= exposes::TOOL_NAME_MAX);
    }
    for case in f["splitCases"].as_array().unwrap() {
        let got = exposes::split_tool_name(s(&case["name"]))
            .map(|(app, tool)| serde_json::json!({ "app": app, "tool": tool }));
        let expected = (!case["expected"].is_null()).then(|| case["expected"].clone());
        assert_eq!(got, expected, "{}", case["name"]);
    }
    for case in f["listCases"].as_array().unwrap() {
        let apps: Vec<(String, Vec<String>)> = case["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|app| {
                let names = app["exposes"].as_array().unwrap().iter().map(|e| s(&e["name"]).to_string()).collect();
                (s(&app["slug"]).to_string(), names)
            })
            .collect();
        let got: Vec<Value> = exposes::list_tool_names(&apps)
            .into_iter()
            .map(|t| serde_json::json!({ "app": t.app, "tool": t.tool, "name": t.name }))
            .collect();
        assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
    }
    for case in f["metaCases"].as_array().unwrap() {
        let got = exposes::provenance_meta(s(&case["slug"]), &case["expose"]);
        assert_eq!(Value::Object(got), case["expected"], "{case}");
    }
}

/// Every key of every fixture is replayed above (a new case list fails
/// here until a test reads it).
#[test]
fn every_case_list_is_replayed() {
    let replayed: &[(&Value, &[&str])] = &[
        (fixture::vault(), &["keyringCases", "aadCases", "sealVectors", "openCases", "malformedCases", "rotationCases"]),
        (fixture::audit(), &["canonicalCases", "hashCases", "statusCases", "retentionCases"]),
        (fixture::access(), &["grantCases", "policyCases", "accessCases", "visibleCases"]),
        (fixture::upstream(), &["flowCases", "callableCases", "missingCases"]),
        (
            fixture::exposes(),
            &[
                "actionAccessCases",
                "upstreamCases",
                "validateCases",
                "tooManyCase",
                "upstreamCallCases",
                "resolveCases",
                "nameCases",
                "hashCases",
                "splitCases",
                "listCases",
                "metaCases",
            ],
        ),
    ];
    for (f, keys) in replayed {
        for key in f.as_object().unwrap().keys().filter(|key| key.ends_with("Cases") || key.ends_with("Case") || key.ends_with("Vectors")) {
            assert!(keys.contains(&key.as_str()), "{key} is not replayed");
        }
    }
}
