//! The WINDOWED `List`: the core lays out only the visible range plus
//! overscan, reports the content extent, and memoizes item extents per key
//! so a scroll step is constant work.
//!
//! Round 2 (docs/round-2-contract.md §4–5, `src/list.ts`): the LIST numbers
//! every renderer shares — template item keys and instance suffixes
//! (duplicates and empty keys → `#<index>`, suffixes ACCUMULATE through
//! nested templates), Table row keys, the one-axis virtual window
//! (vertical OR horizontal), `scrollToIndex` offsets, sections and the
//! sticky header. `fixtures/template-items.json` and
//! `fixtures/virtual-list.json` lock them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data::{absolute_path, get_pointer};

/// The extent an unmeasured item takes when no theme says `$control.row`.
pub const DEFAULT_ESTIMATED_ITEM_HEIGHT: f32 = 40.0;
/// Items rendered beyond each viewport edge (`layout.json windowOverscan`).
pub const DEFAULT_OVERSCAN: usize = crate::layout::WINDOW_OVERSCAN as usize;
/// Lists and Tables with more items than this window (`layout.json
/// windowThreshold`).
pub const WINDOW_THRESHOLD: usize = crate::layout::WINDOW_THRESHOLD as usize;

// ---------------------------------------------------------------------------
// Keys and instances (round 2, §4)
// ---------------------------------------------------------------------------

/// `JSON.stringify` of a value (numbers the JS way).
fn js_json(v: &Value) -> String {
    serde_json::to_string(&crate::json::canonical(v)).unwrap_or_default()
}

/// A key value as a string: scalars like JS `String(v)`, objects and arrays
/// as their JSON; `None` for missing, null and ``.
fn key_string(v: Option<&Value>) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(v @ (Value::Object(_) | Value::Array(_))) => Some(js_json(v)),
        Some(v) => Some(crate::json::to_js_string(v)),
    }
}

/// A key as one instance-suffix segment: `~` → `~0`, `.` → `~1` (as JSON
/// pointer escapes `/`), so dotted keys never collide across nesting levels
/// (`a.b` + `c` vs `a` + `b.c`). The reference's `instanceSegment`.
pub fn instance_segment(key: &str) -> String {
    key.replace('~', "~0").replace('.', "~1")
}

/// The instance key of every item: the value at `key_pointer` (relative to
/// the item; objects as their JSON) — `#<index>` when that value is
/// missing, null, `` or a DUPLICATE of an earlier item's key (more `#`s
/// until unique); without a pointer, the index. `templateItemKeys`.
pub fn template_item_keys(items: &[Value], key_pointer: Option<&str>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::with_capacity(items.len());
    let pointer = key_pointer.map(|k| if k.starts_with('/') { k.to_string() } else { format!("/{k}") });
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut key = index.to_string();
            if let Some(p) = &pointer {
                key = match key_string(get_pointer(item, p)) {
                    Some(own) if !seen.contains(&own) => own,
                    _ => format!("#{index}"),
                };
                while seen.contains(&key) {
                    key = format!("#{key}");
                }
            }
            seen.insert(key.clone());
            key
        })
        .collect()
}

/// Table row keys: the `row_key` FIELD (default `id`) under the same rules.
pub fn table_row_keys(rows: &[Value], row_key: &str) -> Vec<String> {
    let token = row_key.replace('~', "~0").replace('/', "~1");
    template_item_keys(rows, Some(&format!("/{token}")))
}

/// One item a template renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInstance {
    /// The item's key ([`template_item_keys`]).
    pub key: String,
    /// The item's data pointer (its scope).
    pub path: String,
    pub index: usize,
    /// The suffix every node of the item wears: the enclosing instance's
    /// suffix + `.<key>` ([`instance_segment`]: `.a~1b` for key `a.b`). A
    /// node's painted id = its id + the suffix.
    pub instance: String,
}

/// The items a `template` renders in a data scope: one per array item at
/// `template.path` (relative to `scope`), keyed by `template.key`;
/// `instance` = the suffix of the item the template sits in (`` at the top).
/// The key is escaped by [`instance_segment`].
pub fn template_instances(data: &Value, template: &crate::types::Template, scope: &str, instance: &str) -> Vec<TemplateInstance> {
    let path = absolute_path(&template.path, scope);
    let Some(list) = get_pointer(data, &path).and_then(Value::as_array) else { return Vec::new() };
    let keys = template_item_keys(list, template.key.as_deref());
    keys.into_iter().enumerate().map(|(index, key)| TemplateInstance { path: format!("{path}/{index}"), index, instance: format!("{instance}.{}", instance_segment(&key)), key }).collect()
}

