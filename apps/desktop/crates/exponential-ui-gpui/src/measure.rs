//! The in-process measure (the VAPP-4 verdict): taffy's leaf questions are
//! answered per request through gpui's text system — widths with
//! `shape_line`, wrapped heights with `shape_text(.., Some(wrap))` — and
//! gpui's line-layout cache absorbs the repeats. `None` wrap = max-content,
//! `Some(0.0)` = min-content (the widest word). Every answer is the BORDER
//! box of the control: the leaf's `ControlBox` padding/border are added
//! around the shaped content and its fixed/minimum sizes win.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use exponential_ui::measure::{ControlBox, HeightRequest, Intrinsics, LeafRequest, Measure, TextStyle};
use exponential_ui::theme::{Mode, ResolvedTheme};
use gpui::{px, App, Font, FontStyle, FontWeight, SharedString, TextRun, Window};
use serde_json::{Map, Value};

use crate::extension::ExtensionPainter;
use crate::host::HostPlugin;
use crate::paint::markdown::{self, Inline, MdStyles, MdText, TextSpec};
use crate::paint::parts::{control, default_family, mono_family, part_props, px_prop, spacing};

/// The identity the core keys its memo on (a font change = a new id).
const GPUI_MEASURE_ID: u64 = 0x6770_7569;

/// Font family NAMES → the families gpui loads (`HostPlugin::font_family`),
/// memoized; plus the surface's default (`sans`) and `mono` families.
pub(crate) struct Fonts {
    host: Rc<dyn HostPlugin>,
    map: RefCell<HashMap<String, SharedString>>,
    pub default: SharedString,
    pub mono: SharedString,
}

impl Fonts {
    pub fn new(host: Rc<dyn HostPlugin>, theme: Option<&ResolvedTheme>) -> Fonts {
        let mut f = Fonts { host, map: RefCell::new(HashMap::new()), default: SharedString::default(), mono: SharedString::default() };
        f.set_theme(theme);
        f
    }

    pub fn set_theme(&mut self, theme: Option<&ResolvedTheme>) {
        let default = default_family(theme).unwrap_or_else(|| "Inter".to_string());
        let mono = mono_family(theme).unwrap_or_else(|| "Menlo".to_string());
        self.default = self.map(&default);
        self.mono = self.map(&mono);
    }

    fn map(&self, name: &str) -> SharedString {
        if let Some(hit) = self.map.borrow().get(name) {
            return hit.clone();
        }
        let mapped = self.host.font_family(name);
        self.map.borrow_mut().insert(name.to_string(), mapped.clone());
        mapped
    }

    /// The family for an optional name (`None` = the surface default).
    pub fn family(&self, name: Option<&str>) -> SharedString {
        match name {
            Some(n) if !n.is_empty() => self.map(n),
            _ => self.default.clone(),
        }
    }

    /// The gpui font a text style paints with.
    pub fn font(&self, family: Option<&str>, weight: u16) -> Font {
        make_font(self.family(family), weight)
    }
}

/// A plain font of `family` at `weight`.
pub fn make_font(family: SharedString, weight: u16) -> Font {
    Font { family, features: Default::default(), fallbacks: None, weight: FontWeight(weight as f32), style: FontStyle::Normal }
}

fn run(text: &str, font: Font) -> TextRun {
    TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None }
}

/// Text shaping against a window's text system.
pub(crate) struct Shaper<'w> {
    pub window: &'w Window,
    pub fonts: &'w Fonts,
    pub calls: u64,
}

