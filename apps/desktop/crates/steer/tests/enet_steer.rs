//! VAPP-33: steer over eNet's chat channel (`fixtures/steer-enet.json`).
//! Two REAL local eNet nodes started the way the apps start them: the
//! device runs a publisher (VAPP-43: eNet is all it serves), the viewer
//! joins over chat, gets the journal replay, a live event and steers an
//! input; the run's end says `bye`. A teammate is refused from a private
//! session (its own test: both nodes live in one process, so they share the
//! viewer subscriptions). VAPP-43: STORED REPLAY off the device's journal
//! file, and the desktop viewer's no-feed states. VAPP-169: the
//! device-offline end redials once the device answers.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use enet_core::NetConfig;
use serde_json::Value;
use steer::enet::{self, InstallKind, InstallNode, InstallRegistry, InstallSpec};
use steer::enet_steer;
use steer::{
    ActivityEvent, ImageEmbeds, KillSignal, PublishSpec, PublisherHooks, SessionAgent, SteerRuntime,
};

const WAIT: Duration = Duration::from_secs(20);

/// One team, no signature checks (`enet_installs.rs` covers those).
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
        device_id: Some(format!("device-{user}")),
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

#[derive(Default)]
struct Recorded {
    inputs: Mutex<Vec<Vec<u8>>>,
    kills: Mutex<Vec<KillSignal>>,
}

