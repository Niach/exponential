//! The per-coding-session publisher (masterplan-v3 §8.4–§8.7): tee the §6
//! read-loop's raw PTY bytes out as `0x01` frames, inject remote `input`
//! into the shared PTY writer, resize both ways, answer `resync` from the
//! 256 KiB ring, honor claim/kill, and auto-reconnect resuming the room.
//!
//! **Best-effort and non-blocking** (§8.4): if the relay is disabled or
//! unreachable the coding session runs fine locally — the publisher never
//! gates the terminal. The hot-path rule is absolute: a slow socket must
//! NEVER stall the read loop, so the tee is a bounded `try_send` that DROPS
//! output on overflow ([`IN_FLIGHT_CAP`]) while control frames ride a
//! separate unbounded channel that is never dropped or reordered.
//!
//! Claim model (§8.5): the LOCAL user is never gated — their keystrokes go
//! straight to the PTY. "Take over" simply sends `claim`; the relay's
//! publisher-branch (`hub.ts`: `if (room.publisher === conn)
//! publisherTakeover(room)`) force-clears the remote steerer and
//! re-broadcasts presence.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use api::error::ApiError;
use api::steer::MintedTicket;
use api::trpc::TrpcClient;
use futures_util::{SinkExt, StreamExt};
use terminal::RawSink;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use crate::frames::{
    output_frame, ActivityEvent, ClientFrame, PresenceViewer, ServerFrame, CLOSE_REPLACED,
    CLOSE_SESSION_ENDED, CLOSE_UNAUTHORIZED,
};
use crate::ring::RingBuffer;
use crate::{dial, Backoff, DialError, SteerRuntime, WsStream, BACKOFF_RESET_AFTER};

/// §8.4: the tee channel's bounded capacity — on overflow, output chunks are
/// dropped (a laggy relay loses scrollback frames, not correctness). Mirrors
/// the relay's own viewer-side slow-consumer guard.
pub const IN_FLIGHT_CAP: usize = 32;

/// The relay's WebSocket `maxPayloadLength` (bytes). A text frame at or past
/// this makes the relay sever the connection — killing the members' PTY
/// mirror along with the activity stream — so oversize activity frames are
/// dropped client-side instead of sent.
const RELAY_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Clamp a grid dimension to the relay's zod bounds (`helloFrame`/`resizeFrame`
/// require `positive().max(1000)` in `protocol.ts`). Sending cols/rows outside
/// `1..=1000` makes `parseClientFrame` return `null` and the relay SILENTLY
/// drops the frame — the exact silent-hang failure (§8.1): no room is ever
/// created (hello) or no reflow happens (resize). A very wide/tall grid
/// (hi-dpi + tiny font) is the realistic trigger; clamping degrades to a
/// slightly-cropped viewer view instead of a dead room.
fn clamp_dim(value: u16) -> u16 {
    value.clamp(1, 1000)
}

/// §8.7: surfaced after two consecutive fresh-ticket 401s (never silently
/// retry a skewed clock — the native failure mode this fixes).
const CLOCK_SKEW_ERROR: &str = "Steer relay rejected the connection (ticket expired on \
     arrival) — check that this machine's clock is in sync (NTP).";

// ---------------------------------------------------------------------------
// The tee sink (terminal → publisher, §6.14 seam)
// ---------------------------------------------------------------------------

/// The [`RawSink`] the §6 read loop fans raw chunks into. `on_output` runs on
/// the read thread while the sink registry lock is held — it MUST stay cheap
/// and non-blocking, hence `try_send` + drop-on-overflow (output only).
pub struct PublisherSink {
    tx: flume::Sender<Vec<u8>>,
    dropped: AtomicU64,
}

impl PublisherSink {
    fn bounded() -> (Arc<Self>, flume::Receiver<Vec<u8>>) {
        let (tx, rx) = flume::bounded(IN_FLIGHT_CAP);
        (
            Arc::new(Self {
                tx,
                dropped: AtomicU64::new(0),
            }),
            rx,
        )
    }

    /// Output chunks dropped on overflow so far (observability/test surface).
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl RawSink for PublisherSink {
    fn on_output(&self, chunk: &[u8]) {
        // NEVER block the terminal on the relay (§8.4).
        if self.tx.try_send(chunk.to_vec()).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Spec, hooks, tickets
// ---------------------------------------------------------------------------

/// What to publish (§8.4 handshake): the `coding_sessions` row id keys the
/// relay room; the issue id rides along for the phone's session list.
#[derive(Clone, Debug)]
pub struct PublishSpec {
    pub session_id: String,
    pub issue_id: Option<String>,
    /// EXP-32: whether the room's scrubbed activity stream may fan out to
    /// ANONYMOUS public viewers (feedback board with `publicShowCoding='live'`
    /// and not opted private). `true` serializes as ABSENT on the wire (the
    /// legacy hello shape — old relays behave as before); only `false` is
    /// sent explicitly. Authenticated activity-channel members receive the
    /// stream either way. NOTE: this flag alone is NOT the keep-private
    /// enforcement — a pre-EXP-32 relay strips the unknown key and fans
    /// activity to anonymous viewers regardless, so the wiring layer
    /// additionally never spawns the activity emitter for an explicitly
    /// keep-private session (fail closed under relay/desktop deploy skew).
    pub activity_public: bool,
}

/// Publisher-ticket source, injectable for tests. Blocking (ureq) — the loop
/// wraps calls in `spawn_blocking`. `Ok(None)` = instance reports disabled ⇒
/// skip publishing entirely (§8.4 #1).
pub trait PublisherTickets: Send + Sync + 'static {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError>;
}

/// Production tickets over the account's tRPC client:
/// `steer.mintTicket({kind:"publisher", codingSessionId})`.
pub struct TrpcPublisherTickets {
    pub trpc: Arc<TrpcClient>,
    pub coding_session_id: String,
}

impl PublisherTickets for TrpcPublisherTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        Ok(api::steer::mint_publisher_ticket(&self.trpc, &self.coding_session_id)?.into_ticket())
    }
}

