//! #EDE-78: the real `exponential-sealed lock` against the headless
//! compositor, in a runtime directory of its own; #EDE-127: the real
//! `exponential-sealed polkit` against it and a stand-in polkitd. Needs the
//! built binary (and `dbus-daemon` for the polkit agent's bus):
//! `EDE_SEALED_BIN=<path> cargo test -p ede-compositor --test sealed_client -- --ignored`.

mod common;

use std::{
    collections::HashMap,
    os::unix::fs::DirBuilderExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{Capability, SessionId};

/// Each test points the process's `XDG_RUNTIME_DIR` (which the compositor
/// and the clients it starts read) at a directory of its own: one at a time.
static ENV: Mutex<()> = Mutex::new(());

const RED: u32 = 0xffff_0000;
// evdev code; xkb = evdev + 8
const KEY_A: u32 = 30;

/// Does output `index` show anything but black?
fn shows(h: &Harness, index: usize) -> bool {
    h.capture(index).pixels.iter().any(|p| p & 0x00ff_ffff != 0)
}

fn wait(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "needs EDE_SEALED_BIN, the built exponential-sealed"]
fn the_lock_client_covers_every_output_and_follows_them() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let bin = std::env::var("EDE_SEALED_BIN").expect("EDE_SEALED_BIN");
    let runtime = tempfile::tempdir().unwrap();
    // Under ENV: no other test reads the environment meanwhile.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let h = Harness::start(Config::default(), &[(800, 600), (1024, 768)]);
    h.with(|ede| ede.listen().map(|_| ())).unwrap();
    h.with(move |ede| ede.begin_sealed_lock_with(vec![bin, "lock".into()]));
    wait("the first output never showed the prompt", || shows(&h, 0));
    wait("the second output stayed black", || shows(&h, 1));

    // One goes: the prompt stays on the other, once.
    h.with_backend(|ede, backend| backend.remove_output(ede, 1));
    wait("the lock client kept the surface of an output that went", || h.with(|ede| ede.sealed_lock_surfaces().len()) == 1);
    assert!(shows(&h, 0));

    // A new one comes: it shows the prompt too.
    h.with_backend(|ede, backend| {
        backend.add_output(ede, "HEADLESS-3", (1280, 720));
    });
    wait("a new output stayed black", || shows(&h, 1));
    assert!(shows(&h, 0));
    assert!(h.with(|ede| ede.is_locked()));
}

/// #EDE-181: a sealed prompt's card hides the window under it. Over the
/// person's window, bright and busy, the polkit prompt, a consent and the
/// share chooser each show the same pixels in their card's padding
/// whatever the window shows under it: its pattern, then the inverse. With
/// `EDE_PICTURES=<dir>` each is pictured over the window.
#[test]
#[ignore = "needs EDE_SEALED_BIN, the built exponential-sealed, and dbus-daemon"]
fn every_sealed_prompt_hides_the_window_under_its_card() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let bin = std::env::var("EDE_SEALED_BIN").expect("EDE_SEALED_BIN");
    let dir = tempfile::tempdir().unwrap();
    let runtime = dir.path().join("run");
    std::fs::DirBuilder::new().mode(0o700).create(&runtime).unwrap();
    // Under ENV: no other test reads the environment meanwhile.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime) };
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    busy(&mut app, &t, false);
    wait("the person's window never got the keyboard", || {
        app.roundtrip();
        app.state.keyboard_focus.as_ref() == Some(&t.surface)
    });
    let under = h.capture(0);
    for doc in [POLKIT, CONSENT, SHARE] {
        assert!(card_padding(&under, card_width(doc)).iter().all(|p| p & 0x00ff_ffff != 0), "the person's window does not lie under the cards");
    }
    let display = h.with(|ede| ede.listen()).unwrap();
    let sealed = |mode: &str| {
        let mut c = Command::new(&bin);
        c.arg(mode).env("WAYLAND_DISPLAY", &display).env("XDG_RUNTIME_DIR", &runtime).stdout(Stdio::null());
        c
    };

    // The polkit prompt, for the person's own `pkexec`.
    let (_bus, address) = private_bus(dir.path());
    let authority = Authority::default();
    let polkitd = zbus::blocking::connection::Builder::address(address.as_str())
        .unwrap()
        .name("org.freedesktop.PolicyKit1")
        .unwrap()
        .serve_at("/org/freedesktop/PolicyKit1/Authority", authority.clone())
        .unwrap()
        .build()
        .unwrap();
    let agent = Kill(sealed("polkit").env("DBUS_SYSTEM_BUS_ADDRESS", &address).env("XDG_SESSION_ID", "ede181").spawn().unwrap());
    wait("the agent never registered with polkitd", || authority.agent.lock().unwrap().is_some());
    let registered = authority.agent.lock().unwrap().clone().unwrap();
    let person = Kill(Command::new("sleep").arg("120").spawn().unwrap());
    let reply = prompt_with_the_keyboard(&h, &mut app, &polkitd, &registered, "ede181", person.0.id());
    hides(&h, &mut app, &t, "polkit", card_width(POLKIT));
    cancel(&h, &mut app, &t, &polkitd, &registered, "ede181", reply);
    drop(agent);

    // A consent.
    let consent = Kill(sealed("consent").args(["--title", "Install the package list?", "--body", "An agent asks to run apt-get update as the super user."]).spawn().unwrap());
    hides(&h, &mut app, &t, "consent", card_width(CONSENT));
    drop(consent);
    wait("the consent stayed after its end", || layers(&h) == 0);

    // The share chooser.
    let mut share = sealed("share").stdin(Stdio::piped()).spawn().unwrap();
    let ask = r#"{"app_id": "org.example.Viewer", "types": ["monitor", "window"], "multiple": false, "remote_control": false, "outputs": [{"name": "HEADLESS-1", "width": 800, "height": 600}], "windows": [{"id": "1", "title": "app", "app_id": "app"}]}"#;
    std::io::Write::write_all(&mut share.stdin.take().unwrap(), ask.as_bytes()).unwrap();
    let share = Kill(share);
    hides(&h, &mut app, &t, "share", card_width(SHARE));
    drop(share);
}

