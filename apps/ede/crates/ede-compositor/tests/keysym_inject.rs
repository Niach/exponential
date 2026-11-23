//! #EDE-85: a character no configured layout makes (`é`, `😀` on a US
//! layout) reaches the focused client through a throwaway keymap sent to
//! it alone, on the person's screen and in a space; the person's seat keeps
//! its keymap, and the person's other clients hear no keymap at all.

mod common;

use std::os::unix::net::UnixStream;

use common::{
    space::{start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId, Config};
use ede_protocol::{Capability, Error, ErrorCode, InputEvent, SessionId};

const BLUE: u32 = 0xff00_00ff;
const RED: u32 = 0xffff_0000;
/// `eacute` (Latin-1) and U+1F600 as ePilot names them.
const E_ACUTE: u32 = 0xe9;
const GRINNING: u32 = 0x0100_0000 | 0x1f600;
/// `a`, which the US layout makes on a key.
const A: u32 = 0x61;

fn us() -> Config {
    let mut config = Config::default();
    config.keyboard.layout = "us".into();
    config
}

/// The keymap text a desk's seat holds.
fn seat_keymap(h: &Harness, desk: DeskId) -> String {
    h.with(move |ede| {
        let keyboard = ede.desk(desk).seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(ede, |ctx| ctx.xkb().lock().unwrap().keymap_as_string())
    })
}

fn type_keysyms(agent: &mut ede_protocol::Client, keysyms: &[u32]) {
    for keysym in keysyms {
        agent.input(InputEvent::Keysym { keysym: *keysym, pressed: true }).unwrap();
        agent.input(InputEvent::Keysym { keysym: *keysym, pressed: false }).unwrap();
    }
}

#[test]
fn a_character_no_layout_makes_is_typed_on_the_persons_screen() {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(us(), &[(800, 600)]);
    // B: another of the person's clients, not focused.
    let mut b = Session::connect(h.connect());
    b.keyboard();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some() && c.state.keymap_text.is_some());
    b.roundtrip();
    let (keymap, b_keymaps) = (seat_keymap(&h, DeskId::PERSON), b.state.keymaps);

    let mut agent = h.ipc();
    type_keysyms(&mut agent, &[A, E_ACUTE, GRINNING, A]);
    wait_for(&mut app, |c| c.state.typed.len() == 4);
    assert_eq!(app.state.typed, [A, E_ACUTE, GRINNING, A], "each under the keymap the client held when it came");
    assert_eq!(app.state.keymap_text.as_deref(), Some(keymap.as_str()), "the client is back on the seat's keymap");

    assert_eq!(seat_keymap(&h, DeskId::PERSON), keymap, "the person's seat keeps its keymap");
    assert!(h.with(|ede| ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()), "nothing is left down");
    b.roundtrip();
    assert_eq!(b.state.keymaps, b_keymaps, "the person's other clients hear no keymap");

    // A keysym xkb has no character for is still not_found.
    let unknown = agent.input(InputEvent::Keysym { keysym: 0x00ff_fff0, pressed: true }).unwrap_err();
    assert!(matches!(unknown, Error::Failed(f) if f.code == ErrorCode::NotFound));
}

#[test]
fn a_character_no_layout_makes_is_typed_in_a_space() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    // A: the person's window, focused.
    let mut a = Session::connect(h.connect());
    a.keyboard();
    let window_a = a.toplevel("a");
    a.draw(&window_a, 300, 200, BLUE);
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let (person_keymap, a_keymaps) = (seat_keymap(&h, DeskId::PERSON), a.state.keymaps);

    let mut root = h.ipc();
    let grant = root.mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token.clone()).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let mut s = Session::connect(UnixStream::connect(&space.wayland_display).expect("the space's socket"));
    let window_s = s.toplevel("s");
    s.draw(&window_s, 640, 480, RED);
    s.keyboard();
    wait_for(&mut s, |s| s.state.keyboard_focus.is_some() && s.state.keymap_text.is_some());
    let id = space.id.clone();
    let desk = h.with(move |ede| ede.spaces.iter().find(|s| s.id == id).unwrap().desk.id);
    let space_keymap = seat_keymap(&h, desk);

    type_keysyms(&mut agent, &[E_ACUTE, GRINNING]);
    wait_for(&mut s, |c| c.state.typed.len() == 2);
    assert_eq!(s.state.typed, [E_ACUTE, GRINNING]);
    assert_eq!(s.state.keymap_text.as_deref(), Some(space_keymap.as_str()), "the space's client is back on its seat's keymap");
    assert_eq!(seat_keymap(&h, desk), space_keymap, "the space's seat keeps its keymap");

    a.roundtrip();
    assert_eq!(seat_keymap(&h, DeskId::PERSON), person_keymap, "the person's seat keeps its keymap");
    assert_eq!(a.state.keymaps, a_keymaps, "the person's window hears no keymap");
    assert!(a.state.typed.is_empty(), "nor a key");
}
