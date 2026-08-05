use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::*;

use super::shaped::ChipShapedText;
use super::{Block, InlineFootnoteHit, InlineLinkHit, code_highlight_color};
use crate::components::HtmlCssColor;
use crate::theme::ThemeColors;

const SOURCE_LINE_NUMBER_MIN_DIGITS: usize = 2;
const SOURCE_LINE_NUMBER_GAP: f32 = 12.0;
const SOURCE_LINE_NUMBER_DIGIT_WIDTH_RATIO: f32 = 0.62;

fn source_line_count(text: &str) -> usize {
    text.split('\n').count().max(1)
}

fn source_line_number_gutter_width(line_count: usize, font_size: Pixels) -> Pixels {
    let digits = line_count
        .max(1)
        .to_string()
        .len()
        .max(SOURCE_LINE_NUMBER_MIN_DIGITS);
    px(digits as f32 * f32::from(font_size) * SOURCE_LINE_NUMBER_DIGIT_WIDTH_RATIO)
        + px(SOURCE_LINE_NUMBER_GAP)
}

fn source_text_bounds(bounds: Bounds<Pixels>, gutter_width: Pixels) -> Bounds<Pixels> {
    if gutter_width <= px(0.0) {
        return bounds;
    }

    let max_gutter = (f32::from(bounds.size.width) - 1.0).max(0.0);
    let gutter_width = px(f32::from(gutter_width).min(max_gutter));
    Bounds::new(
        point(bounds.origin.x + gutter_width, bounds.origin.y),
        size(
            (bounds.size.width - gutter_width).max(px(1.0)),
            bounds.size.height,
        ),
    )
}

fn source_line_number_tops(lines: &[WrappedLine], line_height: Pixels) -> Vec<Pixels> {
    let mut tops = Vec::with_capacity(lines.len());
    let mut y = Pixels::default();
    for line in lines {
        tops.push(y);
        y += wrapped_line_height(line, line_height);
    }
    tops
}