// ---------------------------------------------------------------------------
// The virtual window (round 2, §5; one axis)
// ---------------------------------------------------------------------------

/// Every item's extent on the list's axis: a measured item (`Some`) keeps
/// its extent; an unmeasured one takes the MEAN measured extent once any
/// item is measured (the content length stays stable while rows measure in),
/// else `row` (the theme's `$control.row`). [`ListWindow`] applies the same
/// rule. Mirrors `itemExtents` (round 2 §5, `virtual-list.json`).
pub fn item_extents(measured: &[Option<f64>], row: f64) -> Vec<f64> {
    let (sum, n) = measured.iter().flatten().fold((0.0, 0usize), |(s, n), m| (s + m, n + 1));
    let estimate = if n > 0 { sum / n as f64 } else { row };
    measured.iter().map(|m| m.unwrap_or(estimate)).collect()
}

/// Start offsets of items laid end to end with `gap` between them (a
/// `divided` list adds `$control.hairline` to the gap): `offsets[i]` =
/// where item i starts, `offsets[count]` = the content length.
pub fn item_offsets(extents: &[f64], gap: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(extents.len() + 1);
    let mut at = 0.0;
    for (i, e) in extents.iter().enumerate() {
        out.push(at);
        at += e + if i + 1 < extents.len() { gap } else { 0.0 };
    }
    out.push(at);
    out
}

/// The window over a list: items `start..end` render, `before`/`after` =
/// the spacers, `total` = the content length.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowRange {
    pub start: usize,
    pub end: usize,
    pub before: f64,
    pub after: f64,
    pub total: f64,
}

/// The items to render for a viewport `[scroll, scroll + viewport)` on the
/// list's axis (relative to its content start): every item intersecting it
/// (an item ending exactly at `scroll` counts) plus `overscan` on each
/// side. `virtualWindow`.
pub fn virtual_window(extents: &[f64], gap: f64, scroll: f64, viewport: f64, overscan: usize) -> WindowRange {
    let count = extents.len();
    let off = item_offsets(extents, gap);
    let total = off[count];
    // Offsets are monotonic: binary searches keep a 100,000-row window O(log n).
    let (mut lo, mut hi) = (0, count);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if off[mid] + extents[mid] < scroll {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let start = lo;
    let end = start + off[start..count].partition_point(|&o| o < scroll + viewport);
    let start = start.saturating_sub(overscan);
    let end = (end + overscan).min(count);
    if end <= start {
        return WindowRange { start, end: start, before: 0.0, after: total, total };
    }
    WindowRange { start, end, before: off[start], after: total - (off[end - 1] + extents[end - 1]), total }
}

/// How `scrollToIndex` aligns the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAlign {
    Start,
    Center,
    End,
    #[default]
    Nearest,
}

impl ScrollAlign {
    pub fn parse(s: &str) -> Option<ScrollAlign> {
        Some(match s {
            "start" => ScrollAlign::Start,
            "center" => ScrollAlign::Center,
            "end" => ScrollAlign::End,
            "nearest" => ScrollAlign::Nearest,
            _ => return None,
        })
    }
}

/// The scroll offset that brings item `index` into a viewport at `scroll`
/// (`scrollToIndex`): `start` = its start at the viewport start after
/// `inset` (the pinned header), `end`, `center`, `nearest` = the smallest
/// move (none when visible). Clamped to `[0, max(0, total - viewport)]`.
#[allow(clippy::too_many_arguments)]
pub fn scroll_offset_for_index(extents: &[f64], gap: f64, index: usize, viewport: f64, scroll: f64, align: ScrollAlign, inset: f64) -> f64 {
    if index >= extents.len() {
        return scroll;
    }
    let off = item_offsets(extents, gap);
    scroll_offset_at(off[index], extents[index], off[extents.len()], viewport, scroll, align, inset)
}

/// [`scroll_offset_for_index`] for an item at `top` with extent `size` in
/// content of length `total` (O(1) over cached offsets).
pub fn scroll_offset_at(top: f64, size: f64, total: f64, viewport: f64, scroll: f64, align: ScrollAlign, inset: f64) -> f64 {
    let next = match align {
        ScrollAlign::Start => top - inset,
        ScrollAlign::End => top + size - viewport,
        ScrollAlign::Center => top + size / 2.0 - (viewport + inset) / 2.0,
        ScrollAlign::Nearest if top - inset < scroll => top - inset,
        ScrollAlign::Nearest if top + size > scroll + viewport => top + size - viewport,
        ScrollAlign::Nearest => scroll,
    };
    next.min((total - viewport).max(0.0)).max(0.0)
}

