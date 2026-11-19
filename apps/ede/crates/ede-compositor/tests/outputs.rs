//! #EDE-139 headless: windows after a display change. A scale or mode
//! change, a resize under the session and an unplug leave every window on
//! an output: the outputs close ranks and their windows go along, tiled and
//! fullscreen windows fill their output again, a window parked on another
//! workspace comes back on one, and who is in front stays.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, desk::DeskId, layout::Tile, Config, Ede};
use smithay::{
    desktop::Window,
    utils::{Logical, Rectangle},
};
use wayland_protocols::xdg::shell::client::xdg_toplevel::State as XdgState;

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const GREEN: u32 = 0xff00_ff00;

type Rect = Rectangle<i32, Logical>;

fn harness(outputs: &[(i32, i32)]) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, outputs)
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rectangle::new((x, y).into(), (w, h).into())
}

fn title(w: &Window) -> String {
    smithay::wayland::compositor::with_states(w.toplevel().unwrap().wl_surface(), |s| {
        s.data_map.get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>().unwrap().lock().unwrap().title.clone().unwrap_or_default()
    })
}

fn window(ede: &Ede, name: &str) -> Window {
    ede.all_windows().into_iter().find(|w| title(w) == name).unwrap_or_else(|| panic!("no window {name}"))
}

/// Window geometries in the space, back to front, with their titles.
fn windows(h: &Harness) -> Vec<(String, Rect)> {
    h.with(|ede| ede.desk.space.elements().map(|w| (title(w), ede.desk.space.element_geometry(w).unwrap())).collect())
}

fn geometry(h: &Harness, name: &'static str) -> Rect {
    windows(h).into_iter().find(|(t, _)| t == name).unwrap_or_else(|| panic!("{name} is not on screen")).1
}

fn outputs(h: &Harness) -> Vec<Rect> {
    h.with(|ede| ede.desk.space.outputs().map(|o| ede.desk.space.output_geometry(o).unwrap()).collect())
}

#[track_caller]
fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

/// A 400×300 window called `name` opens where the pointer is, on `c`.
fn open(h: &Harness, c: &mut Session, name: &'static str, argb: u32) -> common::Toplevel {
    let t = c.toplevel(name);
    c.draw(&t, 400, 300, argb);
    wait_for(c, |_| windows(h).iter().any(|(n, _)| n == name));
    t
}

/// #EDE-139 (1): re-fitting tiled windows after a scale change (the
/// Displays page) puts none above the window being typed into.
#[test]
fn a_display_change_leaves_the_focused_window_in_front_and_activated() {
    let h = harness(&[(1280, 800)]);
    let mut browser = Session::connect(h.connect());
    let b = open(&h, &mut browser, "browser", BLUE);
    b.toplevel.set_maximized();
    browser.roundtrip();
    wait_for(&mut browser, |c| c.state.toplevels[0].size == (1280, 800));
    browser.draw_configured(&b, 0, 0, BLUE);
    let mut term = Session::connect(h.connect());
    term.keyboard();
    let t = open(&h, &mut term, "terminal", RED);
    h.with(|ede| {
        let w = window(ede, "terminal");
        ede.focus(DeskId::PERSON, Some(w));
    });
    wait_for(&mut term, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface) && c.state.toplevels[0].states.contains(&XdgState::Activated));

    h.ipc().set_output("HEADLESS-1", None, Some(1.25), None).unwrap();
    wait_for(&mut browser, |c| c.state.toplevels[0].size == (1024, 640));
    browser.draw_configured(&b, 0, 0, BLUE);
    term.roundtrip();
    let stack: Vec<_> = h.ipc().windows().unwrap().into_iter().map(|w| (w.title.unwrap_or_default(), w.focused)).collect();
    assert_eq!(stack, [("browser".to_string(), false), ("terminal".to_string(), true)], "the maximized window came in front");
    assert!(term.state.toplevels[0].states.contains(&XdgState::Activated));
    assert!(!browser.state.toplevels[0].states.contains(&XdgState::Activated));
}

