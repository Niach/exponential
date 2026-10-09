//! [`SurfaceView`]: the gpui entity that owns one Exponential UI surface.
//!
//! The view renders a [`SurfaceElement`]: a gpui element whose LAYOUT asks
//! the core (a measured gpui layout node: the available width in, the
//! surface height out), so the very first frame lays out at the element's
//! real width — no probe, no jump. Its PREPAINT knows the element's bounds
//! and the host's clip, so it moves windowed lists and the visible region
//! (Dialogs centre in what the user SEES) in the same frame, then builds the
//! element tree: ONE absolutely positioned div per placed node at its frame
//! relative to its parent's frame — nested like the tree so clipping,
//! opacity and text style inherit — never through gpui's `Styled`
//! flex/grid layout. Scroll containers translate their descendants by the
//! core's offsets; open layers paint above the tree as deferred elements.

mod events;
mod input;
mod motion;
mod paint;
mod pinned;
pub(crate) mod state;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use exponential_ui::layout_tree::ToastSpec;
use exponential_ui::measure::{FixedMeasure, Measure, TextStyle};
use exponential_ui::surface::{ApplyOutcome, Frame, Layer, ListOutput, PlacedNode, ScrollOutput, Surface, SurfaceOptions, SurfaceSettings};
use exponential_ui::theme::{Mode, ModeSetting, ResolvedTheme};
use exponential_ui::themes::default_theme;
use exponential_ui::{ExtensionDef, FlatComponent, NestedNode, CORE_CATALOG_ID};
use gpui::{
    deferred, div, prelude::*, px, AnyElement, App, AvailableSpace, Bounds, Context, Element, ElementId, Entity, FocusHandle, Font, GlobalElementId, Hsla, InspectorElementId, LayoutId, Pixels, SharedString,
    Size, Style, Subscription, Task, WeakEntity, Window, WindowAppearance,
};

use crate::extension::ExtensionPainter;
use crate::host::{HostPlugin, NoHost};
use crate::measure::{make_font, Fonts, GpuiMeasure};
use crate::paint::markdown::Block;
use crate::paint::parts::default_ink;
use crate::paint::PaintStyle;
use motion::Motion;
use state::NodeCache;

pub(crate) use input::Field;
pub use state::{focus_order, is_focusable, next_focus, NodeFlags};

/// The surface width before the element knows its own (no host width and
/// no window yet).
pub const DEFAULT_WIDTH: f32 = 900.0;

/// How a [`SurfaceView`] is created.
pub struct SurfaceViewOptions {
    pub surface_id: String,
    pub catalog_id: String,
    /// `None` = geometry mode (no recipes, styles verbatim).
    pub theme: Option<Arc<ResolvedTheme>>,
    pub mode: Mode,
    pub extensions: Vec<ExtensionDef>,
    pub host: Rc<dyn HostPlugin>,
    pub rounding: bool,
    /// The width the surface lays out at before gpui's layout reports the
    /// element's own (`None` = the window's width).
    pub width: Option<f32>,
    /// Locale, built-in string overrides, `system` mode, density, contrast,
    /// font scale, hover, reduced motion, safe-area insets, today. `None` =
    /// the core defaults with `mode` from above.
    pub settings: Option<SurfaceSettings>,
    /// Expand form controls into their parts (default: when themed).
    pub expand_controls: Option<bool>,
    /// Lay out with the core's FIXED measure instead of gpui's text system
    /// (golden geometry: the shared `layout-geometry*.json` fixtures).
    pub fixed_measure: Option<FixedMeasure>,
    /// Round 2 §3: the host's number / date / relative-time formatter
    /// (`None` = the core's English one). The view re-binds once a minute so
    /// relative times move.
    pub formatter: Option<Arc<dyn exponential_ui::format::Formatter>>,
}

impl Default for SurfaceViewOptions {
    fn default() -> Self {
        SurfaceViewOptions {
            surface_id: "surface".into(),
            catalog_id: CORE_CATALOG_ID.into(),
            theme: Some(default_theme()),
            mode: Mode::Dark,
            extensions: Vec::new(),
            host: Rc::new(NoHost),
            rounding: false,
            width: None,
            settings: None,
            expand_controls: None,
            fixed_measure: None,
            formatter: None,
        }
    }
}

/// Numbers of the LAST layout pass.
#[derive(Clone, Debug, Default)]
pub struct PassStats {
    pub nodes: usize,
    /// Per-request measure calls answered (0 = every leaf came from the memo).
    pub measure_calls: u64,
    /// The core's own time (taffy + restyle + rebuild), summed over retries.
    pub layout_ns: u64,
    /// The whole layout call including measurement.
    pub wall_ns: u64,
    /// Passes since the view was created.
    pub passes: u64,
    pub structure_version: u64,
    pub upcalls: u32,
    pub measure_rounds: u32,
    /// Nodes the core restyled / built this pass.
    pub restyled: u32,
    pub built_nodes: u32,
    /// The width the pass laid out at.
    pub width: f32,
}

/// Hover / pressed / focus flags of one node (merged into `set_states`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Interaction {
    pub hover: bool,
    pub pressed: bool,
    pub focus: bool,
    /// Focus that arrived from the keyboard (`:focus-visible`).
    pub focus_visible: bool,
    pub dragover: bool,
}

impl Interaction {
    pub fn states(self) -> Vec<String> {
        let mut out = Vec::new();
        if self.hover {
            out.push("hover".to_string());
        }
        if self.pressed {
            out.push("pressed".to_string());
        }
        if self.focus {
            out.push("focus".to_string());
        }
        if self.focus_visible {
            out.push("focus-visible".to_string());
        }
        if self.dragover {
            out.push("dragover".to_string());
        }
        out
    }
}

/// An in-flight Slider drag.
#[derive(Debug, Clone)]
pub(crate) struct Drag {
    pub track: String,
    pub value: f64,
}

/// An in-flight Resizable handle drag (round 2 §1): the core resizes from
/// the sizes at the START, so only the pointer's start is kept.
#[derive(Debug, Clone)]
pub(crate) struct ResizeDrag {
    pub handle: String,
    pub vertical: bool,
    pub start: f32,
}

/// An in-flight scrollbar thumb drag.
#[derive(Debug, Clone)]
pub(crate) struct ScrollDrag {
    pub id: String,
    pub vertical: bool,
    pub start_pointer: f32,
    pub start_offset: f32,
    /// Content px per pointer px.
    pub ratio: f32,
}

/// An in-flight Drawer drag (toward its edge to dismiss).
#[derive(Debug, Clone)]
pub(crate) struct SheetDrag {
    pub root: u32,
    /// The edge the sheet hangs from (`bottom`, `right`…).
    pub side: String,
    pub start: (f32, f32),
    pub offset: (f32, f32),
}

/// The part of the surface the host shows (surface coordinates): the
/// visible left edge, top and height, from the element's clip.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisibleRegion {
    pub left: f32,
    pub top: f32,
    pub height: f32,
}

/// A running toast timer.
pub(crate) struct ToastTimer {
    pub remaining_ms: f64,
    pub started: Option<Instant>,
    pub task: Option<Task<()>>,
}

/// Parsed markdown per Markdown leaf (by slot), keyed by its text.
type MarkdownCache = HashMap<u32, (String, Rc<Vec<Block>>)>;

