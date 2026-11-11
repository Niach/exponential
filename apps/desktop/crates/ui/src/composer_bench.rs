//! #EDE-65 lane T: THE session composer as the shared chat document draws it
//! (`apps/ede/ui/chat.json`'s `chat_replies` + `chat_composer`, genUI's flat
//! Composer under the exponential theme) against the IDE's own
//! (`composer::session_composer_card`, the "+" tool, a context ring's box,
//! Send, the quick replies over it), drawn HEADLESS with the real text
//! system, pixel for pixel. Run with
//! `EXP_COMPOSER_SHOTS=<dir> cargo test -p ui --release -- --ignored composer_bench --nocapture`:
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

const WINDOW: (f32, f32) = (640., 220.);
/// The frame's padding around the composer column.
const ORIGIN: f32 = 24.;
/// The context ring's box (THE ring button, `web_icon_xs`), a flat swatch
/// on both sides: the bench measures the geometry, not the ring.
const RING: f32 = 24.;
/// `$color.info` in the exponential dark palette (a document colour is a token).
const RING_FILL: u32 = 0x3b82f6;
const PLACEHOLDER: &str = "Reply…";
const REPLIES: [&str; 3] = ["Yes, go ahead", "Show me the plan first", "No"];

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// The empty card: "+" · the field · the ring + Send.
    Card,
    /// The quick replies over the card.
    Replies,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Card => "card",
            State::Replies => "replies",
        }
    }
}

struct IdeFrame {
    state: State,
    mention: Entity<crate::mention_input::MentionInput>,
}

impl Render for IdeFrame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use crate::composer::{composer_tool, session_composer_card, SessionComposerBox, SessionComposerControls, SessionComposerSubmit};
        let plus = composer_tool("bench-plus", crate::icons::registry::UI_ADD, cx).tooltip(crate::chat_screen::PLUS_TOOLTIP);
        let ring: AnyElement = div().size(px(RING)).bg(gpui::rgb(RING_FILL)).into_any_element();
        let replies = (self.state == State::Replies).then(|| {
            let replies = domain::session_tree::ComposerReplies {
                message_id: "m1".into(),
                items: REPLIES.iter().enumerate().map(|(i, text)| domain::session_tree::ComposerReply { id: format!("r{i}"), text: (*text).into() }).collect(),
            };
            crate::composer::composer_replies_row(&replies, false, cx)
        });
        let card = session_composer_card(
            SessionComposerBox::new(self.mention.clone(), SessionComposerSubmit::send("bench-send", true, |_, _, _| {}))
                .tool(plus)
                .controls(SessionComposerControls { device_pin: None, model_pin: None, context_ring: Some(ring) })
                .replies(replies),
            cx,
        );
        div().size_full().bg(cx.theme().background).p(px(ORIGIN)).child(div().w_full().flex().flex_col().justify_end().h_full().child(card))
    }
}

struct EuiFrame {
    surface: Entity<SurfaceView>,
}

impl Render for EuiFrame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background).p(px(ORIGIN)).child(div().w_full().flex().flex_col().justify_end().h_full().child(self.surface.clone()))
    }
}

/// The node `id` of THE chat document.
fn chat_node(id: &str) -> Value {
    fn find(node: &Value, id: &str) -> Option<Value> {
        match node {
            Value::Object(map) if map.get("id").and_then(Value::as_str) == Some(id) => Some(node.clone()),
            Value::Object(map) => map.values().find_map(|v| find(v, id)),
            Value::Array(items) => items.iter().find_map(|v| find(v, id)),
            _ => None,
        }
    }
    let doc: Value = serde_json::from_str(include_str!("../../../../ede/ui/chat.json")).expect("chat.json");
    find(&doc, id).unwrap_or_else(|| panic!("chat.json has no {id}"))
}

/// The document's footer pieces the IDE draws: the replies and the
/// composer, its ring Embed swapped for the swatch.
fn eui_tree() -> Value {
    let mut composer = chat_node("chat_composer");
    composer["props"]["placeholder"] = json!(PLACEHOLDER);
    // The ring's Embed keeps its document style (its box, its gap to Send).
    let mut ring_style = composer["slots"]["trailing"]["style"].clone();
    ring_style["backgroundColor"] = json!("$color.info");
    composer["slots"]["trailing"] = json!({
        "id": "chat_ring",
        "component": "Box",
        "style": ring_style,
    });
    json!({
        "id": "bench_footer",
        "component": "Box",
        "style": {"display": "flex", "flexDirection": "column", "alignItems": "stretch", "width": "100%", "gap": chat_node("chat_footer")["style"]["gap"].clone()},
        "children": [chat_node("chat_slash_popover"), chat_node("chat_replies"), composer],
    })
}

