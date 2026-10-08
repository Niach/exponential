//! The WINDOWED `List`: the core lays out only the visible range plus
//! overscan, reports the estimated content height, and memoizes row heights
//! per item key so a scroll step is constant work (`fixtures`: 10,000 rows).

use std::collections::HashMap;
use std::sync::Arc;

/// Window parameters the React renderer shares (`WindowedList`).
pub const DEFAULT_ESTIMATED_ITEM_HEIGHT: f32 = 40.0;
pub const DEFAULT_OVERSCAN: usize = 5;
/// Lists shorter than this are laid out whole (the React renderer windows past 24).
pub const WINDOW_THRESHOLD: usize = 24;

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

    /// The estimate for an unmeasured row: the average measured height,
    /// else `estimated_item_height`.
    pub fn estimate(&self) -> f32 {
        if self.heights.is_empty() {
            self.estimated_item_height
        } else {
            (self.measured_sum / self.heights.len() as f64) as f32
        }
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
            if c.keys == id && c.len == keys.len() && c.version == self.version && c.gap == self.gap {
                return c.offsets.clone();
            }
        }
        let offsets = Arc::new(self.offsets(keys.iter().map(String::as_str)));
        self.cache = Some(OffsetsCache { keys: id, len: keys.len(), version: self.version, gap: self.gap, offsets: offsets.clone() });
        offsets
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

    /// The rows to lay out for a viewport of `viewport_height` at the current
    /// scroll offset, overscan on both ends. `offsets` = [`Self::offsets`].
    pub fn visible_range(&self, offsets: &[f32], viewport_height: f32) -> VisibleRange {
        let count = offsets.len().saturating_sub(1);
        if count == 0 {
            return VisibleRange::default();
        }
        let top = self.scroll_offset.max(0.0);
        let bottom = top + viewport_height.max(0.0);
        // Binary search on the monotonic offsets: O(log n) per step.
        let first = offsets[..count].partition_point(|&o| o < top).saturating_sub(1);
        let mut first = first;
        // partition_point returns the first index with offset >= top; the row
        // BEFORE it still spans `top` unless it ends before it.
        if first + 1 < count && offsets[first + 1] <= top {
            first += 1;
        }
        let last = offsets[..count].partition_point(|&o| o < bottom);
        let start = first.saturating_sub(self.overscan);
        let end = (last + self.overscan).min(count);
        VisibleRange { start, end: end.max(start) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_is_constant_per_scroll_step() {
        let keys: Vec<String> = (0..10_000).map(|i| format!("k{i}")).collect();
        let mut w = ListWindow::new(40.0, 5, 0.0);
        w.record("k3", 80.0);
        let offsets = w.offsets(keys.iter().map(String::as_str));
        assert_eq!(offsets.len(), 10_001);
        assert_eq!(offsets[10_000], 80.0 * 10_000.0, "unmeasured rows take the measured average");
        w.scroll_offset = 0.0;
        let r = w.visible_range(&offsets, 800.0);
        assert_eq!(r.start, 0);
        assert!(r.end <= 20 && r.end >= 10, "{r:?}");
        w.scroll_offset = 200_000.0;
        let r = w.visible_range(&offsets, 800.0);
        assert!(r.end - r.start <= 2 * 5 + 22);
        assert!(offsets[r.start] <= 200_000.0);
        assert!(offsets[r.end.min(9_999)] >= 200_000.0 + 800.0 - 40.0 * 6.0 || r.end == 10_000);
    }
}