/// EXP-322 vendoring: drop reference spans that touch an inline code span.
///
/// The host decorator is a plain byte scan with no markdown awareness, so a
/// `` `#EXP-42` `` inside backticks comes back decorated. Code wins over a
/// chip (the same rule `build_text_runs` applies to STYLE), and here it has to
/// win over the injected TEXT as well — otherwise the literal source a code
/// span exists to show would grow an issue title.
fn reference_spans_outside_code(
    input: &Block,
    spans: Vec<crate::host::ReferenceSpan>,
) -> Vec<crate::host::ReferenceSpan> {
    let code_ranges: Vec<Range<usize>> = input
        .inline_spans()
        .iter()
        .filter(|span| span.style.code && !span.range.is_empty())
        .map(|span| span.range.clone())
        .collect();
    if code_ranges.is_empty() {
        return spans;
    }
    spans
        .into_iter()
        .filter(|reference| {
            !code_ranges
                .iter()
                .any(|code| code.start < reference.range.end && reference.range.start < code.end)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_text_runs(
    input: &Block,
    shaped: &ChipShapedText,
    base_run: &TextRun,
    underline_thickness: Pixels,
    link_color: Hsla,
    reference_text_color: Hsla,
    reference_token_color: Hsla,
    code_bg: Hsla,
    show_inline_code_backgrounds: bool,
    mono_family: &SharedString,
) -> Vec<TextRun> {
    // EXP-322 vendoring: runs are built in SHAPED coordinates — the string
    // handed to `shape_text` carries the chips' display-only titles, so every
    // document range has to be lifted through the map first. The run lengths
    // must sum to exactly `shaped.text().len()` or gpui panics while shaping.
    let display_text = shaped.text();
    let spans = input.inline_spans();
    // EXP-261 vendoring: host-decorated `@email` / `#IDENT` pills (render-time
    // only; the tokens stay plain GFM text on serialization).
    // `chip` covers token AND injected title (pill extent, medium weight);
    // `token` is the identifier alone, which is the only half that renders
    // monospace — web puts the `::after` title in the body font.
    let reference_spans: Vec<(
        std::ops::Range<usize>,
        std::ops::Range<usize>,
        crate::host::ReferenceKind,
    )> = shaped
        .spans()
        .iter()
        .enumerate()
        .filter_map(|(index, span)| {
            let chip = shaped.chip_range(index)?;
            let token = shaped.token_range(index).unwrap_or_else(|| chip.clone());
            Some((chip, token, span.kind))
        })
        .collect();
    let mut boundaries = vec![0, display_text.len()];
    for span in spans {
        let range = shaped.to_shaped_range(span.range.clone());
        boundaries.push(range.start);
        boundaries.push(range.end);
    }
    for (chip, token, _) in &reference_spans {
        boundaries.push(chip.start);
        boundaries.push(chip.end);
        // token.start is a distinct boundary whenever the chip carries a
        // leading icon gutter (EXP-423) — without it the gutter and the
        // identifier merge into one run and the identifier loses its mono
        // + muted styling. Dedup absorbs the gutter-less case where
        // token.start == chip.start.
        boundaries.push(token.start);
        boundaries.push(token.end);
    }
    if let Some(marked_range) = input.marked_range.as_ref() {
        let range = shaped.to_shaped_range(marked_range.clone());
        boundaries.push(range.start);
        boundaries.push(range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let marked_range = input.marked_range.as_ref();
    let mut runs = Vec::new();
    let mut span_idx = 0usize;
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start >= end {
            continue;
        }

        // Spans are stored in ascending order and boundaries are sorted, so
        // we can advance a single index instead of re-scanning per boundary.
        // Their ranges are DOCUMENT offsets, so compare in shaped coordinates.
        while span_idx < spans.len()
            && shaped.to_shaped(spans[span_idx].range.end) <= start
        {
            span_idx += 1;
        }
        let active_span = spans.get(span_idx).filter(|span| {
            let range = shaped.to_shaped_range(span.range.clone());
            range.start <= start && start < range.end
        });

        let inline_style = active_span.map(|s| s.style).unwrap_or_default();
        let html_style = active_span.and_then(|s| s.html_style);
        let is_link = active_span.map(|s| s.link.is_some()).unwrap_or(false);
        let is_footnote = active_span.map(|s| s.footnote.is_some()).unwrap_or(false);
        let is_marked = marked_range
            .map(|range| {
                let range = shaped.to_shaped_range(range.clone());
                start < range.end && range.start < end
            })
            .unwrap_or(false);

        let mut font = base_run.font.clone();
        if inline_style.bold && font.weight < FontWeight::BOLD {
            font.weight = FontWeight::BOLD;
        }
        if inline_style.italic {
            font.style = FontStyle::Italic;
        }
        // EXP-261 vendoring: inline code renders in the monospace family.
        if inline_style.code {
            font.family = mono_family.clone();
        }

        let mut run_color = if is_link || is_footnote {
            link_color
        } else {
            base_run.color
        };
        if let Some(style) = html_style
            && let Some(color) = style.color
        {
            run_color = html_css_color_to_hsla(color, run_color);
        }
        let underline = (inline_style.underline || is_marked || is_link || is_footnote).then_some(
            UnderlineStyle {
                color: Some(run_color),
                thickness: underline_thickness,
                wavy: false,
            },
        );
        let strikethrough = inline_style.strikethrough.then_some(StrikethroughStyle {
            color: Some(run_color),
            thickness: underline_thickness,
        });

        let mut background_color = if show_inline_code_backgrounds && inline_style.code {
            Some(code_bg)
        } else {
            base_run.background_color
        };
        if let Some(style) = html_style
            && let Some(color) = style.background_color
        {
            background_color = Some(html_css_color_to_hsla(color, run_color));
        }

        // EXP-261 vendoring: reference pills. EXP-322: the background is a
        // rounded quad painted in `prepaint`, so the run itself carries NO
        // background — a flat rect here would double-paint under the pill
        // and square its corners off. EXP-423 Linear look: the identifier —
        // and ONLY the identifier — renders monospace and MUTED
        // (`reference_token_text`); the injected title takes
        // `reference_text` (foreground) at MEDIUM weight. Gutter glyphs sit
        // in the chip-but-not-token band → title styling (they are blank
        // NBSPs anyway). Code style wins (a token inside a code span stays
        // code).
        if !inline_style.code
            && let Some((_, token, kind)) = reference_spans
                .iter()
                .find(|(chip, _, _)| chip.start <= start && start < chip.end)
        {
            if font.weight < FontWeight::MEDIUM {
                font.weight = FontWeight::MEDIUM;
            }
            if *kind == crate::host::ReferenceKind::IssueRef {
                if token.start <= start && start < token.end {
                    font.family = mono_family.clone();
                    run_color = reference_token_color;
                } else {
                    run_color = reference_text_color;
                }
            }
        }

        runs.push(TextRun {
            len: end - start,
            font,
            color: run_color,
            background_color,
            underline,
            strikethrough,
        });
    }

    if runs.is_empty() {
        vec![base_run.clone()]
    } else {
        runs
    }
}

fn html_css_color_to_hsla(color: HtmlCssColor, current_color: Hsla) -> Hsla {
    match color {
        HtmlCssColor::CurrentColor => current_color,
        HtmlCssColor::Rgba(color) => Hsla::from(Rgba {
            r: color.red as f32 / 255.0,
            g: color.green as f32 / 255.0,
            b: color.blue as f32 / 255.0,
            a: color.alpha.clamp(0.0, 1.0),
        }),
    }
}

fn build_code_text_runs(
    input: &Block,
    display_text: &SharedString,
    base_run: &TextRun,
    underline_thickness: Pixels,
    colors: &ThemeColors,
) -> Vec<TextRun> {
    let highlight_spans = input
        .code_highlight_result()
        .map(|r| r.spans.as_slice())
        .unwrap_or(&[]);
    let mut boundaries = vec![0, display_text.len()];
    for span in highlight_spans {
        boundaries.push(span.range.start);
        boundaries.push(span.range.end);
    }
    if let Some(marked_range) = input.marked_range.as_ref() {
        boundaries.push(marked_range.start);
        boundaries.push(marked_range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let marked_range = input.marked_range.as_ref();
    let mut runs = Vec::new();
    let mut span_idx = 0usize;
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start >= end {
            continue;
        }

        let is_marked = marked_range
            .map(|range| start < range.end && range.start < end)
            .unwrap_or(false);
        while span_idx < highlight_spans.len() && highlight_spans[span_idx].range.end <= start {
            span_idx += 1;
        }
        let run_color = highlight_spans
            .get(span_idx)
            .filter(|span| span.range.start <= start && start < span.range.end)
            .map(|span| code_highlight_color(colors, span.class))
            .unwrap_or(base_run.color);

        runs.push(TextRun {
            len: end - start,
            font: base_run.font.clone(),
            color: run_color,
            background_color: base_run.background_color,
            underline: is_marked.then_some(UnderlineStyle {
                color: Some(run_color),
                thickness: underline_thickness,
                wavy: false,
            }),
            strikethrough: None,
        });
    }

    if runs.is_empty() {
        vec![base_run.clone()]
    } else {
        runs
    }
}

/// Compute byte ranges of each hard-line (`\n`-separated) segment in the
/// visible text.  Index `i` in the returned Vec corresponds to the `i`-th
/// WrappedLine produced by `shape_text`.
pub(super) fn hard_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (idx, _) in text.match_indices('\n') {
        ranges.push(start..idx);
        start = idx + 1;
    }
    ranges.push(start..text.len());
    ranges
}

/// Map a flat visible-text offset to `(line_index, offset_within_line)`.
pub(super) fn line_index_for_offset(ranges: &[Range<usize>], offset: usize) -> (usize, usize) {
    let clamped = offset.min(ranges.last().map(|r| r.end).unwrap_or(0));
    for (i, range) in ranges.iter().enumerate() {
        if clamped <= range.end {
            return (i, clamped.saturating_sub(range.start));
        }
    }
    let last = ranges.len() - 1;
    (last, ranges[last].len())
}

pub(crate) fn aligned_line_left(
    line: &WrappedLine,
    bounds: Bounds<Pixels>,
    align: TextAlign,
) -> Pixels {
    let slack = (bounds.size.width - line.width()).max(px(0.0));
    match align {
        TextAlign::Left => bounds.left(),
        TextAlign::Center => bounds.left() + slack / 2.0,
        TextAlign::Right => bounds.left() + slack,
    }
}

pub(super) fn wrapped_line_height(line: &WrappedLine, line_height: Pixels) -> Pixels {
    line.size(line_height).height
}

pub(super) fn wrapped_line_top(
    lines: &[WrappedLine],
    line_height: Pixels,
    line_idx: usize,
) -> Pixels {
    lines.iter().take(line_idx).fold(px(0.0), |height, line| {
        height + wrapped_line_height(line, line_height)
    })
}

pub(super) fn wrapped_line_for_y(
    lines: &[WrappedLine],
    line_height: Pixels,
    relative_y: Pixels,
) -> Option<(usize, Pixels)> {
    if lines.is_empty() {
        return None;
    }

    let mut top = px(0.0);
    for (line_idx, line) in lines.iter().enumerate() {
        let height = wrapped_line_height(line, line_height);
        if relative_y < top + height || line_idx + 1 == lines.len() {
            // EXP-282 vendoring: clamp into the line's OWN band. The last hard
            // line used to hand back an unclamped `y_in_line`, and gpui's
            // `LineLayout::_index_for_position` computes its wrapped-row index
            // as `y / line_height` and answers `Err(0)` once that runs past
            // the line's wrap boundaries — so every click below the last text
            // row (the padding band, a tall block's empty space) snapped the
            // caret to the block START instead of the nearest character on
            // the last row.
            let max_y = (height - px(1.0)).max(px(0.0));
            return Some((line_idx, (relative_y - top).clamp(px(0.0), max_y)));
        }
        top += height;
    }

    Some((lines.len() - 1, px(0.0)))
}

fn wrap_boundary_offset(line: &WrappedLine, wrap_idx: usize) -> Option<usize> {
    let boundary = line.wrap_boundaries().get(wrap_idx)?;
    let run = line.unwrapped_layout.runs.get(boundary.run_ix)?;
    let glyph = run.glyphs.get(boundary.glyph_ix)?;
    Some(glyph.index)
}

fn wrapped_row_offsets(line: &WrappedLine) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(line.wrap_boundaries().len() + 2);
    offsets.push(0);
    for wrap_idx in 0..line.wrap_boundaries().len() {
        if let Some(offset) = wrap_boundary_offset(line, wrap_idx) {
            offsets.push(offset.min(line.len()));
        }
    }
    offsets.push(line.len());
    offsets.dedup();
    offsets
}

fn wrapped_row_origin_x(
    line: &WrappedLine,
    bounds: Bounds<Pixels>,
    align: TextAlign,
    row_start: usize,
    row_end: usize,
) -> Pixels {
    let row_width =
        line.unwrapped_layout.x_for_index(row_end) - line.unwrapped_layout.x_for_index(row_start);
    let align_width = line.width();
    let slack = (align_width - row_width).max(px(0.0));
    let line_left = aligned_line_left(line, bounds, align);
    match align {
        TextAlign::Left => line_left,
        TextAlign::Center => line_left + slack / 2.0,
        TextAlign::Right => line_left + slack,
    }
}

pub(super) fn position_for_offset(
    line: &WrappedLine,
    offset: usize,
    line_height: Pixels,
    prefer_next_wrap_start: bool,
) -> Option<Point<Pixels>> {
    let offsets = wrapped_row_offsets(line);
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let is_start_of_wrapped_row = prefer_next_wrap_start && row_idx > 0 && offset == row_start;
        if is_start_of_wrapped_row || (offset >= row_start && offset < row_end) {
            let row_start_x = line.unwrapped_layout.x_for_index(row_start);
            let x = line.unwrapped_layout.x_for_index(offset) - row_start_x;
            return Some(point(x, line_height * row_idx as f32));
        }
    }

    line.position_for_index(offset, line_height)
}

pub(super) fn cursor_bounds_for_offset(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    offset: usize,
    align: TextAlign,
    cursor_width: Pixels,
) -> Option<Bounds<Pixels>> {
    let ranges = hard_line_ranges(text);
    let (line_idx, offset_in_line) = line_index_for_offset(&ranges, offset);
    let layout = lines.get(line_idx)?;
    let origin_x = aligned_line_left(layout, bounds, align);
    let cursor_pos = position_for_offset(layout, offset_in_line, line_height, true)?;
    let y_offset = bounds.top() + wrapped_line_top(lines, line_height, line_idx);
    Some(Bounds::new(
        point(origin_x + cursor_pos.x, y_offset + cursor_pos.y),
        size(cursor_width, line_height),
    ))
}

pub(super) fn range_bounds(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
    align: TextAlign,
) -> Option<Bounds<Pixels>> {
    let segments = range_segment_bounds(lines, bounds, line_height, text, range.clone(), align);
    if segments.is_empty() {
        return cursor_bounds_for_offset(
            lines,
            bounds,
            line_height,
            text,
            range.start,
            align,
            px(1.0),
        );
    }

    let mut union = segments[0];
    for segment in segments.iter().skip(1) {
        union = Bounds::from_corners(
            point(
                union.left().min(segment.left()),
                union.top().min(segment.top()),
            ),
            point(
                union.right().max(segment.right()),
                union.bottom().max(segment.bottom()),
            ),
        );
    }
    Some(union)
}

fn range_segment_bounds_for_hard_line(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    line_idx: usize,
    start_offset: usize,
    end_offset: usize,
    align: TextAlign,
) -> Vec<Bounds<Pixels>> {
    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };
    let line_top = bounds.top() + wrapped_line_top(lines, line_height, line_idx);
    let offsets = wrapped_row_offsets(line);
    let mut segments = Vec::new();

    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let seg_start = start_offset.max(row_start).min(row_end);
        let seg_end = end_offset.min(row_end).max(row_start);
        if seg_start >= seg_end {
            continue;
        }

        let row_start_x = line.unwrapped_layout.x_for_index(row_start);
        let start_x = line.unwrapped_layout.x_for_index(seg_start) - row_start_x;
        let end_x = line.unwrapped_layout.x_for_index(seg_end) - row_start_x;
        let origin_x = wrapped_row_origin_x(line, bounds, align, row_start, row_end);
        let y = line_top + line_height * row_idx as f32;
        segments.push(Bounds::from_corners(
            point(origin_x + start_x, y),
            point(origin_x + end_x, y + line_height),
        ));
    }

    segments
}

pub(super) fn range_segment_bounds(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
    align: TextAlign,
) -> Vec<Bounds<Pixels>> {
    if range.start >= range.end || lines.is_empty() {
        return Vec::new();
    }

    let ranges = hard_line_ranges(text);
    let (start_line, start_offset) = line_index_for_offset(&ranges, range.start);
    let (end_line, end_offset) = line_index_for_offset(&ranges, range.end);
    let mut segments = Vec::new();

    for line_idx in start_line..=end_line {
        let hard_range = &ranges[line_idx];
        let line_start = if line_idx == start_line {
            start_offset
        } else {
            0
        };
        let line_end = if line_idx == end_line {
            end_offset
        } else {
            hard_range.len()
        };
        segments.extend(range_segment_bounds_for_hard_line(
            lines,
            bounds,
            line_height,
            line_idx,
            line_start,
            line_end,
            align,
        ));
    }

    segments
}