// ---------------------------------------------------------------------------
// Sections and sticky headers
// ---------------------------------------------------------------------------

/// Consecutive items sharing a `sectionBy` value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSection {
    /// The shared value as a string (`` for missing).
    pub value: String,
    /// Index of the section's first item in the data.
    pub start: usize,
    pub count: usize,
}

/// Consecutive items with the same `section_by` value (a pointer relative
/// to each item) form a section; the data order is kept.
pub fn list_sections(items: &[Value], section_by: &str) -> Vec<ListSection> {
    let pointer = if section_by.starts_with('/') { section_by.to_string() } else { format!("/{section_by}") };
    let mut out: Vec<ListSection> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let value = match get_pointer(item, &pointer) {
            None | Some(Value::Null) => String::new(),
            Some(v @ (Value::Object(_) | Value::Array(_))) => js_json(v),
            Some(v) => crate::json::to_js_string(v),
        };
        match out.last_mut() {
            Some(last) if last.value == value => last.count += 1,
            _ => out.push(ListSection { value, start: index, count: 1 }),
        }
    }
    out
}

/// A flat row of a sectioned list: a section header or a data item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionRow {
    Header(usize),
    Item(usize),
}

/// The flat ROW list a sectioned list windows: a header before each
/// section's items.
pub fn section_rows(sections: &[ListSection]) -> Vec<SectionRow> {
    let mut rows = Vec::with_capacity(sections.iter().map(|s| s.count + 1).sum());
    for (i, s) in sections.iter().enumerate() {
        rows.push(SectionRow::Header(i));
        rows.extend((0..s.count).map(|k| SectionRow::Item(s.start + k)));
    }
    rows
}

/// `scrollToIndex` on a SECTIONED list: the DATA index → its flat row
/// ([`section_rows`]; absent → `scroll`, no move), then
/// [`scroll_offset_for_index`] over the rows' extents with `inset` = the
/// extent of the item's own section header when headers are sticky (it
/// pins over the item), else 0. The reference's `scrollOffsetForItem`.
#[allow(clippy::too_many_arguments)]
pub fn scroll_offset_for_item(rows: &[SectionRow], row_extents: &[f64], gap: f64, data_index: usize, viewport: f64, scroll: f64, align: ScrollAlign, sticky_headers: bool) -> f64 {
    let Some(row) = rows.iter().position(|r| *r == SectionRow::Item(data_index)) else { return scroll };
    let header = rows[..row].iter().rposition(|r| matches!(r, SectionRow::Header(_)));
    let inset = match header {
        Some(h) if sticky_headers => row_extents.get(h).copied().unwrap_or(0.0),
        _ => 0.0,
    };
    scroll_offset_for_index(row_extents, gap, row, viewport, scroll, align, inset)
}

/// The pinned header at a scroll offset: `row` (a flat row index) drawn at
/// `offset` on the list's axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StickyHeader {
    pub row: usize,
    pub offset: f64,
}

/// The LAST header row starting at or before `scroll` pins at the viewport
/// start, pushed back by the next header (`min(scroll, nextStart −
/// extent)`). `row_offsets` = [`item_offsets`] of the flat rows,
/// `header_rows` ascending. `None` before the first header.
pub fn sticky_header(row_offsets: &[f64], row_extents: &[f64], header_rows: &[usize], scroll: f64) -> Option<StickyHeader> {
    sticky_header_by(|r| row_offsets[r], |r| row_extents[r], header_rows, scroll)
}

/// [`sticky_header`] over accessors: only the header rows are read
/// (binary search; O(log headers)).
pub fn sticky_header_by(offset: impl Fn(usize) -> f64, extent: impl Fn(usize) -> f64, header_rows: &[usize], scroll: f64) -> Option<StickyHeader> {
    let pinned = header_rows.partition_point(|&h| offset(h) <= scroll);
    let pinned = pinned.checked_sub(1)?;
    let row = header_rows[pinned];
    let at = match header_rows.get(pinned + 1) {
        Some(&next) => scroll.min(offset(next) - extent(row)),
        None => scroll,
    };
    Some(StickyHeader { row, offset: at })
}

