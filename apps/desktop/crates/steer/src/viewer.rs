//! The steering VIEWER connection (EXP-696) — watch and steer a coding
//! session running on ANOTHER of the user's devices.
//!
//! [`publisher`] serves a session this machine is hosting; this module is
//! the other end of that wire, what web/iOS/Android have had since EXP-249.
//! VAPP-43: it talks to the session's DEVICE over eNet's chat channel
//! (`fixtures/steer-enet.json`), falling back to a sealed relay tunnel (see
//! the carriers below): the endpoints
//! `installs.forSession` names, newest first, each sent a `join` until one
//! answers with its `activity_reset` within
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
//! ## The carriers (P2P stabilisation, `steer-enet.json` `fallback`)
//!
//! eNet is the PRIMARY carrier; a sealed relay TUNNEL (`tunnel.rs`, the
//! same envelopes sealed to the session tree key) is its automatic,
//! invisible fallback, run by the one carrier rule
//! ([`tunnel::carrier_step`], replayed against `carrierCases`): eNet first;
//! the relay leg opens when eNet has delivered nothing within
//! [`tunnel::STALL`], has no feed, or drops mid-stream; eNet keeps
//! redialing in the background and takes over on its `activity_reset`, the
//! relay closing once eNet delivers on. Frames apply only from the active
//! carrier; steering goes out on it. No relay leg (a keyless tree, no
//! secret here, the device offline or older on the relay) = eNet only.
//!
//! ## The reconnect rules
//!
//! | signal | meaning | what happens |
//! |---|---|---|
//! | NO FEED: both carriers failed (eNet: no node, no endpoint, every endpoint refused, no `activity_reset` in time; relay: unavailable or dropped) | the device is not reachable (or not running the session yet) | a live row: [`ViewerPhase::Starting`] + jittered 3s→30s eNet redial (each redial retries the relay leg too); an ended row: [`ViewerPhase::Ended`] `device_offline`, NOT terminal (VAPP-169): a silent redial on a kick (presence online, a wakeup) or every [`enet_steer::STALE`] |
//! | `bye` on the active carrier | the run (or a stored replay, `history`) is finished | [`ViewerPhase::Ended`], stop |
//! | `error`, [`enet_steer::STALE`] of silence, a failed send, no `activity_synced` in time | that carrier's feed is gone | the other carrier takes the feed (eNet → relay at once); the phase holds |
//!
//! EXP-1487 (`lostBanner`): with NEITHER carrier active after a live feed,
//! the phase holds while both redial; "Connection lost"
//! ([`ViewerPhase::Reconnecting`]) shows only after
//! [`enet_steer::LOST_GRACE`] AND [`enet_steer::LOST_FAILED_REDIALS`]
//! failed redials (of both carriers) since.
//!
//! Every open is one `steer.view` (`conn_stats.rs`): `settled` at the first
//! applied frame (or [`conn_stats::SETTLE_AFTER`] without one), `ended` when
//! the viewer closes.
//!
//! "Connected" here means JOINED: a carrier is active (its device answered
//! the join); everything the handle sends is gated on it (web gates the
//! composer on the `live` phase for the same reason).
//!
//! [`enet_steer::STALE`]: crate::enet_steer::STALE

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use api::error::ApiError;
use api::steer::TunnelTicketResult;

use crate::conn_stats;
use crate::enet_steer;
use crate::tunnel::{self, Carrier, CarrierAction, CarrierEvent, CarrierState, TunnelKeys};
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

/// #EXP-1678 (`fallback.startingRetry`): the NO FEED redial's backoff.
pub const STARTING_RETRY_BASE_MS: u64 = 3_000;
pub const STARTING_RETRY_MAX_MS: u64 = 30_000;

/// TS `steerStartingRetryCapMs`: the cap of the n-th consecutive NO FEED
/// redial; the wait is random in [cap/2, cap) ([`Backoff::next_delay_equal_jitter`]).
pub fn starting_retry_cap_ms(retries: u32) -> u64 {
    STARTING_RETRY_BASE_MS.saturating_mul(1u64.checked_shl(retries).unwrap_or(u64::MAX)).min(STARTING_RETRY_MAX_MS)
}

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
    /// EXP-1487: how long a dropped live feed redials silently at least
    /// (the fixture's `lostBanner.graceMs`).
    pub lost_grace: Duration,
    /// P2P fallback: no eNet frame by then opens the relay leg (`stallMs`).
    pub stall: Duration,
    /// A join (either carrier) with no `activity_synced` by then is no feed.
    pub join_timeout: Duration,
    /// A dropped relay leg reopens after these (`relayRetryMs`).
    pub relay_retry: Vec<Duration>,
    /// A view with no first frame by then settles `failed` (`connStats`).
    pub settle_after: Duration,
}

