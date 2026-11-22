//! #EDE-172 (P2), the anonymous level: a hello without a token on a socket
//! connection holds no grant. It reads the outputs (and whether a space is
//! ready), and every other op is refused; it never becomes more on the same
//! connection. `space_of_process` takes any grant.

mod common;

use std::{os::unix::net::UnixStream, time::Duration};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{Capability, CaptureTarget, Client, Error, ErrorCode, InputEvent, Request, SessionId, SpaceId, Token, WindowId, PROTOCOL_VERSION};

const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn anonymous(h: &Harness) -> Client {
    let (ours, theirs) = UnixStream::pair().unwrap();
    h.with(move |ede| ede.ipc_insert(theirs));
    Client::from_fd(ours.into()).expect("an anonymous hello")
}

fn code<T>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("expected a refusal, it went through"),
    }
}

/// Every op but `hello`, one request each; `every` has no wildcard arm, so
/// a new op fails to compile here until it is placed.
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
        Request::Input { event: key(), bindings: false },
        Request::Input { event: key(), bindings: true },
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
        Request::MintGrant { session_id: SessionId::new("other").unwrap(), run_id: None, capabilities: vec![Capability::Capture], expires_at: None },
        Request::RevokeGrant { token: Token::new("0".repeat(64)).unwrap() },
        Request::OpenChannel { session_id: None, capabilities: vec![], expires_at: None },
        Request::OpenWayland,
        Request::OpenServices { capabilities: vec![Capability::System] },
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
        Request::SpaceBusy,
    ];
    let ops: std::collections::HashSet<std::mem::Discriminant<Request>> = all.iter().map(every).collect();
    assert_eq!(ops.len(), 41, "one request per op at least");
    all
}

fn every(request: &Request) -> std::mem::Discriminant<Request> {
    match request {
        Request::Hello { .. } | Request::ListWindows | Request::ListOutputs | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::MoveWindow { .. } => {}
        Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::Capture { .. } | Request::Input { .. } | Request::PointerPosition | Request::HoldInput => {}
        Request::ReleaseInput | Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::ListWorkspaces | Request::SwitchWorkspace { .. } => {}
        Request::MoveToWorkspace { .. } | Request::SetOutput { .. } | Request::Subscribe | Request::MintGrant { .. } | Request::RevokeGrant { .. } | Request::OpenChannel { .. } | Request::OpenWayland | Request::OpenServices { .. } => {}
        Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } | Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } => {}
        Request::ClipboardText | Request::LaunchToken | Request::CreateSpace { .. } | Request::DestroySpace { .. } | Request::ListSpaces | Request::Launch { .. } => {}
        Request::SpaceOfProcess { .. } | Request::SpaceReadiness | Request::SpaceBusy => {}
    }
    std::mem::discriminant(request)
}

/// What the person's desk is, as the compositor holds it.
fn person(h: &Harness) -> String {
    let windows = h.ipc().windows().unwrap();
    let state = h.with(|ede| {
        let pointer = ede.desk.seat.get_pointer().unwrap().current_location();
        let keys = ede.desk.seat.get_keyboard().unwrap().pressed_keys().len();
        (pointer, keys, ede.desk.clipboard.serial, ede.person_input_held(), ede.is_locked(), ede.shell_listening(), ede.ipc.grants.len(), ede.active_workspace())
    });
    format!("{windows:?} {state:?}")
}

/// The table: outside the allowlist every op is `forbidden`, and none of
/// them moves the person's desk; the outputs come back.
#[test]
fn an_anonymous_connection_asks_only_the_allowlist() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let window = h.ipc().windows().unwrap()[0].id.clone();
    let mut anonymous = anonymous(&h);
    assert!(anonymous.hello.capabilities.is_empty() && anonymous.grant().is_none(), "{:?}", anonymous.hello);
    assert_eq!(anonymous.hello.session_id.as_str(), "anonymous");
    let before = person(&h);
    for request in requests(&window) {
        match &request {
            Request::ListOutputs => assert_eq!(anonymous.outputs().unwrap().len(), 1),
            // This harness makes no spaces: it reads the op as an older
            // compositor does (space_readiness.rs has the spaces' answer).
            Request::SpaceReadiness => assert!(matches!(anonymous.call(request.clone()), Err(Error::Unsupported(_))), "{request:?}"),
            r => {
                assert!(!r.anonymous(), "{r:?} is on the allowlist and has no row here");
                assert_eq!(code(anonymous.call(request.clone())), ErrorCode::Forbidden, "{request:?}");
            }
        }
    }
    assert_eq!(person(&h), before, "an anonymous connection moved the person's desk");
    std::thread::sleep(Duration::from_millis(100));
    app.roundtrip();
    assert!(!app.state.toplevels[0].closed && app.state.keys.is_empty());
}

/// Anonymous stays anonymous: a later hello with a token on the same
/// connection is refused, and a connection that said hello with a token
/// does not turn anonymous.
#[test]
fn anonymous_is_for_the_whole_connection() {
    let h = harness();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    let mut anonymous = anonymous(&h);
    assert_eq!(code(anonymous.call(Request::Hello { version: PROTOCOL_VERSION, token: Some(root) })), ErrorCode::Forbidden);
    assert_eq!(code(anonymous.windows()), ErrorCode::Forbidden, "still anonymous");
    let mut shell = h.ipc();
    assert_eq!(code(shell.call(Request::Hello { version: PROTOCOL_VERSION, token: None })), ErrorCode::Forbidden);
    assert!(shell.windows().is_ok(), "still the root's connection");
}

/// `space_of_process` takes any grant now (#EDE-172: the polkit agent's
/// channel will hold no capability); a space grant and an anonymous
/// connection still cannot ask it.
#[test]
fn space_of_process_takes_any_grant() {
    let h = harness();
    let mut root = h.ipc();
    let nothing = root.mint(SessionId::new("polkit-agent").unwrap(), vec![], None).unwrap();
    let mut agent = h.ipc_with(nothing.token).unwrap();
    assert!(agent.space_of_process(std::process::id()).unwrap().space.is_none());
    let space = root.mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut space = h.ipc_with(space.token).unwrap();
    assert_eq!(code(space.space_of_process(std::process::id())), ErrorCode::Forbidden);
    assert_eq!(code(anonymous(&h).space_of_process(std::process::id())), ErrorCode::Forbidden);
}

/// On the real socket: `Client::anonymous_at` says hello without a token
/// and reads the outputs; the connection's peer is this process.
#[test]
fn the_socket_takes_an_anonymous_hello_and_knows_its_peer() {
    let h = harness();
    let runtime = tempfile::tempdir().unwrap();
    let r = runtime.path().to_path_buf();
    let socket = h.with(move |ede| ede.ipc_listen(&r, "wayland-9")).unwrap();
    let mut anonymous = Client::anonymous_at(&socket).unwrap();
    assert_eq!(anonymous.outputs().unwrap()[0].width, 800);
    assert_eq!(code(anonymous.capture(CaptureTarget::Output { name: "HEADLESS-1".into() })), ErrorCode::Forbidden);
    let pids = h.with(|ede| ede.ipc_peers().into_iter().map(|p| p.0).collect::<Vec<_>>());
    assert!(pids.contains(&std::process::id()), "{pids:?}");
}
