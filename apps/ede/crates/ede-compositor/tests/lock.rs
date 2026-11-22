//! #NEW-106 headless: the session lock (with a placeholder lock client),
//! idle, input methods and protected surfaces.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{
    backend::headless::Harness,
    desk::DeskId,
    lock::LockState,
    logind::{self, Held, Listener, Signal},
    protected::Refused,
    sealed::{Sealed, HEALTHY, MAX_RESPAWNS, NO_SURFACE},
    Config,
};
use ede_protocol::{Error, ErrorCode};
use smithay::backend::input::{ButtonState, KeyState};
use smithay::reexports::wayland_server::Resource as _;

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
        let w = ede.desk.space.elements().last().unwrap();
        let g = ede.desk.space.element_geometry(w).unwrap();
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

    // #EDE-170: a second copy of the lock client's socket, the way a process
    // another test spawns holds one from its fork to its exec. The compositor
    // hears the hang-up only once the last copy closes, so it comes late.
    let stream = h.connect();
    let copy = stream.try_clone().unwrap();
    let mut locker = Session::connect(stream);
    let lock = locker.lock();
    wait_for(&mut locker, |c| c.state.locked);
    let surfaces = locker.lock_surfaces(&lock);
    locker.draw_lock(&surfaces[0], BLUE);
    drop((surfaces, lock, locker)); // the lock client crashes
    let late = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(copy);
    });

    // Until the compositor has heard the client go, the client's last frame
    // is what shows (it is alive as far as anyone can tell). Wait for the
    // death itself, never a guessed time: a late hang-up (a starved runner,
    // a copy of the socket) outlasts any.
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(|ede| matches!(&ede.lock, LockState::Locked(l) if l.lock.is_alive())) {
        assert!(Instant::now() < deadline, "the compositor never heard the lock client go");
        std::thread::sleep(Duration::from_millis(10));
    }
    late.join().unwrap();
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
        let g = ede.desk.space.element_geometry(ede.desk.space.elements().last().unwrap()).unwrap();
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
        let refused = h.with(|ede| ede.drive_key(DeskId::PERSON, (KEY_A + 8).into(), KeyState::Pressed, 0, false));
        assert_eq!(refused, Err(Refused::Protected));
        let refused = h.with(move |ede| ede.drive_pointer_motion(DeskId::PERSON, (sx as f64, sy as f64).into(), 0));
        assert_eq!(refused, Err(Refused::Protected));
        assert_eq!(h.with(move |ede| ede.drive_pointer_motion(DeskId::PERSON, (nx as f64, ny as f64).into(), 0)), Ok(()));
        let lendable = h.with(|ede| ede.desk.space.elements().map(|w| ede.may_lend(w)).collect::<Vec<_>>());
        assert_eq!(lendable.iter().filter(|l| !**l).count(), 1);

        secret.toplevel.destroy();
        secret.xdg_surface.destroy();
        secret.surface.destroy();
        app.roundtrip();
        std::thread::sleep(Duration::from_millis(50));
    }

    // A normal focused window may be driven.
    h.with(|ede| {
        let w = ede.desk.space.elements().last().cloned();
        ede.focus(DeskId::PERSON, w);
    });
    app.roundtrip();
    assert_eq!(h.with(|ede| ede.drive_key(DeskId::PERSON, (KEY_A + 8).into(), KeyState::Pressed, 0, false)), Ok(()));
    app.roundtrip();
    assert_eq!(app.state.keys.last(), Some(&KEY_A));

    // Locked: nothing may be driven.
    let mut locker = Session::connect(h.connect());
    let _lock = locker.lock();
    assert_eq!(h.with(|ede| ede.drive_key(DeskId::PERSON, (KEY_A + 8).into(), KeyState::Pressed, 0, false)), Err(Refused::Locked));
}

