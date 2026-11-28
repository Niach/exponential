//! eui-core's `Measure` answered by zgui's text stack: the SAME parley
//! shaper (`zgui_text_parley::Shaper` over the app's `Fonts`) the painted
//! `text` elements are laid out with, so the height the core reserves is
//! the height zgui paints. Control sizes follow eui-gpui's measurer
//! (`crates/eui-gpui/src/measure.rs` `measure_leaf`) for the trial's
//! components; every answer is the leaf's BORDER box (`ControlBox`).

use std::collections::HashMap;
use std::sync::Arc;

use eui_core::measure::{ControlBox, HeightRequest, Intrinsics, LeafRequest, Measure, TextStyle};
use serde_json::{Map, Value};
use zgui::geom::CssPx;
use zgui_interned::Ident;
use zgui_text::{BreakRequest, ParagraphCache, ParagraphContent, StyledRun, TextMap, lay_out};
use zgui_text_parley::Shaper;
use zgui_text_style as zts;

use crate::paint::{display_text, str_prop};

/// What a paragraph's lines are shaped as: `pre-wrap` (newlines kept,
/// wrapping at the width) or `nowrap` (one line, a `lines: 1` text).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Wrap {
    PreWrap,
    NoWrap,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct StyleKey {
    family: Option<String>,
    size: u32,
    weight: u16,
    line_height: u32,
    italic: bool,
    tracking: u32,
    wrap: Wrap,
}

/// The measurer: one shaper, its paragraph cache, the lowered styles.
pub struct ZMeasure {
    shaper: Shaper,
    cache: ParagraphCache<<Shaper as zgui_text::ParagraphShaper>::Engine>,
    styles: HashMap<StyleKey, Arc<zts::TextStyle>>,
    paragraph: zts::ParagraphStyle,
    pub calls: u64,
}

/// The shaped size of one paragraph: width, height, first baseline, lines.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shaped {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub lines: usize,
}

impl ZMeasure {
    pub fn new(fonts: &zgui::app::Fonts) -> Self {
        ZMeasure { shaper: fonts.shaper(), cache: ParagraphCache::new(), styles: HashMap::new(), paragraph: zts::ParagraphStyle::initial(), calls: 0 }
    }

    /// The zgui text style the painter's CSS lowers to (`paint::text_css`).
    pub fn style(&mut self, ts: &TextStyle, wrap: Wrap) -> Arc<zts::TextStyle> {
        let key = StyleKey {
            family: ts.font_family.clone(),
            size: (ts.font_size * 64.0) as u32,
            weight: ts.font_weight,
            line_height: (ts.line_height * 64.0) as u32,
            italic: ts.font_style.as_deref() == Some("italic"),
            tracking: (ts.letter_spacing.unwrap_or(0.0) * 64.0) as u32,
            wrap,
        };
        if let Some(s) = self.styles.get(&key) {
            return s.clone();
        }
        let mut s = zts::TextStyle::initial();
        s.family = crate::fonts::stack_names(ts.font_family.as_deref())
            .into_iter()
            .map(|n| match n.as_str() {
                "sans-serif" => zts::FamilyName::Generic(zts::GenericFamily::SansSerif),
                "monospace" => zts::FamilyName::Generic(zts::GenericFamily::Monospace),
                _ => zts::FamilyName::Named(Ident::new(&n)),
            })
            .collect();
        s.size = CssPx(ts.font_size);
        s.weight = ts.font_weight as f32;
        if key.italic {
            s.slant = zts::FontSlant::Italic;
        }
        s.line_height = zts::LineHeight::Length(CssPx(ts.line_height));
        s.letter_spacing = zts::LengthPercent { length: CssPx(ts.letter_spacing.unwrap_or(0.0)), percent: 0.0 };
        s.white_space = zts::WhiteSpaceCollapse::Preserve;
        s.wrap_mode = if wrap == Wrap::NoWrap { zts::WrapMode::NoWrap } else { zts::WrapMode::Wrap };
        let s = Arc::new(s);
        self.styles.insert(key, s.clone());
        s
    }

