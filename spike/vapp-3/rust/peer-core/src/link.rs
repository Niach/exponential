//! The peer link: FROZEN API for the spike (peer-ffi mirrors it 1:1 for Swift/Kotlin; peer-cli
//! drives it). Implementation: str0m `Rtc` on an I/O thread ([`crate::io`]), candidates from
//! [`crate::gather`] (host sockets, hand-rolled STUN srflx, `turn` crate relay), signed DTLS
//! fingerprints ([`crate::signing`]) and the bench protocol ([`crate::bench`]).
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use str0m::{Candidate, CandidateKind, Rtc};

use crate::io::{Cmd, Loop, Msg, Shared};
use crate::signal::{sdp_fingerprint, sdp_ufrag, Body, Envelope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcePolicy {
    /// host + srflx + relay (browser default).
    All,
    /// relay only (browser `iceTransportPolicy: 'relay'`).
    Relay,
    /// host only (same LAN / same machine).
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(Debug, Clone)]
pub struct PeerConfig {
    /// Random id of this peer inside the room (envelope `from`).
    pub peer_id: String,
    /// The relay room (steer ticket `sessionId`); part of the signed bytes.
    pub session_id: String,
    /// `host:port` of the STUN server (coturn), `None` = no srflx.
    pub stun: Option<String>,
    /// `host:port` of the TURN server (coturn), `None` = no relay candidate.
    pub turn: Option<String>,
    pub turn_username: Option<String>,
    pub turn_password: Option<String>,
    pub turn_realm: Option<String>,
    pub policy: IcePolicy,
    /// 32-byte Ed25519 seed; `None` = generate a fresh identity.
    pub identity_seed: Option<Vec<u8>>,
    /// Expected remote public key (base64url); `None` = trust on first use (the spike logs it).
    pub expected_remote_pubkey: Option<String>,
    /// Spike knob: flip one fingerprint byte in OUR outgoing SDP after signing (tamper test).
    pub tamper_sdp: bool,
    /// Spike knob: corrupt OUR signature (valid SDP, bad sig).
    pub tamper_sig: bool,
}

impl PeerConfig {
    pub fn new(peer_id: &str, session_id: &str) -> Self {
        Self {
            peer_id: peer_id.into(),
            session_id: session_id.into(),
            stun: None,
            turn: None,
            turn_username: None,
            turn_password: None,
            turn_realm: None,
            policy: IcePolicy::All,
            identity_seed: None,
            expected_remote_pubkey: None,
            tamper_sdp: false,
            tamper_sig: false,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BenchResult {
    /// Offer created (or received) → data channel open.
    pub connect_ms: f64,
    pub rtt_p50_ms: f64,
    pub rtt_p95_ms: f64,
    /// This peer → remote, megabits per second for `body_bytes`.
    pub up_mbps: f64,
    /// Remote → this peer.
    pub down_mbps: f64,
    pub body_bytes: u64,
    /// host | srflx | prflx | relay (the LOCAL side of the nominated pair).
    pub local_path: String,
    /// host | srflx | prflx | relay (the REMOTE side of the nominated pair).
    pub remote_path: String,
    pub local_candidate: String,
    pub remote_candidate: String,
    pub errors: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("not implemented yet (Lane A)")]
    NotImplemented,
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("signature rejected: {0}")]
    SignatureRejected(String),
    #[error("fingerprint mismatch: {0}")]
    FingerprintMismatch(String),
    #[error("ice/dtls failure: {0}")]
    Connection(String),
    #[error("turn/stun failure: {0}")]
    Gathering(String),
    #[error("closed")]
    Closed,
    #[error("timeout")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

/// One peer connection carrying ONE ordered reliable data channel labelled `exp`.
///
/// Threading: every method is callable from any thread; the I/O loop runs on an internal thread.
/// Outbound signaling (trickled candidates, ICE-restart offers, state changes) is collected via
/// [`PeerLink::poll_signals`] as JSON [`crate::signal::Envelope`]s the host forwards over the relay.
///
/// Addressing: `create_offer` leaves `to` unset (broadcast; only the daemon answers offers), every
/// later envelope is addressed to the peer whose offer/answer we accepted.
pub struct PeerLink {
    config: PeerConfig,
    #[cfg(feature = "signing")]
    identity: crate::signing::Identity,
    shared: Arc<Shared>,
    tx: Mutex<Sender<Msg>>,
    locals: Vec<(SocketAddr, &'static str)>,
    remotes: Mutex<Vec<(SocketAddr, String)>>,
    remote_peer: Mutex<Option<String>>,
    remote_pubkey: Mutex<Option<String>>,
    rejects: Mutex<Vec<String>>,
    gather_ms: f64,
    gather_notes: Vec<String>,
    turn_mapped: Option<SocketAddr>,
    turn_relayed: Option<SocketAddr>,
}

/// Queued-but-unsent bytes above which `send` blocks (backpressure).
const SEND_HIGH_WATER: usize = 1024 * 1024;

fn kind_str(k: CandidateKind) -> &'static str {
    match k {
        CandidateKind::Host => "host",
        CandidateKind::ServerReflexive => "srflx",
        CandidateKind::Relayed => "relay",
        CandidateKind::PeerReflexive => "prflx",
    }
}

impl PeerLink {
    pub fn new(config: PeerConfig) -> Result<Arc<Self>, PeerError> {
        crate::install_crypto();
        #[cfg(feature = "signing")]
        let identity = match &config.identity_seed {
            Some(seed) => crate::signing::Identity::from_seed(seed)
                .ok_or_else(|| PeerError::InvalidArgument("identity_seed must be 32 bytes".into()))?,
            None => crate::signing::Identity::generate(),
        };
        let t0 = Instant::now();
        let (tx, rx) = mpsc::channel::<Msg>();
        let shared = Arc::new(Shared::new());
        let stop = Arc::new(AtomicBool::new(false));
        let mut rtc = Rtc::builder().set_fingerprint_verification(true).build(Instant::now());
        let mut locals: Vec<(SocketAddr, &'static str)> = Vec::new();
        let mut notes = Vec::new();
        let mut host: std::collections::HashMap<SocketAddr, Arc<UdpSocket>> = Default::default();
        let ips = crate::gather::host_ips();

        if config.policy != IcePolicy::Relay {
            for ip in &ips {
                let Ok(sock) = UdpSocket::bind(SocketAddr::new(*ip, 0)) else {
                    notes.push(format!("bind {ip} failed"));
                    continue;
                };
                let addr = sock.local_addr().map_err(|e| PeerError::Gathering(e.to_string()))?;
                let Ok(c) = Candidate::host(addr, "udp") else { continue };
                if rtc.add_local_candidate(c).is_some() {
                    locals.push((addr, "host"));
                    host.insert(addr, Arc::new(sock));
                }
            }
            // srflx: one STUN binding per host socket, in parallel, before the readers start.
            let stun = config.stun.clone().or_else(|| config.turn.clone());
            if config.policy == IcePolicy::All {
                if let Some(stun) = stun {
                    match crate::gather::resolve(&stun) {
                        Ok(server) => {
                            // Only the interface that routes to the STUN server can get an answer;
                            // probing the others just burns the timeout (VPN/bridge interfaces).
                            let route_ip = UdpSocket::bind("0.0.0.0:0")
                                .and_then(|u| u.connect(server).and_then(|_| u.local_addr()))
                                .map(|a| a.ip())
                                .ok();
                            let socks: Vec<_> = host
                                .iter()
                                .filter(|(a, _)| route_ip.is_none_or(|ip| a.ip() == ip))
                                .map(|(a, s)| (*a, s.clone()))
                                .collect();
                            let mapped: Vec<(SocketAddr, Option<SocketAddr>)> = std::thread::scope(|sc| {
                                let hs: Vec<_> = socks
                                    .iter()
                                    .map(|(a, s)| {
                                        sc.spawn(move || (*a, crate::gather::stun_binding(s, server, Duration::from_millis(800))))
                                    })
                                    .collect();
                                hs.into_iter().filter_map(|h| h.join().ok()).collect()
                            });
                            for (base, m) in mapped {
                                let Some(m) = m else { continue };
                                if m == base || locals.iter().any(|(a, _)| *a == m) {
                                    continue;
                                }
                                if let Ok(c) = Candidate::server_reflexive(m, base, "udp") {
                                    if rtc.add_local_candidate(c).is_some() {
                                        locals.push((m, "srflx"));
                                    }
                                }
                            }
                        }
                        Err(e) => notes.push(e.to_string()),
                    }
                }
            }
        }

        #[allow(unused_mut)]
        let mut relay_addr = None;
        #[allow(unused_mut)]
        let mut turn_mapped = None;
        #[cfg(feature = "turn")]
        let mut relay_tx = None;
        if config.policy != IcePolicy::Host {
            match &config.turn {
                #[cfg(feature = "turn")]
                Some(turn) => {
                    let server = crate::gather::resolve(turn)?;
                    let ip = ips.iter().copied().find(|i| !i.is_loopback()).unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
                    let r = crate::gather::start_turn(
                        server,
                        ip,
                        config.turn_username.clone().unwrap_or_default(),
                        config.turn_password.clone().unwrap_or_default(),
                        config.turn_realm.clone().unwrap_or_default(),
                        tx.clone(),
                        Duration::from_secs(5),
                    );
                    match r {
                        Ok(a) => {
                            turn_mapped = a.mapped;
                            match Candidate::relayed(a.relayed, a.local, "udp") {
                                Ok(c) => {
                                    if rtc.add_local_candidate(c).is_some() {
                                        locals.push((a.relayed, "relay"));
                                        relay_addr = Some(a.relayed);
                                        relay_tx = Some(a.tx);
                                    }
                                }
                                Err(e) => notes.push(format!("relayed candidate: {e}")),
                            }
                        }
                        Err(e) if config.policy == IcePolicy::Relay => return Err(e),
                        Err(e) => notes.push(e.to_string()),
                    }
                }
                #[cfg(not(feature = "turn"))]
                Some(_) => {
                    if config.policy == IcePolicy::Relay {
                        return Err(PeerError::Gathering("built without the `turn` feature".into()));
                    }
                    notes.push("turn ignored: built without the `turn` feature".into());
                }
                None => {
                    if config.policy == IcePolicy::Relay {
                        return Err(PeerError::Gathering("policy relay needs a TURN server".into()));
                    }
                }
            }
        }
        if locals.is_empty() {
            return Err(PeerError::Gathering("no local candidates".into()));
        }
        let gather_ms = t0.elapsed().as_secs_f64() * 1000.0;
        tracing::debug!("gathered {:?} in {gather_ms:.1} ms", locals);

        let local_fp = rtc.direct_api().local_dtls_fingerprint().to_string();
        shared.update(|s| s.local_fp = local_fp);
        for sock in host.values() {
            crate::gather::spawn_reader(sock.clone(), tx.clone(), stop.clone());
        }
        let lp = Loop {
            rtc,
            rx,
            shared: shared.clone(),
            host,
            relay_addr,
            #[cfg(feature = "turn")]
            relay_tx,
            stop,
            pending: None,
            channel: None,
            outq: Default::default(),
            expected_remote_fp: None,
            connected: false,
        };
        std::thread::Builder::new()
            .name("peer-rtc".into())
            .spawn(move || lp.run())
            .map_err(|e| PeerError::Other(format!("spawn: {e}")))?;

        Ok(Arc::new(Self {
            config,
            #[cfg(feature = "signing")]
            identity,
            shared,
            tx: Mutex::new(tx),
            locals,
            remotes: Mutex::new(Vec::new()),
            remote_peer: Mutex::new(None),
            remote_pubkey: Mutex::new(None),
            rejects: Mutex::new(Vec::new()),
            gather_ms,
            gather_notes: notes,
            turn_mapped,
            turn_relayed: relay_addr,
        }))
    }

    /// base64url Ed25519 public key of this peer.
    pub fn public_key(&self) -> String {
        #[cfg(feature = "signing")]
        return self.identity.public_key();
        #[cfg(not(feature = "signing"))]
        String::new()
    }

    fn cmd<T>(&self, make: impl FnOnce(crate::io::Reply<T>) -> Cmd) -> Result<T, PeerError> {
        let (rtx, rrx) = mpsc::sync_channel(1);
        self.tx.lock().unwrap().send(Msg::Cmd(make(rtx))).map_err(|_| PeerError::Closed)?;
        rrx.recv_timeout(Duration::from_secs(10)).map_err(|_| PeerError::Closed)?
    }

    /// Sign our SDP (then apply the tamper knobs). Returns (sdp, pub, sig).
    fn sign_sdp(&self, sdp: String) -> Result<(String, String, String), PeerError> {
        let (hash, fp) = sdp_fingerprint(&sdp).ok_or_else(|| PeerError::Other("our sdp has no fingerprint".into()))?;
        let ufrag = sdp_ufrag(&sdp).ok_or_else(|| PeerError::Other("our sdp has no ufrag".into()))?;
        #[cfg(feature = "signing")]
        let mut sig = self.identity.sign(&crate::signing::signed_bytes(&hash, &fp, &ufrag, &self.config.session_id, &self.config.peer_id));
        #[cfg(not(feature = "signing"))]
        let mut sig = {
            let _ = (&hash, &fp, &ufrag);
            String::new()
        };
        if self.config.tamper_sig && !sig.is_empty() {
            let first = if sig.starts_with('A') { "B" } else { "A" };
            sig.replace_range(0..1, first);
        }
        let mut sdp = sdp;
        if self.config.tamper_sdp {
            sdp = tamper_fingerprint(&sdp);
        }
        Ok((sdp, self.public_key(), sig))
    }

    /// Verify a remote offer/answer; `Err(reason)` = reject.
    fn verify(&self, env: &Envelope, sdp: &str, pubkey: &str, sig: &str) -> Result<(), String> {
        if let Some(want) = &self.config.expected_remote_pubkey {
            if want != pubkey {
                return Err("unexpected_peer_key".into());
            }
        }
        if let Some(known) = self.remote_pubkey.lock().unwrap().as_ref() {
            if known != pubkey {
                return Err("unexpected_peer_key".into());
            }
        }
        let (hash, fp) = sdp_fingerprint(sdp).ok_or("fingerprint_missing")?;
        let ufrag = sdp_ufrag(sdp).ok_or("ufrag_missing")?;
        #[cfg(feature = "signing")]
        {
            let msg = crate::signing::signed_bytes(&hash, &fp, &ufrag, &self.config.session_id, &env.from);
            crate::signing::verify(pubkey, sig, &msg).map_err(|_| "fingerprint_signature_invalid".to_string())?;
        }
        #[cfg(not(feature = "signing"))]
        let _ = (env, hash, fp, ufrag, sig);
        Ok(())
    }

    fn reject(&self, env: &Envelope, reason: String) -> PeerError {
        tracing::warn!("rejecting {} from {}: {reason}", kind_of(&env.body), env.from);
        self.rejects.lock().unwrap().push(reason.clone());
        let r = Envelope::new(&self.config.peer_id, Some(&env.from), &self.config.session_id, Body::Reject { reason: reason.clone() });
        self.shared.update(|s| s.signals.push_back(r.to_json()));
        PeerError::SignatureRejected(reason)
    }

    fn accepted_remote(&self, env: &Envelope, pubkey: &str, sdp: &str) {
        let mut pk = self.remote_pubkey.lock().unwrap();
        if pk.is_none() {
            tracing::info!("trust on first use: peer {} key {pubkey}", env.from);
            *pk = Some(pubkey.to_string());
        }
        *self.remote_peer.lock().unwrap() = Some(env.from.clone());
        let mut rs = self.remotes.lock().unwrap();
        for l in sdp.lines() {
            let l = l.trim_end_matches('\r');
            if let Some(c) = l.strip_prefix("a=") {
                if c.starts_with("candidate:") {
                    if let Ok(c) = Candidate::from_sdp_string(c) {
                        rs.push((c.addr(), kind_str(c.kind()).to_string()));
                    }
                }
            }
        }
    }

    fn envelope(&self, body: Body) -> String {
        let to = self.remote_peer.lock().unwrap().clone();
        Envelope::new(&self.config.peer_id, to.as_deref(), &self.config.session_id, body).to_json()
    }

    /// Offerer: create the data channel + a signed offer envelope (JSON) to send to the remote.
    pub fn create_offer(&self) -> Result<String, PeerError> {
        self.shared.update(|s| s.t_start = Some(Instant::now()));
        let sdp = self.cmd(Cmd::CreateOffer)?;
        let (sdp, pubkey, sig) = self.sign_sdp(sdp)?;
        Ok(self.envelope(Body::Offer { sdp, pubkey, sig }))
    }

    /// Answerer: verify + apply a signed offer envelope, return the signed answer envelope.
    pub fn accept_offer(&self, envelope_json: &str) -> Result<String, PeerError> {
        let env = Envelope::parse(envelope_json).ok_or_else(|| PeerError::InvalidArgument("not an envelope".into()))?;
        let Body::Offer { sdp, pubkey, sig } = &env.body else {
            return Err(PeerError::InvalidArgument("not an offer".into()));
        };
        if let Err(reason) = self.verify(&env, sdp, pubkey, sig) {
            return Err(self.reject(&env, reason));
        }
        self.shared.update(|s| {
            s.t_start.get_or_insert(Instant::now());
        });
        self.accepted_remote(&env, pubkey, sdp);
        let answer = self.cmd(|r| Cmd::AcceptOffer(sdp.clone(), r))?;
        let (sdp, pubkey, sig) = self.sign_sdp(answer)?;
        Ok(self.envelope(Body::Answer { sdp, pubkey, sig }))
    }

    /// Offerer: verify + apply the signed answer envelope.
    pub fn accept_answer(&self, envelope_json: &str) -> Result<(), PeerError> {
        let env = Envelope::parse(envelope_json).ok_or_else(|| PeerError::InvalidArgument("not an envelope".into()))?;
        let Body::Answer { sdp, pubkey, sig } = &env.body else {
            return Err(PeerError::InvalidArgument("not an answer".into()));
        };
        if let Err(reason) = self.verify(&env, sdp, pubkey, sig) {
            return Err(self.reject(&env, reason));
        }
        self.accepted_remote(&env, pubkey, sdp);
        self.cmd(|r| Cmd::AcceptAnswer(sdp.clone(), r))
    }

    /// Add a remote ICE candidate (`candidate` envelope JSON or a bare `candidate:` line).
    pub fn add_remote_candidate(&self, candidate: &str) -> Result<(), PeerError> {
        let line = if candidate.trim_start().starts_with('{') {
            let env = Envelope::parse(candidate).ok_or_else(|| PeerError::InvalidArgument("not an envelope".into()))?;
            match env.body {
                Body::Candidate { candidate, .. } => candidate,
                Body::EndOfCandidates => return Ok(()),
                _ => return Err(PeerError::InvalidArgument("not a candidate".into())),
            }
        } else {
            candidate.to_string()
        };
        let line = line.trim().trim_start_matches("a=");
        if line.is_empty() {
            return Ok(()); // browser end-of-candidates ("")
        }
        let c = Candidate::from_sdp_string(line).map_err(|e| PeerError::InvalidArgument(format!("candidate {line}: {e}")))?;
        self.remotes.lock().unwrap().push((c.addr(), kind_str(c.kind()).to_string()));
        self.tx.lock().unwrap().send(Msg::Cmd(Cmd::AddRemoteCandidate(c))).map_err(|_| PeerError::Closed)
    }

    /// Drain outbound signaling envelopes (JSON), waiting up to `timeout` for the first one.
    pub fn poll_signals(&self, timeout: Duration) -> Vec<String> {
        let mut out = Vec::new();
        self.shared.wait_until(timeout, |s| {
            out.extend(s.signals.drain(..));
            !out.is_empty()
        });
        out
    }

    pub fn state(&self) -> LinkState {
        self.shared.st.lock().unwrap().state.unwrap_or(LinkState::New)
    }

    /// Send one binary message on the `exp` channel.
    pub fn send(&self, data: &[u8]) -> Result<(), PeerError> {
        let mut dead = None;
        let ok = self.shared.wait_until(Duration::from_secs(30), |s| {
            if matches!(s.state, Some(LinkState::Closed | LinkState::Failed)) {
                dead = Some(());
                return true;
            }
            if s.out_bytes <= SEND_HIGH_WATER {
                s.out_bytes += data.len();
                return true;
            }
            false
        });
        if dead.is_some() {
            return Err(self.failure().unwrap_or(PeerError::Closed));
        }
        if !ok {
            return Err(PeerError::Timeout);
        }
        self.tx.lock().unwrap().send(Msg::Cmd(Cmd::Send(data.to_vec()))).map_err(|_| PeerError::Closed)
    }

    /// Receive one message, waiting up to `timeout`.
    pub fn recv(&self, timeout: Duration) -> Option<Vec<u8>> {
        let mut got = None;
        self.shared.wait_until(timeout, |s| {
            got = s.inbound.pop_front();
            got.is_some() || matches!(s.state, Some(LinkState::Closed | LinkState::Failed))
        });
        got
    }

    /// Run the CLIENT side of the bench protocol against a peer that serves it
    /// (the daemon serves it automatically once connected).
    pub fn run_bench(&self, body_bytes: u64, pings: u32) -> Result<BenchResult, PeerError> {
        crate::bench::run_client(self, body_bytes, pings)
    }

    /// Serve the bench protocol until the channel closes (daemon side).
    pub fn serve_bench(&self) -> Result<(), PeerError> {
        crate::bench::serve(self)
    }

    /// ICE restart: a new signed offer envelope appears in `poll_signals`; the remote answers.
    /// Used for reconnect after backgrounding and after a Wi-Fi ↔ cellular switch.
    pub fn restart_ice(&self) -> Result<(), PeerError> {
        let sdp = self.cmd(Cmd::RestartIce)?;
        let (sdp, pubkey, sig) = self.sign_sdp(sdp)?;
        let env = self.envelope(Body::Offer { sdp, pubkey, sig });
        self.shared.update(|s| s.signals.push_back(env));
        Ok(())
    }

    /// JSON blob: local/remote candidates, nominated pair, timings, remote pubkey, rejects.
    pub fn diagnostics(&self) -> String {
        let s = self.shared.st.lock().unwrap();
        let ms = |a: Option<Instant>, b: Option<Instant>| match (a, b) {
            (Some(a), Some(b)) => serde_json::json!(b.saturating_duration_since(a).as_secs_f64() * 1000.0),
            _ => serde_json::Value::Null,
        };
        let (lp, rp, lc, rc) = self.paths_locked(&s);
        serde_json::json!({
            "peerId": self.config.peer_id,
            "pub": self.public_key(),
            "state": format!("{:?}", s.state.unwrap_or(LinkState::New)),
            "ice": s.ice_state,
            "gatherMs": self.gather_ms,
            "gatherNotes": self.gather_notes,
            "local": self.locals.iter().map(|(a, k)| format!("{k} {a}")).collect::<Vec<_>>(),
            "remote": self.remotes.lock().unwrap().iter().map(|(a, k)| format!("{k} {a}")).collect::<Vec<_>>(),
            "turnMapped": self.turn_mapped.map(|a| a.to_string()),
            "turnRelayed": self.turn_relayed.map(|a| a.to_string()),
            "localPath": lp, "remotePath": rp, "localCandidate": lc, "remoteCandidate": rc,
            "startToDtlsMs": ms(s.t_start, s.t_connected),
            "startToOpenMs": ms(s.t_start, s.t_open),
            "localFingerprint": s.local_fp,
            "remoteFingerprint": s.remote_fp,
            "remotePeer": self.remote_peer.lock().unwrap().clone(),
            "remotePub": self.remote_pubkey.lock().unwrap().clone(),
            "rejects": self.rejects.lock().unwrap().clone(),
            "errors": s.errors,
            "iceRestarts": s.ice_restarts,
        })
        .to_string()
    }

    pub fn close(&self) {
        let _ = self.tx.lock().unwrap().send(Msg::Cmd(Cmd::Close));
    }

    // ---- crate-internal helpers (not part of the frozen API) ----

    /// Wait for the channel to open; returns connect ms (offer created/received → open).
    pub(crate) fn wait_open(&self, timeout: Duration) -> Result<f64, PeerError> {
        let mut out = None;
        self.shared.wait_until(timeout, |s| {
            if s.channel_open {
                if let (Some(a), Some(b)) = (s.t_start, s.t_open) {
                    out = Some(Ok(b.saturating_duration_since(a).as_secs_f64() * 1000.0));
                } else {
                    out = Some(Ok(0.0));
                }
                return true;
            }
            if matches!(s.state, Some(LinkState::Failed | LinkState::Closed)) {
                out = Some(Err(()));
                return true;
            }
            false
        });
        match out {
            Some(Ok(ms)) => Ok(ms),
            Some(Err(())) => Err(self.failure().unwrap_or(PeerError::Closed)),
            None => Err(PeerError::Timeout),
        }
    }

    pub(crate) fn failure(&self) -> Option<PeerError> {
        let s = self.shared.st.lock().unwrap();
        s.fatal.as_ref().map(|f| {
            if f.starts_with("fingerprint_mismatch") {
                PeerError::FingerprintMismatch(f.clone())
            } else {
                PeerError::Connection(f.clone())
            }
        })
    }

    fn paths_locked(&self, s: &crate::io::St) -> (String, String, String, String) {
        let Some((src, dst)) = s.last_send else { return Default::default() };
        let lk = self.locals.iter().find(|(a, _)| *a == src).map(|(_, k)| *k).unwrap_or("host");
        let rk = self
            .remotes
            .lock()
            .unwrap()
            .iter()
            .find(|(a, _)| *a == dst)
            .map(|(_, k)| k.clone())
            .unwrap_or_else(|| "prflx".into());
        (lk.to_string(), rk.clone(), format!("{lk} {src}"), format!("{rk} {dst}"))
    }

    pub(crate) fn fill_paths(&self, r: &mut BenchResult) {
        let s = self.shared.st.lock().unwrap();
        let (lp, rp, lc, rc) = self.paths_locked(&s);
        r.local_path = lp;
        r.remote_path = rp;
        r.local_candidate = lc;
        r.remote_candidate = rc;
        r.errors.extend(s.errors.iter().cloned());
    }
}

impl Drop for PeerLink {
    fn drop(&mut self) {
        self.close();
    }
}

fn kind_of(b: &Body) -> &'static str {
    match b {
        Body::Offer { .. } => "offer",
        Body::Answer { .. } => "answer",
        _ => "envelope",
    }
}

/// Flip one hex nibble of the `a=fingerprint:` line(s) (tamper test).
pub fn tamper_fingerprint(sdp: &str) -> String {
    let mut out = String::with_capacity(sdp.len());
    for (i, line) in sdp.split_inclusive('\n').enumerate() {
        let _ = i;
        if let Some(rest) = line.strip_prefix("a=fingerprint:") {
            if let Some(sp) = rest.find(' ') {
                let (h, fp) = rest.split_at(sp + 1);
                let mut b: Vec<char> = fp.chars().collect();
                if let Some(c) = b.iter_mut().find(|c| c.is_ascii_hexdigit()) {
                    *c = if *c == '0' { '1' } else { '0' };
                }
                out.push_str("a=fingerprint:");
                out.push_str(h);
                out.extend(b);
                continue;
            }
        }
        out.push_str(line);
    }
    out
}
