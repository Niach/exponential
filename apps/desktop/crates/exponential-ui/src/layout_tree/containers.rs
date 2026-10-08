//! Tabs, Accordion, Carousel and the windowed List.

use serde_json::{json, Value};

use super::{bool_prop, js, str_prop, Builder, ListSpec, NodeKind};
use crate::list::{ListWindow, VisibleRange, WINDOW_THRESHOLD};
use crate::types::UiNode;

impl Builder<'_, '_> {
    pub(crate) fn tabs(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let tabs = owner.props.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
        let values: Vec<String> = tabs.iter().map(|t| t.get("value").map(js).unwrap_or_default()).collect();
        let active = self
            .ctx
            .local
            .tabs
            .get(&owner.id)
            .cloned()
            .or_else(|| owner.props.get("value").map(js))
            .filter(|v| values.contains(v))
            .or_else(|| values.first().cloned())
            .unwrap_or_default();
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        let fill = bool_prop(&owner.props, "fill");
        let list = self.part(&owner, "list", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "stretch", "alignSelf": if fill { "stretch" } else { "flex-start" }}), json!({}));
        for (i, tab) in tabs.iter().enumerate() {
            let label = tab.get("label").map(js).unwrap_or_default();
            let mut props = json!({"text": label, "lines": 1, "value": values[i]});
            if let Some(icon) = tab.get("icon") {
                props["icon"] = icon.clone();
            }
            if let Some(count) = tab.get("count") {
                props["count"] = count.clone();
            }
            let selected = values[i] == active;
            let mut style = json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center"});
            if fill {
                style["flexGrow"] = json!(1);
                style["flexBasis"] = json!(0);
            }
            let t = self.part_in(list, &owner, "tab", "Text", NodeKind::Leaf, style, props, &format!(".{i}"));
            self.query_prop(t, "selected", Value::Bool(selected));
            if selected {
                self.add_state(t, "selected");
            }
            self.nodes[t as usize].pressable = true;
        }
        let content = self.part(&owner, "content", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column"}), json!({}));
        let count = self.child_keys(node, scope).len();
        for i in 0..count {
            let hidden = values.get(i) != Some(&active);
            self.one_child(content, node, i, scope, hidden);
        }
    }

    pub(crate) fn accordion(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let items = owner.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let multiple = str_prop(&owner.props, "type") == Some("multiple");
        let open: Vec<String> = match self.ctx.local.accordion.get(&owner.id) {
            Some(v) => v.clone(),
            None => owner.props.get("value").map(js).map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
        };
        let open: Vec<String> = if multiple { open } else { open.into_iter().take(1).collect() };
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        for (i, item) in items.iter().enumerate() {
            let value = item.get("value").map(js).unwrap_or_default();
            let is_open = open.contains(&value);
            let suffix = format!(".{i}");
            let wrapper = self.part_in(index, &owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column"}), json!({}), &suffix);
            let title = item.get("title").map(js).unwrap_or_default();
            let mut props = json!({"text": title, "lines": 1, "value": value});
            if let Some(count) = item.get("count") {
                props["count"] = count.clone();
            }
            let trigger = self.part_in(wrapper, &owner, "trigger", "Text", NodeKind::Leaf, json!({"display": "flex", "flexDirection": "row", "alignItems": "center"}), props, &suffix);
            self.nodes[trigger as usize].pressable = true;
            self.query_prop(trigger, "open", Value::Bool(is_open));
            if is_open {
                self.add_state(trigger, "open");
            }
            let content = self.part_in(wrapper, &owner, "content", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column"}), json!({}), &suffix);
            self.nodes[content as usize].hidden |= !is_open;
            self.one_child(content, node, i, scope, !is_open);
        }
    }

    pub(crate) fn carousel(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let count = self.child_keys(node, scope).len();
        let page = self.ctx.local.carousel.get(&owner.id).copied().or_else(|| owner.props.get("page").and_then(Value::as_i64)).unwrap_or(0);
        let page = if count == 0 { 0 } else { page.rem_euclid(count as i64) as usize };
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        let pages = self.part(&owner, "page", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "overflow": "hidden"}), json!({}));
        for i in 0..count {
            self.one_child(pages, node, i, scope, i != page);
        }
        if owner.props.get("indicators").and_then(Value::as_bool).unwrap_or(true) && count > 1 {
            self.part(&owner, "indicator", "Box", NodeKind::Leaf, json!({"alignSelf": "center"}), json!({"count": count, "page": page}));
        }
    }

    pub(crate) fn list(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let horizontal = str_prop(&owner.props, "direction") == Some("horizontal");
        let divided = bool_prop(&owner.props, "divided");
        let gap_name = str_prop(&owner.props, "gap").unwrap_or("none");
        let gap = (self.ctx.gap_px)(gap_name);
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!(if horizontal { "row" } else { "column" })), ("gap", json!(gap))]);
        let keys = self.child_keys(node, scope);
        let count = keys.len();
        let windowed = !horizontal && count > WINDOW_THRESHOLD;
        let range = self.window(&owner.id, &keys, gap, windowed, index);
        for i in range.start..range.end {
            if divided && i > range.start {
                let style = if horizontal { json!({"width": 1, "alignSelf": "stretch", "flexShrink": 0}) } else { json!({"height": 1, "alignSelf": "stretch", "flexShrink": 0}) };
                self.part_in(index, &owner, "divider", "Box", NodeKind::Container, style, json!({}), &format!(".{i}"));
            }
            self.one_child(index, node, i, scope, false);
        }
        self.window_end(index);
    }

    /// The visible range of a (possibly windowed) row container `index`
    /// keyed `id`. A windowed one scrolls; a leading SPACER (part `spacer`,
    /// before the rows) stands in for the rows above the window; the caller
    /// adds the trailing one after the rows ([`Self::window_end`]).
    pub(crate) fn window(&mut self, id: &str, keys: &std::sync::Arc<Vec<String>>, gap: f32, windowed: bool, index: u32) -> VisibleRange {
        let count = keys.len();
        let viewport = self.ctx.viewport_height.max(1.0);
        let scroll_y = self.ctx.local.scroll.get(id).map(|s| s.1);
        let window = self.ctx.local.lists.entry(id.to_string()).or_insert_with(|| ListWindow::new(crate::list::DEFAULT_ESTIMATED_ITEM_HEIGHT, crate::list::DEFAULT_OVERSCAN, gap));
        window.gap = gap;
        if let Some(y) = scroll_y {
            window.scroll_offset = y;
        }
        let offsets = window.offsets_cached(keys);
        let content_height = offsets.last().copied().unwrap_or(0.0);
        let range = if windowed { window.visible_range(&offsets, viewport) } else { VisibleRange { start: 0, end: count } };
        if windowed {
            self.nodes[index as usize].base_style.entry("overflowY".to_string()).or_insert(json!("scroll"));
            if range.start > 0 {
                self.spacer(index, "spacerStart", (offsets[range.start] - gap).max(0.0));
            }
        }
        self.lists.push(ListSpec { node: index, id: id.to_string(), keys: keys.clone(), range, offsets, content_height, windowed });
        range
    }

    /// The trailing spacer of a windowed row container (the rows below).
    pub(crate) fn window_end(&mut self, index: u32) {
        let Some(spec) = self.lists.iter().rev().find(|l| l.node == index).cloned() else { return };
        let count = spec.keys.len();
        if spec.windowed && spec.range.end < count {
            self.spacer(index, "spacerEnd", (spec.content_height - spec.offsets[spec.range.end]).max(0.0));
        }
    }

    fn spacer(&mut self, parent: u32, part: &str, height: f32) {
        let owner = self.nodes[parent as usize].clone();
        let id = format!("{}.{part}", owner.id);
        let mut n = self.blank(&id, "Box", Some(parent), owner.layer, NodeKind::Container, &owner.scope);
        n.part = Some("spacer".into());
        n.owner = owner.owner.clone().or(Some(owner.id.clone()));
        n.owner_component = owner.owner_component.clone().or(Some(owner.component.clone()));
        n.base_style = super::obj(json!({"height": height, "flexShrink": 0}));
        self.push(n);
    }
}
