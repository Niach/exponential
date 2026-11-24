//! #EXP-1950: signed tree key wraps (`enet_core::wrap_sig`; packages/
//! domain-contract/fixtures/wrap-signature.json is the spec) for the phone
//! callers: sign a wrap, verify a signature, accept a served wrap, accept a
//! vouch. Identities, install secrets and pins cross as the phones hold
//! them (32 identity bytes, 32 X25519 secret bytes, endpoint ids as
//! lowercase hex, `endpointIdOf`). Nothing calls these yet: no app signs or
//! checks a wrap until the builds of #EXP-1950 follow.
//!
//! The ed25519 is the identity's (iroh), so it lives behind `node`. The
//! PUSH-ONLY build (`--no-default-features`, the iOS Notification Service
//! Extension's `ExponentialNetPush`) exports the same functions and FAILS
//! CLOSED: every one answers `EnetError.Crypto` with the reason
//! `wrap signature: unavailable`, so nothing is ever signed, verified, kept
//! or vouched there.

use crate::EnetError;

/// What a wrap signature covers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EnetWrapStatement {
    /// The tree's root id (a UUID, any casing).
    pub root_id: String,
    pub epoch: u32,
    /// base64url of that epoch's 32-byte tree public key.
    pub tree_public_key: String,
    /// The recipient install's endpoint id (lowercase hex).
    pub recipient_endpoint_id: String,
    /// `session_keys.wrapped` (base64url of 81 bytes).
    pub wrapped: String,
}

/// A wrap as `sessionKeys.mine` answers it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EnetServedWrap {
    pub root_id: String,
    pub epoch: u32,
    pub wrapped: String,
    pub signature: Option<String>,
    pub signer_endpoint_id: Option<String>,
}

/// Why a wrap (or a vouch) is not kept: the fixture's `refusals`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EnetWrapRefusal {
    Malformed,
    Unsigned,
    Unpinned,
    Unopened,
    Signature,
}

#[derive(Clone, PartialEq, Eq, uniffi::Enum)]
pub enum EnetWrapVerdict {
    Verified { secret: Vec<u8>, tree_public_key: String, signer_endpoint_id: String },
    /// Kept without a signature (only where none is required).
    Unsigned { secret: Vec<u8>, tree_public_key: String },
    Refused { reason: EnetWrapRefusal },
}

impl std::fmt::Debug for EnetWrapVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnetWrapVerdict::Verified { tree_public_key, signer_endpoint_id, .. } => {
                write!(f, "Verified(public={tree_public_key}, signer={signer_endpoint_id})")
            }
            EnetWrapVerdict::Unsigned { tree_public_key, .. } => write!(f, "Unsigned(public={tree_public_key})"),
            EnetWrapVerdict::Refused { reason } => write!(f, "Refused({reason:?})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EnetVouchVerdict {
    Verified,
    /// Never `Unopened`: a vouch opens nothing.
    Refused { reason: EnetWrapRefusal },
}

#[cfg(feature = "node")]
mod imp {
    use enet_core::wrap_sig as core;
    use enet_core::Identity;

    use super::*;
    use crate::{bytes32, invalid};

    fn statement(s: &EnetWrapStatement) -> core::WrapStatement<'_> {
        core::WrapStatement {
            root_id: &s.root_id,
            epoch: s.epoch,
            tree_public_key: &s.tree_public_key,
            recipient_endpoint_id: &s.recipient_endpoint_id,
            wrapped: &s.wrapped,
        }
    }

    fn refusal(r: core::WrapRefusal) -> EnetWrapRefusal {
        match r {
            core::WrapRefusal::Malformed => EnetWrapRefusal::Malformed,
            core::WrapRefusal::Unsigned => EnetWrapRefusal::Unsigned,
            core::WrapRefusal::Unpinned => EnetWrapRefusal::Unpinned,
            core::WrapRefusal::Unopened => EnetWrapRefusal::Unopened,
            core::WrapRefusal::Signature => EnetWrapRefusal::Signature,
        }
    }

    pub fn message(s: &EnetWrapStatement) -> Result<Vec<u8>, EnetError> {
        core::wrap_signature_message(&statement(s)).map(|m| m.to_vec()).ok_or_else(|| invalid("malformed wrap statement"))
    }

    pub fn sign(identity: &[u8], s: &EnetWrapStatement) -> Result<String, EnetError> {
        Ok(core::sign_wrap(&Identity::from_slice(identity)?, &statement(s))?)
    }

    pub fn verify(s: &EnetWrapStatement, signature: &str, signer: &str) -> Result<bool, EnetError> {
        Ok(core::verify_wrap_signature(&statement(s), signature, signer))
    }

    pub fn accept(
        wrap: &EnetServedWrap,
        identity: &[u8],
        install_secret: &[u8],
        pins: &[String],
        required: bool,
    ) -> Result<EnetWrapVerdict, EnetError> {
        let identity = Identity::from_slice(identity)?;
        let install_secret = bytes32(install_secret, "install secret")?;
        let served = core::ServedWrap {
            root_id: &wrap.root_id,
            epoch: wrap.epoch,
            wrapped: &wrap.wrapped,
            signature: wrap.signature.as_deref(),
            signer_endpoint_id: wrap.signer_endpoint_id.as_deref(),
        };
        let pinned = |id: &str| pins.iter().any(|pin| pin == id);
        Ok(match core::accept_wrap(&served, &identity, &install_secret, &pinned, required) {
            core::WrapVerdict::Verified { secret, tree_public_key, signer_endpoint_id } => {
                EnetWrapVerdict::Verified { secret: secret.to_vec(), tree_public_key, signer_endpoint_id }
            }
            core::WrapVerdict::Unsigned { secret, tree_public_key } => {
                EnetWrapVerdict::Unsigned { secret: secret.to_vec(), tree_public_key }
            }
            core::WrapVerdict::Refused(reason) => EnetWrapVerdict::Refused { reason: refusal(reason) },
        })
    }

    pub fn vouch(
        s: &EnetWrapStatement,
        signature: Option<&str>,
        signer: Option<&str>,
        pins: &[String],
    ) -> Result<EnetVouchVerdict, EnetError> {
        let pinned = |id: &str| pins.iter().any(|pin| pin == id);
        Ok(match core::accept_vouch(&statement(s), signature, signer, &pinned) {
            Ok(()) => EnetVouchVerdict::Verified,
            Err(reason) => EnetVouchVerdict::Refused { reason: refusal(reason) },
        })
    }
}

