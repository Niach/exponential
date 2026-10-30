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
//! Viewer half: a viewer subscribes to `(sessionId, device endpoint)`
//! ([`subscribe`]) and gets the device's frames as JSON text, exactly what a
//! relay socket would have delivered; [`send_envelope`] carries its frames.

use std::collections::HashMap;
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
/// A join with no `activity_synced` within this long = the relay instead.
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
/// This install runs no such live session.
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
    lock(rooms()).insert(session_id.to_string(), Room { generation, audience, cmd_tx });
    RoomGuard { session_id: session_id.to_string(), generation }
}

/// Is a live publisher of `session_id` serving eNet viewers here?
pub fn has_room(session_id: &str) -> bool {
    lock(rooms()).contains_key(session_id)
}

/// One chat message from `from`, received on `node`. Returns whether it was
/// a steer envelope (the caller handles anything else). `user_in_team`
/// resolves `(team id, endpoint)` to the endpoint's user in that team's
/// registry list.
pub fn receive(
    node: &Arc<Node>,
    from: EndpointId,
    body: &[u8],
    user_in_team: &dyn Fn(&str, &EndpointId) -> Option<String>,
) -> bool {
    let Some(envelope) = parse_envelope(body) else { return false };
    if envelope.from_device() {
        deliver_to_viewer(from, envelope);
        return true;
    }
    let session_id = envelope.session_id.clone();
    let room = lock(rooms())
        .get(&session_id)
        .and_then(|room| Some((room.audience.clone(), room.cmd_tx.upgrade()?)));
    let Some((audience, cmd_tx)) = room else {
        reply(node, from, &session_id, ERROR_NO_SUCH_SESSION);
        return true;
    };
    let user = user_in_team(&audience.team_id, &from);
    if !viewer_allowed(user.as_deref(), &audience.starter_user_id, audience.shared) {
        log::info!(
            "enet steer: {} refused for session {session_id}",
            crate::enet::endpoint_hex(&from)
        );
        reply(node, from, &session_id, ERROR_UNAUTHORIZED);
        return true;
    }
    for mut frame in envelope.frames {
        let cmd = match frame_tag(&frame) {
            Some("join") => PublisherCmd::EnetJoin(EnetPeer { id: from, node: Arc::clone(node) }),
            Some("leave") => PublisherCmd::EnetLeave(from),
            Some(tag) if VIEWER_FRAMES.contains(&tag) => {
                // The relay names the room on what it forwards; here the
                // envelope does.
                if let Value::Object(object) = &mut frame {
                    object.entry("sessionId").or_insert_with(|| Value::String(session_id.clone()));
                }
                PublisherCmd::EnetFrame { from, frame: frame.to_string() }
            }
            // A frame this build does not know: skipped, the rest applies.
            _ => continue,
        };
        if cmd_tx.send(cmd).is_err() {
            reply(node, from, &session_id, ERROR_NO_SUCH_SESSION);
            return true;
        }
    }
    true
}

fn reply(node: &Arc<Node>, peer: EndpointId, session_id: &str, code: &str) {
    let node = Arc::clone(node);
    let session_id = session_id.to_string();
    let frame = error_frame(code);
    tokio::spawn(async move {
        if let Err(err) = send_envelope(&node, peer, &session_id, &[frame]).await {
            log::debug!("enet steer: error reply to {} failed: {err}", crate::enet::endpoint_hex(&peer));
        }
    });
}

// ---------------------------------------------------------------------------
// Device: the per-viewer feeds (owned by the publisher loop)
// ---------------------------------------------------------------------------

/// The joined viewers of one live publisher.
pub(crate) struct Feeds {
    session_id: String,
    viewers: HashMap<EndpointId, flume::Sender<String>>,
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
    pub(crate) fn join(&mut self, peer: EnetPeer, journal: &ActivityJournal) {
        let id = peer.id;
        let tx = match self.viewers.get(&id) {
            Some(tx) if !tx.is_disconnected() => tx.clone(),
            _ => {
                let (tx, rx) = flume::unbounded();
                tokio::spawn(run_feed(peer, self.session_id.clone(), rx));
                self.viewers.insert(id, tx.clone());
                tx
            }
        };
        let _ = tx.send(ClientFrame::ActivityReset.to_json());
        for (seq, event) in journal.replay_seq() {
            let _ = tx.send(ClientFrame::Activity { event: event.clone(), seq }.to_json());
        }
        let _ = tx.send(synced_frame(journal));
        log::info!(
            "enet steer: {} joined {} ({} viewers)",
            crate::enet::endpoint_hex(&id),
            self.session_id,
            self.viewers.len()
        );
    }