const POLKIT: &str = include_str!("../../../sealed/polkit.json");
const CONSENT: &str = include_str!("../../../sealed/consent.json");
const SHARE: &str = include_str!("../../../sealed/share.json");

/// The width of a sealed document's `card`.
fn card_width(doc: &str) -> i32 {
    fn find(node: &serde_json::Value) -> Option<i32> {
        if node["id"] == "card" {
            return node["style"]["width"].as_i64().map(|w| w as i32);
        }
        node["children"].as_array()?.iter().find_map(find)
    }
    find(&serde_json::from_str::<serde_json::Value>(doc).unwrap()["root"]).expect("a card with a width")
}

/// The person's window over the whole output, bright and busy: diagonal
/// bands of saturated colour under a fine white checker; `inverse` = every
/// pixel's colour inverted.
fn busy(app: &mut Session, t: &common::Toplevel, inverse: bool) {
    const BANDS: [u32; 6] = [0xff_0000, 0xff_ff00, 0x00_ff00, 0x00_ffff, 0x00_00ff, 0xff_00ff];
    app.draw_with(t, 800, 600, |x, y| {
        let rgb = if (x / 4 + y / 4) % 2 == 0 { BANDS[((x + y) / 24 % 6) as usize] } else { 0xff_ffff };
        0xff00_0000 | if inverse { rgb ^ 0xff_ffff } else { rgb }
    });
}

/// The left and right padding of a `width`-wide card centred on the
/// output, over the 120 rows round the middle (inside every card): no
/// content, only the card.
fn card_padding(frame: &ede_compositor::backend::headless::Frame, width: i32) -> Vec<u32> {
    let (cx, cy) = (frame.width / 2, frame.height / 2);
    let columns: Vec<i32> = (4..20).flat_map(|d| [cx - width / 2 + d, cx + width / 2 - 1 - d]).collect();
    (cy - 60..cy + 60).flat_map(|y| columns.iter().map(move |&x| frame.at(x, y))).collect()
}

/// A prompt of `name` drew over the person's window: its card's padding
/// is the same over the window's pattern and over its inverse.
fn hides(h: &Harness, app: &mut Session, t: &common::Toplevel, name: &str, width: i32) {
    wait(&format!("the {name} prompt never drew"), || drawn(h));
    // Its first frames settle.
    let mut last = card_padding(&h.capture(0), width);
    wait(&format!("the {name} prompt never settled"), || {
        std::thread::sleep(Duration::from_millis(300));
        let now = card_padding(&h.capture(0), width);
        std::mem::replace(&mut last, now) == last
    });
    picture_as(h, &format!("{name}-over-a-busy-window"));
    busy(app, t, true);
    let inverse = card_padding(&h.capture(0), width);
    busy(app, t, false);
    let changed = last.iter().zip(&inverse).filter(|(a, b)| a != b).count();
    assert_eq!(changed, 0, "{changed} of {} pixels of the {name} prompt's card show the window under it", last.len());
}

