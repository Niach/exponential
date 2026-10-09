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
        // Round 2: role descriptions `$string.carousel` / `$string.slide`.
        let (carousel, slide) = (self.string("carousel"), self.string("slide"));
        self.default_a11y(index, "roleDescription", json!(carousel));
        let pages = self.part(&owner, "page", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "overflow": "hidden"}), json!({}));
        self.default_a11y(pages, "roleDescription", json!(slide));
        for i in 0..count {
            self.one_child(pages, node, i, scope, i != page);
        }
        if count < 2 {
            return;
        }
        // The controls row under the pages: previous, the dots, next. Each
        // button carries the page it goes to (`target`).
        let controls = self.part(&owner, "controls", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center"}), json!({}));
        let wraps = bool_prop(&owner.props, "loop");
        let last = count - 1;
        let nav = |b: &mut Self, part: &str, target: usize, disabled: bool| {
            let props = json!({"icon": super::builtin_icon(&format!("Carousel.{part}")), "label": b.string(part), "variant": "ghost", "size": "icon", "target": target, "disabled": disabled});
            let i = b.part_in(controls, &owner, part, "Button", NodeKind::Leaf, json!({"flexShrink": 0}), props, "");
            b.nodes[i as usize].pressable = true;
        };
        nav(self, "previous", if page == 0 { if wraps { last } else { 0 } } else { page - 1 }, !wraps && page == 0);
        if owner.props.get("indicators").and_then(Value::as_bool).unwrap_or(true) {
            let row = self.part_in(controls, &owner, "indicator", "Box", NodeKind::Leaf, json!({"alignSelf": "center"}), json!({"count": count, "page": page}), "");
            // The `Carousel/indicator` recipe styles each DOT (the painter
            // applies it per dot); the part is the row of them.
            self.nodes[row as usize].part_query = None;
        }
        nav(self, "next", if page == last { if wraps { 0 } else { last } } else { page + 1 }, !wraps && page == last);
    }

    /// A List (round 2 §5): one axis (`direction: horizontal` windows on
    /// x), windowed past [`WINDOW_THRESHOLD`] items against its own, its
    /// nearest scrolling ancestor's or the host's viewport; `divided` puts a
    /// hairline centred in the gap; `sectionBy` groups consecutive template
    /// items under a `section` header (slot bound per section with the
    /// literal scope `{value, count, index}`), `stickyHeaders` pins the
    /// current one (rendered even outside the window).
    pub(crate) fn list(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let horizontal = str_prop(&owner.props, "direction") == Some("horizontal");
        let divided = bool_prop(&owner.props, "divided");
        let gap_name = str_prop(&owner.props, "gap").unwrap_or("none");
        let gap = (self.ctx.gap_px)(gap_name);
        let hairline = self.ctx.hairline;
        // The divider sits in the middle of the gap: item spacing = gap + hairline.
        let flex_gap = if divided { gap / 2.0 } else { gap };
        let gap_eff = if divided { gap + hairline } else { gap };
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!(if horizontal { "row" } else { "column" })), ("gap", json!(flex_gap))]);
        let section_by = str_prop(&owner.props, "sectionBy").map(str::to_string);
        let rows = match (&section_by, &node.template) {
            (Some(by), Some(t)) => {
                let base = crate::data::absolute_path(&t.path, scope);
                let items = crate::data::get_pointer(self.ctx.data, &base).and_then(Value::as_array).cloned().unwrap_or_default();
                let sections = crate::list::list_sections(&items, by);
                let mut rows: Vec<Row> = (0..node.children.len()).map(Row::Static).collect();
                rows.extend(crate::list::section_rows(&sections).into_iter().map(|r| match r {
                    crate::list::SectionRow::Header(h) => Row::Header(h),
                    crate::list::SectionRow::Item(i) => Row::Item(i),
                }));
                Some((rows, sections))
            }
            _ => None,
        };
        let Some((rows, sections)) = rows else {
            let keys = self.child_keys(node, scope);
            let windowed = keys.len() > WINDOW_THRESHOLD;
            let statics = node.children.len();
            let range = self.window(&owner.id, &keys, WindowAxis { gap: gap_eff, flex_gap, horizontal }, windowed, index);
            if let Some(spec) = self.lists.iter_mut().rev().find(|l| l.node == index) {
                // scrollToIndex: a template list's data index follows its
                // static children; a static list's index is the child's.
                spec.static_count = if node.template.is_some() { statics } else { 0 };
            }
            for i in range.start..range.end {
                if divided && i > range.start {
                    self.divider(index, &owner, horizontal, i);
                }
                let child = self.one_child(index, node, i, scope, false);
                // Every item, windowed or not, carries its place in the
                // WHOLE list for assistive tech (round 2 §5; children
                // first, then the template's items, like the reference).
                if let Some(c) = child {
                    self.default_a11y(c, "posInSet", json!(i + 1));
                    self.default_a11y(c, "setSize", json!(keys.len()));
                }
            }
            self.window_end(index);
            return;
        };
        // Sectioned: the flat row list (static children, then a header before
        // each section's items).
        let enclosing = self.enclosing_suffix(node);
        let item_keys = self.child_keys(node, scope);
        let statics = node.children.len();
        let keys: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Static(i) => item_keys[*i].clone(),
                Row::Header(h) => format!("{}.section.{h}", owner.id),
                Row::Item(i) => item_keys[statics + i].clone(),
            })
            .collect();
        let keys = std::sync::Arc::new(keys);
        let windowed = keys.len() > WINDOW_THRESHOLD;
        let sticky = bool_prop(&owner.props, "stickyHeaders");
        let headers: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Header(_))).map(|(p, _)| p).collect();
        let range = self.window(&owner.id, &keys, WindowAxis { gap: gap_eff, flex_gap, horizontal }, windowed, index);
        let data_index: Vec<Option<usize>> = rows
            .iter()
            .map(|r| match r {
                Row::Item(i) => Some(*i),
                _ => None,
            })
            .collect();
        let pinned = if sticky { self.pinned_header(&owner.id, &keys, &headers, gap_eff, horizontal) } else { None };
        if let Some(spec) = self.lists.iter_mut().rev().find(|l| l.node == index) {
            spec.data_index = Some(std::sync::Arc::new(data_index));
            spec.headers = std::sync::Arc::new(headers.clone());
            spec.sticky = sticky;
            spec.static_count = statics;
        }
        // The pinned header above the window is rendered too (absolute, at
        // its own offset; the pass pins it).
        if let Some(p) = pinned.filter(|p| *p < range.start) {
            if let Row::Header(h) = rows[p] {
                let at = self.ctx.local.lists.get_mut(&owner.id).map(|w| w.offsets_cached(&keys)[p]).unwrap_or(0.0);
                self.section_header(index, &owner, node, h, &sections[h], &enclosing, Some((at, horizontal)));
            }
        }
        for pos in range.start..range.end {
            // Divided: every boundary spans gap + hairline (the offsets'
            // model, = the reference's window). Between two items the
            // hairline is the divider (suffixed with the item's index, like
            // the reference's `divider.<item>`); next to a header it is a
            // blank spacer.
            if divided && pos > range.start {
                match (&rows[pos - 1], &rows[pos]) {
                    (Row::Static(_) | Row::Item(_), Row::Static(i)) => self.divider(index, &owner, horizontal, *i),
                    (Row::Static(_) | Row::Item(_), Row::Item(i)) => self.divider(index, &owner, horizontal, statics + i),
                    _ => self.spacer(index, &format!("gap.{pos}"), hairline, horizontal),
                }
            }
            // Headers are not items: an item's place counts the static
            // children, then the template's items (like the reference).
            let item = match rows[pos] {
                Row::Header(h) => {
                    self.section_header(index, &owner, node, h, &sections[h], &enclosing, None);
                    continue;
                }
                Row::Static(i) => i,
                Row::Item(i) => statics + i,
            };
            if let Some(c) = self.one_child(index, node, item, scope, false) {
                self.default_a11y(c, "posInSet", json!(item + 1));
                self.default_a11y(c, "setSize", json!(item_keys.len()));
            }
        }
        self.window_end(index);
    }

    fn divider(&mut self, index: u32, owner: &super::LNode, horizontal: bool, i: usize) {
        let h = self.ctx.hairline;
        let style = if horizontal { json!({"width": h, "alignSelf": "stretch", "flexShrink": 0}) } else { json!({"height": h, "alignSelf": "stretch", "flexShrink": 0}) };
        self.part_in(index, owner, "divider", "Box", NodeKind::Container, style, json!({}), &format!(".{i}"));
    }

    /// The row position of the header pinned at the list's scroll offset.
    fn pinned_header(&mut self, id: &str, keys: &std::sync::Arc<Vec<String>>, headers: &[usize], gap: f32, horizontal: bool) -> Option<usize> {
        let (scroll, _) = self.list_scroll(id, horizontal);
        let window = self.ctx.local.lists.get_mut(id)?;
        window.gap = gap;
        window.sticky_header_cached(keys, headers, scroll as f64).map(|(s, _)| s.row)
    }

    /// A section header: part `section` (`<list>.section.<i>`, a level-3
    /// heading) holding the `section` slot bound with `{value, count,
    /// index}`. `absolute` = a pinned header outside the window, placed at
    /// its own offset.
    #[allow(clippy::too_many_arguments)]
    fn section_header(&mut self, index: u32, owner: &super::LNode, node: &UiNode, h: usize, section: &crate::list::ListSection, enclosing: &str, absolute: Option<(f32, bool)>) {
        let mut style = json!({"display": "flex", "flexDirection": "column", "flexShrink": 0});
        if let Some((at, horizontal)) = absolute {
            style["position"] = json!("absolute");
            if horizontal {
                style["left"] = json!(at);
                style["top"] = json!(0);
                style["bottom"] = json!(0);
            } else {
                style["top"] = json!(at);
                style["left"] = json!(0);
                style["right"] = json!(0);
            }
        }
        let wrapper = self.part_in(index, owner, "section", "Box", NodeKind::Container, style, json!({"section": h, "value": section.value, "count": section.count, "key": format!("{}.section.{h}", owner.id)}), &format!(".{h}"));
        self.nodes[wrapper as usize].accessibility = Some(json!({"role": "heading", "level": 3}));
        let scope = format!("/$row/{}.section/{h}", owner.id);
        self.row_scopes.insert(scope.clone(), json!({"value": section.value, "count": section.count, "index": h}));
        self.nodes[wrapper as usize].scope = scope.clone();
        if let Some(slot) = node.slots.as_ref().and_then(|s| s.get("section")).cloned() {
            let item = self.instance(&slot, &format!("{enclosing}.{h}"), None);
            let layer = self.nodes[wrapper as usize].layer;
            self.add(&item, Some(wrapper), layer, &scope, false);
        }
    }

    /// The scroll offset and viewport a list windows against (round 2: its
    /// own, its nearest scrolling ancestor's or the host's).
    pub(crate) fn list_scroll(&self, id: &str, horizontal: bool) -> (f32, f32) {
        let local = &self.ctx.local;
        let axis = |p: (f32, f32)| if horizontal { p.0 } else { p.1 };
        let own = local.scroll.get(id).copied().map(axis).unwrap_or(0.0);
        match local.list_views.get(id) {
            Some(v) => {
                let scroll = match &v.source {
                    super::ListViewSource::Own => own,
                    super::ListViewSource::Ancestor { id, rel } => local.scroll.get(id).copied().map(axis).unwrap_or(0.0) - rel,
                    super::ListViewSource::Host { rel } => local.surface_scroll.map(|p| axis(p) - rel).unwrap_or(own),
                };
                (scroll.max(0.0), v.viewport)
            }
            None => (own, if horizontal { self.ctx.viewport_width } else { self.ctx.viewport_height }),
        }
    }

    /// The visible range of a (possibly windowed) row container `index`
    /// keyed `id`. A windowed one scrolls; a leading SPACER (part `spacer`,
    /// before the rows) stands in for the rows before the window; the
    /// caller adds the trailing one after the rows ([`Self::window_end`]).
    pub(crate) fn window(&mut self, id: &str, keys: &std::sync::Arc<Vec<String>>, axis: WindowAxis, windowed: bool, index: u32) -> VisibleRange {
        let count = keys.len();
        let (scroll, viewport) = self.list_scroll(id, axis.horizontal);
        let estimate = self.ctx.row_extent;
        let window = self.ctx.local.lists.entry(id.to_string()).or_insert_with(|| ListWindow::new(estimate, crate::list::DEFAULT_OVERSCAN, axis.gap));
        window.gap = axis.gap;
        window.estimated_item_height = estimate;
        window.scroll_offset = scroll;
        let offsets = window.offsets_cached(keys);
        let content_height = offsets.last().copied().unwrap_or(0.0);
        let range = if windowed { window.visible_range(&offsets, viewport.max(1.0)) } else { VisibleRange { start: 0, end: count } };
        if windowed {
            // A BOUNDED list scrolls itself; an unbounded one grows to its
            // content and windows against its scrolling ancestor or the host
            // viewport (round 2 §5).
            let base = &self.nodes[index as usize].base_style;
            let bounded = if axis.horizontal { ["width", "maxWidth"] } else { ["height", "maxHeight"] }.iter().any(|k| base.contains_key(*k)) || base.contains_key("flexGrow");
            let key = if axis.horizontal { "overflowX" } else { "overflowY" };
            if bounded {
                self.nodes[index as usize].base_style.entry(key.to_string()).or_insert(json!("scroll"));
            }
            if range.start > 0 {
                self.spacer(index, "spacerStart", (offsets[range.start] - axis.flex_gap).max(0.0), axis.horizontal);
            }
        }
        self.lists.push(ListSpec {
            node: index,
            id: id.to_string(),
            keys: keys.clone(),
            range,
            offsets,
            content_height,
            windowed,
            horizontal: axis.horizontal,
            gap: axis.gap,
            data_index: None,
            headers: Default::default(),
            sticky: false,
            static_count: 0,
        });
        self.flex_gaps.insert(index, axis.flex_gap);
        range
    }

    /// The trailing spacer of a windowed row container (the rows after).
    pub(crate) fn window_end(&mut self, index: u32) {
        let Some(spec) = self.lists.iter().rev().find(|l| l.node == index).cloned() else { return };
        let count = spec.keys.len();
        if spec.windowed && spec.range.end < count && spec.range.end > 0 {
            let flex_gap = self.flex_gaps.get(&index).copied().unwrap_or(spec.gap);
            let end_of_last = spec.offsets[spec.range.end] - spec.gap;
            self.spacer(index, "spacerEnd", (spec.content_height - end_of_last - flex_gap).max(0.0), spec.horizontal);
        } else if spec.windowed && spec.range.end == 0 && count > 0 {
            self.spacer(index, "spacerEnd", spec.content_height, spec.horizontal);
        }
    }

    fn spacer(&mut self, parent: u32, part: &str, extent: f32, horizontal: bool) {
        let owner = self.nodes[parent as usize].clone();
        let id = format!("{}.{part}", owner.id);
        let mut n = self.blank(&id, "Box", Some(parent), owner.layer, NodeKind::Container, &owner.scope);
        n.part = Some("spacer".into());
        n.owner = owner.owner.clone().or(Some(owner.id.clone()));
        n.owner_component = owner.owner_component.clone().or(Some(owner.component.clone()));
        n.base_style = super::obj(if horizontal { json!({"width": extent, "flexShrink": 0}) } else { json!({"height": extent, "flexShrink": 0}) });
        self.push(n);
    }
}