    /// Shape `text` at `wrap` width (`None` = max-content, `Some(0)` = min-content).
    pub fn shape(&mut self, text: &str, ts: &TextStyle, wrap: Option<f32>, mode: Wrap, lines: Option<u32>) -> Shaped {
        if text.is_empty() {
            return Shaped::default();
        }
        let style = self.style(ts, mode);
        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        let runs = [StyledRun { text: 0..text.len(), style, brush: zgui_scene::PaintSlot(0) }];
        let content = ParagraphContent { text, map: &map, runs: &runs, boxes: &[], paragraph: &self.paragraph, scale: 1.0 };
        let request = BreakRequest::new(&content, wrap.map(CssPx));
        let (_, broken) = lay_out(&mut self.shaper, &mut self.cache, &content, &request);
        let g = &broken.geometry;
        let n = g.lines.len().max(1);
        let shown = lines.filter(|l| *l > 0).map_or(n, |l| n.min(l as usize));
        let width = g.lines.iter().take(shown).map(|l| l.width.0).fold(0.0_f32, f32::max);
        let height = if shown == n { g.size.height.0 } else { g.lines.get(shown).map(|l| l.top.0).unwrap_or(g.size.height.0) };
        Shaped { width: width.ceil(), height, baseline: g.first_baseline().map(|b| b.0).unwrap_or(ts.line_height * 0.75), lines: shown }
    }

    pub fn line(&mut self, text: &str, ts: &TextStyle) -> f32 {
        self.shape(text, ts, None, Wrap::NoWrap, Some(1)).width
    }

    fn baseline(&mut self, ts: &TextStyle) -> f32 {
        self.shape("x", ts, None, Wrap::NoWrap, Some(1)).baseline
    }

    /// A text paragraph as eui-gpui's `para`: pre-wrap; a `lines: 1` text is
    /// nowrap (its min-content the whole line).
    fn para(&mut self, raw: &str, ts: &TextStyle, wrap: Option<f32>, lines: Option<u32>) -> (f32, f32, Option<f32>) {
        if raw.is_empty() {
            return (0.0, 0.0, None);
        }
        let single = lines == Some(1);
        let shown = transform(raw, ts.text_transform.as_deref());
        let s = if single { self.shape(&shown, ts, None, Wrap::NoWrap, Some(1)) } else { self.shape(&shown, ts, wrap, Wrap::PreWrap, lines) };
        let w = match wrap {
            Some(x) if x > 0.0 && single => s.width.min(x),
            _ => s.width,
        };
        (w, s.height.max(ts.line_height * s.lines.max(1) as f32), Some(s.baseline))
    }

    /// One leaf at one border-box wrap width: `(width, height, baseline)`.
    pub fn measure_leaf(&mut self, leaf: &LeafRequest, wrap: Option<f32>) -> (f32, f32, Option<f32>) {
        self.calls += 1;
        let c = leaf.control;
        let inner = inner_wrap(wrap, &c);
        let props = leaf.props;
        let ts = leaf.text_style;
        let plain = |(w, h): (f32, f32)| (w, h, None);
        let content: (f32, f32, Option<f32>) = match (leaf.component, leaf.part) {
            ("Extension", _) => return (0.0, 0.0, None),
            ("Select" | "DatePicker" | "DateRangePicker" | "TimePicker", Some("trigger")) => {
                let text = str_prop(props, "text");
                let text = if text.is_empty() { placeholder(props) } else { text.to_string() };
                let gap = if c.gap > 0.0 { c.gap } else { 8.0 };
                (self.line(&text, ts) + gap + 16.0, ts.line_height, Some(self.baseline(ts)))
            }
            ("Text", part) => {
                let shown = display_text(props.get("text"));
                if part == Some("lineNumber") {
                    let digits = props.get("digits").and_then(Value::as_f64).unwrap_or(1.0).max(1.0) as usize;
                    let (w, h, b) = self.para(&shown, ts, None, Some(1));
                    (w.max(self.line(&"0".repeat(digits), ts)), h, b)
                } else {
                    self.para(&shown, ts, inner, leaf.lines)
                }
            }
            ("Markdown", _) => {
                let (w, h) = crate::paint::markdown_size(self, props, ts, c.gap, inner);
                (w, h, None)
            }
            ("Button" | "Toggle", _) => {
                let label = str_prop(props, "label");
                let has_icon = !str_prop(props, "icon").is_empty();
                let icon_only = matches!(str_prop(props, "size"), "icon" | "icon-xs");
                let icon = if has_icon || icon_only { 16.0 } else { 0.0 };
                let label_w = if icon_only || label.is_empty() { 0.0 } else { self.line(label, ts) };
                let gap = if has_icon && label_w > 0.0 { c.gap } else { 0.0 };
                let h = ts.line_height.max(icon);
                let b = (label_w > 0.0).then(|| (h - ts.line_height) / 2.0 + self.baseline(ts));
                (icon + gap + label_w, h, b)
            }
            ("Link", _) => {
                let label = str_prop(props, "label");
                (self.line(label, ts), ts.line_height, Some(self.baseline(ts)))
            }
            ("CountBadge", _) => {
                let label = display_text(props.get("text"));
                (self.line(&label, ts), ts.line_height, Some(self.baseline(ts)))
            }
            ("Icon", _) => plain((16.0, 16.0)),
            ("Avatar", _) => plain((32.0, 32.0)),
            ("Spinner", _) => plain((20.0, 20.0)),
            ("Ring", _) => plain((32.0, 32.0)),
            ("Input", Some("field")) => (160.0, ts.line_height, Some(self.baseline(ts))),
            ("Textarea", Some("field")) => {
                let rows = props.get("rows").and_then(Value::as_f64).unwrap_or(3.0).max(1.0) as f32;
                (160.0, rows * ts.line_height, Some(self.baseline(ts)))
            }
            ("Checkbox", Some("box" | "checkbox")) | ("Radio", Some("dot")) => plain((16.0, 16.0)),
            ("Switch", Some("track")) => plain((32.0, 20.0)),
            ("Slider", Some("track")) => plain((160.0, 16.0)),
            ("Input" | "Select" | "DatePicker" | "NumberField" | "TimePicker" | "DateRangePicker" | "ChipInput", None) => (160.0, 36.0, None),
            ("Textarea", None) => plain((160.0, 72.0)),
            ("Switch", None) => plain((44.0, 24.0)),
            ("Checkbox" | "Radio", None) => plain((16.0, 16.0)),
            _ => plain((0.0, 0.0)),
        };
        let (w, h) = border_box((content.0, content.1), &c);
        let baseline = content.2.map(|b| {
            let fixed = c.height.is_some_and(|fh| (fh - (content.1 + insets(&c).1)).abs() > 0.5);
            if fixed || matches!(leaf.part, Some("trigger" | "field" | "input" | "search")) || matches!(leaf.component, "Button" | "Toggle") {
                ((h - content.1) / 2.0).max(0.0) + b
            } else {
                top_inset(&c) + b
            }
        });
        (w, h, baseline)
    }
}

