// Round 2 (docs/round-2-contract.md §7): the shared layout numbers
// (catalog/layout.json), one place for the TS reference and the generated
// constants of every target.

import layoutJson from "../catalog/layout.json" with { type: "json" }

export const LAYOUT_CONSTANTS: Readonly<Record<string, number>> = Object.fromEntries(Object.entries(layoutJson).filter(([k]) => !k.startsWith(`$`))) as Record<string, number>
export const WINDOW_THRESHOLD: number = layoutJson.windowThreshold
export const WINDOW_OVERSCAN: number = layoutJson.windowOverscan
export const RESIZE_STEP: number = layoutJson.resizeStep
export const PANEL_MIN: number = layoutJson.panelMin
export const RESIZE_HANDLE_HIT: number = layoutJson.resizeHandleHit
export const FIELD_INTRINSIC_WIDTH: number = layoutJson.fieldIntrinsicWidth
export const MEDIA_INTRINSIC_WIDTH: number = layoutJson.mediaIntrinsicWidth
export const MEDIA_ASPECT_RATIO: number = layoutJson.mediaAspectRatio
export const TREE_GUIDE_COLUMN: number = layoutJson.treeGuideColumn
/** Round 3: the elbow's corner radius and how far the guide verticals overshoot a row's top edge (paint only). */
export const TREE_GUIDE_RADIUS: number = layoutJson.treeGuideRadius
export const TREE_GUIDE_BRIDGE: number = layoutJson.treeGuideBridge
