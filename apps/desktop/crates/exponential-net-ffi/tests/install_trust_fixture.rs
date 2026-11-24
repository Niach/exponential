//! #EXP-1950: packages/domain-contract/fixtures/install-trust.json through
//! the facade's `identityVerify` (strict ed25519) and `identitySign`: every
//! key's stored registration, the approval vector byte-exact, and what must
//! fail. The phones build the messages and run `reconcileTrust` themselves
//! (iOS ExpCore `InstallTrust`, replaying every case of the file).

use exponential_net_ffi::*;
use serde_json::Value;

fn hex(v: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap()
}

fn b64(v: &Value) -> Vec<u8> {
    data_encoding::BASE64URL_NOPAD.decode(v.as_str().unwrap().as_bytes()).unwrap()
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/install-trust.json")).unwrap()
}

fn registration(key: &Value) -> Vec<u8> {
    format!(
        "exp-install-v1\n{}\n{}\n{}\n{}",
        key["userId"].as_str().unwrap(),
        key["endpointId"].as_str().unwrap(),
        key["x25519Public"].as_str().unwrap(),
        key["signedAt"].as_str().unwrap()
    )
    .into_bytes()
}

#[test]
fn every_keys_registration_verifies_and_a_forged_one_does_not() {
    let fx = fixture();
    let keys = fx["keys"].as_object().unwrap();
    let z = &keys["Z"];
    for (name, key) in keys {
        let endpoint = key["endpointId"].as_str().unwrap().to_string();
        assert_eq!(endpoint_id_of(hex(&key["identity"])).unwrap(), endpoint, "{name}");
        let message = registration(key);
        let signature = b64(&key["registrationSignature"]);
        assert_eq!(identity_sign(hex(&key["identity"]), message.clone()).unwrap(), signature, "{name}");
        assert!(identity_verify(endpoint.clone(), message.clone(), signature.clone()), "{name}");
        // `registration: forged` = the server's install Z signed it.
        let forged = identity_sign(hex(&z["identity"]), message.clone()).unwrap();
        assert_eq!(identity_verify(endpoint.clone(), message.clone(), forged), name == "Z", "{name} forged");
        let mut other = message.clone();
        other.push(b'x');
        assert!(!identity_verify(endpoint, other, signature), "{name} tampered");
    }
}

#[test]
fn the_approval_vector_byte_exact() {
    let fx = fixture();
    let v = &fx["approval"]["vector"];
    let message = hex(&v["message"]);
    assert_eq!(message.len() as u64, fx["approval"]["messageBytes"].as_u64().unwrap());
    let approver = v["approverEndpointId"].as_str().unwrap().to_string();
    let signature = b64(&v["signature"]);
    assert_eq!(identity_sign(hex(&fx["keys"]["A"]["identity"]), message.clone()).unwrap(), signature);
    assert!(identity_verify(approver.clone(), message.clone(), signature.clone()));
    // Another kind (byte 28 after the label and its 0x00) fails.
    let mut reset = message.clone();
    reset[28] = 3;
    assert!(!identity_verify(approver.clone(), reset, signature.clone()));
    // Another approver fails.
    let b = fx["keys"]["B"]["endpointId"].as_str().unwrap().to_string();
    assert!(!identity_verify(b, message.clone(), signature.clone()));
    // Malformed: never a panic, always false.
    assert!(!identity_verify(approver.to_uppercase(), message.clone(), signature.clone()));
    assert!(!identity_verify("not hex".into(), message.clone(), signature.clone()));
    assert!(!identity_verify(approver.clone(), message.clone(), signature[..63].to_vec()));
    // A small-order key (the identity point) is refused by the strict check.
    let mut identity_point = vec![0u8; 32];
    identity_point[0] = 1;
    let small = data_encoding::HEXLOWER.encode(&identity_point);
    let mut zero_sig = vec![0u8; 64];
    zero_sig[0] = 1;
    assert!(!identity_verify(small, message, zero_sig));
}
