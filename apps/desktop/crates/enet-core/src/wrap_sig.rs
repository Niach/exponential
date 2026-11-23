//! Signed tree key wraps (#EXP-1950; the spec is
//! `packages/domain-contract/fixtures/wrap-signature.json`, its TS reference
//! `packages/domain-contract/src/wrap-signature.ts`).
//!
//! The install that makes a wrap ([`crate::sealed::wrap_tree_key`]) signs it
//! with its [`Identity`], the ed25519 key whose public half is
//! `installs.endpoint_id`; an install keeps a wrap only when an install it
//! has PINNED signed it.
//!
//! ```text
//! message   = "exp/enet/tree-wrap/1" || 0x00 || rootId[16] || epoch u32 BE
//!             || treePublic[32] || recipient[32] || wrapped[81]      (186 bytes)
//! signature = ed25519(signer identity, message)                     (64 bytes)
//! ```
//!
//! `recipient` = the recipient install's ed25519 public key (its endpoint
//! id, never the row id: the server assigns those). Verification is iroh's
//! `verify_strict`. Nothing here is wired into a holder yet: no platform
//! signs or checks a wrap until the builds of #EXP-1950 follow, and who is
//! pinned is theirs.

use data_encoding::HEXLOWER;
use iroh::{PublicKey, Signature};

use crate::error::{NetError, Result};
use crate::identity::Identity;
use crate::push::PushKeypair;
use crate::sealed::{b64url_decode, b64url_encode, unwrap_tree_key, SEAL_EPOCH_MAX, WRAPPED_TREE_KEY_BYTES};

/// The message's domain label (one 0x00 follows it).
pub const WRAP_SIG_LABEL: &str = "exp/enet/tree-wrap/1";
pub const WRAP_SIG_MESSAGE_BYTES: usize = 186;
pub const WRAP_SIG_BYTES: usize = 64;

/// What a wrap signature covers.
#[derive(Debug, Clone, Copy)]
pub struct WrapStatement<'a> {
    /// The tree's root id (a UUID, any casing).
    pub root_id: &'a str,
    pub epoch: u32,
    /// base64url of that epoch's 32-byte tree public key.
    pub tree_public_key: &'a str,
    /// The recipient install's endpoint id (lowercase hex).
    pub recipient_endpoint_id: &'a str,
    /// `session_keys.wrapped` (base64url of 81 bytes).
    pub wrapped: &'a str,
}

/// The 16 bytes of a UUID in its 8-4-4-4-12 form (any casing).
fn uuid_bytes(text: &str) -> Option<[u8; 16]> {
    let b = text.as_bytes();
    if b.len() != 36 || [8, 13, 18, 23].iter().any(|i| b[*i] != b'-') {
        return None;
    }
    let hex: Vec<u8> = b.iter().filter(|c| **c != b'-').map(u8::to_ascii_lowercase).collect();
    HEXLOWER.decode(&hex).ok()?.try_into().ok()
}

/// The 32 bytes of an endpoint id (lowercase hex only).
fn endpoint_bytes(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    HEXLOWER.decode(text.as_bytes()).ok()?.try_into().ok()
}

fn valid_epoch(epoch: u32) -> bool {
    (1..=SEAL_EPOCH_MAX).contains(&epoch)
}

/// The 186 bytes a wrap's signer signs; `None` when a field is malformed.
pub fn wrap_signature_message(statement: &WrapStatement) -> Option<[u8; WRAP_SIG_MESSAGE_BYTES]> {
    if !valid_epoch(statement.epoch) {
        return None;
    }
    let root = uuid_bytes(statement.root_id)?;
    let recipient = endpoint_bytes(statement.recipient_endpoint_id)?;
    let tree_public: [u8; 32] = b64url_decode(statement.tree_public_key)?.try_into().ok()?;
    let wrapped = b64url_decode(statement.wrapped).filter(|w| w.len() == WRAPPED_TREE_KEY_BYTES)?;
    let mut out = [0u8; WRAP_SIG_MESSAGE_BYTES];
    out[..20].copy_from_slice(WRAP_SIG_LABEL.as_bytes());
    out[21..37].copy_from_slice(&root);
    out[37..41].copy_from_slice(&statement.epoch.to_be_bytes());
    out[41..73].copy_from_slice(&tree_public);
    out[73..105].copy_from_slice(&recipient);
    out[105..].copy_from_slice(&wrapped);
    Some(out)
}

/// An install's endpoint id: lowercase hex of its identity's public key.
pub fn endpoint_id_hex(identity: &Identity) -> String {
    HEXLOWER.encode(identity.endpoint_id().as_bytes())
}

