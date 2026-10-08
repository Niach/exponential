//! The per-structure node cache (children lists, id lookup, a11y roles and
//! labels, parsed markdown, focus order) — rebuilt only when the core's
//! `structure_version` moves or an interaction may have re-resolved props —
//! and the pure focus-order helpers.

use std::collections::HashMap;
use std::rc::Rc;

use exponential_ui::layout_tree::NodeKind;
use exponential_ui::surface::{PlacedNode, Surface};
use gpui::{Role, SharedString};
use serde_json::Value;

use crate::paint::markdown::{self, Block};

/// Part states the core resolved into a node's recipe query (not on
/// `PlacedNode`): a tab `selected`, an accordion trigger `open`, a check
/// part `checked`; plus the macro a node expands (`Card`…).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeFlags {
    pub selected: bool,
    pub open: bool,
    pub checked: bool,
    pub macro_name: Option<String>,
}

#[derive(Default)]
pub(crate) struct NodeCache {
    pub version: u64,
    pub nodes: Vec<PlacedNode>,
    pub children: Vec<Vec<u32>>,
    pub by_id: HashMap<String, u32>,
    /// Extension node id → its kind (the measurer's painter lookup).
    pub kinds: HashMap<String, String>,
    pub flags: Vec<NodeFlags>,
    pub roles: Vec<Option<Role>>,
    pub labels: Vec<Option<SharedString>>,
    pub ids: Vec<SharedString>,
    pub markdown: HashMap<u32, Rc<Vec<Block>>>,
}

impl NodeCache {
    pub fn build(surface: &mut Surface) -> NodeCache {
        let nodes = surface.nodes();
        let version = surface.structure_version();
        let mut children = vec![Vec::new(); nodes.len()];
        let mut by_id = HashMap::with_capacity(nodes.len());
        let mut kinds = HashMap::new();
        let mut flags = Vec::with_capacity(nodes.len());
        let mut markdown_cache = HashMap::new();
        for n in &nodes {
            if let Some(p) = n.parent {
                children[p as usize].push(n.index);
            }
            by_id.insert(n.id.clone(), n.index);
            if let Some(k) = &n.extension_kind {
                kinds.insert(n.id.clone(), k.clone());
            }
            let mut f = NodeFlags::default();
            if let Some(l) = surface.layout_node(n.index) {
                if let Some(q) = &l.part_query {
                    f.selected = q.states.iter().any(|s| s == "selected");
                    f.open = q.states.iter().any(|s| s == "open");
                    f.checked = q.states.iter().any(|s| s == "checked");
                    if l.owner.is_none() {
                        f.macro_name = Some(q.component.clone());
                    }
                }
            }
            flags.push(f);
            if n.component == "Markdown" {
                let text = n.props.get("text").and_then(Value::as_str).unwrap_or("");
                markdown_cache.insert(n.index, Rc::new(markdown::parse(text)));
            }
        }
        let roles = nodes.iter().map(|n| role_of(n, &nodes, &flags[n.index as usize])).collect();
        let labels = nodes.iter().map(a11y_label).collect();
        let ids = nodes.iter().map(|n| SharedString::from(n.id.clone())).collect();
        NodeCache { version, nodes, children, by_id, kinds, flags, roles, labels, ids, markdown: markdown_cache }
    }

    pub fn node(&self, index: u32) -> Option<&PlacedNode> {
        self.nodes.get(index as usize)
    }

    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.by_id.get(id).copied()
    }

    /// The owner node of a synthetic part (else the node itself).
    pub fn owner_of(&self, index: u32) -> u32 {
        self.nodes.get(index as usize).and_then(|n| n.owner.as_deref()).and_then(|o| self.index_of(o)).unwrap_or(index)
    }
}

/// Is a node a host-owned text field (`Input`/`Textarea` `.field`, `Composer`)?
pub fn is_text_field(n: &PlacedNode) -> bool {
    matches!((n.component.as_str(), n.part.as_deref()), ("Input" | "Textarea", Some("field")) | ("Composer", None))
}

fn disabled(n: &PlacedNode) -> bool {
    matches!(n.props.get("disabled"), Some(Value::Bool(true)))
}

/// Keyboard-focusable: pressables, controls, fields, carousel dots.
pub fn is_focusable(n: &PlacedNode) -> bool {
    if n.hidden || disabled(n) {
        return false;
    }
    if n.pressable {
        return true;
    }
    matches!((n.component.as_str(), n.part.as_deref()), ("Button" | "Link" | "Toggle" | "ToggleGroup" | "Composer", _) | ("Box", Some("indicator")))
}

/// The Tab order of one layer (0 = the main tree): focusable nodes in
/// pre-order (= index order within a layer).
pub fn focus_order(nodes: &[PlacedNode], layer: u32) -> Vec<u32> {
    nodes.iter().filter(|n| n.layer == layer && is_focusable(n)).map(|n| n.index).collect()
}

/// The next (or previous) entry of `order` after `current` (wrapping);
/// nothing focused = the first (last).
pub fn next_focus(order: &[u32], current: Option<u32>, backwards: bool) -> Option<u32> {
    if order.is_empty() {
        return None;
    }
    let n = order.len();
    match current.and_then(|c| order.iter().position(|x| *x == c)) {
        Some(p) => Some(order[if backwards { (p + n - 1) % n } else { (p + 1) % n }]),
        None => Some(if backwards { order[n - 1] } else { order[0] }),
    }
}

fn str_of<'a>(n: &'a PlacedNode, key: &str) -> Option<&'a str> {
    n.props.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// The accessible name: `accessibility.label`, else the text/label/alt.
