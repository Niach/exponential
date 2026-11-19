//! #EDE-126 headless: a clipboard per desk. A copy in a space is the
//! space's: the person's serial, owner and paste stay; the agent's own
//! selection there is served to the space's client and counted on the
//! space's clipboard; a read pending when the space closes ends there.

mod common;

use std::os::unix::net::UnixStream;

use common::{
    space::{assert_person_untouched, person_key, person_snapshot, start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{clipboard::Want, desk::DeskId};
use ede_protocol::{ClipboardItem, SessionId};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const KEY_A: u32 = 30;
const TEXT: &str = "text/plain";

fn text(data: &str) -> Vec<ClipboardItem> {
    vec![ClipboardItem { mime: TEXT.into(), data: data.as_bytes().to_vec() }]
}

#[test]
fn a_space_has_a_clipboard_of_its_own() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut a = Session::connect(h.connect());
    let person = a.toplevel("a");
    a.draw(&person, 200, 150, BLUE);
    a.keyboard();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned();
        state.focus(DeskId::PERSON, window);
    });
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let device_a = a.data_device();
    let _copied = a.set_selection(&device_a, &[(TEXT, Some(b"person"))]);

    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    b.keyboard();
    let window = b.toplevel("b");
    b.draw(&window, 640, 480, RED);
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    let device_b = b.data_device();
    let before = person_snapshot(&h);
    let space = move |h: &ede_compositor::backend::headless::Harness| h.with(move |state| (state.desk(desk).clipboard.serial, format!("{:?}", state.desk(desk).clipboard.owner)));
    assert_eq!(space(&h), (0, "Empty".to_string()));

    // The space's client copies: the space's serial and owner move, the
    // person's do not, and the person still pastes their own.
    let _copied_b = b.set_selection(&device_b, &[(TEXT, Some(b"space"))]);
    assert_eq!(space(&h), (1, format!("Client([{TEXT:?}])")));
    assert_person_untouched(&h, &before);
    assert_eq!(a.read_selection(TEXT), b"person");

    // The agent's selection in the space: its client pastes it, and the
    // paste counts on the space's clipboard, not the person's.
    let serial = h.with(move |state| state.clipboard_set(desk, text("agent"), false)).unwrap();
    assert_eq!(serial, 2);
    wait_for(&mut b, |b| b.state.source_cancelled);
    assert_eq!(b.selection_types(), Some(vec![TEXT.to_string()]));
    assert!(!h.with(move |state| state.clipboard_taken(desk, serial)).taken);
    assert_eq!(b.read_selection(TEXT), b"agent");
    assert!(h.with(move |state| state.clipboard_taken(desk, serial)).taken);
    assert_person_untouched(&h, &before);
    a.roundtrip();
    assert_eq!(a.read_selection(TEXT), b"person");

    // And back: a selection set on the person's desk leaves the space's.
    let theirs = h.with(|state| state.clipboard_set(DeskId::PERSON, text("shell"), false)).unwrap();
    assert_eq!(theirs, before.clipboard.0 + 1);
    assert_eq!(space(&h).0, 2);
    b.roundtrip();
    assert_eq!(b.read_selection(TEXT), b"agent");
    let before = person_snapshot(&h);

    // A read of a source that never answers is pending on the space's
    // clipboard, never the person's; the space closes under it. Its pipe
    // and its timer fire later and find no desk.
    let _hung = b.set_selection(&device_b, &[(TEXT, None)]);
    h.with(move |state| {
        assert!(state.clipboard_read(desk, u64::MAX, 1, Want::Text).is_none(), "the source is asked, the reply comes later");
        assert_eq!((state.desk(desk).clipboard.pending(), state.desk.clipboard.pending()), (1, 0));
        assert!(state.close_space(desk));
    });
    std::thread::sleep(std::time::Duration::from_millis(ede_protocol::CLIPBOARD_READ_MS + 200));
    person_key(&h, KEY_A);
    wait_for(&mut a, |a| a.state.keys == [KEY_A]);
    assert_person_untouched(&h, &before);
}
