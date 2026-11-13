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
const KEY_LEFT: u32 = 105 + 8;
const BTN_LEFT: u32 = 0x110;

fn harness(outputs: &[(i32, i32)]) -> Harness {
    // Compositor warnings (an Xwayland that won't start) show in failures.
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    // Flush with the screen: the IDE look's panel margin (#EDE-48) has its
    // own test below; every other geometry here reads off the output.
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, outputs)
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

/// #EDE-64: the Exponential app asks to be maximized before its first
/// frame on eDE; it maps filling the ground, never floating centred.
#[test]
fn a_window_maximized_before_it_maps_opens_filling_the_work_area() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("w");
    t.toplevel.set_maximized();
    c.roundtrip();
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |c| c.state.toplevels[0].states.contains(&XdgState::Maximized) && c.state.toplevels[0].size == (1280, 800));
    c.draw_configured(&t, 400, 300, RED);
    wait_for(&mut c, |_| windows(&h).first().is_some_and(|w| w.1 == Rectangle::new((0, 0).into(), (1280, 800).into())));
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

/// #EDE-60: a top layer (Settings, the switcher, a notification) is drawn
/// above every window, also after the window repaints, is raised or gets
/// the focus under it.
#[test]
fn a_top_layer_stays_above_the_windows_under_it() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = c.toplevel("calculator");
    c.draw(&t, 1000, 600, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    assert_eq!(h.capture(0).at(640, 400), RED);

    // Settings: a 600x400 top layer, centred (no anchor).
    let settings = c.layer("settings", Anchor::empty(), (600, 400), 0);
    c.draw_layer(&settings, GREEN);
    wait_for(&mut c, |_| h.capture(0).at(640, 400) == GREEN);
    let frame = h.capture(0);
    assert_eq!(frame.at(340, 200), GREEN, "the layer's top-left corner");
    assert_eq!(frame.at(939, 599), GREEN);
    assert_eq!(frame.at(200, 150), RED, "the window shows around it");

    // The window repaints under it (GTK redraws on focus loss): still below.
    c.draw(&t, 1000, 600, BLUE);
    wait_for(&mut c, |_| h.capture(0).at(200, 150) == BLUE);
    assert_eq!(h.capture(0).at(640, 400), GREEN, "a repaint under the layer does not bring the window over it");

    // A click on the window raises and focuses it: still below.
    h.with(|ede| {
        ede.pointer_motion_to((200.0, 150.0).into(), 0);
        ede.pointer_button(BTN_LEFT, ButtonState::Pressed, 1);
        ede.pointer_button(BTN_LEFT, ButtonState::Released, 2);
    });
    c.draw(&t, 1000, 600, RED);
    wait_for(&mut c, |_| h.capture(0).at(200, 150) == RED);
    assert_eq!(h.capture(0).at(640, 400), GREEN, "focusing the window does not bring it over the layer");
    assert_eq!(h.capture(0).at(939, 599), GREEN);

    // A window that opens while the layer is up (Settings open, an app
    // launched from it) maps below it too.
    let later = c.toplevel("files");
    c.draw(&later, 800, 500, BLUE);
    wait_for(&mut c, |_| windows(&h).len() == 2);
    eprintln!("windows: {:?}", windows(&h));
    let frame = h.capture(0);
    eprintln!("after mapping files: centre {:#x}, corner {:#x}, (300,250) {:#x}", frame.at(640, 400), frame.at(939, 599), frame.at(300, 250));
    assert_eq!(frame.at(640, 400), GREEN, "a window opened under the layer stays under it");
    assert_eq!(frame.at(939, 599), GREEN);
}