fn eui_data(state: State) -> Value {
    let replies: Vec<Value> = REPLIES.iter().enumerate().map(|(i, text)| json!({"key": format!("r{i}"), "label": text, "messageId": "m1"})).collect();
    json!({
        "composer": {
            "plus": true,
            "ring": true,
            "busy": false,
            "repliesMessageId": if state == State::Replies { "m1" } else { "" },
            "replies": if state == State::Replies { replies } else { Vec::new() },
        }
    })
}

fn eui_surface(state: State, window: &mut Window, cx: &mut gpui::App) -> Entity<SurfaceView> {
    // The IDE's own mount (`chat_tab_composer::ChatTabComposer::mount`):
    // the document theme, dark, the app's markdown host's icons.
    let host = Rc::new(crate::markdown::AppMarkdownHost::for_document(None, cx));
    let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
    cx.new(|cx| {
        let settings = eui_core::surface::SurfaceSettings { locale: "en-US".into(), mode: eui_core::theme::ModeSetting::Dark, ..Default::default() };
        let options = SurfaceViewOptions {
            surface_id: "composer-bench".to_string(),
            theme: Some(theme),
            mode: Mode::Dark,
            host,
            settings: Some(settings),
            lift_layers: true,
            ..SurfaceViewOptions::default()
        };
        let mut view = SurfaceView::new(options, window, cx);
        view.set_data("/chatting", Some(eui_data(state)), cx).expect("the chat data");
        view.set_data("/chat", Some(json!({"draft": ""})), cx).expect("the draft");
        view.set_data("/chatMenu", Some(json!({"open": false, "rows": []})), cx).expect("the menu");
        let tree: NestedNode = serde_json::from_value(eui_tree()).expect("the bench tree");
        let outcome = view.set_nested(tree, cx);
        assert!(outcome.issues.is_empty(), "{:?}", outcome.issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        view
    })
}

fn shoot(dir: &std::path::Path, prefix: &str, state: State) -> image::RgbaImage {
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut eui = None;
    let handle = cx
        .open_window(size(px(WINDOW.0), px(WINDOW.1)), |window: &mut Window, cx| {
            let view: gpui::AnyView = if prefix == "ide" {
                let (_, mention) = crate::composer::session_composer_field(PLACEHOLDER, window, cx);
                cx.new(|_| IdeFrame { state, mention }).into()
            } else {
                let surface = eui_surface(state, window, cx);
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
    if let Some(surface) = eui.as_ref() {
        cx.update(|cx| surface.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx)));
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(dir.join(format!("{prefix}-{}.png", state.name()))).expect("the PNG");
    let image = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
    std::mem::forget(cx);
    image
}

fn compare(state: State) {
    let Some(dir) = std::env::var_os("EXP_COMPOSER_SHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    let ide = shoot(&dir, "ide", state);
    let eui = shoot(&dir, "eui", state);
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
    eprintln!("{}: {count} pixels differ (worst level {worst}), within {bounds:?}", state.name());
    diff.save(dir.join(format!("{}.diff.png", state.name()))).expect("the diff");
}

/// The bench's document is THE chat document's footer: it parses, and the
/// pieces it lifts are where the bench expects them.
#[test]
fn composer_bench_tree_is_the_chat_documents() {
    let tree = eui_tree();
    let _: NestedNode = serde_json::from_value(tree.clone()).expect("a genUI tree");
    assert_eq!(tree["children"][2]["component"], "Composer");
    assert_eq!(tree["children"][2]["props"]["layout"], "flat");
    assert_eq!((tree["children"][0]["id"].as_str(), tree["children"][1]["id"].as_str()), (Some("chat_slash_popover"), Some("chat_replies")));
    let ring = &tree["children"][2]["slots"]["trailing"]["style"];
    assert_eq!((ring["width"].as_f64(), ring["height"].as_f64()), (Some(RING as f64), Some(RING as f64)));
}

#[test]
#[ignore]
fn composer_bench_card() {
    compare(State::Card);
}

#[test]
#[ignore]
fn composer_bench_replies() {
    compare(State::Replies);
}

/// #EDE-65 lane T: the `/` menu opening (a Popover anchored on the composer,
/// opened by data) leaves the composer's field focused: its caret stays.
#[test]
#[ignore]
fn composer_bench_menu_keeps_the_field_focused() {
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut eui = None;
    let handle = cx
        .open_window(size(px(WINDOW.0), px(WINDOW.1 + 240.)), |window: &mut Window, cx| {
            let surface = eui_surface(State::Card, window, cx);
            eui = Some(surface.clone());
            let view: gpui::AnyView = cx.new(|_| EuiFrame { surface }).into();
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let surface = eui.expect("the surface");
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let focused = |cx: &mut gpui::HeadlessAppContext| {
        cx.update_window(window, |_, window, cx| surface.read(cx).field_focus_handle("chat_composer", cx).is_some_and(|h| h.is_focused(window))).expect("the window")
    };
    cx.update_window(window, |_, window, cx| surface.update(cx, |view, cx| assert!(view.focus_field("chat_composer", window, cx), "the composer field"))).expect("focus");
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    assert!(focused(&mut cx), "focused before the menu");
    let row = |key: &str| domain::chat_document::plain_row(json!({"key": key, "command": format!("/{key}"), "description": key, "hint": "", "icon": null, "insert": format!("/{key}"), "prompt": "", "selected": false}));
    // The host's round trip after a typed `/`: the draft written back, the
    // menu opened (`chat_tab_composer::ComposerDoc::element`).
    cx.update(|cx| {
        surface.update(cx, |view, cx| {
            view.set_data("/chat", Some(json!({"draft": "/"})), cx).expect("the draft");
            view.set_data("/chatMenu", Some(json!({"open": true, "rows": [row("compact"), row("new")]})), cx).expect("the menu");
        })
    });
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    assert!(focused(&mut cx), "the open / menu left the composer's field focused");
    std::mem::forget(cx);
}

/// #EDE-65 lane T: a draft's round trip into the hidden draft state
/// (`chat_tab_composer::mirror_draft`) leaves the drawn field focused, where
/// gpui-base's `set_cursor_position` alone moves the focus to that state.
#[test]
#[ignore]
fn composer_bench_a_draft_round_trip_keeps_the_field_focused() {
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut eui = None;
    let mut hidden = None;
    let handle = cx
        .open_window(size(px(WINDOW.0), px(WINDOW.1 + 240.)), |window: &mut Window, cx| {
            let surface = eui_surface(State::Card, window, cx);
            eui = Some(surface.clone());
            let (input, _) = crate::composer::session_composer_field(PLACEHOLDER, window, cx);
            hidden = Some(input);
            let view: gpui::AnyView = cx.new(|_| EuiFrame { surface }).into();
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let (surface, hidden) = (eui.expect("the surface"), hidden.expect("the draft state"));
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let field = cx.update(|cx| surface.read(cx).field_focus_handle("chat_composer", cx)).expect("the field");
    let focused = |cx: &mut gpui::HeadlessAppContext| cx.update_window(window, |_, window, _| field.is_focused(window)).expect("the window");
    // The keystroke path: the field focused, `/` typed into it, flushed.
    cx.update_window(window, |_, window, cx| {
        surface.update(cx, |view, cx| {
            assert!(view.focus_field("chat_composer", window, cx));
            assert!(view.type_into("chat_composer", "/", window, cx));
            view.flush_field("chat_composer", None, window, cx);
        })
    })
    .expect("typed");
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    assert!(focused(&mut cx), "focused after the keystroke");
    // The host's round trip: the draft mirrored into its state, then the
    // menu opened and the draft written back.
    let keep = Some(field.clone());
    cx.update_window(window, |_, window, cx| crate::steer_viewer::chat_tab_composer::mirror_draft(&hidden, "/".into(), keep, window, cx)).expect("mirrored");
    let row = |key: &str| domain::chat_document::plain_row(json!({"key": key, "command": format!("/{key}"), "description": key, "hint": "", "icon": null, "insert": format!("/{key}"), "prompt": "", "selected": false}));
    cx.update(|cx| {
        surface.update(cx, |view, cx| {
            view.set_data("/chat", Some(json!({"draft": "/"})), cx).expect("the draft");
            view.set_data("/chatMenu", Some(json!({"open": true, "rows": [row("compact"), row("new")]})), cx).expect("the menu");
        })
    });
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    assert!(focused(&mut cx), "the field keeps the focus through the host's round trip");
    // Without the restore, gpui-base's cursor move takes it (the bug).
    cx.update_window(window, |_, window, cx| crate::steer_viewer::chat_tab_composer::mirror_draft(&hidden, "/c".into(), None, window, cx)).expect("mirrored");
    assert!(!focused(&mut cx), "set_cursor_position alone moves the focus");
    std::mem::forget(cx);
}
