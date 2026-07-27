//! The per-coding-session publisher (masterplan-v3 §8.4–§8.7, steering v2 =
//! EXP-249): publish the scrubbed activity stream, replay the session's full
//! journal on every (re)connect, inject remote `input` and semantic `answer`
//! frames, honor claim/kill, and auto-reconnect resuming the room.
//!
//! EXP-249 removed the binary PTY mirror it used to carry (no client ever
//! joined `channel:'pty'`): there is no read-loop tee, no ring, no `resync`
//! and no geometry here anymore. What remains on the socket is TEXT only.
//!
//! **Best-effort and non-blocking** (§8.4): if the relay is disabled or
//! unreachable the coding session runs fine locally — the publisher never
//! gates the terminal. Control frames and activity events ride ONE unbounded
//! channel that is never dropped or reordered.
//!
//! Claim model (§8.5): the LOCAL user is never gated — their keystrokes go
//! straight to the PTY. "Take over" simply sends `claim`; the relay's
//! publisher-branch (`hub.ts`: `if (room.publisher === conn)
//! publisherTakeover(room)`) force-clears the remote steerer and
//! re-broadcasts presence.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use api::error::ApiError;
use api::steer::MintedTicket;
use api::trpc::TrpcClient;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use crate::activity::{AnswerLink, RemoteAnswer};
use crate::frames::{
    ActivityEvent, ClientFrame, PresenceViewer, ServerFrame, CLOSE_REPLACED, CLOSE_UNAUTHORIZED,
};
use crate::journal::ActivityJournal;
use crate::{dial, Backoff, DialError, SteerRuntime, WsStream, BACKOFF_RESET_AFTER};

/// The relay's WebSocket `maxPayloadLength` (bytes). A text frame at or past
/// this makes the relay sever the connection — killing the whole activity
/// stream — so oversize activity frames are dropped client-side instead.
const RELAY_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// §8.7: surfaced after two consecutive fresh-ticket 401s (never silently
/// retry a skewed clock — the native failure mode this fixes).
const CLOCK_SKEW_ERROR: &str = "Steer relay rejected the connection (ticket expired on \
     arrival) — check that this machine's clock is in sync (NTP).";

// ---------------------------------------------------------------------------
// Spec, hooks, tickets
// ---------------------------------------------------------------------------

/// What to publish (§8.4 handshake): the `coding_sessions` row id keys the
/// relay room; the issue id rides along for the phone's session list.
#[derive(Clone, Debug)]
pub struct PublishSpec {
    pub session_id: String,
    pub issue_id: Option<String>,
}

/// Publisher-ticket source, injectable for tests. Blocking (reqwest) — the loop
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
///
/// EXP-283: an explicit relay `kill` frame is the ONLY relay signal that may
/// end the local session. A relay-initiated socket CLOSE — whatever its code —
/// is transport-level and must never kill live local work: the relay's
/// idle-publisher detector closes quiet sockets with `CLOSE_SESSION_ENDED`
/// (a laptop sleeping >90s reads that close on wake), and treating it as a
/// kill was tearing down live agents mid-session. Real session ends always
/// also flip the synced `coding_sessions` row, which the §8.8 kill-watch
/// handles durably.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillSignal {
    /// A relay `kill` frame (steer.killSession fan-out / a steerer's kill).
    RemoteKill,
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
    /// Relay-initiated teardown: kill the `claude` child; the exit hook then
    /// ends the `coding_sessions` row (idempotent server-side).
    pub kill: Arc<dyn Fn(KillSignal) + Send + Sync>,
    /// Presence updates → the §8.5 banner state.
    pub presence: Arc<dyn Fn(Presence) + Send + Sync>,
    /// Terminal-state errors worth surfacing (clock skew, repeated rejects).
    pub error: Arc<dyn Fn(String) + Send + Sync>,
    /// EXP-249: semantic `answer` frames → the activity emitter, which owns
    /// the live question state and the TUI key choreography. `None` (no
    /// emitter) makes `answer` a no-op — steerers on such a session fall back
    /// to the legacy keystroke path.
    pub answers: Option<Arc<AnswerLink>>,
}

