//! EXP-1457 step 1 (#VAPP-37): every desktop app and CLI daemon install runs
//! one eNet node (`enet-core`) and registers it with the install registry.
//! The ONE place both hosts use: the identity file, the node start on the
//! steer tokio runtime, `installs.register` on every start, and the access
//! allowlist = the endpoint ids `installs.list` returns for every team the
//! user is in (refreshed on start, every [`ALLOWLIST_REFRESH`] and on a team
//! change). The chat channel carries the opt-in [`PING_ENV`] ping and,
//! EXP-1458, agent-to-agent messages ([`DirectPayload`], sealed as stored;
//! received ones land in `coding::direct_inbox`). Once registered the node
//! also runs the install's KEY HOLDER (`seal_keys.rs`), the process's
//! `coding::sealing` service, whose `deliver_direct` sends through here.
//!
//! Identity: 32 secret-key bytes as one lowercase hex line in a 0600 file
//! (the locked file-store posture of `api::token_store`, never the OS
//! keyring), per ACCOUNT because the registry binds an endpoint id to one
//! user: `{data_dir}/accounts/{account_id}/enet-identity` for the desktop app,
//! `cli-enet-identity` beside it for the daemon (the two share a data dir,
//! the same split as `device-id`/`cli-device-id`). Never synced, never
//! uploaded: only the public endpoint id and the derived X25519 key leave.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use enet_core::{EndpointId, Event, Identity, NetConfig, Node, PushKeypair, RelayChoice, RelayUrl, Ticket};
use tokio::sync::{watch, Notify};

use crate::seal_keys::KeyHolder;

pub use enet_core::NetConfig as EnetConfig;

/// How often the allowlist is reloaded besides start and team changes.
pub const ALLOWLIST_REFRESH: Duration = Duration::from_secs(10 * 60);

/// The fastest a refused unknown peer re-reads the allowlist.
pub const REFUSED_REFRESH: Duration = Duration::from_secs(60);

/// How soon a failed `installs.register` is retried.
pub const REGISTER_RETRY: Duration = Duration::from_secs(30);

/// Set (to anything) to ping every allowed peer over chat after each
/// allowlist pass and log the acked round trip: the live check.
pub const PING_ENV: &str = "EXP_ENET_PING";

/// The ping body. Not content: a fixed probe.
pub const PING_BODY: &[u8] = b"exp-enet-ping/1";

/// The type tag of an agent-to-agent message on `exp/enet/chat/1`.
pub const DIRECT_TAG: &str = "exp-msg";

/// How long one agent-to-agent send waits for its ack.
pub const DIRECT_SEND_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a send to a peer missing from the allowlist waits for the one
/// refresh it triggers.
pub const DIRECT_REFRESH_WAIT: Duration = Duration::from_secs(10);

/// EXP-1458 (contract addendum 3): one agent-to-agent message over eNet
/// chat, the sealed fields exactly as the server stored them:
/// `{t:"exp-msg", v:1, messageId, sessionId, rootId, createdAt,
/// authorSessionId, body, card?}`. Never opened in transit.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectPayload {
    pub t: String,
    pub v: u32,
    pub message_id: String,
    pub session_id: String,
    pub root_id: String,
    #[serde(default)]
    pub created_at: Option<String>,
    pub author_session_id: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<serde_json::Value>,
}

/// The chat body for `message` (a sealed row the server accepted), or
/// `None` when it is no agent-to-agent message (no target session, root or
/// sender session, or nothing sealed to carry).
pub fn direct_payload(message: &domain::rows::MessageRow) -> Option<Vec<u8>> {
    let payload = DirectPayload {
        t: DIRECT_TAG.to_string(),
        v: 1,
        message_id: message.id.clone(),
        session_id: message.session_id.clone(),
        root_id: message.root_id.clone().unwrap_or_default(),
        created_at: message.created_at.clone(),
        author_session_id: message.author_session_id.clone().unwrap_or_default(),
        body: message.body.clone(),
        card: message.card.clone(),
    };
    valid(&payload).then(|| serde_json::to_vec(&payload).ok()).flatten()
}

