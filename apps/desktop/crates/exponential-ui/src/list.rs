//! The WINDOWED `List`: the core lays out only the visible range plus
//! overscan, reports the estimated content height, and memoizes row heights
//! per item key so a scroll step is constant work (`fixtures`: 10,000 rows).

use std::collections::HashMap;

/// Window parameters the React renderer shares (`WindowedList`).
pub const DEFAULT_ESTIMATED_ITEM_HEIGHT: f32 = 40.0;
pub const DEFAULT_OVERSCAN: usize = 5;
/// Lists shorter than this are laid out whole (the React renderer windows past 24).
pub const WINDOW_THRESHOLD: usize = 24;

#[derive(Debug, Clone)]
pub struct ListWindow {
    /// Measured (laid out) heights per item key.
    heights: HashMap<String, f32>,
    pub estimated_item_height: f32,
    pub overscan: usize,
    pub gap: f32,
    pub scroll_offset: f32,
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
        ListWindow { heights: HashMap::new(), estimated_item_height, overscan, gap, scroll_offset: 0.0 }
    }

    /// Remember one row's laid-out height. Returns true when it changed.
    pub fn record(&mut self, key: &str, height: f32) -> bool {
        match self.heights.get(key) {
            Some(h) if (h - height).abs() < 0.001 => false,
            _ => {
                self.heights.insert(key.to_string(), height);
                true
            }
        }
    }

    pub fn height_of(&self, key: &str) -> f32 {
        self.heights.get(key).copied().unwrap_or(self.estimated_item_height)
    }

    pub fn forget(&mut self, key: &str) {
        self.heights.remove(key);
    }

    pub fn clear(&mut self) {
        self.heights.clear();
    }

    /// Offsets of every row's top plus the total height at the end.
    /// Linear in the row count (one pass, no allocation per row beyond the
    /// output); the surface calls it once per layout pass.
    pub fn offsets<'a>(&self, keys: impl Iterator<Item = &'a str>) -> Vec<f32> {
        let mut out = Vec::new();
        let mut y = 0.0f32;
        let mut first = true;
        for key in keys {
            if !first {
                y += self.gap;
            }
            first = false;
            out.push(y);
            y += self.height_of(key);
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
        assert_eq!(offsets[10_000], 40.0 * 9_999.0 + 80.0);
        w.scroll_offset = 0.0;
        let r = w.visible_range(&offsets, 800.0);
        assert_eq!(r.start, 0);
        assert!(r.end <= 32 && r.end >= 20);
        w.scroll_offset = 200_000.0;
        let r = w.visible_range(&offsets, 800.0);
        assert!(r.end - r.start <= 2 * 5 + 22);
        assert!(offsets[r.start] <= 200_000.0);
        assert!(offsets[r.end.min(9_999)] >= 200_000.0 + 800.0 - 40.0 * 6.0 || r.end == 10_000);
    }
}
