//! #EDE-126: a connection keeps the scope its hello gave it. A space grant
//! (`[space]` and nothing else) reaches no op of the person's desk, whatever
//! capability the op names; a person's connection lets go of its hold at
//! its drop even when its grant is gone by then.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{Capability, CaptureTarget, Client, Error, ErrorCode, InputEvent, Request, SessionId, SpaceId, Token, WindowId, PROTOCOL_VERSION};

const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn code<T: std::fmt::Debug>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn space_grant(h: &Harness, session: &str) -> (Token, Client) {
    let grant = h.ipc().mint(SessionId::new(session).unwrap(), vec![Capability::Space], None).unwrap();
    (grant.token.clone(), h.ipc_with(grant.token).unwrap())
}

/// Every op but `hello`, one request each (two where a field decides).
/// `every` has no wildcard arm: a new op fails to compile here.
fn requests(window: &WindowId) -> Vec<Request> {
    let w = || window.clone();
    let space = || SpaceId::new("space-1").unwrap();
    let key = || InputEvent::Key { key: KEY_A, pressed: true };
    let all = vec![
        Request::ListWindows,
        Request::ListOutputs,
        Request::FocusWindow { window: w() },
        Request::CloseWindow { window: w() },
        Request::MoveWindow { window: w(), x: 5, y: 5 },
        Request::MinimizeWindow { window: w() },
        Request::RestoreWindow { window: w() },
        Request::Capture { target: CaptureTarget::Output { name: "HEADLESS-1".into() }, since: None },
        Request::Capture { target: CaptureTarget::Window { window: w() }, since: None },
        Request::Capture { target: CaptureTarget::Space { space: space() }, since: None },
        Request::Input { event: key(), bindings: false },
        Request::Input { event: key(), bindings: true },
        Request::Input { event: InputEvent::PointerMotion { x: 700.0, y: 500.0 }, bindings: false },
        Request::PointerPosition,
        Request::HoldInput,
        Request::ReleaseInput,
        Request::ReloadConfig,
        Request::Lock,
        Request::Logout,
        Request::Unlock,
        Request::ListWorkspaces,
        Request::SwitchWorkspace { index: 1 },
        Request::MoveToWorkspace { window: w(), index: 1 },
        Request::SetOutput { name: "HEADLESS-1".into(), mode: None, scale: Some(2.0), position: None },
        Request::Subscribe,
        Request::MintGrant { session_id: SessionId::new("other").unwrap(), run_id: None, capabilities: vec![Capability::Space], expires_at: None },
        Request::RevokeGrant { token: Token::new("0".repeat(64)).unwrap() },
        Request::BindShortcuts { owner: "p".into(), shortcuts: vec![] },
        Request::UnbindShortcuts { owner: "p".into() },
        Request::ClipboardSerial,
        Request::ClipboardSnapshot,
        Request::ClipboardSet { items: vec![], sensitive: false },
        Request::ClipboardTaken { serial: 1 },
        Request::ClipboardText,
        Request::LaunchToken,
        Request::CreateSpace { width: 640, height: 480 },
        Request::DestroySpace { space: space() },
        Request::ListSpaces,
        Request::Launch { argv: vec!["true".into()], env: Default::default(), cwd: None },
        Request::SpaceOfProcess { pid: std::process::id() },
        Request::SpaceReadiness,
    ];
    let ops: std::collections::HashSet<std::mem::Discriminant<Request>> = all.iter().map(every).collect();
    assert_eq!(ops.len(), 37, "one request per op at least");
    all
}

fn every(request: &Request) -> std::mem::Discriminant<Request> {
    match request {
        Request::Hello { .. } | Request::ListWindows | Request::ListOutputs | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::MoveWindow { .. } => {}
        Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::Capture { .. } | Request::Input { .. } | Request::PointerPosition | Request::HoldInput => {}
        Request::ReleaseInput | Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::ListWorkspaces | Request::SwitchWorkspace { .. } => {}
        Request::MoveToWorkspace { .. } | Request::SetOutput { .. } | Request::Subscribe | Request::MintGrant { .. } | Request::RevokeGrant { .. } | Request::BindShortcuts { .. } => {}
        Request::UnbindShortcuts { .. } | Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText => {}
        Request::LaunchToken | Request::CreateSpace { .. } | Request::DestroySpace { .. } | Request::ListSpaces | Request::Launch { .. } | Request::SpaceOfProcess { .. } | Request::SpaceReadiness => {}
    }
    std::mem::discriminant(request)
}

/// What the person's desk is, as the root sees it and as the compositor holds it.
fn person(h: &Harness) -> String {
    let windows = h.ipc().windows().unwrap();
    let state = h.with(|ede| {
        let pointer = ede.desk.seat.get_pointer().unwrap().current_location();
        let keys = ede.desk.seat.get_keyboard().unwrap().pressed_keys().len();
        (pointer, keys, ede.desk.clipboard.serial, ede.person_input_held(), ede.is_locked(), ede.shell_listening(), ede.ipc.grants.len(), ede.active_workspace())
    });
    format!("{windows:?} {state:?}")
}

