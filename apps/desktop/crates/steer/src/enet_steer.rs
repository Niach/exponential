//! VAPP-33: steer over eNet's chat channel (`exp/enet/chat/1`) between a
//! viewer install and the install RUNNING the session.
//! `packages/domain-contract/fixtures/steer-enet.json` is the spec: one chat
//! message = one envelope `{t:"exp-steer", v:1, sessionId, frames}` carrying
//! the relay's steer frames unchanged.
//!
//! Device half: every live publisher registers its session here
//! ([`register_room`]) for the run's lifetime; [`receive`] (called by
//! `enet.rs` for every chat message) checks the access rule against the
//! session team's registry list and hands join/leave/client frames to that
//! publisher's loop as [`PublisherCmd`]s. The loop owns the [`Feeds`]: one
//! queue + send task per joined viewer, batched per the fixture, with a
//! keepalive, dropped on a failed or timed-out send, `bye` at run end.
//!
//! VAPP-43 STORED REPLAY: a `join` for a session with no live room here is
//! answered from its journal file when the host holds one ([`StoredJournals`]):
//! the replay, then `bye {outcome:"history"}` for an ended row; an idle row's
//! viewers stay joined (keepalive, `history_page` off the file) until the
//! run's publisher comes back here, which takes them over as fresh joins.
//!
//! P2P FALLBACK (`fallback`): a sealed relay TUNNEL (`tunnel.rs`) is served
//! exactly like an eNet viewer endpoint: a [`FeedPeer::Tunnel`] in the same
//! feeds, the same access rule (its user = the ticket's), the same stored
//! replay; only its envelopes travel sealed over the control socket.
//!
//! Viewer half: a viewer subscribes to `(sessionId, device endpoint)`
//! ([`subscribe`]) and gets the device's frames as JSON text, exactly what a
//! relay socket would have delivered; [`send_envelope`] carries its frames.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use coding::SteerAudience;
use enet_core::{EndpointId, Node};
use serde_json::Value;

use crate::frames::{ActivityEvent, ClientFrame};
use crate::journal::ActivityJournal;
use crate::publisher::PublisherCmd;

/// The envelope's type tag.
pub const TAG: &str = "exp-steer";
/// The envelope version.
pub const VERSION: u64 = 1;
/// One envelope's serialized budget (frames are packed while it holds).
pub const BATCH_MAX_BYTES: usize = 262_144;
/// enet-core's chat cap: an envelope over it is never sent.
pub const CHAT_MAX_BYTES: usize = enet_core::MAX_CHAT_MESSAGE;
/// The device's beat to every joined viewer.
pub const KEEPALIVE: Duration = Duration::from_millis(30_000);
/// A viewer that hears nothing for this long re-sends `join`.
pub const STALE: Duration = Duration::from_millis(75_000);
/// EXP-1487 (`lostBanner`): a dropped live feed redials silently; it reads
/// "Connection lost" only once down this long AND after
/// [`LOST_FAILED_REDIALS`] failed redials since the drop.
pub const LOST_GRACE: Duration = Duration::from_millis(5_000);
pub const LOST_FAILED_REDIALS: u32 = 2;

/// EXP-1487 (TS `steerShowConnectionLost`): whether a feed `down` since its
/// drop, with `failed_redials` failed redials since, shows "Connection lost".
pub fn show_connection_lost(down: Duration, failed_redials: u32) -> bool {
    down >= LOST_GRACE && failed_redials >= LOST_FAILED_REDIALS
}

/// EXP-1487 (`noFeed`): what an empty transcript says while NO FEED holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoFeedState {
    /// The ended row's device-offline copy.
    Offline,
    /// #EXP-1703: a live row whose host device is offline in presence
    /// ([`no_feed_away`]; no spinner).
    Away,
    /// A turn is running: "The agent is starting…".
    Starting,
    /// Idle or waiting on a question: never "starting".
    Idle,
}

/// EXP-1487 (TS `steerNoFeedState` with the device online).
pub fn no_feed_state(ended: bool, busy: bool) -> NoFeedState {
    no_feed_state_on(ended, busy, false)
}

/// EXP-1487 / #EXP-1703 (TS `steerNoFeedState`): ended first, then a host
/// device offline in presence (`away`), then busy = starting, else idle.
pub fn no_feed_state_on(ended: bool, busy: bool, device_offline: bool) -> NoFeedState {
    match (ended, device_offline, busy) {
        (true, _, _) => NoFeedState::Offline,
        (false, true, _) => NoFeedState::Away,
        (false, false, true) => NoFeedState::Starting,
        (false, false, false) => NoFeedState::Idle,
    }
}

/// #EXP-1703 (`noFeedAway`): the away empty state's copy; `{device}` in the
/// title, an unknown device = [`NO_FEED_AWAY_DEVICE`].
pub const NO_FEED_AWAY_TITLE: &str = "{device} is offline";
pub const NO_FEED_AWAY_TEXT: &str = "This session continues when it's back. Its transcript loads then.";
pub const NO_FEED_AWAY_DEVICE: &str = "Its device";

/// #EXP-1703 (TS `steerNoFeedAway`): `(title, text)` for `device_name`
/// (`None` / empty = [`NO_FEED_AWAY_DEVICE`]).
pub fn no_feed_away(device_name: Option<&str>) -> (String, &'static str) {
    let device = device_name.filter(|name| !name.is_empty()).unwrap_or(NO_FEED_AWAY_DEVICE);
    (NO_FEED_AWAY_TITLE.replace("{device}", device), NO_FEED_AWAY_TEXT)
}

/// EXP-1487 (`noFeedIdle`): the idle empty state's copy.
pub const NO_FEED_IDLE_TITLE: &str = "No live transcript";
pub const NO_FEED_IDLE_TEXT: &str =
    "The device holding this session isn't publishing right now. Its conversation stays in the chat.";

/// A join with no `activity_synced` within this long = no feed from that
/// endpoint.
pub const JOIN_TIMEOUT: Duration = Duration::from_millis(8_000);
/// A device send that is not acked within this long drops the viewer.
pub const SEND_TIMEOUT: Duration = Duration::from_millis(20_000);

