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
//! No service installed (eNet off, signed out) = nothing seals (writers
//! send plaintext, which stays legal) and only plaintext opens.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
    let file = if daemon { "cli-seal-keys" } else { "seal-keys" };
    data_dir.join("accounts").join(account_id).join(file)
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
            match serde_json::from_str::<Value>(&text) {
                Ok(doc) => {
                    for (root, epochs) in doc.get("keys").and_then(Value::as_object).into_iter().flatten() {
                        for (epoch, secret) in epochs.as_object().into_iter().flatten() {
                            let (Ok(epoch), Some(secret)) = (epoch.parse::<u32>(), secret.as_str().and_then(decode32)) else {
                                continue;
                            };
                            state.secrets.entry(root.clone()).or_default().insert(epoch, secret);
                        }
                    }
                }
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
    /// host's synced `sessions` rows (every pickup pass calls it).
    pub fn note_sessions(&self, rows: &[SessionRow]) {
        let mut keys = Vec::new();
        {
            let mut state = write(&self.state);
            for row in rows {
                let root = row.root_id.as_deref().filter(|r| !r.is_empty()).unwrap_or(&row.id);
                if state.roots.get(&row.id).map(String::as_str) != Some(root) {
                    state.roots.insert(row.id.clone(), root.to_string());
                }
                if row.is_root() {
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

fn decode32(text: &str) -> Option<[u8; 32]> {
    b64url_decode(text)?.try_into().ok()
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
    /// device MCP gateway seals an agent's new-thread start with it. `None`
    /// = new threads go out plaintext.
    fn install_public(&self) -> Option<[u8; 32]> {
        None
    }
    /// `root`'s current (epoch, public key) for WRITING into its own tree:
    /// the cached key, else `publicKeys`, else a fresh tree key this
    /// install creates (`sessionKeys.create`, epoch 1).
    fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), String>;
    /// Another tree's current key for SENDING into it (never creates).
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
    /// Keeps a new root's secret once `sessions.send {seal}` returned its id.
    fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
        adopt_into(self.keyring(), root_id, seal);
    }
}

static SERVICE: RwLock<Option<Arc<dyn SealService>>> = RwLock::new(None);
static FALLBACK: std::sync::OnceLock<Keyring> = std::sync::OnceLock::new();

/// Installs the process's seal service (the key holder, once registered).
pub fn install(service: Arc<dyn SealService>) {
    *write(&SERVICE) = Some(service);
}

/// Removes it (sign-out); nothing seals or opens afterwards.
pub fn uninstall() {
    *write(&SERVICE) = None;
}

pub fn service() -> Option<Arc<dyn SealService>> {
    read(&SERVICE).clone()
}

/// The installed keyring (an empty in-memory one when there is no service).
fn with_keyring<R>(f: impl FnOnce(&Keyring) -> R) -> R {
    match service() {
        Some(service) => f(service.keyring()),
        None => f(FALLBACK.get_or_init(Keyring::in_memory)),
    }
}

/// [`Keyring::changed`] of the installed keyring.
pub fn changed() -> u64 {
    with_keyring(Keyring::changed)
}

/// Feeds the synced `sessions` rows (session → root, current keys).
pub fn note_sessions(rows: &[SessionRow]) {
    with_keyring(|keyring| keyring.note_sessions(rows));
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
    let service = service()?;
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
    /// The session's root is unknown here.
    UnknownRoot,
    /// The tree has no key and none could be created / fetched.
    NoKey(String),
    Invalid(String),
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::NoService => write!(f, "no key holder"),
            SealError::UnknownRoot => write!(f, "unknown session root"),
            SealError::NoKey(why) => write!(f, "no tree key: {why}"),
            SealError::Invalid(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for SealError {}

fn key_for_writing(root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
    let service = service().ok_or(SealError::NoService)?;
    service.ensure_key(root_id).map_err(SealError::NoKey)
}

fn key_for_sending(root_id: &str) -> Result<(u32, [u8; 32]), SealError> {
    let service = service().ok_or(SealError::NoService)?;
    if let Some(current) = service.keyring().current(root_id) {
        return Ok(current);
    }
    match service.public_key(root_id).map_err(SealError::NoKey)? {
        Some(key) => Ok(key),
        // A tree nobody keyed yet: plaintext stays legal there.
        None => Err(SealError::NoKey("the target tree has no key".into())),
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
/// key, never created here. BLOCKING.
pub fn seal_body_for_tree(root_id: &str, text: &str) -> Result<String, SealError> {
    let (epoch, public) = key_for_sending(root_id)?;
    enet_core::sealed::seal_body(&public, epoch, text).map_err(|e| SealError::Invalid(e.to_string()))
}

/// A card sealed for another tree. BLOCKING.
pub fn seal_card_for_tree(root_id: &str, card: &Value) -> Result<Value, SealError> {
    let (epoch, public) = key_for_sending(root_id)?;
    enet_core::sealed::seal_card(&public, epoch, card).map_err(|e| SealError::Invalid(e.to_string()))
}

/// [`seal_body`] for a session (its root resolved), or the plaintext when
/// sealing is impossible (logged): what a writer posts.
pub fn seal_body_or_plain(session_id: &str, text: &str) -> String {
    if text.is_empty() || matches!(parse_body_envelope(text), BodyEnvelope::Value(_)) {
        return text.to_string();
    }
    let sealed = root_of(session_id).ok_or(SealError::UnknownRoot).and_then(|root| seal_body(&root, text));
    match sealed {
        Ok(body) => body,
        Err(SealError::NoService) => text.to_string(),
        Err(err) => {
            log::warn!("sealing: session {session_id} body goes out plaintext: {err}");
            text.to_string()
        }
    }
}

/// [`seal_card`] for a session, or the card as is (logged).
pub fn seal_card_or_plain(session_id: &str, card: &Value) -> Value {
    if card.get("sealed").is_some() {
        return card.clone();
    }
    let sealed = root_of(session_id).ok_or(SealError::UnknownRoot).and_then(|root| seal_card(&root, card));
    match sealed {
        Ok(card) => card,
        Err(SealError::NoService) => card.clone(),
        Err(err) => {
            log::warn!("sealing: session {session_id} card goes out plaintext: {err}");
            card.clone()
        }
    }
}

// --- opening -------------------------------------------------------------

/// Runs `open` with `root`'s secrets; on `noKey` pulls the wraps once (rate
/// limited per root) and tries again. May BLOCK on that pull.
fn opening<T>(root_id: &str, open: impl Fn(&dyn Fn(u32) -> Option<[u8; 32]>) -> (OpenStatus, T)) -> T {
    let service = service();
    let keyring_open = |keyring: &Keyring| open(&|epoch| keyring.secret(root_id, epoch));
    let Some(service) = service else {
        return keyring_open(FALLBACK.get_or_init(Keyring::in_memory)).1;
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

/// The placeholder an agent reads for content this install cannot open.
pub const LOCKED_TEXT: &str = "[sealed content this device cannot open yet]";

/// A message row with its body and card opened (the agent-facing copy).
/// Content this install cannot open reads [`LOCKED_TEXT`]; a locked card
/// keeps its clear part.
pub fn open_message(message: &MessageRow) -> MessageRow {
    let sealed_body = message.body.as_deref().is_some_and(|b| b.starts_with(SEALED_BODY_PREFIX));
    let sealed_card = message.card.as_ref().is_some_and(|c| c.get("sealed").is_some());
    if !sealed_body && !sealed_card {
        return message.clone();
    }
    let root = message
        .root_id
        .clone()
        .filter(|r| !r.is_empty())
        .or_else(|| root_of(&message.session_id))
        .unwrap_or_else(|| message.session_id.clone());
    let mut out = message.clone();
    if sealed_body {
        out.body = Some(open_body_text(&root, message.body_text()).unwrap_or_else(|| LOCKED_TEXT.to_string()));
    }
    if let (true, Some(card)) = (sealed_card, message.card.as_ref()) {
        out.card = Some(open_card(&root, card).card);
    }
    out
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
/// the key travels with the first message in one transaction.
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
    }];
    for (install_id, public) in readers {
        if wraps.iter().any(|w| &w.install_id == install_id) {
            continue;
        }
        let Ok(public) = public_key_from_b64(public) else { continue };
        if let Ok(wrapped) = wrap_tree_key(&public, &key.secret) {
            wraps.push(api::session_keys::InstallWrap { install_id: install_id.clone(), wrapped });
        }
    }
    Ok(NewRootSeal { install_id: own_install_id.to_string(), public_key: key.public_key(), epoch: 1, wraps, secret: key.secret })
}

/// After `sessions.send {seal}` returned the new root: keep its secret.
pub fn adopt_new_root(root_id: &str, seal: &NewRootSeal) {
    with_keyring(|keyring| adopt_into(keyring, root_id, seal));
}

fn adopt_into(keyring: &Keyring, root_id: &str, seal: &NewRootSeal) {
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

    pub fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
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
        fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), String> {
            if let Some(current) = self.keyring.current(root_id) {
                return Ok(current);
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
        fn deliver_direct(&self, to: &DeliverTo, message: &MessageRow) {
            self.delivered.lock().unwrap().push((to.endpoint_id.clone(), message.clone()));
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
        let body = seal_body_or_plain("child", "hello child");
        assert!(body.starts_with("exp-sealed:v1:1:"), "{body}");
        assert_eq!(fake.created.lock().unwrap().as_slice(), ["root"]);
        let card = seal_card_or_plain("child", &serde_json::json!({"kind": "report", "sessionId": "child", "summary": "done"}));
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
    fn locked_content_reads_the_placeholder_and_no_service_stays_plain() {
        let _serial = serial();
        uninstall();
        assert_eq!(seal_body_or_plain("s", "plain"), "plain");
        let fake = install_fake();
        let other = generate_tree_key();
        let body = enet_core::sealed::seal_body(&other.public, 1, "secret").unwrap();
        fake.keyring.note_root("s", "r");
        let row = MessageRow { id: "m".into(), session_id: "s".into(), body: Some(body), ..Default::default() };
        assert_eq!(open_message(&row).body.as_deref(), Some(LOCKED_TEXT));
        uninstall();
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
}
