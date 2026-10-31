//! The steering VIEWER connection (EXP-696) — watch and steer a coding
//! session running on ANOTHER of the user's devices.
//!
//! [`publisher`] serves a session this machine is hosting; this module is
//! the other end of that wire, what web/iOS/Android have had since EXP-249.
//! VAPP-43: it talks to the session's DEVICE over eNet's chat channel and
//! nothing else (`fixtures/steer-enet.json`): the endpoints
//! `installs.forSession` names, newest first, each sent a `join` until one
//! answers with its replay up to `activity_synced` within
//! [`enet_steer::JOIN_TIMEOUT`]. It turns everything that arrives into
//! [`ViewerEvent`]s on a channel, and sends steering back up: whole composer
//! messages and semantic answers (EXP-730: a question is answered ONLY by
//! its wire id).
//!
//! [`publisher`]: crate::publisher
//! [`enet_steer::JOIN_TIMEOUT`]: crate::enet_steer::JOIN_TIMEOUT
//!
//! **The UI is a dumb reducer.** Nothing here interprets a feed: the task
//! emits phases and frames, the caller pumps them into a [`SteerFeed`] and
//! renders it.
//!
//! [`SteerFeed`]: crate::feed::SteerFeed
//!
//! ## The reconnect rules
//!
//! | signal | meaning | what happens |
//! |---|---|---|
//! | NO FEED: no node, no endpoint, every endpoint refused, or no `activity_synced` in time | the device is not reachable (or not running the session yet) | a live row: [`ViewerPhase::Starting`] + jittered 3s→30s redial; an ended row: [`ViewerPhase::Ended`] `device_offline`, NOT terminal (VAPP-169): a silent redial on a kick (presence online, a wakeup) or every [`enet_steer::STALE`] |
//! | `bye` | the run (or a stored replay, `history`) is finished | [`ViewerPhase::Ended`], stop |
//! | `error` after the join | the device stopped serving the session | treated as NO FEED |
//! | [`enet_steer::STALE`] of silence while live | the feed is dead, not the agent quiet (the device beats every 30s) | silent re-join |
//! | a failed send | the device went away | [`ViewerPhase::Reconnecting`] + backoff |
//!
//! "Connected" here means JOINED: everything the handle sends is gated on
//! the join answer (web gates the composer on the `live` phase for the same
//! reason).
//!
//! [`enet_steer::STALE`]: crate::enet_steer::STALE

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use api::error::ApiError;

use crate::enet_steer;
use crate::frames::{ActivityEvent, ClientFrame, ViewerFrame};
use crate::{Backoff, SteerRuntime};

/// The `bye` outcome a viewer reports for an ENDED row whose device cannot
/// be reached: the transcript lives on that machine.
pub const DEVICE_OFFLINE: &str = "device_offline";

/// Steer `input` frames stay under 8 KiB; chunk well under that. The cap
/// is measured in UTF-16 code units — every client validates against a JS
/// string's `length` — so the chunker counts UTF-16 units too (web
/// `INPUT_CHUNK_CHARS`, iOS `inputChunkUtf16`).
pub const INPUT_CHUNK_UTF16: usize = 4096;

/// Timings the viewer loop runs on. Defaults are the production values
/// (web/iOS/Android parity); tests shrink them.
#[derive(Clone, Debug)]
pub struct ViewerTimings {
    /// A LIVE feed that has delivered nothing — not even one of the device's
    /// `keepalive` beats — for this long is dead, not quiet.
    pub live_stale: Duration,
    /// First redial bound while the device's run is not reachable yet.
    pub retry_base: Duration,
    /// Cap the redial bound doubles toward.
    pub retry_cap: Duration,
    /// How often the loop re-checks the staleness window.
    pub liveness_tick: Duration,
    /// VAPP-169: the device-offline `Ended` redials this often (the
    /// fixture's `staleMs`) on top of every kick.
    pub offline_redial: Duration,
}

