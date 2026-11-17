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
const KEY_LEFTALT: u32 = 56;
const KEY_LEFTMETA: u32 = 125;
const KEY_ESC: u32 = 1;
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
        let mut keys: Vec<u32> = ede.desk.seat.get_keyboard().unwrap().pressed_keys().iter().map(|k| k.raw() - 8).collect();
        keys.sort();
        (keys, ede.desk.seat.get_pointer().unwrap().is_grabbed())
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

/// The last hold's end (the person's Esc, `release_input`, the holder
/// going) hands the seat back clean (#EDE-80): what any agent connection
/// still has down is released, not left for its connection to close.
#[test]
fn the_holds_end_releases_what_agents_have_down() {
    let h = harness();
    let (_app, (x, y)) = notes(&h);
    let mut other = h.ipc();
    let mut agent = h.ipc();
    agent.hold_input().unwrap();
    agent.input(key(KEY_LEFTCTRL, true)).unwrap();
    agent.input(InputEvent::PointerMotion { x, y }).unwrap();
    agent.input(InputEvent::PointerButton { button: BTN_LEFT, pressed: true }).unwrap();
    other.input(key(KEY_TAB, true)).unwrap();
    assert_eq!(down(&h), (vec![KEY_TAB, KEY_LEFTCTRL], true));
    person_key(&h, KEY_ESC, KeyState::Pressed);
    assert_eq!(down(&h), (vec![], false), "the agent's keys outlived the person's Esc");
    person_key(&h, KEY_ESC, KeyState::Released);

    agent.hold_input().unwrap();
    agent.input(key(KEY_LEFTCTRL, true)).unwrap();
    agent.release_input().unwrap();
    assert_eq!(down(&h).0, Vec::<u32>::new(), "the agent's key outlived release_input");
    // Letting go without holding ends nothing.
    other.input(key(KEY_TAB, true)).unwrap();
    agent.release_input().unwrap();
    assert_eq!(down(&h).0, [KEY_TAB]);

    agent.hold_input().unwrap();
    drop(agent);
    let deadline = Instant::now() + Duration::from_secs(5);
    while down(&h).0 != Vec::<u32>::new() {
        assert!(Instant::now() < deadline, "still down after the holder went: {:?}", down(&h));
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The `hotkey` events the shell heard, until none comes for a moment.
fn hotkeys(shell: &mut ede_protocol::Client) -> Vec<String> {
    let mut names = Vec::new();
    while let Ok(e) = shell.next_event() {
        if e.kind == ede_protocol::events::HOTKEY {
            names.push(e.data["name"].as_str().unwrap().to_string());
        }
    }
    names
}

/// The release a hold's start makes is not the person's act (#EDE-80): a
/// Super held alone opens no menu, and an open Alt-Tab switcher is
/// cancelled, never committed.
#[test]
fn a_holds_start_fires_none_of_the_persons_bindings() {
    let h = harness();
    let (_app, _) = notes(&h);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut agent = h.ipc();

    person_key(&h, KEY_LEFTMETA, KeyState::Pressed);
    agent.hold_input().unwrap();
    assert_eq!(down(&h).0, Vec::<u32>::new());
    assert_eq!(hotkeys(&mut shell), Vec::<String>::new(), "the hold's start opened the menu");
    agent.release_input().unwrap();
    // Their own Super afterwards is theirs again.
    person_key(&h, KEY_LEFTMETA, KeyState::Released);
    person_key(&h, KEY_LEFTMETA, KeyState::Pressed);
    person_key(&h, KEY_LEFTMETA, KeyState::Released);
    assert_eq!(hotkeys(&mut shell), ["menu.toggle"]);

    person_key(&h, KEY_LEFTALT, KeyState::Pressed);
    person_key(&h, KEY_TAB, KeyState::Pressed);
    person_key(&h, KEY_TAB, KeyState::Released);
    agent.hold_input().unwrap();
    assert_eq!(down(&h).0, Vec::<u32>::new());
    assert_eq!(hotkeys(&mut shell), ["switcher.next", "switcher.cancel"], "the hold's start committed the switcher");
    agent.release_input().unwrap();
    // The switcher is closed: their late Alt release commits nothing.
    person_key(&h, KEY_LEFTALT, KeyState::Released);
    assert_eq!(hotkeys(&mut shell), Vec::<String>::new());
}

const KEY_V: u32 = 47;
const KEY_Z: u32 = 44;
const KEY_CAPSLOCK: u32 = 58;

fn layouts(layout: &str) -> Harness {
    let keyboard = ede_compositor::config::Keyboard { layout: layout.into(), ..Default::default() };
    Harness::start(Config { keyboard, ..Config::default() }, &[(800, 600)])
}

fn active_layout(h: &Harness) -> u32 {
    h.with(|ede| {
        let keyboard = ede.desk.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(ede, |ctx| ctx.xkb().lock().unwrap().active_layout().0)
    })
}

/// The key a keysym resolves to: (evdev code, modifiers' evdev codes, the
/// other layout it is in).
fn key_for(h: &Harness, keysym: u32) -> Option<(u32, Vec<u32>, Option<u32>)> {
    h.with(move |ede| ede.key_for_keysym(keysym)).map(|(key, mods, layout)| (key.raw() - 8, mods.iter().map(|m| m.raw() - 8).collect(), layout.map(|l| l.0)))
}

/// #EXP-1907: with the Russian layout active no key of it makes `v`: the
/// US layout's does, in the US layout for the length of the stroke, so
/// ctrl+v pastes and Latin text is typed as Latin.
#[test]
fn a_keysym_of_another_layout_is_pressed_in_that_layout() {
    let h = layouts("us,ru");
    h.with(|ede| ede.swap_layout(smithay::input::keyboard::Layout(1)));
    assert_eq!(key_for(&h, 0x6cd), Some((KEY_V, vec![], None)), "Cyrillic em is the active layout's");
    assert_eq!(key_for(&h, 0x76), Some((KEY_V, vec![], Some(0))));
    assert_eq!(key_for(&h, 0x56), Some((KEY_V, vec![KEY_LEFTSHIFT], Some(0))));
    assert_eq!(key_for(&h, 0x0100_2603), None, "no layout has a snowman");

    let mut ipc = h.ipc();
    ipc.input(InputEvent::Keysym { keysym: 0x76, pressed: true }).unwrap();
    assert_eq!((down(&h).0, active_layout(&h)), (vec![KEY_V], 0));
    ipc.input(InputEvent::Keysym { keysym: 0x76, pressed: false }).unwrap();
    assert_eq!((down(&h).0, active_layout(&h)), (vec![], 1), "the person's layout is back");

    // A connection that closes mid-stroke gives the layout back too.
    ipc.input(InputEvent::Keysym { keysym: 0x76, pressed: true }).unwrap();
    assert_eq!(active_layout(&h), 0);
    drop(ipc);
    let deadline = Instant::now() + Duration::from_secs(5);
    while (down(&h).0, active_layout(&h)) != (vec![], 1) {
        assert!(Instant::now() < deadline, "the layout stayed switched after the connection closed");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// #EXP-1907: under Caps Lock a typed `a` needs Shift and `A` none; under
/// a held Ctrl the key is a shortcut's and is pressed as the person would.
#[test]
fn a_keysym_is_resolved_under_the_seats_caps_lock() {
    let h = layouts("us");
    assert_eq!(key_for(&h, 0x61), Some((KEY_A, vec![], None)));
    person_key(&h, KEY_CAPSLOCK, KeyState::Pressed);
    person_key(&h, KEY_CAPSLOCK, KeyState::Released);
    assert_eq!(key_for(&h, 0x61), Some((KEY_A, vec![KEY_LEFTSHIFT], None)));
    assert_eq!(key_for(&h, 0x41), Some((KEY_A, vec![], None)));
    person_key(&h, KEY_LEFTCTRL, KeyState::Pressed);
    assert_eq!(key_for(&h, 0x7a), Some((KEY_Z, vec![], None)));
    person_key(&h, KEY_LEFTCTRL, KeyState::Released);
}
