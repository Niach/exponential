use iroh::EndpointId;

use crate::{call::CallId, chat::ChatMessage, records::Record};

/// Why a call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallEndReason {
    /// We hung up.
    Local,
    /// The peer hung up or declined.
    Remote,
    /// The connection failed.
    Failed(String),
}

/// Everything a node reports, in arrival order, on [`crate::Node::events`].
#[derive(Debug, Clone)]
pub enum Event {
    /// A chat message from `from` (already acknowledged to the sender).
    Chat { from: EndpointId, message: ChatMessage },
    /// A record from a peer won its key (gossip or sync); local puts do not
    /// raise this.
    Record { record: Record },
    /// A peer is calling; answer with `accept_call` or `hang_up`.
    CallIncoming { call: CallId, from: EndpointId },
    /// The callee accepted our call; frames may flow.
    CallAccepted { call: CallId },
    /// One audio frame, as the sender passed it to `send_frame`.
    CallFrame { call: CallId, seq: u32, data: Vec<u8> },
    CallEnded { call: CallId, reason: CallEndReason },
    /// An incoming connection was refused by the access policy.
    PeerRefused { peer: EndpointId, alpn: Vec<u8> },
}
