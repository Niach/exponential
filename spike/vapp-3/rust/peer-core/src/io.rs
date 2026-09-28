//! The sans-IO str0m loop on its own thread. Every datagram (host sockets + TURN relay conn) and
//! every API command arrives on ONE std mpsc channel; the loop feeds str0m, drains its outputs
//! (transmits routed by `source` address: host socket or relay conn), and publishes state into
//! [`Shared`] for the blocking API methods.
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::channel::ChannelId;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

use crate::gather::NetIn;
use crate::link::{LinkState, PeerError};

pub(crate) type Reply<T> = SyncSender<Result<T, PeerError>>;

pub(crate) enum Msg {
    Net(NetIn),
    Cmd(Cmd),
}

pub(crate) enum Cmd {
    CreateOffer(Reply<String>),
    AcceptOffer(String, Reply<String>),
    AcceptAnswer(String, Reply<()>),
    AddRemoteCandidate(Candidate),
    Send(Vec<u8>),
    RestartIce(Reply<String>),
    Close,
}

/// State the loop publishes for the API side.
#[derive(Default)]
pub(crate) struct St {
    pub state: Option<LinkState>,
    pub signals: VecDeque<String>,
    pub inbound: VecDeque<Vec<u8>>,
    /// Bytes queued in the loop but not yet accepted by SCTP.
    pub out_bytes: usize,
    pub channel_open: bool,
    pub t_start: Option<Instant>,
    pub t_open: Option<Instant>,
    pub t_connected: Option<Instant>,
    pub last_send: Option<(SocketAddr, SocketAddr)>,
    pub ice_state: String,
    pub errors: Vec<String>,
    pub fatal: Option<String>,
    pub local_fp: String,
    pub remote_fp: Option<String>,
    pub ice_restarts: u32,
}

pub(crate) struct Shared {
    pub st: Mutex<St>,
    pub cv: Condvar,
}

impl Shared {
    pub fn new() -> Self {
        Self { st: Mutex::new(St { state: Some(LinkState::New), ..Default::default() }), cv: Condvar::new() }
    }
    pub fn update<R>(&self, f: impl FnOnce(&mut St) -> R) -> R {
        let mut g = self.st.lock().unwrap();
        let r = f(&mut g);
        self.cv.notify_all();
        r
    }
    /// Wait until `pred` holds or `timeout` passes; returns whether it holds.
    pub fn wait_until(&self, timeout: Duration, mut pred: impl FnMut(&mut St) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        let mut g = self.st.lock().unwrap();
        loop {
            if pred(&mut g) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            g = self.cv.wait_timeout(g, deadline - now).unwrap().0;
        }
    }
}

pub(crate) struct Loop {
    pub rtc: Rtc,
    pub rx: Receiver<Msg>,
    pub shared: Arc<Shared>,
    pub host: HashMap<SocketAddr, Arc<UdpSocket>>,
    pub relay_addr: Option<SocketAddr>,
    #[cfg(feature = "turn")]
    pub relay_tx: Option<tokio::sync::mpsc::UnboundedSender<(Vec<u8>, SocketAddr)>>,
    pub stop: Arc<AtomicBool>,
    pub pending: Option<SdpPendingOffer>,
    pub channel: Option<ChannelId>,
    pub outq: VecDeque<Vec<u8>>,
    /// Fingerprint (hash, HEX) the remote SDP announced; checked against DTLS after connect.
    pub expected_remote_fp: Option<(String, String)>,
    pub connected: bool,
}

const MAX_WAIT: Duration = Duration::from_millis(50);

