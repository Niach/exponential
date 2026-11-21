//! #EXP-2024 (6c) headless: the compositor's two lines for the shell's live
//! view of an agent space. A person-scope capture of a space (the shell
//! watching it) is not use: the space's clock stays slow. The view's layer
//! (the namespace `exponential-shell:space-view` from the shell's process)
//! is black in every capture of the person's output, so a space's pixels
//! never reach a person-seat screenshot, screen memory or a screen share
//! through it. And a top layer that asks for no keyboard (the view) never
//! moves the person's keyboard when it maps, is clicked, resized or goes.

mod common;

use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::{space::start_with_spaces, Session};
use ede_compositor::{backend::headless::Harness, spaces::frames::{FAST, SLOW}, Config};
use ede_protocol::{Capability, CaptureTarget, SessionId, SPACE_VIEW_NAMESPACE};
use smithay::backend::input::{ButtonState, KeyState};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity};

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;
// evdev code; xkb = evdev + 8
const KEY_A: u32 = 30;
const KEY_B: u32 = 48;

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, &[(1280, 800)])
}

fn press(h: &Harness, code: u32) {
    h.with(move |ede| {
        ede.keyboard_key((code + 8).into(), KeyState::Pressed, 0);
        ede.keyboard_key((code + 8).into(), KeyState::Released, 0);
    });
}

fn click(h: &Harness, x: f64, y: f64) {
    h.with(move |ede| {
        ede.pointer_motion_to((x, y).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
}

/// The pinned view as the shell maps it: a top layer at the bottom right
/// with no keyboard, the test's own process standing in for the shell
/// while it maps (a layer's origin is decided once, then).
fn space_view(h: &Harness, s: &mut Session, size: (u32, u32)) -> common::Layer {
    let pid = std::process::id();
    h.with(move |ede| ede.shell_stand_in = Some(pid));
    let l = s.layer(SPACE_VIEW_NAMESPACE, Anchor::Bottom | Anchor::Right, size, 0);
    h.with(|ede| ede.shell_stand_in = None);
    l
}

/// Section 8, 6c: a watched space's clock stays at 100 ms: the shell's
/// captures (the root grant, a person scope) are not use; its own agent's
/// capture is.
#[test]
fn a_watched_spaces_clock_stays_at_100_ms() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, id, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        let space = state.spaces.get(desk).unwrap();
        (desk, space.id.clone(), space.socket())
    });
    let mut b = Session::connect(UnixStream::connect(socket).expect("the space's socket"));
    let window = b.toplevel("b");
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    b.draw_configured(&window, 64, 64, RED);
    let pace = move |h: &Harness| h.with(move |state| state.spaces.get(desk).map(|s| s.pace).unwrap());

    // The live view at its rate: a capture every 250 ms while the app animates.
    let mut root = h.ipc();
    let mut since = None;
    for _ in 0..6 {
        let frame = root.capture_since(CaptureTarget::Space { space: id.clone() }, since).expect("a watcher's capture");
        since = frame.map(|f| f.info.serial).or(since);
        assert!(b.count_frames(&window.surface, Duration::from_millis(250)) > 0, "the app's callbacks go on");
        assert_eq!(pace(&h), SLOW, "watching sped the space up");
    }

    // Its agent's own capture (its space grant) is use: fast.
    let token = h.with(|state| state.ipc.mint_for("run-1", vec![Capability::Space]).token);
    h.ipc_with(token).unwrap().capture(CaptureTarget::Space { space: id }).expect("the agent's capture");
    assert!(b.count_frames(&window.surface, Duration::from_millis(250)) > 0);
    assert_eq!(pace(&h), FAST);
}

