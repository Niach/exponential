//! #EDE-125 headless: agent spaces. A space is a second desk with its own
//! seat, output and socket; nothing of it shows on the person's side.

mod common;

use std::os::unix::{fs::PermissionsExt, net::UnixStream};

use common::{
    space::{person_key, spaces_settled, start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId, Config};
use ede_protocol::{ErrorCode, SessionId};

/// Every global a client of the person's desk is told about, in order.
const PERSONS_GLOBALS: [&str; 19] = [
    "wl_compositor",
    "wl_subcompositor",
    "xdg_wm_base",
    "zwlr_layer_shell_v1",
    "wl_shm",
    "zxdg_output_manager_v1",
    "wl_data_device_manager",
    "wl_seat",
    "ext_idle_notifier_v1",
    "ext_session_lock_manager_v1",
    "zwp_idle_inhibit_manager_v1",
    "zwp_input_method_manager_v2",
    "zwp_text_input_manager_v3",
    "zwp_virtual_keyboard_manager_v1",
    "wp_fractional_scale_manager_v1",
    "wp_viewporter",
    "zxdg_decoration_manager_v1",
    "xdg_activation_v1",
    "wl_output",
];

/// Every global a client of a space is told about, in order: one seat,
/// one output and what works on any desk.
const SPACE_GLOBALS: [&str; 14] = [
    "wl_compositor",
    "wl_subcompositor",
    "xdg_wm_base",
    "wl_shm",
    "zxdg_output_manager_v1",
    "wl_data_device_manager",
    "ext_idle_notifier_v1",
    "zwp_idle_inhibit_manager_v1",
    "zwp_text_input_manager_v3",
    "wp_fractional_scale_manager_v1",
    "wp_viewporter",
    "xdg_activation_v1",
    "wl_seat",
    "wl_output",
];

const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;

fn owner(name: &str) -> SessionId {
    SessionId::new(name).unwrap()
}

#[test]
fn a_space_has_its_own_seat_output_and_socket() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(owner("run-1"), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    assert_ne!(desk, DeskId::PERSON);
    assert_eq!(socket, dir.path().join("wayland-test.spaces/space-1/wayland-0"));
    for own in [socket.parent().unwrap(), socket.parent().unwrap().parent().unwrap()] {
        assert_eq!(std::fs::metadata(own).unwrap().permissions().mode() & 0o777, 0o700, "{}", own.display());
    }

    // B connects to the space's socket and sees exactly the space's seat
    // and output beside the globals of any desk.
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.roundtrip();
    assert_eq!(b.state.globals, SPACE_GLOBALS);
    assert_eq!(b.state.seat_names, ["seat-space-1"]);
    assert_eq!((b.state.output_names.as_slice(), b.state.output_modes.as_slice()), (["space-1".to_string()].as_slice(), [(640, 480)].as_slice()));

    h.with(move |state| {
        let space = state.spaces.get(desk).unwrap();
        assert_eq!((space.id.as_str(), space.owner.as_str(), space.output.name().as_str()), ("space-1", "run-1", "space-1"));
        let its = state.desk(desk);
        assert!(its.id == desk && its.seat != state.desk.seat, "a desk and a seat of its own");
        assert!(its.seat.get_keyboard().is_some() && its.seat.get_pointer().is_some());
        assert_eq!(its.space.outputs().map(|o| o.name()).collect::<Vec<_>>(), ["space-1"]);
        // The person's layout and lists know nothing of it.
        assert_eq!(state.desk.space.outputs().map(|o| o.name()).collect::<Vec<_>>(), ["HEADLESS-1"]);
        assert_eq!(state.output_infos().iter().map(|o| o.name.clone()).collect::<Vec<_>>(), ["HEADLESS-1"]);
    });
}

#[test]
fn a_space_outside_320x240_to_1920x1200_is_refused() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    for (w, h_) in [(319, 240), (320, 239), (1921, 1200), (1920, 1201), (0, 0), (-640, 480)] {
        let refused = h.with(move |state| state.create_space(owner("run-1"), w, h_).err().map(|e| e.code));
        assert_eq!(refused, Some(ErrorCode::Invalid), "{w}x{h_}");
    }
    assert!(!dir.path().join("wayland-test.spaces").exists(), "a refused space leaves no directory");
    for (w, h_) in [(320, 240), (1920, 1200)] {
        h.with(move |state| state.create_space(owner("run-1"), w, h_)).expect("the smallest and the largest");
    }
    assert_eq!(h.with(|state| state.spaces.iter().count()), 2);

    // Without a directory for them no space can be made.
    let plain = Harness::start(Config::default(), &[(800, 600)]);
    assert_eq!(plain.with(|state| state.create_space(owner("run-1"), 640, 480).err().map(|e| e.code)), Some(ErrorCode::Unavailable));
}