/// What a closing layer last painted, renumbered into its own small slot
/// space (`0..n`, root first) so nothing of the live caches (which a
/// compaction may renumber the same pass) leaks into it: painting a ghost
/// swaps this world in for the live one ([`SurfaceView::swap_world`]).
#[derive(Default)]
pub(crate) struct GhostWorld {
    cache: NodeCache,
    frames: Vec<Frame>,
    styles: Vec<PaintStyle>,
    texts: Vec<TextStyle>,
    fonts: Vec<Font>,
    inks: Vec<Hsla>,
    scrolls: HashMap<u32, ScrollOutput>,
    lists: HashMap<u32, ListOutput>,
    chart_hover: HashMap<u32, usize>,
}

/// A layer that just closed: painted from its last state, fading (and
/// sinking back) out over the theme's `motion.fast`, never interactive.
/// `layer` is renumbered into `world`'s slots.
pub(crate) struct ExitingLayer {
    layer: Layer,
    start: Instant,
    duration_ms: f32,
    world: GhostWorld,
}

/// The slot caches as they were before a pass rebuilt them (a compaction
/// renumbered the slots, or the host touched `surface_mut`): the source of
/// a closing layer's ghost.
struct PrePass {
    cache: NodeCache,
    styles: Vec<PaintStyle>,
    texts: Vec<TextStyle>,
    fonts: Vec<Font>,
    inks: Vec<Hsla>,
}

/// What a reader is told about one node (or one built-in sub-control).
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleInfo {
    pub role: gpui::Role,
    pub label: Option<SharedString>,
    pub description: Option<SharedString>,
    /// A heading's level (a List section header = 3).
    pub level: Option<usize>,
    /// A list item's place in the WHOLE list: (position, set size).
    pub position: Option<(usize, usize)>,
}

/// Painted window bounds by node id ([`SurfaceView::record_bounds`]).
type BoundsLog = Rc<RefCell<HashMap<String, Bounds<Pixels>>>>;

/// The gpui entity owning one surface.
pub struct SurfaceView {
    surface: Surface,
    host: Rc<dyn HostPlugin>,
    fonts: Fonts,
    painters: HashMap<String, Rc<dyn ExtensionPainter>>,
    stats: PassStats,
    width: f32,
    /// The host fixed the width (`set_width`): gpui's layout does not move it.
    width_fixed: bool,
    viewport_height: f32,
    visible: VisibleRegion,
    origin: gpui::Point<Pixels>,
    nodes_dirty: bool,
    cache: NodeCache,
    frames: Vec<Frame>,
    /// The main tree's paint order (= a11y order) and each layer's.
    order: Vec<u32>,
    layer_orders: Vec<Vec<u32>>,
    styles: Vec<PaintStyle>,
    texts: Vec<TextStyle>,
    node_fonts: Vec<Font>,
    inks: Vec<Hsla>,
    ink_base: Option<Hsla>,
    layers: Vec<Layer>,
    lists: HashMap<u32, ListOutput>,
    scrolls: HashMap<u32, ScrollOutput>,
    toasts: Vec<ToastSpec>,
    rtl: bool,
    surface_height: f32,
    interaction: HashMap<String, Interaction>,
    states_dirty: HashSet<String>,
    pressed: Option<String>,
    root_focus: FocusHandle,
    focus_handles: HashMap<String, FocusHandle>,
    focused: Option<u32>,
    /// The last focus move came from the keyboard.
    keyboard: bool,
    pending_focus: Option<String>,
    /// The hover-opened trigger keyboard focus sits in.
    focus_trigger: Option<String>,
    fields: HashMap<String, Field>,
    revisions: HashMap<String, u64>,
    hover_timer: Option<(String, Task<()>)>,
    /// The core's hover-overlay close delays (`OutEvent::HoverTimer`).
    hover_close_timers: HashMap<String, Task<()>>,
    open_layers: Vec<String>,
    layer_return: HashMap<String, String>,
    just_dismissed: Option<String>,
    unknown_version: Option<u64>,
    /// The host scroller's offset over the surface last given to the core.
    host_scroll: Option<(f32, f32)>,
    drag: Option<Drag>,
    scroll_drag: Option<ScrollDrag>,
    sheet_drag: Option<SheetDrag>,
    resize_drag: Option<ResizeDrag>,
    /// Round 2: the core's sticky offsets of the last pass (node → dx, dy).
    sticky: HashMap<u32, (f32, f32)>,
    /// Round 2: when each animated node entered the tree (by painted id).
    anim_start: RefCell<HashMap<SharedString, Instant>>,
    slider_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    motion: Motion,
    toast_timers: HashMap<String, ToastTimer>,
    toast_hover: HashSet<String>,
    announcement: Option<(SharedString, String)>,
    copy_resets: HashMap<String, Task<()>>,
    /// ToggleGroup roving item (`group id` → item index).
    group_focus: HashMap<String, usize>,
    /// A calendar day to focus once its month is built (keyboard paging).
    pending_day: Option<(String, String)>,
    /// The category / slice under the pointer, per Chart.
    chart_hover: HashMap<u32, usize>,
    markdown: RefCell<MarkdownCache>,
    /// Layers fading out after they closed.
    exiting: Vec<ExitingLayer>,
    /// Markdown text units: registered while building, committed once
    /// painted (hit-testing needs laid-out text).
    md_pending: crate::paint::markdown::MdUnits,
    md_units: crate::paint::markdown::MdUnits,
    md_selection: Option<crate::paint::markdown::MdSelection>,
    /// The selected leaf's id + text hash when the selection was made: a
    /// change of either (a streamed chunk, a reused slot) drops it.
    md_selection_key: Option<(String, u64)>,
    /// A pointer drag is extending the Markdown selection.
    md_dragging: bool,
    /// The element being built is an exiting layer (no listeners, no focus,
    /// no a11y node).
    ghosting: Cell<bool>,
    /// Added to the motion clock ([`Self::advance_clock`]).
    clock_skew: std::time::Duration,
    debug_bounds: Option<BoundsLog>,
    this: WeakEntity<SurfaceView>,
    /// A measure replacing the gpui text system (golden geometry: the core's
    /// FIXED measure, `SurfaceViewOptions::fixed_measure` or
    /// [`SurfaceView::set_measure`]); `None` = [`GpuiMeasure`].
    measure_override: Option<Box<dyn Measure>>,
    /// The node indices `paint_node` visited in the last render, in paint
    /// order (only while [`SurfaceView::trace_paint`] is on).
    paint_trace: RefCell<Option<Vec<u32>>>,
    /// A bind since the last minute tick formatted a relative time.
    clock_read: Arc<AtomicBool>,
    _subscriptions: Vec<Subscription>,
    _ticker: Task<()>,
}

/// The surface's formatter, noting when a bind formats a relative time (the
/// only output the clock moves), so only those surfaces tick.
struct ClockWatch {
    inner: Arc<dyn exponential_ui::format::Formatter>,
    read: Arc<AtomicBool>,
}

