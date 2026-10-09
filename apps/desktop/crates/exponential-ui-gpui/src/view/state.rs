//! The node cache (the core's STABLE slots: a removed node stays a tombstone
//! so `nodes[i].index == i`), patched from each pass's `NodeDelta`, the a11y
//! roles and labels, and the pure focus helpers (Tab order = PAINT order,
//! roving tab stops for tabs and radios).

use std::collections::HashMap;

use exponential_ui::layout_tree::NodeKind;
use exponential_ui::strings::StringTable;
use exponential_ui::surface::{NodeDelta, PlacedNode, Surface};
use gpui::{Role, SharedString};
use serde_json::Value;

/// The part states the core decided for a node (`PlacedNode::states`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NodeFlags {
    pub selected: bool,
    pub open: bool,
    pub checked: bool,
    pub invalid: bool,
    pub disabled: bool,
}

impl NodeFlags {
    pub fn of(n: &PlacedNode) -> NodeFlags {
        let has = |s: &str| n.states.iter().any(|x| x == s);
        NodeFlags { selected: has("selected"), open: has("open"), checked: has("checked"), invalid: has("invalid"), disabled: has("disabled") }
    }
}

/// The macro a container node is the ROOT of (`Card`, `Group`, `Alert`:
/// the a11y group / alert roles), from the core's part recipe query.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroRoot {
    pub name: String,
    /// The macro's `type` recipe prop (Alert: `info|success|warning|error`).
    pub kind: Option<String>,
}

#[derive(Default)]
pub(crate) struct NodeCache {
    pub version: u64,
    /// By slot (`nodes[i].index == i`); removed slots are tombstones.
    pub nodes: Vec<PlacedNode>,
    pub by_id: HashMap<String, u32>,
    /// Extension node id → its kind (the measurer's painter lookup).
    pub kinds: HashMap<String, String>,
    pub ids: Vec<SharedString>,
    /// The node restyles on hover (`Surface::hover_styled`: a `:hover`
    /// block or a hover recipe rule; theme-dependent, rebuilt on a theme
    /// switch).
    pub hover_styled: Vec<bool>,
    /// Macro roots by slot (sparse).
    pub macros: HashMap<u32, MacroRoot>,
}

impl NodeCache {
    /// Every slot from the surface.
    pub fn build(surface: &mut Surface) -> NodeCache {
        let mut c = NodeCache { version: surface.structure_version(), ..Default::default() };
        for n in surface.nodes() {
            c.put(surface, n);
        }
        c
    }

    /// Apply one pass's delta (`renumbered` = everything moved: rebuild).
    /// With `old`, the LAYER node versions (layer > 0) the delta replaces or
    /// removes are kept there (the first one per slot wins), so a closing
    /// layer can still paint; main-tree nodes are never copied.
    pub fn patch(&mut self, surface: &mut Surface, delta: &NodeDelta, mut old: Option<&mut HashMap<u32, PlacedNode>>) {
        if delta.renumbered {
            *self = NodeCache::build(surface);
            return;
        }
        for &r in &delta.removed {
            if let Some(n) = self.nodes.get_mut(r as usize) {
                if let Some(o) = old.as_deref_mut() {
                    if !n.removed && n.layer > 0 {
                        o.entry(r).or_insert_with(|| n.clone());
                    }
                }
                if self.by_id.get(&n.id) == Some(&r) {
                    self.by_id.remove(&n.id);
                    self.kinds.remove(&n.id);
                }
                n.removed = true;
                n.children.clear();
                self.macros.remove(&r);
            }
        }
        let mut fetch: Vec<u32> = delta.added.iter().chain(delta.changed.iter()).copied().collect();
        fetch.sort_unstable();
        fetch.dedup();
        if let Some(&last) = fetch.last() {
            // New slots past the end: grow ONCE (not per slot).
            self.grow_to(last as usize + 1);
            for n in surface.nodes_at(&fetch) {
                let index = n.index;
                let previous = self.put(surface, n);
                if let Some(o) = old.as_deref_mut() {
                    if !previous.removed && previous.layer > 0 {
                        o.entry(index).or_insert(previous);
                    }
                }
            }
        }
        self.version = surface.structure_version();
    }

