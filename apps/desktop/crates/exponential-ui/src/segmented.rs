//! VAPP-162 (docs/round-5-contract.md §3): Segmented `orientation:
//! vertical` and `option.count`. A vertical `bar` is THE navigation rail: a
//! column of fixed items (item = the rail's width, 72 in every built-in
//! theme, × `$control.tabBar`), laid out by the core (`layout_tree`). A
//! `bar` item's `count` is a badge at its icon's top-end corner.

use serde_json::Value;

use crate::types::Props;

fn str_prop<'a>(props: &'a Props, key: &str) -> Option<&'a str> {
    props.get(key).and_then(Value::as_str)
}

/// Whether the Segmented stacks its segments top to bottom.
pub fn is_vertical(props: &Props) -> bool {
    str_prop(props, "orientation") == Some("vertical")
}

/// Whether the Segmented is the navigation rail (a vertical `bar`).
pub fn is_rail(props: &Props) -> bool {
    is_vertical(props) && str_prop(props, "variant") == Some("bar")
}

/// The object options a Segmented paints.
pub fn item_count(props: &Props) -> usize {
    props.get("items").and_then(Value::as_array).map_or(0, |a| a.iter().filter(|o| o.is_object()).count())
}

/// The rail's height: one `tab_bar` item per option.
pub fn rail_height(props: &Props, tab_bar: f32) -> f32 {
    item_count(props) as f32 * tab_bar
}

/// The item box of a rail at `index` (x, y, w, h) inside a rail `width` wide.
pub fn rail_item(index: usize, width: f32, tab_bar: f32) -> (f32, f32, f32, f32) {
    (0.0, index as f32 * tab_bar, width, tab_bar)
}

/// The badge text of an option's `count` (`99+` above 99); `None` when
/// unset, not a positive number, or 0 (hidden). Only `bar` items paint it.
pub fn badge_text(count: Option<&Value>) -> Option<String> {
    let n = count.and_then(Value::as_f64).filter(|n| n.is_finite())?.floor();
    if n <= 0.0 {
        None
    } else if n > 99.0 {
        Some("99+".into())
    } else {
        Some(format!("{n}"))
    }
}

/// The badge of an option on a Segmented of `variant` (only `bar` paints one).
pub fn option_badge(variant: Option<&str>, option: &Value) -> Option<String> {
    if variant != Some("bar") {
        return None;
    }
    badge_text(option.get("count"))
}

/// The accessibility name of an item: the label, then the count
/// ("Chats, 3"), when the item shows a badge.
pub fn item_accessibility_label(label: &str, badge: Option<&str>) -> String {
    match badge {
        Some(b) => format!("{label}, {b}"),
        None => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn badges_hide_at_zero_and_cap_at_99() {
        assert_eq!(badge_text(None), None);
        assert_eq!(badge_text(Some(&json!(0))), None);
        assert_eq!(badge_text(Some(&json!(3))), Some("3".into()));
        assert_eq!(badge_text(Some(&json!(99))), Some("99".into()));
        assert_eq!(badge_text(Some(&json!(100))), Some("99+".into()));
        assert_eq!(badge_text(Some(&json!("3"))), None);
        assert_eq!(option_badge(Some("segmented"), &json!({"count": 3})), None);
        assert_eq!(option_badge(Some("bar"), &json!({"count": 3})).as_deref(), Some("3"));
        assert_eq!(item_accessibility_label("Chats", Some("3")), "Chats, 3");
    }
}
