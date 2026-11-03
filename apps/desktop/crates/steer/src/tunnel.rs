//! P2P stabilisation: the sealed relay TUNNEL, eNet's fallback carrier
//! (`packages/domain-contract/fixtures/steer-enet.json` `fallback` is the
//! spec; TS reference `packages/domain-contract/src/steer-enet.ts`).
//!
//! eNet stays the primary carrier. The relay carries the SAME exp-steer
//! envelopes, but every one is SEALED to the session's tree key: tunnel
//! `data` = `v1:<epoch>:<b64url push_seal(tree_public, utf8(envelope))>`, in
//! both directions; the relay sees the session id, the tunnel id and
//! ciphertext, nothing else.
//!
//! - [`carrier_step`]: the ONE carrier rule every viewer runs (the
//!   fixture's `carrierCases`); the desktop viewer (`viewer.rs`) enacts it.
//! - [`TreeKeys`] / [`seal`] / [`open`]: the tree key of a session, resolved
//!   once per tunnel (the install's keyring, `coding::sealing`).
//! - [`serve`]: the DEVICE half of one tunnel the relay opened on the
//!   control socket (`control_channel.rs`): served exactly like an eNet
//!   viewer endpoint (`enet_steer.rs`), or `tunnel_close` without the key.

use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use enet_core::sealed::{OpenStatus, SEALED_BODY_PREFIX};
use serde_json::Value;

use crate::enet_steer::{self, Envelope};

/// No eNet frame by then = the relay leg opens.
pub const STALL: Duration = Duration::from_millis(2_500);
/// A tunnel `data` longer than this is dropped (both ends and the relay).
pub const TUNNEL_MAX_DATA_BYTES: usize = 1_572_864;
/// A dropped relay leg reopens after these delays (the last one repeats).
pub const RELAY_RETRY: [Duration; 4] = [
    Duration::from_millis(1_000),
    Duration::from_millis(3_000),
    Duration::from_millis(10_000),
    Duration::from_millis(30_000),
];
/// The relay's refusals of a tunnel dial (`{t:"tunnel_error", code}`).
pub const TUNNEL_ERROR_CODES: &[&str] = &["device_offline", "device_unsupported"];

// ---------------------------------------------------------------------------
// The carrier rule (fixture `carrierCases`)
// ---------------------------------------------------------------------------

/// A feed's carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    Enet,
    Relay,
}

/// The carrier state: which carrier's frames apply, and whether the relay
/// leg is up (opening, stand-by or active).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarrierState {
    pub active: Option<Carrier>,
    pub relay_open: bool,
}