fn point_inside_bounds(bounds: Bounds<Pixels>, position: Point<Pixels>) -> bool {
    position.x >= bounds.left()
        && position.x < bounds.right()
        && position.y >= bounds.top()
        && position.y < bounds.bottom()
}

pub(crate) fn link_at_position<'a>(
    input: &'a Block,
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<&'a InlineLinkHit> {
    if input.is_source_raw_mode()
        || input.display_text().is_empty()
        || lines.is_empty()
        || position.y < bounds.top()
        || position.y >= bounds.bottom()
    {
        return None;
    }

    // EXP-322 vendoring: hit-testing runs against the SHAPED text the lines
    // were laid out from, so document ranges are lifted through the map.
    let shaped = input.shaped_for_layout();
    let text = shaped.text().as_ref();
    let align = input.text_align();

    for span in input.inline_spans() {
        let Some(link) = span.link.as_ref() else {
            continue;
        };
        if span.range.is_empty() {
            continue;
        }

        for link_bounds in range_segment_bounds(
            lines,
            bounds,
            line_height,
            text,
            shaped.to_shaped_range(span.range.clone()),
            align,
        ) {
            if point_inside_bounds(link_bounds, position) {
                return Some(link);
            }
        }
    }

    None
}

/// EXP-261 vendoring: resolve the decorated `@email` / `#IDENT` reference
/// under a pointer position (mirrors [`link_at_position`]).
pub(crate) fn reference_at_position(
    input: &Block,
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<(crate::host::ReferenceKind, String)> {
    if input.is_source_raw_mode()
        || input.display_text().is_empty()
        || lines.is_empty()
        || position.y < bounds.top()
        || position.y >= bounds.bottom()
    {
        return None;
    }

    input.environment.reference_decorator.as_ref()?;
    // EXP-322 vendoring: reuse the scan the layout already did instead of
    // re-running the decorator, and hit-test the WHOLE chip (token + title) so
    // clicking the title opens the issue. The returned value stays the bare
    // token — the host resolves an identifier, not a label.
    let shaped = input.shaped_for_layout();
    let document = input.display_text();
    let text = shaped.text().as_ref();
    let align = input.text_align();

    for (index, span) in shaped.spans().iter().enumerate() {
        let Some(chip) = shaped.chip_range(index) else {
            continue;
        };
        for span_bounds in
            range_segment_bounds(lines, bounds, line_height, text, chip.clone(), align)
        {
            if point_inside_bounds(span_bounds, position) {
                let token = document.get(span.range.clone()).unwrap_or_default();
                return Some((span.kind, token.to_string()));
            }
        }
    }

    None
}

pub(crate) fn footnote_at_position<'a>(
    input: &'a Block,
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<&'a InlineFootnoteHit> {
    if input.is_source_raw_mode()
        || input.display_text().is_empty()
        || lines.is_empty()
        || position.y < bounds.top()
        || position.y >= bounds.bottom()
    {
        return None;
    }

    let shaped = input.shaped_for_layout();
    let text = shaped.text().as_ref();
    let align = input.text_align();

    for span in input.inline_spans() {
        let Some(footnote) = span.footnote.as_ref() else {
            continue;
        };
        if span.range.is_empty() {
            continue;
        }

        for footnote_bounds in range_segment_bounds(
            lines,
            bounds,
            line_height,
            text,
            shaped.to_shaped_range(span.range.clone()),
            align,
        ) {
            if point_inside_bounds(footnote_bounds, position) {
                return Some(footnote);
            }
        }
    }

    None
}

/// Custom low-level [`Element`] that renders a block's inline-formatted
/// text with selection highlights and a blinking cursor.
///
/// Supports multi-line text (used by code blocks) via hard `\n` breaks.
/// Each `\n` produces a separate `WrappedLine` from the text shaper.
pub struct BlockTextElement {
    input: Entity<Block>,
    is_placeholder: bool,
    placeholder_text: Option<SharedString>,
    placeholder_color: Option<Hsla>,
}

/// Single-line text element used to edit a fenced code block's info string.
pub struct CodeLanguageInputElement {
    input: Entity<Block>,
    placeholder: SharedString,
}

impl CodeLanguageInputElement {
    pub fn new(input: Entity<Block>, placeholder: SharedString) -> Self {
        Self { input, placeholder }
    }
}

pub struct CodeLanguageInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    hitbox: Option<Hitbox>,
}

impl IntoElement for CodeLanguageInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CodeLanguageInputElement {
    type RequestLayoutState = ();
    type PrepaintState = CodeLanguageInputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let theme = self.input.read(cx).environment.theme.clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(theme.dimensions.code_language_input_height)
            .max(window.line_height())
            .into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let theme = input.environment.theme.clone();
        let content = input.code_language_text().to_string();
        let is_placeholder = content.is_empty();
        let display_text: SharedString = if is_placeholder {
            self.placeholder.clone()
        } else {
            content.into()
        };
        let focused = input.code_language_focus_handle.is_focused(window);
        let style = window.text_style();
        let run_color = if is_placeholder {
            theme.colors.code_language_input_placeholder
        } else {
            theme.colors.code_language_input_text
        };
        let base_run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: run_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = input
            .code_language_marked_range
            .as_ref()
            .filter(|_| !is_placeholder)
        {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..base_run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run_color),
                        thickness: px(theme.dimensions.underline_thickness),
                        wavy: false,
                    }),
                    ..base_run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..base_run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![base_run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let line_height = bounds.size.height;
        let selection = if focused && !input.code_language_selected_range.is_empty() {
            let start = line.x_for_index(input.code_language_selected_range.start);
            let end = line.x_for_index(input.code_language_selected_range.end);
            Some(fill(
                Bounds::from_corners(
                    point(bounds.left() + start, bounds.top()),
                    point(bounds.left() + end, bounds.bottom()),
                ),
                theme.colors.selection,
            ))
        } else {
            None
        };
        let cursor = if focused
            && input.code_language_selected_range.is_empty()
            && input.cursor_visible()
        {
            let cursor_x = line.x_for_index(input.code_language_cursor_offset());
            Some(fill(
                Bounds::new(
                    point(bounds.left() + cursor_x, bounds.top()),
                    size(px(theme.dimensions.cursor_width), line_height),
                ),
                theme.colors.cursor,
            ))
        } else {
            None
        };
        let hitbox = Some(window.insert_hitbox(bounds, HitboxBehavior::Normal));

        CodeLanguageInputPrepaintState {
            line: Some(line),
            cursor,
            selection,
            hitbox,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(hitbox) = prepaint.hitbox.as_ref()
            && hitbox.is_hovered(window)
        {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }

        let focus_handle = self.input.read(cx).code_language_focus_handle.clone();
        if focus_handle.is_focused(window) {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let line = prepaint.line.take().expect("line should be shaped");
        line.paint(
            bounds.origin,
            bounds.size.height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.code_language_last_layout = Some(line);
            input.code_language_last_bounds = Some(bounds);
        });
    }
}

impl BlockTextElement {
    pub fn new(input: Entity<Block>, is_placeholder: bool) -> Self {
        Self {
            input,
            is_placeholder,
            placeholder_text: None,
            placeholder_color: None,
        }
    }

    pub fn with_placeholder(
        input: Entity<Block>,
        is_placeholder: bool,
        placeholder_text: SharedString,
        placeholder_color: Option<Hsla>,
    ) -> Self {
        Self {
            input,
            is_placeholder,
            placeholder_text: Some(placeholder_text),
            placeholder_color,
        }
    }
}

/// EXP-322 vendoring: what `request_layout` hands `prepaint` — the shaped
/// lines plus the document↔shaped offset map they were shaped with.
pub struct BlockLayoutState {
    lines: Rc<RefCell<Option<Vec<WrappedLine>>>>,
    shaped: ChipShapedText,
}

/// Prepared text layout and paint geometry for one `BlockTextElement` frame.
pub struct PrepaintState {
    lines: Vec<WrappedLine>,
    source_line_numbers: Vec<ShapedLine>,
    source_line_number_gutter_width: Pixels,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
    code_backgrounds: Vec<PaintQuad>,
    /// EXP-423: status icons painted into chip gutters — `(icon box, svg
    /// asset path, tint)`.
    chip_icons: Vec<(Bounds<Pixels>, SharedString, Hsla)>,
    line_height: Pixels,
    hitbox: Hitbox,
    /// EXP-322 vendoring: the map the lines above were shaped with.
    shaped: Option<ChipShapedText>,
}

impl IntoElement for BlockTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BlockTextElement {
    type RequestLayoutState = BlockLayoutState;
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let theme = input.environment.theme.clone();
        let shared_text = input.shared_display_text();
        let is_placeholder = self.is_placeholder;
        let show_inline_code_backgrounds = !input.is_source_raw_mode();
        let show_source_line_numbers = input.show_source_line_numbers();
        let source_line_count = source_line_count(shared_text.as_ref());
        let style = window.text_style();

        let (document_text, text_color): (SharedString, Hsla) = if is_placeholder {
            (
                self.placeholder_text
                    .clone()
                    .unwrap_or_else(|| theme.placeholders.empty_editing.clone().into()),
                self.placeholder_color
                    .unwrap_or(theme.colors.text_placeholder),
            )
        } else {
            (shared_text, style.color)
        };
        // EXP-322 vendoring: one decorator scan per frame, feeding BOTH the
        // shaped string (document text + the chips' display-only titles) and
        // the offset map every consumer of the layout reads back through.
        // Binding the map to the layout it produced is what keeps them from
        // ever going stale relative to each other.
        //
        // Three surfaces must never see an injected title, because their text
        // is not prose: placeholder text is not the document at all, a code
        // BLOCK is literal source (and its syntax highlight spans are document
        // offsets, so an injection would shift every colour after the first
        // token), and SourceRaw mode's whole job is showing the literal
        // markdown bytes. Inline code is dropped span-by-span below — same
        // rule as the run builder's "code style wins", applied to the TEXT.
        //
        // While an IME composition is live the injection is skipped too, so
        // composition offsets behave exactly as they did before the chips
        // existed; the spans still ride along, so the pills keep their styling
        // and stay clickable instead of flattening mid-composition.
        let decorator =
            (!is_placeholder && !input.kind().is_code_block() && !input.is_source_raw_mode())
                .then(|| input.environment.reference_decorator.as_ref())
                .flatten();
        let shaped = match decorator {
            Some(decorator) => {
                let spans =
                    reference_spans_outside_code(input, decorator.scan(document_text.as_ref()));
                if input.marked_range.is_some() {
                    ChipShapedText::identity_with_spans(&document_text, spans)
                } else {
                    ChipShapedText::build(&document_text, spans)
                }
            }
            None => ChipShapedText::identity(&document_text),
        };
        let display_text = shaped.text().clone();

