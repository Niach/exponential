//! Everything that runs an iroh node (feature `node`, default): the
//! facade's tokio runtime, `EnetNode`, its config/records/events, and the
//! identity + ticket helpers.

use std::{
    str::FromStr,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use enet_core as net;
use net::{EndpointId, Node};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{bytes32, invalid, EnetError};

// --- runtime -----------------------------------------------------------

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("enet")
            .enable_all()
            .build()
            .expect("eNet tokio runtime")
    })
}

/// Runs `fut` on the facade runtime; the returned future can be polled by
/// any executor (UniFFI's foreign futures).
async fn on_rt<T: Send + 'static>(fut: impl std::future::Future<Output = Result<T, EnetError>> + Send + 'static) -> Result<T, EnetError> {
    match runtime().spawn(fut).await {
        Ok(r) => r,
        Err(e) if e.is_panic() => Err(EnetError::Panic { reason: panic_text(e.into_panic()) }),
        Err(e) => Err(EnetError::Closed { reason: e.to_string() }),
    }
}

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}

fn peer_id(s: &str) -> Result<EndpointId, EnetError> {
    EndpointId::from_str(s.trim()).map_err(|e| invalid(format!("endpoint id {s}: {e}")))
}

// --- records -----------------------------------------------------------

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetConfig {
    /// Our relay(s); empty = `https://relay.exponential.at`.
    pub relay_urls: Vec<String>,
    /// false = direct addresses only (tests, LAN).
    pub relays_enabled: bool,
    /// Also use number 0's public relays.
    pub n0_relays: bool,
    pub relay_auth_token: Option<String>,
    /// `ip:port` to bind the UDP socket to (default: any, port 0).
    pub bind_addr: Option<String>,
    /// Persist blobs here (needs the `fs-store` feature, on by default).
    pub blob_dir: Option<String>,
    /// Endpoint ids allowed to connect from the start.
    pub allowed_peers: Vec<String>,
    /// Accept every peer (pairing window).
    pub allow_all: bool,
}

impl EnetConfig {
    fn to_net(&self) -> Result<net::NetConfig, EnetError> {
        let relay = if !self.relays_enabled {
            net::RelayChoice::Disabled
        } else if self.relay_urls.is_empty() {
            net::RelayChoice::default()
        } else {
            net::RelayChoice::Custom(self.relay_urls.clone())
        };
        Ok(net::NetConfig {
            relay,
            n0_relays: self.n0_relays,
            relay_auth_token: self.relay_auth_token.clone(),
            bind_addr: self
                .bind_addr
                .as_deref()
                .map(|a| a.parse().map_err(|e| invalid(format!("bind addr {a}: {e}"))))
                .transpose()?,
            blob_dir: self.blob_dir.clone().map(Into::into),
            allowed_peers: self.allowed_peers.iter().map(|p| peer_id(p)).collect::<Result<_, _>>()?,
            allow_all: self.allow_all,
        })
    }
}

