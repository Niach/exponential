//! EXP-283 regression — publisher close-code semantics against a FAKE relay
//! socket (pure Rust, no bun needed): a relay-initiated close with
//! CLOSE_SESSION_ENDED (4001) is what deployed relays' idle-publisher
//! detector sends after ~90s of socket silence (laptop suspend, network
//! stall — the desktop reads the buffered close frame on wake). It is a
//! transport-level signal, NOT a session end: the publisher must treat it as
//! a plain drop — reconnect + re-hello — and NEVER fire the kill hook, which
//! tears down the live agent child and the whole terminal tab. Real kills
//! ride the explicit relay `kill` frame (covered by relay_integration.rs)
//! and the §8.8 own-row Electric flip (sync::kill_watch).

use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use api::error::ApiError;
use api::steer::MintedTicket;
use steer::publisher::{publish, KillSignal, PublishSpec, PublisherHooks, PublisherTickets};
use steer::SteerRuntime;

struct CountingTickets {
    url: String,
    mints: Arc<AtomicUsize>,
}

impl PublisherTickets for CountingTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        self.mints.fetch_add(1, Ordering::SeqCst);
        Ok(Some(MintedTicket {
            ticket: "fake".to_string(),
            url: self.url.clone(),
        }))
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Accept one ws connection and read frames until the publisher's `hello`.
async fn accept_until_hello(
    listener: &tokio::net::TcpListener,
    hellos: &AtomicUsize,
    which: &str,
) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    let (stream, _) = listener
        .accept()
        .await
        .unwrap_or_else(|e| panic!("accept {which}: {e}"));
    let mut ws = tokio_tungstenite::accept_async(stream)
        .await
        .unwrap_or_else(|e| panic!("handshake {which}: {e}"));
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) if text.contains(r#""t":"hello""#) => {
                hellos.fetch_add(1, Ordering::SeqCst);
                return ws;
            }
            Some(Ok(_)) => continue,
            other => panic!("{which} ended before hello: {other:?}"),
        }
    }
}

#[test]
fn idle_close_4001_reconnects_and_never_fires_the_kill_hook() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake relay");
    let port = listener.local_addr().expect("local addr").port();

    let hellos = Arc::new(AtomicUsize::new(0));
    let hellos_in_server = hellos.clone();

    // The fake relay: connection 1 reads `hello`, then closes with 4001 (the
    // idle-publisher eviction). Connection 2 is the reconnect — read `hello`,
    // then hold the socket open (the resumed room) until the publisher says
    // bye.
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("fake relay runtime");
        rt.block_on(async move {
            listener.set_nonblocking(true).expect("nonblocking");
            let listener =
                tokio::net::TcpListener::from_std(listener).expect("tokio listener");

            let mut ws = accept_until_hello(&listener, &hellos_in_server, "conn 1").await;
            ws.send(Message::Close(Some(CloseFrame {
                code: CloseCode::from(4001u16),
                reason: "publisher_idle_91s".into(),
            })))
            .await
            .expect("close 4001");
            // Drain until the client acks the close and the socket ends.
            while let Some(Ok(_)) = ws.next().await {}

            let mut ws = accept_until_hello(&listener, &hellos_in_server, "conn 2").await;
            // Hold until the publisher's clean bye/close (test teardown).
            while let Some(Ok(_)) = ws.next().await {}
        });
    });

    let runtime = SteerRuntime::new().expect("steer runtime");
    let mints = Arc::new(AtomicUsize::new(0));
    let kills: Arc<Mutex<Vec<KillSignal>>> = Arc::new(Mutex::new(Vec::new()));
    let kills_in_hook = kills.clone();

    let handle = publish(
        &runtime,
        PublishSpec {
            session_id: "sess-exp-283".to_string(),
            issue_id: None,
        },
        Arc::new(CountingTickets {
            url: format!("ws://127.0.0.1:{port}/session"),
            mints: mints.clone(),
        }),
        PublisherHooks {
            write_input: Arc::new(|_| {}),
            kill: Arc::new(move |signal| kills_in_hook.lock().unwrap().push(signal)),
            error: Arc::new(|_| {}),
            answers: None,
            agent: steer::activity::SessionAgent::Claude,
            text_sink: None,
            attachments: None,
            commands: None,
        },
    );

    // Surviving the 4001 close means a SECOND hello lands on the fake relay.
    wait_for("re-hello after the 4001 close", || {
        hellos.load(Ordering::SeqCst) >= 2
    });
    assert!(
        kills.lock().expect("kills lock").is_empty(),
        "a relay-initiated close must never fire the kill hook (EXP-283)"
    );
    assert!(
        handle.is_active(),
        "publisher must still be running after the reconnect"
    );
    assert!(
        mints.load(Ordering::SeqCst) >= 2,
        "the reconnect must re-mint a fresh ticket"
    );

    handle.shutdown(None);
    server.join().expect("fake relay thread");
}
