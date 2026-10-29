use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};

use iroh::EndpointId;

type Hook = Arc<dyn Fn(EndpointId, &[u8]) -> bool + Send + Sync>;

/// Who may open connections TO this node (every channel). A peer is let in
/// when the node is open, the peer is in the allowed set, or the app's hook
/// (`(peer, alpn) -> bool`) says yes. Everyone else is refused right after
/// the TLS handshake (so the refusal is authenticated) and an
/// [`crate::Event::PeerRefused`] is raised. Outgoing connections are not
/// gated: dialing a peer is the app's own decision.
#[derive(Clone, Default)]
pub struct Access {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    open: AtomicBool,
    allowed: RwLock<HashSet<EndpointId>>,
    hook: RwLock<Option<Hook>>,
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
    }
    pub fn allowed_peers(&self) -> Vec<EndpointId> {
        self.inner.allowed.read().expect("access lock").iter().copied().collect()
    }
    pub fn set_open(&self, open: bool) {
        self.inner.open.store(open, Ordering::SeqCst);
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
}
