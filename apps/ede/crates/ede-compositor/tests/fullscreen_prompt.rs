//! #EDE-107 headless: a sealed prompt (polkit, consent, share: an Exclusive
//! layer of `exponential-sealed`) that opens while a window is fullscreen is
//! the one layer drawn above that window, and it takes the keyboard and the
//! click. Every other layer stays hidden under fullscreen (#EDE-103).

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use smithay::backend::input::{ButtonState, KeyState};
use wayland_protocols::xdg::shell::client::xdg_toplevel::State as XdgState;
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer,
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity},
};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const GREEN: u32 = 0xff00_ff00;
const YELLOW: u32 = 0xffff_ff00;
const PURPLE: u32 = 0xffff_00ff;
const BLACK: u32 = 0xff00_0000;
// evdev code; xkb = evdev + 8
const KEY_A: u32 = 30;

/// Every output here is 1280×800; a prompt (200×100, no anchors) is centred:
/// (540, 350) to (740, 450) on its output.
const OUTPUT: (i32, i32) = (1280, 800);

fn harness(outputs: usize) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, &vec![OUTPUT; outputs])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn press(h: &Harness, code: u32) {
    h.with(move |ede| {
        ede.keyboard_key((code + 8).into(), KeyState::Pressed, 0);
        ede.keyboard_key((code + 8).into(), KeyState::Released, 0);
    });
}

