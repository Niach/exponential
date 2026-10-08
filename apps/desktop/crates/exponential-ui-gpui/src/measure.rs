//! The in-process measure (the VAPP-4 verdict): taffy's leaf questions are
//! answered per request through gpui's text system and gpui's line-layout
//! cache absorbs the repeats. `None` wrap = max-content, `Some(0.0)` =
//! min-content. Every answer is the BORDER box of the control: the leaf's
//! `ControlBox` padding/border are added around the shaped content and its
//! fixed/minimum sizes win; text leaves also answer their FIRST BASELINE.
//!
//! Plain text breaks where the web breaks ([`crate::text`]: UAX #14
//! opportunities, hanging spaces) and the painter draws the very lines the
//! measurer counted, so the reserved height is the painted height. Fonts
//! resolve through the host's mapping, the CSS generic names
//! (`ui-monospace`, `system-ui`…) and an availability check (a family the
//! text system cannot load becomes the platform's own sans / mono instead of
//! gpui's last-resort stack), with a glyph fallback chain for CJK and emoji.
//! The measurer's identity hashes the resolved fonts, the font epoch
//! ([`crate::view::SurfaceView::fonts_changed`]) and the theme scale, so a
//! late font registration re-measures everything.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use exponential_ui::measure::{ControlBox, HeightRequest, Intrinsics, LeafRequest, Measure, TextStyle};
use exponential_ui::theme::{Mode, ResolvedTheme};
use gpui::{px, App, Font, FontFallbacks, FontStyle, FontWeight, SharedString, TextRun, TextSystem, Window};
use serde_json::{Map, Value};

use crate::extension::ExtensionPainter;
use crate::host::HostPlugin;
use crate::paint::markdown::{self, Inline, MdStyles, MdText, TextSpec};
use crate::paint::parts::{control, default_family, mono_family, part_props, px_prop, spacing};
use crate::text;

/// The base of the identity the core keys its memo on.
const GPUI_MEASURE_BASE: u64 = 0x6770_7569;

/// Glyph fallbacks (CJK, emoji, symbols) after a sans family.
const SANS_FALLBACKS: [&str; 12] = [
    ".SystemUIFont",
    "Noto Sans",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "PingFang SC",
    "Hiragino Sans",
    "Microsoft YaHei",
    "Segoe UI",
    "DejaVu Sans",
    "Apple Color Emoji",
    "Noto Color Emoji",
    "Segoe UI Emoji",
];

/// Mono families tried, in order, when the theme's mono cannot load.
const MONO_CANDIDATES: [&str; 9] = ["SF Mono", "Menlo", "Monaco", "Consolas", "JetBrains Mono", "DejaVu Sans Mono", "Liberation Mono", "Noto Sans Mono", "Courier New"];

/// Is a family name a monospace one (the fallback chain it gets)?
fn looks_mono(family: &str) -> bool {
    let f = family.to_ascii_lowercase();
    f.contains("mono") || f.contains("code") || f.contains("consol") || f.contains("menlo") || f.contains("courier")
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct WrapKey {
    text: String,
    family: SharedString,
    weight: u16,
    italic: bool,
    size: u32,
    tracking: u32,
    wrap: Option<i64>,
}

/// Font family NAMES → the families gpui loads, memoized; the surface's
/// default (`sans`) and `mono` families; the wrapped-lines cache the
/// measurer and the painter share.
pub(crate) struct Fonts {
    host: Rc<dyn HostPlugin>,
    text_system: Arc<TextSystem>,
    map: RefCell<HashMap<String, SharedString>>,
    available: RefCell<HashMap<SharedString, bool>>,
    wraps: RefCell<HashMap<WrapKey, Rc<Vec<String>>>>,
    pub default: SharedString,
    pub mono: SharedString,
    sans_fallbacks: FontFallbacks,
    mono_fallbacks: FontFallbacks,
    epoch: Cell<u64>,
}

impl Fonts {
    pub fn new(host: Rc<dyn HostPlugin>, theme: Option<&ResolvedTheme>, text_system: Arc<TextSystem>) -> Fonts {
        let mut f = Fonts {
            host,
            text_system,
            map: RefCell::new(HashMap::new()),
            available: RefCell::new(HashMap::new()),
            wraps: RefCell::new(HashMap::new()),
            default: SharedString::default(),
            mono: SharedString::default(),
            sans_fallbacks: FontFallbacks::from_fonts(SANS_FALLBACKS.iter().map(|s| s.to_string()).collect()),
            mono_fallbacks: FontFallbacks::from_fonts(MONO_CANDIDATES.iter().chain(SANS_FALLBACKS.iter()).map(|s| s.to_string()).collect()),
            epoch: Cell::new(0),
        };
        f.set_theme(theme);
        f
    }

    pub fn set_theme(&mut self, theme: Option<&ResolvedTheme>) {
        let default = default_family(theme).unwrap_or_else(|| "Inter".to_string());
        let mono = mono_family(theme).unwrap_or_else(|| "ui-monospace".to_string());
        self.default = self.map(&default);
        self.mono = self.map(&mono);
    }

    /// Fonts were registered (or removed): every mapping and measurement
    /// is re-derived.
    pub fn invalidate(&mut self, theme: Option<&ResolvedTheme>) {
        self.epoch.set(self.epoch.get() + 1);
        self.map.borrow_mut().clear();
        self.available.borrow_mut().clear();
        self.wraps.borrow_mut().clear();
        self.set_theme(theme);
    }

    /// Whether the text system loads `family` itself (not a stand-in).
    fn loads(&self, family: &SharedString) -> bool {
        if family.starts_with('.') {
            return true;
        }
        if let Some(hit) = self.available.borrow().get(family) {
            return *hit;
        }
        let font = plain_font(family.clone(), 400, false, None);
        let id = self.text_system.resolve_font(&font);
        let ok = self.text_system.get_font_for_id(id).is_some_and(|f| f.family == *family);
        self.available.borrow_mut().insert(family.clone(), ok);
        ok
    }

    fn map(&self, name: &str) -> SharedString {
        if let Some(hit) = self.map.borrow().get(name) {
            return hit.clone();
        }
        let generic = match name.trim() {
            "ui-monospace" | "monospace" | "ui-mono" => Some(true),
            "system-ui" | "sans-serif" | "ui-sans-serif" | "-apple-system" => Some(false),
            _ => None,
        };
        let mapped = match generic {
            Some(false) => SharedString::from(".SystemUIFont"),
            Some(true) => self.first_mono(),
            None => {
                let m = self.host.font_family(name);
                if self.loads(&m) {
                    m
                } else if looks_mono(&m) {
                    self.first_mono()
                } else {
                    SharedString::from(".SystemUIFont")
                }
            }
        };
        self.map.borrow_mut().insert(name.to_string(), mapped.clone());
        mapped
    }

    fn first_mono(&self) -> SharedString {
        MONO_CANDIDATES.iter().map(|m| SharedString::from(*m)).find(|m| self.loads(m)).unwrap_or_else(|| SharedString::from(".SystemUIFont"))
    }

    /// The family for an optional name (`None` = the surface default).
    pub fn family(&self, name: Option<&str>) -> SharedString {
        match name {
            Some(n) if !n.is_empty() => self.map(n),
            _ => self.default.clone(),
        }
    }

    /// The gpui font a family / weight / style paints with (with the glyph
    /// fallback chain).
    pub fn font(&self, family: Option<&str>, weight: u16, italic: bool) -> Font {
        let fam = self.family(family);
        let mono = fam == self.mono || looks_mono(&fam);
        let fallbacks = if mono { self.mono_fallbacks.clone() } else { self.sans_fallbacks.clone() };
        plain_font(fam, weight, italic, Some(fallbacks))
    }

    /// The font of a resolved text style.
    pub fn for_style(&self, ts: &TextStyle) -> Font {
        self.font(ts.font_family.as_deref(), ts.font_weight, ts.font_style.as_deref() == Some("italic"))
    }

    /// The measurer identity: the resolved fonts and the font epoch.
    pub fn identity(&self) -> u64 {
        let mut h = DefaultHasher::new();
        GPUI_MEASURE_BASE.hash(&mut h);
        self.default.hash(&mut h);
        self.mono.hash(&mut h);
        self.epoch.get().hash(&mut h);
        let mut fams: Vec<(String, SharedString)> = self.map.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        fams.sort();
        fams.hash(&mut h);
        h.finish()
    }

    fn cached_lines(&self, key: &WrapKey) -> Option<Rc<Vec<String>>> {
        self.wraps.borrow().get(key).cloned()
    }

    fn store_lines(&self, key: WrapKey, lines: Rc<Vec<String>>) {
        let mut w = self.wraps.borrow_mut();
        if w.len() > 4096 {
            w.clear();
        }
        w.insert(key, lines);
    }
}

/// A font of `family` at `weight` (italic when asked).
pub fn make_font(family: SharedString, weight: u16) -> Font {
    plain_font(family, weight, false, None)
}

fn plain_font(family: SharedString, weight: u16, italic: bool, fallbacks: Option<FontFallbacks>) -> Font {
    Font { family, features: Default::default(), fallbacks, weight: FontWeight(weight as f32), style: if italic { FontStyle::Italic } else { FontStyle::Normal } }
}

fn run(text: &str, font: Font) -> TextRun {
    TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None }
}

