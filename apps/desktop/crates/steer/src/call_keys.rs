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

// --- the key of a call (the design's section 1) ---------------------------

use std::sync::atomic::{AtomicBool, Ordering};

use coding::sealing::NewRootSeal;
use enet_core::call_seal::uuid_bytes;

/// A conversation's root as the server names it NOW (`sessions.sendTarget`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTarget {
    pub root_id: String,
    /// None = a keyless old chat.
    pub epoch: Option<u32>,
    /// The server marked this key as held by readers it dropped: no call
    /// starts on it.
    pub rotation_due: bool,
}

/// A root's stored key, as `voice.callConversation` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredKey {
    pub root_id: String,
    pub epoch: u32,
    pub public_key: String,
}

/// Why `voice.callConversation` refused. The first two have a rule (one more
/// try each), because each would end a first call `no_key` after 20 s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationRefusal {
    /// A wrap names an install that is not a reader: one on this host's
    /// last allowlist was revoked since.
    NotReader,
    /// Another writer keyed the root between the op's read and its write.
    StaleTree,
    Other(String),
}

/// What the key of a call asks of the server and of this install's key
/// holder. BLOCKING; a fake in the tests.
pub trait CallKeyOps: Send + Sync + 'static {
    /// This process has a key holder. Without one a host ends `no_key` at once.
    fn holder(&self) -> bool;
    /// `sessions.sendTarget {agentId}`; None = no conversation yet.
    fn send_target(&self, agent_id: &str) -> Result<Option<CallTarget>, String>;
    /// A fresh tree key wrapped for this install and the installs on its
    /// allowlist (`refresh` = read the allowlist again first).
    fn new_root_seal(&self, refresh: bool) -> Result<NewRootSeal, String>;
    /// `voice.callConversation`: the chat of a first call, with this key
    /// unless it has one. Answers the root's STORED key.
    fn call_conversation(&self, agent_id: &str, call_id: &str, device_id: &str, seal: &NewRootSeal) -> Result<StoredKey, ConversationRefusal>;
    /// Keep the seal's secret as `root`'s. It overwrites: only for an own key.
    fn adopt(&self, root_id: &str, seal: &NewRootSeal);
    /// Key a keyless root (or read its key); the epoch.
    fn ensure_key(&self, root_id: &str) -> Result<u32, String>;
    /// Rotate THIS root past `flagged`, no other tree.
    fn rotate(&self, root_id: &str, flagged: u32) -> Result<(), String>;
    fn secret(&self, root_id: &str, epoch: u32) -> Option<[u8; 32]>;
    /// The highest epoch this install holds for `root`.
    fn highest_held(&self, root_id: &str) -> Option<u32>;
    /// `sessionKeys.mine {rootIds: [root]}`: it nudges the holder devices.
    fn pull(&self, root_id: &str);
    /// Answer the pending wraps of THIS root (a new install of the caller).
    fn answer_pending(&self, root_id: &str) -> Result<(), String>;
}

/// The no-rollback rule's own reason in the log, so support can tell it from
/// a dropped call.
pub const EPOCH_BELOW_HELD: &str = "epoch_below_held";

/// The secret of (root, epoch): the keyring, else a pull every `pull_every`
/// until the socket abandons the source. An epoch below the highest this
/// install holds is refused: the rule has no way back.
fn held_secret(ops: &dyn CallKeyOps, root_id: &str, epoch: u32, pull_every: Duration, stop: &AtomicBool) -> Result<[u8; 32], String> {
    if let Some(held) = ops.highest_held(root_id).filter(|held| *held > epoch) {
        log::warn!("call keys: {EPOCH_BELOW_HELD}: the server names epoch {epoch} of {root_id}, this install holds {held}");
        return Err(EPOCH_BELOW_HELD.into());
    }
    loop {
        if let Some(secret) = ops.secret(root_id, epoch) {
            return Ok(secret);
        }
        if stop.load(Ordering::SeqCst) {
            return Err("the call ended while its key was pulled".into());
        }
        ops.pull(root_id);
        if let Some(secret) = ops.secret(root_id, epoch) {
            return Ok(secret);
        }
        let waited = std::time::Instant::now();
        while waited.elapsed() < pull_every && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(pull_every.min(Duration::from_millis(50)));
        }
    }
}

