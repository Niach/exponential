//! Records: our own small replicated key/value store per "space" (32 bytes,
//! e.g. a block instance id) over iroh-gossip. NOT iroh-docs.
//!
//! - A record = `{space, key, value, author, clock, signature}`; the author
//!   signs it with its node key, every receiver verifies before applying.
//! - Per key, the last writer wins: the larger `(clock, author, value)` tuple
//!   (lexicographic; `clock` = hybrid logical clock `(millis, counter)`), so
//!   every replica picks the same winner regardless of arrival order.
//! - Puts are broadcast on the space's gossip topic (topic id = space id,
//!   ALPN [`GOSSIP_ALPN`]). When a gossip neighbour comes up, each side pulls
//!   the other's full state over [`RECORDS_SYNC_ALPN`], so a newcomer
//!   converges without waiting for new writes.
//!
//! Record wire format (gossip message = `0x01 || record`, sync frame = record):
//! ```text
//! u8 version (1) | space[32] | u16 BE key len | key (UTF-8)
//! | u64 BE millis | u32 BE counter | author[32] | u32 BE value len | value
//! | signature[64]   (ed25519 by author over "exp/enet/record/1\0" || all bytes before it)
//! ```

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, Weak},
};

use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    EndpointId,
};
use iroh_gossip::{
    api::{Event as GossipEvent, GossipSender},
    TopicId,
};
use n0_future::{task::AbortOnDropHandle, StreamExt};

use crate::{
    error::{transport, NetError, Result},
    event::Event,
    identity::{verify, Identity},
    node::Shared,
    util::{now_ms, read_frame, write_frame, Reader},
};

/// The gossip ALPN (iroh-gossip's protocol under our versioned name).
pub const GOSSIP_ALPN: &[u8] = b"exp/enet/gossip/1";
/// Pull a space's full state from a peer.
pub const RECORDS_SYNC_ALPN: &[u8] = b"exp/enet/records-sync/1";
pub const MAX_RECORD_KEY: usize = 1024;
pub const MAX_RECORD_VALUE: usize = 48 * 1024;
/// The gossip message cap (one record + framing).
pub(crate) const GOSSIP_MAX_MESSAGE: usize = 64 * 1024;
const RECORD_VERSION: u8 = 1;
const SIGN_CONTEXT: &[u8] = b"exp/enet/record/1\0";
const MSG_RECORD: u8 = 1;

pub type SpaceId = [u8; 32];

/// A hybrid logical clock stamp: wall-clock millis, then a counter for
/// writes within the same (or an older observed) millisecond.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Clock {
    pub millis: u64,
    pub counter: u32,
}

/// One signed record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub space: SpaceId,
    pub key: String,
    pub value: Vec<u8>,
    pub author: EndpointId,
    pub clock: Clock,
    pub signature: [u8; 64],
}

impl Record {
    /// Signs a new record (no clock bookkeeping: [`crate::Node::put_record`]
    /// stamps the clock).
    pub fn sign(identity: &Identity, space: SpaceId, key: String, value: Vec<u8>, clock: Clock) -> Result<Self> {
        check_sizes(&key, &value)?;
        let mut r = Record { space, key, value, author: identity.endpoint_id(), clock, signature: [0; 64] };
        r.signature = identity.sign(&r.signed_bytes());
        Ok(r)
    }

    fn body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(96 + self.key.len() + self.value.len());
        out.push(RECORD_VERSION);
        out.extend_from_slice(&self.space);
        out.extend_from_slice(&(self.key.len() as u16).to_be_bytes());
        out.extend_from_slice(self.key.as_bytes());
        out.extend_from_slice(&self.clock.millis.to_be_bytes());
        out.extend_from_slice(&self.clock.counter.to_be_bytes());
        out.extend_from_slice(self.author.as_bytes());
        out.extend_from_slice(&(self.value.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.value);
        out
    }

    fn signed_bytes(&self) -> Vec<u8> {
        let mut m = SIGN_CONTEXT.to_vec();
        m.extend_from_slice(&self.body());
        m
    }

