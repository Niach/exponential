//! A synthetic ~n-node surface for the layout-time measurement: a header row,
//! then a 3-column grid of cards, each card = row(avatar, column(title, meta),
//! badge) = 6 nodes.

use serde_json::json;

pub fn bench_tree(n: usize) -> String {
    let mut cards = Vec::new();
    let header_nodes = 4; // root + header + 2 texts
    let card_nodes = 6;
    let count = n.saturating_sub(header_nodes).div_ceil(card_nodes).max(1);
    for i in 0..count {
        cards.push(json!({
            "id": format!("card-{i}"),
            "kind": "card",
            "style": { "display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 8, "padding": 10, "minWidth": 0 },
            "children": [
                { "id": format!("av-{i}"), "kind": "avatar", "props": { "name": format!("User {i}"), "size": 28 } },
                {
                    "id": format!("col-{i}"),
                    "kind": "box",
                    "style": { "display": "flex", "flexDirection": "column", "flexGrow": 1, "minWidth": 0, "gap": 2 },
                    "children": [
                        { "id": format!("title-{i}"), "kind": "text", "props": { "text": format!("Draft reply #{i} to r/selfhosted"), "variant": "body" } },
                        { "id": format!("meta-{i}"), "kind": "text", "props": { "text": format!("{} min ago · 2 sources", i % 59), "variant": "muted" } }
                    ]
                },
                { "id": format!("badge-{i}"), "kind": "badge", "props": { "count": i % 9 + 1 } }
            ]
        }));
    }
    json!({
        "id": "root",
        "kind": "box",
        "style": { "display": "flex", "flexDirection": "column", "gap": 12, "padding": 16, "width": "100%" },
        "children": [
            {
                "id": "header",
                "kind": "box",
                "style": { "display": "flex", "flexDirection": "row", "alignItems": "center", "gap": 8 },
                "children": [
                    { "id": "hdr-title", "kind": "text", "props": { "text": "Bench surface", "variant": "title" } },
                    { "id": "hdr-sub", "kind": "text", "props": { "text": format!("{} cards", count), "variant": "muted" } }
                ]
            },
            {
                "id": "grid",
                "kind": "box",
                "style": {
                    "display": "grid",
                    "gap": 8,
                    "gridTemplateColumns": "1fr",
                    "@media (min-width: 500px)": { "gridTemplateColumns": "repeat(2, minmax(0, 1fr))" },
                    "@media (min-width: 900px)": { "gridTemplateColumns": "repeat(3, minmax(0, 1fr))" }
                },
                "children": cards
            }
        ]
    })
    .to_string()
}
