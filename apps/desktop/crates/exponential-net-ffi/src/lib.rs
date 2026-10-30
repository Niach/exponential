//! The UniFFI facade over `exponential-net` (eNet) for Swift (iOS, macOS)
//! and Kotlin (Android).
//!
//! Shape of the contract:
//! - `EnetNode.start(config, identity)` (async) builds a node on the
//!   facade's own tokio runtime (one per process, created on first use);
//!   every async method runs on that runtime and is awaited from the host's
//!   executor (Swift concurrency / Kotlin coroutines), so the host needs no
//!   Rust runtime knowledge.
//! - Events (chat, records, calls, refused peers) arrive EITHER through a
//!   foreign `EnetListener` (`setListener`) OR by awaiting `nextEvent()` in a
//!   loop: pick one per node (the listener drains the queue).
//! - Ids cross as strings (endpoint ids = z-base-32 / hex as iroh prints
//!   them, tickets = `enet1…`), bytes as `Data` / `ByteArray`.
//! - Free functions: identity + push-key helpers, `pushSeal`, and
//!   `pushOpen` for notification extensions (no node, no runtime needed).
//! - Feature `node` (default) = the node (src/node.rs). Without it
//!   (`--no-default-features`, the PUSH-ONLY build) the facade is the push
//!   functions alone: no iroh, no tokio, small enough for a Notification
//!   Service Extension's memory cap (bindings under `bindings/push/`).
//! - A Rust panic never crosses the FFI: async work runs in spawned tasks
//!   (a panic = `EnetError.Panic`), and the sync methods are trivial.

use enet_core as net;
use net::NetError;

uniffi::setup_scaffolding!();

#[cfg(feature = "node")]
mod node;
#[cfg(feature = "node")]
pub use node::*;

// --- errors ------------------------------------------------------------

#[derive(Debug, uniffi::Error)]
pub enum EnetError {
    /// Bad input from the host (a malformed id, ticket, key, size).
    Invalid { reason: String },
    /// Binding the endpoint / opening the store failed.
    Bind { reason: String },
    /// Could not reach the peer (offline, no address, timeout).
    Connect { reason: String },
    /// The peer's access policy refused this node.
    Refused { reason: String },
    /// A record or payload failed authentication.
    Crypto { reason: String },
    NotFound { reason: String },
    /// Any other transport failure.
    Transport { reason: String },
    /// The node is shut down.
    Closed { reason: String },
    /// eNet panicked inside this call; the node stays usable.
    Panic { reason: String },
}

impl std::error::Error for EnetError {}

impl std::fmt::Display for EnetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, reason) = match self {
            EnetError::Invalid { reason } => ("invalid", reason),
            EnetError::Bind { reason } => ("bind", reason),
            EnetError::Connect { reason } => ("connect", reason),
            EnetError::Refused { reason } => ("refused", reason),
            EnetError::Crypto { reason } => ("crypto", reason),
            EnetError::NotFound { reason } => ("not found", reason),
            EnetError::Transport { reason } => ("transport", reason),
            EnetError::Closed { reason } => ("closed", reason),
            EnetError::Panic { reason } => ("panicked", reason),
        };
        write!(f, "{kind}: {reason}")
    }
}

impl From<NetError> for EnetError {
    fn from(e: NetError) -> Self {
        let reason = e.to_string();
        match e {
            NetError::Invalid(_) | NetError::Ticket(_) => EnetError::Invalid { reason },
            NetError::Bind(_) => EnetError::Bind { reason },
            NetError::Connect { .. } | NetError::Timeout(_) => EnetError::Connect { reason },
            NetError::Refused(_) => EnetError::Refused { reason },
            NetError::BadSignature | NetError::Crypto(_) => EnetError::Crypto { reason },
            NetError::NotFound(_) => EnetError::NotFound { reason },
            NetError::Transport(_) => EnetError::Transport { reason },
            NetError::Closed => EnetError::Closed { reason },
        }
    }
}

pub(crate) fn invalid(reason: impl Into<String>) -> EnetError {
    EnetError::Invalid { reason: reason.into() }
}

pub(crate) fn bytes32(b: &[u8], what: &str) -> Result<[u8; 32], EnetError> {
    b.try_into().map_err(|_| invalid(format!("{what} must be 32 bytes, got {}", b.len())))
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetPushKeypair {
    /// X25519 secret: give ONLY this to the notification extension.
    pub secret: Vec<u8>,
    /// X25519 public: register it with the push relay.
    pub public: Vec<u8>,
}

impl From<net::PushKeypair> for EnetPushKeypair {
    fn from(k: net::PushKeypair) -> Self {
        Self { secret: k.secret().to_vec(), public: k.public().to_vec() }
    }
}

// --- free functions ----------------------------------------------------

/// The install's push keypair derived from its 32 identity bytes.
#[uniffi::export]
pub fn push_keypair_derive(identity: Vec<u8>) -> Result<EnetPushKeypair, EnetError> {
    Ok(net::PushKeypair::derive_from_identity_bytes(&bytes32(&identity, "identity")?).into())
}

#[uniffi::export]
pub fn push_keypair_generate() -> EnetPushKeypair {
    net::PushKeypair::generate().into()
}

/// Seals a push payload to a recipient's X25519 public key.
#[uniffi::export]
pub fn push_seal(recipient_public: Vec<u8>, plaintext: Vec<u8>) -> Result<Vec<u8>, EnetError> {
    Ok(net::push_seal(&bytes32(&recipient_public, "recipient public key")?, &plaintext)?)
}

/// Opens a sealed push payload: what a Notification Service Extension
/// (iOS) or the FCM service (Android) calls. Needs no node and no runtime.
#[uniffi::export]
pub fn push_open(recipient_secret: Vec<u8>, sealed: Vec<u8>) -> Result<Vec<u8>, EnetError> {
    Ok(net::push_open(&bytes32(&recipient_secret, "recipient secret key")?, &sealed)?)
}
