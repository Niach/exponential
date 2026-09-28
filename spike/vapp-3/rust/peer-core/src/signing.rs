//! Signed DTLS fingerprints (VAPP-1 D4a): each side signs the fingerprint of its own DTLS
//! certificate with its Ed25519 identity key; the peer verifies BEFORE applying the SDP and
//! again against the fingerprint DTLS actually negotiated. A tampering signaling server is caught.
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

pub const DOMAIN: &str = "exp-dtls-fp-v1";

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

pub fn encode(bytes: &[u8]) -> String {
    b64().encode(bytes)
}
pub fn decode(s: &str) -> Option<Vec<u8>> {
    b64().decode(s).ok()
}

/// The bytes that get signed. Binds the DTLS fingerprint to the ICE ufrag (so a swapped ICE
/// session is caught too), the room and the sending peer id (replay across rooms/peers).
pub fn signed_bytes(hash_func: &str, fingerprint_hex: &str, ufrag: &str, session_id: &str, peer_id: &str) -> Vec<u8> {
    format!(
        "{DOMAIN}\n{}\n{}\n{ufrag}\n{session_id}\n{peer_id}",
        hash_func.to_ascii_lowercase(),
        fingerprint_hex.to_ascii_uppercase()
    )
    .into_bytes()
}

pub struct Identity {
    key: SigningKey,
}

impl Identity {
    pub fn generate() -> Self {
        let mut rng = rand::rngs::OsRng;
        Self { key: SigningKey::generate(&mut rng) }
    }
    pub fn from_seed(seed: &[u8]) -> Option<Self> {
        let seed: [u8; 32] = seed.try_into().ok()?;
        Some(Self { key: SigningKey::from_bytes(&seed) })
    }
    pub fn seed(&self) -> [u8; 32] {
        self.key.to_bytes()
    }
    /// base64url public key.
    pub fn public_key(&self) -> String {
        encode(self.key.verifying_key().as_bytes())
    }
    pub fn sign(&self, msg: &[u8]) -> String {
        encode(&self.key.sign(msg).to_bytes())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    BadPublicKey,
    BadSignature,
    Mismatch,
}

pub fn verify(pubkey_b64: &str, sig_b64: &str, msg: &[u8]) -> Result<(), VerifyError> {
    let pk = decode(pubkey_b64).ok_or(VerifyError::BadPublicKey)?;
    let pk: [u8; 32] = pk.try_into().map_err(|_| VerifyError::BadPublicKey)?;
    let vk = VerifyingKey::from_bytes(&pk).map_err(|_| VerifyError::BadPublicKey)?;
    let sig = decode(sig_b64).ok_or(VerifyError::BadSignature)?;
    let sig = Signature::from_slice(&sig).map_err(|_| VerifyError::BadSignature)?;
    vk.verify(msg, &sig).map_err(|_| VerifyError::Mismatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sign_verify_and_tamper() {
        let id = Identity::generate();
        let msg = signed_bytes("sha-256", "AB:CD", "ufrag", "room", "p1");
        let sig = id.sign(&msg);
        assert_eq!(verify(&id.public_key(), &sig, &msg), Ok(()));
        let tampered = signed_bytes("sha-256", "AB:CE", "ufrag", "room", "p1");
        assert_eq!(verify(&id.public_key(), &sig, &tampered), Err(VerifyError::Mismatch));
        let other = Identity::generate();
        assert_eq!(verify(&other.public_key(), &sig, &msg), Err(VerifyError::Mismatch));
    }
}
