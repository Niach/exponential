//! `steer` — the relay publisher (masterplan-v3 §3.1 / §08).
//!
//! Two modules over one WebSocket client stack, on the workspace's ONLY tokio
//! runtime (isolated from gpui's executors and the blocking ureq sync stack):
//!
//! - [`control_channel`] — the per-account device-presence socket: `online`
//!   registration, inbound `start_session` routing to the §7 launcher,
//!   15-minute disabled recheck, exponential backoff with the >60s-lived
//!   reset rule.
//! - [`publisher`] — the per-coding-session PTY publisher: tee `0x01` output
//!   frames off the §6 read-loop [`terminal::RawSink`] seam, inject remote
//!   `input` into the shared PTY writer, resize both ways, ring replay on
//!   `resync`, claim/take-over, kill, and auto-reconnect resuming the room.
//!
//! Wire protocol and ticket format are FROZEN (`apps/steer-relay/src/protocol.ts`,
//! `packages/steer-ticket`) — [`frames`] mirrors them byte-for-byte and the
//! desktop is a ticket **consumer only** (server-minted over
//! `api::steer::mint_*`; it never signs, §8.0/§8.2).
//!
//! ## The seams other crates consume (stated for the §7/§8 hookup)
//!
//! The `coding` crate deliberately does not depend on `steer` (§3.1); the
//! app/ui layer (the coding-flow glue) wires both:
//!
//! 1. **Publisher attach** — after `coding::spawn_prepared` returns
//!    `LaunchOutcome::Spawned { session_id, .. }`, call
//!    [`publisher::publish`] with a [`publisher::PublisherHooks`] built from
//!    the tab's `Terminal` (`session.writer()` for input inject,
//!    `terminal::grid_size(session.term())` for true geometry) — then attach
//!    `handle.raw_sink()` via `Terminal::attach_sink` and detach it on
//!    teardown. Call `handle.notify_local_resize` from the §6.10 resize path,
//!    `handle.take_over()` from the "Take over" banner button, and
//!    `handle.shutdown(Some("exit:<code>"))` from the exit hook.
//! 2. **Control channel** — once per signed-in account, call
//!    [`control_channel::spawn_control_channel`] with the persistent
//!    [`persistent_device_id`], `api::users::hostname()` as the label, and an
//!    `on_start_session` closure that marshals to the gpui foreground and
//!    runs the §7 launcher with `LaunchOrigin::Remote`.
//! 3. **Kill-switch** — the §8.8 own-row Electric watch lives in
//!    `sync::kill_watch` (steer cannot depend on `sync`); its `on_ended`
//!    callback kills the child (`Terminal::kill`) and calls
//!    `handle.session_ended()` so the publisher stops reconnecting and says
//!    a clean `bye`.

pub mod control_channel;
pub mod frames;
pub mod publisher;
pub mod ring;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

pub use api::steer::{MintTicketResult, MintedTicket, SteerConfig, SteerDevice};
pub use control_channel::{
    spawn_control_channel, ControlApi, ControlChannelHandle, DeviceIdentity, TrpcControlApi,
};
pub use frames::{
    output_frame, ClientFrame, PresenceViewer, ServerFrame, SteerPerm, SteerRole,
    CLOSE_REPLACED, CLOSE_SESSION_ENDED, CLOSE_SLOW_CONSUMER, CLOSE_UNAUTHORIZED, OUTPUT_OPCODE,
};
pub use publisher::{
    publish, KillSignal, Presence, PublishSpec, PublisherHandle, PublisherHooks,
    PublisherTickets, TrpcPublisherTickets, IN_FLIGHT_CAP,
};
pub use ring::{RingBuffer, RING_CAP_BYTES};

// ---------------------------------------------------------------------------
// The isolated tokio runtime (§3.5: "the only tokio in the whole desktop
// workspace lives in the steer crate … on its own runtime")
// ---------------------------------------------------------------------------

/// Owns the steer subsystem's tokio runtime. Create ONE per app process and
/// share it (`Arc`) between the control channel and all publishers. Dropping
/// it shuts the runtime down in the background (never blocks the foreground).
pub struct SteerRuntime {
    runtime: tokio::runtime::Runtime,
}

impl SteerRuntime {
    pub fn new() -> std::io::Result<Arc<Self>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("steer-ws")
            .enable_all()
            .build()?;
        Ok(Arc::new(Self { runtime }))
    }

    /// Public for the relay integration tests (they spawn fake viewer
    /// sockets on the same runtime); not part of the app-facing API.
    pub fn handle(&self) -> &tokio::runtime::Handle {
        self.runtime.handle()
    }
}

// ---------------------------------------------------------------------------
// Persistent deviceId (§8.2) — file-based per the §5.7 store posture
// ---------------------------------------------------------------------------

/// The install-persistent `deviceId` (§8.2): ONE id per install, shared with
/// the §7.7 Trust & Run device identity (`{data_dir}/settings.json`,
/// `deviceId` key — file-based, never keyring). A stable id lets the relay's
/// replace-on-reconnect evict the stale socket (`CLOSE_REPLACED`) instead of
/// accumulating ghost devices in the phone picker.
pub use api::trust_store::device_id as persistent_device_id;

