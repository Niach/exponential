//! #NEW-104 headless: ede-protocol end to end, capability tokens included.

mod common;

use std::{
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{events, Capability, CaptureTarget, Client, Error, ErrorCode, InputEvent, Request, SessionId, Token};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;
const KEY_A: u32 = 30;
// evdev codes; a smithay Keycode is the code + 8.
const KEY_TAB: u32 = 15;
const KEY_LEFTALT: u32 = 56;
const KEY_LEFTMETA: u32 = 125;
const KEY_LEFTSHIFT: u32 = 42;
const KEY_LEFTCTRL: u32 = 29;
const KEY_UP: u32 = 103;
const KEY_DOWN: u32 = 108;
const KEY_VOLUMEUP: u32 = 115;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn code(e: Error) -> ErrorCode {
    match e {
        Error::Failed(f) => f.code,
        other => panic!("expected a failure, got {other}"),
    }
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

#[test]
fn hello_needs_a_known_token() {
    let h = harness();
    let bogus = Token::new("0".repeat(64)).unwrap();
    assert_eq!(code(h.ipc_with(bogus).err().unwrap()), ErrorCode::Unauthorized);
    let root = h.ipc();
    assert_eq!(root.hello.capabilities, Capability::ALL.to_vec());
    assert_eq!(root.hello.session_id.as_str(), "exponential-shell");
}

#[test]
fn windows_outputs_and_window_management() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("notes");
    t.toplevel.set_app_id("org.example.Notes".into());
    app.draw(&t, 300, 200, BLUE);

    let mut ipc = h.ipc();
    let outputs = ipc.outputs().unwrap();
    assert_eq!((outputs.len(), outputs[0].width, outputs[0].height), (1, 800, 600));
    let windows = ipc.windows().unwrap();
    assert_eq!(windows.len(), 1);
    let w = &windows[0];
    assert_eq!((w.title.as_deref(), w.app_id.as_deref(), w.width, w.height, w.focused, w.protected), (Some("notes"), Some("org.example.Notes"), 300, 200, true, false));

    ipc.call(Request::MoveWindow { window: w.id.clone(), x: 40, y: 50 }).unwrap();
    let moved = &ipc.windows().unwrap()[0];
    assert_eq!((moved.x, moved.y), (40, 50));
    ipc.call(Request::CloseWindow { window: w.id.clone() }).unwrap();
    wait_for(&mut app, |c| c.state.toplevels[0].closed);
    let gone = ede_protocol::WindowId::new("w999999").unwrap();
    assert_eq!(code(ipc.call(Request::FocusWindow { window: gone }).unwrap_err()), ErrorCode::NotFound);
}

#[test]
fn a_minted_grant_holds_only_what_it_was_given() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let secret = app.toplevel("vault");
    secret.toplevel.set_app_id("org.keepassxc.KeePassXC".into());
    app.draw(&secret, 300, 200, RED);

    let mut root = h.ipc();
    let windows = root.windows().unwrap();
    let vault = windows.iter().find(|w| w.protected).unwrap().clone();
    assert_eq!(vault.title, None, "a protected window's title is not listed");
    let notes = windows.iter().find(|w| !w.protected).unwrap().clone();

    let grant = root.mint(SessionId::new("agent-1").unwrap(), vec![Capability::Capture], None).unwrap();
    let mut agent = h.ipc_with(grant.token.clone()).unwrap();
    assert_eq!(agent.hello.capabilities, [Capability::Capture]);

    // Capture: protected windows black, the rest as drawn.
    let frame = agent.capture(CaptureTarget::Output { name: "HEADLESS-1".into() }).unwrap();
    assert_eq!((frame.info.width, frame.info.height), (800, 600));
    assert_eq!(frame.at(notes.x + 5, notes.y + 5), BLUE);
    assert_eq!(frame.at(vault.x + vault.width - 5, vault.y + vault.height - 5), BLACK);
    let only = agent.capture(CaptureTarget::Window { window: notes.id.clone() }).unwrap();
    assert_eq!((only.info.width, only.info.height, only.at(150, 100)), (300, 200, BLUE));
    assert_eq!(code(agent.capture(CaptureTarget::Window { window: vault.id.clone() }).unwrap_err()), ErrorCode::Forbidden);

    // Nothing it was not given.
    assert_eq!(code(agent.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap_err()), ErrorCode::Forbidden);
    assert_eq!(code(agent.call(Request::FocusWindow { window: notes.id.clone() }).unwrap_err()), ErrorCode::Forbidden);
    assert_eq!(code(agent.mint(SessionId::new("agent-2").unwrap(), vec![Capability::Capture], None).unwrap_err()), ErrorCode::Forbidden);

    // A system grant cannot hand out more than it holds or outlive itself.
    let far = 4_102_444_800_000; // 2100
    let system = root.mint(SessionId::new("daemon").unwrap(), vec![Capability::System, Capability::Capture], Some(far)).unwrap();
    let mut daemon = h.ipc_with(system.token).unwrap();
    assert_eq!(code(daemon.mint(SessionId::new("x").unwrap(), vec![Capability::InputPerson], Some(far)).unwrap_err()), ErrorCode::Forbidden);
    assert_eq!(code(daemon.mint(SessionId::new("x").unwrap(), vec![Capability::Capture], None).unwrap_err()), ErrorCode::Forbidden);
    assert!(daemon.mint(SessionId::new("x").unwrap(), vec![Capability::Capture], Some(far - 1)).is_ok());

    // Expired and revoked grants stop working.
    let expired = root.mint(SessionId::new("old").unwrap(), vec![Capability::Capture], Some(1)).unwrap();
    assert_eq!(code(h.ipc_with(expired.token).err().unwrap()), ErrorCode::Unauthorized);
    root.call(Request::RevokeGrant { token: grant.token }).unwrap();
    assert_eq!(code(agent.windows().unwrap_err()), ErrorCode::Unauthorized);
    let root_token = h.with(|ede| ede.ipc.root_grant().token);
    assert_eq!(code(root.call(Request::RevokeGrant { token: root_token }).unwrap_err()), ErrorCode::Forbidden);
}