/// #EDE-121: a sealed prompt (polkit, consent, share) on the plain desktop
/// is the person's to see and black in everything else: a shared frame, a
/// capture over the protocol, a `since` capture; a window's preview never
/// has it.
#[test]
fn a_sealed_prompt_on_the_desktop_is_black_in_every_capture() {
    use ede_protocol::CaptureTarget;
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    const GREEN: u32 = 0xff00_ff00;
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("notes");
    app.draw(&t, 600, 400, RED);
    let (cx, cy) = {
        let frame = h.capture(0);
        (frame.width / 2, frame.height / 2)
    };
    // A corner of the window the prompt (200×100, centred) leaves uncovered.
    let (wx, wy) = h.with(|ede| {
        let g = ede.desk.space.element_geometry(ede.desk.space.elements().last().unwrap()).unwrap();
        (g.loc.x + 5, g.loc.y + 5)
    });
    assert!(wx < cx - 100 && wy < cy - 50);

    // The test's own process plays `exponential-sealed` while the prompt maps.
    let mut sealed = Session::connect(h.connect());
    let pid = std::process::id();
    h.with(move |ede| ede.sealed_stand_in = Some(pid));
    let l = sealed.keyboard_layer("polkit", Layer::Overlay, (200, 100), KeyboardInteractivity::Exclusive);
    h.with(|ede| ede.sealed_stand_in = None);
    sealed.draw_layer(&l, BLUE);
    // Any other client's overlay is captured as drawn: (0, 0) to (40, 40).
    let mut shell = Session::connect(h.connect());
    let toast = shell.layer("toast", wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor::Top | wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor::Left, (40, 40), 0);
    shell.draw_layer(&toast, GREEN);
    wait_for(&mut sealed, |_| {
        let frame = h.capture(0);
        (frame.at(cx, cy), frame.at(5, 5)) == (BLUE, GREEN)
    });
    assert_eq!(h.capture(0).at(wx, wy), RED);

    let prompt = [(cx - 100, cy - 50), (cx, cy), (cx + 99, cy + 49)];
    let shared = h.capture_shared(0);
    for (x, y) in prompt {
        assert_eq!(shared.at(x, y), BLACK, "the prompt leaked into a shared frame at ({x}, {y})");
    }
    assert_eq!((shared.at(wx, wy), shared.at(5, 5)), (RED, GREEN), "everything else still shows");

    let mut ipc = h.ipc();
    let output = CaptureTarget::Output { name: "HEADLESS-1".into() };
    let shot = ipc.capture(output.clone()).unwrap();
    let since = ipc.capture_since(output, None).unwrap().expect("a first capture has pixels");
    for (x, y) in prompt {
        assert_eq!(shot.at(x, y), BLACK, "the prompt leaked into a capture at ({x}, {y})");
        assert_eq!(since.at(x, y), BLACK, "the prompt leaked into a since capture at ({x}, {y})");
    }
    assert_eq!((shot.at(wx, wy), since.at(wx, wy), shot.at(5, 5)), (RED, RED, GREEN));

    let id = ipc.windows().unwrap()[0].id.clone();
    let preview = ipc.capture(CaptureTarget::Window { window: id }).unwrap();
    assert_eq!((preview.info.width, preview.info.height), (600, 400));
    assert_eq!((preview.at(300, 200), preview.at(5, 5)), (RED, RED), "a window's preview is the window alone");
}

/// #EDE-140: each key is let go before the next, so the seat never absorbs
/// a press as already held, and the prompt's going is the control: the
/// same key then reaches the window below.
#[test]
fn a_virtual_keyboard_cannot_type_into_a_protected_window() {
    const KEY_B: u32 = 48;
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let mut typist = Session::connect(h.connect());
    typist.keyboard();
    let vk = typist.virtual_keyboard();
    let mut time = 0;
    let mut tap = |typist: &mut Session, key: u32| {
        for state in [1, 0] {
            vk.key(time, key, state);
            time += 1;
        }
        typist.roundtrip();
    };

    let notes = app.toplevel("notes");
    app.draw(&notes, 200, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&notes.surface));
    tap(&mut typist, KEY_A);
    wait_for(&mut app, |c| (c.state.keys.as_slice(), c.state.key_releases.as_slice()) == (&[KEY_A][..], &[KEY_A][..]));

    let secret = app.toplevel("pinentry");
    secret.toplevel.set_app_id("pinentry-gnome3".into());
    app.draw(&secret, 200, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&secret.surface));
    tap(&mut typist, KEY_B);
    std::thread::sleep(Duration::from_millis(50));
    app.roundtrip();
    assert_eq!(app.state.keys, [KEY_A], "the virtual keyboard typed into a password prompt");

    secret.toplevel.destroy();
    secret.xdg_surface.destroy();
    secret.surface.destroy();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&notes.surface));
    tap(&mut typist, KEY_B);
    wait_for(&mut app, |c| c.state.keys == [KEY_A, KEY_B]);
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

