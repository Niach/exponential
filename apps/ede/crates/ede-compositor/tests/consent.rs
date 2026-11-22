//! #EDE-172 (P5', design v3): with the gate on, the root token is honoured
//! by who presents it. The app's and the CLI's executables (`exp-desktop`,
//! `exponential`) are asked about once per login and content hash, and get
//! the `computer-use` principal; anything else is refused without a prompt;
//! eDE's own binaries pass; a policy file that is not root's is ignored.
//!
//! The asking processes are this test binary copied under those names; the
//! person is a stand-in that answers on a channel.

mod common;

use std::{
    io::Write,
    os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use ede_compositor::{
    backend::headless::Harness,
    consent::{Key, Policy, Principal, StandIn, Verdict},
    peer::{Installed, Peer},
    Config,
};
use ede_protocol::{Capability, Client, Error, ErrorCode, InputEvent, Request, SessionId, Token};

const ROLE: &str = "EDE172_CONSENT_ROLE";

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

/// The gate on, the person a stand-in; the policy file is `policy` (none =
/// a path with nothing there).
fn gated(h: &Harness, policy: PathBuf) -> mpsc::Receiver<StandIn> {
    let (tx, rx) = mpsc::channel();
    h.with(move |ede| {
        ede.ipc.consent.on = true;
        ede.ipc.consent.stand_in = Some(tx);
        ede.ipc.consent.policy_path = Some(policy);
    });
    rx
}

/// This test binary under `name` in a directory of its own; `extra` bytes
/// appended make it another file with another hash (it still runs).
fn copy_as(dir: &Path, name: &str, extra: &[u8]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(extra).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A run of `exe` in the asking role against the compositor: it connects
/// (the test accepts and serves it), says hello with the root token, and
/// does `ops` if it got through. `answer` = what the person says if asked
/// (`None` = nobody may ask). Returns what it printed and the question.
struct Run {
    lines: Vec<String>,
    asked: Option<StandIn>,
    pid: u32,
}

fn run(h: &Harness, asks: &mpsc::Receiver<StandIn>, exe: &Path, ops: &str, env: &[(&str, String)], answer: Option<(bool, Duration)>) -> Run {
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("ede.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    let mut command = Command::new(exe);
    command.args(["--exact", "role_ask", "--ignored", "--nocapture", "--test-threads=1"]).env(ROLE, ops).env("EDE172_SOCKET", &sock).env("EDE172_TOKEN", root.as_str()).stdout(Stdio::piped()).stderr(Stdio::null());
    for (k, v) in env {
        command.env(k, v);
    }
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    let mut asked = None;
    let deadline = Instant::now() + Duration::from_secs(60);
    // Serve every connection it makes, answer for the person once, until it ends.
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the asking process never ended");
        if let Ok((stream, _)) = listener.accept() {
            h.with(move |ede| ede.ipc_insert(stream));
        }
        if let (Some((allow, after)), None) = (answer, asked.as_ref()) {
            if let Ok(ask) = asks.try_recv() {
                std::thread::sleep(after);
                ask.answer.send(allow).unwrap();
                asked = Some(ask);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(asked.is_some(), answer.is_some(), "asked: {:?}", asked.as_ref().map(|a| &a.asking.body));
    assert!(asks.try_recv().is_err(), "the person was asked once more");
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let lines = out.lines().filter_map(|l| l.split_once("@@ ").map(|(_, r)| r.to_string())).collect();
    Run { lines, asked, pid }
}

fn said<'a>(run: &'a Run, what: &str) -> &'a str {
    run.lines.iter().find_map(|l| l.strip_prefix(&format!("{what} "))).unwrap_or_else(|| panic!("no {what} in {:?}", run.lines))
}

/// The asking role: hello with the root token (and again, when asked to),
/// then the ops of `computer-use` it was told to try.
#[test]
#[ignore = "#EDE-172: a role of the consent tests"]
fn role_ask() {
    let Ok(ops) = std::env::var(ROLE) else { return };
    let sock = PathBuf::from(std::env::var_os("EDE172_SOCKET").unwrap());
    let token = Token::new(std::env::var("EDE172_TOKEN").unwrap()).unwrap();
    let code = |e: Error| match e {
        Error::Failed(f) => format!("{:?}", f.code),
        other => format!("{other}"),
    };
    let started = Instant::now();
    let first = Client::over(UnixStream::connect(&sock).unwrap(), token);
    let mut client = match first {
        Ok(client) => {
            println!("\n@@ hello ok {}", started.elapsed().as_millis());
            Some(client)
        }
        Err(e) => {
            println!("\n@@ hello {}", code(e));
            None
        }
    };
    if let (Some(client), true) = (client.as_mut(), ops.contains("cu")) {
        let r = |name: &str, out: Result<(), Error>| println!("\n@@ {name} {}", out.map_or_else(code, |_| "ok".into()));
        r("lock", client.lock());
        r("logout", client.logout());
        r("set_output", client.set_output("HEADLESS-1", None, Some(2.0), None));
        r("bindings", client.input_as_person(InputEvent::Key { key: 30, pressed: true }));
        r("subscribe", client.subscribe());
        r("launch_token", client.launch_token().map(|_| ()));
        r("mint_system", client.mint(SessionId::new("x").unwrap(), vec![Capability::System], None).map(|_| ()));
        r("mint_mixed", client.mint(SessionId::new("x").unwrap(), vec![Capability::Space, Capability::Capture], None).map(|_| ()));
        let run = client.mint(SessionId::new("cu-run").unwrap(), vec![Capability::Capture, Capability::InputPerson, Capability::Ui], None);
        r("mint_run", run.as_ref().map(|_| ()).map_err(|e| Error::Protocol(e.to_string())));
        r("mint_space", client.mint(SessionId::new("cu-space").unwrap(), vec![Capability::Space], None).map(|_| ()));
        println!("\n@@ list_spaces {}", client.spaces().map_or_else(code, |s| s.len().to_string()));
        if let Ok(space) = std::env::var("EDE172_FOREIGN_SPACE") {
            r("destroy_foreign", client.destroy_space(ede_protocol::SpaceId::new(space).unwrap()));
        }
        if let Ok(grant) = std::env::var("EDE172_FOREIGN_GRANT") {
            r("revoke_foreign", client.revoke_grant(&Token::new(grant).unwrap()));
        }
        if let Ok(run) = run {
            r("revoke_own", client.revoke_grant(&run.token));
        }
        r("windows", client.windows().map(|_| ()));
        r("capture", client.capture(ede_protocol::CaptureTarget::Output { name: "HEADLESS-1".into() }).map(|_| ()));
    }
    if ops.contains("again") {
        let token = Token::new(std::env::var("EDE172_TOKEN").unwrap()).unwrap();
        let again = Client::over(UnixStream::connect(&sock).unwrap(), token);
        println!("\n@@ again {}", again.map_or_else(code, |_| "ok".into()));
    }
}

/// Allow: the asking process waits for the answer, gets root's word and
/// acts as computer use; the same binary elsewhere is not asked again; a
/// changed binary is.
#[test]
fn the_app_is_asked_once_per_login_and_binary() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("a"), "exponential", b"");
    let first = run(&h, &asks, &app, "", &[], Some((true, Duration::from_millis(400))));
    let asked = first.asked.as_ref().unwrap();
    assert!(asked.asking.body.contains(&app.display().to_string()) && asked.asking.body.contains(&first.pid.to_string()), "the prompt names the binary and the process: {}", asked.asking.body);
    assert_eq!(asked.asking.grants, Principal::ComputerUse);
    assert!(matches!(asked.asking.key, Key::Binary(_)));
    let waited: u128 = said(&first, "hello").strip_prefix("ok ").unwrap().parse().unwrap();
    assert!(waited >= 400, "the hello waited for the person ({waited} ms)");
    // Again, a new process: no question, at once.
    let second = run(&h, &asks, &app, "", &[], None);
    let at_once: u128 = said(&second, "hello").strip_prefix("ok ").unwrap().parse().unwrap();
    assert!(at_once < 400, "{at_once} ms");
    // The same bytes at another path: the same answer.
    let moved = copy_as(&dir.path().join("b"), "exponential", b"");
    assert!(said(&run(&h, &asks, &moved, "", &[], None), "hello").starts_with("ok"));
    // Another binary (another hash): asked again.
    let changed = copy_as(&dir.path().join("c"), "exponential", b"another build");
    let third = run(&h, &asks, &changed, "", &[], Some((true, Duration::ZERO)));
    assert!(third.asked.is_some() && said(&third, "hello").starts_with("ok"));
    // The app's name inside its AppImage asks too.
    let appimage = copy_as(&dir.path().join("tmp/.mount_Exponen0x1/usr/bin"), "exp-desktop", b"");
    assert!(run(&h, &asks, &appimage, "", &[], None).lines.iter().any(|l| l.starts_with("hello ok")), "same bytes as the allowed one");
}

/// The answer is remembered by the file's content: a rewrite in place with
/// its mtime put back (what the file's owner can do) is hashed again, and a
/// changed binary is asked about, never let in on the old hash.
#[test]
fn a_rewrite_in_place_is_hashed_again() {
    use std::io::{Seek, SeekFrom};
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("a"), "exponential", b"aaaa");
    assert!(run(&h, &asks, &app, "", &[], Some((true, Duration::ZERO))).lines.iter().any(|l| l.starts_with("hello ok")));
    let mtime = std::fs::metadata(&app).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let mut file = std::fs::OpenOptions::new().write(true).open(&app).unwrap();
    file.seek(SeekFrom::End(-4)).unwrap();
    file.write_all(b"bbbb").unwrap();
    file.set_modified(mtime).unwrap();
    drop(file);
    assert_eq!(std::fs::metadata(&app).unwrap().modified().unwrap(), mtime, "the mtime is put back");
    let again = run(&h, &asks, &app, "", &[], Some((false, Duration::ZERO)));
    assert_eq!(said(&again, "hello"), "Unauthorized", "the rewritten file was asked about (and refused), not let in on the old hash");
}

