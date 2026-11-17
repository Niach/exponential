//! #EDE-82 headless: who takes the keyboard. A new window takes it when the
//! focused app opened it or nothing holds it; another app's window opens
//! behind, and no window takes it from an Exclusive prompt.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use smithay::backend::input::{ButtonState, KeyState};
use wayland_protocols::xdg::shell::client::xdg_toplevel::State as XdgState;

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const GREEN: u32 = 0xff00_ff00;
// evdev code; xkb = evdev + 8
const KEY_A: u32 = 30;

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let mut config = Config::default();
    config.windows.panel_margin = 0;
    Harness::start(config, &[(1280, 800)])
}

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

/// The windows' titles, back to front, with which one is focused.
fn stack(h: &Harness) -> Vec<(String, bool)> {
    let mut ipc = h.ipc();
    ipc.windows().unwrap().into_iter().map(|w| (w.title.unwrap_or_default(), w.focused)).collect()
}

/// An app with one focused 400×300 window at (440, 250) and a keyboard.
fn focused_app(h: &Harness, title: &str, argb: u32) -> (Session, common::Toplevel) {
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel(title);
    app.draw(&t, 400, 300, argb);
    // Nothing held the keyboard: the first window takes it.
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&t.surface));
    (app, t)
}

/// Another app (its own connection) maps a 400×300 window.
fn other_app(h: &Harness, title: &str, argb: u32) -> (Session, common::Toplevel) {
    let mut other = Session::connect(h.connect());
    other.keyboard();
    let t = other.toplevel(title);
    other.draw(&t, 400, 300, argb);
    let title = title.to_string();
    wait_for(&mut other, |_| stack(h).iter().any(|(t, _)| *t == title));
    (other, t)
}

#[test]
fn another_apps_window_opens_behind_and_without_the_keyboard() {
    let h = harness();
    let (mut app, _a) = focused_app(&h, "editor", RED);
    let (mut other, _b) = other_app(&h, "updater", BLUE);

    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    other.roundtrip();
    assert!(other.state.keys.is_empty(), "the window nobody asked for got the key");
    assert_eq!(other.state.keyboard_focus, None);
    assert!(!other.state.toplevels[0].states.contains(&XdgState::Activated));
    assert!(app.state.toplevels[0].states.contains(&XdgState::Activated));
    // In the taskbar like any window, unfocused, and behind the editor.
    assert_eq!(stack(&h), [("updater".to_string(), false), ("editor".to_string(), true)]);
    assert_eq!(h.capture(0).at(600, 400), RED, "the focused window stays in front");
}

#[test]
fn a_dialog_of_the_focused_app_takes_the_keyboard() {
    let h = harness();
    let (mut app, _a) = focused_app(&h, "editor", RED);
    let dialog = app.toplevel("save as");
    app.draw(&dialog, 400, 300, BLUE);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&dialog.surface));
    assert_eq!(stack(&h), [("editor".to_string(), false), ("save as".to_string(), true)]);
    assert_eq!(h.capture(0).at(600, 400), BLUE);
}

#[test]
fn no_window_takes_the_keyboard_from_an_exclusive_prompt() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness();
    let (mut app, a) = focused_app(&h, "terminal", RED);
    let mut prompt = Session::connect(h.connect());
    prompt.keyboard();
    // Centred: (540, 350) to (740, 450).
    let l = prompt.keyboard_layer("polkit", Layer::Overlay, (200, 100), KeyboardInteractivity::Exclusive);
    prompt.draw_layer(&l, GREEN);
    wait_for(&mut prompt, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));

    // Another app's window, then one of the app that had the keyboard.
    let (mut other, _b) = other_app(&h, "updater", BLUE);
    let find = app.toplevel("find");
    app.draw(&find, 100, 100, BLUE);
    wait_for(&mut app, |_| stack(&h).len() == 3);
    press(&h, KEY_A);
    wait_for(&mut prompt, |c| c.state.keys == [KEY_A]);

    // A click on a window beside the prompt does not move the keyboard.
    h.with(|ede| {
        ede.pointer_motion_to((450.0, 260.0).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
    press(&h, KEY_A);
    wait_for(&mut prompt, |c| c.state.keys == [KEY_A, KEY_A]);
    app.roundtrip();
    other.roundtrip();
    assert!(app.state.keys.is_empty() && other.state.keys.is_empty(), "a window got what was typed into the prompt");
    assert_eq!(prompt.state.keyboard_focus.as_ref(), Some(&l.surface));

    // Closed: the window that had the keyboard before the prompt has it again.
    l.layer.destroy();
    l.surface.destroy();
    prompt.roundtrip();
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&a.surface));
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
}

