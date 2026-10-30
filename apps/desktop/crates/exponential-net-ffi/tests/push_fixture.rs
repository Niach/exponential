//! The shared push-seal vectors (packages/domain-contract/fixtures/push-seal.json)
//! through the facade's free functions: runs in the push-only build too
//! (`cargo test -p exponential-net-ffi --no-default-features`).

use exponential_net_ffi::*;
use serde_json::Value;

fn hex(v: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap()
}

#[test]
fn replays_the_push_seal_fixture() {
    let fx: Value =
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/push-seal.json")).unwrap();
    let secret = hex(&fx["recipient"]["secret"]);
    let v = &fx["vector"];
    let opened = push_open(secret.clone(), hex(&v["sealed"])).unwrap();
    assert_eq!(opened, v["plaintext"].as_str().unwrap().as_bytes());
    let r = &fx["rustSealed"];
    assert_eq!(push_open(secret.clone(), hex(&r["sealed"])).unwrap(), r["plaintext"].as_str().unwrap().as_bytes());

    let d = &fx["derive"];
    let kp = push_keypair_derive(hex(&d["identity"])).unwrap();
    assert_eq!(kp.secret, hex(&d["secret"]));
    assert_eq!(kp.public, hex(&d["public"]));

    // A fresh seal to the fixture's public key opens with its secret.
    let sealed = push_seal(hex(&fx["recipient"]["public"]), b"wake".to_vec()).unwrap();
    assert_eq!(push_open(secret, sealed).unwrap(), b"wake");

    for case in fx["refused"].as_array().unwrap() {
        let r = push_open(hex(&case["secret"]), hex(&case["sealed"]));
        assert!(matches!(r, Err(EnetError::Crypto { .. })), "{}: {r:?}", case["name"]);
    }
    assert!(push_seal(hex(&fx["lowOrderPublic"]), b"x".to_vec()).is_err());
}
