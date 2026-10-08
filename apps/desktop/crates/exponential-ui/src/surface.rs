//! A `Surface` = one A2UI surface: its catalog, its normalized tree, its
//! data model, its theme and mode, the local UI state the core owns, and one
//! taffy tree (main root + one root per open overlay layer) kept across
//! passes so only nodes whose resolved style changed are restyled.
//!
//! The API is COARSE by design (the FFI facade mirrors it one to one):
//! messages in (`apply`), viewport/state setters, `layout(measure)` out —
//! never a call per node or per keystroke.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use taffy::prelude::*;
use taffy::style::Direction;

use crate::catalog::{CatalogView, A2UI_BASIC_CATALOG_ID};
use crate::data::{absolute_path, get_pointer, set_pointer, ResolveContext};
use crate::layout_tree::{self, BuildContext, LNode, LayerPlacement, LayerSpec, ListSpec, LocalState, NodeKind};
use crate::measure::{ControlBox, HeightRequest, LeafRequest, Measure, MeasureMemo, MeasureRequest, TextStyle};
use crate::overlay::{place_overlay, OverlayPlacement, PlaceOptions, Rect, Size as OSize, OVERLAY_PADDING};
use crate::recipes::RecipeIndex;
use crate::reducer::{reduce_surface, ReduceOptions, ReduceResult};
use crate::style::{self, BoxKind, StyleContext, Visual};
use crate::theme::{resolve_recipe, resolve_style_values, Mode, ResolvedTheme};
use crate::types::{ExtensionDef, FlatComponent, NestedNode, ReduceIssue, UiNode};

