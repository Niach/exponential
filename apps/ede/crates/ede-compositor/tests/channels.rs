//! #EDE-172 (P1), channels: a grant that lives on connections instead of in
//! a string. A connection the compositor made holds its grant from the
//! start; `open_channel` hands out one more as a descriptor next to its
//! reply; a child opened so is bound (never taken by its token), no wider
//! and no longer-lived than what it was opened on, and its connections end
//! with it, with its parent, and with its parent's last connection.

mod common;

use std::{
    io::Read,
    mem::MaybeUninit,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{read_frame, write_frame, Call, Capability, Client, Error, ErrorCode, InputEvent, Message, Request, SessionId, Token, PROTOCOL_VERSION};
use smithay::reexports::rustix::net::{recvmsg, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags};

const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn code<T>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("expected a refusal, it went through"),
    }
}

fn session(name: &str) -> Option<SessionId> {
    Some(SessionId::new(name).unwrap())
}

/// `true` once a connection the compositor ended answers nothing any more.
fn ended(client: &mut Client) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match client.windows() {
            Err(Error::Io(_)) | Err(Error::Protocol(_)) => return true,
            Ok(_) | Err(_) if Instant::now() >= deadline => return false,
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// The open_channel reply, read by hand: the descriptor rides on the very
/// first bytes of the reply frame (its length), and the frame is the grant.
#[test]
fn a_descriptor_arrives_with_its_reply_frame() {
    let h = harness();
    let (mut raw, theirs) = UnixStream::pair().unwrap();
    assert!(h.with(move |ede| ede.ipc_insert_root_channel(theirs)));
    raw.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let send = |raw: &mut UnixStream, id, request| write_frame(raw, &serde_json::to_vec(&Call { id, request }).unwrap()).unwrap();
    send(&mut raw, 1, Request::Hello { version: PROTOCOL_VERSION, token: None });
    let hello: serde_json::Value = serde_json::from_slice(&read_frame(&mut raw).unwrap()).unwrap();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    assert_eq!(hello["result"]["grant"], root.as_str(), "a channel's hello names the grant it holds: {hello}");
    assert_eq!(hello["result"]["session_id"], "exponential-shell");

    send(&mut raw, 2, Request::OpenChannel { session_id: session("apps"), capabilities: vec![Capability::Ui], expires_at: None });
    let mut length = [0u8; 4];
    let mut space = [MaybeUninit::uninit(); smithay::reexports::rustix::cmsg_space!(ScmRights(2))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let got = recvmsg(&raw, &mut [std::io::IoSliceMut::new(&mut length)], &mut control, RecvFlags::CMSG_CLOEXEC).unwrap();
    assert_eq!(got.bytes, 4);
    let fds: Vec<_> = control.drain().flat_map(|m| match m {
        RecvAncillaryMessage::ScmRights(fds) => fds.collect::<Vec<_>>(),
        _ => vec![],
    }).collect();
    assert_eq!(fds.len(), 1, "one descriptor, on the reply's first bytes");
    let mut frame = vec![0u8; u32::from_le_bytes(length) as usize];
    raw.read_exact(&mut frame).unwrap();
    let reply: Message = serde_json::from_slice(&frame).unwrap();
    let Message::Reply { id: 2, result } = reply else { panic!("the reply to 2, got {reply:?}") };
    assert_eq!((result["sessionId"].as_str(), result["capabilities"].clone()), (Some("apps"), serde_json::json!(["ui"])));

    // The descriptor is a connection holding that grant: a tokenless hello.
    let child = Client::from_fd(fds.into_iter().next().unwrap()).unwrap();
    assert_eq!(child.hello.capabilities, [Capability::Ui]);
    assert_eq!(child.token().as_str(), result["token"].as_str().unwrap(), "its id, as the reply named it");

    // The client does the same: a sibling on the caller's own grant, and a child.
    let mut shell = h.root_channel();
    assert_eq!(shell.token(), &root);
    let (mut sibling, same) = shell.open_channel(None, shell.hello.capabilities.clone(), None).unwrap();
    assert_eq!((same.token, sibling.hello.capabilities.len()), (root, Capability::ALL.len()));
    assert!(sibling.outputs().is_ok());
    let (mut apps, grant) = shell.open_channel(session("apps"), vec![Capability::Ui], None).unwrap();
    assert_eq!((apps.hello.session_id.as_str(), apps.hello.capabilities.as_slice()), ("apps", &[Capability::Ui][..]));
    assert_eq!(code(apps.capture(ede_protocol::CaptureTarget::Output { name: "HEADLESS-1".into() })), ErrorCode::Forbidden, "it holds what it was opened with");
    assert!(h.with(move |ede| ede.ipc.is_bound(&grant.token)));
}

/// A bound grant is never a way in by its token; a channel names no token,
/// and a socket connection's hello without one holds nothing.
#[test]
fn a_bound_grants_token_is_refused_at_hello() {
    let h = harness();
    let mut shell = h.root_channel();
    let (mut apps, grant) = shell.open_channel(session("apps"), vec![Capability::Ui, Capability::Capture], None).unwrap();
    assert!(apps.windows().is_ok());
    assert_eq!(code(h.ipc_with(grant.token.clone())), ErrorCode::Unauthorized, "its token is its id, not a bearer");
    let mut root = h.ipc();
    assert_eq!(code(root.call(Request::Hello { version: PROTOCOL_VERSION, token: Some(grant.token.clone()) })), ErrorCode::Unauthorized);
    assert!(root.windows().is_ok(), "the refused hello left the connection as it was");
    assert_eq!(code(apps.call(Request::Hello { version: PROTOCOL_VERSION, token: Some(root.token().clone()) })), ErrorCode::Forbidden, "a channel never takes another grant");
    assert_eq!(apps.hello.capabilities, [Capability::Capture, Capability::Ui]);
    let (ours, theirs) = UnixStream::pair().unwrap();
    h.with(move |ede| ede.ipc_insert(theirs));
    let mut anonymous = Client::from_fd(ours.into()).unwrap();
    assert!(anonymous.grant().is_none() && anonymous.hello.capabilities.is_empty(), "a socket connection without a token holds nothing (P2: anonymous)");
    assert_eq!(code(anonymous.windows()), ErrorCode::Forbidden);
}

/// A child holds no more than what it was opened on and expires no later;
/// it never holds `space` or bears the shell's name, and a channel on the
/// caller's own grant holds it as it is.
#[test]
fn a_child_is_no_wider_and_no_longer_lived() {
    let h = harness();
    let mut shell = h.root_channel();
    let soon = now_ms() + 60_000;
    let (mut run, grant) = shell.open_channel(session("run-1"), vec![Capability::Capture, Capability::Ui], Some(soon)).unwrap();
    assert_eq!(grant.expires_at, Some(soon));
    let open = |run: &mut Client, caps: Vec<Capability>, expires_at: Option<i64>| run.open_channel(session("run-1-a"), caps, expires_at).map(|(_, g)| g);
    assert_eq!(code(open(&mut run, vec![Capability::Capture, Capability::InputPerson], Some(soon))), ErrorCode::Forbidden, "wider");
    assert_eq!(code(open(&mut run, vec![Capability::Capture], None)), ErrorCode::Forbidden, "never expiring under one that does");
    assert_eq!(code(open(&mut run, vec![Capability::Capture], Some(soon + 1))), ErrorCode::Forbidden, "longer-lived");
    assert_eq!(code(open(&mut run, vec![Capability::Capture], Some(now_ms() - 1))), ErrorCode::Invalid, "expired already");
    assert_eq!(code(run.open_channel(None, vec![Capability::Capture], None)), ErrorCode::Invalid, "the caller's own grant, narrowed, names a session");
    assert_eq!(code(shell.open_channel(session("run-2"), vec![Capability::Space], None)), ErrorCode::Invalid, "a space grant is minted");
    assert_eq!(code(shell.open_channel(session("exponential-shell"), vec![Capability::Ui], None)), ErrorCode::Invalid, "the shell's name");
    let narrower = open(&mut run, vec![Capability::Capture], Some(soon)).unwrap();
    assert_eq!(narrower.capabilities, [Capability::Capture]);
    // A space connection opens nothing.
    let space = h.ipc().mint(SessionId::new("run-9").unwrap(), vec![Capability::Space], None).unwrap();
    let mut space = h.ipc_with(space.token).unwrap();
    assert_eq!(code(space.open_channel(None, vec![Capability::Space], None)), ErrorCode::Forbidden);

    // Expiry holds on a channel as on a bearer grant.
    let (mut short, _) = shell.open_channel(session("run-3"), vec![Capability::Ui], Some(now_ms() + 300)).unwrap();
    assert!(short.windows().is_ok());
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(code(short.windows()), ErrorCode::Unauthorized, "the grant expired");
}

/// Revoking a grant ends what was opened on it, at any depth; a bound
/// grant's own connections end too (by its id, also sent as `grant`), a
/// bearer grant's keep today's rule. A principal whose last connection
/// closed is dropped, and its children with it.
#[test]
fn revoking_a_parent_ends_its_childrens_connections() {
    let h = harness();
    let mut shell = h.root_channel();
    let (mut parent, parent_grant) = shell.open_channel(session("run-1"), vec![Capability::Ui, Capability::Capture], None).unwrap();
    let (mut again, _) = parent.open_channel(None, vec![Capability::Ui, Capability::Capture], None).unwrap();
    let (mut child, _) = parent.open_channel(session("run-1-a"), vec![Capability::Ui], None).unwrap();
    let (mut grandchild, _) = child.open_channel(session("run-1-b"), vec![Capability::Ui], None).unwrap();
    let (mut other, other_grant) = shell.open_channel(session("run-2"), vec![Capability::Ui], None).unwrap();
    // A holder without `system` ends what it opened, never another lineage.
    let (mut opened, opened_grant) = parent.open_channel(session("run-1-c"), vec![Capability::Capture], None).unwrap();
    parent.revoke_grant(&opened_grant.token).unwrap();
    assert!(ended(&mut opened), "the child its holder revoked");
    assert_eq!(code(parent.revoke_grant(&other_grant.token)), ErrorCode::Forbidden, "another lineage's");
    assert_eq!(code(parent.revoke_grant(&Token::new("0".repeat(64)).unwrap())), ErrorCode::Forbidden, "an unknown one is not told apart");
    assert!(parent.windows().is_ok() && other.windows().is_ok());
    let raw = serde_json::json!({"id": 1, "op": "revoke_grant", "grant": parent_grant.token.as_str()});
    let call: Call = serde_json::from_value(raw).unwrap();
    shell.call(call.request).unwrap();
    for (name, client) in [("parent", &mut parent), ("its second connection", &mut again), ("child", &mut child), ("grandchild", &mut grandchild)] {
        assert!(ended(client), "the {name}'s connection outlived the revoke");
    }
    assert!(other.windows().is_ok(), "another lineage is untouched");
    assert_eq!(code(shell.revoke_grant(&parent_grant.token)), ErrorCode::NotFound, "gone");

    // A bearer grant (minted): its children end, its own connection keeps going until it drops.
    let mut root = h.ipc();
    let minted = root.mint(SessionId::new("daemon").unwrap(), vec![Capability::System, Capability::Capture], None).unwrap();
    let mut daemon = h.ipc_with(minted.token.clone()).unwrap();
    let (mut opened, _) = daemon.open_channel(session("daemon-run"), vec![Capability::Capture], None).unwrap();
    root.revoke_grant(&minted.token).unwrap();
    assert!(ended(&mut opened), "the bearer's child outlived it");
    assert_eq!(code(daemon.windows()), ErrorCode::Unauthorized, "the bearer's connection is refused, not ended");

    // Dropped: the last connection of a principal goes, so do its children.
    let (principal, _) = shell.open_channel(session("run-4"), vec![Capability::Ui], None).unwrap();
    let mut principal = principal;
    let (mut run, run_grant) = principal.open_channel(session("run-4-a"), vec![Capability::Ui], None).unwrap();
    drop(principal);
    assert!(ended(&mut run), "a child outlived its principal's last connection");
    assert!(!h.with(move |ede| ede.ipc.is_bound(&run_grant.token) || ede.ipc.grants.iter().any(|g| g.token == run_grant.token)));
    assert!(shell.windows().is_ok() && other.windows().is_ok());
}

/// A channel that drops (or is ended by a revoke) lets go of the person's
/// input it held and the keys it pressed, as any connection does.
#[test]
fn a_dropped_channel_releases_its_holds_and_pressed_keys() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.keyboard_focus.is_none() {
        assert!(Instant::now() < deadline, "the window never took focus");
        std::thread::sleep(Duration::from_millis(10));
        app.roundtrip();
    }
    let released = |h: &Harness| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while h.with(|ede| ede.person_input_held() || !ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    };
    let mut shell = h.root_channel();
    for revoke in [false, true] {
        let (mut run, grant) = shell.open_channel(session("run-1"), vec![Capability::InputPerson], None).unwrap();
        run.hold_input().unwrap();
        run.input(InputEvent::Key { key: KEY_A, pressed: true }).unwrap();
        assert!(h.with(|ede| ede.person_input_held() && !ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()));
        if revoke {
            shell.revoke_grant(&grant.token).unwrap();
            assert!(ended(&mut run));
        } else {
            drop(run);
        }
        assert!(released(&h), "the hold or its key outlived the channel (revoked: {revoke})");
    }
    assert!(shell.windows().is_ok(), "the root channel goes on");
}

/// `--test-token-file`: a test compositor writes its root token there too,
/// 0600; `--control-fd` is a root channel. The binary, as a script runs it.
#[test]
fn a_test_compositor_writes_its_token_where_asked_and_serves_its_control_fd() {
    use std::os::{fd::AsRawFd, unix::fs::PermissionsExt, unix::process::CommandExt};
    let dir = tempfile::tempdir().unwrap();
    let runtime = dir.path().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let token_file = dir.path().join("token");
    let (ours, theirs) = UnixStream::pair().unwrap();
    let fd = theirs.as_raw_fd();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-compositor"));
    command
        .args(["--headless", "320x200", "--no-xwayland", "--safe-mode", "--control-fd", "3", "--test-token-file"])
        .arg(&token_file)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", dir.path())
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("EXP_DIAGNOSTICS", "0")
        .env("RUST_LOG", "warn");
    // Safety: dup2 and fcntl only, between fork and exec.
    unsafe {
        command.pre_exec(move || {
            // At 3 already, the pair's close-on-exec must go; else dup2 drops it.
            let ok = if fd == 3 { libc::fcntl(3, libc::F_SETFD, 0) == 0 } else { libc::dup2(fd, 3) == 3 };
            if ok {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn().unwrap();
    drop(theirs);
    let mut shell = Client::from_fd(ours.into()).unwrap();
    assert!(shell.hello.capabilities.contains(&Capability::System), "the control fd holds the root grant");
    assert_eq!(shell.outputs().unwrap()[0].width, 320);
    let mode = std::fs::metadata(&token_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let token = Token::new(std::fs::read_to_string(&token_file).unwrap().trim()).unwrap();
    assert_eq!(&token, shell.token(), "the same root grant");
    shell.logout().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the compositor did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!token_file.exists(), "removed with the compositor");
}