#[derive(Debug, Clone)]
pub struct ListWindow {
    /// Measured (laid out) heights per item key.
    heights: HashMap<String, f32>,
    /// The sum of the measured heights: unmeasured rows are estimated at the
    /// AVERAGE measured height (the estimate only until the first pass).
    measured_sum: f64,
    pub estimated_item_height: f32,
    pub overscan: usize,
    pub gap: f32,
    pub scroll_offset: f32,
    /// Bumped whenever an offset may move (a height recorded, the gap).
    version: u64,
    /// The offsets last computed.
    cache: Option<OffsetsCache>,
}

/// Offsets memoized for one keys list (by identity) at one version + gap.
#[derive(Debug, Clone)]
struct OffsetsCache {
    keys: usize,
    len: usize,
    version: u64,
    gap: f32,
    estimate: f32,
    offsets: Arc<Vec<f32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisibleRange {
    /// First rendered index (overscan included).
    pub start: usize,
    /// One past the last rendered index.
    pub end: usize,
}

impl ListWindow {
    pub fn new(estimated_item_height: f32, overscan: usize, gap: f32) -> ListWindow {
        ListWindow { heights: HashMap::new(), measured_sum: 0.0, estimated_item_height, overscan, gap, scroll_offset: 0.0, version: 0, cache: None }
    }

    /// Remember one row's laid-out height. Returns true when it changed.
    pub fn record(&mut self, key: &str, height: f32) -> bool {
        match self.heights.get(key) {
            Some(h) if (h - height).abs() < 0.001 => false,
            old => {
                self.measured_sum += (height - old.copied().unwrap_or(0.0)) as f64;
                self.heights.insert(key.to_string(), height);
                self.version += 1;
                true
            }
        }
    }

    /// The extent of an unmeasured item ([`item_extents`]): the mean
    /// measured extent once any item was measured (keeps the scroll extent
    /// stable, so a fling lands on the real end in one pass), else
    /// `estimated_item_height` (the theme's `$control.row`, round 2 §5).
    pub fn estimate(&self) -> f32 {
        self.measured_mean().unwrap_or(self.estimated_item_height)
    }

    /// The mean measured extent (`None` before the first pass).
    pub fn measured_mean(&self) -> Option<f32> {
        (!self.heights.is_empty()).then(|| (self.measured_sum / self.heights.len() as f64) as f32)
    }

    /// Every item's extent in key order (measured, else the estimate).
    pub fn extents<'a>(&self, keys: impl Iterator<Item = &'a str>) -> Vec<f64> {
        let estimate = self.estimate();
        keys.map(|k| self.heights.get(k).copied().unwrap_or(estimate) as f64).collect()
    }

    pub fn height_of(&self, key: &str) -> f32 {
        self.heights.get(key).copied().unwrap_or_else(|| self.estimate())
    }

    pub fn forget(&mut self, key: &str) {
        if let Some(h) = self.heights.remove(key) {
            self.measured_sum -= h as f64;
            self.version += 1;
        }
    }

    pub fn clear(&mut self) {
        self.heights.clear();
        self.measured_sum = 0.0;
        self.version += 1;
    }

    /// [`Self::offsets`] memoized per keys list (by identity) until a height
    /// or the gap changes: a scroll step reuses them (O(log n) per step).
    pub fn offsets_cached(&mut self, keys: &Arc<Vec<String>>) -> Arc<Vec<f32>> {
        let id = Arc::as_ptr(keys) as usize;
        if let Some(c) = &self.cache {
            if c.keys == id && c.len == keys.len() && c.version == self.version && c.gap == self.gap && c.estimate == self.estimate() {
                return c.offsets.clone();
            }
        }
        let offsets = Arc::new(self.offsets(keys.iter().map(String::as_str)));
        self.cache = Some(OffsetsCache { keys: id, len: keys.len(), version: self.version, gap: self.gap, estimate: self.estimate(), offsets: offsets.clone() });
        offsets
    }

    /// The pinned header ([`sticky_header`]) on the memoized offsets
    /// ([`Self::offsets_cached`]): a row's extent = the next offset − the
    /// gap, so nothing per row is recomputed. Header rows past the keys are
    /// ignored; `header_rows` ascending. Returns the pin and the row's own
    /// offset.
    pub fn sticky_header_cached(&mut self, keys: &Arc<Vec<String>>, header_rows: &[usize], scroll: f64) -> Option<(StickyHeader, f64)> {
        let offsets = self.offsets_cached(keys);
        let count = keys.len();
        let gap = self.gap as f64;
        let headers = &header_rows[..header_rows.partition_point(|&h| h < count)];
        let offset = |r: usize| offsets[r] as f64;
        let extent = |r: usize| offsets[r + 1] as f64 - offsets[r] as f64 - if r + 1 < count { gap } else { 0.0 };
        sticky_header_by(offset, extent, headers, scroll).map(|pin| (pin, offset(pin.row)))
    }

