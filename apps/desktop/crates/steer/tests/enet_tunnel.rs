//! P2P stabilisation (`fixtures/steer-enet.json` `fallback`): eNet is the
//! primary carrier, a sealed relay TUNNEL its invisible fallback.
//!
//! Two REAL local eNet nodes (a device running a publisher, a desktop
//! viewer) and a FAKE relay speaking the tunnel wire (the viewer's `sealed`
//! socket, the device's control socket `tunnel_open/data/close`). The
//! viewer streams over eNet; its eNet node is then KILLED mid-stream: the
//! feed keeps streaming over the relay tunnel (sealed: the relay never sees
//! a frame in clear); a new eNet node under the same identity comes back and
//! the view moves back to eNet, the tunnel closing. Connection analytics are
//! recorded on both ends.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use api::steer::{MintedTicket, TunnelTicketResult};
use enet_core::NetConfig;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use steer::control_channel::{spawn_control_channel, ControlApi, DeviceIdentity};
use steer::enet::{self, InstallKind, InstallNode, InstallRegistry, InstallSpec};
use steer::tunnel::{Carrier, TreeKeys, TunnelKeys};
use steer::{
    enet_steer, ActivityEvent, KillSignal, PublishSpec, PublisherHooks, SessionAgent, SteerRuntime, ViewerEvent,
    ViewerPhase, ViewerTimings,
};
use tokio_tungstenite::tungstenite::Message;

const WAIT: Duration = Duration::from_secs(30);

// ── eNet installs (as in tests/enet_steer.rs) ───────────────────────────────

#[derive(Default)]
struct Registry {
    installs: Mutex<Vec<InstallEntry>>,
}

struct As {
    user_id: String,
    registry: Arc<Registry>,
}

impl InstallRegistry for As {
    fn register(&self, input: &RegisterInstall) -> Result<String, ApiError> {
        let mut installs = self.registry.installs.lock().unwrap();
        if let Some(row) = installs.iter().find(|row| row.endpoint_id == input.endpoint_id) {
            return Ok(row.install_id.clone());
        }
        let install_id = format!("install-{}", installs.len());
        installs.push(InstallEntry {
            install_id: install_id.clone(),
            user_id: self.user_id.clone(),
            kind: input.kind.clone(),
            label: input.label.clone(),
            platform: input.platform.clone(),
            endpoint_id: input.endpoint_id.clone(),
            x25519_public: input.x25519_public.clone(),
            last_seen_at: None,
            device_id: input.device_id.clone(),
        });
        Ok(install_id)
    }
    fn list(&self, _team_id: &str) -> Result<Vec<InstallEntry>, ApiError> {
        Ok(self.registry.installs.lock().unwrap().clone())
    }
    fn revoke(&self, _install_id: &str) -> Result<(), ApiError> {
        Ok(())
    }
}

fn start(registry: &Arc<Registry>, dir: &std::path::Path, user: &str, kind: InstallKind) -> InstallNode {
    let spec = InstallSpec {
        kind,
        user_id: user.into(),
        device_id: Some(format!("device-{user}-{kind:?}")),
        label: None,
        identity_path: enet::identity_path(dir, &format!("account-{user}"), kind),
        net: NetConfig::local_only(),
    };
    InstallNode::start(
        &tokio::runtime::Handle::current(),
        spec,
        Arc::new(As { user_id: user.into(), registry: Arc::clone(registry) }),
        Arc::new(|| vec!["team-1".to_string()]),
    )
}

