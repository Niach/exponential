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
/// EXP-1548: the narrowest a Table column without a `width` gets (px); a
/// table whose columns need more than its box scrolls horizontally.
pub const TABLE_COLUMN_MIN: f32 = 96.0;
/// VAPP-279: the widest a `strip` Tabs item gets (px; its title truncates).
pub const TAB_STRIP_MAX_WIDTH: f32 = 240.0;
/// VAPP-279: how far a pointer travels on a strip tab before the press
/// becomes a reorder drag (px).
pub const TAB_DRAG_THRESHOLD: f32 = 4.0;
/// VAPP-279: how long a touch rests on a strip tab before it drags (ms).
pub const TAB_LONG_PRESS_MS: f64 = 500.0;
/// #EXP-1629: a `row` Select's popup width when `popupWidth` is unset (px).
pub const SELECT_ROW_POPUP_WIDTH: f32 = 262.0;
/// #EXP-1629: how far under its value a `row` Select's popup hangs (px):
/// the IDE's picker popover opens a point under the trigger's value.
pub const SELECT_ROW_POPUP_OFFSET: f32 = 1.0;
/// #EXP-1629: how close to the view's edges a `row` Select's popup may go
/// (px): gpui-component's popovers keep 8 from the window around their
/// 260 content, the border outside it.
pub const SELECT_ROW_POPUP_MARGIN: f32 = 7.0;
/// #EXP-1629: the widest a `row` Select's value gets before it truncates.
pub const SELECT_ROW_VALUE_MAX_WIDTH: f32 = 240.0;
/// #EXP-1629: the widest a `row` Select's description gets before it wraps.
pub const SELECT_ROW_DESCRIPTION_MAX_WIDTH: f32 = 460.0;
/// #EXP-1765: the Skeleton rows a `loading` Select's popup shows.
pub const SELECT_LOADING_ROWS: usize = 3;
/// #EXP-1765: how far under a `pill` Select its popup's border box hangs (px).
pub const SELECT_PILL_POPUP_OFFSET: f32 = 3.0;
/// #EXP-1765: how far before the pill's aligned edge its popup's border box
/// starts (px): the IDE popover's ring sits outside its content.
pub const SELECT_PILL_POPUP_INSET: f32 = 1.0;
/// #EXP-1629: how far a Menu item's `preview` sits off its row (px).
pub const MENU_PREVIEW_OFFSET: f32 = 10.0;
/// #EXP-1629: how long the pointer rests on a row before its preview shows,
/// and how long a preview stays after the pointer and the highlight left (ms).
pub const MENU_PREVIEW_OPEN_MS: f64 = 150.0;
pub const MENU_PREVIEW_CLOSE_MS: f64 = 80.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_are_layout_json() {
        for (name, v) in [("windowThreshold", WINDOW_THRESHOLD), ("windowOverscan", WINDOW_OVERSCAN), ("resizeStep", RESIZE_STEP), ("panelMin", PANEL_MIN), ("resizeHandleHit", RESIZE_HANDLE_HIT), ("fieldIntrinsicWidth", FIELD_INTRINSIC_WIDTH), ("mediaIntrinsicWidth", MEDIA_INTRINSIC_WIDTH), ("mediaAspectRatio", MEDIA_ASPECT_RATIO), ("treeGuideColumn", TREE_GUIDE_COLUMN), ("treeGuideRadius", TREE_GUIDE_RADIUS), ("treeGuideBridge", TREE_GUIDE_BRIDGE), ("pendingDelayMs", PENDING_DELAY_MS), ("skeletonPulseMs", SKELETON_PULSE_MS), ("skeletonOpacityLow", SKELETON_OPACITY_LOW), ("skeletonOpacityHigh", SKELETON_OPACITY_HIGH), ("skeletonReducedOpacity", SKELETON_REDUCED_OPACITY), ("searchListFade", SEARCH_LIST_FADE as f64), ("tableColumnMin", TABLE_COLUMN_MIN as f64), ("tabStripMaxWidth", TAB_STRIP_MAX_WIDTH as f64), ("tabDragThreshold", TAB_DRAG_THRESHOLD as f64), ("tabLongPressMs", TAB_LONG_PRESS_MS), ("selectRowPopupWidth", SELECT_ROW_POPUP_WIDTH as f64), ("selectRowPopupOffset", SELECT_ROW_POPUP_OFFSET as f64), ("selectRowValueMaxWidth", SELECT_ROW_VALUE_MAX_WIDTH as f64), ("selectRowPopupMargin", SELECT_ROW_POPUP_MARGIN as f64), ("selectRowDescriptionMaxWidth", SELECT_ROW_DESCRIPTION_MAX_WIDTH as f64), ("selectLoadingRows", SELECT_LOADING_ROWS as f64), ("selectPillPopupOffset", SELECT_PILL_POPUP_OFFSET as f64), ("selectPillPopupInset", SELECT_PILL_POPUP_INSET as f64), ("menuPreviewOffset", MENU_PREVIEW_OFFSET as f64), ("menuPreviewOpenMs", MENU_PREVIEW_OPEN_MS), ("menuPreviewCloseMs", MENU_PREVIEW_CLOSE_MS)] {
            assert_eq!(layout_constant(name), Some(v), "{name}");
        }
        assert_eq!(g::LAYOUT_CONSTANT_NAMES.len(), 32);
        let json: serde_json::Value = serde_json::from_str(g::LAYOUT_JSON).unwrap();
        assert_eq!(json["windowThreshold"], 50);
    }
}