/// What a HOST does when the caller's hello arrives, before it answers
/// (section 1, steps 1 to 4; its hello is step 5). BLOCKING.
pub fn host_key(ops: &dyn CallKeyOps, call_id: &str, agent_id: &str, device_id: &str, pull_every: Duration, stop: &AtomicBool) -> Result<CallKey, String> {
    if !ops.holder() {
        return Err("no key holder".into());
    }
    let refused = |refusal: ConversationRefusal| format!("the chat of a first call was refused: {refusal:?}");
    // 1. The root. No conversation yet: a first call creates the chat with its key.
    let (root_id, mut epoch, due) = match ops.send_target(agent_id)? {
        Some(target) => {
            let epoch = match target.epoch {
                Some(epoch) => epoch,
                None => ops.ensure_key(&target.root_id)?,
            };
            (target.root_id, epoch, target.rotation_due)
        }
        None => {
            let mut seal = ops.new_root_seal(false)?;
            let stored = match ops.call_conversation(agent_id, call_id, device_id, &seal) {
                Ok(stored) => stored,
                // An install on the last allowlist was revoked since: a fresh
                // allowlist, a new seal, once.
                Err(ConversationRefusal::NotReader) => {
                    seal = ops.new_root_seal(true)?;
                    ops.call_conversation(agent_id, call_id, device_id, &seal).map_err(refused)?
                }
                // Another writer keyed the root in between: once more, and
                // the server answers the stored key.
                Err(ConversationRefusal::StaleTree) => ops.call_conversation(agent_id, call_id, device_id, &seal).map_err(refused)?,
                Err(other) => return Err(refused(other)),
            };
            // ONLY an own key is adopted: an adoption overwrites what the
            // keyring holds and no pull repairs a held epoch, so a wrong one
            // would break every later call to this agent on this host.
            if stored.public_key == seal.public_key && stored.epoch == seal.epoch {
                ops.adopt(&stored.root_id, &seal);
            }
            (stored.root_id, stored.epoch, false)
        }
    };
    // 2. A key the server marked as held by readers it dropped: rotated
    // first, THIS root alone, and the hello names the new epoch.
    if due {
        ops.rotate(&root_id, epoch).map_err(|err| format!("the due rotation failed: {err}"))?;
        let rotated = ops.send_target(agent_id)?.filter(|target| target.root_id == root_id && !target.rotation_due);
        epoch = rotated.and_then(|target| target.epoch).ok_or("the rotation is still due")?;
    }
    // 3. Its own secret.
    let secret = held_secret(ops, &root_id, epoch, pull_every, stop)?;
    // 4. The caller's install may be new: its wrap, for this root.
    if let Err(err) = ops.answer_pending(&root_id) {
        log::debug!("call keys: answering the pending wraps of {root_id} failed: {err}");
    }
    Ok(CallKey { root_id, epoch, secret })
}

/// What a CALLER does when the host's hello arrives: the root and the epoch
/// of its OWN answer (`before` = the one from before it dialled), which the
/// handshake's `finish` then holds against the hello. BLOCKING.
pub fn caller_key(ops: &dyn CallKeyOps, agent_id: &str, before: Option<&CallTarget>, hello: &CallHello, pull_every: Duration, stop: &AtomicBool) -> Result<CallKey, String> {
    if !ops.holder() {
        return Err("no key holder".into());
    }
    // Its own answer names the hello's root and epoch, and no due rotation
    // (the real host rotated before its hello). Asked again ONCE.
    let agrees = |target: &CallTarget| uuid_bytes(&target.root_id).is_ok_and(|root| root == hello.root) && target.epoch == Some(hello.epoch) && !target.rotation_due;
    let mine = match before.filter(|target| agrees(target)) {
        Some(target) => target.clone(),
        None => ops
            .send_target(agent_id)?
            .filter(agrees)
            .ok_or("the hello names another root or epoch than this caller's own answer, or the rotation is still due")?,
    };
    let secret = held_secret(ops, &mine.root_id, hello.epoch, pull_every, stop)?;
    Ok(CallKey { root_id: mine.root_id, epoch: hello.epoch, secret })
}

