//! The LAYOUT tree: the normalized `UiNode` tree (macros already expanded)
//! becomes a flat pre-order list of layout nodes the engine and the painters
//! work on. Here the core adds what painters must not invent: the BIND pass
//! (bindings, calls and `$string.<id>` copy resolved in props, styles and
//! recipe props; a node whose `visible` resolves falsy is dropped with its
//! subtree), template children instantiated per item (ids `<template>.<key>`
//! with the template `key`, else `<template>.<i>`), natives with structure
//! of their own get synthetic PART nodes (ids `<owner>.<part>`, recipes keyed
//! on the owner's component so a theme styles them), overlay natives and
//! every popup (Select, the pickers, menus and submenus, tooltips, hover
//! cards, toasts) put their content into LAYERS with their own roots, and
//! extension natives arrive as `Extension` leaves.
//!
//! The builder numbers nodes in pre-order; the surface maps them onto
//! STABLE SLOTS by id afterwards (`surface::reconcile`), so a re-windowed
//! list or an opened overlay keeps every other node's index.

mod containers;
pub mod fields;
mod misc;
mod overlays;
mod table;

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::catalog::CatalogView;
use crate::data::{resolve_value, ResolveContext};
use crate::list::{ListWindow, VisibleRange};
use crate::overlay::{OverlayAlign, OverlaySide};
use crate::recipes::RecipeIndex;
use crate::strings::StringTable;
use crate::theme::RecipeQuery;
use crate::types::{Props, UiNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Container,
    Leaf,
}

/// One layout node. `style` keeps tokens UNRESOLVED and conditions NESTED;
/// the surface resolves them per theme/mode and flattens per pass.
#[derive(Debug, Clone, PartialEq)]
pub struct LNode {
    /// The SLOT (stable across rebuilds by id) once the surface reconciled;
    /// the pre-order position inside a fresh `Built`.
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
    /// Props with bindings, calls and `$string.<id>` resolved.
    pub props: Props,
    /// The structural style (author's + template's), bindings resolved,
    /// token refs NOT resolved, conditions nested.
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
    /// Where the node's SOURCE `UiNode` lives when its id is an instance id
    /// (a template item's descendants, a Table slot cell's controls carry
    /// `<sourceId><suffix>`). `None` = the id itself names a node of the
    /// surface's tree (or the node is a synthetic part).
    pub source: Option<SourceRef>,
    /// The `on` map of the source node (events route through the owner).
    pub on: Option<IndexMap<String, Value>>,
    pub accessibility: Option<Value>,
    /// The overlay node this node opens (a trigger slot's root).
    pub trigger_for: Option<String>,
    /// The enclosing Form's id (fields and submit buttons).
    pub form: Option<String>,
    /// A live region: `polite | assertive` (Text `live`, Toast, Form errors).
    pub live: Option<String>,
}

/// The source of an instance node: the source node's id and the template
/// component whose reduced tree holds it (`None` = the surface's tree).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub id: String,
    pub template: Option<String>,
}

/// How an overlay layer is placed.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerPlacement {
    /// Centred in the viewport (Dialog).
    Centered,
    /// Pinned to one viewport edge (Drawer).
    Edge(OverlaySide),
    /// Against the anchor node's frame (Popover, Tooltip, menus, pickers).
    Anchored { anchor: u32, side: OverlaySide, align: OverlayAlign },
    /// Against a point (a ContextMenu opened at the pointer).
    AtPoint { x: f32, y: f32 },
    /// The toast stack (bottom-centre on phones, bottom-end from `md`),
    /// `order` 0 = nearest the edge.
    Toast { order: u32 },
}

/// Which band a layer stacks in: base < overlay < toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerClass {
    Overlay,
    Toast,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerSpec {
    pub layer: u32,
    pub root: u32,
    pub kind: String,
    pub owner: String,
    pub placement: LayerPlacement,
    pub class: LayerClass,
    /// Dialog/Drawer: a scrim under it, the focus trapped inside.
    pub modal: bool,
    /// Escape / a scrim press / a drag closes it.
    pub dismissible: bool,
    /// The popup is at least as wide as its anchor (Select, pickers).
    pub match_anchor_width: bool,
    /// The layer's body that scrolls when the content is taller than the
    /// viewport (Dialog/Drawer `body`, a popup list).
    pub scroll_body: Option<u32>,
    /// The side is LOGICAL (a submenu opens at the inline end): right in
    /// LTR, left in RTL.
    pub mirror: bool,
}