/// #EDE-17: the sealed lock. The test's own process plays `exponential-sealed`
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
    let l = locker.layer("exponential-sealed", Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 0), -1);
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

/// #EDE-71: a prompt (`exponential-sealed polkit`, consent, share) maps an
/// Exclusive overlay. The password typed right away is the prompt's, never
/// the window's behind it, and the window types again once it closed.
#[test]
fn an_exclusive_overlay_takes_the_keyboard_as_it_maps_and_gives_it_back() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("terminal");
    app.draw(&t, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    let mut prompt = Session::connect(h.connect());
    prompt.keyboard();
    let l = prompt.keyboard_layer("polkit", Layer::Overlay, (200, 100), KeyboardInteractivity::Exclusive);
    // Not before it drew: a surface without a buffer shows nothing to type into.
    app.roundtrip();
    assert_eq!(app.state.keyboard_focus.as_ref(), Some(&t.surface));
    prompt.draw_layer(&l, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_none());
    wait_for(&mut prompt, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    press(&h, KEY_A + 8);
    wait_for(&mut prompt, |c| c.state.keys == [KEY_A]);
    app.roundtrip();
    assert!(app.state.keys.is_empty(), "the window behind the prompt got the password");

    l.layer.destroy();
    l.surface.destroy();
    prompt.roundtrip();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
    press(&h, KEY_A + 8);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
}

/// #EDE-71: a shell surface that took the keyboard on a click (Settings,
/// chat, history, connect are on-demand layers) hands it back as it closes.
#[test]
fn closing_a_focused_layer_gives_the_keyboard_back_to_the_window() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    let mut shell = Session::connect(h.connect());
    shell.keyboard();
    let l = shell.keyboard_layer("settings", Layer::Top, (200, 100), KeyboardInteractivity::OnDemand);
    shell.draw_layer(&l, BLUE);
    // On-demand never takes the keyboard by itself.
    app.roundtrip();
    assert_eq!(app.state.keyboard_focus.as_ref(), Some(&t.surface));
    wait_for(&mut shell, |_| h.capture(0).at(400, 300) == BLUE);
    h.with(|ede| {
        ede.pointer_motion_to((400.0, 300.0).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
    wait_for(&mut shell, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));

    l.layer.destroy();
    l.surface.destroy();
    shell.roundtrip();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
    press(&h, KEY_A + 8);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
}

/// #EDE-71: a prompt over a focused shell surface (Settings asks for the
/// password) gives the keyboard back to that surface, not to a window.
#[test]
fn a_prompt_over_a_focused_layer_gives_the_keyboard_back_to_that_layer() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    let mut shell = Session::connect(h.connect());
    shell.keyboard();
    let settings = shell.keyboard_layer("settings", Layer::Top, (200, 100), KeyboardInteractivity::OnDemand);
    shell.draw_layer(&settings, BLUE);
    wait_for(&mut shell, |_| h.capture(0).at(400, 300) == BLUE);
    h.with(|ede| {
        ede.pointer_motion_to((400.0, 300.0).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
    wait_for(&mut shell, |c| c.state.keyboard_focus.as_ref() == Some(&settings.surface));

    let mut prompt = Session::connect(h.connect());
    prompt.keyboard();
    let l = prompt.keyboard_layer("polkit", Layer::Overlay, (100, 50), KeyboardInteractivity::Exclusive);
    prompt.draw_layer(&l, BLACK);
    wait_for(&mut prompt, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    l.layer.destroy();
    l.surface.destroy();
    prompt.roundtrip();
    wait_for(&mut shell, |c| c.state.keyboard_focus.as_ref() == Some(&settings.surface));
    press(&h, KEY_A + 8);
    wait_for(&mut shell, |c| c.state.keys == [KEY_A]);
    app.roundtrip();
    assert!(app.state.keys.is_empty());
}

/// #EDE-77: a prompt that mapped while the session was locked (polkit asks
/// behind the lock screen) has the keyboard once it is unlocked: the
/// password is not the window's behind it.
fn a_prompt_behind_the_lock_gets_the_keyboard_at_unlock(sealed: bool) {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness(Config::default());
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("terminal");
    app.draw(&t, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));

    // Sealed: pid 1 is the lock client, so nothing of this process is.
    let mut locker = Session::connect(h.connect());
    let token = sealed.then(|| h.with(|ede| ede.begin_sealed_lock_as(1)));
    let lock = (!sealed).then(|| locker.lock());
    if !sealed {
        wait_for(&mut locker, |c| c.state.locked);
    }
    wait_for(&mut app, |c| c.state.keyboard_focus.is_none());

    let mut prompt = Session::connect(h.connect());
    prompt.keyboard();
    let l = prompt.keyboard_layer("polkit", Layer::Overlay, (200, 100), KeyboardInteractivity::Exclusive);
    prompt.draw_layer(&l, BLUE);
    assert!(prompt.state.keyboard_focus.is_none(), "a prompt took the keyboard from the lock");

    match (token, lock) {
        (Some(token), _) => h.ipc_with(token).unwrap().unlock().unwrap(),
        (_, Some(lock)) => {
            lock.unlock_and_destroy();
            locker.roundtrip();
        }
        _ => unreachable!(),
    }
    assert!(!is_locked(&h));
    wait_for(&mut prompt, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    press(&h, KEY_A + 8);
    wait_for(&mut prompt, |c| c.state.keys == [KEY_A]);
    app.roundtrip();
    assert!(app.state.keys.is_empty(), "the window behind the prompt got the password");

    l.layer.destroy();
    l.surface.destroy();
    prompt.roundtrip();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
}

#[test]
fn a_prompt_behind_the_sealed_lock_gets_the_keyboard_at_unlock() {
    a_prompt_behind_the_lock_gets_the_keyboard_at_unlock(true);
}

#[test]
fn a_prompt_behind_an_ext_session_lock_gets_the_keyboard_at_unlock() {
    a_prompt_behind_the_lock_gets_the_keyboard_at_unlock(false);
}

/// Stands in for `exponential-sealed` where the compositor looks first (next
/// to the running binary), so the default config's lock client starts. It
/// ends at once, which keeps the session locked.
fn sealed_stand_in() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let path = std::env::current_exe().unwrap().parent().unwrap().join("exponential-sealed");
        match std::os::unix::fs::symlink("/bin/true", &path) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => panic!("{}: {e}", path.display()),
            _ => {}
        }
    });
}

/// #EDE-70: the default config (the sealed lock, `on_suspend`) listens to
/// logind, and both of its signals lock.
#[test]
fn the_default_config_locks_on_logind_lock_and_before_suspend() {
    sealed_stand_in();
    let config = Config::default();
    assert!(config.lock.command.is_empty() && config.lock.on_suspend);
    assert!(logind::listens(&config), "the default config does not listen to logind");
    let mut off = Config::default();
    off.lock.on_suspend = false;
    assert!(!logind::listens(&off));
    for signal in [Signal::Lock, Signal::PrepareForSleep(true)] {
        let h = harness(Config::default());
        h.with(move |ede| Listener::new(|| None).handle(ede, signal));
        assert!(is_locked(&h), "{signal:?} did not lock");
    }
}

/// #EDE-70: `lock` over ede-protocol (Lock in the start menu) locks with
/// the default config.
#[test]
fn lock_over_the_protocol_locks_with_the_default_config() {
    sealed_stand_in();
    let h = harness(Config::default());
    let mut ipc = h.ipc();
    ipc.lock().unwrap();
    assert!(is_locked(&h));
}

/// #EDE-70: the sealed lock counts as confirmed once every output showed a
/// locked frame, so suspend is held for that long and not for the full 2 s.
#[test]
fn suspend_is_held_only_until_the_sealed_lock_showed() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static INHIBITOR: AtomicU32 = AtomicU32::new(0);
    fn take() -> Option<Held> {
        let held = Held::spawn(std::process::Command::new("sleep").arg("30")).ok()?;
        INHIBITOR.store(held.0.id(), Ordering::SeqCst);
        Some(held)
    }
    sealed_stand_in();
    let h = harness(Config::default());
    assert!(!h.with(|ede| ede.lock_confirmed()));
    h.with(|ede| Listener::new(take).handle(ede, Signal::PrepareForSleep(true)));
    assert!(is_locked(&h));
    let pid = INHIBITOR.load(Ordering::SeqCst);
    assert_ne!(pid, 0, "no inhibitor was taken");
    let deadline = Instant::now() + Duration::from_secs(1);
    while std::path::Path::new(&format!("/proc/{pid}")).exists() {
        assert!(Instant::now() < deadline, "the inhibitor was still held 1 s after the lock");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.with(|ede| ede.lock_confirmed()));
}

