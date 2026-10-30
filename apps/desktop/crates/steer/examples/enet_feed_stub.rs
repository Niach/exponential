//! VAPP-168: a stand-in DEVICE for the store-screenshot runs. Since VAPP-43
//! the steer relay carries no session content, so the scripted "live
//! steering" feed is served the way a real desktop serves a running session:
//! over eNet's chat channel, `exp-steer` envelopes per
//! `packages/domain-contract/fixtures/steer-enet.json`.
//!
//! Driven by `apps/web/scripts/screenshot-desktop.ts`, which writes the feed
//! file, registers this node's endpoint as the demo machine's install (so
//! `installs.forSession` names it and every viewer's allowlist holds it) and
//! keeps it running for the capture:
//!
//! `cargo run -p steer --example enet_feed_stub -- <identity file> <feed.json>`
//!
//! The feed file: `{sessionId, journal: [frame…], live: [frame…], liveEveryMs}`,
//! every frame an `activity {event, seq}` object. `journal` is held from the
//! start; `live` is appended one frame per `liveEveryMs` and pushed to every
//! joined viewer as it lands, like a running agent's next events.
//!
//! On `join`: `activity_reset`, the journal so far, `activity_synced
//! {firstSeq, lastSeq}`, then live frames and a `keepalive` every
//! `keepaliveMs`. `leave` drops the viewer; a re-sent `join` restarts its
//! feed; another session's `join` = `error no_such_session`; `history_page`
//! is answered `done` (nothing was ever evicted); steering frames are logged.
//! Every endpoint is let in: the stub has no registry to check against.
//!
//! Prints `READY <endpoint id hex> <x25519 public>` once online on the relay
//! and `TICKET <enet1…>` (for a hand-driven viewer, e.g. enet-core's
//! `relay_peer` example), then one line per viewer event.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use enet_core::{EndpointId, Event, NetConfig, Node, RelayUrl, Ticket, DEFAULT_RELAY_URL};
use serde::Deserialize;
use serde_json::{json, Value};
use steer::enet::{endpoint_hex, load_or_create_identity, x25519_public};
use steer::enet_steer::{
    batches, envelope_overhead, parse_envelope, send_envelope, BATCH_MAX_BYTES, CHAT_MAX_BYTES, KEEPALIVE,
};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Feed {
    session_id: String,
    journal: Vec<Value>,
    #[serde(default)]
    live: Vec<Value>,
    #[serde(default = "default_live_every")]
    live_every_ms: u64,
}

fn default_live_every() -> u64 {
    1_500
}

/// The recorded frames (serialized) and the live tap, under one lock so a
/// joining viewer's snapshot and subscription never miss or double a frame.
struct Journal {
    frames: Vec<String>,
    seqs: Vec<i64>,
    live: broadcast::Sender<String>,
}

struct Stub {
    node: Node,
    session_id: String,
    journal: Mutex<Journal>,
    viewers: Mutex<HashMap<EndpointId, JoinHandle<()>>>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: enet_feed_stub <identity file> <feed.json>";
    let identity_path = PathBuf::from(args.next().expect(usage));
    let feed_path = PathBuf::from(args.next().expect(usage));
    let feed: Feed = serde_json::from_slice(&std::fs::read(&feed_path).expect("feed file reads"))
        .expect("feed file parses");

    let identity = load_or_create_identity(&identity_path).expect("identity loads");
    let x25519 = x25519_public(&identity);
    let config = NetConfig { allow_all: true, ..NetConfig::default() };
    let node = Node::start(config, identity).await.expect("node starts");
    node.online(Duration::from_secs(30)).await.expect("relay connection");
    let mut events = node.events().expect("events once");

    let (live_tx, _) = broadcast::channel(256);
    let mut journal = Journal { frames: Vec::new(), seqs: Vec::new(), live: live_tx };
    for frame in &feed.journal {
        record(&mut journal, frame);
    }
    let stub = Arc::new(Stub {
        node,
        session_id: feed.session_id.clone(),
        journal: Mutex::new(journal),
        viewers: Mutex::new(HashMap::new()),
    });
    println!("READY {} {x25519}", endpoint_hex(&stub.node.endpoint_id()));
    println!("TICKET {}", stub.node.ticket());

    let live_stub = stub.clone();
    tokio::spawn(async move {
        for frame in feed.live {
            tokio::time::sleep(Duration::from_millis(feed.live_every_ms)).await;
            let mut journal = live_stub.journal.lock().unwrap();
            let serialized = record(&mut journal, &frame);
            let _ = journal.live.send(serialized);
        }
        println!("LIVE done");
    });