    /// Pad the slot vectors to `len` with tombstones (indexed in place).
    fn grow_to(&mut self, len: usize) {
        let start = self.nodes.len();
        if len <= start {
            return;
        }
        self.nodes.reserve(len - start);
        for k in start..len {
            self.nodes.push(PlacedNode {
                index: k as u32,
                id: String::new(),
                component: String::new(),
                part: None,
                owner: None,
                owner_component: None,
                catalog_id: None,
                extension_kind: None,
                depth: 0,
                parent: None,
                children: Vec::new(),
                layer: 0,
                kind: NodeKind::Container,
                props: Default::default(),
                lines: None,
                pressable: false,
                hidden: true,
                accessibility: None,
                trigger_for: None,
                states: Vec::new(),
                form: None,
                live: None,
                removed: true,
            });
        }
        self.ids.resize(len, SharedString::default());
        self.hover_styled.resize(len, false);
    }

    /// Store a node in its slot; the slot's previous node (a tombstone for
    /// a new slot).
    fn put(&mut self, surface: &Surface, n: PlacedNode) -> PlacedNode {
        let i = n.index as usize;
        self.grow_to(i + 1);
        if !n.removed {
            self.by_id.insert(n.id.clone(), n.index);
            if let Some(k) = &n.extension_kind {
                self.kinds.insert(n.id.clone(), k.clone());
            }
        }
        self.ids[i] = SharedString::from(n.id.clone());
        let layout = surface.layout_node(n.index);
        self.hover_styled[i] = !n.removed && surface.hover_styled(n.index);
        let root = layout.filter(|l| l.owner.is_none() && !n.removed).and_then(|l| l.part_query.as_ref()).filter(|q| q.part == "root" && matches!(q.component.as_str(), "Card" | "Group" | "Alert"));
        match root {
            Some(q) => {
                let kind = q.props.get("type").and_then(Value::as_str).map(str::to_string);
                self.macros.insert(n.index, MacroRoot { name: q.component.clone(), kind });
            }
            None => {
                self.macros.remove(&n.index);
            }
        }
        std::mem::replace(&mut self.nodes[i], n)
    }

    /// A live node (tombstones are `None`).
    pub fn node(&self, index: u32) -> Option<&PlacedNode> {
        self.nodes.get(index as usize).filter(|n| !n.removed)
    }

    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.by_id.get(id).copied()
    }

    /// The owner node of a synthetic part (else the node itself).
    pub fn owner_of(&self, index: u32) -> u32 {
        self.node(index).and_then(|n| n.owner.as_deref()).and_then(|o| self.index_of(o)).unwrap_or(index)
    }

    /// The macro root at a slot (`Card`/`Group`/`Alert`).
    pub fn macro_root(&self, index: u32) -> Option<&MacroRoot> {
        self.macros.get(&index)
    }

    /// The text of a macro root's `part` (its `title`, a Group's `footer`):
    /// the nearest descendant (breadth-first, 3 levels) with that id
    /// suffix.
    pub fn macro_part_text(&self, index: u32, part: &str) -> Option<SharedString> {
        let root = self.node(index)?;
        let suffix = format!(".{part}");
        let mut level: Vec<u32> = root.children.clone();
        for _ in 0..3 {
            let mut next = Vec::new();
            for c in level {
                let Some(n) = self.node(c) else { continue };
                if n.id.starts_with(&root.id) && n.id.ends_with(&suffix) {
                    return Some(crate::measure::display_text(n.props.get("text"))).filter(|t| !t.is_empty()).map(SharedString::from);
                }
                next.extend(n.children.iter().copied());
            }
            level = next;
        }
        None
    }

    /// The node that opens overlay `target` (its trigger).
    pub fn trigger_of(&self, target: &str) -> Option<u32> {
        self.nodes.iter().find(|n| !n.removed && n.trigger_for.as_deref() == Some(target)).map(|n| n.index)
    }
}

/// Is a node a host-owned text field (`Input`/`Textarea` `.field`, the
/// `Composer`, NumberField/ChipInput `input`, the Select `search`)?
pub fn is_text_field(n: &PlacedNode) -> bool {
    matches!(
        (n.component.as_str(), n.part.as_deref()),
        ("Input" | "Textarea", Some("field")) | ("Composer", None) | ("NumberField", Some("input")) | ("ChipInput", Some("input")) | ("Select", Some("search"))
    )
}

fn disabled(n: &PlacedNode) -> bool {
    matches!(n.props.get("disabled"), Some(Value::Bool(true))) || n.states.iter().any(|s| s == "disabled")
}