/// A chat body back into the pending-input row the pickup feeds: `None`
/// for anything else (the ping) or a payload that breaks the rules (ids
/// missing, the sender = the target, a body or card that is not sealed).
pub fn parse_direct_payload(body: &[u8]) -> Option<domain::rows::MessageRow> {
    if !body.starts_with(b"{") {
        return None;
    }
    let payload: DirectPayload = serde_json::from_slice(body).ok()?;
    if payload.t != DIRECT_TAG || payload.v != 1 || !valid(&payload) {
        return None;
    }
    Some(domain::rows::MessageRow {
        id: payload.message_id,
        // Keyed by its session the way `key_messages_by_session` keys a
        // synced row; the conversation is the Electric copy's business.
        conversation_id: String::new(),
        session_id: payload.session_id,
        root_id: Some(payload.root_id),
        author_kind: Some(domain::contract::MESSAGE_AUTHOR_KIND_AGENT.to_string()),
        author_session_id: Some(payload.author_session_id),
        body: payload.body,
        card: payload.card,
        created_at: payload.created_at,
        ..Default::default()
    })
}

fn valid(payload: &DirectPayload) -> bool {
    let present = |s: &str| !s.is_empty();
    present(&payload.message_id)
        && present(&payload.session_id)
        && present(&payload.root_id)
        && present(&payload.author_session_id)
        && payload.author_session_id != payload.session_id
        && (payload.body.is_some() || payload.card.is_some())
        && payload.body.as_deref().is_none_or(|b| b.starts_with(enet_core::sealed::SEALED_BODY_PREFIX))
        && payload.card.as_ref().is_none_or(|c| c.get("sealed").is_some_and(serde_json::Value::is_string))
}

/// Which kind of install this process is (`installs.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    Desktop,
    Daemon,
}

impl InstallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallKind::Desktop => "desktop",
            InstallKind::Daemon => "daemon",
        }
    }

    fn identity_file(self) -> &'static str {
        match self {
            InstallKind::Desktop => "enet-identity",
            InstallKind::Daemon => "cli-enet-identity",
        }
    }
}

/// `{data_dir}/accounts/{account_id}/{enet-identity|cli-enet-identity}`.
pub fn identity_path(data_dir: &Path, account_id: &str, kind: InstallKind) -> PathBuf {
    data_dir.join("accounts").join(account_id).join(kind.identity_file())
}

/// The identity in `path`, minted (and written 0600) on first use. A file
/// that exists but is malformed is replaced by a fresh identity (it was
/// never usable, so nothing can be pinned to it); an unreadable one is an
/// error, never minted over.
pub fn load_or_create_identity(path: &Path) -> std::io::Result<Identity> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            if let Some(identity) = parse_identity(&text) {
                return Ok(identity);
            }
            log::warn!("enet: {} is malformed; minting a new identity", path.display());
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let identity = Identity::generate();
    write_identity(path, &identity)?;
    Ok(identity)
}

/// Deletes the identity (after a revoke: the endpoint id can never register
/// again, so the next start mints a new one).
pub fn forget_identity(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => log::warn!("enet: removing {} failed: {err}", path.display()),
    }
}

/// Explicit sign-out from ANOTHER process than the node's (`exponential
/// logout` while the daemon runs or not): revokes the install whose identity
/// is stored at `path` and forgets it. Never mints. BLOCKING.
pub fn revoke_stored_blocking(trpc: &api::TrpcClient, path: &Path) {
    let Some(identity) = std::fs::read_to_string(path).ok().and_then(|text| parse_identity(&text)) else {
        return;
    };
    let endpoint = endpoint_hex(&identity.endpoint_id());
    match api::installs::mine(trpc) {
        Ok(rows) => {
            for row in rows.iter().filter(|row| row.endpoint_id == endpoint && row.revoked_at.is_none()) {
                if let Err(err) = api::installs::revoke(trpc, &row.install_id) {
                    log::warn!("enet: installs.revoke failed: {err}");
                }
            }
        }
        Err(err) => log::warn!("enet: installs.mine failed: {err}"),
    }
    forget_identity(path);
}

fn parse_identity(text: &str) -> Option<Identity> {
    let bytes = data_encoding::HEXLOWER_PERMISSIVE.decode(text.trim().as_bytes()).ok()?;
    Identity::from_slice(&bytes).ok()
}

