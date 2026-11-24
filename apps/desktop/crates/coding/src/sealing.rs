//! EXP-1458: sealed session content on the device (the spec is
//! `packages/domain-contract/fixtures/sealed-content.json`, the primitives
//! `enet_core::sealed`). Every session tree has one X25519 key per epoch;
//! message bodies, cards and titles are sealed to it and only reader
//! installs hold the secret.
//!
//! This module is the device's ONE seal/open surface, gpui- and tokio-free:
//! - [`Keyring`]: the tree keys this install holds (secrets by root and
//!   epoch, 0600 file per account), the roots' current public keys and the
//!   session → root map the hosts feed from their synced rows.
//! - [`SealService`]: the network half (fetch wraps, create a tree key,
//!   look up another tree's public key, deliver agent-to-agent over eNet),
//!   implemented by `steer::seal_keys::KeyHolder` and installed process-wide
//!   with [`install`] when the install's eNet node registers.
//! - The free functions ([`seal_body`], [`open_body`], [`seal_card`],
//!   [`open_card`], [`open_message`], [`open_session_title`]) every writer
//!   and reader uses: the engine's replies, the pickup, the brief, the
//!   device MCP gateway and the desktop UI.
//!
//! #EDE-97 (`sealed-content.json` `clearTree`): NO SILENT PLAINTEXT. Every
//! writer seals ([`seal_body_for_session`], [`seal_card_for_session`]);
//! plaintext is the wire ONLY into a clear tree (an outside MCP client's
//! root, `sessions.clear_tree`, [`tree_clear`]). Without a key holder (eNet
//! off, signed out) or a key, a write FAILS ([`SealError`]) and nothing
//! leaves; only plaintext opens then. The pure decision is [`send_wire`].

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use domain::rows::{MessageRow, SessionRow};
use serde_json::Value;

pub use enet_core::sealed::{
    b64url_decode, b64url_encode, generate_tree_key, parse_body_envelope, public_key_from_b64, unwrap_tree_key,
    wrap_tree_key, BodyEnvelope, OpenStatus, OpenedBody, OpenedCard, TreeKey, SEALED_BODY_PREFIX,
};

/// The keyring file beside the install's eNet identity.
pub fn keyring_path(data_dir: &Path, account_id: &str, daemon: bool) -> PathBuf {
    api::seal_keyring_path(data_dir, account_id, daemon)
}

/// A missing secret is re-fetched at most this often per root.
const FETCH_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Default)]
struct State {
    /// root → epoch → tree secret.
    secrets: BTreeMap<String, BTreeMap<u32, [u8; 32]>>,
    /// root → its current (epoch, public key), as last seen.
    publics: HashMap<String, (u32, [u8; 32])>,
    /// session → root (every tree member, roots map to themselves).
    roots: HashMap<String, String>,
    /// #FEED-177: root → the epoch a rotation must supersede
    /// (`sessions.seal_rotate_epoch`, as last seen).
    rotations: HashMap<String, u32>,
}

/// The tree keys this install holds. Cheap to share (`Arc`); every method
/// is non-blocking except the file write on a new secret.
pub struct Keyring {
    state: RwLock<State>,
    path: Option<PathBuf>,
    changed: AtomicU64,
    fetched: Mutex<HashMap<String, Instant>>,
    listeners: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}

impl Default for Keyring {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl Keyring {
    /// A keyring that never touches disk (tests, the no-service fallback).
    pub fn in_memory() -> Self {
        Self { state: RwLock::default(), path: None, changed: AtomicU64::new(0), fetched: Mutex::default(), listeners: Mutex::default() }
    }

    /// The keyring persisted at `path` (0600), loaded now. An unreadable or
    /// malformed file starts empty: every secret is fetched again from its
    /// wrap (`sessionKeys.mine`), nothing is lost.
    pub fn load(path: PathBuf) -> Self {
        let mut state = State::default();
        if let Ok(text) = std::fs::read_to_string(&path) {
            match enet_core::sealed::keyring_secrets(&text) {
                Ok(secrets) => state.secrets = secrets,
                Err(err) => log::warn!("sealing: {} is malformed, starting empty: {err}", path.display()),
            }
        }
        Self {
            state: RwLock::new(state),
            path: Some(path),
            changed: AtomicU64::new(0),
            fetched: Mutex::default(),
            listeners: Mutex::default(),
        }
    }

    /// The file this keyring persists to (`None` = in memory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Rises whenever a secret or a public key arrives: a reader that
    /// showed a locked row re-opens it.
    pub fn changed(&self) -> u64 {
        self.changed.load(Ordering::SeqCst)
    }

    fn bump(&self) {
        self.changed.fetch_add(1, Ordering::SeqCst);
        for listener in self.listeners.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            listener();
        }
    }

    /// Runs `f` (on the thread that stored the key) whenever a key arrives.
    pub fn on_change(&self, f: Box<dyn Fn() + Send + Sync>) {
        self.listeners.lock().unwrap_or_else(|p| p.into_inner()).push(f);
    }

    /// The secret of `root`'s `epoch`, when held.
    pub fn secret(&self, root_id: &str, epoch: u32) -> Option<[u8; 32]> {
        read(&self.state).secrets.get(root_id)?.get(&epoch).copied()
    }

    /// Every epoch this install holds for `root`.
    pub fn epochs(&self, root_id: &str) -> Vec<u32> {
        read(&self.state).secrets.get(root_id).map(|e| e.keys().copied().collect()).unwrap_or_default()
    }

    /// Stores a secret (from a wrap or a key this install created) and
    /// persists the keyring. Returns whether it was new.
    pub fn insert_secret(&self, root_id: &str, epoch: u32, secret: [u8; 32]) -> bool {
        let fresh = {
            let mut state = write(&self.state);
            state.secrets.entry(root_id.to_string()).or_default().insert(epoch, secret) != Some(secret)
        };
        if fresh {
            self.persist();
            self.bump();
        }
        fresh
    }

    /// The root's current key as last seen (`sessions.seal_public_key`).
    pub fn current(&self, root_id: &str) -> Option<(u32, [u8; 32])> {
        read(&self.state).publics.get(root_id).copied()
    }

    /// Records `root`'s current key; an older epoch never replaces a newer.
    pub fn set_current(&self, root_id: &str, epoch: u32, public: [u8; 32]) {
        let mut state = write(&self.state);
        let newer = state.publics.get(root_id).is_none_or(|(known, key)| epoch > *known || (epoch == *known && *key != public));
        if newer {
            state.publics.insert(root_id.to_string(), (epoch, public));
            drop(state);
            self.bump();
        }
    }

    /// The root of `session_id`, when a row naming it was seen.
    pub fn root_of(&self, session_id: &str) -> Option<String> {
        read(&self.state).roots.get(session_id).cloned()
    }

    /// Records a session's root (an MCP result, a launch).
    pub fn note_root(&self, session_id: &str, root_id: &str) {
        if session_id.is_empty() || root_id.is_empty() {
            return;
        }
        write(&self.state).roots.insert(session_id.to_string(), root_id.to_string());
    }

    /// Learns the session → root map and the roots' current keys from the
    /// host's synced `sessions` rows (every pickup pass calls it). Returns
    /// whether a root newly asks for a rotation (#FEED-177).
    pub fn note_sessions(&self, rows: &[SessionRow]) -> bool {
        let mut keys = Vec::new();
        let mut flagged = false;
        {
            let mut state = write(&self.state);
            for row in rows {
                let root = row.root_id.as_deref().filter(|r| !r.is_empty()).unwrap_or(&row.id);
                if state.roots.get(&row.id).map(String::as_str) != Some(root) {
                    state.roots.insert(row.id.clone(), root.to_string());
                }
                if row.is_root() && row.clear_tree {
                    note_clear_tree(&row.id);
                }
                if row.is_root() {
                    match row.seal_rotate_epoch.and_then(|e| u32::try_from(e).ok()) {
                        Some(epoch) => flagged |= state.rotations.insert(row.id.clone(), epoch) != Some(epoch),
                        None => {
                            state.rotations.remove(&row.id);
                        }
                    }
                    if let (Some(key), Some(epoch)) = (row.seal_public_key.as_deref(), row.seal_epoch) {
                        if let (Ok(public), Ok(epoch)) = (public_key_from_b64(key), u32::try_from(epoch)) {
                            keys.push((row.id.clone(), epoch, public));
                        }
                    }
                }
            }
        }
        for (root, epoch, public) in keys {
            self.set_current(&root, epoch, public);
        }
        flagged
    }

    /// #FEED-177: the roots this install should ROTATE now, with the epoch
    /// each supersedes: flagged (`seal_rotate_epoch`), still at that epoch
    /// as far as this install knows, and its secret held (a live reader).
    /// Whether this install MAY rotate (the starter's or the executing
    /// device's) is the server's call (`sessionKeys.create`).
    pub fn rotations_due(&self) -> Vec<(String, u32)> {
        let state = read(&self.state);
        let mut due: Vec<(String, u32)> = state
            .rotations
            .iter()
            .filter(|(root, flagged)| {
                state.publics.get(*root).is_some_and(|(epoch, _)| epoch <= flagged)
                    && state.secrets.get(*root).is_some_and(|held| {
                        state.publics.get(*root).is_some_and(|(epoch, _)| held.contains_key(epoch))
                    })
            })
            .map(|(root, flagged)| (root.clone(), *flagged))
            .collect();
        due.sort();
        due
    }

    /// The flag of `root` is handled (rotated here, or by another writer).
    pub fn rotated(&self, root_id: &str) {
        write(&self.state).rotations.remove(root_id);
    }

    /// Whether a missing secret of `root` may be fetched now (rate limit).
    fn may_fetch(&self, root_id: &str) -> bool {
        let mut fetched = self.fetched.lock().unwrap_or_else(|p| p.into_inner());
        if fetched.get(root_id).is_some_and(|at| at.elapsed() < FETCH_BACKOFF) {
            return false;
        }
        fetched.insert(root_id.to_string(), Instant::now());
        true
    }

