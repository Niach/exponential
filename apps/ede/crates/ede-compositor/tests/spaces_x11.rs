//! #EDE-125 headless, with Xwayland: the person's X11 clients and an
//! agent's space.
#![cfg(feature = "xwayland")]

mod common;

use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::{
    space::{start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId};
use ede_protocol::SessionId;
use smithay::backend::input::ButtonState;
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, WindowClass},
    wrapper::ConnectionExt as _,
};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const BTN_LEFT: u32 = 0x110;

/// Xwayland on `h`, or `None` (skipped) without the binary.
fn xwayland(h: &Harness) -> Option<x11rb::rust_connection::RustConnection> {
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return None;
    }
    h.with(|ede| ede.start_xwayland());
    let deadline = Instant::now() + Duration::from_secs(30);
    let display = loop {
        if let Some(n) = h.with(|ede| ede.xdisplay) {
            break n;
        }
        assert!(Instant::now() < deadline, "Xwayland never became ready");
        std::thread::sleep(Duration::from_millis(20));
    };
    Some(x11rb::connect(Some(&format!(":{display}"))).expect("connect to Xwayland").0)
}

/// The person presses on an X11 window, then an agent presses in its
/// space, then the X11 client starts a drag. The drag is the person's:
/// the space's seat, which holds the newer press, keeps its pointer, and
/// its client hears of no drag and no offer.
#[test]
fn a_persons_x11_drag_stays_on_their_seat() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let Some(conn) = xwayland(&h) else { return };

    // A, the person's Wayland client, in the top left corner.
    let mut a = Session::connect(h.connect());
    let target = a.toplevel("a");
    a.draw(&target, 100, 80, BLUE);
    a.keyboard();
    a.pointer();
    a.data_device();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned().unwrap();
        state.move_window(&window, (0, 40).into(), false);
    });

    // The person's X11 window, which offers one type to drag.
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    let aux = CreateWindowAux::new().background_pixel(0x00ff_0000).event_mask(EventMask::BUTTON_PRESS);
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 320, 240, 0, WindowClass::INPUT_OUTPUT, 0, &aux).unwrap();
    let atom = |name: &[u8]| conn.intern_atom(false, name).unwrap().reply().unwrap().atom;
    conn.change_property32(PropMode::REPLACE, win, atom(b"XdndAware"), AtomEnum::ATOM, &[5]).unwrap();
    conn.change_property32(PropMode::REPLACE, win, atom(b"XdndTypeList"), AtomEnum::ATOM, &[atom(b"text/plain")]).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let x11 = loop {
        let mapped = h.with(move |state| {
            let window = state.desk.space.elements().find(|w| w.x11_surface().is_some_and(|x| x.window_id() == win) && smithay::wayland::seat::WaylandFocus::wl_surface(*w).is_some())?;
            state.desk.space.element_geometry(window)
        });
        if let Some(geo) = mapped {
            break geo;
        }
        assert!(Instant::now() < deadline, "the X11 window never mapped");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(x11.loc.x > 100 && x11.loc.y > 120, "the two windows of the person do not overlap: {x11:?}");

    // B in a space, with its pointer and data device.
    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    b.pointer();
    b.data_device();
    let window = b.toplevel("b");
    b.draw_configured(&window, 64, 64, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());

    // The person presses on the X11 window; the agent's button goes down after it.
    h.with(move |state| {
        state.person_pointer_motion_to((x11.loc.x as f64 + 160.0, x11.loc.y as f64 + 120.0).into(), 0);
        state.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 0);
        state.drive_pointer_motion(desk, (320.0, 240.0).into(), 0).unwrap();
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Pressed, 0).unwrap();
    });
    wait_for(&mut b, |b| b.state.buttons == [(BTN_LEFT, true)]);

    // The X11 client starts its drag: smithay maps its drop proxy above
    // everything once it has found the press the drag belongs to.
    let above = |conn: &x11rb::rust_connection::RustConnection| conn.query_tree(root).unwrap().reply().unwrap().children.len();
    let before = above(&conn);
    conn.set_selection_owner(win, atom(b"XdndSelection"), x11rb::CURRENT_TIME).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while above(&conn) == before {
        assert!(Instant::now() < deadline, "the X11 drag never started");
        std::thread::sleep(Duration::from_millis(20));
    }

    // It took the person's press. The agent's pointer is still a pointer:
    // its moves reach B as moves, and B hears of no drag and no offer.
    let moved = b.state.pointer_motions;
    h.with(move |state| {
        state.person_pointer_motion_to((50.0, 80.0).into(), 0);
        state.drive_pointer_motion(desk, (300.0, 200.0).into(), 0).unwrap();
        state.drive_pointer_motion(desk, (320.0, 240.0).into(), 0).unwrap();
    });
    wait_for(&mut b, |b| b.state.pointer_motions == moved + 2);
    assert!(b.state.offers.is_empty() && b.state.drag_enters == 0, "the space heard of the person's drag");
    // And the person's client hears no pointer motion while they drag.
    a.roundtrip();
    assert_eq!(a.state.pointer_motions, 0);

    h.with(move |state| {
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Released, 0).unwrap();
        state.person_pointer_button(BTN_LEFT, ButtonState::Released, 0);
        assert!(state.focused_window(DeskId::PERSON).is_some());
    });
}