/// Text shaping against a window's text system.
pub(crate) struct Shaper<'w> {
    pub window: &'w Window,
    pub fonts: &'w Fonts,
    pub calls: u64,
    /// CSS `letter-spacing` (px after every character; 0 = none).
    pub tracking: f32,
}

/// The extra width `letter-spacing` adds to a line (one `ls` per
/// character, the last one included, as browsers do).
pub fn tracking_width(text: &str, ls: f32) -> f32 {
    if ls == 0.0 {
        0.0
    } else {
        ls * text.chars().count() as f32
    }
}

impl<'w> Shaper<'w> {
    pub fn new(window: &'w Window, fonts: &'w Fonts) -> Self {
        Shaper { window, fonts, calls: 0, tracking: 0.0 }
    }

    /// The same shaper with a text style's `letterSpacing`.
    pub fn tracked(mut self, ts: &TextStyle) -> Self {
        self.tracking = ts.letter_spacing.unwrap_or(0.0);
        self
    }

    /// Width of ONE line (no newlines), `letter-spacing` included.
    pub fn line_width(&mut self, text: &str, font: &Font, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.calls += 1;
        let line = self.window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run(text, font.clone())], None);
        f32::from(line.width) + tracking_width(text, self.tracking)
    }

    pub fn max_content(&mut self, text: &str, font: &Font, size: f32) -> f32 {
        text::max_content(text, &mut |s| self.line_width(s, font, size))
    }

    /// Min-content: the widest unbreakable segment (UAX #14).
    pub fn min_content(&mut self, text: &str, font: &Font, size: f32) -> f32 {
        text::min_content(text, &mut |s| self.line_width(s, font, size))
    }

    /// The lines `text` breaks into at `wrap` (`None` = the hard breaks
    /// only) — the lines the painter draws.
    pub fn lines(&mut self, text: &str, font: &Font, size: f32, wrap: Option<f32>) -> Rc<Vec<String>> {
        let key = WrapKey { text: text.to_string(), family: font.family.clone(), weight: font.weight.0 as u16, italic: font.style == FontStyle::Italic, size: size.to_bits(), tracking: self.tracking.to_bits(), wrap: wrap.map(|w| (w * 4.0).round() as i64) };
        if let Some(hit) = self.fonts.cached_lines(&key) {
            return hit;
        }
        let lines = Rc::new(text::wrap(text, wrap, &mut |s| self.line_width(s, font, size)));
        self.fonts.store_lines(key, lines.clone());
        lines
    }

    /// Where the first baseline sits inside a line box of `line_height`
    /// (half-leading + ascent, CSS).
    pub fn baseline(&mut self, font: &Font, size: f32, line_height: f32) -> f32 {
        let ts = self.window.text_system();
        let id = ts.resolve_font(font);
        let ascent = f32::from(ts.ascent(id, px(size)));
        let descent = f32::from(ts.descent(id, px(size))).abs();
        ((line_height - (ascent + descent)) / 2.0 + ascent).max(0.0)
    }

    /// Rich spans (markdown) wrapped at `wrap`.
    fn spans_height(&mut self, inlines: &[Inline], spec: &TextSpec, wrap: Option<f32>) -> f32 {
        let family = self.fonts.family(spec.family.as_deref());
        let colors = markdown::RunColors { ink: gpui::black(), link: gpui::black(), code_bg: None };
        let (text, runs) = markdown::runs(inlines, &family, &self.fonts.mono, spec.weight, colors);
        if text.is_empty() {
            return spec.line_height;
        }
        self.calls += 1;
        match self.window.text_system().shape_text(text, px(spec.size), &runs, wrap.map(|w| px(w.max(1.0))), None) {
            Ok(lines) => lines.iter().map(|l| f32::from(l.size(px(spec.line_height)).height)).sum::<f32>().max(spec.line_height),
            Err(_) => spec.line_height,
        }
    }
}