/// Absolute frame in surface coordinates (points/dp; fractional unless
/// rounding is on).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The static description of one layout node, sent once per structure
/// version (`Surface::nodes`). Pre-order = paint order = accessibility order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedNode {
    pub index: u32,
    pub id: String,
    pub component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_kind: Option<String>,
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    pub layer: u32,
    pub kind: NodeKind,
    pub props: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pressable: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<Value>,
    /// The overlay this node opens when pressed (a trigger slot's root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_for: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlacedFrame {
    pub index: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One overlay layer in paint order, frames in SURFACE coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub layer: u32,
    pub kind: String,
    pub owner: String,
    pub root: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_frame: Option<Frame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<OverlayPlacement>,
    /// Drawer: the viewport edge; Dialog: `centered`.
    pub position: String,
    pub frames: Vec<PlacedFrame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListOutput {
    pub id: String,
    pub node: u32,
    pub content_height: f32,
    pub start: u32,
    pub end: u32,
    pub count: u32,
    pub windowed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutOutput {
    /// Main-tree frames, pre-order.
    pub frames: Vec<PlacedFrame>,
    pub layers: Vec<Layer>,
    pub lists: Vec<ListOutput>,
    /// Indices whose resolved visual changed since the last pass (all of
    /// them on the first pass after a structure change).
    pub visual_changes: Vec<u32>,
    pub structure_version: u64,
    pub surface_width: f32,
    pub surface_height: f32,
    /// Content taller than the bounded height (a transcript card may scroll).
    pub overflow: bool,
    pub measure_rounds: u32,
    pub upcalls: u32,
    pub layout_ns: u64,
}

/// What the host must do after an interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutEvent {
    /// A server event (A2UI `action`): forward it.
    Action { name: String, component_id: String, event: String, context: Value, #[serde(skip_serializing_if = "Option::is_none")] payload: Option<Value> },
    /// `openUrl` or a `Link`.
    OpenUrl { url: String },
    /// A bound value was written through to the data model.
    DataChanged { path: String, value: Value },
    /// A host-owned input edit (the host forwards with its revision).
    Input { component_id: String, name: String, #[serde(skip_serializing_if = "Option::is_none")] path: Option<String>, value: Value, commit: bool },
    /// Local UI state changed: lay out again.
    Relayout,
}

#[derive(Debug, Clone)]
pub struct SurfaceOptions {
    pub catalog_id: String,
    /// `None` = no theme: geometry mode (no recipes, styles verbatim).
    pub theme: Option<Arc<ResolvedTheme>>,
    pub mode: Mode,
    pub extensions: Vec<ExtensionDef>,
    pub rounding: bool,
}

impl Default for SurfaceOptions {
    fn default() -> Self {
        SurfaceOptions {
            catalog_id: crate::catalog::CORE_CATALOG_ID.to_string(),
            theme: Some(crate::themes::default_theme()),
            mode: Mode::Light,
            extensions: Vec::new(),
            rounding: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApplyOutcome {
    pub structure_changed: bool,
    pub issues: Vec<ReduceIssue>,
}

/// Per-node layout state kept across passes.
#[derive(Debug, Clone, Default)]
struct NodeState {
    flat: Map<String, Value>,
    visual: Visual,
    text: TextStyle,
    control: ControlBox,
    /// Bumped when props change: the memo entry is stale.
    content_version: u32,
}

pub struct Surface {
    pub id: String,
    pub catalog_id: String,
    view: Arc<CatalogView>,
    recipes: Arc<RecipeIndex>,
    extensions: Vec<ExtensionDef>,
    theme: Option<Arc<ResolvedTheme>>,
    mode: Mode,
    components: Vec<FlatComponent>,
    nested: Option<NestedNode>,
    data: Value,
    root: Option<UiNode>,
    pub issues: Vec<ReduceIssue>,
    local: LocalState,
    states: HashMap<String, Vec<String>>,

    nodes: Vec<LNode>,
    layers: Vec<LayerSpec>,
    lists: Vec<ListSpec>,
    node_state: Vec<NodeState>,
    tree: TaffyTree<u32>,
    taffy_ids: Vec<NodeId>,
    memo: MeasureMemo,
    memo_versions: HashMap<u32, u32>,
    last_measurer: Option<u64>,
    viewport: (f32, f32),
    max_height: Option<f32>,
    direction: Direction,
    rounding: bool,
    structure_version: u64,
    needs_build: bool,
    needs_restyle: bool,
    visuals_dirty: HashSet<u32>,
    template_cache: HashMap<String, Option<UiNode>>,
    /// Last placements, so a re-entrant event can find anchor frames.
    last_frames: Vec<Frame>,
    /// Measurements by node id that outlive a rebuild (a windowed list's
    /// rows scrolled out and back in cost nothing twice).
    archive: HashMap<String, Carried>,
}

#[derive(Debug, Clone)]
struct Carried {
    intrinsics: crate::measure::Intrinsics,
    heights: Vec<(i64, f32)>,
    props: Map<String, Value>,
    lines: Option<u32>,
}

// SAFETY: taffy's `CompactLength` carries a `*const ()` slot for `calc()`
// expressions, which makes `Style` `!Send`. This crate never builds a calc
// value (every length is a number/percent/auto), so the slot is always the
// tag variant and the tree is plain owned data. The FFI keeps the surface
// behind a `Mutex`.
unsafe impl Send for Surface {}

impl Surface {
    pub fn new(id: &str, options: SurfaceOptions) -> Surface {
        let view = CatalogView::with(&options.extensions);
        let recipes = if options.extensions.is_empty() { RecipeIndex::core() } else { Arc::new(RecipeIndex::new(&view)) };
        let mut tree: TaffyTree<u32> = TaffyTree::new();
        if options.rounding {
            tree.enable_rounding();
        } else {
            tree.disable_rounding();
        }
        Surface {
            id: id.to_string(),
            catalog_id: options.catalog_id,
            view,
            recipes,
            extensions: options.extensions,
            theme: options.theme,
            mode: options.mode,
            components: Vec::new(),
            nested: None,
            data: Value::Object(Map::new()),
            root: None,
            issues: Vec::new(),
            local: LocalState::default(),
            states: HashMap::new(),
            nodes: Vec::new(),
            layers: Vec::new(),
            lists: Vec::new(),
            node_state: Vec::new(),
            tree,
            taffy_ids: Vec::new(),
            memo: MeasureMemo::default(),
            memo_versions: HashMap::new(),
            last_measurer: None,
            viewport: (0.0, 0.0),
            max_height: None,
            direction: Direction::Ltr,
            rounding: options.rounding,
            structure_version: 0,
            needs_build: true,
            needs_restyle: true,
            visuals_dirty: HashSet::new(),
            template_cache: HashMap::new(),
            last_frames: Vec::new(),
            archive: HashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Configuration
    // ------------------------------------------------------------------

    /// Register an extension catalog (validated). The tree is re-reduced.
    pub fn register_extension(&mut self, ext: ExtensionDef) -> Result<(), String> {
        let ext = crate::extension::define_extension(ext)?;
        let mut all = self.current_extensions();
        all.retain(|e| e.id != ext.id);
        all.push(ext);
        self.view = CatalogView::with(&all);
        self.recipes = Arc::new(RecipeIndex::new(&self.view));
        self.extensions = all;
        self.template_cache.clear();
        self.reduce();
        Ok(())
    }

    fn current_extensions(&self) -> Vec<ExtensionDef> {
        self.extensions.clone()
    }

    pub fn set_theme(&mut self, theme: Option<Arc<ResolvedTheme>>) {
        self.theme = theme;
        self.needs_restyle = true;
        self.needs_build = true;
        self.invalidate_measures();
    }

    pub fn theme(&self) -> Option<&Arc<ResolvedTheme>> {
        self.theme.as_ref()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.needs_restyle = true;
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Android accumulates in dp and rounds once itself; the browser never
    /// rounds. Off = fractional frames.
    pub fn set_rounding(&mut self, on: bool) {
        self.rounding = on;
        if on {
            self.tree.enable_rounding();
        } else {
            self.tree.disable_rounding();
        }
    }

    /// Surface size. Height ≤ 0 = as tall as the content (a scrolling host).
    /// `max_height` bounds a transcript card; `LayoutOutput::overflow` says
    /// when the content is taller. Returns whether anything changed.
    pub fn set_viewport(&mut self, width: f32, height: f32, max_height: Option<f32>) -> bool {
        if self.viewport == (width, height) && self.max_height == max_height {
            return false;
        }
        self.viewport = (width, height);
        self.max_height = max_height;
        self.needs_restyle = true;
        // Windowed lists size their range off the viewport height.
        if !self.lists.is_empty() {
            self.needs_build = true;
        }
        true
    }

    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// Replace a node's interaction states (hover/pressed/focus/disabled…).
    pub fn set_states(&mut self, id: &str, states: Vec<String>) -> bool {
        let current = self.states.get(id);
        if current.is_some_and(|c| *c == states) || (current.is_none() && states.is_empty()) {
            return false;
        }
        if states.is_empty() {
            self.states.remove(id);
        } else {
            self.states.insert(id.to_string(), states);
        }
        self.needs_restyle = true;
        true
    }

    /// The pressed set as a whole (the spike's `set_pressed`).
    pub fn set_pressed(&mut self, ids: &[String]) -> bool {
        let mut changed = false;
        let keep: HashSet<&String> = ids.iter().collect();
        let had: Vec<String> = self.states.iter().filter(|(_, s)| s.iter().any(|x| x == "pressed")).map(|(k, _)| k.clone()).collect();
        for id in had {
            if !keep.contains(&id) {
                let mut s = self.states.get(&id).cloned().unwrap_or_default();
                s.retain(|x| x != "pressed");
                changed |= self.set_states(&id, s);
            }
        }
        for id in ids {
            let mut s = self.states.get(id).cloned().unwrap_or_default();
            if !s.iter().any(|x| x == "pressed") {
                s.push("pressed".into());
                changed |= self.set_states(id, s);
            }
        }
        changed
    }

    /// Forget every cached measurement (a font load, Dynamic Type).
    pub fn invalidate_measures(&mut self) {
        self.memo.clear();
        self.memo_versions.clear();
        self.archive.clear();
        self.mark_all_dirty();
    }

    /// Forget one node's cached measurement.
    pub fn mark_dirty(&mut self, index: u32) -> bool {
        self.memo.forget(index);
        self.memo_versions.remove(&index);
        match self.taffy_ids.get(index as usize) {
            Some(id) => self.tree.mark_dirty(*id).is_ok(),
            None => false,
        }
    }

    fn mark_all_dirty(&mut self) {
        for id in &self.taffy_ids {
            let _ = self.tree.mark_dirty(*id);
        }
    }

    // ------------------------------------------------------------------
    // Content
    // ------------------------------------------------------------------

    /// Apply one A2UI server→client message (`createSurface`,
    /// `updateComponents`, `updateDataModel`, `deleteSurface`).
    pub fn apply(&mut self, message: &Value) -> Result<ApplyOutcome, String> {
        let obj = message.as_object().ok_or("message must be an object")?;
        if let Some(create) = obj.get("createSurface") {
            if let Some(cid) = create.get("catalogId").and_then(Value::as_str) {
                self.catalog_id = cid.to_string();
            }
            if let Some(sid) = create.get("surfaceId").and_then(Value::as_str) {
                self.id = sid.to_string();
            }
            self.components.clear();
            self.nested = None;
            self.data = Value::Object(Map::new());
            self.local = LocalState::default();
            self.template_cache.clear();
            self.reduce();
            return Ok(ApplyOutcome { structure_changed: true, issues: self.issues.clone() });
        }
        if let Some(update) = obj.get("updateComponents") {
            let components = update.get("components").ok_or("updateComponents.components is required")?;
            let list: Vec<FlatComponent> = serde_json::from_value(components.clone()).map_err(|e| format!("components: {e}"))?;
            self.components = list;
            self.nested = None;
            self.template_cache.clear();
            self.reduce();
            return Ok(ApplyOutcome { structure_changed: true, issues: self.issues.clone() });
        }
        if let Some(update) = obj.get("updateDataModel") {
            let path = update.get("path").and_then(Value::as_str).unwrap_or("");
            let value = update.get("value").cloned();
            self.set_data(path, value);
            return Ok(ApplyOutcome { structure_changed: self.needs_build, issues: Vec::new() });
        }
        if obj.get("deleteSurface").is_some() {
            self.components.clear();
            self.nested = None;
            self.root = None;
            self.data = Value::Object(Map::new());
            self.local = LocalState::default();
            self.needs_build = true;
            return Ok(ApplyOutcome { structure_changed: true, issues: Vec::new() });
        }
        Err("unknown message: expected createSurface | updateComponents | updateDataModel | deleteSurface".into())
    }

    /// The nested authoring form (fixtures, MCP templates) instead of a flat
    /// component list.
    pub fn set_nested(&mut self, tree: NestedNode) -> ApplyOutcome {
        self.nested = Some(tree);
        self.components.clear();
        self.template_cache.clear();
        self.reduce();
        ApplyOutcome { structure_changed: true, issues: self.issues.clone() }
    }

    /// A flat component list (what `updateComponents` carries).
    pub fn set_components(&mut self, components: Vec<FlatComponent>) -> ApplyOutcome {
        self.components = components;
        self.nested = None;
        self.template_cache.clear();
        self.reduce();
        ApplyOutcome { structure_changed: true, issues: self.issues.clone() }
    }

    /// Write `value` at `path` (`None` removes). Templates and bindings
    /// re-resolve on the next layout.
    pub fn set_data(&mut self, path: &str, value: Option<Value>) {
        set_pointer(&mut self.data, path, value);
        self.needs_build = true;
    }

    pub fn data(&self) -> &Value {
        &self.data
    }

    pub fn root(&self) -> Option<&UiNode> {
        self.root.as_ref()
    }

    fn reduce(&mut self) {
        let options = ReduceOptions::new(&self.catalog_id).with_view(self.view.clone());
        let result: Option<ReduceResult> = if let Some(nested) = &self.nested {
            Some(crate::reducer::reduce_nested(nested, &options))
        } else if !self.components.is_empty() {
            Some(reduce_surface(&self.components, &options))
        } else {
            None
        };
        match result {
            Some(r) => {
                self.root = Some(r.root);
                self.issues = r.issues;
            }
            None => {
                self.root = None;
                self.issues.clear();
            }
        }
        self.needs_build = true;
    }

    fn template_root(&mut self, component_id: &str) -> Option<UiNode> {
        if let Some(cached) = self.template_cache.get(component_id) {
            return cached.clone();
        }
        let options = ReduceOptions { catalog_id: self.catalog_id.clone(), view: self.view.clone(), root_id: Some(component_id.to_string()), expand: true, validate: false };
        let built = if self.components.iter().any(|c| c.id == component_id) { Some(reduce_surface(&self.components, &options).root) } else { None };
        self.template_cache.insert(component_id.to_string(), built.clone());
        built
    }

    // ------------------------------------------------------------------
    // Layout tree
    // ------------------------------------------------------------------

    fn gap_px(&self, name: &str) -> f32 {
        match &self.theme {
            Some(t) => t.tokens.spacing.get(name).copied().unwrap_or(0.0) as f32,
            None => match name {
                "xxs" => 2.0,
                "xs" => 4.0,
                "sm" => 8.0,
                "md" => 12.0,
                "lg" => 16.0,
                "xl" => 24.0,
                _ => 0.0,
            },
        }
    }

    /// Rebuild the layout node list; keep the taffy tree when the structure
    /// (ids, parents, kinds) is unchanged and only restyle what differs.
    fn rebuild(&mut self) {
        let Some(root) = self.root.clone() else {
            self.nodes.clear();
            self.layers.clear();
            self.lists.clear();
            self.node_state.clear();
            self.tree.clear();
            self.taffy_ids.clear();
            self.needs_build = false;
            self.structure_version += 1;
            return;
        };
        // Templates referenced by the tree are reduced up front so the build
        // closure stays borrow-free.
        let mut template_ids: Vec<String> = Vec::new();
        root.walk(&mut |n| {
            if let Some(t) = &n.template {
                template_ids.push(t.component.clone());
            }
        });
        for id in template_ids {
            self.template_root(&id);
        }
        let templates = self.template_cache.clone();
        let (vw, vh) = self.viewport;
        let window_height = if vh > 0.0 { vh } else { self.max_height.unwrap_or(2000.0) };
        let _ = vw;
        let gaps: HashMap<&str, f32> = ["none", "xxs", "xs", "sm", "md", "lg", "xl", "xl2", "xl3"].iter().map(|g| (*g, self.gap_px(g))).collect();
        let built = {
            let template = |id: &str| -> Option<UiNode> { templates.get(id).cloned().flatten() };
            let gap = |name: &str| -> f32 { gaps.get(name).copied().unwrap_or(0.0) };
            let mut ctx = BuildContext { view: &self.view, recipes: &self.recipes, data: &self.data, local: &mut self.local, template: &template, viewport_height: window_height, gap_px: &gap, expand_controls: self.theme.is_some() };
            layout_tree::build(&root, &mut ctx)
        };
        let same_structure = built.nodes.len() == self.nodes.len()
            && built.nodes.iter().zip(&self.nodes).all(|(a, b)| a.id == b.id && a.parent == b.parent && a.kind == b.kind && a.layer == b.layer && a.component == b.component)
            && built.layers.len() == self.layers.len();
        if same_structure {
            for (i, (new, old)) in built.nodes.iter().zip(&self.nodes).enumerate() {
                if new.props != old.props || new.lines != old.lines {
                    self.node_state[i].content_version += 1;
                    self.memo.forget(i as u32);
                    let _ = self.tree.mark_dirty(self.taffy_ids[i]);
                }
                if new.hidden != old.hidden {
                    let _ = self.tree.mark_dirty(self.taffy_ids[i]);
                }
            }
            self.nodes = built.nodes;
            self.layers = built.layers;
            self.lists = built.lists;
            self.needs_build = false;
            self.needs_restyle = true;
            return;
        }
        // Carry measurements over by id: a windowed list's rows or a
        // re-ordered tree keep their intrinsics when their props are unchanged.
        self.archive_measurements();
        let carried = &self.archive;
        self.nodes = built.nodes;
        self.layers = built.layers;
        self.lists = built.lists;
        self.node_state = vec![NodeState::default(); self.nodes.len()];
        self.tree.clear();
        self.taffy_ids.clear();
        self.memo.clear();
        self.memo_versions.clear();
        for n in &self.nodes {
            if let Some(c) = carried.get(&n.id) {
                if c.props == n.props && c.lines == n.lines {
                    self.memo.intrinsics.insert(n.index, c.intrinsics);
                    for (w, h) in &c.heights {
                        self.memo.heights.insert((n.index, *w), *h);
                    }
                    self.memo_versions.insert(n.index, 0);
                }
            }
        }
        for n in &self.nodes {
            let id = self.tree.new_leaf_with_context(Style::default(), n.index).expect("taffy leaf");
            self.taffy_ids.push(id);
        }
        for n in &self.nodes {
            if !n.children.is_empty() {
                let kids: Vec<NodeId> = n.children.iter().map(|c| self.taffy_ids[*c as usize]).collect();
                self.tree.set_children(self.taffy_ids[n.index as usize], &kids).expect("taffy children");
            }
        }
        self.structure_version += 1;
        self.visuals_dirty = (0..self.nodes.len() as u32).collect();
        self.needs_build = false;
        self.needs_restyle = true;
    }

    /// Every current memo entry into the id-keyed archive.
    fn archive_measurements(&mut self) {
        let mut heights_by_index: HashMap<u32, Vec<(i64, f32)>> = HashMap::new();
        for ((idx, w), h) in &self.memo.heights {
            heights_by_index.entry(*idx).or_default().push((*w, *h));
        }
        for (i, old) in self.nodes.iter().enumerate() {
            if let Some(intr) = self.memo.intrinsics.get(&(i as u32)) {
                self.archive.insert(old.id.clone(), Carried { intrinsics: *intr, heights: heights_by_index.remove(&(i as u32)).unwrap_or_default(), props: old.props.clone(), lines: old.lines });
            }
        }
    }

    fn states_of(&self, n: &LNode) -> Vec<String> {
        let mut states = self.states.get(&n.id).cloned().unwrap_or_default();
        if n.props.get("disabled").and_then(Value::as_bool) == Some(true) && !states.iter().any(|s| s == "disabled") {
            states.push("disabled".into());
        }
        if let Some(q) = &n.part_query {
            for s in &q.states {
                if !states.contains(s) {
                    states.push(s.clone());
                }
            }
        }
        states
    }

    /// The theme-resolved style of a node (tokens → values, conditions nested).
    fn themed_style(&self, n: &LNode, states: &[String]) -> Map<String, Value> {
        let Some(theme) = &self.theme else {
            return n.base_style.clone();
        };
        let mut out = Map::new();
        if let Some(q) = &n.own_query {
            let mut q = q.clone();
            q.states = states.to_vec();
            for (k, v) in resolve_recipe(theme, &q, self.mode) {
                out.insert(k, v);
            }
        }
        for (k, v) in resolve_style_values(theme, &n.base_style, self.mode) {
            out.insert(k, v);
        }
        if let Some(q) = &n.part_query {
            let mut q = q.clone();
            for s in states {
                if !q.states.contains(s) {
                    q.states.push(s.clone());
                }
            }
            for (k, v) in resolve_recipe(theme, &q, self.mode) {
                out.insert(k, v);
            }
        }
        out
    }

    /// Re-resolve every node; `set_style` only where the flat map changed.
    fn restyle(&mut self) {
        let surface_width = self.viewport.0;
        let root_flat = match self.nodes.first() {
            Some(root) => {
                let themed = self.themed_style(root, &[]);
                style::resolve_conditions(&themed, StyleContext { surface_width, pressed: false })
            }
            None => Map::new(),
        };
        let direction = style::direction_of(&root_flat);
        let direction_changed = direction != self.direction;
        self.direction = direction;
        for i in 0..self.nodes.len() {
            let n = &self.nodes[i];
            let states = self.states_of(n);
            let pressed = states.iter().any(|s| s == "pressed");
            let themed = self.themed_style(n, &states);
            let mut flat = style::resolve_conditions(&themed, StyleContext { surface_width, pressed });
            style::resolve_logical(&mut flat, direction);
            if n.hidden {
                flat.insert("display".into(), json!("none"));
            }
            let kind = if n.kind == NodeKind::Leaf { BoxKind::Leaf } else { BoxKind::Container };
            let prev = &self.node_state[i];
            if !direction_changed && prev.flat == flat && !prev.flat.is_empty() {
                continue;
            }
            let taffy_style = match style::to_taffy(&flat, direction, kind) {
                Ok(s) => s,
                Err(e) => {
                    self.issues.push(ReduceIssue { id: n.id.clone(), message: format!("style: {e}") });
                    Style { box_sizing: taffy::style::BoxSizing::BorderBox, direction, ..Style::default() }
                }
            };
            let _ = self.tree.set_style(self.taffy_ids[i], taffy_style);
            let visual = style::visual(&flat, kind);
            let text = TextStyle {
                font_size: visual.font_size.unwrap_or(14.0),
                font_weight: visual.font_weight.unwrap_or(400),
                line_height: visual.line_height.unwrap_or_else(|| (visual.font_size.unwrap_or(14.0) * 1.43).round()),
                font_family: visual.font_family.clone(),
            };
            let control = ControlBox {
                padding_horizontal: visual.padding_horizontal.unwrap_or(0.0),
                padding_vertical: visual.padding_vertical.unwrap_or(0.0),
                border_width: visual.border_width.unwrap_or(0.0),
                gap: visual.gap.unwrap_or(0.0),
                min_width: flat.get("minWidth").and_then(style::px),
                min_height: flat.get("minHeight").and_then(style::px),
                width: flat.get("width").and_then(style::px),
                height: flat.get("height").and_then(style::px),
            };
            let state = &mut self.node_state[i];
            if state.visual != visual {
                self.visuals_dirty.insert(i as u32);
            }
            let first = state.flat.is_empty();
            if !first && (state.text != text || state.control != control) {
                // The measurer's inputs changed: its answers are stale.
                self.memo.forget(i as u32);
                let _ = self.tree.mark_dirty(self.taffy_ids[i]);
            }
            state.flat = flat;
            state.visual = visual;
            state.text = text;
            state.control = control;
        }
        self.needs_restyle = false;
    }

    // ------------------------------------------------------------------
    // Layout
    // ------------------------------------------------------------------

    fn leaf_request(&self, i: usize) -> LeafRequest<'_> {
        let n = &self.nodes[i];
        let s = &self.node_state[i];
        LeafRequest { index: n.index, id: &n.id, component: &n.component, part: n.part.as_deref(), props: &n.props, text_style: &s.text, control: s.control, lines: n.lines }
    }

    fn is_measured_leaf(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        n.kind == NodeKind::Leaf && !n.hidden
    }

    /// Prepare the tree, then run the batched passes. At most 3 upcall
    /// rounds (intrinsics, heights, one correction).
    pub fn layout(&mut self, measure: &mut dyn Measure) -> LayoutOutput {
        let started = Instant::now();
        let measurer = measure.measure_id();
        if self.last_measurer.is_some_and(|m| m != measurer) {
            self.invalidate_measures();
        }
        self.last_measurer = Some(measurer);
        if self.needs_build {
            self.rebuild();
        }
        if self.needs_restyle {
            self.restyle();
        }
        let mut upcalls = 0u32;
        let mut rounds = 0u32;
        let (vw, vh) = self.viewport;
        let bounded_h = if vh > 0.0 { Some(vh) } else { self.max_height };
        let mut placements: Vec<(u32, OverlayPlacement)> = Vec::new();
        loop {
            rounds += 1;
            // Phase 1: intrinsics for every leaf without a memo entry.
            let missing: Vec<usize> = (0..self.nodes.len())
                .filter(|&i| self.is_measured_leaf(i) && (!self.memo.intrinsics.contains_key(&(i as u32)) || self.memo_versions.get(&(i as u32)).copied() != Some(self.node_state[i].content_version)))
                .collect();
            if !missing.is_empty() {
                let requests: Vec<LeafRequest> = missing.iter().map(|&i| self.leaf_request(i)).collect();
                let answers = measure.measure_intrinsics(&requests);
                upcalls += 1;
                for (k, &i) in missing.iter().enumerate() {
                    if let Some(a) = answers.get(k) {
                        self.memo.intrinsics.insert(i as u32, *a);
                        self.memo_versions.insert(i as u32, self.node_state[i].content_version);
                        let _ = self.tree.mark_dirty(self.taffy_ids[i]);
                    }
                }
            }
            // Phase 2: taffy against the memo, guesses recorded.
            let guesses = std::cell::RefCell::new(Vec::<HeightRequest>::new());
            let hidden: Vec<bool> = self.nodes.iter().map(|n| n.hidden).collect();
            let main_available = Size { width: AvailableSpace::Definite(vw.max(0.0)), height: match vh > 0.0 { true => AvailableSpace::Definite(vh), false => AvailableSpace::MaxContent } };
            let memo = &self.memo;
            let mut measure_fn = |known: Size<Option<f32>>, avail: Size<AvailableSpace>, _id: NodeId, ctx: Option<&mut u32>, _style: &Style| -> Size<f32> {
                let Some(index) = ctx.map(|i| *i) else { return Size::ZERO };
                if hidden[index as usize] {
                    return Size::ZERO;
                }
                let req = MeasureRequest { index, known_width: known.width, known_height: known.height, available_width: avail.width, available_height: avail.height };
                let (size, guessed) = memo.answer(&req);
                if guessed {
                    guesses.borrow_mut().push(HeightRequest { index, width: size.width });
                }
                size
            };
            if let Some(root) = self.taffy_ids.first() {
                self.tree.compute_layout_with_measure(*root, main_available, &mut measure_fn).expect("taffy layout");
            }
            // Layers: each root laid out against its own available space.
            placements.clear();
            let layer_specs = self.layers.clone();
            let mut main_frames_cache: Option<Vec<Frame>> = None;
            for spec in &layer_specs {
                let root_id = self.taffy_ids[spec.root as usize];
                let pad = OVERLAY_PADDING as f32;
                let vh_eff = if vh > 0.0 { vh } else { self.max_height.unwrap_or(f32::INFINITY) };
                let available = match spec.placement {
                    LayerPlacement::Centered => Size { width: AvailableSpace::Definite((vw - 2.0 * pad).clamp(0.0, 512.0)), height: AvailableSpace::MaxContent },
                    LayerPlacement::Edge(side) => match side {
                        crate::overlay::OverlaySide::Top | crate::overlay::OverlaySide::Bottom => Size { width: AvailableSpace::Definite(vw), height: AvailableSpace::MaxContent },
                        _ => Size { width: AvailableSpace::Definite((vw - 2.0 * pad).clamp(0.0, 360.0)), height: if vh_eff.is_finite() { AvailableSpace::Definite(vh_eff) } else { AvailableSpace::MaxContent } },
                    },
                    LayerPlacement::Anchored { .. } => Size { width: AvailableSpace::Definite((vw - 2.0 * pad).max(0.0)), height: AvailableSpace::MaxContent },
                };
                // Anchored layers shrink to content: lay out at max-content
                // first, then clamp to the viewport.
                if matches!(spec.placement, LayerPlacement::Anchored { .. }) {
                    self.tree.compute_layout_with_measure(root_id, Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent }, &mut measure_fn).expect("layer layout");
                    let natural = self.tree.layout(root_id).expect("layer").size.width;
                    let clamped = natural.min(vw - 2.0 * pad);
                    if clamped < natural {
                        self.tree.compute_layout_with_measure(root_id, Size { width: AvailableSpace::Definite(clamped.max(0.0)), height: AvailableSpace::MaxContent }, &mut measure_fn).expect("layer layout");
                    }
                } else {
                    self.tree.compute_layout_with_measure(root_id, available, &mut measure_fn).expect("layer layout");
                }
                let size = self.tree.layout(root_id).expect("layer").size;
                let placement = match spec.placement {
                    LayerPlacement::Centered => {
                        let x = ((vw - size.width) / 2.0).max(pad);
                        let y = if vh_eff.is_finite() { ((vh_eff - size.height) / 2.0).max(pad) } else { pad };
                        OverlayPlacement { x: x as f64, y: y as f64, side: crate::overlay::OverlaySide::Top, flipped: false }
                    }
                    LayerPlacement::Edge(side) => {
                        let (x, y) = match side {
                            crate::overlay::OverlaySide::Top => (0.0, 0.0),
                            crate::overlay::OverlaySide::Bottom => (0.0, if vh_eff.is_finite() { vh_eff - size.height } else { 0.0 }),
                            crate::overlay::OverlaySide::Left => (0.0, 0.0),
                            crate::overlay::OverlaySide::Right => (vw - size.width, 0.0),
                        };
                        OverlayPlacement { x: x as f64, y: y as f64, side, flipped: false }
                    }
                    LayerPlacement::Anchored { anchor, side, align } => {
                        let frames = main_frames_cache.get_or_insert_with(|| self.collect_frames(0));
                        let a = frames.get(anchor as usize).copied().unwrap_or_default();
                        place_overlay(
                            &Rect { x: a.x as f64, y: a.y as f64, width: a.w as f64, height: a.h as f64 },
                            &OSize { width: size.width as f64, height: size.height as f64 },
                            &OSize { width: vw as f64, height: if vh_eff.is_finite() { vh_eff as f64 } else { f64::INFINITY } },
                            &PlaceOptions { side, align, ..PlaceOptions::default() },
                        )
                    }
                };
                placements.push((spec.layer, placement));
            }
            let pending = guesses.into_inner();
            if pending.is_empty() || rounds >= 3 {
                break;
            }
            // Phase 3: the heights at the widths taffy decided.
            let mut seen = HashSet::new();
            let requests: Vec<HeightRequest> = pending.into_iter().filter(|r| seen.insert((r.index, MeasureMemo::width_key(r.width)))).collect();
            let leaves: Vec<LeafRequest> = requests.iter().map(|r| self.leaf_request(r.index as usize)).collect();
            let heights = measure.measure_heights(&leaves, &requests);
            upcalls += 1;
            for (k, r) in requests.iter().enumerate() {
                if let Some(h) = heights.get(k) {
                    self.memo.heights.insert((r.index, MeasureMemo::width_key(r.width)), *h);
                    let _ = self.tree.mark_dirty(self.taffy_ids[r.index as usize]);
                }
            }
        }
        let layout_ns = started.elapsed().as_nanos() as u64;
        self.output(placements, bounded_h, rounds, upcalls, layout_ns)
    }

    /// Frames of every node of a layer by index (absolute within the layer).
    fn collect_frames(&self, layer: u32) -> Vec<Frame> {
        let mut out = vec![Frame::default(); self.nodes.len()];
        for n in &self.nodes {
            if n.layer != layer || n.parent.is_some() {
                continue;
            }
            self.walk_frames(n.index as usize, 0.0, 0.0, &mut out);
        }
        out
    }

    fn walk_frames(&self, index: usize, ox: f32, oy: f32, out: &mut [Frame]) {
        let n = &self.nodes[index];
        let l = self.tree.layout(self.taffy_ids[index]).expect("layout");
        let x = ox + l.location.x;
        let y = oy + l.location.y;
        out[index] = Frame { x, y, w: l.size.width, h: l.size.height };
        for child in &n.children {
            self.walk_frames(*child as usize, x, y, out);
        }
    }

    fn output(&mut self, placements: Vec<(u32, OverlayPlacement)>, bounded_h: Option<f32>, rounds: u32, upcalls: u32, layout_ns: u64) -> LayoutOutput {
        let frames = self.collect_frames(0);
        let mut main = Vec::with_capacity(self.nodes.len());
        let mut order = Vec::new();
        if !self.nodes.is_empty() {
            self.preorder(0, &mut order);
        }
        for &i in &order {
            let f = frames[i];
            main.push(PlacedFrame { index: i as u32, x: f.x, y: f.y, w: f.w, h: f.h });
        }
        let mut all_frames = frames.clone();
        let mut layers = Vec::new();
        for spec in self.layers.clone() {
            let placement = placements.iter().find(|(l, _)| *l == spec.layer).map(|(_, p)| *p);
            let (dx, dy) = placement.map(|p| (p.x as f32, p.y as f32)).unwrap_or((0.0, 0.0));
            let local = self.collect_frames(spec.layer);
            let mut order = Vec::new();
            self.preorder(spec.root as usize, &mut order);
            let mut lf = Vec::with_capacity(order.len());
            for &i in &order {
                let f = local[i];
                let abs = Frame { x: f.x + dx, y: f.y + dy, w: f.w, h: f.h };
                all_frames[i] = abs;
                lf.push(PlacedFrame { index: i as u32, x: abs.x, y: abs.y, w: abs.w, h: abs.h });
            }
            let anchor_frame = match spec.placement {
                LayerPlacement::Anchored { anchor, .. } => Some(all_frames[anchor as usize]),
                _ => None,
            };
            let position = match spec.placement {
                LayerPlacement::Centered => "centered".to_string(),
                LayerPlacement::Edge(side) => side.as_str().to_string(),
                LayerPlacement::Anchored { .. } => placement.map(|p| p.side.as_str().to_string()).unwrap_or_default(),
            };
            layers.push(Layer { layer: spec.layer, kind: spec.kind.clone(), owner: spec.owner.clone(), root: spec.root, anchor_frame, placement: if matches!(spec.placement, LayerPlacement::Anchored { .. }) { placement } else { None }, position, frames: lf });
        }
        // Windowed lists: remember the rows' real heights.
        let mut lists = Vec::new();
        for spec in self.lists.clone() {
            let window = self.local.lists.entry(spec.id.clone()).or_insert_with(|| crate::list::ListWindow::new(crate::list::DEFAULT_ESTIMATED_ITEM_HEIGHT, crate::list::DEFAULT_OVERSCAN, 0.0));
            let mut changed = false;
            for child in &self.nodes[spec.node as usize].children {
                let c = &self.nodes[*child as usize];
                if c.part.as_deref() == Some("divider") {
                    continue;
                }
                changed |= window.record(&c.id, frames[*child as usize].h);
            }
            if changed && spec.windowed {
                self.needs_build = true;
            }
            let content_height = window.content_height(spec.keys.iter().map(String::as_str));
            lists.push(ListOutput { id: spec.id.clone(), node: spec.node, content_height, start: spec.range.start as u32, end: spec.range.end as u32, count: spec.keys.len() as u32, windowed: spec.windowed });
        }
        self.last_frames = all_frames;
        let root_frame = frames.first().copied().unwrap_or_default();
        let mut visual_changes: Vec<u32> = self.visuals_dirty.drain().collect();
        visual_changes.sort_unstable();
        LayoutOutput {
            frames: main,
            layers,
            lists,
            visual_changes,
            structure_version: self.structure_version,
            surface_width: root_frame.w,
            surface_height: root_frame.h,
            overflow: bounded_h.is_some_and(|h| root_frame.h > h + 0.5),
            measure_rounds: rounds,
            upcalls,
            layout_ns,
        }
    }

    fn preorder(&self, index: usize, out: &mut Vec<usize>) {
        out.push(index);
        for c in &self.nodes[index].children {
            self.preorder(*c as usize, out);
        }
    }

    // ------------------------------------------------------------------
    // Snapshots
    // ------------------------------------------------------------------

    pub fn structure_version(&self) -> u64 {
        self.structure_version
    }

    /// Every layout node, once per structure version.
    pub fn nodes(&mut self) -> Vec<PlacedNode> {
        if self.needs_build {
            self.rebuild();
        }
        self.nodes
            .iter()
            .map(|n| PlacedNode {
                index: n.index,
                id: n.id.clone(),
                component: n.component.clone(),
                part: n.part.clone(),
                owner: n.owner.clone(),
                owner_component: n.owner_component.clone(),
                catalog_id: n.catalog_id.clone(),
                extension_kind: n.extension_kind.clone(),
                depth: n.depth,
                parent: n.parent,
                layer: n.layer,
                kind: n.kind,
                props: n.props.clone(),
                lines: n.lines,
                pressable: n.pressable,
                hidden: n.hidden,
                accessibility: n.accessibility.clone(),
                trigger_for: n.trigger_for.clone(),
            })
            .collect()
    }

    pub fn node_count(&mut self) -> usize {
        if self.needs_build {
            self.rebuild();
        }
        self.nodes.len()
    }

    /// The resolved visual of one node (after a layout pass).
    pub fn visual(&self, index: u32) -> Option<&Visual> {
        self.node_state.get(index as usize).map(|s| &s.visual)
    }

    pub fn visuals(&self) -> Vec<Visual> {
        self.node_state.iter().map(|s| s.visual.clone()).collect()
    }

    pub fn text_style(&self, index: u32) -> Option<&TextStyle> {
        self.node_state.get(index as usize).map(|s| &s.text)
    }

    pub fn layout_node(&self, index: u32) -> Option<&LNode> {
        self.nodes.get(index as usize)
    }

    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.nodes.iter().find(|n| n.id == id).map(|n| n.index)
    }

    pub fn last_frame(&self, index: u32) -> Option<Frame> {
        self.last_frames.get(index as usize).copied()
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    // ------------------------------------------------------------------
    // Local UI state + events
    // ------------------------------------------------------------------

    /// A list scrolled (content offset in px).
    pub fn scroll(&mut self, list_id: &str, offset: f32) -> bool {
        let Some(w) = self.local.lists.get_mut(list_id) else { return false };
        if (w.scroll_offset - offset).abs() < 0.5 {
            return false;
        }
        w.scroll_offset = offset;
        self.needs_build = true;
        true
    }

    /// Open or close an overlay (Dialog, Drawer, Popover, Tooltip, DropdownMenu).
    pub fn set_open(&mut self, id: &str, open: bool) -> Vec<OutEvent> {
        self.local.open.insert(id.to_string(), open);
        self.needs_build = true;
        let mut out = vec![];
        out.extend(self.write_through(id, "open", Value::Bool(open)));
        out.extend(self.fire(id, "change", Some(json!({"open": open}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn source_node(&self, id: &str) -> Option<UiNode> {
        let root = self.root.as_ref()?;
        let mut found = None;
        root.walk(&mut |n| {
            if found.is_none() && n.id == id {
                found = Some(n.clone());
            }
        });
        found
    }

    /// If `prop` on `id` is bound, write `value` to the data model.
    fn write_through(&mut self, id: &str, prop: &str, value: Value) -> Vec<OutEvent> {
        let Some(node) = self.source_node(id) else { return vec![] };
        let Some(binding) = node.props.get(prop).filter(|v| crate::data::is_binding(v)) else { return vec![] };
        let scope = self.nodes.iter().find(|n| n.id == id).map(|n| n.scope.clone()).unwrap_or_default();
        let path = absolute_path(binding["path"].as_str().unwrap_or(""), &scope);
        set_pointer(&mut self.data, &path, Some(value.clone()));
        self.needs_build = true;
        vec![OutEvent::DataChanged { path, value }]
    }

    /// Fire a node's `on.<event>` handler.
    fn fire(&mut self, id: &str, event: &str, payload: Option<Value>) -> Vec<OutEvent> {
        let Some(node) = self.source_node(id) else { return vec![] };
        let Some(action) = node.on.as_ref().and_then(|o| o.get(event)).cloned() else { return vec![] };
        let scope = self.nodes.iter().find(|n| n.id == id).map(|n| n.scope.clone()).unwrap_or_default();
        let ctx = ResolveContext { data: &self.data, scope: &scope };
        let mut out = vec![];
        if let Some(ev) = action.get("event") {
            let name = ev.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let context = ev.get("context").map(|c| crate::data::resolve_value(c, &ctx).unwrap_or(Value::Null)).unwrap_or_else(|| Value::Object(Map::new()));
            out.push(OutEvent::Action { name, component_id: id.to_string(), event: event.to_string(), context, payload });
        }
        if let Some(f) = action.get("function") {
            let resolved = crate::data::resolve_value(f, &ctx);
            if f.get("call").and_then(Value::as_str) == Some("openUrl") {
                if let Some(Value::String(url)) = resolved {
                    out.push(OutEvent::OpenUrl { url });
                }
            }
        }
        out
    }

    /// An interaction on layout node `index`: `press`, `change` (with a
    /// payload such as `{value}` / `{open}` / `{page}`), `select`, `submit`,
    /// `commit`. Local UI state (tabs, accordion, carousel, overlays) is
    /// updated here; bound props write through; `on` handlers fire.
    pub fn event(&mut self, index: u32, event: &str, payload: Option<Value>) -> Vec<OutEvent> {
        let Some(n) = self.nodes.get(index as usize).cloned() else { return vec![] };
        if event == "press" {
            if let Some(target) = n.trigger_for.clone() {
                let mut out = self.fire(&n.id, "press", None);
                let open = !self.local.open.get(&target).copied().unwrap_or_else(|| self.nodes.iter().find(|x| x.id == target).and_then(|o| o.props.get("open").and_then(Value::as_bool)).unwrap_or(false));
                out.extend(self.set_open(&target, open));
                return out;
            }
        }
        let owner_id = n.owner.clone().unwrap_or_else(|| n.id.clone());
        let owner_component = n.owner_component.clone().unwrap_or_else(|| n.component.clone());
        let mut out = vec![];
        match (owner_component.as_str(), n.part.as_deref(), event) {
            ("Tabs", Some("tab"), "press") => {
                let i = n.id.rsplit('.').next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                let owner = self.nodes.iter().find(|x| x.id == owner_id).cloned();
                let value = owner.and_then(|o| o.props.get("tabs").and_then(Value::as_array).and_then(|t| t.get(i)).and_then(|t| t.get("value")).map(crate::json::to_js_string)).unwrap_or_default();
                self.local.tabs.insert(owner_id.clone(), value.clone());
                self.needs_build = true;
                out.extend(self.write_through(&owner_id, "value", Value::String(value.clone())));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": value}))));
                out.push(OutEvent::Relayout);
            }
            ("Accordion", Some("trigger"), "press") => {
                let i = n.id.rsplit('.').next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                let owner = self.nodes.iter().find(|x| x.id == owner_id).cloned();
                let Some(owner) = owner else { return out };
                let value = owner.props.get("items").and_then(Value::as_array).and_then(|t| t.get(i)).and_then(|t| t.get("value")).map(crate::json::to_js_string).unwrap_or_default();
                let multiple = owner.props.get("type").and_then(Value::as_str) == Some("multiple");
                let mut open = self.local.accordion.get(&owner_id).cloned().unwrap_or_else(|| owner.props.get("value").map(crate::json::to_js_string).map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default());
                if open.contains(&value) {
                    open.retain(|v| *v != value);
                } else if multiple {
                    open.push(value.clone());
                } else {
                    open = vec![value.clone()];
                }
                let joined = open.join(",");
                self.local.accordion.insert(owner_id.clone(), open);
                self.needs_build = true;
                out.extend(self.write_through(&owner_id, "value", Value::String(joined.clone())));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": joined}))));
                out.push(OutEvent::Relayout);
            }
            ("Carousel", _, "change") => {
                let page = payload.as_ref().and_then(|p| p.get("page")).and_then(Value::as_i64).unwrap_or(0);
                self.local.carousel.insert(owner_id.clone(), page);
                self.needs_build = true;
                out.extend(self.write_through(&owner_id, "page", json!(page)));
                out.extend(self.fire(&owner_id, "change", Some(json!({"page": page}))));
                out.push(OutEvent::Relayout);
            }
            ("Dialog" | "Drawer" | "Popover" | "Tooltip" | "DropdownMenu", part, "press") if part.is_some() || n.id == owner_id => {
                if part == Some("item") {
                    let value = n.props.get("value").cloned().unwrap_or(Value::Null);
                    out.extend(self.fire(&owner_id, "select", Some(json!({"value": value}))));
                    out.extend(self.set_open(&owner_id, false));
                } else if part == Some("close") {
                    out.extend(self.set_open(&owner_id, false));
                } else {
                    let open = !self.local.open.get(&owner_id).copied().unwrap_or_else(|| self.nodes.iter().find(|x| x.id == owner_id).and_then(|o| o.props.get("open").and_then(Value::as_bool)).unwrap_or(false));
                    out.extend(self.set_open(&owner_id, open));
                }
            }
            ("Dialog" | "Drawer" | "Popover" | "Tooltip" | "DropdownMenu", _, "change") => {
                let open = payload.as_ref().and_then(|p| p.get("open")).and_then(Value::as_bool).unwrap_or(false);
                out.extend(self.set_open(&owner_id, open));
            }
            ("Checkbox" | "Switch", _, "press" | "change") => {
                let current = self.nodes.iter().find(|x| x.id == owner_id).and_then(|o| o.props.get("checked").and_then(Value::as_bool)).unwrap_or(false);
                let checked = payload.as_ref().and_then(|p| p.get("checked")).and_then(Value::as_bool).unwrap_or(!current);
                out.extend(self.write_through(&owner_id, "checked", Value::Bool(checked)));
                out.extend(self.fire(&owner_id, "change", Some(json!({"checked": checked}))));
                out.push(OutEvent::Relayout);
            }
            ("Radio", Some("item" | "dot" | "label"), "press") => {
                // The option index is the id suffix (`<owner>.dot.2`); the
                // ROW (`<owner>.item.2`) carries the option's value.
                let suffix = n.id.rsplit('.').next().unwrap_or_default();
                let row_id = format!("{owner_id}.item.{suffix}");
                let value = self.nodes.iter().find(|x| x.id == row_id).and_then(|row| row.props.get("value").cloned()).unwrap_or(Value::Null);
                out.extend(self.write_through(&owner_id, "value", value.clone()));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": value}))));
                out.push(OutEvent::Relayout);
            }
            ("Toggle", _, "press") => {
                let current = self.local.pressed_toggles.get(&owner_id).copied().unwrap_or_else(|| n.props.get("pressed").and_then(Value::as_bool).unwrap_or(false));
                self.local.pressed_toggles.insert(owner_id.clone(), !current);
                out.extend(self.write_through(&owner_id, "pressed", Value::Bool(!current)));
                out.extend(self.fire(&owner_id, "change", Some(json!({"pressed": !current}))));
                out.push(OutEvent::Relayout);
            }
            ("Link", _, "press") => {
                if let Some(url) = n.props.get("href").and_then(Value::as_str) {
                    out.push(OutEvent::OpenUrl { url: url.to_string() });
                }
                out.extend(self.fire(&owner_id, "press", None));
            }
            ("Input" | "Textarea" | "Select" | "DatePicker" | "Slider" | "Composer" | "ToggleGroup", _, "change" | "commit" | "submit") => {
                let value = payload.as_ref().and_then(|p| p.get("value")).cloned().unwrap_or(Value::Null);
                let name = n.props.get("name").or_else(|| self.nodes.iter().find(|x| x.id == owner_id).and_then(|o| o.props.get("name"))).and_then(Value::as_str).unwrap_or(&owner_id).to_string();
                let path = self.source_node(&owner_id).and_then(|s| s.props.get("value").filter(|v| crate::data::is_binding(v)).map(|b| absolute_path(b["path"].as_str().unwrap_or(""), &n.scope)));
                if event != "submit" {
                    out.extend(self.write_through(&owner_id, "value", value.clone()));
                }
                out.push(OutEvent::Input { component_id: owner_id.clone(), name, path, value: value.clone(), commit: event != "change" });
                out.extend(self.fire(&owner_id, event, Some(json!({"value": value}))));
            }
            _ => {
                out.extend(self.fire(&owner_id, event, payload));
            }
        }
        out
    }

    /// Validation `checks` of a field evaluated against the data model: the
    /// messages of the failing ones.
    pub fn failing_checks(&self, id: &str) -> Vec<String> {
        let Some(node) = self.source_node(id) else { return vec![] };
        let scope = self.nodes.iter().find(|n| n.id == id).map(|n| n.scope.clone()).unwrap_or_default();
        let ctx = ResolveContext { data: &self.data, scope: &scope };
        node.props
            .get("checks")
            .and_then(Value::as_array)
            .map(|checks| {
                checks
                    .iter()
                    .filter(|c| {
                        let ok = c.get("condition").and_then(|cond| crate::data::resolve_value(cond, &ctx)).and_then(|v| v.as_bool()).unwrap_or(true);
                        !ok
                    })
                    .filter_map(|c| c.get("message").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Is this surface a basic-catalog one?
    pub fn is_basic(&self) -> bool {
        self.catalog_id == A2UI_BASIC_CATALOG_ID
    }

    pub fn get_data(&self, path: &str) -> Option<&Value> {
        get_pointer(&self.data, path)
    }
}