/// A windowed list's state for one build.
#[derive(Debug, Clone, PartialEq)]
pub struct ListSpec {
    pub node: u32,
    /// The scroll key (the List's or the Table's id).
    pub id: String,
    pub keys: std::sync::Arc<Vec<String>>,
    pub range: VisibleRange,
    pub offsets: std::sync::Arc<Vec<f32>>,
    pub content_height: f32,
    pub windowed: bool,
}

/// A form field's validation state after a submit: its failed messages.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldErrors {
    pub name: String,
    pub messages: Vec<String>,
}

/// Local UI state the core owns (never sent to the producer unless bound).
#[derive(Debug, Clone, Default)]
pub struct LocalState {
    pub tabs: HashMap<String, String>,
    pub accordion: HashMap<String, Vec<String>>,
    pub carousel: HashMap<String, i64>,
    pub open: HashMap<String, bool>,
    pub pressed_toggles: HashMap<String, bool>,
    pub lists: HashMap<String, ListWindow>,
    /// Any scroll container's offset by node id (x, y).
    pub scroll: HashMap<String, (f32, f32)>,
    /// A searchable Select's query / a ChipInput's typed text.
    pub query: HashMap<String, String>,
    /// A calendar's visible month (year, month 1–12).
    pub month: HashMap<String, (i32, u32)>,
    /// A DateRangePicker's first pick, waiting for the second.
    pub range_anchor: HashMap<String, String>,
    /// The open submenu of a menu (item index).
    pub submenu: HashMap<String, usize>,
    /// Where a ContextMenu was opened (surface coordinates).
    pub context_point: HashMap<String, (f32, f32)>,
    /// The host's latest value of a field (text inputs are host-owned).
    pub field_values: HashMap<String, Value>,
    /// Failed checks per field id, set by a refused submit.
    pub errors: HashMap<String, FieldErrors>,
    /// Unbound Table sort and selection.
    pub sort: HashMap<String, (String, String)>,
    pub selected: HashMap<String, Vec<String>>,
    /// Unbound ChipInput values and FileUpload files.
    pub chips: HashMap<String, Vec<String>>,
    pub files: HashMap<String, Vec<Value>>,
    /// CodeBlocks whose copy button shows `copied`.
    pub copied: HashMap<String, bool>,
    /// The child keys of a container with STATIC children (per reduce).
    pub static_keys: HashMap<String, std::sync::Arc<Vec<String>>>,
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
    /// The surface's built-in string table.
    pub strings: &'a StringTable,
    /// The active breakpoint (`None` = base) for responsive native props.
    pub breakpoint: Option<String>,
    /// The surface locale (week start).
    pub locale: &'a str,
    /// The surface is at least `md` wide (the toast stack sits bottom-end).
    pub wide: bool,
    /// Today as `yyyy-mm-dd` (the host's clock; calendars open on it).
    pub today: Option<String>,
}

pub struct Built {
    pub nodes: Vec<LNode>,
    pub layers: Vec<LayerSpec>,
    pub lists: Vec<ListSpec>,
    /// A native read a responsive prop: a breakpoint change rebuilds.
    pub responsive: bool,
    /// Text whose live region announces when it changes: id → (text, live).
    pub toasts: Vec<ToastSpec>,
    /// Literal Table rows as data scopes (`/$row/<table>/<i>` → the row):
    /// events resolve against them after the build.
    pub row_scopes: HashMap<String, Value>,
    /// Form id → (disabled, busy), seeded into a later subtree rebuild.
    pub forms: HashMap<String, (bool, bool)>,
}

/// What a SUBTREE rebuild inherits from the full build: the enclosing
/// Forms' flags and the literal Table rows.
#[derive(Debug, Clone, Default)]
pub struct BuildSeed {
    pub row_scopes: HashMap<String, Value>,
    pub forms: HashMap<String, (bool, bool)>,
}

/// An open Toast the host times (`duration` 0 = sticky).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToastSpec {
    pub id: String,
    pub duration_ms: f64,
    #[serde(rename = "type")]
    pub kind: String,
}

