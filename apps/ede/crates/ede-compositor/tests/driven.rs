//! #EDE-73 headless: a key or button the agent path pressed never outlives
//! its connection, its release is never refused, and the person's seat and
//! the agent's keys stay apart during a hold.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{Error, ErrorCode, InputEvent, Request};
use smithay::backend::input::{ButtonState, KeyState};

const BLUE: u32 = 0xff00_00ff;
const RED: u32 = 0xffff_0000;
// evdev codes; a smithay Keycode is the code + 8.
const KEY_TAB: u32 = 15;
const KEY_LEFTCTRL: u32 = 29;
const KEY_A: u32 = 30;
const KEY_LEFTSHIFT: u32 = 42;
const BTN_LEFT: u32 = 0x110;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

/// A focused window, and a point inside it.
fn notes(h: &Harness) -> (Session, (f64, f64)) {
    let mut app = Session::connect(h.connect());
    app.keyboard();
    app.pointer();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.keyboard_focus.is_none() {
        assert!(Instant::now() < deadline, "the window never took focus");
        std::thread::sleep(Duration::from_millis(10));
        app.roundtrip();
    }
    let w = h.ipc().windows().unwrap().remove(0);
    (app, ((w.x + 10) as f64, (w.y + 10) as f64))
}

/// The keys down on the seat (evdev codes, sorted), and whether a button
/// is (the pointer's click grab lasts as long as one is down).
fn down(h: &Harness) -> (Vec<u32>, bool) {
    h.with(|ede| {
        let mut keys: Vec<u32> = ede.seat.get_keyboard().unwrap().pressed_keys().iter().map(|k| k.raw() - 8).collect();
        keys.sort();
        (keys, ede.seat.get_pointer().unwrap().is_grabbed())
    })
}

fn key(key: u32, pressed: bool) -> InputEvent {
    InputEvent::Key { key, pressed }
}

fn person_key(h: &Harness, code: u32, state: KeyState) {
    h.with(move |ede| ede.keyboard_key((code + 8).into(), state, 0));
}

fn forbidden(r: Result<(), Error>) -> bool {
    matches!(r, Err(Error::Failed(f)) if f.code == ErrorCode::Forbidden)
}

/// A connection that closes (a host that died, a call that failed midway)
/// lets go of what it pressed, the Shift of a keysym included; another
/// connection's key stays down.
#[test]
fn what_a_connection_pressed_is_released_when_it_closes() {
    let h = harness();
    let (_app, (x, y)) = notes(&h);
    let mut other = h.ipc();
    other.input(key(KEY_TAB, true)).unwrap();

    let mut agent = h.ipc();
    agent.input(key(KEY_LEFTCTRL, true)).unwrap();
    agent.input(InputEvent::Keysym { keysym: 0x41, pressed: true }).unwrap();
    agent.input(InputEvent::PointerMotion { x, y }).unwrap();
    agent.input(InputEvent::PointerButton { button: BTN_LEFT, pressed: true }).unwrap();
    assert_eq!(down(&h), (vec![KEY_TAB, KEY_LEFTCTRL, KEY_A, KEY_LEFTSHIFT], true));

    drop(agent);
    let deadline = Instant::now() + Duration::from_secs(5);
    while down(&h) != (vec![KEY_TAB], false) {
        assert!(Instant::now() < deadline, "still down after the connection closed: {:?}", down(&h));
        std::thread::sleep(Duration::from_millis(10));
    }
    // The dropped `A` took its Shift bookkeeping along: the next one holds
    // and releases its own.
    other.input(InputEvent::Keysym { keysym: 0x41, pressed: true }).unwrap();
    other.input(InputEvent::Keysym { keysym: 0x41, pressed: false }).unwrap();
    assert_eq!(down(&h).0, [KEY_TAB]);
}

/// A press that was allowed is released wherever the focus went: a prompt
/// that opened over it refuses new presses, never the release.
#[test]
fn a_release_is_never_refused() {
    let h = harness();
    let (mut app, (x, y)) = notes(&h);
    let mut ipc = h.ipc();
    ipc.input(key(KEY_A, true)).unwrap();
    ipc.input(InputEvent::PointerMotion { x, y }).unwrap();
    ipc.input(InputEvent::PointerButton { button: BTN_LEFT, pressed: true }).unwrap();

    let secret = app.toplevel("prompt");
    secret.toplevel.set_app_id("gcr-prompter".into());
    app.draw(&secret, 300, 200, RED);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.keyboard_focus.as_ref() != Some(&secret.surface) {
        assert!(Instant::now() < deadline, "the prompt never took focus");
        std::thread::sleep(Duration::from_millis(10));
        app.roundtrip();
    }
    let vault = ipc.windows().unwrap().into_iter().find(|w| w.protected).unwrap();
    ipc.call(Request::MoveWindow { window: vault.id, x: x as i32 - 10, y: y as i32 - 10 }).unwrap();

    assert!(forbidden(ipc.input(key(KEY_TAB, true))), "presses stay refused");
    assert!(forbidden(ipc.input(key(KEY_TAB, false))), "and so does the release of a key nobody drove");
    assert!(forbidden(ipc.input(InputEvent::PointerButton { button: BTN_LEFT + 1, pressed: true })));
    ipc.input(key(KEY_A, false)).unwrap();
    ipc.input(InputEvent::PointerButton { button: BTN_LEFT, pressed: false }).unwrap();
    assert_eq!(down(&h), (vec![], false));
}

/// During a hold the person's keys reach no window, and their release of a
/// key the agent has down does not release it.
#[test]
fn the_persons_release_leaves_the_agents_key_down() {
    let h = harness();
    let (_app, _) = notes(&h);
    let mut agent = h.ipc();
    agent.hold_input().unwrap();
    agent.input(key(KEY_A, true)).unwrap();
    person_key(&h, KEY_A, KeyState::Pressed);
    person_key(&h, KEY_A, KeyState::Released);
    assert_eq!(down(&h).0, [KEY_A], "the agent's key was released by the person");
    agent.input(key(KEY_A, false)).unwrap();
    assert_eq!(down(&h).0, Vec::<u32>::new());
}

/// A hold starts on a clean seat: the modifier the person holds and their
/// drag in flight end with it, and letting go later touches nothing.
#[test]
fn a_hold_starts_on_a_clean_seat() {
    let h = harness();
    let (_app, (x, y)) = notes(&h);
    person_key(&h, KEY_LEFTSHIFT, KeyState::Pressed);
    h.with(move |ede| {
        ede.person_pointer_motion_to((x, y).into(), 0);
        ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 0);
    });
    assert_eq!(down(&h), (vec![KEY_LEFTSHIFT], true));

    let mut agent = h.ipc();
    agent.hold_input().unwrap();
    assert_eq!(down(&h), (vec![], false), "the person's Shift and button merged into the hold");

    agent.input(key(KEY_LEFTSHIFT, true)).unwrap();
    person_key(&h, KEY_LEFTSHIFT, KeyState::Released);
    h.with(|ede| ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 0));
    assert_eq!(down(&h), (vec![KEY_LEFTSHIFT], false));
}
