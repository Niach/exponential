//! Round 8: the gpui Canvas painter against `fixtures/canvas.json` (the
//! compiled lists and packed v2 frames, the refusals, the fit matrices and
//! the pointer mapping) and through a headless window: native text set
//! above the raster by the baseline / align rule, the a11y image, the
//! pointer events (press, move at most once per frame step, leave), the
//! inert drawing, and a raster made only when something changed.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use eui_core::NestedNode;
use eui_gpui::canvas::{line_origin, pointer_point, text_anchor, CanvasPlayer};
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use eui_motion::canvas::{fit_matrix, Fit};
use eui_motion::display::{TextAlign, TextBaseline};
use gpui::{px, App, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use serde_json::{json, Map, Value};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn fixture() -> Value {
    serde_json::from_slice(&std::fs::read(repo().join("packages/exponential-ui/fixtures/canvas.json")).unwrap()).unwrap()
}

/// JSON equality with numbers compared to 1e-4 (f32 round trips).
fn close(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() <= 1e-4 * (1.0 + y.abs()), "{path}: {x} != {y}");
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{path}: length");
            for (i, (x, y)) in x.iter().zip(y).enumerate() {
                close(x, y, &format!("{path}/{i}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            assert_eq!(x.keys().collect::<Vec<_>>(), y.keys().collect::<Vec<_>>(), "{path}: keys");
            for (k, v) in x {
                close(v, &y[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

fn props_of(canvas: &Value) -> Map<String, Value> {
    canvas.as_object().unwrap().clone()
}

#[test]
fn every_compile_case_samples_the_fixture_list_and_packed_frame() {
    let f = fixture();
    let cases = f["compile"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let id = c["id"].as_str().unwrap();
        let mut player = CanvasPlayer::compile(&props_of(&c["canvas"])).unwrap_or_else(|e| panic!("{id}: {e}"));
        let slots = c.get("slots").and_then(Value::as_object).cloned().unwrap_or_default();
        player.set_slots(&slots);
        assert_eq!(json!(player.slot_ids()), c["slotIds"], "{id}: slotIds");
        close(&serde_json::to_value(player.list()).unwrap(), &c["list"], id);
        let mut packed = Vec::new();
        player.list().encode(&mut packed);
        assert_eq!(packed[0], 2.0, "{id}: packed VERSION 2");
        close(&json!(packed), &c["packed"], &format!("{id}/packed"));
        // The text ops are the tail, in source order.
        let first_text = player.list().ops.iter().position(|o| matches!(o, eui_motion::Op::Text { .. }));
        if let Some(i) = first_text {
            assert!(player.list().ops[i..].iter().all(|o| matches!(o, eui_motion::Op::Text { .. })), "{id}: text after every shape");
        }
        // The raster paints (skipping the text) at a device size.
        assert!(player.paint(64, 48, Fit::Contain).is_some(), "{id}: raster");
    }
}

#[test]
fn every_refuse_case_is_refused_with_its_codes_and_paths_in_order() {
    let f = fixture();
    for c in f["refuse"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let err = match CanvasPlayer::compile(&props_of(&c["canvas"])) {
            Ok(_) => panic!("{id}: compiled"),
            Err(e) => e,
        };
        let mut at = 0;
        for issue in c["issues"].as_array().unwrap() {
            let want = format!("{} {}:", issue["code"].as_str().unwrap(), issue["path"].as_str().unwrap());
            let found = err[at..].find(&want).unwrap_or_else(|| panic!("{id}: `{want}` missing (in order) from `{err}`"));
            at += found + want.len();
        }
    }
}

#[test]
fn fit_cases_map_the_drawing_origin_and_scale_the_text() {
    let f = fixture();
    for c in f["fit"].as_array().unwrap() {
        let (s, b) = (&c["size"], &c["box"]);
        let fit = Fit::parse(c["fit"].as_str().unwrap());
        let m = fit_matrix(s[0].as_f64().unwrap(), s[1].as_f64().unwrap(), b[0].as_f64().unwrap(), b[1].as_f64().unwrap(), fit);
        close(&json!(m), &c["matrix"], "fit");
        // A text op at the drawing origin anchors at the matrix's
        // translation, its size scaled by √|det|.
        let (x, y, size) = text_anchor(m, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 10.0);
        assert!((x as f64 - m[4]).abs() < 1e-4 && (y as f64 - m[5]).abs() < 1e-4);
        assert!((size as f64 - 10.0 * (m[0] * m[3]).abs().sqrt()).abs() < 1e-3);
    }
}

#[test]
fn pointer_cases_map_box_points_to_drawing_units() {
    let f = fixture();
    for c in f["pointer"].as_array().unwrap() {
        let (s, b) = (&c["size"], &c["box"]);
        let fit = Fit::parse(c["fit"].as_str().unwrap());
        for p in c["points"].as_array().unwrap() {
            let at = (p["at"][0].as_f64().unwrap() as f32, p["at"][1].as_f64().unwrap() as f32);
            let got = pointer_point((s[0].as_f64().unwrap() as f32, s[1].as_f64().unwrap() as f32), (b[0].as_f64().unwrap() as f32, b[1].as_f64().unwrap() as f32), fit, at).unwrap();
            close(&json!(got), &p["drawing"], "pointer");
        }
    }
}

#[test]
fn the_baseline_and_align_rule() {
    // ascent 10, descent 4, a 30 px line at (100, 50).
    assert_eq!(line_origin(100.0, 50.0, 30.0, 10.0, 4.0, TextAlign::Start, TextBaseline::Alphabetic), (100.0, 50.0));
    assert_eq!(line_origin(100.0, 50.0, 30.0, 10.0, 4.0, TextAlign::Center, TextBaseline::Top), (85.0, 60.0));
    assert_eq!(line_origin(100.0, 50.0, 30.0, 10.0, 4.0, TextAlign::End, TextBaseline::Middle), (70.0, 53.0));
    // A group scale of 2 under a contain fit of 0.5: the size stays.
    let (x, y, size) = text_anchor([0.5, 0.0, 0.0, 0.5, 0.0, 25.0], [2.0, 0.0, 0.0, 2.0, 10.0, 20.0], 12.0);
    assert_eq!((x, y, size), (5.0, 35.0, 12.0));
}

// --- Through the painter -------------------------------------------------

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Vec<ActionEvent>>>);

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.0.borrow_mut().push(event.clone());
    }
    fn open_url(&self, _url: &str, _cx: &mut App) {}
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn chart(pointer: bool, label: Option<&str>) -> Value {
    let mut props = json!({"size": [200, 100], "pointer": pointer, "shapes": [
        {"type": "rect", "position": [100, 50], "size": [200, 100]},
        {"type": "fill", "color": "$color.primary"},
        {"type": "text", "text": "Mon", "position": [100, 90], "size": 10, "align": "center", "baseline": "top", "color": "$color.mutedForeground"},
        {"type": "text", "text": "42", "position": [10, 20], "size": 12, "weight": "bold", "font": "mono"}
    ]});
    if let Some(l) = label {
        props["label"] = json!(l);
    }
    json!({"id": "cv", "component": "Canvas", "props": props, "on": {
        "press": {"event": {"name": "press"}}, "move": {"event": {"name": "move"}}, "leave": {"event": {"name": "leave"}}
    }})
}

fn surface(cx: &mut TestAppContext, child: Value) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    let tree = json!({"id": "root", "component": "Box", "style": {"width": 400, "padding": 20}, "children": [child, {"id": "after", "component": "Button", "props": {"label": "After"}}]});
    view.update(vcx, |v, cx| {
        v.record_bounds(true);
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    draw(vcx);
    draw(vcx);
    (view, log, vcx)
}

fn names(log: &Recorder) -> Vec<(String, Option<Value>)> {
    log.0.borrow().iter().map(|a| {
        let c = a.payload.as_ref().and_then(Value::as_object).or_else(|| a.context.as_object()).filter(|c| c.contains_key("x"));
        (a.name.clone(), c.map(|c| json!({"x": c.get("x"), "y": c.get("y")})))
    }).collect()
}

#[gpui::test]
fn a_canvas_paints_its_raster_and_sets_its_text_natively(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, chart(false, Some("Visits by day")));
    let b = view.read_with(cx, |v, _| v.painted_bounds("cv")).expect("painted");
    // 200 × 100 at the 360 px content width: as wide as its size (maxWidth 100%).
    assert_eq!((f32::from(b.size.width), f32::from(b.size.height)), (200.0, 100.0));
    assert_eq!(view.read_with(cx, |v, _| v.canvas_error("cv")), None);
    let list = view.read_with(cx, |v, _| v.canvas_list("cv")).unwrap();
    assert!(matches!(list.ops.last(), Some(eui_motion::Op::Text { .. })));
    let texts = view.read_with(cx, |v, _| v.canvas_texts("cv"));
    assert_eq!(texts.len(), 2, "{texts:?}");
    let mon = &texts[0];
    assert_eq!(mon.text, "Mon");
    assert!(mon.width > 0.0);
    // Centre-aligned on x = 100; baseline `top` = y + the font's ascent.
    assert!((mon.x + mon.width / 2.0 - 100.0).abs() < 0.01, "{mon:?}");
    // (an hhea ascent may exceed the em: Inter's is ~1.03 em.)
    assert!(mon.baseline > 90.0 && mon.baseline < 90.0 + 15.0, "{mon:?}");
    assert_eq!((mon.size, mon.mono, mon.weight), (10.0, false, 400));
    let n = &texts[1];
    assert_eq!((n.x, n.baseline, n.size, n.mono, n.weight), (10.0, 20.0, 12.0, true, 700));
    // a11y: an image named by its label, never a tab stop.
    let info = view.read_with(cx, |v, _| v.accessible_info("cv")).expect("an a11y node");
    assert_eq!(info.role, gpui::Role::Image);
    assert_eq!(info.label.as_ref().map(|s| s.to_string()), Some("Visits by day".to_string()));
    let first = cx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
    let first = view.read_with(cx, |v, _| first.and_then(|i| v.surface().layout_node(i).map(|n| n.id.clone())));
    assert_eq!(first.as_deref(), Some("after"), "the canvas is no tab stop");
}

#[gpui::test]
fn an_unlabelled_canvas_is_decorative(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, chart(true, None));
    assert!(view.read_with(cx, |v, _| v.accessible_info("cv")).is_none());
}

#[gpui::test]
fn the_raster_is_made_only_when_something_changed(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, chart(false, None));
    let once = view.read_with(cx, |v, _| v.canvas_rasters("cv")).unwrap();
    assert_eq!(once, 1);
    draw(cx);
    cx.update(|window, _| window.refresh());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.canvas_rasters("cv")), Some(1), "a plain repaint reuses the image");
    view.update(cx, |v, cx| v.set_mode(eui_core::theme::Mode::Dark, cx));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.canvas_rasters("cv")), Some(2), "a mode change resamples");
}

