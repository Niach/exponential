//! EXP-1458: the install's KEY HOLDER, beside its eNet node (`enet.rs`).
//! Sealed session content (`packages/domain-contract/fixtures/sealed-content.json`)
//! needs every reader install to hold its trees' secrets; this is the
//! desktop app's and the daemon's half:
//!
//! - **Cache**: the tree secrets this install unwrapped live in
//!   `coding::sealing::Keyring`, a 0600 file beside the eNet identity
//!   (`seal-keys` / `cli-seal-keys`), per account.
//! - **Holder loop**: on start, every [`HOLDER_PASS`] and on the server's
//!   `check_in` nudge (EXP-1472: a new install's registration nudges the
//!   holders; at most every [`WAKE_SPACING`]): `sessionKeys.mine`
//!   (unwrap what is new with the install's X25519 secret, derived from the
//!   eNet identity) then `sessionKeys.pending` → `sessionKeys.wrap` for
//!   every listed reader. Automatic, no approval (the #EXP-1457 decision).
//! - **Writers**: [`KeyHolder::ensure_key`] = the root's current key, else
//!   a fresh tree key `sessionKeys.create`d with this install's own wrap
//!   (the device claiming a server-started session); the other readers get
//!   theirs from the next `pending` pass, which runs at once.
//!
//! - **Rotation** (#FEED-177, #EXP-1951: sealed-content.json `rotation`): a
//!   tree that lost a reader (it turned private, an install was revoked, a
//!   member or a device left: `sessions.seal_rotate_epoch` set, their wraps
//!   dropped; while it is set the server lets ANY reader rotate) gets
//!   a fresh key at `epoch + 1` from the first holder pass of an install
//!   that holds its current secret and may rotate (the starter's installs
//!   or the executing device; the server decides, a refusal is
//!   remembered). It is created exactly as a first key (own wrap, the
//!   current readers through `pending`), so the dropped installs never get
//!   it; old epochs stay openable for old content. A `CONFLICT` = another
//!   holder rotated first: re-read the key.
//!
//! - **Signing** (#EXP-1950, `packages/domain-contract/fixtures/wrap-signature.json`):
//!   every wrap this holder MAKES carries its install's signature (the eNet
//!   identity, `enet_core::wrap_sig`): a create's own wrap (first key and
//!   rotation) and each `pending` answer. A new root's inline seal cannot be
//!   signed (its root id is unknown while sealing): once adopted, the holder
//!   `resign`s its wraps. Decision (c): a key held from before stays, and
//!   the holder signs its own unsigned row of a root's current epoch again,
//!   once per root, at most [`RESIGN_MAX`] wraps per pass; an older server
//!   (`NOT_FOUND`) is asked again after [`RESIGN_NOT_YET`], quietly. The app
//!   and the daemon are two installs, each signing with its own identity.
//!
//! Installed process-wide (`coding::sealing::install`) once the install is
//! registered; the engine's replies, the pickup, the brief and the device
//! MCP gateway all seal and open through it.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::session_keys::{
    CreateKey, HeldKey, InstallWrap, PairWrap, PendingWrap, RequestedKey, ResignOutput, ResignWrap, RootKey, RESIGN_MAX,
};
use coding::sealing::{self, CreateFailure, KeylessStep, Keyring, NewRootSeal, RequestKeyAnswer, SealError, SealService};
use enet_core::sealed::{b64url_encode, generate_tree_key, public_key_from_b64, unwrap_tree_key, wrap_tree_key};
use enet_core::wrap_sig::{endpoint_id_hex, sign_wrap, WrapStatement};
use enet_core::{Identity, PushKeypair};

/// How often a holder answers `pending` (and re-reads `mine`) while online.
pub const HOLDER_PASS: Duration = Duration::from_secs(60);
/// EXP-1472: the closest two woken passes run (a `check_in` nudge wakes one;
/// nudges come with every piece of new work).
pub const WAKE_SPACING: Duration = Duration::from_secs(5);
/// #EXP-1950: how long a server without `sessionKeys.resign` is left alone.
pub const RESIGN_NOT_YET: Duration = Duration::from_secs(30 * 60);
/// #EXP-1950: the most single-root `resign` calls one pass makes (roots a
/// refused batch named, tried alone so one cannot hold back the others).
pub const RESIGN_ALONE_PER_PASS: usize = 8;

/// #EXP-1950 wrap-signature.json `resign.failureCases`: what a holder does
/// after a failed `resign`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResignAfter {
    /// An older server: no such procedure. Quietly again later.
    NotYet,
    /// FORBIDDEN / BAD_REQUEST: the pass goes on without these wraps.
    Refused,
    /// Anything else: the next pass.
    Retry,
}

/// `resignAfter`: `status` = the HTTP status (None = no answer).
pub fn resign_after(status: Option<u16>) -> ResignAfter {
    match status {
        Some(404) => ResignAfter::NotYet,
        Some(400 | 403) => ResignAfter::Refused,
        _ => ResignAfter::Retry,
    }
}

/// The `sessionKeys.*` calls the holder makes: tRPC in the apps, a fake in
/// the tests. BLOCKING.
pub trait KeyApi: Send + Sync + 'static {
    fn create(&self, input: &CreateKey) -> Result<(), ApiError>;
    fn mine(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError>;
    fn pending(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError>;
    fn wrap(&self, install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError>;
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError>;
    /// #EXP-1950 `sessionKeys.resign` (default: a server without it).
    fn resign(&self, _install_id: &str, _wraps: &[ResignWrap]) -> Result<ResignOutput, ApiError> {
        Err(ApiError::Http { status: 404, message: "No procedure found on path \"sessionKeys.resign\"".into() })
    }
    /// #EXP-1960: `sessionKeys.requestKey` (default: no answer).
    fn request_key(&self, _root_id: &str) -> Result<RequestedKey, ApiError> {
        Err(ApiError::Transport { message: "requestKey unsupported".into(), offline: false })
    }
}

impl KeyApi for api::TrpcClient {
    fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
        api::session_keys::create(self, input)
    }
    fn mine(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
        api::session_keys::mine(self, install_id, root_ids)
    }
    fn pending(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
        api::session_keys::pending(self, install_id, root_ids)
    }
    fn wrap(&self, install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
        api::session_keys::wrap(self, install_id, wraps).map(|_| ())
    }
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
        api::session_keys::public_keys(self, root_ids)
    }
    fn request_key(&self, root_id: &str) -> Result<RequestedKey, ApiError> {
        api::session_keys::request_key(self, root_id)
    }
    fn resign(&self, install_id: &str, wraps: &[ResignWrap]) -> Result<ResignOutput, ApiError> {
        api::session_keys::resign(self, install_id, wraps)
    }
}

/// Agent-to-agent delivery over eNet (`enet.rs` sets it once the node
/// runs): `(target install, sealed message row)`.
pub type DirectSender = Arc<dyn Fn(&coding::sealing::DeliverTo, &domain::rows::MessageRow) + Send + Sync>;
/// #EXP-1483: a device's installs `(install_id, x25519_public)`, by its
/// `devices.device_id`.
pub type DeviceReaders = Arc<dyn Fn(&str) -> Vec<(String, String)> + Send + Sync>;
/// #EXP-1950: an install's `(endpoint_id, x25519_public)` by its install id
/// (the eNet allowlist): whom a new root's re-signed wraps name.
pub type InstallLookup = Arc<dyn Fn(&str) -> Option<(String, String)> + Send + Sync>;

/// #EXP-1950: the install's signing identity and its endpoint id.
struct Signer {
    identity: Identity,
    endpoint_id: String,
}

/// #EXP-1950: what is left to sign again.
#[derive(Default)]
struct ResignState {
    /// Roots whose own row of the highest held epoch is unsigned (`mine`).
    own: BTreeMap<String, u32>,
    /// Adopted new roots: `(root, epoch, the seal's target installs)`.
    fresh: Vec<(String, u32, Vec<String>)>,
    /// `(root, epoch)` signed again or refused: never asked again.
    done: HashSet<(String, u32)>,
    /// Roots a refused batch named: tried one per call.
    alone: HashSet<String>,
    not_yet_until: Option<std::time::Instant>,
}

/// One root's wraps to sign again.
struct ResignJob {
    root_id: String,
    epoch: u32,
    targets: Vec<String>,
}

/// One install's key holder. Cheap to clone.
#[derive(Clone)]
pub struct KeyHolder {
    inner: Arc<HolderInner>,
}

struct HolderInner {
    install_id: String,
    /// The install's X25519 secret (`PushKeypair::derive(identity)`).
    install_secret: [u8; 32],
    install_public: [u8; 32],
    /// #EXP-1950: None = this holder signs nothing (tests, old callers).
    signer: Option<Signer>,
    api: Arc<dyn KeyApi>,
    /// Shared with `coding::sealing`'s preload: the reader opened rows off
    /// it before this holder ran.
    keyring: Arc<Keyring>,
    /// Serializes key creation (two writers of one root in this process).
    creating: Mutex<()>,
    /// #FEED-177: rotations the server refused this install (not the
    /// starter's, not the executing device): `(root, flagged epoch)`.
    refused: Mutex<HashSet<(String, u32)>>,
    stopped: AtomicBool,
    direct: Mutex<Option<DirectSender>>,
    /// #EXP-1483: a device's installs from the eNet allowlist.
    device_readers: Mutex<Option<DeviceReaders>>,
    /// The running loop's early-pass signal, and when it last fired.
    wake: Mutex<Option<Arc<tokio::sync::Notify>>>,
    woken_at: Mutex<Option<std::time::Instant>>,
    installs: Mutex<Option<InstallLookup>>,
    resign: Mutex<ResignState>,
}

impl KeyHolder {
    /// A holder that signs nothing.
    pub fn new(install_id: String, push: &PushKeypair, api: Arc<dyn KeyApi>, keyring: impl Into<Arc<Keyring>>) -> KeyHolder {
        KeyHolder::build(install_id, push, None, api, keyring.into())
    }

    /// #EXP-1950: the install's holder, signing every wrap it makes with
    /// `identity` (its X25519 key is derived from it).
    pub fn signing(install_id: String, identity: &Identity, api: Arc<dyn KeyApi>, keyring: impl Into<Arc<Keyring>>) -> KeyHolder {
        let signer = Signer { identity: identity.clone(), endpoint_id: endpoint_id_hex(identity) };
        KeyHolder::build(install_id, &PushKeypair::derive(identity), Some(signer), api, keyring.into())
    }

    fn build(install_id: String, push: &PushKeypair, signer: Option<Signer>, api: Arc<dyn KeyApi>, keyring: Arc<Keyring>) -> KeyHolder {
        KeyHolder {
            inner: Arc::new(HolderInner {
                install_id,
                install_secret: push.secret(),
                install_public: push.public(),
                signer,
                api,
                keyring,
                creating: Mutex::new(()),
                refused: Mutex::default(),
                stopped: AtomicBool::new(false),
                direct: Mutex::new(None),
                device_readers: Mutex::new(None),
                wake: Mutex::new(None),
                woken_at: Mutex::new(None),
                installs: Mutex::new(None),
                resign: Mutex::default(),
            }),
        }
    }

