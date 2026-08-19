//! EXP-322 vendoring: the document ↔ shaped-text offset map that lets a
//! resolved `#IDENT` render as `#IDENT <issue title>` while editing.
//!
//! Web does this with a CSS `::after` on a ProseMirror decoration — generated
//! content that is never part of the document. gpui has no such thing: the
//! element shapes and lays out the block's own text, so the title has to be
//! injected into the string handed to `shape_text` and every document offset
//! mapped across that injection.
//!
//! Two properties keep this affordable and correct:
//!
//! - Every injection is CHIP-LOCAL and never a replacement: the leading icon
//!   gutter (EXP-423) sits immediately before the token and the title suffix
//!   immediately after it, so offsets inside the token stay linear and the
//!   caret can still sit between `#EXP` and `-238`. Both injections BELONG to
//!   the token's document range (EXP-547): a caret at the token start renders
//!   left of the whole pill and a caret at the token end renders right of it
//!   — never between the identifier and its title.
//! - Injected pieces contain no `\n`, so the document and the shaped text
//!   have the SAME hard-line structure. Every doc→pixel path funnels through
//!   `hard_line_ranges` + `line_index_for_offset`, which therefore need no
//!   changes — they just get fed shaped text and shaped offsets.

use gpui::SharedString;

use crate::host::ReferenceSpan;

/// Non-breaking space between the identifier and its title, so the pair never
/// breaks right after the identifier (web pins `white-space: nowrap` on the
/// identifier and leaves the title `normal`, which is the same effect).
const SEPARATOR: &str = "\u{00a0}";

/// EXP-423: the icon gutter injected before an icon-carrying chip's token —
/// blank glyphs that reserve layout space for the status icon painted over
/// them. NBSP on purpose: `char::is_whitespace('\u{00a0}') == false`, so
/// neither `inline_word_chunks` nor gpui's line wrapper breaks inside the
/// gutter (the title `SEPARATOR` above relies on exactly this; U+2007 would
/// be Rust-whitespace and wrap). The `#` stays VISIBLE after the gutter —
/// the native apps hide it under the icon, but the desktop editor keeps it
/// for the edit affordance and the offset-map invariants (EXP-423 ruling 3;
/// documented divergence). Four glyphs (EXP-469): three left the painted
/// icon wider than its reserved space, shoving it against both the pill's
/// edge and the `#`.
pub(crate) const ICON_GUTTER: &str = "\u{00a0}\u{00a0}\u{00a0}\u{00a0}";

/// EXP-469: one blank NBSP injected on EACH side of an icon-carrying chip,
/// OUTSIDE its chip range — the pill quad overdraws `reference_pad_x` beyond
/// its glyphs, which used to swallow the document's own space and leave the
/// pill glued to the surrounding text. The margin stays out of `chip_range`,
/// so it neither fills with pill paint nor hit-tests as the chip.
pub(crate) const CHIP_MARGIN: &str = "\u{00a0}";

/// EXP-547: which side of a document offset an injection sits on — i.e.
/// where a caret AT that offset renders relative to the injected glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertSide {
    /// Injected at a token START (icon gutter + leading margin): a caret at
    /// the offset renders BEFORE the injection, left of the pill.
    AfterCaret,
    /// Injected at a token END (title suffix + trailing margin): a caret at
    /// the offset renders AFTER the injection, right of the pill.
    BeforeCaret,
}

/// The text actually shaped for a block, plus the map back to document offsets.
#[derive(Clone, Debug)]
pub(crate) struct ChipShapedText {
    /// The block's own text, kept so a stored map can be checked against the
    /// current document before it is trusted.
    doc: SharedString,
    text: SharedString,
    spans: Vec<ReferenceSpan>,
    /// `(document byte offset of the insertion point, inserted byte length,
    /// side)`, ascending and non-overlapping. Two insertions may share an
    /// offset only as `BeforeCaret` (a chip's tail) followed by `AfterCaret`
    /// (the next chip's head).
    insertions: Vec<(usize, usize, InsertSide)>,
    /// Shaped range of each chip (icon gutter + token + injected suffix),
    /// parallel to `spans`.
    chip_ranges: Vec<std::ops::Range<usize>>,
    /// Shaped range of each chip's TOKEN alone (no gutter, no suffix),
    /// parallel to `spans`. Recorded at build time: `to_shaped_range` cannot
    /// produce it, because it maps the token start to BEFORE the gutter
    /// insertion (deliberately — that is the caret semantic).
    token_ranges: Vec<std::ops::Range<usize>>,
}

