//! The peer link: FROZEN API for the spike (peer-ffi mirrors it 1:1 for Swift/Kotlin; peer-cli
//! drives it). Lane A implements the bodies (str0m Rtc + UDP I/O thread + STUN/TURN gathering +
//! bench protocol). Until then every method returns `PeerError::NotImplemented` so the mobile
//! lanes can build, link and wire the apps.
use std::sync::Arc;
use std::time::Duration;

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
pub struct PeerLink {
    _config: PeerConfig,
}

impl PeerLink {
    pub fn new(config: PeerConfig) -> Result<Arc<Self>, PeerError> {
        crate::install_crypto();
        Ok(Arc::new(Self { _config: config }))
    }

    /// base64url Ed25519 public key of this peer.
    pub fn public_key(&self) -> String {
        String::new()
    }

    /// Offerer: create the data channel + a signed offer envelope (JSON) to send to the remote.
    pub fn create_offer(&self) -> Result<String, PeerError> {
        Err(PeerError::NotImplemented)
    }

    /// Answerer: verify + apply a signed offer envelope, return the signed answer envelope.
    pub fn accept_offer(&self, envelope_json: &str) -> Result<String, PeerError> {
        let _ = envelope_json;
        Err(PeerError::NotImplemented)
    }

    /// Offerer: verify + apply the signed answer envelope.
    pub fn accept_answer(&self, envelope_json: &str) -> Result<(), PeerError> {
        let _ = envelope_json;
        Err(PeerError::NotImplemented)
    }

    /// Add a remote ICE candidate (`candidate` envelope JSON or a bare `candidate:` line).
    pub fn add_remote_candidate(&self, candidate: &str) -> Result<(), PeerError> {
        let _ = candidate;
        Err(PeerError::NotImplemented)
    }

    /// Drain outbound signaling envelopes (JSON), waiting up to `timeout` for the first one.
    pub fn poll_signals(&self, timeout: Duration) -> Vec<String> {
        let _ = timeout;
        Vec::new()
    }

    pub fn state(&self) -> LinkState {
        LinkState::New
    }

    /// Send one binary message on the `exp` channel.
    pub fn send(&self, data: &[u8]) -> Result<(), PeerError> {
        let _ = data;
        Err(PeerError::NotImplemented)
    }

    /// Receive one message, waiting up to `timeout`.
    pub fn recv(&self, timeout: Duration) -> Option<Vec<u8>> {
        let _ = timeout;
        None
    }

    /// Run the CLIENT side of the bench protocol against a peer that serves it
    /// (the daemon serves it automatically once connected).
    pub fn run_bench(&self, body_bytes: u64, pings: u32) -> Result<BenchResult, PeerError> {
        let _ = (body_bytes, pings);
        Err(PeerError::NotImplemented)
    }

    /// Serve the bench protocol until the channel closes (daemon side).
    pub fn serve_bench(&self) -> Result<(), PeerError> {
        Err(PeerError::NotImplemented)
    }

    /// ICE restart: a new signed offer envelope appears in `poll_signals`; the remote answers.
    /// Used for reconnect after backgrounding and after a Wi-Fi ↔ cellular switch.
    pub fn restart_ice(&self) -> Result<(), PeerError> {
        Err(PeerError::NotImplemented)
    }

    /// JSON blob: local/remote candidates, nominated pair, timings, remote pubkey, rejects.
    pub fn diagnostics(&self) -> String {
        "{}".into()
    }

    pub fn close(&self) {}
}
