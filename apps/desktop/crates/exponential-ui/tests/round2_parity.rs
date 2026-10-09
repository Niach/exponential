//! Round 2 painter parity (the gpui lane's core fixes, each one a web
//! behaviour the conformance run showed): one-line leaves clip on x (taffy
//! floored their max-content contribution at the PARENT's padding), aspect
//! ratios transfer from a known width, builder defaults sit under the
//! native's root recipe, type inherits like CSS, a sticky child of the
//! scroller stays pinned all the way, Table text cells share a row with slot
//! cells equally, TreeGuides stretch to their row.

use exponential_ui::measure::{DirectMeasureWithBaseline, LeafRequest};
use exponential_ui::surface::{Surface, SurfaceOptions};
use exponential_ui::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: exponential_ui::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(400.0, 0.0, None);
    s
}

/// Text: 7 px per char on one line (min-content 0 for `lines: 1`, the
/// widest word otherwise); everything else 16 × 16.
type Answer = (f32, f32, Option<f32>);

fn measure() -> DirectMeasureWithBaseline<impl FnMut(&LeafRequest, Option<f32>) -> Answer> {
    DirectMeasureWithBaseline(
        |leaf: &LeafRequest, wrap: Option<f32>| {
            let text = leaf.text();
            if leaf.component != "Text" {
                return (16.0, 16.0, None);
            }
            let max = 7.0 * text.chars().count() as f32;
            let lh = leaf.text_style.line_height;
            match wrap {
                None => (max, lh, None),
                Some(w) if w <= 0.0 => (if leaf.lines == Some(1) { 0.0 } else { 7.0 * text.split(' ').map(|s| s.chars().count()).max().unwrap_or(0) as f32 }, lh, None),
                Some(w) => (max.min(w), lh, None),
            }
        },
        11,
    )
}

fn frame(s: &mut Surface, id: &str) -> (f32, f32, f32, f32) {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no {id}"));
    let f = s.last_frame(i).unwrap_or_else(|| panic!("{id} not placed"));
    (f.x, f.y, f.w, f.h)
}

#[test]
fn a_one_line_leaf_in_a_padded_row_is_its_content_wide() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "row", "component": "Box", "style": {"display": "flex", "flexDirection": "row"}, "children": [{"id": "b", "component": "Badge", "props": {"text": "3"}}]}
    ]}));
    s.layout(&mut measure());
    let (_, _, w, _) = frame(&mut s, "b");
    let (_, _, lw, _) = frame(&mut s, "b.label");
    assert_eq!((lw, w), (7.0, 7.0 + 16.0), "2 × paddingHorizontal + the label (taffy gave 2 × the padding)");
}

#[test]
fn media_take_their_aspect_ratio_from_a_stretched_width() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "v", "component": "Video", "props": {"src": "https://example.com/v.mp4"}},
        {"id": "i", "component": "Image", "props": {"src": "https://example.com/i.png", "alt": "", "aspectRatio": 2}},
        {"id": "fixed", "component": "Image", "props": {"src": "https://example.com/i.png", "alt": "", "width": 16, "height": 16}}
    ]}));
    s.layout(&mut measure());
    let (_, _, w, h) = frame(&mut s, "v");
    assert!((w - 400.0).abs() < 0.01 && (h - 400.0 / 1.777_777_8).abs() < 0.1, "Video 16:9 by default: {w}×{h}");
    assert_eq!(frame(&mut s, "i").3, 200.0, "Image: its aspectRatio");
    assert_eq!(frame(&mut s, "fixed").3, 16.0, "an Image with a height keeps it");
}

#[test]
fn a_native_root_recipe_wins_over_the_builders_defaults() {
    let mut s = surface(json!({"id": "f", "component": "Form", "props": {"name": "f"}, "children": [
        {"id": "a", "component": "Text", "props": {"text": "A"}},
        {"id": "b", "component": "Text", "props": {"text": "B"}}
    ]}));
    s.layout(&mut measure());
    let (a, b) = (frame(&mut s, "a"), frame(&mut s, "b"));
    assert_eq!(b.1 - (a.1 + a.3), 16.0, "Form/root gap ($spacing.lg), not the builder's md");
    let mut authored = surface(json!({"id": "f", "component": "Form", "props": {"name": "f"}, "style": {"gap": 4}, "children": [
        {"id": "a", "component": "Text", "props": {"text": "A"}},
        {"id": "b", "component": "Text", "props": {"text": "B"}}
    ]}));
    authored.layout(&mut measure());
    let (a, b) = (frame(&mut authored, "a"), frame(&mut authored, "b"));
    assert_eq!(b.1 - (a.1 + a.3), 4.0, "the author's style wins over the recipe");
}