pub(crate) struct Builder<'a, 'b> {
    pub(crate) ctx: &'b mut BuildContext<'a>,
    pub(crate) nodes: Vec<LNode>,
    pub(crate) layers: Vec<LayerSpec>,
    pub(crate) lists: Vec<ListSpec>,
    pub(crate) responsive: bool,
    pub(crate) toasts: Vec<ToastSpec>,
    /// Toast nodes waiting for the toast band (built after every overlay).
    pub(crate) pending_toasts: Vec<(u32, UiNode)>,
    /// The next layer number (layers nest: a submenu opens inside a menu).
    pub(crate) next_layer: u32,
    /// Literal Table rows as data scopes (`/$row/<table>/<i>` → the row).
    pub(crate) row_scopes: HashMap<String, Value>,
    /// Form id → (disabled, busy), for the fields and buttons inside.
    pub(crate) forms: HashMap<String, (bool, bool)>,
    /// Instance id → its source (filled by [`Builder::instance`]).
    pub(crate) sources: HashMap<String, SourceRef>,
    /// Template item suffixes per array (and key) for this build.
    pub(crate) suffixes: HashMap<String, std::sync::Arc<Vec<String>>>,
}

pub(crate) fn obj(v: Value) -> Props {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn with_suffix(node: &UiNode, suffix: &str, template: Option<&str>, sources: &mut HashMap<String, SourceRef>) -> UiNode {
    let mut out = node.clone();
    out.id = format!("{}{suffix}", node.id);
    // An id that is already an instance keeps ITS source (a Table slot
    // inside a template row); an original id is its own source.
    let source = sources.get(&node.id).cloned().unwrap_or_else(|| SourceRef { id: node.id.clone(), template: template.map(str::to_string) });
    sources.insert(out.id.clone(), source);
    out.children = node.children.iter().map(|c| with_suffix(c, suffix, template, sources)).collect();
    if let Some(slots) = &node.slots {
        out.slots = Some(slots.iter().map(|(k, v)| (k.clone(), with_suffix(v, suffix, template, sources))).collect());
    }
    out
}

/// The overlay natives whose content lives in a layer.
pub const OVERLAYS: &[&str] = &["Dialog", "Drawer", "Popover", "Tooltip", "DropdownMenu", "ContextMenu"];

/// The form fields a Form collects (when they carry a `name`).
pub const FORM_FIELDS: &[&str] =
    &["Input", "Textarea", "NumberField", "Checkbox", "Radio", "Switch", "Select", "ChipInput", "DatePicker", "DateRangePicker", "TimePicker", "FileUpload", "Slider"];

pub(crate) fn str_prop<'p>(props: &'p Props, key: &str) -> Option<&'p str> {
    props.get(key).and_then(Value::as_str)
}

