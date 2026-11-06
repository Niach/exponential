//! #NEW-105 end to end, headless: a private dbus-daemon, a private
//! PipeWire, a headless exponential-compositor on a real ede-protocol socket, and
//! the portal served on that bus. Calls go straight to the impl
//! interfaces, as the xdg-desktop-portal frontend makes them.

#[path = "../../ede-compositor/tests/common/mod.rs"]
mod common;

use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_portal::{Ctx, BUS_NAME, PATH};
use futures_util::StreamExt;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const BLUE: u32 = 0xff00_00ff;
type Vardict = HashMap<String, OwnedValue>;

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One PipeWire (with WirePlumber, which links the consumer to the stream
/// as a session does) for the whole test binary (libpipewire reads its
/// runtime dir from the environment once).
fn pipewire() -> &'static PathBuf {
    static PW: OnceLock<(PathBuf, Daemon, tempfile::TempDir)> = OnceLock::new();
    &PW.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        // SAFETY: set before any thread reads it (first use of the fixture).
        unsafe { std::env::set_var("PIPEWIRE_RUNTIME_DIR", &path) };
        // WirePlumber needs a session bus (device reservation, the portal
        // permission store) and a CI container has none: a private one,
        // so the daemons never see a desktop's. All three live in one
        // shell that kills them when this process goes (its stdin closes,
        // `read` returns); nothing inherits our stdout, so a leftover
        // daemon can never hold a CI step open.
        let bus = format!("unix:path={}", path.join("bus").display());
        let child = Command::new("sh")
            .args([
                "-c",
                "dbus-daemon --session --nofork --address=\"$DBUS_SESSION_BUS_ADDRESS\" > \"$PIPEWIRE_RUNTIME_DIR/dbus.log\" 2>&1 & db=$!; \
                 pipewire > \"$PIPEWIRE_RUNTIME_DIR/pipewire.log\" 2>&1 & pw=$!; sleep 0.5; \
                 wireplumber > \"$PIPEWIRE_RUNTIME_DIR/wireplumber.log\" 2>&1 & wp=$!; \
                 read _; kill $wp $pw $db",
            ])
            .env("PIPEWIRE_RUNTIME_DIR", &path)
            .env("XDG_RUNTIME_DIR", &path)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sh");
        let deadline = Instant::now() + Duration::from_secs(30);
        while !path.join("pipewire-0").exists() {
            assert!(Instant::now() < deadline, "pipewire did not start (is pipewire installed?):\n{}", std::fs::read_to_string(path.join("pipewire.log")).unwrap_or_default());
            std::thread::sleep(Duration::from_millis(20));
        }
        // WirePlumber has no readiness file: it is up once PipeWire lists it
        // as a client (a loaded CI runner takes seconds, so no fixed sleep).
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let clients = Command::new("pw-cli").args(["ls", "Client"]).env("PIPEWIRE_RUNTIME_DIR", &path).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            if clients.to_ascii_lowercase().contains("wireplumber") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "wireplumber did not register with pipewire (are dbus, pipewire and wireplumber installed?):\n{clients}\n--- dbus\n{}\n--- pipewire\n{}\n--- wireplumber\n{}",
                std::fs::read_to_string(path.join("dbus.log")).unwrap_or_default(),
                std::fs::read_to_string(path.join("pipewire.log")).unwrap_or_default(),
                std::fs::read_to_string(path.join("wireplumber.log")).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        // Its session scripts (the link policy) load right after it registers.
        std::thread::sleep(Duration::from_millis(500));
        (path, Daemon(child), dir)
    })
    .0
}