impl Loop {
    pub fn run(mut self) {
        loop {
            self.flush();
            let deadline = match self.drain() {
                Ok(t) => t,
                Err(e) => {
                    self.fatal(format!("rtc: {e}"));
                    break;
                }
            };
            if !self.rtc.is_alive() {
                break;
            }
            let wait = deadline.saturating_duration_since(Instant::now()).min(MAX_WAIT);
            let mut stop = false;
            match self.rx.recv_timeout(wait) {
                Ok(m) => {
                    stop |= !self.handle(m);
                    // Batch whatever else is queued, draining outputs after every input.
                    let mut n = 0;
                    while !stop && n < 256 {
                        let Ok(m) = self.rx.try_recv() else { break };
                        if let Err(e) = self.drain() {
                            self.fatal(format!("rtc: {e}"));
                            stop = true;
                            break;
                        }
                        stop |= !self.handle(m);
                        n += 1;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => stop = true,
            }
            if stop {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                if let Err(e) = self.rtc.handle_input(Input::Timeout(now)) {
                    tracing::debug!("timeout input: {e}");
                }
            }
        }
        self.stop.store(true, Ordering::Relaxed);
        #[cfg(feature = "turn")]
        {
            self.relay_tx = None;
        }
        self.shared.update(|s| {
            if !matches!(s.state, Some(LinkState::Failed)) {
                s.state = Some(LinkState::Closed);
            }
        });
    }

    fn fatal(&self, msg: String) {
        tracing::warn!("peer link failed: {msg}");
        self.shared.update(|s| {
            s.errors.push(msg.clone());
            s.fatal = Some(msg);
            s.state = Some(LinkState::Failed);
        });
    }

    /// Returns false to stop the loop.
    fn handle(&mut self, m: Msg) -> bool {
        match m {
            Msg::Net(n) => {
                let Ok(contents) = n.data.as_slice().try_into() else { return true };
                let input = Input::Receive(
                    Instant::now(),
                    Receive { proto: Protocol::Udp, source: n.source, destination: n.destination, contents },
                );
                if let Err(e) = self.rtc.handle_input(input) {
                    tracing::debug!("receive from {}: {e}", n.source);
                }
            }
            Msg::Cmd(c) => return self.command(c),
        }
        true
    }

    fn command(&mut self, c: Cmd) -> bool {
        match c {
            Cmd::CreateOffer(reply) => {
                let mut api = self.rtc.sdp_api();
                let cid = api.add_channel("exp".into());
                let r = match api.apply() {
                    Some((offer, pending)) => {
                        self.pending = Some(pending);
                        self.channel = Some(cid);
                        Ok(offer.to_sdp_string())
                    }
                    None => Err(PeerError::Other("sdp_api produced no offer".into())),
                };
                let _ = reply.send(r);
            }
            Cmd::RestartIce(reply) => {
                let mut api = self.rtc.sdp_api();
                api.ice_restart(true);
                let r = match api.apply() {
                    Some((offer, pending)) => {
                        self.pending = Some(pending);
                        self.shared.update(|s| s.ice_restarts += 1);
                        Ok(offer.to_sdp_string())
                    }
                    None => Err(PeerError::Other("ice_restart produced no offer".into())),
                };
                let _ = reply.send(r);
            }
            Cmd::AcceptOffer(sdp, reply) => {
                let r = SdpOffer::from_sdp_string(&sdp)
                    .map_err(|e| PeerError::InvalidArgument(format!("offer sdp: {e}")))
                    .and_then(|offer| {
                        self.rtc
                            .sdp_api()
                            .accept_offer(offer)
                            .map_err(|e| PeerError::Connection(format!("accept_offer: {e}")))
                    })
                    .map(|a| a.to_sdp_string());
                if r.is_ok() {
                    self.expected_remote_fp = crate::signal::sdp_fingerprint(&sdp);
                }
                let _ = reply.send(r);
            }
            Cmd::AcceptAnswer(sdp, reply) => {
                let r = match self.pending.take() {
                    None => Err(PeerError::InvalidArgument("no pending offer".into())),
                    Some(p) => SdpAnswer::from_sdp_string(&sdp)
                        .map_err(|e| PeerError::InvalidArgument(format!("answer sdp: {e}")))
                        .and_then(|a| {
                            self.rtc
                                .sdp_api()
                                .accept_answer(p, a)
                                .map_err(|e| PeerError::Connection(format!("accept_answer: {e}")))
                        }),
                };
                if r.is_ok() {
                    self.expected_remote_fp = crate::signal::sdp_fingerprint(&sdp);
                }
                let _ = reply.send(r);
            }
            Cmd::AddRemoteCandidate(c) => self.rtc.add_remote_candidate(c),
            Cmd::Send(data) => self.outq.push_back(data),
            Cmd::Close => {
                self.rtc.disconnect();
                return false;
            }
        }
        true
    }

    /// Move queued messages into SCTP while it accepts them (str0m caps the SCTP send buffer at
    /// 128 KiB across streams; `write` returns Ok(false) when full).
    fn flush(&mut self) {
        let Some(cid) = self.channel else { return };
        let mut freed = 0usize;
        while let Some(front) = self.outq.front() {
            let Some(mut ch) = self.rtc.channel(cid) else { break };
            match ch.write(true, front) {
                Ok(true) => {
                    freed += front.len();
                    self.outq.pop_front();
                }
                Ok(false) => break,
                Err(e) => {
                    tracing::debug!("channel write: {e}");
                    break;
                }
            }
        }
        if freed > 0 {
            self.shared.update(|s| s.out_bytes = s.out_bytes.saturating_sub(freed));
        }
    }

    fn drain(&mut self) -> Result<Instant, str0m::RtcError> {
        loop {
            match self.rtc.poll_output()? {
                Output::Timeout(t) => return Ok(t),
                Output::Transmit(t) => {
                    if self.connected {
                        let pair = (t.source, t.destination);
                        let mut g = self.shared.st.lock().unwrap();
                        if g.last_send != Some(pair) {
                            g.last_send = Some(pair);
                        }
                    }
                    if Some(t.source) == self.relay_addr {
                        #[cfg(feature = "turn")]
                        if let Some(tx) = &self.relay_tx {
                            let _ = tx.send((t.contents.to_vec(), t.destination));
                        }
                    } else if let Some(sock) = self.host.get(&t.source) {
                        if let Err(e) = sock.send_to(&t.contents, t.destination) {
                            tracing::trace!("send_to {}: {e}", t.destination);
                        }
                    } else {
                        tracing::debug!("transmit from unknown source {}", t.source);
                    }
                }
                Output::Event(e) => self.event(e),
            }
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::IceConnectionStateChange(s) => {
                tracing::debug!("ice state {s:?}");
                self.shared.update(|st| {
                    st.ice_state = format!("{s:?}");
                    match s {
                        IceConnectionState::Checking => {
                            if matches!(st.state, Some(LinkState::New)) {
                                st.state = Some(LinkState::Connecting);
                            }
                        }
                        IceConnectionState::Disconnected => {
                            if matches!(st.state, Some(LinkState::Connected)) {
                                st.state = Some(LinkState::Disconnected);
                            }
                        }
                        IceConnectionState::Connected | IceConnectionState::Completed => {
                            if matches!(st.state, Some(LinkState::Disconnected)) && st.channel_open {
                                st.state = Some(LinkState::Connected);
                            }
                        }
                        _ => {}
                    }
                });
            }
            Event::Connected => {
                self.connected = true;
                let remote = self.rtc.direct_api().remote_dtls_fingerprint().cloned();
                let remote_s = remote.as_ref().map(|f| f.to_string());
                let mismatch = match (&self.expected_remote_fp, &remote) {
                    (Some((hash, hex)), Some(fp)) => {
                        let got = fp.to_string();
                        let want = format!("{hash} {hex}");
                        (!got.eq_ignore_ascii_case(&want)).then(|| format!("sdp {want} != dtls {got}"))
                    }
                    (Some(_), None) => Some("no dtls fingerprint".into()),
                    _ => None,
                };
                self.shared.update(|s| {
                    s.remote_fp = remote_s;
                    s.t_connected = Some(Instant::now());
                });
                if let Some(m) = mismatch {
                    self.fatal(format!("fingerprint_mismatch: {m}"));
                    self.rtc.disconnect();
                }
            }
            Event::ChannelOpen(id, label) => {
                tracing::debug!("channel open {label}");
                self.channel = Some(id);
                if let Some(mut ch) = self.rtc.channel(id) {
                    ch.set_buffered_amount_low_threshold(64 * 1024);
                }
                self.shared.update(|s| {
                    s.channel_open = true;
                    s.t_open.get_or_insert(Instant::now());
                    if !matches!(s.state, Some(LinkState::Failed)) {
                        s.state = Some(LinkState::Connected);
                    }
                });
            }
            Event::ChannelData(d) => {
                self.shared.update(|s| s.inbound.push_back(d.data));
            }
            Event::ChannelClose(_) | Event::Closed => {
                self.shared.update(|s| {
                    s.channel_open = false;
                    if !matches!(s.state, Some(LinkState::Failed)) {
                        s.state = Some(LinkState::Closed);
                    }
                });
            }
            _ => {}
        }
    }
}