/// Keyboard-focusable: pressables, controls, fields, carousel dots, a
/// Radio's dot (not its row or label), a Checkbox/Switch root.
pub fn is_focusable(n: &PlacedNode) -> bool {
    if n.removed || n.hidden || disabled(n) {
        return false;
    }
    let owner = n.owner_component.as_deref().unwrap_or("");
    match (n.component.as_str(), n.part.as_deref()) {
        ("Box" | "Text", Some("item" | "label")) if owner == "Radio" => false,
        ("Text", Some("label")) => false,
        ("Checkbox" | "Switch", Some("box" | "track")) => false,
        ("Box", Some("row")) if owner == "Table" => n.pressable,
        _ if is_text_field(n) => true,
        ("Slider", Some("track")) | ("Box", Some("indicator")) | ("ToggleGroup", _) | ("Composer", _) => true,
        // Arrow keys move its tooltip between categories (a11y.json); a
        // sparkline has no tooltip and is no tab stop (as on the web).
        ("Chart", None) => !is_sparkline(n),
        ("Button" | "Link" | "Toggle", _) => true,
        _ => n.pressable,
    }
}

/// A Chart drawn as a bare sparkline (no axes, no tooltip, no keys).
pub fn is_sparkline(n: &PlacedNode) -> bool {
    n.component == "Chart" && n.props.get("kind").and_then(Value::as_str) == Some("sparkline")
}