/// A layer surface on the person's outputs has drawn.
fn drawn(h: &Harness) -> bool {
    h.with(|ede| {
        ede.desk.space.outputs().any(|o| smithay::desktop::layer_map_for_output(o).layers().any(|l| smithay::backend::renderer::utils::with_renderer_surface_state(l.wl_surface(), |s| s.buffer().is_some()).unwrap_or(false)))
    })
}

/// A process this test started, ended with the test.
struct Kill(Child);

impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The agent's system bus: a `dbus-daemon` of this test's own.
fn private_bus(dir: &Path) -> (Kill, String) {
    let socket = dir.join("system_bus_socket");
    let config = dir.join("bus.conf");
    let policy = r#"<policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy>"#;
    std::fs::write(&config, format!("<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth>{policy}</busconfig>\n", socket.display())).unwrap();
    let daemon = Command::new("dbus-daemon").arg(format!("--config-file={}", config.display())).args(["--nofork", "--nopidfile"]).stdout(Stdio::null()).spawn().expect("dbus-daemon (the dbus package)");
    let daemon = Kill(daemon);
    wait("the private bus never listened", || socket.exists());
    (daemon, format!("unix:path={}", socket.display()))
}

/// polkitd's side, as much of it as the agent meets: the authority the
/// agent registers with, which then asks the agent to authenticate.
#[derive(Clone, Default)]
struct Authority {
    /// The agent's bus name and object path, once it registered.
    agent: Arc<Mutex<Option<(String, String)>>>,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
impl Authority {
    fn register_authentication_agent(&self, #[zbus(header)] header: zbus::message::Header<'_>, _subject: (String, HashMap<String, zbus::zvariant::OwnedValue>), _locale: String, object_path: String) {
        *self.agent.lock().unwrap() = header.sender().map(|sender| (sender.to_string(), object_path));
    }
}

const AGENT_IFACE: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";

/// polkitd's `BeginAuthentication` for `pkexec true` as the person: the
/// request names its subject's and its caller's process, as polkitd does.
/// The agent answers when the prompt is done (or at once, refused).
fn begin(polkitd: &zbus::blocking::Connection, (name, path): &(String, String), cookie: &str, subject: u32, caller: u32) -> mpsc::Receiver<Result<(), String>> {
    let (conn, name, path, cookie) = (polkitd.clone(), name.clone(), path.clone(), cookie.to_string());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let uid = smithay::reexports::rustix::process::getuid().as_raw();
        let identity: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::from([("uid", zbus::zvariant::Value::from(uid))]);
        let details: HashMap<&str, String> = HashMap::from([("polkit.subject-pid", subject.to_string()), ("polkit.caller-pid", caller.to_string())]);
        let body = ("org.freedesktop.policykit.exec", "Authentication is needed to run `/usr/bin/true' as the super user", "", details, cookie.as_str(), vec![("unix-user", identity)]);
        let reply = conn.call_method(Some(name.as_str()), path.as_str(), Some(AGENT_IFACE), "BeginAuthentication", &body);
        let _ = tx.send(reply.map(|_| ()).map_err(|e| e.to_string()));
    });
    rx
}

/// `pid`'s parent is not in the process group `group` (it was reparented).
fn parent_outside(pid: u32, group: u32) -> bool {
    let field = |pid: u32, n: usize| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|stat| stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(n).and_then(|f| f.parse::<u32>().ok())));
    field(pid, 1).is_some_and(|parent| field(parent, 2) != Some(group))
}

/// polkitd asks about `subject` (the caller is a system service, as for
/// `systemctl`): the agent refuses, and nothing maps on the person's desk
/// meanwhile; the refusal names an agent space.
fn refused(h: &Harness, app: &mut Session, t: &common::Toplevel, polkitd: &zbus::blocking::Connection, registered: &(String, String), cookie: &str, subject: u32) {
    let reply = begin(polkitd, registered, cookie, subject, 1);
    let deadline = Instant::now() + Duration::from_secs(15);
    let answer = loop {
        assert_eq!(layers(h), 0, "a prompt mapped on the person's desk for {cookie}");
        app.roundtrip();
        assert_eq!(app.state.keyboard_focus.as_ref(), Some(&t.surface), "the person's window lost the keyboard to {cookie}");
        if let Ok(answer) = reply.try_recv() {
            break answer;
        }
        assert!(Instant::now() < deadline, "the agent never answered {cookie}");
        std::thread::sleep(Duration::from_millis(50));
    };
    let refusal = answer.expect_err("a request from an agent space was authenticated");
    assert!(refusal.contains("agent space"), "{cookie}: {refusal}");
}

