//! HTTP transport + the per-shape long-poll loop (masterplan-v3 §5.3) —
//! blocking `ureq` over rustls, one dedicated `std::thread` per shape.
//! gpui-free; a direct port of the proven iOS `ShapeClient.pollOnce`/`run`.
//!
//! Load-bearing rules baked in here (§5.6):
//!
//! * **No HTTP cache layer at all** (§5.6a). `ureq` has no shared cache by
//!   default — exactly what we want. We never send `If-None-Match` /
//!   `If-Modified-Since`, never key anything by URL, and additionally send
//!   `Cache-Control: no-store` on every request as an explicit belt to the
//!   proxy's `cache-control: private, no-store` suspender. (A URL-keyed cache
//!   is what poisoned macOS `URLCache` with cross-auth empty snapshots.)
//! * **401 → hard Unauthorized, NEVER anonymous-degrade** (§5.6b). A rejected
//!   bearer is terminal for the whole account pipeline: the first thread to
//!   see it flips the shared stop flag (all 15 siblings exit at their next
//!   loop boundary), invokes the `on_unauthorized` callback (the app shell
//!   wires `AuthStore::handle_unauthorized` — it deletes the stored token),
//!   and emits [`ShapeDelta::Unauthorized`] exactly ONCE per account. No
//!   anonymous retry, no polling with the dead token.
//! * **409 / inline `must-refetch` → atomic refetch** (§5.6c). Mark the
//!   cursor (`offset=-1`, replacement handle when Electric sent one) WITHOUT
//!   touching table rows; the next poll re-snapshots and the batch gets a
//!   synthetic [`ShapeMessage::MustRefetch`] head so `apply_batch` runs the
//!   `DELETE FROM {table}` + fresh inserts in ONE commit — a reader never
//!   observes an empty table.
//! * **~90s read timeout on the live long-poll** — it MUST exceed the
//!   server's ~60s hold window, or every live request times out client-side
//!   and the loop degrades into a <1s hammering short-poll (the
//!   `long-poll-canary.md` failure mode). On top of that,
//!   [`MIN_LIVE_REPOLL`] guards the loop against a *misbehaving* server that
//!   answers live polls instantly: an idle live response never re-polls in
//!   under 1s.
//! * **Backoff** (§5.3): 500ms base, exponential, cap 30s, reset on the first
//!   success. Back off only on transport/5xx errors — never on `up-to-date`
//!   (that's the normal steady state).

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::{
    build_url, contains_snapshot_end, contains_up_to_date, parse_messages, request_params,
    strip_inline_must_refetch, RowKey, ShapeMessage, ShapeResponseHeaders, ShapeState,
};
use crate::shapes::ShapeSpec;
use crate::store::{ShapeStore, StoreError};

/// Call-time token access (§5.7): evaluated at every request, never captured
/// once, so a re-login updates every in-flight loop's next request. `None`
/// parks the loop (signed out) — it never degrades to an anonymous request.
pub type TokenFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// The 401 report hook (§5.6b): the app shell passes
/// `AuthStore::unauthorized_handler_fn()` so the dead token is deleted and the
/// UI routes to login. Called with the account id, at most once per account.
pub type UnauthorizedFn = Arc<dyn Fn(&str) + Send + Sync>;

/// The 426 report hook (EXP-104): the app shell passes a closure that flips
/// the app into the blocking "Update required" state. Called at most once per
/// account pipeline. NO account id — the min-version gate is a property of the
/// whole binary, not one account (contrast [`UnauthorizedFn`]).
pub type UpgradeRequiredFn = Arc<dyn Fn() + Send + Sync>;

