//! #EDE-126 headless: launch into a space. The compositor starts the
//! program and decides its environment: the space's socket and runtime
//! directory, a profile of the space's own, never the person's displays,
//! session bus or token. What was launched ends with the space, and the
//! space's home goes with it.

mod common;

use std::{
    collections::BTreeMap,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use common::{space::start_with_spaces, Session};
use ede_protocol::{Capability, Client, Error, ErrorCode, SessionId, SpaceInfo};

const BLUE: u32 = 0xff00_00ff;
const DUMP: &str = "EDE_TEST_SPACE_DUMP";
const PROFILE: [&str; 4] = ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"];
/// #EDE-138: what a login (or a terminal the compositor was started from)
/// may have that is nobody's business in a space: the launch environment
/// is an allowlist, so none of it reaches a launch.
const LOGIN_ONLY: [(&str, &str); 10] = [
    ("TMUX", "/tmp/tmux-1000/default,1,0"),
    ("TMUX_PANE", "%0"),
    ("STY", "1234.pts-0.host"),
    ("NOTIFY_SOCKET", "/run/user/1000/systemd/notify"),
    ("DOCKER_HOST", "unix:///run/user/1000/docker.sock"),
    ("CONTAINER_HOST", "unix:///run/user/1000/podman/podman.sock"),
    ("SSH_AUTH_SOCK", "/run/user/1000/keyring/ssh"),
    ("GPG_AGENT_INFO", "/run/user/1000/gnupg/S.gpg-agent:0:1"),
    ("GNOME_KEYRING_CONTROL", "/run/user/1000/keyring"),
    ("SESSION_MANAGER", "local/host:@/tmp/.ICE-unix/1"),
];

/// The launched app: this test binary again. It writes its environment,
/// pid and directory to the file it was told, opens a window in the space
/// it finds in `WAYLAND_DISPLAY`, and stays until the compositor is gone.
#[test]
fn launched_client() {
    let Ok(dump) = std::env::var(DUMP) else { return };
    let mut lines: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    lines.push(format!("PID={}", std::process::id()));
    lines.push(format!("CWD={}", std::env::current_dir().unwrap().display()));
    let socket = std::env::var("WAYLAND_DISPLAY").unwrap();
    let mut c = Session::connect(UnixStream::connect(socket).unwrap());
    let t = c.toplevel("launched");
    c.draw(&t, 300, 200, BLUE);
    std::fs::write(format!("{dump}.tmp"), lines.join("\n")).unwrap();
    std::fs::rename(format!("{dump}.tmp"), &dump).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && c.queue.roundtrip(&mut c.state).is_ok() {
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn code<T: std::fmt::Debug>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What the person's session would hand down: none of it may reach a launch.
fn persons_environment() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        for (name, value) in [("DISPLAY", ":77"), ("WAYLAND_SOCKET", "9"), ("XDG_ACTIVATION_TOKEN", "t"), ("DESKTOP_STARTUP_ID", "t"), ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/persons-bus"), ("EDE_TOKEN", "persons-token")]
            .into_iter()
            .chain(LOGIN_ONLY)
        {
            std::env::set_var(name, value);
        }
        for name in PROFILE {
            std::env::set_var(name, format!("/nonexistent/persons/{name}"));
        }
    });
}

fn this_test(dump: &Path) -> (Vec<String>, BTreeMap<String, String>) {
    let argv = vec![std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(), "launched_client".into(), "--nocapture".into()];
    (argv, [(DUMP.to_string(), dump.to_string_lossy().into_owned())].into())
}

fn dumped(dump: &Path) -> BTreeMap<String, String> {
    until("the launched client never wrote its environment", || dump.exists());
    std::fs::read_to_string(dump).unwrap().lines().filter_map(|line| line.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn alive(pid: &str) -> bool {
    // A zombie is not alive: it waits for the compositor's tick to reap it.
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| !stat.rsplit(')').next().unwrap_or_default().trim_start().starts_with('Z'))
}

fn agent(h: &ede_compositor::backend::headless::Harness, session: &str) -> (Client, SpaceInfo) {
    let mut root = h.ipc();
    assert!(root.hello.offers_spaces(), "a compositor that can make a space says so in its hello");
    let grant = root.mint(SessionId::new(session).unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    (agent, space)
}

#[test]
fn a_launch_opens_in_its_space_with_the_spaces_own_environment_and_ends_with_it() {
    persons_environment();
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, space) = agent(&h, "run-1");
    let runtime = PathBuf::from(&space.wayland_display).parent().unwrap().to_path_buf();
    // #EDE-138: the home is on disk beside the other spaces' of this
    // compositor, never in the runtime directory (a tmpfs the person shares).
    let home = dir.path().join("homes").join(format!("p{}", std::process::id())).join(space.id.as_str());
    assert!(!home.starts_with(&runtime));

    let dump = dir.path().join("first.env");
    let (argv, env) = this_test(&dump);
    let pid = agent.launch(argv.clone(), env, None).unwrap();
    let got = dumped(&dump);
    // It opened in the space, not on the person's desk.
    until("the launched window never opened in the space", || agent.windows().unwrap().iter().any(|w| w.title.as_deref() == Some("launched")));
    assert_eq!(h.ipc().windows().unwrap().len(), 0, "the launch opened on the person's desk");
    // The space's socket and runtime directory.
    assert_eq!(got["WAYLAND_DISPLAY"], space.wayland_display);
    assert_eq!(Path::new(&got["XDG_RUNTIME_DIR"]), runtime);
    // Nothing of the person's session.
    for gone in ["DISPLAY", "WAYLAND_SOCKET", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "EDE_TOKEN"].into_iter().chain(LOGIN_ONLY.map(|(name, _)| name)) {
        assert!(!got.contains_key(gone), "{gone} reached the launch");
    }
    // What a launch keeps of the compositor's, and a tmux server, screen
    // sessions and a gpg home of the space's own.
    for kept in ["PATH", "HOME"] {
        assert_eq!(got.get(kept), std::env::var(kept).ok().as_ref(), "{kept}");
    }
    assert_eq!(Path::new(&got["TMUX_TMPDIR"]), runtime);
    assert_eq!(Path::new(&got["SCREENDIR"]), runtime.join("screen"));
    assert!(Path::new(&got["GNUPGHOME"]).starts_with(&home));
    assert_eq!(std::fs::metadata(&got["SCREENDIR"]).map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o777).unwrap(), 0o700, "screen wants its directory 0700");
    assert_ne!(got.get("DBUS_SESSION_BUS_ADDRESS").map(String::as_str), Some("unix:path=/nonexistent/persons-bus"), "the person's session bus reached the launch");
    // A profile of the space's own, under its home, made before the start.
    for name in PROFILE {
        let own = Path::new(&got[name]);
        assert!(own.starts_with(&home) && own.is_dir(), "{name} = {}", own.display());
        assert!(!got.values().any(|v| v == &format!("/nonexistent/persons/{name}")), "the person's {name} reached the launch");
    }
    assert_eq!(std::fs::metadata(&home).map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o777).unwrap(), 0o700);
    assert_eq!(Some(got["CWD"].as_str()), std::env::var("HOME").ok().as_deref(), "the person's home by default");
    assert!(alive(&pid.to_string()) && alive(&got["PID"]));

    // `cwd` is the caller's to choose.
    let second = dir.path().join("second.env");
    let (argv, env) = this_test(&second);
    agent.launch(argv, env, Some(dir.path().to_string_lossy().into_owned())).unwrap();
    assert_eq!(Path::new(&dumped(&second)["CWD"]), dir.path().canonicalize().unwrap());

    // The space closes: what was launched ends, its home and its directory go.
    agent.destroy_space(space.id.clone()).unwrap();
    until("a launched process outlived its space", || !alive(&pid.to_string()) && !alive(&got["PID"]) && !alive(&dumped(&second)["PID"]));
    until("the space's home outlived it", || !home.exists() && !runtime.exists());
    common::space::spaces_settled(&h);
}

