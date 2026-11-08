//! FEED-200: a `swipeable` Tabs pages under a two-finger trackpad swipe
//! (gpui's phased `ScrollDelta::Pixels` wheel events, `view/swipe.rs`): the
//! panels move with the fingers, a fling settles on the neighbour, momentum
//! after the release never pages again, a vertical gesture and one that
//! starts over a node that pans itself (a Slider) leave the pager alone.

use std::rc::Rc;
use std::time::Duration;

use eui_core::NestedNode;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{px, Entity, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext};
use serde_json::{json, Value};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn surface(cx: &mut TestAppContext) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host: Rc::new(eui_gpui::host::NoHost), ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    let tree: Value = json!({"id": "root", "component": "Box", "style": {"padding": 20}, "children": [
        {"id": "swipe_tabs", "component": "Tabs", "props": {"swipeable": true, "tabs": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}, {"label": "Three", "value": "3"}], "value": "1"}, "style": {"width": 300}, "children": [
            {"id": "panel_one", "component": "Box", "style": {"height": 120}, "children": [{"id": "panel_one_text", "component": "Text", "props": {"text": "Panel one"}}]},
            {"id": "panel_two", "component": "Box", "style": {"height": 120}, "children": [{"id": "volume", "component": "Slider", "props": {"label": "Volume", "min": 0, "max": 100, "value": 40}}]},
            {"id": "panel_three", "component": "Box", "style": {"height": 120}, "children": [{"id": "panel_three_text", "component": "Text", "props": {"text": "Panel three"}}]}
        ]}
    ]});
    view.update(vcx, |v, cx| {
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
        v.record_bounds(true);
    });
    for _ in 0..3 {
        draw(vcx);
    }
    (view, vcx)
}

fn selected(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> Option<usize> {
    view.read_with(cx, |v, _| {
        (0..3).find(|i| {
            let index = v.index_of(&format!("swipe_tabs.tab.{i}")).unwrap();
            v.surface().layout_node(index).and_then(|n| n.part_query.as_ref()).is_some_and(|q| q.states.iter().any(|s| s == "selected"))
        })
    })
}

fn x_of(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> f32 {
    view.read_with(cx, |v, _| f32::from(v.painted_bounds(id).unwrap_or_else(|| panic!("{id} painted")).origin.x - v.origin().x))
}

fn wheel(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, dx: f32, dy: f32, phase: TouchPhase) {
    cx.simulate_event(ScrollWheelEvent { position: at, delta: ScrollDelta::Pixels(gpui::point(px(dx), px(dy))), touch_phase: phase, ..Default::default() });
    cx.executor().advance_clock(Duration::from_millis(16));
    draw(cx);
}

#[gpui::test]
fn a_horizontal_trackpad_swipe_pages_and_momentum_never_pages_again(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx);
    assert_eq!(selected(&view, cx), Some(0));
    let content = view.read_with(cx, |v, _| v.painted_bounds("swipe_tabs.content")).expect("the content painted");
    let rest = x_of(&view, cx, "panel_one");
    wheel(cx, content.center(), -4.0, 0.0, TouchPhase::Started);
    for _ in 0..3 {
        wheel(cx, content.center(), -30.0, 1.0, TouchPhase::Moved);
    }
    // The panel follows the fingers (natural scrolling: left = negative).
    let moved = x_of(&view, cx, "panel_one") - rest;
    assert!((moved - -94.0).abs() < 1.0, "the active panel is drawn at the offset ({moved})");
    assert!(x_of(&view, cx, "panel_two") < rest + 300.0, "the neighbour slides in");
    wheel(cx, content.center(), 0.0, 0.0, TouchPhase::Ended);
    assert_eq!(selected(&view, cx), Some(1), "the fling settled on the next tab");
    // The fling's momentum (phase-less `Moved`) never pages again.
    for _ in 0..10 {
        wheel(cx, content.center(), -40.0, 0.0, TouchPhase::Moved);
    }
    cx.executor().advance_clock(Duration::from_millis(600));
    draw(cx);
    assert_eq!(selected(&view, cx), Some(1), "momentum is not a swipe");
}

#[gpui::test]
fn a_slow_short_swipe_springs_back(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx);
    let content = view.read_with(cx, |v, _| v.painted_bounds("swipe_tabs.content")).unwrap();
    let rest = x_of(&view, cx, "panel_one");
    wheel(cx, content.center(), -2.0, 0.0, TouchPhase::Started);
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(120));
        wheel(cx, content.center(), -10.0, 0.0, TouchPhase::Moved);
    }
    cx.executor().advance_clock(Duration::from_millis(120));
    wheel(cx, content.center(), 0.0, 0.0, TouchPhase::Ended);
    assert_eq!(selected(&view, cx), Some(0), "42 px of 300, slowly: no page");
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(1000), cx));
    draw(cx);
    assert!((x_of(&view, cx, "panel_one") - rest).abs() < 0.5, "back at rest");
}

#[gpui::test]
fn vertical_gestures_and_ones_over_a_slider_leave_the_pager_alone(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx);
    let content = view.read_with(cx, |v, _| v.painted_bounds("swipe_tabs.content")).unwrap();
    wheel(cx, content.center(), 0.0, -6.0, TouchPhase::Started);
    for _ in 0..3 {
        wheel(cx, content.center(), -12.0, -30.0, TouchPhase::Moved);
    }
    wheel(cx, content.center(), 0.0, 0.0, TouchPhase::Ended);
    assert_eq!(selected(&view, cx), Some(0), "a vertical gesture is the page's");
    // A mouse wheel (lines) never swipes.
    cx.simulate_event(ScrollWheelEvent { position: content.center(), delta: ScrollDelta::Lines(gpui::point(-5.0, 0.0)), touch_phase: TouchPhase::Started, ..Default::default() });
    draw(cx);
    assert_eq!(selected(&view, cx), Some(0));
    // Page to the Slider's panel, then swipe starting over the Slider.
    wheel(cx, content.center(), -10.0, 0.0, TouchPhase::Started);
    wheel(cx, content.center(), -60.0, 0.0, TouchPhase::Moved);
    wheel(cx, content.center(), 0.0, 0.0, TouchPhase::Ended);
    assert_eq!(selected(&view, cx), Some(1));
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(1000), cx));
    cx.executor().advance_clock(Duration::from_millis(1000));
    draw(cx);
    let slider = view.read_with(cx, |v, _| v.painted_bounds("volume")).expect("the slider painted");
    wheel(cx, slider.center(), -10.0, 0.0, TouchPhase::Started);
    for _ in 0..3 {
        wheel(cx, slider.center(), -60.0, 0.0, TouchPhase::Moved);
    }
    wheel(cx, slider.center(), 0.0, 0.0, TouchPhase::Ended);
    assert_eq!(selected(&view, cx), Some(1), "a swipe starting over the Slider is the Slider's");
}