/// #EDE-139 (2a): scale 2 on a 1920×1080 output leaves 960×540 of it; a
/// window that was at (1300, 700) comes back onto it.
#[test]
fn a_scale_change_brings_back_a_window_it_would_leave_off_screen() {
    let h = harness(&[(1920, 1080)]);
    let mut c = Session::connect(h.connect());
    open(&h, &mut c, "w", BLUE);
    h.with(|ede| {
        let w = window(ede, "w");
        ede.move_window(&w, (1300, 700).into(), false);
    });

    h.ipc().set_output("HEADLESS-1", None, Some(2.0), None).unwrap();
    assert_eq!(outputs(&h), [rect(0, 0, 960, 540)]);
    let g = geometry(&h, "w");
    assert!(rect(0, 0, 960, 540).contains_rect(g), "left off the output at {g:?}");
}

/// #EDE-139 (2b): the left output goes from scale 1 to 1.5. The right one
/// closes up (no gap the pointer cannot cross), its window goes along, and
/// the left output's windows stay on it: a tiled one fills its half again,
/// one near the edge it lost comes back inside it.
#[test]
fn outputs_close_ranks_after_a_scale_change_and_each_keeps_its_windows() {
    let h = harness(&[(1920, 1080), (1920, 1080)]);
    let mut c = Session::connect(h.connect());
    open(&h, &mut c, "right", BLUE);
    open(&h, &mut c, "edge", RED);
    let half = open(&h, &mut c, "half", GREEN);
    h.with(|ede| {
        let (right, edge, half) = (window(ede, "right"), window(ede, "edge"), window(ede, "half"));
        ede.move_window(&right, (2020, 100).into(), false);
        ede.move_window(&edge, (1500, 600).into(), false);
        ede.tile(&half, Tile::Right);
    });
    wait_for(&mut c, |c| c.state.toplevels[2].size == (960, 1080));
    c.draw_configured(&half, 0, 0, GREEN);
    wait_for(&mut c, |_| geometry(&h, "half") == rect(960, 0, 960, 1080));

    h.ipc().set_output("HEADLESS-1", None, Some(1.5), None).unwrap();
    assert_eq!(outputs(&h), [rect(0, 0, 1280, 720), rect(1280, 0, 1920, 1080)], "a gap between the outputs");
    assert_eq!(geometry(&h, "right").loc, (1380, 100).into(), "the right output's window went along");
    let edge = geometry(&h, "edge");
    assert!(rect(0, 0, 1280, 720).contains_rect(edge), "the window near the edge left its output: {edge:?}");
    wait_for(&mut c, |c| c.state.toplevels[2].size == (640, 720));
    c.draw_configured(&half, 0, 0, GREEN);
    wait_for(&mut c, |_| geometry(&h, "half") == rect(640, 0, 640, 720));

    // The pointer crosses onto the right output.
    let p = h.with(|ede| {
        ede.pointer_motion_to((1300.0, 500.0).into(), 0);
        ede.desk.seat.get_pointer().unwrap().current_location()
    });
    assert_eq!((p.x, p.y), (1300.0, 500.0));
}

/// #EDE-139 (2): the right output's mode shrinks under the session (a VM
/// window resized). The window it no longer holds comes back inside it,
/// not onto the first output.
#[test]
fn a_resize_under_the_session_keeps_a_window_on_its_own_output() {
    let h = harness(&[(1280, 800), (1920, 1080)]);
    let mut c = Session::connect(h.connect());
    open(&h, &mut c, "w", BLUE);
    h.with(|ede| {
        let w = window(ede, "w");
        ede.move_window(&w, (1280 + 1500, 700).into(), false);
        let output = ede.output_named("HEADLESS-2").unwrap();
        output.change_current_state(Some(smithay::output::Mode { size: (1024, 768).into(), refresh: 60_000 }), None, None, None);
        ede.output_resized(&output);
    });
    assert_eq!(outputs(&h), [rect(0, 0, 1280, 800), rect(1280, 0, 1024, 768)]);
    let g = geometry(&h, "w");
    assert!(rect(1280, 0, 1024, 768).contains_rect(g), "left its output: {g:?}");
}

