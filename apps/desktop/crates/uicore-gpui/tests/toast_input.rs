//! #EDE-135: the factory toasts in a headless gpui window. The layer is a
//! fixed 380x420 box with a transparent root; a toast card is a strip at
//! its top. Input belongs to the cards alone: a press beside them (on the
//! window under the layer, its close button, its toolbar) is not the shell's.

use std::rc::Rc;

use eui_gpui::host::HostPlugin;
use gpui::{px, Bounds, Pixels, TestAppContext, VisualTestContext};
use serde_json::json;
use uicore_data::Feed;
use uicore_gpui::{Region, Surfaces, WindowedHost};

struct Plugin;
impl HostPlugin for Plugin {}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn feed(toasts: usize) -> Feed {
    let toasts: Vec<_> = (0..toasts).map(|i| json!({"id": i + 1, "app": "Thunderbird", "summary": "New mail", "body": "From Ada", "icon": "", "actions": [], "at": "12:00"})).collect();
    let mut feed = Feed::new();
    feed.set("/notifications", json!({"toasts": toasts, "list": [], "count": 0}));
    feed
}

fn toasts(feed: &Feed, cx: &mut TestAppContext) -> (Surfaces, Region, VisualTestContext) {
    cx.update(gpui_component::init);
    let source = uicore_docs::doc("notifications").expect("the factory toasts").json();
    let mut doc = ede_ui::validate_document("notifications", &source).expect("the document validates");
    // As the shell opens it: the root takes the layer's size.
    if let ede_ui::Body::Nested(root) = &mut doc.body {
        let style = root.style.get_or_insert_with(Default::default);
        style.insert("width".into(), json!(380));
        style.insert("height".into(), json!(420));
    }
    let mut surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(|_| Rc::new(Plugin) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    assert!(cx.update(|cx| surfaces.open_layer(&doc, feed, cx)), "the toasts opened");
    let region = surfaces.region("notifications").cloned().expect("the toasts' region");
    let mut vcx = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut vcx);
    draw(&mut vcx);
    (surfaces, region, vcx)
}

fn contains(rects: &[Bounds<Pixels>], x: f32, y: f32) -> bool {
    rects.iter().any(|r| r.contains(&gpui::point(px(x), px(y))))
}

#[gpui::test]
fn one_toast_is_a_strip_at_the_top_of_its_layer(cx: &mut TestAppContext) {
    let (_surfaces, region, mut vcx) = toasts(&feed(1), cx);
    let card = region.view.read_with(&vcx, |v, _| v.painted_bounds("toast.1").or_else(|| v.painted_bounds("toast"))).expect("the card is drawn");
    let window = vcx.update(|window, _| window.viewport_size());
    assert!(f32::from(card.size.height) < 200.0 && f32::from(window.height) >= 400.0, "card {card:?} in a window of {window:?}");
    assert!(!contains(&[card], 190.0, 300.0), "the point beside the card");
}

/// The window takes input where the cards are, nowhere else.
#[gpui::test]
fn the_toasts_window_takes_input_on_its_cards_alone(cx: &mut TestAppContext) {
    let (surfaces, region, mut vcx) = toasts(&feed(1), cx);
    let input = region.input.clone().expect("the toasts clip their input");
    let rects = input.borrow().clone().expect("the window was given a region");
    assert_eq!(rects.len(), 1, "{rects:?}");
    let card = region.view.read_with(&vcx, |v, _| v.painted_bounds("toast.1").or_else(|| v.painted_bounds("toast"))).expect("the card is drawn");
    assert!(contains(&rects, f32::from(card.center().x), f32::from(card.center().y)), "a press on the card is the shell's");
    assert!(!contains(&rects, 190.0, 300.0), "a press beside the card is the window's under it: {rects:?}");

    // A second toast: two cards take input.
    cx.update(|cx| surfaces.feed_all(&feed(2), cx));
    draw(&mut vcx);
    assert_eq!(input.borrow().as_ref().map(Vec::len), Some(2));
}

/// Only a layer whose root paints nothing clips its input: the toasts.
#[test]
fn only_the_toasts_clip_their_input() {
    let clipping: Vec<&str> = uicore_docs::DOCS
        .iter()
        .filter(|d| ede_ui::validate_document(d.name, &d.json()).is_ok_and(|doc| uicore_gpui::clips_input(&doc)))
        .map(|d| d.name)
        .collect();
    assert_eq!(clipping, ["notifications"]);
}
