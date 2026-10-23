//! N-12: THE stack geometry ×4 (`packages/domain-contract/src/stack.ts`'s
//! twin): the toast's (`fixtures/toast-stack.json` `geometry`) shared with
//! THE Dock's pile (`fixtures/dock.json` `stack`). The newest item in front,
//! the older ones peeking out behind it by `peek` and shrinking by
//! `scale_step` per rank, `visible` of them at most; expanded = every item at
//! full size, `gap` apart. Pure.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackConstants {
    /// How far an older item peeks out behind the one in front of it.
    pub peek: f32,
    /// How much smaller each rank behind the front draws.
    pub scale_step: f32,
    /// How many items draw at all while collapsed.
    pub visible: usize,
    /// The space between items while expanded.
    pub gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackItem {
    /// The item's top inside the stack box.
    pub offset: f32,
    /// Its width factor.
    pub scale: f32,
    /// Whether it draws at all.
    pub visible: bool,
}

/// `heights` oldest first (the last is the front); returns the stack box's
/// height and one item per height in the same order. `anchored_bottom` =
/// the box grows upward from its bottom edge (a bottom stack).
pub fn stack_layout(
    heights: &[f32],
    expanded: bool,
    anchored_bottom: bool,
    constants: StackConstants,
) -> (f32, Vec<StackItem>) {
    let n = heights.len();
    let rank = |i: usize| n - 1 - i;
    // Distance of each item's anchor-side edge from the box's anchor edge.
    let lead: Vec<f32> = if expanded {
        (0..n).map(|i| heights[i + 1..].iter().map(|h| h + constants.gap).sum::<f32>()).collect()
    } else {
        (0..n).map(|i| constants.peek * rank(i) as f32).collect()
    };
    let height = (0..n).map(|i| lead[i] + heights[i]).fold(0f32, f32::max);
    let items = (0..n)
        .map(|i| {
            let r = rank(i);
            let offset = if anchored_bottom { height - lead[i] - heights[i] } else { lead[i] };
            let (scale, visible) = if expanded {
                (1.0, true)
            } else {
                (1.0 - constants.scale_step * r.min(constants.visible - 1) as f32, r < constants.visible)
            };
            StackItem { offset, scale, visible }
        })
        .collect();
    (height, items)
}