/// Section 8, 6c: a capture of the person's output with the view mapped
/// shows black where it is (what the screen shows has it); a layer of
/// another client that names the namespace is no view and stays in it.
#[test]
fn the_space_view_is_black_in_a_capture_of_the_persons_output() {
    let h = harness();
    let mut shell = Session::connect(h.connect());
    let view = space_view(&h, &mut shell, (400, 300));
    shell.draw_layer(&view, GREEN);
    // Bottom right of the 1280×800 output: x 880..1280, y 500..800.
    wait_for(&mut shell, |_| h.capture(0).at(1080, 650) == GREEN);
    let shared = h.capture_shared(0);
    assert_eq!((shared.at(881, 501), shared.at(1080, 650), shared.at(1279, 799)), (BLACK, BLACK, BLACK), "the view leaked into a capture");
    assert_ne!(shared.at(600, 400), BLACK, "the rest of the screen is there");

    // Not the shell: the same namespace does not hide another client.
    let mut other = Session::connect(h.connect());
    let impostor = other.layer(SPACE_VIEW_NAMESPACE, Anchor::Top | Anchor::Left, (200, 100), 0);
    other.draw_layer(&impostor, BLUE);
    wait_for(&mut other, |_| h.capture(0).at(100, 50) == BLUE);
    assert_eq!(h.capture_shared(0).at(100, 50), BLUE, "only the shell's own view is black");
}

/// Section 8, 6c: a keyboard-none top layer that maps, is clicked, resized
/// and unmapped leaves the focused window focused, and every key reaches it.
#[test]
fn a_keyboard_none_top_layer_leaves_the_focused_window_focused() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("terminal");
    app.draw(&t, 400, 300, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    let mut shell = Session::connect(h.connect());
    shell.keyboard();
    shell.pointer();
    let view = space_view(&h, &mut shell, (400, 300));
    shell.draw_layer(&view, GREEN);
    wait_for(&mut shell, |_| h.capture(0).at(1080, 650) == GREEN);
    let keys = |app: &mut Session, sent: &[u32]| {
        wait_for(app, |c| c.state.keys == sent);
        assert_eq!(app.state.keyboard_focus.as_ref(), Some(&t.surface));
    };
    press(&h, KEY_A);
    keys(&mut app, &[KEY_A]);

    // Clicked (the size or the close button): the click is the view's, the keyboard stays.
    click(&h, 1080.0, 650.0);
    wait_for(&mut shell, |c| c.state.press_serial.is_some());
    press(&h, KEY_B);
    keys(&mut app, &[KEY_A, KEY_B]);

    // Resized (the size button): twice as wide.
    view.layer.set_size(800, 300);
    view.surface.commit();
    shell.roundtrip();
    shell.draw_layer(&view, GREEN);
    wait_for(&mut shell, |_| h.capture(0).at(600, 650) == GREEN);
    press(&h, KEY_A);
    keys(&mut app, &[KEY_A, KEY_B, KEY_A]);

    // Unmapped (closed): nothing to give back, nothing moves.
    view.layer.destroy();
    view.surface.destroy();
    shell.roundtrip();
    wait_for(&mut shell, |_| h.capture(0).at(1080, 650) != GREEN);
    press(&h, KEY_B);
    keys(&mut app, &[KEY_A, KEY_B, KEY_A, KEY_B]);
    shell.roundtrip();
    assert!(shell.state.keys.is_empty() && shell.state.keyboard_focus.is_none(), "the view's client took the keyboard");
}

/// The shell's binary replaced while it runs (a package upgrade, a rebuild)
/// reads `exponential-shell (deleted)` in /proc: still the shell, so a view
/// pinned after an upgrade stays black in captures.
#[test]
fn a_replaced_binary_keeps_its_name() {
    use ede_compositor::sealed::exe_name;
    use std::path::Path;
    assert_eq!(exe_name(Path::new("/usr/bin/exponential-shell")).as_deref(), Some("exponential-shell"));
    assert_eq!(exe_name(Path::new("/usr/bin/exponential-shell (deleted)")).as_deref(), Some("exponential-shell"));
    assert_eq!(exe_name(Path::new("/usr/libexec/exponential-sealed (deleted)")).as_deref(), Some("exponential-sealed"));
    assert_eq!(exe_name(Path::new("/tmp/exponential-shell-impostor")).as_deref(), Some("exponential-shell-impostor"));
}
