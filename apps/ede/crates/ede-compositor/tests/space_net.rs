//! #EDE-162 headless: the safety net. In a space that draws on the GPU, an
//! app that ends at start without having drawn anything (vkcube's missing
//! present queue, ANV's swapchain abort, an X11 app on the space's
//! Xwayland, whose frames never come from the launch itself) latches the
//! space back to software, and the agent reads it with its next capture. An app that drew, a clean exit, a
//! launch's own stage failing (127), a space in software: nothing latches.

mod common;

use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::{space::start_with_spaces, Session};
use ede_compositor::{backend::headless::Harness, desk::DeskId, spaces::frames::net::START};
use ede_protocol::{Capability, CaptureTarget, Client, SessionId, SpaceInfo};

const MODE: &str = "EDE_TEST_NET_MODE";

/// Where the launched app says how far it got.
#[cfg(feature = "xwayland")]
const MARK: &str = "EDE_TEST_NET_MARK";

/// The launched app: this test binary again. `draw-then-fail`: it opens a
/// window in its space, draws, then ends with exit 1 (an app that drew and
/// then failed for its own reasons). `x11-then-abort`: an X11 app that fails
/// as an X11 Vulkan app does under ANV: its window maps on the space's own
/// Xwayland, then it aborts (no core written: the abort is seen at once).
#[test]
fn launched_app() {
    match std::env::var(MODE).as_deref() {
        Ok("draw-then-fail") => {
            let mut c = Session::connect(UnixStream::connect(std::env::var("WAYLAND_DISPLAY").unwrap()).unwrap());
            let t = c.toplevel("drew");
            c.draw(&t, 300, 200, 0xff00_00ff);
            std::thread::sleep(Duration::from_millis(300));
            std::process::exit(1);
        }
        #[cfg(feature = "xwayland")]
        Ok("x11-then-abort") => {
            use x11rb::{
                connection::Connection,
                protocol::{
                    xproto::{ConnectionExt as _, CreateWindowAux, EventMask, WindowClass},
                    Event,
                },
            };
            let (conn, screen) = x11rb::connect(Some(&std::env::var("DISPLAY").expect("the space's DISPLAY"))).expect("the space's Xwayland");
            let root = conn.setup().roots[screen].root;
            let win = conn.generate_id().unwrap();
            let aux = CreateWindowAux::new().background_pixel(0x00ff_0000).event_mask(EventMask::STRUCTURE_NOTIFY);
            conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 300, 200, 0, WindowClass::INPUT_OUTPUT, 0, &aux).unwrap();
            conn.map_window(win).unwrap();
            conn.flush().unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Ok(Some(Event::MapNotify(_))) = conn.poll_for_event() {
                    std::fs::write(std::env::var(MARK).unwrap(), "mapped").unwrap();
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(300));
            let _ = smithay::reexports::rustix::process::set_dumpable_behavior(smithay::reexports::rustix::process::DumpableBehavior::NotDumpable);
            std::process::abort();
        }
        _ => {}
    }
}

fn agent(h: &Harness, session: &str) -> (Client, SpaceInfo, DeskId) {
    let mut root = h.ipc();
    let grant = root.mint(SessionId::new(session).unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let id = space.id.clone();
    let desk = h.with(move |state| state.spaces.iter().find(|s| s.id == id).unwrap().desk.id);
    (agent, space, desk)
}

fn sh(script: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), script.into()]
}

fn latched(h: &Harness, desk: DeskId) -> Option<String> {
    h.with(move |state| state.space_latched(desk).map(str::to_string))
}

/// Waits until `pid` has ended and the compositor reaped it (its launches'
/// 1 s tick, which tells the net how it ended).
fn ended(pid: u32) {
    let deadline = Instant::now() + START;
    while std::path::Path::new(&format!("/proc/{pid}")).exists() {
        assert!(Instant::now() < deadline, "the launch was never reaped");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn an_app_that_aborts_at_start_without_drawing_latches_its_gpu_space_and_the_agent_is_told_once() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, space, desk) = agent(&h, "run-1");
    h.with(move |state| state.pretend_space_on_gpu(desk));
    assert!(h.with(move |state| state.space_gpu_allowed(desk)));

    // As ANV's vkcube: an assertion, SIGABRT, no window.
    let pid = agent.launch(sh("kill -ABRT $$"), Default::default(), None).unwrap();
    ended(pid);
    let why = latched(&h, desk).expect("the space latched");
    // `sh` killed by SIGABRT: seen dumping its core (where a core is
    // written), else reaped; or its exit status where dbus-run-session
    // wraps it.
    let crashed = why.starts_with("sh crashed at start on the GPU");
    let ended = why.starts_with("sh ended at start on the GPU (") && (why.contains("signal 6") || why.contains("exit "));
    assert!(crashed || ended, "{why}");
    assert!(!h.with(move |state| state.space_gpu_allowed(desk)), "it draws in software now");

    let target = || CaptureTarget::Space { space: space.id.clone() };
    let first = agent.capture(target()).unwrap();
    assert_eq!(first.info.notice, Some(format!("{why}; this space now draws in software: start it again.")));
    let unchanged = h.ipc().capture(target()).unwrap();
    assert_eq!(unchanged.info.notice, None, "a person-scope capture (the live view) never takes it");
    let again = agent.capture_since(target(), None).unwrap().unwrap();
    assert_eq!(again.info.notice, None, "said once");

    // Its next launch sees lavapipe alone (F3's switch, back on).
    let lavapipe = ede_compositor::spaces::render::lavapipe_icds(std::path::Path::new("/"), std::env::var_os("XDG_DATA_DIRS").as_deref());
    if !lavapipe.is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("env");
        agent.launch(sh(&format!("env > {0}.tmp; mv {0}.tmp {0}", file.display())), Default::default(), None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !file.exists() {
            assert!(Instant::now() < deadline, "the launch never wrote its environment");
            std::thread::sleep(Duration::from_millis(20));
        }
        let env = std::fs::read_to_string(&file).unwrap();
        let driver_files = env.lines().find_map(|l| l.strip_prefix("VK_ICD_FILENAMES=")).expect("lavapipe is pointed at");
        assert!(driver_files.split(':').all(|f| f.contains("lvp_icd")), "{driver_files}");
    }
}