fn write_identity(path: &Path, identity: &Identity) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    // Write a sibling then rename: a torn write never leaves half a key.
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(data_encoding::HEXLOWER.encode(&identity.to_bytes()).as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

/// Lowercase hex of the 32-byte public key: the registry's `endpointId`.
pub fn endpoint_hex(id: &EndpointId) -> String {
    data_encoding::HEXLOWER.encode(id.as_bytes())
}

/// Parses a registry `endpointId`.
pub fn parse_endpoint(hex: &str) -> Option<EndpointId> {
    let bytes: [u8; 32] = data_encoding::HEXLOWER.decode(hex.as_bytes()).ok()?.try_into().ok()?;
    EndpointId::from_bytes(&bytes).ok()
}

/// The install's X25519 public key (`PushKeypair::derive`), unpadded base64url.
pub fn x25519_public(identity: &Identity) -> String {
    data_encoding::BASE64URL_NOPAD.encode(&PushKeypair::derive(identity).public())
}

/// The UTF-8 string the identity signs (server: `installMessage`).
pub fn install_message(user_id: &str, endpoint_id: &str, x25519_public: &str, signed_at: &str) -> String {
    format!("exp-install-v1\n{user_id}\n{endpoint_id}\n{x25519_public}\n{signed_at}")
}

/// What this install sends to `installs.register`, signed at `signed_at`.
pub struct Registration<'a> {
    pub kind: InstallKind,
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub label: Option<&'a str>,
    pub platform: Option<&'a str>,
}

impl Registration<'_> {
    pub fn sign(&self, identity: &Identity, signed_at: &str) -> RegisterInstall {
        let endpoint_id = endpoint_hex(&identity.endpoint_id());
        let x25519_public = x25519_public(identity);
        let message = install_message(self.user_id, &endpoint_id, &x25519_public, signed_at);
        let signature = data_encoding::BASE64URL_NOPAD.encode(&identity.sign(message.as_bytes()));
        RegisterInstall {
            kind: self.kind.as_str().to_string(),
            label: self.label.map(|label| label.chars().take(64).collect()),
            platform: self.platform.map(str::to_string),
            device_id: self.device_id.map(str::to_string),
            endpoint_id,
            x25519_public,
            signed_at: signed_at.to_string(),
            signature,
        }
    }
}

/// The registry the node talks to: `installs.*` over tRPC in the apps, a
/// fake in the tests. Blocking calls (the tRPC stack is blocking reqwest);
/// the node runs them on tokio's blocking pool.
pub trait InstallRegistry: Send + Sync + 'static {
    /// The install id.
    fn register(&self, input: &RegisterInstall) -> Result<String, ApiError>;
    fn list(&self, team_id: &str) -> Result<Vec<InstallEntry>, ApiError>;
    fn revoke(&self, install_id: &str) -> Result<(), ApiError>;
    /// EXP-1458: the `sessionKeys.*` half, for the install's key holder
    /// (`seal_keys.rs`); `None` = no holder (tests that only register).
    fn keys(self: Arc<Self>) -> Option<Arc<dyn crate::seal_keys::KeyApi>> {
        None
    }
}

impl InstallRegistry for api::TrpcClient {
    fn register(&self, input: &RegisterInstall) -> Result<String, ApiError> {
        api::installs::register(self, input).map(|r| r.install_id)
    }
    fn list(&self, team_id: &str) -> Result<Vec<InstallEntry>, ApiError> {
        api::installs::list(self, team_id)
    }
    fn revoke(&self, install_id: &str) -> Result<(), ApiError> {
        api::installs::revoke(self, install_id)
    }
    fn keys(self: Arc<Self>) -> Option<Arc<dyn crate::seal_keys::KeyApi>> {
        Some(self)
    }
}

/// The ids of every team the user is in, read at call time (the desktop's
/// synced `teams` collection, the daemon's `teams` shape).
pub type TeamIds = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

/// How one install's node is started.
#[derive(Clone)]
pub struct InstallSpec {
    pub kind: InstallKind,
    pub user_id: String,
    /// The `devices.device_id` this host registers (desktop + daemon).
    pub device_id: Option<String>,
    pub label: Option<String>,
    pub identity_path: PathBuf,
    /// `NetConfig::default()` in the apps: https://relay.exponential.at,
    /// n0 relays off. `NetConfig::local_only()` in the tests.
    pub net: NetConfig,
}

