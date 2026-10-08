//! The UniFFI facade over `exponential-ui` (VAPP-86) for the SwiftUI
//! (VAPP-88) and Compose (VAPP-89) painters.
//!
//! Shape of the contract (the VAPP-4 verdict): the FFI boundary is COARSE.
//! - A `Surface` object is created once; A2UI messages go in as JSON text.
//! - `nodes()` hands the host the static per-node data once per structure
//!   version; `visuals()` the resolved visuals; `layout(measurer)` returns one
//!   flat frame list plus the overlay layers and crosses back into the host
//!   at most three times per pass (`Measurer`: intrinsics in one batch,
//!   heights in one batch, one correction).
//! - Interactions come in as `event(index, name, payloadJson)` and return the
//!   events the host forwards (server actions, data writes, input edits).
//! - Free functions expose the reducer, the theme loader and the placement
//!   rule so the binding test suites replay the shared fixtures.
//! - The host API (VAPP-91, `catalog/host.json`): a `HostRouter` object
//!   (server messages in, ops out), the `JsonlDecoder` / `SseDecoder`
//!   transports' decoders, and the policy / sources / packages functions,
//!   JSON text in and out.

use std::sync::{Arc, Mutex};

use exponential_ui::measure::{HeightRequest, Intrinsics, LeafRequest, Measure};
use exponential_ui::surface::{Layer, LayoutOutput, OutEvent, PlacedNode, Surface as CoreSurface, SurfaceOptions};
use exponential_ui::theme::{Mode, RecipeQuery, ThemeOptions, ThemeRef};
use exponential_ui::types::{ExtensionDef, FlatComponent, NestedNode};
use serde_json::Value;

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
pub enum UiError {
    Invalid { reason: String },
    /// A theme failed to load; `issues_json` = `[{path, message}]`.
    Theme { issues_json: String },
}

impl std::error::Error for UiError {}

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::Invalid { reason } => write!(f, "invalid: {reason}"),
            UiError::Theme { issues_json } => write!(f, "theme: {issues_json}"),
        }
    }
}