    fn persist(&self) {
        let Some(path) = &self.path else { return };
        let doc = {
            let state = read(&self.state);
            let keys: serde_json::Map<String, Value> = state
                .secrets
                .iter()
                .map(|(root, epochs)| {
                    let epochs: serde_json::Map<String, Value> =
                        epochs.iter().map(|(e, s)| (e.to_string(), Value::String(b64url_encode(s)))).collect();
                    (root.clone(), Value::Object(epochs))
                })
                .collect();
            serde_json::json!({ "v": 1, "keys": keys })
        };
        if let Err(err) = write_private(path, &doc.to_string()) {
            log::warn!("sealing: writing {} failed: {err}", path.display());
        }
    }
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|p| p.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|p| p.into_inner())
}

/// Sibling + rename, 0600, the directory 0700 (the eNet identity posture).
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

/// One message delivered agent-to-agent over eNet (`steer::seal_keys`
/// sends it after the server accepted the row; the receiving pickup feeds
/// it at once and dedups by `id` against the synced copy).
pub type DirectMessage = MessageRow;

/// `sessions.send`'s `deliverTo` (contract addendum 3): the install of the
/// device executing the target session.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliverTo {
    pub install_id: String,
    /// Lowercase hex of the install's eNet endpoint id.
    pub endpoint_id: String,
}

/// The network half of sealing, installed process-wide with [`install`].
/// Every method BLOCKS on the network: call off the UI thread.
pub trait SealService: Send + Sync {
    fn keyring(&self) -> &Keyring;
    /// This install's registry id.
    fn install_id(&self) -> Option<String>;
    /// This install's X25519 public key (`PushKeypair::derive(identity)`),
    /// for wrapping a NEW root's key to itself ([`seal_new_root`]): the
    /// device MCP gateway seals an agent's new-session start with it. `None`
    /// = new sessions go out plaintext.
    fn install_public(&self) -> Option<[u8; 32]> {
        None
    }
    /// #EXP-1483: the installs (`(install_id, x25519_public)`) of
    /// `target_device` (the `devices.device_id` a new session will run on),
    /// wrapped into a NEW root's key at creation so its pickup opens the
    /// brief at once. Empty = none known (they wait for `pending`).
    fn new_root_readers(&self, _target_device: &str) -> Vec<(String, String)> {
        Vec::new()
    }
    /// `root`'s current (epoch, public key) for WRITING into it: the cached
    /// key, else `publicKeys`, else a fresh tree key this install creates
    /// (`sessionKeys.create`, epoch 1). [`SealError::NotReader`] = the server
    /// refused the create; [`SealError::KeyMakerWaits`] = this install makes
    /// no key (#EXP-2123: it waits for approval, nothing was created); any
    /// other failure is [`SealError::NoKey`].
    fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), SealError>;
    /// A tree's current key, if it has one (never creates).
    fn public_key(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String>;
    /// Pull this install's wraps for `root` now (`sessionKeys.mine`).
    fn fetch_secrets(&self, root_id: &str);
    /// The root of a session this host never saw a row of (MCP lookup).
    fn resolve_root(&self, _session_id: &str) -> Option<String> {
        None
    }
    /// After the server accepted `message` (sealed, as stored) it returned
    /// `deliverTo` for: deliver it over eNet chat to that install (the
    /// device executing the target session). Fire and forget.
    fn deliver_direct(&self, _deliver_to: &DeliverTo, _message: &DirectMessage) {}
    /// Keeps a new root's secret once `sessions.send {seal}` returned its id
    /// (#EXP-1950: a key holder then re-signs the seal's wraps, `resign`).
    fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
        adopt_into(self.keyring(), root_id, seal);
    }
    /// EXP-1472: run a holder pass soon (a reader may be waiting for a wrap).
    fn wake(&self) {}
    /// #EXP-2123 (install-trust.json `keyMaker`): whether this install MAKES
    /// chat keys (a first key, a new root's inline seal, a rotation): only a
    /// JOINED install does (an older server without `installs.trust` counts
    /// as joined; a store not reconciled yet is reconciled first). BLOCKING.
    fn makes_keys(&self) -> bool {
        true
    }
    /// #EXP-1960: `sessionKeys.requestKey`: a keyless tree this install may
    /// not key asks its devices to key it. Default: no answer (the bare wait).
    fn request_key(&self, _root_id: &str) -> RequestKeyAnswer {
        RequestKeyAnswer::Error
    }
    /// #EXP-1950 (install-trust.json R2 + R3): whether this install may seal
    /// to (root, epoch, public key) as the server named it (`vouch` = the
    /// answer's, when it carried one). Record-only in release 1: refused only
    /// for a rollback, a key that is not the held secret's, or a vouch whose
    /// signature fails. Default: no rule (no key holder with a pin store).
    fn check_seal(&self, _root_id: &str, _epoch: u32, _public: &[u8; 32], _vouch: Option<&Value>) -> Result<(), SealError> {
        Ok(())
    }
}

/// #EXP-1950: [`SealService::check_seal`] of the installed service (none = no rule).
pub fn check_seal(root_id: &str, epoch: u32, public: &[u8; 32], vouch: Option<&Value>) -> Result<(), SealError> {
    match service() {
        Some(service) => service.check_seal(root_id, epoch, public, vouch),
        None => Ok(()),
    }
}

/// #EXP-1950 (install-trust.json): this install's own code while it waits
/// for approval (`None` = trusted, or no rule): the daemon's doctor row and
/// log say "Waiting for approval · <code>".
static INSTALL_WAITING: RwLock<Option<String>> = RwLock::new(None);

pub fn set_install_waiting(code: Option<String>) {
    *write(&INSTALL_WAITING) = code;
}

pub fn install_waiting() -> Option<String> {
    read(&INSTALL_WAITING).clone()
}

static SERVICE: RwLock<Option<Arc<dyn SealService>>> = RwLock::new(None);
/// The install's persisted keyring, loaded from disk at boot ([`preload`])
/// before its key holder runs: the holder needs a registered install and a
/// running eNet node (network), the reader only the secrets it already has.
static PRELOADED: RwLock<Option<Arc<Keyring>>> = RwLock::new(None);
static FALLBACK: std::sync::OnceLock<Keyring> = std::sync::OnceLock::new();

/// #FEED-184: this process WILL have a seal service (its eNet node is
/// starting, the key holder comes once the install registers). Until it
/// does, writers wait for it ([`sealer`]) instead of sending in clear, and
/// launches still route their agents through the MCP gateway.
static EXPECTED: AtomicBool = AtomicBool::new(false);

/// #FEED-192: when the key holder started coming, while it is.
static EXPECTED_SINCE: RwLock<Option<Instant>> = RwLock::new(None);

/// Moves on every install / uninstall: a swapped keyring is a change even
/// when its own counter reads the same as the one before (a keyring loaded
/// from disk starts at 0, like the empty fallback).
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Installs the process's seal service (the key holder, once registered).
pub fn install(service: Arc<dyn SealService>) {
    *write(&SERVICE) = Some(service);
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Removes it (sign-out); nothing seals or opens afterwards.
pub fn uninstall() {
    *write(&SERVICE) = None;
    GENERATION.fetch_add(1, Ordering::SeqCst);
    *HELD_CLAIMS.lock().unwrap_or_else(|p| p.into_inner()) = None;
}

/// The keyring persisted at `path`, loaded now and made the process's
/// reading keyring until a key holder is installed (which then takes THIS
/// keyring: it calls [`preload`] with the same path): sealed rows open from the first frame
/// after a launch instead of once the install registered. Idempotent per
/// path; another path (an account switch) replaces it.
pub fn preload(path: PathBuf) -> Arc<Keyring> {
    if let Some(keyring) = read(&PRELOADED).as_ref().filter(|k| k.path() == Some(path.as_path())) {
        return Arc::clone(keyring);
    }
    let keyring = Arc::new(Keyring::load(path));
    *write(&PRELOADED) = Some(Arc::clone(&keyring));
    GENERATION.fetch_add(1, Ordering::SeqCst);
    keyring
}

/// Drops the preloaded keyring of `path` (sign-out); another path's stays.
pub fn unload(path: &Path) {
    let mut preloaded = write(&PRELOADED);
    if preloaded.as_ref().is_some_and(|k| k.path() == Some(path)) {
        *preloaded = None;
        drop(preloaded);
        GENERATION.fetch_add(1, Ordering::SeqCst);
    }
}

/// The secret of `root`'s `epoch` in the process's reading keyring (the
/// holder's, else the preloaded one): what a UI opens rows with. Never
/// blocks, never fetches.
pub fn cached_secret(root_id: &str, epoch: u32) -> Option<[u8; 32]> {
    with_keyring(|keyring| keyring.secret(root_id, epoch))
}

pub fn service() -> Option<Arc<dyn SealService>> {
    read(&SERVICE).clone()
}

/// #FEED-184: marks (or clears) a key holder as coming: set when the
/// install's eNet node starts, cleared when it stops or can never register.
pub fn expect_service(expected: bool) {
    EXPECTED.store(expected, Ordering::SeqCst);
    *write(&EXPECTED_SINCE) = expected.then(Instant::now);
    if expected {
        *write(&OFF) = None;
    }
}

/// #EXP-1959: why this process's eNet gave up, so no key holder comes:
/// what the doctor names and what `sessions.noKeyHolder` says in a chat
/// whose agent replies can no longer be sealed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HolderOff {
    /// The eNet identity file could not be read or written.
    IdentityUnreadable,
    /// `Node::start` failed.
    NodeFailed,
    /// `installs.register` refused this install for good (twice).
    RegistrationRefused,
    /// The server has no key API.
    NoKeyApi,
    /// eNet was stopped (sign-out).
    Stopped,
}

impl HolderOff {
    /// The wire code (`sessions.noKeyHolder {reason}`).
    pub fn code(self) -> &'static str {
        match self {
            HolderOff::IdentityUnreadable => "identity_unreadable",
            HolderOff::NodeFailed => "node_failed",
            HolderOff::RegistrationRefused => "registration_refused",
            HolderOff::NoKeyApi => "no_key_api",
            HolderOff::Stopped => "stopped",
        }
    }

    /// The doctor's words for it (`device-doctor.json` `keyHolderOff`).
    pub fn detail(self) -> &'static str {
        match self {
            HolderOff::IdentityUnreadable => "its identity file is unreadable",
            HolderOff::NodeFailed => "its node failed to start",
            HolderOff::RegistrationRefused => "the server refused this install",
            HolderOff::NoKeyApi => "the server has no key API",
            HolderOff::Stopped => "signed out",
        }
    }
}