/// A `presence` broadcast (§8.5): drives the "Remote steering — {name}"
/// banner. `steerer_id` is a userId; resolve the name via the matching
/// [`PresenceViewer`].
#[derive(Clone, Debug, PartialEq)]
pub struct Presence {
    pub viewers: Vec<PresenceViewer>,
    pub steerer_id: Option<String>,
}

/// Why the publisher asked the coding flow to tear the session down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillSignal {
    /// A relay `kill` frame (steer.killSession fan-out / a steerer's kill).
    RemoteKill,
    /// The relay closed the room with `CLOSE_SESSION_ENDED` (4001).
    SessionEnded,
}

/// Remote `input` bytes → the shared PTY writer (§6.5). Aliased so the
/// `&[u8]`-taking `Fn` type stays under clippy's `type_complexity` bar.
pub type InputHook = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// The seam back into the app (§8.9). Every hook is invoked on the steer
/// runtime — implementations marshal to the gpui foreground themselves where
/// needed. Cheap-and-non-blocking applies to all of them.
pub struct PublisherHooks {
    /// Remote `input` frames → the ONE shared PTY writer (§6.5). Build with
    /// [`pty_writer_input_hook`] over `Terminal::writer()`.
    pub write_input: InputHook,
    /// Steerer-origin resize (§8.4): apply via §6 `terminal.resize` — which
    /// already no-ops on an unchanged size, killing the resize ping-pong.
    pub resize: Arc<dyn Fn(u16, u16) + Send + Sync>,
    /// TRUE current geometry for `hello`/re-`hello` (§8.4 #2 — never a
    /// hardcoded 80×24). Build with [`term_geometry_hook`].
    pub geometry: Arc<dyn Fn() -> (u16, u16) + Send + Sync>,
    /// Relay-initiated teardown: kill the `claude` child; the exit hook then
    /// ends the `coding_sessions` row (idempotent server-side).
    pub kill: Arc<dyn Fn(KillSignal) + Send + Sync>,
    /// Presence updates → the §8.5 banner state.
    pub presence: Arc<dyn Fn(Presence) + Send + Sync>,
    /// Terminal-state errors worth surfacing (clock skew, repeated rejects).
    pub error: Arc<dyn Fn(String) + Send + Sync>,
}

/// Keystroke frames (`\r` submit, `\x1b` interrupt / CSI sequences, lone
/// control bytes) must land raw; anything else is message TEXT from a
/// steerer's composer and gets local-paste treatment (bracketed, EXP-72).
fn is_keystroke(bytes: &[u8]) -> bool {
    bytes.first() == Some(&0x1b) || (bytes.len() == 1 && (bytes[0] < 0x20 || bytes[0] == 0x7f))
}

/// [`PublisherHooks::write_input`] over the shared PTY writer
/// (`Terminal::writer()`): remote keystrokes land exactly like local typing,
/// and remote message TEXT lands exactly like a local PASTE — bracketed when
/// the child turned mode 2004 on (EXP-72: an unbracketed text+`\r` burst
/// trips the `claude` TUI's paste heuristic, which inserts a newline instead
/// of submitting).
pub fn pty_writer_input_hook(
    writer: Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>>,
    term: terminal::TermHandle,
) -> InputHook {
    Arc::new(move |bytes| {
        let bracket = !is_keystroke(bytes) && terminal::bracketed_paste_enabled(&term);
        if let Ok(mut w) = writer.lock() {
            if bracket {
                let _ = w.write_all(b"\x1b[200~");
                let _ = w.write_all(bytes);
                let _ = w.write_all(b"\x1b[201~");
            } else {
                let _ = w.write_all(bytes);
            }
            let _ = w.flush();
        }
    })
}

/// [`PublisherHooks::geometry`] over the live grid (`Terminal::term()`).
pub fn term_geometry_hook(
    term: terminal::TermHandle,
) -> Arc<dyn Fn() -> (u16, u16) + Send + Sync> {
    Arc::new(move || terminal::grid_size(&term))
}

// ---------------------------------------------------------------------------
// Handle + commands
// ---------------------------------------------------------------------------

/// Control-path commands (§8.4 backpressure rule: these ride an UNBOUNDED
/// channel — never dropped, never reordered against each other).
#[derive(Clone, Debug, PartialEq)]
enum PublisherCmd {
    /// Local window resized the grid — forward so viewers reflow.
    LocalResize { cols: u16, rows: u16 },
    /// §8.5 "Take over": publisher-sent `claim` force-clears the remote
    /// steerer (relay `publisherTakeover`).
    TakeOver,
    /// §P7: one PUBLIC activity event (already redacted) → `activity` text
    /// frame. Rides the unbounded control channel like the others; low-rate
    /// (per assistant turn / debounced diff), so it never backs up.
    Activity(ActivityEvent),
    /// Clean end: send `bye {outcome}` and close (child exit, local stop,
    /// kill-watch).
    Shutdown { outcome: Option<String> },
}

