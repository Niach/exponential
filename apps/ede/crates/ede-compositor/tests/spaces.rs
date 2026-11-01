//! #NEW-110 headless: agent spaces, their own output, seat, clipboard,
//! frame clock and processes, driven over ede-protocol.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{
    events, Capability, CaptureTarget, Client, Error, ErrorCode, InputEvent, Isolation, Request, SessionId, SpaceId, SpaceInfo, SpaceLimits,
};

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;
const KEY_A: u32 = 30;
const KEY_B: u32 = 48;
const BTN_LEFT: u32 = 0x110;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn code(e: Error) -> ErrorCode {
    match e {
        Error::Failed(f) => f.code,
        other => panic!("expected a failure, got {other}"),
    }
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn space_capture(ipc: &mut Client, space: &SpaceId) -> ede_protocol::Capture {
    ipc.capture(CaptureTarget::Space { space: space.clone(), window: None }).unwrap()
}

fn info(ipc: &mut Client, space: &SpaceId) -> SpaceInfo {
    ipc.spaces().unwrap().into_iter().find(|s| &s.id == space).expect("space listed")
}

#[test]
fn a_space_is_its_own_world() {
    let h = harness();
    let mut ipc = h.ipc();
    let space = ipc.create_space(640, 480, None).unwrap();
    assert_eq!((space.width, space.height, space.fps, space.session_id.as_str()), (640, 480, 15, "ede-shell"));
    assert_eq!(space.limits, SpaceLimits { cpu_percent: 100, memory_mb: 2048 });

    let mut desk = Session::connect(h.connect());
    let mut agent = Session::connect(h.connect_space(space.id.clone()));
    desk.roundtrip();
    agent.roundtrip();
    // Each sees only its own seat and output, and the agent none of the
    // desktop's privileged globals.
    assert_eq!(desk.state.seat_names, ["seat0"]);
    assert_eq!(desk.state.output_names, ["HEADLESS-1"]);
    assert_eq!(agent.state.seat_names, [format!("space:{}", space.id)]);
    assert_eq!(agent.state.output_names, [format!("space:{}", space.id)]);
    for privileged in ["zwlr_layer_shell_v1", "ext_session_lock_manager_v1", "zwp_virtual_keyboard_manager_v1", "zwp_input_method_manager_v2"] {
        assert!(!agent.state.globals.iter().any(|g| g == privileged), "{privileged} offered to a space");
        assert!(desk.state.globals.iter().any(|g| g == privileged) || privileged == "zwp_input_method_manager_v2", "{privileged} missing on the desktop");
    }

    let d = desk.toplevel("notes");
    desk.draw(&d, 300, 200, BLUE);
    let a = agent.toplevel("agent app");
    agent.draw(&a, 200, 100, RED);
    wait_for(&mut agent, |c| c.state.toplevels[0].configures > 0);

    // The desktop shows and lists only its own window; the space only its.
    let windows = ipc.windows().unwrap();
    assert_eq!(windows.iter().map(|w| w.title.as_deref()).collect::<Vec<_>>(), [Some("notes")]);
    let screen = h.capture(0);
    assert!(!screen.pixels.contains(&RED), "a space window reached the monitor");
    let listed = info(&mut ipc, &space.id);
    assert_eq!(listed.windows.len(), 1);
    let w = &listed.windows[0];
    assert_eq!((w.title.as_deref(), w.width, w.height, w.focused), (Some("agent app"), 200, 100, true));
    let frame = space_capture(&mut ipc, &space.id);
    assert_eq!((frame.info.width, frame.info.height), (640, 480));
    assert_eq!(frame.at(w.x + 5, w.y + 5), RED);
    assert!(!frame.pixels.chunks(4).any(|p| u32::from_le_bytes(p.try_into().unwrap()) == BLUE), "a desktop window reached the space");
    // The desktop's ops do not reach into the space.
    assert_eq!(code(ipc.call(Request::FocusWindow { window: w.id.clone() }).unwrap_err()), ErrorCode::NotFound);
    assert_eq!(code(ipc.capture(CaptureTarget::Window { window: w.id.clone() }).unwrap_err()), ErrorCode::NotFound);
}

#[test]
fn covered_windows_still_capture() {
    let h = harness();
    let mut ipc = h.ipc();
    let space = ipc.create_space(400, 300, None).unwrap();
    let mut agent = Session::connect(h.connect_space(space.id.clone()));
    let below = agent.toplevel("below");
    agent.draw(&below, 300, 200, GREEN);
    let above = agent.toplevel("above");
    agent.draw(&above, 300, 200, RED);
    let windows = info(&mut ipc, &space.id).windows;
    let (b, a) = (windows.iter().find(|w| w.title.as_deref() == Some("below")).unwrap(), windows.iter().find(|w| w.title.as_deref() == Some("above")).unwrap());
    // `above` covers most of `below` on the space's output…
    let x = a.x.max(b.x) + 5;
    let y = a.y.max(b.y) + 5;
    assert_eq!(space_capture(&mut ipc, &space.id).at(x, y), RED);
    // …but `below` still has its whole buffer.
    let only = ipc.capture(CaptureTarget::Space { space: space.id.clone(), window: Some(b.id.clone()) }).unwrap();
    assert_eq!((only.info.width, only.info.height), (300, 200));
    assert!(only.pixels.chunks(4).all(|p| u32::from_le_bytes(p.try_into().unwrap()) == GREEN));
}

#[test]
fn input_drives_the_space_seat_and_never_the_person() {
    let h = harness();
    let mut ipc = h.ipc();
    let mut desk = Session::connect(h.connect());
    desk.keyboard();
    let d = desk.toplevel("notes");
    desk.draw(&d, 300, 200, BLUE);
    wait_for(&mut desk, |c| c.state.keyboard_focus.is_some());

    let space = ipc.create_space(640, 480, None).unwrap();
    let mut agent = Session::connect(h.connect_space(space.id.clone()));
    agent.keyboard();
    agent.pointer();
    let first = agent.toplevel("first");
    agent.draw(&first, 200, 150, RED);
    let second = agent.toplevel("second");
    agent.draw(&second, 200, 150, GREEN);
    wait_for(&mut agent, |c| c.state.keyboard_focus.as_ref() == Some(&second.surface));

    ipc.space_input(space.id.clone(), InputEvent::Key { key: KEY_A, pressed: true }).unwrap();
    ipc.space_input(space.id.clone(), InputEvent::Key { key: KEY_A, pressed: false }).unwrap();
    wait_for(&mut agent, |c| c.state.keys == [KEY_A]);
    // The person's window kept its focus and heard nothing.
    desk.roundtrip();
    assert!(desk.state.keys.is_empty());
    assert_eq!(desk.state.keyboard_focus.as_ref(), Some(&d.surface));
    ipc.input(InputEvent::Key { key: KEY_B, pressed: true }).unwrap();
    ipc.input(InputEvent::Key { key: KEY_B, pressed: false }).unwrap();
    wait_for(&mut desk, |c| c.state.keys == [KEY_B]);
    agent.roundtrip();
    assert_eq!(agent.state.keys, [KEY_A]);

    // A click on the uncovered part of `first` focuses it, in the space only.
    let windows = info(&mut ipc, &space.id).windows;
    let f = windows.iter().find(|w| w.title.as_deref() == Some("first")).unwrap();
    let s = windows.iter().find(|w| w.title.as_deref() == Some("second")).unwrap();
    let (x, y) = if s.x > f.x { (f.x + 5, f.y + 5) } else { (f.x + f.width - 5, f.y + f.height - 5) };
    ipc.space_input(space.id.clone(), InputEvent::PointerMotion { x: x as f64, y: y as f64 }).unwrap();
    ipc.space_input(space.id.clone(), InputEvent::PointerButton { button: BTN_LEFT, pressed: true }).unwrap();
    ipc.space_input(space.id.clone(), InputEvent::PointerButton { button: BTN_LEFT, pressed: false }).unwrap();
    wait_for(&mut agent, |c| c.state.keyboard_focus.as_ref() == Some(&first.surface) && c.state.pointer_focus.as_ref() == Some(&first.surface));
    assert!(info(&mut ipc, &space.id).windows.iter().find(|w| w.id == f.id).unwrap().focused);
    desk.roundtrip();
    assert_eq!(desk.state.keyboard_focus.as_ref(), Some(&d.surface));

    // A protected window in a space is black and never driven.
    let vault = agent.toplevel("vault");
    vault.toplevel.set_app_id("org.keepassxc.KeePassXC".into());
    agent.draw(&vault, 200, 150, RED);
    wait_for(&mut agent, |c| c.state.keyboard_focus.as_ref() == Some(&vault.surface));
    assert_eq!(code(ipc.space_input(space.id.clone(), InputEvent::Key { key: KEY_A, pressed: true }).unwrap_err()), ErrorCode::Forbidden);
    let v = info(&mut ipc, &space.id).windows.into_iter().find(|w| w.protected).unwrap();
    assert_eq!(v.title, None);
    assert_eq!(space_capture(&mut ipc, &space.id).at(v.x + 5, v.y + 5), BLACK);
    let alone = CaptureTarget::Space { space: space.id.clone(), window: Some(v.id.clone()) };
    assert_eq!(code(ipc.capture(alone).unwrap_err()), ErrorCode::Forbidden);
    let over = InputEvent::PointerMotion { x: (v.x + 5) as f64, y: (v.y + 5) as f64 };
    assert_eq!(code(ipc.space_input(space.id.clone(), over).unwrap_err()), ErrorCode::Forbidden);
}

#[test]
fn a_space_has_its_own_clipboard() {
    let h = harness();
    let mut ipc = h.ipc();
    let space = ipc.create_space(400, 300, None).unwrap();
    let mut agent = Session::connect(h.connect_space(space.id.clone()));
    agent.keyboard();
    let t = agent.toplevel("copier");
    agent.draw(&t, 200, 100, RED);
    wait_for(&mut agent, |c| c.state.enter_serial.is_some());
    let _source = agent.set_selection();
    agent.roundtrip();

    let id = space.id.clone();
    let (in_space, on_desktop) = h.with(move |ede| {
        use smithay::wayland::selection::data_device::request_data_device_client_selection;
        let probe = |seat: &smithay::input::Seat<ede_compositor::Ede>| {
            let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
            request_data_device_client_selection(seat, "text/plain".into(), a.into()).is_ok()
        };
        (probe(&ede.spaces.get(&id).unwrap().seat), probe(&ede.seat))
    });
    assert!(in_space, "the space seat holds the agent's selection");
    assert!(!on_desktop, "the person's clipboard is untouched");
}

#[test]
fn a_space_offers_frames_at_its_own_capped_rate() {
    let h = harness();
    let mut ipc = h.ipc();
    let space = ipc.create_space(400, 300, Some(5)).unwrap();
    assert_eq!(space.fps, 5);
    assert_eq!(ipc.create_space(400, 300, Some(240)).unwrap().fps, 30, "capped");
    let mut agent = Session::connect(h.connect_space(space.id.clone()));
    let t = agent.toplevel("slow");
    agent.draw(&t, 100, 100, RED);
    let mut desk = Session::connect(h.connect());
    let d = desk.toplevel("fast");
    desk.draw(&d, 100, 100, BLUE);

    let agent_frames = agent.count_frames(&t.surface, Duration::from_secs(1));
    let desk_frames = desk.count_frames(&d.surface, Duration::from_secs(1));
    assert!((2..=8).contains(&agent_frames), "{agent_frames} frames at 5 fps");
    assert!(desk_frames > 20, "{desk_frames} frames on the 60 Hz desktop");
}

#[test]
fn grants_scope_spaces_to_their_session() {
    let h = harness();
    let mut root = h.ipc();
    let far = 4_102_444_800_000; // 2100
    let g1 = root.mint(SessionId::new("agent-1").unwrap(), vec![Capability::Space], Some(far)).unwrap();
    let g2 = root.mint(SessionId::new("agent-2").unwrap(), vec![Capability::Space], Some(far)).unwrap();
    let viewer = root.mint(SessionId::new("viewer").unwrap(), vec![Capability::Capture], None).unwrap();
    let mut a1 = h.ipc_with(g1.token.clone()).unwrap();
    let mut a2 = h.ipc_with(g2.token.clone()).unwrap();
    let mut v = h.ipc_with(viewer.token).unwrap();

    let s1 = a1.create_space(320, 240, None).unwrap();
    assert_eq!(s1.session_id.as_str(), "agent-1");
    assert_eq!(a1.spaces().unwrap().len(), 1);
    // Another session's space does not exist for it.
    assert!(a2.spaces().unwrap().is_empty());
    assert_eq!(code(a2.destroy_space(s1.id.clone()).unwrap_err()), ErrorCode::NotFound);
    assert_eq!(code(a2.capture(CaptureTarget::Space { space: s1.id.clone(), window: None }).unwrap_err()), ErrorCode::NotFound);
    assert_eq!(code(a2.space_input(s1.id.clone(), InputEvent::Key { key: KEY_A, pressed: true }).unwrap_err()), ErrorCode::NotFound);
    assert_eq!(code(a2.launch(s1.id.clone(), vec!["true".into()]).unwrap_err()), ErrorCode::NotFound);
    // `space` is not `capture`, and `capture` is not `space`.
    assert_eq!(code(a1.capture(CaptureTarget::Output { name: "HEADLESS-1".into() }).unwrap_err()), ErrorCode::Forbidden);
    assert_eq!(code(v.capture(CaptureTarget::Space { space: s1.id.clone(), window: None }).unwrap_err()), ErrorCode::Forbidden);
    assert_eq!(code(v.create_space(320, 240, None).unwrap_err()), ErrorCode::Forbidden);
    // Only `system` creates for someone else; the shell sees every space.
    let other = Request::CreateSpace { session_id: Some(SessionId::new("agent-2").unwrap()), width: 320, height: 240, fps: None, limits: None };
    assert_eq!(code(a1.call(other.clone()).unwrap_err()), ErrorCode::Forbidden);
    let s2: SpaceInfo = serde_json::from_value(root.call(other).unwrap()).unwrap();
    assert_eq!(s2.session_id.as_str(), "agent-2");
    assert_eq!(a2.spaces().unwrap().len(), 1);
    assert_eq!(root.spaces().unwrap().len(), 2);
    assert_eq!(code(a1.create_space(10, 10, None).unwrap_err()), ErrorCode::Invalid);

    // Revoking agent-1's only grant closes its space; ending agent-2's
    // session closes its space and its grant.
    root.subscribe().unwrap();
    root.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    root.call(Request::RevokeGrant { token: g1.token }).unwrap();
    let gone = root.next_event().unwrap();
    assert_eq!((gone.kind.as_str(), gone.data["space"].as_str()), (events::SPACE_DESTROYED, Some(s1.id.as_str())));
    root.call(Request::EndSession { session_id: SessionId::new("agent-2").unwrap() }).unwrap();
    assert!(root.spaces().unwrap().is_empty());
    assert_eq!(code(a2.spaces().unwrap_err()), ErrorCode::Unauthorized);
    assert_eq!(code(root.call(Request::EndSession { session_id: SessionId::new("ede-shell").unwrap() }).unwrap_err()), ErrorCode::Forbidden);
}

#[test]
fn closing_a_space_disconnects_its_clients() {
    let h = harness();
    let mut ipc = h.ipc();
    let space = ipc.create_space(320, 240, None).unwrap();
    let socket = std::path::PathBuf::from(&space.wayland_display);
    assert!(socket.is_absolute() && socket.exists());
    // Through its real socket, as a launched app connects.
    let mut agent = Session::connect(std::os::unix::net::UnixStream::connect(&socket).unwrap());
    assert_eq!(agent.state.seat_names, [format!("space:{}", space.id)]);
    let t = agent.toplevel("doomed");
    agent.draw(&t, 100, 100, RED);
    ipc.destroy_space(space.id.clone()).unwrap();
    assert!(!socket.exists(), "the socket is unlinked");
    let deadline = Instant::now() + Duration::from_secs(5);
    while agent.queue.roundtrip(&mut agent.state).is_ok() {
        assert!(Instant::now() < deadline, "still connected");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(code(ipc.destroy_space(space.id).unwrap_err()), ErrorCode::NotFound);
    assert!(ipc.windows().unwrap().is_empty());
}

#[test]
fn launch_runs_in_the_space_and_dies_with_it() {
    launch_and_close(true);
}

#[test]
fn without_a_cgroup_launches_get_a_process_group() {
    launch_and_close(false);
}

fn launch_and_close(use_cgroup: bool) {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.spaces.cgroup = use_cgroup;
    let h = Harness::start(config, &[(800, 600)]);
    let mut ipc = h.ipc();
    let space = ipc.create_space(320, 240, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("env");
    let script = format!("echo \"$WAYLAND_DISPLAY|$DISPLAY|$EDE_SPACE\" > {0}.tmp; cat /proc/self/cgroup >> {0}.tmp; mv {0}.tmp {0}; exec sleep 60", out.display());
    let pid = ipc.launch(space.id.clone(), vec!["sh".into(), "-c".into(), script]).unwrap().pid;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !out.exists() {
        assert!(Instant::now() < deadline, "the launched process never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    let text = std::fs::read_to_string(&out).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next().unwrap(), format!("{}||{}", space.wayland_display, space.id));
    let cgroup = lines.next().unwrap();
    match space.isolation {
        Isolation::Systemd => assert!(cgroup.contains(&format!("_{}.slice/", space.id)) && cgroup.contains("ede-space.slice"), "{cgroup}"),
        Isolation::ProcessGroup => assert!(!cgroup.contains("ede-space"), "{cgroup}"),
    }
    if !use_cgroup {
        assert_eq!(space.isolation, Isolation::ProcessGroup);
    }
    assert_eq!(code(ipc.launch(space.id.clone(), vec![]).unwrap_err()), ErrorCode::Invalid);

    ipc.destroy_space(space.id).unwrap();
    let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    // Gone, or a zombie until the reaper waits for it.
    while proc.exists() && !std::fs::read_to_string(proc.join("stat")).is_ok_and(|s| s.contains(") Z")) {
        assert!(Instant::now() < deadline, "the launched process outlived its space");
        std::thread::sleep(Duration::from_millis(50));
    }
}
