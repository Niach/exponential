//! #EDE-126 headless: nothing a space's client asks moves the person's
//! desk. The handlers that have no guard are inert for a space's surface
//! only while they read the person's desk; this pins it, with the person's
//! output at scale 2 and the person typing. The bindings (the person's
//! own input) never act on a space's desk (#EDE-136).

mod common;

use std::os::unix::net::UnixStream;

use common::{
    space::{assert_person_untouched, events_until_mark, person_key, person_snapshot, start_with_spaces, wait_for},
    Session,
};
use ede_compositor::desk::DeskId;
use ede_protocol::{ErrorCode, InputEvent, Request, SessionId};
use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;

#[test]
fn a_space_client_cannot_move_the_person() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut root = h.ipc();
    root.set_output("HEADLESS-1", None, Some(2.0), None).unwrap();
    let mut a = Session::connect(h.connect());
    let person = a.toplevel("a");
    a.draw(&person, 200, 150, BLUE);
    a.keyboard();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned();
        state.focus(DeskId::PERSON, window);
    });
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    assert_eq!(a.preferred_scale(&person.surface), Some(240), "the person's windows are at their output's scale");

    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    b.pointer();
    let window = b.toplevel("b");
    b.draw(&window, 640, 480, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    root.subscribe().unwrap();
    let before = person_snapshot(&h);
    let inhibited = h.with(|state| (state.idle_inhibited(), state.idle.inhibitors.len()));

    // Its scale is its own output's, 1, never the person's first output's.
    assert_eq!(b.preferred_scale(&window.surface), Some(120));
    // Its popup is configured and kept inside its 640x480 output (asked for
    // past the corner).
    let popup = b.window_popup(&window, (600, 440), (200, 100), false, GREEN);
    let (x, y, w, height) = *b.state.popup_configures.last().expect("a space's popup is configured");
    assert!(x >= 0 && y >= 0 && x + w <= 640 && y + height <= 480, "the popup leaves the space's output: {x},{y} {w}x{height}");
    b.roundtrip();
    // Its popup is tracked as the space's: the person's desk holds no popup of it.
    assert!(h.with(|state| state.desk.space.elements().count()) == 1);
    // Its idle inhibitor is not the person's: their screen still locks on idle.
    let _inhibitor = b.inhibit_idle(&window.surface);
    assert_eq!(h.with(|state| (state.idle_inhibited(), state.idle.inhibitors.len())), inhibited);

    // Every xdg-shell request that has no guard, while the person types.
    let seat = b.state.seat.clone().unwrap();
    window.toplevel.set_title("renamed".into());
    window.toplevel.set_app_id("org.example.B".into());
    window.toplevel.set_minimized();
    window.toplevel.unset_maximized();
    window.toplevel.set_maximized();
    window.toplevel.set_fullscreen(None);
    window.toplevel.unset_fullscreen();
    window.toplevel._move(&seat, 0);
    window.toplevel.resize(&seat, 0, ResizeEdge::BottomRight);
    window.toplevel.show_window_menu(&seat, 0, 10, 10);
    person_key(&h, KEY_A);
    let grabbing = b.window_popup(&window, (10, 10), (50, 50), true, GREEN);
    grabbing.popup.destroy();
    popup.popup.destroy();
    window.surface.commit();
    b.roundtrip();
    person_key(&h, KEY_A);

    wait_for(&mut a, |a| a.state.keys == [KEY_A, KEY_A]);
    assert!(b.state.keys.is_empty(), "the person's keys reached the space");
    assert_person_untouched(&h, &before);
    assert_eq!(events_until_mark(&h, &mut root, "test.mark"), Vec::<String>::new(), "the shell heard of a space");
    assert_eq!(h.with(|state| (state.idle_inhibited(), state.idle.inhibitors.len())), inhibited);
    // The space's window is where it was: mapped, filling its output.
    assert_eq!(h.with(move |state| state.desk(desk).space.elements().map(|w| w.geometry().size).map(|s| (s.w, s.h)).collect::<Vec<_>>()), [(640, 480)]);
    assert!(!b.state.toplevels[window.index].closed);
}

/// #EDE-136: the bindings arms drive the person's own keyboard and pointer,
/// so the bindings are the person's desk alone. The shell's grant on a
/// space's desk (no connection resolves there today: a space connection's
/// bindings stop at its scope) is refused for every event, and neither the
/// person's focus nor the space's hears a key; the person's own bindings
/// still type.
#[test]
fn the_bindings_drive_the_person_desk_alone() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut a = Session::connect(h.connect());
    let person = a.toplevel("a");
    a.draw(&person, 200, 150, BLUE);
    a.keyboard();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned();
        state.focus(DeskId::PERSON, window);
    });
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    b.pointer();
    let window = b.toplevel("b");
    b.draw(&window, 640, 480, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    let before = person_snapshot(&h);
    let space_pointer = || h.with(move |state| state.desk(desk).seat.get_pointer().unwrap().current_location());
    let pointer_before = space_pointer();

    const KEYSYM_A: u32 = 0x61;
    let events = [
        InputEvent::Key { key: KEY_A, pressed: true },
        InputEvent::Key { key: KEY_A, pressed: false },
        InputEvent::Keysym { keysym: KEYSYM_A, pressed: true },
        InputEvent::Keysym { keysym: KEYSYM_A, pressed: false },
        InputEvent::PointerMotionRelative { dx: 40.0, dy: 30.0 },
        InputEvent::PointerMotion { x: 100.0, y: 100.0 },
        InputEvent::PointerButton { button: 0x110, pressed: true },
        InputEvent::PointerButton { button: 0x110, pressed: false },
    ];
    for event in events {
        let shown = format!("{event:?}");
        let refused = h.with(move |state| {
            let grant = state.ipc.root_grant();
            state.ipc_input(u64::MAX, desk, &grant, event, true, 0)
        });
        assert_eq!(refused.map_err(|f| f.code), Err(ErrorCode::Forbidden), "the shell's bindings drove a space's desk: {shown}");
    }
    a.roundtrip();
    b.roundtrip();
    assert!(a.state.keys.is_empty(), "the person's focus heard a key: {:?}", a.state.keys);
    assert!(b.state.keys.is_empty(), "the space's focus heard a key: {:?}", b.state.keys);
    assert_person_untouched(&h, &before);
    assert_eq!(space_pointer(), pointer_before, "the space's pointer moved");
    assert!(h.with(|state| state.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()), "a key stays down on the person's keyboard");

    // The person's own bindings, on their desk, still type.
    let mut root = h.ipc();
    for event in [
        InputEvent::Key { key: KEY_A, pressed: true },
        InputEvent::Key { key: KEY_A, pressed: false },
        InputEvent::Keysym { keysym: KEYSYM_A, pressed: true },
        InputEvent::Keysym { keysym: KEYSYM_A, pressed: false },
    ] {
        root.call(Request::Input { event, bindings: true }).expect("the person's bindings");
    }
    wait_for(&mut a, |a| a.state.keys == [KEY_A, KEY_A]);
    b.roundtrip();
    assert!(b.state.keys.is_empty(), "the person's keys reached the space");
}
