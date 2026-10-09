//! A `Surface` = one A2UI surface: its catalog, its normalized tree, its
//! data model, its theme and settings, the local UI state the core owns,
//! and one layout engine (main root + one root per open layer) kept across
//! passes.
//!
//! Round 1 (renderer hardening):
//! - nodes live in STABLE SLOTS keyed by id: a rebuild (a data write, a tab
//!   switch, an opened overlay, a scrolled window) reconciles by id, keeps
//!   every unchanged node's style, visual and measurement, and reports the
//!   difference as a [`NodeDelta`] (`added` / `removed` / `changed`);
//! - restyle is PER NODE: a state change restyles that node, a viewport
//!   change only the nodes with conditions, a theme/mode/settings change
//!   every node;
//! - settings: locale + strings, mode `light|dark|system`, density,
//!   contrast, font scale, safe-area insets, pointer hover, reduced motion
//!   ([`SurfaceSettings`]).
//!
//! The API is COARSE by design (the FFI facade mirrors it one to one):
//! messages in (`apply`), viewport/state setters, `layout(measure)` out —
//! never a call per node or per keystroke.

mod events;
mod pass;
mod reconcile;
mod restyle;

pub use pass::{LayoutStep, LeafData};

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use taffy::style::Direction;

use crate::catalog::{CatalogView, A2UI_BASIC_CATALOG_ID};
use crate::data::{get_pointer, set_pointer};
use crate::engine::Engine;
use crate::layout_tree::{LNode, LayerClass, LayerSpec, ListSpec, LocalState, NodeKind, ToastSpec};
use crate::measure::{ControlBox, MeasureMemo, TextStyle};
use crate::overlay::OverlayPlacement;
use crate::recipes::RecipeIndex;
use crate::reducer::{reduce_surface, ReduceOptions, ReduceResult};
use crate::strings::StringTable;
use crate::style::Visual;
use crate::theme::{apply_contrast, apply_density, resolve_mode, Density, Mode, ModeSetting, ResolvedTheme};
use crate::types::{ExtensionDef, FlatComponent, NestedNode, ReduceIssue, UiNode};

pub use crate::layout_tree::ToastSpec as Toast;

/// How long a hover overlay (Tooltip, `openOn: hover` Popover) waits after
/// the pointer left its trigger and content before it closes (the
/// reference's `HOVER_CLOSE_MS`): time to cross the gap into the content.
pub const HOVER_CLOSE_MS: u32 = 150;

/// Absolute frame in surface coordinates (points/dp; fractional unless
/// rounding is on).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The static description of one layout node (`Surface::nodes`): fetched
/// once, then patched with [`NodeDelta`]. Indices are STABLE slots; paint
/// order = accessibility order = the order of `LayoutOutput.frames`.
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
    /// Children in paint order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<u32>,
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
    /// The part states the core decided (`selected`, `open`, `checked`,
    /// `invalid`, `disabled`); the host adds `hover`/`pressed`/`focus`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<String>,
    /// The enclosing Form's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    /// A live region: `polite | assertive`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<String>,
    /// A freed slot (the node left the tree; a later node may reuse it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
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
    /// Drawer: the viewport edge; Dialog: `centered`; anchored: the side it
    /// landed on; a context menu: `point`; a toast: `toast`.
    pub position: String,
    /// `overlay` or `toast` (base < overlay < toast; no z-index).
    pub class: LayerClass,
    /// A scrim under it and the focus trapped inside (Dialog, Drawer).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub modal: bool,
    /// Escape, a scrim press or a drag closes it (`event(root, "dismiss")`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dismissible: bool,
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
    /// Round 2: windows on x (`content_height` is then the content width).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub horizontal: bool,
}

/// Round 2 (§2, §5): a node pinned by `position: sticky` (or a List's
/// pinned section header): paint it (and its subtree) translated by
/// `(dx, dy)` on top of its frame, inside the same scroll translation as
/// its siblings. Only nodes that move are listed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StickyOutput {
    pub index: u32,
    pub dx: f32,
    pub dy: f32,
}

/// One scroll container (any `overflow: scroll|auto` node, windowed lists,
/// a Dialog body): its offset (clamped), its viewport (the node's frame) and
/// its scrollable content size. Frames are reported UNSCROLLED: a painter
/// translates every descendant by `-offset` and clips to the frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScrollOutput {
    pub index: u32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub scroll_x: bool,
    pub scroll_y: bool,
}

/// What changed in `nodes()` since the previous `layout()`: `added` and
/// `changed` slots to fetch again (`nodes_at`), `removed` slots to drop.
/// `renumbered` = every index moved (compaction, a new surface): fetch all.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeDelta {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
    pub changed: Vec<u32>,
    pub renumbered: bool,
}

impl NodeDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty() && !self.renumbered
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutOutput {
    /// Main-tree frames, pre-order (= paint order).
    pub frames: Vec<PlacedFrame>,
    pub layers: Vec<Layer>,
    pub lists: Vec<ListOutput>,
    pub scrolls: Vec<ScrollOutput>,
    /// Round 2: sticky nodes and pinned section headers at their pinned
    /// paint offsets this pass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sticky: Vec<StickyOutput>,
    /// Open toasts the host times (`duration_ms` 0 = sticky); a timeout is
    /// `event(<toast root>, "dismiss")` / `dismiss_toast(id)`.
    pub toasts: Vec<ToastSpec>,
    /// Slots whose resolved visual changed since the last pass.
    pub visual_changes: Vec<u32>,
    /// The node changes since the last pass (see [`NodeDelta`]).
    pub delta: NodeDelta,
    /// Bumps whenever the node set changed (added/removed/renumbered).
    pub structure_version: u64,
    pub surface_width: f32,
    pub surface_height: f32,
    /// Content taller than the bounded height (a transcript card may scroll).
    pub overflow: bool,
    /// `ltr` | `rtl` (the root's `direction`, else the locale's).
    pub direction: String,
    /// The active breakpoint (`None` = base).
    pub breakpoint: Option<String>,
    pub measure_rounds: u32,
    pub upcalls: u32,
    /// Nodes restyled this pass (a hover = 1; a theme switch = all).
    pub restyled: u32,
    /// The layout tree was rebuilt (and reconciled) this pass.
    pub rebuilt: bool,
    /// Layout nodes the builder produced this pass: the whole tree on a
    /// rebuild, only a re-windowed list's subtree on a scroll.
    pub built_nodes: u32,
    pub layout_ns: u64,
}