impl Shaper<'_> {
    /// Max-content width of ONE line (no newlines).
    pub fn line_width(&mut self, text: &str, font: &Font, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.calls += 1;
        let line = self.window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run(text, font.clone())], None);
        f32::from(line.width).ceil()
    }

    pub fn max_content(&mut self, text: &str, font: &Font, size: f32) -> f32 {
        text.split('\n').map(|l| self.line_width(l, font, size)).fold(0.0, f32::max)
    }

    /// Min-content: the widest single word.
    pub fn min_content(&mut self, text: &str, font: &Font, size: f32) -> f32 {
        text.split_whitespace().map(|w| self.line_width(w, font, size)).fold(0.0, f32::max)
    }

    /// Height of `text` wrapped at `wrap` (all wrapped lines, `clamp` lines max).
    pub fn wrapped_height(&mut self, text: &str, font: &Font, size: f32, line_height: f32, wrap: Option<f32>, clamp: Option<u32>) -> f32 {
        if text.is_empty() {
            return line_height;
        }
        self.calls += 1;
        let clamp = clamp.filter(|n| *n > 0).map(|n| n as usize);
        match self.window.text_system().shape_text(SharedString::from(text.to_string()), px(size), &[run(text, font.clone())], wrap.map(|w| px(w.max(1.0))), clamp) {
            Ok(lines) => {
                let h: f32 = lines.iter().map(|l| f32::from(l.size(px(line_height)).height)).sum();
                match clamp {
                    Some(n) => h.min(n as f32 * line_height),
                    None => h,
                }
                .max(line_height)
            }
            Err(_) => line_height,
        }
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
        let font = self.fonts.font(spec.family.as_deref(), spec.weight);
        let text = markdown::plain(inlines);
        self.max_content(&text, &font, spec.size)
    }

    fn widest_word(&mut self, inlines: &[Inline], spec: &TextSpec) -> f32 {
        let font = self.fonts.font(spec.family.as_deref(), spec.weight);
        let text = markdown::plain(inlines);
        self.min_content(&text, &font, spec.size)
    }
}

// ---------------------------------------------------------------------------
// Unit-free helpers
// ---------------------------------------------------------------------------

/// The horizontal and vertical insets a control box adds around its content.
pub fn insets(c: &ControlBox) -> (f32, f32) {
    (2.0 * (c.padding_horizontal + c.border_width), 2.0 * (c.padding_vertical + c.border_width))
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

/// A JSON scalar as display text (`3` → "3").
pub fn display_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => exponential_ui::json::to_js_string(other),
    }
}