/// #EXP-1959: the reason of the last [`holder_off`], until one is coming again.
static OFF: RwLock<Option<HolderOff>> = RwLock::new(None);

/// #EXP-1959: eNet gave up for `reason`: no key holder comes in this
/// process ([`expect_service`] `false`, plus why).
pub fn holder_off(reason: HolderOff) {
    expect_service(false);
    *write(&OFF) = Some(reason);
}

/// #EXP-1959: why no key holder runs nor comes in this process, when eNet
/// said so ([`holder_off`]); `None` while one runs or is coming.
pub fn holder_off_reason() -> Option<HolderOff> {
    if service().is_some() || EXPECTED.load(Ordering::SeqCst) {
        return None;
    }
    *read(&OFF)
}

/// #FEED-192: this process has no key holder, and none is about to arrive
/// (one still coming after [`SERVICE_WAIT`] counts as missing: an install
/// that cannot register keeps "coming"). What the doctor reports.
pub fn key_holder_missing() -> bool {
    if service().is_some() {
        return false;
    }
    read(&EXPECTED_SINCE).map_or(true, |since| since.elapsed() >= SERVICE_WAIT)
}

/// A seal service is installed, or one is coming ([`expect_service`]).
pub fn service_expected() -> bool {
    EXPECTED.load(Ordering::SeqCst) || service().is_some()
}

/// #FEED-184: how long a writer waits for an expected key holder (a boot
/// that launched sessions before the install registered); #EDE-97:
/// `sealed-content.json` `clearTree.holderWaitMs`.
pub const SERVICE_WAIT: Duration = Duration::from_secs(20);
const SERVICE_POLL: Duration = Duration::from_millis(250);
static SERVICE_WAIT_MS: AtomicU64 = AtomicU64::new(SERVICE_WAIT.as_millis() as u64);

#[doc(hidden)]
pub fn set_service_wait(wait: Duration) {
    SERVICE_WAIT_MS.store(wait.as_millis() as u64, Ordering::SeqCst);
}

/// The service to seal with: the installed one; while one is EXPECTED,
/// waited for up to [`SERVICE_WAIT`] (then [`SealError::NotReady`], never
/// plaintext); [`SealError::NoService`] only when none ever comes.
/// BLOCKING while waiting.
pub fn sealer() -> Result<Arc<dyn SealService>, SealError> {
    if let Some(service) = service() {
        return Ok(service);
    }
    if !EXPECTED.load(Ordering::SeqCst) {
        return Err(SealError::NoService);
    }
    let deadline = Instant::now() + Duration::from_millis(SERVICE_WAIT_MS.load(Ordering::SeqCst));
    loop {
        if let Some(service) = service() {
            return Ok(service);
        }
        if !EXPECTED.load(Ordering::SeqCst) {
            return Err(SealError::NoService);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(SealError::NotReady);
        }
        std::thread::sleep(left.min(SERVICE_POLL));
    }
}

/// #EXP-1959 (b): a claim's keying + re-seal that found no key holder
/// (a boot claiming queued sessions before its install registered), by
/// session id: run by that session's first sealed write, before it posts
/// ([`seal_body_for_session`] / [`seal_card_for_session`]), so the re-seal
/// still lands while no agent reply follows the first message.
type HeldClaim = Box<dyn FnOnce() + Send>;
static HELD_CLAIMS: Mutex<Option<HashMap<String, HeldClaim>>> = Mutex::new(None);

/// Holds `session_id`'s claim work until a key holder runs (a later hold
/// of the same session replaces it).
pub fn hold_claim(session_id: &str, run: HeldClaim) {
    HELD_CLAIMS.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(Default::default).insert(session_id.to_string(), run);
}

/// Whether `session_id`'s claim work still waits for a key holder.
pub fn claim_held(session_id: &str) -> bool {
    HELD_CLAIMS.lock().unwrap_or_else(|p| p.into_inner()).as_ref().is_some_and(|held| held.contains_key(session_id))
}

/// Runs `session_id`'s held claim work once a key holder is up (waited for
/// like any write, [`sealer`]); while none is, the write fails too (it is
/// retried, never sent ahead of the re-seal). BLOCKING.
fn run_held_claim(session_id: &str) -> Result<(), SealError> {
    if !claim_held(session_id) {
        return Ok(());
    }
    sealer()?;
    let run = HELD_CLAIMS.lock().unwrap_or_else(|p| p.into_inner()).as_mut().and_then(|held| held.remove(session_id));
    if let Some(run) = run {
        run();
    }
    Ok(())
}

/// EXP-1472: the server's `check_in` nudge (a new install may wait for its
/// wraps): the key holder's next pass runs now instead of on its cadence.
pub fn wake_holder() {
    if let Some(service) = service() {
        service.wake();
    }
}

/// The installed keyring, else the preloaded one ([`preload`]), else an
/// empty in-memory one.
fn with_keyring<R>(f: impl FnOnce(&Keyring) -> R) -> R {
    if let Some(service) = service() {
        return f(service.keyring());
    }
    let preloaded = read(&PRELOADED).clone();
    match preloaded {
        Some(keyring) => f(&keyring),
        None => f(FALLBACK.get_or_init(Keyring::in_memory)),
    }
}

/// #FEED-185: whether `root` is a keyed (sealed) tree, as far as this
/// process knows (synced rows feed it even without a key holder).
pub fn tree_keyed(root_id: &str) -> bool {
    with_keyring(|keyring| keyring.current(root_id).is_some())
}

/// #EDE-97: the roots known to be CLEAR trees (`sessions.clear_tree`, roots
/// only, never changes): process-global, independent of the key holder (a
/// clear tree needs none).
static CLEAR: RwLock<Option<std::collections::HashSet<String>>> = RwLock::new(None);

/// Records `root_id` as a clear tree (the synced row says `clear_tree`).
pub fn note_clear_tree(root_id: &str) {
    if root_id.is_empty() || tree_clear(root_id) {
        return;
    }
    write(&CLEAR).get_or_insert_with(Default::default).insert(root_id.to_string());
}

/// #EDE-97: `root_id` is a clear tree: plaintext is its legal wire, nobody
/// keys it.
pub fn tree_clear(root_id: &str) -> bool {
    read(&CLEAR).as_ref().is_some_and(|clear| clear.contains(root_id))
}

/// [`Keyring::changed`] of the installed keyring, moved by every service
/// swap too: the persisted keys a holder brings at boot re-open the rows
/// locked before it registered, not its first `sessionKeys.mine` pull.
pub fn changed() -> u64 {
    (GENERATION.load(Ordering::SeqCst) << 40).wrapping_add(with_keyring(Keyring::changed))
}

/// Feeds the synced `sessions` rows (session → root, current keys).
pub fn note_sessions(rows: &[SessionRow]) {
    // #FEED-177: a newly flagged root rotates in the holder's next pass.
    if with_keyring(|keyring| keyring.note_sessions(rows)) {
        wake_holder();
    }
}

/// EXP-1458: the users the host's synced `users` rows name, `(id, email)`:
/// whom an `@email` in an engine reply mentions (the server cannot read a
/// sealed body, so the writer sends `mentionUserIds`). Independent of the
/// key service: plaintext replies name their mentions too.
static USERS: RwLock<Vec<(String, String)>> = RwLock::new(Vec::new());

/// Feeds the synced `users` rows (each pickup pass; replaces the set).
pub fn note_users(users: impl IntoIterator<Item = (String, String)>) {
    let users: Vec<(String, String)> = users.into_iter().collect();
    let mut known = USERS.write().unwrap_or_else(|p| p.into_inner());
    if *known != users {
        *known = users;
    }
}

/// The user ids `text` mentions (`@email`, resolved against [`note_users`];
/// unknown emails dropped, each user once).
pub fn mention_user_ids(text: &str) -> Vec<String> {
    let users = read(&USERS);
    domain::sealed::mention_user_ids(text, users.iter().map(|(id, email)| (id.as_str(), email.as_str())))
}

/// The root of `session_id`: a synced row, else the service's lookup.
pub fn root_of(session_id: &str) -> Option<String> {
    if let Some(root) = with_keyring(|k| k.root_of(session_id)) {
        return Some(root);
    }
    let service = sealer().ok()?;
    let root = service.resolve_root(session_id)?;
    service.keyring().note_root(session_id, &root);
    Some(root)
}

// --- sealing -------------------------------------------------------------