/// The coding-flow's handle onto a running publisher task.
pub struct PublisherHandle {
    sink: Arc<PublisherSink>,
    cmd_tx: flume::Sender<PublisherCmd>,
    running: Arc<AtomicBool>,
}

/// A cheap `Send + Sync` clone of the publisher's control sender, dedicated to
/// §8.4 resize-up. The terminal session's [`terminal::ResizeObserver`] holds
/// one and calls [`LocalResizeNotifier::notify`] on every genuine local grid
/// change; the publisher task dedups against its last-sent geometry so a
/// steerer-origin resize can't ping-pong. Cloning the handle itself would drag
/// in the sink + running flag, so this exposes only the resize path.
#[derive(Clone)]
pub struct LocalResizeNotifier {
    cmd_tx: flume::Sender<PublisherCmd>,
}

impl LocalResizeNotifier {
    /// Forward a genuine local grid change so remote viewers reflow (§8.4).
    pub fn notify(&self, cols: u16, rows: u16) {
        let _ = self.cmd_tx.send(PublisherCmd::LocalResize { cols, rows });
    }
}

/// A cheap `Send + Sync` clone of the publisher's control sender, dedicated to
/// §P7 public activity events. The activity emitter thread holds one and pushes
/// already-redacted narration/tool/diff events; sending is fire-and-forget and
/// never blocks the emitter (unbounded flume). Sending after the publisher has
/// stopped is a harmless no-op (the pump drops the receiver).
#[derive(Clone)]
pub struct ActivitySender {
    cmd_tx: flume::Sender<PublisherCmd>,
}

impl ActivitySender {
    /// Publish one already-redacted activity event (best-effort).
    pub fn send(&self, event: ActivityEvent) {
        let _ = self.cmd_tx.send(PublisherCmd::Activity(event));
    }
}

impl PublisherHandle {
    /// The tee sink to attach via `Terminal::attach_sink` (and detach on
    /// teardown). The publisher never re-reads the PTY (§8.4).
    pub fn raw_sink(&self) -> Arc<dyn RawSink> {
        self.sink.clone()
    }

    /// Output chunks dropped by the backpressure policy so far.
    pub fn dropped_chunks(&self) -> u64 {
        self.sink.dropped()
    }

    /// Local grid changed (§6.10 step 3): send `resize` up so viewers
    /// reflow. Call only on a genuine integer cell change (the §6 resize
    /// path already debounces); the task additionally skips no-op repeats.
    pub fn notify_local_resize(&self, cols: u16, rows: u16) {
        let _ = self.cmd_tx.send(PublisherCmd::LocalResize { cols, rows });
    }

    /// A cheap [`LocalResizeNotifier`] for the terminal session's resize
    /// observer (§8.4 resize-up) — routes local geometry changes here without
    /// coupling `crates/terminal` to the whole handle.
    pub fn resize_notifier(&self) -> LocalResizeNotifier {
        LocalResizeNotifier {
            cmd_tx: self.cmd_tx.clone(),
        }
    }

    /// The §8.5 "Take over" button: revoke the remote steerer.
    pub fn take_over(&self) {
        let _ = self.cmd_tx.send(PublisherCmd::TakeOver);
    }

    /// A cheap [`ActivitySender`] for the §P7 activity emitter thread — pushes
    /// public activity events onto the same unbounded control channel without
    /// coupling the emitter to the whole handle.
    pub fn activity_sender(&self) -> ActivitySender {
        ActivitySender {
            cmd_tx: self.cmd_tx.clone(),
        }
    }

    /// Clean end (§8.4 End): `bye {outcome}` + close(1000). Outcome format
    /// per spec: `exit:<code>` from the child-exit hook, `killed` for a
    /// kill-path teardown. Idempotent; stops any reconnect loop.
    pub fn shutdown(&self, outcome: Option<String>) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_tx.send(PublisherCmd::Shutdown { outcome });
    }

    /// §8.8 kill-switch entry: the synced `coding_sessions` row flipped to
    /// `ended` (or the local session was torn down out-of-band). Equivalent
    /// to `shutdown(Some("killed"))`.
    pub fn session_ended(&self) {
        self.shutdown(Some("killed".to_string()));
    }

    /// False once the publisher stopped for good (clean end, 4001/4002,
    /// skew give-up).
    pub fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Start publishing a coding session (§8.4). Non-blocking: spawns the task