// ---------------------------------------------------------------------------
// Ticket claims — consume only (§8.2)
// ---------------------------------------------------------------------------

/// Mirror of `packages/steer-ticket` `SteerTicketClaims` — deserialize-only,
/// for logging/telemetry and §8.7 skew checks. The desktop NEVER verifies the
/// HMAC (that is the relay's job; we lack — and must never hold — the secret).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerTicketClaims {
    pub sub: String,
    pub ws: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub role: SteerRole,
    pub perm: SteerPerm,
    /// Unix seconds.
    pub iat: i64,
    /// Unix seconds — the ~60s connect window; the socket outlives it.
    pub exp: i64,
}

/// Parse the claims half of a `base64url(json).base64url(hmac)` ticket.
/// Signature is deliberately NOT checked (§8.2).
pub fn parse_ticket_claims(ticket: &str) -> Option<SteerTicketClaims> {
    let (payload, _sig) = ticket.split_once('.')?;
    let json = base64url_decode(payload)?;
    serde_json::from_slice(&json).ok()
}

/// Minimal RFC 4648 base64url (no padding) decoder — avoids a crypto/base64
/// dependency for a read-only claims peek.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some((byte - b'A') as u32),
            b'a'..=b'z' => Some((byte - b'a' + 26) as u32),
            b'0'..=b'9' => Some((byte - b'0' + 52) as u32),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = input.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() == 1 {
            return None; // 6 bits cannot encode a byte
        }
        let mut acc: u32 = 0;
        for &byte in chunk {
            acc = (acc << 6) | value(byte)?;
        }
        let bits = chunk.len() * 6;
        acc <<= 24 - bits;
        let produced = (bits - 6) / 8 + usize::from(bits % 8 != 0 && bits > 8);
        let produced = produced.min(3).max((chunk.len() * 6) / 8);
        let full = acc.to_be_bytes();
        out.extend_from_slice(&full[1..1 + produced]);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Backoff (shared by control channel + publisher)
// ---------------------------------------------------------------------------

/// Exponential backoff with full jitter: each failure doubles the bound
/// (base → cap); [`Backoff::next_delay`] returns a uniform sample in
/// `[0, bound]`. Reset on sustained success (§8.3 #5 / §8.6).
pub struct Backoff {
    base: Duration,
    cap: Duration,
    current: Duration,
    rng: XorShift64,
}

impl Backoff {
    pub fn new(base: Duration, cap: Duration) -> Self {
        Self {
            base,
            cap,
            current: base,
            rng: XorShift64::seeded(),
        }
    }

    /// The §8.3 control-channel policy: 250ms base, 30s cap.
    pub fn control() -> Self {
        Self::new(Duration::from_millis(250), Duration::from_secs(30))
    }

    /// The §8.6 publisher policy: 250ms base, 15s cap (reconnect promptly —
    /// the relay's `staleTimer` bounds the room's grace window).
    pub fn publisher() -> Self {
        Self::new(Duration::from_millis(250), Duration::from_secs(15))
    }

    /// Current un-jittered bound (test/observability surface).
    pub fn bound(&self) -> Duration {
        self.current
    }

    /// Sample the next delay (full jitter over the current bound), then
    /// double the bound toward the cap.
    pub fn next_delay(&mut self) -> Duration {
        let bound = self.current;
        self.current = (self.current * 2).min(self.cap);
        let nanos = bound.as_nanos() as u64;
        if nanos == 0 {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.rng.next() % (nanos + 1))
    }

    /// Back to base — call after a connection proves itself (§8.3 #5: a
    /// socket that outlived the 60s ticket window is a genuine success).
    pub fn reset(&mut self) {
        self.current = self.base;
    }
}

/// A connection that lived at least this long resets its channel's backoff
/// (the ">60s-lived" rule — outliving the ticket window proves the path).
pub const BACKOFF_RESET_AFTER: Duration = Duration::from_secs(60);

/// Tiny xorshift64* PRNG for jitter — NOT cryptographic (jitter only).
struct XorShift64(u64);

impl XorShift64 {
    fn seeded() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        Self(nanos ^ ((std::process::id() as u64) << 32) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545f4914f6cdd1d)
    }
}

// ---------------------------------------------------------------------------
// Dialing (§8.7 — ws AND wss, never force TLS)
// ---------------------------------------------------------------------------

/// How long a single WebSocket connect may take before it counts as a
/// failure. Also bounds the §8.7 mint→dial budget (dial immediately; the
/// ticket window is ~60s).
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Why a dial failed. The relay rejects a bad/expired ticket with an HTTP 401
/// at upgrade time (`apps/steer-relay/src/index.ts`), which is the §8.7
/// "expired on arrival" signal — distinct from transport failures.
#[derive(Debug)]
pub(crate) enum DialError {
    /// HTTP 401 at upgrade: ticket expired/bad — re-mint once, then surface
    /// the clock-skew error (§8.7).
    Unauthorized,
    Other(String),
}