    pub fn verify(&self) -> bool {
        check_sizes(&self.key, &self.value).is_ok() && verify(&self.author, &self.signed_bytes(), &self.signature)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.body();
        out.extend_from_slice(&self.signature);
        out
    }

    /// Parses a record (does NOT verify the signature: see [`Record::verify`]).
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = Reader::new(buf);
        if r.u8()? != RECORD_VERSION {
            return Err(NetError::Invalid("unknown record version".into()));
        }
        let space = r.array::<32>()?;
        let klen = r.u16()? as usize;
        let key = String::from_utf8(r.take(klen)?.to_vec()).map_err(|_| NetError::Invalid("record key not UTF-8".into()))?;
        let clock = Clock { millis: r.u64()?, counter: r.u32()? };
        let author = EndpointId::from_bytes(&r.array::<32>()?).map_err(|_| NetError::Invalid("bad author".into()))?;
        let vlen = r.u32()? as usize;
        let value = r.take(vlen)?.to_vec();
        let signature = r.array::<64>()?;
        if !r.is_empty() {
            return Err(NetError::Invalid("trailing bytes after record".into()));
        }
        Ok(Record { space, key, value, author, clock, signature })
    }

    /// The last-writer-wins order: does `self` replace `other` for its key?
    pub fn supersedes(&self, other: &Record) -> bool {
        (self.clock, self.author.as_bytes(), &self.value) > (other.clock, other.author.as_bytes(), &other.value)
    }
}

fn check_sizes(key: &str, value: &[u8]) -> Result<()> {
    if key.len() > MAX_RECORD_KEY {
        return Err(NetError::Invalid(format!("record key over {MAX_RECORD_KEY} bytes")));
    }
    if value.len() > MAX_RECORD_VALUE {
        return Err(NetError::Invalid(format!("record value over {MAX_RECORD_VALUE} bytes; store a blob hash instead")));
    }
    Ok(())
}

#[derive(Default)]
struct Space {
    records: BTreeMap<String, Record>,
    sender: Option<GossipSender>,
    _task: Option<AbortOnDropHandle<()>>,
}

#[derive(Default)]
pub(crate) struct RecordsState {
    spaces: Mutex<HashMap<SpaceId, Space>>,
    clock: Mutex<Clock>,
}

impl RecordsState {
    fn tick(&self) -> Clock {
        let now = now_ms();
        let mut c = self.clock.lock().expect("clock lock");
        if now > c.millis {
            *c = Clock { millis: now, counter: 0 };
        } else {
            c.counter = c.counter.saturating_add(1);
        }
        *c
    }

    fn observe(&self, seen: Clock) {
        let mut c = self.clock.lock().expect("clock lock");
        if seen > *c {
            *c = seen;
        }
    }

    /// Verifies and applies; `Ok(true)` = it won its key.
    pub(crate) fn apply(&self, record: Record) -> Result<bool> {
        if !record.verify() {
            return Err(NetError::BadSignature);
        }
        self.observe(record.clock);
        let mut spaces = self.spaces.lock().expect("records lock");
        let space = spaces.entry(record.space).or_default();
        match space.records.get(&record.key) {
            Some(cur) if !record.supersedes(cur) => Ok(false),
            _ => {
                space.records.insert(record.key.clone(), record);
                Ok(true)
            }
        }
    }

    pub(crate) fn get(&self, space: &SpaceId, key: &str) -> Option<Record> {
        self.spaces.lock().expect("records lock").get(space).and_then(|s| s.records.get(key).cloned())
    }

    pub(crate) fn snapshot(&self, space: &SpaceId) -> Vec<Record> {
        self.spaces
            .lock()
            .expect("records lock")
            .get(space)
            .map(|s| s.records.values().cloned().collect())
            .unwrap_or_default()
    }

    fn is_joined(&self, space: &SpaceId) -> bool {
        self.spaces.lock().expect("records lock").get(space).is_some_and(|s| s.sender.is_some())
    }

    pub(crate) fn leave(&self, space: &SpaceId) {
        if let Some(s) = self.spaces.lock().expect("records lock").get_mut(space) {
            s.sender = None;
            s._task = None;
        }
    }
}