        let mut run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // EXP-261 vendoring: code blocks render in the theme's monospace
        // family (upstream had no mono stack at all).
        if !is_placeholder && input.kind().is_code_block() {
            run.font.family = SharedString::from(theme.fonts.mono_family.clone());
        }

        let runs: Vec<TextRun> = if !is_placeholder {
            if input.kind().is_code_block() {
                build_code_text_runs(
                    input,
                    &display_text,
                    &run,
                    px(theme.dimensions.underline_thickness),
                    &theme.colors,
                )
            } else {
                build_text_runs(
                    input,
                    &shaped,
                    &run,
                    px(theme.dimensions.underline_thickness),
                    theme.colors.text_link,
                    theme.colors.reference_text,
                    theme.colors.reference_token_text,
                    theme.colors.code_bg,
                    show_inline_code_backgrounds,
                    &SharedString::from(theme.fonts.mono_family.clone()),
                )
            }
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let source_line_number_gutter_width = show_source_line_numbers
            .then(|| source_line_number_gutter_width(source_line_count, font_size))
            .unwrap_or(px(0.0));

        let shared_lines = Rc::new(RefCell::new(None));
        let shared_lines_clone = shared_lines.clone();
        let layout_width = input.environment.layout_width.clone();

        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        layout_style.min_size.width = px(0.0).into();
        layout_style.max_size.width = relative(1.).into();

        let layout_id = window.request_measured_layout(
            layout_style,
            move |known_dimensions, available_space, window, _cx| {
                let wrap_width = known_dimensions.width.or(match available_space.width {
                    AvailableSpace::Definite(x) => Some(x),
                    AvailableSpace::MinContent => Some(px(1.0)),
                    // EXP-421 soft wrap: MaxContent used to resolve to the
                    // VIEWPORT width, so an unbroken run reported a
                    // near-window intrinsic width and inflated every
                    // ancestor. The recorded slot width (one frame stale;
                    // 0 before the first paint) is the honest wrap bound —
                    // gpui's LineWrapper force-breaks unbroken runs mid-word
                    // once the width is honest.
                    AvailableSpace::MaxContent => {
                        let viewport = window.viewport_size().width.max(px(1.0));
                        let recorded = f32::from_bits(
                            layout_width.load(std::sync::atomic::Ordering::Relaxed),
                        );
                        Some(if recorded > 1.0 {
                            px(recorded).min(viewport)
                        } else {
                            viewport
                        })
                    }
                });
                let text_wrap_width =
                    wrap_width.map(|width| (width - source_line_number_gutter_width).max(px(1.0)));

                match window.text_system().shape_text(
                    display_text.clone(),
                    font_size,
                    &runs,
                    text_wrap_width,
                    None,
                ) {
                    Ok(lines) => {
                        let mut total_size: Size<Pixels> = Size::default();
                        for line in &lines {
                            let ls = line.size(line_height);
                            total_size.height += ls.height;
                            total_size.width = total_size.width.max(ls.width);
                        }
                        total_size.width += source_line_number_gutter_width;
                        *shared_lines_clone.borrow_mut() = Some(lines.into_vec());
                        total_size
                    }
                    Err(_) => Size::default(),
                }
            },
        );

        (
            layout_id,
            BlockLayoutState {
                lines: shared_lines,
                shaped,
            },
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let theme = input.environment.theme.clone();
        let editor_selection_range = input
            .editor_selection_range
            .as_ref()
            .filter(|range| !range.is_empty())
            .cloned();
        let selected_range = editor_selection_range
            .clone()
            .unwrap_or_else(|| input.selected_range.clone());
        let cursor = input.cursor_offset();
        let line_height = window.line_height();
        let focused = input.focus_handle.is_focused(window);
        let show_inline_code_backgrounds = !input.is_source_raw_mode();
        let show_source_line_numbers = input.show_source_line_numbers();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());

        // EXP-322 vendoring: everything below works in SHAPED coordinates —
        // the text that was actually laid out, chips' titles included.
        let shaped = request_layout.shaped.clone();
        let lines = request_layout.lines.borrow_mut().take().unwrap_or_default();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let source_line_number_gutter_width = show_source_line_numbers
            .then(|| source_line_number_gutter_width(lines.len().max(1), font_size))
            .unwrap_or(px(0.0));
        let text_bounds = source_text_bounds(bounds, source_line_number_gutter_width);
        let source_line_numbers = if show_source_line_numbers {
            let run_color = theme.colors.text_placeholder;
            (1..=lines.len().max(1))
                .map(|line_number| {
                    let label = line_number.to_string();
                    window.text_system().shape_line(
                        SharedString::from(label.clone()),
                        font_size,
                        &[TextRun {
                            len: label.len(),
                            font: style.font(),
                            color: run_color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        // Discrete blink: build the cursor quad only when visible, at full
        // opacity — the blink loop notifies solely on visibility flips.
        let cursor_visible = input.cursor_visible();
        let cursor_color = theme.colors.cursor;
        let cursor_width = theme.dimensions.cursor_width;
        let selection_color = theme.colors.selection;
        let text_align = input.text_align();

        let (selection_quads, cursor_quad) =
            if (focused || editor_selection_range.is_some()) && !lines.is_empty() {
                if self.is_placeholder {
                    // Placeholder: cursor after the placeholder text
                    let layout = &lines[0];
                    let origin_x = aligned_line_left(layout, text_bounds, text_align);
                    let cursor_pos = layout
                        .position_for_index(0, line_height)
                        .unwrap_or_default();
                    (
                        vec![],
                        cursor_visible.then(|| {
                            fill(
                                Bounds::new(
                                    point(
                                        origin_x + cursor_pos.x,
                                        text_bounds.top() + cursor_pos.y,
                                    ),
                                    size(px(cursor_width), line_height),
                                ),
                                cursor_color,
                            )
                        }),
                    )
                } else if selected_range.is_empty() {
                    // No selection: just draw the cursor
                    let text = shaped.text().as_ref();
                    (
                        vec![],
                        cursor_bounds_for_offset(
                            &lines,
                            text_bounds,
                            line_height,
                            text,
                            shaped.to_shaped(cursor),
                            text_align,
                            px(cursor_width),
                        )
                        .filter(|_| cursor_visible)
                        .map(|bounds| fill(bounds, cursor_color)),
                    )
                } else {
                    let text = shaped.text().as_ref();
                    let quads = range_segment_bounds(
                        &lines,
                        text_bounds,
                        line_height,
                        text,
                        shaped.to_shaped_range(selected_range),
                        text_align,
                    )
                    .into_iter()
                    .map(|bounds| fill(bounds, selection_color))
                    .collect();
                    (quads, None)
                }
            } else {
                (vec![], None)
            };

        // Compute code-span background quads with rounded corners and padding.
        let mut code_quads = Vec::new();
        if show_inline_code_backgrounds && !self.is_placeholder {
            let text = shaped.text().as_ref();
            let code_color = theme.colors.code_bg;
            let pad_x = px(theme.dimensions.code_bg_pad_x);
            let pad_y = px(theme.dimensions.code_bg_pad_y);
            let radius = px(theme.dimensions.code_bg_radius);
            for span in input.inline_spans() {
                if !span.style.code || span.range.is_empty() {
                    continue;
                }
                for segment in range_segment_bounds(
                    &lines,
                    text_bounds,
                    line_height,
                    text,
                    shaped.to_shaped_range(span.range.clone()),
                    text_align,
                ) {
                    let quad_bounds = Bounds::from_corners(
                        point(segment.left() - pad_x, segment.top() - pad_y),
                        point(segment.right() + pad_x, segment.bottom() + pad_y),
                    );
                    code_quads.push({
                        let mut q = fill(quad_bounds, code_color);
                        q.corner_radii = Corners::all(radius);
                        q
                    });
                }
            }
        }

        // EXP-322 vendoring: the same machinery for resolved `@email` /
        // `#IDENT` references, so a chip reads as a real pill instead of the
        // flat highlight the text run used to carry (web `.issue-ref-pill`).
        let mut chip_icons: Vec<(Bounds<Pixels>, SharedString, Hsla)> = Vec::new();
        if !self.is_placeholder {
            let text = shaped.text().as_ref();
            let reference_color = theme.colors.reference_bg;
            let reference_border = theme.colors.reference_border;
            let pad_x = px(theme.dimensions.reference_pad_x);
            let pad_y = px(theme.dimensions.reference_pad_y);
            // EXP-423: per-kind radius — issue chips are Linear-style small
            // rounded rects, mention pills stay fully round (gpui clamps 999
            // to half the smaller side, so it's "as round as it gets").
            let mention_radius = px(theme.dimensions.reference_radius);
            let issue_radius = px(theme.dimensions.issue_chip_radius);
            // The chip range spans the gutter, token AND title, so the pill
            // wraps all of it — reusing the scan the layout already did.
            for (index, span) in shaped.spans().iter().enumerate() {
                let Some(chip) = shaped.chip_range(index) else {
                    continue;
                };
                let radius = if span.kind == crate::host::ReferenceKind::IssueRef {
                    issue_radius
                } else {
                    mention_radius
                };
                for segment in range_segment_bounds(
                    &lines,
                    text_bounds,
                    line_height,
                    text,
                    chip.clone(),
                    text_align,
                ) {
                    let quad_bounds = Bounds::from_corners(
                        point(segment.left() - pad_x, segment.top() - pad_y),
                        point(segment.right() + pad_x, segment.bottom() + pad_y),
                    );
                    code_quads.push({
                        let mut q = fill(quad_bounds, reference_color);
                        q.corner_radii = Corners::all(radius);
                        // EXP-381: the border is what keeps the pill visible —
                        // the accent fill is nearly the surface colour (web
                        // `.issue-ref-pill` leans on `var(--border)` the same
                        // way).
                        q.border_widths = gpui::Edges::all(px(1.));
                        q.border_color = reference_border;
                        q
                    });
                }
                // EXP-423: the status icon — painted over the FIRST wrapped
                // segment of the NBSP gutter (a soft-wrapped pill shows the
                // icon only on its first row). Skipped entirely on the
                // identity/IME path, where no gutter was injected.
                if let (Some(icon), Some(gutter)) =
                    (span.icon.as_ref(), shaped.icon_gutter_range(index))
                {
                    if let Some(segment) = range_segment_bounds(
                        &lines,
                        text_bounds,
                        line_height,
                        text,
                        gutter,
                        text_align,
                    )
                    .into_iter()
                    .next()
                    {
                        let side = segment.size.height.min(px(14.0));
                        let origin = point(
                            segment.left() + (segment.size.width - side) / 2.0,
                            segment.top() + (segment.size.height - side) / 2.0,
                        );
                        chip_icons.push((
                            Bounds::new(origin, size(side, side)),
                            icon.svg_path.clone(),
                            icon.color,
                        ));
                    }
                }
            }
        }

        PrepaintState {
            lines,
            source_line_numbers,
            source_line_number_gutter_width,
            cursor: cursor_quad,
            selection: selection_quads,
            code_backgrounds: code_quads,
            chip_icons,
            line_height,
            hitbox,
            shaped: Some(shaped),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, hovering_link, hovering_issue_ref) = {
            let input = self.input.read(cx);
            let text_bounds = source_text_bounds(bounds, prepaint.source_line_number_gutter_width);
            let hovering_text = !self.is_placeholder
                && !input.is_source_raw_mode()
                && prepaint.hitbox.is_hovered(window);
            let hovering_link = hovering_text
                && link_at_position(
                    input,
                    &prepaint.lines,
                    text_bounds,
                    prepaint.line_height,
                    window.mouse_position(),
                )
                .is_some();
            // EXP-307 vendoring: issue-ref pills open on plain click, so they
            // advertise it on plain hover (mention pills stay text-cursor —
            // they only edit).
            let hovering_issue_ref = hovering_text
                && reference_at_position(
                    input,
                    &prepaint.lines,
                    text_bounds,
                    prepaint.line_height,
                    window.mouse_position(),
                )
                .is_some_and(|(kind, _)| kind == crate::host::ReferenceKind::IssueRef);
            (input.focus_handle.clone(), hovering_link, hovering_issue_ref)
        };

        if hovering_issue_ref {
            window.set_cursor_style(CursorStyle::PointingHand, &prepaint.hitbox);
        } else if hovering_link {
            // The hand cursor only appears while the Cmd/Ctrl follow modifier is
            // held (matching the gesture that opens the link); a plain hover keeps
            // the text cursor. The editor root repaints on follow-modifier
            // toggles, so this re-evaluates even when the pointer stays still.
            if window.modifiers().secondary() {
                window.set_cursor_style(CursorStyle::PointingHand, &prepaint.hitbox);
            }
        }

        if focus_handle.is_focused(window) {
            let text_bounds = source_text_bounds(bounds, prepaint.source_line_number_gutter_width);
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(text_bounds, self.input.clone()),
                cx,
            );
        }

        // Paint code backgrounds behind text.
        for code_bg in prepaint.code_backgrounds.drain(..) {
            window.paint_quad(code_bg);
        }

        // EXP-423: chip status icons — over the pill quads, under the text
        // (the gutter glyphs are blank NBSPs, so order vs text is cosmetic).
        // `.ok()`: a missing asset (headless tests) must never panic paint.
        for (icon_bounds, svg_path, color) in prepaint.chip_icons.drain(..) {
            window
                .paint_svg(
                    icon_bounds,
                    svg_path,
                    None,
                    gpui::TransformationMatrix::unit(),
                    color,
                    cx,
                )
                .ok();
        }

        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }

        let line_height = prepaint.line_height;
        let lines = std::mem::take(&mut prepaint.lines);
        let text_align = self.input.read(cx).text_align();
        let text_bounds = source_text_bounds(bounds, prepaint.source_line_number_gutter_width);
        let line_number_tops = source_line_number_tops(&lines, line_height);
        let line_number_gap = px(SOURCE_LINE_NUMBER_GAP);
        let line_numbers = std::mem::take(&mut prepaint.source_line_numbers);
        for (line_number, y_offset) in line_numbers.iter().zip(line_number_tops.iter()) {
            let line_number_width = line_number.x_for_index(line_number.len());
            line_number
                .paint(
                    point(
                        text_bounds.left() - line_number_gap - line_number_width,
                        bounds.origin.y + *y_offset,
                    ),
                    line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
        }

        let mut y_offset = Pixels::default();
        for line in &lines {
            let origin_x = aligned_line_left(line, text_bounds, text_align);
            line.paint(
                point(origin_x, text_bounds.origin.y + y_offset),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
            y_offset += wrapped_line_height(line, line_height);
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        let shaped = std::mem::take(&mut prepaint.shaped);
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(lines);
            // EXP-322 vendoring: stored WITH the layout it produced, so every
            // consumer maps through the same map that shaped those lines.
            input.last_shaped = shaped;
            input.last_bounds = Some(text_bounds);
            input.last_line_height = line_height;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        link_at_position, source_line_number_gutter_width, source_line_number_tops,
        source_text_bounds, wrapped_line_height,
    };
    use crate::components::{Block, BlockKind, BlockRecord, InlineTextTree, TableCellPosition};
    use gpui::{
        AppContext, Bounds, Hsla, Modifiers, MouseButton, MouseDownEvent, SharedString,
        TestAppContext, TextAlign, TextRun, VisualTestContext, font, point, px, rgba, size,
    };

    fn shaped_lines(
        text: &str,
        width: gpui::Pixels,
        cx: &mut VisualTestContext,
    ) -> Vec<gpui::WrappedLine> {
        cx.update(|window, _app| {
            window
                .text_system()
                .shape_text(
                    text.to_string().into(),
                    px(16.0),
                    &[TextRun {
                        len: text.len(),
                        font: font(".SystemUIFont"),
                        color: Hsla::from(rgba(0xffffffff)),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    Some(width),
                    None,
                )
                .expect("text should shape")
                .into_vec()
        })
    }

    #[test]
    fn source_line_number_gutter_grows_with_digit_count() {
        let one_digit = source_line_number_gutter_width(9, px(16.0));
        let two_digits = source_line_number_gutter_width(10, px(16.0));
        let three_digits = source_line_number_gutter_width(100, px(16.0));

        assert_eq!(one_digit, two_digits);
        assert!(three_digits > two_digits);
    }

    #[test]
    fn source_text_bounds_are_offset_by_gutter_width() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(300.0), px(120.0)));
        let text_bounds = source_text_bounds(bounds, px(48.0));

        assert_eq!(text_bounds.left(), px(58.0));
        assert_eq!(text_bounds.top(), px(20.0));
        assert_eq!(text_bounds.size.width, px(252.0));
        assert_eq!(text_bounds.size.height, px(120.0));
    }

    #[gpui::test]
    async fn source_line_number_tops_follow_soft_wrapped_hard_lines(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let lines = shaped_lines(
            "this line should wrap before the next hard line\nsecond",
            px(92.0),
            cx,
        );
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "first hard line should soft-wrap"
        );

        let tops = source_line_number_tops(&lines, px(20.0));
        assert_eq!(tops[0], px(0.0));
        assert_eq!(tops[1], wrapped_line_height(&lines[0], px(20.0)));
    }

    #[gpui::test]
    async fn link_hit_matches_only_rendered_link_text(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown("[link](https://example.com)"),
                ),
            )
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(320.0), cx);
        let (hit, miss_right) = block.read_with(cx, |block, _cx| {
            let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(320.0), px(20.0)));
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let layout = &lines[0];
            let start = layout
                .position_for_index(span.range.start, px(20.0))
                .expect("start position");
            let end = layout
                .position_for_index(span.range.end, px(20.0))
                .expect("end position");
            let hit = point((start.x + end.x) / 2.0, px(10.0));
            let miss_right = point(end.x + px(24.0), px(10.0));
            (
                link_at_position(block, &lines, bounds, px(20.0), hit)
                    .map(|link| link.open_target.clone()),
                link_at_position(block, &lines, bounds, px(20.0), miss_right)
                    .map(|link| link.open_target.clone()),
            )
        });

        assert_eq!(hit, Some("https://example.com".to_string()));
        assert_eq!(miss_right, None);
    }

