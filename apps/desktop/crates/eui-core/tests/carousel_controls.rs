//! Round 2 §7: a Carousel with 2+ pages places a `controls` row under the
//! pages: `previous`, the dots (`indicator`), `next`. A button press pages.

mod support;

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use serde_json::{json, Value};

fn carousel(extra: Value) -> Surface {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    let mut root = json!({"id": "root", "component": "Carousel", "children": ["a", "b", "c"]});
    root.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        root,
        {"id": "a", "component": "Text", "text": "1"},
        {"id": "b", "component": "Text", "text": "2"},
        {"id": "c", "component": "Text", "text": "3"}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    s
}

fn find(s: &mut Surface, id: &str) -> (u32, Value) {
    let nodes = s.nodes();
    let i = nodes.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("{id} placed"));
    (i as u32, Value::Object(nodes[i].props.clone()))
}

#[test]
fn the_controls_row_holds_previous_the_dots_and_next() {
    let mut s = carousel(json!({}));
    let nodes = s.nodes();
    let controls = nodes.iter().position(|n| n.id == "root.controls").expect("root.controls") as u32;
    let kids: Vec<&str> = nodes.iter().filter(|n| n.parent == Some(controls)).map(|n| n.id.as_str()).collect();
    assert_eq!(kids, ["root.previous", "root.indicator", "root.next"]);
    let (_, prev) = find(&mut s, "root.previous");
    assert_eq!(prev["disabled"], json!(true), "page 0 without loop");
    assert_eq!(prev["label"], json!("Previous"));
}

#[test]
fn next_and_previous_page_and_loop_wraps() {
    let mut s = carousel(json!({}));
    let (next, _) = find(&mut s, "root.next");
    s.event(next, "press", None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let (_, ind) = find(&mut s, "root.indicator");
    assert_eq!(ind["page"], json!(1));
    let (_, prev) = find(&mut s, "root.previous");
    assert_eq!((prev["target"].clone(), prev["disabled"].clone()), (json!(0), json!(false)));

    let mut s = carousel(json!({"loop": true}));
    let (_, prev) = find(&mut s, "root.previous");
    assert_eq!((prev["target"].clone(), prev["disabled"].clone()), (json!(2), json!(false)));
}

#[test]
fn one_page_has_no_controls() {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Carousel", "children": ["a"]},
        {"id": "a", "component": "Text", "text": "1"}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    assert!(!s.nodes().iter().any(|n| n.id == "root.controls"));
}

// EXP-1495 (round-6 contract §3): the counter line (EXP-1493) and the thumbnail strip.

fn frame(out: &eui_core::surface::LayoutOutput, s: &mut Surface, id: &str) -> (f32, f32, f32, f32) {
    let (i, _) = find(s, id);
    let f = out.frames.iter().find(|f| f.index == i).unwrap_or_else(|| panic!("{id} framed"));
    (f.x, f.y, f.w, f.h)
}

#[test]
fn the_counter_is_its_own_line_above_the_track_and_counts_from_one() {
    // EXP-1493 (round-6 contract §3): the root's FIRST flow child, start
    // aligned, the root's gap (8) above the track, never over a page.
    let mut s = carousel(json!({"counter": true}));
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let (_, counter) = find(&mut s, "root.counter");
    assert_eq!(counter["text"], json!("1 / 3"));
    let (rx, ry, _, _) = frame(&out, &mut s, "root");
    let (_, py, _, _) = frame(&out, &mut s, "root.page");
    let (cx, cy, cw, ch) = frame(&out, &mut s, "root.counter");
    assert_eq!((cx - rx, cy - ry, ch), (0.0, 0.0, 20.0), "on the root's start edge, 20 tall");
    assert_eq!(py - (cy + ch), 8.0, "the root's gap above the track");
    assert!(cw < 390.0, "hugs its text (alignSelf flex-start): {cw}");
    let nodes = s.nodes();
    let root = nodes.iter().position(|n| n.id == "root").unwrap() as u32;
    let kids: Vec<&str> = nodes.iter().filter(|n| n.parent == Some(root)).map(|n| n.id.as_str()).collect();
    assert_eq!(kids, ["root.counter", "root.page", "root.controls"], "the first flow child");
    assert_eq!(nodes.iter().find(|n| n.id == "root.counter").unwrap().accessibility, Some(json!({"hidden": true})));

    let (next, _) = find(&mut s, "root.next");
    s.event(next, "press", None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    assert_eq!(find(&mut s, "root.counter").1["text"], json!("2 / 3"));
    assert!(!carousel(json!({})).nodes().iter().any(|n| n.id == "root.counter"));
}

#[test]
fn the_counter_hugs_the_end_edge_under_rtl() {
    let mut s = carousel(json!({"counter": true, "style": {"direction": "rtl"}}));
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let (rx, _, rw, _) = frame(&out, &mut s, "root");
    let (cx, _, cw, _) = frame(&out, &mut s, "root.counter");
    assert!(((rx + rw) - (cx + cw)).abs() < 0.001, "the right edge under rtl: {cx}+{cw} in {rx}+{rw}");
}

#[test]
fn the_strip_holds_one_52_square_per_page_before_the_controls_and_a_press_pages() {
    let mut s = carousel(json!({"thumbnails": ["a.png", "", "c.png", "ignored.png"]}));
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let nodes = s.nodes();
    let root = nodes.iter().position(|n| n.id == "root").unwrap() as u32;
    let kids: Vec<&str> = nodes.iter().filter(|n| n.parent == Some(root)).map(|n| n.id.as_str()).collect();
    assert_eq!(kids, ["root.page", "root.thumbnails", "root.controls"]);
    let strip = nodes.iter().position(|n| n.id == "root.thumbnails").unwrap() as u32;
    let thumbs: Vec<&str> = nodes.iter().filter(|n| n.parent == Some(strip)).map(|n| n.id.as_str()).collect();
    assert_eq!(thumbs, ["root.thumbnail.0", "root.thumbnail.1", "root.thumbnail.2"], "one per page, the extra URL ignored");
    assert!(nodes.iter().any(|n| n.id == "root.thumbnailImage.0"));
    assert!(!nodes.iter().any(|n| n.id == "root.thumbnailImage.1"), "a missing URL = the empty square");
    let t0 = nodes.iter().find(|n| n.id == "root.thumbnail.0").unwrap();
    assert!(t0.states.iter().any(|st| st == "selected"));
    assert_eq!(t0.accessibility, Some(json!({"role": "button", "label": "Page 1 of 3", "current": true})));
    let (x0, _, w, h) = frame(&out, &mut s, "root.thumbnail.0");
    let (x1, _, _, _) = frame(&out, &mut s, "root.thumbnail.1");
    assert_eq!((w, h, x1 - x0), (52.0, 52.0, 60.0), "52 x 52, gap sm");

    let (t2, _) = find(&mut s, "root.thumbnail.2");
    s.event(t2, "press", None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    assert_eq!(find(&mut s, "root.indicator").1["page"], json!(2));
    assert!(s.nodes().iter().find(|n| n.id == "root.thumbnail.2").unwrap().states.iter().any(|st| st == "selected"));
}

/// EXP-1493: fixtures/carousel.json (THE Carousel spec), every case.
#[test]
fn the_carousel_fixture_replays() {
    let cases = support::carousel::carousel_cases();
    assert_eq!(cases.len(), support::fixture("carousel.json")["cases"].as_array().unwrap().len());
    let failed: Vec<String> = cases.iter().filter_map(|(n, r)| r.as_ref().err().map(|e| format!("{n}: {e}"))).collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