/// The frames a viewer sends.
pub const VIEWER_FRAMES: &[&str] = &[
    "join", "leave", "input", "answer", "interrupt", "unqueue", "set_config", "set_mode", "kill", "history_page",
];
/// The frames a device sends.
pub const DEVICE_FRAMES: &[&str] =
    &["activity_reset", "activity", "activity_synced", "history_chunk", "keepalive", "bye", "error"];
/// This install neither runs the session live nor holds its journal.
pub const ERROR_NO_SUCH_SESSION: &str = "no_such_session";
/// The access rule refused the sender.
pub const ERROR_UNAUTHORIZED: &str = "unauthorized";

// ---------------------------------------------------------------------------
// The contract's pure rules (replayed against the fixture below)
// ---------------------------------------------------------------------------

/// One parsed envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub session_id: String,
    /// Wire frame objects, each with a string `t`.
    pub frames: Vec<Value>,
}

impl Envelope {
    /// The frames' `t` tags, in order.
    pub fn tags(&self) -> Vec<&str> {
        self.frames.iter().filter_map(frame_tag).collect()
    }

    /// Device → viewer (any frame a device sends) rather than viewer → device.
    fn from_device(&self) -> bool {
        self.tags().iter().any(|tag| DEVICE_FRAMES.contains(tag))
    }
}

fn frame_tag(frame: &Value) -> Option<&str> {
    frame.get("t").and_then(Value::as_str)
}

/// A chat body → the envelope, or `None` when it is none (the parse rules).
pub fn parse_envelope(body: &[u8]) -> Option<Envelope> {
    if !body.starts_with(b"{") {
        return None;
    }
    envelope_from(serde_json::from_slice(body).ok()?)
}

/// The parse rules over an already-parsed JSON value.
pub fn envelope_from(value: Value) -> Option<Envelope> {
    let Value::Object(mut object) = value else { return None };
    if object.get("t").and_then(Value::as_str) != Some(TAG) || object.get("v").and_then(Value::as_u64) != Some(VERSION)
    {
        return None;
    }
    let session_id = object.get("sessionId").and_then(Value::as_str).filter(|id| !id.is_empty())?.to_string();
    let Some(Value::Array(frames)) = object.remove("frames") else { return None };
    if frames.is_empty() || !frames.iter().all(|frame| frame.is_object() && frame_tag(frame).is_some()) {
        return None;
    }
    Some(Envelope { session_id, frames })
}

/// The serialized envelope around already-serialized frames (each a JSON
/// object). Byte-for-byte what [`batches`] sizes: overhead + frames + commas.
pub fn envelope_bytes(session_id: &str, frames: &[String]) -> Vec<u8> {
    let mut out = envelope_head(session_id);
    for (index, frame) in frames.iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        out.extend_from_slice(frame.as_bytes());
    }
    out.extend_from_slice(b"]}");
    out
}

fn envelope_head(session_id: &str) -> Vec<u8> {
    let id = serde_json::to_string(session_id).expect("a string serializes");
    format!("{{\"t\":\"{TAG}\",\"v\":{VERSION},\"sessionId\":{id},\"frames\":[").into_bytes()
}

/// The serialized envelope with an empty frames array.
pub fn envelope_overhead(session_id: &str) -> usize {
    envelope_head(session_id).len() + 2
}

/// The device's access rule for one envelope's sender: `viewer_user_id` =
/// the sender endpoint's user in the SESSION team's registry list (`None` =
/// not in it).
pub fn viewer_allowed(viewer_user_id: Option<&str>, starter_user_id: &str, shared: bool) -> bool {
    match viewer_user_id {
        Some(user) => user == starter_user_id || shared,
        None => false,
    }
}

/// The device's batching over serialized frame sizes: the groups of frame
/// indexes (one envelope each) and the frames too big for any envelope.
pub fn batches(sizes: &[usize], overhead: usize, batch_max: usize, chat_max: usize) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut groups = Vec::new();
    let mut dropped = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut bytes = overhead;
    for (index, &size) in sizes.iter().enumerate() {
        if overhead + size > chat_max {
            dropped.push(index);
            continue;
        }
        let next = bytes + size + usize::from(!current.is_empty());
        if !current.is_empty() && next > batch_max {
            groups.push(std::mem::take(&mut current));
            current.push(index);
            bytes = overhead + size;
            continue;
        }
        current.push(index);
        bytes = next;
    }
    if !current.is_empty() {
        groups.push(current);
    }
    (groups, dropped)
}

fn error_frame(code: &str) -> String {
    serde_json::json!({ "t": "error", "code": code }).to_string()
}

const KEEPALIVE_FRAME: &str = r#"{"t":"keepalive"}"#;

