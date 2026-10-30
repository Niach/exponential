//! eNet: Exponential's shared p2p + end-to-end encrypted networking core,
//! built on iroh 1.x. Consumed by Exponential and spezl (natively, through
//! `exponential-net-ffi` on iOS/Android, and in the browser through
//! `exponential-net-wasm`).
//!
//! One [`Node`] = one iroh endpoint (identity = an ed25519 [`Identity`]) and
//! a router with one versioned ALPN per channel:
//! - chat ([`CHAT_ALPN`]): reliable, ordered, acknowledged messages;
//! - files (`iroh_blobs::ALPN`): content-addressed blobs (BLAKE3);
//! - records ([`GOSSIP_ALPN`] + [`RECORDS_SYNC_ALPN`]): signed last-writer-wins
//!   records per 32-byte space over iroh-gossip, with a pull sync on join;
//! - call ([`CALL_ALPN`]): opaque audio frames over QUIC datagrams.
//!
//! Plus [`push`]: sealed push payloads (X25519 + HKDF-SHA256 +
//! ChaCha20-Poly1305) for the push relay's `/push` API, and [`sealed`]:
//! session content sealed to a tree key over the same primitive (EXP-1458).
//!
//! Every connection is QUIC/TLS 1.3 authenticated by the peers' ed25519 keys;
//! a relay only ever forwards ciphertext between endpoint ids. Incoming
//! connections are gated by the node's [`Access`] policy.
//!
//! Feature `node` (default) = everything above. Without it
//! (`--no-default-features`) the crate is [`push`] + [`sealed`] alone on RustCrypto (no
//! iroh, no tokio): what a Notification Service Extension links.

#[cfg(feature = "node")]
mod access;
#[cfg(feature = "node")]
mod blobs;
#[cfg(feature = "node")]
mod call;
#[cfg(feature = "node")]
mod chat;
#[cfg(feature = "node")]
mod config;
mod error;
#[cfg(feature = "node")]
mod event;
#[cfg(feature = "node")]
mod identity;
#[cfg(feature = "node")]
mod node;
pub mod push;
pub mod sealed;
#[cfg(feature = "node")]
mod records;
#[cfg(feature = "node")]
mod ticket;
#[cfg(feature = "node")]
mod util;

#[cfg(feature = "node")]
pub use access::Access;
#[cfg(feature = "node")]
pub use blobs::BlobRef;
#[cfg(feature = "node")]
pub use call::{CallId, CALL_ALPN};
#[cfg(feature = "node")]
pub use chat::{ChatMessage, CHAT_ALPN, MAX_CHAT_MESSAGE};
#[cfg(feature = "node")]
pub use config::{NetConfig, RelayChoice, DEFAULT_RELAY_URL};
pub use error::{NetError, Result};
#[cfg(feature = "node")]
pub use event::{CallEndReason, Event};
#[cfg(feature = "node")]
pub use identity::Identity;
#[cfg(feature = "node")]
pub use node::Node;
pub use push::{push_open, push_seal, PushKeypair};
#[cfg(feature = "node")]
pub use records::{Clock, Record, SpaceId, GOSSIP_ALPN, MAX_RECORD_KEY, MAX_RECORD_VALUE, RECORDS_SYNC_ALPN};
#[cfg(feature = "node")]
pub use ticket::{Ticket, TICKET_PREFIX};

/// Re-exported iroh types that appear in this crate's API.
#[cfg(feature = "node")]
pub use iroh::{EndpointAddr, EndpointId, RelayUrl, SecretKey};
