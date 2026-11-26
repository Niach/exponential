//! Round 4 group B (fixtures/text-runs.json): a Text's `runs` and its
//! `dropCap` on gpui. The spans go through the Markdown span machinery (one
//! inline paragraph: weights, tones, the inline code chip, links, fills);
//! the drop cap is the web's float: a box `lines` × lineHeight tall at the
//! inline start, the spans wrapping beside it for those lines
//! ([`markdown::drop_cap_flow`]), then under it at the full width. The
//! measurer and the painter both lay out through [`RichFlow`], so measure =
//! paint.

use eui_core::measure::TextStyle;
use eui_core::text_runs::{drop_cap_size, drop_cap_split, TextRun};
use gpui::{Font, FontStyle, FontWeight, SharedString};
use serde_json::{Map, Value};

use super::markdown::{self, Block, BlockKind, Inline, MdLayout, MdStyles, MdText, TextSpec};

/// What a Text paints through this path: its spans (the rest after a drop
/// cap) and the cap with its lines. `None` = the plain text path (no runs
/// and no drop cap, or `shimmer`: the joined text plain in the sweep).
pub struct RichSpans {
    pub runs: Vec<TextRun>,
    pub cap: Option<(TextRun, u32)>,
}

/// The core normalised the props (`runs`, `text` joined, `dropCap` lines).
pub fn rich_spans(props: &Map<String, Value>, transform: Option<&str>) -> Option<RichSpans> {
    let lines = props.get("dropCap").and_then(Value::as_u64).unwrap_or(0) as u32;
    let runs: Option<Vec<TextRun>> = props.get("runs").and_then(Value::as_array).map(|a| a.iter().filter_map(TextRun::from_json).collect());
    if props.get("shimmer").and_then(Value::as_bool) == Some(true) || (runs.is_none() && lines == 0) {
        return None;
    }
    let mut runs = runs.unwrap_or_else(|| {
        let text = crate::measure::display_text(props.get("text"));
        if text.is_empty() {
            Vec::new()
        } else {
            vec![TextRun { text, ..TextRun::default() }]
        }
    });
    for r in &mut runs {
        r.text = crate::text::transform(&r.text, transform).into_owned();
    }
    if lines > 0 {
        if let Some((cap, rest)) = drop_cap_split(&runs) {
            return Some(RichSpans { runs: rest, cap: Some((cap, lines)) });
        }
    }
    Some(RichSpans { runs, cap: None })
}

pub fn paragraph(inlines: Vec<Inline>) -> Vec<Block> {
    vec![Block { kind: BlockKind::Paragraph, inlines, list_group: None, lang: None, table_cols: Vec::new() }]
}

pub fn spec_of(ts: &TextStyle) -> TextSpec {
    TextSpec { size: ts.font_size, line_height: ts.line_height, weight: ts.font_weight, family: ts.font_family.clone() }
}

/// The cap's font (the Text's family, the run's weight) and size.
pub fn cap_font(family: SharedString, cap: &TextRun, ts: &TextStyle, lines: u32) -> (Font, f32) {
    let font = Font { family, features: Default::default(), fallbacks: None, weight: FontWeight(cap.font_weight().unwrap_or(ts.font_weight) as f32), style: FontStyle::Normal };
    (font, drop_cap_size(ts.font_size, ts.line_height, lines))
}

/// One laid-out piece: its blocks (chips sized), layout and offset.
pub struct Piece {
    pub blocks: Vec<Block>,
    pub layout: MdLayout,
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

/// The Text's flow at `width`: the spans beside the cap (`cap_w` = its
/// box width; 0 = no cap) and under it.
pub struct RichFlow {
    pub pieces: Vec<Piece>,
    pub height: f32,
}

fn piece(inlines: Vec<Inline>, styles: &MdStyles, width: f32, x: f32, y: f32, text: &mut dyn MdText) -> Piece {
    let blocks = paragraph(inlines);
    let blocks = markdown::size_chips(&blocks, styles, Some(width), text).unwrap_or(blocks);
    let layout = markdown::layout(&blocks, styles, width, text);
    Piece { blocks, layout, x, y, width }
}

/// Lays `inlines` out at `width` beside a cap box `cap_w` wide spanning
/// `lines` (RTL: the cap on the right, the text left of it).
pub fn flow(inlines: &[Inline], styles: &MdStyles, width: f32, cap: Option<(f32, u32)>, rtl: bool, text: &mut dyn MdText) -> RichFlow {
    let lh = styles.body.line_height;
    let Some((cap_w, lines)) = cap else {
        let p = piece(inlines.to_vec(), styles, width, 0.0, 0.0, text);
        let height = p.layout.height;
        return RichFlow { pieces: vec![p], height };
    };
    let boxed = lines as f32 * lh;
    let beside = (width - cap_w).max(1.0);
    // The chips sized for the width beside the cap, then the split.
    let sized = paragraph(inlines.to_vec());
    let sized = markdown::size_chips(&sized, styles, Some(beside), text).unwrap_or(sized);
    let (head, under) = markdown::drop_cap_flow(&sized[0].inlines, &styles.body, beside, lines as usize, text);
    let mut pieces = Vec::new();
    let mut head_h = 0.0;
    if !head.is_empty() {
        let p = piece(head, styles, beside, if rtl { 0.0 } else { cap_w }, 0.0, text);
        head_h = p.layout.height;
        pieces.push(p);
    }
    let mut height = boxed.max(head_h);
    if !under.is_empty() {
        let p = piece(under, styles, width, 0.0, height, text);
        height += p.layout.height;
        pieces.push(p);
    }
    RichFlow { pieces, height }
}

/// Max-content: the cap box + the spans on one line; min-content: the
/// wider of the cap box and the widest unbreakable segment.
pub fn intrinsic(inlines: &[Inline], styles: &MdStyles, cap: Option<(f32, u32)>, min: bool, text: &mut dyn MdText) -> f32 {
    let blocks = paragraph(inlines.to_vec());
    let free = markdown::size_chips(&blocks, styles, None, text).unwrap_or(blocks);
    let content = if min { markdown::min_content_width(&free, styles, text) } else { markdown::max_content_width(&free, styles, text) };
    match cap {
        None => content,
        Some((cap_w, _)) if min => content.max(cap_w),
        Some((cap_w, _)) => cap_w + content,
    }
}

/// Measure-only spans (colours never change a width; every link counts).
pub fn measure_inlines(runs: &[TextRun]) -> Vec<Inline> {
    markdown::text_run_inlines(runs, &|_| None, gpui::black(), &|h| Some(h.to_string()))
}

/// The props the Markdown styles resolve under (none: the document look).
pub fn style_props() -> Map<String, Value> {
    Map::new()
}