/// onto the steer runtime and returns the handle immediately. Wire-up
/// contract (the coding-flow seam):
///
/// 1. `terminal.attach_sink(handle.raw_sink())` — and `detach_sink` on
///    teardown;
/// 2. exit hook → `handle.shutdown(Some(format!("exit:{code}")))`;
/// 3. §6.10 local resize → `handle.notify_local_resize(cols, rows)`;
/// 4. `sync::kill_watch` on_ended → `handle.session_ended()` (after killing
///    the child).
pub fn publish(
    runtime: &SteerRuntime,
    spec: PublishSpec,
    tickets: Arc<dyn PublisherTickets>,
    hooks: PublisherHooks,
) -> PublisherHandle {
    let (sink, out_rx) = PublisherSink::bounded();
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let running = Arc::new(AtomicBool::new(true));
    let handle = PublisherHandle {
        sink,
        cmd_tx,
        running: running.clone(),
    };
    runtime
        .handle()
        .spawn(run_publisher_loop(spec, tickets, hooks, out_rx, cmd_rx, running));
    handle
}

// ---------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------

/// How one connection ended.
enum LoopEnd {
    /// `bye` sent, socket closed — the task is done.
    Clean,
    /// Relay closed with a code (§8.6 semantics apply).
    Closed(Option<u16>),
    /// Unexpected drop (socket error / EOF without close).
    Dropped,
}

async fn run_publisher_loop(
    spec: PublishSpec,
    tickets: Arc<dyn PublisherTickets>,
    hooks: PublisherHooks,
    out_rx: flume::Receiver<Vec<u8>>,
    cmd_rx: flume::Receiver<PublisherCmd>,
    running: Arc<AtomicBool>,
) {
    let mut ring = RingBuffer::default();
    let mut backoff = Backoff::publisher();
    // §8.7: one immediate re-mint is allowed after a fresh-ticket 401; a
    // second consecutive 401 surfaces the clock-skew error and stops.
    let mut unauthorized_once = false;

    'reconnect: while running.load(Ordering::SeqCst) {
        // Mint (blocking ureq off the reactor), then dial IMMEDIATELY (§8.7).
        let tickets_for_mint = tickets.clone();
        let minted = match tokio::task::spawn_blocking(move || tickets_for_mint.mint()).await {
            Ok(result) => result,
            Err(join_err) => {
                log::warn!("steer publisher: mint task panicked: {join_err}");
                running.store(false, Ordering::SeqCst);
                return;
            }
        };
        let url = match minted {
            Ok(Some(ticket)) => ticket.url,
            Ok(None) => {
                // Relay disabled: a normal state — no remote mirror (§8.4).
                log::info!("steer publisher: relay disabled; not publishing");
                running.store(false, Ordering::SeqCst);
                return;
            }
            Err(ApiError::Unauthorized) => {
                log::info!("steer publisher: session token dead; not publishing");
                running.store(false, Ordering::SeqCst);
                return;
            }
            Err(ApiError::Http { status: 403, message }) => {
                // Not the session owner — authorization is mint-time (§8.0).
                (hooks.error)(format!("Steer publish refused: {message}"));
                running.store(false, Ordering::SeqCst);
                return;
            }
            Err(err) => {
                log::debug!("steer publisher: mint failed: {err}");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &running).await.is_break() {
                    return;
                }
                continue 'reconnect;
            }
        };

        let mut ws = match dial(&url).await {
            Ok(stream) => stream,
            Err(DialError::Unauthorized) => {
                if unauthorized_once {
                    // Two consecutive fresh tickets rejected: clock skew.
                    (hooks.error)(CLOCK_SKEW_ERROR.to_string());
                    running.store(false, Ordering::SeqCst);
                    return;
                }
                unauthorized_once = true;
                // Re-mint once and retry immediately (§8.7) — no backoff.
                continue 'reconnect;
            }
            Err(DialError::Other(reason)) => {
                log::debug!("steer publisher: connect failed: {reason}");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &running).await.is_break() {
                    return;
                }
                continue 'reconnect;
            }
        };
        unauthorized_once = false;

        // §8.4 #2 — hello with TRUE current geometry; the relay creates the
        // room (or resumes it on re-hello, ring intact, evicting any stale
        // publisher with CLOSE_REPLACED). No resize comes back; no resync is
        // sent on reconnect — resume live teeing at once (§8.6).
        let (cols, rows) = (hooks.geometry)();
        // Clamp to the relay's zod bound (§8.1): out-of-range geometry is
        // silently dropped, leaving the room uncreated.
        let (cols, rows) = (clamp_dim(cols), clamp_dim(rows));
        let hello = ClientFrame::Hello {
            session_id: &spec.session_id,
            issue_id: spec.issue_id.as_deref(),
            cols: Some(cols),
            rows: Some(rows),
            // Absent = public (the legacy wire shape); only the keep-private
            // opt-out is explicit (EXP-32).
            activity_public: if spec.activity_public { None } else { Some(false) },
        }
        .to_json();
        if let Err(err) = ws.send(Message::Text(hello)).await {
            log::debug!("steer publisher: hello failed: {err}");
            if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &running).await.is_break() {
                return;
            }
            continue 'reconnect;
        }
        log::info!(
            "steer publisher: room {} live at {cols}x{rows}",
            spec.session_id
        );
        let established = Instant::now();
        let mut last_sent_geometry = (cols, rows);

        let end = pump_connection(
            &mut ws,
            &hooks,
            &out_rx,
            &cmd_rx,
            &mut ring,
            &running,
            &mut last_sent_geometry,
        )
        .await;

        match end {
            LoopEnd::Clean => {
                running.store(false, Ordering::SeqCst);
                return;
            }
            LoopEnd::Closed(Some(CLOSE_SESSION_ENDED)) => {
                // 4001: the session is over — never reconnect; make sure the
                // local session tears down too (§8.6).
                log::info!("steer publisher: relay says session ended");
                running.store(false, Ordering::SeqCst);
                (hooks.kill)(KillSignal::SessionEnded);
                return;
            }
            LoopEnd::Closed(Some(CLOSE_REPLACED)) => {
                // 4002: a newer publisher socket owns the room — this socket
                // must not fight it (expected during our own reconnect race).
                log::info!("steer publisher: replaced by a newer socket");
                running.store(false, Ordering::SeqCst);
                return;
            }
            LoopEnd::Closed(Some(CLOSE_UNAUTHORIZED)) => {
                if unauthorized_once {
                    (hooks.error)(CLOCK_SKEW_ERROR.to_string());
                    running.store(false, Ordering::SeqCst);
                    return;
                }
                unauthorized_once = true;
                continue 'reconnect; // re-mint once, immediately (§8.6/§8.7)
            }
            LoopEnd::Closed(_) | LoopEnd::Dropped => {
                // 4008 (viewer-side code — treat as a normal drop) and any
                // other unexpected drop: reconnect while the session is still
                // running, resuming the same room (§8.6). Reconnect promptly —
                // the relay's staleTimer bounds the grace window.
                if established.elapsed() >= BACKOFF_RESET_AFTER {
                    backoff.reset();
                }
                if !running.load(Ordering::SeqCst) {
                    return;
                }
                log::debug!("steer publisher: dropped; reconnecting");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &running).await.is_break() {
                    return;
                }
            }
        }
    }
}