/// Brings `node`'s access allowlist to exactly the endpoint ids in
/// `entries` (minus its own) and teaches it to reach each through `relay`.
/// `complete` = every team's list loaded: only then are ids that vanished
/// revoked (a failed fetch never locks teammates out). Returns how many
/// peers are allowed.
pub fn apply_allowlist(node: &Node, entries: &[InstallEntry], relay: Option<&RelayUrl>, complete: bool) -> usize {
    let own = node.endpoint_id();
    let wanted: BTreeSet<EndpointId> = entries
        .iter()
        .filter_map(|entry| parse_endpoint(&entry.endpoint_id))
        .filter(|id| *id != own)
        .collect();
    let access = node.access();
    if complete {
        for id in access.allowed_peers() {
            if !wanted.contains(&id) {
                access.revoke(&id);
            }
        }
    }
    for id in &wanted {
        access.allow(*id);
        if let Some(relay) = relay {
            let mut ticket = Ticket::new(*id);
            ticket.relays = vec![relay.clone()];
            node.add_ticket(&ticket);
        }
    }
    access.allowed_peers().len()
}

struct Inner {
    spec: InstallSpec,
    registry: Arc<dyn InstallRegistry>,
    teams: TeamIds,
    node: Mutex<Option<Arc<Node>>>,
    install_id: Mutex<Option<String>>,
    wake: Notify,
    stopped: AtomicBool,
    /// Counts finished allowlist passes (tests and the live check wait on it).
    passes: watch::Sender<u64>,
    /// EXP-1458: the key holder, once registered (`seal_keys.rs`).
    holder: Mutex<Option<KeyHolder>>,
    holder_wake: Arc<Notify>,
    /// The last allowlist's entries (the user's other installs a new
    /// tree's key is wrapped for).
    entries: Mutex<Vec<InstallEntry>>,
}

/// One install's running eNet node. Cheap to clone; [`InstallNode::stop`]
/// (sign-out) shuts the node down.
#[derive(Clone)]
pub struct InstallNode {
    inner: Arc<Inner>,
}

impl InstallNode {
    /// Starts the node on `runtime` (the steer runtime) and returns at once:
    /// identity, bind, register and the first allowlist pass run there.
    pub fn start(
        runtime: &tokio::runtime::Handle,
        spec: InstallSpec,
        registry: Arc<dyn InstallRegistry>,
        teams: TeamIds,
    ) -> InstallNode {
        let (passes, _) = watch::channel(0);
        let inner = Arc::new(Inner {
            spec,
            registry,
            teams,
            node: Mutex::new(None),
            install_id: Mutex::new(None),
            wake: Notify::new(),
            stopped: AtomicBool::new(false),
            passes,
            holder: Mutex::new(None),
            holder_wake: Arc::new(Notify::new()),
            entries: Mutex::new(Vec::new()),
        });
        runtime.spawn(run(Arc::clone(&inner)));
        InstallNode { inner }
    }

    /// Reload the allowlist now (a team joined or left).
    pub fn refresh(&self) {
        self.inner.wake.notify_one();
    }

    /// The node, once it is up.
    pub fn node(&self) -> Option<Arc<Node>> {
        lock(&self.inner.node).clone()
    }

    /// The registry's id for this install, once registered.
    pub fn install_id(&self) -> Option<String> {
        lock(&self.inner.install_id).clone()
    }

    /// EXP-1458: the install's key holder (the process's
    /// `coding::sealing` service), once registered.
    pub fn key_holder(&self) -> Option<KeyHolder> {
        lock(&self.inner.holder).clone()
    }

    /// EXP-1458: `sessions.send {seal}` for a NEW root: a fresh tree key
    /// wrapped for this install, the user's other installs on the last
    /// allowlist and the install of `target_device` (the `devices.device_id`
    /// that will run the root, when known). Holders fill any gap through
    /// `pending`. [`adopt`] it with the returned root id.
    ///
    /// [`adopt`]: coding::sealing::SealService::adopt
    pub fn new_root_seal(&self, target_device: Option<&str>) -> Result<coding::sealing::NewRootSeal, String> {
        let node = self.node().ok_or("eNet is not running")?;
        let install_id = self.install_id().ok_or("this install is not registered yet")?;
        let push = PushKeypair::derive(node.identity());
        let readers: Vec<(String, String)> = lock(&self.inner.entries)
            .iter()
            .filter(|entry| {
                entry.user_id == self.inner.spec.user_id
                    || target_device.is_some_and(|device| entry.device_id.as_deref() == Some(device))
            })
            .map(|entry| (entry.install_id.clone(), entry.x25519_public.clone()))
            .collect();
        coding::sealing::seal_new_root(&install_id, &push.public(), &readers).map_err(|e| e.to_string())
    }

