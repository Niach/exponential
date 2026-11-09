//! EXP-1586: the ZoomableImage native through the painter in a headless gpui
//! window (catalog/style.json `zoomableImage`). Every expected view comes
//! from the core's maths (`eui_core::zoom`, locked by `fixtures/zoom.json`):
//! these tests check that gpui's input reaches it — ⌘-wheel about the
//! cursor, a plain wheel that pans only with room, double click, the keys,
//! the pill's buttons and Menu, a drag that rubber-bands and springs back,
//! the delayed `tap`, the bound `scale` written both ways — and that the
//! view also stands on its own outside a surface (the IDE's lightbox).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use eui_core::zoom::{self as z, WheelDeltaMode, ZoomFrame, ZoomPoint, ZoomView};
use eui_core::NestedNode;
use eui_gpui::controls::{ZoomControls, ZoomPart, ZoomableImage, ZoomableImageEvent, ZoomableImageOptions, TAP_DELAY};
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, point, prelude::*, px, App, Bounds, Entity, Modifiers, MouseButton, Pixels, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext};
use serde_json::{json, Value};

/// A 1 × 1 PNG (no network in tests); the natural size is the author's.
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
}

impl Recorder {
    fn named(&self, name: &str) -> Vec<ActionEvent> {
        self.actions.borrow().iter().filter(|a| a.name == name).cloned().collect()
    }
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// A 480 px column holding one ZoomableImage `zoomable` of 1440 × 900
/// (`extra` props merged), its `zoom` / `tap` reported as actions and its
/// `scale` bound to `/zoom`.
fn surface(cx: &mut TestAppContext, extra: Value) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    let mut props = json!({"src": PNG, "alt": "Screenshot", "naturalWidth": 1440, "naturalHeight": 900, "controls": "always", "scale": {"@path": "/zoom"}});
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        props[k] = v;
    }
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 480}, "children": [
        {"id": "zoomable", "component": "ZoomableImage", "props": props, "on": {
            "zoom": {"event": {"name": "zoomed"}},
            "tap": {"event": {"name": "tapped"}}
        }}
    ]});
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(vcx);
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, log, vcx)
}

fn zoom_of(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> Entity<ZoomableImage> {
    view.read_with(cx, |v, _| v.zoomable_image("zoomable")).expect("the ZoomableImage view")
}

fn current(zoom: &Entity<ZoomableImage>, cx: &mut VisualTestContext) -> (ZoomView, ZoomFrame) {
    zoom.read_with(cx, |z, _| (z.view(), z.frame()))
}

fn painted(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds("zoomable")).expect("zoomable painted")
}

fn local(b: Bounds<Pixels>, x: f32, y: f32) -> gpui::Point<Pixels> {
    b.origin + point(px(x), px(y))
}

fn close(a: ZoomView, b: ZoomView) -> bool {
    (a.scale - b.scale).abs() < 1e-4 && (a.x - b.x).abs() < 0.05 && (a.y - b.y).abs() < 0.05
}

/// Let the standard motion (and any timer) run out.
fn settle(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(ms));
    draw(cx);
}

#[gpui::test]
fn the_box_fills_its_column_at_the_pictures_ratio_and_opens_at_fit(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({}));
    let f = view.read_with(cx, |v, _| v.frame(v.index_of("zoomable").unwrap()).unwrap());
    assert!((f.w - 480.0).abs() < 0.5 && (f.h - 300.0).abs() < 0.5, "480 wide, 1440:900 tall: {f:?}");
    let zoom = zoom_of(&view, cx);
    let (v, frame) = current(&zoom, cx);
    assert!(close(v, z::zoom_rest_view(&frame)), "opens at rest: {v:?}");
    assert!((v.scale - 1.0 / 3.0).abs() < 1e-4, "fit = 480 / 1440: {v:?}");
    let info = view.read_with(cx, |v, _| v.accessible_info("zoomable")).unwrap();
    assert_eq!(info.role, gpui::Role::Image);
    assert_eq!(info.label.as_deref(), Some("Screenshot"));
}

