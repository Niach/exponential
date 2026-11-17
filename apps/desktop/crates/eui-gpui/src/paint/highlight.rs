//! #EDE-95 (`fixtures/text-highlight.json`): genUI's search marks on the
//! gpui painter. The rule is the core's ([`eui_core::text_highlight`]); this
//! paints it: `$color.match` behind each match and `$color.matchCurrent`
//! behind the current one, a quad per match piece and line from the font's
//! ascent to its descent around the line's baseline, the glyph box as inline
//! code's fill (`documentPaint.code`; never the line height), painted under
//! the text ([`marked`]; the text keeps its colour, the layout never moves). A Markdown searches each block of the
//! markdown-render.json normal form apart ([`markdown_highlight_texts`]: a
//! paragraph, heading, list item or quote = its runs joined; each table
//! cell, the header first; never a code block, image, attachment or rule),
//! a link by its label, inline code by its text, a host chip as ONE
//! [`HIGHLIGHT_BARRIER`] (chips stay whole).

use std::collections::HashMap;
use std::ops::Range;

use eui_core::text_highlight::{highlight_local_current, highlight_ranges, HighlightSpec, HIGHLIGHT_BARRIER};
use eui_core::theme::{Mode, ResolvedTheme};
use gpui::{point, px, size, AnyElement, App, Bounds, Element, ElementId, Font, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, TextAlign, TextLayout, TextRun, Window};

use super::markdown::{parse, Block, BlockKind, Inline, CODE_CHIP};
use super::parts::theme_color;

/// The two mark fills.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkColors {
    pub matched: Hsla,
    pub current: Hsla,
}

impl MarkColors {
    /// `$color.match` / `$color.matchCurrent` of the theme (the core derives
    /// them from `warning` when a chain sets none).
    pub fn of(theme: Option<&ResolvedTheme>, mode: Mode) -> MarkColors {
        let warning = theme_color(theme, mode, "warning").unwrap_or(gpui::yellow());
        MarkColors {
            matched: theme_color(theme, mode, "match").unwrap_or(warning.opacity(0.3)),
            current: theme_color(theme, mode, "matchCurrent").unwrap_or(warning.opacity(0.6)),
        }
    }

    pub fn fill(&self, current: bool) -> Hsla {
        if current {
            self.current
        } else {
            self.matched
        }
    }
}

/// A painted mark: a byte range of the painted string and its fill.
pub type Mark = (Range<usize>, Hsla);

/// The marks of one text (its matches, the local current one in
/// `matchCurrent`).
pub fn text_marks(text: &str, spec: &HighlightSpec, colors: MarkColors) -> Vec<Mark> {
    let current = highlight_local_current(spec);
    highlight_ranges(text, &spec.query).into_iter().enumerate().map(|(i, r)| (r, colors.fill(Some(i) == current))).collect()
}

/// A mark's fill in one font: a byte range of the painted string, its
/// colour and the run's font (`None` = the painting text style's), whose
/// ascent and descent make the box.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkFill {
    pub range: Range<usize>,
    pub color: Hsla,
    pub font: Option<Font>,
}

/// Marks over a string in the inherited text style (one font).
pub fn plain_fills(marks: &[Mark]) -> Vec<MarkFill> {
    marks.iter().filter(|(r, _)| !r.is_empty()).map(|(r, c)| MarkFill { range: r.clone(), color: *c, font: None }).collect()
}

/// Marks over `runs`' string, cut at the runs' ends (each piece takes its
/// run's font).
pub fn mark_fills(runs: &[TextRun], marks: &[Mark]) -> Vec<MarkFill> {
    let mut out = Vec::new();
    if marks.is_empty() {
        return out;
    }
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        for (r, c) in marks {
            let (a, b) = (r.start.max(start), r.end.min(end));
            if a < b {
                out.push(MarkFill { range: a..b, color: *c, font: Some(run.font.clone()) });
            }
        }
    }
    out.sort_by_key(|f| f.range.start);
    out
}

/// `child` (painting `layout`, a string `len` bytes long) with `fills`
/// painted under it ([`paint_fills`]); no fills = `child` as is.
pub fn marked(child: impl IntoElement, layout: TextLayout, len: usize, fills: Vec<MarkFill>) -> AnyElement {
    if fills.is_empty() {
        return child.into_any_element();
    }
    MarkedText { child: child.into_any_element(), layout, len, fills }.into_any_element()
}

struct MarkedText {
    child: AnyElement,
    layout: TextLayout,
    len: usize,
    fills: Vec<MarkFill>,
}

