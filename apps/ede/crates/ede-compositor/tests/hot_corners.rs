//! #EDE-246 headless: Cinnamon's hot corners and Show desktop.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, config::HotCorners, Config};
use ede_protocol::events;
use smithay::backend::input::ButtonState;

const BLUE: u32 = 0xff00_00ff;
const RED: u32 = 0xffff_0000;
const BTN_LEFT: u32 = 0x110;

fn harness(layout: [&str; 4]) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let config = Config { hot_corners: HotCorners { layout: layout.map(String::from).to_vec(), fullscreen: false }, ..Config::default() };
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

fn to(h: &Harness, x: f64, y: f64) {
    h.with(move |ede| {
        ede.person_pointer_motion_to((x, y).into(), 0);
    });
}

fn click(h: &Harness) {
    h.with(|ede| ede.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 0));
    h.with(|ede| ede.person_pointer_button(BTN_LEFT, ButtonState::Released, 0));
}

/// The next events' kinds (and a hotkey's name), skipping pointer noise.
fn next(shell: &mut ede_protocol::Client) -> String {
    let e = shell.next_event().unwrap();
    if e.kind == events::HOTKEY {
        format!("hotkey {}", e.data["name"].as_str().unwrap())
    } else {
        e.kind
    }
}

/// Mint's defaults: every corner off, the pointer in a corner does nothing.
#[test]
fn mints_corners_are_off() {
    let h = harness(["expo:false:0", "scale:false:0", "scale:false:0", "desktop:false:0"]);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(Config::default().hot_corners, HotCorners::default());
    to(&h, 0.0, 0.0);
    to(&h, 799.0, 599.0);
    click(&h);
    assert!(shell.next_event().is_err(), "nothing happened");
}

/// An enabled corner runs its action after its delay, not before, and not
/// once the pointer left; Expo and Scale are the shell's.
#[test]
fn hovering_an_enabled_corner_runs_it_after_its_delay() {
    let h = harness(["expo:true:0", "scale:true:300", "scale:false:0", "desktop:false:0"]);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    to(&h, 0.0, 0.0);
    assert_eq!(next(&mut shell), "hotkey expo.toggle");
    // In and out of the delayed corner within its delay: nothing.
    to(&h, 799.0, 0.0);
    to(&h, 400.0, 300.0);
    shell.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    assert!(shell.next_event().is_err(), "left before the delay");
    shell.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let at = Instant::now();
    to(&h, 799.5, 1.0);
    assert_eq!(next(&mut shell), "hotkey scale.toggle");
    assert!(at.elapsed() >= Duration::from_millis(290), "after the delay: {:?}", at.elapsed());
    // A click right after the hover ran it runs nothing more.
    click(&h);
    shell.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    assert!(shell.next_event().is_err(), "the click within 500 ms of the hover");
}

/// A click on an enabled corner runs it and stops there: the window under
/// it gets neither the press nor the focus. `desktop` toggles Show desktop:
/// every window to the taskbar, then back, the top one focused.
#[test]
fn a_corner_click_shows_the_desktop_and_brings_it_back() {
    // A long hover delay: only the click runs it.
    let h = harness(["expo:false:0", "scale:false:0", "scale:false:0", "desktop:true:60000"]);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    app.pointer();
    let a = app.toplevel("a");
    app.draw(&a, 300, 200, BLUE);
    let b = app.toplevel("b");
    app.draw(&b, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));
    let mut ipc = h.ipc();
    ipc.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    to(&h, 799.0, 599.0);
    click(&h);
    assert!(ipc.windows().unwrap().iter().all(|w| w.minimized), "every window in the taskbar");
    assert_eq!(h.with(|ede| ede.desk.space.elements().count()), 0);

    click(&h);
    let windows = ipc.windows().unwrap();
    assert!(windows.iter().all(|w| !w.minimized), "back: {windows:?}");
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));

    // Raised by the person in between: the desktop no longer shows, the
    // toggle hides everything again (Muffin). The corner bar's way in:
    // ede-protocol's toggle_desktop.
    ipc.toggle_desktop().unwrap();
    assert!(ipc.windows().unwrap().iter().all(|w| w.minimized));
    ipc.toggle_desktop().unwrap();
    let id_a = ipc.windows().unwrap().into_iter().find(|w| w.title.as_deref() == Some("a")).unwrap().id;
    ipc.focus(id_a).unwrap();
    h.with(|ede| ede.toggle_desktop());
    assert!(ipc.windows().unwrap().iter().all(|w| w.minimized), "hidden again, a included");
}
