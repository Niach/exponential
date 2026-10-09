//! Round 3 (VAPP-102) in the layout core: `Menu` (`openOn: press` = its ONE
//! child is the trigger, no child = the default outline Button;
//! `contextmenu` = the child is the target region), a submenu bound to a
//! source, and `Segmented` sizing (content-sized unless `fill` or `bar`).

use std::collections::HashMap;

use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{LayoutOutput, OutEvent, Surface, SurfaceOptions};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> exponential_ui::surface::PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())).find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

fn press(s: &mut Surface, id: &str) -> Vec<OutEvent> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    s.event(i, "press", None)
}

fn action<'a>(events: &'a [OutEvent], name: &str) -> Option<&'a Value> {
    events.iter().find_map(|e| match e {
        OutEvent::Action { name: n, context, .. } if n == name => Some(context),
        _ => None,
    })
}

fn root(children: Value) -> Value {
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": 40}, "children": children})
}

#[test]
fn a_press_menus_child_is_its_trigger_and_the_menu_opens_under_it() {
    let mut s = surface(root(json!([
        {"id": "m", "component": "Menu", "props": {"items": [{"label": "Rename", "value": "rename"}, {"label": "Delete", "value": "delete"}]},
         "on": {"select": {"event": {"name": "pick"}}},
         "children": [{"id": "more", "component": "Button", "props": {"label": "More", "variant": "ghost"}}]}
    ])));
    s.set_viewport(600.0, 600.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    assert!(s.index_of("m.trigger").is_none(), "the child IS the trigger: no synthesized Button");
    let more = s.nodes().into_iter().find(|n| n.id == "more").unwrap();
    assert_eq!(more.trigger_for.as_deref(), Some("m"));
    press(&mut s, "more");
    let out = s.layout(&mut m);
    assert_eq!(out.layers.len(), 1);
    let (anchor, content) = (frame(&s, &out, "more"), out.layers[0].frames[0]);
    assert_eq!(content.x, anchor.x, "bottom-start");
    assert!(content.y >= anchor.y + anchor.h, "under the trigger");
    let events = press(&mut s, "m.item.1");
    assert_eq!(action(&events, "pick").unwrap()["value"], json!("delete"));
    assert!(s.layout(&mut m).layers.is_empty(), "a pick closes the menu");
}

#[test]
fn a_press_menu_without_a_child_synthesizes_an_outline_button_from_label_and_icon() {
    let mut s = surface(root(json!([
        {"id": "a", "component": "Menu", "props": {"label": "Actions", "items": [{"label": "One", "value": "1"}]}},
        {"id": "b", "component": "Menu", "props": {"icon": "ui-more", "items": [{"label": "One", "value": "1"}]}}
    ])));
    s.set_viewport(600.0, 600.0, None);
    s.layout(&mut fixed());
    let nodes = s.nodes();
    let a = nodes.iter().find(|n| n.id == "a.trigger").expect("a.trigger");
    assert_eq!((a.component.as_str(), a.props["label"].clone(), a.props["variant"].clone()), ("Button", json!("Actions"), json!("outline")));
    let b = nodes.iter().find(|n| n.id == "b.trigger").expect("b.trigger");
    assert_eq!((b.props["icon"].clone(), b.props["size"].clone()), (json!("ui-more"), json!("icon")), "an icon-only trigger");
}

#[test]
fn a_submenu_bound_to_a_source_lists_its_rows() {
    let mut s = surface(root(json!([
        {"id": "m", "component": "Menu", "props": {"label": "Issue", "items": [
            {"label": "Rename", "value": "rename"},
            {"kind": "submenu", "label": "Status", "items": {"path": "/statuses"}}]},
         "on": {"select": {"event": {"name": "pick"}}}}
    ])));
    s.set_data("", Some(json!({"statuses": [{"label": "Backlog", "value": "backlog"}, {"label": "Done", "value": "done"}]}))).unwrap();
    s.set_viewport(600.0, 600.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    press(&mut s, "m.trigger");
    s.layout(&mut m);
    press(&mut s, "m.item.1");
    let out = s.layout(&mut m);
    assert_eq!(out.layers.len(), 2, "the submenu is its own layer");
    let labels: Vec<Value> = ["m.itemLabel.1.0", "m.itemLabel.1.1"]
        .iter()
        .map(|id| s.nodes().into_iter().find(|n| n.id == *id).unwrap_or_else(|| panic!("{id}: {:?}", s.nodes().iter().map(|n| n.id.clone()).collect::<Vec<_>>())).props["text"].clone())
        .collect();
    assert_eq!(labels, [json!("Backlog"), json!("Done")]);
    let events = press(&mut s, "m.item.1.1");
    assert_eq!(action(&events, "pick").unwrap()["value"], json!("done"));
}

#[test]
fn a_segmented_is_content_sized_unless_fill_or_bar() {
    let items = json!([{"label": "A", "value": "a"}, {"label": "B", "value": "b"}]);
    let mut s = surface(root(json!([
        {"id": "seg", "component": "Segmented", "props": {"items": items}},
        {"id": "fill", "component": "Segmented", "props": {"items": items, "fill": true}},
        {"id": "bar", "component": "Segmented", "props": {"items": items, "variant": "bar"}}
    ])));
    s.set_viewport(600.0, 600.0, None);
    let out = s.layout(&mut fixed());
    assert!(frame(&s, &out, "seg").w < 520.0, "content-sized");
    assert_eq!(frame(&s, &out, "fill").w, 520.0);
    assert_eq!(frame(&s, &out, "bar").w, 520.0, "a bar always fills");
}
