//! #EDE-213 headless: a sealed prompt (polkit, consent, share: a layer of
//! `exponential-sealed`) is drawn above every other layer and every other
//! layer's popups, whatever layer the shell asks for and whichever mapped
//! first: the taskbar's Agent spaces list never covers Allow. The click on
//! the prompt is the prompt's, and Escape closes the list, which never
//! takes the keyboard, while the prompt holds it.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use smithay::backend::input::{ButtonState, KeyState};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer,
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity},
};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const GREEN: u32 = 0xff00_ff00;
const YELLOW: u32 = 0xffff_ff00;
// evdev codes; xkb = evdev + 8
const KEY_A: u32 = 30;
const KEY_ESC: u32 = 1;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, &[(1280, 800)])
}

#[track_caller]
fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn press(h: &Harness, code: u32) {
    h.with(move |ede| {
        ede.keyboard_key((code + 8).into(), KeyState::Pressed, 0);
        ede.keyboard_key((code + 8).into(), KeyState::Released, 0);
    });
}

fn click(h: &Harness, x: f64, y: f64) {
    h.with(move |ede| {
        ede.pointer_motion_to((x, y).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
}

/// The `hotkey` events the shell heard, until none comes for a moment.
fn hotkeys(shell: &mut ede_protocol::Client) -> Vec<String> {
    let mut names = Vec::new();
    while let Ok(e) = shell.next_event() {
        if e.kind == ede_protocol::events::HOTKEY {
            names.push(e.data["name"].as_str().unwrap().to_string());
        }
    }
    names
}

/// The shell: its ede-protocol subscription, a keyboard and a pointer, and
/// its 48 px taskbar (a top layer that never takes the keyboard) along the
/// bottom of the 1280×800 output: y 752..800.
fn shell(h: &Harness) -> (ede_protocol::Client, Session, common::Layer) {
    let mut ipc = h.ipc();
    ipc.subscribe().unwrap();
    ipc.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut shell = Session::connect(h.connect());
    shell.keyboard();
    let _ = shell.pointer();
    let taskbar = shell.layer("exponential-shell:taskbar", Anchor::Bottom | Anchor::Left | Anchor::Right, (1280, 48), 48);
    shell.draw_layer(&taskbar, RED);
    wait_for(&mut shell, |_| h.capture(0).at(640, 776) == RED);
    (ipc, shell, taskbar)
}

/// The Agent spaces list: a 400×400 popup of the taskbar that does not
/// grab, at x 440..840, y 252..652: over the whole prompt.
fn spaces_list(h: &Harness, shell: &mut Session, taskbar: &common::Layer) -> common::Popup {
    let pid = std::process::id();
    h.with(move |ede| ede.shell_stand_in = Some(pid));
    let list = shell.named_layer_popup(taskbar, (440, -500), (400, 400), false, GREEN, Some(ede_protocol::SPACE_LIST_NAMESPACE));
    h.with(|ede| ede.shell_stand_in = None);
    list
}

/// What `exponential-sealed consent` maps on `on`: an Exclusive 200×100
/// layer, centred in the area the taskbar leaves (y 0..752): (540, 326) to
/// (740, 426). The test's own process plays
/// `exponential-sealed` while the surface maps.
fn consent(h: &Harness, on: Layer) -> (Session, common::Layer) {
    let mut sealed = Session::connect(h.connect());
    sealed.keyboard();
    let _ = sealed.pointer();
    let pid = std::process::id();
    h.with(move |ede| ede.sealed_stand_in = Some(pid));
    let l = sealed.keyboard_layer("consent", on, (200, 100), KeyboardInteractivity::Exclusive);
    h.with(|ede| ede.sealed_stand_in = None);
    sealed.draw_layer(&l, BLUE);
    wait_for(&mut sealed, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    (sealed, l)
}

/// The prompt on top with the list around it, its click its own, a click
/// on the list beside it the shell's.
fn assert_prompt_on_top(h: &Harness, shell: &mut Session, sealed: &mut Session) {
    wait_for(sealed, |_| h.capture(0).at(640, 376) == BLUE);
    let screen = h.capture(0);
    assert_eq!((screen.at(540, 326), screen.at(739, 425)), (BLUE, BLUE), "the list covers the prompt");
    assert_eq!((screen.at(460, 300), screen.at(820, 600)), (GREEN, GREEN), "the list shows around the prompt");

    sealed.state.press_serial = None;
    shell.state.press_serial = None;
    click(h, 640.0, 376.0);
    wait_for(sealed, |c| c.state.press_serial.is_some());
    shell.roundtrip();
    assert_eq!(shell.state.press_serial, None, "the list took the click on Allow");

    sealed.state.press_serial = None;
    click(h, 460.0, 300.0);
    wait_for(shell, |c| c.state.press_serial.is_some());
    sealed.roundtrip();
    assert_eq!(sealed.state.press_serial, None, "the prompt took a click on the list beside it");
}

#[test]
fn the_consent_prompt_is_drawn_and_clicked_above_the_agent_spaces_list_opened_before_it() {
    let h = harness();
    let (mut ipc, mut shell, taskbar) = shell(&h);
    let list = spaces_list(&h, &mut shell, &taskbar);
    wait_for(&mut shell, |_| h.capture(0).at(640, 376) == GREEN);

    let (mut sealed, l) = consent(&h, Layer::Overlay);
    assert_prompt_on_top(&h, &mut shell, &mut sealed);
    let _ = hotkeys(&mut ipc);

    // The keys are the prompt's; Escape is the shell's, to close its list
    // with, though the list never had the keyboard. (The click on the list
    // above gave it the keyboard, as a menu over a prompt takes it,
    // #EDE-86: the prompt takes it back first.)
    click(&h, 640.0, 376.0);
    wait_for(&mut sealed, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    let _ = hotkeys(&mut ipc);
    press(&h, KEY_A);
    wait_for(&mut sealed, |c| c.state.keys == [KEY_A]);
    press(&h, KEY_ESC);
    assert_eq!(hotkeys(&mut ipc), ["popups.dismiss"]);
    sealed.roundtrip();
    assert_eq!(sealed.state.keys, [KEY_A], "Escape went to the prompt, not to the list");

    // Closed: Escape is the prompt's again.
    shell.close_popup(list);
    wait_for(&mut shell, |_| h.capture(0).at(460, 300) != GREEN);
    press(&h, KEY_ESC);
    wait_for(&mut sealed, |c| c.state.keys == [KEY_A, KEY_ESC]);
    assert!(hotkeys(&mut ipc).is_empty(), "no shell popup open: nothing to dismiss");
}

/// The other order, and the prompt on the top layer while the shell asks
/// for the overlay layer itself: the prompt still stays on top.
#[test]
fn the_consent_prompt_stays_above_a_list_opened_after_it_and_above_the_shells_overlay_layers() {
    let h = harness();
    let (_ipc, mut shell, taskbar) = shell(&h);
    let (mut sealed, _l) = consent(&h, Layer::Top);

    let list = spaces_list(&h, &mut shell, &taskbar);
    wait_for(&mut shell, |_| h.capture(0).at(460, 300) == GREEN);
    assert_prompt_on_top(&h, &mut shell, &mut sealed);

    // A shell overlay (an OSD) over the prompt, mapped after it.
    let osd = shell.keyboard_layer("exponential-shell:osd", Layer::Overlay, (600, 300), KeyboardInteractivity::None);
    shell.draw_layer(&osd, YELLOW);
    wait_for(&mut shell, |_| h.capture(0).at(400, 300) == YELLOW);
    assert_eq!(h.capture(0).at(640, 376), BLUE, "a shell overlay covers the prompt");

    shell.close_popup(list);
    wait_for(&mut shell, |_| h.capture(0).at(460, 270) == YELLOW);
    assert_eq!(h.capture(0).at(640, 376), BLUE);
}