pub(crate) fn bool_prop(props: &Props, key: &str) -> bool {
    props.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn js(v: &Value) -> String {
    crate::json::to_js_string(v)
}

impl<'a, 'b> Builder<'a, 'b> {
    pub(crate) fn resolve_ctx<'s>(&'s self, scope: &'s str) -> ResolveContext<'s> {
        let ctx = ResolveContext::new(self.ctx.data, scope).with_strings(self.ctx.strings);
        match row_scope_root(scope).and_then(|root| self.row_scopes.get_key_value(root)) {
            Some((root, row)) => ctx.with_overlay(root, row),
            None => ctx,
        }
    }

    pub(crate) fn resolve(&self, value: &Value, scope: &str) -> Option<Value> {
        resolve_value(value, &self.resolve_ctx(scope))
    }

    pub(crate) fn resolve_map(&self, map: &Props, scope: &str) -> Props {
        obj(resolve_value(&Value::Object(map.clone()), &self.resolve_ctx(scope)).unwrap_or(Value::Null))
    }

    /// A built-in string by id.
    pub(crate) fn string(&self, id: &str) -> String {
        self.ctx.strings.get(id).cloned().unwrap_or_else(|| id.to_string())
    }

    /// A built-in string with its `{name}` placeholders filled
    /// (`strings::format_string`), e.g. `removeItem` = "Remove {name}".
    pub(crate) fn string_with(&self, id: &str, params: &[(&str, &str)]) -> String {
        let params: Map<String, Value> = params.iter().map(|(k, v)| (k.to_string(), Value::String(v.to_string()))).collect();
        crate::strings::format_string(&self.string(id), &params)
    }

    /// An INSTANCE of `node` (every id + `suffix`), its sources recorded:
    /// `template` = the template component the original ids belong to
    /// (`None` = the surface's tree).
    pub(crate) fn instance(&mut self, node: &UiNode, suffix: &str, template: Option<&str>) -> UiNode {
        with_suffix(node, suffix, template, &mut self.sources)
    }

    pub(crate) fn push(&mut self, mut node: LNode) -> u32 {
        let index = self.nodes.len() as u32;
        node.index = index;
        if let Some(p) = node.parent {
            self.nodes[p as usize].children.push(index);
        }
        self.nodes.push(node);
        index
    }

    pub(crate) fn blank(&self, id: &str, component: &str, parent: Option<u32>, layer: u32, kind: NodeKind, scope: &str) -> LNode {
        let (depth, hidden, form) = parent
            .map(|p| {
                let pn = &self.nodes[p as usize];
                (pn.depth + 1, pn.hidden, pn.form.clone())
            })
            .unwrap_or((0, false, None));
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
            source: None,
            on: None,
            accessibility: None,
            trigger_for: None,
            form,
            live: None,
        }
    }

    /// A synthetic part of `owner` under `parent`, id `<owner>.<part><suffix>`,
    /// with the owner's recipe for that part.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn part_in(&mut self, parent: u32, owner: &LNode, part: &str, component: &str, kind: NodeKind, style: Value, props: Value, suffix: &str) -> u32 {
        let id = format!("{}.{part}{suffix}", owner.id);
        let layer = self.nodes[parent as usize].layer;
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

    /// [`Self::part_in`] directly under the owner.
    pub(crate) fn part(&mut self, owner: &LNode, part: &str, component: &str, kind: NodeKind, style: Value, props: Value) -> u32 {
        self.part_in(owner.index, owner, part, component, kind, style, props, "")
    }

    pub(crate) fn text_part(&mut self, parent: u32, owner: &LNode, part: &str, text: &str, variant: &str, suffix: &str) -> u32 {
        self.part_in(parent, owner, part, "Text", NodeKind::Leaf, json!({}), json!({"text": text, "variant": variant}), suffix)
    }

    /// An icon leaf part with the builtin glyph of `<Owner>.<slot>`.
    pub(crate) fn icon_part(&mut self, parent: u32, owner: &LNode, part: &str, glyph_slot: &str, suffix: &str) -> u32 {
        let name = builtin_icon(glyph_slot).unwrap_or("ui-icon-placeholder");
        self.part_in(parent, owner, part, "Icon", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"name": name, "size": "sm"}), suffix)
    }

    /// Add a state to a node's part query (`selected`, `open`, `checked`…).
    pub(crate) fn add_state(&mut self, index: u32, state: &str) {
        if let Some(q) = &mut self.nodes[index as usize].part_query {
            if !q.states.iter().any(|s| s == state) {
                q.states.push(state.to_string());
            }
        }
    }

    /// Set a recipe prop on a node's part query (a part-level discriminator).
    pub(crate) fn query_prop(&mut self, index: u32, key: &str, value: Value) {
        if let Some(q) = &mut self.nodes[index as usize].part_query {
            q.props.insert(key.to_string(), value);
        }
    }

    pub(crate) fn style_default(&mut self, index: u32, entries: &[(&str, Value)]) {
        let s = &mut self.nodes[index as usize].base_style;
        for (k, v) in entries {
            s.entry(k.to_string()).or_insert(v.clone());
        }
    }

    pub(crate) fn is_overlay(component: &str) -> bool {
        OVERLAYS.contains(&component)
    }

    /// Is a node visible under the data model (`visible` absent = yes)?
    fn visible(&self, node: &UiNode, scope: &str) -> bool {
        crate::data::is_visible(node.visible.as_ref(), &self.resolve_ctx(scope))
    }

    /// Add `node` under `parent`; `None` when its `visible` is falsy.
    pub(crate) fn add(&mut self, node: &UiNode, parent: Option<u32>, layer: u32, scope: &str, force_hidden: bool) -> Option<u32> {
        if !self.visible(node, scope) {
            return None;
        }
        let view = self.ctx.view;
        let def = view.component(&node.component);
        let mut props = self.resolve_map(&node.props, scope);
        self.local_overrides(&node.id, &node.component, &mut props);
        // Inside a disabled Form every field (and button) is inert; inside a
        // busy one the submit buttons show loading.
        let form = parent.and_then(|p| self.nodes[p as usize].form.clone());
        if let Some((disabled, busy)) = form.as_ref().and_then(|f| self.forms.get(f)).copied() {
            if disabled && (FORM_FIELDS.contains(&node.component.as_str()) || node.component == "Button") {
                props.insert("disabled".into(), Value::Bool(true));
            }
            if busy && node.component == "Button" && bool_prop(&props, "submit") {
                props.insert("loading".into(), Value::Bool(true));
            }
        }
        let ext = view.extension_of(&node.component).map(str::to_string);
        let children_rule = def.map(|d| d.children.as_str()).unwrap_or("none");
        let has_children = !node.children.is_empty() || node.template.is_some();
        let component = if ext.is_some() { "Extension".to_string() } else { node.component.clone() };
        let kind = if ext.is_some() {
            if has_children {
                NodeKind::Container
            } else {
                NodeKind::Leaf
            }
        } else {
            match (node.component.as_str(), children_rule) {
                ("Box", _) => NodeKind::Container,
                (_, "none") => NodeKind::Leaf,
                ("Button", "one") => {
                    if has_children {
                        NodeKind::Container
                    } else {
                        NodeKind::Leaf
                    }
                }
                _ => NodeKind::Container,
            }
        };
        let mut n = self.blank(&node.id, &component, parent, layer, kind, scope);
        n.hidden |= force_hidden;
        if let Some(e) = ext {
            n.catalog_id = Some(e);
            n.extension_kind = Some(node.component.clone());
        }
        // The author's `style` (top level) wins over a `style` PROP; both may
        // carry bound values after expansion (resolved here).
        let mut base: Props = props.get("style").and_then(Value::as_object).cloned().unwrap_or_default();
        if let Some(s) = &node.style {
            for (k, v) in self.resolve_map(s, scope) {
                base.insert(k, v);
            }
        }
        n.base_style = base;
        n.lines = props.get("lines").and_then(Value::as_u64).map(|l| l as u32);
        n.pressable = (node.on.as_ref().is_some_and(|o| o.contains_key("press")) || bool_prop(&props, "pressable") || (node.component == "Button" && bool_prop(&props, "submit"))) && !bool_prop(&props, "disabled") && !bool_prop(&props, "loading");
        n.on = node.on.clone();
        n.source = self.sources.get(&node.id).cloned();
        n.accessibility = node.accessibility.as_ref().and_then(|a| self.resolve(a, scope));
        n.live = str_prop(&props, "live").filter(|l| *l != "off").map(str::to_string);
        n.own_query = Some(RecipeQuery::new(node.component.clone(), "root", self.ctx.recipes.native_recipe_props(&node.component, &props), Vec::new()));
        if let Some(r) = &node.recipe {
            n.part_query = Some(RecipeQuery::new(r.macro_.clone(), r.part.clone(), self.resolve_map(&r.props, scope), Vec::new()));
        }
        if node.component == "Form" {
            n.form = Some(node.id.clone());
        }
        n.props = props;
        let index = self.push(n);
        if let Some(errors) = self.ctx.local.errors.get(&node.id) {
            if !errors.messages.is_empty() {
                let _ = errors;
                self.add_state_own(index, "invalid");
            }
        }

        // The component-specific structure; everything else: slots first
        // (pre-order like the reference `preorder`), then children.
        let expand = self.ctx.expand_controls;
        match node.component.as_str() {
            "Tabs" => self.tabs(index, node, scope),
            "Accordion" => self.accordion(index, node, scope),
            "Carousel" => self.carousel(index, node, scope),
            "List" => self.list(index, node, scope),
            "Table" => self.table(index, node, scope),
            "Toast" => self.toast(index, node),
            c if Self::is_overlay(c) => self.overlay(index, node, scope),
            "Form" => self.form(index, node, scope),
            "Input" | "Textarea" if expand => self.field(index),
            "Select" if expand => self.select(index),
            "DatePicker" | "DateRangePicker" if expand => self.date_picker(index),
            "TimePicker" if expand => self.time_picker(index),
            "NumberField" if expand => self.number_field(index),
            "ChipInput" if expand => self.chip_input(index),
            "FileUpload" if expand => self.file_upload(index),
            "CodeBlock" if expand => self.code_block(index),
            "Checkbox" | "Switch" if expand => self.check(index),
            "Radio" if expand => self.radio(index),
            "Slider" if expand => self.slider(index),
            "Chart" => self.chart(index),
            _ => {
                if let Some(slots) = &node.slots {
                    for slot in slots.values() {
                        self.add(slot, Some(index), layer, scope, false);
                    }
                }
                self.children_of(index, node, scope, false);
            }
        }
        Some(index)
    }

    /// A state on a node's OWN recipe query (an invalid field's root).
    fn add_state_own(&mut self, index: u32, state: &str) {
        if let Some(q) = &mut self.nodes[index as usize].own_query {
            if !q.states.iter().any(|s| s == state) {
                q.states.push(state.to_string());
            }
        }
    }

    /// Local UI state that overrides an UNBOUND prop for this build (the
    /// host's latest field value, unbound chips/files/sort/selection).
    fn local_overrides(&self, id: &str, component: &str, props: &mut Props) {
        let local = &self.ctx.local;
        if let Some(v) = local.field_values.get(id) {
            match component {
                "DateRangePicker" => {
                    for k in ["start", "end"] {
                        if let Some(x) = v.get(k) {
                            props.insert(k.into(), x.clone());
                        }
                    }
                }
                _ => {
                    let key = match component {
                        "Checkbox" | "Switch" => "checked",
                        "ChipInput" => "values",
                        "FileUpload" => "files",
                        _ => "value",
                    };
                    props.insert(key.into(), v.clone());
                }
            }
        }
        if component == "Toggle" {
            if let Some(pressed) = local.pressed_toggles.get(id) {
                props.insert("pressed".into(), Value::Bool(*pressed));
            }
        }
        if component == "ChipInput" {
            if let Some(chips) = local.chips.get(id) {
                props.insert("values".into(), Value::Array(chips.iter().cloned().map(Value::String).collect()));
            }
        }
        if component == "FileUpload" {
            if let Some(files) = local.files.get(id) {
                props.insert("files".into(), Value::Array(files.clone()));
            }
        }
        if component == "Table" {
            if let Some((key, dir)) = local.sort.get(id) {
                props.insert("sort".into(), json!({"key": key, "direction": dir}));
            }
            if let Some(sel) = local.selected.get(id) {
                props.insert("selected".into(), Value::Array(sel.iter().cloned().map(Value::String).collect()));
            }
        }
    }

    /// The id suffixes of a template's items (the reference's
    /// `templateItems`): `.<index>` without a `key`; with one, `.<value>`
    /// (objects as JSON), and `.#<index>` when the value is missing, null,
    /// empty or a DUPLICATE of an earlier item's (more `#` until unique), so
    /// instance ids never collide. Once per array per build.
    fn item_suffixes(&mut self, t: &crate::types::Template, base: &str) -> std::sync::Arc<Vec<String>> {
        let cache_key = format!("{base}\u{0}{}", t.key.as_deref().unwrap_or(""));
        if let Some(s) = self.suffixes.get(&cache_key) {
            return s.clone();
        }
        let count = crate::data::get_pointer(self.ctx.data, base).and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let mut key = i.to_string();
            if let Some(k) = &t.key {
                let pointer = crate::data::absolute_path(k, &format!("{base}/{i}"));
                let own = match crate::data::get_pointer(self.ctx.data, &pointer) {
                    None | Some(Value::Null) => None,
                    Some(Value::String(s)) if s.is_empty() => None,
                    Some(v @ (Value::Object(_) | Value::Array(_))) => Some(serde_json::to_string(v).unwrap_or_default()),
                    Some(v) => Some(js(v)),
                };
                key = match own {
                    Some(o) if !seen.contains(&o) => o,
                    _ => format!("#{i}"),
                };
                while seen.contains(&key) {
                    key = format!("#{key}");
                }
            }
            seen.insert(key.clone());
            out.push(format!(".{key}"));
        }
        let out = std::sync::Arc::new(out);
        self.suffixes.insert(cache_key, out.clone());
        out
    }

    /// The node's children (static or from its template) under `parent`.
    pub(crate) fn children_of(&mut self, parent: u32, node: &UiNode, scope: &str, hidden: bool) -> Vec<u32> {
        let mut out = Vec::new();
        let layer = self.nodes[parent as usize].layer;
        for child in &node.children {
            out.extend(self.add(child, Some(parent), layer, scope, hidden));
        }
        if let Some(t) = &node.template {
            let base = crate::data::absolute_path(&t.path, scope);
            let count = crate::data::get_pointer(self.ctx.data, &base).and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            if let Some(tpl) = (self.ctx.template)(&t.component) {
                let suffixes = self.item_suffixes(t, &base);
                for i in 0..count {
                    let item_scope = format!("{base}/{i}");
                    let suffix = suffixes[i].clone();
                    let item = self.instance(&tpl, &suffix, Some(&t.component));
                    out.extend(self.add(&item, Some(parent), layer, &item_scope, hidden));
                }
            }
        }
        out
    }

    /// The keys of a node's children without building them (the windowing
    /// keys: static ids, then template items by key or index). Static lists
    /// are computed once per reduce.
    pub(crate) fn child_keys(&mut self, node: &UiNode, scope: &str) -> std::sync::Arc<Vec<String>> {
        if node.template.is_none() {
            if let Some(k) = self.ctx.local.static_keys.get(&node.id) {
                if k.len() == node.children.len() {
                    return k.clone();
                }
            }
            let keys = std::sync::Arc::new(node.children.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
            self.ctx.local.static_keys.insert(node.id.clone(), keys.clone());
            return keys;
        }
        let mut keys: Vec<String> = node.children.iter().map(|c| c.id.clone()).collect();
        if let Some(t) = &node.template {
            let base = crate::data::absolute_path(&t.path, scope);
            for suffix in self.item_suffixes(t, &base).iter() {
                keys.push(format!("{}{suffix}", t.component));
            }
        }
        std::sync::Arc::new(keys)
    }

    /// Child `i` (static children first, then template items).
    pub(crate) fn one_child(&mut self, parent: u32, node: &UiNode, i: usize, scope: &str, hidden: bool) -> Option<u32> {
        let layer = self.nodes[parent as usize].layer;
        if let Some(child) = node.children.get(i) {
            return self.add(child, Some(parent), layer, scope, hidden);
        }
        let t = node.template.as_ref()?;
        let base = crate::data::absolute_path(&t.path, scope);
        let count = crate::data::get_pointer(self.ctx.data, &base).and_then(Value::as_array).map(Vec::len)?;
        let idx = i.checked_sub(node.children.len())?;
        if idx >= count {
            return None;
        }
        let tpl = (self.ctx.template)(&t.component)?;
        let item_scope = format!("{base}/{idx}");
        let suffix = self.item_suffixes(t, &base).get(idx).cloned().unwrap_or_else(|| format!(".{idx}"));
        let item = self.instance(&tpl, &suffix, Some(&t.component));
        self.add(&item, Some(parent), layer, &item_scope, hidden)
    }

    /// A responsive native prop at the active breakpoint (`Drawer.side`).
    pub(crate) fn responsive_prop(&mut self, props: &Props, key: &str) -> Option<Value> {
        let v = props.get(key)?;
        if crate::macros::is_responsive_value(v) {
            self.responsive = true;
            return crate::macros::responsive_at(v, self.ctx.breakpoint.as_deref()).cloned();
        }
        Some(v.clone())
    }

    /// Is the overlay `id` open (local state over the prop)?
    pub(crate) fn is_open(&self, id: &str, props: &Props, default: bool) -> bool {
        self.ctx.local.open.get(id).copied().unwrap_or_else(|| props.get("open").and_then(Value::as_bool).unwrap_or(default))
    }

    /// Start a new layer: its root container (a synthetic part of `owner`).
    pub(crate) fn layer_root(&mut self, owner: &LNode, part: &str, style: Value) -> (u32, u32) {
        let layer = self.next_layer;
        self.next_layer += 1;
        let mut root = self.blank(&format!("{}.{part}", owner.id), "Box", None, layer, NodeKind::Container, &owner.scope);
        root.part = Some(part.to_string());
        root.owner = Some(owner.id.clone());
        root.owner_component = Some(owner.component.clone());
        root.form = owner.form.clone();
        root.base_style = obj(style);
        root.part_query = Some(RecipeQuery::new(owner.component.clone(), part, self.ctx.recipes.native_recipe_props(&owner.component, &owner.props), vec!["open".into()]));
        let index = self.push(root);
        (layer, index)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_layer(&mut self, layer: u32, root: u32, owner: &LNode, placement: LayerPlacement, modal: bool, dismissible: bool, match_anchor_width: bool, scroll_body: Option<u32>) {
        let class = if matches!(placement, LayerPlacement::Toast { .. }) { LayerClass::Toast } else { LayerClass::Overlay };
        self.layers.push(LayerSpec { layer, root, kind: owner.component.clone(), owner: owner.id.clone(), placement, class, modal, dismissible, match_anchor_width, scroll_body, mirror: false });
    }
}

/// `/$row/<table>/<i>[/…]` → `/$row/<table>/<i>` (a literal Table row's
/// scope root).
pub fn row_scope_root(scope: &str) -> Option<&str> {
    let rest = scope.strip_prefix("/$row/")?;
    let mut end = "/$row/".len();
    for (k, part) in rest.splitn(3, '/').enumerate().take(2) {
        end += part.len() + usize::from(k == 0);
    }
    Some(&scope[..end])
}

/// The builtin glyph of a part (`core.catalog.json` `builtinIcons`).
pub fn builtin_icon(slot: &str) -> Option<&'static str> {
    let g = crate::generated::catalog::BUILTIN_ICON_SLOTS.iter().position(|s| *s == slot)?;
    crate::generated::catalog::BUILTIN_ICON_NAMES.get(g).copied()
}

