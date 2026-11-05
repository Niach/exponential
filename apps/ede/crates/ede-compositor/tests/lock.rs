//! #NEW-106 headless: the session lock (with a placeholder lock client),
//! idle, input methods and protected surfaces.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, protected::Refused, Config};
use ede_protocol::{Error, ErrorCode};
use smithay::backend::input::{ButtonState, KeyState};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;

// evdev codes; xkb = evdev + 8
const KEY_A: u32 = 30;
const KEY_LEFTALT: u32 = 56 + 8;
const KEY_F4: u32 = 62 + 8;

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

fn key(h: &Harness, code: u32, state: KeyState) {
    h.with(move |ede| ede.keyboard_key(code.into(), state, 0));
}

fn press(h: &Harness, code: u32) {
    key(h, code, KeyState::Pressed);
    key(h, code, KeyState::Released);
}

/// The centre of the only (or topmost) window.
fn window_center(h: &Harness) -> (i32, i32) {
    h.with(|ede| {
        let w = ede.space.elements().last().unwrap();
        let g = ede.space.element_geometry(w).unwrap();
        (g.loc.x + g.size.w / 2, g.loc.y + g.size.h / 2)
    })
}

fn code(e: Error) -> ErrorCode {
    match e {
        Error::Failed(f) => f.code,
        other => panic!("expected a failure, got {other}"),
    }
}

fn is_locked(h: &Harness) -> bool {
    h.with(|ede| ede.is_locked())
}

#[test]
fn the_globals_are_advertised() {
    let h = harness(Config::default());
    let client = Session::connect(h.connect());
    for global in [
        "ext_session_lock_manager_v1",
        "ext_idle_notifier_v1",
        "zwp_idle_inhibit_manager_v1",
        "zwp_input_method_manager_v2",
        "zwp_text_input_manager_v3",
        "zwp_virtual_keyboard_manager_v1",
    ] {
        assert!(client.state.globals.iter().any(|g| g == global), "{global} missing");
    }
}

