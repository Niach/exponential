//! #EDE-74 headless: the control plane answers every call and never waits
//! for a client that stopped reading.

mod common;

use std::{
    io::Read,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{events, read_frame, write_frame, Call, CaptureInfo, CaptureTarget, Client, Message, Request, PROTOCOL_VERSION};

const BLUE: u32 = 0xff00_00ff;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

/// A connection the test reads by hand (or not at all), hello already sent.
fn raw(h: &Harness) -> UnixStream {
    let (mut ours, theirs) = UnixStream::pair().expect("socketpair");
    let token = h.with(move |state| {
        state.ipc_insert(theirs);
        state.ipc.root_grant().token
    });
    ours.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    send(&mut ours, 1, Request::Hello { version: PROTOCOL_VERSION, token });
    ours
}

fn send(stream: &mut UnixStream, id: u64, request: Request) {
    write_frame(stream, &serde_json::to_vec(&Call { id, request }).unwrap()).unwrap();
}

fn reply(stream: &mut UnixStream, id: u64) -> serde_json::Value {
    match serde_json::from_slice(&read_frame(stream).expect("a reply in time")).unwrap() {
        Message::Reply { id: got, result } if got == id => result,
        other => panic!("expected the reply to {id}, got {other:?}"),
    }
}

/// The selection's owner exited without a new selection: the read has no
/// pipe to wait for and finishes inside the call. It still answers, and the
/// connection serves the next call.
#[test]
fn a_snapshot_whose_owner_is_gone_answers() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("editor");
    app.draw(&t, 300, 200, BLUE);
    let mut ipc = h.ipc();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.keyboard_focus.is_none() {
        assert!(Instant::now() < deadline, "the window never took focus");
        std::thread::sleep(Duration::from_millis(10));
        app.roundtrip();
    }
    let device = app.data_device();
    let before = ipc.clipboard_serial().unwrap();
    let _source = app.set_selection(&device, &[("text/plain;charset=utf-8", Some(b"copied".as_slice()))]);
    assert!(ipc.clipboard_serial().unwrap() > before);
    drop((device, _source, t, app));
    while !ipc.windows().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "the client never went");
        std::thread::sleep(Duration::from_millis(10));
    }

    let call = std::thread::spawn(move || (ipc.clipboard_snapshot().unwrap(), ipc.clipboard_text().unwrap(), ipc.clipboard_serial().unwrap()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !call.is_finished() {
        assert!(Instant::now() < deadline, "the snapshot never answered");
        std::thread::sleep(Duration::from_millis(10));
    }
    let (snapshot, text, _) = call.join().unwrap();
    assert!(snapshot.items.is_empty());
    assert_eq!(text.text, None);
}

/// Eight captures asked for and never read: megabytes wait in that
/// connection's queue, another client is answered at once, and the first
/// one gets every byte once it reads.
#[test]
fn a_client_that_never_reads_does_not_delay_another_clients_reply() {
    let h = harness();
    let mut other = h.ipc();
    let output = other.outputs().unwrap()[0].name.clone();
    let mut stuck = raw(&h);
    for id in 2..10 {
        send(&mut stuck, id, Request::Capture { target: CaptureTarget::Output { name: output.clone() }, since: None });
    }
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    other.windows().unwrap();
    assert!(started.elapsed() < Duration::from_millis(500), "the other client waited {:?}", started.elapsed());

    reply(&mut stuck, 1);
    for id in 2..10 {
        let info: CaptureInfo = serde_json::from_value(reply(&mut stuck, id)).unwrap();
        assert_eq!((info.width, info.height), (800, 600));
        assert_eq!(read_frame(&mut stuck).expect("the pixels").len(), 800 * 600 * 4);
    }
    // In order, and still served.
    send(&mut stuck, 10, Request::ListWindows);
    reply(&mut stuck, 10);
}

/// A subscriber that never reads is dropped once its queue is full; one
/// that reads hears every event.
#[test]
fn a_subscriber_that_stopped_reading_is_dropped_at_the_cap() {
    let h = harness();
    let mut reader: Client = h.ipc();
    reader.subscribe().unwrap();
    reader.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut stuck = raw(&h);
    send(&mut stuck, 2, Request::Subscribe);
    reader.windows().unwrap();
    const BATCHES: usize = 60;
    for _ in 0..BATCHES {
        h.with(|state| (0..100).for_each(|_| state.emit(events::CONFIG_RELOADED, serde_json::Value::Null)));
        for _ in 0..100 {
            assert_eq!(reader.next_event().unwrap().kind, events::CONFIG_RELOADED);
        }
    }
    // The stuck one was closed: what the socket held, then the end.
    let mut rest = Vec::new();
    stuck.read_to_end(&mut rest).expect("closed, not left hanging");
    assert!(ede_protocol::take_frames(&mut rest).unwrap().len() < BATCHES * 100);
}
