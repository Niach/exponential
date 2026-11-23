//! #EDE-172 (P4): the supervisor the compositor starts (`[session] helpers =
//! "compositor"`) on the channel it inherited: every shell it starts gets a
//! channel of its own holding root at fd 3 and a Wayland connection the
//! compositor made at fd 4. A shell that dies and is started again holds
//! root anew, and what the dead one held is let go. The real supervisor
//! runs as this test binary started again by a headless compositor; its
//! shell is this test binary again (`EDE172_UI_ROLE`).
#![cfg(target_os = "linux")]

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use ede_compositor::{
    backend::headless::Harness,
    helpers::{Channel, Kind, Spec, Wayland},
    Config,
};
use ede_protocol::{Client, InputEvent};
use ede_ui::{supervise::Supervisor, Runtime};

const ROLE: &str = "EDE172_UI_ROLE";
const OUT: &str = "EDE172_UI_OUT";
const UI: &str = "EDE172_UI_TREE";

fn role_argv(name: &str) -> Vec<String> {
    vec![std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(), name.into(), "--ignored".into(), "--nocapture".into(), "--test-threads=1".into()]
}

fn said(out: &Path) -> String {
    std::fs::read_to_string(out).unwrap_or_default()
}

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn say(line: &str) {
    let out = std::env::var_os(OUT).expect("where to report");
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(out).unwrap();
    file.write_all(format!("{line}\n").as_bytes()).unwrap();
}

#[test]
fn a_shell_started_again_holds_root_and_the_dead_ones_holds_are_released() {
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let tmp = tempfile::tempdir().unwrap();
    let factory = tmp.path().join("factory");
    std::fs::create_dir_all(&factory).unwrap();
    let panel = serde_json::json!({
        "surface": {"anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
        "root": {"id": "panel", "component": "Box", "children": [{"id": "clock", "component": "Text", "props": {"text": "factory"}}]}
    });
    std::fs::write(factory.join("panel.express"), ede_ui::decompile_document("panel", &panel).unwrap()).unwrap();
    let out = tmp.path().join("report");
    let spec = Spec {
        name: "test-shell".into(),
        argv: role_argv("role_supervisor"),
        channel: Some(Channel::Root),
        wayland: Some(Wayland::Asked(Kind::Shell)),
        env: vec![(ROLE.into(), "supervisor".into()), (OUT.into(), out.to_string_lossy().into_owned()), (UI.into(), tmp.path().to_string_lossy().into_owned())],
    };
    h.with(move |ede| {
        ede.helpers.base_env = Some(std::env::vars_os().collect());
        ede.start_helpers_with(vec![spec]);
    });
    until("the first shell never held the person's input", || said(&out).contains("held"));
    assert!(h.with(|ede| ede.person_input_held()), "the first shell's hold");
    // The first shell dies now.
    std::fs::write(tmp.path().join("go"), "").unwrap();
    until("the shell was not started again", || said(&out).contains("again"));
    let report = said(&out);
    assert!(report.contains("first exponential-shell 6 wayland-socket\n"), "{report}");
    assert!(report.contains("again exponential-shell 6 wayland-socket\n"), "the second shell holds root and a Wayland connection: {report}");
    until("the dead shell's hold or key outlived it", || h.with(|ede| !ede.person_input_held() && ede.desk.seat.get_keyboard().unwrap().pressed_keys().is_empty()));
    assert!(h.with(|ede| ede.helpers.pid("test-shell")).is_some(), "the supervisor itself goes on");
}

/// The supervisor, as `exponential-ui supervise` runs it on its channel.
#[test]
#[ignore = "#EDE-172: a role of the supervisor's channel test"]
fn role_supervisor() {
    if std::env::var(ROLE).as_deref() != Ok("supervisor") || std::env::var_os("EDE_UI_DIR").is_some() {
        return;
    }
    let control = Client::inherited().expect("a channel at fd 3").expect("hello over it");
    let tree = PathBuf::from(std::env::var_os(UI).unwrap());
    let (ui, _) = Runtime::open_with_factory(&tree.join("ui"), &tree.join("factory")).unwrap();
    let mut supervisor = Supervisor::new(ui, role_argv("role_shell"), |_| {}).unwrap().with_control(control);
    supervisor.run(&AtomicBool::new(false)).unwrap();
}

/// The shell stand-in: the first one holds the person's input, presses a
/// key and dies; the next says what it holds.
#[test]
#[ignore = "#EDE-172: a role of the supervisor's channel test"]
fn role_shell() {
    if std::env::var(ROLE).as_deref() != Ok("supervisor") || std::env::var_os("EDE_UI_DIR").is_none() {
        return;
    }
    let mut channel = Client::inherited().expect("a channel at fd 3").expect("hello over it");
    let wayland = std::env::var("WAYLAND_SOCKET").as_deref() == Ok("4") && std::fs::metadata("/proc/self/fd/4").is_ok_and(|m| std::os::unix::fs::FileTypeExt::is_socket(&m.file_type()));
    let holds = format!("{} {} {}", channel.hello.session_id.as_str(), channel.hello.capabilities.len(), if wayland { "wayland-socket" } else { "no-wayland" });
    let out = PathBuf::from(std::env::var_os(OUT).unwrap());
    if said(&out).contains("held") {
        say(&format!("again {holds}"));
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    say(&format!("first {holds}"));
    channel.hold_input().unwrap();
    channel.input(InputEvent::Key { key: 30, pressed: true }).unwrap();
    say("held");
    let go = PathBuf::from(std::env::var_os(UI).unwrap()).join("go");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !go.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::process::exit(1);
}
