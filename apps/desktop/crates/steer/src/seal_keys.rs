//! EXP-1458: the install's KEY HOLDER, beside its eNet node (`enet.rs`).
//! Sealed session content (`packages/domain-contract/fixtures/sealed-content.json`)
//! needs every reader install to hold its trees' secrets; this is the
//! desktop app's and the daemon's half:
//!
//! - **Cache**: the tree secrets this install unwrapped live in
//!   `coding::sealing::Keyring`, a 0600 file beside the eNet identity
//!   (`seal-keys` / `cli-seal-keys`), per account.
//! - **Holder loop**: on start and every [`HOLDER_PASS`]: `sessionKeys.mine`
//!   (unwrap what is new with the install's X25519 secret, derived from the
//!   eNet identity) then `sessionKeys.pending` → `sessionKeys.wrap` for
//!   every listed reader. Automatic, no approval (the #EXP-1457 decision).
//! - **Writers**: [`KeyHolder::ensure_key`] = the root's current key, else
//!   a fresh tree key `sessionKeys.create`d with this install's own wrap
//!   (the device claiming a server-started session); the other readers get
//!   theirs from the next `pending` pass, which runs at once.
//!
//! Installed process-wide (`coding::sealing::install`) once the install is
//! registered; the engine's replies, the pickup, the brief and the device
//! MCP gateway all seal and open through it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::session_keys::{CreateKey, HeldKey, InstallWrap, PairWrap, PendingWrap, RootKey};
use coding::sealing::{self, Keyring, SealService};
use enet_core::sealed::{generate_tree_key, public_key_from_b64, unwrap_tree_key, wrap_tree_key};
use enet_core::PushKeypair;

/// How often a holder answers `pending` (and re-reads `mine`) while online.
pub const HOLDER_PASS: Duration = Duration::from_secs(60);

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
    keyring: Keyring,
    /// Serializes key creation (two writers of one root in this process).
    creating: Mutex<()>,
    stopped: AtomicBool,
    direct: Mutex<Option<DirectSender>>,
}

impl KeyHolder {
    pub fn new(install_id: String, push: &PushKeypair, api: Arc<dyn KeyApi>, keyring: Keyring) -> KeyHolder {
        KeyHolder {
            inner: Arc::new(HolderInner {
                install_id,
                install_secret: push.secret(),
                install_public: push.public(),
                api,
                keyring,
                creating: Mutex::new(()),
                stopped: AtomicBool::new(false),
                direct: Mutex::new(None),
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

    /// One holder pass: pull, then answer. BLOCKING.
    pub fn pass(&self) {
        if let Err(err) = self.pull(None) {
            log::debug!("seal-keys: sessionKeys.mine failed: {err}");
        }
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

    fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), String> {
        if let Some(current) = self.inner.keyring.current(root_id) {
            return Ok(current);
        }
        let _creating = lock(&self.inner.creating);
        if let Some(current) = self.inner.keyring.current(root_id) {
            return Ok(current);
        }
        if let Some(found) = self.lookup(root_id)? {
            return Ok(found);
        }
        // The root has no key: this install is its first content writer.
        let key = generate_tree_key();
        let own = wrap_tree_key(&self.inner.install_public, &key.secret).map_err(|e| e.to_string())?;
        let input = CreateKey {
            root_id: root_id.to_string(),
            install_id: self.inner.install_id.clone(),
            epoch: 1,
            public_key: key.public_key(),
            wraps: vec![InstallWrap { install_id: self.inner.install_id.clone(), wrapped: own }],
        };
        match self.inner.api.create(&input) {
            Ok(()) => {
                self.inner.keyring.insert_secret(root_id, 1, key.secret);
                self.inner.keyring.set_current(root_id, 1, key.public);
                log::info!("seal-keys: created the tree key of {root_id}");
                // The other readers (the starter's installs, the team when
                // shared) get their wraps from this install's pending pass.
                let holder = self.clone();
                std::thread::spawn(move || {
                    if let Err(err) = holder.answer_pending() {
                        log::debug!("seal-keys: first pending pass failed: {err}");
                    }
                });
                Ok((1, key.public))
            }
            // Another writer keyed it first: use theirs.
            Err(ApiError::Http { status: 409, .. }) => {
                self.lookup(root_id)?.ok_or_else(|| "the tree was keyed concurrently but has no key".to_string())
            }
            Err(err) => Err(err.to_string()),
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

    fn deliver_direct(&self, deliver_to: &coding::sealing::DeliverTo, message: &domain::rows::MessageRow) {
        let sender = lock(&self.inner.direct).clone();
        if let Some(sender) = sender {
            sender(deliver_to, message);
        }
    }
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
        readers: Vec<(String, [u8; 32])>,
        roots: Mutex<HashMap<String, (u32, String)>>,
        wraps: Mutex<Vec<(String, u32, String, String)>>,
    }

    impl KeyApi for FakeKeys {
        fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
            let mut roots = self.roots.lock().unwrap();
            if roots.contains_key(&input.root_id) {
                return Err(ApiError::Http { status: 409, message: "keyed".into() });
            }
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
                for (reader, x) in &self.readers {
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
            for wrap in wraps {
                self.wraps.lock().unwrap().push((wrap.root_id.clone(), wrap.epoch, wrap.target_install_id.clone(), wrap.wrapped.clone()));
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
        let api = Arc::new(FakeKeys {
            readers: vec![("device".into(), device.public()), ("phone".into(), phone.public())],
            ..Default::default()
        });
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
}
