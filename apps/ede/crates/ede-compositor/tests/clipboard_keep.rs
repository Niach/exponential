//! The compositor keeps the clipboard: a client copies and quits at once
//! (Signal's "Copy error and quit"), and the next client still pastes it.
//! A copy marked secret leaves with its client.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, desk::DeskId, Config};

const BLUE: u32 = 0xff00_00ff;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

/// A focused client with a window and a data device.
fn app(h: &Harness, title: &str) -> (Session, wayland_client::protocol::wl_data_device::WlDataDevice) {
    let mut app = Session::connect(h.connect());
    app.keyboard();
    // Before the focus, as a real client does: the focus offers it the selection.
    let device = app.data_device();
    let t = app.toplevel(title);
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    (app, device)
}

fn kept(h: &Harness) -> Option<Vec<String>> {
    h.with(|state| state.desk.clipboard.kept())
}

#[test]
fn a_copy_outlives_the_client_that_made_it() {
    let h = harness();
    let (mut signal, device) = app(&h, "signal");
    let png = [0x89u8, b'P', b'N', b'G', 1, 2, 3];
    let _source = signal.set_selection(&device, &[("text/plain;charset=utf-8", Some("database error ü".as_bytes())), ("application/x-private", Some(b"x")), ("image/png", Some(&png)), ("image/bmp", Some(b"BM"))]);
    // Its text types and its first picture, nothing else.
    wait_for(&mut signal, |_| kept(&h).is_some());
    assert_eq!(kept(&h), Some(vec!["text/plain;charset=utf-8".to_string(), "image/png".to_string()]));
    let copied = h.with(|state| state.desk.clipboard.serial);

    // It quits at once.
    drop(signal);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h.with(|state| matches!(state.desk.clipboard.owner, ede_compositor::clipboard::Owner::Ours(_))) {
        assert!(Instant::now() < deadline, "the kept copy never took over");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.with(|state| state.desk.clipboard.serial) > copied);

    // The next client pastes it.
    let (mut editor, _device) = app(&h, "editor");
    wait_for(&mut editor, |c| c.selection_types().is_some());
    assert_eq!(editor.selection_types(), Some(vec!["text/plain;charset=utf-8".to_string(), "image/png".to_string()]));
    assert_eq!(editor.read_selection("text/plain;charset=utf-8"), "database error ü".as_bytes());
    assert_eq!(editor.read_selection("image/png"), png);
    // And ePilot reads it too.
    assert_eq!(h.ipc().clipboard_text().unwrap().text.as_deref(), Some("database error ü"));
}

/// The client quits while its copy is still read (a type it never
/// answers): what was read takes over once the read ends.
#[test]
fn a_client_that_quits_mid_read_leaves_what_was_read() {
    let h = harness();
    let (mut app_a, device) = app(&h, "quitter");
    let _source = app_a.set_selection(&device, &[("text/plain", Some(b"first".as_slice())), ("text/html", None)]);
    wait_for(&mut app_a, |c| !c.state.held.is_empty());
    assert_eq!(kept(&h), None, "still reading text/html");
    drop(app_a);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h.with(|state| matches!(state.desk.clipboard.owner, ede_compositor::clipboard::Owner::Ours(_))) {
        assert!(Instant::now() < deadline, "the kept copy never took over");
        std::thread::sleep(Duration::from_millis(10));
    }
    let (mut b, _device) = app(&h, "editor");
    wait_for(&mut b, |c| c.selection_types().is_some());
    assert_eq!(b.selection_types(), Some(vec!["text/plain".to_string()]));
    assert_eq!(b.read_selection("text/plain"), b"first");
}

