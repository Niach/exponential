//! #EDE-76 headless: a launch the person made takes the keyboard. The
//! shell's `launch_token` (or a hotkey of the compositor) puts a one-shot
//! token into the launched process's environment; an app that hands it to
//! a running instance activates over xdg-activation.

mod common;

use std::{
    os::unix::net::{UnixListener, UnixStream},
    sync::Arc,
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, state::ClientState, Config};
use ede_protocol::{Capability, Error, ErrorCode, SessionId};
use wayland_client::{protocol::wl_registry, Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const SOCKET: &str = "EDE_TEST_LAUNCH_SOCKET";

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(1280, 800)])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

/// The windows' titles, back to front, with which one is focused.
fn stack(h: &Harness) -> Vec<(String, bool)> {
    let mut ipc = h.ipc();
    ipc.windows().unwrap().into_iter().map(|w| (w.title.unwrap_or_default(), w.focused)).collect()
}

fn until(what: &str, ok: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn focused_app(h: &Harness) -> (Session, common::Toplevel) {
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("editor");
    app.draw(&t, 400, 300, RED);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
    (app, t)
}

/// The app being typed into as a process of its own (what this test
/// starts would count as started by an in-process one).
fn focused_process(h: &Harness, socket: &str) -> std::process::Child {
    let editor = shell_launch(socket, "editor", None);
    until("the first window never took the keyboard", || stack(h) == [("editor".to_string(), true)]);
    editor
}

/// A socket real processes connect to (the compositor reads who they are
/// from it); the directory lives as long as the returned guard.
fn listen(h: &Harness) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wayland");
    let listener = UnixListener::bind(&path).unwrap();
    let mut handle = h.with(|ede| ede.display_handle.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = handle.insert_client(stream, Arc::new(ClientState::default()));
        }
    });
    (dir, path.to_string_lossy().into_owned())
}

