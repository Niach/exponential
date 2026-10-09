//! Round 1 review fixes in the core: controls nested in template rows and
//! Table slot cells write through and fire; subtree rebuilds keep the
//! enclosing Form's flags; scroll offsets clamp into the state; hover cards
//! stay open while the pointer is on their content; disabled controls are
//! inert; template keys never collide; `direction` inherits per node; `%`
//! font sizes follow a parent's restyle in the same pass; snapshots during a
//! stepped pass keep the pass consistent.

use std::collections::HashMap;

use exponential_ui::measure::{FixedMeasure, Intrinsics};
use exponential_ui::surface::{LayoutOutput, LayoutStep, OutEvent, Surface, SurfaceOptions, HOVER_CLOSE_MS};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn geometry(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: None, ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

/// A flat component list (A2UI `updateComponents`): templates are
/// referenced components, not rendered in place.
fn flat(components: Value) -> Surface {
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_components(serde_json::from_value(components).expect("components"));
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn press(s: &mut Surface, id: &str) -> Vec<OutEvent> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    s.event(i, "press", None)
}

fn data_changed(events: &[OutEvent], path: &str) -> Option<Value> {
    events.iter().find_map(|e| match e {
        OutEvent::DataChanged { path: p, value } if p == path => Some(value.clone()),
        _ => None,
    })
}

fn action<'a>(events: &'a [OutEvent], name: &str) -> Option<&'a Value> {
    events.iter().find_map(|e| match e {
        OutEvent::Action { name: n, context, .. } if n == name => Some(context),
        _ => None,
    })
}

fn node(s: &mut Surface, id: &str) -> exponential_ui::surface::PlacedNode {
    s.nodes().into_iter().find(|n| n.id == id && !n.removed).unwrap_or_else(|| panic!("no node {id}"))
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> exponential_ui::surface::PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())).find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

// ---------------------------------------------------------------------------
// Nested controls in template rows and Table slot cells
// ---------------------------------------------------------------------------

#[test]
fn a_control_nested_in_a_template_row_writes_its_binding_and_fires() {
    let mut s = flat(json!([
        {"id": "root", "component": "Box", "children": {"componentId": "row", "path": "/items", "key": "id"}},
        {"id": "row", "component": "Box", "children": ["row-done"]},
        {"id": "row-done", "component": "Switch", "label": "Done", "name": "done", "checked": {"path": "done"}, "on": {"change": {"event": {"name": "toggled", "context": {"id": {"path": "id"}}}}}}
    ]));
    s.set_data("/items", Some(json!([{"id": "a", "done": false}, {"id": "b", "done": true}])));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let events = press(&mut s, "row-done.b");
    assert_eq!(data_changed(&events, "/items/1/done"), Some(json!(false)), "the nested Switch writes ITS row");
    assert_eq!(action(&events, "toggled"), Some(&json!({"id": "b", "checked": false})), "on.change fires with the row's context");
    s.layout(&mut m);
    let events = press(&mut s, "row-done.a");
    assert_eq!(data_changed(&events, "/items/0/done"), Some(json!(true)));
}

#[test]
fn a_table_slot_button_fires_with_its_literal_row_as_scope() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "t", "component": "Table", "props": {"columns": [{"key": "name", "label": "Name"}, {"key": "actions", "label": "", "type": "slot", "slot": "actions"}],
            "rows": [{"id": "r1", "name": "Ada"}, {"id": "r2", "name": "Grace"}]},
         "slots": {"actions": {"id": "edit", "component": "Button", "props": {"label": "Edit"}, "on": {"press": {"event": {"name": "edit", "context": {"id": {"path": "id"}, "name": {"path": "name"}}}}}}}}
    ]}));
    s.set_viewport(500.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let events = press(&mut s, "edit.r2");
    assert_eq!(action(&events, "edit"), Some(&json!({"id": "r2", "name": "Grace"})), "the literal row resolves at event time");
}

