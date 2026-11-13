//! #EXP-1804: CodeBlock `follow` (React's natives/code-follow.test.tsx): a
//! scrolling body stays at its bottom while `code` grows (a live log), stops
//! once the person scrolled up and follows again back at the bottom; without
//! `follow` nothing moves.

use eui_core::measure::{DirectMeasureWithBaseline, LeafRequest};
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn log(n: usize) -> String {
    (1..=n).map(|i| format!("\u{1b}[32m✓\u{1b}[0m step {i}")).collect::<Vec<_>>().join("\n")
}

fn surface(follow: bool) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "log", "component": "CodeBlock", "props": {"code": {"@path": "/log"}, "language": "ansi", "maxLines": 3, "follow": follow}})).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: eui_core::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
    s.set_data("", Some(json!({"log": log(2)}))).expect("data");
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(400.0, 0.0, None);
    s
}

/// Every leaf 7 px per char on one line of its line height.
fn measure() -> DirectMeasureWithBaseline<impl FnMut(&LeafRequest, Option<f32>) -> (f32, f32, Option<f32>)> {
    DirectMeasureWithBaseline(|leaf: &LeafRequest, _wrap: Option<f32>| (7.0 * leaf.text().chars().count() as f32, leaf.text_style.line_height.max(16.0), None), 11)
}

/// The body's (offset_y, bottom) after a pass.
fn grow(s: &mut Surface, n: usize) -> (f32, f32) {
    s.set_data("/log", Some(Value::String(log(n)))).expect("data");
    let out = s.layout(&mut measure());
    let body = s.index_of("log.body").expect("body");
    let sc = out.scrolls.iter().find(|c| c.index == body).expect("the body scrolls");
    let h = s.last_frame(body).expect("placed").h;
    (sc.offset_y, (sc.content_height - h).max(0.0))
}

#[test]
fn follow_sticks_to_the_bottom_until_scrolled_up_and_resumes_at_the_bottom() {
    let mut s = surface(true);
    s.layout(&mut measure());
    let (y, bottom) = grow(&mut s, 6);
    assert!(bottom > 0.0, "six lines overflow three");
    assert_eq!(y, bottom, "at the bottom as lines arrive");
    s.scroll_to("log.body", 0.0, 10.0);
    let (y, bottom) = grow(&mut s, 9);
    assert!(bottom > 10.0);
    assert_eq!(y, 10.0, "scrolled up: stays where the person left it");
    s.scroll_to("log.body", 0.0, bottom);
    let (y, bottom) = grow(&mut s, 12);
    assert_eq!(y, bottom, "back at the bottom: follows again");
}

#[test]
fn without_follow_the_body_stays_at_the_top() {
    let mut s = surface(false);
    s.layout(&mut measure());
    let (y, bottom) = grow(&mut s, 6);
    assert!(bottom > 0.0);
    assert_eq!(y, 0.0);
}