impl ChipShapedText {
    /// No decorator, or nothing resolved: the shaped text IS the document.
    pub(crate) fn identity(text: &SharedString) -> Self {
        Self {
            doc: text.clone(),
            text: text.clone(),
            spans: Vec::new(),
            insertions: Vec::new(),
            chip_ranges: Vec::new(),
            token_ranges: Vec::new(),
        }
    }

    /// Spans WITHOUT the injection: the shaped text stays the document byte for
    /// byte, but the chips still style and hit-test. This is what an IME
    /// composition renders through — injecting while composing would move the
    /// marked range under the input method (icons are skipped there too, same
    /// as titles) — and what a decorated block with nothing to inject
    /// collapses to.
    pub(crate) fn identity_with_spans(text: &SharedString, mut spans: Vec<ReferenceSpan>) -> Self {
        spans.sort_by_key(|span| span.range.start);
        let mut this = Self::identity(text);
        this.chip_ranges = spans
            .iter()
            .map(|span| {
                let start = span.range.start.min(text.len());
                let end = span.range.end.min(text.len());
                if end <= start || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                    0..0
                } else {
                    start..end
                }
            })
            .collect();
        this.token_ranges = this.chip_ranges.clone();
        this.spans = spans;
        this
    }

    pub(crate) fn build(doc: &SharedString, mut spans: Vec<ReferenceSpan>) -> Self {
        spans.sort_by_key(|span| span.range.start);
        let has_injection = spans.iter().any(|span| {
            span.icon.is_some()
                || span.display_suffix.as_ref().is_some_and(|s| !s.is_empty())
        });
        if !has_injection {
            return Self::identity_with_spans(doc, spans);
        }

        let source = doc.as_ref();
        let mut text = String::with_capacity(source.len() + spans.len() * 24);
        let mut insertions = Vec::new();
        let mut chip_ranges = Vec::with_capacity(spans.len());
        let mut token_ranges = Vec::with_capacity(spans.len());
        let mut last = 0usize;
        for span in &spans {
            let start = span.range.start.min(source.len());
            let end = span.range.end.min(source.len());
            if start < last || end <= start || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
                // Overlapping or unusable span: render it plain rather than
                // corrupt the map.
                chip_ranges.push(0..0);
                token_ranges.push(0..0);
                continue;
            }
            text.push_str(&source[last..start]);
            let mut chip_start = text.len();
            // EXP-423: the leading icon gutter — blank NBSP glyphs the paint
            // pass draws the status icon over. Inserted AT the token start
            // on the `AfterCaret` side, so a caret at the token start renders
            // before the gutter (left of the pill), and `to_doc` snaps clicks
            // inside the gutter to the token start.
            // EXP-469: a `CHIP_MARGIN` rides the same insertion but stays
            // BEFORE `chip_start`, so the pill quad never paints over it.
            if span.icon.is_some() {
                text.push_str(CHIP_MARGIN);
                chip_start = text.len();
                text.push_str(ICON_GUTTER);
                insertions.push((
                    start,
                    CHIP_MARGIN.len() + ICON_GUTTER.len(),
                    InsertSide::AfterCaret,
                ));
            }
            let token_start = text.len();
            text.push_str(&source[start..end]);
            let token_end = text.len();
            let mut inserted = 0usize;
            if let Some(suffix) = span.display_suffix.as_ref().filter(|s| !s.is_empty()) {
                // Newlines are flattened, not asserted away: a `\n` here would
                // desynchronize the hard-line structure the whole map relies
                // on, and a title is host data we do not get to trust.
                let piece = format!("{SEPARATOR}{}", suffix.replace('\n', " "));
                text.push_str(&piece);
                inserted = piece.len();
            }
            // EXP-469: the trailing margin — outside the chip range, merged
            // into the same insertion point as the suffix.
            let mut trailing_margin = 0usize;
            if span.icon.is_some() {
                text.push_str(CHIP_MARGIN);
                trailing_margin = CHIP_MARGIN.len();
            }
            // EXP-547: the tail rides the `BeforeCaret` side — a caret at
            // the token end renders AFTER the title and margin, right of the
            // pill, instead of wedged between the identifier and its title.
            if inserted + trailing_margin > 0 {
                insertions.push((end, inserted + trailing_margin, InsertSide::BeforeCaret));
            }
            chip_ranges.push(chip_start..token_end + inserted);
            token_ranges.push(token_start..token_end);
            last = end;
        }
        text.push_str(&source[last.min(source.len())..]);

        Self {
            doc: doc.clone(),
            text: SharedString::from(text),
            spans,
            insertions,
            chip_ranges,
            token_ranges,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_identity(&self) -> bool {
        self.insertions.is_empty()
    }

    pub(crate) fn text(&self) -> &SharedString {
        &self.text
    }

    /// The document text this map was built from.
    pub(crate) fn document(&self) -> &SharedString {
        &self.doc
    }

    pub(crate) fn spans(&self) -> &[ReferenceSpan] {
        &self.spans
    }

    /// Shaped range covering a chip's icon gutter, token AND title, so the
    /// pill quad and hit-testing both treat the chip as one unit.
    pub(crate) fn chip_range(&self, index: usize) -> Option<std::ops::Range<usize>> {
        self.chip_ranges
            .get(index)
            .filter(|range| !range.is_empty())
            .cloned()
    }

    /// Shaped range of a chip's TOKEN only — the identifier, WITHOUT the
    /// leading icon gutter or the injected title. Web renders
    /// `.issue-ref-pill`'s monospace on the identifier alone and leaves the
    /// `::after` title in the body font, so the halves of a chip need
    /// separate ranges.
    pub(crate) fn token_range(&self, index: usize) -> Option<std::ops::Range<usize>> {
        self.token_ranges
            .get(index)
            .filter(|range| !range.is_empty())
            .cloned()
    }

    /// EXP-423: the shaped range of a chip's leading icon gutter — the blank
    /// NBSP glyphs the status icon paints over. `None` when this chip had no
    /// gutter injected (no icon, or the identity/IME path).
    pub(crate) fn icon_gutter_range(&self, index: usize) -> Option<std::ops::Range<usize>> {
        let chip = self.chip_ranges.get(index).filter(|r| !r.is_empty())?;
        let token = self.token_ranges.get(index).filter(|r| !r.is_empty())?;
        (chip.start < token.start).then(|| chip.start..token.start)
    }

    /// Document → shaped. Every injection belongs to its token's document
    /// range: a head insertion (gutter) at the token start is counted only
    /// for offsets strictly PAST it, so a caret at the token start renders
    /// left of the pill; a tail insertion (title + margin) at the token end
    /// is counted for the end offset itself, so a caret at the token end
    /// renders right of the pill (EXP-547 — it used to sit between the
    /// identifier and its title, visually INSIDE the chip). Typing at the
    /// token end still extends the token document-wise (a trailing word
    /// char un-resolves the reference and the chip drops, a space keeps it).
    pub(crate) fn to_shaped(&self, doc: usize) -> usize {
        let doc = doc.min(self.doc_len());
        let mut delta = 0usize;
        for (at, len, side) in &self.insertions {
            if *at > doc {
                break;
            }
            if *at < doc || *side == InsertSide::BeforeCaret {
                delta += len;
            }
        }
        (doc + delta).min(self.text.len())
    }

    pub(crate) fn to_shaped_range(&self, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let start = self.to_shaped(range.start);
        let end = self.to_shaped(range.end).max(start);
        start..end
    }

    /// Shaped → document. Anything inside an injected title snaps to the token
    /// end, so clicking the title puts the caret right after the chip.
    pub(crate) fn to_doc(&self, shaped: usize) -> usize {
        let shaped = shaped.min(self.text.len());
        let mut delta = 0usize;
        for (at, len, _) in &self.insertions {
            let insert_start = at + delta;
            if shaped <= insert_start {
                return shaped - delta;
            }
            if shaped <= insert_start + len {
                return *at;
            }
            delta += len;
        }
        shaped.saturating_sub(delta).min(self.doc_len())
    }

    fn doc_len(&self) -> usize {
        self.doc.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{ChipIcon, ReferenceKind};

    fn span(range: std::ops::Range<usize>, suffix: Option<&str>) -> ReferenceSpan {
        ReferenceSpan {
            range,
            kind: ReferenceKind::IssueRef,
            display_suffix: suffix.map(SharedString::from),
            icon: None,
        }
    }

    /// EXP-423: an icon-carrying span — gets the leading NBSP gutter.
    fn icon_span(range: std::ops::Range<usize>, suffix: Option<&str>) -> ReferenceSpan {
        ReferenceSpan {
            icon: Some(ChipIcon {
                svg_path: SharedString::from("icons/circle.svg"),
                color: gpui::Hsla::default(),
            }),
            ..span(range, suffix)
        }
    }

    #[test]
    fn identity_map_is_the_document_text() {
        let doc = SharedString::from("plain words");
        let shaped = ChipShapedText::identity(&doc);
        assert!(shaped.is_identity());
        assert_eq!(shaped.text().as_ref(), "plain words");
        for i in 0..=doc.len() {
            assert_eq!(shaped.to_shaped(i), i);
            assert_eq!(shaped.to_doc(i), i);
        }
    }

    #[test]
    fn a_span_without_a_suffix_leaves_the_text_alone() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, None)]);
        assert!(shaped.is_identity());
        assert_eq!(shaped.text().as_ref(), "see #EXP-1 now");
        assert_eq!(shaped.chip_range(0), Some(4..10));
    }

    #[test]
    fn the_suffix_is_injected_after_the_token_and_never_inside_it() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        assert_eq!(shaped.text().as_ref(), "see #EXP-1\u{00a0}Title now");
        assert!(!shaped.is_identity());
    }

    #[test]
    fn a_caret_at_the_token_end_maps_after_the_suffix() {
        // EXP-547: the caret sits BEHIND the chip, never between the
        // identifier and its title.
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        let injected = "\u{00a0}Title".len();
        assert_eq!(shaped.to_shaped(9), 9);
        assert_eq!(shaped.to_shaped(10), 10 + injected);
        assert_eq!(shaped.to_shaped(11), 11 + injected);
        // The chip range ends exactly where the caret lands.
        assert_eq!(shaped.chip_range(0).expect("chip").end, shaped.to_shaped(10));
    }

    #[test]
    fn a_selection_over_the_token_covers_the_whole_chip() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        assert_eq!(shaped.to_shaped_range(4..10), shaped.chip_range(0).expect("chip"));
    }

    #[test]
    fn offsets_inside_the_suffix_snap_back_to_the_token_end() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        let injected = "\u{00a0}Title".len();
        for shaped_offset in 10..=10 + injected {
            assert_eq!(shaped.to_doc(shaped_offset), 10, "at {shaped_offset}");
        }
        assert_eq!(shaped.to_doc(10 + injected + 1), 11);
    }

    #[test]
    fn round_trip_is_lossless_for_every_document_offset() {
        let doc = SharedString::from("a #EXP-1 b #EXP-2 c");
        let shaped = ChipShapedText::build(
            &doc,
            vec![span(2..8, Some("First")), span(11..17, Some("Second"))],
        );
        let mut previous = 0usize;
        for i in 0..=doc.len() {
            let mapped = shaped.to_shaped(i);
            assert!(mapped >= previous, "not monotone at {i}");
            previous = mapped;
            assert_eq!(shaped.to_doc(mapped), i, "round trip at {i}");
        }
    }

    #[test]
    fn multiple_chips_on_one_line_accumulate_deltas() {
        let doc = SharedString::from("a #EXP-1 b #EXP-2 c");
        let shaped = ChipShapedText::build(
            &doc,
            vec![span(2..8, Some("First")), span(11..17, Some("Second"))],
        );
        let total = "\u{00a0}First".len() + "\u{00a0}Second".len();
        assert_eq!(shaped.to_shaped(doc.len()), doc.len() + total);
    }

    #[test]
    fn chip_range_covers_token_plus_suffix() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        let range = shaped.chip_range(0).expect("chip range");
        assert_eq!(&shaped.text()[range.clone()], "#EXP-1\u{00a0}Title");
    }

    #[test]
    fn a_multibyte_title_keeps_char_boundaries() {
        let doc = SharedString::from("see #EXP-1");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("größer — ünïcode"))]);
        assert!(shaped.text().is_char_boundary(shaped.to_shaped(10)));
        for i in 0..=doc.len() {
            assert_eq!(shaped.to_doc(shaped.to_shaped(i)), i);
        }
    }

    #[test]
    fn a_multibyte_document_maps_correctly() {
        let doc = SharedString::from("ünïcode #EXP-1 tail");
        let token = doc.find("#EXP-1").expect("token");
        let shaped = ChipShapedText::build(&doc, vec![span(token..token + 6, Some("Title"))]);
        for i in 0..=doc.len() {
            if !doc.is_char_boundary(i) {
                continue;
            }
            assert_eq!(shaped.to_doc(shaped.to_shaped(i)), i, "at {i}");
        }
    }

    #[test]
    fn token_range_stops_before_the_injected_title() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        let token = shaped.token_range(0).expect("token range");
        assert_eq!(&shaped.text()[token.clone()], "#EXP-1");
        let chip = shaped.chip_range(0).expect("chip range");
        assert_eq!(token.start, chip.start);
        assert!(token.end < chip.end);
    }

    #[test]
    fn identity_with_spans_keeps_the_document_text_but_still_chips() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::identity_with_spans(&doc, vec![span(4..10, Some("Title"))]);
        assert!(shaped.is_identity());
        assert_eq!(shaped.text().as_ref(), doc.as_ref());
        assert_eq!(shaped.chip_range(0), Some(4..10));
        assert_eq!(shaped.token_range(0), Some(4..10));
        for i in 0..=doc.len() {
            assert_eq!(shaped.to_shaped(i), i);
            assert_eq!(shaped.to_doc(i), i);
        }
    }

    #[test]
    fn a_newline_in_a_suffix_is_flattened() {
        let doc = SharedString::from("#EXP-1");
        let shaped = ChipShapedText::build(&doc, vec![span(0..6, Some("two\nlines"))]);
        assert!(!shaped.text().contains('\n'));
    }

    // ── EXP-423: the leading icon gutter ────────────────────────────────

    #[test]
    fn an_icon_span_injects_the_gutter_before_the_token() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![icon_span(4..10, Some("Title"))]);
        assert_eq!(
            shaped.text().as_ref(),
            &format!("see {CHIP_MARGIN}{ICON_GUTTER}#EXP-1\u{00a0}Title{CHIP_MARGIN} now")
        );
        // The document is untouched — serialization never sees the gutter.
        assert_eq!(shaped.document().as_ref(), doc.as_ref());
    }

    #[test]
    fn a_caret_at_the_token_start_maps_before_the_gutter() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![icon_span(4..10, Some("Title"))]);
        // The head insertion sits AFTER the caret: the caret renders LEFT of
        // the pill (and its margin), not between gutter and `#`.
        assert_eq!(shaped.to_shaped(4), 4);
        // One byte into the token is past the insertion → shifted.
        assert_eq!(shaped.to_shaped(5), 5 + CHIP_MARGIN.len() + ICON_GUTTER.len());
    }

    #[test]
    fn a_caret_at_the_token_end_maps_after_the_trailing_margin() {
        // EXP-547: with a gutter AND a title, the caret at the token end
        // lands past the title and the trailing margin — right of the pill,
        // symmetric to the token-start caret sitting left of the leading one.
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![icon_span(4..10, Some("Title"))]);
        let head = CHIP_MARGIN.len() + ICON_GUTTER.len();
        let tail = "\u{00a0}Title".len() + CHIP_MARGIN.len();
        assert_eq!(shaped.to_shaped(10), 10 + head + tail);
        assert_eq!(&shaped.text()[shaped.to_shaped(10)..], " now");
        // Whole-token selection = the pill plus both margins.
        assert_eq!(shaped.to_shaped_range(4..10), 4..10 + head + tail);
    }

    #[test]
    fn adjacent_chips_keep_tail_before_and_head_after_the_shared_offset() {
        // A chip's tail and the next chip's head can share one document
        // offset; the caret there renders between the two pills.
        let doc = SharedString::from("#EXP-1#EXP-2");
        let shaped = ChipShapedText::build(
            &doc,
            vec![icon_span(0..6, Some("A")), icon_span(6..12, Some("B"))],
        );
        let head = CHIP_MARGIN.len() + ICON_GUTTER.len();
        let tail_a = "\u{00a0}A".len() + CHIP_MARGIN.len();
        assert_eq!(shaped.to_shaped(6), 6 + head + tail_a);
        assert_eq!(shaped.to_doc(shaped.to_shaped(6)), 6);
        assert_eq!(shaped.chip_range(0).expect("a").end + CHIP_MARGIN.len(), shaped.to_shaped(6));
        // The chip range starts at B's gutter, one leading margin later.
        assert_eq!(shaped.chip_range(1).expect("b").start - CHIP_MARGIN.len(), shaped.to_shaped(6));
    }

    #[test]
    fn clicks_inside_the_gutter_snap_to_the_token_start() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![icon_span(4..10, Some("Title"))]);
        for offset in 5..=4 + CHIP_MARGIN.len() + ICON_GUTTER.len() {
            assert_eq!(shaped.to_doc(offset), 4, "at shaped {offset}");
        }
    }

    #[test]
    fn token_range_excludes_the_gutter_and_chip_range_includes_it() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![icon_span(4..10, Some("Title"))]);
        let token = shaped.token_range(0).expect("token range");
        assert_eq!(&shaped.text()[token.clone()], "#EXP-1");
        // EXP-469: the chip range carries gutter + token + title but NEITHER
        // margin — the pill quad and hit-testing stop before them.
        let chip = shaped.chip_range(0).expect("chip range");
        assert_eq!(
            &shaped.text()[chip.clone()],
            &format!("{ICON_GUTTER}#EXP-1\u{00a0}Title")
        );
        assert_eq!(chip.start + ICON_GUTTER.len(), token.start);
        let gutter = shaped.icon_gutter_range(0).expect("gutter range");
        assert_eq!(gutter, chip.start..token.start);
    }

    #[test]
    fn gutter_and_suffix_insertions_keep_the_map_invertible() {
        let doc = SharedString::from("a #EXP-1 b #EXP-2 c");
        let shaped = ChipShapedText::build(
            &doc,
            vec![icon_span(2..8, Some("First")), icon_span(11..17, Some("Second"))],
        );
        let mut previous = 0usize;
        for i in 0..=doc.len() {
            let mapped = shaped.to_shaped(i);
            assert!(mapped >= previous, "not monotone at {i}");
            previous = mapped;
            assert_eq!(shaped.to_doc(mapped), i, "round trip at {i}");
        }
    }

    #[test]
    fn an_icon_span_without_a_suffix_still_gets_its_gutter() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![icon_span(4..10, None)]);
        assert_eq!(
            shaped.text().as_ref(),
            &format!("see {CHIP_MARGIN}{ICON_GUTTER}#EXP-1{CHIP_MARGIN} now")
        );
        assert!(shaped.icon_gutter_range(0).is_some());
    }

    #[test]
    fn the_identity_path_never_injects_a_gutter() {
        // IME composition path: icons are skipped, same as titles.
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::identity_with_spans(&doc, vec![icon_span(4..10, Some("T"))]);
        assert!(shaped.is_identity());
        assert_eq!(shaped.icon_gutter_range(0), None);
        assert_eq!(shaped.token_range(0), Some(4..10));
    }
}