#[gpui::test]
fn a_pointer_canvas_fires_press_move_and_leave_in_drawing_units(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, chart(true, None));
    let b = view.read_with(cx, |v, _| v.painted_bounds("cv")).expect("painted");
    let at = |x: f32, y: f32| gpui::point(b.origin.x + px(x), b.origin.y + px(y));
    cx.simulate_mouse_down(at(50.0, 25.0), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at(50.0, 25.0), MouseButton::Left, Modifiers::default());
    draw(cx);
    let first = names(&log);
    assert!(first.iter().any(|(n, xy)| n == "press" && *xy == Some(json!({"x": 50.0, "y": 25.0}))), "{first:?}");
    log.0.borrow_mut().clear();
    // Two moves inside one frame step: the first fires, the last is held
    // for the step.
    cx.simulate_mouse_move(at(10.0, 10.0), None, Modifiers::default());
    cx.simulate_mouse_move(at(12.5, 11.0), None, Modifiers::default());
    let moved: Vec<_> = names(&log).into_iter().filter(|(n, _)| n == "move").collect();
    assert_eq!(moved, vec![("move".to_string(), Some(json!({"x": 10.0, "y": 10.0})))]);
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
    let moved: Vec<_> = names(&log).into_iter().filter(|(n, _)| n == "move").collect();
    assert_eq!(moved.len(), 2, "{moved:?}");
    assert_eq!(moved[1].1, Some(json!({"x": 12.5, "y": 11.0})));
    // Out of the box: `leave`, once.
    cx.simulate_mouse_move(gpui::point(px(390.0), px(390.0)), None, Modifiers::default());
    cx.simulate_mouse_move(gpui::point(px(395.0), px(395.0)), None, Modifiers::default());
    let left = names(&log).into_iter().filter(|(n, _)| n == "leave").count();
    assert_eq!(left, 1);
}

#[gpui::test]
fn a_canvas_without_pointer_is_inert(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, chart(false, None));
    let b = view.read_with(cx, |v, _| v.painted_bounds("cv")).expect("painted");
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    cx.simulate_mouse_down(b.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(b.center(), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(log.0.borrow().is_empty(), "{:?}", names(&log));
}

#[gpui::test]
fn a_refused_drawing_paints_nothing_and_reports(cx: &mut TestAppContext) {
    let bad = json!({"id": "cv", "component": "Canvas", "props": {"size": [10, 10], "shapes": [{"type": "merge"}]}});
    let (view, _log, cx) = surface(cx, bad);
    let e = view.read_with(cx, |v, _| v.canvas_error("cv")).expect("refused");
    assert!(e.contains("canvas-shape-type"), "{e}");
}
