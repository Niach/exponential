//! #EDE-172 (P5', design v3): with the gate on, the root token is honoured
//! by who presents it. The app's and the CLI's executables (`exp-desktop`,
//! `exponential`) are asked about once per login and content hash, and get
//! the `computer-use` principal; anything else is refused without a prompt;
//! eDE's own binaries pass; a policy file that is not root's is ignored.
//!
//! The asking processes are this test binary's ELF interpreter copied under
//! those names, running this binary's `role_ask` (#EDE-207: the file the
//! compositor hashes and the test copies is a few hundred KB, not a debug
//! test binary of hundreds of MB); the person is a stand-in that answers
//! on a channel.

mod common;

use std::{
    io::Write,
    os::unix::{fs::{MetadataExt, PermissionsExt}, net::{UnixListener, UnixStream}},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

use ede_compositor::{
    backend::headless::Harness,
    consent::{HashKey, Key, Own, Policy, Principal, StandIn, Verdict},
    peer::{Installed, Peer},
    Config,
};
use ede_protocol::{Capability, Client, Error, ErrorCode, InputEvent, Request, SessionId, Token};

const ROLE: &str = "EDE172_CONSENT_ROLE";

/// How long the role's hello waits for its reply (the hash, then the
/// person): within [`run`]'s own minute, so a slow runner fails there with
/// what the role said, never with a role that gave up first (#EDE-207).
const HELLO_WITHIN: Duration = Duration::from_secs(50);

/// This test binary's ELF interpreter (`PT_INTERP`), when it has one.
fn loader() -> Option<&'static Path> {
    static LOADER: OnceLock<Option<PathBuf>> = OnceLock::new();
    LOADER.get_or_init(|| interpreter(&std::env::current_exe().ok()?)).as_deref()
}

/// The `PT_INTERP` path of the 64-bit little-endian ELF file `exe`.
fn interpreter(exe: &Path) -> Option<PathBuf> {
    use std::{io::Read, os::unix::ffi::OsStrExt};
    let mut head = Vec::new();
    std::fs::File::open(exe).ok()?.take(64 * 1024).read_to_end(&mut head).ok()?;
    let at = |offset: usize, len: usize| head.get(offset..offset + len);
    let u16_at = |o: usize| Some(u16::from_le_bytes(at(o, 2)?.try_into().ok()?) as usize);
    let u32_at = |o: usize| Some(u32::from_le_bytes(at(o, 4)?.try_into().ok()?));
    let u64_at = |o: usize| Some(u64::from_le_bytes(at(o, 8)?.try_into().ok()?) as usize);
    if at(0, 6)? != b"\x7fELF\x02\x01" {
        return None;
    }
    let (table, entry, count) = (u64_at(0x20)?, u16_at(0x36)?, u16_at(0x38)?);
    (0..count).find_map(|i| {
        let header = table + i * entry;
        if u32_at(header)? != 3 {
            return None;
        }
        let path = at(u64_at(header + 8)?, u64_at(header + 32)?)?;
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(path.strip_suffix(b"\0").unwrap_or(path))))
    })
}

/// A process of `exe` (a [`copy_as`]) in the asking role: the interpreter
/// runs this test binary's `role_ask`.
fn asker(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    if loader().is_some() {
        command.arg(std::env::current_exe().unwrap());
    }
    command.args(["--exact", "role_ask", "--ignored", "--nocapture", "--test-threads=1"]);
    command
}

/// `command` started. A copy written a moment ago can still be open for
/// writing in another test's child, between its fork and its exec, and
/// the kernel then refuses to run it (`ETXTBSY`): tried again a while.
fn spawn(command: &mut Command) -> std::process::Child {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match command.spawn() {
            Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            spawned => return spawned.unwrap(),
        }
    }
}

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