#[test]
fn a_form_inside_a_template_row_collects_its_own_instance_fields() {
    let mut s = flat(json!([
        {"id": "root", "component": "Box", "children": {"componentId": "card", "path": "/items", "key": "id"}},
        {"id": "card", "component": "Form", "name": "card", "on": {"submit": {"event": {"name": "save"}}}, "children": ["card-title", "card-go"]},
        {"id": "card-title", "component": "Input", "label": "Title", "name": "title", "value": {"path": "title"}},
        {"id": "card-go", "component": "Button", "label": "Save", "submit": true}
    ]));
    s.set_data("/items", Some(json!([{"id": "a", "title": "One"}, {"id": "b", "title": "Two"}])));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let events = press(&mut s, "card-go.b");
    assert_eq!(action(&events, "save"), Some(&json!({"values": {"title": "Two"}})));
}

// ---------------------------------------------------------------------------
// Subtree rebuilds keep the Form's flags
// ---------------------------------------------------------------------------

#[test]
fn a_tab_switch_inside_a_disabled_form_keeps_its_fields_disabled() {
    let mut s = surface(json!({"id": "f", "component": "Form", "props": {"name": "f", "disabled": true}, "children": [
        {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}]},
         "children": [{"id": "pa", "component": "Input", "props": {"label": "A", "name": "a"}}, {"id": "pb", "component": "Input", "props": {"label": "B", "name": "b"}}]}
    ]}));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    assert_eq!(node(&mut s, "pa").props.get("disabled"), Some(&json!(true)));
    // A disabled Form's tabs still switch (the Tabs is not a field).
    press(&mut s, "tabs.tab.1");
    let out = s.layout(&mut m);
    assert!(out.built_nodes < 20, "a subtree rebuild: {}", out.built_nodes);
    for id in ["pa", "pb"] {
        assert_eq!(node(&mut s, id).props.get("disabled"), Some(&json!(true)), "{id} after the subtree rebuild");
        assert!(!node(&mut s, &format!("{id}.field")).pressable, "{id}.field stays inert");
    }
}

#[test]
fn a_re_windowed_list_inside_a_busy_form_keeps_its_submit_buttons_loading() {
    let rows: Vec<Value> = (0..60).map(|i| json!({"id": format!("b{i}"), "component": "Button", "props": {"label": format!("Save {i}"), "submit": true}})).collect();
    let mut s = geometry(json!({"id": "f", "component": "Form", "props": {"name": "f", "busy": true}, "children": [
        {"id": "list", "component": "List", "style": {"height": 300}, "children": rows}
    ]}));
    s.set_viewport(400.0, 600.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    assert_eq!(node(&mut s, "b0").props.get("loading"), Some(&json!(true)));
    s.scroll("list", 600.0);
    let out = s.layout(&mut m);
    assert!(out.rebuilt);
    let start = out.lists[0].start as usize;
    let id = format!("b{}", start + 1);
    assert_eq!(node(&mut s, &id).props.get("loading"), Some(&json!(true)), "{id} built by the re-window");
}

// ---------------------------------------------------------------------------
// Scroll clamping
// ---------------------------------------------------------------------------

fn hundred_rows() -> Surface {
    let rows: Vec<Value> = (0..100).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 20, "width": 300, "flexShrink": 0}})).collect();
    let mut s = geometry(json!({"id": "root", "component": "Box", "style": {"width": "100%"}, "children": [{"id": "list", "component": "List", "style": {"height": 400}, "children": rows}]}));
    s.set_viewport(400.0, 800.0, None);
    s
}

#[test]
fn a_fling_past_the_end_lands_on_the_last_rows() {
    let mut s = hundred_rows();
    let mut m = fixed();
    s.layout(&mut m);
    s.layout(&mut m);
    assert!(s.scroll("list", 50_000.0));
    let out = s.layout(&mut m);
    let list = s.index_of("list").unwrap();
    let sc = out.scrolls.iter().find(|x| x.index == list).unwrap();
    assert_eq!(sc.offset_y, 1600.0, "100 × 20 − 400");
    assert_eq!(s.scroll_offset("list").1, 1600.0, "the clamped offset is the state");
    let l = &out.lists[0];
    assert!(l.start as usize <= 80 && l.end == 100, "the window covers the viewport 1600–2000: {}..{}", l.start, l.end);
    for i in 80..100 {
        assert!(s.index_of(&format!("r{i}")).is_some(), "row {i} built");
    }
}

