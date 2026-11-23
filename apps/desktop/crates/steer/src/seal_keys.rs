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
//! - **Rotation** (#FEED-177): a tree that turned private
//!   (`sessions.seal_rotate_epoch` set, its teammates' wraps dropped) gets
//!   a fresh key at `epoch + 1` from the first holder pass of an install
//!   that holds its current secret and may rotate (the starter's installs
//!   or the executing device; the server decides, a refusal is
//!   remembered). It is created exactly as a first key (own wrap, the
//!   current readers through `pending`), so the dropped installs never get
//!   it; old epochs stay openable for old content. A `CONFLICT` = another
//!   holder rotated first: re-read the key.
//!
//! Installed process-wide (`coding::sealing::install`) once the install is
//! registered; the engine's replies, the pickup, the brief and the device
//! MCP gateway all seal and open through it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::session_keys::{CreateKey, HeldKey, InstallWrap, PairWrap, PendingWrap, RequestedKey, RootKey};
use coding::sealing::{self, CreateFailure, KeylessStep, Keyring, RequestKeyAnswer, SealError, SealService};
use enet_core::sealed::{generate_tree_key, public_key_from_b64, unwrap_tree_key, wrap_tree_key};
use enet_core::PushKeypair;

/// How often a holder answers `pending` (and re-reads `mine`) while online.
pub const HOLDER_PASS: Duration = Duration::from_secs(60);
/// EXP-1472: the closest two woken passes run (a `check_in` nudge wakes one;
/// nudges come with every piece of new work).
pub const WAKE_SPACING: Duration = Duration::from_secs(5);

/// The `sessionKeys.*` calls the holder makes: tRPC in the apps, a fake in
/// the tests. BLOCKING.
pub trait KeyApi: Send + Sync + 'static {
    fn create(&self, input: &CreateKey) -> Result<(), ApiError>;
    fn mine(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError>;
    fn pending(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError>;
    fn wrap(&self, install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError>;
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError>;
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
}

/// Agent-to-agent delivery over eNet (`enet.rs` sets it once the node
/// runs): `(target install, sealed message row)`.
pub type DirectSender = Arc<dyn Fn(&coding::sealing::DeliverTo, &domain::rows::MessageRow) + Send + Sync>;
/// #EXP-1483: a device's installs `(install_id, x25519_public)`, by its
/// `devices.device_id`.
pub type DeviceReaders = Arc<dyn Fn(&str) -> Vec<(String, String)> + Send + Sync>;

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
}

impl KeyHolder {
    pub fn new(install_id: String, push: &PushKeypair, api: Arc<dyn KeyApi>, keyring: impl Into<Arc<Keyring>>) -> KeyHolder {
        KeyHolder {
            inner: Arc::new(HolderInner {
                install_id,
                install_secret: push.secret(),
                install_public: push.public(),
                api,
                keyring: keyring.into(),
                creating: Mutex::new(()),
                refused: Mutex::default(),
                stopped: AtomicBool::new(false),
                direct: Mutex::new(None),
                device_readers: Mutex::new(None),
                wake: Mutex::new(None),
                woken_at: Mutex::new(None),
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

    /// Unwraps every held key not cached yet. Returns how many were new.
    /// BLOCKING.
    pub fn pull(&self, root_ids: Option<&[String]>) -> Result<usize, ApiError> {
        let held = self.inner.api.mine(&self.inner.install_id, root_ids)?;
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
                Ok(wrapped) => wraps.push(PairWrap {
                    root_id: pair.root_id,
                    epoch: pair.epoch,
                    target_install_id: pair.target_install_id,
                    wrapped,
                }),
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
        let input = CreateKey {
            root_id: root_id.to_string(),
            install_id: self.inner.install_id.clone(),
            epoch,
            public_key: key.public_key(),
            wraps: vec![InstallWrap { install_id: self.inner.install_id.clone(), wrapped: own }],
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

    /// One holder pass: pull, rotate what is flagged, then answer. BLOCKING.
    pub fn pass(&self) {
        if let Err(err) = self.pull(None) {
            log::debug!("seal-keys: sessionKeys.mine failed: {err}");
        }
        self.rotate_due();
        if let Err(err) = self.answer_pending() {
            log::debug!("seal-keys: sessionKeys.pending/wrap failed: {err}");
        }
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
                .map(|(root, epoch, _, wrapped)| HeldKey { root_id: root.clone(), epoch: *epoch, wrapped: wrapped.clone() })
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
                    rows.push(HeldKey { root_id: root.clone(), epoch: 9, wrapped: self.fixture["rotation"]["foreignWrap"].as_str().unwrap().into() });
                }
            }
            for (root, epoch) in self.conflicted.lock().unwrap().iter() {
                rows.push(HeldKey { root_id: root.clone(), epoch: *epoch, wrapped: self.fixture["rotation"]["rotatedWrap"].as_str().unwrap().into() });
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
}