/// The Tab order of one layer (0 = the main tree) from the PAINT order:
/// focusable nodes, a roving tab stop for Tabs (the selected tab) and Radio
/// (the checked dot, else the first).
pub fn focus_order(nodes: &[PlacedNode], paint_order: &[u32], layer: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut roving: HashMap<String, usize> = HashMap::new();
    for &i in paint_order {
        let Some(n) = nodes.get(i as usize) else { continue };
        if n.layer != layer || !is_focusable(n) {
            continue;
        }
        let group = match (n.owner_component.as_deref(), n.part.as_deref()) {
            (Some("Tabs"), Some("tab")) | (Some("Radio"), Some("dot")) => n.owner.clone(),
            _ => None,
        };
        let Some(g) = group else {
            out.push(i);
            continue;
        };
        let active = n.states.iter().any(|s| s == "selected" || s == "checked");
        match roving.get(&g) {
            None => {
                roving.insert(g, out.len());
                out.push(i);
            }
            Some(&pos) if active => out[pos] = i,
            Some(_) => {}
        }
    }
    out
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

/// The accessible name: `accessibility.label`, else the text/label/alt;
/// built-in names (a Spinner's `loading`) come from the surface's string
/// table.
pub fn a11y_label(n: &PlacedNode, strings: &StringTable) -> Option<SharedString> {
    if let Some(l) = n.accessibility.as_ref().and_then(|a| a.get("label")).and_then(Value::as_str) {
        return Some(l.to_string().into());
    }
    let s = match n.component.as_str() {
        "Image" | "Video" => str_of(n, "alt").or_else(|| str_of(n, "title")),
        "Avatar" => str_of(n, "name"),
        "Box" | "Extension" => str_of(n, "label").or_else(|| str_of(n, "title")).or_else(|| str_of(n, "text")),
        "Spinner" => str_of(n, "label").or_else(|| strings.get("loading").map(String::as_str)),
        "Link" => str_of(n, "label").or_else(|| str_of(n, "href")),
        "Chart" => str_of(n, "title").or_else(|| str_of(n, "kind")),
        "Icon" => str_of(n, "label"),
        _ => str_of(n, "text").or_else(|| str_of(n, "label")).or_else(|| str_of(n, "title")).or_else(|| str_of(n, "placeholder")).or_else(|| str_of(n, "alt")),
    };
    s.map(|s| s.to_string().into())
}

/// The accessible description (`accessibility.description`).
pub fn a11y_description(n: &PlacedNode) -> Option<SharedString> {
    n.accessibility.as_ref().and_then(|a| a.get("description")).and_then(Value::as_str).map(|s| SharedString::from(s.to_string()))
}

/// The accesskit role of a node (`None` = no a11y node of its own). A
/// macro ROOT (`macro_root`) maps per a11y.json: Card / Group = a group
/// (labelled by its title), Alert = an alert for `warning`/`error`, a
/// status otherwise.
pub fn role_of(n: &PlacedNode, parent_component: Option<&str>, macro_root: Option<&MacroRoot>) -> Option<Role> {
    let owner = n.owner_component.as_deref().unwrap_or("");
    let kind = n.props.get("kind").and_then(Value::as_str).unwrap_or("item");
    if let Some(m) = macro_root.filter(|_| n.component == "Box") {
        return Some(match m.name.as_str() {
            "Alert" if matches!(m.kind.as_deref(), Some("warning" | "error")) => Role::Alert,
            "Alert" => Role::Status,
            _ => Role::Group,
        });
    }
    Some(match (n.component.as_str(), n.part.as_deref()) {
        ("Button", _) => Role::Button,
        ("Link", _) => Role::Link,
        ("Toggle", _) => Role::Button,
        ("ToggleGroup", _) => Role::RadioGroup,
        ("Text", Some("tab")) => Role::Tab,
        ("Box", Some("list")) if owner == "Tabs" => Role::TabList,
        ("Box", Some("content")) if owner == "Tabs" => Role::TabPanel,
        ("Text", Some("trigger")) => Role::Button,
        ("Select", Some("trigger")) | ("TimePicker", Some("trigger")) => Role::ComboBox,
        ("DatePicker" | "DateRangePicker", Some("trigger")) => Role::DateInput,
        ("Select", Some("search")) => Role::SearchInput,
        ("Box", Some("list")) if matches!(owner, "Select" | "TimePicker") => Role::ListBox,
        ("Text", Some("item")) if matches!(owner, "Select" | "TimePicker") => Role::ListBoxOption,
        ("Box", Some("calendar")) => Role::Grid,
        ("Box", Some("week")) => Role::Row,
        ("Text", Some("day")) => Role::GridCell,
        ("Text", Some("weekday")) => Role::ColumnHeader,
        ("Box", Some("content")) if matches!(owner, "DropdownMenu" | "ContextMenu") => Role::Menu,
        ("Box", Some("item")) if matches!(owner, "DropdownMenu" | "ContextMenu") => match kind {
            "checkbox" => Role::MenuItemCheckBox,
            _ => Role::MenuItem,
        },
        ("Box", Some("content")) if owner == "Tooltip" => Role::Tooltip,
        ("Box", Some("content")) if matches!(owner, "Dialog" | "Drawer") => {
            if n.props.get("dismissible").and_then(Value::as_bool) == Some(false) {
                Role::AlertDialog
            } else {
                Role::Dialog
            }
        }
        ("Box", Some("content")) if owner == "Popover" => Role::Dialog,
        ("Box", Some("root")) if owner == "Toast" => {
            if n.live.as_deref() == Some("assertive") {
                Role::Alert
            } else {
                Role::Status
            }
        }
        ("Box", Some("header")) if owner == "Table" => Role::Row,
        ("Text", Some("headerCell")) => Role::ColumnHeader,
        ("Box", Some("row")) if owner == "Table" => Role::Row,
        ("Text" | "Box", Some("cell")) if owner == "Table" => Role::Cell,
        ("Text", Some("caption")) => Role::Caption,
        ("Box", Some("body")) if owner == "CodeBlock" => Role::Code,
        ("Box", Some("dropzone")) => Role::Button,
        ("Text", _) if n.live.is_some() => Role::Status,
        ("Text", _) => Role::Label,
        ("Markdown", _) => Role::Article,
        ("Input", Some("field")) | ("ChipInput", Some("input")) => Role::TextInput,
        ("NumberField", Some("input")) => Role::SpinButton,
        ("Textarea", Some("field")) | ("Composer", _) => Role::MultilineTextInput,
        ("Checkbox", None) | ("Checkbox", Some("checkbox")) => Role::CheckBox,
        ("Switch", None) => Role::Switch,
        ("Radio", None) => Role::RadioGroup,
        ("Radio", Some("dot")) => Role::RadioButton,
        ("Slider", Some("track")) => Role::Slider,
        ("Box", Some("handle")) if owner == "Resizable" => Role::Splitter,
        ("Resizable", None) => Role::Group,
        ("Box", Some("section")) if owner == "List" => Role::Heading,
        ("Image" | "Avatar" | "Video" | "Chart", _) => Role::Image,
        ("Icon", _) if str_of(n, "label").is_some() => Role::Image,
        ("Ring" | "Spinner", _) => Role::ProgressIndicator,
        ("Unknown", _) => Role::Note,
        ("Box", Some("indicator")) => Role::TabList,
        ("Form", _) => Role::Form,
        ("Table", None) => {
            if n.props.get("selectable").and_then(Value::as_str).is_some_and(|s| s != "none") {
                Role::Grid
            } else {
                Role::Table
            }
        }
        ("List", _) => Role::List,
        _ if parent_component == Some("List") && n.part.as_deref() != Some("divider") => Role::ListItem,
        ("Box", _) if n.pressable => Role::Button,
        ("Extension", _) if n.kind == NodeKind::Leaf => Role::Group,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use exponential_ui::measure::FixedMeasure;
    use exponential_ui::surface::SurfaceOptions;
    use exponential_ui::NestedNode;
    use serde_json::json;

    fn surface(tree: Value) -> (Surface, Vec<u32>) {
        let mut s = Surface::new("t", SurfaceOptions::default());
        s.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap());
        s.set_viewport(600.0, 0.0, None);
        let out = s.layout(&mut FixedMeasure::default());
        let order = out.frames.iter().map(|f| f.index).collect();
        (s, order)
    }

    #[test]
    fn focus_order_follows_paint_order_with_roving_stops() {
        let (mut s, order) = surface(json!({"id": "root", "component": "Box", "children": [
            {"id": "a", "component": "Button", "props": {"label": "A"}},
            {"id": "row", "component": "Box", "on": {"press": {"event": {"name": "row"}}}, "children": [
                {"id": "b", "component": "Button", "props": {"label": "B"}}
            ]},
            {"id": "off", "component": "Button", "props": {"label": "Off", "disabled": true}},
            {"id": "name", "component": "Input", "props": {"label": "Name"}},
            {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}], "value": "2"}, "children": [
                {"id": "p1", "component": "Button", "props": {"label": "In one"}},
                {"id": "p2", "component": "Button", "props": {"label": "In two"}}
            ]},
            {"id": "pick", "component": "Radio", "props": {"options": [{"label": "X", "value": "x"}, {"label": "Y", "value": "y"}], "value": "y"}}
        ]}));
        let cache = NodeCache::build(&mut s);
        let ids: Vec<&str> = focus_order(&cache.nodes, &order, 0).iter().map(|i| cache.nodes[*i as usize].id.as_str()).collect();
        assert_eq!(ids, ["a", "row", "b", "name.field", "tabs.tab.1", "p2", "pick.dot.1"], "one stop per tab list / radio group: the selected one");
        let tab = cache.index_of("tabs.tab.1").unwrap();
        assert_eq!(role_of(&cache.nodes[tab as usize], None, None), Some(Role::Tab));
        assert_eq!(a11y_label(&cache.nodes[cache.index_of("a").unwrap() as usize], s.strings()).as_deref(), Some("A"));
        assert!(NodeFlags::of(&cache.nodes[tab as usize]).selected);
    }

    #[test]
    fn a_delta_patch_matches_a_full_rebuild() {
        let mut s = Surface::new("t", SurfaceOptions::default());
        s.set_nested(serde_json::from_value::<NestedNode>(json!({"id": "root", "component": "Box", "children": [
            {"id": "a", "component": "Text", "props": {"text": "A"}},
            {"id": "b", "component": "Text", "props": {"text": "B"}}
        ]})).unwrap());
        s.set_viewport(400.0, 0.0, None);
        s.layout(&mut FixedMeasure::default());
        let mut cache = NodeCache::build(&mut s);
        s.set_nested(serde_json::from_value::<NestedNode>(json!({"id": "root", "component": "Box", "children": [
            {"id": "b", "component": "Text", "props": {"text": "B2"}},
            {"id": "c", "component": "Button", "props": {"label": "C"}}
        ]})).unwrap());
        let out = s.layout(&mut FixedMeasure::default());
        cache.patch(&mut s, &out.delta, None);
        let full = NodeCache::build(&mut s);
        let live = |c: &NodeCache| {
            let mut v: Vec<(String, u32)> = c.nodes.iter().filter(|n| !n.removed).map(|n| (n.id.clone(), n.index)).collect();
            v.sort();
            v
        };
        assert_eq!(live(&cache), live(&full));
        assert!(cache.index_of("a").is_none());
        let b = cache.index_of("b").unwrap();
        assert_eq!(cache.node(b).unwrap().props.get("text"), Some(&json!("B2")));
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
