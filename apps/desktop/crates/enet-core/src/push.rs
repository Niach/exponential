//! Sealed push payloads for the push relay's `/push` API.
//!
//! The sender seals a small payload to the recipient install's X25519 public
//! key; the relay forwards opaque bytes to APNs/FCM; the recipient's
//! notification extension opens it with its X25519 secret. Format (v1):
//!
//! ```text
//! sealed = 0x01 || eph_pub[32] || ciphertext || tag[16]
//! shared = X25519(eph_secret, recipient_pub)          (all-zero = refused)
//! okm    = HKDF-SHA256(salt = eph_pub || recipient_pub,
//!                      ikm  = shared,
//!                      info = "exp/enet/push/1", L = 44)
//! key    = okm[0..32], nonce = okm[32..44]
//! ciphertext || tag = ChaCha20-Poly1305(key, nonce, aad = 0x01, plaintext)
//! ```
//!
//! Every message has a fresh ephemeral key, so the derived nonce never repeats
//! under one key. The install's keypair is either random
//! ([`PushKeypair::generate`]) or derived from its node identity
//! ([`PushKeypair::derive`], HKDF-SHA256 with info `exp/enet/push-x25519/1`),
//! so only the 32-byte X25519 secret needs to be shared with the extension.

use chacha20poly1305::{aead::Aead, aead::Payload, ChaCha20Poly1305, KeyInit};
use curve25519_dalek::montgomery::MontgomeryPoint;
use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::{NetError, Result};
#[cfg(feature = "node")]
use crate::identity::Identity;

/// The current format's version byte.
pub const PUSH_VERSION: u8 = 1;
/// Bytes added to the plaintext by [`push_seal`] (version + ephemeral key + tag).
pub const PUSH_OVERHEAD: usize = 1 + 32 + 16;
const INFO: &[u8] = b"exp/enet/push/1";
const DERIVE_INFO: &[u8] = b"exp/enet/push-x25519/1";

/// An install's X25519 keypair for sealed pushes.
#[derive(Clone)]
pub struct PushKeypair {
    secret: [u8; 32],
}

impl std::fmt::Debug for PushKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PushKeypair(public={})", data_encoding::HEXLOWER.encode(&self.public()))
    }
}

impl PushKeypair {
    pub fn generate() -> Self {
        Self { secret: random_bytes() }
    }
    /// Deterministic from the node identity (same identity = same keypair).
    #[cfg(feature = "node")]
    pub fn derive(identity: &Identity) -> Self {
        Self::derive_from_identity_bytes(&identity.to_bytes())
    }
    /// [`PushKeypair::derive`] from the identity's 32 secret bytes (no iroh
    /// needed, so push-only builds have it too).
    pub fn derive_from_identity_bytes(identity: &[u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::new(None, identity);
        let mut secret = [0u8; 32];
        hk.expand(DERIVE_INFO, &mut secret).expect("32 bytes is a valid HKDF length");
        Self { secret }
    }
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self { secret }
    }
    pub fn secret(&self) -> [u8; 32] {
        self.secret
    }
    pub fn public(&self) -> [u8; 32] {
        MontgomeryPoint::mul_base_clamped(self.secret).to_bytes()
    }
}

fn derive_key(shared: &MontgomeryPoint, eph_pub: &[u8; 32], recipient_pub: &[u8; 32]) -> Result<ChaChaKey> {
    if shared.to_bytes() == [0u8; 32] {
        return Err(NetError::Crypto("low-order public key".into()));
    }
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(eph_pub);
    salt[32..].copy_from_slice(recipient_pub);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared.as_bytes());
    let mut okm = [0u8; 44];
    hk.expand(INFO, &mut okm).expect("44 bytes is a valid HKDF length");
    Ok(ChaChaKey { key: okm[..32].try_into().unwrap(), nonce: okm[32..].try_into().unwrap() })
}

fn random_bytes() -> [u8; 32] {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("system randomness");
    b
}

struct ChaChaKey {
    key: [u8; 32],
    nonce: [u8; 12],
}

/// Seals `plaintext` to `recipient_public` (X25519). See the module docs for
/// the exact byte format.
pub fn push_seal(recipient_public: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    push_seal_with(random_bytes(), recipient_public, plaintext)
}

