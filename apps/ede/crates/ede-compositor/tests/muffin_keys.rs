//! #EDE-243 headless: Cinnamon's default window keys and Alt+right-drag.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::events;
use smithay::backend::input::{ButtonState, KeyState};

const BLUE: u32 = 0xff00_00ff;
const KEY_LEFTMETA: u32 = 125;
const KEY_LEFTALT: u32 = 56;
const KEY_LEFTSHIFT: u32 = 42;
const KEY_UP: u32 = 103;
const KEY_DOWN: u32 = 108;
const KEY_LEFT: u32 = 105;
const KEY_RIGHT: u32 = 106;
const KEY_D: u32 = 32;
const KEY_SPACE: u32 = 57;
const KEY_F10: u32 = 68;
const KEY_F7: u32 = 65;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

fn harness(config: Config) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(config, &[(800, 600)])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn key(h: &Harness, code: u32, pressed: bool) {
    h.with(move |ede| ede.keyboard_key((code + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, 0));
}

/// `mods` held, `code` tapped.
fn chord(h: &Harness, mods: &[u32], code: u32) {
    for m in mods {
        key(h, *m, true);
    }
    key(h, code, true);
    key(h, code, false);
    for m in mods.iter().rev() {
        key(h, *m, false);
    }
}

fn geometry(h: &Harness) -> (i32, i32, i32, i32, bool) {
    h.with(|ede| {
        let w = ede.desk.space.elements().last().cloned();
        match w {
            Some(w) => {
                let g = ede.desk.space.element_geometry(&w).unwrap();
                (g.loc.x, g.loc.y, g.size.w, g.size.h, false)
            }
            None => (0, 0, 0, 0, true),
        }
    })
}

/// Super+arrows walk Muffin's tiles; Alt+F10 toggles maximize; Super+D
/// shows the desktop and brings it back.
#[test]
fn super_arrows_tile_like_muffin() {
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let area = h.with(|ede| {
        let o = ede.desk.space.outputs().next().unwrap().clone();
        let a = ede.work_area(&o).unwrap();
        (a.loc.x, a.loc.y, a.size.w, a.size.h)
    });
    let tiled = |h: &Harness| h.with(|ede| ede.desk.space.elements().last().map(|w| ede_compositor::state::window_state(w).tile)).flatten();

    chord(&h, &[KEY_LEFTMETA], KEY_UP);
    assert_eq!(tiled(&h), Some(ede_compositor::layout::Tile::Top), "Super+Up: the top half");
    chord(&h, &[KEY_LEFTMETA], KEY_LEFT);
    assert_eq!(tiled(&h), Some(ede_compositor::layout::Tile::TopLeft), "then Left: the quarter");
    chord(&h, &[KEY_LEFTMETA], KEY_DOWN);
    assert_eq!(tiled(&h), Some(ede_compositor::layout::Tile::Left));
    chord(&h, &[KEY_LEFTMETA], KEY_RIGHT);
    assert_eq!(tiled(&h), None, "the opposite side restores");
    chord(&h, &[KEY_LEFTMETA], KEY_UP);
    chord(&h, &[KEY_LEFTMETA], KEY_UP);
    assert_eq!(tiled(&h), Some(ede_compositor::layout::Tile::Maximized), "Up twice maximizes");
    let _ = area;

    chord(&h, &[KEY_LEFTALT], KEY_F10);
    assert_eq!(tiled(&h), None, "Alt+F10 unmaximizes");
    chord(&h, &[KEY_LEFTALT], KEY_F10);
    assert_eq!(tiled(&h), Some(ede_compositor::layout::Tile::Maximized));
    chord(&h, &[KEY_LEFTALT], KEY_F10);

    chord(&h, &[KEY_LEFTMETA], KEY_D);
    assert!(geometry(&h).4, "Super+D: no window on screen");
    chord(&h, &[KEY_LEFTMETA], KEY_D);
    assert!(!geometry(&h).4, "and back");
}

/// Alt+right-drag resizes from the nearest corner; Alt+F7 moves with the
/// pointer until a click.
#[test]
fn alt_right_drag_resizes_and_alt_f7_moves() {
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    app.pointer();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let (x, y, w, hh, _) = geometry(&h);
    // Near the bottom-right corner, Alt held, right button dragged out by 40×30.
    let start = ((x + w - 10) as f64, (y + hh - 10) as f64);
    h.with(move |ede| {
        ede.person_pointer_motion_to(start.into(), 0);
    });
    key(&h, KEY_LEFTALT, true);
    h.with(|ede| ede.person_pointer_button(BTN_RIGHT, ButtonState::Pressed, 0));
    h.with(move |ede| {
        ede.person_pointer_motion_to((start.0 + 40.0, start.1 + 30.0).into(), 1);
    });
    let asked = |c: &mut Session| c.state.toplevels.first().map(|s| s.size);
    wait_for(&mut app, |c| asked(c) == Some((w + 40, hh + 30)));
    h.with(|ede| ede.person_pointer_button(BTN_RIGHT, ButtonState::Released, 2));
    key(&h, KEY_LEFTALT, false);
    assert!(!h.with(|ede| ede.desk.seat.get_pointer().unwrap().is_grabbed()), "the release ends it");

    // Alt+F7: the pointer jumps to the centre, the window follows, a click ends it.
    let (x0, y0, w0, h0, _) = geometry(&h);
    chord(&h, &[KEY_LEFTALT], KEY_F7);
    let at = h.with(|ede| ede.desk.seat.get_pointer().unwrap().current_location());
    assert_eq!((at.x as i32, at.y as i32), (x0 + w0 / 2, y0 + h0 / 2));
    h.with(move |ede| {
        ede.person_pointer_motion_to((at.x + 50.0, at.y + 20.0).into(), 3);
    });
    let (x1, y1, ..) = geometry(&h);
    assert_eq!((x1, y1), (x0 + 50, y0 + 20), "it follows");
    h.with(|ede| ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 4));
    h.with(|ede| ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 5));
    assert!(!h.with(|ede| ede.desk.seat.get_pointer().unwrap().is_grabbed()), "a click ends it");
}

/// Super+Space cycles `[keyboard] layout`, Shift back; the shell hears it.
#[test]
fn super_space_switches_the_keyboard_layout() {
    let mut config = Config::default();
    config.keyboard.layout = "us,de".into();
    config.keyboard.variant = String::new();
    let h = harness(config);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let layout = |shell: &mut ede_protocol::Client| loop {
        let e = shell.next_event().unwrap();
        if e.kind == events::KEYBOARD_LAYOUT {
            return (e.data["index"].as_u64().unwrap(), e.data["layouts"].as_array().unwrap().len());
        }
    };
    chord(&h, &[KEY_LEFTMETA], KEY_SPACE);
    assert_eq!(layout(&mut shell), (1, 2));
    chord(&h, &[KEY_LEFTMETA], KEY_SPACE);
    assert_eq!(layout(&mut shell), (0, 2), "wraps");
    chord(&h, &[KEY_LEFTSHIFT, KEY_LEFTMETA], KEY_SPACE);
    assert_eq!(layout(&mut shell), (1, 2), "Shift: backwards");
}
