//! The ONE HTTP client the desktop app uses (EXP-304).
//!
//! Every outbound request — Electric shape long-polls, tRPC, Better Auth,
//! attachment uploads, the self-updater's release download — goes through the
//! single [`reqwest::blocking::Client`] handed out by [`shared`]. That is not
//! tidiness for its own sake:
//!
//! * **One connection instead of fifteen.** The old `ureq` agent was HTTP/1.1
//!   only, so each of the 16 shape threads dialled its own socket. Every launch
//!   fired 15 simultaneous cold DNS lookups and TLS handshakes at the instance,
//!   and that storm — not the amount of data — is what put ~10s between opening
//!   a client and seeing current state (the mobile clients had the same shape of
//!   bug). reqwest negotiates HTTP/2 via ALPN and multiplexes every request to a
//!   host onto one connection.
//! * **Shared pool.** tRPC calls made while sync is running reuse the very same
//!   connection instead of racing sync for a new one.
//!
//! ## Blocking, deliberately
//!
//! §5.3 rules out an async runtime under gpui's executors. `reqwest::blocking`
//! satisfies that: it owns a private tokio runtime on its own background thread
//! and every call site here stays an ordinary synchronous function. What it does
//! NOT tolerate is being *constructed* from inside an async context, and
//! `crates/steer` reaches into this crate from `spawn_blocking`. Hence
//! [`init`]: call it once, first thing in `main`, from the foreground thread.
//! [`shared`] will lazily initialise as a backstop, but relying on that is how
//! you end up building the client on a tokio worker.
//!
//! ## Dead connections (FEED-69)
//!
//! One multiplexed connection is also one point of failure: when the network
//! kills it silently (sleep, Wi-Fi roam, VPN flip, NAT timeout) every request
//! keeps riding the corpse until something notices. Three layers notice:
//! HTTP/2 keepalive pings close a connection whose peer stopped answering,
//! idle connections leave the pool after [`POOL_IDLE_TIMEOUT`], and a streak
//! of transport failures ([`record_failure`]) swaps in a brand-new client
//! (fresh pool, fresh DNS) — which [`reset`] also does on demand. So
//! [`shared`] hands out the CURRENT client: fetch it per request, never store
//! it in a struct.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;

/// Connect budget for every request. Generous enough for a cold radio /
/// VPN-establishing path, short enough that a black-holed address fails into
/// the caller's retry instead of hanging.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Whole-request budget for ordinary calls (tRPC, auth, uploads) — iOS
/// URLSession parity. Long-polls override this per request; see
/// `sync::client::LIVE_READ_TIMEOUT`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Idle connections kept per host. One per shape (16) plus tRPC headroom, so a
/// poll cycle reuses connections instead of re-dialling — HTTP/2 collapses
/// these into one anyway, but the HTTP/1.1 fallback (plain-HTTP local dev)
/// needs the room.
const POOL_MAX_IDLE_PER_HOST: usize = 32;

/// An idle pooled connection is dropped after this long, so a request after a
/// quiet spell dials fresh instead of reusing a socket the network may have
/// forgotten. Under the usual 60s NAT/load-balancer idle cut.
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// HTTP/2 PING cadence, sent even with no stream open. A connection whose
/// peer does not answer within [`H2_KEEPALIVE_TIMEOUT`] is closed, failing
/// its streams into the callers' retries — which then dial a fresh one.
const H2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
/// The timeout leaves room for a saturated uplink: a big upload delays the
/// PING ack, and 10s killed the shared connection with every long-poll on it.
const H2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(20);

/// TCP keepalive: first probe after 30s idle, then every 10s, dead after 3
/// unanswered. Without the explicit interval/retries the OS defaults apply
/// (macOS: 8 probes 75s apart = ten minutes on a dead socket, FEED-69).
const TCP_KEEPALIVE_IDLE: Duration = Duration::from_secs(30);
const TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const TCP_KEEPALIVE_RETRIES: u32 = 3;

/// Consecutive connection-level failures (across every caller) that swap the
/// client for a fresh one, and the floor between two such swaps. The floor is
/// jittered per rebuild ([`jittered`]) so a fleet of desktops behind one
/// deploy does not drop its pools in lockstep.
const FAILURES_BEFORE_REBUILD: u32 = 3;
const REBUILD_COOLDOWN: Duration = Duration::from_secs(15);

/// Spread of [`jittered`]: ±30%.
const JITTER_PERCENT: u64 = 30;

static CLIENT: OnceLock<RwLock<Client>> = OnceLock::new();
static FAILURE_STREAK: AtomicU32 = AtomicU32::new(0);
/// When the client was last rebuilt, and that rebuild's jittered cooldown.
static LAST_REBUILD: Mutex<Option<(Instant, Duration)>> = Mutex::new(None);