    while let Some(event) = events.recv().await {
        if let Event::Chat { from, message } = event {
            on_chat(&stub, from, &message.body);
        }
    }
}

fn record(journal: &mut Journal, frame: &Value) -> String {
    journal.seqs.push(frame.get("seq").and_then(Value::as_i64).unwrap_or(0));
    let serialized = frame.to_string();
    journal.frames.push(serialized.clone());
    serialized
}

fn on_chat(stub: &Arc<Stub>, from: EndpointId, body: &[u8]) {
    let Some(envelope) = parse_envelope(body) else { return };
    // The viewer dialed us; answering goes out on our own stream to it.
    let mut ticket = Ticket::new(from);
    ticket.relays = vec![DEFAULT_RELAY_URL.parse::<RelayUrl>().expect("relay url")];
    stub.node.add_ticket(&ticket);
    let peer = endpoint_hex(&from);
    if envelope.session_id != stub.session_id {
        println!("REFUSED {peer} {}", envelope.session_id);
        let stub = stub.clone();
        let session_id = envelope.session_id.clone();
        tokio::spawn(async move {
            let frame = json!({ "t": "error", "code": "no_such_session" }).to_string();
            let _ = send_envelope(&stub.node, from, &session_id, &[frame]).await;
        });
        return;
    }
    for frame in &envelope.frames {
        match frame.get("t").and_then(Value::as_str) {
            Some("join") => {
                println!("JOIN {peer}");
                let feed = tokio::spawn(serve(stub.clone(), from));
                if let Some(old) = stub.viewers.lock().unwrap().insert(from, feed) {
                    old.abort();
                }
            }
            Some("leave") => {
                println!("LEAVE {peer}");
                if let Some(old) = stub.viewers.lock().unwrap().remove(&from) {
                    old.abort();
                }
            }
            Some("history_page") => {
                let chunk = json!({
                    "t": "history_chunk",
                    "requestId": frame.get("requestId").cloned().unwrap_or(Value::Null),
                    "events": [],
                    "seqs": [],
                    "done": true,
                })
                .to_string();
                let stub = stub.clone();
                tokio::spawn(async move {
                    let _ = send_envelope(&stub.node, from, &stub.session_id, &[chunk]).await;
                });
            }
            _ => println!("CLIENT {peer} {frame}"),
        }
    }
}

/// One viewer's feed: the replay up to `activity_synced`, then live frames
/// and keepalives until a send fails (the viewer is dropped).
async fn serve(stub: Arc<Stub>, peer: EndpointId) {
    let (mut replay, mut live) = {
        let journal = stub.journal.lock().unwrap();
        let synced = json!({
            "t": "activity_synced",
            "firstSeq": journal.seqs.first().copied().unwrap_or(0),
            "lastSeq": journal.seqs.last().copied().unwrap_or(0),
        });
        let mut replay = vec![json!({ "t": "activity_reset" }).to_string()];
        replay.extend(journal.frames.iter().cloned());
        replay.push(synced.to_string());
        (replay, journal.live.subscribe())
    };
    if send_batched(&stub, peer, &mut replay).await.is_err() {
        return;
    }
    println!("SYNCED {}", endpoint_hex(&peer));
    let mut keepalive = tokio::time::interval(KEEPALIVE);
    keepalive.tick().await;
    loop {
        let mut frames = tokio::select! {
            frame = live.recv() => match frame {
                Ok(frame) => vec![frame],
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            _ = keepalive.tick() => vec![json!({ "t": "keepalive" }).to_string()],
        };
        if send_batched(&stub, peer, &mut frames).await.is_err() {
            return;
        }
    }
}

async fn send_batched(stub: &Stub, peer: EndpointId, frames: &mut Vec<String>) -> Result<(), ()> {
    let sizes: Vec<usize> = frames.iter().map(String::len).collect();
    let overhead = envelope_overhead(&stub.session_id);
    let (groups, dropped) = batches(&sizes, overhead, BATCH_MAX_BYTES, CHAT_MAX_BYTES);
    for index in dropped {
        eprintln!("DROPPED frame {index} ({} bytes over the chat cap)", sizes[index]);
    }
    for group in groups {
        let envelope: Vec<String> = group.iter().map(|&index| std::mem::take(&mut frames[index])).collect();
        if let Err(err) = send_envelope(&stub.node, peer, &stub.session_id, &envelope).await {
            println!("DROP {} {err}", endpoint_hex(&peer));
            return Err(());
        }
    }
    Ok(())
}
