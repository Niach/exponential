//! EXP-1457 step 1 (#VAPP-37): every desktop app and CLI daemon install runs
//! one eNet node (`enet-core`) and registers it with the install registry.
//! The ONE place both hosts use: the identity file, the node start on the
//! steer tokio runtime, `installs.register` on every start, and the access
//! allowlist = the endpoint ids `installs.list` returns for every team the
//! user is in (refreshed on start, every [`ALLOWLIST_REFRESH`] and on a team
//! change). No content travels here yet (#EXP-1458/#EXP-1459): the chat
//! channel only carries the opt-in [`PING_ENV`] ping.
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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use enet_core::{EndpointId, Event, Identity, NetConfig, Node, PushKeypair, RelayChoice, RelayUrl, Ticket};
use tokio::sync::{watch, Notify};

pub use enet_core::NetConfig as EnetConfig;

/// How often the allowlist is reloaded besides start and team changes.
pub const ALLOWLIST_REFRESH: Duration = Duration::from_secs(10 * 60);

/// How soon a failed `installs.register` is retried.
pub const REGISTER_RETRY: Duration = Duration::from_secs(30);

/// Set (to anything) to ping every allowed peer over chat after each
/// allowlist pass and log the acked round trip: the live check.
pub const PING_ENV: &str = "EXP_ENET_PING";

/// The ping body. Not content: a fixed probe.
pub const PING_BODY: &[u8] = b"exp-enet-ping/1";

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

    /// Waits until at least `n` allowlist passes have finished.
    pub async fn passes_at_least(&self, n: u64) {
        let mut rx = self.inner.passes.subscribe();
        let _ = rx.wait_for(|passes| *passes >= n).await;
    }

    /// Sign-out: shuts the node down. The install stays registered (the
    /// next sign-in of the account reuses it).
    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
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
        tokio::spawn(log_events(events));
    }
    *lock(&inner.node) = Some(Arc::clone(&node));
    let relay = relay_url(&inner.spec.net);
    let ping = std::env::var_os(PING_ENV).is_some();

    while !inner.stopped.load(Ordering::SeqCst) {
        if matches!(registered, Registered::Later) {
            registered = register(&inner, &identity).await;
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
    apply_allowlist(node, &entries, relay, complete)
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

async fn log_events(mut events: tokio::sync::mpsc::UnboundedReceiver<Event>) {
    while let Some(event) = events.recv().await {
        match event {
            Event::Chat { from, message } => {
                log::info!("enet: chat from {} ({} bytes, acked)", endpoint_hex(&from), message.body.len())
            }
            Event::PeerRefused { peer, .. } => log::info!("enet: refused {}", endpoint_hex(&peer)),
            _ => {}
        }
    }
}