/// The sealed lock's state, on the compositor thread.
fn sealed<R: Send + 'static>(h: &Harness, f: impl FnOnce(&mut Sealed) -> R + Send + 'static) -> R {
    h.with(|ede| match &mut ede.lock {
        LockState::Sealed(s) => f(s),
        _ => panic!("not sealed-locked"),
    })
}

/// The pid of the lock client the compositor runs (0 = none).
fn lock_child(h: &Harness) -> u32 {
    sealed(h, |s| s.child.as_ref().map_or(0, |c| c.id()))
}

fn runs(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| !s.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z'))
}

/// Locks with `argv` standing in for `exponential-sealed lock`.
fn lock_with(h: &Harness, argv: &[&str]) -> u32 {
    let argv: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
    h.with(move |ede| ede.begin_sealed_lock_with(argv));
    assert!(is_locked(h));
    lock_child(h)
}

/// Ticks (the compositor's once-a-second check) until `ok`.
fn tick_until(h: &Harness, what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        h.with(|ede| ede.sealed_tick());
        if ok() {
            return;
        }
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// #EDE-72: the lock screen's output goes (monitor off, KVM, undock): the
/// prompt moves to an output that is left, or shows on the next one, and
/// takes the keys there.
#[test]
fn the_sealed_lock_survives_its_output_going_and_coming_back() {
    use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(Config::default(), &[(800, 600), (1024, 768)]);
    let pid = std::process::id();
    h.with(move |ede| ede.begin_sealed_lock_as(pid));
    let mut locker = Session::connect(h.connect());
    locker.keyboard();
    let l = locker.layer("exponential-sealed", Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 0), -1);
    locker.draw_layer(&l, BLUE);
    wait_for(&mut locker, |_| h.capture(0).at(790, 590) == BLUE);
    assert_eq!(h.capture(1).at(5, 5), BLACK);
    wait_for(&mut locker, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));

    // Its output goes while another is left: the prompt moves there.
    h.with_backend(|ede, backend| backend.remove_output(ede, 0));
    assert!(is_locked(&h));
    wait_for(&mut locker, |c| c.state.layers[l.index].size == (1024, 768));
    locker.draw_layer(&l, BLUE);
    wait_for(&mut locker, |_| h.capture(0).at(1000, 700) == BLUE);
    press(&h, KEY_A + 8);
    wait_for(&mut locker, |c| c.state.keys == [KEY_A]);

    // The last output goes and a new one comes: the prompt shows on it.
    h.with_backend(|ede, backend| backend.remove_output(ede, 0));
    assert!(is_locked(&h));
    h.with_backend(|ede, backend| {
        backend.add_output(ede, "HEADLESS-3", (1600, 900));
    });
    wait_for(&mut locker, |c| c.state.layers[l.index].size == (1600, 900));
    locker.draw_layer(&l, BLUE);
    wait_for(&mut locker, |_| h.capture(0).at(1500, 800) == BLUE);
    press(&h, KEY_A + 8);
    wait_for(&mut locker, |c| c.state.keys == [KEY_A, KEY_A]);
}