    pub fn install_id(&self) -> &str {
        &self.inner.install_id
    }

    /// The agent-to-agent sender (the eNet half).
    pub fn set_direct_sender(&self, sender: DirectSender) {
        *lock(&self.inner.direct) = Some(sender);
    }

    /// #EXP-1483: where [`SealService::new_root_readers`] looks a device's
    /// installs up (the eNet half).
    pub fn set_device_readers(&self, readers: DeviceReaders) {
        *lock(&self.inner.device_readers) = Some(readers);
    }

    /// #EXP-1950: where a new root's re-signed wraps look their targets up.
    pub fn set_install_lookup(&self, lookup: InstallLookup) {
        *lock(&self.inner.installs) = Some(lookup);
    }

    /// #EXP-1950: the wrap signature of a wrap THIS holder just made, or
    /// None (no signer, no recipient endpoint from an older server, or a
    /// malformed field: the wrap goes unsigned, as before).
    fn sign(&self, root_id: &str, epoch: u32, tree_public_key: &str, recipient: Option<&str>, wrapped: &str) -> Option<String> {
        let signer = self.inner.signer.as_ref()?;
        let statement = WrapStatement { root_id, epoch, tree_public_key, recipient_endpoint_id: recipient?, wrapped };
        match sign_wrap(&signer.identity, &statement) {
            Ok(signature) => Some(signature),
            Err(err) => {
                log::debug!("seal-keys: the wrap of {root_id} epoch {epoch} goes unsigned: {err}");
                None
            }
        }
    }

    /// Unwraps every held key not cached yet. Returns how many were new.
    /// BLOCKING.
    pub fn pull(&self, root_ids: Option<&[String]>) -> Result<usize, ApiError> {
        let held = self.inner.api.mine(&self.inner.install_id, root_ids)?;
        self.note_unsigned(&held);
        let mut fresh = 0;
        for key in held {
            if self.inner.keyring.secret(&key.root_id, key.epoch).is_some() {
                continue;
            }
            match unwrap_tree_key(&self.inner.install_secret, &key.wrapped) {
                Ok(secret) => {
                    if self.inner.keyring.insert_secret(&key.root_id, key.epoch, secret) {
                        fresh += 1;
                    }
                }
                Err(err) => log::warn!("seal-keys: wrap for {} epoch {} does not open: {err}", key.root_id, key.epoch),
            }
        }
        Ok(fresh)
    }

    /// #EXP-1950 decision (c): the roots whose own row of the highest held
    /// epoch is unsigned, to sign again ([`Self::resign_due`]).
    fn note_unsigned(&self, held: &[HeldKey]) {
        if self.inner.signer.is_none() {
            return;
        }
        let mut highest: BTreeMap<&str, &HeldKey> = BTreeMap::new();
        for key in held {
            let entry = highest.entry(&key.root_id).or_insert(key);
            if key.epoch > entry.epoch {
                *entry = key;
            }
        }
        let mut state = lock(&self.inner.resign);
        for (root, key) in highest {
            if key.signature.is_none() && !state.done.contains(&(root.to_string(), key.epoch)) {
                state.own.insert(root.to_string(), key.epoch);
            } else {
                state.own.remove(root);
            }
        }
    }

    /// Answers every pending reader with a wrap. Returns how many pairs
    /// were sent. BLOCKING.
    pub fn answer_pending(&self) -> Result<usize, ApiError> {
        self.answer(None)
    }

    /// #EXP-1941: [`Self::answer_pending`] for ONE root: a call answers the
    /// wrap of the caller's new install before its hello, and one pass over
    /// every root is capped (`PENDING_MAX` pairs, sorted by root id), so
    /// that pair may not be in it. BLOCKING.
    pub fn answer_pending_for(&self, root_id: &str) -> Result<usize, ApiError> {
        self.answer(Some(&[root_id.to_string()]))
    }

    /// #EXP-1941: one rotation of THIS root past `flagged`, now: a call does
    /// not start on a key the server marked as held by readers it dropped,
    /// and [`Self::rotate_due`] rotates every flagged tree of the install.
    /// The root's key is read from the server first (the synced rows lag).
    /// `Ok(false)` = not this install's to rotate, or another holder did.
    pub fn rotate_root(&self, root_id: &str, flagged: u32) -> Result<bool, String> {
        if lock(&self.inner.refused).contains(&(root_id.to_string(), flagged)) {
            return Ok(false);
        }
        self.lookup(root_id)?;
        self.rotate(root_id, flagged)
    }

    fn answer(&self, root_ids: Option<&[String]>) -> Result<usize, ApiError> {
        let pending = self.inner.api.pending(&self.inner.install_id, root_ids)?;
        let mut wraps = Vec::new();
        for pair in pending {
            let Some(secret) = self.inner.keyring.secret(&pair.root_id, pair.epoch) else {
                // The server says we hold it; fetch it for the next pass.
                continue;
            };
            let Ok(target) = public_key_from_b64(&pair.target_x25519_public) else { continue };
            match wrap_tree_key(&target, &secret) {
                Ok(wrapped) => {
                    // #EXP-1950: signed over the key this install HOLDS; a
                    // root whose stored key is another one gets it unsigned
                    // (the server would refuse the whole batch).
                    let ours = b64url_encode(&PushKeypair::from_secret(secret).public()) == pair.public_key;
                    let signature = ours
                        .then(|| self.sign(&pair.root_id, pair.epoch, &pair.public_key, pair.target_endpoint_id.as_deref(), &wrapped))
                        .flatten();
                    wraps.push(PairWrap {
                        root_id: pair.root_id,
                        epoch: pair.epoch,
                        target_install_id: pair.target_install_id,
                        wrapped,
                        signature,
                    })
                }
                Err(err) => log::warn!("seal-keys: wrapping for {} failed: {err}", pair.target_install_id),
            }
        }
        if wraps.is_empty() {
            return Ok(0);
        }
        self.inner.api.wrap(&self.inner.install_id, &wraps)?;
        log::info!("seal-keys: wrapped {} pending reader key(s)", wraps.len());
        Ok(wraps.len())
    }

    /// #FEED-177: rotates every flagged tree this install may rotate
    /// ([`Keyring::rotations_due`]). Returns how many it rotated. BLOCKING.
    pub fn rotate_due(&self) -> usize {
        let mut rotated = 0;
        for (root_id, flagged) in self.inner.keyring.rotations_due() {
            if lock(&self.inner.refused).contains(&(root_id.clone(), flagged)) {
                continue;
            }
            match self.rotate(&root_id, flagged) {
                Ok(true) => rotated += 1,
                Ok(false) => {}
                Err(err) => log::debug!("seal-keys: rotating {root_id} failed: {err}"),
            }
        }
        rotated
    }

    /// One rotation of `root` past `flagged`. `Ok(false)` = not needed or
    /// not this install's (another holder rotated, or the server refused).
    fn rotate(&self, root_id: &str, flagged: u32) -> Result<bool, String> {
        let _creating = lock(&self.inner.creating);
        // Re-checked under the lock: a writer of this process may have moved on.
        let Some((epoch, _)) = self.inner.keyring.current(root_id) else { return Ok(false) };
        if epoch > flagged {
            self.inner.keyring.rotated(root_id);
            return Ok(false);
        }
        match self.create_key(root_id, epoch + 1) {
            Ok(_) => {
                self.inner.keyring.rotated(root_id);
                log::info!("seal-keys: rotated the tree key of {root_id} to epoch {}", epoch + 1);
                Ok(true)
            }
            // Someone else rotated (or keyed) first: theirs is current.
            Err(Created::Conflict) => {
                self.inner.keyring.rotated(root_id);
                self.lookup(root_id)?;
                if let Err(err) = self.pull(Some(&[root_id.to_string()])) {
                    log::debug!("seal-keys: sessionKeys.mine {root_id} failed: {err}");
                }
                Ok(false)
            }
            // Not the starter's install nor the executing device (403), or
            // no longer a reader (404): leave it to the holders that are.
            Err(Created::Refused) => {
                lock(&self.inner.refused).insert((root_id.to_string(), flagged));
                Ok(false)
            }
            // #EDE-97: a clear tree is never keyed (nor rotated).
            Err(Created::Clear) => {
                self.inner.keyring.rotated(root_id);
                Ok(false)
            }
            Err(Created::Failed(err)) => Err(err),
        }
    }

    /// A fresh tree key at `epoch` (a tree's first, or a rotation),
    /// `sessionKeys.create`d with this install's own wrap and adopted as
    /// the root's current key. The other CURRENT readers get theirs from
    /// `pending` (the server lists readers only), answered at once.
    fn create_key(&self, root_id: &str, epoch: u32) -> Result<[u8; 32], Created> {
        let key = generate_tree_key();
        let own = wrap_tree_key(&self.inner.install_public, &key.secret).map_err(|e| Created::Failed(e.to_string()))?;
        let public_key = key.public_key();
        let recipient = self.inner.signer.as_ref().map(|signer| signer.endpoint_id.clone());
        let signature = self.sign(root_id, epoch, &public_key, recipient.as_deref(), &own);
        let input = CreateKey {
            root_id: root_id.to_string(),
            install_id: self.inner.install_id.clone(),
            epoch,
            public_key,
            wraps: vec![InstallWrap { install_id: self.inner.install_id.clone(), wrapped: own, signature }],
        };
        // #EXP-1960 (`keylessCases`): a blip (no answer, a 5xx, a 429) is the
        // same create again, `CREATE_RETRY` x attempt later, up to `CREATE_ATTEMPTS`.
        let mut attempt = 1;
        let answer = loop {
            let answer = self.inner.api.create(&input);
            let failure = match &answer {
                Err(ApiError::Http { status, message }) => Some(sealing::create_failure(Some(*status), message)),
                Err(ApiError::Transport { message, .. }) => Some(sealing::create_failure(None, message)),
                _ => None,
            };
            if failure == Some(CreateFailure::Transient) && sealing::keyless_step(CreateFailure::Transient, attempt, false) == KeylessStep::Retry {
                std::thread::sleep(sealing::CREATE_RETRY * attempt);
                attempt += 1;
                continue;
            }
            break answer;
        };
        match answer {
            Ok(()) => {
                self.inner.keyring.insert_secret(root_id, epoch, key.secret);
                self.inner.keyring.set_current(root_id, epoch, key.public);
                let holder = self.clone();
                std::thread::spawn(move || {
                    if let Err(err) = holder.answer_pending() {
                        log::debug!("seal-keys: first pending pass failed: {err}");
                    }
                });
                Ok(key.public)
            }
            Err(ApiError::Http { status: 409, .. }) => Err(Created::Conflict),
            Err(ApiError::Http { status: 403 | 404, .. }) => Err(Created::Refused),
            // #EDE-97: the server refuses to key a clear tree (its row may
            // not have synced here yet): remembered, plaintext is its wire.
            Err(ApiError::Http { status: 400, message }) if message.contains(coding::sealing::CLEAR_REFUSAL) => {
                coding::sealing::note_clear_tree(root_id);
                Err(Created::Clear)
            }
            Err(err) => Err(Created::Failed(err.to_string())),
        }
    }

