//! #EDE-172 (P3, design 3.5): the virtual keyboard and input-method
//! managers go to installed input methods alone, known by the file their
//! process runs (as eDE's own binaries are, `tests/peer.rs`). An ordinary
//! client sees neither; a listed root-owned binary sees both and types into
//! the focused window; the same file name at another path (a copy) sees
//! neither, nor does a copy named like a default input method (`fcitx5`),
//! nor a binary that only a list the person can write names.
//!
//! And an input method that takes the keyboard and hands a key back through
//! its own virtual keyboard while the person still holds it: the window gets
//! the key once and the input method hears its release (before, both were
//! swallowed as the same key held by two sources, and the input method
//! repeated it forever).
//!
//! The listed binary is the dynamic loader running this test binary again
//! (`ld-linux… <test> …`): the process then runs the loader's file, which
//! root owns on every runner, so its `/proc/<pid>/exe` is that file. The
//! roles run as this test binary started again ([`ROLE`]).

mod common;

use std::{
    io::{BufRead, BufReader},
    os::fd::{AsFd, OwnedFd},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, peer::root_owned, Config, InputMethods};
use smithay::backend::input::KeyState;
use wayland_client::{
    delegate_noop,
    protocol::{wl_registry, wl_seat},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{zwp_input_method_keyboard_grab_v2, zwp_input_method_manager_v2, zwp_input_method_v2},
    zwp_virtual_keyboard_v1::client::{zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1},
};

const ROLE: &str = "EDE172_IM_ROLE";
const SOCKET: &str = "EDE172_IM_SOCKET";
const VK: &str = "zwp_virtual_keyboard_manager_v1";
const IM: &str = "zwp_input_method_manager_v2";
const KEY_A: u32 = 30;
/// What the role says: the managers it sees, and that it typed.
const SEES: &str = "EDE172 sees: ";
const TYPED: &str = "EDE172 typed";
const BLUE: u32 = 0xff00_00ff;

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

/// The dynamic loader this test binary runs under, when root owns it.
fn loader() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    let path = maps.lines().filter_map(|l| l.split_whitespace().nth(5)).find(|p| Path::new(p).file_name().is_some_and(|n| n.to_string_lossy().starts_with("ld-linux")))?;
    let path = std::fs::canonicalize(path).ok()?;
    root_owned(&path).map(|_| path)
}

/// The person's desk with a focused window that counts its keys, and a
/// socket of the person's for the roles.
struct Desk {
    h: Harness,
    app: Session,
    listener: UnixListener,
    socket: PathBuf,
    dir: tempfile::TempDir,
}

fn desk() -> Desk {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let notes = app.toplevel("notes");
    app.draw(&notes, 200, 200, BLUE);
    wait_for(&mut app, |a| a.state.keyboard_focus.as_ref() == Some(&notes.surface));
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wayland-test");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    Desk { h, app, listener, socket, dir }
}

impl Desk {
    /// `argv` (a copy of this test binary, or the loader and this test
    /// binary), in the role [`role_type`], connects the person's socket;
    /// which of the two managers it saw, `(virtual keyboard, input
    /// method)`. A role that got the virtual keyboard has typed an `a` with
    /// it once this returns.
    fn connect(&self, argv: &[&Path]) -> (bool, bool) {
        let (program, args) = argv.split_first().unwrap();
        let mut command = Command::new(program);
        command.args(args).args(["--exact", "role_type", "--ignored", "--nocapture", "--test-threads=1"]);
        command.env(ROLE, "type").env(SOCKET, &self.socket).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = Killed(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(10);
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => panic!("{} never connected: {e}", program.display()),
            }
        };
        stream.set_nonblocking(false).unwrap();
        self.h.with(move |ede| ede.insert_person_client(stream));
        // Its lines, read on a thread so that a role that never says them
        // fails the test instead of hanging it. The test harness writes
        // `test role_type ... ` before what the role says.
        let (tx, said) = std::sync::mpsc::channel();
        let stdout = child.0.stdout.take().unwrap();
        std::thread::spawn(move || BufReader::new(stdout).lines().map_while(Result::ok).try_for_each(|l| tx.send(l)));
        let next = |what: &str| loop {
            match said.recv_timeout(Duration::from_secs(15)) {
                Ok(line) if line.contains(what) => break line,
                Ok(_) => {}
                Err(e) => panic!("{} never said {what:?}: {e}", program.display()),
            }
        };
        let sees = next(SEES);
        let sees = sees.split_once(SEES).unwrap().1;
        let seen = (sees.split(' ').any(|g| g == VK), sees.split(' ').any(|g| g == IM));
        if seen.0 {
            next(TYPED);
        }
        seen
    }

    /// The keys the focused window heard (pressed, released), after the
    /// compositor had time to pass on anything a role sent.
    fn keys(&mut self) -> (Vec<u32>, Vec<u32>) {
        std::thread::sleep(Duration::from_millis(50));
        self.app.roundtrip();
        (self.app.state.keys.clone(), self.app.state.key_releases.clone())
    }
}