async fn ready(installs: &[&InstallNode]) {
    for install in installs {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in installs {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
}

#[derive(Default)]
struct Recorded {
    inputs: Mutex<Vec<Vec<u8>>>,
}

fn hooks(recorded: &Arc<Recorded>) -> PublisherHooks {
    let inputs = Arc::clone(recorded);
    PublisherHooks {
        write_input: Arc::new(move |bytes| inputs.inputs.lock().unwrap().push(bytes.to_vec())),
        kill: Arc::new(|_: KillSignal| {}),
        answers: None,
        agent: SessionAgent::Claude,
        text_sink: None,
        attachments: None,
        commands: None,
        config: None,
        interrupt: None,
        unqueue: None,
        compact: None,
    }
}

// ── The tree key both installs hold ─────────────────────────────────────────

struct FixedKeys(TunnelKeys);

impl TreeKeys for FixedKeys {
    fn resolve(&self, _session_id: &str) -> Option<TunnelKeys> {
        Some(self.0)
    }
}

// ── The fake relay ──────────────────────────────────────────────────────────

/// What the fake relay saw.
#[derive(Default)]
struct RelaySeen {
    online_tunnel: AtomicBool,
    opened: AtomicUsize,
    /// Viewer sockets that went away (the relay told the device).
    closed: AtomicUsize,
    /// Every `data` that crossed it (must never hold a frame in clear).
    data: Mutex<Vec<String>>,
}

#[derive(Default)]
struct RelayState {
    device: Mutex<Option<flume::Sender<String>>>,
    tunnels: Mutex<HashMap<String, flume::Sender<Option<String>>>>,
    seen: RelaySeen,
}

async fn fake_relay() -> (String, Arc<RelayState>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(RelayState::default());
    let relay = Arc::clone(&state);
    tokio::spawn(async move {
        let mut next_tunnel = 0usize;
        while let Ok((stream, _)) = listener.accept().await {
            let uri = Arc::new(Mutex::new(String::new()));
            let seen_uri = Arc::clone(&uri);
            let callback = move |req: &tokio_tungstenite::tungstenite::handshake::server::Request, res| {
                *seen_uri.lock().unwrap() = req.uri().to_string();
                Ok(res)
            };
            let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, callback).await else { continue };
            let uri = uri.lock().unwrap().clone();
            if uri.starts_with("/control") {
                tokio::spawn(device_socket(ws, Arc::clone(&relay)));
            } else {
                next_tunnel += 1;
                let ticket = uri.split("ticket=").nth(1).unwrap_or_default().to_string();
                tokio::spawn(viewer_socket(ws, format!("t{next_tunnel}"), ticket, Arc::clone(&relay)));
            }
        }
    });
    (format!("ws://{addr}"), state)
}

type Ws = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

async fn device_socket(mut ws: Ws, relay: Arc<RelayState>) {
    let (to_device, from_relay) = flume::unbounded::<String>();
    *relay.device.lock().unwrap() = Some(to_device);
    loop {
        tokio::select! {
            out = from_relay.recv_async() => {
                let Ok(text) = out else { return };
                if ws.send(Message::text(text)).await.is_err() { return }
            }
            msg = ws.next() => {
                let Some(Ok(Message::Text(text))) = msg else {
                    if matches!(msg, Some(Ok(_))) { continue }
                    return;
                };
                let frame: Value = serde_json::from_str(&text).unwrap();
                let tunnel = frame["tunnelId"].as_str().unwrap_or_default().to_string();
                match frame["t"].as_str() {
                    Some("online") => relay.seen.online_tunnel.store(frame["tunnel"] == 1, Ordering::SeqCst),
                    Some("tunnel_data") => {
                        let data = frame["data"].as_str().unwrap().to_string();
                        relay.seen.data.lock().unwrap().push(data.clone());
                        if let Some(viewer) = relay.tunnels.lock().unwrap().get(&tunnel) {
                            let _ = viewer.send(Some(data));
                        }
                    }
                    Some("tunnel_close") => {
                        if let Some(viewer) = relay.tunnels.lock().unwrap().remove(&tunnel) {
                            let _ = viewer.send(None);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// One viewer tunnel socket; the fake ticket = `<sessionId>|<userId>`.
async fn viewer_socket(mut ws: Ws, tunnel_id: String, ticket: String, relay: Arc<RelayState>) {
    let (session_id, user_id) = ticket.split_once('|').unwrap();
    let Some(device) = relay.device.lock().unwrap().clone() else {
        let _ = ws.send(Message::text(r#"{"t":"tunnel_error","code":"device_offline"}"#)).await;
        return;
    };
    let (to_viewer, from_device) = flume::unbounded::<Option<String>>();
    relay.tunnels.lock().unwrap().insert(tunnel_id.clone(), to_viewer);
    relay.seen.opened.fetch_add(1, Ordering::SeqCst);
    let open = serde_json::json!({"t": "tunnel_open", "tunnelId": tunnel_id, "sessionId": session_id, "userId": user_id});
    let _ = device.send(open.to_string());
    loop {
        tokio::select! {
            out = from_device.recv_async() => match out {
                Ok(Some(data)) => {
                    let sealed = serde_json::json!({"t": "sealed", "data": data}).to_string();
                    if ws.send(Message::text(sealed)).await.is_err() { break }
                }
                _ => { let _ = ws.close(None).await; break }
            },
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    let data = frame["data"].as_str().unwrap().to_string();
                    relay.seen.data.lock().unwrap().push(data.clone());
                    let _ = device.send(serde_json::json!({"t": "tunnel_data", "tunnelId": tunnel_id, "data": data}).to_string());
                }
                Some(Ok(_)) => {}
                _ => break,
            }
        }
    }
    relay.tunnels.lock().unwrap().remove(&tunnel_id);
    relay.seen.closed.fetch_add(1, Ordering::SeqCst);
    let _ = device.send(serde_json::json!({"t": "tunnel_close", "tunnelId": tunnel_id}).to_string());
}

struct FakeControl(String);

impl ControlApi for FakeControl {
    fn config_enabled(&self) -> Result<bool, ApiError> {
        Ok(true)
    }
    fn mint_control(&self) -> Result<Option<String>, ApiError> {
        Ok(Some(format!("{}/control?ticket=device", self.0)))
    }
}

/// The desktop viewer's source: a swappable eNet node, the device's
/// endpoint, and tunnel tickets to the fake relay.
struct Source {
    node: Mutex<Option<Arc<enet_core::Node>>>,
    endpoints: Vec<String>,
    relay_url: String,
    user_id: String,
}

impl steer::EnetViewerSource for Source {
    fn node(&self) -> Option<Arc<enet_core::Node>> {
        self.node.lock().unwrap().clone()
    }
    fn endpoints(&self, _session_id: &str) -> Result<Vec<String>, ApiError> {
        Ok(self.endpoints.clone())
    }
    fn tunnel_ticket(&self, session_id: &str) -> Result<TunnelTicketResult, ApiError> {
        let ticket = format!("{session_id}|{}", self.user_id);
        Ok(TunnelTicketResult::Ticket(MintedTicket { url: format!("{}/tunnel?ticket={ticket}", self.relay_url), ticket }))
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<Value>>);

impl steer::conn_stats::ConnStatsSink for Sink {
    fn record(&self, events: &[Value]) -> Result<u32, ApiError> {
        self.0.lock().unwrap().extend(events.iter().cloned());
        Ok(events.len() as u32)
    }
}

async fn wait_until(what: &str, predicate: impl Fn() -> bool) {
    tokio::time::timeout(WAIT, async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("in time: {what}"));
}

/// Drains viewer events until `n` live narrations arrived; panics on a
/// user-visible error state.
async fn narrations(events: &flume::Receiver<ViewerEvent>, n: usize) -> Vec<String> {
    let mut seen = Vec::new();
    while seen.len() < n {
        let event = tokio::time::timeout(WAIT, events.recv_async()).await.expect("an event in time").unwrap();
        match event {
            ViewerEvent::Activity(_, ActivityEvent::Narration { text, .. }) if text.starts_with("tick") => seen.push(text),
            ViewerEvent::Phase(phase @ (ViewerPhase::Reconnecting | ViewerPhase::Starting | ViewerPhase::Ended { .. })) => {
                panic!("a carrier switch must stay invisible, got {phase:?}")
            }
            _ => {}
        }
    }
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn the_view_keeps_streaming_over_the_relay_when_enet_dies_and_moves_back_to_enet() {
    let dir = std::env::temp_dir().join(format!("enet-tunnel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let key = enet_core::sealed::generate_tree_key();
    steer::tunnel::set_tree_keys(Arc::new(FixedKeys(TunnelKeys { epoch: 1, public: key.public, secret: key.secret })));
    let sink = Arc::new(Sink::default());
    steer::conn_stats::install(&tokio::runtime::Handle::current(), sink.clone(), "desktop");

    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "dana", InstallKind::Daemon);
    let viewer = start(&registry, &dir, "dana", InstallKind::Desktop);
    ready(&[&device, &viewer]).await;
    let d = device.node().unwrap();
    let v = viewer.node().unwrap();
    d.add_ticket(&v.ticket());
    v.add_ticket(&d.ticket());

    // The device: its control socket announces the tunnel; a live run.
    let (relay_url, relay) = fake_relay().await;
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let control = spawn_control_channel(
        runtime,
        DeviceIdentity { device_id: "device-dana".into(), device_label: "test".into() },
        Arc::new(FakeControl(relay_url.clone())),
        Arc::new(|| {}),
        Arc::new(|_| {}),
    );
    wait_until("the device announced tunnel: 1", || relay.seen.online_tunnel.load(Ordering::SeqCst)).await;
    let session_id = format!("sess-tunnel-{}", std::process::id());
    let recorded = Arc::new(Recorded::default());
    let publisher = steer::publish(
        runtime,
        PublishSpec {
            session_id: session_id.clone(),
            audience: Some(coding::SteerAudience {
                team_id: "team-1".into(),
                starter_user_id: "dana".into(),
                shared: false,
            }),
            ..Default::default()
        },
        hooks(&recorded),
    );
    wait_until("the room", || enet_steer::has_room(&session_id)).await;
    // The run narrates steadily: frames to keep arriving.
    let activity = publisher.activity_sender();
    let ticking = Arc::new(AtomicBool::new(true));
    let still = Arc::clone(&ticking);
    tokio::spawn(async move {
        let mut n = 0;
        while still.load(Ordering::SeqCst) {
            activity.send(ActivityEvent::narration(format!("tick {n}")));
            n += 1;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });

    // The viewer: eNet first.
    let source = Arc::new(Source {
        node: Mutex::new(Some(Arc::clone(&v))),
        endpoints: vec![enet::endpoint_hex(&d.endpoint_id())],
        relay_url: relay_url.clone(),
        user_id: "dana".into(),
    });
    let timings = ViewerTimings {
        live_stale: Duration::from_millis(1_500),
        liveness_tick: Duration::from_millis(100),
        retry_base: Duration::from_millis(500),
        retry_cap: Duration::from_millis(1_000),
        join_timeout: Duration::from_secs(5),
        relay_retry: vec![Duration::from_millis(200)],
        ..ViewerTimings::default()
    };
    let (events_tx, events) = flume::unbounded();
    let handle = steer::spawn_viewer_with(runtime, Some(source.clone()), session_id.clone(), events_tx, timings);
    narrations(&events, 3).await;
    assert_eq!(handle.carrier(), Some(Carrier::Enet), "eNet is the primary carrier");
    assert_eq!(relay.seen.opened.load(Ordering::SeqCst), 0, "a healthy eNet feed never touches the relay");

    // Steering goes out on the active carrier.
    assert!(handle.send_message("over enet"));
    wait_until("the eNet input", || recorded.inputs.lock().unwrap().iter().any(|b| b == b"over enet")).await;

    // KILL eNet mid-stream: the viewer's node goes away.
    *source.node.lock().unwrap() = None;
    viewer.stop();
    v.shutdown().await.unwrap();
    drop(v);
    wait_until("the relay carries the feed", || handle.carrier() == Some(Carrier::Relay)).await;
    assert!(relay.seen.opened.load(Ordering::SeqCst) >= 1);
    // Frames keep arriving, over the sealed tunnel.
    narrations(&events, 5).await;
    assert_eq!(handle.carrier(), Some(Carrier::Relay));
    assert!(handle.send_message("over the relay"));
    wait_until("the relay input", || recorded.inputs.lock().unwrap().iter().any(|b| b == b"over the relay")).await;
    {
        let data = relay.seen.data.lock().unwrap();
        assert!(!data.is_empty());
        for data in data.iter() {
            assert!(data.starts_with("v1:1:"), "sealed values only");
            assert!(!data.contains("tick") && !data.contains("activity") && !data.contains("relay"), "no clear text");
        }
    }

    // eNet comes back under the same identity: it takes the feed over and
    // the tunnel closes.
    let viewer = start(&registry, &dir, "dana", InstallKind::Desktop);
    ready(&[&viewer]).await;
    let v = viewer.node().unwrap();
    d.add_ticket(&v.ticket());
    v.add_ticket(&d.ticket());
    *source.node.lock().unwrap() = Some(Arc::clone(&v));
    handle.kick();
    wait_until("back on eNet", || handle.carrier() == Some(Carrier::Enet)).await;
    wait_until("the tunnel closed", || relay.seen.closed.load(Ordering::SeqCst) >= 1).await;
    narrations(&events, 3).await;
    assert_eq!(handle.carrier(), Some(Carrier::Enet));

    // Analytics: the view (with its fallback) and the device's served feeds.
    handle.shutdown();
    ticking.store(false, Ordering::SeqCst);
    publisher.shutdown(Some("exit:0".into()));
    tokio::time::sleep(Duration::from_millis(500)).await;
    steer::conn_stats::flush();
    wait_until("the analytics", || {
        let events = sink.0.lock().unwrap();
        events.iter().any(|e| e["kind"] == "steer.view" && e["data"]["phase"] == "ended" && e["data"]["outcome"] == "ok")
            && events.iter().any(|e| e["kind"] == "steer.serve" && e["data"]["path"] == "server_fallback")
    })
    .await;
    let events = sink.0.lock().unwrap().clone();
    for event in &events {
        assert!(steer::conn_stats::event_valid(event), "{event}");
        assert!(!event.to_string().contains(&session_id), "no session id leaves");
    }
    // (The other test's viewer records into the same process-wide sink:
    // this view is the one that streamed.)
    let ours = |phase: &str| {
        events
            .iter()
            .find(|e| e["kind"] == "steer.view" && e["data"]["phase"] == phase && e["data"]["outcome"] == "ok")
            .unwrap_or_else(|| panic!("an ok {phase} view in {events:?}"))
    };
    let settled = ours("settled");
    assert_eq!(settled["data"]["platform"], "desktop");
    assert!(["direct", "iroh_relay"].contains(&settled["data"]["path"].as_str().unwrap()), "{settled}");
    let ended = ours("ended");
    assert!(ended["data"]["fallbacks"].as_u64().unwrap() >= 1, "{ended}");
    assert!(ended["data"]["failures"].is_object(), "{ended}");

    control.stop();
    device.stop();
    viewer.stop();
    steer::conn_stats::uninstall();
    let _ = std::fs::remove_dir_all(&dir);
}

/// No relay leg (the device is not on the relay): the viewer behaves
/// exactly as eNet-only, and NO FEED still reads `Starting`.
#[tokio::test(flavor = "multi_thread")]
async fn without_a_relay_leg_the_viewer_is_enet_only() {
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let (relay_url, relay) = fake_relay().await;
    let source = Arc::new(Source {
        node: Mutex::new(None),
        endpoints: Vec::new(),
        relay_url,
        user_id: "dana".into(),
    });
    let (events_tx, events) = flume::unbounded();
    let handle = steer::spawn_viewer(runtime, Some(source), "sess-no-device".into(), events_tx);
    loop {
        let event = tokio::time::timeout(WAIT, events.recv_async()).await.unwrap().unwrap();
        if event == ViewerEvent::Phase(ViewerPhase::Starting) {
            break;
        }
    }
    assert_eq!(handle.carrier(), None);
    // The relay answered `device_offline`: nothing was tunneled.
    assert!(relay.tunnels.lock().unwrap().is_empty());
    handle.shutdown();
}