fn pull_every() -> Duration {
    Duration::from_millis(etalk::call::VOICE_CALL.seal.pull_every_ms)
}

/// Runs one blocking answer on the runtime's blocking pool. Dropping the
/// future (the call ended) raises `stop`: the loop inside ends, and what it
/// still answers is dropped.
fn blocking(run: impl FnOnce(&AtomicBool) -> Result<CallKey, String> + Send + 'static) -> CallKeyFuture<'static> {
    struct Abandon(Arc<AtomicBool>);
    impl Drop for Abandon {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let abandon = Abandon(Arc::clone(&stop));
    Box::pin(async move {
        let _abandon = abandon;
        tokio::task::spawn_blocking(move || run(&stop)).await.map_err(|err| err.to_string())?
    })
}

/// A host's key source for one call.
pub struct HostKeys {
    pub ops: Arc<dyn CallKeyOps>,
    pub call_id: String,
    pub agent_id: String,
    pub device_id: String,
    /// Told the root the call adopted (a first call's chat exists from here
    /// on): the host logic sends the first utterance into it, sealed.
    pub adopted: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

impl CallKeySource for HostKeys {
    fn key(&self, _peer: CallHello) -> CallKeyFuture<'_> {
        let (ops, call_id, agent_id, device_id) = (Arc::clone(&self.ops), self.call_id.clone(), self.agent_id.clone(), self.device_id.clone());
        let adopted = self.adopted.clone();
        blocking(move |stop| {
            let key = host_key(&*ops, &call_id, &agent_id, &device_id, pull_every(), stop)?;
            if let Some(adopted) = adopted {
                adopted(key.root_id.clone());
            }
            Ok(key)
        })
    }
}

/// A caller's key source for one call.
pub struct CallerKeys {
    pub ops: Arc<dyn CallKeyOps>,
    pub agent_id: String,
    /// Its `sendTarget` answer from before it dialled (None = no chat yet).
    pub before: Option<CallTarget>,
}

impl CallKeySource for CallerKeys {
    fn key(&self, peer: CallHello) -> CallKeyFuture<'_> {
        let (ops, agent_id, before) = (Arc::clone(&self.ops), self.agent_id.clone(), self.before.clone());
        blocking(move |stop| caller_key(&*ops, &agent_id, before.as_ref(), &peer, pull_every(), stop))
    }
}

/// `voice.callConversation`'s refusals by what the server answers
/// (apps/web/src/lib/sessions/tree-keys.ts): a second key for a keyed root
/// is a CONFLICT (409); a wrap for a non-reader a BAD_REQUEST whose message
/// says so.
pub fn conversation_refusal(err: &api::ApiError) -> ConversationRefusal {
    match err {
        api::ApiError::Http { status: 409, .. } => ConversationRefusal::StaleTree,
        api::ApiError::Http { status: 400, message } if message.contains("not a reader") => ConversationRefusal::NotReader,
        other => ConversationRefusal::Other(other.to_string()),
    }
}

/// How long a refused first call waits for a fresh allowlist.
const ALLOWLIST_WAIT: Duration = Duration::from_secs(5);

/// [`CallKeyOps`] of a real install: tRPC and the node's key holder. The
/// holder is read at every call: a node that stopped has none, and the call
/// ends `no_key`.
pub struct InstallKeyOps {
    pub trpc: Arc<api::TrpcClient>,
    pub node: crate::enet::InstallNode,
}

impl InstallKeyOps {
    fn key_holder(&self) -> Result<crate::seal_keys::KeyHolder, String> {
        self.node.key_holder().ok_or_else(|| "no key holder".to_string())
    }
}

impl CallKeyOps for InstallKeyOps {
    fn holder(&self) -> bool {
        self.node.key_holder().is_some()
    }

    fn send_target(&self, agent_id: &str) -> Result<Option<CallTarget>, String> {
        let target = api::sessions::send_target(&self.trpc, Some(agent_id), None).map_err(|err| err.to_string())?;
        Ok(target.root_id.map(|root_id| CallTarget { root_id, epoch: target.epoch, rotation_due: target.rotation_due }))
    }

    fn new_root_seal(&self, refresh: bool) -> Result<NewRootSeal, String> {
        if refresh {
            // An install on the last allowlist was revoked: read it again.
            let passes = self.node.passes();
            self.node.refresh();
            if !self.node.wait_passes_blocking(passes + 1, ALLOWLIST_WAIT) {
                return Err("the allowlist could not be read again".into());
            }
        }
        self.node.new_root_seal(None)
    }

    fn call_conversation(&self, agent_id: &str, call_id: &str, device_id: &str, seal: &NewRootSeal) -> Result<StoredKey, ConversationRefusal> {
        let wraps = seal.wraps.iter().map(|wrap| api::sessions::SealWrap { install_id: wrap.install_id.clone(), wrapped: wrap.wrapped.clone() }).collect();
        let seal = api::sessions::SendSeal { install_id: seal.install_id.clone(), public_key: seal.public_key.clone(), epoch: seal.epoch, wraps };
        api::voice::voice_call_conversation(&self.trpc, agent_id, call_id, device_id, &seal)
            .map(|stored| StoredKey { root_id: stored.root_id, epoch: stored.epoch, public_key: stored.public_key })
            .map_err(|err| conversation_refusal(&err))
    }

    fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
        coding::sealing::adopt_new_root(root_id, seal);
    }

    fn ensure_key(&self, root_id: &str) -> Result<u32, String> {
        use coding::sealing::SealService as _;
        self.key_holder()?.ensure_key(root_id).map(|(epoch, _)| epoch).map_err(|err| err.to_string())
    }

    fn rotate(&self, root_id: &str, flagged: u32) -> Result<(), String> {
        // Not this install's to rotate, or done by another holder: the
        // second `sendTarget` tells whether the root is still due.
        self.key_holder()?.rotate_root(root_id, flagged).map(|_| ())
    }

    fn secret(&self, root_id: &str, epoch: u32) -> Option<[u8; 32]> {
        use coding::sealing::SealService as _;
        self.node.key_holder()?.keyring().secret(root_id, epoch)
    }

    fn highest_held(&self, root_id: &str) -> Option<u32> {
        use coding::sealing::SealService as _;
        self.node.key_holder()?.keyring().epochs(root_id).into_iter().max()
    }

    fn pull(&self, root_id: &str) {
        use coding::sealing::SealService as _;
        if let Some(holder) = self.node.key_holder() {
            holder.fetch_secrets(root_id);
        }
    }

    fn answer_pending(&self, root_id: &str) -> Result<(), String> {
        self.key_holder()?.answer_pending_for(root_id).map(|_| ()).map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    const ROOT: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER: &str = "11111111-1111-4111-8111-111111111112";
    const CALL: &str = "22222222-2222-4222-8222-222222222222";
    const AGENT: &str = "33333333-3333-4333-8333-333333333333";
    const QUICK: Duration = Duration::from_millis(5);

    /// The server's and the key holder's rules for one agent's conversation.
    #[derive(Default)]
    struct World {
        no_holder: bool,
        target: Option<CallTarget>,
        /// The root's stored key once a conversation exists: (epoch, public).
        stored: Option<(u32, String)>,
        /// Refusals `callConversation` answers first, in order.
        refusals: VecDeque<ConversationRefusal>,
        /// Somebody keyed the chat a moment before: this key is stored, not ours.
        winner: Option<String>,
        secrets: HashMap<(String, u32), [u8; 32]>,
        /// A wrap that reaches this install on this pull: (pulls left, epoch, secret).
        arriving: Option<(u32, u32, [u8; 32])>,
        rotation_fails: bool,
        /// Other trees flagged at the same time (they must stay untouched).
        other_due: Vec<String>,
        log: Vec<String>,
    }

    #[derive(Default)]
    struct FakeOps(Mutex<World>);

    impl FakeOps {
        fn new(world: World) -> Self {
            FakeOps(Mutex::new(world))
        }
        fn log(&self) -> Vec<String> {
            self.0.lock().unwrap().log.clone()
        }
        fn seal(public: &str) -> NewRootSeal {
            NewRootSeal { install_id: "host".into(), public_key: public.into(), epoch: 1, wraps: Vec::new(), secret: [1; 32] }
        }
    }

    impl CallKeyOps for FakeOps {
        fn holder(&self) -> bool {
            !self.0.lock().unwrap().no_holder
        }
        fn send_target(&self, _agent_id: &str) -> Result<Option<CallTarget>, String> {
            let mut world = self.0.lock().unwrap();
            world.log.push("sendTarget".into());
            Ok(world.target.clone())
        }
        fn new_root_seal(&self, refresh: bool) -> Result<NewRootSeal, String> {
            let mut world = self.0.lock().unwrap();
            world.log.push(format!("seal refresh={refresh}"));
            let count = world.log.iter().filter(|line| line.starts_with("seal ")).count();
            Ok(Self::seal(&format!("own-{count}")))
        }
        fn call_conversation(&self, _agent_id: &str, _call_id: &str, _device_id: &str, seal: &NewRootSeal) -> Result<StoredKey, ConversationRefusal> {
            let mut world = self.0.lock().unwrap();
            world.log.push(format!("callConversation {}", seal.public_key));
            if let Some(refusal) = world.refusals.pop_front() {
                return Err(refusal);
            }
            let public = world.winner.clone().unwrap_or_else(|| seal.public_key.clone());
            world.stored = Some((1, public.clone()));
            world.target = Some(CallTarget { root_id: ROOT.into(), epoch: Some(1), rotation_due: false });
            Ok(StoredKey { root_id: ROOT.into(), epoch: 1, public_key: public })
        }
        fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
            let mut world = self.0.lock().unwrap();
            world.log.push(format!("adopt {}", seal.public_key));
            world.secrets.insert((root_id.into(), seal.epoch), seal.secret);
        }
        fn ensure_key(&self, root_id: &str) -> Result<u32, String> {
            let mut world = self.0.lock().unwrap();
            world.log.push("ensureKey".into());
            world.secrets.insert((root_id.into(), 1), [2; 32]);
            Ok(1)
        }
        fn rotate(&self, root_id: &str, flagged: u32) -> Result<(), String> {
            let mut world = self.0.lock().unwrap();
            world.log.push(format!("rotate {root_id} past {flagged}"));
            if world.rotation_fails {
                return Err("not a rotator".into());
            }
            world.secrets.insert((root_id.into(), flagged + 1), [3; 32]);
            world.target = Some(CallTarget { root_id: root_id.into(), epoch: Some(flagged + 1), rotation_due: false });
            Ok(())
        }
        fn secret(&self, root_id: &str, epoch: u32) -> Option<[u8; 32]> {
            self.0.lock().unwrap().secrets.get(&(root_id.to_string(), epoch)).copied()
        }
        fn highest_held(&self, root_id: &str) -> Option<u32> {
            self.0.lock().unwrap().secrets.keys().filter(|(root, _)| root == root_id).map(|(_, epoch)| *epoch).max()
        }
        fn pull(&self, root_id: &str) {
            let mut world = self.0.lock().unwrap();
            world.log.push("pull".into());
            if let Some((left, epoch, secret)) = world.arriving.take() {
                if left <= 1 {
                    world.secrets.insert((root_id.into(), epoch), secret);
                } else {
                    world.arriving = Some((left - 1, epoch, secret));
                }
            }
        }
        fn answer_pending(&self, root_id: &str) -> Result<(), String> {
            self.0.lock().unwrap().log.push(format!("pending {root_id}"));
            Ok(())
        }
    }

    fn host(ops: &FakeOps) -> Result<CallKey, String> {
        host_key(ops, CALL, AGENT, "desk-7f3a", QUICK, &AtomicBool::new(false))
    }

    fn keyed(epoch: u32, due: bool) -> World {
        World {
            target: Some(CallTarget { root_id: ROOT.into(), epoch: Some(epoch), rotation_due: due }),
            secrets: HashMap::from([((ROOT.to_string(), epoch), [9; 32])]),
            ..World::default()
        }
    }

    fn summary(key: &CallKey) -> (String, u32, [u8; 32]) {
        (key.root_id.clone(), key.epoch, key.secret)
    }

    /// A first call: the chat exists and is keyed before the host's hello,
    /// its own key adopted, the caller install's wrap answered.
    #[test]
    fn a_first_call_has_a_keyed_root_before_the_hello() {
        let ops = FakeOps::default();
        assert_eq!(summary(&host(&ops).unwrap()), (ROOT.to_string(), 1, [1; 32]));
        assert_eq!(ops.log(), ["sendTarget", "seal refresh=false", "callConversation own-1", "adopt own-1", format!("pending {ROOT}").as_str()]);
    }

    /// Two first calls at once, or a first call that races a first message:
    /// the host that lost keeps NO secret of its own (an adoption overwrites
    /// and no pull repairs it) and takes the stored key by pull.
    #[test]
    fn a_host_that_lost_the_race_adopts_nothing() {
        let ops = FakeOps::new(World { winner: Some("theirs".into()), arriving: Some((2, 1, [5; 32])), ..World::default() });
        assert_eq!(summary(&host(&ops).unwrap()), (ROOT.to_string(), 1, [5; 32]), "the stored key, on the second pull");
        let log = ops.log();
        assert!(!log.iter().any(|line| line.starts_with("adopt")), "{log:?}");
        assert_eq!(log.iter().filter(|line| *line == "pull").count(), 2);
    }

    /// The two refusals with a rule are each met with ONE more try (a
    /// refreshed allowlist and a new seal for the non-reader) and the call
    /// connects; a second refusal, or any other, ends it.
    #[test]
    fn the_two_refusals_get_one_more_try_each() {
        let ops = FakeOps::new(World { refusals: [ConversationRefusal::NotReader].into(), ..World::default() });
        assert!(host(&ops).is_ok());
        assert_eq!(ops.log()[1..5], ["seal refresh=false", "callConversation own-1", "seal refresh=true", "callConversation own-2"]);
        let ops = FakeOps::new(World { refusals: [ConversationRefusal::StaleTree].into(), ..World::default() });
        assert!(host(&ops).is_ok());
        assert_eq!(ops.log()[1..4], ["seal refresh=false", "callConversation own-1", "callConversation own-1"]);
        for refusals in [vec![ConversationRefusal::NotReader, ConversationRefusal::NotReader], vec![ConversationRefusal::StaleTree, ConversationRefusal::StaleTree], vec![ConversationRefusal::Other("not your agent".into())]] {
            let tries = if refusals.len() == 2 { 2 } else { 1 };
            let ops = FakeOps::new(World { refusals: refusals.into(), ..World::default() });
            assert!(host(&ops).is_err());
            assert_eq!(ops.log().iter().filter(|line| line.starts_with("callConversation")).count(), tries);
        }
    }

    /// A keyless old chat is keyed; a held secret needs no pull.
    #[test]
    fn a_keyless_root_is_keyed() {
        let ops = FakeOps::new(World { target: Some(CallTarget { root_id: ROOT.into(), epoch: None, rotation_due: false }), ..World::default() });
        assert_eq!(summary(&host(&ops).unwrap()), (ROOT.to_string(), 1, [2; 32]));
        assert_eq!(ops.log(), ["sendTarget", "ensureKey", format!("pending {ROOT}").as_str()]);
    }

    /// The host's install has no wrap yet: it pulls until one arrives, and
    /// stops when the call ends first.
    #[test]
    fn a_secret_arrives_on_a_later_pull() {
        let ops = FakeOps::new(World { secrets: HashMap::new(), arriving: Some((3, 2, [4; 32])), ..keyed(2, false) });
        assert_eq!(summary(&host(&ops).unwrap()), (ROOT.to_string(), 2, [4; 32]));
        assert_eq!(ops.log().iter().filter(|line| *line == "pull").count(), 3);
        // The pending pass is for THIS root (a backlog above 200 pairs of
        // other roots cannot hide the caller's new install).
        assert_eq!(ops.log().last().unwrap(), &format!("pending {ROOT}"));
        let ops = FakeOps::new(World { secrets: HashMap::new(), ..keyed(2, false) });
        assert!(host_key(&ops, CALL, AGENT, "desk-7f3a", QUICK, &AtomicBool::new(true)).is_err(), "abandoned: the loop stops");
        assert!(ops.log().iter().filter(|line| *line == "pull").count() <= 1);
    }

    #[test]
    fn no_key_holder_ends_at_once() {
        let ops = FakeOps::new(World { no_holder: true, ..keyed(1, false) });
        assert!(host(&ops).is_err());
        assert!(ops.log().is_empty(), "nothing is asked");
    }

    /// No rollback: the server names an epoch below the highest this install
    /// holds. Refused, with its own reason.
    #[test]
    fn an_epoch_below_the_highest_held_is_refused() {
        let mut world = keyed(2, false);
        world.secrets.insert((ROOT.to_string(), 3), [8; 32]);
        let ops = FakeOps::new(world);
        assert_eq!(host(&ops).err().as_deref(), Some(EPOCH_BELOW_HELD));
        let hello = CallHello { epoch: 2, root: uuid_bytes(ROOT).unwrap(), eph: [0; 32] };
        let before = CallTarget { root_id: ROOT.into(), epoch: Some(2), rotation_due: false };
        assert_eq!(caller_key(&ops, AGENT, Some(&before), &hello, QUICK, &AtomicBool::new(false)).err().as_deref(), Some(EPOCH_BELOW_HELD));
    }

    /// A due rotation is done first, for THIS root alone, and the hello
    /// names the new epoch; one that fails ends the call.
    #[test]
    fn a_due_rotation_is_done_first_for_this_root_alone() {
        let ops = FakeOps::new(World { other_due: vec![OTHER.into()], ..keyed(4, true) });
        assert_eq!(summary(&host(&ops).unwrap()), (ROOT.to_string(), 5, [3; 32]));
        assert_eq!(ops.log(), ["sendTarget", format!("rotate {ROOT} past 4").as_str(), "sendTarget", format!("pending {ROOT}").as_str()]);
        assert!(ops.0.lock().unwrap().other_due.contains(&OTHER.to_string()));
        let ops = FakeOps::new(World { rotation_fails: true, ..keyed(4, true) });
        assert!(host(&ops).is_err());
        assert!(!ops.log().iter().any(|line| line.starts_with("pending")));
    }

    /// The caller: its own answer must name the hello's root and epoch and
    /// no due rotation; it asks again ONCE (a first call: its answer was
    /// "none"), then pulls its secret.
    #[test]
    fn a_caller_holds_its_own_answer_against_the_hello() {
        let hello = |root: &str, epoch| CallHello { epoch, root: uuid_bytes(root).unwrap(), eph: [0; 32] };
        let caller = |ops: &FakeOps, before: Option<&CallTarget>, hello: &CallHello| caller_key(ops, AGENT, before, hello, QUICK, &AtomicBool::new(false));
        let same = CallTarget { root_id: ROOT.into(), epoch: Some(4), rotation_due: false };

        // The answer from before the dial agrees: no second ask.
        let ops = FakeOps::new(keyed(4, false));
        assert_eq!(summary(&caller(&ops, Some(&same), &hello(ROOT, 4)).unwrap()), (ROOT.to_string(), 4, [9; 32]));
        assert!(ops.log().is_empty());
        // A first call: asked again once, and the secret arrives by pull.
        let ops = FakeOps::new(World { secrets: HashMap::new(), arriving: Some((2, 4, [6; 32])), ..keyed(4, false) });
        assert_eq!(summary(&caller(&ops, None, &hello(ROOT, 4)).unwrap()), (ROOT.to_string(), 4, [6; 32]));
        assert_eq!(ops.log(), ["sendTarget", "pull", "pull"]);
        // The host rotated before its hello: the second answer names the new epoch.
        let due = CallTarget { rotation_due: true, ..same.clone() };
        let ops = FakeOps::new(keyed(5, false));
        assert!(caller(&ops, Some(&due), &hello(ROOT, 5)).is_ok());
        // Still another root, another epoch, or still due after asking again: no call.
        for (world, hello) in [(keyed(4, false), hello(OTHER, 4)), (keyed(4, false), hello(ROOT, 3)), (keyed(4, true), hello(ROOT, 4))] {
            let ops = FakeOps::new(world);
            assert!(caller(&ops, Some(&due), &hello).is_err());
            assert_eq!(ops.log(), ["sendTarget"], "asked again once, never twice");
        }
        // No conversation at all after the hello: no call.
        assert!(caller(&FakeOps::default(), None, &hello(ROOT, 1)).is_err());
    }

    #[test]
    fn the_servers_two_refusals_are_told_apart() {
        let http = |status, message: &str| api::ApiError::Http { status, message: message.into() };
        assert_eq!(conversation_refusal(&http(409, "The tree was keyed since")), ConversationRefusal::StaleTree);
        assert_eq!(conversation_refusal(&http(400, "A wrap names an install that is not a reader")), ConversationRefusal::NotReader);
        assert!(matches!(conversation_refusal(&http(400, "Invalid input")), ConversationRefusal::Other(_)));
        assert!(matches!(conversation_refusal(&http(403, "not your agent")), ConversationRefusal::Other(_)));
    }

    /// The sources run on the blocking pool; a call that ends drops the
    /// future and the pull loop stops.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_source_stops_when_the_call_ends() {
        let ops = Arc::new(FakeOps::new(World { secrets: HashMap::new(), ..keyed(2, false) }));
        let source = HostKeys { ops: ops.clone(), call_id: CALL.into(), agent_id: AGENT.into(), device_id: "desk-7f3a".into(), adopted: None };
        let hello = CallHello { epoch: 0, root: [0; 16], eph: [0; 32] };
        assert!(tokio::time::timeout(Duration::from_millis(100), source.key(hello)).await.is_err(), "still pulling");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let pulls = ops.log().len();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(ops.log().len(), pulls, "abandoned: no pull after the call ended");
        let ready = Arc::new(FakeOps::new(keyed(2, false)));
        let roots = Arc::new(Mutex::new(Vec::new()));
        let told = roots.clone();
        let adopted: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |root| told.lock().unwrap().push(root));
        let source = HostKeys { ops: ready, call_id: CALL.into(), agent_id: AGENT.into(), device_id: "desk-7f3a".into(), adopted: Some(adopted) };
        assert_eq!(source.key(hello).await.unwrap().epoch, 2);
        assert_eq!(*roots.lock().unwrap(), [ROOT], "the host logic is told the root the call adopted");
    }
}