    #[gpui::test]
    async fn secondary_click_follows_link_while_plain_click_edits(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown("a [link](https://example.com) bbbb"),
                ),
            )
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(320.0), cx);
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(320.0), px(20.0)));

        let link_position = block.read_with(cx, |block, _cx| {
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let layout = &lines[0];
            let start = layout
                .position_for_index(span.range.start, px(20.0))
                .expect("start position");
            let end = layout
                .position_for_index(span.range.end, px(20.0))
                .expect("end position");
            point((start.x + end.x) / 2.0, px(10.0))
        });

        block.update(cx, |block, _cx| {
            block.last_layout = Some(lines);
            block.last_bounds = Some(bounds);
            block.last_line_height = px(20.0);
            block.selected_range = 0..0;
        });

        let mut event = MouseDownEvent {
            button: MouseButton::Left,
            position: link_position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        };

        // A plain click on the link moves the caret into the text for editing.
        cx.update(|window, app| {
            block.update(app, |block, cx| block.on_mouse_down(&event, window, cx));
        });
        block.read_with(cx, |block, _cx| {
            assert_ne!(block.selected_range, 0..0);
        });

        // Cmd/Ctrl+click follows the link instead: the caret is left untouched
        // and no drag-selection begins.
        block.update(cx, |block, _cx| block.selected_range = 0..0);
        event.modifiers = Modifiers::secondary_key();
        cx.update(|window, app| {
            block.update(app, |block, cx| block.on_mouse_down(&event, window, cx));
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, 0..0);
            assert!(!block.is_selecting);
        });
    }

    #[gpui::test]
    async fn link_hit_respects_center_alignment(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown("[link](https://example.com)"),
                ),
            );
            block.set_table_cell_mode(
                TableCellPosition { row: 0, column: 0 },
                crate::components::TableColumnAlignment::Center,
            );
            block
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(240.0), cx);
        let (miss_left, hit_center) = block.read_with(cx, |block, _cx| {
            let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(240.0), px(20.0)));
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let layout = &lines[0];
            let origin_x = super::aligned_line_left(layout, bounds, block.text_align());
            let start = layout
                .position_for_index(span.range.start, px(20.0))
                .expect("start position");
            let end = layout
                .position_for_index(span.range.end, px(20.0))
                .expect("end position");
            let miss_left = point(origin_x - px(12.0), px(10.0));
            let hit_center = point(origin_x + (start.x + end.x) / 2.0, px(10.0));
            (
                link_at_position(block, &lines, bounds, px(20.0), miss_left)
                    .map(|link| link.open_target.clone()),
                link_at_position(block, &lines, bounds, px(20.0), hit_center)
                    .map(|link| link.open_target.clone()),
            )
        });

        assert_eq!(miss_left, None);
        assert_eq!(hit_center, Some("https://example.com".to_string()));
    }

    #[gpui::test]
    async fn text_runs_apply_inline_html_color_and_background(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown(
                        "before <span style='color:blue;background-color:#ff0'>marked</span>",
                    ),
                ),
            )
        });

        block.read_with(cx, |block, _cx| {
            let display_text: SharedString = block.display_text().to_string().into();
            let base_run = TextRun {
                len: display_text.len(),
                font: font(".SystemUIFont"),
                color: Hsla::from(rgba(0xffffffff)),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = super::super::shaped::ChipShapedText::identity(&display_text);
            let runs = super::build_text_runs(
                block,
                &shaped,
                &base_run,
                px(1.0),
                Hsla::from(rgba(0x0066ccff)),
                Hsla::from(rgba(0xeeeeeeff)),
                Hsla::from(rgba(0x9999aaff)),
                Hsla::from(rgba(0x111111ff)),
                true,
                &SharedString::from(".SystemUIFont"),
            );
            let marked_run = runs.last().expect("styled text should create a final run");

            assert_eq!(block.display_text(), "before marked");
            assert_eq!(marked_run.len, "marked".len());
            assert_eq!(marked_run.color, Hsla::from(rgba(0x0000ffff)));
            assert_eq!(
                marked_run.background_color,
                Some(Hsla::from(rgba(0xffff00ff)))
            );
        });
    }

    #[gpui::test]
    async fn soft_wrapped_range_segments_stay_within_wrap_width(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let lines = shaped_lines(text, px(80.0), cx);
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "test text should soft-wrap"
        );

        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(80.0), px(120.0)));
        let segments = super::range_segment_bounds(
            &lines,
            bounds,
            px(20.0),
            text,
            0..text.len(),
            TextAlign::Left,
        );

        assert!(segments.len() > 1);
        for segment in segments {
            assert!(segment.left() >= bounds.left());
            assert!(segment.right() <= bounds.right() + px(0.5));
        }
    }

    #[gpui::test]
    async fn wrapped_link_hit_matches_only_visible_segments(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let label = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(
                    BlockKind::Paragraph,
                    InlineTextTree::from_markdown(&format!("[{label}](https://example.com)")),
                ),
            )
        });

        let display_text = block.read_with(cx, |block, _cx| block.display_text().to_string());
        let lines = shaped_lines(&display_text, px(80.0), cx);
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "link text should soft-wrap"
        );

        let (hit, miss_right) = block.read_with(cx, |block, _cx| {
            let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(80.0), px(120.0)));
            let span = block
                .inline_spans()
                .iter()
                .find(|span| span.link.is_some())
                .expect("link span should exist");
            let segments = super::range_segment_bounds(
                &lines,
                bounds,
                px(20.0),
                &display_text,
                span.range.clone(),
                block.text_align(),
            );
            assert!(segments.len() > 1);
            let second_segment = segments[1];
            let hit = point(
                (second_segment.left() + second_segment.right()) / 2.0,
                (second_segment.top() + second_segment.bottom()) / 2.0,
            );
            let miss_right = point(second_segment.right() + px(24.0), hit.y);
            (
                link_at_position(block, &lines, bounds, px(20.0), hit)
                    .map(|link| link.open_target.clone()),
                link_at_position(block, &lines, bounds, px(20.0), miss_right)
                    .map(|link| link.open_target.clone()),
            )
        });

        assert_eq!(hit, Some("https://example.com".to_string()));
        assert_eq!(miss_right, None);
    }

    // EXP-282 vendoring: pointer text selection + whole-band caret placement.
    // The blocks below are painted by hand — layout, bounds and line height
    // are the state a real paint would have left behind — and the bounds sit
    // 4px down from the shell origin, mirroring the block shell's `py(4)`.
    const EXP282_LINE_HEIGHT: gpui::Pixels = px(20.0);
    const EXP282_PAD: gpui::Pixels = px(4.0);

    fn exp282_block(
        text: &str,
        width: gpui::Pixels,
        cx: &mut VisualTestContext,
    ) -> gpui::Entity<Block> {
        let lines = shaped_lines(text, width, cx);
        let height = lines.iter().fold(px(0.0), |height, line| {
            height + wrapped_line_height(line, EXP282_LINE_HEIGHT)
        });
        let block = cx.new(|cx| {
            Block::with_record(
                cx,
                BlockRecord::new(BlockKind::Paragraph, InlineTextTree::plain(text)),
            )
        });
        block.update(cx, |block, _cx| {
            block.last_layout = Some(lines);
            block.last_bounds = Some(Bounds::new(point(px(0.0), EXP282_PAD), size(width, height)));
            block.last_line_height = EXP282_LINE_HEIGHT;
        });
        block
    }

    /// Pointer position at the caret offset `offset` of hard line `line`.
    fn exp282_point(
        block: &gpui::Entity<Block>,
        cx: &VisualTestContext,
        line: usize,
        offset_in_line: usize,
    ) -> gpui::Point<gpui::Pixels> {
        block.read_with(cx, |block, _cx| {
            let lines = block.last_layout.as_ref().expect("painted layout");
            let x = lines[line]
                .position_for_index(offset_in_line, EXP282_LINE_HEIGHT)
                .expect("offset position")
                .x;
            let top = super::wrapped_line_top(lines, EXP282_LINE_HEIGHT, line);
            point(x, EXP282_PAD + top + EXP282_LINE_HEIGHT / 2.0)
        })
    }

    fn exp282_mouse_down(position: gpui::Point<gpui::Pixels>, click_count: usize) -> MouseDownEvent {
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count,
            first_mouse: false,
        }
    }

    fn exp282_drag_to(position: gpui::Point<gpui::Pixels>) -> gpui::MouseMoveEvent {
        gpui::MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        }
    }

    #[gpui::test]
    async fn first_click_drag_selects_text_exp282(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = exp282_block("alpha beta gamma", px(320.0), cx);
        let start = exp282_point(&block, cx, 0, 0);
        let end = exp282_point(&block, cx, 0, 5);

        // The block is NOT focused yet: this is the click that enters it, the
        // one that used to leave `is_selecting` false and kill the drag.
        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_down(&exp282_mouse_down(start, 1), window, cx)
            });
        });
        block.read_with(cx, |block, _cx| {
            assert!(block.is_selecting);
            assert_eq!(block.selected_range, 0..0);
        });

        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_move(&exp282_drag_to(end), window, cx)
            });
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, 0..5);
        });
    }

    #[gpui::test]
    async fn double_click_selects_the_word_under_the_pointer_exp282(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = exp282_block("alpha beta gamma", px(320.0), cx);
        let inside_beta = exp282_point(&block, cx, 0, 8);

        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_down(&exp282_mouse_down(inside_beta, 2), window, cx)
            });
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, 6..10);
            assert!(!block.selection_reversed);
        });
    }

    #[gpui::test]
    async fn triple_click_selects_the_whole_hard_line_exp282(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let text = "alpha beta\nsecond line";
        let block = exp282_block(text, px(320.0), cx);
        let inside_second_line = exp282_point(&block, cx, 1, 3);

        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_down(&exp282_mouse_down(inside_second_line, 3), window, cx)
            });
        });
        let expected = text.find("second").expect("second line")..text.len();
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, expected);
        });

        // A fourth click deliberately stays on the line instead of cycling.
        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_down(&exp282_mouse_down(inside_second_line, 4), window, cx)
            });
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, expected);
        });
    }

    #[gpui::test]
    async fn double_click_drag_never_shrinks_inside_the_word_exp282(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = exp282_block("alpha beta gamma", px(320.0), cx);
        let inside_beta = exp282_point(&block, cx, 0, 8);
        let inside_alpha = exp282_point(&block, cx, 0, 2);

        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_down(&exp282_mouse_down(inside_beta, 2), window, cx);
                // Dragging back left past the word start extends leftwards but
                // keeps the double-clicked word whole.
                block.on_mouse_move(&exp282_drag_to(inside_alpha), window, cx);
            });
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, 2..10);
            assert!(block.selection_reversed);
        });

        // Dragging back INTO the word cannot shrink the selection below it.
        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_move(&exp282_drag_to(inside_beta), window, cx)
            });
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, 6..10);
        });

        // Releasing the button retires the extent, so the next plain click is
        // an ordinary caret move again.
        cx.update(|window, app| {
            block.update(app, |block, cx| {
                block.on_mouse_up(
                    &gpui::MouseUpEvent {
                        button: MouseButton::Left,
                        position: inside_beta,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    },
                    window,
                    cx,
                );
                block.on_mouse_down(&exp282_mouse_down(inside_alpha, 1), window, cx);
            });
        });
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.selected_range, 2..2);
        });
    }

    #[gpui::test]
    async fn clicks_outside_the_text_band_still_follow_x_exp282(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = exp282_block("alpha beta gamma", px(320.0), cx);
        let in_band = exp282_point(&block, cx, 0, 8);

        block.read_with(cx, |block, _cx| {
            let expected = block.index_for_mouse_position(in_band);
            assert_eq!(expected, 8);

            // Below the last text row (a tall block's empty space) — this used
            // to snap the caret to the block start.
            let below = point(in_band.x, px(120.0));
            assert_eq!(block.index_for_mouse_position(below), expected);

            // Inside the shell's top padding band, above the measured text.
            let above = point(in_band.x, px(0.0));
            assert_eq!(block.index_for_mouse_position(above), expected);
        });
    }

    #[gpui::test]
    async fn blank_line_band_click_places_the_caret_by_x_exp282(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        // EXP-271 blank lines load as a U+00A0 paragraph — a few pixels of
        // text in a 28px-tall shell, so nearly every click on that row lands
        // outside the measured band.
        let block = exp282_block("\u{00A0}", px(320.0), cx);

        block.read_with(cx, |block, _cx| {
            let len = block.visible_len();
            assert!(len > 0);
            assert_eq!(block.index_for_mouse_position(point(px(0.0), px(60.0))), 0);
            assert_eq!(
                block.index_for_mouse_position(point(px(300.0), px(60.0))),
                len
            );
        });
    }

    #[gpui::test]
    async fn wrapped_hard_line_top_accumulates_soft_wrap_height(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz\nnext";
        let lines = shaped_lines(text, px(80.0), cx);
        assert_eq!(lines.len(), 2);
        assert!(
            !lines[0].wrap_boundaries().is_empty(),
            "first hard line should soft-wrap"
        );

        let first_height = lines[0].size(px(20.0)).height;
        assert!(first_height > px(20.0));
        assert_eq!(super::wrapped_line_top(&lines, px(20.0), 1), first_height);
    }
}