/// #EDE-65: a click on a shell layer over a window (an Alt-Tab tile, with
/// Alt still held) is the layer's: the window under it is neither focused
/// nor dragged by Alt+drag.
#[test]
fn an_alt_click_on_a_layer_over_a_window_leaves_the_window_alone() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let under = c.toplevel("under");
    c.draw(&under, 1000, 600, RED);
    let focused = c.toplevel("focused");
    c.draw(&focused, 300, 200, BLUE);
    wait_for(&mut c, |_| windows(&h).len() == 2);
    let focus_title = || h.with(|ede| ede.space.elements().find(|w| w.toplevel().is_some_and(|t| t.with_pending_state(|s| s.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated)))).and_then(|w| smithay::wayland::compositor::with_states(w.toplevel().unwrap().wl_surface(), |s| s.data_map.get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>().unwrap().lock().unwrap().title.clone())));
    assert_eq!(focus_title().as_deref(), Some("focused"));
    let before = windows(&h);

    let switcher = c.layer("switcher", Anchor::empty(), (600, 200), 0);
    c.draw_layer(&switcher, GREEN);
    wait_for(&mut c, |_| h.capture(0).at(640, 400) == GREEN);
    key(&h, KEY_LEFTALT, KeyState::Pressed);
    h.with(|ede| {
        ede.pointer_motion_to((500.0, 400.0).into(), 0);
        ede.pointer_button(BTN_LEFT, ButtonState::Pressed, 1);
        ede.pointer_motion_to((700.0, 450.0).into(), 2);
        ede.pointer_button(BTN_LEFT, ButtonState::Released, 3);
    });
    key(&h, KEY_LEFTALT, KeyState::Released);
    c.roundtrip();
    assert_eq!(focus_title().as_deref(), Some("focused"), "the window under the layer took the focus");
    assert_eq!(windows(&h), before, "an Alt+drag started on the window under the layer");
}

/// #EDE-48: the IDE look. With the default `[windows] panel_margin` (10)
/// the work area is the output minus the panel's zone, inset on every
/// side: maximize, Super+Left and a new window all keep off the frame.
#[test]
fn the_panel_margin_keeps_windows_off_the_frame() {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(Config::default(), &[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let panel = c.layer("panel", Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 40), 40);
    c.draw_layer(&panel, GREEN);
    wait_for(&mut c, |_| h.with(|ede| ede.work_area(ede.space.outputs().next().unwrap()).unwrap()) == Rectangle::new((10, 10).into(), (1260, 740).into()));

    let t = c.toplevel("w");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    assert_eq!(windows(&h)[0].1.loc, (440, 230).into(), "centred in the inset area");

    chord(&h, KEY_LEFTMETA, KEY_UP);
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1260, 740));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1 == Rectangle::new((10, 10).into(), (1260, 740).into()));
    let frame = h.capture(0);
    assert_eq!(frame.at(640, 400), RED);
    assert_eq!(frame.at(5, 400), BACKGROUND, "the frame's left margin shows the background");
    assert_eq!(frame.at(640, 755), BACKGROUND, "and the margin above the panel");
    assert_eq!(frame.at(640, 770), GREEN, "the panel itself is untouched");

    chord(&h, KEY_LEFTMETA, KEY_LEFT);
    wait_for(&mut c, |c| c.state.toplevels[0].size == (630, 740));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1 == Rectangle::new((10, 10).into(), (630, 740).into()));
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
///
/// #EXP-1806: with `EDE_XWAYLAND_VERDICT=<file>` a missing Xwayland fails
/// instead of skipping, and the test writes `passed` to that file as its
/// last step. build-ede reads that verdict: Xwayland shares our stderr, so
/// its `(EE)` lines can land inside libtest's `test … ok` line and a grep
/// over the log is a race.
#[cfg(feature = "xwayland")]
#[test]
fn an_x11_window_through_xwayland_opens_centred_takes_focus_and_is_drawn() {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, PropMode, WindowClass},
        wrapper::ConnectionExt as _,
    };
    let verdict = std::env::var_os("EDE_XWAYLAND_VERDICT");
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        assert!(verdict.is_none(), "no Xwayland binary on PATH, and EDE_XWAYLAND_VERDICT requires one");
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let h = harness(&[(1280, 800)]);
    h.with(|ede| ede.start_xwayland());
    let deadline = Instant::now() + Duration::from_secs(30);
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

    let deadline = Instant::now() + Duration::from_secs(30);
    let geo = loop {
        let w = h.with(|ede| ede.space.elements().find(|w| w.x11_surface().is_some()).and_then(|w| ede.space.element_geometry(w)));
        if let Some(g) = w.filter(|g| g.size.w > 0) {
            break g;
        }
        assert!(Instant::now() < deadline, "the X11 window never mapped");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(geo, Rectangle::new((480, 280).into(), (320, 240).into()), "centred like a Wayland window");
    // The focus follows the map asynchronously (the X11 WM handshake), so
    // wait for it instead of racing it: CI runners lost the race.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(|ede| ede.focused_window().is_some_and(|w| w.x11_surface().is_some())) {
        assert!(Instant::now() < deadline, "it has keyboard focus");
        std::thread::sleep(Duration::from_millis(20));
    }

    // Xwayland draws the background pixel once the frame commits.
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.capture(0).at(640, 400) != RED {
        assert!(Instant::now() < deadline, "the X11 window's pixels never showed");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(h.capture(0).at(479, 400), BACKGROUND);
    if let Some(path) = verdict {
        std::fs::write(path, "passed\n").expect("write the verdict");
    }
}