/// #EDE-139 (2c): a fullscreen window fills its output again when the
/// output's mode changes, through the Displays page and under the session
/// (a VM window resized).
#[test]
fn a_fullscreen_window_follows_its_outputs_mode() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = open(&h, &mut c, "game", RED);
    t.toplevel.set_fullscreen(None);
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1280, 800));
    c.draw_configured(&t, 0, 0, RED);

    h.ipc().set_output("HEADLESS-1", Some("1600x900"), None, None).unwrap();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1600, 900) && c.state.toplevels[0].states.contains(&XdgState::Fullscreen));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| geometry(&h, "game") == rect(0, 0, 1600, 900));
    assert_eq!(h.capture(0).at(1590, 890), RED);

    h.with(|ede| {
        let output = ede.output_named("HEADLESS-1").unwrap();
        output.change_current_state(Some(smithay::output::Mode { size: (1920, 1080).into(), refresh: 60_000 }), None, None, None);
        ede.output_resized(&output);
    });
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1920, 1080));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| geometry(&h, "game") == rect(0, 0, 1920, 1080));
}

/// #EDE-139 (2c): unplugged under a fullscreen window, it is fullscreen on
/// the output left, and leaving fullscreen puts it back on that output.
#[test]
fn a_fullscreen_window_on_an_unplugged_output_is_fullscreen_on_the_one_left() {
    let h = harness(&[(1280, 800), (1920, 1080)]);
    let mut c = Session::connect(h.connect());
    h.with(|ede| ede.pointer_motion_to((2000.0, 500.0).into(), 0));
    let t = open(&h, &mut c, "game", RED);
    t.toplevel.set_fullscreen(None);
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1920, 1080));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| geometry(&h, "game") == rect(1280, 0, 1920, 1080));

    h.with_backend(|ede, headless| headless.remove_output(ede, 1));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1280, 800) && c.state.toplevels[0].states.contains(&XdgState::Fullscreen));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| geometry(&h, "game") == rect(0, 0, 1280, 800));
    assert_eq!(h.capture(0).at(640, 400), RED);

    t.toplevel.unset_fullscreen();
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (400, 300) && !c.state.toplevels[0].states.contains(&XdgState::Fullscreen));
    c.draw_configured(&t, 0, 0, RED);
    wait_for(&mut c, |_| geometry(&h, "game").size == (400, 300).into());
    let g = geometry(&h, "game");
    assert!(rect(0, 0, 1280, 800).contains_rect(g), "left fullscreen onto the unplugged output: {g:?}");
}

/// #EDE-139 (2d): a window parked on another workspace while its output
/// went away comes back on the output left.
#[test]
fn a_window_parked_while_its_output_went_away_comes_back_on_an_output() {
    let h = harness(&[(1280, 800), (1920, 1080)]);
    let mut c = Session::connect(h.connect());
    h.with(|ede| ede.pointer_motion_to((2000.0, 500.0).into(), 0));
    open(&h, &mut c, "parked", BLUE);
    assert_eq!(geometry(&h, "parked").loc, (2040, 390).into());
    h.with(|ede| ede.switch_workspace(1));
    h.with_backend(|ede, headless| headless.remove_output(ede, 1));

    h.with(|ede| ede.switch_workspace(0));
    let g = geometry(&h, "parked");
    assert!(rect(0, 0, 1280, 800).contains_rect(g), "came back off screen at {g:?}");
    assert_eq!(h.capture(0).at(g.loc.x + 200, g.loc.y + 150), BLUE);
}

/// #EDE-139 (2d): a maximized window parked on another workspace while
/// its output's scale changed fills the output again when it comes back.
#[test]
fn a_maximized_window_parked_during_a_scale_change_fills_its_output_when_it_comes_back() {
    let h = harness(&[(1280, 800)]);
    let mut c = Session::connect(h.connect());
    let t = open(&h, &mut c, "max", BLUE);
    t.toplevel.set_maximized();
    c.roundtrip();
    wait_for(&mut c, |c| c.state.toplevels[0].size == (1280, 800));
    c.draw_configured(&t, 0, 0, BLUE);
    wait_for(&mut c, |_| geometry(&h, "max") == rect(0, 0, 1280, 800));
    h.with(|ede| ede.switch_workspace(1));

    h.ipc().set_output("HEADLESS-1", None, Some(2.0), None).unwrap();
    h.with(|ede| ede.switch_workspace(0));
    wait_for(&mut c, |c| c.state.toplevels[0].size == (640, 400) && c.state.toplevels[0].states.contains(&XdgState::Maximized));
    c.draw_configured(&t, 0, 0, BLUE);
    wait_for(&mut c, |_| geometry(&h, "max") == rect(0, 0, 640, 400));
}
