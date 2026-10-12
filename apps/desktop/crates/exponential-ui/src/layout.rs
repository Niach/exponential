//! Round 2 (docs/round-2-contract.md §7): the shared layout NUMBERS
//! (`catalog/layout.json`, generated as `LAYOUT_CONSTANT_NAMES`/`VALUES`):
//! one place for the core and every painter.

use crate::generated::catalog as g;

/// A layout constant by name (`windowThreshold`, `mediaAspectRatio`, …).
pub fn layout_constant(name: &str) -> Option<f64> {
    let i = g::LAYOUT_CONSTANT_NAMES.iter().position(|n| *n == name)?;
    g::LAYOUT_CONSTANT_VALUES.get(i)?.parse().ok()
}

/// Items past which List and Table window.
pub const WINDOW_THRESHOLD: f64 = 50.0;
/// Items rendered beyond each viewport edge.
pub const WINDOW_OVERSCAN: f64 = 5.0;
/// Percentage points per arrow key on a Resizable handle.
pub const RESIZE_STEP: f64 = 10.0;
/// A panel's min (percent) when unset.
pub const PANEL_MIN: f64 = 10.0;
/// A Resizable handle's hit area across its hairline (px).
pub const RESIZE_HANDLE_HIT: f64 = 8.0;
/// The max-content width of a text field (px).
pub const FIELD_INTRINSIC_WIDTH: f64 = 160.0;
/// The max-content width of an Image/Video without a width (px).
pub const MEDIA_INTRINSIC_WIDTH: f64 = 320.0;
/// Width / height of an Image without `aspectRatio` or a height; Video's default.
pub const MEDIA_ASPECT_RATIO: f64 = 1.7777778;
/// One TreeGuides gutter column (px).
pub const TREE_GUIDE_COLUMN: f64 = 14.0;
/// The radius of a TreeGuides elbow's corner (px).
pub const TREE_GUIDE_RADIUS: f64 = 3.0;
/// How far a TreeGuides vertical overshoots its row's TOP to bridge a
/// Section divider (px; paint only, never layout).
pub const TREE_GUIDE_BRIDGE: f64 = 1.0;
/// EXP-1259: how long a `loading` Button waits before it shows its pending
/// state (ms).
pub const PENDING_DELAY_MS: f64 = 150.0;
/// The Skeleton's ONE opacity pulse (ms, low → high → low, ease-in-out).
pub const SKELETON_PULSE_MS: f64 = 1600.0;
pub const SKELETON_OPACITY_LOW: f64 = 0.45;
pub const SKELETON_OPACITY_HIGH: f64 = 0.9;
/// The Skeleton's static opacity under reduce motion.
pub const SKELETON_REDUCED_OPACITY: f64 = 0.6;
/// The bottom fade over a SearchList's rows while more sit below (px).
pub const SEARCH_LIST_FADE: f32 = 24.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_are_layout_json() {
        for (name, v) in [("windowThreshold", WINDOW_THRESHOLD), ("windowOverscan", WINDOW_OVERSCAN), ("resizeStep", RESIZE_STEP), ("panelMin", PANEL_MIN), ("resizeHandleHit", RESIZE_HANDLE_HIT), ("fieldIntrinsicWidth", FIELD_INTRINSIC_WIDTH), ("mediaIntrinsicWidth", MEDIA_INTRINSIC_WIDTH), ("mediaAspectRatio", MEDIA_ASPECT_RATIO), ("treeGuideColumn", TREE_GUIDE_COLUMN), ("treeGuideRadius", TREE_GUIDE_RADIUS), ("treeGuideBridge", TREE_GUIDE_BRIDGE), ("pendingDelayMs", PENDING_DELAY_MS), ("skeletonPulseMs", SKELETON_PULSE_MS), ("skeletonOpacityLow", SKELETON_OPACITY_LOW), ("skeletonOpacityHigh", SKELETON_OPACITY_HIGH), ("skeletonReducedOpacity", SKELETON_REDUCED_OPACITY), ("searchListFade", SEARCH_LIST_FADE as f64)] {
            assert_eq!(layout_constant(name), Some(v), "{name}");
        }
        assert_eq!(g::LAYOUT_CONSTANT_NAMES.len(), 17);
        let json: serde_json::Value = serde_json::from_str(g::LAYOUT_JSON).unwrap();
        assert_eq!(json["windowThreshold"], 50);
    }
}
