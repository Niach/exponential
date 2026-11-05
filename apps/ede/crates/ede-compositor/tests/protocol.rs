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
    assert_eq!(root.hello.session_id.as_str(), "ede-shell");
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
    assert_eq!(mode(&runtime.path().join("ede")), 0o700);
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
    assert_eq!(out.modes.len(), 1);
    assert!(out.modes[0].current && out.modes[0].preferred);
    assert_eq!((out.modes[0].width, out.modes[0].height, out.modes[0].refresh), (800, 600, 60_000));

    // A window opened on a 2× output hears the preferred scale.
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("hidpi");
    app.draw(&t, 100, 100, BLUE);
    assert!(ipc.windows().unwrap().iter().any(|w| w.title.as_deref() == Some("hidpi")));

    ipc.set_output("HEADLESS-1", None, Some(1.5), Some((10, 20))).unwrap();
    let out = &ipc.outputs().unwrap()[0];
    assert_eq!((out.scale, out.x, out.y, out.width), (1.5, 10, 20, 534), "800 / 1.5, rounded up");
    assert_eq!(code(ipc.set_output("HEADLESS-1", Some("1234x999"), None, None).unwrap_err()), ErrorCode::Invalid, "a mode the output does not offer");
    assert_eq!(code(ipc.set_output("HEADLESS-1", Some("wide"), None, None).unwrap_err()), ErrorCode::Invalid);
    assert_eq!(code(ipc.set_output("DP-9", None, Some(1.0), None).unwrap_err()), ErrorCode::Invalid);
    // Only the system capability changes outputs.
    let mut root = h.ipc();
    let grant = root.mint(SessionId::new("agent-1").unwrap(), vec![Capability::Ui], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    assert_eq!(code(agent.set_output("HEADLESS-1", None, Some(1.0), None).unwrap_err()), ErrorCode::Forbidden);
    assert!(agent.workspaces().is_ok(), "listing needs no capability");
}