/// Sends one envelope of `frames` (serialized frame objects) to `peer`,
/// waiting for its ack at most [`SEND_TIMEOUT`].
pub async fn send_envelope(node: &Node, peer: EndpointId, session_id: &str, frames: &[String]) -> Result<(), String> {
    let body = envelope_bytes(session_id, frames);
    match tokio::time::timeout(SEND_TIMEOUT, node.send_chat(peer, body)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err("timed out".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Device: the room registry
// ---------------------------------------------------------------------------

/// A viewer endpoint and the node that reaches it (the one it wrote from).
#[derive(Clone)]
pub struct EnetPeer {
    pub id: EndpointId,
    pub node: Arc<Node>,
}

impl std::fmt::Debug for EnetPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EnetPeer({})", crate::enet::endpoint_hex(&self.id))
    }
}

impl PartialEq for EnetPeer {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Who a feed serves: an eNet endpoint, or a relay tunnel (by its id).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PeerId {
    Enet(EndpointId),
    Tunnel(String),
}

/// One viewer of a device feed and how its envelopes reach it.
#[derive(Clone, Debug)]
pub enum FeedPeer {
    Enet(EnetPeer),
    Tunnel(crate::tunnel::TunnelPeer),
}

impl PartialEq for FeedPeer {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl FeedPeer {
    pub fn id(&self) -> PeerId {
        match self {
            FeedPeer::Enet(peer) => PeerId::Enet(peer.id),
            FeedPeer::Tunnel(peer) => PeerId::Tunnel(peer.tunnel_id.clone()),
        }
    }

    /// The largest serialized envelope this carrier takes.
    fn envelope_max(&self) -> usize {
        match self {
            FeedPeer::Enet(_) => CHAT_MAX_BYTES,
            FeedPeer::Tunnel(_) => crate::tunnel::TUNNEL_ENVELOPE_MAX_BYTES,
        }
    }

    /// One envelope of `frames`: eNet waits for the ack (at most
    /// [`SEND_TIMEOUT`]); a tunnel queues it sealed on the control socket.
    async fn send(&self, session_id: &str, frames: &[String]) -> Result<(), String> {
        match self {
            FeedPeer::Enet(peer) => send_envelope(&peer.node, peer.id, session_id, frames).await,
            FeedPeer::Tunnel(peer) => peer.send(&envelope_bytes(session_id, frames)),
        }
    }

    /// `connStats` path: the eNet link's (direct or through an iroh relay),
    /// or the sealed relay fallback.
    fn path(&self) -> &'static str {
        match self {
            FeedPeer::Enet(peer) => crate::conn_stats::enet_path(peer.node.chat_path(&peer.id)),
            FeedPeer::Tunnel(_) => crate::conn_stats::PATH_SERVER_FALLBACK,
        }
    }

    fn label(&self) -> String {
        match self {
            FeedPeer::Enet(peer) => crate::enet::endpoint_hex(&peer.id),
            FeedPeer::Tunnel(peer) => format!("tunnel {}", peer.tunnel_id),
        }
    }
}

struct Room {
    generation: u64,
    audience: SteerAudience,
    /// Weak: the room never keeps a finished publisher's channel open.
    cmd_tx: flume::WeakSender<PublisherCmd>,
}

fn rooms() -> &'static Mutex<HashMap<String, Room>> {
    static ROOMS: OnceLock<Mutex<HashMap<String, Room>>> = OnceLock::new();
    ROOMS.get_or_init(Default::default)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

static GENERATION: AtomicU64 = AtomicU64::new(1);

/// One publisher's entry in the room registry, removed on drop (only if no
/// newer publisher for the same session replaced it).
pub(crate) struct RoomGuard {
    session_id: String,
    generation: u64,
}

impl Drop for RoomGuard {
    fn drop(&mut self) {
        let mut rooms = lock(rooms());
        if rooms.get(&self.session_id).is_some_and(|room| room.generation == self.generation) {
            rooms.remove(&self.session_id);
        }
    }
}

/// Registers the live publisher of `session_id`: eNet viewers of the session
/// reach its loop through `cmd_tx`.
pub(crate) fn register_room(
    session_id: &str,
    audience: SteerAudience,
    cmd_tx: flume::WeakSender<PublisherCmd>,
) -> RoomGuard {
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    let live = cmd_tx.upgrade();
    lock(rooms()).insert(session_id.to_string(), Room { generation, audience, cmd_tx });
    // VAPP-43: the run came back — its stored viewers become live joins.
    let stored = lock(stored_viewers()).remove(session_id);
    if let (Some(viewers), Some(tx)) = (stored, live) {
        for (_, (peer, _feed)) in viewers {
            let _ = tx.send(PublisherCmd::Join(peer));
        }
    }
    RoomGuard { session_id: session_id.to_string(), generation }
}

/// Is a live publisher of `session_id` serving eNet viewers here?
pub fn has_room(session_id: &str) -> bool {
    lock(rooms()).contains_key(session_id)
}

/// One chat message from `from`, received on `node`. Returns whether it was
/// a steer envelope (the caller handles anything else). `user_in_team`
/// resolves `(team id, endpoint)` to the endpoint's user in that team's
/// registry list; `stored` = the host's journals for STORED REPLAY.
pub fn receive(
    node: &Arc<Node>,
    from: EndpointId,
    body: &[u8],
    user_in_team: &dyn Fn(&str, &EndpointId) -> Option<String>,
    stored: Option<&StoredJournals>,
) -> bool {
    let Some(envelope) = parse_envelope(body) else { return false };
    if envelope.from_device() {
        deliver_to_viewer(from, envelope);
        return true;
    }
    let peer = FeedPeer::Enet(EnetPeer { id: from, node: Arc::clone(node) });
    serve_envelope(peer, envelope, &|team_id| user_in_team(team_id, &from), stored);
    true
}

/// P2P FALLBACK: one opened envelope off a relay tunnel, served like an eNet
/// viewer's (`user_id` = the tunnel's, from its server-minted ticket).
pub fn receive_tunnel(peer: &crate::tunnel::TunnelPeer, user_id: &str, envelope: Envelope) {
    if envelope.from_device() {
        return;
    }
    let stored = lock(tunnel_stored()).clone();
    serve_envelope(FeedPeer::Tunnel(peer.clone()), envelope, &|_| Some(user_id.to_string()), stored.as_ref());
}

/// The relay closed a tunnel (or its control socket went): its viewer
/// leaves the session's feed.
pub fn close_tunnel(session_id: &str, tunnel_id: &str) {
    let id = PeerId::Tunnel(tunnel_id.to_string());
    let cmd_tx = lock(rooms()).get(session_id).and_then(|room| room.cmd_tx.upgrade());
    if let Some(cmd_tx) = cmd_tx {
        let _ = cmd_tx.send(PublisherCmd::Leave(id.clone()));
    }
    if let Some(viewers) = lock(stored_viewers()).get_mut(session_id) {
        viewers.remove(&id);
    }
}

/// The host's stored journals for tunnel viewers (the eNet node's own, set
/// beside it: `InstallNode::serve_stored`).
fn tunnel_stored() -> &'static Mutex<Option<StoredJournals>> {
    static STORED: OnceLock<Mutex<Option<StoredJournals>>> = OnceLock::new();
    STORED.get_or_init(Default::default)
}

pub(crate) fn set_tunnel_stored(stored: StoredJournals) {
    *lock(tunnel_stored()) = Some(stored);
}

/// One viewer envelope from `peer`, whatever carried it: the access rule
/// (`user_of(team)` = the sender's user in that team), then the live room's
/// loop or STORED REPLAY.
fn serve_envelope(
    peer: FeedPeer,
    envelope: Envelope,
    user_of: &dyn Fn(&str) -> Option<String>,
    stored: Option<&StoredJournals>,
) {
    let session_id = envelope.session_id.clone();
    let room = lock(rooms())
        .get(&session_id)
        .and_then(|room| Some((room.audience.clone(), room.cmd_tx.upgrade()?)));
    let Some((audience, cmd_tx)) = room else {
        match stored.and_then(|stored| Some((stored, stored.lookup(&session_id)?))) {
            Some((stored, session)) => {
                if allowed(&peer, &session_id, &session.audience, user_of) {
                    serve_stored(peer, envelope, stored, session.ended);
                }
            }
            None => reply(&peer, &session_id, ERROR_NO_SUCH_SESSION),
        }
        return;
    };
    if !allowed(&peer, &session_id, &audience, user_of) {
        return;
    }
    let from = peer.id();
    for mut frame in envelope.frames {
        let cmd = match frame_tag(&frame) {
            Some("join") => PublisherCmd::Join(peer.clone()),
            Some("leave") => PublisherCmd::Leave(from.clone()),
            Some(tag) if VIEWER_FRAMES.contains(&tag) => {
                // The relay names the room on what it forwards; here the
                // envelope does.
                if let Value::Object(object) = &mut frame {
                    object.entry("sessionId").or_insert_with(|| Value::String(session_id.clone()));
                }
                PublisherCmd::Frame { from: from.clone(), frame: frame.to_string() }
            }
            // A frame this build does not know: skipped, the rest applies.
            _ => continue,
        };
        if cmd_tx.send(cmd).is_err() {
            reply(&peer, &session_id, ERROR_NO_SUCH_SESSION);
            return;
        }
    }
}

/// The access rule for one envelope; a refusal is answered `unauthorized`.
fn allowed(
    peer: &FeedPeer,
    session_id: &str,
    audience: &SteerAudience,
    user_of: &dyn Fn(&str) -> Option<String>,
) -> bool {
    let user = user_of(&audience.team_id);
    if viewer_allowed(user.as_deref(), &audience.starter_user_id, audience.shared) {
        return true;
    }
    log::info!("enet steer: {} refused for session {session_id}", peer.label());
    reply(peer, session_id, ERROR_UNAUTHORIZED);
    false
}

fn reply(peer: &FeedPeer, session_id: &str, code: &str) {
    let peer = peer.clone();
    let session_id = session_id.to_string();
    let frame = error_frame(code);
    tokio::spawn(async move {
        if let Err(err) = peer.send(&session_id, &[frame]).await {
            log::debug!("enet steer: error reply to {} failed: {err}", peer.label());
        }
    });
}

// ---------------------------------------------------------------------------
// Device: the per-viewer feeds (owned by the publisher loop)
// ---------------------------------------------------------------------------

/// The joined viewers of one live publisher.
pub(crate) struct Feeds {
    session_id: String,
    viewers: HashMap<PeerId, flume::Sender<String>>,
    /// The `bye` outcome once the run said how it ended.
    outcome: Option<String>,
    _room: Option<RoomGuard>,
}

impl Feeds {
    pub(crate) fn new(session_id: &str, room: Option<RoomGuard>) -> Feeds {
        Feeds { session_id: session_id.to_string(), viewers: HashMap::new(), outcome: None, _room: room }
    }

    /// `join` (or a re-join): `activity_reset`, the journal replay, then
    /// `activity_synced`. A re-join reuses the viewer's queue, so nothing
    /// queued before it can land after the reset.
    pub(crate) fn join(&mut self, peer: FeedPeer, journal: &ActivityJournal) {
        let id = peer.id();
        let label = peer.label();
        let tx = match self.viewers.get(&id) {
            Some(tx) if !tx.is_disconnected() => tx.clone(),
            _ => {
                let tx = spawn_feed(peer, &self.session_id);
                self.viewers.insert(id, tx.clone());
                tx
            }
        };
        replay_to(&tx, journal);
        log::info!("enet steer: {label} joined {} ({} viewers)", self.session_id, self.viewers.len());
    }

    /// Tests only: a raw viewer queue (no node, no feed task), joined.
    #[cfg(test)]
    pub(crate) fn tap(&mut self, id: PeerId, tx: flume::Sender<String>, journal: &ActivityJournal) {
        replay_to(&tx, journal);
        self.viewers.insert(id, tx);
    }

    pub(crate) fn leave(&mut self, id: &PeerId) {
        if self.viewers.remove(id).is_some() {
            log::info!("enet steer: {id:?} left {}", self.session_id);
        }
    }

    /// One recorded event, live, to every joined viewer.
    pub(crate) fn activity(&mut self, seq: u64, event: &ActivityEvent) {
        if self.viewers.is_empty() {
            return;
        }
        let frame = ClientFrame::Activity { event: event.clone(), seq }.to_json();
        self.viewers.retain(|_, tx| tx.send(frame.clone()).is_ok());
    }

    /// A frame for one viewer only (`history_chunk`, an `error`).
    pub(crate) fn send_to(&mut self, id: &PeerId, frame: String) {
        if let Some(tx) = self.viewers.get(id) {
            if tx.send(frame).is_err() {
                self.viewers.remove(id);
            }
        }
    }

    /// The run ended: remembered for the `bye` every viewer gets.
    pub(crate) fn set_outcome(&mut self, outcome: Option<String>) {
        if self.outcome.is_none() {
            self.outcome = outcome;
        }
    }
}

impl Drop for Feeds {
    /// The run is over: `bye {outcome}` to every viewer; each feed drains
    /// what it holds and ends.
    fn drop(&mut self) {
        let bye = ClientFrame::Bye { outcome: self.outcome.as_deref() }.to_json();
        for (_, tx) in self.viewers.drain() {
            let _ = tx.send(bye.clone());
        }
    }
}

/// `activity_reset`, the journal replay (its REPLAY TAIL, `history::replay_tail`),
/// then `activity_synced` naming the span sent.
fn replay_to(tx: &flume::Sender<String>, journal: &ActivityJournal) {
    let _ = tx.send(ClientFrame::ActivityReset.to_json());
    let (events, cut) = crate::history::replay_tail(journal.replay_seq().collect(), crate::history::REPLAY_TAIL);
    let mut first = None;
    for (seq, event) in events {
        if first.is_none() && !crate::history::is_state_slot(event) {
            first = Some(seq);
        }
        let _ = tx.send(ClientFrame::Activity { event: event.clone(), seq }.to_json());
    }
    let (journal_first, last, truncated) = journal.synced_span();
    let first = first.unwrap_or(journal_first);
    let _ = tx.send(synced_frame(first, last, truncated || cut || first > 0));
}

/// The end-of-replay marker, exactly the relay's (`fixtures/steer-enet.json`).
fn synced_frame(first: u64, last: u64, truncated: bool) -> String {
    let mut frame = serde_json::json!({ "t": "activity_synced", "firstSeq": first, "lastSeq": last });
    if truncated {
        frame["truncated"] = Value::Bool(true);
    }
    frame.to_string()
}

/// A new viewer feed task; its queue's sender.
fn spawn_feed(peer: FeedPeer, session_id: &str) -> flume::Sender<String> {
    let (tx, rx) = flume::unbounded();
    tokio::spawn(run_feed(peer, session_id.to_string(), rx));
    tx
}

// ---------------------------------------------------------------------------
// Device: STORED REPLAY (VAPP-43)
// ---------------------------------------------------------------------------

/// The host's stored transcripts: the data dir whose `journal/` holds them
/// and its synced `sessions` rows (who may watch, and whether it ended).
#[derive(Clone)]
pub struct StoredJournals {
    pub data_dir: PathBuf,
    /// Session id → its synced row's [`StoredSession`]; `None` = no row.
    pub session: Arc<dyn Fn(&str) -> Option<StoredSession> + Send + Sync>,
}

/// What STORED REPLAY reads off a session's synced row.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredSession {
    pub audience: SteerAudience,
    pub ended: bool,
}

