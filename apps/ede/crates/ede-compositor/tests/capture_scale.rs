//! #EDE-113 headless: a capture of an output on a scaled output (1.25, 1.5,
//! 2) is what that output shows, pixel for pixel, and a click ePilot makes
//! from it lands where it points.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::{Frame, Harness}, Config};
use ede_protocol::{CaptureTarget, InputEvent};

const BLUE: u32 = 0xff00_00ff;
const RED: u32 = 0xffff_0000;
const BTN_LEFT: u32 = 0x110;
/// The output's mode, in pixels.
const MODE: (i32, i32) = (1200, 900);
/// The window (buffer pixels, buffer scale 1) and its button: off its
/// centre, so a capture drawn at the wrong size puts it elsewhere.
const WINDOW: (i32, i32) = (300, 200);
const BUTTON: (i32, i32, i32, i32) = (200, 140, 80, 40);

fn harness(scale: f64) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::parse(&format!("[[outputs]]\nname = \"HEADLESS-1\"\nscale = {scale}\n")).unwrap(), &[MODE])
}

fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        client.roundtrip();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A window with its button on the output.
fn app_with_button(h: &Harness) -> Session {
    let mut app = Session::connect(h.connect());
    let _pointer = app.pointer();
    let t = app.toplevel("button");
    app.draw_with_rect(&t, WINDOW, BLUE, BUTTON, RED);
    wait_for(&mut app, |_| h.with(|e| e.desk.space.elements().count()) == 1);
    app
}

/// What the output itself shows: the scene drawn the way the real backends
/// draw a monitor (`backend::render` with a damage tracker made from the
/// output, its scale included), in software.
fn monitor(h: &Harness) -> Frame {
    h.with_backend(|state, headless| {
        use smithay::backend::{
            allocator::Fourcc,
            renderer::{damage::OutputDamageTracker, pixman::PixmanRenderer, Bind, Offscreen},
        };
        let output = headless.outputs[0].clone();
        let (w, h) = MODE;
        let mut renderer = PixmanRenderer::new().unwrap();
        let mut image = Offscreen::<smithay::reexports::pixman::Image<'static, 'static>>::create_buffer(&mut renderer, Fourcc::Argb8888, (w, h).into()).unwrap();
        {
            let mut fb = renderer.bind(&mut image).unwrap();
            let mut tracker = OutputDamageTracker::from_output(&output);
            ede_compositor::backend::render(state, &output, &mut renderer, &mut fb, 0, &mut tracker).unwrap();
        }
        let stride = image.stride() / 4;
        // Safety: our own image, `h` rows of `stride` u32s, unbound.
        let data = unsafe { std::slice::from_raw_parts(image.data(), stride * h as usize) };
        Frame { width: w, height: h, pixels: data.chunks(stride).flat_map(|row| row[..w as usize].iter().copied()).collect() }
    })
}

/// The output as an agent captures it (ede-protocol, as ePilot, the
/// portal and `exponential-ctl capture` do).
fn captured(h: &Harness) -> Frame {
    let shot = h.ipc().capture(CaptureTarget::Output { name: "HEADLESS-1".into() }).unwrap();
    let (w, h) = (shot.info.width as i32, shot.info.height as i32);
    Frame { width: w, height: h, pixels: (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| shot.at(x, y)).collect() }
}

/// The bounds (x0, y0, x1, y1, exclusive) of the pixels exactly `argb`.
fn bounds(frame: &Frame, argb: u32) -> (i32, i32, i32, i32) {
    let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..frame.height {
        for x in 0..frame.width {
            if frame.at(x, y) == argb {
                b = (b.0.min(x), b.1.min(y), b.2.max(x + 1), b.3.max(y + 1));
            }
        }
    }
    assert!(b.0 < b.2, "no pixel of {argb:#x} in the frame");
    b
}

/// A capture of an output equals what the output shows, at every scale.
#[test]
fn an_output_capture_equals_the_outputs_own_render_at_every_scale() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let h = harness(scale);
        let _app = app_with_button(&h);
        let shown = monitor(&h);
        let shot = captured(&h);
        assert_eq!((shot.width, shot.height), MODE, "scale {scale}: the output's mode");
        let differing = shot.pixels.iter().zip(&shown.pixels).filter(|(a, b)| a != b).count();
        assert_eq!(differing, 0, "scale {scale}: the capture differs from the output in {differing} pixels");
        // The button's size on the output: its logical size times the scale.
        let (x0, y0, x1, y1) = bounds(&shot, RED);
        let expect = |n: i32| f64::from(n) * scale;
        assert!(((x1 - x0) as f64 - expect(BUTTON.2)).abs() <= 2.0 && ((y1 - y0) as f64 - expect(BUTTON.3)).abs() <= 2.0, "scale {scale}: button {}x{}", x1 - x0, y1 - y0);
        // The harness's own frames are the same picture.
        assert_eq!(h.capture(0).pixels, shown.pixels, "scale {scale}: the harness capture");
        assert_eq!(h.capture_shared(0).pixels, shown.pixels, "scale {scale}: the harness shared capture");
    }
}