#[test]
fn locked_the_screen_shows_only_the_lock_client_and_only_it_gets_keys() {
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let (x, y) = window_center(&h);
    assert_eq!(h.capture(0).at(x, y), RED);

    let mut locker = Session::connect(h.connect());
    locker.keyboard();
    let lock = locker.lock();
    assert!(is_locked(&h));
    // Black at once, before the lock client drew anything; `locked` follows
    // the first frame on every output.
    assert_eq!(h.capture(0).at(x, y), BLACK);
    wait_for(&mut locker, |c| c.state.locked);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_none());

    let surfaces = locker.lock_surfaces(&lock);
    assert_eq!(locker.state.lock_surfaces[0].size, (800, 600), "a lock surface covers its output");
    locker.draw_lock(&surfaces[0], BLUE);
    assert_eq!(h.capture(0).at(x, y), BLUE);
    assert_eq!(h.capture(0).at(5, 5), BLUE);
    // Captures never see the lock screen (nor anything behind it).
    assert_eq!(h.capture_shared(0).at(x, y), BLACK);

    wait_for(&mut locker, |c| c.state.keyboard_focus.is_some());
    press(&h, KEY_A + 8);
    locker.roundtrip();
    app.roundtrip();
    assert_eq!(locker.state.keys, [KEY_A]);
    assert!(app.state.keys.is_empty(), "the app behind the lock got a key");

    // Clicking where the window is focuses nothing behind the lock.
    h.with(move |ede| {
        ede.pointer_motion_to((x as f64, y as f64).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
    // Compositor bindings are off: Alt+F4 does not close the app.
    key(&h, KEY_LEFTALT, KeyState::Pressed);
    press(&h, KEY_F4);
    key(&h, KEY_LEFTALT, KeyState::Released);
    app.roundtrip();
    assert!(!app.state.toplevels[0].closed);

    lock.unlock_and_destroy();
    locker.roundtrip();
    assert!(!is_locked(&h));
    assert_eq!(h.capture(0).at(x, y), RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    press(&h, KEY_A + 8);
    app.roundtrip();
    assert_eq!(app.state.keys, [KEY_A]);
}

#[test]
fn a_dead_lock_client_leaves_the_session_locked_until_a_new_one_unlocks() {
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    let (x, y) = window_center(&h);

    let mut locker = Session::connect(h.connect());
    let lock = locker.lock();
    wait_for(&mut locker, |c| c.state.locked);
    let surfaces = locker.lock_surfaces(&lock);
    locker.draw_lock(&surfaces[0], BLUE);
    drop((surfaces, lock, locker)); // the lock client crashes

    std::thread::sleep(Duration::from_millis(100));
    assert!(is_locked(&h));
    assert_eq!(h.capture(0).at(x, y), BLACK, "the desktop shows after the lock client died");

    let mut second = Session::connect(h.connect());
    let lock = second.lock();
    second.roundtrip();
    assert!(second.state.locked, "the new lock client takes over at once");
    lock.unlock_and_destroy();
    second.roundtrip();
    assert!(!is_locked(&h));
    assert_eq!(h.capture(0).at(x, y), RED);
}

#[test]
fn a_second_lock_client_is_refused_and_cannot_unlock() {
    let h = harness(Config::default());
    let mut first = Session::connect(h.connect());
    let _lock = first.lock();
    wait_for(&mut first, |c| c.state.locked);

    let mut intruder = Session::connect(h.connect());
    let lock = intruder.lock();
    intruder.roundtrip();
    assert!(intruder.state.finished && !intruder.state.locked);
    lock.unlock_and_destroy();
    let _ = intruder.queue.roundtrip(&mut intruder.state); // a protocol error ends it
    assert!(is_locked(&h), "a refused lock object unlocked the session");
}

#[test]
fn protected_windows_are_never_captured_or_driven() {
    let mut config = Config::default();
    config.protected.app_ids = vec!["org.example.Bank".into()];
    let h = harness(config);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let normal = app.toplevel("notes");
    app.draw(&normal, 200, 200, BLUE);
    // A corner the next window (cascaded down-right) leaves uncovered.
    let (nx, ny) = h.with(|ede| {
        let g = ede.space.element_geometry(ede.space.elements().last().unwrap()).unwrap();
        (g.loc.x + 5, g.loc.y + 5)
    });

    for app_id in ["org.keepassxc.KeePassXC", "org.example.Bank"] {
        let secret = app.toplevel(app_id);
        secret.toplevel.set_app_id(app_id.into());
        app.draw(&secret, 200, 200, RED);
        let (sx, sy) = window_center(&h);
        assert_ne!((sx, sy), (nx, ny));
        assert_eq!(h.capture(0).at(sx, sy), RED, "the person sees it");
        let shared = h.capture_shared(0);
        assert_eq!(shared.at(sx, sy), BLACK, "{app_id} leaked into a capture");
        assert_eq!(shared.at(nx, ny), BLUE, "other windows still show");

        // It has keyboard focus (it mapped last): agent keys are refused,
        // the person's go through.
        wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&secret.surface));
        let refused = h.with(|ede| ede.drive_key((KEY_A + 8).into(), KeyState::Pressed, 0, false));
        assert_eq!(refused, Err(Refused::Protected));
        let refused = h.with(move |ede| ede.drive_pointer_motion((sx as f64, sy as f64).into(), 0));
        assert_eq!(refused, Err(Refused::Protected));
        assert_eq!(h.with(move |ede| ede.drive_pointer_motion((nx as f64, ny as f64).into(), 0)), Ok(()));
        let lendable = h.with(|ede| ede.space.elements().map(|w| ede.may_lend(w)).collect::<Vec<_>>());
        assert_eq!(lendable.iter().filter(|l| !**l).count(), 1);

        secret.toplevel.destroy();
        secret.xdg_surface.destroy();
        secret.surface.destroy();
        app.roundtrip();
        std::thread::sleep(Duration::from_millis(50));
    }

    // A normal focused window may be driven.
    h.with(|ede| {
        let w = ede.space.elements().last().cloned();
        ede.focus(w);
    });
    app.roundtrip();
    assert_eq!(h.with(|ede| ede.drive_key((KEY_A + 8).into(), KeyState::Pressed, 0, false)), Ok(()));
    app.roundtrip();
    assert_eq!(app.state.keys.last(), Some(&KEY_A));

    // Locked: nothing may be driven.
    let mut locker = Session::connect(h.connect());
    let _lock = locker.lock();
    assert_eq!(h.with(|ede| ede.drive_key((KEY_A + 8).into(), KeyState::Pressed, 0, false)), Err(Refused::Locked));
}

#[test]
fn a_virtual_keyboard_cannot_type_into_a_protected_window() {
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let mut typist = Session::connect(h.connect());
    typist.keyboard();
    let vk = typist.virtual_keyboard();

    let notes = app.toplevel("notes");
    app.draw(&notes, 200, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&notes.surface));
    vk.key(0, KEY_A, 1);
    typist.roundtrip();
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);

    let secret = app.toplevel("pinentry");
    secret.toplevel.set_app_id("pinentry-gnome3".into());
    app.draw(&secret, 200, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&secret.surface));
    vk.key(1, KEY_A, 1);
    typist.roundtrip();
    std::thread::sleep(Duration::from_millis(50));
    app.roundtrip();
    assert_eq!(app.state.keys, [KEY_A], "the virtual keyboard typed into a password prompt");
}