impl MdText for Shaper<'_> {
    fn height(&mut self, inlines: &[Inline], spec: &TextSpec, width: Option<f32>) -> f32 {
        self.spans_height(inlines, spec, width)
    }

    fn width(&mut self, inlines: &[Inline], spec: &TextSpec) -> f32 {
        let font = self.fonts.font(spec.family.as_deref(), spec.weight, false);
        let text = markdown::plain(inlines);
        self.max_content(&text, &font, spec.size)
    }

    fn widest_word(&mut self, inlines: &[Inline], spec: &TextSpec) -> f32 {
        let font = self.fonts.font(spec.family.as_deref(), spec.weight, false);
        let text = markdown::plain(inlines);
        self.min_content(&text, &font, spec.size)
    }
}

// ---------------------------------------------------------------------------
// Unit-free helpers
// ---------------------------------------------------------------------------

/// The horizontal and vertical insets a control box adds around its content.
pub fn insets(c: &ControlBox) -> (f32, f32) {
    let p = c.padding;
    if p == [0.0; 4] {
        (2.0 * (c.padding_horizontal + c.border_width), 2.0 * (c.padding_vertical + c.border_width))
    } else {
        (p[1] + p[3] + 2.0 * c.border_width, p[0] + p[2] + 2.0 * c.border_width)
    }
}

/// The top inset (padding + border) of a control box.
pub fn top_inset(c: &ControlBox) -> f32 {
    let p = c.padding;
    (if p == [0.0; 4] { c.padding_vertical } else { p[0] }) + c.border_width
}

/// The content wrap width inside a border-box wrap width (`Some(0)` stays
/// min-content).
pub fn inner_wrap(wrap: Option<f32>, c: &ControlBox) -> Option<f32> {
    let (h, _) = insets(c);
    wrap.map(|w| if w <= 0.0 { 0.0 } else { (w - h).max(0.0) })
}

/// Content size → the control's border box: insets added, then the recipe's
/// fixed sizes win and minimums clamp.
pub fn border_box(content: (f32, f32), c: &ControlBox) -> (f32, f32) {
    let (ih, iv) = insets(c);
    let w = c.width.unwrap_or(content.0 + ih).max(c.min_width.unwrap_or(0.0));
    let h = c.height.unwrap_or(content.1 + iv).max(c.min_height.unwrap_or(0.0));
    (w, h)
}

/// A CSS-ish length prop: px number / `"Npx"` → `Px`, `"N%"` → `Percent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Len {
    Px(f32),
    Percent(f32),
}

pub fn len_of(v: Option<&Value>) -> Option<Len> {
    match v? {
        Value::Number(n) => n.as_f64().map(|f| Len::Px(f as f32)),
        Value::String(s) => {
            let s = s.trim();
            if let Some(p) = s.strip_suffix('%') {
                p.trim().parse::<f32>().ok().map(Len::Percent)
            } else {
                s.strip_suffix("px").unwrap_or(s).trim().parse::<f32>().ok().map(Len::Px)
            }
        }
        _ => None,
    }
}

fn str_prop<'a>(props: &'a Map<String, Value>, key: &str) -> &'a str {
    props.get(key).and_then(Value::as_str).unwrap_or("")
}

fn num_prop(props: &Map<String, Value>, key: &str) -> Option<f64> {
    match props.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn has(props: &Map<String, Value>, key: &str) -> bool {
    props.get(key).is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
}

/// A JSON scalar as display text (`3` → "3").
pub fn display_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => exponential_ui::json::to_js_string(other),
    }
}

/// The chart's legend entries (pie/donut: categories; else series names
/// when > 1). The core's `chart.legend` flag decides whether it shows.
pub fn chart_legend(props: &Map<String, Value>) -> Vec<String> {
    let kind = str_prop(props, "kind");
    let series = props.get("series").and_then(Value::as_array).cloned().unwrap_or_default();
    let shows = props.get("chart").and_then(|c| c.get("legend")).and_then(Value::as_bool).unwrap_or(true) && props.get("showLegend").and_then(Value::as_bool) != Some(false);
    if !shows || kind == "sparkline" {
        return Vec::new();
    }
    if matches!(kind, "pie" | "donut") {
        props.get("categories").and_then(Value::as_array).map(|c| c.iter().map(|v| display_text(Some(v))).collect()).unwrap_or_default()
    } else if series.len() > 1 {
        series.iter().map(|s| display_text(s.get("name"))).collect()
    } else {
        Vec::new()
    }
}