/// A window capture stays at the window's own (logical) size, whatever the
/// output's scale: its buffer at scale 1, pixel for pixel.
#[test]
fn a_window_capture_stays_at_the_windows_own_size_on_a_scaled_output() {
    let h = harness(1.5);
    let _app = app_with_button(&h);
    let mut ipc = h.ipc();
    let id = ipc.windows().unwrap()[0].id.clone();
    let shot = ipc.capture(CaptureTarget::Window { window: id }).unwrap();
    assert_eq!((shot.info.width as i32, shot.info.height as i32), WINDOW);
    assert_eq!(shot.at(0, 0), BLUE);
    assert_eq!((shot.at(BUTTON.0, BUTTON.1), shot.at(BUTTON.0 + BUTTON.2 - 1, BUTTON.1 + BUTTON.3 - 1)), (RED, RED));
    assert_eq!(shot.at(BUTTON.0 - 1, BUTTON.1), BLUE);
}

/// ePilot's mapping (`epilot::tools::Frame`): the display is the output's
/// logical rect; the screenshot (the output's pixels) is fitted into a
/// frame of the logical size bounded by 1280x800 (never upscaled), and a
/// frame point goes back to the screen as `origin + point / (frame / logical)`.
fn epilot_click_point(logical: (i32, i32, u32, u32), pixels: (i32, i32), at: (f64, f64)) -> (i32, i32) {
    let (x, y, w, h) = logical;
    let fit = (1280.0 / f64::from(w)).min(800.0 / f64::from(h)).min(1.0);
    let image = ((f64::from(w) * fit).round().max(1.0), (f64::from(h) * fit).round().max(1.0));
    // The pixel in the fitted screenshot (encode_png resizes the capture).
    let frame = (at.0 * image.0 / f64::from(pixels.0), at.1 * image.1 / f64::from(pixels.1));
    let (sx, sy) = (image.0 / f64::from(w), image.1 / f64::from(h));
    let clamp = |v: f64, max: u32| v.round().clamp(0.0, f64::from(max - 1)) as i32;
    (x + clamp(frame.0 / sx, w), y + clamp(frame.1 / sy, h))
}

/// The agent's side end to end: a screenshot, the centre of the button in
/// it, a click through ePilot's mapping: the client gets it on its button.
#[test]
fn a_click_on_the_button_an_agent_sees_lands_on_it_on_a_scaled_output() {
    for scale in [1.5, 2.0] {
        let h = harness(scale);
        let mut app = app_with_button(&h);
        let mut ipc = h.ipc();
        let out = ipc.outputs().unwrap().remove(0);
        let shot = captured(&h);
        let (x0, y0, x1, y1) = bounds(&shot, RED);
        let centre = (f64::from(x0 + x1) / 2.0, f64::from(y0 + y1) / 2.0);
        let (x, y) = epilot_click_point((out.x, out.y, out.width as u32, out.height as u32), (shot.width, shot.height), centre);
        ipc.input(InputEvent::PointerMotion { x: f64::from(x), y: f64::from(y) }).unwrap();
        ipc.input(InputEvent::PointerButton { button: BTN_LEFT, pressed: true }).unwrap();
        ipc.input(InputEvent::PointerButton { button: BTN_LEFT, pressed: false }).unwrap();
        wait_for(&mut app, |c| c.state.press_at.is_some());
        let (px, py) = app.state.press_at.unwrap();
        let inside = (f64::from(BUTTON.0)..f64::from(BUTTON.0 + BUTTON.2)).contains(&px) && (f64::from(BUTTON.1)..f64::from(BUTTON.1 + BUTTON.3)).contains(&py);
        assert!(inside, "scale {scale}: the click landed at ({px}, {py}) on the window, the button is {BUTTON:?}");
        let centre = (f64::from(BUTTON.0) + f64::from(BUTTON.2) / 2.0, f64::from(BUTTON.1) + f64::from(BUTTON.3) / 2.0);
        assert!((px - centre.0).abs() <= 2.0 && (py - centre.1).abs() <= 2.0, "scale {scale}: ({px}, {py}) is the button's centre {centre:?}");
    }
}
