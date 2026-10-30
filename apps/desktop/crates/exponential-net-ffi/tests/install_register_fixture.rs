//! The shared install registration vector
//! (packages/domain-contract/fixtures/install-register.json) through the
//! facade: endpoint id, derived X25519 key and `identitySign` byte-exact.

use exponential_net_ffi::*;
use serde_json::Value;

fn hex(v: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap()
}

#[test]
fn replays_the_install_register_fixture() {
    let fx: Value =
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/install-register.json"))
            .unwrap();
    let identity = hex(&fx["identity"]);
    let endpoint_id = endpoint_id_of(identity.clone()).unwrap();
    assert_eq!(endpoint_id, fx["endpointId"].as_str().unwrap());
    let x25519 = data_encoding::BASE64URL_NOPAD.encode(&push_keypair_derive(identity.clone()).unwrap().public);
    assert_eq!(x25519, fx["x25519Public"].as_str().unwrap());

    let message = format!(
        "exp-install-v1\n{}\n{endpoint_id}\n{x25519}\n{}",
        fx["userId"].as_str().unwrap(),
        fx["signedAt"].as_str().unwrap()
    );
    assert_eq!(message, fx["message"].as_str().unwrap());
    let signature = identity_sign(identity, message.into_bytes()).unwrap();
    assert_eq!(signature, hex(&fx["signatureHex"]));
    assert_eq!(data_encoding::BASE64URL_NOPAD.encode(&signature), fx["signature"].as_str().unwrap());

    assert!(identity_sign(vec![0; 31], b"x".to_vec()).is_err());
}
