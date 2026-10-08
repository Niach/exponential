package at.exponential.ui.catalog

/**
 * The catalog numbers and lists this painter reads, mirrored from
 * `packages/exponential-ui/generated/ExponentialUICatalog.generated.kt`
 * (`CatalogConstantsTest` locks them against it, so a catalog change fails
 * the build until they are copied here).
 */
object CatalogConstants {
    /** `catalog/layout.json` (round 2 §7). */
    const val WINDOW_THRESHOLD = 50
    const val WINDOW_OVERSCAN = 5
    const val RESIZE_STEP = 10f
    const val PANEL_MIN = 10f
    const val RESIZE_HANDLE_HIT = 8f
    const val FIELD_INTRINSIC_WIDTH = 160f
    const val MEDIA_INTRINSIC_WIDTH = 320f
    const val MEDIA_ASPECT_RATIO = 1.7777778f
    const val TREE_GUIDE_COLUMN = 16f

    /** `layout.json` by name (the order of `layoutConstantNames`). */
    val layout: Map<String, Float> = mapOf(
        "windowThreshold" to WINDOW_THRESHOLD.toFloat(),
        "windowOverscan" to WINDOW_OVERSCAN.toFloat(),
        "resizeStep" to RESIZE_STEP,
        "panelMin" to PANEL_MIN,
        "resizeHandleHit" to RESIZE_HANDLE_HIT,
        "fieldIntrinsicWidth" to FIELD_INTRINSIC_WIDTH,
        "mediaIntrinsicWidth" to MEDIA_INTRINSIC_WIDTH,
        "mediaAspectRatio" to MEDIA_ASPECT_RATIO,
        "treeGuideColumn" to TREE_GUIDE_COLUMN,
    )

    /** `locale.json` `rtlMirroredIcons`: the directional glyphs mirrored under rtl (round 1 §4). */
    val rtlMirroredIcons: List<String> = listOf(
        "ui-back", "ui-chevron-left", "ui-chevron-right", "ui-arrow-right", "ui-send", "ui-undo", "ui-external-link",
        "arrow-left", "arrow-right", "chevron-left", "chevron-right", "send", "undo-2", "external-link",
    )

    /** `style.json` `animations` names (round 2 §2). */
    val animationNames: List<String> = listOf("pulse", "spin", "fade-in", "slide-in-up", "slide-in-down", "slide-in-left", "slide-in-right", "shimmer")
}