impl exponential_ui::format::Formatter for ClockWatch {
    fn locale(&self) -> String {
        self.inner.locale()
    }
    fn number(&self, value: f64, options: exponential_ui::format::NumberOptions) -> String {
        self.inner.number(value, options)
    }
    fn currency(&self, value: f64, code: &str, options: exponential_ui::format::NumberOptions) -> String {
        self.inner.currency(value, code, options)
    }
    fn percent(&self, value: f64, decimals: Option<u32>) -> String {
        self.inner.percent(value, decimals)
    }
    fn date(&self, value: exponential_ui::format::DateValue, options: &exponential_ui::format::DateOptions) -> String {
        self.inner.date(value, options)
    }
    fn relative_time(&self, value: i64, unit: exponential_ui::format::RelativeUnit) -> String {
        self.read.store(true, Ordering::SeqCst);
        self.inner.relative_time(value, unit)
    }
    fn plural(&self, value: f64) -> exponential_ui::format::PluralCategory {
        self.inner.plural(value)
    }
}

impl SurfaceView {
    pub fn new(options: SurfaceViewOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self::build_view(options, Some(window), cx);
        view._subscriptions.push(cx.observe_window_appearance(window, |this: &mut SurfaceView, window, cx| {
            if this.surface.settings().mode == ModeSetting::System {
                this.sync_appearance(window);
                cx.notify();
            }
        }));
        view
    }

    /// [`SurfaceView::new`] outside a window update (a host runtime creating
    /// a surface when its transport delivers `createSurface`, VAPP-91). The
    /// width is the host's (else [`DEFAULT_WIDTH`]) until the element lays
    /// out; `mode: system` re-reads the appearance on every pass.
    pub fn without_window(options: SurfaceViewOptions, cx: &mut Context<Self>) -> Self {
        Self::build_view(options, None, cx)
    }

    fn build_view(options: SurfaceViewOptions, window: Option<&mut Window>, cx: &mut Context<Self>) -> Self {
        let mut surface = Surface::new(&options.surface_id, SurfaceOptions { catalog_id: options.catalog_id, theme: options.theme.clone(), mode: options.mode, extensions: options.extensions, rounding: options.rounding, expand_controls: options.expand_controls });
        if let Some(settings) = options.settings.clone() {
            surface.set_settings(settings);
        } else {
            surface.set_mode(options.mode);
        }
        let clock_read = Arc::new(AtomicBool::new(false));
        let base = options.formatter.clone().unwrap_or_else(|| surface.formatter().clone());
        surface.set_formatter(Arc::new(ClockWatch { inner: base, read: clock_read.clone() }));
        // Relative times move: a tree whose last bind formatted one re-binds
        // once a minute (§3); the others never tick.
        let read = clock_read.clone();
        let ticker = cx.spawn(async move |this: WeakEntity<SurfaceView>, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(60)).await;
            if !read.swap(false, Ordering::SeqCst) {
                continue;
            }
            if this
                .update(cx, |v, cx| {
                    v.surface.tick();
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        });
        let fonts = Fonts::new(options.host.clone(), options.theme.as_deref(), cx.text_system().clone());
        let window_size = window.map(|w| w.viewport_size()).map(|s| (f32::from(s.width), f32::from(s.height))).unwrap_or((0.0, 0.0));
        let width = options.width.filter(|w| *w > 0.0).unwrap_or(if window_size.0 > 0.0 { window_size.0 } else { DEFAULT_WIDTH });
        SurfaceView {
            surface,
            host: options.host,
            fonts,
            painters: HashMap::new(),
            stats: PassStats::default(),
            width,
            width_fixed: options.width.is_some(),
            viewport_height: 0.0,
            // Until the first prepaint sees the clip: the window.
            visible: VisibleRegion { left: 0.0, top: 0.0, height: window_size.1 },
            origin: gpui::Point::default(),
            nodes_dirty: true,
            cache: NodeCache::default(),
            frames: Vec::new(),
            order: Vec::new(),
            layer_orders: Vec::new(),
            styles: Vec::new(),
            texts: Vec::new(),
            node_fonts: Vec::new(),
            inks: Vec::new(),
            ink_base: None,
            layers: Vec::new(),
            lists: HashMap::new(),
            scrolls: HashMap::new(),
            toasts: Vec::new(),
            rtl: false,
            surface_height: 0.0,
            interaction: HashMap::new(),
            states_dirty: HashSet::new(),
            pressed: None,
            root_focus: cx.focus_handle(),
            focus_handles: HashMap::new(),
            focused: None,
            keyboard: false,
            pending_focus: None,
            focus_trigger: None,
            fields: HashMap::new(),
            revisions: HashMap::new(),
            hover_timer: None,
            hover_close_timers: HashMap::new(),
            open_layers: Vec::new(),
            layer_return: HashMap::new(),
            just_dismissed: None,
            unknown_version: None,
            host_scroll: None,
            drag: None,
            scroll_drag: None,
            sheet_drag: None,
            resize_drag: None,
            sticky: HashMap::new(),
            anim_start: RefCell::new(HashMap::new()),
            slider_bounds: Rc::new(RefCell::new(HashMap::new())),
            motion: Motion::default(),
            toast_timers: HashMap::new(),
            toast_hover: HashSet::new(),
            announcement: None,
            copy_resets: HashMap::new(),
            group_focus: HashMap::new(),
            pending_day: None,
            chart_hover: HashMap::new(),
            markdown: RefCell::new(HashMap::new()),
            exiting: Vec::new(),
            md_pending: Rc::default(),
            md_units: Rc::default(),
            md_selection: None,
            md_selection_key: None,
            md_dragging: false,
            ghosting: Cell::new(false),
            clock_skew: std::time::Duration::ZERO,
            debug_bounds: None,
            this: cx.entity().downgrade(),
            measure_override: options.fixed_measure.map(|m| Box::new(m) as Box<dyn Measure>),
            paint_trace: RefCell::new(None),
            _subscriptions: Vec::new(),
            clock_read,
            _ticker: ticker,
        }
    }

    /// Round 2 §3: format through the host's formatter (built from the
    /// surface's locale + time zone); the tree re-binds.
    pub fn set_formatter(&mut self, formatter: Arc<dyn exponential_ui::format::Formatter>, cx: &mut Context<Self>) {
        self.surface.set_formatter(Arc::new(ClockWatch { inner: formatter, read: self.clock_read.clone() }));
        cx.notify();
    }

    /// Whether a bind since the last minute tick formatted a relative time
    /// (the surface re-binds at the next tick; the others never tick).
    pub fn ticks_minutely(&self) -> bool {
        self.clock_read.load(Ordering::SeqCst)
    }

    /// Pin the clock relative times read (epoch ms; `None` = the wall clock).
    pub fn set_clock(&mut self, now_ms: Option<f64>, cx: &mut Context<Self>) {
        self.surface.set_clock(now_ms);
        cx.notify();
    }

    /// Apply one A2UI server→client message.
    pub fn apply(&mut self, message: &serde_json::Value, cx: &mut Context<Self>) -> Result<ApplyOutcome, String> {
        let out = self.surface.apply(message);
        cx.notify();
        out
    }

    /// Replace the flat component list (what a host's `components` op
    /// carries, already merged by id).
    pub fn set_components(&mut self, components: Vec<FlatComponent>, cx: &mut Context<Self>) -> ApplyOutcome {
        let out = self.surface.set_components(components);
        self.nodes_dirty = true;
        cx.notify();
        out
    }

    /// The nested authoring form (fixtures, templates).
    pub fn set_nested(&mut self, tree: NestedNode, cx: &mut Context<Self>) -> ApplyOutcome {
        let out = self.surface.set_nested(tree);
        cx.notify();
        out
    }