/// The leading/trailing chrome a one-line text part carries beside its text
/// (an icon, a count, a chevron, a check, a sort arrow): `(lead, trail)` px.
pub fn text_chrome(owner_component: &str, part: Option<&str>, props: &Map<String, Value>, gap: f32, count_w: f32) -> (f32, f32) {
    let icon = |key: &str| if has(props, key) { 16.0 + gap.max(4.0) } else { 0.0 };
    match (owner_component, part) {
        ("Tabs", Some("tab")) => (icon("icon"), if has(props, "count") { 4.0 + count_w + 12.0 } else { 0.0 }),
        ("Accordion", Some("trigger")) => (0.0, 16.0 + gap.max(8.0)),
        ("Select", Some("item")) => (icon("icon"), 16.0 + gap.max(8.0)),
        ("DropdownMenu" | "ContextMenu", Some("itemLabel")) => (icon("icon"), 0.0),
        ("Table", Some("headerCell")) => (0.0, if has(props, "sortIcon") || props.get("sortable").and_then(Value::as_bool) == Some(true) { 16.0 + 4.0 } else { 0.0 }),
        _ => (0.0, 0.0),
    }
}

/// Is a leaf a host-owned one-line field (`search`, NumberField/ChipInput `input`)?
pub fn is_inline_field(component: &str, part: Option<&str>) -> bool {
    matches!((component, part), ("Select", Some("search")) | ("NumberField", Some("input")) | ("ChipInput", Some("input")))
}

// ---------------------------------------------------------------------------
// The measure
// ---------------------------------------------------------------------------

/// The painter's `Measure`: built per pass from the window, the host's font
/// mapping, the extension painters and the live text of host-owned fields.
pub struct GpuiMeasure<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
    fonts: &'a Fonts,
    theme: Option<&'a ResolvedTheme>,
    mode: Mode,
    painters: &'a HashMap<String, Rc<dyn ExtensionPainter>>,
    /// Extension node id → its kind (`LeafRequest` carries no kind).
    kinds: &'a HashMap<String, String>,
    /// Field id → the text the user typed (not yet in the props).
    live: &'a HashMap<String, String>,
    identity: u64,
    calls: u64,
    shaped: u64,
    /// Extension leaves measured before their kind was known.
    pub(crate) unknown_extensions: Vec<u32>,
}

/// Content size and the first baseline from the CONTENT top.
type Content = (f32, f32, Option<f32>);

