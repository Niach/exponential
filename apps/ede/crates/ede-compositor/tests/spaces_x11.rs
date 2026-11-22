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
    Some(common::x11_connect(display, h.with(|ede| ede.xauthority(ede_compositor::desk::DeskId::PERSON)).as_deref()).expect("connect to Xwayland"))
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
    let in_space = common::x11_connect(display, h.with(move |state| state.xauthority(desk)).as_deref()).expect("the space's display");
    let mut theirs = Vec::new();
    for _ in 0..10 {
        theirs.push((true, x11_window(&person, 0x00ff_0000)));
        theirs.push((false, x11_window(&in_space, 0x0000_00ff)));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (person_ok, space_ok, wrong) = h.with(move |state| {
            use smithay::{reexports::wayland_server::Resource, wayland::seat::WaylandFocus};
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
        assert!(
            Instant::now() < deadline,
            "the closed space's Xwayland ({}), socket ({}) or lock ({}) is still there",
            !gone(pid),
            std::path::Path::new(&format!("/tmp/.X11-unix/X{again}")).exists(),
            std::path::Path::new(&format!("/tmp/.X{again}-lock")).exists()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    common::space::spaces_settled(&h);
}

/// #EDE-149: an X11 client of a space grabs its server and maps a window.
/// The space's window manager asks that server about the window (on the
/// compositor's thread) and gets no answer while the grab holds: past
/// `ROUND_TRIP_LIMIT` its connection is cut and the server is ended. The
/// compositor waited that long once, the person's Xwayland and the space
/// live on, and the space's next launch gets a new server.
#[test]
fn a_space_server_that_stops_answering_costs_the_space_not_the_compositor() {
    use ede_compositor::spaces::x11::ROUND_TRIP_LIMIT;
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(person) = xwayland(&h) else { return };
    let (mut agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    let desk = space_desk(&space);
    let pid = xwayland_pid(display).expect("the space's Xwayland process");

    let grabber = common::x11_connect(display, h.with(move |state| state.xauthority(desk)).as_deref()).expect("the space's display");
    grabber.grab_server().unwrap();
    x11_window(&grabber, BLUE);

    // The compositor answers on while the window manager waits: once for
    // the limit at most (and the harness's own turn), then at once.
    let (mut longest, deadline) = (Duration::ZERO, Instant::now() + Duration::from_secs(30));
    while h.with(move |state| state.spaces.get(desk).is_some_and(|s| s.x11().is_some())) {
        assert!(Instant::now() < deadline, "the space's server that stopped answering was never cut off");
        let asked = Instant::now();
        h.with(|_| ());
        longest = longest.max(asked.elapsed());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(longest < ROUND_TRIP_LIMIT + Duration::from_secs(1), "the compositor waited {longest:?} for the space's server");
    let gone = |pid: u32| std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |stat| stat.rsplit(')').next().unwrap_or_default().trim_start().starts_with('Z'));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !gone(pid) {
        assert!(Instant::now() < deadline, "the server that stopped answering still runs");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(agent.spaces().unwrap(), [space.clone()], "the space lives on");
    assert!(h.with(|state| state.xdisplay.is_some() && state.xwm.is_some()), "the person's Xwayland lives on");
    // The person's server still answers, and its window manager with it.
    let before = h.with(|state| state.desk.space.elements().filter(|w| w.x11_surface().is_some()).count());
    x11_window(&person, RED);
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.with(|state| state.desk.space.elements().filter(|w| w.x11_surface().is_some()).count()) == before {
        assert!(Instant::now() < deadline, "the person's X11 window never mapped");
        std::thread::sleep(Duration::from_millis(20));
    }

    // The next launch: another server.
    let out = dir.path().join("again.display");
    agent.launch(vec!["sh".into(), "-c".into(), format!("echo \"$DISPLAY\" > {0}.tmp && mv {0}.tmp {0}; exec sleep 60", out.display())], Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| state.space_xwm_ready(desk)) || !out.exists() {
        assert!(Instant::now() < deadline, "no Xwayland after the next launch");
        std::thread::sleep(Duration::from_millis(20));
    }
    let again: u32 = std::fs::read_to_string(&out).unwrap().trim().trim_start_matches(':').parse().unwrap();
    assert!(xwayland_pid(again).is_some_and(|new| new != pid), "the space's new Xwayland");
    agent.destroy_space(space.id.clone()).unwrap();
    common::space::spaces_settled(&h);
}

/// #EDE-138: an X11 window launched into a space fills it, takes the
/// space's keyboard and gets the agent's keys, while the person types into
/// their own window and gets exactly theirs. The person's focus, pointer,
/// windows and clipboard stay as they were, and the shell hears of no
/// window of the space's.
#[test]
fn an_x11_window_in_a_space_gets_the_agents_keys_while_the_person_types() {
    use x11rb::protocol::{xproto::ConnectionExt as _, Event};
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(_person_x) = xwayland(&h) else { return };
    // The person's Wayland window, focused.
    let mut a = Session::connect(h.connect());
    let target = a.toplevel("a");
    a.draw(&target, 100, 80, BLUE);
    a.keyboard();
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let before = common::space::person_snapshot(&h);
    let mut shell = h.ipc();
    shell.subscribe().unwrap();

    let (_agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    let desk = space_desk(&space);
    let conn = common::x11_connect(display, h.with(move |state| state.xauthority(desk)).as_deref()).expect("the space's display");
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    let aux = CreateWindowAux::new().background_pixel(0x0000_00ff).event_mask(EventMask::KEY_PRESS | EventMask::FOCUS_CHANGE);
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 150, 0, WindowClass::INPUT_OUTPUT, 0, &aux).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();

    // It fills the space and has the space's keyboard.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let shown = h.with(move |state| {
            use smithay::wayland::seat::WaylandFocus;
            let d = &state.spaces.get(desk)?.desk;
            let window = d.space.elements().find(|w| w.x11_surface().is_some_and(|x| x.window_id() == win))?;
            let surface = window.wl_surface()?.into_owned();
            let focused = d.seat.get_keyboard().unwrap().current_focus() == Some(surface);
            Some((d.space.element_geometry(window)?, focused))
        });
        if let Some((geometry, true)) = shown {
            assert_eq!((geometry.loc.x, geometry.loc.y, geometry.size.w, geometry.size.h), (0, 0, 640, 480), "the X11 window fills the space");
            break;
        }
        assert!(Instant::now() < deadline, "the X11 window never mapped with the space's keyboard: {shown:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(conn.get_geometry(win).unwrap().reply().unwrap().width, 640, "X knows the size the space gave it");

    // The agent types into the space while the person types into theirs.
    const KEY_A: u32 = 30;
    const KEY_B: u32 = 48;
    for _ in 0..3 {
        h.with(move |state| {
            use smithay::backend::input::KeyState;
            state.drive_key(desk, (KEY_A + 8).into(), KeyState::Pressed, 0, false).unwrap();
            state.drive_key(desk, (KEY_A + 8).into(), KeyState::Released, 0, false).unwrap();
        });
        common::space::person_key(&h, KEY_B);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut presses = Vec::new();
    while presses.len() < 3 {
        assert!(Instant::now() < deadline, "the X11 window got {} of the agent's 3 keys", presses.len());
        while let Some(event) = conn.poll_for_event().unwrap() {
            if let Event::KeyPress(press) = event {
                presses.push(press.detail);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(presses, [KEY_A as u8 + 8; 3], "the X11 window got exactly the agent's keys");
    wait_for(&mut a, |a| a.state.keys.len() >= 3);
    assert_eq!(a.state.keys, [KEY_B; 3], "the person's window got exactly the person's keys");

    // Nothing of the person's moved, and the shell heard of no window.
    common::space::assert_person_untouched(&h, &before);
    let events = common::space::events_until_mark(&h, &mut shell, "x11-space");
    assert!(events.iter().all(|e| !e.starts_with("window.")), "the shell heard of the space's X11 window: {events:?}");
    // The agent's window list names it, as X11.
    let windows = _agent_windows(&h, desk);
    assert_eq!(windows, [(true, 640, 480)]);

    // The space closes with its X11 window holding its keyboard: both of
    // its seats go.
    let mut agent = _agent;
    agent.destroy_space(space.id.clone()).unwrap();
    common::space::spaces_settled(&h);
}

/// The space's windows as its agent lists them: (x11, width, height).
fn _agent_windows(h: &Harness, desk: DeskId) -> Vec<(bool, i32, i32)> {
    h.with(move |state| {
        let space = state.spaces.get(desk).unwrap();
        space.desk.space.elements().filter(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect())).map(|w| {
            let g = space.desk.space.element_geometry(w).unwrap();
            (w.x11_surface().is_some(), g.size.w, g.size.h)
        }).collect()
    })
}

const UTF8: &str = "text/plain;charset=utf-8";

/// An atom of `conn`'s server.
fn atom(conn: &impl Connection, name: &str) -> u32 {
    conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom
}

/// Waits until the X11 window `win` of space `desk` holds the space's keyboard.
fn wait_for_space_keyboard(h: &Harness, desk: DeskId, win: u32) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| {
        use smithay::wayland::seat::WaylandFocus;
        let Some(d) = state.try_desk(desk) else { return false };
        let surface = d.space.elements().find(|w| w.x11_surface().is_some_and(|x| x.window_id() == win)).and_then(|w| w.wl_surface().map(|s| s.into_owned()));
        surface.is_some() && d.seat.get_keyboard().unwrap().current_focus() == surface
    }) {
        assert!(Instant::now() < deadline, "the space's X11 window never took the space's keyboard");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The X11 client of `win` pastes CLIPBOARD as UTF8_STRING (retried until
/// an owner answers); `pump` runs the source's client meanwhile.
fn x11_paste(conn: &impl Connection, win: u32, pump: &mut dyn FnMut()) -> Vec<u8> {
    use x11rb::protocol::Event;
    let (clipboard, utf8, prop) = (atom(conn, "CLIPBOARD"), atom(conn, "UTF8_STRING"), atom(conn, "EDE_TEST"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "the X11 client never got the selection");
        conn.convert_selection(win, clipboard, utf8, prop, x11rb::CURRENT_TIME).unwrap();
        conn.flush().unwrap();
        let wait = Instant::now() + Duration::from_secs(2);
        while Instant::now() < wait {
            match conn.poll_for_event().unwrap() {
                Some(Event::SelectionNotify(n)) if n.property == prop => return conn.get_property(true, win, prop, AtomEnum::ANY, 0, u32::MAX).unwrap().reply().unwrap().value,
                Some(Event::SelectionNotify(_)) => break,
                _ => {
                    pump();
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Answers the SelectionRequests waiting on `conn`: TARGETS, and
/// UTF8_STRING with `text` (`None` = never: a hung owner).
fn serve_x11(conn: &impl Connection, text: Option<&[u8]>) {
    use x11rb::protocol::{
        xproto::{SelectionNotifyEvent, SELECTION_NOTIFY_EVENT},
        Event,
    };
    let (targets, utf8) = (atom(conn, "TARGETS"), atom(conn, "UTF8_STRING"));
    while let Some(event) = conn.poll_for_event().unwrap() {
        let Event::SelectionRequest(r) = event else { continue };
        let property = if r.target == targets {
            conn.change_property32(PropMode::REPLACE, r.requestor, r.property, AtomEnum::ATOM, &[targets, utf8]).unwrap();
            r.property
        } else if r.target == utf8 {
            let Some(text) = text else { continue };
            conn.change_property8(PropMode::REPLACE, r.requestor, r.property, utf8, text).unwrap();
            r.property
        } else {
            x11rb::NONE
        };
        let notify = SelectionNotifyEvent { response_type: SELECTION_NOTIFY_EVENT, sequence: 0, time: r.time, requestor: r.requestor, selection: r.selection, target: r.target, property };
        conn.send_event(false, r.requestor, EventMask::NO_EVENT, notify).unwrap();
        conn.flush().unwrap();
    }
}

/// The owner of CLIPBOARD on `conn`'s server.
fn x11_clipboard_owner(conn: &impl Connection) -> u32 {
    conn.get_selection_owner(atom(conn, "CLIPBOARD")).unwrap().reply().unwrap().owner
}

/// #EDE-138: a space's clipboard crosses the space's own Xwayland both
/// ways. A Wayland copy in the space pastes in its X11 app, and so does
/// the agent's own selection (taken on the space's clipboard); the X11
/// app's copy reaches the agent's read and the space's Wayland client. The
/// person's serial, owner, paste and X11 CLIPBOARD stay as they were.
#[test]
fn a_spaces_clipboard_crosses_its_own_xwayland_both_ways() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(person_x) = xwayland(&h) else { return };
    // The person's Wayland window, focused, with a copy of its own, which
    // the person's Xwayland follows.
    let mut a = Session::connect(h.connect());
    let person = a.toplevel("a");
    a.draw(&person, 100, 80, BLUE);
    a.keyboard();
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let device_a = a.data_device();
    let _copied_a = a.set_selection(&device_a, &[(UTF8, Some(b"person"))]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while x11_clipboard_owner(&person_x) == x11rb::NONE {
        assert!(Instant::now() < deadline, "the person's Xwayland never took the person's copy");
        std::thread::sleep(Duration::from_millis(20));
    }
    let person_owner = x11_clipboard_owner(&person_x);
    let before = common::space::person_snapshot(&h);

    let (mut agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    let desk = space_desk(&space);
    // The space's Wayland client copies.
    let socket = h.with(move |state| state.spaces.get(desk).unwrap().socket());
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    let window = b.toplevel("b");
    b.draw(&window, 640, 480, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    let device_b = b.data_device();
    let _copied_b = b.set_selection(&device_b, &[(UTF8, Some(b"from wayland"))]);

    // The space's X11 app, with the space's keyboard, pastes it.
    let conn = common::x11_connect(display, h.with(move |state| state.xauthority(desk)).as_deref()).expect("the space's display");
    let conn = Arc::new(conn);
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 150, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE)).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    wait_for_space_keyboard(&h, desk, win);
    assert_eq!(x11_paste(&*conn, win, &mut || b.roundtrip()), b"from wayland");

    // The agent's own selection: pasted in X11, taken on the space's clipboard.
    let serial = agent.clipboard_set(vec![ede_protocol::ClipboardItem { mime: UTF8.into(), data: b"from agent".to_vec() }], false).unwrap();
    assert_eq!(x11_paste(&*conn, win, &mut || {}), b"from agent");
    assert!(agent.clipboard_taken(serial).unwrap().taken, "the X11 paste is not counted on the space's clipboard");

    // The X11 app copies: the agent reads it, and so does the space's
    // Wayland client once it has the keyboard again.
    let stop = Arc::new(AtomicBool::new(false));
    let server = {
        let (conn, stop) = (conn.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                serve_x11(&*conn, Some(b"from x11"));
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    };
    conn.set_selection_owner(win, atom(&*conn, "CLIPBOARD"), x11rb::CURRENT_TIME).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while agent.clipboard_serial().unwrap() == serial {
        assert!(Instant::now() < deadline, "the space's X11 copy never reached the space's clipboard");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(agent.clipboard_text().unwrap().text.as_deref(), Some("from x11"));
    let offers = b.state.offers.len();
    h.with(move |state| {
        let wayland = state.spaces.get(desk).unwrap().desk.space.elements().find(|w| w.x11_surface().is_none()).cloned();
        state.space_focus(desk, wayland);
    });
    wait_for(&mut b, |b| b.state.offers.len() > offers && b.selection_types().is_some_and(|types| types.iter().any(|t| t == UTF8)));
    assert_eq!(b.read_selection(UTF8), b"from x11");
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();

    // The person's desk, paste and X11 CLIPBOARD: as they were.
    common::space::assert_person_untouched(&h, &before);
    a.roundtrip();
    assert_eq!(a.read_selection(UTF8), b"person");
    assert_eq!(x11_clipboard_owner(&person_x), person_owner, "the person's X11 CLIPBOARD changed hands");
}

/// #EDE-138: what the agent copied before its space's Xwayland ran (the
/// text for an X11 app it is about to launch) pastes in that app.
#[test]
fn an_agents_copy_from_before_its_xwayland_ran_pastes_in_x11() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let grant = h.ipc().mint(SessionId::new("run-1").unwrap(), vec![ede_protocol::Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let desk = space_desk(&space);
    agent.clipboard_set(vec![ede_protocol::ClipboardItem { mime: UTF8.into(), data: b"early".to_vec() }], false).unwrap();
    h.with(|state| state.spaces.x11 = true);
    let out = dir.path().join("early.display");
    agent.launch(vec!["sh".into(), "-c".into(), format!("echo \"$DISPLAY\" > {0}.tmp && mv {0}.tmp {0}; exec sleep 60", out.display())], Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| state.space_xwm_ready(desk)) || !out.exists() {
        assert!(Instant::now() < deadline, "the space's Xwayland never became ready");
        std::thread::sleep(Duration::from_millis(20));
    }
    let display: u32 = std::fs::read_to_string(&out).unwrap().trim().trim_start_matches(':').parse().unwrap();
    let conn = common::x11_connect(display, h.with(move |state| state.xauthority(desk)).as_deref()).expect("the space's display");
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 150, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE)).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    wait_for_space_keyboard(&h, desk, win);
    assert_eq!(x11_paste(&conn, win, &mut || {}), b"early");
}

/// #EDE-138: a drag the space's X11 app starts is the space's: it runs on
/// the space's seat, also when the person pressed a button after the
/// agent did (smithay would take the newest press). It enters the space's
/// Wayland client; the person's pointer stays theirs and their client
/// hears of no drag.
#[test]
fn a_spaces_x11_drag_stays_on_the_spaces_seat() {
    use x11rb::protocol::xproto::ClientMessageEvent;
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(_person_x) = xwayland(&h) else { return };
    // The person's Wayland client A, at the top left.
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

    let (_agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    let desk = space_desk(&space);
    // B fills the space.
    let socket = h.with(move |state| state.spaces.get(desk).unwrap().socket());
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    b.pointer();
    b.data_device();
    let window = b.toplevel("b");
    b.draw_configured(&window, 64, 64, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());

    // The space's X11 dialog (transient, so centred at its own size over
    // B), which offers one type to drag.
    let conn = common::x11_connect(display, h.with(move |state| state.xauthority(desk)).as_deref()).expect("the space's display");
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    let aux = CreateWindowAux::new().background_pixel(0x0000_ff00).event_mask(EventMask::BUTTON_PRESS);
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 150, 0, WindowClass::INPUT_OUTPUT, 0, &aux).unwrap();
    conn.change_property32(PropMode::REPLACE, win, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::WINDOW, &[root]).unwrap();
    conn.change_property32(PropMode::REPLACE, win, atom(&conn, "XdndAware"), AtomEnum::ATOM, &[5]).unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    wait_for_space_keyboard(&h, desk, win);
    let dialog = h.with(move |state| {
        let d = &state.spaces.get(desk).unwrap().desk;
        d.space.element_geometry(d.space.elements().find(|w| w.x11_surface().is_some_and(|x| x.window_id() == win)).unwrap()).unwrap()
    });
    assert!(dialog.loc.x > 100 && dialog.loc.y > 100, "the dialog is centred: {dialog:?}");

    // The agent presses on the dialog; the person's button goes down after it, on A.
    let (x, y) = (dialog.loc.x as f64 + 100.0, dialog.loc.y as f64 + 75.0);
    h.with(move |state| {
        state.drive_pointer_motion(desk, (x, y).into(), 0).unwrap();
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Pressed, 0).unwrap();
        state.person_pointer_motion_to((50.0, 80.0).into(), 0);
        state.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 0);
    });
    wait_for(&mut a, |a| a.state.buttons == [(BTN_LEFT, true)]);

    // The X11 app starts its drag: smithay maps its drop proxy over the
    // space's screen, and the app tells the proxy what it offers.
    let children = |conn: &x11rb::rust_connection::RustConnection| conn.query_tree(root).unwrap().reply().unwrap().children;
    let before = children(&conn);
    conn.set_selection_owner(win, atom(&conn, "XdndSelection"), x11rb::CURRENT_TIME).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let proxy = loop {
        if let Some(proxy) = children(&conn).into_iter().find(|w| !before.contains(w)) {
            break proxy;
        }
        assert!(Instant::now() < deadline, "the space's X11 drag never started");
        std::thread::sleep(Duration::from_millis(20));
    };
    let enter = ClientMessageEvent::new(32, proxy, atom(&conn, "XdndEnter"), [win, 5 << 24, atom(&conn, "text/plain"), 0, 0]);
    conn.send_event(false, proxy, EventMask::NO_EVENT, enter).unwrap();
    conn.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // The agent moves off the dialog onto B: the drag enters B, with the type.
    h.with(move |state| state.drive_pointer_motion(desk, (20.0, 20.0).into(), 0).unwrap());
    wait_for(&mut b, |b| b.state.drag_enters == 1);
    assert!(b.state.offers.iter().any(|(_, types)| types.iter().any(|t| t == "text/plain")), "B's drag offer: {:?}", b.state.offers.iter().map(|(_, t)| t).collect::<Vec<_>>());

    // The person's pointer is still a pointer: its moves reach A as moves,
    // and A hears of no drag and no offer.
    let moved = a.state.pointer_motions;
    h.with(|state| {
        state.person_pointer_motion_to((60.0, 90.0).into(), 0);
        state.person_pointer_motion_to((70.0, 100.0).into(), 0);
    });
    wait_for(&mut a, |a| a.state.pointer_motions == moved + 2);
    assert!(a.state.offers.is_empty() && a.state.drag_enters == 0, "the person heard of the space's drag");

    h.with(move |state| {
        state.drive_pointer_button(desk, BTN_LEFT, ButtonState::Released, 0).unwrap();
        state.person_pointer_button(BTN_LEFT, ButtonState::Released, 0);
        assert!(state.focused_window(DeskId::PERSON).is_some());
    });
}

/// #EDE-138: spaces closed in the middle of a paste through their
/// Xwayland, both ways (an X11 app reading a Wayland client that never
/// answers; the agent reading an X11 owner that never answers), leave the
/// compositor running: the agent's read ends `unavailable`, and the hung
/// client giving up its pipe after its space's window manager is gone
/// reaches nothing of it. The person types on.
#[test]
fn spaces_closed_mid_paste_leave_the_compositor_running() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(_person_x) = xwayland(&h) else { return };
    let mut a = Session::connect(h.connect());
    let person = a.toplevel("a");
    a.draw(&person, 100, 80, BLUE);
    a.keyboard();
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let before = common::space::person_snapshot(&h);

    // Space 1: its Wayland client copies and never answers; its X11 app pastes.
    let (_agent1, space1, display1) = space_with_x11(&h, dir.path(), "run-1");
    let desk1 = space_desk(&space1);
    let socket = h.with(move |state| state.spaces.get(desk1).unwrap().socket());
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    let window = b.toplevel("b");
    b.draw(&window, 640, 480, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    let device_b = b.data_device();
    let _hung = b.set_selection(&device_b, &[(UTF8, None)]);
    let x1 = common::x11_connect(display1, h.with(move |state| state.xauthority(desk1)).as_deref()).expect("space 1's display");
    let win1 = x11_window(&x1, 0x0000_00ff);
    wait_for_space_keyboard(&h, desk1, win1);
    x1.convert_selection(win1, atom(&x1, "CLIPBOARD"), atom(&x1, "UTF8_STRING"), atom(&x1, "EDE_TEST"), x11rb::CURRENT_TIME).unwrap();
    x1.flush().unwrap();
    wait_for(&mut b, |b| !b.state.held.is_empty());

    // Space 2: its X11 app copies and never hands over the text; the agent reads.
    let (agent2, space2, display2) = space_with_x11(&h, dir.path(), "run-2");
    let desk2 = space_desk(&space2);
    let x2 = common::x11_connect(display2, h.with(move |state| state.xauthority(desk2)).as_deref()).expect("space 2's display");
    let win2 = x11_window(&x2, 0x00ff_00ff);
    wait_for_space_keyboard(&h, desk2, win2);
    let serial = h.with(move |state| state.desk(desk2).clipboard.serial);
    x2.set_selection_owner(win2, atom(&x2, "CLIPBOARD"), x11rb::CURRENT_TIME).unwrap();
    x2.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.with(move |state| state.desk(desk2).clipboard.serial) == serial {
        assert!(Instant::now() < deadline, "space 2's X11 copy never reached its clipboard");
        serve_x11(&x2, None);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut agent2 = agent2;
    let read = std::thread::spawn(move || agent2.clipboard_text());
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.with(move |state| state.desk(desk2).clipboard.pending()) == 0 {
        assert!(Instant::now() < deadline, "the agent's read never went to the X11 owner");
        serve_x11(&x2, None);
        std::thread::sleep(Duration::from_millis(10));
    }

    // Both spaces close mid-paste.
    h.with(move |state| {
        assert!(state.close_space(desk1));
        assert!(state.close_space(desk2));
    });
    let answer = read.join().unwrap();
    assert!(matches!(&answer, Err(ede_protocol::Error::Failed(f)) if f.code == ede_protocol::ErrorCode::Unavailable), "{answer:?}");
    common::space::spaces_settled(&h);
    // The hung client lets go of its pipe only now.
    b.state.held.clear();
    std::thread::sleep(Duration::from_millis(ede_protocol::CLIPBOARD_READ_MS + 300));

    const KEY_A: u32 = 30;
    common::space::person_key(&h, KEY_A);
    wait_for(&mut a, |a| a.state.keys == [KEY_A]);
    common::space::assert_person_untouched(&h, &before);
    drop((x1, x2));
}

/// #EDE-138: each Xwayland admits its own cookie alone. A client without
/// one, or with the other side's, is refused by the person's server and by
/// a space's; each side's programs are given their own file as
/// `XAUTHORITY` (the person's children by the compositor, a space's
/// launches by the space).
#[test]
fn each_xwayland_admits_only_its_own_cookie() {
    use ede_compositor::desk::DeskId;
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let Some(_person) = xwayland(&h) else { return };
    let person_display = h.with(|state| state.xdisplay).unwrap();
    let person_file = h.with(|state| state.xauthority(DeskId::PERSON)).expect("the person's X authority");
    let (mut agent, space, display) = space_with_x11(&h, dir.path(), "run-1");
    let desk = space_desk(&space);
    let space_file = h.with(move |state| state.xauthority(desk)).expect("the space's X authority");
    assert_ne!(person_file, space_file);
    assert_ne!(ede_compositor::xauth::cookie_for(&person_file, person_display), ede_compositor::xauth::cookie_for(&space_file, display));

    for (number, own, other, whose) in [(person_display, &person_file, &space_file, "the person's"), (display, &space_file, &person_file, "the space's")] {
        assert!(common::x11_connect(number, Some(own)).is_ok(), "{whose} server refused its own cookie");
        assert!(common::x11_connect(number, None).is_err(), "{whose} server admitted a client without a cookie");
        // The other side's file has no entry for this display, and its
        // cookie is not this server's.
        let theirs = ede_compositor::xauth::cookie_for(other, if number == display { person_display } else { display }).unwrap();
        let stream = x11rb::rust_connection::DefaultStream::connect(&x11rb::reexports::x11rb_protocol::parse_display::ConnectAddress::Socket(format!("/tmp/.X11-unix/X{number}").into())).unwrap().0;
        let with_theirs = x11rb::rust_connection::RustConnection::connect_to_stream_with_auth_info(stream, 0, ede_compositor::xauth::MIT_MAGIC_COOKIE.to_vec(), theirs);
        assert!(with_theirs.is_err(), "{whose} server admitted the other side's cookie");
    }

    // What each side's programs are given.
    let out = dir.path().join("space.env");
    agent.launch(vec!["sh".into(), "-c".into(), format!("env > {0}.tmp && mv {0}.tmp {0}; exec sleep 60", out.display())], Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !out.exists() {
        assert!(Instant::now() < deadline, "the launch never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    let env = std::fs::read_to_string(&out).unwrap();
    assert!(env.lines().any(|line| line == format!("XAUTHORITY={}", space_file.display())), "{env}");
    assert!(env.lines().any(|line| line == format!("DISPLAY=:{display}")), "{env}");
    let person_env: Vec<(String, String)> = h.with(|state| {
        let cmd = state.client_command(&["true".to_string()]).unwrap();
        cmd.get_envs().filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string()))).collect()
    });
    assert!(person_env.contains(&("XAUTHORITY".to_string(), person_file.to_string_lossy().into_owned())), "{person_env:?}");
    assert!(person_env.contains(&("DISPLAY".to_string(), format!(":{person_display}"))), "{person_env:?}");
}

/// #EDE-138: the person's Xwayland that dies starts again on the same
/// display with the same cookie file: its windows were let go of, an X11
/// app that connects afterwards with the cookie works, one without is
/// refused, and what the session was given (`DISPLAY`, `XAUTHORITY`)
/// stays right.
#[test]
fn the_persons_xwayland_starts_again_when_it_dies() {
    use ede_compositor::desk::DeskId;
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let Some(conn) = xwayland(&h) else { return };
    let display = h.with(|state| state.xdisplay).unwrap();
    let file = h.with(|state| state.xauthority(DeskId::PERSON)).unwrap();
    let before = h.with(|state| state.xwm.as_ref().map(|wm| wm.id())).unwrap();
    let win = x11_window(&conn, 0x00ff_0000);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| state.desk.space.elements().any(|w| w.x11_surface().is_some_and(|x| x.window_id() == win))) {
        assert!(Instant::now() < deadline, "the X11 window never mapped");
        std::thread::sleep(Duration::from_millis(20));
    }

    let pid = xwayland_pid(display).expect("the person's Xwayland process");
    assert!(std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).status().unwrap().success());
    drop(conn);
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.with(move |state| state.xwm.as_ref().map(|wm| wm.id()).is_none_or(|id| id == before)) {
        assert!(Instant::now() < deadline, "the person's Xwayland never started again");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(h.with(|state| state.desk.space.elements().all(|w| w.x11_surface().is_none())), "a window of the dead server stayed");
    assert_eq!(h.with(|state| state.xdisplay), Some(display), "another display");
    assert_eq!(h.with(|state| state.xauthority(DeskId::PERSON)).as_deref(), Some(file.as_path()), "another cookie file");
    assert_ne!(xwayland_pid(display), Some(pid));

    assert!(common::x11_connect(display, None).is_err(), "the new server admitted a client without a cookie");
    let again = common::x11_connect(display, Some(&file)).expect("the new server refused the cookie");
    let win = x11_window(&again, 0x0000_ff00);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(move |state| state.desk.space.elements().any(|w| w.x11_surface().is_some_and(|x| x.window_id() == win))) {
        assert!(Instant::now() < deadline, "an X11 window on the new server never mapped");
        std::thread::sleep(Duration::from_millis(20));
    }
}
