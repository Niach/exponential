//! The steering VIEWER connection (EXP-696) — watch and steer a coding
//! session running on ANOTHER of the user's devices.
//!
//! The third relay role. [`publisher`] streams a session this machine is
//! hosting and [`control_channel`] announces the machine itself; this module
//! is the other end of the publisher's wire, and it is what web/iOS/Android
//! have had since EXP-249. It dials the relay with a server-minted VIEWER
//! ticket, joins the one audience there is (`channel: "activity"`), turns
//! everything that arrives into [`ViewerEvent`]s on a channel, and sends
//! steering back up: whole composer messages, semantic answers, raw
//! keystrokes.
//!
//! [`publisher`]: crate::publisher
//! [`control_channel`]: crate::control_channel
//!
//! **The UI is a dumb reducer.** Nothing here interprets a feed: the socket
//! task emits phases and frames, the caller pumps them into a [`SteerFeed`]
//! and renders it. That split is what makes the whole protocol testable
//! without a UI — and the feed testable without a socket.
//!
//! [`SteerFeed`]: crate::feed::SteerFeed
//!
//! ## The reconnect rules (mirrored from `steer-session-store.ts`)
//!
//! A viewer socket dies for many reasons and only some of them mean "the
//! session is over". Getting this wrong strands the user on a dead feed or
//! spins the relay's connect budget, so each case is explicit:
//!
//! | signal | meaning | what happens |
//! |---|---|---|
//! | `error no_such_session` | the desktop's publisher has not `hello`'d yet | [`ViewerPhase::Starting`] + jittered 3s→30s redial |
//! | `bye {outcome:"publisher_lost"}` | the publisher's socket dropped; the run may live | retry like any drop |
//! | `bye` (any other outcome) | the room is finished | [`ViewerPhase::Ended`], stop |
//! | close 4008 (slow consumer) | the relay evicted a saturated socket | SILENT immediate redial — no phase flap |
//! | close 4003 (unauthorized) | a "no" a retry re-mints forever | [`ViewerPhase::Unauthorized`], stop |
//! | 45s of total silence while live | the socket is dead, not the agent quiet | silent redial (EXP-648: the relay's 15s `keepalive` is what makes "quiet" distinguishable from "gone") |
//! | anything else | a transport drop | [`ViewerPhase::Reconnecting`] + backoff |
//!
//! Steering is owner-only and enforced at MINT time (EXP-312) — there is no
//! client-side gate here, and there must not be one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use api::error::ApiError;
use api::steer::MintedTicket;
use api::trpc::TrpcClient;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use crate::frames::{
    ActivityEvent, ClientFrame, ViewerFrame, CLOSE_SESSION_ENDED, CLOSE_SLOW_CONSUMER,
    CLOSE_UNAUTHORIZED,
};
use crate::{dial, Backoff, DialError, SteerRuntime, WsStream};

/// The relay rejects `input` frames over 8 KiB; chunk well under that. The cap
/// is measured in UTF-16 code units — the relay validates against a JS
/// string's `length` — so the chunker counts UTF-16 units too (web
/// `INPUT_CHUNK_CHARS`, iOS `inputChunkUtf16`).
pub const INPUT_CHUNK_UTF16: usize = 4096;

/// §8.7: surfaced after two consecutive fresh-ticket 401s — the relay only
/// rejects a ticket at upgrade time when it is expired on arrival, and a
/// second fresh one failing the same way is a clock, not a network.
const CLOCK_SKEW_ERROR: &str = "Steer relay rejected the viewer ticket (expired on arrival) — \
     check that this machine's clock is in sync (NTP).";

/// Timings the viewer loop runs on. Defaults are the production values
/// (web/iOS/Android parity); tests shrink them.
#[derive(Clone, Debug)]
pub struct ViewerTimings {
    /// EXP-648: a LIVE socket that has delivered nothing — not even one of
    /// the relay's 15s `keepalive` beats — for this long is dead, not quiet.
    /// Three missed beats. Mirrors web `LIVE_STALE_MS`, iOS
    /// `liveStaleSeconds`, Android `liveStaleMs`.
    pub live_stale: Duration,
    /// First redial bound while the desktop's publisher is still starting.
    pub retry_base: Duration,
    /// Cap the redial bound doubles toward.
    pub retry_cap: Duration,
    /// How often the loop re-checks the staleness window. Coarse: one timer
    /// for the connection instead of one per frame.
    pub liveness_tick: Duration,
}

impl Default for ViewerTimings {
    fn default() -> Self {
        Self {
            live_stale: Duration::from_secs(45),
            retry_base: Duration::from_secs(3),
            retry_cap: Duration::from_secs(30),
            liveness_tick: Duration::from_secs(5),
        }
    }
}

// ---------------------------------------------------------------------------
// Tickets
// ---------------------------------------------------------------------------

/// Viewer-ticket source, injectable for tests (the [`crate::publisher`]
/// `PublisherTickets` pattern). Blocking (reqwest underneath) — the loop wraps
/// calls in `spawn_blocking`. `Ok(None)` = the instance reports steering
/// disabled ⇒ there is nothing to watch, ever, on this deployment.
pub trait ViewerTickets: Send + Sync + 'static {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError>;
}

/// Production tickets over the account's tRPC client:
/// `steer.mintTicket({kind:"viewer", codingSessionId})`. The server decides
/// authorization there and nowhere else (EXP-312: a LIVE session is
/// viewable/steerable only by its owner).
pub struct TrpcViewerTickets {
    pub trpc: Arc<TrpcClient>,
    pub coding_session_id: String,
}

impl ViewerTickets for TrpcViewerTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        Ok(api::steer::mint_viewer_ticket(&self.trpc, &self.coding_session_id)?.into_ticket())
    }
}

// ---------------------------------------------------------------------------
// What the UI observes
// ---------------------------------------------------------------------------

