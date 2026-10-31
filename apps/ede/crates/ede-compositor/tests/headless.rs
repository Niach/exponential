//! The compositor end to end, headless: a real Wayland client over a socket,
//! real input calls, and the pixels pixman drew. Runs in CI without a GPU.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, layout::Tile, state::window_state, Config};
use smithay::{
    backend::input::{ButtonState, KeyState},
    utils::Rectangle,
};
use wayland_protocols::xdg::shell::client::xdg_toplevel::State as XdgState;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const GREEN: u32 = 0xff00_ff00;
const BACKGROUND: u32 = 0xff0a_0a0a;

// evdev codes + 8 = xkb keycodes
const KEY_LEFTALT: u32 = 56 + 8;
const KEY_LEFTMETA: u32 = 125 + 8;
const KEY_F4: u32 = 62 + 8;
const KEY_UP: u32 = 103 + 8;
const KEY_DOWN: u32 = 108 + 8;
const BTN_LEFT: u32 = 0x110;

fn harness(outputs: &[(i32, i32)]) -> Harness {
    Harness::start(Config::default(), outputs)
}

/// Window geometries in the space, back to front, with their titles.
fn windows(h: &Harness) -> Vec<(String, Rectangle<i32, smithay::utils::Logical>)> {
    h.with(|ede| {
        ede.space
            .elements()
            .map(|w| {
                let title = smithay::wayland::compositor::with_states(w.toplevel().unwrap().wl_surface(), |s| {
                    s.data_map
                        .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .title
                        .clone()
                        .unwrap_or_default()
                });
                (title, ede.space.element_geometry(w).unwrap())
            })
            .collect()
    })
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn key(h: &Harness, code: u32, state: KeyState) {
    h.with(move |ede| ede.keyboard_key(code.into(), state, 0));
}

fn chord(h: &Harness, modifier: u32, code: u32) {
    key(h, modifier, KeyState::Pressed);
    key(h, code, KeyState::Pressed);
    key(h, code, KeyState::Released);
    key(h, modifier, KeyState::Released);
}

/// Alt+drag from `from` to `to` in a few steps.
fn alt_drag(h: &Harness, from: (f64, f64), to: (f64, f64)) {
    key(h, KEY_LEFTALT, KeyState::Pressed);
    h.with(move |ede| {
        ede.pointer_motion_to(from.into(), 0);
        ede.pointer_button(BTN_LEFT, ButtonState::Pressed, 1);
        for i in 1..=4 {
            let t = i as f64 / 4.0;
            ede.pointer_motion_to((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t).into(), 1 + i);
        }
        ede.pointer_button(BTN_LEFT, ButtonState::Released, 10);
    });
    key(h, KEY_LEFTALT, KeyState::Released);
}

#[test]
fn a_client_window_opens_centred_and_is_drawn() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("calculator");
    assert_eq!(c.state.toplevels[0].configures, 1, "initial configure");
    // Not mapped before its first buffer.
    assert!(windows(&h).is_empty());

    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    assert_eq!(windows(&h), vec![("calculator".into(), Rectangle::new((440, 250).into(), (400, 300).into()))]);
    // It got keyboard focus, and the configure says so.
    wait_for(&mut c, |c| c.state.toplevels[0].states.contains(&XdgState::Activated));

    let frame = h.capture(0);
    assert_eq!((frame.width, frame.height), (1280, 800));
    assert_eq!(frame.at(640, 400), RED, "the window's pixels are on screen");
    assert_eq!(frame.at(440, 250), RED);
    assert_eq!(frame.at(439, 250), BACKGROUND);
    assert_eq!(frame.at(10, 10), BACKGROUND, "the design-token background elsewhere");
}

#[test]
fn new_windows_cascade_and_stack() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let a = c.toplevel("a");
    c.draw(&a, 400, 300, RED);
    let b = c.toplevel("b");
    c.draw(&b, 400, 300, BLUE);
    wait_for(&mut c, |_| windows(&h).len() == 2);
    let w = windows(&h);
    assert_eq!(w[1], ("b".into(), Rectangle::new((472, 282).into(), (400, 300).into())));
    // b is on top where they overlap.
    assert_eq!(h.capture(0).at(600, 400), BLUE);
}