/// One connection's select loop (§8.4's pseudocode, made real).
async fn pump_connection(
    ws: &mut WsStream,
    hooks: &PublisherHooks,
    out_rx: &flume::Receiver<Vec<u8>>,
    cmd_rx: &flume::Receiver<PublisherCmd>,
    ring: &mut RingBuffer,
    running: &Arc<AtomicBool>,
    last_sent_geometry: &mut (u16, u16),
) -> LoopEnd {
    // EXP-72: when a steerer's Enter (a bare `\r` frame) chases their message
    // text this closely, the child can read text+`\r` as ONE chunk and the
    // `claude` TUI's paste heuristic inserts a newline instead of submitting.
    // Hold the `\r` back until the child has had a beat to drain the text.
    // Ordering is safe — this task is the only remote-input writer.
    const ENTER_SEPARATION: Duration = Duration::from_millis(150);
    let mut last_input_at: Option<Instant> = None;
    loop {
        tokio::select! {
            // 1) terminal output → binary 0x01 frame (+ replay ring).
            chunk = out_rx.recv_async() => {
                let Ok(chunk) = chunk else { return LoopEnd::Dropped };
                ring.push(&chunk);
                if ws.send(Message::Binary(output_frame(&chunk))).await.is_err() {
                    return LoopEnd::Dropped;
                }
            }
            // 2) local control commands (unbounded — never dropped).
            cmd = cmd_rx.recv_async() => {
                let Ok(cmd) = cmd else { return LoopEnd::Dropped };
                match cmd {
                    PublisherCmd::LocalResize { cols, rows } => {
                        // Clamp to the relay's zod bound (§8.1) so a wide grid
                        // never silently drops the reflow, then anti-ping-pong:
                        // only genuine changes go up (§8.4).
                        let (cols, rows) = (clamp_dim(cols), clamp_dim(rows));
                        if (cols, rows) != *last_sent_geometry {
                            *last_sent_geometry = (cols, rows);
                            let frame = ClientFrame::Resize { cols, rows }.to_json();
                            if ws.send(Message::Text(frame)).await.is_err() {
                                return LoopEnd::Dropped;
                            }
                        }
                    }
                    PublisherCmd::TakeOver => {
                        // §8.5: publisher-sent claim = relay publisherTakeover.
                        if ws.send(Message::Text(ClientFrame::Claim.to_json())).await.is_err() {
                            return LoopEnd::Dropped;
                        }
                    }
                    PublisherCmd::Activity(event) => {
                        // §P7: publish one already-redacted public activity
                        // event. The relay fans it to public viewers only.
                        let frame = ClientFrame::Activity { event }.to_json();
                        // The emitter caps event strings in UTF-8 bytes, but
                        // JSON escaping can still inflate pathological content
                        // past the relay's frame limit — dropping the event
                        // beats letting the relay close the shared socket.
                        if frame.len() >= RELAY_MAX_PAYLOAD_BYTES {
                            log::warn!(
                                "steer publisher: dropping oversize activity frame ({} bytes)",
                                frame.len()
                            );
                        } else if ws.send(Message::Text(frame)).await.is_err() {
                            return LoopEnd::Dropped;
                        }
                    }
                    PublisherCmd::Shutdown { outcome } => {
                        let bye = ClientFrame::Bye { outcome: outcome.as_deref() }.to_json();
                        let _ = ws.send(Message::Text(bye)).await;
                        let _ = ws.close(None).await; // 1000 normal closure
                        return LoopEnd::Clean;
                    }
                }
            }
            // 3) relay → publisher control frames.
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => match ServerFrame::parse(&text) {
                    Some(ServerFrame::Input { data }) => {
                        if data == "\r" {
                            if let Some(at) = last_input_at {
                                let elapsed = at.elapsed();
                                if elapsed < ENTER_SEPARATION {
                                    tokio::time::sleep(ENTER_SEPARATION - elapsed).await;
                                }
                            }
                        }
                        last_input_at = Some(Instant::now());
                        (hooks.write_input)(data.as_bytes())
                    }
                    Some(ServerFrame::Resize { cols, rows }) => (hooks.resize)(cols, rows),
                    Some(ServerFrame::Kill) => {
                        // §8.4: relay kill → end the session. The kill hook
                        // kills the child (whose exit hook ends the synced
                        // row); we close the room cleanly right away.
                        log::info!("steer publisher: kill received");
                        running.store(false, Ordering::SeqCst);
                        (hooks.kill)(KillSignal::RemoteKill);
                        let bye = ClientFrame::Bye { outcome: Some("killed") }.to_json();
                        let _ = ws.send(Message::Text(bye)).await;
                        let _ = ws.close(None).await;
                        return LoopEnd::Clean;
                    }
                    Some(ServerFrame::Presence { viewers, steerer_id }) => {
                        (hooks.presence)(Presence { viewers, steerer_id });
                    }
                    Some(ServerFrame::Resync) => {
                        // Slow-consumer recovery ONLY (§8.4): resend the ring
                        // as 0x01 frames. NOT viewer join, NOT reconnect.
                        for chunk in ring.replay() {
                            if ws.send(Message::Binary(output_frame(chunk))).await.is_err() {
                                return LoopEnd::Dropped;
                            }
                        }
                    }
                    Some(ServerFrame::Bye { outcome }) => {
                        log::debug!("steer publisher: relay bye ({outcome:?})");
                        return LoopEnd::Dropped;
                    }
                    Some(ServerFrame::Error { code, message }) => {
                        log::debug!("steer publisher: relay error {code} ({message:?})");
                        return LoopEnd::Dropped;
                    }
                    Some(ServerFrame::StartSession { .. }) => {
                        // Control-socket frame; never valid here. Ignore.
                    }
                    None => log::debug!("steer publisher: unparseable frame ignored"),
                },
                Some(Ok(Message::Close(frame))) => {
                    return LoopEnd::Closed(close_code(&frame));
                }
                Some(Ok(_binary_or_ping)) => {
                    // Publishers PRODUCE 0x01 frames, never consume them
                    // (§8.1); pings are answered by tungstenite internally.
                }
                Some(Err(err)) => {
                    log::debug!("steer publisher: socket error: {err}");
                    return LoopEnd::Dropped;
                }
                None => return LoopEnd::Dropped,
            }
        }
    }
}