impl StoredSession {
    /// `None` when the row names no team or starter.
    pub fn of(row: &domain::rows::SessionRow) -> Option<StoredSession> {
        Some(StoredSession { audience: SteerAudience::of(row)?, ended: row.is_ended() })
    }
}

impl StoredJournals {
    /// The session's row, when its journal file is here too.
    fn lookup(&self, session_id: &str) -> Option<StoredSession> {
        let path = crate::history::journal_path(&self.data_dir, session_id)?;
        if !path.is_file() {
            return None;
        }
        (self.session)(session_id)
    }
}

/// EXP-1531: `node` lost its last connection to `peer` (enet-core
/// `Event::PeerDisconnected`: it hung up, or the QUIC idle timeout found the
/// link dead). Both halves act NOW instead of waiting out a send timeout or
/// the stale timer: as a DEVICE every feed to that viewer is dropped (a send
/// into a dead link would block its queue for `sendTimeoutMs`); as a VIEWER
/// every subscription to that device closes, which the viewer reads as a
/// dropped feed (silent redial, the relay fallback). Skipped when a
/// connection to the peer is already back (a fast re-join raced the event).
/// #EXP-1526: keyed by `PeerId::Enet`, so a relay TUNNEL viewer of the same
/// user is never dropped by an eNet disconnect.
/// Returns how many subscriptions it closed.
pub fn peer_disconnected(node: &Node, peer: EndpointId) -> usize {
    if node.is_connected(&peer) {
        return 0;
    }
    let live: Vec<_> = lock(rooms()).values().filter_map(|room| room.cmd_tx.upgrade()).collect();
    for tx in live {
        let _ = tx.send(PublisherCmd::Leave(PeerId::Enet(peer)));
    }
    for viewers in lock(stored_viewers()).values_mut() {
        viewers.remove(&PeerId::Enet(peer));
    }
    let mut viewers = lock(viewers());
    let keys: Vec<ViewerKey> = viewers.keys().filter(|(_, device)| *device == peer).cloned().collect();
    keys.iter().filter_map(|key| viewers.remove(key)).map(|subs| subs.len()).sum()
}

