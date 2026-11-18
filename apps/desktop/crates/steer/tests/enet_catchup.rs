//! #EDE-108 S2 BACKGROUND CATCH-UP over eNet (`catchUp` in
//! `fixtures/chat-store.json`): three REAL local eNet nodes of one person,
//! started the way the apps start them. The runner A holds the journal;
//! reader B catches up off it (complete, caught up); A goes offline; reader
//! C then catches up off B's chat store (complete through B's watermark,
//! not "caught up": only a runner confirms). A teammate's node is refused a
//! private session's page; a second pass moves nothing (dedup by the asker's
//! runs). Prints the measured throughput of both walks.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::error::ApiError;
use api::installs::{InstallEntry, RegisterInstall};
use chat_store::catchup::Ask;
use chat_store::sqlite::SqliteStore;
use enet_core::NetConfig;
use steer::enet::{self, InstallKind, InstallNode, InstallRegistry, InstallSpec};
use steer::enet_catchup::{self, CatchUpDeps, CatchUpSession};
use steer::{enet_steer, ActivityEvent};

const WAIT: Duration = Duration::from_secs(20);
/// The transcript's size (`CATCHUP_EVENTS` overrides it to measure the
/// byte cap: past `burstBytes` a walk settles at `bytesPerSecond`).
fn session_events() -> u64 {
    std::env::var("CATCHUP_EVENTS").ok().and_then(|n| n.parse().ok()).unwrap_or(3_000)
}

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

