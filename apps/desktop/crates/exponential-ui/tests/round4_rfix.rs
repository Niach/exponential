//! VAPP-103 rfix: the round-4 review's core findings, one regression each.

use std::collections::HashMap;

use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{Surface, SurfaceOptions};
use serde_json::json;

fn surface() -> Surface {
    let mut s = Surface::new("t", SurfaceOptions { theme: exponential_ui::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
    s.set_viewport(400.0, 800.0, None);
    s
}

/// A grid item holding a 160 px child keeps its height; the next item sits
/// under it (the layout cache served a width-only answer's height: 0).
#[test]
fn a_grid_item_keeps_the_height_of_its_content() {
    let mut s = surface();
    s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [
        {"id": "root", "component": "Box", "style": {"display": "grid"}, "children": ["a", "b"]},
        {"id": "a", "component": "Box", "style": {"width": 180}, "children": ["tall"]},
        {"id": "tall", "component": "Box", "style": {"height": 160}},
        {"id": "b", "component": "Text", "text": "hello world this is text"},
    ]}}))
    .unwrap();
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let frame = |s: &mut Surface, id: &str| {
        let i = s.index_of(id).unwrap();
        out.frames.iter().find(|f| f.index == i).cloned().unwrap()
    };
    let (a, b) = (frame(&mut s, "a"), frame(&mut s, "b"));
    assert_eq!(a.h, 160.0, "{a:?}");
    assert!(b.y >= 160.0, "{b:?}");
}

/// A streamed `updateComponents` reports its own components' issues at
/// once (the tree still reduces lazily).
#[test]
fn update_components_reports_its_components_issues() {
    let mut s = surface();
    let out = s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [
        {"id": "root", "component": "Stack", "children": ["x", "later"]},
        {"id": "x", "component": "NoSuchThing"},
    ]}}))
    .unwrap();
    let messages: Vec<String> = out.issues.iter().map(|i| format!("{}: {}", i.id, i.message)).collect();
    assert_eq!(messages, vec!["x: unknown component NoSuchThing".to_string()]);
    // The tree's issues (a missing child) come with the reduce.
    assert!(s.issues().iter().any(|i| i.id == "later"), "{:?}", s.issues());
    let ok = s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": "later", "component": "Text", "text": "hi"}]}})).unwrap();
    assert!(ok.issues.is_empty(), "{:?}", ok.issues);
}

/// The reducer checks an EXTENSION component's calls along its own prop
/// schemas (`validate_node_in` over the options' view): a call nested in a
/// TrendLine `values` item is checked, never skipped as schema-less.
#[test]
fn the_reducer_checks_extension_calls_against_the_view() {
    use exponential_ui::extension::define_extension;
    use exponential_ui::reducer::{reduce_nested, ReduceOptions};
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/catalog-extension.json");
    let file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let ext = define_extension(serde_json::from_value(file["extension"].clone()).unwrap()).expect("the example extension");
    let view = exponential_ui::catalog::CatalogView::with(&[ext]);
    let catalog = file["extension"]["id"].as_str().unwrap();
    let node = serde_json::from_value(json!({"id": "t", "component": "TrendLine", "props": {"values": [1, {"call": "nope", "args": {}}]}})).unwrap();
    let issues = reduce_nested(&node, &ReduceOptions::new(catalog).with_view(view)).issues;
    assert!(issues.iter().any(|i| i.id == "t" && i.message.starts_with("props.values[1]") && i.message.contains("unknown function \"nope\"")), "{issues:?}");
}

/// One wire naming: every OutEvent is camelCase, kinds AND fields
/// (`componentId` on action, input, pickFiles and functionCall alike;
/// `delayMs`), the host API's form.
#[test]
fn out_events_are_camel_case_on_the_wire() {
    use exponential_ui::surface::OutEvent;
    let wire = |e: OutEvent| serde_json::to_value(e).unwrap();
    assert_eq!(
        wire(OutEvent::Action { name: "save".into(), component_id: "b".into(), event: "press".into(), context: json!({}), payload: None }),
        json!({"kind": "action", "name": "save", "componentId": "b", "event": "press", "context": {}})
    );
    assert_eq!(
        wire(OutEvent::Input { component_id: "q".into(), name: "q".into(), path: Some("/q".into()), value: json!("a"), commit: false }),
        json!({"kind": "input", "componentId": "q", "name": "q", "path": "/q", "value": "a", "commit": false})
    );
    assert_eq!(wire(OutEvent::PickFiles { component_id: "up".into(), accept: None, multiple: true }), json!({"kind": "pickFiles", "componentId": "up", "multiple": true}));
    assert_eq!(wire(OutEvent::FunctionCall { component_id: "b".into(), name: "app.toast".into(), args: json!({}) }), json!({"kind": "functionCall", "componentId": "b", "name": "app.toast", "args": {}}));
    assert_eq!(wire(OutEvent::HoverTimer { owner: "hc".into(), delay_ms: 150 }), json!({"kind": "hoverTimer", "owner": "hc", "delayMs": 150}));
    // And back.
    let back: OutEvent = serde_json::from_value(json!({"kind": "pickFiles", "componentId": "up", "multiple": false})).unwrap();
    assert_eq!(back, OutEvent::PickFiles { component_id: "up".into(), accept: None, multiple: false });
}

/// The image header every renderer's pixel cap reads (the natives through
/// the FFI): unknown formats pass to the platform, a known one without a
/// readable size is refused, frames multiply.
#[test]
fn the_image_header_names_size_frames_or_unreadable() {
    use exponential_ui::host::{image_header, media_image_check, ImageHeader};
    let mut gif = b"GIF89a\x00\x10\x00\x10\x00\x00\x00".to_vec();
    for _ in 0..3 {
        gif.extend_from_slice(b"\x2c\x00\x00\x00\x00\x00\x10\x00\x10\x00\x02\x02\x44\x01\x00");
    }
    gif.push(0x3b);
    assert_eq!(image_header(&gif), ImageHeader::Size { width: 4096, height: 4096, frames: 3 });
    assert_eq!(media_image_check(&gif, 1 << 20, 4096 * 4096 * 2), Err("media is 4096×4096 × 3 frames, over 33554432 pixels".to_string()));
    assert_eq!(image_header(b"\x89PNG\r\n\x1a\n\x01"), ImageHeader::Unreadable);
    assert_eq!(image_header(b"II*\x00"), ImageHeader::Unknown, "TIFF: the platform decides");
    assert_eq!(image_header(br#"<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="20000"/>"#), ImageHeader::Size { width: 20_000, height: 20_000, frames: 1 });
}