/// The layer surfaces on the person's outputs (the prompt is one).
fn layers(h: &Harness) -> usize {
    h.with(|ede| ede.desk.space.outputs().map(|o| smithay::desktop::layer_map_for_output(o).layers().count()).sum())
}

/// #EDE-127: polkitd finds the authentication agent by the login session,
/// and an agent space's processes are in the person's session. So the real
/// agent hears a request whose subject is a process launched into a space:
/// it refuses it before anything maps (no prompt, the person's window keeps
/// the keyboard and their next key), and the request fails. A request from
/// the person's own process still opens the prompt, which takes the
/// keyboard. #EDE-143: while an agent space exists, a process outside the
/// person's session (and, while the space's launches run outside systemd
/// scopes, every process) gets the prompt without the keyboard until a
/// click; with no space (before the first, after the last closed) the same
/// request takes the keyboard as before. With `EDE_PICTURES=<dir>` the
/// states are pictured.
#[test]
#[ignore = "needs EDE_SEALED_BIN, the built exponential-sealed, and dbus-daemon"]
fn the_polkit_agent_refuses_a_request_from_an_agent_space_before_anything_maps() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let bin = std::env::var("EDE_SEALED_BIN").expect("EDE_SEALED_BIN");
    let dir = tempfile::tempdir().unwrap();
    let runtime = dir.path().join("run");
    std::fs::DirBuilder::new().mode(0o700).create(&runtime).unwrap();
    // Under ENV: no other test reads the environment meanwhile.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime) };
    let mut config = Config::default();
    config.keyboard.layout = "us".into();
    let h = Harness::start(config, &[(800, 600)]);

    // The person's window, with the keyboard.
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    wait("the person's window never got the keyboard", || {
        app.roundtrip();
        app.state.keyboard_focus.as_ref() == Some(&t.surface)
    });
    let display = h.with(|ede| ede.listen()).unwrap();

    // polkitd on a bus of the test's own, and the real agent registered with it.
    let (_bus, address) = private_bus(dir.path());
    let authority = Authority::default();
    let polkitd = zbus::blocking::connection::Builder::address(address.as_str())
        .unwrap()
        .name("org.freedesktop.PolicyKit1")
        .unwrap()
        .serve_at("/org/freedesktop/PolicyKit1/Authority", authority.clone())
        .unwrap()
        .build()
        .unwrap();
    let agent = Command::new(&bin).arg("polkit").env("DBUS_SYSTEM_BUS_ADDRESS", &address).env("XDG_SESSION_ID", "ede127").env("WAYLAND_DISPLAY", &display).env("XDG_RUNTIME_DIR", &runtime).spawn().unwrap();
    let _agent = Kill(agent);
    wait("the agent never registered with polkitd", || authority.agent.lock().unwrap().is_some());
    let registered = authority.agent.lock().unwrap().clone().unwrap();

    // 0. #EDE-143: no agent space yet. A process outside the person's
    // session (here: the session seen from another cgroup, as a terminal's
    // scope is) asks: the prompt takes the keyboard, as before #EDE-143.
    let person = Kill(Command::new("sleep").arg("120").spawn().unwrap());
    let elsewhere = || h.with(|ede| ede.spaces.session_cgroup = Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/elsewhere.scope\n".into()));
    elsewhere();
    let reply = prompt_with_the_keyboard(&h, &mut app, &polkitd, &registered, "no-space", person.0.id());
    picture(&h, "no-space-with-the-keyboard");
    cancel(&h, &mut app, &t, &polkitd, &registered, "no-space", reply);

    // An agent space, and a process its shell started there.
    let mut root = h.ipc();
    assert!(root.hello.offers_spaces());
    let grant = root.mint(SessionId::new("run-ede127").unwrap(), vec![Capability::Space], None).unwrap();
    let mut space = h.ipc_with(grant.token).unwrap();
    let info = space.create_space(640, 480).unwrap();

    // 0b. The space is open (nothing launched yet): the same request shows
    // the prompt WITHOUT the keyboard and names the process; a click into
    // it takes the keyboard.
    let reply = prompt_without_the_keyboard(&h, &mut app, &t, &polkitd, &registered, "space-open", person.0.id());
    picture(&h, "space-open-without-the-keyboard");
    click_into_the_prompt(&h, &mut app);
    cancel(&h, &mut app, &t, &polkitd, &registered, "space-open", reply);
    h.with(|ede| ede.spaces.session_cgroup = None);
    // A child of its shell, and one that cleared its environment and lost
    // its parent (only the compositor still knows it: its launch's group).
    let pids = dir.path().join("pids");
    std::fs::create_dir(&pids).unwrap();
    let script = r#"
        ( env -i sleep 120 & echo $! > "$D/orphan.tmp"; mv "$D/orphan.tmp" "$D/orphan" )
        sleep 120 & echo $! > "$D/child.tmp"; mv "$D/child.tmp" "$D/child"
        wait"#;
    let head = space.launch(vec!["sh".into(), "-c".into(), script.into()], [("D".to_string(), pids.to_string_lossy().into_owned())].into(), None).unwrap();
    let pid_in = |name: &str| {
        let file = pids.join(name);
        wait(&format!("the space's shell never wrote {name}"), || file.exists());
        std::fs::read_to_string(&file).unwrap().trim().parse::<u32>().unwrap()
    };
    let (child, orphan) = (pid_in("child"), pid_in("orphan"));
    // `$!` comes before the fork has run `env`, and `env` before `sleep`.
    wait("the space's children never ran sleep", || [child, orphan].into_iter().all(|pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|comm| comm.trim() == "sleep")));
    wait("the orphan kept its parent", || parent_outside(orphan, head));

    // 1. The space's processes ask: refused, nothing maps on the person's desk.
    refused(&h, &mut app, &t, &polkitd, &registered, &format!("a request from {}'s shell", info.id.as_str()), child);
    refused(&h, &mut app, &t, &polkitd, &registered, &format!("a request from {}'s orphan", info.id.as_str()), orphan);
    // Nothing comes later either, and the person's next key is theirs.
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(100));
        app.roundtrip();
        assert_eq!(layers(&h), 0, "a prompt mapped after the refusal");
        assert_eq!(app.state.keyboard_focus.as_ref(), Some(&t.surface));
    }
    common::space::person_key(&h, KEY_A);
    wait("the person's key went elsewhere", || {
        app.roundtrip();
        app.state.keys.contains(&KEY_A)
    });

    // 2. #EDE-143: while the space's launches run outside systemd scopes
    // (this harness has no user manager), the session's cgroup vouches for
    // no one: the person's own request shows the prompt WITHOUT taking the
    // keyboard; what they type stays in their window.
    let without = prompt_without_the_keyboard(&h, &mut app, &t, &polkitd, &registered, "from-person-unscoped", person.0.id());
    cancel(&h, &mut app, &t, &polkitd, &registered, "from-person-unscoped", without);

    // 3. The space closes and its launches end: the person's own request
    // opens the prompt and it takes the keyboard; polkitd's cancel closes
    // it and gives the keyboard back.
    space.destroy_space(info.id.clone()).unwrap();
    wait("the closed space's launches never ended", || h.with(|ede| ede.spaces.launched()) == 0);
    let reply = prompt_with_the_keyboard(&h, &mut app, &polkitd, &registered, "from-person", person.0.id());
    picture(&h, "with-the-keyboard");
    cancel(&h, &mut app, &t, &polkitd, &registered, "from-person", reply);

    // 4. The space closed again: the request from outside the person's
    // session takes the keyboard again, as with no space before.
    elsewhere();
    let reply = prompt_with_the_keyboard(&h, &mut app, &polkitd, &registered, "space-closed", person.0.id());
    picture(&h, "space-closed-with-the-keyboard");
    cancel(&h, &mut app, &t, &polkitd, &registered, "space-closed", reply);
}