#[test]
fn a_list_that_shrinks_under_its_offset_re_windows_at_the_clamped_offset() {
    let mut s = hundred_rows();
    let mut m = fixed();
    s.layout(&mut m);
    s.layout(&mut m);
    s.scroll("list", 1600.0);
    s.layout(&mut m);
    // The surface drops to 60 rows (a new component list).
    let rows: Vec<Value> = (0..60).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 20, "width": 300, "flexShrink": 0}})).collect();
    s.set_nested(serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"width": "100%"}, "children": [{"id": "list", "component": "List", "style": {"height": 400}, "children": rows}]})).unwrap());
    s.layout(&mut m);
    let relayout = s.take_events();
    assert!(relayout.iter().any(|e| matches!(e, OutEvent::Relayout)), "the host is asked for one more pass");
    let out = s.layout(&mut m);
    let l = &out.lists[0];
    assert_eq!(s.scroll_offset("list").1, 800.0, "60 × 20 − 400");
    assert!(l.start as usize <= 40 && l.end == 60, "{}..{}", l.start, l.end);
}

#[test]
fn a_page_scrolled_list_is_not_clamped_by_its_own_box() {
    // No height: the list grows to its content and a PAGE scrolls it; the
    // host's offset (how far the page moved past its top) positions the
    // window and must not clamp to 0.
    let rows: Vec<Value> = (0..100).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 20, "width": 300, "flexShrink": 0}})).collect();
    let mut s = geometry(json!({"id": "root", "component": "Box", "style": {"width": "100%"}, "children": [{"id": "list", "component": "List", "children": rows}]}));
    s.set_viewport(400.0, 300.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    s.layout(&mut m);
    s.scroll("list", 1000.0);
    let out = s.layout(&mut m);
    assert_eq!(s.scroll_offset("list").1, 1000.0);
    assert!(out.lists[0].start > 0);
}

// ---------------------------------------------------------------------------
// Hover cards
// ---------------------------------------------------------------------------

#[test]
fn a_hover_card_stays_open_while_the_pointer_moves_into_its_content() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "hc", "component": "HoverCard", "slots": {"trigger": {"id": "who", "component": "Text", "props": {"text": "@ada"}}},
         "children": [{"id": "profile", "component": "Link", "props": {"label": "Profile", "href": "https://example.com/ada"}}]}
    ]}));
    s.set_viewport(400.0, 600.0, None);
    // The core times the close (what a native host asks for).
    s.set_settings(exponential_ui::surface::SurfaceSettings { hover_close_ms: HOVER_CLOSE_MS, ..s.settings().clone() });
    let mut m = fixed();
    s.layout(&mut m);
    s.set_states("who", vec!["hover".into()]);
    assert!(s.layout(&mut m).layers.iter().any(|l| l.owner == "hc"));
    s.take_events();
    // Leaving the trigger: a close timer, the card still open.
    s.set_states("who", vec![]);
    let timer = s.take_events();
    assert!(timer.iter().any(|e| matches!(e, OutEvent::HoverTimer { owner, delay_ms } if owner == "hc" && *delay_ms == HOVER_CLOSE_MS)), "{timer:?}");
    assert!(s.layout(&mut m).layers.iter().any(|l| l.owner == "hc"), "open while the pointer crosses the gap");
    // …the pointer arrives on the content before the timer fires.
    s.set_states("profile", vec!["hover".into()]);
    assert!(s.hover_timeout("hc").is_empty(), "the content holds it open");
    assert!(s.layout(&mut m).layers.iter().any(|l| l.owner == "hc"));
    let events = press(&mut s, "profile");
    assert!(events.iter().any(|e| matches!(e, OutEvent::OpenUrl { url } if url == "https://example.com/ada")), "the link inside is pressable");
    // Leaving the content: closes when the timer fires.
    s.set_states("profile", vec![]);
    assert!(s.take_events().iter().any(|e| matches!(e, OutEvent::HoverTimer { .. })));
    let closed = s.hover_timeout("hc");
    assert!(closed.iter().any(|e| matches!(e, OutEvent::Relayout)));
    assert!(s.layout(&mut m).layers.is_empty());
    // Reopening starts clean (the content's old hover left with it).
    s.set_states("who", vec!["hover".into()]);
    assert!(s.layout(&mut m).layers.iter().any(|l| l.owner == "hc"));
    s.set_states("who", vec![]);
    assert_eq!(s.hover_timeout("hc").iter().filter(|e| matches!(e, OutEvent::Relayout)).count(), 1);
}