impl Default for ViewerTimings {
    fn default() -> Self {
        Self {
            live_stale: enet_steer::STALE,
            retry_base: Duration::from_millis(STARTING_RETRY_BASE_MS),
            retry_cap: Duration::from_millis(STARTING_RETRY_MAX_MS),
            liveness_tick: Duration::from_secs(5),
            offline_redial: enet_steer::STALE,
            lost_grace: enet_steer::LOST_GRACE,
            stall: tunnel::STALL,
            join_timeout: enet_steer::JOIN_TIMEOUT,
            relay_retry: tunnel::RELAY_RETRY.to_vec(),
            settle_after: conn_stats::SETTLE_AFTER,
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
    /// BLOCKING (`steer.tunnelTicket`): the sealed relay tunnel's ticket,
    /// eNet's fallback carrier. Default: no relay leg.
    fn tunnel_ticket(&self, _session_id: &str) -> Result<TunnelTicketResult, ApiError> {
        Ok(TunnelTicketResult::Unavailable)
    }
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

    fn tunnel_ticket(&self, session_id: &str) -> Result<TunnelTicketResult, ApiError> {
        api::steer::tunnel_ticket(&self.trpc, session_id)
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
    /// #EXP-1703: the host device is offline in presence.
    host_offline: Arc<AtomicBool>,
    carrier: Arc<AtomicU8>,
}

impl ViewerHandle {
    /// The carrier whose frames apply right now (diagnostics and tests:
    /// carriers are invisible in the UI).
    pub fn carrier(&self) -> Option<Carrier> {
        match self.carrier.load(Ordering::SeqCst) {
            1 => Some(Carrier::Enet),
            2 => Some(Carrier::Relay),
            _ => None,
        }
    }

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

    /// #EXP-1703 (`steer-enet.json` A DEVICE THAT IS AWAY, web
    /// `noteHostOffline`): the host device is offline in presence (the synced
    /// devices row; unknown = online). A live row's NO FEED then redials on
    /// the device-offline cadence ([`ViewerTimings::offline_redial`], and at
    /// once on a [`Self::kick`]: presence online, a wakeup), never on the
    /// starting backoff.
    pub fn note_host_offline(&self, offline: bool) {
        self.host_offline.store(offline, Ordering::SeqCst);
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
    let host_offline = Arc::new(AtomicBool::new(false));
    let carrier = Arc::new(AtomicU8::new(0));
    let handle = ViewerHandle {
        cmd_tx,
        events_tx: events_tx.clone(),
        connected: connected.clone(),
        running: running.clone(),
        session_ended: session_ended.clone(),
        host_offline: host_offline.clone(),
        carrier: carrier.clone(),
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
        host_offline,
        carrier,
    ));
    handle
}

// ---------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------

/// A failure label (`connStats.errorClasses`).
type Class = &'static str;

/// How one carrier leg failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegFailure {
    /// eNet: no feed (no node, no endpoint, a failed send, no
    /// `activity_reset` in time); `refused` / `unauthorized` only when
    /// EVERY endpoint answered `error` or `bye` (#EXP-1678).
    NoFeed(Class),
    /// The leg had a feed (or a socket) and lost it.
    Dropped(Class),
    /// Relay: there is no relay leg for this session right now.
    Unavailable(Class),
}

impl LegFailure {
    fn class(self) -> Class {
        match self {
            LegFailure::NoFeed(class) | LegFailure::Dropped(class) | LegFailure::Unavailable(class) => class,
        }
    }
}

/// What a leg task tells the loop.
#[derive(Debug)]
enum LegMsg {
    /// One device frame. `path` = the `connStats` path, on the leg's
    /// `activity_reset`.
    Frame { carrier: Carrier, gen: u64, text: String, path: Option<&'static str> },
    Failed { carrier: Carrier, gen: u64, failure: LegFailure },
    /// The relay leg is AVAILABLE (this install holds the tree key): only
    /// now is it a fallback (#EXP-1678).
    RelayAvailable { gen: u64 },
}

/// What the loop asks of a leg.
enum LegOut {
    Frames(Vec<String>),
    /// Re-send `join` (the stand-by relay becoming the feed).
    Join,
    /// `leave` and end.
    Close,
}

/// A running leg task (dropping it ends the task: its `out` disconnects).
struct Leg {
    gen: u64,
    out: flume::Sender<LegOut>,
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

/// The `steer.view` numbers of this open (`connStats`).
struct ViewStats {
    opened: Instant,
    /// open → the first applied frame, and the carrier that delivered it.
    first: Option<(u64, &'static str)>,
    settled: bool,
    fallbacks: u32,
    reconnects: u32,
    failures: std::collections::BTreeMap<&'static str, u32>,
    last_error: Option<&'static str>,
}

impl ViewStats {
    fn new() -> ViewStats {
        ViewStats {
            opened: Instant::now(),
            first: None,
            settled: false,
            fallbacks: 0,
            reconnects: 0,
            failures: Default::default(),
            last_error: None,
        }
    }

    fn failed(&mut self, class: Class) {
        let class = conn_stats::error_class(class);
        *self.failures.entry(class).or_default() += 1;
        self.last_error = Some(class);
    }

    fn data(&self, phase: &'static str) -> conn_stats::ConnData {
        conn_stats::ConnData {
            phase,
            outcome: if self.first.is_some() { "ok" } else { "failed" },
            path: self.first.map(|(_, path)| path),
            first_frame_ms: self.first.map(|(ms, _)| ms),
            // #EXP-1678: a desktop window is never "in the background" here.
            error_class: conn_stats::view_error_class(self.first.is_some(), self.last_error, false)
                .filter(|_| self.first.is_none() || phase == "ended"),
            ..Default::default()
        }
    }

    /// `settled`, once: the first frame, or the settle window without one.
    fn settle(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        conn_stats::record(conn_stats::KIND_VIEW, conn_stats::SOURCE_VIEWER, self.data("settled"));
    }

    fn ended(&mut self) {
        let mut data = self.data("ended");
        data.duration_ms = Some(self.opened.elapsed().as_millis() as u64);
        data.fallbacks = Some(self.fallbacks);
        data.reconnects = Some(self.reconnects);
        data.failures = (!self.failures.is_empty()).then(|| self.failures.clone());
        conn_stats::record(conn_stats::KIND_VIEW, conn_stats::SOURCE_VIEWER, data);
    }
}

/// The viewer loop's state: the carrier rule ([`tunnel::carrier_step`])
/// over an eNet leg and a sealed relay leg, the phases and the analytics.
struct Run {
    session_id: String,
    enet: Option<Arc<dyn EnetViewerSource>>,
    events_tx: flume::Sender<ViewerEvent>,
    timings: ViewerTimings,
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    session_ended: Arc<AtomicBool>,
    /// #EXP-1703: [`ViewerHandle::note_host_offline`].
    host_offline: Arc<AtomicBool>,
    carrier_flag: Arc<AtomicU8>,
    phases: Phases,
    backoff: Backoff,
    carriers: CarrierState,
    enet_leg: Option<Leg>,
    relay_leg: Option<Leg>,
    legs_tx: flume::Sender<LegMsg>,
    gen: u64,
    stall_at: Option<Instant>,
    enet_redial_at: Option<Instant>,
    relay_reopen_at: Option<Instant>,
    relay_attempt: usize,
    /// The last eNet / relay attempt failed (cleared by its reset).
    enet_failed: bool,
    relay_failed: bool,
    /// EXP-1487: since when NEITHER carrier is active after a live feed,
    /// and how many redials (of both) failed since.
    lost: Option<(Instant, u32)>,
    stats: ViewStats,
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
    host_offline: Arc<AtomicBool>,
    carrier_flag: Arc<AtomicU8>,
) {
    let (legs_tx, legs_rx) = flume::unbounded();
    let mut run = Run {
        session_id,
        enet,
        phases: Phases { tx: events_tx.clone(), current: None },
        events_tx,
        backoff: Backoff::new(timings.retry_base, timings.retry_cap),
        timings,
        running,
        connected,
        session_ended,
        host_offline,
        carrier_flag,
        carriers: CarrierState::INITIAL,
        enet_leg: None,
        relay_leg: None,
        legs_tx,
        gen: 0,
        stall_at: None,
        enet_redial_at: None,
        relay_reopen_at: None,
        relay_attempt: 0,
        enet_failed: false,
        relay_failed: false,
        lost: None,
        stats: ViewStats::new(),
    };
    run.phases.set(ViewerPhase::Connecting);
    run.step(CarrierEvent::Start);
    loop {
        if !run.running.load(Ordering::SeqCst) {
            run.close();
            return;
        }
        let deadline = run.next_deadline();
        tokio::select! {
            cmd = cmd_rx.recv_async() => match cmd {
                Ok(ViewerCmd::Frames(frames)) => {
                    // Steering goes out on the ACTIVE carrier (a failed send
                    // comes back as that leg's drop).
                    if let Some(leg) = run.active_leg() {
                        let _ = leg.out.send(LegOut::Frames(frames));
                    }
                }
                Ok(ViewerCmd::Kick) => run.kick(),
                Ok(ViewerCmd::Shutdown) | Err(_) => {
                    run.running.store(false, Ordering::SeqCst);
                    run.close();
                    return;
                }
            },
            msg = legs_rx.recv_async() => {
                // `run` holds a sender: the channel never closes.
                let Ok(msg) = msg else { continue };
                if let Some(outcome) = run.leg_msg(msg) {
                    run.close();
                    stop(&run.running, &mut run.phases, ViewerPhase::Ended { outcome });
                    return;
                }
            }
            _ = tokio::time::sleep_until(deadline.into()) => run.timers(),
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

impl Run {
    fn active_leg(&self) -> Option<&Leg> {
        match self.carriers.active? {
            Carrier::Enet => self.enet_leg.as_ref(),
            Carrier::Relay => self.relay_leg.as_ref(),
        }
    }

    fn next_deadline(&self) -> Instant {
        let settle = (!self.stats.settled).then(|| self.stats.opened + self.timings.settle_after);
        [self.stall_at, self.enet_redial_at, self.relay_reopen_at, settle]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(3_600))
    }

    /// One carrier event: the rule's next state, and its side actions run.
    /// Returns whether the event's frame applies.
    fn step(&mut self, event: CarrierEvent) -> bool {
        let before = self.carriers.active;
        let (next, actions) = tunnel::carrier_step(self.carriers, event);
        self.carriers = next;
        let mut apply = false;
        for action in actions {
            match action {
                CarrierAction::DialEnet => self.spawn_enet(),
                CarrierAction::ArmStall => self.stall_at = Some(Instant::now() + self.timings.stall),
                CarrierAction::OpenRelay => self.spawn_relay(),
                CarrierAction::RejoinRelay => match &self.relay_leg {
                    Some(leg) => {
                        let _ = leg.out.send(LegOut::Join);
                    }
                    None => self.spawn_relay(),
                },
                CarrierAction::CloseRelay => {
                    if let Some(leg) = self.relay_leg.take() {
                        let _ = leg.out.send(LegOut::Close);
                    }
                    self.relay_attempt = 0;
                }
                CarrierAction::ReopenRelayLater => {
                    self.relay_leg = None;
                    let retry = &self.timings.relay_retry;
                    let delay = retry[self.relay_attempt.min(retry.len() - 1)];
                    self.relay_attempt += 1;
                    self.relay_reopen_at = Some(Instant::now() + delay);
                }
                CarrierAction::RedialEnetLater => {
                    self.enet_leg = None;
                    let delay = self.redial_delay();
                    self.enet_redial_at = Some(Instant::now() + delay);
                }
                CarrierAction::Apply => apply = true,
                CarrierAction::Drop => {}
            }
        }
        self.sync_active(before);
        apply
    }

    /// The next eNet redial's wait: the `startingRetry` backoff (reset by a
    /// feed, see [`Run::apply`]). VAPP-169: under the device-offline copy
    /// (both carriers failed, the row ended) the slow beat; #EXP-1703: so
    /// does a live row's NO FEED while its host is offline in presence.
    fn redial_delay(&mut self) -> Duration {
        let away = self.session_ended.load(Ordering::SeqCst) || self.host_offline.load(Ordering::SeqCst);
        if away && self.relay_failed {
            self.timings.offline_redial
        } else {
            self.backoff.next_delay_equal_jitter()
        }
    }

    /// The active carrier changed: `Connected`, the lost clock, the flag.
    fn sync_active(&mut self, before: Option<Carrier>) {
        let now = self.carriers.active;
        self.carrier_flag.store(carrier_code(now), Ordering::SeqCst);
        match (before, now) {
            (None, Some(_)) => {
                if self.stats.first.is_some() {
                    self.stats.reconnects += 1;
                }
                self.lost = None;
                if !self.connected.swap(true, Ordering::SeqCst) {
                    let _ = self.events_tx.send(ViewerEvent::Connected(true));
                }
            }
            (Some(_), None) => {
                if self.stats.first.is_some() {
                    self.lost = Some((Instant::now(), 0));
                }
                if self.connected.swap(false, Ordering::SeqCst) {
                    let _ = self.events_tx.send(ViewerEvent::Connected(false));
                }
            }
            _ => {}
        }
    }

    fn next_gen(&mut self) -> u64 {
        self.gen += 1;
        self.gen
    }

    fn spawn_enet(&mut self) {
        if self.enet_leg.is_some() {
            return;
        }
        self.enet_redial_at = None;
        let gen = self.next_gen();
        let (out, out_rx) = flume::unbounded();
        self.enet_leg = Some(Leg { gen, out });
        tokio::spawn(enet_leg(
            self.enet.clone(),
            self.session_id.clone(),
            gen,
            self.timings.clone(),
            self.legs_tx.clone(),
            out_rx,
        ));
    }

    fn spawn_relay(&mut self) {
        self.relay_reopen_at = None;
        // This round's relay leg is opening: its stall has nothing left to do.
        self.stall_at = None;
        // `fallbacks` counts once the leg is available (`RelayAvailable`).
        let gen = self.next_gen();
        let (out, out_rx) = flume::unbounded();
        if let Some(previous) = self.relay_leg.replace(Leg { gen, out }) {
            let _ = previous.out.send(LegOut::Close);
        }
        tokio::spawn(relay_leg(
            self.enet.clone(),
            self.session_id.clone(),
            gen,
            self.timings.clone(),
            self.legs_tx.clone(),
            out_rx,
        ));
    }

    fn current_gen(&self, carrier: Carrier) -> Option<u64> {
        match carrier {
            Carrier::Enet => self.enet_leg.as_ref().map(|leg| leg.gen),
            Carrier::Relay => self.relay_leg.as_ref().map(|leg| leg.gen),
        }
    }

    /// One leg message. `Some(outcome)` = the active carrier said `bye`.
    fn leg_msg(&mut self, msg: LegMsg) -> Option<Option<String>> {
        match msg {
            LegMsg::RelayAvailable { gen } => {
                if self.current_gen(Carrier::Relay) == Some(gen) {
                    self.stats.fallbacks += 1;
                }
                None
            }
            LegMsg::Frame { carrier, gen, text, path } => {
                if self.current_gen(carrier) != Some(gen) {
                    return None;
                }
                let reset = ViewerFrame::parse(&text).is_some_and(|frame| matches!(frame, ViewerFrame::ActivityReset));
                let event = match (carrier, reset) {
                    (Carrier::Enet, true) => CarrierEvent::EnetReset,
                    (Carrier::Enet, false) => CarrierEvent::EnetSynced,
                    (Carrier::Relay, true) => CarrierEvent::RelayReset,
                    (Carrier::Relay, false) => CarrierEvent::RelaySynced,
                };
                if reset {
                    match carrier {
                        Carrier::Enet => self.enet_failed = false,
                        Carrier::Relay => {
                            self.relay_failed = false;
                            self.relay_attempt = 0;
                        }
                    }
                }
                if !self.step(event) {
                    return None;
                }
                if reset && carrier == Carrier::Enet && self.relay_leg.is_some() {
                    log::info!("steer viewer: {} back on eNet", self.session_id);
                }
                self.apply(&text, path.unwrap_or(match carrier {
                    Carrier::Enet => conn_stats::PATH_DIRECT,
                    Carrier::Relay => conn_stats::PATH_SERVER_FALLBACK,
                }))
            }
            LegMsg::Failed { carrier, gen, failure } => {
                if self.current_gen(carrier) != Some(gen) {
                    return None;
                }
                log::debug!("steer viewer: {carrier:?} leg of {} failed: {failure:?}", self.session_id);
                self.stats.failed(failure.class());
                let event = match (carrier, failure) {
                    // #EXP-1678: every endpoint said no (see `join_over_enet`).
                    (Carrier::Enet, LegFailure::NoFeed("refused" | "unauthorized")) => CarrierEvent::EnetRefused,
                    (Carrier::Enet, LegFailure::NoFeed(_)) => CarrierEvent::EnetNoFeed,
                    (Carrier::Enet, _) => CarrierEvent::EnetDropped,
                    // A relay that WAS the feed and goes away is a drop.
                    (Carrier::Relay, LegFailure::Unavailable(_)) if self.carriers.active != Some(Carrier::Relay) => {
                        CarrierEvent::RelayUnavailable
                    }
                    (Carrier::Relay, _) => CarrierEvent::RelayDropped,
                };
                match carrier {
                    Carrier::Enet => {
                        self.enet_failed = true;
                        self.enet_leg = None;
                    }
                    Carrier::Relay => {
                        self.relay_failed = true;
                        self.relay_leg = None;
                    }
                }
                if event == CarrierEvent::EnetDropped && self.carriers.active == Some(Carrier::Enet) {
                    log::info!("steer viewer: eNet feed of {} dropped ({}); falling back", self.session_id, failure.class());
                }
                self.step(event);
                if event == CarrierEvent::EnetRefused && self.carriers.active.is_none() && !self.carriers.relay_open {
                    // #EXP-1678: no relay leg (it reaches the same device):
                    // NO FEED at once, and no stall opening one later.
                    self.stall_at = None;
                    self.relay_failed = true;
                }
                self.no_feed();
                None
            }
        }
    }

    /// EXP-1487 + `fallback` VISIBLE STATE, after a failure: the lost
    /// banner counts failed redials of BOTH carriers once neither is active;
    /// NO FEED (both failed) reads starting / device offline.
    fn no_feed(&mut self) {
        if self.carriers.active.is_some() {
            return;
        }
        if let Some((since, failed)) = self.lost.as_mut() {
            *failed += 1;
            if since.elapsed() >= self.timings.lost_grace && *failed >= enet_steer::LOST_FAILED_REDIALS {
                self.phases.set(ViewerPhase::Reconnecting);
            }
        }
        if !(self.enet_failed && self.relay_failed) {
            return;
        }
        // #EXP-1678 STARTING REDIAL: NO FEED always has an eNet redial due
        // (a kick cuts it short).
        if self.enet_leg.is_none() && self.enet_redial_at.is_none() {
            let delay = self.redial_delay();
            self.enet_redial_at = Some(Instant::now() + delay);
        }
        if self.session_ended.load(Ordering::SeqCst) {
            // VAPP-169: not terminal: the loop keeps redialing under it.
            self.phases.set(ViewerPhase::device_offline());
        } else if self.lost.is_none() {
            self.phases.set(ViewerPhase::Starting);
        }
    }

    /// One frame of the active carrier → the UI. `Some(outcome)` = `bye`.
    fn apply(&mut self, text: &str, path: &'static str) -> Option<Option<String>> {
        let frame = ViewerFrame::parse(text);
        if matches!(frame, Some(ViewerFrame::Activity { .. } | ViewerFrame::ActivityReset)) {
            if !self.phases.is(&ViewerPhase::Live) {
                self.phases.set(ViewerPhase::Live);
                self.backoff.reset();
            }
            if self.stats.first.is_none() {
                self.stats.first = Some((self.stats.opened.elapsed().as_millis() as u64, path));
                self.stats.settle();
            }
        }
        let event = match frame? {
            ViewerFrame::Activity { event, seq } => ViewerEvent::Activity(seq, event),
            ViewerFrame::ActivityReset => ViewerEvent::Reset,
            ViewerFrame::ActivitySynced { first_seq, truncated, .. } => {
                ViewerEvent::Synced { first_seq, truncated: truncated.unwrap_or(false) }
            }
            ViewerFrame::HistoryChunk { request_id, events, seqs, done } => {
                // A `None` slot is an event kind this build does not know
                // (compat review R6 #3): dropped AFTER the zip so the
                // numbering of its neighbours stays right.
                let events = events
                    .into_iter()
                    .enumerate()
                    .filter_map(|(ix, event)| Some((seqs.get(ix).copied(), event?)))
                    .collect();
                ViewerEvent::HistoryPage { request_id, events, done }
            }
            // Counted by the leg's liveness; forwarded because EXP-656
            // uses it as an end-of-replay signal.
            ViewerFrame::Keepalive => ViewerEvent::Keepalive,
            // eNet has no close frame: a `bye` IS the end.
            ViewerFrame::Bye { outcome } => return Some(outcome.filter(|o| o != "ended")),
            // The legs turn an `error` into their drop.
            ViewerFrame::Error { .. } => return None,
        };
        let _ = self.events_tx.send(event);
        None
    }

    /// A wakeup nudge: a pending redial or reopen runs now. A healthy feed
    /// is left alone (its legs watch their own staleness).
    fn kick(&mut self) {
        let now = Instant::now();
        if self.enet_leg.is_none() && self.carriers.active != Some(Carrier::Enet) {
            self.enet_redial_at = Some(now);
        }
        if self.relay_reopen_at.is_some() {
            self.relay_reopen_at = Some(now);
        }
    }

    fn timers(&mut self) {
        let now = Instant::now();
        if self.stall_at.is_some_and(|at| at <= now) {
            self.stall_at = None;
            self.step(CarrierEvent::Stall);
        }
        if self.enet_redial_at.is_some_and(|at| at <= now) {
            self.enet_redial_at = None;
            if redial_is_a_start(self.carriers, self.relay_reopen_at.is_some()) {
                self.step(CarrierEvent::Start);
            } else if self.carriers.active != Some(Carrier::Enet) {
                self.spawn_enet();
            }
        }
        if self.relay_reopen_at.is_some_and(|at| at <= now) {
            self.relay_reopen_at = None;
            // The reopen is the stall's: nothing active, no relay leg.
            self.step(CarrierEvent::Stall);
        }
        if !self.stats.settled && now >= self.stats.opened + self.timings.settle_after {
            self.stats.settle();
        }
    }

    /// The viewer closes: both legs leave, the `ended` view is recorded.
    fn close(&mut self) {
        for leg in [self.enet_leg.take(), self.relay_leg.take()].into_iter().flatten() {
            let _ = leg.out.send(LegOut::Close);
        }
        if self.connected.swap(false, Ordering::SeqCst) {
            let _ = self.events_tx.send(ViewerEvent::Connected(false));
        }
        self.carrier_flag.store(carrier_code(None), Ordering::SeqCst);
        if self.stats.first.is_none() && !self.stats.settled {
            // Closed before anything settled: the view still counts.
            self.stats.settle();
        }
        self.stats.ended();
    }
}

/// #EXP-1679 (`fallback` STARTING REDIAL): a redial with nothing active, no
/// relay leg and no relay reopen due is a new round, a `start` with its stall.
/// Without it the relay's next try waits for this eNet attempt to fail: up to
/// `sendTimeoutMs` + `joinTimeoutMs` per endpoint, on top of the backoff.
fn redial_is_a_start(carriers: CarrierState, relay_reopen_due: bool) -> bool {
    carriers.active.is_none() && !carriers.relay_open && !relay_reopen_due
}

fn carrier_code(carrier: Option<Carrier>) -> u8 {
    match carrier {
        None => 0,
        Some(Carrier::Enet) => 1,
        Some(Carrier::Relay) => 2,
    }
}

// ---------------------------------------------------------------------------
// The legs
// ---------------------------------------------------------------------------

/// One carrier's link to the device.
enum Wire {
    Enet(EnetLink),
    Relay(RelayLink),
}

impl Wire {
    async fn next(&mut self) -> Result<String, LegFailure> {
        match self {
            Wire::Enet(link) => link.next().await.ok_or(LegFailure::Dropped("closed")),
            Wire::Relay(link) => link.next().await,
        }
    }

    async fn send(&mut self, frames: Vec<String>) -> bool {
        match self {
            Wire::Enet(link) => link.send(frames).await,
            Wire::Relay(link) => link.send(&frames).await,
        }
    }

    async fn leave(self) {
        match self {
            Wire::Enet(link) => link.leave(),
            Wire::Relay(link) => link.close().await,
        }
    }
}

/// The eNet leg: DISCOVERY, then the feed until it drops.
async fn enet_leg(
    source: Option<Arc<dyn EnetViewerSource>>,
    session_id: String,
    gen: u64,
    timings: ViewerTimings,
    msgs: flume::Sender<LegMsg>,
    out: flume::Receiver<LegOut>,
) {
    match join_over_enet(source.as_ref(), &session_id, timings.join_timeout).await {
        Ok(link) => {
            let path = conn_stats::enet_path(link.node.chat_path(&link.device));
            let synced_by = link.synced_by;
            run_leg(Carrier::Enet, gen, Wire::Enet(link), path, synced_by, &timings, &msgs, &out).await;
        }
        Err(class) => {
            let _ = msgs.send(LegMsg::Failed { carrier: Carrier::Enet, gen, failure: LegFailure::NoFeed(class) });
        }
    }
}

/// The relay leg: the tree key, a tunnel ticket, the sealed join, then the
/// feed until it drops.
async fn relay_leg(
    source: Option<Arc<dyn EnetViewerSource>>,
    session_id: String,
    gen: u64,
    timings: ViewerTimings,
    msgs: flume::Sender<LegMsg>,
    out: flume::Receiver<LegOut>,
) {
    let keys = match relay_keys(source.as_ref(), &session_id).await {
        Ok(keys) => keys,
        Err(failure) => {
            log::debug!("steer viewer: no relay leg for {session_id}: {failure:?}");
            let _ = msgs.send(LegMsg::Failed { carrier: Carrier::Relay, gen, failure });
            return;
        }
    };
    let _ = msgs.send(LegMsg::RelayAvailable { gen });
    match open_tunnel(source, &session_id, keys).await {
        Ok(link) => {
            log::info!("steer viewer: relay tunnel open for {session_id}");
            let deadline = Instant::now() + timings.join_timeout;
            let path = conn_stats::PATH_SERVER_FALLBACK;
            run_leg(Carrier::Relay, gen, Wire::Relay(link), path, deadline, &timings, &msgs, &out).await;
        }
        Err(failure) => {
            log::debug!("steer viewer: no relay tunnel for {session_id}: {failure:?}");
            let _ = msgs.send(LegMsg::Failed { carrier: Carrier::Relay, gen, failure });
        }
    }
}

/// One joined leg's select loop: frames to the loop, steering out, its own
/// staleness and `activity_synced` deadline.
#[allow(clippy::too_many_arguments)]
async fn run_leg(
    carrier: Carrier,
    gen: u64,
    mut wire: Wire,
    path: &'static str,
    deadline: Instant,
    timings: &ViewerTimings,
    msgs: &flume::Sender<LegMsg>,
    out: &flume::Receiver<LegOut>,
) {
    let mut liveness = tokio::time::interval(timings.liveness_tick);
    liveness.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Any inbound frame proves the feed — including the device's
    // `keepalive` beat, which exists for exactly this (EXP-648).
    let mut last_rx = Instant::now();
    let mut synced_by = Some(deadline);
    let failure = loop {
        tokio::select! {
            cmd = out.recv_async() => match cmd {
                Ok(LegOut::Frames(frames)) => {
                    if !wire.send(frames).await {
                        break LegFailure::Dropped("send_failed");
                    }
                }
                Ok(LegOut::Join) => {
                    if !wire.send(vec![ClientFrame::join().to_json()]).await {
                        break LegFailure::Dropped("send_failed");
                    }
                    synced_by = Some(Instant::now() + timings.join_timeout);
                }
                Ok(LegOut::Close) | Err(_) => {
                    wire.leave().await;
                    return;
                }
            },
            frame = wire.next() => {
                let text = match frame {
                    Ok(text) => text,
                    Err(failure) => break failure,
                };
                last_rx = Instant::now();
                let tag = frame_tag(&text);
                match tag.as_deref() {
                    Some("error") => {
                        log::debug!("steer viewer: the device stopped serving the session over {carrier:?}: {text}");
                        let unauthorized = text.contains(enet_steer::ERROR_UNAUTHORIZED);
                        break LegFailure::Dropped(if unauthorized { "unauthorized" } else { "closed" });
                    }
                    Some("activity_synced") => synced_by = None,
                    _ => {}
                }
                let path = (tag.as_deref() == Some("activity_reset")).then_some(path);
                let bye = tag.as_deref() == Some("bye");
                let _ = msgs.send(LegMsg::Frame { carrier, gen, text, path });
                if bye {
                    // The device dropped this viewer with it.
                    return;
                }
            },
            _ = liveness.tick() => {
                // EXP-648: silence past two device keepalives means the feed
                // is dead, not that the agent is parked on a question.
                if last_rx.elapsed() > timings.live_stale {
                    break LegFailure::Dropped("stale");
                }
                if synced_by.is_some_and(|by| Instant::now() > by) {
                    break LegFailure::Dropped("join_timeout");
                }
            }
        }
    };
    let _ = msgs.send(LegMsg::Failed { carrier, gen, failure });
}

fn frame_tag(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| value.get("t").and_then(|t| t.as_str()).map(str::to_string))
}

/// A joined eNet feed: the device's frames arrive on `sub`; `held` = what
/// arrived while joining, delivered first.
struct EnetLink {
    node: Arc<enet_core::Node>,
    device: enet_core::EndpointId,
    session_id: String,
    sub: enet_steer::ViewerSubscription,
    held: VecDeque<String>,
    /// When this endpoint's `activity_synced` is due.
    synced_by: Instant,
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

    /// Tell the device we `leave` (best effort). EXP-1487: not while
    /// another viewer here still reads this session from this device: the
    /// device keys viewers by endpoint, so it would end that feed too.
    fn leave(&self) {
        if self.sub.shared() {
            return;
        }
        let (node, device, session_id) = (Arc::clone(&self.node), self.device, self.session_id.clone());
        tokio::spawn(async move {
            let _ = enet_steer::send_envelope(&node, device, &session_id, &[LEAVE_FRAME.to_string()]).await;
        });
    }
}

const LEAVE_FRAME: &str = r#"{"t":"leave"}"#;

/// DISCOVERY: `join` each install running the session, newest first, until
/// one answers with its `activity_reset` within `join_timeout` (the
/// replay then streams; its `activity_synced` is due by the same deadline).
/// #EXP-1679: the deadline starts at THAT endpoint's join, as on the other
/// clients: counted from the leg's start, discovery and an earlier unanswered
/// endpoint spent it, and the leg dropped a feed that had just answered.
/// `Err` = NO FEED, with its class.
async fn join_over_enet(
    source: Option<&Arc<dyn EnetViewerSource>>,
    session_id: &str,
    join_timeout: Duration,
) -> Result<EnetLink, Class> {
    let source = source.ok_or("no_node")?;
    let node = source.node().ok_or("no_node")?;
    let lookup = Arc::clone(source);
    let id = session_id.to_string();
    let endpoints = match tokio::task::spawn_blocking(move || lookup.endpoints(&id)).await {
        Ok(Ok(endpoints)) => endpoints,
        Ok(Err(err)) => {
            log::debug!("steer viewer: installs.forSession failed: {err}");
            return Err("other");
        }
        Err(_) => return Err("other"),
    };
    let own = node.endpoint_id();
    let join = ClientFrame::join().to_json();
    let mut failure = "no_endpoint";
    // #EXP-1678: the last endpoint that did NOT answer (send failed, timed
    // out): `refused` / `unauthorized` only when every endpoint answered.
    let mut unanswered: Option<Class> = None;
    for device in endpoints.iter().filter_map(|hex| crate::enet::parse_endpoint(hex)).filter(|id| *id != own) {
        let sub = enet_steer::subscribe(session_id, device);
        if let Err(err) = enet_steer::send_envelope(&node, device, session_id, &[join.clone()]).await {
            log::debug!("steer viewer: eNet join to {} failed: {err}", crate::enet::endpoint_hex(&device));
            unanswered = Some("send_failed");
            continue;
        }
        let deadline = tokio::time::Instant::now() + join_timeout;
        let synced_by = Instant::now() + join_timeout;
        loop {
            let Ok(Ok(frame)) = tokio::time::timeout_at(deadline, sub.frames.recv_async()).await else {
                log::debug!("steer viewer: eNet join to {} unanswered", crate::enet::endpoint_hex(&device));
                unanswered = Some("join_timeout");
                break;
            };
            match frame_tag(&frame).as_deref() {
                Some("error") => {
                    log::debug!("steer viewer: {} refused the eNet join: {frame}", crate::enet::endpoint_hex(&device));
                    failure = if frame.contains(enet_steer::ERROR_UNAUTHORIZED) { "unauthorized" } else { "refused" };
                    break;
                }
                Some("bye") => {
                    log::debug!("steer viewer: {} said bye to the eNet join", crate::enet::endpoint_hex(&device));
                    failure = "refused";
                    break;
                }
                Some("activity_reset") => {
                    log::info!("steer viewer: joined {session_id} over eNet");
                    let held = VecDeque::from([frame]);
                    return Ok(EnetLink { node, device, session_id: session_id.to_string(), sub, held, synced_by });
                }
                // A frame of an earlier feed of this pair: not this join's.
                _ => {}
            }
        }
    }
    Err(unanswered.unwrap_or(failure))
}

/// A joined sealed relay tunnel (`steer-enet.json` `fallback`): the tunnel
/// socket, the tree key, and what one sealed envelope held beyond the frame
/// returned.
struct RelayLink {
    ws: crate::WsStream,
    keys: TunnelKeys,
    session_id: String,
    held: VecDeque<String>,
}

impl RelayLink {
    async fn next(&mut self) -> Result<String, LegFailure> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        loop {
            if let Some(frame) = self.held.pop_front() {
                return Ok(frame);
            }
            match self.ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                    match value.get("t").and_then(|t| t.as_str()) {
                        Some("sealed") => {
                            let data = value.get("data").and_then(|d| d.as_str()).unwrap_or_default();
                            match tunnel::open(&self.keys, &self.session_id, data) {
                                Some(envelope) => self.held.extend(envelope.frames.iter().map(|f| f.to_string())),
                                None => log::debug!("steer viewer: a sealed frame did not open; dropped"),
                            }
                        }
                        Some("tunnel_error") => {
                            let code = value.get("code").and_then(|c| c.as_str()).unwrap_or("other");
                            return Err(LegFailure::Unavailable(conn_stats::error_class(code)));
                        }
                        _ => {}
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(LegFailure::Dropped("relay_closed")),
                Some(Ok(_)) => {}
            }
        }
    }

    /// One sealed envelope of `frames`. `false` = gone.
    async fn send(&mut self, frames: &[String]) -> bool {
        use futures_util::SinkExt;
        let body = enet_steer::envelope_bytes(&self.session_id, frames);
        let Some(data) = tunnel::seal(&self.keys, &body) else {
            log::warn!("steer viewer: an envelope ({} bytes) did not seal; dropped", body.len());
            return true;
        };
        let frame = serde_json::json!({ "t": "sealed", "data": data }).to_string();
        self.ws.send(tokio_tungstenite::tungstenite::Message::text(frame)).await.is_ok()
    }

    async fn close(mut self) {
        let _ = self.send(&[LEAVE_FRAME.to_string()]).await;
        let _ = self.ws.close(None).await;
    }
}

/// The relay leg's AVAILABILITY on this install (before any network): a
/// node and this install's tree secret.
async fn relay_keys(source: Option<&Arc<dyn EnetViewerSource>>, session_id: &str) -> Result<TunnelKeys, LegFailure> {
    source.ok_or(LegFailure::Unavailable("no_node"))?;
    let id = session_id.to_string();
    let keys = tokio::task::spawn_blocking(move || tunnel::resolve_keys(&id)).await.ok().flatten();
    keys.ok_or(LegFailure::Unavailable("no_key"))
}

/// The relay leg's dial: `steer.tunnelTicket`, the dial, the sealed `join`.
async fn open_tunnel(
    source: Option<Arc<dyn EnetViewerSource>>,
    session_id: &str,
    keys: TunnelKeys,
) -> Result<RelayLink, LegFailure> {
    let source = source.ok_or(LegFailure::Unavailable("no_node"))?;
    let id = session_id.to_string();
    let ticket = tokio::task::spawn_blocking(move || source.tunnel_ticket(&id)).await;
    let url = match ticket {
        Ok(Ok(TunnelTicketResult::Ticket(ticket))) => ticket.url,
        Ok(Ok(TunnelTicketResult::Disabled | TunnelTicketResult::Unavailable)) => {
            return Err(LegFailure::Unavailable("device_offline"))
        }
        Ok(Err(err)) => {
            log::debug!("steer viewer: steer.tunnelTicket failed: {err}");
            return Err(LegFailure::Dropped("ticket_failed"));
        }
        Err(_) => return Err(LegFailure::Dropped("ticket_failed")),
    };
    let ws = match crate::dial(&url).await {
        Ok(ws) => ws,
        Err(err) => {
            log::debug!("steer viewer: tunnel dial failed: {err:?}");
            return Err(LegFailure::Dropped("relay_closed"));
        }
    };
    let mut link = RelayLink { ws, keys, session_id: session_id.to_string(), held: VecDeque::new() };
    if !link.send(&[ClientFrame::join().to_json()]).await {
        return Err(LegFailure::Dropped("relay_closed"));
    }
    Ok(link)
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

    /// #EXP-1678: `fallback.startingRetry` + `startingRetryCases`.
    #[test]
    fn starting_retry_cases() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json")).unwrap();
        let retry = &fixture["fallback"]["startingRetry"];
        assert_eq!(retry["baseMs"], STARTING_RETRY_BASE_MS);
        assert_eq!(retry["maxMs"], STARTING_RETRY_MAX_MS);
        let timings = ViewerTimings::default();
        for case in fixture["startingRetryCases"].as_array().unwrap() {
            let retries = case["retries"].as_u64().unwrap() as u32;
            let cap = case["capMs"].as_u64().unwrap();
            assert_eq!(starting_retry_cap_ms(retries), cap, "retries {retries}");
            // The viewer's backoff: the n-th delay lies in [cap/2, cap).
            let mut backoff = Backoff::new(timings.retry_base, timings.retry_cap);
            for _ in 0..retries {
                backoff.next_delay_equal_jitter();
            }
            assert_eq!(backoff.bound(), Duration::from_millis(cap), "retries {retries}");
            let delay = backoff.next_delay_equal_jitter();
            assert!(delay >= Duration::from_millis(cap / 2) && delay < Duration::from_millis(cap), "retries {retries}");
        }
    }

    /// #EXP-1679: only an idle redial is a new round.
    #[test]
    fn an_idle_redial_is_a_start() {
        let idle = CarrierState::INITIAL;
        assert!(redial_is_a_start(idle, false));
        assert!(!redial_is_a_start(idle, true), "the relay's own reopen is due");
        assert!(!redial_is_a_start(CarrierState { active: None, relay_open: true }, false));
        assert!(!redial_is_a_start(CarrierState { active: Some(Carrier::Relay), relay_open: true }, false));
        assert!(!redial_is_a_start(CarrierState { active: Some(Carrier::Enet), relay_open: false }, false));
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