impl Measure for ZMeasure {
    fn measure_id(&self) -> u64 {
        0x7a67_7569
    }

    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves
            .iter()
            .map(|leaf| {
                let (max_w, h, baseline) = self.measure_leaf(leaf, None);
                let (min_w, _, _) = self.measure_leaf(leaf, Some(0.0));
                Intrinsics { min_content_width: min_w.min(max_w), max_content_width: max_w, height_at_max_content: h, baseline }
            })
            .collect()
    }

    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests.iter().map(|r| leaves.iter().find(|l| l.index == r.index).map(|leaf| self.measure_leaf(leaf, Some(r.width)).1).unwrap_or(0.0)).collect()
    }
}

fn placeholder(props: &Map<String, Value>) -> String {
    props.get("placeholder").and_then(Value::as_str).unwrap_or("").to_string()
}

pub fn transform(s: &str, t: Option<&str>) -> String {
    match t {
        Some("uppercase") => s.to_uppercase(),
        Some("lowercase") => s.to_lowercase(),
        _ => s.to_string(),
    }
}

pub fn border_box(content: (f32, f32), c: &ControlBox) -> (f32, f32) {
    let (ih, iv) = insets(c);
    let w = c.width.unwrap_or(content.0 + ih).max(c.min_width.unwrap_or(0.0));
    let h = c.height.unwrap_or(content.1 + iv).max(c.min_height.unwrap_or(0.0));
    (w, h)
}

pub fn inner_wrap(wrap: Option<f32>, c: &ControlBox) -> Option<f32> {
    let (h, _) = insets(c);
    wrap.map(|w| if w <= 0.0 { 0.0 } else { (w - h).max(0.0) })
}

pub fn insets(c: &ControlBox) -> (f32, f32) {
    let p = c.padding;
    if p == [0.0; 4] {
        (2.0 * (c.padding_horizontal + c.border_width), 2.0 * (c.padding_vertical + c.border_width))
    } else {
        (p[1] + p[3] + 2.0 * c.border_width, p[0] + p[2] + 2.0 * c.border_width)
    }
}

pub fn top_inset(c: &ControlBox) -> f32 {
    let p = c.padding;
    (if p == [0.0; 4] { c.padding_vertical } else { p[0] }) + c.border_width
}