/// `base` ±30%, uniformly: retry timers that every client would otherwise
/// fire in the same instant (the rebuild cooldown, sync's error backoff).
pub fn jittered(base: Duration) -> Duration {
    // A v4 uuid is the randomness this crate already links.
    let roll = (uuid::Uuid::new_v4().as_u128() % 1_000) as u64;
    jitter_at(base, roll)
}

/// [`jittered`] for one `roll` in `0..1000` (0 = −30%, 999 ≈ +30%).
fn jitter_at(base: Duration, roll: u64) -> Duration {
    let percent = 100 - JITTER_PERCENT + roll.min(999) * 2 * JITTER_PERCENT / 999;
    base * percent as u32 / 100
}

fn build() -> Client {
    // The keepalive pings live on the async builder only; the blocking
    // builder wraps one, so configure there and convert.
    let builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
        // All 16 shape long-polls ride ONE connection once HTTP/2 is
        // negotiated, so a connection the network killed silently would stall
        // every one of them until its own 90s budget expired — and tRPC with
        // them. The pings catch it on HTTP/2, the TCP probes on the HTTP/1.1
        // fallback too; both also keep NAT/firewall state alive across a long
        // idle hold.
        .http2_keep_alive_interval(H2_KEEPALIVE_INTERVAL)
        .http2_keep_alive_timeout(H2_KEEPALIVE_TIMEOUT)
        .http2_keep_alive_while_idle(true)
        .tcp_keepalive(TCP_KEEPALIVE_IDLE)
        .tcp_keepalive_interval(TCP_KEEPALIVE_INTERVAL)
        .tcp_keepalive_retries(TCP_KEEPALIVE_RETRIES);
    // No `http2_prior_knowledge`: ALPN negotiates, so a plain-HTTP local
    // backend cleanly falls back to HTTP/1.1 (still pooled).
    reqwest::blocking::ClientBuilder::from(builder)
        .timeout(DEFAULT_TIMEOUT)
        .build()
        .expect("failed to build the shared HTTP client")
}

fn slot() -> &'static RwLock<Client> {
    CLIENT.get_or_init(|| RwLock::new(build()))
}

/// Build the shared client eagerly. Call once from `main`, on the foreground
/// thread, before anything can reach [`shared`] from a tokio context.
pub fn init() {
    let _ = slot();
}