/// A private `dbus-daemon --session` for one world and its address.
fn session_bus() -> (Daemon, String) {
    let mut dbus = Command::new("dbus-daemon").args(["--session", "--nofork", "--print-address"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("dbus-daemon (the dbus package)");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(dbus.stdout.as_mut().unwrap()), &mut line).unwrap();
    (Daemon(dbus), line.trim().to_string())
}

struct World {
    h: Harness,
    bus: zbus::Connection,
    _portal: zbus::Connection,
    _dbus: Daemon,
    dir: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// The tests share one PipeWire and one WirePlumber: one world at a time.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn world(portal_toml: &str) -> World {
    let guard = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter(std::env::var("RUST_LOG").unwrap_or("warn".into())).try_init();
    pipewire();
    let dir = tempfile::tempdir().unwrap();
    let (dbus, address) = session_bus();

    let h = Harness::start(Config::default(), &[(800, 600)]);
    let runtime = dir.path().to_path_buf();
    let socket = h.with(move |ede| ede.ipc_listen(&runtime, "wayland-test")).unwrap();
    let token = ede_protocol::Token::new(std::fs::read_to_string(dir.path().join("exponential-desktop/wayland-test.token")).unwrap()).unwrap();
    let config = dir.path().join("portal.toml");
    // Hermetic: an `exponential-sealed` installed on this machine (the
    // package) must not become the chooser; the tests pick the focused
    // output or window as CI, which has none installed, does.
    let portal_toml = if portal_toml.contains("chooser") {
        portal_toml.to_string()
    } else if portal_toml.contains("[screencast]") {
        portal_toml.replacen("[screencast]\n", "[screencast]\nchooser = []\n", 1)
    } else {
        format!("[screencast]\nchooser = []\n{portal_toml}")
    };
    std::fs::write(&config, format!("[screenshot]\ndir = \"{}\"\n{portal_toml}", dir.path().join("shots").display())).unwrap();
    let connect: ede_portal::Connect = Arc::new(move || ede_protocol::Client::connect(&socket, token.clone()));
    let ctx = Ctx::new(connect, Some(config));
    let (portal, bus) = zbus::block_on(async {
        let portal = ede_portal::serve(zbus::connection::Builder::address(address.as_str()).unwrap(), ctx).await.unwrap();
        let bus = zbus::connection::Builder::address(address.as_str()).unwrap().build().await.unwrap();
        (portal, bus)
    });
    World { h, bus, _portal: portal, _dbus: dbus, dir, _guard: guard }
}

impl World {
    fn call<R: serde::de::DeserializeOwned + zbus::zvariant::Type>(&self, iface: &str, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> zbus::Result<R> {
        zbus::block_on(async {
            let reply = self.bus.call_method(Some(BUS_NAME), PATH, Some(iface), method, body).await?;
            reply.body().deserialize::<R>()
        })
    }

    fn window(&self, title: &str, w: i32, h: i32) -> Session {
        let mut app = Session::connect(self.h.connect());
        app.keyboard();
        let t = app.toplevel(title);
        app.draw(&t, w, h, BLUE);
        app
    }
}

fn path(s: &str) -> ObjectPath<'_> {
    ObjectPath::try_from(s).unwrap()
}

fn none() -> HashMap<&'static str, Value<'static>> {
    HashMap::new()
}

const SCREENCAST: &str = "org.freedesktop.impl.portal.ScreenCast";
const REMOTE: &str = "org.freedesktop.impl.portal.RemoteDesktop";
const SHORTCUTS: &str = "org.freedesktop.impl.portal.GlobalShortcuts";

#[test]
fn settings_say_dark() {
    let w = world("");
    let all: HashMap<String, Vardict> = w.call("org.freedesktop.impl.portal.Settings", "ReadAll", &(vec!["org.freedesktop.*"],)).unwrap();
    assert_eq!(u32::try_from(&all["org.freedesktop.appearance"]["color-scheme"]).unwrap(), 1);
    let one: OwnedValue = w.call("org.freedesktop.impl.portal.Settings", "Read", &("org.freedesktop.appearance", "color-scheme")).unwrap();
    assert_eq!(u32::try_from(&one).unwrap(), 1);
    assert!(w.call::<OwnedValue>("org.freedesktop.impl.portal.Settings", "Read", &("org.gnome.desktop.interface", "gtk-theme")).is_err());
}

#[test]
fn a_screenshot_is_a_png_of_the_screen() {
    let w = world("");
    let _app = w.window("notes", 300, 200);
    std::thread::sleep(Duration::from_millis(100));
    let (response, results): (u32, Vardict) =
        w.call("org.freedesktop.impl.portal.Screenshot", "Screenshot", &(path("/r/1"), "org.example", "", none())).unwrap();
    assert_eq!(response, 0);
    let uri = <&str>::try_from(&results["uri"]).unwrap().to_string();
    let file = uri.strip_prefix("file://").unwrap();
    assert!(file.starts_with(w.dir.path().join("shots").to_str().unwrap()));
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(file).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (800, 600));
    let windows = ede_protocol_client(&w).windows().unwrap();
    let (x, y) = (windows[0].x + 10, windows[0].y + 10);
    let i = ((y * 800 + x) * 4) as usize;
    assert_eq!(&buf[i..i + 4], &[0, 0, 255, 255], "the window is blue in the screenshot");
}