#[test]
fn without_a_core_close_delay_the_content_still_holds_a_hover_card_open() {
    // gpui's way: the host delays the trigger's un-hover itself; the core
    // closes at once when NOTHING holds the card, and the content holds it.
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "hc", "component": "HoverCard", "slots": {"trigger": {"id": "who", "component": "Text", "props": {"text": "@ada"}}},
         "children": [{"id": "card", "component": "Text", "props": {"text": "Ada Lovelace"}}]},
        {"id": "tip", "component": "Tooltip", "props": {"content": "Saves"}, "children": [{"id": "btn", "component": "Button", "props": {"label": "Save"}}]}
    ]}));
    s.set_viewport(400.0, 600.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    s.set_states("who", vec!["hover".into()]);
    s.layout(&mut m);
    s.set_states("card", vec!["hover".into()]);
    s.set_states("who", vec![]);
    assert!(s.layout(&mut m).layers.iter().any(|l| l.owner == "hc"), "the content holds it");
    assert!(!s.take_events().iter().any(|e| matches!(e, OutEvent::HoverTimer { .. })));
    s.set_states("card", vec![]);
    assert!(s.layout(&mut m).layers.is_empty(), "closed at once");
    // Only the trigger itself opens it: a child of the trigger (a host that
    // reports the innermost hovered node and delays the TRIGGER) does not.
    s.set_states("btn", vec!["hover".into()]);
    assert!(s.layout(&mut m).layers.is_empty(), "the trigger's child does not open the tooltip");
}

// ---------------------------------------------------------------------------
// Disabled controls are inert
// ---------------------------------------------------------------------------

#[test]
fn a_disabled_control_ignores_activation_from_any_host_path() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "c", "component": "Checkbox", "props": {"label": "C", "name": "c", "disabled": true, "checked": {"path": "/c"}}, "on": {"change": {"event": {"name": "changed"}}}},
        {"id": "n", "component": "NumberField", "props": {"label": "N", "name": "n", "disabled": true, "value": {"path": "/n"}}},
        {"id": "sel", "component": "Select", "props": {"label": "S", "name": "s", "disabled": true, "value": {"path": "/s"}, "options": [{"label": "A", "value": "a"}]}},
        {"id": "b", "component": "Button", "props": {"label": "Go", "loading": true}, "on": {"press": {"event": {"name": "go"}}}}
    ]}));
    s.set_data("/n", Some(json!(1)));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    for id in ["c", "n.increment", "n.decrement", "sel.trigger", "b"] {
        let Some(i) = s.index_of(id) else { continue };
        assert!(s.event(i, "press", None).is_empty(), "{id} is inert");
    }
    let c = s.index_of("c").unwrap();
    assert!(s.event(c, "change", Some(json!({"checked": true}))).is_empty());
    assert_eq!(s.get_data("/c"), None);
    assert_eq!(s.get_data("/n"), Some(&json!(1)));
}

// ---------------------------------------------------------------------------
// Template keys
// ---------------------------------------------------------------------------

