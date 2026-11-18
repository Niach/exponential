//! #EXP-1947 (Exponential's `fixtures/image-actions.json`, the gpui part):
//! a host's image menu and source rule through the painter in a headless
//! gpui window. A RENDERED content picture (`Image`, a Markdown block
//! image) is handed to `HostPlugin::image_menu` and a right-click on it is
//! the image menu's, never the context Menu's around it (`pressMenu`); a
//! picture that has not rendered falls through to that Menu; a picture the
//! host's `image_source` refuses paints as a link (opened through the host)
//! or its alt, and is never fetched.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::{HostPlugin, ImageSourceVerdict, ImageTarget};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, AnyElement, App, Bounds, Entity, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

/// A 1 × 1 PNG (no network in tests).
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

#[derive(Clone, Default)]
struct Host {
    /// The srcs the image menu opened on.
    menus: Rc<RefCell<Vec<String>>>,
    opened: Rc<RefCell<Vec<String>>>,
    targets: Rc<RefCell<Vec<ImageTarget>>>,
}

impl HostPlugin for Host {
    fn image_source(&self, src: &str, alt: &str) -> Option<ImageSourceVerdict> {
        if src.starts_with("https://other.example/") {
            let text = if alt.is_empty() { "Image: other.example".to_string() } else { alt.to_string() };
            return Some(ImageSourceVerdict::Link { href: src.to_string(), text });
        }
        if src.starts_with("javascript:") {
            return Some(ImageSourceVerdict::Text(alt.to_string()));
        }
        Some(ImageSourceVerdict::Load)
    }

    /// A Markdown block image's box (the 1 × 1 picture drawn 200 × 100).
    fn image_size(&self, _src: &str, _cx: &App) -> Option<(f32, f32)> {
        Some((200.0, 100.0))
    }

    fn has_image_menu(&self, _cx: &App) -> bool {
        true
    }

    fn image_menu(&self, target: ImageTarget, picture: AnyElement, _window: &mut Window, _cx: &mut App) -> AnyElement {
        let (menus, src) = (self.menus.clone(), target.src.clone());
        self.targets.borrow_mut().push(target.clone());
        div()
            .id(target.id.clone())
            .size_full()
            .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                cx.stop_propagation();
                menus.borrow_mut().push(src.clone());
            })
            .child(picture)
            .into_any_element()
    }

    fn open_url(&self, url: &str, _cx: &mut App) {
        self.opened.borrow_mut().push(url.to_string());
    }
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(400.0)).child(self.view.clone())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn surface(cx: &mut TestAppContext, children: Value) -> (Entity<SurfaceView>, Host, &mut VisualTestContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 400}, "children": [
        {"id": "ctx", "component": "Menu", "props": {"openOn": "contextmenu", "items": [{"label": "Delete", "value": "delete"}]}, "children": [
            {"id": "col", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 400}, "children": children}
        ]}
    ]});
    cx.update(gpui_component::init);
    let host = Host::default();
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host: Rc::new(host.clone()), ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
        });
        Pane { view }
    });
    for _ in 0..3 {
        draw(vcx);
    }
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, host, vcx)
}

fn painted(view: &Entity<SurfaceView>, id: &str, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} painted"))
}

fn right_click(at: gpui::Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    draw(cx);
    draw(cx);
}

fn menu_open(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "ctx"))
}

#[gpui::test]
fn a_rendered_image_opens_the_hosts_image_menu_never_the_menu_around_it(cx: &mut TestAppContext) {
    let (view, host, cx) = surface(cx, json!([
        {"id": "pic", "component": "Image", "props": {"src": format!("{PNG}"), "alt": "Shot"}, "style": {"width": 120, "height": 80}},
        {"id": "gap", "component": "Box", "style": {"height": 40, "width": 400}}
    ]));
    let b = painted(&view, "pic", cx);
    right_click(b.center(), cx);
    assert_eq!(*host.menus.borrow(), vec![PNG.to_string()], "the image menu");
    assert!(!menu_open(&view, cx), "never merged with the context Menu around it");
    let target = host.targets.borrow().last().cloned().expect("a target");
    assert_eq!((target.src.as_str(), target.alt.as_str()), (PNG, "Shot"));
    assert_eq!(target.bytes().map(|b| b.len() > 8), Ok(true), "the original bytes through the media policy");
    // Elsewhere on the Menu's area: its own menu.
    let gap = painted(&view, "gap", cx);
    right_click(gap.center(), cx);
    assert!(menu_open(&view, cx), "the owner's menu");
    assert_eq!(host.menus.borrow().len(), 1);
}