/// The compositor's own pair and a real process of the person that runs
/// no input method's file see neither manager; text-input-v3 stays theirs.
#[test]
fn an_ordinary_client_sees_no_virtual_keyboard_manager() {
    let mut d = desk();
    let own = Session::connect(d.h.connect());
    assert!(!own.state.globals.iter().any(|g| g == VK || g == IM), "{:?}", own.state.globals);
    assert!(own.state.globals.iter().any(|g| g == "zwp_text_input_manager_v3"), "text-input-v3 is every app's");

    let test = std::env::current_exe().unwrap();
    assert_eq!(d.connect(&[test.as_path()]), (false, false), "this test binary is no input method");
    assert_eq!(d.keys(), (vec![], vec![]));
}

/// A listed root-owned binary sees both managers and types into the
/// focused window; a copy of that very file at another path, with the
/// same name, sees neither and types nothing.
#[test]
fn a_listed_binary_types_and_the_same_name_at_another_path_does_not() {
    let Some(loader) = loader() else {
        eprintln!("skipped: no root-owned dynamic loader here");
        return;
    };
    let mut d = desk();
    let test = std::env::current_exe().unwrap();
    let listed = loader.clone();
    d.h.with(move |ede| ede.input_methods = InputMethods::of(&[listed]));
    assert_eq!(d.connect(&[loader.as_path(), test.as_path()]), (true, true), "{} is listed", loader.display());
    wait_for(&mut d.app, |a| (a.state.keys.as_slice(), a.state.key_releases.as_slice()) == (&[KEY_A][..], &[KEY_A][..]));

    let copy = d.dir.path().join(loader.file_name().unwrap());
    std::fs::copy(&loader, &copy).unwrap();
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(d.connect(&[copy.as_path(), test.as_path()]), (false, false), "a copy at {} is another file", copy.display());
    assert_eq!(d.keys(), (vec![KEY_A], vec![KEY_A]), "the copy typed nothing");
}