/// Where the connection stands. `Ended` and `Unauthorized` are TERMINAL — the
/// loop has stopped and the handle is inert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewerPhase {
    /// Minting + dialing the first socket.
    Connecting,
    /// The relay answered `no_such_session` while the session row still says
    /// running: the desktop is still dialing its publisher socket. Auto-
    /// redialing with jittered backoff.
    Starting,
    /// Joined; frames are flowing.
    Live,
    /// An unexpected drop — redialing with backoff. (The two SILENT redials,
    /// slow-consumer eviction and live staleness, deliberately do NOT enter
    /// this phase: the session never stopped being live, and a flicker to
    /// "Reconnecting" for a socket that comes straight back is a lie.)
    Reconnecting,
    /// The room is finished. `outcome` is the relay's, minus the uninformative
    /// `"ended"`.
    Ended { outcome: Option<String> },
    /// A refusal a retry cannot turn into a yes: steering disabled on the
    /// instance, a mint refused for ownership or a gone row, a ticket the
    /// relay rejected twice. Terminal by design (web `closed {terminal:true}`)
    /// — respawn the viewer to try again.
    Unauthorized { detail: Option<String> },
}

impl ViewerPhase {
    /// Whether the loop has stopped for good.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ViewerPhase::Ended { .. } | ViewerPhase::Unauthorized { .. }
        )
    }
}

/// Everything the socket task tells the UI, in arrival order. Apply them to a
/// [`crate::feed::SteerFeed`] and render it; the enum is deliberately close to
/// the wire so the UI adds no protocol logic of its own.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewerEvent {
    /// A phase TRANSITION (emitted only when the phase actually changes — a
    /// per-frame re-emission would re-render the world on every replayed
    /// message).
    Phase(ViewerPhase),
    /// The socket itself opened / went away. Distinct from the phase: a
    /// silent redial keeps the phase steady while the socket is briefly down,
    /// and send affordances should dim honestly for that gap.
    Connected(bool),
    /// One activity event → `SteerFeed::apply`.
    Activity(ActivityEvent),
    /// `activity_reset` → `SteerFeed::apply_reset` (which STAGES; it does not
    /// blank the feed — EXP-656).
    Reset,
    /// `activity_synced` → `SteerFeed::apply_synced`.
    Synced,
    /// The relay's 15s liveness beat. Carries nothing; the loop has already
    /// counted it. Forwarded because EXP-656 also treats it as an
    /// end-of-replay signal for a markerless republish.
    Keepalive,
    /// A message THIS client just sent — emitted from
    /// [`ViewerHandle::send_message`] so the local echo and the inbound
    /// stream reach the feed through the one ordered channel.
    LocalMessage(String),
}

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

/// What the handle asks the socket task to do.
#[derive(Clone, Debug, PartialEq)]
enum ViewerCmd {
    /// Pre-serialized text frames, written in order.
    Frames(Vec<String>),
    /// Abandon the current socket and redial NOW (cutting short any backoff).
    Kick,
    /// Stop for good.
    Shutdown,
}

/// The UI's handle onto a running viewer connection.
pub struct ViewerHandle {
    cmd_tx: flume::Sender<ViewerCmd>,
    events_tx: flume::Sender<ViewerEvent>,
    connected: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    session_ended: Arc<AtomicBool>,
}

impl ViewerHandle {
    /// Send one message to the agent: the text (chunked), then a SEPARATE
    /// `\r` frame. Bundled into one write, TUIs read the trailing return as
    /// part of a PASTE and insert a newline instead of submitting (EXP-72).
    ///
    /// The message is also emitted as [`ViewerEvent::LocalMessage`], so the
    /// feed renders it immediately and dedupes its transcript-derived
    /// `user_message` twin when the publisher echoes it back.
    ///
    /// `false` when the socket is down — the caller keeps its draft.
    pub fn send_message(&self, text: &str) -> bool {
        if text.is_empty() || !self.send_frames(message_frames(text)) {
            return false;
        }
        let _ = self.events_tx.send(ViewerEvent::LocalMessage(text.to_string()));
        true
    }

    /// Protocol v2 answer: the relay forwards it verbatim to the desktop,
    /// which drives its own picker and confirms with `answer_ack`. The caller
    /// locks the card (`SteerFeed::note_answer_sent`) the instant this
    /// returns true.
    pub fn send_answer(
        &self,
        question_id: &str,
        ask_id: Option<&str>,
        keys: &[String],
        text: Option<&str>,
    ) -> bool {
        self.send_frames(vec![ClientFrame::Answer {
            question_id: question_id.to_string(),
            ask_id: ask_id.map(str::to_string),
            keys: keys.to_vec(),
            text: text.map(str::to_string),
        }
        .to_json()])
    }

    /// LEGACY answer path, for a desktop that publishes no question ids: raw
    /// keystrokes, one frame each, and NO trailing `\r` — a digit already
    /// selects AND advances in claude's picker, so the extra return cascaded
    /// into the next question and auto-answered it (EXP-249).
    pub fn send_keystrokes(&self, keys: &[String]) -> bool {
        self.send_frames(keystroke_frames(keys))
    }

    /// A wakeup nudge (the machine woke, the network came back, the host
    /// device came online): abandon whatever socket is there and redial now,
    /// cutting short a pending backoff. Cheap — callers may fire it freely.
    pub fn kick(&self) {
        let _ = self.cmd_tx.send(ViewerCmd::Kick);
    }

    /// The synced `coding_sessions` row says the run ended. The redial loops
    /// take it as the truth: a `no_such_session` for an ended run can only
    /// park the viewer in [`ViewerPhase::Starting`] forever, because the
    /// publisher is gone (web `noteSessionStatus`, EXP-639).
    pub fn note_session_ended(&self) {
        self.session_ended.store(true, Ordering::SeqCst);
    }

    /// Close the connection for good (the tab closed, the app is signing
    /// out). Idempotent.
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_tx.send(ViewerCmd::Shutdown);
    }

    /// Whether the socket is open right now (dim the composer honestly).
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// False once the loop stopped for good (shutdown, ended, unauthorized).
    pub fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn send_frames(&self, frames: Vec<String>) -> bool {
        if frames.is_empty() || !self.is_connected() {
            return false;
        }
        self.cmd_tx.send(ViewerCmd::Frames(frames)).is_ok()
    }
}