/// #EDE-16: a window that leaves decorations to the compositor gets a
/// title bar above its geometry, tiles under it, closes from its button;
/// one that insists on its own gets none.
#[test]
fn server_side_frames_sit_above_the_window_and_their_buttons_work() {
    use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;
    let h = harness(&[(800, 600)]);
    let mut app = Session::connect(h.connect());
    assert!(app.state.globals.iter().any(|g| g == "zxdg_decoration_manager_v1"));
    let t = app.toplevel("Terminal");
    app.decorate(&t, None);
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.decoration_modes.get(&t.index) == Some(&(Mode::ServerSide as u32)));
    let (geo, framed) = h.with(|ede| {
        let w = ede.space.elements().next().cloned().unwrap();
        (ede.space.element_geometry(&w).unwrap(), ede.framed_geometry(&w).unwrap())
    });
    assert_eq!(framed.size.h, geo.size.h + ede_compositor::frames::HEIGHT);
    assert_eq!(framed.loc.y + ede_compositor::frames::HEIGHT, geo.loc.y);
    assert!(framed.loc.y >= 0, "the frame is on screen: {framed:?}");
    // The bar is the card colour, drawn by the compositor.
    let frame = h.capture(0);
    let bar = frame.at(geo.loc.x + 10, geo.loc.y - 10);
    assert_eq!(bar & 0x00ff_ffff, 0x0017_1717, "{bar:08x}");
    assert_eq!(frame.at(geo.loc.x + 10, geo.loc.y + 10), BLUE);

    // Maximized: the frame stays, the window fills the rest of the work area.
    let max_x = (geo.loc.x + geo.size.w - 2 * 28 - 4 + 10) as f64;
    h.with(move |ede| {
        ede.pointer_motion_to((max_x, (geo.loc.y - 18) as f64).into(), 0);
        ede.pointer_button(0x110, smithay::backend::input::ButtonState::Pressed, 0);
        ede.pointer_button(0x110, smithay::backend::input::ButtonState::Released, 0);
    });
    wait_for(&mut app, |c| c.state.toplevels[t.index].size.1 > 0 && c.state.toplevels[t.index].size.1 < 600);
    let (max_geo, tile) = h.with(|ede| {
        let w = ede.space.elements().next().cloned().unwrap();
        (ede.space.element_geometry(&w).unwrap(), window_state(&w).tile)
    });
    assert_eq!(tile, Some(Tile::Maximized));
    assert_eq!(max_geo.loc.y, ede_compositor::frames::HEIGHT, "the bar sits at the top of the work area");

    // The close button asks the app to close.
    let close_x = (max_geo.loc.x + max_geo.size.w - 28 - 4 + 10) as f64;
    h.with(move |ede| {
        ede.pointer_motion_to((close_x, (max_geo.loc.y - 18) as f64).into(), 0);
        ede.pointer_button(0x110, smithay::backend::input::ButtonState::Pressed, 0);
        ede.pointer_button(0x110, smithay::backend::input::ButtonState::Released, 0);
    });
    wait_for(&mut app, |c| c.state.toplevels[t.index].closed);

    // Client-side decorations: no frame.
    let csd = app.toplevel("Browser");
    app.decorate(&csd, Some(Mode::ClientSide));
    app.draw(&csd, 300, 200, RED);
    wait_for(&mut app, |c| c.state.decoration_modes.get(&csd.index) == Some(&(Mode::ClientSide as u32)));
    assert!(h.with(|ede| {
        let w = ede.space.elements().last().cloned().unwrap();
        ede.frame_height(&w) == 0
    }));
}