    /// Write `value` at `path` of the data model (`None` removes).
    pub fn set_data(&mut self, path: &str, value: Option<serde_json::Value>, cx: &mut Context<Self>) {
        self.surface.set_data(path, value);
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Option<Arc<ResolvedTheme>>, cx: &mut Context<Self>) {
        self.fonts.set_theme(theme.as_deref());
        self.surface.set_theme(theme);
        // Every cached font/visual re-reads (the default family moved), and
        // the node cache's `hover_styled` (recipes moved).
        self.styles.clear();
        self.nodes_dirty = true;
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        let mut s = self.surface.settings().clone();
        s.mode = match mode {
            Mode::Light => ModeSetting::Light,
            Mode::Dark => ModeSetting::Dark,
        };
        self.surface.set_settings(s);
        self.surface.set_mode(mode);
        cx.notify();
    }

    /// Locale, strings, `system` mode, density, contrast, font scale, hover,
    /// reduced motion, insets, today (`mode: system` follows the window's
    /// appearance live).
    pub fn set_settings(&mut self, settings: SurfaceSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.surface.set_settings(settings);
        self.sync_appearance(window);
        cx.notify();
    }

    fn sync_appearance(&mut self, window: &Window) {
        if self.surface.settings().mode != ModeSetting::System {
            return;
        }
        let dark = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
        if self.surface.settings().system_dark != dark {
            self.surface.set_mode_setting(ModeSetting::System, dark);
        }
    }

    /// Fonts were registered with the text system after this view measured:
    /// every family re-resolves and every leaf is measured again.
    pub fn fonts_changed(&mut self, cx: &mut Context<Self>) {
        self.fonts.invalidate(self.surface.theme().map(|t| t.as_ref()));
        self.surface.invalidate_measures();
        self.node_fonts.clear();
        cx.notify();
    }

    /// Paint (and measure) extension nodes of `kind` with `painter`.
    pub fn register_painter(&mut self, kind: impl Into<String>, painter: Box<dyn ExtensionPainter>) {
        self.painters.insert(kind.into(), Rc::from(painter));
        self.surface.invalidate_measures();
    }

    /// [`SurfaceView::register_painter`] with a painter shared between
    /// views (a host runtime hands one painter to every surface).
    pub fn register_painter_rc(&mut self, kind: impl Into<String>, painter: Rc<dyn ExtensionPainter>) {
        self.painters.insert(kind.into(), painter);
        self.surface.invalidate_measures();
    }

    /// Lay out with `measure` instead of the gpui text system (`None` =
    /// back to [`GpuiMeasure`]). Conformance runs the fixed geometry measure
    /// through the painter this way.
    pub fn set_measure(&mut self, measure: Option<Box<dyn Measure>>, cx: &mut Context<Self>) {
        self.measure_override = measure;
        self.surface.invalidate_measures();
        cx.notify();
    }

    /// The placed nodes of the last pass (index = layout node index).
    pub fn placed_nodes(&self) -> &[PlacedNode] {
        &self.cache.nodes
    }

    /// The frame (surface coordinates) the painter positions node `index`'s
    /// div at, as of the last pass.
    pub fn frame(&self, index: u32) -> Option<Frame> {
        self.frames.get(index as usize).copied()
    }

    /// The lines a `Text` leaf paints, as of the last pass (`None` for any
    /// other node): 0 for an empty text, 1 for a one-line or chrome text,
    /// else its text broken at the content width like the painter — never
    /// the frame height over the line height (a stretched text keeps its
    /// lines). The conformance dump's `lines`.
    pub fn painted_lines(&self, index: u32, window: &Window) -> Option<u32> {
        let i = index as usize;
        let n = self.cache.nodes.get(i).filter(|n| n.index == index && n.component == "Text")?;
        let ts = self.texts.get(i)?;
        let raw = crate::measure::display_text(n.props.get("text"));
        let shown = crate::text::transform(&raw, ts.text_transform.as_deref()).into_owned();
        if shown.is_empty() {
            return Some(0);
        }
        let chrome = crate::measure::text_chrome(n.owner_component.as_deref().unwrap_or(""), n.part.as_deref(), &n.props, self.styles.get(i).map(|s| s.gap).unwrap_or(0.0), 0.0);
        if n.lines == Some(1) || chrome != (0.0, 0.0) {
            return Some(1);
        }
        let f = self.frames.get(i)?;
        let [t, r, b, l] = self.styles.get(i).map(|s| s.insets()).unwrap_or([0.0; 4]);
        let _ = (t, b);
        let font = self.node_fonts.get(i).cloned().unwrap_or_else(|| self.fonts.for_style(ts));
        let mut shaper = crate::measure::Shaper::new(window, &self.fonts).tracked(ts);
        let lines = shaper.lines(&shown, &font, ts.font_size, Some((f.w - l - r).max(1.0))).len() as u32;
        Some(n.lines.filter(|c| *c > 0).map_or(lines, |c| lines.min(c)))
    }

    /// The content height of the last pass.
    pub fn surface_height(&self) -> f32 {
        self.surface_height
    }

    /// Record the order `render` paints nodes in ([`SurfaceView::paint_trace`]).
    pub fn trace_paint(&mut self, on: bool) {
        *self.paint_trace.borrow_mut() = on.then(Vec::new);
    }

    /// The node indices painted by the last render, in paint order (empty
    /// unless [`SurfaceView::trace_paint`] is on).
    pub fn paint_trace(&self) -> Vec<u32> {
        self.paint_trace.borrow().clone().unwrap_or_default()
    }

    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    /// Direct access; the next frame re-reads the node list.
    pub fn surface_mut(&mut self) -> &mut Surface {
        self.nodes_dirty = true;
        &mut self.surface
    }

    /// Numbers of the last layout pass.
    pub fn stats(&self) -> &PassStats {
        &self.stats
    }

    /// The host's visible height (windowed lists size their range off it;
    /// Dialogs centre in it). 0 = the element's clip decides.
    pub fn set_viewport_height(&mut self, height: f32, cx: &mut Context<Self>) {
        if (height - self.viewport_height).abs() > 0.5 {
            self.viewport_height = height.max(0.0);
            cx.notify();
        }
    }

