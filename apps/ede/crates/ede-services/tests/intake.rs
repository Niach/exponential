//! #EDE-172 (P13): the daemon behind the compositor. Connections come over
//! the compositor's intake, each with the capabilities of the principal
//! that asked: one holding `ui` alone cannot call a `system` action;
//! `grant` is refused on every connection; the command line's `get` and
//! `call` run in its own process with no daemon (and never ask a socket);
//! a daemon started by hand in a session whose compositor runs it serves
//! nothing; the compositor's close ends the daemon's intake.

use std::{
    os::unix::net::UnixStream,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};

use ede_protocol::{
    intake::{self, Handed},
    Capability, SessionId,
};
use ede_services::{
    socket::{serve_intake, Client},
    ActionSpec, Registry, Service,
};
use serde_json::{json, Value};

/// A domain with an action of each kind.
struct Desk;
impl Service for Desk {
    fn name(&self) -> &'static str {
        "desk"
    }
    fn description(&self) -> &'static str {
        "a test"
    }
    fn snapshot(&self) -> Value {
        json!({"available": true})
    }
    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("power_off", "ends the session", json!({}), &[], Some(Capability::System)),
            ActionSpec::new("set_volume", "sets the volume", json!({"percent": {"type": "integer"}}), &["percent"], Some(Capability::Ui)),
            ActionSpec::new("ping", "pong", json!({}), &[], None),
        ]
    }
    fn call(&self, action: &str, _: &Value) -> Result<Value, String> {
        Ok(json!({"did": action}))
    }
}

/// A daemon serving its intake on a thread: the compositor's end.
fn daemon() -> (std::os::fd::OwnedFd, thread::JoinHandle<std::io::Result<()>>) {
    let (compositor, theirs) = intake::pair().unwrap();
    let registry = Arc::new(Registry::new(vec![Box::new(Desk)]));
    let served = thread::spawn(move || serve_intake(theirs, registry));
    (compositor, served)
}

/// A connection handed to the daemon as the compositor hands it.
fn handed(compositor: &std::os::fd::OwnedFd, session: &str, capabilities: &[Capability]) -> Client {
    let (asker, theirs) = UnixStream::pair().unwrap();
    intake::send(compositor, &Handed { session_id: SessionId::new(session).unwrap(), capabilities: capabilities.to_vec() }, &theirs).unwrap();
    let client = Client::from_fd(asker.into());
    client.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    client
}

fn call(client: &mut Client, action: &str, args: Value) -> Result<Value, String> {
    client.call(json!({"op": "call", "domain": "desk", "action": action, "args": args}))
}

#[test]
fn a_connection_holds_the_capabilities_it_was_handed_with() {
    let (compositor, served) = daemon();
    let mut ui = handed(&compositor, "apps", &[Capability::Ui]);
    assert_eq!(call(&mut ui, "power_off", json!({})), Err("needs the system capability".into()), "ui alone calls no system action");
    assert_eq!(call(&mut ui, "set_volume", json!({"percent": 40})).unwrap()["did"], "set_volume");
    assert_eq!(call(&mut ui, "ping", json!({})).unwrap()["did"], "ping");
    assert_eq!(ui.get("desk").unwrap()["available"], true, "a snapshot needs no capability");

    let mut shell = handed(&compositor, "root", &Capability::ALL);
    assert_eq!(call(&mut shell, "power_off", json!({})).unwrap()["did"], "power_off", "the shell's buttons hold every capability");
    let mut none = handed(&compositor, "other", &[]);
    assert!(call(&mut none, "set_volume", json!({"percent": 1})).is_err());

    drop(compositor);
    served.join().unwrap().expect("the compositor's close ends the intake cleanly");
    // What it handed over before stays served until its asker hangs up.
    assert_eq!(call(&mut ui, "ping", json!({})).unwrap()["did"], "ping");
}

#[test]
fn grant_is_refused() {
    let (compositor, _served) = daemon();
    let mut shell = handed(&compositor, "root", &Capability::ALL);
    let refused = shell.call(json!({"op": "grant", "session": "run-1", "label": "agent", "capabilities": ["system"]})).unwrap_err();
    assert!(refused.contains("grant is gone"), "{refused}");
    let registry = Registry::new(vec![Box::new(Desk)]);
    assert!(ede_services::socket::handle(&json!({"op": "grant", "session": "run-1"}), &registry, &Capability::ALL).is_err(), "on the socket too");
}

fn cli(runtime: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_exponential-services"));
    command.env("XDG_RUNTIME_DIR", runtime).env("EXP_DIAGNOSTICS", "0").env_remove(ede_protocol::CONTROL_FD_ENV).env_remove(ede_protocol::process::HELPERS_ENV).stdin(Stdio::null());
    command
}

/// `get` and `call` run in the command's own process: no daemon is there,
/// and a listener at the socket's path is never asked.
#[test]
fn the_command_line_gets_and_calls_with_no_daemon() {
    let runtime = tempfile::tempdir().unwrap();
    let got = cli(runtime.path()).args(["get", "about"]).output().unwrap();
    assert!(got.status.success(), "{}", String::from_utf8_lossy(&got.stderr));
    let about: Value = serde_json::from_slice(&got.stdout).unwrap();
    assert!(about.get("available").is_some(), "{about}");

    let unknown = cli(runtime.path()).args(["call", "nope", "x"]).output().unwrap();
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("no domain nope"), "answered here: {}", String::from_utf8_lossy(&unknown.stderr));

    // Something listens where services.sock was, and never answers.
    let path = runtime.path().join("exponential-desktop").join("services.sock");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let mut child = cli(runtime.path()).args(["get", "about"]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(std::time::Instant::now() < deadline, "the command line waited on the socket");
        thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());

    let grant = cli(runtime.path()).args(["grant", "run-1"]).output().unwrap();
    assert_eq!(grant.status.code(), Some(2), "exponential-services grant is gone");
}

/// In a session whose compositor runs the daemon (`EDE_HELPERS_SPAWNED`),
/// one started by hand finds no intake, binds no socket and ends.
#[test]
fn a_daemon_started_by_hand_where_the_compositor_runs_it_serves_nothing() {
    let runtime = tempfile::tempdir().unwrap();
    let out = cli(runtime.path()).arg("serve").env(ede_protocol::process::HELPERS_ENV, "1").output().unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no intake"), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!runtime.path().join("exponential-desktop").join("services.sock").exists(), "no socket was bound");
}