#[test]
fn dropping_a_window_on_the_left_edge_tiles_it_and_dragging_it_off_restores_it() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("editor");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());

    alt_drag(&h, (640.0, 400.0), (0.0, 400.0));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (640, 800));
    assert!(c.state.toplevels[0].states.contains(&XdgState::TiledLeft));
    assert_eq!(h.with(|ede| window_state(ede.space.elements().next().unwrap()).tile), Some(Tile::Left));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1 == Rectangle::new((0, 0).into(), (640, 800).into()));
    let frame = h.capture(0);
    assert_eq!((frame.at(0, 0), frame.at(639, 799), frame.at(640, 400)), (RED, RED, BACKGROUND));

    // Pulled off the tile: back to 400×300 under the pointer.
    alt_drag(&h, (320.0, 10.0), (700.0, 300.0));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (400, 300));
    assert!(!c.state.toplevels[0].states.contains(&XdgState::TiledLeft));
    assert_eq!(h.with(|ede| window_state(ede.space.elements().next().unwrap()).tile), None);
}

#[test]
fn window_edges_stick_to_the_screen_edge() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("w");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    // Dropped 7 px from the left edge: it sticks flush.
    alt_drag(&h, (640.0, 400.0), (207.0, 400.0));
    assert_eq!(windows(&h)[0].1.loc, (0, 250).into());
    // 30 px away is outside the 12 px threshold: no snap.
    alt_drag(&h, (200.0, 400.0), (230.0, 400.0));
    assert_eq!(windows(&h)[0].1.loc, (30, 250).into());
}

#[test]
fn super_up_maximizes_and_super_down_restores() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("w");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());

    chord(&h, KEY_LEFTMETA, KEY_UP);
    wait_for(&mut c, |c| c.state.toplevels[0].states.contains(&XdgState::Maximized));
    assert_eq!(c.state.toplevels[0].size, (1280, 800));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1 == Rectangle::new((0, 0).into(), (1280, 800).into()));

    chord(&h, KEY_LEFTMETA, KEY_DOWN);
    wait_for(&mut c, |c| !c.state.toplevels[0].states.contains(&XdgState::Maximized));
    assert_eq!(c.state.toplevels[0].size, (400, 300));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1 == Rectangle::new((440, 250).into(), (400, 300).into()));
}

#[test]
fn a_client_maximize_request_fills_the_work_area() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("w");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    t.toplevel.set_maximized();
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1280, 800));
    t.toplevel.unset_maximized();
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (400, 300));
}

#[test]
fn alt_f4_asks_the_focused_window_to_close_and_focus_falls_to_the_next() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let a = c.toplevel("a");
    c.draw(&a, 400, 300, RED);
    let b = c.toplevel("b");
    c.draw(&b, 400, 300, BLUE);
    wait_for(&mut c, |_| windows(&h).len() == 2);

    chord(&h, KEY_LEFTALT, KEY_F4);
    wait_for(&mut c, |c| c.state.toplevels[1].closed);
    assert!(!c.state.toplevels[0].closed);

    // The client obeys; a regains focus.
    b.toplevel.destroy();
    b.xdg_surface.destroy();
    b.surface.destroy();
    c.roundtrip();
    wait_for(&mut c, |_| windows(&h).len() == 1);
    wait_for(&mut c, |c| c.state.toplevels[0].states.contains(&XdgState::Activated));
}

#[test]
fn a_panel_layer_reserves_its_zone_and_windows_place_and_tile_around_it() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let panel = c.layer("panel", Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 40), 40);
    assert_eq!(c.state.layers[0].size, (1280, 40), "stretched across the output");
    c.draw_layer(&panel, GREEN);
    wait_for(&mut c, |_| h.with(|ede| ede.work_area(ede.space.outputs().next().unwrap()).unwrap().size.h) == 760);

    let t = c.toplevel("w");
    // The initial configure bounds it to the work area.
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    assert_eq!(windows(&h)[0].1.loc, (440, 230).into(), "centred in the 760 px above the panel");

    chord(&h, KEY_LEFTMETA, KEY_UP);
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1280, 760));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1.size == (1280, 760).into());

    let frame = h.capture(0);
    assert_eq!(frame.at(640, 759), RED);
    assert_eq!(frame.at(640, 760), GREEN, "the panel is drawn below the window's work area");
    assert_eq!(frame.at(5, 795), GREEN);
}

#[test]
fn outputs_line_up_left_to_right_and_windows_open_on_the_pointers_output() {
    let h = harness(&[(1280, 800), (1920, 1080)]);
    let geos = h.with(|ede| ede.space.outputs().map(|o| ede.space.output_geometry(o).unwrap()).collect::<Vec<_>>());
    assert_eq!(geos, vec![Rectangle::new((0, 0).into(), (1280, 800).into()), Rectangle::new((1280, 0).into(), (1920, 1080).into())]);

    let mut c = Session::connect(h.connect());
    assert_eq!(c.state.outputs.len(), 2, "both wl_outputs advertised");
    h.with(|ede| ede.pointer_motion_to((2000.0, 500.0).into(), 0));
    let t = c.toplevel("w");
    c.draw(&t, 400, 300, BLUE);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    assert_eq!(windows(&h)[0].1.loc, (1280 + 760, 390).into());
    let second = h.capture(1);
    assert_eq!(second.at(960, 540), BLUE);
    assert_eq!(h.capture(0).at(640, 400), BACKGROUND);

    // The pointer can't leave the outputs.
    h.with(|ede| ede.pointer_motion_to((9000.0, -50.0).into(), 1));
    let p = h.with(|ede| ede.seat.get_pointer().unwrap().current_location());
    assert_eq!((p.x, p.y), (3199.0, 0.0));
}

