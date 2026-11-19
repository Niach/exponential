//! #EDE-126: the acceptance test of agent spaces. The person types in
//! their window while an agent works in its space, both for 200 rounds,
//! and neither hears a thing of the other: not a key, a modifier, a
//! pointer event, a clipboard, a window event or a pixel; not while the
//! session is locked, and not after the space is gone.

mod common;

use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use common::{
    space::{assert_person_untouched, events_until_mark, person_key, person_key_state, person_snapshot, spaces_settled, start_with_spaces, wait_for},
    Session,
};
use ede_compositor::desk::DeskId;
use ede_protocol::{Capability, CaptureTarget, ClipboardItem, Error, ErrorCode, InputEvent, Request, SessionId};
use smithay::backend::input::ButtonState;

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;
const KEY_LEFTSHIFT: u32 = 42;
const BTN_LEFT: u32 = 0x110;
const TEXT: &str = "text/plain";
/// The person's keys (a, s, d, f) and the agent's (j, k, l): disjoint.
const P: [u32; 4] = [30, 31, 32, 33];
const G: [u32; 3] = [36, 37, 38];
/// The keysyms of J, K, L: each takes the space's own Shift.
const G_UPPER: [u32; 3] = [0x4a, 0x4b, 0x4c];
const ROUNDS: usize = 200;