/// Read timeout for the blocking socket — MUST exceed the server's ~60s
/// long-poll hold window (§5.3; the `long-poll-canary.md` contract).
pub const LIVE_READ_TIMEOUT: Duration = Duration::from_secs(90);
/// TCP/TLS connect timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Error backoff base / cap (§5.3).
pub const BACKOFF_BASE: Duration = Duration::from_millis(500);
pub const BACKOFF_CAP: Duration = Duration::from_secs(30);
/// Pause after a 409/pending-refetch or a non-live no-progress poll (iOS
/// `pollOnce` "shouldPause" parity).
pub const REFETCH_PAUSE: Duration = Duration::from_millis(500);
/// Park interval while signed out (no token yet).
pub const SIGNED_OUT_PARK: Duration = Duration::from_secs(2);
/// The <1s repeat guard (Phase-2 gate): an idle live poll (no row ops) never
/// re-polls in under this interval, even against a server that answers live
/// requests instantly instead of holding them.
pub const MIN_LIVE_REPOLL: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------------------
// Transport (trait-injected for tests, ureq in production)
// ---------------------------------------------------------------------------

/// A raw shape-proxy response. Non-2xx statuses come back as `Ok` responses —
/// only DNS/TCP/TLS/timeout failures are [`TransportError`]s — so the
/// 401/409 state machine lives in ONE place ([`ShapeClient::poll_once`]).
#[derive(Debug, Clone)]
pub struct TransportResponse {
    pub status: u16,
    /// Raw `(name, value)` pairs; names matched case-insensitively downstream.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// DNS / TCP / TLS / timeout — transient; the loop retries with backoff.
#[derive(Debug, Clone)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transport: {}", self.0)
    }
}

impl std::error::Error for TransportError {}

/// The blocking HTTP seam (§5.3 testing guidance): production is
/// [`UreqTransport`]; tests inject an in-process server or a scripted impl.
///
/// Contract for implementors: **no caching of any kind** (§5.6a) — every
/// `fetch` must hit the network; and the read timeout must exceed the ~60s
/// live hold window (§5.3).
pub trait ShapeTransport: Send + Sync {
    fn fetch(&self, url: &str, bearer: &str) -> Result<TransportResponse, TransportError>;
}

/// Production transport: blocking `ureq` over rustls (§5.3 crate choice — no
/// async runtime under gpui's executor). One `Agent` (connection pool) shared
/// by all shape threads of a manager; **no cache middleware, ever**.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            // ~90s read: must exceed the server's ~60s long-poll hold (§5.3).
            .timeout_read(LIVE_READ_TIMEOUT)
            .build();
        Self { agent }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapeTransport for UreqTransport {
    fn fetch(&self, url: &str, bearer: &str) -> Result<TransportResponse, TransportError> {
        let result = self
            .agent
            .get(url)
            .set("Authorization", &format!("Bearer {bearer}"))
            .set("Accept", "application/json")
            // Explicit no-cache discipline (§5.6a) — belt to the proxy's
            // `private, no-store` suspender. Never If-None-Match/-Modified.
            .set("Cache-Control", "no-store")
            // EXP-104: the client-version header so a stale build's shape polls
            // are 426-gated just like tRPC.
            .set(
                domain::client_version::CLIENT_VERSION_HEADER,
                &domain::client_version::client_version_header_value(),
            )
            .call();
        let response = match result {
            Ok(response) => response,
            // Non-2xx is a *response* (the 401/409 machine handles it), not a
            // transport failure.
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(t)) => return Err(TransportError(t.to_string())),
        };
        let status = response.status();
        let headers = response
            .headers_names()
            .into_iter()
            .map(|name| {
                let value = response.header(&name).unwrap_or_default().to_string();
                (name, value)
            })
            .collect();
        let mut body = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|e| TransportError(format!("body read: {e}")))?;
        Ok(TransportResponse {
            status,
            headers,
            body,
        })
    }
}

// ---------------------------------------------------------------------------
// Outward deltas (the gpui-free boundary — §5.8)
// ---------------------------------------------------------------------------

/// What a shape thread emits over the `flume::Sender<ShapeDelta>` after each
/// batch commits (§5.8). The collections layer (the only gpui seam) drains
/// these on a foreground task and re-hydrates from the read-only SQLite
/// connection — the channel carries change *notifications*, not row data.
#[derive(Debug, Clone)]
pub enum ShapeDelta {
    /// A batch committed to SQLite for `shape`.
    Applied {
        account_id: String,
        /// Shape/table name (snake_case, == [`ShapeSpec::name`]).
        shape: &'static str,
        /// The row keys this batch upserted/deleted. Empty for a pure
        /// `up-to-date` heartbeat.
        keys: Vec<RowKey>,
        /// The batch atomically replaced the WHOLE table (§5.6c refetch) —
        /// re-hydrate the table wholesale, point reads are not enough.
        full_replace: bool,
        /// The batch carried `up-to-date` — the shape is at head (feeds
        /// `wait_for_first_sync`, §5.10).
        up_to_date: bool,
    },
    /// The session token was rejected (hard 401). Emitted at
    /// most ONCE per account; the pipeline is already tearing itself down.
    /// The UI must route this account to the login screen — never render an
    /// empty board, never retry anonymously.
    Unauthorized { account_id: String },
}