/// Build the layout tree for a normalized root. `Unknown` nodes become
/// leaves the painter renders as the placeholder.
pub fn build(root: &UiNode, ctx: &mut BuildContext) -> Built {
    let mut b = Builder { ctx, nodes: Vec::new(), layers: Vec::new(), lists: Vec::new(), responsive: false, toasts: Vec::new(), pending_toasts: Vec::new(), next_layer: 1, row_scopes: HashMap::new(), forms: HashMap::new(), sources: HashMap::new(), suffixes: HashMap::new() };
    if b.add(root, None, 0, "", false).is_none() {
        // An invisible root: an empty surface (one hidden Box).
        let mut n = b.blank(&root.id, "Box", None, 0, NodeKind::Container, "");
        n.hidden = true;
        b.push(n);
    }
    b.flush_toasts();
    // Layers in paint order: a parent layer before the layers it opened
    // (layer numbers are handed out when a layer's root is created).
    b.layers.sort_by_key(|l| (l.class == LayerClass::Toast, l.layer));
    Built { nodes: b.nodes, layers: b.layers, lists: b.lists, responsive: b.responsive, toasts: b.toasts, row_scopes: b.row_scopes, forms: b.forms }
}

/// Rebuild ONE subtree (a re-windowed List/Table) under an existing parent:
/// node 0 of the result is a stub of `parent` (the caller keeps the real
/// one), the rest is the subtree in pre-order. `seed` = what the full build
/// knew above it (the enclosing Forms' disabled/busy, literal Table rows),
/// so the subtree comes out exactly as a full rebuild would build it.
pub fn build_subtree(node: &UiNode, parent: &LNode, scope: &str, seed: BuildSeed, ctx: &mut BuildContext) -> Built {
    let mut stub = parent.clone();
    stub.index = 0;
    stub.parent = None;
    stub.children.clear();
    let layer = stub.layer;
    let mut b = Builder { ctx, nodes: vec![stub], layers: Vec::new(), lists: Vec::new(), responsive: false, toasts: Vec::new(), pending_toasts: Vec::new(), next_layer: u32::MAX / 2, row_scopes: seed.row_scopes, forms: seed.forms, sources: HashMap::new(), suffixes: HashMap::new() };
    b.add(node, Some(0), layer, scope, false);
    b.flush_toasts();
    Built { nodes: b.nodes, layers: b.layers, lists: b.lists, responsive: b.responsive, toasts: b.toasts, row_scopes: b.row_scopes, forms: b.forms }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_glyphs_resolve() {
        assert_eq!(builtin_icon("CodeBlock.copy"), Some("ui-copy"));
        assert_eq!(builtin_icon("Table.sortIcon.asc"), Some("ui-chevron-up"));
        assert_eq!(builtin_icon("Nope.x"), None);
    }
}