// EXP-322 vendoring: the chip title is display-only — it is injected into the
// string the element shapes, never into the document.
#[cfg(test)]
mod chip_tests {
    use super::{build_text_runs, reference_at_position};
    use crate::components::block::shaped::ChipShapedText;
    use crate::components::{Block, BlockKind, BlockRecord, InlineTextTree};
    use crate::environment::MarkdownEditorEnvironment;
    use crate::host::{ReferenceDecorator, ReferenceKind, ReferenceSpan};
    use gpui::{
        AppContext, Bounds, Hsla, SharedString, TestAppContext, TextRun, VisualTestContext, font,
        point, px, rgba, size,
    };
    use std::sync::Arc;

    const TEXT: &str = "see #EXP-1 now";
    const TITLE: &str = "Fix login flow";

    struct TitleDecorator;

    impl ReferenceDecorator for TitleDecorator {
        fn scan(&self, text: &str) -> Vec<ReferenceSpan> {
            match text.find("#EXP-1") {
                Some(start) => vec![ReferenceSpan {
                    range: start..start + "#EXP-1".len(),
                    kind: ReferenceKind::IssueRef,
                    display_suffix: Some(SharedString::from(TITLE)),
                    icon: None,
                }],
                None => Vec::new(),
            }
        }
    }