/// `session_keys.signature`: the identity's signature over the statement
/// (base64url, 86 chars).
pub fn sign_wrap(identity: &Identity, statement: &WrapStatement) -> Result<String> {
    let message =
        wrap_signature_message(statement).ok_or_else(|| NetError::Invalid("malformed wrap statement".into()))?;
    Ok(b64url_encode(&identity.sign(&message)))
}

fn signature_bytes(text: &str) -> Option<[u8; WRAP_SIG_BYTES]> {
    b64url_decode(text)?.try_into().ok()
}

/// Whether `signature` is `signer_endpoint_id`'s over the statement. Says
/// nothing about the signer being trusted: [`accept_wrap`] asks the pins.
pub fn verify_wrap_signature(statement: &WrapStatement, signature: &str, signer_endpoint_id: &str) -> bool {
    let (Some(message), Some(signature), Some(signer)) =
        (wrap_signature_message(statement), signature_bytes(signature), endpoint_bytes(signer_endpoint_id))
    else {
        return false;
    };
    let Ok(signer) = PublicKey::from_bytes(&signer) else { return false };
    signer.verify(&message, &Signature::from_bytes(&signature)).is_ok()
}

/// Why a wrap is not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapRefusal {
    /// A field is not what the wire allows, or a signature came without
    /// its signer (or the reverse).
    Malformed,
    /// No signature, where one is required.
    Unsigned,
    /// The signer is not a pinned install.
    Unpinned,
    /// It does not open with this install's secret.
    Unopened,
    /// The signature is not the signer's over this root, epoch, key,
    /// recipient and bytes.
    Signature,
}

impl WrapRefusal {
    /// The fixture / TS spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            WrapRefusal::Malformed => "malformed",
            WrapRefusal::Unsigned => "unsigned",
            WrapRefusal::Unpinned => "unpinned",
            WrapRefusal::Unopened => "unopened",
            WrapRefusal::Signature => "signature",
        }
    }
}

/// A wrap as `sessionKeys.mine` answers it.
#[derive(Debug, Clone, Copy)]
pub struct ServedWrap<'a> {
    pub root_id: &'a str,
    pub epoch: u32,
    pub wrapped: &'a str,
    pub signature: Option<&'a str>,
    pub signer_endpoint_id: Option<&'a str>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum WrapVerdict {
    Verified { secret: [u8; 32], tree_public_key: String, signer_endpoint_id: String },
    /// Kept without a signature (only where none is required): an older
    /// build made it.
    Unsigned { secret: [u8; 32], tree_public_key: String },
    Refused(WrapRefusal),
}

impl std::fmt::Debug for WrapVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WrapVerdict::Verified { tree_public_key, signer_endpoint_id, .. } => {
                write!(f, "Verified(public={tree_public_key}, signer={signer_endpoint_id})")
            }
            WrapVerdict::Unsigned { tree_public_key, .. } => write!(f, "Unsigned(public={tree_public_key})"),
            WrapVerdict::Refused(reason) => write!(f, "Refused({})", reason.as_str()),
        }
    }
}

impl WrapVerdict {
    /// The fixture / TS spelling.
    pub fn status(&self) -> &'static str {
        match self {
            WrapVerdict::Verified { .. } => "verified",
            WrapVerdict::Unsigned { .. } => "unsigned",
            WrapVerdict::Refused(_) => "refused",
        }
    }
}

/// Whether this install keeps a served wrap. The install builds the signed
/// message ITSELF: the root and epoch it asked for, the public key of the
/// secret it unwrapped, its own endpoint id, the wrapped bytes. `pinned` =
/// whether an endpoint id is an install it has pinned. A wrap that carries a
/// signature must pass with it, also where none is `required`.
pub fn accept_wrap(
    wrap: &ServedWrap,
    identity: &Identity,
    install_secret: &[u8; 32],
    pinned: &dyn Fn(&str) -> bool,
    required: bool,
) -> WrapVerdict {
    let shaped = uuid_bytes(wrap.root_id).is_some()
        && valid_epoch(wrap.epoch)
        && b64url_decode(wrap.wrapped).is_some_and(|w| w.len() == WRAPPED_TREE_KEY_BYTES)
        && wrap.signature.is_some() == wrap.signer_endpoint_id.is_some()
        && wrap.signature.is_none_or(|s| signature_bytes(s).is_some())
        && wrap.signer_endpoint_id.is_none_or(|s| endpoint_bytes(s).is_some());
    if !shaped {
        return WrapVerdict::Refused(WrapRefusal::Malformed);
    }
    let signed = wrap.signature.zip(wrap.signer_endpoint_id);
    match signed {
        None if required => return WrapVerdict::Refused(WrapRefusal::Unsigned),
        Some((_, signer)) if !pinned(signer) => return WrapVerdict::Refused(WrapRefusal::Unpinned),
        _ => {}
    }
    let Ok(secret) = unwrap_tree_key(install_secret, wrap.wrapped) else {
        return WrapVerdict::Refused(WrapRefusal::Unopened);
    };
    let tree_public_key = b64url_encode(&PushKeypair::from_secret(secret).public());
    let Some((signature, signer)) = signed else {
        return WrapVerdict::Unsigned { secret, tree_public_key };
    };
    let recipient = endpoint_id_hex(identity);
    let statement = WrapStatement {
        root_id: wrap.root_id,
        epoch: wrap.epoch,
        tree_public_key: &tree_public_key,
        recipient_endpoint_id: &recipient,
        wrapped: wrap.wrapped,
    };
    if !verify_wrap_signature(&statement, signature, signer) {
        return WrapVerdict::Refused(WrapRefusal::Signature);
    }
    WrapVerdict::Verified { secret, tree_public_key, signer_endpoint_id: signer.to_string() }
}

