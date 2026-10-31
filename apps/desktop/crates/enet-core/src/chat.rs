//! Chat: reliable, ordered, acknowledged messages to one peer.
//!
//! Wire (`exp/enet/chat/1`): the sender keeps ONE bidirectional stream per
//! peer and writes frames `[u32 BE len][id:16][timestamp_ms: u64 BE][body]`;
//! the receiver answers every frame with its 16-byte id once the message is
//! queued as an [`crate::Event::Chat`]. One stream = in-order delivery; the
//! ack = the sender knows it arrived. Sends to one peer are serialized.
//!
//! EXP-1475: a peer that dials us afresh has lost its own link to us, most
//! likely because it restarted under the same identity (a reloaded browser
//! tab). Our cached link to it is then dead without a close, so it is closed
//! on that dial and the next send reconnects instead of waiting out the ack.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use iroh::{
    endpoint::{Connection, RecvStream, SendStream, WeakConnectionHandle},
    protocol::{AcceptError, ProtocolHandler},
    EndpointId,
};

use crate::{
    error::{NetError, Result},
    event::Event,
    node::{classify, refusal, Shared},
    util::{now_ms, random_bytes, read_frame, write_frame, Reader},
};

pub const CHAT_ALPN: &[u8] = b"exp/enet/chat/1";
/// The largest chat body (bigger payloads go through blobs).
pub const MAX_CHAT_MESSAGE: usize = 1024 * 1024;
const ACK_TIMEOUT_SECS: u64 = 20;

/// One chat message: opaque bytes (the app's encoding), a random id and the
/// sender's wall-clock timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: [u8; 16],
    pub timestamp_ms: u64,
    pub body: Vec<u8>,
}

impl ChatMessage {
    pub(crate) fn new(body: Vec<u8>) -> Self {
        Self { id: random_bytes::<16>(), timestamp_ms: now_ms(), body }
    }
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + self.body.len());
        out.extend_from_slice(&self.id);
        out.extend_from_slice(&self.timestamp_ms.to_be_bytes());
        out.extend_from_slice(&self.body);
        out
    }
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = Reader::new(buf);
        let id = r.array::<16>()?;
        let timestamp_ms = r.u64()?;
        Ok(Self { id, timestamp_ms, body: r.rest().to_vec() })
    }
}

struct Link {
    _conn: Connection,
    send: SendStream,
    recv: RecvStream,
}

type Slot = Arc<tokio::sync::Mutex<Option<Link>>>;

#[derive(Default)]
pub(crate) struct ChatState {
    links: Mutex<HashMap<EndpointId, Slot>>,
    /// The connection under each peer's cached link, reachable while a send
    /// holds the slot (the slot's lock can be held for a whole ack wait).
    dialed: Mutex<HashMap<EndpointId, WeakConnectionHandle>>,
}

/// The close code of a link dropped because the peer dialed us afresh.
const PEER_REDIALED_CODE: u32 = 0;

impl ChatState {
    fn slot(&self, peer: EndpointId) -> Slot {
        self.links.lock().expect("chat lock").entry(peer).or_default().clone()
    }

    /// The peer dialed us: close our link to it, a send waiting on it fails
    /// at once and reconnects.
    fn peer_redialed(&self, peer: &EndpointId) {
        let dialed = self.dialed.lock().expect("chat lock").remove(peer);
        if let Some(conn) = dialed.and_then(|weak| weak.upgrade()) {
            conn.close(PEER_REDIALED_CODE.into(), b"peer redialed");
        }
    }
}

pub(crate) async fn send(shared: &Shared, peer: EndpointId, body: Vec<u8>) -> Result<ChatMessage> {
    if body.len() > MAX_CHAT_MESSAGE {
        return Err(NetError::Invalid(format!("chat message over {MAX_CHAT_MESSAGE} bytes; use a blob")));
    }
    let msg = ChatMessage::new(body);
    let frame = msg.encode();
    let slot = shared.chat.slot(peer);
    let mut link = slot.lock().await;
    // A cached stream may be dead (peer restarted, network change): one retry
    // on a fresh connection.
    for attempt in 0..2 {
        // Out of the slot while in flight: a send cancelled mid-exchange (a
        // caller's timeout) drops the link instead of leaving it cached.
        let mut l = match link.take() {
            Some(l) => l,
            None => {
                let conn = shared.connect(peer, CHAT_ALPN).await?;
                let (send, recv) = match conn.open_bi().await {
                    Ok(s) => s,
                    Err(e) => return Err(classify(&conn, crate::error::transport(e))),
                };
                shared.chat.dialed.lock().expect("chat lock").insert(peer, conn.weak_handle());
                Link { _conn: conn, send, recv }
            }
        };
        match exchange(&mut l, &frame, &msg.id).await {
            Ok(()) => {
                *link = Some(l);
                return Ok(msg);
            }
            Err(e) => {
                if let Some(refused) = refusal(&l._conn) {
                    return Err(refused);
                }
                if attempt == 1 {
                    return Err(e);
                }
            }
        }
    }
    unreachable!()
}

async fn exchange(link: &mut Link, frame: &[u8], id: &[u8; 16]) -> Result<()> {
    write_frame(&mut link.send, frame).await?;
    let mut ack = [0u8; 16];
    n0_future::time::timeout(std::time::Duration::from_secs(ACK_TIMEOUT_SECS), link.recv.read_exact(&mut ack))
        .await
        .map_err(|_| NetError::Timeout("chat ack".into()))?
        .map_err(crate::error::transport)?;
    if &ack != id {
        return Err(NetError::Transport("chat ack out of order".into()));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct ChatProtocol {
    pub(crate) shared: std::sync::Weak<Shared>,
}

impl ProtocolHandler for ChatProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let from = conn.remote_id();
        if let Some(shared) = self.shared.upgrade() {
            shared.chat.peer_redialed(&from);
        }
        loop {
            let (mut send, mut recv) = match conn.accept_bi().await {
                Ok(s) => s,
                Err(_) => return Ok(()), // connection closed
            };
            let shared = self.shared.clone();
            n0_future::task::spawn(async move {
                while let Ok(Some(frame)) = read_frame(&mut recv, MAX_CHAT_MESSAGE + 24).await {
                    let Ok(message) = ChatMessage::decode(&frame) else { break };
                    let id = message.id;
                    let Some(shared) = shared.upgrade() else { break };
                    shared.emit(Event::Chat { from, message });
                    drop(shared);
                    if send.write_all(&id).await.is_err() {
                        break;
                    }
                }
            });
        }
    }
}