/// What the host must do after an interaction (`non_exhaustive`: match with
/// a wildcard arm; new kinds are additive).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[non_exhaustive]
pub enum OutEvent {
    /// A server event (A2UI `action`): forward it. `context` = the author's
    /// context with the event payload merged over it.
    Action { name: String, component_id: String, event: String, context: Value, #[serde(skip_serializing_if = "Option::is_none")] payload: Option<Value> },
    /// `openUrl` or a `Link`.
    OpenUrl { url: String },
    /// An `on.<event>` `functionCall` (or the legacy `function`) naming a
    /// HOST function (not one of the catalog's built-ins, `FUNCTION_NAMES`):
    /// the host looks it up in its registry and runs it through the policy
    /// gate (`host::decide_function`, VAPP-91). `args` = the call's args
    /// resolved against the data model and scope. Serialized `{kind:
    /// "functionCall", componentId, name, args}` (the host API's camelCase;
    /// the older variants keep `component_id`).
    FunctionCall {
        #[serde(rename = "componentId")]
        component_id: String,
        name: String,
        args: Value,
    },
    /// A bound value was written through to the data model.
    DataChanged { path: String, value: Value },
    /// A host-owned input edit (the host forwards with its revision).
    Input { component_id: String, name: String, #[serde(skip_serializing_if = "Option::is_none")] path: Option<String>, value: Value, commit: bool },
    /// Move keyboard focus (a Form's first invalid field, `focus` command).
    Focus { id: String, index: u32 },
    /// Speak through the platform's live region.
    Announce { text: String, live: String },
    /// Put text on the clipboard (CodeBlock copy).
    Copy { text: String },
    /// Open the platform file picker for a FileUpload; the picked files come
    /// back as `event(<dropzone>, "upload", {files})` (bytes via the host).
    PickFiles { component_id: String, #[serde(skip_serializing_if = "Option::is_none")] accept: Option<String>, multiple: bool },
    /// Local UI state changed: lay out again.
    Relayout,
    /// Call [`Surface::hover_timeout`]`(owner)` after `delay_ms` (a hover
    /// overlay whose trigger and content were left; it closes then unless
    /// the pointer came back). Keep ONE timer per owner: a new `HoverTimer`
    /// for the same owner restarts it.
    HoverTimer { owner: String, delay_ms: u32 },
    /// Round 2: scroll the host viewport of the whole surface to `(x, y)`
    /// (a `scrollToIndex` on a list the page scrolls), then report it with
    /// [`Surface::set_surface_scroll`].
    ScrollSurface { x: f32, y: f32 },
}

/// What a host may ask of a live surface (`catalog/a11y.json` `commands`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceCommand {
    Focus { id: String },
    Announce { text: String, #[serde(default)] live: Option<String> },
    ScrollIntoView { id: String },
    /// Round 2: `scrollToIndex {id, index, align?}` (`start | center | end |
    /// nearest`, default nearest).
    ScrollToIndex {
        id: String,
        index: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<String>,
    },
}

/// Safe-area insets (status bar, notch, home indicator) layers keep clear of.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// `normal | high | system` (system = the platform's high-contrast flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContrastSetting {
    Normal,
    High,
    #[default]
    System,
}

/// What a host sets per surface (docs/round-1-contract.md §4–6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SurfaceSettings {
    /// BCP 47; the week start and the text direction. Default `en-US`.
    pub locale: String,
    /// Built-in string overrides by id.
    pub strings: IndexMap<String, String>,
    pub mode: ModeSetting,
    /// The platform prefers dark (`mode: system`).
    pub system_dark: bool,
    pub density: Density,
    pub contrast: ContrastSetting,
    /// The platform asks for more contrast (`contrast: system`).
    pub system_high_contrast: bool,
    /// Dynamic Type / font scale, applied to every resolved type size.
    pub font_scale: f32,
    /// A hover-capable pointer (`@media (hover: hover)`, hover popovers).
    pub hover: bool,
    /// `@media (prefers-reduced-motion: reduce)`; transitions become 0 ms.
    pub reduced_motion: bool,
    pub insets: Insets,
    /// Today as `yyyy-mm-dd` (calendars open on it and mark it).
    pub today: Option<String>,
    /// How a hover overlay (Tooltip, `openOn: hover` Popover) closes when
    /// the pointer left its trigger and its content. `0` (default) = at
    /// once: the HOST delays forwarding the trigger's un-hover (gpui's
    /// grace). `> 0` (natives: [`HOVER_CLOSE_MS`]) = the core holds it open
    /// and raises [`OutEvent::HoverTimer`]; the host calls
    /// [`Surface::hover_timeout`] when it fires.
    pub hover_close_ms: u32,
}

impl Default for SurfaceSettings {
    fn default() -> Self {
        SurfaceSettings {
            locale: crate::locale::DEFAULT_LOCALE.to_string(),
            strings: IndexMap::new(),
            mode: ModeSetting::Light,
            system_dark: false,
            density: Density::Default,
            contrast: ContrastSetting::Normal,
            system_high_contrast: false,
            font_scale: 1.0,
            hover: true,
            reduced_motion: false,
            insets: Insets::default(),
            today: None,
            hover_close_ms: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceOptions {
    pub catalog_id: String,
    /// `None` = no theme: geometry mode (no recipes, styles verbatim).
    pub theme: Option<Arc<ResolvedTheme>>,
    pub mode: Mode,
    pub extensions: Vec<ExtensionDef>,
    pub rounding: bool,
    /// Expand form controls into their parts (default: when themed). Off =
    /// geometry mode: every native is ONE measured leaf.
    pub expand_controls: Option<bool>,
}

impl Default for SurfaceOptions {
    fn default() -> Self {
        SurfaceOptions {
            catalog_id: crate::catalog::CORE_CATALOG_ID.to_string(),
            theme: Some(crate::themes::default_theme()),
            mode: Mode::Light,
            extensions: Vec::new(),
            rounding: false,
            expand_controls: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApplyOutcome {
    pub structure_changed: bool,
    /// The reduce issues of a whole tree (`set_nested`, `set_components`)
    /// or this message's own refusal (a data write). VAPP-103: a streamed
    /// `updateComponents` reduces LAZILY (a surface sent one component
    /// per message costs linear, not quadratic): its issues are
    /// [`Surface::issues`] after it.
    pub issues: Vec<ReduceIssue>,
}

/// `%` radii a node asked for (resolved against its laid-out box).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct RadiusPercent {
    all: Option<f32>,
    corners: [Option<f32>; 4],
}

/// Per-slot layout state kept across passes.
#[derive(Debug, Clone, Default)]
struct NodeState {
    flat: Map<String, Value>,
    visual: Visual,
    text: TextStyle,
    control: ControlBox,
    /// Bumped when props change: the memo entry is stale.
    content_version: u32,
    /// The themed style has `@media` blocks: a viewport change restyles it.
    conditional: bool,
    radius_pct: Option<RadiusPercent>,
    /// The px radii last resolved from `radius_pct` (a box change re-resolves).
    styled: bool,
    /// The node's resolved (inherited) direction.
    direction: Direction,
    /// Its line height is a px value set here or inherited (else `normal`).
    line_height_set: bool,
}

#[derive(Debug, Clone)]
struct Carried {
    intrinsics: crate::measure::Intrinsics,
    heights: Vec<(i64, f32)>,
    props: Map<String, Value>,
    lines: Option<u32>,
}

pub struct Surface {
    pub id: String,
    pub catalog_id: String,
    view: Arc<CatalogView>,
    recipes: Arc<RecipeIndex>,
    extensions: Vec<ExtensionDef>,
    /// The theme as set; `effective` = density + contrast applied.
    theme: Option<Arc<ResolvedTheme>>,
    effective: Option<Arc<ResolvedTheme>>,
    settings: SurfaceSettings,
    strings: StringTable,
    mode: Mode,
    expand_controls: Option<bool>,
    components: Vec<FlatComponent>,
    nested: Option<NestedNode>,
    data: Value,
    root: Option<UiNode>,
    pub(crate) issues: Vec<ReduceIssue>,
    local: LocalState,
    states: HashMap<String, Vec<String>>,

    /// Slots (dead ones have `live[i] == false`).
    nodes: Vec<LNode>,
    live: Vec<bool>,
    slot_of: HashMap<String, u32>,
    free: Vec<u32>,
    main_root: Option<u32>,
    layers: Vec<LayerSpec>,
    lists: Vec<ListSpec>,
    toasts: Vec<ToastSpec>,
    node_state: Vec<NodeState>,
    engine: Engine,
    memo: MeasureMemo,
    memo_versions: HashMap<u32, u32>,
    last_measurer: Option<u64>,
    viewport: (f32, f32),
    max_height: Option<f32>,
    direction: Direction,
    rounding: bool,
    structure_version: u64,
    needs_build: bool,
    restyle_all: bool,
    style_dirty: BTreeSet<u32>,
    visuals_dirty: HashSet<u32>,
    /// The LIFTED templates of the last reduce (`ReduceResult::templates`,
    /// round 2), by component id.
    template_cache: HashMap<String, Option<UiNode>>,
    /// VAPP-103: `components` changed since the last reduce (a streamed
    /// `updateComponents` reduces on the next read, never per message).
    reduce_pending: bool,
    /// `components` positions by id (an update replaces BY ID in O(1)).
    component_index: HashMap<String, usize>,
    /// Last placements by slot, so an event can find anchor frames.
    last_frames: Vec<Frame>,
    /// Measurements by node id that outlive a slot (a windowed list's rows
    /// scrolled out and back in cost nothing twice).
    archive: HashMap<String, Carried>,
    responsive: bool,
    breakpoint: Option<String>,
    delta_added: BTreeSet<u32>,
    delta_removed: BTreeSet<u32>,
    delta_changed: BTreeSet<u32>,
    renumbered: bool,
    pending: Vec<OutEvent>,
    scrolls: Vec<ScrollOutput>,
    /// Counters for the current pass (`LayoutOutput::restyled`/`rebuilt`).
    restyled: u32,
    rebuilt: bool,
    built_nodes: u32,
    /// A stepped layout pass in progress (`layout_begin`).
    step_state: Option<pass::PassState>,
    /// Nodes whose subtree alone rebuilds next pass (a moved window, local
    /// tab/accordion/carousel/toggle state).
    subtree_pending: BTreeSet<String>,
    /// Hover overlays waiting for their close timer.
    hover_closing: HashSet<String>,
    /// What the last build knew that events and subtree rebuilds need: the
    /// literal Table rows (data scopes) and the Forms' disabled/busy flags.
    seed: crate::layout_tree::BuildSeed,
    /// Round 2: the surface formatter (default: the English fallback).
    formatter: Arc<dyn crate::format::Formatter>,
    /// A fixed clock for relative times (`None` = the wall clock).
    clock: Option<f64>,
    /// Bumped on every write to `data` (template key caches).
    data_version: u64,
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
        let mut engine = Engine::new();
        engine.set_rounding(options.rounding);
        let settings = SurfaceSettings { mode: if options.mode == Mode::Dark { ModeSetting::Dark } else { ModeSetting::Light }, ..SurfaceSettings::default() };
        let mut s = Surface {
            id: id.to_string(),
            catalog_id: options.catalog_id,
            view,
            recipes,
            extensions: options.extensions,
            theme: options.theme,
            effective: None,
            strings: crate::strings::DEFAULT_STRINGS.clone(),
            settings,
            mode: options.mode,
            expand_controls: options.expand_controls,
            components: Vec::new(),
            nested: None,
            data: Value::Object(Map::new()),
            root: None,
            issues: Vec::new(),
            local: LocalState::default(),
            states: HashMap::new(),
            nodes: Vec::new(),
            live: Vec::new(),
            slot_of: HashMap::new(),
            free: Vec::new(),
            main_root: None,
            layers: Vec::new(),
            lists: Vec::new(),
            toasts: Vec::new(),
            node_state: Vec::new(),
            engine,
            memo: MeasureMemo::default(),
            memo_versions: HashMap::new(),
            last_measurer: None,
            viewport: (0.0, 0.0),
            max_height: None,
            direction: Direction::Ltr,
            rounding: options.rounding,
            structure_version: 0,
            needs_build: true,
            restyle_all: true,
            style_dirty: BTreeSet::new(),
            visuals_dirty: HashSet::new(),
            template_cache: HashMap::new(),
            reduce_pending: false,
            component_index: HashMap::new(),
            last_frames: Vec::new(),
            archive: HashMap::new(),
            responsive: false,
            breakpoint: None,
            delta_added: BTreeSet::new(),
            delta_removed: BTreeSet::new(),
            delta_changed: BTreeSet::new(),
            renumbered: false,
            pending: Vec::new(),
            scrolls: Vec::new(),
            restyled: 0,
            rebuilt: false,
            built_nodes: 0,
            step_state: None,
            subtree_pending: BTreeSet::new(),
            hover_closing: HashSet::new(),
            seed: Default::default(),
            formatter: Arc::new(crate::format::EnglishFormatter),
            clock: None,
            data_version: 0,
        };
        s.refresh_theme();
        s
    }

    // ------------------------------------------------------------------
    // Configuration
    // ------------------------------------------------------------------

    /// Round 2: format numbers, currencies, percents, dates and relative
    /// times through `formatter` (the host's, built in the surface's locale
    /// and the host's time zone; or [`crate::format::ZonedEnglishFormatter`]
    /// at the host's UTC offset). The core has no zone database: the zone
    /// reaches it only through the formatter. The tree re-binds.
    pub fn set_formatter(&mut self, formatter: Arc<dyn crate::format::Formatter>) {
        self.formatter = formatter;
        self.needs_build = true;
    }

    /// The formatter in effect.
    pub fn formatter(&self) -> &Arc<dyn crate::format::Formatter> {
        &self.formatter
    }

    /// Pin the clock relative times read (epoch ms; `None` = the wall
    /// clock). A host re-binds a surface showing `formatRelativeTime`
    /// without `now` at least once a minute ([`Surface::tick`]).
    pub fn set_clock(&mut self, now_ms: Option<f64>) {
        self.clock = now_ms;
        self.needs_build = true;
    }

    /// Re-bind (relative times move): the next layout rebuilds.
    pub fn tick(&mut self) {
        self.needs_build = true;
    }

    /// Round 2 (§3): whether the surface shows a time that moves with the
    /// clock — a `formatRelativeTime` call without `now` in the tree or a
    /// template, or a Table `relativeTime` column — so the host calls
    /// [`Surface::tick`] at least once a minute. `false` with a pinned clock
    /// ([`Surface::set_clock`]). O(nodes): ask after a reduce or layout.
    pub fn uses_clock(&self) -> bool {
        fn relative_call(v: &Value) -> bool {
            match v {
                Value::Object(o) => (o.get("call").and_then(Value::as_str) == Some("formatRelativeTime") && o.get("args").and_then(|a| a.get("now")).is_none()) || o.values().any(relative_call),
                Value::Array(a) => a.iter().any(relative_call),
                _ => false,
            }
        }
        if self.clock.is_some() {
            return false;
        }
        let mut found = false;
        for tree in self.root.iter().chain(self.template_cache.values().flatten()) {
            tree.walk(&mut |n| found = found || n.props.values().any(relative_call));
        }
        found
            || self.nodes.iter().zip(&self.live).any(|(n, live)| {
                *live && n.component == "Table" && n.props.get("columns").and_then(Value::as_array).is_some_and(|cols| cols.iter().any(|c| c.get("type").and_then(Value::as_str) == Some("relativeTime")))
            })
    }

    /// Register an extension catalog (validated). The tree is re-reduced.
    pub fn register_extension(&mut self, ext: ExtensionDef) -> Result<(), String> {
        let ext = crate::extension::define_extension(ext)?;
        let mut all = self.extensions.clone();
        all.retain(|e| e.id != ext.id);
        all.push(ext);
        self.view = CatalogView::with(&all);
        self.recipes = Arc::new(RecipeIndex::new(&self.view));
        self.extensions = all;
        self.template_cache.clear();
        self.reduce();
        Ok(())
    }

    /// The theme in effect: the set theme with the surface's density and
    /// contrast applied (once per change, so every resolver stays agnostic).
    fn refresh_theme(&mut self) {
        let contrast = match self.settings.contrast {
            ContrastSetting::High => true,
            ContrastSetting::Normal => false,
            ContrastSetting::System => self.settings.system_high_contrast,
        };
        self.effective = self.theme.as_ref().map(|t| {
            if self.settings.density == Density::Default && !contrast {
                return t.clone();
            }
            let mut out = apply_density(t, self.settings.density);
            if contrast {
                out = apply_contrast(&out);
            }
            Arc::new(out)
        });
        self.restyle_all = true;
    }

    pub fn set_theme(&mut self, theme: Option<Arc<ResolvedTheme>>) {
        self.theme = theme;
        self.refresh_theme();
        // Gap tokens feed List gaps; expansion depends on a theme.
        self.needs_build = true;
        self.invalidate_measures();
    }

    pub fn theme(&self) -> Option<&Arc<ResolvedTheme>> {
        self.theme.as_ref()
    }

    /// The theme with density and contrast applied (what the core resolves
    /// against; painters reading tokens should use this one).
    pub fn effective_theme(&self) -> Option<&Arc<ResolvedTheme>> {
        self.effective.as_ref()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.settings.mode = if mode == Mode::Dark { ModeSetting::Dark } else { ModeSetting::Light };
        if self.mode != mode {
            self.mode = mode;
            self.restyle_all = true;
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn settings(&self) -> &SurfaceSettings {
        &self.settings
    }

    /// Replace every setting at once; the parts that changed take effect on
    /// the next layout (a type-size change re-measures).
    pub fn set_settings(&mut self, settings: SurfaceSettings) {
        let old = std::mem::replace(&mut self.settings, settings);
        let s = self.settings.clone();
        if old.strings != s.strings {
            self.strings = crate::strings::string_table(&s.strings);
            self.needs_build = true;
        }
        if old.locale != s.locale || old.today != s.today {
            self.needs_build = true;
            self.restyle_all = true;
        }
        if old.density != s.density || old.contrast != s.contrast || old.system_high_contrast != s.system_high_contrast {
            self.refresh_theme();
        }
        let mode = resolve_mode(s.mode, s.system_dark);
        if mode != self.mode {
            self.mode = mode;
            self.restyle_all = true;
        }
        if (old.font_scale - s.font_scale).abs() > f32::EPSILON || old.hover != s.hover || old.reduced_motion != s.reduced_motion {
            self.restyle_all = true;
        }
        if old.insets != s.insets {
            self.engine.mark_all_dirty();
        }
    }

    pub fn set_locale(&mut self, locale: &str) {
        let mut s = self.settings.clone();
        s.locale = locale.to_string();
        self.set_settings(s);
    }

    pub fn set_strings(&mut self, overrides: IndexMap<String, String>) {
        let mut s = self.settings.clone();
        s.strings = overrides;
        self.set_settings(s);
    }

    /// `light | dark | system`, with the platform's current preference.
    pub fn set_mode_setting(&mut self, mode: ModeSetting, system_dark: bool) {
        let mut s = self.settings.clone();
        s.mode = mode;
        s.system_dark = system_dark;
        self.set_settings(s);
    }

    pub fn set_density(&mut self, density: Density) {
        let mut s = self.settings.clone();
        s.density = density;
        self.set_settings(s);
    }

    pub fn set_contrast(&mut self, contrast: ContrastSetting, system_high: bool) {
        let mut s = self.settings.clone();
        s.contrast = contrast;
        s.system_high_contrast = system_high;
        self.set_settings(s);
    }

    /// Dynamic Type / font scale (1 = the theme's sizes).
    pub fn set_font_scale(&mut self, scale: f32) {
        let mut s = self.settings.clone();
        s.font_scale = scale.max(0.1);
        self.set_settings(s);
    }

    pub fn set_insets(&mut self, insets: Insets) {
        let mut s = self.settings.clone();
        s.insets = insets;
        self.set_settings(s);
    }

    /// A hover-capable pointer and the reduced-motion preference.
    pub fn set_pointer(&mut self, hover: bool, reduced_motion: bool) {
        let mut s = self.settings.clone();
        s.hover = hover;
        s.reduced_motion = reduced_motion;
        self.set_settings(s);
    }

    /// Android accumulates in dp and rounds once itself; the browser never
    /// rounds. Off = fractional frames.
    pub fn set_rounding(&mut self, on: bool) {
        self.rounding = on;
        self.engine.set_rounding(on);
        self.engine.mark_all_dirty();
    }

    /// Surface size. Height ≤ 0 = as tall as the content (a scrolling host).
    /// `max_height` bounds a transcript card; `LayoutOutput::overflow` says
    /// when the content is taller. Returns whether anything changed.
    pub fn set_viewport(&mut self, width: f32, height: f32, max_height: Option<f32>) -> bool {
        if self.viewport == (width, height) && self.max_height == max_height {
            return false;
        }
        let height_changed = self.viewport.1 != height || self.max_height != max_height;
        self.viewport = (width, height);
        self.max_height = max_height;
        for (i, s) in self.node_state.iter().enumerate() {
            if s.conditional && self.live.get(i).copied().unwrap_or(false) {
                self.style_dirty.insert(i as u32);
            }
        }
        // Windowed lists size their range off the viewport height; a
        // responsive native follows the breakpoint.
        if (height_changed && !self.lists.is_empty()) || (self.responsive && self.active_breakpoint() != self.breakpoint) {
            self.needs_build = true;
        }
        true
    }

    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// The breakpoint of the current width (theme tokens, else the defaults).
    fn active_breakpoint(&self) -> Option<String> {
        crate::conditions::active_breakpoint(self.viewport.0, &self.breakpoints())
    }

    fn breakpoints(&self) -> IndexMap<String, f32> {
        match &self.effective {
            Some(t) if !t.tokens.breakpoint.is_empty() => t.tokens.breakpoint.iter().map(|(k, v)| (k.clone(), *v as f32)).collect(),
            _ => crate::conditions::default_breakpoints(),
        }
    }

    /// Replace a node's interaction states (hover/pressed/focus/
    /// focus-visible/disabled…). Restyles THAT node only. A hover or a
    /// keyboard focus on a Tooltip's or a hover Popover's TRIGGER opens it
    /// at once (the host owns the open delay); it STAYS open while the
    /// pointer (or focus) is on the trigger or on ANY node of its layer.
    /// When both are left it closes at once, or — with
    /// `settings.hover_close_ms > 0` — raises [`OutEvent::HoverTimer`] and
    /// closes on [`Self::hover_timeout`] unless the pointer came back
    /// (crossing the gap from trigger to content). The change events are in
    /// `take_events`.
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
        if let Some(&slot) = self.slot_of.get(id) {
            self.style_dirty.insert(slot);
            for target in self.hover_targets(slot) {
                let open = self.local.open.get(&target).copied().unwrap_or(false);
                let active = self.hover_active(&target);
                if active && !open {
                    self.hover_closing.remove(&target);
                    let events = self.set_open(&target, true);
                    self.pending.extend(events.into_iter().filter(|e| !matches!(e, OutEvent::Relayout)));
                } else if active {
                    self.hover_closing.remove(&target);
                } else if open && self.settings.hover_close_ms == 0 {
                    let events = self.set_open(&target, false);
                    self.pending.extend(events.into_iter().filter(|e| !matches!(e, OutEvent::Relayout)));
                } else if open && self.hover_closing.insert(target.clone()) {
                    self.pending.push(OutEvent::HoverTimer { owner: target, delay_ms: self.settings.hover_close_ms });
                }
            }
        }
        true
    }

    /// A [`OutEvent::HoverTimer`] fired: close the hover overlay `owner`
    /// unless its trigger or its layer is hovered (or keyboard-focused)
    /// again. Returns the change events (`Relayout` when it closed).
    pub fn hover_timeout(&mut self, owner: &str) -> Vec<OutEvent> {
        if !self.hover_closing.remove(owner) || !self.opens_on_hover(owner) || self.hover_active(owner) {
            return vec![];
        }
        if !self.local.open.get(owner).copied().unwrap_or(false) {
            return vec![];
        }
        self.set_open(owner, false)
    }

    /// The hover overlays a node keeps open: the one it is the TRIGGER of
    /// (the node itself, so a host's open delay on the trigger holds), and
    /// every hover overlay whose LAYER it is in (then on from that overlay's
    /// own node: a hover card inside a hover card keeps both).
    fn hover_targets(&self, slot: u32) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = Some(slot);
        let mut guard = 0;
        while let Some(c) = cur {
            guard += 1;
            if guard > 4096 || !self.live.get(c as usize).copied().unwrap_or(false) {
                break;
            }
            let n = &self.nodes[c as usize];
            if let Some(t) = n.trigger_for.as_ref().filter(|_| c == slot) {
                if self.opens_on_hover(t) && !out.contains(t) {
                    out.push(t.clone());
                }
            }
            cur = match n.parent {
                Some(p) => Some(p),
                None => match self.layers.iter().find(|l| l.root == c) {
                    Some(l) => {
                        if self.opens_on_hover(&l.owner) && !out.contains(&l.owner) {
                            out.push(l.owner.clone());
                        }
                        self.slot_of.get(&l.owner).copied()
                    }
                    None => None,
                },
            };
        }
        out
    }

    /// Is any live node that keeps `target` open hovered or focused?
    fn hover_active(&self, target: &str) -> bool {
        self.states.iter().any(|(id, st)| {
            st.iter().any(|s| s == "hover" || s == "focus-visible" || s == "focus")
                && self.slot_of.get(id).is_some_and(|&slot| self.live[slot as usize] && self.hover_targets(slot).iter().any(|t| t == target))
                // A plain `focus` keeps it open only inside its layer (focus
                // moved into the card); on the trigger it takes focus-visible.
                && (st.iter().any(|s| s == "hover" || s == "focus-visible") || self.in_layer_of(self.slot_of[id], target))
        })
    }

    /// Is `slot` inside the layer `owner` opened?
    fn in_layer_of(&self, slot: u32, owner: &str) -> bool {
        let Some(layer) = self.layers.iter().find(|l| l.owner == owner) else { return false };
        let mut cur = Some(slot);
        while let Some(c) = cur {
            if c == layer.root {
                return true;
            }
            cur = self.nodes[c as usize].parent;
        }
        false
    }

    fn opens_on_hover(&self, target: &str) -> bool {
        let Some(&slot) = self.slot_of.get(target) else { return false };
        let n = &self.nodes[slot as usize];
        n.component == "Tooltip" || (n.component == "Popover" && n.props.get("openOn").and_then(Value::as_str) == Some("hover"))
    }

    /// The pressed set as a whole (the spike's `set_pressed`).
    pub fn set_pressed(&mut self, ids: &[String]) -> bool {
        self.set_flagged("pressed", ids)
    }

    /// The hovered set as a whole: `ids` carry the `hover` state (recipes'
    /// `hover` / style `:hover` resolve through it), every other node loses
    /// it. Same effects as [`Self::set_states`] (a hover overlay opens).
    pub fn set_hovered(&mut self, ids: &[String]) -> bool {
        self.set_flagged("hover", ids)
    }

    /// One node's `hover` on or off, its other states kept (a host's pointer
    /// enter / leave).
    pub fn set_hover(&mut self, id: &str, hovered: bool) -> bool {
        let mut s = self.states.get(id).cloned().unwrap_or_default();
        let has = s.iter().any(|x| x == "hover");
        if has == hovered {
            return false;
        }
        if hovered {
            s.push("hover".into());
        } else {
            s.retain(|x| x != "hover");
        }
        self.set_states(id, s)
    }

    /// The interaction states the HOST set on a node (`hover`, `pressed`,
    /// `focus`…; [`Self::set_states`]), empty when none.
    pub fn host_states(&self, id: &str) -> &[String] {
        self.states.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Exactly `ids` carry `flag`; the rest of each node's states are kept.
    fn set_flagged(&mut self, flag: &str, ids: &[String]) -> bool {
        let mut changed = false;
        let keep: HashSet<&String> = ids.iter().collect();
        let had: Vec<String> = self.states.iter().filter(|(_, s)| s.iter().any(|x| x == flag)).map(|(k, _)| k.clone()).collect();
        for id in had {
            if !keep.contains(&id) {
                let mut s = self.states.get(&id).cloned().unwrap_or_default();
                s.retain(|x| x != flag);
                changed |= self.set_states(&id, s);
            }
        }
        for id in ids {
            let mut s = self.states.get(id).cloned().unwrap_or_default();
            if !s.iter().any(|x| x == flag) {
                s.push(flag.into());
                changed |= self.set_states(id, s);
            }
        }
        changed
    }

    /// Events the core raised outside a call that returns them (a hover
    /// opening a tooltip, a live region announcing).
    pub fn take_events(&mut self) -> Vec<OutEvent> {
        std::mem::take(&mut self.pending)
    }

    /// Forget every cached measurement (a font load, Dynamic Type).
    pub fn invalidate_measures(&mut self) {
        self.memo.clear();
        self.memo_versions.clear();
        self.archive.clear();
        self.engine.mark_all_dirty();
    }

    /// Forget one node's cached measurement.
    pub fn mark_dirty(&mut self, index: u32) -> bool {
        if !self.live.get(index as usize).copied().unwrap_or(false) {
            return false;
        }
        self.memo.forget(index);
        self.memo_versions.remove(&index);
        self.engine.mark_dirty(index);
        true
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
            self.set_component_list(Vec::new());
            self.nested = None;
            self.data = Value::Object(Map::new());
            self.data_version += 1;
            self.local = LocalState::default();
            self.template_cache.clear();
            self.reduce();
            return Ok(ApplyOutcome { structure_changed: true, issues: self.issues.clone() });
        }
        if let Some(update) = obj.get("updateComponents") {
            let components = update.get("components").ok_or("updateComponents.components is required")?;
            let list: Vec<FlatComponent> = serde_json::from_value(components.clone()).map_err(|e| format!("components: {e}"))?;
            return Ok(self.update_components(list));
        }
        self.ensure_reduced();
        if let Some(update) = obj.get("updateDataModel") {
            let path = update.get("path").and_then(Value::as_str).unwrap_or("");
            let value = update.get("value").cloned();
            let issues = match self.set_data(path, value) {
                Ok(()) => Vec::new(),
                Err(message) => vec![ReduceIssue { id: path.to_string(), message }],
            };
            return Ok(ApplyOutcome { structure_changed: self.needs_build, issues });
        }
        if obj.get("deleteSurface").is_some() {
            self.set_component_list(Vec::new());
            self.nested = None;
            self.root = None;
            self.data = Value::Object(Map::new());
            self.data_version += 1;
            self.local = LocalState::default();
            self.needs_build = true;
            return Ok(ApplyOutcome { structure_changed: true, issues: Vec::new() });
        }
        Err("unknown message: expected createSurface | updateComponents | updateDataModel | deleteSurface".into())
    }

    /// An `updateComponents` payload. A2UI: a later update replaces
    /// components BY ID and keeps the rest (the TS reference's SurfaceStore
    /// does the same). VAPP-103: O(update) here, the tree reduces LAZILY on
    /// the next read (layout, an event, [`Self::issues`]), so a surface
    /// streamed one component per message costs linear, not quadratic.
    pub fn update_components(&mut self, list: Vec<FlatComponent>) -> ApplyOutcome {
        if self.nested.is_some() {
            self.set_component_list(Vec::new());
        }
        // The previous version of every id this update touches (`None` =
        // new), for the local field values.
        let mut replaced: HashMap<String, Option<FlatComponent>> = HashMap::new();
        for c in list {
            match self.component_index.get(&c.id) {
                Some(&i) => {
                    let old = std::mem::replace(&mut self.components[i], c);
                    replaced.entry(old.id.clone()).or_insert(Some(old));
                }
                None => {
                    replaced.entry(c.id.clone()).or_insert(None);
                    self.component_index.insert(c.id.clone(), self.components.len());
                    self.components.push(c);
                }
            }
        }
        self.nested = None;
        self.retain_updated_field_values(&replaced);
        self.reduce_pending = true;
        self.needs_build = true;
        ApplyOutcome { structure_changed: true, issues: Vec::new() }
    }

    /// The nested authoring form (fixtures, MCP templates) instead of a flat
    /// component list.
    pub fn set_nested(&mut self, tree: NestedNode) -> ApplyOutcome {
        self.nested = Some(tree);
        self.set_component_list(Vec::new());
        self.template_cache.clear();
        self.local.field_values.clear();
        self.reduce();
        ApplyOutcome { structure_changed: true, issues: self.issues.clone() }
    }

    /// A flat component list (what `updateComponents` carries).
    pub fn set_components(&mut self, components: Vec<FlatComponent>) -> ApplyOutcome {
        let before = if self.nested.is_some() { Vec::new() } else { std::mem::take(&mut self.components) };
        self.set_component_list(components);
        self.nested = None;
        self.template_cache.clear();
        self.retain_field_values(&before);
        self.reduce();
        ApplyOutcome { structure_changed: true, issues: self.issues.clone() }
    }

    /// Replace the whole component list (the id index follows).
    fn set_component_list(&mut self, components: Vec<FlatComponent>) {
        self.component_index = components.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
        self.components = components;
    }

    /// [`Self::retain_field_values`] after a by-id update: `replaced` = the
    /// previous version of each id it touched (`None` = new); every other
    /// component is unchanged. O(field values), never O(components).
    fn retain_updated_field_values(&mut self, replaced: &HashMap<String, Option<FlatComponent>>) {
        if self.local.field_values.is_empty() {
            return;
        }
        let (components, index) = (&self.components, &self.component_index);
        self.local.field_values.retain(|key, _| {
            let mut candidate = key.as_str();
            loop {
                if let Some(old) = replaced.get(candidate) {
                    let new = index.get(candidate).map(|&i| &components[i]);
                    return matches!((old.as_ref(), new), (Some(a), Some(b)) if a == b);
                }
                if index.contains_key(candidate) {
                    return true;
                }
                match candidate.rfind('.') {
                    Some(i) => candidate = &candidate[..i],
                    None => return false,
                }
            }
        });
    }

    /// After a component update, keep an unbound control's local value
    /// while the component that owns it is unchanged (React keeps the state
    /// of a still-mounted node the same way); drop it when that component
    /// was removed or re-sent different. A value's owner = the longest
    /// component id equal to the key or prefixing it at a `.` (parts and
    /// template instances).
    fn retain_field_values(&mut self, before: &[FlatComponent]) {
        if self.local.field_values.is_empty() {
            return;
        }
        let old: HashMap<&str, &FlatComponent> = before.iter().map(|c| (c.id.as_str(), c)).collect();
        let new: HashMap<&str, &FlatComponent> = self.components.iter().map(|c| (c.id.as_str(), c)).collect();
        self.local.field_values.retain(|key, _| {
            let mut candidate = key.as_str();
            loop {
                if old.contains_key(candidate) || new.contains_key(candidate) {
                    return matches!((old.get(candidate), new.get(candidate)), (Some(a), Some(b)) if a == b);
                }
                match candidate.rfind('.') {
                    Some(i) => candidate = &candidate[..i],
                    None => return false,
                }
            }
        });
    }

    /// Write `value` at `path` (`None` removes). Templates and bindings
    /// re-resolve on the next layout.
    /// VAPP-103: a refused write (a pointer past the limits, a non-index
    /// token or an index past the end of an array) changes nothing and
    /// returns the reason.
    pub fn set_data(&mut self, path: &str, value: Option<Value>) -> Result<(), String> {
        set_pointer(&mut self.data, path, value)?;
        self.data_version += 1;
        self.needs_build = true;
        Ok(())
    }

    pub fn data(&self) -> &Value {
        &self.data
    }

    pub fn root(&mut self) -> Option<&UiNode> {
        self.ensure_reduced();
        self.root.as_ref()
    }

    /// The reduce issues (plus the build's: styles, template items), after
    /// any streamed update is reduced.
    pub fn issues(&mut self) -> &[ReduceIssue] {
        self.ensure_reduced();
        &self.issues
    }

    /// Reduce a streamed update now (every reader of the tree calls it).
    pub(crate) fn ensure_reduced(&mut self) {
        if self.reduce_pending {
            self.reduce();
        }
    }

    fn reduce(&mut self) {
        crate::roomy(|| self.reduce_now())
    }

    fn reduce_now(&mut self) {
        self.reduce_pending = false;
        self.template_cache.clear();
        self.local.static_keys.clear();
        self.local.key_cache.clear();
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
                // Round 2: template nodes are lifted out of the tree; items
                // instantiate them from this table.
                self.template_cache = r.templates.unwrap_or_default().into_iter().map(|(id, n)| (id, Some(n))).collect();
            }
            None => {
                self.root = None;
                self.issues.clear();
            }
        }
        self.needs_build = true;
    }

    fn gap_px(&self, name: &str) -> f32 {
        match &self.effective {
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

    // ------------------------------------------------------------------
    // Snapshots
    // ------------------------------------------------------------------

    pub fn structure_version(&self) -> u64 {
        self.structure_version
    }

    fn placed(&self, i: usize) -> PlacedNode {
        let n = &self.nodes[i];
        let removed = !self.live[i];
        let mut states: Vec<String> = Vec::new();
        for q in [&n.part_query, &n.own_query].into_iter().flatten() {
            for s in &q.states {
                if !states.contains(s) {
                    states.push(s.clone());
                }
            }
        }
        PlacedNode {
            index: i as u32,
            id: n.id.clone(),
            component: n.component.clone(),
            part: n.part.clone(),
            owner: n.owner.clone(),
            owner_component: n.owner_component.clone(),
            catalog_id: n.catalog_id.clone(),
            extension_kind: n.extension_kind.clone(),
            depth: n.depth,
            parent: if removed { None } else { n.parent },
            children: if removed { Vec::new() } else { n.children.clone() },
            layer: n.layer,
            kind: n.kind,
            props: if removed { Map::new() } else { n.props.clone() },
            lines: n.lines,
            pressable: n.pressable && !removed,
            hidden: n.hidden || removed,
            accessibility: n.accessibility.clone(),
            trigger_for: n.trigger_for.clone(),
            states,
            form: n.form.clone(),
            live: n.live.clone(),
            removed,
        }
    }

    /// Every slot (removed ones as tombstones, so `nodes()[i].index == i`).
    pub fn nodes(&mut self) -> Vec<PlacedNode> {
        self.settle();
        (0..self.nodes.len()).map(|i| self.placed(i)).collect()
    }

    /// The given slots only (a delta's `added` + `changed`).
    pub fn nodes_at(&mut self, indices: &[u32]) -> Vec<PlacedNode> {
        self.settle();
        indices.iter().filter(|i| (**i as usize) < self.nodes.len()).map(|i| self.placed(*i as usize)).collect()
    }

    /// Live nodes.
    pub fn node_count(&mut self) -> usize {
        self.settle();
        self.live.iter().filter(|l| **l).count()
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
        self.nodes.get(index as usize).filter(|_| self.live.get(index as usize).copied().unwrap_or(false))
    }

    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.slot_of.get(id).copied()
    }

    /// Round 2 (VAPP-100): does the node restyle under the pointer? True
    /// when its style has a `:hover` block or its recipe (the native's own
    /// root recipe, its part or macro recipe) has a rule on `state: hover`
    /// that its props match ([`crate::theme::recipe_reacts_to`]), in the
    /// effective theme. A painter tracks the pointer over these nodes (as
    /// over pressables, triggers and fields) and reports `hover`. Ask again
    /// after a theme switch.
    pub fn hover_styled(&self, index: u32) -> bool {
        let Some(n) = self.layout_node(index) else { return false };
        if n.base_style.contains_key(":hover") {
            return true;
        }
        let Some(theme) = &self.effective else { return false };
        [&n.own_query, &n.part_query].into_iter().flatten().any(|q| crate::theme::recipe_reacts_to(theme, q, "hover"))
    }

    pub fn last_frame(&self, index: u32) -> Option<Frame> {
        self.last_frames.get(index as usize).copied()
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Is this surface a basic-catalog one?
    pub fn is_basic(&self) -> bool {
        self.catalog_id == A2UI_BASIC_CATALOG_ID
    }

    pub fn get_data(&self, path: &str) -> Option<&Value> {
        get_pointer(&self.data, path)
    }

    /// The built-in string table in effect (defaults + the host's).
    pub fn strings(&self) -> &StringTable {
        &self.strings
    }

    /// Apply a pending rebuild / re-window before a snapshot. Not while a
    /// stepped pass is running (the FFI measures between the steps without
    /// the lock): the pass finishes on the nodes it styled and measured, and
    /// the rebuild waits for the next pass.
    fn settle(&mut self) {
        if self.step_state.is_some() {
            return;
        }
        self.ensure_reduced();
        if self.needs_build {
            self.subtree_pending.clear();
            self.rebuild();
        } else {
            for id in std::mem::take(&mut self.subtree_pending) {
                if !self.rebuild_subtree(&id) {
                    self.rebuild();
                    self.subtree_pending.clear();
                    break;
                }
            }
        }
    }

    fn states_of(&self, n: &LNode) -> Vec<String> {
        let mut states = self.states.get(&n.id).cloned().unwrap_or_default();
        if n.props.get("disabled").and_then(Value::as_bool) == Some(true) && !states.iter().any(|s| s == "disabled") {
            states.push("disabled".into());
        }
        for q in [&n.part_query, &n.own_query].into_iter().flatten() {
            for s in &q.states {
                if !states.contains(s) {
                    states.push(s.clone());
                }
            }
        }
        states
    }
}
