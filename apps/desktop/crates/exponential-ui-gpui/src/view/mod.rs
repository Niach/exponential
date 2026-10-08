//! [`SurfaceView`]: the gpui entity that owns one Exponential UI surface.
//!
//! Each frame: interaction states are flushed into the core, the core lays
//! the tree out against a [`GpuiMeasure`] (one pass; steady state = memo
//! hits only), and every placed node is painted as ONE absolutely positioned
//! div at its frame relative to its parent's frame — nested like the tree so
//! clipping, opacity and text style inherit — never through gpui's `Styled`
//! flex/grid layout. Open overlay layers paint above the tree as deferred
//! elements at their surface-coordinate frames.

mod events;
mod input;
mod paint;
pub(crate) mod state;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use exponential_ui::measure::TextStyle;
use exponential_ui::surface::{ApplyOutcome, Frame, Layer, ListOutput, Surface, SurfaceOptions};
use exponential_ui::theme::{Mode, ResolvedTheme};
use exponential_ui::themes::default_theme;
use exponential_ui::{ExtensionDef, NestedNode, CORE_CATALOG_ID};
use gpui::{canvas, deferred, div, prelude::*, px, Bounds, Context, FocusHandle, Font, Hsla, Pixels, ScrollHandle, Subscription, Task, WeakEntity, Window};

use crate::extension::ExtensionPainter;
use crate::host::{HostPlugin, NoHost};
use crate::measure::{make_font, Fonts, GpuiMeasure};
use crate::paint::parts::default_ink;
use crate::paint::PaintStyle;
use state::NodeCache;

pub(crate) use input::{Field, Popup};
pub use state::{focus_order, is_focusable, next_focus};

/// The surface width until the first paint reports the element's own width.
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
}

impl Default for SurfaceViewOptions {
    fn default() -> Self {
        SurfaceViewOptions { surface_id: "surface".into(), catalog_id: CORE_CATALOG_ID.into(), theme: Some(default_theme()), mode: Mode::Dark, extensions: Vec::new(), host: Rc::new(NoHost), rounding: false }
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
}

/// Hover / pressed / focus flags of one node (merged into `set_states`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Interaction {
    pub hover: bool,
    pub pressed: bool,
    pub focus: bool,
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
        out
    }
}

/// A painter-local value of a control whose prop is not bound (the core only
/// writes bound values through): shown until the external prop changes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mirror {
    pub external: serde_json::Value,
    pub local: serde_json::Value,
}

/// An in-flight Slider drag.
#[derive(Debug, Clone)]
pub(crate) struct Drag {
    pub track: String,
    pub value: f64,
}

/// The gpui entity owning one surface.
pub struct SurfaceView {
    surface: Surface,
    host: Rc<dyn HostPlugin>,
    fonts: Fonts,
    painters: HashMap<String, Rc<dyn ExtensionPainter>>,
    stats: PassStats,
    width: f32,
    viewport_height: f32,
    nodes_dirty: bool,
    cache: NodeCache,
    frames: Vec<Frame>,
    styles: Vec<PaintStyle>,
    texts: Vec<TextStyle>,
    node_fonts: Vec<Font>,
    inks: Vec<Hsla>,
    ink_base: Option<Hsla>,
    layers: Vec<Layer>,
    lists: HashMap<u32, ListOutput>,
    surface_height: f32,
    interaction: HashMap<String, Interaction>,
    states_dirty: HashSet<String>,
    pressed: Option<String>,
    root_focus: FocusHandle,
    focus_handles: HashMap<String, FocusHandle>,
    focused: Option<u32>,
    fields: HashMap<String, Field>,
    revisions: HashMap<String, u64>,
    mirrors: HashMap<String, Mirror>,
    popup: Option<Popup>,
    tooltip: Option<(String, Task<()>)>,
    open_layers: Vec<String>,
    layer_return: HashMap<String, String>,
    just_dismissed: Option<String>,
    unknown_version: Option<u64>,
    list_scrolls: HashMap<String, ScrollHandle>,
    list_offsets: Rc<RefCell<HashMap<String, f32>>>,
    drag: Option<Drag>,
    slider_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    this: WeakEntity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

impl SurfaceView {
    pub fn new(options: SurfaceViewOptions, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let surface = Surface::new(&options.surface_id, SurfaceOptions { catalog_id: options.catalog_id, theme: options.theme.clone(), mode: options.mode, extensions: options.extensions, rounding: options.rounding });
        let fonts = Fonts::new(options.host.clone(), options.theme.as_deref());
        SurfaceView {
            surface,
            host: options.host,
            fonts,
            painters: HashMap::new(),
            stats: PassStats::default(),
            width: DEFAULT_WIDTH,
            viewport_height: 0.0,
            nodes_dirty: true,
            cache: NodeCache::default(),
            frames: Vec::new(),
            styles: Vec::new(),
            texts: Vec::new(),
            node_fonts: Vec::new(),
            inks: Vec::new(),
            ink_base: None,
            layers: Vec::new(),
            lists: HashMap::new(),
            surface_height: 0.0,
            interaction: HashMap::new(),
            states_dirty: HashSet::new(),
            pressed: None,
            root_focus: cx.focus_handle(),
            focus_handles: HashMap::new(),
            focused: None,
            fields: HashMap::new(),
            revisions: HashMap::new(),
            mirrors: HashMap::new(),
            popup: None,
            tooltip: None,
            open_layers: Vec::new(),
            layer_return: HashMap::new(),
            just_dismissed: None,
            unknown_version: None,
            list_scrolls: HashMap::new(),
            list_offsets: Rc::new(RefCell::new(HashMap::new())),
            drag: None,
            slider_bounds: Rc::new(RefCell::new(HashMap::new())),
            this: cx.entity().downgrade(),
            _subscriptions: Vec::new(),
        }
    }