impl Default for ViewerTimings {
    fn default() -> Self {
        Self {
            live_stale: enet_steer::STALE,
            retry_base: Duration::from_secs(3),
            retry_cap: Duration::from_secs(30),
            liveness_tick: Duration::from_secs(5),
            offline_redial: enet_steer::STALE,
        }
    }
}

/// VAPP-33: where a viewer finds the session's device over eNet.
pub trait EnetViewerSource: Send + Sync + 'static {
    /// This install's running node, if any.
    fn node(&self) -> Option<Arc<enet_core::Node>>;
    /// BLOCKING (`installs.forSession`): the endpoint ids (hex) of the
    /// installs running `session_id`, newest first.
    fn endpoints(&self, session_id: &str) -> Result<Vec<String>, ApiError>;
}

/// The production source: this account's install node and the registry's
/// `installs.forSession` (the desktop app and the CLI alike).
pub struct InstallViewerSource {
    pub install: crate::enet::InstallNode,
    pub trpc: Arc<api::TrpcClient>,
}

impl EnetViewerSource for InstallViewerSource {
    fn node(&self) -> Option<Arc<enet_core::Node>> {
        self.install.node()
    }

    fn endpoints(&self, session_id: &str) -> Result<Vec<String>, ApiError> {
        Ok(api::installs::for_session(&self.trpc, session_id)?
            .into_iter()
            .map(|install| install.endpoint_id)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// What the UI observes
// ---------------------------------------------------------------------------

/// Where the connection stands. `Ended` and `Unauthorized` are TERMINAL — the
/// loop has stopped and the handle is inert — except the device-offline
/// `Ended` (VAPP-169), which keeps redialing.
///
/// VAPP-167: serde so the daemon's viewer proxy ([`crate::viewer_proxy`])
/// can stream it to a CLI over the local socket.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewerPhase {
    /// The first join is under way.
    Connecting,
    /// No feed while the session row still says alive: the device may still
    /// be connecting. Auto-redialing with jittered backoff.
    Starting,
    /// Joined; frames are flowing.
    Live,
    /// An unexpected drop — redialing with backoff. (The silent re-join on
    /// staleness deliberately does NOT enter this phase.)
    Reconnecting,
    /// The run is finished (or, `device_offline`, its device unreachable).
    /// `outcome` is the device's, minus the uninformative `"ended"`.
    Ended { outcome: Option<String> },
    /// A refusal a retry cannot turn into a yes. The viewer task never
    /// emits it since VAPP-43; hosts use it for "nothing to dial with".
    Unauthorized { detail: Option<String> },
}

impl ViewerPhase {
    /// Whether the loop has stopped for good.
    pub fn is_terminal(&self) -> bool {
        !self.is_device_offline()
            && matches!(
                self,
                ViewerPhase::Ended { .. } | ViewerPhase::Unauthorized { .. }
            )
    }

    /// VAPP-169: the device-offline `Ended` — the copy shows, the loop
    /// redials.
    pub fn is_device_offline(&self) -> bool {
        matches!(self, ViewerPhase::Ended { outcome: Some(outcome) } if outcome == DEVICE_OFFLINE)
    }

    fn device_offline() -> Self {
        ViewerPhase::Ended { outcome: Some(DEVICE_OFFLINE.to_string()) }
    }
}

/// VAPP-169 (`steer-enet.json` `offlineRedial`): the phase a redial trigger
/// meets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedialPhase {
    /// The device-offline `Ended`.
    Offline,
    /// Ended by `bye`: the transcript is on screen.
    Ended,
    Starting,
}

/// VAPP-169: what may prompt a redial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedialTrigger {
    PresenceOnline,
    PresenceOffline,
    StaleTimer,
    Wakeup,
}

/// VAPP-169 (TS `steerOfflineRedial`): whether `trigger` redials a viewer in
/// `phase`. The loop enacts it: a kick (presence online, a wakeup) cuts any
/// wait short, the stale timer runs only under the offline copy.
pub fn offline_redial(phase: RedialPhase, trigger: RedialTrigger) -> bool {
    if trigger == RedialTrigger::PresenceOffline || phase == RedialPhase::Ended {
        return false;
    }
    phase == RedialPhase::Offline || trigger != RedialTrigger::StaleTimer
}

