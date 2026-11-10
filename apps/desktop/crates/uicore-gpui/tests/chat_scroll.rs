//! #EXP-1780: Exponential Desktop's chat (the factory `chat` document, sized
//! like the shell's panel) in a headless gpui window: a conversation longer
//! than the panel scrolls inside `chat_scroll`, clipped above the composer,
//! and the shell's FOLLOW (`follow_chat`) holds it at the newest row.

use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, Entity, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{follow_chat, inline, load, CHAT_SCROLL};

const PANEL: (f32, f32) = (800.0, 480.0);

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(PANEL.0)).h(px(PANEL.1)).child(self.view.clone())
    }
}

fn rows(n: usize) -> Value {
    Value::Array(
        (0..n)
            .map(|i| {
                let side = if i % 2 == 0 { "out" } else { "in" };
                json!({"id": format!("r{i}"), "kind": "user", "bubble": true, "side": side, "author": "Ada", "body": format!("Message number {i}, long enough to read as a real line of a conversation."), "gap": 8})
            })
            .collect(),
    )
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn mount(cx: &mut TestAppContext, count: usize) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut doc = inline::document(uicore_docs::doc("chat").expect("the factory chat"));
    // The shell sizes the root to its panel (`size_root`).
    inline::set_style(&mut doc, "chat", "width", json!(PANEL.0));
    inline::set_style(&mut doc, "chat", "height", json!(PANEL.1));
    let mut feed = Feed::new();
    feed.set("/chatting", json!({"face": "conversation", "title": "A long chat", "rows": rows(count), "composer": {"placeholder": "Reply", "busy": false}}));
    let theme = eui_core::themes::builtin_theme("exponential");
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let options = SurfaceViewOptions { surface_id: "chat".into(), theme, mode: eui_core::theme::Mode::Dark, ..Default::default() };
        let view = cx.new(|cx| {
            let mut v = SurfaceView::new(options, window, cx);
            v.record_bounds(true);
            load(&mut v, &doc, &feed, cx);
            follow_chat(&mut v, cx);
            v
        });
        Pane { view }
    });
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

/// The rows are their own scroll container (eUI's rule: `overflowY` auto
/// or scroll); without it the shell paints them unclipped under the
/// composer and past the panel's edge.
#[test]
fn the_rows_are_a_scroll_container() {
    let doc = uicore_docs::doc("chat").expect("the factory chat").json();
    let mut walk = vec![&doc["root"]];
    let list = loop {
        let node = walk.pop().expect("chat_scroll in the document");
        if node["id"] == CHAT_SCROLL {
            break node;
        }
        walk.extend(node["children"].as_array().into_iter().flatten());
    };
    let style = list["props"]["style"].as_object().expect("the list's style");
    assert_eq!(eui_core::style::is_scroll_container(style), (false, true), "{style:?}");
}

#[gpui::test]
fn a_long_conversation_scrolls_above_the_composer_at_its_newest_row(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx, 60);
    settle(cx);
    let (offset, viewport, content) = view.read_with(cx, |v, _| v.scroll_extent(CHAT_SCROLL)).expect("the rows scroll on their own");
    assert!(content > viewport * 2.0, "60 rows overflow the panel: content {content}, viewport {viewport}");
    assert!(viewport < PANEL.1, "the rows' viewport sits inside the panel: {viewport}");
    assert!((offset - (content - viewport)).abs() < 0.5, "held at the newest row: offset {offset} of {}", content - viewport);
    let list = view.read_with(cx, |v, _| v.painted_bounds(CHAT_SCROLL)).expect("the rows painted");
    let composer = view.read_with(cx, |v, _| v.painted_bounds("chat_composer")).expect("the composer painted");
    assert!(list.bottom() <= composer.top(), "the rows end above the composer: {list:?} vs {composer:?}");
    assert!(f32::from(composer.bottom()) <= PANEL.1 + 0.5, "the composer stays inside the panel: {composer:?}");
}