pub fn a11y_label(n: &PlacedNode) -> Option<SharedString> {
    if let Some(l) = n.accessibility.as_ref().and_then(|a| a.get("label")).and_then(Value::as_str) {
        return Some(l.to_string().into());
    }
    let s = match n.component.as_str() {
        "Image" | "Video" => str_of(n, "alt").or_else(|| str_of(n, "title")),
        "Avatar" => str_of(n, "name"),
        "Box" | "Extension" => str_of(n, "label").or_else(|| str_of(n, "title")),
        "Spinner" => str_of(n, "label").or(Some("Loading")),
        "Link" => str_of(n, "label").or_else(|| str_of(n, "href")),
        "Chart" => str_of(n, "title").or_else(|| str_of(n, "kind")),
        _ => str_of(n, "text").or_else(|| str_of(n, "label")).or_else(|| str_of(n, "title")).or_else(|| str_of(n, "placeholder")).or_else(|| str_of(n, "alt")),
    };
    s.map(|s| s.to_string().into())
}

/// The accesskit role of a node (`None` = no a11y node of its own).
pub fn role_of(n: &PlacedNode, nodes: &[PlacedNode], f: &NodeFlags) -> Option<Role> {
    let owner = n.owner_component.as_deref().unwrap_or("");
    let parent_component = n.parent.and_then(|p| nodes.get(p as usize)).map(|p| p.component.as_str());
    Some(match (n.component.as_str(), n.part.as_deref()) {
        ("Button", _) => Role::Button,
        ("Link", _) => Role::Link,
        ("Toggle", _) => Role::Button,
        ("ToggleGroup", _) => Role::RadioGroup,
        ("Text", Some("tab")) => Role::Tab,
        ("Box", Some("list")) if owner == "Tabs" => Role::TabList,
        ("Box", Some("content")) if owner == "Tabs" => Role::TabPanel,
        ("Text", Some("trigger")) => Role::Button,
        ("Text", Some("item")) => Role::MenuItem,
        ("Box", Some("content")) if n.layer > 0 => match owner {
            "Tooltip" => Role::Tooltip,
            "DropdownMenu" => Role::Menu,
            _ => Role::Dialog,
        },
        ("Text", _) => Role::Label,
        ("Markdown", _) => Role::Article,
        ("Input", Some("field")) => Role::TextInput,
        ("Textarea", Some("field")) | ("Composer", _) => Role::MultilineTextInput,
        ("Select", Some("field")) => Role::ComboBox,
        ("DatePicker", Some("field")) => Role::DateInput,
        ("Checkbox", None) => Role::CheckBox,
        ("Switch", None) => Role::Switch,
        ("Radio", None) => Role::RadioGroup,
        ("Box", Some("item")) if owner == "Radio" => Role::RadioButton,
        ("Slider", Some("track")) => Role::Slider,
        ("Image" | "Avatar" | "Video" | "Chart", _) => Role::Image,
        ("Icon", _) if str_of(n, "label").is_some() => Role::Image,
        ("Ring" | "Spinner", _) => Role::ProgressIndicator,
        ("Unknown", _) => Role::Note,
        ("Box", Some("indicator")) => Role::TabList,
        ("List", _) => Role::List,
        _ if parent_component == Some("List") && n.part.as_deref() != Some("divider") => Role::ListItem,
        ("Box", _) if n.pressable => Role::Button,
        ("Box", _) if matches!(f.macro_name.as_deref(), Some("Card" | "Group" | "Alert")) => Role::Group,
        ("Extension", _) if n.kind == NodeKind::Leaf => Role::Group,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use exponential_ui::surface::SurfaceOptions;
    use exponential_ui::NestedNode;
    use serde_json::json;

    fn surface(tree: Value) -> Surface {
        let mut s = Surface::new("t", SurfaceOptions::default());
        s.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap());
        s
    }

    #[test]
    fn focus_order_is_pre_order_and_skips_hidden_and_disabled() {
        let mut s = surface(json!({"id": "root", "component": "Box", "children": [
            {"id": "a", "component": "Button", "props": {"label": "A"}},
            {"id": "row", "component": "Box", "on": {"press": {"event": {"name": "row"}}}, "children": [
                {"id": "b", "component": "Button", "props": {"label": "B"}}
            ]},
            {"id": "off", "component": "Button", "props": {"label": "Off", "disabled": true}},
            {"id": "name", "component": "Input", "props": {"label": "Name"}},
            {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}]}, "children": [
                {"id": "p1", "component": "Button", "props": {"label": "In one"}},
                {"id": "p2", "component": "Button", "props": {"label": "In two"}}
            ]}
        ]}));
        let cache = NodeCache::build(&mut s);
        let order: Vec<&str> = focus_order(&cache.nodes, 0).iter().map(|i| cache.nodes[*i as usize].id.as_str()).collect();
        assert_eq!(order, ["a", "row", "b", "name.field", "tabs.tab.0", "tabs.tab.1", "p1"]);
        assert_eq!(cache.roles[cache.index_of("tabs.tab.0").unwrap() as usize], Some(Role::Tab));
        assert_eq!(cache.labels[cache.index_of("a").unwrap() as usize].as_deref(), Some("A"));
        assert!(cache.flags[cache.index_of("tabs.tab.0").unwrap() as usize].selected);
    }

    #[test]
    fn next_focus_wraps_both_ways() {
        let order = [3, 5, 9];
        assert_eq!(next_focus(&order, None, false), Some(3));
        assert_eq!(next_focus(&order, None, true), Some(9));
        assert_eq!(next_focus(&order, Some(5), false), Some(9));
        assert_eq!(next_focus(&order, Some(9), false), Some(3));
        assert_eq!(next_focus(&order, Some(3), true), Some(9));
        assert_eq!(next_focus(&order, Some(4), false), Some(3));
        assert_eq!(next_focus(&[], Some(4), false), None);
    }
}