// ---------------------------------------------------------------------------
// Poll errors / outcome
// ---------------------------------------------------------------------------

/// One poll's failure modes (iOS `ShapeError` parity).
#[derive(Debug)]
pub enum ShapeError {
    /// Hard 401 — terminal for the account pipeline (§5.6b).
    Unauthorized,
    /// HTTP 426 — the min-version gate (EXP-104). Terminal like the 401 path
    /// (stop polling), but the token is left intact.
    UpgradeRequired,
    /// Any other non-2xx status — transient, retried with backoff.
    Http(u16),
    Transport(TransportError),
    Store(StoreError),
}

impl std::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeError::Unauthorized => write!(f, "unauthorized (session token rejected)"),
            ShapeError::UpgradeRequired => write!(f, "client upgrade required (426)"),
            ShapeError::Http(status) => write!(f, "http {status}"),
            ShapeError::Transport(e) => write!(f, "{e}"),
            ShapeError::Store(e) => write!(f, "store: {e}"),
        }
    }
}

impl std::error::Error for ShapeError {}

/// What one successful poll tells the loop about pacing.
struct PollOutcome {
    /// Pause [`REFETCH_PAUSE`] before the next poll: a refetch is pending
    /// (409 / inline must-refetch) or a non-live poll made no progress
    /// (iOS `pollOnce` return-bool parity).
    pause: bool,
    /// This was a live long-poll that carried no row ops — subject to the
    /// [`MIN_LIVE_REPOLL`] <1s repeat guard.
    idle_live: bool,
}

// ---------------------------------------------------------------------------
// ShapeClient
// ---------------------------------------------------------------------------

/// Everything one shape thread needs. Built by the `SyncManager` (one per
/// shape per account); `unauthorized_reported` is shared by all 15 threads of
/// an account so the 401 signal fires exactly once.
pub struct ShapeClientConfig {
    pub account_id: String,
    /// Normalized instance base URL (`https://app.exponential.at`).
    pub base_url: String,
    pub spec: &'static ShapeSpec,
    pub store: Arc<ShapeStore>,
    pub token: TokenFn,
    pub transport: Arc<dyn ShapeTransport>,
    pub deltas: flume::Sender<ShapeDelta>,
    /// Per-ACCOUNT dedupe flag for the 401 signal (§5.6b: 15 threads may all
    /// 401 at the same instant; exactly one reports).
    pub unauthorized_reported: Arc<AtomicBool>,
    /// The app-shell hook that deletes the stored token + routes to login
    /// (`AuthStore::handle_unauthorized`). Optional so headless tests can
    /// observe the delta alone.
    pub on_unauthorized: Option<UnauthorizedFn>,
    /// Per-ACCOUNT dedupe flag for the 426 signal (EXP-104), mirroring
    /// `unauthorized_reported`: the shape threads may all 426 at once; exactly
    /// one reports.
    pub upgrade_required_reported: Arc<AtomicBool>,
    /// The app-shell hook that flips the app into the blocking "Update
    /// required" state (EXP-104). Optional so headless tests can assert the
    /// loop teardown alone.
    pub on_upgrade_required: Option<UpgradeRequiredFn>,
}

/// One shape's blocking long-poll engine. [`ShapeClient::run`] is the thread
/// body; everything else is the §5.3 poll state machine.
pub struct ShapeClient {
    cfg: ShapeClientConfig,
}

impl ShapeClient {
    pub fn new(cfg: ShapeClientConfig) -> Self {
        Self { cfg }
    }