#[test]
fn type_inherits_like_css() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "code", "component": "CodeBlock", "props": {"code": "let a = 1", "title": "a.ts"}},
        {"id": "box", "component": "Box", "style": {"fontFamily": "$type.family.mono", "lineHeight": 30}, "children": [{"id": "inner", "component": "Box", "children": [{"id": "body", "component": "Text", "props": {"text": "x"}}]}]}
    ]}));
    s.layout(&mut measure());
    let style = |s: &Surface, id: &str| s.text_style(s.index_of(id).unwrap()).cloned().unwrap();
    let title = style(&s, "code.title");
    assert_eq!(title.line_height, 20.0, "an unset line height is the inherited px value ($type.lineHeight.sm), not normal for the smaller size");
    let code = style(&s, "code.code.0");
    assert!(code.font_family.as_deref().is_some_and(|f| !f.is_empty()), "the code line inherits the root's mono family");
    let inner = style(&s, "inner");
    assert_eq!((inner.line_height, inner.font_family.is_some()), (30.0, true), "a box inherits its parent's line height and family");
    assert_eq!(style(&s, "body").line_height, 20.0, "a Text variant sets its own line height (body)");
}

#[test]
fn a_sticky_child_of_the_scroller_stays_pinned_all_the_way() {
    let mut children = vec![json!({"id": "pin", "component": "Box", "style": {"position": "sticky", "top": 0, "height": 30, "flexShrink": 0}})];
    children.extend((0..20).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 40, "flexShrink": 0}})));
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "pane", "component": "Box", "style": {"height": 200, "overflowY": "auto", "display": "flex", "flexDirection": "column"}, "children": children}
    ]}));
    s.layout(&mut measure());
    s.scroll_to("pane", 0.0, 500.0);
    let out = s.layout(&mut measure());
    let pin = s.index_of("pin").unwrap();
    let dy = out.sticky.iter().find(|p| p.index == pin).map(|p| p.dy);
    assert_eq!(dy, Some(500.0), "pinned at the scroller's top past its own frame height");
}

#[test]
fn table_text_cells_and_slot_cells_share_the_row_equally() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "t", "component": "Table", "props": {"columns": [{"key": "a", "label": "A"}, {"key": "b", "label": "B"}], "rows": [{"id": "x", "a": "one", "b": "two"}]},
         "slots": {"cell": {"id": "slot", "component": "Chip", "props": {"label": {"path": "b"}}}}}
    ]}));
    s.layout(&mut measure());
    let nodes: Vec<String> = s.nodes().into_iter().map(|n| n.id).collect();
    let cells: Vec<f32> = nodes.iter().filter(|id| id.starts_with("t.cell.x.")).map(|id| frame(&mut s, id).2).collect();
    assert!(cells.len() == 2 && (cells[0] - cells[1]).abs() < 0.01, "equal columns: {cells:?}");
}

#[test]
fn tree_guides_are_depth_columns_wide_and_stretch_to_their_row() {
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center"}, "children": [
            {"id": "g", "component": "TreeGuides", "props": {"depth": 2}},
            {"id": "label", "component": "Text", "props": {"text": "Nested", "variant": "caption"}}
        ]}
    ]}));
    s.layout(&mut measure());
    let (g, label) = (frame(&mut s, "g"), frame(&mut s, "label"));
    assert_eq!((g.2, g.3), (28.0, label.3), "depth × 14, the row's height");
}

#[test]
fn a_tree_section_of_rows_reserves_the_gutter_the_core_filled() {
    // Round 3: Row `depth` emits a TreeGuides part; the core fills its
    // elbow / tee / passThrough from the sibling rows.
    let mut s = surface(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "tree", "component": "Section", "props": {"tree": true}, "children": [
            {"id": "a", "component": "Row", "props": {"title": "Root"}},
            {"id": "b", "component": "Row", "props": {"title": "Child", "depth": 1}},
            {"id": "c", "component": "Row", "props": {"title": "Grandchild", "depth": 2}},
            {"id": "d", "component": "Row", "props": {"title": "Second child", "depth": 1}}
        ]}
    ]}));
    s.layout(&mut measure());
    let nodes = s.nodes();
    let guides = |id: &str| nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("{id}")).props.clone();
    assert_eq!(guides("b.guides"), *json!({"depth": 1, "elbowAt": 0, "tee": true, "passThrough": []}).as_object().unwrap());
    assert_eq!(guides("c.guides"), *json!({"depth": 2, "elbowAt": 1, "tee": false, "passThrough": [0]}).as_object().unwrap());
    assert!(nodes.iter().all(|n| n.id != "a.guides"), "a root row has no guides part");
    assert_eq!(frame(&mut s, "c.guides").2, 28.0);
}