/// Everything the socket task tells the UI, in arrival order. Apply them to a
/// [`crate::feed::SteerFeed`] and render it; the enum is deliberately close to
/// the wire so the UI adds no protocol logic of its own.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ViewerEvent {
    /// A phase TRANSITION (emitted only when the phase actually changes — a
    /// per-frame re-emission would re-render the world on every replayed
    /// message).
    Phase(ViewerPhase),
    /// The socket is JOINED (the device answered the join) / went
    /// away. Distinct from the phase: a silent redial keeps the phase steady
    /// while the socket is briefly down, and send affordances should dim
    /// honestly for that gap. Deliberately not "opened": the device applies
    /// `input` only from a joined connection, so a socket that is open but
    /// unanswered can send nothing and must not look otherwise.
    Connected(bool),
    /// One activity event → `SteerFeed::apply`, with the publisher's wire
    /// sequence (EXP-783 — every frame carries one).
    Activity(u64, ActivityEvent),
    /// `activity_reset` → `SteerFeed::apply_reset` (which STAGES; it does not
    /// blank the feed — EXP-656).
    Reset,
    /// `activity_synced` → `SteerFeed::apply_synced`. EXP-783: it names the
    /// span the replay covered, so the feed can keep the pages BELOW
    /// `first_seq` instead of swapping the whole transcript away.
    Synced {
        first_seq: u64,
        truncated: bool,
    },
    /// EXP-783: one page of OLDER transcript, answering
    /// [`ViewerHandle::request_history_page`]. Prepended, never appended.
    HistoryPage {
        request_id: String,
        events: Vec<(Option<u64>, ActivityEvent)>,
        done: bool,
    },
    /// The device's liveness beat. Carries nothing; the loop has already
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
    /// `false` when the socket is down OR not joined yet — the caller keeps
    /// its draft, and NOTHING is echoed: a local message for input the device
    /// silently dropped is the one lie the feed must never tell.
    pub fn send_message(&self, text: &str) -> bool {
        if text.is_empty() || !self.send_frames(message_frames(text)) {
            return false;
        }
        let _ = self.events_tx.send(ViewerEvent::LocalMessage(text.to_string()));
        true
    }

    /// Protocol v2 answer: the device drives its own picker and confirms with `answer_ack`. The caller
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

    /// EXP-783: ask for the page of transcript BELOW `before_seq`. The device
    /// reads it off the run's journal (live, or VAPP-43 stored); the answer arrives as
    /// [`ViewerEvent::HistoryPage`]. `false` when the socket is not joined —
    /// the caller retries on its next scroll to the top.
    pub fn request_history_page(&self, request_id: &str, before_seq: u64, limit: u32) -> bool {
        self.send_frames(vec![ClientFrame::HistoryPage {
            request_id: request_id.to_string(),
            before_seq,
            limit,
        }
        .to_json()])
    }

    /// EXP-746: change one live agent option (a chip the publisher advertised
    /// in `config_state`). Fire-and-forget — the publisher's NEXT
    /// `config_state` is the confirmation, so there is no optimistic lock and
    /// no ack deadline (unlike [`ViewerHandle::send_answer`]). A BLANK
    /// `value` is the "CLI default / unset" choice and rides the wire as
    /// such. `false` when the socket is down or not joined.
    pub fn send_config(&self, id: &str, value: &str) -> bool {
        self.send_frames(vec![ClientFrame::SetConfig {
            id: id.to_string(),
            value: value.to_string(),
        }
        .to_json()])
    }

    /// EXP-746: switch to one of `config_state.modes`. Same fire-and-forget
    /// contract as [`ViewerHandle::send_config`].
    pub fn send_mode(&self, id: &str) -> bool {
        self.send_frames(vec![ClientFrame::SetMode { id: id.to_string() }.to_json()])
    }

    /// EXP-790: stop the turn in flight without ending the run — the Stop
    /// glyph the composer shows while the agent is working. Fire-and-forget
    /// like [`ViewerHandle::send_mode`]: the engine cancels the turn and the
    /// feed shows the agent stopping; nothing locks or waits. The frame is
    /// spelled here rather than through [`ClientFrame`] so this stays a
    /// one-line addition beside the protocol's own enum (Android/iOS send the
    /// same bytes). `false` when the socket is down or not joined.
    pub fn send_interrupt(&self) -> bool {
        self.send_frames(vec![r#"{"t":"interrupt"}"#.to_string()])
    }

    /// EXP-861: revoke ONE message the device holds queued behind the
    /// running turn. Fire-and-forget like [`ViewerHandle::send_interrupt`]:
    /// the device's next `queue` frame is the confirmation. `false` when
    /// the socket is down or not joined.
    pub fn send_unqueue(&self, id: &str) -> bool {
        self.send_frames(vec![ClientFrame::Unqueue { id: id.to_string() }.to_json()])
    }

    /// A wakeup nudge (the machine woke, the network came back, the host
    /// device came online): cut short a pending backoff and redial now.
    ///
    /// Acts only on a connection that is provably stuck — a live socket that
    /// spoke inside the staleness window, and a dial younger than its join
    /// deadline, are left alone (redialing a healthy socket costs a full
    /// replay for nothing). Cheap and idempotent: callers may fire it freely,
    /// which is what makes it safe to hang off every window activation.
    /// Mirrors the web store's `kick`.
    pub fn kick(&self) {
        let _ = self.cmd_tx.send(ViewerCmd::Kick);
    }

    /// The synced `sessions` row says the run ended. The redial loops
    /// take it as the truth: a `no_such_session` for an ended run can only
    /// park the viewer in [`ViewerPhase::Starting`] forever, because the
    /// publisher is gone (web `noteSessionStatus`, EXP-639).
    pub fn note_session_ended(&self) {
        self.note_session_status(true);
    }

    /// The synced `sessions` row's liveness, reported on every row change.
    /// A row flipping back to running clears an earlier end, so its NO FEED
    /// redials read [`ViewerPhase::Starting`] again (VAPP-174).
    pub fn note_session_status(&self, ended: bool) {
        self.session_ended.store(ended, Ordering::SeqCst);
    }

    /// Close the connection for good (the tab closed, the app is signing
    /// out). Idempotent.
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_tx.send(ViewerCmd::Shutdown);
    }

    /// Whether the socket is open AND joined right now — i.e. whether input
    /// sent this instant would actually reach the agent (dim the composer
    /// honestly).
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// False once the loop stopped for good (shutdown, ended, unauthorized).
    /// VAPP-169: true under the device-offline `Ended` — it redials.
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
/// carry over is the UNIT: every client validates against a JS string's
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

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// Start watching a coding session over eNet (`enet` = this install's node
/// and the session's endpoints; `None` = no eNet here: NO FEED). Non-blocking:
/// spawns the task onto the steer runtime and returns the handle
/// immediately. Drain `events_rx` on the UI side and apply what arrives to a
/// [`crate::feed::SteerFeed`].
pub fn spawn_viewer(
    runtime: &SteerRuntime,
    enet: Option<Arc<dyn EnetViewerSource>>,
    session_id: String,
    events_tx: flume::Sender<ViewerEvent>,
) -> ViewerHandle {
    spawn_viewer_with(runtime, enet, session_id, events_tx, ViewerTimings::default())
}

/// [`spawn_viewer`] with explicit timings (tests shrink the windows).
pub fn spawn_viewer_with(
    runtime: &SteerRuntime,
    enet: Option<Arc<dyn EnetViewerSource>>,
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
        enet,
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

/// How one feed ended — the whole reconnect policy keys on this.
#[derive(Debug)]
enum ConnEnd {
    /// The handle said stop.
    Shutdown,
    /// The handle said redial now.
    Kicked,
    /// `bye`: the run (or its stored replay) is finished.
    Ended(Option<String>),
    /// NO FEED: nothing answered the join, or the device stopped serving.
    NoFeed,
    /// Live but mute past the staleness window. Silent re-join.
    Stale,
    /// A failed send.
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
    enet: Option<Arc<dyn EnetViewerSource>>,
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

    while running.load(Ordering::SeqCst) {
        let end = match join_over_enet(enet.as_ref(), &session_id).await {
            Some(mut link) => {
                pump_connection(&mut link, &events_tx, &mut phases, &cmd_rx, &timings, &mut backoff, &connected).await
            }
            None => ConnEnd::NoFeed,
        };

        if connected.swap(false, Ordering::SeqCst) {
            let _ = events_tx.send(ViewerEvent::Connected(false));
        }

        // The delay this end earns before the next join, and whether there
        // IS a next join.
        let delay = match end {
            ConnEnd::Shutdown => {
                running.store(false, Ordering::SeqCst);
                return;
            }
            ConnEnd::Ended(outcome) => {
                stop(&running, &mut phases, ViewerPhase::Ended { outcome });
                return;
            }
            ConnEnd::NoFeed => {
                // VAPP-43: the relay carries no session content, so there is
                // no fallback. An ended row's transcript lives on its device;
                // a live one may still be connecting (an `in_review` run is
                // still alive and steerable, EXP-194).
                // VAPP-169: the device-offline copy is not terminal: redial
                // under it on a kick or every `offline_redial`, silently
                // (the phase holds; NO FEED again re-sets the same one).
                if session_ended.load(Ordering::SeqCst) {
                    phases.set(ViewerPhase::device_offline());
                    timings.offline_redial
                } else {
                    phases.set(ViewerPhase::Starting);
                    backoff.next_delay_equal_jitter()
                }
            }
            // The user (or a wakeup) asked: no delay, no phase flap.
            ConnEnd::Kicked => Duration::ZERO,
            ConnEnd::Stale => {
                // Live on paper over a feed that said nothing for two
                // keepalives: re-join under the live phase.
                log::debug!("steer viewer: silent re-join (no frames for {:?})", timings.live_stale);
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
    // The flag goes down FIRST: the phase event is what a view learns the
    // ending from, and it must never read `is_active()` as `true` off the
    // back of a terminal phase (the loop is already returning).
    running.store(false, Ordering::SeqCst);
    phases.set(phase);
}

/// One joined feed's select loop.
async fn pump_connection(
    link: &mut EnetLink,
    events_tx: &flume::Sender<ViewerEvent>,
    phases: &mut Phases,
    cmd_rx: &flume::Receiver<ViewerCmd>,
    timings: &ViewerTimings,
    backoff: &mut Backoff,
    connected: &Arc<AtomicBool>,
) -> ConnEnd {
    let mut liveness = tokio::time::interval(timings.liveness_tick);
    liveness.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Any inbound frame proves the feed — including the device's
    // `keepalive` beat, which exists for exactly this (EXP-648).
    let mut last_rx = Instant::now();
    // `join_over_enet` returned only once the device answered the join with
    // its replay: what the handle sends from here is applied.
    if !connected.swap(true, Ordering::SeqCst) {
        let _ = events_tx.send(ViewerEvent::Connected(true));
    }
    loop {
        tokio::select! {
            cmd = cmd_rx.recv_async() => match cmd {
                Ok(ViewerCmd::Frames(frames)) => {
                    if !link.send(frames).await {
                        return ConnEnd::Dropped(Some("send failed".into()));
                    }
                }
                Ok(ViewerCmd::Kick) => {
                    // A wakeup nudge, not a Reconnect button: a feed that
                    // spoke inside the staleness window is healthy, and
                    // re-joining it would buy a full replay and nothing else.
                    if last_rx.elapsed() <= timings.live_stale {
                        continue;
                    }
                    link.leave();
                    return ConnEnd::Kicked;
                }
                Ok(ViewerCmd::Shutdown) | Err(_) => {
                    link.leave();
                    return ConnEnd::Shutdown;
                }
            },
            frame = link.next() => {
                let Some(text) = frame else {
                    return ConnEnd::Dropped(Some("subscription closed".into()));
                };
                last_rx = Instant::now();
                match ViewerFrame::parse(&text) {
                    Some(ViewerFrame::Activity { event, seq }) => {
                        if !phases.is(&ViewerPhase::Live) {
                            phases.set(ViewerPhase::Live);
                            backoff.reset();
                        }
                        let _ = events_tx.send(ViewerEvent::Activity(seq, event));
                    }
                    Some(ViewerFrame::ActivityReset) => {
                        if !phases.is(&ViewerPhase::Live) {
                            phases.set(ViewerPhase::Live);
                            backoff.reset();
                        }
                        let _ = events_tx.send(ViewerEvent::Reset);
                    }
                    Some(ViewerFrame::ActivitySynced { first_seq, truncated, .. }) => {
                        let _ = events_tx.send(ViewerEvent::Synced {
                            first_seq,
                            truncated: truncated.unwrap_or(false),
                        });
                    }
                    Some(ViewerFrame::HistoryChunk { request_id, events, seqs, done }) => {
                        // A `None` slot is an event kind this build does not
                        // know (compat review R6 #3): dropped AFTER the zip so
                        // the numbering of its neighbours stays right.
                        let events = events
                            .into_iter()
                            .enumerate()
                            .filter_map(|(ix, event)| Some((seqs.get(ix).copied(), event?)))
                            .collect();
                        let _ = events_tx.send(ViewerEvent::HistoryPage { request_id, events, done });
                    }
                    Some(ViewerFrame::Keepalive) => {
                        // Counted by the `last_rx` stamp above; never a phase
                        // change. Forwarded because EXP-656 uses it as an
                        // end-of-replay signal.
                        let _ = events_tx.send(ViewerEvent::Keepalive);
                    }
                    // eNet has no close frame: a `bye` IS the end.
                    Some(ViewerFrame::Bye { outcome }) => {
                        return ConnEnd::Ended(outcome.filter(|o| o != "ended"));
                    }
                    Some(ViewerFrame::Error { code, .. }) => {
                        log::debug!("steer viewer: the device stopped serving the session ({code})");
                        return ConnEnd::NoFeed;
                    }
                    // Unknown frames: ignored, never a teardown.
                    None => log::debug!("steer viewer: unparseable frame ignored"),
                }
            },
            _ = liveness.tick() => {
                // EXP-648: silence past two device keepalives means the feed
                // is dead, not that the agent is parked on a question.
                if last_rx.elapsed() > timings.live_stale {
                    link.leave();
                    return ConnEnd::Stale;
                }
            }
        }
    }
}

/// A joined eNet feed: the device's frames arrive on `sub`; `held` = what
/// arrived while joining (the replay), delivered first.
struct EnetLink {
    node: Arc<enet_core::Node>,
    device: enet_core::EndpointId,
    session_id: String,
    sub: enet_steer::ViewerSubscription,
    held: VecDeque<String>,
}

impl EnetLink {
    async fn next(&mut self) -> Option<String> {
        if let Some(frame) = self.held.pop_front() {
            return Some(frame);
        }
        self.sub.frames.recv_async().await.ok()
    }

    /// Writes `frames` in order (one envelope). `false` = gone.
    async fn send(&mut self, frames: Vec<String>) -> bool {
        match enet_steer::send_envelope(&self.node, self.device, &self.session_id, &frames).await {
            Ok(()) => true,
            Err(err) => {
                log::debug!("steer viewer: eNet send failed: {err}");
                false
            }
        }
    }

    /// Tell the device we `leave` (best effort).
    fn leave(&self) {
        let (node, device, session_id) = (Arc::clone(&self.node), self.device, self.session_id.clone());
        tokio::spawn(async move {
            let _ = enet_steer::send_envelope(&node, device, &session_id, &[LEAVE_FRAME.to_string()]).await;
        });
    }
}

const LEAVE_FRAME: &str = r#"{"t":"leave"}"#;

/// DISCOVERY: `join` each install running the session, newest first, until
/// one answers with its replay up to `activity_synced` within
/// [`enet_steer::JOIN_TIMEOUT`]. `None` = NO FEED.
async fn join_over_enet(source: Option<&Arc<dyn EnetViewerSource>>, session_id: &str) -> Option<EnetLink> {
    let source = source?;
    let node = source.node()?;
    let lookup = Arc::clone(source);
    let id = session_id.to_string();
    let endpoints = match tokio::task::spawn_blocking(move || lookup.endpoints(&id)).await {
        Ok(Ok(endpoints)) => endpoints,
        Ok(Err(err)) => {
            log::debug!("steer viewer: installs.forSession failed: {err}");
            return None;
        }
        Err(_) => return None,
    };
    let own = node.endpoint_id();
    let join = ClientFrame::join().to_json();
    for device in endpoints.iter().filter_map(|hex| crate::enet::parse_endpoint(hex)).filter(|id| *id != own) {
        let sub = enet_steer::subscribe(session_id, device);
        if let Err(err) = enet_steer::send_envelope(&node, device, session_id, &[join.clone()]).await {
            log::debug!("steer viewer: eNet join to {} failed: {err}", crate::enet::endpoint_hex(&device));
            continue;
        }
        let deadline = tokio::time::Instant::now() + enet_steer::JOIN_TIMEOUT;
        let mut held = VecDeque::new();
        loop {
            let Ok(Ok(frame)) = tokio::time::timeout_at(deadline, sub.frames.recv_async()).await else {
                log::debug!("steer viewer: eNet join to {} unanswered", crate::enet::endpoint_hex(&device));
                break;
            };
            let tag = serde_json::from_str::<serde_json::Value>(&frame)
                .ok()
                .and_then(|value| value.get("t").and_then(|t| t.as_str()).map(str::to_string));
            match tag.as_deref() {
                Some("error") => {
                    log::debug!("steer viewer: {} refused the eNet join: {frame}", crate::enet::endpoint_hex(&device));
                    break;
                }
                Some("activity_synced") => {
                    held.push_back(frame);
                    log::info!("steer viewer: joined {session_id} over eNet");
                    return Some(EnetLink { node, device, session_id: session_id.to_string(), sub, held });
                }
                _ => held.push_back(frame),
            }
        }
    }
    None
}

/// Interruptible delay between joins: `Break` on shutdown, `Continue` on a
/// kick (redial now) or the delay elapsing. Frames sent meanwhile are
/// dropped here — the handle already refused them, so this only catches the
/// race where the feed died between the check and the send.
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

    /// VAPP-169: the fixture's `offlineRedial` cases.
    #[test]
    fn offline_redial_cases() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json")).unwrap();
        for case in fixture["offlineRedial"].as_array().unwrap() {
            let phase = match case["phase"].as_str().unwrap() {
                "offline" => RedialPhase::Offline,
                "ended" => RedialPhase::Ended,
                "starting" => RedialPhase::Starting,
                other => panic!("unknown phase {other}"),
            };
            let trigger = match case["trigger"].as_str().unwrap() {
                "presenceOnline" => RedialTrigger::PresenceOnline,
                "presenceOffline" => RedialTrigger::PresenceOffline,
                "staleTimer" => RedialTrigger::StaleTimer,
                "wakeup" => RedialTrigger::Wakeup,
                other => panic!("unknown trigger {other}"),
            };
            assert_eq!(offline_redial(phase, trigger), case["redial"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_device_offline_end_is_not_terminal() {
        assert!(!ViewerPhase::device_offline().is_terminal());
        assert!(ViewerPhase::device_offline().is_device_offline());
        assert!(ViewerPhase::Ended { outcome: Some("history".into()) }.is_terminal());
        assert!(ViewerPhase::Ended { outcome: None }.is_terminal());
    }

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
}