impl<'a> GpuiMeasure<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        window: &'a mut Window,
        cx: &'a mut App,
        fonts: &'a Fonts,
        theme: Option<&'a ResolvedTheme>,
        mode: Mode,
        painters: &'a HashMap<String, Rc<dyn ExtensionPainter>>,
        kinds: &'a HashMap<String, String>,
        live: &'a HashMap<String, String>,
    ) -> Self {
        let identity = fonts.identity();
        GpuiMeasure { window, cx, fonts, theme, mode, painters, kinds, live, identity, calls: 0, shaped: 0, unknown_extensions: Vec::new() }
    }

    /// Per-request measure calls answered this pass.
    pub fn calls(&self) -> u64 {
        self.calls
    }

    /// Text shaping calls made this pass (cache hits included).
    pub fn shaped(&self) -> u64 {
        self.shaped
    }

    fn shaper(&self) -> Shaper<'_> {
        Shaper::new(self.window, self.fonts)
    }

    fn part(&self, component: &str, part: &str, props: &Map<String, Value>, states: &[String]) -> Map<String, Value> {
        part_props(self.theme, self.mode, component, part, props, states)
    }

    /// Plain text content at an inner wrap width, with `lines` truncation.
    fn para(&mut self, raw: &str, ts: &TextStyle, wrap: Option<f32>, lines: Option<u32>) -> Content {
        let font = self.fonts.for_style(ts);
        let shown = text::transform(raw, ts.text_transform.as_deref());
        let single = lines == Some(1);
        let clamp = lines.filter(|n| *n > 0).map(|n| n as usize);
        let mut s = self.shaper().tracked(ts);
        let baseline = Some(s.baseline(&font, ts.font_size, ts.line_height));
        let lh = ts.line_height;
        let out = match wrap {
            None => {
                let w = s.max_content(&shown, &font, ts.font_size);
                let n = if single { 1 } else { s.lines(&shown, &font, ts.font_size, None).len() };
                (w, lh * clamp.map_or(n, |c| n.min(c)).max(1) as f32)
            }
            // A one-line (ellipsized) text shrinks to nothing, like CSS
            // `white-space: nowrap; min-width: 0`.
            Some(w) if w <= 0.0 => {
                if single {
                    (0.0, lh)
                } else {
                    let m = s.min_content(&shown, &font, ts.font_size);
                    let n = s.lines(&shown, &font, ts.font_size, Some(m)).len();
                    (m, lh * clamp.map_or(n, |c| n.min(c)).max(1) as f32)
                }
            }
            Some(w) => {
                let max = s.max_content(&shown, &font, ts.font_size);
                if single {
                    (max.min(w), lh)
                } else {
                    let used = max.min(w.max(s.min_content(&shown, &font, ts.font_size)));
                    let n = s.lines(&shown, &font, ts.font_size, Some(w.max(used))).len();
                    (used, lh * clamp.map_or(n, |c| n.min(c)).max(1) as f32)
                }
            }
        };
        self.shaped += s.calls;
        (out.0, out.1, baseline)
    }

    fn line(&mut self, raw: &str, ts: &TextStyle) -> f32 {
        let font = self.fonts.for_style(ts);
        let shown = text::transform(raw, ts.text_transform.as_deref());
        let mut s = self.shaper().tracked(ts);
        let w = s.max_content(&shown, &font, ts.font_size);
        self.shaped += s.calls;
        w
    }

    fn line_baseline(&mut self, ts: &TextStyle) -> f32 {
        let font = self.fonts.for_style(ts);
        self.shaper().baseline(&font, ts.font_size, ts.line_height)
    }

    /// A one-line text with fixed chrome beside it.
    fn row(&mut self, raw: &str, ts: &TextStyle, (lead, trail): (f32, f32), wrap: Option<f32>) -> Content {
        let w = lead + self.line(raw, ts) + trail;
        let used = match wrap {
            None => w,
            Some(x) if x <= 0.0 => lead + trail,
            Some(x) => w.min(x),
        };
        (used, ts.line_height, Some(self.line_baseline(ts)))
    }

    fn markdown(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> Content {
        let text = str_prop(leaf.props, "text");
        let blocks = markdown::parse(text);
        let ts = leaf.text_style;
        let body = TextSpec { size: ts.font_size, line_height: ts.line_height, weight: ts.font_weight, family: ts.font_family.clone() };
        let styles = MdStyles::resolve(self.theme, self.mode, body, leaf.props);
        let clamp = num_prop(leaf.props, "lines").filter(|n| *n > 0.0).map(|n| n as usize);
        let mut s = self.shaper();
        let out = match wrap {
            None => {
                let w = markdown::max_content_width(&blocks, &styles, &mut s);
                (w, markdown::layout(&blocks, &styles, w, &mut s).clamped_height(clamp, ts.line_height))
            }
            Some(w) if w <= 0.0 => {
                let w = markdown::min_content_width(&blocks, &styles, &mut s);
                (w, markdown::layout(&blocks, &styles, w, &mut s).clamped_height(clamp, ts.line_height))
            }
            Some(w) => (w, markdown::layout(&blocks, &styles, w, &mut s).clamped_height(clamp, ts.line_height)),
        };
        self.shaped += s.calls;
        let baseline = Some(self.line_baseline(ts));
        (out.0, out.1.max(if blocks.is_empty() { 0.0 } else { ts.line_height }), baseline)
    }

    fn button(&mut self, leaf: &LeafRequest, component: &str) -> Content {
        let props = leaf.props;
        let ts = leaf.text_style;
        let label = str_prop(props, "label");
        let has_icon = has(props, "icon") || props.get("loading").and_then(Value::as_bool) == Some(true);
        let icon_only = str_prop(props, "size") == "icon";
        let icon = if has_icon || icon_only { px_prop(&self.part(component, "icon", props, &[]), "width").unwrap_or_else(|| control(self.theme, "iconSm", 16.0)) } else { 0.0 };
        let label_w = if icon_only || label.is_empty() { 0.0 } else { self.line(label, ts) };
        let gap = if has_icon && label_w > 0.0 { leaf.control.gap } else { 0.0 };
        let w = if has_icon || icon_only { icon } else { 0.0 } + gap + label_w;
        let h = ts.line_height.max(if has_icon { icon } else { 0.0 });
        let baseline = (label_w > 0.0).then(|| (h - ts.line_height) / 2.0 + self.line_baseline(ts));
        (w, h, baseline)
    }

    fn media(&self, props: &Map<String, Value>, wrap: Option<f32>, default: (f32, f32)) -> (f32, f32) {
        let ratio = num_prop(props, "aspectRatio").map(|r| r as f32).filter(|r| *r > 0.0).unwrap_or(default.0 / default.1);
        let fixed_w = match len_of(props.get("width")) {
            Some(Len::Px(w)) => Some(w),
            _ => None,
        };
        let fixed_h = match len_of(props.get("height")) {
            Some(Len::Px(h)) => Some(h),
            _ => None,
        };
        let w = match (fixed_w, wrap) {
            (Some(w), _) => w,
            (None, None) => default.0,
            (None, Some(x)) if x <= 0.0 => 0.0,
            (None, Some(x)) => x.min(default.0.max(x)),
        };
        let h = fixed_h.unwrap_or(if w > 0.0 { w / ratio } else { default.0 / ratio });
        (w, h)
    }

    fn skeleton(&self, props: &Map<String, Value>, wrap: Option<f32>) -> (f32, f32) {
        let w = match (len_of(props.get("width")), wrap) {
            (Some(Len::Px(w)), _) => w,
            (Some(Len::Percent(p)), Some(x)) if x > 0.0 => x * p / 100.0,
            (Some(Len::Percent(_)), Some(_)) => 0.0,
            (Some(Len::Percent(_)) | None, None) => 240.0,
            (None, Some(x)) if x > 0.0 => x,
            (None, Some(_)) => 0.0,
        };
        let h = match len_of(props.get("height")) {
            Some(Len::Px(h)) => h,
            _ => 16.0,
        };
        (w, h)
    }

    fn chart(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32) {
        let props = leaf.props;
        let ts = leaf.text_style;
        let kind = str_prop(props, "kind");
        let w = match wrap {
            None if kind == "sparkline" => 120.0,
            None => 320.0,
            Some(x) if x <= 0.0 => 0.0,
            Some(x) => x,
        };
        let mut h = num_prop(props, "height").unwrap_or(if kind == "sparkline" { 32.0 } else { 200.0 }) as f32;
        let gap = spacing(self.theme, "xs");
        if !str_prop(props, "title").is_empty() {
            h += ts.line_height + gap;
        }
        if !chart_legend(props).is_empty() {
            let legend = self.part("Chart", "legend", props, &[]);
            h += px_prop(&legend, "lineHeight").unwrap_or(16.0) + gap;
        }
        (w, h)
    }

    /// The composer: its LIVE text wrapped at the field width, clamped to
    /// the field's min height and 200 px, plus the send bar.
    fn composer(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32) {
        let props = leaf.props;
        let ts = leaf.text_style;
        let field = self.part("Composer", "field", props, &[]);
        let fs = px_prop(&field, "fontSize").unwrap_or(ts.font_size);
        let lh = px_prop(&field, "lineHeight").unwrap_or(ts.line_height);
        let min_h = px_prop(&field, "minHeight").unwrap_or(lh);
        let send = px_prop(&self.part("Composer", "send", props, &[]), "height").unwrap_or_else(|| control(self.theme, "buttonIcon", 36.0));
        let gap = leaf.control.gap;
        let live = self.live.get(leaf.id).cloned();
        let value = live.unwrap_or_else(|| str_prop(props, "value").to_string());
        let text = if value.is_empty() { str_prop(props, "placeholder").to_string() } else { value };
        // An empty field (no placeholder either) is one line tall.
        let text = if text.is_empty() { " ".to_string() } else { text };
        let field_ts = TextStyle { font_size: fs, line_height: lh, ..ts.clone() };
        let w = match wrap {
            None => 320.0,
            Some(x) if x <= 0.0 => 120.0,
            Some(x) => x,
        };
        let (_, th, _) = self.para(&text, &field_ts, Some(w.max(1.0)), None);
        (w, th.clamp(min_h, 200.0) + gap + send)
    }

    /// A Textarea field: `rows` lines, or (autosize) its live text's
    /// wrapped lines clamped to `rows..maxRows`.
    fn textarea(&mut self, leaf: &LeafRequest, inner: Option<f32>) -> (f32, f32) {
        let props = leaf.props;
        let ts = leaf.text_style;
        let rows = num_prop(props, "rows").unwrap_or(3.0).max(1.0) as f32;
        let w = 160.0;
        if props.get("autosize").and_then(Value::as_bool) != Some(true) {
            return (w, rows * ts.line_height);
        }
        let value = self.live.get(leaf.id).cloned().unwrap_or_else(|| str_prop(props, "value").to_string());
        let max = num_prop(props, "maxRows").map(|m| m as f32).unwrap_or(f32::INFINITY);
        let at = match inner {
            Some(x) if x > 0.0 => x,
            _ => w,
        };
        let font = self.fonts.for_style(ts);
        let mut s = self.shaper();
        let lines = s.lines(&value, &font, ts.font_size, Some(at)).len() as f32;
        self.shaped += s.calls;
        (w, lines.max(rows).min(max.max(rows)) * ts.line_height)
    }

    /// A host-owned one-line field: its text (or placeholder) + a caret.
    fn inline_field(&mut self, leaf: &LeafRequest) -> Content {
        let props = leaf.props;
        let typed = self.live.get(leaf.id).cloned().unwrap_or_else(|| str_prop(props, "text").to_string());
        let value = if typed.is_empty() { display_text(props.get("value")) } else { typed };
        let ph = str_prop(props, "placeholder").to_string();
        // The host's text input paints without tracking (an editable field
        // has no per-glyph spacing): measured the same.
        let ts = &TextStyle { letter_spacing: None, ..leaf.text_style.clone() };
        let w = self.line(&value, ts).max(self.line(&ph, ts)) + 2.0;
        let icon = if leaf.part == Some("search") && has(props, "icon") { 16.0 + 8.0 } else { 0.0 };
        (w.max(24.0) + icon, ts.line_height, Some(self.line_baseline(ts)))
    }

    fn toggle_group(&mut self, leaf: &LeafRequest) -> (f32, f32) {
        let props = leaf.props;
        let items = props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let item = self.part("ToggleGroup", "item", props, &[]);
        let pad = px_prop(&item, "paddingHorizontal").or_else(|| px_prop(&item, "padding")).unwrap_or(12.0);
        let border = px_prop(&item, "borderWidth").unwrap_or(0.0);
        let h = px_prop(&item, "height").unwrap_or(36.0);
        let fs = px_prop(&item, "fontSize").unwrap_or(leaf.text_style.font_size);
        let ts = TextStyle {
            font_size: fs,
            font_weight: item.get("fontWeight").and_then(Value::as_u64).map(|w| w as u16).unwrap_or(500),
            line_height: leaf.text_style.line_height,
            font_family: item.get("fontFamily").and_then(Value::as_str).map(str::to_string),
            // Items inherit the group's tracking and case (painted so).
            letter_spacing: leaf.text_style.letter_spacing,
            text_transform: leaf.text_style.text_transform.clone(),
            ..Default::default()
        };
        let gap = leaf.control.gap;
        let mut w = 0.0;
        for (i, it) in items.iter().enumerate() {
            let label = display_text(it.get("label"));
            let has_icon = it.get("icon").and_then(Value::as_str).is_some();
            let lw = if label.is_empty() { 0.0 } else { self.line(&label, &ts) };
            let iw = if has_icon { 16.0 } else { 0.0 };
            let inner_gap = if has_icon && lw > 0.0 { 6.0 } else { 0.0 };
            w += 2.0 * (pad + border) + iw + inner_gap + lw;
            if i > 0 {
                w += gap;
            }
        }
        (w, h)
    }

    /// A picker trigger (Select / DatePicker / DateRangePicker / TimePicker):
    /// the core's `text` + its glyph, at least 160 wide like the web.
    fn trigger(&mut self, leaf: &LeafRequest) -> Content {
        let ts = leaf.text_style;
        let text = str_prop(leaf.props, "text");
        let text = if text.is_empty() { str_prop(leaf.props, "placeholder") } else { text };
        let gap = leaf.control.gap.max(spacing(self.theme, "sm"));
        let w = self.line(text, ts) + gap + 16.0;
        let (ih, _) = insets(&leaf.control);
        (w.max(160.0 - ih), ts.line_height, Some(self.line_baseline(ts)))
    }

    /// One leaf at one border-box wrap width: `(width, height, baseline)`.
    pub fn measure_leaf(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32, Option<f32>) {
        self.calls += 1;
        let c = leaf.control;
        let inner = inner_wrap(wrap, &c);
        let props = leaf.props;
        let ts = leaf.text_style;
        let owner = leaf.component;
        let plain = |(w, h): (f32, f32)| (w, h, None);
        let content: Content = match (leaf.component, leaf.part) {
            ("Extension", _) => return (0.0, 0.0, None),
            _ if is_inline_field(leaf.component, leaf.part) => self.inline_field(leaf),
            ("Select" | "DatePicker" | "DateRangePicker" | "TimePicker", Some("trigger")) => self.trigger(leaf),
            ("Text", part) => {
                let count_w = props.get("count").map(|v| display_text(Some(v))).filter(|s| !s.is_empty()).map(|s| self.line(&s, ts)).unwrap_or(0.0);
                let chrome = text_chrome(leaf.owner_component.unwrap_or(owner), part, props, c.gap, count_w);
                let raw = str_prop(props, "text");
                if chrome != (0.0, 0.0) {
                    self.row(raw, ts, chrome, inner)
                } else if part == Some("cell") && str_prop(props, "cellType") == "boolean" {
                    (16.0, ts.line_height, None)
                } else if part == Some("cell") && str_prop(props, "cellType") == "badge" {
                    let (w, h, b) = self.para(raw, &TextStyle { font_size: (ts.font_size - 2.0).max(10.0), ..ts.clone() }, None, Some(1));
                    (w + 16.0, h.max(ts.line_height), b)
                } else {
                    self.para(raw, ts, inner, leaf.lines)
                }
            }
            ("Markdown", _) => self.markdown(leaf, inner),
            ("Button" | "Toggle" | "DropdownMenu", _) => self.button(leaf, owner),
            ("Link", _) => {
                let label = str_or(props, "label", str_prop(props, "href"));
                let w = self.line(&label, ts);
                (w, ts.line_height, Some(self.line_baseline(ts)))
            }
            ("Icon", _) => plain((16.0, 16.0)),
            ("Avatar", _) => plain((32.0, 32.0)),
            ("Image", _) => plain(self.media(props, inner, (320.0, 180.0))),
            ("Video", _) => plain(self.media(props, inner, (320.0, 180.0))),
            ("AudioPlayer", _) => {
                let title = str_prop(props, "title");
                let track = if title.is_empty() { 0.0 } else { ts.line_height + spacing(self.theme, "xs") };
                let w = match inner {
                    None => 300.0,
                    Some(x) if x <= 0.0 => 160.0,
                    Some(x) => x,
                };
                plain((w, track + 40.0))
            }
            ("Spinner", _) => plain((20.0, 20.0)),
            ("Ring", _) => plain((32.0, 32.0)),
            ("Skeleton", _) => plain(self.skeleton(props, inner)),
            ("Chart", _) => plain(self.chart(leaf, inner)),
            ("Composer", _) => plain(self.composer(leaf, inner)),
            ("TreeGuides", _) => {
                let depth = num_prop(props, "depth").unwrap_or(0.0).max(0.0) as f32;
                plain((depth * 16.0, ts.line_height))
            }
            ("ToggleGroup", _) => plain(self.toggle_group(leaf)),
            ("Unknown", _) => {
                let label = self.part("Unknown", "label", props, &[]);
                let lts = TextStyle {
                    font_size: px_prop(&label, "fontSize").unwrap_or(12.0),
                    line_height: px_prop(&label, "lineHeight").unwrap_or(16.0),
                    font_weight: 400,
                    font_family: label.get("fontFamily").and_then(Value::as_str).map(str::to_string),
                    ..Default::default()
                };
                let text = unknown_label(props, leaf.component);
                self.para(&text, &lts, inner, None)
            }
            ("Box", Some("indicator")) => {
                let n = num_prop(props, "count").unwrap_or(0.0).max(0.0) as f32;
                let dot = px_prop(&self.part("Carousel", "indicator", props, &[]), "width").unwrap_or(8.0);
                let gap = spacing(self.theme, "xs");
                plain((n * dot + (n - 1.0).max(0.0) * gap, dot + spacing(self.theme, "sm")))
            }
            ("Input", Some("field")) => (160.0, ts.line_height, Some(self.line_baseline(ts))),
            ("Textarea", Some("field")) => {
                let (w, h) = self.textarea(leaf, inner);
                (w, h, Some(self.line_baseline(ts)))
            }
            ("Checkbox", Some("box" | "checkbox")) | ("Radio", Some("dot")) => plain((16.0, 16.0)),
            ("Switch", Some("track")) => plain((32.0, 20.0)),
            ("Slider", Some("track")) => {
                let thumb = px_prop(&self.part("Slider", "thumb", props, &[]), "height").unwrap_or_else(|| control(self.theme, "slider", 16.0));
                plain((160.0, thumb))
            }
            // Geometry mode (no theme): every native is ONE measured leaf.
            ("Input" | "Select" | "DatePicker" | "NumberField" | "TimePicker" | "DateRangePicker" | "ChipInput", None) => (160.0, 36.0, None),
            ("Textarea", None) => plain((160.0, 72.0)),
            ("Switch", None) => plain((44.0, 24.0)),
            ("Checkbox" | "Radio", None) => plain((16.0, 16.0)),
            ("Slider", None) => plain((160.0, 16.0)),
            ("Table", None) => {
                let rows = props.get("rows").and_then(Value::as_array).map(Vec::len).unwrap_or(0) as f32;
                plain((inner.filter(|w| *w > 0.0).unwrap_or(320.0), (rows + 1.0) * 36.0))
            }
            ("CodeBlock", None) => {
                let code = str_prop(props, "code");
                let mono = TextStyle { font_family: Some("ui-monospace".into()), ..ts.clone() };
                self.para(code, &mono, None, None)
            }
            ("FileUpload", None) => plain((240.0, 96.0)),
            _ => plain((0.0, 0.0)),
        };
        let (w, h) = border_box((content.0, content.1), &c);
        let baseline = content.2.map(|b| {
            let fixed = c.height.is_some_and(|fh| (fh - (content.1 + insets(&c).1)).abs() > 0.5);
            if fixed || matches!(leaf.part, Some("trigger" | "field" | "input" | "search")) || matches!(leaf.component, "Button" | "Toggle") {
                // A control centres its line in its box.
                ((h - content.1) / 2.0).max(0.0) + b
            } else {
                top_inset(&c) + b
            }
        });
        (w, h, baseline)
    }

    fn extension(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> Option<(f32, f32)> {
        let Some(kind) = self.kinds.get(leaf.id).cloned() else {
            self.unknown_extensions.push(leaf.index);
            return None;
        };
        let painter = self.painters.get(&kind)?.clone();
        painter.measure(leaf, wrap, self.window, self.cx)
    }

    fn answer(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32, Option<f32>) {
        if leaf.component == "Extension" {
            self.calls += 1;
            let (w, h) = self.extension(leaf, wrap).unwrap_or((0.0, 0.0));
            return (w, h, None);
        }
        self.measure_leaf(leaf, wrap)
    }
}

fn str_or(props: &Map<String, Value>, key: &str, fallback: &str) -> String {
    match props.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => fallback.to_string(),
    }
}