// ---------------------------------------------------------------------------
// Frame building (pure — the chunking rule is the fiddly part)
// ---------------------------------------------------------------------------

/// Split a message into `input` frame payloads of at most
/// [`INPUT_CHUNK_UTF16`] UTF-16 code units, NEVER splitting a surrogate pair.
///
/// Rust strings are UTF-8 and this walks `chars`, so a surrogate pair (one
/// `char` whose `len_utf16()` is 2) is inherently indivisible — the explicit
/// pair-check the JS/Swift chunkers need has no counterpart here. What DOES
/// carry over is the UNIT: the relay validates against a JS string's
/// `length`, so the budget is UTF-16 code units, not bytes and not chars.
pub fn chunk_input(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut units = 0usize;
    for ch in text.chars() {
        let width = ch.len_utf16();
        if units + width > INPUT_CHUNK_UTF16 && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            units = 0;
        }
        current.push(ch);
        units += width;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// One composer message on the wire: the chunks, then a SEPARATE `\r` frame.
pub fn message_frames(text: &str) -> Vec<String> {
    let mut frames: Vec<String> = chunk_input(text)
        .into_iter()
        .map(|data| ClientFrame::Input { data }.to_json())
        .collect();
    if frames.is_empty() {
        return frames;
    }
    frames.push(
        ClientFrame::Input {
            data: "\r".to_string(),
        }
        .to_json(),
    );
    frames
}

/// Raw keystrokes: one `input` frame per key, no trailing `\r`.
pub fn keystroke_frames(keys: &[String]) -> Vec<String> {
    keys.iter()
        .map(|key| {
            ClientFrame::Input {
                data: key.clone(),
            }
            .to_json()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// Start watching a coding session. Non-blocking: spawns the socket task onto
/// the steer runtime and returns the handle immediately. Drain `events_rx` on
/// the UI side and apply what arrives to a [`crate::feed::SteerFeed`].
pub fn spawn_viewer(
    runtime: &SteerRuntime,
    tickets: Arc<dyn ViewerTickets>,
    session_id: String,
    events_tx: flume::Sender<ViewerEvent>,
) -> ViewerHandle {
    spawn_viewer_with(
        runtime,
        tickets,
        session_id,
        events_tx,
        ViewerTimings::default(),
    )
}

/// [`spawn_viewer`] with explicit timings (tests shrink the windows).
pub fn spawn_viewer_with(
    runtime: &SteerRuntime,
    tickets: Arc<dyn ViewerTickets>,
    session_id: String,
    events_tx: flume::Sender<ViewerEvent>,
    timings: ViewerTimings,
) -> ViewerHandle {
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let running = Arc::new(AtomicBool::new(true));
    let connected = Arc::new(AtomicBool::new(false));
    let session_ended = Arc::new(AtomicBool::new(false));
    let handle = ViewerHandle {
        cmd_tx,
        events_tx: events_tx.clone(),
        connected: connected.clone(),
        running: running.clone(),
        session_ended: session_ended.clone(),
    };
    runtime.handle().spawn(run_viewer_loop(
        session_id,
        tickets,
        events_tx,
        timings,
        cmd_rx,
        running,
        connected,
        session_ended,
    ));
    handle
}

// ---------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------

/// How one connection ended — the whole reconnect policy keys on this.
#[derive(Debug)]
enum ConnEnd {
    /// The handle said stop.
    Shutdown,
    /// The handle said redial now.
    Kicked,
    /// `bye` with a terminal outcome, or a bare `CLOSE_SESSION_ENDED`.
    Ended(Option<String>),
    /// `error no_such_session` — the publisher has not arrived yet.
    Starting,
    /// `CLOSE_UNAUTHORIZED` — terminal.
    Unauthorized,
    /// `CLOSE_SLOW_CONSUMER` — an eviction, not an ending. Silent redial.
    Evicted,
    /// Live but mute past the staleness window. Silent redial.
    Stale,
    /// Any other close, socket error or EOF.
    Dropped(Option<String>),
}

/// Emits phase changes without repeating itself.
struct Phases {
    tx: flume::Sender<ViewerEvent>,
    current: Option<ViewerPhase>,
}

impl Phases {
    fn set(&mut self, phase: ViewerPhase) {
        if self.current.as_ref() == Some(&phase) {
            return;
        }
        self.current = Some(phase.clone());
        let _ = self.tx.send(ViewerEvent::Phase(phase));
    }

    fn is(&self, phase: &ViewerPhase) -> bool {
        self.current.as_ref() == Some(phase)
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_viewer_loop(
    session_id: String,
    tickets: Arc<dyn ViewerTickets>,
    events_tx: flume::Sender<ViewerEvent>,
    timings: ViewerTimings,
    cmd_rx: flume::Receiver<ViewerCmd>,
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    session_ended: Arc<AtomicBool>,
) {
    let mut phases = Phases {
        tx: events_tx.clone(),
        current: None,
    };
    phases.set(ViewerPhase::Connecting);
    let mut backoff = Backoff::new(timings.retry_base, timings.retry_cap);
    // §8.7: one immediate re-mint is allowed after a fresh-ticket 401; a
    // second consecutive one is a clock, not a network.
    let mut unauthorized_once = false;
    // Consecutive slow-consumer evictions. The FIRST redials instantly (the
    // session never stopped being live); a repeat means something is
    // pathological, so it takes the backoff instead of spinning.
    let mut consecutive_evictions = 0u32;

    while running.load(Ordering::SeqCst) {
        // Mint (blocking reqwest off the reactor), then dial IMMEDIATELY —
        // the ticket's connect window is ~60s (§8.7).
        let tickets_for_mint = tickets.clone();
        let minted = match tokio::task::spawn_blocking(move || tickets_for_mint.mint()).await {
            Ok(result) => result,
            Err(join_err) => {
                log::warn!("steer viewer: mint task panicked: {join_err}");
                stop(&running, &mut phases, ViewerPhase::Unauthorized { detail: None });
                return;
            }
        };
        let url = match minted {
            Ok(Some(ticket)) => ticket.url,
            Ok(None) => {
                // Steering is off on this instance: nothing to retry.
                stop(
                    &running,
                    &mut phases,
                    ViewerPhase::Unauthorized {
                        detail: Some("Live steering is unavailable on this instance.".into()),
                    },
                );
                return;
            }
            Err(ApiError::Unauthorized) => {
                stop(
                    &running,
                    &mut phases,
                    ViewerPhase::Unauthorized {
                        detail: Some("Signed out — sign in again to watch this session.".into()),
                    },
                );
                return;
            }
            // A mint refused for ownership (403) or for a row that is gone
            // (404) mints the same "no" forever (web: FORBIDDEN / NOT_FOUND).
            Err(ApiError::Http {
                status: status @ (403 | 404),
                message,
            }) => {
                log::info!("steer viewer: mint refused ({status}): {message}");
                stop(
                    &running,
                    &mut phases,
                    ViewerPhase::Unauthorized {
                        detail: Some(message),
                    },
                );
                return;
            }
            Err(err) => {
                // Transport, 5xx: worth a retry.
                log::debug!("steer viewer: mint failed: {err}");
                if !phases.is(&ViewerPhase::Starting) {
                    phases.set(ViewerPhase::Reconnecting);
                }
                if wait(backoff.next_delay_equal_jitter(), &cmd_rx, &running)
                    .await
                    .is_break()
                {
                    return;
                }
                continue;
            }
        };

        let mut ws = match dial(&url).await {
            Ok(stream) => stream,
            Err(DialError::Unauthorized) => {
                if unauthorized_once {
                    stop(
                        &running,
                        &mut phases,
                        ViewerPhase::Unauthorized {
                            detail: Some(CLOCK_SKEW_ERROR.to_string()),
                        },
                    );
                    return;
                }
                unauthorized_once = true;
                continue; // re-mint once, immediately (§8.7)
            }
            Err(DialError::Other(reason)) => {
                log::debug!("steer viewer: connect failed: {reason}");
                if !phases.is(&ViewerPhase::Starting) {
                    phases.set(ViewerPhase::Reconnecting);
                }
                if wait(backoff.next_delay_equal_jitter(), &cmd_rx, &running)
                    .await
                    .is_break()
                {
                    return;
                }
                continue;
            }
        };
        unauthorized_once = false;

        // The ONE audience. The feed is NEVER wiped here: the relay answers
        // the join with an explicit `activity_reset`, so a redial that never
        // lands keeps showing what was already on screen.
        if ws.send(Message::Text(ClientFrame::join().to_json())).await.is_err() {
            if wait(backoff.next_delay_equal_jitter(), &cmd_rx, &running)
                .await
                .is_break()
            {
                return;
            }
            continue;
        }
        connected.store(true, Ordering::SeqCst);
        let _ = events_tx.send(ViewerEvent::Connected(true));
        log::info!("steer viewer: joined room {session_id}");

        let end = pump_connection(
            &mut ws,
            &events_tx,
            &mut phases,
            &cmd_rx,
            &timings,
            &mut backoff,
        )
        .await;

        connected.store(false, Ordering::SeqCst);
        let _ = events_tx.send(ViewerEvent::Connected(false));

        // The delay this end earns before the next dial, and whether there IS
        // a next dial.
        if !matches!(end, ConnEnd::Evicted) {
            consecutive_evictions = 0;
        }
        let delay = match end {
            ConnEnd::Shutdown => {
                running.store(false, Ordering::SeqCst);
                return;
            }
            ConnEnd::Ended(outcome) => {
                stop(&running, &mut phases, ViewerPhase::Ended { outcome });
                return;
            }
            ConnEnd::Unauthorized => {
                stop(
                    &running,
                    &mut phases,
                    ViewerPhase::Unauthorized {
                        detail: Some("The relay refused this viewer ticket.".into()),
                    },
                );
                return;
            }
            ConnEnd::Starting => {
                // An `in_review` run is still alive and steerable (EXP-194) —
                // only a truly ended one stops the redial.
                if session_ended.load(Ordering::SeqCst) {
                    stop(
                        &running,
                        &mut phases,
                        ViewerPhase::Ended { outcome: None },
                    );
                    return;
                }
                phases.set(ViewerPhase::Starting);
                backoff.next_delay_equal_jitter()
            }
            ConnEnd::Kicked => {
                // The user (or a wakeup) asked: no delay, no phase flap.
                Duration::ZERO
            }
            ConnEnd::Evicted => {
                // EXP-621: an eviction is not an ending. The phase holds
                // (usually `live`), so nothing flickers; only a REPEAT
                // eviction pays a backoff.
                consecutive_evictions += 1;
                if consecutive_evictions > 1 {
                    backoff.next_delay_equal_jitter()
                } else {
                    Duration::ZERO
                }
            }
            ConnEnd::Stale => {
                // EXP-648: live on paper over a socket that said nothing for
                // three keepalives. Redial under the live phase so a socket
                // that turns out fine never flashes "Reconnecting".
                if session_ended.load(Ordering::SeqCst) {
                    stop(&running, &mut phases, ViewerPhase::Ended { outcome: None });
                    return;
                }
                log::debug!("steer viewer: silent redial (no frames for {:?})", timings.live_stale);
                Duration::ZERO
            }
            ConnEnd::Dropped(reason) => {
                if let Some(reason) = reason {
                    log::debug!("steer viewer: dropped: {reason}");
                }
                phases.set(ViewerPhase::Reconnecting);
                backoff.next_delay_equal_jitter()
            }
        };
        if wait(delay, &cmd_rx, &running).await.is_break() {
            return;
        }
    }
}

fn stop(running: &Arc<AtomicBool>, phases: &mut Phases, phase: ViewerPhase) {
    phases.set(phase);
    running.store(false, Ordering::SeqCst);
}

/// One connection's select loop.
async fn pump_connection(
    ws: &mut WsStream,
    events_tx: &flume::Sender<ViewerEvent>,
    phases: &mut Phases,
    cmd_rx: &flume::Receiver<ViewerCmd>,
    timings: &ViewerTimings,
    backoff: &mut Backoff,
) -> ConnEnd {
    let mut liveness = tokio::time::interval(timings.liveness_tick);
    liveness.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Any inbound frame proves the socket — including the relay's `keepalive`
    // beat, which exists for exactly this (EXP-648).
    let mut last_rx = Instant::now();
    // Set by the frame handler; the socket close that follows is expected.
    let mut pending_end: Option<ConnEnd> = None;
    loop {
        tokio::select! {
            cmd = cmd_rx.recv_async() => match cmd {
                Ok(ViewerCmd::Frames(frames)) => {
                    for frame in frames {
                        if ws.send(Message::Text(frame)).await.is_err() {
                            return ConnEnd::Dropped(Some("send failed".into()));
                        }
                    }
                }
                Ok(ViewerCmd::Kick) => {
                    let _ = ws.close(None).await;
                    return ConnEnd::Kicked;
                }
                Ok(ViewerCmd::Shutdown) | Err(_) => {
                    let _ = ws.close(None).await; // 1000 normal closure
                    return ConnEnd::Shutdown;
                }
            },
            msg = ws.next() => {
                if matches!(msg, Some(Ok(_))) {
                    last_rx = Instant::now();
                }
                match msg {
                    Some(Ok(Message::Text(text))) => match ViewerFrame::parse(&text) {
                        Some(ViewerFrame::Activity { event }) => {
                            // The join was answered ⇒ the room is live. A
                            // long-lived connection also earns a fresh
                            // backoff for whatever comes next.
                            if !phases.is(&ViewerPhase::Live) {
                                phases.set(ViewerPhase::Live);
                                backoff.reset();
                            }
                            let _ = events_tx.send(ViewerEvent::Activity(event));
                        }
                        Some(ViewerFrame::ActivityReset) => {
                            if !phases.is(&ViewerPhase::Live) {
                                phases.set(ViewerPhase::Live);
                                backoff.reset();
                            }
                            let _ = events_tx.send(ViewerEvent::Reset);
                        }
                        Some(ViewerFrame::ActivitySynced) => {
                            let _ = events_tx.send(ViewerEvent::Synced);
                        }
                        Some(ViewerFrame::Keepalive) => {
                            // Counted by the `last_rx` stamp above; never a
                            // phase change. Forwarded because EXP-656 uses it
                            // as an end-of-replay signal.
                            let _ = events_tx.send(ViewerEvent::Keepalive);
                        }
                        Some(ViewerFrame::Bye { outcome }) => {
                            if outcome.as_deref() == Some("publisher_lost") {
                                // The desktop's relay socket dropped, but the
                                // run may still be going — the synced row is
                                // the truth. Stay retryable.
                                pending_end = Some(ConnEnd::Dropped(Some("publisher_lost".into())));
                            } else {
                                let outcome = outcome.filter(|o| o != "ended");
                                pending_end = Some(ConnEnd::Ended(outcome));
                            }
                        }
                        Some(ViewerFrame::Error { code, message }) => {
                            if code == "no_such_session" {
                                // Not live on the relay YET — the desktop may
                                // still be connecting.
                                pending_end = Some(ConnEnd::Starting);
                            } else {
                                pending_end = Some(ConnEnd::Dropped(Some(
                                    message.unwrap_or(code),
                                )));
                            }
                        }
                        // Unknown/publisher-bound frames: ignored, never a
                        // teardown (the relay ships ahead of desktop builds).
                        None => log::debug!("steer viewer: unparseable frame ignored"),
                    },
                    Some(Ok(Message::Close(frame))) => {
                        // A `bye`/`error` already decided what this close
                        // means — the relay always closes right behind them.
                        if let Some(end) = pending_end.take() {
                            return end;
                        }
                        return match close_code(&frame) {
                            Some(CLOSE_UNAUTHORIZED) => ConnEnd::Unauthorized,
                            Some(CLOSE_SLOW_CONSUMER) => ConnEnd::Evicted,
                            Some(CLOSE_SESSION_ENDED) => ConnEnd::Ended(None),
                            code => ConnEnd::Dropped(code.map(|c| format!("close {c}"))),
                        };
                    }
                    Some(Ok(_ping_or_binary)) => {
                        // The relay speaks TEXT only; a pong's whole job here
                        // is the `last_rx` stamp above.
                    }
                    Some(Err(err)) => {
                        return pending_end
                            .take()
                            .unwrap_or(ConnEnd::Dropped(Some(err.to_string())));
                    }
                    None => {
                        return pending_end.take().unwrap_or(ConnEnd::Dropped(None));
                    }
                }
            },
            _ = liveness.tick() => {
                // EXP-648: silence past three relay keepalives means the
                // socket is dead, not that the agent is parked on a question.
                if phases.is(&ViewerPhase::Live) && last_rx.elapsed() > timings.live_stale {
                    let _ = ws.close(None).await;
                    return ConnEnd::Stale;
                }
            }
        }
    }
}

fn close_code(frame: &Option<CloseFrame<'_>>) -> Option<u16> {
    frame.as_ref().map(|f| u16::from(f.code))
}

/// Interruptible delay between dials: `Break` on shutdown, `Continue` on a
/// kick (redial now) or the delay elapsing. Frames sent into a down socket are
/// dropped here — the handle already refused them, so this only catches the
/// race where the socket died between the check and the send.
async fn wait(
    delay: Duration,
    cmd_rx: &flume::Receiver<ViewerCmd>,
    running: &Arc<AtomicBool>,
) -> std::ops::ControlFlow<()> {
    if !running.load(Ordering::SeqCst) {
        return std::ops::ControlFlow::Break(());
    }
    let deadline = tokio::time::Instant::now() + delay;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return std::ops::ControlFlow::Continue(()),
            cmd = cmd_rx.recv_async() => match cmd {
                Ok(ViewerCmd::Shutdown) | Err(_) => {
                    running.store(false, Ordering::SeqCst);
                    return std::ops::ControlFlow::Break(());
                }
                Ok(ViewerCmd::Kick) => return std::ops::ControlFlow::Continue(()),
                Ok(ViewerCmd::Frames(_)) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    // ── Pure frame building ────────────────────────────────────────────────

    #[test]
    fn a_message_is_chunked_text_then_a_separate_return() {
        // EXP-72: the `\r` MUST be its own frame — bundled, the TUI's paste
        // heuristic inserts a newline instead of submitting.
        assert_eq!(
            message_frames("do the thing"),
            vec![
                r#"{"t":"input","data":"do the thing"}"#.to_string(),
                r#"{"t":"input","data":"\r"}"#.to_string(),
            ]
        );
        assert!(message_frames("").is_empty(), "nothing to send, no Enter");
    }

    #[test]
    fn chunking_counts_utf16_units_and_never_splits_a_pair() {
        // ASCII: exactly at the cap is one chunk, one over is two.
        let exact = "a".repeat(INPUT_CHUNK_UTF16);
        assert_eq!(chunk_input(&exact).len(), 1);
        let over = "a".repeat(INPUT_CHUNK_UTF16 + 1);
        let chunks = chunk_input(&over);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chars().count(), INPUT_CHUNK_UTF16);
        assert_eq!(chunks[1], "a");

        // An emoji is TWO UTF-16 units, so the cap is reached at half the
        // char count — and no chunk may end mid-pair.
        let emoji = "🚀".repeat(INPUT_CHUNK_UTF16); // 2x the cap in units
        let chunks = chunk_input(&emoji);
        assert_eq!(chunks.len(), 2);
        for chunk in &chunks {
            let units: usize = chunk.chars().map(char::len_utf16).sum();
            assert!(units <= INPUT_CHUNK_UTF16, "{units} units in one frame");
            assert!(
                chunk.chars().all(|c| c == '🚀'),
                "a chunk boundary never lands inside a character"
            );
        }
        // Everything round-trips: chunking loses nothing.
        assert_eq!(chunks.concat(), emoji);
    }

    #[test]
    fn keystrokes_go_one_frame_each_with_no_trailing_return() {
        // EXP-249: a digit already selects AND advances in claude's picker, so
        // an extra `\r` would answer the NEXT question.
        assert_eq!(
            keystroke_frames(&["1".to_string(), "\t".to_string()]),
            vec![
                r#"{"t":"input","data":"1"}"#.to_string(),
                r#"{"t":"input","data":"\t"}"#.to_string(),
            ]
        );
    }

    // ── Against a fake relay ───────────────────────────────────────────────

    struct FakeTickets {
        url: String,
    }

    impl ViewerTickets for FakeTickets {
        fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
            Ok(Some(MintedTicket {
                ticket: "fake.fake".to_string(),
                url: self.url.clone(),
            }))
        }
    }

    struct DisabledTickets;

    impl ViewerTickets for DisabledTickets {
        fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
            Ok(None)
        }
    }

    /// One accepted viewer socket: what the client sent, and a way to push
    /// frames (or a close) back down.
    struct Conn {
        seen: flume::Receiver<String>,
        inject: flume::Sender<Message>,
    }

    impl Conn {
        fn next_frame(&self) -> String {
            self.seen
                .recv_timeout(Duration::from_secs(5))
                .expect("a frame from the viewer")
        }

        fn send(&self, frame: &str) {
            self.inject.send(Message::Text(frame.to_string())).unwrap();
        }

        fn close(&self, code: u16) {
            self.inject
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::from(code),
                    reason: "".into(),
                })))
                .unwrap();
        }
    }

    /// A fake relay that accepts EVERY connection (the redial tests need the
    /// second and third), handing each one out over `conns`.
    fn fake_relay(runtime: &SteerRuntime) -> (u16, flume::Receiver<Conn>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (conns_tx, conns_rx) = flume::unbounded::<Conn>();
        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            loop {
                let Ok((stream, _addr)) = listener.accept().await else {
                    break;
                };
                let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                    continue;
                };
                let (seen_tx, seen_rx) = flume::unbounded::<String>();
                let (inject_tx, inject_rx) = flume::unbounded::<Message>();
                if conns_tx
                    .send(Conn {
                        seen: seen_rx,
                        inject: inject_tx,
                    })
                    .is_err()
                {
                    break;
                }
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            inject = inject_rx.recv_async() => {
                                let Ok(message) = inject else { break };
                                if ws.send(message).await.is_err() { break; }
                            }
                            msg = ws.next() => match msg {
                                Some(Ok(Message::Text(text))) => { let _ = seen_tx.send(text); }
                                Some(Ok(_)) => {}
                                _ => break,
                            }
                        }
                    }
                });
            }
        });
        (port, conns_rx)
    }

    struct Harness {
        _runtime: Arc<SteerRuntime>,
        handle: ViewerHandle,
        conns: flume::Receiver<Conn>,
        events: flume::Receiver<ViewerEvent>,
    }

    impl Harness {
        fn start(timings: ViewerTimings) -> Self {
            let runtime = SteerRuntime::new().unwrap();
            let (port, conns) = fake_relay(&runtime);
            let (events_tx, events) = flume::unbounded();
            let handle = spawn_viewer_with(
                &runtime,
                Arc::new(FakeTickets {
                    url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
                }),
                "sess-v".to_string(),
                events_tx,
                timings,
            );
            Self {
                _runtime: runtime,
                handle,
                conns,
                events,
            }
        }

        fn fast() -> Self {
            Self::start(ViewerTimings {
                live_stale: Duration::from_millis(250),
                retry_base: Duration::from_millis(10),
                retry_cap: Duration::from_millis(40),
                liveness_tick: Duration::from_millis(25),
            })
        }

        fn next_conn(&self) -> Conn {
            self.conns
                .recv_timeout(Duration::from_secs(5))
                .expect("a viewer connection")
        }

        /// Accept a connection, assert its join, and answer it so the phase
        /// goes live.
        fn go_live(&self) -> Conn {
            let conn = self.next_conn();
            assert_eq!(conn.next_frame(), r#"{"t":"join","channel":"activity"}"#);
            conn.send(r#"{"t":"activity_reset"}"#);
            self.wait_for_phase(&ViewerPhase::Live);
            // Drain the join answer's own event too, so a test that waits for
            // the NEXT `Reset` (a redial's replay) isn't fooled by this one.
            assert_eq!(self.next_event(), ViewerEvent::Reset);
            conn
        }

        fn next_event(&self) -> ViewerEvent {
            self.events
                .recv_timeout(Duration::from_secs(5))
                .expect("a viewer event")
        }

        /// Drain until the given phase arrives, returning everything seen on
        /// the way (so a test can assert what did NOT happen).
        fn wait_for_phase(&self, phase: &ViewerPhase) -> Vec<ViewerEvent> {
            let mut seen = Vec::new();
            loop {
                let event = self.next_event();
                let hit = matches!(&event, ViewerEvent::Phase(p) if p == phase);
                seen.push(event);
                if hit {
                    return seen;
                }
            }
        }
    }

    fn phases(events: &[ViewerEvent]) -> Vec<&ViewerPhase> {
        events
            .iter()
            .filter_map(|event| match event {
                ViewerEvent::Phase(phase) => Some(phase),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn joins_the_activity_channel_and_goes_live_on_the_first_frame() {
        let harness = Harness::fast();
        assert_eq!(
            harness.next_event(),
            ViewerEvent::Phase(ViewerPhase::Connecting)
        );
        let conn = harness.next_conn();
        // The relay's zod REQUIRES the channel literal.
        assert_eq!(conn.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        assert_eq!(harness.next_event(), ViewerEvent::Connected(true));

        conn.send(r#"{"t":"activity_reset"}"#);
        assert_eq!(harness.next_event(), ViewerEvent::Phase(ViewerPhase::Live));
        assert_eq!(harness.next_event(), ViewerEvent::Reset);

        conn.send(r#"{"t":"activity","event":{"kind":"narration","text":"working"}}"#);
        assert_eq!(
            harness.next_event(),
            ViewerEvent::Activity(ActivityEvent::narration("working"))
        );
        conn.send(r#"{"t":"activity_synced"}"#);
        assert_eq!(harness.next_event(), ViewerEvent::Synced);
        // EXP-648: a keepalive is forwarded but never a phase change.
        conn.send(r#"{"t":"keepalive"}"#);
        assert_eq!(harness.next_event(), ViewerEvent::Keepalive);
        harness.handle.shutdown();
    }

    #[test]
    fn a_sent_message_chunks_ends_with_a_lone_return_and_echoes_locally() {
        let harness = Harness::fast();
        let conn = harness.go_live();
        assert!(harness.handle.send_message("do the thing"));
        assert_eq!(conn.next_frame(), r#"{"t":"input","data":"do the thing"}"#);
        assert_eq!(conn.next_frame(), r#"{"t":"input","data":"\r"}"#);
        // The local echo rides the SAME ordered channel as inbound frames.
        let mut saw_echo = false;
        for _ in 0..8 {
            if harness.next_event() == ViewerEvent::LocalMessage("do the thing".into()) {
                saw_echo = true;
                break;
            }
        }
        assert!(saw_echo, "the sent message is echoed to the feed");
        harness.handle.shutdown();
    }

    #[test]
    fn answers_and_keystrokes_reach_the_relay_in_their_wire_shapes() {
        let harness = Harness::fast();
        let conn = harness.go_live();
        assert!(harness.handle.send_answer(
            "toolu_01#0",
            Some("toolu_01"),
            &["1".to_string()],
            Some("purple"),
        ));
        assert_eq!(
            conn.next_frame(),
            r#"{"t":"answer","questionId":"toolu_01#0","askId":"toolu_01","keys":["1"],"text":"purple"}"#
        );
        // The legacy path: raw keystrokes, no trailing Enter.
        assert!(harness
            .handle
            .send_keystrokes(&["2".to_string(), "\t".to_string()]));
        assert_eq!(conn.next_frame(), r#"{"t":"input","data":"2"}"#);
        assert_eq!(conn.next_frame(), r#"{"t":"input","data":"\t"}"#);
        harness.handle.shutdown();
    }

    #[test]
    fn sending_into_a_down_socket_reports_false() {
        let harness = Harness::fast();
        // Nothing is connected yet — the caller keeps its draft.
        assert!(!harness.handle.send_message("too early"));
        assert!(!harness.handle.send_keystrokes(&["1".to_string()]));
        assert!(!harness.handle.send_answer("q", None, &["1".to_string()], None));
        harness.handle.shutdown();
    }

    #[test]
    fn a_slow_consumer_eviction_redials_without_a_phase_change() {
        let harness = Harness::fast();
        let conn = harness.go_live();
        conn.close(CLOSE_SLOW_CONSUMER);

        // A second socket arrives and joins…
        let next = harness.next_conn();
        assert_eq!(next.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        next.send(r#"{"t":"activity_reset"}"#);

        // …and NOTHING in between claimed the session was reconnecting or
        // over: an eviction is not an ending.
        let mut seen = Vec::new();
        loop {
            let event = harness.next_event();
            let done = event == ViewerEvent::Reset;
            seen.push(event);
            if done {
                break;
            }
        }
        assert!(
            phases(&seen).is_empty(),
            "the live phase held across the eviction: {:?}",
            phases(&seen)
        );
        assert!(
            seen.contains(&ViewerEvent::Connected(false))
                && seen.contains(&ViewerEvent::Connected(true)),
            "but `connected` dipped honestly for the gap"
        );
        harness.handle.shutdown();
    }

    #[test]
    fn bye_publisher_lost_retries_while_any_other_outcome_ends() {
        // The desktop's socket dropped; the run may still be going.
        let harness = Harness::fast();
        let conn = harness.go_live();
        conn.send(r#"{"t":"bye","outcome":"publisher_lost"}"#);
        conn.close(CLOSE_SESSION_ENDED);
        harness.wait_for_phase(&ViewerPhase::Reconnecting);
        let next = harness.next_conn();
        assert_eq!(next.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        harness.handle.shutdown();

        // A real ending stops the loop for good, outcome and all.
        let harness = Harness::fast();
        let conn = harness.go_live();
        conn.send(r#"{"t":"bye","outcome":"exit:0"}"#);
        conn.close(CLOSE_SESSION_ENDED);
        harness.wait_for_phase(&ViewerPhase::Ended {
            outcome: Some("exit:0".to_string()),
        });
        assert!(!harness.handle.is_active());
        assert!(
            harness.conns.recv_timeout(Duration::from_millis(300)).is_err(),
            "an ended room is never redialed"
        );
    }

    #[test]
    fn no_such_session_parks_in_starting_and_keeps_redialing() {
        let harness = Harness::fast();
        let conn = harness.next_conn();
        assert_eq!(conn.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        // The desktop has not `hello`'d yet.
        conn.send(r#"{"t":"error","code":"no_such_session"}"#);
        conn.close(CLOSE_SESSION_ENDED);
        harness.wait_for_phase(&ViewerPhase::Starting);

        // It keeps trying — the publisher may arrive at any moment.
        let next = harness.next_conn();
        assert_eq!(next.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        next.send(r#"{"t":"activity_reset"}"#);
        harness.wait_for_phase(&ViewerPhase::Live);
        harness.handle.shutdown();
    }

    #[test]
    fn a_run_the_synced_row_calls_ended_stops_the_starting_redial() {
        // EXP-639: an ended run has no publisher left, so redialing can only
        // draw `no_such_session` forever.
        let harness = Harness::fast();
        let conn = harness.next_conn();
        conn.next_frame();
        harness.handle.note_session_ended();
        conn.send(r#"{"t":"error","code":"no_such_session"}"#);
        conn.close(CLOSE_SESSION_ENDED);
        harness.wait_for_phase(&ViewerPhase::Ended { outcome: None });
        assert!(!harness.handle.is_active());
    }

    #[test]
    fn a_live_socket_that_goes_silent_is_redialed_silently() {
        let harness = Harness::fast(); // live_stale = 250ms
        // Held open and never spoken through again: the socket stays
        // ESTABLISHED (no close frame, no error), which is exactly the
        // OS-killed-connection case the keepalive window exists for.
        let _mute = harness.go_live();

        let next = harness.next_conn();
        assert_eq!(next.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        next.send(r#"{"t":"activity_reset"}"#);
        let mut seen = Vec::new();
        loop {
            let event = harness.next_event();
            let done = event == ViewerEvent::Reset;
            seen.push(event);
            if done {
                break;
            }
        }
        // The phase held at `live` throughout — a socket that turns out fine
        // must never flash "Reconnecting".
        assert!(
            phases(&seen).is_empty(),
            "silent redial, no phase flap: {:?}",
            phases(&seen)
        );
        harness.handle.shutdown();
    }

    #[test]
    fn an_unauthorized_close_is_terminal() {
        let harness = Harness::fast();
        let conn = harness.go_live();
        conn.close(CLOSE_UNAUTHORIZED);
        let seen = harness.wait_for_phase(&ViewerPhase::Unauthorized {
            detail: Some("The relay refused this viewer ticket.".into()),
        });
        assert!(phases(&seen).last().unwrap().is_terminal());
        assert!(!harness.handle.is_active());
        assert!(
            harness.conns.recv_timeout(Duration::from_millis(300)).is_err(),
            "a refused ticket mints the same no forever"
        );
    }

    #[test]
    fn a_disabled_instance_stops_before_it_ever_dials() {
        let runtime = SteerRuntime::new().unwrap();
        let (events_tx, events) = flume::unbounded();
        let handle = spawn_viewer(
            &runtime,
            Arc::new(DisabledTickets),
            "sess-v".to_string(),
            events_tx,
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap(),
            ViewerEvent::Phase(ViewerPhase::Connecting)
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap(),
            ViewerEvent::Phase(ViewerPhase::Unauthorized {
                detail: Some("Live steering is unavailable on this instance.".into()),
            })
        );
        assert!(!handle.is_active());
    }

    #[test]
    fn a_kick_cuts_short_the_backoff_and_redials_now() {
        let harness = Harness::start(ViewerTimings {
            live_stale: Duration::from_secs(45),
            // A backoff long enough that only the kick can explain a prompt
            // redial.
            retry_base: Duration::from_secs(20),
            retry_cap: Duration::from_secs(30),
            liveness_tick: Duration::from_secs(5),
        });
        let conn = harness.next_conn();
        conn.next_frame();
        conn.send(r#"{"t":"error","code":"no_such_session"}"#);
        conn.close(CLOSE_SESSION_ENDED);
        harness.wait_for_phase(&ViewerPhase::Starting);

        harness.handle.kick();
        let next = harness
            .conns
            .recv_timeout(Duration::from_secs(5))
            .expect("the kick redialed instead of waiting out the backoff");
        assert_eq!(next.next_frame(), r#"{"t":"join","channel":"activity"}"#);
        harness.handle.shutdown();
    }

    #[test]
    fn shutdown_closes_the_socket_and_stops_the_loop() {
        let harness = Harness::fast();
        let _conn = harness.go_live();
        harness.handle.shutdown();
        assert!(!harness.handle.is_active());
        assert!(
            harness.conns.recv_timeout(Duration::from_millis(300)).is_err(),
            "a shut-down viewer never redials"
        );
        assert!(!harness.handle.send_message("too late"));
    }
}