#[test]
fn input_drives_normal_windows_and_never_protected_ones() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());

    let mut ipc = h.ipc();
    ipc.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap();
    ipc.input(InputEvent::Key { key: KEY_A, pressed: false }).unwrap();
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);

    let secret = app.toplevel("prompt");
    secret.toplevel.set_app_id("gcr-prompter".into());
    app.draw(&secret, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&secret.surface));
    assert_eq!(code(ipc.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap_err()), ErrorCode::Forbidden);
    let vault = ipc.windows().unwrap().into_iter().find(|w| w.protected).unwrap();
    let over = InputEvent::PointerMotion { x: (vault.x + 10) as f64, y: (vault.y + 10) as f64 };
    assert_eq!(code(ipc.input(over).unwrap_err()), ErrorCode::Forbidden);
}

#[test]
fn subscribers_hear_windows_and_the_lock() {
    let h = harness();
    let mut ipc = h.ipc();
    ipc.subscribe().unwrap();
    ipc.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    let mut app = Session::connect(h.connect());
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let opened = ipc.next_event().unwrap();
    assert_eq!(opened.kind, events::WINDOW_OPENED);
    assert!(opened.errors().is_empty(), "{:?}", opened.errors());
    assert_eq!(ipc.next_event().unwrap().kind, events::WINDOW_FOCUSED);

    let mut locker = Session::connect(h.connect());
    let _lock = locker.lock();
    assert_eq!(ipc.next_event().unwrap().kind, events::SESSION_LOCKED);

    // An unsubscribed connection hears nothing (its calls still answer).
    let mut quiet = h.ipc();
    assert!(quiet.windows().is_ok());
}

/// A person's key (bindings on), both edges or one.
fn press(h: &Harness, code: u32, pressed: bool) {
    use smithay::backend::input::KeyState;
    h.with(move |ede| ede.keyboard_key((code + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, 0));
}

#[test]
fn hotkeys_reach_a_listening_shell_and_the_switcher_commits_on_alt_release() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());

    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    // Alt+Tab, Tab again, Alt up: next, next, commit; Tab never reaches the app.
    press(&h, KEY_LEFTALT, true);
    press(&h, KEY_TAB, true);
    press(&h, KEY_TAB, false);
    press(&h, KEY_TAB, true);
    press(&h, KEY_TAB, false);
    press(&h, KEY_LEFTALT, false);
    let names = |shell: &mut ede_protocol::Client, n: usize| -> Vec<String> {
        (0..n).map(|_| {
            let e = shell.next_event().unwrap();
            assert_eq!(e.kind, events::HOTKEY);
            e.data["name"].as_str().unwrap().to_string()
        }).collect()
    };
    assert_eq!(names(&mut shell, 3), ["switcher.next", "switcher.next", "switcher.commit"]);
    app.roundtrip();
    assert!(!app.state.keys.contains(&KEY_TAB), "Tab stayed with the switcher: {:?}", app.state.keys);
    // Super alone: the menu. A media key: the OSD, and the app never sees it.
    press(&h, KEY_LEFTMETA, true);
    press(&h, KEY_LEFTMETA, false);
    press(&h, KEY_VOLUMEUP, true);
    press(&h, KEY_VOLUMEUP, false);
    assert_eq!(names(&mut shell, 2), ["menu.toggle", "volume.up"]);
    app.roundtrip();
    assert!(!app.state.keys.contains(&KEY_VOLUMEUP));
    // Ctrl+Alt+Up and Down (#EDE-49): Expo and Scale, the keys never reach the app.
    press(&h, KEY_LEFTCTRL, true);
    press(&h, KEY_LEFTALT, true);
    press(&h, KEY_UP, true);
    press(&h, KEY_UP, false);
    press(&h, KEY_DOWN, true);
    press(&h, KEY_DOWN, false);
    press(&h, KEY_LEFTALT, false);
    press(&h, KEY_LEFTCTRL, false);
    assert_eq!(names(&mut shell, 2), ["expo.toggle", "scale.toggle"]);
    app.roundtrip();
    assert!(!app.state.keys.contains(&KEY_UP) && !app.state.keys.contains(&KEY_DOWN), "{:?}", app.state.keys);
}