impl Builder<'_, '_> {
    /// Round 2 (§1): a Resizable — a flex row (column) of `panel` parts and
    /// `handle` parts between them. Panels grow by their percent (basis 0),
    /// so they share the group's main axis minus the handles exactly like
    /// `panelExtents`; a handle is `$control.hairline` thick in layout (the
    /// painter adds the `resizeHandleHit` hit area centred on it) and holds
    /// a `grip` with `handle: true`. A collapsed panel (0) is hidden.
    pub(crate) fn resizable(&mut self, index: u32, node: &UiNode, scope: &str) {
        let props = self.nodes[index as usize].props.clone();
        let vertical = self.responsive_prop(&props, "direction").as_ref().and_then(Value::as_str) == Some("vertical");
        let direction = if vertical { "vertical" } else { "horizontal" };
        self.nodes[index as usize].props.insert("direction".into(), json!(direction));
        if let Some(q) = &mut self.nodes[index as usize].own_query {
            q.props.insert("direction".into(), json!(direction));
        }
        let owner = self.nodes[index as usize].clone();
        // A group is block-level: it fills its container's width.
        self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!(if vertical { "column" } else { "row" })), ("alignItems", json!("stretch")), ("width", json!("100%")), ("minWidth", json!(0))]);
        let count = self.child_keys(node, scope).len();
        let limits = crate::resizable::PanelLimits::list(owner.props.get("panels"));
        let sizes = match self.ctx.local.sizes.get(&owner.id).filter(|s| s.len() == count) {
            Some(s) => s.clone(),
            None => crate::resizable::normalize_sizes(owner.props.get("sizes").and_then(Value::as_array).map(Vec::as_slice), count, &limits),
        };
        self.nodes[index as usize].props.insert("sizes".into(), json!(sizes));
        let grip = bool_prop(&owner.props, "handle");
        let hairline = self.ctx.hairline;
        let label = self.string("resize");
        for (i, size) in sizes.iter().enumerate() {
            if i > 0 {
                let h = i - 1;
                let mut style = json!({"flexShrink": 0, "alignSelf": "stretch", "display": "flex", "alignItems": "center", "justifyContent": "center"});
                style[if vertical { "height" } else { "width" }] = json!(hairline);
                let lim_min = limits.get(h).and_then(|l| l.min).unwrap_or(crate::resizable::PANEL_MIN);
                let lim_max = limits.get(h).and_then(|l| l.max).unwrap_or(100.0);
                // The handle's LINE runs across the axis.
                let orientation = if vertical { "horizontal" } else { "vertical" };
                let controls = format!("{}.panel.{h}", owner.id);
                let handle = self.part_in(index, &owner, "handle", "Box", NodeKind::Container, style, json!({"handle": h, "orientation": orientation, "valueNow": sizes[h], "valueMin": lim_min, "valueMax": lim_max, "controls": controls, "label": label, "hit": crate::resizable::RESIZE_HANDLE_HIT}), &format!(".{h}"));
                self.nodes[handle as usize].pressable = true;
                self.nodes[handle as usize].accessibility = Some(json!({"role": "separator", "label": label, "orientation": orientation, "valueNow": sizes[h], "valueMin": lim_min, "valueMax": lim_max, "controls": controls}));
                if grip {
                    self.part_in(handle, &owner, "grip", "Box", NodeKind::Container, json!({"flexShrink": 0}), json!({}), &format!(".{h}"));
                }
            }
            let style = json!({"display": "flex", "flexDirection": "column", "flexGrow": size, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "minHeight": 0, "overflow": "hidden"});
            let panel = self.part_in(index, &owner, "panel", "Box", NodeKind::Container, style, json!({"index": i, "size": size}), &format!(".{i}"));
            let collapsed = *size <= 0.0;
            self.nodes[panel as usize].hidden |= collapsed;
            self.one_child(panel, node, i, scope, collapsed);
        }
    }
}

/// One row of a sectioned List.
#[derive(Debug, Clone, Copy)]
enum Row {
    Static(usize),
    Header(usize),
    Item(usize),
}

/// The axis and gaps a window lays out on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowAxis {
    /// The spacing the offsets use (gap + hairline when divided).
    pub gap: f32,
    /// The flex `gap` between children.
    pub flex_gap: f32,
    pub horizontal: bool,
}

pub(crate) use WindowAxis as Axis;