const KEY_ESC: u32 = 1;
const KEY_LEFTMETA: u32 = 125;

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

/// The shell: its ede-protocol subscription, and a 400×48 taskbar that
/// never takes the keyboard itself.
fn shell_with_taskbar(h: &Harness) -> (ede_protocol::Client, Session, common::Layer) {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let mut ipc = h.ipc();
    ipc.subscribe().unwrap();
    ipc.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut shell = Session::connect(h.connect());
    shell.keyboard();
    let bar = shell.keyboard_layer("taskbar", Layer::Top, (400, 48), KeyboardInteractivity::None);
    shell.draw_layer(&bar, GREEN);
    (ipc, shell, bar)
}

/// #EDE-86: the start menu is an `xdg_popup` of the taskbar, a layer that
/// never takes the keyboard. Opened on Super while a window is typed into,
/// the menu gets the next keys, Esc included, and the window gets them back
/// when it closes.
#[test]
fn a_layer_popup_opened_on_a_hotkey_takes_the_keys_from_the_window_until_it_closes() {
    let h = harness();
    let (mut app, a) = focused_app(&h, "terminal", RED);
    let (mut ipc, mut shell, bar) = shell_with_taskbar(&h);

    press(&h, KEY_LEFTMETA);
    assert_eq!(hotkeys(&mut ipc), ["menu.toggle"]);
    let menu = shell.layer_popup(&bar, (300, 200), true, BLUE);
    wait_for(&mut shell, |c| c.state.keyboard_focus.as_ref() == Some(&menu.surface));
    assert_eq!(stack(&h), [("terminal".to_string(), false)]);
    // Super itself went to the window, as any modifier does.
    wait_for(&mut app, |c| c.state.keys == [KEY_LEFTMETA]);

    press(&h, KEY_A);
    wait_for(&mut shell, |c| c.state.keys == [KEY_A]);
    // Esc in the menu is the shell's to close it with.
    press(&h, KEY_ESC);
    assert_eq!(hotkeys(&mut ipc), ["popups.dismiss"]);
    app.roundtrip();
    assert_eq!(app.state.keys, [KEY_LEFTMETA], "the window got what was typed into the menu");

    shell.close_popup(menu);
    wait_for(&mut app, |c| c.state.keyboard_focus.as_ref() == Some(&a.surface));
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_LEFTMETA, KEY_A]);
    shell.roundtrip();
    assert_eq!(shell.state.keys, [KEY_A]);
}

/// A popup that does not grab (a hover preview of the taskbar) leaves the
/// keyboard with the window.
#[test]
fn a_layer_popup_that_does_not_grab_leaves_the_keyboard_with_the_window() {
    let h = harness();
    let (mut app, a) = focused_app(&h, "terminal", RED);
    let (_ipc, mut shell, bar) = shell_with_taskbar(&h);
    let _preview = shell.layer_popup(&bar, (300, 200), false, BLUE);
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    shell.roundtrip();
    assert!(shell.state.keys.is_empty());
    assert_eq!(app.state.keyboard_focus.as_ref(), Some(&a.surface));
}

