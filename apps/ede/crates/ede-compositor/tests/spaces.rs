//! #EDE-125 headless: agent spaces. A space is a second desk with its own
//! seat, output and socket; nothing of it shows on the person's side.

mod common;

use std::os::unix::{fs::PermissionsExt, net::UnixStream};

use common::{
    space::{assert_person_untouched, events_until_mark, person_key, person_snapshot, spaces_settled, start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId, Config};
use ede_protocol::{Capability, Error, ErrorCode, Request, SessionId, SpaceId};
use smithay::backend::input::{ButtonState, KeyState};

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

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;
const KEY_B: u32 = 48;
const KEY_G: u32 = 34;
const KEY_LEFTSHIFT: u32 = 42;
const BTN_LEFT: u32 = 0x110;

/// A client of the person with one focused window, its keyboard bound.
fn person_with_a_window(h: &Harness) -> Session {
    let mut a = Session::connect(h.connect());
    let window = a.toplevel("a");
    a.draw(&window, 200, 150, BLUE);
    a.keyboard();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned();
        state.focus(DeskId::PERSON, window);
    });
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    a
}

/// A space of 640x480 for `run-1` and a client connected to it.
fn space_with_a_client(h: &Harness) -> (DeskId, std::path::PathBuf, Session) {
    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(owner("run-1"), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    (desk, socket, b)
}

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
    let mut a = person_with_a_window(&h);
    let before = person_snapshot(&h);
    let (desk, socket, mut b) = space_with_a_client(&h);
    // It closes with a window that has its keyboard.
    b.keyboard();
    let window = b.toplevel("b");
    b.draw_configured(&window, 64, 64, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    // And with a key and a button down, in the middle of a drag.
    b.pointer();
    let device = b.data_device();
    h.with(move |state| {
        state.drive_pointer_motion(desk, (10.0, 10.0).into(), 0).unwrap();
        state.drive_key(desk, (KEY_G + 8).into(), KeyState::Pressed, 0, false).unwrap();
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Pressed, 0).unwrap();
    });
    wait_for(&mut b, |b| b.state.press_serial.is_some() && b.state.keys == [KEY_G]);
    b.start_drag(&device, &window.surface);

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
    assert_person_untouched(&h, &before);

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

/// A space's window fills the space's output and takes the space's
/// keyboard, a dialog is centred on it; the person types meanwhile, and
/// their focus, cursor and window list stay as they were. The shell hears
/// of no window.
#[test]
fn a_window_fills_its_space_and_takes_its_keyboard() {
    use smithay::input::{pointer::CursorImageStatus, SeatHandler};
    use wayland_protocols::xdg::shell::client::xdg_toplevel::State;

    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let mut a = person_with_a_window(&h);
    events_until_mark(&h, &mut shell, "test.start");
    let before = person_snapshot(&h);

    let (desk, _socket, mut b) = space_with_a_client(&h);
    b.keyboard();
    person_key(&h, KEY_A);
    let window = b.toplevel("b");
    assert_eq!(b.state.toplevels[window.index].size, (640, 480));
    assert!(b.state.toplevels[window.index].states.contains(&State::Maximized));
    b.draw_configured(&window, 64, 64, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.as_ref() == Some(&window.surface));
    wait_for(&mut b, |b| b.state.toplevels[window.index].states.contains(&State::Activated));
    person_key(&h, KEY_B);

    // A dialog keeps its own size, sits in the middle and takes the keyboard;
    // when it goes the window under it has the keyboard again.
    let dialog = b.child_toplevel(&window, "d");
    assert_eq!(b.state.toplevels[dialog.index].size, (0, 0));
    b.draw(&dialog, 100, 80, BLUE);
    wait_for(&mut b, |b| b.state.keyboard_focus.as_ref() == Some(&dialog.surface));
    let places = h.with(move |state| {
        let space = &state.desk(desk).space;
        space.elements().map(|w| space.element_geometry(w).unwrap()).map(|g| (g.loc.x, g.loc.y, g.size.w, g.size.h)).collect::<Vec<_>>()
    });
    assert_eq!(places, [(0, 0, 640, 480), (270, 200, 100, 80)]);
    dialog.toplevel.destroy();
    dialog.xdg_surface.destroy();
    dialog.surface.destroy();
    wait_for(&mut b, |b| b.state.keyboard_focus.as_ref() == Some(&window.surface));

    // It is as large as it gets and never fullscreen: each request is
    // answered with the state it has.
    let configures = b.state.toplevels[window.index].configures;
    window.toplevel.set_fullscreen(None);
    b.roundtrip();
    window.toplevel.set_maximized();
    b.roundtrip();
    window.toplevel.unset_fullscreen();
    b.roundtrip();
    let now = b.state.toplevels[window.index].clone();
    assert_eq!((now.configures, now.size), (configures + 3, (640, 480)));
    assert!(now.states.contains(&State::Maximized) && !now.states.contains(&State::Fullscreen));

    // The space's seat changes the space's cursor.
    h.with(move |state| {
        let seat = state.desk(desk).seat.clone();
        SeatHandler::cursor_image(state, &seat, CursorImageStatus::Hidden);
        assert_eq!(state.desk(desk).cursor_status, CursorImageStatus::Hidden);
    });

    // The person typed all along: their client has both keys, the space's none.
    wait_for(&mut a, |a| a.state.keys == [KEY_A, KEY_B]);
    assert!(b.state.keys.is_empty());
    assert_person_untouched(&h, &before);
    assert_eq!(shell.windows().unwrap().len(), 1);
    assert_eq!(events_until_mark(&h, &mut shell, "test.end"), Vec::<String>::new());
}

/// A space has a clock of its own: a commit arms one tick, slow while
/// nobody uses the space and fast after a capture, and an idle space arms
/// none. Its capture shows the space alone, the person's shows none of
/// it, and the person's lock changes neither.
#[test]
fn a_space_has_its_own_frames_and_capture() {
    use ede_compositor::spaces::frames::{FAST, SLOW};
    use std::time::Duration;
    const GREEN: u32 = 0xff00_ff00;

    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut a = person_with_a_window(&h);
    let before = person_snapshot(&h);
    let (desk, _socket, mut b) = space_with_a_client(&h);
    let window = b.toplevel("b");
    b.draw_configured(&window, 64, 64, RED);
    let clock = move |h: &Harness| h.with(move |state| state.spaces.get(desk).map(|s| (s.scheduled, s.pace, s.ticks, s.serial)).unwrap());
    let settled = |h: &Harness| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while clock(h).0 {
            assert!(std::time::Instant::now() < deadline, "the space's clock never rests");
            std::thread::sleep(Duration::from_millis(10));
        }
        clock(h)
    };

    // The window's commits armed the clock, slowly: nobody uses the space.
    let (_, pace, ticks, serial) = settled(&h);
    assert!(ticks > 0 && serial > 0 && pace == SLOW, "{ticks} ticks, serial {serial}, {pace:?}");
    // Idle: nothing is armed and nothing ticks.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(clock(&h), (false, SLOW, ticks, serial));

    // A client that animates gets its callbacks, ten a second at most.
    person_key(&h, KEY_A);
    let slow = b.count_frames(&window.surface, Duration::from_millis(600));
    assert!((1..=7).contains(&slow), "{slow} callbacks in 600 ms on the slow clock");
    assert_eq!(settled(&h).3, serial, "a commit without a new buffer changes nothing");

    // The capture shows the space alone, and makes the clock fast.
    let frame = h.with(move |state| state.capture_space(desk)).expect("a capture");
    assert_eq!((frame.width, frame.height, frame.at(320, 240)), (640, 480, RED));
    assert!(!frame.pixels.contains(&BLUE), "nothing of the person's desk");
    assert!(b.count_frames(&window.surface, Duration::from_millis(200)) > 0);
    assert_eq!(settled(&h).1, FAST);
    // A new buffer moves the serial, once, whoever looks first.
    b.draw_configured(&window, 64, 64, GREEN);
    let frame = h.with(move |state| state.capture_space(desk)).expect("a capture");
    assert_eq!(frame.at(320, 240), GREEN);
    assert_eq!(settled(&h).3, serial + 1);

    // The person's capture has their window and nothing of the space.
    let persons = h.capture_shared(0);
    assert!(persons.pixels.contains(&BLUE) && !persons.pixels.contains(&GREEN));
    wait_for(&mut a, |a| a.state.keys == [KEY_A]);
    assert_person_untouched(&h, &before);

    // Locked: the person's capture is black, the space draws and ticks on.
    let mut locker = Session::connect(h.connect());
    let _lock = locker.lock();
    assert!(h.with(|state| state.is_locked()));
    assert!(h.capture_shared(0).pixels.iter().all(|p| *p == 0xff00_0000));
    assert_eq!(h.with(move |state| state.capture_space(desk)).expect("a capture").at(320, 240), GREEN);
    assert!(b.count_frames(&window.surface, Duration::from_millis(200)) > 0);
}