pub(crate) async fn join(shared: &Arc<Shared>, space: SpaceId, bootstrap: Vec<EndpointId>) -> Result<()> {
    let topic = shared
        .gossip
        .subscribe(TopicId::from(space), bootstrap)
        .await
        .map_err(transport)?;
    let (sender, mut receiver) = topic.split();
    let weak: Weak<Shared> = Arc::downgrade(shared);
    let task = n0_future::task::spawn(async move {
        while let Some(ev) = receiver.next().await {
            let Some(shared) = weak.upgrade() else { break };
            match ev {
                Ok(GossipEvent::Received(msg)) => {
                    if msg.content.first() == Some(&MSG_RECORD) {
                        if let Ok(record) = Record::decode(&msg.content[1..]) {
                            if record.space == space {
                                apply_remote(&shared, record);
                            }
                        }
                    }
                }
                Ok(GossipEvent::NeighborUp(peer)) => {
                    let weak = Arc::downgrade(&shared);
                    n0_future::task::spawn(async move {
                        if let Some(shared) = weak.upgrade() {
                            if let Err(e) = sync_from(&shared, space, peer).await {
                                tracing::debug!(%peer, "records sync failed: {e}");
                            }
                        }
                    });
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let mut spaces = shared.records.spaces.lock().expect("records lock");
    let s = spaces.entry(space).or_default();
    s.sender = Some(sender);
    s._task = Some(AbortOnDropHandle::new(task));
    Ok(())
}

pub(crate) fn apply_remote(shared: &Shared, record: Record) -> bool {
    match shared.records.apply(record.clone()) {
        Ok(true) => {
            shared.emit(Event::Record { record });
            true
        }
        _ => false,
    }
}

pub(crate) async fn put(shared: &Shared, space: SpaceId, key: String, value: Vec<u8>) -> Result<Record> {
    let record = Record::sign(&shared.identity, space, key, value, shared.records.tick())?;
    shared.records.apply(record.clone())?;
    let sender = shared.records.spaces.lock().expect("records lock").get(&space).and_then(|s| s.sender.clone());
    if let Some(sender) = sender {
        let mut msg = vec![MSG_RECORD];
        msg.extend_from_slice(&record.encode());
        sender.broadcast(msg.into()).await.map_err(transport)?;
    }
    Ok(record)
}

/// Pulls `space` from `peer`; returns how many records won their key.
pub(crate) async fn sync_from(shared: &Shared, space: SpaceId, peer: EndpointId) -> Result<usize> {
    let conn = shared.connect(peer, RECORDS_SYNC_ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await.map_err(crate::node::on_conn(&conn))?;
    send.write_all(&space).await.map_err(crate::node::on_conn(&conn))?;
    send.finish().map_err(crate::node::on_conn(&conn))?;
    let mut applied = 0;
    while let Some(frame) = read_frame(&mut recv, GOSSIP_MAX_MESSAGE).await.map_err(|e| crate::node::classify(&conn, e))? {
        let record = Record::decode(&frame)?;
        if record.space == space && apply_remote(shared, record) {
            applied += 1;
        }
    }
    conn.close(0u32.into(), b"done");
    Ok(applied)
}

#[derive(Debug, Clone)]
pub(crate) struct SyncProtocol {
    pub(crate) shared: Weak<Shared>,
}

impl ProtocolHandler for SyncProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let mut space = [0u8; 32];
        recv.read_exact(&mut space).await.map_err(AcceptError::from_err)?;
        let Some(shared) = self.shared.upgrade() else { return Ok(()) };
        // Only spaces we are a member of are served.
        let records = if shared.records.is_joined(&space) { shared.records.snapshot(&space) } else { Vec::new() };
        drop(shared);
        for r in records {
            write_frame(&mut send, &r.encode()).await.map_err(AcceptError::from_err)?;
        }
        send.finish()?;
        // Let the requester read to the end; it closes the connection.
        let _ = conn.closed().await;
        Ok(())
    }
}
