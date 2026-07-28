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
//! - The injection is a pure SUFFIX at a token's end, never a replacement, so
//!   offsets inside the token stay linear and the caret can still sit between
//!   `#EXP` and `-238`.
//! - Injected titles contain no `\n`, so the document and the shaped text have
//!   the SAME hard-line structure. Every doc→pixel path funnels through
//!   `hard_line_ranges` + `line_index_for_offset`, which therefore need no
//!   changes — they just get fed shaped text and shaped offsets.

use gpui::SharedString;

use crate::host::ReferenceSpan;

/// Non-breaking space between the identifier and its title, so the pair never
/// breaks right after the identifier (web pins `white-space: nowrap` on the
/// identifier and leaves the title `normal`, which is the same effect).
const SEPARATOR: &str = "\u{00a0}";

/// The text actually shaped for a block, plus the map back to document offsets.
#[derive(Clone, Debug)]
pub(crate) struct ChipShapedText {
    /// The block's own text, kept so a stored map can be checked against the
    /// current document before it is trusted.
    doc: SharedString,
    text: SharedString,
    spans: Vec<ReferenceSpan>,
    /// `(document byte offset of the insertion point, inserted byte length)`,
    /// ascending and non-overlapping.
    insertions: Vec<(usize, usize)>,
    /// Shaped range of each chip (token + its injected suffix), parallel to
    /// `spans`.
    chip_ranges: Vec<std::ops::Range<usize>>,
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
        }
    }

    /// Spans WITHOUT the injection: the shaped text stays the document byte for
    /// byte, but the chips still style and hit-test. This is what an IME
    /// composition renders through — injecting while composing would move the
    /// marked range under the input method — and what a decorated block with
    /// nothing to inject collapses to.
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
        this.spans = spans;
        this
    }

    pub(crate) fn build(doc: &SharedString, mut spans: Vec<ReferenceSpan>) -> Self {
        spans.sort_by_key(|span| span.range.start);
        let has_suffix = spans
            .iter()
            .any(|span| span.display_suffix.as_ref().is_some_and(|s| !s.is_empty()));
        if !has_suffix {
            return Self::identity_with_spans(doc, spans);
        }

        let source = doc.as_ref();
        let mut text = String::with_capacity(source.len() + spans.len() * 24);
        let mut insertions = Vec::new();
        let mut chip_ranges = Vec::with_capacity(spans.len());
        let mut last = 0usize;
        for span in &spans {
            let start = span.range.start.min(source.len());
            let end = span.range.end.min(source.len());
            if start < last || end <= start || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
                // Overlapping or unusable span: render it plain rather than
                // corrupt the map.
                chip_ranges.push(0..0);
                continue;
            }
            text.push_str(&source[last..start]);
            let chip_start = text.len();
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
                insertions.push((end, inserted));
            }
            chip_ranges.push(chip_start..token_end + inserted);
            last = end;
        }
        text.push_str(&source[last.min(source.len())..]);

        Self {
            doc: doc.clone(),
            text: SharedString::from(text),
            spans,
            insertions,
            chip_ranges,
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

    /// Shaped range covering a chip's token AND its title, so the pill quad and
    /// hit-testing both treat the chip as one unit.
    pub(crate) fn chip_range(&self, index: usize) -> Option<std::ops::Range<usize>> {
        self.chip_ranges
            .get(index)
            .filter(|range| !range.is_empty())
            .cloned()
    }

    /// Shaped range of a chip's TOKEN only — the identifier, WITHOUT the
    /// injected title. Web renders `.issue-ref-pill`'s monospace on the
    /// identifier alone and leaves the `::after` title in the body font, so the
    /// two halves of a chip need separate ranges.
    pub(crate) fn token_range(&self, index: usize) -> Option<std::ops::Range<usize>> {
        let span = self.spans.get(index)?;
        let range = self.to_shaped_range(span.range.clone());
        (!range.is_empty()).then_some(range)
    }

    /// Document → shaped. Strictly `<` at an insertion point, so a caret at a
    /// token's end renders BEFORE the title (ProseMirror parity: the caret sits
    /// between the text node and its `::after`), and typing there extends the
    /// token.
    pub(crate) fn to_shaped(&self, doc: usize) -> usize {
        let doc = doc.min(self.doc_len());
        let mut delta = 0usize;
        for (at, len) in &self.insertions {
            if *at < doc {
                delta += len;
            } else {
                break;
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
    /// end, so clicking the title puts the caret right after the identifier.
    pub(crate) fn to_doc(&self, shaped: usize) -> usize {
        let shaped = shaped.min(self.text.len());
        let mut delta = 0usize;
        for (at, len) in &self.insertions {
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
    use crate::host::ReferenceKind;

    fn span(range: std::ops::Range<usize>, suffix: Option<&str>) -> ReferenceSpan {
        ReferenceSpan {
            range,
            kind: ReferenceKind::IssueRef,
            display_suffix: suffix.map(SharedString::from),
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
    fn a_caret_at_the_token_end_maps_before_the_suffix() {
        let doc = SharedString::from("see #EXP-1 now");
        let shaped = ChipShapedText::build(&doc, vec![span(4..10, Some("Title"))]);
        assert_eq!(shaped.to_shaped(10), 10);
        assert_eq!(shaped.to_shaped(11), 11 + "\u{00a0}Title".len());
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
}