fn ede_protocol_client(w: &World) -> ede_protocol::Client {
    let socket = ede_protocol::socket_path(w.dir.path(), "wayland-test");
    let token = ede_protocol::Token::new(std::fs::read_to_string(w.dir.path().join("exponential-desktop/wayland-test.token")).unwrap()).unwrap();
    ede_protocol::Client::connect(&socket, token).unwrap()
}

#[test]
fn global_shortcuts_bind_and_activate() {
    let w = world("");
    let _app = w.window("notes", 300, 200);
    let session = "/org/freedesktop/portal/desktop/session/1_1/s1";
    let (r, _): (u32, Vardict) = w.call(SHORTCUTS, "CreateSession", &(path("/r/1"), path(session), "org.example.Rec", none())).unwrap();
    assert_eq!(r, 0);
    let mut rec: HashMap<&str, Value> = HashMap::new();
    rec.insert("description", Value::from("Start recording"));
    rec.insert("preferred_trigger", Value::from("CTRL+ALT+r"));
    let (r, results): (u32, Vardict) = w.call(SHORTCUTS, "BindShortcuts", &(path("/r/2"), path(session), vec![("rec", rec)], "", none())).unwrap();
    assert_eq!(r, 0);
    let rows = <Vec<(String, Vardict)>>::try_from(results["shortcuts"].try_clone().unwrap()).unwrap();
    assert_eq!(<&str>::try_from(&rows[0].1["trigger_description"]).unwrap(), "CTRL+ALT+r");

    let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface(SHORTCUTS).unwrap().member("Activated").unwrap().build();
    let mut signals = zbus::block_on(zbus::MessageStream::for_match_rule(rule, &w.bus, None)).unwrap();
    use smithay::backend::input::KeyState;
    for (code, state) in [(29u32, KeyState::Pressed), (56, KeyState::Pressed), (19, KeyState::Pressed), (19, KeyState::Released), (56, KeyState::Released), (29, KeyState::Released)] {
        w.h.with(move |ede| ede.keyboard_key((code + 8).into(), state, 1));
    }
    let msg = zbus::block_on(async {
        let timeout = async_timeout(Duration::from_secs(15));
        futures_util::pin_mut!(timeout);
        match futures_util::future::select(signals.next(), timeout).await {
            futures_util::future::Either::Left((Some(Ok(msg)), _)) => Some(msg),
            _ => None,
        }
    });
    let msg = msg.expect("no Activated signal");
    let body = msg.body();
    let (s, id, _, _): (ObjectPath, String, u64, Vardict) = body.deserialize().unwrap();
    assert_eq!((s.as_str(), id.as_str()), (session, "rec"));

    // Closing the session unbinds: someone else can take the trigger.
    let () = zbus::block_on(w.bus.call_method(Some(BUS_NAME), session, Some("org.freedesktop.impl.portal.Session"), "Close", &())).unwrap().body().deserialize().unwrap();
    let again = ede_protocol_client(&w).bind_shortcuts("/x", vec![ede_protocol::ShortcutSpec { id: "x".into(), trigger: "CTRL+ALT+r".into() }]).unwrap();
    assert!(again[0].trigger.is_some());
}

async fn async_timeout(d: Duration) {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.send(());
    });
    blocking::unblock(move || rx.recv()).await.ok();
}

#[test]
fn remote_control_is_never_silent() {
    let w = world("");
    let session = "/org/freedesktop/portal/desktop/session/1_1/rd1";
    let _: (u32, Vardict) = w.call(REMOTE, "CreateSession", &(path("/r/1"), path(session), "org.example.Remote", none())).unwrap();
    let (r, _): (u32, Vardict) = w.call(REMOTE, "Start", &(path("/r/2"), path(session), "org.example.Remote", "", none())).unwrap();
    assert_eq!(r, 1, "no chooser, not allowed: refused");
    assert!(w.call::<()>(REMOTE, "NotifyPointerMotion", &(path(session), none(), 5.0f64, 5.0f64)).is_err());
}

