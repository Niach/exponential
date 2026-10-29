//! Calls: opaque audio frames (the app's Opus packets) over QUIC datagrams.
//!
//! Wire (`exp/enet/call/1`, one connection per call):
//! - control: the caller opens a bidirectional stream and writes `0x01`
//!   (ring); the callee answers `0x02` (accept) or closes the connection
//!   (decline). Either side ends the call by closing the connection
//!   (code 0 = hang up).
//! - frames: one datagram per frame, `[u32 BE seq][payload]`. A frame that
//!   does not fit the path's datagram size (or a path without datagrams)
//!   goes on a unidirectional stream instead, `[u32 BE len][u32 BE seq][payload]`,
//!   so nothing is ever silently dropped by eNet; the network may still lose
//!   datagrams (that is the point: no head-of-line blocking).
//!
//! eNet numbers frames per call (`seq`, from 0) and does no timing: the
//! app's jitter buffer orders, conceals and plays them.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
};

use bytes::Bytes;
use iroh::{
    endpoint::{Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler},
    EndpointId,
};

use crate::{
    error::{transport, NetError, Result},
    event::{CallEndReason, Event},
    node::Shared,
    util::Reader,
};

pub const CALL_ALPN: &[u8] = b"exp/enet/call/1";
/// The largest frame `send_frame` takes (an Opus packet is far smaller).
pub const MAX_CALL_FRAME: usize = 64 * 1024;
const RING: u8 = 1;
const ACCEPT: u8 = 2;

/// A call handle, unique within one node.
pub type CallId = u64;

struct Call {
    conn: Connection,
    peer: EndpointId,
    accepted: AtomicBool,
    seq: AtomicU32,
    /// The callee's control send stream until it accepts.
    control: tokio::sync::Mutex<Option<SendStream>>,
    fallback: tokio::sync::Mutex<Option<SendStream>>,
    ended_locally: AtomicBool,
}

#[derive(Default)]
pub(crate) struct CallState {
    next: AtomicU64,
    calls: Mutex<HashMap<CallId, Arc<Call>>>,
}

impl CallState {
    fn get(&self, id: CallId) -> Result<Arc<Call>> {
        self.calls.lock().expect("calls lock").get(&id).cloned().ok_or_else(|| NetError::NotFound(format!("call {id}")))
    }
    fn insert(&self, call: Call) -> (CallId, Arc<Call>) {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let call = Arc::new(call);
        self.calls.lock().expect("calls lock").insert(id, call.clone());
        (id, call)
    }
    pub(crate) fn peer(&self, id: CallId) -> Option<EndpointId> {
        self.calls.lock().expect("calls lock").get(&id).map(|c| c.peer)
    }
}

fn new_call(conn: Connection, control: Option<SendStream>) -> Call {
    Call {
        peer: conn.remote_id(),
        conn,
        accepted: AtomicBool::new(false),
        seq: AtomicU32::new(0),
        control: tokio::sync::Mutex::new(control),
        fallback: tokio::sync::Mutex::new(None),
        ended_locally: AtomicBool::new(false),
    }
}

/// Spawns the receive side (datagrams, fallback streams, close) of a call.
fn spawn_receivers(shared: &Arc<Shared>, id: CallId, call: &Arc<Call>) {
    let weak = Arc::downgrade(shared);
    let conn = call.conn.clone();
    n0_future::task::spawn(async move {
        while let Ok(d) = conn.read_datagram().await {
            if d.len() < 4 {
                continue;
            }
            let seq = u32::from_be_bytes(d[..4].try_into().unwrap());
            let Some(s) = weak.upgrade() else { break };
            s.emit(Event::CallFrame { call: id, seq, data: d[4..].to_vec() });
        }
    });
    let weak = Arc::downgrade(shared);
    let conn = call.conn.clone();
    n0_future::task::spawn(async move {
        while let Ok(recv) = conn.accept_uni().await {
            n0_future::task::spawn(read_fallback(weak.clone(), id, recv));
        }
    });
    let weak = Arc::downgrade(shared);
    let call = call.clone();
    n0_future::task::spawn(async move {
        let err = call.conn.closed().await;
        let Some(s) = weak.upgrade() else { return };
        s.calls.calls.lock().expect("calls lock").remove(&id);
        let reason = if call.ended_locally.load(Ordering::SeqCst) {
            CallEndReason::Local
        } else {
            match err {
                iroh::endpoint::ConnectionError::ApplicationClosed(_) => CallEndReason::Remote,
                other => CallEndReason::Failed(other.to_string()),
            }
        };
        s.emit(Event::CallEnded { call: id, reason });
    });
}

