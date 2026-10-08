//! The host MEASURE contract. The VAPP-4 verdict: never one FFI upcall per
//! taffy measure call (taffy asks a leaf 10–15 times per pass). The core
//! batches: ONE upcall for the intrinsic widths of every dirty leaf (with the
//! resolved text style, so Compose can pick the font before its first pass),
//! taffy runs against a Rust-side memo, ONE upcall for the heights at the
//! widths taffy decided, and a third round only when a height changed a width
//! decision (counted in `LayoutOutput::measure_rounds`). In-process painters
//! (gpui) may answer per request through [`DirectMeasure`]; nothing crosses
//! an FFI there.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Typography a measured leaf is laid out with (the theme's resolved recipe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle { font_size: 14.0, font_weight: 400, line_height: 20.0, font_family: None }
    }
}

/// The control box a leaf's recipe fixes (padding inside the measured size,
/// definite sizes taffy already knows). The measurer adds `padding_*` around
/// the content it shapes and honours `min_*`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ControlBox {
    pub padding_horizontal: f32,
    pub padding_vertical: f32,
    pub border_width: f32,
    pub gap: f32,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// One leaf the host must measure. `index` names the layout node; the host
/// keys its own caches on it. `lines` = the `Text` prop (truncate to N lines).
#[derive(Debug, Clone)]
pub struct LeafRequest<'a> {
    pub index: u32,
    pub id: &'a str,
    /// The native kind (`Text`, `Button`, `Input`, `Extension`…).
    pub component: &'a str,
    /// The synthetic part of a native when the leaf is one (`Tabs/tab`).
    pub part: Option<&'a str>,
    pub props: &'a Map<String, Value>,
    pub text_style: &'a TextStyle,
    pub control: ControlBox,
    pub lines: Option<u32>,
}

impl LeafRequest<'_> {
    /// The text a textual leaf shows.
    pub fn text(&self) -> &str {
        self.props
            .get("text")
            .or_else(|| self.props.get("label"))
            .or_else(|| self.props.get("title"))
            .or_else(|| self.props.get("content"))
            .or_else(|| self.props.get("placeholder"))
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

/// What the host answers for one leaf: the width it needs with every line
/// break taken (`min_content`), with none (`max_content`), and the height at
/// `max_content` width (one line for text). Controls with a fixed box return
/// it in both widths.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Intrinsics {
    pub min_content_width: f32,
    pub max_content_width: f32,
    pub height_at_max_content: f32,
}

/// Second phase: the height of leaf `index` when laid out `width` wide.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HeightRequest {
    pub index: u32,
    pub width: f32,
}

/// Implemented by the host. Both calls receive batches; the core never calls
/// them per taffy request.
pub trait Measure {
    /// Identity of this measurer (font scale, Dynamic Type state…). Two
    /// consecutive passes with different ids invalidate every memo.
    fn measure_id(&self) -> u64 {
        2
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics>;
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32>;
}

/// The per-request form for in-process painters (gpui): one closure that
/// answers `(leaf, wrap_width)` with a size. `None` = unconstrained
/// (max-content); `Some(0.0)` = min-content.
pub struct DirectMeasure<F>(pub F, pub u64);

impl<F: FnMut(&LeafRequest, Option<f32>) -> (f32, f32)> Measure for DirectMeasure<F> {
    fn measure_id(&self) -> u64 {
        self.1
    }

    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves
            .iter()
            .map(|leaf| {
                let (max_w, h) = (self.0)(leaf, None);
                let (min_w, _) = (self.0)(leaf, Some(0.0));
                Intrinsics { min_content_width: min_w, max_content_width: max_w, height_at_max_content: h }
            })
            .collect()
    }

    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests
            .iter()
            .map(|r| leaves.iter().find(|l| l.index == r.index).map(|leaf| (self.0)(leaf, Some(r.width)).1).unwrap_or(0.0))
            .collect()
    }
}

pub const FIXED_CHAR_WIDTH: f32 = 8.0;
pub const FIXED_LINE_HEIGHT: f32 = 20.0;

/// The geometry-test measure every platform reproduces exactly: 8 px per
/// character on ONE 20 px line (no wrapping), fixed boxes per control, or an
/// explicit size per node id (`fixtures/layout-geometry.json` `measures`).
#[derive(Debug, Clone, Default)]
pub struct FixedMeasure {
    pub sizes: HashMap<String, (f32, f32)>,
    /// When true, text leaves WRAP at 8 px per character (for the list and
    /// overlay tests); the geometry fixture keeps it off.
    pub wrap: bool,
}

impl FixedMeasure {
    pub fn with_sizes(sizes: HashMap<String, (f32, f32)>) -> FixedMeasure {
        FixedMeasure { sizes, wrap: false }
    }

    fn chars(s: &str) -> f32 {
        s.chars().count() as f32
    }

