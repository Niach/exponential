//! #EXP-1760: the genUI IconPicker / ColorPicker against the IDE's
//! `board_form::icon_picker` / `color_picker`, drawn HEADLESS with the real
//! text system, closed and open, pixel for pixel. Run with
//! `EXP_PICKER_SHOTS=<dir> cargo test -p ui --release -- --ignored picker_bench --nocapture`:
//! every state writes `ide-<state>.png`, `eui-<state>.png` and
//! `<state>.diff.png` (the differing pixels red over a dimmed copy) and
//! prints the count.
use std::rc::Rc;

use eui_core::theme::Mode;
use eui_core::types::NestedNode;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, size, AnyElement, Entity, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use crate::shell_bench::headless;
use crate::transcript_bench::draw;

const WINDOW: (f32, f32) = (480., 520.);
/// Where the trigger's top-left sits (the frame's padding).
const ORIGIN: f32 = 24.;

#[derive(Clone, Copy, PartialEq)]
enum Which {
    Icon,
    Color,
}

struct IdeFrame {
    which: Which,
}

impl Render for IdeFrame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let picker: AnyElement = match self.which {
            Which::Icon => crate::board_form::icon_picker("bench", domain::contract::BOARD_ICON_VALUES, Some("rocket"), Some("#3b82f6"), false, |_, _, _| {}, cx).into_any_element(),
            Which::Color => crate::board_form::color_picker("bench", "#3b82f6", |_, _, _| {}, cx).into_any_element(),
        };
        div().size_full().bg(cx.theme().background).p(px(ORIGIN)).child(div().flex().child(picker))
    }
}

struct EuiFrame {
    surface: Entity<SurfaceView>,
}

impl Render for EuiFrame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background).p(px(ORIGIN)).child(div().flex().child(self.surface.clone()))
    }
}

fn eui_tree(which: Which, open: bool) -> Value {
    let picker = match which {
        Which::Icon => json!({"id": "ip", "component": "IconPicker", "props": {"value": "rocket", "color": "#3b82f6", "open": open}}),
        Which::Color => json!({"id": "cp", "component": "ColorPicker", "props": {"value": "#3b82f6", "open": open}}),
    };
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start"}, "children": [picker]})
}

fn eui_surface(tree: Value, window: &mut Window, cx: &mut gpui::App) -> Entity<SurfaceView> {
    let dark = cx.theme().is_dark();
    cx.new(|cx| {
        let options = SurfaceViewOptions {
            surface_id: "picker-bench".to_string(),
            theme: eui_core::themes::builtin_theme("exponential"),
            mode: if dark { Mode::Dark } else { Mode::Light },
            host: Rc::new(crate::exponential_ui_screen::bench_specimen_host()),
            lift_layers: true,
            ..SurfaceViewOptions::default()
        };
        let mut view = SurfaceView::new(options, window, cx);
        let tree: NestedNode = serde_json::from_value(tree).expect("the bench tree");
        let outcome = view.set_nested(tree, cx);
        assert!(outcome.issues.is_empty(), "{:?}", outcome.issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        view
    })
}

/// One frame (`ide` or `eui`) of `which`, optionally after a press on the
/// trigger's centre (`open`), saved as `<prefix>-<state>.png`.
fn shoot(dir: &std::path::Path, prefix: &str, which: Which, open: bool) -> image::RgbaImage {
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut eui = None;
    let handle = cx
        .open_window(size(px(WINDOW.0), px(WINDOW.1)), |window: &mut Window, cx| {
            let view: gpui::AnyView = if prefix == "ide" { cx.new(|_| IdeFrame { which }).into() } else {
                let surface = eui_surface(eui_tree(which, open), window, cx);
                eui = Some(surface.clone());
                cx.new(|_| EuiFrame { surface }).into()
            };
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    if let (true, Some(surface)) = (false, eui.as_ref()) {
        // A headless click never reaches an eUI popup (settings_bench's
        // rule): press the trigger node, as the pointer would.
        let trigger = if which == Which::Icon { "ip.trigger" } else { "cp.trigger" };
        cx.update_window(window, |_, window, cx| {
            surface.update(cx, |view, cx| {
                let index = view.index_of(trigger).expect("the trigger");
                view.press(index, window, cx);
            })
        })
        .expect("a press");
    } else if open && eui.is_none() {
        // The 32 px trigger's centre.
        let position = gpui::point(px(ORIGIN + 16.), px(ORIGIN + 16.));
        cx.update_window(window, |_, window, cx| {
            window.dispatch_event(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }), cx);
            window.dispatch_event(gpui::PlatformInput::MouseDown(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false }), cx);
        })
        .expect("a press");
        draw(window, &mut cx);
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.dispatch_event(gpui::PlatformInput::MouseUp(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 }), cx);
        })
        .expect("a release");
        // Park the pointer away from the grid so no cell paints its hover.
        let away = gpui::point(px(WINDOW.0 - 4.), px(4.));
        cx.update_window(window, |_, window, cx| {
            window.dispatch_event(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position: away, pressed_button: None, modifiers: Default::default() }), cx);
        })
        .expect("a move");
    }
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    // The headless clock stands still: run an open eUI popup's enter fade
    // out by hand (settings_bench's Voice pane does the same).
    if let Some(surface) = eui.as_ref() {
        cx.update(|cx| surface.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx)));
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    let state = format!("{}-{}", if which == Which::Icon { "icon" } else { "color" }, if open { "open" } else { "closed" });
    shot.save(dir.join(format!("{prefix}-{state}.png"))).expect("the PNG");
    let image = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
    std::mem::forget(cx);
    image
}