fn invalid(e: impl std::fmt::Display) -> UiError {
    UiError::Invalid { reason: e.to_string() }
}

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, UiError> {
    serde_json::from_str(json).map_err(invalid)
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiFrame {
    pub index: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPlacement {
    pub x: f64,
    pub y: f64,
    pub side: String,
    pub flipped: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLayer {
    pub layer: u32,
    pub kind: String,
    pub owner: String,
    pub root: u32,
    pub anchor_frame: Option<FfiFrame>,
    pub placement: Option<FfiPlacement>,
    /// `centered`, a viewport edge (`top|right|bottom|left`) or the side an
    /// anchored layer landed on.
    pub position: String,
    pub frames: Vec<FfiFrame>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiList {
    pub id: String,
    pub node: u32,
    pub content_height: f32,
    pub start: u32,
    pub end: u32,
    pub count: u32,
    pub windowed: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLayout {
    /// Main-tree frames in pre-order = paint order = accessibility order.
    pub frames: Vec<FfiFrame>,
    pub layers: Vec<FfiLayer>,
    pub lists: Vec<FfiList>,
    /// Node indices whose visual changed since the previous pass.
    pub visual_changes: Vec<u32>,
    pub structure_version: u64,
    pub surface_width: f32,
    pub surface_height: f32,
    pub overflow: bool,
    pub measure_rounds: u32,
    pub upcalls: u32,
    pub layout_ns: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiTextStyle {
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub font_family: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiControlBox {
    pub padding_horizontal: f32,
    pub padding_vertical: f32,
    pub border_width: f32,
    pub gap: f32,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// One leaf the host must measure. `props_json` = the resolved props.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLeaf {
    pub index: u32,
    pub id: String,
    pub component: String,
    pub part: Option<String>,
    pub props_json: String,
    pub text: String,
    pub text_style: FfiTextStyle,
    pub control: FfiControlBox,
    pub lines: Option<u32>,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiIntrinsics {
    pub min_content_width: f32,
    pub max_content_width: f32,
    pub height_at_max_content: f32,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiHeightRequest {
    pub index: u32,
    pub width: f32,
}

/// Implemented by the host (SwiftUI `sizeThatFits` / Compose intrinsics),
/// called in BATCHES: at most three times per layout pass.
#[uniffi::export(with_foreign)]
pub trait Measurer: Send + Sync {
    /// Identity of the measurer's state (font scale…); a change invalidates
    /// every cached measurement.
    fn measure_id(&self) -> u64;
    fn measure_intrinsics(&self, leaves: Vec<FfiLeaf>) -> Vec<FfiIntrinsics>;
    fn measure_heights(&self, leaves: Vec<FfiLeaf>, requests: Vec<FfiHeightRequest>) -> Vec<f32>;
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiNode {
    pub index: u32,
    pub id: String,
    pub component: String,
    pub part: Option<String>,
    pub owner: Option<String>,
    pub owner_component: Option<String>,
    pub catalog_id: Option<String>,
    pub extension_kind: Option<String>,
    pub depth: u32,
    pub parent: Option<u32>,
    pub layer: u32,
    pub is_leaf: bool,
    pub props_json: String,
    pub lines: Option<u32>,
    pub pressable: bool,
    pub hidden: bool,
    pub trigger_for: Option<String>,
    pub accessibility_json: Option<String>,
    /// The states the core resolved into the node's recipe query (a tab
    /// `selected`, an accordion trigger `open`, a check part `checked`).
    pub part_states: Vec<String>,
    /// The macro a non-part node expands (`Card`, `Alert`…), else null.
    pub macro_name: Option<String>,
}

/// A part's resolved look (VAPP-88): the painted visual plus the flat style
/// map (`width`, `height`, `borderWidth`… as JSON) for the parts the core does
/// not synthesize (a Checkbox `check`, a Switch `thumb`, a Select `trigger`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPartStyle {
    pub visual: FfiVisual,
    pub style_json: String,
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct FfiVisual {
    pub background_color: Option<String>,
    pub color: Option<String>,
    pub border_width: Option<f32>,
    pub border_color: Option<String>,
    pub border_radius: Option<f32>,
    pub opacity: Option<f32>,
    /// `[{x, y, blur, spread, color}]`, or null.
    pub box_shadow_json: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub line_height: Option<f32>,
    pub font_family: Option<String>,
    pub text_align: Option<String>,
    pub padding_horizontal: Option<f32>,
    pub padding_vertical: Option<f32>,
    pub gap: Option<f32>,
    pub native: bool,
    pub overflow_hidden: bool,
    pub overflow_scroll: bool,
}

/// One event for the host: `kind` = `action | openUrl | functionCall |
/// dataChanged | input | relayout`, `json` = the event's fields
/// (`functionCall`: `{componentId, name, args}`, a host function for the
/// registry + `decideFunction` gate).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiEvent {
    pub kind: String,
    pub json: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiApplyOutcome {
    pub structure_changed: bool,
    pub issues_json: String,
}

fn leaf(l: &LeafRequest) -> FfiLeaf {
    FfiLeaf {
        index: l.index,
        id: l.id.to_string(),
        component: l.component.to_string(),
        part: l.part.map(str::to_string),
        props_json: Value::Object(l.props.clone()).to_string(),
        text: l.text().to_string(),
        text_style: FfiTextStyle { font_size: l.text_style.font_size, font_weight: l.text_style.font_weight, line_height: l.text_style.line_height, font_family: l.text_style.font_family.clone() },
        control: FfiControlBox {
            padding_horizontal: l.control.padding_horizontal,
            padding_vertical: l.control.padding_vertical,
            border_width: l.control.border_width,
            gap: l.control.gap,
            min_width: l.control.min_width,
            min_height: l.control.min_height,
            width: l.control.width,
            height: l.control.height,
        },
        lines: l.lines,
    }
}

struct ForeignMeasure(Arc<dyn Measurer>);

impl Measure for ForeignMeasure {
    fn measure_id(&self) -> u64 {
        self.0.measure_id()
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        self.0
            .measure_intrinsics(leaves.iter().map(leaf).collect())
            .into_iter()
            .map(|i| Intrinsics { min_content_width: i.min_content_width, max_content_width: i.max_content_width, height_at_max_content: i.height_at_max_content })
            .collect()
    }
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        self.0.measure_heights(leaves.iter().map(leaf).collect(), requests.iter().map(|r| FfiHeightRequest { index: r.index, width: r.width }).collect())
    }
}

fn frame(f: &exponential_ui::surface::PlacedFrame) -> FfiFrame {
    FfiFrame { index: f.index, x: f.x, y: f.y, w: f.w, h: f.h }
}

fn layer(l: &Layer) -> FfiLayer {
    FfiLayer {
        layer: l.layer,
        kind: l.kind.clone(),
        owner: l.owner.clone(),
        root: l.root,
        anchor_frame: l.anchor_frame.map(|a| FfiFrame { index: 0, x: a.x, y: a.y, w: a.w, h: a.h }),
        placement: l.placement.map(|p| FfiPlacement { x: p.x, y: p.y, side: p.side.as_str().to_string(), flipped: p.flipped }),
        position: l.position.clone(),
        frames: l.frames.iter().map(frame).collect(),
    }
}

fn convert(out: LayoutOutput) -> FfiLayout {
    FfiLayout {
        frames: out.frames.iter().map(frame).collect(),
        layers: out.layers.iter().map(layer).collect(),
        lists: out.lists.iter().map(|l| FfiList { id: l.id.clone(), node: l.node, content_height: l.content_height, start: l.start, end: l.end, count: l.count, windowed: l.windowed }).collect(),
        visual_changes: out.visual_changes,
        structure_version: out.structure_version,
        surface_width: out.surface_width,
        surface_height: out.surface_height,
        overflow: out.overflow,
        measure_rounds: out.measure_rounds,
        upcalls: out.upcalls,
        layout_ns: out.layout_ns,
    }
}

fn node(n: &PlacedNode, lnode: Option<&exponential_ui::layout_tree::LNode>) -> FfiNode {
    let query = lnode.and_then(|l| l.part_query.as_ref());
    FfiNode {
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
        is_leaf: n.kind == exponential_ui::layout_tree::NodeKind::Leaf,
        props_json: Value::Object(n.props.clone()).to_string(),
        lines: n.lines,
        pressable: n.pressable,
        hidden: n.hidden,
        trigger_for: n.trigger_for.clone(),
        accessibility_json: n.accessibility.as_ref().map(Value::to_string),
        part_states: query.map(|q| q.states.clone()).unwrap_or_default(),
        macro_name: match (query, lnode) {
            (Some(q), Some(l)) if l.owner.is_none() => Some(q.component.clone()),
            _ => None,
        },
    }
}

fn visual(v: &exponential_ui::style::Visual) -> FfiVisual {
    FfiVisual {
        background_color: v.background_color.clone(),
        color: v.color.clone(),
        border_width: v.border_width,
        border_color: v.border_color.clone(),
        border_radius: v.border_radius,
        opacity: v.opacity,
        box_shadow_json: v.box_shadow.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default()),
        font_size: v.font_size,
        font_weight: v.font_weight,
        line_height: v.line_height,
        font_family: v.font_family.clone(),
        text_align: v.text_align.clone(),
        padding_horizontal: v.padding_horizontal,
        padding_vertical: v.padding_vertical,
        gap: v.gap,
        native: v.native,
        overflow_hidden: v.overflow_hidden,
        overflow_scroll: v.overflow_scroll,
    }
}

fn events(list: Vec<OutEvent>) -> Vec<FfiEvent> {
    list.into_iter()
        .map(|e| {
            let v = serde_json::to_value(&e).unwrap_or(Value::Null);
            let kind = v.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
            FfiEvent { kind, json: v.to_string() }
        })
        .collect()
}

fn mode_of(mode: &str) -> Result<Mode, UiError> {
    Mode::parse(mode).ok_or_else(|| invalid(format!("mode must be light|dark, got {mode:?}")))
}

/// ONE surface. Every method locks it; the host calls from any thread.
#[derive(uniffi::Object)]
pub struct Surface {
    inner: Mutex<CoreSurface>,
}

#[uniffi::export]
impl Surface {
    /// `theme_id` = a built-in id (default `exponential`), `""` = no theme
    /// (geometry mode); `mode` = `light|dark`.
    #[uniffi::constructor]
    pub fn new(surface_id: String, catalog_id: String, theme_id: Option<String>, mode: String) -> Result<Arc<Self>, UiError> {
        let theme = match theme_id.as_deref() {
            Some("") => None,
            Some(id) => Some(exponential_ui::themes::builtin_theme(id).ok_or_else(|| invalid(format!("unknown built-in theme {id:?}")))?),
            None => Some(exponential_ui::themes::default_theme()),
        };
        let options = SurfaceOptions { catalog_id, theme, mode: mode_of(&mode)?, extensions: Vec::new(), rounding: false };
        Ok(Arc::new(Surface { inner: Mutex::new(CoreSurface::new(&surface_id, options)) }))
    }

    /// A surface painting with a `Theme` object (`None` = geometry mode).
    #[uniffi::constructor]
    pub fn with_theme(surface_id: String, catalog_id: String, theme: Option<Arc<Theme>>, mode: String) -> Result<Arc<Self>, UiError> {
        let options = SurfaceOptions { catalog_id, theme: theme.map(|t| t.inner.clone()), mode: mode_of(&mode)?, extensions: Vec::new(), rounding: false };
        Ok(Arc::new(Surface { inner: Mutex::new(CoreSurface::new(&surface_id, options)) }))
    }

    /// Paint with this `Theme` object from the next pass on.
    pub fn set_theme(&self, theme: Arc<Theme>) {
        self.inner.lock().unwrap().set_theme(Some(theme.inner.clone()));
    }

    /// The theme in force, as a `Theme` object (null in geometry mode).
    pub fn theme(&self) -> Option<Arc<Theme>> {
        self.inner.lock().unwrap().theme().map(|t| Arc::new(Theme { inner: t.clone() }))
    }

    /// `light|dark`.
    pub fn mode(&self) -> String {
        self.inner.lock().unwrap().mode().as_str().to_string()
    }

    /// Load a theme file (JSON; `extends` may name a built-in) and use it.
    pub fn set_theme_json(&self, theme_json: String) -> Result<(), UiError> {
        let theme = load_theme_inner(&theme_json, None)?;
        self.inner.lock().unwrap().set_theme(Some(Arc::new(theme)));
        Ok(())
    }

    pub fn set_builtin_theme(&self, id: String) -> Result<(), UiError> {
        let theme = exponential_ui::themes::builtin_theme(&id).ok_or_else(|| invalid(format!("unknown built-in theme {id:?}")))?;
        self.inner.lock().unwrap().set_theme(Some(theme));
        Ok(())
    }

    pub fn set_mode(&self, mode: String) -> Result<(), UiError> {
        self.inner.lock().unwrap().set_mode(mode_of(&mode)?);
        Ok(())
    }

    /// Register an extension catalog (its JSON definition), validated.
    pub fn register_extension(&self, extension_json: String) -> Result<(), UiError> {
        let ext: ExtensionDef = parse(&extension_json)?;
        self.inner.lock().unwrap().register_extension(ext).map_err(invalid)
    }

    /// One A2UI server→client message as JSON.
    pub fn apply(&self, message_json: String) -> Result<FfiApplyOutcome, UiError> {
        let message: Value = parse(&message_json)?;
        let outcome = self.inner.lock().unwrap().apply(&message).map_err(invalid)?;
        Ok(FfiApplyOutcome { structure_changed: outcome.structure_changed, issues_json: serde_json::to_string(&outcome.issues).unwrap_or_default() })
    }

    /// The nested authoring form (fixtures, MCP templates).
    pub fn set_nested(&self, nested_json: String) -> Result<FfiApplyOutcome, UiError> {
        let tree: NestedNode = parse(&nested_json)?;
        let outcome = self.inner.lock().unwrap().set_nested(tree);
        Ok(FfiApplyOutcome { structure_changed: outcome.structure_changed, issues_json: serde_json::to_string(&outcome.issues).unwrap_or_default() })
    }

    /// A flat component list (the `updateComponents` payload's `components`).
    pub fn set_components(&self, components_json: String) -> Result<FfiApplyOutcome, UiError> {
        let components: Vec<FlatComponent> = parse(&components_json)?;
        let outcome = self.inner.lock().unwrap().set_components(components);
        Ok(FfiApplyOutcome { structure_changed: outcome.structure_changed, issues_json: serde_json::to_string(&outcome.issues).unwrap_or_default() })
    }

    /// Write at a JSON pointer (`value_json` null/None removes).
    pub fn set_data(&self, path: String, value_json: Option<String>) -> Result<(), UiError> {
        let value = match value_json {
            Some(v) => Some(parse::<Value>(&v)?),
            None => None,
        };
        self.inner.lock().unwrap().set_data(&path, value);
        Ok(())
    }

    pub fn data_json(&self) -> String {
        self.inner.lock().unwrap().data().to_string()
    }

    pub fn issues_json(&self) -> String {
        serde_json::to_string(&self.inner.lock().unwrap().issues).unwrap_or_default()
    }

    /// Height ≤ 0 = as tall as the content; `max_height` bounds a card.
    pub fn set_viewport(&self, width: f32, height: f32, max_height: Option<f32>) -> bool {
        self.inner.lock().unwrap().set_viewport(width, height, max_height)
    }

    /// Off = fractional frames (the default; Android rounds once itself).
    pub fn set_rounding(&self, on: bool) {
        self.inner.lock().unwrap().set_rounding(on)
    }

    pub fn set_states(&self, id: String, states: Vec<String>) -> bool {
        self.inner.lock().unwrap().set_states(&id, states)
    }

    pub fn set_pressed(&self, ids: Vec<String>) -> bool {
        self.inner.lock().unwrap().set_pressed(&ids)
    }

    pub fn invalidate_measures(&self) {
        self.inner.lock().unwrap().invalidate_measures()
    }

    pub fn mark_dirty(&self, index: u32) -> bool {
        self.inner.lock().unwrap().mark_dirty(index)
    }

    pub fn layout(&self, measurer: Arc<dyn Measurer>) -> FfiLayout {
        let mut m = ForeignMeasure(measurer);
        convert(self.inner.lock().unwrap().layout(&mut m))
    }

    /// The fixed fake measure (8 px per character, 20 px lines, control
    /// boxes; `sizes_json` = `{id: {w, h}}` overrides), for geometry tests.
    pub fn layout_fixed(&self, sizes_json: Option<String>, wrap: bool) -> Result<FfiLayout, UiError> {
        let mut sizes = std::collections::HashMap::new();
        if let Some(json) = sizes_json {
            let map: serde_json::Map<String, Value> = parse(&json)?;
            for (id, m) in map {
                sizes.insert(id, (m["w"].as_f64().unwrap_or(0.0) as f32, m["h"].as_f64().unwrap_or(0.0) as f32));
            }
        }
        let mut m = exponential_ui::measure::FixedMeasure { sizes, wrap };
        Ok(convert(self.inner.lock().unwrap().layout(&mut m)))
    }

    pub fn structure_version(&self) -> u64 {
        self.inner.lock().unwrap().structure_version()
    }

    pub fn node_count(&self) -> u32 {
        self.inner.lock().unwrap().node_count() as u32
    }

    /// Every layout node, once per structure version.
    pub fn nodes(&self) -> Vec<FfiNode> {
        let mut inner = self.inner.lock().unwrap();
        let placed = inner.nodes();
        placed.iter().map(|n| node(n, inner.layout_node(n.index))).collect()
    }

    pub fn visuals(&self) -> Vec<FfiVisual> {
        self.inner.lock().unwrap().visuals().iter().map(visual).collect()
    }

    pub fn visual(&self, index: u32) -> Option<FfiVisual> {
        self.inner.lock().unwrap().visual(index).map(visual)
    }

    pub fn text_style(&self, index: u32) -> Option<FfiTextStyle> {
        self.inner.lock().unwrap().text_style(index).map(|t| FfiTextStyle { font_size: t.font_size, font_weight: t.font_weight, line_height: t.line_height, font_family: t.font_family.clone() })
    }

    pub fn index_of(&self, id: String) -> Option<u32> {
        self.inner.lock().unwrap().index_of(&id)
    }

    /// An interaction on node `index`: `press`, `change`, `select`, `submit`,
    /// `commit`, with an optional JSON payload (`{value}`, `{open}`, `{page}`).
    pub fn event(&self, index: u32, name: String, payload_json: Option<String>) -> Result<Vec<FfiEvent>, UiError> {
        let payload = match payload_json {
            Some(p) => Some(parse::<Value>(&p)?),
            None => None,
        };
        Ok(events(self.inner.lock().unwrap().event(index, &name, payload)))
    }

    pub fn set_open(&self, id: String, open: bool) -> Vec<FfiEvent> {
        events(self.inner.lock().unwrap().set_open(&id, open))
    }

    pub fn scroll(&self, list_id: String, offset: f32) -> bool {
        self.inner.lock().unwrap().scroll(&list_id, offset)
    }

    pub fn failing_checks(&self, id: String) -> Vec<String> {
        self.inner.lock().unwrap().failing_checks(&id)
    }
}

// ---------------------------------------------------------------------------
// Theme: a resolved theme the painter queries per part (VAPP-88). Painters
// never read recipes themselves: `resolve_part` answers a sub-part the core
// does not synthesize exactly like the core resolves a synthetic part.
// ---------------------------------------------------------------------------

/// A RESOLVED theme (built-in or loaded), shared by surfaces and painters.
#[derive(uniffi::Object)]
pub struct Theme {
    inner: Arc<exponential_ui::theme::ResolvedTheme>,
}

#[uniffi::export]
impl Theme {
    /// A built-in theme (`neutral`, `exponential`, `playful`).
    #[uniffi::constructor]
    pub fn builtin(id: String) -> Result<Arc<Self>, UiError> {
        let inner = exponential_ui::themes::builtin_theme(&id).ok_or_else(|| invalid(format!("unknown built-in theme {id:?}")))?;
        Ok(Arc::new(Theme { inner }))
    }

    /// Load a theme file over the built-ins (+ `parents_json` = extra sources).
    #[uniffi::constructor]
    pub fn load(theme_json: String, parents_json: Option<String>) -> Result<Arc<Self>, UiError> {
        let theme = load_theme_inner(&theme_json, parents_json.as_deref())?;
        Ok(Arc::new(Theme { inner: Arc::new(theme) }))
    }

    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// The RESOLVED theme as JSON (what `builtin_theme_json` returns).
    pub fn resolved_json(&self) -> String {
        serde_json::to_string(&*self.inner).unwrap_or_default()
    }

    /// A sub-part's look: `owner_component/part` for the OWNER's props (the
    /// recipe props are derived here, like the core does) and `states`.
    pub fn resolve_part(&self, owner_component: String, part: String, owner_props_json: String, states: Vec<String>, mode: String) -> Result<FfiPartStyle, UiError> {
        let props: serde_json::Map<String, Value> = parse(&owner_props_json)?;
        let query = RecipeQuery::new(owner_component.as_str(), part, exponential_ui::recipes::native_recipe_props(&owner_component, &props), states);
        let style = exponential_ui::theme::resolve_recipe(&self.inner, &query, mode_of(&mode)?);
        let v = exponential_ui::style::visual(&style, exponential_ui::style::BoxKind::Leaf);
        Ok(FfiPartStyle { visual: visual(&v), style_json: Value::Object(style).to_string() })
    }

    /// A colour token for the mode (`foreground`, `primary`, `chart1`…), hex.
    pub fn color(&self, name: String, mode: String) -> Option<String> {
        let mode = Mode::parse(&mode)?;
        self.inner.modes.get(mode).color.get(&name).cloned()
    }

    /// Every colour token of the mode as `{name: hex}` JSON.
    pub fn colors_json(&self, mode: String) -> String {
        match Mode::parse(&mode) {
            Some(mode) => serde_json::to_string(&self.inner.modes.get(mode).color).unwrap_or_default(),
            None => "{}".into(),
        }
    }

    pub fn spacing(&self, name: String) -> Option<f64> {
        self.inner.tokens.spacing.get(&name).copied()
    }

    pub fn radius(&self, name: String) -> Option<f64> {
        self.inner.tokens.radius.get(&name).copied()
    }

    /// A control token in px (`input`, `switch`, `iconSm`…).
    pub fn control(&self, name: String) -> Option<f64> {
        self.inner.tokens.control.get(&name).copied()
    }

    pub fn type_size(&self, name: String) -> Option<f64> {
        self.inner.tokens.r#type.size.get(&name).copied()
    }

    pub fn line_height(&self, name: String) -> Option<f64> {
        self.inner.tokens.r#type.line_height.get(&name).copied()
    }

    pub fn opacity(&self, name: String) -> Option<f64> {
        self.inner.tokens.opacity.get(&name).copied()
    }

    /// A family NAME (`sans` → `Inter`); the host registers the font.
    pub fn font_family(&self, kind: String) -> Option<String> {
        self.inner.tokens.r#type.family.get(&kind).cloned()
    }

    /// `{family: {fallback, weights, source}}` JSON.
    pub fn fonts_json(&self) -> String {
        serde_json::to_string(&self.inner.fonts).unwrap_or_default()
    }

    /// The numeric box a control's recipe fixes (`{width, height, …}`), JSON.
    pub fn control_geometry(&self, component: String, props_json: String) -> Result<String, UiError> {
        let props: serde_json::Map<String, Value> = parse(&props_json)?;
        let g = exponential_ui::geometry::control_geometry(&self.inner, &component, &props, &[]);
        serde_json::to_string(&g).map_err(invalid)
    }
}

// ---------------------------------------------------------------------------
// Free functions: the reducer, themes and placement for binding test suites
// and hosts that need them without a surface.
// ---------------------------------------------------------------------------

fn extensions_of(extensions_json: Option<String>) -> Result<Vec<ExtensionDef>, UiError> {
    match extensions_json {
        None => Ok(Vec::new()),
        Some(json) => {
            let list: Vec<ExtensionDef> = parse(&json)?;
            list.into_iter().map(|e| exponential_ui::extension::define_extension(e).map_err(invalid)).collect()
        }
    }
}

/// Reduce a flat component list → `{root, issues}` JSON.
#[uniffi::export]
pub fn reduce_surface_json(components_json: String, catalog_id: String, extensions_json: Option<String>) -> Result<String, UiError> {
    let components: Vec<FlatComponent> = parse(&components_json)?;
    let extensions = extensions_of(extensions_json)?;
    let options = exponential_ui::reducer::ReduceOptions::new(&catalog_id).with_view(exponential_ui::catalog::CatalogView::with(&extensions));
    let result = exponential_ui::reducer::reduce_surface(&components, &options);
    serde_json::to_string(&result).map_err(invalid)
}

/// Reduce the nested authoring form → `{root, issues}` JSON.
#[uniffi::export]
pub fn reduce_nested_json(nested_json: String, catalog_id: String, extensions_json: Option<String>) -> Result<String, UiError> {
    let tree: NestedNode = parse(&nested_json)?;
    let extensions = extensions_of(extensions_json)?;
    let options = exponential_ui::reducer::ReduceOptions::new(&catalog_id).with_view(exponential_ui::catalog::CatalogView::with(&extensions));
    let result = exponential_ui::reducer::reduce_nested(&tree, &options);
    serde_json::to_string(&result).map_err(invalid)
}

/// Validate an extension definition; `[]` when fine.
#[uniffi::export]
pub fn extension_errors(extension_json: String) -> Result<Vec<String>, UiError> {
    let ext: ExtensionDef = parse(&extension_json)?;
    Ok(exponential_ui::extension::validate_extension(&ext))
}

fn load_theme_inner(theme_json: &str, parents_json: Option<&str>) -> Result<exponential_ui::theme::ResolvedTheme, UiError> {
    let source: Value = parse(theme_json)?;
    let mut refs = exponential_ui::themes::builtin_refs();
    if let Some(p) = parents_json {
        let parents: Vec<Value> = parse(p)?;
        refs.extend(parents.into_iter().map(ThemeRef::Source));
    }
    let options = ThemeOptions::core(&refs);
    exponential_ui::theme::try_load_theme(&source, &options).map_err(|issues| UiError::Theme { issues_json: serde_json::to_string(&issues).unwrap_or_default() })
}

/// Load a theme file over the built-ins (+ `parents_json` = extra sources);
/// returns the RESOLVED theme JSON or `UiError::Theme` with the issues.
#[uniffi::export]
pub fn load_theme_json(theme_json: String, parents_json: Option<String>) -> Result<String, UiError> {
    let theme = load_theme_inner(&theme_json, parents_json.as_deref())?;
    serde_json::to_string(&theme).map_err(invalid)
}

/// The issues of a theme file (`[]` when it loads).
#[uniffi::export]
pub fn theme_issues_json(theme_json: String, parents_json: Option<String>) -> String {
    match load_theme_inner(&theme_json, parents_json.as_deref()) {
        Ok(_) => "[]".into(),
        Err(UiError::Theme { issues_json }) => issues_json,
        Err(e) => serde_json::json!([{"path": "theme", "message": e.to_string()}]).to_string(),
    }
}

#[uniffi::export]
pub fn builtin_theme_ids() -> Vec<String> {
    exponential_ui::themes::BUILTIN_THEME_IDS.iter().map(|s| s.to_string()).collect()
}

#[uniffi::export]
pub fn default_theme_id() -> String {
    exponential_ui::themes::DEFAULT_THEME_ID.to_string()
}

/// A built-in theme RESOLVED, as JSON.
#[uniffi::export]
pub fn builtin_theme_json(id: String) -> Option<String> {
    exponential_ui::themes::builtin_theme(&id).map(|t| serde_json::to_string(&*t).unwrap_or_default())
}

/// A part's concrete visuals for one mode (`theme_json` = a RESOLVED theme,
/// e.g. from `builtin_theme_json`), as JSON.
#[uniffi::export]
pub fn resolve_recipe_json(theme_json: String, component: String, part: String, props_json: String, states: Vec<String>, mode: String) -> Result<String, UiError> {
    let theme: exponential_ui::theme::ResolvedTheme = parse(&theme_json)?;
    let props: serde_json::Map<String, Value> = parse(&props_json)?;
    let query = RecipeQuery::new(component, part, props, states);
    let style = exponential_ui::theme::resolve_recipe(&theme, &query, mode_of(&mode)?);
    Ok(Value::Object(style).to_string())
}

/// The numeric box a control's recipe fixes (`{width, height, …}`), as JSON.
#[uniffi::export]
pub fn control_geometry_json(theme_json: String, component: String, props_json: String) -> Result<String, UiError> {
    let theme: exponential_ui::theme::ResolvedTheme = parse(&theme_json)?;
    let props: serde_json::Map<String, Value> = parse(&props_json)?;
    let g = exponential_ui::geometry::control_geometry(&theme, &component, &props, &[]);
    serde_json::to_string(&g).map_err(invalid)
}

/// The overlay placement rule (`side` = `top|right|bottom|left`).
#[uniffi::export]
#[allow(clippy::too_many_arguments)]
pub fn place_overlay(anchor_x: f64, anchor_y: f64, anchor_w: f64, anchor_h: f64, size_w: f64, size_h: f64, viewport_w: f64, viewport_h: f64, side: String) -> Result<FfiPlacement, UiError> {
    let side = exponential_ui::overlay::OverlaySide::parse(&side).ok_or_else(|| invalid(format!("bad side {side:?}")))?;
    let p = exponential_ui::overlay::place_overlay(
        &exponential_ui::overlay::Rect { x: anchor_x, y: anchor_y, width: anchor_w, height: anchor_h },
        &exponential_ui::overlay::Size { width: size_w, height: size_h },
        &exponential_ui::overlay::Size { width: viewport_w, height: viewport_h },
        &exponential_ui::overlay::PlaceOptions::side(side),
    );
    Ok(FfiPlacement { x: p.x, y: p.y, side: p.side.as_str().to_string(), flipped: p.flipped })
}

/// Canonical JSON equality (numbers by value): what the binding test suites
/// compare fixtures with.
#[uniffi::export]
pub fn json_equal(a: String, b: String) -> bool {
    match (serde_json::from_str::<Value>(&a), serde_json::from_str::<Value>(&b)) {
        (Ok(a), Ok(b)) => exponential_ui::json::equal(&a, &b),
        _ => false,
    }
}

/// The first path where two JSON documents differ (`""` when equal).
#[uniffi::export]
pub fn json_diff(a: String, b: String) -> String {
    fn walk(a: &Value, b: &Value, path: &str) -> Option<String> {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for k in x.keys().chain(y.keys()) {
                    match (x.get(k), y.get(k)) {
                        (Some(p), Some(q)) => {
                            if let Some(d) = walk(p, q, &format!("{path}/{k}")) {
                                return Some(d);
                            }
                        }
                        _ => return Some(format!("{path}/{k}")),
                    }
                }
                None
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return Some(format!("{path} (length {} vs {})", x.len(), y.len()));
                }
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    if let Some(d) = walk(p, q, &format!("{path}/{i}")) {
                        return Some(d);
                    }
                }
                None
            }
            _ => {
                if exponential_ui::json::equal(a, b) {
                    None
                } else {
                    Some(format!("{path}: {a} vs {b}"))
                }
            }
        }
    }
    match (serde_json::from_str::<Value>(&a), serde_json::from_str::<Value>(&b)) {
        (Ok(a), Ok(b)) => walk(&a, &b, "").unwrap_or_default(),
        _ => "unparseable".into(),
    }
}

#[uniffi::export]
pub fn core_catalog_id() -> String {
    exponential_ui::catalog::CORE_CATALOG_ID.to_string()
}

#[uniffi::export]
pub fn basic_catalog_id() -> String {
    exponential_ui::catalog::A2UI_BASIC_CATALOG_ID.to_string()
}

/// A synthetic ~n-node surface (nested form) for timing.
#[uniffi::export]
pub fn bench_tree_json(n: u32) -> String {
    exponential_ui::bench::bench_tree_json(n as usize)
}

/// The crate version.
#[uniffi::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------------------------------------------------------------------
// The host API (VAPP-91): `exponential_ui::host`, JSON in and out. The
// platform host owns the I/O (transports, resolvers, function registry) and
// performs the router's ops on its surfaces.
// ---------------------------------------------------------------------------

use exponential_ui::host as h;

fn json_string<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn decision_of(s: &str) -> Result<h::FunctionDecision, UiError> {
    h::FunctionDecision::parse(s).ok_or_else(|| invalid(format!("a decision is allow|ask|deny|not_found, got {s:?}")))
}

/// Server messages in, ops out (`catalog/host.json` ops), one per connection.
#[derive(uniffi::Object)]
pub struct HostRouter {
    inner: Mutex<h::HostRouter>,
}

#[uniffi::export]
impl HostRouter {
    /// `extension_ids` = the extension catalog ids the host registered.
    #[uniffi::constructor]
    pub fn new(extension_ids: Vec<String>) -> Arc<Self> {
        Arc::new(HostRouter { inner: Mutex::new(h::HostRouter::new(&extension_ids)) })
    }

    /// One server message (JSON) → the ops to perform, in order, as a JSON
    /// array (`[{op: create|components|data|bind|delete|send, …}]`). Never
    /// fails: unparseable text is an INVALID_MESSAGE `send` op.
    pub fn route(&self, message_json: String) -> String {
        let ops = match serde_json::from_str::<Value>(&message_json) {
            Ok(message) => self.inner.lock().unwrap().route(&message),
            Err(_) => vec![serde_json::json!({"op": "send", "message": h::error_message(h::INVALID_MESSAGE, "", "a message is a JSON object", None)})],
        };
        Value::Array(ops).to_string()
    }

    /// Install a declarative package (JSON); returns its issues as a JSON
    /// array `[{path, message}]` (installed only when empty).
    pub fn install_package(&self, package_json: String) -> Result<String, UiError> {
        let pkg: Value = parse(&package_json)?;
        Ok(json_string(&self.inner.lock().unwrap().install_package(&pkg)))
    }

    pub fn register_extension(&self, id: String) {
        self.inner.lock().unwrap().register_extension(&id)
    }

    pub fn supported_catalog_ids(&self) -> Vec<String> {
        self.inner.lock().unwrap().supported_catalog_ids()
    }

    /// The live surfaces, in creation order.
    pub fn surface_ids(&self) -> Vec<String> {
        self.inner.lock().unwrap().surface_ids()
    }

    /// The package whose template created the surface (its function policy).
    pub fn package_id_of(&self, surface_id: String) -> Option<String> {
        self.inner.lock().unwrap().package_id_of(&surface_id)
    }
}

/// A2UI JSONL over a byte stream: `push` any chunking, `end` flushes.
/// Both return `{"messages": […], "issues": [{at, message}]}`.
#[derive(uniffi::Object)]
pub struct JsonlDecoder {
    inner: Mutex<h::JsonlDecoder>,
}

#[uniffi::export]
impl JsonlDecoder {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(JsonlDecoder { inner: Mutex::new(h::JsonlDecoder::new()) })
    }

    pub fn push(&self, chunk: String) -> String {
        self.inner.lock().unwrap().push(&chunk).to_json().to_string()
    }

    pub fn end(&self) -> String {
        self.inner.lock().unwrap().end().to_json().to_string()
    }
}

/// Server-Sent Events (`data:` lines per event; an event = one message or
/// JSONL). Same `{messages, issues}` JSON as `JsonlDecoder`.
#[derive(uniffi::Object)]
pub struct SseDecoder {
    inner: Mutex<h::SseDecoder>,
}

#[uniffi::export]
impl SseDecoder {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(SseDecoder { inner: Mutex::new(h::SseDecoder::new()) })
    }

    pub fn push(&self, chunk: String) -> String {
        self.inner.lock().unwrap().push(&chunk).to_json().to_string()
    }

    pub fn end(&self) -> String {
        self.inner.lock().unwrap().end().to_json().to_string()
    }
}

/// A whole JSONL document (or one JSON value / a JSON array) →
/// `{messages, issues}` JSON.
#[uniffi::export]
pub fn decode_jsonl_json(text: String) -> String {
    h::decode_jsonl(&text).to_json().to_string()
}

/// The A2UI messages inside an MCP tool result (JSON) → `{messages, issues}`.
#[uniffi::export]
pub fn messages_from_mcp_result_json(result_json: String) -> Result<String, UiError> {
    let result: Value = parse(&result_json)?;
    Ok(h::messages_from_mcp_result(&result).to_json().to_string())
}

/// A client message as the MCP `tools/call` that carries it back (`tool`
/// defaults to `a2ui_event`).
#[uniffi::export]
pub fn mcp_action_call_json(message_json: String, tool: Option<String>) -> Result<String, UiError> {
    let message: Value = parse(&message_json)?;
    Ok(h::mcp_action_call(&message, tool.as_deref()).to_string())
}

/// The function gate: `allow | ask | deny | not_found`. `policy_json` =
/// `{allow?, ask?, deny?, default?}`.
#[uniffi::export]
pub fn decide_function(policy_json: Option<String>, name: String, registered: bool) -> Result<String, UiError> {
    let policy: Option<h::FunctionPolicy> = policy_json.map(|p| parse(&p)).transpose()?;
    Ok(h::decide_function(policy.as_ref(), &name, registered).as_str().to_string())
}

/// Two stacked decisions: the stricter wins.
#[uniffi::export]
pub fn combine_decisions(a: String, b: String) -> Result<String, UiError> {
    Ok(h::combine_decisions(decision_of(&a)?, decision_of(&b)?).as_str().to_string())
}

/// A package's `functions` list (JSON array, or null) as a policy JSON.
#[uniffi::export]
pub fn package_policy_json(functions_json: Option<String>) -> Result<String, UiError> {
    let functions: Option<Vec<String>> = functions_json.map(|f| parse(&f)).transpose()?;
    Ok(json_string(&h::package_policy(functions.as_deref())))
}

/// openUrl / Link: `{allowed, url?, reason?}` JSON. `policy_json` =
/// `{schemes?, hosts?, baseUrl?}`.
#[uniffi::export]
pub fn decide_url_json(policy_json: Option<String>, url: String) -> Result<String, UiError> {
    let policy: Option<h::UrlPolicy> = policy_json.map(|p| parse(&p)).transpose()?;
    Ok(json_string(&h::decide_url(policy.as_ref(), &url)))
}

/// The media loader's request `{url, headers}` JSON (null when the url does
/// not resolve). `options_json` = `{baseUrl?, rules?: [{prefix, headers}]}`.
#[uniffi::export]
pub fn media_request_json(url: String, options_json: String) -> Result<Option<String>, UiError> {
    let options: h::MediaOptions = parse(&options_json)?;
    Ok(h::media_request(&url, &options).map(|r| json_string(&r)))
}

/// A binding source URI → `{uri, scheme, name, params}` JSON, null when it
/// does not parse.
#[uniffi::export]
pub fn parse_source_json(uri: String) -> Option<String> {
    h::parse_source(&uri).map(|s| json_string(&s))
}

/// The core, the core lite, the basic catalog, then the extension ids.
#[uniffi::export]
pub fn supported_catalog_ids_for(extension_ids: Vec<String>) -> Vec<String> {
    h::supported_catalog_ids(&extension_ids)
}

/// A2UI `a2uiClientCapabilities` JSON.
#[uniffi::export]
pub fn client_capabilities_json(extension_ids: Vec<String>) -> String {
    h::client_capabilities(&extension_ids).to_string()
}

/// A package's issues `[{path, message}]` JSON (`catalog_ids` default: the
/// three built-in catalogs).
#[uniffi::export]
pub fn validate_package_json(package_json: String, catalog_ids: Option<Vec<String>>) -> Result<String, UiError> {
    let pkg: Value = parse(&package_json)?;
    let ids = catalog_ids.unwrap_or_else(|| h::supported_catalog_ids::<&str>(&[]));
    Ok(json_string(&h::validate_package(&pkg, &ids)))
}

/// A package template as its server messages (JSON array), null when the
/// package has no such template. `data_json` = the caller's data.
#[uniffi::export]
pub fn template_messages_json(package_json: String, template_id: String, surface_id: String, data_json: Option<String>) -> Result<Option<String>, UiError> {
    let pkg: Value = parse(&package_json)?;
    let data: Option<Value> = data_json.map(|d| parse(&d)).transpose()?;
    Ok(h::template_messages(&pkg, &template_id, &surface_id, data.as_ref()).map(|m| Value::Array(m).to_string()))
}

/// A client action message (A2UI v0.9 `action`) JSON.
#[uniffi::export]
pub fn action_message_json(surface_id: String, component_id: String, name: String, context_json: String, payload_json: Option<String>, timestamp: String) -> Result<String, UiError> {
    let context: Value = parse(&context_json)?;
    let payload: Option<Value> = payload_json.map(|p| parse(&p)).transpose()?;
    Ok(h::action_message(&surface_id, &component_id, &name, context, payload, &timestamp).to_string())
}

/// A client error message (A2UI v0.9 `error`) JSON.
#[uniffi::export]
pub fn error_message_json(code: String, surface_id: String, message: String, path: Option<String>) -> String {
    h::error_message(&code, &surface_id, &message, path.as_deref()).to_string()
}

/// `catalog/host.json` without its comments.
#[uniffi::export]
pub fn host_contract_json() -> String {
    h::host_contract().to_string()
}