/// #EDE-48: minimize over ede-protocol and from the app; a minimized window
/// leaves the screen, keeps its place, and a focus from the taskbar restores it.
#[test]
fn minimized_windows_leave_the_screen_and_come_back_where_they_were() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let a = app.toplevel("a");
    app.draw(&a, 300, 200, BLUE);
    let b = app.toplevel("b");
    app.draw(&b, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));

    let mut ipc = h.ipc();
    ipc.subscribe().unwrap();
    ipc.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let kinds = |ipc: &mut ede_protocol::Client, n: usize| -> Vec<String> { (0..n).map(|_| ipc.next_event().unwrap().kind).collect() };
    let windows = ipc.windows().unwrap();
    let before = windows.iter().find(|w| w.title.as_deref() == Some("b")).unwrap().clone();
    let id_b = before.id.clone();
    assert!(!before.minimized);

    ipc.minimize(id_b.clone()).unwrap();
    let ev = ipc.next_event().unwrap();
    assert_eq!((ev.kind.as_str(), ev.data["window"].as_str()), (events::WINDOW_MINIMIZED, Some(id_b.as_str())));
    assert_eq!(kinds(&mut ipc, 1), [events::WINDOW_FOCUSED], "focus fell to a");
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&a.surface));
    // Listed as minimized at its old place; not drawn, not on the space.
    let b_info = ipc.windows().unwrap().into_iter().find(|w| w.id == id_b).unwrap();
    assert!(b_info.minimized && !b_info.focused && (b_info.x, b_info.y) == (before.x, before.y));
    assert!(h.with(|ede| ede.space.elements().count() == 1));
    assert_eq!(h.capture(0).at(before.x + 5, before.y + 5), BLUE, "a shows where b (cascaded over it) was");
    // Its last frame is still there for the taskbar's hover preview.
    let shot = ipc.capture(CaptureTarget::Window { window: id_b.clone() }).unwrap();
    assert_eq!((shot.info.width, shot.info.height, shot.at(150, 100)), (300, 200, RED));

    // Focusing it from the taskbar restores it where it was, on top.
    ipc.focus(id_b.clone()).unwrap();
    assert_eq!(kinds(&mut ipc, 2), [events::WINDOW_RESTORED, events::WINDOW_FOCUSED]);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));
    let back = ipc.windows().unwrap().into_iter().find(|w| w.id == id_b).unwrap();
    assert!(!back.minimized && back.focused && (back.x, back.y) == (before.x, before.y));
    assert_eq!(h.capture(0).at(before.x + 5, before.y + 5), RED);

    // The app's own minimize button, then `restore_window`.
    b.toplevel.set_minimized();
    app.roundtrip();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&a.surface));
    assert_eq!(kinds(&mut ipc, 2), [events::WINDOW_MINIMIZED, events::WINDOW_FOCUSED]);
    ipc.restore(id_b.clone()).unwrap();
    assert_eq!(kinds(&mut ipc, 2), [events::WINDOW_RESTORED, events::WINDOW_FOCUSED]);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));
    ipc.restore(id_b.clone()).unwrap(); // not minimized: nothing happens, no error

    // Minimized, moved to workspace 2 while minimized, then focused: the
    // restore brings workspace 2 along.
    ipc.minimize(id_b.clone()).unwrap();
    ipc.move_to_workspace(id_b.clone(), 1).unwrap();
    assert_eq!(ipc.workspaces().unwrap()[1].windows, vec![id_b.clone()]);
    assert!(ipc.windows().unwrap().iter().any(|w| w.id == id_b && w.minimized && w.workspace == 1));
    ipc.focus(id_b.clone()).unwrap();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));
    assert!(ipc.workspaces().unwrap()[1].active);
    let on_two = ipc.windows().unwrap().into_iter().find(|w| w.id == id_b).unwrap();
    assert!(!on_two.minimized && on_two.workspace == 1 && (on_two.x, on_two.y) == (before.x, before.y));

    // Closing a minimized window from the taskbar works and is announced.
    ipc.minimize(id_b.clone()).unwrap();
    ipc.close_window(id_b.clone()).unwrap();
    wait_for(&mut app, |c| c.state.toplevels[1].closed);
    b.toplevel.destroy();
    app.roundtrip();
    wait_for(&mut app, |_| h.with(|ede| ede.all_windows().len() == 1));
}