/// #EDE-72: a lock client that runs but shows nothing (its surface closed,
/// it hangs) is started again after [`NO_SURFACE`].
#[test]
fn a_lock_client_that_shows_nothing_is_started_again() {
    let h = harness(Config::default());
    let first = lock_with(&h, &["sleep", "30"]);
    h.with(|ede| ede.sealed_tick());
    assert_eq!(lock_child(&h), first, "restarted before it had the time to show");
    sealed(&h, |s| s.seen = Instant::now() - NO_SURFACE);
    tick_until(&h, "a lock client without a surface kept running", || ![0, first].contains(&lock_child(&h)));
    assert!(!runs(first), "the old lock client still runs");
    assert!(is_locked(&h));
}

/// #EDE-72: quick deaths count up to [`MAX_RESPAWNS`], but a client that
/// lived [`HEALTHY`] starts the count anew (#NEW-106).
#[test]
fn a_lock_client_that_was_healthy_is_started_again_whatever_died_before() {
    let h = harness(Config::default());
    let first = lock_with(&h, &["sleep", "30"]);
    sealed(&h, |s| {
        s.respawns = MAX_RESPAWNS;
        s.started = Instant::now() - HEALTHY;
        s.child.as_mut().unwrap().kill().unwrap();
    });
    tick_until(&h, "the lock client was not started again", || ![0, first].contains(&lock_child(&h)));
    assert!(runs(lock_child(&h)));
}