/// Why content went out plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// No key holder in this process (eNet off, signed out).
    NoService,
    /// #FEED-184: a key holder is coming but is not running yet (the
    /// install is still registering).
    NotReady,
    /// The session's root is unknown here.
    UnknownRoot,
    /// The tree has no key and none could be created / fetched.
    NoKey(String),
    /// #FEED-188: a keyless tree this install may not key (not a reader).
    /// Plaintext stays legal there; the server refuses it once it is keyed.
    NotReader,
    /// #EXP-2123 (install-trust.json `keyMaker`): this install waits for
    /// approval (or the server refused its key write with
    /// `KEY_MAKER_REFUSAL`): it makes no chat key. A writer asks the tree's
    /// devices (`requestKey`) and seals to the key they make, else fails
    /// with [`NO_KEY_REFUSAL`]; never plaintext.
    KeyMakerWaits,
    /// #FEED-185: the tree is sealed but this process has no key holder
    /// and none is coming (signed out, eNet off, registration refused).
    NoHolder,
    /// #EDE-97: a clear tree (`sessions.clear_tree`): nobody keys it and
    /// plaintext IS its legal wire (not a failure for a writer).
    ClearTree,
    Invalid(String),
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::NoService => write!(f, "no key holder"),
            SealError::NotReady => write!(f, "this device's key holder is not running yet (its install is still registering)"),
            SealError::UnknownRoot => write!(f, "unknown session root"),
            SealError::NoKey(why) => write!(f, "no tree key: {why}"),
            SealError::NotReader => write!(f, "this install may not key this chat (not a reader)"),
            SealError::KeyMakerWaits => write!(f, "{NO_KEY_REFUSAL}"),
            SealError::NoHolder => write!(
                f,
                "this chat is end-to-end encrypted and this device's key holder is not running (signed out, offline or not registered when the session started); restart the app or daemon while signed in and online, then send again"
            ),
            SealError::ClearTree => write!(f, "{CLEAR_REFUSAL}"),
            SealError::Invalid(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for SealError {}

/// `sealed-content.json` `clearTree.clearRefusal`.
pub const CLEAR_REFUSAL: &str = "This chat stays in clear: an MCP client outside Exponential started it";
/// `sealed-content.json` `clearTree.noKeyRefusal`.
pub const NO_KEY_REFUSAL: &str = "This chat is end-to-end encrypted and this device holds no key for it yet. Try again in a moment.";

fn key_for_writing(root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
    if tree_clear(root_id) {
        return Err(SealError::ClearTree);
    }
    let service = sealer()?;
    let (epoch, public) = match service.ensure_key(root_id) {
        Err(SealError::NotReader) => Err(SealError::NoKey(SealError::NotReader.to_string())),
        // #EXP-2123 (`keyMaker` firstKey = `requestKey`): this install makes
        // no key: the tree's devices are asked and the content seals to the
        // key they make, else the send fails; never plaintext.
        Err(SealError::KeyMakerWaits) => match requested_key(service.as_ref(), root_id) {
            Err(SealError::NotReader) => Err(SealError::NoKey(NO_KEY_REFUSAL.to_string())),
            other => other,
        },
        other => other,
    }?;
    service.check_seal(root_id, epoch, &public, None)?;
    Ok((epoch, public))
}

/// EXP-1472: how long a send into a tree this install may not key waits for
/// its first key (its device creates it when it claims the root).
pub const SEND_KEY_WAIT: Duration = Duration::from_secs(20);
const SEND_KEY_POLL: Duration = Duration::from_secs(1);
/// [`SEND_KEY_WAIT`] in ms (tests shorten it).
static SEND_KEY_WAIT_MS: AtomicU64 = AtomicU64::new(SEND_KEY_WAIT.as_millis() as u64);

#[doc(hidden)]
pub fn set_send_key_wait(wait: Duration) {
    SEND_KEY_WAIT_MS.store(wait.as_millis() as u64, Ordering::SeqCst);
}

fn key_for_sending(root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
    let (epoch, public) = key_for_sending_unchecked(root_id)?;
    check_seal(root_id, epoch, &public, None)?;
    Ok((epoch, public))
}

fn key_for_sending_unchecked(root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
    if tree_clear(root_id) {
        return Err(SealError::ClearTree);
    }
    let service = sealer()?;
    if let Some(current) = service.keyring().current(root_id) {
        return Ok(current);
    }
    // #FEED-188: a tree nobody keyed takes its key from its first writer, as
    // on web and the phones. Waiting for its device cannot work for an idle
    // chat older than sealing: only a claim keys it there, and this send is
    // what would wake it. The other readers get theirs through `pending`.
    match service.ensure_key(root_id) {
        Err(SealError::NotReader) => requested_key(service.as_ref(), root_id),
        // #EXP-2123: an install that makes no key asks the same way; failing
        // it is the no-key refusal (never the keyless-tree plaintext).
        Err(SealError::KeyMakerWaits) => match requested_key(service.as_ref(), root_id) {
            Err(SealError::NotReader) => Err(SealError::NoKey(NO_KEY_REFUSAL.to_string())),
            other => other,
        },
        other => other,
    }
}

/// Not a reader (or, #EXP-2123, an install that makes no key): #EXP-1960
/// an idle chat's device claims nothing, so it is ASKED to key the tree
/// (`sessionKeys.requestKey`), then its key is waited for (EXP-1472); a
/// tree no device hosts fails at once. #EDE-97: after the wait the send
/// FAILS (NotReader), never plaintext.
fn requested_key(service: &dyn SealService, root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
    let answer = service.request_key(root_id);
    match (keyless_step_after_request(&answer), answer) {
        (KeylessStep::Sealed, RequestKeyAnswer::Keyed(epoch, public)) => {
            service.keyring().set_current(root_id, epoch, public);
            return Ok((epoch, public));
        }
        (KeylessStep::Plain, _) => {
            note_clear_tree(root_id);
            return Err(SealError::ClearTree);
        }
        (KeylessStep::Failed, _) => return Err(SealError::NotReader),
        _ => {}
    }
    let deadline = Instant::now() + Duration::from_millis(SEND_KEY_WAIT_MS.load(Ordering::SeqCst));
    loop {
        if let Some(key) = service.public_key(root_id).map_err(SealError::NoKey)? {
            return Ok(key);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(SealError::NotReader);
        }
        std::thread::sleep(left.min(SEND_KEY_POLL));
    }
}

/// EXP-1472: keys a root at its claim (the device about to run it), so
/// content sent into the tree from elsewhere finds a key to seal with.
/// BLOCKING; a no-op without a key holder or when the root is keyed.
pub fn ensure_tree_key(root_id: &str) {
    // Never waits for a coming key holder: the claim goes on, the first
    // sealed write keys the root instead.
    if service().is_none() || tree_clear(root_id) {
        return;
    }
    let Ok(service) = sealer() else { return };
    // #EXP-2123 (`keyMaker` deviceKey = `skip`): an install that makes no
    // key (it waits for approval, or the server refused its key write)
    // leaves the tree to an approved holder: nothing created, nothing asked.
    let found = match service.ensure_key(root_id) {
        Err(SealError::KeyMakerWaits) => {
            log::info!("sealing: the tree key of {root_id} is left to an approved install: this one waits for approval");
            return;
        }
        Err(SealError::NotReader) => Err(SealError::NoKey(SealError::NotReader.to_string())),
        other => other,
    };
    match found.and_then(|(epoch, public)| service.check_seal(root_id, epoch, &public, None)) {
        Ok(_) | Err(SealError::NoService | SealError::ClearTree) => {}
        Err(err) => log::warn!("sealing: the tree key of {root_id} was not created at its claim: {err}"),
    }
}

/// A body (`messages.body`, `sessions.sealed_title`) sealed to `root`'s
/// tree, creating the tree key when the root has none. BLOCKING.
pub fn seal_body(root_id: &str, text: &str) -> Result<String, SealError> {
    let (epoch, public) = key_for_writing(root_id)?;
    enet_core::sealed::seal_body(&public, epoch, text).map_err(|e| SealError::Invalid(e.to_string()))
}

/// A card sealed to `root`'s tree (`{...clear, sealed}`). BLOCKING.
pub fn seal_card(root_id: &str, card: &Value) -> Result<Value, SealError> {
    let (epoch, public) = key_for_writing(root_id)?;
    enet_core::sealed::seal_card(&public, epoch, card).map_err(|e| SealError::Invalid(e.to_string()))
}

/// A body sealed for ANOTHER tree (agent-to-agent): its current public
/// key, created here when the tree has none and this install is a reader
/// ([`SealError::NotReader`] otherwise). BLOCKING.
pub fn seal_body_for_tree(root_id: &str, text: &str) -> Result<String, SealError> {
    let (epoch, public) = key_for_sending(root_id)?;
    enet_core::sealed::seal_body(&public, epoch, text).map_err(|e| SealError::Invalid(e.to_string()))
}

/// EXP-1458: `line` sealed to one listening tree's current key (`None` = a
/// keyless tree: its listeners get the neutral line). BLOCKING.
pub fn listener_copy(
    tree: &api::session_device::ListenerTree,
    line: &str,
) -> Option<api::session_device::ListenerCopy> {
    let (Some(epoch), Some(key)) = (tree.epoch, tree.public_key.as_deref()) else { return None };
    let public = public_key_from_b64(key).ok()?;
    if let Some(service) = service() {
        service.keyring().set_current(&tree.root_id, epoch, public);
    }
    match seal_body_for_tree(&tree.root_id, line) {
        Ok(body) => Some(api::session_device::ListenerCopy { root_id: tree.root_id.clone(), body }),
        Err(err) => {
            log::warn!("sealing: no listener copy for tree {}: {err}", tree.root_id);
            None
        }
    }
}

/// A card sealed for another tree. BLOCKING.
pub fn seal_card_for_tree(root_id: &str, card: &Value) -> Result<Value, SealError> {
    let (epoch, public) = key_for_sending(root_id)?;
    enet_core::sealed::seal_card(&public, epoch, card).map_err(|e| SealError::Invalid(e.to_string()))
}

/// #EDE-97: what a writer posts into `session_id`'s tree: the body SEALED
/// to its tree (a keyless tree is keyed first, or waits for its device's
/// claim to key it, [`SEND_KEY_WAIT`]; a coming key holder is waited for,
/// [`SERVICE_WAIT`]). Plaintext ONLY into a clear tree ([`tree_clear`]);
/// empty and already-sealed text go as given. Any other failure is an
/// `Err` and the caller sends NOTHING (no silent plaintext). BLOCKING.
pub fn seal_body_for_session(session_id: &str, text: &str) -> Result<String, SealError> {
    if text.is_empty() || matches!(parse_body_envelope(text), BodyEnvelope::Value(_)) {
        return Ok(text.to_string());
    }
    match session_root(session_id).and_then(|root| {
        run_held_claim(session_id)?;
        seal_body_for_tree(&root, text)
    }) {
        Err(SealError::ClearTree) => Ok(text.to_string()),
        other => other,
    }
}

/// [`seal_body_for_session`] for a card (`{...clear, sealed}`); a sealed
/// card goes as given, a clear tree takes it plaintext. BLOCKING.
pub fn seal_card_for_session(session_id: &str, card: &Value) -> Result<Value, SealError> {
    if card.get("sealed").is_some() {
        return Ok(card.clone());
    }
    match session_root(session_id).and_then(|root| {
        run_held_claim(session_id)?;
        seal_card_for_tree(&root, card)
    }) {
        Err(SealError::ClearTree) => Ok(card.clone()),
        other => other,
    }
}

/// The root a session writes into; [`SealError::ClearTree`] when the
/// session itself is a known clear root (no lookup, no holder needed).
fn session_root(session_id: &str) -> Result<String, SealError> {
    if tree_clear(session_id) {
        return Err(SealError::ClearTree);
    }
    #[cfg(test)]
    if test_support::plain_is_legal() {
        return Err(SealError::ClearTree);
    }
    let root = match with_keyring(|k| k.root_of(session_id)) {
        Some(root) => root,
        None => {
            // The holder's lookup; no holder (or one that never comes) is
            // the failure, not an unknown root.
            let service = sealer()?;
            let root = service.resolve_root(session_id).ok_or(SealError::UnknownRoot)?;
            service.keyring().note_root(session_id, &root);
            root
        }
    };
    if tree_clear(&root) {
        return Err(SealError::ClearTree);
    }
    Ok(root)
}

// --- the writer's decision (`clearTree.writerCases` / `claimCases`) ------

/// Where a send's text goes (`sendWire`'s `tree`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendTree {
    /// A `clear_tree` root.
    Clear,
    /// The send creates the root.
    New,
    Keyed,
    /// An existing root without a key.
    Keyless,
}