#[test]
fn without_a_shell_alt_tab_cycles_focus_and_media_keys_reach_the_app() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let a = app.toplevel("a");
    app.draw(&a, 300, 200, BLUE);
    let b = app.toplevel("b");
    app.draw(&b, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));
    press(&h, KEY_LEFTALT, true);
    press(&h, KEY_TAB, true);
    press(&h, KEY_TAB, false);
    press(&h, KEY_LEFTALT, false);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&a.surface));
    press(&h, KEY_VOLUMEUP, true);
    press(&h, KEY_VOLUMEUP, false);
    wait_for(&mut app, |c| c.state.keys.contains(&KEY_VOLUMEUP));
    // Logout is a system op: an agent's narrower grant cannot end the session.
    let mut root = h.ipc();
    let agent = root.mint(SessionId::new("agent-1").unwrap(), vec![Capability::Ui], None).unwrap();
    let mut agent = h.ipc_with(agent.token).unwrap();
    assert_eq!(code(agent.logout().unwrap_err()), ErrorCode::Forbidden);
}

#[test]
fn reload_config_applies_the_file() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("compositor.toml");
    std::fs::write(&path, "background = \"#102030\"\n").unwrap();
    let p = path.clone();
    h.with(move |ede| ede.config_path = Some(p));
    let mut ipc = h.ipc();
    ipc.call(Request::ReloadConfig).unwrap();
    assert_eq!(h.capture(0).at(5, 5), 0xff10_2030);

    std::fs::write(&path, "[windows]\nsnap_treshold = 1\n").unwrap();
    assert_eq!(code(ipc.call(Request::ReloadConfig).unwrap_err()), ErrorCode::Invalid);
    assert_eq!(h.capture(0).at(5, 5), 0xff10_2030, "a broken file keeps the running config");
}

#[test]
fn the_socket_lives_in_a_private_directory() {
    let h = harness();
    let runtime = tempfile::tempdir().unwrap();
    let r = runtime.path().to_path_buf();
    let socket = h.with(move |ede| ede.ipc_listen(&r, "wayland-9")).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&runtime.path().join("exponential-desktop")), 0o700);
    let token_file = ede_protocol::token_path(runtime.path(), "wayland-9");
    assert_eq!(mode(&token_file), 0o600);
    let token = Token::new(std::fs::read_to_string(&token_file).unwrap()).unwrap();
    let mut client = Client::connect(&socket, token).unwrap();
    assert_eq!(client.outputs().unwrap().len(), 1);
}

/// #EDE-18: workspaces over ede-protocol and the bindings; parked windows
/// leave the screen and the taskbar still lists them.
#[test]
fn workspaces_park_windows_and_the_taskbar_brings_them_back() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let a = app.toplevel("a");
    app.draw(&a, 300, 200, BLUE);
    let b = app.toplevel("b");
    app.draw(&b, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));

    let mut ipc = h.ipc();
    ipc.subscribe().unwrap();
    ipc.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let spaces = ipc.workspaces().unwrap();
    assert_eq!(spaces.len(), 4, "Cinnamon's four by default");
    assert!(spaces[0].active && spaces[0].windows.len() == 2);

    // Super+2: both windows park, the screen shows the background only.
    const KEY_2: u32 = 3;
    press(&h, KEY_LEFTMETA, true);
    press(&h, KEY_2, true);
    press(&h, KEY_2, false);
    press(&h, KEY_LEFTMETA, false);
    let switched = ipc.next_event().unwrap();
    assert_eq!((switched.kind.as_str(), switched.data["workspace"].as_u64()), (events::WORKSPACE_SWITCHED, Some(1)));
    assert!(h.with(|ede| ede.space.elements().count() == 0));
    let windows = ipc.windows().unwrap();
    assert_eq!(windows.len(), 2, "parked windows are still listed");
    assert!(windows.iter().all(|w| w.workspace == 0 && !w.focused));
    assert_eq!(h.capture(0).at(400, 300), 0xff0a_0a0a, "nothing on workspace 2");

    // Focusing a parked window from the taskbar switches back to it.
    let id_a = windows.iter().find(|w| w.title.as_deref() == Some("a")).unwrap().id.clone();
    ipc.focus(id_a.clone()).unwrap();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&a.surface));
    assert_eq!(ipc.workspaces().unwrap()[0].active, true);
    assert!(h.with(|ede| ede.space.elements().count() == 2));

    // Move b to workspace 3 over the protocol: it leaves, a stays.
    let id_b = windows.iter().find(|w| w.title.as_deref() == Some("b")).unwrap().id.clone();
    ipc.move_to_workspace(id_b.clone(), 2).unwrap();
    assert!(h.with(|ede| ede.space.elements().count() == 1));
    let spaces = ipc.workspaces().unwrap();
    assert_eq!(spaces[2].windows, vec![id_b.clone()]);
    assert_eq!(ipc.windows().unwrap().iter().find(|w| w.id == id_b).unwrap().workspace, 2);
    ipc.switch_workspace(2).unwrap();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&b.surface));
    assert_eq!(code(ipc.switch_workspace(42).unwrap_err()), ErrorCode::Invalid);
}