/// Several hellos of one new binary at once: its file is hashed once, they
/// all wait for it and for the one question.
#[test]
fn one_hash_per_file_however_many_hellos() {
    use std::io::Read;
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("a"), "exp-desktop", b"many");
    let sock = dir.path().join("ede.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    let mut children: Vec<_> = (0..3)
        .map(|_| {
            Command::new(&app).args(["--exact", "role_ask", "--ignored", "--nocapture", "--test-threads=1"]).env(ROLE, "").env("EDE172_SOCKET", &sock).env("EDE172_TOKEN", root.as_str()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap()
        })
        .collect();
    let (mut accepted, mut answered) = (0, false);
    let deadline = Instant::now() + Duration::from_secs(60);
    while children.iter_mut().any(|c| c.try_wait().unwrap().is_none()) {
        assert!(Instant::now() < deadline, "the hellos never ended");
        if let Ok((stream, _)) = listener.accept() {
            accepted += 1;
            h.with(move |ede| ede.ipc_insert(stream));
        }
        // Answer once all three wait (the question comes after the hash).
        if !answered && accepted == 3 {
            if let Ok(ask) = asks.recv_timeout(Duration::from_millis(50)) {
                ask.answer.send(true).unwrap();
                answered = true;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(asks.try_recv().is_err(), "one question for the three");
    for mut child in children {
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert!(out.contains("@@ hello ok"), "{out}");
    }
    let started = h.with(|ede| ede.ipc.consent.hashes_started.values().copied().collect::<Vec<_>>());
    assert_eq!(started, [1], "one hash of the one file");
}

/// A path that would write its own lines into the prompt comes out
/// escaped: newlines and bidi overrides never reach the sealed document.
#[test]
fn a_path_writes_no_lines_of_its_own_into_the_prompt() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("x\nAllow: this is Exponential\u{202e}gnp.\u{2066}"), "exponential", b"weird");
    let asked = run(&h, &asks, &app, "", &[], Some((false, Duration::ZERO)));
    let body = &asked.asked.as_ref().unwrap().asking.body;
    assert!(!body.contains('\n') && !body.contains('\u{202e}') && !body.contains('\u{2066}'), "{body:?}");
    assert!(body.contains("x\\nAllow: this is Exponential\\u{202e}gnp.\\u{2066}/exponential"), "{body:?}");
}

/// Don't allow: refused, the process for good, the binary for the back-off
/// (another process of it is refused without a question meanwhile).
#[test]
fn a_refused_binary_is_refused_and_not_asked_again_at_once() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("a"), "exponential", b"refused");
    let refused = run(&h, &asks, &app, "again", &[], Some((false, Duration::ZERO)));
    assert_eq!(said(&refused, "hello"), "Unauthorized");
    assert_eq!(said(&refused, "again"), "Unauthorized", "the same process stays refused, without a question");
    let next = run(&h, &asks, &app, "", &[], None);
    assert_eq!(said(&next, "hello"), "Unauthorized", "a new process of it waits out the back-off");
}