impl IntoElement for MarkedText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MarkedText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.prepaint(window, cx);
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        paint_fills(&self.layout, self.len, &self.fills, window);
        self.child.paint(window, cx);
    }
}

/// Each fill as a quad per visual line it lands on: x from the shaped
/// line's positions of its ends (the text's alignment applied as gpui
/// paints it), y = the line's baseline - the fill font's ascent .. +
/// its descent. A truncated text (gpui's `…`) fills up to the ellipsis.
fn paint_fills(layout: &TextLayout, len: usize, fills: &[MarkFill], window: &mut Window) {
    let Some(geometry) = fill_rects(layout, len, fills, window) else { return };
    for (bounds, color) in geometry {
        window.paint_quad(gpui::fill(bounds, color));
    }
}

fn fill_rects(layout: &TextLayout, len: usize, fills: &[MarkFill], window: &Window) -> Option<Vec<(Bounds<Pixels>, Hsla)>> {
    let bounds = layout.bounds();
    let lh = layout.line_height();
    let shown = layout.len();
    // A truncated layout ends in its ellipsis: nothing past the kept text.
    let limit = if shown < len { shown.saturating_sub("…".len()) } else { len };
    let style = window.text_style();
    let ts = window.text_system().clone();
    let base = ts.resolve_font(&style.font());
    let mut out = Vec::new();
    let mut top = bounds.origin.y;
    let mut line_start = 0;
    for wrapped in layout.line_layouts() {
        let l = &wrapped.unwrapped_layout;
        // As gpui lays a line: half-leading, then the line's ascent.
        let baseline_in_row = (lh - l.ascent - l.descent) / 2.0 + l.ascent;
        // The visual rows: (start byte, start x, end byte, end x).
        let mut rows = Vec::with_capacity(wrapped.wrap_boundaries.len() + 1);
        let (mut from, mut from_x) = (0usize, px(0.0));
        for b in &wrapped.wrap_boundaries {
            let g = &l.runs[b.run_ix].glyphs[b.glyph_ix];
            rows.push((from, from_x, g.index, g.position.x));
            (from, from_x) = (g.index, g.position.x);
        }
        rows.push((from, from_x, l.len, l.width));
        for (k, (r0, x0, r1, x1)) in rows.iter().copied().enumerate() {
            let row_top = top + lh * k as f32;
            let baseline = row_top + baseline_in_row;
            let room = bounds.size.width;
            let left = match style.text_align {
                TextAlign::Left => bounds.origin.x,
                TextAlign::Center => bounds.origin.x + (room - (x1 - x0)) / 2.0,
                TextAlign::Right => bounds.origin.x + room - (x1 - x0),
            } - x0;
            for f in fills {
                let a = f.range.start.max(line_start + r0).min(limit);
                let b = f.range.end.min(line_start + r1).min(limit);
                if a >= b {
                    continue;
                }
                let (xa, xb) = (l.x_for_index(a - line_start), l.x_for_index(b - line_start));
                let (xa, xb) = (xa.min(xb), xa.max(xb));
                if xb <= xa {
                    continue;
                }
                let id = f.font.as_ref().map_or(base, |font| ts.resolve_font(font));
                let ascent = ts.ascent(id, l.font_size);
                let descent = ts.descent(id, l.font_size).abs();
                out.push((Bounds { origin: point(left + xa, baseline - ascent), size: size(xb - xa, ascent + descent) }, f.color));
            }
        }
        top += lh * rows.len() as f32;
        line_start += l.len + 1;
    }
    Some(out)
}

/// Each line's marks (line-local byte ranges): `lines` are the pieces of
/// `source` in order (a wrap drops the hanging spaces and the breaks), the
/// marks are `source`'s, cut at the line ends.
pub fn line_marks(source: &str, lines: &[String], marks: &[Mark]) -> Vec<Vec<Mark>> {
    let mut from = 0;
    lines
        .iter()
        .map(|line| {
            let Some(found) = source.get(from..).and_then(|rest| rest.find(line.as_str())) else { return Vec::new() };
            let start = from + found;
            let end = start + line.len();
            from = end;
            marks.iter().filter(|(r, _)| r.start < end && r.end > start).map(|(r, c)| (r.start.max(start) - start..r.end.min(end) - start, *c)).collect()
        })
        .collect()
}