#[gpui::test]
fn a_picture_that_has_not_rendered_falls_through_to_the_owner(cx: &mut TestAppContext) {
    let (view, host, cx) = surface(cx, json!([
        {"id": "pic", "component": "Image", "props": {"src": "https://nowhere.invalid/x.png", "alt": "Shot"}, "style": {"width": 120, "height": 80}}
    ]));
    let b = painted(&view, "pic", cx);
    right_click(b.center(), cx);
    assert!(host.menus.borrow().is_empty(), "no image menu on a picture still loading or failed");
    assert!(menu_open(&view, cx), "the press fell through to the owner's menu");
}

#[gpui::test]
fn another_websites_image_is_a_link_and_a_refused_one_its_alt(cx: &mut TestAppContext) {
    let (view, host, cx) = surface(cx, json!([
        {"id": "ext", "component": "Image", "props": {"src": "https://other.example/cat.png", "alt": ""}, "style": {"width": 200, "height": 40}},
        {"id": "md", "component": "Markdown", "props": {"text": "![A cat](https://other.example/b.png)\n\n![Script](javascript:alert(1))\n\n![](javascript:x)"}}
    ]));
    // The Image's stand-in: the link in its box, opened through the host.
    let b = painted(&view, "ext", cx);
    cx.simulate_click(b.origin + gpui::point(px(10.0), b.size.height / 2.0), Modifiers::default());
    assert_eq!(*host.opened.borrow(), vec!["https://other.example/cat.png".to_string()]);
    right_click(b.center(), cx);
    assert!(host.menus.borrow().is_empty(), "a link carries no image menu");
    // The Markdown: a link paragraph, then the alt as text; the empty alt draws nothing.
    let blocks = view.read_with(cx, |_, cx| eui_gpui::paint::markdown::parse_for(&host, "![A cat](https://other.example/b.png)\n\n![Script](javascript:alert(1))\n\n![](javascript:x)", cx));
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    assert_eq!(blocks[0].kind, eui_gpui::paint::markdown::BlockKind::Paragraph);
    assert_eq!((blocks[0].inlines[0].text.as_str(), blocks[0].inlines[0].link.as_deref()), ("A cat", Some("https://other.example/b.png")));
    assert_eq!((blocks[1].inlines[0].text.as_str(), blocks[1].inlines[0].link.as_deref()), ("Script", None));
    let md = painted(&view, "md", cx);
    cx.simulate_click(md.origin + gpui::point(px(12.0), px(8.0)), Modifiers::default());
    assert_eq!(host.opened.borrow().last().map(String::as_str), Some("https://other.example/b.png"), "the Markdown link opens through the host");
}

#[gpui::test]
fn a_rendered_markdown_image_opens_the_image_menu(cx: &mut TestAppContext) {
    let text = format!("![Shot]({PNG})");
    let (view, host, cx) = surface(cx, json!([{"id": "md", "component": "Markdown", "props": {"text": text}}]));
    let md = painted(&view, "md", cx);
    right_click(md.origin + gpui::point(px(20.0), px(20.0)), cx);
    assert_eq!(*host.menus.borrow(), vec![PNG.to_string()]);
    assert!(!menu_open(&view, cx));
}

#[test]
fn the_copy_fetches_the_original() {
    use eui_gpui::media::original_src;
    assert_eq!(original_src("/api/attachments/a?w=480"), "/api/attachments/a");
    assert_eq!(original_src("/api/attachments/a?x=1&w=480#f"), "/api/attachments/a?x=1#f");
    assert_eq!(original_src(PNG), PNG);
}
