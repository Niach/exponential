//! Headless: the person picks a part of their screen (`pick_region`): a
//! dragged band across two outputs (one at 1.25) answers its logical
//! rectangle, a click the window under the pointer (frame included) or the
//! output, Escape and the right button cancel, one pick at a time, the
//! caller's connection going ends it, and the band shows on the person's
//! monitors but never in a capture.

mod common;

use std::{
    thread::JoinHandle,
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{backend::headless::Harness, Config};
use ede_protocol::{Capability, CaptureTarget, Client, Error, ErrorCode, InputEvent, PickMode, PickedRegion, Request, RunId, SessionId};

const BLUE: u32 = 0xff00_00ff;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const KEY_ESC: u32 = 1;
/// The band's border: the theme's selection colour, opaque.
const BORDER: u32 = 0xffe5_e5e5;

/// HEADLESS-1 1200x900 at 1, HEADLESS-2 1000x800 at 1.25 (800x640 logical) to its right.
fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::parse("[[outputs]]\nname = \"HEADLESS-2\"\nscale = 1.25\n").unwrap(), &[(1200, 900), (1000, 800)])
}

fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn picking(h: &Harness) -> bool {
    h.with(|e| e.screen_pick.is_some())
}

/// A pick asked on its own connection; the thread returns the answer.
fn pick(h: &Harness, mode: PickMode) -> JoinHandle<Result<Option<PickedRegion>, Error>> {
    let mut client = h.ipc();
    let asked = std::thread::spawn(move || client.pick_region(mode));
    wait_until("the pick starts", || picking(h));
    asked
}

fn answer(asked: JoinHandle<Result<Option<PickedRegion>, Error>>) -> Option<PickedRegion> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !asked.is_finished() {
        assert!(Instant::now() < deadline, "the pick never answered");
        std::thread::sleep(Duration::from_millis(10));
    }
    asked.join().unwrap().expect("the pick answers")
}

/// The person's own hand (the root grant's `bindings` input).
struct Hand(Client);

impl Hand {
    fn new(h: &Harness) -> Self {
        Self(h.ipc())
    }
    fn to(&mut self, x: f64, y: f64) {
        self.0.input_as_person(InputEvent::PointerMotion { x, y }).unwrap();
    }
    fn button(&mut self, button: u32, pressed: bool) {
        self.0.input_as_person(InputEvent::PointerButton { button, pressed }).unwrap();
    }
    fn click(&mut self, button: u32) {
        self.button(button, true);
        self.button(button, false);
    }
    fn key(&mut self, key: u32) {
        self.0.input_as_person(InputEvent::Key { key, pressed: true }).unwrap();
        self.0.input_as_person(InputEvent::Key { key, pressed: false }).unwrap();
    }
}

/// Each channel within 2 of `want`.
fn near(px: u32, want: u32) -> bool {
    (0..4).all(|i| ((px >> (i * 8)) & 0xff).abs_diff((want >> (i * 8)) & 0xff) <= 2)
}

fn crosshair(h: &Harness) -> bool {
    h.with(|e| matches!(e.desk.cursor_status, smithay::input::pointer::CursorImageStatus::Named(smithay::input::pointer::CursorIcon::Crosshair)))
}

/// The pick's grabs are gone and the arrow is back.
fn released(h: &Harness) -> bool {
    h.with(|e| {
        let pointer = e.desk.seat.get_pointer().unwrap().is_grabbed();
        let keyboard = e.desk.seat.get_keyboard().unwrap().is_grabbed();
        let arrow = matches!(e.desk.cursor_status, smithay::input::pointer::CursorImageStatus::Named(smithay::input::pointer::CursorIcon::Default));
        e.screen_pick.is_none() && !pointer && !keyboard && arrow
    })
}