    pub(crate) fn leave(&mut self, id: &EndpointId) {
        if self.viewers.remove(id).is_some() {
            log::info!("enet steer: {} left {}", crate::enet::endpoint_hex(id), self.session_id);
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
    pub(crate) fn send_to(&mut self, id: &EndpointId, frame: String) {
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

/// The end-of-replay marker, exactly the relay's (`fixtures/steer-enet.json`).
fn synced_frame(journal: &ActivityJournal) -> String {
    let (first, last, truncated) = journal.synced_span();
    let mut frame = serde_json::json!({ "t": "activity_synced", "firstSeq": first, "lastSeq": last });
    if truncated {
        frame["truncated"] = Value::Bool(true);
    }
    frame.to_string()
}

/// One viewer's feed: drains its queue into envelopes, beats a keepalive
/// when idle, and ends on a failed send (the viewer is dropped: its queue's
/// sender sees the disconnect) or once the queue closes.
async fn run_feed(peer: EnetPeer, session_id: String, rx: flume::Receiver<String>) {
    let overhead = envelope_overhead(&session_id);
    loop {
        let first = match tokio::time::timeout(KEEPALIVE, rx.recv_async()).await {
            Ok(Ok(frame)) => frame,
            Ok(Err(_)) => return,
            Err(_) => KEEPALIVE_FRAME.to_string(),
        };
        let mut frames = vec![first];
        while let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        }
        let sizes: Vec<usize> = frames.iter().map(String::len).collect();
        let (groups, dropped) = batches(&sizes, overhead, BATCH_MAX_BYTES, CHAT_MAX_BYTES);
        for index in dropped {
            log::warn!("enet steer: dropping an oversize frame ({} bytes)", sizes[index]);
        }
        for group in groups {
            let batch: Vec<String> = group.into_iter().map(|index| std::mem::take(&mut frames[index])).collect();
            if let Err(err) = send_envelope(&peer.node, peer.id, &session_id, &batch).await {
                log::info!(
                    "enet steer: viewer {} of {session_id} dropped: {err}",
                    crate::enet::endpoint_hex(&peer.id)
                );
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Viewer: subscriptions to a device's frames
// ---------------------------------------------------------------------------

type ViewerKey = (String, EndpointId);

fn viewers() -> &'static Mutex<HashMap<ViewerKey, (u64, flume::Sender<String>)>> {
    static VIEWERS: OnceLock<Mutex<HashMap<ViewerKey, (u64, flume::Sender<String>)>>> = OnceLock::new();
    VIEWERS.get_or_init(Default::default)
}

/// One viewer's subscription; dropping it unsubscribes.
pub struct ViewerSubscription {
    key: ViewerKey,
    generation: u64,
    /// The device's frames (JSON text, one per wire frame), in order.
    pub frames: flume::Receiver<String>,
}

impl Drop for ViewerSubscription {
    fn drop(&mut self) {
        let mut viewers = lock(viewers());
        if viewers.get(&self.key).is_some_and(|(generation, _)| *generation == self.generation) {
            viewers.remove(&self.key);
        }
    }
}

/// Subscribes to the frames `device` sends for `session_id` (envelopes from
/// any other endpoint, or for another session, never reach it).
pub fn subscribe(session_id: &str, device: EndpointId) -> ViewerSubscription {
    let (tx, frames) = flume::unbounded();
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    let key = (session_id.to_string(), device);
    lock(viewers()).insert(key.clone(), (generation, tx));
    ViewerSubscription { key, generation, frames }
}

fn deliver_to_viewer(from: EndpointId, envelope: Envelope) {
    let key = (envelope.session_id, from);
    let mut viewers = lock(viewers());
    let Some((_, tx)) = viewers.get(&key) else {
        log::debug!("enet steer: frames for {} from an endpoint nobody joined; ignored", key.0);
        return;
    };
    for frame in envelope.frames {
        if tx.send(frame.to_string()).is_err() {
            viewers.remove(&key);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
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