/// #EDE-86: the menu opened over an Exclusive prompt and closed again: the
/// keyboard is with the prompt again, not with the window behind it.
#[test]
fn a_menu_closed_over_an_exclusive_prompt_gives_the_keyboard_back_to_the_prompt() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness();
    let (mut app, _a) = focused_app(&h, "terminal", RED);
    let mut prompt = Session::connect(h.connect());
    prompt.keyboard();
    let l = prompt.keyboard_layer("polkit", Layer::Overlay, (200, 100), KeyboardInteractivity::Exclusive);
    prompt.draw_layer(&l, GREEN);
    wait_for(&mut prompt, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));

    let (_ipc, mut shell, bar) = shell_with_taskbar(&h);
    let menu = shell.layer_popup(&bar, (300, 200), true, BLUE);
    wait_for(&mut shell, |c| c.state.keyboard_focus.as_ref() == Some(&menu.surface));
    shell.close_popup(menu);
    wait_for(&mut prompt, |c| c.state.keyboard_focus.as_ref() == Some(&l.surface));
    press(&h, KEY_A);
    wait_for(&mut prompt, |c| c.state.keys == [KEY_A]);
    app.roundtrip();
    assert!(app.state.keys.is_empty(), "the window behind the prompt got the key");
}

/// A left click at `(x, y)`.
fn click(h: &Harness, x: f64, y: f64) {
    h.with(move |ede| {
        ede.pointer_motion_to((x, y).into(), 0);
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
}

/// #EXP-1912: the computer-use banner on eDE is an overlay layer that asks
/// for no keyboard and has an empty input region: it shows above the window
/// the agent drives, which keeps the keyboard and gets the click under it.
#[test]
fn a_banner_layer_takes_neither_the_keyboard_nor_the_click_under_it() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness();
    let (mut app, a) = focused_app(&h, "browser", RED);
    app.pointer();
    let mut banner = Session::connect(h.connect());
    banner.keyboard();
    banner.pointer();
    // Centred: (540, 350) to (740, 450), over the window.
    let l = banner.keyboard_layer("exponential-overlay:computer-use-banner", Layer::Overlay, (200, 100), KeyboardInteractivity::None);
    banner.no_input(&l.surface);
    banner.draw_layer(&l, GREEN);
    wait_for(&mut banner, |_| h.capture(0).at(640, 400) == GREEN);

    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    click(&h, 640.0, 400.0);
    wait_for(&mut app, |c| c.state.press_serial.is_some());
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A, KEY_A]);
    banner.roundtrip();
    assert_eq!(banner.state.press_serial, None, "the banner took the click meant for the window under it");
    assert!(banner.state.keys.is_empty() && banner.state.keyboard_focus.is_none(), "the banner took the keyboard");
    assert_eq!(app.state.keyboard_focus.as_ref(), Some(&a.surface));
    assert!(app.state.toplevels[0].states.contains(&XdgState::Activated));
    assert_eq!(stack(&h), [("browser".to_string(), true)], "the banner is no window: the taskbar never lists it");
    assert_eq!(h.capture(0).at(640, 400), GREEN, "the banner stays above the window");
}

/// #EXP-1912: the push-to-talk capsule and the call pill are overlay layers
/// that ask for no keyboard but keep their clicks (Stop, Mute, End): the
/// window being typed into keeps the keyboard through a click on them.
#[test]
fn a_capsule_layer_takes_its_click_and_leaves_the_keyboard_with_the_window() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::KeyboardInteractivity};
    let h = harness();
    let (mut app, a) = focused_app(&h, "editor", RED);
    app.pointer();
    let mut capsule = Session::connect(h.connect());
    capsule.keyboard();
    capsule.pointer();
    let l = capsule.keyboard_layer("exponential-overlay:push-to-talk", Layer::Overlay, (200, 100), KeyboardInteractivity::None);
    capsule.draw_layer(&l, GREEN);
    wait_for(&mut capsule, |_| h.capture(0).at(640, 400) == GREEN);

    click(&h, 640.0, 400.0);
    wait_for(&mut capsule, |c| c.state.press_serial.is_some());
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    capsule.roundtrip();
    assert_eq!(app.state.press_serial, None, "the window under the capsule got the capsule's click");
    assert!(capsule.state.keys.is_empty() && capsule.state.keyboard_focus.is_none(), "the capsule took the keyboard");
    assert_eq!(app.state.keyboard_focus.as_ref(), Some(&a.surface));
    assert!(app.state.toplevels[0].states.contains(&XdgState::Activated));
}