/// This process's key holder (`sendWire`'s `holder`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Ready,
    /// Coming up within [`SERVICE_WAIT`]: the writer waits for it.
    Late,
    /// None within the wait (signed out, eNet off, registration refused).
    None,
}

/// What leaves (`sendWire`'s result).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendWire {
    /// Only into a clear tree.
    Plain,
    /// A new root's key rides as `seal`.
    Inline,
    /// `sessionKeys.create` first, then sealed.
    Create,
    /// The tree's current key.
    Sealed,
    /// Nothing leaves: the send fails.
    Failed,
}

/// #EDE-97: the pure writer decision of every writer x4 (the TS reference
/// `sendWire`, `packages/domain-contract/src/sealed-content.ts`): `reader`
/// = this install may key a keyless tree, `key_arrives` = its device's
/// claim keys it within [`SEND_KEY_WAIT`].
pub fn send_wire(tree: SendTree, holder: Holder, reader: bool, key_arrives: bool) -> SendWire {
    match (tree, holder) {
        (SendTree::Clear, _) => SendWire::Plain,
        (_, Holder::None) => SendWire::Failed,
        (SendTree::New, _) => SendWire::Inline,
        (SendTree::Keyed, _) => SendWire::Sealed,
        (SendTree::Keyless, _) if reader => SendWire::Create,
        (SendTree::Keyless, _) if key_arrives => SendWire::Sealed,
        (SendTree::Keyless, _) => SendWire::Failed,
    }
}

// --- a keyless tree's key (`clearTree.keylessCases`, #EXP-1960) ----------

/// `sealed-content.json` `clearTree.createAttempts` / `createRetryMs`.
pub const CREATE_ATTEMPTS: u32 = 3;
pub const CREATE_RETRY: Duration = Duration::from_millis(500);

/// A failed `sessionKeys.create` (`keylessCases`' `failure`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateFailure {
    Clear,
    Stale,
    NotReader,
    WrapRefused,
    Refused,
    Transient,
}

/// `sessionKeys.requestKey`'s answer (`keylessCases`' `answer`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestKeyAnswer {
    Keyed(u32, [u8; 32]),
    Clear,
    Requested,
    NoHost,
    /// A real NOT_FOUND: the bare wait.
    NotFound,
    /// No answer, an older server: the bare wait.
    Error,
}

/// What a writer does next (`keylessCases`' `step`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeylessStep {
    Plain,
    ServerKey,
    RequestKey,
    RetryOwn,
    Retry,
    Failed,
    Sealed,
    Wait,
}

/// A failed create read from its HTTP status + message (the TS `createFailure`;
/// `None` = no answer: a transport blip).
pub fn create_failure(status: Option<u16>, message: &str) -> CreateFailure {
    if message.contains(CLEAR_REFUSAL) {
        return CreateFailure::Clear;
    }
    if status == Some(409) || message.starts_with("The tree is at epoch") || message.contains(STALE_REFUSAL_MARK) {
        return CreateFailure::Stale;
    }
    if message.contains(WRAP_REFUSAL) {
        return CreateFailure::WrapRefused;
    }
    match status {
        Some(404) => CreateFailure::NotReader,
        Some(408 | 429) | None => CreateFailure::Transient,
        Some(code) if (400..500).contains(&code) => CreateFailure::Refused,
        _ => CreateFailure::Transient,
    }
}

/// `sealed-content.json` `clearTree.wrapRefusal`.
pub const WRAP_REFUSAL: &str = "A wrap names an install that is not a reader";
/// `staleTree.refusal` (the CONFLICT for a key write into a tree that moved on).
const STALE_REFUSAL_MARK: &str = "This chat's encryption key changed. Try again in a moment.";

/// #EXP-1960: the next step after a failed create (the TS `keylessStep`).
pub fn keyless_step(failure: CreateFailure, attempt: u32, own_only: bool) -> KeylessStep {
    match failure {
        CreateFailure::Clear => KeylessStep::Plain,
        CreateFailure::Stale => KeylessStep::ServerKey,
        CreateFailure::NotReader => KeylessStep::RequestKey,
        CreateFailure::WrapRefused if own_only => KeylessStep::RequestKey,
        CreateFailure::WrapRefused => KeylessStep::RetryOwn,
        CreateFailure::Refused => KeylessStep::Failed,
        CreateFailure::Transient if attempt < CREATE_ATTEMPTS => KeylessStep::Retry,
        CreateFailure::Transient => KeylessStep::Failed,
    }
}

/// #EXP-1960: the next step once the tree's devices were asked.
pub fn keyless_step_after_request(answer: &RequestKeyAnswer) -> KeylessStep {
    match answer {
        RequestKeyAnswer::Keyed(..) => KeylessStep::Sealed,
        RequestKeyAnswer::Clear => KeylessStep::Plain,
        RequestKeyAnswer::NoHost => KeylessStep::Failed,
        RequestKeyAnswer::Requested | RequestKeyAnswer::NotFound | RequestKeyAnswer::Error => KeylessStep::Wait,
    }
}

/// #EDE-97 `claimCases`: the device keys a root it claims without a key,
/// unless it is a clear tree.
pub fn claim_keys(clear_tree: bool, keyed: bool) -> bool {
    !clear_tree && !keyed
}

/// #EDE-97 `claimCases`: the device re-seals the session's first message
/// when it is still plaintext and the tree is not clear (keyed by then).
pub fn claim_reseals(clear_tree: bool, first_plain: bool) -> bool {
    !clear_tree && first_plain
}

// --- opening -------------------------------------------------------------

/// Runs `open` with `root`'s secrets; on `noKey` pulls the wraps once (rate
/// limited per root) and tries again. May BLOCK on that pull.
fn opening<T>(root_id: &str, open: impl Fn(&dyn Fn(u32) -> Option<[u8; 32]>) -> (OpenStatus, T)) -> T {
    let service = service();
    let keyring_open = |keyring: &Keyring| open(&|epoch| keyring.secret(root_id, epoch));
    let Some(service) = service else {
        return with_keyring(|keyring| keyring_open(keyring).1);
    };
    let (status, value) = keyring_open(service.keyring());
    if status != OpenStatus::NoKey || !service.keyring().may_fetch(root_id) {
        return value;
    }
    service.fetch_secrets(root_id);
    keyring_open(service.keyring()).1
}

/// Opens a body of `root`'s tree.
pub fn open_body(root_id: &str, body: &str) -> OpenedBody {
    if !body.starts_with(SEALED_BODY_PREFIX) {
        return OpenedBody { status: OpenStatus::Plain, epoch: None, text: Some(body.to_string()) };
    }
    opening(root_id, |keys| {
        let opened = enet_core::sealed::open_body(body, keys);
        (opened.status, opened)
    })
}

/// Opens a card of `root`'s tree.
pub fn open_card(root_id: &str, card: &Value) -> OpenedCard {
    if card.get("sealed").is_none() {
        return OpenedCard { status: OpenStatus::Plain, epoch: None, card: card.clone() };
    }
    opening(root_id, |keys| {
        let opened = enet_core::sealed::open_card(card, keys);
        (opened.status, opened)
    })
}

/// The text a reader shows for a body: the plaintext / opened text, else
/// `None` (locked: no key yet, a newer version, broken).
pub fn open_body_text(root_id: &str, body: &str) -> Option<String> {
    open_body(root_id, body).text
}

/// FEED-178: [`open_body_text`] that keeps pulling the tree's wraps (every
/// `step`, the holder woken) while the key is missing, up to `wait`. A new
/// root's key is wrapped for the sender's install only; the device that
/// runs it gets its wrap from a holder's `pending` pass moments later.
pub fn open_body_text_waiting(root_id: &str, body: &str, wait: Duration, step: Duration) -> Option<String> {
    let started = Instant::now();
    loop {
        let opened = open_body(root_id, body);
        if opened.text.is_some() || opened.status != OpenStatus::NoKey || started.elapsed() >= wait {
            return opened.text;
        }
        std::thread::sleep(step.min(wait.saturating_sub(started.elapsed())));
        if let Some(service) = service() {
            service.wake();
            service.fetch_secrets(root_id);
        }
    }
}

/// The placeholder an agent reads for content this install cannot open.
pub const LOCKED_TEXT: &str = "[encrypted content this device cannot open yet]";

/// A message row with its body and card opened (the agent-facing copy).
/// Content this install cannot open reads [`LOCKED_TEXT`]; a locked card
/// keeps its clear part.
pub fn open_message(message: &MessageRow) -> MessageRow {
    open_message_keyed(message).0
}

/// [`open_message`], plus whether a part stayed shut for want of its key
/// (`noKey`: the key may still arrive, #EXP-2086), as opposed to content
/// that can never open here (a newer version, broken).
pub fn open_message_keyed(message: &MessageRow) -> (MessageRow, bool) {
    let sealed_body = message.body.as_deref().is_some_and(|b| b.starts_with(SEALED_BODY_PREFIX));
    let sealed_card = message.card.as_ref().is_some_and(|c| c.get("sealed").is_some());
    if !sealed_body && !sealed_card {
        return (message.clone(), false);
    }
    let root = message
        .root_id
        .clone()
        .filter(|r| !r.is_empty())
        .or_else(|| root_of(&message.session_id))
        .unwrap_or_else(|| message.session_id.clone());
    let mut out = message.clone();
    let mut no_key = false;
    if sealed_body {
        let opened = open_body(&root, message.body_text());
        no_key |= opened.status == OpenStatus::NoKey;
        out.body = Some(opened.text.unwrap_or_else(|| LOCKED_TEXT.to_string()));
    }
    if let (true, Some(card)) = (sealed_card, message.card.as_ref()) {
        let opened = open_card(&root, card);
        no_key |= opened.status == OpenStatus::NoKey;
        out.card = Some(opened.card);
    }
    (out, no_key)
}