    /// Apply one A2UI server→client message.
    pub fn apply(&mut self, message: &serde_json::Value, cx: &mut Context<Self>) -> Result<ApplyOutcome, String> {
        let out = self.surface.apply(message);
        self.nodes_dirty = true;
        cx.notify();
        out
    }

    /// The nested authoring form (fixtures, templates).
    pub fn set_nested(&mut self, tree: NestedNode, cx: &mut Context<Self>) -> ApplyOutcome {
        let out = self.surface.set_nested(tree);
        self.nodes_dirty = true;
        cx.notify();
        out
    }

    /// Write `value` at `path` of the data model (`None` removes).
    pub fn set_data(&mut self, path: &str, value: Option<serde_json::Value>, cx: &mut Context<Self>) {
        self.surface.set_data(path, value);
        self.nodes_dirty = true;
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Option<Arc<ResolvedTheme>>, cx: &mut Context<Self>) {
        self.fonts.set_theme(theme.as_deref());
        self.surface.set_theme(theme);
        self.nodes_dirty = true;
        // Every cached font/visual re-reads (the default family moved).
        self.styles.clear();
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.surface.set_mode(mode);
        cx.notify();
    }

    /// Paint (and measure) extension nodes of `kind` with `painter`.
    pub fn register_painter(&mut self, kind: impl Into<String>, painter: Box<dyn ExtensionPainter>) {
        self.painters.insert(kind.into(), Rc::from(painter));
        self.surface.invalidate_measures();
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
    /// Dialogs centre in it). 0 = unknown.
    pub fn set_viewport_height(&mut self, height: f32, cx: &mut Context<Self>) {
        if (height - self.viewport_height).abs() > 0.5 {
            self.viewport_height = height.max(0.0);
            cx.notify();
        }
    }

    /// The surface width the next pass lays out at (normally probed from the
    /// element's own bounds, one frame late).
    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
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

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cache = NodeCache::build(&mut self.surface);
        self.nodes_dirty = false;
        if self.unknown_version != Some(self.cache.version) {
            self.unknown_version = Some(self.cache.version);
            for n in self.cache.nodes.iter().filter(|n| n.component == "Unknown") {
                self.host.on_unknown(n);
            }
        }
        // Focus handles for focusable non-field nodes, kept by id.
        let mut handles = HashMap::new();
        for n in &self.cache.nodes {
            if state::is_focusable(n) && !state::is_text_field(n) {
                let h = self.focus_handles.remove(&n.id).unwrap_or_else(|| cx.focus_handle());
                handles.insert(n.id.clone(), h);
            }
            if n.component == "List" {
                self.list_scrolls.entry(n.id.clone()).or_default();
            }
        }
        self.focus_handles = handles;
        self.sync_fields(window, cx);
        if let Some(p) = &self.popup {
            if self.cache.index_of(&p.field).is_none() {
                self.popup = None;
            }
        }
    }

