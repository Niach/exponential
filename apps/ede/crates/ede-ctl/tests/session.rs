//! #EDE-172 (P7): `exponential-ctl session` runs a batch of commands from
//! stdin on ONE connection (the person is asked once per process), and an
//! `exponential-ctl` the compositor refuses writes nothing.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use ede_compositor::{backend::headless::Harness, Config};

fn listening(runtime: &Path) -> Harness {
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let runtime = runtime.to_path_buf();
    h.with(move |state| {
        state.ipc_listen(&runtime, "wayland-test").expect("the control socket");
    });
    h
}

fn ctl(runtime: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exponential-ctl"));
    cmd.env("XDG_RUNTIME_DIR", runtime).env("WAYLAND_DISPLAY", "wayland-test").env_remove("EDE_TOKEN");
    cmd
}

/// The connections exponential-ctl process `pid` holds on the control socket.
fn connections_of(h: &Harness, pid: u32) -> usize {
    h.with(move |state| state.ipc_peers().into_iter().filter(|(p, _)| *p == pid).count())
}

#[test]
fn a_session_runs_its_commands_on_one_connection() {
    let dir = tempfile::tempdir().unwrap();
    let h = listening(dir.path());
    let shot: PathBuf = dir.path().join("my shot.png");
    let mut child = ctl(dir.path()).arg("session").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let pid = child.id();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();

    writeln!(stdin, "# the outputs first").unwrap();
    writeln!(stdin, "outputs").unwrap();
    stdin.flush().unwrap();
    let first = out.next().unwrap().unwrap();
    assert!(first.contains("\"HEADLESS-1\""), "{first}");
    assert_eq!(connections_of(&h, pid), 1, "one connection");

    writeln!(stdin, "capture --png '{}'", shot.display()).unwrap();
    writeln!(stdin, "pointer 10 10").unwrap();
    writeln!(stdin, "no-such-command").unwrap();
    writeln!(stdin, "outputs").unwrap();
    stdin.flush().unwrap();
    let captured = out.next().unwrap().unwrap();
    assert!(captured.contains("\"width\":800"), "{captured}");
    // The last line's answer: every line before it ran.
    let last = out.next().unwrap().unwrap();
    assert!(last.contains("\"HEADLESS-1\""), "{last}");
    assert_eq!(h.with(|state| state.desk.seat.get_pointer().unwrap().current_location()), (10.0, 10.0).into(), "the pointer line ran");
    assert_eq!(connections_of(&h, pid), 1, "still the one connection, also after a line that failed");

    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a failed line fails the session");
    assert!(stderr.contains("line 5:") && !stderr.contains("line 4:") && !stderr.contains("line 3:"), "only the bad line is named: {stderr}");
    assert!(shot.exists(), "the capture line wrote its picture (a quoted path with a space)");
}

#[test]
fn a_refused_exponential_ctl_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let h = listening(dir.path());
    // The gate on, no policy file, nobody to ask: this build of
    // exponential-ctl is not the root-owned installed one, so it is refused.
    let policy = dir.path().join("no-policy.toml");
    h.with(move |state| {
        state.ipc.consent.on = true;
        state.ipc.consent.policy_path = Some(policy);
    });
    let shot = dir.path().join("shot.png");
    let output = ctl(dir.path()).args(["capture", "--png"]).arg(&shot).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a refused capture fails");
    assert!(stderr.contains("did not let exponential-ctl in"), "{stderr}");
    assert!(!shot.exists(), "a refused capture wrote a file");
    let mut session = ctl(dir.path()).arg("session").stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    writeln!(session.stdin.take().unwrap(), "capture --png {}", shot.display()).unwrap();
    assert!(!session.wait_with_output().unwrap().status.success());
    assert!(!shot.exists(), "a refused session wrote a file");
}
