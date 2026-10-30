//! EXP-1259 (the UI batch): the low-level pieces the app settled on, built
//! from the natives every painter already draws.
//!
//! - `SearchList` (packages/domain-contract/fixtures/search-list.json): a
//!   `head` row (the host-owned `search` field + the muted `count` while a
//!   query is typed) over the `body`: a `list` of `item` rows (`icon`,
//!   mono `identifier`, `label`, the picked row's `check`) at most
//!   `maxRows` tall, scrolling, under a bottom `fade` while rows sit below.
//!   The HIGHLIGHTED row (`selected` state) is core state: the field's
//!   arrows move it (no wrap), Enter or Tab picks it, a hover moves it,
//!   typing puts it back on the top row.
//! - A searchable Menu submenu: the same head + list + fade around the
//!   submenu's entries (`menuItem.searchable`).
//! - `Skeleton` rows (feedback-states.json `placeholder`): `rows` × a `row`
//!   (the row height) of a `dot` and two `bar`s, hairline-divided; the dot
//!   and bars are `Skeleton` leaves, so every painter's pulse applies.

use serde_json::{json, Value};

use super::{js, str_prop, Builder, LNode, NodeKind};

/// The bars' widths per placeholder row (lead, title): the web's
/// `SkeletonRows`, so a stack never reads as one ruled block.
pub const SKELETON_BARS: [(f32, f32); 3] = [(56.0, 168.0), (56.0, 120.0), (56.0, 192.0)];

/// The rows a search list shows by default before it scrolls.
pub const SEARCH_LIST_ROWS: usize = 8;

/// Does a search row match the query (case-insensitive, label or identifier)?
pub fn search_matches(label: &str, identifier: &str, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty() || label.to_lowercase().contains(&needle) || identifier.to_lowercase().contains(&needle)
}

/// The scroll offset that keeps row `active` (fixed `row` px rows) inside a
/// `viewport` px window currently at `offset`.
pub fn reveal_offset(active: usize, row: f32, viewport: f32, offset: f32) -> f32 {
    let top = active as f32 * row;
    let bottom = top + row;
    if top < offset {
        top
    } else if bottom > offset + viewport {
        (bottom - viewport).max(0.0)
    } else {
        offset
    }
}

impl Builder<'_, '_> {
    /// The index of the node built with `id` in this build.
    pub(crate) fn slot_of_id(&self, id: &str) -> Option<u32> {
        self.nodes.iter().rposition(|n| n.id == id).map(|i| i as u32)
    }

