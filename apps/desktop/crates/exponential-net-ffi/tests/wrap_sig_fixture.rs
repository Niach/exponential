//! #EXP-1950: packages/domain-contract/fixtures/wrap-signature.json through
//! the facade's free functions, as Swift and Kotlin call them: the vector,
//! every `accepted`, `refused` and `vouchCases` case by name. In the
//! push-only build (`--no-default-features`) every function fails closed.

use exponential_net_ffi::*;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/wrap-signature.json")).unwrap()
}

#[cfg(feature = "node")]
mod node {
    use super::*;

    fn hex(v: &Value) -> Vec<u8> {
        data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap()
    }
    fn text(v: &Value) -> String {
        v.as_str().unwrap().to_string()
    }
    fn pins(case: &Value) -> Vec<String> {
        case["pins"].as_array().unwrap().iter().map(text).collect()
    }
    fn refusal(r: EnetWrapRefusal) -> &'static str {
        match r {
            EnetWrapRefusal::Malformed => "malformed",
            EnetWrapRefusal::Unsigned => "unsigned",
            EnetWrapRefusal::Unpinned => "unpinned",
            EnetWrapRefusal::Unopened => "unopened",
            EnetWrapRefusal::Signature => "signature",
        }
    }

    fn accept(fx: &Value, case: &Value) -> EnetWrapVerdict {
        accept_wrap(
            EnetServedWrap {
                root_id: text(&case["rootId"]),
                // One epoch above u32 stays malformed.
                epoch: u32::try_from(case["epoch"].as_u64().unwrap()).unwrap_or(u32::MAX),
                wrapped: text(&case["wrapped"]),
                signature: case["signature"].as_str().map(Into::into),
                signer_endpoint_id: case["signerEndpointId"].as_str().map(Into::into),
            },
            hex(&fx["recipient"]["identity"]),
            hex(&fx["recipient"]["installSecret"]),
            pins(case),
            case["required"].as_bool().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn the_installs_endpoint_ids_are_the_fixtures() {
        let fx = fixture();
        for install in ["signer", "recipient", "other"] {
            assert_eq!(endpoint_id_of(hex(&fx[install]["identity"])).unwrap(), text(&fx[install]["endpointId"]), "{install}");
        }
    }

    #[test]
    fn signs_the_vector_byte_exact() {
        let fx = fixture();
        let v = &fx["vector"];
        let statement = EnetWrapStatement {
            root_id: text(&v["rootId"]),
            epoch: v["epoch"].as_u64().unwrap() as u32,
            tree_public_key: text(&v["treePublicKey"]),
            recipient_endpoint_id: text(&v["recipientEndpointId"]),
            wrapped: text(&v["wrapped"]),
        };
        let message = wrap_signature_message(statement.clone()).unwrap();
        assert_eq!(message.len(), fx["messageBytes"].as_u64().unwrap() as usize);
        assert_eq!(message, hex(&v["message"]));
        let signature = sign_wrap(hex(&fx["signer"]["identity"]), statement.clone()).unwrap();
        assert_eq!(signature, text(&v["signature"]));
        let signer = text(&fx["signer"]["endpointId"]);
        assert!(verify_wrap_signature(statement.clone(), signature.clone(), signer.clone()).unwrap());
        assert!(!verify_wrap_signature(statement.clone(), signature.clone(), text(&fx["other"]["endpointId"])).unwrap());
        assert!(!verify_wrap_signature(EnetWrapStatement { epoch: 2, ..statement.clone() }, signature, signer).unwrap());
        let malformed = EnetWrapStatement { epoch: 0, ..statement.clone() };
        assert!(matches!(sign_wrap(hex(&fx["signer"]["identity"]), malformed.clone()), Err(EnetError::Invalid { .. })));
        assert!(matches!(wrap_signature_message(malformed), Err(EnetError::Invalid { .. })));
        assert!(matches!(sign_wrap(vec![0; 31], statement), Err(EnetError::Invalid { .. })));
    }

    #[test]
    fn replays_accepted() {
        let fx = fixture();
        for case in fx["accepted"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let (status, secret, public, signer) = match accept(&fx, case) {
                EnetWrapVerdict::Verified { secret, tree_public_key, signer_endpoint_id } => {
                    ("verified", secret, tree_public_key, Some(signer_endpoint_id))
                }
                EnetWrapVerdict::Unsigned { secret, tree_public_key } => ("unsigned", secret, tree_public_key, None),
                EnetWrapVerdict::Refused { reason } => panic!("{name}: refused {}", refusal(reason)),
            };
            assert_eq!(status, text(&case["status"]), "{name}");
            assert_eq!(secret, hex(&fx["tree"]["secret"]), "{name}");
            assert_eq!(public, text(&fx["tree"]["publicKey"]), "{name}");
            assert_eq!(signer.as_deref(), case["signerEndpointId"].as_str(), "{name}");
        }
    }

    #[test]
    fn replays_refused_each_with_its_reason() {
        let fx = fixture();
        let mut seen = Vec::new();
        for case in fx["refused"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            match accept(&fx, case) {
                EnetWrapVerdict::Refused { reason } => {
                    assert_eq!(refusal(reason), text(&case["reason"]), "{name}");
                    seen.push(refusal(reason));
                }
                other => panic!("{name}: {other:?}"),
            }
        }
        for reason in fx["refusals"].as_array().unwrap() {
            assert!(seen.contains(&reason.as_str().unwrap()), "no case refuses as {reason}");
        }
    }

    #[test]
    fn replays_the_vouch_cases() {
        let fx = fixture();
        for case in fx["vouchCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let got = accept_vouch(
                EnetWrapStatement {
                    root_id: text(&case["rootId"]),
                    epoch: case["epoch"].as_u64().unwrap() as u32,
                    tree_public_key: text(&case["treePublicKey"]),
                    recipient_endpoint_id: text(&case["recipientEndpointId"]),
                    wrapped: text(&case["wrapped"]),
                },
                case["signature"].as_str().map(Into::into),
                case["signerEndpointId"].as_str().map(Into::into),
                pins(case),
            )
            .unwrap();
            let got = match got {
                EnetVouchVerdict::Verified => "verified",
                EnetVouchVerdict::Refused { reason } => refusal(reason),
            };
            assert_eq!(got, text(&case["result"]), "{name}");
        }
    }

    #[test]
    fn a_short_identity_or_install_secret_is_invalid() {
        let fx = fixture();
        let case = &fx["accepted"][0];
        let wrap = EnetServedWrap {
            root_id: text(&case["rootId"]),
            epoch: 1,
            wrapped: text(&case["wrapped"]),
            signature: None,
            signer_endpoint_id: None,
        };
        let secret = hex(&fx["recipient"]["installSecret"]);
        assert!(matches!(accept_wrap(wrap.clone(), vec![0; 31], secret, vec![], false), Err(EnetError::Invalid { .. })));
        assert!(matches!(
            accept_wrap(wrap, hex(&fx["recipient"]["identity"]), vec![0; 31], vec![], false),
            Err(EnetError::Invalid { .. })
        ));
    }
}

