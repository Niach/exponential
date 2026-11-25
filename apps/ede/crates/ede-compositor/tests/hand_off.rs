//! #EDE-224 headless: a hand-off to the Exponential app. The app takes the
//! link on its own socket, without a launch token, so its new window opens
//! behind the focused one (#EDE-76); the shell then focuses it from the
//! root connection, and it comes on top and activated, also when it opens
//! maximized and a fullscreen window was up.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use wayland_protocols::xdg::shell::client::xdg_toplevel::State as XdgState;

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, &[(1280, 800)])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

/// The windows' titles, back to front, with which one is focused.
fn stack(h: &Harness) -> Vec<(String, bool)> {
    h.ipc().windows().unwrap().into_iter().map(|w| (w.title.unwrap_or_default(), w.focused)).collect()
}

/// The shell's press: `focus_window` on `title` from the root connection.
fn shell_focuses(h: &Harness, title: &str) {
    let mut ipc = h.ipc();
    let id = ipc.windows().unwrap().into_iter().find(|w| w.title.as_deref() == Some(title)).expect("the window is listed").id;
    ipc.focus(id).unwrap();
}

/// Client A with a focused 400×300 window, client B (another connection,
/// no token) mapping a window later: it stays behind.
fn a_focused_then_b(h: &Harness) -> (Session, common::Toplevel, Session, common::Toplevel) {
    let mut a = Session::connect(h.connect());
    a.keyboard();
    let ta = a.toplevel("game");
    a.draw(&ta, 400, 300, RED);
    wait_for(&mut a, |c| c.state.keyboard_focus.as_ref() == Some(&ta.surface));

    let mut b = Session::connect(h.connect());
    b.keyboard();
    let tb = b.toplevel("Exponential");
    b.draw(&tb, 400, 300, BLUE);
    wait_for(&mut b, |_| stack(h).iter().any(|(t, _)| t == "Exponential"));
    b.roundtrip();
    assert_eq!(stack(h), [("Exponential".to_string(), false), ("game".to_string(), true)], "the token-less window opens behind");
    assert_eq!(b.state.keyboard_focus, None);
    assert!(!b.state.toplevels[0].states.contains(&XdgState::Activated));
    (a, ta, b, tb)
}

#[test]
fn a_window_opened_behind_comes_on_top_when_the_shell_focuses_it() {
    let h = harness();
    let (mut a, _ta, mut b, tb) = a_focused_then_b(&h);
    assert_eq!(h.capture(0).at(640, 400), RED);

    shell_focuses(&h, "Exponential");
    wait_for(&mut b, |c| c.state.keyboard_focus.as_ref() == Some(&tb.surface) && c.state.toplevels[0].states.contains(&XdgState::Activated));
    assert_eq!(stack(&h), [("game".to_string(), false), ("Exponential".to_string(), true)]);
    // Drawn on top once it acks the activated configure.
    b.draw_configured(&tb, 400, 300, BLUE);
    wait_for(&mut b, |_| h.capture(0).at(640, 400) == BLUE);
    a.roundtrip();
    assert!(!a.state.toplevels[0].states.contains(&XdgState::Activated));
}

/// The app opens maximized; the focused window was fullscreen (a game).
#[test]
fn a_maximized_window_behind_a_fullscreen_one_comes_on_top_when_the_shell_focuses_it() {
    let h = harness();
    let (mut a, ta, mut b, tb) = a_focused_then_b(&h);
    ta.toplevel.set_fullscreen(None);
    a.roundtrip();
    wait_for(&mut a, |c| c.state.toplevels[0].states.contains(&XdgState::Fullscreen));
    a.draw_configured(&ta, 0, 0, RED);
    wait_for(&mut a, |_| {
        let f = h.capture(0);
        (f.at(0, 0), f.at(1279, 799)) == (RED, RED)
    });

    tb.toplevel.set_maximized();
    b.roundtrip();
    wait_for(&mut b, |c| c.state.toplevels[0].states.contains(&XdgState::Maximized));
    b.draw_configured(&tb, 0, 0, BLUE);
    b.roundtrip();
    assert_eq!(h.capture(0).at(640, 400), RED, "the game stays in front until the shell focuses the app");

    shell_focuses(&h, "Exponential");
    wait_for(&mut b, |c| c.state.keyboard_focus.as_ref() == Some(&tb.surface) && c.state.toplevels[0].states.contains(&XdgState::Activated));
    b.draw_configured(&tb, 0, 0, BLUE);
    wait_for(&mut b, |_| {
        let f = h.capture(0);
        (f.at(640, 400), f.at(1279, 799)) == (BLUE, BLUE)
    });
    assert_eq!(stack(&h), [("game".to_string(), false), ("Exponential".to_string(), true)]);
}