/// #EDE-18: an output's scale and mode over ede-protocol and from the config.
#[test]
fn outputs_take_a_scale_and_a_mode() {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let config = Config::parse("[[outputs]]\nname = \"HEADLESS-1\"\nscale = 2.0\n").unwrap();
    let h = Harness::start(config, &[(800, 600)]);
    let mut ipc = h.ipc();
    let out = &ipc.outputs().unwrap()[0];
    assert_eq!((out.scale, out.width, out.height), (2.0, 400, 300), "the logical size is the mode over the scale");
    let current: Vec<_> = out.modes.iter().filter(|m| m.current).collect();
    assert_eq!(current.len(), 1);
    assert!(current[0].preferred);
    assert_eq!((current[0].width, current[0].height, current[0].refresh), (800, 600, 60_000));
    assert_eq!(out.modes.len(), 4, "its own size and the virtual GPU's three");

    // A window opened on a 2× output hears the preferred scale.
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("hidpi");
    app.draw(&t, 100, 100, BLUE);
    assert!(ipc.windows().unwrap().iter().any(|w| w.title.as_deref() == Some("hidpi")));

    ipc.set_output("HEADLESS-1", None, Some(1.5), Some((10, 20))).unwrap();
    let out = &ipc.outputs().unwrap()[0];
    assert_eq!((out.scale, out.x, out.y, out.width), (1.5, 10, 20, 534), "800 / 1.5, rounded up");
    assert_eq!(code(ipc.set_output("HEADLESS-1", Some("1234x999"), None, None).unwrap_err()), ErrorCode::Invalid, "a mode the output does not offer");
    // #EDE-65: a mode change (a VM window resized) reaches the logical size.
    ipc.set_output("HEADLESS-1", Some("1600x900"), Some(1.0), None).unwrap();
    let out = &ipc.outputs().unwrap()[0];
    assert_eq!((out.width, out.height), (1600, 900));
    assert_eq!(code(ipc.set_output("HEADLESS-1", Some("wide"), None, None).unwrap_err()), ErrorCode::Invalid);
    assert_eq!(code(ipc.set_output("DP-9", None, Some(1.0), None).unwrap_err()), ErrorCode::Invalid);
    // Only the system capability changes outputs.
    let mut root = h.ipc();
    let grant = root.mint(SessionId::new("agent-1").unwrap(), vec![Capability::Ui], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    assert_eq!(code(agent.set_output("HEADLESS-1", None, Some(1.0), None).unwrap_err()), ErrorCode::Forbidden);
    assert!(agent.workspaces().is_ok(), "listing needs no capability");
}

#[test]
fn global_shortcuts_fire_from_the_person_and_keep_the_key() {
    use ede_protocol::ShortcutSpec;
    use smithay::backend::input::KeyState;
    const CTRL: u32 = 29 + 8;
    const ALT: u32 = 56 + 8;
    const R: u32 = 19 + 8;
    let key = |h: &Harness, code: u32, state: KeyState| h.with(move |ede| ede.keyboard_key(code.into(), state, 7));
    let chord = |h: &Harness| {
        key(h, CTRL, KeyState::Pressed);
        key(h, ALT, KeyState::Pressed);
        key(h, R, KeyState::Pressed);
        key(h, R, KeyState::Released);
        key(h, ALT, KeyState::Released);
        key(h, CTRL, KeyState::Released);
    };
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());

    let mut portal = h.ipc();
    let spec = |id: &str, trigger: &str| ShortcutSpec { id: id.into(), trigger: trigger.into() };
    let bound = portal.bind_shortcuts("/s/1", vec![spec("rec", "ctrl+alt+r"), spec("typing", "a"), spec("quit", "LOGO+SHIFT+e")]).unwrap();
    let triggers: Vec<_> = bound.iter().map(|b| b.trigger.as_deref()).collect();
    assert_eq!(triggers, [Some("CTRL+ALT+r"), None, None], "plain keys and compositor bindings stay unbound");

    portal.subscribe().unwrap();
    portal.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    chord(&h);
    let down = portal.next_event().unwrap();
    let up = portal.next_event().unwrap();
    assert_eq!((down.kind.as_str(), up.kind.as_str()), (events::SHORTCUT_ACTIVATED, events::SHORTCUT_DEACTIVATED));
    assert_eq!(down.data["id"], "rec");
    assert_eq!(down.data["owner"], "/s/1");
    app.roundtrip();
    assert!(!app.state.keys.contains(&(R - 8)), "the shortcut's key reached the client");

    // Synthetic input never fires a shortcut; a protected prompt blocks it.
    portal.input(InputEvent::Key { key: R - 8, pressed: true }).ok();
    let secret = app.toplevel("prompt");
    secret.toplevel.set_app_id("gcr-prompter".into());
    app.draw(&secret, 300, 200, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&secret.surface));
    chord(&h);
    portal.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    while let Ok(e) = portal.next_event() {
        assert!(!e.kind.starts_with("shortcut."), "a shortcut fired over a protected prompt");
    }

    // The owner's connection goes, its shortcuts go.
    drop(portal);
    let mut other = h.ipc();
    let again = other.bind_shortcuts("/s/2", vec![spec("rec", "CTRL+ALT+r")]).unwrap();
    assert_eq!(again[0].trigger.as_deref(), Some("CTRL+ALT+r"));
}

