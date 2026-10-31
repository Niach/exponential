//! packages/ecore/fixtures/ecore.json, replayed against the generated rules
//! and the Rust shapes (TS `ecore.test.ts`, Swift `ECoreTests`, Kotlin
//! `ECoreTest` replay the same file).

use ecore::*;
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/ecore/fixtures/ecore.json")).unwrap()
}

fn cases<'a>(fixture: &'a Value, key: &str) -> &'a Vec<Value> {
    fixture[key].as_array().unwrap_or_else(|| panic!("{key} is an array"))
}

#[test]
fn ids() {
    let f = fixture();
    for case in cases(&f, "idCases") {
        let id = case["id"].as_str().unwrap();
        let valid = case["valid"].as_bool().unwrap();
        assert_eq!(is_valid_id(id), valid, "{id}");
        assert_eq!(SessionId::new(id).is_ok(), valid, "{id}");
        assert_eq!(serde_json::from_value::<DeviceId>(json!(id)).is_ok(), valid, "{id}");
    }
}

#[test]
fn tokens_and_bearer() {
    let f = fixture();
    for case in cases(&f, "tokenCases") {
        let token = case["token"].as_str().unwrap();
        assert_eq!(is_valid_token(token), case["valid"].as_bool().unwrap(), "{token}");
    }
    for case in cases(&f, "bearerCases") {
        let header = case["header"].as_str().unwrap();
        assert_eq!(parse_bearer(header), case["token"].as_str(), "{header:?}");
        assert_eq!(Token::from_header(header).map(|t| t.as_str().to_string()).as_deref(), case["token"].as_str());
    }
    let b = &f["bearerHeader"];
    assert_eq!(bearer_header(b["token"].as_str().unwrap()), b["header"].as_str().unwrap());
    let token = Token::new(b["token"].as_str().unwrap()).unwrap();
    assert_eq!(token.header(), b["header"].as_str().unwrap());
    assert!(!format!("{token:?}").contains(token.as_str()), "Debug hides the token");
}

#[test]
fn capabilities_and_grants() {
    let f = fixture();
    for case in cases(&f, "capabilityCases") {
        let name = case["name"].as_str().unwrap();
        let known = case["known"].as_bool().unwrap();
        assert_eq!(Capability::parse(name).is_some(), known, "{name}");
        if known {
            assert_eq!(Capability::parse(name).unwrap().as_str(), name);
            assert_eq!(serde_json::to_value(Capability::parse(name).unwrap()).unwrap(), json!(name));
        }
    }
    for case in cases(&f, "grantCases") {
        let grant: Grant = serde_json::from_value(json!({
            "token": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "sessionId": "s-1",
            "capabilities": case["capabilities"],
            "expiresAt": case["expiresAt"],
        }))
        .unwrap();
        let capability = Capability::parse(case["capability"].as_str().unwrap()).unwrap();
        let now = case["now"].as_i64().unwrap();
        assert_eq!(grant.admits(capability, now), case["admits"].as_bool().unwrap(), "{case}");
    }
}

#[test]
fn verdicts_and_errors() {
    let f = fixture();
    let verdicts = cases(&f, "verdictCases");
    assert_eq!(verdicts.len(), DeliveryVerdict::ALL.len());
    for case in verdicts {
        let v: DeliveryVerdict = serde_json::from_value(case["verdict"].clone()).unwrap();
        assert_eq!(v.may_retry(), case["mayRetry"].as_bool().unwrap(), "{v}");
    }
    let errors = cases(&f, "errorCases");
    assert_eq!(errors.len(), ErrorCode::ALL.len());
    for case in errors {
        let code: ErrorCode = serde_json::from_value(case["code"].clone()).unwrap();
        assert_eq!(code.is_refusal(), case["refusal"].as_bool().unwrap(), "{code}");
        assert_eq!(code.default_retryable(), case["retryable"].as_bool().unwrap(), "{code}");
        let failure = Failure::new(code, "x");
        assert_eq!(failure.retryable, code.default_retryable());
        assert_eq!(failure.is_refusal(), code.is_refusal());
    }
    for case in f["errorMessage"]["cases"].as_array().unwrap() {
        if let Some(message) = case["message"].as_str() {
            assert_eq!(clamp_message(message), case["clamped"].as_str().unwrap());
        } else {
            let long = case["repeat"].as_str().unwrap().repeat(case["times"].as_u64().unwrap() as usize);
            let clamped = Failure::new(ErrorCode::Internal, &long).message;
            assert_eq!(clamped.chars().count() as u64, case["clampedLength"].as_u64().unwrap());
        }
    }
}

#[test]
fn envelopes() {
    let f = fixture();
    for case in cases(&f, "envelopeCases") {
        let mut raw = case["envelope"].clone();
        raw["data"] = json!({});
        let envelope: Envelope = serde_json::from_value(raw).unwrap();
        let expected: Vec<&str> = case["errors"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
        assert_eq!(envelope.errors(), expected, "{}", case["envelope"]);
    }
    let session = SessionId::new("s-1").unwrap();
    let built = Envelope::new("evt-1", "session.ended", 1, "eagent", Some(&session), json!({ "ok": true }));
    assert!(built.errors().is_empty());
    let wire = serde_json::to_value(&built).unwrap();
    assert_eq!(wire["v"], json!(ENVELOPE_VERSION));
    assert_eq!(wire["sessionId"], json!("s-1"));
}
