//! The LAYOUT tree: the normalized `UiNode` tree (macros already expanded)
//! becomes a flat pre-order list of layout nodes taffy and the painters work
//! on. Here the core adds what painters must not invent: data bindings are
//! resolved, template children are instantiated per item, natives with
//! structure of their own (Tabs, Accordion, Carousel, List, form controls)
//! get synthetic PART nodes (ids `<owner>.<part>`, recipes keyed on the
//! owner's component so a theme styles them), overlay natives (Dialog,
//! Drawer, Popover, Tooltip, DropdownMenu) put their content into LAYERS with
//! their own roots, and extension natives arrive as `Extension` leaves.
//!
//! Painters therefore see only: `Box` containers, measured leaf natives,
//! synthetic parts (painted by the owner's chrome rules) and extension leaves.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::catalog::{CatalogView, UNKNOWN_COMPONENT};
use crate::data::{resolve_value, ResolveContext};
use crate::list::{ListWindow, VisibleRange, WINDOW_THRESHOLD};
use crate::overlay::{OverlayAlign, OverlaySide};
use crate::recipes::RecipeIndex;
use crate::theme::RecipeQuery;
use crate::types::{Props, UiNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Container,
    Leaf,
}

/// One layout node. `style` keeps tokens RESOLVED and conditions NESTED; the
/// surface flattens conditions per pass and re-resolves on a theme change.
#[derive(Debug, Clone, PartialEq)]
pub struct LNode {
    pub index: u32,
    pub id: String,
    /// The native kind painted (`Box`, `Text`, `Button`, …, `Extension`).
    pub component: String,
    /// The synthetic part of a native (`Tabs/tab`, `Input/label`).
    pub part: Option<String>,
    /// The id of the native that owns the part.
    pub owner: Option<String>,
    pub owner_component: Option<String>,
    /// For `Extension` nodes: the extension's catalog id and native kind.
    pub catalog_id: Option<String>,
    pub extension_kind: Option<String>,
    pub depth: u32,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub layer: u32,
    pub kind: NodeKind,
    /// Props with bindings and calls resolved.
    pub props: Props,
    /// The structural style (author's + template's), token refs NOT resolved.
    pub base_style: Props,
    /// The native's own root recipe (Text/root for its variant).
    pub own_query: Option<RecipeQuery>,
    /// The macro or native PART recipe (Badge/label, Tabs/tab).
    pub part_query: Option<RecipeQuery>,
    pub hidden: bool,
    pub lines: Option<u32>,
    pub pressable: bool,
    /// The data scope (template item pointer) bindings resolved against.
    pub scope: String,
    /// The `on` map of the source node (events route through the owner).
    pub on: Option<indexmap::IndexMap<String, Value>>,
    pub accessibility: Option<Value>,
    /// The overlay node this node opens (a trigger slot's root).
    pub trigger_for: Option<String>,
}

/// How an overlay layer is placed.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerPlacement {
    /// Centred in the viewport (Dialog).
    Centered,
    /// Pinned to one viewport edge (Drawer).
    Edge(OverlaySide),
    /// Against the anchor node's frame (Popover, Tooltip, DropdownMenu).
    Anchored { anchor: u32, side: OverlaySide, align: OverlayAlign },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerSpec {
    pub layer: u32,
    pub root: u32,
    pub kind: String,
    pub owner: String,
    pub placement: LayerPlacement,
}

/// A windowed list's state for one build.
#[derive(Debug, Clone, PartialEq)]
pub struct ListSpec {
    pub node: u32,
    pub id: String,
    pub keys: Vec<String>,
    pub range: VisibleRange,
    pub offsets: Vec<f32>,
    pub content_height: f32,
    pub windowed: bool,
}

/// Local UI state the core owns (never sent to the producer unless bound).
#[derive(Debug, Clone, Default)]
pub struct LocalState {
    pub tabs: std::collections::HashMap<String, String>,
    pub accordion: std::collections::HashMap<String, Vec<String>>,
    pub carousel: std::collections::HashMap<String, i64>,
    pub open: std::collections::HashMap<String, bool>,
    pub pressed_toggles: std::collections::HashMap<String, bool>,
    pub lists: std::collections::HashMap<String, ListWindow>,
}

pub struct BuildContext<'a> {
    pub view: &'a CatalogView,
    pub recipes: &'a RecipeIndex,
    pub data: &'a Value,
    pub local: &'a mut LocalState,
    /// Reduce a template component by id into a normalized subtree.
    pub template: &'a dyn Fn(&str) -> Option<UiNode>,
    pub viewport_height: f32,
    /// The theme's default gap for `List` items per `gap` enum, in px.
    pub gap_px: &'a dyn Fn(&str) -> f32,
    /// Expand form controls into label/field/description parts (a themed
    /// surface); off = geometry mode, every native is ONE measured leaf.
    pub expand_controls: bool,
}