/// A request whose prompt takes the keyboard (the person's, or any while
/// no agent space exists): it maps, the person's window loses the
/// keyboard, and nothing answers it without them.
fn prompt_with_the_keyboard(h: &Harness, app: &mut Session, polkitd: &zbus::blocking::Connection, registered: &(String, String), cookie: &str, subject: u32) -> mpsc::Receiver<Result<(), String>> {
    let reply = begin(polkitd, registered, cookie, subject, subject);
    wait(&format!("{cookie}: the request showed no prompt"), || layers(h) > 0);
    wait(&format!("{cookie}: the prompt never took the keyboard"), || {
        app.roundtrip();
        app.state.keyboard_focus.is_none()
    });
    assert!(reply.try_recv().is_err(), "{cookie}: the request was answered without the person");
    reply
}

/// A click into the middle of the screen (the prompt's card) gives the
/// prompt the keyboard.
fn click_into_the_prompt(h: &Harness, app: &mut Session) {
    let (w, hh) = h.with(|ede| ede.desk.space.outputs().next().and_then(|o| o.current_mode()).map(|m| (m.size.w, m.size.h))).unwrap();
    h.with(move |ede| {
        use smithay::backend::input::ButtonState;
        ede.person_pointer_motion_to((f64::from(w) / 2.0, f64::from(hh) / 2.0).into(), 0);
        ede.person_pointer_button(0x110, ButtonState::Pressed, 0);
        ede.person_pointer_button(0x110, ButtonState::Released, 0);
    });
    wait("a click into the prompt never gave it the keyboard", || {
        app.roundtrip();
        app.state.keyboard_focus.is_none()
    });
}