impl CarrierState {
    pub const INITIAL: CarrierState = CarrierState { active: None, relay_open: false };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierEvent {
    Start,
    Stall,
    EnetNoFeed,
    /// Every endpoint answered the join with `error`/`bye` (#EXP-1678): the
    /// device said no, and the relay reaches the same device.
    EnetRefused,
    /// The eNet feed's `activity_reset`.
    EnetReset,
    /// Every later eNet frame (synced, live, keepalive, bye).
    EnetSynced,
    EnetDropped,
    RelayReset,
    RelaySynced,
    RelayDropped,
    RelayUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierAction {
    DialEnet,
    ArmStall,
    OpenRelay,
    RejoinRelay,
    CloseRelay,
    ReopenRelayLater,
    RedialEnetLater,
    Apply,
    Drop,
}

/// TS `steerCarrierStep`: eNet first, the relay only when eNet stalls, has
/// no feed or drops; eNet keeps redialing while the relay is active and
/// takes over on its `activity_reset`; frames apply only from the active
/// carrier.
pub fn carrier_step(state: CarrierState, event: CarrierEvent) -> (CarrierState, Vec<CarrierAction>) {
    use CarrierAction::*;
    use CarrierEvent as E;
    let CarrierState { active, relay_open } = state;
    let at = |active, relay_open, actions: Vec<CarrierAction>| (CarrierState { active, relay_open }, actions);
    let idle = active.is_none() && !relay_open;
    match event {
        E::Start => at(None, false, vec![DialEnet, ArmStall]),
        E::Stall if idle => at(None, true, vec![OpenRelay]),
        E::Stall => at(active, relay_open, vec![]),
        E::EnetNoFeed if idle => at(None, true, vec![OpenRelay, RedialEnetLater]),
        E::EnetNoFeed => at(active, relay_open, vec![RedialEnetLater]),
        E::EnetRefused if active == Some(Carrier::Relay) => at(active, relay_open, vec![RedialEnetLater]),
        E::EnetRefused => at(active, relay_open, vec![]),
        E::EnetReset => at(Some(Carrier::Enet), relay_open, vec![Apply]),
        E::EnetSynced if active != Some(Carrier::Enet) => at(active, relay_open, vec![Drop]),
        E::EnetSynced if relay_open => at(active, false, vec![Apply, CloseRelay]),
        E::EnetSynced => at(active, relay_open, vec![Apply]),
        E::EnetDropped if active != Some(Carrier::Enet) => at(active, relay_open, vec![RedialEnetLater]),
        E::EnetDropped if relay_open => at(None, true, vec![RejoinRelay, RedialEnetLater]),
        E::EnetDropped => at(None, true, vec![OpenRelay, RedialEnetLater]),
        E::RelayReset if active == Some(Carrier::Enet) => at(active, relay_open, vec![Drop]),
        E::RelayReset => at(Some(Carrier::Relay), relay_open, vec![Apply]),
        E::RelaySynced if active == Some(Carrier::Relay) => at(active, relay_open, vec![Apply]),
        E::RelaySynced => at(active, relay_open, vec![Drop]),
        E::RelayDropped if active == Some(Carrier::Enet) => at(active, false, vec![]),
        E::RelayDropped => at(None, false, vec![ReopenRelayLater]),
        E::RelayUnavailable => at(active, false, vec![]),
    }
}

// ---------------------------------------------------------------------------
// The tree key and the sealed wire
// ---------------------------------------------------------------------------

/// One session tree's current key, as this install holds it.
#[derive(Clone, Copy)]
pub struct TunnelKeys {
    pub epoch: u32,
    pub public: [u8; 32],
    pub secret: [u8; 32],
}

/// Where the tunnel finds a session's tree key. BLOCKING (it may ask the
/// server for the root's public key once).
pub trait TreeKeys: Send + Sync + 'static {
    /// The current epoch's key with its SECRET, or `None` (a keyless tree,
    /// no seal service, or this install holds no wrap of it yet).
    fn resolve(&self, session_id: &str) -> Option<TunnelKeys>;
}

/// The production keys: the process's seal service (`coding::sealing`).
struct SealingKeys;

impl TreeKeys for SealingKeys {
    fn resolve(&self, session_id: &str) -> Option<TunnelKeys> {
        let root = coding::sealing::root_of(session_id)?;
        let service = coding::sealing::service()?;
        let (epoch, public) = service.public_key(&root).ok().flatten()?;
        let secret = match service.keyring().secret(&root, epoch) {
            Some(secret) => secret,
            None => {
                service.fetch_secrets(&root);
                service.keyring().secret(&root, epoch)?
            }
        };
        Some(TunnelKeys { epoch, public, secret })
    }
}

fn tree_keys() -> &'static RwLock<Arc<dyn TreeKeys>> {
    static KEYS: OnceLock<RwLock<Arc<dyn TreeKeys>>> = OnceLock::new();
    KEYS.get_or_init(|| RwLock::new(Arc::new(SealingKeys)))
}

/// Replaces the key source (tests).
#[doc(hidden)]
pub fn set_tree_keys(keys: Arc<dyn TreeKeys>) {
    *tree_keys().write().unwrap_or_else(|p| p.into_inner()) = keys;
}

/// The session's tree key. BLOCKING.
pub fn resolve_keys(session_id: &str) -> Option<TunnelKeys> {
    let keys = Arc::clone(&tree_keys().read().unwrap_or_else(|p| p.into_inner()));
    keys.resolve(session_id)
}