/// A drag from output 1 into output 2 (scale 1.25) answers its logical
/// rectangle; meanwhile the band is on both monitors at each one's scale
/// and in no capture.
#[test]
fn a_drag_across_two_outputs_answers_its_logical_rectangle() {
    let h = harness();
    let outputs = h.ipc().outputs().unwrap();
    let second = outputs.iter().find(|o| o.name == "HEADLESS-2").unwrap().clone();
    assert_eq!((second.width, second.height, second.scale), (800, 640, 1.25), "{second:?}");
    let x2 = second.x;
    let mut hand = Hand::new(&h);
    hand.to(1100.0, 100.0);
    let asked = pick(&h, PickMode::Area);
    assert!(crosshair(&h), "a crosshair while picking");
    hand.button(BTN_LEFT, true);
    hand.to(f64::from(x2) + 300.0, 500.0);

    // The person's monitors: the band's border at each output's own scale
    // and place (HEADLESS-2's local logical 0..300 x 100..500 = physical
    // 0..375 x 125..625).
    let shown2 = h.capture(1);
    assert!(near(shown2.at(200, 125), BORDER), "top border on HEADLESS-2: {:#x}", shown2.at(200, 125));
    assert!(near(shown2.at(200, 624), BORDER), "bottom border on HEADLESS-2: {:#x}", shown2.at(200, 624));
    assert!(near(shown2.at(374, 300), BORDER), "right border on HEADLESS-2: {:#x}", shown2.at(374, 300));
    assert!(!near(shown2.at(376, 300), BORDER) && !near(shown2.at(200, 123), BORDER), "the band ends at its edges");
    let shown1 = h.capture(0);
    assert!(near(shown1.at(1150, 100), BORDER) && near(shown1.at(1100, 300), BORDER), "the band on HEADLESS-1");
    // Its fill is translucent: neither the border nor what is under it.
    let shared2 = h.capture_shared(1);
    assert_ne!(shown2.at(200, 300), shared2.at(200, 300), "the fill tints the monitor");
    // A capture never has it: the harness's shared frame, and an agent's.
    for (x, y) in [(200, 125), (200, 624), (374, 300), (200, 300)] {
        assert!(!near(shared2.at(x, y), BORDER), "the band in a capture at ({x}, {y})");
    }
    let shot = h.ipc().capture(CaptureTarget::Output { name: "HEADLESS-2".into() }).unwrap();
    assert_eq!(shot.at(200, 125), shared2.at(200, 125), "ede-protocol's capture");
    assert!(!near(h.capture_shared(0).at(1150, 100), BORDER), "nor on HEADLESS-1");

    hand.button(BTN_LEFT, false);
    let picked = answer(asked);
    assert_eq!(picked, Some(PickedRegion { x: 1100, y: 100, width: x2 + 300 - 1100, height: 400, window: None }));
    wait_until("the grabs go", || released(&h));
    assert!(!near(h.capture(1).at(200, 125), BORDER), "the band is gone");
}

/// A click without a drag answers the window under the pointer, frame
/// included, and the window never sees the click; on bare desktop, the
/// output under the pointer.
#[test]
fn a_click_answers_the_window_or_the_output() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    let _pointer = app.pointer();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_until("the window maps", || h.with(|e| e.desk.space.elements().count()) == 1);
    let info = h.ipc().windows().unwrap().remove(0);
    let framed = h.with(|e| {
        let w = e.desk.space.elements().next().unwrap().clone();
        let f = e.framed_geometry(&w).unwrap();
        (f.loc.x, f.loc.y, f.size.w, f.size.h)
    });
    let mut hand = Hand::new(&h);
    let asked = pick(&h, PickMode::Area);
    hand.to(f64::from(info.x + info.width / 2), f64::from(info.y + info.height / 2));
    hand.button(BTN_LEFT, true);
    hand.to(f64::from(info.x + info.width / 2) + 2.0, f64::from(info.y + info.height / 2) + 1.0);
    hand.button(BTN_LEFT, false);
    let picked = answer(asked).expect("a window");
    assert_eq!((picked.x, picked.y, picked.width, picked.height), framed, "the window with its frame");
    assert_eq!(picked.window, Some(info.id.clone()));
    wait_until("the grabs go", || released(&h));
    app.roundtrip();
    assert!(app.state.buttons.is_empty(), "the window saw the pick's click: {:?}", app.state.buttons);

    // Bare desktop on HEADLESS-2: that output.
    let second = h.ipc().outputs().unwrap().into_iter().find(|o| o.name == "HEADLESS-2").unwrap();
    let asked = pick(&h, PickMode::Area);
    hand.to(f64::from(second.x) + 700.0, 600.0);
    hand.click(BTN_LEFT);
    assert_eq!(answer(asked), Some(PickedRegion { x: second.x, y: second.y, width: second.width, height: second.height, window: None }));
}

