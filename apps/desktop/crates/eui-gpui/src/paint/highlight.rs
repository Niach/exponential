//! #EDE-95 (`fixtures/text-highlight.json`): genUI's search marks on the
//! gpui painter. The rule is the core's ([`eui_core::text_highlight`]); this
//! paints it: `$color.match` behind each match and `$color.matchCurrent`
//! behind the current one, as a text-run background (the text keeps its
//! colour, the layout never moves). A Markdown searches each block of the
//! markdown-render.json normal form apart ([`markdown_highlight_texts`]: a
//! paragraph, heading, list item or quote = its runs joined; each table
//! cell, the header first; never a code block, image, attachment or rule),
//! a link by its label, inline code by its text, a host chip as ONE
//! [`HIGHLIGHT_BARRIER`] (chips stay whole).

use std::collections::HashMap;
use std::ops::Range;

use eui_core::text_highlight::{highlight_local_current, highlight_ranges, HighlightSpec, HIGHLIGHT_BARRIER};
use eui_core::theme::{Mode, ResolvedTheme};
use gpui::{HighlightStyle, Hsla, TextRun};

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

/// `marks` as gpui highlights over a string's inherited style.
pub fn highlight_styles(marks: &[Mark]) -> Vec<(Range<usize>, HighlightStyle)> {
    marks.iter().map(|(r, c)| (r.clone(), HighlightStyle { background_color: Some(*c), ..HighlightStyle::default() })).collect()
}

/// Runs with each mark's range on its fill (runs split at the marks' ends).
pub fn mark_runs(runs: Vec<TextRun>, marks: &[Mark]) -> Vec<TextRun> {
    if marks.is_empty() {
        return runs;
    }
    let mut out = Vec::with_capacity(runs.len() + 2 * marks.len());
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        let mut cuts = vec![start, end];
        for (r, _) in marks {
            cuts.push(r.start.clamp(start, end));
            cuts.push(r.end.clamp(start, end));
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let fill = marks.iter().find(|(r, _)| r.start <= w[0] && w[1] <= r.end).map(|(_, c)| *c);
            out.push(TextRun { len: w[1] - w[0], background_color: fill.or(run.background_color), ..run.clone() });
        }
    }
    out
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