    /// The intrinsic size of a leaf by kind (the spike's table).
    pub fn intrinsic(&self, leaf: &LeafRequest) -> (f32, f32) {
        if let Some(size) = self.sizes.get(leaf.id) {
            return *size;
        }
        let text = leaf.text();
        let str_prop = |k: &str| leaf.props.get(k).and_then(Value::as_str).unwrap_or("");
        let c = leaf.control;
        let (w, h) = match leaf.component {
            "Text" | "Link" => (FIXED_CHAR_WIDTH * Self::chars(text), leaf.text_style.line_height.max(FIXED_LINE_HEIGHT)),
            "Markdown" => {
                let lines = text.lines().count().max(1) as f32;
                let widest = text.lines().map(Self::chars).fold(0.0, f32::max);
                (FIXED_CHAR_WIDTH * widest, FIXED_LINE_HEIGHT * lines)
            }
            "Button" | "Toggle" => (FIXED_CHAR_WIDTH * Self::chars(str_prop("label")) + 24.0, c.height.unwrap_or(36.0)),
            "Input" | "Select" | "DatePicker" => (160.0, c.height.unwrap_or(36.0)),
            "Textarea" | "Composer" => (160.0, c.height.unwrap_or(72.0)),
            "Switch" => (44.0, 24.0),
            "Checkbox" | "Radio" => (16.0, 16.0),
            "Avatar" => {
                let s = c.width.unwrap_or(32.0);
                (s, s)
            }
            "Icon" => (c.width.unwrap_or(16.0), c.height.unwrap_or(16.0)),
            "Image" | "Video" => (320.0, 180.0),
            "Slider" => (160.0, 16.0),
            "Spinner" | "Ring" => (c.width.unwrap_or(20.0), c.height.unwrap_or(20.0)),
            "Skeleton" => (
                leaf.props.get("width").and_then(Value::as_f64).unwrap_or(120.0) as f32,
                leaf.props.get("height").and_then(Value::as_f64).unwrap_or(16.0) as f32,
            ),
            _ => (c.width.unwrap_or(0.0), c.height.unwrap_or(0.0)),
        };
        (c.width.unwrap_or(w).max(c.min_width.unwrap_or(0.0)), c.height.unwrap_or(h).max(c.min_height.unwrap_or(0.0)))
    }

    fn wraps(&self, leaf: &LeafRequest) -> bool {
        self.wrap && !self.sizes.contains_key(leaf.id) && matches!(leaf.component, "Text" | "Markdown")
    }
}

impl Measure for FixedMeasure {
    fn measure_id(&self) -> u64 {
        1
    }

    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves
            .iter()
            .map(|leaf| {
                let (w, h) = self.intrinsic(leaf);
                let min = if self.wraps(leaf) {
                    leaf.text().split_whitespace().map(Self::chars).fold(0.0, f32::max) * FIXED_CHAR_WIDTH
                } else {
                    w
                };
                Intrinsics { min_content_width: min, max_content_width: w, height_at_max_content: h }
            })
            .collect()
    }

    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests
            .iter()
            .map(|r| {
                let Some(leaf) = leaves.iter().find(|l| l.index == r.index) else { return 0.0 };
                let (w, h) = self.intrinsic(leaf);
                if !self.wraps(leaf) || r.width >= w || r.width <= 0.0 {
                    return h;
                }
                let per_line = (r.width / FIXED_CHAR_WIDTH).floor().max(1.0);
                let lines = (Self::chars(leaf.text()) / per_line).ceil().max(1.0);
                let lines = match leaf.lines {
                    Some(n) if n > 0 => lines.min(n as f32),
                    _ => lines,
                };
                lines * FIXED_LINE_HEIGHT
            })
            .collect()
    }
}

/// The Rust-side memo taffy's measure closure answers from. Keyed by layout
/// node index; `content_version` bumps clear a leaf's entries.
#[derive(Debug, Default, Clone)]
pub struct MeasureMemo {
    pub intrinsics: HashMap<u32, Intrinsics>,
    /// (index, width in 1/64 px) → height.
    pub heights: HashMap<(u32, i64), f32>,
}

impl MeasureMemo {
    pub fn width_key(width: f32) -> i64 {
        (width * 64.0).round() as i64
    }

    pub fn clear(&mut self) {
        self.intrinsics.clear();
        self.heights.clear();
    }

    pub fn forget(&mut self, index: u32) {
        self.intrinsics.remove(&index);
        self.heights.retain(|(i, _), _| *i != index);
    }

    pub fn height_at(&self, index: u32, width: f32) -> Option<f32> {
        self.heights.get(&(index, Self::width_key(width))).copied()
    }
}

/// A measured leaf's answer for one taffy request, from the memo. `width`
/// taffy already knows wins; else the available space picks min/max content;
/// a definite available width clamps max-content (text wraps).
#[derive(Debug, Clone, Copy)]
pub struct MeasureRequest {
    pub index: u32,
    pub known_width: Option<f32>,
    pub known_height: Option<f32>,
    pub available_width: taffy::AvailableSpace,
    pub available_height: taffy::AvailableSpace,
}

impl MeasureMemo {
    /// Answer from the memo. The second value says whether the HEIGHT was a
    /// guess (the max-content height at a narrower width), i.e. a second
    /// phase must ask the host for it.
    pub fn answer(&self, req: &MeasureRequest) -> (taffy::geometry::Size<f32>, bool) {
        let Some(i) = self.intrinsics.get(&req.index) else {
            return (taffy::geometry::Size { width: req.known_width.unwrap_or(0.0), height: req.known_height.unwrap_or(0.0) }, false);
        };
        let width = req.known_width.unwrap_or(match req.available_width {
            // CSS: a definite available width shrinks the content down to its
            // min-content and no further (the box overflows instead).
            taffy::AvailableSpace::Definite(w) => w.clamp(i.min_content_width.min(i.max_content_width), i.max_content_width),
            taffy::AvailableSpace::MinContent => i.min_content_width,
            taffy::AvailableSpace::MaxContent => i.max_content_width,
        });
        if let Some(h) = req.known_height {
            return (taffy::geometry::Size { width, height: h }, false);
        }
        // No line-break opportunity (min == max) = the height never changes
        // with the width: no second phase.
        if width + 0.01 >= i.max_content_width || (i.max_content_width - i.min_content_width).abs() < 0.01 {
            return (taffy::geometry::Size { width, height: i.height_at_max_content }, false);
        }
        match self.height_at(req.index, width) {
            Some(h) => (taffy::geometry::Size { width, height: h }, false),
            None => (taffy::geometry::Size { width, height: i.height_at_max_content }, true),
        }
    }
}