    /// The rows visible before a search list scrolls (`maxRows`, ≥ 1).
    fn max_rows(props: &crate::types::Props) -> usize {
        props.get("maxRows").and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 1.0).map(|n| n.floor() as usize).unwrap_or(SEARCH_LIST_ROWS)
    }

    /// The highlighted row of the list keyed `key`, clamped to `count`.
    fn active_row(&self, key: &str, count: usize) -> usize {
        self.ctx.local.active.get(key).copied().unwrap_or(0).min(count.saturating_sub(1))
    }

    /// The `head` row: the `search` field (a host-owned one-line field,
    /// component `SearchList`, keyed `key` for its query) and the `count`.
    #[allow(clippy::too_many_arguments)]
    fn search_head(&mut self, parent: u32, owner: &LNode, suffix: &str, placeholder: &str, query: &str, count: Option<usize>, focus_first: bool) -> u32 {
        let head = self.part_in(parent, owner, "head", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}), json!({}), suffix);
        let search = self.part_in(head, owner, "search", "SearchList", NodeKind::Leaf, json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0, "alignSelf": "stretch"}), json!({"placeholder": placeholder, "value": query, "icon": super::builtin_icon("SearchList.search"), "autofocus": focus_first}), suffix);
        self.nodes[search as usize].pressable = true;
        if let Some(n) = count {
            let c = self.part_in(head, owner, "count", "Text", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"text": n.to_string(), "variant": "muted", "lines": 1}), suffix);
            self.nodes[c as usize].live = Some("polite".into());
        }
        search
    }

    /// The scrolling `list` (at most `max_rows` rows tall) inside a
    /// relative `body`; returns (body, list).
    fn search_body(&mut self, parent: u32, owner: &LNode, suffix: &str, max_rows: usize, row: f32) -> (u32, u32) {
        let body = self.part_in(parent, owner, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "position": "relative", "flexShrink": 1, "minHeight": 0}), json!({}), suffix);
        self.nodes[body as usize].part_query = None;
        let max = max_rows as f32 * row;
        let list = self.part_in(body, owner, "list", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "flexShrink": 1, "minHeight": 0, "maxHeight": max, "overflowY": "auto"}), json!({}), suffix);
        (body, list)
    }

    /// The bottom `fade` while rows sit below the list's window.
    #[allow(clippy::too_many_arguments)]
    fn search_fade(&mut self, body: u32, owner: &LNode, suffix: &str, list_id: &str, rows: usize, max_rows: usize, row: f32) {
        if rows <= max_rows {
            return;
        }
        let viewport = max_rows as f32 * row;
        let offset = self.ctx.local.scroll.get(list_id).map(|s| s.1).unwrap_or(0.0);
        if offset + viewport >= rows as f32 * row - 0.5 {
            return;
        }
        let fade = crate::layout::SEARCH_LIST_FADE;
        self.part_in(body, owner, "fade", "Box", NodeKind::Container, json!({"position": "absolute", "left": 0, "right": 0, "bottom": 0, "height": fade}), json!({}), suffix);
    }

    /// SearchList: head + body (list, fade).
    pub(crate) fn search_list(&mut self, index: u32) {
        self.nodes[index as usize].kind = NodeKind::Container;
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column")), ("overflow", json!("hidden"))]);
        let owner = self.nodes[index as usize].clone();
        let items: Vec<Value> = owner.props.get("items").and_then(Value::as_array).map(|a| a.iter().filter(|o| o.is_object()).cloned().collect()).unwrap_or_default();
        let query = self.ctx.local.query.get(&owner.id).cloned().or_else(|| owner.props.get("query").map(js)).unwrap_or_default();
        // A host that searches (`on.search`) ranks the rows itself.
        let host_filters = owner.on.as_ref().is_some_and(|o| o.contains_key("search"));
        let shown: Vec<(usize, &Value)> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| host_filters || search_matches(&it.get("label").map(js).unwrap_or_default(), &it.get("identifier").map(js).unwrap_or_default(), &query))
            .collect();
        let max_rows = Self::max_rows(&owner.props);
        let show_count = owner.props.get("showCount").and_then(Value::as_bool).unwrap_or(true) && !query.trim().is_empty();
        let placeholder = str_prop(&owner.props, "placeholder").map(str::to_string).unwrap_or_else(|| self.string("search"));
        self.search_head(index, &owner, "", &placeholder, &query, show_count.then_some(shown.len()), false);
        let row = self.ctx.row_extent;
        let (body, list) = self.search_body(index, &owner, "", max_rows, row);
        let active = self.active_row(&owner.id, shown.len());
        let picked = owner.props.get("value").map(js).unwrap_or_default();
        for (pos, (i, item)) in shown.iter().enumerate() {
            let suffix = format!(".{i}");
            let value = item.get("value").map(js).unwrap_or_default();
            let label = item.get("label").map(js).unwrap_or_default();
            let disabled = item.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            let row = self.part_in(list, &owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}), json!({"value": value, "label": label, "position": pos, "disabled": disabled}), &suffix);
            self.nodes[row as usize].pressable = !disabled;
            if pos == active {
                self.add_state(row, "selected");
            }
            if disabled {
                self.add_state(row, "disabled");
            }
            if let Some(icon) = item.get("icon").filter(|v| v.is_string()) {
                self.part_in(row, &owner, "icon", "Icon", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"name": icon, "size": "sm"}), &suffix);
            }
            if let Some(identifier) = item.get("identifier").map(js).filter(|s| !s.is_empty()) {
                self.part_in(row, &owner, "identifier", "Text", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"text": identifier, "lines": 1}), &suffix);
            }
            self.part_in(row, &owner, "label", "Text", NodeKind::Leaf, json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}), json!({"text": label, "lines": 1}), &suffix);
            if !picked.is_empty() && value == picked {
                self.icon_part(row, &owner, "check", "SearchList.check", &suffix);
            }
        }
        if shown.is_empty() {
            let empty = owner.props.get("emptyText").map(js).unwrap_or_else(|| self.string("noResults"));
            self.text_part(list, &owner, "empty", &empty, "muted", "");
        }
        let list_id = self.nodes[list as usize].id.clone();
        self.search_fade(body, &owner, "", &list_id, shown.len(), max_rows, row);
    }

    /// A searchable Menu submenu's content (`menuItem.searchable`): head,
    /// then the entries that match the query as menu rows (the highlighted
    /// one `selected`), the empty line, the fade. `suffix` = the submenu
    /// row's (`.2`), so the parts are `<menu>.search.2`, `<menu>.list.2`.
    pub(crate) fn menu_search(&mut self, root: u32, owner: &LNode, item: &Value, suffix: &str) {
        let key = format!("{}.search{suffix}", owner.id);
        let query = self.ctx.local.query.get(&key).cloned().unwrap_or_default();
        let sub_items = item.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let entries: Vec<(usize, Value)> = sub_items
            .into_iter()
            .enumerate()
            .filter(|(_, it)| !matches!(it.get("kind").and_then(Value::as_str), Some("separator" | "label")))
            .filter(|(_, it)| search_matches(&it.get("label").map(js).unwrap_or_default(), "", &query))
            .collect();
        let placeholder = str_prop(item.as_object().unwrap_or(&serde_json::Map::new()), "placeholder").map(str::to_string).unwrap_or_else(|| self.string("search"));
        let count = (!query.trim().is_empty()).then_some(entries.len());
        self.search_head(root, owner, suffix, &placeholder, &query, count, true);
        let row = self.ctx.menu_row;
        let (body, list) = self.search_body(root, owner, suffix, SEARCH_LIST_ROWS, row);
        let active = self.active_row(&key, entries.len());
        for (pos, (i, entry)) in entries.iter().enumerate() {
            let row_suffix = format!("{suffix}.{i}");
            let disabled = entry.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            let label = entry.get("label").map(js).unwrap_or_default();
            let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("item");
            let mut props = json!({"text": label, "value": entry.get("value").cloned().unwrap_or(Value::Null), "kind": kind, "disabled": disabled, "lines": 1, "position": pos});
            if kind == "checkbox" {
                props["checked"] = json!(entry.get("checked").and_then(Value::as_bool).unwrap_or(false));
            }
            let row = self.part_in(list, owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": "$spacing.sm", "flexShrink": 0}), props, &row_suffix);
            self.nodes[row as usize].pressable = !disabled;
            if pos == active {
                self.add_state(row, "selected");
            }
            if disabled {
                self.add_state(row, "disabled");
            }
            let mut label_props = json!({"text": label, "lines": 1});
            if let Some(icon) = entry.get("icon") {
                label_props["icon"] = icon.clone();
            }
            let l = self.part_in(row, owner, "itemLabel", "Text", NodeKind::Leaf, json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}), label_props, &row_suffix);
            self.nodes[l as usize].part_query = None;
            if kind == "checkbox" && entry.get("checked").and_then(Value::as_bool) == Some(true) {
                let glyph = format!("{}.check", owner.component);
                self.icon_part(row, owner, "check", &glyph, &row_suffix);
            }
        }
        if entries.is_empty() {
            let empty = self.string("noResults");
            self.text_part(list, owner, "empty", &empty, "muted", suffix);
        }
        let list_id = self.nodes[list as usize].id.clone();
        self.search_fade(body, owner, suffix, &list_id, entries.len(), SEARCH_LIST_ROWS, row);
    }

    /// Skeleton with `rows`: the root holds `rows` placeholder rows (the
    /// row height, else `rowHeight`), each a `dot` (unless `dot: false`)
    /// and two `bar`s; the root paints nothing itself.
    pub(crate) fn skeleton_rows(&mut self, index: u32) {
        let owner = self.nodes[index as usize].clone();
        let rows = owner.props.get("rows").and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 1.0).map(|n| n.floor().min(50.0) as usize).unwrap_or(0);
        if rows == 0 {
            return;
        }
        self.nodes[index as usize].kind = NodeKind::Container;
        // The block's own recipe (its fill) gives way to the rows'.
        self.nodes[index as usize].own_query = None;
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column")), ("alignSelf", json!("stretch"))]);
        let dot = owner.props.get("dot").and_then(Value::as_bool).unwrap_or(true);
        let height = owner.props.get("rowHeight").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0);
        for r in 0..rows {
            let suffix = format!(".{r}");
            // The ROW height (the recipe only spaces the row): `rowHeight`, else `$control.row`.
            let mut style = json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "height": height.map(|h| json!(h)).unwrap_or_else(|| json!("$control.row"))});
            if r > 0 {
                style["borderTopWidth"] = json!("$border.hairline");
                style["borderColor"] = json!("$color.border");
            }
            let row = self.part_in(index, &owner, "row", "Box", NodeKind::Container, style, json!({}), &suffix);
            let leaf = |b: &mut Self, part: &str, style: Value, suffix: &str| b.part_in(row, &owner, part, "Skeleton", NodeKind::Leaf, style, json!({}), suffix);
            if dot {
                leaf(self, "dot", json!({"flexShrink": 0}), &suffix);
            }
            // The two bars: `<id>.bar.<row>.0` (the lead) and `.1` (the title).
            let (lead, title) = SKELETON_BARS[r % SKELETON_BARS.len()];
            leaf(self, "bar", json!({"width": lead, "flexShrink": 0}), &format!("{suffix}.0"));
            leaf(self, "bar", json!({"width": title, "flexShrink": 1, "minWidth": 0}), &format!("{suffix}.1"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_matches_the_label_or_the_identifier_case_insensitively() {
        assert!(search_matches("Ship the search list", "EXP-12", ""));
        assert!(search_matches("Ship the search list", "EXP-12", "SEARCH"));
        assert!(search_matches("Ship the search list", "EXP-12", "exp-1"));
        assert!(!search_matches("Ship the search list", "EXP-12", "tabs"));
    }

    #[test]
    fn the_highlighted_row_scrolls_into_the_window() {
        // 32 px rows, 8 visible.
        assert_eq!(reveal_offset(0, 32.0, 256.0, 0.0), 0.0);
        assert_eq!(reveal_offset(8, 32.0, 256.0, 0.0), 32.0);
        assert_eq!(reveal_offset(3, 32.0, 256.0, 64.0), 64.0);
        assert_eq!(reveal_offset(1, 32.0, 256.0, 64.0), 32.0);
    }
}