/// The push-only build signs, verifies, keeps and vouches nothing.
#[cfg(not(feature = "node"))]
#[test]
fn the_push_only_build_fails_closed() {
    let fx = fixture();
    let case = &fx["vouchCases"][0];
    let s = |key: &str| case[key].as_str().unwrap().to_string();
    let statement = EnetWrapStatement {
        root_id: s("rootId"),
        epoch: 1,
        tree_public_key: s("treePublicKey"),
        recipient_endpoint_id: s("recipientEndpointId"),
        wrapped: s("wrapped"),
    };
    let unavailable = |e: EnetError| matches!(e, EnetError::Crypto { reason } if reason == "wrap signature: unavailable");
    assert!(unavailable(wrap_signature_message(statement.clone()).unwrap_err()));
    assert!(unavailable(sign_wrap(vec![1; 32], statement.clone()).unwrap_err()));
    assert!(unavailable(verify_wrap_signature(statement.clone(), s("signature"), s("signerEndpointId")).unwrap_err()));
    let wrap = EnetServedWrap {
        root_id: s("rootId"),
        epoch: 1,
        wrapped: s("wrapped"),
        signature: Some(s("signature")),
        signer_endpoint_id: Some(s("signerEndpointId")),
    };
    assert!(unavailable(accept_wrap(wrap, vec![1; 32], vec![1; 32], vec![s("signerEndpointId")], false).unwrap_err()));
    assert!(unavailable(
        accept_vouch(statement, Some(s("signature")), Some(s("signerEndpointId")), vec![s("signerEndpointId")]).unwrap_err()
    ));
}