/// Every message opened ([`open_message`]).
pub fn open_messages(messages: &[MessageRow]) -> Vec<MessageRow> {
    messages.iter().map(open_message).collect()
}

/// A session's title: the opened `sealed_title`, else the plaintext title.
pub fn open_session_title(row: &SessionRow) -> Option<String> {
    let sealed = row.sealed_title.as_deref().filter(|t| !t.is_empty());
    if let Some(sealed) = sealed {
        let root = row.root_id.as_deref().filter(|r| !r.is_empty()).unwrap_or(&row.id);
        if let Some(text) = open_body_text(root, sealed) {
            return Some(text);
        }
    }
    row.title.clone()
}

/// The rows with their titles opened (`title` = the opened sealed title).
pub fn open_sessions(rows: &[SessionRow]) -> Vec<SessionRow> {
    rows.iter()
        .map(|row| match row.sealed_title.as_deref().filter(|t| !t.is_empty()) {
            None => row.clone(),
            Some(_) => SessionRow { title: open_session_title(row), ..row.clone() },
        })
        .collect()
}

// --- new roots -----------------------------------------------------------

/// `sessions.send {seal}` for a root that does not exist yet (addendum 2):
/// the key travels with the first message in one transaction. #EXP-1950:
/// its wraps go UNSIGNED (a signature names the root, unknown until the
/// server answers); the holder re-signs them once adopted ([`SealService::adopt`]).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRootSeal {
    pub install_id: String,
    pub public_key: String,
    pub epoch: u32,
    pub wraps: Vec<api::session_keys::InstallWrap>,
    #[serde(skip)]
    pub secret: [u8; 32],
}

/// A fresh tree key wrapped for this install plus `readers`
/// (`(install_id, x25519_public)`; duplicates and malformed keys skipped).
pub fn seal_new_root(own_install_id: &str, own_public: &[u8; 32], readers: &[(String, String)]) -> Result<NewRootSeal, SealError> {
    let key = generate_tree_key();
    let mut wraps = vec![api::session_keys::InstallWrap {
        install_id: own_install_id.to_string(),
        wrapped: wrap_tree_key(own_public, &key.secret).map_err(|e| SealError::Invalid(e.to_string()))?,
        signature: None,
    }];
    for (install_id, public) in readers {
        if wraps.iter().any(|w| &w.install_id == install_id) {
            continue;
        }
        let Ok(public) = public_key_from_b64(public) else { continue };
        if let Ok(wrapped) = wrap_tree_key(&public, &key.secret) {
            wraps.push(api::session_keys::InstallWrap { install_id: install_id.clone(), wrapped, signature: None });
        }
    }
    Ok(NewRootSeal { install_id: own_install_id.to_string(), public_key: key.public_key(), epoch: 1, wraps, secret: key.secret })
}

/// After `sessions.send {seal}` returned the new root: keep its secret.
pub fn adopt_new_root(root_id: &str, seal: &NewRootSeal) {
    with_keyring(|keyring| adopt_into(keyring, root_id, seal));
}

/// [`SealService::adopt`]'s keyring half.
pub fn adopt_into(keyring: &Keyring, root_id: &str, seal: &NewRootSeal) {
    keyring.insert_secret(root_id, seal.epoch, seal.secret);
    if let Ok(public) = public_key_from_b64(&seal.public_key) {
        keyring.set_current(root_id, seal.epoch, public);
    }
    keyring.note_root(root_id, root_id);
}

#[cfg(test)]
pub(crate) mod test_support {
    //! A [`SealService`] over a fixed keyring, for this crate's tests and
    //! the gateway's. Installing it is process-global: tests that do hold
    //! [`serial`].
    use super::*;

    pub static SERIAL: Mutex<()> = Mutex::new(());

