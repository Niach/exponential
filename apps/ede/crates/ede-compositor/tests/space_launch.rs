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
        for (name, value) in [("DISPLAY", ":77"), ("WAYLAND_SOCKET", "9"), ("XDG_ACTIVATION_TOKEN", "t"), ("DESKTOP_STARTUP_ID", "t"), ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/persons-bus"), ("EDE_TOKEN", "persons-token")] {
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
    let home = runtime.join("home");

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
    for gone in ["DISPLAY", "WAYLAND_SOCKET", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "EDE_TOKEN"] {
        assert!(!got.contains_key(gone), "{gone} reached the launch");
    }
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
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XDG_RUNTIME_DIR", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "DBUS_SESSION_BUS_ADDRESS", "EDE_TOKEN", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
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