/// A left click at `(x, y)`.
fn click(h: &Harness, x: f64, y: f64) {
    h.with(move |ede| {
        ede.pointer_motion_to((x, y).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
}

/// `c` opens a window on output `output` (where the pointer is) and makes it
/// fullscreen there.
fn fullscreen_window(h: &Harness, c: &mut Session, title: &str, argb: u32, output: usize) -> common::Toplevel {
    let t = c.toplevel(title);
    c.draw(&t, 400, 300, argb);
    wait_for(c, |_| h.capture(output).at(640, 400) == argb);
    t.toplevel.set_fullscreen(None);
    c.roundtrip();
    let index = t.index;
    wait_for(c, |c| c.state.toplevels[index].states.contains(&XdgState::Fullscreen));
    assert_eq!(c.state.toplevels[index].size, OUTPUT);
    c.draw_configured(&t, 0, 0, argb);
    wait_for(c, |_| {
        let frame = h.capture(output);
        (frame.at(0, 0), frame.at(1279, 799)) == (argb, argb)
    });
    t
}

/// A layer surface of `size` on `on`, on output `output` (`None` = the
/// compositor's choice, the first one).
fn layer_on(s: &mut Session, namespace: &str, on: Layer, output: Option<usize>, anchor: Anchor, size: (u32, u32), keyboard: KeyboardInteractivity) -> common::Layer {
    let surface = s.state.compositor.as_ref().unwrap().create_surface(&s.qh, ());
    let index = s.state.layers.len();
    s.state.layers.push(Default::default());
    let output = output.map(|i| s.state.outputs[i].clone());
    let layer = s.state.layer_shell.as_ref().expect("layer shell advertised").get_layer_surface(&surface, output.as_ref(), on, namespace.into(), &s.qh, index);
    layer.set_anchor(anchor);
    layer.set_size(size.0, size.1);
    layer.set_keyboard_interactivity(keyboard);
    surface.commit();
    s.roundtrip();
    common::Layer { index, surface, layer }
}

/// What `exponential-sealed polkit` maps: an Exclusive overlay. The test's
/// own process plays `exponential-sealed` while the surface maps (a layer's
/// origin is decided once, then).
fn sealed_prompt(h: &Harness, s: &mut Session, output: Option<usize>) -> common::Layer {
    sealed_prompt_taking(h, s, output, KeyboardInteractivity::Exclusive)
}

/// [`sealed_prompt`] with the keyboard taken at once (Exclusive) or on a
/// click (OnDemand: #EDE-143, a request from outside the person's session).
fn sealed_prompt_taking(h: &Harness, s: &mut Session, output: Option<usize>, keyboard: KeyboardInteractivity) -> common::Layer {
    let pid = std::process::id();
    h.with(move |ede| ede.sealed_stand_in = Some(pid));
    let l = layer_on(s, "polkit", Layer::Overlay, output, Anchor::empty(), (200, 100), keyboard);
    h.with(|ede| ede.sealed_stand_in = None);
    l
}

fn close_layer(s: &mut Session, l: common::Layer) {
    l.layer.destroy();
    l.surface.destroy();
    s.roundtrip();
}

/// The prompt is in the person's frame, above the fullscreen window, and
/// black in a capture like every sealed surface; it draws at the refresh rate.
#[test]
fn a_sealed_prompt_is_drawn_above_a_fullscreen_window_and_black_in_a_capture() {
    let h = harness(1);
    let mut game = Session::connect(h.connect());
    let _t = fullscreen_window(&h, &mut game, "game", RED, 0);

    let mut sealed = Session::connect(h.connect());
    let l = sealed_prompt(&h, &mut sealed, None);
    sealed.draw_layer(&l, BLUE);
    wait_for(&mut sealed, |_| h.capture(0).at(640, 400) == BLUE);
    let frame = h.capture(0);
    assert_eq!((frame.at(540, 350), frame.at(739, 449)), (BLUE, BLUE), "the whole prompt");
    assert_eq!((frame.at(539, 400), frame.at(740, 400), frame.at(640, 349), frame.at(640, 450)), (RED, RED, RED, RED), "the fullscreen window shows around it");

    let shared = h.capture_shared(0);
    assert_eq!((shared.at(540, 350), shared.at(640, 400), shared.at(739, 449)), (BLACK, BLACK, BLACK), "the prompt leaked into a capture");
    assert_eq!((shared.at(539, 400), shared.at(100, 100)), (RED, RED), "the window around it is still captured");

    let frames = sealed.count_frames(&l.surface, Duration::from_millis(500));
    assert!(frames >= 10, "a shown prompt gets one frame callback per refresh: {frames}");

    close_layer(&mut sealed, l);
    wait_for(&mut sealed, |_| h.capture(0).at(640, 400) == RED);
}

/// The keys are the prompt's, a click on it reaches it, a click beside it is
/// the fullscreen window's (which does not get the keyboard for it), and the
/// window types again once the prompt closed.
#[test]
fn a_sealed_prompt_over_a_fullscreen_window_takes_the_keys_and_its_click_and_gives_the_keyboard_back() {
    let h = harness(1);
    let mut game = Session::connect(h.connect());
    game.keyboard();
    let _game_pointer = game.pointer();
    let t = fullscreen_window(&h, &mut game, "game", RED, 0);
    wait_for(&mut game, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    let mut sealed = Session::connect(h.connect());
    sealed.keyboard();
    let _sealed_pointer = sealed.pointer();
    let l = sealed_prompt(&h, &mut sealed, None);
    sealed.draw_layer(&l, BLUE);
    wait_for(&mut sealed, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    wait_for(&mut game, |c| c.state.keyboard_focus.is_none());
    press(&h, KEY_A);
    wait_for(&mut sealed, |c| c.state.keys == [KEY_A]);

    click(&h, 640.0, 400.0);
    wait_for(&mut sealed, |c| c.state.press_serial.is_some());
    game.roundtrip();
    assert_eq!(game.state.press_serial, None, "the window under the prompt took the prompt's click");

    sealed.state.press_serial = None;
    click(&h, 100.0, 100.0);
    wait_for(&mut game, |c| c.state.press_serial.is_some());
    press(&h, KEY_A);
    wait_for(&mut sealed, |c| c.state.keys == [KEY_A, KEY_A]);
    game.roundtrip();
    assert_eq!(sealed.state.press_serial, None, "the prompt took a click beside it");
    assert_eq!(sealed.state.keyboard_focus.as_ref(), Some(&l.surface), "a click on the window took the keyboard from the prompt");
    assert!(game.state.keys.is_empty() && game.state.keyboard_focus.is_none(), "the window behind the prompt got the password");

    close_layer(&mut sealed, l);
    wait_for(&mut game, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
    press(&h, KEY_A);
    wait_for(&mut game, |c| c.state.keys == [KEY_A]);
    game.state.press_serial = None;
    click(&h, 640.0, 400.0);
    wait_for(&mut game, |c| c.state.press_serial.is_some());
}

/// #EDE-143: a prompt for a request from outside the person's session
/// shows above the fullscreen window WITHOUT taking the keyboard: what the
/// person types stays in their window, until they click into the prompt.
/// It is as sealed as the other (black in a capture), and closing it gives
/// the keyboard back.
#[test]
fn a_sealed_prompt_that_waits_for_a_click_shows_above_fullscreen_and_leaves_the_keyboard() {
    let h = harness(1);
    let mut game = Session::connect(h.connect());
    game.keyboard();
    let _game_pointer = game.pointer();
    let t = fullscreen_window(&h, &mut game, "game", RED, 0);
    wait_for(&mut game, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    let mut sealed = Session::connect(h.connect());
    sealed.keyboard();
    let _sealed_pointer = sealed.pointer();
    let l = sealed_prompt_taking(&h, &mut sealed, None, KeyboardInteractivity::OnDemand);
    sealed.draw_layer(&l, BLUE);
    // It shows, above the fullscreen window.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while h.capture(0).at(640, 400) != BLUE {
        assert!(std::time::Instant::now() < deadline, "the prompt is not drawn above the fullscreen window");
        std::thread::sleep(Duration::from_millis(20));
    }
    // The person's keys stay theirs.
    press(&h, KEY_A);
    wait_for(&mut game, |c| c.state.keys == [KEY_A]);
    sealed.roundtrip();
    assert!(sealed.state.keyboard_focus.is_none() && sealed.state.keys.is_empty(), "the prompt took the keyboard by itself");

    // A click into it: the next key is the prompt's, none the window's.
    click(&h, 640.0, 400.0);
    wait_for(&mut sealed, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    press(&h, KEY_A);
    wait_for(&mut sealed, |c| c.state.keys == [KEY_A]);
    game.roundtrip();
    assert_eq!(game.state.keys, [KEY_A], "the window got a key typed into the prompt");
    assert_eq!(game.state.press_serial, None, "the window under the prompt took the prompt's click");

    close_layer(&mut sealed, l);
    wait_for(&mut game, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
}

/// The panel (top layer) and a pill (an overlay that keeps its clicks) open
/// at the same time as the prompt: neither is drawn, and a click where they
/// lie is the fullscreen window's.
#[test]
fn ordinary_layers_stay_hidden_and_unclickable_under_fullscreen_while_a_sealed_prompt_shows() {
    let h = harness(1);
    let mut shell = Session::connect(h.connect());
    let _shell_pointer = shell.pointer();
    let panel = shell.layer("panel", Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 40), 40);
    shell.draw_layer(&panel, GREEN);
    let mut game = Session::connect(h.connect());
    let _game_pointer = game.pointer();
    let _t = fullscreen_window(&h, &mut game, "game", RED, 0);
    // (490, 0) to (790, 50).
    let pill = layer_on(&mut shell, "exponential-overlay:call-pill", Layer::Overlay, None, Anchor::Top, (300, 50), KeyboardInteractivity::None);
    shell.draw_layer(&pill, YELLOW);

    let mut sealed = Session::connect(h.connect());
    let l = sealed_prompt(&h, &mut sealed, None);
    sealed.draw_layer(&l, BLUE);
    wait_for(&mut sealed, |_| h.capture(0).at(640, 400) == BLUE);
    let frame = h.capture(0);
    assert_eq!(frame.at(640, 790), RED, "the panel is drawn above a fullscreen window");
    assert_eq!(frame.at(640, 20), RED, "an ordinary overlay is drawn above a fullscreen window");

    for (x, y, what) in [(640.0, 790.0, "the hidden panel"), (640.0, 20.0, "the hidden overlay")] {
        game.state.press_serial = None;
        click(&h, x, y);
        wait_for(&mut game, |c| c.state.press_serial.is_some());
        shell.roundtrip();
        assert_eq!(shell.state.press_serial, None, "{what} took the click");
    }
    sealed.roundtrip();
    assert_eq!(sealed.state.press_serial, None);
}

/// Only `exponential-sealed` is drawn above a fullscreen window: any other
/// client's Exclusive overlay stays hidden like every layer.
#[test]
fn an_exclusive_overlay_of_another_client_stays_hidden_under_fullscreen() {
    let h = harness(1);
    let mut game = Session::connect(h.connect());
    let _game_pointer = game.pointer();
    let _t = fullscreen_window(&h, &mut game, "game", RED, 0);

    let mut other = Session::connect(h.connect());
    other.keyboard();
    let _other_pointer = other.pointer();
    let l = layer_on(&mut other, "launcher", Layer::Overlay, None, Anchor::empty(), (200, 100), KeyboardInteractivity::Exclusive);
    other.draw_layer(&l, BLUE);
    // It mapped (it holds the keyboard, as since #EDE-71).
    wait_for(&mut other, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    assert_eq!(h.capture(0).at(640, 400), RED, "another client's overlay is drawn above a fullscreen window");
    click(&h, 640.0, 400.0);
    wait_for(&mut game, |c| c.state.press_serial.is_some());
    other.roundtrip();
    assert_eq!(other.state.press_serial, None, "the hidden overlay took the click");
}

/// Two outputs, a fullscreen window on each: the prompt shows on its own
/// output; the other one draws what it drew and its clicks go where they went.
#[test]
fn with_two_outputs_only_the_prompts_output_shows_it() {
    let h = harness(2);
    let mut shell = Session::connect(h.connect());
    let _shell_pointer = shell.pointer();
    assert_eq!(shell.state.outputs.len(), 2);
    let panel = layer_on(&mut shell, "panel", Layer::Top, Some(1), Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 40), KeyboardInteractivity::None);
    shell.draw_layer(&panel, GREEN);
    wait_for(&mut shell, |_| h.capture(1).at(640, 790) == GREEN);

    let mut game = Session::connect(h.connect());
    let _game_pointer = game.pointer();
    let _g = fullscreen_window(&h, &mut game, "game", RED, 0);
    h.with(|ede| ede.pointer_motion_to((1280.0 + 640.0, 400.0).into(), 0));
    let mut video = Session::connect(h.connect());
    let _video_pointer = video.pointer();
    let _v = fullscreen_window(&h, &mut video, "video", PURPLE, 1);
    let before = h.capture(1).pixels;
    assert!(before.iter().all(|p| *p == PURPLE), "the second output shows only its fullscreen window");

    let mut sealed = Session::connect(h.connect());
    let _sealed_pointer = sealed.pointer();
    let l = sealed_prompt(&h, &mut sealed, Some(0));
    sealed.draw_layer(&l, BLUE);
    wait_for(&mut sealed, |_| h.capture(0).at(640, 400) == BLUE);
    assert_eq!(h.capture(0).at(100, 100), RED);
    assert!(h.capture(1).pixels == before, "the prompt changed the other output");

    for (x, y) in [(640.0, 400.0), (640.0, 790.0)] {
        video.state.press_serial = None;
        click(&h, 1280.0 + x, y);
        wait_for(&mut video, |c| c.state.press_serial.is_some());
    }
    for c in [&mut sealed, &mut shell, &mut game] {
        c.roundtrip();
        assert_eq!(c.state.press_serial, None, "a click on the other output's fullscreen window went elsewhere");
    }
    click(&h, 640.0, 400.0);
    wait_for(&mut sealed, |c| c.state.press_serial.is_some());
}