/// What an agent drives lands in its space and nowhere else: its keys
/// never carry the Shift the person holds, its click neither reaches the
/// person's client nor closes the shell's menu, its seat's idle
/// notification resumes while the person's idle clock stands still.
#[test]
fn driven_input_lands_on_its_desk() {
    use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;
    const SHIFT: u32 = 1;

    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let mut a = person_with_a_window(&h);
    a.pointer();
    // The shell's menu is open: a press of the person outside it would dismiss it.
    let panel = a.layer("panel", Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 40), 40);
    a.draw_layer(&panel, BLUE);
    let _menu = a.layer_popup(&panel, (100, 100), false, BLUE);
    assert!(h.with(|state| state.layer_popups_open()));

    let (desk, _socket, mut b) = space_with_a_client(&h);
    b.keyboard();
    b.pointer();
    let window = b.toplevel("b");
    b.draw_configured(&window, 64, 64, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.as_ref() == Some(&window.surface));
    let _idle = b.idle_notification(50);
    wait_for(&mut b, |b| b.state.idled == 1);

    // The person holds Shift through everything the agent does.
    h.with(|state| state.keyboard_key((KEY_LEFTSHIFT + 8).into(), KeyState::Pressed, 0));
    wait_for(&mut a, |a| a.state.modifiers.last() == Some(&SHIFT));
    events_until_mark(&h, &mut shell, "test.start");
    let before = person_snapshot(&h);
    let active = h.with(|state| state.idle.last_activity);
    let heard = |a: &Session| (a.state.keys.len(), a.state.modifiers.len(), a.state.pointer_enters, a.state.pointer_motions, a.state.buttons.len());
    let persons = heard(&a);

    // A key: the space's client gets a plain g, and its user is back.
    h.with(move |state| {
        state.drive_key(desk, (KEY_G + 8).into(), KeyState::Pressed, 0, false).unwrap();
        state.drive_key(desk, (KEY_G + 8).into(), KeyState::Released, 0, false).unwrap();
    });
    wait_for(&mut b, |b| b.state.keys == [KEY_G] && b.state.resumed == 1);
    assert!(b.state.modifiers.iter().all(|m| *m == 0), "the person's Shift is not the space's: {:?}", b.state.modifiers);
    // A keysym that needs Shift holds the space's own.
    h.with(move |state| {
        assert!(state.drive_keysym(desk, 0x47, true, false, 0).unwrap());
        assert!(state.drive_keysym(desk, 0x47, false, false, 0).unwrap());
    });
    wait_for(&mut b, |b| b.state.keys == [KEY_G, KEY_LEFTSHIFT, KEY_G] && b.state.modifiers.contains(&SHIFT) && b.state.modifiers.last() == Some(&0));
    // The pointer stays inside the space's output; a click reaches its client.
    h.with(move |state| {
        state.drive_pointer_motion(desk, (5000.0, 100.0).into(), 0).unwrap();
        assert_eq!(state.desk(desk).seat.get_pointer().unwrap().current_location(), (639.0, 100.0).into());
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Pressed, 0).unwrap();
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Released, 0).unwrap();
    });
    wait_for(&mut b, |b| b.state.pointer_enters == 1 && b.state.buttons == [(BTN_LEFT, true), (BTN_LEFT, false)]);

    // The person's client heard none of it, their idle clock did not move,
    // their desk is as it was and the shell was told nothing: the menu stays.
    a.roundtrip();
    assert_eq!(heard(&a), persons);
    assert_eq!(h.with(|state| state.idle.last_activity), active);
    assert_person_untouched(&h, &before);
    assert_eq!(events_until_mark(&h, &mut shell, "test.end"), Vec::<String>::new());
    assert!(h.with(|state| state.layer_popups_open()));
    h.with(|state| state.keyboard_key((KEY_LEFTSHIFT + 8).into(), KeyState::Released, 0));
    wait_for(&mut a, |a| a.state.modifiers.last() == Some(&0));
}