/// The text of the `Unknown` placeholder.
pub fn unknown_label(props: &Map<String, Value>, component: &str) -> String {
    format!("Unknown component {}", str_or(props, "component", component))
}

/// A Select's trigger text: the chosen option labels or the placeholder.
pub fn select_label(props: &Map<String, Value>) -> String {
    let options = props.get("options").and_then(Value::as_array).cloned().unwrap_or_default();
    let chosen: Vec<String> = match props.get("value") {
        Some(Value::Array(vals)) => vals.iter().map(|v| display_text(Some(v))).collect(),
        Some(Value::String(s)) if s.contains(',') => s.split(',').map(str::to_string).collect(),
        Some(v) if !v.is_null() => vec![display_text(Some(v))],
        _ => Vec::new(),
    };
    let labels: Vec<String> = options.iter().filter(|o| chosen.contains(&display_text(o.get("value")))).map(|o| display_text(o.get("label"))).collect();
    if labels.is_empty() {
        str_or(props, "placeholder", "Choose")
    } else {
        labels.join(", ")
    }
}

/// `"2026-10-14"` → `"Oct 14, 2026"`.
pub fn date_label(value: &str) -> Option<String> {
    let (y, m, d) = crate::paint::date::parse_iso(value)?;
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    Some(format!("{} {}, {}", MONTHS[(m - 1) as usize], d, y))
}