const KEY_ESC: u32 = 1;

fn person_key(h: &Harness, code: u32) {
    use smithay::backend::input::KeyState;
    h.with(move |ede| {
        ede.keyboard_key((code + 8).into(), KeyState::Pressed, 0);
        ede.keyboard_key((code + 8).into(), KeyState::Released, 0);
    });
}

/// #EDE-23: a hold keeps the person's keys and pointer from every window
/// while the agent's land; their Esc ends it (and reaches no window), a
/// holder that goes away lets go, a grant without `input:person` cannot hold.
#[test]
fn a_hold_locks_the_person_out_until_their_esc() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());

    let grant = h.ipc().mint(SessionId::new("epilot").unwrap(), vec![Capability::InputPerson], None).unwrap();
    let mut agent = h.ipc_with(grant.token.clone()).unwrap();
    agent.hold_input().unwrap();
    assert!(h.with(|ede| ede.person_input_held()));

    person_key(&h, KEY_A);
    let before = h.with(|ede| ede.seat.get_pointer().unwrap().current_location());
    assert!(!h.with(|ede| ede.person_pointer_motion_to((400.0, 400.0).into(), 0)));
    assert_eq!(h.with(|ede| ede.seat.get_pointer().unwrap().current_location()), before);
    agent.input(InputEvent::Key { key: KEY_LEFTSHIFT, pressed: true }).unwrap();
    agent.input(InputEvent::Key { key: KEY_LEFTSHIFT, pressed: false }).unwrap();
    wait_for(&mut app, |c| c.state.keys == [KEY_LEFTSHIFT]);

    agent.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    person_key(&h, KEY_ESC);
    assert_eq!(agent.next_event().unwrap().kind, events::INPUT_ESCAPE);
    assert!(!h.with(|ede| ede.person_input_held()));
    person_key(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_LEFTSHIFT, KEY_A]);
    assert!(!app.state.keys.contains(&KEY_ESC), "the Esc that ends a hold reaches no window");

    agent.hold_input().unwrap();
    assert!(h.with(|ede| ede.person_input_held()));
    agent.release_input().unwrap();
    assert!(!h.with(|ede| ede.person_input_held()));
    agent.hold_input().unwrap();
    drop(agent);
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(|ede| ede.person_input_held()) {
        assert!(Instant::now() < deadline, "the hold outlived its connection");
        std::thread::sleep(Duration::from_millis(10));
    }
    let watcher = h.ipc().mint(SessionId::new("watcher").unwrap(), vec![Capability::Capture], None).unwrap();
    assert_eq!(code(h.ipc_with(watcher.token).unwrap().hold_input().unwrap_err()), ErrorCode::Forbidden);
}

/// #EDE-22: a keysym finds its key and level on the layout; `A` = Shift
/// held around `a`, released with it; a keysym no key makes is `not_found`.
#[test]
fn keysyms_find_their_key_and_level_on_the_layout() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());

    let mut ipc = h.ipc();
    for (keysym, pressed) in [(0x61, true), (0x61, false), (0x41, true), (0x41, false)] {
        ipc.input(InputEvent::Keysym { keysym, pressed }).unwrap();
    }
    wait_for(&mut app, |c| c.state.keys == [KEY_A, KEY_LEFTSHIFT, KEY_A]);
    assert!(h.with(|ede| ede.seat.get_keyboard().unwrap().pressed_keys().is_empty()), "Shift is let go with the key");
    let snowman = InputEvent::Keysym { keysym: 0x0100_2603, pressed: true };
    assert_eq!(code(ipc.input(snowman).unwrap_err()), ErrorCode::NotFound);

    ipc.input(InputEvent::PointerMotion { x: 120.0, y: 80.0 }).unwrap();
    let at = ipc.pointer().unwrap();
    assert_eq!((at.x, at.y), (120.0, 80.0));
}

#[test]
fn relative_motion_and_scrolling() {
    let h = harness();
    let mut ipc = h.ipc();
    ipc.input(InputEvent::PointerMotion { x: 100.0, y: 100.0 }).unwrap();
    ipc.input(InputEvent::PointerMotionRelative { dx: 25.0, dy: -10.0 }).unwrap();
    let at = h.with(|ede| ede.seat.get_pointer().unwrap().current_location());
    assert_eq!((at.x, at.y), (125.0, 90.0));
    ipc.input(InputEvent::PointerAxis { dx: 0.0, dy: 30.0 }).unwrap();
    ipc.input(InputEvent::PointerAxisDiscrete { dx: 0, dy: -2 }).unwrap();
}

fn item(mime: &str, data: &[u8]) -> ede_protocol::ClipboardItem {
    ede_protocol::ClipboardItem { mime: mime.into(), data: data.to_vec() }
}

