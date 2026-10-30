use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
};

use iroh::{endpoint::Connection, EndpointId};

use crate::node::NOT_ALLOWED_CODE;

type Hook = Arc<dyn Fn(EndpointId, &[u8]) -> bool + Send + Sync>;

/// Who may open connections TO this node (every channel). A peer is let in
/// when the node is open, the peer is in the allowed set, or the app's hook
/// (`(peer, alpn) -> bool`) says yes. Everyone else is refused right after
/// the TLS handshake (so the refusal is authenticated) and an
/// [`crate::Event::PeerRefused`] is raised. Outgoing connections are not
/// gated: dialing a peer is the app's own decision.
///
/// Taking access away also ends what was already let in: [`Access::revoke`]
/// and `set_open(false)` close every admitted connection whose peer is no
/// longer allowed on its channel (close code 403, so the peer sees
/// `Refused`), instead of letting a cached link keep talking.
#[derive(Clone, Default)]
pub struct Access {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    open: AtomicBool,
    allowed: RwLock<HashSet<EndpointId>>,
    hook: RwLock<Option<Hook>>,
    /// Incoming connections let in, per peer (pruned as they close).
    live: Mutex<HashMap<EndpointId, Vec<Connection>>>,
}

impl std::fmt::Debug for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Access")
            .field("open", &self.is_open())
            .field("allowed", &self.inner.allowed.read().map(|s| s.len()).unwrap_or(0))
            .finish()
    }
}

impl Access {
    pub fn allow(&self, peer: EndpointId) {
        self.inner.allowed.write().expect("access lock").insert(peer);
    }
    pub fn revoke(&self, peer: &EndpointId) {
        self.inner.allowed.write().expect("access lock").remove(peer);
        self.enforce(Some(peer));
    }
    pub fn allowed_peers(&self) -> Vec<EndpointId> {
        self.inner.allowed.read().expect("access lock").iter().copied().collect()
    }
    pub fn set_open(&self, open: bool) {
        self.inner.open.store(open, Ordering::SeqCst);
        if !open {
            self.enforce(None);
        }
    }
    pub fn is_open(&self) -> bool {
        self.inner.open.load(Ordering::SeqCst)
    }
    /// Install (or clear) the app's decision hook. Called on a network task:
    /// keep it fast and never block on the node.
    pub fn set_hook(&self, hook: Option<impl Fn(EndpointId, &[u8]) -> bool + Send + Sync + 'static>) {
        *self.inner.hook.write().expect("access lock") = hook.map(|h| Arc::new(h) as Hook);
    }
    pub fn is_allowed(&self, peer: &EndpointId, alpn: &[u8]) -> bool {
        if self.is_open() || self.inner.allowed.read().expect("access lock").contains(peer) {
            return true;
        }
        let hook = self.inner.hook.read().expect("access lock").clone();
        hook.is_some_and(|h| h(*peer, alpn))
    }

    /// Remembers an incoming connection the policy let in.
    pub(crate) fn admitted(&self, conn: &Connection) {
        let mut live = self.inner.live.lock().expect("access lock");
        live.retain(|_, conns| {
            conns.retain(|c| c.close_reason().is_none());
            !conns.is_empty()
        });
        live.entry(conn.remote_id()).or_default().push(conn.clone());
    }

    /// Closes the admitted connections (of `peer`, or of everyone) whose
    /// peer the policy no longer allows on that channel.
    fn enforce(&self, peer: Option<&EndpointId>) {
        let candidates: Vec<Connection> = {
            let live = self.inner.live.lock().expect("access lock");
            match peer {
                Some(p) => live.get(p).cloned().unwrap_or_default(),
                None => live.values().flatten().cloned().collect(),
            }
        };
        for conn in candidates {
            if !self.is_allowed(&conn.remote_id(), conn.alpn()) {
                conn.close(NOT_ALLOWED_CODE.into(), b"not allowed");
            }
        }
    }
}