fn start(registry: &Arc<Registry>, dir: &Path, user: &str, device: &str, kind: InstallKind) -> InstallNode {
    let spec = InstallSpec {
        kind,
        user_id: user.into(),
        device_id: Some(device.into()),
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

/// One private session of dana's, ended (what every holder's synced row says).
fn stored(dir: &Path, session_id: &str, device: &str, store: Option<enet_catchup::ChatStoreSlot>) -> enet_steer::StoredJournals {
    let id = session_id.to_string();
    enet_steer::StoredJournals {
        data_dir: dir.to_path_buf(),
        session: Arc::new(move |session_id| {
            (session_id == id).then(|| enet_steer::StoredSession {
                audience: coding::SteerAudience { team_id: "team-1".into(), starter_user_id: "dana".into(), shared: false },
                ended: true,
            })
        }),
        device_id: Some(device.to_string()),
        chat_store: store,
    }
}

async fn wait_until(what: &str, limit: Duration, predicate: impl Fn() -> bool) {
    tokio::time::timeout(limit, async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} in time"));
}

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("enet-catchup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn deps(install: &InstallNode, store: &Arc<SqliteStore>, device: &str, session: &CatchUpSession) -> CatchUpDeps {
    let session = session.clone();
    CatchUpDeps {
        install: install.clone(),
        store: Arc::clone(store),
        own_device_id: device.into(),
        user_id: "dana".into(),
        sessions: Arc::new(move || vec![session.clone()]),
        may_run: Arc::new(|| true),
        on_stored: None,
    }
}

fn span(store: &SqliteStore, session_id: &str) -> u64 {
    store.span(session_id, "device-a").unwrap().map_or(0, |span| span.count)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reader_serves_another_reader_while_the_runner_is_offline() {
    let session_events = session_events();
    let registry = Arc::new(Registry::default());
    let (dir_a, dir_b, dir_c, dir_t) = (dir("a"), dir("b"), dir("c"), dir("t"));
    let a = start(&registry, &dir_a, "dana", "device-a", InstallKind::Daemon);
    let b = start(&registry, &dir_b, "dana", "device-b", InstallKind::Desktop);
    let c = start(&registry, &dir_c, "dana", "device-c", InstallKind::Desktop);
    let teammate = start(&registry, &dir_t, "erin", "device-t", InstallKind::Desktop);
    let all = [&a, &b, &c, &teammate];
    for install in all {
        tokio::time::timeout(WAIT, install.passes_at_least(1)).await.unwrap();
    }
    for install in all {
        install.refresh();
        tokio::time::timeout(WAIT, install.passes_at_least(2)).await.unwrap();
    }
    let nodes: Vec<_> = all.iter().map(|install| install.node().unwrap()).collect();
    for x in &nodes {
        for y in &nodes {
            if x.endpoint_id() != y.endpoint_id() {
                x.add_ticket(&y.ticket());
            }
        }
    }

    // The runner's journal: prose and tool output, ~1 KiB per event on
    // average (a real transcript's mix).
    let session_id = format!("sess-catchup-{}", std::process::id());
    let mut writer = steer::JournalWriter::open(&dir_a, &session_id).unwrap();
    for i in 0..session_events {
        let event = match i % 3 {
            0 => ActivityEvent::user_message(format!("ask {i}")),
            1 => ActivityEvent::narration(format!("reply {i}: {}", "lorem ipsum ".repeat(20))),
            _ => ActivityEvent::narration(format!("output {i}: {}", "x".repeat(2_400))),
        };
        writer.append(&event);
    }
    drop(writer);
    let journal_bytes = std::fs::metadata(steer::history::journal_path(&dir_a, &session_id).unwrap()).unwrap().len();
    a.serve_stored(stored(&dir_a, &session_id, "device-a", None));
    let store_b = Arc::new(SqliteStore::open(&dir_b.join("chat-store.sqlite")).unwrap());
    let store_c = Arc::new(SqliteStore::open(&dir_c.join("chat-store.sqlite")).unwrap());
    b.serve_stored(stored(&dir_b, &session_id, "device-b", Some(Arc::new(Mutex::new(Some(Arc::clone(&store_b)))))));
    c.serve_stored(stored(&dir_c, &session_id, "device-c", Some(Arc::new(Mutex::new(Some(Arc::clone(&store_c)))))));
    let session = CatchUpSession {
        session_id: session_id.clone(),
        root_id: session_id.clone(),
        device_id: "device-a".into(),
        updated_at: Some("2026-11-17T10:00:00.000Z".into()),
        busy: false,
    };

    // B off the runner: everything, complete, caught up.
    let runtime = tokio::runtime::Handle::current();
    let started = Instant::now();
    let loop_b = enet_catchup::spawn(&runtime, deps(&b, &store_b, "device-b", &session));
    wait_until("B caught up", Duration::from_secs(60), || loop_b.stats.caught_up.load(Ordering::SeqCst) == 1).await;
    let b_elapsed = started.elapsed();
    assert_eq!(span(&store_b, &session_id), session_events);
    assert_eq!(store_b.complete_to(&session_id, "device-a").unwrap(), Some(session_events - 1));
    let b_bytes = loop_b.stats.bytes.load(Ordering::SeqCst);
    drop(loop_b);

    // A teammate is refused a private session's page (the viewer rule).
    let refused = enet_catchup::ask_page(
        &nodes[3],
        nodes[1].endpoint_id(),
        &session_id,
        "device-a",
        &Ask { after_seq: None, limit: 10, have: Vec::new(), min_at: None },
    )
    .await;
    assert_eq!(refused.unwrap_err(), "unauthorized");

    // The runner goes offline; C walks off B's store.
    a.stop();
    let started = Instant::now();
    let loop_c = enet_catchup::spawn(&runtime, deps(&c, &store_c, "device-c", &session));
    let first_event = Arc::new(Mutex::new(None::<Instant>));
    {
        let first_event = Arc::clone(&first_event);
        let store_c = Arc::clone(&store_c);
        let id = session_id.clone();
        wait_until("C holds every event", Duration::from_secs(120), move || {
            let held = span(&store_c, &id);
            if held > 0 {
                first_event.lock().unwrap().get_or_insert_with(Instant::now);
            }
            held == session_events
        })
        .await;
    }
    let c_elapsed = first_event.lock().unwrap().map_or(started.elapsed(), |first| first.elapsed());
    wait_until("C's pass done", Duration::from_secs(60), || loop_c.stats.passes.load(Ordering::SeqCst) >= 1).await;
    assert_eq!(store_c.complete_to(&session_id, "device-a").unwrap(), Some(session_events - 1), "B was complete, so C is");
    assert_eq!(loop_c.stats.caught_up.load(Ordering::SeqCst), 0, "readers alone never confirm a session");
    let c_bytes = loop_c.stats.bytes.load(Ordering::SeqCst);

    // A second walk moves nothing: the asker's runs cover every seq.
    let before = (loop_c.stats.asks.load(Ordering::SeqCst), loop_c.stats.events.load(Ordering::SeqCst));
    let again = enet_catchup::ask_page(
        &nodes[2],
        nodes[1].endpoint_id(),
        &session_id,
        "device-a",
        &Ask { after_seq: None, limit: 200, have: store_c.have_runs(&session_id, "device-a", None, 64).unwrap(), min_at: None },
    )
    .await
    .unwrap();
    assert!(again.events.is_empty() && again.answer.done);
    assert_eq!(loop_c.stats.events.load(Ordering::SeqCst), before.1);
    drop(loop_c);

    // The two stores read back the same transcript.
    let read = |store: &SqliteStore| -> Vec<(u64, serde_json::Value)> {
        store.read_events(&session_id, "device-a", None, None).unwrap().into_iter().map(|e| (e.seq, e.event)).collect()
    };
    assert_eq!(read(&store_b), read(&store_c));

    let rate = |bytes: u64, elapsed: Duration| bytes as f64 / 1_048_576.0 / elapsed.as_secs_f64();
    println!(
        "CATCH-UP THROUGHPUT: journal {journal_bytes} B, {session_events} events. runner A -> reader B: {} B on the wire in {:.0} ms ({:.2} MiB/s, {:.0} events/s); reader B -> reader C (runner offline): {} B in {:.0} ms ({:.2} MiB/s, {:.0} events/s); cap {} MiB/s after a {} MiB burst",
        b_bytes,
        b_elapsed.as_secs_f64() * 1000.0,
        rate(b_bytes, b_elapsed),
        session_events as f64 / b_elapsed.as_secs_f64(),
        c_bytes,
        c_elapsed.as_secs_f64() * 1000.0,
        rate(c_bytes, c_elapsed),
        session_events as f64 / c_elapsed.as_secs_f64(),
        chat_store::catchup::BYTES_PER_SECOND / 1_048_576,
        chat_store::catchup::BURST_BYTES / 1_048_576,
    );
    for dir in [dir_a, dir_b, dir_c, dir_t] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