#[cfg(not(feature = "node"))]
mod imp {
    use super::*;

    fn unavailable<T>() -> Result<T, EnetError> {
        Err(EnetError::Crypto { reason: "wrap signature: unavailable".into() })
    }

    pub fn message(_: &EnetWrapStatement) -> Result<Vec<u8>, EnetError> {
        unavailable()
    }

    pub fn sign(_: &[u8], _: &EnetWrapStatement) -> Result<String, EnetError> {
        unavailable()
    }

    pub fn verify(_: &EnetWrapStatement, _: &str, _: &str) -> Result<bool, EnetError> {
        unavailable()
    }

    pub fn accept(_: &EnetServedWrap, _: &[u8], _: &[u8], _: &[String], _: bool) -> Result<EnetWrapVerdict, EnetError> {
        unavailable()
    }

    pub fn vouch(_: &EnetWrapStatement, _: Option<&str>, _: Option<&str>, _: &[String]) -> Result<EnetVouchVerdict, EnetError> {
        unavailable()
    }
}

/// The 186 bytes a wrap's signer signs (`Invalid` when a field is malformed).
#[uniffi::export]
pub fn wrap_signature_message(statement: EnetWrapStatement) -> Result<Vec<u8>, EnetError> {
    imp::message(&statement)
}

/// `session_keys.signature`: the identity's (32 bytes) signature over the
/// statement, base64url (86 chars). `Invalid` on a malformed statement.
#[uniffi::export]
pub fn sign_wrap(identity: Vec<u8>, statement: EnetWrapStatement) -> Result<String, EnetError> {
    imp::sign(&identity, &statement)
}

/// Whether `signature` is `signer_endpoint_id`'s over the statement
/// (strict ed25519). Says nothing about the signer being trusted:
/// `acceptWrap` and `acceptVouch` ask the pins.
#[uniffi::export]
pub fn verify_wrap_signature(statement: EnetWrapStatement, signature: String, signer_endpoint_id: String) -> Result<bool, EnetError> {
    imp::verify(&statement, &signature, &signer_endpoint_id)
}

/// Whether this install keeps a served wrap: it builds the signed message
/// ITSELF (the root and epoch it asked for, the public key of the secret it
/// unwrapped, its own endpoint id from `identity`, the bytes). `pins` = the
/// endpoint ids it has pinned. A wrap that carries a signature must pass with
/// it, also where none is `required`. `Invalid` only for an identity or an
/// install secret that is not 32 bytes.
#[uniffi::export]
pub fn accept_wrap(
    wrap: EnetServedWrap,
    identity: Vec<u8>,
    install_secret: Vec<u8>,
    pins: Vec<String>,
    required: bool,
) -> Result<EnetWrapVerdict, EnetError> {
    imp::accept(&wrap, &identity, &install_secret, &pins, required)
}

/// Whether an install that holds NO secret of (root, epoch) may seal to
/// `statement.tree_public_key`: a pinned install's signed wrap of that epoch,
/// made for ANOTHER install, names that key.
#[uniffi::export]
pub fn accept_vouch(
    statement: EnetWrapStatement,
    signature: Option<String>,
    signer_endpoint_id: Option<String>,
    pins: Vec<String>,
) -> Result<EnetVouchVerdict, EnetError> {
    imp::vouch(&statement, signature.as_deref(), signer_endpoint_id.as_deref(), &pins)
}