/// The launched app: this test binary again, as a client of its own.
#[test]
fn launched_client() {
    let Ok(socket) = std::env::var(SOCKET) else { return };
    let mut c = Session::connect(UnixStream::connect(socket).unwrap());
    let t = c.toplevel(&std::env::var("EDE_TEST_LAUNCH_TITLE").unwrap());
    c.draw(&t, 300, 200, BLUE);
    let deadline = Instant::now() + Duration::from_secs(20);
    // Until it is closed, or the compositor is gone.
    while !c.state.toplevels[0].closed && Instant::now() < deadline && c.queue.roundtrip(&mut c.state).is_ok() {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `sh -c` line that starts [`launched_client`] titled `title`.
fn launch_line(socket: &str, title: &str) -> String {
    format!("{SOCKET}={socket} EDE_TEST_LAUNCH_TITLE={title} exec {} --exact launched_client --nocapture", std::env::current_exe().unwrap().display())
}

/// What the shell does: `sh -c` with the token in the environment.
fn shell_launch(socket: &str, title: &str, token: Option<&str>) -> std::process::Child {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(launch_line(socket, title)).env_remove("XDG_ACTIVATION_TOKEN").env_remove("DESKTOP_STARTUP_ID");
    if let Some(token) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", token).env("DESKTOP_STARTUP_ID", token);
    }
    cmd.spawn().unwrap()
}

fn close_all(h: &Harness, children: Vec<std::process::Child>) {
    h.with(|ede| {
        for w in ede.desk.space.elements().cloned().collect::<Vec<_>>() {
            ede.close_window(&w);
        }
    });
    for mut child in children {
        let _ = child.wait();
    }
}

#[test]
fn a_window_launched_through_the_shell_takes_the_keyboard_once() {
    let h = harness();
    let (_dir, socket) = listen(&h);
    let editor = focused_process(&h, &socket);
    let mut shell = h.ipc();
    let token = shell.launch_token().unwrap();

    let files = shell_launch(&socket, "files", Some(&token));
    until("the launched window never took the keyboard", || stack(&h).last() == Some(&("files".to_string(), true)));

    // The token is used up: a second process with it is like any other app,
    // and so is one launched with no token (an agent's, an app's).
    let again = shell_launch(&socket, "again", Some(&token));
    let agent = shell_launch(&socket, "agent", None);
    until("the other windows never mapped", || stack(&h).len() == 4);
    let s = stack(&h);
    assert_eq!(s.last(), Some(&("files".to_string(), true)), "{s:?}");
    assert!(s.iter().all(|(title, focused)| *focused == (title == "files")), "{s:?}");
    close_all(&h, vec![editor, files, again, agent]);
}

#[test]
fn a_hotkey_launch_of_the_compositor_takes_the_keyboard() {
    let h = harness();
    let (_dir, socket) = listen(&h);
    let editor = focused_process(&h, &socket);
    let line = launch_line(&socket, "terminal");
    h.with(move |ede| ede.spawn_for_person(&["sh".to_string(), "-c".to_string(), line]));
    until("the hotkey's window never took the keyboard", || stack(&h).last() == Some(&("terminal".to_string(), true)));
    close_all(&h, vec![editor]);
}

#[test]
fn only_the_shell_gets_a_launch_token_and_it_runs_out() {
    let h = harness();
    let mut root = h.ipc();
    let agent = root.mint(SessionId::new("agent-1").unwrap(), vec![Capability::Ui, Capability::InputPerson], None).unwrap();
    let mut agent = h.ipc_with(agent.token).unwrap();
    match agent.launch_token() {
        Err(Error::Failed(f)) => assert_eq!(f.code, ErrorCode::Forbidden),
        other => panic!("an agent got a launch token: {other:?}"),
    }
    let (late, fresh) = (root.launch_token().unwrap(), root.launch_token().unwrap());
    let (late_used, fresh_used, twice) = h.with(move |ede| {
        let now = Instant::now();
        (ede.take_launch_token(&late, now + Duration::from_secs(121)), ede.take_launch_token(&fresh, now + Duration::from_secs(119)), ede.take_launch_token(&fresh, now))
    });
    assert_eq!((late_used, fresh_used, twice), (false, true, false), "120 s, once");
}

#[derive(Default)]
struct Activation {
    global: Option<xdg_activation_v1::XdgActivationV1>,
    token: Option<String>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Activation {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            if interface == "xdg_activation_v1" {
                state.global = Some(registry.bind(name, 1, qh, ()));
            }
        }
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for Activation {
    fn event(_: &mut Self, _: &xdg_activation_v1::XdgActivationV1, _: xdg_activation_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for Activation {
    fn event(state: &mut Self, _: &xdg_activation_token_v1::XdgActivationTokenV1, event: xdg_activation_token_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.token = Some(token);
        }
    }
}

/// A running instance that was handed a token (a second browser window
/// from the menu) activates its window with it; a token the background app
/// made up or asked for itself does nothing.
#[test]
fn a_running_app_activates_its_window_with_the_launch_token_only() {
    let h = harness();
    let (mut app, _t) = focused_app(&h);
    let mut browser = Session::connect(h.connect());
    let b = browser.toplevel("browser");
    browser.draw(&b, 300, 200, BLUE);
    wait_for(&mut browser, |_| stack(&h).len() == 2);
    assert_eq!(stack(&h)[0], ("browser".to_string(), false));

    let mut queue = browser.conn.new_event_queue::<Activation>();
    let qh = queue.handle();
    let mut act = Activation::default();
    browser.conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut act).unwrap();
    let global = act.global.clone().expect("xdg_activation_v1 advertised");

    // Its own token, asked for without the keyboard, and a made-up one.
    let own = global.get_activation_token(&qh, ());
    own.commit();
    queue.roundtrip(&mut act).unwrap();
    global.activate(act.token.take().expect("a token"), &b.surface);
    global.activate("made-up".into(), &b.surface);
    queue.roundtrip(&mut act).unwrap();
    app.roundtrip();
    assert_eq!(stack(&h)[1], ("editor".to_string(), true), "a background app activated itself");

    let token = h.ipc().launch_token().unwrap();
    global.activate(token, &b.surface);
    queue.roundtrip(&mut act).unwrap();
    wait_for(&mut browser, |_| stack(&h)[1] == ("browser".to_string(), true));
}