/// #EDE-72: a lock client that could not start is tried again on the next
/// tick, not never.
#[test]
fn a_lock_client_that_could_not_start_is_tried_again() {
    let h = harness(Config::default());
    lock_with(&h, &["sleep", "30"]);
    sealed(&h, |s| {
        s.command = vec!["/nonexistent/exponential-sealed".into()];
        s.child.as_mut().unwrap().kill().unwrap();
    });
    tick_until(&h, "the dead lock client was never noticed", || lock_child(&h) == 0);
    sealed(&h, |s| s.command = vec!["sleep".into(), "30".into()]);
    tick_until(&h, "a failed start ended respawning", || lock_child(&h) != 0);
    assert!(is_locked(&h));
}

/// #EDE-79: a lock client that keeps dying is started again after a pause
/// that grows, not never: the screen does not stay black until the next
/// login.
#[test]
fn a_lock_client_that_keeps_dying_is_started_again_after_a_pause() {
    let h = harness(Config::default());
    let first = lock_with(&h, &["sleep", "30"]);
    sealed(&h, |s| {
        s.respawns = MAX_RESPAWNS;
        s.child.as_mut().unwrap().kill().unwrap();
    });
    tick_until(&h, "the dead lock client was never noticed", || lock_child(&h) == 0);
    assert!(!runs(first));
    h.with(|ede| ede.sealed_tick());
    assert_eq!(lock_child(&h), 0, "started again without a pause");
    sealed(&h, |s| s.retry_at = Instant::now());
    tick_until(&h, "a lock client that kept dying was never started again", || lock_child(&h) != 0);
    assert!(is_locked(&h));
}

/// #EDE-79: the grant alone does not unlock: `unlock` counts only on the
/// lock client's own connection (a process that read the grant from the
/// client's stdin or memory is another pid).
#[test]
fn only_the_lock_clients_own_connection_unlocks() {
    let h = harness(Config::default());
    lock_with(&h, &["sleep", "30"]);
    let token = sealed(&h, |s| s.token.clone());
    let mut thief = h.ipc_with(token).unwrap();
    assert_eq!(code(thief.unlock().unwrap_err()), ErrorCode::Forbidden);
    assert!(is_locked(&h), "a stolen grant unlocked the session");
    // The lock client's own pid on the connection: it unlocks.
    let (token, pid) = sealed(&h, |s| (s.token.clone(), s.pid));
    h.with(move |ede| ede.sealed_unlock(&token, Some(pid), None)).unwrap();
    assert!(!is_locked(&h));
}

/// #EDE-72: the unlock grant reaches the lock client over its stdin, never
/// in its environment (`/proc/<pid>/environ` is every process's to read).
#[test]
fn the_unlock_grant_is_not_in_the_lock_clients_environment() {
    let dir = tempfile::tempdir().unwrap();
    let got = dir.path().join("grant");
    let h = harness(Config::default());
    let pid = lock_with(&h, &["sh", "-c", r#"IFS= read -r grant; printf %s "$grant" > "$0.part"; mv "$0.part" "$0"; exec sleep 30"#, got.to_str().unwrap()]);
    let token = sealed(&h, |s| s.token.as_str().to_string());
    let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap();
    assert!(!String::from_utf8_lossy(&environ).contains(&token), "the unlock grant is in the lock client's environment");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !got.exists() {
        assert!(Instant::now() < deadline, "the lock client was handed nothing on its stdin");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(&got).unwrap(), token);
}

/// #EDE-72: the lock client never outlives the compositor.
#[test]
fn the_lock_client_ends_with_the_compositor() {
    let h = harness(Config::default());
    let pid = lock_with(&h, &["sleep", "30"]);
    assert!(runs(pid));
    drop(h);
    let deadline = Instant::now() + Duration::from_secs(2);
    while runs(pid) {
        assert!(Instant::now() < deadline, "the lock client outlived the compositor");
        std::thread::sleep(Duration::from_millis(10));
    }
}
