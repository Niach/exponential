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
use api::session_keys::{CreateKey, HeldKey, InstallWrap, PairWrap, PendingWrap, RootKey};
use coding::sealing::{self, Keyring, SealError, SealService};
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
    fn pending(&self, install_id: &str) -> Result<Vec<PendingWrap>, ApiError>;
    fn wrap(&self, install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError>;
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError>;
}

impl KeyApi for api::TrpcClient {
    fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
        api::session_keys::create(self, input)
    }
    fn mine(&self, install_id: &str, root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
        api::session_keys::mine(self, install_id, root_ids)
    }
    fn pending(&self, install_id: &str) -> Result<Vec<PendingWrap>, ApiError> {
        api::session_keys::pending(self, install_id)
    }
    fn wrap(&self, install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
        api::session_keys::wrap(self, install_id, wraps).map(|_| ())
    }
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
        api::session_keys::public_keys(self, root_ids)
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
        let pending = self.inner.api.pending(&self.inner.install_id)?;
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
        match self.inner.api.create(&input) {
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
    }

    impl FakeKeys {
        fn with_readers(readers: Vec<(String, [u8; 32])>) -> FakeKeys {
            FakeKeys { readers: Mutex::new(readers), ..Default::default() }
        }
    }

    impl KeyApi for FakeKeys {
        fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
            *self.attempts.lock().unwrap() += 1;
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
        fn pending(&self, install_id: &str) -> Result<Vec<PendingWrap>, ApiError> {
            let wraps = self.wraps.lock().unwrap();
            let mut out = Vec::new();
            for (root, (epoch, public)) in self.roots.lock().unwrap().iter() {
                if !wraps.iter().any(|(r, e, i, _)| r == root && e == epoch && i == install_id) {
                    continue;
                }
                for (reader, x) in self.readers.lock().unwrap().iter() {
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
            Ok(out)
        }
        fn wrap(&self, _install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
            // Like the server: current readers only, one wrap per pair.
            let readers = self.readers.lock().unwrap();
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
        assert!(api.pending("device").unwrap().is_empty());
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
}