/// Dial the server-provided ticket URL **as-is** (§8.2 — never reconstruct
/// it; the relay reads `?ticket=` from the query string). `connect_async`
/// branches on the URL scheme itself: `ws://` plain TCP (LAN self-host),
/// `wss://` rustls with native roots (cloud). Self-signed relay certs are a
/// deliberate non-goal for now (§8.7 open question — no
/// `danger_accept_invalid_certs` escape hatch by default).
pub(crate) async fn dial(url: &str) -> Result<WsStream, DialError> {
    match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((stream, _response))) => Ok(stream),
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response)))
            if response.status().as_u16() == 401 =>
        {
            Err(DialError::Unauthorized)
        }
        Ok(Err(err)) => Err(DialError::Other(redact_ticket(&err.to_string()))),
        Err(_elapsed) => Err(DialError::Other(format!(
            "connect timed out after {CONNECT_TIMEOUT:?}"
        ))),
    }
}

/// Keep tickets out of logs/errors: URLs carry `?ticket=<sensitive>`.
pub(crate) fn redact_ticket(message: &str) -> String {
    match message.find("ticket=") {
        Some(at) => {
            let end = message[at..]
                .find(|c: char| c == '&' || c == ' ' || c == '"')
                .map(|rel| at + rel)
                .unwrap_or(message.len());
            format!("{}ticket=<redacted>{}", &message[..at], &message[end..])
        }
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_cap_and_resets() {
        let mut backoff = Backoff::control();
        assert_eq!(backoff.bound(), Duration::from_millis(250));
        let mut bounds = Vec::new();
        for _ in 0..10 {
            bounds.push(backoff.bound());
            let delay = backoff.next_delay();
            assert!(delay <= *bounds.last().unwrap(), "jitter within the bound");
        }
        assert_eq!(bounds[0], Duration::from_millis(250));
        assert_eq!(bounds[1], Duration::from_millis(500));
        assert_eq!(bounds[2], Duration::from_secs(1));
        assert_eq!(*bounds.last().unwrap(), Duration::from_secs(30), "capped");
        backoff.reset();
        assert_eq!(backoff.bound(), Duration::from_millis(250));
    }

    #[test]
    fn publisher_backoff_caps_at_15s() {
        let mut backoff = Backoff::publisher();
        for _ in 0..12 {
            backoff.next_delay();
        }
        assert_eq!(backoff.bound(), Duration::from_secs(15));
    }

    #[test]
    fn device_id_is_stable_per_install() {
        // Identity is owned by api::trust_store (§7.7); steer only requires
        // stability — the relay's replace-on-reconnect depends on it (§8.2).
        let dir = std::env::temp_dir().join(format!(
            "exp-steer-device-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = persistent_device_id(&dir);
        let second = persistent_device_id(&dir);
        assert_eq!(first, second, "one UUID per install");
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_ticket_claims_without_verifying() {
        // A real ticket shape: base64url(JSON claims) + "." + base64url(sig).
        // Signature is garbage on purpose — parse must not care.
        let claims_json = r#"{"sub":"user-1","ws":"ws-1","sessionId":"sess-1","role":"publisher","perm":"steer","iat":1751500000,"exp":1751500060}"#;
        let payload = base64url_encode_for_test(claims_json.as_bytes());
        let ticket = format!("{payload}.AAAA");
        let claims = parse_ticket_claims(&ticket).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.session_id.as_deref(), Some("sess-1"));
        assert_eq!(claims.role, SteerRole::Publisher);
        assert_eq!(claims.perm, SteerPerm::Steer);
        assert_eq!(claims.exp - claims.iat, 60);
        assert_eq!(parse_ticket_claims("no-dot"), None);
        assert_eq!(parse_ticket_claims("!!!.sig"), None);
    }

    #[test]
    fn base64url_decodes_all_lengths() {
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let encoded = base64url_encode_for_test(input);
            assert_eq!(
                base64url_decode(&encoded).as_deref(),
                Some(input),
                "round-trip {input:?}"
            );
        }
        assert_eq!(base64url_decode("A"), None, "lone sextet is malformed");
    }

    #[test]
    fn redacts_tickets_from_error_text() {
        assert_eq!(
            redact_ticket("connect ws://relay.lan/ws?ticket=abc.def failed"),
            "connect ws://relay.lan/ws?ticket=<redacted> failed"
        );
        assert_eq!(redact_ticket("plain error"), "plain error");
    }

    /// Test-only base64url encoder (prod code never encodes tickets).
    fn base64url_encode_for_test(input: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let mut acc: u32 = 0;
            for (i, &byte) in chunk.iter().enumerate() {
                acc |= (byte as u32) << (16 - i * 8);
            }
            let sextets = [
                (acc >> 18) & 0x3f,
                (acc >> 12) & 0x3f,
                (acc >> 6) & 0x3f,
                acc & 0x3f,
            ];
            let keep = 1 + chunk.len() * 8 / 6;
            for &sextet in sextets.iter().take(keep) {
                out.push(ALPHABET[sextet as usize] as char);
            }
        }
        out
    }
}