type StoredViewers = HashMap<String, HashMap<PeerId, (FeedPeer, flume::Sender<String>)>>;

/// The viewers joined to an idle session's journal, until its run comes back
/// ([`register_room`] hands them over).
fn stored_viewers() -> &'static Mutex<StoredViewers> {
    static STORED: OnceLock<Mutex<StoredViewers>> = OnceLock::new();
    STORED.get_or_init(Default::default)
}

/// Is `endpoint` joined to `session_id`'s stored journal here?
pub fn has_stored_viewer(session_id: &str, endpoint: &EndpointId) -> bool {
    let id = PeerId::Enet(*endpoint);
    lock(stored_viewers()).get(session_id).is_some_and(|viewers| viewers.contains_key(&id))
}

/// One allowed envelope for a session served from its journal: `join`,
/// `leave` and `history_page` apply; a stored run cannot be steered.
fn serve_stored(peer: FeedPeer, envelope: Envelope, stored: &StoredJournals, ended: bool) {
    let session_id = envelope.session_id;
    let from = peer.id();
    for frame in envelope.frames {
        match frame_tag(&frame) {
            Some("join") => stored_join(peer.clone(), &session_id, stored.data_dir.clone(), ended),
            Some("leave") => {
                if let Some(viewers) = lock(stored_viewers()).get_mut(&session_id) {
                    viewers.remove(&from);
                }
            }
            Some("history_page") => {
                let page = |key: &str| frame.get(key).and_then(Value::as_u64);
                let request_id = frame.get("requestId").and_then(Value::as_str);
                if let (Some(request_id), Some(before_seq), Some(limit)) =
                    (request_id, page("beforeSeq"), page("limit"))
                {
                    stored_page(&session_id, &from, stored.data_dir.clone(), request_id.to_string(), before_seq, limit);
                }
            }
            _ => {}
        }
    }
}