    /// Fix the surface width (gpui's layout no longer moves it).
    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.width_fixed = true;
        if width > 0.0 && (width - self.width).abs() > 0.5 {
            self.width = width;
            cx.notify();
        }
    }

    /// The layout node index of a component id.
    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.cache.index_of(id).or_else(|| self.surface.index_of(id))
    }

    /// Run one layout pass now (render does this every frame).
    pub fn layout_now(&mut self, window: &mut Window, cx: &mut Context<Self>) -> &PassStats {
        self.pass(window, cx);
        &self.stats
    }

    /// The focused node (as of the last pass or focus move).
    pub fn focused(&self) -> Option<u32> {
        self.focused
    }

    /// The open overlay layers of the last pass, in paint order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The surface direction of the last pass.
    pub fn is_rtl(&self) -> bool {
        self.rtl
    }

    /// The visible region the last frame used (surface coordinates).
    pub fn visible_region(&self) -> VisibleRegion {
        self.visible
    }

    /// The last announcement (`announce`, a live region, a Form error):
    /// `(text, politeness)`. It is also exposed as a `status` a11y node.
    pub fn announcement(&self) -> Option<(SharedString, String)> {
        self.announcement.clone()
    }

    /// The interaction states the painter set on a node (`hover`,
    /// `pressed`, `focus`, `focus-visible`, `dragover`).
    pub fn node_states(&self, id: &str) -> Vec<String> {
        self.interaction.get(id).copied().unwrap_or_default().states()
    }

    /// Record every painted node's window bounds (a test and automation aid;
    /// adds one probe per node while on).
    pub fn record_bounds(&mut self, on: bool) {
        self.debug_bounds = on.then(|| Rc::new(RefCell::new(HashMap::new())));
    }

    /// A painted node's window bounds (with [`Self::record_bounds`] on).
    pub fn painted_bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.debug_bounds.as_ref().and_then(|b| b.borrow().get(id).copied())
    }

    /// What the painter fills a node with and the text colour it draws in
    /// (inherited when the node sets none), as of the last frame: a test
    /// and automation aid like [`Self::painted_bounds`].
    pub fn painted_colors(&self, id: &str) -> Option<(Option<Hsla>, Hsla)> {
        let i = self.index_of(id)? as usize;
        Some((self.styles.get(i).and_then(|s| s.bg), *self.inks.get(i)?))
    }

    /// The surface's origin in window coordinates (as of the last frame).
    pub fn origin(&self) -> gpui::Point<Pixels> {
        self.origin
    }

    /// The text field values typed but maybe not yet flushed.
    fn live_texts(&self, cx: &App) -> HashMap<String, String> {
        self.fields.iter().map(|(id, f)| (id.clone(), f.value(cx))).collect()
    }

    /// One layout pass: states in, layout, caches refreshed.
    pub(crate) fn pass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible_h = if self.viewport_height > 0.0 { self.viewport_height } else { self.visible.height };
        let max_h = (visible_h > 0.0).then_some(visible_h);
        self.sync_appearance(window);
        self.surface.set_viewport(self.width, 0.0, max_h);
        self.track_focus(window, cx);
        self.flush_states();
        // While a layer is open, keep what this pass replaces: a layer that
        // closes paints it fading out. A rebuild (the host touched the
        // surface, a compaction renumbered the slots) moves the whole
        // previous cache aside; a delta patch keeps only the replaced layer
        // nodes.
        let keep_old = !self.layers.is_empty() && !self.surface.settings().reduced_motion;
        let mut pre: Option<PrePass> = None;
        let mut old_nodes: HashMap<u32, PlacedNode> = HashMap::new();
        if self.nodes_dirty {
            let fresh = NodeCache::build(&mut self.surface);
            self.retire_caches(fresh, keep_old, &mut pre);
            self.nodes_dirty = false;
        }
        let theme = self.surface.theme().cloned();
        let mode = self.surface.mode();
        let started = Instant::now();
        let mut calls = 0;
        let mut layout_ns = 0;
        let mut changed: HashSet<u32> = HashSet::new();
        let mut tries = 0;
        let live = self.live_texts(cx);
        let out = loop {
            let (out, unknown) = if let Some(m) = self.measure_override.as_mut() {
                (self.surface.layout(m.as_mut()), Vec::new())
            } else {
                let mut m = GpuiMeasure::new(window, cx, &self.fonts, theme.as_deref(), mode, &self.painters, &self.cache.kinds, &live);
                let out = self.surface.layout(&mut m);
                calls += m.calls();
                (out, std::mem::take(&mut m.unknown_extensions))
            };
            layout_ns += out.layout_ns;
            changed.extend(out.visual_changes.iter().copied());
            changed.extend(out.delta.added.iter().copied());
            changed.extend(out.delta.changed.iter().copied());
            if self.cache.nodes.is_empty() || out.delta.renumbered {
                let fresh = NodeCache::build(&mut self.surface);
                self.retire_caches(fresh, keep_old, &mut pre);
            } else {
                // Old versions only in the numbering `pre` (if any) was
                // taken in: never after a rebuild this pass.
                self.cache.patch(&mut self.surface, &out.delta, (keep_old && pre.is_none()).then_some(&mut old_nodes));
            }
            tries += 1;
            // An extension leaf measured before its kind was known: measure
            // it again now that the cache knows it.
            if unknown.is_empty() || tries >= 3 {
                break out;
            }
            for i in unknown {
                self.surface.mark_dirty(i);
            }
        };
        let wall_ns = started.elapsed().as_nanos() as u64;
        self.after_layout(out, changed, old_nodes, pre, theme.as_deref(), mode, window, cx);
        self.stats.measure_calls = calls;
        self.stats.layout_ns = layout_ns;
        self.stats.wall_ns = wall_ns;
        self.stats.passes += 1;
        self.stats.width = self.width;
        if std::env::var_os("EXP_UI_TRACE").is_some() {
            eprintln!(
                "[exponential-ui gpui] {} pass {} width {:.0}: {} nodes · {} measure calls · {} upcalls · {} restyled · {} built · core {} µs · wall {} µs · layers {}",
                self.surface.id,
                self.stats.passes,
                self.width,
                self.stats.nodes,
                calls,
                self.stats.upcalls,
                self.stats.restyled,
                self.stats.built_nodes,
                layout_ns / 1000,
                wall_ns / 1000,
                self.layers.len()
            );
        }
    }

    /// Install a rebuilt node cache. With `keep` (a layer is open) the
    /// previous slot caches move into `pre` (the first rebuild of a pass
    /// wins: it holds what was painted); else they are dropped.
    fn retire_caches(&mut self, fresh: NodeCache, keep: bool, pre: &mut Option<PrePass>) {
        let cache = std::mem::replace(&mut self.cache, fresh);
        let styles = std::mem::take(&mut self.styles);
        if keep && pre.is_none() {
            *pre = Some(PrePass { cache, styles, texts: std::mem::take(&mut self.texts), fonts: std::mem::take(&mut self.node_fonts), inks: std::mem::take(&mut self.inks) });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn after_layout(&mut self, out: exponential_ui::surface::LayoutOutput, changed: HashSet<u32>, old_nodes: HashMap<u32, PlacedNode>, pre: Option<PrePass>, theme: Option<&ResolvedTheme>, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.cache.nodes.len();
        // Layers that closed this pass keep painting, fading out (captured
        // before the caches below move on); a reopened one drops its ghost.
        self.exiting.retain(|g| !out.layers.iter().any(|l| l.owner == g.layer.owner && l.layer == g.layer.layer));
        if !self.surface.settings().reduced_motion {
            let exit_ms = self.surface.theme().and_then(|t| t.tokens.motion.get("fast").copied()).unwrap_or(120.0) as f32;
            let closed: Vec<Layer> = self.layers.iter().filter(|l| !out.layers.iter().any(|o| o.owner == l.owner && o.layer == l.layer)).cloned().collect();
            for layer in closed {
                if let Some(ghost) = self.ghost_of(layer, &old_nodes, pre.as_ref(), exit_ms) {
                    self.exiting.push(ghost);
                }
            }
        }
        if self.unknown_version != Some(self.cache.version) {
            self.unknown_version = Some(self.cache.version);
            for node in self.cache.nodes.iter().filter(|x| !x.removed && x.component == "Unknown") {
                self.host.on_unknown(node);
            }
        }
        self.sync_handles(window, cx);
        // Frames by slot: the main tree, then every layer's.
        let previous = std::mem::take(&mut self.frames);
        self.frames = vec![Frame::default(); n];
        self.order = out.frames.iter().map(|f| f.index).collect();
        for f in &out.frames {
            if let Some(slot) = self.frames.get_mut(f.index as usize) {
                *slot = Frame { x: f.x, y: f.y, w: f.w, h: f.h };
            }
        }
        self.layer_orders = out.layers.iter().map(|l| l.frames.iter().map(|f| f.index).collect()).collect();
        for l in &out.layers {
            for f in &l.frames {
                if let Some(slot) = self.frames.get_mut(f.index as usize) {
                    *slot = Frame { x: f.x, y: f.y, w: f.w, h: f.h };
                }
            }
        }
        // Visual caches: only what the core reports changed (or new slots).
        let all = self.styles.len() != n || self.node_fonts.len() != n;
        if all {
            self.styles = vec![PaintStyle::default(); n];
            self.texts = vec![TextStyle::default(); n];
            self.node_fonts = vec![make_font(self.fonts.default.clone(), 400); n];
        }
        let indices: Vec<u32> = if all { (0..n as u32).collect() } else { changed.iter().copied().filter(|i| (*i as usize) < n).collect() };
        let reduced = self.surface.settings().reduced_motion;
        let now = self.clock();
        for i in &indices {
            let i = *i as usize;
            if let Some(v) = self.surface.visual(i as u32) {
                let next = PaintStyle::from_visual(v);
                if !all && !reduced {
                    self.motion.style_changed(i as u32, &self.styles[i], &next, now);
                }
                self.styles[i] = next;
            }
            if let Some(t) = self.surface.text_style(i as u32) {
                self.node_fonts[i] = self.fonts.for_style(t);
                self.texts[i] = t.clone();
            }
        }
        if !reduced {
            for (i, f) in self.frames.iter().enumerate() {
                if let (Some(old), Some(style)) = (previous.get(i), self.styles.get(i)) {
                    if let Some(t) = style.transition.filter(|_| old.w > 0.0 && (old.w != f.w || old.h != f.h)) {
                        self.motion.frame_changed(i as u32, *old, *f, t, now);
                    }
                }
            }
        }
        let base = default_ink(theme, mode);
        self.compute_inks(base);
        self.lists = out.lists.iter().map(|l| (l.node, l.clone())).collect();
        self.scrolls = out.scrolls.iter().map(|s| (s.index, *s)).collect();
        self.sticky = out.sticky.iter().map(|s| (s.index, (s.dx, s.dy))).collect();
        // An animation restarts when its node re-enters the tree.
        if all || !out.delta.removed.is_empty() {
            let ids = &self.cache.by_id;
            self.anim_start.borrow_mut().retain(|id, _| ids.contains_key(id.as_ref()));
        }
        self.rtl = out.direction == "rtl";
        self.surface_height = out.surface_height;
        self.sync_toasts(&out.toasts, window, cx);
        self.toasts = out.toasts.clone();
        let layers = out.layers;
        self.layers_changed(&layers, now, window, cx);
        self.layers = layers;
        self.stats.nodes = self.cache.nodes.iter().filter(|x| !x.removed).count();
        self.stats.structure_version = out.structure_version;
        self.stats.upcalls = out.upcalls;
        self.stats.measure_rounds = out.measure_rounds;
        self.stats.restyled = out.restyled;
        self.stats.built_nodes = out.built_nodes;
        if let Some(id) = self.pending_focus.take() {
            self.focus_id(&id, window, cx);
        }
        // Only a rebuilt or changed slot can hold other text.
        if self.md_selection.is_some_and(|sel| (all || changed.contains(&sel.node)) && self.md_text_key(sel.node) != self.md_selection_key) {
            self.md_selection = None;
            self.md_selection_key = None;
            self.md_dragging = false;
        }
        let late = self.surface.take_events();
        if !late.is_empty() {
            self.dispatch(late, None, cx);
        }
    }

    /// Text colour inheritance in TREE order (slots are not pre-order).
    fn compute_inks(&mut self, base: Hsla) {
        let n = self.cache.nodes.len();
        self.ink_base = Some(base);
        self.inks = vec![base; n];
        let mut roots: Vec<u32> = self.order.first().copied().into_iter().collect();
        roots.extend(self.layers_roots());
        let mut stack: Vec<(u32, Hsla)> = roots.into_iter().map(|r| (r, base)).collect();
        while let Some((i, inherited)) = stack.pop() {
            let Some(node) = self.cache.nodes.get(i as usize) else { continue };
            let own = self.styles.get(i as usize).and_then(|s| s.color).unwrap_or(inherited);
            self.inks[i as usize] = own;
            for c in &node.children {
                stack.push((*c, own));
            }
        }
    }

    /// A closed layer's last painted state (`None` when nothing of it is
    /// left to paint), renumbered into its own [`GhostWorld`]. The source is
    /// the pre-pass caches: `pre` when this pass rebuilt them, else the live
    /// ones with `old` (the node versions the delta replaced) over them.
    /// Only nodes OF the layer are taken (plus the owners its parts name,
    /// for their recipe props).
    fn ghost_of(&self, layer: Layer, old: &HashMap<u32, PlacedNode>, pre: Option<&PrePass>, duration_ms: f32) -> Option<ExitingLayer> {
        if duration_ms <= 0.0 {
            return None;
        }
        let (cache, styles, texts, fonts, inks) = match pre {
            Some(p) => (&p.cache, &p.styles, &p.texts, &p.fonts, &p.inks),
            None => (&self.cache, &self.styles, &self.texts, &self.node_fonts, &self.inks),
        };
        let node_at = |i: u32| old.get(&i).or_else(|| cache.nodes.get(i as usize).filter(|n| !n.removed));
        // Old slot → ghost slot, in the layer's paint order (root first).
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut taken: Vec<(u32, Frame)> = Vec::new();
        for f in &layer.frames {
            if node_at(f.index).is_some_and(|n| n.layer == layer.layer) && !map.contains_key(&f.index) {
                map.insert(f.index, taken.len() as u32);
                taken.push((f.index, Frame { x: f.x, y: f.y, w: f.w, h: f.h }));
            }
        }
        if !map.contains_key(&layer.root) || taken.iter().all(|(i, _)| node_at(*i).is_none_or(|n| n.hidden)) {
            return None;
        }
        // The owners the layer's parts name (a Dialog's content → the
        // Dialog), so recipe lookups resolve: never painted (no parent).
        let owners: Vec<u32> = taken
            .iter()
            .filter_map(|(i, _)| node_at(*i).and_then(|n| n.owner.as_deref()))
            .chain(std::iter::once(layer.owner.as_str()))
            .filter_map(|id| cache.by_id.get(id).copied())
            .filter(|i| !map.contains_key(i))
            .collect();
        for i in owners {
            if node_at(i).is_some() && !map.contains_key(&i) {
                map.insert(i, taken.len() as u32);
                taken.push((i, Frame::default()));
            }
        }
        let mut world = GhostWorld::default();
        for (k, (i, frame)) in taken.iter().enumerate() {
            let slot = *i as usize;
            let mut n = node_at(*i).cloned()?;
            n.index = k as u32;
            n.parent = n.parent.and_then(|p| map.get(&p).copied());
            n.children = n.children.iter().filter_map(|c| map.get(c).copied()).collect();
            world.cache.by_id.insert(n.id.clone(), k as u32);
            if let Some(kind) = &n.extension_kind {
                world.cache.kinds.insert(n.id.clone(), kind.clone());
            }
            world.cache.ids.push(SharedString::from(n.id.clone()));
            world.cache.hover_styled.push(false);
            world.cache.nodes.push(n);
            world.frames.push(*frame);
            world.styles.push(styles.get(slot).cloned().unwrap_or_default());
            world.texts.push(texts.get(slot).cloned().unwrap_or_default());
            world.fonts.push(fonts.get(slot).cloned().unwrap_or_else(|| self.default_font()));
            world.inks.push(inks.get(slot).copied().unwrap_or(gpui::white()));
            // Scroll offsets, list windows and a chart's tooltip as painted
            // (these maps still hold the pre-pass state).
            if let Some(s) = self.scrolls.get(i) {
                world.scrolls.insert(k as u32, ScrollOutput { index: k as u32, ..*s });
            }
            if let Some(l) = self.lists.get(i) {
                world.lists.insert(k as u32, ListOutput { node: k as u32, ..l.clone() });
            }
            if let Some(h) = self.chart_hover.get(i) {
                world.chart_hover.insert(k as u32, *h);
            }
        }
        let frames = layer.frames.iter().filter_map(|f| map.get(&f.index).map(|k| exponential_ui::surface::PlacedFrame { index: *k, ..*f })).collect();
        let layer = Layer { root: map[&layer.root], frames, ..layer };
        Some(ExitingLayer { layer, start: self.now_or_instant(), duration_ms, world })
    }

    fn now_or_instant(&self) -> Instant {
        self.motion.now.unwrap_or_else(|| self.clock())
    }

    /// The motion clock: now, plus what [`Self::advance_clock`] added.
    fn clock(&self) -> Instant {
        Instant::now() + self.clock_skew
    }

    /// Move the motion clock forward (tests and automation: an exit fade
    /// or a transition finishes without waiting in real time).
    pub fn advance_clock(&mut self, by: std::time::Duration, cx: &mut Context<Self>) {
        self.clock_skew += by;
        cx.notify();
    }

    /// Swap a ghost's world in for the live slot caches (call again to swap
    /// back).
    fn swap_world(&mut self, w: &mut GhostWorld) {
        std::mem::swap(&mut self.cache, &mut w.cache);
        std::mem::swap(&mut self.frames, &mut w.frames);
        std::mem::swap(&mut self.styles, &mut w.styles);
        std::mem::swap(&mut self.texts, &mut w.texts);
        std::mem::swap(&mut self.node_fonts, &mut w.fonts);
        std::mem::swap(&mut self.inks, &mut w.inks);
        std::mem::swap(&mut self.scrolls, &mut w.scrolls);
        std::mem::swap(&mut self.lists, &mut w.lists);
        std::mem::swap(&mut self.chart_hover, &mut w.chart_hover);
    }

    /// The layers still fading out after they closed (owner ids).
    pub fn exiting_layers(&self) -> Vec<String> {
        self.exiting.iter().map(|g| g.layer.owner.clone()).collect()
    }

    /// The accessibility a node is painted with (an inspection aid: the
    /// platform tree is only built while a screen reader is on). Also
    /// answers the built-in sub-controls: `<composer>.send`,
    /// `<composer>.attach`, `<indicator>.dot.<i>`.
    pub fn accessible_info(&self, id: &str) -> Option<AccessibleInfo> {
        if let Some(n) = self.cache.index_of(id).and_then(|i| self.cache.node(i)) {
            return self.node_a11y(n);
        }
        let (base, rest) = id.split_once('.')?;
        let button = |label: String| AccessibleInfo { role: gpui::Role::Button, label: Some(label.into()), description: None, level: None, position: None };
        if let Some(n) = self.cache.index_of(base).and_then(|i| self.cache.node(i)).filter(|n| n.component == "Composer") {
            let (send, attach) = self.composer_labels(n);
            return match rest {
                "send" => Some(button(send)),
                "attach" => Some(button(attach)),
                _ => None,
            };
        }
        // `<indicator id>.dot.<i>` (the indicator id holds dots itself).
        let (owner, page) = id.rsplit_once(".dot.")?;
        let page: usize = page.parse().ok()?;
        let n = self.cache.index_of(owner).and_then(|i| self.cache.node(i))?;
        let total = n.props.get("count").and_then(serde_json::Value::as_f64).unwrap_or(0.0).max(0.0) as usize;
        (page < total).then(|| AccessibleInfo { role: gpui::Role::Tab, label: Some(self.carousel_dot_label(page, total).into()), description: None, level: None, position: Some((page + 1, total)) })
    }

    /// What a fading layer paints (an inspection aid): its nodes in paint
    /// order (root first) as `(id, has a background)`.
    pub fn exiting_nodes(&self, owner: &str) -> Vec<(String, bool)> {
        let Some(g) = self.exiting.iter().find(|g| g.layer.owner == owner) else { return Vec::new() };
        g.layer.frames.iter().filter_map(|f| g.world.cache.node(f.index).map(|n| (n.id.clone(), g.world.styles.get(f.index as usize).is_some_and(|s| s.bg.is_some())))).collect()
    }

    fn layers_roots(&self) -> Vec<u32> {
        self.layer_orders.iter().filter_map(|o| o.first().copied()).collect()
    }

    /// Focus handles for focusable non-field nodes, kept by id; field states.
    fn sync_handles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut handles = HashMap::new();
        for n in self.cache.nodes.iter().filter(|n| !n.removed) {
            if (state::is_focusable(n) || self.scroll_focusable(n.index)) && !state::is_text_field(n) {
                let h = self.focus_handles.remove(&n.id).unwrap_or_else(|| cx.focus_handle());
                handles.insert(n.id.clone(), h);
            }
        }
        self.focus_handles = handles;
        self.sync_fields(window, cx);
    }

    /// A scroll container that overflows takes keyboard focus (to scroll).
    fn scroll_focusable(&self, index: u32) -> bool {
        self.scrolls.get(&index).is_some_and(|s| (s.scroll_y && s.content_height > self.frames.get(index as usize).map(|f| f.h).unwrap_or(0.0) + 0.5) || (s.scroll_x && s.content_width > self.frames.get(index as usize).map(|f| f.w).unwrap_or(0.0) + 0.5))
    }

    fn flush_states(&mut self) {
        for id in std::mem::take(&mut self.states_dirty) {
            let states = self.interaction.get(&id).copied().unwrap_or_default().states();
            if states.is_empty() {
                self.interaction.remove(&id);
            }
            self.surface.set_states(&id, states);
        }
    }

    pub(crate) fn set_interaction(&mut self, id: &str, f: impl FnOnce(&mut Interaction)) -> bool {
        let entry = self.interaction.entry(id.to_string()).or_default();
        let before = *entry;
        f(entry);
        let changed = before != *entry;
        if changed {
            self.states_dirty.insert(id.to_string());
        }
        changed
    }

    fn default_font(&self) -> Font {
        self.fonts.font(None, 400, false)
    }

    /// The layout gpui asks for: a pass at `width` (the available width),
    /// answering the surface's size.
    fn measure_at(&mut self, width: Option<f32>, window: &mut Window, cx: &mut Context<Self>) -> Size<Pixels> {
        match width.filter(|w| *w > 0.0 && w.is_finite()) {
            Some(w) => {
                if !self.width_fixed && (w - self.width).abs() > 0.25 {
                    self.width = w;
                }
            }
            // A content-size probe from the host's layout: answer the last
            // pass (no layout at a width the element will not get).
            None if self.stats.passes > 0 => return Size { width: px(self.width), height: px(self.surface_height) },
            None => {}
        }
        self.pass(window, cx);
        Size { width: px(self.width), height: px(self.surface_height) }
    }

    /// Prepaint: the element's bounds and clip are known. Move the visible
    /// region and the windowed lists the host scrolls, re-laying out once
    /// when they moved, then build the element tree.
    fn build(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.origin = bounds.origin;
        let mask = window.content_mask().bounds;
        let top = (f32::from(mask.origin.y) - f32::from(bounds.origin.y)).max(0.0);
        let bottom = f32::from(mask.origin.y + mask.size.height) - f32::from(bounds.origin.y);
        let left = (f32::from(mask.origin.x) - f32::from(bounds.origin.x)).max(0.0);
        let region = VisibleRegion { left, top, height: (bottom - top).max(0.0) };
        let mut again = false;
        if (region.height - self.visible.height).abs() > 0.5 && self.viewport_height <= 0.0 {
            again = true;
        }
        self.visible = region;
        // Round 2 §5: the host scrolls the WHOLE surface: windowed lists
        // without a bounded size window against it and sticky nodes without
        // a scrolling ancestor pin against it (the core decides which).
        let host_scroll = (region.left, region.top);
        if self.host_scroll.is_none_or(|(x, y)| (x - host_scroll.0).abs() >= 1.0 || (y - host_scroll.1).abs() >= 1.0) {
            self.host_scroll = Some(host_scroll);
            if self.surface.set_surface_scroll(host_scroll.0, host_scroll.1) {
                again = true;
            }
        }
        if !self.width_fixed && (f32::from(bounds.size.width) - self.width).abs() > 0.5 {
            self.width = f32::from(bounds.size.width);
            again = true;
        }
        if again {
            self.pass(window, cx);
        }
        let now = self.clock();
        self.motion.now = Some(now);
        self.exiting.retain(|g| now.saturating_duration_since(g.start).as_secs_f32() * 1000.0 < g.duration_ms);
        if self.motion.active(now) || !self.exiting.is_empty() {
            window.request_animation_frame();
        }
        self.paint_root(window, cx)
    }

    /// The element tree: the main tree, the layers (deferred), the a11y
    /// status node for announcements.
    fn paint_root(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(t) = self.paint_trace.borrow_mut().as_mut() {
            t.clear();
        }
        let theme = self.surface.theme().cloned();
        let ink = default_ink(theme.as_deref(), self.surface.mode());
        let mut root = div()
            .id("exponential-ui-surface")
            .relative()
            .w(px(self.width))
            .h(px(self.surface_height))
            .track_focus(&self.root_focus)
            .key_context("ExponentialUiSurface")
            .on_key_down(cx.listener(Self::on_key))
            // Any pointer press ends keyboard mode (no `:focus-visible`) and
            // drops a Markdown selection (a press in Markdown starts anew).
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                this.keyboard = false;
                if this.md_selection.take().is_some_and(|s| !s.is_empty()) {
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(Self::on_pointer_move))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_pointer_up))
            .font(self.default_font())
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(ink);
        // A drag in flight (slider, scrollbar, sheet, text selection) follows the pointer
        // anywhere in the window, over layers and outside the surface too.
        if self.drag.is_some() || self.scroll_drag.is_some() || self.sheet_drag.is_some() || self.resize_drag.is_some() || self.md_dragging {
            let this = self.this.clone();
            root = root.child(
                gpui::canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let (a, b) = (this.clone(), this.clone());
                        window.on_mouse_event(move |ev: &gpui::MouseMoveEvent, phase, window, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                let _ = a.update(cx, |v, cx| v.on_pointer_move(ev, window, cx));
                            }
                        });
                        window.on_mouse_event(move |ev: &gpui::MouseUpEvent, phase, window, cx| {
                            if phase == gpui::DispatchPhase::Capture && ev.button == gpui::MouseButton::Left {
                                let _ = b.update(cx, |v, cx| v.on_pointer_up(ev, window, cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        if let Some(&main) = self.order.first() {
            if let Some(el) = self.paint_node(main, paint::Place { origin: (0.0, 0.0), abs: (0.0, 0.0), clip: None }, window, cx) {
                root = root.child(el);
            }
        }
        // Closed layers fade out under the open ones (toasts under toasts).
        let mut ghosts = std::mem::take(&mut self.exiting);
        let now = self.now_or_instant();
        for (k, g) in ghosts.iter_mut().enumerate() {
            let t = now.saturating_duration_since(g.start).as_secs_f32() * 1000.0 / g.duration_ms;
            let fade = 1.0 - crate::paint::cubic_bezier(motion::ENTER_EASING, t.clamp(0.0, 1.0));
            self.swap_world(&mut g.world);
            self.ghosting.set(true);
            let el = self.paint_layer_at(k, &g.layer, Some(fade), window, cx);
            self.ghosting.set(false);
            self.swap_world(&mut g.world);
            let priority = match g.layer.class {
                exponential_ui::layout_tree::LayerClass::Toast => 1000,
                _ => 0,
            };
            root = root.child(deferred(el).with_priority(priority));
        }
        self.exiting = ghosts;
        for k in 0..self.layers.len() {
            let layer = self.layers[k].clone();
            let el = self.paint_layer(k, &layer, window, cx);
            let priority = match layer.class {
                exponential_ui::layout_tree::LayerClass::Toast => 1000 + k,
                _ => k + 1,
            };
            root = root.child(deferred(el).with_priority(priority));
        }
        if let Some((text, _)) = self.announcement.clone() {
            root = root.child(div().id("exponential-ui-announcement").absolute().size(px(1.0)).overflow_hidden().opacity(0.0).role(gpui::Role::Status).aria_label(text));
        }
        root.into_any_element()
    }
}

/// The element a [`SurfaceView`] renders (see the module docs).
pub struct SurfaceElement {
    view: Entity<SurfaceView>,
}

impl IntoElement for SurfaceElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for SurfaceElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, _cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        let view = self.view.clone();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            if std::env::var_os("EXP_UI_TRACE").is_some() {
                eprintln!("[exponential-ui gpui] measure known {known:?} available {available:?}");
            }
            let width = known.width.map(f32::from).or(match available.width {
                AvailableSpace::Definite(w) => Some(f32::from(w)),
                _ => None,
            });
            view.update(cx, |v, cx| v.measure_at(width, window, cx))
        });
        (id, ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let mut el = self.view.update(cx, |v, cx| v.build(bounds, window, cx));
        el.prepaint_as_root(bounds.origin, bounds.size.map(AvailableSpace::Definite), window, cx);
        Some(el)
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), prepaint: &mut Option<AnyElement>, window: &mut Window, cx: &mut App) {
        if let Some(el) = prepaint {
            el.paint(window, cx);
        }
    }
}

impl Render for SurfaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        SurfaceElement { view: cx.entity() }
    }
}