#[test]
fn template_keys_never_collide() {
    let mut s = flat(json!([
        {"id": "root", "component": "Box", "children": {"componentId": "row", "path": "/items", "key": "id"}},
        {"id": "row", "component": "Text", "text": {"path": "name"}}
    ]));
    // A keyed item, an unkeyed one, a duplicate key and an index-like key.
    s.set_data("/items", Some(json!([{"id": "1", "name": "one"}, {"name": "x"}, {"id": "1", "name": "dup"}, {"id": "#1", "name": "hash"}])));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let texts: Vec<(String, Value)> = s.nodes().into_iter().filter(|n| !n.removed && n.id.starts_with("row.")).map(|n| (n.id, n.props["text"].clone())).collect();
    assert_eq!(
        texts,
        vec![("row.1".to_string(), json!("one")), ("row.#1".to_string(), json!("x")), ("row.#2".to_string(), json!("dup")), ("row.#3".to_string(), json!("hash"))],
        "the reference's templateItems ids"
    );
    let events = press_text(&mut s, "row.#2");
    assert!(events.is_empty());
}

fn press_text(s: &mut Surface, id: &str) -> Vec<OutEvent> {
    assert!(s.index_of(id).is_some(), "{id} addressable");
    press(s, id)
}

// ---------------------------------------------------------------------------
// Direction per node
// ---------------------------------------------------------------------------

#[test]
fn an_inner_rtl_box_resolves_its_logical_keys_and_flex_rows_right_to_left() {
    let mut s = geometry(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 400}, "children": [
        {"id": "quote", "component": "Box", "style": {"direction": "rtl", "display": "flex", "flexDirection": "row", "paddingInlineStart": 16, "textAlign": "start"}, "children": [
            {"id": "a", "component": "Box", "style": {"width": 50, "height": 10}},
            {"id": "b", "component": "Box", "style": {"width": 50, "height": 10, "marginInlineStart": 8}}
        ]},
        {"id": "ltr", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "paddingInlineStart": 16}, "children": [{"id": "c", "component": "Box", "style": {"width": 50, "height": 10}}]}
    ]}));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    let out = s.layout(&mut m);
    assert_eq!(out.direction, "ltr", "the surface stays LTR");
    let a = frame(&s, &out, "a");
    let b = frame(&s, &out, "b");
    assert_eq!(a.x + a.w, 400.0 - 16.0, "padding on the RIGHT, the first item at the right edge");
    assert_eq!(b.x + b.w, a.x - 8.0, "b's inline-start margin is on its right");
    assert_eq!(s.visual(s.index_of("quote").unwrap()).unwrap().text_align.as_deref(), Some("right"));
    assert_eq!(frame(&s, &out, "c").x, 16.0, "the LTR sibling is untouched");
}

// ---------------------------------------------------------------------------
// `%` font sizes follow in the same pass
// ---------------------------------------------------------------------------