async fn read_fallback(weak: Weak<Shared>, id: CallId, mut recv: RecvStream) {
    while let Ok(Some(frame)) = crate::util::read_frame(&mut recv, MAX_CALL_FRAME + 4).await {
        let mut r = Reader::new(&frame);
        let Ok(seq) = r.u32() else { break };
        let Some(s) = weak.upgrade() else { break };
        s.emit(Event::CallFrame { call: id, seq, data: r.rest().to_vec() });
    }
}

pub(crate) async fn start(shared: &Arc<Shared>, peer: EndpointId) -> Result<CallId> {
    let conn = shared.connect(peer, CALL_ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await.map_err(crate::node::on_conn(&conn))?;
    send.write_all(&[RING]).await.map_err(crate::node::on_conn(&conn))?;
    let (id, call) = shared.calls.insert(new_call(conn, None));
    spawn_receivers(shared, id, &call);
    let weak = Arc::downgrade(shared);
    n0_future::task::spawn(async move {
        let mut b = [0u8; 1];
        if recv.read_exact(&mut b).await.is_ok() && b[0] == ACCEPT {
            call.accepted.store(true, Ordering::SeqCst);
            if let Some(s) = weak.upgrade() {
                s.emit(Event::CallAccepted { call: id });
            }
        }
        // keep the control stream open for the call's lifetime
        let _keep = send;
        let _ = call.conn.closed().await;
    });
    Ok(id)
}

pub(crate) async fn accept(shared: &Shared, id: CallId) -> Result<()> {
    let call = shared.calls.get(id)?;
    let mut control = call.control.lock().await;
    let Some(send) = control.as_mut() else {
        return Err(NetError::Invalid("not an incoming ringing call".into()));
    };
    send.write_all(&[ACCEPT]).await.map_err(transport)?;
    call.accepted.store(true, Ordering::SeqCst);
    Ok(())
}

/// Sends one frame; returns its sequence number.
pub(crate) async fn send_frame(shared: &Shared, id: CallId, data: &[u8]) -> Result<u32> {
    if data.len() > MAX_CALL_FRAME {
        return Err(NetError::Invalid(format!("call frame over {MAX_CALL_FRAME} bytes")));
    }
    let call = shared.calls.get(id)?;
    if !call.accepted.load(Ordering::SeqCst) {
        return Err(NetError::Invalid("call not accepted yet".into()));
    }
    let seq = call.seq.fetch_add(1, Ordering::SeqCst);
    let fits = call.conn.max_datagram_size().is_some_and(|max| data.len() + 4 <= max);
    if fits {
        let mut d = Vec::with_capacity(4 + data.len());
        d.extend_from_slice(&seq.to_be_bytes());
        d.extend_from_slice(data);
        match call.conn.send_datagram(Bytes::from(d)) {
            Ok(()) => return Ok(seq),
            Err(iroh::endpoint::SendDatagramError::TooLarge) | Err(iroh::endpoint::SendDatagramError::Disabled) => {}
            Err(iroh::endpoint::SendDatagramError::UnsupportedByPeer) => {}
            Err(e) => return Err(transport(e)),
        }
    }
    let mut fb = call.fallback.lock().await;
    if fb.is_none() {
        *fb = Some(call.conn.open_uni().await.map_err(transport)?);
    }
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(&seq.to_be_bytes());
    body.extend_from_slice(data);
    crate::util::write_frame(fb.as_mut().expect("set above"), &body).await?;
    Ok(seq)
}

pub(crate) fn hang_up(shared: &Shared, id: CallId) -> Result<()> {
    let call = shared.calls.get(id)?;
    call.ended_locally.store(true, Ordering::SeqCst);
    call.conn.close(0u32.into(), b"hang up");
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct CallProtocol {
    pub(crate) shared: Weak<Shared>,
}

impl ProtocolHandler for CallProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let (send, mut recv) = conn.accept_bi().await?;
        let mut b = [0u8; 1];
        recv.read_exact(&mut b).await.map_err(AcceptError::from_err)?;
        if b[0] != RING {
            conn.close(1u32.into(), b"bad call control");
            return Ok(());
        }
        let Some(shared) = self.shared.upgrade() else { return Ok(()) };
        let from = conn.remote_id();
        let (id, call) = shared.calls.insert(new_call(conn.clone(), Some(send)));
        spawn_receivers(&shared, id, &call);
        shared.emit(Event::CallIncoming { call: id, from });
        drop(shared);
        let _ = conn.closed().await;
        drop(recv);
        Ok(())
    }
}
