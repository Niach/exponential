//! The Window macro (VAPP-290) through the painter in a headless gpui
//! window: a pure macro over Box / Text, so nothing here is painter code.
//! The 34 px chrome strip at the top (three 10 x 10 dots at the start, the
//! optional title centred in the WHOLE window, a flex spacer at the end), the
//! body filling the rest with the one child inside it, and the neutral
//! recipes (card fill, hairline border, `$radius.lg`, the dots in
//! mutedForeground at `$opacity.quaternary`).

use eui_core::NestedNode;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, Entity, TestAppContext, VisualTestContext};
use serde_json::Value;

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

fn surface(cx: &mut TestAppContext, tree: Value) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, ..Default::default() };
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
    (view, vcx)
}

fn frame(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> eui_core::surface::Frame {
    view.read_with(cx, |v, _| v.frame(v.index_of(id).unwrap_or_else(|| panic!("no {id}"))).unwrap())
}

fn painted(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> gpui::Bounds<gpui::Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} not painted"))
}

fn style(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> eui_gpui::paint::PaintStyle {
    view.read_with(cx, |v, _| v.painted_style(id)).unwrap_or_else(|| panic!("{id} has no painted style"))
}

fn text_of(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> String {
    view.read_with(cx, |v, _| v.surface().layout_node(v.index_of(id).unwrap()).and_then(|n| n.props.get("text").and_then(Value::as_str).map(str::to_string))).unwrap_or_default()
}

fn exists(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> bool {
    view.read_with(cx, |v, _| v.placed_nodes().iter().any(|n| n.id == id && !n.removed))
}

fn specimen(id: &str) -> Value {
    let doc: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/specimens.json")).unwrap();
    doc["specimens"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap_or_else(|| panic!("no specimen {id}"))["node"].clone()
}

fn hex(c: gpui::Hsla) -> String {
    let r = gpui::Rgba::from(c);
    format!("#{:02x}{:02x}{:02x}{:02x}", (r.r * 255.0).round() as u8, (r.g * 255.0).round() as u8, (r.b * 255.0).round() as u8, (r.a * 255.0).round() as u8)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.6
}

#[gpui::test]
fn the_window_specimen_paints_the_chrome_strip_the_dots_the_centred_title_and_the_body(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, specimen("exponential-ui-window"));
    let unknown = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| !n.removed && n.component == "Unknown").count());
    assert_eq!(unknown, 0);

    let root = frame(&view, cx, "c0_window");
    assert!(root.w > 0.0 && near(root.h, 200.0), "the specimen's height 200: {root:?}");

    // The chrome: 34 tall, at the top, inside the hairline border.
    let chrome = frame(&view, cx, "c0_window.chrome");
    assert!(near(chrome.h, 34.0), "a 34 px strip: {chrome:?}");
    assert!(near(chrome.y, root.y + 1.0) && near(chrome.x, root.x + 1.0) && near(chrome.w, root.w - 2.0), "at the top, inside the border: {chrome:?} in {root:?}");
    painted(&view, cx, "c0_window.chrome");
    assert!(view.read_with(cx, |v, _| v.accessible_info("c0_window.chrome")).is_none(), "the chrome is hidden from AT");

    // Three 10 x 10 dots, the first after the border + $spacing.md, 6 apart.
    let mut prev: Option<eui_core::surface::Frame> = None;
    for i in 0..3 {
        let id = format!("c0_window.chrome.dots.dot.{i}");
        let d = frame(&view, cx, &id);
        assert!(near(d.w, 10.0) && near(d.h, 10.0), "{id}: 10 x 10: {d:?}");
        assert!(near(d.y + d.h / 2.0, chrome.y + chrome.h / 2.0) || near(d.y + d.h / 2.0, chrome.y + (chrome.h - 1.0) / 2.0), "{id}: centred in the strip: {d:?} in {chrome:?}");
        match prev {
            None => assert!(near(d.x, root.x + 1.0 + 12.0), "{id}: at the start, after $spacing.md: {d:?} in {root:?}"),
            Some(p) => assert!(near(d.x, p.x + p.w + 6.0), "{id}: 6 px after the previous: {d:?}"),
        }
        painted(&view, cx, &id);
        let st = style(&view, cx, &id);
        assert_eq!(st.bg.map(hex).as_deref(), Some("#737373ff"), "{id}: mutedForeground");
        assert!(st.opacity.is_some_and(|o| (o - 0.3).abs() < 0.01), "{id}: $opacity.quaternary: {:?}", st.opacity);
        assert!(st.radii[0] >= 5.0, "{id}: a full radius: {:?}", st.radii);
        prev = Some(d);
    }
    assert!(!exists(&view, cx, "c0_window.chrome.dots.dot.3"), "range(3) = three dots");

    // The title: centred in the WHOLE window (the two flex-1 sides balance it).
    assert_eq!(text_of(&view, cx, "c0_window.chrome.title"), "exponential.at");
    let title = frame(&view, cx, "c0_window.chrome.title");
    assert!(near(title.x + title.w / 2.0, root.x + root.w / 2.0), "the title centred in the root: {title:?} in {root:?}");
    assert!(title.y >= chrome.y && title.y + title.h <= chrome.y + chrome.h + 0.5, "inside the strip: {title:?}");
    painted(&view, cx, "c0_window.chrome.title");
    assert_eq!(style(&view, cx, "c0_window.chrome.title").color.map(hex).as_deref(), Some("#737373ff"), "the title in mutedForeground");

    // The body: below the chrome, filling the rest; the child inside it.
    let body = frame(&view, cx, "c0_window.body");
    assert!(near(body.y, chrome.y + chrome.h), "the body right below the chrome: {body:?}");
    assert!(near(body.y + body.h, root.y + root.h - 1.0) && near(body.w, root.w - 2.0), "the body fills the rest: {body:?} in {root:?}");
    let child = frame(&view, cx, "c0_body");
    assert!(child.x >= body.x - 0.5 && child.y >= body.y - 0.5 && child.x + child.w <= body.x + body.w + 0.5 && child.y + child.h <= body.y + body.h + 0.5, "the child inside the body: {child:?} in {body:?}");
    painted(&view, cx, "c0_body_open");

    // The root's painted recipe: the card, a hairline border, $radius.lg, clipped, a shadow.
    let st = style(&view, cx, "c0_window");
    assert_eq!(st.border, [1.0; 4], "a hairline border");
    assert_eq!(st.radii, [10.0; 4], "$radius.lg");
    assert!(st.bg.is_some(), "the card fill");
    assert!(st.clip_x && st.clip_y, "overflow hidden");
    assert!(!st.shadows.is_empty(), "$shadow.md");
    assert_eq!(style(&view, cx, "c0_window.chrome").border[2], 1.0, "the chrome's hairline bottom border");
    let info = view.read_with(cx, |v, _| v.accessible_info("c0_window")).expect("a group");
    assert_eq!((info.role, info.label.as_deref()), (gpui::Role::Group, Some("exponential.at")), "a group named by its title");
}

#[gpui::test]
fn an_untitled_window_has_no_title_node_and_keeps_its_dots(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, specimen("exponential-ui-window"));
    assert!(!exists(&view, cx, "c1_untitled.chrome.title"), "no title prop = no title node");
    assert!(exists(&view, cx, "c1_untitled.chrome.end"));
    let (root, chrome, body) = (frame(&view, cx, "c1_untitled"), frame(&view, cx, "c1_untitled.chrome"), frame(&view, cx, "c1_untitled.body"));
    assert!(near(root.h, 160.0) && near(chrome.h, 34.0), "{root:?} {chrome:?}");
    assert!(near(body.y, chrome.y + chrome.h) && near(body.y + body.h, root.y + root.h - 1.0), "the body fills the rest: {body:?}");
    let d0 = frame(&view, cx, "c1_untitled.chrome.dots.dot.0");
    assert!(near(d0.x, root.x + 13.0), "{d0:?}");
    painted(&view, cx, "c1_untitled.chrome.dots.dot.2");
}