/// One serialized envelope → its tunnel `data`; `None` = over
/// [`TUNNEL_MAX_DATA_BYTES`] or not sealable.
pub fn seal(keys: &TunnelKeys, envelope: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(envelope).ok()?;
    let body = enet_core::sealed::seal_body(&keys.public, keys.epoch, text).ok()?;
    let data = body.strip_prefix(SEALED_BODY_PREFIX)?.to_string();
    (data.len() <= TUNNEL_MAX_DATA_BYTES).then_some(data)
}

/// A tunnel `data` → the envelope of `session_id` it carries; anything that
/// does not open (no key, refused, malformed) or is no envelope of that
/// session is `None` (dropped).
pub fn open(keys: &TunnelKeys, session_id: &str, data: &str) -> Option<Envelope> {
    if data.len() > TUNNEL_MAX_DATA_BYTES {
        return None;
    }
    let body = format!("{SEALED_BODY_PREFIX}{data}");
    let opened = enet_core::sealed::open_body(&body, &|epoch| (epoch == keys.epoch).then_some(keys.secret));
    if opened.status != OpenStatus::Opened {
        return None;
    }
    enet_steer::parse_envelope(opened.text?.as_bytes()).filter(|envelope| envelope.session_id == session_id)
}

/// The largest serialized envelope a tunnel carries: its sealed form must
/// fit [`TUNNEL_MAX_DATA_BYTES`] (base64url of the plaintext + the seal's
/// overhead), and never more than one eNet chat message.
pub const TUNNEL_ENVELOPE_MAX_BYTES: usize = {
    let sealed = TUNNEL_MAX_DATA_BYTES / 4 * 3 - 256;
    if sealed < enet_steer::CHAT_MAX_BYTES {
        sealed
    } else {
        enet_steer::CHAT_MAX_BYTES
    }
};

// ---------------------------------------------------------------------------
// The device: one tunnel the relay opened
// ---------------------------------------------------------------------------

/// `tunnel_open {tunnelId, sessionId, userId}` off the control socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelOpen {
    pub tunnel_id: String,
    pub session_id: String,
    /// The viewer's user, from the server-minted ticket.
    pub user_id: String,
}

/// The control-socket frames of a tunnel (relay → device).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TunnelFrame {
    Open(TunnelOpen),
    Data { tunnel_id: String, data: String },
    Close { tunnel_id: String },
}

impl TunnelFrame {
    /// `None` = not a tunnel frame (the control channel's own parse runs).
    pub fn parse(text: &str) -> Option<TunnelFrame> {
        let value: Value = serde_json::from_str(text).ok()?;
        let field = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let tunnel_id = field("tunnelId").filter(|id| !id.is_empty())?;
        match value.get("t").and_then(Value::as_str)? {
            "tunnel_open" => Some(TunnelFrame::Open(TunnelOpen {
                tunnel_id,
                session_id: field("sessionId").filter(|id| !id.is_empty())?,
                user_id: field("userId").filter(|id| !id.is_empty())?,
            })),
            "tunnel_data" => Some(TunnelFrame::Data { tunnel_id, data: field("data")? }),
            "tunnel_close" => Some(TunnelFrame::Close { tunnel_id }),
            _ => None,
        }
    }
}

/// Device → relay: one sealed envelope for the tunnel's viewer.
pub fn data_frame(tunnel_id: &str, data: &str) -> String {
    serde_json::json!({ "t": "tunnel_data", "tunnelId": tunnel_id, "data": data }).to_string()
}

/// Device → relay: end the tunnel (the relay closes the viewer socket).
pub fn close_frame(tunnel_id: &str) -> String {
    serde_json::json!({ "t": "tunnel_close", "tunnelId": tunnel_id }).to_string()
}

/// A tunnel's viewer as the device's feeds see it: its envelopes go out
/// sealed, as `tunnel_data` on the control socket (`out`).
#[derive(Clone)]
pub struct TunnelPeer {
    pub tunnel_id: String,
    pub keys: TunnelKeys,
    pub out: flume::Sender<String>,
}