    /// Waits until at least `n` allowlist passes have finished.
    pub async fn passes_at_least(&self, n: u64) {
        let mut rx = self.inner.passes.subscribe();
        let _ = rx.wait_for(|passes| *passes >= n).await;
    }

    /// Sign-out: shuts the node down. The install stays registered (the
    /// next sign-in of the account reuses it).
    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
        if let Some(holder) = lock(&self.inner.holder).take() {
            holder.stop();
        }
        self.inner.holder_wake.notify_one();
        self.inner.wake.notify_one();
    }

    /// Explicit sign-out of this install: `installs.revoke`, then forget the
    /// identity (a revoked endpoint id never registers again) and stop.
    /// BLOCKING (one tRPC call): run it off the UI thread. `registry` =
    /// a client holding a SNAPSHOT of the session token (sign-out deletes
    /// the stored one while this runs).
    pub fn revoke_blocking(&self, registry: &dyn InstallRegistry) {
        self.stop();
        if let Some(install_id) = self.install_id() {
            match registry.revoke(&install_id) {
                Ok(()) => log::info!("enet: revoked install {install_id}"),
                Err(err) => log::warn!("enet: installs.revoke failed: {err}"),
            }
        }
        forget_identity(&self.inner.spec.identity_path);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn relay_url(net: &NetConfig) -> Option<RelayUrl> {
    match &net.relay {
        RelayChoice::Custom(urls) => urls.first().and_then(|url| url.parse().ok()),
        RelayChoice::Disabled => None,
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

enum Registered {
    Yes,
    /// Transient: retried on the next pass.
    Later,
    /// The registry refused this endpoint for good (revoked / another account).
    Never,
}

async fn register(inner: &Arc<Inner>, identity: &Identity) -> Registered {
    let input = Registration {
        kind: inner.spec.kind,
        user_id: &inner.spec.user_id,
        device_id: inner.spec.device_id.as_deref(),
        label: inner.spec.label.as_deref(),
        platform: Some(std::env::consts::OS),
    }
    .sign(identity, &coding::agent_accounts::now_iso());
    let registry = Arc::clone(&inner.registry);
    match blocking(move || registry.register(&input)).await {
        Some(Ok(install_id)) => {
            log::info!(
                "enet: registered {} install {install_id} as {}",
                inner.spec.kind.as_str(),
                endpoint_hex(&identity.endpoint_id())
            );
            *lock(&inner.install_id) = Some(install_id);
            Registered::Yes
        }
        Some(Err(ApiError::Http { status: 403, .. })) => Registered::Never,
        Some(Err(ApiError::Http { status: 409, message })) => {
            log::warn!("enet: installs.register refused ({message}); minting a new identity");
            Registered::Never
        }
        Some(Err(err)) => {
            log::warn!("enet: installs.register failed (retried next pass): {err}");
            Registered::Later
        }
        None => Registered::Later,
    }
}

async fn run(inner: Arc<Inner>) {
    let path = inner.spec.identity_path.clone();
    let mut fresh_mints = 0;
    let (node, identity, mut registered) = loop {
        let load = path.clone();
        let identity = match blocking(move || load_or_create_identity(&load)).await {
            Some(Ok(identity)) => identity,
            Some(Err(err)) => {
                log::warn!("enet: identity file {} unreadable, eNet off: {err}", path.display());
                return;
            }
            None => return,
        };
        let registered = register(&inner, &identity).await;
        if matches!(registered, Registered::Never) && fresh_mints == 0 {
            // A revoked (or foreign) endpoint can never come back: start over
            // with a new identity, once.
            log::warn!("enet: this install's endpoint was revoked; minting a new identity");
            let forget = path.clone();
            let _ = blocking(move || forget_identity(&forget)).await;
            fresh_mints += 1;
            continue;
        }
        if inner.stopped.load(Ordering::SeqCst) {
            return;
        }
        match Node::start(inner.spec.net.clone(), identity.clone()).await {
            Ok(node) => break (Arc::new(node), identity, registered),
            Err(err) => {
                log::warn!("enet: node failed to start, eNet off: {err}");
                return;
            }
        }
    };
    if let Some(events) = node.events() {
        tokio::spawn(log_events(Arc::clone(&inner), events));
    }
    *lock(&inner.node) = Some(Arc::clone(&node));
    let relay = relay_url(&inner.spec.net);
    let ping = std::env::var_os(PING_ENV).is_some();

    while !inner.stopped.load(Ordering::SeqCst) {
        if matches!(registered, Registered::Later) {
            registered = register(&inner, &identity).await;
        }
        if matches!(registered, Registered::Yes) {
            start_holder(&inner, &identity);
            if let Some(holder) = lock(&inner.holder).as_ref() {
                holder.set_direct_sender(direct_sender(&inner));
            }
        }
        let allowed = refresh_allowlist(&inner, &node, relay.as_ref()).await;
        if ping {
            ping_peers(&node).await;
        }
        log::debug!("enet: allowlist pass done ({allowed} peers)");
        inner.passes.send_modify(|passes| *passes += 1);
        // Unregistered (offline at boot, or the devices row not written
        // yet): retry soon instead of in ten minutes.
        let wait = match registered {
            Registered::Later => REGISTER_RETRY,
            _ => ALLOWLIST_REFRESH,
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = inner.wake.notified() => {}
        }
    }
    *lock(&inner.node) = None;
    if let Err(err) = node.shutdown().await {
        log::debug!("enet: node shutdown: {err}");
    }
    log::info!("enet: node stopped");
}

/// EXP-1458: starts the key holder once the install has its registry id
/// and installs it as the process's seal service. Idempotent.
fn start_holder(inner: &Arc<Inner>, identity: &Identity) {
    if lock(&inner.holder).is_some() || inner.stopped.load(Ordering::SeqCst) {
        return;
    }
    let Some(install_id) = lock(&inner.install_id).clone() else { return };
    let Some(api) = Arc::clone(&inner.registry).keys() else { return };
    let path = inner.spec.identity_path.with_file_name(match inner.spec.kind {
        InstallKind::Desktop => "seal-keys",
        InstallKind::Daemon => "cli-seal-keys",
    });
    let holder = KeyHolder::new(install_id, &PushKeypair::derive(identity), api, coding::sealing::Keyring::load(path));
    coding::sealing::install(Arc::new(holder.clone()));
    holder.spawn(&tokio::runtime::Handle::current(), Arc::clone(&inner.holder_wake));
    *lock(&inner.holder) = Some(holder);
    log::info!("enet: key holder running");
}

/// EXP-1458: the key holder's agent-to-agent sender: the sealed row as a
/// [`DirectPayload`] over chat to the target install, on the steer runtime
/// (fire and forget, acked or logged).
fn direct_sender(inner: &Arc<Inner>) -> crate::seal_keys::DirectSender {
    let weak = Arc::downgrade(inner);
    let runtime = tokio::runtime::Handle::current();
    Arc::new(move |to: &coding::sealing::DeliverTo, message: &domain::rows::MessageRow| {
        let Some(body) = direct_payload(message) else {
            log::debug!("enet: message {} is no direct message; Electric delivers it", message.id);
            return;
        };
        let Some(peer) = parse_endpoint(&to.endpoint_id) else {
            log::warn!("enet: deliverTo endpoint {:?} is malformed", to.endpoint_id);
            return;
        };
        runtime.spawn(send_direct(weak.clone(), peer, message.id.clone(), body));
    })
}

async fn send_direct(inner: Weak<Inner>, peer: EndpointId, message_id: String, body: Vec<u8>) {
    let Some(inner) = inner.upgrade() else { return };
    let Some(node) = lock(&inner.node).clone() else {
        log::debug!("enet: node down; Electric delivers {message_id}");
        return;
    };
    if !node.access().allowed_peers().contains(&peer) {
        // Maybe the target registered after our last list: refresh once.
        let mut passes = inner.passes.subscribe();
        let seen = *passes.borrow();
        inner.wake.notify_one();
        let _ = tokio::time::timeout(DIRECT_REFRESH_WAIT, passes.wait_for(|n| *n > seen)).await;
        if !node.access().allowed_peers().contains(&peer) {
            log::info!("enet: {} is no team install; Electric delivers {message_id}", endpoint_hex(&peer));
            return;
        }
    }
    let started = std::time::Instant::now();
    match tokio::time::timeout(DIRECT_SEND_TIMEOUT, node.send_chat(peer, body)).await {
        Ok(Ok(_)) => log::info!(
            "enet: direct {message_id} → {} acked in {:?}",
            endpoint_hex(&peer),
            started.elapsed()
        ),
        Ok(Err(err)) => log::info!("enet: direct {message_id} → {} failed: {err}", endpoint_hex(&peer)),
        Err(_) => log::info!("enet: direct {message_id} → {} timed out", endpoint_hex(&peer)),
    }
}

/// EXP-1458: an agent-to-agent message from an allowed peer goes into the
/// process's direct inbox (still sealed); anything else is only logged.
fn receive_chat(inner: &Inner, from: &EndpointId, body: &[u8]) {
    if body == PING_BODY {
        log::info!("enet: ping from {} (acked)", endpoint_hex(from));
        return;
    }
    let Some(row) = parse_direct_payload(body) else {
        log::info!("enet: chat from {} ({} bytes, acked)", endpoint_hex(from), body.len());
        return;
    };
    let allowed = lock(&inner.node).as_ref().is_some_and(|node| node.access().allowed_peers().contains(from));
    if !allowed {
        log::warn!("enet: direct message from {} outside the allowlist dropped", endpoint_hex(from));
        return;
    }
    log::info!("enet: direct {} for session {} from {}", row.id, row.session_id, endpoint_hex(from));
    coding::direct_inbox::push(row);
}

async fn refresh_allowlist(inner: &Arc<Inner>, node: &Arc<Node>, relay: Option<&RelayUrl>) -> usize {
    let teams = Arc::clone(&inner.teams);
    let registry = Arc::clone(&inner.registry);
    let Some((entries, complete)) = blocking(move || {
        let mut entries = Vec::new();
        let mut complete = true;
        for team_id in teams() {
            match registry.list(&team_id) {
                Ok(rows) => entries.extend(rows),
                Err(err) => {
                    log::warn!("enet: installs.list {team_id} failed: {err}");
                    complete = false;
                }
            }
        }
        (entries, complete)
    })
    .await
    else {
        return node.access().allowed_peers().len();
    };
    let allowed = apply_allowlist(node, &entries, relay, complete);
    *lock(&inner.entries) = entries;
    allowed
}

async fn ping_peers(node: &Arc<Node>) {
    for peer in node.access().allowed_peers() {
        let started = std::time::Instant::now();
        let sent = tokio::time::timeout(Duration::from_secs(20), node.send_chat(peer, PING_BODY.to_vec())).await;
        match sent {
            Ok(Ok(_)) => log::info!("enet: ping {} acked in {:?}", endpoint_hex(&peer), started.elapsed()),
            Ok(Err(err)) => log::info!("enet: ping {} failed: {err}", endpoint_hex(&peer)),
            Err(_) => log::info!("enet: ping {} timed out", endpoint_hex(&peer)),
        }
    }
}

async fn log_events(inner: Arc<Inner>, mut events: tokio::sync::mpsc::UnboundedReceiver<Event>) {
    let mut last_refused_refresh: Option<std::time::Instant> = None;
    while let Some(event) = events.recv().await {
        match event {
            Event::Chat { from, message } => receive_chat(&inner, &from, &message.body),
            Event::PeerRefused { peer, .. } => {
                log::info!("enet: refused {}", endpoint_hex(&peer));
                // Maybe a teammate's install registered after our last
                // list: re-read it (at most once per REFUSED_REFRESH).
                if last_refused_refresh.is_none_or(|at| at.elapsed() >= REFUSED_REFRESH) {
                    last_refused_refresh = Some(std::time::Instant::now());
                    inner.wake.notify_one();
                }
            }
            _ => {}
        }
    }
}