    fn environment() -> MarkdownEditorEnvironment {
        MarkdownEditorEnvironment {
            reference_decorator: Some(Arc::new(TitleDecorator)),
            ..MarkdownEditorEnvironment::default()
        }
    }

    fn shaped_lines(
        text: &str,
        width: gpui::Pixels,
        cx: &mut VisualTestContext,
    ) -> Vec<gpui::WrappedLine> {
        cx.update(|window, _app| {
            window
                .text_system()
                .shape_text(
                    text.to_string().into(),
                    px(16.0),
                    &[TextRun {
                        len: text.len(),
                        font: font(".SystemUIFont"),
                        color: Hsla::from(rgba(0xffffffff)),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    Some(width),
                    None,
                )
                .expect("text should shape")
                .into_vec()
        })
    }

    fn chip_block(cx: &mut TestAppContext) -> gpui::Entity<Block> {
        cx.new(|cx| {
            Block::with_record_and_environment(
                cx,
                BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown(TEXT)),
                Arc::new(environment()),
            )
        })
    }

    /// Paint a decorated block in a real window so `request_layout` — the only
    /// caller of `ChipShapedText::build` — actually runs, then hand back the
    /// map it stored.
    fn painted_shaped(
        cx: &mut TestAppContext,
        record: BlockRecord,
        prepare: impl FnOnce(&mut Block),
    ) -> (String, ChipShapedText) {
        let (block, cx) = cx.add_window_view(|_window, cx| {
            let mut block = Block::with_record_and_environment(cx, record, Arc::new(environment()));
            prepare(&mut block);
            block
        });
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        block.read_with(cx, |block, _cx| {
            (
                block.display_text().to_string(),
                block
                    .last_shaped
                    .clone()
                    .expect("a painted block stores its shaped map"),
            )
        })
    }

    /// EXP-423: a decorator that also carries a status icon.
    struct IconTitleDecorator;

    impl ReferenceDecorator for IconTitleDecorator {
        fn scan(&self, text: &str) -> Vec<ReferenceSpan> {
            let mut spans = TitleDecorator.scan(text);
            for span in &mut spans {
                span.icon = Some(crate::host::ChipIcon {
                    svg_path: SharedString::from("icons/circle.svg"),
                    color: Hsla::from(rgba(0x22cc88ff)),
                });
            }
            spans
        }
    }

    /// EXP-423: painting an icon-carrying chip in a REAL window drives the
    /// whole path — gutter injection in `request_layout`, the gutter-segment
    /// icon box in `prepaint`, and `paint_svg` in `paint` (the asset miss is
    /// tolerated via `.ok()` headless).
    #[gpui::test]
    async fn an_icon_chip_injects_its_gutter_when_painted(cx: &mut TestAppContext) {
        let (block, cx) = cx.add_window_view(|_window, cx| {
            Block::with_record_and_environment(
                cx,
                BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown(TEXT)),
                Arc::new(MarkdownEditorEnvironment {
                    reference_decorator: Some(Arc::new(IconTitleDecorator)),
                    ..MarkdownEditorEnvironment::default()
                }),
            )
        });
        for _ in 0..2 {
            cx.update(|window, cx| {
                window.draw(cx).clear();
            });
            cx.run_until_parked();
        }
        block.read_with(cx, |block, _cx| {
            let shaped = block
                .last_shaped
                .clone()
                .expect("a painted block stores its shaped map");
            let gutter = shaped
                .icon_gutter_range(0)
                .expect("the icon chip injects a gutter");
            assert_eq!(
                &shaped.text()[gutter],
                crate::components::block::shaped::ICON_GUTTER
            );
            // Serialization stays the raw document.
            assert_eq!(shaped.document().as_ref(), TEXT);
        });
    }

    /// D1: a code BLOCK is literal source — and `build_code_text_runs` colours
    /// it from DOCUMENT offsets, so an injected title would shift every
    /// highlight after the first token.
    #[gpui::test]
    async fn a_chip_inside_a_code_block_is_never_title_injected(cx: &mut TestAppContext) {
        let (document, shaped) = painted_shaped(
            cx,
            BlockRecord::new(
                BlockKind::CodeBlock {
                    language: Some("ts".into()),
                },
                InlineTextTree::plain("// see #EXP-1 for context\n"),
            ),
            |_block| {},
        );
        assert!(document.contains("#EXP-1"));
        assert_eq!(shaped.text().as_ref(), document.as_str());
        assert!(!shaped.text().contains(TITLE));
        assert!(shaped.chip_range(0).is_none());
    }