/// Anything not named like the app or the CLI is refused without a
/// question; a test's shell stand-in (eDE's own) passes without one.
#[test]
fn others_are_refused_without_a_question_and_edes_own_pass() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let foreign = copy_as(&dir.path().join("a"), "ede172-probe", b"");
    assert_eq!(said(&run(&h, &asks, &foreign, "", &[], None), "hello"), "Unauthorized");
    // The shell's stand-in, by its pid: root at once.
    let shell = copy_as(&dir.path().join("b"), "ede172-shell", b"");
    let dir2 = tempfile::tempdir().unwrap();
    let sock = dir2.path().join("ede.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    let child = Command::new(&shell).args(["--exact", "role_ask", "--ignored", "--nocapture", "--test-threads=1"]).env(ROLE, "").env("EDE172_SOCKET", &sock).env("EDE172_TOKEN", root.as_str()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let pid = child.id();
    h.with(move |ede| ede.shell_stand_in = Some(pid));
    let (stream, _) = listener.accept().unwrap();
    h.with(move |ede| ede.ipc_insert(stream));
    let out = child.wait_with_output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("@@ hello ok"));
    assert!(asks.try_recv().is_err(), "nobody was asked");
    // The test's own root connection (the compositor's process) passes.
    assert!(h.ipc().windows().is_ok());
}

