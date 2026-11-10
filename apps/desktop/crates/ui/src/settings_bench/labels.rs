//! #EXP-1629: Settings → Labels in every state (rows in several colours and
//! a long name, empty, no team, the create form empty / typed / duplicate /
//! busy / failed, a row's rename error, a row's swatch popover open, the
//! delete hovered).
use super::*;
use crate::settings::labels::{LabelsPane, LabelsShot};

/// Where the pointer goes before the picture.
#[derive(Clone, Copy)]
enum Pointer {
    Rest,
    Hover((f32, f32)),
    /// A left click (the gpui pane's picture of the open swatch popover).
    #[allow(dead_code)]
    Click((f32, f32)),
    /// The document's press on a node (the eUI pane's click).
    Press(&'static str),
}

fn frames(window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext) {
    for _ in 0..6 {
        draw(window, cx);
        cx.run_until_parked();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
        draw(window, cx);
        cx.run_until_parked();
    }
}

fn mouse(window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext, event: gpui::PlatformInput) {
    cx.update_window(window, |_, window, cx| window.dispatch_event(event, cx)).expect("a pointer event");
}

fn shoot(name: &str, shot: LabelsShot, pointer: Pointer) {
    let Some(dir) = std::env::var_os("EXP_SETTINGS_SHOTS") else { return };
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut frame = None;
    let handle = cx
        .open_window(size(px(PANE.0), px(PANE.1)), |window: &mut Window, cx| {
            let pane = cx.new(|cx| LabelsPane::for_shots(window, cx));
            let view = cx.new(|_| Frame { pane: pane.clone().into() });
            frame = Some(pane);
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let frame = frame.expect("the frame");
    cx.update_window(window, |_, window, cx| frame.update(cx, |pane, cx| pane.show(shot, window, cx))).expect("the state");
    frames(window, &mut cx);
    match pointer {
        Pointer::Rest => {}
        Pointer::Hover((x, y)) => {
            let position = gpui::point(px(x), px(y));
            mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }));
            frames(window, &mut cx);
        }
        Pointer::Press(id) => {
            cx.update_window(window, |_, window, cx| frame.update(cx, |pane, cx| pane.press_for_shots(id, window, cx))).expect("the press");
            frames(window, &mut cx);
            cx.update(|cx| frame.update(cx, |pane, cx| pane.press_for_shots_settle(cx)));
            frames(window, &mut cx);
        }
        Pointer::Click((x, y)) => {
            let position = gpui::point(px(x), px(y));
            mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }));
            mouse(window, &mut cx, gpui::PlatformInput::MouseDown(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false }));
            frames(window, &mut cx);
            mouse(window, &mut cx, gpui::PlatformInput::MouseUp(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 }));
            frames(window, &mut cx);
            // Park the pointer away so nothing paints its hover.
            let away = gpui::point(px(PANE.0 - 4.), px(PANE.1 - 4.));
            mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position: away, pressed_button: None, modifiers: Default::default() }));
            frames(window, &mut cx);
        }
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("the PNG");
    if let Some(before) = std::env::var_os("EXP_SETTINGS_BEFORE") {
        let before = image::open(std::path::Path::new(&before).join(format!("{name}.png"))).expect("the earlier picture").to_rgba8();
        let after = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
        assert_eq!(before.dimensions(), after.dimensions());
        let mut diff = after.clone();
        let (mut count, mut bounds, mut worst) = (0u32, (u32::MAX, u32::MAX, 0u32, 0u32), 0u8);
        for (x, y, pixel) in diff.enumerate_pixels_mut() {
            let a = before.get_pixel(x, y);
            if a == pixel {
                pixel.0 = [pixel.0[0] / 3, pixel.0[1] / 3, pixel.0[2] / 3, 255];
                continue;
            }
            worst = worst.max((0..3).map(|c| a.0[c].abs_diff(pixel.0[c])).max().unwrap_or(0));
            count += 1;
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
            pixel.0 = [255, 0, 0, 255];
        }
        eprintln!("{name}: {count} pixels differ (worst level {worst}), within {bounds:?}");
        diff.save(std::path::Path::new(&dir).join(format!("{name}.diff.png"))).expect("the diff");
    }
    std::mem::forget(cx);
}

fn label(id: &str, name: &str, color: Option<&str>, order: f64) -> domain::rows::Label {
    domain::rows::Label { id: id.into(), team_id: "team-shots".into(), name: name.into(), color: color.map(Into::into), sort_order: Some(order), created_at: None, updated_at: None }
}

fn rows() -> Vec<domain::rows::Label> {
    vec![
        label("l1", "bug", Some("#ef4444"), 1.),
        label("l2", "feature", Some("#22c55e"), 2.),
        label("l3", "a label with a very long name that keeps going well past the row's width so the field clips its text", Some("#3b82f6"), 3.),
        label("l4", "design", None, 4.),
        label("l5", "needs-triage", Some("#a855f7"), 5.),
    ]
}

fn base() -> LabelsShot {
    LabelsShot { labels: Some(rows()), creating: false, typed: String::new(), color: "#22c55e".into(), submitting: false, create_error: None, row_error: None, draft: None }
}

fn form(typed: &str) -> LabelsShot {
    LabelsShot { creating: true, typed: typed.into(), color: "#0ea5e9".into(), ..base() }
}

/// The first row's swatch and delete (window points).
#[allow(dead_code)]
const SWATCH: (f32, f32) = (45., 89.);
const DELETE: (f32, f32) = (709., 89.);

#[test]
#[ignore]
fn settings_labels_rows() {
    shoot("labels-rows", base(), Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_empty() {
    shoot("labels-empty", LabelsShot { labels: Some(Vec::new()), ..base() }, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_no_team() {
    shoot("labels-no-team", LabelsShot { labels: None, ..base() }, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_create_empty() {
    shoot("labels-create-empty", LabelsShot { color: "#22c55e".into(), ..form("") }, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_create_first() {
    shoot("labels-create-first", LabelsShot { labels: Some(Vec::new()), ..form("") }, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_create_typed() {
    shoot("labels-create-typed", form("regression"), Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_create_duplicate() {
    shoot("labels-create-duplicate", form("Bug"), Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_create_busy() {
    shoot("labels-create-busy", LabelsShot { submitting: true, ..form("regression") }, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_create_failed() {
    shoot("labels-create-failed", LabelsShot { create_error: Some("Failed to create label.".into()), ..form("regression") }, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_row_error() {
    let shot = LabelsShot { row_error: Some(("l2".into(), "A label with this name already exists.".into())), draft: Some(("l2".into(), "Bug".into())), ..base() };
    shoot("labels-row-error", shot, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_labels_color_open() {
    // The gpui pane's picture: `Pointer::Click(SWATCH)`.
    shoot("labels-color-open", base(), Pointer::Press("swatch.customTrigger.l1|swatch.l1.customTrigger|swatch.l1"));
}

#[test]
#[ignore]
fn settings_labels_delete_hover() {
    shoot("labels-delete-hover", base(), Pointer::Hover(DELETE));
}
