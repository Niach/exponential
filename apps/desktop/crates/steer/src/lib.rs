//! `steer` — the desktop's relay client, in all THREE roles (masterplan-v3
//! §3.1 / §08).
//!
//! Three modules over one WebSocket client stack, on the team's ONLY tokio
//! runtime (isolated from gpui's executors and the blocking reqwest sync stack):
//!
//! - [`control_channel`] — the per-account device-presence socket: `online`
//!   registration, inbound `start_session` routing to the §7 launcher,
//!   15-minute disabled recheck, exponential backoff with the >60s-lived
//!   reset rule.
//! - [`publisher`] — the per-coding-session publisher: push the scrubbed
//!   [`activity`] stream, replay the session [`journal`] on every (re)connect,
//!   inject remote `input`/`answer` into the shared PTY writer, claim/
//!   take-over, kill, and auto-reconnect resuming the room. EXP-249 removed
//!   the binary PTY mirror it used to carry.
//! - [`viewer`] (EXP-696) — the other end of that wire: watch and steer a
//!   session running on ANOTHER of the user's devices, the role web/iOS/
//!   Android have had since EXP-249. It joins `channel:'activity'`, turns the
//!   inbound stream into [`viewer::ViewerEvent`]s and sends messages/answers/
//!   keystrokes back. The pure feed model it drives is [`feed`] (a port of
//!   the web store's reducer + `agent-feed.ts`), and the composer's image
//!   template is [`image_message`]. Transport and model are deliberately
//!   split: the feed is testable with no socket, the socket with no UI.
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
//!    the tab's `Terminal` (`session.writer()` for input inject), then start
//!    [`activity::spawn_emitter`] with `handle.activity_sender()`, the
//!    session's [`hooks::HookServer`] receiver, and an [`activity::Steering`]
//!    seam whose [`activity::AnswerLink`] also rides the publisher hooks. Call
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
//! 4. **Viewer attach** (EXP-696) — for a session hosted ELSEWHERE, call
//!    [`viewer::spawn_viewer`] with a [`viewer::TrpcViewerTickets`] and a
//!    `flume` sender; drain the receiver on the UI side into a
//!    [`feed::SteerFeed`] and render it. The UI owns exactly two timers —
//!    the [`feed::ANSWER_ACK_TIMEOUT`] card lock and the EXP-656
//!    [`feed::REPLAY_QUIET`]/[`feed::REPLAY_MAX`] staged-replay fallback —
//!    and feeds the session's synced status back with
//!    [`viewer::ViewerHandle::note_session_ended`] so the redial loops stop
//!    chasing a publisher that is gone.

pub mod activity;
pub mod agent_login_driver;
pub mod codex_activity;
pub mod codex_approval_picker;
pub mod codex_login_picker;
pub mod commands;
pub mod control_channel;
pub mod feed;
pub mod frames;
pub mod hooks;
pub mod image_message;
pub mod journal;
pub mod login_picker;
pub mod permission_picker;
pub mod pi_activity;
pub mod pi_observer;
pub mod plan_picker;
pub mod publisher;
pub mod question_picker;
pub mod viewer;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

pub use api::steer::{MintTicketResult, MintedTicket, SteerConfig};
pub use control_channel::{
    spawn_control_channel, ControlApi, ControlChannelHandle, DeviceIdentity, RemoteStart,
    RemoteStartSubject, TrpcControlApi,
};
pub use activity::{
    spawn_emitter as spawn_activity_emitter, AnswerLink, EmitterConfig, Redactor, RemoteAnswer,
    Steering, TurnSignal,
};
pub use feed::{
    active_question_ids, answer_key, collect_subagents, group_feed_rows, summarize_subagent_row,
    AnswerState, AnswerStatus, FeedItem, FeedItemId, FeedKind, FeedRow, QuestionCard, SteerFeed,
    SubagentSummary, ANSWER_ACK_TIMEOUT, ECHO_CAP, FEED_CAP, REPLAY_MAX, REPLAY_QUIET,
};
pub use frames::{
    ActivityEvent, ClientFrame, QuestionOption, ServerFrame, StartInput, StartRepoGroup,
    SteerRole, SubagentStatus, ViewerFrame, ACTIVITY_CHANNEL, CLOSE_REPLACED, CLOSE_SESSION_ENDED,
    CLOSE_SLOW_CONSUMER, CLOSE_UNAUTHORIZED,
};
pub use image_message::{
    build_steer_image_message, image_marker, insert_image_marker, parse_steer_message,
    renumber_image_markers, ParsedSteerMessage, MAX_STEER_IMAGES,
};
pub use hooks::{
    hook_settings_json, write_hook_curl_config, HookContext, HookEvent, HookEventKind,
    HookQuestion, HookQuestionOption, HookServer, HOOK_CONFIG_ENV, HOOK_PORT_ENV,
};
pub use journal::{ActivityJournal, JOURNAL_BYTE_CAP, JOURNAL_EVENT_CAP};
pub use publisher::{
    image_localizer, publish, ActivitySender, AttachmentHook, KillSignal, PublishSpec,
    PublisherHandle, PublisherHooks, PublisherTickets, TrpcPublisherTickets,
};
pub use viewer::{
    chunk_input, spawn_viewer, spawn_viewer_with, TrpcViewerTickets, ViewerEvent, ViewerHandle,
    ViewerPhase, ViewerTickets, ViewerTimings, INPUT_CHUNK_UTF16,
};

// ---------------------------------------------------------------------------
// The isolated tokio runtime (§3.5: "the only tokio in the whole desktop
// team lives in the steer crate … on its own runtime")
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
pub use api::device_identity::device_id as persistent_device_id;

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
    /// teamId the ticket is scoped to (empty string for control tickets).
    pub team: String,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub role: SteerRole,
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

    /// The EXP-696 viewer policy: 3s base, 30s cap — the web store's
    /// `startingRetryDelay` and the natives' reconnect backoff, to the
    /// millisecond. Pair it with [`Backoff::next_delay_equal_jitter`].
    pub fn viewer() -> Self {
        Self::new(Duration::from_secs(3), Duration::from_secs(30))
    }

    /// Current un-jittered bound (test/observability surface).
    pub fn bound(&self) -> Duration {
        self.current
    }

    /// Equal jitter: half the bound fixed, half random (`bound/2 + rand *
    /// bound/2`). Desynchronizes clients that started waiting together while
    /// keeping a FLOOR on the delay — full jitter can sample ~0 and redial
    /// instantly, which is right for a publisher racing to resume its room
    /// and wrong for a herd of viewers waiting on a desktop that is still
    /// starting. Mirrors the web/iOS/Android viewer backoff.
    pub fn next_delay_equal_jitter(&mut self) -> Duration {
        let bound = self.current;
        self.current = (self.current * 2).min(self.cap);
        let nanos = bound.as_nanos() as u64;
        if nanos == 0 {
            return Duration::ZERO;
        }
        let half = nanos / 2;
        Duration::from_nanos(half + self.rng.next() % (half + 1))
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
                .find(['&', ' ', '"'])
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
        // Identity is owned by api::device_identity (§7.7); steer only requires
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
        let claims_json = r#"{"sub":"user-1","team":"team-1","sessionId":"sess-1","role":"publisher","iat":1751500000,"exp":1751500060}"#;
        let payload = base64url_encode_for_test(claims_json.as_bytes());
        let ticket = format!("{payload}.AAAA");
        let claims = parse_ticket_claims(&ticket).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.session_id.as_deref(), Some("sess-1"));
        assert_eq!(claims.role, SteerRole::Publisher);
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
