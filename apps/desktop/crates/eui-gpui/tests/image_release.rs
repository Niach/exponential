//! #EXP-2024 (6a) through the painter in a headless gpui window: an Image
//! whose `src` moves off a `data:` url releases the old picture, its asset
//! in gpui's cache and its tile in the window's sprite atlas. A live
//! picture fed as one `data:` url a frame (the shell's switcher and taskbar
//! previews, a pinned space view) kept every frame for the app's life
//! before: a pinned view at 4 frames a second leaked gigabytes an hour.

use std::rc::Rc;
use std::sync::Arc;

use eui_core::NestedNode;
use eui_gpui::host::HostPlugin;
use eui_gpui::media::{held_data_pictures, image_key, MediaKey, MediaLoader};
use eui_gpui::paint::natives::base64_encode;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, Entity, RenderImage, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

struct Host;
impl HostPlugin for Host {}

/// A 2 × 2 24-bit BMP whose pixels carry `n` (every frame a new picture,
/// as every capture is).
fn frame(n: u32) -> String {
    let [a, b, c, _] = n.to_le_bytes();
    let row = [a, b, c, c, b, a, 0, 0];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(14u32 + 40 + 16).to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&(14u32 + 40).to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    bytes.extend_from_slice(&row);
    bytes.extend_from_slice(&row);
    format!("data:image/bmp;base64,{}", base64_encode(&bytes))
}

fn key(src: &str) -> MediaKey {
    image_key(&Host, src).expect("a data: url passes the default media policy")
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

fn tree(images: &[(&str, &str)]) -> NestedNode {
    let children: Vec<Value> = images.iter().map(|(id, src)| json!({"id": id, "component": "Image", "props": {"src": src, "alt": "Space"}, "style": {"width": 96, "height": 60}})).collect();
    serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 400}, "children": children})).unwrap()
}

fn surface<'a>(cx: &'a mut TestAppContext, images: &[(&str, &str)]) -> (Entity<SurfaceView>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host: Rc::new(Host), ..Default::default() };
    let first = tree(images);
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(first, cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
        });
        Pane { view }
    });
    for _ in 0..3 {
        draw(vcx);
    }
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

fn show(view: &Entity<SurfaceView>, images: &[(&str, &str)], cx: &mut VisualTestContext) {
    let next = tree(images);
    view.update(cx, |v, cx| {
        let out = v.set_nested(next, cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    draw(cx);
    draw(cx);
}

fn cached(src: &str, cx: &mut VisualTestContext) -> bool {
    let key = key(src);
    cx.update(|_, cx| cx.has_asset::<MediaLoader>(&key))
}

/// The decoded picture of a src that is in the cache now (never starts a load).
fn decoded(src: &str, cx: &mut VisualTestContext) -> Arc<RenderImage> {
    assert!(cached(src, cx), "{} is cached", &src[..40]);
    let key = key(src);
    cx.update(|window, cx| window.get_asset::<MediaLoader>(&key, cx)).expect("decoded").expect("a picture")
}

fn in_atlas(image: &RenderImage, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.has_image_atlas_entry(image))
}

/// Section 8, 6a: 200 distinct frames through one Image leave ONE asset
/// (and one atlas tile): each frame's picture is released once the next
/// one is on screen.
#[gpui::test]
fn two_hundred_frames_through_one_image_leave_one_asset(cx: &mut TestAppContext) {
    let frames: Vec<String> = (0..200).map(frame).collect();
    let (view, cx) = surface(cx, &[("live", &frames[0])]);
    let mut pictures = vec![decoded(&frames[0], cx)];
    assert!(in_atlas(&pictures[0], cx), "the first frame is painted");
    for src in &frames[1..] {
        show(&view, &[("live", src)], cx);
        pictures.push(decoded(src, cx));
    }
    let left: Vec<usize> = (0..frames.len()).filter(|&i| cached(&frames[i], cx)).collect();
    assert_eq!(left, vec![199], "one asset: the frame on screen");
    let tiles: Vec<usize> = (0..pictures.len()).filter(|&i| in_atlas(&pictures[i], cx)).collect();
    assert_eq!(tiles, vec![199], "one atlas tile: the frame on screen");
    assert_eq!(held_data_pictures(), 1);
}

/// An Image whose src goes empty (the live view's fallback) or whose node
/// leaves the tree (a closed list) lets its picture go; a picture two
/// Images show stays until the second one moves off it.
#[gpui::test]
fn a_picture_goes_with_its_last_image(cx: &mut TestAppContext) {
    let (a, b, c) = (frame(1), frame(2), frame(3));
    let (view, cx) = surface(cx, &[("one", &a), ("two", &a), ("three", &b)]);
    let (pa, pb) = (decoded(&a, cx), decoded(&b, cx));
    show(&view, &[("one", &c), ("two", &a), ("three", &b)], cx);
    assert!(cached(&a, cx) && in_atlas(&pa, cx), "\"two\" still shows it");
    show(&view, &[("one", &c), ("two", &c), ("three", &b)], cx);
    assert!(!cached(&a, cx) && !in_atlas(&pa, cx), "its last Image moved off it");
    show(&view, &[("one", &c), ("two", &c), ("three", "")], cx);
    assert!(!cached(&b, cx) && !in_atlas(&pb, cx), "an empty src (the fallback) lets it go");
    let pc = decoded(&c, cx);
    show(&view, &[], cx);
    assert!(!cached(&c, cx) && !in_atlas(&pc, cx), "the nodes left the tree");
    assert_eq!(held_data_pictures(), 0);
}