/// The default config: our relay, nobody allowed yet.
#[uniffi::export]
pub fn default_config() -> EnetConfig {
    EnetConfig {
        relay_urls: vec![net::DEFAULT_RELAY_URL.to_string()],
        relays_enabled: true,
        n0_relays: false,
        relay_auth_token: None,
        bind_addr: None,
        blob_dir: None,
        allowed_peers: Vec::new(),
        allow_all: false,
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetChatMessage {
    pub id: Vec<u8>,
    pub timestamp_ms: u64,
    pub body: Vec<u8>,
}

impl From<net::ChatMessage> for EnetChatMessage {
    fn from(m: net::ChatMessage) -> Self {
        Self { id: m.id.to_vec(), timestamp_ms: m.timestamp_ms, body: m.body }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetRecord {
    pub space: Vec<u8>,
    pub key: String,
    pub value: Vec<u8>,
    pub author: String,
    pub clock_millis: u64,
    pub clock_counter: u32,
}

impl From<net::Record> for EnetRecord {
    fn from(r: net::Record) -> Self {
        Self {
            space: r.space.to_vec(),
            key: r.key,
            value: r.value,
            author: r.author.to_string(),
            clock_millis: r.clock.millis,
            clock_counter: r.clock.counter,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetBlob {
    pub hash: Vec<u8>,
    pub hash_hex: String,
    pub size: u64,
    /// iroh `blob…` ticket (this node + the hash).
    pub ticket: String,
}

impl From<net::BlobRef> for EnetBlob {
    fn from(b: net::BlobRef) -> Self {
        Self { hash: b.hash.to_vec(), hash_hex: b.hash_hex(), size: b.size, ticket: b.ticket }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnetTicket {
    pub endpoint_id: String,
    pub relays: Vec<String>,
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum EnetEvent {
    Chat { from: String, message: EnetChatMessage },
    Record { record: EnetRecord },
    CallIncoming { call: u64, from: String },
    CallAccepted { call: u64 },
    CallFrame { call: u64, seq: u32, data: Vec<u8> },
    /// `reason` = `local` | `remote` | `failed: …`.
    CallEnded { call: u64, reason: String },
    PeerRefused { peer: String, alpn: String },
}

impl From<net::Event> for EnetEvent {
    fn from(e: net::Event) -> Self {
        match e {
            net::Event::Chat { from, message } => EnetEvent::Chat { from: from.to_string(), message: message.into() },
            net::Event::Record { record } => EnetEvent::Record { record: record.into() },
            net::Event::CallIncoming { call, from } => EnetEvent::CallIncoming { call, from: from.to_string() },
            net::Event::CallAccepted { call } => EnetEvent::CallAccepted { call },
            net::Event::CallFrame { call, seq, data } => EnetEvent::CallFrame { call, seq, data },
            net::Event::CallEnded { call, reason } => EnetEvent::CallEnded {
                call,
                reason: match reason {
                    net::CallEndReason::Local => "local".into(),
                    net::CallEndReason::Remote => "remote".into(),
                    net::CallEndReason::Failed(r) => format!("failed: {r}"),
                },
            },
            net::Event::PeerRefused { peer, alpn } => {
                EnetEvent::PeerRefused { peer: peer.to_string(), alpn: String::from_utf8_lossy(&alpn).into_owned() }
            }
        }
    }
}

// --- foreign traits ----------------------------------------------------

/// Receives a node's events, on an eNet thread: hop to the UI thread
/// yourself, and never block.
#[uniffi::export(with_foreign)]
pub trait EnetListener: Send + Sync {
    fn on_event(&self, event: EnetEvent);
}

/// The app's access decision for peers outside the allowed set
/// (`peer` = endpoint id, `alpn` = the channel). Called on an eNet thread.
#[uniffi::export(with_foreign)]
pub trait EnetAccessPolicy: Send + Sync {
    fn allow(&self, peer: String, alpn: String) -> bool;
}

// --- the node ----------------------------------------------------------

#[derive(uniffi::Object)]
pub struct EnetNode {
    node: Arc<Node>,
    events: Arc<tokio::sync::Mutex<UnboundedReceiver<net::Event>>>,
    listener_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[uniffi::export]
impl EnetNode {
    /// Starts a node with a 32-byte identity (see `generateIdentity`).
    #[uniffi::constructor]
    pub async fn start(config: EnetConfig, identity: Vec<u8>) -> Result<Arc<EnetNode>, EnetError> {
        let net_config = config.to_net()?;
        let identity = net::Identity::from_slice(&identity)?;
        let node = on_rt(async move { Ok(Node::start(net_config, identity).await?) }).await?;
        let events = node.events().expect("fresh node has its events");
        Ok(Arc::new(EnetNode {
            node: Arc::new(node),
            events: Arc::new(tokio::sync::Mutex::new(events)),
            listener_task: Mutex::new(None),
        }))
    }

    pub fn endpoint_id(&self) -> String {
        self.node.endpoint_id().to_string()
    }

    /// The node's `enet1…` ticket (for a QR code or a link).
    pub fn ticket(&self) -> String {
        self.node.ticket().to_string()
    }

    /// Waits until the home relay is connected (no-op without relays).
    pub async fn online(&self, timeout_ms: u64) -> Result<(), EnetError> {
        let node = self.node.clone();
        on_rt(async move { Ok(node.online(Duration::from_millis(timeout_ms)).await?) }).await
    }

    /// Learns a peer's addresses from its `enet1…` ticket; returns its id.
    /// Does NOT allow it in (`allowPeer`).
    pub fn add_ticket(&self, ticket: String) -> Result<String, EnetError> {
        let t = net::Ticket::from_str(&ticket)?;
        self.node.add_ticket(&t);
        Ok(t.endpoint_id.to_string())
    }

    pub fn allow_peer(&self, peer: String) -> Result<(), EnetError> {
        self.node.access().allow(peer_id(&peer)?);
        Ok(())
    }

    pub fn revoke_peer(&self, peer: String) -> Result<(), EnetError> {
        self.node.access().revoke(&peer_id(&peer)?);
        Ok(())
    }

    pub fn allowed_peers(&self) -> Vec<String> {
        self.node.access().allowed_peers().iter().map(|p| p.to_string()).collect()
    }

    /// Accept everyone (e.g. while a pairing QR code is on screen).
    pub fn set_open(&self, open: bool) {
        self.node.access().set_open(open)
    }

    pub fn set_access_policy(&self, policy: Option<Arc<dyn EnetAccessPolicy>>) {
        self.node.access().set_hook(policy.map(|p| {
            move |peer: EndpointId, alpn: &[u8]| p.allow(peer.to_string(), String::from_utf8_lossy(alpn).into_owned())
        }));
    }

    /// Delivers every event to `listener` (replaces a previous one); `None`
    /// stops the delivery (events queue for `nextEvent` again).
    pub fn set_listener(&self, listener: Option<Arc<dyn EnetListener>>) {
        let mut slot = self.listener_task.lock().expect("listener lock");
        if let Some(t) = slot.take() {
            t.abort();
        }
        if let Some(l) = listener {
            let events = self.events.clone();
            *slot = Some(runtime().spawn(async move {
                let mut rx = events.lock().await;
                while let Some(e) = rx.recv().await {
                    let l = l.clone();
                    // a throwing/panicking listener must not end the pump
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || l.on_event(e.into())));
                }
            }));
        }
    }

    /// The next event; `None` once the node is shut down.
    pub async fn next_event(&self) -> Option<EnetEvent> {
        let events = self.events.clone();
        on_rt(async move { Ok(events.lock().await.recv().await.map(EnetEvent::from)) }).await.ok().flatten()
    }

    // chat

    /// Sends and waits for the peer's acknowledgement.
    pub async fn send_chat(&self, peer: String, body: Vec<u8>) -> Result<EnetChatMessage, EnetError> {
        let (node, peer) = (self.node.clone(), peer_id(&peer)?);
        on_rt(async move { Ok(node.send_chat(peer, body).await?.into()) }).await
    }

    // files

    pub async fn blob_add(&self, data: Vec<u8>) -> Result<EnetBlob, EnetError> {
        let node = self.node.clone();
        on_rt(async move { Ok(node.blob_add(data).await?.into()) }).await
    }

    pub async fn blob_add_path(&self, path: String) -> Result<EnetBlob, EnetError> {
        let node = self.node.clone();
        on_rt(async move { Ok(node.blob_add_path(std::path::Path::new(&path)).await?.into()) }).await
    }

    pub async fn blob_get(&self, hash: Vec<u8>) -> Result<Option<Vec<u8>>, EnetError> {
        let (node, hash) = (self.node.clone(), bytes32(&hash, "hash")?);
        on_rt(async move { Ok(node.blob_get(hash).await?) }).await
    }

    pub async fn blob_fetch(&self, peer: String, hash: Vec<u8>) -> Result<Vec<u8>, EnetError> {
        let (node, peer, hash) = (self.node.clone(), peer_id(&peer)?, bytes32(&hash, "hash")?);
        on_rt(async move { Ok(node.blob_fetch(peer, hash).await?) }).await
    }

    pub async fn blob_fetch_ticket(&self, ticket: String) -> Result<Vec<u8>, EnetError> {
        let node = self.node.clone();
        on_rt(async move { Ok(node.blob_fetch_ticket(&ticket).await?) }).await
    }

    // records

    /// Subscribes to a space (32 bytes): its records arrive as
    /// `EnetEvent.Record`, and its state is pulled from neighbours.
    pub async fn join_space(&self, space: Vec<u8>, bootstrap: Vec<String>) -> Result<(), EnetError> {
        let space = bytes32(&space, "space")?;
        let bootstrap = bootstrap.iter().map(|p| peer_id(p)).collect::<Result<Vec<_>, _>>()?;
        let node = self.node.clone();
        on_rt(async move { Ok(node.join_space(space, bootstrap).await?) }).await
    }

    pub fn leave_space(&self, space: Vec<u8>) -> Result<(), EnetError> {
        self.node.leave_space(&bytes32(&space, "space")?);
        Ok(())
    }

    pub async fn put_record(&self, space: Vec<u8>, key: String, value: Vec<u8>) -> Result<EnetRecord, EnetError> {
        let (node, space) = (self.node.clone(), bytes32(&space, "space")?);
        on_rt(async move { Ok(node.put_record(space, key, value).await?.into()) }).await
    }

    pub fn get_record(&self, space: Vec<u8>, key: String) -> Result<Option<EnetRecord>, EnetError> {
        Ok(self.node.get_record(&bytes32(&space, "space")?, &key).map(Into::into))
    }

    pub fn records(&self, space: Vec<u8>) -> Result<Vec<EnetRecord>, EnetError> {
        Ok(self.node.records(&bytes32(&space, "space")?).into_iter().map(Into::into).collect())
    }

    // calls

    /// Rings the peer; `EnetEvent.CallAccepted` follows when it answers.
    pub async fn start_call(&self, peer: String) -> Result<u64, EnetError> {
        let (node, peer) = (self.node.clone(), peer_id(&peer)?);
        on_rt(async move { Ok(node.start_call(peer).await?) }).await
    }

    pub async fn accept_call(&self, call: u64) -> Result<(), EnetError> {
        let node = self.node.clone();
        on_rt(async move { Ok(node.accept_call(call).await?) }).await
    }

    /// Sends one encoded audio frame (an Opus packet); returns its sequence number.
    pub async fn send_frame(&self, call: u64, frame: Vec<u8>) -> Result<u32, EnetError> {
        let node = self.node.clone();
        on_rt(async move { Ok(node.send_frame(call, &frame).await?) }).await
    }

    pub fn hang_up(&self, call: u64) -> Result<(), EnetError> {
        Ok(self.node.hang_up(call)?)
    }

    /// Closes every connection and stops serving.
    pub async fn shutdown(&self) -> Result<(), EnetError> {
        if let Some(t) = self.listener_task.lock().expect("listener lock").take() {
            t.abort();
        }
        let node = self.node.clone();
        on_rt(async move { Ok(node.shutdown().await?) }).await
    }
}

// --- free functions ----------------------------------------------------

/// 32 fresh random identity bytes (store them in the keychain).
#[uniffi::export]
pub fn generate_identity() -> Vec<u8> {
    net::Identity::generate().to_bytes().to_vec()
}

/// The endpoint id an identity has.
#[uniffi::export]
pub fn endpoint_id_of(identity: Vec<u8>) -> Result<String, EnetError> {
    Ok(net::Identity::from_slice(&identity)?.endpoint_id().to_string())
}

/// The identity's 64-byte ed25519 signature over `message` (verifiable
/// against its endpoint id), e.g. the `exp-install-v1` registration string.
#[uniffi::export]
pub fn identity_sign(identity: Vec<u8>, message: Vec<u8>) -> Result<Vec<u8>, EnetError> {
    Ok(net::Identity::from_slice(&identity)?.sign(&message).to_vec())
}

/// Parses (validates) an `enet1…` ticket.
#[uniffi::export]
pub fn parse_ticket(ticket: String) -> Result<EnetTicket, EnetError> {
    let t = net::Ticket::from_str(&ticket)?;
    Ok(EnetTicket {
        endpoint_id: t.endpoint_id.to_string(),
        relays: t.relays.iter().map(|r| r.to_string()).collect(),
        addrs: t.addrs.iter().map(|a| a.to_string()).collect(),
    })
}