/// `point`: one click is that pixel (floored logical coordinates).
#[test]
fn a_point_pick_is_one_pixel() {
    let h = harness();
    let mut hand = Hand::new(&h);
    let asked = pick(&h, PickMode::Point);
    hand.to(10.7, 20.2);
    hand.click(BTN_LEFT);
    assert_eq!(answer(asked), Some(PickedRegion { x: 10, y: 20, width: 1, height: 1, window: None }));
    wait_until("the grabs go", || released(&h));
}

/// Escape and the right button cancel (`null`), in the middle of a drag too.
#[test]
fn escape_and_the_right_button_cancel() {
    let h = harness();
    let mut app = Session::connect(h.connect());
    let _keyboard = app.keyboard();
    let t = app.toplevel("notes");
    app.draw(&t, 300, 200, BLUE);
    wait_until("the window maps", || h.with(|e| e.desk.space.elements().count()) == 1);
    let mut hand = Hand::new(&h);

    let asked = pick(&h, PickMode::Area);
    hand.to(100.0, 100.0);
    hand.button(BTN_LEFT, true);
    hand.to(300.0, 300.0);
    hand.key(KEY_ESC);
    hand.button(BTN_LEFT, false);
    assert_eq!(answer(asked), None, "Escape");
    wait_until("the grabs go", || released(&h));
    app.roundtrip();
    assert!(app.state.keys.is_empty(), "the window saw the pick's Escape: {:?}", app.state.keys);

    let asked = pick(&h, PickMode::Point);
    hand.click(BTN_RIGHT);
    assert_eq!(answer(asked), None, "the right button");
    wait_until("the grabs go", || released(&h));
}

/// One pick at a time; an agent's grant, a space's or one without
/// `capture` never starts one.
#[test]
fn one_pick_at_a_time_and_only_the_persons() {
    let h = harness();
    let mut hand = Hand::new(&h);
    let asked = pick(&h, PickMode::Area);
    let second = h.ipc().pick_region(PickMode::Point);
    assert!(matches!(&second, Err(e) if e.has_code(ErrorCode::Conflict)), "{second:?}");
    hand.key(KEY_ESC);
    assert_eq!(answer(asked), None);
    wait_until("the grabs go", || released(&h));

    let mut root = h.ipc();
    let agent: ede_protocol::Grant = serde_json::from_value(
        root.call(Request::MintGrant { session_id: SessionId::new("agent").unwrap(), run_id: Some(RunId::new("run-1").unwrap()), capabilities: vec![Capability::Capture], expires_at: None }).unwrap(),
    )
    .unwrap();
    let refused = h.ipc_with(agent.token).unwrap().pick_region(PickMode::Area);
    assert!(matches!(&refused, Err(e) if e.has_code(ErrorCode::Forbidden)), "an agent's grant: {refused:?}");
    let ui = root.mint(SessionId::new("panel").unwrap(), vec![Capability::Ui], None).unwrap();
    let refused = h.ipc_with(ui.token).unwrap().pick_region(PickMode::Area);
    assert!(matches!(&refused, Err(e) if e.has_code(ErrorCode::Forbidden)), "no capture: {refused:?}");
    assert!(!picking(&h));
}

/// The caller's connection going ends its pick: the person's pointer and
/// keyboard are theirs again.
#[test]
fn the_pick_ends_with_its_callers_connection() {
    let h = harness();
    let mut client = h.ipc();
    let stream = client.try_clone_stream().unwrap();
    let asked = std::thread::spawn(move || client.pick_region(PickMode::Area));
    wait_until("the pick starts", || picking(&h));
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    wait_until("the pick ends", || released(&h));
    assert!(asked.join().unwrap().is_err(), "no answer on a closed connection");
}