#[test]
fn a_launch_is_refused_what_the_compositor_decides() {
    persons_environment();
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, _space) = agent(&h, "run-1");
    let sleep = || vec!["sleep".to_string(), "30".to_string()];
    // The names the compositor sets or drops are not the caller's.
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XDG_RUNTIME_DIR", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "DBUS_SESSION_BUS_ADDRESS", "EDE_TOKEN", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "TMUX_TMPDIR", "SCREENDIR", "GNUPGHOME"] {
        assert_eq!(code(agent.launch(sleep(), [(name.to_string(), "x".to_string())].into(), None)), ErrorCode::Invalid, "{name}");
    }
    assert_eq!(code(agent.launch(vec![], Default::default(), None)), ErrorCode::Invalid);
    assert_eq!(code(agent.launch(vec!["no-such-program-ede-126".into()], Default::default(), None)), ErrorCode::NotFound);
    assert_eq!(code(agent.launch(sleep(), Default::default(), Some("/nonexistent/dir".into()))), ErrorCode::NotFound);
    assert_eq!(h.with(|state| state.spaces.launched()), 0, "a refused launch started something");
    // 32 at a time, per compositor; the rest is `limit`.
    for _ in 0..32 {
        agent.launch(sleep(), Default::default(), None).unwrap();
    }
    assert_eq!(code(agent.launch(sleep(), Default::default(), None)), ErrorCode::Limit);
    // Another space's grant launches nothing here either, and a run
    // without a space has none to launch into.
    let unbound = h.ipc().mint(SessionId::new("run-2").unwrap(), vec![Capability::Space], None).unwrap();
    assert_eq!(code(h.ipc_with(unbound.token).unwrap().launch(sleep(), Default::default(), None)), ErrorCode::NotFound);
    drop(h);
}