    /// One holder pass: pull, rotate what is flagged, answer, then sign
    /// again what is unsigned. BLOCKING.
    pub fn pass(&self) {
        if let Err(err) = self.pull(None) {
            log::debug!("seal-keys: sessionKeys.mine failed: {err}");
        }
        self.rotate_due();
        if let Err(err) = self.answer_pending() {
            log::debug!("seal-keys: sessionKeys.pending/wrap failed: {err}");
        }
        self.resign_due();
    }

    /// #EXP-1950 (wrap-signature.json `resign`): fresh signed wraps of a
    /// root's current epoch replacing unsigned rows: an adopted new root's
    /// (this install and the seal's targets) and this install's own rows of
    /// keys held from before. Once per root; one batch of at most
    /// [`RESIGN_MAX`] wraps plus [`RESIGN_ALONE_PER_PASS`] single roots per
    /// pass. Returns how many rows the server replaced. BLOCKING.
    pub fn resign_due(&self) -> u32 {
        if self.inner.signer.is_none() {
            return 0;
        }
        let (batch, alone) = {
            let state = lock(&self.inner.resign);
            if state.not_yet_until.is_some_and(|until| std::time::Instant::now() < until) {
                return 0;
            }
            let mut jobs: Vec<ResignJob> = state
                .fresh
                .iter()
                .map(|(root_id, epoch, targets)| ResignJob { root_id: root_id.clone(), epoch: *epoch, targets: targets.clone() })
                .collect();
            for (root_id, epoch) in &state.own {
                if !jobs.iter().any(|job| &job.root_id == root_id) {
                    jobs.push(ResignJob { root_id: root_id.clone(), epoch: *epoch, targets: vec![self.inner.install_id.clone()] });
                }
            }
            let (alone, batch): (Vec<ResignJob>, Vec<ResignJob>) = jobs.into_iter().partition(|job| state.alone.contains(&job.root_id));
            (batch, alone.into_iter().take(RESIGN_ALONE_PER_PASS).collect::<Vec<_>>())
        };
        let mut replaced = 0;
        let mut calls: Vec<Vec<(ResignJob, Vec<ResignWrap>)>> = Vec::new();
        let mut packed = Vec::new();
        let mut count = 0;
        for job in batch {
            let wraps = self.resign_wraps(&job);
            if wraps.is_empty() || count + wraps.len() > RESIGN_MAX {
                continue;
            }
            count += wraps.len();
            packed.push((job, wraps));
        }
        if !packed.is_empty() {
            calls.push(packed);
        }
        for job in alone {
            let wraps = self.resign_wraps(&job);
            if !wraps.is_empty() {
                calls.push(vec![(job, wraps)]);
            }
        }
        for call in calls {
            let wraps: Vec<ResignWrap> = call.iter().flat_map(|(_, wraps)| wraps.iter().cloned()).collect();
            let answer = self.inner.api.resign(&self.inner.install_id, &wraps);
            let after = match &answer {
                Ok(output) => {
                    replaced += output.replaced;
                    None
                }
                Err(ApiError::Http { status, .. }) => Some(resign_after(Some(*status))),
                Err(_) => Some(resign_after(None)),
            };
            let mut state = lock(&self.inner.resign);
            match after {
                None => call.iter().for_each(|(job, _)| finish_resign(&mut state, job)),
                Some(ResignAfter::NotYet) => {
                    state.not_yet_until = Some(std::time::Instant::now() + RESIGN_NOT_YET);
                    break;
                }
                Some(ResignAfter::Refused) if call.len() > 1 => {
                    call.iter().for_each(|(job, _)| {
                        state.alone.insert(job.root_id.clone());
                    });
                }
                Some(ResignAfter::Refused) => {
                    let job = &call[0].0;
                    log::debug!("seal-keys: signing {} again was refused", job.root_id);
                    // A new root's target may be no reader any more: its
                    // own row alone next, else never again.
                    let own = vec![self.inner.install_id.clone()];
                    match state.fresh.iter_mut().find(|(root, epoch, _)| *root == job.root_id && *epoch == job.epoch) {
                        Some((_, _, targets)) if *targets != own => *targets = own,
                        _ => finish_resign(&mut state, job),
                    }
                }
                Some(ResignAfter::Retry) => {}
            }
        }
        if replaced > 0 {
            log::info!("seal-keys: signed {replaced} wrap(s) again");
        }
        replaced
    }

    /// Fresh signed wraps of `job`'s key for its targets: this install, and
    /// the others the allowlist names (one it does not know is left out).
    /// Never the bytes of a wrap this holder did not make.
    fn resign_wraps(&self, job: &ResignJob) -> Vec<ResignWrap> {
        let Some(secret) = self.inner.keyring.secret(&job.root_id, job.epoch) else { return Vec::new() };
        let Some(signer) = self.inner.signer.as_ref() else { return Vec::new() };
        let tree_public = b64url_encode(&PushKeypair::from_secret(secret).public());
        let lookup = lock(&self.inner.installs).clone();
        let mut out = Vec::new();
        for target in &job.targets {
            let (endpoint_id, x25519) = if *target == self.inner.install_id {
                (signer.endpoint_id.clone(), self.inner.install_public)
            } else {
                let Some((endpoint_id, x25519)) = lookup.as_ref().and_then(|lookup| lookup(target)) else { continue };
                let Ok(x25519) = public_key_from_b64(&x25519) else { continue };
                (endpoint_id, x25519)
            };
            let Ok(wrapped) = wrap_tree_key(&x25519, &secret) else { continue };
            let Some(signature) = self.sign(&job.root_id, job.epoch, &tree_public, Some(&endpoint_id), &wrapped) else { continue };
            out.push(ResignWrap { root_id: job.root_id.clone(), epoch: job.epoch, install_id: target.clone(), wrapped, signature });
        }
        if out.is_empty() {
            // Nothing this holder can sign for it (a malformed id): not again.
            finish_resign(&mut lock(&self.inner.resign), job);
        }
        out
    }

    fn lookup(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String> {
        let rows = self.inner.api.public_keys(&[root_id.to_string()]).map_err(|e| e.to_string())?;
        let Some(row) = rows.into_iter().find(|row| row.root_id == root_id) else {
            return Ok(None);
        };
        let public = public_key_from_b64(&row.public_key).map_err(|e| e.to_string())?;
        self.inner.keyring.set_current(root_id, row.epoch, public);
        Ok(Some((row.epoch, public)))
    }

    /// Stops the loop (sign-out) and uninstalls the service.
    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
        sealing::uninstall();
    }

    /// Runs the holder loop on `runtime` (blocking calls on its blocking
    /// pool) until [`KeyHolder::stop`]; `wake` runs a pass early.
    pub fn spawn(&self, runtime: &tokio::runtime::Handle, wake: Arc<tokio::sync::Notify>) {
        *lock(&self.inner.wake) = Some(Arc::clone(&wake));
        let holder = self.clone();
        runtime.spawn(async move {
            while !holder.inner.stopped.load(Ordering::SeqCst) {
                let pass = holder.clone();
                let _ = tokio::task::spawn_blocking(move || pass.pass()).await;
                tokio::select! {
                    _ = tokio::time::sleep(HOLDER_PASS) => {}
                    _ = wake.notified() => {}
                }
            }
        });
    }
}

impl SealService for KeyHolder {
    fn keyring(&self) -> &Keyring {
        &self.inner.keyring
    }

    /// #EXP-1950: the seal went unsigned; its wraps are signed again now
    /// that the root exists.
    fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
        sealing::adopt_into(&self.inner.keyring, root_id, seal);
        if self.inner.signer.is_some() {
            let targets = seal.wraps.iter().map(|wrap| wrap.install_id.clone()).collect();
            lock(&self.inner.resign).fresh.push((root_id.to_string(), seal.epoch, targets));
            self.wake();
        }
    }

    fn install_id(&self) -> Option<String> {
        Some(self.inner.install_id.clone())
    }

    fn install_public(&self) -> Option<[u8; 32]> {
        Some(self.inner.install_public)
    }

    fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
        if let Some(current) = self.inner.keyring.current(root_id) {
            return Ok(current);
        }
        let _creating = lock(&self.inner.creating);
        if let Some(current) = self.inner.keyring.current(root_id) {
            return Ok(current);
        }
        if let Some(found) = self.lookup(root_id).map_err(SealError::NoKey)? {
            return Ok(found);
        }
        // The root has no key: this install is its first content writer.
        // The other readers (the starter's installs, the team when shared)
        // get their wraps from this install's pending pass.
        match self.create_key(root_id, 1) {
            Ok(public) => {
                log::info!("seal-keys: created the tree key of {root_id}");
                Ok((1, public))
            }
            // Another writer keyed it first: use theirs.
            Err(Created::Conflict) => self
                .lookup(root_id)
                .map_err(SealError::NoKey)?
                .ok_or_else(|| SealError::NoKey("the tree was keyed concurrently but has no key".to_string())),
            Err(Created::Refused) => Err(SealError::NotReader),
            Err(Created::Clear) => Err(SealError::ClearTree),
            Err(Created::Failed(err)) => Err(SealError::NoKey(err)),
        }
    }

    fn public_key(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String> {
        if let Some(current) = self.inner.keyring.current(root_id) {
            return Ok(Some(current));
        }
        self.lookup(root_id)
    }

    fn fetch_secrets(&self, root_id: &str) {
        if let Err(err) = self.pull(Some(&[root_id.to_string()])) {
            log::debug!("seal-keys: sessionKeys.mine {root_id} failed: {err}");
        }
    }

    fn wake(&self) {
        let mut woken_at = lock(&self.inner.woken_at);
        if woken_at.is_some_and(|at| at.elapsed() < WAKE_SPACING) {
            return;
        }
        if let Some(wake) = lock(&self.inner.wake).as_ref() {
            *woken_at = Some(std::time::Instant::now());
            wake.notify_one();
        }
    }

    fn request_key(&self, root_id: &str) -> RequestKeyAnswer {
        match self.inner.api.request_key(root_id) {
            Ok(RequestedKey::Keyed { epoch, public_key }) => match public_key_from_b64(&public_key) {
                Ok(public) => RequestKeyAnswer::Keyed(epoch, public),
                Err(_) => RequestKeyAnswer::Error,
            },
            Ok(RequestedKey::Clear) => RequestKeyAnswer::Clear,
            Ok(RequestedKey::Requested) => {
                log::info!("seal-keys: asked the devices of {root_id} to key it");
                RequestKeyAnswer::Requested
            }
            Ok(RequestedKey::NoHost) => RequestKeyAnswer::NoHost,
            Err(ApiError::Http { status: 404, .. }) => RequestKeyAnswer::NotFound,
            Err(err) => {
                log::debug!("seal-keys: sessionKeys.requestKey {root_id} failed: {err}");
                RequestKeyAnswer::Error
            }
        }
    }

    fn new_root_readers(&self, target_device: &str) -> Vec<(String, String)> {
        let readers = lock(&self.inner.device_readers).clone();
        readers.map(|readers| readers(target_device)).unwrap_or_default()
    }

    fn deliver_direct(&self, deliver_to: &coding::sealing::DeliverTo, message: &domain::rows::MessageRow) {
        let sender = lock(&self.inner.direct).clone();
        if let Some(sender) = sender {
            sender(deliver_to, message);
        }
    }
}

