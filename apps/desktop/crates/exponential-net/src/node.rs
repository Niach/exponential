use std::sync::{Arc, Mutex};

use iroh::{
    address_lookup::MemoryLookup,
    endpoint::{presets, AfterHandshakeOutcome, Connection, EndpointHooks, Side},
    protocol::Router,
    Endpoint, EndpointAddr, EndpointId, RelayMode,
};
use iroh_blobs::{api::Store, BlobsProtocol};
use iroh_gossip::Gossip;
use tokio::sync::mpsc;

use crate::{
    access::Access,
    blobs::{self, BlobRef},
    call::{self, CallId, CallProtocol, CALL_ALPN},
    chat::{self, ChatMessage, ChatProtocol, CHAT_ALPN},
    config::NetConfig,
    error::{NetError, Result},
    event::Event,
    identity::Identity,
    records::{self, Record, SpaceId, SyncProtocol, GOSSIP_ALPN, GOSSIP_MAX_MESSAGE, RECORDS_SYNC_ALPN},
    ticket::Ticket,
};

/// State shared by the node handle and its protocol handlers.
pub(crate) struct Shared {
    pub(crate) identity: Identity,
    pub(crate) endpoint: Endpoint,
    pub(crate) lookup: MemoryLookup,
    pub(crate) relays: Vec<iroh::RelayUrl>,
    pub(crate) store: Store,
    pub(crate) gossip: Gossip,
    pub(crate) chat: chat::ChatState,
    pub(crate) calls: call::CallState,
    pub(crate) records: records::RecordsState,
    events: mpsc::UnboundedSender<Event>,
}

impl Shared {
    pub(crate) fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub(crate) fn add_peer(&self, addr: EndpointAddr) {
        if !addr.is_empty() {
            self.lookup.add_endpoint_info(addr);
        }
    }

    pub(crate) async fn connect(&self, peer: EndpointId, alpn: &[u8]) -> Result<Connection> {
        if peer == self.endpoint.id() {
            return Err(NetError::Invalid("cannot connect to self".into()));
        }
        self.endpoint.connect(peer, alpn).await.map_err(|e| {
            let reason = e.to_string();
            let refused = format!("{e:?}").contains("NOT_ALLOWED") || reason.contains("not allowed");
            if refused {
                NetError::Refused(reason)
            } else {
                NetError::Connect { peer: peer.to_string(), reason }
            }
        })
    }

    pub(crate) fn ticket(&self) -> Ticket {
        let mut t = Ticket::from_addr(&self.endpoint.addr());
        #[cfg(not(wasm_browser))]
        for a in self.endpoint.bound_sockets() {
            if !a.ip().is_unspecified() && !t.addrs.contains(&a) {
                t.addrs.push(a);
            }
        }
        if t.relays.is_empty() {
            t.relays = self.relays.clone();
        }
        t
    }
}

/// Refuses incoming connections the [`Access`] policy does not allow. Runs
/// after the TLS handshake, so the remote id is authenticated.
#[derive(Debug)]
struct AccessHook {
    access: Access,
    events: mpsc::UnboundedSender<Event>,
}

/// The QUIC application error code an access refusal closes with.
pub(crate) const NOT_ALLOWED_CODE: u32 = 403;

/// `Some(Refused)` when the peer closed `conn` because its access policy
/// does not let us in (the dial itself succeeds: the refusal comes right
/// after the handshake).
pub(crate) fn refusal(conn: &Connection) -> Option<NetError> {
    match conn.close_reason()? {
        iroh::endpoint::ConnectionError::ApplicationClosed(c) if c.error_code == NOT_ALLOWED_CODE.into() => {
            Some(NetError::Refused(format!("{} does not allow this node", conn.remote_id().fmt_short())))
        }
        _ => None,
    }
}

/// `map_err` adapter: a transport failure on `conn`, or `Refused`.
pub(crate) fn on_conn<E: std::fmt::Display>(conn: &Connection) -> impl Fn(E) -> NetError + '_ {
    move |e| classify(conn, crate::error::transport(e))
}

/// Maps a failure on `conn` to `Refused` when the peer refused us.
pub(crate) fn classify(conn: &Connection, err: NetError) -> NetError {
    refusal(conn).unwrap_or(err)
}