fn code<T: std::fmt::Debug>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// A `[space]` grant for `session`, living `ms` from now (`None` = for
/// good), and a connection that said hello with it.
fn space_grant(h: &Harness, session: &str, ms: Option<i64>) -> (ede_protocol::Grant, ede_protocol::Client) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let grant = h.ipc().mint(owner(session), vec![Capability::Space], ms.map(|ms| now + ms)).unwrap();
    (grant.clone(), h.ipc_with(grant.token).unwrap())
}

/// A space grant makes its session's one space; the fifth session finds
/// no room; a dropped connection closes nothing; the owner or `system`
/// destroys it, another session cannot.
#[test]
fn a_space_grant_creates_its_one_space() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let mut root = h.ipc();
    let (_, mut agent) = space_grant(&h, "run-1", None);
    assert_eq!(agent.spaces().unwrap(), []);
    let space = agent.create_space(640, 480).unwrap();
    let socket = dir.path().join("wayland-test.spaces/space-1/wayland-0");
    assert_eq!((space.id.as_str(), space.session_id.as_str(), space.output.as_str(), space.width, space.height), ("space-1", "run-1", "space-1", 640, 480));
    assert_eq!(space.wayland_display, socket.to_string_lossy());
    Session::connect(UnixStream::connect(&space.wayland_display).expect("the space's socket"));
    // Again is the same space, whatever size is asked for, also on a new connection.
    assert_eq!(agent.create_space(800, 600).unwrap(), space);
    assert_eq!(code(agent.create_space(1, 1)), ErrorCode::Invalid);
    drop(agent);
    let (_, mut agent) = space_grant(&h, "run-1", None);
    assert_eq!(agent.create_space(640, 480).unwrap(), space);
    assert_eq!(agent.spaces().unwrap(), [space.clone()]);

    // Four spaces at most; each session lists its own, `system` lists all.
    let mut others: Vec<_> = (2..=4).map(|n| space_grant(&h, &format!("run-{n}"), None).1).collect();
    for other in &mut others {
        other.create_space(320, 240).unwrap();
    }
    assert_eq!(others[0].spaces().unwrap().iter().map(|s| s.id.as_str().to_string()).collect::<Vec<_>>(), ["space-2"]);
    assert_eq!(root.spaces().unwrap().len(), 4);
    let (_, mut fifth) = space_grant(&h, "run-5", None);
    assert_eq!(code(fifth.create_space(320, 240)), ErrorCode::Limit);

    // Another session cannot destroy it; its owner can, and so can `system`.
    assert_eq!(code(others[0].destroy_space(space.id.clone())), ErrorCode::Forbidden);
    assert_eq!(code(others[0].destroy_space(SpaceId::new("space-9").unwrap())), ErrorCode::NotFound);
    agent.destroy_space(space.id.clone()).unwrap();
    assert!(!socket.exists());
    assert_eq!(agent.spaces().unwrap(), []);
    root.destroy_space(SpaceId::new("space-2").unwrap()).unwrap();
    assert_eq!(root.spaces().unwrap().len(), 2);
    fifth.create_space(320, 240).expect("room again");
    spaces_settled(&h);
}