/// #EDE-138: an agent's space with an Xwayland of its own, started at its
/// first launch. The agent's grant, its space and the display the launch
/// got (from the program's own environment).
fn space_with_x11(h: &Harness, dir: &std::path::Path, session: &str) -> (ede_protocol::Client, ede_protocol::SpaceInfo, u32) {
    let grant = h.ipc().mint(SessionId::new(session).unwrap(), vec![ede_protocol::Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    h.with(|state| state.spaces.x11 = true);
    let out = dir.join(format!("{session}.display"));
    agent.launch(vec!["sh".into(), "-c".into(), format!("echo \"$DISPLAY\" > {0}.tmp && mv {0}.tmp {0}; exec sleep 60", out.display())], Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !out.exists() {
        assert!(Instant::now() < deadline, "the launch never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    let display: u32 = std::fs::read_to_string(&out).unwrap().trim().strip_prefix(':').expect("a DISPLAY").parse().unwrap();
    let desk = space_desk(&space);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| state.space_xwm_ready(desk)) {
        assert!(Instant::now() < deadline, "the space's Xwayland never became ready");
        std::thread::sleep(Duration::from_millis(20));
    }
    (agent, space, display)
}

fn space_desk(space: &ede_protocol::SpaceInfo) -> DeskId {
    DeskId::from_space_id(&space.id).expect("space-n")
}

/// A window of `conn`'s, mapped.
fn x11_window(conn: &x11rb::rust_connection::RustConnection, color: u32) -> u32 {
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    let aux = CreateWindowAux::new().background_pixel(color);
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 150, 0, WindowClass::INPUT_OUTPUT, 0, &aux).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    win
}

/// The process of the Xwayland serving `display`, a child of this process.
fn xwayland_pid(display: u32) -> Option<u32> {
    let me = std::process::id();
    std::fs::read_dir("/proc").ok()?.flatten().filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok()).find(|pid| {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let parent = stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(1)?.parse::<u32>().ok());
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
        parent == Some(me) && args.first().is_some_and(|a| a.ends_with(b"Xwayland")) && args.iter().any(|a| *a == format!(":{display}").as_bytes())
    })
}

/// #EDE-138: a space's X11 apps get a display of their own (never `:0`,
/// never the person's), served by an Xwayland that binds the space's seat
/// and output alone; X11 windows of the person's server and of the
/// space's, made one after the other, each get their own server's
/// surface (one serial names a surface of each server), and land on
/// their own desk.
#[test]
fn a_spaces_x11_apps_get_a_display_and_a_desk_of_their_own() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(person) = xwayland(&h) else { return };
    let person_display = h.with(|state| state.xdisplay).unwrap();
    let (_agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    assert!(display >= ede_compositor::spaces::x11::FIRST_DISPLAY && display != person_display, ":{display}");
    let desk = space_desk(&space);

    // Its server binds the space's seat and output, none of the person's.
    let (on_space, on_person) = h.with(move |state| {
        let space = state.spaces.get(desk).unwrap();
        let (client, _) = space.x11().expect("the space's Xwayland");
        let client = client.clone();
        let space_globals = space.desk.seat.client_seats(&client).len() + space.output.client_outputs(&client).count();
        let person_globals = state.desk.seat.client_seats(&client).len() + state.desk.space.outputs().map(|o| o.client_outputs(&client).count()).sum::<usize>();
        (space_globals, person_globals)
    });
    assert!(on_space >= 2, "the space's Xwayland bound no seat or output of its space");
    assert_eq!(on_person, 0, "the space's Xwayland bound the person's seat or output");

    // Ten windows on each server, one after the other.
    let (in_space, _) = x11rb::connect(Some(&format!(":{display}"))).expect("the space's display");
    let mut theirs = Vec::new();
    for _ in 0..10 {
        theirs.push((true, x11_window(&person, 0x00ff_0000)));
        theirs.push((false, x11_window(&in_space, 0x0000_00ff)));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (person_ok, space_ok, wrong) = h.with(move |state| {
            use smithay::wayland::seat::WaylandFocus;
            let space_client = state.spaces.get(desk).unwrap().x11().unwrap().0.clone();
            let client_of = |w: &smithay::desktop::Window| w.wl_surface().and_then(|s| s.client());
            let person: Vec<_> = state.desk.space.elements().filter(|w| w.x11_surface().is_some()).filter_map(client_of).collect();
            let space: Vec<_> = state.spaces.get(desk).unwrap().desk.space.elements().filter(|w| w.x11_surface().is_some()).filter_map(client_of).collect();
            let wrong = person.iter().filter(|c| **c == space_client).count() + space.iter().filter(|c| **c != space_client).count();
            (person.len(), space.len(), wrong)
        });
        assert_eq!(wrong, 0, "an X11 window has the other server's surface");
        if person_ok == 10 && space_ok == 10 {
            break;
        }
        assert!(Instant::now() < deadline, "not all windows mapped: {person_ok} of the person's, {space_ok} of the space's");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(theirs.len(), 20);
}

/// #EDE-138: a space's Xwayland that dies is let go of (its space and the
/// person's desk live on), and the space's next launch starts another.
/// A closed space's Xwayland leaves nothing: no process, socket or lock.
#[test]
fn a_spaces_xwayland_that_dies_comes_back_and_goes_with_its_space() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(_person) = xwayland(&h) else { return };
    let (mut agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    let desk = space_desk(&space);
    let pid = xwayland_pid(display).expect("the space's Xwayland process");
    assert!(std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).status().unwrap().success());
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.with(move |state| state.spaces.get(desk).is_some_and(|s| s.x11().is_some())) {
        assert!(Instant::now() < deadline, "the dead Xwayland was never let go of");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(agent.spaces().unwrap(), [space.clone()], "the space lives on");
    assert!(h.with(|state| state.xdisplay.is_some() && state.xwm.is_some()), "the person's Xwayland lives on");

    // The next launch: another Xwayland.
    let out = dir.path().join("again.display");
    agent.launch(vec!["sh".into(), "-c".into(), format!("echo \"$DISPLAY\" > {0}.tmp && mv {0}.tmp {0}; exec sleep 60", out.display())], Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| state.space_xwm_ready(desk)) || !out.exists() {
        assert!(Instant::now() < deadline, "no Xwayland after the next launch");
        std::thread::sleep(Duration::from_millis(20));
    }
    let again: u32 = std::fs::read_to_string(&out).unwrap().trim().trim_start_matches(':').parse().unwrap();
    let pid = xwayland_pid(again).expect("the new Xwayland");

    // The space closes: its Xwayland, its socket and its lock go.
    agent.destroy_space(space.id.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let gone = |pid: u32| std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |stat| stat.rsplit(')').next().unwrap_or_default().trim_start().starts_with('Z'));
    while !gone(pid) || std::path::Path::new(&format!("/tmp/.X11-unix/X{again}")).exists() || std::path::Path::new(&format!("/tmp/.X{again}-lock")).exists() {
        assert!(Instant::now() < deadline, "the closed space's Xwayland, socket or lock is still there");
        std::thread::sleep(Duration::from_millis(20));
    }
    common::space::spaces_settled(&h);
}
