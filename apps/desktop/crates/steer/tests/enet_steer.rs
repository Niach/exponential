//! VAPP-33: steer over eNet's chat channel (`fixtures/steer-enet.json`).
//! Two REAL local eNet nodes started the way the apps start them: the
//! device runs a publisher (relay disabled, so eNet is all it serves), the
//! viewer joins over chat, gets the journal replay, a live event and steers
//! an input; the run's end says `bye`. A teammate is refused from a private
//! session (its own test: both nodes live in one process, so they share the
//! viewer subscriptions).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use api::steer::MintedTicket;
use enet_core::NetConfig;
use serde_json::Value;
use steer::enet::{self, InstallKind, InstallNode, InstallRegistry, InstallSpec};
use steer::enet_steer;
use steer::{
    ActivityEvent, ImageEmbeds, KillSignal, PublishSpec, PublisherHooks, PublisherTickets, SessionAgent, SteerRuntime,
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

/// The relay is disabled: the publisher serves eNet only.
struct NoRelay;

impl PublisherTickets for NoRelay {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        Ok(None)
    }
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
        error: Arc::new(|_| {}),
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
        Arc::new(NoRelay),
        hooks(&recorded),
    );
    let activity = handle.activity_sender();
    activity.send(ActivityEvent::narration("before the join"));
    wait_until(|| enet_steer::has_room(&session_id)).await;
    assert!(handle.is_active(), "a disabled relay keeps serving eNet");

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
        Arc::new(NoRelay),
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