    /// Offsets of every row's top plus the total height at the end.
    /// Linear in the row count (one pass, no allocation per row beyond the
    /// output); the surface calls it once per layout pass.
    pub fn offsets<'a>(&self, keys: impl Iterator<Item = &'a str>) -> Vec<f32> {
        let mut out = Vec::new();
        let mut y = 0.0f32;
        let mut first = true;
        let estimate = self.estimate();
        for key in keys {
            if !first {
                y += self.gap;
            }
            first = false;
            out.push(y);
            y += self.heights.get(key).copied().unwrap_or(estimate);
        }
        out.push(y);
        out
    }

    /// Total content height for `count` rows (estimated where unmeasured).
    pub fn content_height<'a>(&self, keys: impl Iterator<Item = &'a str>) -> f32 {
        self.offsets(keys).last().copied().unwrap_or(0.0)
    }

    /// The items to lay out for a viewport of `viewport` (on the list's
    /// axis) at the current scroll offset, overscan on both ends: every item
    /// intersecting `[scroll, scroll + viewport)` (an item ending exactly at
    /// `scroll` counts), [`virtual_window`] on memoized offsets, O(log n).
    /// `offsets` = [`Self::offsets`].
    pub fn visible_range(&self, offsets: &[f32], viewport: f32) -> VisibleRange {
        let count = offsets.len().saturating_sub(1);
        if count == 0 {
            return VisibleRange::default();
        }
        let top = self.scroll_offset.max(0.0);
        let bottom = top + viewport.max(0.0);
        // Item i ends at offsets[i + 1] - gap (the last at offsets[count]).
        let end_of = |i: usize| if i + 1 < count { offsets[i + 1] - self.gap } else { offsets[count] };
        let (mut lo, mut hi) = (0, count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if end_of(mid) < top {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let first = lo;
        let last = first + offsets[first..count].partition_point(|&o| o < bottom);
        let start = first.saturating_sub(self.overscan);
        let end = (last + self.overscan).min(count);
        VisibleRange { start: start.min(end), end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasured_items_take_row_then_the_measured_mean() {
        assert_eq!(item_extents(&[None, None], 36.0), vec![36.0, 36.0]);
        assert_eq!(item_extents(&[Some(40.0), None, Some(60.0)], 36.0), vec![40.0, 50.0, 60.0]);
        // The window applies the same rule.
        let keys = ["a", "b", "c"];
        let mut w = ListWindow::new(36.0, 5, 0.0);
        assert_eq!(w.extents(keys.iter().copied()), vec![36.0; 3]);
        w.record("a", 40.0);
        w.record("c", 60.0);
        assert_eq!(w.extents(keys.iter().copied()), item_extents(&[Some(40.0), None, Some(60.0)], 36.0));
    }

    #[test]
    fn window_is_constant_per_scroll_step() {
        let keys: Vec<String> = (0..10_000).map(|i| format!("k{i}")).collect();
        let mut w = ListWindow::new(40.0, 5, 0.0);
        w.record("k3", 80.0);
        let offsets = w.offsets(keys.iter().map(String::as_str));
        assert_eq!(offsets.len(), 10_001);
        assert_eq!(offsets[10_000], 80.0 * 10_000.0, "unmeasured rows take the measured mean");
        w.scroll_offset = 0.0;
        let r = w.visible_range(&offsets, 800.0);
        assert_eq!(r.start, 0);
        assert!(r.end <= 20 && r.end >= 10, "{r:?}");
        w.scroll_offset = 200_000.0;
        let r = w.visible_range(&offsets, 800.0);
        assert!(r.end - r.start <= 2 * 5 + 22);
        assert!(offsets[r.start] <= 200_000.0);
        assert!(offsets[r.end.min(9_999)] >= 200_000.0 + 800.0 - 40.0 * 6.0 || r.end == 10_000);
        // The same window as the shared `virtual_window` (round 2).
        let extents = w.extents(keys.iter().map(String::as_str));
        let shared = virtual_window(&extents, 0.0, 200_000.0, 800.0, 5);
        assert_eq!((r.start, r.end), (shared.start, shared.end));
    }
}