fn compare(which: Which, open: bool) {
    let Some(dir) = std::env::var_os("EXP_PICKER_SHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    let ide = shoot(&dir, "ide", which, open);
    let eui = shoot(&dir, "eui", which, open);
    assert_eq!(ide.dimensions(), eui.dimensions());
    let mut diff = eui.clone();
    let (mut count, mut bounds, mut worst) = (0u32, (u32::MAX, u32::MAX, 0u32, 0u32), 0u8);
    for (x, y, pixel) in diff.enumerate_pixels_mut() {
        let a = ide.get_pixel(x, y);
        if a == pixel {
            pixel.0 = [pixel.0[0] / 3, pixel.0[1] / 3, pixel.0[2] / 3, 255];
            continue;
        }
        let level = (0..3).map(|c| a.0[c].abs_diff(pixel.0[c])).max().unwrap_or(0);
        worst = worst.max(level);
        count += 1;
        bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
        pixel.0 = [255, 0, 0, 255];
    }
    let state = format!("{}-{}", if which == Which::Icon { "icon" } else { "color" }, if open { "open" } else { "closed" });
    eprintln!("{state}: {count} pixels differ (worst level {worst}), within {bounds:?}");
    diff.save(dir.join(format!("{state}.diff.png"))).expect("the diff");
}

#[test]
#[ignore]
fn picker_bench_icon_closed() {
    compare(Which::Icon, false);
}

#[test]
#[ignore]
fn picker_bench_icon_open() {
    compare(Which::Icon, true);
}

#[test]
#[ignore]
fn picker_bench_color_closed() {
    compare(Which::Color, false);
}

#[test]
#[ignore]
fn picker_bench_color_open() {
    compare(Which::Color, true);
}

/// #EXP-1760: the PR's desktop picture: the catalog specimen (a pick, none,
/// the open grid) painted by the gpui painter under the exponential theme,
/// `EXP_PICKER_SHOTS/desktop-<specimen>.png`.
fn shoot_specimen(specimen: &str, root: &str) {
    let Some(dir) = std::env::var_os("EXP_PICKER_SHOTS") else { return };
    let tree = crate::exponential_ui_screen::specimen_node(specimen, root).expect("the specimen");
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut eui = None;
    let handle = cx
        .open_window(size(px(720.), px(760.)), |window: &mut Window, cx| {
            let surface = eui_surface(tree, window, cx);
            eui = Some(surface.clone());
            let view = cx.new(|_| EuiFrame { surface });
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let surface = eui.expect("the surface");
    cx.update(|cx| surface.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx)));
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::PathBuf::from(dir).join(format!("desktop-{specimen}.png"))).expect("the PNG");
    std::mem::forget(cx);
}

#[test]
#[ignore]
fn picker_bench_specimens() {
    shoot_specimen("exponential-ui-icon-picker", "specimen_icon_picker");
    shoot_specimen("exponential-ui-color-picker", "specimen_color_picker");
}
