//! The sealed-content vectors (packages/domain-contract/fixtures/sealed-content.json)
//! through the facade's free functions, as Swift/Kotlin call them: runs in
//! the push-only build too (`cargo test -p exponential-net-ffi --no-default-features`).
//! The byte-exact seals (fixed ephemeral keys) are replayed in enet-core.

use exponential_net_ffi::*;
use serde_json::Value;

fn hex(v: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap()
}

fn status(s: EnetSealStatus) -> &'static str {
    match s {
        EnetSealStatus::Plain => "plain",
        EnetSealStatus::Opened => "opened",
        EnetSealStatus::NoKey => "noKey",
        EnetSealStatus::Unsupported => "unsupported",
        EnetSealStatus::Malformed => "malformed",
        EnetSealStatus::Refused => "refused",
    }
}

#[test]
fn replays_the_sealed_content_fixture() {
    let fx: Value =
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap();
    let keys = || -> Vec<EnetEpochKey> {
        fx["keys"].as_object().unwrap().iter().map(|(e, s)| EnetEpochKey { epoch: e.parse().unwrap(), secret: hex(s) }).collect()
    };
    let w = &fx["wrap"];
    assert_eq!(unwrap_tree_key(hex(&fx["install"]["secret"]), w["wrapped"].as_str().unwrap().into()).unwrap(), hex(&w["treeSecret"]));
    assert!(unwrap_tree_key(vec![0; 31], w["wrapped"].as_str().unwrap().into()).is_err());

    for case in fx["bodyCases"].as_array().unwrap() {
        let got = open_body(case["body"].as_str().unwrap().into(), keys()).unwrap();
        assert_eq!(status(got.status), case["status"].as_str().unwrap(), "{}", case["name"]);
        assert_eq!(got.epoch.map(u64::from), case["epoch"].as_u64());
        assert_eq!(got.text.as_deref(), case["text"].as_str());
    }
    for case in fx["cardCases"].as_array().unwrap() {
        let got = open_card(case["card"].to_string(), keys()).unwrap();
        assert_eq!(status(got.status), case["status"].as_str().unwrap(), "{}", case["name"]);
        assert_eq!(got.epoch.map(u64::from), case["epoch"].as_u64());
        assert_eq!(serde_json::from_str::<Value>(&got.card_json).unwrap(), case["opened"]);
    }
    for split in fx["card"]["splits"].as_array().unwrap() {
        let got = open_card(split["envelope"].to_string(), keys()).unwrap();
        assert_eq!(got.status, EnetSealStatus::Opened, "{}", split["name"]);
    }

    // Round trips with a fresh tree key.
    let tree = generate_tree_key();
    let own = || vec![EnetEpochKey { epoch: 5, secret: tree.secret.clone() }];
    let wrapped = wrap_tree_key(fx["install"]["x25519Public"].as_str().unwrap().into(), tree.secret.clone()).unwrap();
    assert_eq!(unwrap_tree_key(hex(&fx["install"]["secret"]), wrapped).unwrap(), tree.secret);
    let body = seal_body(tree.public_key.clone(), 5, "hello".into()).unwrap();
    assert!(body.starts_with("exp-sealed:v1:5:"));
    assert_eq!(open_body(body, own()).unwrap().text.as_deref(), Some("hello"));
    let card = seal_card(tree.public_key.clone(), 5, r#"{"kind":"report","sessionId":"s","summary":"done","final":true}"#.into()).unwrap();
    let clear: Value = serde_json::from_str(&card).unwrap();
    assert!(clear.get("summary").is_none() && clear["final"] == Value::Bool(true));
    let opened: Value = serde_json::from_str(&open_card(card, own()).unwrap().card_json).unwrap();
    assert_eq!(opened["summary"], "done");
    assert!(seal_card(tree.public_key, 5, "[]".into()).is_err());
    assert!(open_body("exp-sealed:v1:1:AAAA".into(), vec![EnetEpochKey { epoch: 1, secret: vec![1] }]).is_err());
}
