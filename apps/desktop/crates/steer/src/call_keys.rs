//! #EXP-1941: where a call HOST gets the key of a call. A host the server
//! told `seal` joins with a [`CallSealing`]: its own call id, agent id and
//! device id (they go into every key) and a [`CallKeySource`], which does
//! what must happen before the host's hello: resolve the conversation's root
//! on the server, create or key it for a first call, rotate a due key, hold
//! the tree SECRET and answer the caller install's pending wrap (the design
//! `call-seal-design.md` on #EXP-1941, section 1). The real source follows
//! with the key of a call; this is the trait and a fixed source for tests.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

pub use enet_core::call_seal::CallHello;

/// The key a host answers a caller's hello with: the root and the epoch its
/// hello names, and that tree's secret. Never logged; wiped when dropped.
pub struct CallKey {
    pub root_id: String,
    pub epoch: u32,
    pub secret: [u8; 32],
}

impl Drop for CallKey {
    fn drop(&mut self) {
        self.secret.fill(0);
    }
}

pub type CallKeyFuture<'a> = Pin<Box<dyn Future<Output = Result<CallKey, String>> + Send + 'a>>;

pub trait CallKeySource: Send + Sync {
    /// The key of this call, once, for the OTHER end's hello. A host gets
    /// the caller's (its root is all zeros on a first call) and answers the
    /// root and the epoch its own hello will name. A caller gets the host's
    /// and answers the root and the epoch its OWN `sendTarget` answer names
    /// now (asked again once when the hello names others or a rotation was
    /// due): `finish` holds the two against the hello. `Err` = no key (its
    /// reason is logged, never a key): the call ends `no_key`. The socket
    /// drops the future when the call ends first: a source must stop then.
    fn key(&self, peer: CallHello) -> CallKeyFuture<'_>;
}

/// What a host told `seal` joins with.
#[derive(Clone)]
pub struct CallSealing {
    /// The ring's call id and agent id, after the server vouched for both.
    pub call_id: String,
    pub agent_id: String,
    /// The HOST's steer device id: a host's own, a caller's `startCall` answer.
    pub device_id: String,
    /// CALLER ONLY: the root and the epoch its `sendTarget` answer named
    /// before it dialled (None on a first call: its hello carries zeros).
    pub caller_target: Option<(String, u32)>,
    pub keys: Arc<dyn CallKeySource>,
}

/// A source with one fixed answer after `delay` (None = no key): tests.
#[doc(hidden)]
pub struct FixedKeySource {
    pub key: Option<(String, u32, [u8; 32])>,
    pub delay: Duration,
}

impl CallKeySource for FixedKeySource {
    fn key(&self, _peer: CallHello) -> CallKeyFuture<'_> {
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            let (root_id, epoch, secret) = self.key.clone().ok_or("no key holder")?;
            Ok(CallKey { root_id, epoch, secret })
        })
    }
}