/// How a [`KeyHolder::create_key`] failed.
enum Created {
    /// 409: the tree is keyed (or rotated) already.
    Conflict,
    /// 403/404: this install may not write this key.
    Refused,
    /// #EDE-97: 400 `clearTree.clearRefusal`: a clear tree.
    Clear,
    Failed(String),
}

/// A root signed again (or refused): never asked again in this process.
fn finish_resign(state: &mut ResignState, job: &ResignJob) {
    state.done.insert((job.root_id.clone(), job.epoch));
    state.fresh.retain(|(root, epoch, _)| !(*root == job.root_id && *epoch == job.epoch));
    if state.own.get(&job.root_id) == Some(&job.epoch) {
        state.own.remove(&job.root_id);
    }
    state.alone.remove(&job.root_id);
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The server's key rules for one tree, minus Postgres: readers are a
    /// fixed set of installs.
    #[derive(Default)]
    struct FakeKeys {
        /// The CURRENT readers (`pending` lists only them).
        readers: Mutex<Vec<(String, [u8; 32])>>,
        /// Who may rotate (the starter's installs, the executing device).
        rotators: Vec<String>,
        /// Installs `create` answers 404 (not a reader of the tree).
        strangers: Vec<String>,
        roots: Mutex<HashMap<String, (u32, String)>>,
        wraps: Mutex<Vec<(String, u32, String, String)>>,
        creates: Mutex<Vec<(String, u32, String)>>,
        attempts: Mutex<u32>,
        /// #EXP-1960: `create` answers this many 5xx first.
        blips: Mutex<u32>,
        /// `requestKey`'s answer, and the roots asked.
        request: Option<RequestedKey>,
        requested: Mutex<Vec<String>>,
    }

    impl FakeKeys {
        fn with_readers(readers: Vec<(String, [u8; 32])>) -> FakeKeys {
            FakeKeys { readers: Mutex::new(readers), ..Default::default() }
        }
    }

    impl KeyApi for FakeKeys {
        fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
            *self.attempts.lock().unwrap() += 1;
            {
                let mut blips = self.blips.lock().unwrap();
                if *blips > 0 {
                    *blips -= 1;
                    return Err(ApiError::Http { status: 502, message: "Bad Gateway".into() });
                }
            }
            if self.strangers.contains(&input.install_id) {
                return Err(ApiError::Http { status: 404, message: "Session not found".into() });
            }
            let mut roots = self.roots.lock().unwrap();
            // `createTreeKeyInTx`: epoch 1 on a keyless tree, else exactly
            // `seal_epoch + 1` by a rotator.
            match roots.get(&input.root_id) {
                None if input.epoch == 1 => {}
                Some((epoch, _)) if input.epoch == epoch + 1 => {
                    if !self.rotators.contains(&input.install_id) {
                        return Err(ApiError::Http { status: 403, message: "not a rotator".into() });
                    }
                }
                _ => return Err(ApiError::Http { status: 409, message: "keyed".into() }),
            }
            self.creates.lock().unwrap().push((input.root_id.clone(), input.epoch, input.install_id.clone()));
            roots.insert(input.root_id.clone(), (input.epoch, input.public_key.clone()));
            for wrap in &input.wraps {
                self.wraps.lock().unwrap().push((input.root_id.clone(), input.epoch, wrap.install_id.clone(), wrap.wrapped.clone()));
            }
            Ok(())
        }
        fn mine(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
            Ok(self
                .wraps
                .lock()
                .unwrap()
                .iter()
                .filter(|(root, _, install, _)| install == install_id && root_ids.is_none_or(|ids| ids.contains(root)))
                .map(|(root, epoch, _, wrapped)| HeldKey { root_id: root.clone(), epoch: *epoch, wrapped: wrapped.clone(), signature: None, signer_endpoint_id: None })
                .collect())
        }
        fn pending(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
            // Snapshots, one lock at a time: `create` holds `roots` while it
            // takes `wraps`, and `create_key` runs this `pending` on its own
            // thread, so nesting `wraps` → `roots` here deadlocked a test
            // (release suite 42214218335 hung 2 h on seal_keys).
            let wraps = self.wraps.lock().unwrap().clone();
            let roots = self.roots.lock().unwrap().clone();
            let readers = self.readers.lock().unwrap().clone();
            let mut out = Vec::new();
            // Like the server: sorted by root id, at most PENDING_MAX pairs.
            let mut roots: Vec<_> = roots.into_iter().filter(|(root, _)| root_ids.is_none_or(|ids| ids.contains(root))).collect();
            roots.sort();
            for (root, (epoch, public)) in roots.iter() {
                if !wraps.iter().any(|(r, e, i, _)| r == root && e == epoch && i == install_id) {
                    continue;
                }
                for (reader, x) in readers.iter() {
                    if !wraps.iter().any(|(r, e, i, _)| r == root && e == epoch && i == reader) {
                        out.push(PendingWrap {
                            root_id: root.clone(),
                            epoch: *epoch,
                            public_key: public.clone(),
                            target_install_id: reader.clone(),
                            target_x25519_public: sealing::b64url_encode(x),
                            target_endpoint_id: None,
                        });
                    }
                }
            }
            out.truncate(api::session_keys::PENDING_MAX);
            Ok(out)
        }
        fn wrap(&self, _install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
            // Like the server: current readers only, one wrap per pair.
            let readers = self.readers.lock().unwrap().clone();
            let mut held = self.wraps.lock().unwrap();
            for wrap in wraps {
                let reader = readers.iter().any(|(install, _)| *install == wrap.target_install_id);
                let known = held.iter().any(|(r, e, i, _)| *r == wrap.root_id && *e == wrap.epoch && *i == wrap.target_install_id);
                if reader && !known {
                    held.push((wrap.root_id.clone(), wrap.epoch, wrap.target_install_id.clone(), wrap.wrapped.clone()));
                }
            }
            Ok(())
        }
        fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
            Ok(self
                .roots
                .lock()
                .unwrap()
                .iter()
                .filter(|(root, _)| root_ids.contains(root))
                .map(|(root, (epoch, public))| RootKey { root_id: root.clone(), epoch: *epoch, public_key: public.clone() })
                .collect())
        }
        fn request_key(&self, root_id: &str) -> Result<RequestedKey, ApiError> {
            self.requested.lock().unwrap().push(root_id.to_string());
            self.request.clone().ok_or(ApiError::Http { status: 404, message: "Session not found".into() })
        }
    }

    /// #EXP-1960: a 5xx on create is the same create again, a few times.
    #[test]
    fn a_blip_on_create_is_retried_then_keyed() {
        let device = PushKeypair::generate();
        let api = Arc::new(FakeKeys { blips: Mutex::new(1), ..FakeKeys::with_readers(vec![("device".into(), device.public())]) });
        let holder = KeyHolder::new("device".into(), &device, api.clone(), Keyring::in_memory());
        assert_eq!(holder.ensure_key("root").unwrap().0, 1);
        assert_eq!(*api.attempts.lock().unwrap(), 2);
        // Never answering: the attempts run out, a NoKey (the send fails), no wait.
        let api = Arc::new(FakeKeys { blips: Mutex::new(9), ..Default::default() });
        let holder = KeyHolder::new("device".into(), &device, api.clone(), Keyring::in_memory());
        assert!(matches!(holder.ensure_key("root"), Err(SealError::NoKey(_))));
        assert_eq!(*api.attempts.lock().unwrap(), sealing::CREATE_ATTEMPTS);
    }

    /// #EXP-1960: `requestKey`'s answers, read for the writer.
    #[test]
    fn request_key_answers_map_to_the_writer_steps() {
        let phone = PushKeypair::generate();
        let theirs = generate_tree_key();
        for (answer, want) in [
            (Some(RequestedKey::Requested), RequestKeyAnswer::Requested),
            (Some(RequestedKey::NoHost), RequestKeyAnswer::NoHost),
            (Some(RequestedKey::Clear), RequestKeyAnswer::Clear),
            (Some(RequestedKey::Keyed { epoch: 1, public_key: theirs.public_key() }), RequestKeyAnswer::Keyed(1, theirs.public)),
            (None, RequestKeyAnswer::NotFound),
        ] {
            let api = Arc::new(FakeKeys { request: answer, ..Default::default() });
            let holder = KeyHolder::new("phone".into(), &phone, api.clone(), Keyring::in_memory());
            assert_eq!(holder.request_key("root"), want);
            assert_eq!(api.requested.lock().unwrap().as_slice(), ["root"]);
        }
    }

    #[test]
    fn the_device_creates_a_missing_key_and_a_second_install_gets_it_through_pending() {
        let device = PushKeypair::generate();
        let phone = PushKeypair::generate();
        let api = Arc::new(FakeKeys::with_readers(vec![("device".into(), device.public()), ("phone".into(), phone.public())]));
        let holder = KeyHolder::new("device".into(), &device, api.clone(), Keyring::in_memory());
        let (epoch, public) = holder.ensure_key("root").unwrap();
        assert_eq!(epoch, 1);
        // Idempotent: the cached key, no second create.
        assert_eq!(holder.ensure_key("root").unwrap(), (1, public));
        holder.answer_pending().unwrap();

        let reader = KeyHolder::new("phone".into(), &phone, api.clone(), Keyring::in_memory());
        assert_eq!(reader.pull(None).unwrap(), 1);
        let body = enet_core::sealed::seal_body(&public, 1, "for every reader").unwrap();
        let opened = enet_core::sealed::open_body(&body, &|e| reader.keyring().secret("root", e));
        assert_eq!(opened.text.as_deref(), Some("for every reader"));
        // Nothing left pending.
        assert!(api.pending("device", None).unwrap().is_empty());
    }

    #[test]
    fn a_concurrently_keyed_root_uses_the_other_writers_key() {
        let device = PushKeypair::generate();
        let api = Arc::new(FakeKeys::default());
        let theirs = generate_tree_key();
        api.roots.lock().unwrap().insert("root".into(), (1, theirs.public_key()));
        let holder = KeyHolder::new("device".into(), &device, api, Keyring::in_memory());
        assert_eq!(holder.ensure_key("root").unwrap(), (1, theirs.public));
        assert_eq!(holder.public_key("other").unwrap(), None);
    }

    /// #FEED-188: what a sender tells apart: plaintext stays legal in a
    /// keyless tree it may not key, never after a failure it can retry.
    #[test]
    fn a_stranger_may_not_key_a_tree() {
        let phone = PushKeypair::generate();
        let api = Arc::new(FakeKeys { strangers: vec!["phone".into()], ..Default::default() });
        let holder = KeyHolder::new("phone".into(), &phone, api.clone(), Keyring::in_memory());
        assert_eq!(holder.ensure_key("root"), Err(SealError::NotReader));
        assert!(api.creates.lock().unwrap().is_empty());
    }

    /// The synced root row of a keyed tree, flagged for rotation or not.
    fn root_row(api: &FakeKeys, flagged: Option<i64>) -> domain::rows::SessionRow {
        let (epoch, public) = api.roots.lock().unwrap().get("root").cloned().unwrap();
        domain::rows::SessionRow {
            id: "root".into(),
            seal_public_key: Some(public),
            seal_epoch: Some(i64::from(epoch)),
            seal_rotate_epoch: flagged,
            ..Default::default()
        }
    }

    /// #FEED-177: device + phone (the starter's) + a teammate's laptop hold
    /// epoch 1; the tree turns private and the laptop stops being a reader.
    fn turned_private() -> (Arc<FakeKeys>, [PushKeypair; 3], [u8; 32]) {
        let keys = [PushKeypair::generate(), PushKeypair::generate(), PushKeypair::generate()];
        let api = Arc::new(FakeKeys {
            readers: Mutex::new(vec![
                ("device".into(), keys[0].public()),
                ("phone".into(), keys[1].public()),
                ("mate".into(), keys[2].public()),
            ]),
            rotators: vec!["device".into(), "phone".into()],
            ..Default::default()
        });
        let first = KeyHolder::new("device".into(), &keys[0], api.clone(), Keyring::in_memory());
        let (_, public) = first.ensure_key("root").unwrap();
        first.answer_pending().unwrap();
        assert_eq!(api.wraps.lock().unwrap().len(), 3);
        // `dropNonReaderKeysInTx` (readers first: a late wrap is refused).
        api.readers.lock().unwrap().retain(|(install, _)| install != "mate");
        api.wraps.lock().unwrap().retain(|(_, _, install, _)| install != "mate");
        (api, keys, public)
    }

    fn holder(api: &Arc<FakeKeys>, install: &str, key: &PushKeypair) -> KeyHolder {
        let holder = KeyHolder::new(install.into(), key, api.clone(), Keyring::in_memory());
        holder.pull(None).unwrap();
        holder.keyring().note_sessions(&[root_row(api, Some(1))]);
        holder
    }

    #[test]
    fn a_flagged_tree_rotates_once_for_its_current_readers_only() {
        let (api, keys, old_public) = turned_private();
        let device = holder(&api, "device", &keys[0]);
        // The teammate's install unwrapped epoch 1 before the turn.
        let mate = holder(&api, "mate", &keys[2]);
        mate.keyring().insert_secret("root", 1, device.keyring().secret("root", 1).unwrap());
        assert_eq!(device.keyring().rotations_due(), vec![("root".to_string(), 1)]);

        device.pass();
        let (epoch, public) = device.keyring().current("root").unwrap();
        assert_eq!(epoch, 2);
        assert_ne!(public, old_public);
        assert_eq!(api.roots.lock().unwrap()["root"].0, 2);
        // The current readers get epoch 2; the dropped install never does.
        device.answer_pending().unwrap();
        let epoch2: Vec<String> = api
            .wraps
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, e, _, _)| *e == 2)
            .map(|(_, _, install, _)| install.clone())
            .collect();
        assert!(epoch2.contains(&"device".to_string()) && epoch2.contains(&"phone".to_string()));
        assert!(!epoch2.contains(&"mate".to_string()));
        assert_eq!(mate.pull(None).unwrap(), 0);
        // New content seals with epoch 2: the old secret does not open it,
        // the phone's new wrap does; old content stays openable.
        let new_body = enet_core::sealed::seal_body(&public, 2, "after the turn").unwrap();
        assert_eq!(enet_core::sealed::open_body(&new_body, &|e| mate.keyring().secret("root", e)).text, None);
        let phone = holder(&api, "phone", &keys[1]);
        let opened = enet_core::sealed::open_body(&new_body, &|e| phone.keyring().secret("root", e));
        assert_eq!(opened.text.as_deref(), Some("after the turn"));
        let old_body = enet_core::sealed::seal_body(&old_public, 1, "before").unwrap();
        let opened = enet_core::sealed::open_body(&old_body, &|e| device.keyring().secret("root", e));
        assert_eq!(opened.text.as_deref(), Some("before"));

        // Idempotent: the still-flagged synced row (sync lag) and a second
        // pass never rotate again.
        let stale = domain::rows::SessionRow {
            id: "root".into(),
            seal_public_key: Some(sealing::b64url_encode(&old_public)),
            seal_epoch: Some(1),
            seal_rotate_epoch: Some(1),
            ..Default::default()
        };
        device.keyring().note_sessions(&[stale]);
        assert_eq!(device.keyring().current("root").unwrap().0, 2);
        assert!(device.keyring().rotations_due().is_empty());
        device.pass();
        assert_eq!(api.creates.lock().unwrap().iter().filter(|(_, e, _)| *e == 2).count(), 1);
    }

    /// #EXP-1941: a call rotates ITS root, now, and no other flagged tree;
    /// it reads the root's key from the server first.
    #[test]
    fn a_calls_root_rotates_alone() {
        let (api, keys, _) = turned_private();
        let device = holder(&api, "device", &keys[0]);
        // Another tree of this install is flagged at the same time.
        device.ensure_key("other").unwrap();
        device.keyring().note_sessions(&[domain::rows::SessionRow { id: "other".into(), ..root_row(&api, Some(1)) }]);
        assert_eq!(device.keyring().rotations_due().len(), 2);
        assert_eq!(device.rotate_root("root", 1), Ok(true));
        assert_eq!(api.roots.lock().unwrap()["root"].0, 2);
        assert_eq!(api.roots.lock().unwrap()["other"].0, 1, "the other flagged tree is not this call's");
        assert_eq!(device.keyring().rotations_due(), vec![("other".to_string(), 1)]);
        assert!(device.keyring().secret("root", 2).is_some());
        // Done already (another ring, another holder): nothing to do.
        assert_eq!(device.rotate_root("root", 1), Ok(false));
        // An install whose synced rows have not arrived: it reads the key itself.
        let (api, keys, _) = turned_private();
        let fresh = KeyHolder::new("phone".into(), &keys[1], api.clone(), Keyring::in_memory());
        fresh.pull(None).unwrap();
        assert_eq!(fresh.rotate_root("root", 1), Ok(true));
        // Not a rotator: refused by the server, never an error loop.
        let (api, keys, _) = turned_private();
        api.readers.lock().unwrap().push(("mate".into(), keys[2].public()));
        let mate = KeyHolder::new("mate".into(), &keys[2], api.clone(), Keyring::in_memory());
        mate.keyring().insert_secret("root", 1, [1; 32]);
        assert_eq!(mate.rotate_root("root", 1), Ok(false));
        assert_eq!(api.roots.lock().unwrap()["root"].0, 1);
    }

    /// #EXP-1941: one pass answers at most `PENDING_MAX` pairs sorted by
    /// root id; a call's root behind that backlog is answered by its own pass.
    #[test]
    fn a_calls_root_is_answered_behind_a_backlog() {
        let keys = [PushKeypair::generate(), PushKeypair::generate()];
        let api = Arc::new(FakeKeys::with_readers(vec![("device".into(), keys[0].public()), ("phone".into(), keys[1].public())]));
        let device = KeyHolder::new("device".into(), &keys[0], api.clone(), Keyring::in_memory());
        // 201 keyed roots the phone has no wrap for; the call's root sorts last.
        let roots: Vec<String> = (0..api::session_keys::PENDING_MAX).map(|n| format!("a-{n:03}")).chain(["z-call".to_string()]).collect();
        for root in &roots {
            api.roots.lock().unwrap().insert(root.clone(), (1, "k".into()));
            api.wraps.lock().unwrap().push((root.clone(), 1, "device".into(), "w".into()));
            device.keyring().insert_secret(root, 1, [3; 32]);
        }
        let phone_has = |root: &str| api.wraps.lock().unwrap().iter().any(|(r, _, install, _)| r == root && install == "phone");
        assert_eq!(device.answer_pending().unwrap(), api::session_keys::PENDING_MAX);
        assert!(!phone_has("z-call"), "one pass does not reach it");
        assert_eq!(device.answer_pending_for("z-call").unwrap(), 1);
        assert!(phone_has("z-call"));
    }

    #[test]
    fn a_rotation_conflict_adopts_the_other_holders_key() {
        let (api, keys, _) = turned_private();
        let device = holder(&api, "device", &keys[0]);
        let phone = holder(&api, "phone", &keys[1]);
        // The phone (the starter's) rotates first and answers pending.
        assert_eq!(phone.rotate_due(), 1);
        phone.answer_pending().unwrap();
        let theirs = phone.keyring().current("root").unwrap();
        // The device still saw epoch 1 flagged: its create CONFLICTs, it
        // re-reads the key and holds the phone's secret.
        assert_eq!(device.keyring().current("root").unwrap().0, 1);
        assert_eq!(device.rotate_due(), 0);
        assert_eq!(device.keyring().current("root"), Some(theirs));
        assert_eq!(device.keyring().secret("root", 2), phone.keyring().secret("root", 2));
        assert!(device.keyring().rotations_due().is_empty());
        assert_eq!(api.creates.lock().unwrap().iter().filter(|(_, e, _)| *e == 2).count(), 1);
    }

    #[test]
    fn an_install_that_may_not_rotate_asks_once() {
        let (api, keys, _) = turned_private();
        // A reader that is neither the starter's nor the executing device.
        api.readers.lock().unwrap().push(("mate".into(), keys[2].public()));
        let mate = holder(&api, "mate", &keys[2]);
        mate.keyring().insert_secret("root", 1, [9; 32]);
        let before = *api.attempts.lock().unwrap();
        assert_eq!(mate.rotate_due(), 0);
        assert_eq!(mate.rotate_due(), 0);
        assert_eq!(*api.attempts.lock().unwrap(), before + 1);
        assert_eq!(api.roots.lock().unwrap()["root"].0, 1);
    }

    // ── #EXP-1951: sealed-content.json `rotation.holderCases` ───────────────

    fn rotation_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }

    fn hex32(text: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (n, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[n * 2..n * 2 + 2], 16).unwrap();
        }
        out
    }

    /// The fixture's server double: it answers what the case says, and a
    /// `serves: foreign` one keeps handing out a row wrapped for another
    /// install.
    struct RotationDouble {
        case: serde_json::Value,
        fixture: serde_json::Value,
        answers: Mutex<Vec<String>>,
        /// Accepted creates `(root, epoch, public key)`.
        created: Mutex<Vec<(String, u32, String)>>,
        /// CONFLICTs: the other holder's key is current at that epoch.
        conflicted: Mutex<Vec<(String, u32)>>,
        creates: Mutex<Vec<(String, u32, Vec<String>)>>,
        wraps: Mutex<Vec<(String, u32, String)>>,
    }

    impl RotationDouble {
        fn readers(&self) -> Vec<String> {
            self.case["readers"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect()
        }
    }

    impl KeyApi for RotationDouble {
        fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
            let wraps = input.wraps.iter().map(|wrap| wrap.install_id.clone()).collect();
            self.creates.lock().unwrap().push((input.root_id.clone(), input.epoch, wraps));
            let answer = {
                let mut answers = self.answers.lock().unwrap();
                if answers.is_empty() { "ok".to_string() } else { answers.remove(0) }
            };
            let refuse = |status| Err(ApiError::Http { status, message: answer.clone() });
            match answer.as_str() {
                "ok" => {
                    self.created.lock().unwrap().push((input.root_id.clone(), input.epoch, input.public_key.clone()));
                    Ok(())
                }
                "blip" => Err(ApiError::Transport { message: "no answer".into(), offline: true }),
                "CONFLICT" => {
                    self.conflicted.lock().unwrap().push((input.root_id.clone(), input.epoch));
                    refuse(409)
                }
                "FORBIDDEN" => refuse(403),
                "NOT_FOUND" => refuse(404),
                other => panic!("unknown answer {other}"),
            }
        }
        fn mine(&self, _install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
            let mut rows = Vec::new();
            if self.case["serves"] == "foreign" {
                for root in self.case["held"].as_object().unwrap().keys() {
                    rows.push(HeldKey { root_id: root.clone(), epoch: 9, wrapped: self.fixture["rotation"]["foreignWrap"].as_str().unwrap().into(), signature: None, signer_endpoint_id: None });
                }
            }
            for (root, epoch) in self.conflicted.lock().unwrap().iter() {
                rows.push(HeldKey { root_id: root.clone(), epoch: *epoch, wrapped: self.fixture["rotation"]["rotatedWrap"].as_str().unwrap().into(), signature: None, signer_endpoint_id: None });
            }
            rows.retain(|row| root_ids.is_none_or(|ids| ids.contains(&row.root_id)));
            Ok(rows)
        }
        fn pending(&self, _install_id: &str, _root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
            let done = self.wraps.lock().unwrap().clone();
            let mut out = Vec::new();
            for (root, epoch, public) in self.created.lock().unwrap().iter() {
                for reader in self.readers() {
                    if done.iter().any(|(r, _, to)| r == root && *to == reader) {
                        continue;
                    }
                    out.push(PendingWrap {
                        root_id: root.clone(),
                        epoch: *epoch,
                        public_key: public.clone(),
                        target_install_id: reader,
                        target_x25519_public: self.fixture["tree"]["publicKey"].as_str().unwrap().into(),
                        target_endpoint_id: None,
                    });
                }
            }
            Ok(out)
        }
        fn wrap(&self, _install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
            let mut sent = self.wraps.lock().unwrap();
            for wrap in wraps {
                // Like the server: a pair another pass wrapped is skipped.
                if !sent.iter().any(|(r, _, to)| *r == wrap.root_id && *to == wrap.target_install_id) {
                    sent.push((wrap.root_id.clone(), wrap.epoch, wrap.target_install_id.clone()));
                }
            }
            Ok(())
        }
        fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
            Ok(self
                .conflicted
                .lock()
                .unwrap()
                .iter()
                .filter(|(root, _)| root_ids.contains(root))
                .map(|(root, epoch)| RootKey { root_id: root.clone(), epoch: *epoch, public_key: self.fixture["rotatedTree"]["publicKey"].as_str().unwrap().into() })
                .collect())
        }
    }

    /// #EXP-1951: every holder platform replays the same cases. Here the
    /// due roots arrive as the synced root rows (`seal_rotate_epoch`).
    #[test]
    fn rotation_holder_cases_replay() {
        let fixture = rotation_fixture();
        let install = PushKeypair::from_secret(hex32(fixture["install"]["secret"].as_str().unwrap()));
        let secret_at = |epoch: u32| hex32(fixture[if epoch == 1 { "tree" } else { "rotatedTree" }]["secret"].as_str().unwrap());
        for case in fixture["rotation"]["holderCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let api = Arc::new(RotationDouble {
                case: case.clone(),
                fixture: fixture.clone(),
                answers: Mutex::new(case["create"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()),
                created: Mutex::default(),
                conflicted: Mutex::default(),
                creates: Mutex::default(),
                wraps: Mutex::default(),
            });
            let holder = KeyHolder::new("self".into(), &install, api.clone(), Keyring::in_memory());
            let held = case["held"].as_object().unwrap();
            for (root, epochs) in held {
                for epoch in epochs.as_array().unwrap() {
                    let epoch = epoch.as_u64().unwrap() as u32;
                    holder.keyring().insert_secret(root, epoch, secret_at(epoch));
                }
            }
            // The synced rows: each due root flagged, at the epoch this install knows as current.
            let rows: Vec<domain::rows::SessionRow> = case["due"]
                .as_array()
                .unwrap()
                .iter()
                .map(|due| {
                    let root = due["rootId"].as_str().unwrap();
                    let flagged = due["epoch"].as_i64().unwrap();
                    let current = held.get(root).and_then(|e| e.as_array()).and_then(|e| e.iter().filter_map(|n| n.as_i64()).max()).unwrap_or(flagged);
                    let public = enet_core::PushKeypair::from_secret(secret_at(current as u32)).public();
                    domain::rows::SessionRow {
                        id: root.into(),
                        seal_public_key: Some(sealing::b64url_encode(&public)),
                        seal_epoch: Some(current),
                        seal_rotate_epoch: Some(flagged),
                        ..Default::default()
                    }
                })
                .collect();
            holder.keyring().note_sessions(&rows);
            for _ in 0..case["passes"].as_u64().unwrap_or(1) {
                holder.pass();
            }
            // The create's own pending pass runs on a thread: let it finish.
            std::thread::sleep(Duration::from_millis(50));

            let creates: Vec<serde_json::Value> = api
                .creates
                .lock()
                .unwrap()
                .iter()
                .map(|(root, epoch, wraps)| serde_json::json!({ "rootId": root, "epoch": epoch, "wraps": wraps }))
                .collect();
            assert_eq!(serde_json::Value::Array(creates), case["creates"], "{name}: creates");
            let mut wraps: Vec<(String, u64, String)> = api.wraps.lock().unwrap().iter().map(|(r, e, to)| (r.clone(), u64::from(*e), to.clone())).collect();
            wraps.sort();
            let mut want: Vec<(String, u64, String)> = case["wraps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| (w["rootId"].as_str().unwrap().into(), w["epoch"].as_u64().unwrap(), w["to"].as_str().unwrap().into()))
                .collect();
            want.sort();
            assert_eq!(wraps, want, "{name}: wraps");
            for (root, epochs) in case["holds"].as_object().unwrap() {
                for epoch in 1..=3u32 {
                    let wanted = epochs.as_array().unwrap().iter().any(|e| e.as_u64() == Some(u64::from(epoch)));
                    assert_eq!(holder.keyring().secret(root, epoch).is_some(), wanted, "{name}: {root} epoch {epoch}");
                }
            }
            // The foreign row a lying server handed out is never held.
            for root in held.keys() {
                assert!(holder.keyring().secret(root, 9).is_none(), "{name}: a foreign wrap was kept");
            }
        }
    }

    // ── #EXP-1950: wrap-signature.json, a key API that VERIFIES like the server ──

    fn signature_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/wrap-signature.json")).unwrap()
    }

    const ROOT: &str = "01234567-89ab-4def-8123-456789abcdef";
    const OTHER_ROOT: &str = "11111111-1111-4111-8111-111111111111";

    /// One stored wrap: `session_keys` with its #3394 columns.
    #[derive(Clone, Debug)]
    struct Row {
        root: String,
        epoch: u32,
        install: String,
        wrapped: String,
        signature: Option<String>,
        signer: Option<String>,
    }

    /// The server's key rules with the signature checks of #3394 and #3408:
    /// a signature must be the CALLER's install's over (root, epoch, the
    /// STORED tree public key, the target's endpoint id, the wrapped bytes),
    /// else the write is refused (BAD_REQUEST). `resign` follows `resign.order`.
    #[derive(Default)]
    struct SignedKeys {
        /// install id → (endpoint id, X25519 public).
        installs: HashMap<String, (String, [u8; 32])>,
        /// Current readers of every root.
        readers: Mutex<Vec<String>>,
        rotators: Vec<String>,
        roots: Mutex<HashMap<String, (u32, String)>>,
        rows: Mutex<Vec<Row>>,
        /// The unsigned rows from before #EXP-1950 ([`SignedKeys::legacy`]).
        legacy_wraps: Mutex<Vec<String>>,
        /// Every wrapped value written with a signature.
        signed: Mutex<Vec<String>>,
        /// `resign` answers this status instead (an old server: 404).
        resign_status: Option<u16>,
        /// Roots whose `resign` the server refuses (FORBIDDEN, the caller no reader).
        resign_refuses: Vec<String>,
        resign_calls: Mutex<Vec<Vec<ResignWrap>>>,
        /// `pending` from a server older than #3394: no target endpoint.
        old_pending: bool,
    }

    /// `resignOutcome` (wrap-signature.ts), replayed against `resign.cases`.
    fn resign_rule(caller_reader: bool, current_epoch: bool, target_reader: bool, signature_valid: bool, row: &str) -> (&'static str, &'static str) {
        let rule = if !caller_reader {
            "callerNotReader"
        } else if !current_epoch {
            "staleEpoch"
        } else if !target_reader {
            "targetNotReader"
        } else if !signature_valid {
            "badSignature"
        } else if row == "none" {
            "noRow"
        } else if row == "signed" {
            "signedRow"
        } else {
            "unsignedRow"
        };
        let outcome = match rule {
            "callerNotReader" | "targetNotReader" | "badSignature" => "refused",
            "unsignedRow" => "replaced",
            _ => "skipped",
        };
        (rule, outcome)
    }

    impl SignedKeys {
        fn new(installs: &[(&str, &Identity)], readers: &[&str]) -> SignedKeys {
            SignedKeys {
                installs: installs
                    .iter()
                    .map(|(id, identity)| (id.to_string(), (endpoint_id_hex(identity), PushKeypair::derive(identity).public())))
                    .collect(),
                readers: Mutex::new(readers.iter().map(|r| r.to_string()).collect()),
                ..Default::default()
            }
        }

        /// The server's check of one signature (`checkWrapSignatures`).
        fn verifies(&self, caller: &str, root: &str, epoch: u32, target: &str, wrapped: &str, signature: &str) -> bool {
            let Some((public, stored_epoch)) = self.roots.lock().unwrap().get(root).map(|(e, p)| (p.clone(), *e)) else { return false };
            if stored_epoch != epoch {
                return false;
            }
            let statement = WrapStatement {
                root_id: root,
                epoch,
                tree_public_key: &public,
                recipient_endpoint_id: &self.installs[target].0,
                wrapped,
            };
            enet_core::wrap_sig::verify_wrap_signature(&statement, signature, &self.installs[caller].0)
        }

        fn bad_signature() -> ApiError {
            ApiError::Http { status: 400, message: "A wrap's signature is not your install's over this key".into() }
        }

        fn store(&self, caller: &str, root: &str, epoch: u32, target: &str, wrapped: &str, signature: Option<&str>) {
            if signature.is_some() {
                self.signed.lock().unwrap().push(wrapped.to_string());
            }
            self.rows.lock().unwrap().push(Row {
                root: root.into(),
                epoch,
                install: target.into(),
                wrapped: wrapped.into(),
                signature: signature.map(str::to_string),
                signer: signature.map(|_| self.installs[caller].0.clone()),
            });
        }

        /// What `sessions.send {seal}` stores: the root and its UNSIGNED wraps.
        fn store_seal(&self, root: &str, seal: &NewRootSeal) {
            self.roots.lock().unwrap().insert(root.into(), (seal.epoch, seal.public_key.clone()));
            for wrap in &seal.wraps {
                self.store(&seal.install_id, root, seal.epoch, &wrap.install_id, &wrap.wrapped, None);
            }
        }

        /// A row from before #EXP-1950: an unsigned wrap of `secret`.
        fn legacy(&self, root: &str, epoch: u32, secret: &[u8; 32], targets: &[&str]) {
            let public = b64url_encode(&PushKeypair::from_secret(*secret).public());
            self.roots.lock().unwrap().insert(root.into(), (epoch, public));
            for target in targets {
                let wrapped = wrap_tree_key(&self.installs[*target].1, secret).unwrap();
                self.legacy_wraps.lock().unwrap().push(wrapped.clone());
                self.store("", root, epoch, target, &wrapped, None);
            }
        }

        /// Every stored row of (root, epoch), checked as a reader would: the
        /// signer's signature over the statement IT builds from the secret the
        /// row opens to, its own endpoint id and the bytes.
        fn assert_signed(&self, root: &str, epoch: u32, signer: &Identity, secrets: &HashMap<String, [u8; 32]>) -> usize {
            let rows: Vec<Row> = self.rows.lock().unwrap().iter().filter(|r| r.root == root && r.epoch == epoch).cloned().collect();
            for row in &rows {
                let secret = unwrap_tree_key(&secrets[&row.install], &row.wrapped).expect("the wrap opens for its recipient");
                let statement = WrapStatement {
                    root_id: root,
                    epoch,
                    tree_public_key: &b64url_encode(&PushKeypair::from_secret(secret).public()),
                    recipient_endpoint_id: &self.installs[&row.install].0,
                    wrapped: &row.wrapped,
                };
                let signature = row.signature.as_deref().unwrap_or_else(|| panic!("{root} epoch {epoch} for {} is unsigned", row.install));
                assert_eq!(row.signer.as_deref(), Some(endpoint_id_hex(signer).as_str()));
                assert!(enet_core::wrap_sig::verify_wrap_signature(&statement, signature, &endpoint_id_hex(signer)));
                // The same signature names exactly this row.
                let other = WrapStatement { epoch: epoch + 1, ..statement };
                assert!(!enet_core::wrap_sig::verify_wrap_signature(&other, signature, &endpoint_id_hex(signer)));
            }
            rows.len()
        }
    }

    impl KeyApi for SignedKeys {
        fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
            {
                let mut roots = self.roots.lock().unwrap();
                match roots.get(&input.root_id) {
                    None if input.epoch == 1 => {}
                    Some((epoch, _)) if input.epoch == epoch + 1 && self.rotators.contains(&input.install_id) => {}
                    _ => return Err(ApiError::Http { status: 409, message: "keyed".into() }),
                }
                // Verified against the key this create stores.
                let previous = roots.insert(input.root_id.clone(), (input.epoch, input.public_key.clone()));
                drop(roots);
                for wrap in &input.wraps {
                    if let Some(signature) = &wrap.signature {
                        if !self.verifies(&input.install_id, &input.root_id, input.epoch, &wrap.install_id, &wrap.wrapped, signature) {
                            let mut roots = self.roots.lock().unwrap();
                            match previous {
                                Some(previous) => roots.insert(input.root_id.clone(), previous),
                                None => roots.remove(&input.root_id),
                            };
                            return Err(Self::bad_signature());
                        }
                    }
                }
            }
            for wrap in &input.wraps {
                self.store(&input.install_id, &input.root_id, input.epoch, &wrap.install_id, &wrap.wrapped, wrap.signature.as_deref());
            }
            Ok(())
        }
        fn mine(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
            let rows: Vec<HeldKey> = self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.install == install_id && root_ids.is_none_or(|ids| ids.contains(&r.root)))
                .map(|r| HeldKey { root_id: r.root.clone(), epoch: r.epoch, wrapped: r.wrapped.clone(), signature: r.signature.clone(), signer_endpoint_id: r.signer.clone() })
                .collect();
            Ok(rows)
        }
        fn pending(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
            let rows = self.rows.lock().unwrap().clone();
            let roots = self.roots.lock().unwrap().clone();
            let readers = self.readers.lock().unwrap().clone();
            let mut out = Vec::new();
            for (root, (epoch, public)) in roots.iter().filter(|(root, _)| root_ids.is_none_or(|ids| ids.contains(root))) {
                if !rows.iter().any(|r| &r.root == root && r.epoch == *epoch && r.install == install_id) {
                    continue;
                }
                for reader in &readers {
                    if !rows.iter().any(|r| &r.root == root && r.epoch == *epoch && &r.install == reader) {
                        out.push(PendingWrap {
                            root_id: root.clone(),
                            epoch: *epoch,
                            public_key: public.clone(),
                            target_install_id: reader.clone(),
                            target_x25519_public: b64url_encode(&self.installs[reader].1),
                            target_endpoint_id: (!self.old_pending).then(|| self.installs[reader].0.clone()),
                        });
                    }
                }
            }
            Ok(out)
        }
        fn wrap(&self, install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
            for wrap in wraps {
                if let Some(signature) = &wrap.signature {
                    if !self.verifies(install_id, &wrap.root_id, wrap.epoch, &wrap.target_install_id, &wrap.wrapped, signature) {
                        return Err(Self::bad_signature());
                    }
                }
            }
            // Like the server: a pair another pass wrapped meanwhile is skipped.
            for wrap in wraps {
                let known = self.rows.lock().unwrap().iter().any(|r| r.root == wrap.root_id && r.epoch == wrap.epoch && r.install == wrap.target_install_id);
                if !known {
                    self.store(install_id, &wrap.root_id, wrap.epoch, &wrap.target_install_id, &wrap.wrapped, wrap.signature.as_deref());
                }
            }
            Ok(())
        }
        fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
            Ok(self
                .roots
                .lock()
                .unwrap()
                .iter()
                .filter(|(root, _)| root_ids.contains(root))
                .map(|(root, (epoch, public))| RootKey { root_id: root.clone(), epoch: *epoch, public_key: public.clone() })
                .collect())
        }
        fn resign(&self, install_id: &str, wraps: &[ResignWrap]) -> Result<ResignOutput, ApiError> {
            self.resign_calls.lock().unwrap().push(wraps.to_vec());
            if let Some(status) = self.resign_status {
                return Err(ApiError::Http { status, message: "refused".into() });
            }
            let readers = self.readers.lock().unwrap().clone();
            let mut replace = Vec::new();
            let mut skipped = 0;
            for wrap in wraps {
                let current = self.roots.lock().unwrap().get(&wrap.root_id).is_some_and(|(epoch, _)| *epoch == wrap.epoch);
                let row = self
                    .rows
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|r| r.root == wrap.root_id && r.epoch == wrap.epoch && r.install == wrap.install_id)
                    .map_or("none", |r| if r.signature.is_some() { "signed" } else { "unsigned" });
                let valid = current && self.verifies(install_id, &wrap.root_id, wrap.epoch, &wrap.install_id, &wrap.wrapped, &wrap.signature);
                let caller = readers.contains(&install_id.to_string()) && !self.resign_refuses.contains(&wrap.root_id);
                match resign_rule(caller, current, readers.contains(&wrap.install_id), valid, row) {
                    ("badSignature", _) => return Err(Self::bad_signature()),
                    (_, "refused") => return Err(ApiError::Http { status: 403, message: "Not a reader of this tree".into() }),
                    (_, "replaced") => replace.push(wrap.clone()),
                    _ => skipped += 1,
                }
            }
            let mut rows = self.rows.lock().unwrap();
            for wrap in &replace {
                let row = rows.iter_mut().find(|r| r.root == wrap.root_id && r.epoch == wrap.epoch && r.install == wrap.install_id).unwrap();
                row.wrapped = wrap.wrapped.clone();
                row.signature = Some(wrap.signature.clone());
                row.signer = Some(self.installs[install_id].0.clone());
                self.signed.lock().unwrap().push(wrap.wrapped.clone());
            }
            Ok(ResignOutput { replaced: replace.len() as u32, skipped })
        }
    }

    /// A signing holder over the double, plus every install's X25519 secret.
    struct Signed {
        api: Arc<SignedKeys>,
        ids: HashMap<String, Identity>,
        secrets: HashMap<String, [u8; 32]>,
    }

    impl Signed {
        fn new(installs: &[&str], readers: &[&str]) -> Signed {
            let ids: HashMap<String, Identity> = installs.iter().map(|id| (id.to_string(), Identity::generate())).collect();
            let pairs: Vec<(&str, &Identity)> = ids.iter().map(|(id, identity)| (id.as_str(), identity)).collect();
            let api = Arc::new(SignedKeys::new(&pairs, readers));
            let secrets = ids.iter().map(|(id, identity)| (id.clone(), PushKeypair::derive(identity).secret())).collect();
            Signed { api, ids, secrets }
        }
        fn holder(&self, install: &str) -> KeyHolder {
            let holder = KeyHolder::signing(install.into(), &self.ids[install], self.api.clone(), Keyring::in_memory());
            let installs = self.api.installs.clone();
            holder.set_install_lookup(Arc::new(move |id: &str| installs.get(id).map(|(endpoint, x)| (endpoint.clone(), b64url_encode(x)))));
            holder
        }
    }

    /// The double checks like the server: the fixture's vector verifies,
    /// a tampered one does not.
    #[test]
    fn the_double_verifies_the_fixture_vector() {
        let fx = signature_fixture();
        let v = &fx["vector"];
        let statement = WrapStatement {
            root_id: v["rootId"].as_str().unwrap(),
            epoch: v["epoch"].as_u64().unwrap() as u32,
            tree_public_key: v["treePublicKey"].as_str().unwrap(),
            recipient_endpoint_id: v["recipientEndpointId"].as_str().unwrap(),
            wrapped: v["wrapped"].as_str().unwrap(),
        };
        let signer = fx["signer"]["endpointId"].as_str().unwrap();
        assert!(enet_core::wrap_sig::verify_wrap_signature(&statement, v["signature"].as_str().unwrap(), signer));
        let identity = Identity::from_bytes(hex32(fx["signer"]["identity"].as_str().unwrap()));
        assert_eq!(sign_wrap(&identity, &statement).unwrap(), v["signature"].as_str().unwrap());
        assert!(!enet_core::wrap_sig::verify_wrap_signature(&statement, v["signature"].as_str().unwrap(), fx["other"]["endpointId"].as_str().unwrap()));
        let tampered = WrapStatement { epoch: 2, ..statement };
        assert!(!enet_core::wrap_sig::verify_wrap_signature(&tampered, v["signature"].as_str().unwrap(), signer));
    }

    /// `resign.cases`: the double's `resign` follows the same order.
    #[test]
    fn resign_cases_replay() {
        let fx = signature_fixture();
        let cases = fx["resign"]["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let b = |k: &str| case[k].as_bool().unwrap();
            let (rule, outcome) = resign_rule(b("callerReader"), b("currentEpoch"), b("targetReader"), b("signatureValid"), case["row"].as_str().unwrap());
            assert_eq!((rule, outcome), (case["rule"].as_str().unwrap(), case["outcome"].as_str().unwrap()), "{}", case["name"]);
            assert_eq!(fx["resign"]["outcomes"][rule], outcome);
        }
        assert_eq!(fx["resign"]["maxWraps"].as_u64().unwrap() as usize, RESIGN_MAX);
        assert_eq!(fx["resign"]["procedure"], "sessionKeys.resign");
    }

    /// `resign.failureCases`: what the holder does after a failed `resign`.
    #[test]
    fn resign_failure_cases_replay() {
        let fx = signature_fixture();
        for case in fx["resign"]["failureCases"].as_array().unwrap() {
            let status = match case["code"].as_str() {
                None => None,
                Some("NOT_FOUND") => Some(404),
                Some("BAD_REQUEST") => Some(400),
                Some("FORBIDDEN") => Some(403),
                Some("INTERNAL_SERVER_ERROR") => Some(500),
                Some(other) => panic!("unknown code {other}"),
            };
            let want = match case["after"].as_str().unwrap() {
                "notYet" => ResignAfter::NotYet,
                "refused" => ResignAfter::Refused,
                "retry" => ResignAfter::Retry,
                other => panic!("unknown after {other}"),
            };
            assert_eq!(resign_after(status), want, "{}", case["name"]);
        }
    }

    /// The first key, the pending answers and a rotation: every wrap the
    /// holder makes is signed by its install for the right (root, epoch,
    /// tree key, recipient, bytes), and the double accepted each one.
    #[test]
    fn create_pending_and_rotation_wraps_are_signed() {
        let mut world = Signed::new(&["device", "phone", "laptop"], &["device", "phone", "laptop"]);
        Arc::get_mut(&mut world.api).unwrap().rotators = vec!["device".into()];
        let device = world.holder("device");
        let (epoch, _) = device.ensure_key(ROOT).unwrap();
        assert_eq!(epoch, 1);
        // The create's own pending pass runs on a thread beside this one.
        device.answer_pending().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(world.api.assert_signed(ROOT, 1, &world.ids["device"], &world.secrets), 3);

        // The laptop is dropped; the device rotates: epoch 2, signed again.
        world.api.readers.lock().unwrap().retain(|r| r != "laptop");
        assert_eq!(device.rotate_root(ROOT, 1), Ok(true));
        device.answer_pending().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(world.api.assert_signed(ROOT, 2, &world.ids["device"], &world.secrets), 2);

        // A reader's own pass over the signed rows: nothing to sign again.
        let phone = world.holder("phone");
        phone.pass();
        assert!(world.api.resign_calls.lock().unwrap().is_empty());
        assert!(phone.keyring().secret(ROOT, 2).is_some());
    }

    /// An older server's `pending` names no endpoint: the wraps go unsigned
    /// (the additive field), and nothing fails.
    #[test]
    fn an_old_servers_pending_is_answered_unsigned() {
        let mut world = Signed::new(&["device", "phone"], &["device", "phone"]);
        Arc::get_mut(&mut world.api).unwrap().old_pending = true;
        let device = world.holder("device");
        device.ensure_key(ROOT).unwrap();
        device.answer_pending().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let rows = world.api.rows.lock().unwrap().clone();
        let phone = rows.iter().find(|r| r.install == "phone").unwrap();
        assert_eq!(phone.signature, None);
        assert!(rows.iter().find(|r| r.install == "device").unwrap().signature.is_some());
    }

    /// Decision (c): a key held from before stays; each holder signs its own
    /// unsigned row of the current epoch again, once, with a FRESH wrap
    /// (never the bytes it was served); signed rows are never touched.
    #[test]
    fn held_keys_are_signed_again_once_per_root() {
        let world = Signed::new(&["device", "phone"], &["device", "phone"]);
        let secret = [7u8; 32];
        world.api.legacy(ROOT, 1, &secret, &["device", "phone"]);
        world.api.legacy(OTHER_ROOT, 1, &[8u8; 32], &["device"]);
        let device = world.holder("device");
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 1, "one batch for both roots");
        assert_eq!(world.api.resign_calls.lock().unwrap()[0].len(), 2);
        let rows = world.api.rows.lock().unwrap().clone();
        assert!(rows.iter().filter(|r| r.install == "device").all(|r| r.signature.is_some()));
        assert_eq!(rows.iter().find(|r| r.install == "phone").unwrap().signature, None, "another reader's row is its own holder's");
        // Again: nothing to ask.
        device.pass();
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 1);
        // The phone signs its own.
        let phone = world.holder("phone");
        phone.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 2);
        let rows = world.api.rows.lock().unwrap().clone();
        let mine = rows.iter().find(|r| r.install == "phone" && r.root == ROOT).unwrap();
        assert_eq!(mine.signer.as_deref(), Some(endpoint_id_hex(&world.ids["phone"]).as_str()));
        assert_eq!(unwrap_tree_key(&world.secrets["phone"], &mine.wrapped).unwrap(), secret);
        // Never signed a wrap it did not make: none of the bytes it was served.
        let legacy = world.api.legacy_wraps.lock().unwrap().clone();
        assert_eq!(legacy.len(), 3);
        assert!(world.api.signed.lock().unwrap().iter().all(|w| !legacy.contains(w)));
    }

    /// A new root's inline seal goes unsigned (its root id is unknown while
    /// sealing); once adopted the holder signs every wrap of it again.
    #[test]
    fn a_new_roots_seal_is_signed_once_the_root_exists() {
        let world = Signed::new(&["device", "phone"], &["device", "phone"]);
        let device = world.holder("device");
        let phone_x = b64url_encode(&world.api.installs["phone"].1);
        let seal = sealing::seal_new_root("device", &device.install_public().unwrap(), &[("phone".into(), phone_x)]).unwrap();
        assert!(seal.wraps.iter().all(|w| w.signature.is_none()));
        world.api.store_seal(ROOT, &seal);
        device.adopt(ROOT, &seal);
        assert_eq!(device.resign_due(), 2);
        assert_eq!(world.api.assert_signed(ROOT, 1, &world.ids["device"], &world.secrets), 2);
        let rows = world.api.rows.lock().unwrap().clone();
        assert!(rows.iter().all(|r| seal.wraps.iter().all(|w| w.wrapped != r.wrapped)), "fresh wraps, never the seal's bytes");
        assert_eq!(device.resign_due(), 0);
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 1);
    }

    /// A server without `resign` (404): quiet, and not asked again soon.
    #[test]
    fn an_old_server_without_resign_is_left_alone() {
        let mut world = Signed::new(&["device"], &["device"]);
        Arc::get_mut(&mut world.api).unwrap().resign_status = Some(404);
        world.api.legacy(ROOT, 1, &[7u8; 32], &["device"]);
        let device = world.holder("device");
        device.pass();
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 1);
        assert!(device.keyring().secret(ROOT, 1).is_some());
        // A blip is the next pass's.
        let mut world = Signed::new(&["device"], &["device"]);
        Arc::get_mut(&mut world.api).unwrap().resign_status = Some(502);
        world.api.legacy(ROOT, 1, &[7u8; 32], &["device"]);
        let device = world.holder("device");
        device.pass();
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 2);
    }

    /// A refused batch: its roots go one per call, the refused one never again.
    #[test]
    fn a_refused_batch_goes_root_by_root() {
        let mut world = Signed::new(&["device"], &["device"]);
        Arc::get_mut(&mut world.api).unwrap().resign_refuses = vec![OTHER_ROOT.into()];
        world.api.legacy(ROOT, 1, &[7u8; 32], &["device"]);
        world.api.legacy(OTHER_ROOT, 1, &[8u8; 32], &["device"]);
        let device = world.holder("device");
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 1);
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 3, "each root alone");
        let rows = world.api.rows.lock().unwrap().clone();
        assert!(rows.iter().find(|r| r.root == ROOT).unwrap().signature.is_some());
        assert!(rows.iter().find(|r| r.root == OTHER_ROOT).unwrap().signature.is_none());
        device.pass();
        assert_eq!(world.api.resign_calls.lock().unwrap().len(), 3);
    }

    /// A holder without an identity signs nothing (and asks nothing).
    #[test]
    fn a_holder_without_an_identity_signs_nothing() {
        let world = Signed::new(&["device", "phone"], &["device", "phone"]);
        let device = KeyHolder::new("device".into(), &PushKeypair::derive(&world.ids["device"]), world.api.clone(), Keyring::in_memory());
        device.ensure_key(ROOT).unwrap();
        device.answer_pending().unwrap();
        device.pass();
        assert!(world.api.rows.lock().unwrap().iter().all(|r| r.signature.is_none()));
        assert!(world.api.resign_calls.lock().unwrap().is_empty());
    }
}