    /// The §5.3 poll loop — runs until `stop` flips or a hard 401 tears the
    /// account pipeline down. Cooperative cancellation: the flag is checked
    /// before every request and between every sleep slice; an in-flight live
    /// read can linger up to [`LIVE_READ_TIMEOUT`], but its result is
    /// discarded (checked again before apply) and the thread exits at the
    /// next boundary.
    pub fn run(&self, stop: &Arc<AtomicBool>) {
        let mut backoff = BACKOFF_BASE;
        // Transient `electric-cursor` echo (§5.2) — per-loop memory, never
        // persisted.
        let mut cursor: Option<String> = None;
        while !stop.load(Ordering::Relaxed) {
            let Some(token) = (self.cfg.token)() else {
                // Signed out / token not yet resolved: park, NEVER go
                // anonymous (§5.6b; requireAuth shapes would hard-401).
                sleep_with_stop(stop, SIGNED_OUT_PARK);
                continue;
            };
            let started = Instant::now();
            match self.poll_once(&token, &mut cursor, stop) {
                Ok(outcome) => {
                    backoff = BACKOFF_BASE; // reset on success (§5.3)
                    if outcome.pause {
                        sleep_with_stop(stop, REFETCH_PAUSE);
                    } else if outcome.idle_live {
                        // <1s repeat guard: a live poll that came back idle
                        // in under MIN_LIVE_REPOLL waits out the remainder —
                        // the loop can never degrade into a tight spin even
                        // against a server that answers instantly.
                        let elapsed = started.elapsed();
                        if elapsed < MIN_LIVE_REPOLL {
                            sleep_with_stop(stop, MIN_LIVE_REPOLL - elapsed);
                        }
                    }
                }
                Err(ShapeError::Unauthorized) => {
                    // §5.6b: terminal for the whole account pipeline.
                    self.report_unauthorized(stop);
                    return;
                }
                Err(ShapeError::UpgradeRequired) => {
                    // EXP-104: terminal like the 401 path — stop polling — but
                    // the token is left intact (the session is fine; the build
                    // is stale).
                    self.report_upgrade_required(stop);
                    return;
                }
                Err(err) => {
                    log::warn!(
                        "[sync {}::{}] {err}",
                        self.cfg.account_id,
                        self.cfg.spec.name
                    );
                    sleep_with_stop(stop, backoff);
                    backoff = (backoff * 2).min(BACKOFF_CAP); // cap 30s (§5.3)
                }
            }
        }
    }