/// The table: under a space grant with no space yet, an op outside the
/// allowlist is `forbidden`, one inside it finds no space, and none of them
/// reaches the person's desk (before this, six ops needed no capability and
/// answered a space grant with the person's windows, outputs and pointer).
#[test]
fn a_space_grant_reaches_no_person_op() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let window = h.ipc().windows().unwrap()[0].id.clone();
    let (_, mut agent) = space_grant(&h, "run-1");
    assert_eq!(agent.hello.capabilities, [Capability::Space]);
    let before = person(&h);
    for request in requests(&window) {
        let expect = match &request {
            // #EXP-1514: before the flip read as an older compositor does
            // (space_readiness.rs); outside the allowlist with the switch.
            Request::SpaceReadiness => {
                assert!(matches!(agent.call(request.clone()), Err(Error::Unsupported(_))), "{request:?}");
                continue;
            }
            r if !r.in_space() => ErrorCode::Forbidden,
            // This harness has no directory for spaces.
            Request::CreateSpace { .. } => ErrorCode::Unavailable,
            // Its own spaces: none.
            Request::ListSpaces => {
                assert_eq!(agent.spaces().unwrap(), []);
                continue;
            }
            _ => ErrorCode::NotFound,
        };
        assert_eq!(code(agent.call(request.clone())), expect, "{request:?}");
    }
    assert_eq!(person(&h), before, "a space grant moved the person's desk");
    app.roundtrip();
    assert!(!app.state.toplevels[0].closed && app.state.keys.is_empty());
}

/// The person's side of the new ops: only a space grant creates a space or
/// launches into one; naming another's space takes `system`.
#[test]
fn a_person_grant_makes_no_space() {
    let h = harness();
    let mut root = h.ipc();
    assert_eq!(code(root.create_space(640, 480)), ErrorCode::Forbidden);
    assert_eq!(code(root.launch(vec!["true".into()], Default::default(), None)), ErrorCode::Forbidden);
    let watcher = root.mint(SessionId::new("watcher").unwrap(), vec![Capability::Capture], None).unwrap();
    let mut watcher = h.ipc_with(watcher.token).unwrap();
    let space = CaptureTarget::Space { space: SpaceId::new("space-1").unwrap() };
    assert_eq!(code(watcher.call(Request::Capture { target: space, since: None })), ErrorCode::Forbidden);
    assert_eq!(code(watcher.spaces()), ErrorCode::Forbidden);
}

/// A space grant is `[space]` alone and never carries the shell's session
/// name (the bindings and the shell's subscription go by that name).
#[test]
fn a_mixed_or_root_named_mint_is_refused() {
    let h = harness();
    let mut root = h.ipc();
    let run = || SessionId::new("run-1").unwrap();
    assert_eq!(code(root.mint(run(), vec![Capability::Space, Capability::Capture], None)), ErrorCode::Invalid);
    assert_eq!(code(root.mint(run(), vec![Capability::Space, Capability::System], None)), ErrorCode::Invalid);
    let shell = || SessionId::new("exponential-shell").unwrap();
    assert_eq!(code(root.mint(shell(), vec![Capability::Space], None)), ErrorCode::Invalid);
    assert_eq!(code(root.mint(shell(), vec![Capability::InputPerson], None)), ErrorCode::Invalid);
    assert_eq!(h.with(|ede| ede.ipc.grants.len()), 1, "a refused mint left a grant");
    assert!(root.mint(run(), vec![Capability::Space], None).is_ok());
}

/// The scope is the first hello's: a later hello on the same connection
/// cannot turn a space connection into a person's, or back.
#[test]
fn a_second_hello_cannot_change_the_scope() {
    let h = harness();
    let root_token = h.with(|ede| ede.ipc.root_grant().token);
    let (space_token, mut agent) = space_grant(&h, "run-1");
    assert_eq!(code(agent.call(Request::Hello { version: PROTOCOL_VERSION, token: root_token.clone() })), ErrorCode::Forbidden);
    assert_eq!(code(agent.windows()), ErrorCode::NotFound, "still a space connection");
    let mut root = h.ipc();
    assert_eq!(code(root.call(Request::Hello { version: PROTOCOL_VERSION, token: space_token })), ErrorCode::Forbidden);
    assert!(root.windows().is_ok(), "still the root's connection");
    assert!(root.call(Request::Hello { version: PROTOCOL_VERSION, token: root_token }).is_ok(), "the same scope again is today's hello");
}

/// A pin, green before and after: a foreground run revokes its grant before
/// its hold connection drops. The hold and the key it drove must still end
/// with the connection (a scope read from the grant at the drop would find
/// none and leave the person's input held).
#[test]
fn revoke_then_drop_releases_the_hold() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let mut root = h.ipc();
    let grant = root.mint(SessionId::new("epilot").unwrap(), vec![Capability::InputPerson], None).unwrap();
    let mut agent = h.ipc_with(grant.token.clone()).unwrap();
    agent.hold_input().unwrap();
    agent.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap();
    assert!(h.with(|ede| ede.person_input_held()));
    root.call(Request::RevokeGrant { token: grant.token }).unwrap();
    assert_eq!(code(agent.release_input()), ErrorCode::Unauthorized, "the grant is gone");
    assert!(h.with(|ede| ede.person_input_held()), "the hold lives as long as its connection");
    drop(agent);
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(|ede| ede.person_input_held() || !ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()) {
        assert!(Instant::now() < deadline, "the hold or its key outlived the connection");
        std::thread::sleep(Duration::from_millis(10));
    }
}