/// `join` (or a re-join) on a stored journal: reset, the file's replay,
/// synced, then `bye history` for an ended row (the viewer is dropped) or a
/// place in the stored-viewer registry for an idle one.
fn stored_join(peer: FeedPeer, session_id: &str, data_dir: PathBuf, ended: bool) {
    let tx = if ended {
        spawn_feed(peer, session_id)
    } else {
        let mut stored = lock(stored_viewers());
        let viewers = stored.entry(session_id.to_string()).or_default();
        match viewers.get(&peer.id()) {
            Some((_, tx)) if !tx.is_disconnected() => tx.clone(),
            _ => {
                let tx = spawn_feed(peer.clone(), session_id);
                viewers.insert(peer.id(), (peer, tx.clone()));
                tx
            }
        }
    };
    let id = session_id.to_string();
    tokio::spawn(async move {
        let read_id = id.clone();
        let events = tokio::task::spawn_blocking(move || crate::history::read_journal_seq(&data_dir, &read_id))
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let held = events.len();
        let (events, cut) = crate::history::replay_tail(events, crate::history::REPLAY_TAIL);
        log::info!("enet steer: replaying {} of {held} stored event(s) of {id}", events.len());
        for frame in stored_replay(&events, ended, cut) {
            let _ = tx.send(frame);
        }
    });
}

/// The frames a stored join gets: `activity_reset`, the file's replay (its
/// REPLAY TAIL), `activity_synced` (the first/last LOG seq sent; `truncated`
/// when the tail left older events out), and for an ended row
/// `bye {outcome:"history"}`.
pub fn stored_replay(events: &[(u64, ActivityEvent)], ended: bool, truncated: bool) -> Vec<String> {
    let mut frames = vec![ClientFrame::ActivityReset.to_json()];
    frames.extend(events.iter().map(|(seq, event)| ClientFrame::Activity { event: event.clone(), seq: *seq }.to_json()));
    let mut log = events.iter().filter(|(_, event)| !crate::history::is_state_slot(event)).map(|(seq, _)| *seq);
    let first = log.next().unwrap_or(0);
    let last = log.last().unwrap_or(first);
    frames.push(synced_frame(first, last, truncated));
    if ended {
        frames.push(ClientFrame::Bye { outcome: Some(crate::history::HISTORY_OUTCOME) }.to_json());
    }
    frames
}

