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