/// #EDE-16: with no shell subscribed the compositor draws the recovery
/// screen; a shell that subscribes clears it.
#[test]
fn the_recovery_screen_shows_without_a_shell() {
    let h = harness(&[(800, 600)]);
    assert_ne!(h.capture(0).at(400, 300) & 0x00ff_ffff, 0x0017_1717, "still within the start grace");
    h.with(|ede| ede.recovery.start_grace = Duration::ZERO);
    assert_eq!(h.capture(0).at(400, 300) & 0x00ff_ffff, 0x0017_1717, "the card in the middle");
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(h.capture(0).at(400, 300), BACKGROUND);
    drop(shell);
    std::thread::sleep(Duration::from_millis(100));
    h.with(|ede| ede.recovery.return_grace = Duration::ZERO);
    assert_eq!(h.capture(0).at(400, 300) & 0x00ff_ffff, 0x0017_1717, "gone for longer than the grace");
}

/// Presses the left button at `at` and hands the press serial back, as a
/// client-side title bar does before it asks for a move or a resize.
fn press_on_window(h: &Harness, c: &mut Session, at: (f64, f64)) -> u32 {
    c.state.press_serial = None;
    h.with(move |ede| {
        ede.pointer_motion_to(at.into(), 0);
        ede.pointer_button(BTN_LEFT, ButtonState::Pressed, 1);
    });
    wait_for(c, |c| c.state.press_serial.is_some());
    c.state.press_serial.unwrap()
}

/// Drags the held button to `to` in a few steps and lets go.
fn drag_to(h: &Harness, from: (f64, f64), to: (f64, f64)) {
    h.with(move |ede| {
        for i in 1..=4 {
            let t = i as f64 / 4.0;
            ede.pointer_motion_to((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t).into(), 1 + i);
        }
        ede.pointer_button(BTN_LEFT, ButtonState::Released, 10);
    });
}

/// #EDE-65: a client that draws its own title bar (GTK, gpui) moves and
/// resizes through xdg_toplevel's requests: the drag follows the pointer,
/// a resize respects the edge and the client's minimum size, and a
/// maximized window dragged off its tile stays under the pointer even
/// though the client asked only past its own drag threshold.
#[test]
fn a_client_side_title_bar_moves_resizes_and_unmaximizes_its_window() {
    use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let _pointer = c.pointer();
    let seat = c.state.seat.clone().unwrap();
    let t = c.toplevel("csd");
    c.draw(&t, 400, 300, RED);
    wait_for(&mut c, |_| !windows(&h).is_empty());
    let start = windows(&h)[0].1;
    assert_eq!(h.with(|ede| ede.frame_height(ede.space.elements().next().unwrap())), 0, "no xdg-decoration: the client draws its own bar");

    // Move: the header drag.
    let grab = ((start.loc.x + 150) as f64, (start.loc.y + 10) as f64);
    let serial = press_on_window(&h, &mut c, grab);
    t.toplevel._move(&seat, serial);
    c.roundtrip();
    drag_to(&h, grab, (grab.0 - 100.0, grab.1 + 140.0));
    c.roundtrip();
    let moved = windows(&h)[0].1;
    assert_eq!(moved.loc, (start.loc.x - 100, start.loc.y + 140).into());

    // Resize from the bottom-right corner.
    let corner = ((moved.loc.x + 399) as f64, (moved.loc.y + 299) as f64);
    let serial = press_on_window(&h, &mut c, corner);
    t.toplevel.resize(&seat, serial, ResizeEdge::BottomRight);
    c.roundtrip();
    h.with(move |ede| ede.pointer_motion_to((corner.0 + 100.0, corner.1 + 50.0).into(), 2));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (500, 350) && c.state.toplevels[0].states.contains(&XdgState::Resizing));
    c.draw_configured(&t, 0, 0, RED);
    h.with(|ede| ede.pointer_button(BTN_LEFT, ButtonState::Released, 3));
    wait_for(&mut c, |c| !c.state.toplevels[0].states.contains(&XdgState::Resizing));
    c.draw_configured(&t, 0, 0, RED);
    assert_eq!(windows(&h)[0].1, Rectangle::new(moved.loc, (500, 350).into()));

    // From the top-left corner, past the client's minimum size: clamped,
    // and the bottom-right corner stays put.
    t.toplevel.set_min_size(200, 150);
    c.draw_configured(&t, 0, 0, RED);
    let top_left = ((moved.loc.x + 1) as f64, (moved.loc.y + 1) as f64);
    let serial = press_on_window(&h, &mut c, top_left);
    t.toplevel.resize(&seat, serial, ResizeEdge::TopLeft);
    c.roundtrip();
    h.with(|ede| ede.pointer_motion_to((1200.0, 780.0).into(), 2));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (200, 150));
    c.draw_configured(&t, 0, 0, RED);
    h.with(|ede| ede.pointer_button(BTN_LEFT, ButtonState::Released, 3));
    c.roundtrip();
    c.draw_configured(&t, 0, 0, RED);
    let small = windows(&h)[0].1;
    assert_eq!(small, Rectangle::new((moved.loc.x + 300, moved.loc.y + 200).into(), (200, 150).into()));

    // Double-click on its header = the client's maximize request.
    t.toplevel.set_maximized();
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1280, 800) && c.state.toplevels[0].states.contains(&XdgState::Maximized));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| windows(&h)[0].1.size == (1280, 800).into());

    // Dragged off by its header: the client asks for the move only after
    // its threshold, the window comes back floating under the pointer.
    let header = (640.0, 10.0);
    let serial = press_on_window(&h, &mut c, header);
    h.with(|ede| ede.pointer_motion_to((646.0, 30.0).into(), 2));
    t.toplevel._move(&seat, serial);
    c.roundtrip();
    drag_to(&h, (646.0, 30.0), (700.0, 300.0));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (200, 150) && !c.state.toplevels[0].states.contains(&XdgState::Maximized));
    c.draw_configured(&t, 0, 0, RED);
    let floating = windows(&h)[0].1;
    assert_eq!(h.with(|ede| window_state(ede.space.elements().next().unwrap()).tile), None);
    assert_eq!(300 - floating.loc.y, 10, "the pointer holds the header where it grabbed it: {floating:?}");
    assert!(floating.to_f64().contains((700.0, 300.0)), "{floating:?}");
}