/// #EXP-1912: the call pill floats inside an overlay layer over the whole
/// output whose input region is the pill alone: a click on the pill is the
/// pill's, a click anywhere else is the window's under the layer.
#[test]
fn a_click_beside_the_pill_in_its_layer_reaches_the_window_under_it() {
    use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity}};
    let h = harness();
    let (mut app, a) = focused_app(&h, "editor", RED);
    app.pointer();
    let mut pill = Session::connect(h.connect());
    pill.keyboard();
    pill.pointer();
    let l = pill.keyboard_layer("exponential-overlay:call-pill", Layer::Overlay, (1280, 800), KeyboardInteractivity::None);
    l.layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    l.surface.commit();
    pill.roundtrip();
    wait_for(&mut pill, |c| c.state.layers[0].size == (1280, 800));
    // The pill: (460, 260) to (560, 300), over the window's corner.
    pill.input_region(&l.surface, Some((460, 260, 100, 40)));
    pill.draw_layer(&l, GREEN);
    wait_for(&mut pill, |_| h.capture(0).at(640, 400) == GREEN);

    click(&h, 700.0, 450.0);
    wait_for(&mut app, |c| c.state.press_serial.is_some());
    pill.roundtrip();
    assert_eq!(pill.state.press_serial, None, "the layer took a click beside the pill");
    app.state.press_serial = None;
    click(&h, 500.0, 280.0);
    wait_for(&mut pill, |c| c.state.press_serial.is_some());
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    assert_eq!(app.state.press_serial, None, "the window under the pill got the pill's click");
    assert!(pill.state.keys.is_empty() && pill.state.keyboard_focus.is_none(), "the pill took the keyboard");
    assert_eq!(app.state.keyboard_focus.as_ref(), Some(&a.surface));
}

#[test]
fn a_background_window_that_asks_for_fullscreen_stays_behind() {
    let h = harness();
    let (mut app, _a) = focused_app(&h, "editor", RED);
    let (mut other, b) = other_app(&h, "player", BLUE);

    b.toplevel.set_fullscreen(None);
    other.roundtrip();
    wait_for(&mut other, |c| c.state.toplevels[0].states.contains(&XdgState::Fullscreen));
    assert_eq!(other.state.toplevels[0].size, (1280, 800));
    other.draw_configured(&b, 0, 0, BLUE);
    wait_for(&mut other, |_| h.capture(0).at(10, 10) == BLUE);
    // Fullscreen in size, but the output still shows the window typed into.
    assert_eq!(h.capture(0).at(600, 400), RED, "a hidden fullscreen window blanked the focused one");
    assert_eq!(stack(&h), [("player".to_string(), false), ("editor".to_string(), true)]);
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    other.roundtrip();
    assert!(other.state.keys.is_empty());

    // Once the person switches to it, it is the output's fullscreen window.
    h.with(|ede| {
        let player = ede.desk.space.elements().next().cloned();
        ede.focus(player);
    });
    wait_for(&mut other, |_| h.capture(0).at(600, 400) == BLUE);
}

#[test]
fn a_background_window_that_maximizes_stays_behind() {
    let h = harness();
    let (mut app, _a) = focused_app(&h, "editor", RED);
    let (mut other, b) = other_app(&h, "updater", BLUE);

    b.toplevel.set_maximized();
    other.roundtrip();
    wait_for(&mut other, |c| c.state.toplevels[0].states.contains(&XdgState::Maximized));
    other.draw_configured(&b, 0, 0, BLUE);
    wait_for(&mut other, |_| h.capture(0).at(10, 10) == BLUE);
    assert_eq!(h.capture(0).at(600, 400), RED, "the maximized window came in front of the focused one");
    assert!(!other.state.toplevels[0].states.contains(&XdgState::Activated));
    app.roundtrip();
    assert!(app.state.toplevels[0].states.contains(&XdgState::Activated));
    assert_eq!(stack(&h), [("updater".to_string(), false), ("editor".to_string(), true)]);
}