impl EndpointHooks for AccessHook {
    async fn after_handshake<'a>(&'a self, conn: &'a Connection) -> AfterHandshakeOutcome {
        if matches!(conn.side(), Side::Server) {
            let peer = conn.remote_id();
            if !self.access.is_allowed(&peer, conn.alpn()) {
                let _ = self.events.send(Event::PeerRefused { peer, alpn: conn.alpn().to_vec() });
                return AfterHandshakeOutcome::Reject { error_code: NOT_ALLOWED_CODE.into(), reason: b"not allowed".to_vec() };
            }
        }
        AfterHandshakeOutcome::Accept
    }
}

/// One eNet node: an iroh endpoint + router serving chat, blobs, records
/// (gossip + sync) and calls. Cheap to share behind an `Arc`; every method
/// takes `&self`.
pub struct Node {
    shared: Arc<Shared>,
    router: Router,
    access: Access,
    events: Mutex<Option<mpsc::UnboundedReceiver<Event>>>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({})", self.endpoint_id().fmt_short())
    }
}

impl Node {
    /// Binds the endpoint and starts serving. Needs a tokio runtime on
    /// native hosts (the browser runs on the JS event loop).
    pub async fn start(config: NetConfig, identity: Identity) -> Result<Node> {
        let relay_mode = config.relay_mode()?;
        let relays: Vec<iroh::RelayUrl> = match &relay_mode {
            RelayMode::Custom(map) => map.urls(),
            _ => Vec::new(),
        };
        #[cfg(wasm_browser)]
        if relays.is_empty() {
            return Err(NetError::Invalid("browsers need a relay".into()));
        }
        let access = Access::default();
        access.set_open(config.allow_all);
        for p in &config.allowed_peers {
            access.allow(*p);
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let lookup = MemoryLookup::new();
        #[allow(unused_mut)]
        let mut builder = Endpoint::builder(presets::Minimal)
            .secret_key(identity.secret_key().clone())
            .relay_mode(relay_mode)
            .address_lookup(lookup.clone())
            .hooks(AccessHook { access: access.clone(), events: tx.clone() });
        #[cfg(not(wasm_browser))]
        if let Some(addr) = config.bind_addr {
            builder = builder.bind_addr(addr).map_err(|e| NetError::Bind(e.to_string()))?;
        }
        let endpoint = builder.bind().await.map_err(|e| NetError::Bind(e.to_string()))?;
        let store = blobs::open_store(&config).await?;
        let gossip = Gossip::builder().max_message_size(GOSSIP_MAX_MESSAGE).alpn(GOSSIP_ALPN).spawn(endpoint.clone());
        let shared = Arc::new(Shared {
            identity,
            endpoint: endpoint.clone(),
            lookup,
            relays,
            store: store.clone(),
            gossip: gossip.clone(),
            chat: Default::default(),
            calls: Default::default(),
            records: Default::default(),
            events: tx,
        });
        let weak = Arc::downgrade(&shared);
        let router = Router::builder(endpoint)
            .accept(CHAT_ALPN, ChatProtocol { shared: weak.clone() })
            .accept(CALL_ALPN, CallProtocol { shared: weak.clone() })
            .accept(GOSSIP_ALPN, gossip)
            .accept(RECORDS_SYNC_ALPN, SyncProtocol { shared: weak })
            .accept(iroh_blobs::ALPN, BlobsProtocol::new(&store, None))
            .spawn();
        Ok(Node { shared, router, access, events: Mutex::new(Some(rx)) })
    }

    // --- identity, addressing, access ---------------------------------

    pub fn endpoint_id(&self) -> EndpointId {
        self.shared.endpoint.id()
    }
    pub fn identity(&self) -> &Identity {
        &self.shared.identity
    }
    /// The underlying iroh endpoint (escape hatch; prefer the channels).
    pub fn endpoint(&self) -> &Endpoint {
        &self.shared.endpoint
    }
    /// The node's current address card (home relay + direct addresses).
    pub fn ticket(&self) -> Ticket {
        self.shared.ticket()
    }
    /// Waits until the node is reachable through its home relay (no-op
    /// without relays). Times out after `timeout`.
    pub async fn online(&self, timeout: std::time::Duration) -> Result<()> {
        if self.shared.relays.is_empty() {
            return Ok(());
        }
        n0_future::time::timeout(timeout, self.shared.endpoint.online())
            .await
            .map_err(|_| NetError::Timeout("relay connection".into()))
    }
    pub fn access(&self) -> &Access {
        &self.access
    }
    /// Teaches the node how to reach a peer (from a ticket, a QR code, the
    /// app's contact list). Does not allow the peer in: see [`Node::access`].
    pub fn add_peer(&self, addr: EndpointAddr) {
        self.shared.add_peer(addr)
    }
    pub fn add_ticket(&self, ticket: &Ticket) {
        self.shared.add_peer(ticket.to_addr())
    }
    /// The event stream (taken once; later calls return `None`).
    pub fn events(&self) -> Option<mpsc::UnboundedReceiver<Event>> {
        self.events.lock().expect("events lock").take()
    }

    // --- chat ----------------------------------------------------------

    /// Sends a message and waits for the peer's acknowledgement.
    pub async fn send_chat(&self, peer: EndpointId, body: Vec<u8>) -> Result<ChatMessage> {
        chat::send(&self.shared, peer, body).await
    }

    // --- files ---------------------------------------------------------

    pub async fn blob_add(&self, data: Vec<u8>) -> Result<BlobRef> {
        blobs::add_bytes(&self.shared, data).await
    }
    #[cfg(not(wasm_browser))]
    pub async fn blob_add_path(&self, path: &std::path::Path) -> Result<BlobRef> {
        blobs::add_path(&self.shared, path).await
    }
    /// A blob from the local store, if present.
    pub async fn blob_get(&self, hash: [u8; 32]) -> Result<Option<Vec<u8>>> {
        blobs::get(&self.shared, hash).await
    }
    /// Fetches (verified) a blob from `peer` into the local store.
    pub async fn blob_fetch(&self, peer: EndpointId, hash: [u8; 32]) -> Result<Vec<u8>> {
        blobs::fetch(&self.shared, peer, hash).await
    }
    pub async fn blob_fetch_ticket(&self, ticket: &str) -> Result<Vec<u8>> {
        blobs::fetch_ticket(&self.shared, ticket).await
    }

    // --- records -------------------------------------------------------

    /// Joins a space's gossip topic (bootstrapping from known peers) and
    /// pulls their state as neighbours come up.
    pub async fn join_space(&self, space: SpaceId, bootstrap: Vec<EndpointId>) -> Result<()> {
        records::join(&self.shared, space, bootstrap).await
    }
    pub fn leave_space(&self, space: &SpaceId) {
        self.shared.records.leave(space)
    }
    /// Signs, applies and broadcasts a record.
    pub async fn put_record(&self, space: SpaceId, key: impl Into<String>, value: Vec<u8>) -> Result<Record> {
        records::put(&self.shared, space, key.into(), value).await
    }
    pub fn get_record(&self, space: &SpaceId, key: &str) -> Option<Record> {
        self.shared.records.get(space, key)
    }
    /// Every current record of a space, by key.
    pub fn records(&self, space: &SpaceId) -> Vec<Record> {
        self.shared.records.snapshot(space)
    }
    /// Applies records from elsewhere (a persisted snapshot, another
    /// transport). Each is verified; returns how many won their key.
    /// Raises no events.
    pub fn apply_records(&self, records: Vec<Record>) -> Result<usize> {
        let mut n = 0;
        for r in records {
            if self.shared.records.apply(r)? {
                n += 1;
            }
        }
        Ok(n)
    }
    /// Pulls a space's state from one peer now; returns how many records won.
    pub async fn sync_space(&self, space: SpaceId, peer: EndpointId) -> Result<usize> {
        records::sync_from(&self.shared, space, peer).await
    }

    // --- calls ---------------------------------------------------------

    /// Rings `peer`; [`Event::CallAccepted`] follows when it answers.
    pub async fn start_call(&self, peer: EndpointId) -> Result<CallId> {
        call::start(&self.shared, peer).await
    }
    pub async fn accept_call(&self, call: CallId) -> Result<()> {
        call::accept(&self.shared, call).await
    }
    /// Sends one encoded audio frame; returns its sequence number.
    pub async fn send_frame(&self, call: CallId, frame: &[u8]) -> Result<u32> {
        call::send_frame(&self.shared, call, frame).await
    }
    pub fn hang_up(&self, call: CallId) -> Result<()> {
        call::hang_up(&self.shared, call)
    }
    pub fn call_peer(&self, call: CallId) -> Option<EndpointId> {
        self.shared.calls.peer(call)
    }

    // --- lifecycle -----------------------------------------------------

    /// Stops serving and closes the endpoint (peers see the connections close).
    pub async fn shutdown(&self) -> Result<()> {
        self.router.shutdown().await.map_err(|e| NetError::Transport(e.to_string()))
    }
}