/// What computer use is held to: never the shell's power, mints no wider
/// than the app's, and only its own lineage's spaces and grants.
#[test]
fn computer_use_is_held_to_what_the_app_does() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    // A space and a grant of another lineage (the root's).
    let spaces = dir.path().join("wayland-test.spaces");
    let foreign_space = h.with(move |ede| {
        ede.spaces.open(spaces);
        let desk = ede.create_space(SessionId::new("ctl-space").unwrap(), 640, 480).expect("a space");
        ede.spaces.get(desk).unwrap().id.as_str().to_string()
    });
    let foreign_grant = h.ipc().mint(SessionId::new("someone").unwrap(), vec![Capability::Ui], None).unwrap().token;
    let app = copy_as(&dir.path().join("a"), "exp-desktop", b"");
    let cu = run(&h, &asks, &app, "cu", &[("EDE172_FOREIGN_SPACE", foreign_space.clone()), ("EDE172_FOREIGN_GRANT", foreign_grant.as_str().to_string())], Some((true, Duration::ZERO)));
    for refused in ["lock", "logout", "set_output", "bindings", "subscribe", "launch_token", "mint_system", "revoke_foreign"] {
        assert_eq!(said(&cu, refused), "Forbidden", "{refused}: {:?}", cu.lines);
    }
    assert_eq!(said(&cu, "mint_mixed"), "Invalid", "a space grant is [space] alone, for anyone");
    for allowed in ["mint_run", "mint_space", "revoke_own", "windows", "capture"] {
        assert_eq!(said(&cu, allowed), "ok", "{allowed}: {:?}", cu.lines);
    }
    assert_eq!(said(&cu, "list_spaces"), "0", "another lineage's space is not listed");
    assert_eq!(said(&cu, "destroy_foreign"), "NotFound");
    assert!(h.with(move |ede| ede.spaces.iter().any(|s| s.id.as_str() == foreign_space)), "it is still there");
    assert!(!h.with(|ede| ede.is_locked()) && h.with(move |ede| ede.ipc.grants.iter().any(|g| g.token == foreign_grant)));
}

