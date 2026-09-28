//! UniFFI facade over peer-core. FROZEN for the spike: Swift (PeerFFI.xcframework) and Kotlin
//! (libpeer_ffi.so + JNA) bindings are generated from this file with
//!   cargo run -p peer-ffi --features bindgen --bin uniffi-bindgen -- generate --library <lib> --language swift|kotlin --out-dir <dir>
use std::sync::Arc;
use std::time::Duration;

uniffi::setup_scaffolding!();

#[derive(uniffi::Enum, Clone, Copy)]
pub enum IcePolicy {
    All,
    Relay,
    Host,
}

#[derive(uniffi::Enum, Clone, Copy)]
pub enum LinkState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(uniffi::Record, Clone)]
pub struct PeerConfig {
    pub peer_id: String,
    pub session_id: String,
    pub stun: Option<String>,
    pub turn: Option<String>,
    pub turn_username: Option<String>,
    pub turn_password: Option<String>,
    pub turn_realm: Option<String>,
    pub policy: IcePolicy,
    pub identity_seed: Option<Vec<u8>>,
    pub expected_remote_pubkey: Option<String>,
    pub tamper_sdp: bool,
    pub tamper_sig: bool,
}

#[derive(uniffi::Record, Clone)]
pub struct BenchResult {
    pub connect_ms: f64,
    pub rtt_p50_ms: f64,
    pub rtt_p95_ms: f64,
    pub up_mbps: f64,
    pub down_mbps: f64,
    pub body_bytes: u64,
    pub local_path: String,
    pub remote_path: String,
    pub local_candidate: String,
    pub remote_candidate: String,
    pub errors: Vec<String>,
    /// The same result as JSON (for the `result` envelope).
    pub json: String,
}

#[derive(uniffi::Error, Debug, thiserror::Error)]
#[uniffi(flat_error)]
pub enum PeerError {
    #[error("{0}")]
    Failed(String),
}

impl From<peer_core::PeerError> for PeerError {
    fn from(e: peer_core::PeerError) -> Self {
        PeerError::Failed(e.to_string())
    }
}

fn to_core(c: PeerConfig) -> peer_core::PeerConfig {
    peer_core::PeerConfig {
        peer_id: c.peer_id,
        session_id: c.session_id,
        stun: c.stun,
        turn: c.turn,
        turn_username: c.turn_username,
        turn_password: c.turn_password,
        turn_realm: c.turn_realm,
        policy: match c.policy {
            IcePolicy::All => peer_core::IcePolicy::All,
            IcePolicy::Relay => peer_core::IcePolicy::Relay,
            IcePolicy::Host => peer_core::IcePolicy::Host,
        },
        identity_seed: c.identity_seed,
        expected_remote_pubkey: c.expected_remote_pubkey,
        tamper_sdp: c.tamper_sdp,
        tamper_sig: c.tamper_sig,
    }
}

#[derive(uniffi::Object)]
pub struct PeerLink {
    inner: Arc<peer_core::PeerLink>,
}

#[uniffi::export]
impl PeerLink {
    #[uniffi::constructor]
    pub fn new(config: PeerConfig) -> Result<Arc<Self>, PeerError> {
        Ok(Arc::new(Self { inner: peer_core::PeerLink::new(to_core(config))? }))
    }
    pub fn public_key(&self) -> String {
        self.inner.public_key()
    }
    pub fn create_offer(&self) -> Result<String, PeerError> {
        Ok(self.inner.create_offer()?)
    }
    pub fn accept_offer(&self, envelope_json: String) -> Result<String, PeerError> {
        Ok(self.inner.accept_offer(&envelope_json)?)
    }
    pub fn accept_answer(&self, envelope_json: String) -> Result<(), PeerError> {
        Ok(self.inner.accept_answer(&envelope_json)?)
    }
    pub fn add_remote_candidate(&self, candidate: String) -> Result<(), PeerError> {
        Ok(self.inner.add_remote_candidate(&candidate)?)
    }
    /// Blocks up to `timeout_ms` for the first outbound signaling envelope, then drains.
    pub fn poll_signals(&self, timeout_ms: u32) -> Vec<String> {
        self.inner.poll_signals(Duration::from_millis(timeout_ms as u64))
    }
    pub fn state(&self) -> LinkState {
        match self.inner.state() {
            peer_core::LinkState::New => LinkState::New,
            peer_core::LinkState::Connecting => LinkState::Connecting,
            peer_core::LinkState::Connected => LinkState::Connected,
            peer_core::LinkState::Disconnected => LinkState::Disconnected,
            peer_core::LinkState::Failed => LinkState::Failed,
            peer_core::LinkState::Closed => LinkState::Closed,
        }
    }
    pub fn send(&self, data: Vec<u8>) -> Result<(), PeerError> {
        Ok(self.inner.send(&data)?)
    }
    pub fn recv(&self, timeout_ms: u32) -> Option<Vec<u8>> {
        self.inner.recv(Duration::from_millis(timeout_ms as u64))
    }
    /// Client side of the bench protocol (the daemon serves it once connected).
    pub fn run_bench(&self, body_bytes: u64, pings: u32) -> Result<BenchResult, PeerError> {
        let r = self.inner.run_bench(body_bytes, pings)?;
        let json = serde_json::to_string(&r).unwrap_or_default();
        Ok(BenchResult {
            connect_ms: r.connect_ms,
            rtt_p50_ms: r.rtt_p50_ms,
            rtt_p95_ms: r.rtt_p95_ms,
            up_mbps: r.up_mbps,
            down_mbps: r.down_mbps,
            body_bytes: r.body_bytes,
            local_path: r.local_path,
            remote_path: r.remote_path,
            local_candidate: r.local_candidate,
            remote_candidate: r.remote_candidate,
            errors: r.errors,
            json,
        })
    }
    pub fn restart_ice(&self) -> Result<(), PeerError> {
        Ok(self.inner.restart_ice()?)
    }
    pub fn diagnostics(&self) -> String {
        self.inner.diagnostics()
    }
    pub fn close(&self) {
        self.inner.close()
    }
}

#[uniffi::export]
pub fn peer_version() -> String {
    peer_core::version().to_string()
}

/// Build a signaling envelope carrying a free-form result payload (JSON string) for the daemon.
#[uniffi::export]
pub fn result_envelope(from: String, to: Option<String>, session_id: String, payload_json: String) -> String {
    let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::String(payload_json));
    peer_core::signal::Envelope::new(&from, to.as_deref(), &session_id, peer_core::signal::Body::Result { payload }).to_json()
}