/// Runs `call` on its own thread (a deferred clipboard read waits for the
/// client) while `app` pumps its events.
fn pumped<R: Send + 'static>(app: &mut Session, call: impl FnOnce() -> R + Send + 'static) -> R {
    let handle = std::thread::spawn(call);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !handle.is_finished() {
        assert!(Instant::now() < deadline, "the clipboard call never answered");
        app.roundtrip();
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.join().unwrap()
}

/// ePilot's paste on eDE: the compositor owns a selection, the focused
/// client sees and reads it (`taken`), a client's own selection moves the
/// serial and is read back through pipes (a hung type dropped after its
/// timeout), and an empty set clears it.
#[test]
fn the_clipboard_is_set_read_back_and_taken() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("editor");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let device = app.data_device();

    let mut ipc = h.ipc();
    let grant = ipc.mint(SessionId::new("epilot").unwrap(), vec![Capability::Capture, Capability::InputPerson], None).unwrap();
    let mut epilot = h.ipc_with(grant.token).unwrap();
    let capture_only = ipc.mint(SessionId::new("viewer").unwrap(), vec![Capability::Capture], None).unwrap();
    assert_eq!(code(h.ipc_with(capture_only.token).unwrap().clipboard_serial().unwrap_err()), ErrorCode::Forbidden);

    let before = epilot.clipboard_serial().unwrap();
    assert_eq!(epilot.clipboard_text().unwrap().text, None);
    let serial = epilot.clipboard_set(vec![item("text/plain;charset=utf-8", b"hunter2"), item("text/plain", b"hunter2")], true).unwrap();
    assert!(serial > before);
    assert_eq!(epilot.clipboard_serial().unwrap(), serial);
    let snapshot = epilot.clipboard_snapshot().unwrap();
    assert_eq!(snapshot.serial, serial);
    let mimes: Vec<&str> = snapshot.items.iter().map(|i| i.mime.as_str()).collect();
    assert_eq!(mimes, ["text/plain;charset=utf-8", "text/plain", ede_protocol::CLIPBOARD_SECRET_HINT]);
    assert_eq!(snapshot.items[2].data, b"secret");
    assert_eq!(epilot.clipboard_text().unwrap().text.as_deref(), Some("hunter2"));

    // The focused client is offered it; reading the hint is not a paste.
    wait_for(&mut app, |c| c.selection_types().is_some_and(|t| t.len() == 3));
    assert_eq!(app.read_selection(ede_protocol::CLIPBOARD_SECRET_HINT), b"secret");
    assert_eq!(epilot.clipboard_taken(serial).unwrap(), ede_protocol::ClipboardTaken { serial, taken: false });
    assert_eq!(app.read_selection("text/plain"), b"hunter2");
    assert_eq!(epilot.clipboard_taken(serial).unwrap(), ede_protocol::ClipboardTaken { serial, taken: true });

    // Larger than a pipe's buffer: written as the reader drains it.
    let big: Vec<u8> = (0..600_000u32).map(|i| (i % 251) as u8).collect();
    let serial = epilot.clipboard_set(vec![item("application/octet-stream", &big)], false).unwrap();
    wait_for(&mut app, |c| c.selection_types().is_some_and(|t| t == ["application/octet-stream"]));
    assert_eq!(app.read_selection("application/octet-stream"), big);
    assert!(epilot.clipboard_taken(serial).unwrap().taken);

    // The client takes the clipboard: the serial moves past ours.
    let _source = app.set_selection(&device, &[("text/plain;charset=utf-8", Some("copied ü".as_bytes())), ("application/x-hang", None), ("text/html", Some(b"<b>copied</b>"))]);
    let theirs = epilot.clipboard_serial().unwrap();
    assert!(theirs > serial);
    assert_eq!(epilot.clipboard_taken(serial).unwrap(), ede_protocol::ClipboardTaken { serial: theirs, taken: false });

    let started = Instant::now();
    let (snapshot, epilot) = pumped(&mut app, move || (epilot.clipboard_snapshot().unwrap(), epilot));
    let mut epilot = epilot;
    assert!(started.elapsed() < Duration::from_secs(3), "the hung type timed out: {:?}", started.elapsed());
    assert_eq!(snapshot.serial, theirs);
    let read: Vec<(&str, &[u8])> = snapshot.items.iter().map(|i| (i.mime.as_str(), i.data.as_slice())).collect();
    assert_eq!(read, [("text/plain;charset=utf-8", "copied ü".as_bytes()), ("text/html", b"<b>copied</b>".as_slice())]);
    let (text, epilot2) = pumped(&mut app, move || (epilot.clipboard_text().unwrap(), epilot));
    epilot = epilot2;
    assert_eq!(text.text.as_deref(), Some("copied ü"));
    // The connection still answers in order after the deferred replies.
    assert_eq!(epilot.clipboard_serial().unwrap(), theirs);

    // Restore = set the saved items back; empty = clear.
    let restored = epilot.clipboard_set(snapshot.items.clone(), false).unwrap();
    wait_for(&mut app, |c| c.state.source_cancelled);
    assert_eq!(epilot.clipboard_snapshot().unwrap().items, snapshot.items);
    let cleared = epilot.clipboard_set(vec![], false).unwrap();
    assert!(cleared > restored);
    wait_for(&mut app, |c| c.state.selection.is_none());
    assert_eq!(epilot.clipboard_snapshot().unwrap(), ede_protocol::ClipboardSnapshot { serial: cleared, items: vec![] });
    assert_eq!(epilot.clipboard_text().unwrap().text, None);
}

