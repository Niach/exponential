//! #EDE-162 headless: a space's clock asks whether its scene changed with
//! no renderer (nothing is imported, so a GPU client's buffers count as a
//! software client's do). Its serial moves with what the space shows and
//! with nothing else.

mod common;

use std::{os::unix::net::UnixStream, time::Duration};

use common::{
    space::{start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId};
use ede_protocol::SessionId;

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;

/// A 640x480 space and a client of it.
fn space_with_a_client(h: &Harness) -> (DeskId, Session) {
    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    (desk, Session::connect(UnixStream::connect(socket).expect("the space's socket")))
}

/// The space's serial once its clock rests.
fn settled(h: &Harness, desk: DeskId) -> u64 {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let (scheduled, serial) = h.with(move |state| state.spaces.get(desk).map(|s| (s.scheduled, s.serial)).unwrap());
        if !scheduled {
            return serial;
        }
        assert!(std::time::Instant::now() < deadline, "the space's clock never rests");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn capture(h: &Harness, desk: DeskId) -> ede_compositor::backend::headless::Frame {
    h.with(move |state| state.capture_space(desk)).expect("a capture")
}

/// New pixels move the serial: a new buffer, a subsurface that maps or
/// moves, a popup. A commit that only asks for the next frame does not.
#[test]
fn a_spaces_serial_moves_with_what_it_shows_only() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, mut b) = space_with_a_client(&h);
    let window = b.toplevel("b");
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    b.draw_configured(&window, 64, 64, RED);
    let drawn = settled(&h, desk);
    assert!(drawn > 0);

    // An animation that draws nothing new.
    assert!(b.count_frames(&window.surface, Duration::from_millis(400)) > 0);
    assert_eq!(settled(&h, desk), drawn, "frame callbacks alone change nothing");

    // A subsurface shows with its parent's commit, then moves.
    let (_sub, subsurface) = b.subsurface(&window.surface, (10, 10), (20, 20), GREEN);
    let mapped = settled(&h, desk);
    assert!(mapped > drawn, "a mapped subsurface is new pixels");
    assert_eq!(capture(&h, desk).at(15, 15), GREEN);
    subsurface.set_position(100, 100);
    window.surface.commit();
    b.roundtrip();
    let moved = settled(&h, desk);
    assert!(moved > mapped, "a moved subsurface is new pixels");
    let frame = capture(&h, desk);
    assert_eq!((frame.at(15, 15), frame.at(105, 105)), (RED, GREEN));

    // A popup of the window.
    let _popup = b.window_popup(&window, (200, 200), (30, 30), false, BLUE);
    let popped = settled(&h, desk);
    assert!(popped > moved, "a mapped popup is new pixels");
    assert!(capture(&h, desk).pixels.contains(&BLUE));

    // Looking changes nothing either.
    assert_eq!(settled(&h, desk), popped);
}

/// A protected window is black in its space's capture, and its space's
/// serial stands still while it is the only thing that changes: its black
/// box is the same element on every tick (it was a new one each time, so
/// every tick moved the serial and a `since` capture always drew).
#[test]
fn a_protected_window_in_a_space_is_black_and_keeps_its_serial_still() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, mut b) = space_with_a_client(&h);
    let secret = b.toplevel("vault");
    secret.toplevel.set_app_id("org.keepassxc.KeePassXC".into());
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    b.draw_configured(&secret, 64, 64, RED);
    let drawn = settled(&h, desk);

    assert!(b.count_frames(&secret.surface, Duration::from_millis(400)) > 0);
    assert_eq!(settled(&h, desk), drawn, "a still protected window is no damage");
    let frame = capture(&h, desk);
    assert_eq!(frame.at(320, 240), BLACK);
    assert!(!frame.pixels.contains(&RED), "nothing of the protected window shows");
    assert_eq!(settled(&h, desk), drawn);

    // Its new pixels are not shown, so they are no damage either.
    b.draw_configured(&secret, 64, 64, GREEN);
    assert_eq!(settled(&h, desk), drawn, "what a capture cannot show does not move the serial");
    assert!(!capture(&h, desk).pixels.contains(&GREEN));
}