    /// D1: same rule inside a code SPAN — code style already won over the
    /// pill's styling; it has to win over the injected text too.
    #[gpui::test]
    async fn a_chip_inside_inline_code_is_never_title_injected(cx: &mut TestAppContext) {
        let (document, shaped) = painted_shaped(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("see `#EXP-1` now"),
            ),
            |_block| {},
        );
        assert!(document.contains("#EXP-1"));
        assert_eq!(shaped.text().as_ref(), document.as_str());
        assert!(!shaped.text().contains(TITLE));
        assert!(shaped.chip_range(0).is_none());
    }

    /// ...and a chip in ordinary prose in the SAME block still gets its title,
    /// so the filter is a scalpel and not a switch.
    #[gpui::test]
    async fn a_chip_outside_the_code_span_still_gets_its_title(cx: &mut TestAppContext) {
        let (_, shaped) = painted_shaped(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`code` then #EXP-1"),
            ),
            |_block| {},
        );
        assert!(shaped.text().contains(TITLE));
        assert!(shaped.chip_range(0).is_some());
    }

    /// D2: SourceRaw mode exists to show the literal markdown bytes.
    #[gpui::test]
    async fn source_raw_mode_shows_the_literal_bytes(cx: &mut TestAppContext) {
        let (document, shaped) = painted_shaped(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown(TEXT)),
            |block| block.set_source_raw_mode(),
        );
        assert!(document.contains("#EXP-1"));
        assert_eq!(shaped.text().as_ref(), document.as_str());
        assert!(!shaped.text().contains(TITLE));
        assert!(shaped.chip_range(0).is_none());
    }

    /// D4: composing keeps the injection OFF (composition offsets must not
    /// move under the input method) but must NOT flatten the pill — the spans
    /// ride along so the chip keeps its quad, its weight and its click target.
    #[gpui::test]
    async fn an_ime_composition_keeps_the_chip_styled_without_injecting(cx: &mut TestAppContext) {
        let (document, shaped) = painted_shaped(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown(TEXT)),
            |block| block.marked_range = Some(0..3),
        );
        assert_eq!(shaped.text().as_ref(), document.as_str());
        assert!(!shaped.text().contains(TITLE));
        // The chip is still a chip: styled, hit-testable, just untitled.
        assert_eq!(shaped.spans().len(), 1);
        let chip = shaped.chip_range(0).expect("chip range while composing");
        assert_eq!(&document[chip], "#EXP-1");
    }

    #[gpui::test]
    async fn the_chip_title_never_enters_the_document_text(cx: &mut TestAppContext) {
        let block = chip_block(cx);
        block.read_with(cx, |block, _cx| {
            assert_eq!(block.display_text(), TEXT);
            let shaped = ChipShapedText::build(
                &SharedString::from(TEXT),
                TitleDecorator.scan(TEXT),
            );
            assert!(shaped.text().len() > TEXT.len());
            assert!(shaped.text().contains(TITLE));
        });
    }

    /// gpui panics while shaping when the run lengths do not sum to the text
    /// length — the single highest-risk invariant of the injection.
    #[gpui::test]
    async fn run_lengths_sum_to_the_shaped_text_length(cx: &mut TestAppContext) {
        let block = chip_block(cx);
        block.read_with(cx, |block, _cx| {
            let shaped =
                ChipShapedText::build(&SharedString::from(TEXT), TitleDecorator.scan(TEXT));
            let base_run = TextRun {
                len: shaped.text().len(),
                font: font(".SystemUIFont"),
                color: Hsla::from(rgba(0xffffffff)),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = build_text_runs(
                block,
                &shaped,
                &base_run,
                px(1.0),
                Hsla::from(rgba(0x0066ccff)),
                Hsla::from(rgba(0xeeeeeeff)),
                Hsla::from(rgba(0x9999aaff)),
                Hsla::from(rgba(0x111111ff)),
                true,
                &SharedString::from(".SystemUIFont"),
            );
            assert_eq!(
                runs.iter().map(|run| run.len).sum::<usize>(),
                shaped.text().len()
            );
        });
    }

    /// D5/D6: the pill paints its OWN foreground colour, and only the
    /// identifier half is monospace — the title rides the body font, like
    /// web's `::after`.
    #[gpui::test]
    async fn only_the_identifier_is_monospace_and_the_pill_owns_its_colour(
        cx: &mut TestAppContext,
    ) {
        let block = chip_block(cx);
        block.read_with(cx, |block, _cx| {
            let shaped =
                ChipShapedText::build(&SharedString::from(TEXT), TitleDecorator.scan(TEXT));
            let base_run = TextRun {
                len: shaped.text().len(),
                font: font(".SystemUIFont"),
                color: Hsla::from(rgba(0xffffffff)),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let link = Hsla::from(rgba(0x0066ccff));
            let reference = Hsla::from(rgba(0x00ff00ff));
            let token_color = Hsla::from(rgba(0x88aa88ff));
            let mono = SharedString::from("MonoTestFamily");
            let runs = build_text_runs(
                block,
                &shaped,
                &base_run,
                px(1.0),
                link,
                reference,
                token_color,
                Hsla::from(rgba(0x111111ff)),
                true,
                &mono,
            );

            let chip = shaped.chip_range(0).expect("chip range");
            let token = shaped.token_range(0).expect("token range");
            let mut offset = 0usize;
            let mut saw_token = false;
            let mut saw_title = false;
            for run in &runs {
                let range = offset..offset + run.len;
                offset = range.end;
                if range.start >= token.start && range.end <= token.end {
                    saw_token = true;
                    assert_eq!(run.font.family, mono, "identifier renders monospace");
                    // EXP-423 Linear look: the identifier is MUTED.
                    assert_eq!(run.color, token_color, "identifier uses the token colour");
                } else if range.start >= token.end && range.end <= chip.end {
                    saw_title = true;
                    assert_eq!(
                        run.font.family, base_run.font.family,
                        "the title stays in the body font"
                    );
                    assert_eq!(run.color, reference, "the title uses the pill foreground");
                }
            }
            assert!(saw_token && saw_title, "chip must split into token + title");
        });
    }

    /// Same split with an ICON-carrying span: the leading NBSP gutter makes
    /// token.start a distinct boundary (token.start != chip.start), and the
    /// identifier must STILL render mono + muted — the production case, since
    /// every resolved issue ref carries an icon.
    #[gpui::test]
    async fn the_identifier_stays_monospace_and_muted_behind_an_icon_gutter(
        cx: &mut TestAppContext,
    ) {
        let block = chip_block(cx);
        block.read_with(cx, |block, _cx| {
            let shaped =
                ChipShapedText::build(&SharedString::from(TEXT), IconTitleDecorator.scan(TEXT));
            let base_run = TextRun {
                len: shaped.text().len(),
                font: font(".SystemUIFont"),
                color: Hsla::from(rgba(0xffffffff)),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let reference = Hsla::from(rgba(0x00ff00ff));
            let token_color = Hsla::from(rgba(0x88aa88ff));
            let mono = SharedString::from("MonoTestFamily");
            let runs = build_text_runs(
                block,
                &shaped,
                &base_run,
                px(1.0),
                Hsla::from(rgba(0x0066ccff)),
                reference,
                token_color,
                Hsla::from(rgba(0x111111ff)),
                true,
                &mono,
            );

            let chip = shaped.chip_range(0).expect("chip range");
            let token = shaped.token_range(0).expect("token range");
            assert!(
                token.start > chip.start,
                "an icon span must inject a leading gutter"
            );
            let mut offset = 0usize;
            let mut saw_token = false;
            for run in &runs {
                let range = offset..offset + run.len;
                offset = range.end;
                if range.start >= token.start && range.end <= token.end {
                    saw_token = true;
                    assert_eq!(
                        run.font.family, mono,
                        "identifier renders monospace ({:?})",
                        range
                    );
                    assert_eq!(run.color, token_color, "identifier uses the token colour");
                }
            }
            assert!(saw_token, "a run must cover the identifier alone");
            assert_eq!(
                runs.iter().map(|run| run.len).sum::<usize>(),
                shaped.text().len()
            );
        });
    }

    /// Clicking the TITLE half of a chip must resolve the reference — and hand
    /// the host the bare token, not the rendered label.
    #[gpui::test]
    async fn clicking_the_chip_title_resolves_the_bare_token(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = chip_block(cx);
        let shaped = ChipShapedText::build(&SharedString::from(TEXT), TitleDecorator.scan(TEXT));
        let lines = shaped_lines(shaped.text().as_ref(), px(600.0), cx);
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(600.0), px(20.0)));

        let chip = shaped.chip_range(0).expect("chip range");
        // A point inside the TITLE, past the identifier.
        let title_x = {
            let layout = &lines[0];
            let token_end = layout
                .position_for_index(chip.start + "#EXP-1".len(), px(20.0))
                .expect("token end");
            let chip_end = layout.position_for_index(chip.end, px(20.0)).expect("chip end");
            (token_end.x + chip_end.x) / 2.0
        };

        block.update(cx, |block, _cx| {
            block.last_shaped = Some(shaped.clone());
            block.last_bounds = Some(bounds);
            block.last_line_height = px(20.0);
        });

        block.read_with(cx, |block, _cx| {
            let hit =
                reference_at_position(block, &lines, bounds, px(20.0), point(title_x, px(10.0)));
            assert_eq!(hit, Some((ReferenceKind::IssueRef, "#EXP-1".to_string())));
        });
    }

    /// ...and it places the caret after the identifier rather than somewhere
    /// that does not exist in the document.
    #[gpui::test]
    async fn clicking_the_chip_title_places_the_caret_after_the_token(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let block = chip_block(cx);
        let shaped = ChipShapedText::build(&SharedString::from(TEXT), TitleDecorator.scan(TEXT));
        let lines = shaped_lines(shaped.text().as_ref(), px(600.0), cx);
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(600.0), px(20.0)));
        let chip = shaped.chip_range(0).expect("chip range");
        let title_x = {
            let layout = &lines[0];
            let token_end = layout
                .position_for_index(chip.start + "#EXP-1".len(), px(20.0))
                .expect("token end");
            let chip_end = layout.position_for_index(chip.end, px(20.0)).expect("chip end");
            (token_end.x + chip_end.x) / 2.0
        };

        block.update(cx, |block, _cx| {
            block.last_layout = Some(lines);
            block.last_shaped = Some(shaped);
            block.last_bounds = Some(bounds);
            block.last_line_height = px(20.0);
        });

        block.read_with(cx, |block, _cx| {
            let offset = block.index_for_mouse_position(point(title_x, px(10.0)));
            assert_eq!(offset, TEXT.find("#EXP-1").unwrap() + "#EXP-1".len());
        });
    }
}