impl TunnelPeer {
    /// One serialized envelope to the viewer. `Err` = the control socket
    /// is gone (the viewer is dropped).
    pub fn send(&self, envelope: &[u8]) -> Result<(), String> {
        let Some(data) = seal(&self.keys, envelope) else {
            log::warn!("steer tunnel: an envelope ({} bytes) did not seal; dropped", envelope.len());
            return Ok(());
        };
        self.out.send(data_frame(&self.tunnel_id, &data)).map_err(|_| "control socket closed".to_string())
    }
}

impl std::fmt::Debug for TunnelPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TunnelPeer({})", self.tunnel_id)
    }
}

/// Serves one tunnel until the relay closes it (or the control socket goes:
/// `data` disconnects): the tree key first (none = `tunnel_close`, nothing
/// else), then every viewer envelope through `enet_steer`, exactly like an
/// eNet viewer's.
pub async fn serve(open: TunnelOpen, out: flume::Sender<String>, data: flume::Receiver<String>) {
    let id = open.session_id.clone();
    let keys = tokio::task::spawn_blocking(move || resolve_keys(&id)).await.ok().flatten();
    let Some(keys) = keys else {
        log::info!("steer tunnel: no tree key for {}; closing tunnel {}", open.session_id, open.tunnel_id);
        let _ = out.send(close_frame(&open.tunnel_id));
        return;
    };
    let peer = TunnelPeer { tunnel_id: open.tunnel_id.clone(), keys, out };
    log::info!("steer tunnel: {} open for {}", open.tunnel_id, open.session_id);
    while let Ok(data) = data.recv_async().await {
        match self::open(&peer.keys, &open.session_id, &data) {
            Some(envelope) => enet_steer::receive_tunnel(&peer, &open.user_id, envelope),
            None => log::debug!("steer tunnel: a data on {} did not open; dropped", open.tunnel_id),
        }
    }
    enet_steer::close_tunnel(&open.session_id, &open.tunnel_id);
    log::info!("steer tunnel: {} closed", open.tunnel_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json")).unwrap()
    }

    fn state_of(value: &Value) -> CarrierState {
        let active = match value["active"].as_str().unwrap() {
            "none" => None,
            "enet" => Some(Carrier::Enet),
            "relay" => Some(Carrier::Relay),
            other => panic!("unknown carrier {other}"),
        };
        let relay_open = match value["relay"].as_str().unwrap() {
            "open" => true,
            "closed" => false,
            other => panic!("unknown relay {other}"),
        };
        CarrierState { active, relay_open }
    }

    fn event_of(name: &str) -> CarrierEvent {
        use CarrierEvent::*;
        match name {
            "start" => Start,
            "stall" => Stall,
            "enetNoFeed" => EnetNoFeed,
            "enetRefused" => EnetRefused,
            "enetReset" => EnetReset,
            "enetSynced" => EnetSynced,
            "enetDropped" => EnetDropped,
            "relayReset" => RelayReset,
            "relaySynced" => RelaySynced,
            "relayDropped" => RelayDropped,
            "relayUnavailable" => RelayUnavailable,
            other => panic!("unknown event {other}"),
        }
    }

    fn action_name(action: CarrierAction) -> &'static str {
        use CarrierAction::*;
        match action {
            DialEnet => "dialEnet",
            ArmStall => "armStall",
            OpenRelay => "openRelay",
            RejoinRelay => "rejoinRelay",
            CloseRelay => "closeRelay",
            ReopenRelayLater => "reopenRelayLater",
            RedialEnetLater => "redialEnetLater",
            Apply => "apply",
            Drop => "drop",
        }
    }

    #[test]
    fn carrier_cases() {
        let f = fixture();
        let cases = f["carrierCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let (next, actions) = carrier_step(state_of(&case["state"]), event_of(case["event"].as_str().unwrap()));
            assert_eq!(next, state_of(&case["next"]), "{}", case["name"]);
            let names: Vec<&str> = actions.into_iter().map(action_name).collect();
            let want: Vec<&str> = case["actions"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
            assert_eq!(names, want, "{}", case["name"]);
        }
    }

    #[test]
    fn the_constants_are_the_fixtures() {
        let f = &fixture()["fallback"];
        assert_eq!(f["stallMs"], STALL.as_millis() as u64);
        assert_eq!(f["tunnelMaxDataBytes"], TUNNEL_MAX_DATA_BYTES);
        let retry: Vec<u64> = RELAY_RETRY.iter().map(|d| d.as_millis() as u64).collect();
        assert_eq!(f["relayRetryMs"], serde_json::json!(retry));
        assert_eq!(f["tunnelErrorCodes"], serde_json::json!(TUNNEL_ERROR_CODES));
    }

    fn keys() -> TunnelKeys {
        let key = enet_core::sealed::generate_tree_key();
        TunnelKeys { epoch: 3, public: key.public, secret: key.secret }
    }

    #[test]
    fn a_sealed_envelope_opens_only_with_its_tree_key_and_session() {
        let keys = keys();
        let body = enet_steer::envelope_bytes("s1", &[r#"{"t":"join","channel":"activity"}"#.to_string()]);
        let data = seal(&keys, &body).unwrap();
        assert!(data.starts_with("v1:3:"), "the sealed-value grammar after the body prefix");
        assert!(!data.contains("join"), "ciphertext only");
        let envelope = open(&keys, "s1", &data).unwrap();
        assert_eq!(envelope.tags(), ["join"]);
        assert!(open(&keys, "s2", &data).is_none(), "another session's tunnel drops it");
        assert!(open(&self::keys(), "s1", &data).is_none(), "another tree's key does not open it");
        assert!(open(&keys, "s1", "v1:3:not-base64").is_none());
        assert!(open(&keys, "s1", &"x".repeat(TUNNEL_MAX_DATA_BYTES + 1)).is_none());
        // A plaintext envelope is never accepted off a tunnel.
        assert!(open(&keys, "s1", std::str::from_utf8(&body).unwrap()).is_none());
    }

    #[test]
    fn the_largest_envelope_still_seals_under_the_cap() {
        let keys = keys();
        let filler = "a".repeat(TUNNEL_ENVELOPE_MAX_BYTES - enet_steer::envelope_overhead("s1") - 40);
        let frame = serde_json::json!({ "t": "activity", "x": filler }).to_string();
        let body = enet_steer::envelope_bytes("s1", &[frame]);
        assert!(body.len() <= TUNNEL_ENVELOPE_MAX_BYTES);
        assert!(seal(&keys, &body).is_some());
    }

    #[test]
    fn tunnel_frames_parse_non_strictly() {
        assert_eq!(
            TunnelFrame::parse(r#"{"t":"tunnel_open","tunnelId":"t1","sessionId":"s1","userId":"u1","extra":1}"#),
            Some(TunnelFrame::Open(TunnelOpen { tunnel_id: "t1".into(), session_id: "s1".into(), user_id: "u1".into() }))
        );
        assert_eq!(
            TunnelFrame::parse(r#"{"t":"tunnel_data","tunnelId":"t1","data":"v1:1:x"}"#),
            Some(TunnelFrame::Data { tunnel_id: "t1".into(), data: "v1:1:x".into() })
        );
        assert_eq!(
            TunnelFrame::parse(r#"{"t":"tunnel_close","tunnelId":"t1"}"#),
            Some(TunnelFrame::Close { tunnel_id: "t1".into() })
        );
        assert_eq!(TunnelFrame::parse(r#"{"t":"tunnel_open","tunnelId":"t1","sessionId":"s1"}"#), None);
        assert_eq!(TunnelFrame::parse(r#"{"t":"check_in"}"#), None);
        assert_eq!(
            TunnelFrame::parse(&data_frame("t1", "d")),
            Some(TunnelFrame::Data { tunnel_id: "t1".into(), data: "d".into() })
        );
    }
}