/// Whether an install that holds NO secret of (root, epoch) may seal to
/// `statement.tree_public_key`: a signed wrap of that epoch made for another
/// install vouches for the key. Never [`WrapRefusal::Unopened`].
pub fn accept_vouch(
    statement: &WrapStatement,
    signature: Option<&str>,
    signer_endpoint_id: Option<&str>,
    pinned: &dyn Fn(&str) -> bool,
) -> std::result::Result<(), WrapRefusal> {
    let shaped = wrap_signature_message(statement).is_some()
        && signature.is_some() == signer_endpoint_id.is_some()
        && signature.is_none_or(|s| signature_bytes(s).is_some())
        && signer_endpoint_id.is_none_or(|s| endpoint_bytes(s).is_some());
    if !shaped {
        return Err(WrapRefusal::Malformed);
    }
    let Some((signature, signer)) = signature.zip(signer_endpoint_id) else {
        return Err(WrapRefusal::Unsigned);
    };
    if !pinned(signer) {
        return Err(WrapRefusal::Unpinned);
    }
    if !verify_wrap_signature(statement, signature, signer) {
        return Err(WrapRefusal::Signature);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sealed::{wrap_tree_key_with, TreeKey};
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/wrap-signature.json")).unwrap()
    }
    fn hex(v: &Value) -> Vec<u8> {
        HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap()
    }
    fn hex32(v: &Value) -> [u8; 32] {
        hex(v).try_into().unwrap()
    }
    fn text(v: &Value) -> &str {
        v.as_str().unwrap()
    }
    fn pins(case: &Value) -> Vec<String> {
        case["pins"].as_array().unwrap().iter().map(|p| text(p).to_string()).collect()
    }

    #[test]
    fn replays_the_constants_and_the_keys() {
        let fx = fixture();
        assert_eq!(text(&fx["label"]), WRAP_SIG_LABEL);
        assert_eq!(fx["messageBytes"].as_u64().unwrap() as usize, WRAP_SIG_MESSAGE_BYTES);
        assert_eq!(fx["signatureBytes"].as_u64().unwrap() as usize, WRAP_SIG_BYTES);
        let layout: u64 = fx["layout"].as_object().unwrap().values().map(|n| n.as_u64().unwrap()).sum();
        assert_eq!(layout as usize, WRAP_SIG_MESSAGE_BYTES);
        for install in ["signer", "recipient", "other"] {
            let identity = Identity::from_bytes(hex32(&fx[install]["identity"]));
            assert_eq!(endpoint_id_hex(&identity), text(&fx[install]["endpointId"]), "{install}");
        }
        let derived = PushKeypair::derive_from_identity_bytes(&hex32(&fx["recipient"]["identity"]));
        assert_eq!(derived.secret(), hex32(&fx["recipient"]["installSecret"]));
        assert_eq!(b64url_encode(&derived.public()), text(&fx["recipient"]["x25519Public"]));
        assert_eq!(TreeKey::from_secret(hex32(&fx["tree"]["secret"])).public_key(), text(&fx["tree"]["publicKey"]));
        assert_eq!(
            TreeKey::from_secret(hex32(&fx["otherTree"]["secret"])).public_key(),
            text(&fx["otherTree"]["publicKey"])
        );
    }

    #[test]
    fn replays_the_vector_byte_exact() {
        let fx = fixture();
        let v = &fx["vector"];
        let recipient_public = PushKeypair::from_secret(hex32(&fx["recipient"]["installSecret"])).public();
        let wrapped =
            wrap_tree_key_with(hex32(&v["ephemeralSecret"]), &recipient_public, &hex32(&fx["tree"]["secret"])).unwrap();
        assert_eq!(wrapped, text(&v["wrapped"]));
        let statement = WrapStatement {
            root_id: text(&v["rootId"]),
            epoch: v["epoch"].as_u64().unwrap() as u32,
            tree_public_key: text(&v["treePublicKey"]),
            recipient_endpoint_id: text(&v["recipientEndpointId"]),
            wrapped: &wrapped,
        };
        assert_eq!(wrap_signature_message(&statement).unwrap().to_vec(), hex(&v["message"]));
        let signer = Identity::from_bytes(hex32(&fx["signer"]["identity"]));
        let signature = sign_wrap(&signer, &statement).unwrap();
        assert_eq!(signature, text(&v["signature"]));
        assert_eq!(b64url_decode(&signature).unwrap(), hex(&v["signatureHex"]));
        assert!(verify_wrap_signature(&statement, &signature, text(&fx["signer"]["endpointId"])));
        assert!(!verify_wrap_signature(&statement, &signature, text(&fx["other"]["endpointId"])));
        assert!(!verify_wrap_signature(&WrapStatement { epoch: 2, ..statement }, &signature, text(&fx["signer"]["endpointId"])));
        assert!(sign_wrap(&signer, &WrapStatement { epoch: 0, ..statement }).is_err());
    }

    fn accept(fx: &Value, case: &Value) -> WrapVerdict {
        let identity = Identity::from_bytes(hex32(&fx["recipient"]["identity"]));
        let pins = pins(case);
        // The fixture's epochs fit u32; one above the wire's ceiling stays malformed.
        let epoch = u32::try_from(case["epoch"].as_u64().unwrap()).unwrap_or(u32::MAX);
        accept_wrap(
            &ServedWrap {
                root_id: text(&case["rootId"]),
                epoch,
                wrapped: text(&case["wrapped"]),
                signature: case["signature"].as_str(),
                signer_endpoint_id: case["signerEndpointId"].as_str(),
            },
            &identity,
            &hex32(&fx["recipient"]["installSecret"]),
            &|id| pins.iter().any(|pin| pin == id),
            case["required"].as_bool().unwrap(),
        )
    }

    #[test]
    fn replays_accepted() {
        let fx = fixture();
        for case in fx["accepted"].as_array().unwrap() {
            let name = text(&case["name"]);
            let got = accept(&fx, case);
            assert_eq!(got.status(), text(&case["status"]), "{name}");
            match got {
                WrapVerdict::Verified { secret, tree_public_key, signer_endpoint_id } => {
                    assert_eq!(secret, hex32(&fx["tree"]["secret"]), "{name}");
                    assert_eq!(tree_public_key, text(&fx["tree"]["publicKey"]), "{name}");
                    assert_eq!(signer_endpoint_id, text(&case["signerEndpointId"]), "{name}");
                }
                WrapVerdict::Unsigned { secret, tree_public_key } => {
                    assert_eq!(secret, hex32(&fx["tree"]["secret"]), "{name}");
                    assert_eq!(tree_public_key, text(&fx["tree"]["publicKey"]), "{name}");
                }
                WrapVerdict::Refused(reason) => panic!("{name}: refused {}", reason.as_str()),
            }
        }
    }

    #[test]
    fn replays_refused_each_with_its_reason() {
        let fx = fixture();
        let mut seen: Vec<&str> = Vec::new();
        for case in fx["refused"].as_array().unwrap() {
            let name = text(&case["name"]);
            match accept(&fx, case) {
                WrapVerdict::Refused(reason) => {
                    assert_eq!(reason.as_str(), text(&case["reason"]), "{name}");
                    seen.push(reason.as_str());
                }
                other => panic!("{name}: {other:?}"),
            }
        }
        for reason in fx["refusals"].as_array().unwrap() {
            assert!(seen.contains(&text(reason)), "no case refuses as {reason}");
        }
    }

    #[test]
    fn replays_the_vouch_cases() {
        let fx = fixture();
        for case in fx["vouchCases"].as_array().unwrap() {
            let name = text(&case["name"]);
            let pins = pins(case);
            let got = accept_vouch(
                &WrapStatement {
                    root_id: text(&case["rootId"]),
                    epoch: case["epoch"].as_u64().unwrap() as u32,
                    tree_public_key: text(&case["treePublicKey"]),
                    recipient_endpoint_id: text(&case["recipientEndpointId"]),
                    wrapped: text(&case["wrapped"]),
                },
                case["signature"].as_str(),
                case["signerEndpointId"].as_str(),
                &|id| pins.iter().any(|pin| pin == id),
            );
            let got = got.map_or_else(|reason| reason.as_str(), |()| "verified");
            assert_eq!(got, text(&case["result"]), "{name}");
        }
    }
}