fn code<T: std::fmt::Debug>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn the_person_types_while_an_agent_works_in_its_space() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);

    // A: the person's window, focused, with their keyboard, pointer and copy.
    let mut a = Session::connect(h.connect());
    let window_a = a.toplevel("a");
    a.draw(&window_a, 300, 200, BLUE);
    a.keyboard();
    a.pointer();
    h.with(|state| {
        let window = state.desk.space.elements().next().cloned();
        state.focus(DeskId::PERSON, window);
    });
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());
    let device_a = a.data_device();
    let _copied = a.set_selection(&device_a, &[(TEXT, Some(b"person"))]);
    let mut root = h.ipc();
    let at_a = root.windows().unwrap().iter().map(|w| (w.x, w.y)).next().unwrap();
    let inside_a = move |dx: i32, dy: i32| (f64::from(at_a.0 + 20 + dx), f64::from(at_a.1 + 20 + dy));
    h.with(move |state| state.person_pointer_motion_to(inside_a(0, 0).into(), 0));
    wait_for(&mut a, |a| a.state.pointer_enters == 1);

    // The agent: a space grant for run-1, its space, and B inside it.
    root.subscribe().unwrap();
    let grant = root.mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token.clone()).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let socket = std::path::PathBuf::from(&space.wayland_display);
    let mut b = Session::connect(UnixStream::connect(&socket).expect("the space's socket"));
    let window_b = b.toplevel("b");
    b.draw(&window_b, 640, 480, RED);
    b.keyboard();
    b.pointer();
    wait_for(&mut b, |b| b.state.keyboard_focus.is_some());
    let _device_b = b.data_device();

    let mut before = person_snapshot(&h);
    let heard = (a.state.modifiers.len(), a.state.pointer_motions, a.state.keymaps);
    let key = |agent: &mut ede_protocol::Client, event: fn(bool) -> InputEvent| {
        agent.input(event(true)).unwrap();
        agent.input(event(false)).unwrap();
    };

    // 200 rounds. The person holds Shift through the first hundred.
    let mut held = false;
    person_key_state(&h, KEY_LEFTSHIFT, true);
    for i in 0..ROUNDS {
        if i == 100 {
            person_key_state(&h, KEY_LEFTSHIFT, false);
            agent.clipboard_set(vec![ClipboardItem { mime: TEXT.into(), data: b"agent".to_vec() }], false).unwrap();
        }
        person_key(&h, P[i % 4]);
        // The agent's key: raw on even rounds, on odd ones the upper-case
        // letter's keysym, which holds the space's own Shift around it.
        match (i % 2, i % 3) {
            (0, 0) => key(&mut agent, |pressed| InputEvent::Key { key: G[0], pressed }),
            (0, 1) => key(&mut agent, |pressed| InputEvent::Key { key: G[1], pressed }),
            (0, _) => key(&mut agent, |pressed| InputEvent::Key { key: G[2], pressed }),
            (_, 0) => key(&mut agent, |pressed| InputEvent::Keysym { keysym: G_UPPER[0], pressed }),
            (_, 1) => key(&mut agent, |pressed| InputEvent::Keysym { keysym: G_UPPER[1], pressed }),
            (_, _) => key(&mut agent, |pressed| InputEvent::Keysym { keysym: G_UPPER[2], pressed }),
        }
        if i % 10 == 0 {
            agent.input(InputEvent::PointerMotion { x: 100.0 + i as f64, y: 100.0 }).unwrap();
            let step = i as i32 / 10;
            h.with(move |state| state.person_pointer_motion_to(inside_a(1 + step, 1).into(), 0));
        }
        if i == 50 {
            key(&mut agent, |pressed| InputEvent::PointerButton { button: BTN_LEFT, pressed });
        }
        if i == 60 {
            h.with(|state| {
                state.person_pointer_button(BTN_LEFT, ButtonState::Pressed, 0);
                state.person_pointer_button(BTN_LEFT, ButtonState::Released, 0);
            });
            // The person's own click is theirs to hear of.
            let clicked = events_until_mark(&h, &mut root, "test.click");
            assert!(clicked.iter().all(|kind| kind == ede_protocol::events::WINDOW_FOCUSED), "{clicked:?}");
        }
        held |= h.with(|state| state.person_input_held());
        // B reads what it is sent, as a client does.
        b.roundtrip();
    }

    // A heard the person and only the person.
    wait_for(&mut a, |a| a.state.key_releases.len() == ROUNDS + 1);
    let typed: Vec<u32> = (0..ROUNDS).map(|i| P[i % 4]).collect();
    assert_eq!(a.state.keys, [&[KEY_LEFTSHIFT][..], &typed[..]].concat());
    assert_eq!(a.state.key_releases, [&typed[..100], &[KEY_LEFTSHIFT][..], &typed[100..]].concat());
    assert_eq!(a.state.modifiers[heard.0..], [1, 0], "A's modifiers are the person's Shift, down and up");
    assert_eq!(a.state.keymaps, heard.2, "A was sent another keymap");
    assert_eq!((a.state.pointer_enters, a.state.pointer_motions - heard.1), (1, 20));
    assert_eq!(a.state.buttons, [(BTN_LEFT, true), (BTN_LEFT, false)]);
    // B heard the agent and only the agent.
    wait_for(&mut b, |b| b.state.key_releases.len() == ROUNDS + 100);
    let driven: Vec<u32> = (0..ROUNDS).map(|i| G[i % 3]).collect();
    assert_eq!(b.state.keys.iter().copied().filter(|key| *key != KEY_LEFTSHIFT).collect::<Vec<_>>(), driven);
    assert_eq!(b.state.keys.iter().filter(|key| **key == KEY_LEFTSHIFT).count(), 100, "the space's own Shift, once per keysym");
    assert!(b.state.pointer_enters >= 1 && b.state.pointer_motions >= 1);
    assert_eq!(b.state.buttons, [(BTN_LEFT, true), (BTN_LEFT, false)]);
    assert_eq!(b.state.keymaps, 1, "the space's keyboard changed its keymap");
    // The person's desk is as it was, but for where they put their pointer.
    before.pointer = inside_a(20, 1);
    assert_person_untouched(&h, &before);
    assert!(!held, "the person's input was held");
    assert_eq!(a.read_selection(TEXT), b"person");
    wait_for(&mut b, |b| b.selection_types().is_some());
    assert_eq!(b.read_selection(TEXT), b"agent");
    let shell = events_until_mark(&h, &mut root, "test.mark");
    assert_eq!(shell, Vec::<String>::new(), "the shell heard of the space");

    // Ten more of the agent's keys and moves with the person still: their
    // idle clock and their cursor do not move.
    let still = h.with(|state| (state.idle.last_activity, format!("{:?}", state.desk.cursor_status)));
    for i in 0..10 {
        key(&mut agent, |pressed| InputEvent::Key { key: G[0], pressed });
        agent.input(InputEvent::PointerMotion { x: 50.0 + f64::from(i), y: 50.0 }).unwrap();
    }
    assert_eq!(h.with(|state| (state.idle.last_activity, format!("{:?}", state.desk.cursor_status))), still);

    // Each sees its own screen.
    let own = agent.capture(CaptureTarget::Output { name: space.output.clone() }).unwrap();
    assert_eq!((own.info.width, own.info.height, own.at(320, 240)), (640, 480, RED));
    let theirs = root.capture(CaptureTarget::Output { name: "HEADLESS-1".into() }).unwrap();
    assert_eq!(theirs.at(at_a.0 + 20, at_a.1 + 20), BLUE);
    assert!(!theirs.pixels.chunks_exact(4).any(|p| u32::from_le_bytes(p.try_into().unwrap()) == RED), "the space shows on the person's output");
    assert_eq!(code(agent.capture(CaptureTarget::Output { name: "HEADLESS-1".into() })), ErrorCode::NotFound);

    // The session locks: the space goes on, the person's screen is black.
    let pid = std::process::id();
    let unlock = h.with(move |state| state.begin_sealed_lock_as(pid));
    assert!(h.with(|state| state.is_locked()));
    wait_for(&mut b, |b| b.state.keys.len() == ROUNDS + 100 + 10);
    key(&mut agent, |pressed| InputEvent::Key { key: G[1], pressed });
    wait_for(&mut b, |b| b.state.keys.last() == Some(&G[1]));
    assert!(b.count_frames(&window_b.surface, Duration::from_millis(300)) > 0, "a locked session stalls the space's frames");
    assert_eq!(agent.capture(CaptureTarget::Output { name: space.output.clone() }).unwrap().at(320, 240), RED);
    assert_eq!(h.capture_shared(0).at(at_a.0 + 20, at_a.1 + 20), BLACK);
    assert_eq!(code(agent.call(Request::HoldInput)), ErrorCode::Forbidden);
    h.ipc_with(unlock).unwrap().unlock().unwrap();
    wait_for(&mut a, |a| a.state.keyboard_focus.is_some());

    // The grant is revoked: the space ends and leaves nothing.
    root.call(Request::RevokeGrant { token: grant.token }).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while b.queue.roundtrip(&mut b.state).is_ok() {
        assert!(Instant::now() < deadline, "the space's client outlived its grant");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!socket.exists() && !socket.parent().unwrap().exists());
    assert_eq!(root.windows().unwrap().len(), 1);
    spaces_settled(&h);
    let typed_a = a.state.keys.len();
    person_key(&h, P[0]);
    wait_for(&mut a, |a| a.state.keys.len() == typed_a + 1);
}
