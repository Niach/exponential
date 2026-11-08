//! FEED-202: a Table's `totals` row in the layout core: one `row` part after
//! the body, each number / currency column's sum in its cell format,
//! `Total` in a first column that sums nothing; a column `format` formats
//! its dates.

use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::json;

#[test]
fn the_totals_row_sums_and_formats() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "t", "component": "Table", "props": {
        "columns": [
            {"key": "vendor", "label": "Vendor"},
            {"key": "date", "label": "Date", "type": "date", "format": "dd.MM.yy", "width": 104},
            {"key": "net", "label": "Net", "type": "currency", "currency": "EUR", "decimals": 2, "align": "end", "width": 96}
        ],
        "rows": [
            {"id": "a", "vendor": "Hetzner", "date": "2026-11-03", "net": 38.66},
            {"id": "b", "vendor": "Libro", "date": "2026-10-21", "net": "20.75"}
        ],
        "totals": true
    }}))
    .expect("tree");
    let mut s = Surface::new("s", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(600.0, 400.0, None);
    let nodes = s.nodes();
    let text = |id: &str| nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no {id}")).props["text"].clone();
    assert_eq!(text("t.cell.a.1"), json!("03.11.26"));
    assert_eq!(text("t.cell.#totals.0"), json!("Total"));
    assert_eq!(text("t.cell.#totals.1"), json!(""));
    assert!(text("t.cell.#totals.2").as_str().is_some_and(|t| t.contains("59.41")), "{}", text("t.cell.#totals.2"));
}