/// [`push_seal`] with a caller-chosen ephemeral secret: test vectors only
/// (a reused ephemeral key reuses the nonce).
pub(crate) fn push_seal_with(eph: [u8; 32], recipient_public: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let eph_pub = MontgomeryPoint::mul_base_clamped(eph).to_bytes();
    let shared = MontgomeryPoint(*recipient_public).mul_clamped(eph);
    let k = derive_key(&shared, &eph_pub, recipient_public)?;
    let ct = ChaCha20Poly1305::new(&k.key.into())
        .encrypt(&k.nonce.into(), Payload { msg: plaintext, aad: &[PUSH_VERSION] })
        .map_err(|_| NetError::Crypto("seal failed".into()))?;
    let mut out = Vec::with_capacity(PUSH_OVERHEAD + plaintext.len());
    out.push(PUSH_VERSION);
    out.extend_from_slice(&eph_pub);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Opens a [`push_seal`]ed payload with the recipient's X25519 secret.
/// Fails on an unknown version, truncation or any tampering.
pub fn push_open(recipient_secret: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < PUSH_OVERHEAD {
        return Err(NetError::Crypto("sealed payload too short".into()));
    }
    if sealed[0] != PUSH_VERSION {
        return Err(NetError::Crypto(format!("unknown push format version {}", sealed[0])));
    }
    let eph_pub: [u8; 32] = sealed[1..33].try_into().unwrap();
    let recipient_pub = MontgomeryPoint::mul_base_clamped(*recipient_secret).to_bytes();
    let shared = MontgomeryPoint(eph_pub).mul_clamped(*recipient_secret);
    let k = derive_key(&shared, &eph_pub, &recipient_pub)?;
    ChaCha20Poly1305::new(&k.key.into())
        .decrypt(&k.nonce.into(), Payload { msg: &sealed[33..], aad: &[PUSH_VERSION] })
        .map_err(|_| NetError::Crypto("sealed payload failed authentication".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip_and_tamper() {
        let kp = PushKeypair::generate();
        let sealed = push_seal(&kp.public(), b"hello push").unwrap();
        assert_eq!(sealed.len(), PUSH_OVERHEAD + 10);
        assert_eq!(sealed[0], 1);
        assert_eq!(push_open(&kp.secret(), &sealed).unwrap(), b"hello push");
        // Every byte matters: version, ephemeral key, ciphertext, tag.
        for i in 0..sealed.len() {
            let mut t = sealed.clone();
            t[i] ^= 0x01;
            assert!(push_open(&kp.secret(), &t).is_err(), "byte {i} tampered but opened");
        }
        assert!(push_open(&kp.secret(), &sealed[..sealed.len() - 1]).is_err());
        let other = PushKeypair::generate();
        assert!(push_open(&other.secret(), &sealed).is_err());
        // Two seals of the same plaintext differ (fresh ephemeral key).
        assert_ne!(push_seal(&kp.public(), b"x").unwrap(), push_seal(&kp.public(), b"x").unwrap());
    }

    #[cfg(feature = "node")]
    #[test]
    fn derive_is_deterministic() {
        let id = Identity::generate();
        assert_eq!(PushKeypair::derive(&id).public(), PushKeypair::derive(&id).public());
        assert_ne!(PushKeypair::derive(&id).public(), PushKeypair::derive(&Identity::generate()).public());
    }

    #[test]
    fn low_order_key_refused() {
        assert!(push_seal(&[0u8; 32], b"x").is_err());
    }

    /// The shared vectors every implementation replays (web, Swift, Kotlin).
    #[test]
    fn replays_the_push_seal_fixture() {
        let fx: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/push-seal.json")).unwrap();
        let hex = |v: &serde_json::Value| data_encoding::HEXLOWER.decode(v.as_str().unwrap().as_bytes()).unwrap();
        let key = |v: &serde_json::Value| -> [u8; 32] { hex(v).try_into().unwrap() };
        let secret = key(&fx["recipient"]["secret"]);
        assert_eq!(PushKeypair::from_secret(secret).public(), key(&fx["recipient"]["public"]));

        let v = &fx["vector"];
        let plaintext = v["plaintext"].as_str().unwrap().as_bytes();
        let sealed = push_seal_with(key(&v["ephemeralSecret"]), &key(&fx["recipient"]["public"]), plaintext).unwrap();
        assert_eq!(sealed, hex(&v["sealed"]));
        let opened = push_open(&secret, &sealed).unwrap();
        assert_eq!(opened, plaintext);
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&opened).unwrap(), v["opened"]);

        let r = &fx["rustSealed"];
        assert_eq!(push_open(&secret, &hex(&r["sealed"])).unwrap(), r["plaintext"].as_str().unwrap().as_bytes());

        let d = &fx["derive"];
        let kp = PushKeypair::derive_from_identity_bytes(&key(&d["identity"]));
        assert_eq!(kp.secret(), key(&d["secret"]));
        assert_eq!(kp.public(), key(&d["public"]));

        for case in fx["refused"].as_array().unwrap() {
            assert!(push_open(&key(&case["secret"]), &hex(&case["sealed"])).is_err(), "{} opened", case["name"]);
        }
        assert!(push_seal(&key(&fx["lowOrderPublic"]), b"x").is_err());
    }
}