/// A stored viewer's `history_page`, answered off the file.
fn stored_page(session_id: &str, from: &PeerId, data_dir: PathBuf, request_id: String, before_seq: u64, limit: u64) {
    let Some(tx) = lock(stored_viewers()).get(session_id).and_then(|viewers| viewers.get(from)).map(|(_, tx)| tx.clone())
    else {
        return;
    };
    let id = session_id.to_string();
    tokio::spawn(async move {
        let limit = limit.min(u64::from(crate::history::HISTORY_PAGE_MAX)) as usize;
        let page = tokio::task::spawn_blocking(move || {
            crate::history::read_journal_page(&data_dir, &id, before_seq, limit).unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        let _ = tx.send(crate::history::history_chunk_frame(&request_id, page));
    });
}

/// One viewer's feed: drains its queue into envelopes, beats a keepalive
/// when idle, and ends on a failed send (the viewer is dropped: its queue's
/// sender sees the disconnect) or once the queue closes. Its end is one
/// `steer.serve` (`connStats`): the carrier it served on, for how long.
async fn run_feed(peer: FeedPeer, session_id: String, rx: flume::Receiver<String>) {
    let started = std::time::Instant::now();
    let mut path = None;
    let failure = feed_loop(&peer, &session_id, &rx, &mut path).await;
    let data = crate::conn_stats::ConnData {
        phase: "ended",
        outcome: if path.is_some() { "ok" } else { "failed" },
        path,
        error_class: failure.filter(|_| path.is_none()),
        duration_ms: Some(started.elapsed().as_millis() as u64),
        ..Default::default()
    };
    crate::conn_stats::record(crate::conn_stats::KIND_SERVE, crate::conn_stats::SOURCE_DEVICE, data);
}

/// [`run_feed`]'s loop: `path` = the carrier, once an envelope went out;
/// the error class when a send failed.
async fn feed_loop(
    peer: &FeedPeer,
    session_id: &str,
    rx: &flume::Receiver<String>,
    path: &mut Option<&'static str>,
) -> Option<&'static str> {
    let overhead = envelope_overhead(session_id);
    let envelope_max = peer.envelope_max();
    loop {
        let first = match tokio::time::timeout(KEEPALIVE, rx.recv_async()).await {
            Ok(Ok(frame)) => frame,
            Ok(Err(_)) => return None,
            Err(_) => KEEPALIVE_FRAME.to_string(),
        };
        let mut frames = vec![first];
        while let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        }
        let sizes: Vec<usize> = frames.iter().map(String::len).collect();
        let (groups, dropped) = batches(&sizes, overhead, BATCH_MAX_BYTES, envelope_max);
        for index in dropped {
            log::warn!("enet steer: dropping an oversize frame ({} bytes)", sizes[index]);
        }
        for group in groups {
            let batch: Vec<String> = group.into_iter().map(|index| std::mem::take(&mut frames[index])).collect();
            if let Err(err) = peer.send(session_id, &batch).await {
                log::info!("enet steer: viewer {} of {session_id} dropped: {err}", peer.label());
                return Some("send_failed");
            }
            if path.is_none() {
                *path = Some(peer.path());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Viewer: subscriptions to a device's frames
// ---------------------------------------------------------------------------

type ViewerKey = (String, EndpointId);

/// EXP-1487: every subscriber of one (session, device), by generation. Two
/// viewers of one session on this install (the agent screen's transcript and
/// its root composer) share the device's one feed for this endpoint: a
/// single slot made the second's join evict the first, which rejoined and
/// evicted the second, forever ("Connection lost" flickering).
type Subscribers = Vec<(u64, flume::Sender<String>)>;

fn viewers() -> &'static Mutex<HashMap<ViewerKey, Subscribers>> {
    static VIEWERS: OnceLock<Mutex<HashMap<ViewerKey, Subscribers>>> = OnceLock::new();
    VIEWERS.get_or_init(Default::default)
}

/// One viewer's subscription; dropping it unsubscribes.
pub struct ViewerSubscription {
    key: ViewerKey,
    generation: u64,
    /// The device's frames (JSON text, one per wire frame), in order.
    pub frames: flume::Receiver<String>,
}

impl ViewerSubscription {
    /// Whether another viewer on this install still reads this session from
    /// this device: then a `leave` would end ITS feed too (the device keys
    /// viewers by endpoint), so only the last subscriber leaves.
    pub fn shared(&self) -> bool {
        lock(viewers())
            .get(&self.key)
            .is_some_and(|subs| subs.iter().any(|(generation, _)| *generation != self.generation))
    }
}

impl Drop for ViewerSubscription {
    fn drop(&mut self) {
        let mut viewers = lock(viewers());
        if let Some(subs) = viewers.get_mut(&self.key) {
            subs.retain(|(generation, _)| *generation != self.generation);
            if subs.is_empty() {
                viewers.remove(&self.key);
            }
        }
    }
}

/// Subscribes to the frames `device` sends for `session_id` (envelopes from
/// any other endpoint, or for another session, never reach it). Every
/// subscriber of the pair gets every frame (fixtures/steer-enet.json
/// SUBSCRIPTIONS).
pub fn subscribe(session_id: &str, device: EndpointId) -> ViewerSubscription {
    let (tx, frames) = flume::unbounded();
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    let key = (session_id.to_string(), device);
    lock(viewers()).entry(key.clone()).or_default().push((generation, tx));
    ViewerSubscription { key, generation, frames }
}

fn deliver_to_viewer(from: EndpointId, envelope: Envelope) {
    let key = (envelope.session_id, from);
    let mut viewers = lock(viewers());
    let Some(subs) = viewers.get_mut(&key) else {
        log::debug!("enet steer: frames for {} from an endpoint nobody joined; ignored", key.0);
        return;
    };
    subs.retain(|(_, tx)| envelope.frames.iter().all(|frame| tx.send(frame.to_string()).is_ok()));
    if subs.is_empty() {
        viewers.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    /// EXP-1531: the device's link dying closes every subscription to it at
    /// once (the viewer's "subscription closed" drop), never another
    /// device's; a re-subscribe afterwards works.
    #[tokio::test(flavor = "multi_thread")]
    async fn peer_disconnected_closes_that_devices_subscriptions_at_once() {
        let node = Node::start(enet_core::NetConfig::local_only(), enet_core::Identity::generate()).await.unwrap();
        let device = enet_core::SecretKey::from_bytes(&[11; 32]).public();
        let other = enet_core::SecretKey::from_bytes(&[12; 32]).public();
        let first = subscribe("sess-exp-1531", device);
        let second = subscribe("sess-exp-1531", device);
        let elsewhere = subscribe("sess-exp-1531", other);
        assert_eq!(peer_disconnected(&node, device), 2);
        assert!(first.frames.is_disconnected() && second.frames.is_disconnected());
        assert!(first.frames.recv_async().await.is_err(), "the viewer's next() sees the drop");
        assert!(!elsewhere.frames.is_disconnected());
        drop((first, second));
        let again = subscribe("sess-exp-1531", device);
        deliver_to_viewer(device, Envelope { session_id: "sess-exp-1531".into(), frames: vec![serde_json::json!({"t":"keepalive"})] });
        assert_eq!(again.frames.try_recv().unwrap(), r#"{"t":"keepalive"}"#);
        node.shutdown().await.unwrap();
    }

    /// EXP-1487 (SUBSCRIPTIONS): two viewers of one session share the
    /// device's feed; neither evicts the other, only the last one leaves.
    #[test]
    fn two_subscribers_of_one_session_both_get_every_frame() {
        let device = enet_core::SecretKey::from_bytes(&[7; 32]).public();
        let session = "sess-exp-1487";
        let deliver = |t: &str| {
            let frames = vec![serde_json::json!({ "t": t })];
            deliver_to_viewer(device, Envelope { session_id: session.into(), frames });
        };
        let first = subscribe(session, device);
        assert!(!first.shared());
        let second = subscribe(session, device);
        assert!(first.shared() && second.shared());
        deliver("keepalive");
        assert_eq!(first.frames.try_recv().unwrap(), r#"{"t":"keepalive"}"#);
        assert_eq!(second.frames.try_recv().unwrap(), r#"{"t":"keepalive"}"#);
        drop(second);
        assert!(!first.shared());
        deliver("activity_reset");
        assert_eq!(first.frames.try_recv().unwrap(), r#"{"t":"activity_reset"}"#);
        assert!(!first.frames.is_disconnected());
    }

    #[test]
    fn the_constants_are_the_fixtures() {
        let f = fixture();
        assert_eq!(f["tag"], TAG);
        assert_eq!(f["version"], VERSION);
        assert_eq!(f["batchMaxBytes"], BATCH_MAX_BYTES);
        assert_eq!(f["chatMaxBytes"], CHAT_MAX_BYTES);
        assert_eq!(f["keepaliveMs"], KEEPALIVE.as_millis() as u64);
        assert_eq!(f["staleMs"], STALE.as_millis() as u64);
        assert_eq!(f["joinTimeoutMs"], JOIN_TIMEOUT.as_millis() as u64);
        assert_eq!(f["sendTimeoutMs"], SEND_TIMEOUT.as_millis() as u64);
        assert_eq!(f["noFeedIdle"]["title"], NO_FEED_IDLE_TITLE);
        assert_eq!(f["noFeedIdle"]["text"], NO_FEED_IDLE_TEXT);
        assert_eq!(f["noFeedAway"]["title"], NO_FEED_AWAY_TITLE);
        assert_eq!(f["noFeedAway"]["text"], NO_FEED_AWAY_TEXT);
        assert_eq!(f["noFeedAway"]["device"], NO_FEED_AWAY_DEVICE);
        assert_eq!(no_feed_away(Some("macbook")).0, "macbook is offline");
        assert_eq!(no_feed_away(None).0, "Its device is offline");
        assert_eq!(no_feed_away(Some("")).0, "Its device is offline");
        for case in f["noFeed"].as_array().unwrap() {
            let state = match case["state"].as_str().unwrap() {
                "offline" => NoFeedState::Offline,
                "away" => NoFeedState::Away,
                "starting" => NoFeedState::Starting,
                "idle" => NoFeedState::Idle,
                other => panic!("unknown state {other}"),
            };
            let (ended, busy) = (case["ended"].as_bool().unwrap(), case["busy"].as_bool().unwrap());
            // #EXP-1703: `deviceOffline` absent = false.
            let device_offline = case["deviceOffline"].as_bool().unwrap_or(false);
            assert_eq!(no_feed_state_on(ended, busy, device_offline), state, "{}", case["name"]);
            if !device_offline {
                assert_eq!(no_feed_state(ended, busy), state, "{}", case["name"]);
            }
        }
        assert_eq!(f["lostBanner"]["graceMs"], LOST_GRACE.as_millis() as u64);
        assert_eq!(f["lostBanner"]["failedRedials"], LOST_FAILED_REDIALS);
        for case in f["lostBannerCases"].as_array().unwrap() {
            let down = Duration::from_millis(case["downMs"].as_u64().unwrap());
            let failed = case["failedRedials"].as_u64().unwrap() as u32;
            assert_eq!(show_connection_lost(down, failed), case["show"].as_bool().unwrap(), "{}", case["name"]);
        }
        let list = |key: &str| -> Vec<String> {
            f[key].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
        };
        assert_eq!(list("viewerFrames"), VIEWER_FRAMES);
        assert_eq!(list("deviceFrames"), DEVICE_FRAMES);
        assert_eq!(list("errorCodes"), [ERROR_NO_SUCH_SESSION, ERROR_UNAUTHORIZED]);
    }

    #[test]
    fn access_cases() {
        for case in fixture()["access"].as_array().unwrap() {
            let allowed = viewer_allowed(
                case["viewerUserId"].as_str(),
                case["starterUserId"].as_str().unwrap(),
                case["shared"].as_bool().unwrap(),
            );
            assert_eq!(allowed, case["allowed"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn parse_cases() {
        for case in fixture()["parse"].as_array().unwrap() {
            let body = match case.get("bodyText") {
                Some(text) => text.as_str().unwrap().to_string(),
                None => case["body"].to_string(),
            };
            let parsed = parse_envelope(body.as_bytes());
            assert_eq!(parsed.is_some(), case["ok"].as_bool().unwrap(), "{}", case["name"]);
            if let Some(envelope) = parsed {
                assert_eq!(envelope.session_id, case["sessionId"].as_str().unwrap());
                let types: Vec<&str> = case["types"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
                assert_eq!(envelope.tags(), types, "{}", case["name"]);
            }
        }
    }

    #[test]
    fn batch_cases() {
        let indexes = |v: &Value| -> Vec<usize> { v.as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as usize).collect() };
        for case in fixture()["batch"].as_array().unwrap() {
            let sizes = indexes(&case["sizes"]);
            let (groups, dropped) = batches(
                &sizes,
                case["overhead"].as_u64().unwrap() as usize,
                case["batchMaxBytes"].as_u64().unwrap() as usize,
                case["chatMaxBytes"].as_u64().unwrap() as usize,
            );
            let want: Vec<Vec<usize>> = case["groups"].as_array().unwrap().iter().map(indexes).collect();
            assert_eq!(groups, want, "{}", case["name"]);
            assert_eq!(dropped, indexes(&case["dropped"]), "{}", case["name"]);
        }
    }

    /// VAPP-43 STORED REPLAY: reset, the file, synced over the LOG seqs
    /// (state slots skipped, never truncated), and `bye history` only for an
    /// ended row.
    #[test]
    fn a_stored_replay_spans_the_log_and_ends_an_ended_row() {
        let events = vec![
            (0, ActivityEvent::narration("a")),
            (2, ActivityEvent::narration("b")),
            (1, ActivityEvent::diff("state")),
        ];
        let idle = stored_replay(&events, false, false);
        assert_eq!(idle.len(), 5);
        assert_eq!(idle[0], r#"{"t":"activity_reset"}"#);
        assert!(idle[3].contains(r#""kind":"diff""#));
        assert_eq!(idle[4], r#"{"t":"activity_synced","firstSeq":0,"lastSeq":2}"#);
        let ended = stored_replay(&events, true, false);
        assert_eq!(ended.last().unwrap(), r#"{"t":"bye","outcome":"history"}"#);
        assert_eq!(
            stored_replay(&[], false, false),
            vec![r#"{"t":"activity_reset"}"#.to_string(), r#"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#.to_string()]
        );
        // REPLAY TAIL: a cut replay says so.
        let cut = stored_replay(&events[1..], false, true);
        assert_eq!(cut.last().unwrap(), r#"{"t":"activity_synced","firstSeq":2,"lastSeq":2,"truncated":true}"#);
    }

    #[test]
    fn an_envelope_is_exactly_overhead_plus_frames_plus_commas() {
        let frames = vec![r#"{"t":"keepalive"}"#.to_string(), r#"{"t":"input","data":"hi"}"#.to_string()];
        let body = envelope_bytes("s\"1", &frames);
        assert_eq!(body.len(), envelope_overhead("s\"1") + frames[0].len() + frames[1].len() + 1);
        let envelope = parse_envelope(&body).unwrap();
        assert_eq!(envelope.session_id, "s\"1");
        assert_eq!(envelope.tags(), ["keepalive", "input"]);
        assert!(envelope.from_device());
        assert_eq!(envelope_overhead("s1"), envelope_bytes("s1", &[]).len());
    }
}