    /// Direct port of iOS `ShapeClient.pollOnce` (§5.3/§5.6). One request,
    /// one applied batch, one cursor persist — all driven by the *persisted*
    /// [`ShapeState`] so a quit between a 409 and its refetch still resumes
    /// into the atomic replacement.
    fn poll_once(
        &self,
        token: &str,
        cursor: &mut Option<String>,
        stop: &Arc<AtomicBool>,
    ) -> Result<PollOutcome, ShapeError> {
        let spec = self.cfg.spec;
        let saved = self
            .cfg
            .store
            .shape_state(spec.name)
            .map_err(ShapeError::Store)?;
        let refetching = saved.as_ref().is_some_and(|s| s.needs_refetch);
        let was_live = saved.as_ref().is_some_and(|s| s.is_live) && !refetching;

        // The client sends ONLY offset/handle/live/cursor — NEVER `where` or
        // `columns` (§5.2, load-bearing: sorted-where identity + the
        // issue_subscribers email exclusion are inherited from the proxy).
        let params = request_params(saved.as_ref(), cursor.as_deref());
        let url = build_url(&self.cfg.base_url, spec.path, &params);
        // Dev-only request trace (§5.11 runtime gates 3/6: cursor resume +
        // long-poll cadence are verified off this line). Env-gated, never on
        // by default; the URL carries no secrets (the bearer is a header).
        if std::env::var_os("EXP_SYNC_LOG").is_some() {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            eprintln!("[sync-log {ts}] {} GET {url}", spec.name);
        }
        let response = self
            .cfg
            .transport
            .fetch(&url, token)
            .map_err(ShapeError::Transport)?;

        // Stopped while the read was in flight (sign-out/quit): discard the
        // result without touching the store.
        if stop.load(Ordering::Relaxed) {
            return Ok(PollOutcome {
                pause: false,
                idle_live: false,
            });
        }

        let headers = ShapeResponseHeaders::from_pairs(
            response
                .headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        );

        if response.status == 401 {
            return Err(ShapeError::Unauthorized);
        }
        if response.status == 426 {
            // EXP-104 min-version gate — before the generic non-2xx path so a
            // stale build tears down instead of backing off forever.
            return Err(ShapeError::UpgradeRequired);
        }
        if response.status == 409 {
            // The shape rotated (§5.6c step 1): persist the refetch marker
            // with Electric's replacement handle — do NOT delete table rows;
            // stale rows stay visible until the refetch replaces them
            // atomically.
            self.cfg
                .store
                .mark_needs_refetch(spec.name, headers.handle.as_deref())
                .map_err(ShapeError::Store)?;
            *cursor = None;
            return Ok(PollOutcome {
                pause: true,
                idle_live: false,
            });
        }
        if !(200..300).contains(&response.status) {
            return Err(ShapeError::Http(response.status));
        }

        let msgs = parse_messages(&response.body, spec.composite_keys());
        let (mut msgs, had_inline_refetch) = strip_inline_must_refetch(msgs);

        if had_inline_refetch {
            // Inline must-refetch (§5.6c): apply whatever rode alongside,
            // then mark the refetch (no replacement handle — the inline case
            // carries none). The dead handle/offset from this response is
            // never persisted.
            if !msgs.is_empty() {
                let keys = row_keys(&msgs);
                self.cfg
                    .store
                    .apply_batch(spec, &msgs, None)
                    .map_err(ShapeError::Store)?;
                self.emit_applied(keys, false, false);
            }
            self.cfg
                .store
                .mark_needs_refetch(spec.name, None)
                .map_err(ShapeError::Store)?;
            *cursor = None;
            return Ok(PollOutcome {
                pause: true,
                idle_live: false,
            });
        }

        // Only up-to-date flips the shape live (§5.2 — snapshot-end never
        // does; the parser already dropped it).
        let saw_up_to_date = contains_up_to_date(&msgs);

        // Post-409/inline refetch: THIS response is the fresh snapshot.
        // Prepend the synthetic MustRefetch head so apply_batch runs
        // DELETE + fresh INSERTs inside one commit (§5.6c step 2) — a reader
        // never observes an empty table.
        if refetching {
            // §5.6c hardening: a refetch response that decoded to ZERO
            // messages must NOT blindly run the DELETE head or adopt the
            // response cursor: that would persist an empty table AND clear
            // the refetch marker — the "all issues vanished"
            // symptom, made durable. The ONE legitimate zero-message form is
            // a genuinely empty snapshot, which live Electric (1.6.9) sends
            // as a LONE `snapshot-end` control (no rows, no `up-to-date` —
            // that only arrives on the follow-up poll). Only that form may
            // complete the swap; anything else (empty/malformed body) keeps
            // the marker and stale rows and retries after the pause.
            if msgs.is_empty() && !contains_snapshot_end(&response.body) {
                return Ok(PollOutcome {
                    pause: true,
                    idle_live: false,
                });
            }
            msgs.insert(0, ShapeMessage::MustRefetch);
        }

        let keys = row_keys(&msgs);
        let next_state = ShapeState::after_apply(&headers, saved.as_ref(), saw_up_to_date);
        if !msgs.is_empty() || next_state.is_some() {
            // Rows + cursor in ONE transaction (§5.4); when the response
            // carried no handle/offset the rows still apply and the cursor
            // stays put (at-least-once + idempotent upserts).
            self.cfg
                .store
                .apply_batch(spec, &msgs, next_state.as_ref())
                .map_err(ShapeError::Store)?;
        }
        let has_row_ops = !keys.is_empty() || refetching;
        if has_row_ops || saw_up_to_date {
            self.emit_applied(keys, refetching, saw_up_to_date);
        }

        // Transient cursor echo for the next live poll (§5.2).
        *cursor = headers.cursor.clone();

        Ok(PollOutcome {
            // iOS parity: pace when a NON-live poll made no progress, so a
            // response that never reaches up-to-date can't spin-request.
            pause: !was_live && !saw_up_to_date && msgs.is_empty(),
            idle_live: was_live && !has_row_ops,
        })
    }