/// Keystroke frames (`\r` submit, `\x1b` interrupt / CSI sequences, any lone
/// byte — EXP-78: a single printable char is a picker answer, e.g. a digit
/// selecting an AskUserQuestion/plan-approval option, and must land raw or the
/// TUI sees a paste instead of a keypress) must land raw; anything else is
/// message TEXT from a steerer's composer and gets local-paste treatment
/// (bracketed, EXP-72 — a one-char message landing unbracketed is
/// indistinguishable from typing it, harmless).
fn is_keystroke(bytes: &[u8]) -> bool {
    bytes.len() == 1 || bytes.first() == Some(&0x1b)
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

// ---------------------------------------------------------------------------
// Handle + commands
// ---------------------------------------------------------------------------

/// Control-path commands (§8.4 backpressure rule: these ride an UNBOUNDED
/// channel — never dropped, never reordered against each other).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PublisherCmd {
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
    cmd_tx: flume::Sender<PublisherCmd>,
    running: Arc<AtomicBool>,
}

/// A cheap `Send + Sync` clone of the publisher's control sender, dedicated to
/// §P7 activity events (the member-only activity channel). The activity
/// emitter thread holds one and pushes already-redacted narration/tool/diff
/// events; sending is fire-and-forget and never blocks the emitter (unbounded
/// flume). Sending after the publisher has
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

    /// Test-only pair: a sender plus the receiving end to assert on what the
    /// activity emitter actually published.
    #[cfg(test)]
    pub(crate) fn test_pair() -> (Self, flume::Receiver<PublisherCmd>) {
        let (cmd_tx, rx) = flume::unbounded();
        (Self { cmd_tx }, rx)
    }
}

impl PublisherHandle {
    /// The §8.5 "Take over" button: revoke the remote steerer.
    pub fn take_over(&self) {
        let _ = self.cmd_tx.send(PublisherCmd::TakeOver);
    }

    /// A cheap [`ActivitySender`] for the §P7 activity emitter thread — pushes
    /// activity events onto the same unbounded control channel without
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

