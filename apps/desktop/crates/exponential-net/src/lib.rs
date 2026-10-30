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
//! ChaCha20-Poly1305) for the push relay's `/push` API.
//!
//! Every connection is QUIC/TLS 1.3 authenticated by the peers' ed25519 keys;
//! a relay only ever forwards ciphertext between endpoint ids. Incoming
//! connections are gated by the node's [`Access`] policy.

mod access;
mod blobs;
mod call;
mod chat;
mod config;
mod error;
mod event;
mod identity;
mod node;
pub mod push;
mod records;
mod ticket;
mod util;

pub use access::Access;
pub use blobs::BlobRef;
pub use call::{CallId, CALL_ALPN};
pub use chat::{ChatMessage, CHAT_ALPN, MAX_CHAT_MESSAGE};
pub use config::{NetConfig, RelayChoice, DEFAULT_RELAY_URL};
pub use error::{NetError, Result};
pub use event::{CallEndReason, Event};
pub use identity::Identity;
pub use node::Node;
pub use push::{push_open, push_seal, PushKeypair};
pub use records::{Clock, Record, SpaceId, GOSSIP_ALPN, MAX_RECORD_KEY, MAX_RECORD_VALUE, RECORDS_SYNC_ALPN};
pub use ticket::{Ticket, TICKET_PREFIX};

/// Re-exported iroh types that appear in this crate's API.
pub use iroh::{EndpointAddr, EndpointId, RelayUrl, SecretKey};