    fn emit_applied(&self, keys: Vec<RowKey>, full_replace: bool, up_to_date: bool) {
        let _ = self.cfg.deltas.send(ShapeDelta::Applied {
            account_id: self.cfg.account_id.clone(),
            shape: self.cfg.spec.name,
            keys,
            full_replace,
            up_to_date,
        });
    }

    /// §5.6b, once per ACCOUNT: flip the shared stop flag (all 15 sibling
    /// threads exit at their next loop boundary — none keeps polling with the
    /// dead token), run the app-shell hook (deletes the stored token, routes
    /// to login), emit [`ShapeDelta::Unauthorized`].
    fn report_unauthorized(&self, stop: &Arc<AtomicBool>) {
        stop.store(true, Ordering::Relaxed);
        if !self.cfg.unauthorized_reported.swap(true, Ordering::SeqCst) {
            log::warn!(
                "[sync {}::{}] hard 401 — tearing down the account pipeline",
                self.cfg.account_id,
                self.cfg.spec.name
            );
            if let Some(hook) = &self.cfg.on_unauthorized {
                hook(&self.cfg.account_id);
            }
            let _ = self.cfg.deltas.send(ShapeDelta::Unauthorized {
                account_id: self.cfg.account_id.clone(),
            });
        }
    }

    /// EXP-104, once per ACCOUNT: flip the shared stop flag (all sibling
    /// threads exit at their next loop boundary — none keeps polling a build
    /// the server refuses) and run the app-shell hook (flips the app into the
    /// blocking "Update required" state). Mirrors [`Self::report_unauthorized`]
    /// but emits NO delta and NEVER touches the stored token — the session is
    /// valid; only the binary is stale.
    fn report_upgrade_required(&self, stop: &Arc<AtomicBool>) {
        stop.store(true, Ordering::Relaxed);
        if !self
            .cfg
            .upgrade_required_reported
            .swap(true, Ordering::SeqCst)
        {
            log::warn!(
                "[sync {}::{}] HTTP 426 — client too old; stopping polling and gating the app",
                self.cfg.account_id,
                self.cfg.spec.name
            );
            if let Some(hook) = &self.cfg.on_upgrade_required {
                hook();
            }
        }
    }
}

/// The row keys a batch touches (upserts + deletes; controls carry none).
fn row_keys(msgs: &[ShapeMessage]) -> Vec<RowKey> {
    msgs.iter()
        .filter_map(|m| match m {
            ShapeMessage::Insert { key, .. }
            | ShapeMessage::Update { key, .. }
            | ShapeMessage::Delete { key } => Some(key.clone()),
            ShapeMessage::UpToDate | ShapeMessage::MustRefetch => None,
        })
        .collect()
}

/// Sleep `total` in short slices, returning early the moment `stop` flips —
/// keeps sign-out/quit teardown inside ~100ms even mid-backoff (§5.3).
pub(crate) fn sleep_with_stop(stop: &Arc<AtomicBool>, total: Duration) {
    const SLICE: Duration = Duration::from_millis(50);
    let deadline = Instant::now() + total;
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        std::thread::sleep(SLICE.min(deadline - now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_with_stop_returns_early_on_stop() {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let started = Instant::now();
            sleep_with_stop(&stop2, Duration::from_secs(10));
            started.elapsed()
        });
        std::thread::sleep(Duration::from_millis(120));
        stop.store(true, Ordering::Relaxed);
        let elapsed = handle.join().unwrap();
        assert!(elapsed < Duration::from_secs(1), "returned in {elapsed:?}");
    }

    #[test]
    fn read_timeout_exceeds_the_server_hold_window() {
        // §5.3: 90s read > ~60s server hold — below it, every live request
        // times out client-side and the loop degrades into a hammering
        // short-poll (the long-poll-canary failure).
        assert!(LIVE_READ_TIMEOUT >= Duration::from_secs(75));
        assert!(MIN_LIVE_REPOLL >= Duration::from_secs(1));
    }
}
