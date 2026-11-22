//! #EDE-162 headless: a space's captures wait their turn on the thread
//! that serves the person's frames. A space that changed is drawn at most
//! once per floor; callers asking at once share one pass; a capture that
//! finds nothing new answers at once; a space closed while its capture
//! waits answers `not_found`.

mod common;

use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::{
    space::{start_with_spaces, wait_for},
    Session, Toplevel,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId, spaces::frames::FLOOR};
use ede_protocol::{CaptureTarget, ErrorCode, SessionId, SpaceId};

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;

/// A 640x480 space with a window of a client.
fn space_with_a_window(h: &Harness) -> (DeskId, SpaceId, Session, Toplevel) {
    let (desk, id, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        let space = state.spaces.get(desk).unwrap();
        (desk, space.id.clone(), space.socket())
    });
    let mut b = Session::connect(UnixStream::connect(socket).expect("the space's socket"));
    let window = b.toplevel("b");
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    b.draw_configured(&window, 64, 64, RED);
    (desk, id, b, window)
}

/// A new buffer right after another waits for the space's next tick (an
/// app presents at most once per tick, #EDE-162): until it went through.
fn applied(h: &Harness, desk: DeskId) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(move |state| state.space_held_commits(desk)) > 0 {
        assert!(Instant::now() < deadline, "a held commit never went through");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn passes(h: &Harness, desk: DeskId) -> u64 {
    h.with(move |state| state.space_passes(desk))
}

/// When space `desk`'s last pass started and when it was done, on the
/// compositor's clock (#EDE-198: when a reply reaches the test is the
/// test's own scheduling).
fn last_pass(h: &Harness, desk: DeskId) -> (Instant, Instant) {
    h.with(move |state| state.space_last_pass(desk)).expect("a pass")
}

/// Space `desk`'s clock rests: no tick is armed, its next buffer goes
/// through at once.
fn rested(h: &Harness, desk: DeskId) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(move |state| state.spaces.get(desk).is_some_and(|s| s.scheduled)) {
        assert!(Instant::now() < deadline, "the space's clock never rests");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_changing_space_is_drawn_at_most_once_per_floor_and_an_unchanged_one_answers_at_once() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, id, mut b, window) = space_with_a_window(&h);
    // The new buffer below goes through at once: the capture after it asks
    // inside the first one's floor.
    rested(&h, desk);
    let mut root = h.ipc();
    let target = || CaptureTarget::Space { space: id.clone() };

    let asked_first = Instant::now();
    let first = root.capture(target()).expect("a capture");
    assert_eq!(first.at(320, 240), RED);
    assert_eq!(passes(&h, desk), 1);
    let (_, first_done) = last_pass(&h, desk);

    // Nothing new: answered at once, with no turn to wait for and no pass.
    assert!(root.capture_since(target(), Some(first.info.serial)).unwrap().is_none());
    assert_eq!(h.with(move |state| state.space_captures_waited(desk)), 0, "an unchanged space's capture waited for its turn");
    assert_eq!(passes(&h, desk), 1);

    // New pixels right away: drawn, but not before the floor is over. The
    // floor holds between the compositor's passes, so that is where it is
    // measured.
    b.draw_configured(&window, 64, 64, GREEN);
    applied(&h, desk);
    let second = root.capture_since(target(), Some(first.info.serial)).unwrap().expect("new pixels");
    let (second_started, _) = last_pass(&h, desk);
    let apart = second_started.duration_since(first_done);
    assert!(apart >= FLOOR, "two passes {apart:?} apart");
    // Nor does a caller see two inside a floor: from before its first ask
    // to its second reply.
    assert!(asked_first.elapsed() >= FLOOR, "a caller saw two passes {:?} apart", asked_first.elapsed());
    assert_eq!(second.at(320, 240), GREEN);
    assert!(second.info.serial > first.info.serial);
    assert_eq!(passes(&h, desk), 2);
}

#[test]
fn callers_that_ask_for_one_space_at_once_share_one_pass() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, id, mut b, window) = space_with_a_window(&h);
    // A long floor: both callers surely ask while it holds.
    h.with(|state| state.set_space_capture_floor(Duration::from_secs(1)));
    let mut root = h.ipc();
    let first = root.capture(CaptureTarget::Space { space: id.clone() }).unwrap();
    b.draw_configured(&window, 64, 64, BLUE);
    applied(&h, desk);

    // Both ask inside the floor of the first pass: both wait, one pass
    // answers both.
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let mut client = h.ipc();
            let (id, since) = (id.clone(), first.info.serial);
            std::thread::spawn(move || client.capture_since(CaptureTarget::Space { space: id }, Some(since)).unwrap().expect("new pixels"))
        })
        .collect();
    let frames: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(frames[0].info.serial, frames[1].info.serial);
    assert_eq!((frames[0].at(320, 240), frames[1].at(320, 240)), (BLUE, BLUE));
    assert_eq!(frames[0].pixels, frames[1].pixels);
    assert_eq!(passes(&h, desk), 2, "one pass for the first capture, one for both callers");
}

#[test]
fn a_space_closed_while_its_capture_waits_answers_not_found() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, id, mut b, window) = space_with_a_window(&h);
    h.with(|state| state.set_space_capture_floor(Duration::from_secs(5)));
    let mut root = h.ipc();
    let first = root.capture(CaptureTarget::Space { space: id.clone() }).unwrap();
    b.draw_configured(&window, 64, 64, GREEN);
    applied(&h, desk);

    let mut client = h.ipc();
    let since = first.info.serial;
    let waiting = std::thread::spawn(move || client.capture_since(CaptureTarget::Space { space: id }, Some(since)));
    // It waits for the floor; the space closes meanwhile.
    std::thread::sleep(Duration::from_millis(200));
    assert!(h.with(move |state| state.close_space(desk)));
    match waiting.join().unwrap() {
        Err(ede_protocol::Error::Failed(failure)) => assert_eq!(failure.code, ErrorCode::NotFound),
        other => panic!("a closed space's capture answered {other:?}"),
    }
    // The connection serves its next call.
    assert!(root.spaces().unwrap().is_empty());
}