#[test]
fn an_allowed_app_drives_the_pointer_and_keyboard() {
    let w = world("[remote_desktop]\nallow = [\"org.example.Remote\"]\n");
    let mut app = w.window("notes", 300, 200);
    let session = "/org/freedesktop/portal/desktop/session/1_1/rd2";
    let _: (u32, Vardict) = w.call(REMOTE, "CreateSession", &(path("/r/1"), path(session), "org.example.Remote", none())).unwrap();
    let mut types: HashMap<&str, Value> = HashMap::new();
    types.insert("types", Value::from(1u32));
    let _: (u32, Vardict) = w.call(SCREENCAST, "SelectSources", &(path("/r/2"), path(session), "org.example.Remote", types)).unwrap();
    let (r, results): (u32, Vardict) = w.call(REMOTE, "Start", &(path("/r/3"), path(session), "org.example.Remote", "", none())).unwrap();
    assert_eq!(r, 0);
    assert_eq!(u32::try_from(&results["devices"]).unwrap(), 3);
    let streams = <Vec<(u32, Vardict)>>::try_from(results["streams"].try_clone().unwrap()).unwrap();
    let node = streams[0].0;
    let () = w.call(REMOTE, "NotifyPointerMotionAbsolute", &(path(session), none(), node, 120.0f64, 80.0f64)).unwrap();
    let at = w.h.with(|ede| ede.seat.get_pointer().unwrap().current_location());
    assert_eq!((at.x, at.y), (120.0, 80.0));
    let () = w.call(REMOTE, "NotifyPointerMotion", &(path(session), none(), 5.0f64, -5.0f64)).unwrap();
    let () = w.call(REMOTE, "NotifyKeyboardKeycode", &(path(session), none(), 30i32, 1u32)).unwrap();
    let () = w.call(REMOTE, "NotifyKeyboardKeycode", &(path(session), none(), 30i32, 0u32)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.keys != [30] {
        assert!(Instant::now() < deadline, "the key never arrived");
        std::thread::sleep(Duration::from_millis(10));
        app.roundtrip();
    }
}

#[test]
fn a_screencast_streams_the_output_over_pipewire() {
    let w = world("[screencast]\nframerate = 20\n");
    let _app = w.window("notes", 300, 200);
    let session = "/org/freedesktop/portal/desktop/session/1_1/sc1";
    let _: (u32, Vardict) = w.call(SCREENCAST, "CreateSession", &(path("/r/1"), path(session), "org.example.Meet", none())).unwrap();
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("types", Value::from(1u32));
    opts.insert("persist_mode", Value::from(2u32));
    let _: (u32, Vardict) = w.call(SCREENCAST, "SelectSources", &(path("/r/2"), path(session), "org.example.Meet", opts)).unwrap();
    let (r, results): (u32, Vardict) = w.call(SCREENCAST, "Start", &(path("/r/3"), path(session), "org.example.Meet", "", none())).unwrap();
    assert_eq!(r, 0);
    let streams = <Vec<(u32, Vardict)>>::try_from(results["streams"].try_clone().unwrap()).unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(<(i32, i32)>::try_from(streams[0].1["size"].try_clone().unwrap()).unwrap(), (800, 600));
    assert_eq!(u32::try_from(&results["persist_mode"]).unwrap(), 2);
    assert!(results.contains_key("restore_data"));

    let windows = ede_protocol_client(&w).windows().unwrap();
    let probe = (windows[0].x + 10, windows[0].y + 10);
    let pixel = consume(streams[0].0, probe);
    assert_eq!(pixel, [0xff, 0, 0], "BGRx bytes of the blue window");

    let () = zbus::block_on(w.bus.call_method(Some(BUS_NAME), session, Some("org.freedesktop.impl.portal.Session"), "Close", &())).unwrap().body().deserialize().unwrap();
}

/// A consumer stream for the screencast node (WirePlumber links it, as it
/// does for any app), the BGR bytes at `at` of the first frame. WirePlumber
/// takes a moment to load its link policy after it registers, so a stream
/// that sees no frame in a few seconds is connected again, a few times,
/// before the diagnostics come out.
fn consume(node: u32, at: (i32, i32)) -> [u8; 3] {
    // The node by its serial, never its name: an earlier world's producer
    // may still be registered under the same name, and ids are reused.
    let info = Command::new("pw-cli").args(["info", &node.to_string()]).env("PIPEWIRE_RUNTIME_DIR", pipewire()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let serial = info.lines().find_map(|l| l.trim_start_matches(|c: char| c == '*' || c.is_whitespace()).strip_prefix("object.serial = ")).map(|v| v.trim_matches('"').to_string()).unwrap_or_else(|| panic!("node {node} has no object.serial:\n{info}"));
    let mut last = String::new();
    for attempt in 1..=4 {
        match consume_once(&serial, at, Duration::from_secs(5)) {
            Ok(pixel) => return pixel,
            Err(e) => {
                eprintln!("attempt {attempt}: {e}");
                last = e;
            }
        }
    }
    for what in ["Node", "Link"] {
        let out = Command::new("pw-cli").args(["ls", what]).env("PIPEWIRE_RUNTIME_DIR", pipewire()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        eprintln!("--- pw-cli ls {what}\n{out}");
    }
    eprintln!("--- wireplumber\n{}", std::fs::read_to_string(pipewire().join("wireplumber.log")).unwrap_or_default());
    panic!("no frame arrived: {last}")
}

fn consume_once(serial: &str, at: (i32, i32), timeout: Duration) -> Result<[u8; 3], String> {
    let serial = serial.to_string();
    use pipewire as pw;
    use pw::{properties::properties, spa};
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None).unwrap();
        let context = pw::context::ContextRc::new(&mainloop, None).unwrap();
        let core = context.connect_rc(None).unwrap();
        // WirePlumber links us to the node we target, as it does for any app.
        let stream = pw::stream::StreamRc::new(core.clone(), "probe", properties! { *pw::keys::MEDIA_TYPE => "Video", *pw::keys::MEDIA_CATEGORY => "Capture", *pw::keys::MEDIA_ROLE => "Screen", "target.object" => serial.as_str() }).unwrap();
        let tx2 = tx.clone();
        let _listener = stream
            .add_local_listener_with_user_data(0u32)
            .state_changed(|_, _, _, _| {})
            .param_changed(|_, width, id, param| {
                if let (Some(param), true) = (param, id == spa::param::ParamType::Format.as_raw()) {
                    let mut info = spa::param::video::VideoInfoRaw::default();
                    if info.parse(param).is_ok() {
                        *width = info.size().width;
                    }
                }
            })
            .process(move |s, width| {
                let Some(mut buffer) = s.dequeue_buffer() else { return };
                let datas = buffer.datas_mut();
                let stride = datas[0].chunk().stride() as usize;
                let stride = if stride == 0 { *width as usize * 4 } else { stride };
                let Some(bytes) = datas[0].data() else { return };
                let i = at.1 as usize * stride + at.0 as usize * 4;
                if bytes.len() >= i + 3 && *width > 0 {
                    let _ = tx2.send([bytes[i], bytes[i + 1], bytes[i + 2]]);
                }
            })
            .register()
            .unwrap();
        let format = spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(spa::pod::object!(
                spa::utils::SpaTypes::ObjectParamFormat,
                spa::param::ParamType::EnumFormat,
                spa::pod::property!(spa::param::format::FormatProperties::MediaType, Id, spa::param::format::MediaType::Video),
                spa::pod::property!(spa::param::format::FormatProperties::MediaSubtype, Id, spa::param::format::MediaSubtype::Raw),
                spa::pod::property!(spa::param::format::FormatProperties::VideoFormat, Id, spa::param::video::VideoFormat::BGRx),
            )),
        )
        .unwrap()
        .0
        .into_inner();
        stream
            .connect(spa::utils::Direction::Input, None, pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS, &mut [spa::pod::Pod::from_bytes(&format).unwrap()])
            .unwrap();
        let ml = mainloop.clone();
        let timer = mainloop.loop_().add_timer(move |_| ml.quit());
        timer.update_timer(Some(timeout), None).into_result().unwrap();
        mainloop.run();
    });
    rx.recv_timeout(timeout).map_err(|e| e.to_string())
}
