//! Per-peer connection tracking (EXP-1531): every authenticated connection,
//! dialed or accepted, on any ALPN, is registered here after its handshake.
//!
//! - When the LAST live connection to a peer closes (the peer closed it, the
//!   QUIC idle timeout found it dead, a network change killed it), the node
//!   raises [`crate::Event::PeerDisconnected`] at once, so a viewer can treat
//!   its feed as dropped instead of waiting out a stale timer.
//! - [`PathKind`]: the transmission path the peer's connections use right now
//!   (a direct UDP path or one through an iroh relay), for connection stats.
//!
//! Handles are weak: tracking never keeps a connection alive.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use iroh::{
    endpoint::{Connection, WeakConnectionHandle},
    EndpointId,
};
use tokio::sync::mpsc;

use crate::event::Event;

/// How a peer is reached right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// A direct UDP path (holepunched, LAN or port-mapped).
    Direct,
    /// Through an iroh relay (no direct path, or not yet upgraded).
    IrohRelay,
}

impl PathKind {
    /// The connStats `path` value (`fixtures/steer-enet.json` `connStats.paths`).
    pub fn as_str(self) -> &'static str {
        match self {
            PathKind::Direct => "direct",
            PathKind::IrohRelay => "iroh_relay",
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct Links {
    peers: Arc<Mutex<HashMap<EndpointId, Vec<(usize, WeakConnectionHandle)>>>>,
}

impl std::fmt::Debug for Links {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Links")
    }
}

impl Links {
    /// Tracks `conn` until it closes; the last one of its peer to close
    /// raises `PeerDisconnected` on `events`.
    pub(crate) fn track(&self, conn: &Connection, events: mpsc::UnboundedSender<Event>) {
        let peer = conn.remote_id();
        let id = conn.stable_id();
        let weak = conn.weak_handle();
        // Taken while `conn` is strong: guaranteed to see the close.
        let closed = weak.closed();
        self.peers.lock().expect("links lock").entry(peer).or_default().push((id, weak));
        let peers = self.peers.clone();
        n0_future::task::spawn(async move {
            closed.await;
            let last = {
                let mut map = peers.lock().expect("links lock");
                let Some(list) = map.get_mut(&peer) else { return };
                list.retain(|(i, w)| *i != id && w.upgrade().is_some_and(|c| c.close_reason().is_none()));
                let empty = list.is_empty();
                if empty {
                    map.remove(&peer);
                }
                empty
            };
            if last {
                let _ = events.send(Event::PeerDisconnected { peer });
            }
        });
    }

    fn live(&self, peer: &EndpointId) -> Vec<Connection> {
        let map = self.peers.lock().expect("links lock");
        map.get(peer)
            .map(|list| {
                list.iter()
                    .filter_map(|(_, w)| w.upgrade())
                    .filter(|c| c.close_reason().is_none())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Closes every open connection to `peer` (both directions).
    pub(crate) fn close(&self, peer: &EndpointId) -> usize {
        let live = self.live(peer);
        for conn in &live {
            conn.close(0u32.into(), b"disconnect");
        }
        live.len()
    }

    /// Whether any connection to `peer` is open.
    pub(crate) fn connected(&self, peer: &EndpointId) -> bool {
        !self.live(peer).is_empty()
    }

    /// The best path any open connection to `peer` transmits on: direct
    /// wins over relayed; `None` = no open connection.
    pub(crate) fn path(&self, peer: &EndpointId) -> Option<PathKind> {
        let mut best = None;
        for conn in self.live(peer) {
            for p in conn.paths().iter() {
                if !p.is_selected() {
                    continue;
                }
                if p.is_ip() {
                    return Some(PathKind::Direct);
                }
                if p.is_relay() {
                    best = Some(PathKind::IrohRelay);
                }
            }
        }
        best
    }
}