/// The process-wide HTTP client as of NOW. The handle is cheap (an `Arc`) and
/// shares the connection pool; take one per request so a [`reset`] reaches
/// every caller.
pub fn shared() -> Client {
    slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Swap in a brand-new client: empty connection pool, fresh DNS and TLS. In
/// flight requests finish on the old one, which drops with its last handle.
/// Built on its own thread because the blocking client must never be
/// constructed on a tokio worker (see the module docs). The offline banner's
/// Retry calls this; so does a failure streak. The swap lands a moment AFTER
/// this returns — a caller about to issue requests wants [`reset_blocking`].
pub fn reset(reason: &str) {
    let _ = spawn_rebuild(reason);
}

/// [`reset`], returning only once the fresh client is in place, so the very
/// next [`shared`] cannot hand out the old one. The wake watchdog restarts
/// every pipeline right behind this. Blocks for one client build: never call
/// it on the UI thread.
pub fn reset_blocking(reason: &str) {
    if let Some(rebuild) = spawn_rebuild(reason) {
        if rebuild.join().is_err() {
            log::warn!("[http] the client rebuild panicked");
        }
    }
}

fn spawn_rebuild(reason: &str) -> Option<std::thread::JoinHandle<()>> {
    *LAST_REBUILD.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((Instant::now(), jittered(REBUILD_COOLDOWN)));
    FAILURE_STREAK.store(0, Ordering::Relaxed);
    log::info!("[http] rebuilding the shared client ({reason})");
    let spawned = std::thread::Builder::new()
        .name("http-client-rebuild".to_string())
        .spawn(|| {
            let fresh = build();
            *slot().write().unwrap_or_else(|e| e.into_inner()) = fresh;
        });
    match spawned {
        Ok(handle) => Some(handle),
        Err(err) => {
            log::warn!("[http] could not spawn the client rebuild: {err}");
            None
        }
    }
}

/// A request reached the server (any status): the connection works.
pub fn record_success() {
    FAILURE_STREAK.store(0, Ordering::Relaxed);
}

/// A request failed at the CONNECTION level. After
/// [`FAILURES_BEFORE_REBUILD`] in a row with no success between them the
/// pooled connection is presumed dead and the client is rebuilt, at most once
/// per (jittered) [`REBUILD_COOLDOWN`].
pub fn record_failure() {
    let streak = FAILURE_STREAK.fetch_add(1, Ordering::Relaxed).saturating_add(1);
    let last = *LAST_REBUILD.lock().unwrap_or_else(|e| e.into_inner());
    if should_rebuild(streak, last.map(|(at, cooldown)| (at.elapsed(), cooldown))) {
        reset(&format!("{streak} transport failures in a row"));
    }
}

/// [`record_failure`] for a reqwest error, counting only the failures a new
/// connection could cure (see [`is_connection_failure`]).
pub fn record_request_error(err: &reqwest::Error) {
    if is_connection_failure(err.is_connect(), err.is_timeout(), err.is_request() || err.is_body()) {
        record_failure();
    }
}

/// Which transport failures say the CONNECTION is bad: it never came up
/// (DNS/TCP/TLS, a connect timeout included) or it broke mid-request (reset,
/// closed by a keepalive ping). A request that merely ran out its own budget
/// on a working connection does not: a slow server after a deploy or under DB
/// load would otherwise have every desktop drop its pool at once.
pub(crate) fn is_connection_failure(is_connect: bool, is_timeout: bool, is_io: bool) -> bool {
    is_connect || (is_io && !is_timeout)
}

/// `last` = how long ago the client was rebuilt, and that rebuild's cooldown.
fn should_rebuild(streak: u32, last: Option<(Duration, Duration)>) -> bool {
    streak >= FAILURES_BEFORE_REBUILD
        && last.is_none_or(|(elapsed, cooldown)| elapsed >= cooldown)
}

/// An error with its whole `source()` chain, outermost first. reqwest's own
/// `Display` stops at "error sending request for url (…)"; the cause (DNS,
/// connect, TLS, timeout, reset) sits in the sources, and a log line without
/// it cannot tell them apart (FEED-69).
pub fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        // Wrappers often repeat their source verbatim in their own Display.
        if !out.ends_with(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        source = cause.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<Layer>>);

    impl std::fmt::Display for Layer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for Layer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|layer| layer as _)
        }
    }

    #[test]
    fn error_chain_names_every_cause() {
        let err = Layer(
            "error sending request for url (https://app.exponential.at/api/trpc)",
            Some(Box::new(Layer(
                "client error (Connect)",
                Some(Box::new(Layer("dns error: failed to lookup address", None))),
            ))),
        );
        assert_eq!(
            error_chain(&err),
            "error sending request for url (https://app.exponential.at/api/trpc): \
             client error (Connect): dns error: failed to lookup address"
        );
        assert_eq!(error_chain(&Layer("alone", None)), "alone");
        // A wrapper that already ends with its source is not doubled.
        let echo = Layer("send failed: reset", Some(Box::new(Layer("reset", None))));
        assert_eq!(error_chain(&echo), "send failed: reset");
    }

    #[test]
    fn a_failure_streak_rebuilds_once_per_cooldown() {
        assert!(!should_rebuild(FAILURES_BEFORE_REBUILD - 1, None));
        assert!(should_rebuild(FAILURES_BEFORE_REBUILD, None));
        let cooldown = REBUILD_COOLDOWN;
        assert!(!should_rebuild(40, Some((cooldown - Duration::from_secs(1), cooldown))));
        assert!(should_rebuild(40, Some((cooldown, cooldown))));
    }

    #[test]
    fn only_connection_failures_feed_the_streak() {
        // (is_connect, is_timeout, is_io)
        assert!(is_connection_failure(true, false, true), "connect refused / DNS");
        assert!(is_connection_failure(true, true, true), "connect timeout");
        assert!(is_connection_failure(false, false, true), "reset mid-request");
        // The server was slow on a connection that works.
        assert!(!is_connection_failure(false, true, true), "request timeout");
        // Builder / redirect / decode faults say nothing about the socket.
        assert!(!is_connection_failure(false, false, false));
    }

    #[test]
    fn jitter_stays_within_thirty_percent() {
        let base = Duration::from_secs(10);
        assert_eq!(jitter_at(base, 0), Duration::from_secs(7));
        assert_eq!(jitter_at(base, 999), Duration::from_secs(13));
        for _ in 0..200 {
            let rolled = jittered(base);
            assert!(rolled >= Duration::from_secs(7) && rolled <= Duration::from_secs(13), "{rolled:?}");
        }
    }

    #[test]
    fn the_client_builds_with_keepalives() {
        // Guards the async→blocking builder conversion and every option on it.
        let _ = build();
    }
}