/// The marks of `lines` joined by a one-byte separator, each line shifted
/// by its prefix (`prefixes[i]` = bytes put before line `i`'s own text,
/// e.g. a bidi mark).
pub fn joined_marks(per_line: &[Vec<Mark>], lens: &[usize], prefixes: &[usize]) -> Vec<Mark> {
    let mut out = Vec::new();
    let mut at = 0;
    for (i, marks) in per_line.iter().enumerate() {
        let shift = at + prefixes.get(i).copied().unwrap_or(0);
        out.extend(marks.iter().map(|(r, c)| (r.start + shift..r.end + shift, *c)));
        at += prefixes.get(i).copied().unwrap_or(0) + lens.get(i).copied().unwrap_or(0) + 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

/// The searched text of some spans: a host chip = one barrier, inline code
/// its text (its run holds a placeholder once sized), the rest as is.
pub fn inlines_highlight_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .filter(|span| !span.text.is_empty())
        .map(|span| match &span.chip {
            Some(chip) if chip.kind == CODE_CHIP => chip.label.clone(),
            Some(_) => HIGHLIGHT_BARRIER.to_string(),
            None => span.text.clone(),
        })
        .collect()
}

/// A Markdown unit: (block index, table row (0 = the header), cell); a
/// text block is `(i, 0, 0)`.
pub type MdUnitKey = (usize, usize, usize);

/// The searched units of the blocks, in document order.
pub fn markdown_highlight_units(blocks: &[Block]) -> Vec<(MdUnitKey, String)> {
    let mut out = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        match &b.kind {
            BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::ListItem { .. } | BlockKind::Quote => out.push(((i, 0, 0), inlines_highlight_text(&b.inlines))),
            BlockKind::Table { header, rows } => {
                for (r, row) in std::iter::once(header).chain(rows.iter()).enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        out.push(((i, r, c), inlines_highlight_text(cell)));
                    }
                }
            }
            BlockKind::CodeBlock | BlockKind::Rule | BlockKind::Image { .. } | BlockKind::Attachment { .. } => {}
        }
    }
    out
}

/// The searched text of each block, in document order (the TS
/// `markdownHighlightTexts`).
pub fn markdown_highlight_texts(blocks: &[Block]) -> Vec<String> {
    markdown_highlight_units(blocks).into_iter().map(|(_, t)| t).collect()
}

/// A Markdown's matches (what a host numbers).
pub fn markdown_highlight_count(text: &str, query: &str) -> usize {
    markdown_highlight_texts(&parse(text)).iter().map(|t| eui_core::text_highlight::highlight_count(t, query)).sum()
}

/// Each searched unit's marks (byte ranges into its searched text), the
/// current one numbered across the units in order.
pub fn markdown_marks(blocks: &[Block], spec: &HighlightSpec, colors: MarkColors) -> HashMap<MdUnitKey, Vec<Mark>> {
    let current = highlight_local_current(spec);
    let mut n = 0;
    let mut out = HashMap::new();
    for (key, text) in markdown_highlight_units(blocks) {
        let ranges = highlight_ranges(&text, &spec.query);
        if ranges.is_empty() {
            continue;
        }
        let marks = ranges
            .into_iter()
            .map(|r| {
                let mark = (r, colors.fill(Some(n) == current));
                n += 1;
                mark
            })
            .collect();
        out.insert(key, marks);
    }
    out
}

/// A unit's marks split onto what paints them: the runs' string (the
/// spans' texts joined, as [`super::markdown::runs`] builds it) and each
/// inline code chip's label (by its index among the spans' chips).
pub fn split_unit_marks(inlines: &[Inline], marks: &[Mark]) -> (Vec<Mark>, HashMap<usize, Vec<Mark>>) {
    let mut text_marks = Vec::new();
    let mut chip_marks: HashMap<usize, Vec<Mark>> = HashMap::new();
    if marks.is_empty() {
        return (text_marks, chip_marks);
    }
    let (mut searched, mut shown, mut chip) = (0usize, 0usize, 0usize);
    for span in inlines.iter().filter(|span| !span.text.is_empty()) {
        let (len, code) = match &span.chip {
            Some(c) if c.kind == CODE_CHIP => (c.label.len(), true),
            Some(_) => (HIGHLIGHT_BARRIER.len_utf8(), false),
            None => (span.text.len(), false),
        };
        let (s0, s1) = (searched, searched + len);
        for (r, c) in marks.iter().filter(|(r, _)| r.start < s1 && r.end > s0) {
            let local = r.start.max(s0) - s0..r.end.min(s1) - s0;
            if code {
                chip_marks.entry(chip).or_default().push((local, *c));
            } else if span.chip.is_none() {
                text_marks.push((local.start + shown..local.end + shown, *c));
            }
        }
        if span.chip.is_some() {
            chip += 1;
        }
        searched = s1;
        shown += span.text.len();
    }
    (text_marks, chip_marks)
}