#[test]
fn no_space_no_difference() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut before = Session::connect(h.connect());
    assert_eq!(before.state.globals, PERSONS_GLOBALS);

    h.with(|state| state.create_space(owner("run-1"), 640, 480)).expect("a space");
    // The person's client hears of nothing new, and one that comes while
    // the space lives sees what a client saw before.
    before.roundtrip();
    assert_eq!(before.state.globals, PERSONS_GLOBALS);
    let with = Session::connect(h.connect());
    assert_eq!(with.state.globals, PERSONS_GLOBALS);
    assert_eq!((with.state.seat_names.as_slice(), with.state.output_names.as_slice()), (["seat0".to_string()].as_slice(), ["HEADLESS-1".to_string()].as_slice()));
}

/// A space that closes takes its clients, its socket, its directory, its
/// globals and its seat along; the person's client hears of two removals
/// and of nothing else.
#[test]
fn a_closed_space_leaves_nothing() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let mut a = Session::connect(h.connect());
    let window = a.toplevel("a");
    a.draw(&window, 200, 150, BLUE);
    let _keyboard = a.keyboard();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned();
        state.focus(DeskId::PERSON, window);
    });
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());

    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(owner("run-1"), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));

    assert!(h.with(move |state| state.close_space(desk)));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while b.queue.roundtrip(&mut b.state).is_ok() {
        assert!(std::time::Instant::now() < deadline, "the space's client is still connected");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!socket.exists() && !socket.parent().unwrap().exists(), "its socket and its directory are gone");
    assert!(UnixStream::connect(&socket).is_err());
    h.with(move |state| {
        assert!(state.try_desk(desk).is_none() && state.spaces.iter().count() == 0);
        assert!(!state.close_space(desk), "it closes once");
        assert!(!state.close_space(DeskId::PERSON), "the person's desk never closes");
    });
    spaces_settled(&h);

    // The person's client heard two removals, of names it was never told.
    a.roundtrip();
    assert_eq!(a.state.removed.len(), 2);
    assert!(a.state.removed.iter().all(|name| !a.state.global_names.contains(name)));
    assert_eq!(a.state.globals, PERSONS_GLOBALS);
    // It still has the keyboard, and one that comes now sees what one saw before.
    person_key(&h, KEY_A);
    wait_for(&mut a, |a| a.state.keys == [KEY_A]);
    assert_eq!(Session::connect(h.connect()).state.globals, PERSONS_GLOBALS);

    // A number is never given out twice; the compositor's end removes what is left.
    let next = h.with(|state| state.create_space(owner("run-1"), 640, 480).map(|desk| state.spaces.get(desk).unwrap().socket())).expect("a space");
    assert_eq!(next, dir.path().join("wayland-test.spaces/space-2/wayland-0"));
    drop(h);
    assert!(!dir.path().join("wayland-test.spaces").exists(), "nothing stays on disk after the compositor");
}

#[test]
fn what_an_earlier_compositor_left_is_swept() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("wayland-test.spaces");
    std::fs::create_dir_all(stale.join("space-7")).unwrap();
    std::fs::write(stale.join("space-7/wayland-0"), b"").unwrap();
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let spaces = stale.clone();
    h.with(move |state| state.spaces.open(spaces));
    assert!(!stale.exists());
}