#[test]
fn a_percent_font_size_follows_a_parent_hover_in_the_same_pass() {
    let mut s = surface(json!({"id": "root", "component": "Box", "children": [
        {"id": "p", "component": "Box", "style": {"fontSize": 14, ":hover": {"fontSize": 18}}, "children": [
            {"id": "c", "component": "Text", "props": {"text": "Half"}, "style": {"fontSize": "50%"}}
        ]}
    ]}));
    s.set_viewport(400.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    let c = s.index_of("c").unwrap();
    assert_eq!(s.text_style(c).unwrap().font_size, 7.0);
    s.set_states("p", vec!["hover".into()]);
    s.layout(&mut m);
    assert_eq!(s.text_style(c).unwrap().font_size, 9.0, "in the SAME pass as the parent's restyle");
}

// ---------------------------------------------------------------------------
// Snapshots during a stepped pass
// ---------------------------------------------------------------------------

#[test]
fn a_snapshot_during_a_stepped_pass_does_not_rebuild_under_it() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [
        {"id": "a", "component": "Text", "props": {"text": "Alpha"}}
    ]}));
    s.set_viewport(300.0, 0.0, None);
    let LayoutStep::Intrinsics(leaves) = s.layout_begin(1) else { panic!("intrinsics first") };
    // Mid-pass (the FFI measures without the lock): the tree changes and a
    // snapshot is read.
    s.set_nested(serde_json::from_value(json!({"id": "root", "component": "Box", "children": [{"id": "z", "component": "Text", "props": {"text": "Zed"}}]})).unwrap());
    assert!(s.nodes().iter().any(|n| n.id == "a" && !n.removed), "the pass's nodes stay until it ends");
    let answers: Vec<Intrinsics> = leaves.iter().map(|_| Intrinsics { min_content_width: 40.0, max_content_width: 40.0, height_at_max_content: 20.0, baseline: None }).collect();
    let mut step = s.layout_intrinsics(&answers);
    while let LayoutStep::Heights(_, r) = step {
        step = s.layout_heights(&vec![20.0; r.len()]);
    }
    let LayoutStep::Done(out) = step else { panic!("done") };
    assert!(out.surface_height >= 20.0, "the pass finished on styled, measured nodes: {}", out.surface_height);
    let next = s.layout(&mut fixed());
    assert!(next.rebuilt && s.index_of("z").is_some(), "the new tree lands on the next pass");
    // An abandoned pass (a failing measurer) leaves no state behind.
    s.invalidate_measures();
    assert!(matches!(s.layout_begin(2), LayoutStep::Intrinsics(_)));
    assert!(s.layout_in_progress());
    s.layout_abort();
    assert!(!s.layout_in_progress());
    assert!(s.layout(&mut fixed()).surface_height > 0.0);
}

// ---------------------------------------------------------------------------
// Merge by id keeps local control state (VAPP-99)
// ---------------------------------------------------------------------------

fn checked(s: &mut Surface) -> Value {
    node(s, "tg").props.get("checked").cloned().unwrap_or(Value::Null)
}

#[test]
fn a_partial_update_keeps_an_untouched_unbound_controls_local_value() {
    let mut s = flat(json!([
        {"id": "root", "component": "Box", "children": ["tg", "msg"]},
        {"id": "tg", "component": "Checkbox", "name": "agree", "label": "Bold"},
        {"id": "msg", "component": "Text", "text": "one"}
    ]));
    s.set_viewport(300.0, 0.0, None);
    let mut m = fixed();
    s.layout(&mut m);
    press(&mut s, "tg");
    s.layout(&mut m);
    assert_eq!(checked(&mut s), json!(true));

    // updateComponents carrying only the Text: the Checkbox keeps its state.
    s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": "msg", "component": "Text", "text": "two"}]}})).expect("apply");
    s.layout(&mut m);
    assert_eq!(checked(&mut s), json!(true));
    assert_eq!(node(&mut s, "msg").props["text"], json!("two"));

    // A host re-sending the merged list unchanged (set_components): kept.
    s.set_components(serde_json::from_value(json!([
        {"id": "root", "component": "Box", "children": ["tg", "msg"]},
        {"id": "tg", "component": "Checkbox", "name": "agree", "label": "Bold"},
        {"id": "msg", "component": "Text", "text": "three"}
    ])).unwrap());
    s.layout(&mut m);
    assert_eq!(checked(&mut s), json!(true));

    // The Checkbox itself re-sent different: its local value resets.
    s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": "tg", "component": "Checkbox", "name": "agree", "label": "Italic"}]}})).expect("apply");
    s.layout(&mut m);
    assert_ne!(checked(&mut s), json!(true));
}

#[test]
fn an_a2ui_function_call_on_a_two_way_macro_is_reported_not_silently_dropped() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "c", "component": "Collapsible",
        "props": {"title": "More", "open": {"path": "/o"}}, "on": {"change": {"functionCall": {"call": "harness.track"}}}})).unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert_eq!(outcome.issues.len(), 1, "{:?}", outcome.issues);
    assert!(outcome.issues[0].message.starts_with("on.change: a function action replaces the two-way set of props.open"));
}