/// The asking executable ([`loader`], else this test binary) under `name`
/// in a directory of its own; `extra` bytes appended make it another file
/// with another hash (it still runs).
fn copy_as(dir: &Path, name: &str, extra: &[u8]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::copy(loader().map_or_else(|| std::env::current_exe().unwrap(), Path::to_path_buf), &path).unwrap();
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
    let mut command = asker(exe);
    command.env(ROLE, ops).env("EDE172_SOCKET", &sock).env("EDE172_TOKEN", root.as_str()).stdout(Stdio::piped()).stderr(Stdio::null());
    for (k, v) in env {
        command.env(k, v);
    }
    let mut child = spawn(&mut command);
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
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let lines: Vec<String> = out.lines().filter_map(|l| l.split_once("@@ ").map(|(_, r)| r.to_string())).collect();
    assert_eq!(asked.is_some(), answer.is_some(), "asked: {:?}; it said {lines:?}", asked.as_ref().map(|a| &a.asking.body));
    assert!(asks.try_recv().is_err(), "the person was asked once more");
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
    let first = Client::over_waiting(UnixStream::connect(&sock).unwrap(), token, HELLO_WITHIN);
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
        r("open_services", client.open_services(vec![Capability::Ui]).map(|_| ()));
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
        let again = Client::over_waiting(UnixStream::connect(&sock).unwrap(), token, HELLO_WITHIN);
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
    // How long the person reads: what tells waiting for them from at once
    // on a slow runner too (#EDE-207).
    let reads = Duration::from_millis(1500);
    let first = run(&h, &asks, &app, "", &[], Some((true, reads)));
    let asked = first.asked.as_ref().unwrap();
    assert!(asked.asking.body.contains(&app.display().to_string()) && asked.asking.body.contains(&first.pid.to_string()), "the prompt names the binary and the process: {}", asked.asking.body);
    assert_eq!(asked.asking.grants, Principal::ComputerUse);
    assert!(matches!(asked.asking.key, Key::Binary(_)));
    let waited: u128 = said(&first, "hello").strip_prefix("ok ").unwrap().parse().unwrap();
    assert!(waited >= reads.as_millis(), "the hello waited for the person ({waited} ms)");
    // Again, a new process (hashed again, #EDE-206): no question, at once.
    let second = run(&h, &asks, &app, "", &[], None);
    let at_once: u128 = said(&second, "hello").strip_prefix("ok ").unwrap().parse().unwrap();
    assert!(at_once < reads.as_millis(), "{at_once} ms");
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
/// changed binary is asked about, never let in on the old hash. Also within
/// the same second where timestamps are coarse (#EDE-206): the rewritten
/// file then reads with the very stat the old hash was taken at. Here the
/// file-wide cache holds the old hash under the rewritten file's stat, and
/// the person's file is hashed again all the same, for its new process.
#[test]
fn a_rewrite_in_place_is_hashed_again() {
    use std::io::{Seek, SeekFrom};
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("a"), "exponential", b"aaaa");
    let first = run(&h, &asks, &app, "", &[], Some((true, Duration::ZERO)));
    assert!(first.lines.iter().any(|l| l.starts_with("hello ok")));
    let Some(Key::Binary(allowed)) = first.asked.map(|a| a.asking.key) else { panic!("the app is asked about by its content") };
    let mtime = std::fs::metadata(&app).unwrap().modified().unwrap();
    let mut file = std::fs::OpenOptions::new().write(true).open(&app).unwrap();
    file.seek(SeekFrom::End(-4)).unwrap();
    file.write_all(b"bbbb").unwrap();
    file.set_modified(mtime).unwrap();
    drop(file);
    let meta = std::fs::metadata(&app).unwrap();
    assert_eq!(meta.modified().unwrap(), mtime, "the mtime is put back");
    let same_second = HashKey::File(meta.dev(), meta.ino(), meta.size(), meta.mtime(), meta.ctime(), meta.ctime_nsec());
    let hashed = h.with(move |ede| {
        ede.ipc.consent.remember_hash(same_second, allowed);
        ede.ipc.consent.hashes_started
    });
    let again = run(&h, &asks, &app, "", &[], Some((false, Duration::ZERO)));
    assert_eq!(said(&again, "hello"), "Unauthorized", "the rewritten file was asked about (and refused), not let in on the old hash");
    assert_eq!(h.with(|ede| ede.ipc.consent.hashes_started), hashed + 1, "hashed again");
}