    /// One layout pass: states in, layout, caches refreshed.
    fn pass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let max_h = (self.viewport_height > 0.0).then_some(self.viewport_height);
        self.surface.set_viewport(self.width, 0.0, max_h);
        self.track_focus(window, cx);
        self.flush_states();
        if self.nodes_dirty || self.surface.structure_version() != self.cache.version || (self.cache.nodes.is_empty() && self.surface.root().is_some()) {
            self.sync(window, cx);
        }
        let theme = self.surface.theme().cloned();
        let mode = self.surface.mode();
        let started = Instant::now();
        let mut calls = 0;
        let mut layout_ns = 0;
        let mut changed: HashSet<u32> = HashSet::new();
        let mut tries = 0;
        let out = loop {
            let mut m = GpuiMeasure::new(window, cx, &self.fonts, theme.as_deref(), mode, &self.painters, &self.cache.kinds);
            let out = self.surface.layout(&mut m);
            calls += m.calls();
            layout_ns += out.layout_ns;
            changed.extend(out.visual_changes.iter().copied());
            tries += 1;
            if out.structure_version == self.cache.version || tries >= 3 {
                break out;
            }
            self.sync(window, cx);
        };
        let wall_ns = started.elapsed().as_nanos() as u64;
        let n = self.cache.nodes.len();
        // Frames by index: the main tree, then every layer's.
        self.frames.clear();
        self.frames.resize(n, Frame::default());
        for f in &out.frames {
            if let Some(slot) = self.frames.get_mut(f.index as usize) {
                *slot = Frame { x: f.x, y: f.y, w: f.w, h: f.h };
            }
        }
        for l in &out.layers {
            for f in &l.frames {
                if let Some(slot) = self.frames.get_mut(f.index as usize) {
                    *slot = Frame { x: f.x, y: f.y, w: f.w, h: f.h };
                }
            }
        }
        // Visual caches: only what the core reports changed.
        let all = self.styles.len() != n;
        if all {
            self.styles = vec![PaintStyle::default(); n];
            self.texts = vec![TextStyle::default(); n];
            self.node_fonts = vec![make_font(self.fonts.default.clone(), 400); n];
        }
        let indices: Vec<u32> = if all { (0..n as u32).collect() } else { changed.iter().copied().filter(|i| (*i as usize) < n).collect() };
        for i in &indices {
            let i = *i as usize;
            if let Some(v) = self.surface.visual(i as u32) {
                self.styles[i] = PaintStyle::from_visual(v);
            }
            if let Some(t) = self.surface.text_style(i as u32) {
                self.node_fonts[i] = self.fonts.font(t.font_family.as_deref(), t.font_weight);
                self.texts[i] = t.clone();
            }
        }
        let base = default_ink(theme.as_deref(), mode);
        if all || !indices.is_empty() || self.inks.len() != n || self.ink_base != Some(base) {
            self.ink_base = Some(base);
            self.inks = Vec::with_capacity(n);
            for i in 0..n {
                let inherited = self.cache.nodes[i].parent.and_then(|p| self.inks.get(p as usize).copied()).unwrap_or(base);
                let own = self.styles[i].color.unwrap_or(inherited);
                self.inks.push(own);
            }
        }
        self.lists = out.lists.iter().map(|l| (l.node, l.clone())).collect();
        self.surface_height = out.surface_height;
        self.layers_changed(&out.layers, window, cx);
        self.layers = out.layers;
        self.stats.nodes = n;
        self.stats.measure_calls = calls;
        self.stats.layout_ns = layout_ns;
        self.stats.wall_ns = wall_ns;
        self.stats.passes += 1;
        self.stats.structure_version = out.structure_version;
        self.stats.upcalls = out.upcalls;
        self.stats.measure_rounds = out.measure_rounds;
        if std::env::var_os("EXP_UI_TRACE").is_some() {
            eprintln!(
                "[exponential-ui gpui] {} pass {} width {:.0}: {} nodes · {} measure calls · {} upcalls · core {} µs · wall {} µs · layers {}",
                self.surface.id,
                self.stats.passes,
                self.width,
                n,
                calls,
                out.upcalls,
                layout_ns / 1000,
                wall_ns / 1000,
                self.layers.len()
            );
        }
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
        make_font(self.fonts.default.clone(), 400)
    }
}

impl Render for SurfaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.pass(window, cx);
        let theme = self.surface.theme().cloned();
        let ink = default_ink(theme.as_deref(), self.surface.mode());
        let this = self.this.clone();
        let probe = canvas(
            move |bounds, _, cx| {
                let width = f32::from(bounds.size.width);
                if let Some(this) = this.upgrade() {
                    this.update(cx, |this, cx| this.set_width(width, cx));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(1.0));
        let mut root = div()
            .id("exponential-ui-surface")
            .relative()
            .w_full()
            .h(px(self.surface_height))
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_move(cx.listener(Self::on_drag_move))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_drag_end))
            .font(self.default_font())
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(ink)
            .child(probe);
        if let Some(root_node) = self.cache.nodes.first() {
            if !root_node.hidden && root_node.layer == 0 {
                root = root.child(self.paint_node(0, (0.0, 0.0), window, cx));
            }
        }
        for (k, layer) in self.layers.iter().enumerate() {
            let el = self.paint_layer(layer, window, cx);
            root = root.child(deferred(el).with_priority(k + 1));
        }
        if self.popup.is_some() {
            if let Some(el) = self.paint_popup(window, cx) {
                root = root.child(deferred(el).with_priority(self.layers.len() + 10));
            }
        }
        root
    }
}