/// A space ends with its grant: at once when the session's last space
/// grant is revoked, and when it expired, after the grace, unless the
/// session was granted anew meanwhile.
#[test]
fn a_space_ends_with_its_grant() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    h.with(|state| state.spaces.expiry_grace = 3);
    let mut root = h.ipc();
    let listed = |root: &mut ede_protocol::Client| root.spaces().unwrap().iter().map(|s| s.session_id.as_str().to_string()).collect::<Vec<_>>();
    let tick = |n: u32| h.with(move |state| (0..n).for_each(|_| state.spaces_tick()));

    // Revoked: with a second grant of the session alive the space stays,
    // with the last one gone it closes at once.
    let (first, mut agent) = space_grant(&h, "run-1", None);
    let (second, _) = space_grant(&h, "run-1", None);
    let space = agent.create_space(640, 480).unwrap();
    let mut b = Session::connect(UnixStream::connect(&space.wayland_display).unwrap());
    root.call(Request::RevokeGrant { token: first.token }).unwrap();
    assert_eq!(listed(&mut root), ["run-1"]);
    root.call(Request::RevokeGrant { token: second.token }).unwrap();
    assert_eq!(listed(&mut root), Vec::<String>::new());
    assert!(!std::path::Path::new(&space.wayland_display).exists());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while b.queue.roundtrip(&mut b.state).is_ok() {
        assert!(std::time::Instant::now() < deadline, "the space's client is still connected");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Expired: the space outlives its grant by the grace, counted in the
    // compositor's own ticks, and no longer.
    let (_, mut agent) = space_grant(&h, "run-2", Some(150));
    agent.create_space(640, 480).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(code(agent.spaces()), ErrorCode::Unauthorized);
    tick(1);
    assert_eq!(listed(&mut root), ["run-2"]);
    // A new grant for the session inside the grace keeps it, and the count starts over.
    let (_, mut renewed) = space_grant(&h, "run-2", Some(60_000));
    tick(5);
    assert_eq!(renewed.spaces().unwrap().len(), 1);
    // A connection that goes closes nothing.
    drop((agent, renewed));
    tick(5);
    assert_eq!(listed(&mut root), ["run-2"]);

    let (_, mut short) = space_grant(&h, "run-3", Some(150));
    short.create_space(640, 480).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    tick(1);
    assert_eq!(listed(&mut root), ["run-2", "run-3"]);
    tick(3);
    assert_eq!(listed(&mut root), ["run-2"]);
    spaces_settled(&h);
}