#[test]
fn idle_runs_the_lock_command_unless_inhibited() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("locked");
    let mut config = Config::default();
    config.idle.lock_after = 1;
    config.lock.command = vec!["sh".into(), "-c".into(), format!("touch {}", marker.display())];

    // Inhibited by a visible window: no lock.
    let h = harness(config.clone());
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("video");
    app.draw(&t, 200, 200, BLUE);
    let _inhibitor = app.inhibit_idle(&t.surface);
    std::thread::sleep(Duration::from_millis(2500));
    assert!(!marker.exists(), "locked while a video inhibited idle");
    drop(h);

    let h = harness(config);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "the idle lock never ran");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(h);
}

/// #EDE-17: the sealed lock. The test's own process plays `ede-sealed`
/// (the compositor trusts the pid it spawned): its overlay layer surface
/// is the lock screen, windows vanish from the output and from captures,
/// only the grant minted for the lock client unlocks.
#[test]
fn the_sealed_lock_shows_only_the_lock_clients_surface_and_its_grant_unlocks() {
    use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let (cx, cy) = window_center(&h);
    assert_eq!(h.capture(0).at(cx, cy), RED);

    let mut events = h.ipc();
    events.subscribe().unwrap();
    events.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let pid = std::process::id();
    let token = h.with(move |ede| ede.begin_sealed_lock_as(pid));
    assert_eq!(events.next_event().unwrap().kind, ede_protocol::events::SESSION_LOCKED);
    assert!(is_locked(&h));
    // Black until the lock client draws, the app gone.
    assert_eq!(h.capture(0).at(cx, cy), 0xff00_0000);
    assert_eq!(h.capture_shared(0).at(cx, cy), 0xff00_0000, "a capture never sees the lock");

    // The lock client (this process) draws an overlay layer surface.
    let mut locker = Session::connect(h.connect());
    let l = locker.layer("ede-sealed", Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 0), -1);
    locker.draw_layer(&l, BLUE);
    wait_for(&mut locker, |_| h.capture(0).at(cx, cy) == BLUE);
    assert_eq!(h.capture_shared(0).at(cx, cy), 0xff00_0000, "still black for captures");
    // Keys go to the lock surface, never to the app.
    locker.keyboard();
    wait_for(&mut locker, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    press(&h, KEY_A);
    wait_for(&mut locker, |c| !c.state.keys.is_empty());
    app.roundtrip();
    assert!(app.state.keys.is_empty(), "the app heard nothing");

    // The root grant cannot unlock; the lock client's can.
    let mut root = h.ipc();
    assert_eq!(code(root.unlock().unwrap_err()), ErrorCode::Forbidden);
    let mut lock_client = h.ipc_with(token).unwrap();
    lock_client.unlock().unwrap();
    assert_eq!(events.next_event().unwrap().kind, ede_protocol::events::SESSION_UNLOCKED);
    assert!(!is_locked(&h));
    l.layer.destroy();
    l.surface.destroy();
    locker.roundtrip();
    wait_for(&mut app, |_| h.capture(0).at(cx, cy) == RED);
    // The grant was single-use: it is gone.
    assert!(matches!(lock_client.windows(), Err(ede_protocol::Error::Failed(_))));
}