/// #EDE-206: what a hash is cached by. A file the person can write: by the
/// process too, each new process of it hashed once, the file unchanged or
/// not; root's own file: by the file, for every process that runs it.
#[test]
fn a_persons_file_is_hashed_per_process_and_roots_per_file() {
    use ede_compositor::consent::hash_key;
    let sleep = std::fs::canonicalize("/usr/bin/sleep").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let theirs = dir.path().join("exponential");
    std::fs::copy(&sleep, &theirs).unwrap();
    let theirs = std::fs::canonicalize(theirs).unwrap();
    let running = |exe: &Path| {
        let child = spawn(Command::new(exe).arg("30"));
        let peer = peer_running(child.id(), exe);
        (child, peer)
    };
    let (a, peer_a) = running(&theirs);
    let (b, peer_b) = running(&theirs);
    let key_a = hash_key(&peer_a).unwrap();
    assert!(matches!(key_a, HashKey::Process { pid, start, .. } if pid == peer_a.pid && start == peer_a.start), "{key_a:?}");
    assert_eq!(hash_key(&peer_a), Some(key_a), "one process: one key");
    assert_ne!(hash_key(&peer_b), Some(key_a), "another process of the same file: hashed again");
    let mut children = vec![a, b];
    if ede_compositor::peer::root_owned(&sleep).is_some() {
        let (c, peer_c) = running(&sleep);
        let (d, peer_d) = running(&sleep);
        let key_c = hash_key(&peer_c).unwrap();
        assert!(matches!(key_c, HashKey::File(..)), "{key_c:?}");
        assert_eq!(hash_key(&peer_d), Some(key_c), "root's file: one key for every process");
        children.extend([c, d]);
    } else {
        eprintln!("skipped root's half: {} is not root-owned here", sleep.display());
    }
    for child in &mut children {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Several hellos of one new binary at once: each process of it is hashed
/// once (#EDE-206: the file is the person's, so its hash is kept per
/// process; a second hello of a process uses it), and they all wait for the
/// one question.
#[test]
fn one_hash_per_process_however_many_hellos() {
    use std::io::Read;
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    let app = copy_as(&dir.path().join("a"), "exp-desktop", b"many");
    let sock = dir.path().join("ede.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    let mut children: Vec<_> = (0..3).map(|_| spawn(asker(&app).env(ROLE, "again").env("EDE172_SOCKET", &sock).env("EDE172_TOKEN", root.as_str()).stdout(Stdio::piped()).stderr(Stdio::null()))).collect();
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
        assert!(out.contains("@@ hello ok") && out.contains("@@ again ok"), "{out}");
    }
    assert_eq!(h.with(|ede| ede.ipc.consent.hashes_started), 3, "one hash per process, six hellos");
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
    let child = spawn(asker(&shell).env(ROLE, "").env("EDE172_SOCKET", &sock).env("EDE172_TOKEN", root.as_str()).stdout(Stdio::piped()).stderr(Stdio::null()));
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
    for refused in ["lock", "logout", "set_output", "bindings", "subscribe", "launch_token", "open_services", "mint_system", "revoke_foreign"] {
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
    // The units start eDE's programs: they pass by file.
    let units = Own { by_file: true, ..Own::default() };
    let as_shell = Installed::of(&[("exponential-shell".into(), sleep.clone())]);
    assert_eq!(gate.verdict(Some(&peer), &as_shell, &units, &none, None), Verdict::Grant(Principal::Root));
    let as_ctl = Installed::of(&[("exponential-ctl".into(), sleep.clone())]);
    let Verdict::Ask(ask) = gate.verdict(Some(&peer), &as_ctl, &units, &none, None) else { panic!("ctl is asked") };
    assert_eq!((ask.key.clone(), ask.grants), (Key::Process(peer.pid, peer.start), Principal::Root));
    let mut gate = gate;
    gate.answered(&ask.key, true, &[(peer.pid, peer.start)], &[]);
    assert_eq!(gate.verdict(Some(&peer), &as_ctl, &units, &none, None), Verdict::Grant(Principal::Root), "that process, from now on");
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
    let Verdict::Ask(asked) = gate.verdict(Some(&second), &as_ctl, &units, &none, None) else { panic!("another ctl process is asked") };
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
    assert!(matches!(gate.verdict(Some(&third_peer), &as_ctl, &units, &none, None), Verdict::Refuse(_)), "ctl in a loop is not asked again at once");
    for mut child in [other, third] {
        let _ = child.kill();
        let _ = child.wait();
    }
    let policy = Policy::parse(&format!("[[allow]]\npath = \"{}\"\nprincipal = \"computer-use\"\n", sleep.display())).unwrap();
    assert_eq!(gate.verdict(Some(&peer), &Installed::default(), &units, &policy, None), Verdict::Grant(Principal::ComputerUse));
    let wrong_hash = Policy::parse(&format!("[[allow]]\npath = \"{}\"\nsha256 = \"{}\"\nprincipal = \"root\"\n", sleep.display(), "0".repeat(64))).unwrap();
    assert!(matches!(gate.verdict(Some(&peer), &Installed::default(), &units, &wrong_hash, Some(&[1; 32])), Verdict::Refuse(_)), "another hash is not the entry's");
    let _ = child.kill();
    let _ = child.wait();
}

/// `exe` (root-owned) as process `pid` once it runs it.
fn peer_running(pid: u32, exe: &Path) -> Peer {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(p) = Peer::of_pid(pid).filter(|p| p.exe_path().as_deref() == Some(exe)) {
            return p;
        }
        assert!(Instant::now() < deadline, "{} never ran", exe.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// P14 (eDE's flip): with the compositor starting the session's helpers,
/// eDE's genuine binary gets root with no prompt only as a child the
/// compositor started (here the test is the compositor); the same file
/// started by anything else is refused, as is a listed pid whose parent
/// is not the compositor (a pid used again).
#[test]
fn edes_own_binaries_pass_only_as_the_compositors_children() {
    let sleep = std::fs::canonicalize("/usr/bin/sleep").unwrap();
    let as_shell = Installed::of(&[("exponential-shell".into(), sleep.clone())]);
    if as_shell.paths().is_empty() {
        eprintln!("skipped: {} is not root-owned here", sleep.display());
        return;
    }
    let gate = ede_compositor::consent::Gate::default();
    let none = Policy::default();
    let mut ours = Command::new(&sleep).arg("30").spawn().unwrap();
    let peer = peer_running(ours.id(), &sleep);
    let by_file = Own { by_file: true, ..Own::default() };
    assert_eq!(gate.verdict(Some(&peer), &as_shell, &by_file, &none, None), Verdict::Grant(Principal::Root), "the units start it: by file");
    let helpers = Own { pids: vec![std::process::id()], children: vec![], by_file: false };
    let Verdict::Refuse(why) = gate.verdict(Some(&peer), &as_shell, &helpers, &none, None) else { panic!("eDE's file, not started by the compositor, passed") };
    assert!(why.contains("the compositor did not start it"), "{why}");
    let child = Own { children: vec![ours.id()], ..helpers.clone() };
    assert_eq!(gate.verdict(Some(&peer), &as_shell, &child, &none, None), Verdict::Grant(Principal::Root), "the compositor's child");
    assert!(!gate.needs_hash(&peer, &as_shell, &none), "eDE's own file is never hashed");
    // A listed pid run by someone else: its parent is not the compositor.
    let out = Command::new("/bin/sh").arg("-c").arg(format!("{} 30 >/dev/null 2>&1 & echo $!", sleep.display())).output().unwrap();
    let theirs: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    let their_peer = peer_running(theirs, &sleep);
    let listed = Own { children: vec![theirs], ..helpers.clone() };
    assert!(matches!(gate.verdict(Some(&their_peer), &as_shell, &listed, &none, None), Verdict::Refuse(_)), "a pid in the list whose parent is not the compositor");
    // Safety: signalling the sleep this test started through sh.
    unsafe { libc::kill(theirs as libc::pid_t, libc::SIGKILL) };
    let _ = ours.kill();
    let _ = ours.wait();
}

/// What the root-owned interpreter runs: hello with the root token on
/// `EDE172_SOCKET`, the answer's frame to `EDE172_OUT`, then it stays.
const PY_HELLO: &str = "import os, socket, struct, time\ns = socket.socket(socket.AF_UNIX)\ns.connect(os.environ['EDE172_SOCKET'])\nhello = bytes.fromhex(os.environ['EDE172_HELLO'])\ns.sendall(struct.pack('<I', len(hello)) + hello)\ndef exact(n):\n    b = b''\n    while len(b) < n:\n        c = s.recv(n - len(b))\n        if not c:\n            raise SystemExit('closed')\n        b += c\n    return b\nn = struct.unpack('<I', exact(4))[0]\nbody = exact(n)\nopen(os.environ['EDE172_OUT'] + '.part', 'wb').write(body)\nos.rename(os.environ['EDE172_OUT'] + '.part', os.environ['EDE172_OUT'])\ntime.sleep(60)\n";

/// A root-owned interpreter taken for eDE's shell (`exe`) says hello with
/// the root token: started by `start` (a command of the test's), else by
/// the compositor as its helper. What the compositor answered, as JSON.
fn interpreted_hello(h: &Harness, exe: &Path, start: impl FnOnce(Vec<(String, String)>) -> Option<Command>) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("ede.sock");
    let out = dir.path().join("said");
    let listener = UnixListener::bind(&sock).unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = h.with(|ede| ede.ipc.root_grant().token);
    let hello = serde_json::to_vec(&ede_protocol::Call { id: 1, request: Request::Hello { version: ede_protocol::PROTOCOL_VERSION, token: Some(root) } }).unwrap();
    let hex: String = hello.iter().map(|b| format!("{b:02x}")).collect();
    let env = vec![("EDE172_SOCKET".to_string(), sock.display().to_string()), ("EDE172_HELLO".to_string(), hex), ("EDE172_OUT".to_string(), out.display().to_string())];
    let mut by_hand = start(env.clone()).map(|mut c| c.spawn().unwrap());
    if by_hand.is_none() {
        let spec = ede_compositor::helpers::Spec { name: "ede172-interpreter".into(), argv: vec![exe.display().to_string(), "-I".into(), "-c".into(), PY_HELLO.into()], channel: None, wayland: None, env };
        h.with(move |ede| ede.start_helpers_with(vec![spec]));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while !out.exists() {
        assert!(Instant::now() < deadline, "the interpreter never said what it was answered");
        if let Ok((stream, _)) = listener.accept() {
            h.with(move |ede| ede.ipc_insert(stream));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(child) = by_hand.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// The same end to end, on the harness's compositor: a root-owned
/// interpreter taken for eDE's installed shell passes by file while the
/// units start the helpers; once the compositor starts them it is refused
/// when started by hand, and passes as the compositor's own helper.
#[test]
fn a_genuine_shell_started_by_hand_is_refused_once_the_compositor_starts_the_helpers() {
    let Ok(python) = std::fs::canonicalize("/usr/bin/python3") else {
        eprintln!("skipped: no /usr/bin/python3");
        return;
    };
    let as_shell = Installed::of(&[("exponential-shell".into(), python.clone())]);
    if as_shell.paths().is_empty() {
        eprintln!("skipped: {} is not root-owned here", python.display());
        return;
    }
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let asks = gated(&h, dir.path().join("no-policy.toml"));
    h.with(move |ede| ede.installed = as_shell);
    let by_hand = |env: Vec<(String, String)>| {
        let mut command = Command::new(&python);
        command.args(["-I", "-c", PY_HELLO]).envs(env).stdout(Stdio::null()).stderr(Stdio::null());
        Some(command)
    };
    let kind = |said: &serde_json::Value| said["type"].as_str().unwrap_or_default().to_string();
    let said = interpreted_hello(&h, &python, by_hand);
    assert_eq!(kind(&said), "reply", "the units start the helpers: by file ({said})");
    h.with(|ede| ede.start_helpers_with(vec![]));
    let said = interpreted_hello(&h, &python, by_hand);
    assert_eq!((kind(&said), said["error"]["code"].as_str()), ("error".to_string(), Some("unauthorized")), "started by hand: {said}");
    assert!(said["error"]["message"].as_str().unwrap_or_default().contains("the compositor did not start it"), "{said}");
    let said = interpreted_hello(&h, &python, |_| None);
    assert_eq!(kind(&said), "reply", "the compositor's own helper: {said}");
    assert!(asks.try_recv().is_err(), "nobody was asked");
}

/// A policy file that root does not own (here: the test's), group-writable
/// or not, is ignored: no principal comes from it, and (P14) its `[control]
/// consent = false` and `[session] helpers = "units"` do not weaken a real
/// session; the same words from a file root's policy parses are followed.
#[test]
fn a_policy_file_not_owned_by_root_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.toml");
    let text = "[control]\nconsent = false\n[session]\nhelpers = \"units\"\n[[allow]]\npath = \"/usr/bin/sleep\"\nprincipal = \"root\"\n";
    std::fs::write(&path, text).unwrap();
    for mode in [0o644, 0o664] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(Policy::load(&path).is_err(), "mode {mode:o}");
        let mut gate = ede_compositor::consent::Gate::default();
        gate.policy_path = Some(path.clone());
        assert_eq!(gate.policy(), Policy::default());
        let (real, _) = Config::default().for_a_real_session(&gate.policy());
        assert_eq!((real.session.helpers(), real.control.consent()), (ede_compositor::config::Helpers::Compositor, true), "mode {mode:o}: the gate stays on");
    }
    let (followed, _) = Config::default().for_a_real_session(&Policy::parse(text).unwrap());
    assert_eq!((followed.session.helpers(), followed.control.consent()), (ede_compositor::config::Helpers::Units, false), "root's file is followed");
    assert_eq!(Policy::load(&dir.path().join("none.toml")).unwrap(), None);
}

/// A real session whose person's `compositor.toml` (any process of theirs
/// can write it) says `consent = false` and `helpers = "units"` keeps the
/// gate on and the helpers the compositor's (P14).
#[test]
fn the_persons_file_never_turns_the_gate_off() {
    let persons = Config::parse("[control]\nconsent = false\n[session]\nhelpers = \"units\"\n").unwrap();
    let (real, said) = persons.for_a_real_session(&Policy::default());
    assert_eq!((real.session.helpers(), real.control.consent()), (ede_compositor::config::Helpers::Compositor, true));
    assert_eq!(said.len(), 2, "{said:?}");
}

/// Off in a test's or a nested compositor (a real session's gate is on
/// since eDE's flip, P14, unless root's policy file says `consent =
/// false`), the token works from anyone: no question.
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
