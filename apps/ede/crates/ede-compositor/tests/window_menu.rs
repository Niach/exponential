//! #EDE-253 headless: Cinnamon's window menu (Alt+Space, Alt+middle-click)
//! and what its entries ask of the compositor (Always on Top, Always on
//! Visible Workspace, Move Titlebar Onscreen), and the keyboard applet's
//! layouts.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, state::window_state, Config};
use ede_protocol::{events, WindowAction, WindowMenu};
use smithay::backend::input::{ButtonState, KeyState};

const BLUE: u32 = 0xff00_00ff;
const RED: u32 = 0xffff_0000;
const KEY_LEFTALT: u32 = 56;
const KEY_SPACE: u32 = 57;
const BTN_MIDDLE: u32 = 0x112;

fn harness(config: Config) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(config, &[(800, 600)])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

fn key(h: &Harness, code: u32, pressed: bool) {
    h.with(move |ede| ede.keyboard_key((code + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, 0));
}

fn shell(h: &Harness) -> ede_protocol::Client {
    let mut shell = h.ipc();
    shell.subscribe().unwrap();
    shell.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    shell
}

fn next_menu(shell: &mut ede_protocol::Client) -> WindowMenu {
    loop {
        let e = shell.next_event().unwrap();
        if e.kind == events::WINDOW_MENU {
            return serde_json::from_value(e.data).unwrap();
        }
    }
}

/// Alt+Space opens the focused window's menu at its corner; Alt+middle at
/// the pointer, and the window never sees that click.
#[test]
fn alt_space_and_alt_middle_click_ask_for_the_window_menu() {
    let h = harness(Config::default());
    let mut shell = shell(&h);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    app.pointer();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let (x, y) = h.with(|ede| {
        let w = ede.desk.space.elements().last().unwrap().clone();
        let g = ede.desk.space.element_geometry(&w).unwrap();
        (g.loc.x, g.loc.y)
    });

    key(&h, KEY_LEFTALT, true);
    key(&h, KEY_SPACE, true);
    key(&h, KEY_SPACE, false);
    key(&h, KEY_LEFTALT, false);
    let menu = next_menu(&mut shell);
    assert_eq!((menu.x, menu.y), (x, y), "at the window's corner");
    assert!(!menu.maximized && !menu.above && !menu.sticky && !menu.offscreen && menu.resizable);
    assert!(menu.workspaces > 1, "Cinnamon lists four");

    let at = ((x + 40) as f64, (y + 30) as f64);
    h.with(move |ede| {
        ede.person_pointer_motion_to(at.into(), 0);
    });
    app.roundtrip();
    let buttons_before = app.state.buttons.len();
    key(&h, KEY_LEFTALT, true);
    h.with(|ede| ede.person_pointer_button(BTN_MIDDLE, ButtonState::Pressed, 1));
    h.with(|ede| ede.person_pointer_button(BTN_MIDDLE, ButtonState::Released, 2));
    key(&h, KEY_LEFTALT, false);
    let menu = next_menu(&mut shell);
    assert_eq!((menu.x, menu.y), (x + 40, y + 30), "at the pointer");
    app.roundtrip();
    assert_eq!(app.state.buttons.len(), buttons_before, "the app saw neither the press nor the release");
}

/// Always on Top keeps a window in front of one focused after it; Always
/// on Visible Workspace rides along on a switch.
#[test]
fn always_on_top_and_on_visible_workspace() {
    let h = harness(Config::default());
    let mut shell = shell(&h);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let a = app.toplevel("a");
    app.draw(&a, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let first = h.with(|ede| ede_compositor::ipc::window_id(ede.desk.space.elements().last().unwrap()));
    shell.window_action(first.clone(), WindowAction::Above).unwrap();
    let b = app.toplevel("b");
    app.draw(&b, 300, 200, RED);
    wait_for(&mut app, |c| c.state.toplevels.len() == 2);
    let top = |h: &Harness| h.with(|ede| ede.desk.space.elements().last().map(ede_compositor::ipc::window_id));
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(|ede| ede.desk.space.elements().count()) < 2 {
        assert!(Instant::now() < deadline, "the second window maps");
        app.roundtrip();
    }
    assert_eq!(top(&h), Some(first.clone()), "the window on top stays in front of the new one");
    shell.window_action(first.clone(), WindowAction::NotAbove).unwrap();
    assert!(!h.with(|ede| ede.desk.space.elements().any(|w| window_state(w).above)));

    shell.window_action(first.clone(), WindowAction::Stick).unwrap();
    shell.switch_workspace(2).unwrap();
    let on_screen = h.with(|ede| ede.desk.space.elements().map(ede_compositor::ipc::window_id).collect::<Vec<_>>());
    assert_eq!(on_screen, vec![first.clone()], "only the sticky window came along");
    let ws = h.with(|ede| ede.desk.space.elements().map(|w| window_state(w).workspace).next());
    assert_eq!(ws, Some(2));
}

/// Move Titlebar Onscreen: a window whose bar left every output comes back
/// with its bar on the nearest one.
#[test]
fn move_titlebar_onscreen() {
    let h = harness(Config::default());
    let mut shell = shell(&h);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.is_some());
    let id = h.with(|ede| ede_compositor::ipc::window_id(ede.desk.space.elements().last().unwrap()));
    shell.move_window(id.clone(), 100, -400).unwrap();
    let off = h.with(|ede| {
        let w = ede.desk.space.elements().last().unwrap().clone();
        ede.titlebar_offscreen(&w)
    });
    assert!(off, "the bar is above the output");
    shell.window_action(id, WindowAction::MoveTitlebarOnscreen).unwrap();
    let (off, y) = h.with(|ede| {
        let w = ede.desk.space.elements().last().unwrap().clone();
        (ede.titlebar_offscreen(&w), ede.framed_geometry(&w).unwrap().loc.y)
    });
    assert!(!off && y >= 0, "back on the output ({y})");
}

/// The keyboard applet's layouts: listed, picked, announced.
#[test]
fn keyboard_layouts_are_listed_and_set() {
    let mut config = Config::default();
    config.keyboard.layout = "us,de".into();
    config.keyboard.variant = String::new();
    let h = harness(config);
    let mut shell = shell(&h);
    let now = shell.keyboard_layouts().unwrap();
    assert_eq!((now.index, now.layouts.len()), (0, 2));
    assert_eq!(now.short, vec!["us".to_string(), "de".to_string()]);
    let set = shell.set_keyboard_layout(1).unwrap();
    assert_eq!(set.index, 1);
    loop {
        let e = shell.next_event().unwrap();
        if e.kind == events::KEYBOARD_LAYOUT {
            assert_eq!(e.data["index"], 1);
            break;
        }
    }
    assert!(shell.set_keyboard_layout(5).is_err(), "no such layout");
}