    thread_local! {
        static HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Holds [`SERIAL`] for this test. Re-entrant on one thread (a nested
    /// call holds nothing), so a fixture can take it too: pickup's `Host`
    /// does, because every claim, reply and leftover note seals through the
    /// process-global service, so a test running one beside a test that
    /// installed a fake would key ITS roots in that fake.
    pub struct Serial(#[allow(dead_code)] Option<std::sync::MutexGuard<'static, ()>>);

    impl Drop for Serial {
        fn drop(&mut self) {
            if self.0.is_some() {
                HELD.with(|held| held.set(false));
            }
        }
    }

    static PLAIN: AtomicBool = AtomicBool::new(false);

    /// Tests about something else than sealing (pickup's control flow)
    /// write as if every tree were clear while the guard lives.
    pub(crate) fn plain_is_legal() -> bool {
        PLAIN.load(Ordering::SeqCst)
    }

    pub struct PlainGuard;

    impl Drop for PlainGuard {
        fn drop(&mut self) {
            PLAIN.store(false, Ordering::SeqCst);
        }
    }

    /// Hold [`serial`] too: the flag is process-global.
    pub fn allow_plain() -> PlainGuard {
        PLAIN.store(true, Ordering::SeqCst);
        PlainGuard
    }

    pub fn serial() -> Serial {
        if HELD.with(|held| held.get()) {
            return Serial(None);
        }
        let guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        HELD.with(|held| held.set(true));
        Serial(Some(guard))
    }

    #[derive(Default)]
    pub struct FakeService {
        pub keyring: Keyring,
        pub created: Mutex<Vec<String>>,
        pub delivered: Mutex<Vec<(String, MessageRow)>>,
        /// [`SealService::install_public`] (`None` = new roots stay plain).
        pub install_public: Option<[u8; 32]>,
        /// [`SealService::resolve_root`] answers.
        pub roots: Mutex<HashMap<String, String>>,
        /// [`SealService::new_root_readers`]: `(device, install, public)`.
        pub device_installs: Vec<(String, String, String)>,
        /// Roots [`SealService::ensure_key`] refuses to key (not a reader).
        pub not_reader_of: Mutex<Vec<String>>,
        /// #EXP-1960: [`SealService::request_key`]'s answer (`None` = no answer).
        pub request_answer: Mutex<Option<RequestKeyAnswer>>,
        /// The roots whose devices were asked.
        pub requested: Mutex<Vec<String>>,
        /// The key the asked device makes (it lands on the next poll).
        pub device_keys: Mutex<Option<(u32, [u8; 32])>>,
        /// #EXP-2123: this install waits for approval: it makes no key.
        pub waits: AtomicBool,
    }

    impl SealService for FakeService {
        fn keyring(&self) -> &Keyring {
            &self.keyring
        }
        fn install_id(&self) -> Option<String> {
            Some("install-1".into())
        }
        fn install_public(&self) -> Option<[u8; 32]> {
            self.install_public
        }
        fn resolve_root(&self, session_id: &str) -> Option<String> {
            self.roots.lock().unwrap().get(session_id).cloned()
        }
        fn new_root_readers(&self, target_device: &str) -> Vec<(String, String)> {
            self.device_installs
                .iter()
                .filter(|(device, _, _)| device == target_device)
                .map(|(_, install, public)| (install.clone(), public.clone()))
                .collect()
        }
        fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
            if let Some(current) = self.keyring.current(root_id) {
                return Ok(current);
            }
            if self.not_reader_of.lock().unwrap().iter().any(|root| root == root_id) {
                return Err(SealError::NotReader);
            }
            if self.waits.load(Ordering::SeqCst) {
                return Err(SealError::KeyMakerWaits);
            }
            let key = generate_tree_key();
            self.keyring.insert_secret(root_id, 1, key.secret);
            self.keyring.set_current(root_id, 1, key.public);
            self.created.lock().unwrap().push(root_id.to_string());
            Ok((1, key.public))
        }
        fn public_key(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String> {
            Ok(self.keyring.current(root_id))
        }
        fn fetch_secrets(&self, _root_id: &str) {}
        fn request_key(&self, root_id: &str) -> RequestKeyAnswer {
            self.requested.lock().unwrap().push(root_id.to_string());
            if let Some((epoch, public)) = *self.device_keys.lock().unwrap() {
                self.keyring.set_current(root_id, epoch, public);
            }
            self.request_answer.lock().unwrap().clone().unwrap_or(RequestKeyAnswer::Error)
        }
        fn deliver_direct(&self, to: &DeliverTo, message: &MessageRow) {
            self.delivered.lock().unwrap().push((to.endpoint_id.clone(), message.clone()));
        }
        fn makes_keys(&self) -> bool {
            !self.waits.load(Ordering::SeqCst)
        }
    }

    /// Installs a fresh fake and returns it.
    pub fn install_fake() -> Arc<FakeService> {
        let fake = Arc::new(FakeService::default());
        install(fake.clone());
        fake
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }

    fn hex32(v: &Value) -> [u8; 32] {
        let s = v.as_str().unwrap();
        (0..32).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap()).collect::<Vec<_>>().try_into().unwrap()
    }

    /// The fixture's tree secrets, held for `root`.
    fn fixture_keyring(root: &str) -> Keyring {
        let fx = fixture();
        let keyring = Keyring::in_memory();
        for (epoch, secret) in fx["keys"].as_object().unwrap() {
            keyring.insert_secret(root, epoch.parse().unwrap(), hex32(secret));
        }
        keyring
    }

    #[test]
    fn opens_the_fixture_bodies_and_cards_through_the_service() {
        let _serial = serial();
        let fake = Arc::new(FakeService { keyring: fixture_keyring("root-1"), ..Default::default() });
        install(fake.clone());
        let fx = fixture();
        for case in fx["bodyCases"].as_array().unwrap() {
            let got = open_body("root-1", case["body"].as_str().unwrap());
            assert_eq!(got.status.as_str(), case["status"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(got.text.as_deref(), case["text"].as_str(), "{}", case["name"]);
        }
        for case in fx["cardCases"].as_array().unwrap() {
            let got = open_card("root-1", &case["card"]);
            assert_eq!(got.status.as_str(), case["status"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(got.card, case["opened"], "{}", case["name"]);
        }
        uninstall();
    }

    #[test]
    fn messages_and_titles_open_with_the_rows_root() {
        let _serial = serial();
        let fake = install_fake();
        fake.keyring.note_sessions(&[
            SessionRow { id: "root".into(), root_id: Some("root".into()), ..Default::default() },
            SessionRow { id: "child".into(), root_id: Some("root".into()), parent_id: Some("root".into()), ..Default::default() },
        ]);
        let body = seal_body_for_session("child", "hello child").unwrap();
        assert!(body.starts_with("exp-sealed:v1:1:"), "{body}");
        assert_eq!(fake.created.lock().unwrap().as_slice(), ["root"]);
        let card = seal_card_for_session("child", &serde_json::json!({"kind": "report", "sessionId": "child", "summary": "done"})).unwrap();
        assert!(card.get("summary").is_none());
        let row = MessageRow { id: "m".into(), session_id: "child".into(), body: Some(body), card: Some(card), ..Default::default() };
        let opened = open_message(&row);
        assert_eq!(opened.body.as_deref(), Some("hello child"));
        assert_eq!(opened.card.unwrap()["summary"], "done");

        let title = seal_body("root", "Fix the login").unwrap();
        let session = SessionRow {
            id: "child".into(),
            root_id: Some("root".into()),
            title: Some("Session".into()),
            sealed_title: Some(title),
            ..Default::default()
        };
        assert_eq!(open_session_title(&session).as_deref(), Some("Fix the login"));
        uninstall();
    }

    #[test]
    fn locked_content_reads_the_placeholder_and_no_service_never_sends_plain() {
        let _serial = serial();
        uninstall();
        // #EDE-97: no key holder = the write fails, never plaintext...
        assert_eq!(seal_body_for_session("s", "plain"), Err(SealError::NoService));
        assert_eq!(seal_card_for_session("s", &serde_json::json!({"kind": "report"})), Err(SealError::NoService));
        // ...except empty or already sealed text.
        assert_eq!(seal_body_for_session("s", "").as_deref(), Ok(""));
        let fake = install_fake();
        let other = generate_tree_key();
        let body = enet_core::sealed::seal_body(&other.public, 1, "secret").unwrap();
        fake.keyring.note_root("s", "r");
        let row = MessageRow { id: "m".into(), session_id: "s".into(), body: Some(body), ..Default::default() };
        assert_eq!(open_message(&row).body.as_deref(), Some(LOCKED_TEXT));
        uninstall();
    }

    #[test]
    fn a_holder_installed_after_boot_reads_as_a_change() {
        let _serial = serial();
        uninstall();
        let before = changed();
        // A holder whose keyring was loaded from disk: its counter is 0 too.
        install_fake();
        assert_ne!(changed(), before);
        uninstall();
    }

    /// Cold start (`sealed-content.json` `coldStart`): the keys persisted by
    /// the last run open the rows before any key holder runs (no install
    /// registered, no network); a key landing later re-opens them; the
    /// holder then runs on that SAME keyring (the roots the rows taught it
    /// stay).
    #[test]
    fn a_cold_start_opens_with_the_persisted_keys_before_the_holder() {
        let _serial = serial();
        uninstall();
        let fx = fixture();
        let dir = std::env::temp_dir().join(format!("exp-sealing-cold-{}", std::process::id()));
        for (i, case) in fx["coldStart"]["cases"].as_array().unwrap().iter().enumerate() {
            let name = case["name"].as_str().unwrap();
            assert_eq!(case["holder"], "none", "{name}");
            let path = dir.join(format!("seal-keys-{i}"));
            let _ = std::fs::remove_file(&path);
            // What the previous run stored.
            let previous = Keyring::load(path.clone());
            for (epoch, secret) in case["persisted"].as_object().unwrap() {
                previous.insert_secret("root-1", epoch.parse().unwrap(), hex32(secret));
            }
            drop(previous);
            let body = case["body"].as_str().unwrap();
            let before = changed();
            let keyring = preload(path.clone());
            assert_ne!(changed(), before, "{name}: the preload re-opens rows shown locked");
            let first = open_body("root-1", body);
            assert_eq!(first.status.as_str(), case["firstFrame"].as_str().unwrap(), "{name}: first frame");
            if case["firstFrame"] == "opened" {
                assert_eq!(first.text.as_deref(), case["text"].as_str(), "{name}");
            }
            let seen = changed();
            for (epoch, secret) in case["arrives"].as_object().into_iter().flatten() {
                keyring.insert_secret("root-1", epoch.parse().unwrap(), hex32(secret));
                assert_ne!(changed(), seen, "{name}: an arriving key re-opens the rows");
            }
            let after = open_body("root-1", body);
            assert_eq!(after.status.as_str(), case["after"].as_str().unwrap(), "{name}: after");
            if case["after"] == "opened" {
                assert_eq!(after.text.as_deref(), case["text"].as_str(), "{name}");
            }
            // The holder registers later and takes the same keyring.
            note_sessions(&[SessionRow { id: "child".into(), root_id: Some("root-1".into()), ..Default::default() }]);
            assert!(Arc::ptr_eq(&keyring, &preload(path.clone())), "{name}");
            assert_eq!(keyring.root_of("child").as_deref(), Some("root-1"), "{name}");
            // Another account's node stopping leaves it; its own sign-out drops it.
            unload(&dir.join("other"));
            assert!(read(&PRELOADED).is_some(), "{name}");
            unload(&path);
            assert!(read(&PRELOADED).is_none(), "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_keyring_persists_and_new_roots_adopt_their_secret() {
        let dir = std::env::temp_dir().join(format!("exp-sealing-{}", std::process::id()));
        let path = dir.join("seal-keys");
        let keyring = Keyring::load(path.clone());
        let key = generate_tree_key();
        assert!(keyring.insert_secret("r", 2, key.secret));
        assert!(!keyring.insert_secret("r", 2, key.secret));
        let again = Keyring::load(path.clone());
        assert_eq!(again.secret("r", 2), Some(key.secret));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);

        let own = enet_core::PushKeypair::generate();
        let reader = enet_core::PushKeypair::generate();
        let seal = seal_new_root("me", &own.public(), &[("you".into(), b64url_encode(&reader.public())), ("bad".into(), "x".into())])
            .unwrap();
        assert_eq!(seal.wraps.len(), 2);
        assert_eq!(unwrap_tree_key(&reader.secret(), &seal.wraps[1].wrapped).unwrap(), seal.secret);
        let json = serde_json::to_value(&seal).unwrap();
        assert!(json.get("secret").is_none());
        assert_eq!(json["installId"], "me");
    }

    /// #FEED-185: a daemon restarted after a reboot reloads its tree
    /// secrets from disk and relearns each tree's current key from the
    /// synced rows: its agent's sends into the sealed tree seal again
    /// (nothing about the key is lost with the process).
    #[test]
    fn a_restarted_daemon_reloads_its_tree_keys_and_seals_again() {
        let _serial = serial();
        let dir = std::env::temp_dir().join(format!("exp-sealing-185-{}", std::process::id()));
        let path = dir.join("cli-seal-keys");
        let tree = generate_tree_key();
        {
            let before = Keyring::load(path.clone());
            before.insert_secret("root-185", 1, tree.secret);
            before.set_current("root-185", 1, tree.public);
        }
        // The "reboot": a fresh process, a fresh keyring from the same file.
        let fake = Arc::new(FakeService { keyring: Keyring::load(path.clone()), ..Default::default() });
        install(fake.clone());
        assert_eq!(fake.keyring.secret("root-185", 1), Some(tree.secret));
        note_sessions(&[SessionRow {
            id: "root-185".into(),
            seal_public_key: Some(b64url_encode(&tree.public)),
            seal_epoch: Some(1),
            ..Default::default()
        }]);
        assert_eq!(fake.keyring.current("root-185"), Some((1, tree.public)));
        let body = seal_body("root-185", "PING-185").unwrap();
        assert!(body.starts_with(SEALED_BODY_PREFIX));
        let opened = enet_core::sealed::open_body(&body, &|_| fake.keyring.secret("root-185", 1));
        assert_eq!(opened.text.as_deref(), Some("PING-185"));
        // No key was minted: the reloaded one is the tree's.
        assert!(fake.created.lock().unwrap().is_empty());
        uninstall();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-1960: `clearTree.keylessCases` replayed through the pure steps.
    #[test]
    fn the_keyless_steps_replay_the_fixture() {
        let fx = fixture();
        let clear = &fx["clearTree"];
        assert_eq!(CREATE_ATTEMPTS as u64, clear["createAttempts"].as_u64().unwrap());
        assert_eq!(CREATE_RETRY.as_millis() as u64, clear["createRetryMs"].as_u64().unwrap());
        assert_eq!(WRAP_REFUSAL, clear["wrapRefusal"].as_str().unwrap());
        assert_eq!(STALE_REFUSAL_MARK, fx["staleTree"]["refusal"].as_str().unwrap());
        let step_of = |name: &str| match name {
            "plain" => KeylessStep::Plain,
            "serverKey" => KeylessStep::ServerKey,
            "requestKey" => KeylessStep::RequestKey,
            "retryOwn" => KeylessStep::RetryOwn,
            "retry" => KeylessStep::Retry,
            "failed" => KeylessStep::Failed,
            "sealed" => KeylessStep::Sealed,
            "wait" => KeylessStep::Wait,
            other => panic!("step {other}"),
        };
        for case in clear["keylessCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let got = if let Some(answer) = case["answer"].as_str() {
                keyless_step_after_request(&match answer {
                    "keyed" => RequestKeyAnswer::Keyed(1, [0; 32]),
                    "clear" => RequestKeyAnswer::Clear,
                    "requested" => RequestKeyAnswer::Requested,
                    "noHost" => RequestKeyAnswer::NoHost,
                    "notFound" => RequestKeyAnswer::NotFound,
                    "error" => RequestKeyAnswer::Error,
                    other => panic!("{name}: answer {other}"),
                })
            } else {
                let failure = match case["failure"].as_str().unwrap() {
                    "clear" => CreateFailure::Clear,
                    "stale" => CreateFailure::Stale,
                    "notReader" => CreateFailure::NotReader,
                    "wrapRefused" => CreateFailure::WrapRefused,
                    "refused" => CreateFailure::Refused,
                    "transient" => CreateFailure::Transient,
                    other => panic!("{name}: failure {other}"),
                };
                keyless_step(failure, case["attempt"].as_u64().unwrap_or(1) as u32, case["ownOnly"].as_bool().unwrap_or(false))
            };
            assert_eq!(got, step_of(case["step"].as_str().unwrap()), "{name}");
        }
        assert_eq!(create_failure(Some(404), "Session not found"), CreateFailure::NotReader);
        assert_eq!(create_failure(Some(400), CLEAR_REFUSAL), CreateFailure::Clear);
        assert_eq!(create_failure(Some(409), "x"), CreateFailure::Stale);
        assert_eq!(create_failure(Some(400), WRAP_REFUSAL), CreateFailure::WrapRefused);
        assert_eq!(create_failure(Some(403), "Not your install"), CreateFailure::Refused);
        assert_eq!(create_failure(Some(502), "bad gateway"), CreateFailure::Transient);
        assert_eq!(create_failure(None, "connection reset"), CreateFailure::Transient);
    }

    /// #EDE-97: `clearTree.writerCases` and `claimCases` replayed through
    /// the pure decisions; the waits are the fixture's.
    #[test]
    fn the_writer_and_claim_decisions_replay_the_clear_tree_fixture() {
        let fx = fixture();
        let clear = &fx["clearTree"];
        assert_eq!(SERVICE_WAIT.as_millis() as u64, clear["holderWaitMs"].as_u64().unwrap());
        assert_eq!(SEND_KEY_WAIT.as_millis() as u64, clear["keyWaitMs"].as_u64().unwrap());
        assert_eq!(CLEAR_REFUSAL, clear["clearRefusal"].as_str().unwrap());
        assert_eq!(NO_KEY_REFUSAL, clear["noKeyRefusal"].as_str().unwrap());
        for case in clear["writerCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let tree = match case["tree"].as_str().unwrap() {
                "clear" => SendTree::Clear,
                "new" => SendTree::New,
                "keyed" => SendTree::Keyed,
                "keyless" => SendTree::Keyless,
                other => panic!("{name}: tree {other}"),
            };
            let holder = match case["holder"].as_str().unwrap() {
                "ready" => Holder::Ready,
                "late" => Holder::Late,
                "none" => Holder::None,
                other => panic!("{name}: holder {other}"),
            };
            let wire = send_wire(tree, holder, case["reader"].as_bool().unwrap_or(false), case["keyArrives"].as_bool().unwrap_or(false));
            let want = match case["wire"].as_str().unwrap() {
                "plain" => SendWire::Plain,
                "inline" => SendWire::Inline,
                "create" => SendWire::Create,
                "sealed" => SendWire::Sealed,
                "failed" => SendWire::Failed,
                other => panic!("{name}: wire {other}"),
            };
            assert_eq!(wire, want, "{name}");
        }
        for case in clear["claimCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let (clear_tree, keyed, first_plain) =
                (case["clearTree"].as_bool().unwrap(), case["keyed"].as_bool().unwrap(), case["firstPlain"].as_bool().unwrap());
            assert_eq!(claim_keys(clear_tree, keyed), case["keys"].as_bool().unwrap(), "{name}: keys");
            assert_eq!(claim_reseals(clear_tree, first_plain), case["reseals"].as_bool().unwrap(), "{name}: reseals");
        }
    }

    /// #EDE-97: a clear tree (its synced root says `clear_tree`) takes
    /// plaintext from every writer, with or without a key holder, and is
    /// never keyed; any other tree without a holder fails.
    #[test]
    fn a_clear_tree_takes_plaintext_and_is_never_keyed() {
        let _serial = serial();
        uninstall();
        note_sessions(&[
            SessionRow { id: "clear-root".into(), root_id: Some("clear-root".into()), clear_tree: true, ..Default::default() },
            SessionRow { id: "clear-child".into(), root_id: Some("clear-root".into()), parent_id: Some("clear-root".into()), ..Default::default() },
        ]);
        assert!(tree_clear("clear-root"));
        assert_eq!(seal_body_for_session("clear-root", "hello").as_deref(), Ok("hello"));
        let card = serde_json::json!({"kind": "report", "summary": "done"});
        assert_eq!(seal_card_for_session("clear-root", &card), Ok(card.clone()));

        let fake = install_fake();
        note_sessions(&[
            SessionRow { id: "clear-root".into(), root_id: Some("clear-root".into()), clear_tree: true, ..Default::default() },
            SessionRow { id: "clear-child".into(), root_id: Some("clear-root".into()), parent_id: Some("clear-root".into()), ..Default::default() },
            SessionRow { id: "sealed-root".into(), root_id: Some("sealed-root".into()), ..Default::default() },
        ]);
        assert_eq!(seal_body_for_session("clear-child", "hi").as_deref(), Ok("hi"));
        assert_eq!(seal_card_for_session("clear-child", &card), Ok(card.clone()));
        assert_eq!(seal_body("clear-root", "hi"), Err(SealError::ClearTree));
        assert_eq!(seal_body_for_tree("clear-root", "hi"), Err(SealError::ClearTree));
        ensure_tree_key("clear-root");
        assert!(fake.created.lock().unwrap().is_empty(), "nobody keys a clear tree");
        // Another tree seals (keyed by this writer).
        let body = seal_body_for_session("sealed-root", "secret").unwrap();
        assert!(body.starts_with(SEALED_BODY_PREFIX), "{body}");
        assert_eq!(fake.created.lock().unwrap().as_slice(), ["sealed-root"]);
        uninstall();
        // The same tree without a holder: nothing leaves.
        assert_eq!(seal_body_for_session("sealed-root", "secret"), Err(SealError::NoService));
    }

    /// #EXP-1960: a pre-#EDE-97 keyless root this install may not key, its
    /// device idle: the device is asked to key it, the send is sealed once
    /// it did; a tree no device hosts fails at once, never a silent wait.
    #[test]
    fn a_keyless_tree_asks_its_device_and_no_host_fails_at_once() {
        let _serial = serial();
        let root = "8e2f6a90-1c3b-4d5e-8f70-a1b2c3d4e5f6";
        let child = "3a9d0c4e-5b6f-4a70-8c91-d2e3f4a5b6c7";
        let fake = install_fake();
        fake.keyring.note_root(child, root);
        fake.not_reader_of.lock().unwrap().push(root.into());
        let key = generate_tree_key();
        *fake.device_keys.lock().unwrap() = Some((1, key.public));
        *fake.request_answer.lock().unwrap() = Some(RequestKeyAnswer::Requested);
        let body = seal_body_for_session(child, "still there?").unwrap();
        assert!(body.starts_with(SEALED_BODY_PREFIX), "{body}");
        assert_eq!(fake.requested.lock().unwrap().as_slice(), [root]);
        uninstall();

        let fake = install_fake();
        fake.keyring.note_root(child, root);
        fake.not_reader_of.lock().unwrap().push(root.into());
        *fake.request_answer.lock().unwrap() = Some(RequestKeyAnswer::NoHost);
        set_send_key_wait(Duration::from_secs(10));
        let started = Instant::now();
        let refused = seal_body_for_session(child, "hello");
        set_send_key_wait(SEND_KEY_WAIT);
        assert_eq!(refused, Err(SealError::NotReader));
        assert!(started.elapsed() < Duration::from_secs(2), "no silent wait");
        uninstall();
    }

    /// #EXP-2123 (install-trust.json `keyMaker`): an install that waits for
    /// approval keys nothing. firstKey: its own tree's writer (UI, answers:
    /// `seal_body`) and a send into another tree ask the devices and seal to
    /// their key, else fail with the no-key refusal (never plaintext);
    /// deviceKey (a claim): skipped, nobody asked.
    #[test]
    fn a_waiting_install_asks_the_devices_and_never_keys_or_goes_plain() {
        let _serial = serial();
        let fake = install_fake();
        fake.waits.store(true, Ordering::SeqCst);
        let key = generate_tree_key();
        *fake.device_keys.lock().unwrap() = Some((1, key.public));
        *fake.request_answer.lock().unwrap() = Some(RequestKeyAnswer::Requested);
        let body = seal_body("wk-root", "for the approved readers").unwrap();
        assert_eq!(enet_core::sealed::open_body(&body, &|e| (e == 1).then_some(key.secret)).text.as_deref(), Some("for the approved readers"));
        assert_eq!(fake.requested.lock().unwrap().as_slice(), ["wk-root"]);
        assert!(fake.created.lock().unwrap().is_empty());
        uninstall();

        let fake = install_fake();
        fake.waits.store(true, Ordering::SeqCst);
        *fake.request_answer.lock().unwrap() = Some(RequestKeyAnswer::NoHost);
        let refusal = Err(SealError::NoKey(NO_KEY_REFUSAL.into()));
        assert_eq!(seal_body("wk-none", "never in clear"), refusal);
        assert_eq!(seal_card("wk-none", &serde_json::json!({"kind": "report", "summary": "x"})).map(|_| ()), Err(SealError::NoKey(NO_KEY_REFUSAL.into())));
        assert_eq!(seal_body_for_tree("wk-none", "never in clear"), refusal);
        let asked = fake.requested.lock().unwrap().len();
        ensure_tree_key("wk-claim");
        assert_eq!(fake.requested.lock().unwrap().len(), asked, "a claim asks nobody");
        assert!(fake.created.lock().unwrap().is_empty());
        uninstall();
    }

    /// #EDE-97: a keyless tree this install may not key and no claim keys
    /// within the wait FAILS (was: plaintext); a holder still coming that
    /// never arrives fails too.
    #[test]
    fn no_key_and_no_holder_fail_instead_of_plaintext() {
        let _serial = serial();
        let fake = install_fake();
        fake.keyring.note_root("nr-child", "nr-root");
        fake.not_reader_of.lock().unwrap().push("nr-root".into());
        set_send_key_wait(Duration::from_millis(300));
        let refused = seal_body_for_session("nr-child", "hello");
        set_send_key_wait(SEND_KEY_WAIT);
        assert_eq!(refused, Err(SealError::NotReader));
        uninstall();

        expect_service(true);
        set_service_wait(Duration::from_millis(300));
        let late = seal_body_for_session("nr-child", "hello");
        set_service_wait(SERVICE_WAIT);
        expect_service(false);
        assert_eq!(late, Err(SealError::NotReady));
    }
}