#[test]
fn unplugging_an_output_moves_its_windows_onto_the_remaining_one() {
    let h = harness(&[(1280, 800), (1920, 1080), (1280, 800)]);
    let mut c = Session::connect(h.connect());
    h.with(|ede| ede.pointer_motion_to((2000.0, 500.0).into(), 0));
    let t = c.toplevel("w");
    c.draw(&t, 400, 300, BLUE);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    assert_eq!(windows(&h)[0].1.loc, (2040, 390).into());

    h.with_backend(|ede, headless| headless.remove_output(ede, 1));
    let geos = h.with(|ede| ede.space.outputs().map(|o| ede.space.output_geometry(o).unwrap()).collect::<Vec<_>>());
    assert_eq!(geos, vec![Rectangle::new((0, 0).into(), (1280, 800).into()), Rectangle::new((1280, 0).into(), (1280, 800).into())], "no gap left");
    assert_eq!(windows(&h)[0].1.loc, (440, 250).into(), "rescued onto the first output");
    assert_eq!(h.capture(0).at(640, 400), BLUE);
}

#[test]
fn fullscreen_covers_the_panel_and_the_output_shows_only_that_window() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let panel = c.layer("panel", Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 40), 40);
    c.draw_layer(&panel, GREEN);
    let behind = c.toplevel("behind");
    c.draw(&behind, 300, 200, BLUE);
    let t = c.toplevel("game");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| windows(&h).len() == 2);

    t.toplevel.set_fullscreen(None);
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[1].states.contains(&XdgState::Fullscreen));
    assert_eq!(c.state.toplevels[1].size, (1280, 800), "the whole output, not the work area");
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[1].1 == Rectangle::new((0, 0).into(), (1280, 800).into()));
    let frame = h.capture(0);
    assert_eq!((frame.at(640, 790), frame.at(0, 0)), (RED, RED), "over the panel");

    // Super+Up does nothing to a fullscreen window; leaving restores it.
    chord(&h, KEY_LEFTMETA, KEY_UP);
    t.toplevel.unset_fullscreen();
    c.roundtrip();
    wait_for(&mut c, |c| !c.state.toplevels[1].states.contains(&XdgState::Fullscreen));
    assert_eq!(c.state.toplevels[1].size, (400, 300));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[1].1.size == (400, 300).into());
    assert_eq!(h.capture(0).at(640, 790), GREEN, "the panel is back");
}

/// Xwayland must be installed (CI installs it); skipped otherwise.
#[cfg(feature = "xwayland")]
#[test]
fn an_x11_window_through_xwayland_opens_centred_takes_focus_and_is_drawn() {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, PropMode, WindowClass},
        wrapper::ConnectionExt as _,
    };
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let h = harness(&[(1280, 800)]);
    h.with(|ede| ede.start_xwayland());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(n) = h.with(|ede| ede.xdisplay) {
            break n;
        }
        assert!(Instant::now() < deadline, "Xwayland never became ready");
        std::thread::sleep(Duration::from_millis(20));
    };

    let (conn, screen) = x11rb::connect(Some(&format!(":{display}"))).expect("connect to Xwayland");
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        320,
        240,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().background_pixel(0x00ff_0000),
    )
    .unwrap();
    conn.change_property8(PropMode::REPLACE, win, AtomEnum::WM_NAME, AtomEnum::STRING, b"xclient").unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let geo = loop {
        let w = h.with(|ede| ede.space.elements().find(|w| w.x11_surface().is_some()).and_then(|w| ede.space.element_geometry(w)));
        if let Some(g) = w.filter(|g| g.size.w > 0) {
            break g;
        }
        assert!(Instant::now() < deadline, "the X11 window never mapped");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(geo, Rectangle::new((480, 280).into(), (320, 240).into()), "centred like a Wayland window");
    assert!(h.with(|ede| ede.focused_window().is_some_and(|w| w.x11_surface().is_some())), "it has keyboard focus");

    // Xwayland draws the background pixel once the frame commits.
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.capture(0).at(640, 400) != RED {
        assert!(Instant::now() < deadline, "the X11 window's pixels never showed");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(h.capture(0).at(479, 400), BACKGROUND);
}