/// The chart's legend entries (pie: categories; else series names when > 1).
pub fn chart_legend(props: &Map<String, Value>) -> Vec<String> {
    let kind = str_prop(props, "kind");
    let series = props.get("series").and_then(Value::as_array).cloned().unwrap_or_default();
    if kind == "pie" {
        props.get("categories").and_then(Value::as_array).map(|c| c.iter().map(|v| display_text(Some(v))).collect()).unwrap_or_default()
    } else if series.len() > 1 {
        series.iter().map(|s| display_text(s.get("name"))).collect()
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// The measure
// ---------------------------------------------------------------------------

/// The painter's `Measure`: built per pass from the window, the host's font
/// mapping and the extension painters.
pub struct GpuiMeasure<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
    fonts: &'a Fonts,
    theme: Option<&'a ResolvedTheme>,
    mode: Mode,
    painters: &'a HashMap<String, Rc<dyn ExtensionPainter>>,
    /// Extension node id → its kind (`LeafRequest` carries no kind).
    kinds: &'a HashMap<String, String>,
    calls: u64,
    shaped: u64,
}

impl<'a> GpuiMeasure<'a> {
    pub(crate) fn new(
        window: &'a mut Window,
        cx: &'a mut App,
        fonts: &'a Fonts,
        theme: Option<&'a ResolvedTheme>,
        mode: Mode,
        painters: &'a HashMap<String, Rc<dyn ExtensionPainter>>,
        kinds: &'a HashMap<String, String>,
    ) -> Self {
        GpuiMeasure { window, cx, fonts, theme, mode, painters, kinds, calls: 0, shaped: 0 }
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
        Shaper { window: self.window, fonts: self.fonts, calls: 0 }
    }

    fn font(&self, ts: &TextStyle) -> Font {
        self.fonts.font(ts.font_family.as_deref(), ts.font_weight)
    }

    fn part(&self, component: &str, part: &str, props: &Map<String, Value>, states: &[String]) -> Map<String, Value> {
        part_props(self.theme, self.mode, component, part, props, states)
    }

    /// Plain text content `(w, h)` at an inner wrap width.
    fn text(&mut self, text: &str, ts: &TextStyle, wrap: Option<f32>, lines: Option<u32>) -> (f32, f32) {
        let font = self.font(ts);
        let mut s = self.shaper();
        let single = lines == Some(1);
        let out = match wrap {
            None => (s.max_content(text, &font, ts.font_size), s.wrapped_height(text, &font, ts.font_size, ts.line_height, None, lines)),
            // A one-line (ellipsized) text shrinks to nothing, like CSS
            // `white-space: nowrap; min-width: 0`.
            Some(w) if w <= 0.0 => (if single { 0.0 } else { s.min_content(text, &font, ts.font_size) }, ts.line_height),
            Some(w) => {
                let max = s.max_content(text, &font, ts.font_size);
                let used = if single { max.min(w) } else { max.min(w.max(s.min_content(text, &font, ts.font_size))) };
                let h = if single { ts.line_height } else { s.wrapped_height(text, &font, ts.font_size, ts.line_height, Some(used.max(1.0)), lines) };
                (used, h)
            }
        };
        self.shaped += s.calls;
        out
    }

    fn line(&mut self, text: &str, ts: &TextStyle) -> f32 {
        let font = self.font(ts);
        let mut s = self.shaper();
        let w = s.max_content(text, &font, ts.font_size);
        self.shaped += s.calls;
        w
    }

    /// Text plus fixed-width chrome beside it (tab icon/count, accordion
    /// chevron, menu icon), one line.
    fn text_with_chrome(&mut self, text: &str, ts: &TextStyle, chrome: f32, wrap: Option<f32>) -> (f32, f32) {
        let w = self.line(text, ts) + chrome;
        let used = match wrap {
            None => w,
            Some(x) if x <= 0.0 => chrome,
            Some(x) => w.min(x),
        };
        (used, ts.line_height)
    }

    fn markdown(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32) {
        let text = str_prop(leaf.props, "text");
        let blocks = markdown::parse(text);
        let ts = leaf.text_style;
        let body = TextSpec { size: ts.font_size, line_height: ts.line_height, weight: ts.font_weight, family: ts.font_family.clone() };
        let styles = MdStyles::resolve(self.theme, self.mode, body, leaf.props);
        let mut s = self.shaper();
        let out = match wrap {
            None => {
                let w = markdown::max_content_width(&blocks, &styles, &mut s);
                (w, markdown::layout(&blocks, &styles, w, &mut s).height)
            }
            Some(w) if w <= 0.0 => {
                let w = markdown::min_content_width(&blocks, &styles, &mut s);
                (w, markdown::layout(&blocks, &styles, w, &mut s).height)
            }
            Some(w) => (w, markdown::layout(&blocks, &styles, w, &mut s).height),
        };
        self.shaped += s.calls;
        (out.0, out.1.max(if blocks.is_empty() { 0.0 } else { ts.line_height }))
    }

    fn button(&mut self, leaf: &LeafRequest, component: &str) -> (f32, f32) {
        let props = leaf.props;
        let ts = leaf.text_style;
        let label = str_prop(props, "label");
        let has_icon = !str_prop(props, "icon").is_empty() || props.get("loading").and_then(Value::as_bool) == Some(true);
        let icon_only = str_prop(props, "size") == "icon";
        let icon = if has_icon || icon_only { px_prop(&self.part(component, "icon", props, &[]), "width").unwrap_or_else(|| control(self.theme, "iconSm", 16.0)) } else { 0.0 };
        let label_w = if icon_only || label.is_empty() { 0.0 } else { self.line(label, ts) };
        let gap = if has_icon && label_w > 0.0 { leaf.control.gap } else { 0.0 };
        let w = if has_icon || icon_only { icon } else { 0.0 } + gap + label_w;
        (w, ts.line_height.max(if has_icon { icon } else { 0.0 }))
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
        let w = match wrap {
            None => 320.0,
            Some(x) if x <= 0.0 => 0.0,
            Some(x) => x,
        };
        let mut h = num_prop(props, "height").unwrap_or(200.0) as f32;
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

    fn composer(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32) {
        let props = leaf.props;
        let ts = leaf.text_style;
        let field = self.part("Composer", "field", props, &[]);
        let fs = px_prop(&field, "fontSize").unwrap_or(ts.font_size);
        let lh = px_prop(&field, "lineHeight").unwrap_or(ts.line_height);
        let min_h = px_prop(&field, "minHeight").unwrap_or(lh);
        let send = px_prop(&self.part("Composer", "send", props, &[]), "height").unwrap_or_else(|| control(self.theme, "buttonIcon", 36.0));
        let gap = leaf.control.gap;
        let value = str_prop(props, "value");
        let text = if value.is_empty() { str_prop(props, "placeholder") } else { value };
        let text = if text.is_empty() { "Message" } else { text };
        let field_ts = TextStyle { font_size: fs, line_height: lh, font_weight: ts.font_weight, font_family: ts.font_family.clone() };
        let w = match wrap {
            None => 320.0,
            Some(x) if x <= 0.0 => 120.0,
            Some(x) => x,
        };
        let (_, th) = self.text(text, &field_ts, Some(w.max(1.0)), None);
        (w, th.clamp(min_h, 200.0) + gap + send)
    }

    fn toggle_group(&mut self, leaf: &LeafRequest) -> (f32, f32) {
        let props = leaf.props;
        let items = props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let item = self.part("ToggleGroup", "item", props, &[]);
        let pad = px_prop(&item, "paddingHorizontal").or_else(|| px_prop(&item, "padding")).unwrap_or(12.0);
        let border = px_prop(&item, "borderWidth").unwrap_or(0.0);
        let h = px_prop(&item, "height").unwrap_or(36.0);
        let fs = px_prop(&item, "fontSize").unwrap_or(leaf.text_style.font_size);
        let ts = TextStyle { font_size: fs, font_weight: item.get("fontWeight").and_then(Value::as_u64).map(|w| w as u16).unwrap_or(500), line_height: leaf.text_style.line_height, font_family: item.get("fontFamily").and_then(Value::as_str).map(str::to_string) };
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

    /// Select / DatePicker `.field`: the TRIGGER recipe (the field has none).
    fn trigger(&mut self, leaf: &LeafRequest, component: &str) -> (f32, f32) {
        let props = leaf.props;
        let t = self.part(component, "trigger", props, &[]);
        let fs = px_prop(&t, "fontSize").unwrap_or(leaf.text_style.font_size);
        let lh = px_prop(&t, "lineHeight").unwrap_or(leaf.text_style.line_height);
        let pad = px_prop(&t, "paddingHorizontal").or_else(|| px_prop(&t, "padding")).unwrap_or(12.0);
        let border = px_prop(&t, "borderWidth").unwrap_or(0.0);
        let h = px_prop(&t, "height").unwrap_or(lh + 16.0);
        let ts = TextStyle { font_size: fs, line_height: lh, font_weight: 400, font_family: t.get("fontFamily").and_then(Value::as_str).map(str::to_string) };
        let label = if component == "Select" { select_label(props) } else { date_label(str_prop(props, "value")).unwrap_or_else(|| str_or(props, "placeholder", "Pick a date")) };
        let text_w = self.line(&label, &ts);
        let w = (text_w + 2.0 * (pad + border) + 16.0 + spacing(self.theme, "sm")).max(160.0);
        (w, h)
    }

    /// One leaf at one border-box wrap width.
    pub fn measure_leaf(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32) {
        self.calls += 1;
        let c = leaf.control;
        let inner = inner_wrap(wrap, &c);
        let props = leaf.props;
        let ts = leaf.text_style;
        let owner_component = leaf.component;
        let content = match (leaf.component, leaf.part) {
            ("Extension", _) => return (0.0, 0.0),
            ("Text", Some("tab")) => {
                let icon = if props.get("icon").and_then(Value::as_str).is_some() { 16.0 + 4.0 } else { 0.0 };
                let count = props.get("count").map(|v| display_text(Some(v)));
                let count_w = match count {
                    Some(c) if !c.is_empty() => 4.0 + self.line(&c, ts) + 12.0,
                    _ => 0.0,
                };
                self.text_with_chrome(str_prop(props, "text"), ts, icon + count_w, inner)
            }
            ("Text", Some("trigger")) => {
                let mut text = str_prop(props, "text").to_string();
                if let Some(count) = props.get("count") {
                    text.push_str(&format!(" · {}", display_text(Some(count))));
                }
                let chrome = 16.0 + c.gap.max(8.0);
                self.text_with_chrome(&text, ts, chrome, inner)
            }
            ("Text", Some("item")) => {
                let icon = if props.get("icon").and_then(Value::as_str).is_some() { 16.0 + c.gap.max(8.0) } else { 0.0 };
                self.text_with_chrome(str_prop(props, "text"), ts, icon, inner)
            }
            ("Text", _) => self.text(str_prop(props, "text"), ts, inner, leaf.lines),
            ("Markdown", _) => self.markdown(leaf, inner),
            ("Button" | "Toggle", _) => self.button(leaf, owner_component),
            ("Link", _) => {
                let label = str_or(props, "label", str_prop(props, "href"));
                (self.line(&label, ts), ts.line_height)
            }
            ("Icon", _) => (16.0, 16.0),
            ("Avatar", _) => (32.0, 32.0),
            ("Image", _) => self.media(props, inner, (320.0, 180.0)),
            ("Video", _) => self.media(props, inner, (320.0, 180.0)),
            ("AudioPlayer", _) => {
                let title = str_prop(props, "title");
                let track = if title.is_empty() { 0.0 } else { ts.line_height + spacing(self.theme, "xs") };
                let w = match inner {
                    None => 300.0,
                    Some(x) if x <= 0.0 => 160.0,
                    Some(x) => x,
                };
                (w, track + 40.0)
            }
            ("Spinner", _) => (20.0, 20.0),
            ("Ring", _) => (32.0, 32.0),
            ("Skeleton", _) => self.skeleton(props, inner),
            ("Chart", _) => self.chart(leaf, inner),
            ("Composer", _) => self.composer(leaf, inner),
            ("TreeGuides", _) => {
                let depth = num_prop(props, "depth").unwrap_or(0.0).max(0.0) as f32;
                (depth * 16.0, ts.line_height)
            }
            ("ToggleGroup", _) => self.toggle_group(leaf),
            ("Unknown", _) => {
                let label = self.part("Unknown", "label", props, &[]);
                let lts = TextStyle {
                    font_size: px_prop(&label, "fontSize").unwrap_or(12.0),
                    line_height: px_prop(&label, "lineHeight").unwrap_or(16.0),
                    font_weight: 400,
                    font_family: label.get("fontFamily").and_then(Value::as_str).map(str::to_string),
                };
                let text = unknown_label(props, leaf.component);
                self.text(&text, &lts, inner, None)
            }
            ("Box", Some("indicator")) => {
                let n = num_prop(props, "count").unwrap_or(0.0).max(0.0) as f32;
                let dot = px_prop(&self.part("Carousel", "indicator", props, &[]), "width").unwrap_or(8.0);
                let gap = spacing(self.theme, "xs");
                (n * dot + (n - 1.0).max(0.0) * gap, dot + spacing(self.theme, "sm"))
            }
            ("Input", Some("field")) => (160.0, ts.line_height),
            ("Textarea", Some("field")) => {
                let rows = num_prop(props, "rows").unwrap_or(3.0).max(1.0) as f32;
                (160.0, rows * ts.line_height)
            }
            ("Select" | "DatePicker", Some("field")) => return self.trigger(leaf, leaf.component),
            ("Checkbox", Some("box")) | ("Radio", Some("dot")) => (16.0, 16.0),
            ("Switch", Some("track")) => (32.0, 20.0),
            ("Slider", Some("track")) => {
                let thumb = px_prop(&self.part("Slider", "thumb", props, &[]), "height").unwrap_or_else(|| control(self.theme, "slider", 16.0));
                (160.0, thumb)
            }
            // Geometry mode (no theme): every native is ONE measured leaf.
            ("Input" | "Select" | "DatePicker", None) => (160.0, 36.0),
            ("Textarea", None) => (160.0, 72.0),
            ("Switch", None) => (44.0, 24.0),
            ("Checkbox" | "Radio", None) => (16.0, 16.0),
            ("Slider", None) => (160.0, 16.0),
            _ => (0.0, 0.0),
        };
        border_box(content, &c)
    }

    fn extension(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> Option<(f32, f32)> {
        let kind = self.painter_kind(leaf)?;
        let painter = self.painters.get(&kind)?.clone();
        painter.measure(leaf, wrap, self.window, self.cx)
    }

    fn painter_kind(&self, leaf: &LeafRequest) -> Option<String> {
        self.kinds.get(leaf.id).cloned()
    }

    fn answer(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32) {
        if leaf.component == "Extension" {
            self.calls += 1;
            return self.extension(leaf, wrap).unwrap_or((0.0, 0.0));
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
        GPUI_MEASURE_ID
    }

    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves
            .iter()
            .map(|leaf| {
                let (max_w, h) = self.answer(leaf, None);
                let (min_w, _) = self.answer(leaf, Some(0.0));
                Intrinsics { min_content_width: min_w.min(max_w), max_content_width: max_w, height_at_max_content: h }
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
        let c = ControlBox { padding_horizontal: 16.0, padding_vertical: 0.0, border_width: 1.0, gap: 8.0, min_width: Some(80.0), min_height: None, width: None, height: Some(36.0) };
        assert_eq!(insets(&c), (34.0, 2.0));
        assert_eq!(border_box((20.0, 20.0), &c), (80.0, 36.0));
        assert_eq!(border_box((100.0, 20.0), &c), (134.0, 36.0));
        assert_eq!(inner_wrap(Some(134.0), &c), Some(100.0));
        assert_eq!(inner_wrap(Some(0.0), &c), Some(0.0));
        assert_eq!(inner_wrap(None, &c), None);
        assert_eq!(border_box((10.0, 10.0), &ControlBox::default()), (10.0, 10.0));
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
        assert_eq!(date_label("2026-10-14").as_deref(), Some("Oct 14, 2026"));
        assert_eq!(date_label("nope"), None);
    }

    #[test]
    fn legends_follow_the_chart_kind() {
        let bar = json!({"kind": "bar", "series": [{"name": "Runs"}, {"name": "Fails"}]}).as_object().unwrap().clone();
        assert_eq!(chart_legend(&bar), ["Runs", "Fails"]);
        let single = json!({"kind": "line", "series": [{"name": "Runs"}]}).as_object().unwrap().clone();
        assert!(chart_legend(&single).is_empty());
        let pie = json!({"kind": "pie", "categories": ["a", "b"], "series": [{"values": [1, 2]}]}).as_object().unwrap().clone();
        assert_eq!(chart_legend(&pie), ["a", "b"]);
    }
}