/// #EDE-65: the server-side bar has the IDE's controls (minimize next to
/// maximize and close), and a client-decorated window stacked over that bar
/// takes a press there itself.
#[test]
fn the_frame_minimizes_and_a_window_above_it_keeps_its_own_presses() {
    use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;
    let h = harness(&[(800, 600)]);
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("Terminal");
    app.decorate(&t, None);
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.decoration_modes.get(&t.index) == Some(&(Mode::ServerSide as u32)));
    let geo = windows(&h)[0].1;
    let bar_y = (geo.loc.y - 18) as f64;

    // A client-decorated window over the bar's title.
    let mut other = Session::connect(h.connect());
    let _pointer = other.pointer();
    let csd = other.toplevel("Calculator");
    other.draw(&csd, 200, 200, RED);
    wait_for(&mut other, |_| windows(&h).len() == 2);
    let over = (geo.loc.x - 50, geo.loc.y - 100);
    h.with(move |ede| {
        let w = ede.space.elements().last().cloned().unwrap();
        ede.space.map_element(w, over, true);
    });
    other.state.press_serial = None;
    let title = ((geo.loc.x + 20) as f64, bar_y);
    h.with(move |ede| {
        ede.pointer_motion_to(title.into(), 0);
        ede.pointer_button(BTN_LEFT, ButtonState::Pressed, 1);
        ede.pointer_button(BTN_LEFT, ButtonState::Released, 2);
    });
    wait_for(&mut other, |c| c.state.press_serial.is_some());
    assert_eq!(windows(&h)[0].1, geo, "the frame under it did not start a move");

    // Minimize: third from the right; the window leaves the screen and
    // comes back where it was.
    let min_x = (geo.loc.x + geo.size.w - 4 - 2 * 28 - 14) as f64;
    h.with(move |ede| {
        ede.pointer_motion_to((min_x, bar_y).into(), 3);
        ede.pointer_button(BTN_LEFT, ButtonState::Pressed, 4);
        ede.pointer_button(BTN_LEFT, ButtonState::Released, 5);
    });
    assert!(h.with(|ede| ede.space.elements().count() == 1 && ede.minimized.len() == 1));
    let restored = h.with(|ede| {
        let w = ede.minimized[0].clone();
        ede.restore_window(&w);
        ede.space.element_geometry(&w)
    });
    assert_eq!(restored, Some(geo));
}