pub struct Built {
    pub nodes: Vec<LNode>,
    pub layers: Vec<LayerSpec>,
    pub lists: Vec<ListSpec>,
}

struct Builder<'a, 'b> {
    ctx: &'b mut BuildContext<'a>,
    nodes: Vec<LNode>,
    layers: Vec<LayerSpec>,
    lists: Vec<ListSpec>,
}

fn obj(v: Value) -> Props {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn with_suffix(node: &UiNode, suffix: &str) -> UiNode {
    let mut out = node.clone();
    out.id = format!("{}{suffix}", node.id);
    out.children = node.children.iter().map(|c| with_suffix(c, suffix)).collect();
    if let Some(slots) = &node.slots {
        out.slots = Some(slots.iter().map(|(k, v)| (k.clone(), with_suffix(v, suffix))).collect());
    }
    out
}

const OVERLAYS: &[&str] = &["Dialog", "Drawer", "Popover", "Tooltip", "DropdownMenu"];

impl<'a, 'b> Builder<'a, 'b> {
    fn push(&mut self, mut node: LNode) -> u32 {
        let index = self.nodes.len() as u32;
        node.index = index;
        if let Some(p) = node.parent {
            self.nodes[p as usize].children.push(index);
        }
        self.nodes.push(node);
        index
    }

    fn blank(&self, id: &str, component: &str, parent: Option<u32>, layer: u32, kind: NodeKind, scope: &str) -> LNode {
        let (depth, hidden) = parent.map(|p| (self.nodes[p as usize].depth + 1, self.nodes[p as usize].hidden)).unwrap_or((0, false));
        LNode {
            index: 0,
            id: id.to_string(),
            component: component.to_string(),
            part: None,
            owner: None,
            owner_component: None,
            catalog_id: None,
            extension_kind: None,
            depth,
            parent,
            children: Vec::new(),
            layer,
            kind,
            props: Map::new(),
            base_style: Map::new(),
            own_query: None,
            part_query: None,
            hidden,
            lines: None,
            pressable: false,
            scope: scope.to_string(),
            on: None,
            accessibility: None,
            trigger_for: None,
        }
    }

    /// A synthetic part of `owner`: a container or a leaf with the owner's
    /// recipe for that part.
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, owner: &LNode, part: &str, component: &str, kind: NodeKind, style: Value, props: Value, extra_states: &[&str]) -> u32 {
        let id = format!("{}.{part}", owner.id);
        let mut n = self.blank(&id, component, Some(owner.index), owner.layer, kind, &owner.scope);
        n.part = Some(part.to_string());
        n.owner = Some(owner.id.clone());
        n.owner_component = Some(owner.component.clone());
        n.base_style = obj(style);
        n.props = obj(props);
        let owner_component = owner.component.clone();
        let mut query_props = self.ctx.recipes.native_recipe_props(&owner_component, &owner.props);
        for s in extra_states {
            query_props.insert((*s).to_string(), Value::Bool(true));
        }
        n.part_query = Some(RecipeQuery::new(owner_component, part, query_props, Vec::new()));
        if let Some(l) = n.props.get("lines").and_then(Value::as_u64) {
            n.lines = Some(l as u32);
        }
        self.push(n)
    }

    #[allow(clippy::too_many_arguments)]
    fn part_in(&mut self, parent: u32, owner: &LNode, part: &str, component: &str, kind: NodeKind, style: Value, props: Value, id_suffix: &str) -> u32 {
        let id = format!("{}.{part}{id_suffix}", owner.id);
        // A part lives in its PARENT's layer: a Dialog's title/body/close sit
        // under the layer root, not in the owner's (main) layer (VAPP-91).
        let layer = self.nodes.get(parent as usize).map_or(owner.layer, |p| p.layer);
        let mut n = self.blank(&id, component, Some(parent), layer, kind, &owner.scope);
        n.part = Some(part.to_string());
        n.owner = Some(owner.id.clone());
        n.owner_component = Some(owner.component.clone());
        n.base_style = obj(style);
        n.props = obj(props);
        let query_props = self.ctx.recipes.native_recipe_props(&owner.component, &owner.props);
        n.part_query = Some(RecipeQuery::new(owner.component.clone(), part, query_props, Vec::new()));
        if let Some(l) = n.props.get("lines").and_then(Value::as_u64) {
            n.lines = Some(l as u32);
        }
        self.push(n)
    }

    fn text_part(&mut self, parent: u32, owner: &LNode, part: &str, text: &str, variant: &str, suffix: &str) -> u32 {
        self.part_in(parent, owner, part, "Text", NodeKind::Leaf, json!({}), json!({"text": text, "variant": variant}), suffix)
    }

    fn resolved_props(&self, node: &UiNode, scope: &str) -> Props {
        let ctx = ResolveContext { data: self.ctx.data, scope };
        obj(resolve_value(&Value::Object(node.props.clone()), &ctx).unwrap_or(Value::Null))
    }

    fn is_overlay(component: &str) -> bool {
        OVERLAYS.contains(&component)
    }

    /// Add `node` under `parent`; returns its index.
    fn add(&mut self, node: &UiNode, parent: Option<u32>, layer: u32, scope: &str, force_hidden: bool) -> u32 {
        let view = self.ctx.view;
        let def = view.component(&node.component);
        let props = self.resolved_props(node, scope);
        let ext = view.extension_of(&node.component).map(str::to_string);
        let children_rule = def.map(|d| d.children.as_str()).unwrap_or("none");
        let has_children = !node.children.is_empty() || node.template.is_some();
        let component = if ext.is_some() { "Extension".to_string() } else { node.component.clone() };
        let kind = if ext.is_some() {
            if has_children { NodeKind::Container } else { NodeKind::Leaf }
        } else {
            match (node.component.as_str(), children_rule) {
                ("Box", _) => NodeKind::Container,
                (_, "none") => NodeKind::Leaf,
                ("Button", "one") => if has_children { NodeKind::Container } else { NodeKind::Leaf },
                _ => NodeKind::Container,
            }
        };
        let mut n = self.blank(&node.id, &component, parent, layer, kind, scope);
        n.hidden |= force_hidden;
        if let Some(e) = ext {
            n.catalog_id = Some(e);
            n.extension_kind = Some(node.component.clone());
        }
        // The author's `style` (top level) wins over a `style` PROP.
        let mut base: Props = props.get("style").and_then(Value::as_object).cloned().unwrap_or_default();
        if let Some(s) = &node.style {
            for (k, v) in s {
                base.insert(k.clone(), v.clone());
            }
        }
        n.base_style = base;
        n.lines = props.get("lines").and_then(Value::as_u64).map(|l| l as u32);
        n.pressable = node.on.as_ref().is_some_and(|o| o.contains_key("press")) || props.get("pressable").and_then(Value::as_bool).unwrap_or(false);
        n.on = node.on.clone();
        n.accessibility = node.accessibility.clone();
        n.own_query = Some(RecipeQuery::new(
            node.component.clone(),
            "root",
            self.ctx.recipes.native_recipe_props(&node.component, &props),
            Vec::new(),
        ));
        if let Some(r) = &node.recipe {
            n.part_query = Some(RecipeQuery::new(r.macro_.clone(), r.part.clone(), r.props.clone(), Vec::new()));
        }
        n.props = props;
        let index = self.push(n);

        // Slots first (pre-order like the reference `preorder`), then the
        // component-specific structure.
        match node.component.as_str() {
            "Tabs" => self.tabs(index, node, scope),
            "Accordion" => self.accordion(index, node, scope),
            "Carousel" => self.carousel(index, node, scope),
            "List" => self.list(index, node, scope),
            c if Self::is_overlay(c) => self.overlay(index, node, scope),
            "Input" | "Textarea" | "Select" | "DatePicker" if self.ctx.expand_controls => self.field(index, node),
            "Checkbox" | "Switch" if self.ctx.expand_controls => self.check(index, node),
            "Radio" if self.ctx.expand_controls => self.radio(index, node),
            "Slider" if self.ctx.expand_controls => self.slider(index, node),
            _ => {
                if let Some(slots) = &node.slots {
                    for slot in slots.values() {
                        self.add(slot, Some(index), layer, scope, false);
                    }
                }
                self.children_of(index, node, scope, false);
            }
        }
        index
    }

    /// The node's children (static or from its template) under `parent`.
    fn children_of(&mut self, parent: u32, node: &UiNode, scope: &str, hidden: bool) -> Vec<u32> {
        let mut out = Vec::new();
        for child in &node.children {
            out.push(self.add(child, Some(parent), self.nodes[parent as usize].layer, scope, hidden));
        }
        if let Some(t) = &node.template {
            let items = crate::data::get_pointer(self.ctx.data, &crate::data::absolute_path(&t.path, scope)).and_then(Value::as_array).cloned().unwrap_or_default();
            if let Some(tpl) = (self.ctx.template)(&t.component) {
                let base = crate::data::absolute_path(&t.path, scope);
                for (i, _) in items.iter().enumerate() {
                    let item = with_suffix(&tpl, &format!(".{i}"));
                    let item_scope = format!("{base}/{i}");
                    out.push(self.add(&item, Some(parent), self.nodes[parent as usize].layer, &item_scope, hidden));
                }
            }
        }
        out
    }

    /// The keys and count of a node's children without building them.
    fn child_keys(&self, node: &UiNode, scope: &str) -> Vec<String> {
        let mut keys: Vec<String> = node.children.iter().map(|c| c.id.clone()).collect();
        if let Some(t) = &node.template {
            let n = crate::data::get_pointer(self.ctx.data, &crate::data::absolute_path(&t.path, scope)).and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
            for i in 0..n {
                keys.push(format!("{}.{i}", t.component));
            }
        }
        keys
    }

    fn one_child(&mut self, parent: u32, node: &UiNode, i: usize, scope: &str, hidden: bool) -> Option<u32> {
        if let Some(child) = node.children.get(i) {
            return Some(self.add(child, Some(parent), self.nodes[parent as usize].layer, scope, hidden));
        }
        let t = node.template.as_ref()?;
        let base = crate::data::absolute_path(&t.path, scope);
        let items = crate::data::get_pointer(self.ctx.data, &base).and_then(Value::as_array)?;
        let idx = i.checked_sub(node.children.len())?;
        if idx >= items.len() {
            return None;
        }
        let tpl = (self.ctx.template)(&t.component)?;
        let item = with_suffix(&tpl, &format!(".{idx}"));
        Some(self.add(&item, Some(parent), self.nodes[parent as usize].layer, &format!("{base}/{idx}"), hidden))
    }

    fn tabs(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let tabs = owner.props.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
        let values: Vec<String> = tabs.iter().map(|t| t.get("value").map(crate::json::to_js_string).unwrap_or_default()).collect();
        let active = self.ctx.local.tabs.get(&owner.id).cloned()
            .or_else(|| owner.props.get("value").map(crate::json::to_js_string))
            .filter(|v| values.contains(v))
            .or_else(|| values.first().cloned())
            .unwrap_or_default();
        self.nodes[index as usize].base_style.entry("display".to_string()).or_insert(json!("flex"));
        self.nodes[index as usize].base_style.entry("flexDirection".to_string()).or_insert(json!("column"));
        let fill = owner.props.get("fill").and_then(Value::as_bool).unwrap_or(false);
        let list = self.part(&owner, "list", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "stretch", "alignSelf": if fill { "stretch" } else { "flex-start" }}), json!({}), &[]);
        for (i, tab) in tabs.iter().enumerate() {
            let label = tab.get("label").map(crate::json::to_js_string).unwrap_or_default();
            let mut props = json!({"text": label, "lines": 1});
            if let Some(icon) = tab.get("icon") {
                props["icon"] = icon.clone();
            }
            if let Some(count) = tab.get("count") {
                props["count"] = count.clone();
            }
            let selected = values.get(i) == Some(&active);
            let mut style = json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center"});
            if fill {
                style["flexGrow"] = json!(1);
                style["flexBasis"] = json!(0);
            }
            let t = self.part_in(list, &owner, "tab", "Text", NodeKind::Leaf, style, props, &format!(".{i}"));
            if let Some(q) = &mut self.nodes[t as usize].part_query {
                q.props.insert("selected".into(), Value::Bool(selected));
                if selected {
                    q.states.push("selected".into());
                }
            }
            self.nodes[t as usize].pressable = true;
        }
        let content = self.part(&owner, "content", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column"}), json!({}), &[]);
        let count = self.child_keys(node, scope).len();
        for i in 0..count {
            let hidden = values.get(i) != Some(&active);
            self.one_child(content, node, i, scope, hidden);
        }
    }

    fn accordion(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let items = owner.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let multiple = owner.props.get("type").and_then(Value::as_str) == Some("multiple");
        let open: Vec<String> = match self.ctx.local.accordion.get(&owner.id) {
            Some(v) => v.clone(),
            None => owner.props.get("value").map(crate::json::to_js_string).map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
        };
        let open: Vec<String> = if multiple { open } else { open.into_iter().take(1).collect() };
        self.nodes[index as usize].base_style.entry("display".to_string()).or_insert(json!("flex"));
        self.nodes[index as usize].base_style.entry("flexDirection".to_string()).or_insert(json!("column"));
        for (i, item) in items.iter().enumerate() {
            let value = item.get("value").map(crate::json::to_js_string).unwrap_or_default();
            let is_open = open.contains(&value);
            let wrapper = self.part_in(index, &owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column"}), json!({}), &format!(".{i}"));
            let title = item.get("title").map(crate::json::to_js_string).unwrap_or_default();
            let mut props = json!({"text": title, "lines": 1});
            if let Some(count) = item.get("count") {
                props["count"] = count.clone();
            }
            let trigger = self.part_in(wrapper, &owner, "trigger", "Text", NodeKind::Leaf, json!({"display": "flex", "flexDirection": "row", "alignItems": "center"}), props, &format!(".{i}"));
            self.nodes[trigger as usize].pressable = true;
            if let Some(q) = &mut self.nodes[trigger as usize].part_query {
                q.props.insert("open".into(), Value::Bool(is_open));
                if is_open {
                    q.states.push("open".into());
                }
            }
            let content = self.part_in(wrapper, &owner, "content", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column"}), json!({}), &format!(".{i}"));
            self.nodes[content as usize].hidden = !is_open || self.nodes[content as usize].hidden;
            self.one_child(content, node, i, scope, !is_open);
        }
    }

    fn carousel(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let count = self.child_keys(node, scope).len();
        let page = self.ctx.local.carousel.get(&owner.id).copied().or_else(|| owner.props.get("page").and_then(Value::as_i64)).unwrap_or(0);
        let page = if count == 0 { 0 } else { page.rem_euclid(count as i64) as usize };
        self.nodes[index as usize].base_style.entry("display".to_string()).or_insert(json!("flex"));
        self.nodes[index as usize].base_style.entry("flexDirection".to_string()).or_insert(json!("column"));
        let pages = self.part(&owner, "page", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "overflow": "hidden"}), json!({}), &[]);
        for i in 0..count {
            self.one_child(pages, node, i, scope, i != page);
        }
        if owner.props.get("indicators").and_then(Value::as_bool).unwrap_or(true) && count > 1 {
            self.part(&owner, "indicator", "Box", NodeKind::Leaf, json!({"alignSelf": "center"}), json!({"count": count, "page": page}), &[]);
        }
    }

    fn list(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let horizontal = owner.props.get("direction").and_then(Value::as_str) == Some("horizontal");
        let divided = owner.props.get("divided").and_then(Value::as_bool).unwrap_or(false);
        let gap_name = owner.props.get("gap").and_then(Value::as_str).unwrap_or("none");
        let gap = (self.ctx.gap_px)(gap_name);
        {
            let s = &mut self.nodes[index as usize].base_style;
            s.entry("display".to_string()).or_insert(json!("flex"));
            s.entry("flexDirection".to_string()).or_insert(json!(if horizontal { "row" } else { "column" }));
            s.entry("gap".to_string()).or_insert(json!(gap));
        }
        let keys = self.child_keys(node, scope);
        let count = keys.len();
        let windowed = !horizontal && count > WINDOW_THRESHOLD;
        let window = self.ctx.local.lists.entry(owner.id.clone()).or_insert_with(|| ListWindow::new(crate::list::DEFAULT_ESTIMATED_ITEM_HEIGHT, crate::list::DEFAULT_OVERSCAN, gap));
        window.gap = gap;
        let offsets = window.offsets(keys.iter().map(String::as_str));
        let content_height = offsets.last().copied().unwrap_or(0.0);
        let range = if windowed { window.visible_range(&offsets, self.ctx.viewport_height.max(1.0)) } else { VisibleRange { start: 0, end: count } };
        if windowed {
            let s = &mut self.nodes[index as usize].base_style;
            s.insert("paddingTop".into(), json!(offsets[range.start]));
            let tail = content_height - offsets[range.end.min(count)];
            s.insert("paddingBottom".into(), json!(tail.max(0.0)));
            s.entry("overflow".to_string()).or_insert(json!("scroll"));
        }
        for i in range.start..range.end {
            if divided && i > range.start {
                let style = if horizontal { json!({"width": 1, "alignSelf": "stretch", "flexShrink": 0}) } else { json!({"height": 1, "alignSelf": "stretch", "flexShrink": 0}) };
                self.part_in(index, &owner, "divider", "Box", NodeKind::Container, style, json!({}), &format!(".{i}"));
            }
            self.one_child(index, node, i, scope, false);
        }
        self.lists.push(ListSpec { node: index, id: owner.id.clone(), keys, range, offsets, content_height, windowed });
    }

    fn overlay(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let layer_parent = owner.layer;
        // The trigger stays inline (the overlay node itself is a transparent
        // container around it); DropdownMenu without a trigger slot gets a
        // default button.
        {
            let s = &mut self.nodes[index as usize].base_style;
            s.entry("display".to_string()).or_insert(json!("flex"));
            s.entry("flexDirection".to_string()).or_insert(json!("row"));
            s.entry("alignSelf".to_string()).or_insert(json!("flex-start"));
        }
        let mut anchor: Option<u32> = None;
        if let Some(trigger) = node.slots.as_ref().and_then(|s| s.get("trigger")) {
            anchor = Some(self.add(trigger, Some(index), layer_parent, scope, false));
        } else if owner.component == "DropdownMenu" {
            let mut props = json!({"label": owner.props.get("label").cloned().unwrap_or(Value::String("Menu".into())), "variant": "outline"});
            if let Some(icon) = owner.props.get("icon") {
                props["icon"] = icon.clone();
                if owner.props.get("label").is_none() {
                    props["size"] = json!("icon");
                }
            }
            let t = self.part(&owner, "trigger", "Button", NodeKind::Leaf, json!({}), props, &[]);
            self.nodes[t as usize].pressable = true;
            anchor = Some(t);
        } else if owner.component == "Tooltip" {
            // A tooltip wraps its children as the anchor.
            let wrap = self.part(&owner, "anchor", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row"}), json!({}), &[]);
            self.children_of(wrap, node, scope, false);
            anchor = Some(wrap);
            self.nodes[anchor.unwrap() as usize].part = Some("anchor".into());
        }
        if let Some(a) = anchor {
            self.nodes[a as usize].pressable |= owner.component != "Tooltip";
            self.nodes[a as usize].trigger_for = Some(owner.id.clone());
        }
        let open = self.ctx.local.open.get(&owner.id).copied().unwrap_or_else(|| owner.props.get("open").and_then(Value::as_bool).unwrap_or(false));
        if !open {
            return;
        }
        let layer = self.layers.len() as u32 + 1;
        let side = owner.props.get("side").and_then(Value::as_str).and_then(OverlaySide::parse);
        let placement = match owner.component.as_str() {
            "Dialog" => LayerPlacement::Centered,
            "Drawer" => LayerPlacement::Edge(side.unwrap_or(OverlaySide::Right)),
            "Tooltip" => LayerPlacement::Anchored { anchor: anchor.unwrap_or(index), side: side.unwrap_or(OverlaySide::Top), align: OverlayAlign::Center },
            "DropdownMenu" => LayerPlacement::Anchored { anchor: anchor.unwrap_or(index), side: OverlaySide::Bottom, align: OverlayAlign::Start },
            _ => LayerPlacement::Anchored { anchor: anchor.unwrap_or(index), side: side.unwrap_or(OverlaySide::Bottom), align: OverlayAlign::Center },
        };
        let content_style = match owner.component.as_str() {
            "Dialog" => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.lg", "borderRadius": "$radius.xl", "width": "100%"}),
            "Drawer" => match placement {
                LayerPlacement::Edge(OverlaySide::Top | OverlaySide::Bottom) => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.lg", "width": "100%"}),
                _ => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.lg", "height": "100%"}),
            },
            "Tooltip" => json!({"display": "flex", "flexDirection": "row", "paddingHorizontal": "$spacing.sm", "paddingVertical": "$spacing.xs", "borderRadius": "$radius.md"}),
            "DropdownMenu" => json!({"display": "flex", "flexDirection": "column", "padding": "$spacing.xs", "borderRadius": "$radius.md", "minWidth": 160}),
            _ => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.md", "borderRadius": "$radius.md"}),
        };
        let mut root = self.blank(&format!("{}.content", owner.id), "Box", None, layer, NodeKind::Container, scope);
        root.part = Some("content".into());
        root.owner = Some(owner.id.clone());
        root.owner_component = Some(owner.component.clone());
        root.base_style = obj(content_style);
        root.part_query = Some(RecipeQuery::new(owner.component.clone(), "content", self.ctx.recipes.native_recipe_props(&owner.component, &owner.props), vec!["open".into()]));
        let root = self.push(root);
        let owner_now = self.nodes[index as usize].clone();
        match owner.component.as_str() {
            "Dialog" | "Drawer" => {
                if owner.component == "Drawer" && matches!(placement, LayerPlacement::Edge(OverlaySide::Bottom)) {
                    self.part_in(root, &owner_now, "handle", "Box", NodeKind::Container, json!({"width": 40, "height": 4, "borderRadius": "$radius.full", "alignSelf": "center"}), json!({}), "");
                }
                if let Some(t) = owner.props.get("title").and_then(Value::as_str) {
                    self.text_part(root, &owner_now, "title", t, "title", "");
                }
                if let Some(d) = owner.props.get("description").and_then(Value::as_str) {
                    self.text_part(root, &owner_now, "description", d, "muted", "");
                }
                let body = self.part_in(root, &owner_now, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md"}), json!({}), "");
                self.children_of(body, node, scope, false);
                if let Some(footer) = node.slots.as_ref().and_then(|s| s.get("footer")) {
                    let f = self.part_in(root, &owner_now, "footer", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "gap": "$spacing.sm"}), json!({}), "");
                    self.add(footer, Some(f), layer, scope, false);
                }
                if owner.component == "Dialog" && owner.props.get("dismissible").and_then(Value::as_bool).unwrap_or(true) {
                    let close = self.part_in(root, &owner_now, "close", "Button", NodeKind::Leaf, json!({"position": "absolute", "top": "$spacing.sm", "insetInlineEnd": "$spacing.sm"}), json!({"icon": "ui-close", "variant": "ghost", "size": "icon", "label": "Close"}), "");
                    self.nodes[close as usize].pressable = true;
                }
            }
            "Tooltip" => {
                let text = owner.props.get("content").map(crate::json::to_js_string).unwrap_or_default();
                self.text_part(root, &owner_now, "label", &text, "caption", "");
            }
            "DropdownMenu" => {
                let items = owner.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
                for (i, item) in items.iter().enumerate() {
                    if item.get("separator").and_then(Value::as_bool) == Some(true) {
                        self.part_in(root, &owner_now, "separator", "Box", NodeKind::Container, json!({"height": 1, "marginVertical": "$spacing.xs"}), json!({}), &format!(".{i}"));
                        continue;
                    }
                    let mut props = json!({"text": item.get("label").map(crate::json::to_js_string).unwrap_or_default(), "lines": 1, "value": item.get("value").cloned().unwrap_or(Value::Null)});
                    if let Some(icon) = item.get("icon") {
                        props["icon"] = icon.clone();
                    }
                    if item.get("destructive").and_then(Value::as_bool) == Some(true) {
                        props["destructive"] = json!(true);
                    }
                    let row = self.part_in(root, &owner_now, "item", "Text", NodeKind::Leaf, json!({"display": "flex", "flexDirection": "row", "alignItems": "center"}), props, &format!(".{i}"));
                    self.nodes[row as usize].pressable = true;
                }
            }
            _ => {
                self.children_of(root, node, scope, false);
            }
        }
        self.layers.push(LayerSpec { layer, root, kind: owner.component.clone(), owner: owner.id.clone(), placement });
    }

    /// Input / Textarea / Select / DatePicker: label, the host-measured field,
    /// description and error lines.
    fn field(&mut self, index: u32, node: &UiNode) {
        let owner = self.nodes[index as usize].clone();
        {
            let s = &mut self.nodes[index as usize].base_style;
            s.entry("display".to_string()).or_insert(json!("flex"));
            s.entry("flexDirection".to_string()).or_insert(json!("column"));
            s.entry("gap".to_string()).or_insert(json!("$spacing.xs"));
        }
        self.nodes[index as usize].kind = NodeKind::Container;
        if let Some(label) = owner.props.get("label").and_then(Value::as_str) {
            self.text_part(index, &owner, "label", label, "label", "");
        }
        let mut field_props = owner.props.clone();
        field_props.remove("label");
        field_props.remove("description");
        field_props.remove("style");
        let f = self.part_in(index, &owner, "field", &owner.component, NodeKind::Leaf, json!({"alignSelf": "stretch"}), Value::Object(field_props), "");
        self.nodes[f as usize].pressable = true;
        if let Some(d) = owner.props.get("description").and_then(Value::as_str) {
            self.text_part(index, &owner, "description", d, "muted", "");
        }
        if let Some(e) = owner.props.get("error").and_then(Value::as_str) {
            self.text_part(index, &owner, "error", e, "caption", "");
        }
        let _ = node;
    }

    /// Checkbox / Switch: the control box beside label + description.
    fn check(&mut self, index: u32, node: &UiNode) {
        let owner = self.nodes[index as usize].clone();
        {
            let s = &mut self.nodes[index as usize].base_style;
            s.entry("display".to_string()).or_insert(json!("flex"));
            s.entry("flexDirection".to_string()).or_insert(json!("row"));
            s.entry("alignItems".to_string()).or_insert(json!("center"));
            s.entry("gap".to_string()).or_insert(json!("$spacing.sm"));
        }
        self.nodes[index as usize].kind = NodeKind::Container;
        self.nodes[index as usize].pressable = true;
        let control_part = if owner.component == "Switch" { "track" } else { "box" };
        let checked = owner.props.get("checked").and_then(Value::as_bool).unwrap_or(false);
        let control = |b: &mut Self, parent: u32| {
            let c = b.part_in(parent, &owner, control_part, &owner.component, NodeKind::Leaf, json!({"flexShrink": 0}), json!({"checked": checked, "disabled": owner.props.get("disabled").and_then(Value::as_bool).unwrap_or(false)}), "");
            if let Some(q) = &mut b.nodes[c as usize].part_query {
                if checked {
                    q.states.push("checked".into());
                }
            }
            c
        };
        let text = |b: &mut Self, parent: u32| {
            let has_desc = owner.props.get("description").and_then(Value::as_str).is_some();
            if has_desc {
                let col = b.part_in(parent, &owner, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.xxs", "flexGrow": 1, "minWidth": 0}), json!({}), "");
                if let Some(label) = owner.props.get("label").and_then(Value::as_str) {
                    b.text_part(col, &owner, "label", label, "body", "");
                }
                if let Some(d) = owner.props.get("description").and_then(Value::as_str) {
                    b.text_part(col, &owner, "description", d, "muted", "");
                }
            } else if let Some(label) = owner.props.get("label").and_then(Value::as_str) {
                let l = b.text_part(parent, &owner, "label", label, "body", "");
                b.nodes[l as usize].base_style.insert("flexGrow".into(), json!(1));
                b.nodes[l as usize].base_style.insert("minWidth".into(), json!(0));
            }
        };
        if owner.component == "Switch" {
            text(self, index);
            control(self, index);
        } else {
            control(self, index);
            text(self, index);
        }
        let _ = node;
    }

    fn radio(&mut self, index: u32, _node: &UiNode) {
        let owner = self.nodes[index as usize].clone();
        let horizontal = owner.props.get("orientation").and_then(Value::as_str) == Some("horizontal");
        {
            let s = &mut self.nodes[index as usize].base_style;
            s.entry("display".to_string()).or_insert(json!("flex"));
            s.entry("flexDirection".to_string()).or_insert(json!("column"));
            s.entry("gap".to_string()).or_insert(json!("$spacing.xs"));
        }
        self.nodes[index as usize].kind = NodeKind::Container;
        if let Some(label) = owner.props.get("label").and_then(Value::as_str) {
            self.text_part(index, &owner, "label", label, "label", "");
        }
        let items = self.part(&owner, "items", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": if horizontal { "row" } else { "column" }, "gap": "$spacing.sm", "flexWrap": "wrap"}), json!({}), &[]);
        let value = owner.props.get("value").map(crate::json::to_js_string);
        let options = owner.props.get("options").and_then(Value::as_array).cloned().unwrap_or_default();
        for (i, option) in options.iter().enumerate() {
            let v = option.get("value").map(crate::json::to_js_string).unwrap_or_default();
            let checked = value.as_deref() == Some(v.as_str());
            // VAPP-90: the ROW is a plain option row (no recipe — the
            // `Radio/item` recipe is the 16 px circle, and on the row it
            // shrank every option to the circle so the labels overlapped);
            // the `.dot` LEAF wears the `item` circle recipe (the sizing part
            // of `control-geometry.json`), and the painter draws the inner
            // `dot` recipe inside it when checked.
            let row = self.part_in(items, &owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": "$spacing.sm"}), json!({"value": v, "checked": checked}), &format!(".{i}"));
            self.nodes[row as usize].pressable = true;
            self.nodes[row as usize].part_query = None;
            let dot = self.part_in(row, &owner, "dot", "Radio", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"checked": checked}), &format!(".{i}"));
            self.nodes[dot as usize].pressable = true;
            let mut circle = RecipeQuery::new("Radio", "item", self.ctx.recipes.native_recipe_props("Radio", &owner.props), Vec::new());
            if checked {
                circle.states.push("checked".into());
            }
            self.nodes[dot as usize].part_query = Some(circle);
            let label = option.get("label").map(crate::json::to_js_string).unwrap_or_default();
            self.text_part(row, &owner, "label", &label, "body", &format!(".{i}"));
        }
    }

    fn slider(&mut self, index: u32, _node: &UiNode) {
        let owner = self.nodes[index as usize].clone();
        {
            let s = &mut self.nodes[index as usize].base_style;
            s.entry("display".to_string()).or_insert(json!("flex"));
            s.entry("flexDirection".to_string()).or_insert(json!("column"));
            s.entry("gap".to_string()).or_insert(json!("$spacing.xs"));
        }
        self.nodes[index as usize].kind = NodeKind::Container;
        if owner.props.get("label").is_some() {
            let head = self.part(&owner, "header", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between"}), json!({}), &[]);
            if let Some(label) = owner.props.get("label").and_then(Value::as_str) {
                self.text_part(head, &owner, "label", label, "label", "");
            }
            if let Some(v) = owner.props.get("value") {
                let text = crate::json::to_js_string(v);
                self.text_part(head, &owner, "value", &text, "muted", "");
            }
        }
        let mut props = owner.props.clone();
        props.remove("label");
        let track = self.part_in(index, &owner, "track", "Slider", NodeKind::Leaf, json!({"alignSelf": "stretch"}), Value::Object(props), "");
        self.nodes[track as usize].pressable = true;
    }
}

/// Build the layout tree for a normalized root. `Unknown` nodes become
/// leaves the painter renders as the placeholder.
pub fn build(root: &UiNode, ctx: &mut BuildContext) -> Built {
    let mut b = Builder { ctx, nodes: Vec::new(), layers: Vec::new(), lists: Vec::new() };
    b.add(root, None, 0, "", false);
    let _ = UNKNOWN_COMPONENT;
    Built { nodes: b.nodes, layers: b.layers, lists: b.lists }
}
