//! #EDE-266 headless: who drives an agent space (`control_space`, the
//! computer-use tray's Pause, Take over and Hand back). Paused or taken
//! over, the space grant's driving ops are `conflict` and its captures
//! still answer; what the agent had down is let go at the switch. Taken
//! over, the person's pointer over the shell's view drives the space's
//! pointer (fit = contain, the letterbox does nothing) and a press in the
//! picture gives the space their keys until a press elsewhere.

mod common;

use std::os::unix::net::UnixStream;

use common::{
    space::{person_key, start_with_spaces, wait_for},
    Session,
};
use ede_compositor::backend::headless::Harness;
use ede_protocol::{Capability, CaptureTarget, ClipboardItem, Error, ErrorCode, InputEvent, SessionId, SpaceControl, SpaceViewRect, SPACE_VIEW_NAMESPACE};
use smithay::backend::input::ButtonState;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
// evdev codes; xkb = evdev + 8
const KEY_A: u32 = 30;
const KEY_B: u32 = 48;
const BTN_LEFT: u32 = 0x110;

fn code<T: std::fmt::Debug>(r: Result<T, Error>) -> ErrorCode {
    match r {
        Err(Error::Failed(f)) => f.code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn message<T: std::fmt::Debug>(r: Result<T, Error>) -> String {
    match r {
        Err(Error::Failed(f)) => f.message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn space_grant(h: &Harness, session: &str) -> ede_protocol::Client {
    let grant = h.ipc().mint(SessionId::new(session).unwrap(), vec![Capability::Space], None).unwrap();
    h.ipc_with(grant.token).unwrap()
}

/// The space's own app: one 640x480 window that has the space's keyboard.
fn space_app(socket: &str) -> (Session, common::Toplevel) {
    let mut b = Session::connect(UnixStream::connect(socket).expect("the space's socket"));
    b.keyboard();
    b.pointer();
    let window = b.toplevel("b");
    b.draw(&window, 640, 480, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    (b, window)
}

/// Paused: the agent's input, focus, close, launch and clipboard set are
/// `conflict` and its captures, lists and clipboard reads answer; the key
/// it held is let go. Agent again: input lands. Person needs a view.
#[test]
fn a_paused_space_refuses_its_agents_input_and_still_captures() {
    let (h, _dir) = start_with_spaces(&[(1280, 800)]);
    let mut root = h.ipc();
    let mut agent = space_grant(&h, "run-1");
    let space = agent.create_space(640, 480).unwrap();
    assert_eq!(space.control, SpaceControl::Agent, "an agent drives a new space");
    let (mut b, _window) = space_app(&space.wayland_display);
    let window = agent.windows().unwrap()[0].id.clone();

    // The agent holds A down, then the person pauses: A is let go in the space.
    agent.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap();
    wait_for(&mut b, |b| b.state.keys.contains(&KEY_A));
    root.control_space(space.id.clone(), SpaceControl::Paused, None).unwrap();
    wait_for(&mut b, |b| b.state.key_releases.contains(&KEY_A));
    assert!(h.with(|ede| ede.spaces.iter().all(|s| s.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty())), "nothing stays down in the space");
    assert_eq!(root.spaces().unwrap()[0].control, SpaceControl::Paused);
    assert_eq!(agent.spaces().unwrap()[0].control, SpaceControl::Paused, "its agent sees it is paused");

    let paused = "the person paused computer use in this space; wait and try again";
    assert_eq!(code(agent.input(InputEvent::Key { key: KEY_B, pressed: true })), ErrorCode::Conflict);
    assert_eq!(message(agent.input(InputEvent::PointerMotion { x: 10.0, y: 10.0 })), paused);
    assert_eq!(code(agent.input(InputEvent::Key { key: KEY_A, pressed: false })), ErrorCode::Conflict, "a release too: nothing is down");
    assert_eq!(code(agent.focus(window.clone())), ErrorCode::Conflict);
    assert_eq!(code(agent.close_window(window.clone())), ErrorCode::Conflict);
    assert_eq!(code(agent.launch(vec!["true".into()], Default::default(), None)), ErrorCode::Conflict);
    assert_eq!(code(agent.clipboard_set(vec![ClipboardItem { mime: "text/plain".into(), data: b"x".to_vec() }], false)), ErrorCode::Conflict);
    b.roundtrip();
    assert!(!b.state.keys.contains(&KEY_B), "nothing of the refused input landed");

    // Observation is never blocked (L-10).
    let capture = agent.capture(CaptureTarget::Space { space: space.id.clone() }).expect("a paused space still captures");
    assert_eq!((capture.info.width, capture.info.height), (640, 480));
    assert_eq!(agent.windows().unwrap().len(), 1);
    assert_eq!(agent.outputs().unwrap().len(), 1);
    agent.clipboard_serial().unwrap();
    agent.clipboard_text().unwrap();
    agent.pointer().unwrap();

    // Taking it over needs the view it is driven through.
    assert_eq!(code(root.control_space(space.id.clone(), SpaceControl::Person, None)), ErrorCode::Invalid);
    let bad = SpaceViewRect { namespace: SPACE_VIEW_NAMESPACE.into(), x: 0, y: 0, width: 0, height: 10 };
    assert_eq!(code(root.control_space(space.id.clone(), SpaceControl::Person, Some(bad))), ErrorCode::Invalid);
    assert_eq!(root.spaces().unwrap()[0].control, SpaceControl::Paused, "a refused switch changes nothing");
    let view = SpaceViewRect { namespace: SPACE_VIEW_NAMESPACE.into(), x: 0, y: 0, width: 400, height: 300 };
    root.control_space(space.id.clone(), SpaceControl::Person, Some(view)).unwrap();
    assert_eq!(message(agent.input(InputEvent::Key { key: KEY_B, pressed: true })), "the person has taken over this space; wait until they hand it back");
    agent.capture(CaptureTarget::Space { space: space.id.clone() }).expect("a taken-over space still captures");

    // Handed back: the agent drives again.
    root.control_space(space.id.clone(), SpaceControl::Agent, None).unwrap();
    agent.input(InputEvent::Key { key: KEY_B, pressed: true }).unwrap();
    agent.input(InputEvent::Key { key: KEY_B, pressed: false }).unwrap();
    wait_for(&mut b, |b| b.state.keys.contains(&KEY_B));
    agent.focus(window).unwrap();

    // Only `system` says who drives; a space grant never; no such space is not_found.
    assert_eq!(code(agent.control_space(space.id.clone(), SpaceControl::Paused, None)), ErrorCode::Forbidden);
    assert_eq!(code(root.control_space(ede_protocol::SpaceId::new("space-9").unwrap(), SpaceControl::Paused, None)), ErrorCode::NotFound);
}

/// Taken over: over the view's picture the person's pointer drives the
/// space's (the shell's layer hears nothing), the letterbox does nothing,
/// a press in the picture gives the space their keys, a press elsewhere
/// takes them back; handing back and the close let go of what is down.
#[test]
fn the_person_drives_a_taken_over_space_through_the_view() {
    let (h, _dir) = start_with_spaces(&[(1280, 800)]);
    let mut root = h.ipc();
    // The person's own app, with their keyboard, at the top left.
    let mut a = Session::connect(h.connect());
    a.keyboard();
    a.pointer();
    let mine = a.toplevel("mine");
    a.draw(&mine, 300, 200, BLUE);
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let mine_at = h.with(|ede| ede.desk.space.elements().next().and_then(|w| ede.desk.space.element_location(w)).map(|p| (p.x as f64, p.y as f64)).unwrap());

    let mut agent = space_grant(&h, "run-1");
    let space = agent.create_space(640, 480).unwrap();
    let (mut b, _window) = space_app(&space.wayland_display);

    // The shell's view: 400x300 at the bottom right (x 880..1280, y 500..800).
    let mut shell = Session::connect(h.connect());
    shell.pointer();
    let pid = std::process::id();
    h.with(move |ede| ede.shell_stand_in = Some(pid));
    let layer = shell.layer(SPACE_VIEW_NAMESPACE, Anchor::Bottom | Anchor::Right, (400, 300), 0);
    h.with(|ede| ede.shell_stand_in = None);
    shell.draw_layer(&layer, GREEN);
    shell.roundtrip();

    // Before the take-over the view is the shell's like any layer.
    h.with(|ede| ede.pointer_motion_to((1000.0, 600.0).into(), 0));
    wait_for(&mut shell, |s| s.state.pointer_enters == 1);
    h.with(|ede| ede.pointer_motion_to((100.0, 100.0).into(), 1));
    shell.roundtrip();

    // The picture in the top 400x200 of the layer: 640x480 at scale 5/12 is
    // 266.67x200, centred: x 946.67..1213.33, y 500..700.
    let view = SpaceViewRect { namespace: SPACE_VIEW_NAMESPACE.into(), x: 0, y: 0, width: 400, height: 200 };
    root.control_space(space.id.clone(), SpaceControl::Person, Some(view)).unwrap();
    assert_eq!(root.spaces().unwrap()[0].control, SpaceControl::Person);
    let space_pointer = |h: &Harness| h.with(|ede| ede.spaces.iter().next().map(|s| s.desk.seat.get_pointer().unwrap().current_location()).map(|p| (p.x.round(), p.y.round())).unwrap());
    let shell_enters = shell.state.pointer_enters;

    // The letterbox (inside the rect, beside the picture): nothing anywhere.
    h.with(|ede| {
        ede.person_pointer_motion_to((900.0, 600.0).into(), 2);
    });
    assert_eq!(space_pointer(&h), (0.0, 0.0), "the letterbox moves no space pointer");
    // The picture's centre is the space's (320, 240).
    h.with(|ede| {
        ede.person_pointer_motion_to((1080.0, 600.0).into(), 3);
    });
    assert_eq!(space_pointer(&h), (320.0, 240.0));
    wait_for(&mut b, |b| b.state.pointer_at.is_some());
    let person_at = h.with(|ede| ede.desk.seat.get_pointer().unwrap().current_location());
    assert_eq!((person_at.x, person_at.y), (1080.0, 600.0), "the person's own pointer (and cursor) moves along");
    shell.roundtrip();
    assert_eq!(shell.state.pointer_enters, shell_enters, "the shell's layer hears nothing of it");

    // A press in the picture: the space's button, and the space is keyed.
    h.with(|ede| {
        ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 4);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 5);
    });
    wait_for(&mut b, |b| b.state.buttons == [(BTN_LEFT, true), (BTN_LEFT, false)]);
    shell.roundtrip();
    assert!(shell.state.buttons.is_empty(), "the shell's layer got no button");
    let a_keys = a.state.keys.len();
    person_key(&h, KEY_A);
    wait_for(&mut b, |b| b.state.keys.contains(&KEY_A) && b.state.key_releases.contains(&KEY_A));
    a.roundtrip();
    assert_eq!(a.state.keys.len(), a_keys, "a keyed space's key never reaches the person's app");

    // The wheel over the picture scrolls the space.
    h.with(|ede| {
        let frame = smithay::input::pointer::AxisFrame::new(smithay::backend::input::InputTime::from_millis(6)).value(smithay::backend::input::Axis::Vertical, 15.0);
        assert!(ede.space_view_axis(frame));
    });
    wait_for(&mut b, |b| !b.state.axes.is_empty());

    // A key held down in the space is let go when the person hands it back.
    h.with(|ede| ede.keyboard_key((KEY_B + 8).into(), smithay::backend::input::KeyState::Pressed, 7));
    wait_for(&mut b, |b| b.state.keys.contains(&KEY_B));
    root.control_space(space.id.clone(), SpaceControl::Agent, None).unwrap();
    wait_for(&mut b, |b| b.state.key_releases.contains(&KEY_B));
    h.with(|ede| ede.keyboard_key((KEY_B + 8).into(), smithay::backend::input::KeyState::Released, 8));
    // Handed back, the view is the shell's again and the keys the person's.
    let b_keys = b.state.keys.len();
    h.with(|ede| {
        ede.person_pointer_motion_to((1081.0, 601.0).into(), 9);
    });
    wait_for(&mut shell, |s| s.state.pointer_enters > shell_enters);
    person_key(&h, KEY_A);
    wait_for(&mut a, |a| a.state.keys.len() > a_keys);
    b.roundtrip();
    assert_eq!(b.state.keys.len(), b_keys);

    // Taken over again and keyed; a press on the person's own app takes the keys back.
    let view = SpaceViewRect { namespace: SPACE_VIEW_NAMESPACE.into(), x: 0, y: 0, width: 400, height: 200 };
    root.control_space(space.id.clone(), SpaceControl::Person, Some(view.clone())).unwrap();
    h.with(|ede| {
        ede.person_pointer_motion_to((1080.0, 600.0).into(), 10);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 11);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 12);
    });
    person_key(&h, KEY_B);
    wait_for(&mut b, |b| b.state.keys.len() > b_keys);
    let (a_keys, b_keys) = (a.state.keys.len(), b.state.keys.len());
    h.with(move |ede| {
        ede.person_pointer_motion_to((mine_at.0 + 50.0, mine_at.1 + 50.0).into(), 13);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 14);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 15);
    });
    person_key(&h, KEY_B);
    wait_for(&mut a, |a| a.state.keys.len() > a_keys);
    b.roundtrip();
    assert_eq!(b.state.keys.len(), b_keys, "a press elsewhere ends the keys' way into the space");

    // Keyed again, then the view's layer goes: the keys are the person's.
    h.with(|ede| {
        ede.person_pointer_motion_to((1080.0, 600.0).into(), 16);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 17);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 18);
    });
    assert!(h.with(|ede| ede.space_keyed().is_some()));
    drop(layer);
    drop(shell);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.with(|ede| ede.space_keyed().is_some()) {
        assert!(std::time::Instant::now() < deadline, "the layer went and the space stayed keyed");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Closing a taken-over space leaves nothing behind.
    root.destroy_space(space.id.clone()).unwrap();
    let a_keys = a.state.keys.len();
    person_key(&h, KEY_A);
    wait_for(&mut a, |a| a.state.keys.len() > a_keys);
}