impl Measure for GpuiMeasure<'_> {
    fn measure_id(&self) -> u64 {
        self.identity
    }

    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves
            .iter()
            .map(|leaf| {
                let (max_w, h, baseline) = self.answer(leaf, None);
                let (min_w, _, _) = self.answer(leaf, Some(0.0));
                Intrinsics { min_content_width: min_w.min(max_w), max_content_width: max_w, height_at_max_content: h, baseline }
            })
            .collect()
    }

    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests.iter().map(|r| leaves.iter().find(|l| l.index == r.index).map(|leaf| self.answer(leaf, Some(r.width)).1).unwrap_or(0.0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn control_boxes_wrap_the_content() {
        let c = ControlBox { padding_horizontal: 16.0, padding_vertical: 0.0, padding: [0.0, 16.0, 0.0, 16.0], border_width: 1.0, gap: 8.0, min_width: Some(80.0), min_height: None, width: None, height: Some(36.0) };
        assert_eq!(insets(&c), (34.0, 2.0));
        assert_eq!(border_box((20.0, 20.0), &c), (80.0, 36.0));
        assert_eq!(border_box((100.0, 20.0), &c), (134.0, 36.0));
        assert_eq!(inner_wrap(Some(134.0), &c), Some(100.0));
        assert_eq!(inner_wrap(Some(0.0), &c), Some(0.0));
        assert_eq!(inner_wrap(None, &c), None);
        assert_eq!(border_box((10.0, 10.0), &ControlBox::default()), (10.0, 10.0));
        let uneven = ControlBox { padding: [2.0, 4.0, 6.0, 8.0], padding_horizontal: 6.0, padding_vertical: 4.0, ..Default::default() };
        assert_eq!(insets(&uneven), (12.0, 8.0));
        assert_eq!(top_inset(&uneven), 2.0);
    }

    #[test]
    fn lengths_parse() {
        assert_eq!(len_of(Some(&json!(12))), Some(Len::Px(12.0)));
        assert_eq!(len_of(Some(&json!("120px"))), Some(Len::Px(120.0)));
        assert_eq!(len_of(Some(&json!("50%"))), Some(Len::Percent(50.0)));
        assert_eq!(len_of(Some(&json!("auto"))), None);
        assert_eq!(len_of(None), None);
    }

    #[test]
    fn select_and_date_labels() {
        let props = json!({"options": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}], "value": "b"}).as_object().unwrap().clone();
        assert_eq!(select_label(&props), "B");
        let none = json!({"options": [], "placeholder": "Pick"}).as_object().unwrap().clone();
        assert_eq!(select_label(&none), "Pick");
        let multi = json!({"options": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}], "value": ["a", "b"]}).as_object().unwrap().clone();
        assert_eq!(select_label(&multi), "A, B");
        let joined = json!({"options": [{"label": "A", "value": "a"}, {"label": "B", "value": "b"}], "value": "a,b"}).as_object().unwrap().clone();
        assert_eq!(select_label(&joined), "A, B");
        assert_eq!(date_label("2026-10-14").as_deref(), Some("Oct 14, 2026"));
        assert_eq!(date_label("nope"), None);
    }

    #[test]
    fn legends_follow_the_chart_kind() {
        let bar = json!({"kind": "bar", "series": [{"name": "Runs"}, {"name": "Fails"}]}).as_object().unwrap().clone();
        assert_eq!(chart_legend(&bar), ["Runs", "Fails"]);
        let single = json!({"kind": "line", "series": [{"name": "Runs"}]}).as_object().unwrap().clone();
        assert!(chart_legend(&single).is_empty());
        let pie = json!({"kind": "donut", "categories": ["a", "b"], "series": [{"values": [1, 2]}]}).as_object().unwrap().clone();
        assert_eq!(chart_legend(&pie), ["a", "b"]);
        let hidden = json!({"kind": "bar", "showLegend": false, "series": [{"name": "Runs"}, {"name": "Fails"}]}).as_object().unwrap().clone();
        assert!(chart_legend(&hidden).is_empty());
        let spark = json!({"kind": "sparkline", "series": [{"name": "a"}, {"name": "b"}]}).as_object().unwrap().clone();
        assert!(chart_legend(&spark).is_empty());
    }

    #[test]
    fn text_parts_reserve_their_chrome() {
        let tab = json!({"text": "Inbox", "icon": "nav-inbox", "count": 3}).as_object().unwrap().clone();
        assert_eq!(text_chrome("Tabs", Some("tab"), &tab, 4.0, 8.0), (20.0, 24.0));
        let item = json!({"text": "Open"}).as_object().unwrap().clone();
        assert_eq!(text_chrome("Select", Some("item"), &item, 8.0, 0.0), (0.0, 24.0), "the check slot is always reserved");
        assert_eq!(text_chrome("Text", None, &item, 8.0, 0.0), (0.0, 0.0));
        assert!(is_inline_field("NumberField", Some("input")));
        assert!(!is_inline_field("Input", Some("field")));
    }
}
