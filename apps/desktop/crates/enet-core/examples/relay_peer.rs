//! A native eNet node driven over stdin/stdout, homed on ONE relay (no n0
//! relays): the native side of the web app's wasm smoke test
//! (scripts/enet-wasm-smoke.ts, EXP-1457), where a browser-build node and
//! this one talk through a local `iroh-relay --dev`.
//!
//! `cargo run -p enet-core --example relay_peer -- http://127.0.0.1:3340`
//!
//! Prints `READY <endpoint id> <ticket>`, then one line per event:
//! `CHAT <from> <utf8 body>`, `REFUSED <peer>`. Reads commands:
//! `ticket <enet1…>`, `allow <id>`, `send <id> <text>` (answers `ACKED <id>`
//! or `SENDERR <id> <error>`), `quit`.

use std::{io::BufRead, str::FromStr, time::Duration};

use enet_core::{EndpointId, Event, Identity, NetConfig, Node, RelayChoice, Ticket};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let relay = std::env::args().nth(1).expect("usage: relay_peer <relay url>");
    let config = NetConfig { relay: RelayChoice::Custom(vec![relay]), ..Default::default() };
    let node = Node::start(config, Identity::generate()).await.expect("node starts");
    node.online(Duration::from_secs(20)).await.expect("relay connection");
    let mut events = node.events().expect("events once");
    println!("READY {} {}", node.endpoint_id(), node.ticket());

    tokio::spawn(async move {
        while let Some(e) = events.recv().await {
            match e {
                Event::Chat { from, message } => {
                    println!("CHAT {from} {}", String::from_utf8_lossy(&message.body))
                }
                Event::PeerRefused { peer, .. } => println!("REFUSED {peer}"),
                _ => {}
            }
        }
    });

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let id = |s: &str| EndpointId::from_str(s).expect("endpoint id");
    while let Some(line) = rx.recv().await {
        let mut parts = line.splitn(3, ' ');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("ticket"), Some(t), _) => node.add_ticket(&Ticket::from_str(t).expect("ticket")),
            (Some("allow"), Some(p), _) => node.access().allow(id(p)),
            (Some("send"), Some(p), body) => match node.send_chat(id(p), body.unwrap_or("").as_bytes().to_vec()).await {
                Ok(_) => println!("ACKED {p}"),
                Err(e) => println!("SENDERR {p} {e}"),
            },
            (Some("quit"), _, _) => break,
            _ => println!("UNKNOWN {line}"),
        }
    }
    node.shutdown().await.ok();
}
