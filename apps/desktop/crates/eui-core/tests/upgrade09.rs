//! VAPP-289 (round 9 §14): replays `fixtures/upgrade09.json` byte for byte
//! (key order included), like `src/upgrade09.test.ts`. The Ed25519 itself is
//! the host's: the `verify` cases hand the verifier the fixture's status.

use eui_core::upgrade09::{
    upgrade_card09, upgrade_client_message09, upgrade_message09, upgrade_package09, upgrade_package_body09, PackageVerification, Upgrade09State,
};
use serde_json::Value;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/upgrade09.json");

fn bytes(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

#[test]
fn replays_every_case() {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    let cases = doc["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    let mut failed = Vec::new();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let mut check = |what: &str, got: String, want: String| {
            if got != want {
                failed.push(format!("{name} {what}:\n  got  {got}\n  want {want}"));
            }
        };
        match c["kind"].as_str().unwrap() {
            "message" => {
                let (messages, state) = upgrade_message09(&c["input"], &Upgrade09State::default());
                assert_eq!(messages.len(), 1, "{name}");
                check("message", bytes(&messages[0]), bytes(&c["expected"]));
                let want: Upgrade09State = serde_json::from_value(c["expectedState"].clone()).unwrap();
                check("state", format!("{state:?}"), format!("{want:?}"));
            }
            "stream" => {
                let mut state = Upgrade09State::default();
                let mut out = Vec::new();
                for m in c["inputs"].as_array().unwrap() {
                    let before = state.clone();
                    let (messages, next) = upgrade_message09(m, &state);
                    assert_eq!(state, before, "{name}: the state passed in is never mutated");
                    assert_eq!(messages.len(), 1, "{name}");
                    out.extend(messages);
                    state = next;
                }
                check("stream", bytes(&Value::Array(out)), bytes(&c["expected"]));
                let want: Upgrade09State = serde_json::from_value(c["expectedState"].clone()).unwrap();
                check("state", format!("{state:?}"), format!("{want:?}"));
            }
            "client" => check("client", bytes(&upgrade_client_message09(&c["input"])), bytes(&c["expected"])),
            "card" => check("card", bytes(&upgrade_card09(&c["input"])), bytes(&c["expected"])),
            "package" => {
                if c["verify"] == Value::Bool(true) {
                    let expected = &c["expected"];
                    let u = upgrade_package09(&c["input"], |_| PackageVerification {
                        status: expected["status"].as_str().unwrap().to_string(),
                        author: expected.get("author").cloned(),
                    });
                    let mut out = serde_json::Map::new();
                    out.insert("package".into(), u.package.unwrap_or(Value::Null));
                    out.insert("status".into(), Value::String(u.status));
                    if let Some(a) = u.author {
                        out.insert("author".into(), a);
                    }
                    check("package", bytes(&Value::Object(out)), bytes(expected));
                } else {
                    check("package", bytes(&upgrade_package_body09(&c["input"])), bytes(&c["expected"]));
                }
            }
            other => panic!("{name}: unknown kind {other}"),
        }
    }
    assert!(failed.is_empty(), "{} of {} cases failed:\n{}", failed.len(), cases.len(), failed.join("\n"));
}