/// A GPU space's launches see exactly its GPU's Vulkan driver and lavapipe
/// (F3's switch off at its launch); here the GPU is pretended to be Intel's.
#[test]
fn a_gpu_spaces_launch_sees_its_gpus_vulkan_driver_and_lavapipe() {
    use ede_compositor::spaces::render::{icds_named, lavapipe_icds, vulkan_environment};
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, _space, desk) = agent(&h, "run-1");
    h.with(move |state| {
        state.pretend_space_on_gpu(desk);
        state.pretend_space_gpu_driver("i915");
    });
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("env");
    agent.launch(sh(&format!("env > {0}.tmp; mv {0}.tmp {0}", file.display())), Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !file.exists() {
        assert!(Instant::now() < deadline, "the launch never wrote its environment");
        std::thread::sleep(Duration::from_millis(20));
    }
    let env = std::fs::read_to_string(&file).unwrap();
    let seen = env.lines().find_map(|l| l.strip_prefix("VK_ICD_FILENAMES="));
    let data = std::env::var_os("XDG_DATA_DIRS");
    let intel = icds_named(std::path::Path::new("/"), data.as_deref(), "intel_icd");
    let lavapipe = lavapipe_icds(std::path::Path::new("/"), data.as_deref());
    let expected = vulkan_environment(false, Some(intel.as_slice()), &lavapipe);
    assert_eq!(seen, expected.first().map(|(_, files)| files.to_str().unwrap()), "{env}");
    if let Some(seen) = seen.filter(|_| !intel.is_empty()) {
        assert!(seen.split(':').any(|f| f.contains("intel_icd")), "a GPU space is not forced onto lavapipe: {seen}");
        assert!(seen.split(':').all(|f| f.contains("intel_icd") || f.contains("lvp_icd")), "nothing but its GPU's driver and lavapipe: {seen}");
    }
}

#[test]
fn a_clean_exit_a_stage_failure_and_an_app_that_drew_latch_nothing() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, _space, desk) = agent(&h, "run-1");
    h.with(move |state| state.pretend_space_on_gpu(desk));
    for script in ["exit 0", "exit 127"] {
        let pid = agent.launch(sh(script), Default::default(), None).unwrap();
        ended(pid);
        assert_eq!(latched(&h, desk), None, "{script}");
    }
    // Drew a window, then failed: its GPU path worked.
    let argv = vec![std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(), "launched_app".into(), "--nocapture".into()];
    let pid = agent.launch(argv, [(MODE.to_string(), "draw-then-fail".to_string())].into(), None).unwrap();
    ended(pid);
    assert_eq!(latched(&h, desk), None, "an app that drew is not a failed GPU path");
}

/// The X11 case (F3's Xwayland per space): an X11 app's frames reach the
/// space through the space's Xwayland, never as a client of the launch, so
/// an X11 app that maps its window and then aborts counts as having drawn
/// nothing: its GPU space latches.
#[cfg(feature = "xwayland")]
#[test]
fn an_x11_app_that_fails_at_start_latches_its_gpu_space() {
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, _space, desk) = agent(&h, "run-1");
    h.with(move |state| {
        state.spaces.x11 = true;
        state.pretend_space_on_gpu(desk);
    });
    let mark = dir.path().join("x11-app");
    let argv = vec![std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(), "launched_app".into(), "--nocapture".into()];
    let env = [(MODE.to_string(), "x11-then-abort".to_string()), (MARK.to_string(), mark.to_string_lossy().into_owned())].into();
    agent.launch(argv, env, None).unwrap();
    let deadline = Instant::now() + START + Duration::from_secs(5);
    let why = loop {
        if let Some(why) = latched(&h, desk) {
            break why;
        }
        assert!(Instant::now() < deadline, "an X11 app that failed at start never latched its space");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(mark.exists(), "the X11 app never got its window mapped on the space's Xwayland");
    assert!(why.contains("at start on the GPU"), "{why}");
    assert!(!h.with(move |state| state.space_gpu_allowed(desk)), "it draws in software now");
}

#[test]
fn a_space_in_software_watches_nothing() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (mut agent, _space, desk) = agent(&h, "run-1");
    assert!(!h.with(move |state| state.space_gpu_allowed(desk)));
    let pid = agent.launch(sh("exit 3"), Default::default(), None).unwrap();
    ended(pid);
    assert_eq!(latched(&h, desk), None);
}
