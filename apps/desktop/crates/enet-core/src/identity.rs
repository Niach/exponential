use iroh::{EndpointId, SecretKey};

use crate::error::{NetError, Result};

/// A node's long-term identity: an ed25519 secret key. Its public half is
/// the [`EndpointId`] peers dial and allowlist. Persist [`Identity::to_bytes`]
/// (32 bytes) in the platform keychain; never send it anywhere.
#[derive(Clone)]
pub struct Identity {
    secret: SecretKey,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Identity({})", self.endpoint_id().fmt_short())
    }
}

impl Identity {
    /// A fresh random identity.
    pub fn generate() -> Self {
        Self::from_bytes(crate::util::random_bytes::<32>())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { secret: SecretKey::from_bytes(&bytes) }
    }

    /// Loads 32 secret-key bytes (anything else is refused).
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let b: [u8; 32] = bytes
            .try_into()
            .map_err(|_| NetError::Invalid(format!("identity must be 32 bytes, got {}", bytes.len())))?;
        Ok(Self::from_bytes(b))
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.secret.public()
    }

    pub fn secret_key(&self) -> &SecretKey {
        &self.secret
    }

    /// An ed25519 signature over `msg` (e.g. the install registration string).
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.secret.sign(msg).to_bytes()
    }
}

/// Verifies an ed25519 signature by `author` over `msg`.
pub(crate) fn verify(author: &EndpointId, msg: &[u8], sig: &[u8; 64]) -> bool {
    author.verify(msg, &iroh::Signature::from_bytes(sig)).is_ok()
}