    /// False once the publisher stopped for good (clean end, kill frame,
    /// 4002 replaced, skew give-up).
    pub fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Start publishing a coding session (§8.4). Non-blocking: spawns the task
/// onto the steer runtime and returns the handle immediately. Wire-up
/// contract (the coding-flow seam):
///
/// 1. `spawn_activity_emitter` with `handle.activity_sender()` — the events
///    this task journals and republishes;
/// 2. exit hook → `handle.shutdown(Some(format!("exit:{code}")))`;
/// 3. `sync::kill_watch` on_ended → `handle.session_ended()` (after killing
///    the child).
pub fn publish(
    runtime: &SteerRuntime,
    spec: PublishSpec,
    tickets: Arc<dyn PublisherTickets>,
    hooks: PublisherHooks,
) -> PublisherHandle {
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let running = Arc::new(AtomicBool::new(true));
    let handle = PublisherHandle {
        cmd_tx,
        running: running.clone(),
    };
    runtime
        .handle()
        .spawn(run_publisher_loop(spec, tickets, hooks, cmd_rx, running));
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
    cmd_rx: flume::Receiver<PublisherCmd>,
    running: Arc<AtomicBool>,
) {
    // EXP-249: the session's full published history. Every connection starts
    // with `activity_reset` + this journal, so a viewer joining a resumed room
    // (or after a relay restart) sees the session from its first event.
    let mut journal = ActivityJournal::new();
    let mut backoff = Backoff::publisher();
    // §8.7: one immediate re-mint is allowed after a fresh-ticket 401; a
    // second consecutive 401 surfaces the clock-skew error and stops.
    let mut unauthorized_once = false;

    'reconnect: while running.load(Ordering::SeqCst) {
        // Mint (blocking reqwest off the reactor), then dial IMMEDIATELY (§8.7).
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
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut journal, &running).await.is_break() {
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
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut journal, &running).await.is_break() {
                    return;
                }
                continue 'reconnect;
            }
        };
        unauthorized_once = false;

        // §8.4 #2 — hello creates the room (or resumes it on re-hello,
        // evicting any stale publisher with CLOSE_REPLACED).
        let hello = ClientFrame::Hello {
            session_id: &spec.session_id,
            issue_id: spec.issue_id.as_deref(),
            // EXP-90: the anonymous public-activity audience is removed —
            // always send the explicit opt-out, because an ABSENT key means
            // "public" to legacy relays (manual, independent deploys).
            activity_public: Some(false),
        }
        .to_json();
        if let Err(err) = ws.send(Message::Text(hello)).await {
            log::debug!("steer publisher: hello failed: {err}");
            if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut journal, &running).await.is_break() {
                return;
            }
            continue 'reconnect;
        }
        log::info!("steer publisher: room {} live", spec.session_id);
        let established = Instant::now();

        // EXP-249 full-history re-publish: clear whatever the room (and its
        // viewers) still hold, then replay the journal in order. A resumed
        // `--continue` transcript seeds the journal from byte 0, so this is
        // the ONE deliberate place a feed is rebuilt.
        if !republish_history(&mut ws, &journal).await {
            log::debug!("steer publisher: history replay failed; reconnecting");
            if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut journal, &running).await.is_break() {
                return;
            }
            continue 'reconnect;
        }

        let end = pump_connection(&mut ws, &hooks, &cmd_rx, &mut journal, &running).await;

        match end {
            LoopEnd::Clean => {
                running.store(false, Ordering::SeqCst);
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
                // Any other close or unexpected drop: reconnect while the
                // session is still running, resuming the same room (§8.6).
                // That INCLUDES 4001 (EXP-283): the relay's idle-publisher
                // detector closes quiet sockets with CLOSE_SESSION_ENDED —
                // e.g. read on wake after a laptop sleep >90s — and a relay
                // close must never kill live local work. Real kills arrive as
                // the explicit `kill` frame or the §8.8 own-row Electric flip;
                // re-hello resets the relay's idle timer and resumes the room.
                // Reconnect promptly — the relay's staleTimer bounds the
                // grace window.
                if established.elapsed() >= BACKOFF_RESET_AFTER {
                    backoff.reset();
                }
                if !running.load(Ordering::SeqCst) {
                    return;
                }
                log::debug!("steer publisher: dropped; reconnecting");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut journal, &running).await.is_break() {
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
    cmd_rx: &flume::Receiver<PublisherCmd>,
    journal: &mut ActivityJournal,
    running: &Arc<AtomicBool>,
) -> LoopEnd {
    // EXP-72: when a steerer's Enter (a bare `\r` frame) chases their message
    // text this closely, the child can read text+`\r` as ONE chunk and the
    // `claude` TUI's paste heuristic inserts a newline instead of submitting.
    // Hold the `\r` back until the child has had a beat to drain the text.
    // Ordering is safe — this task is the only remote-input writer.
    const ENTER_SEPARATION: Duration = Duration::from_millis(150);
    // EXP-249 belt-and-braces: a pre-v2 client answers a picker by sending the
    // option digit and then a bare `\r`. The digit ALONE already submits (and
    // auto-advances a multi-question ask), so the trailing Enter would answer
    // the NEXT question with whatever it is sitting on. Swallow it while an
    // ask is pending.
    const ENTER_CASCADE_WINDOW: Duration = Duration::from_millis(500);
    // REV2-X: Send periodic pings so the relay can detect dead publishers.
    // During plan mode (or any idle period), the desktop sends no activity,
    // and without pings, a dropped connection can sit undetected while UIs retry
    // endlessly against a stale `no_such_session`.
    const PING_INTERVAL: Duration = Duration::from_secs(30);
    let mut last_input_at: Option<Instant> = None;
    let mut last_digit_at: Option<Instant> = None;
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            // 1) local control commands (unbounded — never dropped).
            cmd = cmd_rx.recv_async() => {
                let Ok(cmd) = cmd else { return LoopEnd::Dropped };
                match cmd {
                    PublisherCmd::TakeOver => {
                        // §8.5: publisher-sent claim = relay publisherTakeover.
                        if ws.send(Message::Text(ClientFrame::Claim.to_json())).await.is_err() {
                            return LoopEnd::Dropped;
                        }
                    }
                    PublisherCmd::Activity(mut event) => {
                        // §P7: publish one already-redacted activity event.
                        // The relay fans it to the member activity audience.
                        // Stamp it first: the journal replays the ORIGINAL
                        // timeline after a reconnect.
                        if event.at_mut().is_none() {
                            *event.at_mut() = Some(now_millis());
                        }
                        journal.push(event.clone());
                        if !send_activity(ws, event).await {
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
            // 2) relay → publisher control frames.
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => match ServerFrame::parse(&text) {
                    Some(ServerFrame::Input { data }) => {
                        let ask_pending = hooks
                            .answers
                            .as_ref()
                            .is_some_and(|answers| answers.ask_pending());
                        if data == "\r" {
                            let cascade = ask_pending
                                && last_digit_at
                                    .is_some_and(|at| at.elapsed() < ENTER_CASCADE_WINDOW);
                            if cascade {
                                last_digit_at = None;
                                continue;
                            }
                            if let Some(at) = last_input_at {
                                let elapsed = at.elapsed();
                                if elapsed < ENTER_SEPARATION {
                                    tokio::time::sleep(ENTER_SEPARATION - elapsed).await;
                                }
                            }
                        }
                        if data.len() == 1 && data.as_bytes()[0].is_ascii_digit() {
                            last_digit_at = Some(Instant::now());
                        }
                        last_input_at = Some(Instant::now());
                        (hooks.write_input)(data.as_bytes())
                    }
                    // EXP-249: the semantic answer path — the emitter owns
                    // question identity + the TUI key choreography, so the
                    // publisher only routes.
                    Some(ServerFrame::Answer { question_id, ask_id, keys }) => {
                        if let Some(answers) = &hooks.answers {
                            answers.submit(RemoteAnswer { question_id, ask_id, keys });
                        }
                    }
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
                    // The relay speaks TEXT only since EXP-249; pings are
                    // answered by tungstenite internally.
                }
                Some(Err(err)) => {
                    log::debug!("steer publisher: socket error: {err}");
                    return LoopEnd::Dropped;
                }
                None => return LoopEnd::Dropped,
            },
            // 3) periodic ping to keep the connection alive and let the relay
            // detect dead publishers (REV2-X: plan mode can idle for minutes).
            _ = ping_interval.tick() => {
                if ws.send(Message::Ping(vec![])).await.is_err() {
                    return LoopEnd::Dropped;
                }
            }
        }
    }
}

/// `activity_reset` + the whole journal, in order. `false` on a socket error
/// (the caller reconnects and tries again from the top).
async fn republish_history(ws: &mut WsStream, journal: &ActivityJournal) -> bool {
    if ws
        .send(Message::Text(ClientFrame::ActivityReset.to_json()))
        .await
        .is_err()
    {
        return false;
    }
    for event in journal.replay() {
        if !send_activity(ws, event.clone()).await {
            return false;
        }
    }
    true
}

/// Send one activity event. The emitter caps event strings in UTF-8 bytes, but
/// JSON escaping can still inflate pathological content past the relay's frame
/// limit — dropping the event beats letting the relay close the socket, so an
/// oversize frame is a skip, not a failure.
async fn send_activity(ws: &mut WsStream, event: ActivityEvent) -> bool {
    let frame = ClientFrame::Activity { event }.to_json();
    if frame.len() >= RELAY_MAX_PAYLOAD_BYTES {
        log::warn!(
            "steer publisher: dropping oversize activity frame ({} bytes)",
            frame.len()
        );
        return true;
    }
    ws.send(Message::Text(frame)).await.is_ok()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0)
}

fn close_code(frame: &Option<CloseFrame<'_>>) -> Option<u16> {
    frame.as_ref().map(|f| u16::from(f.code))
}

/// Interruptible backoff sleep: `Break` on `Shutdown` (we're disconnected —
/// nothing to `bye`) or when `running` flipped. Activity published while the
/// socket is down still goes into the journal, so the reconnect's re-publish
/// carries it; a take-over is momentary UI state, safely superseded by the
/// re-`hello`.
async fn sleep_or_shutdown(
    delay: Duration,
    cmd_rx: &flume::Receiver<PublisherCmd>,
    journal: &mut ActivityJournal,
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
                Ok(PublisherCmd::Activity(mut event)) => {
                    if event.at_mut().is_none() {
                        *event.at_mut() = Some(now_millis());
                    }
                    journal.push(event);
                }
                Ok(PublisherCmd::TakeOver) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ── Control path (§8.4): never dropped, never reordered ────────────────

    #[test]
    fn control_channel_is_unbounded_and_lossless() {
        // §8.4: control frames are NEVER dropped — activity/kill/bye ride an
        // unbounded path however chatty the session gets.
        let (cmd_tx, cmd_rx) = flume::unbounded();
        for i in 0..10_000u16 {
            cmd_tx
                .send(PublisherCmd::Activity(ActivityEvent::narration(format!(
                    "line {i}"
                ))))
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
        let (cmd_tx, cmd_rx) = flume::unbounded();
        let handle = PublisherHandle {
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
        hook(b"1"); // EXP-78: picker answer digit — a keypress, not a paste
        assert_eq!(recorded.lock().unwrap().as_slice(), b"\r\x1b\x1b[A1");
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
        kills: Mutex<Vec<KillSignal>>,
        presences: Mutex<Vec<Presence>>,
        errors: Mutex<Vec<String>>,
    }

    fn recording_hooks(recorded: Arc<Recorded>) -> PublisherHooks {
        recording_hooks_with(recorded, None)
    }

    fn recording_hooks_with(
        recorded: Arc<Recorded>,
        answers: Option<Arc<AnswerLink>>,
    ) -> PublisherHooks {
        let r1 = recorded.clone();
        let r2 = recorded.clone();
        let r3 = recorded.clone();
        let r4 = recorded;
        PublisherHooks {
            write_input: Arc::new(move |bytes| {
                r1.inputs.lock().unwrap().push(bytes.to_vec());
            }),
            kill: Arc::new(move |signal| {
                r2.kills.lock().unwrap().push(signal);
            }),
            presence: Arc::new(move |presence| {
                r3.presences.lock().unwrap().push(presence);
            }),
            error: Arc::new(move |message| {
                r4.errors.lock().unwrap().push(message);
            }),
            answers,
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

    /// A fake relay socket: everything the publisher sends lands on `seen`,
    /// everything the test injects goes back down the wire.
    fn fake_relay(
        runtime: &SteerRuntime,
    ) -> (u16, flume::Receiver<String>, flume::Sender<Message>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (seen_tx, seen_rx) = flume::unbounded::<String>();
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
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
        });
        (port, seen_rx, inject_tx)
    }

    #[test]
    fn publisher_hellos_publishes_activity_steers_and_kills_against_a_fake_relay() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);

        let recorded = Arc::new(Recorded::default());
        let (link, answers_rx) = AnswerLink::new();
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-t".to_string(),
                issue_id: Some("issue-t".to_string()),
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks_with(recorded.clone(), Some(link.clone())),
        );

        // 1) hello — no geometry since EXP-249. EXP-90: every hello carries
        // the explicit activityPublic:false.
        let hello = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            hello,
            r#"{"t":"hello","sessionId":"sess-t","issueId":"issue-t","activityPublic":false}"#
        );
        // 2) …immediately followed by the (empty) history re-publish.
        let reset = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(reset, r#"{"t":"activity_reset"}"#);

        // 3) activity events go out stamped and land in the journal.
        let sender = handle.activity_sender();
        sender.send(ActivityEvent::narration("Session started"));
        let narration = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            narration.starts_with(r#"{"t":"activity","event":{"kind":"narration","text":"Session started","at":"#),
            "{narration}"
        );

        // 4) remote input → the PTY-writer hook, byte-identical.
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"ls\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(recorded.inputs.lock().unwrap()[0], b"ls\r");

        // 5) a semantic answer routes to the emitter's link, NOT the PTY.
        inject_tx
            .send(Message::Text(
                r#"{"t":"answer","questionId":"toolu_1#0","askId":"toolu_1","keys":["2"]}"#
                    .to_string(),
            ))
            .unwrap();
        let answer = answers_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            answer,
            RemoteAnswer {
                question_id: "toolu_1#0".to_string(),
                ask_id: Some("toolu_1".to_string()),
                keys: vec!["2".to_string()],
            }
        );
        assert_eq!(recorded.inputs.lock().unwrap().len(), 1, "answers never keystroke");

        // 6) presence → banner hook.
        inject_tx
            .send(Message::Text(
                r#"{"t":"presence","viewers":[{"userId":"v1","name":"Phone","perm":"steer"}],"steererId":"v1"}"#.to_string(),
            ))
            .unwrap();
        wait_for(|| !recorded.presences.lock().unwrap().is_empty());
        let presence = recorded.presences.lock().unwrap()[0].clone();
        assert_eq!(presence.steerer_id.as_deref(), Some("v1"));
        assert_eq!(presence.viewers[0].name, "Phone");

        // 7) take over → a publisher `claim`.
        handle.take_over();
        let claim = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(claim, r#"{"t":"claim"}"#);

        // 8) relay kill → kill hook fires, clean bye goes out, task stops.
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
    fn a_legacy_enter_cascade_is_dropped_while_an_ask_is_pending() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let (link, _answers_rx) = AnswerLink::new();
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-e".to_string(),
                issue_id: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks_with(recorded.clone(), Some(link.clone())),
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        // A pre-v2 client answering a picker: digit, then a bare Enter. The
        // digit already submitted, so the Enter must not reach the PTY.
        link.set_ask_pending(true);
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"2"}"#.to_string()))
            .unwrap();
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(
            recorded.inputs.lock().unwrap().as_slice(),
            &[b"2".to_vec()],
            "the chased Enter is swallowed"
        );

        // With no ask pending, an Enter is ordinary steering input again.
        link.set_ask_pending(false);
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"\r"}"#.to_string()))
            .unwrap();
        wait_for(|| recorded.inputs.lock().unwrap().len() == 2);
        assert_eq!(recorded.inputs.lock().unwrap()[1], b"\r");
        handle.shutdown(None);
    }

    #[test]
    fn a_reconnect_resets_and_replays_the_journal() {
        let runtime = SteerRuntime::new().unwrap();
        // Two sequential relay sockets on ONE port: the first is dropped after
        // the publisher hellos, forcing the reconnect path.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (seen_tx, seen_rx) = flume::unbounded::<(usize, String)>();
        let (drop_tx, drop_rx) = flume::unbounded::<()>();
        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            for connection in 0..2usize {
                let Ok((stream, _addr)) = listener.accept().await else { return };
                let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else { return };
                loop {
                    tokio::select! {
                        _ = drop_rx.recv_async() => break, // sever this socket
                        msg = ws.next() => match msg {
                            Some(Ok(Message::Text(text))) => {
                                let _ = seen_tx.send((connection, text));
                            }
                            Some(Ok(_)) => {}
                            _ => break,
                        }
                    }
                }
                drop(ws);
            }
        });

        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-j".to_string(),
                issue_id: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded.clone()),
        );
        assert!(seen_rx.recv_timeout(Duration::from_secs(5)).unwrap().1.contains("hello"));
        assert_eq!(seen_rx.recv_timeout(Duration::from_secs(5)).unwrap().1, r#"{"t":"activity_reset"}"#);

        let sender = handle.activity_sender();
        sender.send(ActivityEvent::narration("first"));
        sender.send(ActivityEvent::tool("Edit", Some("a.rs".into())));
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // Sever the socket; the publisher re-mints, re-hellos, and rebuilds
        // the whole feed on the fresh connection.
        drop_tx.send(()).unwrap();
        let mut second: Vec<String> = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while second.len() < 4 {
            assert!(Instant::now() < deadline, "no replay after reconnect: {second:?}");
            if let Ok((connection, text)) = seen_rx.recv_timeout(Duration::from_millis(250)) {
                if connection == 1 {
                    second.push(text);
                }
            }
        }
        assert!(second[0].contains(r#""t":"hello""#), "{:?}", second[0]);
        assert_eq!(second[1], r#"{"t":"activity_reset"}"#);
        assert!(second[2].contains(r#""text":"first""#), "{:?}", second[2]);
        assert!(second[3].contains(r#""name":"Edit""#), "{:?}", second[3]);
        handle.shutdown(None);
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
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded),
        );
        let hello = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            hello,
            r#"{"t":"hello","sessionId":"sess-x","activityPublic":false}"#
        );
        assert_eq!(
            seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            r#"{"t":"activity_reset"}"#
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
            },
            Arc::new(DisabledTickets),
            recording_hooks(recorded.clone()),
        );
        wait_for(|| !handle.is_active());
        assert!(recorded.errors.lock().unwrap().is_empty(), "EXP-4: no noise");
    }
}