/// A copy named like a default input method (`fcitx5` in a directory the
/// person owns) is none, and a list the person can write lists nothing,
/// not even a root-owned binary.
#[test]
fn a_renamed_copy_and_a_list_the_person_writes_count_for_nothing() {
    let mut d = desk();
    d.h.with(|ede| ede.input_methods = InputMethods::system());
    let fcitx5 = d.dir.path().join("fcitx5");
    std::fs::copy(std::env::current_exe().unwrap(), &fcitx5).unwrap();
    std::fs::set_permissions(&fcitx5, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(d.connect(&[fcitx5.as_path()]), (false, false), "a copy named fcitx5 is no input method");

    if let Some(loader) = loader() {
        let list = d.dir.path().join("input-methods.toml");
        std::fs::write(&list, format!("paths = [{:?}]\n", loader.display().to_string())).unwrap();
        let methods = InputMethods::of(&[]).with_list(list);
        d.h.with(move |ede| ede.input_methods = methods);
        assert_eq!(d.connect(&[loader.as_path(), std::env::current_exe().unwrap().as_path()]), (false, false), "a list the person owns counts for nothing");
    }
    assert_eq!(d.keys(), (vec![], vec![]));
}

/// An input method as fcitx5 and ibus are: it grabs the keyboard and hands
/// back what it does not use through a virtual keyboard of its own.
#[derive(Default)]
struct Ime {
    seat: Option<wl_seat::WlSeat>,
    input_methods: Option<zwp_input_method_manager_v2::ZwpInputMethodManagerV2>,
    virtual_keyboards: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
    /// The grab's keymap (format, fd, size).
    keymap: Option<(u32, OwnedFd, u32)>,
    /// Every key the grab handed it: (evdev code, pressed).
    keys: Vec<(u32, bool)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Ime {
    fn event(ime: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_seat" => ime.seat = Some(registry.bind(name, version.min(7), qh, ())),
                IM => ime.input_methods = Some(registry.bind(name, 1, qh, ())),
                VK => ime.virtual_keyboards = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2, ()> for Ime {
    fn event(ime: &mut Self, _: &zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2, event: zwp_input_method_keyboard_grab_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Keymap { format, fd, size } => {
                let format = match format {
                    WEnum::Value(f) => f as u32,
                    WEnum::Unknown(f) => f,
                };
                ime.keymap = Some((format, fd, size));
            }
            zwp_input_method_keyboard_grab_v2::Event::Key { key, state, .. } => ime.keys.push((key, state == WEnum::Value(wayland_client::protocol::wl_keyboard::KeyState::Pressed))),
            _ => {}
        }
    }
}

delegate_noop!(Ime: ignore wl_seat::WlSeat);
delegate_noop!(Ime: zwp_input_method_manager_v2::ZwpInputMethodManagerV2);
delegate_noop!(Ime: ignore zwp_input_method_v2::ZwpInputMethodV2);
delegate_noop!(Ime: zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(Ime: zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);

fn until_ime(queue: &mut EventQueue<Ime>, ime: &mut Ime, what: &str, ok: impl Fn(&Ime) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(ime) {
        assert!(Instant::now() < deadline, "{what}: the input method heard {:?}", ime.keys);
        std::thread::sleep(Duration::from_millis(10));
        queue.roundtrip(ime).unwrap();
    }
}

/// The person presses `a`; the input method's grab takes it and the input
/// method hands it back through its virtual keyboard while the key is still
/// down; the person lets go. The window gets the `a` once, pressed and
/// released, and the input method hears the release, so it never repeats
/// the key on its own.
#[test]
fn an_input_methods_key_reaches_the_window_and_its_release_reaches_the_input_method() {
    let mut d = desk();
    let conn = Connection::from_socket(d.h.connect_input_method()).unwrap();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut ime = Ime::default();
    queue.roundtrip(&mut ime).unwrap();
    let seat = ime.seat.clone().expect("the person's seat");
    let input_method = ime.input_methods.as_ref().expect("the input-method manager").get_input_method(&seat, &qh, ());
    let _grab = input_method.grab_keyboard(&qh, ());
    queue.roundtrip(&mut ime).unwrap();

    d.h.with(|ede| ede.keyboard_key((KEY_A + 8).into(), KeyState::Pressed, 0));
    until_ime(&mut queue, &mut ime, "the grab never handed over the press", |ime| ime.keys == [(KEY_A, true)] && ime.keymap.is_some());
    let (format, fd, size) = ime.keymap.take().unwrap();
    let vk = ime.virtual_keyboards.as_ref().expect("the virtual keyboard manager").create_virtual_keyboard(&seat, &qh, ());
    vk.keymap(format, fd.as_fd(), size);
    vk.key(1, KEY_A, 1);
    queue.roundtrip(&mut ime).unwrap();
    wait_for(&mut d.app, |a| a.state.keys == [KEY_A]);

    d.h.with(|ede| ede.keyboard_key((KEY_A + 8).into(), KeyState::Released, 0));
    until_ime(&mut queue, &mut ime, "the release never reached the input method", |ime| ime.keys == [(KEY_A, true), (KEY_A, false)]);
    vk.key(2, KEY_A, 0);
    queue.roundtrip(&mut ime).unwrap();
    wait_for(&mut d.app, |a| a.state.key_releases == [KEY_A]);
    assert_eq!(d.keys(), (vec![KEY_A], vec![KEY_A]), "the window got the key once");
}

/// The role: connect, say which managers it sees, type an `a` with the
/// virtual keyboard when it got one, then wait for stdin to close.
#[test]
#[ignore = "#EDE-172: a role of the input-method tests"]
fn role_type() {
    if std::env::var(ROLE).as_deref() != Ok("type") {
        return;
    }
    let mut me = Session::connect(UnixStream::connect(std::env::var_os(SOCKET).unwrap()).unwrap());
    let sees: Vec<&str> = [VK, IM].into_iter().filter(|g| me.state.globals.iter().any(|seen| seen == g)).collect();
    println!("{SEES}{}", if sees.is_empty() { "neither".to_string() } else { sees.join(" ") });
    if me.state.virtual_keyboard_manager.is_some() {
        me.keyboard();
        let vk = me.virtual_keyboard();
        for (time, state) in [(0, 1), (1, 0)] {
            vk.key(time, KEY_A, state);
        }
        me.roundtrip();
        println!("{TYPED}");
    }
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}