fn close_code(frame: &Option<CloseFrame<'_>>) -> Option<u16> {
    frame.as_ref().map(|f| u16::from(f.code))
}

/// Interruptible backoff sleep: `Break` on `Shutdown` (we're disconnected —
/// nothing to `bye`) or when `running` flipped. Other commands during a
/// disconnect are momentary UI state and safely superseded by the re-`hello`.
async fn sleep_or_shutdown(
    delay: Duration,
    cmd_rx: &flume::Receiver<PublisherCmd>,
    running: &Arc<AtomicBool>,
) -> std::ops::ControlFlow<()> {
    let deadline = tokio::time::Instant::now() + delay;
    loop {
        if !running.load(Ordering::SeqCst) {
            return std::ops::ControlFlow::Break(());
        }
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return std::ops::ControlFlow::Continue(()),
            cmd = cmd_rx.recv_async() => match cmd {
                Ok(PublisherCmd::Shutdown { .. }) | Err(_) => {
                    running.store(false, Ordering::SeqCst);
                    return std::ops::ControlFlow::Break(());
                }
                Ok(_ignored_while_disconnected) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ── Backpressure policy (§8.4): drop output, never block, never wedge ──

    #[test]
    fn sink_never_blocks_and_drops_past_the_cap() {
        let (sink, rx) = PublisherSink::bounded();
        let start = Instant::now();
        for i in 0..100u8 {
            sink.on_output(&[i]);
        }
        // 100 sends with no drainer return immediately (try_send).
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_eq!(rx.len(), IN_FLIGHT_CAP, "channel holds exactly the cap");
        assert_eq!(sink.dropped(), (100 - IN_FLIGHT_CAP) as u64);
        // The buffered chunks are the OLDEST (drop-newest-on-overflow): the
        // consumer resumes from a contiguous prefix, the ring covers the gap.
        let first = rx.recv().unwrap();
        assert_eq!(first, vec![0u8]);
    }

    #[test]
    fn sink_recovers_after_drain() {
        let (sink, rx) = PublisherSink::bounded();
        for i in 0..(IN_FLIGHT_CAP as u8 + 5) {
            sink.on_output(&[i]);
        }
        assert_eq!(sink.dropped(), 5);
        while rx.try_recv().is_ok() {}
        sink.on_output(b"after");
        assert_eq!(sink.dropped(), 5, "no new drops once drained");
        assert_eq!(rx.recv().unwrap(), b"after");
    }

    #[test]
    fn control_channel_is_unbounded_and_lossless() {
        // §8.4: control frames are NEVER dropped — resize/kill/bye ride an
        // unbounded path even when output is saturated.
        let (cmd_tx, cmd_rx) = flume::unbounded();
        for i in 0..10_000u16 {
            cmd_tx
                .send(PublisherCmd::LocalResize {
                    cols: i % 500 + 1,
                    rows: 40,
                })
                .expect("unbounded send never fails");
        }
        cmd_tx
            .send(PublisherCmd::Shutdown { outcome: None })
            .unwrap();
        assert_eq!(cmd_rx.len(), 10_001);
    }

    // ── Handle semantics ────────────────────────────────────────────────────

    #[test]
    fn shutdown_flips_running_and_queues_bye() {
        let (sink, _out_rx) = PublisherSink::bounded();
        let (cmd_tx, cmd_rx) = flume::unbounded();
        let handle = PublisherHandle {
            sink,
            cmd_tx,
            running: Arc::new(AtomicBool::new(true)),
        };
        assert!(handle.is_active());
        handle.shutdown(Some("exit:0".to_string()));
        assert!(!handle.is_active());
        assert_eq!(
            cmd_rx.try_recv().unwrap(),
            PublisherCmd::Shutdown {
                outcome: Some("exit:0".to_string())
            }
        );
        handle.session_ended();
        assert_eq!(
            cmd_rx.try_recv().unwrap(),
            PublisherCmd::Shutdown {
                outcome: Some("killed".to_string())
            }
        );
    }

    // ── EXP-72: remote input = keystrokes raw, message text = local paste ──

    /// A `Terminal::writer()`-shaped writer that records into a shared Vec.
    fn vec_writer() -> (
        Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>>,
        Arc<Mutex<Vec<u8>>>,
    ) {
        struct SharedVec(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for SharedVec {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let writer: Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>> = Arc::new(
            std::sync::Mutex::new(Box::new(SharedVec(recorded.clone()))),
        );
        (writer, recorded)
    }

    /// Feed raw bytes into an emulator term (the emulator-test `advance`
    /// pattern) — turbofish REQUIRED for the `T: Timeout` default param.
    fn advance(term: &terminal::TermHandle, bytes: &[u8]) {
        let mut processor = vte::ansi::Processor::<vte::ansi::StdSyncHandler>::new();
        processor.advance(&mut *term.lock(), bytes);
    }

    #[test]
    fn keystroke_frames_pass_raw_even_with_bracketed_paste_on() {
        let (writer, recorded) = vec_writer();
        let term = terminal::Emulator::new(80, 24).term();
        advance(&term, b"\x1b[?2004h");
        let hook = pty_writer_input_hook(writer, term);
        hook(b"\r"); // Enter (submit)
        hook(b"\x1b"); // interrupt
        hook(b"\x1b[A"); // CSI sequence (arrow up)
        assert_eq!(recorded.lock().unwrap().as_slice(), b"\r\x1b\x1b[A");
    }

    #[test]
    fn text_frames_are_bracketed_when_the_child_enabled_mode_2004() {
        let (writer, recorded) = vec_writer();
        let term = terminal::Emulator::new(80, 24).term();
        advance(&term, b"\x1b[?2004h");
        let hook = pty_writer_input_hook(writer, term);
        hook("fix the login bug".as_bytes());
        assert_eq!(
            recorded.lock().unwrap().as_slice(),
            b"\x1b[200~fix the login bug\x1b[201~"
        );
    }

    #[test]
    fn text_frames_pass_raw_when_mode_2004_is_off() {
        let (writer, recorded) = vec_writer();
        let term = terminal::Emulator::new(80, 24).term();
        let hook = pty_writer_input_hook(writer, term);
        hook(b"echo hi");
        assert_eq!(recorded.lock().unwrap().as_slice(), b"echo hi");
    }

    // ── Full-task test against a local fake relay (tokio-tungstenite server)

    struct FakeTickets {
        url: String,
    }

    impl PublisherTickets for FakeTickets {
        fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
            Ok(Some(MintedTicket {
                ticket: "fake.fake".to_string(),
                url: self.url.clone(),
            }))
        }
    }

    #[derive(Default)]
    struct Recorded {
        inputs: Mutex<Vec<Vec<u8>>>,
        resizes: Mutex<Vec<(u16, u16)>>,
        kills: Mutex<Vec<KillSignal>>,
        presences: Mutex<Vec<Presence>>,
        errors: Mutex<Vec<String>>,
    }

    fn recording_hooks(recorded: Arc<Recorded>) -> PublisherHooks {
        let r1 = recorded.clone();
        let r2 = recorded.clone();
        let r3 = recorded.clone();
        let r4 = recorded.clone();
        let r5 = recorded;
        PublisherHooks {
            write_input: Arc::new(move |bytes| {
                r1.inputs.lock().unwrap().push(bytes.to_vec());
            }),
            resize: Arc::new(move |cols, rows| {
                r2.resizes.lock().unwrap().push((cols, rows));
            }),
            geometry: Arc::new(|| (100, 30)),
            kill: Arc::new(move |signal| {
                r3.kills.lock().unwrap().push(signal);
            }),
            presence: Arc::new(move |presence| {
                r4.presences.lock().unwrap().push(presence);
            }),
            error: Arc::new(move |message| {
                r5.errors.lock().unwrap().push(message);
            }),
        }
    }

    /// Wait (bounded) until `predicate` holds — hooks fire on runtime threads.
    fn wait_for(predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(Instant::now() < deadline, "timed out waiting");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn publisher_hellos_teams_inputs_resyncs_and_kills_against_a_fake_relay() {
        let runtime = SteerRuntime::new().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();

        // Server-side transcript, observed from the fake relay.
        let (seen_tx, seen_rx) = flume::unbounded::<String>();
        let (bin_tx, bin_rx) = flume::unbounded::<Vec<u8>>();
        // Frames the test injects relay→publisher.
        let (inject_tx, inject_rx) = flume::unbounded::<Message>();

        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (stream, _addr) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            loop {
                tokio::select! {
                    inject = inject_rx.recv_async() => {
                        let Ok(message) = inject else { break };
                        if ws.send(message).await.is_err() { break; }
                    }
                    msg = ws.next() => match msg {
                        Some(Ok(Message::Text(text))) => { let _ = seen_tx.send(text); }
                        Some(Ok(Message::Binary(bytes))) => { let _ = bin_tx.send(bytes); }
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
        });

        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-t".to_string(),
                issue_id: Some("issue-t".to_string()),
                activity_public: true,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded.clone()),
        );

        // 1) hello with TRUE geometry (the hook says 100×30, not 80×24).
        // activity_public: true stays ABSENT on the wire (legacy shape).
        let hello = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            hello,
            r#"{"t":"hello","sessionId":"sess-t","issueId":"issue-t","cols":100,"rows":30}"#
        );

        // 2) teed output arrives as 0x01 binary frames (and fills the ring).
        handle.raw_sink().on_output(b"chunk-1");
        handle.raw_sink().on_output(b"chunk-2");
        let first = bin_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first, output_frame(b"chunk-1"));
        let second = bin_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(second, output_frame(b"chunk-2"));

        // 3) remote input → the PTY-writer hook, byte-identical.
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"ls\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(recorded.inputs.lock().unwrap()[0], b"ls\r");

        // 4) steerer resize → resize hook.
        inject_tx
            .send(Message::Text(r#"{"t":"resize","cols":90,"rows":25}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.resizes.lock().unwrap().is_empty());
        assert_eq!(recorded.resizes.lock().unwrap()[0], (90, 25));

        // 5) presence → banner hook.
        inject_tx
            .send(Message::Text(
                r#"{"t":"presence","viewers":[{"userId":"v1","name":"Phone","perm":"steer"}],"steererId":"v1"}"#.to_string(),
            ))
            .unwrap();
        wait_for(|| !recorded.presences.lock().unwrap().is_empty());
        let presence = recorded.presences.lock().unwrap()[0].clone();
        assert_eq!(presence.steerer_id.as_deref(), Some("v1"));
        assert_eq!(presence.viewers[0].name, "Phone");

        // 6) local resize forwards (deduped against the hello geometry).
        handle.notify_local_resize(100, 30); // no-op: unchanged
        handle.notify_local_resize(120, 40);
        let resize = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(resize, r#"{"t":"resize","cols":120,"rows":40}"#);

        // 7) take over → a publisher `claim`.
        handle.take_over();
        let claim = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(claim, r#"{"t":"claim"}"#);

        // 8) resync → the ring replays as 0x01 frames (slow-consumer path).
        inject_tx
            .send(Message::Text(r#"{"t":"resync"}"#.to_string()))
            .unwrap();
        let replay_1 = bin_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(replay_1, output_frame(b"chunk-1"));
        let replay_2 = bin_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(replay_2, output_frame(b"chunk-2"));

        // 9) relay kill → kill hook fires, clean bye goes out, task stops.
        inject_tx
            .send(Message::Text(r#"{"t":"kill"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.kills.lock().unwrap().is_empty());
        assert_eq!(recorded.kills.lock().unwrap()[0], KillSignal::RemoteKill);
        let bye = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(bye, r#"{"t":"bye","outcome":"killed"}"#);
        wait_for(|| !handle.is_active());
        assert!(recorded.errors.lock().unwrap().is_empty());
    }

    #[test]
    fn clean_shutdown_sends_bye_with_exit_outcome() {
        let runtime = SteerRuntime::new().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (seen_tx, seen_rx) = flume::unbounded::<String>();

        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (stream, _addr) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(text) = msg {
                    let _ = seen_tx.send(text);
                }
            }
        });

        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-x".to_string(),
                issue_id: None,
                // EXP-32: the keep-private path — hello carries the explicit
                // opt-out.
                activity_public: false,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded),
        );
        let hello = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            hello,
            r#"{"t":"hello","sessionId":"sess-x","cols":100,"rows":30,"activityPublic":false}"#
        );

        handle.shutdown(Some("exit:0".to_string()));
        let bye = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(bye, r#"{"t":"bye","outcome":"exit:0"}"#);
        wait_for(|| !handle.is_active());
    }

    #[test]
    fn disabled_relay_ends_the_task_without_noise() {
        struct DisabledTickets;
        impl PublisherTickets for DisabledTickets {
            fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
                Ok(None)
            }
        }
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-d".to_string(),
                issue_id: None,
                activity_public: true,
            },
            Arc::new(DisabledTickets),
            recording_hooks(recorded.clone()),
        );
        wait_for(|| !handle.is_active());
        assert!(recorded.errors.lock().unwrap().is_empty(), "EXP-4: no noise");
    }
}