/// The verdicts that need a root-owned file: eDE's own binary passes,
/// `exponential-ctl` is asked per process, a policy entry grants without a
/// question. `/usr/bin/sleep` stands in for each (root-owned here).
#[test]
fn installed_binaries_ctl_and_the_policy() {
    let sleep = std::fs::canonicalize("/usr/bin/sleep").unwrap();
    if Installed::of(&[("sleep".into(), sleep.clone())]).paths().is_empty() {
        eprintln!("skipped: {} is not root-owned here", sleep.display());
        return;
    }
    let mut child = Command::new(&sleep).arg("30").spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let peer = loop {
        if let Some(p) = Peer::of_pid(child.id()).filter(|p| p.exe_path().as_deref() == Some(sleep.as_path())) {
            break p;
        }
        assert!(Instant::now() < deadline, "sleep never ran");
        std::thread::sleep(Duration::from_millis(20));
    };
    let gate = ede_compositor::consent::Gate::default();
    let none = Policy::default();
    let as_shell = Installed::of(&[("exponential-shell".into(), sleep.clone())]);
    assert_eq!(gate.verdict(Some(&peer), &as_shell, &[], &none, None), Verdict::Grant(Principal::Root));
    let as_ctl = Installed::of(&[("exponential-ctl".into(), sleep.clone())]);
    let Verdict::Ask(ask) = gate.verdict(Some(&peer), &as_ctl, &[], &none, None) else { panic!("ctl is asked") };
    assert_eq!((ask.key.clone(), ask.grants), (Key::Process(peer.pid, peer.start), Principal::Root));
    let mut gate = gate;
    gate.answered(&ask.key, true, &[(peer.pid, peer.start)], &[]);
    assert_eq!(gate.verdict(Some(&peer), &as_ctl, &[], &none, None), Verdict::Grant(Principal::Root), "that process, from now on");
    // A "Don't allow" to ctl backs its file off: another ctl process (the
    // same file) is refused without a question for the back-off.
    let other = Command::new(&sleep).arg("30").spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let second = loop {
        if let Some(p) = Peer::of_pid(other.id()).filter(|p| p.exe_path().as_deref() == Some(sleep.as_path())) {
            break p;
        }
        assert!(Instant::now() < deadline, "sleep never ran");
        std::thread::sleep(Duration::from_millis(20));
    };
    let Verdict::Ask(asked) = gate.verdict(Some(&second), &as_ctl, &[], &none, None) else { panic!("another ctl process is asked") };
    gate.answered(&asked.key, false, &[(second.pid, second.start)], &[asked.file.unwrap()]);
    let third = Command::new(&sleep).arg("30").spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let third_peer = loop {
        if let Some(p) = Peer::of_pid(third.id()).filter(|p| p.exe_path().as_deref() == Some(sleep.as_path())) {
            break p;
        }
        assert!(Instant::now() < deadline, "sleep never ran");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(matches!(gate.verdict(Some(&third_peer), &as_ctl, &[], &none, None), Verdict::Refuse(_)), "ctl in a loop is not asked again at once");
    for mut child in [other, third] {
        let _ = child.kill();
        let _ = child.wait();
    }
    let policy = Policy::parse(&format!("[[allow]]\npath = \"{}\"\nprincipal = \"computer-use\"\n", sleep.display())).unwrap();
    assert_eq!(gate.verdict(Some(&peer), &Installed::default(), &[], &policy, None), Verdict::Grant(Principal::ComputerUse));
    let wrong_hash = Policy::parse(&format!("[[allow]]\npath = \"{}\"\nsha256 = \"{}\"\nprincipal = \"root\"\n", sleep.display(), "0".repeat(64))).unwrap();
    assert!(matches!(gate.verdict(Some(&peer), &Installed::default(), &[], &wrong_hash, Some(&[1; 32])), Verdict::Refuse(_)), "another hash is not the entry's");
    let _ = child.kill();
    let _ = child.wait();
}

/// A policy file that root does not own (here: the test's), group-writable
/// or not, is ignored: no principal comes from it.
#[test]
fn a_policy_file_not_owned_by_root_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.toml");
    std::fs::write(&path, "[[allow]]\npath = \"/usr/bin/sleep\"\nprincipal = \"root\"\n").unwrap();
    for mode in [0o644, 0o664] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(Policy::load(&path).is_err(), "mode {mode:o}");
        let mut gate = ede_compositor::consent::Gate::default();
        gate.policy_path = Some(path.clone());
        assert_eq!(gate.policy(), Policy::default());
    }
    assert_eq!(Policy::load(&dir.path().join("none.toml")).unwrap(), None);
}

/// Off (the default until the flip), the token works from anyone, as
/// today: no question.
#[test]
fn the_gate_is_off_by_default() {
    let h = harness();
    assert!(!h.with(|ede| ede.ipc.consent.on));
    let dir = tempfile::tempdir().unwrap();
    let (tx, asks) = mpsc::channel();
    h.with(move |ede| ede.ipc.consent.stand_in = Some(tx));
    let foreign = copy_as(&dir.path().join("a"), "ede172-probe", b"");
    assert!(said(&run(&h, &asks, &foreign, "", &[], None), "hello").starts_with("ok"));
    let _ = ErrorCode::Unauthorized;
    let _ = Request::ListWindows;
}