#[gpui::test]
fn a_tall_picture_opens_at_fit_width_on_its_top_in_a_square_window(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({"naturalWidth": 1000, "naturalHeight": 8000}));
    let f = view.read_with(cx, |v, _| v.frame(v.index_of("zoomable").unwrap()).unwrap());
    assert!((f.h - 480.0).abs() < 0.5, "capped at the width: {f:?}");
    let zoom = zoom_of(&view, cx);
    let (v, _) = current(&zoom, cx);
    assert!((v.scale - 0.48).abs() < 1e-4 && v.y.abs() < 1e-3 && v.x.abs() < 1e-3, "fit width at the top: {v:?}");
    // A plain wheel scrolls it (room on y), the surface keeps its place.
    let b = painted(&view, cx);
    cx.simulate_event(ScrollEvent { position: local(b, 240.0, 240.0), delta: ScrollDelta::Pixels(point(px(0.0), px(-120.0))), ..Default::default() });
    draw(cx);
    let (after, _) = current(&zoom, cx);
    assert!((after.y + 120.0).abs() < 1e-3 && (after.scale - v.scale).abs() < 1e-9, "panned down 120: {after:?}");
}

#[gpui::test]
fn cmd_wheel_zooms_about_the_cursor_and_reports_once_settled(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let b = painted(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    let at = ZoomPoint { x: 100.0, y: 80.0 };
    cx.simulate_event(ScrollEvent { position: local(b, 100.0, 80.0), delta: ScrollDelta::Pixels(point(px(0.0), px(100.0))), modifiers: Modifiers { platform: true, ..Default::default() }, ..Default::default() });
    draw(cx);
    let (v, _) = current(&zoom, cx);
    let want = z::zoom_wheel(rest, -100.0, WheelDeltaMode::Pixel, at, &frame);
    assert!(close(v, want), "wheel up zooms in about the cursor: {v:?} vs {want:?}");
    assert!(v.scale > rest.scale);
    // The image point under the cursor stays put.
    let before = ((at.x - rest.x) / rest.scale, (at.y - rest.y) / rest.scale);
    let after = ((at.x - v.x) / v.scale, (at.y - v.y) / v.scale);
    assert!((before.0 - after.0).abs() < 0.05 && (before.1 - after.1).abs() < 0.05, "{before:?} vs {after:?}");
    // A burst reports once, after it settled; the bound `scale` follows.
    assert!(log.named("zoomed").is_empty(), "not while the wheel moves");
    settle(cx, 200);
    let zoomed = log.named("zoomed");
    assert_eq!(zoomed.len(), 1, "{zoomed:?}");
    assert!((zoomed[0].context["scale"].as_f64().unwrap() - v.scale).abs() < 1e-6);
    assert_eq!(zoomed[0].context["atFit"], json!(false));
    let bound = view.read_with(cx, |v, _| v.surface().get_data("/zoom").cloned());
    assert!((bound.and_then(|b| b.as_f64()).unwrap() - v.scale).abs() < 1e-6, "two-way scale");
}

#[gpui::test]
fn a_plain_wheel_pans_only_with_room(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let b = painted(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    cx.simulate_event(ScrollEvent { position: local(b, 240.0, 150.0), delta: ScrollDelta::Pixels(point(px(-40.0), px(-40.0))), ..Default::default() });
    draw(cx);
    assert!(close(current(&zoom, cx).0, rest), "at fit there is no room: the host's wheel");
    // Zoomed to 100%, the same wheel pans by the deltas.
    zoom.update_in(cx, |z, window, cx| z.zoom_to_scale(1.0, window, cx));
    settle(cx, 400);
    let (zoomed, _) = current(&zoom, cx);
    cx.simulate_event(ScrollEvent { position: local(b, 240.0, 150.0), delta: ScrollDelta::Pixels(point(px(-40.0), px(-30.0))), ..Default::default() });
    draw(cx);
    let (v, _) = current(&zoom, cx);
    assert!(close(v, z::zoom_scroll(zoomed, 40.0, 30.0, &frame)), "{v:?}");
    assert!((v.scale - 1.0).abs() < 1e-9);
}

#[gpui::test]
fn a_double_click_toggles_rest_and_the_double_tap_scale_about_the_point(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let b = painted(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    let at = local(b, 120.0, 90.0);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.simulate_event(gpui::MouseDownEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 2, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 2 });
    settle(cx, 400);
    let (v, _) = current(&zoom, cx);
    let want = z::zoom_double_tap(rest, ZoomPoint { x: 120.0, y: 90.0 }, &frame);
    assert!(close(v, want) && (v.scale - 1.0).abs() < 1e-6, "max(100%, 2 × rest) about the point: {v:?} vs {want:?}");
    assert!(log.named("tapped").is_empty(), "a double click never taps");
    assert_eq!(log.named("zoomed").len(), 1);
    cx.simulate_event(gpui::MouseDownEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 2, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 2 });
    settle(cx, 400);
    assert!(close(current(&zoom, cx).0, rest), "zoomed → rest");
    assert_eq!(log.named("zoomed").last().unwrap().context["atFit"], json!(true));
}

#[gpui::test]
fn a_single_click_at_rest_taps_after_the_double_click_window(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let b = painted(&view, cx);
    cx.simulate_click(local(b, 200.0, 100.0), Modifiers::default());
    assert!(log.named("tapped").is_empty(), "waits for a second click");
    cx.executor().advance_clock(TAP_DELAY + Duration::from_millis(10));
    cx.run_until_parked();
    assert_eq!(log.named("tapped").len(), 1);
    // The click focused the picture: its tab stop is the surface's.
    assert_eq!(view.read_with(cx, |v, _| v.focused().and_then(|i| v.placed_nodes().get(i as usize).map(|n| n.id.clone()))), Some("zoomable".to_string()));
}

#[gpui::test]
fn the_keys_step_rest_and_actual_size_while_focused(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let b = painted(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    cx.simulate_click(local(b, 200.0, 100.0), Modifiers::default());
    draw(cx);
    cx.simulate_keystrokes("=");
    settle(cx, 400);
    let (up, _) = current(&zoom, cx);
    assert!(close(up, z::zoom_step(rest, 1, &frame)) && (up.scale - 0.5).abs() < 1e-6, "= steps up the ladder: {up:?}");
    cx.simulate_keystrokes("=");
    settle(cx, 400);
    cx.simulate_keystrokes("-");
    settle(cx, 400);
    assert!((current(&zoom, cx).0.scale - 0.5).abs() < 1e-6, "- steps down");
    cx.simulate_keystrokes("1");
    settle(cx, 400);
    let (one, _) = current(&zoom, cx);
    assert!((one.scale - 1.0).abs() < 1e-6, "1 = 100%: {one:?}");
    cx.simulate_keystrokes("right");
    draw(cx);
    assert!(close(current(&zoom, cx).0, one), "arrows stay the host's");
    cx.simulate_keystrokes("0");
    settle(cx, 400);
    assert!(close(current(&zoom, cx).0, rest), "0 = the rest view");
    assert_eq!(log.named("zoomed").len(), 5, "every settled step reports");
    // The surface's own key entry routes there too.
    let taken = view.update_in(cx, |v, window, cx| v.handle_key("+", false, Some("+"), window, cx));
    assert!(taken);
}

#[gpui::test]
fn the_pill_steps_fits_and_picks_from_its_menu(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    let pill = cx.debug_bounds("zoomable-image.pill").expect("controls: always draws the pill");
    let b = painted(&view, cx);
    // Bottom-centre, `pill.inset` above the picture's bottom edge.
    let (pill_mid, box_mid) = (f32::from(pill.center().x), f32::from(b.center().x));
    assert!((pill_mid - box_mid).abs() < 1.0, "centred: {pill:?} in {b:?}");
    assert!((f32::from(b.bottom() - pill.bottom()) - z::ZOOM.pill.inset as f32).abs() < 0.6 && (f32::from(pill.size.height) - z::ZOOM.pill.height as f32).abs() < 0.6, "{pill:?}");
    // − is disabled at the minimum.
    let minus = cx.debug_bounds("zoomable-image.zoomOut").unwrap();
    cx.simulate_click(minus.center(), Modifiers::default());
    settle(cx, 400);
    assert!(close(current(&zoom, cx).0, rest), "− at the min does nothing");
    let plus = cx.debug_bounds("zoomable-image.zoomIn").unwrap();
    cx.simulate_click(plus.center(), Modifiers::default());
    settle(cx, 400);
    assert!(close(current(&zoom, cx).0, z::zoom_step(rest, 1, &frame)), "+ steps about the centre");
    assert!(log.named("tapped").is_empty(), "a pill press is no tap");
    let fit = cx.debug_bounds("zoomable-image.fit").unwrap();
    cx.simulate_click(fit.center(), Modifiers::default());
    settle(cx, 400);
    assert!(close(current(&zoom, cx).0, rest), "fit = the rest view");
    // The percent opens THE Menu: Fit, 50%, 100%, 200%, 400%.
    let percent = cx.debug_bounds("zoomable-image.percent").unwrap();
    cx.simulate_click(percent.center(), Modifiers::default());
    draw(cx);
    assert!(zoom.read_with(cx, |z, _| z.menu_open()));
    let items = z::zoom_menu(rest, &frame, "Fit");
    assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Fit", "50%", "100%", "200%", "400%"]);
    let row = cx.debug_bounds("zoomable-image.menu.3").expect("the 200% row");
    assert!(row.bottom() <= percent.top(), "the menu opens above the pill: {row:?} over {percent:?}");
    cx.simulate_click(row.center(), Modifiers::default());
    settle(cx, 400);
    let (v, _) = current(&zoom, cx);
    assert!((v.scale - 2.0).abs() < 1e-6 && !zoom.read_with(cx, |z, _| z.menu_open()), "{v:?}");
    assert_eq!(log.named("zoomed").last().unwrap().context["scale"], json!(2.0));
    // + is disabled at the max.
    zoom.update_in(cx, |z, window, cx| z.zoom_to_scale(8.0, window, cx));
    settle(cx, 400);
    let plus = cx.debug_bounds("zoomable-image.zoomIn").unwrap();
    cx.simulate_click(plus.center(), Modifiers::default());
    settle(cx, 400);
    assert!((current(&zoom, cx).0.scale - 8.0).abs() < 1e-6);
}

#[gpui::test]
fn a_pinch_zooms_about_the_fingers_and_springs_back_inside_the_limits(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let b = painted(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    let at = local(b, 300.0, 120.0);
    let pinch = |delta: f32, phase: gpui::TouchPhase| gpui::PinchEvent { position: at, delta, modifiers: Modifiers::default(), phase };
    cx.simulate_event(pinch(0.0, gpui::TouchPhase::Started));
    cx.simulate_event(pinch(0.5, gpui::TouchPhase::Moved));
    cx.simulate_event(pinch(1.0 / 3.0, gpui::TouchPhase::Moved));
    draw(cx);
    let (v, _) = current(&zoom, cx);
    let want = z::zoom_pinch(rest, 1.5 * (4.0 / 3.0), ZoomPoint { x: 300.0, y: 120.0 }, &frame);
    assert!(close(v, want) && (v.scale - 2.0 / 3.0).abs() < 1e-4, "{v:?} vs {want:?}");
    cx.simulate_event(pinch(0.0, gpui::TouchPhase::Ended));
    settle(cx, 400);
    assert_eq!(log.named("zoomed").len(), 1, "a released pinch reports");
    // Out past the minimum: damped while held, back at fit once released.
    zoom.update_in(cx, |z, window, cx| z.reset(window, cx));
    settle(cx, 400);
    cx.simulate_event(pinch(0.0, gpui::TouchPhase::Started));
    cx.simulate_event(pinch(-0.5, gpui::TouchPhase::Moved));
    draw(cx);
    let (held, _) = current(&zoom, cx);
    assert!(held.scale < rest.scale && held.scale > rest.scale * 0.5, "rubber band below the min: {held:?}");
    cx.simulate_event(pinch(0.0, gpui::TouchPhase::Ended));
    settle(cx, 400);
    assert!(close(current(&zoom, cx).0, rest), "sprung back to fit");
}

#[gpui::test]
fn a_drag_pans_rubber_bands_past_the_edge_and_springs_back(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    zoom.update_in(cx, |z, window, cx| z.zoom_to_scale(1.0, window, cx));
    settle(cx, 400);
    let (start, frame) = current(&zoom, cx);
    let b = painted(&view, cx);
    let from = local(b, 240.0, 150.0);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    // Within the room: the picture follows the pointer.
    cx.simulate_mouse_move(from + point(px(-30.0), px(-20.0)), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert!(close(current(&zoom, cx).0, z::zoom_drag(start, -30.0, -20.0, &frame)));
    // Far past the top-left edge: damped.
    cx.simulate_mouse_move(from + point(px(800.0), px(600.0)), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    let (held, _) = current(&zoom, cx);
    assert!(close(held, z::zoom_drag(start, 800.0, 600.0, &frame)), "{held:?}");
    assert!(held.x > 0.0 && held.x < 800.0 + start.x, "rubber band: {held:?}");
    cx.simulate_mouse_up(from + point(px(800.0), px(600.0)), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(current(&zoom, cx).0.x > 0.0, "springs, not snaps");
    settle(cx, 400);
    let (released, _) = current(&zoom, cx);
    assert!(close(released, z::zoom_clamp(held, &frame)), "released = the clamped view: {released:?}");
}

#[gpui::test]
fn a_release_keeps_moving_and_stops_at_the_edges(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    zoom.update_in(cx, |z, window, cx| z.zoom_to_scale(1.0, window, cx));
    settle(cx, 400);
    let b = painted(&view, cx);
    let from = local(b, 240.0, 150.0);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    for k in 1..=4 {
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.simulate_mouse_move(from + point(px(-10.0 * k as f32), px(0.0)), Some(MouseButton::Left), Modifiers::default());
    }
    draw(cx);
    let (released, frame) = current(&zoom, cx);
    cx.simulate_mouse_up(from + point(px(-40.0), px(0.0)), MouseButton::Left, Modifiers::default());
    settle(cx, 2000);
    let (v, _) = current(&zoom, cx);
    assert!(v.x < released.x - 50.0, "momentum carries it on: {v:?} from {released:?}");
    assert!(close(v, z::zoom_clamp(v, &frame)), "never past an edge");
}

#[gpui::test]
fn the_bound_scale_zooms_about_the_centre(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, json!({}));
    let zoom = zoom_of(&view, cx);
    let (rest, frame) = current(&zoom, cx);
    view.update(cx, |v, cx| v.set_data("/zoom", Some(json!(2.0)), cx)).unwrap();
    draw(cx);
    draw(cx);
    let (v, _) = current(&zoom, cx);
    assert!(close(v, z::zoom_to(rest, 2.0, z::zoom_centre(&frame), &frame)), "{v:?}");
    assert!(log.named("zoomed").is_empty(), "the host's own write is no event");
}

#[gpui::test]
fn controls_auto_hides_after_the_last_activity_and_stays_while_hovered(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({"controls": "auto"}));
    let zoom = zoom_of(&view, cx);
    assert!(zoom.read_with(cx, |z, _| z.pill_visible()), "shown on appear");
    cx.executor().advance_clock(Duration::from_millis(z::ZOOM.auto_hide_ms as u64 + 50));
    draw(cx);
    assert!(!zoom.read_with(cx, |z, _| z.pill_visible()), "hidden after autoHideMs");
    let b = painted(&view, cx);
    cx.simulate_mouse_move(local(b, 100.0, 100.0), None, Modifiers::default());
    draw(cx);
    assert!(zoom.read_with(cx, |z, _| z.pill_visible()), "a pointer move shows it again");
    // Kept while hovered.
    let pill = cx.debug_bounds("zoomable-image.pill").unwrap();
    cx.simulate_mouse_move(pill.center(), None, Modifiers::default());
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(z::ZOOM.auto_hide_ms as u64 * 2));
    draw(cx);
    assert!(zoom.read_with(cx, |z, _| z.pill_visible()), "kept while hovered");
}

#[gpui::test]
fn controls_none_never_draws_the_pill(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({"controls": "none"}));
    let zoom = zoom_of(&view, cx);
    assert!(!zoom.read_with(cx, |z, _| z.pill_visible()));
    assert!(cx.debug_bounds("zoomable-image.pill").is_none());
    assert_eq!(zoom.read_with(cx, |z, _| z.frame().natural.width), 1440.0);
    let _ = ZoomControls::None;
}

#[gpui::test]
fn the_view_stands_on_its_own_for_the_ide(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let events: Rc<RefCell<Vec<ZoomableImageEvent>>> = Rc::default();
    let seen = events.clone();
    let (zoom, cx) = cx.add_window_view(move |_, cx| ZoomableImage::new("shot-1", None, Some((2000.0, 1000.0)), ZoomableImageOptions { alt: "Shot".into(), ..Default::default() }, cx));
    let target = zoom.clone();
    cx.update(move |_, cx| cx.subscribe(&target, move |_, ev: &ZoomableImageEvent, _| seen.borrow_mut().push(ev.clone())).detach());
    draw(cx);
    draw(cx);
    let (v, frame) = zoom.read_with(cx, |z, _| (z.view(), z.frame()));
    assert!(frame.viewport.width > 0.0 && frame.viewport.height > 0.0, "it measured its own box: {frame:?}");
    assert!(close(v, z::zoom_rest_view(&frame)));
    zoom.update_in(cx, |z, window, cx| z.press_part(ZoomPart::ZoomIn, window, cx));
    settle(cx, 400);
    assert!(matches!(events.borrow().last(), Some(ZoomableImageEvent::Zoom { at_fit: false, .. })), "{:?}", events.borrow());
    // A new picture = the rest view, at once.
    zoom.update(cx, |z, cx| z.set_source("shot-2", None, Some((500.0, 500.0)), cx));
    draw(cx);
    let (v, frame) = zoom.read_with(cx, |z, _| (z.view(), z.frame()));
    assert!(close(v, z::zoom_rest_view(&frame)), "{v:?}");
}