/// A request whose prompt shows without the keyboard (#EDE-143): it maps,
/// and the person's window keeps the keyboard and gets their key.
fn prompt_without_the_keyboard(h: &Harness, app: &mut Session, t: &common::Toplevel, polkitd: &zbus::blocking::Connection, registered: &(String, String), cookie: &str, subject: u32) -> mpsc::Receiver<Result<(), String>> {
    let reply = begin(polkitd, registered, cookie, subject, subject);
    wait(&format!("{cookie} showed no prompt"), || layers(h) > 0);
    // It has drawn (it shows), and a while later it still has no keyboard.
    wait(&format!("{cookie}'s prompt never drew"), || shows_beside(h, t));
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));
        app.roundtrip();
        assert_eq!(app.state.keyboard_focus.as_ref(), Some(&t.surface), "{cookie}'s prompt took the keyboard");
    }
    let keys = app.state.keys.len();
    common::space::person_key(h, KEY_A);
    wait(&format!("the person's key went to {cookie}'s prompt"), || {
        app.roundtrip();
        app.state.keys.len() > keys
    });
    assert!(reply.try_recv().is_err(), "{cookie} was answered without the person");
    reply
}

/// polkitd cancels `cookie`: the request ends as cancelled, the prompt
/// closes, the person's window has the keyboard.
fn cancel(h: &Harness, app: &mut Session, t: &common::Toplevel, polkitd: &zbus::blocking::Connection, registered: &(String, String), cookie: &str, reply: mpsc::Receiver<Result<(), String>>) {
    polkitd.call_method(Some(registered.0.as_str()), registered.1.as_str(), Some(AGENT_IFACE), "CancelAuthentication", &(cookie,)).unwrap();
    assert!(reply.recv_timeout(Duration::from_secs(15)).expect("the cancelled request never ended").is_err());
    wait(&format!("{cookie}'s prompt stayed after the cancel"), || layers(h) == 0);
    wait("the person's window never got the keyboard back", || {
        app.roundtrip();
        app.state.keyboard_focus.as_ref() == Some(&t.surface)
    });
}

/// Something but the person's red window and black shows on the output:
/// the prompt has drawn.
fn shows_beside(h: &Harness, _t: &common::Toplevel) -> bool {
    h.capture(0).pixels.iter().any(|p| p & 0x00ff_ffff != 0 && *p != RED)
}

/// With `EDE_PICTURES=<dir>`: the person's screen as it is now, as
/// `<dir>/polkit-<name>.png` (the pictures of the prompt on #EDE-143).
fn picture(h: &Harness, name: &str) {
    picture_as(h, &format!("polkit-{name}"));
}

/// [`picture`] as `<dir>/<name>.png`.
fn picture_as(h: &Harness, name: &str) {
    let Some(dir) = std::env::var_os("EDE_PICTURES") else { return };
    let frame = h.capture(0);
    let rgba: Vec<u8> = frame.pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 0xff]).collect();
    let path = std::path::Path::new(&dir).join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width as u32, frame.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    eprintln!("picture: {}", path.display());
}
