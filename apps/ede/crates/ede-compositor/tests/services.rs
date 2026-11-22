//! #EDE-172 (P13), headless: the services daemon behind the compositor
//! (`[session] helpers = "compositor"`). The compositor starts it with its
//! intake at fd 3 and hands it a connection for each `open_services` a
//! grant asks for, with exactly the capabilities asked, never more than the
//! grant holds; a daemon started again gets a new intake; without a daemon
//! of its own the compositor has none to hand over (its unit serves the
//! socket).
//!
//! The daemon is this test binary started again (`EDE172_SERVICES_ROLE`).

use std::{
    io::Write,
    os::{
        fd::{FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ede_compositor::{
    backend::headless::Harness,
    helpers::{Channel, Spec},
    Config,
};
use ede_protocol::{read_frame, write_frame, Capability, Error, ErrorCode, SessionId};

const ROLE: &str = "EDE172_SERVICES_ROLE";
const OUT: &str = "EDE172_SERVICES_OUT";

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

/// The daemon stand-in as the compositor's helper.
fn daemon(out: &Path) -> Spec {
    let argv = [std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(), "role_daemon".into(), "--ignored".into(), "--nocapture".into(), "--test-threads=1".into()].into();
    Spec { name: "test-services".into(), argv, channel: Some(Channel::Intake), wayland: None, env: vec![(ROLE.into(), "daemon".into()), (OUT.into(), out.to_string_lossy().into_owned())] }
}

/// The lines the stand-in wrote to `out`, once `count` of them contain `what`.
fn report(out: &Path, what: &str, count: usize) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let text = std::fs::read_to_string(out).unwrap_or_default();
        if text.lines().filter(|l| l.contains(what)).count() >= count {
            return text;
        }
        assert!(Instant::now() < deadline, "the daemon never said {what:?} {count} times; it said {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The asker's end answers as the daemon's end of the same connection.
fn served(fd: OwnedFd) -> String {
    let mut stream = UnixStream::from(fd);
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    String::from_utf8(read_frame(&mut stream).expect("the daemon answers on the handed connection")).unwrap()
}

fn code(result: Result<OwnedFd, Error>) -> ErrorCode {
    match result {
        Err(Error::Failed(f)) => f.code,
        Ok(_) => panic!("handed over"),
        Err(e) => panic!("{e}"),
    }
}

/// The shell's root channel gets a connection holding what it asked; a
/// child holding `ui` gets one with `ui` and is refused `system`; what the
/// daemon got with each is what was asked.
#[test]
fn the_intake_carries_the_capabilities_of_the_grant_that_asked() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let spec = daemon(&out);
    let env: Vec<_> = std::env::vars_os().filter(|(k, _)| k != "LD_PRELOAD").collect();
    h.with(move |ede| {
        ede.helpers.base_env = Some(env);
        ede.start_helpers_with(vec![spec]);
    });
    report(&out, "intake", 1);
    assert!(h.with(|ede| ede.services.running()));

    let mut root = h.root_channel();
    assert_eq!(served(root.open_services(vec![Capability::Ui, Capability::System]).unwrap()), "served 1");
    let said = report(&out, "handed", 1);
    let shell_session = root.hello.session_id.as_str().to_string();
    assert!(said.contains(&format!("handed {shell_session} [\"ui\",\"system\"]")), "{said}");

    let (mut apps, _) = root.open_channel(Some(SessionId::new("apps").unwrap()), vec![Capability::Ui], None).unwrap();
    assert_eq!(code(apps.open_services(vec![Capability::System])), ErrorCode::Forbidden, "no more than the grant holds");
    assert_eq!(code(apps.open_services(vec![Capability::Ui, Capability::Capture])), ErrorCode::Forbidden);
    assert_eq!(served(apps.open_services(vec![Capability::Ui]).unwrap()), "served 2");
    let said = report(&out, "handed", 2);
    assert!(said.contains("handed apps [\"ui\"]"), "{said}");
    assert_eq!(said.lines().filter(|l| l.contains("handed")).count(), 2, "a refused ask reached the daemon: {said}");
}

/// A daemon that ends is started again with a new intake, and connections
/// are handed to the new one.
#[test]
fn a_daemon_started_again_gets_a_new_intake() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report");
    let spec = daemon(&out);
    let env: Vec<_> = std::env::vars_os().filter(|(k, _)| k != "LD_PRELOAD").collect();
    h.with(move |ede| {
        ede.helpers.base_env = Some(env);
        ede.start_helpers_with(vec![spec]);
    });
    report(&out, "intake", 1);
    let first = h.with(|ede| ede.helpers.pid("test-services")).expect("the daemon runs");
    // Safety: ending the test's own child.
    unsafe { libc::kill(first as libc::pid_t, libc::SIGKILL) };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        h.with(|ede| ede.helpers_tick());
        if h.with(|ede| ede.helpers.pid("test-services")).is_some_and(|pid| pid != first) {
            break;
        }
        assert!(Instant::now() < deadline, "the daemon was not started again");
        std::thread::sleep(Duration::from_millis(50));
    }
    report(&out, "intake", 2);
    let mut root = h.root_channel();
    assert_eq!(served(root.open_services(vec![Capability::Ui]).unwrap()), "served 1", "the new daemon took it");
}

/// Without a daemon of its own (the units run it) the compositor hands
/// over nothing.
#[test]
fn without_its_daemon_the_compositor_hands_over_nothing() {
    let h = harness();
    let mut root = h.root_channel();
    assert_eq!(code(root.open_services(vec![Capability::Ui])), ErrorCode::Unavailable);
}

/// The stand-in: take the intake at fd 3, say so, then for each connection
/// say what came with it and answer on it.
#[test]
#[ignore = "#EDE-172: a role of the services tests"]
fn role_daemon() {
    if std::env::var(ROLE).as_deref() != Ok("daemon") {
        return;
    }
    let out = PathBuf::from(std::env::var_os(OUT).unwrap());
    let say = |line: &str| {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&out).unwrap();
        file.write_all(format!("{line}\n").as_bytes()).unwrap();
    };
    let fd: i32 = std::env::var(ede_protocol::CONTROL_FD_ENV).unwrap().parse().unwrap();
    // Safety: the compositor handed this descriptor to this process for this use.
    let intake = unsafe { OwnedFd::from_raw_fd(fd) };
    say(&format!("intake at {fd}"));
    let mut kept = Vec::new();
    let mut n = 0;
    while let Some((handed, connection)) = ede_protocol::intake::receive(&intake).unwrap() {
        n += 1;
        say(&format!("handed {} {}", handed.session_id.as_str(), serde_json::to_string(&handed.capabilities).unwrap()));
        let mut stream = UnixStream::from(connection);
        write_frame(&mut stream, format!("served {n}").as_bytes()).unwrap();
        kept.push(stream);
    }
    say("intake closed");
}
