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
//!   `pushOpen` for notification extensions (no node, no runtime needed);
//!   sealed session content (EXP-1458): `generateTreeKey`, `wrapTreeKey`,
//!   `unwrapTreeKey`, `sealBody`, `openBody`, `sealCard`, `openCard`.
//! - Feature `node` (default) = the node (src/node.rs). Without it
//!   (`--no-default-features`, the PUSH-ONLY build) the facade is the push
//!   and sealed-content functions alone: no iroh, no tokio, small enough for a Notification
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

// --- sealed session content (EXP-1458) ----------------------------------
// fixtures/sealed-content.json is the spec; Swift/Kotlin replay it through
// these. Cards cross as JSON strings, public keys and envelopes as the
// base64url strings the server stores, secrets as raw bytes.

/// A session tree's keypair: `public_key` = `sessions.seal_public_key`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetTreeKey {
    pub secret: Vec<u8>,
    pub public_key: String,
}

/// One tree secret this install holds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetEpochKey {
    pub epoch: u32,
    pub secret: Vec<u8>,
}

/// What opening sealed content gave (fixture spelling: plain, opened,
/// noKey, unsupported, malformed, refused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EnetSealStatus {
    Plain,
    Opened,
    NoKey,
    Unsupported,
    Malformed,
    Refused,
}

impl From<net::sealed::OpenStatus> for EnetSealStatus {
    fn from(s: net::sealed::OpenStatus) -> Self {
        use net::sealed::OpenStatus as S;
        match s {
            S::Plain => Self::Plain,
            S::Opened => Self::Opened,
            S::NoKey => Self::NoKey,
            S::Unsupported => Self::Unsupported,
            S::Malformed => Self::Malformed,
            S::Refused => Self::Refused,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetOpenedBody {
    pub status: EnetSealStatus,
    pub epoch: Option<u32>,
    /// Set for plain and opened only.
    pub text: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetOpenedCard {
    pub status: EnetSealStatus,
    pub epoch: Option<u32>,
    /// JSON: the full card (plain / opened) or the clear part alone.
    pub card_json: String,
}

fn epoch_keys(keys: Vec<EnetEpochKey>) -> Result<impl Fn(u32) -> Option<[u8; 32]>, EnetError> {
    let keys = keys
        .into_iter()
        .map(|k| Ok((k.epoch, bytes32(&k.secret, "tree secret")?)))
        .collect::<Result<Vec<_>, EnetError>>()?;
    Ok(move |epoch| keys.iter().find(|(e, _)| *e == epoch).map(|(_, s)| *s))
}

fn tree_public(public_key: &str) -> Result<[u8; 32], EnetError> {
    Ok(net::sealed::public_key_from_b64(public_key)?)
}

#[uniffi::export]
pub fn generate_tree_key() -> EnetTreeKey {
    let key = net::sealed::generate_tree_key();
    EnetTreeKey { secret: key.secret.to_vec(), public_key: key.public_key() }
}

/// `session_keys.wrapped` for a reader install (`install_public` = its
/// base64url `installs.x25519_public`).
#[uniffi::export]
pub fn wrap_tree_key(install_public: String, tree_secret: Vec<u8>) -> Result<String, EnetError> {
    Ok(net::sealed::wrap_tree_key(&tree_public(&install_public)?, &bytes32(&tree_secret, "tree secret")?)?)
}

#[uniffi::export]
pub fn unwrap_tree_key(install_secret: Vec<u8>, wrapped: String) -> Result<Vec<u8>, EnetError> {
    Ok(net::sealed::unwrap_tree_key(&bytes32(&install_secret, "install secret")?, &wrapped)?.to_vec())
}

/// A body (or sealed title) envelope: `exp-sealed:v1:<epoch>:<b64url>`.
#[uniffi::export]
pub fn seal_body(tree_public_key: String, epoch: u32, text: String) -> Result<String, EnetError> {
    Ok(net::sealed::seal_body(&tree_public(&tree_public_key)?, epoch, &text)?)
}

#[uniffi::export]
pub fn open_body(body: String, keys: Vec<EnetEpochKey>) -> Result<EnetOpenedBody, EnetError> {
    let opened = net::sealed::open_body(&body, &epoch_keys(keys)?);
    Ok(EnetOpenedBody { status: opened.status.into(), epoch: opened.epoch, text: opened.text })
}

/// A card envelope (JSON in, JSON out): `{...clear, "sealed": "v1:…"}`.
#[uniffi::export]
pub fn seal_card(tree_public_key: String, epoch: u32, card_json: String) -> Result<String, EnetError> {
    let card: serde_json::Value = serde_json::from_str(&card_json).map_err(|e| invalid(format!("card JSON: {e}")))?;
    Ok(net::sealed::seal_card(&tree_public(&tree_public_key)?, epoch, &card)?.to_string())
}

#[uniffi::export]
pub fn open_card(card_json: String, keys: Vec<EnetEpochKey>) -> Result<EnetOpenedCard, EnetError> {
    let card: serde_json::Value = serde_json::from_str(&card_json).map_err(|e| invalid(format!("card JSON: {e}")))?;
    let opened = net::sealed::open_card(&card, &epoch_keys(keys)?);
    Ok(EnetOpenedCard { status: opened.status.into(), epoch: opened.epoch, card_json: opened.card.to_string() })
}