/// The clipboard across Xwayland: an X11 client pastes the selection
/// eDE set (and that counts as taken), and eDE snapshots an X11 client's
/// selection through the XWM. Xwayland must be installed; skipped otherwise.
#[cfg(feature = "xwayland")]
#[test]
fn the_clipboard_crosses_xwayland_both_ways() {
    use x11rb::{
        connection::Connection,
        protocol::{
            xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent, WindowClass, SELECTION_NOTIFY_EVENT},
            Event,
        },
        wrapper::ConnectionExt as _,
        CURRENT_TIME,
    };
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let h = harness();
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
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 100, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE))
        .unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(|ede| ede.focused_window().is_some_and(|w| w.x11_surface().is_some())) {
        assert!(Instant::now() < deadline, "the X11 window never took focus");
        std::thread::sleep(Duration::from_millis(20));
    }
    let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom;
    let (clipboard, utf8, targets, prop) = (atom("CLIPBOARD"), atom("UTF8_STRING"), atom("TARGETS"), atom("EDE_TEST"));

    let mut ipc = h.ipc();
    let serial = ipc.clipboard_set(vec![item("text/plain;charset=utf-8", b"from ede"), item("UTF8_STRING", b"from ede")], false).unwrap();
    // X11 pastes: convert CLIPBOARD to UTF8_STRING (retried until the XWM owns it).
    let deadline = Instant::now() + Duration::from_secs(10);
    let pasted = 'paste: loop {
        assert!(Instant::now() < deadline, "the X11 client never got the selection");
        conn.convert_selection(win, clipboard, utf8, prop, CURRENT_TIME).unwrap();
        conn.flush().unwrap();
        let wait = Instant::now() + Duration::from_secs(2);
        while Instant::now() < wait {
            match conn.poll_for_event().unwrap() {
                Some(Event::SelectionNotify(n)) if n.property == prop => {
                    let reply = conn.get_property(true, win, prop, AtomEnum::ANY, 0, u32::MAX).unwrap().reply().unwrap();
                    break 'paste reply.value;
                }
                Some(Event::SelectionNotify(_)) => break,
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(pasted, b"from ede");
    assert!(ipc.clipboard_taken(serial).unwrap().taken);

    // X11 copies: eDE reads it back through the XWM.
    conn.set_selection_owner(win, clipboard, CURRENT_TIME).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while ipc.clipboard_serial().unwrap() == serial {
        assert!(Instant::now() < deadline, "the X11 selection never reached eDE");
        // Answer the XWM's TARGETS query meanwhile.
        serve_x11(&conn, win, targets, utf8, b"from x11");
        std::thread::sleep(Duration::from_millis(10));
    }
    let handle = std::thread::spawn(move || (ipc.clipboard_text().unwrap(), ipc.clipboard_snapshot().unwrap()));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !handle.is_finished() {
        assert!(Instant::now() < deadline, "the X11 read never answered");
        serve_x11(&conn, win, targets, utf8, b"from x11");
        std::thread::sleep(Duration::from_millis(5));
    }
    let (text, snapshot) = handle.join().unwrap();
    assert_eq!(text.text.as_deref(), Some("from x11"));
    assert!(snapshot.items.iter().any(|i| i.data == b"from x11"), "{:?}", snapshot.items);

    /// Answers pending SelectionRequests: TARGETS and UTF8_STRING.
    fn serve_x11(conn: &impl Connection, win: u32, targets: u32, utf8: u32, text: &[u8]) {
        while let Some(event) = conn.poll_for_event().unwrap() {
            let Event::SelectionRequest(r) = event else { continue };
            let property = if r.target == targets {
                conn.change_property32(PropMode::REPLACE, r.requestor, r.property, AtomEnum::ATOM, &[targets, utf8]).unwrap();
                r.property
            } else if r.target == utf8 {
                conn.change_property8(PropMode::REPLACE, r.requestor, r.property, utf8, text).unwrap();
                r.property
            } else {
                x11rb::NONE
            };
            let notify = SelectionNotifyEvent { response_type: SELECTION_NOTIFY_EVENT, sequence: 0, time: r.time, requestor: r.requestor, selection: r.selection, target: r.target, property };
            conn.send_event(false, r.requestor, EventMask::NO_EVENT, notify).unwrap();
            conn.flush().unwrap();
        }
        let _ = win;
    }
}
