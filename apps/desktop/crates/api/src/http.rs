//! The ONE HTTP client the desktop app uses (EXP-304).
//!
//! Every outbound request — Electric shape long-polls, tRPC, Better Auth,
//! attachment uploads, the self-updater's release download — goes through the
//! single [`reqwest::blocking::Client`] handed out by [`shared`]. That is not
//! tidiness for its own sake:
//!
//! * **One connection instead of fifteen.** The old `ureq` agent was HTTP/1.1
//!   only, so each of the 15 shape threads dialled its own socket. Every launch
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

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::blocking::Client;

/// Connect budget for every request. Generous enough for a cold radio /
/// VPN-establishing path, short enough that a black-holed address fails into
/// the caller's retry instead of hanging.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Whole-request budget for ordinary calls (tRPC, auth, uploads) — iOS
/// URLSession parity. Long-polls override this per request; see
/// `sync::client::LIVE_READ_TIMEOUT`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Idle connections kept per host. One per shape (15) plus tRPC headroom, so a
/// poll cycle reuses connections instead of re-dialling — HTTP/2 collapses
/// these into one anyway, but the HTTP/1.1 fallback (plain-HTTP local dev)
/// needs the room.
const POOL_MAX_IDLE_PER_HOST: usize = 32;

static CLIENT: OnceLock<Client> = OnceLock::new();

fn build() -> Client {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DEFAULT_TIMEOUT)
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
        // All 15 shape long-polls ride ONE connection once HTTP/2 is
        // negotiated, so a connection the network killed silently would stall
        // every one of them until its own 90s budget expired. TCP keepalive
        // probes surface that in ~30s — and, unlike HTTP/2 pings (which
        // reqwest's blocking builder does not expose), they work on the
        // HTTP/1.1 fallback too. They also keep NAT/firewall state alive
        // across a long idle hold.
        .tcp_keepalive(Duration::from_secs(30))
        // No `http2_prior_knowledge`: ALPN negotiates, so a plain-HTTP local
        // backend cleanly falls back to HTTP/1.1 (still pooled).
        .build()
        .expect("failed to build the shared HTTP client")
}

/// Build the shared client eagerly. Call once from `main`, on the foreground
/// thread, before anything can reach [`shared`] from a tokio context.
pub fn init() {
    let _ = CLIENT.set(build());
}

/// The process-wide HTTP client. Cloning a `Client` is cheap and shares the
/// same connection pool — clone freely rather than building your own.
pub fn shared() -> &'static Client {
    CLIENT.get_or_init(build)
}
