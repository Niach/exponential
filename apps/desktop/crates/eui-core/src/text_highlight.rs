//! #EDE-95 (`fixtures/text-highlight.json`): search-match marks on genUI
//! text (Text, Markdown, MessageBubble, Row `highlight`), the rule the TS
//! reference packages/exponential-ui/src/highlight.ts implements. A match is
//! the query found LITERALLY in the text after folding both per Unicode
//! scalar: canonical decomposition (NFD), lower case, combining marks
//! dropped (U+0300–036F, 1AB0–1AFF, 1DC0–1DFF, 20D0–20FF, FE20–FE2F).
//! Matches run left to right and never overlap; a match ends after the
//! scalars its last folded character came from plus the combining marks
//! right after them. A query that folds to nothing matches nothing. Ranges
//! here are BYTE ranges into the Rust `str` (the fixture's are UTF-16).

use std::ops::Range;

use serde_json::Value;
use unicode_normalization::char::decompose_canonical;

/// A chip (or any piece that never matches) inside a searched text: no
/// query contains it, so no match crosses it.
pub const HIGHLIGHT_BARRIER: char = '\u{FFFC}';

/// The `highlight` prop (catalog defs `highlight`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpec {
    pub query: String,
    /// The 0-based index of the current match across the host's numbering.
    pub current: Option<i64>,
    /// Text whose matches number before this node's (Row's subtitle gets the title).
    pub before: Option<String>,
}

fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

/// One scalar folded (NFD, lower case, marks dropped) into `out`.
fn fold_scalar(ch: char, out: &mut Vec<char>) {
    decompose_canonical(ch, |d| {
        for l in d.to_lowercase() {
            if !is_mark(l) {
                out.push(l);
            }
        }
    });
}

/// The folded characters and, per folded character, the byte span of the
/// scalar it came from.
fn fold(text: &str) -> (Vec<char>, Vec<Range<usize>>) {
    let mut chars = Vec::with_capacity(text.len());
    let mut spans = Vec::with_capacity(text.len());
    let mut buf = Vec::new();
    for (at, ch) in text.char_indices() {
        buf.clear();
        fold_scalar(ch, &mut buf);
        for &c in &buf {
            chars.push(c);
            spans.push(at..at + ch.len_utf8());
        }
    }
    (chars, spans)
}

fn fold_query(query: &str) -> Vec<char> {
    let mut out = fold(query).0;
    out.retain(|c| *c != HIGHLIGHT_BARRIER);
    out
}

/// Every match of `query` in `text`, left to right, never overlapping
/// (byte ranges).
pub fn highlight_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let q = fold_query(query);
    if q.is_empty() || text.is_empty() {
        return Vec::new();
    }
    let (chars, spans) = fold(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i + q.len() <= chars.len() {
        if chars[i..i + q.len()] != q[..] {
            i += 1;
            continue;
        }
        let mut end = spans[i + q.len() - 1].end;
        // The combining marks right after the match (folded to nothing) join it.
        while let Some(c) = text[end..].chars().next() {
            if !is_mark(c) {
                break;
            }
            end += c.len_utf8();
        }
        out.push(spans[i].start..end);
        i += q.len();
    }
    out
}

/// How many matches `query` has in `text`.
pub fn highlight_count(text: &str, query: &str) -> usize {
    highlight_ranges(text, query).len()
}

/// The prop as a spec: an object with a string `query` that folds to
/// something (else `None`).
pub fn highlight_spec(value: &Value) -> Option<HighlightSpec> {
    let v = value.as_object()?;
    let query = v.get("query")?.as_str()?;
    if fold_query(query).is_empty() {
        return None;
    }
    let current = v.get("current").and_then(Value::as_f64).filter(|n| n.is_finite()).map(|n| n.floor() as i64);
    let before = v.get("before").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    Some(HighlightSpec { query: query.to_string(), current, before })
}

/// The index among THIS node's matches painted current (`None` = none):
/// `current` minus the matches of `before`.
pub fn highlight_local_current(spec: &HighlightSpec) -> Option<usize> {
    let current = spec.current.filter(|c| *c >= 0)?;
    let local = current - spec.before.as_deref().map_or(0, |b| highlight_count(b, &spec.query)) as i64;
    usize::try_from(local).ok()
}