/// X11 windows share Xwayland's one connection: the same app is the same
/// `_NET_WM_PID`, or a window transient for the focused one.
#[cfg(feature = "xwayland")]
#[test]
fn an_x11_window_takes_the_keyboard_only_from_its_own_app() {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, PropMode, WindowClass},
        wrapper::ConnectionExt as _,
    };
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let h = harness();
    h.with(|ede| ede.start_xwayland());
    let until = |what: &str, ok: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ok() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    until("Xwayland never became ready", &|| h.with(|ede| ede.xdisplay).is_some());
    let display = h.with(|ede| ede.xdisplay).unwrap();
    let (mut app, _a) = focused_app(&h, "editor", RED);

    let (conn, screen) = x11rb::connect(Some(&format!(":{display}"))).expect("connect to Xwayland");
    let root = conn.setup().roots[screen].root;
    let open = |transient_for: Option<u32>| {
        let win = conn.generate_id().unwrap();
        conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, win, root, 0, 0, 320, 240, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new()).unwrap();
        if let Some(parent) = transient_for {
            conn.change_property32(PropMode::REPLACE, win, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::WINDOW, &[parent]).unwrap();
        }
        conn.map_window(win).unwrap();
        conn.flush().unwrap();
        win
    };
    let x11_windows = || h.with(|ede| ede.desk.space.elements().filter(|w| w.x11_surface().is_some()).count());
    let focused_x11 = || h.with(|ede| ede.focused_window().and_then(|w| w.x11_surface().map(|x| x.window_id())));

    let first = open(None);
    until("the X11 window never mapped", &|| x11_windows() == 1);
    press(&h, KEY_A);
    wait_for(&mut app, |c| c.state.keys == [KEY_A]);
    assert_eq!(focused_x11(), None, "an X11 window nobody asked for took the keyboard");

    // The person switches to it; its own dialog then takes the keyboard.
    h.with(|ede| {
        let x11 = ede.desk.space.elements().find(|w| w.x11_surface().is_some()).cloned();
        ede.focus(x11);
    });
    until("the X11 window never took the keyboard", &|| focused_x11() == Some(first));
    let dialog = open(Some(first));
    until("the X11 dialog of the focused window did not take the keyboard", &|| focused_x11() == Some(dialog));
}

/// #EDE-103: the pointer's focus follows the stack under a still pointer:
/// after a raise the click is the raised window's, after a close it is the
/// one now under it.
#[test]
fn a_click_under_a_still_pointer_reaches_the_window_now_on_top() {
    let h = harness();
    let (mut app, _a) = focused_app(&h, "editor", RED);
    let _app_pointer = app.pointer();
    let (mut other, _b) = other_app(&h, "player", BLUE);
    let _other_pointer = other.pointer();
    let at = h.with(|ede| {
        let geos: Vec<_> = ede.desk.space.elements().map(|w| ede.desk.space.element_geometry(w).unwrap()).collect();
        let both = geos[0].intersection(geos[1]).expect("the windows overlap");
        smithay::utils::Point::<f64, smithay::utils::Logical>::from(((both.loc.x + 10) as f64, (both.loc.y + 10) as f64))
    });
    // The editor on top has the pointer.
    h.with(move |ede| ede.pointer_motion_to(at, 0));
    // The player comes to the front (Alt+Tab): no motion.
    h.with(|ede| {
        let player = ede.desk.space.elements().next().cloned();
        ede.focus(player);
    });
    h.with(|ede| {
        ede.pointer_button(0x110, ButtonState::Pressed, 1);
        ede.pointer_button(0x110, ButtonState::Released, 2);
    });
    other.roundtrip();
    app.roundtrip();
    assert!(other.state.press_serial.is_some(), "the raised window did not get the click");
    assert_eq!(app.state.press_serial, None, "the window behind got the click");

    // The player goes away under the still pointer: the editor is under it.
    drop(other);
    wait_for(&mut app, |_| stack(&h).len() == 1);
    h.with(|ede| {
        ede.pointer_button(0x110, ButtonState::Pressed, 3);
        ede.pointer_button(0x110, ButtonState::Released, 4);
    });
    app.roundtrip();
    assert!(app.state.press_serial.is_some(), "the click went to the closed window");
}
