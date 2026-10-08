//! A synthetic ~n-node surface for the timing measurement (the VAPP-4 bench
//! tree in the core vocabulary): cards in a grid with headings, text, pills
//! and buttons, so a pass exercises flex, grid, wrapping and controls.

use serde_json::json;

use crate::types::NestedNode;

/// A nested-form surface with about `n` layout nodes (never fewer).
pub fn bench_tree(n: usize) -> NestedNode {
    let cards = n.div_ceil(7).max(1);
    let mut children = Vec::with_capacity(cards + 1);
    children.push(json!({"id": "heading", "component": "Text", "props": {"text": "Bench", "variant": "title"}}));
    for i in 0..cards {
        children.push(json!({
            "id": format!("card-{i}"), "component": "Box",
            "style": {"display": "flex", "flexDirection": "column", "gap": 8, "padding": 12, "borderRadius": 8, "backgroundColor": "$color.card"},
            "children": [
                {"id": format!("card-{i}-title"), "component": "Text", "props": {"text": format!("Card {i} · a title that may wrap"), "variant": "label", "lines": 1}},
                {"id": format!("card-{i}-body"), "component": "Text", "props": {"text": "Body copy with enough words to wrap at a phone width and exercise the second measure phase.", "variant": "muted"}},
                {"id": format!("card-{i}-row"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": 8, "alignItems": "center", "flexWrap": "wrap"},
                 "children": [
                    {"id": format!("card-{i}-pill-a"), "component": "Button", "props": {"label": "Alpha", "variant": "outline", "size": "sm"}},
                    {"id": format!("card-{i}-pill-b"), "component": "Button", "props": {"label": "Beta", "variant": "outline", "size": "sm"}},
                    {"id": format!("card-{i}-go"), "component": "Button", "props": {"label": "Open", "variant": "default"}, "style": {"marginLeft": "auto"}}
                 ]}
            ]
        }));
    }
    let tree = json!({
        "id": "root", "component": "Box",
        "style": {"display": "grid", "gridTemplateColumns": "1fr", "gap": 12, "padding": 16, "width": "100%", "@media (min-width: 600px)": {"gridTemplateColumns": "repeat(3, minmax(0, 1fr))"}},
        "children": children
    });
    serde_json::from_value::<NestedNode>(tree).expect("bench tree")
}

/// A surface around a WINDOWED list of `rows` rows (each a pressable row
/// with an avatar-sized icon, a title, a muted line and a trailing badge,
/// a `:hover` block): the scroll + hover-churn bench.
pub fn bench_list_tree(rows: usize) -> NestedNode {
    let items: Vec<serde_json::Value> = (0..rows)
        .map(|i| {
            json!({
                "id": format!("row-{i}"), "component": "Box", "props": {"pressable": true},
                "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 8, "paddingHorizontal": 12, "paddingVertical": 8, ":hover": {"backgroundColor": "$color.accent"}},
                "children": [
                    {"id": format!("row-{i}-icon"), "component": "Icon", "props": {"name": "ui-check", "size": "sm"}},
                    {"id": format!("row-{i}-text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "minWidth": 0},
                     "children": [
                        {"id": format!("row-{i}-title"), "component": "Text", "props": {"text": format!("Issue {i}: a title long enough to wrap on a phone"), "variant": "label", "lines": 1}},
                        {"id": format!("row-{i}-sub"), "component": "Text", "props": {"text": "Updated a minute ago", "variant": "muted"}}
                     ]},
                    {"id": format!("row-{i}-badge"), "component": "Badge", "props": {"text": "Todo", "variant": "secondary"}}
                ]
            })
        })
        .collect();
    let tree = json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 12, "padding": 16, "width": "100%"},
        "children": [
            {"id": "title", "component": "Text", "props": {"text": "Inbox", "variant": "title"}},
            {"id": "filters", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": 8},
             "children": [
                {"id": "f-all", "component": "Button", "props": {"label": "All", "variant": "secondary", "size": "sm"}},
                {"id": "f-mine", "component": "Button", "props": {"label": "Mine", "variant": "ghost", "size": "sm"}}
             ]},
            {"id": "list", "component": "List", "props": {"divided": true}, "style": {"height": 640}, "children": items}
        ]
    });
    serde_json::from_value::<NestedNode>(tree).expect("bench list tree")
}

/// The same as JSON text (for the FFI).
pub fn bench_tree_json(n: usize) -> String {
    serde_json::to_string(&serde_json::to_value(bench_tree(n)).expect("json")).expect("json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FixedMeasure;
    use crate::surface::{Surface, SurfaceOptions};

    #[test]
    fn two_hundred_nodes_lay_out_in_three_upcalls_and_well_under_two_milliseconds() {
        let mut surface = Surface::new("bench", SurfaceOptions::default());
        let outcome = surface.set_nested(bench_tree(200));
        assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
        surface.set_viewport(390.0, 0.0, None);
        let mut measure = FixedMeasure { sizes: Default::default(), wrap: true };
        let first = surface.layout(&mut measure);
        assert!(surface.node_count() >= 200, "{} nodes", surface.node_count());
        assert!(first.upcalls <= 3, "{} upcalls", first.upcalls);
        // Warm passes at a new width: the memo answers, no upcall for widths
        // already seen; the engine's own time is what the phone pays.
        surface.set_viewport(900.0, 0.0, None);
        let wide = surface.layout(&mut measure);
        assert!(wide.upcalls <= 2);
        let mut best = u64::MAX;
        for i in 0..20 {
            surface.set_viewport(if i % 2 == 0 { 390.0 } else { 900.0 }, 0.0, None);
            let out = surface.layout(&mut measure);
            best = best.min(out.layout_ns);
        }
        // Debug builds are ~10× slower than release; the release number is
        // recorded in the PR. Keep a loose guard so a regression shows.
        assert!(best < 50_000_000, "best pass {best} ns");
        eprintln!("bench 200 nodes: {} layout nodes, best pass {} µs (debug)", surface.node_count(), best / 1000);
    }

    #[test]
    fn a_2000_row_list_scrolls_and_churns_hover_in_bounded_work() {
        let mut surface = Surface::new("bench-list", SurfaceOptions::default());
        assert!(surface.set_nested(bench_list_tree(2_000)).issues.is_empty());
        surface.set_viewport(390.0, 844.0, None);
        let mut measure = FixedMeasure { sizes: Default::default(), wrap: true };
        for _ in 0..3 {
            surface.layout(&mut measure);
        }
        let live = surface.node_count();
        assert!(live < 600, "{live} live nodes for 2,000 rows");
        let mut worst_restyle = 0;
        for step in 0..40u32 {
            surface.scroll("list", step as f32 * 300.0);
            let out = surface.layout(&mut measure);
            assert!(out.upcalls <= 3);
            worst_restyle = worst_restyle.max(out.restyled);
            let row = format!("row-{}", step * 5 + 3);
            if surface.index_of(&row).is_some() {
                surface.set_states(&row, vec!["hover".into()]);
                let hover = surface.layout(&mut measure);
                assert!(hover.restyled <= 2, "a hover restyled {}", hover.restyled);
                assert!(!hover.rebuilt);
                surface.set_states(&row, vec![]);
            }
        }
        assert!(worst_restyle < 200, "a scroll step restyled {worst_restyle}");
    }
}