const CLIENT_SOCKET: &str = "EDE_TEST_SPACE_SOCKET";

/// A client of a space that the compositor did not launch and whose
/// environment does not name the space: this test binary again, connected
/// to the socket it was given under another name. It stays until the
/// compositor is gone.
#[test]
fn foreign_space_client() {
    let Ok(socket) = std::env::var(CLIENT_SOCKET) else { return };
    let ready = std::env::var(DUMP).unwrap();
    let mut c = Session::connect(UnixStream::connect(socket).unwrap());
    let t = c.toplevel("foreign");
    c.draw(&t, 200, 100, BLUE);
    std::fs::write(&ready, std::process::id().to_string()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && c.queue.roundtrip(&mut c.state).is_ok() {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `pid`'s parent is not in the process group `group` (it was reparented).
fn parent_outside(pid: u32, group: u32) -> bool {
    let field = |pid: u32, n: usize| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|stat| stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(n).and_then(|f| f.parse::<u32>().ok())));
    field(pid, 1).is_some_and(|parent| field(parent, 2) != Some(group))
}

fn pid_in(file: &Path) -> u32 {
    until(&format!("{} was never written", file.display()), || file.exists());
    std::fs::read_to_string(file).unwrap().trim().parse().unwrap()
}

/// #EDE-127: the compositor answers which space a process belongs to (the
/// polkit agent asks before it shows the person a prompt): a launch, what
/// it started (also after leaving its group, or after its parent died),
/// what connected to the space's socket; the person's processes belong to
/// none. Only `system` may ask.
#[test]
fn the_compositor_knows_which_processes_are_a_spaces() {
    use ede_protocol::{Membership, ProcessSpace};
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, space) = agent(&h, "run-1");
    let mut root = h.ipc();
    let of = |root: &mut Client, pid: u32| root.space_of_process(pid).unwrap();

    // The person's: this process (the compositor's own) and a child of it.
    let person = std::process::Command::new("sleep").arg("60").spawn().unwrap();
    assert_eq!(of(&mut root, std::process::id()), None);
    assert_eq!(of(&mut root, person.id()), None);
    assert_eq!(of(&mut root, 1), None);
    assert_eq!(of(&mut root, u32::MAX), None, "no such process");

    // A shell in the space that starts three processes: a child, one that
    // cleared its environment and lost its parent (still in the launch's
    // process group), and one in a session of its own that lost its parent
    // (still with the space's runtime directory).
    let d = dir.path().join("pids");
    std::fs::create_dir(&d).unwrap();
    let script = r#"
        ( env -i sleep 60 & echo $! > "$D/group.tmp"; mv "$D/group.tmp" "$D/group" )
        ( setsid sleep 60 & echo $! > "$D/env.tmp"; mv "$D/env.tmp" "$D/env" )
        sleep 60 & echo $! > "$D/child.tmp"; mv "$D/child.tmp" "$D/child"
        wait"#;
    let head = agent.launch(vec!["sh".into(), "-c".into(), script.into()], [("D".to_string(), d.to_string_lossy().into_owned())].into(), None).unwrap();
    let (child, group, env) = (pid_in(&d.join("child")), pid_in(&d.join("group")), pid_in(&d.join("env")));
    let ours = |pid: u32| of(&mut h.ipc(), pid).unwrap_or_else(|| panic!("process {pid} is not known as the space's"));
    assert_eq!(ours(head), ProcessSpace { space: space.id.clone(), by: Membership::Launch, pid: head });
    assert_eq!(ours(child).space, space.id);
    // `$!` comes before the fork has run `env` and `setsid`, and they before `sleep`.
    until("the subshells' children never ran sleep", || [child, group, env].into_iter().all(|pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|comm| comm.trim() == "sleep")));
    until("the subshells' children never lost their parent", || [group, env].into_iter().all(|pid| parent_outside(pid, head)));
    let by_group = ours(group);
    assert_eq!((by_group.space, by_group.by), (space.id.clone(), Membership::Group), "an orphan with no environment, in the launch's group");
    let by_env = ours(env);
    assert_eq!((by_env.space, by_env.by, by_env.pid), (space.id.clone(), Membership::Environment, env), "an orphan in its own session, with the space's runtime directory");

    // A client of the space's socket that was not launched there and whose
    // environment does not name it.
    let ready = dir.path().join("foreign.ready");
    let mut foreign = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "foreign_space_client", "--nocapture"])
        .env(CLIENT_SOCKET, &space.wayland_display)
        .env(DUMP, &ready)
        .env_remove("WAYLAND_DISPLAY")
        .spawn()
        .unwrap();
    let pid = pid_in(&ready);
    assert_eq!(ours(pid), ProcessSpace { space: space.id.clone(), by: Membership::Client, pid });
    assert_eq!(of(&mut root, person.id()), None, "the person's process is still the person's");

    // Only `system` asks: a space grant and a `ui` grant are refused.
    assert_eq!(code(agent.space_of_process(head)), ErrorCode::Forbidden);
    let ui = root.mint(SessionId::new("ui-only").unwrap(), vec![Capability::Ui], None).unwrap();
    assert_eq!(code(h.ipc_with(ui.token).unwrap().space_of_process(head)), ErrorCode::Forbidden);

    let _ = foreign.kill();
    let _ = foreign.wait();
    let mut person = person;
    let _ = person.kill();
    let _ = person.wait();
    for pid in [group, env] {
        let _ = smithay::reexports::rustix::process::kill_process(smithay::reexports::rustix::process::Pid::from_raw(pid as i32).unwrap(), smithay::reexports::rustix::process::Signal::KILL);
    }
}