fn hooks(recorded: &Arc<Recorded>) -> PublisherHooks {
    let inputs = Arc::clone(recorded);
    let kills = Arc::clone(recorded);
    PublisherHooks {
        write_input: Arc::new(move |bytes| inputs.inputs.lock().unwrap().push(bytes.to_vec())),
        kill: Arc::new(move |signal| kills.kills.lock().unwrap().push(signal)),
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

/// The next device frame within [`WAIT`], parsed.
async fn next(frames: &flume::Receiver<String>) -> Value {
    let text = tokio::time::timeout(WAIT, frames.recv_async()).await.expect("a frame in time").expect("subscribed");
    serde_json::from_str(&text).unwrap()
}

/// The next device frame that is not a keepalive.
async fn next_real(frames: &flume::Receiver<String>) -> Value {
    loop {
        let frame = next(frames).await;
        if frame["t"] != "keepalive" {
            return frame;
        }
    }
}

async fn wait_until(predicate: impl Fn() -> bool) {
    tokio::time::timeout(WAIT, async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("in time");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_viewer_joins_gets_the_replay_and_live_events_and_steers_over_enet() {
    let dir = std::env::temp_dir().join(format!("enet-steer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "alice", InstallKind::Daemon);
    let viewer = start(&registry, &dir, "alice", InstallKind::Desktop);
    let teammate = start(&registry, &dir, "bob", InstallKind::Desktop);
    for install in [&device, &viewer, &teammate] {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    // The later ones registered after the first lists: re-read them all.
    for install in [&device, &viewer, &teammate] {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let d = device.node().unwrap();
    let v = viewer.node().unwrap();
    let t = teammate.node().unwrap();
    // Local-only nodes have no relay: introduce the direct addresses.
    for (a, b) in [(&d, &v), (&d, &t)] {
        a.add_ticket(&b.ticket());
        b.add_ticket(&a.ticket());
    }

    let session_id = format!("sess-enet-{}", std::process::id());
    // Leaked: a runtime may not drop inside the test's async context.
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let recorded = Arc::new(Recorded::default());
    let handle = steer::publish(
        runtime,
        PublishSpec {
            session_id: session_id.clone(),
            issue_id: None,
            journal_dir: None,
            embeds: ImageEmbeds::default(),
            audience: Some(coding::SteerAudience {
                team_id: "team-1".into(),
                starter_user_id: "alice".into(),
                shared: false,
            }),
        },
        hooks(&recorded),
    );
    let activity = handle.activity_sender();
    activity.send(ActivityEvent::narration("before the join"));
    wait_until(|| enet_steer::has_room(&session_id)).await;
    assert!(handle.is_active(), "the run serves eNet until it ends");

    // Join: reset, the replay, the end-of-replay marker.
    let sub = enet_steer::subscribe(&session_id, d.endpoint_id());
    let join = vec![r#"{"t":"join","channel":"activity"}"#.to_string()];
    enet_steer::send_envelope(&v, d.endpoint_id(), &session_id, &join).await.expect("join acked");
    assert_eq!(next_real(&sub.frames).await["t"], "activity_reset");
    let replayed = next_real(&sub.frames).await;
    assert_eq!(replayed["t"], "activity");
    assert_eq!(replayed["seq"], 0);
    assert_eq!(replayed["event"]["text"], "before the join");
    let synced = next_real(&sub.frames).await;
    assert_eq!(synced, serde_json::json!({"t": "activity_synced", "firstSeq": 0, "lastSeq": 0}));

    // A live event reaches the joined viewer.
    activity.send(ActivityEvent::narration("live"));
    let live = next_real(&sub.frames).await;
    assert_eq!(live["seq"], 1);
    assert_eq!(live["event"]["text"], "live");

    // Steering: a keystroke reaches the run's input.
    let input = vec![r#"{"t":"input","data":"y"}"#.to_string()];
    enet_steer::send_envelope(&v, d.endpoint_id(), &session_id, &input).await.expect("input acked");
    wait_until(|| recorded.inputs.lock().unwrap().iter().any(|bytes| bytes == b"y")).await;

    // A session this install does not run says so.
    let missing = enet_steer::subscribe("no-such-session", d.endpoint_id());
    enet_steer::send_envelope(&v, d.endpoint_id(), "no-such-session", &join).await.expect("acked");
    assert_eq!(next_real(&missing.frames).await, serde_json::json!({"t": "error", "code": "no_such_session"}));

    // The run ends: the viewer hears `bye`.
    enet_steer::send_envelope(&v, d.endpoint_id(), &session_id, &join).await.expect("re-join acked");
    assert_eq!(next_real(&sub.frames).await["t"], "activity_reset");
    handle.shutdown(Some("exit:0".into()));
    let bye = loop {
        let frame = next_real(&sub.frames).await;
        if frame["t"] == "bye" {
            break frame;
        }
    };
    assert_eq!(bye["outcome"], "exit:0");
    wait_until(|| !enet_steer::has_room(&session_id)).await;
    assert!(recorded.kills.lock().unwrap().is_empty());

    device.stop();
    viewer.stop();
    teammate.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_teammate_is_refused_from_a_private_session() {
    let dir = std::env::temp_dir().join(format!("enet-steer-refused-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "alice", InstallKind::Daemon);
    let teammate = start(&registry, &dir, "bob", InstallKind::Desktop);
    for install in [&device, &teammate] {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in [&device, &teammate] {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let d = device.node().unwrap();
    let t = teammate.node().unwrap();
    d.add_ticket(&t.ticket());
    t.add_ticket(&d.ticket());

    let session_id = format!("sess-private-{}", std::process::id());
    // Leaked: a runtime may not drop inside the test's async context.
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let recorded = Arc::new(Recorded::default());
    let handle = steer::publish(
        runtime,
        PublishSpec {
            session_id: session_id.clone(),
            audience: Some(coding::SteerAudience {
                team_id: "team-1".into(),
                starter_user_id: "alice".into(),
                shared: false,
            }),
            ..Default::default()
        },
        hooks(&recorded),
    );
    wait_until(|| enet_steer::has_room(&session_id)).await;
    let sub = enet_steer::subscribe(&session_id, d.endpoint_id());
    let frames = vec![r#"{"t":"join","channel":"activity"}"#.to_string(), r#"{"t":"input","data":"n"}"#.to_string()];
    enet_steer::send_envelope(&t, d.endpoint_id(), &session_id, &frames).await.expect("acked");
    assert_eq!(next_real(&sub.frames).await, serde_json::json!({"t": "error", "code": "unauthorized"}));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(recorded.inputs.lock().unwrap().is_empty(), "a refused envelope steers nothing");

    handle.shutdown(None);
    device.stop();
    teammate.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The desktop viewer's eNet source in the tests: a fixed node + endpoints.
struct Source {
    node: Option<Arc<enet_core::Node>>,
    endpoints: Vec<String>,
}

impl steer::EnetViewerSource for Source {
    fn node(&self) -> Option<Arc<enet_core::Node>> {
        self.node.clone()
    }
    fn endpoints(&self, _session_id: &str) -> Result<Vec<String>, ApiError> {
        Ok(self.endpoints.clone())
    }
}

async fn next_event(events: &flume::Receiver<steer::ViewerEvent>) -> steer::ViewerEvent {
    tokio::time::timeout(WAIT, events.recv_async()).await.expect("an event in time").expect("viewer running")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_desktop_viewer_steers_over_enet_and_has_no_fallback() {
    use steer::{ViewerEvent, ViewerPhase};
    let dir = std::env::temp_dir().join(format!("enet-steer-viewer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "carol", InstallKind::Daemon);
    let viewer = start(&registry, &dir, "carol", InstallKind::Desktop);
    for install in [&device, &viewer] {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in [&device, &viewer] {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let d = device.node().unwrap();
    let v = viewer.node().unwrap();
    d.add_ticket(&v.ticket());
    v.add_ticket(&d.ticket());

    let session_id = format!("sess-viewer-{}", std::process::id());
    // Leaked: a runtime may not drop inside the test's async context.
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let recorded = Arc::new(Recorded::default());
    let publisher = steer::publish(
        runtime,
        PublishSpec {
            session_id: session_id.clone(),
            audience: Some(coding::SteerAudience {
                team_id: "team-1".into(),
                starter_user_id: "carol".into(),
                shared: false,
            }),
            ..Default::default()
        },
        hooks(&recorded),
    );
    publisher.activity_sender().send(ActivityEvent::narration("hello from the device"));
    wait_until(|| enet_steer::has_room(&session_id)).await;

    // eNet: own endpoint skipped, the device joined, the reducer fed.
    let (events_tx, events) = flume::unbounded();
    let source = Source {
        node: Some(Arc::clone(&v)),
        endpoints: vec![enet::endpoint_hex(&v.endpoint_id()), enet::endpoint_hex(&d.endpoint_id())],
    };
    let handle = steer::spawn_viewer(runtime, Some(Arc::new(source)), session_id.clone(), events_tx);
    let mut seen = Vec::new();
    while !seen.iter().any(|event| matches!(event, ViewerEvent::Synced { .. })) {
        seen.push(next_event(&events).await);
    }
    assert!(seen.contains(&ViewerEvent::Reset));
    assert!(seen.contains(&ViewerEvent::Connected(true)));
    assert!(seen.contains(&ViewerEvent::Phase(ViewerPhase::Live)));
    assert!(seen.iter().any(|event| matches!(
        event,
        ViewerEvent::Activity(0, ActivityEvent::Narration { text, .. }) if text == "hello from the device"
    )));
    assert!(handle.send_message("go"));
    wait_until(|| recorded.inputs.lock().unwrap().iter().any(|bytes| bytes == b"go")).await;
    publisher.activity_sender().send(ActivityEvent::narration("live"));
    loop {
        if let ViewerEvent::Activity(1, _) = next_event(&events).await {
            break;
        }
    }
    // The run ends: the viewer ends with it.
    publisher.shutdown(Some("exit:0".into()));
    loop {
        if let ViewerEvent::Phase(ViewerPhase::Ended { outcome }) = next_event(&events).await {
            assert_eq!(outcome.as_deref(), Some("exit:0"));
            break;
        }
    }

    // VAPP-43 NO FEED (no endpoint): a live row waits in `Starting`…
    let (events_tx, events) = flume::unbounded();
    let nowhere = steer::spawn_viewer(
        runtime,
        Some(Arc::new(Source { node: Some(Arc::clone(&v)), endpoints: Vec::new() })),
        "sess-nowhere".into(),
        events_tx,
    );
    loop {
        if let ViewerEvent::Phase(ViewerPhase::Starting) = next_event(&events).await {
            break;
        }
    }
    // …and an ended one says its device is offline (no relay fallback),
    // VAPP-169: and keeps redialing under that copy.
    nowhere.note_session_ended();
    nowhere.kick();
    loop {
        if let ViewerEvent::Phase(ViewerPhase::Ended { outcome }) = next_event(&events).await {
            assert_eq!(outcome.as_deref(), Some(steer::viewer::DEVICE_OFFLINE));
            break;
        }
    }
    assert!(nowhere.is_active(), "VAPP-169: the device-offline end is not terminal");
    nowhere.shutdown();
    // No node at all: the same.
    let (events_tx, events) = flume::unbounded();
    let offline = steer::spawn_viewer(runtime, None, "sess-no-node".into(), events_tx);
    offline.note_session_ended();
    loop {
        if let ViewerEvent::Phase(ViewerPhase::Ended { outcome }) = next_event(&events).await {
            assert_eq!(outcome.as_deref(), Some(steer::viewer::DEVICE_OFFLINE));
            break;
        }
    }

    device.stop();
    viewer.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The device's synced rows for STORED REPLAY: one session, ended or not.
fn stored(dir: &std::path::Path, session_id: &str, starter: &str, ended: Arc<std::sync::atomic::AtomicBool>) -> enet_steer::StoredJournals {
    let id = session_id.to_string();
    let starter = starter.to_string();
    enet_steer::StoredJournals {
        data_dir: dir.to_path_buf(),
        session: Arc::new(move |session_id| {
            (session_id == id).then(|| enet_steer::StoredSession {
                audience: coding::SteerAudience { team_id: "team-1".into(), starter_user_id: starter.clone(), shared: false },
                ended: ended.load(std::sync::atomic::Ordering::SeqCst),
            })
        }),
    }
}

/// VAPP-43 STORED REPLAY: a join for a session with no live run here is
/// answered off the journal file — an ended row with `bye history`, an idle
/// one by staying joined (pages off the file) until the run comes back and
/// takes the viewer over; a session with a row but no file (or a file but no
/// row) is `no_such_session`, and a teammate is refused a private one.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_journal_is_replayed_over_enet() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = std::env::temp_dir().join(format!("enet-steer-stored-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "dana", InstallKind::Daemon);
    let viewer = start(&registry, &dir, "dana", InstallKind::Desktop);
    let teammate = start(&registry, &dir, "erin", InstallKind::Desktop);
    for install in [&device, &viewer, &teammate] {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in [&device, &viewer, &teammate] {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let d = device.node().unwrap();
    let v = viewer.node().unwrap();
    let t = teammate.node().unwrap();
    for (a, b) in [(&d, &v), (&d, &t)] {
        a.add_ticket(&b.ticket());
        b.add_ticket(&a.ticket());
    }

    let session_id = format!("sess-stored-{}", std::process::id());
    let journals = dir.join("data");
    let mut writer = steer::JournalWriter::open(&journals, &session_id).unwrap();
    for i in 0..3 {
        writer.append(&ActivityEvent::narration(format!("row {i}")));
    }
    writer.append(&ActivityEvent::diff("state"));
    drop(writer);
    let ended = Arc::new(AtomicBool::new(true));
    device.serve_stored(stored(&journals, &session_id, "dana", Arc::clone(&ended)));
    let join = vec![r#"{"t":"join","channel":"activity"}"#.to_string()];

    // An ENDED row: the whole file, synced, then `bye history`.
    let sub = enet_steer::subscribe(&session_id, d.endpoint_id());
    enet_steer::send_envelope(&v, d.endpoint_id(), &session_id, &join).await.expect("join acked");
    assert_eq!(next_real(&sub.frames).await["t"], "activity_reset");
    for i in 0..3 {
        let row = next_real(&sub.frames).await;
        assert_eq!((row["seq"].as_u64(), row["event"]["text"].as_str()), (Some(i), Some(format!("row {i}").as_str())));
    }
    assert_eq!(next_real(&sub.frames).await["event"]["kind"], "diff");
    assert_eq!(next_real(&sub.frames).await, serde_json::json!({"t": "activity_synced", "firstSeq": 0, "lastSeq": 2}));
    assert_eq!(next_real(&sub.frames).await, serde_json::json!({"t": "bye", "outcome": "history"}));

    // A teammate is refused a private session's journal too.
    let refused = enet_steer::subscribe(&session_id, d.endpoint_id());
    drop(sub);
    enet_steer::send_envelope(&t, d.endpoint_id(), &session_id, &join).await.expect("acked");
    assert_eq!(next_real(&refused.frames).await, serde_json::json!({"t": "error", "code": "unauthorized"}));
    drop(refused);

    // No file for a session = no_such_session.
    let missing = enet_steer::subscribe("sess-no-file", d.endpoint_id());
    enet_steer::send_envelope(&v, d.endpoint_id(), "sess-no-file", &join).await.expect("acked");
    assert_eq!(next_real(&missing.frames).await, serde_json::json!({"t": "error", "code": "no_such_session"}));
    drop(missing);

    // An IDLE row: the replay, no `bye`; a page comes off the file.
    ended.store(false, Ordering::SeqCst);
    let sub = enet_steer::subscribe(&session_id, d.endpoint_id());
    enet_steer::send_envelope(&v, d.endpoint_id(), &session_id, &join).await.expect("join acked");
    loop {
        if next_real(&sub.frames).await["t"] == "activity_synced" {
            break;
        }
    }
    wait_until(|| enet_steer::has_stored_viewer(&session_id, &v.endpoint_id())).await;
    let page = vec![r#"{"t":"history_page","requestId":"p1","beforeSeq":2,"limit":1}"#.to_string()];
    enet_steer::send_envelope(&v, d.endpoint_id(), &session_id, &page).await.expect("page acked");
    let chunk = next_real(&sub.frames).await;
    assert_eq!(chunk["t"], "history_chunk");
    assert_eq!(chunk["requestId"], "p1");
    assert_eq!(chunk["seqs"], serde_json::json!([1]));

    // The run comes back here: the stored viewer becomes a live join.
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let recorded = Arc::new(Recorded::default());
    let handle = steer::publish(
        runtime,
        PublishSpec {
            session_id: session_id.clone(),
            journal_dir: Some(journals.clone()),
            audience: Some(coding::SteerAudience {
                team_id: "team-1".into(),
                starter_user_id: "dana".into(),
                shared: false,
            }),
            ..Default::default()
        },
        hooks(&recorded),
    );
    assert_eq!(next_real(&sub.frames).await["t"], "activity_reset");
    assert_eq!(next_real(&sub.frames).await["t"], "activity_synced");
    assert!(!enet_steer::has_stored_viewer(&session_id, &v.endpoint_id()));
    handle.activity_sender().send(ActivityEvent::narration("back"));
    let live = next_real(&sub.frames).await;
    assert_eq!((live["seq"].as_u64(), live["event"]["text"].as_str()), (Some(4), Some("back")));
    handle.shutdown(Some("exit:0".into()));

    device.stop();
    viewer.stop();
    teammate.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// VAPP-167: the daemon VIEWS FOR THE CLI. A CLI on a machine whose daemon
/// holds the install's eNet identity asks the daemon over its local socket;
/// the daemon joins the session's device over its own node and streams the
/// replay back as viewer events, through to the stored replay's `bye`.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_proxies_a_cli_viewer_over_its_local_socket() {
    use steer::{ViewerEvent, ViewerPhase};
    let dir = std::env::temp_dir().join(format!("enet-steer-proxy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "fay", InstallKind::Daemon);
    let daemon = start(&registry, &dir, "fay", InstallKind::Desktop);
    for install in [&device, &daemon] {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in [&device, &daemon] {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let d = device.node().unwrap();
    let v = daemon.node().unwrap();
    d.add_ticket(&v.ticket());
    v.add_ticket(&d.ticket());

    let session_id = format!("sess-proxy-{}", std::process::id());
    let journals = dir.join("data");
    let mut writer = steer::JournalWriter::open(&journals, &session_id).unwrap();
    writer.append(&ActivityEvent::narration("done elsewhere"));
    drop(writer);
    device.serve_stored(stored(&journals, &session_id, "fay", Arc::new(std::sync::atomic::AtomicBool::new(true))));

    let runtime: &'static SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));
    let source: Arc<dyn steer::EnetViewerSource> = Arc::new(Source {
        node: Some(Arc::clone(&v)),
        endpoints: vec![enet::endpoint_hex(&d.endpoint_id())],
    });
    let socket = dir.join("cli-daemon.sock");
    let proxy = steer::viewer_proxy::serve(
        &socket,
        Arc::new(move |session_id, events_tx| steer::spawn_viewer(runtime, Some(Arc::clone(&source)), session_id, events_tx)),
    )
    .unwrap();

    let (events_tx, events) = flume::unbounded();
    let cli = steer::viewer_proxy::follow(&socket, &session_id, true, events_tx).unwrap();
    let mut seen = Vec::new();
    loop {
        let event = next_event(&events).await;
        let done = matches!(event, ViewerEvent::Phase(ViewerPhase::Ended { .. }));
        seen.push(event);
        if done {
            break;
        }
    }
    assert!(seen.iter().any(|event| matches!(
        event,
        ViewerEvent::Activity(0, ActivityEvent::Narration { text, .. }) if text == "done elsewhere"
    )), "{seen:?}");
    assert!(seen.iter().any(|event| matches!(event, ViewerEvent::Synced { .. })), "{seen:?}");
    assert_eq!(seen.last(), Some(&ViewerEvent::Phase(ViewerPhase::Ended { outcome: Some("history".into()) })));
    // The daemon closes the stream after the terminal phase.
    assert!(tokio::time::timeout(WAIT, events.recv_async()).await.expect("closed in time").is_err());

    cli.shutdown();
    proxy.stop();
    device.stop();
    daemon.stop();
    let _ = std::fs::remove_dir_all(&dir);
}


/// VAPP-169 OFFLINE REDIAL: an ended row whose device does not answer shows
/// the device-offline `Ended` and keeps the loop alive; once the device
/// answers, a kick (presence online, a wakeup) and, separately, the stale
/// timer alone redial silently (no `Connecting`) and the stored replay
/// arrives, ending in the plain `bye history`.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_offline_viewer_redials_when_the_device_comes_back() {
    use std::sync::atomic::AtomicBool;
    use steer::{ViewerEvent, ViewerPhase, ViewerTimings};
    let dir = std::env::temp_dir().join(format!("enet-steer-redial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Arc::new(Registry::default());
    let device = start(&registry, &dir, "fred", InstallKind::Daemon);
    let viewer = start(&registry, &dir, "fred", InstallKind::Desktop);
    for install in [&device, &viewer] {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in [&device, &viewer] {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let d = device.node().unwrap();
    let v = viewer.node().unwrap();
    d.add_ticket(&v.ticket());
    v.add_ticket(&d.ticket());

    let session_id = format!("sess-redial-{}", std::process::id());
    let journals = dir.join("data");
    let mut writer = steer::JournalWriter::open(&journals, &session_id).unwrap();
    writer.append(&ActivityEvent::narration("kept on the device"));
    drop(writer);
    device.serve_stored(stored(&journals, &session_id, "fred", Arc::new(AtomicBool::new(true))));
    let runtime: &SteerRuntime = Box::leak(Box::new(SteerRuntime::new().unwrap()));

    for by_kick in [true, false] {
        let source = Arc::new(Redial { node: Arc::clone(&v), endpoints: Mutex::new(Vec::new()) });
        let timings = ViewerTimings {
            retry_base: Duration::from_millis(50),
            retry_cap: Duration::from_millis(100),
            // The kick round must not be rescued by the timer.
            offline_redial: if by_kick { Duration::from_secs(600) } else { Duration::from_millis(300) },
            ..Default::default()
        };
        let (events_tx, events) = flume::unbounded();
        let handle = steer::spawn_viewer_with(runtime, Some(source.clone()), session_id.clone(), events_tx, timings);
        handle.note_session_ended();
        loop {
            if let ViewerEvent::Phase(phase) = next_event(&events).await {
                if phase.is_device_offline() {
                    break;
                }
            }
        }
        assert!(handle.is_active(), "the device-offline end keeps the loop");
        // A few NO FEED redials under the copy: no phase flaps.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(handle.is_active());
        if by_kick {
            assert!(events.try_iter().all(|event| !matches!(event, ViewerEvent::Phase(_))), "the phase holds");
        }
        // The device comes back.
        *source.endpoints.lock().unwrap() = vec![enet::endpoint_hex(&d.endpoint_id())];
        if by_kick {
            handle.kick();
        }
        let mut seen = Vec::new();
        loop {
            let event = next_event(&events).await;
            let done = matches!(&event, ViewerEvent::Phase(ViewerPhase::Ended { .. }));
            seen.push(event);
            if done {
                break;
            }
        }
        assert!(!seen.contains(&ViewerEvent::Phase(ViewerPhase::Connecting)), "a silent redial");
        assert!(seen.contains(&ViewerEvent::Reset));
        assert!(seen.iter().any(|event| matches!(
            event,
            ViewerEvent::Activity(0, ActivityEvent::Narration { text, .. }) if text == "kept on the device"
        )));
        assert!(seen.iter().any(|event| matches!(event, ViewerEvent::Synced { .. })));
        assert_eq!(seen.last(), Some(&ViewerEvent::Phase(ViewerPhase::Ended { outcome: Some("history".into()) })));
        wait_until(|| !handle.is_active()).await;
    }

    device.stop();
    viewer.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// VAPP-169: a source whose endpoints appear once the device is back.
struct Redial {
    node: Arc<enet_core::Node>,
    endpoints: Mutex<Vec<String>>,
}

impl steer::EnetViewerSource for Redial {
    fn node(&self) -> Option<Arc<enet_core::Node>> {
        Some(Arc::clone(&self.node))
    }
    fn endpoints(&self, _session_id: &str) -> Result<Vec<String>, ApiError> {
        Ok(self.endpoints.lock().unwrap().clone())
    }
}