/// A password manager's copy (marked secret) is never kept: it goes with
/// its client, as it always did.
#[test]
fn a_secret_copy_is_not_kept() {
    let h = harness();
    let (mut vault, device) = app(&h, "vault");
    let _source = vault.set_selection(&device, &[("text/plain", Some(b"hunter2".as_slice())), (ede_protocol::CLIPBOARD_SECRET_HINT, Some(b"secret"))]);
    vault.roundtrip();
    std::thread::sleep(Duration::from_millis(100));
    vault.roundtrip();
    assert_eq!(kept(&h), None);
    drop(vault);
    let (mut b, _device) = app(&h, "editor");
    b.roundtrip();
    std::thread::sleep(Duration::from_millis(100));
    b.roundtrip();
    assert_eq!(b.selection_types(), None);
    assert!(h.with(|state| matches!(state.desk.clipboard.owner, ede_compositor::clipboard::Owner::Empty)));
    assert_eq!(h.with(|state| state.desk.clipboard.pending()), 0);
}

/// The same for an X11 client: it owns CLIPBOARD, eDE reads its copy
/// through the XWM, the client quits, and a Wayland client pastes.
/// Xwayland must be installed; skipped otherwise.
#[cfg(feature = "xwayland")]
#[test]
fn an_x11_copy_outlives_its_client() {
    use x11rb::{
        connection::Connection,
        protocol::{
            xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent, WindowClass, SELECTION_NOTIFY_EVENT},
            Event,
        },
        wrapper::ConnectionExt as _,
        CURRENT_TIME,
    };
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let h = harness();
    h.with(|ede| ede.start_xwayland());
    let deadline = Instant::now() + Duration::from_secs(30);
    let display = loop {
        if let Some(n) = h.with(|ede| ede.xdisplay) {
            break n;
        }
        assert!(Instant::now() < deadline, "Xwayland never became ready");
        std::thread::sleep(Duration::from_millis(20));
    };
    let conn = common::x11_connect(display, h.with(|ede| ede.xauthority(DeskId::PERSON)).as_deref()).expect("connect to Xwayland");
    let root = conn.setup().roots[0].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 200, 100, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE))
        .unwrap();
    conn.map_window(win).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.with(|ede| ede.focused_window(DeskId::PERSON).is_some_and(|w| w.x11_surface().is_some())) {
        assert!(Instant::now() < deadline, "the X11 window never took focus");
        std::thread::sleep(Duration::from_millis(20));
    }
    let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom;
    let (clipboard, utf8, targets) = (atom("CLIPBOARD"), atom("UTF8_STRING"), atom("TARGETS"));

    conn.set_selection_owner(win, clipboard, CURRENT_TIME).unwrap();
    conn.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while kept(&h).is_none() {
        assert!(Instant::now() < deadline, "eDE never kept the X11 copy");
        serve_x11(&conn, targets, utf8, b"from x11");
        std::thread::sleep(Duration::from_millis(10));
    }
    let mimes = kept(&h).unwrap();

    // The X11 client quits.
    drop(conn);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !h.with(|state| matches!(state.desk.clipboard.owner, ede_compositor::clipboard::Owner::Ours(_))) {
        assert!(Instant::now() < deadline, "the kept copy never took over");
        std::thread::sleep(Duration::from_millis(10));
    }
    let (mut editor, _device) = app(&h, "editor");
    wait_for(&mut editor, |c| c.selection_types().is_some());
    assert_eq!(editor.selection_types(), Some(mimes.clone()));
    assert_eq!(editor.read_selection(&mimes[0]), b"from x11");

    /// Answers pending SelectionRequests: TARGETS and UTF8_STRING.
    fn serve_x11(conn: &impl Connection, targets: u32, utf8: u32, text: &[u8]) {
        while let Some(event) = conn.poll_for_event().unwrap() {
            let Event::SelectionRequest(r) = event else { continue };
            let property = if r.target == targets {
                conn.change_property32(PropMode::REPLACE, r.requestor, r.property, AtomEnum::ATOM, &[targets, utf8]).unwrap();
                r.property
            } else if r.target == utf8 {
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
}
